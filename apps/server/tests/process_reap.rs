//! S11 step 8 — `run.reap` closes a process run whose driving `process.step` job died.
//!
//! A worker killed with `SIGKILL` cannot write anything. Its job is reclaimed by the lease, and
//! once the attempts are spent it is dead-lettered — but a process run is driven only by
//! `process.step`, so `reap_dead_runs_tx` (which matches `run.execute`) cannot see it. The run
//! would stay `running` forever and hold `uq_run_live` on its card. These tests prove the second
//! half of the sweep, and that a LIVE job is never touched.

mod common;

use conveyor_api::Config;
use conveyor_server::worker;
use serde_json::json;

fn config() -> Config {
    Config::from_lookup(|_| None).expect("a default config")
}

async fn statuses(db: &common::TestDb, run: uuid::Uuid) -> Vec<(i32, String)> {
    sqlx::query_as(
        "SELECT position, status FROM run_step WHERE run_id = $1 ORDER BY position, attempt",
    )
    .bind(run)
    .fetch_all(&db.super_pool)
    .await
    .expect("the run steps")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dead_step_job_is_reaped_as_crashed_and_frees_the_card() {
    let db = common::TestDb::fresh().await;
    let graph = common::process_fixture::seed(
        &db,
        "s11-reap",
        &[
            ("cards.get", json!({"id": "{{card.id}}"})),
            ("cards.get", json!({"id": "{{card.id}}"})),
        ],
        "dead",
    )
    .await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "run.reap",
        &json!({}),
    )
    .await
    .expect("the reap sweep runs");

    let (status, error_class) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, error_class FROM run WHERE id = $1",
    )
    .bind(graph.run)
    .fetch_one(&db.super_pool)
    .await
    .expect("the run");
    assert_eq!(status, "failed");
    assert_eq!(error_class.as_deref(), Some("crashed"));

    // The job named position 1: that step failed, the rest were never reached.
    assert_eq!(
        statuses(&db, graph.run).await,
        vec![(1, "failed".to_owned()), (2, "skipped".to_owned())]
    );
    let error: Option<String> =
        sqlx::query_scalar("SELECT error FROM run_step WHERE run_id = $1 AND position = 1")
            .bind(graph.run)
            .fetch_one(&db.super_pool)
            .await
            .expect("the failed step's error");
    assert_eq!(error.as_deref(), Some("the process.step job died"));

    // Both step transitions and the run failure are on the run's topic.
    let events: Vec<(String, String)> = sqlx::query_as(
        "SELECT type, coalesce(payload->>'status', payload->>'error_class')
           FROM event WHERE tenant_id = $1 AND topic = $2 ORDER BY seq",
    )
    .bind(graph.tenant.as_uuid())
    .bind(format!("run:{}", graph.run))
    .fetch_all(&db.super_pool)
    .await
    .expect("run events");
    assert_eq!(
        events,
        vec![
            ("run_step.changed".to_owned(), "failed".to_owned()),
            ("run_step.changed".to_owned(), "skipped".to_owned()),
            ("run.failed".to_owned(), "crashed".to_owned()),
        ]
    );

    // The card's live-run slot is free: a second run on the same card is accepted. Before the
    // reap this INSERT would be refused by `uq_run_live`.
    sqlx::query(
        "INSERT INTO run (tenant_id, card_id, project_id, status, attempt, process_id, inputs)
         SELECT tenant_id, card_id, project_id, 'running', 2, process_id, '{}'::jsonb
           FROM run WHERE id = $1",
    )
    .bind(graph.run)
    .execute(&db.super_pool)
    .await
    .expect("the card can start another run");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_step_job_is_not_reaped() {
    let db = common::TestDb::fresh().await;
    let graph = common::process_fixture::seed(
        &db,
        "s11-live",
        &[("cards.get", json!({"id": "{{card.id}}"}))],
        "pending",
    )
    .await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "run.reap",
        &json!({}),
    )
    .await
    .expect("the reap sweep runs");

    let status: String = sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(graph.run)
        .fetch_one(&db.super_pool)
        .await
        .expect("the run");
    assert_eq!(status, "running", "a live job still drives the run");
    assert_eq!(
        statuses(&db, graph.run).await,
        vec![(1, "queued".to_owned())]
    );
}

/// A run parked on a gate is NOT reaped even when its `process.step` job is dead.
///
/// The reaper's predicate is `r.status IN ('queued','running')`, so `waiting_approval` is
/// outside the sweep: the run is waiting for a human, not for a worker, and its job is
/// expected to be gone (the gate opened before anything was claimed). Reaping it would crash
/// a run a person can still approve. Pinned here because the SQL predicate is the only
/// guard (step-9 handoff, open question 3).
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_approval_run_with_a_dead_job_is_not_reaped() {
    let db = common::TestDb::fresh().await;
    let graph = common::process_fixture::seed(
        &db,
        "s11-waiting",
        &[("cards.get", json!({"id": "{{card.id}}"}))],
        "dead",
    )
    .await;
    // Park the run exactly as a gate does (`open_gate_tx` moves running → waiting_approval).
    sqlx::query("UPDATE run SET status = 'waiting_approval' WHERE id = $1")
        .bind(graph.run)
        .execute(&db.super_pool)
        .await
        .expect("park the run");

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "run.reap",
        &json!({}),
    )
    .await
    .expect("the reap sweep runs");

    let status: String = sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(graph.run)
        .fetch_one(&db.super_pool)
        .await
        .expect("the run");
    assert_eq!(
        status, "waiting_approval",
        "a gate is waiting for a human, not for a worker"
    );
    assert_eq!(
        statuses(&db, graph.run).await,
        vec![(1, "queued".to_owned())],
        "the parked step is untouched"
    );
}
