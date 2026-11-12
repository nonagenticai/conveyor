//! S19 Part B step 9 — the four runner properties, each driven through the real driver.
//!
//! 1. **`deadline_at` is a total ceiling**, not an inactivity timer: bytes arriving never push it
//!    back, and once passed it stays passed.
//! 2. **Jittered backoff with the attempt visible** — the attempt number is in the row before the
//!    retry is scheduled, and the delay is the eval's own policy, not a constant.
//! 3. **Strict per-attempt scoring** — the row holds the score of the attempt that wrote it. It
//!    is never a merge of two attempts, which would hide a regression behind a lucky retry.
//! 4. **A failure class on every failed attempt** — by whose fault it was, in the CHECK's
//!    spelling. `timeout` is the one class a test cannot drive (it needs a 300 s wall clock), so
//!    its classification is asserted at the domain's own function instead and said so.

mod common;

use conveyor_domain::eval::{CaseDeadline, FailureClass};
use conveyor_server::eval_runner::run_case;
use serde_json::json;

#[path = "eval_support/mod.rs"]
mod fx;

#[test]
fn a_deadline_is_a_total_ceiling_and_nothing_pushes_it_back() {
    let started = chrono::Utc::now();
    let deadline = CaseDeadline::starting(started, std::time::Duration::from_secs(300));

    assert_eq!(
        deadline.deadline_at(),
        started + chrono::Duration::seconds(300)
    );
    assert!(!deadline.is_expired(started));
    assert!(!deadline.is_expired(deadline.deadline_at() - chrono::Duration::milliseconds(1)));
    assert!(
        deadline.is_expired(deadline.deadline_at()),
        "at the ceiling the attempt is over"
    );
    // ⛔ The property that separates a ceiling from an inactivity timer: a stream that keeps
    // emitting bytes produces `now` values that move forward, and every one of them is expired.
    let mut now = deadline.deadline_at();
    for _ in 0..5 {
        now += chrono::Duration::seconds(1);
        assert!(
            deadline.is_expired(now),
            "a later `now` can never make an expired attempt live again"
        );
    }
    assert_eq!(
        deadline.deadline_at(),
        started + chrono::Duration::seconds(300),
        "and the deadline itself never moves"
    );

    // `408` is the ceiling by another name (`FailureClass::from_status`); the driver cannot
    // reach its own 300 s path in a test, so this is where that mapping is held.
    assert_eq!(FailureClass::from_status(408), FailureClass::Timeout);
    assert!(FailureClass::Timeout.is_retryable());
}

#[tokio::test]
async fn every_failed_attempt_carries_its_class_and_its_attempt_number() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // `BOOM` is a permanent 5xx for this case: every call fails, which is what makes the
    // attempt NUMBER observable rather than implicit.
    let fixture = fx::fixture(&db, &["classify: BOOM"]).await;
    let case = fixture.cases[0];
    let run = |case| {
        let db = &db;
        let fixture = &fixture;
        async move {
            run_case(
                &db.runtime,
                "conveyor",
                fixture.tenant,
                fixture.run_id,
                case,
                0,
            )
            .await
        }
    };

    let first = run(case).await.expect_err("a 5xx fails the attempt");
    let second = run(case).await.expect_err("a 5xx fails the attempt again");

    let errors = fx::attempt_errors(&db, &fixture).await;
    assert_eq!(
        errors,
        vec![(1, "serving".to_owned(), 0), (2, "serving".to_owned(), 1)],
        "the attempt number and retry count are written BEFORE the retry is scheduled"
    );
    assert!(
        fx::results(&db, &fixture).await.is_empty(),
        "no slot was written"
    );

    // Jittered: the delay is a function of the attempt, not a constant.
    assert!(first.retry_after > std::time::Duration::ZERO, "{first:?}");
    assert!(second.retry_after > std::time::Duration::ZERO, "{second:?}");
    assert_ne!(
        first.retry_after, second.retry_after,
        "an identical delay per attempt is a fixed interval, not a backoff: {first:?} {second:?}"
    );
    let policy = conveyor_domain::eval::RetryPolicy::default();
    assert!(
        second.retry_after <= std::time::Duration::from_millis(policy.cap_ms),
        "the delay saturates at the eval's own cap, never grows without bound: {second:?}"
    );
    // A `serving` failure is the service's fault, so it stays retryable until the attempts run
    // out — never `permanent` on the first 5xx.
    assert!(
        !first.permanent && !second.permanent,
        "{first:?} {second:?}"
    );
}

#[tokio::test]
async fn the_row_holds_the_score_of_the_attempt_that_wrote_it() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // One case answers the expected label, one answers another.
    let fixture = fx::fixture(&db, &["classify: hit", "classify: MISS"]).await;
    for case in &fixture.cases {
        run_case(
            &db.runtime,
            "conveyor",
            fixture.tenant,
            fixture.run_id,
            *case,
            0,
        )
        .await
        .expect("the driver ran the case");
    }

    let rows = fx::results(&db, &fixture).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let mut scores: Vec<f64> = rows
        .iter()
        .map(|row| {
            row.grade["label_accuracy"]
                .as_f64()
                .unwrap_or_else(|| panic!("a numeric score: {row:?}"))
        })
        .collect();
    scores.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert_eq!(
        scores,
        vec![0.0, 1.0],
        "each slot holds its own attempt's score; an average of 0.5 would be a merge across \
         attempts and would hide both the pass and the failure: {rows:?}"
    );
}

#[tokio::test]
async fn a_grader_that_cannot_decide_is_a_grader_failure_not_a_zero() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // A `rubric` grader whose judge is the subject: the guard refuses before any token is
    // spent, and the attempt is a `grader` failure — never a zero, which would read as a wrong
    // answer and inflate every regression the instrument exists to find.
    let fixture = fx::fixture_with(
        &db,
        &["classify: graded"],
        &[fx::MetricSpec {
            key: "judge_quality",
            grader_kind: "rubric",
            grader: json!({"judge_model": "eval-model"}),
            rubric: Some("Score the answer against the label."),
        }],
    )
    .await;
    let case = fixture.cases[0];

    let failure = run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect_err("a judge that is the subject cannot grade");
    assert!(failure.message.contains("judge_is_subject"), "{failure:?}");
    assert!(
        fx::results(&db, &fixture).await.is_empty(),
        "a grader failure writes no slot"
    );
    let errors = fx::attempt_errors(&db, &fixture).await;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].1, "grader", "{errors:?}");
}

#[tokio::test]
async fn a_case_input_the_subject_cannot_take_is_a_harness_failure() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // `inputs_schema` is a non-empty object, so the case text must parse as one. This one does
    // not, which is the caller's fault — `harness`, and never a subject failure.
    let fixture = fx::fixture(&db, &["{not json"]).await;
    let case = fixture.cases[0];

    let failure = run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect_err("an unparseable case input is a harness failure");
    assert!(
        failure.permanent,
        "no retry can fix a case the instrument cannot express: {failure:?}"
    );
    let errors = fx::attempt_errors(&db, &fixture).await;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].1, "harness", "{errors:?}");
    assert!(fx::results(&db, &fixture).await.is_empty());
}
