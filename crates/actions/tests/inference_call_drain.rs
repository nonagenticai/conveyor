//! #412 — a metered external call survives the cancellation of the action that made it.
//!
//! A cancelled run drops the invoke future. When the request is a metered model call, the
//! provider has already prefilled the prompt and the router books a zero cost event for an
//! attempt whose client leaves — so the request must still reach its last byte.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::json;

/// What the fake upstream recorded about its own life.
#[derive(Default)]
struct Tail {
    /// The request arrived: the action reached the HTTP call.
    started: AtomicBool,
    /// The last chunk was written: the reader stayed to the end.
    finished: AtomicBool,
    /// The body was dropped before its last chunk: the reader left.
    dropped: AtomicBool,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_metered_call_is_read_to_completion() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let tail = Arc::new(Tail::default());
    let state = tail.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move || {
            let seen = state.clone();
            seen.started.store(true, Ordering::SeqCst);
            async move {
                let (tx, rx) =
                    tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);
                tokio::spawn(async move {
                    for i in 0..6 {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        // A closed receiver is the reader leaving: the body was dropped before
                        // its end, which is exactly what the drain must prevent.
                        if tx.send(Ok(bytes::Bytes::from_static(b"{}"))).await.is_err() {
                            seen.dropped.store(true, Ordering::SeqCst);
                            return;
                        }
                        if i + 1 == 6 {
                            seen.finished.store(true, Ordering::SeqCst);
                        }
                    }
                });
                (
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);

    let request = InvokeRequest {
        action: "completions.create".to_owned(),
        input: json!({"messages": [{"role": "user", "content": "hello"}]}),
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-drain".to_owned()),
        trace_id: Some("trace-drain".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None,
    };
    let pool = pg.runtime.clone();
    let tenant = seeded.tenant;
    let task = tokio::spawn(async move { invoke(&pool, registry(), tenant, request).await });

    // The action is in flight; the run is cancelled and the future is dropped. The DB work
    // before the HTTP call varies, so wait for the upstream to see the request first.
    for _ in 0..200 {
        if tail.started.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        tail.started.load(Ordering::SeqCst),
        "the action must reach the upstream before the cancel"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    task.abort();
    tokio::time::sleep(Duration::from_millis(900)).await;

    assert!(
        tail.finished.load(Ordering::SeqCst),
        "the response must be read to its last chunk after the cancel"
    );
    assert!(
        !tail.dropped.load(Ordering::SeqCst),
        "the response must not be abandoned mid-body"
    );
}
