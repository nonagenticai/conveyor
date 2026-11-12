//! S13 step 8's end-to-end proof: a published workflow's `agent.run` node drives the capped loop
//! against a stubbed inference-router, and every model call and tool call leaves one
//! `action_invocation` on the run's trace id (CONTRACT §17: "one `action_invocation` per tool
//! call"). The caps themselves are proved in `crates/workflow/tests/workflow_agent.rs`.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
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

/// A published one-node workflow whose node carries the explicit `agent.run` step input.
async fn published_agent_workflow(running: &support::Running) -> String {
    let (status, created) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({
                "key": "cap.agent",
                "title": "Capability agent",
                "definition": {
                    "schema": 1,
                    "nodes": [{
                        "id": "think",
                        "kind": "action",
                        "action": "agent.run",
                        "input_template": {
                            "model": "test-model",
                            "messages": [
                                {"role": "system", "content": "you are a test"},
                                {"role": "user", "content": "go"}
                            ],
                            "tools": [{
                                "type": "function",
                                "function": {"name": "workflows.list", "parameters": {"type": "object"}}
                            }],
                            "max_turns": 4
                        }
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
async fn an_agent_node_runs_the_capped_loop_and_traces_every_call() {
    let (pg, seeded, running) = fixture().await;
    let (base, calls) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let workflow_id = published_agent_workflow(&running).await;

    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Agent run test"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = id_of(output(&created), "id");

    let (status, body) = running
        .invoke(
            "workflow_runs.start",
            "dev:alice",
            json!({"workflow_id": workflow_id, "project_id": project_id}),
            None,
            None,
            Some("rid-agent"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = uuid::Uuid::parse_str(&id_of(output(&body), "id")).expect("uuid");

    // Tick 1 executes the agent node; tick 2 sees every node finished and settles the run.
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
    assert_eq!(calls.load(Ordering::SeqCst), 2, "two model calls");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let rows: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT node_id, status, output FROM run_step WHERE run_id = $1
          ORDER BY position, attempt",
    )
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read trace");
    let invocations: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT action, outcome, trace_id FROM action_invocation
          WHERE tenant_id = $1 AND trace_id = $2 ORDER BY created_at, action",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(format!("run:{run_id}"))
    .fetch_all(&mut *tx)
    .await
    .expect("read invocations");
    tx.commit().await.expect("commit");

    assert_eq!(rows.len(), 1, "one executed node, one row: {rows:?}");
    assert_eq!(rows[0].0, "think");
    assert_eq!(rows[0].1, "succeeded");
    let step = &rows[0].2;
    assert_eq!(step["output"], "all done");
    assert_eq!(step["turns"], 2);
    assert_eq!(step["tool_calls"][0]["name"], "workflows.list");
    assert_eq!(step["tool_calls"][0]["is_error"], false);
    assert!(
        step["tool_calls"][0]["invocation_id"].is_string(),
        "the tool call cites its invocation: {step}"
    );

    // Two model calls + one tool call, all on the run's trace id, all successful.
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
            .all(|row| row.2.as_deref() == Some(format!("run:{run_id}").as_str())),
        "every invocation carries the run trace id: {invocations:?}"
    );
}
