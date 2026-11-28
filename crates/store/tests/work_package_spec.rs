//! `work_package_spec`'s schema doors (SPEC_V2 §5.8, D69; conveyor#935).
//!
//! One spec per `(tenant, card)`: the write key, and the second door behind `cards.spec_put`.
//! The table is tenant-scoped and RLS-forced, and the parent card is resolved in the caller's
//! tenant before every write (CONTRACT §2), so a card id from another tenant is never accepted.
//! This test proves both doors on the real Postgres schema: the store refuses a foreign card as
//! its parent, RLS refuses a forged row for a foreign tenant, and `UNIQUE(tenant_id, card_id)`
//! refuses a second row directly.
//!
//! `cargo test -p conveyor-store --test work_package_spec`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::work_package_spec::{
    NewWorkPackageSpec, WorkPackageSpecStoreError, get_tx, upsert_tx,
};
use serde_json::json;

fn spec(feature: &str) -> NewWorkPackageSpec {
    NewWorkPackageSpec {
        story_persona: "a delivery team".to_owned(),
        story_action: "the work package to carry its spec".to_owned(),
        story_benefit: "done is judged per scenario".to_owned(),
        feature: feature.to_owned(),
        dor: json!({}),
    }
}

#[tokio::test]
async fn one_row_per_tenant_and_card_and_nothing_crosses_tenants() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "wps-a").await;
    let b = support::seed_tenant(&db.super_pool, "wps-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil");
    let tenant_b = TenantId::from_uuid(b.tenant).expect("non-nil");

    // ── The upsert is the one row per (tenant, card), and it bumps `version` ────────────────
    let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope A");
    let first = upsert_tx(
        &mut tx,
        tenant_a,
        a.card,
        &spec(
            "Feature: first\n  @verify=V1\n  Scenario: one\n    Given a\n    When b\n    Then c\n",
        ),
    )
    .await
    .expect("the first write");
    assert_eq!(first.version, 1, "a first write starts at version 1");
    let second = upsert_tx(
        &mut tx,
        tenant_a,
        a.card,
        &spec(
            "Feature: second\n  @verify=V1\n  Scenario: one\n    Given a\n    When b\n    Then c\n",
        ),
    )
    .await
    .expect("the second write");
    assert_eq!(second.version, 2, "a second write bumps the version");
    tx.commit().await.expect("commit A");

    let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope A");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM work_package_spec WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(a.tenant)
    .bind(a.card)
    .fetch_one(&mut *tx)
    .await
    .expect("the row count");
    assert_eq!(count, 1, "two upserts leave one row");
    tx.commit().await.expect("commit the count");

    // ── Tenant B cannot read A's spec, even naming A's card ────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope B");
    let leaked = get_tx(&mut tx, tenant_b, a.card)
        .await
        .expect("the read runs");
    assert!(leaked.is_none(), "B must not read A's spec");
    tx.commit().await.expect("commit B's read");

    // ── Tenant B cannot write A's card: the parent is resolved in B's scope ────────────────
    let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope B");
    let refused = upsert_tx(&mut tx, tenant_b, a.card, &spec("Feature: forged"))
        .await
        .expect_err("B must not write a spec onto A's card");
    assert!(
        matches!(refused, WorkPackageSpecStoreError::Parent(_)),
        "the foreign card is the parent refusal, not a database error: {refused}"
    );

    // ── RLS refuses a forged row that names A's tenant inside B's scope ────────────────────
    let forged = sqlx::query(
        "INSERT INTO work_package_spec
             (tenant_id, card_id, story_persona, story_action, story_benefit, feature, dor)
         VALUES ($1, $2, 'p', 'a', 'b', 'Feature: forged', '{}'::jsonb)",
    )
    .bind(a.tenant)
    .bind(a.card)
    .execute(&mut *tx)
    .await;
    assert!(
        forged.is_err(),
        "RLS must refuse a row written for another tenant"
    );
    tx.rollback().await.expect("roll back B's forged attempt");

    // ── UNIQUE(tenant_id, card_id) refuses a second row directly ───────────────────────────
    let duplicate = sqlx::query(
        "INSERT INTO work_package_spec
             (tenant_id, card_id, story_persona, story_action, story_benefit, feature, dor)
         VALUES ($1, $2, 'p', 'a', 'b', 'Feature: duplicate', '{}'::jsonb)",
    )
    .bind(a.tenant)
    .bind(a.card)
    .execute(&db.super_pool)
    .await;
    let error = duplicate.expect_err("the write key is unique per card");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23505"),
        "the duplicate is a unique violation: {error}"
    );
}
