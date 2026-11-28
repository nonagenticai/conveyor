//! `conveyor admin action invoke --actor-id` — an explicit, validated actor (#511).
//!
//! The operator CLI used to act only as the tenant's first active member, so a governance act
//! could never be performed by a DISTINCT approver — even though the live tenant carries
//! `conveyor-approver` for exactly that. `ActionCaller::new_as` selects the actor and refuses one
//! that is not an active member; this pins both halves down on the real engine.

mod common;

use conveyor_server::admin::action::{ActionCaller, CallError};
use conveyor_server::admin::install::{InstallArgs, run};
use conveyor_store as store;

fn manifest() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../seed/tenant-install.json")
        .canonicalize()
        .expect("the checked-in install manifest")
}

#[tokio::test]
async fn a_named_actor_creates_as_itself_and_a_stranger_is_refused() {
    let db = common::TestDb::fresh().await;
    let pool = &db.super_pool;

    run(
        pool,
        &InstallArgs {
            slug: "actor-id",
            name: "Actor id test tenant",
            admin_email: "actor-admin@example.test",
            admin_name: "Actor Admin",
            tenant_id: None,
            manifest: Some(manifest()),
        },
        "conveyor",
    )
    .await
    .expect("install the tenant");
    let tenant = store::tenants::id_by_slug(pool, "actor-id")
        .await
        .expect("look up the tenant")
        .expect("the tenant exists");

    // A second ACTIVE member, distinct from the install admin — the approver the CLI lacked.
    let second: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, status)
         VALUES ('actor-id:second', 'actor-second@example.test', 'Second Member', 'active')
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert the second user");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(tenant.as_uuid())
        .bind(second)
        .execute(pool)
        .await
        .expect("insert the second membership");

    // The named actor is who the engine attributes the write to.
    let caller = ActionCaller::new_as(pool, tenant, "conveyor", Some(second))
        .await
        .expect("an active member is a valid actor");
    let created = caller
        .call(
            "projects.create",
            serde_json::json!({"title": "A project owned by the named actor"}),
            None,
        )
        .await
        .expect("projects.create");
    let owner = created
        .get("owner_user_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| raw.parse::<uuid::Uuid>().ok());
    assert_eq!(
        owner,
        Some(second),
        "the named actor owns the project: {created}"
    );

    // The precondition is the CALL's, not the payload's: a stale `If-Match` is refused and the
    // current version writes. This is the path the record harness needs for the actions whose
    // edits are version-guarded (`projects.programme_meta`, `documents.publish`).
    let project_id: uuid::Uuid = created["id"]
        .as_str()
        .expect("the project id")
        .parse()
        .expect("a uuid");
    let version = created["version"].as_i64().expect("the project version");
    let written = caller
        .call_gated(
            "projects.programme_meta",
            serde_json::json!({"id": project_id, "meta": {"viability": "rework"}}),
            Some(version),
            None,
            None,
        )
        .await
        .expect("the current version writes");
    assert_eq!(
        written["project"]["meta"]["viability"], "rework",
        "{written}"
    );
    let stale = caller
        .call_gated(
            "projects.programme_meta",
            serde_json::json!({"id": project_id, "meta": {"viability": "fold"}}),
            Some(version - 1),
            None,
            None,
        )
        .await;
    assert!(
        matches!(&stale, Err(CallError::Action { status: 412, .. })),
        "a stale If-Match is refused: {:?}",
        stale.err().map(|error| error.to_string()),
    );

    // A user with no membership of this tenant can never be the actor.
    let stranger = uuid::Uuid::now_v7();
    let refused = ActionCaller::new_as(pool, tenant, "conveyor", Some(stranger)).await;
    assert!(
        matches!(&refused, Err(CallError::NotMember(id)) if *id == stranger),
        "a user outside the tenant must be refused: {:?}",
        refused.err().map(|error| error.to_string()),
    );
}
