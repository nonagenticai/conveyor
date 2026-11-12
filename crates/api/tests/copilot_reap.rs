//! S10 §5.4/§6 step 12: a `streaming` turn older than 300 s is interrupted, once.
//!
//! The stranded row is inserted directly (a killed process writes no terminal update), then the real
//! reaper runs. It must flip the turn, emit its event, and be a no-op on the second run — the job is
//! a schedule row that fires every minute.

mod copilot;

use uuid::Uuid;

use conveyor_domain::copilot::REAP_AFTER_SECS;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_streaming_turn_older_than_the_window_is_interrupted_once() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;

    // A turn a killed process left `streaming`, six minutes ago.
    let turn_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO conversation_turn
             (id, tenant_id, conversation_id, seq, role, body, meta, created_at, updated_at)
         VALUES ($1, $2, $3, 1, 'assistant', '', '{\"status\":\"streaming\"}',
                 now() - interval '6 minutes', now() - interval '6 minutes')",
    )
    .bind(turn_id)
    .bind(seeded.tenant_id)
    .bind(conversation)
    .execute(&pg.super_pool)
    .await
    .expect("insert the stranded turn");

    let reaped =
        conveyor_api::copilot::turns::reap(&running.state.pool, seeded.tenant, REAP_AFTER_SECS)
            .await
            .expect("reap");
    assert_eq!(reaped, 1, "the stranded turn must be reaped");

    let meta = copilot::turn_meta(&pg, turn_id).await;
    assert_eq!(meta["status"], "interrupted", "{meta}");

    // The reap is durable: the turn's event is written with it.
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM event WHERE topic = $1")
        .bind(format!("conversation:{conversation}"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("count events");
    assert_eq!(events, 1, "an interrupted turn is announced once");

    // A second tick finds nothing: the reaper is idempotent.
    let again =
        conveyor_api::copilot::turns::reap(&running.state.pool, seeded.tenant, REAP_AFTER_SECS)
            .await
            .expect("second reap");
    assert_eq!(again, 0);

    // A fresh streaming turn is NOT reaped: the window is 300 s, not "any streaming turn".
    let fresh = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO conversation_turn
             (id, tenant_id, conversation_id, seq, role, body, meta)
         VALUES ($1, $2, $3, 2, 'assistant', '', '{\"status\":\"streaming\"}')",
    )
    .bind(fresh)
    .bind(seeded.tenant_id)
    .bind(conversation)
    .execute(&pg.super_pool)
    .await
    .expect("insert a fresh turn");
    assert_eq!(
        conveyor_api::copilot::turns::reap(&running.state.pool, seeded.tenant, REAP_AFTER_SECS)
            .await
            .expect("third reap"),
        0
    );
    assert_eq!(copilot::turn_meta(&pg, fresh).await["status"], "streaming");
}
