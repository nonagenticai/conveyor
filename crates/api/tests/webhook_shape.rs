//! S05 §7 property (d): the webhook envelope, and the CRUD around it.
//!
//! Proved here, against the served router and a real Postgres:
//!
//! * `webhooks.create` is `201 + ETag` and is the ONE response carrying the secret; the read
//!   route (`GET /api/boards/{id}/webhooks`) and `webhooks.update` omit the member entirely;
//! * `webhooks.update`/`webhooks.delete` require `If-Match`, and a stale one is `412` with
//!   the current subscription in the body;
//! * `webhooks.test` delivers synchronously to an HTTP recorder, with
//!   `Authorization: Bearer <secret>`, and reports `WebhookDeliveryOut`;
//! * a real `cards.move` writes ONE `webhook.deliver` job whose `{event, data, prevData,
//!   user}` body is exactly the plan's envelope, with `data.list_id` the new lane and
//!   `prevData.list_id` the old one. The mutation itself makes no HTTP call: the recorder is
//!   empty until the job is drained;
//! * `events[]`/`excluded_events[]` select the subscription — a `card.updated` emits no job
//!   for a `card.moved`-only subscription.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};

// ── the delivery recorder ─────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Delivery {
    auth: Option<String>,
    body: Value,
}

struct Recorder {
    base: String,
    deliveries: Arc<Mutex<Vec<Delivery>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Recorder {
    async fn start() -> Recorder {
        let deliveries = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/hook", post(record))
            .with_state(deliveries.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind recorder");
        let addr = listener.local_addr().expect("recorder addr");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Recorder {
            base: format!("http://{addr}/hook"),
            deliveries,
            task,
        }
    }

    fn take(&self) -> Vec<Delivery> {
        std::mem::take(&mut *self.deliveries.lock().expect("recorder"))
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn record(
    State(state): State<Arc<Mutex<Vec<Delivery>>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    state.lock().expect("recorder").push(Delivery {
        auth: headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        body,
    });
    StatusCode::OK
}

// ── fixtures ──────────────────────────────────────────────────────────────────────────

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
    support::seed(&pg, "dev-b", "bob", "Bob").await;
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

async fn job_count(pg: &support::Pg, tenant_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'webhook.deliver'")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("job count")
}

fn assert_envelope_shape(body: &Value) {
    let object = body
        .as_object()
        .unwrap_or_else(|| panic!("the webhook body is not an object: {body}"));
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = ["event", "data", "prevData", "user"].into_iter().collect();
    assert_eq!(
        actual, expected,
        "the envelope is exactly {{event,data,prevData,user}}"
    );
    assert!(
        body["user"].is_object(),
        "user is an object: {}",
        body["user"]
    );
    assert!(body["user"]["id"].is_string(), "user.id is a string");
    assert!(body["user"]["name"].is_string(), "user.name is a string");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn webhook_crud_is_write_once_and_the_card_move_envelope_is_exact() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "Webhooks").await;
    let active = lanes["active"].clone();
    let waiting = lanes["waiting"].clone();
    let recorder = Recorder::start().await;

    let (status, created) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": active,
                "name": "Observed",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let card_id = id_of(output(&created), "id");

    // ── the event vocabulary is the register ─────────────────────────────────────────────
    let (status, refused) = running
        .invoke(
            "webhooks.create",
            "dev:alice",
            json!({"board_id": board_id, "url": recorder.base, "events": ["list.changed"]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "events", "{refused}");
    assert!(
        refused["errors"][0]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("card.moved")),
        "the refusal names the real vocabulary: {refused}"
    );

    let (status, refused) = running
        .invoke(
            "webhooks.create",
            "dev:alice",
            json!({"board_id": board_id, "url": "file:///etc/passwd"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 422,
        "only http/https endpoints are webhooks: {refused}"
    );

    // ── create: 201 + ETag + the secret, ONCE ───────────────────────────────────────────
    let (status, headers, created) = running
        .invoke_full(
            "webhooks.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "url": recorder.base,
                "events": ["card.moved"],
                "excluded_events": ["card.updated"],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let webhook_id = id_of(output(&created), "id");
    let secret = output(&created)["secret"]
        .as_str()
        .unwrap_or_else(|| panic!("create must return the secret once: {created}"))
        .to_owned();
    assert_eq!(secret.len(), 64, "32 random bytes, hex");
    assert_eq!(output(&created)["version"], 1);
    assert_eq!(output(&created)["events"], json!(["card.moved"]));

    // The secret is sealed under the tenant KEK (migration 0014) and never returned by a read. Before
    // 0014 this assertion read `stored == secret` — the column held the bearer token in the clear, and a
    // leak that passes is the one thing this test must not do again.
    let (stored, key_id): (Option<Vec<u8>>, Option<String>) = sqlx::query_as(
        "SELECT secret_ciphertext, secret_key_id FROM webhook_subscription WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&webhook_id).expect("uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("stored secret");
    let stored = stored.expect("a sealed secret");
    assert_ne!(
        stored,
        secret.as_bytes(),
        "the row stores the sealed value, not the token"
    );
    assert_eq!(key_id.as_deref(), Some("kek:v1"));

    let (status, listed) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{board_id}/webhooks"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{listed}");
    assert!(
        items[0].get("secret").is_none(),
        "the read route must omit the secret member: {}",
        items[0]
    );
    assert!(
        !listed.to_string().contains(&secret),
        "the secret must not appear anywhere in the read document"
    );

    // ── update: If-Match required, stale is 412 with the current row, no secret ─────────
    let (status, refused) = running
        .invoke(
            "webhooks.update",
            "dev:alice",
            json!({"id": webhook_id, "active": false}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 412,
        "an edit without If-Match is refused: {refused}"
    );
    assert_eq!(refused["current"]["version"], 1);

    let (status, headers, updated) = running
        .invoke_full(
            "webhooks.update",
            "dev:alice",
            json!({"id": webhook_id, "url": recorder.base, "events": ["card.moved"], "active": true}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert!(
        output(&updated).get("secret").is_none(),
        "update must not re-return the secret: {updated}"
    );

    let (status, stale) = running
        .invoke(
            "webhooks.update",
            "dev:alice",
            json!({"id": webhook_id, "active": false}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");
    assert_eq!(stale["current"]["version"], 2);

    // ── webhooks.test: ONE synchronous delivery, bearer secret, the same envelope ──────
    let (status, tested) = running
        .invoke(
            "webhooks.test",
            "dev:alice",
            json!({"id": webhook_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{tested}");
    assert_eq!(output(&tested)["ok"], true, "{tested}");
    assert_eq!(output(&tested)["status_code"], 200);
    assert_eq!(output(&tested)["attempts"], 1);
    assert_eq!(output(&tested)["webhook_id"], webhook_id.as_str());

    let expected_auth = format!("Bearer {secret}");
    let deliveries = recorder.take();
    assert_eq!(
        deliveries.len(),
        1,
        "the test action delivers synchronously"
    );
    assert_eq!(deliveries[0].auth.as_deref(), Some(expected_auth.as_str()));
    assert_eq!(deliveries[0].body["event"], "webhook.test");
    assert_eq!(deliveries[0].body["prevData"], Value::Null);
    assert_eq!(deliveries[0].body["user"]["id"], seeded.user_id.to_string());
    assert_eq!(deliveries[0].body["user"]["name"], "Alice");
    assert_envelope_shape(&deliveries[0].body);

    // ── a real move: one job, exact envelope, no inline HTTP ───────────────────────────
    let (status, moved) = running
        .invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": waiting}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(output(&moved)["list_id"], waiting.as_str());

    assert!(
        recorder.take().is_empty(),
        "the mutation itself must not POST; delivery is a job (S5-D7)"
    );
    assert_eq!(job_count(&pg, seeded.tenant_id).await, 1);

    let (dedupe_key, payload, max_attempts): (String, Value, i32) = sqlx::query_as(
        "SELECT dedupe_key, payload, max_attempts FROM job
          WHERE tenant_id = $1 AND kind = 'webhook.deliver'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("delivery job");
    let event_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM event WHERE tenant_id = $1 AND type = 'card.moved'
          ORDER BY seq DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("card.moved event");
    assert_eq!(dedupe_key, format!("{event_id}:{webhook_id}"));
    assert_eq!(max_attempts, 3, "S05 §5: three attempts");
    assert_eq!(payload["subscription_id"], webhook_id.as_str());

    let body = &payload["body"];
    assert_envelope_shape(body);
    assert_eq!(body["event"], "card.moved");
    assert_eq!(
        body["data"]["list_id"], waiting,
        "data is the NEW lane's representation"
    );
    assert_eq!(
        body["prevData"]["list_id"], active,
        "prevData is the OLD lane's representation"
    );
    assert_eq!(body["data"]["id"], card_id.as_str());
    assert_eq!(body["user"]["id"], seeded.user_id.to_string());
    assert_eq!(body["user"]["name"], "Alice");

    // ── drain: the worker's own sender, with the stored secret ─────────────────────────
    let target = conveyor_store::webhook::delivery_target(
        &pg.runtime,
        seeded.tenant,
        uuid::Uuid::parse_str(&webhook_id).expect("uuid"),
    )
    .await
    .expect("delivery target")
    .expect("the subscription exists");
    assert_eq!(target.url, recorder.base);
    let secret_text = target
        .secret
        .as_deref()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    let outcome = conveyor_actions::board::webhooks::deliver_once(
        target.id,
        &target.url,
        secret_text.as_deref(),
        body,
    )
    .await;
    assert!(outcome.ok, "{outcome:?}");
    let delivered = recorder.take();
    assert_eq!(delivered.len(), 1, "one job, one POST");
    assert_eq!(delivered[0].auth.as_deref(), Some(expected_auth.as_str()));
    assert_eq!(&delivered[0].body, body);

    // ── selection: a card.updated is NOT delivered to a card.moved-only subscription ───
    let (status, updated_card) = running
        .invoke(
            "cards.update",
            "dev:alice",
            json!({"id": card_id, "name": "Renamed"}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated_card}");
    assert_eq!(
        job_count(&pg, seeded.tenant_id).await,
        1,
        "card.updated is excluded by `events: [card.moved]`"
    );
    assert!(recorder.take().is_empty());

    let (status, moved_again) = running
        .invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": active}),
            Some("\"3\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved_again}");
    assert_eq!(
        job_count(&pg, seeded.tenant_id).await,
        2,
        "a second matching event queues a second job"
    );

    // ── delete: If-Match, 204, then the row is gone ────────────────────────────────────
    let (status, refused) = running
        .invoke(
            "webhooks.delete",
            "dev:alice",
            json!({"id": webhook_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{refused}");
    let (status, deleted) = running
        .invoke(
            "webhooks.delete",
            "dev:alice",
            json!({"id": webhook_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{deleted}");
    let (status, listed) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{board_id}/webhooks"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(listed["items"].as_array().expect("items").len(), 0);
    let (status, gone) = running
        .invoke(
            "webhooks.delete",
            "dev:alice",
            json!({"id": webhook_id}),
            Some("\"3\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{gone}");

    // Every CRUD step announced itself in the outbox.
    let types: Vec<String> = sqlx::query_scalar(
        "SELECT type FROM event WHERE tenant_id = $1 AND type = 'webhook.changed'
          ORDER BY seq",
    )
    .bind(seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("webhook.changed events");
    assert_eq!(types.len(), 3, "create + update + delete");

    running.shutdown().await;
}

/// Finding #6 — the address policy at the action boundary. A literal metadata-service address is
/// refused at registration, by field; a private target is accepted, because every legitimate
/// webhook target on this estate is private.
///
/// Loopback is allowed in this process by the harness (`CONVEYOR_WEBHOOK_ALLOW_LOOPBACK`), because
/// the recorder is a loopback server; the refusal of loopback itself is proved by the unit test in
/// `crates/actions/src/board/webhooks.rs`, which runs without that variable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_webhook_may_not_target_the_metadata_service() {
    let (_pg, running, _seeded) = fixture().await;

    let (status, refused) = running
        .invoke(
            "webhooks.create",
            "dev:alice",
            json!({"url": "http://169.254.169.254/latest/meta-data/", "events": ["card.moved"]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "url", "{refused}");
    assert!(
        refused["errors"][0]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("link-local")),
        "the refusal names the address class: {refused}"
    );

    let (status, created) = running
        .invoke(
            "webhooks.create",
            "dev:alice",
            json!({"url": "http://10.0.0.5:8080/hook", "events": ["card.moved"]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 201,
        "a private target is the product's own case: {created}"
    );

    running.shutdown().await;
}
