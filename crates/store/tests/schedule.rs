//! Schedule seeding, cron materialisation and the one-job-under-concurrency property.
//!
//! `cargo test -p conveyor-store --test schedule -- --nocapture`.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use chrono::Utc;
use conveyor_domain::TenantId;
use conveyor_store::schedule::{self, DEFAULT_SCHEDULES};
use sqlx::PgPool;
use uuid::Uuid;

async fn tenants(db: &common::TestDb) -> Vec<TenantId> {
    let mut out = Vec::new();
    for (slug, name) in [("sched-a", "Sched A"), ("sched-b", "Sched B")] {
        let id: Uuid =
            sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
                .bind(slug)
                .bind(name)
                .fetch_one(&db.super_pool)
                .await
                .expect("seed a tenant");
        out.push(TenantId::from_uuid(id).expect("non-nil"));
    }
    out
}

async fn schedule_count(pool: &PgPool, tenant: TenantId) -> i64 {
    let mut tx = conveyor_store::begin_scoped(pool, tenant)
        .await
        .expect("scope");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM schedule")
        .fetch_one(&mut *tx)
        .await
        .expect("count schedules");
    tx.commit().await.expect("commit");
    count
}

async fn make_due(db: &common::TestDb, tenant: TenantId, kind: &str) {
    sqlx::query("UPDATE schedule SET next_run_at = now() - interval '1 second' WHERE tenant_id = $1 AND kind = $2")
        .bind(tenant.as_uuid())
        .bind(kind)
        .execute(&db.super_pool)
        .await
        .expect("make the schedule due");
}

async fn live_jobs(pool: &PgPool, tenant: TenantId, kind: &str) -> i64 {
    let mut tx = conveyor_store::begin_scoped(pool, tenant)
        .await
        .expect("scope");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE kind = $1 AND status IN ('pending', 'running')",
    )
    .bind(kind)
    .fetch_one(&mut *tx)
    .await
    .expect("count jobs");
    tx.commit().await.expect("commit");
    count
}

#[tokio::test]
async fn defaults_are_seeded_once_per_tenant_and_are_idempotent() {
    let db = common::TestDb::fresh().await;
    let tenants = tenants(&db).await;
    let pool = &db.runtime;

    let inserted = schedule::seed_defaults(pool).await.expect("seed defaults");
    println!(
        "seed_defaults inserted {inserted} rows for {} tenants",
        tenants.len()
    );
    assert_eq!(inserted, tenants.len() * DEFAULT_SCHEDULES.len());
    for tenant in &tenants {
        assert_eq!(
            schedule_count(pool, *tenant).await,
            DEFAULT_SCHEDULES.len() as i64
        );
    }

    let again = schedule::seed_defaults(pool).await.expect("seed again");
    assert_eq!(again, 0, "a second run inserts nothing");

    // The seeded rows are due in the future, so a materialise at `now` creates nothing.
    let created = schedule::materialise(pool, tenants[0], Utc::now())
        .await
        .expect("materialise");
    assert!(created.is_empty(), "future schedules create no job");
}

#[tokio::test]
async fn a_due_schedule_materialises_one_job_and_advances() {
    let db = common::TestDb::fresh().await;
    let tenant = tenants(&db).await[0];
    let pool = &db.runtime;
    schedule::seed_defaults(pool).await.expect("seed");

    make_due(&db, tenant, "licence.sync").await;
    let now = Utc::now();
    let created = schedule::materialise(pool, tenant, now)
        .await
        .expect("materialise");
    println!("materialised {} job(s): {created:?}", created.len());
    assert_eq!(created.len(), 1);
    assert_eq!(live_jobs(pool, tenant, "licence.sync").await, 1);

    // The next tick is in the future: the same `now` creates nothing and the second call
    // with the SAME instant is a no-op even if the clock does not move.
    let created_again = schedule::materialise(pool, tenant, now)
        .await
        .expect("materialise again");
    assert!(created_again.is_empty());
    assert_eq!(live_jobs(pool, tenant, "licence.sync").await, 1);

    let (last_run_at, next_run_at): (Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>) =
        sqlx::query_as("SELECT last_run_at, next_run_at FROM schedule WHERE tenant_id = $1 AND kind = 'licence.sync'")
            .bind(tenant.as_uuid())
            .fetch_one(&db.super_pool)
            .await
            .expect("schedule row");
    assert!(last_run_at.is_some());
    assert!(next_run_at > now, "next_run_at advanced: {next_run_at}");
}

#[tokio::test]
async fn two_concurrent_materialisers_create_exactly_one_job() {
    let db = common::TestDb::fresh().await;
    let tenant = tenants(&db).await[0];
    let pool = &db.runtime;
    schedule::seed_defaults(pool).await.expect("seed");
    make_due(&db, tenant, "event.prune").await;

    let now = Utc::now();
    let (left, right) = tokio::join!(
        schedule::materialise(pool, tenant, now),
        schedule::materialise(pool, tenant, now)
    );
    let left = left.expect("left materialiser");
    let right = right.expect("right materialiser");
    println!(
        "concurrent materialise: left={} right={}",
        left.len(),
        right.len()
    );
    assert_eq!(
        left.len() + right.len(),
        1,
        "the advisory lock serialises the two runs; exactly one creates the job"
    );
    assert_eq!(live_jobs(pool, tenant, "event.prune").await, 1);
}

#[tokio::test]
async fn a_pending_job_from_the_previous_tick_suppresses_the_next_job() {
    let db = common::TestDb::fresh().await;
    let tenant = tenants(&db).await[0];
    let pool = &db.runtime;
    schedule::seed_defaults(pool).await.expect("seed");
    // Wall-clock independence: park every seeded schedule except `job.prune` one day out.
    // The 5-minute-boundary case is reproduced deliberately below, never waited for.
    sqlx::query(
        "UPDATE schedule SET next_run_at = now() + interval '1 day'
          WHERE tenant_id = $1 AND kind <> 'job.prune'",
    )
    .bind(tenant.as_uuid())
    .execute(&db.super_pool)
    .await
    .expect("park the other seeded schedules");

    make_due(&db, tenant, "job.prune").await;
    assert_eq!(
        schedule::materialise(pool, tenant, Utc::now())
            .await
            .expect("first tick")
            .len(),
        1
    );
    // Force the schedule due again without completing the job.
    make_due(&db, tenant, "job.prune").await;
    // The case that used to turn this suite red one minute in five: another seeded schedule
    // (`licence.sync`, `*/5 * * * *`) is due on the same tick. It must still create its job;
    // only `job.prune` is suppressed.
    make_due(&db, tenant, "licence.sync").await;
    let second = schedule::materialise(pool, tenant, Utc::now() + chrono::Duration::minutes(1))
        .await
        .expect("second tick");
    // The created set is NOT necessarily empty: at a 5-minute boundary the seeded
    // `licence.sync` (`*/5 * * * *`) is due too. The property under test is narrower — the
    // SAME schedule (`job.prune`) did not produce a second job while its first is live.
    let second_kinds: Vec<String> = if second.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT kind FROM job WHERE id = ANY($1) ORDER BY kind")
            .bind(second.as_slice())
            .fetch_all(&db.super_pool)
            .await
            .expect("read the jobs the second tick created")
    };
    println!("second tick created {second:?} {second_kinds:?} (job.prune deduped by the live job)");
    assert!(
        second_kinds.iter().any(|kind| kind == "licence.sync"),
        "the boundary simulation must materialise the other due schedule: {second_kinds:?}"
    );
    assert!(
        second_kinds.iter().all(|kind| kind != "job.prune"),
        "the same schedule must not create a second job: {second_kinds:?}"
    );
    assert_eq!(live_jobs(pool, tenant, "job.prune").await, 1);
}
