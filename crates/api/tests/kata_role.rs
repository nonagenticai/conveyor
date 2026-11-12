//! The `kata_executor` role — a machine-only synthetic tenant-admin (George 2026-11-09).
//!
//! Three properties, all over the real HTTP surface:
//! * the migration accepts the value (`members.set_role` can write it),
//! * `members.set_role` refuses it for a HUMAN and leaves that membership untouched,
//! * the role's grants are an `admin`'s: an admin-only action (`departments.create`) succeeds
//!   for the service principal and stays `403` for a plain member.
//!
//! The process executor runs as this role; `crates/actions/tests/process_*.rs` prove that
//! path end to end.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

async fn provision(
    pg: &support::Pg,
    tenant_id: Uuid,
    subject: &str,
    kind: &str,
    role: &str,
) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, display_name, kind) VALUES ($1, $1, $2)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET kind = excluded.kind
         RETURNING id",
    )
    .bind(subject)
    .bind(kind)
    .fetch_one(&pg.owner)
    .await
    .expect("insert app_user");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .execute(&mut *tx)
    .await
    .expect("insert membership");
    tx.commit().await.expect("commit provision");
    user_id
}

async fn membership_role(pg: &support::Pg, tenant: TenantId, user: Uuid) -> String {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    sqlx::query_scalar("SELECT role FROM membership WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .expect("membership role")
}

#[tokio::test]
async fn kata_executor_is_a_machine_only_tenant_admin() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let carol = provision(&pg, seeded.tenant_id, "dev:carol", "human", "member").await;
    let service = provision(&pg, seeded.tenant_id, "dev:svc", "service", "member").await;

    // A HUMAN may not acquire the synthetic admin: refused, membership untouched.
    let (status, body) = running
        .invoke(
            "members.set_role",
            "dev:alice",
            json!({"user_id": carol, "role": "kata_executor"}),
            Some("1"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("machine principal"),
        "the refusal names the rule: {body}"
    );
    assert_eq!(
        membership_role(&pg, seeded.tenant, carol).await,
        "member",
        "a refused role change writes nothing"
    );

    // A MACHINE principal may hold it — and the value round-trips through the database CHECK.
    let (status, body) = running
        .invoke(
            "members.set_role",
            "dev:alice",
            json!({"user_id": service, "role": "kata_executor"}),
            Some("1"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["role"], "kata_executor", "{body}");
    assert_eq!(
        membership_role(&pg, seeded.tenant, service).await,
        "kata_executor"
    );

    // The grant is an admin's, not a member's. S06d: a machine credential is now channel-bound,
    // so the *role* table is asserted directly; the connector's own HTTP call is refused by its
    // grant set (and the administrative floor), not by the role.
    let departments = conveyor_actions::registry()
        .get("departments.create")
        .expect("departments.create is registered");
    assert!(
        departments.permits(conveyor_auth::Role::KataExecutor),
        "kata_executor must hold departments:write for the engine's internal calls"
    );
    assert!(!departments.permits(conveyor_auth::Role::Member));
    let (status, body) = running
        .invoke(
            "departments.create",
            "dev:svc",
            json!({"name": "Kata"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 403,
        "a connector's channel grant and the machine floor bound it: {body}"
    );
    let (status, body) = running
        .invoke(
            "departments.create",
            "dev:carol",
            json!({"name": "Nope"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "a member must not: {body}");
}
