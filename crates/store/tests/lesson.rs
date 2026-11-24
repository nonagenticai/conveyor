//! T8 (#79) P1.3 — the database itself refuses a lesson with one independent reference.
//!
//! The action is not the only writer: a migration, a retro or a repair script can reach the
//! table directly. `0069_lesson_lifecycle.sql` carries `CHECK (independent_count <> 1)`, so the
//! same rule holds without the action layer. Zero (an execution/retro record awaiting its second
//! reference) and two or more (a reviewable proposal) are both accepted.
//!
//! `cargo test -p conveyor-store --test lesson`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use uuid::Uuid;

async fn tenant(db: &support::TestDb) -> TenantId {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ($1, 'Organisational memory') RETURNING id",
    )
    .bind(format!("lesson-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    TenantId::from_uuid(tenant_id).expect("tenant id")
}

#[tokio::test]
async fn the_database_refuses_a_lesson_with_one_independent_ref() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db).await;

    // P1.3: a single independent reference is refused by the table, with no action involved.
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let refused = sqlx::query(
        "INSERT INTO lesson (tenant_id, statement, evidence, independent_count, source, state)
         VALUES ($1, 'one bad run is not doctrine', $2, 1, 'propose', 'proposed')",
    )
    .bind(tenant.as_uuid())
    .bind(serde_json::json!([{"kind": "run", "id": Uuid::now_v7()}]))
    .execute(&mut *tx)
    .await;
    let error = refused.expect_err("a lesson with one independent reference must be refused");
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("expected a database refusal, got {error}"));
    assert_eq!(
        database.constraint(),
        Some("lesson_independent_count_not_one"),
        "the refusal is the independence CHECK, not something incidental: {database}"
    );
    tx.rollback().await.expect("roll back the refused insert");

    // No row was stored.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM lesson WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("count lessons");
    assert_eq!(count, 0, "the refused row is not stored");

    // Zero refs (execution/retro provenance) and two refs (a reviewable proposal) are accepted.
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO lesson (tenant_id, statement, evidence, independent_count, source, state)
         VALUES ($1, 'execution provenance', '[]'::jsonb, 0, 'execution', 'proposed')",
    )
    .bind(tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect("zero independent refs are allowed");
    sqlx::query(
        "INSERT INTO lesson (tenant_id, statement, evidence, independent_count, source, state)
         VALUES ($1, 'two independent refs', $2, 2, 'propose', 'proposed')",
    )
    .bind(tenant.as_uuid())
    .bind(serde_json::json!([
        {"kind": "card", "id": Uuid::now_v7()},
        {"kind": "run", "id": Uuid::now_v7()},
    ]))
    .execute(&mut *tx)
    .await
    .expect("two independent refs are allowed");
    tx.commit().await.expect("commit");

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM lesson WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("count lessons");
    assert_eq!(count, 2, "only the two legal rows exist");
}
