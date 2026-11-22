#![forbid(unsafe_code)]
//! `tool_bindings.*` — the writer for the tool grant (SPEC_V2 §11.1, §11.4; T13 finding
//! `v1-p111-1`).
//!
//! A `tool_binding` is one of the two rows that authorise a tool call: it pins a published
//! toolkit version to a job binding or a single run, carries the effect ceiling the broker
//! enforces and the sealed credential alias it opens. Until this module existed, only the tests
//! wrote that row, so a real tenant's broker could never resolve a call.
//!
//! * `tool_bindings.create` — resolve the published toolkit version, validate one target and the
//!   ceiling, insert the grant, read it back.
//! * `tool_bindings.update` — narrow the ceiling, set or clear the credential alias, enable or
//!   disable the grant. The ceiling narrows, never raises (§11.4).
//! * `tool_bindings.retire` — disable the grant; in-flight runs finish, new runs do not resolve.
//! * `tool_bindings.list` — the grants for one job binding, one run, or all.
//!
//! A draft or retired toolkit version is refused at creation: a binding must pin something the
//! broker may call. The write is tenant-scoped and every response is the row read back.

use conveyor_store as store;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};

const CEILINGS: [&str; 4] = ["read", "write", "spend", "destructive"];

fn store_error(error: store::tool::ToolStoreError, action: &'static str) -> ActionError {
    if let store::tool::ToolStoreError::Database(sqlx::Error::Database(detail)) = &error
        && detail.is_unique_violation()
    {
        return ActionError::new(
            409,
            "binding_exists",
            "this target already holds a binding for that toolkit version",
        );
    }
    tracing::error!(%error, action, "tool binding store error");
    ActionError::internal()
}

/// `tool_bindings.create`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolBindingCreateIn {
    /// The toolkit's callable prefix (`coding`).
    pub toolkit_key: String,
    /// The published version to pin; absent = the latest published version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit_version: Option<i64>,
    /// The durable job binding this grant belongs to (one of the two targets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// The single run this grant belongs to (one of the two targets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// `read | write | spend | destructive`; default `read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_ceiling: Option<String>,
    /// The sealed tenant secret the broker opens for a credentialed destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
}

/// `tool_bindings.update`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolBindingUpdateIn {
    /// The binding.
    pub id: Uuid,
    /// The new ceiling; it narrows, never raises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_ceiling: Option<String>,
    /// The new sealed credential alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// Clear the credential alias (mutually exclusive with `credential_alias`).
    #[serde(default)]
    pub clear_credential_alias: bool,
    /// Enable or disable the grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// `tool_bindings.retire`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolBindingRetireIn {
    /// The binding.
    pub id: Uuid,
}

/// `tool_bindings.list`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolBindingListIn {
    /// Only the grants for this job binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// Only the grants for this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
}

/// One binding as the actions return it.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolBindingOut {
    /// The binding id.
    pub id: Uuid,
    /// The job binding target.
    pub job_binding_id: Option<Uuid>,
    /// The run target.
    pub run_id: Option<Uuid>,
    /// The pinned toolkit version row.
    pub toolkit_id: Uuid,
    /// The toolkit key.
    pub toolkit_key: String,
    /// The toolkit's semantic version.
    pub toolkit_version: i64,
    /// The enforced ceiling.
    pub effect_ceiling: String,
    /// The sealed credential alias.
    pub credential_alias: Option<String>,
    /// Whether the broker resolves it.
    pub enabled: bool,
    /// The row lock.
    pub version: i64,
}

impl From<store::tool::ToolBindingRecord> for ToolBindingOut {
    fn from(row: store::tool::ToolBindingRecord) -> Self {
        Self {
            id: row.id,
            job_binding_id: row.job_binding_id,
            run_id: row.run_id,
            toolkit_id: row.toolkit_id,
            toolkit_key: row.toolkit_key,
            toolkit_version: row.toolkit_version,
            effect_ceiling: row.effect_ceiling,
            credential_alias: row.credential_alias,
            enabled: row.enabled,
            version: row.version,
        }
    }
}

/// `tool_bindings.list`'s answer.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolBindingListOut {
    /// The bindings, toolkit key order.
    pub bindings: Vec<ToolBindingOut>,
}

fn validate_ceiling(value: &str) -> Result<&str, ActionError> {
    if CEILINGS.contains(&value) {
        Ok(value)
    } else {
        Err(ActionError::new(
            422,
            "effect_ceiling_invalid",
            format!("effect_ceiling must be one of {CEILINGS:?}"),
        ))
    }
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
    input: ToolBindingCreateIn,
) -> Result<ToolBindingOut, ActionError> {
    if input.job_binding_id.is_some() == input.run_id.is_some() {
        return Err(ActionError::new(
            422,
            "binding_target",
            "exactly one of job_binding_id and run_id is required",
        ));
    }
    let ceiling = input.effect_ceiling.as_deref().unwrap_or("read");
    validate_ceiling(ceiling)?;
    let toolkit = match input.toolkit_version {
        Some(version) => {
            store::tool::toolkit_by_key_version(&mut *tx, ctx.tenant, &input.toolkit_key, version)
                .await
                .map_err(|error| store_error(error, "tool_bindings.create"))?
        }
        None => store::tool::latest_toolkit(&mut *tx, ctx.tenant, &input.toolkit_key)
            .await
            .map_err(|error| store_error(error, "tool_bindings.create"))?,
    }
    .ok_or_else(|| {
        ActionError::new(
            404,
            "toolkit_not_found",
            format!("no toolkit {:?} at that version", input.toolkit_key),
        )
    })?;
    if toolkit.status != "published" {
        return Err(ActionError::new(
            409,
            "toolkit_not_published",
            format!(
                "toolkit {:?} version {} is {}; only a published version can be bound",
                toolkit.key, toolkit.toolkit_version, toolkit.status
            ),
        ));
    }
    let id = store::tool::insert_binding(
        &mut *tx,
        ctx.tenant,
        &store::tool::NewToolBinding {
            job_binding_id: input.job_binding_id,
            run_id: input.run_id,
            toolkit_id: toolkit.id,
            effect_ceiling: ceiling,
            credential_alias: input.credential_alias.as_deref(),
        },
    )
    .await
    .map_err(|error| store_error(error, "tool_bindings.create"))?;
    let row = store::tool::binding_by_id(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| store_error(error, "tool_bindings.create"))?
        .ok_or_else(ActionError::internal)?;
    Ok(row.into())
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
    input: ToolBindingUpdateIn,
) -> Result<ToolBindingOut, ActionError> {
    if let Some(ceiling) = input.effect_ceiling.as_deref() {
        validate_ceiling(ceiling)?;
    }
    if input.clear_credential_alias && input.credential_alias.is_some() {
        return Err(ActionError::new(
            422,
            "credential_alias_ambiguous",
            "set credential_alias or clear_credential_alias, not both",
        ));
    }
    let alias = if input.clear_credential_alias {
        Some(None)
    } else {
        input.credential_alias.as_deref().map(Some)
    };
    let changed = store::tool::update_binding(
        &mut *tx,
        ctx.tenant,
        input.id,
        &store::tool::ToolBindingPatch {
            effect_ceiling: input.effect_ceiling.as_deref(),
            credential_alias: alias,
            enabled: input.enabled,
        },
    )
    .await
    .map_err(|error| store_error(error, "tool_bindings.update"))?;
    if changed == 0 {
        return Err(ActionError::new(
            404,
            "binding_not_found",
            "no such binding",
        ));
    }
    let row = store::tool::binding_by_id(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_error(error, "tool_bindings.update"))?
        .ok_or_else(ActionError::internal)?;
    Ok(row.into())
}

#[crate::action(
    name = "tool_bindings.retire",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_bindings_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolBindingRetireIn,
) -> Result<ToolBindingOut, ActionError> {
    let row = store::tool::binding_by_id(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_error(error, "tool_bindings.retire"))?
        .ok_or_else(|| ActionError::new(404, "binding_not_found", "no such binding"))?;
    store::tool::retire_binding(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_error(error, "tool_bindings.retire"))?;
    let row = store::tool::binding_by_id(&mut *tx, ctx.tenant, row.id)
        .await
        .map_err(|error| store_error(error, "tool_bindings.retire"))?
        .ok_or_else(ActionError::internal)?;
    Ok(row.into())
}

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
    input: ToolBindingListIn,
) -> Result<ToolBindingListOut, ActionError> {
    let rows = store::tool::bindings_for(&mut *tx, ctx.tenant, input.job_binding_id, input.run_id)
        .await
        .map_err(|error| store_error(error, "tool_bindings.list"))?;
    Ok(ToolBindingListOut {
        bindings: rows.into_iter().map(ToolBindingOut::from).collect(),
    })
}
