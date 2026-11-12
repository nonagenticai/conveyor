//! RBAC store behaviour: the tenant policy, invites, the last-admin guard and the
//! email-keyed identity rebind.
//!
//! The harness is `apps/server/tests/common/mod.rs` by `#[path]`: one ephemeral Postgres per
//! test binary, one fresh migrated database per test. Fixtures are inserted as the superuser;
//! everything under test runs as `conveyor_runtime` in a tenant-scoped transaction.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::TenantId;
use conveyor_store::rbac::{self, MemberStoreError};
use conveyor_store::{begin_scoped, identity, invite};
use sqlx::PgPool;
use uuid::Uuid;

async fn tenant(db: &common::TestDb, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
            .bind(slug)
            .bind(slug.to_uppercase())
            .fetch_one(&db.super_pool)
            .await
            .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn user(db: &common::TestDb, subject: &str, email: &str, kind: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $1, $3) RETURNING id",
    )
    .bind(subject)
    .bind(email)
    .bind(kind)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a user")
}

async fn member(db: &common::TestDb, tenant: Uuid, user: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(tenant)
    .bind(user)
    .bind(role)
    .execute(&db.super_pool)
    .await
    .expect("seed a membership");
}

/// Set a role on the runtime pool as the given tenant.
async fn set_role(
    db: &common::TestDb,
    tenant: TenantId,
    user: Uuid,
    role: &str,
) -> Result<(), MemberStoreError> {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let result = rbac::member_set_role_tx(&mut tx, tenant, user, role, None).await;
    tx.commit().await.expect("commit");
    result.map(|_| ())
}

#[tokio::test]
async fn the_policy_is_per_role_per_tenant_and_invisible_across_tenants() {
    let db = common::TestDb::fresh().await;
    let a = tenant(&db, "rbac-a").await;
    let b = tenant(&db, "rbac-b").await;

    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
    rbac::set_policy_tx(
        &mut tx,
        a,
        &[
            ("member", "projects:write", false),
            ("viewer", "boards:read", false),
        ],
    )
    .await
    .expect("write the policy");
    tx.commit().await.expect("commit");

    let member_policy = rbac::policy_for(&db.runtime, a, "member")
        .await
        .expect("read member policy");
    assert_eq!(member_policy.get("projects:write"), Some(&false));
    assert_eq!(
        member_policy.get("boards:read"),
        None,
        "another role's row must not leak into this role's map"
    );
    let viewer_policy = rbac::policy_for(&db.runtime, a, "viewer")
        .await
        .expect("read viewer policy");
    assert_eq!(viewer_policy.get("boards:read"), Some(&false));
    assert!(
        rbac::policy_for(&db.runtime, a, "admin")
            .await
            .expect("read admin policy")
            .is_empty(),
        "admin is not editable"
    );
    assert!(
        rbac::policy_for(&db.runtime, b, "member")
            .await
            .expect("read other tenant")
            .is_empty(),
        "tenant B must see none of tenant A's policy"
    );

    // Reset clears every role, not one.
    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
    let removed = rbac::reset_policy_tx(&mut tx, a).await.expect("reset");
    tx.commit().await.expect("commit");
    assert_eq!(removed, 2);
    assert!(
        rbac::policy_for(&db.runtime, a, "viewer")
            .await
            .expect("read after reset")
            .is_empty()
    );
}

#[tokio::test]
async fn an_invite_is_single_use_and_binds_a_membership_on_first_accept() {
    let db = common::TestDb::fresh().await;
    let a = tenant(&db, "invite-a").await;
    let a_uuid = a.as_uuid();

    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
    let first = invite::create_tx(&mut tx, a, "Ada@Example.com", "member", None)
        .await
        .expect("create the invite");
    // Re-inviting the same address is the same invitation with a new role.
    let second = invite::create_tx(&mut tx, a, "ada@example.com", "admin", None)
        .await
        .expect("re-invite");
    assert_eq!(
        first.id, second.id,
        "one pending invite per (tenant, email)"
    );
    assert_eq!(second.role, "admin");
    let listed = invite::list_tx(&mut tx).await.expect("list");
    assert_eq!(listed.len(), 1);
    tx.commit().await.expect("commit");

    // Login's read, before any tenant is known.
    let pending = invite::pending_for_email(&db.runtime, "ada@example.com")
        .await
        .expect("find the invite by email");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].tenant_slug, "invite-a");

    let ada = user(&db, "oidc:ada", "ada@example.com", "human").await;
    let joined = invite::accept_all_for(&db.runtime, ada, "ada@example.com")
        .await
        .expect("accept");
    assert_eq!(joined.len(), 1);

    let role: String =
        sqlx::query_scalar("SELECT role FROM membership WHERE tenant_id = $1 AND user_id = $2")
            .bind(a_uuid)
            .bind(ada)
            .fetch_one(&db.super_pool)
            .await
            .expect("the membership exists");
    assert_eq!(role, "admin", "the invite's role, not a default");

    // A second accept is a no-op: the invite is no longer pending.
    let again = invite::accept_all_for(&db.runtime, ada, "ada@example.com")
        .await
        .expect("re-accept");
    assert!(again.is_empty());
    let pending = invite::pending_for_email(&db.runtime, "ada@example.com")
        .await
        .expect("no pending invite remains");
    assert!(pending.is_empty());
}

#[tokio::test]
async fn one_tenants_invite_is_invisible_to_another() {
    let db = common::TestDb::fresh().await;
    let a = tenant(&db, "invite-a2").await;
    let b = tenant(&db, "invite-b2").await;

    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope of A");
    invite::create_tx(&mut tx, a, "shared@example.com", "member", None)
        .await
        .expect("invite into A");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, b).await.expect("scope of B");
    let listed = invite::list_tx(&mut tx).await.expect("list B");
    assert!(listed.is_empty(), "B must not read A's invite");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_last_human_admin_cannot_be_demoted_or_removed() {
    let db = common::TestDb::fresh().await;
    let a = tenant(&db, "guard-a").await;
    let a_uuid = a.as_uuid();
    let ann = user(&db, "oidc:ann", "ann@example.com", "human").await;
    let ben = user(&db, "oidc:ben", "ben@example.com", "human").await;
    let machine = user(&db, "oidc:kata", "kata@example.com", "service").await;
    member(&db, a_uuid, ann, "admin").await;
    member(&db, a_uuid, ben, "member").await;
    member(&db, a_uuid, machine, "kata_executor").await;

    // Two admins: demoting one is allowed.
    member(&db, a_uuid, ben, "admin").await;
    set_role(&db, a, ben, "member").await.expect("demote ben");

    // One human admin left: demoting Ann is refused, even though a kata_executor exists.
    assert!(matches!(
        set_role(&db, a, ann, "member").await,
        Err(MemberStoreError::LastAdmin)
    ));
    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
    assert!(matches!(
        rbac::member_remove_tx(&mut tx, a, ann).await,
        Err(MemberStoreError::LastAdmin)
    ));
    tx.rollback().await.expect("rollback");

    // A non-admin may be removed.
    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
    rbac::member_remove_tx(&mut tx, a, ben)
        .await
        .expect("remove ben");
    tx.commit().await.expect("commit");

    // A second human admin restores the right to demote Ann.
    member(&db, a_uuid, ben, "admin").await;
    set_role(&db, a, ann, "viewer").await.expect("demote ann");
}

#[tokio::test]
async fn platform_admin_is_global_and_grant_twice_is_one_row() {
    let db = common::TestDb::fresh().await;
    let ada = user(&db, "oidc:plat", "plat@example.com", "human").await;
    assert!(
        !rbac::is_platform_admin(&db.runtime, ada)
            .await
            .expect("read")
    );
    rbac::grant_platform_admin(&db.runtime, ada, None)
        .await
        .expect("grant");
    rbac::grant_platform_admin(&db.runtime, ada, None)
        .await
        .expect("grant again");
    assert!(
        rbac::is_platform_admin(&db.runtime, ada)
            .await
            .expect("read")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_admin")
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    assert_eq!(count, 1);
    assert!(
        rbac::revoke_platform_admin(&db.runtime, ada)
            .await
            .expect("revoke")
    );
    assert!(
        !rbac::is_platform_admin(&db.runtime, ada)
            .await
            .expect("read")
    );
}

#[tokio::test]
async fn a_recreated_keycloak_account_rebinds_the_same_user_row() {
    let db = common::TestDb::fresh().await;
    let ada = user(&db, "oidc:old", "ada@example.com", "human").await;
    let a = tenant(&db, "rebind-a").await;
    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
    invite::grant_membership_tx(&mut tx, a, ada, "admin")
        .await
        .expect("membership");
    tx.commit().await.expect("commit");

    // The new account presents a new subject and the same verified email.
    let found = identity::resolve_identity_by_email(&db.runtime, "ADA@example.com")
        .await
        .expect("resolve by email")
        .expect("the email resolves");
    assert_eq!(found.user_id.as_uuid(), ada);
    assert_eq!(found.subject, "oidc:old");

    let changed = identity::rebind_subject(&db.runtime, ada, "oidc:new")
        .await
        .expect("rebind");
    assert_eq!(changed, 1);
    assert!(
        identity::resolve_identity(&db.runtime, "oidc:old")
            .await
            .expect("resolve old")
            .is_none(),
        "the old subject must no longer resolve"
    );
    let rebound = identity::resolve_identity(&db.runtime, "oidc:new")
        .await
        .expect("resolve new")
        .expect("the new subject resolves");
    assert_eq!(rebound.user_id.as_uuid(), ada);
    assert_eq!(
        rebound.memberships.len(),
        1,
        "memberships follow the row, not the subject"
    );

    // Rebinding to the subject it already holds is a no-op.
    let again = identity::rebind_subject(&db.runtime, ada, "oidc:new")
        .await
        .expect("rebind again");
    assert_eq!(again, 0);
}

#[tokio::test]
async fn an_unknown_email_resolves_to_nothing_and_creates_no_user() {
    let db = common::TestDb::fresh().await;
    assert!(
        identity::resolve_identity_by_email(&db.runtime, "nobody@example.com")
            .await
            .expect("resolve")
            .is_none()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app_user")
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    assert_eq!(count, 0, "a lookup never signs anyone up");
}

#[allow(dead_code)]
fn _pool_type_check(_: &PgPool) {}
