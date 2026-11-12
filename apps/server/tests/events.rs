//! SSE: replay ordering, tenant filtering, the live doorbell, `resync`, and the licence gate.
//!
//! `cargo test -p conveyor-server --test events -- --nocapture`. A real axum server on an
//! ephemeral port, a real HTTP client reading the stream chunk by chunk, and a real
//! `LISTEN conveyor_event` connection: the doorbell is proven by inserting an event after
//! the stream is open, not by calling the handler twice.
//!
//! Both tenants publish to the SAME topic string on purpose: the tenant isolation being
//! proven is RLS, not the topic filter.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use conveyor_api::{AppState, Authenticator, Config, Metrics, app};
use conveyor_auth::AuthConfig;
use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::outbox::{self, NewEvent};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower::ServiceExt;

const TOPIC: &str = "board:0195f48b-d000-7000-8000-000000000001";

async fn dev_state(db: &common::TestDb) -> AppState {
    let runtime_url = db.runtime_url.clone();
    let config = Config::from_lookup(|key| match key {
        "CONVEYOR_DEV_AUTH" => Some("1".to_owned()),
        "DATABASE_URL" => Some(runtime_url.clone()),
        _ => None,
    })
    .expect("dev configuration");
    // The Platform/billing clients: unconfigured in tests, which is the `None` per service
    // the state already models — the same shape `AppState::connect` builds.
    let (services, _warnings) = conveyor_api::services::Services::from_config(&config);
    AppState {
        pool: db.runtime.clone(),
        listen: conveyor_store::EventListen::connect(&db.runtime_url)
            .await
            .expect("LISTEN connection"),
        config: Arc::new(config),
        auth: Arc::new(Authenticator::new(AuthConfig::development()).expect("dev authenticator")),
        services,
        metrics: Metrics::install(),
    }
}

async fn tenant_id(db: &common::TestDb, slug: &str) -> TenantId {
    conveyor_store::tenants::id_by_slug(&db.super_pool, slug)
        .await
        .expect("tenant lookup")
        .unwrap_or_else(|| panic!("tenant {slug} is seeded"))
}

async fn spawn_http(state: AppState) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app(state)).await.expect("serve");
    });
    format!("http://{addr}")
}

async fn grant_licence(db: &common::TestDb, tenant: TenantId, status: &str) {
    sqlx::query(
        "INSERT INTO app_licence (tenant_id, app_key, status, checked_at)
         VALUES ($1, 'conveyor', $2, now())
         ON CONFLICT (tenant_id, app_key) DO UPDATE
             SET status = excluded.status, checked_at = now(), updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(status)
    .execute(&db.super_pool)
    .await
    .expect("grant a licence row");
}

async fn seed_event(db: &common::TestDb, tenant: TenantId, topic: &str, n: i64) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO event (tenant_id, topic, type, aggregate_type, aggregate_id, payload)
         VALUES ($1, $2, 'card.moved', 'card', $3, $4)
         RETURNING seq",
    )
    .bind(tenant.as_uuid())
    .bind(topic)
    .bind(uuid::Uuid::now_v7())
    .bind(json!({"n": n}))
    .fetch_one(&db.super_pool)
    .await
    .expect("seed an event")
}

async fn append_live(db: &common::TestDb, tenant: TenantId, topic: &str, n: i64) -> i64 {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let row = outbox::append(
        &mut tx,
        tenant,
        &NewEvent {
            topic: topic.to_owned(),
            event_type: "card.moved".to_owned(),
            aggregate_type: "card".to_owned(),
            aggregate_id: uuid::Uuid::now_v7(),
            payload: json!({"n": n}),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await
    .expect("append through the outbox");
    tx.commit().await.expect("commit");
    row.seq
}

#[derive(Debug, Clone, PartialEq)]
struct Frame {
    id: Option<String>,
    event: Option<String>,
    data: Option<String>,
}

fn parse_frame(raw: &str) -> Option<Frame> {
    let mut frame = Frame {
        id: None,
        event: None,
        data: None,
    };
    let mut any = false;
    for line in raw.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.strip_prefix(' ').unwrap_or(value).to_owned();
        match field {
            "id" => {
                frame.id = Some(value);
                any = true;
            }
            "event" => {
                frame.event = Some(value);
                any = true;
            }
            "data" => {
                frame.data = Some(value);
                any = true;
            }
            // `retry:` and unknown fields are ignored by this reader.
            _ => {}
        }
    }
    any.then_some(frame)
}

async fn read_frames(
    response: &mut reqwest::Response,
    want: usize,
    within: Duration,
) -> Vec<Frame> {
    tokio::time::timeout(within, async {
        let mut buffer = String::new();
        let mut frames = Vec::new();
        while frames.len() < want {
            let Some(chunk) = response.chunk().await.expect("read the SSE body") else {
                break;
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(index) = buffer.find("\n\n") {
                let raw: String = buffer.drain(..index + 2).collect();
                if let Some(frame) = parse_frame(&raw) {
                    frames.push(frame);
                }
                if frames.len() >= want {
                    break;
                }
            }
        }
        frames
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {want} SSE frame(s)"))
}

fn data_of(frame: &Frame) -> Value {
    serde_json::from_str(frame.data.as_deref().expect("a data member")).expect("JSON data")
}

async fn open(
    client: &reqwest::Client,
    base: &str,
    login: &str,
    topics: &str,
    last: Option<i64>,
) -> reqwest::Response {
    let mut request = client
        .get(format!("{base}/api/events?topics={topics}"))
        .header("authorization", format!("Bearer dev:{login}"));
    if let Some(last) = last {
        request = request.header("last-event-id", last.to_string());
    }
    let response = request.send().await.expect("open the SSE stream");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the subscription was refused"
    );
    assert!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream")),
        "content-type: {:?}",
        response.headers().get("content-type")
    );
    response
}

#[tokio::test]
async fn replay_is_ordered_filters_by_tenant_and_resumes_after_last_event_id() {
    let db = common::TestDb::fresh().await;
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed dev");
    let a = tenant_id(&db, "dev-a").await;
    let b = tenant_id(&db, "dev-b").await;
    grant_licence(&db, a, "active").await;
    grant_licence(&db, b, "active").await;

    let a_seqs = [
        seed_event(&db, a, TOPIC, 1).await,
        seed_event(&db, a, TOPIC, 2).await,
        seed_event(&db, a, TOPIC, 3).await,
    ];
    let b_seqs = [
        seed_event(&db, b, TOPIC, 10).await,
        seed_event(&db, b, TOPIC, 11).await,
    ];
    println!("A seqs={a_seqs:?} B seqs={b_seqs:?} (same topic {TOPIC})");

    let state = dev_state(&db).await;
    let base = spawn_http(state).await;
    let client = reqwest::Client::new();

    // ── no cursor: A's three events, in seq order, none of B's ─────────────────────────
    let mut response = open(&client, &base, "alice", TOPIC, None).await;
    let frames = read_frames(&mut response, 3, Duration::from_secs(10)).await;
    let ids: Vec<String> = frames
        .iter()
        .map(|frame| frame.id.clone().expect("every event frame carries an id"))
        .collect();
    println!("alice (no cursor) -> {ids:?}");
    assert_eq!(
        ids,
        a_seqs.iter().map(i64::to_string).collect::<Vec<_>>(),
        "A's events, in seq order"
    );
    for frame in &frames {
        assert_eq!(frame.event.as_deref(), Some("card.moved"));
        let data = data_of(frame);
        assert_eq!(data["topic"], TOPIC);
        assert_eq!(data["aggregate_type"], "card");
        assert!(
            !b_seqs.contains(&data["seq"].as_i64().expect("seq")),
            "tenant B's event must never appear on A's stream"
        );
    }
    drop(response);

    // ── Last-Event-ID: only what came after e1 ─────────────────────────────────────────
    let mut resumed = open(&client, &base, "alice", TOPIC, Some(a_seqs[0])).await;
    let resumed_frames = read_frames(&mut resumed, 2, Duration::from_secs(10)).await;
    let resumed_ids: Vec<String> = resumed_frames
        .iter()
        .map(|frame| frame.id.clone().expect("id"))
        .collect();
    println!("alice (Last-Event-ID {} ) -> {resumed_ids:?}", a_seqs[0]);
    assert_eq!(
        resumed_ids,
        vec![a_seqs[1].to_string(), a_seqs[2].to_string()],
        "e2 and e3, once each, in order"
    );
    drop(resumed);

    // ── tenant B sees only its own rows, on the same topic string ─────────────────────
    let mut bob = open(&client, &base, "bob", TOPIC, None).await;
    let bob_frames = read_frames(&mut bob, 2, Duration::from_secs(10)).await;
    let bob_ids: Vec<String> = bob_frames
        .iter()
        .map(|frame| frame.id.clone().expect("id"))
        .collect();
    println!("bob -> {bob_ids:?}");
    assert_eq!(
        bob_ids,
        b_seqs.iter().map(i64::to_string).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_live_event_reaches_an_open_stream_and_another_tenants_does_not() {
    let db = common::TestDb::fresh().await;
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed dev");
    let a = tenant_id(&db, "dev-a").await;
    let b = tenant_id(&db, "dev-b").await;
    grant_licence(&db, a, "active").await;
    grant_licence(&db, b, "active").await;

    let state = dev_state(&db).await;
    let base = spawn_http(state).await;
    let client = reqwest::Client::new();
    let topic = "run:0195f48b-d000-7000-8000-000000000002";

    // Open first: the listener is live and the replay is empty.
    let mut response = open(&client, &base, "alice", topic, None).await;

    let first = append_live(&db, a, topic, 1).await;
    let frames = read_frames(&mut response, 1, Duration::from_secs(10)).await;
    println!("live doorbell delivered seq {first}");
    assert_eq!(frames[0].id.as_deref(), Some(first.to_string().as_str()));
    assert_eq!(data_of(&frames[0])["payload"]["n"], 1_i64);

    // B's event rings the same doorbell; A's stream must not emit it. The next frame the
    // client sees is A's own later event, which proves B's was filtered rather than queued.
    let b_seq = append_live(&db, b, topic, 99).await;
    let second = append_live(&db, a, topic, 2).await;
    let frames = read_frames(&mut response, 1, Duration::from_secs(10)).await;
    let id = frames[0].id.as_deref().expect("id");
    println!("after B seq {b_seq} and A seq {second}: next frame id={id}");
    assert_eq!(id, second.to_string());
    assert_ne!(id, b_seq.to_string());
}

#[tokio::test]
async fn a_pruned_history_or_a_cursor_ahead_is_told_to_resync() {
    let db = common::TestDb::fresh().await;
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed dev");
    let a = tenant_id(&db, "dev-a").await;
    let b = tenant_id(&db, "dev-b").await;
    grant_licence(&db, a, "active").await;

    let first = seed_event(&db, a, TOPIC, 1).await;
    let _b_gap = seed_event(&db, b, TOPIC, 50).await; // makes A's min > first + 1
    let second = seed_event(&db, a, TOPIC, 2).await;
    let third = seed_event(&db, a, TOPIC, 3).await;
    // A prune (or a retention job) removed A's first event.
    sqlx::query("DELETE FROM event WHERE seq = $1")
        .bind(first)
        .execute(&db.super_pool)
        .await
        .expect("prune the first event");
    println!("pruned A's seq {first}; retained {second}, {third}");

    let state = dev_state(&db).await;
    let base = spawn_http(state).await;
    let client = reqwest::Client::new();

    let mut response = open(&client, &base, "alice", TOPIC, Some(first)).await;
    let frames = read_frames(&mut response, 3, Duration::from_secs(10)).await;
    println!("resync frames: {frames:?}");
    assert_eq!(frames[0].event.as_deref(), Some("resync"));
    let resync = data_of(&frames[0]);
    assert_eq!(resync["reason"], "history_pruned");
    assert_eq!(resync["last_event_id"], first);
    assert_eq!(frames[1].id.as_deref(), Some(second.to_string().as_str()));
    assert_eq!(frames[2].id.as_deref(), Some(third.to_string().as_str()));
    drop(response);

    // A cursor from another database (ahead of this one) is also a resync, not a hang.
    let mut ahead = open(&client, &base, "alice", TOPIC, Some(9_000_000)).await;
    let frames = read_frames(&mut ahead, 1, Duration::from_secs(10)).await;
    assert_eq!(frames[0].event.as_deref(), Some("resync"));
    assert_eq!(data_of(&frames[0])["reason"], "cursor_ahead");
}

#[tokio::test]
async fn the_licence_gate_fails_closed_and_topics_are_validated() {
    let db = common::TestDb::fresh().await;
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed dev");
    // S04 (decision 9) made `migrate --dev-seed` grant an ACTIVE licence, because the
    // action invocation route is licence-gated. This test is about the gate failing CLOSED,
    // so it starts from the no-licence state the seed used to leave.
    sqlx::query("DELETE FROM app_licence")
        .execute(&db.super_pool)
        .await
        .expect("clear the seeded licence");
    let a = tenant_id(&db, "dev-a").await;
    let state = dev_state(&db).await;
    let base = spawn_http(state).await;
    let client = reqwest::Client::new();

    // No `app_licence` row: 403 problem+json.
    let response = client
        .get(format!("{base}/api/events?topics={TOPIC}"))
        .header("authorization", "Bearer dev:alice")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let body: Value = response.json().await.expect("problem body");
    println!("no licence -> 403 {body}");
    assert_eq!(body["type"], "licence_required");
    assert_eq!(body["status"], 403);

    // An unknown status is refused too.
    grant_licence(&db, a, "unknown").await;
    let response = client
        .get(format!("{base}/api/events?topics={TOPIC}"))
        .header("authorization", "Bearer dev:alice")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Active: the stream opens.
    grant_licence(&db, a, "active").await;
    let response = open(&client, &base, "alice", TOPIC, None).await;
    drop(response);

    // A malformed topic is 400, and it is checked after the credential but before the
    // stream: a bad request never opens a connection.
    let response = client
        .get(format!("{base}/api/events?topics=Board:x"))
        .header("authorization", "Bearer dev:alice")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.expect("problem body");
    assert_eq!(body["errors"][0]["field"], "topics");

    // No credential: 401 before the licence is consulted.
    let response = client
        .get(format!("{base}/api/events?topics={TOPIC}"))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_health_and_metrics_routes_are_not_licence_gated() {
    let db = common::TestDb::fresh().await;
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed dev");
    let state = dev_state(&db).await;
    let app = app(state);

    for path in ["/health", "/health/live", "/health/ready", "/metrics"] {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .expect("request"),
            )
            .await
            .expect("one request");
        assert_eq!(response.status(), StatusCode::OK, "{path}");
    }
}
