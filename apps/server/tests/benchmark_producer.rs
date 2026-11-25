//! S18 step 1 — the `conveyor` arm's producer, end to end.
//!
//! Five properties, each a way the producer could lie about what the arm measured:
//!
//! 1. **The happy path is the real loop.** One `produce()` makes exactly one model call through
//!    `agent.run`'s shared seam; the request carries the problem statement and none of the answer
//!    key, has no tools and no system message, and the case records the diff with its SHA-256.
//! 2. **No patch is `no_attempt`, not a failure.** An empty answer is withheld from the graded
//!    denominator.
//! 3. **A typed refusal is `no_attempt`, not a retried job.** A 4xx from the router records the
//!    case and returns `Ok`, so the worker does not re-run it for ever.
//! 4. **A transport fault is retryable.** A 5xx spends the attempt and returns a retryable
//!    `JobFailure` — the one class that may be retried.
//! 5. **A re-delivered job replays.** The second `produce()` buys no second model call and does
//!    not rewrite the case.
//!
//! One binary: `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global and the stub's call log is
//! asserted, so the tests serialise on a mutex.
//!
//! `rbuild -- test -p conveyor-server --test benchmark_producer`.

mod common;

use std::sync::{Arc, Mutex, OnceLock};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{
    Json, Router,
    routing::{get, post},
};
use conveyor_domain::TenantId;
use conveyor_server::benchmark_producer::produce;
use conveyor_store as store;
use conveyor_store::benchmark::{NewRun, SeedDocument, SeedInstance};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The patch the stub answers with, in `git diff` format.
const DIFF: &str = "diff --git a/django/core.py b/django/core.py\n\
--- a/django/core.py\n\
+++ b/django/core.py\n\
@@ -1 +1 @@\n\
-a\n\
+b\n";

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Serialises the tests: the stub and its call log are process-global.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Default)]
struct Stub {
    calls: Mutex<Vec<Value>>,
}

static STUB: OnceLock<Arc<Stub>> = OnceLock::new();

/// Start the router stub once per process and point the engine at it.
fn stub() -> &'static Arc<Stub> {
    STUB.get_or_init(|| {
        let stub = Arc::new(Stub::default());
        let state = stub.clone();
        let (ready, address) = std::sync::mpsc::channel();
        // A thread, not a `tokio::spawn`: `#[tokio::test]` drops its runtime when the test ends,
        // so a server spawned inside the first test is dead for the second.
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the stub's runtime");
            runtime.block_on(async move {
                let app = Router::new()
                    .route("/api/chat/completions", post(answer))
                    .route("/{*path}", get(source))
                    .with_state(state);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind the router stub");
                let address = listener.local_addr().expect("stub address");
                ready
                    .send(format!("http://{address}"))
                    .expect("publish the stub");
                axum::serve(listener, app).await.expect("serve the stub");
            });
        });
        let base = address.recv().expect("the stub's address");
        // SAFETY: set once before the first invocation in this binary; the engine reads it per
        // call and nothing here changes it again.
        unsafe {
            std::env::set_var("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
            std::env::set_var("BENCHMARK_SOURCE_BASE", &base);
        }
        stub
    })
}

/// The router's answer. The markers live in the problem statement — the only thing a test can
/// vary without touching process-global state.
async fn answer(
    State(stub): State<Arc<Stub>>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    stub.calls.lock().expect("calls").push(body.clone());
    let text = body.to_string();
    if text.contains("BOOM") {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"message": "the router fell over"}})),
        );
    }
    if text.contains("REFUSE") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {"message": "the router refused the request"}})),
        );
    }
    let content = if text.contains("EMPTY") { "" } else { DIFF };
    (
        StatusCode::OK,
        Json(json!({
            "id": "chatcmpl-producer",
            "object": "chat.completion",
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
        })),
    )
}

/// The baseline's source fetch, served from the stub.
async fn source(Path(path): Path<String>) -> (StatusCode, String) {
    if path.ends_with("django/core.py") {
        (StatusCode::OK, "SOURCE-MARKER\n".to_owned())
    } else {
        (StatusCode::NOT_FOUND, String::new())
    }
}

fn document(problem: &str) -> SeedDocument {
    SeedDocument {
        schema: 1,
        slug: format!("producer-{}", Uuid::now_v7()),
        dataset: "SWE-bench/SWE-bench_Verified".to_owned(),
        dataset_revision: "rev".to_owned(),
        swebench_version: "5.0.2".to_owned(),
        parser_version: "swebench-5.0.2".to_owned(),
        image_lock_sha256: "lock".to_owned(),
        seed_sha256: format!("seed-{}", Uuid::now_v7()),
        instances: vec![SeedInstance {
            instance_id: "repo__i-0".to_owned(),
            repo: "django/django".to_owned(),
            base_commit: "0".repeat(40),
            image: "registry.example.com:5000/swebench/repo@sha256:abc".to_owned(),
            problem_statement: problem.to_owned(),
            oracle_files: vec!["django/core.py".to_owned()],
            gold_patch: "GOLD-ANSWER-MUST-NOT-REACH-THE-MODEL".to_owned(),
            test_patch: "TEST-ANSWER-MUST-NOT-REACH-THE-MODEL".to_owned(),
            fail_to_pass: vec!["tests/test_core.py::test_fix".to_owned()],
            pass_to_pass: vec!["tests/test_core.py::test_keep".to_owned()],
            eval_script: "#!/bin/bash\necho eval\n".to_owned(),
            parser: "parse_log_django".to_owned(),
        }],
    }
}

struct Fixture {
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    instance_id: String,
}

/// One tenant with one active member (the principal an agent run acts as), one active capability
/// agent (tool-less, and pinned to its own system-only recipe since SPEC_V2 §7.7 requires compiled
/// text for every call), one seeded suite and one `conveyor` run over its single instance.
async fn fixture(db: &common::TestDb, problem: &str) -> Fixture {
    let slug = format!("producer-{}", Uuid::now_v7());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(&slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");

    // An agent invocation acts as a real principal (CONTRACT §17): the loop resolves the tenant's
    // first active admin, so the fixture must have one.
    let actor: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('benchmark actor') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("actor");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant_id)
        .bind(actor)
        .execute(&db.super_pool)
        .await
        .expect("the tenant's actor");

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'bench', 'Bench', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    // 0035 makes a `process_agent` name its process; this arm measures a tool-less agent that
    // answers from the problem statement alone, so it is a `tool_agent`.
    // SPEC_V2 §7.7 (#697): an active executor must name a resolvable prompt; the fixture carries
    // its own so the arm does not depend on the tenant's catalogue.
    let (bench_prompt,): (Uuid,) = sqlx::query_as(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'bench_prompt', 1, 'active', 'Benchmark prompt') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("the benchmark prompt");
    sqlx::query(
        "INSERT INTO prompt_section
             (tenant_id, prompt_id, position, source_kind, required, template)
         VALUES ($1, $2, 1, 'system', true, 'Answer the benchmark case.')",
    )
    .bind(tenant_id)
    .bind(bench_prompt)
    .execute(&mut *tx)
    .await
    .expect("the benchmark prompt section");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'bench-agent', 'tool_agent', 'active', 'test-model',
                 'bench_prompt@1', 8, 300)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(capability)
    .fetch_one(&mut *tx)
    .await
    .expect("capability agent");

    let (suite, _) = store::benchmark::seed_suite_tx(&mut tx, tenant, &document(problem))
        .await
        .expect("suite");
    let instance_ids = vec!["repo__i-0".to_owned()];
    let run = store::benchmark::create_run_tx(
        &mut tx,
        tenant,
        &NewRun {
            suite_id: suite.id,
            arm: "conveyor",
            subject_kind: "capability_agent",
            capability_agent_id: Some(agent),
            process_id: None,
            subject_version: Some(1),
            label: "producer-proof",
            options: &json!({}),
            instance_ids: &instance_ids,
        },
    )
    .await
    .expect("run");
    let case_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND instance_id = $3",
    )
    .bind(tenant_id)
    .bind(run.id)
    .bind("repo__i-0")
    .fetch_one(&mut *tx)
    .await
    .expect("case");
    tx.commit().await.expect("commit");

    Fixture {
        tenant,
        run_id: run.id,
        case_id,
        instance_id: "repo__i-0".to_owned(),
    }
}

fn payload(fixture: &Fixture) -> Value {
    json!({
        "run_id": fixture.run_id.to_string(),
        "instance_id": fixture.instance_id,
    })
}

/// `(produce_status, patch, patch_sha256, attempts)`.
async fn case_row(
    db: &common::TestDb,
    fixture: &Fixture,
) -> (String, Option<String>, Option<String>, i32) {
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    let row = sqlx::query_as(
        "SELECT produce_status, patch, patch_sha256, attempts
           FROM benchmark_case WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.case_id)
    .fetch_one(&mut *tx)
    .await
    .expect("case row");
    tx.commit().await.expect("commit");
    row
}

/// `(prompt_ref, run_id, caller_kind)` of the tenant's most recent model call.
async fn inference_row(
    db: &common::TestDb,
    fixture: &Fixture,
) -> (Option<String>, Option<Uuid>, String) {
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    let row = sqlx::query_as(
        "SELECT prompt_ref, run_id, caller_kind FROM inference_call
          WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("inference call");
    tx.commit().await.expect("commit");
    row
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_producer_drives_the_real_loop_and_records_the_patch() {
    let _serial = SERIAL.lock().await;
    let stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "The ordering endpoint returns the wrong total.").await;
    let before = stub.calls.lock().expect("calls").len();

    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("produce");

    // Cloned, so a failed assertion below cannot poison the shared mutex for the other tests.
    let calls = stub.calls.lock().expect("calls").clone();
    assert_eq!(
        calls.len(),
        before + 1,
        "one case is exactly one model call"
    );
    let request = &calls[before];
    let text = request.to_string();
    assert!(
        text.contains("wrong total"),
        "the problem statement must reach the model: {text}"
    );
    assert!(
        !text.contains("GOLD-ANSWER"),
        "the gold patch must never reach the model: {text}"
    );
    assert!(
        !text.contains("TEST-ANSWER"),
        "the test patch must never reach the model: {text}"
    );
    assert!(
        !text.contains("tests/test_core.py::test_fix"),
        "fail_to_pass must never reach the model: {text}"
    );
    assert_eq!(request["model"], json!("test-model"));
    assert!(
        request.get("tools").is_none(),
        "the arm is deliberately tool-less: the field is OMITTED, never an empty array — \
         OpenAI-compatible backends reject `tools: []` (vLLM answers 400 before the model)"
    );
    let messages = request["messages"].as_array().expect("messages");
    let system: Vec<&Value> = messages
        .iter()
        .filter(|message| message["role"] == "system")
        .collect();
    assert_eq!(
        system.len(),
        1,
        "SPEC_V2 §7.7 (#697): the one compiled prompt is the system message"
    );
    assert!(
        system[0]["content"]
            .as_str()
            .is_some_and(|text| text.contains("Answer the benchmark case.")),
        "the active executor's recipe renders into the call"
    );
    let (calls_hashed, calls_total): (i64, i64) = {
        let row: (i64, i64) = sqlx::query_as(
            "SELECT count(*) FILTER (WHERE prompt_sha256 ~ '^[0-9a-f]{64}$'), count(*)
               FROM inference_call WHERE tenant_id = $1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("the ledger");
        row
    };
    assert_eq!(
        calls_hashed, calls_total,
        "every call carries a 64-hex prompt hash"
    );

    let (status, patch, sha, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "ok");
    assert_eq!(patch.as_deref(), Some(DIFF.trim()));
    let mut hasher = Sha256::new();
    hasher.update(DIFF.trim().as_bytes());
    let expected = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(sha.as_deref(), Some(expected.as_str()));
    assert_eq!(attempts, 1);

    let (prompt_ref, run_id, caller_kind) = inference_row(&db, &fixture).await;
    assert_eq!(
        prompt_ref.as_deref(),
        Some("bench_prompt@1"),
        "SPEC_V2 §7.7 (#697): the ledger names the compiled prompt the loop applied"
    );
    assert_eq!(
        run_id,
        Some(fixture.run_id),
        "the benchmark run is the correlation id on the ledger row"
    );
    assert_eq!(caller_kind, "agent_run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_empty_answer_is_no_attempt_not_a_model_failure() {
    let _serial = SERIAL.lock().await;
    let _stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "EMPTY: the agent has nothing to say.").await;

    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("produce");

    let (status, patch, sha, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "no_attempt", "no patch is withheld, never graded");
    assert!(patch.is_none());
    assert!(sha.is_none());
    assert_eq!(attempts, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_typed_refusal_is_no_attempt_not_a_retried_job() {
    let _serial = SERIAL.lock().await;
    let stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "REFUSE: the router will not answer.").await;
    let before = stub.calls.lock().expect("calls").len();

    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("a typed refusal finishes the job");

    let after = stub.calls.lock().expect("calls").len();
    assert_eq!(after, before + 1, "the refusal still reached the router");
    let (status, patch, _, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "no_attempt");
    assert!(patch.is_none());
    assert_eq!(attempts, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_transport_fault_is_retryable_and_spends_the_attempt() {
    let _serial = SERIAL.lock().await;
    let _stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "BOOM: the router falls over.").await;

    let failure = produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect_err("a 5xx must retry the job");
    assert!(
        !failure.permanent,
        "an outage is retryable, never a dead-letter"
    );

    let (status, patch, _, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "pending", "a transport fault writes no outcome");
    assert!(patch.is_none());
    assert_eq!(attempts, 1, "the attempt was spent before the call");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_redelivered_job_replays_and_buys_no_second_call() {
    let _serial = SERIAL.lock().await;
    let stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "The same case, delivered twice.").await;

    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("first delivery");
    let after_first = stub.calls.lock().expect("calls").len();

    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("re-delivery");

    let after_second = stub.calls.lock().expect("calls").len();
    assert_eq!(
        after_second, after_first,
        "the idempotency key replays the model call"
    );
    let (status, patch, _, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "ok");
    assert_eq!(patch.as_deref(), Some(DIFF.trim()));
    assert_eq!(
        attempts, 1,
        "a re-delivered job does not spend a second attempt"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_edit_to_the_agent_row_cannot_change_a_started_run() {
    let _serial = SERIAL.lock().await;
    let stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "The pinned configuration must win.").await;

    // The run carries the configuration it was started with; the live row is then edited to a
    // different model, which must not reach the loop.
    let snapshot = json!({
        "model_ref": "test-model",
        "prompt_ref": null,
        "allowed_actions": [],
        "max_iterations": 8,
        "timeout_secs": 300,
        "version": 1,
    });
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    sqlx::query(
        "UPDATE benchmark_run
            SET options = jsonb_set(options, '{agent_snapshot}', $3::jsonb)
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .bind(snapshot.to_string())
    .execute(&mut *tx)
    .await
    .expect("pin the run");
    sqlx::query(
        "UPDATE capability_agent
            SET model_ref = 'edited-model', max_iterations = 1
          WHERE tenant_id = $1
            AND id = (SELECT capability_agent_id FROM benchmark_run
                       WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .execute(&mut *tx)
    .await
    .expect("edit the live agent");
    tx.commit().await.expect("commit");
    let stored: Value =
        sqlx::query_scalar("SELECT options FROM benchmark_run WHERE tenant_id = $1 AND id = $2")
            .bind(fixture.tenant.as_uuid())
            .bind(fixture.run_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the stored options");
    assert_eq!(
        stored["agent_snapshot"]["model_ref"],
        json!("test-model"),
        "the fixture pinned the snapshot: {stored}"
    );

    let before = stub.calls.lock().expect("calls").len();
    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("produce");

    let calls = stub.calls.lock().expect("calls").clone();
    assert_eq!(calls.len(), before + 1, "one case is one model call");
    assert_eq!(
        calls[before]["model"],
        json!("test-model"),
        "the pinned model, never the edited live row: {}",
        calls[before]
    );
}

struct ProcessFixture {
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    instance_id: String,
    process_id: Uuid,
    problem: String,
}

/// One tenant with the published `swebench-fix` process (its capability, its process agent and
/// one agent step), the `benchmark` card type, one seeded suite and one `process`-subject
/// benchmark run. `agent_status` is the process agent's status, so a test can make the process
/// unready without touching anything else.
async fn process_fixture(db: &common::TestDb, problem: &str, agent_status: &str) -> ProcessFixture {
    let slug = format!("producer-{}", Uuid::now_v7());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(&slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let actor: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('benchmark actor') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("actor");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant_id)
        .bind(actor)
        .execute(&db.super_pool)
        .await
        .expect("the tenant's actor");

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'development', 'Development', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    // 0035 makes a `process_agent` name its process. SPEC_V2 §7.7 (#697): it must also name a
    // resolvable prompt; the fixture carries its own.
    let (fix_prompt,): (Uuid,) = sqlx::query_as(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'swebench_prompt', 1, 'active', 'SWE-bench prompt') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("the process prompt");
    sqlx::query(
        "INSERT INTO prompt_section
             (tenant_id, prompt_id, position, source_kind, required, template)
         VALUES ($1, $2, 1, 'system', true, 'Fix the benchmark case.')",
    )
    .bind(tenant_id)
    .bind(fix_prompt)
    .execute(&mut *tx)
    .await
    .expect("the process prompt section");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, process_key, status, model_ref, prompt_ref,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'swebench-fix', 'process_agent', 'swebench-fix', $3, 'test-model',
                 'swebench_prompt@1', 12, 300)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(capability)
    .bind(agent_status)
    .fetch_one(&mut *tx)
    .await
    .expect("process agent");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process
             (tenant_id, key, title, status, version, capability_id, outputs)
         VALUES ($1, 'swebench-fix', 'SWE-bench fix', 'published', 1, $2, $3::jsonb)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(capability)
    // P3a's seed: the process declares its answer as `patch`, which `runs.get` composes from
    // the terminal agent step's `output.output`.
    .bind(json!(["patch"]).to_string())
    .fetch_one(&mut *tx)
    .await
    .expect("the published swebench-fix process");
    sqlx::query(
        "INSERT INTO process_step
             (tenant_id, process_id, position, action, input_template, kind, actor,
              process_agent_id, on_result, max_attempts)
         VALUES ($1, $2, 1, 'agent.run', '{\"function\":\"fix\"}'::jsonb, 'agent',
                 'process_agent', $3, 'fail', 1)",
    )
    .bind(tenant_id)
    .bind(process)
    .bind(agent)
    .execute(&mut *tx)
    .await
    .expect("the process's one agent step");
    sqlx::query(
        "INSERT INTO card_type (tenant_id, code, label, definition)
         VALUES ($1, 'benchmark', 'Benchmark', '{}'::jsonb)",
    )
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .expect("the benchmark card type");

    let (suite, _) = store::benchmark::seed_suite_tx(&mut tx, tenant, &document(problem))
        .await
        .expect("suite");
    let instance_ids = vec!["repo__i-0".to_owned()];
    let run = store::benchmark::create_run_tx(
        &mut tx,
        tenant,
        &NewRun {
            suite_id: suite.id,
            arm: "conveyor",
            subject_kind: "process",
            capability_agent_id: None,
            process_id: Some(process),
            subject_version: Some(1),
            label: "process-proof",
            options: &json!({}),
            instance_ids: &instance_ids,
        },
    )
    .await
    .expect("run");
    let case_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND instance_id = $3",
    )
    .bind(tenant_id)
    .bind(run.id)
    .bind("repo__i-0")
    .fetch_one(&mut *tx)
    .await
    .expect("case");
    tx.commit().await.expect("commit");

    ProcessFixture {
        tenant,
        run_id: run.id,
        case_id,
        instance_id: "repo__i-0".to_owned(),
        process_id: process,
        problem: problem.to_owned(),
    }
}

fn process_payload(fixture: &ProcessFixture) -> Value {
    json!({
        "run_id": fixture.run_id.to_string(),
        "instance_id": fixture.instance_id,
    })
}

/// `(produce_status, patch, patch_sha256, attempts)` of a process fixture's case.
async fn process_case_row(
    db: &common::TestDb,
    fixture: &ProcessFixture,
) -> (String, Option<String>, Option<String>, i32) {
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    let row = sqlx::query_as(
        "SELECT produce_status, patch, patch_sha256, attempts
           FROM benchmark_case WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.case_id)
    .fetch_one(&mut *tx)
    .await
    .expect("case row");
    tx.commit().await.expect("commit");
    row
}

/// Drive the started process run terminal, with one terminal agent answer.
///
/// The start tick materialised the process's one step; this is the executor's terminal write
/// (`succeeded` + the action's output document), then the run's own terminal state.
async fn finish_process(
    db: &common::TestDb,
    fixture: &ProcessFixture,
    output: Value,
    status: &str,
) {
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    let root_run_id: Uuid = sqlx::query_scalar(
        "SELECT r.id FROM run r
           JOIN card c ON c.tenant_id = r.tenant_id AND c.id = r.card_id
          WHERE r.tenant_id = $1 AND c.meta->>'instance_id' = $2
            AND r.parent_run_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.instance_id.as_str())
    .fetch_one(&mut *tx)
    .await
    .expect("the started process run");
    sqlx::query(
        "UPDATE run_step
            SET status = 'succeeded', output = $3::jsonb, finished_at = now()
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(root_run_id)
    .bind(output.to_string())
    .execute(&mut *tx)
    .await
    .expect("the terminal step");
    sqlx::query("UPDATE run SET status = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(fixture.tenant.as_uuid())
        .bind(root_run_id)
        .bind(status)
        .execute(&mut *tx)
        .await
        .expect("the terminal run");
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_process_arm_starts_the_pinned_process_on_the_case_card_and_waits() {
    let _serial = SERIAL.lock().await;
    let db = common::TestDb::fresh().await;
    let fixture = process_fixture(
        &db,
        "The ordering endpoint returns the wrong total.",
        "active",
    )
    .await;
    let input = process_payload(&fixture);

    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the start tick");

    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    // The host is the run's own project/board/list, keyed by the run id.
    let project: Uuid =
        sqlx::query_scalar("SELECT id FROM project WHERE tenant_id = $1 AND key = $2")
            .bind(fixture.tenant.as_uuid())
            .bind(format!("benchmark-{}", fixture.run_id))
            .fetch_one(&mut *tx)
            .await
            .expect("the host project");
    let board: Uuid =
        sqlx::query_scalar("SELECT id FROM board WHERE tenant_id = $1 AND project_id = $2")
            .bind(fixture.tenant.as_uuid())
            .bind(project)
            .fetch_one(&mut *tx)
            .await
            .expect("the host board");
    let (card_id, code, name, description, meta): (Uuid, String, String, String, Value) =
        sqlx::query_as(
            "SELECT c.id, t.code, c.name, c.description, c.meta
               FROM card c
               JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
              WHERE c.tenant_id = $1 AND c.board_id = $2
              ORDER BY c.position, c.id",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(board)
        .fetch_one(&mut *tx)
        .await
        .expect("the case card");
    assert_eq!(code, "benchmark", "the process arm's card type is P3a's");
    assert_eq!(name, fixture.instance_id);
    assert_eq!(
        description, fixture.problem,
        "the problem statement is the model's material"
    );
    assert_eq!(meta["instance_id"], json!(fixture.instance_id));
    assert_eq!(meta["base_commit"], json!("0".repeat(40)));
    assert!(
        !meta.to_string().contains("GOLD-ANSWER"),
        "the answer key must never reach the card: {meta}"
    );
    let (status, process_id, process_version): (String, Uuid, i64) = sqlx::query_as(
        "SELECT status, process_id, process_version FROM run
          WHERE tenant_id = $1 AND card_id = $2 AND parent_run_id IS NULL",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *tx)
    .await
    .expect("the process run");
    assert_eq!(
        process_id, fixture.process_id,
        "the run pins the process the benchmark run measured"
    );
    assert_eq!(
        process_version, 1,
        "the pinned version is the published one"
    );
    assert_eq!(status, "running");
    tx.commit().await.expect("commit");

    let (produce_status, _, _, attempts) = process_case_row(&db, &fixture).await;
    assert_eq!(
        produce_status, "pending",
        "a live process is not collected yet; P3d reads the run's typed output on its terminal tick"
    );
    assert_eq!(attempts, 1, "the start tick is the one attempt");

    // A second tick is a WAIT: the live process starts nothing and spends nothing.
    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the wait tick");
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    let cards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM card WHERE tenant_id = $1 AND board_id = $2")
            .bind(fixture.tenant.as_uuid())
            .bind(board)
            .fetch_one(&mut *tx)
            .await
            .expect("cards");
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2")
            .bind(fixture.tenant.as_uuid())
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .expect("runs");
    tx.commit().await.expect("commit");
    assert_eq!(cards, 1, "one case is one card");
    assert_eq!(runs, 1, "one case is one process run");
    let (_, _, _, attempts) = process_case_row(&db, &fixture).await;
    assert_eq!(attempts, 1, "waiting spends no producer attempt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_not_ready_process_is_no_attempt_not_a_retried_job() {
    let _serial = SERIAL.lock().await;
    let db = common::TestDb::fresh().await;
    let fixture = process_fixture(&db, "The process cannot start.", "retired").await;

    produce(
        &db.runtime,
        fixture.tenant,
        "conveyor",
        &process_payload(&fixture),
    )
    .await
    .expect("a typed refusal finishes the job");

    let (produce_status, _, _, attempts) = process_case_row(&db, &fixture).await;
    assert_eq!(
        produce_status, "no_attempt",
        "an unready binding is withheld, never graded"
    );
    assert_eq!(
        attempts, 1,
        "the start attempt was spent before the refusal"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_process_arm_collects_the_typed_patch_once() {
    let _serial = SERIAL.lock().await;
    let db = common::TestDb::fresh().await;
    let fixture =
        process_fixture(&db, "The terminal process's answer is the case.", "active").await;
    let input = process_payload(&fixture);

    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the start tick");
    // The process ran to a terminal success with one agent answer. P3b composes the declared
    // `patch` from that step's `output.output`; P3d must read exactly that.
    finish_process(&db, &fixture, json!({ "output": DIFF.trim() }), "succeeded").await;

    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the collect tick");

    let (status, patch, sha, attempts) = process_case_row(&db, &fixture).await;
    assert_eq!(status, "ok", "the typed output becomes the case's patch");
    assert_eq!(patch.as_deref(), Some(DIFF.trim()));
    let expected = sha256_hex(DIFF.trim());
    assert_eq!(sha.as_deref(), Some(expected.as_str()));
    assert_eq!(attempts, 1, "collecting the typed output spends no attempt");

    // A re-delivered collector writes nothing, spends nothing and starts no second process.
    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the re-delivered collect tick");
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    let runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run r
           JOIN card c ON c.tenant_id = r.tenant_id AND c.id = r.card_id
          WHERE r.tenant_id = $1 AND c.meta->>'instance_id' = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.instance_id.as_str())
    .fetch_one(&mut *tx)
    .await
    .expect("runs");
    tx.commit().await.expect("commit");
    assert_eq!(runs, 1, "one case is one process run, never a second");
    let (status, _, _, attempts) = process_case_row(&db, &fixture).await;
    assert_eq!(status, "ok");
    assert_eq!(attempts, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_terminal_process_without_a_patch_is_no_attempt_not_invented() {
    let _serial = SERIAL.lock().await;
    let db = common::TestDb::fresh().await;
    let fixture = process_fixture(&db, "The terminal process stores no answer.", "active").await;
    let input = process_payload(&fixture);

    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the start tick");
    // The process declares `patch`, but its terminal step stored no answer, so P3b composes
    // `{"patch": null}` — the no_attempt signal. Nothing here may become an invented patch.
    finish_process(&db, &fixture, json!({ "turns": 3 }), "succeeded").await;

    produce(&db.runtime, fixture.tenant, "conveyor", &input)
        .await
        .expect("the collect tick");

    let (status, patch, sha, attempts) = process_case_row(&db, &fixture).await;
    assert_eq!(status, "no_attempt", "a withheld case is never graded");
    assert!(patch.is_none(), "no patch is invented from a null output");
    assert!(sha.is_none());
    assert_eq!(attempts, 1, "the start tick is still the only attempt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_baseline_arm_fetches_the_source_and_records_a_patch() {
    let _serial = SERIAL.lock().await;
    let stub = stub();
    let db = common::TestDb::fresh().await;
    let fixture = fixture(&db, "The baseline sees the source.").await;

    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scoped");
    sqlx::query(
        "UPDATE benchmark_run
            SET arm = 'baseline', subject_kind = 'suite', capability_agent_id = NULL,
                subject_version = NULL, options = $3::jsonb
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .bind(json!({"model_ref": "test-model"}).to_string())
    .execute(&mut *tx)
    .await
    .expect("make the run a baseline");
    tx.commit().await.expect("commit");

    let before = stub.calls.lock().expect("calls").len();
    produce(&db.runtime, fixture.tenant, "conveyor", &payload(&fixture))
        .await
        .expect("produce");

    let calls = stub.calls.lock().expect("calls").clone();
    assert_eq!(calls.len(), before + 1, "one case is one model call");
    let text = calls[before].to_string();
    assert_eq!(calls[before]["model"], json!("test-model"));
    assert!(
        text.contains("SOURCE-MARKER"),
        "the source reached the model: {text}"
    );
    assert!(
        text.contains("## Source files at the base commit"),
        "the baseline's edit contract reached the model: {text}"
    );

    let (status, patch, _sha, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "ok");
    assert_eq!(patch.as_deref(), Some(DIFF.trim()));
    assert_eq!(attempts, 1);
}
