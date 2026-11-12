#![forbid(unsafe_code)]
//! `eval_climbs.*` — the hillclimb's six actions (S20; SPEC D26, CONTRACT §20).
//!
//! The loop is deliberately **not** autonomous: `eval_climbs.round` creates one round, enqueues its
//! run and returns. A person (or an agent holding the scope) calls it again after the run finishes,
//! which is when the previous round is settled — kept or reverted, with the reason recorded on the
//! run. That cadence is the method's, and it is why there is no second driver here: a round is an
//! `eval_run`, so the eval tick and the reaper already supervise it.
//!
//! Five rules are enforced here rather than documented and hoped for:
//!
//! - **No round before the plan is signed.** `start` writes `gate='eval_climb_plan'` carrying the
//!   goal, the will/won't-change table and the stop condition; `round` refuses `409 plan_pending`
//!   until a tenant-admin decides it.
//! - **The split is not the climb's.** No input carries a split and none is written; the headline
//!   is the `test` half S19 drew and locked.
//! - **The analyzer never reads a held-out transcript.** `round` records the trace ids it was handed
//!   and refuses `422 test_transcript_in_round` if any belongs to a `test`-split case.
//! - **One round is one hypothesis.** The prediction must name the climb's target metric; a round
//!   that predicts two things is stacking.
//! - **Spend is summed, never stored.** `get` prices the result rows at read time; the ceiling is
//!   checked against that sum, so it cannot drift from what it counts.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_domain::TenantId;
use conveyor_domain::eval::climb::{
    BucketReport, ClimbError, Decision, Goal, Hold, Movement, PriorPattern, RoundOutcome,
    StopReason, Stopping, content_round_refused, delta_above_floor, keep_or_revert,
    stopping_reached, triage_required,
};
use conveyor_domain::eval::{Readiness, RunStatus, Variant};
use conveyor_store as store;
use conveyor_store::eval::climb as climb_store;
use conveyor_store::eval::climb::Climb;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::run_core::{RunRequest, all_results, start_or_resume_run_tx};
use super::{EVAL_CLIMB_CHANGED, EVAL_CLIMB_ROUND_COMPLETED, emit, eval_error, not_ready};

/// How many runs are read per page when a climb's rounds are loaded.
const RUN_PAGE: i64 = 200;

/// Whether a run's work is over and its numbers can be read.
///
/// ⛔ **Not `RunStatus::is_terminal()`**, which S19 defines as *cannot change again* — only `void`
/// and `cancelled`, because a `complete` run is resumable through `eval_runs.start`. A climb needs
/// the opposite question answered: `complete` and `partial` are measurable, `void` and `cancelled`
/// are not, and `queued`/`running` are still moving. Reading `is_terminal` here made every
/// finished baseline look absent, which is how this comment came to exist.
fn finished(status: RunStatus) -> bool {
    status.finished()
}

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `eval_climbs.start` — the goal, the budget and the stopping condition.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClimbStartIn {
    /// The eval to climb; its current version is the instrument every round runs under.
    pub eval_id: Uuid,
    /// `{target, direction, hold[]}`. A `cost_usd` target needs at least one quality hold.
    pub goal: Value,
    /// `plateau | rounds | manual`; default `plateau`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopping: Option<String>,
    /// Flat rounds that end a plateau; default 3, never below 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plateau_k: Option<i32>,
    /// The round count, required when `stopping = rounds`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<i32>,
    /// The noise band. Must sit above the measured floor for this sample size.
    pub delta: f64,
    /// Repetitions per case, default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reps: Option<i32>,
    /// The spend ceiling, in cents, when the caller sets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_ceiling_cents: Option<i32>,
    /// Whether every round needs its own sign-off. Default **true**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approve_each_round: Option<bool>,
    /// Patch path prefixes a round must not touch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_limits: Option<Vec<String>>,
}

/// `eval_climbs.get` / `eval_climbs.stop` — one climb by id.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClimbIdIn {
    /// The climb.
    pub id: Uuid,
}

/// `eval_climbs.stop` — close the climb and write its report.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClimbStopIn {
    /// The climb.
    pub id: Uuid,
    /// Why it is being closed.
    pub reason: String,
}

/// `eval_climbs.list` — the caller's climbs.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClimbListIn {
    /// Only climbs of this eval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_id: Option<Uuid>,
    /// Only climbs in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The cursor from the previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Page size, 1–100. Default 20.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

/// `eval_climbs.compare` — the paired comparison of two runs.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClimbCompareIn {
    /// The earlier run.
    pub a_run_id: Uuid,
    /// The later run.
    pub b_run_id: Uuid,
    /// The metric compared; defaults to the version's single quality metric.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// The score a case must reach to count as resolved. Default 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_threshold: Option<f64>,
}

/// `eval_climbs.round` — one hypothesis.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RoundIn {
    /// The climb.
    pub id: Uuid,
    /// Resume round N instead of creating a new one. Required under `approve_each_round` once the
    /// round's gate has been decided, because the run already exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<i32>,
    /// What changed, in prose. For a categorisation round this is replaced by the bucket table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_md: Option<String>,
    /// The patch under test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_patch: Option<String>,
    /// The surface being changed (a file, a prompt, a step); recorded, not enforced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The pre-registered prediction. Required for a content round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction: Option<Value>,
    /// The categorisation, required once two consecutive rounds have gone flat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buckets: Option<Value>,
    /// The cost-walk lever this round is fought at. Required for a `cost_usd` goal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lever: Option<String>,
    /// The `eval_trace` ids the analyzer was handed. Any `test`-split id voids the round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_trace_ids: Option<Vec<Uuid>>,
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One climb, as every climb action returns it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClimbView {
    /// The climb id.
    pub id: String,
    /// The eval it climbs.
    pub eval_id: String,
    /// The frozen instrument.
    pub version_id: String,
    /// `{target, direction, hold[], lever, stop}`.
    pub goal: Value,
    /// The metric being moved.
    pub target: String,
    /// `higher | lower`.
    pub direction: String,
    /// Repetitions per case.
    pub reps: i32,
    /// `plateau | rounds | manual`.
    pub stopping: String,
    /// Flat rounds that end a plateau.
    pub plateau_k: i32,
    /// The round count, when `stopping = rounds`.
    pub max_rounds: Option<i32>,
    /// The noise band.
    pub delta: f64,
    /// Patch path prefixes the climb must not touch.
    pub off_limits: Vec<String>,
    /// Whether every round needs its own sign-off.
    pub approve_each_round: bool,
    /// The spend ceiling, when the caller set one.
    pub cost_ceiling_cents: Option<i32>,
    /// How many rounds have been created.
    pub current_round: i32,
    /// The round with the best held-out score.
    pub best_round: Option<i32>,
    /// That round's held-out score.
    pub best_test_score: Option<f64>,
    /// `planned | running | plateaued | stopped`.
    pub status: String,
    /// Cumulative spend, summed from the result rows at read time. `null` on `list`, which does
    /// not read them.
    pub spend_cents: Option<f64>,
    /// `cents` when the version prices its tokens, `tokens` when it does not.
    pub spend_unit: String,
    /// The final report, once the climb stops.
    pub narrative_md: Option<String>,
    /// Who started it.
    pub created_by: Option<String>,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When it last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// The rounds, oldest first. Empty on `list`.
    pub rounds: Vec<RoundSummary>,
}

/// One round, as the climb's read model reports it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RoundSummary {
    /// The round number.
    pub round: i32,
    /// The run that measured it.
    pub run_id: String,
    /// `v<N>`.
    pub variant: String,
    /// `queued | running | partial | complete | void | cancelled`.
    pub status: String,
    /// The train mean, when the run is terminal.
    pub train: Option<f64>,
    /// The held-out mean, the headline.
    pub test: Option<f64>,
    /// The held-out movement against the previous round.
    pub delta: Option<f64>,
    /// `keep | revert`, once settled.
    pub decision: Option<String>,
    /// Why, when reverted.
    pub reason: Option<String>,
    /// What changed.
    pub change_md: Option<String>,
}

/// One round, as `eval_climbs.round` returns it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RoundOut {
    /// The climb.
    pub climb_id: String,
    /// The round number.
    pub round: i32,
    /// `v<N>`.
    pub variant: String,
    /// The run created (or resumed) for this round.
    pub run_id: String,
    /// How many case jobs this call enqueued; `0` when the round's gate is still pending.
    pub queued_jobs: usize,
    /// How many slots were already filled.
    pub skipped_slots: usize,
    /// The pending sign-off, when `approve_each_round` is set and nothing has been enqueued yet.
    pub awaiting_approval: Option<String>,
}

/// The paired comparison's answer.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CompareOut {
    /// Whether the two runs measured the same thing. Always `true` here; a refusal is a `409`.
    pub comparable: bool,
    /// The metric compared.
    pub metric: String,
    /// Cases both runs resolved.
    pub both: usize,
    /// Cases neither resolved.
    pub neither: usize,
    /// Cases only `a` resolved.
    pub a_only: usize,
    /// Cases only `b` resolved.
    pub b_only: usize,
    /// `a_only + b_only`, the discordant pairs the exact test counts.
    pub discordant: usize,
    /// Cases present in exactly one run and excluded from the pairing.
    pub missing_in_one: usize,
    /// The exact two-sided McNemar p-value.
    pub p_value: f64,
    /// Mean paired score difference, `b - a`.
    pub delta: f64,
    /// The lower end of the paired interval.
    pub ci_low: f64,
    /// The upper end.
    pub ci_high: f64,
    /// Whether the difference is significant at `p < 0.05` **and** in `b`'s favour.
    ///
    /// This is the comparison action's own test, not the climb's band: it reads two runs and has
    /// no climb to ask, so it cannot apply `eval_climb.delta`. A climb's keep/revert decision is
    /// `domain::eval::climb::keep_or_revert`, which does.
    pub significant: bool,
}

impl From<ClimbError> for ActionError {
    fn from(error: ClimbError) -> Self {
        match error {
            ClimbError::Vocabulary { what, value } => invalid(
                what,
                "invalid_value",
                format!("{what} must be one of the documented values, not {value:?}"),
            ),
            ClimbError::Goal(detail) => invalid("goal", "invalid_value", detail),
            ClimbError::Prediction(detail) => invalid("prediction", "invalid_value", detail),
            ClimbError::Buckets(detail) => invalid("buckets", "invalid_value", detail),
            ClimbError::Delta => invalid("delta", "invalid_value", "delta must be positive"),
        }
    }
}

fn as_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or_default()
}

/// `f64` → `NUMERIC`, refusing a value the column cannot hold.
fn as_decimal(value: f64) -> Result<Decimal, ActionError> {
    Decimal::try_from(value).map_err(|_| {
        invalid(
            "delta",
            "invalid_value",
            "delta is out of range for a numeric",
        )
    })
}

// ── pricing ─────────────────────────────────────────────────────────────────────────────

/// Price one result's `usage` against the version's `prices`, in cents.
///
/// `prices` is the version's free-form jsonb; the two keys this understands are `input` and
/// `output`, US dollars per million tokens (the shape `eval_version.prices` carries in the
/// domain's own tests). Anything else — no prices, a missing rate, a usage object that names no
/// token counts — returns `None`, and the climb then reports **tokens** instead of cents rather
/// than inventing a rate.
pub(crate) fn price_usage(usage: &Value, prices: &Value) -> Option<f64> {
    let input_rate = prices.get("input").and_then(Value::as_f64)?;
    let output_rate = prices.get("output").and_then(Value::as_f64)?;
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_f64)?;
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    // Cents: (tokens / 1e6) dollars * 100.
    Some((input * input_rate + output * output_rate) / 10_000.0)
}

/// The unpriced fallback: how many tokens a result cost.
pub(crate) fn tokens_of(usage: &Value) -> f64 {
    usage
        .get("total_tokens")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            let input = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let output = usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            input + output
        })
}

// ── reading a run ───────────────────────────────────────────────────────────────────────

/// One run's numbers, computed in Rust from its result rows (never a SQL aggregate).
struct RunNumbers {
    outcome: RoundOutcome,
    /// Cumulative spend for this run, in the unit [`Pricing::unit`] names.
    spend: f64,
    /// How many rows were left out of every mean (not `ok`, or ungraded).
    excluded: usize,
}

/// Whether the version prices its tokens, and if so how.
struct Pricing {
    /// `cents` or `tokens`.
    unit: &'static str,
    /// The version's price document.
    prices: Value,
}

async fn run_numbers(
    tx: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    round: i32,
    goal: &Goal,
    pricing: &Pricing,
    action: &str,
) -> Result<RunNumbers, ActionError> {
    let rows = all_results(tx, tenant, run_id, action).await?;
    let mut train = Vec::new();
    let mut test = Vec::new();
    let mut cost = Vec::new();
    let mut holds: Vec<(String, Vec<f64>)> = goal
        .hold
        .iter()
        .map(|hold: &Hold| (hold.metric.clone(), Vec::new()))
        .collect();
    let mut excluded = 0_usize;

    for row in &rows {
        let priced = match pricing.unit {
            "cents" => price_usage(&row.usage, &pricing.prices).unwrap_or_default(),
            _ => tokens_of(&row.usage),
        };
        // A result counts towards *something* even when ungraded: its spend is real money.
        if row.status != "ok" {
            excluded += 1;
            continue;
        }
        // ⛔ Spend and guardrails are measured for EVERY scorable row, not only the ones carrying
        // the target's grade. A `cost_usd` climb has no `cost_usd` grade key at all, so gating
        // these on the target left its cost means at zero (every round reverted `cost_not_cleared`)
        // and its hold readings empty (the guardrail passed vacuously — the one thing it exists to
        // stop).
        cost.push(priced);
        for (metric, values) in holds.iter_mut() {
            if let Some(value) = row.grade.get(metric.as_str()).and_then(Value::as_f64) {
                values.push(value);
            }
        }
        match row.grade.get(&goal.target).and_then(Value::as_f64) {
            Some(score) => match row.split.as_str() {
                "train" => train.push(score),
                "test" => test.push(score),
                _ => {}
            },
            // A quality goal's row without its target is unmeasurable; a cost round's rows are
            // not (their cost and holds are above).
            None if !goal.is_cost() => excluded += 1,
            None => {}
        }
    }

    let mean = |values: &[f64]| {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    };
    let hold_readings = holds
        .into_iter()
        .map(|(metric, values)| {
            let declared = goal
                .hold
                .iter()
                .find(|hold| hold.metric == metric)
                .map(|hold| hold.min)
                .unwrap_or(0.0);
            conveyor_domain::eval::climb::HoldReading {
                metric,
                value: mean(&values),
                min: declared,
            }
        })
        .collect();
    let spend = rows
        .iter()
        .map(|row| match pricing.unit {
            "cents" => price_usage(&row.usage, &pricing.prices).unwrap_or_default(),
            _ => tokens_of(&row.usage),
        })
        .sum();

    Ok(RunNumbers {
        outcome: RoundOutcome {
            round,
            train: mean(&train),
            test: mean(&test),
            cost_cents: Some(mean(&cost)),
            n_train: train.len(),
            n_test: test.len(),
            holds: hold_readings,
        },
        spend,
        excluded,
    })
}

/// A climb round's run, with what was recorded on it.
struct RoundRow {
    round: i32,
    run_id: Uuid,
    variant: String,
    status: RunStatus,
    summary: Value,
}

/// Every round created for this climb, oldest first.
async fn climb_rounds(
    tx: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    action: &str,
) -> Result<Vec<RoundRow>, ActionError> {
    let mut rows = Vec::new();
    let mut cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)> = None;
    loop {
        let page =
            store::eval::list_runs_tx(&mut *tx, tenant, Some(eval_id), None, cursor, RUN_PAGE)
                .await
                .map_err(|error| eval_error(error, action))?;
        let short = (page.len() as i64) < RUN_PAGE;
        if let Some(last) = page.last() {
            cursor = Some((last.created_at, last.id));
        }
        for run in page {
            if run.round > 0 {
                rows.push(RoundRow {
                    round: run.round,
                    run_id: run.id,
                    variant: run.variant.as_str(),
                    status: run.status,
                    summary: run.summary,
                });
            }
        }
        if short {
            break;
        }
    }
    rows.sort_by_key(|row| row.round);
    Ok(rows)
}

/// The baseline run (`variant='baseline'`) when it is terminal.
async fn baseline_run(
    tx: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    action: &str,
) -> Result<Option<Uuid>, ActionError> {
    let run = store::eval::find_run_tx(&mut *tx, tenant, eval_id, "baseline")
        .await
        .map_err(|error| eval_error(error, action))?;
    Ok(run.filter(|run| finished(run.status)).map(|run| run.id))
}

// ── settling ────────────────────────────────────────────────────────────────────────────

/// What a settle learned about one round.
struct Settled {
    round: i32,
    run_id: Uuid,
    outcome: RoundOutcome,
    /// The held-out mean of the run this one was measured against (the previous round, or the
    /// baseline for round 1). Kept so the recorded delta is the comparison that was actually made.
    base_test: f64,
    /// Rows left out of the means — not `ok`, or ungraded. Recorded so a round with a thin
    /// measurement is visible rather than quietly averaged over three rows.
    excluded: usize,
    pattern: PriorPattern,
    decision: Decision,
}

/// The whole loop's state, recomputed from the rows.
struct SettleState {
    /// Rounds 1..=current, oldest first.
    settled: Vec<Settled>,
    /// The run of the newest round, when it is not terminal yet.
    in_flight: Option<(i32, Uuid, RunStatus)>,
    /// Consecutive flat rounds, reset by a categorisation round.
    flat_streak: i32,
    /// Cumulative spend across the climb's rounds.
    spend: f64,
}

/// Recompute every round's decision from the rows, and say what is still running.
///
/// Recomputation is deliberate: the rows are the evidence, and a decision persisted from an
/// earlier call is re-derived rather than trusted, so a `stop` after a resumed session cannot
/// inherit a number nobody can reproduce.
async fn settle(
    tx: &mut PgConnection,
    tenant: TenantId,
    climb: &Climb,
    goal: &Goal,
    pricing: &Pricing,
    action: &str,
) -> Result<SettleState, ActionError> {
    let delta = as_f64(climb.delta);
    let rounds = climb_rounds(tx, tenant, climb.eval_id, action).await?;

    let baseline = baseline_run(tx, tenant, climb.eval_id, action).await?;
    let mut base: Option<RoundOutcome> = match baseline {
        Some(run_id) => Some(
            run_numbers(tx, tenant, run_id, 0, goal, pricing, action)
                .await?
                .outcome,
        ),
        None => None,
    };

    let mut settled: Vec<Settled> = Vec::new();
    let mut in_flight = None;
    let mut spend = 0.0_f64;
    let mut prior: Option<PriorPattern> = None;

    for row in &rounds {
        let numbers = run_numbers(tx, tenant, row.run_id, row.round, goal, pricing, action).await?;
        spend += numbers.spend;
        let categorisation = row
            .summary
            .get("categorisation")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !finished(row.status) {
            in_flight = Some((row.round, row.run_id, row.status));
            break;
        }
        let base_outcome = match base.clone() {
            Some(outcome) => outcome,
            None => {
                // Round 1 with no baseline: unmeasurable. Leave it unsettled and let the caller
                // refuse when it tries to create round 2.
                in_flight = Some((row.round, row.run_id, row.status));
                break;
            }
        };
        let decision = keep_or_revert(goal, &base_outcome, &numbers.outcome, prior, delta);
        let pattern = PriorPattern {
            train: goal
                .direction
                .movement(base_outcome.train, numbers.outcome.train, delta),
            test: goal
                .direction
                .movement(base_outcome.test, numbers.outcome.test, delta),
            kept: decision == Decision::Keep,
        };
        prior = Some(pattern);
        base = Some(numbers.outcome.clone());
        settled.push(Settled {
            round: row.round,
            run_id: row.run_id,
            outcome: numbers.outcome,
            base_test: base_outcome.test,
            excluded: numbers.excluded,
            pattern,
            decision,
        });
        // A categorisation round resets the flat streak: it moved the diagnosis, not the number.
        if categorisation {
            prior = Some(PriorPattern {
                train: Movement::Flat,
                test: Movement::Flat,
                kept: false,
            });
        }
    }

    // ⛔ A categorisation round **counts** toward the streak; it does not reset it.
    //
    // It was resetting, and that made `content_round_refused` (streak ≥ 3) unreachable: the
    // mandatory `buckets` round fires at streak 2, and its reset meant no streak could ever reach
    // three — a gate that could not fail. The slice's "three rounds that do not clear the band"
    // includes the categorisation round, because its numbers are still flat, and the fourth
    // content round against that same wall is what must be refused (trap 6).
    let mut flat_streak = 0_i32;
    for round in &settled {
        if round.decision.is_flat() {
            flat_streak += 1;
        } else {
            flat_streak = 0;
        }
    }

    Ok(SettleState {
        settled,
        in_flight,
        flat_streak,
        spend,
    })
}

// ── views ───────────────────────────────────────────────────────────────────────────────

fn round_summaries(rounds: &[RoundRow]) -> Vec<RoundSummary> {
    rounds
        .iter()
        .map(|row| RoundSummary {
            round: row.round,
            run_id: row.run_id.to_string(),
            variant: row.variant.clone(),
            status: row.status.as_str().to_owned(),
            train: row.summary.get("train").and_then(Value::as_f64),
            test: row.summary.get("test").and_then(Value::as_f64),
            delta: row.summary.get("delta").and_then(Value::as_f64),
            decision: row
                .summary
                .get("decision")
                .and_then(Value::as_str)
                .map(str::to_owned),
            reason: row
                .summary
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
            change_md: row
                .summary
                .get("change_md")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect()
}

fn climb_view(climb: &Climb, spend_cents: Option<f64>, unit: &str) -> ClimbView {
    let goal = Goal::parse(&climb.goal).unwrap_or(Goal {
        target: String::new(),
        direction: conveyor_domain::eval::climb::Direction::Higher,
        hold: Vec::new(),
        lever: 0,
        stop: None,
    });
    ClimbView {
        id: climb.id.to_string(),
        eval_id: climb.eval_id.to_string(),
        version_id: climb.version_id.to_string(),
        goal: climb.goal.clone(),
        target: goal.target,
        direction: climb.direction.clone(),
        reps: climb.reps,
        stopping: climb.stopping.clone(),
        plateau_k: climb.plateau_k,
        max_rounds: climb.max_rounds,
        delta: as_f64(climb.delta),
        off_limits: climb.off_limits.clone(),
        approve_each_round: climb.approve_each_round,
        cost_ceiling_cents: climb.cost_ceiling_cents,
        current_round: climb.current_round,
        best_round: climb.best_round,
        best_test_score: climb.best_test_score.map(as_f64),
        status: climb.status.clone(),
        spend_cents,
        spend_unit: unit.to_owned(),
        narrative_md: climb.narrative_md.clone(),
        created_by: climb.created_by.map(|id| id.to_string()),
        created_at: climb.created_at,
        updated_at: climb.updated_at,
        rounds: Vec::new(),
    }
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Start a climb: the goal, the band and the stopping condition, then the plan sign-off.
#[crate::action(
    name = "eval_climbs.start",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_climbs_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClimbStartIn,
) -> Result<HandlerOutput<ClimbView>, ActionError> {
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let version_id = eval
        .current_version_id
        .ok_or_else(|| not_ready(ctx.action, "the eval has no version to climb"))?;

    let facts = store::eval::eval_readiness_tx(&mut *tx, ctx.tenant, eval.id, version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let readiness = Readiness::evaluate(facts);
    if !readiness.is_ready() {
        let gates: Vec<&str> = readiness
            .missing()
            .iter()
            .map(|gate| gate.as_str())
            .collect();
        return Err(not_ready(
            ctx.action,
            &format!(
                "the climb is refused until every gate is satisfied: {}",
                gates.join(", ")
            ),
        ));
    }

    if climb_store::find_open_climb_tx(&mut *tx, ctx.tenant, eval.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .is_some()
    {
        return Err(ActionError::new(
            409,
            "climb_open",
            format!(
                "{}: this eval already has a climb that has not stopped; stop it before starting \
                 another (a second climb continues the variant numbering, so it is not the \
                 numbering that is refused — it is two keepers of one best-so-far)",
                ctx.action
            ),
        ));
    }

    let goal = Goal::parse(&input.goal)?;
    let stopping_raw = input.stopping.as_deref().unwrap_or("plateau");
    let plateau_k = input.plateau_k.unwrap_or(3);
    let stopping = Stopping::parse(stopping_raw, plateau_k, input.rounds)?;
    let reps = input.reps.unwrap_or(1);
    if reps < 1 {
        return Err(invalid(
            "reps",
            "invalid_value",
            "`reps` must be at least 1",
        ));
    }

    // The band must be measurable at this sample size, and the number is said out loud.
    let cases = {
        let mut all = Vec::new();
        let mut cursor: Option<(i32, Uuid)> = None;
        loop {
            let page = store::eval::list_cases_tx(&mut *tx, ctx.tenant, version_id, cursor, 500)
                .await
                .map_err(|error| eval_error(error, ctx.action))?;
            let short = page.len() < 500;
            if let Some(last) = page.last() {
                cursor = Some((last.ordinal, last.id));
            }
            all.extend(page);
            if short {
                break;
            }
        }
        all
    };
    let test_cases = cases.iter().filter(|case| case.split == "test").count();
    if test_cases == 0 {
        return Err(not_ready(
            ctx.action,
            "the version has no held-out cases, so a climb has no headline to report",
        ));
    }
    if let Err(floor) = delta_above_floor(input.delta, test_cases, reps as u32) {
        return Err(ActionError::new(
            422,
            "delta_below_noise",
            format!(
                "{}: delta {} sits at or below the measured noise floor {:.4} for {} held-out \
                 cases at {} reps. A change that small is kept or reverted by chance — set delta \
                 above the floor, or measure with more cases or reps",
                ctx.action, input.delta, floor, test_cases, reps
            ),
        ));
    }

    let max_rounds = match stopping {
        Stopping::Rounds { max_rounds } => Some(max_rounds),
        _ => None,
    };
    let off_limits = input.off_limits.unwrap_or_default();
    let approve_each_round = input.approve_each_round.unwrap_or(true);
    let goal_doc = json!({
        "target": goal.target,
        "direction": goal.direction.as_str(),
        "hold": goal.hold,
        "lever": goal.lever,
    });

    let climb = climb_store::insert_climb_tx(
        &mut *tx,
        ctx.tenant,
        &climb_store::NewClimb {
            eval_id: eval.id,
            version_id,
            goal: &goal_doc,
            direction: goal.direction.as_str(),
            reps,
            stopping: stopping_raw,
            plateau_k,
            max_rounds,
            delta: as_decimal(input.delta)?,
            off_limits: &off_limits,
            approve_each_round,
            cost_ceiling_cents: input.cost_ceiling_cents,
            created_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    // The plan sign-off: the goal, what the climb will and will not change, and how it stops.
    let meta = json!({
        "climb_id": climb.id,
        "goal": goal_doc,
        "will_change": [goal.target],
        "wont_change": off_limits,
        "stop": {
            "stopping": stopping_raw,
            "plateau_k": plateau_k,
            "max_rounds": max_rounds,
        },
        "delta": input.delta,
        "reps": reps,
        "cost_ceiling_cents": input.cost_ceiling_cents,
        "approve_each_round": approve_each_round,
    });
    let mut approval =
        store::approval::NewApproval::new(climb_store::PLAN_GATE, climb_store::GATE_KIND);
    approval.eval_id = Some(eval.id);
    approval.requested_by = Some(ctx.actor_id);
    approval.meta = meta;
    store::approval::insert_approval_tx(&mut *tx, ctx.tenant, &approval)
        .await
        .map_err(|error| match error {
            store::approval::ApprovalStoreError::Conflict => ActionError::new(
                409,
                "gate_pending",
                format!(
                    "{}: a plan sign-off is already pending for this eval",
                    ctx.action
                ),
            ),
            _ => ActionError::internal(),
        })?;

    emit(
        tx,
        &ctx,
        eval.id,
        climb.id,
        "planned",
        EVAL_CLIMB_CHANGED,
        json!({"status": "planned", "target": climb.goal.get("target")}),
    )
    .await?;

    Ok(HandlerOutput::created(climb_view(
        &climb,
        Some(0.0),
        "tokens",
    )))
}

/// One climb, with its rounds and its summed spend.
#[crate::action(
    name = "eval_climbs.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_climbs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClimbIdIn,
) -> Result<HandlerOutput<ClimbView>, ActionError> {
    let climb = climb_store::get_climb_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("climb"))?;
    let goal = Goal::parse(&climb.goal)?;
    let pricing = pricing_of(&mut *tx, ctx.tenant, climb.version_id, ctx.action).await?;
    let state = settle(tx, ctx.tenant, &climb, &goal, &pricing, ctx.action).await?;
    let rounds = climb_rounds(tx, ctx.tenant, climb.eval_id, ctx.action).await?;
    let mut view = climb_view(&climb, Some(state.spend), pricing.unit);
    view.rounds = round_summaries(&rounds);
    Ok(HandlerOutput::ok(view))
}

/// A page of climbs.
#[crate::action(
    name = "eval_climbs.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_climbs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClimbListIn,
) -> Result<HandlerOutput<Page<ClimbView>>, ActionError> {
    let limit = input.limit.unwrap_or(20).clamp(1, 100);
    let cursor = match input.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            Uuid::parse_str(raw)
                .map_err(|_| invalid("cursor", "invalid_value", "the cursor is not a climb id"))?,
        ),
    };
    let rows = climb_store::list_climbs_tx(
        &mut *tx,
        ctx.tenant,
        input.eval_id,
        input.status.as_deref(),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    let has_more = rows.len() as i64 > limit;
    let page: Vec<ClimbView> = rows
        .into_iter()
        .take(limit as usize)
        .map(|climb| climb_view(&climb, None, "tokens"))
        .collect();
    let next = page.last().map(|view| view.id.clone());
    Ok(HandlerOutput::ok(Page {
        items: page,
        next_cursor: if has_more { next } else { None },
    }))
}

/// The version's prices, and the unit the climb reports spend in.
async fn pricing_of(
    tx: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
    action: &str,
) -> Result<Pricing, ActionError> {
    let version = store::eval::get_version_tx(&mut *tx, tenant, version_id)
        .await
        .map_err(|error| eval_error(error, action))?;
    let priced = version
        .prices
        .get("input")
        .and_then(Value::as_f64)
        .is_some()
        && version
            .prices
            .get("output")
            .and_then(Value::as_f64)
            .is_some();
    Ok(Pricing {
        unit: if priced { "cents" } else { "tokens" },
        prices: version.prices.clone(),
    })
}

/// Stop the climb, settle its last round and write the report.
#[crate::action(
    name = "eval_climbs.stop",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_climbs_stop(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClimbStopIn,
) -> Result<HandlerOutput<ClimbView>, ActionError> {
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(invalid("reason", "invalid_value", "a stop needs a reason"));
    }
    let climb = climb_store::lock_climb_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("climb"))?;
    if climb.status == "stopped" {
        return Err(ActionError::new(
            409,
            "climb_stopped",
            format!("{}: the climb is already stopped", ctx.action),
        ));
    }

    let goal = Goal::parse(&climb.goal)?;
    let pricing = pricing_of(&mut *tx, ctx.tenant, climb.version_id, ctx.action).await?;
    let state = settle(tx, ctx.tenant, &climb, &goal, &pricing, ctx.action).await?;
    let rounds = climb_rounds(tx, ctx.tenant, climb.eval_id, ctx.action).await?;

    // Persist every round's decision before the report reads them, so the report can never
    // describe a decision the rows do not carry — and re-read the rounds afterwards, because the
    // view must show what was just written rather than what it read before writing.
    persist_decisions(tx, &ctx, climb.eval_id, &rounds, &state).await?;
    let rounds = climb_rounds(tx, ctx.tenant, climb.eval_id, ctx.action).await?;

    let (best_round, best_score) = best_of(&state, &goal);
    let mut goal_doc = climb.goal.clone();
    if let Some(object) = goal_doc.as_object_mut() {
        object.insert(
            "stop".to_owned(),
            json!({"reason": reason, "at": chrono::Utc::now().to_rfc3339()}),
        );
    }
    let report = build_report(&climb, &rounds, &state, best_round, best_score, reason);
    let best_test_score = match best_score {
        Some(score) => Some(as_decimal(score)?),
        None => None,
    };
    let stopped = climb_store::stop_climb_tx(
        &mut *tx,
        ctx.tenant,
        climb.id,
        &goal_doc,
        &report,
        best_round,
        best_test_score,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    emit(
        tx,
        &ctx,
        climb.eval_id,
        climb.id,
        "stopped",
        EVAL_CLIMB_CHANGED,
        json!({"status": "stopped", "reason": reason, "best_round": best_round}),
    )
    .await?;

    let mut view = climb_view(&stopped, Some(state.spend), pricing.unit);
    view.rounds = round_summaries(&rounds);
    Ok(HandlerOutput::ok(view))
}

// ── the round ──────────────────────────────────────────────────────────────────────────

/// One round: a hypothesis, its run, and — when `approve_each_round` — its own sign-off.
#[crate::action(
    name = "eval_climbs.round",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_climbs_round(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RoundIn,
) -> Result<HandlerOutput<RoundOut>, ActionError> {
    let climb = climb_store::lock_climb_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("climb"))?;
    if climb.status == "stopped" {
        return Err(ActionError::new(
            409,
            "climb_stopped",
            format!(
                "{}: the climb is stopped; nothing more can be run",
                ctx.action
            ),
        ));
    }
    if climb.status == "plateaued" {
        return Err(ActionError::new(
            409,
            "climb_plateaued",
            format!(
                "{}: the climb plateaued — {} consecutive flat rounds. A new target is a new \
                 climb; silently retargeting is how a hillclimb reports a win that is not one",
                ctx.action, climb.plateau_k
            ),
        ));
    }

    // No round before the plan is signed. The gate is the plan; carrying it out is work.
    match climb_store::plan_gate_status_tx(&mut *tx, ctx.tenant, climb.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .as_deref()
    {
        Some("approved") => {}
        Some("pending") | None => {
            return Err(ActionError::new(
                409,
                "plan_pending",
                format!(
                    "{}: the climb's plan sign-off has not been decided; approving the goal, the \
                     will/won't-change table and the stop condition is what authorises round 1",
                    ctx.action
                ),
            ));
        }
        Some(other) => {
            return Err(ActionError::new(
                409,
                "plan_rejected",
                format!(
                    "{}: the plan's sign-off is {other}; a rejected plan cannot be carried out",
                    ctx.action
                ),
            ));
        }
    }

    let mut goal = Goal::parse(&climb.goal)?;
    let pricing = pricing_of(&mut *tx, ctx.tenant, climb.version_id, ctx.action).await?;
    let rounds = climb_rounds(tx, ctx.tenant, climb.eval_id, ctx.action).await?;

    // A resume is the same action: the round's run already exists and only its jobs are missing.
    if let Some(round) = input.round {
        let row = rounds
            .iter()
            .find(|row| row.round == round)
            .ok_or_else(|| {
                ActionError::new(
                    409,
                    "round_absent",
                    format!("{}: round {round} has not been created", ctx.action),
                )
            })?;
        if finished(row.status) {
            return Err(ActionError::new(
                409,
                "round_terminal",
                format!(
                    "{}: round {round} already finished ({}) and cannot be resumed",
                    ctx.action,
                    row.status.as_str()
                ),
            ));
        }
        if climb.approve_each_round
            && climb_store::approved_round_gate_tx(&mut *tx, ctx.tenant, climb.eval_id, round)
                .await
                .map_err(|error| eval_error(error, ctx.action))?
                .is_none()
        {
            return Err(awaiting(ctx.action, round));
        }
        let variant =
            Variant::parse(&row.variant).map_err(|error| eval_error(error.into(), ctx.action))?;
        let case_ids = version_case_ids(tx, ctx.tenant, climb.version_id, ctx.action).await?;
        let environment = environment_of(tx, ctx.tenant, climb.eval_id, ctx.action).await?;
        let started = start_or_resume_run_tx(
            tx,
            &ctx,
            &RunRequest {
                eval_id: climb.eval_id,
                version_id: climb.version_id,
                variant,
                mode: conveyor_domain::eval::RunMode::Full,
                reps: climb.reps,
                case_ids,
                round,
                cost_ceiling_cents: climb.cost_ceiling_cents,
                environment: &environment,
                enqueue: true,
            },
        )
        .await?;
        return Ok(HandlerOutput::created(RoundOut {
            climb_id: climb.id.to_string(),
            round,
            variant: started.run.variant.as_str(),
            run_id: started.run.id.to_string(),
            queued_jobs: started.queued_jobs,
            skipped_slots: started.skipped_slots,
            awaiting_approval: None,
        }));
    }

    // ── a new round ────────────────────────────────────────────────────────────────────
    let state = settle(tx, ctx.tenant, &climb, &goal, &pricing, ctx.action).await?;
    // Persist the decisions of the rounds this call just settled, so `eval_climb.round_completed`
    // fires when a round completes rather than only when the climb is stopped.
    persist_decisions(tx, &ctx, climb.eval_id, &rounds, &state).await?;
    // ⛔ The cost staircase moves on **kept rounds**, derived from the settled rows rather than
    // incremented in place: re-settling the same round (every call re-derives) must not advance it
    // twice. `Goal::advance_lever` existed and was tested, but nothing ever called it, so a cost
    // walk asked the caller for `caching_health` forever.
    if goal.is_cost() {
        let kept = state
            .settled
            .iter()
            .filter(|round| round.decision == Decision::Keep)
            .count();
        while goal.lever < kept {
            goal.advance_lever();
        }
    }
    if let Some((round, _run, status)) = state.in_flight {
        return Err(ActionError::new(
            409,
            "round_in_flight",
            format!(
                "{}: round {round} is still {}. A round returns to the caller; settling it is \
                 what the next call does",
                ctx.action,
                status.as_str()
            ),
        ));
    }
    if baseline_run(tx, ctx.tenant, climb.eval_id, ctx.action)
        .await?
        .is_none()
    {
        return Err(ActionError::new(
            409,
            "baseline_missing",
            format!(
                "{}: round 1 is measured against the `baseline` variant, and this eval has no \
                 finished baseline run. Start one with eval_runs.start (variant omitted) and let \
                 it complete",
                ctx.action
            ),
        ));
    }

    let stopping = Stopping::parse(&climb.stopping, climb.plateau_k, climb.max_rounds)?;
    if let Some(reason) = stopping_reached(stopping, climb.current_round, state.flat_streak) {
        if reason == StopReason::Plateau {
            let mut goal_doc = climb.goal.clone();
            if let Some(object) = goal_doc.as_object_mut() {
                object.insert("stop".to_owned(), json!({"reason": "plateau"}));
            }
            climb_store::advance_climb_tx(
                &mut *tx,
                ctx.tenant,
                climb.id,
                &climb_store::ClimbAdvance {
                    current_round: climb.current_round,
                    best_round: climb.best_round,
                    best_test_score: climb.best_test_score,
                    status: "plateaued",
                    goal: &goal_doc,
                },
            )
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
        }
        return Err(ActionError::new(
            409,
            reason.as_str(),
            format!(
                "{}: the climb's stopping condition is met ({})",
                ctx.action,
                reason.as_str()
            ),
        ));
    }

    // ── stall triage ───────────────────────────────────────────────────────────────────
    let buckets = match input.buckets.as_ref() {
        Some(value) => Some(BucketReport::parse(value)?),
        None => None,
    };
    if buckets.is_none() {
        if content_round_refused(state.flat_streak) {
            return Err(ActionError::new(
                409,
                "content_round_refused",
                format!(
                    "{}: {} consecutive rounds are flat. A further content round against the same \
                     wall is refused — this round must carry `buckets`",
                    ctx.action, state.flat_streak
                ),
            ));
        }
        if triage_required(state.flat_streak) {
            return Err(ActionError::new(
                422,
                "buckets_required",
                format!(
                    "{}: {} consecutive rounds are flat, so the failure is the grader, the \
                     harness, the artifact's structure or variance — not more content. Carry \
                     `buckets` naming which",
                    ctx.action, state.flat_streak
                ),
            ));
        }
    }

    // ── the hypothesis ─────────────────────────────────────────────────────────────────
    let prediction = match (input.prediction.as_ref(), buckets.as_ref()) {
        (Some(value), None) => Some(conveyor_domain::eval::climb::Prediction::parse(value)?),
        (None, Some(_)) => None,
        (Some(_), Some(_)) => {
            return Err(invalid(
                "prediction",
                "invalid_value",
                "a round is a hypothesis or a categorisation, never both",
            ));
        }
        (None, None) => {
            return Err(invalid(
                "prediction",
                "invalid_value",
                "a content round carries a pre-registered `prediction`",
            ));
        }
    };
    if let Some(prediction) = &prediction {
        if prediction.metric != goal.target {
            return Err(ActionError::new(
                422,
                "one_hypothesis",
                format!(
                    "{}: this round predicts `{}` but the climb's target is `{}`. One round is \
                     one hypothesis — stacking makes a win unattributable and a regression \
                     impossible to bisect",
                    ctx.action, prediction.metric, goal.target
                ),
            ));
        }
        if prediction.direction != goal.direction {
            return Err(invalid(
                "prediction.direction",
                "invalid_value",
                format!(
                    "the climb improves `{}`, so a prediction in the other direction is not a \
                     hypothesis about this climb",
                    goal.direction.as_str()
                ),
            ));
        }
        if prediction.best_case() < as_f64(climb.delta) {
            return Err(ActionError::new(
                409,
                "change_below_floor",
                format!(
                    "{}: the prediction's best case {} is below the climb's band {}. A change \
                     whose best case sits inside the band is kept or reverted by chance",
                    ctx.action,
                    prediction.best_case(),
                    as_f64(climb.delta)
                ),
            ));
        }
    }

    // ── the cost walk's position ───────────────────────────────────────────────────────
    if !goal.lever_allows(input.lever.as_deref()) {
        return Err(ActionError::new(
            422,
            "lever_order",
            format!(
                "{}: this round must be fought at the `{}` lever; skipping a lever makes the \
                 saving unattributable",
                ctx.action,
                goal.current_lever().unwrap_or("-")
            ),
        ));
    }

    // ── the guardrails ─────────────────────────────────────────────────────────────────
    if let Some(patch) = input.change_patch.as_deref() {
        for prefix in &climb.off_limits {
            if patch.contains(prefix.as_str()) {
                return Err(ActionError::new(
                    422,
                    "off_limits",
                    format!(
                        "{}: the patch touches `{prefix}`, which this climb's plan lists as \
                         off-limits",
                        ctx.action
                    ),
                ));
            }
        }
    }
    if let Some(ceiling) = climb.cost_ceiling_cents
        && state.spend >= ceiling as f64
    {
        {
            return Err(ActionError::new(
                409,
                "cost_ceiling_reached",
                format!(
                    "{}: the climb has spent {:.2} against a ceiling of {}",
                    ctx.action, state.spend, ceiling
                ),
            ));
        }
    }

    // ── the analyzer never reads the held-out half ─────────────────────────────────────
    let trace_ids = input.analysis_trace_ids.clone().unwrap_or_default();
    if !trace_ids.is_empty() {
        let splits = climb_store::trace_splits_tx(&mut *tx, ctx.tenant, &trace_ids)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
        let mut held_out = Vec::new();
        for (id, split) in &splits {
            if split == "test" {
                held_out.push(id.to_string());
            }
        }
        if !held_out.is_empty() {
            return Err(ActionError::new(
                422,
                "test_transcript_in_round",
                format!(
                    "{}: the analysis was handed {} held-out transcript(s) ({}…). A round whose \
                     proposal cites the held-out half is void — the wall is data access, not an \
                     instruction, and `evals:approve` is what reads the test split",
                    ctx.action,
                    held_out.len(),
                    held_out.first().map(String::as_str).unwrap_or("-")
                ),
            ));
        }
    }

    // ── create the round's run ─────────────────────────────────────────────────────────
    let round_no = climb.current_round + 1;
    let variant_no = climb_store::next_variant_tx(&mut *tx, ctx.tenant, climb.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let case_ids = version_case_ids(tx, ctx.tenant, climb.version_id, ctx.action).await?;
    if case_ids.is_empty() {
        return Err(not_ready(ctx.action, "the version has no cases to run"));
    }
    let environment = environment_of(tx, ctx.tenant, climb.eval_id, ctx.action).await?;

    // A categorisation round's prose IS its bucket table, in place of a diff.
    let change_md = match (&buckets, input.change_md.clone()) {
        (Some(report), extra) => {
            let mut text = report.to_markdown();
            if let Some(extra) = extra {
                text.push('\n');
                text.push_str(&extra);
            }
            text
        }
        (None, Some(change_md)) if !change_md.trim().is_empty() => change_md,
        (None, _) => {
            return Err(invalid(
                "change_md",
                "invalid_value",
                "a content round says what changed",
            ));
        }
    };
    let enqueue = !climb.approve_each_round;
    let started = start_or_resume_run_tx(
        tx,
        &ctx,
        &RunRequest {
            eval_id: climb.eval_id,
            version_id: climb.version_id,
            variant: Variant::Numbered(u32::try_from(variant_no).unwrap_or(u32::MAX)),
            mode: conveyor_domain::eval::RunMode::Full,
            reps: climb.reps,
            case_ids,
            round: round_no,
            cost_ceiling_cents: climb.cost_ceiling_cents,
            environment: &environment,
            enqueue,
        },
    )
    .await?;

    let mut summary = json!({
        "change_md": change_md,
        "target": input.target,
        "lever": input.lever,
        "analysis_trace_ids": trace_ids,
        "categorisation": buckets.is_some(),
        "prediction": input.prediction,
    });
    if let Some(prediction) = &prediction
        && let Some(object) = summary.as_object_mut()
    {
        object.insert(
            "prediction".to_owned(),
            serde_json::to_value(prediction).unwrap_or(Value::Null),
        );
    }
    climb_store::set_run_round_tx(
        &mut *tx,
        ctx.tenant,
        started.run.id,
        round_no,
        &change_md,
        input.change_patch.as_deref(),
        &summary,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    // Advance the loop's own counters: the round exists now, and the lever it was fought at.
    let mut goal_doc = climb.goal.clone();
    if let Some(object) = goal_doc.as_object_mut() {
        object.insert("lever".to_owned(), json!(goal.lever));
    }
    climb_store::advance_climb_tx(
        &mut *tx,
        ctx.tenant,
        climb.id,
        &climb_store::ClimbAdvance {
            current_round: round_no,
            best_round: climb.best_round,
            best_test_score: climb.best_test_score,
            status: "running",
            goal: &goal_doc,
        },
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    let awaiting_approval = if climb.approve_each_round {
        // `run_id` cannot carry an eval run: `approval.run_id` is a foreign key to the
        // control-plane `run` table. The round is named in `meta` and the gate is eval-scoped,
        // where `uq_appr_eval_pending` gives one pending round per eval.
        let mut approval =
            store::approval::NewApproval::new(climb_store::ROUND_GATE, climb_store::GATE_KIND);
        approval.eval_id = Some(climb.eval_id);
        approval.requested_by = Some(ctx.actor_id);
        approval.meta = json!({
            "climb_id": climb.id,
            "run_id": started.run.id,
            "round": round_no,
            "change_md": change_md,
            "change_patch": input.change_patch,
            "prediction": input.prediction,
            "buckets": input.buckets,
        });
        let approval = store::approval::insert_approval_tx(&mut *tx, ctx.tenant, &approval)
            .await
            .map_err(|_| ActionError::internal())?;
        Some(approval.id)
    } else {
        None
    };

    emit(
        tx,
        &ctx,
        climb.eval_id,
        climb.id,
        "running",
        EVAL_CLIMB_CHANGED,
        json!({"status": "running", "round": round_no, "variant": started.run.variant.as_str()}),
    )
    .await?;

    // ⛔ The round is CREATED and the call SUCCEEDS. Returning the refusal as an error would roll
    // the transaction back — the run row and the gate would vanish with it, and "record the round,
    // then wait" would be impossible to express in a transactional handler. Execution is what is
    // gated, and `queued_jobs: 0` with `awaiting_approval` set is the unambiguous signal.
    Ok(HandlerOutput::created(RoundOut {
        climb_id: climb.id.to_string(),
        round: round_no,
        variant: started.run.variant.as_str(),
        run_id: started.run.id.to_string(),
        queued_jobs: started.queued_jobs,
        skipped_slots: started.skipped_slots,
        awaiting_approval: awaiting_approval.map(|id| id.to_string()),
    }))
}

fn awaiting(action: &str, round: i32) -> ActionError {
    ActionError::new(
        409,
        "awaiting_approval",
        format!("{action}: round {round}'s sign-off has not been decided"),
    )
}

/// Every case id of the version, in ordinal order.
async fn version_case_ids(
    tx: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
    action: &str,
) -> Result<Vec<Uuid>, ActionError> {
    let mut out = Vec::new();
    let mut cursor: Option<(i32, Uuid)> = None;
    loop {
        let page = store::eval::list_cases_tx(&mut *tx, tenant, version_id, cursor, 500)
            .await
            .map_err(|error| eval_error(error, action))?;
        let short = page.len() < 500;
        if let Some(last) = page.last() {
            cursor = Some((last.ordinal, last.id));
        }
        out.extend(page.into_iter().map(|case| case.id));
        if short {
            break;
        }
    }
    Ok(out)
}

/// The eval's environment document, recorded on every run so a round is reproducible.
async fn environment_of(
    tx: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    action: &str,
) -> Result<Value, ActionError> {
    let eval = store::eval::get_eval_tx(&mut *tx, tenant, eval_id)
        .await
        .map_err(|error| eval_error(error, action))?;
    Ok(json!({
        "model": eval.model_ref,
        "prompt": eval.prompt_ref,
        "entry_action": eval.entry_action,
        "trace_source": eval.trace_source,
    }))
}

// ── the comparison ─────────────────────────────────────────────────────────────────────

/// The paired comparison of two runs, with S17's exact McNemar.
#[crate::action(
    name = "eval_climbs.compare",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_climbs_compare(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClimbCompareIn,
) -> Result<HandlerOutput<CompareOut>, ActionError> {
    let threshold = input.pass_threshold.unwrap_or(0.5);
    if !(0.0..=1.0).contains(&threshold) {
        return Err(invalid(
            "pass_threshold",
            "invalid_value",
            "a pass threshold is a score in [0, 1]",
        ));
    }

    let a = climb_store::run_signature_tx(&mut *tx, ctx.tenant, input.a_run_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("a run"))?;
    let b = climb_store::run_signature_tx(&mut *tx, ctx.tenant, input.b_run_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("b run"))?;
    if a.eval_id != b.eval_id {
        return Err(ActionError::new(
            409,
            "not_comparable",
            format!(
                "{}: the two runs measure different evals, so there is nothing to compare",
                ctx.action
            ),
        ));
    }
    let refusal = conveyor_domain::eval::climb::comparable(
        &conveyor_domain::eval::climb::RunSignature {
            version_id: a.version_id,
            digest: a.digest.clone(),
            split_sha256: a.split_sha256.clone(),
        },
        &conveyor_domain::eval::climb::RunSignature {
            version_id: b.version_id,
            digest: b.digest.clone(),
            split_sha256: b.split_sha256.clone(),
        },
    );
    if let Err(why) = refusal {
        return Err(ActionError::new(
            409,
            "not_comparable",
            format!(
                "{}: runs {} and {} are not comparable — their {} differ. A delta across them is \
                 a number with no meaning, and a re-grade must be compared against another \
                 re-grade, never against the run it replaced",
                ctx.action,
                a.variant,
                b.variant,
                why.as_str()
            ),
        ));
    }

    // Which metric: the caller's, or the version's only quality metric.
    let metrics = store::eval::list_metrics_tx(&mut *tx, ctx.tenant, a.version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let metric = match input.metric.clone() {
        Some(metric) => metric,
        None if metrics.len() == 1 => metrics[0].key.clone(),
        None if metrics.is_empty() => {
            return Err(not_ready(
                ctx.action,
                "the version declares no metric to compare",
            ));
        }
        None => {
            let keys: Vec<&str> = metrics.iter().map(|metric| metric.key.as_str()).collect();
            return Err(ActionError::new(
                409,
                "metric_ambiguous",
                format!(
                    "{}: the version declares {} metrics ({}); name the one to compare",
                    ctx.action,
                    keys.len(),
                    keys.join(", ")
                ),
            ));
        }
    };

    let a_scores = case_scores(tx, ctx.tenant, input.a_run_id, &metric, ctx.action).await?;
    let b_scores = case_scores(tx, ctx.tenant, input.b_run_id, &metric, ctx.action).await?;

    let mut a_map = std::collections::BTreeMap::new();
    let mut b_map = std::collections::BTreeMap::new();
    for (case, score) in &a_scores {
        a_map.insert(
            case.to_string(),
            conveyor_domain::eval::climb::resolved(*score, threshold),
        );
    }
    for (case, score) in &b_scores {
        b_map.insert(
            case.to_string(),
            conveyor_domain::eval::climb::resolved(*score, threshold),
        );
    }

    // The estate's one exact test (S17). The two arms differ only in `pipeline`, which its own
    // comparability rule deliberately ignores — the eval-level refusal above already ran.
    let arm_a = conveyor_domain::benchmark::Arm {
        pipeline: conveyor_domain::benchmark::Pipeline::Direct,
        oracle: conveyor_domain::benchmark::Oracle::Files,
        prompt: conveyor_domain::benchmark::Prompt::BugFix,
    };
    let arm_b = conveyor_domain::benchmark::Arm {
        pipeline: conveyor_domain::benchmark::Pipeline::Conveyor,
        oracle: conveyor_domain::benchmark::Oracle::Files,
        prompt: conveyor_domain::benchmark::Prompt::BugFix,
    };
    let tested =
        conveyor_domain::benchmark::compare(arm_a, arm_b, &a_map, &b_map).map_err(|_| {
            ActionError::new(
                409,
                "not_comparable",
                format!("{}: the two runs' arms are not comparable", ctx.action),
            )
        })?;

    let mut differences = Vec::new();
    for (case, a_score) in &a_scores {
        if let Some(b_score) = b_scores.get(case) {
            differences.push(b_score - a_score);
        }
    }
    let (delta, ci_low, ci_high) =
        conveyor_domain::eval::climb::paired_delta_ci(&differences, 1.96);

    Ok(HandlerOutput::ok(CompareOut {
        comparable: true,
        metric,
        both: tested.counts.both,
        neither: tested.counts.neither,
        a_only: tested.counts.a_only,
        b_only: tested.counts.b_only,
        discordant: tested.counts.a_only + tested.counts.b_only,
        missing_in_one: tested.counts.missing_in_one,
        p_value: tested.p_value,
        delta,
        ci_low,
        ci_high,
        significant: tested.p_value < 0.05 && delta > 0.0,
    }))
}

/// One run's per-case mean score, over its repetitions and over `ok` rows only.
async fn case_scores(
    tx: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    metric: &str,
    action: &str,
) -> Result<std::collections::BTreeMap<Uuid, f64>, ActionError> {
    let rows = all_results(tx, tenant, run_id, action).await?;
    let mut sums: std::collections::BTreeMap<Uuid, (f64, usize)> =
        std::collections::BTreeMap::new();
    for row in &rows {
        if row.status != "ok" {
            continue;
        }
        let Some(score) = row.grade.get(metric).and_then(Value::as_f64) else {
            continue;
        };
        let entry = sums.entry(row.case_id).or_insert((0.0, 0));
        entry.0 += score;
        entry.1 += 1;
    }
    Ok(sums
        .into_iter()
        .map(|(case, (sum, count))| (case, sum / count as f64))
        .collect())
}

// ── the report ──────────────────────────────────────────────────────────────────────────

/// The winner: the kept round with the best number **for this goal**.
///
/// A quality climb's winner is its highest held-out score. A `cost_usd` climb's is its **cheapest**
/// kept round: its held-out means are all zero (there is no `cost_usd` grade key), so choosing by
/// test score silently named the first kept round instead of the cheapest one.
fn best_of(state: &SettleState, goal: &Goal) -> (Option<i32>, Option<f64>) {
    let mut best: Option<&Settled> = None;
    for round in &state.settled {
        if round.decision != Decision::Keep {
            continue;
        }
        let better = match best {
            None => true,
            Some(current) if goal.is_cost() => {
                round.outcome.cost_cents.unwrap_or(f64::MAX)
                    < current.outcome.cost_cents.unwrap_or(f64::MAX)
            }
            Some(current) => round.outcome.test > current.outcome.test,
        };
        if better {
            best = Some(round);
        }
    }
    match best {
        Some(round) => (Some(round.round), Some(round.outcome.test)),
        None => (None, None),
    }
}

/// Rewrite every round's `summary` with its decision, its movements and its numbers.
async fn persist_decisions(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    eval_id: Uuid,
    rounds: &[RoundRow],
    state: &SettleState,
) -> Result<(), ActionError> {
    for round in &state.settled {
        let row = rounds.iter().find(|row| row.round == round.round);
        if row
            .and_then(|row| row.summary.get("decision"))
            .and_then(Value::as_str)
            == Some(round.decision.as_str())
        {
            continue;
        }
        let patch = json!({
            "train": round.outcome.train,
            "test": round.outcome.test,
            "excluded": round.excluded,
            "delta": round.outcome.test - round.base_test,
            "train_movement": movement_name(round.pattern.train),
            "test_movement": movement_name(round.pattern.test),
            "decision": round.decision.as_str(),
            "reason": match round.decision {
                Decision::Keep => Value::Null,
                Decision::Revert(reason) => json!(reason.as_str()),
            },
        });
        climb_store::merge_run_summary_tx(&mut *tx, ctx.tenant, round.run_id, &patch)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
        emit(
            tx,
            ctx,
            eval_id,
            round.run_id,
            round.decision.as_str(),
            EVAL_CLIMB_ROUND_COMPLETED,
            json!({"round": round.round, "decision": round.decision.as_str()}),
        )
        .await?;
    }
    Ok(())
}

fn movement_name(movement: Movement) -> &'static str {
    match movement {
        Movement::Up => "up",
        Movement::Flat => "flat",
        Movement::Down => "down",
    }
}

/// The status table plus the four-part summary.
fn build_report(
    climb: &Climb,
    rounds: &[RoundRow],
    state: &SettleState,
    best_round: Option<i32>,
    best_score: Option<f64>,
    reason: &str,
) -> String {
    let mut out = String::new();
    out.push_str("# Eval climb report\n\n");
    out.push_str(&format!(
        "Target `{}` ({}) · band {} · {} rounds · stopped: {}\n\n",
        climb
            .goal
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("-"),
        climb.direction,
        as_f64(climb.delta),
        state.settled.len(),
        reason
    ));
    out.push_str("| round | variant | train | test | Δ test | decision | reason |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    let mut previous: Option<f64> = None;
    for round in rounds {
        let settled = state.settled.iter().find(|row| row.round == round.round);
        let train = settled.map(|row| row.outcome.train);
        let test = settled.map(|row| row.outcome.test);
        let delta = match (test, previous) {
            (Some(test), Some(previous)) => format!("{:+.4}", test - previous),
            _ => "—".to_owned(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            round.round,
            round.variant,
            train
                .map(|value| format!("{value:.4}"))
                .unwrap_or_else(|| "—".to_owned()),
            test.map(|value| format!("{value:.4}"))
                .unwrap_or_else(|| "—".to_owned()),
            delta,
            settled.map(|row| row.decision.as_str()).unwrap_or("—"),
            settled
                .and_then(|row| match row.decision {
                    Decision::Keep => None,
                    Decision::Revert(reason) => Some(reason.as_str()),
                })
                .unwrap_or("—"),
        ));
        if let Some(test) = test {
            previous = Some(test);
        }
    }

    out.push_str("\n## What moved\n\n");
    let kept: Vec<&Settled> = state
        .settled
        .iter()
        .filter(|round| round.decision == Decision::Keep)
        .collect();
    if kept.is_empty() {
        out.push_str("No round was kept: every change was reverted on the held-out number.\n");
    } else {
        for round in &kept {
            out.push_str(&format!(
                "- round {}: held-out {:.4} (train {:.4})\n",
                round.round, round.outcome.test, round.outcome.train
            ));
        }
    }

    out.push_str("\n## What did not\n\n");
    let reverted: Vec<&Settled> = state
        .settled
        .iter()
        .filter(|round| round.decision != Decision::Keep)
        .collect();
    if reverted.is_empty() {
        out.push_str("Every round was kept.\n");
    } else {
        for round in &reverted {
            let reason = match round.decision {
                Decision::Revert(reason) => reason.as_str(),
                Decision::Keep => "-",
            };
            out.push_str(&format!("- round {}: reverted ({reason})\n", round.round));
        }
    }

    out.push_str("\n## Why\n\n");
    if state.flat_streak >= 2 {
        out.push_str(&format!(
            "{} consecutive rounds were flat: the failure is not more content. The buckets the \
             triage round named are the material to work from.\n",
            state.flat_streak
        ));
    } else {
        out.push_str(
            "Each decision above was taken on the held-out split, with the train split as \
             corroboration. The split is the version's and was never redrawn.\n",
        );
    }

    out.push_str("\n## What to do next\n\n");
    match (best_round, best_score) {
        (Some(round), Some(score)) => out.push_str(&format!(
            "**Winner: round {round}**, held-out {score:.4}. Applying it to the live process is \
             a person's `processes.update`; this climb names the version and ships the patch, it \
             never rewrites a published process.\n"
        )),
        _ => out
            .push_str("No round beat the baseline, so the baseline stands. Nothing is applied.\n"),
    }
    out
}
