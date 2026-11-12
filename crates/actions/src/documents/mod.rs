#![forbid(unsafe_code)]
//! S09 — documents and secrets: the two surfaces where a mistake leaks something.
//!
//! **Documents.** The row is metadata; the bytes live in MinIO (`integrations::objects`) and are addressed by
//! `object_key` with a `content_hash` the bytes must reproduce. An upload is validated *before* anything is
//! stored: 10 MiB cap, a content-type allowlist, and a magic-byte check that the declared type is the real
//! one. A download re-hashes the bytes and refuses a mismatch — a corrupted object is a `500`, never a file
//! that silently differs from the hash the row promised.
//!
//! **Secrets.** A secret is write-only. `secrets.create` and `secrets.rotate` take `value`; nothing returns it.
//! The sealed form lives in `secret.ciphertext` (see `conveyor_store::secret` for the envelope) and there is no
//! reveal action in the registry — a test asserts the manifest has none, which is stronger than a convention.
//!
//! **One event pattern.** Every mutation appends exactly one `document.*` / `secret.*` row through
//! [`emit_document_event`] in its own transaction, exactly as the portfolio surface does.
//!
//! **Reads that emit.** `documents.download` does not emit (it is a plain read of an existing fact); the
//! exports that DO emit are S08's `models.render`/`export_xmi`, recorded as CONTRACT §7's one named exception.

use serde_json::Value;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

pub mod actions;
pub mod ingest;
pub mod repositories;
pub mod secrets;

/// The upload cap, in bytes: 10 MiB exactly (S09 §5.2).
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// The content types a document may declare. `application/octet-stream` is deliberately absent.
pub const ALLOWED_CONTENT_TYPES: [&str; 8] = [
    "application/pdf",
    "application/json",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "text/csv",
    "text/markdown",
    "text/plain",
];

/// The event type every document mutation emits.
pub const DOCUMENT_CREATED: &str = "document.created";
/// Emitted by `documents.update` and by the `document.purge` job.
pub const DOCUMENT_UPDATED: &str = "document.updated";
/// Emitted by `documents.delete`.
pub const DOCUMENT_DELETED: &str = "document.deleted";
/// Emitted by `documents.ingest` — the upstream job id has been recorded on the row.
pub const DOCUMENT_INGESTED: &str = "document.ingested";
/// The job kind that removes a tombstoned document's object.
pub const DOCUMENT_PURGE_KIND: &str = "document.purge";

/// Append one document event in the caller's transaction.
pub async fn emit_document_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &'static str,
    id: uuid::Uuid,
    payload: Value,
) -> Result<(), ActionError> {
    let row = conveyor_store::NewEvent {
        topic: format!("document:{id}"),
        event_type: event_type.to_owned(),
        aggregate_type: "document".to_owned(),
        aggregate_id: id,
        payload,
        correlation_id: None,
        causation_id: None,
    };
    conveyor_store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, event = event_type, action = ctx.action, "append document event");
            ActionError::internal()
        })?;
    Ok(())
}

/// A document store failure as the status the caller can act on.
pub fn document_error(
    error: conveyor_store::document::DocumentStoreError,
    action: &str,
) -> ActionError {
    use conveyor_store::document::DocumentStoreError as E;
    match error {
        E::NotFound => ActionError::resource_not_found("such document"),
        E::VersionMismatch { current } => ActionError::precondition(format!(
            "{action}: the document is at version {}, not the version the caller supplied",
            current.get()
        )),
        E::BadCursor => invalid(
            "cursor",
            "invalid_value",
            "not a cursor this service issued",
        ),
        E::Parent(error) => crate::control::parent_error(error, action),
        E::Database(error) => crate::control::store_error(error, action),
    }
}

/// A secret store failure. A missing KEK is a `500` with a named cause — never a plaintext write.
pub fn secret_error(error: conveyor_store::secret::SecretError, action: &str) -> ActionError {
    use conveyor_store::secret::SecretError as E;
    match error {
        E::NotFound => ActionError::resource_not_found("such secret"),
        E::VersionMismatch { current } => ActionError::precondition(format!(
            "{action}: the secret is at version {}, not the version the caller supplied",
            current.get()
        )),
        E::NameTaken(name) => ActionError::new(
            409,
            "conflict",
            format!("{action}: a secret named {name:?} already exists in this tenant"),
        ),
        E::KeyUnavailable => {
            tracing::error!(action, "CONVEYOR_SECRET_KEK is absent or malformed");
            ActionError::new(
                500,
                "internal",
                "the secret key is not configured; refusing to store or return a value",
            )
        }
        E::UnknownKeyId(key) => {
            tracing::error!(action, %key, "secret key generation is not available");
            ActionError::new(
                500,
                "internal",
                "the secret key generation is not available",
            )
        }
        E::Undecryptable => ActionError::new(500, "internal", "the stored secret cannot be opened"),
        E::Database(error) => crate::control::store_error(error, action),
    }
}

/// An object-store failure. Absent configuration is a `503`: the service is not able to store bytes.
pub fn object_error(error: crate::integrations::objects::ObjectError, action: &str) -> ActionError {
    use crate::integrations::objects::ObjectError as E;
    match error {
        E::Unavailable => {
            tracing::error!(action, "object storage is not configured");
            ActionError::new(503, "unavailable", "object storage is not configured")
        }
        E::NotFound => ActionError::new(500, "internal", "the document's object is missing"),
        E::Store(detail) => {
            tracing::error!(action, %detail, "object storage failure");
            ActionError::new(500, "internal", "object storage failed")
        }
    }
}

/// A `422` naming the field, with the code the caller can act on.
pub fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}
