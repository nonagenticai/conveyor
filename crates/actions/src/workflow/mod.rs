#![forbid(unsafe_code)]
//! `workflows.*` — the S13 engine's authoring surface (one document, versioned by key).
//!
//! Six actions, one per step of the definition's life: create a draft, edit the draft, publish
//! it, retire it, read one, list them. The graph validator lives in `crates/workflow` and is
//! **not** called here: publishing a document the engine cannot execute is S13 step 5's gate
//! (`workflows.publish` calls the validator then, when the engine's publish path lands). Until
//! then this surface stores and freezes exactly what the caller sent — the 256 KiB ceiling, the
//! one-published-per-key rule and the fork counter are already the database's.
//!
//! The store is [`conveyor_store::workflow::workflow`]; nothing here speaks SQL. Every mutation
//! appends its outbox event in the same transaction (CONTRACT §7), and the four event names are
//! in [`crate::board::events::S05_EVENT_TYPES`] so a webhook can subscribe to them.

pub mod actions;
pub mod agent;
pub mod node;
pub mod runs;
pub mod trace;
pub mod views;

use conveyor_store as store;
use conveyor_store::workflow::WorkflowStoreError;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

/// Emitted by `workflows.create` (`{id, key, version}`).
pub const WORKFLOW_CREATED: &str = "workflow.created";
/// Emitted by `workflows.update` (`{id, key, version}`).
pub const WORKFLOW_UPDATED: &str = "workflow.updated";
/// Emitted by `workflows.publish` (`{id, key, version, revision_id, content_hash}`).
pub const WORKFLOW_PUBLISHED: &str = "workflow.published";
/// Emitted by `workflows.retire` (`{id, key}`).
pub const WORKFLOW_RETIRED: &str = "workflow.retired";

/// A 422 on one field.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Append one workflow event in the caller's transaction.
pub async fn emit_workflow_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &'static str,
    id: Uuid,
    payload: Value,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("workflow:{id}"),
        event_type: event_type.to_owned(),
        aggregate_type: "workflow".to_owned(),
        aggregate_id: id,
        payload,
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, event = event_type, action = ctx.action, "append workflow event");
            ActionError::internal()
        })?;
    Ok(())
}

/// A workflow store refusal as the status the caller can act on.
///
/// A lifecycle refusal is a `409` — the caller's read is stale, not its syntax. A document over
/// the ceiling is a `422` on `definition`, because the caller can fix it by sending fewer bytes.
pub fn workflow_error(error: WorkflowStoreError, action: &str) -> ActionError {
    use WorkflowStoreError as E;
    match error {
        E::NotFound => ActionError::resource_not_found("such workflow"),
        E::NotDraft(status) => ActionError::new(
            409,
            "conflict",
            format!("{action}: the workflow is {status}, not a draft"),
        ),
        E::NotPublished => ActionError::new(
            409,
            "conflict",
            format!("{action}: the workflow is not published"),
        ),
        E::VersionMismatch { current } => ActionError::new(
            409,
            "conflict",
            format!(
                "{action}: the workflow moved to version {} while this call was in flight",
                current.get()
            ),
        ),
        E::DefinitionTooLarge { bytes } => invalid(
            "definition",
            "too_large",
            format!("the definition is {bytes} bytes; the ceiling is 262144"),
        ),
        E::Conflict { constraint } => ActionError::new(
            409,
            "conflict",
            format!("{action}: a conflicting workflow row already exists ({constraint})"),
        ),
        E::Invalid { constraint } => invalid(
            "definition",
            "invalid_value",
            format!("the database refused the workflow value ({constraint})"),
        ),
        E::Database(error) => {
            tracing::error!(%error, action, "workflow store failure");
            ActionError::internal()
        }
    }
}
