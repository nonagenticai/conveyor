#![forbid(unsafe_code)]
//! `benchmark_runs.trend` — S18 build-order step 2: a suite's runs as a version-to-version trend.
//!
//! **The trend is a query, not a table** (S18 §9.3). Every point is derived from the stored
//! `benchmark_run` row and the graded `benchmark_case` verdicts; nothing is rolled up and left to
//! drift, and the baseline is read from the suite's own promoted run, never inferred.
//!
//! **Only a `conveyor` run is plotted, and only against a `conveyor` baseline** (S18 §9.4). A
//! control (`gold`/`empty`), a `baseline`-arm row, a baseline that is not itself `succeeded`, or a
//! run whose oracle/prompt differs is **displayed** with `comparable:false` and the reason —
//! never dropped, never averaged into a number. That refusal is the whole point of the
//! instrument: mixing arms is the visual form of the comparison CONTRACT §19 refuses.
//!
//! The comparison itself is [`crate::benchmark::compare_two_tx`], the same function
//! `benchmark_runs.compare` serves, so a point can never disagree with what a person reading
//! `benchmark_runs.compare` gets.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::actions::clamp_limit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::benchmark::runs::ARMS;
use crate::benchmark::{CompareView, benchmark_error, compare_two_tx, invalid};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// `benchmark_runs.trend` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrendIn {
    /// The suite whose runs form the trend.
    pub suite_id: Uuid,
    /// Only runs of this arm (`gold | empty | baseline | conveyor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arm: Option<String>,
    /// How many of the newest points to return (default from the shared envelope, max 100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// One point: one succeeded run of the suite, with its delta against the promoted baseline.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrendPoint {
    /// The run.
    pub run_id: Uuid,
    /// `gold | empty | baseline | conveyor` — the arm this run was started as.
    pub arm: String,
    /// `suite | capability_agent | procedure` — the executor the run measured.
    pub subject_kind: String,
    /// The pinned capability agent or procedure id; `None` for a `suite` subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<Uuid>,
    /// The subject's version **as it was when the run started** (S18 §3).
    ///
    /// Recorded on the run by `benchmark_runs.start`, never joined at read time: a
    /// `capability_agent`'s version is an optimistic-concurrency counter that any edit bumps, so a
    /// live join would claim a version a past run never measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_version: Option<i64>,
    /// The predictions label the arm identity is derived from.
    pub label: String,
    /// When the driver started the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Cases that produced a patch (the graded denominator).
    pub n_graded: i32,
    /// Cases the grader resolved.
    pub n_resolved: i32,
    /// `n_resolved / n_graded`, or `null` when nothing was graded (never `0.0` for "no data").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// `None` for non-control arms; `false` for a control that missed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_ok: Option<bool>,
    /// `candidate_better | baseline_better | unchanged`; `null` when there is nothing to compare.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// The paired comparison against the suite's promoted baseline.
    ///
    /// Absent = there is no baseline, or this point **is** the baseline. A refusal
    /// (`comparable:false` + `reason`) = the point is displayed but not plotted: a control, a
    /// non-conveyor baseline, or a differing oracle/prompt.
    ///
    /// The shape is `benchmark_runs.compare`'s own [`CompareView`] — one comparison vocabulary,
    /// not two. (`direction` is a sibling field rather than a member of it, so `CompareView` stays
    /// exactly what the compare action serves.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vs_baseline: Option<CompareView>,
}

/// `benchmark_runs.trend` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrendView {
    /// The suite's promoted baseline run, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_run_id: Option<Uuid>,
    /// The points, newest first (the store's own order).
    pub points: Vec<TrendPoint>,
}

/// The direction of a comparable delta, with the **point** as `a` and the baseline as `b`.
fn direction(view: &CompareView) -> Option<String> {
    if !view.comparable {
        return None;
    }
    let a_only = view.a_only?;
    let b_only = view.b_only?;
    Some(
        match a_only.cmp(&b_only) {
            std::cmp::Ordering::Greater => "candidate_better",
            std::cmp::Ordering::Less => "baseline_better",
            std::cmp::Ordering::Equal => "unchanged",
        }
        .to_owned(),
    )
}

/// The suite's runs as a trend, with each point's delta against the promoted baseline.
#[crate::action(
    name = "benchmark_runs.trend",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_trend(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrendIn,
) -> Result<HandlerOutput<TrendView>, ActionError> {
    const ACTION: &str = "benchmark_runs.trend";

    if let Some(arm) = input.arm.as_deref()
        && !ARMS.contains(&arm)
    {
        return Err(invalid(
            "arm",
            "invalid_value",
            "arm must be gold, empty, baseline or conveyor",
        ));
    }

    let suite = store::benchmark::get_suite_tx(&mut *tx, ctx.tenant, input.suite_id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    // One point costs three queries (the paired verdict read on both sides, plus the point's own
    // row), all inside this 30 s transaction — so the page is capped well below the envelope's 200.
    let limit = clamp_limit(input.limit).min(100);
    // Only finished runs are points: a queued or running run has no number yet, and a failed one
    // has a partial denominator that would read as a regression it never measured.
    let runs = store::benchmark::list_runs_tx(
        &mut *tx,
        ctx.tenant,
        Some(suite.id),
        input.arm.as_deref(),
        Some("succeeded"),
        None,
        limit,
    )
    .await
    .map_err(|error| benchmark_error(error, ACTION))?;

    // The baseline is whatever the suite promoted — never "the newest run" (S18 §9.2).
    let baseline = match suite.baseline_run_id {
        Some(id) => Some(
            store::benchmark::get_run_tx(&mut *tx, ctx.tenant, id)
                .await
                .map_err(|error| benchmark_error(error, ACTION))?,
        ),
        None => None,
    };

    let mut points = Vec::with_capacity(runs.len());
    for run in &runs {
        let mut point = TrendPoint {
            run_id: run.id,
            arm: run.arm.clone(),
            subject_kind: run.subject_kind.clone(),
            subject_id: run.capability_agent_id.or(run.process_id),
            subject_version: run.subject_version,
            label: run.label.clone(),
            started_at: run.started_at,
            n_graded: run.n_graded,
            n_resolved: run.n_resolved,
            rate: (run.n_graded > 0).then(|| f64::from(run.n_resolved) / f64::from(run.n_graded)),
            control_ok: run.control_ok,
            direction: None,
            vs_baseline: None,
        };

        if let Some(baseline) = baseline.as_ref()
            && baseline.id != run.id
        {
            // S18 §9.4, in order, before any number is computed. The oracle/prompt rule in
            // `compare_two_tx` cannot catch these on its own: a gold control and a conveyor run
            // can carry the same label suffix and are still different instruments.
            let view = if baseline.status != "succeeded" {
                CompareView::refused(format!(
                    "the suite's promoted baseline is {} — a partial run is not a baseline",
                    baseline.status
                ))
            } else if baseline.arm != "conveyor" {
                CompareView::refused(format!(
                    "the suite's promoted baseline is a {} run; only a conveyor run is the \
                     capability baseline",
                    baseline.arm
                ))
            } else if run.arm != "conveyor" {
                CompareView::refused(format!(
                    "this is a {} run — a control is displayed but never plotted against the \
                     capability baseline",
                    run.arm
                ))
            } else {
                compare_two_tx(&mut *tx, ctx.tenant, run, baseline, ACTION).await?
            };
            point.direction = direction(&view);
            point.vs_baseline = Some(view);
        }

        points.push(point);
    }
    // The store pages newest-first; a trend is read oldest-first, left to right.
    points.reverse();

    Ok(HandlerOutput::ok(TrendView {
        baseline_run_id: suite.baseline_run_id,
        points,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparable(a_only: usize, b_only: usize) -> CompareView {
        CompareView {
            comparable: true,
            reason: None,
            both: Some(0),
            neither: Some(0),
            a_only: Some(a_only),
            b_only: Some(b_only),
            discordant: Some(a_only + b_only),
            p_value: Some(1.0),
            missing_in_one: Some(0),
        }
    }

    #[test]
    fn a_refusal_has_no_direction() {
        assert_eq!(direction(&CompareView::refused("arms differ")), None);
    }

    #[test]
    fn the_direction_is_read_from_the_candidate_as_a() {
        assert_eq!(
            direction(&comparable(3, 1)).as_deref(),
            Some("candidate_better")
        );
        assert_eq!(
            direction(&comparable(1, 3)).as_deref(),
            Some("baseline_better")
        );
        assert_eq!(direction(&comparable(2, 2)).as_deref(), Some("unchanged"));
    }
}
