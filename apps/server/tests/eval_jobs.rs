//! S19 Part B step 10 — `eval.run.tick` finalises, `eval.reap` abandons.
//!
//! ⛔ **N×R jobs finalise nothing.** No single case knows it was the last one, so without the
//! tick a run stays `running` forever after its final result lands. The tick answers that from
//! two derived counts, never from a stored counter a lost driver could leave stale.
//!
//! The reaper's key is `last_polled_at`, **never case silence** — a run whose cases are quiet is
//! a run whose cases are slow (the S17 lesson). Both are proved here by backdating one run and
//! leaving another live, in the same tenant, in the same pass.

mod common;

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_server::worker;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

#[path = "eval_support/mod.rs"]
mod fx;

fn config() -> conveyor_server::Config {
    conveyor_server::Config::from_lookup(|_| None).expect("a default config")
}

async fn run_kind(db: &common::TestDb, tenant: TenantId, kind: &str) {
    worker::run_kind(&db.runtime, &config(), None, tenant, kind, &json!({}))
        .await
        .unwrap_or_else(|error| panic!("{kind}: {error:?}"));
}

/// One `eval.case.run` job for `(case, rep)`, exactly as `eval_runs.start` writes it.
async fn enqueue_case_job(db: &common::TestDb, fixture: &fx::Fixture, case: Uuid) {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let dedupe = format!("{}:{}:0", fixture.run_id, case);
    let payload = json!({"run_id": fixture.run_id, "case_id": case, "rep": 0});
    store::jobs::enqueue_tx(
        &mut tx,
        fixture.tenant,
        "eval.case.run",
        &dedupe,
        payload,
        None,
        5,
    )
    .await
    .expect("enqueue the case job");
    tx.commit().await.expect("commit");
}

struct RunRow {
    status: String,
    last_polled_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

async fn read_run(db: &common::TestDb, tenant: TenantId, run_id: Uuid) -> RunRow {
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let row: (String, Option<DateTime<Utc>>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, last_polled_at, finished_at FROM eval_run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read the run");
    tx.commit().await.expect("commit");
    RunRow {
        status: row.0,
        last_polled_at: row.1,
        finished_at: row.2,
    }
}

#[tokio::test]
async fn the_tick_heartbeats_while_work_outstanding_and_finalises_when_it_is_not() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: one", "classify: two"]).await;
    for case in &fixture.cases {
        enqueue_case_job(&db, &fixture, *case).await;
    }

    // ── pass 1: two jobs outstanding, nothing measured ──
    run_kind(&db, fixture.tenant, "eval.run.tick").await;
    let first = read_run(&db, fixture.tenant, fixture.run_id).await;
    assert_eq!(
        first.status, "queued",
        "a run with outstanding jobs is not finalised"
    );
    assert!(
        first.last_polled_at.is_some(),
        "⛔ the heartbeat is the tick's job: without it the reaper would abandon a live run"
    );
    assert!(first.finished_at.is_none());

    // ── the cases run, and their jobs complete ──
    for case in &fixture.cases {
        conveyor_server::eval_runner::run_case(
            &db.runtime,
            "conveyor",
            fixture.tenant,
            fixture.run_id,
            *case,
            0,
        )
        .await
        .expect("the driver ran the case");
    }
    loop {
        let Some(job) = store::jobs::claim(&db.runtime, fixture.tenant, 60)
            .await
            .expect("claim a job")
        else {
            break;
        };
        store::jobs::finish_succeeded(
            &db.runtime,
            fixture.tenant,
            job.id,
            job.lease_token.expect("a claimed job holds a lease token"),
        )
        .await
        .expect("finish the job");
    }
    assert_eq!(fx::results(&db, &fixture).await.len(), 2);

    // ── pass 2: nothing outstanding, every slot filled ──
    run_kind(&db, fixture.tenant, "eval.run.tick").await;
    let done = read_run(&db, fixture.tenant, fixture.run_id).await;
    assert_eq!(done.status, "complete", "the tick is what finalises a run");
    assert!(
        done.finished_at.is_some(),
        "a complete run records when it finished"
    );
}

#[tokio::test]
async fn a_run_whose_slots_exhausted_their_attempts_finalises_partial() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: one", "classify: two"]).await;
    for case in &fixture.cases {
        enqueue_case_job(&db, &fixture, *case).await;
    }

    // One case lands.
    conveyor_server::eval_runner::run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        fixture.cases[0],
        0,
    )
    .await
    .expect("the driver ran the first case");

    // Both jobs leave the outstanding set: one succeeded, one died permanently.
    let mut finished_one = false;
    loop {
        let Some(job) = store::jobs::claim(&db.runtime, fixture.tenant, 60)
            .await
            .expect("claim a job")
        else {
            break;
        };
        let token = job.lease_token.expect("a claimed job holds a lease token");
        if !finished_one {
            finished_one = true;
            store::jobs::finish_succeeded(&db.runtime, fixture.tenant, job.id, token)
                .await
                .expect("finish the job");
        } else {
            // `retry_after` zero with the attempt count already at its ceiling is the dead path.
            store::jobs::fail(
                &db.runtime,
                fixture.tenant,
                job.id,
                token,
                "the subject refused this case five times",
                std::time::Duration::ZERO,
            )
            .await
            .expect("fail the job");
        }
    }

    run_kind(&db, fixture.tenant, "eval.run.tick").await;
    let run = read_run(&db, fixture.tenant, fixture.run_id).await;
    assert_eq!(
        run.status, "partial",
        "the work is over and a slot is empty: `partial` says exactly that, and the attempt \
         errors stay readable"
    );
    assert_ne!(run.status, "complete", "a missing slot is never a pass");
    assert_ne!(run.status, "void", "something WAS measured");
}

#[tokio::test]
async fn the_reaper_abandons_the_unpolled_run_and_leaves_the_live_one_alone() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: one"]).await;

    // A second run on the same instrument, so both are in flight for the same tenant and the
    // same pass: the only difference between them is when they were last polled.
    let environment = json!({});
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let stale_run = store::eval::insert_run_tx(
        &mut tx,
        fixture.tenant,
        &store::eval::NewRun {
            eval_id: fixture.eval_id,
            version_id: fixture.version_id,
            variant: "v1",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: 1,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the second run")
    .id;
    sqlx::query(
        "UPDATE eval_run SET last_polled_at = now() - interval '10 minutes'
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(stale_run)
    .execute(&mut *tx)
    .await
    .expect("backdate the abandoned run");
    // The live one was polled recently; it must survive the same pass.
    sqlx::query("UPDATE eval_run SET last_polled_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(fixture.tenant.as_uuid())
        .bind(fixture.run_id)
        .execute(&mut *tx)
        .await
        .expect("refresh the live run");
    tx.commit().await.expect("commit");

    run_kind(&db, fixture.tenant, "eval.reap").await;

    let abandoned = read_run(&db, fixture.tenant, stale_run).await;
    assert_eq!(
        abandoned.status, "void",
        "ten minutes without a tick is not a slow run, it is an unattended one"
    );
    let live = read_run(&db, fixture.tenant, fixture.run_id).await;
    assert_eq!(
        live.status, "queued",
        "⛔ the reaper keys on `last_polled_at`, never on case silence: a run its own cases have \
         not touched yet is still running"
    );
}
