//! Behaviour of `POST /internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure`.
//!
//! Every case runs against a real Postgres 18 created by `common` and a stub billing server, the
//! same harness `tests/bundle_ensure_apply.rs` uses. The route is mounted on its own router with
//! an injected `Extension<User>`: these tests exercise the ensure's authorization, validation,
//! orchestration and response mapping, while the auth middleware has its own battery.
//!
//! What is proved here: the insert/adopt/dependency decisions and their response shape, a second
//! call writing nothing, a dry run writing nothing and nulling an insert's `license_id`, a
//! per-app refusal as a 409 carrying the normal body, and every whole-request refusal — archived
//! tenant, held tenant lock, money tenant, role/owner/ensurer, foreign tenant, unknown bundle —
//! plus FastAPI's body validation.

mod common;

use std::sync::{Arc, Mutex, Once};

use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::post as route_post;
use axum::{Extension, Json, Router};
use conveyor_auth::oauth::ClientCredentials;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::db::bundle_ensure_apply::TenantEnsureLock;
use conveyor_licenses::router::internal_ensure::internal_ensure_tenant_bundle;
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const KEY: &str = "non-agentic-ai";
const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0001);
const TENANT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0002);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0003);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0004);
const DEP_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0005);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0006);
const DEP_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0007);
const SVC: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0008);
const SVC2: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0009);
const LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_000a);
const LICENCE2: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_000b);

// ---------------------------------------------------------------------------------------------
// The billing stub: records every push and answers 200.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Call {
    path: String,
}

#[derive(Clone, Default)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
}

impl Stub {
    fn new() -> Self {
        Self::default()
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }

    fn calls_at(&self, path: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.path == path)
            .collect()
    }
}

fn record(stub: &Stub, path: &str) {
    stub.calls.lock().expect("stub lock").push(Call {
        path: path.to_owned(),
    });
}

async fn budget_push(AxumState(stub): AxumState<Stub>, _headers: HeaderMap) -> StatusCode {
    record(&stub, "/api/budget");
    StatusCode::OK
}

async fn quota_push(AxumState(stub): AxumState<Stub>, _headers: HeaderMap) -> StatusCode {
    record(&stub, "/api/quota/config");
    StatusCode::OK
}

async fn token() -> Json<Value> {
    Json(json!({"access_token": "stub-token", "token_type": "Bearer", "expires_in": 3600}))
}

async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/budget", route_post(budget_push))
        .route("/api/quota/config", route_post(quota_push))
        .route("/token", route_post(token))
        .with_state(stub);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let address = listener.local_addr().expect("stub address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}")
}

fn provisioner(base: &str) -> MeteringProvisioner {
    let creds = ClientCredentials::new(&format!("{base}/token"), "licenses", "secret")
        .expect("client credentials");
    MeteringProvisioner::new(base, Some(creds)).expect("metering provisioner")
}

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

/// `ENSURE_LOCK_TIMEOUT=1s` for this test binary, written exactly once BEFORE any test reads it.
///
/// Edition 2024 made `set_var` unsafe because another thread may read the environment. Every test
/// calls this first, and `Once` gives the happens-before edge (the same pattern as
/// `tests/bundle_ensure_apply.rs`).
fn short_lock_timeout() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        std::env::set_var("ENSURE_LOCK_TIMEOUT", "1s");
    });
}

fn state(pool: PgPool, meter: MeteringProvisioner) -> AppState {
    AppState::new(
        pool,
        Config {
            port: 0,
            environment: "test".to_owned(),
            version: "0.0.0".to_owned(),
            trusted_issuers: Vec::new(),
            introspect_require_auth: false,
            service_actor_id: None,
        },
        JwtVerifier::new(Vec::new(), Arc::new(UnavailableJwks)),
    )
    .with_metering(Arc::new(meter))
}

fn user_with(roles: &[&str], owner: Option<&str>, cross_owner: bool) -> User {
    User {
        sub: "0195f48b-d000-7000-8000-0000000000aa".to_owned(),
        preferred_username: "ensurer".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: Some(TENANT.to_string()),
        owner_key: owner.map(str::to_owned),
        cross_owner,
        ..User::default()
    }
}

fn ensurer() -> User {
    user_with(&["bundle-provisioner"], Some("owner-a"), false)
}

/// The route-level app: no auth middleware, the caller is the injected `Extension<User>`.
fn app(db: &common::TestDb, user: User, meter: MeteringProvisioner) -> Router {
    Router::new()
        .route(
            "/internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure",
            route_post(internal_ensure_tenant_bundle),
        )
        .layer(Extension(user))
        .with_state(state(db.pool.clone(), meter))
}

async fn ensure(app: &Router, tenant: Uuid, bundle: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::post(format!(
        "/internal/tenants/{tenant}/bundles/{bundle}/ensure"
    ))
    .header("content-type", "application/json")
    .body(Body::from(body.to_string()))
    .expect("request");
    let response = app.clone().oneshot(request).await.expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

/// owner-a owns TENANT; owner-b owns TENANT_B. The bundle sells `demo` and is ensurable by
/// owner-a only. `workers` exists as a possible dependency.
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    sqlx::query(
        "INSERT INTO tenant_owner (key, display_name)
         VALUES ('owner-a', 'Owner A'), ('owner-b', 'Owner B')",
    )
    .execute(&db.pool)
    .await
    .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key)
         VALUES ($1, 'tenant-a', now(), 'owner-a'), ($2, 'tenant-b', now(), 'owner-b')",
    )
    .bind(TENANT)
    .bind(TENANT_B)
    .execute(&db.pool)
    .await
    .expect("tenants");
    sqlx::query(
        "INSERT INTO app (id, key, name, created_at)
         VALUES ($1, 'demo', 'Demo', now()), ($2, 'workers', 'Workers', now())",
    )
    .bind(APP)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("apps");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, token_budget_monthly, quota_monthly, created_at)
         VALUES ($1, $2, 'basic', 'Basic', 1000, 100, now()),
                ($3, $4, 'basic', 'Basic', 500, 50, now())",
    )
    .bind(PLAN)
    .bind(APP)
    .bind(DEP_PLAN)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("plans");
    sqlx::query("INSERT INTO bundle (id, key, name, created_at) VALUES ($1, $2, $2, now())")
        .bind(BUNDLE)
        .bind(KEY)
        .execute(&db.pool)
        .await
        .expect("bundle");
    sqlx::query("INSERT INTO bundle_app (bundle_id, app_id, created_at) VALUES ($1, $2, now())")
        .bind(BUNDLE)
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("bundle_app");
    sqlx::query("INSERT INTO bundle_ensurer (bundle_id, owner_key) VALUES ($1, 'owner-a')")
        .bind(BUNDLE)
        .execute(&db.pool)
        .await
        .expect("bundle_ensurer");
    db
}

async fn requires(db: &common::TestDb, app: Uuid, required: Uuid, caller_owner: &str) {
    sqlx::query(
        "INSERT INTO app_requires (app_id, required_app_id, caller_owner_key) VALUES ($1, $2, $3)",
    )
    .bind(app)
    .bind(required)
    .bind(caller_owner)
    .execute(&db.pool)
    .await
    .expect("app_requires");
}

async fn subject_row(db: &common::TestDb, id: Uuid, keycloak_sub: &str) {
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'service', now())",
    )
    .bind(id)
    .bind(keycloak_sub)
    .execute(&db.pool)
    .await
    .expect("subject");
}

async fn licence_row(db: &common::TestDb, id: Uuid, subject: Uuid, bundle: Option<Uuid>) {
    sqlx::query(
        "INSERT INTO license (
             id, subject_id, app_id, plan_id, status, starts_at, tenant_id, bundle_id,
             metering_provisioned_at)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, $6, now())",
    )
    .bind(id)
    .bind(subject)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .bind(bundle)
    .execute(&db.pool)
    .await
    .expect("license");
}

/// `(licences, subjects, tenant_bundle, tenant_app_grant, tenant_dependency_grant)`.
async fn counts(db: &common::TestDb) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM license),
                (SELECT count(*) FROM subject),
                (SELECT count(*) FROM tenant_bundle),
                (SELECT count(*) FROM tenant_app_grant),
                (SELECT count(*) FROM tenant_dependency_grant)",
    )
    .fetch_one(&db.pool)
    .await
    .expect("counts")
}

/// `(bundle_id, is_dependency, metered)` of one licence.
async fn licence_facts(db: &common::TestDb, id: Uuid) -> (Option<Uuid>, bool, bool) {
    sqlx::query_as(
        "SELECT bundle_id, is_dependency, metering_provisioned_at IS NOT NULL
           FROM license WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .expect("licence facts")
}

async fn roomy(db: &common::TestDb) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&db.url)
        .await
        .expect("roomy pool")
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_first_ensure_inserts_and_answers_the_reference_shape() {
    short_lock_timeout();
    let db = seeded().await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);
    let app = app(&db, ensurer(), meter);

    let (status, body) = ensure(&app, TENANT, KEY, json!({"plans": ["basic"]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let license_id = body["apps"]["demo"]["license_id"]
        .as_str()
        .unwrap_or_else(|| panic!("insert licence id: {body}"))
        .to_owned();

    // The exact wire shape: every required key present, nulls explicit, no extras.
    assert_eq!(
        body,
        json!({
            "tenant_id": TENANT.to_string(),
            "bundle": KEY,
            "budget_currency": null,
            "apps": {
                "demo": {
                    "decision": "insert",
                    "license_id": license_id,
                    "plan": "basic",
                    "metering": "token_and_quota",
                    "dependency": false,
                }
            },
            "refused": [],
        })
    );

    // The hold, the service subject and a metered licence carrying the bundle.
    assert_eq!(counts(&db).await.2, 1, "tenant_bundle row");
    let licence_uuid = Uuid::try_parse(&license_id).expect("licence uuid");
    let (bundle, dependency, metered) = licence_facts(&db, licence_uuid).await;
    assert_eq!(bundle, Some(BUNDLE));
    assert!(!dependency);
    assert!(metered, "the accepted pushes stamp the licence");
    assert_eq!(stub.calls_at("/api/quota/config").len(), 1);
    assert_eq!(stub.calls_at("/api/budget").len(), 1);
}

#[tokio::test]
async fn a_second_ensure_is_all_exists_and_writes_nothing() {
    short_lock_timeout();
    let db = seeded().await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let (status, first) = ensure(
        &app(&db, ensurer(), meter.clone()),
        TENANT,
        KEY,
        json!({"plans": ["basic"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let license_id = first["apps"]["demo"]["license_id"].clone();
    let before = counts(&db).await;
    let calls_before = stub.calls().len();

    // The default plan preference (`["standard"]`) does not matter: the app already exists.
    let (status, again) = ensure(&app(&db, ensurer(), meter), TENANT, KEY, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["apps"]["demo"]["decision"], json!("exists"));
    assert_eq!(again["apps"]["demo"]["license_id"], license_id);
    assert_eq!(again["apps"]["demo"]["plan"], json!("basic"));
    assert_eq!(again["apps"]["demo"]["metering"], json!("token_and_quota"));
    assert_eq!(again["apps"]["demo"]["dependency"], json!(false));
    assert_eq!(again["refused"], json!([]));
    assert_eq!(counts(&db).await, before, "nothing new may be written");
    assert_eq!(stub.calls().len(), calls_before, "no second metering push");
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_nulls_the_insert_licence() {
    short_lock_timeout();
    let db = seeded().await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);
    let before = counts(&db).await;

    let (status, body) = ensure(
        &app(&db, ensurer(), meter),
        TENANT,
        KEY,
        json!({"plans": ["basic"], "dry_run": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["apps"]["demo"]["decision"], json!("insert"));
    assert_eq!(body["apps"]["demo"]["license_id"], Value::Null);
    assert_eq!(body["apps"]["demo"]["plan"], json!("basic"));
    assert_eq!(counts(&db).await, before, "a dry run writes nothing");
    assert!(stub.calls().is_empty(), "a dry run pushes no metering");
}

#[tokio::test]
async fn the_adopt_path_adopts_the_sole_unbundled_licence() {
    short_lock_timeout();
    let db = seeded().await;
    subject_row(&db, SVC, &format!("svc:{TENANT}:demo")).await;
    licence_row(&db, LICENCE, SVC, None).await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let (status, body) = ensure(&app(&db, ensurer(), meter), TENANT, KEY, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["apps"]["demo"]["decision"], json!("adopt"));
    assert_eq!(
        body["apps"]["demo"]["license_id"],
        json!(LICENCE.to_string())
    );
    assert_eq!(body["apps"]["demo"]["plan"], json!("basic"));
    assert_eq!(
        licence_facts(&db, LICENCE).await.0,
        Some(BUNDLE),
        "the sole unbundled licence is adopted"
    );
    assert_eq!(counts(&db).await.0, 1, "an adopt mints no second licence");
    assert!(stub.calls().is_empty(), "an adopt pushes no metering");
}

#[tokio::test]
async fn the_dependency_path_inserts_the_shared_dependency() {
    short_lock_timeout();
    let db = seeded().await;
    requires(&db, APP, DEP_APP, "owner-b").await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let (status, body) = ensure(
        &app(&db, ensurer(), meter),
        TENANT,
        KEY,
        json!({"plans": ["basic"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["apps"]["demo"]["decision"], json!("insert"));
    assert_eq!(body["apps"]["workers"]["decision"], json!("insert"));
    assert_eq!(body["apps"]["workers"]["plan"], json!("basic"));
    assert_eq!(body["apps"]["workers"]["dependency"], json!(true));
    let dependency_id = body["apps"]["workers"]["license_id"]
        .as_str()
        .expect("dependency licence id");
    let (bundle, dependency, metered) =
        licence_facts(&db, Uuid::try_parse(dependency_id).expect("uuid")).await;
    assert_eq!(
        bundle, None,
        "a dependency licence is shared: bundle_id NULL"
    );
    assert!(dependency);
    assert!(metered);
    // Both inserts owe metering.
    assert_eq!(stub.calls_at("/api/quota/config").len(), 2);
}

#[tokio::test]
async fn a_refusing_app_answers_409_with_the_same_body() {
    short_lock_timeout();
    let db = seeded().await;
    // Two tenant-level candidates for the same app: REFUSE `several_candidates`.
    subject_row(&db, SVC, &format!("svc:{TENANT}:demo")).await;
    subject_row(&db, SVC2, &format!("svc:{TENANT}:demo-two")).await;
    licence_row(&db, LICENCE, SVC, None).await;
    licence_row(&db, LICENCE2, SVC2, None).await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let (status, body) = ensure(&app(&db, ensurer(), meter), TENANT, KEY, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    // The 409 carries the NORMAL response body — not the `{"detail": …}` error envelope.
    assert!(body.get("detail").is_none(), "{body}");
    assert_eq!(body["tenant_id"], json!(TENANT.to_string()));
    assert_eq!(body["bundle"], json!(KEY));
    assert_eq!(body["budget_currency"], Value::Null);
    assert_eq!(body["apps"], json!({}));
    assert_eq!(
        body["refused"],
        json!([{"app": "demo", "reason": "several_candidates"}])
    );
}

#[tokio::test]
async fn an_archived_tenant_is_409_with_nothing_written() {
    short_lock_timeout();
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");
    let before = counts(&db).await;

    let (status, body) = ensure(
        &app(&db, ensurer(), provisioner(&start_stub(Stub::new()).await)),
        TENANT,
        KEY,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], json!("tenant_archived"));
    assert_eq!(body["detail"]["tenant_id"], json!(TENANT.to_string()));
    assert_eq!(body["detail"]["bundle"], json!(KEY));
    assert_eq!(counts(&db).await, before, "nothing may be written");
}

#[tokio::test]
async fn a_held_tenant_lock_is_409_ensure_in_progress() {
    short_lock_timeout();
    let db = seeded().await;
    let pool = roomy(&db).await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let lock = TenantEnsureLock::acquire(&pool, TENANT)
        .await
        .expect("acquire the tenant lock");
    let before = counts(&db).await;
    let (status, body) = ensure(&app(&db, ensurer(), meter), TENANT, KEY, json!({})).await;
    lock.release().await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], json!("ensure_in_progress"));
    assert!(
        body["detail"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("still running")),
        "{body}"
    );
    assert_eq!(counts(&db).await, before, "nothing may be written");
}

#[tokio::test]
async fn a_money_tenant_with_a_token_override_is_422() {
    short_lock_timeout();
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET budget_currency = 'EUR' WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("budget currency");
    let before = counts(&db).await;

    let (status, body) = ensure(
        &app(&db, ensurer(), provisioner(&start_stub(Stub::new()).await)),
        TENANT,
        KEY,
        json!({"token_budget_override": 5}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["detail"]["error"],
        json!("token_budget_for_money_tenant")
    );
    assert_eq!(counts(&db).await, before, "nothing may be written");
}

#[tokio::test]
async fn role_owner_and_ensurer_refusals_are_403() {
    short_lock_timeout();
    let db = seeded().await;
    let meter = provisioner(&start_stub(Stub::new()).await);
    let before = counts(&db).await;

    // A missing role (read-only) is the dependency's 403, before anything else.
    let (status, body) = ensure(
        &app(
            &db,
            user_with(&["license-reader"], Some("owner-a"), false),
            meter.clone(),
        ),
        TENANT,
        KEY,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("One of roles required")),
        "{body}"
    );

    // A known role but no `api_client` row: 403 `unknown_api_client`.
    let (status, body) = ensure(
        &app(
            &db,
            user_with(&["bundle-provisioner"], None, false),
            meter.clone(),
        ),
        TENANT,
        KEY,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], json!("unknown_api_client"));

    // The tenant OWNER but not a `bundle_ensurer`: 403 `bundle_not_ensurable_by_caller`.
    let (status, body) = ensure(
        &app(
            &db,
            user_with(&["bundle-provisioner"], Some("owner-b"), false),
            meter,
        ),
        TENANT_B,
        KEY,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"]["error"],
        json!("bundle_not_ensurable_by_caller")
    );
    assert_eq!(
        body["detail"]["message"],
        json!("owner 'owner-b' may not ensure bundle 'non-agentic-ai'")
    );
    assert_eq!(counts(&db).await, before, "nothing may be written");
}

#[tokio::test]
async fn a_foreign_tenant_is_404_and_a_grantee_is_403_owner_only() {
    short_lock_timeout();
    let db = seeded().await;
    let meter = provisioner(&start_stub(Stub::new()).await);

    // owner-a on owner-b's tenant: indistinguishable from absent.
    let (status, body) = ensure(
        &app(&db, ensurer(), meter.clone()),
        TENANT_B,
        KEY,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], json!("tenant_not_found"));
    assert_eq!(body["detail"]["tenant_id"], json!(TENANT_B.to_string()));
    assert_eq!(
        body["detail"]["message"],
        json!(format!("tenant {TENANT_B} is not in tenants"))
    );

    // With an ACTIVE grant the tenant resolves, but only the owner may ensure.
    sqlx::query(
        "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by)
         VALUES ($1, 'owner-a', 'test')",
    )
    .bind(TENANT_B)
    .execute(&db.pool)
    .await
    .expect("grant");
    let before_grant = counts(&db).await;
    let (status, body) = ensure(&app(&db, ensurer(), meter), TENANT_B, KEY, json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], json!("tenant_owner_only"));
    assert_eq!(counts(&db).await, before_grant, "nothing may be written");
}

#[tokio::test]
async fn an_unknown_bundle_is_404() {
    short_lock_timeout();
    let db = seeded().await;
    let before = counts(&db).await;

    let (status, body) = ensure(
        &app(&db, ensurer(), provisioner(&start_stub(Stub::new()).await)),
        TENANT,
        "nope",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], json!("bundle_not_found"));
    assert_eq!(counts(&db).await, before, "nothing may be written");
}

#[tokio::test]
async fn the_body_validation_is_fastapis_422() {
    short_lock_timeout();
    let db = seeded().await;
    let meter = provisioner(&start_stub(Stub::new()).await);

    // Duplicate plan codes: the field validator's value_error.
    let (status, body) = ensure(
        &app(&db, ensurer(), meter.clone()),
        TENANT,
        KEY,
        json!({"plans": ["basic", "basic"]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], json!("value_error"));
    assert!(
        body["detail"][0]["msg"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate plan code")),
        "{body}"
    );

    // An empty list is FastAPI's `too_short` (min_length=1).
    let (status, body) = ensure(
        &app(&db, ensurer(), meter.clone()),
        TENANT,
        KEY,
        json!({"plans": []}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], json!("too_short"));

    // The overrides are bounded by i32::MAX.
    let (status, body) = ensure(
        &app(&db, ensurer(), meter.clone()),
        TENANT,
        KEY,
        json!({"token_budget_override": 2147483648i64}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], json!("less_than_equal"));

    // A non-JSON body is `json_invalid`.
    let request = Request::post(format!("/internal/tenants/{TENANT}/bundles/{KEY}/ensure"))
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .expect("request");
    let response = app(&db, ensurer(), meter)
        .oneshot(request)
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["detail"][0]["type"], json!("json_invalid"));
}
