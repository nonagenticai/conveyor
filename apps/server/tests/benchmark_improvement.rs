//! S18 step 3 — `benchmark.improvement.run` refuses an empty payload permanently, leaves an
//! already judged candidate untouched, and schedules the next look for a live one.
//!
//! `cargo test -p conveyor-server --test benchmark_improvement`.

mod common;

use conveyor_actions::benchmark::runs::IMPROVEMENT_KIND;
use conveyor_domain::TenantId;
use conveyor_server::benchmark_improvement::run;
use conveyor_store as store;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn a_payload_without_a_run_id_is_a_permanent_refusal() {
    // The refusal happens before the pool is touched, so a lazy pool is the honest fixture: if
    // the check ever moved behind a query, the failure would be retryable, not permanent.
    let failure = run(
        &common::lazy_pool(),
        TenantId::new(),
        "conveyor-test",
        &json!({}),
    )
    .await
    .expect_err("an empty payload cannot name a run");
    assert!(
        failure.permanent,
        "a missing run_id can never succeed on retry: {}",
        failure.message
    );
    assert!(!failure.deferred, "{}", failure.message);
    assert!(
        failure.message.contains("run_id"),
        "the refusal must name the missing field: {}",
        failure.message
    );
}

/// A tenant in this test's fresh database.
async fn seed_tenant(db: &common::TestDb) -> TenantId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('improvement', 'improvement') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    TenantId::from_uuid(id).expect("tenant id")
}

/// A suite row — the run's only foreign key.
async fn seed_suite(db: &common::TestDb, tenant: TenantId) -> Uuid {
    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_suite
             (slug, dataset, dataset_revision, seed_sha256, image_lock_sha256,
              swebench_version, parser_version)
         VALUES ('improvement-v1', 'SWE-bench/SWE-bench_Verified', 'rev', 'seed', 'lock',
                 '5.0.2', 'swebench-5.0.2')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("suite");
    tx.commit().await.expect("commit");
    id
}

#[tokio::test]
async fn an_already_judged_candidate_writes_nothing() {
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db).await;
    let suite_id = seed_suite(&db, tenant).await;
    let baseline = Uuid::now_v7();
    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let candidate: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_run
             (suite_id, arm, subject_kind, capability_agent_id, label, status,
              improvement_verdict, improvement_of_run_id, improvement_note)
         VALUES ($1, 'conveyor', 'capability_agent', $2, 'candidate', 'succeeded',
                 'keep', $3, 'already judged')
         RETURNING id",
    )
    .bind(suite_id)
    .bind(Uuid::now_v7())
    .bind(baseline)
    .fetch_one(&mut *tx)
    .await
    .expect("candidate run");
    tx.commit().await.expect("commit");

    run(
        &db.runtime,
        tenant,
        "conveyor-test",
        &json!({"run_id": candidate, "baseline_run_id": baseline}),
    )
    .await
    .expect("a re-delivered judgement is a no-op, not a failure");

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let stored = store::benchmark::get_run_tx(&mut tx, tenant, candidate)
        .await
        .expect("candidate run");
    assert_eq!(stored.improvement_verdict.as_deref(), Some("keep"));
    assert_eq!(stored.improvement_note.as_deref(), Some("already judged"));
    assert_eq!(stored.improvement_of_run_id, Some(baseline));
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("job count");
    assert_eq!(jobs, 0, "an already judged candidate schedules nothing");
}

#[tokio::test]
async fn a_running_candidate_schedules_the_next_look() {
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db).await;
    let suite_id = seed_suite(&db, tenant).await;
    let baseline = Uuid::now_v7();
    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let candidate: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_run
             (suite_id, arm, subject_kind, capability_agent_id, label, status)
         VALUES ($1, 'conveyor', 'capability_agent', $2, 'candidate', 'running')
         RETURNING id",
    )
    .bind(suite_id)
    .bind(Uuid::now_v7())
    .fetch_one(&mut *tx)
    .await
    .expect("candidate run");
    tx.commit().await.expect("commit");

    run(
        &db.runtime,
        tenant,
        "conveyor-test",
        &json!({"run_id": candidate, "baseline_run_id": baseline}),
    )
    .await
    .expect("a live candidate schedules a look, it does not fail");

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scoped");
    let jobs: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, dedupe_key FROM job WHERE tenant_id = $1")
            .bind(tenant.as_uuid())
            .fetch_all(&mut *tx)
            .await
            .expect("jobs");
    assert_eq!(
        jobs.len(),
        1,
        "exactly one follow-up is scheduled: {jobs:?}"
    );
    assert_eq!(jobs[0].0, IMPROVEMENT_KIND);
    let prefix = format!("benchmark:improvement:{candidate}:");
    assert!(
        jobs[0].1.starts_with(&prefix),
        "{:?} must carry the tick behind {prefix:?}",
        jobs[0].1
    );
}
