//! The served document is the captured lock, and every registered operation is
//! documented. The stronger response-shape statement is `tests/parity.rs`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use conveyor_research::contract;
use conveyor_research::router::{self, AppState, Config, ROUTES};

fn state(enable_docs: bool) -> AppState {
    AppState::new(Config {
        bind_host: "127.0.0.1".to_string(),
        bind_port: 0,
        enable_docs,
        ssrf_allowed_hosts: Vec::new(),
        service_name: "webscraper-service".to_string(),
        version: "1.0.0".to_string(),
    })
}

async fn get(app: axum::Router, path: &str) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[test]
fn the_served_document_equals_the_lock() {
    let served = router::openapi();
    let captured = contract::lock().expect("lock parses");
    assert_eq!(served, captured, "the served document is the capture");
}

#[test]
fn the_lock_names_six_operations_and_every_route_is_locked() {
    let served = router::openapi();
    assert_eq!(contract::operations(&served).len(), 6);
    assert_eq!(ROUTES.len(), 6);
    contract::check(&served, ROUTES).expect("registered == documented == captured");
}

#[test]
fn docs_routes_are_not_contract_operations() {
    let served = router::openapi();
    let operations = contract::operations(&served);
    assert!(
        !operations
            .iter()
            .any(|operation| operation.contains("/openapi.json"))
    );
    assert!(
        !operations
            .iter()
            .any(|operation| operation.contains("/docs"))
    );
}

#[tokio::test]
async fn enable_docs_serves_the_lock_and_a_docs_page() {
    let app = router::build_app(state(true));
    let (status, body) = get(app, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let served: serde_json::Value = serde_json::from_str(&body).expect("served json");
    assert_eq!(served, contract::lock().expect("lock parses"));

    let app = router::build_app(state(true));
    let (status, body) = get(app, "/docs").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("/openapi.json"));
}

#[tokio::test]
async fn docs_routes_are_absent_when_disabled() {
    for path in ["/openapi.json", "/docs"] {
        let app = router::build_app(state(false));
        let (status, _) = get(app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn the_six_operations_answer() {
    for &(method, path) in ROUTES {
        let app = router::build_app(state(true));
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .expect("request"),
            )
            .await
            .expect("response");
        // No operation answers 404: the route itself exists. A validation
        // failure is acceptable here; parity proves the captured bodies.
        assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}
