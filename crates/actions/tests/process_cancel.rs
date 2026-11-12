//! S11 step 9 — cancelling a process run.
//!
//! The public cancel is S06's `runs.cancel`; S11 composes it rather than adding a second cancel
//! surface. These tests pin what the process layer adds: every remaining `run_step` is
//! `skipped` and announced, a pending gate is voided (so it can never be answered), and a run
//! that is already terminal is untouched.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// A project with its template board, and that board's `active` lane.
async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Cancel test {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");

    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = text(&output(&view)["boards"][0], "id");

    let (status, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    (board_id, text(&lane["list"], "id"))
}

async fn create_card(running: &support::Running, board: &str, lane: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

async fn create_process(running: &support::Running, key: &str, sop: &str, steps: Value) -> String {
    // S13: every step names the published SOP it is judged against.
    let mut steps = steps;
    if let Some(items) = steps.as_array_mut() {
        for step in items {
            step["procedure_id"] = json!(sop);
        }
    }
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({"key": key, "title": format!("Process {key}"), "steps": steps}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn publish(running: &support::Running, id: &str) {
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

async fn start(running: &support::Running, card: &str, key: &str) -> String {
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": key, "inputs": {}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn execute(pg: &support::Pg, seeded: &support::Seeded, run: &str, position: i32) {
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        Uuid::parse_str(run).expect("run id"),
        position,
        1,
    )
    .await
    .unwrap_or_else(|failure| panic!("step {position} failed: {}", failure.message));
}

async fn run_status(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> String {
    sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(run).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row")
}

async fn step_statuses(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
) -> Vec<(i32, String)> {
    sqlx::query_as(
        "SELECT position, status FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND attempt = 1 ORDER BY position",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the run steps")
}

async fn events(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT type FROM event WHERE tenant_id = $1 AND topic = $2 ORDER BY seq")
        .bind(seeded.tenant_id)
        .bind(format!("run:{run}"))
        .fetch_all(&pg.super_pool)
        .await
        .expect("run events")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_skips_the_remaining_steps_and_announces_each() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-cancel-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-cancel",
        &sop,
        json!([
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-cancel").await;

    execute(&pg, &seeded, &run, 1).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "running");

    let (status, body) = running
        .invoke(
            "runs.cancel",
            TOKEN,
            json!({"id": run, "reason": "test cancel"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    assert_eq!(run_status(&pg, &seeded, &run).await, "cancelled");
    assert_eq!(
        step_statuses(&pg, &seeded, &run).await,
        vec![
            (1, "succeeded".to_owned()),
            (2, "skipped".to_owned()),
            (3, "skipped".to_owned())
        ],
        "no queued row may survive a cancel"
    );
    let events = events(&pg, &seeded, &run).await;
    assert_eq!(
        events,
        [
            "run.created",
            "run_step.requested",
            "run.started",
            "run_step.changed",
            "run_step.changed",
            "run_step.requested",
            "run_step.changed",
            "run_step.changed",
            "run.cancelled"
        ],
        "each skipped step is announced on the run topic before the run's own event"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_voids_a_pending_gate_and_the_gate_can_no_longer_be_answered() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-cancel-sop").await;
    sqlx::query(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta)
         VALUES ($1, 'gate', 'human_approval', 'Gate', '{}'::jsonb)
         ON CONFLICT (tenant_id, kind, code) DO NOTHING",
    )
    .bind(seeded.tenant_id)
    .execute(&pg.super_pool)
    .await
    .expect("seed the gate code");
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-cancel-gate",
        &sop,
        json!([
            {"gate_kind": "human_approval",
             "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-cancel-gate").await;

    execute(&pg, &seeded, &run, 1).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "waiting_approval");
    let approval: Uuid = sqlx::query_scalar(
        "SELECT id FROM approval WHERE tenant_id = $1 AND run_id = $2 AND kind = 'process'",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the pending gate");

    let (status, body) = running
        .invoke(
            "runs.cancel",
            TOKEN,
            json!({"id": run, "reason": "test cancel"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    assert_eq!(run_status(&pg, &seeded, &run).await, "cancelled");
    let (gate_status,): (String,) = sqlx::query_as("SELECT status FROM approval WHERE id = $1")
        .bind(approval)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the voided gate");
    assert_eq!(gate_status, "voided");
    assert_eq!(
        step_statuses(&pg, &seeded, &run).await,
        vec![(1, "skipped".to_owned()), (2, "skipped".to_owned())]
    );

    // A voided gate is not pending, so a late decision is the ordinary 409.
    let (status, body) = running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
}

/// A child process with two queued steps, published under `key`.
async fn child_process(running: &support::Running, key: &str, sop: &str) {
    let child = create_process(
        running,
        key,
        sop,
        json!([
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(running, &child).await;
}

/// Cancelling a parent is a cancel of the whole tree: the child's own queued steps are as
/// unreachable as the parent's, and leaving them `queued` under a cancelled run is the zombie
/// subtree the 2026-11-10 audit found (runs.cancel skipped only the named run's steps).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_parent_skips_its_childrens_steps_too() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-cancel-tree").await;
    child_process(&running, "cancel-child", &sop).await;
    let parent = create_process(
        &running,
        "cancel-parent",
        &sop,
        json!([{"kind": "call", "child_process_key": "cancel-child",
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &parent).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start(&running, &card, "cancel-parent").await;

    // The call step starts the child and returns waiting on it; the child sits with two queued
    // steps because nothing drives it in this test.
    execute(&pg, &seeded, &run, 1).await;
    let (child, child_status): (Uuid, String) =
        sqlx::query_as("SELECT id, status FROM run WHERE tenant_id = $1 AND parent_run_id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the child run");
    assert!(
        matches!(child_status.as_str(), "queued" | "running"),
        "{child_status}"
    );

    let (status, body) = running
        .invoke(
            "runs.cancel",
            TOKEN,
            json!({"id": run, "reason": "cancel the tree"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (child_now,): (String,) =
        sqlx::query_as("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(child)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the child row");
    assert_eq!(child_now, "cancelled", "the sweep cancelled the child");
    let child_steps: Vec<(i32, String)> = sqlx::query_as(
        "SELECT position, status FROM run_step WHERE tenant_id = $1 AND run_id = $2 ORDER BY position",
    )
    .bind(seeded.tenant_id)
    .bind(child)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the child's steps");
    assert_eq!(child_steps.len(), 2, "{child_steps:?}");
    assert!(
        child_steps.iter().all(|(_, status)| status == "skipped"),
        "every unreachable child step is skipped, not left queued: {child_steps:?}"
    );
}

/// A process run has one writer: its step executor. `runs.finish` (agent-reachable) used to be
/// able to terminate one underneath it, stranding queued steps and live children.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_run_cannot_be_finished_by_hand() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-finish-guard").await;
    let process = create_process(
        &running,
        "finish-guard",
        &sop,
        json!([
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start(&running, &card, "finish-guard").await;

    let (status, body) = running
        .invoke(
            "runs.finish",
            TOKEN,
            json!({"id": run, "outcome": "succeeded"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "run.process_owned", "{body}");
    let after = run_status(&pg, &seeded, &run).await;
    assert!(
        matches!(after.as_str(), "queued" | "running"),
        "the refusal wrote nothing: {after}"
    );
}
