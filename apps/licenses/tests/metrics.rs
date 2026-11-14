//! `/metrics` is public (the auth middleware exempts it) and serves Prometheus text.
//!
//! The endpoint has no documented operation, so it is not part of `ROUTES`; this test is the
//! proof that the scrape path exists at all.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use conveyor_licenses::router;
use tower::ServiceExt;

#[tokio::test]
async fn metrics_is_public_and_serves_prometheus_text() {
    let response = router::router(router::test_state())
        .oneshot(
            Request::get("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answers");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; version=0.0.4; charset=utf-8")
    );

    // The registry is process-wide and may already carry samples from another test; the
    // contract here is the 200 and the exposition content type, not a particular sample.
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let text = String::from_utf8(bytes.to_vec()).expect("utf-8 exposition");
    assert!(!text.contains("<!DOCTYPE"), "must not be an error page");
}
