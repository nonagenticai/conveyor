//! The write path, end to end: a served request produces its `inference_call` row and its
//! `billing:costs` event.
//!
//! The parity targets are the oracle's own artifacts (`contract/golden/ledger_*.json`,
//! `cost_*.txt`), captured from the pinned old image. The hashes are compared verbatim: they are
//! deterministic for a fixed request, and they are the one part of the row that a plausible-looking
//! implementation gets wrong silently.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::post;
use conveyor_inference_router::billing::AllowAllGate;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::metering::{CostEvent, CostSink};
use conveyor_inference_router::provider::BoxFuture;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

/// The oracle's request hash for `contract/golden/completion_attributed.request`.
const GOLDEN_REQUEST_SHA256: &str =
    "86e40348206a3f554dd6bf74ed35eef822db7ffe9e84f731b33da1dfd6f45813";

#[derive(Default)]
struct Recorder(Mutex<Vec<CostEvent>>);

impl CostSink for Recorder {
    fn emit<'a>(&'a self, event: &'a CostEvent) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.0.lock().expect("recorder").push(event.clone());
        })
    }
}

async fn stub_provider() -> std::net::SocketAddr {
    async fn handler(axum::Json(_body): axum::Json<Value>) -> axum::Json<Value> {
        axum::Json(json!({
            "id": "chatcmpl-test",
            "model": "stub-model",
            "served_model": "stub-model",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "stub completion"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
        }))
    }
    let app = Router::new().route("/v1/chat/completions", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("stub serves") });
    address
}

/// The `completion_attributed` request, byte for byte: model, content and max_tokens all feed the
/// request fingerprint, so changing any of them changes the hash this test pins.
fn attributed_request() -> Body {
    Body::from(
        json!({
            "model": "stub-model",
            "messages": [{"role": "user", "content": "attributed"}],
            "max_tokens": 8
        })
        .to_string(),
    )
}

#[tokio::test]
async fn a_served_request_writes_its_row_and_one_cost_event() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("the service's own migrations");

    let address = stub_provider().await;
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\ndefault_chain: [stub-openai]\n"
    ))
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let config = Config::from_lookup(|_| None).expect("defaults");
    let recorder = Arc::new(Recorder::default());
    let state = AppState::with_deps(
        config,
        router,
        AppDeps {
            ledger: Some(db.pool.clone()),
            budget: Arc::new(AllowAllGate),
            cost: recorder.clone(),
            ..AppDeps::default()
        },
    );

    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .header("x-tenant-id", "00000000-0000-4000-8000-0000000000a1")
                .header("x-correlation-id", "00000000-0000-4000-8000-0000000000e1")
                .header("x-worker-name", "golden-worker")
                .header("x-aa-trace-id", "00000000-0000-4000-8000-0000000000f6")
                .header("x-aa-card-id", "00000000-0000-4000-8000-0000000000f7")
                .header("x-project-id", "00000000-0000-4000-8000-0000000000f5")
                .header("x-aa-parent-exec", "00000000-0000-4000-8000-0000000000f8")
                .header("x-stage", "golden")
                .body(attributed_request())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let row: Value =
        sqlx::query_scalar("SELECT row_to_json(t) FROM (SELECT * FROM inference_call LIMIT 1) t")
            .fetch_one(&db.pool)
            .await
            .expect("the served request wrote a ledger row");

    assert_eq!(row["request_sha256"], GOLDEN_REQUEST_SHA256);
    assert_eq!(
        row["response_sha256"],
        "ba6f4f2b57c61a5dcf72a7b77e61340cfd9239c18f3e558200fb81b7d85637d1"
    );
    assert_eq!(row["tenant_id"], "00000000-0000-4000-8000-0000000000a1");
    assert_eq!(
        row["correlation_id"],
        "00000000-0000-4000-8000-0000000000e1"
    );
    assert_eq!(row["caller_worker"], "golden-worker");
    assert_eq!(row["trace_id"], "00000000-0000-4000-8000-0000000000f6");
    assert_eq!(row["card_id"], "00000000-0000-4000-8000-0000000000f7");
    assert_eq!(row["project_id"], "00000000-0000-4000-8000-0000000000f5");
    assert_eq!(
        row["parent_execution_id"],
        "00000000-0000-4000-8000-0000000000f8"
    );
    assert_eq!(row["requested_model"], "stub-model");
    assert_eq!(row["served_provider"], "stub-openai");
    assert_eq!(row["temperature"], 0.1);
    assert_eq!(row["max_tokens"], 8);
    assert_eq!(row["finish_reason"], "stop");
    assert_eq!(row["status"], "ok");
    assert_eq!(row["traffic_source"], "live");
    assert_eq!(row["streamed"], false);
    assert_eq!(row["response_json_valid"], false);
    // No policy routed this request.
    assert!(row["policy_version"].is_null());
    assert!(row["hop_index"].is_null());
    // The request text hash is over OUR upstream bytes, so it is asserted as well-formed rather
    // than compared to the oracle's (the two clients serialise the same request differently).
    let text_sha = row["request_text_sha256"]
        .as_str()
        .expect("a served request hashes the text it sent");
    assert_eq!(text_sha.len(), 64);

    let events = recorder.0.lock().expect("recorder");
    assert_eq!(events.len(), 1, "one event per provider attempt");
    let event = &events[0];
    assert_eq!(event.provider, "stub-openai");
    assert_eq!(event.model, "stub-model");
    assert_eq!(event.served_model.as_deref(), Some("stub-model"));
    assert_eq!(
        event.correlation_id.as_deref(),
        Some("00000000-0000-4000-8000-0000000000e1")
    );
    assert_eq!(event.stage.as_deref(), Some("golden"));
    assert_eq!(event.worker_name.as_deref(), Some("golden-worker"));
    assert_eq!(event.service_key, "inference-router");
    assert_eq!(event.cost_source, "inference-router");
    assert_eq!(event.input_tokens, 7);
    assert_eq!(event.output_tokens, 3);
    assert_eq!(event.schema_id.as_deref(), Some("default"));
    assert!(!event.fallback_used);
    assert_eq!(
        event.project_id.as_deref(),
        Some("00000000-0000-4000-8000-0000000000f5")
    );
    assert_eq!(
        event.card_id.as_deref(),
        Some("00000000-0000-4000-8000-0000000000f7")
    );
    assert_eq!(
        event.execution_id.as_deref(),
        Some("00000000-0000-4000-8000-0000000000f8")
    );

    // Field presence, in the reference's order: the six identity fields are always on the wire.
    let names: Vec<&str> = event.field_pairs().iter().map(|(name, _)| *name).collect();
    assert_eq!(
        &names[..8],
        &[
            "tenant_id",
            "license_id",
            "subject_id",
            "correlation_id",
            "stage",
            "worker_name",
            "service_key",
            "cost_source"
        ]
    );
}

#[tokio::test]
async fn a_database_failure_never_fails_the_customers_request() {
    // The pool points at a database that does not have the ledger table: the insert fails, and the
    // response must still be the provider's 200.
    let address = stub_provider().await;
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\ndefault_chain: [stub-openai]\n"
    ))
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let config = Config::from_lookup(|_| None).expect("defaults");
    let db = common::fresh().await; // migrated? no: no tables, so every INSERT fails
    let state = AppState::with_deps(
        config,
        router,
        AppDeps {
            ledger: Some(db.pool.clone()),
            budget: Arc::new(AllowAllGate),
            cost: Arc::new(Recorder::default()),
            ..AppDeps::default()
        },
    );
    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .body(attributed_request())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

/// The oracle's own policy document (`contract/stubs/policy-local.json`) — the same bytes the
/// pinned image was seeded with. Using the file, not a copy of its shape, is the point.
const POLICY_DOC: &str = include_str!("../contract/stubs/policy-local.json");

const TENANT: &str = "00000000-0000-4000-8000-0000000000a1";

async fn seed_policy(pool: &sqlx::PgPool, doc: &str) {
    let tenant = uuid::Uuid::parse_str(TENANT).expect("tenant uuid");
    let doc: Value = serde_json::from_str(doc).expect("a policy document");
    sqlx::query(
        "INSERT INTO tenant_inference_policy_version (tenant_id, version, doc, doc_sha256, actor_kind, actor_ref_hash)
         VALUES ($1, 1, $2, 'test', 'test', 'test')",
    )
    .bind(tenant)
    .bind(&doc)
    .execute(pool)
    .await
    .expect("the version row");
    sqlx::query("INSERT INTO tenant_inference_policy (tenant_id, active_version) VALUES ($1, 1)")
        .bind(tenant)
        .execute(pool)
        .await
        .expect("the active row");
}

fn policy_config() -> Config {
    Config::from_lookup(|key| (key == "INFERENCE_ROUTER_POLICY_MODE").then(|| "on".to_owned()))
        .expect("policy mode on")
}

async fn policy_state(pool: sqlx::PgPool, address: std::net::SocketAddr) -> AppState {
    let policy_store = Arc::new(conveyor_inference_router::policy::PolicyStore::new(
        Some(pool.clone()),
        Duration::from_secs(15),
    ));
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\n      local-llm: stub-model\ndefault_chain: [stub-openai]\n"
    ))
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    AppState::with_deps(
        policy_config(),
        router,
        AppDeps {
            ledger: Some(pool),
            budget: Arc::new(AllowAllGate),
            cost: Arc::new(Recorder::default()),
            policy: policy_store,
        },
    )
}

fn policy_request(model: &str) -> Body {
    Body::from(
        json!({
            "model": model,
            "messages": [{"role": "user", "content": "attributed"}],
            "max_tokens": 8
        })
        .to_string(),
    )
}

fn policy_headers() -> axum::http::request::Builder {
    Request::post("/api/chat/completions")
        .header("content-type", "application/json")
        .header("x-tenant-id", TENANT)
}

#[tokio::test]
async fn a_policy_routed_request_records_the_hop_it_took() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    seed_policy(&db.pool, POLICY_DOC).await;

    let address = stub_provider().await;
    let state = policy_state(db.pool.clone(), address).await;
    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            policy_headers()
                // The policy's platform hop is named by the reserved model.
                .body(policy_request("local-llm"))
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let row: Value =
        sqlx::query_scalar("SELECT row_to_json(t) FROM (SELECT * FROM inference_call LIMIT 1) t")
            .fetch_one(&db.pool)
            .await
            .expect("a ledger row");
    // The oracle's own policy-routed row (`contract/golden/ledger_policy_routed.json`).
    assert_eq!(row["requested_model"], "local-llm");
    assert_eq!(row["served_model"], "stub-model");
    assert_eq!(row["policy_version"], 1);
    assert_eq!(row["llm_setup"], 1);
    assert_eq!(row["hop_index"], 0);
    assert_eq!(row["hop_reason"], "first");
    assert_eq!(row["key_owner"], "platform");
}

#[tokio::test]
async fn a_policy_refusal_happens_before_anything_is_dialled_or_written() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    seed_policy(&db.pool, POLICY_DOC).await;

    let address = stub_provider().await;
    let state = policy_state(db.pool.clone(), address).await;
    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            policy_headers()
                // A concrete model the policy does not name: refused, not served outside it.
                .body(policy_request("stub-model"))
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&db.pool)
        .await
        .expect("count");
    assert_eq!(rows, 0, "a refused request must not reach the ledger");
}

/// The oracle's own 429 body for an over-budget tenant.
const OVER_BUDGET_BODY: &str = include_str!("../contract/golden/completion_over_budget.body");

/// A billing double that answers by the caller's tenant: `over` breaches, `nobudget` has no
/// active budget (a 429 that must NOT refuse), anyone else is allowed with a handle.
async fn billing_stub() -> std::net::SocketAddr {
    async fn reserve(
        axum::extract::State(_): axum::extract::State<()>,
        headers: axum::http::HeaderMap,
    ) -> axum::response::Response {
        let tenant = headers
            .get("x-tenant-id")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        if tenant.contains("over") {
            return axum::response::IntoResponse::into_response((
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"error": {"details": {
                    "reason": "over_budget", "remaining_tokens": 0, "cap_tokens": 1000
                }}})),
            ));
        }
        if tenant.contains("nobudget") {
            return axum::response::IntoResponse::into_response((
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"error": {"details": {"reason": "no_active_budget"}}})),
            ));
        }
        axum::response::IntoResponse::into_response(axum::Json(json!({"data": {
            "allowed": true,
            "remaining_tokens": 999999,
            "cap_tokens": 1000000,
            "reservation_id": "00000000-0000-4000-8000-0000000000d1"
        }})))
    }
    let app = Router::new()
        .route("/api/budget/reserve", post(reserve))
        .with_state(());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind billing stub");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("billing stub") });
    address
}

fn budget_config(billing: &str) -> Config {
    let billing = billing.to_owned();
    Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_BUDGET_GATE_MODE" => Some("enforce".to_owned()),
        "INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL" => Some(billing.clone()),
        _ => None,
    })
    .expect("budget config")
}

async fn gated_state(
    pool: sqlx::PgPool,
    provider: std::net::SocketAddr,
    billing: &str,
    recorder: Arc<Recorder>,
) -> AppState {
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{provider}/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\ndefault_chain: [stub-openai]\n"
    ))
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let gate = conveyor_inference_router::billing::HttpBudgetGate::new(
        billing.to_owned(),
        Duration::from_millis(750),
        60,
        conveyor_inference_router::config::GateMode::Enforce,
    )
    .expect("gate");
    AppState::with_deps(
        budget_config(billing),
        router,
        AppDeps {
            ledger: Some(pool),
            budget: Arc::new(gate),
            cost: recorder,
            ..AppDeps::default()
        },
    )
}

fn tenant_headers(
    builder: axum::http::request::Builder,
    tenant: &str,
) -> axum::http::request::Builder {
    builder
        .header("content-type", "application/json")
        .header("x-tenant-id", tenant)
}

#[tokio::test]
async fn an_over_budget_request_is_refused_with_the_measured_body() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let provider = stub_provider().await;
    let billing = billing_stub().await;
    let state = gated_state(
        db.pool.clone(),
        provider,
        &format!("http://{billing}"),
        Arc::new(Recorder::default()),
    )
    .await;

    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            tenant_headers(Request::post("/api/chat/completions"), "over")
                .body(attributed_request())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let got: Value = serde_json::from_slice(&bytes).expect("json");
    let want: Value = serde_json::from_str(OVER_BUDGET_BODY).expect("the golden body");
    assert_eq!(got, want, "the refusal body must match the captured one");

    // Refused at admission: nothing was dialled and nothing was written.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&db.pool)
        .await
        .expect("count");
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn a_budget_that_is_not_armed_for_this_tenant_fails_open() {
    // `no_active_budget` arrives as a 429 but is NOT a breach: the request is served.
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let provider = stub_provider().await;
    let billing = billing_stub().await;
    let recorder = Arc::new(Recorder::default());
    let state = gated_state(
        db.pool.clone(),
        provider,
        &format!("http://{billing}"),
        recorder.clone(),
    )
    .await;

    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            tenant_headers(Request::post("/api/chat/completions"), "nobudget")
                .body(attributed_request())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(recorder.0.lock().expect("recorder").len(), 1);
}

#[tokio::test]
async fn an_unreachable_billing_fails_open() {
    // A port nothing listens on: transport error, allow, serve.
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let provider = stub_provider().await;
    let state = gated_state(
        db.pool.clone(),
        provider,
        "http://127.0.0.1:1",
        Arc::new(Recorder::default()),
    )
    .await;
    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            tenant_headers(
                Request::post("/api/chat/completions"),
                "00000000-0000-4000-8000-0000000000b4",
            )
            .body(attributed_request())
            .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

// --- effective-tenant delegation (S8a.0) -------------------------------------------
//
// `AUTH_DELEGATING_SUBJECTS` is armed in the live deployment, so a platform service can name the
// tenant a call is FOR. Only an INTROSPECTED subject may vouch; anything else keeps the caller's
// own tenant. The billed tenant and the cost event's `delegated_by` are what these assert.

const LICENCE_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CALLER_TENANT: &str = "00000000-0000-4000-8000-0000000000a1";
const DELEGATED_TENANT: &str = "00000000-0000-4000-8000-0000000000b2";
const DELEGATING_SUBJECT: &str = "8382f0b3-c082-4f97-8d47-120ce5a18e05";
const OTHER_SUBJECT: &str = "01a0dd02-d324-77bb-acd8-7c31fb6e95ec";

async fn stub_licences(subject: &str) -> std::net::SocketAddr {
    let payload = json!({
        "active": true,
        "tenant_id": CALLER_TENANT,
        "license_id": "00000000-0000-4000-8000-0000000000c1",
        "app": {"key": "inference-router"},
        "expires_at": "2030-01-01T00:00:00Z",
        "subject_id": subject,
    });
    async fn handler(
        axum::extract::State(payload): axum::extract::State<Value>,
    ) -> axum::Json<Value> {
        axum::Json(payload)
    }
    let app = Router::new()
        .route("/license/introspect", post(handler))
        .with_state(payload);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind licences");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("licences stub serves")
    });
    address
}

async fn delegation_state(
    pool: sqlx::PgPool,
    provider: std::net::SocketAddr,
    licences: std::net::SocketAddr,
    allowlist: &str,
    recorder: Arc<Recorder>,
) -> AppState {
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{provider}/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\ndefault_chain: [stub-openai]\n"
    ))
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let url = format!("http://{licences}/license/introspect");
    let allowlist = allowlist.to_owned();
    let config = Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_INTROSPECT_MODE" => Some("enforce".to_owned()),
        "INFERENCE_ROUTER_LICENSE_INTROSPECT_URL" => Some(url.clone()),
        "AUTH_DELEGATING_SUBJECTS" => Some(allowlist.clone()),
        _ => None,
    })
    .expect("delegation config");
    AppState::with_deps(
        config,
        router,
        AppDeps {
            ledger: Some(pool),
            budget: Arc::new(AllowAllGate),
            cost: recorder,
            ..AppDeps::default()
        },
    )
}

fn delegated_request() -> axum::http::request::Builder {
    Request::post("/api/chat/completions")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {LICENCE_TOKEN}"))
        .header("x-effective-tenant-id", DELEGATED_TENANT)
}

/// The `completion_delegated` golden's body, byte for byte — the fingerprints below are the
/// oracle's for exactly this request (`contract/golden/ledger_delegated.json`).
fn delegated_body() -> Body {
    Body::from(
        json!({
            "model": "stub-model",
            "messages": [{"role": "user", "content": "delegated"}],
            "max_tokens": 8
        })
        .to_string(),
    )
}

async fn one_row(pool: &sqlx::PgPool) -> Value {
    sqlx::query_scalar("SELECT row_to_json(t) FROM (SELECT * FROM inference_call LIMIT 1) t")
        .fetch_one(pool)
        .await
        .expect("a ledger row")
}

#[tokio::test]
async fn a_vouched_delegation_moves_the_billed_tenant_and_stamps_delegated_by() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let provider = stub_provider().await;
    let licences = stub_licences(DELEGATING_SUBJECT).await;
    let recorder = Arc::new(Recorder::default());
    let state = delegation_state(
        db.pool.clone(),
        provider,
        licences,
        DELEGATING_SUBJECT,
        recorder.clone(),
    )
    .await;

    let response = conveyor_inference_router::router::router(state)
        .oneshot(delegated_request().body(delegated_body()).expect("request"))
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let row = one_row(&db.pool).await;
    // The oracle's own row for this request: the billed tenant is the DELEGATED one, the licence
    // still belongs to the caller, and the fingerprints are unchanged by delegation.
    assert_eq!(row["tenant_id"], DELEGATED_TENANT);
    assert_eq!(row["licence_id"], "00000000-0000-4000-8000-0000000000c1");
    assert_eq!(
        row["request_sha256"],
        "4b5b9232f60197f46d643854e179f2df84c99bded7cc7aee97fc1cfea4568eec"
    );
    assert_eq!(
        row["response_sha256"],
        "ba6f4f2b57c61a5dcf72a7b77e61340cfd9239c18f3e558200fb81b7d85637d1"
    );
    let events = recorder.0.lock().expect("recorder");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tenant_id.as_deref(), Some(DELEGATED_TENANT));
    assert_eq!(events[0].delegated_by.as_deref(), Some(DELEGATING_SUBJECT));
}

#[tokio::test]
async fn an_unvouched_delegation_keeps_the_callers_tenant() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let provider = stub_provider().await;
    // The licence server vouches for a subject that is NOT on the fleet allowlist.
    let licences = stub_licences(OTHER_SUBJECT).await;
    let recorder = Arc::new(Recorder::default());
    let state = delegation_state(
        db.pool.clone(),
        provider,
        licences,
        DELEGATING_SUBJECT,
        recorder.clone(),
    )
    .await;

    let response = conveyor_inference_router::router::router(state)
        .oneshot(delegated_request().body(delegated_body()).expect("request"))
        .await
        .expect("router answers");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let row = one_row(&db.pool).await;
    assert_eq!(row["tenant_id"], CALLER_TENANT);
    let events = recorder.0.lock().expect("recorder");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tenant_id.as_deref(), Some(CALLER_TENANT));
    assert_eq!(events[0].delegated_by, None);
}
