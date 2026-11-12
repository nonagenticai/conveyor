//! The tenant policy actually changes what an action may do.
//!
//! The store proves the rows round-trip and the matrix test proves the screen; this proves the
//! one thing that matters: the SAME caller is refused, then allowed, then refused again as the
//! tenant's stored override changes — with no restart and no cache to clear.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;

async fn seed_member(pg: &support::Pg, tenant: uuid::Uuid, subject: &str) -> uuid::Uuid {
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $1, 'human') RETURNING id",
    )
    .bind(subject)
    .bind(format!("{}@dev.invalid", subject.replace(':', "-")))
    .fetch_one(&pg.owner)
    .await
    .expect("seed the member");
    let tenant = conveyor_domain::TenantId::from_uuid(tenant).expect("tenant id");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *tx)
        .await
        .expect("seed the membership");
    tx.commit().await.expect("commit");
    id
}

#[tokio::test]
async fn a_stored_override_changes_the_same_callers_answer() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    seed_member(&pg, seeded.tenant_id, "dev:carol").await;
    let running = support::Running::start(&pg).await;

    // `projects:write` is an ADMIN scope in the compiled table: a member is refused.
    let create = json!({"title": "Policy proof"});
    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:carol",
            create.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 403,
        "a member must not create a project by default: {body}"
    );

    // The admin grants it to Member for THIS tenant.
    let (status, body) = running
        .invoke(
            "roles.policy_set",
            "dev:alice",
            json!({"entries": [{"role": "member", "scope": "projects:write", "allowed": true}]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    // The same caller, the same request: now permitted, and the project is the proof.
    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:carol",
            create.clone(),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 201,
        "the stored override must permit the call: {body}"
    );

    // Revoking it again refuses the next call. No cache, no restart.
    let (status, body) = running
        .invoke(
            "roles.policy_set",
            "dev:alice",
            json!({"entries": [{"role": "member", "scope": "projects:write", "allowed": false}]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:carol",
            json!({"title": "Refused again"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "the revoked scope must refuse again: {body}");

    running.shutdown().await;
}

#[tokio::test]
async fn the_kernel_and_the_fixed_roles_are_refused_by_the_action() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    seed_member(&pg, seeded.tenant_id, "dev:carol").await;
    let running = support::Running::start(&pg).await;

    // Kernel: members:write cannot be revoked from any role.
    let (status, body) = running
        .invoke(
            "roles.policy_set",
            "dev:alice",
            json!({"entries": [{"role": "member", "scope": "members:write", "allowed": false}]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");

    // Admin is not editable.
    let (status, body) = running
        .invoke(
            "roles.policy_set",
            "dev:alice",
            json!({"entries": [{"role": "admin", "scope": "cards:read", "allowed": false}]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");

    // An unknown scope is refused rather than stored and silently ignored.
    let (status, body) = running
        .invoke(
            "roles.policy_set",
            "dev:alice",
            json!({"entries": [{"role": "member", "scope": "nonsense:write", "allowed": true}]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");

    // And a member cannot edit the policy at all.
    let (status, body) = running
        .invoke(
            "roles.policy_set",
            "dev:carol",
            json!({"entries": []}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");

    running.shutdown().await;
}
