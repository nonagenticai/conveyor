#![forbid(unsafe_code)]
//! `eval_grade` — one output graded once against one instrument (migration 0034).
//!
//! S19's run judges a stored **case set**: the driver invokes a subject per case and grades the
//! captured output. A process step has no case — the output is the step's own — so an
//! `inspection.kind = 'eval'` SOP needs a one-shot grade of one output. This module is the row
//! for that path, and only that path; the run, the report and the climb are untouched.
//!
//! Three facts are deliberate:
//!
//! - **The output is not stored here.** `output_hash` is its identity (the same relationship
//!   `inference_call.prompt_sha256` has with a prompt); the output itself rides the `eval.grade`
//!   job payload, and the step's own `run_step.output` already holds it.
//! - **The row is written twice and only twice**: [`insert_pending_tx`] writes `pending`, and
//!   [`set_terminal_tx`] moves it to `accepted | rejected | failed` with
//!   `WHERE status = 'pending'`. The conditional update is the exactly-once guarantee — a
//!   replayed job finds no pending row and writes nothing.
//! - **A grader failure is `failed`, never `rejected`.** A rejected output was judged and lost;
//!   a failed one was never judged, and the two must not be read as the same event.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// The three terminal statuses [`set_terminal_tx`] may write.
pub const TERMINAL_STATUSES: [&str; 3] = ["accepted", "rejected", "failed"];

/// One `eval_grade` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EvalGrade {
    /// The row id — the handle the caller polls.
    pub id: Uuid,
    /// The tenant that owns the row (the RLS policy binds on it).
    pub tenant_id: Uuid,
    /// The eval whose metrics graded the output.
    pub eval_id: Uuid,
    /// The instrument frozen on the eval when the grade was created.
    pub version_id: Uuid,
    /// The graded output's identity (lowercase hex sha256), never the answer key.
    pub output_hash: String,
    /// `pending | accepted | rejected | failed`.
    pub status: String,
    /// `{metric_key: score}` — the same shape `eval_case_result.grade` carries.
    pub grade: Value,
    /// Per-metric explanation/detail — the grader's evidence.
    pub detail: Value,
    /// The mean over the quality metrics; `None` until graded.
    pub score: Option<Decimal>,
    /// The threshold the caller judged at.
    pub threshold: Decimal,
    /// Why the verdict is what it is.
    pub reason: Option<String>,
    /// Why the grade could not be reached (status `failed`).
    pub error: Option<String>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A pending grade to insert.
#[derive(Clone, Debug)]
pub struct NewEvalGrade<'a> {
    /// The eval whose current version grades the output, resolved in the caller's tenant.
    pub eval_id: Uuid,
    /// The eval's current version — the instrument the grade is bound to.
    pub version_id: Uuid,
    /// The output's identity, from `content_hash`.
    pub output_hash: &'a str,
    /// The acceptance bar, `0..=1`.
    pub threshold: Decimal,
}

/// The terminal state of a grade.
#[derive(Clone, Debug)]
pub struct TerminalGrade<'a> {
    /// `accepted | rejected | failed`.
    pub status: &'a str,
    /// `{metric_key: score}`, an object even when a metric failed to grade.
    pub grade: &'a Value,
    /// Per-metric explanation.
    pub detail: &'a Value,
    /// The mean over the quality metrics; absent on a failure.
    pub score: Option<Decimal>,
    /// Why the verdict is what it is.
    pub reason: Option<&'a str>,
    /// Why no verdict could be reached.
    pub error: Option<&'a str>,
}

const GRADE_COLUMNS: &str = "id, tenant_id, eval_id, version_id, output_hash, status, grade, \
                             detail, score, threshold, reason, error, created_at, updated_at";

/// Insert one `pending` grade and return its id.
///
/// The eval and version are resolved by the caller **in its tenant** before this call; the
/// table's foreign keys do not carry tenancy, so resolving first is what stops a cross-tenant
/// `version_id` from being written (CONTRACT §2). The status is the schema default.
pub async fn insert_pending_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewEvalGrade<'_>,
) -> Result<Uuid, EvalStoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_grade (tenant_id, eval_id, version_id, output_hash, threshold)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(input.eval_id)
    .bind(input.version_id)
    .bind(input.output_hash)
    .bind(input.threshold)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_write_error)?;
    Ok(id)
}

/// One grade by id in the caller's tenant, if it exists.
pub async fn get_grade_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    grade_id: Uuid,
) -> Result<Option<EvalGrade>, EvalStoreError> {
    let sql = format!("SELECT {GRADE_COLUMNS} FROM eval_grade WHERE tenant_id = $1 AND id = $2");
    let row: Option<EvalGrade> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(grade_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Move a `pending` grade to its terminal state.
///
/// Returns the updated row, or `None` when the row is absent **or** is no longer pending: the
/// `status = 'pending'` predicate is what makes a replayed `eval.grade` job a no-op instead of a
/// second verdict on the same output.
///
/// A `status` outside [`TERMINAL_STATUSES`] is [`EvalStoreError::Invalid`] naming the column's
/// own CHECK, so a caller cannot park a row back in `pending` through this write.
pub async fn set_terminal_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    grade_id: Uuid,
    input: &TerminalGrade<'_>,
) -> Result<Option<EvalGrade>, EvalStoreError> {
    if !TERMINAL_STATUSES.contains(&input.status) {
        return Err(EvalStoreError::Invalid {
            constraint: "eval_grade_status_check".to_owned(),
        });
    }
    let sql = format!(
        "UPDATE eval_grade
            SET status = $3, grade = $4, detail = $5, score = $6, reason = $7, error = $8,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'pending'
        RETURNING {GRADE_COLUMNS}"
    );
    let row: Option<EvalGrade> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(grade_id)
        .bind(input.status)
        .bind(input.grade)
        .bind(input.detail)
        .bind(input.score)
        .bind(input.reason)
        .bind(input.error)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row)
}
