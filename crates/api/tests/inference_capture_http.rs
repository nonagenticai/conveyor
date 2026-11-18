//! S16 W7 — an ordinary HTTP call and an ordinary MCP call both reach the ledger.
//!
//! The two transport entry points used to pass `usage: None`, so a model call made through
//! either escaped `inference_call` entirely (W7's confirmed gap). They now supply the one
//! caller kind the schema allows to carry no join key (`other`); this test proves the row is
//! written through the real served router, not by calling the engine seam directly.
//!
//! One test per binary on purpose: `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global, so a
//! second concurrent test in this file would race the first one's stub (the same rule
//! `agent_budget.rs` states).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use axum::Router;
use axum::routing::post;
use serde_json::{Value, json};

/// The router's answer: a genuine OpenAI-compatible usage block plus the correlation id the
/// engine sent, so the test can join what the model saw to what the ledger stored.
fn stub() -> Router {
    Router::new().route(
        "/api/chat/completions",
        post(
            |headers: axum::http::HeaderMap, axum::Json(_body): axum::Json<Value>| async move {
                let correlation = headers
                    .get("x-correlation-id")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("missing")
                    .to_owned();
                axum::Json(json!({
                    "id": "chatcmpl-http-mcp",
                    "model": "served-model",
                    "correlation": correlation,
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 11,
                        "completion_tokens": 3,
                        "total_tokens": 14,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 6
                    }
                }))
            },
        ),
    )
}

#[derive(Debug, sqlx::FromRow)]
struct LedgerRow {
    caller_kind: String,
    correlation_id: Option<String>,
    prompt_tokens: i32,
    total_tokens: i32,
    action_invocation_id: Option<uuid::Uuid>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_and_mcp_calls_each_write_one_ledger_row() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, stub()).await;
    });
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);

    let running = support::Running::start(&pg).await;
    let input = json!({
        "model": "alias-model",
        "messages": [{"role": "user", "content": "hello"}],
    });

    // ── HTTP: the first row is the ordinary transport ──────────────────────────────────────
    let (status, body) = running
        .invoke(
            "completions.create",
            "dev:alice",
            input.clone(),
            None,
            None,
            Some("rid-http"),
        )
        .await;
    assert_eq!(status, 200, "HTTP completions.create: {body}");

    // ── MCP: the second row comes through tools/call, the same engine seam ────────────────
    let mut mcp = support::Mcp::connect(&running, "dev:alice").await;
    let answer = mcp.call("completions_create", input, None).await;
    assert!(
        !answer
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "MCP completions_create: {answer}"
    );

    let rows: Vec<LedgerRow> = sqlx::query_as(
        "SELECT caller_kind, correlation_id, prompt_tokens, total_tokens, action_invocation_id
           FROM inference_call
          ORDER BY created_at",
    )
    .fetch_all(&pg.super_pool)
    .await
    .expect("the ledger rows");

    assert_eq!(rows.len(), 2, "one row per transport call");
    assert!(
        rows.iter().all(|row| row.caller_kind == "other"),
        "an ordinary transport call is `other`: {rows:?}",
    );
    assert!(
        rows.iter().all(|row| row.action_invocation_id.is_some()),
        "each row joins its invocation: {rows:?}",
    );
    assert!(
        rows.iter()
            .all(|row| (row.prompt_tokens, row.total_tokens) == (11, 14)),
        "the router's own counts: {rows:?}",
    );
    assert!(
        rows.iter()
            .any(|row| row.correlation_id.as_deref() == Some("rid-http")),
        "the HTTP request id is the transport correlation: {rows:?}",
    );
}
