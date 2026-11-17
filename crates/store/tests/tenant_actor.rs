//! CONTRACT §17: an internal invocation acts as a real principal.
//!
//! `store::identity::tenant_actor` is the loop's source of that principal. It answers the
//! tenant's first active admin (falling back to the first active member), and `None` when the
//! tenant has no active member — which the loop refuses typed (`agent.no_actor`) rather than
//! inventing a user. Measured live 2026-11-17: the loop passed the nil uuid, so
//! `documents.compose` failed `document_created_by_fk` and no agent run could create a document.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::identity::tenant_actor;
use uuid::Uuid;

#[tokio::test]
async fn a_membered_tenant_resolves_an_actor_and_a_bare_tenant_has_none() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "actor-tenant").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant id");

    // The store fixture seeds users but no membership; give this tenant one active member.
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(seed.tenant)
        .bind(seed.user)
        .execute(&db.super_pool)
        .await
        .expect("one active member");

    let actor = tenant_actor(&db.runtime, tenant).await.expect("the lookup");
    assert_eq!(
        actor,
        Some(seed.user),
        "the seeded tenant's active member is the actor"
    );

    let bare: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('bare-tenant', 'Bare') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("a tenant with no member");
    let bare = TenantId::from_uuid(bare).expect("tenant id");
    assert_eq!(
        tenant_actor(&db.runtime, bare).await.expect("the lookup"),
        None,
        "a tenant with no active member has no actor to act as"
    );
}
