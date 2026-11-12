#![forbid(unsafe_code)]
//! The procedure (SOP) surface: `procedures.create` · `.get` · `.list` · `.update` · `.publish` ·
//! `.retire`.
//!
//! A **procedure** is the judge contract a process step is measured against (CONTRACT §21, SPEC
//! D28): the definition of ready, the definition of done, the acceptance criteria, the inspection
//! and the declared outputs. It is *not* the sequence — that is a process (`crate::process`) — and a
//! step names the SOP it is judged against by `procedure_id`.
//!
//! The version family is the same one a process uses (S11 §5.3): rows share `(tenant_id, key)`, each
//! with a `version` and a `status`; `create` is the only way a version appears and it always produces
//! a **draft**; `update` edits a draft; `publish` validates and freezes the version, retiring the
//! key's previous published row in the same transaction.
//!
//! **Publishing is where the judge is certified.** `conveyor_domain::procedure::validate_publish`
//! refuses an empty `inspection`, an empty `acceptance_criteria`, an out-of-range eval threshold, an
//! over-cap reject budget and a malformed output path — as a `422 procedure.inspection` that names
//! every finding, never as a run whose judged step silently accepts. Nothing is written until the
//! draft passes.
//!
//! **Slugs.** This family's refusals are dotted (`procedure.draft_exists`, `procedure.not_draft`,
//! `procedure.not_published`, `procedure.not_found`, `procedure.version_mismatch`,
//! `procedure.inspection`) because the slice's acceptance criteria name them.

use chrono::{DateTime, Utc};
use conveyor_domain::procedure::{SopError, validate_publish};
use conveyor_store as store;
use conveyor_store::procedure::{NewSop, SopPatch, SopRecord, SopStoreError};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

pub mod procedures;
pub mod views;

pub use views::*;

/// The event a successful publish appends.
pub(crate) const PROCEDURE_PUBLISHED: &str = "procedure.published";
/// The event a successful retire appends.
pub(crate) const PROCEDURE_RETIRED: &str = "procedure.retired";

/// One procedure as every read returns it.
pub(crate) fn procedure_out(record: &SopRecord) -> ProcedureOut {
    ProcedureOut {
        id: record.id,
        key: record.key.clone(),
        title: record.title.clone(),
        description: record.description.clone(),
        status: record.status.clone(),
        version: record.version,
        lock_version: record.lock_version,
        definition_of_ready: record.definition_of_ready.clone(),
        definition_of_done: record.definition_of_done.clone(),
        acceptance_criteria: record.acceptance_criteria.clone(),
        inspection: record.inspection.clone(),
        outputs: record.outputs.clone(),
    }
}

/// A `422` on one field of the caller's document.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// A row that does not exist in the caller's tenant.
pub(crate) fn not_found() -> ActionError {
    ActionError::new(404, "procedure.not_found", "no procedure in this tenant")
}

/// Parse a `procedure.status` from the wire.
pub(crate) fn parse_status(raw: &str) -> Result<&'static str, ActionError> {
    match raw {
        "draft" => Ok("draft"),
        "published" => Ok("published"),
        "retired" => Ok("retired"),
        _ => Err(invalid(
            "status",
            "invalid_value",
            "must be one of draft, published, retired",
        )),
    }
}

/// A procedure-store failure.
pub(crate) fn procedure_error(error: SopStoreError, action: &str) -> ActionError {
    match error {
        SopStoreError::NotFound => not_found(),
        SopStoreError::DraftExists => ActionError::new(
            409,
            "procedure.draft_exists",
            "this key already has a draft; edit it, or publish it before creating another version",
        ),
        SopStoreError::NotDraft(status) => ActionError::new(
            409,
            "procedure.not_draft",
            format!("the procedure is {status}, not a draft"),
        ),
        SopStoreError::NotPublished => ActionError::new(
            409,
            "procedure.not_published",
            "only a published procedure can be retired",
        ),
        // The `update` handler answers this variant itself, with the representation the caller
        // should now be looking at; this arm is the honest answer if any other path ever sees one.
        SopStoreError::VersionMismatch { current } => ActionError::new(
            412,
            "procedure.version_mismatch",
            format!("the procedure moved to version {current} before this call"),
        ),
        SopStoreError::Database(error) => {
            tracing::error!(%error, action, "procedure store failure");
            ActionError::internal()
        }
    }
}

/// A publish-time refusal: the draft is well-formed JSON but is not a runnable judge.
///
/// Every finding is reported at once — fixing a draft one publish at a time is the slow path — and
/// each names the member to edit, so a client does not have to parse the prose.
pub(crate) fn inspection_refusal(errors: &[SopError]) -> ActionError {
    let detail = errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    ActionError::new(422, "procedure.inspection", detail).with_errors(
        errors
            .iter()
            .map(|error| FieldError {
                field: inspection_field(error).to_owned(),
                code: "inspection".to_owned(),
                detail: error.to_string(),
            })
            .collect(),
    )
}

/// The member a publish refusal asks the caller to edit.
fn inspection_field(error: &SopError) -> &'static str {
    match error {
        SopError::BadInspection(_) | SopError::BadThreshold | SopError::BadMaxRejects => {
            "inspection"
        }
        SopError::NoAcceptanceCriteria | SopError::BadCriteria => "acceptance_criteria",
        SopError::BadOutput(_) => "outputs",
    }
}

/// Validate a draft's judge contract before it is published.
pub(crate) fn validated(record: &SopRecord) -> Result<(), ActionError> {
    validate_publish(
        &record.acceptance_criteria,
        &record.inspection,
        &record.outputs,
    )
    .map(|_| ())
    .map_err(|errors| inspection_refusal(&errors))
}

/// The cursor a `procedures.list` page hands back: `["<created_at>", "<id>"]`.
///
/// The store's sort key is `(created_at DESC, id DESC)`, so the pair — not the timestamp alone — is
/// what makes the cursor stable when two rows share a millisecond.
pub(crate) fn cursor_of(record: &SopRecord) -> String {
    serde_json::to_string(&(record.created_at, record.id)).unwrap_or_else(|_| String::from("[]"))
}

/// Decode a `procedures.list` cursor.
pub(crate) fn decode_cursor(
    cursor: Option<&str>,
) -> Result<Option<(DateTime<Utc>, Uuid)>, ActionError> {
    let Some(raw) = cursor else {
        return Ok(None);
    };
    serde_json::from_str::<(DateTime<Utc>, Uuid)>(raw)
        .map(Some)
        .map_err(|_| {
            invalid(
                "cursor",
                "invalid_cursor",
                "a procedures cursor is the [created_at, id] pair the last page returned",
            )
        })
}

/// Append one procedure event in the caller's transaction.
pub(crate) async fn emit_procedure_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &str,
    record: &SopRecord,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("procedure:{}", record.id),
        event_type: event_type.to_owned(),
        aggregate_type: "procedure".to_owned(),
        aggregate_id: record.id,
        payload: serde_json::json!({
            "id": record.id,
            "key": record.key,
            "version": record.version,
        }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            // The outbox insert shares the mutation's transaction, so this rolls the change back: a
            // procedure row is never written unannounced.
            tracing::error!(%error, event_type, action = ctx.action, "append the procedure event");
            ActionError::internal()
        })?;
    Ok(())
}

/// Build the patch an edit applies from the members the caller actually named.
pub(crate) fn patch_of(input: &ProcedureUpdateIn) -> SopPatch {
    SopPatch {
        title: input.title.as_deref().map(str::trim).map(str::to_owned),
        description: input.description.clone(),
        definition_of_ready: input.definition_of_ready.clone(),
        definition_of_done: input.definition_of_done.clone(),
        acceptance_criteria: input.acceptance_criteria.clone(),
        inspection: input.inspection.clone(),
        outputs: input.outputs.clone(),
    }
}

/// The defaults a `create` gives the members the caller omitted (the column defaults).
pub(crate) fn new_sop(input: ProcedureCreateIn) -> NewSop {
    NewSop {
        key: input.key.trim().to_owned(),
        title: input.title.trim().to_owned(),
        description: input.description.unwrap_or_default(),
        definition_of_ready: input
            .definition_of_ready
            .unwrap_or_else(|| serde_json::json!({})),
        definition_of_done: input
            .definition_of_done
            .unwrap_or_else(|| serde_json::json!({})),
        acceptance_criteria: input
            .acceptance_criteria
            .unwrap_or_else(|| serde_json::json!([])),
        inspection: input.inspection.unwrap_or_else(|| serde_json::json!({})),
        outputs: input.outputs.unwrap_or_else(|| serde_json::json!([])),
    }
}
