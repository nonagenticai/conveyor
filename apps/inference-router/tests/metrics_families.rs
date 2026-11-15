//! The family-set gate: every one of the 15 alert-keyed families must be present at scrape after
//! the boot-time pre-seed, and the two label normalisers must be exactly three-way bounded.
//!
//! The bottom of the file drives real completions through the router and reads the exposition the
//! sidecar would scrape — the attempt trio is the alert surface, so a family that is merely
//! *present* is not evidence that it *moves*.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::metrics::{FAMILIES, caller_label, preseed, router, worker_label};
use conveyor_inference_router::router::router as api_router;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn every_family_is_rendered_at_zero_after_preseed() {
    let providers = vec!["fixture-alpha".to_string(), "fixture-beta".to_string()];
    preseed(&providers);

    let response = router()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .expect("metrics request"),
        )
        .await
        .expect("the metrics router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("metrics body");
    let body = String::from_utf8(bytes.to_vec()).expect("metrics body is UTF-8");

    assert_eq!(FAMILIES.len(), 15);
    let rendered: Vec<&str> = FAMILIES
        .iter()
        .copied()
        .filter(|family| body.contains(family))
        .collect();
    assert_eq!(
        rendered.len(),
        FAMILIES.len(),
        "families missing from the scrape: {:?}",
        FAMILIES
            .iter()
            .copied()
            .filter(|family| !body.contains(family))
            .collect::<Vec<_>>()
    );

    // The provider set is config-bounded and was actually used, at zero.
    for provider in &providers {
        let label = format!("provider=\"{provider}\"");
        assert!(
            body.contains(&label),
            "pre-seeded provider label {label} missing from the scrape"
        );
    }
    assert!(
        body.lines().any(
            |line| line.starts_with("inference_router_provider_attempts_total{")
                && line.contains("provider=\"fixture-alpha\"")
                && line.contains(" 0")
        ),
        "the attempts family must be present at zero for each provider"
    );

    // Nothing raw or blank may have been pre-seeded.
    assert!(!body.contains("provider=\"\""));
}

#[test]
fn caller_label_is_exactly_three_way() {
    assert_eq!(caller_label(None), "unknown");
    assert_eq!(caller_label(Some("")), "unknown");
    assert_eq!(caller_label(Some("   ")), "unknown");
    assert_eq!(caller_label(Some("w2v")), "w2v");
    assert_eq!(caller_label(Some("care-bus")), "care-bus");
    assert_eq!(caller_label(Some("chat-bridge-merqi")), "chat-bridge-merqi");
    assert_eq!(caller_label(Some("unknown")), "other");
    assert_eq!(caller_label(Some("not-a-registered-caller")), "other");
    assert_eq!(caller_label(Some("W2V")), "other");
}

#[test]
fn worker_label_is_exactly_three_way() {
    assert_eq!(worker_label(None), "unknown");
    assert_eq!(worker_label(Some("")), "unknown");
    assert_eq!(worker_label(Some("  \t ")), "unknown");
    assert_eq!(worker_label(Some("core-app")), "core-app");
    assert_eq!(
        worker_label(Some("requirement_quality_llm")),
        "requirement_quality_llm"
    );
    assert_eq!(worker_label(Some("unknown")), "other");
    assert_eq!(worker_label(Some("some-raw-worker-name")), "other");
}

// ── P2b: the attempt trio, driven end to end ────────────────────────────────────────────────
//
// The recorder is process-wide and the test binary runs its tests in parallel, so every test uses
// its own provider name. A shared name would make these assertions race with each other, which is
// the one way a metric test can pass without proving anything.

async fn stub_provider(content: &'static str) -> std::net::SocketAddr {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |axum::Json(_body): axum::Json<Value>| async move {
            axum::Json(json!({
                "id": "chatcmpl-metrics",
                "model": "metric-model",
                "served_model": "metric-model",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": content},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("stub serves") });
    address
}

async fn failing_provider() -> std::net::SocketAddr {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "upstream broke") }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("stub serves") });
    address
}

/// One provider, one chain, and the default (hermetic) dependencies: no ledger pool and no budget
/// gate, because this test is about the metric write path, not the row.
fn configured(provider: &str, address: std::net::SocketAddr) -> Router {
    // No `model_aliases`: a declared alias is an auto-pin in `route()`, and these tests are about
    // the chain path. The alias is what the `pinned` test opts into with `X-Provider` instead.
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: {provider}\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: metric-model\ndefault_chain: [{provider}]\n"
    ))
    .expect("catalog");
    // Boot pre-seeds before any traffic; the recorder must be installed before the first attempt
    // or the counter macro resolves to a no-op and the test passes over a series that never existed.
    preseed(&[]);
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let state = AppState::with_deps(
        Config::from_lookup(|_| None).expect("defaults"),
        router,
        AppDeps::default(),
    );
    api_router(state)
}

fn completion() -> Body {
    Body::from(
        json!({
            "model": "metric-model",
            "messages": [{"role": "user", "content": "metrics"}],
            "max_tokens": 8
        })
        .to_string(),
    )
}

async fn scrape() -> String {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .expect("metrics request"),
        )
        .await
        .expect("the metrics router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("metrics body");
    String::from_utf8(bytes.to_vec()).expect("metrics body is UTF-8")
}

/// The value of the single series belonging to `family` that carries every `key="value"` pair.
/// `None` means the series is ABSENT, which is a different statement from `0`.
fn series_value(body: &str, family: &str, labels: &[(&str, &str)]) -> Option<f64> {
    body.lines()
        .filter(|line| line.starts_with(family) && line.as_bytes().get(family.len()) != Some(&b'_'))
        .find(|line| {
            labels
                .iter()
                .all(|(key, value)| line.contains(&format!("{key}=\"{value}\"")))
        })
        .and_then(|line| line.rsplit(' ').next())
        .and_then(|value| value.parse().ok())
}

#[tokio::test]
async fn a_served_request_books_the_attempt_trio() {
    let address = stub_provider("metric completion").await;
    let response = configured("stub-metrics-ok", address)
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .header("x-app-id", "w2v")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let body = scrape().await;
    let labels = [
        ("provider", "stub-metrics-ok"),
        ("path", "routed"),
        ("outcome", "ok"),
        ("caller", "w2v"),
    ];
    assert_eq!(
        series_value(&body, "inference_router_provider_attempts_total", &labels),
        Some(1.0),
        "one attempt, routed, ok, labelled by the bounded caller"
    );
    let seconds = series_value(
        &body,
        "inference_router_provider_attempt_seconds_total",
        &labels,
    )
    .expect("the seconds series moves with the attempt");
    assert!(
        seconds > 0.0,
        "a served attempt spends wall clock: {seconds}"
    );
    assert!(
        series_value(
            &body,
            "inference_router_provider_errors_total",
            &[("provider", "stub-metrics-ok")]
        )
        .is_none(),
        "an ok attempt books no error reason"
    );
}

#[tokio::test]
async fn a_pinned_request_books_the_pinned_path() {
    let address = stub_provider("pinned completion").await;
    let response = configured("stub-metrics-pinned", address)
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .header("x-provider", "stub-metrics-pinned")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let body = scrape().await;
    assert_eq!(
        series_value(
            &body,
            "inference_router_provider_attempts_total",
            &[
                ("provider", "stub-metrics-pinned"),
                ("path", "pinned"),
                ("outcome", "ok"),
                ("caller", "unknown"),
            ],
        ),
        Some(1.0),
        "an X-Provider request is booked on the pinned path, not the routed one"
    );
}

#[tokio::test]
async fn a_failed_attempt_books_its_classification() {
    let address = failing_provider().await;
    let response = configured("stub-metrics-fail", address)
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = scrape().await;
    assert_eq!(
        series_value(
            &body,
            "inference_router_provider_attempts_total",
            &[
                ("provider", "stub-metrics-fail"),
                ("path", "routed"),
                ("outcome", "error"),
                ("caller", "unknown"),
            ],
        ),
        Some(1.0),
        "the failed attempt is counted before the chain gives up"
    );
    assert_eq!(
        series_value(
            &body,
            "inference_router_provider_errors_total",
            &[
                ("provider", "stub-metrics-fail"),
                ("path", "routed"),
                ("reason", "upstream_5xx"),
            ],
        ),
        Some(1.0),
        "the error series carries the attempt's own classification"
    );
}
