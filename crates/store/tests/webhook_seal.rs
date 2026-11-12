//! Migration `0014`: the webhook signing secret is sealed under the tenant KEK, not stored in the clear.
//!
//! Three properties, each a way this could go wrong silently:
//!
//! 1. **What is written is not the token.** The column is named `secret_ciphertext`; before 0014 it held the
//!    bearer token itself, and nothing failed — a leak that reads as correct.
//! 2. **A row written before the migration still delivers.** `secret_key_id IS NULL` passes the stored bytes
//!    through, so the new binary works before `conveyor admin reseal-webhooks` has run.
//! 3. **The reseal converts and is idempotent.** Running it twice must not double-seal (which would make the
//!    token unreadable on the second run).
//!
//! `cargo test -p conveyor-store --test webhook_seal -- --nocapture`.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::TenantId;
use conveyor_store::webhook::{self, NewWebhook};
use sqlx::PgPool;
use uuid::Uuid;

/// A fixed 32-byte KEK for this test binary. It never leaves this box.
const TEST_KEK: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// The sealer caches the first key it reads for the life of the process, so it is set once, before anything
/// seals. Edition 2024 made `set_var` unsafe; this binary sets it before the runtime starts any work.
fn ensure_kek() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var("CONVEYOR_SECRET_KEK").is_err() {
            // SAFETY: one process-wide value, set before the first store call; no other thread reads the
            // environment in this test binary.
            unsafe { std::env::set_var("CONVEYOR_SECRET_KEK", TEST_KEK) };
        }
    });
}

async fn tenant(db: &common::TestDb, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
            .bind(slug)
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn insert(db: &common::TestDb, tenant: TenantId, secret: &[u8]) -> Uuid {
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope");
    let record = webhook::insert_webhook_tx(
        &mut tx,
        tenant,
        &NewWebhook {
            board_id: None,
            url: "https://example.test/hook".to_owned(),
            secret: secret.to_vec(),
            events: Vec::new(),
            excluded_events: Vec::new(),
            active: true,
        },
    )
    .await
    .expect("insert a webhook");
    tx.commit().await.expect("commit");
    record.id
}

/// The stored bytes and key id, read as the superuser (the positive control).
async fn stored(pool: &PgPool, id: Uuid) -> (Option<Vec<u8>>, Option<String>) {
    sqlx::query_as(
        "SELECT secret_ciphertext, secret_key_id FROM webhook_subscription WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("read the row")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stored_secret_is_not_the_token() {
    ensure_kek();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "seal-a").await;
    let token = b"the-real-webhook-token";
    let id = insert(&db, tenant, token).await;

    let (ciphertext, key_id) = stored(&db.super_pool, id).await;
    let ciphertext = ciphertext.expect("a sealed secret");
    assert_ne!(
        ciphertext, token,
        "the column is named secret_ciphertext; the token must not be readable from the row"
    );
    // nonce (24) + ciphertext (plaintext len) + tag (16).
    assert_eq!(ciphertext.len(), 24 + token.len() + 16);
    assert_eq!(key_id.as_deref(), Some("kek:v1"));

    // And the two readers still get the plaintext back.
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope");
    let read = webhook::secret_tx(&mut tx, tenant, id)
        .await
        .expect("open the secret")
        .expect("a secret");
    tx.commit().await.expect("commit");
    assert_eq!(read, token);

    let target = webhook::delivery_target(&db.runtime, tenant, id)
        .await
        .expect("read the delivery target")
        .expect("a subscription");
    assert_eq!(target.secret.as_deref(), Some(&token[..]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pre_0014_row_still_delivers() {
    ensure_kek();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "seal-b").await;

    // A row written by the old binary: the token in the clear, no key id.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO webhook_subscription (tenant_id, url, secret_ciphertext, active)
         VALUES ($1, 'https://example.test/legacy', $2, true) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(b"legacy-token".to_vec())
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a legacy row");

    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope");
    let read = webhook::secret_tx(&mut tx, tenant, id)
        .await
        .expect("open the secret")
        .expect("a secret");
    tx.commit().await.expect("commit");
    assert_eq!(read, b"legacy-token");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reseal_converts_every_legacy_row_once() {
    ensure_kek();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "seal-c").await;

    let mut ids = Vec::new();
    for token in [b"legacy-one".as_slice(), b"legacy-two".as_slice()] {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO webhook_subscription (tenant_id, url, secret_ciphertext, active)
             VALUES ($1, $2, $3, true) RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(format!("https://example.test/{}", ids.len()))
        .bind(token.to_vec())
        .fetch_one(&db.super_pool)
        .await
        .expect("insert a legacy row");
        ids.push(id);
    }

    let report = webhook::reseal_legacy(&db.owner).await.expect("reseal");
    assert_eq!(report.resealed, 2, "{report:?}");

    for (id, token) in ids
        .iter()
        .zip([b"legacy-one".as_slice(), b"legacy-two".as_slice()])
    {
        let (ciphertext, key_id) = stored(&db.super_pool, *id).await;
        assert_eq!(key_id.as_deref(), Some("kek:v1"));
        assert_ne!(
            ciphertext.as_deref(),
            Some(token),
            "the row is still plaintext"
        );
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
            .await
            .expect("scope");
        let read = webhook::secret_tx(&mut tx, tenant, *id)
            .await
            .expect("open")
            .expect("a secret");
        tx.commit().await.expect("commit");
        assert_eq!(&read[..], token);
    }

    // Idempotent: the second run converts nothing and does not double-seal.
    let second = webhook::reseal_legacy(&db.owner)
        .await
        .expect("reseal twice");
    assert_eq!(second.resealed, 0, "{second:?}");
}
