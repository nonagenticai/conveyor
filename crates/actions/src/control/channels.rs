#![forbid(unsafe_code)]
//! Connector administration: kinds, bindings, the authorization matrix, and the Telegram
//! transport (S06 §5.2, S06d).
//!
//! The binding is per `(tenant, channel_kind, route_key)` and holds **non-secret** config plus
//! the **names** of sealed tenant secrets. `channels.bind` validates the config against the
//! connector's own schema and refuses a name that does not exist, so a binding is usable when it
//! is created rather than failing at the first message. `channels.kinds` exposes the same schema
//! to the web UI: onboarding a gateway that speaks the JSON envelope is a binding, not a deploy.
//!
//! `channels.policy_*` is the tenant-admin's grant matrix. It mirrors `roles.policy_*`: absent
//! row = compiled default, the stored set is exactly what the admin submitted. The machine floor
//! (`machine_forbidden`) is enforced here as a refusal to store, and again at invocation, so a
//! stored row can never widen a channel past it.
//!
//! **The Telegram send lives here, not in the worker.** It is an HTTP call with a deadline,
//! exactly like [`crate::board::webhooks::deliver_once`], and keeping it beside the channel
//! actions means the Bot API's wire shape has one owner.

use std::collections::BTreeMap;
use std::time::Duration;

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::channel::{ChannelBindingRecord, NewChannelBinding};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::channel::{command, connectors};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{ActionKind, HandlerOutput, Scope};

use super::invalid;
use super::views::{
    ChannelBindIn, ChannelBindingAdminOut, ChannelBindingOut, ChannelGetIn, ChannelIdentityOut,
    ChannelKindOut, ChannelKindsOut, ChannelListIn, ChannelListOut, ChannelPolicyAction,
    ChannelPolicyBinding, ChannelPolicyChannel, ChannelPolicyEntryIn, ChannelPolicyGroup,
    ChannelPolicyResetIn, ChannelPolicySetIn, ChannelPolicyView, ChannelTestIn, ChannelTestOut,
    ChannelUnbindIn,
};

/// The Bot API base the transport posts to unless a binding config overrides it.
pub const DEFAULT_TELEGRAM_API_BASE: &str = connectors::telegram::DEFAULT_API_BASE;

/// How long one `sendMessage` may take before the attempt is a failure.
pub const TELEGRAM_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// The longest Telegram `description` echoed into `delivery.last_error`.
pub const TELEGRAM_ERROR_CHARS: usize = 200;

// ── kinds ───────────────────────────────────────────────────────────────────────────────

/// The connector kinds this build serves, with the schema and defaults the UI renders.
#[crate::action(
    name = "channels.kinds",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_kinds(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    _input: crate::views::EmptyIn,
) -> Result<ChannelKindsOut, ActionError> {
    Ok(ChannelKindsOut {
        kinds: connectors::all().iter().map(kind_out).collect(),
    })
}

fn kind_out(spec: &connectors::ConnectorSpec) -> ChannelKindOut {
    ChannelKindOut {
        kind: spec.kind.to_owned(),
        label: spec.label.to_owned(),
        can_send: spec.capabilities.can_send,
        can_receive: spec.capabilities.can_receive,
        identity: match spec.capabilities.identity {
            connectors::Identity::Provisioned => "provisioned",
            connectors::Identity::Credential => "credential",
            connectors::Identity::Human => "human",
        }
        .to_owned(),
        config_schema: serde_json::to_value((spec.config_schema)()).unwrap_or(Value::Null),
        secret_fields: spec.secret_fields.iter().map(|s| (*s).to_owned()).collect(),
        default_actions: spec
            .default_actions
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        instructions: spec.instructions.to_owned(),
    }
}

// ── bindings ────────────────────────────────────────────────────────────────────────────

/// Bind one connector route to the calling tenant.
///
/// The upsert is keyed on `(tenant_id, channel_kind, route_key)`, so a replayed bind replaces
/// the config and stays one row. The config is validated by the connector's own schema, every
/// declared secret **name** is checked to exist, and a provisioned kind gets its per-tenant
/// service identity in the same transaction.
#[crate::action(
    name = "channels.bind",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_bind(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChannelBindIn,
) -> Result<ChannelBindingAdminOut, ActionError> {
    let action = "channels.bind";
    ensure_own_tenant(action, input.tenant_id, ctx.tenant)?;
    let Some(spec) = connectors::find(&input.channel_kind) else {
        return Err(invalid(
            "channel_kind",
            "invalid_value",
            format!(
                "unknown connector kind {:?}; this build serves {}",
                input.channel_kind,
                connectors::all()
                    .iter()
                    .map(|c| c.kind)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    };
    let config = input.config.unwrap_or_else(empty_object);
    (spec.validate)(&config).map_err(|detail| {
        invalid(
            "config",
            "invalid_value",
            format!("{}: {detail}", input.channel_kind),
        )
    })?;
    for name in connectors::secret_names(spec, &config) {
        let exists = store::secret::exists_named_tx(&mut *tx, ctx.tenant, &name)
            .await
            .map_err(|error| store_error(error, action))?;
        if !exists {
            return Err(invalid(
                "config",
                "secret_not_found",
                format!(
                    "the secret {name:?} does not exist in this tenant; create it first in Secrets"
                ),
            ));
        }
    }
    if spec.capabilities.identity == connectors::Identity::Provisioned {
        store::channel::ensure_service_account_tx(&mut *tx, ctx.tenant, spec.kind)
            .await
            .map_err(|error| store_error(error, action))?;
    }
    let binding = store::channel::upsert_binding_tx(
        &mut *tx,
        ctx.tenant,
        &NewChannelBinding {
            channel_kind: input.channel_kind,
            route_key: input.route_key,
            config,
            active: true,
        },
    )
    .await
    .map_err(|error| channel_error(error, action))?;
    binding_admin_out(&mut *tx, ctx.tenant, &binding).await
}

/// Read one binding.
#[crate::action(
    name = "channels.get",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChannelGetIn,
) -> Result<ChannelBindingAdminOut, ActionError> {
    let binding = store::channel::get_binding_tx(&mut *tx, ctx.tenant, input.binding_id)
        .await
        .map_err(|error| store_error(error, "channels.get"))?
        .ok_or_else(|| ActionError::resource_not_found("such channel binding"))?;
    binding_admin_out(&mut *tx, ctx.tenant, &binding).await
}

/// Every binding of the tenant.
#[crate::action(
    name = "channels.list",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: ChannelListIn,
) -> Result<ChannelListOut, ActionError> {
    let rows = store::channel::list_bindings_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "channels.list"))?;
    let mut bindings = Vec::with_capacity(rows.len());
    for row in &rows {
        bindings.push(binding_admin_out(&mut *tx, ctx.tenant, row).await?);
    }
    Ok(ChannelListOut { bindings })
}

/// Remove one channel route from the calling tenant.
#[crate::action(
    name = "channels.unbind",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_unbind(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChannelUnbindIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let action = "channels.unbind";
    ensure_own_tenant(action, input.tenant_id, ctx.tenant)?;
    let deleted = store::channel::delete_binding_tx(
        &mut *tx,
        ctx.tenant,
        &input.channel_kind,
        &input.route_key,
    )
    .await
    .map_err(|error| store_error(error, action))?;
    if !deleted {
        return Err(ActionError::resource_not_found("channel binding"));
    }
    Ok(HandlerOutput::no_content(()))
}

/// Queue one test message through a binding's real transport.
///
/// The test is the delivery itself: the row is sent by the ordinary `delivery.send` job, so the
/// UI proves the whole path (config, secret, adapter) rather than a bypass that could succeed
/// while the real path fails.
#[crate::action(
    name = "channels.test",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_test(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChannelTestIn,
) -> Result<ChannelTestOut, ActionError> {
    let action = "channels.test";
    let binding = store::channel::get_binding_tx(&mut *tx, ctx.tenant, input.binding_id)
        .await
        .map_err(|error| store_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such channel binding"))?;
    let Some(spec) = connectors::find(&binding.channel_kind) else {
        return Err(ActionError::new(
            409,
            "channel_unavailable",
            format!(
                "{action}: this build does not serve connector kind {:?}",
                binding.channel_kind
            ),
        ));
    };
    if !spec.capabilities.can_send {
        return Err(ActionError::new(
            422,
            "channel_sendless",
            format!(
                "{action}: connector kind {:?} does not send; its interactions are read through \
                 the app itself",
                binding.channel_kind
            ),
        ));
    }
    if !binding.active {
        return Err(ActionError::new(
            409,
            "channel_inactive",
            format!("{action}: the binding is inactive"),
        ));
    }
    let message = store::delivery::insert_message_tx(
        &mut *tx,
        ctx.tenant,
        &store::delivery::NewMessage {
            channel_kind: binding.channel_kind.clone(),
            sender_id: None,
            kind: "channel-test".to_owned(),
            body: "Conveyor connector test: this binding is configured correctly.".to_owned(),
            meta: json!({ "binding_id": binding.id }),
            correlation_id: None,
        },
    )
    .await
    .map_err(|error| delivery_error(error, action))?;
    let recipient = format!("binding:{}", binding.id);
    let inserted = store::delivery::insert_delivery_tx(
        &mut *tx,
        ctx.tenant,
        &store::delivery::NewDelivery::new(message.id, &binding.channel_kind, &recipient),
    )
    .await
    .map_err(|error| delivery_error(error, action))?;
    store::delivery::enqueue_delivery_send_tx(&mut *tx, ctx.tenant, inserted.delivery.id, None)
        .await
        .map_err(|error| store_error(error, action))?;
    Ok(ChannelTestOut {
        message_id: message.id.to_string(),
        delivery_id: inserted.delivery.id.to_string(),
    })
}

/// The admin projection of one binding, including secret presence (never a value).
async fn binding_admin_out(
    conn: &mut PgConnection,
    tenant: TenantId,
    binding: &ChannelBindingRecord,
) -> Result<ChannelBindingAdminOut, ActionError> {
    let spec = connectors::find(&binding.channel_kind);
    let mut secrets_set = BTreeMap::new();
    if let Some(spec) = spec {
        for name in connectors::secret_names(spec, &binding.config) {
            let exists = store::secret::exists_named_tx(&mut *conn, tenant, &name)
                .await
                .map_err(|error| store_error(error, "channels.bind"))?;
            secrets_set.insert(name, exists);
        }
    }
    Ok(ChannelBindingAdminOut {
        id: binding.id.to_string(),
        tenant_id: tenant.to_string(),
        channel_kind: binding.channel_kind.clone(),
        route_key: binding.route_key.clone(),
        config: binding.config.clone(),
        active: binding.active,
        inbound_url: spec
            .filter(|spec| spec.capabilities.can_receive)
            .map(|_| inbound_url(binding.id)),
        secrets_set,
    })
}

/// The generic inbound path the UI shows the operator.
pub fn inbound_url(binding_id: Uuid) -> String {
    format!("/api/channels/{binding_id}/inbound")
}

// ── the grant matrix ────────────────────────────────────────────────────────────────────

/// The tenant's channel authorization matrix: compiled defaults, stored overrides, and the
/// floor — in one document the screen renders.
#[crate::action(
    name = "channels.policy_get",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_policy_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: crate::views::EmptyIn,
) -> Result<ChannelPolicyView, ActionError> {
    policy_view(&mut *tx, ctx.tenant).await
}

/// Replace one machine kind's grant overrides.
#[crate::action(
    name = "channels.policy_set",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_policy_set(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChannelPolicySetIn,
) -> Result<ChannelPolicyView, ActionError> {
    let spec = machine_kind(&input.channel_kind, "channels.policy_set")?;
    let mut entries: Vec<(&str, bool)> = Vec::with_capacity(input.entries.len());
    let mut seen = std::collections::BTreeSet::new();
    for ChannelPolicyEntryIn { action, allowed } in &input.entries {
        let Some(spec_action) = crate::registry().get(action) else {
            return Err(invalid(
                "action",
                "invalid_value",
                format!("unknown action {action:?}"),
            ));
        };
        if !seen.insert(spec_action.name) {
            return Err(invalid(
                "action",
                "duplicate",
                format!("{action:?} appears more than once in this submission"),
            ));
        }
        if let Some(reason) = denial(spec_action) {
            return Err(invalid(
                "action",
                "not_grantable",
                format!("{action:?} cannot be granted to a machine channel: {reason}"),
            ));
        }
        entries.push((spec_action.name, *allowed));
    }
    store::rbac::set_channel_policy_tx(&mut *tx, ctx.tenant, spec.kind, &entries)
        .await
        .map_err(|error| store_error(error, "channels.policy_set"))?;
    policy_view(&mut *tx, ctx.tenant).await
}

/// Delete one machine kind's overrides; the compiled defaults apply again.
#[crate::action(
    name = "channels.policy_reset",
    version = 1,
    kind = "internal",
    scopes = ["channels:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn channels_policy_reset(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChannelPolicyResetIn,
) -> Result<ChannelPolicyView, ActionError> {
    let spec = machine_kind(&input.channel_kind, "channels.policy_reset")?;
    store::rbac::reset_channel_policy_tx(&mut *tx, ctx.tenant, spec.kind)
        .await
        .map_err(|error| store_error(error, "channels.policy_reset"))?;
    policy_view(&mut *tx, ctx.tenant).await
}

fn machine_kind(
    kind: &str,
    action: &str,
) -> Result<&'static connectors::ConnectorSpec, ActionError> {
    let Some(spec) = connectors::find(kind) else {
        return Err(invalid(
            "channel_kind",
            "invalid_value",
            format!("unknown connector kind {kind:?}"),
        ));
    };
    if spec.capabilities.identity == connectors::Identity::Human {
        return Err(invalid(
            "channel_kind",
            "invalid_value",
            format!("{action}: the web channel uses the human role policy, not channel grants"),
        ));
    }
    Ok(spec)
}

/// Why an action is closed to every machine channel, or `None` when it is grantable.
fn denial(spec: &crate::spec::ActionSpec) -> Option<&'static str> {
    if spec.kind == ActionKind::Workflow {
        return Some("workflow action");
    }
    if spec.scopes.contains(&Scope::EvalsApprove) {
        return Some("human sign-off");
    }
    if crate::channel::is_denied_action(spec.name) {
        return Some("person-only");
    }
    let resource = crate::channel::resource_of(spec.name);
    if resource == "workflows" {
        return Some("workflow authoring");
    }
    if spec.tenancy == crate::spec::Tenancy::Platform {
        return Some("platform");
    }
    if crate::channel::is_denied_resource(resource) {
        return Some("administration");
    }
    None
}

async fn policy_view(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<ChannelPolicyView, ActionError> {
    let bindings = store::channel::list_bindings_tx(&mut *conn, tenant)
        .await
        .map_err(|error| store_error(error, "channels.policy_get"))?;
    let mut channels = Vec::new();
    for spec in connectors::machine_kinds() {
        let overrides = store::rbac::channel_policy_tx(conn, tenant, spec.kind)
            .await
            .map_err(|error| store_error(error, "channels.policy_get"))?;
        let kind_bindings: Vec<ChannelPolicyBinding> = bindings
            .iter()
            .filter(|binding| binding.channel_kind == spec.kind)
            .map(|binding| ChannelPolicyBinding {
                route_key: binding.route_key.clone(),
                active: binding.active,
                inbound_url: spec
                    .capabilities
                    .can_receive
                    .then(|| inbound_url(binding.id)),
            })
            .collect();
        let identity = match spec.capabilities.identity {
            connectors::Identity::Provisioned => {
                let found = store::channel::service_account_tx(&mut *conn, tenant, spec.kind)
                    .await
                    .map_err(|error| store_error(error, "channels.policy_get"))?;
                found.map(|user_id| ChannelIdentityOut {
                    user_id: user_id.to_string(),
                    subject: store::channel::service_subject(tenant, spec.kind),
                    display_name: format!("{} connector", spec.kind),
                })
            }
            connectors::Identity::Credential | connectors::Identity::Human => None,
        };
        channels.push(ChannelPolicyChannel {
            channel_kind: spec.kind.to_owned(),
            label: spec.label.to_owned(),
            bound: kind_bindings.iter().any(|binding| binding.active),
            bindings: kind_bindings,
            identity,
            groups: matrix(spec, &overrides),
        });
    }
    Ok(ChannelPolicyView { channels })
}

fn matrix(
    connector: &connectors::ConnectorSpec,
    overrides: &BTreeMap<String, bool>,
) -> Vec<ChannelPolicyGroup> {
    let mut groups: BTreeMap<&'static str, Vec<ChannelPolicyAction>> = BTreeMap::new();
    for spec in crate::registry().iter() {
        if spec.kind == ActionKind::Workflow {
            continue;
        }
        let denial = denial(spec);
        let grantable = denial.is_none();
        let default = connector.default_actions.contains(&spec.name);
        let overridden = overrides.contains_key(spec.name);
        let allowed = grantable && overrides.get(spec.name).copied().unwrap_or(default);
        groups
            .entry(crate::channel::resource_of(spec.name))
            .or_default()
            .push(ChannelPolicyAction {
                action: spec.name.to_owned(),
                default: grantable && default,
                allowed,
                overridden,
                grantable,
                denial: denial.map(str::to_owned),
            });
    }
    groups
        .into_iter()
        .map(|(resource, actions)| ChannelPolicyGroup {
            resource: resource.to_owned(),
            actions,
        })
        .collect()
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────

/// S06 ships no cross-tenant binding path (CONTRACT §9): the body's tenant must be the
/// caller's, and anything else is refused before a row is read or written.
fn ensure_own_tenant(action: &str, named: Uuid, tenant: TenantId) -> Result<(), ActionError> {
    if named == tenant.as_uuid() {
        return Ok(());
    }
    Err(ActionError::new(
        403,
        "forbidden",
        format!(
            "{action}: a channel binding is written only in the caller's own tenant; \
             S06 ships no cross-tenant impersonation path"
        ),
    ))
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// The original minimal projection, kept for callers that only need the row.
#[allow(dead_code)]
fn binding_out(tenant: TenantId, row: &ChannelBindingRecord) -> ChannelBindingOut {
    ChannelBindingOut {
        id: row.id.to_string(),
        tenant_id: tenant.to_string(),
        channel_kind: row.channel_kind.clone(),
        route_key: row.route_key.clone(),
        config: row.config.clone(),
        active: row.active,
    }
}

fn channel_error(error: store::ChannelStoreError, action: &str) -> ActionError {
    match error {
        store::ChannelStoreError::Parent(parent) => super::parent_error(parent, action),
        store::ChannelStoreError::Database(error) => store_error(error, action),
    }
}

fn delivery_error(error: store::delivery::DeliveryStoreError, action: &str) -> ActionError {
    tracing::error!(%error, action, "delivery store failure");
    ActionError::internal()
}

fn store_error(error: sqlx::Error, action: &str) -> ActionError {
    if let Some(database) = error.as_database_error()
        && database.is_unique_violation()
    {
        return ActionError::new(
            409,
            "conflict",
            format!("{action:?} would create a duplicate row in this tenant"),
        );
    }
    tracing::error!(%error, action, "store failure");
    ActionError::internal()
}

/// The command help text for a channel authority; used by the inbound route.
pub fn help_text(authority: &conveyor_auth::ChannelAuthority) -> String {
    command::help_text(authority)
}

// ── the Telegram transport ──────────────────────────────────────────────────────────────

/// What one `sendMessage` attempt did.
#[derive(Clone, Debug)]
pub struct TelegramSendOutcome {
    /// True only for a 2xx answer from the Bot API.
    pub ok: bool,
    /// The HTTP status, when the API answered at all (a timeout has none).
    pub status_code: Option<u16>,
    /// Why the attempt failed, already safe to store in `delivery.last_error`.
    pub error: Option<String>,
}

/// POST one `sendMessage` to the Bot API.
///
/// The request carries the message body and the binding's chat/topic references; the bot
/// token is a path segment, so **no failure path may echo the URL** — the error strings below
/// are built from `reqwest::Error::without_url()` and the API's own `description`, which is
/// what makes them safe to persist in `delivery.last_error`.
pub async fn send_telegram_message(
    api_base: &str,
    token: &str,
    chat_id: &str,
    message_thread_id: Option<i64>,
    text: &str,
) -> TelegramSendOutcome {
    let url = format!("{}/bot{token}/sendMessage", api_base.trim_end_matches('/'));
    let mut body = serde_json::Map::new();
    body.insert("chat_id".to_owned(), json!(chat_id));
    body.insert("text".to_owned(), json!(text));
    if let Some(thread) = message_thread_id {
        body.insert("message_thread_id".to_owned(), json!(thread));
    }

    let client = match reqwest::Client::builder()
        .timeout(TELEGRAM_SEND_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return TelegramSendOutcome {
                ok: false,
                status_code: None,
                error: Some(format!(
                    "the HTTP client could not be built: {}",
                    error.without_url()
                )),
            };
        }
    };
    let outcome =
        tokio::time::timeout(TELEGRAM_SEND_TIMEOUT, client.post(&url).json(&body).send()).await;
    match outcome {
        Err(_) => TelegramSendOutcome {
            ok: false,
            status_code: None,
            error: Some(format!(
                "the telegram api did not answer within {TELEGRAM_SEND_TIMEOUT:?}"
            )),
        },
        Ok(Err(error)) => TelegramSendOutcome {
            ok: false,
            status_code: None,
            error: Some(format!(
                "the telegram api request failed: {}",
                error.without_url()
            )),
        },
        Ok(Ok(response)) => {
            let status = response.status();
            if status.is_success() {
                return TelegramSendOutcome {
                    ok: true,
                    status_code: Some(status.as_u16()),
                    error: None,
                };
            }
            let body = response.text().await.unwrap_or_default();
            let description = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default()
                .chars()
                .take(TELEGRAM_ERROR_CHARS)
                .collect::<String>();
            let detail = if description.is_empty() {
                format!("the telegram api answered {status}")
            } else {
                format!("the telegram api answered {status}: {description}")
            };
            TelegramSendOutcome {
                ok: false,
                status_code: Some(status.as_u16()),
                error: Some(detail),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mismatched_tenant_is_refused_before_any_write() {
        let tenant = TenantId::from_uuid(uuid::Uuid::now_v7()).expect("non-nil tenant");
        assert!(ensure_own_tenant("channels.bind", tenant.as_uuid(), tenant).is_ok());
        let other = uuid::Uuid::now_v7();
        let error = ensure_own_tenant("channels.bind", other, tenant).expect_err("refused");
        assert_eq!(error.status, 403);
    }

    #[test]
    fn the_inbound_path_names_the_binding() {
        let id = Uuid::now_v7();
        assert_eq!(inbound_url(id), format!("/api/channels/{id}/inbound"));
    }
}
