//! S11 §6 step 8 — a process step that names `agent.run` is driven by the step executor.
//!
//! `agent.run`'s handler refuses by construction: an internal action holds the invocation
//! transaction and no pool, so it cannot invoke the model or a tool. CONTRACT §17 therefore puts
//! resolution and the loop in the **step executor**, and S11's `process.step` is one exactly as
//! S13's `workflow.node` is. This proof is the wiring: before the shared seam, the process
//! executor invoked the handler and the step failed with the handler's typed 409.
//!
//! One test per binary: `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global, and a second test
//! would inherit the empty value the first one sets (the copilot suite documents the same trap).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const PROCESS_KEY: &str = "dev-agent-step";

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
            json!({"title": format!("Agent step {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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

/// One active capability and its one active executor, with `prompt_ref = agent@1`.
///
/// An `agent` step must name a live `process_agent` (S13 §5.2), so the fixture owns the row the
/// step binds to rather than relying on the run-level resolution the unbound path used.
async fn seed_executor(pg: &support::Pg, seeded: &support::Seeded) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.agent.step', 'Agent step', 'active') RETURNING id",
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
                 'test-model', 'agent@1', '{}'::jsonb, ARRAY['workflows.list'], 4, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .bind(PROCESS_KEY)
    .fetch_one(&mut *tx)
    .await
    .expect("insert executor");
    tx.commit().await.expect("commit");
    (agent_id, capability_id)
}

/// Bind the process to the executor's capability, the way a seeded process is bound, then publish.
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

/// A one-step process whose step is an `agent` step bound to `agent_id` (created, not yet
/// published — the capability is bound in between, as the seed binds it).
async fn created_process(running: &support::Running, sop: &str, agent_id: &str) -> String {
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": PROCESS_KEY,
                "title": "Process dev-agent-step",
                "steps": [{
                    "kind": "agent",
                    "action": "agent.run",
                    "process_agent_id": agent_id,
                    "procedure_id": sop,
                    "input_template": {
                        "model": "test-model",
                        // The bound agent supplies the system prompt (`agent@1`); the step input
                        // carries only the turn it adds.
                        "messages": [
                            {"role": "user", "content": "go"}
                        ],
                        "max_turns": 4
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
            json!({"card_id": card, "process_key": "dev-agent-step", "inputs": {}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// A stub router: the first call asks for `workflows.list`, the second answers with text.
async fn stub_router() -> (String, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let state = calls.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(_body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                let n = state.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    axum::Json(json!({
                        "id": "chatcmpl-1",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": null,
                                "tool_calls": [{
                                    "id": "call-1",
                                    "type": "function",
                                    "function": {"name": "workflows.list", "arguments": "{}"}
                                }]
                            },
                            "finish_reason": "tool_calls"
                        }],
                        "usage": {"total_tokens": 5}
                    }))
                } else {
                    axum::Json(json!({
                        "id": "chatcmpl-2",
                        "choices": [{
                            "index": 0,
                            "message": {"role": "assistant", "content": "all done"},
                            "finish_reason": "stop"
                        }],
                        "usage": {"total_tokens": 7}
                    }))
                }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_step_naming_agent_run_drives_the_loop_and_a_replay_is_a_noop() {
    let (pg, running, seeded) = fixture().await;
    let (base, calls) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = support::publish_sop(&running, "process-agent-sop").await;
    let (agent, capability) = seed_executor(&pg, &seeded).await;
    let process = created_process(&running, &sop, &agent.to_string()).await;
    bind_capability(&pg, &seeded, &process, capability).await;
    publish_process(&running, &process).await;
    let run = start(&running, &card).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, action, out, error): (String, String, Value, Option<String>) = sqlx::query_as(
        "SELECT status, action, output, error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("step row");
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(action, "agent.run");
    assert_eq!(out["output"], "all done");
    assert_eq!(out["turns"], 2);
    assert_eq!(out["tool_calls"][0]["name"], "workflows.list");
    assert_eq!(out["tool_calls"][0]["is_error"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "two model calls");

    // Two model calls + one tool call, all on the run's trace id.
    let invocations: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT action, outcome, trace_id FROM action_invocation
          WHERE tenant_id = $1 AND trace_id = $2 ORDER BY created_at, action",
    )
    .bind(seeded.tenant_id)
    .bind(format!("run:{run}"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("invocations");
    let mut actions: Vec<&str> = invocations.iter().map(|row| row.0.as_str()).collect();
    actions.sort_unstable();
    assert_eq!(
        actions,
        vec!["inference.chat", "inference.chat", "workflows.list"]
    );
    assert!(
        invocations.iter().all(|row| row.1 == "ok"),
        "{invocations:?}"
    );
    assert!(
        invocations
            .iter()
            .all(|row| row.2.as_deref() == Some(format!("run:{run}").as_str())),
        "every invocation carries the run trace id: {invocations:?}"
    );

    // The run settles when the last step records.
    let run_status: String =
        sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("run status");
    assert_eq!(run_status, "succeeded");

    // A replayed terminal step is a no-op: no second model call, no second invocation.
    execute(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "the replay buys nothing");
    let after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation WHERE tenant_id = $1 AND trace_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("run:{run}"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("invocation count");
    assert_eq!(after, 3, "the replay writes nothing");
}
