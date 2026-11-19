#![forbid(unsafe_code)]
//! T11 — the metrics read models over the existing ledgers (SPEC_V2 §7.5, §11.9; issue #84).
//!
//! Every function is `_tx`: the caller already opened a tenant-scoped transaction. Nothing here
//! stores a metric: the threshold and reconciliation writers store configuration and verdicts,
//! and every figure is an aggregate over `approval`, `benefit_profile`/`benefit_transition`,
//! `programme_tranche`/`programme_tranche_review`, `run` and `inference_call` at read time (D37).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

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

/// One benefit profile and its latest transition.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BenefitRow {
    pub id: Uuid,
    pub reference: String,
    pub status: String,
    pub baseline: Option<Value>,
    pub target: Option<Value>,
    pub baseline_measured_at: Option<DateTime<Utc>>,
    pub post_measured_at: Option<DateTime<Utc>>,
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

/// The programme's benefit profiles with their latest transition.
pub async fn benefit_rows_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<BenefitRow>, MetricsStoreError> {
    let sql = "SELECT b.id, b.reference, b.status, b.baseline, b.target,
                      t.baseline_measured_at, t.post_measured_at
                 FROM benefit_profile b
                 LEFT JOIN LATERAL (
                     SELECT baseline_measured_at, post_measured_at
                       FROM benefit_transition
                      WHERE benefit_id = b.id AND tenant_id = b.tenant_id
                      ORDER BY created_at DESC, id DESC
                      LIMIT 1
                 ) t ON true
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
