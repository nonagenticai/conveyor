//! The capture write path: `X-Capture: full` from an allowlisted `X-App-Id` stores the
//! request/response bodies and the verbatim wire bytes; every other combination stores hashes
//! only. The read route is exercised through the same router, so the two halves are proven
//! together — a store nobody can read, or a read that finds nothing, both fail here.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use conveyor_inference_router::billing::AllowAllGate;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::metering::NoopCostSink;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

const INTERNAL_SECRET: &str = "stub-internal-secret";
const CAPTURE_APP: &str = "probe-app";

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

fn config() -> Config {
    Config::from_lookup(|key| match key {
        "INFERENCE_CAPTURE_ALLOWED_CALLERS" => Some(CAPTURE_APP.to_owned()),
        "INFERENCE_ROUTER_INTERNAL_SECRET" => Some(INTERNAL_SECRET.to_owned()),
        _ => None,
    })
    .expect("config")
}

async fn state() -> (AppState, common::TestDb) {
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
    let state = AppState::with_deps(
        config(),
        router,
        AppDeps {
            ledger: Some(db.pool.clone()),
            budget: Arc::new(AllowAllGate),
            cost: Arc::new(NoopCostSink),
            ..AppDeps::default()
        },
    );
    (state, db)
}

fn body() -> Body {
    Body::from(
        json!({
            "model": "stub-model",
            "messages": [{"role": "user", "content": "captured"}],
            "max_tokens": 8
        })
        .to_string(),
    )
}

fn completion(headers: &[(&str, &str)]) -> Request<Body> {
    let mut request = Request::post("/api/chat/completions")
        .header("content-type", "application/json")
        .header("x-tenant-id", "00000000-0000-4000-8000-0000000000a1");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.body(body()).expect("request")
}

async fn row(db: &common::TestDb) -> Value {
    sqlx::query_scalar(
        "SELECT json_build_object(
            'id', id,
            'request_json', request_json,
            'response_json', response_json,
            'request_text', request_text,
            'response_text', response_text,
            'request_sha256', request_sha256,
            'request_text_sha256', request_text_sha256
         ) FROM inference_call ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&db.pool)
    .await
    .expect("the row")
}

async fn captured(state: &AppState, id: i64) -> (StatusCode, Value) {
    let response = conveyor_inference_router::router::router(state.clone())
        .oneshot(
            Request::get(format!("/api/internal/inference-calls/{id}/capture"))
                .header("x-internal-secret", INTERNAL_SECRET)
                .header("x-app-id", CAPTURE_APP)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

#[tokio::test]
async fn an_allowlisted_capture_writes_the_bodies_and_the_read_serves_them() {
    let (state, db) = state().await;
    let response = conveyor_inference_router::router::router(state.clone())
        .oneshot(completion(&[
            ("x-capture", "full"),
            ("x-app-id", CAPTURE_APP),
        ]))
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let row = row(&db).await;
    assert_eq!(row["request_json"]["model"], json!("stub-model"));
    assert!(
        row["response_json"].is_object(),
        "the response body is stored: {row}"
    );
    assert!(row["request_text"].is_string(), "the wire bytes are stored");
    assert!(row["response_text"].is_string());

    let id = row["id"].as_i64().expect("id");
    let (status, capture) = captured(&state, id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(capture["request"], row["request_json"]);
    assert_eq!(capture["response"], row["response_json"]);
    // The read's own integrity checks: the stored body minus `model` re-hashes to
    // `request_sha256`, and the stored text re-hashes to `request_text_sha256`.
    assert_eq!(capture["request_sha256_verified"], json!(true));
    assert_eq!(capture["request_text_verified"], json!(true));
}

#[tokio::test]
async fn without_x_capture_the_body_is_not_stored() {
    let (state, db) = state().await;
    let response = conveyor_inference_router::router::router(state.clone())
        .oneshot(completion(&[("x-app-id", CAPTURE_APP)]))
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let row = row(&db).await;
    assert!(row["request_json"].is_null());
    assert!(row["response_json"].is_null());
    assert!(row["request_text"].is_null());
    assert!(row["response_text"].is_null());
    assert!(
        row["request_text_sha256"].is_string(),
        "the hash is always written"
    );

    let id = row["id"].as_i64().expect("id");
    let (status, capture) = captured(&state, id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(capture["detail"], json!("not captured"));
}

#[tokio::test]
async fn an_unlisted_app_id_cannot_capture() {
    let (state, db) = state().await;
    let response = conveyor_inference_router::router::router(state.clone())
        .oneshot(completion(&[
            ("x-capture", "full"),
            ("x-app-id", "not-on-the-list"),
        ]))
        .await
        .expect("router answers");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a refused capture still serves"
    );

    let row = row(&db).await;
    assert!(row["request_json"].is_null());
    assert!(row["response_json"].is_null());
    assert!(row["request_text"].is_null());
    assert!(row["response_text"].is_null());
}
