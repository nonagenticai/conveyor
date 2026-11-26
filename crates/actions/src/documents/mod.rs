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
// #450: `artifacts.list` — the V1 catalogue name served from this store (§11.3, §11.10, D57).
pub mod artifacts;
pub mod ingest;
pub mod repositories;
pub mod revisions;
pub mod secrets;

/// The upload cap, in bytes: 10 MiB exactly (S09 §5.2).
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// The text-output cap, in bytes: 256 KiB. An output is a design note, evidence record or
/// decision, not a file; binary content stays on the 10 MiB object path.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// The content types a document may declare. `application/octet-stream` is deliberately absent.
/// Every type the store will hold, each with a magic-byte check in
/// `actions::bytes_match_type` — the allowlist and the verification are one decision and must
/// change together.
///
/// The three media types are here because a media executor's INPUT is a document: a
/// `runner/v1` job reads its footage through `GET /api/jobs/{id}/inputs/{name}`, and that route
/// resolves a document row. Without them a tenant had no way to put a video in a job at all
/// (#185). A 10 MiB upload cap still applies, so this is for short clips and stills, not masters.
pub const ALLOWED_CONTENT_TYPES: [&str; 12] = [
    "application/pdf",
    "application/json",
    "audio/mpeg",
    "audio/wav",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "text/csv",
    "text/markdown",
    "text/plain",
    "video/mp4",
    // #366: a workbook `sheets.export` produced in document mode. It is a ZIP container like
    // pptx/docx, so the declared type is the caller's claim and the magic bytes are the proof.
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
];

/// The event type every document mutation emits.
pub const DOCUMENT_CREATED: &str = "document.created";
/// Emitted by `documents.update` and by the `document.purge` job.
pub const DOCUMENT_UPDATED: &str = "document.updated";
/// Emitted by `documents.delete`.
pub const DOCUMENT_DELETED: &str = "document.deleted";
/// Emitted by `documents.ingest` — the upstream job id has been recorded on the row.
pub const DOCUMENT_INGESTED: &str = "document.ingested";
/// Emitted by `documents.publish` when an output enters the public window.
pub const DOCUMENT_PUBLISHED: &str = "document.published";
/// Emitted by `documents.publish` when an output leaves it.
pub const DOCUMENT_UNPUBLISHED: &str = "document.unpublished";
/// Emitted by `documents.request_publish` — an author's signal to an admin.
pub const DOCUMENT_PUBLISH_REQUESTED: &str = "document.publish_requested";
/// Emitted by `documents.return` — the admin's rework order on a requested draft.
pub const DOCUMENT_RETURNED: &str = "document.returned";
/// The job kind that removes a tombstoned document's object.
pub const DOCUMENT_PURGE_KIND: &str = "document.purge";

/// The publish gate: a text output may not become public without its category's required
/// headings. Unknown categories and bodyless (object) documents are never blocked — the standard
/// cannot say what they require. The refusal names every missing heading so the console can offer
/// the triage, and it is a transition check: an already-published row is never re-checked.
pub(crate) fn require_complete(
    category: Option<&str>,
    body: Option<&str>,
) -> Result<(), ActionError> {
    let (Some(category), Some(body)) = (category, body) else {
        return Ok(());
    };
    let missing = conveyor_domain::output_standard::missing_sections(category, body);
    if missing.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "output_incomplete",
        format!(
            "the output is missing required sections: {}",
            missing.join(", ")
        ),
    )
    .with_errors(
        missing
            .into_iter()
            .map(|heading| FieldError {
                field: "body".to_owned(),
                code: "output_incomplete".to_owned(),
                detail: format!("missing required section: {heading}"),
            })
            .collect(),
    ))
}

/// Append one document event in the caller's transaction.
pub async fn emit_document_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &'static str,
    id: uuid::Uuid,
    payload: Value,
) -> Result<(), ActionError> {
    emit_document_event_call(conn, &ctx.call(), event_type, id, payload).await
}

/// Append one document event from the call attribution alone (an imported action has no
/// compiled spec to build a `HandlerCtx` from).
pub async fn emit_document_event_call(
    conn: &mut PgConnection,
    ctx: &crate::invoke::CallContext,
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
            tracing::error!(%error, event = event_type, "append document event");
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
