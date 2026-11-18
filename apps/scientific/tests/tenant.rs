//! conveyor#299 — the reference's tenant gate, restored (enforce mode).
//!
//! Every refusal has a positive control one input away, so no assertion can pass because the gate
//! is inert or because it refuses everything. The introspector is a process-local stub answering
//! by token prefix (`tests/support`), exercised through the real middleware.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt;

use conveyor_scientific::config::Config;
use conveyor_scientific::router;
use conveyor_scientific::state::AppState;

fn app() -> axum::Router {
    router::router(Arc::new(AppState::new(
        Config::from_lookup(&support::auth_vars()).expect("the environment parses"),
    )))
}

async fn get(headers: &[(&str, &str)]) -> (StatusCode, HeaderMap, serde_json::Value) {
    let mut builder = Request::builder().uri("/api/cache/stats");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, headers, value)
}

#[tokio::test]
async fn no_credential_is_refused() {
    let (status, headers, body) = get(&[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED", "{body}");
    assert_eq!(
        headers
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer")
    );
}

#[tokio::test]
async fn a_plain_tenant_is_refused_in_enforce() {
    let (status, _, body) = get(&[("x-tenant-id", "t1")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("AUTH_TOKENLESS_IDENTITY_MODE=enforce"),
        "the refusal names the mode: {body}"
    );

    // A malformed id is the 400, not the 401.
    let (status, _, body) = get(&[("x-tenant-id", "a/b")]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "INVALID_TENANT_ID", "{body}");
}

#[tokio::test]
async fn an_empty_bearer_is_malformed() {
    let (status, _, body) = get(&[("x-tenant-id", "Bearer")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "MALFORMED_CREDENTIAL", "{body}");
}

#[tokio::test]
async fn a_licence_in_the_tenant_header_names_the_tenant() {
    let (status, _, body) = get(&[("x-tenant-id", &support::licence_header("t1"))]).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // An inactive/unknown licence is the reference's 401.
    let (status, _, body) = get(&[("x-tenant-id", "Bearer expired-token")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED", "{body}");
}

#[tokio::test]
async fn an_authorization_licence_serves_its_tenant() {
    let (status, _, body) = get(&[("authorization", &support::licence_header("t1"))]).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the Authorization carrier resolves the licence's tenant: {body}"
    );
}

#[tokio::test]
async fn a_foreign_tenant_needs_a_vouch() {
    let licence = support::licence_header("t1");

    // A licence for t1 may not assert t2 without an allowlisted subject.
    let (status, _, body) = get(&[("authorization", &licence), ("x-tenant-id", "t2")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "TENANT_ASSERTION_NOT_PERMITTED");

    // The same shape, one input away: an allowlisted (on-behalf) subject may assert t2.
    let (status, _, body) = get(&[
        ("authorization", "Bearer onbehalf-own"),
        ("x-tenant-id", "t2"),
    ])
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A subject that is valid but not allowlisted is still refused.
    let (status, _, body) = get(&[
        ("authorization", "Bearer stranger-own"),
        ("x-tenant-id", "t2"),
    ])
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn introspection_without_a_verdict_is_a_503() {
    let (status, headers, body) = get(&[("authorization", "Bearer unavailable-token")]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        headers
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        Some("5")
    );
    assert_eq!(body["error"]["code"], "LICENSE_INTROSPECTION_UNAVAILABLE");
}

#[tokio::test]
async fn health_is_public() {
    let response = app()
        .oneshot(
            Request::get("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
}
