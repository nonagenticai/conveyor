//! #579 — the async invocation surface: submit, then poll.
//!
//! The estate's answer to Cloudflare's ~100 s first-byte cap is `202 + id + poll` (w2v for
//! images, agent for eval runs, mcp for runs, coding for jobs). These tests pin the half that
//! conveyor did not have: the submit cannot defer its own validation, the poll returns exactly
//! what the synchronous call would have, and another tenant cannot see any of it.

mod support;

use std::time::Duration;

use axum::http::Method;
use serde_json::{Value, json};

const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// A host that answers one render immediately.
async fn answering_host() -> String {
    let app = axum::Router::new().route(
        "/v1/images/generations",
        axum::routing::post(|| async {
            axum::Json(json!({
                "created": 1_700_000_000,
                "data": [{"b64_json": PNG_1X1, "mime_type": "image/png", "size": "512x512"}],
                "output_format": "png",
                "size": "512x512",
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the answering host");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

fn want() -> Value {
    json!({"input": {"prompt": "a cat", "size": "512x512", "seed": 7, "steps": 4}})
}

/// Poll until the invocation settles. The budget is generous because a spawned task only needs
/// to run the stub, not a render.
async fn poll_settled(running: &support::Running, token: &str, id: &str) -> (u16, Value) {
    for _ in 0..100 {
        let (status, _headers, body) = running
            .http_full(
                Method::GET,
                &format!("/api/invocations/{id}"),
                Some(token),
                &[],
                None,
            )
            .await;
        if status != 202 {
            return (status, body);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the invocation never settled");
}

/// P1.1 + P2.1 + P2.2 — submit answers 202 at once, and the poll answers the synchronous body.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_long_action_is_submitted_and_polled() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let base = answering_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);
    let running = support::Running::start(&pg).await;

    let (status, headers, body) = running
        .http_full(
            Method::POST,
            "/api/actions/media.generate_image/invocations",
            Some("dev:alice"),
            &[("prefer", "respond-async")],
            Some(want()),
        )
        .await;
    assert_eq!(status, 202, "the submit must not wait: {body}");
    let id = body["invocation_id"]
        .as_str()
        .expect("the submit names its invocation")
        .to_owned();
    assert_eq!(
        headers.get("location").map(String::as_str),
        Some(format!("/api/invocations/{id}").as_str()),
        "the caller is told where to poll"
    );

    let (status, settled) = poll_settled(&running, "dev:alice", &id).await;
    assert_eq!(status, 200, "{}", settled);
    assert!(
        settled["output"]["document"]["id"].is_string(),
        "the poll returns the same body the synchronous call would: {settled}"
    );
}

/// P1.2 — validation never becomes asynchronous.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invalid_input_fails_before_the_202() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let base = answering_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);
    let running = support::Running::start(&pg).await;

    let (status, _headers, body) = running
        .http_full(
            Method::POST,
            "/api/actions/media.generate_image/invocations",
            Some("dev:alice"),
            &[("prefer", "respond-async")],
            Some(json!({"input": {"prompt": "a cat", "size": "2048x2048"}})),
        )
        .await;
    assert_eq!(status, 422, "a bad input is refused, not queued: {body}");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM async_invocation")
        .fetch_one(&pg.super_pool)
        .await
        .expect("the invocation rows");
    assert_eq!(rows, 0, "a refused submit reserves nothing");
}

/// P1.3 — the same Idempotency-Key replays the first invocation instead of a second render.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_async_submit_replays_the_first_invocation() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let base = answering_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);
    let running = support::Running::start(&pg).await;

    let mut first = None;
    for _ in 0..2 {
        let (status, _headers, body) = running
            .http_full(
                Method::POST,
                "/api/actions/media.generate_image/invocations",
                Some("dev:alice"),
                &[
                    ("prefer", "respond-async"),
                    ("idempotency-key", "async-once"),
                ],
                Some(want()),
            )
            .await;
        assert_eq!(status, 202, "{body}");
        let id = body["invocation_id"].as_str().expect("an id").to_owned();
        match &first {
            None => first = Some(id),
            Some(expected) => assert_eq!(&id, expected, "the second submit replayed the first"),
        }
    }
}

/// P2.4 — another tenant cannot read the invocation, and cannot tell it exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_tenant_cannot_read_the_invocation() {
    let pg = support::Pg::fresh().await;
    let _a = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let _b = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let base = answering_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);
    let running = support::Running::start(&pg).await;

    let (status, _headers, body) = running
        .http_full(
            Method::POST,
            "/api/actions/media.generate_image/invocations",
            Some("dev:alice"),
            &[("prefer", "respond-async")],
            Some(want()),
        )
        .await;
    assert_eq!(status, 202, "{body}");
    let id = body["invocation_id"].as_str().expect("an id").to_owned();
    let _ = poll_settled(&running, "dev:alice", &id).await;

    let (status, _headers, body) = running
        .http_full(
            Method::GET,
            &format!("/api/invocations/{id}"),
            Some("dev:bob"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 404, "another tenant's record is invisible: {body}");
}
