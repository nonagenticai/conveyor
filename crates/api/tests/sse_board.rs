//! S05's SSE gate: a second client on `board:{id}` sees `card.moved` live and on replay,
//! and `card:{id}` is a FILTER over the board topic, not a second outbox row.
//!
//! The stream is opened with a real HTTP request to the served router and parsed frame by
//! frame, so the doorbell (`pg_notify`), the tenant-scoped replay query and the topic
//! resolution are all exercised end to end — nothing here calls the producer directly.
//!
//! Timing: the live assertion measures the interval between the mover's `200` and the frame.
//! The bound is the gate's own **200 ms** (S05 §7 property (e)): delivery is DOORBELL-driven,
//! so the frame is enqueued by `pg_notify` in the mutation's transaction and must reach the
//! subscriber almost immediately. A polling loop would miss this bound by construction.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

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
    /// Frames parsed but not yet handed out.
    ready: Vec<Frame>,
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
            ready: Vec::new(),
        }
    }

    /// The next real frame, or `None` when nothing arrived inside `patience`.
    ///
    /// Comment blocks (axum's 15 s keepalive) carry no `data:` line and are skipped: they
    /// prove the connection is alive, not that an event was delivered.
    async fn next(&mut self, patience: Duration) -> Option<Frame> {
        let deadline = Instant::now() + patience;
        loop {
            if let Some(frame) = self.ready.pop() {
                return Some(frame);
            }
            if let Some(index) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..index + 2).collect();
                if let Some(frame) = parse_block(&block) {
                    return Some(frame);
                }
                continue;
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let chunk = match tokio::time::timeout(remaining, self.response.chunk()).await {
                Ok(Ok(Some(bytes))) => bytes,
                // A closed stream or an error ends the wait exactly like a timeout.
                _ => return None,
            };
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
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

// ── fixtures ───────────────────────────────────────────────────────────────────────────

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // Bob's tenant, so a cross-tenant subscription can be proved empty.
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
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

async fn max_seq(pg: &support::Pg, tenant_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar("SELECT coalesce(max(seq), 0) FROM event WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("max seq")
}

async fn move_card(
    running: &support::Running,
    card_id: &str,
    list_id: &str,
    version: &str,
) -> Value {
    let (status, moved) = running
        .invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": list_id}),
            Some(version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved}");
    output(&moved).clone()
}

// ── the tests ──────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_client_on_the_board_sees_card_moved_live_and_on_replay() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "SSE test").await;
    let active = lanes["active"].clone();
    let waiting = lanes["waiting"].clone();
    let card_id = create_card(&running, &board_id, &active, "Mover").await;

    // The cursor a client resumes from: ""everything up to now"".
    let before = max_seq(&pg, seeded.tenant_id).await;
    // A second client, already subscribed, before the move happens.
    let mut stream = Sse::open(
        &running.base(),
        "dev:alice",
        &format!("board:{board_id}"),
        Some(before),
    )
    .await;

    let started = Instant::now();
    move_card(&running, &card_id, &waiting, "\"1\"").await;
    let frame = stream
        .next(Duration::from_secs(2))
        .await
        .expect("the mover's 200 must be followed by a frame");
    let elapsed = started.elapsed();

    assert_eq!(frame.event, "card.moved");
    assert_eq!(frame.data["type"], "card.moved");
    assert_eq!(frame.data["topic"], format!("board:{board_id}"));
    assert_eq!(frame.data["aggregate_type"], "card");
    assert_eq!(frame.data["aggregate_id"], card_id.as_str());
    assert_eq!(frame.data["payload"]["card_id"], card_id.as_str());
    assert_eq!(frame.data["payload"]["to_list_id"], waiting.as_str());
    assert_eq!(
        frame.data["payload"]["from_list_id"],
        active.as_str(),
        "the payload names the lane being left"
    );
    assert!(frame.id.unwrap_or(0) > before, "the SSE id is event.seq");
    println!("card.moved delivered in {} ms", elapsed.as_millis());
    assert!(
        elapsed < Duration::from_millis(200),
        "property (e): board:{{id}} delivery must be under 200 ms, was {elapsed:?}"
    );

    // ── replay: a cursor just below the move re-delivers it ──────────────────────────
    let mut replayed = Sse::open(
        &running.base(),
        "dev:alice",
        &format!("board:{board_id}"),
        Some(before),
    )
    .await;
    let frame = replayed
        .next(Duration::from_secs(2))
        .await
        .expect("the replay must deliver the missed event");
    assert_eq!(frame.event, "card.moved");
    assert_eq!(frame.data["aggregate_id"], card_id.as_str());

    // ── the same topic in another tenant carries nothing ─────────────────────────────
    let mut bob = Sse::open(
        &running.base(),
        "dev:bob",
        &format!("board:{board_id}"),
        None,
    )
    .await;
    let card_b = create_card(&running, &board_id, &active, "Second").await;
    // A move in dev-a that dev-b must never see.
    let moved = move_card(&running, &card_id, &active, "\"2\"").await;
    assert_eq!(moved["list_id"], active.as_str());
    assert!(
        bob.next(Duration::from_millis(1_000)).await.is_none(),
        "another tenant's subscription is empty (RLS on `event`)"
    );
    // And the replay path is scoped the same way.
    let mut bob_replay = Sse::open(
        &running.base(),
        "dev:bob",
        &format!("board:{board_id}"),
        Some(0),
    )
    .await;
    assert!(
        bob_replay.next(Duration::from_millis(500)).await.is_none(),
        "replay is tenant-scoped, not just live delivery"
    );

    // ── `card:{id}` is a filter over the board topic ─────────────────────────────────
    let mut card_b_stream = Sse::open(
        &running.base(),
        "dev:alice",
        &format!("card:{card_b}"),
        Some(max_seq(&pg, seeded.tenant_id).await),
    )
    .await;
    // A move of the OTHER card must not reach it.
    move_card(&running, &card_id, &waiting, "\"3\"").await;
    assert!(
        card_b_stream
            .next(Duration::from_millis(1_000))
            .await
            .is_none(),
        "card:{{id}} filters on aggregate_id: another card's move is invisible"
    );
    // Its own card's move must.
    move_card(&running, &card_b, &waiting, "\"1\"").await;
    let frame = card_b_stream
        .next(Duration::from_secs(2))
        .await
        .expect("the filtered subscription delivers its own card");
    assert_eq!(frame.event, "card.moved");
    assert_eq!(frame.data["aggregate_id"], card_b.as_str());

    // ── an unknown card is a 404, not a healthy empty stream ─────────────────────────
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/events?topics=card:0195f48b-d000-7000-8000-0000000000ff",
            running.base()
        ))
        .header("authorization", "Bearer dev:alice")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 404, "an unresolvable card id is refused");

    running.shutdown().await;
}
