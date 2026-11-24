#![forbid(unsafe_code)]
//! T11 — the metrics read models over the existing ledgers (SPEC_V2 §7.5, §11.9; issue #84).
//!
//! Every function is `_tx`: the caller already opened a tenant-scoped transaction. Nothing here
//! stores a metric: the threshold and reconciliation writers store configuration and verdicts,
//! and every figure is an aggregate over `approval`, `benefit_profile`/`benefit_transition`,
//! `programme_tranche`/`programme_tranche_review`, `run` and `inference_call` at read time (D37).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

/// The one read-time price of a tool call (SPEC_V2 §7.5, D52): the `tool_call` ledger row's
/// recorded cost, or zero when the destination priced none. Every tool-spend figure — the
/// inference usage report, `metrics.usage` and `billing.summary` — sums this expression over the
/// ledger at read time; none stores a roll-up. The enclosing statement must name `tool_call` as
/// `tc` and aggregate the values itself.
pub(crate) const TOOL_CALL_PRICE: &str = "coalesce(tc.cost, 0)";

/// The metrics store's failures.
#[derive(Debug, thiserror::Error)]
pub enum MetricsStoreError {
    /// A concurrent writer took the next threshold version.
    #[error("the metric threshold changed concurrently; re-read and retry")]
    Conflict,
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// One stored threshold version.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThresholdRecord {
    pub id: Uuid,
    pub level: String,
    pub version: i64,
    pub metrics: Value,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

/// One gate's governance counts over a window.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GateCount {
    pub gate: String,
    pub approved: i64,
    pub rejected: i64,
    pub decided: i64,
    pub with_evidence: i64,
    pub latency_seconds: Option<f64>,
}

/// One benefit profile, its latest transition and its latest reading (0071). The evaluation
/// fields are `None` when the profile has no `benefit_measurement` row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BenefitRow {
    pub id: Uuid,
    pub reference: String,
    pub status: String,
    pub baseline: Option<Value>,
    pub target: Option<Value>,
    pub baseline_measured_at: Option<DateTime<Utc>>,
    pub post_measured_at: Option<DateTime<Utc>>,
    /// When the transition was enacted; a `post` reading after this makes the benefit realised.
    pub enacted_at: Option<DateTime<Utc>>,
    /// The kind of the latest measurement (`baseline | interim | post`).
    pub measurement_kind: Option<String>,
    /// The latest measurement's target and actual values.
    pub measurement_target: Option<f64>,
    pub measurement_actual: Option<f64>,
    /// When the latest measurement was taken.
    pub measurement_measured_at: Option<DateTime<Utc>>,
    /// The latest `baseline` reading's actual value, used to orient a reduction target.
    pub baseline_actual: Option<f64>,
}

/// One programme risk row and its review date.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProgrammeRiskRow {
    pub id: Uuid,
    pub code: String,
    pub title: String,
    pub severity: Option<String>,
    pub probability: Option<String>,
    pub impact: Option<i32>,
    pub status: String,
    pub owner_job_id: Option<Uuid>,
    pub review_at: Option<chrono::NaiveDate>,
}

/// One benefit put at risk by a non-met project dependency.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BenefitAtRisk {
    pub reference: String,
    pub via_project: String,
}

/// One tranche and its latest review decision.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrancheRow {
    pub number: i32,
    pub state: String,
    pub decision: Option<String>,
}

/// Run counts over a window.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunTotals {
    pub total: i64,
    pub failed: i64,
}

/// Token totals over a window.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenTotals {
    pub tokens: i64,
}

/// The latest version of each configured level, in C0–C5 order.
pub async fn latest_thresholds_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<ThresholdRecord>, MetricsStoreError> {
    let rows = sqlx::query_as::<_, ThresholdRecord>(
        "SELECT DISTINCT ON (level) id, level, version, metrics, note, created_at
           FROM metric_threshold
          WHERE tenant_id = $1
          ORDER BY level, version DESC",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Insert the next version of one level. The version is per (tenant, level); an edit is a new
/// row, never an update, so a computed level can always name the version it used.
pub async fn insert_threshold_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    level: &str,
    metrics: &Value,
    note: &str,
    actor: Option<Uuid>,
) -> Result<ThresholdRecord, MetricsStoreError> {
    let sql = "INSERT INTO metric_threshold (tenant_id, level, version, metrics, note, created_by)
               SELECT $1, $2, coalesce(max(version), 0) + 1, $3, $4, $5
                 FROM metric_threshold
                WHERE tenant_id = $1 AND level = $2
               RETURNING id, level, version, metrics, note, created_at";
    sqlx::query_as::<_, ThresholdRecord>(AssertSqlSafe(sql.to_owned()))
        .bind(tenant.as_uuid())
        .bind(level)
        .bind(metrics)
        .bind(note)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(ref db) if db.is_unique_violation() => {
                MetricsStoreError::Conflict
            }
            other => MetricsStoreError::Database(other),
        })
}

/// The governance counts per gate over `[since, until)`; the never-refused rule reads these.
pub async fn gate_counts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Vec<GateCount>, MetricsStoreError> {
    let sql = "SELECT gate,
                      count(*) FILTER (WHERE status = 'approved')::bigint AS approved,
                      count(*) FILTER (WHERE status = 'rejected')::bigint AS rejected,
                      count(*) FILTER (WHERE status IN ('approved', 'rejected'))::bigint AS decided,
                      count(*) FILTER (WHERE status IN ('approved', 'rejected')
                        AND ((meta ? 'evidence_ref' AND btrim(meta ->> 'evidence_ref') <> '')
                          OR (jsonb_typeof(meta -> 'evidence') = 'array'
                              AND jsonb_array_length(meta -> 'evidence') > 0)))::bigint AS with_evidence,
                      (avg(extract(epoch FROM (updated_at - created_at)))
                        FILTER (WHERE status IN ('approved', 'rejected')))::double precision AS latency_seconds
                 FROM approval
                WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3
                GROUP BY gate
                ORDER BY gate";
    Ok(
        sqlx::query_as::<_, GateCount>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(since)
            .bind(until)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// The programme's benefit profiles with their latest transition and latest measurement.
pub async fn benefit_rows_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<BenefitRow>, MetricsStoreError> {
    let sql = "SELECT b.id, b.reference, b.status, b.baseline, b.target,
                      t.baseline_measured_at, t.post_measured_at, t.enacted_at,
                      m.kind AS measurement_kind,
                      m.target_value AS measurement_target,
                      m.actual_value AS measurement_actual,
                      m.measured_at AS measurement_measured_at,
                      bm.actual_value AS baseline_actual
                 FROM benefit_profile b
                 LEFT JOIN LATERAL (
                     SELECT baseline_measured_at, post_measured_at, enacted_at
                       FROM benefit_transition
                      WHERE benefit_id = b.id AND tenant_id = b.tenant_id
                      ORDER BY created_at DESC, id DESC
                      LIMIT 1
                 ) t ON true
                 LEFT JOIN LATERAL (
                     SELECT kind, target_value::double precision AS target_value,
                            actual_value::double precision AS actual_value, measured_at
                       FROM benefit_measurement
                      WHERE benefit_id = b.id AND tenant_id = b.tenant_id
                      ORDER BY measured_at DESC, id DESC
                      LIMIT 1
                 ) m ON true
                 LEFT JOIN LATERAL (
                     SELECT actual_value::double precision AS actual_value
                       FROM benefit_measurement
                      WHERE benefit_id = b.id AND tenant_id = b.tenant_id AND kind = 'baseline'
                      ORDER BY measured_at DESC, id DESC
                      LIMIT 1
                 ) bm ON true
                WHERE b.tenant_id = $1 AND b.programme_id = $2 AND b.status <> 'retired'
                ORDER BY b.reference";
    Ok(
        sqlx::query_as::<_, BenefitRow>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(programme_id)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// The programme's risk register (0070), reviewed by date. A risk with no review date sorts
/// last: it is not overdue, but it is not scheduled either.
pub async fn programme_risk_rows_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<ProgrammeRiskRow>, MetricsStoreError> {
    let sql = "SELECT id, code, title, severity, probability, impact, status, owner_job_id,
                      review_at
                 FROM project_register_item
                WHERE tenant_id = $1 AND programme_id = $2 AND kind = 'risk'
                ORDER BY review_at NULLS LAST, created_at DESC, id DESC";
    Ok(
        sqlx::query_as::<_, ProgrammeRiskRow>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(programme_id)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// The benefits whose map names an output of a project that a non-met dependency points at.
pub async fn benefit_at_risk_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<BenefitAtRisk>, MetricsStoreError> {
    let sql = "SELECT DISTINCT b.reference, p.key AS via_project
                 FROM project_dependency d
                 JOIN project p ON p.id = d.to_project AND p.tenant_id = d.tenant_id
                 JOIN benefit_link l ON l.tenant_id = d.tenant_id AND l.programme_id = d.programme_id
                 JOIN benefit_profile b ON b.id = l.benefit_id AND b.tenant_id = l.tenant_id
                WHERE d.tenant_id = $1 AND d.programme_id = $2
                  AND d.status <> 'met'
                  AND (l.output_ref = p.key OR l.output_ref = p.name)
                ORDER BY b.reference";
    Ok(
        sqlx::query_as::<_, BenefitAtRisk>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(programme_id)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// The programme's tranches and their latest recorded review decision.
pub async fn tranche_rows_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<TrancheRow>, MetricsStoreError> {
    let sql = "SELECT t.number, t.state, r.decision
                 FROM programme_tranche t
                 LEFT JOIN LATERAL (
                     SELECT decision
                       FROM programme_tranche_review
                      WHERE tranche_id = t.id AND tenant_id = t.tenant_id
                      ORDER BY reviewed_at DESC, id DESC
                      LIMIT 1
                 ) r ON true
                WHERE t.tenant_id = $1 AND t.programme_id = $2
                ORDER BY t.number";
    Ok(
        sqlx::query_as::<_, TrancheRow>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(programme_id)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// Run counts over `[since, until)`.
pub async fn run_totals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<RunTotals, MetricsStoreError> {
    let sql = "SELECT count(*)::bigint AS total,
                      count(*) FILTER (WHERE status = 'failed')::bigint AS failed
                 FROM run
                WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3";
    Ok(
        sqlx::query_as::<_, RunTotals>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(since)
            .bind(until)
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// Rework counts over `[since, until)`: the terms of the rework rate.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct ReworkTotals {
    /// Every run in the window — the ratio's denominator.
    pub total: i64,
    /// The runs that were rework: a retry, or a run an approval returned.
    pub rework: i64,
}

/// Rework is derived at read time from the run and approval rows, never stored (D37).
///
/// A run is rework when the card was executed again on a later attempt (`run.attempt > 1`, the
/// sequence [`super::run::next_attempt_for_card_tx`] derives) or when an [`approval`] on it has
/// `status = 'rejected'` — the card was returned. Both counts are returned beside each other so a
/// caller can expose the ratio with its own terms; the rate itself is the caller's division.
pub async fn rework_totals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Option<Uuid>,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<ReworkTotals, MetricsStoreError> {
    let row = sqlx::query_as::<_, ReworkTotals>(
        "SELECT count(*)::bigint AS total,
                count(*) FILTER (
                    WHERE r.attempt > 1
                       OR EXISTS (
                           SELECT 1
                             FROM approval a
                            WHERE a.tenant_id = r.tenant_id
                              AND a.run_id = r.id
                              AND a.status = 'rejected'
                       ))::bigint AS rework
           FROM run r
          WHERE r.tenant_id = $1 AND r.created_at >= $2 AND r.created_at < $3
            AND ($4::uuid IS NULL OR r.project_id = $4)",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .bind(project_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

/// Tokens recorded in the inference ledger over `[since, until)`.
pub async fn token_totals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<TokenTotals, MetricsStoreError> {
    let sql = "SELECT coalesce(sum(prompt_tokens + completion_tokens), 0)::bigint AS tokens
                 FROM inference_call
                WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3";
    Ok(
        sqlx::query_as::<_, TokenTotals>(AssertSqlSafe(sql.to_owned()))
            .bind(tenant.as_uuid())
            .bind(since)
            .bind(until)
            .fetch_one(&mut *conn)
            .await?,
    )
}

// ── metering levels and the run drill-down (P1.2/P1.3) ───────────────────────────────────

/// One metering row at a level. `cost_usd` is derived at read time from `commercial_price`;
/// `tool_cost_usd` is the same read-time derivation over the `tool_call` ledger (the recorded
/// per-call price, [`TOOL_CALL_PRICE`], summed — never a stored roll-up). A call whose
/// attribution is missing lands under `unattributed`.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub key: String,
    pub calls: i64,
    pub tool_calls: i64,
    pub tokens: i64,
    pub cost_usd: f64,
    pub tool_cost_usd: f64,
    pub priced_calls: i64,
    pub unpriced_calls: i64,
    pub runs: i64,
    /// The third D52 source (T19, #606): accepted attendance × closed duration, at the same
    /// grain as tokens and tool calls.
    pub participant_hours: f64,
}

/// One run step's spend.
#[derive(Debug, Clone, PartialEq)]
pub struct StepUsage {
    pub step_id: Option<Uuid>,
    pub action: Option<String>,
    pub position: Option<i32>,
    pub tokens: i64,
    pub cost_usd: f64,
    pub tool_cost_usd: f64,
}

/// One inference call at the drill-down grain.
#[derive(Debug, Clone, PartialEq)]
pub struct CallUsage {
    pub id: Uuid,
    pub model: String,
    pub tokens: i64,
    pub cost_usd: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// One tool call at the drill-down grain.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallUsage {
    pub id: Uuid,
    pub tool_key: String,
    pub cost_usd: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// The run's total and its drill-down.
#[derive(Debug, Clone, PartialEq)]
pub struct RunDrilldown {
    pub run_id: Uuid,
    pub tokens: i64,
    pub cost_usd: f64,
    pub tool_cost_usd: f64,
    pub steps: Vec<StepUsage>,
    pub calls: Vec<CallUsage>,
    pub tool_calls: Vec<ToolCallUsage>,
}

/// The closed metering level set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeteringLevel {
    Tenant,
    Department,
    Programme,
    Project,
    Job,
    Run,
    Step,
    Call,
}

impl MeteringLevel {
    /// Parse the action's level spelling.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "tenant" => Self::Tenant,
            "department" => Self::Department,
            "programme" => Self::Programme,
            "project" => Self::Project,
            "job" => Self::Job,
            "run" => Self::Run,
            "step" => Self::Step,
            "call" => Self::Call,
            _ => return None,
        })
    }

    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::Department => "department",
            Self::Programme => "programme",
            Self::Project => "project",
            Self::Job => "job",
            Self::Run => "run",
            Self::Step => "step",
            Self::Call => "call",
        }
    }

    /// The grouping expression over the attributed CTEs. `priced` exposes `id`, `run_id`,
    /// `run_step_id`, `caller_kind`, `project_id`, `job_binding_id`, `programme_id` and
    /// `department`.
    fn key(self) -> &'static str {
        match self {
            Self::Tenant => "'tenant'",
            Self::Department => "coalesce(department, 'unattributed')",
            Self::Programme => "coalesce(programme_id::text, 'unattributed')",
            Self::Project => "coalesce(project_id::text, 'unattributed')",
            Self::Job => "coalesce(job_binding_id::text, 'unattributed')",
            Self::Run => "coalesce(run_id::text, 'unattributed')",
            Self::Step => "coalesce(run_step_id::text, 'unattributed')",
            Self::Call => "id::text",
        }
    }
}

/// The per-level attribution filter, applied to the run/project/programme columns.
fn level_filter(level: MeteringLevel, filter: Option<Uuid>) -> (String, bool) {
    let Some(id) = filter else {
        return (String::new(), false);
    };
    let clause = match level {
        MeteringLevel::Programme => " AND programme_id = $4",
        MeteringLevel::Project => " AND project_id = $4",
        MeteringLevel::Job => " AND job_binding_id = $4",
        MeteringLevel::Run | MeteringLevel::Step | MeteringLevel::Call => " AND run_id = $4",
        MeteringLevel::Tenant | MeteringLevel::Department => "",
    };
    (clause.to_owned(), !clause.is_empty() && id != Uuid::nil())
}

/// Every metering level recomputes from the ledgers; no row is stored.
pub async fn usage_by_level_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    level: MeteringLevel,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    filter: Option<Uuid>,
) -> Result<Vec<UsageRow>, MetricsStoreError> {
    let key = level.key();
    // `caller_kind = 'other'` is the synthetic/administrative caller: it is its own explicit
    // bucket, never a project's usage, even when the call carries a run.
    let inference_key = match level {
        MeteringLevel::Project => {
            "CASE WHEN caller_kind = 'other' THEN 'unattributed' \
             ELSE coalesce(project_id::text, 'unattributed') END"
        }
        _ => key,
    };
    let (filter_clause, has_filter) = level_filter(level, filter);
    let inference = format!(
        "WITH priced AS (
             SELECT ic.id, ic.run_id, ic.run_step_id, ic.caller_kind, r.project_id,
                    r.job_binding_id, p.programme_id,
                    coalesce(dep.name, 'unattributed') AS department,
                    (ic.prompt_tokens + ic.completion_tokens)::bigint AS tokens,
                    CASE
                        WHEN pr.model IS NULL THEN NULL
                        WHEN pr.unit = 'token' THEN
                            (ic.prompt_tokens::numeric * pr.input_usd_per_1m
                             + ic.completion_tokens::numeric * pr.output_usd_per_1m) / 1000000
                        WHEN ic.usage_units IS NOT NULL THEN ic.usage_units * pr.unit_usd
                        ELSE NULL
                    END AS cost
               FROM inference_call ic
               LEFT JOIN run r ON r.id = ic.run_id
               LEFT JOIN project p ON p.id = r.project_id
               LEFT JOIN job_binding jb ON jb.id = r.job_binding_id
               LEFT JOIN job_department jd ON jd.job_id = jb.job_id AND jd.is_primary
               LEFT JOIN department dep ON dep.id = jd.department_id
               LEFT JOIN commercial_price pr
                      ON pr.model = coalesce(ic.served_model, ic.model)
                     AND ((pr.unit = 'token' AND ic.usage_unit IS NULL) OR pr.unit = ic.usage_unit)
              WHERE ic.tenant_id = $1 AND ic.created_at >= $2 AND ic.created_at < $3{filter_clause}
         )
         SELECT {inference_key} AS key,
                count(*)::bigint AS calls,
                coalesce(sum(tokens), 0)::bigint AS tokens,
                coalesce(round(sum(cost), 6), 0)::double precision AS cost_usd,
                count(cost)::bigint AS priced_calls,
                (count(*) - count(cost))::bigint AS unpriced_calls,
                count(DISTINCT run_id)::bigint AS runs
           FROM priced
          GROUP BY 1
          ORDER BY 1"
    );
    let tools = format!(
        "WITH attributed AS (
             SELECT tc.id, tc.run_id, tc.run_step_id, r.project_id, r.job_binding_id,
                    p.programme_id,
                    coalesce(dep.name, 'unattributed') AS department,
                    {TOOL_CALL_PRICE}::numeric AS cost
               FROM tool_call tc
               LEFT JOIN run r ON r.id = tc.run_id
               LEFT JOIN project p ON p.id = r.project_id
               LEFT JOIN job_binding jb ON jb.id = r.job_binding_id
               LEFT JOIN job_department jd ON jd.job_id = jb.job_id AND jd.is_primary
               LEFT JOIN department dep ON dep.id = jd.department_id
              WHERE tc.tenant_id = $1 AND tc.created_at >= $2 AND tc.created_at < $3{filter_clause}
         )
         SELECT {key} AS key,
                count(*)::bigint AS tool_calls,
                coalesce(round(sum(cost), 6), 0)::double precision AS cost_usd,
                count(DISTINCT run_id)::bigint AS runs
           FROM attributed
          GROUP BY 1
          ORDER BY 1"
    );
    let mut rows: BTreeMap<String, UsageRow> = BTreeMap::new();
    for row in fetch_inference_level(
        &mut *conn, &inference, tenant, since, until, filter, has_filter,
    )
    .await?
    {
        rows.insert(row.key.clone(), row);
    }
    for (key, tool_calls, cost, runs) in
        fetch_tool_level(&mut *conn, &tools, tenant, since, until, filter, has_filter).await?
    {
        let row = rows.entry(key.clone()).or_insert_with(|| UsageRow {
            key,
            calls: 0,
            tool_calls: 0,
            tokens: 0,
            cost_usd: 0.0,
            tool_cost_usd: 0.0,
            priced_calls: 0,
            unpriced_calls: 0,
            runs: 0,
            participant_hours: 0.0,
        });
        row.tool_calls += tool_calls;
        row.tool_cost_usd = cost;
        row.runs = row.runs.max(runs);
    }
    // The third source: a closed meeting's accepted seats × its actual duration (capped by the
    // type's timebox). Run/step/call have no meeting grain, so they carry no hours.
    if !matches!(
        level,
        MeteringLevel::Run | MeteringLevel::Step | MeteringLevel::Call
    ) {
        let hours = format!(
            "WITH attended AS (
                 SELECT m.id,
                        NULL::uuid AS run_id,
                        NULL::uuid AS run_step_id,
                        b.project_id,
                        CASE WHEN a.participant_kind = 'job_binding'
                             THEN a.participant_id END AS job_binding_id,
                        p.programme_id,
                        coalesce(dep.name, 'unattributed') AS department,
                        EXTRACT(EPOCH FROM least(m.closed_at - m.opens_at, t.timebox))
                            / 3600.0 AS hours
                   FROM meeting m
                   JOIN meeting_type t ON t.id = m.meeting_type_id
                        AND t.tenant_id = m.tenant_id
                   JOIN meeting_attendance a ON a.meeting_id = m.id
                        AND a.tenant_id = m.tenant_id
                        AND a.response_state = 'accepted' AND a.seat <> 'observer'
                   JOIN card c ON c.id = m.card_id AND c.tenant_id = m.tenant_id
                   JOIN board b ON b.id = c.board_id AND b.tenant_id = m.tenant_id
                   JOIN project p ON p.id = b.project_id AND p.tenant_id = m.tenant_id
                   LEFT JOIN job_binding jb ON a.participant_kind = 'job_binding'
                        AND jb.id = a.participant_id AND jb.tenant_id = m.tenant_id
                   LEFT JOIN job_department jd ON jd.job_id = jb.job_id AND jd.is_primary
                   LEFT JOIN department dep ON dep.id = coalesce(a.department_id, jd.department_id)
                        AND dep.tenant_id = m.tenant_id
                  WHERE m.tenant_id = $1 AND m.status = 'closed'
                    AND m.closed_at IS NOT NULL AND m.opens_at IS NOT NULL
                    AND m.closed_at >= $2 AND m.closed_at < $3{filter_clause}
             )
             SELECT {key} AS key,
                    coalesce(sum(hours), 0)::double precision AS participant_hours
               FROM attended
              GROUP BY 1
              ORDER BY 1"
        );
        for (key, participant_hours) in
            fetch_hours_level(&mut *conn, &hours, tenant, since, until, filter, has_filter).await?
        {
            let row = rows.entry(key.clone()).or_insert_with(|| UsageRow {
                key,
                calls: 0,
                tool_calls: 0,
                tokens: 0,
                cost_usd: 0.0,
                tool_cost_usd: 0.0,
                priced_calls: 0,
                unpriced_calls: 0,
                runs: 0,
                participant_hours: 0.0,
            });
            row.participant_hours = participant_hours;
        }
    }
    Ok(rows.into_values().collect())
}

async fn fetch_hours_level(
    conn: &mut PgConnection,
    sql: &str,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    filter: Option<Uuid>,
    has_filter: bool,
) -> Result<Vec<(String, f64)>, MetricsStoreError> {
    let mut query = sqlx::query_as::<_, (String, f64)>(AssertSqlSafe(sql.to_owned()))
        .bind(tenant.as_uuid())
        .bind(since)
        .bind(until);
    if has_filter {
        query = query.bind(filter);
    }
    Ok(query.fetch_all(&mut *conn).await?)
}

#[derive(Debug, sqlx::FromRow)]
struct InferenceLevelRow {
    key: String,
    calls: i64,
    tokens: i64,
    cost_usd: f64,
    priced_calls: i64,
    unpriced_calls: i64,
    runs: i64,
}

async fn fetch_inference_level(
    conn: &mut PgConnection,
    sql: &str,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    filter: Option<Uuid>,
    has_filter: bool,
) -> Result<Vec<UsageRow>, MetricsStoreError> {
    let mut query = sqlx::query_as::<_, InferenceLevelRow>(AssertSqlSafe(sql.to_owned()))
        .bind(tenant.as_uuid())
        .bind(since)
        .bind(until);
    if has_filter {
        query = query.bind(filter);
    }
    Ok(query
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| UsageRow {
            key: row.key,
            calls: row.calls,
            tool_calls: 0,
            tokens: row.tokens,
            cost_usd: row.cost_usd,
            tool_cost_usd: 0.0,
            priced_calls: row.priced_calls,
            unpriced_calls: row.unpriced_calls,
            runs: row.runs,
            participant_hours: 0.0,
        })
        .collect())
}

#[derive(Debug, sqlx::FromRow)]
struct ToolLevelRow {
    key: String,
    tool_calls: i64,
    cost_usd: f64,
    runs: i64,
}

async fn fetch_tool_level(
    conn: &mut PgConnection,
    sql: &str,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    filter: Option<Uuid>,
    has_filter: bool,
) -> Result<Vec<(String, i64, f64, i64)>, MetricsStoreError> {
    let mut query = sqlx::query_as::<_, ToolLevelRow>(AssertSqlSafe(sql.to_owned()))
        .bind(tenant.as_uuid())
        .bind(since)
        .bind(until);
    if has_filter {
        query = query.bind(filter);
    }
    Ok(query
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| (row.key, row.tool_calls, row.cost_usd, row.runs))
        .collect())
}

/// The run's total and the step/call rows it is the sum of (P1.3).
pub async fn run_drilldown_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<RunDrilldown, MetricsStoreError> {
    let total = sqlx::query_as::<_, (i64, f64, f64)>(AssertSqlSafe(format!(
        "SELECT coalesce(sum(ic.prompt_tokens + ic.completion_tokens), 0)::bigint,
                coalesce(round(sum(CASE
                    WHEN pr.model IS NULL THEN NULL
                    WHEN pr.unit = 'token' THEN
                        (ic.prompt_tokens::numeric * pr.input_usd_per_1m
                         + ic.completion_tokens::numeric * pr.output_usd_per_1m) / 1000000
                    WHEN ic.usage_units IS NOT NULL THEN ic.usage_units * pr.unit_usd
                    ELSE NULL END), 6), 0)::double precision,
                coalesce((SELECT round(sum({TOOL_CALL_PRICE}), 6)::double precision
                            FROM tool_call tc
                           WHERE tc.tenant_id = $1 AND tc.run_id = $2), 0)
           FROM inference_call ic
           LEFT JOIN commercial_price pr
                  ON pr.model = coalesce(ic.served_model, ic.model)
                 AND ((pr.unit = 'token' AND ic.usage_unit IS NULL) OR pr.unit = ic.usage_unit)
          WHERE ic.tenant_id = $1 AND ic.run_id = $2"
    )))
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_one(&mut *conn)
    .await?;
    let steps = sqlx::query_as::<_, (Option<Uuid>, Option<String>, Option<i32>, i64, f64, f64)>(
        AssertSqlSafe(format!(
            "WITH calls AS (
             SELECT ic.run_step_id,
                    (ic.prompt_tokens + ic.completion_tokens)::bigint AS tokens,
                    CASE
                        WHEN pr.model IS NULL THEN NULL
                        WHEN pr.unit = 'token' THEN
                            (ic.prompt_tokens::numeric * pr.input_usd_per_1m
                             + ic.completion_tokens::numeric * pr.output_usd_per_1m) / 1000000
                        WHEN ic.usage_units IS NOT NULL THEN ic.usage_units * pr.unit_usd
                        ELSE NULL
                    END AS cost
               FROM inference_call ic
               LEFT JOIN commercial_price pr
                      ON pr.model = coalesce(ic.served_model, ic.model)
                     AND ((pr.unit = 'token' AND ic.usage_unit IS NULL) OR pr.unit = ic.usage_unit)
              WHERE ic.tenant_id = $1 AND ic.run_id = $2
         ), tools AS (
             SELECT tc.run_step_id, {TOOL_CALL_PRICE}::numeric AS cost
               FROM tool_call tc
              WHERE tc.tenant_id = $1 AND tc.run_id = $2
         )
         SELECT s.id, s.action, s.position,
                coalesce(c.tokens, 0)::bigint,
                coalesce(round(c.cost, 6), 0)::double precision,
                coalesce(round(t.cost, 6), 0)::double precision
           FROM run_step s
           LEFT JOIN (SELECT run_step_id, sum(tokens)::bigint AS tokens, sum(cost) AS cost
                        FROM calls GROUP BY run_step_id) c ON c.run_step_id = s.id
           LEFT JOIN (SELECT run_step_id, sum(cost) AS cost
                        FROM tools GROUP BY run_step_id) t ON t.run_step_id = s.id
          WHERE s.tenant_id = $1 AND s.run_id = $2
          ORDER BY s.position, s.id"
        )),
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    let calls = sqlx::query_as::<_, (Uuid, String, i64, Option<f64>, DateTime<Utc>)>(
        "SELECT ic.id, coalesce(ic.served_model, ic.model),
                (ic.prompt_tokens + ic.completion_tokens)::bigint,
                CASE
                    WHEN pr.model IS NULL THEN NULL
                    WHEN pr.unit = 'token' THEN
                        (ic.prompt_tokens::numeric * pr.input_usd_per_1m
                         + ic.completion_tokens::numeric * pr.output_usd_per_1m) / 1000000
                    WHEN ic.usage_units IS NOT NULL THEN ic.usage_units * pr.unit_usd
                    ELSE NULL
                END::double precision,
                ic.created_at
           FROM inference_call ic
           LEFT JOIN commercial_price pr
                  ON pr.model = coalesce(ic.served_model, ic.model)
                 AND ((pr.unit = 'token' AND ic.usage_unit IS NULL) OR pr.unit = ic.usage_unit)
          WHERE ic.tenant_id = $1 AND ic.run_id = $2
          ORDER BY ic.created_at, ic.id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    let tool_calls = sqlx::query_as::<_, (Uuid, String, Option<f64>, DateTime<Utc>)>(
        "SELECT id, tool_key, cost::double precision, created_at
           FROM tool_call WHERE tenant_id = $1 AND run_id = $2
          ORDER BY created_at, id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(RunDrilldown {
        run_id,
        tokens: total.0,
        cost_usd: total.1,
        tool_cost_usd: total.2,
        steps: steps
            .into_iter()
            .map(|row| StepUsage {
                step_id: row.0,
                action: row.1,
                position: row.2,
                tokens: row.3,
                cost_usd: row.4,
                tool_cost_usd: row.5,
            })
            .collect(),
        calls: calls
            .into_iter()
            .map(|row| CallUsage {
                id: row.0,
                model: row.1,
                tokens: row.2,
                cost_usd: row.3,
                created_at: row.4,
            })
            .collect(),
        tool_calls: tool_calls
            .into_iter()
            .map(|row| ToolCallUsage {
                id: row.0,
                tool_key: row.1,
                cost_usd: row.2,
                created_at: row.3,
            })
            .collect(),
    })
}

// ── the derived cost and the reconciliation record (P2) ──────────────────────────────────

/// The reconciliation row to write. A struct, not ten arguments: the shape is the record.
pub struct NewReconciliation<'a> {
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
    pub conveyor_usd: Option<f64>,
    pub billing_usd: Option<f64>,
    pub delta_usd: Option<f64>,
    pub state: &'a str,
    pub upstream_error: Option<&'a str>,
    pub incident_card_id: Option<Uuid>,
    pub actor: Option<Uuid>,
}

/// Record (or re-record) one period's verdict. The unique period makes a re-run an update.
pub async fn record_reconciliation_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    record: NewReconciliation<'_>,
) -> Result<Uuid, MetricsStoreError> {
    let sql = "INSERT INTO metric_reconciliation
                   (tenant_id, period_start, period_end, conveyor_usd, billing_usd, delta_usd,
                    state, upstream_error, incident_card_id, recorded_by)
               VALUES ($1, $2, $3, $4::double precision, $5::double precision, $6::double precision,
                       $7, $8, $9, $10)
               ON CONFLICT (tenant_id, period_start, period_end) DO UPDATE
                   SET conveyor_usd = EXCLUDED.conveyor_usd,
                       billing_usd = EXCLUDED.billing_usd,
                       delta_usd = EXCLUDED.delta_usd,
                       state = EXCLUDED.state,
                       upstream_error = EXCLUDED.upstream_error,
                       incident_card_id = EXCLUDED.incident_card_id,
                       recorded_by = EXCLUDED.recorded_by,
                       created_at = now()
               RETURNING id";
    Ok(sqlx::query_scalar(AssertSqlSafe(sql.to_owned()))
        .bind(tenant.as_uuid())
        .bind(record.period_start)
        .bind(record.period_end)
        .bind(record.conveyor_usd)
        .bind(record.billing_usd)
        .bind(record.delta_usd)
        .bind(record.state)
        .bind(record.upstream_error)
        .bind(record.incident_card_id)
        .bind(record.actor)
        .fetch_one(&mut *conn)
        .await?)
}

/// One recorded reconciliation period, as the read action serves it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReconciliationRecord {
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
    pub conveyor_usd: Option<f64>,
    pub billing_usd: Option<f64>,
    pub delta_usd: Option<f64>,
    pub state: String,
    pub upstream_error: Option<String>,
    pub incident_card_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// The recorded verdicts, newest period first. The table holds one row per period (its unique
/// constraint makes a re-run an update), so this is the latest state per period; the caller
/// derives the trend from consecutive deltas and reads the incident link from the row.
pub async fn list_reconciliations_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<ReconciliationRecord>, MetricsStoreError> {
    Ok(sqlx::query_as(
        "SELECT period_start, period_end,
                conveyor_usd::double precision AS conveyor_usd,
                billing_usd::double precision AS billing_usd,
                delta_usd::double precision AS delta_usd,
                state, upstream_error, incident_card_id, created_at
           FROM metric_reconciliation
          WHERE tenant_id = $1
          ORDER BY period_start DESC, period_end DESC
          LIMIT $2",
    )
    .bind(tenant.as_uuid())
    .bind(limit.clamp(1, 365))
    .fetch_all(&mut *conn)
    .await?)
}

/// The conveyor's own derived spend over `[since, until)`: inference cost from the price catalogue
/// plus the ledger's recorded tool cost. Unpriced calls contribute tokens but no money; the
/// reconciliation reports a figure only when both sides can be read (P2.3 never invents a zero).
pub async fn conveyor_cost_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<f64, MetricsStoreError> {
    let inference: f64 = sqlx::query_scalar(
        "SELECT coalesce(round(sum(CASE
                    WHEN pr.model IS NULL THEN NULL
                    WHEN pr.unit = 'token' THEN
                        (ic.prompt_tokens::numeric * pr.input_usd_per_1m
                         + ic.completion_tokens::numeric * pr.output_usd_per_1m) / 1000000
                    WHEN ic.usage_units IS NOT NULL THEN ic.usage_units * pr.unit_usd
                    ELSE NULL END), 6), 0)::double precision
           FROM inference_call ic
           LEFT JOIN commercial_price pr
                  ON pr.model = coalesce(ic.served_model, ic.model)
                 AND ((pr.unit = 'token' AND ic.usage_unit IS NULL) OR pr.unit = ic.usage_unit)
          WHERE ic.tenant_id = $1 AND ic.created_at >= $2 AND ic.created_at < $3",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .fetch_one(&mut *conn)
    .await?;
    let tools: f64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT coalesce(round(sum({TOOL_CALL_PRICE}), 6), 0)::double precision
           FROM tool_call tc
          WHERE tc.tenant_id = $1 AND tc.created_at >= $2 AND tc.created_at < $3"
    )))
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .fetch_one(&mut *conn)
    .await?;
    Ok(((inference + tools) * 1_000_000.0).round() / 1_000_000.0)
}

/// One window's usage, as the conveyor's own ledgers report it.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageSummary {
    /// Derived spend over the window: inference from the price catalogue, tools from the ledger.
    pub total_usd: f64,
    /// Model calls recorded in the window.
    pub inference_calls: i64,
    /// Tool calls recorded in the window.
    pub tool_calls: i64,
    /// Calls the price catalogue or the tool ledger could not price. A nonzero count makes the
    /// window's total incomplete: a summary may report it, and a reconciliation must refuse an
    /// exact verdict while it is nonzero (v1-p75-6).
    pub unpriced_calls: i64,
}

/// The ledger summary `billing.summary` serves (issue #450; SPEC_V2 §11.3, #46).
///
/// The Python billing service is not a conveyor dependency: the figure is the same
/// [`conveyor_cost_tx`] derivation the reconciliation already trusts, plus the two call counts.
/// An empty window is a zero with zero calls — never an upstream error.
pub async fn usage_summary_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<UsageSummary, MetricsStoreError> {
    let total_usd = conveyor_cost_tx(&mut *conn, tenant, since, until).await?;
    let inference_calls: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM inference_call
          WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .fetch_one(&mut *conn)
    .await?;
    let tool_calls: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM tool_call
          WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .fetch_one(&mut *conn)
    .await?;
    // A call is unpriced when the cost derivation could not price it: an inference call whose
    // model/unit has no catalogue row, or a tool call the ledger recorded without a cost. The
    // join is the same one `conveyor_cost_tx` uses, so the count names exactly the rows whose
    // contribution to `total_usd` is a silent zero.
    let unpriced_inference: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM inference_call ic
           LEFT JOIN commercial_price pr
                  ON pr.model = coalesce(ic.served_model, ic.model)
                 AND ((pr.unit = 'token' AND ic.usage_unit IS NULL) OR pr.unit = ic.usage_unit)
          WHERE ic.tenant_id = $1 AND ic.created_at >= $2 AND ic.created_at < $3
            AND pr.model IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .fetch_one(&mut *conn)
    .await?;
    let unpriced_tools: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM tool_call
          WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3
            AND cost IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(since)
    .bind(until)
    .fetch_one(&mut *conn)
    .await?;
    Ok(UsageSummary {
        total_usd,
        inference_calls,
        tool_calls,
        unpriced_calls: unpriced_inference + unpriced_tools,
    })
}

// ── flow (P1.1) ─────────────────────────────────────────────────────────────────────────

/// The flow metrics, all read-time aggregates over `card`, `list` and the `card.moved` outbox.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowRow {
    pub wip: i64,
    pub throughput: i64,
    pub lead_time_seconds: Option<f64>,
    pub cycle_time_seconds: Option<f64>,
    pub queue_time_seconds: Option<f64>,
    pub block_time_seconds: Option<f64>,
    pub flow_efficiency: Option<f64>,
}

/// WIP is the cards in an `active` or `waiting` list now; the times are over the cards that moved
/// into a `done` list inside the window, using their full event history for the earlier spans.
pub async fn flow_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Option<Uuid>,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<FlowRow, MetricsStoreError> {
    let wip_sql = if project_id.is_some() {
        "SELECT count(*)::bigint
           FROM card c
           JOIN board b ON b.id = c.board_id
           JOIN list l ON l.id = c.list_id
          WHERE c.tenant_id = $1 AND c.archived_at IS NULL
            AND l.kind IN ('active', 'waiting') AND b.project_id = $2"
    } else {
        "SELECT count(*)::bigint
           FROM card c
           JOIN board b ON b.id = c.board_id
           JOIN list l ON l.id = c.list_id
          WHERE c.tenant_id = $1 AND c.archived_at IS NULL
            AND l.kind IN ('active', 'waiting')"
    };
    let mut wip_query =
        sqlx::query_scalar::<_, i64>(AssertSqlSafe(wip_sql.to_owned())).bind(tenant.as_uuid());
    if let Some(id) = project_id {
        wip_query = wip_query.bind(id);
    }
    let wip: i64 = wip_query.fetch_one(&mut *conn).await?;
    let done_filter = if project_id.is_some() {
        " AND b.project_id = $4"
    } else {
        ""
    };
    let flow_sql = format!(
        "WITH done AS (
             SELECT e.aggregate_id AS card_id, min(e.created_at) AS done_at
               FROM event e
               JOIN list l ON l.id = (e.payload ->> 'to_list_id')::uuid
               JOIN card c ON c.id = e.aggregate_id
               JOIN board b ON b.id = c.board_id
              WHERE e.tenant_id = $1 AND e.type = 'card.moved'
                AND e.created_at >= $2 AND e.created_at < $3
                AND l.kind = 'done'{done_filter}
              GROUP BY e.aggregate_id
         ), moves AS (
             SELECT e.aggregate_id AS card_id, e.created_at AS at, l.kind AS to_kind
               FROM event e
               JOIN list l ON l.id = (e.payload ->> 'to_list_id')::uuid
              WHERE e.tenant_id = $1 AND e.type = 'card.moved'
                AND e.aggregate_id IN (SELECT card_id FROM done)
         ), ordered AS (
             SELECT card_id, at, to_kind,
                    lead(at) OVER (PARTITION BY card_id ORDER BY at) AS next_at
               FROM moves
         ), per_card AS (
             SELECT d.card_id, d.done_at, c.created_at,
                    min(o.at) FILTER (WHERE o.to_kind = 'active') AS first_active,
                    coalesce(sum(CASE WHEN o.to_kind = 'waiting'
                        THEN extract(epoch FROM (o.next_at - o.at)) END), 0) AS block_seconds,
                    coalesce(sum(CASE WHEN o.to_kind = 'active'
                        THEN extract(epoch FROM (o.next_at - o.at)) END), 0) AS active_seconds
               FROM done d
               JOIN card c ON c.id = d.card_id
               JOIN ordered o ON o.card_id = d.card_id
              GROUP BY d.card_id, d.done_at, c.created_at
         )
         SELECT count(*)::bigint AS throughput,
                avg(extract(epoch FROM (done_at - created_at)))::double precision AS lead_avg,
                avg(extract(epoch FROM (done_at - first_active)))::double precision AS cycle_avg,
                avg(extract(epoch FROM (first_active - created_at)))::double precision AS queue_avg,
                avg(block_seconds)::double precision AS block_avg,
                avg(active_seconds / NULLIF(extract(epoch FROM (done_at - created_at)), 0))::double precision AS efficiency
           FROM per_card"
    );
    let mut flow_query = sqlx::query_as::<_, FlowAgg>(AssertSqlSafe(flow_sql))
        .bind(tenant.as_uuid())
        .bind(since)
        .bind(until);
    if let Some(id) = project_id {
        flow_query = flow_query.bind(id);
    }
    let row = flow_query.fetch_one(&mut *conn).await?;
    Ok(FlowRow {
        wip,
        throughput: row.throughput,
        lead_time_seconds: row.lead_avg,
        cycle_time_seconds: row.cycle_avg,
        queue_time_seconds: row.queue_avg,
        block_time_seconds: row.block_avg,
        flow_efficiency: row.efficiency,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct FlowAgg {
    throughput: i64,
    lead_avg: Option<f64>,
    cycle_avg: Option<f64>,
    queue_avg: Option<f64>,
    block_avg: Option<f64>,
    efficiency: Option<f64>,
}
