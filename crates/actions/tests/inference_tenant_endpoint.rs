//! conveyor#160 P1: the executor resolves a tenant's **registered** inference destination before
//! the platform environment, and falls back to it when the tenant has not registered one.
//!
//! Three tenants, three stub servers: Alice and Bob each have a row pointing at their own stub,
//! Carol has none and must reach the platform `CONVEYOR_EXTERNAL_INFERENCE_URL`. The same tenant
//! then loses the platform value and must refuse exactly as before — a visible 503, never a guess.
//!
//! The stub answers with the `Host` it was reached on, which is what makes "each run reached its
//! own destination" a fact rather than a request count that a retry could fake.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde_json::{Value, json};

const INFERENCE_ENV: &str = "CONVEYOR_EXTERNAL_INFERENCE_URL";

/// A stub that records every call and echoes the authority it was reached on.
struct Marker {
    base: String,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Marker {
    async fn start() -> Marker {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .fallback(answer)
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the marker stub");
        let addr = listener.local_addr().expect("marker address");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Marker {
            base: format!("http://{addr}"),
            hits,
            task,
        }
    }

    /// The `Host` header a caller sends to this stub (`127.0.0.1:<port>`).
    fn authority(&self) -> &str {
        self.base.trim_start_matches("http://")
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(State(hits): State<Arc<AtomicUsize>>, headers: HeaderMap) -> Json<Value> {
    hits.fetch_add(1, Ordering::SeqCst);
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    Json(json!({"stub": true, "host": host}))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tenant_reaches_its_registered_destination_and_the_platform_env_is_the_fallback() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let _carol = support::seed(&pg, "dev-c", "carol", "Carol").await;

    let alice_stub = Marker::start().await;
    let bob_stub = Marker::start().await;
    let platform = Marker::start().await;
    support::set_env(INFERENCE_ENV, &platform.base);

    // The rows are written as the superuser: this test proves the READ path the executor uses.
    // The registration write path (console + audit + sealed credential) is P2's.
    for (tenant_id, base) in [
        (alice.tenant_id, &alice_stub.base),
        (bob.tenant_id, &bob_stub.base),
    ] {
        sqlx::query(
            "INSERT INTO tenant_inference_endpoint (tenant_id, destination_url) VALUES ($1, $2)",
        )
        .bind(tenant_id)
        .bind(base)
        .execute(&pg.super_pool)
        .await
        .expect("register the tenant destination");
    }

    let running = support::Running::start(&pg).await;
    let input = json!({"messages": [{"role": "user", "content": "hi"}]});

    // Alice: her row wins over the platform environment that is also set.
    let (status, body) = running
        .invoke(
            "completions.create",
            "dev:alice",
            input.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["host"], alice_stub.authority());
    assert_eq!(alice_stub.hits(), 1, "Alice's destination answered once");
    assert_eq!(bob_stub.hits(), 0, "Bob's destination was not called");
    assert_eq!(platform.hits(), 0, "the platform default was not called");

    // Bob: his own row, not Alice's.
    let (status, body) = running
        .invoke(
            "completions.create",
            "dev:bob",
            input.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["host"], bob_stub.authority());
    assert_eq!(bob_stub.hits(), 1);
    assert_eq!(
        alice_stub.hits(),
        1,
        "Alice's destination was not called again"
    );

    // Carol: no row, so the platform value applies (P1.2).
    let (status, body) = running
        .invoke(
            "completions.create",
            "dev:carol",
            input.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["host"], platform.authority());
    assert_eq!(platform.hits(), 1);

    // Carol again, now with no row and no platform value: the action refuses exactly as before.
    // SAFETY: the three invocations above have returned; the engine reads this key only inside a
    // call, and this test binary has no other test running (each test file is its own process).
    unsafe { std::env::remove_var(INFERENCE_ENV) };
    let (status, body) = running
        .invoke("completions.create", "dev:carol", input, None, None, None)
        .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["type"], "action-service-unconfigured");
}
