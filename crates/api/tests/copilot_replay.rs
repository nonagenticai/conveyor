//! S10 §7 (d): a reconnect with `Last-Event-ID` receives the turn exactly once.
//!
//! The stream is a real HTTP subscription to the served router, so the doorbell, the tenant-scoped
//! replay and the topic filter are all exercised. The turn event is the only durable frame: deltas
//! are not recorded, which is why a replay cannot fabricate a partial turn (S10-D4).

mod copilot;

use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_after_the_cursor_receives_the_turn_exactly_once() {
    let pg = copilot::support::Pg::fresh().await;
    let _seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;
    let topic = format!("conversation:{conversation}");

    // Subscribe before the turn, exactly as a live client does.
    let mut live = copilot::Sse::open(&running.base(), "dev:alice", &topic, None).await;

    model.text("Live answer.");
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "hello").await;
    assert_eq!(status, 201, "{body}");
    let turn_id = body["turn"]["id"].as_str().expect("turn id").to_owned();

    let frame = live
        .next(Duration::from_secs(5))
        .await
        .expect("the turn event must arrive live");
    assert_eq!(frame.event, "conversation_turn.created");
    // The frame's data is the event row; the turn is its payload.
    assert_eq!(frame.data["topic"], topic.as_str());
    assert_eq!(frame.data["payload"]["turn"]["id"], turn_id);
    assert_eq!(frame.data["payload"]["turn"]["meta"]["status"], "completed");
    let seq = frame.id.expect("every event carries its outbox seq");

    // Reconnect after that seq: nothing new, so the turn is delivered once.
    let mut replay = copilot::Sse::open(&running.base(), "dev:alice", &topic, Some(seq)).await;
    assert!(
        replay.next(Duration::from_millis(700)).await.is_none(),
        "a cursor at the turn's own seq must not replay it"
    );

    // One durable event for the conversation, and its seq monotonic.
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT seq, type FROM event WHERE topic = $1 ORDER BY seq")
            .bind(&topic)
            .fetch_all(&pg.super_pool)
            .await
            .expect("read the outbox");
    assert_eq!(rows.len(), 1, "one terminal turn, one event: {rows:?}");
    assert_eq!(rows[0].0, seq);
    assert_eq!(rows[0].1, "conversation_turn.created");

    // A second turn's event has a strictly larger seq — the replay cursor stays meaningful.
    model.text("Second answer.");
    let (status, second) = copilot::turn(&running, "dev:alice", conversation, "again").await;
    assert_eq!(status, 201, "{second}");
    let next = replay
        .next(Duration::from_secs(5))
        .await
        .expect("the second turn arrives on the same subscription");
    assert!(
        next.id.expect("seq") > seq,
        "event ids are monotonic per conversation"
    );
    assert_eq!(
        next.data["payload"]["turn"]["seq"], 4,
        "two turns, two seqs each"
    );
}
