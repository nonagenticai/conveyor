//! The delivery-proof certificate: RLS, append-only grants and cross-tenant invisibility.
//!
//! `cargo test -p conveyor-store --test proof_rls -- --nocapture`.
//!
//! The vantage points are the ones `s07_schema.rs` established: the superuser writes fixtures
//! (it bypasses RLS, the positive control), and every assertion about visibility runs as
//! `conveyor_runtime` in a scoped transaction, which is the production read path.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::{begin_scoped, proof};
use uuid::Uuid;

/// 64 hex characters, as every digest check on the table requires.
fn digest(seed: char) -> String {
    seed.to_string().repeat(64)
}

/// Insert one certificate for the tenant as the superuser, bypassing RLS.
async fn seed_proof(pool: &sqlx::PgPool, tenant: Uuid, subject: Uuid, seed: char) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO proof (tenant_id, subject_type, subject_id, spec_name, spec_sha256,
                            facts_canonical, facts_sha256, lean_source, theorem, trust, verdict,
                            computation_time_ms, idempotency_key, certificate_sha256)
         VALUES ($1, 'card', $2, 'spec.md', $3,
                 '{\"cards\":[]}', $3, 'theorem delivery : True := by trivial', 'delivery',
                 'kernel', 'proven', 12.5, $4, $5)
         RETURNING id",
    )
    .bind(tenant)
    .bind(subject)
    .bind(digest('a'))
    .bind(format!("invocation-{seed}"))
    .bind(digest(seed))
    .fetch_one(pool)
    .await
    .expect("seed proof")
}

#[tokio::test]
async fn the_proof_table_forces_rls_and_withholds_update_and_delete() {
    let db = support::TestDb::fresh().await;

    let (enabled, forced): (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE relname = 'proof'",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("pg_class row-security flags");
    assert!(enabled, "proof: ENABLE ROW LEVEL SECURITY is missing");
    assert!(forced, "proof: FORCE ROW LEVEL SECURITY is missing");

    let (can_update, can_delete): (bool, bool) = sqlx::query_as(
        "SELECT has_table_privilege('conveyor_runtime', 'proof', 'UPDATE'),
                has_table_privilege('conveyor_runtime', 'proof', 'DELETE')",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("runtime privileges");
    assert!(
        !can_update,
        "a certificate is written once; conveyor_runtime must not own UPDATE"
    );
    assert!(
        !can_delete,
        "a certificate is evidence; conveyor_runtime must not own DELETE"
    );
}

#[tokio::test]
async fn tenant_b_cannot_see_tenant_a_proof() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "proof-rls-a").await;
    let b = support::seed_tenant(&db.super_pool, "proof-rls-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil");
    let tenant_b = TenantId::from_uuid(b.tenant).expect("non-nil");

    let proof_a = seed_proof(&db.super_pool, a.tenant, a.card, 'a').await;
    let proof_b = seed_proof(&db.super_pool, b.tenant, b.card, 'b').await;

    // A's scope sees exactly A's certificate, and cannot fetch B's by id.
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("scope a transaction to A");
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM proof")
        .fetch_one(&mut *tx)
        .await
        .expect("count under A");
    assert_eq!(visible, 1, "A must see exactly its own certificate");
    assert!(
        proof::get_tx(&mut tx, tenant_a, proof_a)
            .await
            .expect("read A's proof")
            .is_some(),
        "A must read its own certificate"
    );
    assert!(
        proof::get_tx(&mut tx, tenant_a, proof_b)
            .await
            .expect("read B's proof")
            .is_none(),
        "B's certificate must not resolve in A's scope"
    );
    tx.rollback().await.expect("roll back the read scope");

    // B's scope is the mirror image.
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("scope a transaction to B");
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM proof")
        .fetch_one(&mut *tx)
        .await
        .expect("count under B");
    assert_eq!(visible, 1, "B must see exactly its own certificate");
    assert!(
        proof::get_tx(&mut tx, tenant_b, proof_a)
            .await
            .expect("read A's proof from B")
            .is_none(),
        "A's certificate must not resolve in B's scope"
    );
    tx.rollback().await.expect("roll back the read scope");
}
