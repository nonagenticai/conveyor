#![forbid(unsafe_code)]
//! `eval_case_result` and `eval_attempt_error` — the `(run, case, rep)` slot and the failed
//! attempts that never occupy it (S19 Part A step 3; CONTRACT §20).
//!
//! The driver that fills these is Part B; this module owns the two writes and the guarantees
//! they carry, because both are schema facts rather than driver conventions:
//!
//! - **One result per slot.** `eval_case_result_slot` is
//!   `(tenant_id, eval_run_id, case_id, rep)`; a second write is [`EvalStoreError::Conflict`]
//!   naming it. A re-run of a case is a new `rep` or a new run, never an overwrite.
//! - **A failed attempt is not a slot.** `eval_attempt_error` is append-only and has its own
//!   uniqueness (`eval_attempt_unique`, one row per `(run, case, rep, attempt)`), so a resume
//!   re-runs exactly the attempts that produced no scorable output and never reuses a row whose
//!   score and trace came from different calls. A later success writes `eval_case_result` and
//!   leaves the error rows in place, superseded but never deleted.
//!
//! Both tables carry no foreign keys to `eval_run` or `eval_case` (0026 says so deliberately: the
//! driver has already loaded the run and the case in the caller's tenant before it scores them).
//! Nothing here re-validates a tenancy the schema does not claim; the rows are still confined to
//! the caller's tenant by the forced RLS policy on `tenant_id`.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// One scored case attempt, written once per `(run, case, rep)` slot.
#[derive(Clone, Debug)]
pub struct NewCaseResult<'a> {
    /// The run this result belongs to.
    pub eval_run_id: Uuid,
    /// The case the attempt ran.
    pub case_id: Uuid,
    /// The repetition inside the run, from 0.
    pub rep: i32,
    /// `ok | truncated | pending_label`.
    pub status: &'a str,
    /// Why generation stopped, when it did.
    pub stop_reason: Option<&'a str>,
    /// The grade object; a JSON **object**, never a scalar (with more than one quality metric a
    /// scalar renders every column as a dash and is read as a zero).
    pub grade: &'a Value,
    /// Per-metric explanation, when the grader produced one.
    pub explanation: &'a Value,
    /// The model that produced the output.
    pub model: &'a str,
    /// Token usage of the subject call.
    pub usage: &'a Value,
    /// Wall-clock latency of the subject call, in milliseconds.
    pub latency_ms: i32,
    /// Tool calls the subject made.
    pub tool_calls: i32,
    /// The `action_invocation` this attempt ran under, when there is one.
    pub action_invocation_id: Option<Uuid>,
    /// The subject's own `run`, when the subject was a conveyor run.
    pub subject_run_id: Option<Uuid>,
    /// The judge's model, when a judge graded the attempt.
    pub judge_model: Option<&'a str>,
    /// The judge's token usage.
    pub judge_usage: Option<&'a Value>,
    /// A human's overriding score, when one was recorded.
    pub human_score: Option<Decimal>,
}

/// One failed attempt, appended beside the slot it never filled.
#[derive(Clone, Debug)]
pub struct NewAttemptError<'a> {
    /// The run this attempt belongs to.
    pub eval_run_id: Uuid,
    /// The case the attempt ran.
    pub case_id: Uuid,
    /// The repetition inside the run, from 0.
    pub rep: i32,
    /// The attempt number inside the slot, from 1.
    pub attempt: i32,
    /// `refusal | harness | serving | timeout | served_model_mismatch | grader`.
    pub failure_class: &'a str,
    /// How many retries had been issued when this attempt failed.
    pub retry_count: i32,
    /// The model that was serving, when one was.
    pub model: Option<&'a str>,
    /// Token usage of the failed call, when it was billed.
    pub usage: Option<&'a Value>,
    /// The failure message (non-empty).
    pub message: &'a str,
    /// When the attempt was abandoned, when a deadline applied.
    pub deadline_at: Option<DateTime<Utc>>,
}

/// Write one case result into its `(run, case, rep)` slot.
///
/// Returns the new row id. A second write for the same slot is [`EvalStoreError::Conflict`]
/// carrying `eval_case_result_slot`; a scalar `grade` is [`EvalStoreError::Invalid`] carrying
/// `eval_case_result_grade_object`.
pub async fn insert_case_result_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewCaseResult<'_>,
) -> Result<Uuid, EvalStoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_case_result (tenant_id, eval_run_id, case_id, rep, status,
                                       stop_reason, grade, explanation, model, usage, latency_ms,
                                       tool_calls, action_invocation_id, subject_run_id,
                                       judge_model, judge_usage, human_score)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(input.eval_run_id)
    .bind(input.case_id)
    .bind(input.rep)
    .bind(input.status)
    .bind(input.stop_reason)
    .bind(input.grade)
    .bind(input.explanation)
    .bind(input.model)
    .bind(input.usage)
    .bind(input.latency_ms)
    .bind(input.tool_calls)
    .bind(input.action_invocation_id)
    .bind(input.subject_run_id)
    .bind(input.judge_model)
    .bind(input.judge_usage)
    .bind(input.human_score)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_write_error)?;
    Ok(id)
}

/// Append one failed attempt.
///
/// Returns the new row id. The write is append-only: it never touches `eval_case_result`, so the
/// slot stays empty for the retry and the evidence of the failure survives the later success. A
/// replayed `(run, case, rep, attempt)` is [`EvalStoreError::Conflict`] carrying
/// `eval_attempt_unique` — one attempt is one row.
pub async fn insert_attempt_error_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewAttemptError<'_>,
) -> Result<Uuid, EvalStoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_attempt_error (tenant_id, eval_run_id, case_id, rep, attempt,
                                         failure_class, retry_count, model, usage, message,
                                         deadline_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(input.eval_run_id)
    .bind(input.case_id)
    .bind(input.rep)
    .bind(input.attempt)
    .bind(input.failure_class)
    .bind(input.retry_count)
    .bind(input.model)
    .bind(input.usage)
    .bind(input.message)
    .bind(input.deadline_at)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_write_error)?;
    Ok(id)
}

// ── reads, the label write, and the attempt counter (Part B step 10) ────────────────────────

/// Why the `label` write refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelRefusal {
    /// The row's status is `ok` or `truncated`; only `pending_label` rows are labelled.
    NotPending,
}

/// One stored result as a reader sees it.
///
/// ⛔ There is no `expected` field. The answer key lives in `eval_case` and is read only by
/// [`super::cases::get_case_expected_tx`], inside the grader; this type is what every action and
/// every view returns, and [`RESULT_COLUMNS`] never selects the column.
#[derive(Clone, Debug)]
pub struct ResultRow {
    /// The row id.
    pub id: Uuid,
    /// The run it belongs to.
    pub eval_run_id: Uuid,
    /// The case it ran.
    pub case_id: Uuid,
    /// The repetition.
    pub rep: i32,
    /// `ok | truncated | pending_label`.
    pub status: String,
    /// Why generation stopped, when it did.
    pub stop_reason: Option<String>,
    /// `{metric_key: score}` — always an object.
    pub grade: Value,
    /// Per-metric explanation.
    pub explanation: Value,
    /// The model that produced the output, from the response.
    pub model: String,
    /// The subject call's token usage.
    pub usage: Value,
    /// Wall-clock latency.
    pub latency_ms: i32,
    /// Tool calls the subject made.
    pub tool_calls: i32,
    /// The invocation this attempt ran under.
    pub action_invocation_id: Option<Uuid>,
    /// The subject's own run, when it produced one.
    pub subject_run_id: Option<Uuid>,
    /// The judge's model.
    pub judge_model: Option<String>,
    /// The judge's token usage.
    pub judge_usage: Option<Value>,
    /// A person's overriding score.
    pub human_score: Option<Decimal>,
    /// When the row was written.
    pub created_at: DateTime<Utc>,
    /// The case's split (`train | test | none`) — the held-out wall reads this.
    pub split: String,
    /// The case's first tag, when it has one.
    pub tag: Option<String>,
}

/// One failed attempt, as a reader sees it.
#[derive(Clone, Debug)]
pub struct AttemptError {
    /// The row id.
    pub id: Uuid,
    /// The case it ran.
    pub case_id: Uuid,
    /// The repetition.
    pub rep: i32,
    /// The attempt number inside the slot, from 1.
    pub attempt: i32,
    /// `refusal | harness | serving | timeout | served_model_mismatch | grader`.
    pub failure_class: String,
    /// How many retries had been issued.
    pub retry_count: i32,
    /// The model that was serving, when one was.
    pub model: Option<String>,
    /// The failed call's usage, when it was billed.
    pub usage: Option<Value>,
    /// The failure message.
    pub message: String,
    /// The ceiling that fired, when one did.
    pub deadline_at: Option<DateTime<Utc>>,
    /// When the attempt was abandoned.
    pub created_at: DateTime<Utc>,
}

/// A result page's filters.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResultFilter<'a> {
    /// Only results of a case in this split.
    pub split: Option<&'a str>,
    /// Only results of a case whose first tag is this.
    pub tag: Option<&'a str>,
    /// Only results in this status (`pending_label` is the human queue).
    pub status: Option<&'a str>,
}

const RESULT_COLUMNS: &str = "c.id, c.eval_run_id, c.case_id, c.rep, c.status, c.stop_reason, \
                              c.grade, c.explanation, c.model, c.usage, c.latency_ms, \
                              c.tool_calls, c.action_invocation_id, c.subject_run_id, \
                              c.judge_model, c.judge_usage, c.human_score, c.created_at, \
                              k.split, CASE WHEN cardinality(k.tags) > 0 THEN k.tags[1] END AS tag";

#[derive(Debug, sqlx::FromRow)]
struct ResultDbRow {
    id: Uuid,
    eval_run_id: Uuid,
    case_id: Uuid,
    rep: i32,
    status: String,
    stop_reason: Option<String>,
    grade: Value,
    explanation: Value,
    model: String,
    usage: Value,
    latency_ms: i32,
    tool_calls: i32,
    action_invocation_id: Option<Uuid>,
    subject_run_id: Option<Uuid>,
    judge_model: Option<String>,
    judge_usage: Option<Value>,
    human_score: Option<Decimal>,
    created_at: DateTime<Utc>,
    split: String,
    tag: Option<String>,
}

impl ResultDbRow {
    fn into_row(self) -> ResultRow {
        ResultRow {
            id: self.id,
            eval_run_id: self.eval_run_id,
            case_id: self.case_id,
            rep: self.rep,
            status: self.status,
            stop_reason: self.stop_reason,
            grade: self.grade,
            explanation: self.explanation,
            model: self.model,
            usage: self.usage,
            latency_ms: self.latency_ms,
            tool_calls: self.tool_calls,
            action_invocation_id: self.action_invocation_id,
            subject_run_id: self.subject_run_id,
            judge_model: self.judge_model,
            judge_usage: self.judge_usage,
            human_score: self.human_score,
            created_at: self.created_at,
            split: self.split,
            tag: self.tag,
        }
    }
}

/// One keyset page of a run's results, ordered by `(case_id, rep)`.
///
/// `eval_case` is joined only for `split` and the first tag — the answer-key column is never
/// selected, so a page of results cannot carry one even by accident.
pub async fn list_results_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    filter: ResultFilter<'_>,
    cursor: Option<(Uuid, i32)>,
    limit: i64,
) -> Result<Vec<ResultRow>, EvalStoreError> {
    let sql = format!(
        "SELECT {RESULT_COLUMNS}
           FROM eval_case_result c
           JOIN eval_case k ON k.tenant_id = c.tenant_id AND k.id = c.case_id
          WHERE c.tenant_id = $1
            AND c.eval_run_id = $2
            AND ($3::text IS NULL OR c.status = $3)
            AND ($4::text IS NULL OR k.split = $4)
            AND ($5::text IS NULL OR (cardinality(k.tags) > 0 AND k.tags[1] = $5))
            AND ($6::uuid IS NULL OR (c.case_id, c.rep) > ($6, $7))
          ORDER BY c.case_id ASC, c.rep ASC
          LIMIT $8"
    );
    let rows: Vec<ResultDbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(filter.status)
        .bind(filter.split)
        .bind(filter.tag)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(ResultDbRow::into_row).collect())
}

/// One result by slot, if it exists.
pub async fn get_result_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
) -> Result<Option<ResultRow>, EvalStoreError> {
    let sql = format!(
        "SELECT {RESULT_COLUMNS}
           FROM eval_case_result c
           JOIN eval_case k ON k.tenant_id = c.tenant_id AND k.id = c.case_id
          WHERE c.tenant_id = $1 AND c.eval_run_id = $2 AND c.case_id = $3 AND c.rep = $4"
    );
    let row: Option<ResultDbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(case_id)
        .bind(rep)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(ResultDbRow::into_row))
}

/// Record a person's score on a `pending_label` result and release it to `ok`.
///
/// The status change and the score move together: a row that kept `pending_label` with a
/// `human_score` set would be excluded from the report while claiming to be labelled, which is
/// exactly what `eval_case_result_label`'s CHECK forbids. A row that is not `pending_label`
/// returns [`EvalStoreError::Label`] rather than overwriting an automated score.
pub async fn label_result_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    result_id: Uuid,
    human_score: Decimal,
    note: Option<&str>,
) -> Result<ResultRow, EvalStoreError> {
    let sql = format!(
        "WITH updated AS (
             UPDATE eval_case_result
                SET status = 'ok',
                    human_score = $3,
                    explanation = CASE WHEN $4::text IS NULL THEN explanation
                                       ELSE jsonb_set(explanation, '{{human_note}}',
                                                      to_jsonb($4::text), true) END
              WHERE tenant_id = $1 AND id = $2 AND status = 'pending_label'
              RETURNING id, eval_run_id, case_id, rep, status, stop_reason, grade, explanation,
                        model, usage, latency_ms, tool_calls, action_invocation_id,
                        subject_run_id, judge_model, judge_usage, human_score, created_at
         )
         SELECT {RESULT_COLUMNS}
           FROM updated c
           JOIN eval_case k ON k.tenant_id = $1 AND k.id = c.case_id"
    );
    let row: Option<ResultDbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(result_id)
        .bind(human_score)
        .bind(note)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => Ok(row.into_row()),
        None => {
            // Distinguish "no such row" from "not pending": the caller's refusal differs.
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT status FROM eval_case_result WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(result_id)
            .fetch_optional(&mut *conn)
            .await?;
            match exists {
                Some(_) => Err(EvalStoreError::Label(LabelRefusal::NotPending)),
                None => Err(EvalStoreError::NotFound {
                    what: "eval case result",
                }),
            }
        }
    }
}

/// The next attempt number for a slot, from 1.
///
/// `eval_attempt_error` is append-only and unique on `(run, case, rep, attempt)`, so the maximum
/// plus one is both the next number and the guarantee that no attempt is written twice. A
/// lease-expiry overlap makes the second insert a [`EvalStoreError::Conflict`] on
/// `eval_attempt_unique`, which is the correct outcome: one attempt, one row.
pub async fn next_attempt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
) -> Result<i32, EvalStoreError> {
    let (next,): (i32,) = sqlx::query_as(
        "SELECT COALESCE(MAX(attempt), 0) + 1
           FROM eval_attempt_error
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = $4",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(case_id)
    .bind(rep)
    .fetch_one(&mut *conn)
    .await?;
    Ok(next)
}

/// Every failed attempt for a run, ordered by slot and then attempt.
pub async fn attempt_errors_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<AttemptError>, EvalStoreError> {
    let rows: Vec<AttemptRow> = sqlx::query_as(
        "SELECT id, case_id, rep, attempt, failure_class, retry_count, model, usage, message,
                deadline_at, created_at
           FROM eval_attempt_error
          WHERE tenant_id = $1 AND eval_run_id = $2
          ORDER BY case_id ASC, rep ASC, attempt ASC",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(AttemptRow::into_error).collect())
}

#[derive(Debug, sqlx::FromRow)]
struct AttemptRow {
    id: Uuid,
    case_id: Uuid,
    rep: i32,
    attempt: i32,
    failure_class: String,
    retry_count: i32,
    model: Option<String>,
    usage: Option<Value>,
    message: String,
    deadline_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl AttemptRow {
    fn into_error(self) -> AttemptError {
        AttemptError {
            id: self.id,
            case_id: self.case_id,
            rep: self.rep,
            attempt: self.attempt,
            failure_class: self.failure_class,
            retry_count: self.retry_count,
            model: self.model,
            usage: self.usage,
            message: self.message,
            deadline_at: self.deadline_at,
            created_at: self.created_at,
        }
    }
}
