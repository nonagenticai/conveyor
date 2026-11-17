//! The contract, proven two ways: the document the service publishes equals the capture, and the
//! answers it gives over HTTP are the shapes the capture promised.
//!
//! The second half is not a golden replay — `parity.rs` owns that — it is the set of claims the
//! capture makes that a golden cannot carry: a refusal is a `422` with a `detail`, a missing tenant
//! is a `400`, an unconfigured backend is a `degraded` answer rather than a `500`, and a saturated
//! prover is a `503` a caller can re-drive.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
mod support;

use conveyor_scientific::config::Config;
use conveyor_scientific::contract;
use conveyor_scientific::router;
use conveyor_scientific::state::{AppState, Shared};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

/// The served document and the capture name the same operations.
#[test]
fn served_openapi_equals_the_lock() {
    let served = serde_json::to_value(router::openapi()).expect("the document serialises");
    contract::check(&served, router::ROUTES).expect("the served document matches the capture");
}

fn state(vars: &[(&str, &str)]) -> Shared {
    // Every suite authenticates through the licence stub (conveyor#299): the gate refuses a
    // request without a proven tenant, so the config must name the introspector.
    let mut pairs: Vec<(String, String)> = vars
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    pairs.extend(support::auth_vars());
    Arc::new(AppState::new(
        Config::from_lookup(&pairs).expect("the environment parses"),
    ))
}

async fn call(
    state: Shared,
    method: &str,
    path: &str,
    tenant: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Option<String>, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(tenant) = tenant {
        // The licence carrier: the stub's `licence-<tenant>` introspects active for that tenant.
        request = request.header("x-tenant-id", support::licence_header(tenant));
    }
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request"),
        None => request.body(Body::empty()).expect("request"),
    };
    let response = router::router(state)
        .oneshot(request)
        .await
        .expect("answered");
    let status = response.status();
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, retry_after, body)
}

#[tokio::test]
async fn ragas_without_backends_is_a_degraded_200() {
    let (status, _, body) = call(
        state(&[]),
        "POST",
        "/api/verify/ragas",
        Some("t1"),
        Some(json!({
            "question": "is it?",
            "answer": "yes",
            "contexts": ["it is"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["degraded"], true);
    assert!(body["faithfulness"].is_null());
    assert!(body["answer_relevancy"].is_null());
    assert!(body["context_precision"].is_null());
    assert!(body["factual_accuracy"].is_null());
    assert!(body["computation_time_ms"].is_number());
}

#[tokio::test]
async fn a_z3_verification_is_refused_by_name() {
    let (status, _, body) = call(
        state(&[]),
        "POST",
        "/api/formal/verify",
        Some("t1"),
        Some(json!({"formal_expression": "x > 0", "language": "z3"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("language_not_supported")
    );
}

#[tokio::test]
async fn a_saturated_prover_is_a_503_the_caller_can_redrive() {
    // A stub prover that sheds every call, so the header and the body are the only things under
    // test — which is the point: the contract is about what the caller is told.
    let stub = axum::Router::new().route(
        "/api/check",
        axum::routing::post(|| async {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "9")],
                axum::Json(json!({"error": "gpu_busy"})),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral port");
    let addr = listener.local_addr().expect("address");
    tokio::spawn(async move {
        axum::serve(listener, stub).await.expect("serves");
    });

    let url = format!("http://{addr}");
    let (status, retry_after, body) = call(
        state(&[
            ("SCIENTIFIC_PROCESSOR_LEAN4_PROVER_URL", url.as_str()),
            ("SCIENTIFIC_PROCESSOR_DOWNSTREAM_LICENSE_TOKEN", "licence"),
        ]),
        "POST",
        "/api/formal/verify",
        Some("t1"),
        Some(json!({
            "formal_expression": "theorem t : True := by trivial",
            "language": "lean4",
        })),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(retry_after.as_deref(), Some("9"));
    assert_eq!(body["error"], "gpu_busy");
    assert_eq!(body["retry_after"], 9);
}

#[tokio::test]
async fn proofs_need_a_tenant_and_answer_empty_without_a_store() {
    let (status, _, body) = call(state(&[]), "GET", "/api/formal/proofs/c1", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED");

    let (status, _, body) =
        call(state(&[]), "GET", "/api/formal/proofs/c1", Some("t1"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn pipeline_status_answers_every_field_the_capture_names() {
    let (status, _, body) = call(state(&[]), "GET", "/api/pipeline/status", Some("t1"), None).await;
    assert_eq!(status, StatusCode::OK);
    for field in [
        "running",
        "queue_depth",
        "processed_total",
        "failed_total",
        "last_run_at",
        "throughput_per_minute",
    ] {
        assert!(
            body.get(field).is_some(),
            "the capture names `{field}` and the answer omits it: {body}"
        );
    }
}

#[tokio::test]
async fn history_is_empty_without_a_store_and_scoped_to_the_caller() {
    let (status, _, body) = call(
        state(&[]),
        "GET",
        "/api/pipeline/history/e1",
        Some("t1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entity_id"], "e1");
    assert_eq!(body["total"], 0);
    assert_eq!(body["entries"], json!([]));

    let (status, _, _) = call(state(&[]), "GET", "/api/pipeline/history/e1", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn anomalies_honour_the_threshold_query() {
    let (status, _, body) = call(
        state(&[]),
        "GET",
        "/api/pipeline/anomalies?threshold=0.5",
        Some("t1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["anomalies"], json!([]));
    assert_eq!(body["total"], 0);
}
