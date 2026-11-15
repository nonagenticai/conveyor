//! Behaviour of the seven catalogue PLAN and REGION routes, against a real Postgres 18 built
//! by `common`, plus the plan update's billing re-push against a stub billing server.
//!
//! Each case pins the reference's observable contract: the exact response JSON (explicit nulls
//! included), the role and app-owner guards, the create defaults, the update presence semantics,
//! the delete side effects, the list filters, and FastAPI's 422 array for a malformed body.
//!
//! The push cases drive the REAL [`MeteringProvisioner`] at a stub HTTP server, so the recorded
//! path/body is what production sends; a 500 from the stub proves a billing failure cannot
//! change the update's 200 and is repaired through `metering_verified_at`.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::routing::{get as route_get, post as route_post};
use axum::{Json, Router};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::catalogue_plans::{
    create_plan, create_region, delete_plan, get_plan, list_plans, list_regions, update_plan,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3101);
const TENANT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3102);
const SUBJECT_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3201);
const SUBJECT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3202);
const APP_ALPHA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3301);
const APP_ZETA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3302);
const REGION_EU: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3401);
const REGION_US: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3402);
const REGION_OFF: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3403);
const PLAN_BASIC: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3501);
const PLAN_REGIONED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3502);
const PLAN_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3503);
const PLAN_PUSH: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3504);
const LIC_BASIC: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3601);
const LIC_PUSH: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3602);
const LIC_OVERRIDE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3603);
const LIC_EXPIRED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3604);
const LIC_DELETED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3605);
const LIC_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_3606);

const CREATED: &str = "2026-01-02T03:04:05+00:00";

fn config() -> Config {
    Config {
        port: 0,
        environment: "test".to_owned(),
        version: "0.0.0".to_owned(),
        trusted_issuers: Vec::new(),
        introspect_require_auth: false,
        service_actor_id: None,
    }
}

fn app_state(pool: sqlx::PgPool, metering: Option<Arc<MeteringProvisioner>>) -> AppState {
    let state = AppState::new(
        pool,
        config(),
        JwtVerifier::new(Vec::new(), Arc::new(UnavailableJwks)),
    );
    match metering {
        Some(metering) => state.with_metering(metering),
        None => state,
    }
}

fn user(roles: &[&str], sub: &str, owner: Option<&str>) -> User {
    User {
        sub: sub.to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

fn admin(owner: &str) -> User {
    user(&["license-admin"], "admin", Some(owner))
}

fn reader() -> User {
    user(&["license-reader"], "reader", Some("owner-a"))
}

fn cross_owner() -> User {
    User {
        cross_owner: true,
        ..admin("owner-c")
    }
}

/// The seven routes alone, with the caller the middleware would have built.
fn route(db: &common::TestDb, caller: User, metering: Option<Arc<MeteringProvisioner>>) -> Router {
    Router::new()
        .route("/plans", route_get(list_plans).post(create_plan))
        .route(
            "/plans/{plan_id}",
            route_get(get_plan).patch(update_plan).delete(delete_plan),
        )
        .route("/regions", route_get(list_regions).post(create_region))
        .layer(axum::Extension(caller))
        .with_state(app_state(db.pool.clone(), metering))
}

async fn send(app: Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(path);
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request"),
        None => request.body(Body::empty()).expect("request"),
    };
    let response = app.oneshot(request).await.expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 512 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::GET, path, None).await
}

async fn post(app: Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, Method::POST, path, Some(body)).await
}

async fn patch(app: Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, Method::PATCH, path, Some(body)).await
}

async fn delete(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::DELETE, path, None).await
}

fn ids(body: &Value) -> Vec<String> {
    let mut ids: Vec<String> = body
        .as_array()
        .expect("a bare JSON array")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect();
    ids.sort();
    ids
}

async fn count(db: &common::TestDb, sql: &'static str, id: Uuid) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .expect("count")
}

/// One seeded licence: the columns the re-push selection discriminates on.
struct LicenceSeed {
    id: Uuid,
    subject: Uuid,
    tenant: Uuid,
    app: Uuid,
    plan: Uuid,
    status: &'static str,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    override_budget: Option<i32>,
    verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Two owners, two tenants, two subjects, two apps, three regions, four plans and the six
/// licences the re-push selection must discriminate between.
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");
    let instant = chrono::DateTime::parse_from_rfc3339(CREATED)
        .expect("instant")
        .with_timezone(&chrono::Utc);

    for (owner, name) in [
        ("owner-a", "Owner A"),
        ("owner-b", "Owner B"),
        ("owner-c", "Owner C"),
    ] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $2)")
            .bind(owner)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }
    for (id, name, owner) in [
        (TENANT_A, "tenant-a", "owner-a"),
        (TENANT_B, "tenant-b", "owner-b"),
    ] {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
             VALUES ($1, $2, now(), $3, 'local_allowed')",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .execute(&db.pool)
        .await
        .expect("tenant");
    }
    for (id, sub) in [(SUBJECT_A, "sub-a"), (SUBJECT_B, "sub-b")] {
        sqlx::query(
            "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
             VALUES ($1, $2, 'portal', now())",
        )
        .bind(id)
        .bind(sub)
        .execute(&db.pool)
        .await
        .expect("subject");
    }
    for (id, key, name, audience) in [
        (APP_ALPHA, "alpha", "Alpha", "human"),
        (APP_ZETA, "zeta", "Zeta", "platform"),
    ] {
        sqlx::query(
            "INSERT INTO app (id, key, name, description, revoked, audience, created_at)
             VALUES ($1, $2, $3, NULL, false, $4, $5)",
        )
        .bind(id)
        .bind(key)
        .bind(name)
        .bind(audience)
        .bind(instant)
        .execute(&db.pool)
        .await
        .expect("app");
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, 'owner-a')")
            .bind(id)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    for (id, code, name, active) in [
        (REGION_EU, "eu", "Europe", true),
        (REGION_US, "us", "United States", true),
        (REGION_OFF, "apac", "Asia Pacific", false),
    ] {
        sqlx::query(
            "INSERT INTO region (id, code, name, active, created_at) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(id)
        .bind(code)
        .bind(name)
        .bind(active)
        .bind(instant)
        .execute(&db.pool)
        .await
        .expect("region");
    }
    for (id, app, code, name, region, budget, revoked) in [
        (
            PLAN_BASIC,
            APP_ALPHA,
            "basic",
            "Basic",
            None,
            Some(50_000_000),
            false,
        ),
        (
            PLAN_REGIONED,
            APP_ALPHA,
            "eu-plan",
            "EU Plan",
            Some(REGION_EU),
            Some(10),
            false,
        ),
        (
            PLAN_REVOKED,
            APP_ALPHA,
            "legacy",
            "Legacy",
            None,
            None,
            true,
        ),
        (
            PLAN_PUSH,
            APP_ZETA,
            "push",
            "Push",
            None,
            Some(50_000_000),
            false,
        ),
    ] {
        sqlx::query(
            "INSERT INTO plan
                 (id, app_id, code, name, created_at, rate_limit_rpm, rate_limit_burst,
                  region_id, quota_monthly, token_budget_monthly, revoked)
             VALUES ($1, $2, $3, $4, $5, 60, 10, $6, 1000, $7, $8)",
        )
        .bind(id)
        .bind(app)
        .bind(code)
        .bind(name)
        .bind(instant)
        .bind(region)
        .bind(budget)
        .bind(revoked)
        .execute(&db.pool)
        .await
        .expect("plan");
    }

    let licences = [
        // A licence on PLAN_BASIC: makes delete_plan refuse with a 409.
        LicenceSeed {
            id: LIC_BASIC,
            subject: SUBJECT_A,
            tenant: TENANT_A,
            app: APP_ALPHA,
            plan: PLAN_BASIC,
            status: "active",
            expires_at: None,
            deleted_at: None,
            override_budget: None,
            verified_at: None,
        },
        // The live, plan-driven licence a budget change must reach.
        LicenceSeed {
            id: LIC_PUSH,
            subject: SUBJECT_A,
            tenant: TENANT_A,
            app: APP_ZETA,
            plan: PLAN_PUSH,
            status: "active",
            expires_at: None,
            deleted_at: None,
            override_budget: None,
            verified_at: Some(instant),
        },
        // An override: its effective budget does not move with the plan.
        LicenceSeed {
            id: LIC_OVERRIDE,
            subject: SUBJECT_B,
            tenant: TENANT_B,
            app: APP_ZETA,
            plan: PLAN_PUSH,
            status: "active",
            expires_at: None,
            deleted_at: None,
            override_budget: Some(7_000_000),
            verified_at: None,
        },
        // active but past expires_at: not live.
        LicenceSeed {
            id: LIC_EXPIRED,
            subject: SUBJECT_B,
            tenant: TENANT_A,
            app: APP_ZETA,
            plan: PLAN_PUSH,
            status: "active",
            expires_at: Some(instant),
            deleted_at: None,
            override_budget: None,
            verified_at: None,
        },
        // soft-deleted: not an entitlement.
        LicenceSeed {
            id: LIC_DELETED,
            subject: SUBJECT_A,
            tenant: TENANT_B,
            app: APP_ZETA,
            plan: PLAN_PUSH,
            status: "active",
            expires_at: None,
            deleted_at: Some(instant),
            override_budget: None,
            verified_at: None,
        },
        // revoked: never pushed.
        LicenceSeed {
            id: LIC_REVOKED,
            subject: SUBJECT_A,
            tenant: TENANT_A,
            app: APP_ZETA,
            plan: PLAN_PUSH,
            status: "revoked",
            expires_at: None,
            deleted_at: None,
            override_budget: None,
            verified_at: None,
        },
    ];
    for licence in licences {
        sqlx::query(
            "INSERT INTO license
                 (id, subject_id, app_id, plan_id, tenant_id, status, starts_at, created_at,
                  updated_at, expires_at, deleted_at, token_budget_override, metering_verified_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $7, $8, $9, $10, $11)",
        )
        .bind(licence.id)
        .bind(licence.subject)
        .bind(licence.app)
        .bind(licence.plan)
        .bind(licence.tenant)
        .bind(licence.status)
        .bind(instant)
        .bind(licence.expires_at)
        .bind(licence.deleted_at)
        .bind(licence.override_budget)
        .bind(licence.verified_at)
        .execute(&db.pool)
        .await
        .expect("license");
    }
    db
}

// ---------------------------------------------------------------------------------------------
// The billing stub
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
    budget_status: Arc<Mutex<u16>>,
}

impl Stub {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            budget_status: Arc::new(Mutex::new(200)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }

    fn set_budget_status(&self, status: u16) {
        *self.budget_status.lock().expect("stub lock") = status;
    }
}

async fn budget_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    stub.calls.lock().expect("stub lock").push(Call {
        path: "/api/budget".to_owned(),
        tenant: headers
            .get("x-tenant-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        body: Some(body),
    });
    StatusCode::from_u16(*stub.budget_status.lock().expect("stub lock")).expect("status")
}

async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/budget", route_post(budget_push))
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

fn provisioner(base: &str) -> Arc<MeteringProvisioner> {
    // No service account: the push must still carry the path, tenant header and body this test
    // asserts; the bearer is the same code path provisioning already covers.
    Arc::new(MeteringProvisioner::new(base, None).expect("metering provisioner"))
}

/// The spawned re-push runs after the response; wait for its first billing call.
async fn wait_for_budget_calls(stub: &Stub) -> Vec<Call> {
    for _ in 0..160 {
        let calls = stub.calls();
        if !calls.is_empty() {
            return calls;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no billing call within 8s");
}

async fn wait_for_verified_null(db: &common::TestDb, license: Uuid) {
    for _ in 0..200 {
        let verified: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT metering_verified_at FROM license WHERE id = $1")
                .bind(license)
                .fetch_one(&db.pool)
                .await
                .expect("verified");
        if verified.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("metering_verified_at was not cleared within 10s");
}

// ---------------------------------------------------------------------------------------------
// Plans — create
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn create_plan_returns_the_exact_shape_with_the_metered_defaults() {
    let db = seeded().await;
    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/plans",
        json!({"app_id": APP_ALPHA.to_string(), "code": "fresh", "name": "Fresh"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body,
        json!({
            "id": body["id"].clone(),
            "app_id": APP_ALPHA.to_string(),
            "code": "fresh",
            "name": "Fresh",
            "description": null,
            "region_id": null,
            "duration_days": null,
            "rate_limit_rpm": 60,
            "rate_limit_burst": 10,
            "quota_monthly": 1_000_000,
            "token_budget_monthly": 50_000_000,
            "revoked": false,
            "created_at": body["created_at"].clone()
        })
    );

    // An EXPLICIT null is "unlimited" for the quota/budget; on the two NOT NULL rate limits the
    // reference's omitted-kwarg path lands on the column defaults (60/10), not NULL.
    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/plans",
        json!({
            "app_id": APP_ALPHA.to_string(),
            "code": "nulls",
            "name": "Nulls",
            "description": "explicit",
            "region_id": REGION_EU.to_string(),
            "duration_days": 30,
            "rate_limit_rpm": null,
            "rate_limit_burst": null,
            "quota_monthly": null,
            "token_budget_monthly": null
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["description"], "explicit");
    assert_eq!(body["region_id"], REGION_EU.to_string());
    assert_eq!(body["duration_days"], 30);
    assert_eq!(body["rate_limit_rpm"], 60);
    assert_eq!(body["rate_limit_burst"], 10);
    assert!(body["quota_monthly"].is_null());
    assert!(body["token_budget_monthly"].is_null());

    // The unique (app, code, COALESCE(region)) index is the reference's conflict.
    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/plans",
        json!({"app_id": APP_ALPHA.to_string(), "code": "basic", "name": "Dup"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body,
        json!({"detail": "Plan with code 'basic' already exists for this app/region"})
    );

    let missing_app = Uuid::from_u128(99);
    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/plans",
        json!({"app_id": missing_app.to_string(), "code": "x", "name": "X"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"detail": format!("App {missing_app} not found")})
    );

    let missing_region = Uuid::from_u128(98);
    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/plans",
        json!({
            "app_id": APP_ALPHA.to_string(),
            "code": "x",
            "name": "X",
            "region_id": missing_region.to_string()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"detail": format!("Region {missing_region} not found")})
    );

    // Role and app-ownership guards, in the reference's order.
    let (status, body) = post(
        route(&db, reader(), None),
        "/plans",
        json!({"app_id": APP_ALPHA.to_string(), "code": "x", "name": "X"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));

    let (status, body) = post(
        route(&db, admin("owner-b"), None),
        "/plans",
        json!({"app_id": APP_ALPHA.to_string(), "code": "foreign", "name": "Foreign"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "app_not_administrable_by_caller",
            "message": "owner 'owner-b' does not issue this app and may not administer it"
        }})
    );

    let (status, _) = post(
        route(&db, cross_owner(), None),
        "/plans",
        json!({"app_id": APP_ALPHA.to_string(), "code": "break-glass", "name": "BG"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn create_plan_refuses_a_malformed_body_with_fastapis_422() {
    let db = seeded().await;
    for payload in [
        json!({"code": "x", "name": "X"}),
        json!({"app_id": APP_ALPHA.to_string(), "name": "X"}),
        json!({"app_id": APP_ALPHA.to_string(), "code": "x"}),
        json!({"app_id": "not-a-uuid", "code": "x", "name": "X"}),
        json!({"app_id": APP_ALPHA.to_string(), "code": "x", "name": "X", "duration_days": "abc"}),
    ] {
        let (status, body) = post(
            route(&db, admin("owner-a"), None),
            "/plans",
            payload.clone(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{payload} -> {body}"
        );
        assert!(body["detail"].is_array(), "{body}");
    }

    // The exact bound error, field for field.
    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/plans",
        json!({
            "app_id": APP_ALPHA.to_string(),
            "code": "negative",
            "name": "Negative",
            "quota_monthly": -1
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({"detail": [{
            "type": "greater_than_equal",
            "loc": ["body", "quota_monthly"],
            "msg": "Input should be greater than or equal to 0",
            "input": -1,
            "ctx": {"ge": 0}
        }]})
    );

    // An empty body is a missing field at loc ["body"].
    let request = Request::builder()
        .method(Method::POST)
        .uri("/plans")
        .body(Body::empty())
        .expect("request");
    let response = route(&db, admin("owner-a"), None)
        .oneshot(request)
        .await
        .expect("router");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["detail"][0]["type"], "missing");
    assert_eq!(body["detail"][0]["loc"], json!(["body"]));
}

// ---------------------------------------------------------------------------------------------
// Plans — update
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn update_plan_writes_only_the_fields_the_caller_sent() {
    let db = seeded().await;
    let path = format!("/plans/{PLAN_BASIC}");

    let (status, body) = patch(
        route(&db, admin("owner-a"), None),
        &path,
        json!({"name": "Renamed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Renamed");
    assert_eq!(body["code"], "basic");
    assert_eq!(body["quota_monthly"], 1000);

    let (status, body) = patch(
        route(&db, admin("owner-a"), None),
        &path,
        json!({"description": null, "revoked": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["description"].is_null());
    assert_eq!(body["revoked"], json!(true));

    let (status, body) = patch(
        route(&db, admin("owner-a"), None),
        &path,
        json!({"revoked": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revoked"], json!(false));

    for (payload, expected) in [
        (json!({}), json!("No fields to update")),
        (json!({"unknown": 1}), json!("No fields to update")),
        (
            json!({"revoked": null}),
            json!("revoked must be true or false"),
        ),
    ] {
        let (status, body) =
            patch(route(&db, admin("owner-a"), None), &path, payload.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{payload}");
        assert_eq!(body, json!({"detail": expected}));
    }

    let (status, body) = patch(
        route(&db, admin("owner-a"), None),
        "/plans/not-a-uuid",
        json!({"name": "X"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid plan ID: not-a-uuid"}));

    let unknown = Uuid::from_u128(97);
    let (status, body) = patch(
        route(&db, admin("owner-a"), None),
        &format!("/plans/{unknown}"),
        json!({"name": "X"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "Plan not found"}));

    let (status, body) = patch(
        route(&db, admin("owner-a"), None),
        &path,
        json!({"quota_monthly": -1}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"][0]["type"], "greater_than_equal");

    let (status, _) = patch(route(&db, reader(), None), &path, json!({"name": "X"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = patch(
        route(&db, admin("owner-b"), None),
        &path,
        json!({"name": "X"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"]["error"], "app_not_administrable_by_caller");
}

#[tokio::test]
async fn update_plan_pushes_a_changed_token_budget_to_billing() {
    let db = seeded().await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;
    let app = route(&db, admin("owner-a"), Some(provisioner(&base)));

    let (status, body) = patch(
        app.clone(),
        &format!("/plans/{PLAN_PUSH}"),
        json!({"token_budget_monthly": 60_000_000}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["token_budget_monthly"], 60_000_000);

    let calls = wait_for_budget_calls(&stub).await;
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].path, "/api/budget");
    assert_eq!(calls[0].tenant, Some(TENANT_A.to_string()));
    assert_eq!(
        calls[0].body,
        Some(json!({
            "tenant_id": TENANT_A.to_string(),
            "license_id": LIC_PUSH.to_string(),
            "budget_tokens": 60_000_000,
            "period": "monthly",
            "source": "license_sync"
        }))
    );
    // The override, expired, soft-deleted and revoked licences were all excluded.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(stub.calls().len(), 1);

    // An update that does NOT change the value pushes nothing.
    let (status, _) = patch(
        app,
        &format!("/plans/{PLAN_PUSH}"),
        json!({"token_budget_monthly": 60_000_000}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(stub.calls().len(), 1);
}

#[tokio::test]
async fn a_failed_billing_push_never_fails_the_update() {
    let db = seeded().await;
    let stub = Stub::new();
    stub.set_budget_status(500);
    let base = start_stub(stub.clone()).await;
    let app = route(&db, admin("owner-a"), Some(provisioner(&base)));

    let (status, body) = patch(
        app,
        &format!("/plans/{PLAN_PUSH}"),
        json!({"token_budget_monthly": 70_000_000}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a billing fault must not reach the caller: {body}"
    );
    assert_eq!(body["token_budget_monthly"], 70_000_000);

    // The reference clears the verification stamp on failure so the reconcile sweep repairs it.
    wait_for_verified_null(&db, LIC_PUSH).await;
    assert!(
        !stub.calls().is_empty(),
        "the push was attempted against billing"
    );
}

// ---------------------------------------------------------------------------------------------
// Plans — reads and delete
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn list_plans_hides_revoked_and_applies_both_filters() {
    let db = seeded().await;
    let (status, body) = get(route(&db, reader(), None), "/plans").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), {
        let mut expected = vec![
            PLAN_BASIC.to_string(),
            PLAN_REGIONED.to_string(),
            PLAN_PUSH.to_string(),
        ];
        expected.sort();
        expected
    });

    let (status, body) = get(
        route(&db, admin("owner-a"), None),
        &format!("/plans?app_id={APP_ALPHA}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), {
        let mut expected = vec![PLAN_BASIC.to_string(), PLAN_REGIONED.to_string()];
        expected.sort();
        expected
    });

    let (status, body) = get(
        route(&db, admin("owner-a"), None),
        &format!("/plans?app_id={APP_ALPHA}&region_id={REGION_EU}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![PLAN_REGIONED.to_string()]);

    // An empty filter is falsy in the reference and means "no filter".
    let (status, body) = get(route(&db, admin("owner-a"), None), "/plans?app_id=").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().expect("array").len(), 3);

    let (status, body) = get(route(&db, admin("owner-a"), None), "/plans?app_id=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid UUID"}));

    let (status, body) = get(route(&db, user(&[], "nobody", None), None), "/plans").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-reader"})
    );
}

#[tokio::test]
async fn get_plan_returns_one_row_and_404s_otherwise() {
    let db = seeded().await;
    let (status, body) = get(route(&db, reader(), None), &format!("/plans/{PLAN_BASIC}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "id": PLAN_BASIC.to_string(),
            "app_id": APP_ALPHA.to_string(),
            "code": "basic",
            "name": "Basic",
            "description": null,
            "region_id": null,
            "duration_days": null,
            "rate_limit_rpm": 60,
            "rate_limit_burst": 10,
            "quota_monthly": 1000,
            "token_budget_monthly": 50_000_000,
            "revoked": false,
            "created_at": CREATED
        })
    );

    let unknown = Uuid::from_u128(96);
    let (status, body) = get(
        route(&db, admin("owner-a"), None),
        &format!("/plans/{unknown}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "Plan not found"}));

    let (status, body) = get(route(&db, admin("owner-a"), None), "/plans/nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid UUID"}));
}

#[tokio::test]
async fn delete_plan_is_a_204_or_409_and_never_destroys_a_referenced_plan() {
    let db = seeded().await;

    let (status, body) = delete(
        route(&db, admin("owner-a"), None),
        &format!("/plans/{PLAN_BASIC}"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body,
        json!({"detail": "Cannot delete plan: licenses still reference it"})
    );
    assert_eq!(
        count(&db, "SELECT count(*) FROM plan WHERE id = $1", PLAN_BASIC).await,
        1
    );

    // The owner rule runs BEFORE the 409: a non-issuer learns nothing about dependencies.
    let (status, body) = delete(
        route(&db, admin("owner-b"), None),
        &format!("/plans/{PLAN_BASIC}"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"]["error"], "app_not_administrable_by_caller");

    // cross_owner is the documented break-glass path and reaches the conflict.
    let (status, _) = delete(
        route(&db, cross_owner(), None),
        &format!("/plans/{PLAN_BASIC}"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, body) = delete(route(&db, reader(), None), &format!("/plans/{PLAN_BASIC}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));

    let (status, body) = delete(route(&db, admin("owner-a"), None), "/plans/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid plan_id format"}));

    // A plan with no licences is destroyed and the row is gone.
    let (status, body) = delete(
        route(&db, admin("owner-a"), None),
        &format!("/plans/{PLAN_REVOKED}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(body, Value::Null);
    assert_eq!(
        count(&db, "SELECT count(*) FROM plan WHERE id = $1", PLAN_REVOKED).await,
        0
    );

    let unknown = Uuid::from_u128(95);
    let (status, body) = delete(
        route(&db, admin("owner-a"), None),
        &format!("/plans/{unknown}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "Plan not found"}));
}

// ---------------------------------------------------------------------------------------------
// Regions
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn create_region_is_idempotent_by_code() {
    let db = seeded().await;

    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/regions",
        json!({"code": "eu", "name": "Europe Renamed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "id": REGION_EU.to_string(),
            "code": "eu",
            "name": "Europe",
            "active": true,
            "created_at": CREATED
        })
    );

    let (status, body) = post(
        route(&db, admin("owner-a"), None),
        "/regions",
        json!({"code": "latam", "name": "Latin America", "active": false}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["id"].as_str().expect("id").to_owned();
    assert_eq!(
        body,
        json!({
            "id": id,
            "code": "latam",
            "name": "Latin America",
            "active": false,
            "created_at": body["created_at"].clone()
        })
    );

    // The same code again returns the SAME row unchanged, still 200.
    let (status, again) = post(
        route(&db, admin("owner-a"), None),
        "/regions",
        json!({"code": "latam", "name": "Different name"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["id"], id);
    assert_eq!(again["name"], "Latin America");

    for payload in [
        json!({"name": "No Code"}),
        json!({"code": "no-name"}),
        json!({"code": "bad", "name": "Bad", "active": "maybe"}),
    ] {
        let (status, body) = post(
            route(&db, admin("owner-a"), None),
            "/regions",
            payload.clone(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{payload} -> {body}"
        );
        assert!(body["detail"].is_array(), "{body}");
    }

    let (status, body) = post(
        route(&db, reader(), None),
        "/regions",
        json!({"code": "forbidden", "name": "Forbidden"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));
}

#[tokio::test]
async fn list_regions_defaults_to_active_only() {
    let db = seeded().await;

    let (status, body) = get(route(&db, reader(), None), "/regions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), {
        let mut expected = vec![REGION_EU.to_string(), REGION_US.to_string()];
        expected.sort();
        expected
    });

    let (status, body) = get(
        route(&db, admin("owner-a"), None),
        "/regions?active_only=false",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), {
        let mut expected = vec![
            REGION_EU.to_string(),
            REGION_US.to_string(),
            REGION_OFF.to_string(),
        ];
        expected.sort();
        expected
    });

    let (status, body) = get(
        route(&db, admin("owner-a"), None),
        "/regions?active_only=maybe",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        json!({"detail": [{
            "type": "bool_parsing",
            "loc": ["query", "active_only"],
            "msg": "Input should be a valid boolean, unable to interpret input",
            "input": "maybe"
        }]})
    );

    let (status, _) = get(route(&db, user(&[], "nobody", None), None), "/regions").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
