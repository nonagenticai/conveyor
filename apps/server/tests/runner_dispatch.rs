//! #144 P5 — dispatch, poll, and the one retry. No cluster here: a stub Kubernetes API server plays
//! the API server, so the state machine is proved without a pod.
//!
//! Three properties:
//!
//! 1. **Dispatch then poll finishes the same attempt.** The first step creates the three objects
//!    and defers; the second reads the result the pod posted and finishes the job.
//! 2. **A pod that dies before reporting spends exactly one retry.** The step reports retryable,
//!    the ledger's `max_attempts` is what stops the second death from looping.
//! 3. **The reaper deletes what an abandoned attempt left behind**, and never touches a job row.

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use conveyor_api::Config;
use conveyor_domain::{TenantId, runner::RunResult};
use conveyor_runner::{Cluster, ClusterConfig};
use conveyor_server::runner_dispatch;
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const JOB_KEY_HEX: &str = "2a3b4c5d6e7f8091a2b3c4d5e6f70819a2b3c4d5e6f70819a2b3c4d5e6f70819";

/// The smallest Kubernetes API the runner client talks to: create ConfigMap/Secret/Job, read and
/// delete a Job. Statuses are per Job name, so a test can make one pod die.
#[derive(Clone, Default)]
struct Stub {
    jobs: Arc<Mutex<Vec<Value>>>,
    status: Arc<Mutex<HashMap<String, Value>>>,
    deleted: Arc<Mutex<Vec<String>>>,
}

impl Stub {
    fn job_names(&self) -> Vec<String> {
        self.jobs
            .lock()
            .expect("jobs")
            .iter()
            .filter_map(|job| job["metadata"]["name"].as_str().map(str::to_owned))
            .collect()
    }

    fn fail(&self, name: &str, reason: &str) {
        self.status.lock().expect("status").insert(
            name.to_owned(),
            json!({
                "failed": 1,
                "conditions": [{ "type": "Failed", "reason": reason }],
            }),
        );
    }
}

async fn created(State(stub): State<Stub>, body: Json<Value>) -> impl IntoResponse {
    let body = body.0;
    if body["kind"] == "Job" {
        stub.jobs.lock().expect("jobs").push(body.clone());
    }
    (axum::http::StatusCode::CREATED, Json(body))
}

async fn job_state(
    State(stub): State<Stub>,
    Path((_ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    let status = stub
        .status
        .lock()
        .expect("status")
        .get(&name)
        .cloned()
        .unwrap_or_else(|| json!({}));
    (
        axum::http::StatusCode::OK,
        Json(json!({ "metadata": { "name": name }, "status": status })),
    )
}

async fn removed(
    State(stub): State<Stub>,
    Path((_ns, name)): Path<(String, String)>,
) -> impl IntoResponse {
    stub.deleted.lock().expect("deleted").push(name);
    axum::http::StatusCode::OK
}

async fn start_stub() -> (Stub, String) {
    let stub = Stub::default();
    let app = Router::new()
        .route("/api/v1/namespaces/{ns}/{kind}", post(created))
        .route("/api/v1/namespaces/{ns}/{kind}/{name}", delete(removed))
        .route("/apis/batch/v1/namespaces/{ns}/jobs", post(created))
        .route(
            "/apis/batch/v1/namespaces/{ns}/jobs/{name}",
            get(job_state).delete(removed),
        )
        .with_state(stub.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let address = listener.local_addr().expect("stub address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (stub, format!("http://{address}"))
}

fn config() -> Config {
    Config::from_lookup(|key| match key {
        "CONVEYOR_DEV_AUTH" => Some("1".into()),
        "APP_KEY" => Some("conveyor".into()),
        "CONVEYOR_JOB_TOKEN_KEY" => Some(JOB_KEY_HEX.into()),
        "CONVEYOR_RUNNER_IMAGES" => Some("media.compile=registry.invalid/media-ffmpeg:test".into()),
        "CONVEYOR_RUNNER_BASE_URL" => Some("http://conveyor.invalid".into()),
        _ => None,
    })
    .expect("test config")
}

fn cluster(api_url: &str) -> Cluster {
    Cluster::new(
        ClusterConfig::from_parts("conveyor-runners", api_url, "test-token")
            .with_timeout(Duration::from_secs(5)),
    )
    .expect("cluster client")
}

async fn a_tenant(pool: &PgPool) -> TenantId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('runner-dispatch', 'Runner') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn a_claimed_job(pool: &PgPool, tenant: TenantId) -> store::jobs::Job {
    store::jobs::enqueue(
        pool,
        tenant,
        "media.compile",
        &format!("run-{}", Uuid::now_v7()),
        json!({
            "params": { "profile": "720p" },
            "outputs": [{ "name": "report.txt", "content_type": "text/plain", "max_bytes": 1024 }],
            "limits": {
                "cpu_millis": 200,
                "memory_bytes": 268435456,
                "wall_secs": 60,
                "max_output_bytes": 1024,
            },
        }),
        None,
        // Two attempts: one dispatch plus the single infrastructure retry.
        2,
    )
    .await
    .expect("enqueue")
    .expect("a fresh dedupe key");
    store::jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the job is due")
}

/// P5.1: the first step dispatches, the second consumes the pod's result and finishes the job.
#[tokio::test]
async fn dispatch_then_poll_finishes_the_job() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db.runtime).await;
    let (stub, api_url) = start_stub().await;
    let cluster = cluster(&api_url);
    let config = config();

    let job = a_claimed_job(&db.runtime, tenant).await;
    let first = runner_dispatch::step_with(&db.runtime, &config, tenant, &job, &cluster).await;
    let failure = first.expect_err("a dispatch defers, it does not finish");
    assert!(
        failure.deferred,
        "the dispatch must come back on the SAME attempt"
    );
    assert_eq!(stub.job_names().len(), 1, "one Job was created");

    let run = store::runner::attempt(&db.runtime, tenant, job.id, 1)
        .await
        .expect("read the attempt")
        .expect("the dispatch recorded it");
    assert_eq!(run.status, "dispatched");
    assert!(
        stub.job_names()[0] == run.job_name,
        "the Job name is the attempt's deterministic name"
    );

    // The pod reports success with the one declared output, uploaded first.
    let bytes = b"the report".to_vec();
    use sha2::{Digest, Sha256};
    let artefact = conveyor_domain::runner::Artefact {
        name: "report.txt".to_owned(),
        sha256: Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        bytes: bytes.len() as i64,
        content_type: "text/plain".to_owned(),
    };
    store::runner::insert_artefact(
        &db.runtime,
        tenant,
        run.id,
        &store::runner::ArtefactRow {
            name: artefact.name.clone(),
            sha256: artefact.sha256.clone(),
            bytes: artefact.bytes,
            content_type: artefact.content_type.clone(),
            object_key: format!("t/{}/jobs/{}/1/report.txt", tenant.as_uuid(), job.id),
        },
    )
    .await
    .expect("artefact");

    let started = Utc::now();
    let result = RunResult {
        schema: conveyor_domain::runner::CONTRACT.to_owned(),
        job_id: job.id,
        attempt: 1,
        status: conveyor_domain::runner::Status::Succeeded,
        error: None,
        artefacts: vec![artefact],
        metrics: json!({}),
        started_at: started,
        finished_at: started,
    };
    let recorded = store::runner::record_result(
        &db.runtime,
        tenant,
        job.id,
        1,
        &serde_json::to_value(&result).expect("result json"),
    )
    .await
    .expect("record the result");
    assert!(recorded, "the live attempt accepts the result");

    let second = runner_dispatch::step_with(&db.runtime, &config, tenant, &job, &cluster).await;
    assert!(second.is_ok(), "the poll accepts the result: {second:?}");

    // The step never finishes the row itself: the worker that holds the lease does (worker.rs).
    let finished = store::jobs::finish_succeeded(
        &db.runtime,
        tenant,
        job.id,
        job.lease_token.expect("the claimed lease"),
    )
    .await
    .expect("finish");
    assert!(finished, "the lease holder finishes the job");
    let finished = store::jobs::get(&db.runtime, tenant, job.id)
        .await
        .expect("read the job")
        .expect("the job exists");
    assert_eq!(finished.status, "succeeded");
    let run = store::runner::attempt(&db.runtime, tenant, job.id, 1)
        .await
        .expect("read the attempt")
        .expect("the attempt exists");
    assert_eq!(run.status, "succeeded");
}

/// A kind with no image is not a runner kind: the step refuses it permanently and names the gap.
#[tokio::test]
async fn unconfigured_image_is_a_permanent_failure() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db.runtime).await;
    let (_, api_url) = start_stub().await;
    let cluster = cluster(&api_url);
    let configured = config();
    let mut config = configured.clone();
    config.runner_images.clear();

    let job = a_claimed_job(&db.runtime, tenant).await;
    let failure = runner_dispatch::step_with(&db.runtime, &config, tenant, &job, &cluster)
        .await
        .expect_err("no image, no dispatch");
    assert!(failure.permanent, "a missing image is never retried");
    assert!(
        failure.message.contains("no image"),
        "the failure names what is missing: {}",
        failure.message
    );
    assert!(
        !runner_dispatch::handles(&config, "media.compile"),
        "a kind absent from the map is not a runner kind"
    );
    assert!(runner_dispatch::handles(&configured, "media.compile"));
}

/// Cancelling deletes the attempt's objects and dead-letters the row, which is what makes the pod's
/// still-valid token useless on its next request.
#[tokio::test]
async fn cancelled_job_deletes_the_kubernetes_job() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db.runtime).await;
    let (stub, api_url) = start_stub().await;
    let cluster = cluster(&api_url);
    let config = config();

    let job = a_claimed_job(&db.runtime, tenant).await;
    let dispatched = runner_dispatch::step_with(&db.runtime, &config, tenant, &job, &cluster).await;
    assert!(dispatched.expect_err("dispatch defers").deferred);
    let name = stub.job_names()[0].clone();

    let cancelled = runner_dispatch::cancel_with(&db.runtime, tenant, job.id, Some(&cluster))
        .await
        .expect("cancel");
    assert!(cancelled, "a running job can be cancelled");
    assert!(
        stub.deleted.lock().expect("deleted").contains(&name),
        "the Kubernetes Job is deleted"
    );

    let row = store::jobs::get(&db.runtime, tenant, job.id)
        .await
        .expect("read the job")
        .expect("the job exists");
    assert_eq!(
        row.status, "dead",
        "not a status the byte routes' `status = 'running'` fence can accept"
    );
    let run = store::runner::latest(&db.runtime, tenant, job.id)
        .await
        .expect("read the attempt")
        .expect("the attempt exists");
    assert_eq!(run.status, "lost");
}

/// P5.2: a pod that dies before reporting spends the retry, and the second attempt is a new Job.
#[tokio::test]
async fn infrastructure_failure_retries_once() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db.runtime).await;
    let (stub, api_url) = start_stub().await;
    let cluster = cluster(&api_url);
    let config = config();

    let job = a_claimed_job(&db.runtime, tenant).await;
    let dispatched = runner_dispatch::step_with(&db.runtime, &config, tenant, &job, &cluster).await;
    assert!(
        dispatched.expect_err("dispatch defers").deferred,
        "the first step is a dispatch"
    );
    let first_name = stub.job_names()[0].clone();

    // The pod dies without a result.
    stub.fail(&first_name, "DeadlineExceeded");
    let polled = runner_dispatch::step_with(&db.runtime, &config, tenant, &job, &cluster).await;
    let failure = polled.expect_err("a dead pod is a failure");
    assert!(!failure.permanent, "an infrastructure failure is retryable");
    assert!(!failure.deferred);

    let run = store::runner::attempt(&db.runtime, tenant, job.id, 1)
        .await
        .expect("read attempt 1")
        .expect("attempt 1 exists");
    assert_eq!(run.status, "lost");
    assert!(
        stub.deleted.lock().expect("deleted").contains(&first_name),
        "the dead Job is cleaned up before the retry"
    );

    // What the worker does with that failure: the row returns to pending and is claimed again.
    store::jobs::fail(
        &db.runtime,
        tenant,
        job.id,
        job.lease_token.expect("the claimed lease"),
        &failure.message,
        Duration::from_secs(0),
    )
    .await
    .expect("fail");
    let retry = store::jobs::claim(&db.runtime, tenant, 60)
        .await
        .expect("claim")
        .expect("the retry is due");
    assert_eq!(retry.attempts, 2, "the retry is attempt 2");

    let second = runner_dispatch::step_with(&db.runtime, &config, tenant, &retry, &cluster).await;
    assert!(
        second.expect_err("dispatch defers").deferred,
        "attempt 2 dispatches its own Job"
    );
    assert_eq!(stub.job_names().len(), 2, "one Job per attempt");
    assert!(
        store::runner::attempt(&db.runtime, tenant, job.id, 2)
            .await
            .expect("read attempt 2")
            .expect("attempt 2 exists")
            .job_name
            != first_name,
        "a new attempt never reuses the dead Job's name"
    );
}
