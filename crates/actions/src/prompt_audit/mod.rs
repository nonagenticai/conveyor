#![forbid(unsafe_code)]
//! The prompt-audit surface (S16): `prompt_audits.*` and `prompt_findings.*`.
//!
//! The audit inventories the prompt surfaces the conveyor owns, classifies them against the
//! `claude-api` prompt-audit method's four anti-pattern groups, and reports findings with
//! confidence and a proposed diff. Two paths share one inventory and one finding vocabulary:
//!
//! - [`audits::prompt_audits_check`] is **deterministic and synchronous** — no model call, nothing
//!   persisted — and its `high`-confidence findings can refuse a publish (S16 CCR-2).
//! - `prompt_audit.run` (the worker arm) makes ONE bounded `completions.create` with the versioned
//!   built-in prompt [`PROMPT_AUDIT_V1`] and persists its findings under an audit row.
//!
//! **Findings are inert without a person:** `open → accepted | rejected` is the whole decision
//! surface here; `accepted → applied` belongs to an ordinary `processes.update`. The audit never
//! holds `processes:write` over the surface it audits (CONTRACT §18).

pub mod audits;
pub mod findings;
pub mod scan;
mod views;

pub use views::{
    AuditScopeIn, CheckFindingView, FindingView, PromptAuditCheckIn, PromptAuditCheckOut,
    PromptAuditGetIn, PromptAuditGetOut, PromptAuditListIn, PromptAuditStartIn, PromptAuditView,
    PromptFindingDecideIn, PromptFindingListIn,
};

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::prompt_audit::{FindingRow, PromptAuditRow, PromptAuditStoreError};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

/// The copilot's system prompt. The text lives in `conveyor-store` (SPEC D27: it is the
/// `copilot_system@1` seed), so the audit and the copilot read one copy; `api::copilot::turns`
/// re-exports this constant and resolves the catalogue row at run time.
pub use conveyor_store::prompts::COPILOT_SYSTEM_V1 as COPILOT_SYSTEM_PROMPT;

/// The built-in audit prompt, versioned (`prompt_audit@1`). Changing it means `prompt_audit@2`;
/// an audit row names the version that produced its findings, and rewriting v1 would make every
/// past finding unreproducible. The text lives in `conveyor-store` because it is seed material.
pub use conveyor_store::prompts::PROMPT_AUDIT_V1;

/// A 422 with one field error.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Map a store failure onto the action surface.
pub(crate) fn store_error(error: PromptAuditStoreError, action: &str) -> ActionError {
    match error {
        PromptAuditStoreError::NotFound => ActionError::resource_not_found("prompt_audit"),
        PromptAuditStoreError::Illegal(detail) => {
            ActionError::new(409, "prompt_audit.illegal_transition", detail)
        }
        PromptAuditStoreError::AlreadyDecided(status) => ActionError::new(
            409,
            "prompt_finding.already_decided",
            format!("the finding is already {status}"),
        ),
        PromptAuditStoreError::Invalid(inner) => {
            ActionError::new(422, "prompt_finding.invalid", inner.to_string())
        }
        PromptAuditStoreError::Database(inner) => {
            tracing::error!(%inner, action, "prompt audit store");
            ActionError::internal()
        }
    }
}

/// Row → view.
pub(crate) fn audit_view(row: PromptAuditRow) -> PromptAuditView {
    PromptAuditView {
        id: row.id,
        scope: row.scope,
        target_model: row.target_model,
        prompt_ref: row.prompt_ref,
        status: row.status,
        finding_count: row.finding_count,
        error: row.error,
        requested_by: row.requested_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Row → view.
pub(crate) fn finding_view(row: FindingRow) -> FindingView {
    FindingView {
        id: row.id,
        audit_id: row.audit_id,
        location: row.location,
        evidence: row.evidence,
        pattern: row.pattern,
        why_obsolete: row.why_obsolete,
        confidence: row.confidence,
        action: row.action,
        replacement: row.replacement,
        status: row.status,
        decided_by: row.decided_by,
        decided_at: row.decided_at,
        created_at: row.created_at,
    }
}

/// Encode a `(created_at, id)` keyset cursor.
pub(crate) fn cursor_of(created_at: DateTime<Utc>, id: Uuid) -> String {
    serde_json::to_string(&(created_at, id)).unwrap_or_else(|_| String::from("null"))
}

/// Decode a `(created_at, id)` keyset cursor.
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
                "the cursor the last page returned",
            )
        })
}

/// Append an audit event in the caller's transaction.
pub(crate) async fn emit_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &str,
    audit_id: Uuid,
    payload: serde_json::Value,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("prompt_audit:{audit_id}"),
        event_type: event_type.to_owned(),
        aggregate_type: "prompt_audit".to_owned(),
        aggregate_id: audit_id,
        payload,
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, event_type, action = ctx.action, "append the prompt-audit event");
            ActionError::internal()
        })?;
    Ok(())
}
