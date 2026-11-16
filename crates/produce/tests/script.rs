//! P3 — `produce.script`: the storyline's shot list and the duration budget it must satisfy.
//!
//! Two properties carry the action, and neither needs a model:
//!
//! * the same brief yields the same shot ids, so a rerun cannot renumber a shot list under a
//!   caller that already holds the ids (`deterministic_ids`);
//! * a script outside the budget is refused by naming the shots that cross the line, not by
//!   silently truncating it (`duration_budget_reports_shots`).

use conveyor_produce::script::{self, BudgetError, MIN_SHOT_MS, TOLERANCE};

/// The brief every test in this file shares.
const BRIEF: &str = "a ninety-second explainer: why a shot list needs stable ids";

#[test]
fn deterministic_ids() {
    // Same brief, same target, twice: identical ids, in the same order.
    let first = script::skeleton(BRIEF, 10_000);
    let second = script::skeleton(BRIEF, 10_000);
    let ids_first: Vec<&str> = first.iter().map(|shot| shot.id.as_str()).collect();
    let ids_second: Vec<&str> = second.iter().map(|shot| shot.id.as_str()).collect();
    assert_eq!(ids_first, ids_second, "the same brief must yield the same shot ids");

    // Ten seconds is ten one-second shots, and the split sums to the target exactly.
    assert_eq!(first.len(), 10);
    let total: u32 = first.iter().map(|shot| shot.duration_ms).sum();
    assert_eq!(total, 10_000);

    // A different brief is a different identity even at the same positions.
    let other = script::skeleton("a different brief entirely", 10_000);
    assert_ne!(ids_first[0], other[0].id);

    // The separator matters: ("ab", 1) must not collide with ("a", 11).
    assert_ne!(script::shot_id("ab", 1), script::shot_id("a", 11));

    // Ids are fixed-width hex, so they sort and compare as opaque strings.
    assert_eq!(first[0].id.len(), 12);
    assert!(first[0].id.chars().all(|c| c.is_ascii_hexdigit()));

    // A populated script keeps the skeleton's ids: binding model text never re-identifies a shot.
    let descriptions = vec![(first[2].id.clone(), "the camera holds on the empty grid".to_string())];
    let script = script::assemble(BRIEF, 10_000, "a through-line", &descriptions);
    let ids_after: Vec<&str> = script.shots.iter().map(|shot| shot.id.as_str()).collect();
    assert_eq!(ids_first, ids_after);
    assert_eq!(script.shots[2].description, "the camera holds on the empty grid");
    // A shot the model did not answer for keeps an empty description rather than borrowing one.
    assert_eq!(script.shots[1].description, "");
}

#[test]
fn duration_budget_reports_shots() {
    // A ten-shot skeleton for ten seconds is exactly on budget.
    let mut script = conveyor_produce::script::Script {
        storyline: "a through-line".to_string(),
        shots: script::skeleton(BRIEF, 10_000),
        target_duration_ms: 10_000,
    };
    assert_eq!(script::validate(&script), Ok(()));

    // Five shots stretched to 3s each: 15s against a 10s budget. The upper bound is
    // 10s × (1 + TOLERANCE) = 11s, so the fourth shot is the one that crosses it.
    for shot in script.shots.iter_mut().take(5) {
        shot.duration_ms = 3_000;
    }
    for shot in script.shots.iter_mut().skip(5) {
        shot.duration_ms = 0;
    }
    let error = script::validate(&script).expect_err("15s cannot pass a 10s budget");
    let BudgetError::Over { target_ms, actual_ms, offending, .. } = error else {
        panic!("expected an over-budget refusal, got {error:?}");
    };
    assert_eq!(target_ms, 10_000);
    assert_eq!(actual_ms, 15_000);
    // Shots 1-3 total 9s (under 11s); shot 4 brings the total to 12s, so shot 4 and everything
    // after it is named. Cutting that suffix is sufficient.
    let expected: Vec<String> = script.shots[3..].iter().map(|shot| shot.id.clone()).collect();
    assert_eq!(offending, expected);

    // Under budget is refused too, and names no shots: there is nothing to cut.
    let mut short = conveyor_produce::script::Script {
        storyline: "a through-line".to_string(),
        shots: script::skeleton(BRIEF, 10_000),
        target_duration_ms: 10_000,
    };
    for shot in short.shots.iter_mut() {
        shot.duration_ms = MIN_SHOT_MS / 2;
    }
    match script::validate(&short) {
        Err(BudgetError::Under { short_by_ms, .. }) => assert!(short_by_ms > 0),
        other => panic!("expected an under-budget refusal, got {other:?}"),
    }

    // TOLERANCE is the documented ten percent, both ways.
    assert!((TOLERANCE - 0.10).abs() < f64::EPSILON);

    // An empty script is a refusal, not a zero-length success.
    let empty = conveyor_produce::script::Script {
        storyline: String::new(),
        shots: Vec::new(),
        target_duration_ms: 10_000,
    };
    assert_eq!(script::validate(&empty), Err(BudgetError::Empty));
}
