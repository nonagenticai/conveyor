//! #577 — a caller that disappears must not poison its idempotency key.
//!
//! An external claim is committed before the call so a concurrent replay sees it. Every failure
//! the engine *observes* releases it (`record_failure`), but a dropped future observes nothing:
//! the caller disconnected, the proxy gave up, the run was cancelled. Measured 2026-11-22 on the
//! deployed build — a render cut at 126 s left `moss-cat-demo-2026-11-22` at `status_code IS NULL`
//! hours after the caller was gone, and every retry answered `409 … still running` until the TTL.
//!
//! These two tests are the two halves of the rule: a cancelled call releases, and a completed one
//! keeps its row so the replay contract still holds.

mod support;

use std::time::Duration;

use conveyor_actions::{InvokeRequest, UsageContext, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use serde_json::json;
use uuid::Uuid;

/// A real 1×1 PNG: the object store's content-type check reads the magic bytes.
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// A host that accepts the request and never answers inside the test's patience.
async fn hanging_host() -> String {
    let app = axum::Router::new().route(
        "/v1/images/generations",
        axum::routing::post(|| async {
            tokio::time::sleep(Duration::from_secs(120)).await;
            axum::Json(json!({"data": []}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the hanging host");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

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

fn request(user_id: Uuid, key: &str) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: "media.generate_image".to_owned(),
        input: json!({"prompt": "a cat", "size": "512x512", "seed": 7, "steps": 4}),
        precondition: None,
        idempotency_key: Some(key.to_owned()),
        approval_id: None,
        actor_id: user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-claim".to_owned()),
        trace_id: Some("trace-claim".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        // The same attributable caller the render test uses: a render is a metered spend, and the
        // ledger row is what makes it one.
        usage: Some(UsageContext {
            caller_kind: CallerKind::AgentRun,
            run_id: Some(Uuid::now_v7()),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("agent:image".to_owned()),
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: Some("corr-claim".to_owned()),
        }),
    }
}

/// How many rows the tenant still holds for one key.
async fn rows_for(pg: &support::Pg, key: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM idempotency_key WHERE key = $1")
        .bind(key)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the idempotency rows")
}

/// P1.1 — the caller gives up; the claim does not outlive it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_call_releases_its_claim() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let base = hanging_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);
    let key = "cancelled-call";

    // The claim is an INSERT inside the future under test, so an observer proves it existed
    // BEFORE the caller walks away. Without this the release assertion below could pass on a row
    // that was never written, and the test would prove nothing.
    let observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watcher = {
        let pool = pg.super_pool.clone();
        let observed = observed.clone();
        let key = key.to_owned();
        tokio::spawn(async move {
            for _ in 0..100 {
                let rows: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM idempotency_key WHERE key = $1")
                        .bind(&key)
                        .fetch_one(&pool)
                        .await
                        .unwrap_or(0);
                if rows > 0 {
                    observed.store(true, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
    };

    let invocation = conveyor_actions::invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(seeded.user_id, key),
    );
    let abandoned = tokio::time::timeout(Duration::from_millis(400), invocation).await;
    assert!(
        abandoned.is_err(),
        "the caller gave up before the host answered"
    );
    watcher.await.expect("the observer task");
    assert!(
        observed.load(std::sync::atomic::Ordering::SeqCst),
        "the claim was never observed, so this test proves nothing about releasing it"
    );

    // The guard's release runs on a spawned task, so the assertion polls rather than sleeps once.
    let mut remaining = rows_for(&pg, key).await;
    for _ in 0..60 {
        if remaining == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        remaining = rows_for(&pg, key).await;
    }
    assert_eq!(
        remaining, 0,
        "a dropped invocation must release its claim, or every retry answers 409 until the TTL"
    );
}

/// P1.2 — a call that finishes keeps its row, so the replay contract is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_successful_call_still_replays() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let base = answering_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);
    let key = "completed-call";

    let first = conveyor_actions::invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(seeded.user_id, key),
    )
    .await
    .expect("the render");
    assert_eq!(first.status, 200, "{}", first.body);
    assert!(!first.replayed);

    let replay = conveyor_actions::invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(seeded.user_id, key),
    )
    .await
    .expect("the replay");
    assert!(replay.replayed, "the same key must replay, not re-render");
    assert_eq!(
        rows_for(&pg, key).await,
        1,
        "the settled row is the replay record and must survive"
    );
}
