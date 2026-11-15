//! The dedicated metrics listener: the config field and the scrape endpoint in isolation.
//!
//! No database and no catalog: this suite is about the port, not the API.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use conveyor_inference_router::config::{Config, ConfigError};
use tower::ServiceExt;

fn parse(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    Config::from_lookup(|key| map.get(key).cloned())
}

#[test]
fn the_metrics_port_is_parsed_and_disabled_by_default() {
    assert_eq!(parse(&[]).expect("defaults").metrics_port, 0);
    assert_eq!(
        parse(&[("INFERENCE_ROUTER_METRICS_PORT", "9150")])
            .expect("port parses")
            .metrics_port,
        9150
    );
    assert_eq!(
        parse(&[("INFERENCE_ROUTER_METRICS_PORT", "0")])
            .expect("zero is the explicit off switch")
            .metrics_port,
        0
    );
    assert_eq!(
        parse(&[("INFERENCE_ROUTER_METRICS_PORT", "notaport")]),
        Err(ConfigError::InvalidMetricsPort("notaport".to_owned()))
    );
}

#[tokio::test]
async fn the_metrics_router_serves_the_prometheus_text_format() {
    // Building the router installs the process-wide recorder. An exporter with no recorded
    // family renders an empty body, so one test-only sample makes the format deterministic.
    let router = conveyor_inference_router::metrics::router();
    metrics::counter!("inference_router_test_samples_total").increment(1);

    let response = router
        .oneshot(
            Request::get("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("the metrics router answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; version=0.0.4; charset=utf-8")
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let text = String::from_utf8(bytes.to_vec()).expect("utf-8 exposition");
    assert!(
        text.contains("# "),
        "the scrape body is the Prometheus text format: {text:?}"
    );
}
