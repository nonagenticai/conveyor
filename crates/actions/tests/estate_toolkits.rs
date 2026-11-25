//! T14 — the estate import wave, its probe and the zero-use report (SPEC_V2 §11.2, §11.8, §11.9;
//! issue #83).
//!
//! Every test runs against a fresh Postgres 18 and a real HTTP destination. The destination's hit
//! counter is the instrument for "refused before the backend": an outage refusal that still called
//! upstream would move the counter, so the assertion is on the wire, not on the error text alone.
//!
//! The suite is serial for the same reason `tool_platform.rs` is: `CONVEYOR_EXTERNAL_<SERVICE>_URL`
//! is process-global, and each test points it at its own stub.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use conveyor_domain::TenantId;
use conveyor_store::{begin_scoped, tool};
use serde_json::{Value, json};
use uuid::Uuid;

/// One test at a time: the destination base URL is process-global.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The destination the coding toolkit dispatches to.
const CODING_URL: &str = "CONVEYOR_EXTERNAL_CODING_URL";

/// A destination stub whose health answer and latency the test controls.
struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
    state: Arc<StubState>,
}

struct StubState {
    health_status: AtomicUsize,
    delay_ms: AtomicUsize,
    hits: Arc<AtomicUsize>,
}

async fn answer(
    axum::extract::State(state): axum::extract::State<Arc<StubState>>,
    uri: axum::http::Uri,
) -> impl axum::response::IntoResponse {
    state.hits.fetch_add(1, Ordering::SeqCst);
    let delay = state.delay_ms.load(Ordering::SeqCst) as u64;
    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    if uri.path() == "/health" {
        let status = state.health_status.load(Ordering::SeqCst) as u16;
        let code = axum::http::StatusCode::from_u16(status).expect("a valid status");
        return (code, axum::Json(json!({"health": status == 200}))).into_response();
    }
    (
        axum::http::StatusCode::OK,
        axum::Json(json!({"stub": true, "path": uri.path()})),
    )
        .into_response()
}

use axum::response::IntoResponse as _;

impl Stub {
    async fn start() -> Stub {
        let hits = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(StubState {
            health_status: AtomicUsize::new(200),
            delay_ms: AtomicUsize::new(0),
            hits: hits.clone(),
        });
        let app = axum::Router::new()
            .fallback(axum::routing::any(answer))
            .with_state(state.clone());
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
            state,
        }
    }

    fn fail_health(&self) {
        self.state.health_status.store(500, Ordering::SeqCst);
    }

    fn pass_health(&self) {
        self.state.health_status.store(200, Ordering::SeqCst);
    }

    fn delay(&self, ms: usize) {
        self.state.delay_ms.store(ms, Ordering::SeqCst);
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// A seeded tenant, one running run, and the shared action server.
struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    run_id: Uuid,
    stub: Stub,
    running: support::Running,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let stub = Stub::start().await;
        support::set_env(CODING_URL, &stub.base);
        let run_id = seed_run(&pg, seeded.tenant).await;
        let running = support::Running::start(&pg).await;
        Fixture {
            pg,
            tenant: seeded.tenant,
            run_id,
            stub,
            running,
        }
    }

    async fn invoke(&self, action: &str, input: Value) -> (u16, Value) {
        self.running
            .invoke(action, "dev:alice", input, None, None, None)
            .await
    }

    async fn publish(&self) -> Value {
        let (status, body) = self.invoke("toolkits.publish", json!({})).await;
        assert_eq!(status, 200, "publish: {body}");
        body
    }

    async fn call(&self, name: &str, input: Value) -> (u16, Value) {
        self.invoke(
            "tools.call",
            json!({"run_id": self.run_id, "tool": name, "input": input}),
        )
        .await
    }

    /// Bind the run to one published toolkit, as a job binding would.
    async fn bind(&self, toolkit_key: &str, ceiling: &str) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        let toolkit_id: Uuid =
            sqlx::query_scalar("SELECT id FROM toolkit WHERE tenant_id = $1 AND key = $2")
                .bind(self.tenant.as_uuid())
                .bind(toolkit_key)
                .fetch_one(&mut *tx)
                .await
                .expect("the published toolkit");
        sqlx::query(
            "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
             VALUES ($1, $2, $3, $4, true)",
        )
        .bind(self.tenant.as_uuid())
        .bind(self.run_id)
        .bind(toolkit_id)
        .bind(ceiling)
        .execute(&mut *tx)
        .await
        .expect("insert the binding");
        tx.commit().await.expect("commit the binding");
    }

    async fn ledger(&self) -> Vec<tool::ToolCallRow> {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped read");
        let rows = tool::calls_for_run(&mut tx, self.tenant, self.run_id)
            .await
            .expect("read the ledger");
        tx.commit().await.expect("commit the read");
        rows
    }

    async fn toolkit_status(&self, key: &str) -> (String, i64) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped read");
        let row = tool::latest_toolkit(&mut tx, self.tenant, key)
            .await
            .expect("read the toolkit")
            .expect("the toolkit exists");
        tx.commit().await.expect("commit the read");
        (row.status, row.toolkit_version)
    }
}

async fn seed_run(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("T14-{}", Uuid::now_v7().simple()))
    .bind("Estate toolkits proof")
    .fetch_one(&mut *tx)
    .await
    .expect("insert the project");
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the fixture");
    run_id
}

fn outcomes(body: &Value) -> Vec<(String, String, i64)> {
    body["output"]["toolkits"]
        .as_array()
        .expect("the publish answer carries toolkits")
        .iter()
        .map(|row| {
            (
                row["key"].as_str().expect("key").to_owned(),
                row["outcome"].as_str().expect("outcome").to_owned(),
                row["toolkit_version"].as_i64().expect("version"),
            )
        })
        .collect()
}

// ── P1.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_publish_is_idempotent_and_a_change_bumps_the_version() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;

    let first = fixture.publish().await;
    let rows = outcomes(&first);
    // #450 removed the three manifests whose actions are internal now (`artifacts`, `billing`,
    // `worker_management`); a toolkit binds External handlers only. #560 adds the scientific
    // processor's draft; #561 adds the webscraper and Lean prover drafts.
    assert_eq!(rows.len(), 8, "one row per embedded manifest: {first}");
    for (key, outcome, version) in &rows {
        assert_eq!(outcome, "created", "{key} on the first publish");
        assert_eq!(*version, 1, "{key} starts at version 1");
    }

    let second = fixture.publish().await;
    for (key, outcome, version) in outcomes(&second) {
        assert_eq!(outcome, "unchanged", "{key} on the second publish");
        assert_eq!(version, 1, "{key} keeps its version");
    }

    // A changed manifest content — simulated by rewriting the stored hash, which is exactly what
    // a different manifest would produce — inserts the next version and leaves the old one alone.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query(
        "UPDATE toolkit SET meta = meta || '{\"content_hash\": \"stale\"}'::jsonb
          WHERE tenant_id = $1 AND key = 'coding'",
    )
    .bind(fixture.tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect("rewrite the hash");
    tx.commit().await.expect("commit the rewrite");

    let third = fixture.publish().await;
    let coding = outcomes(&third)
        .into_iter()
        .find(|(key, _, _)| key == "coding")
        .expect("the coding row");
    assert_eq!(coding.1, "version_bumped");
    assert_eq!(coding.2, 2);
    let (status, version) = fixture.toolkit_status("coding").await;
    assert_eq!((status.as_str(), version), ("published", 2));
}

// ── P1.3 / P2.1 ─────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_scientific_toolkit_publishes_as_a_draft() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    // The service's address is configured: the draft is the manifest's declared status, not an
    // unconfigured-host accident (#560 P5.2).
    support::set_env("CONVEYOR_EXTERNAL_SCIENTIFIC_URL", &fixture.stub.base);

    let first = fixture.publish().await;
    let scientific = outcomes(&first)
        .into_iter()
        .find(|(key, _, _)| key == "scientific")
        .expect("the scientific row");
    assert_eq!(
        scientific,
        ("scientific".to_owned(), "created".to_owned(), 1),
        "{first}"
    );
    let (status, version) = fixture.toolkit_status("scientific").await;
    assert_eq!((status.as_str(), version), ("draft", 1));

    // A second publish of the same manifest is unchanged — one immutable version.
    let second = fixture.publish().await;
    let scientific = outcomes(&second)
        .into_iter()
        .find(|(key, _, _)| key == "scientific")
        .expect("the scientific row");
    assert_eq!(scientific.1, "unchanged");
    assert_eq!(scientific.2, 1);

    // A draft resolves no binding: the broker answers `409 tool_not_ready` before the destination.
    fixture.bind("scientific", "read").await;
    let hits = fixture.stub.hits();
    let (code, body) = fixture
        .call("scientific_stats.meta_analysis", json!({"studies": []}))
        .await;
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["type"], "tool_not_ready", "{body}");
    assert_eq!(fixture.stub.hits(), hits, "no network call was made");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unconfigured_toolkit_is_draft_and_uncallable() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.publish().await;

    let (status, version) = fixture.toolkit_status("retrieval").await;
    assert_eq!((status.as_str(), version), ("draft", 1));

    // A binding to a draft toolkit is a row the broker will not resolve: `resolve` requires a
    // published toolkit, which is what makes a draft unbound and uncallable rather than merely
    // unadvertised.
    fixture.bind("retrieval", "read").await;
    let hits = fixture.stub.hits();
    let (code, body) = fixture
        .call("retrieval.query", json!({"query": "anything"}))
        .await;
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["type"], "tool_not_ready", "{body}");
    assert_eq!(
        body["errors"][0]["field"], "tool",
        "the refusal names the tool: {body}"
    );
    assert_eq!(fixture.stub.hits(), hits, "no network call was made");

    // And the probe reports the reason rather than inventing a destination.
    let (code, probe) = fixture
        .invoke("tools.probe", json!({"toolkit": "retrieval"}))
        .await;
    assert_eq!(code, 200, "{probe}");
    assert_eq!(
        probe["output"]["toolkits"][0]["probe"], "unconfigured",
        "{probe}"
    );
}

// ── P4.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_probe_unbinds_and_the_call_is_409() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.publish().await;
    fixture.bind("coding", "write").await;

    // Green first: the toolkit is callable.
    fixture.stub.pass_health();
    let (code, probe) = fixture
        .invoke("tools.probe", json!({"toolkit": "coding"}))
        .await;
    assert_eq!(code, 200, "{probe}");
    assert_eq!(probe["output"]["toolkits"][0]["probe"], "live", "{probe}");

    // Then the destination stops answering its health path.
    fixture.stub.fail_health();
    let (code, probe) = fixture
        .invoke("tools.probe", json!({"toolkit": "coding"}))
        .await;
    assert_eq!(code, 200, "{probe}");
    assert_eq!(
        probe["output"]["toolkits"][0]["probe"], "unreachable",
        "{probe}"
    );
    assert!(
        probe["output"]["toolkits"][0]["bindings_disabled"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "the probe unbinds: {probe}"
    );

    // The refusal happens before the destination, and it names the blocking tool.
    let hits = fixture.stub.hits();
    let (code, body) = fixture
        .call("coding.jobs_get", json!({"id": "job-1"}))
        .await;
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["type"], "tool_not_ready", "{body}");
    assert_eq!(body["errors"][0]["field"], "tool", "{body}");
    assert_eq!(fixture.stub.hits(), hits, "refused before the backend");
    assert!(
        fixture.ledger().await.is_empty(),
        "no ledger row for a refusal"
    );
}

// ── P2.2 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_probe_green_toolkit_is_callable() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.publish().await;
    fixture.bind("coding", "write").await;

    // A failed probe unbinds; the destination returns; a green probe re-binds exactly what it
    // unbound and the call succeeds with its ledger row.
    fixture.stub.fail_health();
    fixture
        .invoke("tools.probe", json!({"toolkit": "coding"}))
        .await;
    fixture.stub.pass_health();
    let (code, probe) = fixture
        .invoke("tools.probe", json!({"toolkit": "coding"}))
        .await;
    assert_eq!(code, 200, "{probe}");
    assert_eq!(probe["output"]["toolkits"][0]["probe"], "live", "{probe}");
    assert_eq!(
        probe["output"]["toolkits"][0]["bindings_restored"].as_u64(),
        Some(1),
        "the refusal lifted: {probe}"
    );

    let (code, body) = fixture
        .call("coding.jobs_get", json!({"id": "job-1"}))
        .await;
    assert_eq!(code, 200, "{body}");
    let ledger = fixture.ledger().await;
    assert_eq!(ledger.len(), 1, "one ledger row carries the call");
    assert_eq!(ledger[0].toolkit_key, "coding");
    assert_eq!(ledger[0].status, "ok");
}

// ── P4.2 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_in_flight_run_finishes_after_the_unbind() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.publish().await;
    fixture.bind("coding", "write").await;

    // The destination answers slowly, and the unbind waits for it to have RECEIVED the call:
    // the stub counts a hit before its delay, so a hit proves the broker already resolved the
    // binding and is inside the dispatch. A wall-clock guess instead (the first version of this
    // test slept 120 ms) fails on a loaded CI runner, where the request has not reached the
    // broker yet and the unbind lands first — which would be the `409` this test must not see.
    fixture.stub.delay(600);
    let call = fixture.running.invoke(
        "tools.call",
        "dev:alice",
        json!({"run_id": fixture.run_id, "tool": "coding.jobs_get", "input": {"id": "job-1"}}),
        None,
        None,
        None,
    );
    let unbind = async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while fixture.stub.hits() == 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            fixture.stub.hits() >= 1,
            "the call must reach the destination before the unbind"
        );
        let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
            .await
            .expect("a scoped write");
        let toolkit_id: Uuid =
            sqlx::query_scalar("SELECT id FROM toolkit WHERE tenant_id = $1 AND key = 'coding'")
                .bind(fixture.tenant.as_uuid())
                .fetch_one(&mut *tx)
                .await
                .expect("the coding toolkit");
        let disabled = tool::disable_bindings(&mut tx, fixture.tenant, toolkit_id)
            .await
            .expect("unbind mid-flight");
        tx.commit().await.expect("commit the unbind");
        disabled
    };
    let ((code, body), disabled) = tokio::join!(call, unbind);
    assert!(disabled >= 1, "the binding was disabled while the call ran");
    assert_eq!(code, 200, "an unbind never kills an in-flight call: {body}");
    let ledger = fixture.ledger().await;
    assert_eq!(ledger.len(), 1, "its ledger row is kept");
}

// ── P3.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_zero_use_report_flags_only_uncalled_tools() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.publish().await;
    fixture.bind("coding", "write").await;

    let (code, body) = fixture
        .call("coding.jobs_get", json!({"id": "job-1"}))
        .await;
    assert_eq!(code, 200, "{body}");

    let (code, report) = fixture.invoke("tools.usage", json!({})).await;
    assert_eq!(code, 200, "{report}");
    let rows = report["output"]["rows"].as_array().expect("rows");
    let called = rows
        .iter()
        .find(|row| row["toolkit"] == "coding" && row["tool"] == "jobs_get")
        .expect("the called tool");
    assert_eq!(called["calls"], 1, "{report}");
    assert_eq!(called["zero_use"], false, "{report}");

    let zero = report["output"]["zero_use"]
        .as_array()
        .expect("the zero-use set");
    assert!(
        zero.iter().any(|row| row["toolkit"] == "speech"),
        "an uncalled toolkit is flagged: {report}"
    );
    assert!(
        !zero
            .iter()
            .any(|row| row["toolkit"] == "coding" && row["tool"] == "jobs_get"),
        "the called tool is not flagged: {report}"
    );

    let (code, refused) = fixture
        .invoke("tools.usage", json!({"window_days": 0}))
        .await;
    assert_eq!(code, 422, "a zero-day window is refused: {refused}");
}

#[test]
fn a_write_tool_without_its_contract_is_refused() {
    let mut manifest = conveyor_actions::toolkits::manifest::manifests()
        .expect("the manifests validate")[0]
        .clone();
    assert_eq!(manifest.key, "coding");
    manifest.tools[0].idempotency = None;
    manifest.tools[0].verification = None;
    let error =
        conveyor_actions::toolkits::publish::check_contract(&manifest).expect_err("refused");
    let message = format!("{error:?}");
    assert!(
        message.contains("jobs_create"),
        "the refusal names the tool: {message}"
    );
}

#[tokio::test]
async fn the_published_tool_carries_its_contract() {
    let fixture = Fixture::new().await;
    fixture.publish().await;
    let (status, body) = fixture
        .invoke(
            "tools.get",
            json!({"toolkit": "coding", "tool": "jobs_create"}),
        )
        .await;
    assert_eq!(status, 200, "tools.get: {body}");
    assert_eq!(body["output"]["idempotency"], "required", "{body}");
    assert_eq!(body["output"]["verification"], "jobs_get", "{body}");
    assert!(body["output"]["output_schema"].is_object(), "{body}");
    assert!(body["output"]["redact"].is_array(), "{body}");
}

// ── #561: the research and Lean wave ─────────────────────────────────────────────────────

/// #561 P4.3: the research and Lean manifests publish once, each as a draft; a draft resolves no
/// binding, and the probe reports the destination's health path instead of inventing one.
///
/// A draft has no resolvable binding, so there is nothing for a failed probe to disable here; the
/// unbind/re-enable path is the published `coding` binding in `a_failed_probe_unbinds_and_the_call_is_409`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_research_and_lean_wave_publishes_once() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    support::set_env("CONVEYOR_EXTERNAL_WEBSCRAPER_URL", &fixture.stub.base);
    support::set_env("CONVEYOR_EXTERNAL_LEAN4_URL", &fixture.stub.base);

    let first = fixture.publish().await;
    for key in ["webscraper", "lean4"] {
        let row = outcomes(&first)
            .into_iter()
            .find(|(k, _, _)| k == key)
            .unwrap_or_else(|| panic!("{key} row: {first}"));
        assert_eq!(
            row,
            (key.to_owned(), "created".to_owned(), 1),
            "{key}: {first}"
        );
        let (status, version) = fixture.toolkit_status(key).await;
        assert_eq!((status.as_str(), version), ("draft", 1), "{key}");
    }

    let second = fixture.publish().await;
    for key in ["webscraper", "lean4"] {
        let row = outcomes(&second)
            .into_iter()
            .find(|(k, _, _)| k == key)
            .unwrap_or_else(|| panic!("{key} row: {second}"));
        assert_eq!(row.1, "unchanged", "{key}: {second}");
        assert_eq!(row.2, 1, "{key}: {second}");
    }

    // A draft binds nothing: the broker refuses before any destination call.
    fixture.bind("webscraper", "read").await;
    let hits = fixture.stub.hits();
    let (code, body) = fixture
        .call(
            "webscraper.scrape_url",
            json!({"url": "https://example.test/page"}),
        )
        .await;
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["type"], "tool_not_ready", "{body}");
    assert_eq!(fixture.stub.hits(), hits, "no network call was made");

    // The probe walks the wave and reads the manifest's health path.
    fixture.stub.pass_health();
    let (code, probe) = fixture
        .invoke("tools.probe", json!({"toolkit": "webscraper"}))
        .await;
    assert_eq!(code, 200, "{probe}");
    assert_eq!(probe["output"]["toolkits"][0]["probe"], "live", "{probe}");
}
