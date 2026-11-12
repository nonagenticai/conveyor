//! S13 — the care-bus over a real Redis (CONTRACT §7).
//!
//! The bus is a **nudge**, never the record, so what these tests prove is the transport's contract
//! rather than any run state: an envelope can be delivered twice (at-least-once), a pending entry
//! is reclaimable but only an ack clears it, the delivery ceiling really puts the original envelope
//! and its reason on `bus:dead`, and a large job document survives the trip intact.
//!
//! Each test takes its own tenant, so `stream_for` gives it a stream no sibling test touches and
//! the four may run concurrently in one binary against one Redis. `support::redis` starts that
//! Redis (or uses `TEST_REDIS_URL` / `REDIS_URL`).

mod support;

use conveyor_bus::{
    DEAD_STREAM, Envelope, GROUP_PROCESS, MAX_DELIVERIES, TOPIC_WORK_REQUESTED, stream_for,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// A `work.requested` envelope as the process runtime would publish it.
fn envelope(tenant: Uuid, job: Option<Value>) -> Envelope {
    Envelope {
        id: Uuid::now_v7(),
        topic: TOPIC_WORK_REQUESTED.to_owned(),
        from: "process".to_owned(),
        correlation_id: Some(Uuid::now_v7()),
        procedure: Some("process.card.build@3".to_owned()),
        tenant_id: tenant,
        job,
        payload: json!({"step": "read", "attempt": 1}),
    }
}

/// A tenant, its group's stream, and the group — the fixture every test starts from.
async fn fixture() -> (conveyor_bus::Bus, Uuid, String) {
    let bus = support::redis::bus().await;
    let tenant = Uuid::now_v7();
    let stream = stream_for(tenant, GROUP_PROCESS);
    bus.ensure_group(&stream, GROUP_PROCESS)
        .await
        .expect("XGROUP CREATE MKSTREAM");
    (bus, tenant, stream)
}

/// (1) A consumer group **balances** a stream, it does not de-duplicate it: a second consumer of
/// the same group receives the same entry after claiming it, and both copies carry one envelope id.
#[tokio::test]
async fn two_consumers_of_one_group_carry_the_same_envelope() {
    let (bus, tenant, stream) = fixture().await;
    let sent = envelope(tenant, None);
    let entry = bus.publish(&stream, &sent).await.expect("XADD");

    let first = bus
        .read_group(&stream, GROUP_PROCESS, "process-a", 10, None)
        .await
        .expect("XREADGROUP");
    assert_eq!(first.len(), 1, "the group's first read gets the entry");
    assert_eq!(first[0].0, entry);
    assert_eq!(first[0].1.id, sent.id);
    assert_eq!(first[0].1, sent, "the envelope round-trips field for field");

    // The group's cursor is shared: consumer `b` sees nothing *new* …
    let fresh = bus
        .read_group(&stream, GROUP_PROCESS, "process-b", 10, None)
        .await
        .expect("XREADGROUP");
    assert!(
        fresh.is_empty(),
        "a group splits work between consumers; it does not fan out"
    );

    // … but the entry is still pending, so `b` claims it: the same envelope, delivered twice.
    let claimed = bus
        .autoclaim(&stream, GROUP_PROCESS, "process-b", 0, 10)
        .await
        .expect("XAUTOCLAIM");
    assert_eq!(claimed.len(), 1, "an idle pending entry is claimable");
    assert_eq!(claimed[0].0, entry, "the stream entry id is the same one");
    assert_eq!(claimed[0].1.id, sent.id, "and so is the envelope id");
}

/// (2) An unacked entry stays pending, `autoclaim` takes it over, and only `XACK` clears it.
#[tokio::test]
async fn an_unacked_entry_is_reclaimed_and_only_an_ack_clears_it() {
    let (bus, tenant, stream) = fixture().await;
    let sent = envelope(tenant, None);
    let entry = bus.publish(&stream, &sent).await.expect("XADD");

    let read = bus
        .read_group(&stream, GROUP_PROCESS, "process-a", 10, None)
        .await
        .expect("XREADGROUP");
    assert_eq!(read.len(), 1);
    assert_eq!(
        bus.deliveries(&stream, GROUP_PROCESS, &entry)
            .await
            .expect("XPENDING"),
        1,
        "the first read is delivery one"
    );

    let claimed = bus
        .autoclaim(&stream, GROUP_PROCESS, "process-b", 0, 10)
        .await
        .expect("XAUTOCLAIM");
    assert_eq!(claimed.len(), 1, "min_idle 0 claims the pending entry");
    assert_eq!(claimed[0].1.id, sent.id);
    assert_eq!(
        bus.deliveries(&stream, GROUP_PROCESS, &entry)
            .await
            .expect("XPENDING"),
        2,
        "a claim is a delivery, which is why a reclaim loop must count them"
    );

    bus.ack(&stream, GROUP_PROCESS, &entry).await.expect("XACK");
    assert_eq!(
        bus.deliveries(&stream, GROUP_PROCESS, &entry)
            .await
            .expect("XPENDING"),
        0,
        "an acked entry leaves the pending list"
    );

    // A blocking read with nothing to deliver returns empty. This also pins BLOCK's argument
    // order: Redis parses COUNT and BLOCK before STREAMS, and a command that put BLOCK after it
    // would be a syntax error, not an empty reply.
    let idle = bus
        .read_group(&stream, GROUP_PROCESS, "process-c", 10, Some(25))
        .await
        .expect("XREADGROUP … BLOCK");
    assert!(idle.is_empty());
}

/// (3) At the delivery ceiling the envelope is dead-lettered with its reason, and the source entry
/// is acked so it is never redelivered.
#[tokio::test]
async fn five_deliveries_dead_letter_the_envelope_and_its_reason() {
    let (bus, tenant, stream) = fixture().await;
    let sent = envelope(tenant, None);
    let entry = bus.publish(&stream, &sent).await.expect("XADD");

    let first = bus
        .read_group(&stream, GROUP_PROCESS, "process-a", 10, None)
        .await
        .expect("XREADGROUP");
    assert_eq!(first.len(), 1);
    // The first delivery came from the read; each reclaim is one more.
    for _ in 1..MAX_DELIVERIES {
        let claimed = bus
            .autoclaim(&stream, GROUP_PROCESS, "process-a", 0, 10)
            .await
            .expect("XAUTOCLAIM");
        assert_eq!(claimed.len(), 1);
    }
    assert_eq!(
        bus.deliveries(&stream, GROUP_PROCESS, &entry)
            .await
            .expect("XPENDING"),
        MAX_DELIVERIES,
        "the ceiling is reached, not silently exceeded"
    );

    let reason = format!("gave up after {MAX_DELIVERIES} deliveries");
    let dead_id = bus
        .dead_letter(&sent, &reason)
        .await
        .expect("XADD bus:dead");
    bus.ack(&stream, GROUP_PROCESS, &entry)
        .await
        .expect("XACK after dead-lettering");
    assert_eq!(
        bus.deliveries(&stream, GROUP_PROCESS, &entry)
            .await
            .expect("XPENDING"),
        0,
        "the source entry is acked, so it is never redelivered"
    );

    // No group reads bus:dead, so the dead letter is inspected as raw fields — by the id the
    // dead-lettering returned, which keeps this exact even with sibling tests on the same Redis.
    let fields = bus
        .read_entry(DEAD_STREAM, &dead_id)
        .await
        .expect("XRANGE bus:dead");
    assert_eq!(
        fields.get("reason").map(String::as_str),
        Some(reason.as_str()),
        "the reason is stored beside the envelope"
    );
    let original: Envelope = serde_json::from_str(
        fields
            .get("envelope")
            .expect("bus:dead stores the original envelope"),
    )
    .expect("the original envelope parses");
    assert_eq!(
        original, sent,
        "bus:dead holds the original envelope, not a summary of it"
    );
}

/// (4) A large job document survives the trip byte for byte.
#[tokio::test]
async fn a_large_job_round_trips_intact() {
    let (bus, tenant, stream) = fixture().await;
    let rows: Vec<Value> = (0..4_096)
        .map(|index| {
            json!({
                "index": index,
                "ref": format!("artifact-{index:04}"),
                "ok": index % 3 == 0,
            })
        })
        .collect();
    let job = json!({
        "kind": "card.build",
        "instructions": "x".repeat(256 * 1024),
        "rows": rows,
    });
    let sent = envelope(tenant, Some(job));

    // Prove the instrument: a small envelope would pass the equality below just as well.
    let size = serde_json::to_vec(&sent).expect("serialise").len();
    assert!(
        size > 256 * 1024,
        "the payload is genuinely large: {size} bytes"
    );

    let entry = bus.publish(&stream, &sent).await.expect("XADD");
    let read = bus
        .read_group(&stream, GROUP_PROCESS, "process-a", 10, None)
        .await
        .expect("XREADGROUP");
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].0, entry);
    assert_eq!(read[0].1, sent, "the job document round-trips unchanged");
}

/// The spec's bus property, end to end: **a Redis loss re-delivers without a double outcome.**
///
/// Postgres is the record: the `event` row is the publish source and `run_step` is the outcome.
/// This test relays a real `run_step.requested`, consumes it, executes the step (what the worker
/// does), wipes Redis, and replays — the run state is unchanged, the step's effect happened once,
/// and the relay re-publishes nothing it already carried. A second execution of the same attempt
/// is the loser of the executor's claim, not a second effect.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flushall_replays_from_postgres_without_a_double_outcome() {
    use conveyor_store::begin_scoped;

    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    let bus = support::redis::bus().await;
    bus.flush_all().await.expect("flush the shared test Redis");

    // A project, a card, and a two-step process run: starting it writes the first
    // `run_step.requested` event and its `process.step` job.
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": format!("Bus replay {}", Uuid::now_v7().simple()),
                   "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project = created["output"]
        .get("id")
        .or(created.get("id"))
        .and_then(Value::as_str);
    let project = project.unwrap_or_else(|| panic!("no project id in {created}"));
    let (status, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board = view["output"]["boards"][0]["id"]
        .as_str()
        .or(view["boards"][0]["id"].as_str())
        .unwrap_or_else(|| panic!("no board in {view}"))
        .to_owned();
    let (status, board_view) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board_view}");
    let body = board_view.get("output").unwrap_or(&board_view);
    let lane = body["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .expect("an active lane");
    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({"board_id": board, "list_id": lane["list"]["id"],
                   "name": "Replay", "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    let card_id = card["output"]["id"]
        .as_str()
        .or(card["id"].as_str())
        .expect("a card id")
        .to_owned();

    let sop = support::publish_sop(&running, "bus-replay-sop").await;
    let (status, process) = running
        .invoke(
            "processes.create",
            "dev:alice",
            json!({"key": "bus-replay", "title": "Bus replay", "steps": [
                {"action": "cards.get", "procedure_id": sop, "input_template": {"id": "{{card.id}}"}},
                {"action": "cards.get", "procedure_id": sop, "input_template": {"id": "{{card.id}}"}}
            ]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{process}");
    let process_id = process["output"]["id"]
        .as_str()
        .or(process["id"].as_str())
        .expect("a process id");
    let (status, published) = running
        .invoke(
            "processes.publish",
            "dev:alice",
            json!({"process_id": process_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    let (status, started) = running
        .invoke(
            "process_runs.start",
            "dev:alice",
            json!({"card_id": card_id, "process_key": "bus-replay"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{started}");
    let run = started["output"]["id"]
        .as_str()
        .or(started["id"].as_str())
        .expect("a run id");
    let run_id = Uuid::parse_str(run).expect("run id");

    // ── relay one page: publish the unpublished events, record the cursor ─────────────────
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    // Only the run-step nudge is relayed here. The setup above also wrote project/board/card/
    // process events that no relay has carried yet (this harness starts the app, not the
    // worker), so the unfiltered page is not deterministic: the property under test is about
    // the step's nudge, and it must be the one this page carries.
    let events: Vec<_> = conveyor_store::bus::unpublished_tx(&mut tx, seeded.tenant, 100)
        .await
        .expect("the outbox page")
        .into_iter()
        .filter(|event| event.event_type == conveyor_store::process::RUN_STEP_REQUESTED)
        .collect();
    assert_eq!(
        events.len(),
        1,
        "the start wrote exactly one run_step.requested"
    );
    let stream = stream_for(seeded.tenant_id, GROUP_PROCESS);
    for event in &events {
        let envelope = Envelope {
            id: event.id,
            topic: event.event_type.clone(),
            from: "process".to_owned(),
            correlation_id: None,
            procedure: None,
            tenant_id: seeded.tenant_id,
            job: Some(event.payload.clone()),
            payload: event.payload.clone(),
        };
        let entry = bus.publish(&stream, &envelope).await.expect("XADD");
        conveyor_store::bus::mark_published_tx(&mut tx, seeded.tenant, event.id, &entry)
            .await
            .expect("mark published");
    }
    tx.commit().await.expect("commit the relay page");
    bus.ensure_group(&stream, GROUP_PROCESS)
        .await
        .expect("XGROUP CREATE");

    // ── consume and execute, as the worker does ─────────────────────────────────────────
    let delivered = bus
        .read_group(&stream, GROUP_PROCESS, "replay-a", 10, None)
        .await
        .expect("XREADGROUP");
    assert_eq!(delivered.len(), 1, "one nudge for the first step");
    assert_eq!(
        delivered[0]
            .1
            .job
            .as_ref()
            .and_then(|job| job["position"].as_i64()),
        Some(1)
    );
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        run_id,
        1,
        1,
    )
    .await
    .expect("the consumer executes the step");
    bus.ack(&stream, GROUP_PROCESS, &delivered[0].0)
        .await
        .expect("XACK");

    // ── the Redis loss ───────────────────────────────────────────────────────────────────
    bus.flush_all().await.expect("FLUSHALL");
    let (status, _) = running
        .invoke(
            "runs.get",
            "dev:alice",
            json!({"id": run}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "the run is still readable with Redis gone");

    // ── replay: nothing is owed, and a re-execution is a replay, never a second effect ───
    let relayed: Vec<Uuid> = events.iter().map(|event| event.id).collect();
    let still_owed: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND id = ANY($2) AND published_at IS NULL",
    )
    .bind(seeded.tenant_id)
    .bind(&relayed)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the relayed rows' cursor");
    assert_eq!(
        still_owed.0, 0,
        "every carried event keeps its cursor; the relay re-sends nothing it already published"
    );
    let (successes,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND status = 'succeeded'",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the step trace");
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        run_id,
        1,
        1,
    )
    .await
    .expect("the replay is a no-op");
    let (successes_after,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND status = 'succeeded'",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the step trace after the replay");
    assert_eq!(
        successes_after, successes,
        "a re-delivered step is the executor's replay, not a second effect"
    );
}
