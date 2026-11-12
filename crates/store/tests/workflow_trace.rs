#![forbid(unsafe_code)]
//! S13 step 9 — the run trace read.
//!
//! Three properties, each a silent failure if it were wrong: the rows are ordered by
//! `(position, attempt)` (a retry is a second row, not a replacement), a process step's row
//! (`node_id IS NULL`) is not a workflow effect, and FORCE RLS keeps another tenant's trace dark.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::workflow::run_step::run_trace_tx;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

async fn insert_project(conn: &mut PgConnection, tenant: TenantId, key: &str) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(key)
    .fetch_one(&mut *conn)
    .await
    .expect("insert project");
    id
}

async fn insert_workflow(conn: &mut PgConnection, tenant: TenantId, key: &str) -> Uuid {
    let definition = json!({
        "schema": 1,
        "nodes": [
            {"id": "claim", "kind": "action", "action": "cards.get"},
            {"id": "move", "kind": "action", "action": "cards.move"}
        ],
        "edges": []
    });
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workflow (tenant_id, key, title, version, definition)
         VALUES ($1, $2, $3, 1, $4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(key)
    .bind(&definition)
    .fetch_one(&mut *conn)
    .await
    .expect("insert workflow");
    id
}

async fn insert_run(
    conn: &mut PgConnection,
    tenant: TenantId,
    project: Uuid,
    workflow: Uuid,
) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO run (tenant_id, project_id, workflow_id, status)
         VALUES ($1, $2, $3, 'succeeded') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project)
    .bind(workflow)
    .fetch_one(&mut *conn)
    .await
    .expect("insert run");
    id
}

#[allow(clippy::too_many_arguments)]
async fn insert_step(
    conn: &mut PgConnection,
    tenant: TenantId,
    run: Uuid,
    position: i32,
    attempt: i32,
    node_id: Option<&str>,
    action: &str,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO run_step
             (tenant_id, run_id, position, attempt, action, status, node_id, input, output,
              duration_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(tenant.as_uuid())
    .bind(run)
    .bind(position)
    .bind(attempt)
    .bind(action)
    .bind(status)
    .bind(node_id)
    .bind(json!({"position": position, "attempt": attempt}))
    .bind(json!({"ok": true}))
    .bind(3_i32)
    .execute(&mut *conn)
    .await
    .expect("insert run_step");
}

#[tokio::test]
async fn a_trace_is_ordered_and_carries_the_workflow_identity() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let tenant = tenant(&seed);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let project = insert_project(&mut tx, tenant, "s13-a").await;
    let workflow = insert_workflow(&mut tx, tenant, "cap.x").await;
    let run = insert_run(&mut tx, tenant, project, workflow).await;
    // Inserted out of order on purpose: the read is ordered, not the insertion order.
    insert_step(
        &mut tx,
        tenant,
        run,
        2,
        1,
        Some("move"),
        "cards.move",
        "succeeded",
    )
    .await;
    insert_step(
        &mut tx,
        tenant,
        run,
        1,
        1,
        Some("claim"),
        "cards.get",
        "failed",
    )
    .await;
    insert_step(
        &mut tx,
        tenant,
        run,
        1,
        2,
        Some("claim"),
        "cards.get",
        "succeeded",
    )
    .await;
    // A process step's row is not a workflow effect.
    insert_step(&mut tx, tenant, run, 3, 1, None, "cards.get", "succeeded").await;
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let trace = run_trace_tx(&mut tx, tenant, run)
        .await
        .expect("read trace")
        .expect("a workflow run");
    tx.commit().await.expect("commit");

    assert_eq!(trace.workflow_key, "cap.x");
    assert_eq!(trace.workflow_version, 1);
    assert_eq!(trace.definition["schema"], json!(1));
    let keys: Vec<(i32, i32, &str)> = trace
        .rows
        .iter()
        .map(|row| (row.position, row.attempt, row.node_id.as_str()))
        .collect();
    assert_eq!(keys, vec![(1, 1, "claim"), (1, 2, "claim"), (2, 1, "move")]);
    assert_eq!(trace.rows[0].status, "failed");
    assert_eq!(trace.rows[1].status, "succeeded");
}

#[tokio::test]
async fn another_tenant_cannot_see_the_trace() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "dev-a").await;
    let b = support::seed_tenant(&db.super_pool, "dev-b").await;
    let tenant_a = tenant(&a);
    let tenant_b = tenant(&b);

    let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope a");
    let project = insert_project(&mut tx, tenant_a, "s13-a").await;
    let workflow = insert_workflow(&mut tx, tenant_a, "cap.x").await;
    let run_a = insert_run(&mut tx, tenant_a, project, workflow).await;
    insert_step(
        &mut tx,
        tenant_a,
        run_a,
        1,
        1,
        Some("claim"),
        "cards.get",
        "succeeded",
    )
    .await;
    tx.commit().await.expect("commit a");

    let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope b");
    let project_b = insert_project(&mut tx, tenant_b, "s13-b").await;
    let workflow_b = insert_workflow(&mut tx, tenant_b, "cap.x").await;
    let run_b = insert_run(&mut tx, tenant_b, project_b, workflow_b).await;
    insert_step(
        &mut tx,
        tenant_b,
        run_b,
        1,
        1,
        Some("claim"),
        "cards.get",
        "succeeded",
    )
    .await;

    assert!(
        run_trace_tx(&mut tx, tenant_b, run_a)
            .await
            .expect("read a as b")
            .is_none(),
        "another tenant's run is invisible"
    );
    let own = run_trace_tx(&mut tx, tenant_b, run_b)
        .await
        .expect("read b")
        .expect("b's own run");
    assert_eq!(own.workflow_key, "cap.x");
    assert_eq!(own.rows.len(), 1);

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM run_step WHERE run_id = $1 AND tenant_id = $2")
            .bind(run_a)
            .bind(tenant_b.as_uuid())
            .fetch_one(&mut *tx)
            .await
            .expect("count under RLS");
    assert_eq!(count, 0);
    tx.commit().await.expect("commit b");
}
