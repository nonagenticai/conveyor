//! S06 §5.6's SSE proof for the control plane: `run:{id}`, `approval:{id}` and `tenant`.
//!
//! One real HTTP subscription to `GET /api/events`, parsed frame by frame. Everything below
//! is driven through the served router (claims/runs/gate decisions/andon) or through the S11
//! store seam (`store::approval::open_gate_tx`) — nothing calls the producer directly, so the
//! outbox row, the `pg_notify` doorbell, the tenant-scoped replay and the topic resolution
//! are all exercised end to end.
//!
//! Properties:
//! * every delivered frame carries the next matching `event.seq` — strictly increasing, no
//!   gap and no duplicate within the subscription, exactly the set the tenant's outbox holds
//!   for these three topics;
//! * a second `run:{id}` (created and started while the subscription is live) never appears
//!   on this stream;
//! * reconnecting with `Last-Event-ID` set before the first event replays the same events,
//!   once each, starting after the cursor.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

// ── the SSE client ─────────────────────────────────────────────────────────────────────

/// One parsed frame: `id` (the `event.seq`), `event` (the type) and the JSON `data`.
#[derive(Debug)]
struct Frame {
    id: Option<i64>,
    event: String,
    data: Value,
}

struct Sse {
    buffer: String,
    response: reqwest::Response,
}

impl Sse {
    async fn open(base: &str, token: &str, topics: &str, last_event_id: Option<i64>) -> Sse {
        let mut request = reqwest::Client::new()
            .get(format!("{base}/api/events?topics={topics}"))
            .header("authorization", format!("Bearer {token}"))
            .header("accept", "text/event-stream");
        if let Some(id) = last_event_id {
            request = request.header("last-event-id", id.to_string());
        }
        let response = request.send().await.expect("open the SSE stream");
        assert_eq!(response.status(), 200, "the stream must open");
        Sse {
            buffer: String::new(),
            response,
        }
    }

    /// The next real frame, or `None` when nothing arrived inside `patience`.
    ///
    /// Comment blocks (axum's 15 s keepalive) carry no `event:`/`data:` pair and are
    /// skipped: they prove the connection is alive, not that an event was delivered.
    async fn next(&mut self, patience: Duration) -> Option<Frame> {
        let deadline = Instant::now() + patience;
        loop {
            if let Some(index) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..index + 2).collect();
                if let Some(frame) = parse_block(&block) {
                    return Some(frame);
                }
                continue;
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            match tokio::time::timeout(remaining, self.response.chunk()).await {
                Ok(Ok(Some(bytes))) => self.buffer.push_str(&String::from_utf8_lossy(&bytes)),
                // A closed stream or an error ends the wait exactly like a timeout.
                _ => return None,
            }
        }
    }
}

fn parse_block(block: &str) -> Option<Frame> {
    let mut id = None;
    let mut event = None;
    let mut data = String::new();
    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("id:") {
            id = rest.trim().parse::<i64>().ok();
        } else if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim());
        }
    }
    event.map(|event| Frame {
        id,
        event,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
    })
}

// ── fixtures and drivers ───────────────────────────────────────────────────────────────

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn make_board(running: &support::Running, title: &str) -> (String, HashMap<String, String>) {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = id_of(output(&created), "id");
    let (_, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (_, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    let mut lanes = HashMap::new();
    for row in output(&board)["lists"].as_array().expect("lists") {
        lanes.insert(
            row["list"]["kind"].as_str().expect("kind").to_owned(),
            id_of(&row["list"], "id"),
        );
    }
    (board_id, lanes)
}

async fn create_card(
    running: &support::Running,
    board_id: &str,
    list_id: &str,
    name: &str,
) -> String {
    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": name,
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    id_of(output(&card), "id")
}

async fn create_run(running: &support::Running, card_id: &str) -> String {
    let (status, created) = running
        .invoke(
            "runs.create",
            "dev:alice",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "runs.create: {created}");
    id_of(output(&created), "id")
}

async fn start_run(running: &support::Running, run_id: &str) {
    let (status, started) = running
        .invoke(
            "runs.start",
            "dev:alice",
            json!({"id": run_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "runs.start: {started}");
    assert_eq!(output(&started)["status"], "running");
}

async fn open_gate(pg: &support::Pg, seeded: &support::Seeded, run_id: &str) -> Uuid {
    let run_uuid: Uuid = run_id.parse().expect("a run uuid");
    let mut tx = store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("the S11 seam opens a scoped transaction");
    let approval = store::approval::open_gate_tx(
        &mut tx,
        seeded.tenant,
        run_uuid,
        "review",
        "gate",
        1,
        None,
        Some(seeded.user_id),
    )
    .await
    .expect("open the gate on the running run");
    tx.commit().await.expect("commit the gate");
    approval.id
}

async fn decide_approve(running: &support::Running, approval_id: Uuid) {
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            "dev:alice",
            json!({"approval_id": approval_id.to_string(), "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "approvals.decide: {decided}");
    assert_eq!(output(&decided)["status"], "approved");
}

async fn finish_run(running: &support::Running, run_id: &str) {
    let (status, finished) = running
        .invoke(
            "runs.finish",
            "dev:alice",
            json!({"id": run_id, "outcome": "succeeded"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "runs.finish: {finished}");
    assert_eq!(output(&finished)["status"], "succeeded");
}

async fn engage_andon(running: &support::Running) {
    let (status, body) = running
        .invoke(
            "andon.engage",
            "dev:alice",
            json!({"reason": "sse control test"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "andon.engage: {body}");
    assert_eq!(output(&body)["engaged"], true);
}

async fn release_andon(running: &support::Running) {
    let (status, body) = running
        .invoke("andon.release", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "andon.release: {body}");
    assert_eq!(output(&body)["engaged"], false);
}

/// The tenant-scoped read as the runtime role: the same RLS scope every request runs under.
///
/// `(seq, topic, type)` per matching outbox row, in outbox order — the authority the stream
/// is compared against, and the diagnostics a mismatch prints.
async fn matching_events(
    pg: &support::Pg,
    tenant: TenantId,
    topics: &[String],
) -> Vec<(i64, String, String)> {
    let mut tx = store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the outbox read");
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT seq, topic, type FROM event
          WHERE tenant_id = $1 AND topic = ANY($2) ORDER BY seq",
    )
    .bind(tenant.as_uuid())
    .bind(topics.to_vec())
    .fetch_all(&mut *tx)
    .await
    .expect("read the events this subscription selects");
    tx.commit().await.expect("commit the read");
    rows
}

fn seqs_of(rows: &[(i64, String, String)]) -> Vec<i64> {
    rows.iter().map(|row| row.0).collect()
}

async fn topic_event_count(pg: &support::Pg, tenant: TenantId, topic: &str) -> i64 {
    let mut tx = store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the outbox read");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1 AND topic = $2")
            .bind(tenant.as_uuid())
            .bind(topic)
            .fetch_one(&mut *tx)
            .await
            .expect("count the topic's events");
    tx.commit().await.expect("commit the read");
    count
}

/// Collect frames until `want` have arrived, waiting on the stream rather than sleeping.
async fn collect_at_least(stream: &mut Sse, want: usize) -> Vec<Frame> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frames = Vec::new();
    while frames.len() < want {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        match stream.next(remaining).await {
            Some(frame) => frames.push(frame),
            None => break,
        }
    }
    frames
}

fn seq_of(frame: &Frame) -> i64 {
    frame
        .id
        .unwrap_or_else(|| panic!("every delivered frame carries event.seq as its id: {frame:?}"))
}

fn assert_exactly_once_in_seq_order(delivered: &[i64], expected: &[i64], what: &str) {
    // No duplicate: one `seq` per frame.
    let unique: BTreeSet<i64> = delivered.iter().copied().collect();
    assert_eq!(
        unique.len(),
        delivered.len(),
        "{what}: a seq was delivered twice: {delivered:?}"
    );
    // Strictly increasing, so the stream cannot reorder what the outbox ordered.
    assert!(
        delivered.windows(2).all(|pair| pair[0] < pair[1]),
        "{what}: seqs are not strictly increasing: {delivered:?}"
    );
    // No gap: exactly the set the tenant's outbox holds for these topics.
    assert_eq!(
        delivered, expected,
        "{what}: delivered seqs differ from the tenant's matching outbox rows"
    );
}

fn assert_no_foreign_run(frames: &[Frame], run_b: &str) {
    let run_b_topic = format!("run:{run_b}");
    for frame in frames {
        assert_ne!(
            frame.data["topic"], run_b_topic,
            "a run:{run_b} event reached a subscription that never named it: {frame:?}"
        );
        assert_ne!(
            frame.data["aggregate_id"], run_b,
            "an aggregate of another run reached this subscription: {frame:?}"
        );
    }
}

// ── the tests ──────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn control_topics_deliver_each_event_once_in_seq_order_and_never_another_run() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "SSE control").await;
    let card_id = create_card(&running, &board_id, &lanes["active"], "Control card").await;

    // The run's own start history, then the gate that parks it.
    let run_a = create_run(&running, &card_id).await;
    start_run(&running, &run_a).await;
    let approval = open_gate(&pg, &seeded, &run_a).await;
    let topics = vec![
        format!("run:{run_a}"),
        format!("approval:{approval}"),
        "tenant".to_owned(),
    ];

    // Subscribe BEFORE the decision. Cursor 0 makes the producer replay the run's already
    // written events, then follow the doorbell for everything driven live below.
    let mut stream = Sse::open(&running.base(), "dev:alice", &topics.join(","), Some(0)).await;

    decide_approve(&running, approval).await;
    finish_run(&running, &run_a).await;
    engage_andon(&running).await;
    release_andon(&running).await;

    // A second run on the same card, created and started while this subscription is LIVE.
    // Its `run.created`/`run.started` are real outbox rows; they must never be delivered here.
    let run_b = create_run(&running, &card_id).await;
    start_run(&running, &run_b).await;

    let expected_rows = matching_events(&pg, seeded.tenant, &topics).await;
    let expected = seqs_of(&expected_rows);
    assert_eq!(
        expected.len(),
        7,
        "the register for this drive is run.created, run.started, approval.requested, \
         approval.approved, run.succeeded, andon.engaged, andon.released; outbox: {expected_rows:?}"
    );
    assert_eq!(
        topic_event_count(&pg, seeded.tenant, &format!("run:{run_b}")).await,
        2,
        "positive control: the second run really did emit run.created + run.started"
    );

    let frames = collect_at_least(&mut stream, expected.len()).await;
    let delivered: Vec<i64> = frames.iter().map(seq_of).collect();
    assert_exactly_once_in_seq_order(&delivered, &expected, "live subscription");
    assert_no_foreign_run(&frames, &run_b);

    // Every frame's `data` repeats the envelope the client routes on.
    for frame in &frames {
        assert_eq!(frame.data["seq"], seq_of(frame), "{frame:?}");
        assert_eq!(frame.data["type"], frame.event.as_str(), "{frame:?}");
    }

    // Nothing else is pending for this subscription: a duplicate or a foreign event would
    // have to arrive inside this window (delivery is doorbell-driven, not polled).
    assert!(
        stream.next(Duration::from_millis(1_000)).await.is_none(),
        "the subscription delivered more frames than the tenant's matching outbox rows"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnecting_with_last_event_id_replays_the_same_events_once() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "SSE replay").await;
    let card_id = create_card(&running, &board_id, &lanes["active"], "Replay card").await;

    let run_a = create_run(&running, &card_id).await;
    start_run(&running, &run_a).await;
    let approval = open_gate(&pg, &seeded, &run_a).await;
    let topics = vec![
        format!("run:{run_a}"),
        format!("approval:{approval}"),
        "tenant".to_owned(),
    ];

    decide_approve(&running, approval).await;
    finish_run(&running, &run_a).await;
    engage_andon(&running).await;
    release_andon(&running).await;

    // A different run's events exist in the same tenant and must not appear on this topic
    // set's replay either.
    let run_b = create_run(&running, &card_id).await;
    start_run(&running, &run_b).await;
    assert_eq!(
        topic_event_count(&pg, seeded.tenant, &format!("run:{run_b}")).await,
        2,
        "positive control: the other run's events are in the outbox"
    );

    let expected_rows = matching_events(&pg, seeded.tenant, &topics).await;
    let expected = seqs_of(&expected_rows);
    assert_eq!(
        expected.len(),
        7,
        "the same seven-event drive as the live test; outbox: {expected_rows:?}"
    );
    let cursor = expected.first().copied().expect("the drive emitted events") - 1;

    let mut replayed = Sse::open(
        &running.base(),
        "dev:alice",
        &topics.join(","),
        Some(cursor),
    )
    .await;
    let frames = collect_at_least(&mut replayed, expected.len()).await;
    let delivered: Vec<i64> = frames.iter().map(seq_of).collect();

    assert!(
        delivered.first().is_some_and(|seq| *seq > cursor),
        "replay starts AFTER the cursor, never at it: cursor {cursor}, delivered {delivered:?}"
    );
    assert_exactly_once_in_seq_order(&delivered, &expected, "replay");
    assert_no_foreign_run(&frames, &run_b);
    assert!(
        replayed.next(Duration::from_millis(1_000)).await.is_none(),
        "replay delivered more than the matching outbox rows"
    );

    running.shutdown().await;
}
