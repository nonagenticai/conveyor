#![forbid(unsafe_code)]
//! The judge/park reads and writes the step executor needs, kept out of `process` so the run-tree
//! and version-family code stay one subject each.
//!
//! The parking protocol:
//!
//! 1. the action succeeded → its output is stored while the step is still `running`, and
//!    `run_step.judge` is set to a **park** document: `{"decision":"pending","grade_id":…}` while
//!    an eval grades the output, `{"decision":"unavailable",…}` while the judge is down. The row
//!    is now a **stored output awaiting a judge**, not an action to re-invoke;
//! 2. a parked job re-enters the same attempt, re-reads the STORED output and asks the judge
//!    again — it never calls the action a second time (D12);
//! 3. accept/reject writes the terminal verdict, then the ordinary record-and-chain applies. Only
//!    a readable verdict can do this; an outage is never a reject (D28).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// The dedupe tag of an eval park: the attempt waits for its one-shot `eval_grade` row.
pub const JUDGE_GRADE_TAG: &str = "grade";

/// The dedupe tag of a judge outage: the attempt waits for the judge itself to answer.
pub const JUDGE_WAIT_TAG: &str = "wait";

/// Store the action's output without ending the attempt, and mark the row as awaiting a judge.
///
/// `judge` is the whole parked document — `{"decision":"pending","grade_id":…,"parks":n}` for an
/// eval, `{"decision":"unavailable",…}` for an outage — so a re-delivered job can tell what it is
/// waiting for without re-invoking the action (D12).
pub async fn record_output_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_step_id: Uuid,
    output: &Value,
    judge: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE run_step
            SET output = $3, judge = $4, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'running'",
    )
    .bind(tenant.as_uuid())
    .bind(run_step_id)
    .bind(output)
    .bind(judge)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Write one judge verdict onto a live attempt.
///
/// The `status = 'running'` predicate is the claim's: a verdict describes the output of the
/// attempt that owns the row. A row a cancel or the reaper already recorded is theirs, and a
/// verdict written over it would describe an output nobody is waiting for.
pub async fn set_verdict_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_step_id: Uuid,
    verdict: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE run_step
            SET judge = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'running'",
    )
    .bind(tenant.as_uuid())
    .bind(run_step_id)
    .bind(verdict)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The dedupe key of the job that re-judges one parked attempt: the step's own key with a
/// `:<tag>:<n>` suffix.
///
/// It must not share the step's base key: the job that parked the attempt is still `running` while
/// it enqueues its successor, so a shared key would collapse the successor onto it and the run
/// would stall. `<n>` counts the parks, so each wait gets a slot of its own.
pub fn judge_park_dedupe_key(
    run_id: Uuid,
    position: i32,
    attempt: i32,
    tag: &str,
    parks: u32,
) -> String {
    format!(
        "{}:{tag}:{parks}",
        crate::process::process_step_dedupe_key(run_id, position, attempt)
    )
}

/// Re-enqueue one parked attempt under its own dedupe slot, no earlier than `run_after`.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_judge_park_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
    tag: &str,
    parks: u32,
    run_after: Option<DateTime<Utc>>,
) -> Result<Option<Uuid>, sqlx::Error> {
    let payload = json!({
        "run_id": run_id,
        "position": position,
        "attempt": attempt,
    });
    // ⛔ No `run_step.requested` announcement here, deliberately: a park and a call poll are
    // enqueued with a BACKOFF (`run_after`), and the bus consumer executes the step it is nudged
    // about immediately — announcing would re-call laya or a live child on every backoff and
    // defeat the wait. These transitions are the job poller's; the bus carries the work that is
    // due now (the chained step and the approved gate's resume).
    crate::jobs::enqueue_tx(
        conn,
        tenant,
        crate::process::PROCESS_STEP_KIND,
        &judge_park_dedupe_key(run_id, position, attempt, tag, parks),
        payload,
        run_after,
        crate::process::PROCESS_STEP_MAX_ATTEMPTS,
    )
    .await
}

/// How many prior attempts of this `(run, position)` the judge REJECTED.
///
/// The reject budget is derived from the trace, not stored: a rerun appends an attempt, and the
/// new attempt's row is `queued`, so it never counts itself.
pub async fn count_judge_rejects_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3
            AND judge->>'decision' = 'reject'",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(position)
    .fetch_one(&mut *conn)
    .await
}

/// The live child run of `(parent, step)`, if any.
///
/// A `call` step starts one child and polls it; the partial unique index is what makes "one live
/// child" true, so this read can only ever answer one row.
pub async fn live_child_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    parent_run_id: Uuid,
    step_id: Uuid,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, status
           FROM run
          WHERE tenant_id = $1 AND parent_run_id = $2 AND step_id = $3
            AND status IN ('queued', 'running', 'waiting_approval')
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(parent_run_id)
    .bind(step_id)
    .fetch_optional(&mut *conn)
    .await
}

/// One run's status by id.
pub async fn run_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await
}
