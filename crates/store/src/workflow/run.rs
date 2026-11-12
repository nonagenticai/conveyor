//! A workflow run's rows and its driving job (S13 steps 6–7).
//!
//! The run is S03's `run` row with `workflow_id` set (migration `0019`). Its state is the
//! `run_step` trace: every attempt appends a row and a retry is a new `attempt` at the same
//! `position`, so `UNIQUE (run_id, position, attempt)` is what makes a replayed job collapse onto
//! the attempt it already started. The job's dedupe key is the same triple, mirroring S11's
//! `procedure.step` (`crate::process::procedure_step_dedupe_key`).

use conveyor_domain::TenantId;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{WorkflowStoreError, map_write_error};

/// The job kind that drives one workflow node attempt.
pub const WORKFLOW_NODE_KIND: &str = "workflow.node";

/// The job's own attempts before it is declared dead. A node's retries are separate attempts (new
/// rows); this bounds a job that keeps failing before it ever records one.
pub const WORKFLOW_NODE_MAX_ATTEMPTS: i32 = 5;

/// The spelling of one driving-job key: `<run_id>:tick:<ordinal>`.
///
/// ⛔ The ordinal is load-bearing, not decoration. The live-dedupe index treats a `running` job as
/// live, and a tick's own job row is `running` while its handler enqueues the successor — so a
/// constant key collides with the caller itself, the insert is a no-op, and the run executes
/// exactly one node and stalls. Measured 2026-11-09 on the §7 gate5 harness: one `run_step`, one
/// succeeded `workflow.node` job, nothing pending. The ordinal is the number of `run_step` rows
/// the run has recorded in this transaction: it advances on every executed step, including the
/// attempt row a retry appends, so consecutive ticks never share a key while a replay of the
/// *same* enqueue still collapses onto the live job.
pub fn workflow_node_dedupe_key(run_id: Uuid, ordinal: i64) -> String {
    format!("{run_id}:tick:{ordinal}")
}

/// Enqueue the job that drives a run's next node attempt, on the caller's transaction.
///
/// `delay_secs` is how long the job waits before it is claimable (a procedure child still running
/// re-enqueues itself with a delay).
pub async fn enqueue_workflow_node_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    delay_secs: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    // Read in the caller's transaction, so a step row written a moment ago is counted and the
    // next tick's key differs from the running job's.
    let ordinal: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run_step WHERE tenant_id = $1 AND run_id = $2")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .fetch_one(&mut *conn)
            .await?;
    let payload = json!({ "run_id": run_id });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts, run_after)
         VALUES ($1, $2, $3, $4, $5, now() + make_interval(secs => $6))
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(WORKFLOW_NODE_KIND)
    .bind(workflow_node_dedupe_key(run_id, ordinal))
    .bind(payload)
    .bind(WORKFLOW_NODE_MAX_ATTEMPTS)
    .bind(delay_secs)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Record what a still-running attempt has produced so far (a procedure node's child run id).
pub async fn record_running_output_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
    output: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE run_step SET output = $5, updated_at = now()
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(position)
    .bind(attempt)
    .bind(output)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Point an already-inserted run at its workflow (S03's forward reference, migration `0019`).
pub async fn set_workflow_id_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    workflow_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE run SET workflow_id = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(workflow_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// What the driving handler needs from the run row.
#[derive(Clone, Debug)]
pub struct WorkflowRunContext {
    /// The workflow the run executes; `None` means the row is not a workflow run.
    pub workflow_id: Option<Uuid>,
    /// The caller's inputs (the template context's `run.inputs`).
    pub inputs: Value,
    /// `run.status` as stored.
    pub status: String,
    /// The run's card, when it is card-scoped.
    pub card_id: Option<Uuid>,
    /// The run's project.
    pub project_id: Uuid,
    /// The `capability_agent` the step executor resolves for a model-driven node (S13 step 8,
    /// CONTRACT §17). `None` means the run declared no executor: an `agent.run` node then needs its
    /// whole config in the node input.
    pub capability_agent_id: Option<Uuid>,
}

/// Read the workflow-specific columns of one run.
/// The `run` columns this module reads, in the order the query selects them.
type RunContextRow = (
    Option<Uuid>,
    Value,
    String,
    Option<Uuid>,
    Uuid,
    Option<Uuid>,
);

pub async fn workflow_run_context_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<WorkflowRunContext>, sqlx::Error> {
    let row: Option<RunContextRow> = sqlx::query_as(
        "SELECT workflow_id, inputs, status, card_id, project_id, capability_agent_id
           FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(workflow_id, inputs, status, card_id, project_id, capability_agent_id)| {
            WorkflowRunContext {
                workflow_id,
                inputs,
                status,
                card_id,
                project_id,
                capability_agent_id,
            }
        },
    ))
}

/// One attempt to start, as the trace records it.
#[derive(Clone, Debug)]
pub struct NewWorkflowStep {
    /// Execution order within the run (1-based).
    pub position: i32,
    /// 1 for the first attempt.
    pub attempt: i32,
    /// The node the attempt runs.
    pub node_id: String,
    /// The action an `action` node invokes, or the node kind otherwise (`run_step.action` is NOT NULL).
    pub action: String,
    /// The resolved input.
    pub input: Value,
    /// The masked `sha256:` hash of the resolved input (S13 step 9's trace witness).
    pub input_hash: Option<String>,
}

/// Start (or restart) one attempt's `run_step` row.
///
/// A crash between the claim and the outcome leaves the row `running`; the next job restarts the
/// same `(position, attempt)` — the conflict clause makes that an update, never a second row.
pub async fn start_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    step: &NewWorkflowStep,
) -> Result<Uuid, WorkflowStoreError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO run_step
             (tenant_id, run_id, position, attempt, action, status, input, node_id, input_hash,
              started_at)
         VALUES ($1, $2, $3, $4, $5, 'running', $6, $7, $8, now())
         ON CONFLICT (run_id, position, attempt) DO UPDATE
            SET status = 'running', started_at = now(), updated_at = now(),
                input = EXCLUDED.input, action = EXCLUDED.action,
                input_hash = EXCLUDED.input_hash
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(step.position)
    .bind(step.attempt)
    .bind(&step.action)
    .bind(&step.input)
    .bind(&step.node_id)
    .bind(&step.input_hash)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_write_error)?;
    row.map(|(id,)| id)
        .ok_or_else(|| WorkflowStoreError::Invalid {
            constraint: "run_step".to_owned(),
        })
}

/// Finish an attempt's row with its outcome.
#[allow(clippy::too_many_arguments)]
pub async fn finish_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
    status: &str,
    output: &Value,
    output_hash: Option<&str>,
    duration_ms: Option<i32>,
    error: Option<&str>,
) -> Result<(), WorkflowStoreError> {
    sqlx::query(
        "UPDATE run_step
            SET status = $5, output = $6, output_hash = $7, duration_ms = $8,
                error = $9, finished_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(position)
    .bind(attempt)
    .bind(status)
    .bind(output)
    .bind(output_hash)
    .bind(duration_ms)
    .bind(error)
    .execute(&mut *conn)
    .await
    .map_err(map_write_error)?;
    Ok(())
}

/// One `run_step` row as the replay reads it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RunStepRow {
    /// The row's id — the run tree's `step_id` when a procedure child is started from it.
    pub id: Uuid,
    /// Execution order within the run.
    pub position: i32,
    /// 1 for the first attempt.
    pub attempt: i32,
    /// The node (`NULL` for a procedure step's row — never for a workflow run).
    pub node_id: Option<String>,
    /// `queued` | `running` | `succeeded` | `failed` | `skipped`.
    pub status: String,
    /// The resolved input.
    pub input: Value,
    /// The recorded output.
    pub output: Value,
    /// Wall time, when the row finished.
    pub duration_ms: Option<i32>,
    /// The failure message, when it failed.
    pub error: Option<String>,
}

/// Every row of a run, in replay order.
pub async fn list_run_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<RunStepRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, position, attempt, node_id, status, input, output, duration_ms, error
           FROM run_step
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY position, attempt",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await
}

/// The position the next new node attempt takes (`max(position) + 1`).
pub async fn next_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<i32, sqlx::Error> {
    let row: (i32,) = sqlx::query_as(
        "SELECT coalesce(max(position), 0) + 1 FROM run_step
          WHERE tenant_id = $1 AND run_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.0)
}
