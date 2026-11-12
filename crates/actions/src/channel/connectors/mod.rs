#![forbid(unsafe_code)]
//! The connector registry (S06d).
//!
//! A connector is **one small declaration**: a typed config (whose `schemars` schema renders the
//! web form), the secret fields it names, its compiled default grant set, and three functions —
//! `authenticate`, `parse`, `send`. Everything else (the service identity, the grant check, the
//! shared command grammar, the multicast dispatch, the delivery state machine) is core and is
//! shared by every connector. Onboarding a gateway that speaks the JSON envelope is a
//! `webhook` binding and **zero Rust**; a native API is one module in this directory.
//!
//! The core never parses a platform payload and never authorizes. It hands the adapter bytes and
//! receives an [`Inbound`]; the `channel` module decides what the caller may do.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use conveyor_domain::TenantId;
use http::HeaderMap;
use schemars::Schema;
use serde_json::Value;
use sqlx::PgConnection;

use super::MCP_DEFAULT_ACTIONS;

pub mod telegram;
pub mod webhook;

/// What one connector kind can do, for the admin screen and the dispatcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    /// The core provisions one service account per `(tenant, kind)`.
    Provisioned,
    /// The identity comes from the caller's credential (the MCP licence principal).
    Credential,
    /// A human session (the web channel); grants are the tenant role policy.
    Human,
}

/// A connector's transport capabilities.
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    /// The interaction dispatcher may offer requests to a binding of this kind.
    pub can_send: bool,
    /// The generic inbound route can receive a response.
    pub can_receive: bool,
    pub identity: Identity,
}

/// What the adapter extracted from one inbound request. No authority is decided here.
#[derive(Clone, Debug, Default)]
pub struct Inbound {
    /// The platform sender id/address; audit evidence, never authorization.
    pub sender_ref: Option<String>,
    /// Free text (the shared command grammar parses it).
    pub text: Option<String>,
    /// A structured `{action, input}` command, for JSON gateways.
    pub structured: Option<Value>,
    /// The connector's own replay key (Telegram `update_id`, webhook `id`).
    pub idempotency_key: Option<String>,
    /// A request id for the invocation audit.
    pub request_id: Option<String>,
}

/// Everything an adapter needs to make one outbound call.
#[derive(Clone, Debug)]
pub struct SendRequest {
    /// The binding's non-secret config.
    pub config: Value,
    /// The resolved plaintext values, keyed by the config field that names each secret.
    pub secrets: BTreeMap<String, String>,
    /// The message body.
    pub text: String,
}

/// What one send attempt did.
#[derive(Clone, Debug)]
pub struct SendOutcome {
    /// True only for a 2xx from the platform.
    pub ok: bool,
    /// The status, when the platform answered at all.
    pub status_code: Option<u16>,
    /// Why it failed, already safe to store in `delivery.last_error`.
    pub error: Option<String>,
}

impl SendOutcome {
    /// The success outcome.
    pub fn ok(status: u16) -> Self {
        Self {
            ok: true,
            status_code: Some(status),
            error: None,
        }
    }

    /// The failure outcome.
    pub fn failed(status: Option<u16>, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            status_code: status,
            error: Some(error.into()),
        }
    }
}

/// The future an adapter's `send` returns (it owns its request, so it is `'static`).
pub type SendFuture = Pin<Box<dyn Future<Output = SendOutcome> + Send>>;

/// One connector kind's declaration.
pub struct ConnectorSpec {
    pub kind: &'static str,
    pub label: &'static str,
    pub capabilities: Capabilities,
    /// The typed config's schema; the web UI renders the form from it.
    pub config_schema: fn() -> Schema,
    /// Config fields whose value is the **name** of a sealed tenant secret.
    pub secret_fields: &'static [&'static str],
    /// The compiled grant default (overridable per tenant; the machine floor still wins).
    pub default_actions: &'static [&'static str],
    /// One paragraph the UI shows beside the binding.
    pub instructions: &'static str,
    /// Deserialize the config (which also rejects unknown fields).
    pub validate: fn(&Value) -> Result<(), String>,
    /// Verify the per-binding inbound credential.
    pub authenticate: fn(&HeaderMap, &Value, &BTreeMap<String, String>) -> bool,
    /// Platform payload → [`Inbound`]. `None` is a consumed no-op.
    pub parse: fn(&[u8], &Value) -> Option<Inbound>,
    /// Platform call. The core resolved the named secrets and never passes a credential value
    /// to an action response.
    pub send: fn(SendRequest) -> SendFuture,
}

/// The built-in registry. Adding a native connector is one module and one entry here.
pub static CONNECTORS: [ConnectorSpec; 4] = [telegram::SPEC, webhook::SPEC, web::SPEC, mcp::SPEC];

/// Every built-in connector.
pub fn all() -> &'static [ConnectorSpec] {
    &CONNECTORS
}

/// The declaration for `kind`, when this build serves it.
pub fn find(kind: &str) -> Option<&'static ConnectorSpec> {
    CONNECTORS.iter().find(|connector| connector.kind == kind)
}

/// The kinds with a machine service identity and tenant-configurable grants.
pub fn machine_kinds() -> impl Iterator<Item = &'static ConnectorSpec> {
    CONNECTORS
        .iter()
        .filter(|connector| connector.capabilities.identity != Identity::Human)
}

/// The config fields that name a sealed secret, in declaration order.
pub fn secret_names(spec: &ConnectorSpec, config: &Value) -> Vec<String> {
    spec.secret_fields
        .iter()
        .filter_map(|field| {
            config
                .get(*field)
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// Resolve every secret the config names. The caller must pass a tenant-scoped transaction.
pub async fn resolve_secrets_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    spec: &ConnectorSpec,
    config: &Value,
) -> Result<BTreeMap<String, String>, ResolveError> {
    let mut resolved = BTreeMap::new();
    for name in secret_names(spec, config) {
        let bytes = conveyor_store::secret::open_named_tx(&mut *conn, tenant, &name).await?;
        let text = String::from_utf8(bytes).map_err(|_| ResolveError::NotUtf8(name.clone()))?;
        resolved.insert(name, text);
    }
    Ok(resolved)
}

/// Why a connector's secrets could not be resolved.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The named secret is absent (or the KEK is unavailable).
    #[error(transparent)]
    Secret(#[from] conveyor_store::secret::SecretError),
    /// The stored value is not a UTF-8 credential.
    #[error("the secret {0:?} is not valid UTF-8")]
    NotUtf8(String),
}

/// The web channel: no external transport. Its deliveries are the in-app inbox rows and its
/// actions are the human session's; there is nothing to configure.
mod web {
    use super::*;

    /// `web` takes no config.
    #[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Config {}

    fn validate(config: &Value) -> Result<(), String> {
        serde_json::from_value::<Config>(config.clone())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn schema() -> Schema {
        schemars::schema_for!(Config)
    }

    fn never_authenticate(
        _headers: &HeaderMap,
        _config: &Value,
        _secrets: &BTreeMap<String, String>,
    ) -> bool {
        false
    }

    fn never_parse(_body: &[u8], _config: &Value) -> Option<Inbound> {
        None
    }

    fn never_send(_request: SendRequest) -> SendFuture {
        Box::pin(async { SendOutcome::failed(None, "the web channel has no external transport") })
    }

    pub const SPEC: ConnectorSpec = ConnectorSpec {
        kind: conveyor_domain::channel::WEB,
        label: "Web",
        capabilities: Capabilities {
            can_send: true,
            can_receive: false,
            identity: Identity::Human,
        },
        config_schema: schema,
        secret_fields: &[],
        default_actions: &[],
        instructions: "The in-app inbox. Its authorization is the human role policy; no service \
                       account or grant matrix applies.",
        validate,
        authenticate: never_authenticate,
        parse: never_parse,
        send: never_send,
    };
}

/// The MCP channel: the agent calls the conveyor's own `/mcp`; there is no webhook to configure
/// and pending interactions reach it through `tasks/get`, so the dispatcher does not push.
mod mcp {
    use super::*;

    /// `mcp` takes no config.
    #[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Config {}

    fn validate(config: &Value) -> Result<(), String> {
        serde_json::from_value::<Config>(config.clone())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn schema() -> Schema {
        schemars::schema_for!(Config)
    }

    fn never_authenticate(
        _headers: &HeaderMap,
        _config: &Value,
        _secrets: &BTreeMap<String, String>,
    ) -> bool {
        false
    }

    fn never_parse(_body: &[u8], _config: &Value) -> Option<Inbound> {
        None
    }

    fn never_send(_request: SendRequest) -> SendFuture {
        Box::pin(async {
            SendOutcome::failed(None, "MCP pending interactions are read through tasks/get")
        })
    }

    pub const SPEC: ConnectorSpec = ConnectorSpec {
        kind: conveyor_domain::channel::MCP,
        label: "MCP",
        capabilities: Capabilities {
            can_send: false,
            can_receive: false,
            identity: Identity::Credential,
        },
        config_schema: schema,
        secret_fields: &[],
        default_actions: &MCP_DEFAULT_ACTIONS,
        instructions: "The agent's own MCP surface. Its identity is the licence principal; no \
                       config is stored here, and pending approvals arrive through MCP tasks.",
        validate,
        authenticate: never_authenticate,
        parse: never_parse,
        send: never_send,
    };
}
