#![forbid(unsafe_code)]
//! The tool platform's rows (SPEC_V2 §11.8; issue #76, T13).
//!
//! What lives here and nothing else:
//!
//! * **resolution** — given a run and the canonical name `<toolkit>.<operation>`, find the one
//!   published tool version the run's binding pins, with the binding's effect ceiling and
//!   credential alias. A missing binding, toolkit, tool or version is `None`: the broker turns
//!   that into a typed `409 tool_not_ready` before any network call (§11.3 step 1).
//! * **the ledger** — one append-only `tool_call` row per call that reached a resolved tool. The
//!   row is written by the broker on the pool, after the handler transaction, so a rolled-back
//!   call still leaves its record.
//!
//! A run-scoped grant (one run) is more specific than a job-binding grant, so it wins when both
//! exist. The ceiling is data copied from the binding row; the broker compares ranks and never
//! infers an effect from an HTTP method.

use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Why a tool row could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum ToolStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The run context a broker call is resolved against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunToolContext {
    /// The run the call belongs to (and the ledger's `run_id`).
    pub run_id: Uuid,
    /// The step, when the caller named one that belongs to this run.
    pub run_step_id: Option<Uuid>,
    /// The card, copied from the run for the ledger's convenience.
    pub card_id: Option<Uuid>,
    /// The durable binding this run pins, if any.
    pub job_binding_id: Option<Uuid>,
}

/// One published tool version, resolved through a binding.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTool {
    /// The `tool` row's id — the ledger's pin to this exact version.
    pub tool_id: Uuid,
    /// The toolkit version row the binding pins.
    pub toolkit_id: Uuid,
    /// The toolkit's key (`coding`).
    pub toolkit_key: String,
    /// The toolkit's semantic version (not the row lock).
    pub toolkit_version: i64,
    /// The operation key inside the toolkit (`jobs.create`).
    pub tool_key: String,
    /// The closed schema the broker validates against before the destination call.
    pub input_schema: Value,
    /// The effect class stored as data on the tool version.
    pub effect: String,
    /// The tool's own timeout, in seconds.
    pub timeout_secs: i32,
    /// The result fields the broker redacts.
    pub redact: Value,
    /// The declared external action that dispatches the destination call.
    pub handler_action: String,
    /// The binding's ceiling — it narrows the tool's effect and never raises it.
    pub effect_ceiling: String,
    /// The sealed credential alias the broker resolves (never shown to a job).
    pub credential_alias: Option<String>,
}

/// Read the run a broker call is made for.
///
/// Returns `None` for a run outside the tenant or an unknown id; the broker refuses both with the
/// same `409 tool_not_ready` so an id probe cannot distinguish them.
pub async fn run_context(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<RunToolContext>, ToolStoreError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        card_id: Option<Uuid>,
        job_binding_id: Option<Uuid>,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, card_id, job_binding_id FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| RunToolContext {
        run_id: row.id,
        run_step_id: None,
        card_id: row.card_id,
        job_binding_id: row.job_binding_id,
    }))
}

/// Whether a step belongs to the run it is being recorded against.
pub async fn step_belongs_to_run(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    step_id: Uuid,
) -> Result<bool, ToolStoreError> {
    let found: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM run_step WHERE tenant_id = $1 AND run_id = $2 AND id = $3")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .bind(step_id)
            .fetch_optional(conn)
            .await?;
    Ok(found.is_some())
}

const RESOLVED_COLUMNS: &str = "t.id AS tool_id, k.id AS toolkit_id, k.key AS toolkit_key, \
     k.toolkit_version, t.key AS tool_key, t.input_schema, t.effect, t.timeout_secs, \
     t.redact, t.handler_action, b.effect_ceiling, b.credential_alias";

#[derive(sqlx::FromRow)]
struct ResolvedRow {
    tool_id: Uuid,
    toolkit_id: Uuid,
    toolkit_key: String,
    toolkit_version: i64,
    tool_key: String,
    input_schema: Value,
    effect: String,
    timeout_secs: i32,
    redact: Value,
    handler_action: String,
    effect_ceiling: String,
    credential_alias: Option<String>,
}

impl From<ResolvedRow> for ResolvedTool {
    fn from(row: ResolvedRow) -> Self {
        Self {
            tool_id: row.tool_id,
            toolkit_id: row.toolkit_id,
            toolkit_key: row.toolkit_key,
            toolkit_version: row.toolkit_version,
            tool_key: row.tool_key,
            input_schema: row.input_schema,
            effect: row.effect,
            timeout_secs: row.timeout_secs,
            redact: row.redact,
            handler_action: row.handler_action,
            effect_ceiling: row.effect_ceiling,
            credential_alias: row.credential_alias,
        }
    }
}

/// Resolve a canonical tool name through the run's grants.
///
/// Only a **published** toolkit and a **published** tool resolve; a draft or retired version is
/// indistinguishable from a missing one, which is what makes retirement safe (§11.9). A
/// run-scoped grant wins over a job-binding grant for the same toolkit.
pub async fn resolve(
    conn: &mut PgConnection,
    tenant: TenantId,
    run: &RunToolContext,
    toolkit_key: &str,
    tool_key: &str,
) -> Result<Option<ResolvedTool>, ToolStoreError> {
    let sql = format!(
        "SELECT {RESOLVED_COLUMNS}
           FROM tool_binding b
           JOIN toolkit k ON k.id = b.toolkit_id AND k.tenant_id = b.tenant_id
           JOIN tool t ON t.toolkit_id = k.id AND t.tenant_id = k.tenant_id
          WHERE b.tenant_id = $1
            AND b.enabled
            AND (b.run_id = $2 OR (b.job_binding_id IS NOT NULL AND b.job_binding_id = $3))
            AND k.key = $4
            AND k.status = 'published'
            AND t.key = $5
            AND t.status = 'published'
          ORDER BY (b.run_id IS NULL) ASC, b.created_at DESC
          LIMIT 1"
    );
    let row: Option<ResolvedRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run.run_id)
        .bind(run.job_binding_id)
        .bind(toolkit_key)
        .bind(tool_key)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(ResolvedTool::from))
}

/// One call to write into the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewToolCall<'a> {
    /// The run, when the call belongs to one.
    pub run_id: Option<Uuid>,
    /// The step, when the call belongs to one.
    pub run_step_id: Option<Uuid>,
    /// The card, copied from the run.
    pub card_id: Option<Uuid>,
    /// The exact tool version row.
    pub tool_id: Uuid,
    /// The canonical name the caller used.
    pub tool_key: &'a str,
    /// The toolkit key.
    pub toolkit_key: &'a str,
    /// The pinned toolkit version.
    pub toolkit_version: i64,
    /// The SHA-256 of the validated input.
    pub input_hash: &'a str,
    /// `ok | precondition_failed | check_failed | pending | backend_error`.
    pub status: &'a str,
    /// The destination's HTTP status, when it answered.
    pub http_status: Option<i32>,
    /// Wall-clock latency of the destination call.
    pub latency_ms: i32,
    /// Bytes sent.
    pub request_bytes: Option<i32>,
    /// Bytes received.
    pub response_bytes: Option<i32>,
    /// Metered cost, when the tool spends.
    pub cost: Option<Decimal>,
    /// A short class the caller can branch on.
    pub error_class: Option<&'a str>,
    /// The error text (already redacted).
    pub error_message: Option<&'a str>,
}

/// Append one ledger row and return its id.
///
/// The broker calls this on the pool, not inside the handler transaction: a failed call must
/// still leave its record (§11.3 step 7).
pub async fn insert_call(
    conn: &mut PgConnection,
    tenant: TenantId,
    call: &NewToolCall<'_>,
) -> Result<Uuid, ToolStoreError> {
    let id: (Uuid,) = sqlx::query_as(
        "INSERT INTO tool_call (tenant_id, run_id, run_step_id, card_id, tool_id, tool_key,
                                toolkit_key, toolkit_version, input_hash, status, http_status,
                                latency_ms, request_bytes, response_bytes, cost, error_class,
                                error_message)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(call.run_id)
    .bind(call.run_step_id)
    .bind(call.card_id)
    .bind(call.tool_id)
    .bind(call.tool_key)
    .bind(call.toolkit_key)
    .bind(call.toolkit_version)
    .bind(call.input_hash)
    .bind(call.status)
    .bind(call.http_status)
    .bind(call.latency_ms)
    .bind(call.request_bytes)
    .bind(call.response_bytes)
    .bind(call.cost)
    .bind(call.error_class)
    .bind(call.error_message)
    .fetch_one(conn)
    .await?;
    Ok(id.0)
}

/// The ledger for one run, oldest first — the read side of P2.1's assertion.
#[derive(Debug, sqlx::FromRow)]
pub struct ToolCallRow {
    /// The ledger row.
    pub id: Uuid,
    /// The run it was made for.
    pub run_id: Option<Uuid>,
    /// The tool version it pinned.
    pub tool_id: Uuid,
    /// The canonical name the caller used.
    pub tool_key: String,
    /// The toolkit key.
    pub toolkit_key: String,
    /// The pinned toolkit version.
    pub toolkit_version: i64,
    /// The input hash.
    pub input_hash: String,
    /// The call's terminal status.
    pub status: String,
    /// The destination's HTTP status.
    pub http_status: Option<i32>,
    /// Latency in milliseconds.
    pub latency_ms: i32,
}

/// Every call recorded for one run, oldest first.
pub async fn calls_for_run(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<ToolCallRow>, ToolStoreError> {
    let rows: Vec<ToolCallRow> = sqlx::query_as(
        "SELECT id, run_id, tool_id, tool_key, toolkit_key, toolkit_version, input_hash, status,
                http_status, latency_ms
           FROM tool_call
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY created_at ASC, id ASC",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// Every call recorded against one tool version, for the zero-use and health reports (§11.9).
pub async fn calls_for_tool(
    conn: &mut PgConnection,
    tenant: TenantId,
    tool_id: Uuid,
    limit: i64,
) -> Result<Vec<ToolCallRow>, ToolStoreError> {
    let rows: Vec<ToolCallRow> = sqlx::query_as(
        "SELECT id, run_id, tool_id, tool_key, toolkit_key, toolkit_version, input_hash, status,
                http_status, latency_ms
           FROM tool_call
          WHERE tenant_id = $1 AND tool_id = $2
          ORDER BY created_at DESC, id DESC
          LIMIT $3",
    )
    .bind(tenant.as_uuid())
    .bind(tool_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}
