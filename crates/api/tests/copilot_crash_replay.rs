//! S10 §5.3 (S10-D3) — a killed worker **replays**; it never runs the action twice.
//!
//! `claim_proposal_execution_tx` short-circuits any ordinary second call, so the replay branch is only
//! reachable from the state a crash leaves behind: the invocation committed, and the approval's
//! `executed_invocation_id` never written. This reproduces exactly that state, because the alternative —
//! asserting on `execute()` returning `Skipped` — proves the short-circuit and never touches the branch
//! the claim exists for.
//!
//! Its own binary for the reason `tests/copilot/mod.rs` gives: `CONVEYOR_EXTERNAL_INFERENCE_URL` is
//! process-global, so one test per process.

mod copilot;

use serde_json::json;
use uuid::Uuid;

use conveyor_api::copilot::proposal::{Execution, execute};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crashed_execution_replays_on_the_proposal_key_and_runs_nothing_twice() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let token = "dev:alice";

    let (status, created) = running
        .invoke(
            "projects.create",
            token,
            json!({"title": "Copilot replay"}),
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

    model.tool_call("projects_close", json!({"id": project_id}));
    let conversation = copilot::conversation(&running, token).await;
    let (status, body) = copilot::turn(&running, token, conversation, "close the project").await;
    assert_eq!(status, 201, "{body}");
    let approval_id =
        Uuid::parse_str(body["proposal_id"].as_str().expect("a proposal id")).expect("uuid");

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

    let first = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("first execute");
    let Execution::Executed(first_invocation) = first else {
        panic!("the approved proposal must execute: {first:?}");
    };
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "projects.close").await,
        1
    );

    // While the terminal marker stands, a second call is a no-op. That is the *other* guarantee.
    let short_circuited = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("second execute");
    assert_eq!(short_circuited, Execution::Skipped, "already executed");

    // The crash window: the invocation committed, the approval never learned of it.
    sqlx::query(
        "UPDATE approval
            SET meta = meta - 'executed_invocation_id' - 'executed_at'
          WHERE id = $1 AND kind = 'copilot.action'",
    )
    .bind(approval_id)
    .execute(&pg.super_pool)
    .await
    .expect("simulate the lost terminal write");

    let replayed = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("replay");
    assert_eq!(
        replayed,
        Execution::Executed(first_invocation),
        "the replay must cite the invocation the first attempt wrote"
    );
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "projects.close").await,
        1,
        "one invocation across both attempts: the key made the retry a replay, not a re-run"
    );
}
