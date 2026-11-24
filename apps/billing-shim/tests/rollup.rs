//! In-crate end-to-end tests: a loopback stub stands in for billing-service, so nothing here
//! touches the estate or the internet.
//!
//! They pin the contract the shim owns:
//! * the translation (from/to/group_by/tenant/bearer) and the reshaped response,
//! * an upstream 500 becomes `502 billing_upstream_error`,
//! * an unconfigured upstream URL becomes `503 billing_unconfigured`,
//! * caller mistakes stay 400s instead of being blamed on the upstream.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower::ServiceExt;

use conveyor_billing_shim::router::{self, AppState};
use conveyor_billing_shim::upstream::BillingService;

const TENANT: &str = "11111111-1111-1111-1111-111111111111";

/// What the stub upstream saw on the one request it answers.
#[derive(Debug, Clone)]
struct SeenRequest {
    tenant: Option<String>,
    authorization: Option<String>,
    query: Option<String>,
}

type Seen = Arc<Mutex<Option<SeenRequest>>>;

#[derive(Clone)]
struct StubState {
    seen: Seen,
    status: StatusCode,
    body: Value,
}

/// The stub's `GET /api/usage/breakdown`: record the request, answer the fixed status/body.
async fn breakdown_stub(State(state): State<StubState>, headers: HeaderMap, uri: Uri) -> Response {
    {
        let mut seen = state.seen.lock().expect("the seen slot is not poisoned");
        *seen = Some(SeenRequest {
            tenant: headers
                .get("x-tenant-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            authorization: headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            query: uri.query().map(str::to_owned),
        });
    }
    (state.status, Json(state.body.clone())).into_response()
}

/// Start the stub on an ephemeral loopback port; return its base URL and the capture slot.
async fn spawn_stub(status: StatusCode, body: Value) -> (String, Seen) {
    let seen: Seen = Arc::new(Mutex::new(None));
    let app = Router::new()
        .route("/api/usage/breakdown", get(breakdown_stub))
        .with_state(StubState {
            seen: seen.clone(),
            status,
            body,
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port is available");
    let address = listener.local_addr().expect("the bound address is known");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("the stub serves");
    });
    (format!("http://{address}"), seen)
}

fn shim(base_url: Option<&str>, token: Option<&str>) -> Router {
    let billing =
        BillingService::new(base_url, token.map(str::to_owned)).expect("the shim client builds");
    router::router(AppState { billing })
}

fn rollup_request(tenant: bool, group_by: Option<&[&str]>) -> Request<Body> {
    let mut body = json!({
        "since": "2026-11-01",
        "until": "2026-11-08",
    });
    if let Some(group_by) = group_by {
        body["group_by"] = json!(group_by);
    }
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/usage/rollup")
        .header("content-type", "application/json");
    if tenant {
        builder = builder.header("x-tenant-id", TENANT);
    }
    builder
        .body(Body::from(
            serde_json::to_vec(&body).expect("the request body serialises"),
        ))
        .expect("the request builds")
}

async fn read_json(response: Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");
    serde_json::from_slice(&bytes).expect("the body is JSON")
}

/// The upstream envelope exactly as billing-service emits it: `dims` holds the requested
/// dimensions (null where unattributed) and money is a decimal STRING.
fn upstream_envelope() -> Value {
    json!({
        "data": {
            "tenant_id": TENANT,
            "from": "2026-11-01",
            "to": "2026-11-08",
            "group_by": ["day", "project", "actor", "app", "provider", "model"],
            "totals": {"events": 3, "input_tokens": 0, "output_tokens": 0, "cost_usd": "12.34"},
            "rows": [
                {
                    "dims": {
                        "day": "2026-11-02",
                        "project": "project-1",
                        "actor": null,
                        "app": "conveyor",
                        "provider": "openai",
                        "model": "gpt-5"
                    },
                    "events": 2,
                    "input_tokens": 10,
                    "output_tokens": 20,
                    "cost_usd": "10.00"
                },
                {
                    "dims": {
                        "day": "2026-11-03",
                        "project": null,
                        "actor": "actor-1",
                        "app": null,
                        "provider": "local",
                        "model": "llama"
                    },
                    "events": 1,
                    "input_tokens": 5,
                    "output_tokens": 5,
                    "cost_usd": "2.34"
                }
            ],
            "truncated": false
        },
        "meta": {"request_id": "stub"}
    })
}

#[tokio::test]
async fn the_rollup_is_translated_and_reshaped() {
    let (base, seen) = spawn_stub(StatusCode::OK, upstream_envelope()).await;
    let response = shim(Some(&base), Some("test-token"))
        .oneshot(rollup_request(true, None))
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;

    // The contract's three top-level fields, and nothing else.
    let top = body.as_object().expect("an object");
    assert_eq!(top.len(), 3);
    assert_eq!(body["source"], json!("billing-service"));
    assert_eq!(body["total_usd"].as_f64(), Some(12.34));
    // The total is the upstream's, not a sum of the returned rows.
    let rows = body["rows"].as_array().expect("rows is an array");
    assert_eq!(rows.len(), 2);

    // Row 1: every requested dimension survives, nulls included.
    let first = rows[0].as_object().expect("a row object");
    assert_eq!(first.len(), 2);
    assert_eq!(
        rows[0]["dimensions"],
        json!({
            "day": "2026-11-02",
            "project": "project-1",
            "actor": null,
            "app": "conveyor",
            "provider": "openai",
            "model": "gpt-5"
        })
    );
    assert_eq!(rows[0]["cost_usd"].as_f64(), Some(10.0));
    // Upstream-only fields (events, tokens...) are not invented into the answer.
    assert!(first.get("events").is_none());

    // Row 2: an unattributed project stays null, never "" and never a fill-in.
    assert_eq!(rows[1]["dimensions"]["project"], Value::Null);
    assert_eq!(rows[1]["dimensions"]["app"], Value::Null);
    assert_eq!(rows[1]["cost_usd"].as_f64(), Some(2.34));

    // The upstream request: the body's fields land on the breakdown query, the tenant and
    // bearer are forwarded, and the default group_by is the six contract dimensions.
    let seen = seen
        .lock()
        .expect("the seen slot is not poisoned")
        .clone()
        .expect("the stub saw the request");
    assert_eq!(seen.tenant.as_deref(), Some(TENANT));
    assert_eq!(seen.authorization.as_deref(), Some("Bearer test-token"));
    let query = reqwest::Url::parse(&format!(
        "http://stub/?{}",
        seen.query.expect("the request carries a query")
    ))
    .expect("the query parses");
    let pairs: HashMap<String, String> = query.query_pairs().into_owned().collect();
    assert_eq!(pairs.get("from").map(String::as_str), Some("2026-11-01"));
    assert_eq!(pairs.get("to").map(String::as_str), Some("2026-11-08"));
    assert_eq!(
        pairs.get("group_by").map(String::as_str),
        Some("day,project,actor,app,provider,model")
    );
}

#[tokio::test]
async fn a_requested_subset_is_forwarded_unchanged() {
    let subset = json!({
        "data": {
            "totals": {"events": 1, "input_tokens": 0, "output_tokens": 0, "cost_usd": "0.50"},
            "rows": [
                {
                    "dims": {"day": "2026-11-04", "model": "llama"},
                    "events": 1,
                    "input_tokens": 0,
                    "output_tokens": 0,
                    "cost_usd": "0.50"
                }
            ]
        },
        "meta": {}
    });
    let (base, seen) = spawn_stub(StatusCode::OK, subset).await;
    let response = shim(Some(&base), None)
        .oneshot(rollup_request(true, Some(&["day", "model"])))
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(
        body["rows"][0]["dimensions"],
        json!({"day": "2026-11-04", "model": "llama"})
    );
    assert_eq!(body["total_usd"].as_f64(), Some(0.5));

    let seen = seen
        .lock()
        .expect("the seen slot is not poisoned")
        .clone()
        .expect("the stub saw the request");
    assert_eq!(seen.authorization, None);
    let query = reqwest::Url::parse(&format!(
        "http://stub/?{}",
        seen.query.expect("the request carries a query")
    ))
    .expect("the query parses");
    let pairs: HashMap<String, String> = query.query_pairs().into_owned().collect();
    assert_eq!(pairs.get("group_by").map(String::as_str), Some("day,model"));
}

#[tokio::test]
async fn an_upstream_500_is_a_502_billing_upstream_error() {
    let (base, _) = spawn_stub(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"detail": "the rollup engine is down"}),
    )
    .await;
    let response = shim(Some(&base), None)
        .oneshot(rollup_request(true, None))
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = read_json(response).await;
    assert_eq!(body["type"], json!("billing_upstream_error"));
    let detail = body["detail"].as_str().expect("a detail string");
    assert!(detail.contains("500"), "detail names the status: {detail}");
}

#[tokio::test]
async fn an_unconfigured_upstream_url_is_a_503_billing_unconfigured() {
    let response = shim(None, None)
        .oneshot(rollup_request(true, None))
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = read_json(response).await;
    assert_eq!(body["type"], json!("billing_unconfigured"));
}

#[tokio::test]
async fn health_answers_ok_without_the_upstream() {
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .expect("the request builds");
    let response = shim(None, None)
        .oneshot(request)
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(read_json(response).await, json!({"status": "ok"}));
}

#[tokio::test]
async fn an_unknown_group_by_dimension_is_a_400_without_calling_upstream() {
    let (base, seen) = spawn_stub(StatusCode::OK, json!({"data": {}})).await;
    let response = shim(Some(&base), None)
        .oneshot(rollup_request(true, Some(&["day", "funding"])))
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = read_json(response).await;
    assert_eq!(body["type"], json!("invalid_request"));
    assert!(
        seen.lock()
            .expect("the seen slot is not poisoned")
            .is_none(),
        "a locally-refused request never reaches the upstream"
    );
}

#[tokio::test]
async fn a_missing_tenant_header_is_a_400_tenant_required() {
    let response = shim(None, None)
        .oneshot(rollup_request(false, None))
        .await
        .expect("the shim answers");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = read_json(response).await;
    assert_eq!(body["type"], json!("tenant_required"));
}
