//! `/api/review-jobs/{job_id}` — the durable job read and cooperative cancel (`FND-006`).
//!
//! Ported from the reference's `api/routers/review_jobs.py` + `application/services/jobs.py`.
//! The two routes are top level, not under `/api/reviews/{review_id}`: a caller holding a job id
//! from a snapshot creation has the job id and nothing else. The job row carries its own
//! `review_id`, so the resource scoping is derived from the row rather than from the URL.
//!
//! The authorisation is the review's — the job's review lead — with one addition: the person who
//! submitted the job may read and cancel their own. Without that, a reviewer permitted to start an
//! import could not ask whether it had finished.
//!
//! `cancel_requested` is reported BESIDE `state`, never inside it: it is not a seventh public
//! state, and a client polling `state` always sees one of the six.
//!
//! ⛔ Nothing executes these jobs yet. No snapshot producer in this port claims its job row; a
//! `queued` job is durable, attributable and cancellable, not necessarily progressing.

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::actor::Actor;
use crate::db;
use crate::evidence_review::pool;
use crate::evidence_review::reviews::require_review_lead_for;
use crate::problem::Problem;
use crate::state::Shared;

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn iso_opt(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(iso)
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct JobRow {
    job_id: Uuid,
    tenant_id: Uuid,
    review_id: Uuid,
    job_type: String,
    state: String,
    command_hash: String,
    input_snapshot_hash: Option<String>,
    idempotency_key: Option<String>,
    originating_subject_id: Option<String>,
    attempt: i32,
    max_attempts: i32,
    progress: Value,
    cancel_requested_at: Option<DateTime<Utc>>,
    waiting_reason: Option<String>,
    waiting_dependency_ref: Option<String>,
    reconcile_after: Option<DateTime<Utc>>,
    result_summary: Option<Value>,
    result_artifact_ref: Option<String>,
    run_manifest_ref: Option<String>,
    error_code: Option<String>,
    error_detail: Option<String>,
    queued_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    terminal_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct AttemptRow {
    attempt_number: i32,
    worker_id: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    outcome: Option<String>,
    error_code: Option<String>,
    error_detail: Option<String>,
    resource_summary: Option<Value>,
}

/// The job, or a 404 that does not distinguish absent from another tenant's — a 403 would
/// confirm that a job with that id exists somewhere.
async fn load_job(tx: &mut Transaction<'_, Postgres>, job_id: Uuid) -> Result<JobRow, Problem> {
    sqlx::query_as::<_, JobRow>(
        "SELECT job_id, tenant_id, review_id, job_type, state, command_hash,
                input_snapshot_hash, idempotency_key, originating_subject_id, attempt,
                max_attempts, progress, cancel_requested_at, waiting_reason,
                waiting_dependency_ref, reconcile_after, result_summary, result_artifact_ref,
                run_manifest_ref, error_code, error_detail, queued_at, started_at, terminal_at
           FROM durable_jobs WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the job", error))?
    .ok_or_else(|| Problem::not_found(format!("job {job_id} was not found")))
}

async fn attempts(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
) -> Result<Vec<AttemptRow>, Problem> {
    sqlx::query_as::<_, AttemptRow>(
        "SELECT attempt_number, worker_id, started_at, ended_at, outcome, error_code,
                error_detail, resource_summary
           FROM job_attempts WHERE job_id = $1 ORDER BY attempt_number",
    )
    .bind(job_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the job's attempts", error))
}

/// Lead over the job's review, or the verified subject that submitted it.
async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    job: &JobRow,
) -> Result<(), Problem> {
    let subject = actor.subject.as_deref().map(str::trim).unwrap_or_default();
    if !subject.is_empty() && job.originating_subject_id.as_deref() == Some(subject) {
        return Ok(());
    }
    require_review_lead_for(tx, actor, job.review_id, "reading or cancelling this job")
        .await
        .map(|_| ())
        .map_err(|_| {
            Problem::forbidden(format!(
                "job {} belongs to a review you do not lead, and you did not submit it",
                job.job_id
            ))
        })
}

fn is_terminal(state: &str) -> bool {
    matches!(state, "succeeded" | "failed" | "cancelled")
}

fn render(job: &JobRow, attempts: &[AttemptRow]) -> Value {
    json!({
        "job_id": job.job_id.to_string(),
        "review_id": job.review_id.to_string(),
        "job_type": &job.job_type,
        "state": &job.state,
        "cancel_requested": job.cancel_requested_at.is_some(),
        "cancel_requested_at": iso_opt(job.cancel_requested_at),
        "attempt": job.attempt,
        "max_attempts": job.max_attempts,
        "command_hash": &job.command_hash,
        "input_snapshot_hash": &job.input_snapshot_hash,
        "idempotency_key": &job.idempotency_key,
        "progress": &job.progress,
        "waiting": if job.state != "waiting" {
            Value::Null
        } else {
            json!({
                "reason": &job.waiting_reason,
                "dependency_ref": &job.waiting_dependency_ref,
                "reconcile_after": iso_opt(job.reconcile_after),
            })
        },
        "result": {
            "summary": &job.result_summary,
            "artifact_ref": &job.result_artifact_ref,
            "run_manifest_ref": &job.run_manifest_ref,
            "error_code": &job.error_code,
            "error_detail": &job.error_detail,
        },
        "queued_at": iso(job.queued_at),
        "started_at": iso_opt(job.started_at),
        "terminal_at": iso_opt(job.terminal_at),
        // The attempt history is part of the read: the one question an operator always asks
        // about a job that took three tries is what happened on the first two.
        "attempts": attempts
            .iter()
            .map(|attempt| json!({
                "attempt_number": attempt.attempt_number,
                "worker_id": &attempt.worker_id,
                "started_at": iso(attempt.started_at),
                "ended_at": iso_opt(attempt.ended_at),
                "outcome": &attempt.outcome,
                "error_code": &attempt.error_code,
                "error_detail": &attempt.error_detail,
                "resource_summary": &attempt.resource_summary,
            }))
            .collect::<Vec<Value>>(),
    })
}

#[utoipa::path(get, path = "/api/review-jobs/{job_id}", operation_id = "get_review_job", responses((status = 200, description = "ok")))]
pub async fn get_review_job(
    State(state): State<Shared>,
    actor: Actor,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the job transaction", error))?;
    let job = load_job(&mut tx, job_id).await?;
    authorize(&mut tx, &actor, &job).await?;
    let attempts = attempts(&mut tx, job_id).await?;
    let value = render(&job, &attempts);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the job read", error))?;
    Ok(Json(value))
}

/// Ask for a job to stop. 200 whether or not it was still running: a terminal job answers with
/// its outcome rather than an error, so a client that cancels and one that polls read the same
/// shape.
#[utoipa::path(post, path = "/api/review-jobs/{job_id}:cancel", operation_id = "cancel_review_job", responses((status = 200, description = "ok")))]
pub async fn cancel_review_job(
    State(state): State<Shared>,
    actor: Actor,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the job transaction", error))?;
    let job = load_job(&mut tx, job_id).await?;
    authorize(&mut tx, &actor, &job).await?;

    let job = if is_terminal(&job.state) || job.cancel_requested_at.is_some() {
        // Nothing to do: the work stopped, or somebody already asked it to.
        job
    } else {
        // A `queued` job is flagged, never terminated here: `claim` refuses to start a flagged
        // job, and there must be exactly one writer of terminal state.
        sqlx::query(
            "UPDATE durable_jobs
                SET cancel_requested_at = now(), cancel_requested_by = $1
              WHERE job_id = $2",
        )
        .bind(actor.actor_subject())
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("requesting the job's cancellation", error))?;
        load_job(&mut tx, job_id).await?
    };
    let attempts = attempts(&mut tx, job_id).await?;
    let value = render(&job, &attempts);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the cancellation", error))?;
    Ok(Json(value))
}
