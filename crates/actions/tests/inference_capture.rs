//! S16 — the engine's capture seam: one successful model call writes one ledger row, with the
//! router's own token counts, the four-way cache class, the prompt identity and the correlation
//! id the engine sent.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::routing::post;
use conveyor_actions::{InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use serde_json::{Value, json};
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
                    let actor = headers
                        .get("x-actor-id")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("missing")
                        .to_owned();
                    let project = headers
                        .get("x-project-id")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("missing")
                        .to_owned();
                    axum::Json(json!({
                        "id": "chatcmpl-test",
                        "model": "served-model",
                        "correlation": correlation,
                        "actor": actor,
                        "project": project,
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
        approval_id: None,
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
    assert_eq!(
        outcome.body["output"]["actor"],
        json!(seeded.user_id.to_string()),
        "the router records the acting principal from X-Actor-Id"
    );
    assert_eq!(
        outcome.body["output"]["project"],
        json!(conveyor_actions::invoke::tenant_level_project_id(seeded.tenant).to_string()),
        "the router records the tenant-level project id from X-Project-Id"
    );

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
        approval_id: None,
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

/// P1.4 — the completeness guarantee is an explicit amendment, not a retry loop.
///
/// A ledger write can fail while the paid model call has already succeeded. The contract is: the
/// call keeps its success, the failure is logged, and the paid call is **never retried** merely to
/// repair its accounting (S16 W7). Break only the ledger and prove all three.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_capture_failure_keeps_the_call_and_never_retries_the_paid_request() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = Router::new().route(
        "/api/chat/completions",
        post(move |axum::Json(_body): axum::Json<Value>| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                axum::Json(json!({
                    "id": "chatcmpl-capture-failure",
                    "model": "served-model",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 5,
                        "completion_tokens": 2,
                        "total_tokens": 7,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0
                    }
                }))
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

    // Only the capture target is missing: the invocation, audit and idempotency bookkeeping all
    // still have their tables, so a successful commit proves the capture rolled back to its own
    // savepoint instead of poisoning the caller's transaction.
    sqlx::query("ALTER TABLE inference_call RENAME TO inference_call_unavailable")
        .execute(&pg.super_pool)
        .await
        .expect("break only the ledger");

    let request = InvokeRequest {
        action: "completions.create".to_owned(),
        input: json!({
            "model": "alias-model",
            "messages": [{"role": "user", "content": "hello"}],
        }),
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-capture-failure".to_owned()),
        trace_id: None,
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: Some(UsageContext {
            caller_kind: CallerKind::ProcessStep,
            run_id: Some(Uuid::now_v7()),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("process:capture-failure".to_owned()),
            prompt_version: Some("1:1".to_owned()),
            correlation_id: Some("corr-capture-failure".to_owned()),
        }),
    };
    let outcome = invoke(&pg.runtime, registry(), seeded.tenant, request)
        .await
        .expect("a capture failure must not fail the paid call");
    assert_eq!(outcome.status, 200, "{}", outcome.body);

    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the paid model call is never retried to repair accounting"
    );
    let invocations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation WHERE action = 'completions.create'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("count invocations");
    assert_eq!(
        invocations, 1,
        "the action's own bookkeeping still commits after the capture rolled back"
    );
}
