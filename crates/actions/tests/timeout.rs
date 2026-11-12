//! The caller's deadline (S04 trap 4): `fixtures.slow` sleeps 600 ms with a 200 ms timeout.
//! The transaction rolls back, the timeout is recorded in a SEPARATE short transaction, and
//! the idempotency claim is released so the action is retryable.

mod support;

use conveyor_store::begin_scoped;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_action_times_out_rolls_back_and_records_a_timeout_row() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let stub = support::Stub::start().await;
    support::set_env(support::STUB_ENV, &stub.base);
    let running = support::Running::start(&pg).await;

    let key = "slow-once";
    let (status, body) = running
        .invoke(
            "fixtures.slow",
            "dev:alice",
            json!({"delay_ms": 600}),
            None,
            Some(key),
            Some("rid-slow"),
        )
        .await;
    assert_eq!(status, 504, "{body}");
    assert_eq!(body["type"], "action-timeout");
    assert!(
        stub.hits.get() >= 1,
        "the upstream call must have started (the timeout is the CALLER's, not a pre-check)"
    );

    // The rolled-back transaction still leaves ONE timeout row and ONE error audit row.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (timeouts,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM action_invocation
          WHERE action = 'fixtures.slow' AND outcome = 'timeout'",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("count timeouts");
    let (errors,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_event
          WHERE action = 'fixtures.slow' AND status = 'error'",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("count audit errors");
    tx.commit().await.expect("commit");
    assert_eq!(
        timeouts, 1,
        "a timeout must record itself outside its own transaction"
    );
    assert_eq!(errors, 1, "and it is an audit error, not a silent drop");

    // The key was released: a retry runs again (and times out again), never 409 in-flight.
    let (status, body) = running
        .invoke(
            "fixtures.slow",
            "dev:alice",
            json!({"delay_ms": 600}),
            None,
            Some(key),
            None,
        )
        .await;
    assert_eq!(status, 504, "{body}");

    running.shutdown().await;
}
