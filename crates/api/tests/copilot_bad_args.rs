//! A tool call whose arguments are not JSON must fail the turn, never run with fabricated `{}`.
//!
//! Its own binary for the reason `tests/copilot/mod.rs` gives: `CONVEYOR_EXTERNAL_INFERENCE_URL` is
//! process-global.

mod copilot;

use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_call_with_malformed_arguments_fails_the_turn() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;

    model.push(json!({
        "model": "stub-model",
        "choices": [{
            "finish_reason": "tool_calls",
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": {"name": "cards_get", "arguments": "{\"id\":"}
                }]
            }
        }]
    }));

    let (status, body) =
        copilot::turn(&running, "dev:alice", conversation, "where is my card?").await;
    assert_eq!(status, 422, "malformed arguments are a failed turn: {body}");
    assert_eq!(
        body["type"], "inference-bad-tool-arguments",
        "the typed refusal: {body}"
    );

    let page = copilot::turns(&running, "dev:alice", conversation).await;
    let assistant = &page["items"][1];
    assert_eq!(assistant["meta"]["status"], "failed", "{assistant}");
    assert_eq!(
        assistant["meta"]["error"]["type"], "/problems/inference-bad-tool-arguments",
        "{assistant}"
    );

    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "cards.get").await,
        0,
        "the tool must not run with an empty or partial input"
    );
}
