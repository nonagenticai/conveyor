//! Behaviour of `POST` and `DELETE /licenses/user/{subject_id}`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, plus
//! a stub billing server for the metering side. The route-level router injects a `User` with
//! `Extension`, so the handler's own guards and issuance authorization are exercised without the
//! JWT/opaque-token path (that path is covered by `tests/introspect.rs`).
//!
//! The reference is `main.py::create_user_license` (:4825) and `delete_user_licenses` (:10265) at
//! pinned `4e8a08d`.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{Router, get, post};
use axum::{Extension, Json};
use conveyor_auth::oauth::ClientCredentials;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::user_licenses::{
    create_user_license, delete_user_licenses, list_user_licenses,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_0003);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_0004);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_0005);
const OTHER_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_000a);

/// The Keycloak `sub` the portal passes as `subject_id`.
const SUB: &str = "0195f48b-d000-7000-8000-000000000045";

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

fn state(pool: sqlx::PgPool, metering: Option<Arc<MeteringProvisioner>>) -> AppState {
    let state = AppState::new(
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
    );
    match metering {
        Some(metering) => state.with_metering(metering),
        None => state,
    }
}

fn route(state: AppState, user: User) -> Router {
    Router::new()
        .route(
            "/licenses/user/{subject_id}",
            get(list_user_licenses)
                .post(create_user_license)
                .delete(delete_user_licenses),
        )
        .layer(Extension(user))
        .with_state(state)
}

fn user(roles: &[&str], sub: &str, tenant: Option<Uuid>, owner_key: Option<&str>) -> User {
    User {
        sub: sub.to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: tenant.map(|tenant| tenant.to_string()),
        owner_key: owner_key.map(str::to_owned),
        ..User::default()
    }
}

async fn send(app: Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(match body {
            Some(body) => Body::from(serde_json::to_vec(&body).expect("json")),
            None => Body::empty(),
        })
        .expect("request");
    let response = app.oneshot(request).await.expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn create_body() -> Value {
    json!({
        "app_id": APP.to_string(),
        "plan_id": PLAN.to_string(),
        "metadata": {"source": "portal"},
    })
}

/// One owned tenant, one app the owner issues, one metered plan. No subject, no licence.
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-a', 'Owner A')")
        .execute(&db.pool)
        .await
        .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-a', now(), 'owner-a', 'local_allowed')",
    )
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("tenant");
    sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, 'demo', 'Demo', now())")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at, quota_monthly, token_budget_monthly)
         VALUES ($1, $2, 'basic', 'Basic', now(), 100, 1000)",
    )
    .bind(PLAN)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("plan");
    sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, 'owner-a')")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("app_issuer");
    db
}

async fn insert_subject_and_licence(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(SUBJECT)
    .bind(SUB)
    .execute(pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id, metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, NULL::jsonb)",
    )
    .bind(LICENSE)
    .bind(SUBJECT)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .execute(pool)
    .await
    .expect("license");
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at)
         VALUES ($1, $2, 'hash-1', 'api', now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_7300_8000_0000_0000_000b))
    .bind(LICENSE)
    .execute(pool)
    .await
    .expect("token");
}

// ---------------------------------------------------------------------------------------------
// Billing stub: records every metering push.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Call {
    path: String,
    tenant: Option<String>,
    body: Option<Value>,
}

#[derive(Clone)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
    status: Arc<Mutex<u16>>,
}

impl Stub {
    fn new(status: u16) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(Mutex::new(status)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }
}

fn record(stub: &Stub, path: &str, headers: &HeaderMap, body: Value) {
    stub.calls.lock().expect("stub lock").push(Call {
        path: path.to_owned(),
        tenant: headers
            .get("x-tenant-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        body: Some(body),
    });
}

async fn push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
    path: &'static str,
) -> StatusCode {
    record(&stub, path, &headers, body);
    StatusCode::from_u16(*stub.status.lock().expect("stub lock")).expect("status")
}

async fn budget_push(stub: AxumState<Stub>, headers: HeaderMap, body: Json<Value>) -> StatusCode {
    push(stub, headers, body, "/api/budget").await
}

async fn quota_push(stub: AxumState<Stub>, headers: HeaderMap, body: Json<Value>) -> StatusCode {
    push(stub, headers, body, "/api/quota/config").await
}

async fn token() -> Json<Value> {
    Json(json!({"access_token": "stub-token", "token_type": "Bearer", "expires_in": 3600}))
}

async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/budget", post(budget_push))
        .route("/api/quota/config", post(quota_push))
        .route("/token", post(token))
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

/// Poll a cheap condition on the shared stub, yielding so the spawned task can run.
async fn wait_for<F: Fn() -> bool>(check: F, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {label}");
}

// ---------------------------------------------------------------------------------------------
// POST
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_create_returns_the_captured_shape_and_meters_the_licence() {
    let db = seeded().await;
    let stub = Stub::new(201);
    let base = start_stub(stub.clone()).await;
    let app = route(
        state(db.pool.clone(), Some(Arc::new(provisioner(&base)))),
        user(
            &["license-manager"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );

    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // The captured `LicenseResponse` has 22 properties; every one is serialised, nulls included.
    assert_eq!(body.as_object().expect("object").len(), 22, "{body}");
    let license_id = body["id"].as_str().expect("id").to_owned();
    assert_eq!(body["tenant_id"], TENANT.to_string());
    assert_eq!(body["app_id"], APP.to_string());
    assert_eq!(body["plan_id"], PLAN.to_string());
    assert_eq!(body["status"], "active");
    assert_eq!(body["effective_status"], "active");
    assert_eq!(body["metadata"], json!({"source": "portal"}));
    assert_eq!(body["app"]["key"], "demo");
    assert_eq!(body["app"]["name"], "Demo");
    // `_enrich_license_response` populates the plan on this route.
    assert_eq!(body["plan"]["code"], "basic");
    assert_eq!(body["plan"]["quota_monthly"], 100);
    assert_eq!(body["plan"]["token_budget_monthly"], 1000);

    // The licence row, and the subject it created.
    let row: (Uuid, Uuid, Option<Value>) =
        sqlx::query_as("SELECT subject_id, tenant_id, metadata FROM license WHERE id::text = $1")
            .bind(&license_id)
            .fetch_one(&db.pool)
            .await
            .expect("licence row");
    assert_eq!(row.1, TENANT);
    assert_eq!(row.2, Some(json!({"source": "portal"})));
    let (keycloak_sub, subject_tenant): (String, Option<Uuid>) =
        sqlx::query_as("SELECT keycloak_sub, tenant_id FROM subject WHERE id = $1")
            .bind(row.0)
            .fetch_one(&db.pool)
            .await
            .expect("subject row");
    assert_eq!(keycloak_sub, SUB);
    // `attribute_subject_to_tenant`: the just-written licence is the evidence of parentage.
    assert_eq!(subject_tenant, Some(TENANT));

    // Fire-and-forget metering: both mirrors, for this licence and tenant.
    let expected_id = license_id.clone();
    wait_for(
        || {
            stub.calls()
                .iter()
                .filter(|call| {
                    call.body
                        .as_ref()
                        .and_then(|body| body["license_id"].as_str())
                        == Some(expected_id.as_str())
                })
                .count()
                >= 2
        },
        "both metering pushes",
    )
    .await;
    let calls = stub.calls();
    let expected_tenant = TENANT.to_string();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.path == "/api/budget")
            .count(),
        1,
        "{calls:?}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.path == "/api/quota/config")
            .count(),
        1,
        "{calls:?}"
    );
    for call in &calls {
        assert_eq!(call.tenant.as_deref(), Some(expected_tenant.as_str()));
        assert_eq!(
            call.body.as_ref().expect("body")["tenant_id"],
            expected_tenant
        );
    }
    let budget = calls
        .iter()
        .find(|call| call.path == "/api/budget")
        .expect("budget call");
    assert_eq!(budget.body.as_ref().expect("body")["budget_tokens"], 1000);
    assert_eq!(
        budget.body.as_ref().expect("body")["source"],
        "license_sync"
    );
    let quota = calls
        .iter()
        .find(|call| call.path == "/api/quota/config")
        .expect("quota call");
    assert_eq!(quota.body.as_ref().expect("body")["quota_monthly"], 100);
    assert_eq!(
        quota.body.as_ref().expect("body")["app_id"],
        APP.to_string()
    );

    // The stamp is written only after every owed push was accepted.
    let license_id = Uuid::parse_str(&license_id).expect("uuid");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stamp: Option<chrono::DateTime<chrono::Utc>> = None;
    while Instant::now() < deadline {
        stamp = sqlx::query_scalar("SELECT metering_provisioned_at FROM license WHERE id = $1")
            .bind(license_id)
            .fetch_one(&db.pool)
            .await
            .expect("stamp");
        if stamp.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(stamp.is_some(), "metering_provisioned_at must be stamped");
}

#[tokio::test]
async fn a_second_post_returns_the_existing_active_licence() {
    let db = seeded().await;
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );

    let (status, first) = send(
        app.clone(),
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    // No metering provisioner is attached: the licence is created, its stamp stays NULL.
    let (status, second) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    assert_eq!(second["id"], first["id"], "the active licence is returned");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM license WHERE tenant_id = $1 AND app_id = $2")
            .bind(TENANT)
            .bind(APP)
            .fetch_one(&db.pool)
            .await
            .expect("count");
    assert_eq!(count, 1);
    let stamp: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT metering_provisioned_at FROM license WHERE id::text = $1")
            .bind(first["id"].as_str().expect("id"))
            .fetch_one(&db.pool)
            .await
            .expect("stamp");
    assert!(stamp.is_none());
}

#[tokio::test]
async fn a_self_tenant_mint_is_admitted() {
    let db = seeded().await;
    let tenant_ref = TENANT.to_string();
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{tenant_ref}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let subject_id = Uuid::parse_str(body["subject_id"].as_str().expect("subject")).expect("uuid");
    let keycloak_sub: String = sqlx::query_scalar("SELECT keycloak_sub FROM subject WHERE id = $1")
        .bind(subject_id)
        .fetch_one(&db.pool)
        .await
        .expect("subject");
    assert_eq!(keycloak_sub, tenant_ref);
    assert_eq!(body["tenant_id"], tenant_ref);
}

#[tokio::test]
async fn a_registered_tenant_id_as_subject_is_a_409() {
    let db = seeded().await;
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-b', 'Owner B')")
        .execute(&db.pool)
        .await
        .expect("owner b");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-b', now(), 'owner-b', 'local_allowed')",
    )
    .bind(OTHER_TENANT)
    .execute(&db.pool)
    .await
    .expect("other tenant");
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{OTHER_TENANT}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let detail = body["detail"].as_str().expect("a string detail");
    assert!(detail.contains("already registered tenant"), "{detail}");
}

#[tokio::test]
async fn issuance_refusals_use_the_captured_error_keys() {
    let db = seeded().await;

    // 1. The caller's owner does not issue the app.
    sqlx::query("DELETE FROM app_issuer WHERE app_id = $1")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("drop issuer");
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_issuable_by_caller");

    // 2. The caller issues the app but neither owns nor holds a grant on the tenant.
    sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, 'owner-a')")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("issuer");
    sqlx::query(
        "INSERT INTO tenant_owner (key, display_name) VALUES ('owner-b', 'Owner B')
         ON CONFLICT DO NOTHING",
    )
    .execute(&db.pool)
    .await
    .expect("owner b");
    sqlx::query("UPDATE tenants SET owner_key = 'owner-b' WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("re-owner");
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_not_accessible");

    // 3. An unknown client is refused before any issuance branch.
    sqlx::query("UPDATE tenants SET owner_key = 'owner-a' WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("restore owner");
    let app = route(
        state(db.pool.clone(), None),
        user(&["license-admin"], "operator-sub", Some(TENANT), None),
    );
    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "unknown_api_client");
}

#[tokio::test]
async fn invalid_bodies_are_fastapi_422s() {
    let db = seeded().await;
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );

    let (status, body) = send(
        app.clone(),
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "missing");
    assert_eq!(body["detail"][0]["loc"], json!(["body", "app_id"]));

    let (status, body) = send(
        app.clone(),
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(json!({"app_id": APP.to_string(), "plan_id": "not-a-uuid"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "uuid_parsing");
    assert_eq!(body["detail"][0]["loc"], json!(["body", "plan_id"]));

    // `AwareDatetime`: an offset-less value is refused, never reinterpreted.
    let (status, body) = send(
        app.clone(),
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(json!({
            "app_id": APP.to_string(),
            "plan_id": PLAN.to_string(),
            "starts_at": "2027-01-01T00:00:00",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "datetime_parsing");

    // A non-human realm cannot hide a person from the seat count.
    let (status, body) = send(
        app.clone(),
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(json!({
            "app_id": APP.to_string(),
            "plan_id": PLAN.to_string(),
            "subject_realm": "service",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "value_error");

    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(json!({
            "app_id": APP.to_string(),
            "plan_id": PLAN.to_string(),
            "subject_realm": "BAD REALM",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "string_pattern_mismatch");
}

#[tokio::test]
async fn metering_down_does_not_change_the_response() {
    let db = seeded().await;
    let stub = Stub::new(500);
    let base = start_stub(stub.clone()).await;
    let app = route(
        state(db.pool.clone(), Some(Arc::new(provisioner(&base)))),
        user(
            &["license-admin"],
            "operator-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(
        app,
        "POST",
        &format!("/licenses/user/{SUB}"),
        Some(create_body()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let license_id = Uuid::parse_str(body["id"].as_str().expect("id")).expect("uuid");

    // Both legs retry once; wait until every attempt has been made, then assert the row was never
    // stamped — an unmetered licence is repairable, a lying stamp is not.
    wait_for(|| stub.calls().len() >= 4, "both metering retries").await;
    let stamp: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT metering_provisioned_at FROM license WHERE id = $1")
            .bind(license_id)
            .fetch_one(&db.pool)
            .await
            .expect("stamp");
    assert!(stamp.is_none(), "a failed provisioning must not stamp");
}

// ---------------------------------------------------------------------------------------------
// DELETE
// ---------------------------------------------------------------------------------------------

/// The retained-row audit columns the soft-delete assertions read.
#[derive(sqlx::FromRow)]
struct DeletedLicenceRow {
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
    deleted_by: Option<String>,
    delete_reason: Option<String>,
    deleted_from_status: Option<String>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[tokio::test]
async fn delete_soft_requires_confirmation_then_stamps_and_revokes() {
    let db = seeded().await;
    insert_subject_and_licence(&db.pool).await;
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "admin-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let path = format!("/licenses/user/{SUB}?reason=cleanup");

    // Unconfirmed: fail closed, with the exact radius.
    let (status, body) = send(app.clone(), "DELETE", &path, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["detail"]["error"],
        "licence_delete_requires_confirmation"
    );
    assert_eq!(body["detail"]["licenses"], 1);
    assert_eq!(body["detail"]["active_licences"], 1);
    assert_eq!(body["detail"]["tokens"], 1);
    assert_eq!(body["detail"]["subject_id"], SUBJECT.to_string());
    assert_eq!(body["detail"]["tenant_id"], TENANT.to_string());

    // A mismatched acknowledgement is a different refusal.
    let (status, body) = send(
        app.clone(),
        "DELETE",
        &format!("{path}&confirm_licenses=2"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "licence_delete_count_mismatch");

    let (status, body) = send(
        app.clone(),
        "DELETE",
        &format!("{path}&confirm_licenses=1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"deleted": 1, "mode": "soft", "released_dependency_ids": []})
    );

    // The row is RETAINED and stamped; status/revoked_at/tokens make the deletion bite.
    let row: DeletedLicenceRow = sqlx::query_as(
        "SELECT deleted_at, status, deleted_by, delete_reason, deleted_from_status, revoked_at
           FROM license WHERE id = $1",
    )
    .bind(LICENSE)
    .fetch_one(&db.pool)
    .await
    .expect("retained row");
    assert!(row.deleted_at.is_some(), "deleted_at stamped");
    assert_eq!(row.status, "revoked");
    assert_eq!(row.deleted_by.as_deref(), Some("admin-sub"));
    assert_eq!(row.delete_reason.as_deref(), Some("cleanup"));
    assert_eq!(row.deleted_from_status.as_deref(), Some("active"));
    assert!(row.revoked_at.is_some(), "revoked_at stamped");
    let token_revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE license_id = $1")
        .bind(LICENSE)
        .fetch_one(&db.pool)
        .await
        .expect("token");
    assert!(token_revoked);

    // A repeat skips the already-stamped row (first deletion stays the first).
    let (status, body) = send(app, "DELETE", &format!("{path}&confirm_licenses=1"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], 0);
}

#[tokio::test]
async fn delete_needs_the_admin_role_and_an_authorized_app() {
    let db = seeded().await;
    insert_subject_and_licence(&db.pool).await;

    // The role guard is realm-only and precedes any subject lookup.
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-reader"],
            "reader-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(app, "DELETE", &format!("/licenses/user/{SUB}"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"], "Role 'license-admin' required");

    // An unknown subject is a 404.
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "admin-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(app, "DELETE", "/licenses/user/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Subject not found"}));

    // The caller must issue every app the deletion would touch.
    sqlx::query("DELETE FROM app_issuer WHERE app_id = $1")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("drop issuer");
    let app = route(
        state(db.pool.clone(), None),
        user(
            &["license-admin"],
            "admin-sub",
            Some(TENANT),
            Some("owner-a"),
        ),
    );
    let (status, body) = send(app, "DELETE", &format!("/licenses/user/{SUB}"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_issuable_by_caller");
}
