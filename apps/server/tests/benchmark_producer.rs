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

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Router, routing::post};
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

/// One tenant, one active capability agent (a tool-less, prompt-less agent — a benchmark run has
/// no run/procedure facts to render a catalogue prompt from), one seeded suite and one `conveyor`
/// run over its single instance.
async fn fixture(db: &common::TestDb, problem: &str) -> Fixture {
    let slug = format!("producer-{}", Uuid::now_v7());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(&slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");

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
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'bench-agent', 'tool_agent', 'active', 'test-model', NULL, 8, 300)
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
    assert_eq!(
        request["tools"],
        json!([]),
        "the arm is deliberately tool-less"
    );
    let messages = request["messages"].as_array().expect("messages");
    assert!(
        messages.iter().all(|message| message["role"] != "system"),
        "no prompt is rendered for a benchmark run, so none may be sent or claimed"
    );

    let (status, patch, sha, attempts) = case_row(&db, &fixture).await;
    assert_eq!(status, "ok");
    assert_eq!(patch.as_deref(), Some(DIFF.trim()));
    let mut hasher = Sha256::new();
    hasher.update(DIFF.trim().as_bytes());
    let expected = format!("{:x}", hasher.finalize());
    assert_eq!(sha.as_deref(), Some(expected.as_str()));
    assert_eq!(attempts, 1);

    let (prompt_ref, run_id, caller_kind) = inference_row(&db, &fixture).await;
    assert_eq!(
        prompt_ref, None,
        "the ledger must not claim a prompt the loop never applied"
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
