#![forbid(unsafe_code)]
//! S20's pure rules: the keep/revert table, the stopping predicate, the stall triage, the noise
//! floor, the cost-walk lever order, and the comparability refusal.
//!
//! These are the decisions the slice says must be the **machine's, not a judgement call**, so each
//! one is asserted here rather than left to a handler's prose. Nothing in this file touches a
//! database: `crates/actions/tests/eval_climb_gate.rs` is where the same rules meet rows.

use conveyor_domain::eval::climb::{
    Bucket, BucketReport, Decision, Direction, Goal, Hold, HoldReading, Movement, NotComparable,
    Prediction, PriorPattern, RevertReason, RoundOutcome, RunSignature, StopReason, Stopping,
    comparable, content_round_refused, delta_above_floor, keep_or_revert, noise_floor,
    paired_delta_ci, resolved, stopping_reached, triage_required,
};
use uuid::Uuid;

fn goal() -> Goal {
    Goal {
        target: "pass".to_owned(),
        direction: Direction::Higher,
        hold: Vec::new(),
        lever: 0,
        stop: None,
    }
}

fn cost_goal() -> Goal {
    Goal {
        target: "cost_usd".to_owned(),
        direction: Direction::Lower,
        hold: vec![Hold {
            metric: "pass".to_owned(),
            min: 0.80,
        }],
        lever: 0,
        stop: None,
    }
}

fn outcome(round: i32, train: f64, test: f64) -> RoundOutcome {
    RoundOutcome {
        round,
        train,
        test,
        cost_cents: None,
        n_train: 20,
        n_test: 10,
        holds: Vec::new(),
    }
}

fn cost_outcome(round: i32, test: f64, cost_cents: f64, hold_value: f64) -> RoundOutcome {
    RoundOutcome {
        round,
        train: test,
        test,
        cost_cents: Some(cost_cents),
        n_train: 20,
        n_test: 10,
        holds: vec![HoldReading {
            metric: "pass".to_owned(),
            value: hold_value,
            min: 0.80,
        }],
    }
}

// ── the keep/revert table ─────────────────────────────────────────────────────────────────────

#[test]
fn both_signals_up_is_the_only_unambiguous_keep() {
    let base = outcome(0, 0.50, 0.50);
    let current = outcome(1, 0.60, 0.60);
    assert_eq!(
        keep_or_revert(&goal(), &base, &current, None, 0.05),
        Decision::Keep
    );
}

#[test]
fn train_up_and_held_out_flat_is_a_revert() {
    // The shape of fitting the visible set: the improvement did not transfer.
    let base = outcome(0, 0.50, 0.50);
    let current = outcome(1, 0.70, 0.51);
    assert_eq!(
        keep_or_revert(&goal(), &base, &current, None, 0.05),
        Decision::Revert(RevertReason::HeldOutFlat),
        "a change that lifts train and leaves the held-out number inside the band is reverted"
    );
}

#[test]
fn a_train_down_held_out_up_pattern_needs_a_second_witness() {
    let base = outcome(0, 0.60, 0.50);
    let current = outcome(1, 0.45, 0.65);
    assert_eq!(
        keep_or_revert(&goal(), &base, &current, None, 0.05),
        Decision::Revert(RevertReason::OneOff),
        "one round of this pattern is a coin flip, not evidence"
    );

    // The same pattern in the previous round, kept, makes it a repeat.
    assert_eq!(
        keep_or_revert(
            &goal(),
            &base,
            &current,
            Some(PriorPattern {
                train: Movement::Down,
                test: Movement::Up,
                kept: true,
            }),
            0.05
        ),
        Decision::Keep,
        "a pattern that repeats on a second run is kept"
    );

    // The same pattern, but the earlier round was reverted: not a witness.
    assert_eq!(
        keep_or_revert(
            &goal(),
            &base,
            &current,
            Some(PriorPattern {
                train: Movement::Down,
                test: Movement::Up,
                kept: false,
            }),
            0.05
        ),
        Decision::Revert(RevertReason::OneOff),
        "a reverted round cannot corroborate the next one"
    );
}

#[test]
fn a_tie_is_no_change() {
    let base = outcome(0, 0.50, 0.50);
    let current = outcome(1, 0.50, 0.50);
    let decision = keep_or_revert(&goal(), &base, &current, None, 0.05);
    assert_eq!(decision, Decision::Revert(RevertReason::NoChange));
    assert!(decision.is_flat(), "a tie counts towards the plateau");
}

#[test]
fn a_dropping_held_out_number_is_never_kept() {
    let base = outcome(0, 0.50, 0.50);
    let current = outcome(1, 0.70, 0.40);
    assert_eq!(
        keep_or_revert(&goal(), &base, &current, None, 0.05),
        Decision::Revert(RevertReason::TrainDown)
    );
}

// ── the cost walk ────────────────────────────────────────────────────────────────────────────

#[test]
fn a_cost_round_that_broke_its_guardrail_is_reverted_however_cheap_it_got() {
    let base = cost_outcome(0, 0.90, 10.0, 0.90);
    let current = cost_outcome(1, 0.90, 1.0, 0.70);
    assert_eq!(
        keep_or_revert(&cost_goal(), &base, &current, None, 0.05),
        Decision::Revert(RevertReason::HoldBroken)
    );
}

#[test]
fn a_cost_walk_keeps_only_when_cost_clears_the_band_and_quality_holds() {
    let base = cost_outcome(0, 0.90, 10.0, 0.90);
    let current = cost_outcome(1, 0.91, 4.0, 0.90);
    assert_eq!(
        keep_or_revert(&cost_goal(), &base, &current, None, 0.05),
        Decision::Keep
    );

    // Cost barely moved: the walk has not paid for itself, however green the guardrail is.
    let flat = cost_outcome(2, 0.91, 9.99, 0.90);
    assert_eq!(
        keep_or_revert(&cost_goal(), &base, &flat, None, 0.05),
        Decision::Revert(RevertReason::CostNotCleared)
    );
}

#[test]
fn a_cost_walk_cannot_skip_a_lever() {
    let mut goal = cost_goal();
    assert!(goal.lever_allows(Some("caching_health")));
    assert!(
        !goal.lever_allows(Some("prompt_climb")),
        "the staircase is walked in order; skipping is refused"
    );
    assert!(!goal.lever_allows(None), "a cost round must name its lever");

    goal.advance_lever();
    assert!(goal.lever_allows(Some("prompt_audit")));
    assert!(!goal.lever_allows(Some("caching_health")), "no going back");
}

#[test]
fn a_cost_goal_without_a_guardrail_is_refused_at_parse() {
    let refused = Goal::parse(&serde_json::json!({
        "target": "cost_usd",
        "direction": "lower"
    }));
    let error = refused.expect_err("a cost search with no hold is a race to the cheapest answer");
    assert!(
        error.to_string().contains("hold"),
        "the refusal names the missing guardrail: {error}"
    );

    let refused = Goal::parse(&serde_json::json!({
        "target": "cost_usd",
        "direction": "higher",
        "hold": [{"metric": "pass", "min": 0.8}]
    }));
    assert!(
        refused.is_err(),
        "a cost goal always improves downward, whatever the caller wrote"
    );
}

// ── the stopping predicate ──────────────────────────────────────────────────────────────────

#[test]
fn a_plateau_needs_three_flat_rounds() {
    let stopping = Stopping::parse("plateau", 3, None).expect("k = 3 is legal");
    assert_eq!(stopping_reached(stopping, 2, 2), None, "two is still noise");
    assert_eq!(stopping_reached(stopping, 3, 3), Some(StopReason::Plateau));
}

#[test]
fn a_plateau_k_below_three_is_refused_by_vocabulary() {
    assert!(
        Stopping::parse("plateau", 2, None).is_err(),
        "two flat rounds is too few to call a plateau"
    );
}

#[test]
fn a_round_count_stops_at_the_count_and_manual_never_stops_itself() {
    let stopping = Stopping::parse("rounds", 3, Some(2)).expect("max_rounds = 2");
    assert_eq!(stopping_reached(stopping, 1, 0), None);
    assert_eq!(
        stopping_reached(stopping, 2, 0),
        Some(StopReason::RoundsExhausted)
    );
    let manual = Stopping::parse("manual", 3, None).expect("manual");
    assert_eq!(stopping_reached(manual, 99, 99), None);
}

#[test]
fn rounds_without_a_max_is_refused() {
    assert!(Stopping::parse("rounds", 3, None).is_err());
}

// ── stall triage ────────────────────────────────────────────────────────────────────────────

#[test]
fn a_fourth_content_round_is_refused() {
    assert!(!triage_required(1), "one flat round is still a normal loop");
    assert!(
        triage_required(2),
        "after two flat rounds the next round must categorise"
    );
    assert!(
        !content_round_refused(2),
        "a categorisation round is still allowed at two"
    );
    assert!(
        content_round_refused(3),
        "a fourth content round against the same wall is refused"
    );
}

#[test]
fn the_bucket_vocabulary_is_closed_and_a_report_must_count_something() {
    for name in [
        "artifact_gap",
        "grader_disagreement",
        "harness",
        "structural",
        "variance",
    ] {
        assert!(Bucket::parse(name).is_ok(), "{name} is in the vocabulary");
    }
    assert!(Bucket::parse("model_is_bad").is_err(), "no sixth bucket");

    let empty = BucketReport::parse(&serde_json::json!([]));
    assert!(
        empty.is_err(),
        "an empty categorisation categorises nothing"
    );

    let zeroed = BucketReport::parse(&serde_json::json!([
        {"bucket": "variance", "count": 0, "line": "nothing moved"}
    ]));
    assert!(
        zeroed.is_err(),
        "a report that counts nothing is not a report"
    );

    let report = BucketReport::parse(&serde_json::json!([
        {"bucket": "harness", "count": 4, "line": "two 504s and two timeouts"}
    ]))
    .expect("a real report");
    let markdown = report.to_markdown();
    assert!(markdown.contains("`harness`"));
    assert!(markdown.contains("two 504s and two timeouts"));
}

// ── the noise floor ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_change_inside_the_noise_floor_is_refused_with_the_number() {
    // 25 held-out cases, one rep: the half-width is 1/sqrt(25) = 0.2.
    assert!((noise_floor(25, 1) - 0.2).abs() < 1e-12);
    let refused = delta_above_floor(0.1, 25, 1);
    assert_eq!(
        refused,
        Err(0.2),
        "the refusal carries the floor so the caller can say it out loud"
    );
    assert!(delta_above_floor(0.25, 25, 1).is_ok());
    assert!(delta_above_floor(0.0, 25, 1).is_err());
}

#[test]
fn no_cases_means_no_measurable_delta() {
    assert!(noise_floor(0, 1).is_infinite());
    assert!(delta_above_floor(0.5, 0, 1).is_err());
}

#[test]
fn a_prediction_inside_the_floor_is_refused_and_a_bold_one_is_not() {
    let timid = Prediction::parse(&serde_json::json!({
        "metric": "pass", "direction": "higher", "interval": [0.0, 0.05],
        "falsifier": "the held-out pass rate does not move by 0.25"
    }))
    .expect("parses");
    assert!(
        timid.best_case() < 0.25,
        "the best case of a timid change is inside a 0.25 band"
    );

    let bold = Prediction::parse(&serde_json::json!({
        "metric": "cost_usd", "direction": "lower", "interval": [0.30, 0.60],
        "falsifier": "cost does not fall by 0.30"
    }))
    .expect("parses");
    assert!(
        (bold.best_case() - 0.60).abs() < 1e-12,
        "a prediction's interval is a gain magnitude in the improving direction, so the best \
         case is its upper end whichever way the metric improves"
    );

    let signed = Prediction::parse(&serde_json::json!({
        "metric": "cost_usd", "direction": "lower", "interval": [-0.60, -0.30],
        "falsifier": "cost does not fall"
    }));
    assert!(
        signed.is_err(),
        "a negative interval would let the sign double as the direction; the refusal says so"
    );
}

// ── comparability ──────────────────────────────────────────────────────────────────────────

fn signature(version: Uuid, digest: &str, split: &str) -> RunSignature {
    RunSignature {
        version_id: version,
        digest: digest.to_owned(),
        split_sha256: Some(split.to_owned()),
    }
}

#[test]
fn a_comparison_across_versions_graders_or_splits_is_refused() {
    let version = Uuid::now_v7();
    let other = Uuid::now_v7();
    let a = signature(version, "graders-a", "split-a");
    assert_eq!(comparable(&a, &a), Ok(()));

    assert_eq!(
        comparable(&a, &signature(other, "graders-a", "split-a")),
        Err(NotComparable::Version)
    );
    assert_eq!(
        comparable(&a, &signature(version, "graders-b", "split-a")),
        Err(NotComparable::Graders)
    );
    assert_eq!(
        comparable(&a, &signature(version, "graders-a", "split-b")),
        Err(NotComparable::Split)
    );
}

// ── the paired delta ──────────────────────────────────────────────────────────────────────

#[test]
fn the_paired_interval_reproduces_by_hand() {
    // diffs [1, 1, 1, 1, -1]: mean 0.6, s^2 = 0.8, se = 0.4, z = 1.96.
    let (delta, low, high) = paired_delta_ci(&[1.0, 1.0, 1.0, 1.0, -1.0], 1.96);
    assert!((delta - 0.6).abs() < 1e-12, "delta {delta}");
    assert!((low - (0.6 - 1.96 * 0.4)).abs() < 1e-12, "low {low}");
    assert!((high - (0.6 + 1.96 * 0.4)).abs() < 1e-12, "high {high}");
}

#[test]
fn an_empty_pairing_has_no_interval() {
    assert_eq!(paired_delta_ci(&[], 1.96), (0.0, 0.0, 0.0));
}

#[test]
fn a_case_is_resolved_at_the_threshold() {
    assert!(resolved(0.5, 0.5));
    assert!(!resolved(0.49, 0.5));
}
