//! CONTRACT §17's budget is a wall clock: a model call still in flight when the cap passes must
//! fail typed (`agent.timeout`) **at the cap**, never when the model finally answers.
//!
//! Measured live 2026-11-17: a run reported `agent.timeout: the agent ran 347753 ms; its cap is
//! 300000 ms`, because the loop's deadline was only read between turns. This proof pins the
//! executor's half: a stub router that sleeps far past a one-second cap makes the step fail at
//! the cap, and the step row carries the typed error.
//!
//! One test per binary: `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const PROCESS_KEY: &str = "dev-agent-budget";
/// The stub's answer is deliberately slower than the agent's cap.
const MODEL_SLEEP: Duration = Duration::from_secs(5);
/// The cap the executor must enforce, in seconds (the column's floor).
const AGENT_TIMEOUT_SECS: i32 = 1;

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
            json!({"title": format!("Agent budget {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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

/// One active capability and its one active executor, capped at [`AGENT_TIMEOUT_SECS`].
async fn seed_executor(pg: &support::Pg, seeded: &support::Seeded) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.agent.budget', 'Agent budget', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert capability");
    let (agent_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, process_key, status, model_ref, prompt_ref,
              params, allowed_actions, max_iterations, timeout_secs)
         VALUES ($1, $2, 'agent', 'process_agent', $3, 'active',
                 'test-model', 'agent@1', '{}'::jsonb, ARRAY['workflows.list'], 2, $4)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .bind(PROCESS_KEY)
    .bind(AGENT_TIMEOUT_SECS)
    .fetch_one(&mut *tx)
    .await
    .expect("insert executor");
    tx.commit().await.expect("commit");
    (agent_id, capability_id)
}

async fn bind_capability(
    pg: &support::Pg,
    seeded: &support::Seeded,
    process: &str,
    capability: Uuid,
) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query("UPDATE process SET capability_id = $1 WHERE id = $2")
        .bind(capability)
        .bind(Uuid::parse_str(process).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("bind capability");
    tx.commit().await.expect("commit");
}

async fn created_process(running: &support::Running, sop: &str, agent_id: &str) -> String {
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": PROCESS_KEY,
                "title": "Process dev-agent-budget",
                "steps": [{
                    "kind": "agent",
                    "action": "agent.run",
                    "process_agent_id": agent_id,
                    "procedure_id": sop,
                    "input_template": {
                        "model": "test-model",
                        "messages": [
                            {"role": "user", "content": "go"}
                        ],
                        "max_turns": 2
                    }
                }]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn publish_process(running: &support::Running, process: &str) {
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
}

async fn start(running: &support::Running, card: &str) -> String {
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": PROCESS_KEY, "inputs": {}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// A router that answers only after [`MODEL_SLEEP`] — far past the agent's one-second cap.
async fn slow_router() -> (String, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let state = calls.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(_body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                state.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(MODEL_SLEEP).await;
                axum::Json(json!({
                    "id": "chatcmpl-slow",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "finally"},
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
    (base, calls)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_model_call_that_outlives_the_cap_fails_typed_at_the_cap() {
    let (pg, running, seeded) = fixture().await;
    let (base, calls) = slow_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);

    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = support::publish_sop(&running, "agent-budget-sop").await;
    let (agent, capability) = seed_executor(&pg, &seeded).await;
    let process = created_process(&running, &sop, &agent.to_string()).await;
    bind_capability(&pg, &seeded, &process, capability).await;
    publish_process(&running, &process).await;
    let run = start(&running, &card).await;

    // The harness may also run a worker, so the step can be executed by either driver. Driving it
    // here is the fallback, and the persisted `run_step` row — not this call's return value — is
    // what the contract is asserted against.
    let started = Instant::now();
    let _ = conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        Uuid::parse_str(&run).expect("run id"),
        1,
        1,
    )
    .await;

    let mut terminal: Option<(String, Option<String>)> = None;
    for _ in 0..40 {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT status, error FROM run_step
              WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
        )
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(&run).expect("run id"))
        .fetch_optional(&pg.super_pool)
        .await
        .expect("step row");
        if let Some((status, _)) = &row
            && status != "running"
            && status != "queued"
        {
            terminal = row;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let elapsed = started.elapsed();
    let (status, error) = terminal.expect("the step reached a terminal state");
    let error = error.expect("a failed step carries its error");

    assert_eq!(status, "failed", "{error}");
    assert!(
        error.contains("agent.timeout"),
        "the failure must be the typed timeout, got: {error}"
    );
    assert!(
        elapsed < MODEL_SLEEP,
        "the cap must fire before the slow model answers: {elapsed:?}"
    );
    // One second of budget, a little slack for setup and the poll interval; the point is that the
    // step is bounded by the cap, not by the five-second answer.
    assert!(
        elapsed < Duration::from_secs(4),
        "the cap must not overshoot by the model's latency: {elapsed:?}"
    );
    assert!(
        calls.load(Ordering::SeqCst) >= 1,
        "the model was asked before the cap fired"
    );
}
