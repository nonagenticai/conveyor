//! S09 §5.6 — `documents.ingest` against the **real** upstream contract.
//!
//! The upstream is `strata-ingest`. Its contract was read from its own source on 2026-11-08, and three of its
//! facts are what these tests exist for:
//!
//! 1. **The token is the tenant.** `Authorization: Bearer <token>` maps to a tenant upstream, so a conveyor
//!    tenant with no token must be refused *before* any request — otherwise one tenant's document lands in
//!    another tenant's upstream account. [`a_tenant_without_a_token_is_refused_before_any_request`] is the
//!    test that would catch a "helpful" default.
//! 2. **`store_id` must precede the first `file` part.** The upstream refuses a file that arrives first, so the
//!    multipart body's field order is part of the contract, not an implementation detail.
//! 3. **A 202 answers `{"jobs":[{"job_id":…}]}`** — there is no top-level `job_id` on the multipart path.
//!
//! ⚠️ **Nothing here writes to the live service.** The stub is in-process; the live leg is S14's, and pushing
//! ingest jobs into a production service is not something a test may do.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use tokio::task::JoinHandle;

/// The job id every 202 in this file answers with.
const JOB: &str = "0197a0c0-7000-7000-8000-00000000cafe";

/// The env var is process-wide, so tests that set it must not run at the same time.
fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// What the stub answers.
#[derive(Clone, Copy)]
enum Answer {
    Accepted,
    Unauthorized,
    BadRequest,
    AcceptedWithoutJobs,
}

/// What the stub saw.
#[derive(Clone, Default)]
struct Seen {
    bearer: Option<String>,
    content_type: String,
    body: String,
}

#[derive(Clone)]
struct StubState {
    answer: Answer,
    hits: Arc<AtomicUsize>,
    seen: Arc<Mutex<Option<Seen>>>,
}

/// An in-process `strata_ingest` that answers one fixed thing and records the request.
struct IngestStub {
    base: String,
    hits: Arc<AtomicUsize>,
    seen: Arc<Mutex<Option<Seen>>>,
    task: JoinHandle<()>,
}

impl IngestStub {
    async fn start(answer: Answer) -> IngestStub {
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Mutex::new(None));
        let state = StubState {
            answer,
            hits: hits.clone(),
            seen: seen.clone(),
        };
        let app = Router::new()
            .route("/api/ingest/jobs", post(ingest_jobs))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the ingest stub");
        let addr = listener.local_addr().expect("stub address");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        IngestStub {
            base: format!("http://{addr}"),
            hits,
            seen,
            task,
        }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    fn seen(&self) -> Seen {
        self.seen
            .lock()
            .expect("the stub's request lock")
            .clone()
            .unwrap_or_default()
    }
}

impl Drop for IngestStub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn ingest_jobs(State(state): State<StubState>, headers: HeaderMap, body: Bytes) -> Response {
    state.hits.fetch_add(1, Ordering::SeqCst);
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    *state.seen.lock().expect("the stub's request lock") = Some(Seen {
        bearer,
        content_type,
        body: String::from_utf8_lossy(&body).into_owned(),
    });
    match state.answer {
        Answer::Accepted => (
            StatusCode::ACCEPTED,
            axum::Json(json!({"jobs": [{"job_id": JOB, "status": "queued"}]})),
        )
            .into_response(),
        Answer::Unauthorized => (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": {"code": "unauthorized"}})),
        )
            .into_response(),
        Answer::BadRequest => (
            StatusCode::BAD_REQUEST,
            axum::Json(
                json!({"error": {"code": "`store_id` must come before the first `file` part"}}),
            ),
        )
            .into_response(),
        Answer::AcceptedWithoutJobs => {
            (StatusCode::ACCEPTED, axum::Json(json!({"job_id": JOB}))).into_response()
        }
    }
}

/// Create a document through the action and return its id.
async fn create_document(running: &support::Running, token: &str, title: &str) -> String {
    use base64::Engine as _;
    let (status, body) = running
        .invoke(
            "documents.create",
            token,
            json!({
                "title": title,
                "content_type": "text/plain",
                "content_base64": base64::engine::general_purpose::STANDARD.encode(b"hello strata"),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    output(&body)["id"]
        .as_str()
        .expect("a document id")
        .to_owned()
}

/// Create the tenant's upstream token secret.
async fn create_token(running: &support::Running, token: &str, value: &str) {
    let (status, body) = running
        .invoke(
            "secrets.create",
            token,
            json!({"name": "strata_ingest.token", "value": value}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ingest_records_the_job_id_the_upstream_returned() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Accepted).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let document = create_document(&running, "dev:alice", "Ingest me").await;

    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["external_ref"], JOB);
    assert_eq!(stub.hits(), 1, "exactly one upstream call");

    // The contract, not just the call: the tenant's own bearer, multipart, and `store_id` first.
    let seen = stub.seen();
    assert_eq!(seen.bearer.as_deref(), Some("Bearer tok-dev-a"));
    assert!(
        seen.content_type.starts_with("multipart/form-data"),
        "the upstream's only file route is multipart: {}",
        seen.content_type
    );
    let store_at = seen
        .body
        .find("name=\"store_id\"")
        .expect("a store_id part");
    let file_at = seen.body.find("name=\"file\"").expect("a file part");
    assert!(
        store_at < file_at,
        "the upstream refuses a `file` part that arrives before `store_id`"
    );
    assert!(
        seen.body.contains("hello strata"),
        "the document's bytes travel in the body"
    );

    // The job id is a fact on the row and an event on the trail.
    let reference: Option<String> =
        sqlx::query_scalar("SELECT external_ref FROM document WHERE id = $1::uuid")
            .bind(&document)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the document");
    assert_eq!(reference.as_deref(), Some(JOB));

    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE aggregate_id = $1::uuid AND type = 'document.ingested'",
    )
    .bind(&document)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the events");
    assert_eq!(events, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tenant_without_a_token_is_refused_before_any_request() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Accepted).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    // No `strata_ingest.token` secret for this tenant.
    let document = create_document(&running, "dev:alice", "No token").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "ingest_not_configured", "{body}");
    assert_eq!(
        stub.hits(),
        0,
        "an unconfigured tenant must not reach the upstream at all"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_already_ingested_document_is_a_conflict() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Accepted).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let document = create_document(&running, "dev:alice", "Twice").await;
    let input = json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"});
    let (first, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            input.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(first, 200, "{body}");
    let (second, body) = running
        .invoke("documents.ingest", "dev:alice", input, None, None, None)
        .await;
    assert_eq!(second, 409, "{body}");
    assert_eq!(body["type"], "conflict", "{body}");
    assert_eq!(stub.hits(), 1, "a second push would duplicate upstream");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upstream_401_is_502_and_leaves_no_reference() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Unauthorized).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "stale-token").await;
    let document = create_document(&running, "dev:alice", "Rejected token").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 502, "{body}");
    assert_eq!(body["type"], "ingest_upstream_failed", "{body}");

    let reference: Option<String> =
        sqlx::query_scalar("SELECT external_ref FROM document WHERE id = $1::uuid")
            .bind(&document)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the document");
    assert!(reference.is_none(), "a refused push writes nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upstream_4xx_is_422() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::BadRequest).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let document = create_document(&running, "dev:alice", "Bad store").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "ingest_rejected", "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_202_without_jobs_is_a_protocol_failure() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::AcceptedWithoutJobs).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let document = create_document(&running, "dev:alice", "No jobs").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 502, "{body}");
    assert_eq!(body["type"], "ingest_upstream_failed", "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_document_is_404() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Accepted).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({
                "id": "0197a0c0-7000-7000-8000-00000000dead",
                "store_id": "0197a0c0-7000-7000-8000-000000000051"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(stub.hits(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unconfigured_upstream_is_503() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    // Explicitly unset: another test in this binary may have set it.
    support::set_env("CONVEYOR_STRATA_INGEST_URL", "");

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let document = create_document(&running, "dev:alice", "No service").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["type"], "ingest_unavailable", "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_tenant_pushes_under_its_own_token() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Accepted).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-a").await;
    create_token(&running, "dev:bob", "tok-b").await;

    // Alice's document carries Alice's token…
    let document = create_document(&running, "dev:alice", "Alice doc").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:alice",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        stub.seen().bearer.as_deref(),
        Some("Bearer tok-a"),
        "the token is the tenant; Alice's document must not travel under anyone else's"
    );

    // …and Bob's carries Bob's. A shared or default token would make the first assertion pass and this one
    // fail, which is why both are here.
    let document = create_document(&running, "dev:bob", "Bob doc").await;
    let (status, body) = running
        .invoke(
            "documents.ingest",
            "dev:bob",
            json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(stub.seen().bearer.as_deref(), Some("Bearer tok-b"));
}

#[tokio::test(flavor = "multi_thread")]
async fn two_concurrent_ingests_push_exactly_once() {
    let _guard = env_lock().lock().await;
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let stub = IngestStub::start(Answer::Accepted).await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);

    create_token(&running, "dev:alice", "tok-dev-a").await;
    let document = create_document(&running, "dev:alice", "Race me").await;
    let input = json!({"id": document, "store_id": "0197a0c0-7000-7000-8000-000000000051"});

    // Two independent invocations, distinct idempotency keys: the conflict check and the upstream call are
    // separated by seconds, so without the row lock both would pass the check and both would push.
    let (first, second) = tokio::join!(
        running.invoke(
            "documents.ingest",
            "dev:alice",
            input.clone(),
            None,
            Some("race-a"),
            None
        ),
        running.invoke(
            "documents.ingest",
            "dev:alice",
            input,
            None,
            Some("race-b"),
            None
        ),
    );

    let statuses = [first.0, second.0];
    let mut sorted = statuses;
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [200, 409],
        "exactly one wins and the other is a conflict: {first:?} / {second:?}"
    );
    assert_eq!(
        stub.hits(),
        1,
        "the losing call must not reach the upstream at all"
    );
}
