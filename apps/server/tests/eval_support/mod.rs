//! S19's driver fixture: a subject stub, one eval version, one run, and the reads the driver
//! tests assert on.
//!
//! Shared by `eval_runner`, `eval_runner_properties` and `eval_jobs` — three binaries, because
//! `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global and read on every call, so a test that
//! pointed it at a second stub would race every test beside it.
//!
//! ⛔ The stub owns a **thread**, not a `tokio::spawn`. `#[tokio::test]` drops its runtime when
//! the test ends, so a server spawned inside the first test is dead for the second, and the
//! symptom is `action-upstream-failed: inference answered 502` — which reads as a transport bug
//! and is not one.

// A shared fixture module compiled into three test binaries: each reads a different subset, so a
// field or helper used by one is "never read" in another. `-D warnings` on the workspace (CI's
// clippy gate) turns that into a hard failure; the module is deliberately a superset.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Router, routing::post};
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::common::TestDb;

/// The stub's per-process state: how many `FLAKY` cases it has answered.
#[derive(Clone, Default)]
struct Stub {
    flaky: Arc<AtomicUsize>,
}

static BASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Start the subject stub once per process and point the engine at it.
pub fn stub_base() -> &'static str {
    BASE.get_or_init(|| {
        let (ready, addr) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the stub's runtime");
            runtime.block_on(async move {
                let app = Router::new()
                    .route("/api/chat/completions", post(answer))
                    .with_state(Stub::default());
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind the subject stub");
                let addr = listener.local_addr().expect("stub addr");
                ready
                    .send(format!("http://{addr}"))
                    .expect("publish the stub");
                axum::serve(listener, app).await.expect("serve the stub");
            });
        });
        let base = addr.recv().expect("the stub's address");
        // SAFETY: set before the first invocation in this binary; the engine reads it per call
        // and nothing here changes it again.
        unsafe {
            std::env::set_var("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
        }
        base
    })
}

/// The router's document, shaped like the real one.
///
/// The markers live in the case input, which is the only thing a test can vary without touching
/// process-global state: `BOOM` is a 5xx, `WRONG` answers as another model, `FLAKY` fails its
/// first call and succeeds afterwards, and `MISS` answers the wrong label.
async fn answer(State(state): State<Stub>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    let text = body.to_string();
    if text.contains("BOOM") {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"message": "the subject fell over"}})),
        );
    }
    if text.contains("FLAKY") && state.flaky.fetch_add(1, Ordering::SeqCst) == 0 {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"message": "transient"}})),
        );
    }
    let model = if text.contains("WRONG") {
        "other-model"
    } else {
        "eval-model"
    };
    let content = if text.contains("MISS") {
        "refund"
    } else {
        "billing"
    };
    // A judge call asks for a structured verdict (`response_format`), and the audit's three
    // known negatives must be rejected through the real `grade_from_judge` parser. The rubric
    // text is the only per-test seam a judge request carries: a rubric containing `LENIENT`
    // makes the stub a judge that accepts everything — the reversal the audit's control is for.
    if body.get("response_format").is_some() {
        let score = if text.contains("LENIENT") { 0.9 } else { 0.05 };
        let verdict = json!({
            "score": score,
            "explanation": "the stub's verdict on the candidate",
        });
        return (
            StatusCode::OK,
            Json(json!({
                "id": "chatcmpl-eval-judge",
                "object": "chat.completion",
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": verdict.to_string()},
                    "finish_reason": "stop",
                }],
                "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4},
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "id": "chatcmpl-eval",
            "object": "chat.completion",
            "model": model,
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4},
        })),
    )
}

/// One metric on the fixture's version.
pub struct MetricSpec {
    /// The metric key, which is also the key in the grade object.
    pub key: &'static str,
    /// `programmatic | rubric | pairwise | human | none`.
    pub grader_kind: &'static str,
    /// The grader document.
    pub grader: Value,
    /// The rubric text a `rubric`/`pairwise` grader requires.
    pub rubric: Option<&'static str>,
}

/// The one metric most tests use: normalised exact over the subject's answer text.
pub fn exact_metric() -> MetricSpec {
    MetricSpec {
        key: "label_accuracy",
        grader_kind: "programmatic",
        grader: json!({"kind": "exact", "field": "choices.0.message.content"}),
        rubric: None,
    }
}

/// The rows every driver test measures.
pub struct Fixture {
    /// The tenant every row belongs to.
    pub tenant: TenantId,
    /// The eval.
    pub eval_id: Uuid,
    /// Its frozen version.
    pub version_id: Uuid,
    /// The run that names them.
    pub run_id: Uuid,
    /// The cases, in insertion order.
    pub cases: Vec<Uuid>,
}

/// One eval version and one run over `cases` — all through the store.
///
/// The driver never checks readiness (that is `eval_runs.start`'s job), so this fixture stops at
/// the rows: a version, its cases, its metrics and a run that names them.
pub async fn fixture(db: &TestDb, cases: &[&str]) -> Fixture {
    fixture_with(db, cases, &[exact_metric()]).await
}

/// [`fixture`] with a caller-chosen metric set.
pub async fn fixture_with(db: &TestDb, cases: &[&str], metrics: &[MetricSpec]) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('s19-run', 'S19') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    // A non-empty `inputs_schema` is what makes the case text a REQUEST rather than a prompt:
    // `subject_request` parses it as an object and hands it to the subject verbatim.
    let inputs_schema = json!({"type": "object"});

    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let eval = store::eval::insert_eval_tx(
        &mut tx,
        tenant,
        &store::eval::NewEval {
            name: "driver proof",
            purpose: "one pilot end to end",
            subject_kind: "completions",
            entry_action: "completions.create",
            model_ref: Some("eval-model"),
            prompt_ref: None,
            inputs_schema: &inputs_schema,
            trace_source: "synthesized",
            created_by: None,
        },
    )
    .await
    .expect("insert eval");
    let version = store::eval::create_version_tx(&mut tx, tenant, eval.id, None)
        .await
        .expect("insert version");
    let mut case_ids = Vec::new();
    for text in cases {
        // A case that is not JSON at all is how `harness` failures are driven; everything else
        // becomes the request the subject receives.
        let input = if text.starts_with('{') {
            text.to_string()
        } else {
            json!({"messages": [{"role": "user", "content": text}]}).to_string()
        };
        let expected = json!("billing");
        let tags = vec!["triage".to_owned()];
        let attachments = json!([]);
        let meta = json!({});
        let case = store::eval::insert_case_tx(
            &mut tx,
            tenant,
            &store::eval::NewEvalCase {
                version_id: version.id,
                input: input.as_str(),
                tags: &tags,
                expected: Some(&expected),
                attachments: &attachments,
                source: "handwritten",
                gold_provenance: "human_written",
                gold_model: None,
                meta: &meta,
            },
        )
        .await
        .expect("insert case");
        case_ids.push(case.id);
    }
    for (ordinal, metric) in metrics.iter().enumerate() {
        store::eval::insert_metric_tx(
            &mut tx,
            tenant,
            &store::eval::NewEvalMetric {
                version_id: version.id,
                key: metric.key,
                role: "quality",
                kind: "binary",
                better: "higher",
                label: metric.key,
                unit: None,
                grader_kind: metric.grader_kind,
                grader: &metric.grader,
                rubric: metric.rubric,
                ordinal: i32::try_from(ordinal).expect("a small metric list"),
            },
        )
        .await
        .expect("insert metric");
    }
    let environment = json!({"entry_action": "completions.create"});
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: eval.id,
            version_id: version.id,
            variant: "baseline",
            round: 0,
            mode: "pilot",
            reps: 1,
            case_count: cases.len() as i32,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert run");
    tx.commit().await.expect("commit the fixture");

    Fixture {
        tenant,
        eval_id: eval.id,
        version_id: version.id,
        run_id: run.id,
        cases: case_ids,
    }
}

/// One `(case, rep)` row, as the report will read it.
#[derive(Debug)]
pub struct StoredResult {
    /// `ok | truncated | pending_label`.
    pub status: String,
    /// The grade object.
    pub grade: Value,
    /// The model the response named.
    pub model: String,
    /// The attempt's wall clock.
    pub latency_ms: i32,
}

/// Every result row of the run, in `(case, rep)` order.
pub async fn results(db: &TestDb, fixture: &Fixture) -> Vec<StoredResult> {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let rows: Vec<(String, Value, String, i32)> = sqlx::query_as(
        "SELECT status, grade, model, latency_ms FROM eval_case_result
          WHERE tenant_id = $1 AND eval_run_id = $2
          ORDER BY case_id, rep",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the results");
    tx.commit().await.expect("commit");
    rows.into_iter()
        .map(|(status, grade, model, latency_ms)| StoredResult {
            status,
            grade,
            model,
            latency_ms,
        })
        .collect()
}

/// `(attempt, failure_class, retry_count)` per recorded failure, oldest first.
pub async fn attempt_errors(db: &TestDb, fixture: &Fixture) -> Vec<(i32, String, i32)> {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let rows: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT attempt, failure_class, retry_count FROM eval_attempt_error
          WHERE tenant_id = $1 AND eval_run_id = $2
          ORDER BY attempt",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the attempt errors");
    tx.commit().await.expect("commit");
    rows
}

/// How many transcripts the run owns.
pub async fn trace_count(db: &TestDb, fixture: &Fixture) -> i64 {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_trace WHERE tenant_id = $1 AND eval_run_id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count traces");
    tx.commit().await.expect("commit");
    count
}
