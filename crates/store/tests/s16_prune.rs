#![forbid(unsafe_code)]
//! S16 retention: the ledger prunes at 90 days and an audit at 180 days after its last finding is
//! decided; an audit with an open finding is never pruned.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::prune::{
    INFERENCE_CALL_RETENTION_DAYS, PROMPT_AUDIT_RETENTION_DAYS, PRUNE_BATCH, inference_call_prune,
    prompt_audit_prune,
};
use uuid::Uuid;

async fn tenant(db: &support::TestDb, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

#[tokio::test]
async fn the_ledger_prunes_at_ninety_days() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-prune-calls").await;
    sqlx::query(
        "INSERT INTO inference_call (tenant_id, caller_kind, model, created_at)
         VALUES ($1, 'other', 'm', now() - interval '100 days'),
                ($1, 'other', 'm', now())",
    )
    .bind(tenant.as_uuid())
    .execute(&db.super_pool)
    .await
    .expect("seed calls");

    let deleted = inference_call_prune(
        &db.runtime,
        tenant,
        INFERENCE_CALL_RETENTION_DAYS,
        PRUNE_BATCH,
    )
    .await
    .expect("prune");
    assert_eq!(deleted, 1);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    assert_eq!(left, 1, "the recent call survives");
}

#[tokio::test]
async fn an_audit_prunes_only_after_its_last_finding_is_decided() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-prune-audits").await;
    let decider = Uuid::now_v7();
    let decided: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt_audit (tenant_id, target_model, prompt_ref, status, updated_at)
         VALUES ($1, 'm', 'prompt_audit@1', 'succeeded', now() - interval '200 days')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed decided audit");
    sqlx::query(
        "INSERT INTO prompt_audit_finding
             (tenant_id, audit_id, location, evidence, pattern, why_obsolete, confidence, action,
              status, decided_by, decided_at)
         VALUES ($1, $2, 'process_step:x', 'TEXT', '1a', 'why', 'high', 'flag', 'accepted', $3,
                 now() - interval '200 days')",
    )
    .bind(tenant.as_uuid())
    .bind(decided)
    .bind(decider)
    .execute(&db.super_pool)
    .await
    .expect("seed decided finding");

    let open: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt_audit (tenant_id, target_model, prompt_ref, status, updated_at)
         VALUES ($1, 'm', 'prompt_audit@1', 'succeeded', now() - interval '200 days')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed open audit");
    sqlx::query(
        "INSERT INTO prompt_audit_finding
             (tenant_id, audit_id, location, evidence, pattern, why_obsolete, confidence, action)
         VALUES ($1, $2, 'process_step:y', 'TEXT', '1a', 'why', 'low', 'flag')",
    )
    .bind(tenant.as_uuid())
    .bind(open)
    .execute(&db.super_pool)
    .await
    .expect("seed open finding");

    let deleted = prompt_audit_prune(
        &db.runtime,
        tenant,
        PROMPT_AUDIT_RETENTION_DAYS,
        PRUNE_BATCH,
    )
    .await
    .expect("prune");
    assert_eq!(deleted, 1, "only the fully-decided old audit is pruned");
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM prompt_audit")
        .fetch_one(&db.super_pool)
        .await
        .expect("count audits");
    let findings: i64 = sqlx::query_scalar("SELECT count(*) FROM prompt_audit_finding")
        .fetch_one(&db.super_pool)
        .await
        .expect("count findings");
    assert_eq!(audits, 1, "the audit with an open finding stays");
    assert_eq!(findings, 1, "the pruned audit's finding went with it");
}
