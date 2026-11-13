//! Every `/api/internal/*` route is guarded, through the real router.
//!
//! The unit tests in `src/internal.rs` prove the guard's decision table; this file proves the
//! WIRING — that a route added or copied without calling the guard would fail here.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::AppState;
use tower::ServiceExt;

const SECRET: &str = "stub-internal-secret";

const ROUTES: [&str; 6] = [
    "/api/internal/inference-calls",
    "/api/internal/inference-calls/pinning-summary",
    "/api/internal/inference-calls/repeat-groups",
    "/api/internal/inference-calls/summary",
    "/api/internal/inference-calls/1/capture",
    "/api/internal/served-models/timeline",
];

fn state(secret: Option<&str>) -> AppState {
    let secret = secret.map(str::to_owned);
    let config = Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_INTERNAL_SECRET" => secret.clone(),
        _ => None,
    })
    .expect("config");
    let catalog = Catalog::from_yaml(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://127.0.0.1:9/v1\n    model: stub-model\ndefault_chain: [stub-openai]\n",
    )
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    AppState::new(config, router)
}

async fn get(state: AppState, path: &str, headers: &[(&str, &str)]) -> StatusCode {
    let mut request = Request::get(path);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    conveyor_inference_router::router::router(state)
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("router answers")
        .status()
}

#[tokio::test]
async fn every_internal_route_refuses_a_missing_or_wrong_secret() {
    for path in ROUTES {
        assert_eq!(
            get(state(Some(SECRET)), path, &[]).await,
            StatusCode::UNAUTHORIZED,
            "{path} without a secret"
        );
        assert_eq!(
            get(state(Some(SECRET)), path, &[("x-internal-secret", "wrong")]).await,
            StatusCode::UNAUTHORIZED,
            "{path} with a wrong secret"
        );
    }
}

#[tokio::test]
async fn every_internal_route_fails_closed_when_the_secret_is_unconfigured() {
    for path in ROUTES {
        assert_eq!(
            get(state(None), path, &[("x-internal-secret", SECRET)]).await,
            StatusCode::SERVICE_UNAVAILABLE,
            "{path}"
        );
    }
}

#[tokio::test]
async fn every_internal_route_is_not_found_through_the_public_ingress() {
    for path in ROUTES {
        assert_eq!(
            get(
                state(Some(SECRET)),
                path,
                &[("host", "api.nonagentic.ai"), ("x-internal-secret", SECRET),],
            )
            .await,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[tokio::test]
async fn the_right_secret_reaches_the_handler() {
    // The handlers are still 501 stubs in this slice: reaching one is the proof the guard
    // passed. S8a.3 replaces the 501 with the read.
    for path in ROUTES {
        assert_eq!(
            get(state(Some(SECRET)), path, &[("x-internal-secret", SECRET)]).await,
            StatusCode::NOT_IMPLEMENTED,
            "{path}"
        );
    }
}
