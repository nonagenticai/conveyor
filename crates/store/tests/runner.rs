//! The runner's attempt ledger: the stale-attempt fence, the continuation that does not spend a
//! retry, and the cascade that keeps an attempt's artefacts attached to its own run.
//!
//! `cargo test -p conveyor-store --test runner -- --nocapture`. Every assertion prints the row
//! state it read, so the evidence is the output and not only a green tick.
//!
//! The harness is `apps/server/tests/common/mod.rs` by `#[path]`, exactly as `jobs.rs` uses it:
//! one ephemeral Postgres per test binary, one fresh database per test, migrations applied through
//! the binary's own migrator.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use std::time::Duration;

use conveyor_domain::TenantId;
use conveyor_store::jobs;
use conveyor_store::runner::{self, ArtefactRow, Inserted, NewRun};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_tenant(db: &common::TestDb) -> TenantId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('runner-a', 'Runner A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

/// Enqueue and claim one runner job; returns the job id and the claimed row.
async fn a_claimed_job(pool: &PgPool, tenant: TenantId) -> conveyor_store::jobs::Job {
    jobs::enqueue(
        pool,
        tenant,
        "media.compile",
        &format!("k-{}", Uuid::now_v7()),
        json!({ "document_id": Uuid::now_v7() }),
        None,
        3,
    )
    .await
    .expect("enqueue")
    .expect("a fresh dedupe key is free");
    jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the job is due")
}

fn new_run(job_id: Uuid, attempt: i32) -> NewRun<'static> {
    NewRun {
        job_id,
        attempt,
        image: "registry.invalid/conveyor-media-ffmpeg:test",
        namespace: "conveyor-runners",
        job_name: "run-test",
        instruction: json!({ "schema": "runner/v1", "job_id": job_id, "attempt": attempt }),
    }
}

async fn job_state(pool: &PgPool, tenant: TenantId, id: Uuid) -> (String, i32) {
    // `job` is under RLS: an unscoped pool reads nothing, so the assertion scopes its own read.
    let mut tx = conveyor_store::scope::begin_scoped(pool, tenant)
        .await
        .expect("scope the read");
    let row = sqlx::query_as("SELECT status, attempts FROM job WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .expect("read the job row");
    tx.rollback().await.expect("roll back the read scope");
    row
}

/// P4.1: attempt 1 superseded by attempt 2 — attempt 1's result arrives and writes nothing.
#[tokio::test]
async fn stale_attempt_result_is_rejected() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let job = a_claimed_job(pool, tenant).await;
    assert_eq!(job.attempts, 1, "the first claim counts one attempt");
    runner::begin(pool, tenant, &new_run(job.id, job.attempts))
        .await
        .expect("begin attempt 1")
        .expect("a new (job_id, attempt) inserts");

    // The pod dies; the ledger spends the attempt and hands the job to attempt 2.
    jobs::fail(
        pool,
        tenant,
        job.id,
        job.lease_token.expect("a claimed job holds a lease token"),
        "the pod never reported",
        Duration::from_secs(0),
    )
    .await
    .expect("fail attempt 1");
    let second = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the retry is due");
    assert_eq!(second.attempts, 2, "the retry counts a second attempt");
    runner::begin(pool, tenant, &new_run(second.id, second.attempts))
        .await
        .expect("begin attempt 2")
        .expect("attempt 2 is a new key");

    // Attempt 1's pod finally posts. It is stale, so it must write nothing.
    let accepted =
        runner::record_result(pool, tenant, job.id, 1, &json!({ "status": "succeeded" }))
            .await
            .expect("the stale write runs");
    assert!(!accepted, "a superseded attempt's result is refused");

    let (status, attempts) = job_state(pool, tenant, job.id).await;
    println!("job after the stale write: status={status} attempts={attempts}");
    assert_eq!(status, "running", "the job row is untouched");
    assert_eq!(attempts, 2, "the attempt counter is untouched");

    let first = runner::attempt(pool, tenant, job.id, 1)
        .await
        .expect("read attempt 1")
        .expect("attempt 1 exists");
    assert_eq!(
        first.status, "dispatched",
        "the refused write left attempt 1's own row alone"
    );
    assert!(first.result.is_none(), "no result was stored");
    assert!(!first.is_terminal(), "attempt 1 is still live");

    let second = runner::attempt(pool, tenant, job.id, 2)
        .await
        .expect("read attempt 2")
        .expect("attempt 2 exists");
    assert!(!second.is_terminal(), "attempt 2 has not reported yet");
}

/// A continuation is the SAME attempt: `defer` must not spend a retry, while a real failure must.
#[tokio::test]
async fn a_continuation_keeps_the_attempt_number() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let job = a_claimed_job(pool, tenant).await;
    assert_eq!(job.attempts, 1);
    let token = job.lease_token.expect("a claimed job holds a lease token");

    let deferred = jobs::defer(pool, tenant, job.id, token, Duration::from_secs(0))
        .await
        .expect("defer");
    assert!(deferred, "the holder's defer is accepted");

    let again = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the continuation is due");
    println!("continuation claim: attempts={}", again.attempts);
    assert_eq!(
        again.attempts, 1,
        "polling the same attempt is not a retry, so the pod's idempotency key does not move"
    );

    // A genuine failure still spends one: the next claim is attempt 2.
    jobs::fail(
        pool,
        tenant,
        again.id,
        again.lease_token.expect("lease"),
        "the pod never started",
        Duration::from_secs(0),
    )
    .await
    .expect("fail");
    let retry = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the retry is due");
    assert_eq!(retry.attempts, 2, "a failure counts a real attempt");
}

/// P4: artefacts hang off their own run and die with the job.
#[tokio::test]
async fn artefacts_cascade_with_the_run() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let job = a_claimed_job(pool, tenant).await;
    let run = runner::begin(pool, tenant, &new_run(job.id, job.attempts))
        .await
        .expect("begin")
        .expect("a new key inserts");

    let artefact = ArtefactRow {
        name: "video.mp4".to_owned(),
        sha256: "a".repeat(64),
        bytes: 1024,
        content_type: "video/mp4".to_owned(),
        object_key: format!("t/{}/jobs/{}/1/video.mp4", tenant.as_uuid(), job.id),
    };
    assert_eq!(
        runner::insert_artefact(pool, tenant, run.id, &artefact)
            .await
            .expect("insert"),
        Inserted::New
    );
    assert_eq!(
        runner::insert_artefact(pool, tenant, run.id, &artefact)
            .await
            .expect("insert again"),
        Inserted::Repeated,
        "the same bytes under the same name are an idempotent retry"
    );
    let mut changed = artefact.clone();
    changed.sha256 = "b".repeat(64);
    assert_eq!(
        runner::insert_artefact(pool, tenant, run.id, &changed)
            .await
            .expect("insert different bytes"),
        Inserted::Conflict,
        "a name may not silently change its bytes"
    );

    assert_eq!(
        runner::artefacts(pool, tenant, run.id)
            .await
            .expect("list")
            .len(),
        1
    );

    // The job is the parent: deleting it takes the run and its artefacts with it.
    sqlx::query("DELETE FROM job WHERE id = $1")
        .bind(job.id)
        .execute(&db.super_pool)
        .await
        .expect("delete the job as the superuser");
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM runner_run WHERE job_id = $1")
        .bind(job.id)
        .fetch_one(&db.super_pool)
        .await
        .expect("count runs");
    let artefacts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner_artefact WHERE run_id = $1")
            .bind(run.id)
            .fetch_one(&db.super_pool)
            .await
            .expect("count artefacts");
    println!("after the job delete: runs={runs} artefacts={artefacts}");
    assert_eq!(runs, 0, "the attempt row cascades");
    assert_eq!(artefacts, 0, "the artefacts cascade through the run");
}
