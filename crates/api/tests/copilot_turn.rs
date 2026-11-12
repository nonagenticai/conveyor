//! S10 §7 (f): an unavailable router fails the turn, records the failure, and runs **no** tool.
//!
//! An unconfigured service is the deterministic form of "the breaker is open": the engine refuses
//! with `503 action-service-unconfigured` before any HTTP call, which is the same refusal a broken
//! breaker produces (`crates/actions/src/invoke.rs:296-302,759-776`).

mod copilot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unavailable_router_fails_the_turn_and_runs_no_tool() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    // The service is now unconfigured: the action refuses before it dials.
    copilot::support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", "");

    let conversation = copilot::conversation(&running, "dev:alice").await;
    let (status, body) = copilot::turn(
        &running,
        "dev:alice",
        conversation,
        "move card C to the Done list",
    )
    .await;

    assert_eq!(status, 503, "an unavailable router must answer 503: {body}");
    assert_eq!(body["type"], "action-service-unconfigured", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("recorded as failed"),
        "the problem must say the turn was recorded: {body}"
    );

    // The durable record: one assistant turn, terminal `failed`, with the same problem inside.
    let page = copilot::turns(&running, "dev:alice", conversation).await;
    let turns = page["items"].as_array().expect("items");
    assert_eq!(
        turns.len(),
        2,
        "the user turn and the assistant turn: {page}"
    );
    let assistant = &turns[1];
    assert_eq!(assistant["role"], "assistant");
    assert_eq!(assistant["meta"]["status"], "failed", "{assistant}");
    assert_eq!(assistant["meta"]["error"]["status"], 503, "{assistant}");
    assert_eq!(
        assistant["meta"]["error"]["type"],
        "/problems/action-service-unconfigured"
    );

    // Zero tool invocations, zero model calls: the copilot never invents a tool call.
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "cards.move").await,
        0
    );
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "completions.create").await,
        1,
        "the failed model call itself is recorded as one invocation"
    );
    assert_eq!(model.hits(), 0, "no HTTP call reached the router");
}
