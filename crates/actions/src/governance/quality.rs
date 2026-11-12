#![forbid(unsafe_code)]
//! `quality.score` — advisory scoring of an immutable revision (S07 §5.5).
//!
//! The action does not score: it loads the revision, refuses anything that is not a
//! governed-asset revision, and enqueues a `quality.score` job **on the caller's
//! transaction**. The rubric runs in the worker, after the caller committed the revision it
//! is scoring — scoring a row that could still roll back would evaluate a document that
//! never existed.
//!
//! Nothing here writes `quality_evaluation` and nothing touches `governed_asset`: the score
//! is appended by the job and is advisory, so it can never change a status or block a
//! transition. Re-invoking while a job is still live reuses it (the dedupe key is
//! `revision:<id>`) and reports `queued: true` with no new `job_id`.

use conveyor_store as store;
use conveyor_store::quality::QualityStoreError;
use conveyor_store::revision::RevisionStoreError;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{QualityScoreIn, QualityScoreOut, store_error};

/// A revision read failure: a database error is the caller's `500`, anything else is a
/// stored value the domain no longer knows (a bug, not the caller's input).
fn revision_error(error: RevisionStoreError, action: &str) -> ActionError {
    match error {
        RevisionStoreError::Database(error) => store_error(error, action),
        other => {
            tracing::error!(%other, action, "revision read failed");
            ActionError::internal()
        }
    }
}

/// A quality store failure. Enqueueing can only fail with a database error; the verdict
/// variants belong to the conflict surface and are unreachable here.
fn quality_error(error: QualityStoreError, action: &str) -> ActionError {
    match error {
        QualityStoreError::Database(error) => store_error(error, action),
        other => {
            tracing::error!(%other, action, "quality store failed");
            ActionError::internal()
        }
    }
}

/// Queue a rubric evaluation of one governed-asset revision.
///
/// A missing revision and a revision of any other entity type are both a `404`: S07 scores
/// governed assets only, and a `model` revision (S08) is out of this slice rather than a bad
/// request the caller can fix.
///
/// The response reports the outcome honestly: `201` when this call created the job,
/// `200` when an identical job was already pending or running (`job_id` is then absent,
/// because the live job is not this call's row). `queued` is true in both cases — the work
/// is on the queue.
#[crate::action(
    name = "quality.score",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn quality_score(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: QualityScoreIn,
) -> Result<HandlerOutput<QualityScoreOut>, ActionError> {
    let action = "quality.score";
    let revision = store::revision::get_revision_tx(&mut *tx, ctx.tenant, input.revision_id)
        .await
        .map_err(|error| revision_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("revision"))?;
    if revision.entity_type != store::governance::GOVERNED_ASSET_ENTITY_TYPE {
        return Err(ActionError::new(
            404,
            "not_found",
            format!(
                "revision {} belongs to entity type {:?}; only a {} revision can be scored",
                revision.id,
                revision.entity_type,
                store::governance::GOVERNED_ASSET_ENTITY_TYPE,
            ),
        ));
    }

    let job_id = store::quality::enqueue_quality_score_tx(&mut *tx, ctx.tenant, revision.id)
        .await
        .map_err(|error| quality_error(error, action))?;

    let out = QualityScoreOut {
        revision_id: revision.id.to_string(),
        job_id: job_id.map(|id| id.to_string()),
        queued: true,
    };
    Ok(if job_id.is_some() {
        HandlerOutput::created(out)
    } else {
        HandlerOutput::ok(out)
    })
}
