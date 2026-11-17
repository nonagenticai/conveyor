//! #170 P1 — the client's two measurable properties: it refuses without a credential and it
//! returns the API's objects unmodified.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::routing::get;
use conveyor_ops::{Client, OpsConfig, OpsError};
use serde_json::{Value, json};

/// Start a stub API server that answers one pod list and counts the requests it saw.
async fn stub_api() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = axum::Router::new().route(
        "/api/v1/namespaces/conveyor/pods",
        get(move |headers: axum::http::HeaderMap| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                let auth = headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_owned();
                if auth != "Bearer test-token" {
                    // A wrong credential must fail the test as a 401, not vanish in a task panic.
                    return (axum::http::StatusCode::UNAUTHORIZED, axum::Json(json!({})));
                }
                let body = json!({
                    "kind": "PodList",
                    "apiVersion": "v1",
                    "metadata": {"resourceVersion": "42"},
                    "items": [
                        {
                            "metadata": {"name": "conveyor-worker-abc", "namespace": "conveyor"},
                            "status": {"phase": "Running"}
                        }
                    ]
                });
                (axum::http::StatusCode::OK, axum::Json(body))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the API stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, hits)
}

/// P1.1 — no service-account token: a typed refusal, and no request leaves the process.
#[tokio::test]
async fn absent_credential_is_unconfigured() {
    // Nothing in the lookup at all.
    let error = OpsConfig::from_lookup(|_| None, |_| None).expect_err("no token means no config");
    assert!(matches!(error, OpsError::Unconfigured), "{error:?}");

    // An API address *is* present — the credential is what is missing, and the stub sees nothing.
    let (base, hits) = stub_api().await;
    let address = base.clone();
    let error = OpsConfig::from_lookup(
        move |key| (key == "OPS_API_URL").then(|| address.clone()),
        |_| None,
    )
    .expect_err("an address without a token is still unconfigured");
    assert!(matches!(error, OpsError::Unconfigured), "{error:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 0, "no request may be made");
}

/// P1.2 — the client returns exactly the objects the API returned.
#[tokio::test]
async fn pod_list_passes_through() {
    let (base, hits) = stub_api().await;
    let client =
        Client::new(OpsConfig::from_parts(base, "test-token")).expect("a configured client");

    let document: Value = client
        .get_json("/api/v1/namespaces/conveyor/pods", &[])
        .await
        .expect("the stub answers");

    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(document["kind"], json!("PodList"));
    assert_eq!(document["metadata"]["resourceVersion"], json!("42"));
    assert_eq!(
        document["items"][0]["metadata"]["name"],
        json!("conveyor-worker-abc")
    );
    assert_eq!(document["items"][0]["status"]["phase"], json!("Running"));
}

/// A non-success status is surfaced with the API's own code, never flattened to a generic error.
#[tokio::test]
async fn an_api_error_keeps_its_status() {
    let app = axum::Router::new().route(
        "/api/v1/namespaces/conveyor/pods",
        get(|| async {
            (
                axum::http::StatusCode::FORBIDDEN,
                axum::Json(json!({"kind": "Status", "message": "forbidden"})),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let client = Client::new(OpsConfig::from_parts(base, "test-token")).expect("client");
    let error = client
        .get_json("/api/v1/namespaces/conveyor/pods", &[])
        .await
        .expect_err("403 is not a success");
    assert_eq!(error.status(), Some(403));
}
