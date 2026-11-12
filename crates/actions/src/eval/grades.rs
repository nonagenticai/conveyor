#![forbid(unsafe_code)]
//! `eval_grades.*` — the one-shot per-output grade (migration 0034; the path
//! `inspection.kind = 'eval'` uses).
//!
//! S19's `eval_runs.*` measures a stored **case set**: the driver invokes a subject per case and
//! grades what it captured. A process step has no case — the output is the step's own — so the
//! SOP's judge needs to grade one value against the eval's current instrument. That is exactly
//! two actions:
//!
//! - [`eval_grades_create`] resolves the eval's **current version**, hashes the output, writes
//!   the `pending` row and enqueues one `eval.grade` job carrying the value to grade. It never
//!   grades inline: a judge call inside the handler's transaction would open a second connection
//!   behind the same row locks (the same reason [`super::graders`] renders but does not invoke).
//! - [`eval_grades_get`] reads the row back. The SOP polls it; `status`, `accepted` and `score`
//!   are the verdict, and `error` is why there is none.
//!
//! ⛔ **The output is hashed, not stored.** `output_hash` is the identity (exactly as
//! `inference_call.prompt_sha256` identifies a prompt) and the value itself rides the job
//! payload. The answer key, when the caller supplies one, is never written to the row either.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{eval_error, not_ready};

/// The job kind one grade under. One job per `eval_grade` row.
pub const GRADE_JOB_KIND: &str = "eval.grade";

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `eval_grades.create` — one output, the eval that judges it, and the acceptance bar.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GradeCreateIn {
    /// The eval whose current version grades the output.
    pub eval_id: Uuid,
    /// The output to grade — the step's product, exactly as the subject produced it.
    pub output: Value,
    /// What the output was produced for, as a judge prompt sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_input: Option<Value>,
    /// The answer key a programmatic/pairwise grader compares against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// Every quality metric must reach this score, `0..=1`.
    pub threshold: f64,
}

/// `eval_grades.get` — one grade by id.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GradeIdIn {
    /// The grade id the create returned.
    pub grade_id: Uuid,
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// What `eval_grades.create` returns: the row's handle and its initial state.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GradeCreatedOut {
    /// The new grade row.
    pub grade_id: String,
    /// Always `pending`; the verdict arrives on the row.
    pub status: String,
}

/// One grade as `eval_grades.get` returns it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GradeView {
    /// The row id.
    pub grade_id: String,
    /// The eval whose metrics judged it.
    pub eval_id: String,
    /// The instrument the grade is bound to.
    pub version_id: String,
    /// The graded output's identity (sha256).
    pub output_hash: String,
    /// `pending | accepted | rejected | failed`.
    pub status: String,
    /// `{metric_key: score}`.
    pub grade: Value,
    /// Per-metric explanation.
    pub detail: Value,
    /// The mean over the quality metrics; `null` until graded.
    pub score: Option<f64>,
    /// The bar the caller judged at.
    pub threshold: f64,
    /// Why the verdict is what it is.
    pub reason: Option<String>,
    /// Why no verdict could be reached.
    pub error: Option<String>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

fn grade_view(row: &store::eval::EvalGrade) -> GradeView {
    GradeView {
        grade_id: row.id.to_string(),
        eval_id: row.eval_id.to_string(),
        version_id: row.version_id.to_string(),
        output_hash: row.output_hash.clone(),
        status: row.status.clone(),
        grade: row.grade.clone(),
        detail: row.detail.clone(),
        score: row.score.map(|score| score.to_f64().unwrap_or_default()),
        threshold: row.threshold.to_f64().unwrap_or_default(),
        reason: row.reason.clone(),
        error: row.error.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Write a `pending` grade and queue the `eval.grade` job that settles it.
#[crate::action(
    name = "eval_grades.create",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_grades_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: GradeCreateIn,
) -> Result<HandlerOutput<GradeCreatedOut>, ActionError> {
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let Some(version_id) = eval.current_version_id else {
        return Err(not_ready(
            ctx.action,
            "the eval has no version, so there is no instrument to grade against",
        ));
    };

    let threshold = Decimal::try_from(input.threshold).map_err(|_| {
        invalid(
            "threshold",
            "invalid_value",
            "the threshold is not a number between 0 and 1",
        )
    })?;
    if threshold < Decimal::ZERO || threshold > Decimal::ONE {
        return Err(invalid(
            "threshold",
            "invalid_value",
            "the threshold must be between 0 and 1",
        ));
    }

    let output_hash = conveyor_domain::compare::content_hash(&input.output);
    let grade_id = store::eval::insert_pending_tx(
        &mut *tx,
        ctx.tenant,
        &store::eval::NewEvalGrade {
            eval_id: eval.id,
            version_id,
            output_hash: &output_hash,
            threshold,
        },
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    // The row and its job ride one transaction: a grade that exists with no job would stay
    // `pending` forever, and a job whose row rolled back would grade nothing.
    let _enqueued = store::jobs::enqueue_tx(
        &mut *tx,
        ctx.tenant,
        GRADE_JOB_KIND,
        &grade_id.to_string(),
        serde_json::json!({
            "grade_id": grade_id,
            "output": input.output,
            "case_input": input.case_input,
            "expected": input.expected,
            "threshold": input.threshold,
        }),
        None,
        conveyor_domain::eval::RetryPolicy::default().max_attempts as i32,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, grade = %grade_id, "enqueueing an eval.grade job failed");
        ActionError::internal()
    })?;

    Ok(HandlerOutput::created(GradeCreatedOut {
        grade_id: grade_id.to_string(),
        status: "pending".to_owned(),
    }))
}

/// One grade by id, in the caller's tenant.
#[crate::action(
    name = "eval_grades.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_grades_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: GradeIdIn,
) -> Result<GradeView, ActionError> {
    let row = store::eval::get_grade_tx(&mut *tx, ctx.tenant, input.grade_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("eval grade"))?;
    Ok(grade_view(&row))
}

/// Render a caller's case input for a judge prompt: a string is itself, anything else its
/// canonical JSON. Absent means the judge sees nothing.
pub fn case_input_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => conveyor_domain::compare::canonical_json(other),
    }
}
