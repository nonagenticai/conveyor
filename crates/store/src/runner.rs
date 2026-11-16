//! The runner's attempt ledger: one row per Kubernetes Job, fenced on the job's own attempt.
//!
//! `runner_run` is deliberately NOT a second queue. The `job` row (0011_platform.sql:120) stays
//! the unit of work and the only thing a worker leases; a run row records what the dispatch did
//! with one attempt at that job — the image, the namespace and Job name, the pod's progress, and
//! the result it posted. `(job_id, attempt)` is the idempotency key (ADR 0007 §Decision 4).
//!
//! **The attempt number is the job's, not a counter of our own.** A dispatch reads
//! `job.attempts` at claim time and writes it here; every pod-side write is fenced on
//! `job.attempts = runner_run.attempt` plus a live attempt — `LIVE_ATTEMPT` below, which accepts
//! the worker's claim (`running`, lease live) and the deferred gap between two polls (`pending`,
//! `continuation`). An attempt whose pod arrives after the row was reclaimed bumps `job.attempts`
//! and therefore updates zero rows — the stale-attempt test in `crates/store/tests/runner.rs` is
//! the acceptance for that fence.
//!
//! Bytes never live here. An artefact row names an object conveyor wrote itself from the bytes it
//! received; no presigned URL and no store credential exists anywhere in this path (ADR 0007
//! §Decision 4).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::scope::begin_scoped;

/// One `runner_run` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Run {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub attempt: i32,
    pub image: String,
    pub namespace: String,
    pub job_name: String,
    /// The `runner/v1` instruction this attempt was dispatched with.
    pub instruction: Value,
    pub status: String,
    pub progress: Value,
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl Run {
    /// The verdicts after which the row is never written again.
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "succeeded" | "failed" | "lost")
    }
}

/// What a dispatch records when it creates the Kubernetes Job.
#[derive(Clone, Debug)]
pub struct NewRun<'a> {
    pub job_id: Uuid,
    pub attempt: i32,
    pub image: &'a str,
    pub namespace: &'a str,
    pub job_name: &'a str,
    /// The instruction the pod is dispatched with, kept for the routes' validation.
    pub instruction: Value,
}

/// One `runner_artefact` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ArtefactRow {
    pub name: String,
    pub sha256: String,
    pub bytes: i64,
    pub content_type: String,
    pub object_key: String,
}

/// What an artefact insert did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Inserted {
    /// A new row for a name that had none.
    New,
    /// The same name and the same sha256 arrived again — an idempotent retry.
    Repeated,
    /// The name exists with different bytes. Refused, never overwritten.
    Conflict,
}

/// Record a new attempt. `(job_id, attempt)` is unique, so a second dispatch of the same attempt
/// returns `None` instead of overwriting the first.
pub async fn begin(
    pool: &PgPool,
    tenant: TenantId,
    new: &NewRun<'_>,
) -> Result<Option<Run>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let run: Option<Run> = sqlx::query_as(
        "INSERT INTO runner_run (tenant_id, job_id, attempt, image, namespace, job_name, instruction)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (job_id, attempt) DO NOTHING
         RETURNING *",
    )
    .bind(tenant.as_uuid())
    .bind(new.job_id)
    .bind(new.attempt)
    .bind(new.image)
    .bind(new.namespace)
    .bind(new.job_name)
    .bind(&new.instruction)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(run)
}

/// The attempt row for `(job_id, attempt)`, if this dispatch ever created one.
pub async fn attempt(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    attempt: i32,
) -> Result<Option<Run>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: Option<Run> =
        sqlx::query_as("SELECT * FROM runner_run WHERE job_id = $1 AND attempt = $2")
            .bind(job_id)
            .bind(attempt)
            .fetch_optional(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(row)
}

/// The latest attempt row for a job, whatever its state — what a poll started without knowing its
/// attempt number should read.
pub async fn latest(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
) -> Result<Option<Run>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: Option<Run> =
        sqlx::query_as("SELECT * FROM runner_run WHERE job_id = $1 ORDER BY attempt DESC LIMIT 1")
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(row)
}

/// The attempt is live when the worker holds the claim, OR when it is between two polls.
///
/// A dispatched pod outlives the worker's claim: `store::jobs::defer` leaves the row `pending`
/// with the lease cleared so the next poll can re-claim it without spending an attempt. Fencing a
/// pod-side write on `status = 'running'` therefore refuses almost every one — measured live
/// 2026-11-16, on the first real dispatch: `POST /result` answered 403, and once the API-side
/// fence was corrected the same write answered 409 because these statements had not moved with
/// it. `conveyor_api::job_routes::fence` states the same rule; the two must agree.
const LIVE_ATTEMPT: &str = "EXISTS (SELECT 1 FROM job j
                         WHERE j.id = r.job_id AND j.attempts = r.attempt
                           AND ((j.status = 'running' AND j.lease_until > now())
                             OR (j.status = 'pending' AND j.continuation)))";

/// Mark the pod as observed running. Fenced like every pod-side write.
pub async fn mark_running(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    attempt: i32,
) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let sql = format!(
        "UPDATE runner_run r
            SET status = 'running', started_at = coalesce(r.started_at, now())
          WHERE r.job_id = $1 AND r.attempt = $2 AND r.status = 'dispatched'
            AND {LIVE_ATTEMPT}"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(job_id)
        .bind(attempt)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Record the pod's progress. Fenced on the live attempt and the live job, and refused once the
/// result has been posted (a late progress write must not muddy a finished result).
pub async fn record_progress(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    attempt: i32,
    progress: &Value,
) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let sql = format!(
        "UPDATE runner_run r
            SET progress = $3
          WHERE r.job_id = $1 AND r.attempt = $2
            AND r.status IN ('dispatched', 'running')
            AND {LIVE_ATTEMPT}"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(job_id)
        .bind(attempt)
        .bind(progress)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Store the pod's result on the attempt. This is the fence the stale-attempt test measures:
/// a pod whose `attempt` no longer equals `job.attempts`, or whose job has already stopped
/// running, writes nothing.
pub async fn record_result(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    attempt: i32,
    result: &Value,
) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let sql = format!(
        "UPDATE runner_run r
            SET status = 'reported', result = $3
          WHERE r.job_id = $1 AND r.attempt = $2
            AND r.status IN ('dispatched', 'running')
            AND {LIVE_ATTEMPT}"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(job_id)
        .bind(attempt)
        .bind(result)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Insert one artefact, idempotently on `(run_id, name)`.
///
/// A repeated upload of the same bytes is a no-op that reports [`Inserted::Repeated`]; the same
/// name with different bytes is refused rather than overwritten, because a result that names one
/// hash must not be able to ship another.
pub async fn insert_artefact(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    artefact: &ArtefactRow,
) -> Result<Inserted, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row = sqlx::query(
        "INSERT INTO runner_artefact (tenant_id, run_id, name, sha256, bytes, content_type, object_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (run_id, name) DO UPDATE
            SET bytes = runner_artefact.bytes
          WHERE runner_artefact.sha256 = EXCLUDED.sha256
        RETURNING (xmax = 0) AS fresh",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(&artefact.name)
    .bind(&artefact.sha256)
    .bind(artefact.bytes)
    .bind(&artefact.content_type)
    .bind(&artefact.object_key)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(match row {
        None => Inserted::Conflict,
        Some(row) => {
            if row.get::<bool, _>("fresh") {
                Inserted::New
            } else {
                Inserted::Repeated
            }
        }
    })
}

/// Every artefact recorded for an attempt, by name.
pub async fn artefacts(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<ArtefactRow>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows: Vec<ArtefactRow> = sqlx::query_as(
        "SELECT name, sha256, bytes, content_type, object_key
           FROM runner_artefact WHERE run_id = $1 ORDER BY name",
    )
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}

/// Write a verdict on an attempt. Only a non-terminal row moves, so two finishers cannot both
/// land and a late writer after [`finish`] changes nothing.
pub async fn finish(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    status: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows = sqlx::query(
        "UPDATE runner_run
            SET status = $2, finished_at = now()
          WHERE id = $1 AND status IN ('dispatched', 'running', 'reported')",
    )
    .bind(run_id)
    .bind(status)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Live attempts created before `cutoff` — what the reaper sweeps.
pub async fn stale_before(
    pool: &PgPool,
    tenant: TenantId,
    cutoff: DateTime<Utc>,
) -> Result<Vec<Run>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows: Vec<Run> = sqlx::query_as(
        "SELECT * FROM runner_run
          WHERE status IN ('dispatched', 'running', 'reported') AND created_at < $1
          ORDER BY created_at",
    )
    .bind(cutoff)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}
