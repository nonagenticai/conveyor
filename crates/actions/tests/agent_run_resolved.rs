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
    seed_executor_with(pg, seeded, &["workflows.list"]).await
}

/// The same executor with the tool surface the test names — `&[]` for an agent with NO tools,
/// which is what every seeded process agent has.
async fn seed_executor_with(
    pg: &support::Pg,
    seeded: &support::Seeded,
    allowed_actions: &[&str],
) -> (Uuid, Uuid) {
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
    let allowed: Vec<String> = allowed_actions
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    // SPEC_V2 §7.7: an active executor must name a resolvable prompt, so the fixture carries one.
    // It is a workflow node, not a process step, so `agent@1`'s required process section would
    // refuse; one required `system` section compiles anywhere.
    sqlx::query(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'resolved_exec', 1, 'active', 'Resolved executor prompt')",
    )
    .bind(seeded.tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect("insert prompt");
    let (prompt_id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM prompt WHERE tenant_id = $1 AND key = 'resolved_exec' AND version = 1",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("prompt id");
    sqlx::query(
        "INSERT INTO prompt_section
             (tenant_id, prompt_id, position, source_kind, required, template)
         VALUES ($1, $2, 1, 'system', true, 'You are a resolved test executor.')",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(prompt_id)
    .execute(&mut *tx)
    .await
    .expect("insert prompt section");
    let (agent_id,): (Uuid,) = sqlx::query_as(
        // A `process_agent` must name the process it runs (0035's CHECK); this fixture is a
        // generic executor, so it declares the tool-agent role.
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref, allowed_actions,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'agent', 'tool_agent', 'active', 'resolved-model', 'resolved_exec@1', $3, 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .bind(&allowed)
    .fetch_one(&mut *tx)
    .await
    .expect("insert executor");
    tx.commit().await.expect("commit");
    (capability_id, agent_id)
}

/// A stub that answers text immediately and records each request's `model` and whole body.
async fn stub_router() -> (String, Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<Value>>>) {
    let models: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let state = models.clone();
    let captured = bodies.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            let captured = captured.clone();
            async move {
                if let Some(model) = body.get("model").and_then(Value::as_str) {
                    state.lock().expect("models").push(model.to_owned());
                }
                captured.lock().expect("bodies").push(body);
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
    (base, models, bodies)
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
    let (base, models, bodies) = stub_router().await;
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
                "capability_agent_id": agent_id,
                // The model-calling run must declare its spend ceiling (#526 / D45).
                "inputs": {"limits": {"cost_budget_usd": 5.0}}
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
    let bodies = bodies.lock().expect("bodies");
    assert_eq!(bodies.len(), 1, "one model call");
    assert_eq!(
        bodies[0]["tools"][0]["function"]["name"], "workflows.list",
        "a tool-bearing executor sends its tool surface unchanged: {}",
        bodies[0]
    );
}

/// An agent whose `allowed_actions` is empty — every seeded process agent — must send NO `tools`
/// field. An empty array is not a harmless spelling of "none": vLLM answers 400
/// `tools must not be an empty array` before it dials the model, which made every agent step fail
/// live on 2026-11-15.
#[tokio::test]
async fn an_executor_with_no_tools_omits_the_field_from_the_model_request() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, _models, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability, agent) = seed_executor_with(&pg, &seeded, &[]).await;
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");

    let bodies = bodies.lock().expect("bodies");
    assert_eq!(bodies.len(), 1, "one model call");
    assert!(
        bodies[0].get("tools").is_none(),
        "an agent with no tool surface must OMIT `tools`; an empty array is rejected upstream: {}",
        bodies[0]
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
    // The model-calling run must declare its spend ceiling (#526 / D45).
    let mut input = json!({
        "workflow_id": workflow_id,
        "project_id": project_id,
        "inputs": {"limits": {"cost_budget_usd": 5.0}}
    });
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
    let (base, models, _bodies) = stub_router().await;
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

// ── #556: the run's bound tools on the agent surface ──────────────────────────────────────────

use std::sync::atomic::{AtomicUsize, Ordering};

use conveyor_store::tool;

/// A destination that counts every request it answers.
struct ToolStub {
    base: String,
    hits: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct ToolStubState {
    hits: Arc<AtomicUsize>,
}

async fn tool_stub_answer(
    axum::extract::State(state): axum::extract::State<ToolStubState>,
    uri: axum::http::Uri,
) -> axum::Json<Value> {
    state.hits.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({"stub": true, "path": uri.path()}))
}

impl ToolStub {
    async fn start() -> ToolStub {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .fallback(axum::routing::any(tool_stub_answer))
            .with_state(ToolStubState { hits: hits.clone() });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the destination stub");
        let addr = listener.local_addr().expect("stub addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        ToolStub {
            base: format!("http://{addr}"),
            hits,
        }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// One published toolkit, one published tool and a run-scoped binding.
#[allow(clippy::too_many_arguments)]
async fn seed_bound_tool(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    run_id: &str,
    toolkit_key: &str,
    tool_key: &str,
    handler_action: &str,
    effect: &str,
    ceiling: &str,
    input_schema: Value,
) {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let toolkit_id: Uuid = sqlx::query_scalar(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, owner, status,
                              auth_adapter)
         VALUES ($1, $2, 1, $3, $4, 'platform', 'published', 'none') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_key)
    .bind(format!("{toolkit_key} toolkit"))
    .bind(toolkit_key)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the toolkit");
    let _tool_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, timeout_secs,
                           handler_action, status)
         VALUES ($1, $2, $3, $4, $5, 30, $6, 'published') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .bind(tool_key)
    .bind(&input_schema)
    .bind(effect)
    .bind(handler_action)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the tool");
    sqlx::query(
        "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
         VALUES ($1, $2, $3, $4, true)",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(run_id).expect("run uuid"))
    .bind(toolkit_id)
    .bind(ceiling)
    .execute(&mut *tx)
    .await
    .expect("insert the binding");
    tx.commit().await.expect("commit the tool fixture");
}

fn echo_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"message": {"type": "string"}},
        "required": ["message"],
        "additionalProperties": false
    })
}

/// A router that asks for `fixtures_echo` once, then answers text.
async fn stub_router_with_a_tool_call() -> (String, Arc<Mutex<Vec<Value>>>) {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = bodies.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let captured = captured.clone();
            let calls = calls.clone();
            async move {
                captured.lock().expect("bodies").push(body);
                let first = calls.fetch_add(1, Ordering::SeqCst) == 0;
                let message = if first {
                    json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-1",
                            "type": "function",
                            "function": {
                                "name": "fixtures_echo",
                                "arguments": "{\"message\":\"hello\"}"
                            }
                        }]
                    })
                } else {
                    json!({"role": "assistant", "content": "done"})
                };
                axum::Json(json!({
                    "id": "chatcmpl-tool",
                    "choices": [{
                        "index": 0,
                        "message": message,
                        "finish_reason": if first { "tool_calls" } else { "stop" }
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
    (base, bodies)
}

#[tokio::test]
async fn bound_tools_reach_the_model() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, bodies) = stub_router_with_a_tool_call().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let tool_stub = ToolStub::start().await;
    support::set_env(support::STUB_ENV, &tool_stub.base);

    let (_capability, agent) = seed_executor_with(&pg, &seeded, &["tools.call"]).await;
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;
    seed_bound_tool(
        &pg,
        seeded.tenant,
        &run,
        "fixtures",
        "echo",
        "fixtures.echo",
        "read",
        "read",
        echo_schema(),
    )
    .await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");

    {
        let bodies = bodies.lock().expect("bodies");
        assert_eq!(bodies.len(), 2, "one tool turn, then the answer");
        let tools = bodies[0]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("the bound surface is sent: {}", bodies[0]));
        assert_eq!(tools.len(), 1, "only the bound tool: {tools:?}");
        assert_eq!(tools[0]["function"]["name"], "fixtures_echo");
        assert_eq!(
            tools[0]["function"]["parameters"]["required"],
            json!(["message"]),
            "the tool's pinned schema, not the broker's generic one"
        );
    }

    assert_eq!(tool_stub.hits(), 1, "the destination was called once");
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let rows = tool::calls_for_run(
        &mut tx,
        seeded.tenant,
        Uuid::parse_str(&run).expect("run uuid"),
    )
    .await
    .expect("read the ledger");
    tx.commit().await.expect("commit");
    assert_eq!(rows.len(), 1, "one ledgered tool call");
    assert_eq!(rows[0].toolkit_key, "fixtures");
    assert_eq!(rows[0].tool_key, "fixtures.echo");
    assert_eq!(rows[0].status, "ok");
}

#[tokio::test]
async fn no_grant_means_no_tool() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, _models, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability, agent) = seed_executor_with(&pg, &seeded, &["tools.call"]).await;
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
    let bodies = bodies.lock().expect("bodies");
    assert!(
        bodies[0].get("tools").is_none(),
        "tools.call without a binding projects nothing: {}",
        bodies[0]
    );
}

#[tokio::test]
async fn the_agent_cannot_exceed_its_ceiling() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, _models, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability, agent) = seed_executor_with(&pg, &seeded, &["tools.call"]).await;
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;
    // A write tool under a read ceiling: the broker would refuse it, and the projection must
    // never offer it in the first place.
    seed_bound_tool(
        &pg,
        seeded.tenant,
        &run,
        "fixtures",
        "echo",
        "fixtures.echo",
        "write",
        "read",
        echo_schema(),
    )
    .await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
    let bodies = bodies.lock().expect("bodies");
    assert!(
        bodies[0].get("tools").is_none(),
        "a tool above the ceiling is absent from the model's surface: {}",
        bodies[0]
    );
}

#[tokio::test]
async fn the_caller_cannot_supply_broker_tools() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, bodies) = stub_router_with_a_tool_call().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let tool_stub = ToolStub::start().await;
    support::set_env(support::STUB_ENV, &tool_stub.base);
    let (_capability, agent) = seed_executor_with(&pg, &seeded, &["tools.call"]).await;
    // The node claims a broker map of its own; it is not a step-input field, so it cannot widen
    // the surface the binding resolved.
    let workflow = published_agent_workflow_with(
        &running,
        "cap.broker_tools",
        json!({
            "messages": [{"role": "user", "content": "go"}],
            "broker_tools": {"evil": "coding.jobs.create"}
        }),
    )
    .await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;
    seed_bound_tool(
        &pg,
        seeded.tenant,
        &run,
        "fixtures",
        "echo",
        "fixtures.echo",
        "read",
        "read",
        echo_schema(),
    )
    .await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
    let bodies = bodies.lock().expect("bodies");
    let names: Vec<&str> = bodies[0]["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        names,
        vec!["fixtures_echo"],
        "the binding is the only source"
    );
}

#[tokio::test]
async fn a_bound_run_without_tools_call_gets_no_broker_tool() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, _models, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability, agent) = seed_executor_with(&pg, &seeded, &["workflows.list"]).await;
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;
    seed_bound_tool(
        &pg,
        seeded.tenant,
        &run,
        "fixtures",
        "echo",
        "fixtures.echo",
        "read",
        "read",
        echo_schema(),
    )
    .await;

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "succeeded");
    let bodies = bodies.lock().expect("bodies");
    let names: Vec<&str> = bodies[0]["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        names,
        vec!["workflows.list"],
        "a binding without tools.call does not widen the agent's surface"
    );
}

/// P2.4 (S21 step 11, issue #697): an active executor with no prompt cannot buy a call. The
/// prompt is removed AFTER the start so the run is the pre-guard shape the executor must refuse:
/// the start-time readiness door is covered separately by
/// `a_run_with_no_compiled_prompt_is_refused_at_start`, and this test keeps the second door —
/// a run that started with a prompt and then lost it cannot reach the model.
#[tokio::test]
async fn a_run_with_no_compiled_prompt_makes_no_call() {
    let (pg, seeded, running) = fixture().await;
    let _router = ROUTER.lock().await;
    let (base, models, _bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_capability, agent) = seed_executor(&pg, &seeded).await;
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let run = start_workflow_run(&running, &project, &workflow, Some(agent)).await;

    sqlx::query("UPDATE capability_agent SET prompt_ref = NULL WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant.as_uuid())
        .bind(agent)
        .execute(&pg.super_pool)
        .await
        .expect("clear the prompt after the start");

    drive(&pg, &seeded, &run).await;
    assert_eq!(run_status(&pg, &seeded, &run).await, "failed");
    let error = step_error(&pg, &seeded, &run).await.unwrap_or_default();
    assert!(error.contains("capability_not_ready"), "{error}");
    assert!(
        models.lock().expect("models").is_empty(),
        "zero model calls without compiled text"
    );
}

/// The first door: a root run whose bound agent has no prompt is refused at START, before any
/// run row exists. The executor's second door is `a_run_with_no_compiled_prompt_makes_no_call`.
#[tokio::test]
async fn a_run_with_no_compiled_prompt_is_refused_at_start() {
    let (pg, seeded, running) = fixture().await;
    let (_capability, agent) = seed_executor(&pg, &seeded).await;
    sqlx::query("UPDATE capability_agent SET prompt_ref = NULL WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant.as_uuid())
        .bind(agent)
        .execute(&pg.super_pool)
        .await
        .expect("clear the prompt");
    let workflow = published_agent_workflow(&running).await;
    let project = project(&running).await;
    let input = json!({
        "workflow_id": workflow,
        "project_id": project,
        "capability_agent_id": agent,
        "inputs": {"limits": {"cost_budget_usd": 5.0}}
    });
    let (status, body) = running
        .invoke("workflow_runs.start", "dev:alice", input, None, None, None)
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body.pointer("/errors/0/code").and_then(Value::as_str),
        Some("prompt_not_configured"),
        "the refusal names the missing prompt: {body}"
    );
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seeded.tenant.as_uuid())
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the runs");
    assert_eq!(runs, 0, "a refused start writes no run row");
}
