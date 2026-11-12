//! S06 §7 property (e) and §5.6: a failed send exhausts its budget on ONE row, the backoff
//! grows, `web` is delivered, and a fan-out writes one delivery and one deduped job per
//! recipient.
//!
//! A real `conveyor worker` child process drains the `delivery.send` jobs (the placement
//! rule: a test that must drive a job handler lives where `CARGO_BIN_EXE_conveyor` exists).
//! `telegram` is a real transport (D2): the retry proof points it at a stub Bot API that
//! answers 500, so every attempt fails retryably. A kind with no registered transport at all
//! (`carrier-pigeon` — `delivery.channel_kind` has no CHECK) is the no-transport case, and
//! the budget still turns it terminal. `web` is the delivered one.
//!
//! The test advances the clock by moving the same job row's `run_after` to `now()` after it
//! has OBSERVED the scheduled backoff — it never shortens the assertion, it only skips the
//! wall-clock wait. A real 10 s + 20 s + 40 s + 80 s sleep would be the whole test's budget.

mod common;

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::json;
use uuid::Uuid;

/// The tenant KEK the test process and the child worker share (S06d's connector secrets are
/// sealed under it). Set before the first `sealer()` call in this binary.
const TEST_KEK: &str = "Y29udmV5b3ItdGVzdC1rZWstMzJieXRlcy1sb25nISE=";

fn set_kek() {
    // SAFETY: set before the first `sealer()` call in this binary; the keystore caches it once.
    unsafe {
        std::env::set_var("CONVEYOR_SECRET_KEK", TEST_KEK);
    }
}

/// A child worker that is killed on drop, so a failed assertion cannot leave a poller running
/// against the test database.
struct WorkerChild {
    child: Child,
}

impl WorkerChild {
    fn spawn(db: &common::TestDb) -> Self {
        Self::spawn_with(db, &[])
    }

    /// Spawn for a connector test. The Bot API base is per binding config (S06d), not an
    /// environment variable; the worker only needs the tenant KEK to open the binding secret.
    fn spawn_telegram(db: &common::TestDb) -> Self {
        Self::spawn_with(db, &[])
    }

    fn spawn_with(db: &common::TestDb, extra: &[(&str, &str)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_conveyor"));
        command
            .arg("worker")
            .env("APP_ENV", "test")
            .env("DATABASE_URL", &db.runtime_url)
            .env("DATABASE_LISTEN_URL", &db.runtime_url)
            .env("DATABASE_MIGRATE_URL", &db.runtime_url)
            .env("APP_KEY", "conveyor")
            .env("JOB_LEASE_SECS", "2")
            .env("RUST_LOG", "info")
            .env("CONVEYOR_SECRET_KEK", TEST_KEK)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (name, value) in extra {
            command.env(name, value);
        }
        let child = command.spawn().expect("spawn the conveyor worker binary");
        Self { child }
    }
}

/// A stub Bot API that answers every request `500`, so the telegram transport takes the
/// retryable-failure path instead of a permanent configuration refusal.
async fn telegram_failure_stub() -> String {
    use axum::http::StatusCode;

    let app = axum::Router::new().fallback(|| async { StatusCode::INTERNAL_SERVER_ERROR });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the telegram stub");
    let addr = listener.local_addr().expect("the stub's address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Debug)]
struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    alice: Uuid,
    bob: Uuid,
}

/// Seed the minimum a send needs: a tenant and two members. Fixtures are raw SQL as the
/// SUPERUSER; every behaviour under test runs as the runtime role in a tenant-scoped
/// transaction.
async fn seed(db: &common::TestDb) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('retry', 'Retry') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let mut users = Vec::new();
    for (subject, name) in [("dev:alice", "Alice"), ("dev:bob", "Bob")] {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO app_user (oidc_subject, display_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(subject)
        .bind(name)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed app_user");
        sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
            .bind(tenant_id)
            .bind(id)
            .execute(&db.super_pool)
            .await
            .expect("seed membership");
        users.push(id);
    }
    Fixture {
        tenant: TenantId::from_uuid(tenant_id).expect("non-nil tenant"),
        tenant_id,
        alice: users[0],
        bob: users[1],
    }
}

async fn scoped(
    db: &common::TestDb,
    tenant: TenantId,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope a transaction")
}

/// One message and one delivery on `channel_kind`, with its `delivery.send` job — the shape
/// `messages.send` commits.
async fn seed_delivery(db: &common::TestDb, fixture: &Fixture, channel_kind: &str) -> (Uuid, Uuid) {
    let mut tx = scoped(db, fixture.tenant).await;
    let message = store::delivery::insert_message_tx(
        &mut tx,
        fixture.tenant,
        &store::delivery::NewMessage::new(channel_kind, "hello"),
    )
    .await
    .expect("insert the message");
    let inserted = store::delivery::insert_delivery_tx(
        &mut tx,
        fixture.tenant,
        &store::delivery::NewDelivery::new(message.id, channel_kind, &fixture.alice.to_string()),
    )
    .await
    .expect("insert the delivery");
    store::delivery::enqueue_delivery_send_tx(&mut tx, fixture.tenant, inserted.delivery.id, None)
        .await
        .expect("enqueue the send job");
    tx.commit().await.expect("commit the fixture");
    (message.id, inserted.delivery.id)
}

/// The tenant's active telegram route. The Bot API is addressed through the binding's
/// `chat_id`; without one the handler refuses permanently, which would prove nothing about
/// the retry path.
async fn seed_telegram_binding(db: &common::TestDb, fixture: &Fixture, api_base: &str) {
    set_kek();
    let mut tx = scoped(db, fixture.tenant).await;
    store::secret::create_secret_tx(
        &mut tx,
        fixture.tenant,
        "telegram-token",
        b"123:test-token",
        "tenant",
    )
    .await
    .expect("seed the bot token secret");
    store::secret::create_secret_tx(
        &mut tx,
        fixture.tenant,
        "telegram-incoming",
        b"telegram-test-secret",
        "tenant",
    )
    .await
    .expect("seed the incoming secret");
    store::channel::upsert_binding_tx(
        &mut tx,
        fixture.tenant,
        &store::channel::NewChannelBinding {
            channel_kind: "telegram".to_owned(),
            route_key: "default".to_owned(),
            config: json!({
                "chat_id": "-1001234",
                "bot_token_secret": "telegram-token",
                "incoming_secret": "telegram-incoming",
                "api_base": api_base,
            }),
            active: true,
        },
    )
    .await
    .expect("bind the telegram route");
    tx.commit().await.expect("commit the binding");
}

async fn delivery_state(db: &common::TestDb, delivery_id: Uuid) -> (String, i32, bool) {
    let (state, attempts, sent_at): (String, i32, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT state, attempts, sent_at FROM delivery WHERE id = $1")
            .bind(delivery_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the delivery");
    (state, attempts, sent_at.is_some())
}

/// The newest `delivery.send` job for one delivery: `(status, run_after)`.
async fn send_job(db: &common::TestDb, delivery_id: Uuid) -> Option<(String, DateTime<Utc>)> {
    sqlx::query_as(
        "SELECT status, run_after FROM job
          WHERE kind = 'delivery.send' AND dedupe_key = $1
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .bind(store::delivery::delivery_send_dedupe_key(delivery_id))
    .fetch_optional(&db.super_pool)
    .await
    .expect("read the send job")
}

/// Skip the wall-clock wait: make the scheduled retry due now. The assertions have already
/// read the scheduled instant.
async fn make_due(db: &common::TestDb, delivery_id: Uuid) {
    sqlx::query(
        "UPDATE job SET run_after = now()
          WHERE kind = 'delivery.send' AND dedupe_key = $1 AND status = 'pending'",
    )
    .bind(store::delivery::delivery_send_dedupe_key(delivery_id))
    .execute(&db.super_pool)
    .await
    .expect("make the retry due");
}

async fn event_count(db: &common::TestDb, delivery_id: Uuid, state: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE type = 'delivery.changed'
            AND payload->>'delivery_id' = $1
            AND payload->>'state' = $2",
    )
    .bind(delivery_id.to_string())
    .bind(state)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the delivery events")
}

/// Every `delivery.changed` for one delivery, whatever its state.
async fn delivery_changed_count(db: &common::TestDb, delivery_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE type = 'delivery.changed' AND payload->>'delivery_id' = $1",
    )
    .bind(delivery_id.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("count every delivery.changed")
}

async fn wait_for<F, Fut>(what: &str, timeout: Duration, mut predicate: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if predicate().await {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out after {timeout:?} waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `min(2^attempts · 5 s, 900 s)` — spelled independently of the handler.
fn intended_backoff(attempts: i32) -> ChronoDuration {
    let secs = (1i64 << attempts).saturating_mul(5).min(900);
    ChronoDuration::seconds(secs)
}

/// Drive the drainer through every intermediate attempt of one delivery, OBSERVING each
/// scheduled retry before making it due. Returns the observed waits, in attempt order.
///
/// The clock is only skipped, never shortened: every scheduled `run_after` is asserted to be
/// in the future while it is still the scheduled one, and the caller asserts its magnitude.
async fn drive_to_the_budget(db: &common::TestDb, delivery_id: Uuid) -> Vec<ChronoDuration> {
    let mut observed = Vec::new();
    for attempt in 1..store::delivery::DELIVERY_MAX_ATTEMPTS {
        // Wait until the attempt has been recorded AND the retry is scheduled in the future
        // (a `running` job is the handler mid-flight; its run_after is stale).
        wait_for("a scheduled retry", Duration::from_secs(20), || async {
            let (state, attempts, _) = delivery_state(db, delivery_id).await;
            let job = send_job(db, delivery_id).await;
            state == "sent"
                && attempts == attempt
                && job.as_ref().is_some_and(|(status, run_after)| {
                    status == "pending" && *run_after > Utc::now()
                })
        })
        .await;

        let (status, run_after) = send_job(db, delivery_id).await.expect("the send job");
        assert_eq!(
            status, "pending",
            "attempt {attempt}: the job waits for the retry"
        );
        let wait = run_after - Utc::now();
        assert!(
            wait > ChronoDuration::zero(),
            "attempt {attempt}: run_after {run_after} must be in the future"
        );
        observed.push(wait);
        make_due(db, delivery_id).await;
    }

    wait_for(
        "the budget to be spent",
        Duration::from_secs(20),
        || async {
            let (state, attempts, _) = delivery_state(db, delivery_id).await;
            state == "failed" && attempts == store::delivery::DELIVERY_MAX_ATTEMPTS
        },
    )
    .await;
    wait_for(
        "the send job to finish",
        Duration::from_secs(10),
        || async {
            send_job(db, delivery_id)
                .await
                .is_some_and(|(status, _)| status == "succeeded")
        },
    )
    .await;
    observed
}

/// The terminal row's recorded failure, with the one-row invariant checked first.
async fn failed_once(
    db: &common::TestDb,
    fixture: &Fixture,
    message_id: Uuid,
    delivery_id: Uuid,
    expected_attempts: i32,
) -> String {
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM delivery WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(message_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the deliveries");
    assert_eq!(rows, 1, "a failed retry never creates a second row");

    let (state, attempts, sent) = delivery_state(db, delivery_id).await;
    assert_eq!(state, "failed");
    assert_eq!(attempts, expected_attempts);
    assert!(sent, "sent_at was stamped on the first attempt");
    assert_eq!(
        event_count(db, delivery_id, "failed").await,
        1,
        "exactly one terminal delivery.changed"
    );
    let last_error: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM delivery WHERE id = $1")
            .bind(delivery_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read last_error");
    last_error.expect("the terminal row records its failure")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_telegram_transport_exhausts_the_budget_and_the_backoff_grows() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    // The binding, its sealed token and its per-binding API base are present, so the handler
    // reaches the wire; the stub answers 500, which is the ordinary retryable failure (§5.6).
    let stub = telegram_failure_stub().await;
    seed_telegram_binding(&db, &fixture, &stub).await;
    let (message_id, delivery_id) = seed_delivery(&db, &fixture, "telegram").await;
    let _worker = WorkerChild::spawn_telegram(&db);

    let observed = drive_to_the_budget(&db, delivery_id).await;
    assert_eq!(
        observed.len(),
        4,
        "four intermediate attempts: {observed:?}"
    );
    assert!(
        observed.windows(2).all(|pair| pair[1] > pair[0]),
        "the backoff must grow between attempts: {observed:?}"
    );
    for (index, wait) in observed.iter().enumerate() {
        let expected = intended_backoff(index as i32 + 1);
        assert!(
            *wait > expected - ChronoDuration::seconds(3)
                && *wait < expected + ChronoDuration::seconds(3),
            "attempt {}: run_after is {wait:?} away, expected {expected:?} +/- 3s",
            index + 1
        );
    }

    let last_error = failed_once(
        &db,
        &fixture,
        message_id,
        delivery_id,
        store::delivery::DELIVERY_MAX_ATTEMPTS,
    )
    .await;
    assert!(
        last_error.contains("telegram api"),
        "the Bot API failure is recorded on the row: {last_error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unsupported_channel_kind_lands_failed_with_one_clear_row() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    // `delivery.channel_kind` has no CHECK constraint; only `channel_binding` restricts the
    // vocabulary. A kind no transport can ever answer is a PERMANENT failure: no later attempt
    // can succeed, so the row is closed on the first attempt and the job dead-letters instead
    // of retrying four times.
    let (message_id, delivery_id) = seed_delivery(&db, &fixture, "carrier-pigeon").await;
    let _worker = WorkerChild::spawn(&db);

    wait_for(
        "the unsupported delivery to fail permanently",
        Duration::from_secs(20),
        || async { delivery_state(&db, delivery_id).await.0 == "failed" },
    )
    .await;
    let (state, attempts, _) = delivery_state(&db, delivery_id).await;
    assert_eq!(state, "failed");
    assert_eq!(attempts, 1, "a permanent failure is not retried");

    let last_error = failed_once(&db, &fixture, message_id, delivery_id, 1).await;
    assert!(
        last_error.contains("no transport is registered") && last_error.contains("carrier-pigeon"),
        "the terminal row names the kind and why it cannot be sent: {last_error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_web_delivery_is_marked_delivered() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    let (_, delivery_id) = seed_delivery(&db, &fixture, "web").await;
    let _worker = WorkerChild::spawn(&db);

    wait_for(
        "the delivery to be delivered",
        Duration::from_secs(15),
        || async { delivery_state(&db, delivery_id).await.0 == "delivered" },
    )
    .await;
    wait_for(
        "the send job to finish",
        Duration::from_secs(10),
        || async {
            send_job(&db, delivery_id)
                .await
                .is_some_and(|(status, _)| status == "succeeded")
        },
    )
    .await;

    let (state, attempts, sent) = delivery_state(&db, delivery_id).await;
    assert_eq!(state, "delivered");
    assert_eq!(attempts, 1, "a delivered row took exactly one attempt");
    assert!(sent);
    let (delivered_at, last_error): (Option<DateTime<Utc>>, Option<String>) =
        sqlx::query_as("SELECT delivered_at, last_error FROM delivery WHERE id = $1")
            .bind(delivery_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the delivery");
    assert!(delivered_at.is_some());
    assert!(last_error.is_none());
    assert_eq!(event_count(&db, delivery_id, "delivered").await, 1);
}

/// A TERMINAL delivery is never re-sent: the drainer finishes the job quietly, leaving the
/// state and the attempt counter alone and announcing nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_delivery_is_never_re_sent() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    // `seed_delivery` writes the message, the delivery and its `delivery.send` job — the
    // shape `messages.send` commits.
    let (_message_id, delivery_id) = seed_delivery(&db, &fixture, "web").await;

    // Seed the TERMINAL state through the store primitives the channel uses.
    let mut tx = scoped(&db, fixture.tenant).await;
    store::delivery::record_send_attempt_tx(&mut tx, fixture.tenant, delivery_id)
        .await
        .expect("record the first attempt")
        .expect("the row is live");
    store::delivery::mark_delivered_tx(&mut tx, fixture.tenant, delivery_id)
        .await
        .expect("mark delivered")
        .expect("the row is live");
    tx.commit().await.expect("commit the ack");
    let before = delivery_state(&db, delivery_id).await;
    assert_eq!(
        before,
        ("delivered".to_owned(), 1, true),
        "the seeded row is terminal, has one attempt and a sent_at"
    );
    assert_eq!(delivery_changed_count(&db, delivery_id).await, 0);

    let _worker = WorkerChild::spawn(&db);
    wait_for(
        "the terminal delivery's job to finish",
        Duration::from_secs(15),
        || async {
            send_job(&db, delivery_id)
                .await
                .is_some_and(|(status, _)| status == "succeeded")
        },
    )
    .await;

    let after = delivery_state(&db, delivery_id).await;
    assert_eq!(
        after, before,
        "the terminal row is untouched by the drainer"
    );
    assert_eq!(
        delivery_changed_count(&db, delivery_id).await,
        0,
        "a terminal delivery emits no second delivery.changed"
    );
}

/// An EXPIRED delivery is closed before any transport is tried: `expired`, one
/// `delivery.changed`, and no send attempt on the row.
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_delivery_is_closed_without_a_send_attempt() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;

    // A REAL transport kind whose config is absent: if the expiry branch ever moved below
    // the attempt stamp, this row would reach `telegram_send` and land `failed` with one
    // attempt instead of `expired` with none.
    let mut tx = scoped(&db, fixture.tenant).await;
    let message = store::delivery::insert_message_tx(
        &mut tx,
        fixture.tenant,
        &store::delivery::NewMessage::new("telegram", "too late"),
    )
    .await
    .expect("insert the message");
    let mut delivery =
        store::delivery::NewDelivery::new(message.id, "telegram", &fixture.alice.to_string());
    delivery.expires_at = Some(Utc::now() - ChronoDuration::minutes(5));
    let inserted = store::delivery::insert_delivery_tx(&mut tx, fixture.tenant, &delivery)
        .await
        .expect("insert the delivery");
    store::delivery::enqueue_delivery_send_tx(&mut tx, fixture.tenant, inserted.delivery.id, None)
        .await
        .expect("enqueue the send job");
    tx.commit().await.expect("commit the fixture");
    let delivery_id = inserted.delivery.id;

    let _worker = WorkerChild::spawn(&db);
    wait_for(
        "the delivery to expire",
        Duration::from_secs(15),
        || async { delivery_state(&db, delivery_id).await.0 == "expired" },
    )
    .await;
    wait_for(
        "the send job to finish",
        Duration::from_secs(10),
        || async {
            send_job(&db, delivery_id)
                .await
                .is_some_and(|(status, _)| status == "succeeded")
        },
    )
    .await;

    let (state, attempts, sent) = delivery_state(&db, delivery_id).await;
    assert_eq!(state, "expired");
    assert_eq!(
        attempts, 0,
        "expiry is decided before any send attempt is stamped"
    );
    assert!(!sent, "no attempt means no sent_at");
    assert_eq!(
        event_count(&db, delivery_id, "expired").await,
        1,
        "exactly one delivery.changed says expired"
    );
    assert_eq!(
        delivery_changed_count(&db, delivery_id).await,
        1,
        "and it is the only delivery.changed"
    );
}

/// Invoke one action as the seeded owner.
async fn invoke(
    db: &common::TestDb,
    fixture: &Fixture,
    action: &str,
    input: serde_json::Value,
) -> conveyor_actions::InvokeOutcome {
    match conveyor_actions::invoke(
        &db.runtime,
        conveyor_actions::registry::registry(),
        fixture.tenant,
        conveyor_actions::InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: None,
            actor_id: fixture.alice,
            actor_kind: "user".to_owned(),
            role: Role::Admin,
            policy: Default::default(),
            authority: conveyor_actions::Authority::Role,
            request_id: Some("delivery-retry-send".to_owned()),
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        },
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => panic!("{action} refused: {} {}", error.status, error.detail),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn messages_send_writes_one_delivery_and_one_deduped_job_per_recipient() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;

    let outcome = invoke(
        &db,
        &fixture,
        "messages.send",
        json!({
            "recipients": [fixture.alice.to_string(), fixture.bob.to_string()],
            "channel_kind": "web",
            "body": "hello both",
        }),
    )
    .await;
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let output = &outcome.body["output"];
    let message_id = output["message_id"]
        .as_str()
        .expect("message_id")
        .to_owned();
    let delivery_ids: Vec<String> = output["delivery_ids"]
        .as_array()
        .expect("delivery_ids")
        .iter()
        .map(|id| id.as_str().expect("a delivery id").to_owned())
        .collect();
    assert_eq!(delivery_ids.len(), 2, "one delivery per recipient");

    let message = Uuid::parse_str(&message_id).expect("message uuid");
    let (sender, channel_kind): (Option<Uuid>, String) =
        sqlx::query_as("SELECT sender_id, channel_kind FROM message WHERE id = $1")
            .bind(message)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the message");
    assert_eq!(sender, Some(fixture.alice), "the actor is the sender");
    assert_eq!(channel_kind, "web");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM delivery WHERE message_id = $1")
        .bind(message)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the deliveries");
    assert_eq!(rows, 2, "exactly N deliveries for N distinct recipients");

    let dedupe_keys: Vec<String> = delivery_ids
        .iter()
        .map(|id| {
            store::delivery::delivery_send_dedupe_key(Uuid::parse_str(id).expect("a delivery uuid"))
        })
        .collect();
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE kind = 'delivery.send' AND dedupe_key = ANY($1)",
    )
    .bind(&dedupe_keys)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the send jobs");
    assert_eq!(jobs, 2, "one job per delivery, keyed by delivery:<id>");

    // A re-enqueue while the job is live collapses onto it: that is the dedupe the whole
    // retry path rests on.
    let delivery_id = Uuid::parse_str(&delivery_ids[0]).expect("a delivery uuid");
    let mut tx = scoped(&db, fixture.tenant).await;
    let replay =
        store::delivery::enqueue_delivery_send_tx(&mut tx, fixture.tenant, delivery_id, None)
            .await
            .expect("re-enqueue");
    tx.commit().await.expect("commit the replay");
    assert!(replay.is_none(), "a live deduped job is not duplicated");

    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE type = 'message.sent' AND payload->>'message_id' = $1 AND topic = $2",
    )
    .bind(&message_id)
    .bind(format!("message:{message_id}"))
    .fetch_one(&db.super_pool)
    .await
    .expect("count message.sent");
    assert_eq!(events, 1, "one message.sent on the message's own topic");
}

/// Enqueue one job outside the action surface, as the scheduler would.
async fn enqueue_kind(db: &common::TestDb, tenant_id: Uuid, kind: &str, dedupe_key: &str) {
    sqlx::query("INSERT INTO job (tenant_id, kind, dedupe_key) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(kind)
        .bind(dedupe_key)
        .execute(&db.super_pool)
        .await
        .expect("enqueue the job");
}

async fn job_status(db: &common::TestDb, kind: &str, dedupe_key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM job WHERE kind = $1 AND dedupe_key = $2")
        .bind(kind)
        .bind(dedupe_key)
        .fetch_optional(&db.super_pool)
        .await
        .expect("read the job")
}

#[tokio::test(flavor = "multi_thread")]
async fn approval_closed_emit_hands_each_closed_gate_off_once() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    let approval_id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status)
         VALUES ($1, 'close', 'gate', 'approved') RETURNING id",
    )
    .bind(fixture.tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a closed gate");
    let _worker = WorkerChild::spawn(&db);

    // No mcp binding: nothing can be handed off, so no message is written.
    enqueue_kind(&db, fixture.tenant_id, "approval.closed.emit", "close-1").await;
    wait_for(
        "the emitter to run without a binding",
        Duration::from_secs(15),
        || async {
            job_status(&db, "approval.closed.emit", "close-1")
                .await
                .as_deref()
                == Some("succeeded")
        },
    )
    .await;
    assert_eq!(closed_message_count(&db, &fixture, approval_id).await, 0);

    // Bind the mcp route; the same closed gate is now handed off exactly once.
    let mut tx = scoped(&db, fixture.tenant).await;
    store::channel::upsert_binding_tx(
        &mut tx,
        fixture.tenant,
        &store::channel::NewChannelBinding::new("mcp", "default"),
    )
    .await
    .expect("bind the mcp route");
    tx.commit().await.expect("commit the binding");

    enqueue_kind(&db, fixture.tenant_id, "approval.closed.emit", "close-2").await;
    wait_for(
        "the emitter to hand the gate off",
        Duration::from_secs(15),
        || async { closed_message_count(&db, &fixture, approval_id).await == 1 },
    )
    .await;
    wait_for(
        "the emitter job to finish",
        Duration::from_secs(10),
        || async {
            job_status(&db, "approval.closed.emit", "close-2")
                .await
                .as_deref()
                == Some("succeeded")
        },
    )
    .await;

    let (channel_kind, recipient_ref): (String, String) = sqlx::query_as(
        "SELECT d.channel_kind, d.recipient_ref
           FROM delivery d JOIN message m ON m.id = d.message_id
          WHERE m.kind = 'approval-closed' AND m.correlation_id = $1",
    )
    .bind(approval_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("the hand-off delivery");
    assert_eq!(channel_kind, "mcp");
    assert_eq!(recipient_ref, "default");

    // A replay finds the message it already wrote: still one message, one delivery.
    enqueue_kind(&db, fixture.tenant_id, "approval.closed.emit", "close-3").await;
    wait_for("the replay to finish", Duration::from_secs(15), || async {
        job_status(&db, "approval.closed.emit", "close-3")
            .await
            .as_deref()
            == Some("succeeded")
    })
    .await;
    assert_eq!(
        closed_message_count(&db, &fixture, approval_id).await,
        1,
        "the NOT EXISTS makes the emitter idempotent"
    );
}

/// How many `approval-closed` messages one approval has in the fixture's tenant.
async fn closed_message_count(db: &common::TestDb, fixture: &Fixture, approval_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM message
          WHERE tenant_id = $1 AND kind = 'approval-closed' AND correlation_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(approval_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the close messages")
}
