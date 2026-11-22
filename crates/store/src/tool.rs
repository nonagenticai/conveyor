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
    /// The tool version's idempotency policy (§11.3): required | optional | forbidden.
    pub idempotency: String,
    /// The tool's own timeout, in seconds.
    pub timeout_secs: i32,
    /// The result fields the broker redacts.
    pub redact: Value,
    /// The declared external action that dispatches the destination call.
    pub handler_action: String,
    /// The write verification declaration (§11.3); `{}` on a non-writing tool.
    pub verify: Value,
    /// The binding's ceiling — it narrows the tool's effect and never raises it.
    pub effect_ceiling: String,
    /// The sealed credential alias the broker resolves (never shown to a job).
    pub credential_alias: Option<String>,
    /// The toolkit's auth adapter (`bearer | none | sealed-profile`). The broker refuses a
    /// binding with no `credential_alias` against a toolkit whose adapter is not `none`.
    pub auth_adapter: String,
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
     k.toolkit_version, t.key AS tool_key, t.input_schema, t.effect, t.idempotency, \
     t.timeout_secs, t.redact, t.handler_action, t.verify, b.effect_ceiling, b.credential_alias, \
     k.auth_adapter";

#[derive(sqlx::FromRow)]
struct ResolvedRow {
    tool_id: Uuid,
    toolkit_id: Uuid,
    toolkit_key: String,
    toolkit_version: i64,
    tool_key: String,
    input_schema: Value,
    effect: String,
    idempotency: String,
    timeout_secs: i32,
    redact: Value,
    handler_action: String,
    verify: Value,
    effect_ceiling: String,
    credential_alias: Option<String>,
    auth_adapter: String,
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
            idempotency: row.idempotency,
            timeout_secs: row.timeout_secs,
            redact: row.redact,
            handler_action: row.handler_action,
            verify: row.verify,
            effect_ceiling: row.effect_ceiling,
            credential_alias: row.credential_alias,
            auth_adapter: row.auth_adapter,
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

/// Resolve every published tool the run's grants make callable.
///
/// This is the projection's source (SPEC_V2 §4.5: "the curated list of tools bound to this job,
/// with their closed schemas"). Unlike [`resolve`], it is not scoped to one canonical name; unlike
/// a per-toolkit read, it applies the same grant rule, so a run-scoped grant wins over a
/// job-binding grant for the same toolkit. The binding's ceiling travels with the row: the caller
/// filters ranks for the model's list, and the broker re-checks at call time.
pub async fn bound_tools(
    conn: &mut PgConnection,
    tenant: TenantId,
    run: &RunToolContext,
) -> Result<Vec<ResolvedTool>, ToolStoreError> {
    let sql = format!(
        "SELECT DISTINCT ON (k.key, t.key) {RESOLVED_COLUMNS}
           FROM tool_binding b
           JOIN toolkit k ON k.id = b.toolkit_id AND k.tenant_id = b.tenant_id
           JOIN tool t ON t.toolkit_id = k.id AND t.tenant_id = k.tenant_id
          WHERE b.tenant_id = $1
            AND b.enabled
            AND (b.run_id = $2 OR (b.job_binding_id IS NOT NULL AND b.job_binding_id = $3))
            AND k.status = 'published'
            AND t.status = 'published'
          ORDER BY k.key, t.key, (b.run_id IS NULL) ASC, b.created_at DESC"
    );
    let rows: Vec<ResolvedRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run.run_id)
        .bind(run.job_binding_id)
        .fetch_all(conn)
        .await?;
    Ok(rows.into_iter().map(ResolvedTool::from).collect())
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

// ── T14 (#83): the estate import wave, its probes and the zero-use report ─────────────────────
//
// A published toolkit version is an immutable row set: `toolkit` + its `tool` rows. A change in
// the manifest's content inserts the next `toolkit_version`; it never updates a published row.
// The only in-place transition is status (draft → published → retired), which is what makes a
// draft unbound and a retirement safe: `resolve` requires `status = 'published'` on both.

/// One toolkit version.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ToolkitRow {
    /// The row.
    pub id: Uuid,
    /// The callable prefix (`coding`).
    pub key: String,
    /// The version this row pins.
    pub toolkit_version: i64,
    /// The destination service alias (`coding`).
    pub service: String,
    /// The human title.
    pub title: String,
    /// Who owns the toolkit.
    pub owner: String,
    /// `draft | published | retired`.
    pub status: String,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: String,
    /// The licence app, when the toolkit is licence-gated.
    pub licence_app: Option<String>,
    /// Free-form data; carries `content_hash` and `health_path`.
    pub meta: Value,
}

/// One tool row of a toolkit version.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ToolRow {
    /// The row.
    pub id: Uuid,
    /// The version it belongs to.
    pub toolkit_id: Uuid,
    /// The operation key inside the toolkit.
    pub key: String,
    /// The registry action the broker dispatches.
    pub handler_action: String,
    /// `read | write | spend | destructive`.
    pub effect: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The closed input schema (a copy of the action's, taken at publish time).
    pub input_schema: Value,
}

/// A toolkit version to insert.
pub struct NewToolkit<'a> {
    /// The callable prefix.
    pub key: &'a str,
    /// The next version for this key.
    pub toolkit_version: i64,
    /// The human title.
    pub title: &'a str,
    /// The destination service alias.
    pub service: &'a str,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: &'a str,
    /// The licence app, when gated.
    pub licence_app: Option<&'a str>,
    /// Who owns the toolkit.
    pub owner: &'a str,
    /// `draft | published`.
    pub status: &'a str,
    /// `content_hash` and `health_path`.
    pub meta: &'a Value,
}

/// A tool row to insert under a toolkit version.
pub struct NewTool<'a> {
    /// The toolkit version.
    pub toolkit_id: Uuid,
    /// The operation key.
    pub key: &'a str,
    /// The registry action.
    pub handler_action: &'a str,
    /// The closed input schema copied from the action.
    pub input_schema: &'a Value,
    /// The write verification declaration (§11.3); `{}` on a non-writing tool.
    pub verify: &'a Value,
    /// The result fields the broker redacts (§11.3).
    pub redact: &'a [String],
    /// The effect class copied from the action.
    pub effect: &'a str,
    /// 1..=300.
    pub timeout_secs: i32,
}

/// The newest version of one toolkit key, whatever its status.
pub async fn latest_toolkit(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<ToolkitRow>, ToolStoreError> {
    let row: Option<ToolkitRow> = sqlx::query_as(
        "SELECT id, key, toolkit_version, service, title, owner, status, auth_adapter,
                licence_app, meta
           FROM toolkit
          WHERE tenant_id = $1 AND key = $2
          ORDER BY toolkit_version DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

/// Every published toolkit version, oldest key first.
pub async fn published_toolkits(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<ToolkitRow>, ToolStoreError> {
    let rows: Vec<ToolkitRow> = sqlx::query_as(
        "SELECT id, key, toolkit_version, service, title, owner, status, auth_adapter,
                licence_app, meta
           FROM toolkit
          WHERE tenant_id = $1 AND status = 'published'
          ORDER BY key, toolkit_version",
    )
    .bind(tenant.as_uuid())
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// One toolkit version by id.
pub async fn toolkit_by_id(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ToolkitRow>, ToolStoreError> {
    let row: Option<ToolkitRow> = sqlx::query_as(
        "SELECT id, key, toolkit_version, service, title, owner, status, auth_adapter,
                licence_app, meta
           FROM toolkit
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

/// Insert one toolkit version and return its id.
pub async fn insert_toolkit(
    conn: &mut PgConnection,
    tenant: TenantId,
    toolkit: &NewToolkit<'_>,
) -> Result<Uuid, ToolStoreError> {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, auth_adapter,
                              licence_app, owner, status, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit.key)
    .bind(toolkit.toolkit_version)
    .bind(toolkit.title)
    .bind(toolkit.service)
    .bind(toolkit.auth_adapter)
    .bind(toolkit.licence_app)
    .bind(toolkit.owner)
    .bind(toolkit.status)
    .bind(toolkit.meta)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Insert one tool row and return its id.
pub async fn insert_tool(
    conn: &mut PgConnection,
    tenant: TenantId,
    tool: &NewTool<'_>,
) -> Result<Uuid, ToolStoreError> {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, timeout_secs,
                           handler_action, verify, redact, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'published')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(tool.toolkit_id)
    .bind(tool.key)
    .bind(tool.input_schema)
    .bind(tool.effect)
    .bind(tool.timeout_secs)
    .bind(tool.handler_action)
    .bind(tool.verify)
    .bind(serde_json::json!(tool.redact))
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Move one toolkit version's status (`draft` → `published` → `retired`).
pub async fn set_toolkit_status(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    status: &str,
) -> Result<(), ToolStoreError> {
    sqlx::query(
        "UPDATE toolkit SET status = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(status)
    .execute(conn)
    .await?;
    Ok(())
}

/// Every tool row of one toolkit version, key order.
pub async fn tools_of(
    conn: &mut PgConnection,
    tenant: TenantId,
    toolkit_id: Uuid,
) -> Result<Vec<ToolRow>, ToolStoreError> {
    let rows: Vec<ToolRow> = sqlx::query_as(
        "SELECT id, toolkit_id, key, handler_action, effect, status, input_schema
           FROM tool
          WHERE tenant_id = $1 AND toolkit_id = $2
          ORDER BY key",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// Disable every binding of one toolkit version, marking the ones this probe turned off.
///
/// The marker matters: a binding an operator disabled by hand must not be re-enabled by a
/// later green probe. Returns the number of bindings this call disabled.
pub async fn disable_bindings(
    conn: &mut PgConnection,
    tenant: TenantId,
    toolkit_id: Uuid,
) -> Result<u64, ToolStoreError> {
    let done = sqlx::query(
        "UPDATE tool_binding
            SET enabled = false,
                meta = meta || jsonb_build_object('probe_disabled', true),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND toolkit_id = $2 AND enabled",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .execute(conn)
    .await?
    .rows_affected();
    Ok(done)
}

/// Re-enable only the bindings a failed probe disabled, clearing the marker.
pub async fn reenable_probe_disabled(
    conn: &mut PgConnection,
    tenant: TenantId,
    toolkit_id: Uuid,
) -> Result<u64, ToolStoreError> {
    let done = sqlx::query(
        "UPDATE tool_binding
            SET enabled = true,
                meta = meta - 'probe_disabled',
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND toolkit_id = $2
            AND NOT enabled
            AND meta @> '{\"probe_disabled\": true}'::jsonb",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .execute(conn)
    .await?
    .rows_affected();
    Ok(done)
}

/// One row of the zero-use report (§11.9): a published tool version and its calls in a window.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ToolUsageRow {
    /// The toolkit key.
    pub toolkit_key: String,
    /// The operation key.
    pub tool_key: String,
    /// The toolkit's owner — who is asked about a zero-use tool.
    pub owner: String,
    /// The pinned version the calls were made against.
    pub toolkit_version: i64,
    /// Calls inside the window.
    pub calls: i64,
    /// The newest call inside the window, when there is one.
    pub last_call_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Every published tool version with its call count since `since`, zero-use rows included.
pub async fn usage_since(
    conn: &mut PgConnection,
    tenant: TenantId,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<ToolUsageRow>, ToolStoreError> {
    let rows: Vec<ToolUsageRow> = sqlx::query_as(
        "SELECT k.key AS toolkit_key, t.key AS tool_key, k.owner, k.toolkit_version,
                count(c.id)::bigint AS calls,
                max(c.created_at) AS last_call_at
           FROM tool t
           JOIN toolkit k ON k.id = t.toolkit_id AND k.tenant_id = t.tenant_id
           LEFT JOIN tool_call c
                  ON c.tool_id = t.id AND c.tenant_id = t.tenant_id AND c.created_at >= $2
          WHERE t.tenant_id = $1 AND t.status = 'published' AND k.status = 'published'
          GROUP BY k.key, t.key, k.owner, k.toolkit_version, t.id
          ORDER BY k.key, t.key, k.toolkit_version",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}
