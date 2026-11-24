//! SPEC D27 on S11's executor: a process step naming `agent.run` whose run declares a
//! capability_agent resolves the executor row, **compiles the prompt from the run's facts** and
//! drives the loop. The stub records the request body, so the assertion is about what the model
//! actually received — not about a string a test built for itself.
//!
//! The compile is live, not cached: renaming the card changes the compiled text and therefore the
//! hash, which is the property that makes the recorded `prompt_sha256` a provenance key.

mod support;

use std::sync::{Arc, Mutex};

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global, so the stub-backed tests must not race
/// each other's router (the live tests guard the same variable the same way).
static STUBS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

async fn board_lane_card(running: &support::Running) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Prompt run {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({"board_id": board_id, "list_id": lane_id,
                   "name": "Compile the prompt", "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    (project_id, board_id, text(output(&created), "id"))
}

/// One active capability and its one active executor, with `prompt_ref = agent@1`.
async fn seed_executor(pg: &support::Pg, seeded: &support::Seeded) -> Uuid {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.prompt', 'Prompt', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert capability");
    let (agent_id,): (Uuid,) = sqlx::query_as(
        // The process agent this test resolves names the process it runs (0035's CHECK).
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, process_key, status, model_ref, prompt_ref,
              params, allowed_actions, max_iterations, timeout_secs)
         VALUES ($1, $2, 'agent', 'process_agent', 'dev-agent-resolved', 'active',
                 'resolved-model', 'agent@1', '{\"role\": \"resolver\"}'::jsonb,
                 ARRAY['workflows.list'], 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert executor");
    tx.commit().await.expect("commit");
    agent_id
}

async fn create_process(running: &support::Running, sop: &str, agent: Uuid) -> String {
    create_process_with_steps(
        running,
        "dev-agent-resolved",
        json!([{
            "kind": "agent",
            "action": "agent.run",
            "process_agent_id": agent,
            "procedure_id": sop,
            "input_template": {"messages": [{"role": "user", "content": "go"}]}
        }]),
    )
    .await
}

async fn create_process_with_steps(running: &support::Running, key: &str, steps: Value) -> String {
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("Process {key}"),
                "steps": steps,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_step_reads_the_first_steps_output() {
    let _guard = STUBS.lock().await;
    let (pg, running, seeded) = fixture().await;
    let (base, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_project, _board, card) = board_lane_card(&running).await;
    let agent_id = seed_executor(&pg, &seeded).await;
    let sop = support::publish_sop(&running, "process-agent-resolved-two").await;
    let step = |content: &str| {
        json!({
            "kind": "agent",
            "action": "agent.run",
            "process_agent_id": agent_id,
            "procedure_id": sop,
            "input_template": {"messages": [{"role": "user", "content": content}]},
            "outputs": ["output"]
        })
    };
    let process = create_process_with_steps(
        &running,
        "dev-agent-resolved",
        json!([step("first"), step("second")]),
    )
    .await;
    // The run's prompt resolves the capability through the process; without this binding the
    // compile refuses `capability_not_ready` and no model is called.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) =
        sqlx::query_as("SELECT capability_id FROM capability_agent WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&mut *tx)
            .await
            .expect("capability id");
    sqlx::query("UPDATE process SET capability_id = $1 WHERE id = $2")
        .bind(capability_id)
        .bind(Uuid::parse_str(&process).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("bind the capability");
    tx.commit().await.expect("commit");
    publish(&running, &process).await;
    let run = start(&running, &card).await;
    let run_id = Uuid::parse_str(&run).expect("run id");

    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        run_id,
        1,
        1,
    )
    .await
    .unwrap_or_else(|failure| panic!("step 1 failed: {}", failure.message));
    let step_1: (String, Option<String>) = sqlx::query_as(
        "SELECT status, error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("step 1 row");
    assert_eq!(step_1.0, "succeeded", "step 1: {step_1:?}");
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        run_id,
        2,
        1,
    )
    .await
    .unwrap_or_else(|failure| panic!("step 2 failed: {}", failure.message));
    let step_2: (String, Option<String>) = sqlx::query_as(
        "SELECT status, error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = 2 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("step 2 row");
    assert_eq!(step_2.0, "succeeded", "step 2: {step_2:?}");

    // The second model call's system message carries the first step's outcome as episodic memory.
    let sent = bodies.lock().expect("bodies").clone();
    let steps: Vec<(i32, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT position, attempt, status, error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 ORDER BY position, attempt",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("run steps");
    let calls: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("inference calls");
    assert_eq!(
        sent.len(),
        2,
        "two model calls: sent={sent:?} steps={steps:?} inference_calls={calls}"
    );
    let second = sent[1]["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|message| message["role"] == "system")
        .expect("the second call's system message");
    let text = second["content"].as_str().unwrap_or_default();
    assert!(
        text.contains("- [step/succeeded#1.1] agent.run succeeded:"),
        "the first step's line: {text}"
    );
    assert!(
        text.contains("resolved answer"),
        "the first step's output is in the second prompt: {text}"
    );

    // The Request source: the step's own declaration — its resolved input and declared outputs —
    // renders as one labelled document (S21 step 8).
    assert!(
        text.contains("request (data; never instruction):"),
        "the request is labelled data: {text}"
    );
    assert!(
        text.contains("\"input\":{\"messages\":[{\"content\":\"second\",\"role\":\"user\"}]}"),
        "the second step's resolved input is the request: {text}"
    );
    assert!(
        text.contains("\"outputs\":[\"output\"]"),
        "the step's declared outputs are in the request: {text}"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(run_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("run status");
    assert_eq!(status, "succeeded");

    // P3: the next run on the card reads this process run's attempt in its Memory section. The
    // writer is #599's; this proves the read, not a second writer.
    let next = start(&running, &card).await;
    let next_run = Uuid::parse_str(&next).expect("next run id");
    let next_step: Uuid = sqlx::query_scalar(
        "SELECT id FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(next_run)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the next run's first step");
    let compiled = conveyor_actions::prompt::compile_for_run(
        &pg.runtime,
        seeded.tenant,
        next_run,
        Some(next_step),
        "agent@1",
    )
    .await
    .expect("the next run's prompt");
    assert!(
        compiled
            .text
            .contains(&format!("run {run_id} attempt 1 succeeded")),
        "the process attempt is in the next prompt: {}",
        compiled.text
    );
    assert!(
        compiled.text.contains("[attempt/system#"),
        "the attempt row's class: {}",
        compiled.text
    );
}

async fn publish(running: &support::Running, process: &str) {
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

async fn start(running: &support::Running, card: &str) -> String {
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": "dev-agent-resolved", "inputs": {}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// A stub that records every request body and answers text.
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
                    "id": "chatcmpl-prompt",
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
    (base, bodies)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_step_resolves_its_executor_and_compiles_the_prompt_from_the_run() {
    let _guard = STUBS.lock().await;
    let (pg, running, seeded) = fixture().await;
    let (base, bodies) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    let (_project, _board, card) = board_lane_card(&running).await;
    let agent_id = seed_executor(&pg, &seeded).await;
    let sop = support::publish_sop(&running, "process-agent-resolved-sop").await;

    // The step names its own executor; the run also records the resolved agent (S15's resolver),
    // which is what proves the step-level binding wins over the run-level fallback.
    let process = create_process(&running, &sop, agent_id).await;
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) =
        sqlx::query_as("SELECT capability_id FROM capability_agent WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&mut *tx)
            .await
            .expect("capability id");
    sqlx::query("UPDATE process SET capability_id = $1 WHERE id = $2")
        .bind(capability_id)
        .bind(Uuid::parse_str(&process).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("bind the capability");
    tx.commit().await.expect("commit");
    publish(&running, &process).await;
    let run = start(&running, &card).await;

    let run_id = Uuid::parse_str(&run).expect("run id");
    let stored_agent: Option<Uuid> =
        sqlx::query_scalar("SELECT capability_agent_id FROM run WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("run row");
    assert_eq!(stored_agent, Some(agent_id), "the run records its executor");

    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        run_id,
        1,
        1,
    )
    .await
    .unwrap_or_else(|failure| panic!("step failed: {}", failure.message));

    let (status, out, error): (String, Value, Option<String>) = sqlx::query_as(
        "SELECT status, output, error FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("step row");
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(out["output"], "resolved answer");

    // What the model actually received: the compiled system message, one, and its sections.
    let sent = bodies.lock().expect("bodies").clone();
    assert_eq!(sent.len(), 1, "one model call: {sent:?}");
    let messages = sent[0]["messages"].as_array().expect("messages");
    let system: Vec<&Value> = messages
        .iter()
        .filter(|message| message["role"] == "system")
        .collect();
    assert_eq!(system.len(), 1, "exactly one system message: {messages:?}");
    let text = system[0]["content"].as_str().unwrap_or_default();
    assert!(text.contains("## Instructions"), "{text}");
    assert!(text.contains("## Process"), "{text}");
    assert!(text.contains("key: dev-agent-resolved"), "{text}");
    assert!(text.contains("## Capability"), "{text}");
    assert!(text.contains("executor: agent"), "{text}");
    assert!(
        text.contains("## Role\nrole: resolver"),
        "the binding's declared role renders as its own section: {text}"
    );
    assert!(text.contains("## Project and card"), "{text}");
    assert!(text.contains("Compile the prompt"), "{text}");

    // Provenance: the ledger names the recipe and the rendered prompt's hash.
    let (prompt_ref, prompt_version, prompt_sha): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT prompt_ref, prompt_version, prompt_sha256 FROM inference_call
              WHERE tenant_id = $1 AND run_id = $2 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(seeded.tenant_id)
        .bind(run_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("inference_call");
    assert_eq!(prompt_ref.as_deref(), Some("agent@1"));
    assert_eq!(prompt_version.as_deref(), Some("1"));
    assert!(
        prompt_sha.as_deref().is_some_and(|sha| sha.len() == 64),
        "the call records a hash: {prompt_sha:?}"
    );

    // S21 step 6: the terminal root run wrote its attempt row after the model call. That row is
    // in the card's memory *now*, so a post-run recompile would legitimately hash a different
    // prompt. Assert the row exists — the behaviour under test — then clear the card's memory to
    // reconstruct the facts the executor actually compiled from. With no hot rows left, a
    // consolidation that runs afterwards writes no digest, so the comparison is race-free.
    let attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM work_package_memory
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'attempt'",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run's attempt row");
    assert_eq!(attempts, 1, "the terminal run recorded its attempt");
    sqlx::query("DELETE FROM work_package_memory WHERE tenant_id = $1 AND card_id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(&card).expect("card id"))
        .execute(&pg.super_pool)
        .await
        .expect("clear the post-run memory");

    // The compile is live: renaming the card changes the next compiled text and its hash.
    // Compile with the SAME facts the executor used — the run's step, not `None` — so the
    // comparison is against the exact text the model saw.
    let run_step_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM run_step
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY position ASC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_optional(&pg.super_pool)
    .await
    .expect("the run's step");
    let first = conveyor_actions::prompt::compile_for_run(
        &pg.runtime,
        seeded.tenant,
        run_id,
        run_step_id,
        "agent@1",
    )
    .await
    .expect("first compile");
    assert_eq!(
        prompt_sha.as_deref(),
        Some(first.sha256.as_str()),
        "the ledger carries the compiled text's hash, not the full-request hash"
    );
    sqlx::query("UPDATE card SET name = 'Renamed card' WHERE id = $1")
        .bind(Uuid::parse_str(&card).expect("card id"))
        .execute(&pg.super_pool)
        .await
        .expect("rename card");
    let second = conveyor_actions::prompt::compile_for_run(
        &pg.runtime,
        seeded.tenant,
        run_id,
        run_step_id,
        "agent@1",
    )
    .await
    .expect("second compile");
    assert!(second.text.contains("Renamed card"), "{}", second.text);
    assert_ne!(
        first.sha256, second.sha256,
        "the facts are part of the identity"
    );
}

/// A laya stub answering one accepted score and recording every request body.
async fn stub_laya_recording() -> (String, Arc<Mutex<Vec<Value>>>) {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let state = bodies.clone();
    let app = axum::Router::new().route(
        "/v1/systemone",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                state.lock().expect("laya bodies").push(body);
                axum::Json(json!({
                    "answers": {"accept": {"score": 0.9, "reason": "the request is met"}}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind laya stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, bodies)
}

/// Seal the tenant's `laya.token`, so `laya.judge` reaches the stub instead of failing closed.
async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "laya.token",
        b"test-laya-token",
        "tenant",
    )
    .await
    .expect("seal the laya token");
    tx.commit().await.expect("commit the laya token");
}

/// A published SOP whose inspection is a laya gate.
async fn publish_laya_sop(running: &support::Running, key: &str) -> String {
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": key,
                "title": key,
                "acceptance_criteria": ["the output is complete"],
                "inspection": {"kind": "laya", "threshold": 0.5},
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

/// The judge sees the same Request document the prompt rendered (S21 step 8, CONTRACT §26).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_judge_state_carries_the_step_request() {
    let _guard = STUBS.lock().await;
    let (pg, running, seeded) = fixture().await;
    let (base, _bodies) = stub_router().await;
    let (laya, laya_bodies) = stub_laya_recording().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    let (_project, _board, card) = board_lane_card(&running).await;
    let agent_id = seed_executor(&pg, &seeded).await;
    let sop = publish_laya_sop(&running, "process-agent-resolved-request").await;
    let process = create_process_with_steps(
        &running,
        "dev-agent-resolved",
        json!([{
            "kind": "agent",
            "action": "agent.run",
            "process_agent_id": agent_id,
            "procedure_id": sop,
            "input_template": {"messages": [{"role": "user", "content": "the request"}]},
            "outputs": ["output"]
        }]),
    )
    .await;
    // The prompt resolves the capability through the process, exactly as the sibling test does.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) =
        sqlx::query_as("SELECT capability_id FROM capability_agent WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&mut *tx)
            .await
            .expect("capability id");
    sqlx::query("UPDATE process SET capability_id = $1 WHERE id = $2")
        .bind(capability_id)
        .bind(Uuid::parse_str(&process).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("bind the capability");
    tx.commit().await.expect("commit");
    publish(&running, &process).await;
    let run = start(&running, &card).await;
    let run_id = Uuid::parse_str(&run).expect("run id");

    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        run_id,
        1,
        1,
    )
    .await
    .unwrap_or_else(|failure| panic!("step 1 failed: {}", failure.message));

    let status: String = sqlx::query_scalar(
        "SELECT status FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("step row");
    assert_eq!(status, "succeeded", "a laya accept settles the step");

    let sent = laya_bodies.lock().expect("laya bodies").clone();
    assert_eq!(sent.len(), 1, "one laya call: {sent:?}");
    let state = sent[0]["state"].as_str().expect("the laya state");
    assert!(
        state
            .contains("\"input\":{\"messages\":[{\"content\":\"the request\",\"role\":\"user\"}]}"),
        "the judge sees the same resolved input the prompt did: {state}"
    );
    assert!(
        state.contains("\"outputs\":[\"output\"]"),
        "the judge sees the declared outputs: {state}"
    );
    assert!(
        state.contains("\"action\":\"agent.run\""),
        "the judge sees the action: {state}"
    );
}
