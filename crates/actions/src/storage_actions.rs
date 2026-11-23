#![forbid(unsafe_code)]
//! `storage.put` / `storage.get` — the job-facing object path (SPEC_V2 §11.10; T13 finding
//! `v1-p1110-1`).
//!
//! `JobStorage` already enforces the contract: the key starts `tenant/<tenant>/`, the write is
//! size-capped, the read is hash-checkable and the configured store asks for SSE. What was
//! missing is a live call path — no action, tool or toolkit called the adapter, so a run could
//! not persist bytes. These two actions are that path. The tenant comes from the session, never
//! from the input, so a cross-tenant key cannot be expressed; the caller names only a project or
//! run and one safe segment. Bytes travel base64 inside the JSON envelope.

use base64::Engine as _;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::storage::{JobStorage, StorageError, sha256_hex};

fn storage() -> Result<JobStorage, ActionError> {
    JobStorage::from_env().ok_or_else(|| {
        ActionError::new(
            503,
            "storage_unavailable",
            "object storage is not configured on this service",
        )
    })
}

fn map_error(error: StorageError) -> ActionError {
    match error {
        StorageError::Unavailable => ActionError::new(
            503,
            "storage_unavailable",
            "object storage is not configured on this service",
        ),
        StorageError::InvalidName(name) => ActionError::new(
            422,
            "object_name_invalid",
            format!("{name:?} is not a safe single key segment"),
        ),
        StorageError::TooLarge { size, max } => ActionError::new(
            413,
            "object_too_large",
            format!("the object is {size} bytes; the adapter caps one object at {max}"),
        ),
        StorageError::NotFound => {
            ActionError::new(404, "object_not_found", "no object at that key")
        }
        StorageError::HashMismatch { expected, actual } => ActionError::new(
            502,
            "object_hash_mismatch",
            format!("the object's sha256 is {actual}, not the expected {expected}"),
        ),
        StorageError::Store(detail) => ActionError::new(502, "storage_failed", detail),
    }
}

/// `storage.put`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct StoragePutIn {
    /// The project segment, when the object belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The run segment, when the object belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// One safe segment (`report.csv`); no separators, no `..`.
    pub name: String,
    /// The bytes, base64-encoded.
    pub body_base64: String,
}

/// `storage.get`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct StorageGetIn {
    /// The project segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The run segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// One safe segment.
    pub name: String,
}

/// What a write produced.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct StoredObjectOut {
    /// The tenant-prefixed key.
    pub key: String,
    /// The SHA-256 of the stored bytes.
    pub sha256: String,
    /// The number of bytes stored.
    pub bytes: u64,
}

/// What a read returned.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ReadObjectOut {
    /// The tenant-prefixed key.
    pub key: String,
    /// The SHA-256 of the returned bytes.
    pub sha256: String,
    /// The number of bytes returned.
    pub bytes: u64,
    /// The bytes, base64-encoded.
    pub body_base64: String,
}

#[crate::action(
    name = "storage.put",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn storage_put(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StoragePutIn,
) -> Result<StoredObjectOut, ActionError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&input.body_base64)
        .map_err(|error| {
            ActionError::new(
                422,
                "object_body_invalid",
                format!("body_base64 is not valid base64: {error}"),
            )
        })?;
    let storage = storage()?;
    let stored = storage
        .put(
            ctx.tenant.as_uuid(),
            input.project_id,
            input.run_id,
            &input.name,
            Bytes::from(bytes),
        )
        .await
        .map_err(map_error)?;
    Ok(StoredObjectOut {
        key: stored.key,
        sha256: stored.sha256,
        bytes: stored.bytes as u64,
    })
}

#[crate::action(
    name = "storage.get",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn storage_get(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StorageGetIn,
) -> Result<ReadObjectOut, ActionError> {
    let storage = storage()?;
    let bytes = storage
        .get(
            ctx.tenant.as_uuid(),
            input.project_id,
            input.run_id,
            &input.name,
        )
        .await
        .map_err(map_error)?;
    let key = storage
        .key(
            ctx.tenant.as_uuid(),
            input.project_id,
            input.run_id,
            &input.name,
        )
        .map_err(map_error)?;
    Ok(ReadObjectOut {
        key,
        sha256: sha256_hex(&bytes),
        bytes: bytes.len() as u64,
        body_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}
