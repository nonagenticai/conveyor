//! S13 step 8, the resolved path: a run declares its `capability_agent`, and the step executor
//! resolves `model_ref`, `allowed_actions` → tool schemas and the caps into the step input —
//! `agent.run` never reads the row itself (CONTRACT §17). A separate binary from `agent_run`
//! because `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global.

mod support;

use std::sync::{Arc, Mutex};

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

/// Serialises the tests that bind the process-global inference URL to their own stub.
static ROUTER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

/// One active capability and its one active executor, as S15's actions would create them.
async fn seed_executor(pg: &support::Pg, seeded: &support::Seeded) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.agent', 'Agent', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert capability");
    let (agent_id,): (Uuid,) = sqlx::query_as(
        // A `process_agent` must name the process it runs (0035's CHECK); this fixture is a
        // generic executor, so it declares the tool-agent role.
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, allowed_actions,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'agent', 'tool_agent', 'active', 'resolved-model',
                 ARRAY['workflows.list'], 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert executor");
    tx.commit().await.expect("commit");
    (capability_id, agent_id)
}

/// A stub that answers text immediately and records the `model` each request asked for.
async fn stub_router() -> (String, Arc<Mutex<Vec<String>>>) {
    let models: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let state = models.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                if let Some(model) = body.get("model").and_then(Value::as_str) {
                    state.lock().expect("models").push(model.to_owned());
                }
                axum::Json(json!({
                    "id": "chatcmpl-resolved",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "resolved answer"},
                        "finish_reason": "stop"
                    }],
                    "usage": {"total_tokens": 9}
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
    (base, models)
}

/// A published one-node workflow whose input carries only the user message: the executor resolves
/// the model and the tools from the run's executor row.
async fn published_agent_workflow(running: &support::Running) -> String {
    published_agent_workflow_with(
        running,
        "cap.resolved",
        json!({"messages": [{"role": "user", "content": "go"}]}),
    )
    .await
}

/// The same workflow with the step input the test names (the bound path's narrowing/conflict cases).
async fn published_agent_workflow_with(
    running: &support::Running,
    key: &str,
    input: Value,
) -> String {
    let (status, created) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({
                "key": key,
                "title": "Resolved capability agent",
                "definition": {
                    "schema": 1,
                    "nodes": [{
                        "id": "think",
                        "kind": "action",
                        "action": "agent.run",
                        "input_template": input
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
async fn the_run_declares_its_executor_and_the_step_resolves_the_config() {
    let (pg, seeded, running) = fixture().await;
    // The inference URL is process-global: hold the lock from before binding it to this stub
    // until the recorded models are asserted, or a concurrent test overwrites it.
    let _router = ROUTER.lock().await;
    let (base, models) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability_id, agent_id) = seed_executor(&pg, &seeded).await;
    let workflow_id = published_agent_workflow(&running).await;

    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Resolved agent test"}),
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
            json!({
                "workflow_id": workflow_id,
                "project_id": project_id,
                "capability_agent_id": agent_id
            }),
            None,
            None,
            Some("rid-resolved"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = Uuid::parse_str(&id_of(output(&body), "id")).expect("uuid");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (stored,): (Option<Uuid>,) =
        sqlx::query_as("SELECT capability_agent_id FROM run WHERE id = $1")
            .bind(run_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the run");
    tx.commit().await.expect("commit");
    assert_eq!(stored, Some(agent_id), "the run records its executor");

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
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT action, output FROM run_step WHERE run_id = $1 ORDER BY position, attempt",
    )
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read trace");
    tx.commit().await.expect("commit");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, "agent.run");
    assert_eq!(rows[0].1["output"], "resolved answer");
    assert_eq!(rows[0].1["turns"], 1);
    assert_eq!(
        rows[0].1["capability_agent_id"],
        json!(agent_id),
        "the resolved id travels in the step input and the output"
    );

    assert_eq!(
        models.lock().expect("models").as_slice(),
        ["resolved-model"],
        "the model came from the row, not the node input"
    );
}

/// An ordinary project for a workflow run.
async fn project(running: &support::Running) -> String {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": format!("Resolved conflicts {}", Uuid::now_v7().simple())}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    id_of(output(&created), "id")
}

/// Start one run of the workflow, bound to `agent` when the test names one.
async fn start_workflow_run(
    running: &support::Running,
    project_id: &str,
    workflow_id: &str,
    agent: Option<Uuid>,
) -> String {
    let mut input = json!({"workflow_id": workflow_id, "project_id": project_id});
    if let Some(agent) = agent {
        input["capability_agent_id"] = json!(agent);
    }
    let (status, body) = running
        .invoke("workflow_runs.start", "dev:alice", input, None, None, None)
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// Drive the run to its terminal state.
///
/// Two passes, because the executor writes the node's outcome first and settles the RUN on the
/// next plan: after a failed (or succeeded) node the run is still `running` until the terminal
/// status is recorded (§ the existing resolved-path test drives the same two ticks).
async fn drive(pg: &support::Pg, seeded: &support::Seeded, run: &str) {
    for pass in 1..=2 {
        conveyor_actions::workflow::node::execute_node(
            &pg.runtime,
            seeded.tenant,
            "dev",
            Uuid::parse_str(run).expect("run id"),
        )
        .await
        .unwrap_or_else(|failure| panic!("tick {pass}: {}", failure.message));
    }
}

async fn run_status(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> String {
    sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(run).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row")
}

/// The latest attempt's recorded error, when it failed.
async fn step_error(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY position DESC, attempt DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run_step row")
}

#[tokio::test]
async fn a_bound_run_refuses_a_conflicting_explicit_model() {
    let (pg, seeded, running) = fixture().await;
    let (_capability, agent) = seed_executor(&pg, &seeded).await;
    let workflow = published_agent_workflow_with(
        &running,
        "cap.model_conflict",
        json!({
            "model": "other-model",
            "messages": [{"role": "user", "content": "go"}]
        }),
    )
    .await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    let error = step_error(&pg, &seeded, &run).await.unwrap_or_default();
    assert!(error.contains("agent.config_conflict"), "{error}");
}

#[tokio::test]
async fn a_bound_run_refuses_node_supplied_tools() {
    let (pg, seeded, running) = fixture().await;
    let (_capability, agent) = seed_executor(&pg, &seeded).await;
    let workflow = published_agent_workflow_with(
        &running,
        "cap.tools_conflict",
        json!({
            "messages": [{"role": "user", "content": "go"}],
            "tools": [{"type": "function", "function": {"name": "cards.move", "parameters": {"type": "object"}}}]
        }),
    )
    .await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    let error = step_error(&pg, &seeded, &run).await.unwrap_or_default();
    assert!(error.contains("agent.config_conflict"), "{error}");
}

#[tokio::test]
async fn a_bound_run_refuses_a_cap_above_the_row() {
    let (pg, seeded, running) = fixture().await;
    let (_capability, agent) = seed_executor(&pg, &seeded).await;
    // The fixture's row allows 3 turns; the step asks for 4.
    let workflow = published_agent_workflow_with(
        &running,
        "cap.raise",
        json!({
            "messages": [{"role": "user", "content": "go"}],
            "max_turns": 4
        }),
    )
    .await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    let error = step_error(&pg, &seeded, &run).await.unwrap_or_default();
    assert!(error.contains("agent.cap_exceeds_bound"), "{error}");
}

#[tokio::test]
async fn a_bound_run_may_lower_a_cap() {
    let (pg, seeded, running) = fixture().await;
    // The inference URL is process-global: hold the lock from before binding it to this stub
    // until the recorded models are asserted, or a concurrent test overwrites it.
    let _router = ROUTER.lock().await;
    let (base, models) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability, agent) = seed_executor(&pg, &seeded).await;
    let workflow = published_agent_workflow_with(
        &running,
        "cap.lower",
        json!({
            "messages": [{"role": "user", "content": "go"}],
            "max_turns": 1
        }),
    )
    .await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
    assert_eq!(
        models.lock().expect("models").as_slice(),
        ["resolved-model"],
        "the row's model is used even when the step narrows a cap"
    );
}

#[tokio::test]
async fn an_unbound_run_refuses_a_claimed_executor() {
    let (pg, seeded, running) = fixture().await;
    let workflow = published_agent_workflow_with(
        &running,
        "cap.claim",
        json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "go"}],
            "capability_agent_id": Uuid::now_v7()
        }),
    )
    .await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, None).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    let error = step_error(&pg, &seeded, &run).await.unwrap_or_default();
    assert!(error.contains("agent.config_conflict"), "{error}");
}
