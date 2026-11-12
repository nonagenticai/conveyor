//! S16 — `prompt_audit.run`: one bounded `completions.create` over the deterministic inventory,
//! findings persisted, and the audit's state advanced. The model here is a scripted HTTP stub;
//! the live-router rehearsal is documented in the slice evidence.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::prompt_audit_run;
use serde_json::json;
use uuid::Uuid;

fn set_router(base: &str) {
    // SAFETY: set before the first invocation in this binary; the engine reads it once per call.
    unsafe {
        std::env::set_var("CONVEYOR_EXTERNAL_INFERENCE_URL", base);
    }
}

async fn seed_audit(db: &common::TestDb) -> (TenantId, Uuid) {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('s16-run', 'S16') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let audit_id: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt_audit (tenant_id, scope, target_model, prompt_ref, status)
         VALUES ($1, $2, 'audit-model', 'prompt_audit@1', 'queued') RETURNING id",
    )
    .bind(tenant_id)
    .bind(json!({"surfaces": ["copilot"], "process_ids": []}))
    .fetch_one(&db.super_pool)
    .await
    .expect("seed audit");
    // SPEC D27: the audit's instruction is a catalogue row, so the fixture seeds the built-ins
    // exactly as `seed::dev` does for a real tenant.
    conveyor_store::seed::prompts_for_tenant(&db.super_pool, tenant_id)
        .await
        .expect("seed the prompt catalogue");
    (TenantId::from_uuid(tenant_id).expect("non-nil"), audit_id)
}

async fn stub(answer: String) -> String {
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move || {
            let answer = answer.clone();
            async move {
                axum::Json(json!({
                    "id": "chatcmpl-audit",
                    "model": "audit-model",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": answer},
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 20,
                        "completion_tokens": 10,
                        "total_tokens": 30,
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
    base
}

#[tokio::test]
async fn the_run_persists_findings_and_marks_the_audit_succeeded() {
    let db = common::TestDb::fresh().await;
    let (tenant, audit_id) = seed_audit(&db).await;
    let answer = json!({
        "findings": [{
            "location": "copilot:SYSTEM_PROMPT",
            "evidence": "Never invent a tool name",
            "pattern": "1a",
            "why_obsolete": "current models follow the plain instruction",
            "confidence": "medium",
            "action": "flag"
        }]
    })
    .to_string();
    let base = stub(answer).await;
    set_router(&base);

    prompt_audit_run::run(
        &db.runtime,
        tenant,
        "conveyor-test",
        &json!({"audit_id": audit_id}),
    )
    .await
    .expect("the audit run");

    let (status, count): (String, i32) =
        sqlx::query_as("SELECT status, finding_count FROM prompt_audit WHERE id = $1")
            .bind(audit_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read audit");
    assert_eq!((status.as_str(), count), ("succeeded", 1));
    let findings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM prompt_audit_finding WHERE audit_id = $1")
            .bind(audit_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read findings");
    assert_eq!(findings, 1);
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event WHERE type = 'prompt_audit.completed'")
            .fetch_one(&db.super_pool)
            .await
            .expect("count events");
    assert_eq!(events, 1, "the completion is announced");

    // A terminal audit is a no-op on retry.
    prompt_audit_run::run(
        &db.runtime,
        tenant,
        "conveyor-test",
        &json!({"audit_id": audit_id}),
    )
    .await
    .expect("terminal replay");
}

#[tokio::test]
async fn a_malformed_answer_fails_the_audit_with_the_error_recorded() {
    let db = common::TestDb::fresh().await;
    let (tenant, audit_id) = seed_audit(&db).await;
    let base = stub("this is not JSON".to_owned()).await;
    set_router(&base);

    let failure = prompt_audit_run::run(
        &db.runtime,
        tenant,
        "conveyor-test",
        &json!({"audit_id": audit_id}),
    )
    .await
    .expect_err("a malformed answer must fail the audit");
    assert!(failure.permanent, "{failure}");

    let (status, error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM prompt_audit WHERE id = $1")
            .bind(audit_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read audit");
    assert_eq!(status, "failed");
    assert!(
        error.unwrap_or_default().contains("not JSON"),
        "the parse error is recorded"
    );
}
