//! S06d — the generic inbound path, end to end against real Postgres and a stub Bot API.
//!
//! The route is a connector callback, so the assertions are about what the CALLBACK does with
//! the per-binding secret and the update body: the same `approvals.decide` the web calls is
//! what records the decision. A decision-bearing command is attributed to the sender's mapped
//! tenant principal (the binding's `senders` identity map), never to the connector's service
//! account; an unmapped or revoked sender is refused before any decision row exists. Every
//! other command still acts as the service identity, and the transport + sender are recorded
//! in `via`.

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
    /// The seeded human the binding's `senders` map points `tg:99` at.
    principal: Uuid,
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
                "senders": {"tg:99": seeded.user_id},
            }),
            active: true,
        },
    )
    .await
    .expect("bind the telegram route");
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

    // The connector identity is a service principal; §5.4's human-only default applies unless
    // the tenant's policy declares this gate service-decidable (migration 0093).
    sqlx::query(
        "INSERT INTO approval_policy
             (tenant_id, action, approval, self_approval, approver_kind, conditions,
              required_decisions, updated_by)
         VALUES ($1, 'manual', 'required', 'forbidden', 'service', '{}'::jsonb, 1, $2)
         ON CONFLICT (tenant_id, action) DO UPDATE
            SET approver_kind = 'service', updated_by = excluded.updated_by",
    )
    .bind(seeded.tenant_id)
    .bind(seeded.user_id)
    .execute(&pg.super_pool)
    .await
    .expect("declare the manual gate service-decidable");

    Fixture {
        tenant: seeded.tenant,
        tenant_id: seeded.tenant_id,
        binding_id: binding.id,
        principal: seeded.user_id,
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

fn update_from(update_id: i64, from: i64, text: String) -> Value {
    json!({
        "update_id": update_id,
        "message": {
            "chat": {"id": CHAT_ID},
            "from": {"id": from},
            "text": text,
        }
    })
}

fn update(update_id: i64, text: String) -> Value {
    update_from(update_id, 99, text)
}

/// The text of the last reply the connector sent to the chat.
fn last_reply(fixture: &Fixture) -> String {
    let bodies = fixture.stub.bodies.lock().expect("bodies");
    bodies
        .last()
        .and_then(|body| body.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
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
async fn a_mapped_senders_reply_is_that_principals_decision_with_its_transport() {
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
        rows[0].0, fixture.principal,
        "the mapped tenant principal, not the connector's service account"
    );
    assert_eq!(rows[0].1, "human");
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
async fn an_unmapped_sender_is_refused_and_writes_no_decision() {
    let pg = support::Pg::fresh().await;
    let fixture = fixture(&pg).await;
    let served = serve(&pg).await;

    // tg:100 is in the chat but not in the binding's identity map.
    let (status, _) = post(
        &served,
        fixture.binding_id,
        Some(INCOMING_VALUE),
        update_from(11, 100, format!("approve {}", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 200, "a refusal is acknowledged, not retried");
    assert!(
        decisions(&pg, fixture.tenant_id, fixture.approval_id)
            .await
            .is_empty(),
        "an unmapped sender writes no decision row"
    );
    assert!(
        last_reply(&fixture).contains("channel_sender_unmapped"),
        "the transport-typed refusal reached the sender: {:?}",
        last_reply(&fixture)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mapped_sender_with_no_membership_is_refused_as_revoked() {
    let pg = support::Pg::fresh().await;
    let fixture = fixture(&pg).await;
    let served = serve(&pg).await;

    // The map now points tg:99 at a principal with no membership in this tenant.
    let (_, binding) = conveyor_store::channel::find_binding_by_id(&pg.owner, fixture.binding_id)
        .await
        .expect("read the binding")
        .expect("the binding exists");
    let mut config = binding.config.clone();
    config["senders"] = json!({"tg:99": Uuid::now_v7()});
    let mut tx = conveyor_store::begin_scoped(&pg.owner, fixture.tenant)
        .await
        .expect("scope");
    conveyor_store::channel::upsert_binding_tx(
        &mut tx,
        fixture.tenant,
        &conveyor_store::channel::NewChannelBinding {
            channel_kind: "telegram".to_owned(),
            route_key: binding.route_key.clone(),
            config,
            active: true,
        },
    )
    .await
    .expect("rebind with the revoked principal");
    tx.commit().await.expect("commit");

    let (status, _) = post(
        &served,
        fixture.binding_id,
        Some(INCOMING_VALUE),
        update(12, format!("approve {}", fixture.approval_id)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        decisions(&pg, fixture.tenant_id, fixture.approval_id)
            .await
            .is_empty(),
        "a revoked sender writes no decision row"
    );
    assert!(
        last_reply(&fixture).contains("channel_sender_revoked"),
        "the refusal reached the sender: {:?}",
        last_reply(&fixture)
    );
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
                "senders": {"mailto:alice@example.invalid": seeded.user_id},
            }),
            active: true,
        },
    )
    .await
    .expect("bind the webhook");
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

    // The gate keeps the default human approver kind: the sender is mapped to a tenant
    // principal, so the decision is a person's and needs no service-approver policy row.

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
    assert_eq!(
        rows[0].0, seeded.user_id,
        "the mapped sender's tenant principal, not the service account"
    );
    assert_eq!(rows[0].1, "human");
    assert_eq!(rows[0].2["channel"], "webhook");
    assert_eq!(rows[0].2["sender_ref"], "mailto:alice@example.invalid");
}
