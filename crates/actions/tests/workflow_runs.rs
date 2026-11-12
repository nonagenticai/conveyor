//! `workflow_runs.start|get|cancel` plus the `workflow.node` executor (S13 steps 6–7).
//!
//! The last test is the point of the slice: a published workflow runs its action node end to end —
//! start creates the run and its tick, the tick advances the engine and invokes the action, the
//! next tick settles the run — with the trace (`run_step`) as the only state between ticks.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} is missing from {value}"))
        .to_owned()
}

/// A fresh database with the dev catalogue seeded (projects.create needs a board template).
async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
}

/// Create a project (which creates its board and lists) and one card in its first lane.
async fn project_and_card(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Workflow run test"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = id_of(output(&created), "id");

    let (status, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let boards = output(&view)["boards"].as_array().expect("boards");
    let board_id = id_of(&boards[0], "id");
    let (status, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let list_id = id_of(&output(&board)["lists"][1]["list"], "id");

    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": "First card",
                "card_type_code": "feature"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    (project_id, id_of(output(&card), "id"))
}

/// Create and publish a one-action workflow (`cards.get` on the run's card).
async fn published_workflow(running: &support::Running) -> String {
    let (status, created) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({
                "key": "cap.x",
                "title": "Capability X",
                "definition": {
                    "schema": 1,
                    "nodes": [{
                        "id": "read",
                        "kind": "action",
                        "action": "cards.get",
                        "input_template": {"id": "{{card.id}}"}
                    }],
                    "edges": []
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let workflow_id = id_of(output(&created), "workflow_id");
    let (status, published) = running
        .invoke(
            "workflows.publish",
            "dev:alice",
            json!({"workflow_id": workflow_id}),
            Some("1"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    workflow_id
}

#[tokio::test]
async fn start_creates_a_running_run_and_enqueues_one_tick() {
    let (pg, seeded, running) = fixture().await;
    let (project_id, card_id) = project_and_card(&running).await;
    let workflow_id = published_workflow(&running).await;

    let (status, body) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": workflow_id, "project_id": project_id, "card_id": card_id}),
            None,
            None,
            Some("rid-start"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run = output(&body);
    assert_eq!(run["status"], "running");
    let run_id = id_of(run, "id");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (workflow,): (Option<uuid::Uuid>,) =
        sqlx::query_as("SELECT workflow_id FROM run WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&run_id).expect("uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("read run");
    let (jobs, kind): (i64, String) = sqlx::query_as(
        "SELECT count(*), min(kind) FROM job
          WHERE payload->>'run_id' = $1 AND status IN ('pending', 'running')",
    )
    .bind(&run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read jobs");
    tx.commit().await.expect("commit");
    assert_eq!(
        workflow,
        Some(uuid::Uuid::parse_str(&workflow_id).expect("uuid")),
        "the run names its workflow"
    );
    assert_eq!(jobs, 1, "one live tick per run");
    assert_eq!(kind, "workflow.node");
}

#[tokio::test]
async fn start_refuses_a_draft_and_a_second_live_run_on_the_same_card() {
    let (_pg, _seeded, running) = fixture().await;
    let (project_id, card_id) = project_and_card(&running).await;

    // A draft never runs.
    let (status, created) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({
                "key": "cap.draft",
                "title": "Draft",
                "definition": {"schema": 1, "nodes": [{"id": "a", "kind": "action", "action": "cards.get"}], "edges": []}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let draft_id = id_of(output(&created), "workflow_id");
    let (status, body) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": draft_id, "project_id": project_id, "card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");

    // One live run per card is the database's rule (`uq_run_live`).
    let workflow_id = published_workflow(&running).await;
    let (status, first) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": workflow_id, "project_id": project_id, "card_id": card_id}),
            None,
            None,
            Some("rid-1"),
        )
        .await;
    assert_eq!(status, 201, "{first}");
    let (status, second) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": workflow_id, "project_id": project_id, "card_id": card_id}),
            None,
            None,
            Some("rid-2"),
        )
        .await;
    assert_eq!(status, 409, "{second}");
}

#[tokio::test]
async fn a_started_workflow_runs_its_action_node_and_succeeds() {
    let (pg, seeded, running) = fixture().await;
    let (project_id, card_id) = project_and_card(&running).await;
    let workflow_id = published_workflow(&running).await;

    let (status, body) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": workflow_id, "project_id": project_id, "card_id": card_id}),
            None,
            None,
            Some("rid-start"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = uuid::Uuid::parse_str(&id_of(output(&body), "id")).expect("uuid");

    // Tick 1 executes the node; tick 2 sees every node finished and settles the run.
    for tick in 1..=2 {
        conveyor_actions::workflow::node::execute_node(&pg.runtime, seeded.tenant, "dev", run_id)
            .await
            .unwrap_or_else(|failure| panic!("tick {tick}: {}", failure.message));
    }

    let (status, run) = running
        .invoke(
            "workflow_runs.get",
            "dev:alice",
            json!({"run_id": run_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(output(&run)["status"], "succeeded");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let rows: Vec<(String, String, i32, String)> = sqlx::query_as(
        "SELECT node_id, status, position, action FROM run_step WHERE run_id = $1
          ORDER BY position, attempt",
    )
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read trace");
    tx.commit().await.expect("commit");
    assert_eq!(rows.len(), 1, "one executed node, one row: {rows:?}");
    assert_eq!(rows[0].0, "read");
    assert_eq!(rows[0].1, "succeeded");
    assert_eq!(rows[0].2, 1);
    assert_eq!(rows[0].3, "cards.get");
}

#[tokio::test]
async fn cancel_stops_the_tick_without_advancing_the_run() {
    let (pg, seeded, running) = fixture().await;
    let (project_id, card_id) = project_and_card(&running).await;
    let workflow_id = published_workflow(&running).await;

    let (status, body) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": workflow_id, "project_id": project_id, "card_id": card_id}),
            None,
            None,
            Some("rid-start"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = uuid::Uuid::parse_str(&id_of(output(&body), "id")).expect("uuid");

    let (status, cancelled) = running
        .invoke(
            "workflow_runs.cancel",
            "dev:alice",
            json!({"run_id": run_id, "reason": "operator"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{cancelled}");
    assert_eq!(output(&cancelled)["status"], "cancelled");

    // The tick is claimed anyway; it must write nothing for a run that is no longer live.
    conveyor_actions::workflow::node::execute_node(&pg.runtime, seeded.tenant, "dev", run_id)
        .await
        .expect("a cancelled run is a no-op, not a failure");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM run_step WHERE run_id = $1")
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await
        .expect("count rows");
    let (run_status,): (String,) = sqlx::query_as("SELECT status FROM run WHERE id = $1")
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await
        .expect("read run");
    tx.commit().await.expect("commit");
    assert_eq!(rows, 0, "a cancelled run advances nothing");
    assert_eq!(run_status, "cancelled");
}
