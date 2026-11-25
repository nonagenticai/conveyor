#![forbid(unsafe_code)]
//! `tool_bindings.create|update|retire` — the grant becomes operable (SPEC_V2 §11.4, §11.8;
//! issue #557).
//!
//! A binding is tenant-scoped and names exactly one target: a durable job binding or a run. It
//! pins one published toolkit version and narrows that toolkit's effect with a ceiling. The
//! ceiling can only narrow: `create` refuses a ceiling above the strongest effect the toolkit
//! publishes, and `update` refuses a ceiling above the current one. The broker checks the same
//! rank again at the call, so the action is not the only guard.
//!
//! `update` and `retire` require `If-Match` (or `_version`). A stale precondition is `412` with
//! the current representation, the repository-wide CONTRACT §3 rule — not the `409` the issue's
//! first draft named.

use conveyor_store as store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{Effect, FieldError};
use crate::views::{Page, PageIn};

/// `tool_bindings.create`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct BindingCreateIn {
    /// The durable job binding this grant belongs to. Exactly one of this and `run_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// The one-off run this grant belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The published toolkit version to pin.
    pub toolkit_id: Uuid,
    /// The effect ceiling (default `read`); it may not exceed the toolkit's strongest tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_ceiling: Option<String>,
    /// The sealed credential alias the broker resolves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// The per-minute rate limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_min: Option<i32>,
}

/// `tool_bindings.update`'s input. Absent fields keep their stored value; an empty credential
/// alias clears it.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct BindingUpdateIn {
    /// The binding to update.
    pub id: Uuid,
    /// The new ceiling; it may only narrow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_ceiling: Option<String>,
    /// The new credential alias; the empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// The new per-minute rate limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_min: Option<i32>,
    /// Whether the grant is live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// `tool_bindings.retire`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct BindingRetireIn {
    /// The binding to retire.
    pub id: Uuid,
}

/// One binding, as the actions return it.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct BindingOut {
    /// The row.
    pub id: Uuid,
    /// The durable job binding, when this is a job grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// The one-off run, when this is a run grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The pinned toolkit version.
    pub toolkit_id: Uuid,
    /// The effect ceiling.
    pub effect_ceiling: String,
    /// The sealed credential alias.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// The per-minute rate limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_min: Option<i32>,
    /// Whether the grant is live.
    pub enabled: bool,
    /// The If-Match version.
    pub version: i64,
}

fn invalid(field: &str, code: &str, detail: String) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail,
    }])
}

fn out(row: &store::tool::BindingRow) -> BindingOut {
    BindingOut {
        id: row.id,
        job_binding_id: row.job_binding_id,
        run_id: row.run_id,
        toolkit_id: row.toolkit_id,
        effect_ceiling: row.effect_ceiling.clone(),
        credential_alias: row.credential_alias.clone(),
        rate_limit_per_min: row.rate_limit_per_min,
        enabled: row.enabled,
        version: row.version,
    }
}

/// The current representation a stale `If-Match` returns (CONTRACT §3).
fn current(row: &store::tool::BindingRow) -> Value {
    json!({
        "id": row.id,
        "job_binding_id": row.job_binding_id,
        "run_id": row.run_id,
        "toolkit_id": row.toolkit_id,
        "effect_ceiling": row.effect_ceiling,
        "credential_alias": row.credential_alias,
        "rate_limit_per_min": row.rate_limit_per_min,
        "enabled": row.enabled,
        "version": row.version,
    })
}

/// The expected row version, or the `412` the contract requires.
fn expected_version(ctx: &HandlerCtx, current: Value) -> Result<i64, ActionError> {
    crate::board::require_precondition_with(ctx, ctx.action, current)?;
    ctx.version.ok_or_else(ActionError::internal)
}

#[crate::action(
    name = "tool_bindings.create",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_bindings_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingCreateIn,
) -> Result<BindingOut, ActionError> {
    match (input.job_binding_id.is_some(), input.run_id.is_some()) {
        (true, false) | (false, true) => {}
        _ => {
            return Err(invalid(
                "job_binding_id",
                "one_target",
                "name exactly one of job_binding_id or run_id".to_owned(),
            ));
        }
    }
    if let Some(rate) = input.rate_limit_per_min
        && rate <= 0
    {
        return Err(invalid(
            "rate_limit_per_min",
            "range",
            "the rate limit must be positive".to_owned(),
        ));
    }
    let toolkit = store::tool::toolkit_by_id(&mut *tx, ctx.tenant, input.toolkit_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "tool_bindings.create: read the toolkit");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("toolkit"))?;
    if toolkit.status != "published" {
        return Err(ActionError::resource_not_found("toolkit"));
    }
    let tools = store::tool::tools_of(&mut *tx, ctx.tenant, toolkit.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "tool_bindings.create: read the toolkit's tools");
            ActionError::internal()
        })?;
    let strongest = tools
        .iter()
        .filter_map(|tool| Effect::parse(&tool.effect))
        .map(Effect::rank)
        .max()
        .unwrap_or(Effect::Read.rank());
    let ceiling_text = input.effect_ceiling.as_deref().unwrap_or("read");
    let ceiling = Effect::parse(ceiling_text).ok_or_else(|| {
        invalid(
            "effect_ceiling",
            "effect",
            format!("{ceiling_text:?} is not read|write|spend|destructive"),
        )
    })?;
    if ceiling.rank() > strongest {
        // #557 P2.1 names the widened-grant refusal with the effect code the T18 Tools contract
        // and SPEC_V2 §11.4 use at the call: `403 tool_effect_forbidden`. The issue's first
        // draft said 422; the correction is recorded on #557.
        return Err(ActionError::new(
            403,
            "tool_effect_forbidden",
            format!(
                "{ceiling_text:?} is above the toolkit's strongest effect; the binding narrows only"
            ),
        ));
    }
    let row = store::tool::insert_binding(
        &mut *tx,
        ctx.tenant,
        &store::tool::NewBinding {
            job_binding_id: input.job_binding_id,
            run_id: input.run_id,
            toolkit_id: toolkit.id,
            effect_ceiling: ceiling.as_str(),
            credential_alias: input.credential_alias.as_deref(),
            rate_limit_per_min: input.rate_limit_per_min,
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "tool_bindings.create: insert the binding");
        ActionError::internal()
    })?;
    Ok(out(&row))
}

#[crate::action(
    name = "tool_bindings.update",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_bindings_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingUpdateIn,
) -> Result<BindingOut, ActionError> {
    let row = store::tool::binding_by_id(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "tool_bindings.update: read the binding");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("tool binding"))?;
    let expected = expected_version(&ctx, current(&row))?;

    let current_ceiling = Effect::parse(&row.effect_ceiling).unwrap_or(Effect::Read);
    let ceiling = match input.effect_ceiling.as_deref() {
        Some(text) => {
            let parsed = Effect::parse(text).ok_or_else(|| {
                invalid(
                    "effect_ceiling",
                    "effect",
                    format!("{text:?} is not read|write|spend|destructive"),
                )
            })?;
            if parsed.rank() > current_ceiling.rank() {
                return Err(ActionError::new(
                    403,
                    "tool_effect_forbidden",
                    format!(
                        "{text:?} is above the current ceiling {:?}; the binding may only narrow",
                        row.effect_ceiling
                    ),
                ));
            }
            parsed
        }
        None => current_ceiling,
    };
    if let Some(rate) = input.rate_limit_per_min
        && rate <= 0
    {
        return Err(invalid(
            "rate_limit_per_min",
            "range",
            "the rate limit must be positive".to_owned(),
        ));
    }
    let credential = match input.credential_alias.as_deref() {
        Some("") => None,
        Some(value) => Some(value.to_owned()),
        None => row.credential_alias.clone(),
    };
    let updated = store::tool::update_binding(
        &mut *tx,
        ctx.tenant,
        row.id,
        expected,
        &store::tool::BindingUpdate {
            effect_ceiling: ceiling.as_str(),
            credential_alias: credential.as_deref(),
            rate_limit_per_min: input.rate_limit_per_min.or(row.rate_limit_per_min),
            enabled: input.enabled.unwrap_or(row.enabled),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "tool_bindings.update: update the binding");
        ActionError::internal()
    })?;
    match updated {
        Some(row) => Ok(out(&row)),
        None => {
            let latest = store::tool::binding_by_id(&mut *tx, ctx.tenant, input.id)
                .await
                .ok()
                .flatten();
            Err(ActionError::precondition_with(
                latest.as_ref().map(current).unwrap_or(Value::Null),
                "the binding changed; re-read it".to_owned(),
            ))
        }
    }
}

#[crate::action(
    name = "tool_bindings.retire",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_bindings_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingRetireIn,
) -> Result<BindingOut, ActionError> {
    let row = store::tool::binding_by_id(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "tool_bindings.retire: read the binding");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("tool binding"))?;
    let expected = expected_version(&ctx, current(&row))?;
    let actor = ctx.actor_id.to_string();
    let retired = store::tool::retire_binding(&mut *tx, ctx.tenant, row.id, expected, &actor)
        .await
        .map_err(|error| {
            tracing::error!(%error, "tool_bindings.retire: retire the binding");
            ActionError::internal()
        })?;
    match retired {
        Some(row) => Ok(out(&row)),
        None => {
            let latest = store::tool::binding_by_id(&mut *tx, ctx.tenant, input.id)
                .await
                .ok()
                .flatten();
            Err(ActionError::precondition_with(
                latest.as_ref().map(current).unwrap_or(Value::Null),
                "the binding changed; re-read it".to_owned(),
            ))
        }
    }
}

/// `tool_bindings.list`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct BindingListIn {
    /// Only this durable job binding's grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// Only this run's grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// Only live (`true`) or disabled (`false`) grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The shared page envelope; the list is capped at 200 and is not cursor-paged.
    #[serde(default)]
    pub page: PageIn,
}

/// Every grant in the tenant, filtered by the target the caller names.
#[crate::action(
    name = "tool_bindings.list",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_bindings_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingListIn,
) -> Result<Page<BindingOut>, ActionError> {
    let limit = i64::from(input.page.limit.unwrap_or(50).clamp(1, 200));
    let rows = store::tool::bindings_list(
        &mut *tx,
        ctx.tenant,
        input.job_binding_id,
        input.run_id,
        input.enabled,
        limit,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "tool_bindings.list: read the grants");
        ActionError::internal()
    })?;
    Ok(Page {
        items: rows.iter().map(out).collect(),
        next_cursor: None,
    })
}
