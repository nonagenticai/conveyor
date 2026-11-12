//! S10 §7 (a) as a scripted session: a read tool runs, the model answers, the turn completes.
//!
//! This is the loop without a proposal: `projects.get` has `effect = read` and no hint, so the tool
//! runs inside the turn, its invocation id is recorded, and the model's second answer closes the turn
//! `completed`. The proposal half of the scripted session is `copilot_proposal.rs`.

mod copilot;

use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scripted_session_runs_a_read_tool_and_completes() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;

    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Scripted session"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = created["output"]["id"]
        .as_str()
        .expect("a project id")
        .to_owned();

    // Round 1: the model reads the project. Round 2: it answers.
    model.tool_call("projects_get", json!({"id": project_id}));
    model.text("The project is open and has one board.");

    let conversation = copilot::conversation(&running, "dev:alice").await;
    let (status, body) = copilot::turn(
        &running,
        "dev:alice",
        conversation,
        "what does the project look like?",
    )
    .await;
    assert_eq!(status, 201, "{body}");

    let turn = &body["turn"];
    assert_eq!(turn["meta"]["status"], "completed", "{body}");
    assert_eq!(turn["body"], "The project is open and has one board.");
    let calls = turn["meta"]["tool_calls"].as_array().expect("tool_calls");
    assert_eq!(calls.len(), 1, "{body}");
    assert_eq!(calls[0]["action"], "projects.get");
    assert_eq!(calls[0]["outcome"], "ok");
    assert!(
        calls[0]["invocation_id"].is_string(),
        "a committed tool call is cited by its invocation: {body}"
    );
    assert_eq!(turn["meta"]["model"]["action"], "completions.create");

    // Two model rounds, one real action, one turn.
    assert_eq!(model.hits(), 2);
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "projects.get").await,
        1
    );

    // The conversation holds exactly the user turn and the assistant turn, ascending by seq.
    let page = copilot::turns(&running, "dev:alice", conversation).await;
    let items = page["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "{page}");
    assert_eq!(items[0]["role"], "user");
    assert_eq!(items[0]["seq"], 1);
    assert_eq!(items[1]["role"], "assistant");
    assert_eq!(items[1]["seq"], 2);
    assert!(page["next_seq"].is_null());
}
