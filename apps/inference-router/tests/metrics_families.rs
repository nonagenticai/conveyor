//! The family-set gate: every one of the 15 alert-keyed families must be present at scrape after
//! the boot-time pre-seed, and the two label normalisers must be exactly three-way bounded.
//!
//! The bottom of the file drives real completions through the router and reads the exposition the
//! sidecar would scrape — the attempt trio is the alert surface, so a family that is merely
//! *present* is not evidence that it *moves*.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::metering::{CostEvent, CostSink};
use conveyor_inference_router::metrics::{FAMILIES, caller_label, preseed, router, worker_label};
use conveyor_inference_router::provider::BoxFuture;
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

// ── P2c: the two admission counters ─────────────────────────────────────────────────────────
//
// `requests_received_total` is label-free and the recorder is process-wide, and this binary runs
// its tests in parallel, so a test can prove that its own request MOVED the counter but never that
// it moved it by exactly one. The project-id family carries a caller label, so each test below
// takes its own caller and its series are asserted exactly.

fn requests_received(body: &str) -> f64 {
    series_value(body, "inference_router_requests_received_total", &[])
        .expect("the requests family is pre-seeded at zero")
}

fn project_series(body: &str, outcome: &str, caller: &str) -> Option<f64> {
    series_value(
        body,
        "inference_router_project_id_header_total",
        &[("outcome", outcome), ("caller", caller)],
    )
}

/// The same one-provider router as [`configured`], with the licence gate in `enforce`: a caller
/// carrying no verifiable token is refused before the handler's admission counter can run.
fn enforcing(provider: &str, address: std::net::SocketAddr) -> Router {
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: {provider}\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: metric-model\ndefault_chain: [{provider}]\n"
    ))
    .expect("catalog");
    preseed(&[]);
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let config = Config::from_lookup(|key| {
        (key == "INFERENCE_ROUTER_INTROSPECT_MODE").then(|| "enforce".to_owned())
    })
    .expect("config");
    api_router(AppState::new(config, router))
}

#[tokio::test]
async fn an_unparseable_body_is_counted_before_it_is_rejected() {
    let address = stub_provider("never dialled").await;
    let router = configured("stub-metrics-422", address);
    let before = requests_received(&scrape().await);

    let response = router
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"metric-model""#))
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let after = requests_received(&scrape().await);
    assert!(
        after > before,
        "a 422 is counted by the route layer, before the body is parsed: {before} -> {after}"
    );
}

#[tokio::test]
async fn a_licence_refusal_is_counted_but_its_project_id_is_not() {
    let address = stub_provider("never dialled").await;
    let router = enforcing("stub-metrics-401", address);
    let before = requests_received(&scrape().await);

    let response = router
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .header("x-app-id", "care-bus")
                .header("x-project-id", "7d9c2b6e-0f3a-4a57-9c1e-2b6a1f0e4d55")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = scrape().await;
    let after = requests_received(&body);
    assert!(
        after > before,
        "a licence-refused request reached the completion route: {before} -> {after}"
    );
    for outcome in ["absent", "valid", "malformed"] {
        assert_eq!(
            project_series(&body, outcome, "care-bus"),
            Some(0.0),
            "the reference resolves X-Project-Id after its auth dependency, so a 401 is never \
             counted ({outcome})"
        );
    }
}

#[tokio::test]
async fn the_project_id_header_is_classified_absent_valid_or_malformed() {
    let address = stub_provider("project id").await;
    let router = configured("stub-metrics-project", address);

    async fn post(
        router: Router,
        caller: &'static str,
        project: Option<&'static str>,
    ) -> StatusCode {
        let mut request = Request::post("/api/chat/completions")
            .header("content-type", "application/json")
            .header("x-app-id", caller);
        if let Some(project) = project {
            request = request.header("x-project-id", project);
        }
        router
            .oneshot(request.body(completion()).expect("request"))
            .await
            .expect("router answers")
            .status()
    }

    assert_eq!(post(router.clone(), "karma", None).await, StatusCode::OK);
    assert_eq!(
        post(
            router.clone(),
            "canary",
            Some("7d9c2b6e-0f3a-4a57-9c1e-2b6a1f0e4d55")
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        post(router, "replay", Some("not-a-uuid")).await,
        StatusCode::OK
    );

    let body = scrape().await;
    assert_eq!(
        project_series(&body, "absent", "karma"),
        Some(1.0),
        "a caller that sent no X-Project-Id is counted absent"
    );
    assert_eq!(
        project_series(&body, "valid", "canary"),
        Some(1.0),
        "a UUID is counted valid and stored on the row"
    );
    assert_eq!(
        project_series(&body, "malformed", "replay"),
        Some(1.0),
        "a present non-UUID is counted malformed"
    );
    assert_eq!(
        project_series(&body, "absent", "replay"),
        Some(0.0),
        "a malformed header must NOT be counted absent"
    );
    assert_eq!(project_series(&body, "valid", "replay"), Some(0.0));
}

// ── P2d: the cost-write token gate ──────────────────────────────────────────────────────────
//
// `tokens_total` is booked only after the XADD reports success, while the unlicensed-cost trio is
// booked BEFORE it. A scripted sink stands in for Redis so both sides of the gate are provable,
// and each test takes its own provider name because the recorder is process-wide.

/// A cost sink with a fixed delivery verdict, standing in for the Redis XADD.
struct ScriptedSink {
    delivered: bool,
}

impl CostSink for ScriptedSink {
    fn emit<'a>(&'a self, _event: &'a CostEvent) -> BoxFuture<'a, bool> {
        Box::pin(async move { self.delivered })
    }
}

/// The same one-provider router as [`configured`], with the cost sink injected and the provider
/// pre-seeded so its token series exists at zero before any traffic reaches it.
fn configured_with_cost(
    provider: &str,
    address: std::net::SocketAddr,
    cost: Arc<dyn CostSink>,
) -> Router {
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: {provider}\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: metric-model\ndefault_chain: [{provider}]\n"
    ))
    .expect("catalog");
    preseed(&[provider.to_owned()]);
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let state = AppState::with_deps(
        Config::from_lookup(|_| None).expect("defaults"),
        router,
        AppDeps {
            cost,
            ..AppDeps::default()
        },
    );
    api_router(state)
}

async fn serve_one(router: Router) -> StatusCode {
    router
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers")
        .status()
}

#[tokio::test]
async fn an_undelivered_cost_event_leaves_the_token_families_at_zero() {
    let address = stub_provider("undelivered").await;
    let router = configured_with_cost(
        "stub-metrics-undelivered",
        address,
        Arc::new(ScriptedSink { delivered: false }),
    );
    assert_eq!(serve_one(router).await, StatusCode::OK);

    let body = scrape().await;
    let labels = [
        ("service_key", "inference-router"),
        ("provider", "stub-metrics-undelivered"),
    ];
    for kind in ["prompt", "completion"] {
        let mut series = labels.to_vec();
        series.push(("kind", kind));
        assert_eq!(
            series_value(&body, "inference_router_tokens_total", &series),
            Some(0.0),
            "the sink refused delivery, so {kind} tokens must not be booked"
        );
    }
    // The trio is booked BEFORE the XADD: the spend happened whether or not the write landed.
    assert_eq!(
        series_value(
            &body,
            "inference_router_cost_events_unlicensed_total",
            &[
                ("service_key", "inference-router"),
                ("provider", "stub-metrics-undelivered"),
                ("caller", "unknown"),
                ("worker", "unknown"),
            ],
        ),
        Some(1.0),
        "an unlicensed attempt is counted even when the stream write fails"
    );
    assert_eq!(
        series_value(
            &body,
            "inference_router_cost_tokens_unlicensed_total",
            &[
                ("service_key", "inference-router"),
                ("provider", "stub-metrics-undelivered"),
                ("caller", "unknown"),
                ("worker", "unknown"),
            ],
        ),
        Some(10.0),
        "the unlicensed token total is 7 prompt + 3 completion"
    );
}

#[tokio::test]
async fn a_delivered_cost_event_books_the_attempt_tokens() {
    let address = stub_provider("delivered").await;
    let router = configured_with_cost(
        "stub-metrics-delivered",
        address,
        Arc::new(ScriptedSink { delivered: true }),
    );
    assert_eq!(serve_one(router).await, StatusCode::OK);

    let body = scrape().await;
    for (kind, expected) in [("prompt", 7.0), ("completion", 3.0)] {
        assert_eq!(
            series_value(
                &body,
                "inference_router_tokens_total",
                &[
                    ("service_key", "inference-router"),
                    ("provider", "stub-metrics-delivered"),
                    ("kind", kind),
                ],
            ),
            Some(expected),
            "a delivered event books its {kind} tokens after the XADD returns"
        );
    }
}

#[tokio::test]
async fn the_v1_alias_is_served_and_counted_like_the_api_path() {
    let address = stub_provider("alias completion").await;
    let router = configured("stub-metrics-alias", address);
    let before = requests_received(&scrape().await);

    let response = router
        .oneshot(
            Request::post("/v1/chat/completions")
                .header("content-type", "application/json")
                .header("x-app-id", "agent")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let body = scrape().await;
    let after = requests_received(&body);
    assert!(
        after > before,
        "the /v1 alias sits on the same counting sub-router as the api path: {before} -> {after}"
    );
    assert_eq!(
        series_value(
            &body,
            "inference_router_provider_attempts_total",
            &[
                ("provider", "stub-metrics-alias"),
                ("path", "routed"),
                ("outcome", "ok"),
                ("caller", "agent"),
            ],
        ),
        Some(1.0),
        "the alias dispatches through the same handler"
    );
}

// ── P2e: the effective-tenant family ────────────────────────────────────────────────────────
//
// The family is booked inside the resolver, once per request, and its `outcome` label is the
// reference's closed set. The recorder is process-wide and this binary runs in parallel, so this
// test proves its own requests MOVED a series, never an exact count. The other three outcomes are
// classified by `effective_tenant`'s unit tests in `src/auth.rs`.

#[tokio::test]
async fn the_effective_tenant_family_moves_for_absent_and_self() {
    let address = stub_provider("effective tenant").await;
    let router = configured("stub-metrics-tenant", address);

    let before = scrape().await;
    let absent_before = series_value(
        &before,
        "inference_router_effective_tenant_total",
        &[("outcome", "absent")],
    )
    .expect("the family is pre-seeded at zero");
    let self_before = series_value(
        &before,
        "inference_router_effective_tenant_total",
        &[("outcome", "self")],
    )
    .expect("the family is pre-seeded at zero");

    // No X-Tenant-Id: there is no caller tenant to resolve from.
    let response = router
        .clone()
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    // X-Tenant-Id names the caller's own tenant; the asserted tenant is that same tenant.
    let response = router
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .header("x-tenant-id", "0195f48b-d000-7000-8000-000000000001")
                .header(
                    "x-effective-tenant-id",
                    "0195f48b-d000-7000-8000-000000000001",
                )
                .body(completion())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let body = scrape().await;
    let absent_after = series_value(
        &body,
        "inference_router_effective_tenant_total",
        &[("outcome", "absent")],
    )
    .expect("the family is pre-seeded at zero");
    let self_after = series_value(
        &body,
        "inference_router_effective_tenant_total",
        &[("outcome", "self")],
    )
    .expect("the family is pre-seeded at zero");
    assert!(
        absent_after > absent_before,
        "a request with no X-Tenant-Id books absent: {absent_before} -> {absent_after}"
    );
    assert!(
        self_after > self_before,
        "a request asserting its own tenant books self: {self_before} -> {self_after}"
    );
}
