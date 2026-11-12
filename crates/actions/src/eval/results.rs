#![forbid(unsafe_code)]
//! `eval_results.*` — the scored rows, the transcript behind one, and the human label
//! (S19 Part B steps 11–12; CONTRACT §20).
//!
//! ⛔ **The held-out wall is access, not a prompt.** `eval_results.get` returns a result *and its
//! transcript*; when the case's split is `test` the caller must hold `evals:approve`, which
//! `kata_executor` — and therefore every agent principal — deliberately does not. A read that
//! relied on "do not look at the test set" would be a suggestion; this is a `403`.
//!
//! No view here carries `expected`: [`ResultRow`] has no such field, and neither does
//! [`ResultView`]. The answer key is read by the grader only.

use std::str::FromStr;

use conveyor_store as store;
use conveyor_store::eval::{ResultFilter, ResultRow};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{HandlerOutput, Scope};
use crate::views::{Page, PageIn};

use super::{EVAL_RESULT_RECORDED, emit, eval_error};

/// The scope a `test`-split read needs.
const HELD_OUT_SCOPE: Scope = Scope::EvalsApprove;

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `eval_results.list` — one run's rows, filtered.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ResultListIn {
    /// The run whose results are read.
    pub run_id: Uuid,
    /// Only results of a case in this split (`train | test | none`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Only results of a case whose first tag is this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Only results in this status; `pending_label` is the human queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `eval_results.get` — one slot, with its transcript.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ResultGetIn {
    /// The run the slot belongs to.
    pub run_id: Uuid,
    /// The case.
    pub case_id: Uuid,
    /// The repetition.
    pub rep: i32,
}

/// `eval_results.label` — a person's score on a parked row.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ResultLabelIn {
    /// The result to label; it must be `pending_label`.
    pub result_id: Uuid,
    /// The person's score, `0..=1`.
    pub human_score: f64,
    /// Why, in prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One scored case.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ResultView {
    /// The row id.
    pub id: String,
    /// The run it belongs to.
    pub eval_run_id: String,
    /// The case it ran.
    pub case_id: String,
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
    /// Wall-clock latency of the subject call.
    pub latency_ms: i32,
    /// Tool calls the subject made.
    pub tool_calls: i32,
    /// The invocation this attempt ran under.
    pub action_invocation_id: Option<String>,
    /// The subject's own run, when it produced one.
    pub subject_run_id: Option<String>,
    /// The judge's model, when a judge graded it.
    pub judge_model: Option<String>,
    /// A person's score, when one was recorded.
    pub human_score: Option<String>,
    /// `train | test | none`.
    pub split: String,
    /// The case's first tag.
    pub tag: Option<String>,
    /// When the row was written.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// One scored case with the transcript behind it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ResultDetailOut {
    /// The result.
    pub result: ResultView,
    /// The turn list, or `null` when the transcript was written after the result and is absent.
    pub turns: Option<Value>,
    /// The transcript's hash, when there is one.
    pub trace_sha256: Option<String>,
}

fn result_view(row: &ResultRow) -> ResultView {
    ResultView {
        id: row.id.to_string(),
        eval_run_id: row.eval_run_id.to_string(),
        case_id: row.case_id.to_string(),
        rep: row.rep,
        status: row.status.clone(),
        stop_reason: row.stop_reason.clone(),
        grade: row.grade.clone(),
        explanation: row.explanation.clone(),
        model: row.model.clone(),
        usage: row.usage.clone(),
        latency_ms: row.latency_ms,
        tool_calls: row.tool_calls,
        action_invocation_id: row.action_invocation_id.map(|id| id.to_string()),
        subject_run_id: row.subject_run_id.map(|id| id.to_string()),
        judge_model: row.judge_model.clone(),
        human_score: row.human_score.map(|score| score.to_string()),
        split: row.split.clone(),
        tag: row.tag.clone(),
        created_at: row.created_at,
    }
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// One page of a run's results, in `(case, rep)` order.
#[crate::action(
    name = "eval_results.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_results_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ResultListIn,
) -> Result<Page<ResultView>, ActionError> {
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);

    // A `test` listing is a held-out read: the wall is the scope, not a filter the caller opts
    // into. `null` (unfiltered) is not held out — the report and the queue need it — but a list
    // that names `test` and does not hold the scope is refused.
    if input.split.as_deref() == Some("test") && !HELD_OUT_SCOPE.granted_to(ctx.role) {
        return Err(held_out(ctx.action));
    }

    let filter = ResultFilter {
        split: input.split.as_deref(),
        tag: input.tag.as_deref(),
        status: input.status.as_deref(),
    };
    let mut rows = store::eval::list_results_tx(
        &mut *tx,
        ctx.tenant,
        input.run_id,
        filter,
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| format!("{}:{}", row.case_id, row.rep))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(result_view).collect(),
        next_cursor,
    })
}

/// One result and its transcript. A `test`-split slot needs `evals:approve`.
#[crate::action(
    name = "eval_results.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_results_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ResultGetIn,
) -> Result<ResultDetailOut, ActionError> {
    let row =
        store::eval::get_result_tx(&mut *tx, ctx.tenant, input.run_id, input.case_id, input.rep)
            .await
            .map_err(|error| eval_error(error, ctx.action))?
            .ok_or_else(|| ActionError::resource_not_found("eval case result"))?;

    if row.split == "test" && !HELD_OUT_SCOPE.granted_to(ctx.role) {
        return Err(held_out(ctx.action));
    }

    let trace =
        store::eval::get_trace_tx(&mut *tx, ctx.tenant, input.run_id, input.case_id, input.rep)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;

    Ok(ResultDetailOut {
        result: result_view(&row),
        turns: trace.as_ref().map(|trace| trace.turns.clone()),
        trace_sha256: trace.map(|trace| trace.sha256),
    })
}

/// Record a person's score on a parked result and release it to `ok`.
#[crate::action(
    name = "eval_results.label",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_results_label(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ResultLabelIn,
) -> Result<HandlerOutput<ResultDetailOut>, ActionError> {
    if !(0.0..=1.0).contains(&input.human_score) {
        return Err(invalid(
            "human_score",
            "invalid_value",
            "`human_score` must be between 0 and 1",
        ));
    }
    let score = Decimal::from_str(&input.human_score.to_string()).map_err(|_| {
        invalid(
            "human_score",
            "invalid_value",
            "`human_score` must be a finite decimal",
        )
    })?;

    let row = store::eval::label_result_tx(
        &mut *tx,
        ctx.tenant,
        input.result_id,
        score,
        input.note.as_deref(),
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    emit(
        tx,
        &ctx,
        row.eval_run_id,
        row.id,
        &row.status,
        EVAL_RESULT_RECORDED,
        serde_json::json!({"status": row.status, "human_score": input.human_score}),
    )
    .await?;

    Ok(HandlerOutput::created(ResultDetailOut {
        result: result_view(&row),
        turns: None,
        trace_sha256: None,
    }))
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────

fn held_out(action: &str) -> ActionError {
    ActionError::new(
        403,
        "held_out_split",
        format!(
            "{action}: this case is in the held-out test split; reading it needs evals:approve, \
             which a machine principal does not hold"
        ),
    )
}

/// Decode `eval_results.list`'s opaque cursor: `<case uuid>:<rep>`.
fn decode_cursor(raw: Option<&str>) -> Result<Option<(Uuid, i32)>, ActionError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let (id, rep) = raw
        .split_once(':')
        .ok_or_else(|| invalid("cursor", "invalid_value", "the cursor is malformed"))?;
    let id = Uuid::from_str(id).map_err(|_| {
        invalid(
            "cursor",
            "invalid_value",
            "the cursor's case id is malformed",
        )
    })?;
    let rep: i32 = rep
        .parse()
        .map_err(|_| invalid("cursor", "invalid_value", "the cursor's rep is malformed"))?;
    Ok(Some((id, rep)))
}
