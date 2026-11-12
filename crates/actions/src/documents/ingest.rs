#![forbid(unsafe_code)]
//! `documents.ingest` — hand one stored document to `strata_ingest` (S09 §5.6).
//!
//! The action owns three things the generic external executor could not: it reads the bytes from the object
//! store, it resolves the **tenant's own** upstream token, and it writes the returned job id back onto the
//! document. Everything else — the timeout, the audit row, the RFC 9457 rendering — is the engine's.
//!
//! Order matters and is deliberate:
//!
//! 1. **Token first.** A tenant with no `strata_ingest.token` secret is `422 ingest_not_configured` before a
//!    single byte is read or a single request is made. A shared or default token would push one tenant's
//!    document into another tenant's upstream account, which is the one failure this action must make
//!    impossible rather than unlikely.
//! 2. **Already ingested is a `409`.** The upstream job id is not replaceable by a second push — two pushes
//!    would produce two documents upstream and a document row pointing at one of them.
//! 3. **Bytes come from `fetch_bytes`**, the same hash-verified path `documents.download` uses, so a corrupted
//!    object is refused here for the same reason it is refused there.
//! 4. The push happens inside the caller's transaction. That is a deliberate cost: the alternative (commit,
//!    call, then a second transaction) has no seam in the handler API, and the client's own timeout bounds the
//!    hold. Written down so the next reader does not discover it as a surprise under load.

use bytes::Bytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::actions::{DocumentOut, fetch_bytes};
use super::{DOCUMENT_INGESTED, document_error, emit_document_event, secret_error};
use crate::integrations::ingest::{Ingest, IngestError, IngestRequest};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The secret name that holds a tenant's `strata_ingest` bearer token.
///
/// ⚠️ The upstream's token **is** its tenant (`STRATA_INGEST_API_TOKENS`), so this secret names a credential
/// that must belong to the same tenant as the caller — there is no shared token and no default.
pub const INGEST_TOKEN_SECRET: &str = "strata_ingest.token";

/// One document to hand to `strata_ingest`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentIngestIn {
    /// The document whose bytes are pushed.
    pub id: Uuid,
    /// The **upstream** store the document lands in — a leaf of this tenant in strata.
    ///
    /// The conveyor has no notion of a strata store, so this cannot be defaulted or discovered locally; the
    /// upstream validates it as a leaf of the caller's tenant and answers `400`/`404` otherwise.
    pub store_id: Uuid,
    /// The upstream's own source identifier, when the caller keeps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// The source run; only legal together with `source_id` (the upstream rejects it alone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run: Option<String>,
    /// Options forwarded to the upstream verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
}

/// Hand one document to `strata_ingest`.
#[crate::action(
    name = "documents.ingest",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_ingest(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentIngestIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.ingest";

    // 1. The tenant's own token, or a refusal that names the missing configuration.
    let token = match conveyor_store::secret::open_named_tx(
        &mut *tx,
        ctx.tenant,
        INGEST_TOKEN_SECRET,
    )
    .await
    {
        Ok(bytes) => {
            // A token that is not legal in an HTTP header would fail deep inside the client and surface as
            // "the ingest service could not be reached" — a network diagnosis for a credential defect. Refuse
            // it here, where the cause is the one the caller can fix.
            let token = String::from_utf8(bytes).map_err(|_| {
                ActionError::new(
                    422,
                    "ingest_not_configured",
                    format!(
                        "{action}: this tenant's {INGEST_TOKEN_SECRET} is not a UTF-8 bearer token"
                    ),
                )
            })?;
            let token = token.trim().to_owned();
            if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
                return Err(ActionError::new(
                    422,
                    "ingest_not_configured",
                    format!(
                        "{action}: this tenant's {INGEST_TOKEN_SECRET} is not a legal bearer token \
                         (it must be printable ASCII with no whitespace)"
                    ),
                ));
            }
            token
        }
        Err(conveyor_store::secret::SecretError::NotFound) => {
            return Err(ActionError::new(
                422,
                "ingest_not_configured",
                format!(
                    "{action}: this tenant has no {INGEST_TOKEN_SECRET} secret; create one before ingesting"
                ),
            ));
        }
        Err(error) => return Err(secret_error(error, action)),
    };

    // 2. Refuse a second push: the job id is a fact about the document, not a counter.
    //
    // ⚠️ The read takes a **row lock** (`FOR UPDATE`). The check and the write of `external_ref` are separated
    // by the upstream call, which takes seconds; without the lock a second concurrent ingest would pass this
    // same check and push the document twice, which is exactly the outcome this action exists to prevent.
    let record = conveyor_store::document::get_document_locked_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    if let Some(existing) = record.external_ref.as_deref() {
        return Err(ActionError::new(
            409,
            "conflict",
            format!("{action}: the document is already ingested as {existing}"),
        ));
    }

    // 3. The bytes, hash-verified by the same path the download uses.
    let loaded = fetch_bytes(&mut *tx, ctx.tenant, input.id, action).await?;

    // 4. The client. Absent configuration is a 503 — the service cannot do this right now.
    let ingest = Ingest::from_env().ok_or_else(|| {
        ActionError::new(
            503,
            "ingest_unavailable",
            format!("{action}: CONVEYOR_STRATA_INGEST_URL is not configured"),
        )
    })?;

    // 5. Push.
    //
    // ⚠️ **The known at-least-once window.** The upstream's 202 arrives before this transaction commits, so a
    // commit failure, a cancellation or a statement timeout after the 202 leaves an upstream job that this row
    // does not name; the caller retries and pushes again. That cannot be closed from inside one transaction —
    // the alternatives (a push job outside the tx, or a two-phase claim) are a later slice. What is done here
    // is to make the retry land on the SAME upstream source rather than a second one: a caller that supplies
    // no `source_id` gets a deterministic `conveyor:<document id>`, which strata records as the document's
    // provenance.
    let source_id = input
        .source_id
        .or_else(|| Some(format!("conveyor:{}", input.id)));
    let request = IngestRequest {
        store_id: input.store_id,
        source_id,
        source_run: input.source_run,
        options: input.options,
        filename: filename_for(&record.title),
        content_type: loaded.content_type.clone(),
        bytes: Bytes::from(loaded.bytes),
    };
    let accepted = match ingest.submit(&token, request).await {
        Ok(accepted) => accepted,
        Err(error) => return Err(ingest_action_error(error, action)),
    };

    // 6. The job id becomes a column, in this transaction.
    let reference = accepted.job_id.to_string();
    let updated =
        conveyor_store::document::set_external_ref_tx(&mut *tx, ctx.tenant, input.id, &reference)
            .await
            .map_err(|error| document_error(error, action))?;

    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_INGESTED,
        input.id,
        json!({
            "id": input.id.to_string(),
            "external_ref": reference,
            "job_status": accepted.status,
            "store_id": input.store_id.to_string(),
        }),
    )
    .await?;

    Ok(HandlerOutput::ok(DocumentOut::from(updated)))
}

/// The file name the upstream records: the document's title, with anything a path would choke on removed.
fn filename_for(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '.' | '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches([' ', '.']).trim();
    if trimmed.is_empty() {
        "document".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The upstream's answer as the status the caller can act on.
fn ingest_action_error(error: IngestError, action: &str) -> ActionError {
    use IngestError as E;
    match error {
        E::Unavailable => ActionError::new(
            503,
            "ingest_unavailable",
            format!("{action}: the ingest service is not configured"),
        ),
        E::Unauthorized => ActionError::new(
            502,
            "ingest_upstream_failed",
            format!("{action}: the ingest service rejected this tenant's token"),
        ),
        E::NotReady => ActionError::new(
            503,
            "ingest_unavailable",
            format!("{action}: the ingest service is not ready; retry later"),
        ),
        E::Rejected { status, body } if status < 500 => ActionError::new(
            422,
            "ingest_rejected",
            format!("{action}: the ingest service refused the document ({status}): {body}"),
        ),
        E::Rejected { status, body } => ActionError::new(
            502,
            "ingest_upstream_failed",
            format!("{action}: the ingest service failed ({status}): {body}"),
        ),
        E::Protocol => ActionError::new(
            502,
            "ingest_upstream_failed",
            format!("{action}: the ingest service answered a body this client cannot read"),
        ),
        E::Transport(message) => ActionError::new(
            503,
            "ingest_unavailable",
            format!("{action}: the ingest service could not be reached: {message}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_becomes_a_safe_file_name() {
        assert_eq!(filename_for("Q3 report.pdf"), "Q3 report.pdf");
        assert_eq!(filename_for("   "), "document");

        // The property that matters: a traversal attempt cannot survive as a path. Asserting one exact string
        // here would pin the sanitiser's edge behaviour (leading dots are trimmed, so the literal is
        // `_.._etc_passwd`) to a test that exists to prove no separator survives.
        let hostile = filename_for("../../etc/passwd");
        assert!(!hostile.contains('/'), "{hostile}");
        assert!(!hostile.starts_with('.'), "{hostile}");
        assert!(hostile.ends_with("etc_passwd"), "{hostile}");
        assert!(!filename_for("..").is_empty());
    }
}
