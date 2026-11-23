//! A router answer cut at its token cap must fail the turn — never be delivered as complete.
//!
//! Its own binary for the reason `tests/copilot/mod.rs` gives: `CONVEYOR_EXTERNAL_INFERENCE_URL` is
//! process-global.

mod copilot;

use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_truncated_at_the_token_cap_fails_the_turn() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;

    // A partial tool call: the argument JSON is cut mid-string, exactly what a `length` finish
    // leaves behind. If the finish_reason is ignored, this parses as a tool call with `{}`.
    model.push(json!({
        "model": "stub-model",
        "choices": [{
            "finish_reason": "length",
            "message": {
                "role": "assistant",
                "content": "the partial answer",
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": {"name": "cards_get", "arguments": "{\"id\":\""}
                }]
            }
        }]
    }));

    let (status, body) =
        copilot::turn(&running, "dev:alice", conversation, "where is my card?").await;
    assert_eq!(status, 422, "a truncated answer is a failed turn: {body}");
    assert_eq!(
        body["type"], "inference-truncated",
        "the typed refusal, not a generic failure: {body}"
    );

    // The turn is durable and failed; the partial text is not the answer.
    let page = copilot::turns(&running, "dev:alice", conversation).await;
    let assistant = &page["items"][1];
    assert_eq!(assistant["meta"]["status"], "failed", "{assistant}");
    assert_eq!(
        assistant["meta"]["error"]["type"], "/problems/inference-truncated",
        "{assistant}"
    );
    assert_ne!(
        assistant["body"], "the partial answer",
        "the truncated text must not be delivered as the answer"
    );

    // The whole point: the partial call never ran.
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "cards.get").await,
        0,
        "a truncated tool call must not execute"
    );
}
