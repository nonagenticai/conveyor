//! The catalogue reads: who may see which provider, and the 404 that does not disclose one.
//!
//! The rule is the reference's: a customer (anything not PROVEN to be a listed platform tenant)
//! sees only `customer_eligible` providers; a proven platform tenant sees the whole catalogue.
//! An asserted `X-Tenant-Id` is never proof.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TENANT: &str = "00000000-0000-4000-8000-0000000000a1";

fn catalog() -> Catalog {
    Catalog::from_yaml(
        "providers:
  - name: local
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: http://127.0.0.1:9/v1
    model: local-model
    model_aliases:
      local-alias: local-model
  - name: commercial
    enabled: true
    protocol: openai
    endpoint: http://127.0.0.1:9/v1
    model: big-model
default_chain: [local, commercial]
",
    )
    .expect("catalog")
}

fn state(config: Config) -> AppState {
    let router = RouterService::new(catalog(), Duration::from_secs(1)).expect("router");
    AppState::new(config, router)
}

fn customer_config() -> Config {
    Config::from_lookup(|_| None).expect("defaults")
}

fn platform_config(licences: std::net::SocketAddr) -> Config {
    let url = format!("http://{licences}/license/introspect");
    Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_INTROSPECT_MODE" => Some("enforce".to_owned()),
        "INFERENCE_ROUTER_LICENSE_INTROSPECT_URL" => Some(url.clone()),
        "COMMERCIAL_PLATFORM_TENANT_IDS" => Some(TENANT.to_owned()),
        _ => None,
    })
    .expect("platform config")
}

async fn stub_licences() -> std::net::SocketAddr {
    let payload = json!({
        "active": true,
        "tenant_id": TENANT,
        "license_id": "00000000-0000-4000-8000-0000000000c1",
        "app": {"key": "inference-router"},
        "expires_at": "2030-01-01T00:00:00Z",
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
    tokio::spawn(async move { axum::serve(listener, app).await.expect("stub serves") });
    address
}

async fn get_json(state: AppState, path: &str, headers: &[(&str, &str)]) -> (StatusCode, Value) {
    let mut request = Request::get(path).header("accept", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = conveyor_inference_router::router::router(state)
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn model_ids(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .expect("data")
        .iter()
        .map(|entry| entry["id"].as_str().expect("id").to_owned())
        .collect()
}

#[tokio::test]
async fn a_customer_sees_only_customer_eligible_providers() {
    let (status, models) = get_json(state(customer_config()), "/api/models", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        model_ids(&models),
        ["default", "local-model", "local-alias"]
    );
    assert_eq!(models["data"][0]["provider_chain"], json!(["local"]));

    let (status, providers) = get_json(state(customer_config()), "/api/providers", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(providers["data"][0]["name"], "local");
    assert_eq!(providers["default_chain"], json!(["local"]));
    assert_eq!(providers["data"].as_array().map(Vec::len), Some(1));

    // The commercial provider is invisible, and answers exactly like an unknown one.
    let (status, body) = get_json(
        state(customer_config()),
        "/api/providers/commercial/models",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["detail"], "Unknown provider: commercial");
    let (status, _) = get_json(
        state(customer_config()),
        "/api/providers/acme-nonexistent/models",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_asserted_platform_tenant_is_still_a_customer() {
    // No introspection: the header is an assertion, and an assertion is never proof.
    let (status, models) = get_json(
        state(customer_config()),
        "/api/models",
        &[("x-tenant-id", TENANT)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        model_ids(&models),
        ["default", "local-model", "local-alias"]
    );
}

#[tokio::test]
async fn a_proven_platform_tenant_sees_the_commercial_catalogue() {
    let licences = stub_licences().await;
    let (status, models) = get_json(
        state(platform_config(licences)),
        "/api/models",
        &[("authorization", &format!("Bearer {TOKEN}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        model_ids(&models),
        ["default", "local-model", "local-alias", "big-model"]
    );
    assert_eq!(
        models["data"][0]["provider_chain"],
        json!(["local", "commercial"])
    );

    let (status, providers) = get_json(
        state(platform_config(licences)),
        "/api/providers",
        &[("authorization", &format!("Bearer {TOKEN}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = providers["data"]
        .as_array()
        .expect("data")
        .iter()
        .map(|entry| entry["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["local", "commercial"]);
    assert_eq!(providers["default_chain"], json!(["local", "commercial"]));
    assert_eq!(providers["data"][1]["protocol"], "openai");

    let (status, body) = get_json(
        state(platform_config(licences)),
        "/api/providers/commercial/models",
        &[("authorization", &format!("Bearer {TOKEN}"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["provider"], "commercial");
    assert_eq!(body["data"][0]["id"], "big-model");
}

#[tokio::test]
async fn the_v1_models_alias_answers_the_same_catalogue() {
    let (status, models) = get_json(state(customer_config()), "/v1/models", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        model_ids(&models),
        ["default", "local-model", "local-alias"]
    );
}
