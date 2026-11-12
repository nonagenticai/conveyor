//! S06d — multicast and first response, end to end.
//!
//! One pending approval is offered to every eligible channel: the human web inboxes whose role
//! policy allows the response and the active webhook binding whose grant allows it. A second
//! tick inserts nothing. When one person answers, the close sweep expires the losing recipients
//! and sends each one closure notice — the data model's original "first-response-counts" rule,
//! now with a dispatcher.

mod common;

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Json;
use axum::routing::any;
use conveyor_domain::{Decision, TenantId};
use conveyor_store as store;
use serde_json::json;
use uuid::Uuid;

/// The tenant KEK the test process and the child worker share.
const TEST_KEK: &str = "Y29udmV5b3ItdGVzdC1rZWstMzJieXRlcy1sb25nISE=";

fn set_kek() {
    // SAFETY: set before the first `sealer()` call in this binary; the keystore caches it once.
    unsafe {
        std::env::set_var("CONVEYOR_SECRET_KEK", TEST_KEK);
    }
}

struct WorkerChild {
    child: Child,
}

impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_worker(db: &common::TestDb) -> WorkerChild {
    let child = Command::new(env!("CARGO_BIN_EXE_conveyor"))
        .arg("worker")
        .env("APP_ENV", "test")
        .env("DATABASE_URL", &db.runtime_url)
        .env("DATABASE_LISTEN_URL", &db.runtime_url)
        .env("DATABASE_MIGRATE_URL", &db.runtime_url)
        .env("APP_KEY", "conveyor")
        .env("JOB_LEASE_SECS", "2")
        .env("CONVEYOR_SECRET_KEK", TEST_KEK)
        .env("RUST_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the conveyor worker binary");
    WorkerChild { child }
}

struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn stub() -> Stub {
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    let app = axum::Router::new().fallback(any(move || {
        let seen = seen.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            Json(json!({ "ok": true }))
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
        task,
    }
}

struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    admin: Uuid,
    member: Uuid,
    viewer: Uuid,
    approval: Uuid,
    binding: Uuid,
    stub: Stub,
}

async fn seed(db: &common::TestDb) -> Fixture {
    set_kek();
    let stub = stub().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('dispatch', 'Dispatch') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let mut users = Vec::new();
    for (name, role) in [
        ("admin", "admin"),
        ("member", "member"),
        ("viewer", "viewer"),
    ] {
        let user: Uuid = sqlx::query_scalar(
            "INSERT INTO app_user (display_name, kind) VALUES ($1, 'human') RETURNING id",
        )
        .bind(name)
        .fetch_one(&db.super_pool)
        .await
        .expect("user");
        sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(tenant_id)
            .bind(user)
            .bind(role)
            .execute(&db.super_pool)
            .await
            .expect("membership");
        users.push(user);
    }
    let [admin, member, viewer] = [users[0], users[1], users[2]];

    let mut tx = store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope");
    store::secret::create_secret_tx(&mut tx, tenant, "hook-out", b"out-token", "tenant")
        .await
        .expect("outbound secret");
    store::secret::create_secret_tx(&mut tx, tenant, "hook-in", b"in-token", "tenant")
        .await
        .expect("inbound secret");
    let binding = store::channel::upsert_binding_tx(
        &mut tx,
        tenant,
        &store::channel::NewChannelBinding {
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
    tx.commit().await.expect("commit");

    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, meta)
         VALUES ($1, 'review', 'gate', 'pending', $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(json!({"description": "Please review the change"}))
    .fetch_one(&db.super_pool)
    .await
    .expect("approval");

    Fixture {
        tenant,
        tenant_id,
        admin,
        member,
        viewer,
        approval,
        binding: binding.id,
        stub,
    }
}

async fn tick(db: &common::TestDb, tenant_id: Uuid, kind: &str, key: &str) {
    sqlx::query("INSERT INTO job (tenant_id, kind, dedupe_key) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(kind)
        .bind(key)
        .execute(&db.super_pool)
        .await
        .expect("enqueue the sweep job");
}

async fn wait_for<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_approval_is_multicast_then_the_losers_are_closed() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    let _worker = spawn_worker(&db);
    tick(&db, fixture.tenant_id, "interaction.dispatch", "dispatch-1").await;

    let tenant = fixture.tenant_id;
    wait_for("three dispatches", || async {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM interaction_dispatch WHERE tenant_id = $1")
                .bind(tenant)
                .fetch_one(&db.super_pool)
                .await
                .unwrap_or(0);
        count == 3
    })
    .await;

    let recipients: Vec<(String, String)> = sqlx::query_as(
        "SELECT channel_kind, recipient_ref FROM interaction_dispatch WHERE tenant_id = $1 ORDER BY channel_kind, recipient_ref",
    )
    .bind(tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("dispatches");
    // Two humans who hold approvals:write (admin + member); the viewer does not.
    assert_eq!(
        recipients,
        vec![
            ("web".to_owned(), fixture.admin.to_string()),
            ("web".to_owned(), fixture.member.to_string()),
            ("webhook".to_owned(), format!("binding:{}", fixture.binding)),
        ],
        "the viewer is excluded by the response scope"
    );
    assert!(
        !recipients
            .iter()
            .any(|(_, recipient)| recipient == &fixture.viewer.to_string())
    );

    // A second tick is a no-op (the dispatch PK is the idempotency).
    tick(&db, fixture.tenant_id, "interaction.dispatch", "dispatch-2").await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM interaction_dispatch WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("count");
    assert_eq!(count, 3, "a replayed dispatch tick inserts nothing");

    // The admin answers; the gate resolves.
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    store::approval::record_decision_tx(
        &mut tx,
        fixture.tenant,
        fixture.approval,
        &Decision::approve(fixture.admin),
        false,
    )
    .await
    .expect("the human decision");
    tx.commit().await.expect("commit the decision");

    tick(&db, fixture.tenant_id, "interaction.close", "close-1").await;
    wait_for("every dispatch to be notified", || async {
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM interaction_dispatch
                  WHERE tenant_id = $1 AND closed_notified_at IS NULL",
        )
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .unwrap_or(1);
        open == 0
    })
    .await;

    // No request delivery is left live: a delivered row is terminal (the recipient already saw
    // it) and a pending/sent row is expired. Each recipient got exactly one closure notice.
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM delivery d
           JOIN message m ON m.id = d.message_id
          WHERE d.tenant_id = $1 AND m.kind = 'interaction-requested'
            AND d.state IN ('pending', 'sent')",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count live");
    assert_eq!(live, 0, "no offer is left live after the answer");

    let notices: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM message WHERE tenant_id = $1 AND kind = 'interaction-closed'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count notices");
    assert_eq!(notices, 3);

    // The webhook got the request and the closure notice through its binding.
    wait_for("the webhook's two sends", || {
        let hits = fixture.stub.hits.clone();
        async move { hits.load(Ordering::SeqCst) >= 2 }
    })
    .await;

    // Deterministic supersede: an offer that is still `pending` when the answer lands is
    // expired with the resolving channel named. (The first wave above races the worker's own
    // send, so this second wave is seeded without a `delivery.send` job.)
    let second: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, meta)
         VALUES ($1, 'review-2', 'gate', 'pending', $2) RETURNING id",
    )
    .bind(tenant)
    .bind(json!({"description": "A slower review"}))
    .fetch_one(&db.super_pool)
    .await
    .expect("second approval");
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let message = store::delivery::insert_message_tx(
        &mut tx,
        fixture.tenant,
        &store::delivery::NewMessage {
            channel_kind: "web".to_owned(),
            sender_id: None,
            kind: "interaction-requested".to_owned(),
            body: "pending".to_owned(),
            meta: json!({"approval_id": second}),
            correlation_id: Some(second),
        },
    )
    .await
    .expect("manual offer");
    let recipient = fixture.member.to_string();
    let inserted = store::delivery::insert_delivery_tx(
        &mut tx,
        fixture.tenant,
        &store::delivery::NewDelivery::new(message.id, "web", &recipient),
    )
    .await
    .expect("manual delivery");
    store::interaction::insert_dispatch_tx(
        &mut tx,
        fixture.tenant,
        second,
        "web",
        &recipient,
        None,
        message.id,
    )
    .await
    .expect("manual dispatch");
    store::approval::record_decision_tx(
        &mut tx,
        fixture.tenant,
        second,
        &Decision::approve(fixture.admin),
        false,
    )
    .await
    .expect("second decision");
    tx.commit().await.expect("commit the second wave");

    tick(&db, fixture.tenant_id, "interaction.close", "close-2").await;
    wait_for("the manual offer to expire", || {
        let pool = db.super_pool.clone();
        let delivery = inserted.delivery.id;
        async move {
            let row: Option<(String, Option<String>)> =
                sqlx::query_as("SELECT state, last_error FROM delivery WHERE id = $1")
                    .bind(delivery)
                    .fetch_optional(&pool)
                    .await
                    .expect("read the manual delivery");
            row.is_some_and(|(state, error)| {
                state == "expired"
                    && error.is_some_and(|error| error.starts_with("superseded: resolved via "))
            })
        }
    })
    .await;
}
