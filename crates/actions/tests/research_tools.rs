//! T14 (#561) — the research and Lean wave's declared calls (SPEC_V2 §11.2/§11.9; ADR 0006).
//!
//! Every test runs against a fresh Postgres 18 and a counting destination stub that answers every
//! path with `{"stub": true, "path": ...}` — the path is the instrument for "the declaration
//! reached the operation it names". The stub counts every request, which is the instrument for
//! "refused before the backend": a schema or credential refusal that still called upstream would
//! move the counter.
//!
//! The suite is serial for the same reason `tool_platform.rs` and `estate_toolkits.rs` are:
//! `CONVEYOR_EXTERNAL_<SERVICE>_URL` is process-global, and each test points its services at its
//! own stub.
//!
//! The media service is deliberately absent: its `/api/*` routes authenticate on `X-Tenant-Id`,
//! which `HandlerRef::External` cannot send, so its five operations are provenance `DROP` rows,
//! not actions. See `crates/actions/src/integrations/research.rs` for the measurement.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::response::IntoResponse as _;
use serde_json::{Value, json};

/// One test at a time: the destination base URLs are process-global.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const WEBSCRAPER_URL: &str = "CONVEYOR_EXTERNAL_WEBSCRAPER_URL";
const LEAN4_URL: &str = "CONVEYOR_EXTERNAL_LEAN4_URL";

/// A destination stub that counts every request and names the path it was given.
struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
}

async fn answer(
    axum::extract::State(hits): axum::extract::State<Arc<AtomicUsize>>,
    uri: axum::http::Uri,
) -> impl axum::response::IntoResponse {
    hits.fetch_add(1, Ordering::SeqCst);
    (
        axum::http::StatusCode::OK,
        axum::Json(json!({"stub": true, "path": uri.path()})),
    )
        .into_response()
}

impl Stub {
    async fn start() -> Stub {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .fallback(axum::routing::any(answer))
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the destination stub");
        let addr = listener.local_addr().expect("stub addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Stub {
            base: format!("http://{addr}"),
            hits,
        }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

struct Fixture {
    pg: support::Pg,
    seeded: support::Seeded,
    stub: Stub,
    running: support::Running,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let stub = Stub::start().await;
        support::set_env(WEBSCRAPER_URL, &stub.base);
        support::set_env(LEAN4_URL, &stub.base);
        let running = support::Running::start(&pg).await;
        Fixture {
            pg,
            seeded,
            stub,
            running,
        }
    }

    async fn invoke(&self, action: &str, input: Value) -> (u16, Value) {
        self.running
            .invoke(action, "dev:alice", input, None, None, None)
            .await
    }

    async fn seal_lean4_token(&self) {
        let mut tx = conveyor_store::begin_scoped(&self.pg.runtime, self.seeded.tenant)
            .await
            .expect("scoped tx");
        conveyor_store::secret::create_secret_tx(
            &mut tx,
            self.seeded.tenant,
            "lean4.token",
            b"parity-lean4-token",
            "tenant",
        )
        .await
        .expect("seal the lean4 token");
        tx.commit().await.expect("commit the lean4 token");
    }
}

/// P4.1: the scraper's two operations are the ones the app declares, and the answer is the
/// upstream document passed through unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scrape_reaches_the_declared_path() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;

    let (status, body) = fixture
        .invoke(
            "webscraper.scrape_url",
            json!({"url": "https://example.test/page"}),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["path"], "/api/scrape/url", "{body}");

    let (status, body) = fixture
        .invoke(
            "webscraper.scrape_bulk",
            json!({"urls": ["https://example.test/a", "https://example.test/b"]}),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["path"], "/api/scrape/bulk", "{body}");
    assert_eq!(fixture.stub.hits(), 2, "each call reached the service once");
}

/// P4.1: the top-level schema is closed — an unknown field is a 422 before the backend.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_field_is_refused_before_the_backend() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;

    let (status, body) = fixture
        .invoke(
            "webscraper.scrape_url",
            json!({"url": "https://example.test/page", "not_a_field": true}),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        fixture.stub.hits(),
        0,
        "the refusal must not reach the service"
    );
}

/// P4.2: the Lean prover's three operations are the declared paths, and the sealed bearer is what
/// lets the call leave the process.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lean_check_reaches_the_declared_path() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.seal_lean4_token().await;

    for (action, input, path) in [
        (
            "lean4.check",
            json!({"source": "#check 1 + 1"}),
            "/api/check",
        ),
        (
            "lean4.elaborate",
            json!({"term": "1 + 1", "context": "open Nat"}),
            "/api/elaborate",
        ),
        (
            "lean4.prove",
            json!({"theorem": "theorem t : 1 + 1 = 2 := by norm_num"}),
            "/api/prove",
        ),
    ] {
        let (status, body) = fixture.invoke(action, input).await;
        assert_eq!(status, 200, "{action}: {body}");
        assert_eq!(body["output"]["path"], path, "{action}: {body}");
    }
    assert_eq!(fixture.stub.hits(), 3, "each call reached the prover once");
}

/// P4.2's negative: without the sealed bearer the call is a typed 503, never an anonymous request
/// the service answers 401 (the defect #410 records for the audio tools).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lean_call_without_the_sealed_token_is_refused_before_the_backend() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;

    let (status, body) = fixture
        .invoke("lean4.check", json!({"source": "#check 1"}))
        .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(
        fixture.stub.hits(),
        0,
        "a missing credential must not reach the prover"
    );
}
