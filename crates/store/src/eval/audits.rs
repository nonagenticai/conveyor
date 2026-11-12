#![forbid(unsafe_code)]
//! `eval_audit` and `eval_audit_finding` — the health audit's rows (S19 Part C step 14).
//!
//! The audit is a durable row, not a transient report: it is `queued` when `eval_audits.start`
//! writes it and one job moves it to `running` and then `succeeded | failed`. The findings are
//! append-only beside it and each starts `open`; only a person's `eval_findings.decide` moves one
//! to `accepted | fixed | dismissed`, and that transition stamps who and when — the audit
//! proposes and never applies (D25).
//!
//! The scores the controls measured (`oracle_score`, `null_score`) and the resolution numbers
//! (`noise_floor`, `headroom`) are columns, so the audit's headline is queryable without
//! unpacking `summary`. Nothing here computes them; the worker does.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::EvalStoreError;

const AUDIT_COLUMNS: &str = "id, eval_id, version_id, target_run_id, status, oracle_score, \
                             null_score, noise_floor, headroom, summary, error, requested_by, \
                             created_at, updated_at";

const FINDING_COLUMNS: &str = "id, audit_id, section, item, severity, evidence, suggestion, \
                               status, decided_by, decided_at, created_at";

/// One audit row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AuditRow {
    /// The row id.
    pub id: Uuid,
    /// The eval audited.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// The run the resolution numbers were read from, when one was named.
    pub target_run_id: Option<Uuid>,
    /// `queued | running | succeeded | failed`.
    pub status: String,
    /// The answer key's own score under the instrument's programmatic graders.
    pub oracle_score: Option<Decimal>,
    /// An empty answer's score.
    pub null_score: Option<Decimal>,
    /// The headline metric's noise floor.
    pub noise_floor: Option<Decimal>,
    /// The headline metric's headroom.
    pub headroom: Option<Decimal>,
    /// Which checks ran, and the resolution line.
    pub summary: Value,
    /// Why a failed audit failed.
    pub error: Option<String>,
    /// Who asked for it.
    pub requested_by: Option<Uuid>,
    /// When the row was written.
    pub created_at: DateTime<Utc>,
    /// When the status last moved.
    pub updated_at: DateTime<Utc>,
}

/// A new audit.
#[derive(Clone, Debug)]
pub struct NewAudit {
    /// The eval audited.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// The run the resolution numbers are read from, when one is named.
    pub target_run_id: Option<Uuid>,
    /// Who asked.
    pub requested_by: Option<Uuid>,
}

/// One finding row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct FindingRow {
    /// The row id.
    pub id: Uuid,
    /// The audit it belongs to.
    pub audit_id: Uuid,
    /// The checklist section (`1`..`6`).
    pub section: String,
    /// The check's stable name.
    pub item: String,
    /// `misleading | noise | generality`.
    pub severity: String,
    /// What was observed.
    pub evidence: String,
    /// One concrete change worth considering.
    pub suggestion: String,
    /// `open | accepted | fixed | dismissed`.
    pub status: String,
    /// Who decided it.
    pub decided_by: Option<Uuid>,
    /// When it was decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

/// A finding to write.
#[derive(Clone, Copy, Debug)]
pub struct NewFinding<'a> {
    /// `1`..`6`.
    pub section: &'a str,
    /// The check's stable name.
    pub item: &'a str,
    /// `misleading | noise | generality`.
    pub severity: &'a str,
    /// What was observed (non-empty).
    pub evidence: &'a str,
    /// One concrete change worth considering (non-empty).
    pub suggestion: &'a str,
}

/// Write a queued audit and return its id.
pub async fn insert_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewAudit,
) -> Result<Uuid, EvalStoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_audit (tenant_id, eval_id, version_id, target_run_id, requested_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(input.eval_id)
    .bind(input.version_id)
    .bind(input.target_run_id)
    .bind(input.requested_by)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// One audit, if it exists in this tenant.
pub async fn get_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<AuditRow>, EvalStoreError> {
    let sql = format!("SELECT {AUDIT_COLUMNS} FROM eval_audit WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// A keyset page of audits, newest first.
pub async fn list_audits_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Option<Uuid>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<AuditRow>, EvalStoreError> {
    let sql = format!(
        "SELECT {AUDIT_COLUMNS} FROM eval_audit
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR eval_id = $2)
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?)
}

/// The statuses an audit may move from, per target status.
///
/// `running → running` is deliberate: a job whose attempt failed transiently re-enters at the
/// top, and the transition must not turn that retry into a permanent refusal. A terminal audit
/// (`succeeded | failed`) is never a source.
fn allowed_from(target: &str) -> &'static [&'static str] {
    match target {
        "running" => &["queued", "running"],
        "succeeded" | "failed" => &["queued", "running"],
        _ => &[],
    }
}

/// Move an audit's status, writing the controls' numbers on a terminal move.
///
/// A move from a status the target does not accept is [`EvalStoreError::Illegal`]: a job that
/// re-enters a finished audit must not rewrite its findings.
#[allow(clippy::too_many_arguments)]
pub async fn transition_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    target: &str,
    oracle: Option<Decimal>,
    null: Option<Decimal>,
    noise_floor: Option<Decimal>,
    headroom: Option<Decimal>,
    summary: Option<&Value>,
    error: Option<&str>,
) -> Result<AuditRow, EvalStoreError> {
    let from: Vec<String> = allowed_from(target)
        .iter()
        .map(|status| (*status).to_owned())
        .collect();
    if from.is_empty() {
        return Err(EvalStoreError::Invalid {
            constraint: "eval_audit_status".to_owned(),
        });
    }
    let sql = format!(
        "UPDATE eval_audit
            SET status = $3,
                oracle_score = COALESCE($4, oracle_score),
                null_score = COALESCE($5, null_score),
                noise_floor = COALESCE($6, noise_floor),
                headroom = COALESCE($7, headroom),
                summary = COALESCE($8, summary),
                error = $9,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = ANY($10)
          RETURNING {AUDIT_COLUMNS}"
    );
    let row: Option<AuditRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(target)
        .bind(oracle)
        .bind(null)
        .bind(noise_floor)
        .bind(headroom)
        .bind(summary)
        .bind(error)
        .bind(from)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotOpen { what: "eval audit" })
}

/// Append one finding.
pub async fn insert_finding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    audit_id: Uuid,
    input: &NewFinding<'_>,
) -> Result<Uuid, EvalStoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_audit_finding
             (tenant_id, audit_id, section, item, severity, evidence, suggestion)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(audit_id)
    .bind(input.section)
    .bind(input.item)
    .bind(input.severity)
    .bind(input.evidence)
    .bind(input.suggestion)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// A keyset page of findings, most severe first.
pub async fn list_findings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    audit_id: Option<Uuid>,
    severity: Option<&str>,
    status: Option<&str>,
    cursor: Option<(String, Uuid)>,
    limit: i64,
) -> Result<Vec<FindingRow>, EvalStoreError> {
    let sql = format!(
        "SELECT {FINDING_COLUMNS} FROM eval_audit_finding
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR audit_id = $2)
            AND ($3::text IS NULL OR severity = $3)
            AND ($4::text IS NULL OR status = $4)
            AND ($5::text IS NULL OR (severity, id) > ($5, $6))
          ORDER BY severity ASC, id ASC
          LIMIT $7"
    );
    let (cursor_severity, cursor_id) = match cursor {
        Some((severity, id)) => (Some(severity), Some(id)),
        None => (None, None),
    };
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(audit_id)
        .bind(severity)
        .bind(status)
        .bind(cursor_severity)
        .bind(cursor_id)
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?)
}

/// Record a person's decision on one open finding.
pub async fn decide_finding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    decision: &str,
    decided_by: Uuid,
) -> Result<FindingRow, EvalStoreError> {
    let sql = format!(
        "UPDATE eval_audit_finding
            SET status = $3, decided_by = $4, decided_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'open'
          RETURNING {FINDING_COLUMNS}"
    );
    let row: Option<FindingRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(decision)
        .bind(decided_by)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => Ok(row),
        None => {
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT status FROM eval_audit_finding WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            match exists {
                Some(_) => Err(EvalStoreError::NotOpen {
                    what: "eval audit finding",
                }),
                None => Err(EvalStoreError::NotFound {
                    what: "eval audit finding",
                }),
            }
        }
    }
}
