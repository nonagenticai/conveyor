//! S13 — each `agent` step runs under its **own** bound process agent.
//!
//! The run-level resolver (`process_agent_for_process_tx`) answers `None` when a process names
//! more than one distinct agent, because "the capability's one active executor" is not a fact
//! about such a process. Before 2026-11-10 the executor passed that `None` (falling back to the
//! run-level column) for every step, so a tenant process with two differently bound agents was
//! unexecutable — the second agent never ran. Dispatch now reads `process_step.process_agent_id`
//! first.
//!
//! The proof is the model each call actually named: the step's own agent carries `model_ref`, and
//! the request body is recorded by the stub, so an assertion about "the right agent ran" is an
//! assertion about what the model received.
//!
//! One test per binary: `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global.

mod support;

use std::sync::{Arc, Mutex};

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const PROCESS_KEY: &str = "dev-two-agents";

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

async fn board_card_and_lane(running: &support::Running) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Two agents {}", Uuid::now_v7().simple()),
                   "project_type_code": "delivery"}),
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
    let lane_id = text(&lane["list"], "id");
    let (status, card) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({"board_id": board_id, "list_id": lane_id,
                   "name": "Two agents", "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    (text(output(&card), "id"), board_id, lane_id)
}

/// One active capability plus two active process agents, each with its own model.
async fn seed_two_agents(pg: &support::Pg, seeded: &support::Seeded) -> (Uuid, Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.two.agents', 'Two agents', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert capability");
    let mut agents = Vec::new();
    for (key, model) in [("first", "model-first"), ("second", "model-second")] {
        let (agent_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO capability_agent
                 (tenant_id, capability_id, key, role, process_key, status, model_ref,
                  prompt_ref, params, allowed_actions, max_iterations, timeout_secs)
             VALUES ($1, $2, $3, 'process_agent', $4, 'active', $5,
                     'agent@1', '{}'::jsonb, ARRAY[]::text[], 4, 60)
             RETURNING id",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(capability_id)
        .bind(key)
        .bind(PROCESS_KEY)
        .bind(model)
        .fetch_one(&mut *tx)
        .await
        .expect("insert executor");
        agents.push(agent_id);
    }
    tx.commit().await.expect("commit");
    (capability_id, agents[0], agents[1])
}

/// A two-step process, each step bound to a different agent, published and started.
#[allow(clippy::too_many_arguments)]
async fn published_and_started(
    pg: &support::Pg,
    seeded: &support::Seeded,
    running: &support::Running,
    sop: &str,
    capability: Uuid,
    first: Uuid,
    second: Uuid,
    card: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": PROCESS_KEY,
                "title": "Two agents",
                "steps": [
                    {"kind": "agent", "process_agent_id": first, "procedure_id": sop,
                     "input_template": {"messages": [{"role": "user", "content": "first"}]}},
                    {"kind": "agent", "process_agent_id": second, "procedure_id": sop,
                     "input_template": {"messages": [{"role": "user", "content": "second"}]}}
                ]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process = text(output(&body), "id");

    // The process runs under the capability its steps' agents belong to; the prompt compiler reads
    // that capability's knowledge with the agent's `prompt_ref`.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query("UPDATE process SET capability_id = $1 WHERE id = $2")
        .bind(capability)
        .bind(Uuid::parse_str(&process).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("bind capability");
    tx.commit().await.expect("commit");

    let (status, published) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");

    let (status, started) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": PROCESS_KEY, "inputs": {}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{started}");
    text(output(&started), "id")
}

/// A stub that records every request body and answers text (no tool call, one turn).
async fn stub_router() -> (String, Arc<Mutex<Vec<Value>>>) {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let state = bodies.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                state.lock().expect("bodies").push(body);
                axum::Json(json!({
                    "id": "chatcmpl-two-agents",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "done"},
                        "finish_reason": "stop"
                    }],
                    "usage": {"total_tokens": 3}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, bodies)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_agent_step_runs_under_its_own_bound_executor() {
    let (pg, running, seeded) = fixture().await;
    let (base, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (card, _board, _lane) = board_card_and_lane(&running).await;
    let (capability, first, second) = seed_two_agents(&pg, &seeded).await;
    let sop = support::publish_sop(&running, "two-agent-sop").await;
    let run = published_and_started(
        &pg, &seeded, &running, &sop, capability, first, second, &card,
    )
    .await;

    let run_id = Uuid::parse_str(&run).expect("run id");
    for position in [1, 2] {
        conveyor_actions::process::step::execute_step(
            &pg.runtime,
            seeded.tenant,
            "conveyor",
            run_id,
            position,
            1,
        )
        .await
        .unwrap_or_else(|failure| panic!("step {position}: {}", failure.message));
    }

    let statuses: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT position, status, error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 ORDER BY position",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the run's steps");
    assert_eq!(statuses.len(), 2, "{statuses:?}");
    for (position, status, error) in &statuses {
        assert_eq!(status, "succeeded", "step {position}: {error:?}");
    }

    let models: Vec<String> = bodies
        .lock()
        .expect("bodies")
        .iter()
        .map(|body| body["model"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        models,
        vec!["model-first".to_owned(), "model-second".to_owned()],
        "each step used its own bound agent's model: {models:?}"
    );
}
