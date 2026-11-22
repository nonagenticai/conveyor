//! #556 P1.1 — a run's bound tools are one query.
//!
//! `store::tool::resolve` answers one canonical name; the projection needs the whole grant. The
//! rules that can go wrong silently: a draft toolkit or tool looks callable, a second run's grant
//! leaks in, or the binding's ceiling is lost on the way to the model.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::tool::{self, RunToolContext};
use serde_json::json;
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

async fn insert_run(db: &support::TestDb, seed: &support::Seeded) -> Uuid {
    let mut tx = begin_scoped(&db.runtime, tenant(seed))
        .await
        .expect("scope");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.project)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the run");
    id
}

async fn insert_toolkit(db: &support::TestDb, tenant: TenantId, key: &str, status: &str) -> Uuid {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, owner, status)
         VALUES ($1, $2, 1, $3, $2, 'platform', $4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(format!("{key} toolkit"))
    .bind(status)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the toolkit");
    tx.commit().await.expect("commit the toolkit");
    id
}

async fn insert_tool(
    db: &support::TestDb,
    tenant: TenantId,
    toolkit_id: Uuid,
    key: &str,
    status: &str,
) {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, timeout_secs,
                           handler_action, status)
         VALUES ($1, $2, $3, $4, 'read', 30, 'fixtures.echo', $5)",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .bind(key)
    .bind(json!({"type": "object", "additionalProperties": false}))
    .bind(status)
    .execute(&mut *tx)
    .await
    .expect("insert the tool");
    tx.commit().await.expect("commit the tool");
}

async fn bind(db: &support::TestDb, tenant: TenantId, run_id: Uuid, toolkit_id: Uuid) {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
         VALUES ($1, $2, $3, 'read', true)",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(toolkit_id)
    .execute(&mut *tx)
    .await
    .expect("insert the binding");
    tx.commit().await.expect("commit the binding");
}

#[tokio::test]
async fn bound_tools_are_the_runs_grants() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "556-bound").await;
    let tenant = tenant(&seed);
    let mine = insert_run(&db, &seed).await;
    let other = insert_run(&db, &seed).await;

    let coding = insert_toolkit(&db, tenant, "coding", "published").await;
    insert_tool(&db, tenant, coding, "jobs.get", "published").await;
    insert_tool(&db, tenant, coding, "jobs.create", "draft").await;
    let draft = insert_toolkit(&db, tenant, "draft_tk", "draft").await;
    insert_tool(&db, tenant, draft, "x", "published").await;
    bind(&db, tenant, mine, coding).await;
    bind(&db, tenant, other, coding).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let rows = tool::bound_tools(
        &mut tx,
        tenant,
        &RunToolContext {
            run_id: mine,
            run_step_id: None,
            card_id: None,
            job_binding_id: None,
        },
    )
    .await
    .expect("bound tools");
    tx.commit().await.expect("commit the read");

    let keys: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| (row.toolkit_key.as_str(), row.tool_key.as_str()))
        .collect();
    assert_eq!(
        keys,
        vec![("coding", "jobs.get")],
        "one published tool through this run's grant; draft and the other run are absent"
    );
    assert_eq!(rows[0].effect, "read");
    assert_eq!(rows[0].effect_ceiling, "read");
    assert_eq!(rows[0].input_schema["type"], "object");
}
