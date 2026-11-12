//! S15 step 12 — the pre-run capability guard over real HTTP.
//!
//! Properties, each a way readiness could silently pass:
//!
//! * no active executor ⇒ `409 capability_not_ready` with `no_active_agent`, and **zero** `run`
//!   and `run_step` rows;
//! * an `allowed_actions` entry the registry does not know ⇒ `409` with `unknown_action`;
//! * a ready capability records `run.capability_id` / `run.capability_agent_id` and starts normally.
//!
//! `rbuild -- test -p conveyor-actions --test run_capability -- --nocapture`.

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

async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Cap {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");
    let (_, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = text(&output(&view)["boards"][0], "id");
    let (_, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
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

async fn published_process(running: &support::Running, sop: &str) -> String {
    let steps = json!([
        {"action": "cards.claim", "procedure_id": sop,
         "input_template": {"id": "{{card.id}}", "_version": "{{card.version}}"}},
        {"action": "cards.comment", "procedure_id": sop,
         "input_template": {"id": "{{card.id}}", "body": "done"}}
    ]);
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({"key": format!("cap-{}", Uuid::now_v7().simple()), "title": "Capability", "card_type_code": "feature", "steps": steps}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
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
    id
}

/// A capability row plus one executor, bound to the process. Returns (capability, agent).
async fn bind(
    pg: &support::Pg,
    seeded: &support::Seeded,
    process: &str,
    agent_status: &str,
    allowed_actions: &[&str],
) -> (Uuid, Uuid) {
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, $2, 'Design', 'active') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("design-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed capability");
    let actions: Vec<String> = allowed_actions
        .iter()
        .map(|action| (*action).to_owned())
        .collect();
    let agent: Uuid = sqlx::query_scalar(
        // A generic executor, not a process agent (which must name its process).
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, allowed_actions)
         VALUES ($1, $2, $3, 'tool_agent', $4, 'local-llm', $5) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(capability)
    .bind(format!("agent-{}", Uuid::now_v7().simple()))
    .bind(agent_status)
    .bind(actions)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed agent");
    sqlx::query("UPDATE process SET capability_id = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(process).expect("process id"))
        .bind(capability)
        .execute(&pg.super_pool)
        .await
        .expect("bind the process");
    (capability, agent)
}

async fn counts(pg: &support::Pg, seeded: &support::Seeded) -> (i64, i64) {
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count runs");
    let steps: i64 = sqlx::query_scalar("SELECT count(*) FROM run_step WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count run steps");
    (runs, steps)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_active_agent_refuses_before_any_run_row() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "run-capability-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running, &sop).await;
    bind(&pg, &seeded, &process, "draft", &["cards.claim"]).await;

    let (status, _, body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    let codes: Vec<&str> = body["errors"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["code"].as_str()).collect())
        .unwrap_or_default();
    assert!(codes.contains(&"agent_missing"), "{body}");
    assert_eq!(counts(&pg, &seeded).await, (0, 0), "a refusal wrote rows");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_allowed_action_refuses_before_any_run_row() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "run-capability-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running, &sop).await;
    let (capability, _) = bind(
        &pg,
        &seeded,
        &process,
        "active",
        &["cards.claim", "does.not_exist"],
    )
    .await;

    let (status, _, body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    let codes: Vec<&str> = body["errors"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["code"].as_str()).collect())
        .unwrap_or_default();
    assert!(codes.contains(&"action_not_registered"), "{body}");
    assert_eq!(counts(&pg, &seeded).await, (0, 0), "a refusal wrote rows");

    // S15 step 12: `capabilities.get` returns the SAME projection the 409 carries.
    let (status, get_body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": capability}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{get_body}");
    let get_codes: Vec<&str> = output(&get_body)["blocking"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["code"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(get_codes, codes, "the GET and the 409 must not drift");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ready_capability_records_the_run_binding() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "run-capability-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running, &sop).await;
    let (capability, agent) = bind(&pg, &seeded, &process, "active", &["cards.claim"]).await;

    let (status, _, body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = Uuid::parse_str(&text(output(&body), "id")).expect("run id");

    let (recorded_capability, recorded_agent): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT capability_id, capability_agent_id FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run row");
    assert_eq!(recorded_capability, Some(capability));
    assert_eq!(recorded_agent, Some(agent));
    let (runs, steps) = counts(&pg, &seeded).await;
    assert_eq!((runs, steps), (1, 2));
}
