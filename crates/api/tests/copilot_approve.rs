//! S10 §7 (b, c): an approval gates execution, and a second decision changes nothing.
//!
//! `projects.close` is `effect = destructive`, so the same proposal machinery every destructive
//! action uses is exercised end to end: propose → approve → execute → one invocation, the project
//! closed, and the originating turn `completed` with the executed tool call (S10 §7 b). Property (c)
//! is the three-layer guarantee: a second decision by the same approver is refused by the
//! `(approval_id, approver_id)` unique index, a second execution claim finds the proposal executed,
//! and the invocation count stays 1.

mod copilot;

use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_api::copilot::proposal::{Execution, execute};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approval_gates_execution_and_a_second_decision_changes_nothing() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `projects.create` builds its board from the catalogue; without it the action refuses.
    copilot::catalogue(&pg).await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let token = "dev:alice";

    // A real project, created through the action surface.
    let (status, created) = running
        .invoke(
            "projects.create",
            token,
            json!({"title": "Copilot cutover"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "projects.create: {created}");
    let project_id = created["output"]["id"]
        .as_str()
        .expect("a project id")
        .to_owned();

    // The model proposes closing it.
    model.tool_call("projects_close", json!({"id": project_id}));
    let conversation = copilot::conversation(&running, token).await;
    let (status, body) = copilot::turn(&running, token, conversation, "close the project").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["turn"]["meta"]["status"], "proposed", "{body}");
    let approval_id =
        Uuid::parse_str(body["proposal_id"].as_str().expect("a proposal id")).expect("uuid");
    let turn_id = Uuid::parse_str(body["turn"]["id"].as_str().expect("turn id")).expect("uuid");

    // Approve as an authorized member.
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            token,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "approvals.decide: {decided}");

    // Execute exactly as the worker's `copilot.proposal.execute` does.
    let outcome = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("execute the proposal");
    assert!(
        matches!(outcome, Execution::Executed(_)),
        "the approved proposal must execute: {outcome:?}"
    );

    // (b) exactly one invocation, and the project is closed.
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "projects.close").await,
        1
    );
    let (status, project) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/projects/{project_id}"),
            Some(token),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{project}");
    assert_eq!(project["project"]["status"], "closed", "{project}");

    // The originating turn is completed with the executed tool call.
    let meta = copilot::turn_meta(&pg, turn_id).await;
    assert_eq!(meta["status"], "completed", "{meta}");
    assert_eq!(
        meta["approval_id"],
        Value::String(approval_id.to_string()),
        "the completed turn still cites its proposal"
    );
    let calls = meta["tool_calls"].as_array().expect("tool_calls");
    assert_eq!(calls.len(), 1, "{meta}");
    assert_eq!(calls[0]["action"], "projects.close");
    assert!(
        calls[0]["invocation_id"].is_string(),
        "the executed invocation is cited: {meta}"
    );

    // (c) a second decision by the same approver is refused by the unique index.
    let (second, second_body) = running
        .invoke(
            "approvals.decide",
            token,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert!(
        second == 409 || second == 200,
        "a second decision is refused or a no-op, never a second count: {second} {second_body}"
    );

    // A second execution claim finds it executed and does nothing.
    let again = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("second execute");
    assert_eq!(again, Execution::Skipped, "already executed");
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "projects.close").await,
        1,
        "two claims, one effect"
    );
}
