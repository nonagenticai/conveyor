//! S10 §6 / CONTRACT §17 — the round cap is **hard**, and the refusal is never a silent truncation.
//!
//! Its own binary for the reason `tests/copilot/mod.rs` gives: `CONVEYOR_EXTERNAL_INFERENCE_URL` is
//! process-global, and `copilot_turn` sets it empty to prove the unavailable-router path. Two tests in
//! one process would race on it.

mod copilot;

use conveyor_domain::copilot::MAX_ROUNDS;

/// The stub answers every round with a tool call, so the loop can only leave through `MAX_ROUNDS`.
///
/// The tool it asks for is schema-valid and on the caller's surface but names a row that does not exist:
/// a refused tool is an agent-loop event (the model is told why and may recover), which is exactly what
/// makes the cap reachable instead of ending the turn on round one.
///
/// ⚠️ The terminal state is `refused` (422, `/problems/copilot-refused`) — **not** `failed`. A failed
/// turn is the router being unavailable; a refused turn is the copilot declining to go further.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_turn_that_never_stops_calling_tools_is_refused_at_the_round_cap() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;

    let missing = uuid::Uuid::now_v7();
    for _ in 0..=MAX_ROUNDS {
        model.tool_call("cards_get", serde_json::json!({ "id": missing }));
    }

    let conversation = copilot::conversation(&running, "dev:alice").await;
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "keep going").await;

    // A refused turn is a **created** turn whose terminal state says why: the handler returns the
    // problem status only for `failed` (the router was unusable, nothing usable was produced). The
    // refusal itself rides the record — `meta.status` + `meta.error` — and the body is its detail.
    assert_eq!(status, 201, "the turn is recorded, not rejected: {body}");
    let meta = &body["turn"]["meta"];
    assert_eq!(meta["status"], "refused", "{body}");
    assert_eq!(meta["error"]["type"], "/problems/copilot-refused", "{body}");
    assert_eq!(meta["error"]["status"], 422, "{body}");
    assert!(
        body["turn"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("exceeded its 8 tool rounds"),
        "the refusal must name the cap, not truncate silently: {body}"
    );

    let page = copilot::turns(&running, "dev:alice", conversation).await;
    let assistant = &page["items"][1];
    assert_eq!(assistant["meta"]["status"], "refused", "{assistant}");
    assert_eq!(
        assistant["meta"]["error"]["type"], "/problems/copilot-refused",
        "{assistant}"
    );
    assert_eq!(
        assistant["meta"]["tool_calls"].as_array().map(Vec::len),
        Some(usize::from(MAX_ROUNDS)),
        "one recorded tool call per round and not one more: {assistant}"
    );
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "completions.create").await,
        i64::from(MAX_ROUNDS),
        "the model is asked exactly MAX_ROUNDS times"
    );
    assert_eq!(
        model.hits(),
        usize::from(MAX_ROUNDS),
        "the answer after the cap is never requested"
    );
}
