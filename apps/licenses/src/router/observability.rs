//! `GET /metrics` — the Prometheus scrape endpoint.
//!
//! The live Python service exposes `/metrics` outside its OpenAPI schema; the router serves the
//! same path here. It is in the auth middleware's public set and is deliberately NOT in `ROUTES`
//! or the `OpenApi` document: `--check-lock` compares documented JSON operations, and a scrape
//! endpoint is neither.

use std::sync::OnceLock;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// The process-wide recorder. `install_recorder` may run once per process; tests that build
/// several routers reuse the same registry.
fn handle() -> &'static PrometheusHandle {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("installing the Prometheus recorder once per process")
    })
}

/// `GET /metrics`. Always 200: the scrape must answer even when the database does not.
pub async fn metrics() -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        handle().render(),
    )
        .into_response()
}
