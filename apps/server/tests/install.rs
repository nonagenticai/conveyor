//! `conveyor admin install` — a fresh tenant from one command (S14 P5).
//!
//! The command must create the tenant's local rows (tenant, first admin, membership), import
//! the catalogue and the default process catalogue, arm the manifest's schedules through the
//! real action engine, and converge on a second run: same tenant id, nothing re-created, no
//! duplicate schedule.

mod common;

use conveyor_server::admin::action::ActionCaller;
use conveyor_server::admin::install::{InstallArgs, run};
use conveyor_store as store;

fn manifest() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../seed/tenant-install.json")
        .canonicalize()
        .expect("the checked-in install manifest")
}

fn args<'a>(slug: &'a str) -> InstallArgs<'a> {
    InstallArgs {
        slug,
        name: "Install test tenant",
        admin_email: "install-admin@example.test",
        admin_name: "Install Admin",
        tenant_id: None,
        manifest: Some(manifest()),
    }
}

#[tokio::test]
async fn a_fresh_tenant_is_installed_and_a_second_run_converges() {
    let db = common::TestDb::fresh().await;
    let pool = &db.super_pool;

    let first = run(pool, &args("install-a"), "conveyor")
        .await
        .expect("the first install");

    // The tenant, its admin and the membership exist under the slug.
    let tenant_id = store::tenants::id_by_slug(pool, "install-a")
        .await
        .expect("look up the tenant")
        .expect("the tenant exists");
    assert_eq!(tenant_id, first.tenant_id);
    let admin_email: Option<String> =
        sqlx::query_scalar("SELECT email FROM app_user WHERE id = $1")
            .bind(first.admin_user_id)
            .fetch_one(pool)
            .await
            .expect("the admin row");
    assert_eq!(admin_email.as_deref(), Some("install-admin@example.test"));
    let admin_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM membership WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(first.admin_user_id)
    .fetch_optional(pool)
    .await
    .expect("the membership");
    assert_eq!(admin_role.as_deref(), Some("admin"));

    // The catalogue and the process catalogue are content, so a fresh tenant owns some.
    assert!(
        first.processes > 0,
        "the default process catalogue installs processes: {first:?}"
    );
    assert!(first.sops > 0, "the processes carry their SOPs: {first:?}");
    assert!(!first.schedules.is_empty(), "the manifest arms schedules");

    // The schedules are tenant-scoped rows, not a manifest claim.
    let tenant = conveyor_domain::TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let caller = ActionCaller::new(pool, tenant, "conveyor")
        .await
        .expect("the tenant's service actor");
    let listed = caller
        .call("schedules.list", serde_json::json!({}), None)
        .await
        .expect("schedules.list");
    let rows = listed.as_array().cloned().unwrap_or_default();
    assert_eq!(
        rows.len(),
        first.schedules.len(),
        "one row per manifest schedule: {listed}"
    );
    for kind in &first.schedules {
        assert!(
            rows.iter()
                .any(|row| row.get("kind").and_then(serde_json::Value::as_str) == Some(kind)),
            "the schedule {kind} is installed: {listed}"
        );
    }

    // The second run converges: same tenant, nothing re-created, same schedule count.
    let second = run(pool, &args("install-a"), "conveyor")
        .await
        .expect("the second install");
    assert_eq!(second.tenant_id, first.tenant_id, "the slug resolves to the same id");
    assert_eq!(second.processes, 0, "a second run creates no process");
    assert_eq!(second.sops, 0, "a second run creates no SOP");
    assert_eq!(second.agents, 0, "a second run creates no agent");
    assert_eq!(second.maps, 0, "a second run creates no map");
    assert_eq!(second.schedules, first.schedules);

    let listed = caller
        .call("schedules.list", serde_json::json!({}), None)
        .await
        .expect("schedules.list after the second run");
    assert_eq!(
        listed.as_array().map(Vec::len),
        Some(first.schedules.len()),
        "no duplicate schedule on a second run: {listed}"
    );
}
