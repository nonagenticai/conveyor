#![forbid(unsafe_code)]
//! S13 — the driving tick must be enqueued for the NEXT step while the current tick's job is
//! still `running`.
//!
//! The live-dedupe index treats a `running` job as live. With a constant `<run>:tick` key, the
//! tick handler's own insert collided with the job that was executing it, became a no-op, and the
//! run stopped after one node — measured on the §7 gate5 shell harness (one `run_step`, one
//! succeeded `workflow.node` job, nothing pending). The ordinal in the key is the number of
//! `run_step` rows, so the successor is always a new key.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::workflow::run::enqueue_workflow_node_tx;

#[tokio::test]
async fn the_next_tick_is_inserted_while_the_current_job_still_runs() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil tenant");

    // The first tick has been claimed: its job is live at ordinal 0.
    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped writer");
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, status)
         VALUES ($1, 'workflow.node', $2, $3, 'running')",
    )
    .bind(seed.tenant)
    .bind(format!("{}:tick:0", seed.run))
    .bind(serde_json::json!({"run_id": seed.run}))
    .execute(&mut *tx)
    .await
    .expect("the running tick");
    tx.commit().await.expect("commit");

    // The handler records its step and enqueues the successor in the same transaction.
    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped writer");
    sqlx::query(
        "INSERT INTO run_step (tenant_id, run_id, position, attempt, node_id, action, status)
         VALUES ($1, $2, 1, 1, 'read', 'cards.get', 'succeeded')",
    )
    .bind(seed.tenant)
    .bind(seed.run)
    .execute(&mut *tx)
    .await
    .expect("the recorded step");
    let next = enqueue_workflow_node_tx(&mut tx, tenant, seed.run, 0)
        .await
        .expect("enqueue the next tick");
    tx.commit().await.expect("commit");

    let next = next.expect("the next tick must be inserted while the current job still runs");
    let key: String = sqlx::query_scalar("SELECT dedupe_key FROM job WHERE id = $1")
        .bind(next)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the new job");
    assert_eq!(key, format!("{}:tick:1", seed.run));

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'workflow.node'
            AND status IN ('pending', 'running')",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count live ticks");
    assert_eq!(live, 2, "the running tick and its successor coexist");
}
