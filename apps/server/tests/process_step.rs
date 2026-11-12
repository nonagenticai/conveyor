//! S11 step 8 — the worker's `process.step` dispatch.
//!
//! `crates/actions/tests/process_step.rs` covers the executor's behaviour; this file covers the
//! arm that dispatches it: a well-formed payload advances a queued step to a succeeded run, and a
//! payload missing a field is a PERMANENT job failure (never a retry loop over a bug).

mod common;

use conveyor_api::Config;
use conveyor_server::worker;
use serde_json::json;
use uuid::Uuid;

fn config() -> Config {
    Config::from_lookup(|_| None).expect("a default config")
}

async fn step_status(db: &common::TestDb, run: Uuid, position: i32) -> (String, Option<String>) {
    sqlx::query_as(
        "SELECT status, error FROM run_step
          WHERE run_id = $1 AND position = $2 AND attempt = 1",
    )
    .bind(run)
    .bind(position)
    .fetch_one(&db.super_pool)
    .await
    .expect("the run step")
}

async fn run_status(db: &common::TestDb, run: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, error_class FROM run WHERE id = $1")
        .bind(run)
        .fetch_one(&db.super_pool)
        .await
        .expect("the run")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_worker_advances_a_queued_step_job_to_a_succeeded_run() {
    let db = common::TestDb::fresh().await;
    let graph = common::process_fixture::seed(
        &db,
        "s11-worker",
        &[
            ("cards.get", json!({"id": "{{card.id}}"})),
            ("cards.get", json!({"id": "{{card.id}}"})),
        ],
        "running",
    )
    .await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "process.step",
        &json!({"run_id": graph.run, "position": 1, "attempt": 1}),
    )
    .await
    .expect("step 1 runs");

    let (status, error) = step_status(&db, graph.run, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    let input: serde_json::Value = sqlx::query_scalar(
        "SELECT input FROM run_step WHERE run_id = $1 AND position = 1 AND attempt = 1",
    )
    .bind(graph.run)
    .fetch_one(&db.super_pool)
    .await
    .expect("the recorded input");
    assert_eq!(input["id"], json!(graph.card), "step 1 resolved card.id");
    // The record committed the next position's job; the run is still running.
    assert_eq!(run_status(&db, graph.run).await.0, "running");
    let (next, _) = step_status(&db, graph.run, 2).await;
    assert_eq!(next, "queued");

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "process.step",
        &json!({"run_id": graph.run, "position": 2, "attempt": 1}),
    )
    .await
    .expect("step 2 runs");

    let (status, error) = step_status(&db, graph.run, 2).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(run_status(&db, graph.run).await.0, "succeeded");

    let events: Vec<String> = sqlx::query_scalar(
        "SELECT type FROM event WHERE tenant_id = $1 AND topic = $2 ORDER BY seq",
    )
    .bind(graph.tenant.as_uuid())
    .bind(format!("run:{}", graph.run))
    .fetch_all(&db.super_pool)
    .await
    .expect("run events");
    assert_eq!(
        events,
        [
            "run_step.changed",
            "run_step.changed",
            // The chain's next-step job is announced beside the record that enqueued it.
            "run_step.requested",
            "run_step.changed",
            "run_step.changed",
            "run.succeeded"
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_step_payload_is_permanent() {
    let db = common::TestDb::fresh().await;
    let graph = common::process_fixture::seed(
        &db,
        "s11-worker-bad",
        &[("cards.get", json!({"id": "{{card.id}}"}))],
        "running",
    )
    .await;

    let failure = worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "process.step",
        &json!({"run_id": graph.run}),
    )
    .await
    .expect_err("the payload has no position");
    assert!(failure.permanent, "{failure:?}");
    // Nothing was claimed: the step is still queued.
    let (status, _) = step_status(&db, graph.run, 1).await;
    assert_eq!(status, "queued");
}
