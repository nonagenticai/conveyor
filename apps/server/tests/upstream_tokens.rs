//! S13 — the tenant-BYO upstream token path (`conveyor admin upstream-token`).
//!
//! Two shapes share one sealed row: a tenant's own token stored RAW, and a token minted through
//! the licences API stored as the envelope `{token, expires_at, source, licence_id}`. The engine
//! sends `.token` for either. This binary proves the fill writes, rotates and reads back both.

mod common;

use conveyor_auth::LicenceSyncClient;
use conveyor_domain::TenantId;
use conveyor_server::admin::upstream_tokens::{
    MINTER_NOT_CONFIGURED, TokenSource, UpstreamService, fill,
};
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

/// #744: the prover's credential is its own secret, under its own name, reported without a value.
///
/// The prover is a separate service with a separate licence (`lean4-prover`), so sealing it must
/// never touch `inference.token` or `laya.token` — one service, one secret.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lean4_token_seals_under_its_own_name() {
    init_env();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "tok-lean4").await;

    let filled = fill(
        &db.super_pool,
        None,
        "tok-lean4",
        UpstreamService::Lean4,
        TokenSource::Byo(b"byo-lean4-token".to_vec()),
    )
    .await
    .expect("seal the prover's BYO token");

    assert_eq!(filled.secret, "lean4.token");
    assert_eq!(filled.source, "byo");
    assert_eq!(filled.expires_at, None);
    // The report names the secret and its source; the value never appears in it.
    let report = format!("{filled:?}");
    assert!(!report.contains("byo-lean4-token"), "{report}");

    assert_eq!(open(&db, tenant, "lean4.token").await, b"byo-lean4-token");
    // And nothing else was written: a separate service, a separate secret.
    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM secret WHERE tenant_id = $1 ORDER BY name")
            .bind(tenant)
            .fetch_all(&db.super_pool)
            .await
            .expect("list the tenant's secrets");
    assert_eq!(names, vec!["lean4.token".to_owned()]);
}

/// A licences stub that refuses the bundle-token route, as it does for an app key it does not
/// know.
async fn refusing_licences_stub() -> String {
    let app = axum::Router::new().fallback(axum::routing::post(|| async {
        (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(json!({ "detail": "no such app" })),
        )
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the refusing licences stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

/// #744: a mint against an unknown catalogue app fails and writes nothing.
///
/// The default has to be a refusal: a silent success here would seal a token the prover will
/// only ever answer `401` to, and the operator would believe the credential works.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mint_for_an_unknown_app_fails() {
    init_env();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "tok-lean4-mint").await;
    let base = refusing_licences_stub().await;
    let client = LicenceSyncClient::new(&base).expect("licences client");

    let error = fill(
        &db.super_pool,
        Some(&client),
        "tok-lean4-mint",
        UpstreamService::Lean4,
        TokenSource::Mint,
    )
    .await
    .expect_err("a refused mint is an error, never a sealed token");

    assert!(error.contains("licences mint"), "{error}");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM secret WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the secret rows");
    assert_eq!(rows, 0, "a failed mint writes nothing");
}

/// #776: a mint with no minter credential is refused, names the variable, and writes nothing.
///
/// The refusal has to happen before the HTTP call and before the row: falling back to the
/// backend's credential would seal a token the licences service refuses at introspection, and
/// the operator would believe the prover's credential works.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mint_without_the_minter_credential_is_refused() {
    init_env();
    let db = common::TestDb::fresh().await;
    let tenant = tenant(&db, "tok-no-minter").await;

    let error = fill(
        &db.super_pool,
        None,
        "tok-no-minter",
        UpstreamService::Lean4,
        TokenSource::Mint,
    )
    .await
    .expect_err("a mint with no minter credential is refused");

    assert_eq!(error, MINTER_NOT_CONFIGURED);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM secret WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the secret rows");
    assert_eq!(rows, 0, "a refused mint writes no secret row");
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
