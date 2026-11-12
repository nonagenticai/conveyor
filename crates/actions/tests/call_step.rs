//! S13 — the `call` step kind.
//!
//! A call step starts a child run (process-first, workflow-second) and polls it. These tests
//! drive the real executor against a real database: start the parent, tick the child by hand,
//! then re-enter the parent attempt exactly as the parked `call` job would.
//!
//! The properties asserted:
//!
//! * the child is started ONCE, with the run-tree edge (`parent_run_id`, `step_id`, `depth=1`);
//! * a workflow key starts a workflow child, not a process and not a root;
//! * a live child parks the step under the `call` dedupe tag and the poll finishes it;
//! * `on_result` decides a failed child (continue → skipped, retry → a second child, fail → the
//!   run fails), and a child failure never fires `err.card`;
//! * a retired target is a recorded step failure, not a dead-lettered job;
//! * cancelling the parent cancels the live child.

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
            json!({"title": format!("Call test {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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
                "name": format!("Call card {}", Uuid::now_v7().simple()),
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

async fn publish_sop(running: &support::Running, key: &str, rule: &str) -> String {
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("SOP {key}"),
                "acceptance_criteria": ["the step's output is complete"],
                "inspection": {"kind": "checker", "rule": rule},
                "outputs": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

async fn create_process(running: &support::Running, key: &str, steps: Value) -> String {
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

/// Run one step attempt exactly as the worker's `process.step` arm does.
async fn execute(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) {
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        Uuid::parse_str(run).expect("run id"),
        position,
        attempt,
    )
    .await
    .unwrap_or_else(|failure| panic!("step {position}/{attempt} failed: {}", failure.message));
}

/// `(status, input, output, error, judge)` of one step attempt.
async fn step_row(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> (String, Value, Value, Option<String>, Value) {
    sqlx::query_as(
        "SELECT status, input, output, error, COALESCE(judge, 'null'::jsonb) FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(position)
    .bind(attempt)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run_step row")
}

/// `(id, status, parent_run_id, step_id, depth)` of every child of a run, oldest first.
async fn children(
    pg: &support::Pg,
    seeded: &support::Seeded,
    parent: &str,
) -> Vec<(Uuid, String, Option<Uuid>, Option<Uuid>, i32)> {
    sqlx::query_as(
        "SELECT id, status, parent_run_id, step_id, depth FROM run
          WHERE tenant_id = $1 AND parent_run_id = $2
          ORDER BY created_at, id",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(parent).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the child runs")
}

async fn run_status(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> String {
    sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(run).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row")
}

async fn workflow_id(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> Option<Uuid> {
    sqlx::query_scalar("SELECT workflow_id FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(run).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row")
}

async fn err_card_jobs(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'err.card' AND dedupe_key = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("err.card:{run}"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("err.card count")
}

async fn call_jobs(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1 AND kind = 'process.step' AND dedupe_key LIKE $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("{run}:1:1:call:%"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("call poll count")
}

/// A published child process whose one step succeeds (`cards.get` on the card).
async fn child_process(running: &support::Running, key: &str) -> String {
    let sop = publish_sop(running, &format!("{key}.sop"), "always").await;
    let id = create_process(
        running,
        key,
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(running, &id).await;
    id
}

/// A published child process whose one step fails (a `cards.get` of a UUID that does not exist).
async fn failing_child_process(running: &support::Running, key: &str) -> String {
    let sop = publish_sop(running, &format!("{key}.sop"), "always").await;
    let id = create_process(
        running,
        key,
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "00000000-0000-0000-0000-000000000000"}}]),
    )
    .await;
    publish(running, &id).await;
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_runs_a_child_process_once_and_judges_its_output() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.parent", "always").await;
    child_process(&running, "call-child").await;
    let parent = create_process(
        &running,
        "call-parent",
        json!([{
            "kind": "call",
            "child_process_key": "call-child",
            "procedure_id": sop,
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-parent").await;

    // The first tick starts the child and parks the parent step on it.
    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _input, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "{error:?}");
    assert_eq!(judge["decision"], "waiting_child", "{judge}");
    let child_id = text(&out, "child_run_id");
    assert_eq!(call_jobs(&pg, &seeded, &run).await, 1, "one poll job");

    let kids = children(&pg, &seeded, &run).await;
    assert_eq!(kids.len(), 1, "exactly one child");
    let (kid, kid_status, parent_run, step_id, depth) = &kids[0];
    assert_eq!(*parent_run, Some(Uuid::parse_str(&run).unwrap()));
    assert!(
        step_id.is_some(),
        "the child names the process_step that started it"
    );
    assert_eq!(*depth, 1);
    assert_eq!(kid.to_string(), child_id);

    // The same edge through the API: `runs.get` carries the run tree, not just the row.
    let (status, body) = running
        .invoke("runs.get", TOKEN, json!({"id": run}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let tree = &output(&body)["tree"];
    assert!(tree["parent_run_id"].is_null(), "{tree}");
    assert_eq!(tree["depth"].as_i64(), Some(0));
    let rows = tree["children"].as_array().expect("children");
    assert_eq!(rows.len(), 1, "{tree}");
    assert_eq!(rows[0]["id"], json!(child_id));
    assert_eq!(rows[0]["depth"].as_i64(), Some(1));
    assert_eq!(
        rows[0]["step_id"],
        json!(step_id.map(|id| id.to_string())),
        "the child names the process_step that started it"
    );

    // Tick the child to success, then re-enter the parent attempt as its parked job would.
    execute(&pg, &seeded, &child_id, 1, 1).await;
    assert_eq!(run_status(&pg, &seeded, &child_id).await, "succeeded");
    assert_eq!(*kid_status, "running", "the child starts running");

    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _input, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(out["child_run_id"], json!(child_id));
    assert_eq!(out["child_status"], json!("succeeded"));
    assert_eq!(judge["decision"], json!("accept"), "{judge}");
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
    assert_eq!(
        children(&pg, &seeded, &run).await.len(),
        1,
        "the poll never starts a second child"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_falls_back_to_a_published_workflow() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "callwf.parent", "always").await;

    let (status, body) = running
        .invoke(
            "workflows.create",
            TOKEN,
            json!({
                "key": "call-flow",
                "title": "Call flow",
                "definition": {
                    "schema": 1,
                    "nodes": [{"id": "start", "kind": "action", "action": "cards.get",
                               "input_template": {"id": "{{card.id}}"}}],
                    "edges": [],
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let workflow = text(output(&body), "workflow_id");
    let (status, body) = running
        .invoke(
            "workflows.publish",
            TOKEN,
            json!({"workflow_id": workflow}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let parent = create_process(
        &running,
        "callwf-parent",
        json!([{
            "kind": "call",
            "child_process_key": "call-flow",
            "procedure_id": sop,
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "callwf-parent").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let kids = children(&pg, &seeded, &run).await;
    assert_eq!(kids.len(), 1, "a workflow child was started");
    let (kid, kid_status, parent_run, step_id, depth) = &kids[0];
    assert_eq!(*parent_run, Some(Uuid::parse_str(&run).unwrap()));
    assert!(step_id.is_some());
    assert_eq!(*depth, 1);
    assert_eq!(kid_status, "running");
    assert_eq!(
        workflow_id(&pg, &seeded, &kid.to_string()).await,
        Some(Uuid::parse_str(&workflow).unwrap()),
        "the child is a workflow run, not a process run"
    );

    // The workflow engine's own tests drive its nodes; here the child's completion is the state
    // the parent poll reads.
    sqlx::query(
        "UPDATE run SET status = 'succeeded', finished_at = now(),
                        version = version + 1 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(*kid)
    .execute(&pg.super_pool)
    .await
    .expect("finish the workflow child");

    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _input, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(out["child_status"], json!("succeeded"));
    assert_eq!(judge["decision"], json!("accept"), "{judge}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_child_continues_the_parent_and_never_fires_err_card() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.continue", "always").await;
    failing_child_process(&running, "call-fail-child").await;
    let parent = create_process(
        &running,
        "call-continue",
        json!([
            {"kind": "call", "child_process_key": "call-fail-child", "procedure_id": sop,
             "on_result": "continue", "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "procedure_id": sop, "input_template": {"id": "{{card.id}}"}},
        ]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-continue").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let child_id = text(&step_row(&pg, &seeded, &run, 1, 1).await.2, "child_run_id");
    execute(&pg, &seeded, &child_id, 1, 1).await;
    assert_eq!(run_status(&pg, &seeded, &child_id).await, "failed");

    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _input, _out, error, _judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(
        status, "skipped",
        "on_result=continue records then moves on"
    );
    assert!(error.is_some(), "the child's failure text is kept");
    assert_eq!(
        err_card_jobs(&pg, &seeded, &child_id).await,
        0,
        "a child's failure is its parent step's business, not err.card"
    );

    execute(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_child_fails_the_parent_with_one_err_card() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.fail", "always").await;
    failing_child_process(&running, "call-fail2-child").await;
    let parent = create_process(
        &running,
        "call-fail-parent",
        json!([{
            "kind": "call",
            "child_process_key": "call-fail2-child",
            "procedure_id": sop,
            "on_result": "fail",
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-fail-parent").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let child_id = text(&step_row(&pg, &seeded, &run, 1, 1).await.2, "child_run_id");
    execute(&pg, &seeded, &child_id, 1, 1).await;
    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _input, _out, error, _judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed", "{error:?}");
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    assert_eq!(
        err_card_jobs(&pg, &seeded, &run).await,
        1,
        "the ROOT failure is the one err.card"
    );
    assert_eq!(
        err_card_jobs(&pg, &seeded, &child_id).await,
        0,
        "the child never fires err.card"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retried_call_starts_a_second_child() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.retry", "always").await;
    failing_child_process(&running, "call-retry-child").await;
    let parent = create_process(
        &running,
        "call-retry",
        json!([{
            "kind": "call",
            "child_process_key": "call-retry-child",
            "procedure_id": sop,
            "on_result": "retry",
            "max_attempts": 2,
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-retry").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let first = text(&step_row(&pg, &seeded, &run, 1, 1).await.2, "child_run_id");
    execute(&pg, &seeded, &first, 1, 1).await;
    execute(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(
        step_row(&pg, &seeded, &run, 1, 1).await.0,
        "failed",
        "the first attempt failed"
    );

    // The chain appended attempt 2; it has no stored child, so it starts its own.
    execute(&pg, &seeded, &run, 1, 2).await;
    let second = text(&step_row(&pg, &seeded, &run, 1, 2).await.2, "child_run_id");
    assert_ne!(first, second, "a retried call starts a NEW child");
    assert_eq!(children(&pg, &seeded, &run).await.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retired_call_target_is_a_recorded_step_failure() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.retired", "always").await;
    let child = child_process(&running, "call-retired-child").await;
    let parent = create_process(
        &running,
        "call-retired",
        json!([{
            "kind": "call",
            "child_process_key": "call-retired-child",
            "procedure_id": sop,
            "on_result": "fail",
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-retired").await;

    let (status, body) = running
        .invoke(
            "processes.retire",
            TOKEN,
            json!({"process_id": child}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _input, _out, error, _judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed");
    let error = error.expect("the refusal is recorded");
    assert!(
        error.contains("call_target_unknown"),
        "the step names the missing target: {error}"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    assert!(
        children(&pg, &seeded, &run).await.is_empty(),
        "nothing was started"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_the_parent_cancels_the_live_child() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.cancel", "always").await;
    let _child = child_process(&running, "call-cancel-child").await;
    let parent = create_process(
        &running,
        "call-cancel",
        json!([{
            "kind": "call",
            "child_process_key": "call-cancel-child",
            "procedure_id": sop,
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-cancel").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let kids = children(&pg, &seeded, &run).await;
    let (child_id, ..) = &kids[0];
    assert_eq!(
        run_status(&pg, &seeded, &child_id.to_string()).await,
        "running"
    );

    let (status, body) = running
        .invoke(
            "runs.cancel",
            TOKEN,
            json!({"id": run, "reason": "the caller stopped it"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        run_status(&pg, &seeded, &child_id.to_string()).await,
        "cancelled",
        "the live child is part of the parent's unit of work"
    );
    assert_eq!(
        err_card_jobs(&pg, &seeded, &child_id.to_string()).await,
        0,
        "a cascade is not a failure"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_deliveries_of_one_call_attempt_start_exactly_one_child() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(&running, "call.race", "always").await;
    child_process(&running, "call-race-child").await;
    let parent = create_process(
        &running,
        "call-race",
        json!([{
            "kind": "call",
            "child_process_key": "call-race-child",
            "procedure_id": sop,
            "input_template": {"id": "{{card.id}}"},
        }]),
    )
    .await;
    publish(&running, &parent).await;
    let run = start(&running, &card, "call-race").await;
    let run_id = Uuid::parse_str(&run).expect("run id");

    // Two workers can both claim the same attempt (the claim is running, a replay re-invokes).
    // The second start is refused by the one-live-child index; it must ADOPT the winner's child,
    // not fail the step.
    let (first, second) = tokio::join!(
        conveyor_actions::process::step::execute_step(
            &pg.runtime,
            seeded.tenant,
            "conveyor",
            run_id,
            1,
            1,
        ),
        conveyor_actions::process::step::execute_step(
            &pg.runtime,
            seeded.tenant,
            "conveyor",
            run_id,
            1,
            1,
        ),
    );
    assert!(first.is_ok(), "{first:?}");
    assert!(second.is_ok(), "{second:?}");

    let kids = children(&pg, &seeded, &run).await;
    assert_eq!(kids.len(), 1, "one attempt has exactly one child");
    let (status, _input, out, error, _judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(
        status, "running",
        "the step is parked on its child: {error:?}"
    );
    assert_eq!(out["child_run_id"], json!(kids[0].0.to_string()));
}

// ── the workflow's procedure node (W5) ───────────────────────────────────────────────────────

/// The project a board belongs to.
async fn board_project(pg: &support::Pg, board: &str) -> Uuid {
    sqlx::query_scalar("SELECT project_id FROM board WHERE id = $1")
        .bind(Uuid::parse_str(board).expect("board id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the board's project")
}

/// Drive the workflow executor twice: the first pass records the node's outcome (or starts/
/// polls its child), the second settles the run's terminal status. A single pass leaves a
/// finished run `running` — measured, not assumed.
async fn workflow_tick(pg: &support::Pg, seeded: &support::Seeded, run: &str) {
    for pass in 1..=2 {
        conveyor_actions::workflow::node::execute_node(
            &pg.runtime,
            seeded.tenant,
            "dev",
            Uuid::parse_str(run).expect("run id"),
        )
        .await
        .unwrap_or_else(|failure| panic!("workflow tick {pass}: {}", failure.message));
    }
}

/// A published one-node workflow whose node starts the child process named by `child`.
async fn published_procedure_workflow(
    running: &support::Running,
    key: &str,
    child: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "workflows.create",
            TOKEN,
            json!({
                "key": key,
                "title": "Workflow procedure child",
                "definition": {
                    "schema": 1,
                    "nodes": [{"id": "child", "kind": "procedure", "procedure_key": child}],
                    "edges": [],
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "workflow_id");
    let (status, body) = running
        .invoke(
            "workflows.publish",
            TOKEN,
            json!({"workflow_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

/// Start the workflow on the card and return its run id.
async fn start_workflow(
    running: &support::Running,
    pg: &support::Pg,
    workflow: &str,
    board: &str,
    card: &str,
) -> String {
    let project = board_project(pg, board).await;
    let (status, body) = running
        .invoke(
            "workflow_runs.start",
            TOKEN,
            json!({"workflow_id": workflow, "project_id": project, "card_id": card}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_workflow_procedure_node_starts_a_child_under_the_parent() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    child_process(&running, "wfnode-child").await;
    let workflow = published_procedure_workflow(&running, "wfnode-flow", "wfnode-child").await;
    let run = start_workflow(&running, &pg, &workflow, &board, &card).await;

    workflow_tick(&pg, &seeded, &run).await;

    let kids = children(&pg, &seeded, &run).await;
    assert_eq!(
        kids.len(),
        1,
        "the procedure node started one child, not a second root"
    );
    let (kid, kid_status, parent_run, step_id, depth) = &kids[0];
    assert_eq!(*parent_run, Some(Uuid::parse_str(&run).unwrap()));
    assert_eq!(*depth, 1);
    assert_eq!(*kid_status, "running");
    let parent_step: Uuid = sqlx::query_scalar(
        "SELECT id FROM run_step
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY position, attempt LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&run).unwrap())
    .fetch_one(&pg.super_pool)
    .await
    .expect("the parent's run_step row");
    assert_eq!(
        *step_id, None,
        "a workflow child names no process_step (the edge is a run_step, 0037)"
    );
    let workflow_step: Option<Uuid> =
        sqlx::query_scalar("SELECT workflow_step_id FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(*kid)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the child's workflow step");
    assert_eq!(
        workflow_step,
        Some(parent_step),
        "the child names the parent's run_step row"
    );

    // The child's own step, then the parent's poll.
    execute(&pg, &seeded, &kid.to_string(), 1, 1).await;
    assert_eq!(
        run_status(&pg, &seeded, &kid.to_string()).await,
        "succeeded",
        "the child process finished its one step"
    );
    workflow_tick(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_workflow_run_cancels_its_procedure_child() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    child_process(&running, "wfcancel-child").await;
    let workflow = published_procedure_workflow(&running, "wfcancel-flow", "wfcancel-child").await;
    let run = start_workflow(&running, &pg, &workflow, &board, &card).await;
    workflow_tick(&pg, &seeded, &run).await;
    let kid = children(&pg, &seeded, &run).await[0].0;

    let (status, body) = running
        .invoke(
            "workflow_runs.cancel",
            TOKEN,
            json!({"run_id": run}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(run_status(&pg, &seeded, &run).await, "cancelled");
    assert_eq!(
        run_status(&pg, &seeded, &kid.to_string()).await,
        "cancelled",
        "a cancelled parent takes its live child with it"
    );
}
