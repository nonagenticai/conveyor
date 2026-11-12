//! RLS and store behaviour for the six S06 comms tables.
//!
//! `cargo test -p conveyor-store --test channel -- --nocapture`.
//!
//! Four assertions (S06 §5.1/§7, CONTRACT §2):
//! 1. as `conveyor_runtime` scoped to A, only A's row is visible in all six tables; the
//!    superuser control sees both tenants' rows, and the schema owner, with no scope set,
//!    sees none — that is what FORCE buys;
//! 2. a replayed fan-out hits `(message_id, channel_kind, recipient_ref)` and returns the
//!    existing row instead of a second one;
//! 3. a cross-tenant message id is refused by the tenant-scoped resolver — the single-column
//!    FK alone would accept it, which is proved first, so the test cannot pass vacuously;
//! 4. bindings upsert on their natural key and a preference upserts per (user, channel kind).
//!
//! Fixtures are inserted as the SUPERUSER with explicit `tenant_id`s: the superuser is the
//! positive control. Everything under test runs as `conveyor_runtime` in a scoped
//! transaction.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{DeliveryState, TenantId};
use conveyor_store::begin_scoped;
use conveyor_store::channel::{
    NewChannelBinding, delete_binding_tx, get_binding_tx, get_preference_tx, list_bindings_tx,
    upsert_binding_tx, upsert_preference_tx,
};
use conveyor_store::delivery::{
    DELIVERY_MAX_ATTEMPTS, DeliveryStoreError, NewDelivery, NewMessage, insert_delivery_tx,
    insert_message_tx, mark_delivered_tx, mark_expired_tx, mark_failed_tx, mark_sent_tx,
    pending_deliveries_tx, recipient_deliveries_tx,
};
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use support::TestDb;
use uuid::Uuid;

/// The four S06 tables that survive the S00 verdicts, alphabetically. The isolation assertion walks this
/// list. ⚠️ 2026-11-08: `audience`/`audience_member` were dropped by migration 0011 (measured unused).
const COMMS_TABLES: [&str; 4] = [
    "channel_binding",
    "channel_preference",
    "delivery",
    "message",
];

async fn count(pool: &PgPool, sql: String) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .expect("count")
}

async fn count_tx(conn: &mut PgConnection, sql: String) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(&mut *conn)
        .await
        .expect("count")
}

/// A tenant and one `app_user`, as the superuser.
async fn seed_tenant_user(pool: &PgPool, slug: &str) -> (Uuid, Uuid) {
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
            .bind(slug)
            .bind(format!("Tenant {slug}"))
            .fetch_one(pool)
            .await
            .expect("seed tenant");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("user-{slug}"))
            .fetch_one(pool)
            .await
            .expect("seed app_user");
    (tenant, user)
}

/// One row in each of the six comms tables, as the superuser, with explicit tenant ids.
async fn seed_all_four(pool: &PgPool, slug: &str) -> (Uuid, Uuid, Uuid) {
    let (tenant, user) = seed_tenant_user(pool, slug).await;

    sqlx::query(
        "INSERT INTO channel_binding (tenant_id, channel_kind, route_key, config)
         VALUES ($1, 'telegram', $2, '{\"chat_id\": -100}'::jsonb)",
    )
    .bind(tenant)
    .bind(format!("route-{slug}"))
    .execute(pool)
    .await
    .expect("seed channel_binding");

    sqlx::query(
        "INSERT INTO channel_preference (tenant_id, user_id, channel_kind, quiet_hours)
         VALUES ($1, $2, 'telegram', '{\"from\":\"22:00\"}'::jsonb)",
    )
    .bind(tenant)
    .bind(user)
    .execute(pool)
    .await
    .expect("seed channel_preference");

    let message: Uuid = sqlx::query_scalar(
        "INSERT INTO message (tenant_id, channel_kind, sender_id, body)
         VALUES ($1, 'telegram', $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(user)
    .bind(format!("message {slug}"))
    .fetch_one(pool)
    .await
    .expect("seed message");

    sqlx::query(
        "INSERT INTO delivery (tenant_id, message_id, channel_kind, recipient_ref)
         VALUES ($1, $2, 'telegram', $3)",
    )
    .bind(tenant)
    .bind(message)
    .bind(format!("chat:{slug}"))
    .execute(pool)
    .await
    .expect("seed delivery");

    (tenant, user, message)
}

#[tokio::test]
async fn the_four_comms_tables_isolate_runtime_owner_and_superuser() {
    let db = TestDb::fresh().await;
    let (tenant_a, _, _) = seed_all_four(&db.super_pool, "comms-a").await;
    let (tenant_b, _, _) = seed_all_four(&db.super_pool, "comms-b").await;
    let a = TenantId::from_uuid(tenant_a).expect("non-nil");

    // ── the superuser control: both tenants' rows exist in every table ──────────────────
    for table in COMMS_TABLES {
        let n = count(&db.super_pool, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(n, 2, "superuser must see both {table} rows, saw {n}");
    }

    // ── the schema owner with no scope: FORCE RLS shows nothing ─────────────────────────
    for table in COMMS_TABLES {
        let n = count(&db.owner, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            n, 0,
            "the owner must see 0 rows of {table} without a scope (FORCE); saw {n}"
        );
    }

    // ── runtime scoped to A: exactly A's row, never B's ─────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, a)
        .await
        .expect("scope a transaction to A");
    for table in COMMS_TABLES {
        let total = count_tx(&mut tx, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            total, 1,
            "runtime scoped to A must see exactly one {table} row"
        );
        let foreign: i64 = {
            let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1");
            sqlx::query_scalar(AssertSqlSafe(sql))
                .bind(tenant_b)
                .fetch_one(&mut *tx)
                .await
                .expect("count B")
        };
        assert_eq!(foreign, 0, "B's {table} rows must be invisible to A");
    }
    tx.rollback().await.expect("roll back the read scope");
}

#[tokio::test]
async fn a_replayed_fan_out_hits_the_unique_key_and_the_attempt_budget_lands_failed() {
    let db = TestDb::fresh().await;
    let (tenant, _user) = seed_tenant_user(&db.super_pool, "fan-a").await;
    let a = TenantId::from_uuid(tenant).expect("non-nil");

    let mut tx = begin_scoped(&db.runtime, a)
        .await
        .expect("scope a transaction to A");

    let message = insert_message_tx(&mut tx, a, &NewMessage::new("web", "hello"))
        .await
        .expect("insert message");
    assert_eq!(message.kind, "text");

    let first = insert_delivery_tx(
        &mut tx,
        a,
        &NewDelivery::new(message.id, "web", "user:alice"),
    )
    .await
    .expect("first delivery");
    assert!(!first.existed, "the first insert is not a replay");
    assert_eq!(first.delivery.state, DeliveryState::Pending);
    assert_eq!(first.delivery.attempts, 0);

    // ── the unique key: the same (message, channel, recipient) is ONE row ───────────────
    let replay = insert_delivery_tx(
        &mut tx,
        a,
        &NewDelivery::new(message.id, "web", "user:alice"),
    )
    .await
    .expect("a replayed fan-out is an idempotent result, not an error");
    assert!(replay.existed, "the replay must report the existing row");
    assert_eq!(replay.delivery.id, first.delivery.id);
    assert_eq!(
        replay.delivery.version, first.delivery.version,
        "a replay writes nothing"
    );

    let second = insert_delivery_tx(&mut tx, a, &NewDelivery::new(message.id, "web", "user:bob"))
        .await
        .expect("a second recipient is a second row");
    assert!(!second.existed);
    assert_ne!(second.delivery.id, first.delivery.id);

    // A different channel for the same recipient is a third key, not a conflict.
    let other_channel = insert_delivery_tx(
        &mut tx,
        a,
        &NewDelivery::new(message.id, "telegram", "user:alice"),
    )
    .await
    .expect("channel kind is part of the key");
    assert!(!other_channel.existed);

    // ── the drainer path: claim, sent, delivered, and a replayed acknowledgement ───────
    let claimed = pending_deliveries_tx(&mut tx, a, 10).await.expect("claim");
    assert_eq!(claimed.len(), 3, "all three live rows are claimable");

    let sent = mark_sent_tx(&mut tx, a, first.delivery.id)
        .await
        .expect("mark sent")
        .expect("the row is live");
    assert_eq!(sent.state, DeliveryState::Sent);
    assert!(
        sent.sent_at.is_some(),
        "sent_at is stamped by the database clock"
    );

    let delivered = mark_delivered_tx(&mut tx, a, first.delivery.id)
        .await
        .expect("mark delivered")
        .expect("the row is live");
    assert_eq!(delivered.state, DeliveryState::Delivered);
    assert!(delivered.delivered_at.is_some());

    let replay_ack = mark_delivered_tx(&mut tx, a, first.delivery.id)
        .await
        .expect("a replayed ack is a no-op")
        .expect("the row exists");
    assert_eq!(replay_ack.state, DeliveryState::Delivered);
    assert_eq!(
        replay_ack.version, delivered.version,
        "replaying the same acknowledgement must not write"
    );

    // ── the budget: attempts 1..4 stay live, attempt 5 lands failed ────────────────────
    for attempt in 1..DELIVERY_MAX_ATTEMPTS {
        let row = mark_failed_tx(
            &mut tx,
            a,
            second.delivery.id,
            "stub returned 500",
            DELIVERY_MAX_ATTEMPTS,
        )
        .await
        .expect("record a failed attempt")
        .expect("the row exists");
        assert_eq!(row.attempts, attempt);
        assert_eq!(
            row.state,
            DeliveryState::Pending,
            "below the budget the row stays live"
        );
        assert_eq!(row.last_error.as_deref(), Some("stub returned 500"));
    }
    let failed = mark_failed_tx(
        &mut tx,
        a,
        second.delivery.id,
        "stub returned 500",
        DELIVERY_MAX_ATTEMPTS,
    )
    .await
    .expect("the last attempt")
    .expect("the row exists");
    assert_eq!(failed.attempts, DELIVERY_MAX_ATTEMPTS);
    assert_eq!(failed.state, DeliveryState::Failed);
    let failed_replay = mark_failed_tx(
        &mut tx,
        a,
        second.delivery.id,
        "another failure",
        DELIVERY_MAX_ATTEMPTS,
    )
    .await
    .expect("a replay on a terminal row is a no-op")
    .expect("the row exists");
    assert_eq!(failed_replay.attempts, DELIVERY_MAX_ATTEMPTS);
    assert_eq!(
        failed_replay.last_error.as_deref(),
        Some("stub returned 500"),
        "a terminal replay does not rewrite the recorded error"
    );

    // ── a delivery closed before it was ever handed to a channel ───────────────────────
    let expiring = mark_expired_tx(&mut tx, a, other_channel.delivery.id)
        .await
        .expect("expire a pending delivery")
        .expect("the row is live");
    assert_eq!(expiring.state, DeliveryState::Expired);

    let inbox = recipient_deliveries_tx(&mut tx, a, "user:alice", 10)
        .await
        .expect("inbox");
    assert_eq!(
        inbox.len(),
        2,
        "both of alice's deliveries are in the inbox"
    );
    assert!(inbox.iter().any(|row| row.body == "hello"));

    tx.commit().await.expect("commit the drainer transaction");

    // Exactly three delivery rows survived: the two recipients' plus the second channel.
    let deliveries = count(&db.super_pool, "SELECT count(*) FROM delivery".to_owned()).await;
    assert_eq!(deliveries, 3, "a replay created no fourth row");
}

#[tokio::test]
async fn a_cross_tenant_message_id_is_refused_by_the_resolver() {
    let db = TestDb::fresh().await;
    let (tenant_a, _user_a) = seed_tenant_user(&db.super_pool, "cross-a").await;
    let (tenant_b, _user_b) = seed_tenant_user(&db.super_pool, "cross-b").await;
    let a = TenantId::from_uuid(tenant_a).expect("non-nil");

    let message_b: Uuid = sqlx::query_scalar(
        "INSERT INTO message (tenant_id, channel_kind, body) VALUES ($1, 'web', 'b only')
         RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed B's message");

    // The premise, proved rather than asserted: the single-column FK accepts a delivery in
    // tenant A that names tenant B's message (referential checks bypass RLS). This is the
    // exact hole the resolver exists to close (CONTRACT §2).
    let mut super_tx = db.super_pool.begin().await.expect("superuser transaction");
    sqlx::query(
        "INSERT INTO delivery (tenant_id, message_id, channel_kind, recipient_ref)
         VALUES ($1, $2, 'web', 'user:alice')",
    )
    .bind(tenant_a)
    .bind(message_b)
    .execute(&mut *super_tx)
    .await
    .expect("the FK alone does NOT enforce tenancy — the resolver must");
    super_tx.rollback().await.expect("roll the proof back");

    let mut tx = begin_scoped(&db.runtime, a)
        .await
        .expect("scope a transaction to A");
    let error = insert_delivery_tx(
        &mut tx,
        a,
        &NewDelivery::new(message_b, "web", "user:alice"),
    )
    .await
    .expect_err("another tenant's message must not be deliverable");
    assert!(
        matches!(error, DeliveryStoreError::MessageNotFound),
        "the resolver, not the FK, must refuse: {error:?}"
    );
    tx.rollback().await.expect("roll back the refused write");

    let deliveries = count(&db.super_pool, "SELECT count(*) FROM delivery".to_owned()).await;
    assert_eq!(deliveries, 0, "the refusal wrote nothing");
}

#[tokio::test]
async fn bindings_upsert_on_their_key_and_preferences_are_per_user() {
    let db = TestDb::fresh().await;
    let (tenant_a, user_a) = seed_tenant_user(&db.super_pool, "bind-a").await;
    let (tenant_b, _user_b) = seed_tenant_user(&db.super_pool, "bind-b").await;
    let a = TenantId::from_uuid(tenant_a).expect("non-nil");

    let mut tx = begin_scoped(&db.runtime, a).await.expect("scope to A");

    let mut input = NewChannelBinding::new("telegram", "default");
    let first = upsert_binding_tx(&mut tx, a, &input).await.expect("bind");
    assert_eq!(first.version, 1);
    assert!(first.active);

    input.config = serde_json::json!({"chat_id": -100, "topic_id": 7});
    let second = upsert_binding_tx(&mut tx, a, &input).await.expect("rebind");
    assert_eq!(second.id, first.id, "the natural key names one row");
    assert_eq!(second.version, 2);
    assert_eq!(
        second.config,
        serde_json::json!({"chat_id": -100, "topic_id": 7})
    );

    let listed = list_bindings_tx(&mut tx, a).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, first.id);
    let read = get_binding_tx(&mut tx, a, first.id)
        .await
        .expect("get")
        .expect("the binding exists");
    assert_eq!(read.config, second.config);

    let pref = upsert_preference_tx(&mut tx, a, user_a, "telegram", None, None)
        .await
        .expect("first preference");
    assert!(pref.enabled);
    assert_eq!(pref.quiet_hours, serde_json::json!({}));
    assert_eq!(pref.version, 1);

    let quiet = serde_json::json!({"from": "22:00", "to": "07:00"});
    let updated = upsert_preference_tx(&mut tx, a, user_a, "telegram", Some(false), Some(&quiet))
        .await
        .expect("update preference");
    assert_eq!(updated.id, pref.id, "one row per (user, channel kind)");
    assert!(!updated.enabled);
    assert_eq!(updated.quiet_hours, quiet);
    assert_eq!(updated.version, 2);
    let read_back = get_preference_tx(&mut tx, a, user_a, "telegram")
        .await
        .expect("get preference")
        .expect("the preference exists");
    assert_eq!(read_back.id, pref.id);
    assert!(!read_back.enabled);

    assert!(
        delete_binding_tx(&mut tx, a, "telegram", "default")
            .await
            .expect("unbind")
    );
    assert!(
        !delete_binding_tx(&mut tx, a, "telegram", "default")
            .await
            .expect("a replayed unbind is a no-op, not a 404")
    );
    tx.commit().await.expect("commit");

    // B sees none of A's configuration, and A's own reads see the deleted binding gone.
    let b = TenantId::from_uuid(tenant_b).expect("non-nil");
    let mut tx_b = begin_scoped(&db.runtime, b).await.expect("scope to B");
    assert_eq!(
        list_bindings_tx(&mut tx_b, b).await.expect("list B").len(),
        0
    );
    assert!(
        get_preference_tx(&mut tx_b, b, user_a, "telegram")
            .await
            .expect("get B")
            .is_none(),
        "a preference is tenant-scoped, not global to the user"
    );
    tx_b.rollback().await.expect("roll back B");

    let mut tx_a = begin_scoped(&db.runtime, a).await.expect("re-scope to A");
    assert_eq!(
        list_bindings_tx(&mut tx_a, a).await.expect("list A").len(),
        0
    );
    tx_a.rollback().await.expect("roll back A");
}
