//! S16 — the engine's capture seam: one successful model call writes one ledger row, with the
//! router's own token counts, the four-way cache class, the prompt identity and the correlation
//! id the engine sent.

mod support;

use conveyor_actions::{InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_external_call_records_the_router_usage_field_for_field() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // A stub that answers the router's path with a genuine `usage` block and echoes the
    // correlation id it received.
    let app =
        axum::Router::new().route(
            "/api/chat/completions",
            axum::routing::post(
                |headers: axum::http::HeaderMap,
                 axum::Json(_body): axum::Json<serde_json::Value>| async move {
                    let correlation = headers
                        .get("x-correlation-id")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("missing")
                        .to_owned();
                    axum::Json(json!({
                        "id": "chatcmpl-test",
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
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);

    let run_id = Uuid::now_v7();
    let step_id = Uuid::now_v7();
    let request = InvokeRequest {
        action: "completions.create".to_owned(),
        input: json!({
            "model": "alias-model",
            "messages": [{"role": "user", "content": "hello"}],
        }),
        precondition: None,
        idempotency_key: None,
        actor_id: seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-capture".to_owned()),
        trace_id: Some("trace-capture".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: Some(UsageContext {
            caller_kind: CallerKind::ProcessStep,
            run_id: Some(run_id),
            run_step_id: Some(step_id),
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("process:test".to_owned()),
            prompt_version: Some("1:2".to_owned()),
            correlation_id: Some("corr-1".to_owned()),
        }),
    };
    let outcome = invoke(&pg.runtime, registry(), seeded.tenant, request)
        .await
        .expect("the external call");
    assert_eq!(outcome.status, 200, "{}", outcome.body);

    #[derive(sqlx::FromRow)]
    struct LedgerRow {
        model: String,
        served_model: Option<String>,
        prompt_tokens: i32,
        completion_tokens: i32,
        total_tokens: i32,
        cache_read_input_tokens: i32,
        cache_status: String,
        caller_kind: String,
        correlation_id: Option<String>,
        prompt_ref: Option<String>,
        prompt_version: Option<String>,
        prompt_sha256: Option<String>,
        action_invocation_id: Option<Uuid>,
    }
    let row: LedgerRow = sqlx::query_as(
        "SELECT model, served_model, prompt_tokens, completion_tokens, total_tokens,
                cache_read_input_tokens, cache_status, caller_kind, correlation_id, prompt_ref,
                prompt_version, prompt_sha256, action_invocation_id
           FROM inference_call",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("the ledger row");
    assert_eq!(row.model, "alias-model", "the requested alias");
    assert_eq!(
        row.served_model.as_deref(),
        Some("served-model"),
        "the provider's answer"
    );
    assert_eq!(
        (
            row.prompt_tokens,
            row.completion_tokens,
            row.total_tokens,
            row.cache_read_input_tokens
        ),
        (11, 3, 14, 6),
        "the router's own usage, field for field"
    );
    assert_eq!(row.cache_status, "hit", "read > 0 is a hit");
    assert_eq!(row.caller_kind, "process_step");
    assert_eq!(
        row.correlation_id.as_deref(),
        Some("corr-1"),
        "X-Correlation-Id is stored"
    );
    assert_eq!(row.prompt_ref.as_deref(), Some("process:test"));
    assert_eq!(row.prompt_version.as_deref(), Some("1:2"));
    assert!(
        row.prompt_sha256
            .as_deref()
            .is_some_and(|sha| sha.len() == 64),
        "the prompt hash is stored, never the prompt"
    );
    assert!(
        row.action_invocation_id.is_some(),
        "the invocation id joins the row to its call"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_without_a_usage_context_writes_no_row() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // The same action, invoked without a usage context (an ordinary HTTP call): the engine
    // captures only what the caller named.
    let request = InvokeRequest {
        action: "completions.create".to_owned(),
        input: json!({"messages": [{"role": "user", "content": "hello"}]}),
        precondition: None,
        idempotency_key: None,
        actor_id: seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: None,
        trace_id: None,
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None,
    };
    let _ = invoke(&pg.runtime, registry(), seeded.tenant, request).await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count");
    assert_eq!(rows, 0);
}
