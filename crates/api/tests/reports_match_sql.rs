//! S09 §7 (e): every number a report shows equals the SQL count on the same snapshot.
//!
//! The point is not that the action computes *something* — it is that it computes **the same thing** an operator
//! would get by querying the tables directly. A report that drifted from its source would look plausible and be
//! wrong, which is the failure mode this test exists to make impossible.
//!
//! The fixture seeds facts the action must count — two `card.moved` events into `done`, one delivered and one
//! failed delivery — and the test compares the action's document with the same counts taken on the superuser pool
//! (RLS would otherwise hide them from the comparison).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_number_equals_its_sql_count() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // Two cards reached `done`: the report counts events, because a moved card no longer records where it came
    // from (the list is the state, and it has already changed).
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO event (tenant_id, topic, type, aggregate_type, aggregate_id, payload)
             VALUES ($1, 'board:x', 'card.moved', 'card', gen_random_uuid(),
                     '{\"from_kind\": \"active\", \"to_kind\": \"done\"}'::jsonb)",
        )
        .bind(alice.tenant_id)
        .execute(&pg.super_pool)
        .await
        .expect("seed a card.moved event");
    }
    // One delivered, one failed delivery.
    for state in ["delivered", "failed"] {
        let message: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO message (tenant_id, channel_kind, body) VALUES ($1, 'web', 'x') RETURNING id",
        )
        .bind(alice.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("seed a message");
        sqlx::query(
            "INSERT INTO delivery (tenant_id, message_id, channel_kind, recipient_ref, state)
             VALUES ($1, $2, 'web', $3, $4)",
        )
        .bind(alice.tenant_id)
        .bind(message)
        .bind(format!("user-{state}"))
        .bind(state)
        .execute(&pg.super_pool)
        .await
        .expect("seed a delivery");
    }

    let (status, body) = running
        .invoke(
            "documents.report",
            "dev:alice",
            json!({"title": "Weekly", "window_days": 30}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let numbers = output(&body)["numbers"].clone();
    assert_eq!(output(&body)["document"]["kind"], "report");
    assert_eq!(
        output(&body)["document"]["object_key"],
        Value::Null,
        "a report has no bytes"
    );

    // The same eight counts, asked of the database directly.
    let (cards_opened, cards_done, wip, runs_ok, runs_failed, pending, delivered, failed): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM card WHERE tenant_id = $1 AND created_at >= now() - interval '30 days'),
           (SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'card.moved'
              AND (payload->>'to_kind') = 'done' AND created_at >= now() - interval '30 days'),
           (SELECT count(*) FROM card c JOIN list l ON l.id = c.list_id
             WHERE c.tenant_id = $1 AND l.kind = 'active' AND c.archived_at IS NULL),
           (SELECT count(*) FROM run WHERE tenant_id = $1 AND status = 'succeeded'
              AND finished_at >= now() - interval '30 days'),
           (SELECT count(*) FROM run WHERE tenant_id = $1 AND status = 'failed'
              AND finished_at >= now() - interval '30 days'),
           (SELECT count(*) FROM approval WHERE tenant_id = $1 AND status = 'pending'),
           (SELECT count(*) FROM delivery WHERE tenant_id = $1 AND state = 'delivered'
              AND updated_at >= now() - interval '30 days'),
           (SELECT count(*) FROM delivery WHERE tenant_id = $1 AND state = 'failed'
              AND updated_at >= now() - interval '30 days')",
    )
    .bind(alice.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the comparison query");

    assert_eq!(numbers["cards_opened"], cards_opened);
    assert_eq!(numbers["cards_done"], cards_done);
    assert_eq!(numbers["wip"], wip);
    assert_eq!(numbers["runs_succeeded"], runs_ok);
    assert_eq!(numbers["runs_failed"], runs_failed);
    assert_eq!(numbers["approvals_pending"], pending);
    assert_eq!(numbers["deliveries_delivered"], delivered);
    assert_eq!(numbers["deliveries_failed"], failed);
    // And the fixture's facts are visible, so the comparison is not four zeros against four zeros.
    assert_eq!(cards_done, 2, "the two seeded moves are counted");
    assert_eq!(delivered, 1);
    assert_eq!(failed, 1);

    running.shutdown().await;
}
