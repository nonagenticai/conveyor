//! S06d — the generic inbound path, end to end against real Postgres and a stub Bot API.
//!
//! The route is a connector callback, so the assertions are about what the CALLBACK does with
//! the per-binding secret and the update body: the same `approvals.decide` the web calls is
//! what records the decision, the decision names the tenant's **service identity** (not a human),
//! the transport and sender are recorded in `via`, and every well-formed-but-uninteresting
//! update is acknowledged (`200`) instead of retried.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::routing::any;
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

const CHAT_ID: i64 = 42;
const BOT_TOKEN_SECRET: &str = "telegram-bot-token";
const INCOMING_SECRET: &str = "telegram-incoming";
const INCOMING_VALUE: &str = "inbound-s3cret";

/// A stub Bot API: any path answers 200 and counts a hit.
struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn stub() -> Stub {
    let hits = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = hits.clone();
    let kept = bodies.clone();
    let app = axum::Router::new().fallback(any(move |Json(body): Json<Value>| {
        let seen = seen.clone();
        let kept = kept.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            kept.lock().expect("bodies").push(body);
            Json(json!({"ok": true}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let addr = listener.local_addr().expect("local addr");
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Stub {
        base: format!("http://{addr}"),
        hits,
        bodies,
        task,
    }
}

/// The app under test.
struct Served {
    base: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Served {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(pg: &support::Pg) -> Served {
    let state = conveyor_api::AppState::connect(support::config(pg))
        .await
        .expect("the app state");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let app = conveyor_api::app(state);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Served {
        base: format!("http://{addr}"),
        task,
    }
}

struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    binding_id: Uuid,
    service_user: Uuid,
    approval_id: Uuid,
    stub: Stub,
}

async fn fixture(pg: &support::Pg) -> Fixture {
    let stub = stub().await;
    let seeded = support::seed(pg, "dev-a", "alice", "Alice").await;
    // `projects.create` builds its board from the catalogue's standard template.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("seed the catalogue");

    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        BOT_TOKEN_SECRET,
        b"123:token",
        "tenant",
    )
    .await
    .expect("bot token secret");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        INCOMING_SECRET,
        INCOMING_VALUE.as_bytes(),
        "tenant",
    )
    .await
    .expect("incoming secret");
    let binding = conveyor_store::channel::upsert_binding_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::channel::NewChannelBinding {
            channel_kind: "telegram".to_owned(),
            route_key: "default".to_owned(),
            config: json!({
                "chat_id": CHAT_ID.to_string(),
                "bot_token_secret": BOT_TOKEN_SECRET,
                "incoming_secret": INCOMING_SECRET,
                "api_base": stub.base,
            }),
            active: true,
        },
    )
    .await
    .expect("bind the telegram route");
    let service_user =
        conveyor_store::channel::ensure_service_account_tx(&mut tx, seeded.tenant, "telegram")
            .await
            .expect("provision the connector identity");
    tx.commit().await.expect("commit");

    let approval_id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status)
         VALUES ($1, 'manual', 'gate', 'pending') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed a pending approval");

    Fixture {
        tenant: seeded.tenant,
        tenant_id: seeded.tenant_id,
        binding_id: binding.id,
        service_user,
        approval_id,
        stub,
    }
}

async fn post(
    served: &Served,
    binding_id: Uuid,
    secret: Option<&str>,
    body: Value,
) -> (u16, Value) {
    let client = reqwest::Client::new();
    let mut request = client
        .post(format!("{}/api/channels/{binding_id}/inbound", served.base))
        .json(&body);
    if let Some(secret) = secret {
        request = request.header("x-telegram-bot-api-secret-token", secret);
    }
    let response = request.send().await.expect("inbound request");
    let status = response.status().as_u16();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

fn update(update_id: i64, text: String) -> Value {
    json!({
        "update_id": update_id,
        "message": {
            "chat": {"id": CHAT_ID},
            "from": {"id": 99},
            "text": text,
        }
    })
}

async fn decisions(
    pg: &support::Pg,
    tenant_id: Uuid,
    approval_id: Uuid,
) -> Vec<(Uuid, String, Value)> {
    sqlx::query_as(
        "SELECT approver_id, decided_by_kind, via FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2 ORDER BY created_at",
    )
    .bind(tenant_id)
    .bind(approval_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("read decisions")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_is_the_service_identitys_decision_recorded_with_its_transport() {
    let pg = support::Pg::fresh().await;
    let fixture = fixture(&pg).await;
    let served = serve(&pg).await;

    let (status, body) = post(
        &served,
        fixture.binding_id,
        Some(INCOMING_VALUE),
        update(1, format!("approve {} looks right", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    let rows = decisions(&pg, fixture.tenant_id, fixture.approval_id).await;
    assert_eq!(rows.len(), 1, "one reply is one decision");
    assert_eq!(
        rows[0].0, fixture.service_user,
        "the connector identity, not a human"
    );
    assert_eq!(rows[0].1, "service");
    assert_eq!(rows[0].2["channel"], "telegram");
    assert_eq!(rows[0].2["sender_ref"], "tg:99");

    // A replay of the same update does not decide twice.
    let (status, _) = post(
        &served,
        fixture.binding_id,
        Some(INCOMING_VALUE),
        update(1, format!("approve {}", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        decisions(&pg, fixture.tenant_id, fixture.approval_id)
            .await
            .len(),
        1
    );

    // The chat got an acknowledgement through the binding's own (DB-stored) token.
    assert!(fixture.stub.hits.load(Ordering::SeqCst) >= 1);
    let _ = fixture.tenant;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_revoked_grant_refuses_the_command_and_a_granted_create_runs() {
    let pg = support::Pg::fresh().await;
    let fixture = fixture(&pg).await;
    let served = serve(&pg).await;

    // Revoke approvals.decide for the telegram kind.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, fixture.tenant)
        .await
        .expect("scope");
    conveyor_store::rbac::set_channel_policy_tx(
        &mut tx,
        fixture.tenant,
        "telegram",
        &[("approvals.decide", false)],
    )
    .await
    .expect("revoke");
    tx.commit().await.expect("commit");

    let (status, _) = post(
        &served,
        fixture.binding_id,
        Some(INCOMING_VALUE),
        update(2, format!("approve {}", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 200, "a business refusal is acknowledged");
    assert!(
        decisions(&pg, fixture.tenant_id, fixture.approval_id)
            .await
            .is_empty(),
        "a revoked grant writes no decision"
    );

    // `create` is in the compiled default and is still granted.
    let (status, _) = post(
        &served,
        fixture.binding_id,
        Some(INCOMING_VALUE),
        update(3, "create A project from the chat".to_owned()),
    )
    .await;
    assert_eq!(status, 200);
    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(fixture.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count projects");
    assert_eq!(
        projects,
        1,
        "the granted create command ran; the chat was told: {:?}",
        fixture.stub.bodies.lock().expect("bodies")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_callback_refuses_a_bad_secret_and_acks_a_foreign_chat() {
    let pg = support::Pg::fresh().await;
    let fixture = fixture(&pg).await;
    let served = serve(&pg).await;

    let (status, _) = post(
        &served,
        fixture.binding_id,
        Some("wrong"),
        update(4, format!("approve {}", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 401);
    assert!(
        decisions(&pg, fixture.tenant_id, fixture.approval_id)
            .await
            .is_empty()
    );

    let (status, _) = post(
        &served,
        fixture.binding_id,
        None,
        update(5, format!("approve {}", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 401);

    // A message for another chat is consumed without a decision.
    let foreign = json!({
        "update_id": 6,
        "message": {"chat": {"id": 7}, "text": format!("approve {}", fixture.approval_id)}
    });
    let (status, _) = post(&served, fixture.binding_id, Some(INCOMING_VALUE), foreign).await;
    assert_eq!(status, 200);
    assert!(
        decisions(&pg, fixture.tenant_id, fixture.approval_id)
            .await
            .is_empty()
    );

    // An unknown binding is acknowledged, never a 404 the platform would retry.
    let unknown = Uuid::now_v7();
    let (status, _) = post(
        &served,
        unknown,
        Some(INCOMING_VALUE),
        update(7, "help".to_owned()),
    )
    .await;
    assert_eq!(status, 200);
}

/// The same standard with a second adapter: a JSON gateway is a binding, not a deploy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_webhook_gateway_uses_the_shared_grammar_and_the_same_identity_model() {
    let pg = support::Pg::fresh().await;
    let stub = stub().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "hook-out",
        b"out-token",
        "tenant",
    )
    .await
    .expect("outbound secret");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "hook-in",
        b"in-token",
        "tenant",
    )
    .await
    .expect("inbound secret");
    let binding = conveyor_store::channel::upsert_binding_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::channel::NewChannelBinding {
            channel_kind: "webhook".to_owned(),
            route_key: "default".to_owned(),
            config: json!({
                "endpoint_url": stub.base,
                "incoming_secret": "hook-in",
                "outbound_secret": "hook-out",
            }),
            active: true,
        },
    )
    .await
    .expect("bind the webhook");
    let service_user =
        conveyor_store::channel::ensure_service_account_tx(&mut tx, seeded.tenant, "webhook")
            .await
            .expect("provision the webhook identity");
    tx.commit().await.expect("commit");

    let approval_id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status)
         VALUES ($1, 'manual', 'gate', 'pending') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("approval");

    let served = serve(&pg).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/api/channels/{}/inbound",
            served.base, binding.id
        ))
        .header("authorization", "Bearer in-token")
        .json(&json!({
            "id": "evt-1",
            "sender": "mailto:alice@example.invalid",
            "text": format!("approve {approval_id} via mail"),
        }))
        .send()
        .await
        .expect("inbound");
    assert_eq!(response.status().as_u16(), 200);

    let rows: Vec<(Uuid, String, Value)> = sqlx::query_as(
        "SELECT approver_id, decided_by_kind, via FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("decisions");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, service_user);
    assert_eq!(rows[0].1, "service");
    assert_eq!(rows[0].2["channel"], "webhook");
    assert_eq!(rows[0].2["sender_ref"], "mailto:alice@example.invalid");
}
