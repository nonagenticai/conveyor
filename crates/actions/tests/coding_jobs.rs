//! The two `coding_jobs.*` externals: the caller's tenant on the wire, and the caller's body
//! passed through verbatim (S09 §5.1/§5.3, build-order item 11).
//!
//! This lives in its own test binary because it sets `CONVEYOR_EXTERNAL_CODING_URL`: `set_var` is
//! process-wide and unsafe in edition 2024, and a second env-setting test in the same binary would
//! race `external_actions.rs`'s fixtures URL. One binary, one env key, one runtime.

mod support;

use std::sync::Arc;

use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_coding_job_carries_the_callers_tenant_and_verbatim_body() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // A stub that records the coding call's headers and body. The engine passes the input
    // verbatim, so the body it receives is the caller's create document unchanged.
    type Seen = Arc<tokio::sync::Mutex<Option<(axum::http::HeaderMap, serde_json::Value)>>>;
    let seen: Seen = Arc::new(tokio::sync::Mutex::new(None));
    let app = axum::Router::new().fallback({
        let seen = Arc::clone(&seen);
        move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<serde_json::Value>| {
            let seen = Arc::clone(&seen);
            async move {
                *seen.lock().await = Some((headers, body));
                axum::Json(json!({"id": "job-1", "status": "queued"}))
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let addr = listener.local_addr().expect("stub addr");
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    support::set_env("CONVEYOR_EXTERNAL_CODING_URL", &format!("http://{addr}"));

    let running = support::Running::start(&pg).await;
    let body = json!({
        "kind": "test",
        "input": {"source": {"kind": "inline", "files": [{"path": "src/lib.rs", "text": ""}]}},
        "timeout_seconds": 120,
    });
    let (status, response) = running
        .invoke(
            "coding_jobs.create",
            "dev:alice",
            body.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(
        response["output"],
        json!({"id": "job-1", "status": "queued"})
    );

    let expected = seeded.tenant.to_string();
    let (headers, sent) = seen.lock().await.clone().expect("the stub was called");
    assert_eq!(
        headers
            .get(conveyor_actions::invoke::TENANT_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str()),
        "the coding service sees the caller's tenant"
    );
    assert!(
        headers.get("x-actor-id").is_none(),
        "only model calls carry the billing actor: a coding job is not billed to a principal"
    );
    assert!(
        headers.get("x-project-id").is_none(),
        "only model calls carry the billing project"
    );
    assert_eq!(sent, body, "an external input is passed through verbatim");

    running.shutdown().await;
    task.abort();
}
