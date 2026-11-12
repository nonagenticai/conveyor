#![forbid(unsafe_code)]
//! `eval_runs.report` — a run's numbers, recomputed from its rows (S19 Part C step 13;
//! CONTRACT §20, SPEC D25).
//!
//! **Nothing here is stored.** There is no rollup column and no maintained counter: the report
//! reads the run, its declared metrics, every result row and every failed attempt, and derives
//! the numbers in Rust. The same rows therefore produce the same report twice, and the report
//! cannot drift from the climb, which prices the same rows with the same arithmetic.
//!
//! ⚠️ **Cost is priced from the recorded usage, not from `inference_call`.** D23's amendment
//! (`caller_kind='eval'` plus `inference_call.eval_run_id`) was specified in S19 §9 and is not
//! built, so the ledger has no eval key; its only link is `action_invocation_id`, which covers
//! the subject call but not the judge (a separate invocation). Pricing the row's own
//! `usage`/`judge_usage` and each failed attempt's billed `usage` is the same arithmetic S20's
//! climb uses, so the two surfaces cannot disagree. When the amendment lands, the ledger becomes
//! the authority and this note must be removed with it.
//!
//! Three rules the numbers obey:
//!
//! - **Per-case means, then the mean of means.** A run with uneven reps (a failed attempt leaves
//!   a gap) must not weight a case by how many of its reps succeeded.
//! - **A truncated or parked attempt is counted, never averaged.** `truncated` and
//!   `pending_label` rows are excluded from every mean and reported on their own; a mean that
//!   included them would inflate every regression.
//! - **The noise floor is the held-out sample's.** `1/sqrt(n·reps)` over the headline quality
//!   metric's test-split cases, falling back to all graded cases when the run has no test rows.
//!   A confidence interval narrower than that floor is not a result.

use std::collections::{BTreeMap, BTreeSet};

use conveyor_store as store;
use conveyor_store::eval::{AttemptError, EvalMetric, ResultRow};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};

use super::climbs::{price_usage, tokens_of};
use super::eval_error;
use super::run_core::all_results;

/// The z-score a 95 % normal interval uses.
const Z_95: f64 = 1.96;

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `eval_runs.report` — one run's report.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ReportIn {
    /// The run to report on.
    pub run_id: Uuid,
    /// What the per-group breakdown groups by: `tag` (the default) or `split`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One declared metric's numbers, over the cases that carry a value.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricStatOut {
    /// The report's column key.
    pub key: String,
    /// The legend text.
    pub label: String,
    /// `quality | perf`.
    pub role: String,
    /// `binary | judge | float`.
    pub kind: String,
    /// `higher | lower`.
    pub better: String,
    /// The value's unit, when it has one.
    pub unit: Option<String>,
    /// Cases carrying at least one scorable value.
    pub n: i64,
    /// Cases the run covered but this metric has no value for.
    pub missing: i64,
    /// The mean of the case means; `null` when nothing was scored.
    pub mean: Option<f64>,
    /// The lower bound of the 95 % interval over the case means.
    pub ci_low: Option<f64>,
    /// The upper bound of the 95 % interval over the case means.
    pub ci_high: Option<f64>,
}

/// One group of the per-tag (or per-split) breakdown.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GroupStatOut {
    /// The group's value: a first tag, a split, or `untagged`.
    pub group: String,
    /// Cases in the group.
    pub n: i64,
    /// The same metric columns, computed inside the group.
    pub metrics: Vec<MetricStatOut>,
}

/// The failed attempts a run produced, counted rather than averaged.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ErrorCountsOut {
    /// Every failed attempt row.
    pub total: i64,
    /// Distinct `(case, rep)` slots with at least one failed attempt.
    pub failed_slots: i64,
    /// Failed attempts by `failure_class`.
    pub by_class: BTreeMap<String, i64>,
}

/// What the run spent, in the unit the version prices in.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CostOut {
    /// `cents` when the version prices its tokens, otherwise `tokens`.
    pub unit: String,
    /// The subject calls' cost.
    pub subject: f64,
    /// The judge calls' cost.
    pub judge: f64,
    /// The failed attempts' billed cost.
    pub failed: f64,
    /// `subject + judge + failed`.
    pub total: f64,
    /// Billed usage objects that could not be priced (no rates, or no token counts). Counted,
    /// never silently treated as free.
    pub unpriced: i64,
}

/// One run's report.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ReportOut {
    /// The run.
    pub run_id: Uuid,
    /// The eval it measures.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// `baseline | v<N>`.
    pub variant: String,
    /// `queued | running | partial | complete | void | cancelled`.
    pub status: String,
    /// Repetitions per case.
    pub reps: i32,
    /// How many cases the run covers.
    pub cases: i64,
    /// Slots that produced a scorable result.
    pub graded: i64,
    /// Slots the runner cut at `max_tokens`.
    pub truncated: i64,
    /// The failed attempts.
    pub errors: ErrorCountsOut,
    /// Every declared quality metric.
    pub metrics: Vec<MetricStatOut>,
    /// Every declared perf metric (measured, never graded).
    pub perf: Vec<MetricStatOut>,
    /// The per-tag (or per-split) breakdown.
    pub per_tag: Vec<GroupStatOut>,
    /// `1/sqrt(n·reps)` for the headline quality metric; `null` when nothing was scored.
    pub noise_floor: Option<f64>,
    /// The headline metric's distance to its ceiling; `null` when the metric has no bounded
    /// ceiling (a float or judge score) or nothing was scored.
    pub headroom: Option<f64>,
    /// What the run spent.
    pub cost: CostOut,
}

// ── the report ──────────────────────────────────────────────────────────────────────────

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    mean.is_finite().then_some(mean)
}

/// Per-case means for one metric over a set of rows.
///
/// A case contributes one number — the mean of its scorable reps — so a case with three of four
/// reps scored does not count three times as much as one with a single rep.
fn case_means(rows: &[&ResultRow], key: &str) -> Vec<f64> {
    let mut per_case: BTreeMap<Uuid, Vec<f64>> = BTreeMap::new();
    for row in rows {
        if row.status != "ok" {
            continue;
        }
        if let Some(value) = row.grade.get(key).and_then(Value::as_f64)
            && value.is_finite()
        {
            per_case.entry(row.case_id).or_default().push(value);
        }
    }
    per_case
        .values()
        .filter_map(|values| mean(values))
        .collect()
}

/// One declared metric's statistics over `rows`, with `covered` cases in the run.
fn stat(metric: &EvalMetric, rows: &[&ResultRow], covered: i64) -> MetricStatOut {
    let means = case_means(rows, &metric.key);
    let n = means.len() as i64;
    let (mean_value, ci_low, ci_high) = if means.is_empty() {
        (None, None, None)
    } else {
        let (mean, low, high) = conveyor_domain::eval::climb::paired_delta_ci(&means, Z_95);
        (Some(mean), Some(low), Some(high))
    };
    MetricStatOut {
        key: metric.key.clone(),
        label: metric.label.clone(),
        role: metric.role.clone(),
        kind: metric.kind.clone(),
        better: metric.better.clone(),
        unit: metric.unit.clone(),
        n,
        missing: (covered - n).max(0),
        mean: mean_value.filter(|value| value.is_finite()),
        ci_low: ci_low.filter(|value| value.is_finite()),
        ci_high: ci_high.filter(|value| value.is_finite()),
    }
}

/// The groups the per-tag breakdown uses, in a stable order.
fn grouped<'a>(rows: &[&'a ResultRow], group_by: &str) -> Vec<(String, Vec<&'a ResultRow>)> {
    let mut groups: BTreeMap<String, Vec<&ResultRow>> = BTreeMap::new();
    for row in rows {
        let key = match group_by {
            "split" => row.split.clone(),
            _ => row.tag.clone().unwrap_or_else(|| "untagged".to_owned()),
        };
        groups.entry(key).or_default().push(row);
    }
    groups.into_iter().collect()
}

/// The headline quality metric: the first declared one, in ordinal order.
fn headline(metrics: &[EvalMetric]) -> Option<&EvalMetric> {
    metrics.iter().find(|metric| metric.role == "quality")
}

/// The distance from a binary metric's mean to its ceiling (1.0).
///
/// `None` for a float or judge score: no column declares a ceiling, and inventing one would make
/// "headroom" a number nobody can defend.
fn headroom_of(metric: &EvalMetric, stats: &MetricStatOut) -> Option<f64> {
    if metric.kind != "binary" {
        return None;
    }
    let mean = stats.mean?;
    Some(if metric.better == "lower" {
        mean
    } else {
        1.0 - mean
    })
}

/// The report — in the caller's transaction, over the rows as they are now.
async fn report(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    input: ReportIn,
) -> Result<ReportOut, ActionError> {
    let group_by = match input.group_by.as_deref() {
        None | Some("tag") => "tag",
        Some("split") => "split",
        Some(other) => {
            return Err(invalid(
                "group_by",
                "invalid_value",
                format!("group_by must be `tag` or `split`, not {other:?}"),
            ));
        }
    };

    let run = store::eval::get_run_tx(&mut *tx, ctx.tenant, input.run_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let version = store::eval::get_version_tx(&mut *tx, ctx.tenant, run.version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let metrics = store::eval::list_metrics_tx(&mut *tx, ctx.tenant, run.version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let rows = all_results(&mut *tx, ctx.tenant, run.id, ctx.action).await?;
    let attempts: Vec<AttemptError> = store::eval::attempt_errors_tx(&mut *tx, ctx.tenant, run.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let covered = i64::from(run.case_count);
    let graded = rows.iter().filter(|row| row.status == "ok").count() as i64;
    let truncated = rows.iter().filter(|row| row.status == "truncated").count() as i64;

    // ── the failed attempts ─────────────────────────────────────────────────────────────
    let mut by_class: BTreeMap<String, i64> = BTreeMap::new();
    let mut failed_slots: BTreeSet<(Uuid, i32)> = BTreeSet::new();
    for attempt in &attempts {
        *by_class.entry(attempt.failure_class.clone()).or_default() += 1;
        failed_slots.insert((attempt.case_id, attempt.rep));
    }

    // ── the metric columns ──────────────────────────────────────────────────────────────
    let borrowed: Vec<&ResultRow> = rows.iter().collect();
    let quality: Vec<MetricStatOut> = metrics
        .iter()
        .filter(|metric| metric.role == "quality")
        .map(|metric| stat(metric, &borrowed, covered))
        .collect();
    let perf: Vec<MetricStatOut> = metrics
        .iter()
        .filter(|metric| metric.role == "perf")
        .map(|metric| stat(metric, &borrowed, covered))
        .collect();

    // ── the per-group breakdown ─────────────────────────────────────────────────────────
    let mut per_tag = Vec::new();
    for (group, group_rows) in grouped(&borrowed, group_by) {
        let group_cases: i64 = group_rows
            .iter()
            .map(|row| row.case_id)
            .collect::<BTreeSet<_>>()
            .len() as i64;
        per_tag.push(GroupStatOut {
            group,
            n: group_cases,
            metrics: metrics
                .iter()
                .map(|metric| stat(metric, &group_rows, group_cases))
                .collect(),
        });
    }

    // ── the floor and the headroom ──────────────────────────────────────────────────────
    //
    // Both are the **held-out** numbers: the headline metric over the test-split cases when the
    // run has one, otherwise every graded case (a pilot instrument that never drew a split). The
    // `metrics[]` columns above stay whole-run; the per-split row in `per_tag` carries the same
    // held-out figure for a caller that wants it by name.
    let (noise_floor, headroom) = match headline(&metrics) {
        Some(metric) => {
            let test_rows: Vec<&ResultRow> = borrowed
                .iter()
                .copied()
                .filter(|row| row.split == "test")
                .collect();
            let (scope, scope_cases): (Vec<&ResultRow>, i64) = if test_rows.is_empty() {
                (borrowed.clone(), covered)
            } else {
                let cases = test_rows
                    .iter()
                    .map(|row| row.case_id)
                    .collect::<BTreeSet<_>>();
                (test_rows, cases.len() as i64)
            };
            let n = case_means(&scope, &metric.key).len();
            let floor = if n == 0 {
                None
            } else {
                let floor = conveyor_domain::eval::climb::noise_floor(n, run.reps.max(0) as u32);
                floor.is_finite().then_some(floor)
            };
            let stats = stat(metric, &scope, scope_cases);
            (floor, headroom_of(metric, &stats))
        }
        None => (None, None),
    };

    // ── the cost ────────────────────────────────────────────────────────────────────────
    let priced = price_usage_available(&version.prices);
    let mut cost = CostOut {
        unit: if priced { "cents" } else { "tokens" }.to_owned(),
        subject: 0.0,
        judge: 0.0,
        failed: 0.0,
        total: 0.0,
        unpriced: 0,
    };
    let add = |usage: &Value, slot: &mut f64, unpriced: &mut i64| {
        if usage
            .as_object()
            .map(|object| object.is_empty())
            .unwrap_or(true)
        {
            return;
        }
        if priced {
            match price_usage(usage, &version.prices) {
                Some(cents) => *slot += cents,
                // A non-empty usage with no rates is not free: it is unpriced.
                None => *unpriced += 1,
            }
        } else {
            *slot += tokens_of(usage);
        }
    };
    for row in &rows {
        add(&row.usage, &mut cost.subject, &mut cost.unpriced);
        if let Some(usage) = row.judge_usage.as_ref() {
            add(usage, &mut cost.judge, &mut cost.unpriced);
        }
    }
    for attempt in &attempts {
        if let Some(usage) = attempt.usage.as_ref() {
            add(usage, &mut cost.failed, &mut cost.unpriced);
        }
    }
    cost.total = cost.subject + cost.judge + cost.failed;

    Ok(ReportOut {
        run_id: run.id,
        eval_id: run.eval_id,
        version_id: run.version_id,
        variant: run.variant.as_str().to_owned(),
        status: run.status.as_str().to_owned(),
        reps: run.reps,
        cases: covered,
        graded,
        truncated,
        errors: ErrorCountsOut {
            total: attempts.len() as i64,
            failed_slots: failed_slots.len() as i64,
            by_class,
        },
        metrics: quality,
        perf,
        per_tag,
        noise_floor,
        headroom,
        cost,
    })
}

/// Whether the version's prices carry the two rates [`price_usage`] understands.
fn price_usage_available(prices: &Value) -> bool {
    prices.get("input").and_then(Value::as_f64).is_some()
        && prices.get("output").and_then(Value::as_f64).is_some()
}

// ── the action ──────────────────────────────────────────────────────────────────────────

/// One run's report: per-metric means with intervals, the perf columns, the per-tag breakdown,
/// the failed attempts, the noise floor and headroom, and what the run spent.
#[crate::action(
    name = "eval_runs.report",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_report(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReportIn,
) -> Result<ReportOut, ActionError> {
    report(tx, &ctx, input).await
}
