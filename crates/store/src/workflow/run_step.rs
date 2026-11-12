#![forbid(unsafe_code)]
//! The run trace read (S13 step 9).
//!
//! One read: the run's workflow identity and every `run_step` row that belongs to a workflow
//! node, ordered by `(position, attempt)`. The writers live in [`super::run`]; this module is
//! read-only so `workflow_traces.export` never mutates the trace it exports.

use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// One `run_step` row as the trace exports it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RunTraceRow {
    /// Execution order within the run (1-based).
    pub position: i32,
    /// 1 for the first attempt; a retry is a second row at the same position.
    pub attempt: i32,
    /// The node id within the definition (always present for a workflow row).
    pub node_id: String,
    /// The action an `action` node invoked, `procedure:<key>`, or the control node's kind.
    pub action: String,
    /// `queued` | `running` | `succeeded` | `failed` | `skipped`.
    pub status: String,
    /// The resolved input.
    pub input: Value,
    /// The recorded output.
    pub output: Value,
    /// The masked input hash, when the row carries one.
    pub input_hash: Option<String>,
    /// The masked output hash, when the row carries one.
    pub output_hash: Option<String>,
    /// Wall time in milliseconds, when the row finished.
    pub duration_ms: Option<i32>,
}

/// A workflow run's identity and its trace rows.
#[derive(Clone, Debug)]
pub struct RunTrace {
    /// The workflow's stable key.
    pub workflow_key: String,
    /// The version the run executed.
    pub workflow_version: i64,
    /// The definition the version froze.
    pub definition: Value,
    /// Every workflow-node row, ordered by `(position, attempt)`.
    pub rows: Vec<RunTraceRow>,
}

/// Read one run's workflow header and its trace rows.
///
/// `None` when no run with this id belongs to the tenant, or when the run is not workflow-driven
/// (`run.workflow_id IS NULL`): there is no workflow trace to export. Every statement binds the
/// tenant explicitly and runs under the caller's `begin_scoped` scope, so FORCE RLS is the second
/// line of defence rather than the only one.
pub async fn run_trace_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<RunTrace>, sqlx::Error> {
    let header: Option<(String, i64, Value)> = sqlx::query_as(
        "SELECT w.key, w.version, w.definition
           FROM run r
           JOIN workflow w ON w.id = r.workflow_id AND w.tenant_id = r.tenant_id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some((workflow_key, workflow_version, definition)) = header else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<_, RunTraceRow>(
        "SELECT position, attempt, node_id, action, status, input, output, input_hash,
                output_hash, duration_ms
           FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND node_id IS NOT NULL
          ORDER BY position, attempt",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(Some(RunTrace {
        workflow_key,
        workflow_version,
        definition,
        rows,
    }))
}
