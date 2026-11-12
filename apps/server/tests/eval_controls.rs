//! S19's controls through the real runner and grader (S19 §7 gate line 8).
//!
//! An eval is unproven until three things hold, and none of them is a happy-path demo:
//!
//! - **the oracle scores ≈100 %** — a subject that answers correctly passes;
//! - **the null scores ≈0 %** — a subject that answers the wrong thing does not;
//! - **an induced failure is a classified attempt, not a zero** — a 5xx leaves the `(case, rep)`
//!   slot empty and appends a `serving` row to `eval_attempt_error`.
//!
//! The fourth control — a judge must reject its three known negatives — is exercised through
//! `eval_audit.rs`'s `judge_accepts_negatives` reversal, where the stub can be made lenient.

mod common;

use conveyor_server::worker;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;

#[path = "eval_support/mod.rs"]
mod fx;

fn config() -> conveyor_server::Config {
    conveyor_server::Config::from_lookup(|_| None).expect("a default config")
}

/// One `eval.case.run` job per case, exactly as `eval_runs.start` writes them.
async fn run_all_cases(db: &common::TestDb, fixture: &fx::Fixture) {
    for case in &fixture.cases {
        let mut tx = begin_scoped(&db.owner, fixture.tenant)
            .await
            .expect("scope");
        store::jobs::enqueue_tx(
            &mut tx,
            fixture.tenant,
            "eval.case.run",
            &format!("{}:{}:0", fixture.run_id, case),
            json!({"run_id": fixture.run_id, "case_id": case, "rep": 0}),
            None,
            5,
        )
        .await
        .expect("enqueue the case job");
        tx.commit().await.expect("commit");
        // A failed attempt is this test's subject in one case; the runner records it in
        // `eval_attempt_error` and returns the failure, so the return value is not asserted here.
        let _ = worker::run_kind(
            &db.runtime,
            &config(),
            None,
            fixture.tenant,
            "eval.case.run",
            &json!({"run_id": fixture.run_id, "case_id": case, "rep": 0}),
        )
        .await;
    }
}

struct Counts {
    results: i64,
    mean: Option<f64>,
    errors: i64,
    error_class: Option<String>,
}

async fn counts(db: &common::TestDb, fixture: &fx::Fixture) -> Counts {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let results: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_case_result WHERE tenant_id = $1 AND eval_run_id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count results");
    let mean: Option<f64> = sqlx::query_scalar(
        "SELECT avg((grade->>'label_accuracy')::numeric)::float8 FROM eval_case_result
          WHERE tenant_id = $1 AND eval_run_id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("mean grade");
    let errors: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_attempt_error WHERE tenant_id = $1 AND eval_run_id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count attempts");
    let error_class: Option<String> = sqlx::query_scalar(
        "SELECT failure_class FROM eval_attempt_error
          WHERE tenant_id = $1 AND eval_run_id = $2 LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_optional(&mut *tx)
    .await
    .expect("read the class");
    tx.commit().await.expect("commit");
    Counts {
        results,
        mean,
        errors,
        error_class,
    }
}

#[tokio::test]
async fn the_oracle_scores_one_through_the_runner_and_grader() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // The stub answers `billing`; the answer key is `billing`.
    let fixture = fx::fixture(&db, &["classify: one", "classify: two", "classify: three"]).await;
    run_all_cases(&db, &fixture).await;

    let counts = counts(&db, &fixture).await;
    assert_eq!(counts.results, 3, "every case produced a scorable result");
    let mean = counts.mean.expect("a mean");
    assert!(
        (mean - 1.0).abs() < 1e-9,
        "the oracle must score 1.0, got {mean}"
    );
    assert_eq!(counts.errors, 0, "no attempt failed");
}

#[tokio::test]
async fn the_null_scores_zero_through_the_runner_and_grader() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // `MISS` makes the stub answer `refund`; the answer key stays `billing`.
    let fixture = fx::fixture(&db, &["MISS one", "MISS two"]).await;
    run_all_cases(&db, &fixture).await;

    let counts = counts(&db, &fixture).await;
    assert_eq!(counts.results, 2);
    let mean = counts.mean.expect("a mean");
    assert!(
        (mean - 0.0).abs() < 1e-9,
        "the null must score 0.0, got {mean}"
    );
}

#[tokio::test]
async fn an_induced_error_is_a_classified_attempt_and_never_a_slot() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // `BOOM` makes the stub answer 500 on every attempt.
    let fixture = fx::fixture(&db, &["BOOM one"]).await;
    run_all_cases(&db, &fixture).await;

    let counts = counts(&db, &fixture).await;
    assert_eq!(
        counts.results, 0,
        "a failed attempt never occupies the slot"
    );
    assert!(counts.errors >= 1, "the attempt is recorded");
    assert_eq!(
        counts.error_class.as_deref(),
        Some("serving"),
        "an upstream 5xx is a serving failure, not a model failure"
    );
}
