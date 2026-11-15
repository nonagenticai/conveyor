//! `GET /metrics` — the dedicated Prometheus scrape listener.
//!
//! The scrape is deliberately NOT on the API listener: the API document is the captured contract
//! (`contract/openapi.lock.json`), and a scrape endpoint is not a documented operation. The
//! listener is enabled by `INFERENCE_ROUTER_METRICS_PORT`; `0` (the default) means off.

use std::sync::OnceLock;

use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
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

/// The metrics listener's router: only `GET /metrics`, nothing else is reachable on this port.
///
/// Building the router installs the recorder, so families recorded after boot reach the scrape.
pub fn router() -> Router {
    handle();
    Router::new().route("/metrics", get(metrics))
}
