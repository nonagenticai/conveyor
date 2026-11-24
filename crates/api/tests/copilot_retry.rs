//! A transient router failure is retried; a persistent one is not.
//!
//! Its own binary for the reason `tests/copilot/mod.rs` gives: `CONVEYOR_EXTERNAL_INFERENCE_URL`
//! is process-global.

mod copilot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transient_router_failure_is_retried_and_three_in_a_row_are_not() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;

    // One 502, then the answer: the turn completes on the second attempt, under the same
    // `(turn, round)` idempotency key (the engine released the claim when the call failed).
    model.fail_next(1);
    model.text("the answer");
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "hello").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["turn"]["meta"]["status"], "completed", "{body}");
    assert!(
        body["turn"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("the answer"),
        "{body}"
    );
    assert_eq!(model.hits(), 2, "one failed attempt and one served attempt");
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "completions.create").await,
        2,
        "the failed attempt is recorded and the retry is its own invocation"
    );

    // Three 502s in a row: the turn ends failed after exactly three attempts, never a fourth.
    model.fail_next(3);
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "again").await;
    assert_eq!(
        status, 502,
        "a persistent router failure still fails the turn: {body}"
    );
    assert_eq!(model.hits(), 5, "three more attempts and no fourth");
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "completions.create").await,
        5
    );
}
