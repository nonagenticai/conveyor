//! `secrets.*` — write-only, sealed at rest, never revealed (S09 §5.4).
//!
//! **There is no `secrets.reveal`.** Not in this file, not anywhere in the registry — and a test in
//! `crates/api/tests/secrets_never_revealed.rs` asserts that against the served manifest, so a future action
//! that returns a value fails the suite rather than passing review.
//!
//! The value is sealed by `conveyor_store::secret` before the row is written: the plaintext exists in this
//! process's memory for one call and never in a query, a log line or an event payload. An event names the
//! secret and its version; it never carries anything derived from the value.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::secret::SecretRecord;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{invalid, secret_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// A secret's metadata. There is no field here that could hold a value.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SecretOut {
    /// The row id.
    pub id: String,
    /// The caller-chosen name, unique per tenant.
    pub name: String,
    /// `tenant` today.
    pub scope: String,
    /// The key generation the ciphertext was sealed under.
    pub key_id: String,
    /// Bumped by every rotation.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

impl From<SecretRecord> for SecretOut {
    fn from(record: SecretRecord) -> Self {
        Self {
            id: record.id.to_string(),
            name: record.name,
            scope: record.scope,
            key_id: record.key_id,
            version: record.version,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

/// Create one secret.
///
/// `value` is marked `x-secret`: utoipa and the MCP projection strip examples for it, and no response document
/// has a field that could echo it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SecretCreateIn {
    /// The name; unique per tenant.
    pub name: String,
    /// The plaintext. Write-only — never returned, never logged.
    #[schemars(extend("x-secret" = true))]
    pub value: String,
    /// `tenant` unless the caller says otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Create a secret.
#[crate::action(
    name = "secrets.create",
    version = 1,
    kind = "internal",
    scopes = ["secrets:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn secrets_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SecretCreateIn,
) -> Result<HandlerOutput<SecretOut>, ActionError> {
    let action = "secrets.create";
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    if input.value.is_empty() {
        return Err(invalid("value", "required", "must not be empty"));
    }
    let scope = input.scope.as_deref().unwrap_or("tenant").trim();
    let record =
        store::secret::create_secret_tx(&mut *tx, ctx.tenant, name, input.value.as_bytes(), scope)
            .await
            .map_err(|error| secret_error(error, action))?;
    emit_secret_event(
        &mut *tx,
        &ctx,
        "secret.created",
        record.id,
        json!({"id": record.id, "name": record.name}),
    )
    .await?;
    Ok(HandlerOutput::created(SecretOut::from(record)))
}

/// Rotate one secret: a new value under the same name, `version + 1`.
///
/// `effect = destructive`: the old credential does not survive, so an agent's call becomes a
/// proposal instead of running. The proposal seals its input (`api/src/copilot/proposal.rs`), which
/// is what lets a secret-bearing action be proposal-triggering without putting the value in
/// member-readable `approval.meta`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SecretRotateIn {
    /// The secret id.
    pub id: Uuid,
    /// The new plaintext. Write-only.
    #[schemars(extend("x-secret" = true))]
    pub value: String,
}

/// Rotate a secret. The old value does not survive (there is no history row).
#[crate::action(
    name = "secrets.rotate",
    version = 1,
    kind = "internal",
    scopes = ["secrets:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn secrets_rotate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SecretRotateIn,
) -> Result<HandlerOutput<SecretOut>, ActionError> {
    let action = "secrets.rotate";
    if input.value.is_empty() {
        return Err(invalid("value", "required", "must not be empty"));
    }
    let current = store::secret::get_secret_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| secret_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such secret"))?;
    // A rotation is a destructive change to a credential: it is If-Match-guarded like every other write.
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(SecretOut::from(current)).unwrap_or(serde_json::Value::Null),
    )?;
    let record = store::secret::rotate_secret_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        input.value.as_bytes(),
    )
    .await
    .map_err(|error| secret_error(error, action))?;
    emit_secret_event(
        &mut *tx,
        &ctx,
        "secret.rotated",
        record.id,
        json!({"id": record.id, "version": record.version}),
    )
    .await?;
    Ok(HandlerOutput::ok(SecretOut::from(record)))
}

/// The id input every single-row secret read takes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SecretIdIn {
    /// The secret id.
    pub id: Uuid,
}

/// Read one secret's metadata.
#[crate::action(
    name = "secrets.get",
    version = 1,
    kind = "internal",
    scopes = ["secrets:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn secrets_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SecretIdIn,
) -> Result<HandlerOutput<SecretOut>, ActionError> {
    let record = store::secret::get_secret_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| secret_error(error, "secrets.get"))?
        .ok_or_else(|| ActionError::resource_not_found("such secret"))?;
    Ok(HandlerOutput::ok(SecretOut::from(record)))
}

/// The list takes no filter: a tenant has few secrets and they are named uniquely.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SecretListIn {}

/// One page of secrets — metadata only.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SecretListOut {
    /// The secrets, ordered by name.
    pub items: Vec<SecretOut>,
}

/// List this tenant's secrets.
#[crate::action(
    name = "secrets.list",
    version = 1,
    kind = "internal",
    scopes = ["secrets:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn secrets_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: SecretListIn,
) -> Result<HandlerOutput<SecretListOut>, ActionError> {
    let records = store::secret::list_secrets_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| secret_error(error, "secrets.list"))?;
    Ok(HandlerOutput::ok(SecretListOut {
        items: records.into_iter().map(SecretOut::from).collect(),
    }))
}

/// Delete one secret.
#[crate::action(
    name = "secrets.delete",
    version = 1,
    kind = "internal",
    scopes = ["secrets:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn secrets_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SecretIdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let action = "secrets.delete";
    let current = store::secret::get_secret_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| secret_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such secret"))?;
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(SecretOut::from(current)).unwrap_or(serde_json::Value::Null),
    )?;
    store::secret::delete_secret_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| secret_error(error, action))?;
    emit_secret_event(
        &mut *tx,
        &ctx,
        "secret.deleted",
        input.id,
        json!({"id": input.id}),
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

/// Append one secret event in the caller's transaction. The payload never carries the value.
async fn emit_secret_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &'static str,
    id: Uuid,
    payload: serde_json::Value,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("secret:{id}"),
        event_type: event_type.to_owned(),
        aggregate_type: "secret".to_owned(),
        aggregate_id: id,
        payload,
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, event = event_type, action = ctx.action, "append secret event");
            ActionError::internal()
        })?;
    Ok(())
}
