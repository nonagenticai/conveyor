//! S11 step 9 — the `approval.expire` worker path for process gates.
//!
//! The decision path (`approved`/`rejected`) is covered in `conveyor-actions`'
//! `process_step` tests. This test covers the other entry point: a due gate is expired by the
//! maintenance sweep and the gated step's `on_result` is applied in the same transaction, instead
//! of the generic "cancel the run" path a non-process gate takes.

mod common;

use conveyor_api::Config;
use conveyor_server::worker;
use serde_json::json;

fn config() -> Config {
    Config::from_lookup(|_| None).expect("a default config")
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_process_gate_applies_the_steps_on_error_in_the_worker() {
    let db = common::TestDb::fresh().await;
    let graph = common::process_fixture::seed(
        &db,
        "s11-gate-expire",
        &[
            ("cards.get", json!({"id": "{{card.id}}"})),
            ("cards.get", json!({"id": "{{card.id}}"})),
        ],
        "running",
    )
    .await;

    // Position 1 becomes a gated step, the run parks, and a due gate exists. Seeding the
    // approval directly is the state `open_gate_with_meta_tx` commits; the test is about the
    // sweep that resolves it, not about opening it (that is the actions test's job).
    sqlx::query(
        "UPDATE process_step SET gate_kind = 'human_approval', on_result = 'continue'
          WHERE id = $1",
    )
    .bind(graph.steps[0])
    .execute(&db.super_pool)
    .await
    .expect("gate the step");
    sqlx::query("UPDATE run SET status = 'waiting_approval' WHERE id = $1")
        .bind(graph.run)
        .execute(&db.super_pool)
        .await
        .expect("park the run");
    sqlx::query(
        "INSERT INTO approval
             (tenant_id, run_id, card_id, project_id, gate, kind, status,
              required_decisions, expires_at, meta)
         SELECT tenant_id, id, card_id, project_id, 'human_approval', 'process', 'pending',
                1, now() - interval '1 minute', '{\"position\":1,\"attempt\":1}'::jsonb
           FROM run WHERE id = $1",
    )
    .bind(graph.run)
    .execute(&db.super_pool)
    .await
    .expect("seed the due gate");

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        graph.tenant,
        "approval.expire",
        &json!({}),
    )
    .await
    .expect("the expiry sweep runs");

    let (status, error): (String, Option<String>) = sqlx::query_as(
        "SELECT status, error FROM run_step
          WHERE run_id = $1 AND position = 1 AND attempt = 1",
    )
    .bind(graph.run)
    .fetch_one(&db.super_pool)
    .await
    .expect("the gated step");
    assert_eq!(status, "skipped", "{error:?}");
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("process.gate_expired")),
        "the expiry is kept on the row, got {error:?}"
    );

    let run_status: String = sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(graph.run)
        .fetch_one(&db.super_pool)
        .await
        .expect("the run");
    assert_eq!(
        run_status, "running",
        "on_result = continue keeps the run alive; a generic gate would have been cancelled"
    );

    let next_job: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE kind = 'process.step' AND payload->>'run_id' = $1
            AND payload->>'position' = '2' AND status IN ('pending', 'running')",
    )
    .bind(graph.run.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("the next position's job");
    assert_eq!(next_job, 1, "the chain continues at position 2");
}
