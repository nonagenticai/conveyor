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
