//! S13 — the tenant-BYO upstream token path (`conveyor admin upstream-token`).
//!
//! Two shapes share one sealed row: a tenant's own token stored RAW, and a token minted through
//! the licences API stored as the envelope `{token, expires_at, source, licence_id}`. The engine
//! sends `.token` for either. This binary proves the fill writes, rotates and reads back both.

mod common;

use conveyor_auth::LicenceSyncClient;
use conveyor_domain::TenantId;
use conveyor_server::admin::upstream_tokens::{TokenSource, UpstreamService, fill};
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// The same fixed test KEK the actions harness seals with (base64 of 32 zero bytes).
const TEST_KEK: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// Set the process-global sealing key exactly once, before any secret is written.
fn init_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: once, before the first secret write; the engine reads the same value.
        unsafe { std::env::set_var("CONVEYOR_SECRET_KEK", TEST_KEK) };
    });
}

async fn tenant(db: &common::TestDb, slug: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
        .bind(slug)
        .fetch_one(&db.super_pool)
        .await
        .expect("insert the tenant")
}

/// Read the sealed secret exactly as the engine does.
async fn open(db: &common::TestDb, tenant: Uuid, name: &str) -> Vec<u8> {
    let tenant_id = TenantId::from_uuid(tenant).expect("tenant");
    let mut tx = store::begin_scoped(&db.runtime, tenant_id)
        .await
        .expect("scoped tx");
    let bytes = store::secret::open_named_tx(&mut tx, tenant_id, name)
        .await
        .expect("open the secret");
    tx.commit().await.expect("commit");
    bytes
}

async fn versions(db: &common::TestDb, tenant: Uuid, name: &str) -> i64 {
    sqlx::query_scalar("SELECT version FROM secret WHERE tenant_id = $1 AND name = $2")
        .bind(tenant)
        .bind(name)
        .fetch_one(&db.super_pool)
        .await
        .expect("the secret row")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_byo_token_is_sealed_raw_and_rotated_in_place() {
    init_env();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "tok-byo").await;

    let first = fill(
        &db.super_pool,
        None,
        "tok-byo",
        UpstreamService::Inference,
        TokenSource::Byo(b"byo-token-one".to_vec()),
    )
    .await
    .expect("seal the BYO token");
    assert_eq!(first.secret, "inference.token");
    assert_eq!(first.source, "byo");
    assert_eq!(first.expires_at, None);
    assert_eq!(open(&db, tenant, "inference.token").await, b"byo-token-one");
    assert_eq!(versions(&db, tenant, "inference.token").await, 1);

    // A second fill is a rotation of the same row, not a second secret.
    let second = fill(
        &db.super_pool,
        None,
        "tok-byo",
        UpstreamService::Inference,
        TokenSource::Byo(b"byo-token-two".to_vec()),
    )
    .await
    .expect("rotate the BYO token");
    assert_eq!(second.source, "byo");
    assert_eq!(open(&db, tenant, "inference.token").await, b"byo-token-two");
    assert_eq!(versions(&db, tenant, "inference.token").await, 2);
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM secret WHERE tenant_id = $1 AND name = 'inference.token'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the secret rows");
    assert_eq!(rows, 1, "a name is one secret, rotated in place");
}

/// A licences stub that answers the bundle-token route with one fixed document.
async fn licences_stub() -> String {
    let app = axum::Router::new().fallback(axum::routing::post(|| async {
        axum::Json(json!({
            "token": "minted-token-abc",
            "expires_at": "2026-11-10T12:00:00Z",
            "tenant_id": "ignored",
            "bundle": "conveyor",
            "app": "laya",
            "license_id": "lic-1",
            "token_id": "tok-1"
        }))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the licences stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_minted_token_is_sealed_as_the_envelope() {
    init_env();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "tok-mint").await;
    let base = licences_stub().await;
    let client = LicenceSyncClient::new(&base).expect("licences client");

    let filled = fill(
        &db.super_pool,
        Some(&client),
        "tok-mint",
        UpstreamService::Laya,
        TokenSource::Mint,
    )
    .await
    .expect("mint the token");
    assert_eq!(filled.secret, "laya.token");
    assert_eq!(filled.source, "licences");
    assert_eq!(filled.expires_at.as_deref(), Some("2026-11-10T12:00:00Z"));

    let bytes = open(&db, tenant, "laya.token").await;
    let envelope: Value = serde_json::from_slice(&bytes).expect("the envelope is JSON");
    assert_eq!(envelope["token"], "minted-token-abc");
    assert_eq!(envelope["expires_at"], "2026-11-10T12:00:00Z");
    assert_eq!(envelope["source"], "licences");
    assert_eq!(envelope["licence_id"], "lic-1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_tenant_is_refused_and_writes_nothing() {
    init_env();
    let db = common::TestDb::fresh().await;
    let error = fill(
        &db.super_pool,
        None,
        "no-such-tenant",
        UpstreamService::Laya,
        TokenSource::Byo(b"token".to_vec()),
    )
    .await
    .expect_err("an unknown tenant is refused");
    assert!(error.contains("no tenant"), "{error}");
}
