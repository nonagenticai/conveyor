//! Behaviour of the three licence-admin operations:
//! `GET /licenses`, `GET /licenses/admin/all` and `PATCH /licenses/{license_id}/budget`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline,
//! so the predicates, the tenant scoping, `include_deleted` and the budget write are exercised
//! against the captured schema rather than a mock. The assertions pin the EXACT JSON the
//! capture describes (the 22-property `LicenseResponse`, explicit nulls included).

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, State as AxumState};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get as route_get, patch, post as route_post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::BillingTeardown;
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::licence_admin::{
    list_admin_licenses, list_licenses, set_license_budget,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1101);
const TENANT2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1102);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1201);
const APP2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1202);
const APP3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1203);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1301);
const PLAN2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1302);
const PLAN3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1303);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1401);
const SUBJECT2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1402);
const L1: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1501);
const L2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1502);
const L3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1503);
const L4: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1504);
const L5: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1505);
const L6: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1506);
const L7: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1507);

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("rfc3339 instant")
        .with_timezone(&Utc)
}

fn state(pool: sqlx::PgPool) -> AppState {
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
}

fn user(roles: &[&str], sub: &str, tenant: Option<Uuid>, owner: Option<&str>) -> User {
    User {
        sub: sub.to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: tenant.map(|tenant| tenant.to_string()),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

/// The three routes alone, with the caller the middleware would have built.
fn route(db: &common::TestDb, user: User) -> Router {
    Router::new()
        .route("/licenses", route_get(list_licenses))
        .route("/licenses/admin/all", route_get(list_admin_licenses))
        .route("/licenses/{license_id}/budget", patch(set_license_budget))
        .layer(axum::Extension(user))
        .with_state(state(db.pool.clone()))
}

/// The budget route with BOTH billing clients pointed at `base`.
fn route_with_billing(db: &common::TestDb, user: User, base: &str) -> Router {
    let state = state(db.pool.clone())
        .with_metering(Arc::new(
            MeteringProvisioner::new(base, None).expect("metering provisioner"),
        ))
        .with_billing(Arc::new(
            BillingTeardown::new(base, None).expect("billing teardown"),
        ));
    Router::new()
        .route("/licenses/{license_id}/budget", patch(set_license_budget))
        .layer(axum::Extension(user))
        .with_state(state)
}

// ---------------------------------------------------------------------------------------------
// The billing stub: records every call and answers with configured statuses/rows.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct BillingCall {
    method: &'static str,
    path: String,
    tenant: Option<String>,
    body: Option<Value>,
}

#[derive(Clone)]
struct BillingStub {
    calls: Arc<Mutex<Vec<BillingCall>>>,
    budget_status: Arc<Mutex<u16>>,
    rows: Arc<Mutex<Vec<Value>>>,
}

impl BillingStub {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            budget_status: Arc::new(Mutex::new(200)),
            rows: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<BillingCall> {
        self.calls.lock().expect("stub lock").clone()
    }

    fn set_budget_status(&self, status: u16) {
        *self.budget_status.lock().expect("stub lock") = status;
    }

    fn set_rows(&self, rows: Vec<Value>) {
        *self.rows.lock().expect("stub lock") = rows;
    }
}

fn record_billing(
    stub: &BillingStub,
    method: &'static str,
    path: String,
    headers: &HeaderMap,
    body: Option<Value>,
) {
    let tenant = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    stub.calls.lock().expect("stub lock").push(BillingCall {
        method,
        path,
        tenant,
        body,
    });
}

async fn budget_push(
    AxumState(stub): AxumState<BillingStub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record_billing(&stub, "POST", "/api/budget".to_owned(), &headers, Some(body));
    StatusCode::from_u16(*stub.budget_status.lock().expect("stub lock")).expect("status")
}

async fn budget_list(
    AxumState(stub): AxumState<BillingStub>,
    headers: HeaderMap,
    Path(tenant): Path<String>,
) -> axum::response::Response {
    record_billing(
        &stub,
        "GET",
        format!("/api/budget/list/{tenant}"),
        &headers,
        None,
    );
    let rows = stub.rows.lock().expect("stub lock").clone();
    Json(json!({"data": rows})).into_response()
}

async fn budget_delete(
    AxumState(stub): AxumState<BillingStub>,
    headers: HeaderMap,
    Path(budget_id): Path<String>,
) -> StatusCode {
    record_billing(
        &stub,
        "DELETE",
        format!("/api/budget/{budget_id}"),
        &headers,
        None,
    );
    StatusCode::OK
}

async fn start_billing_stub(stub: BillingStub) -> String {
    let app = Router::new()
        .route("/api/budget", route_post(budget_push))
        .route("/api/budget/list/{tenant_id}", route_get(budget_list))
        .route("/api/budget/{budget_id}", delete(budget_delete))
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

/// Poll until `predicate` holds: the push is SPAWNED, so it lands after the response.
async fn wait_for_billing(
    stub: &BillingStub,
    predicate: impl Fn(&[BillingCall]) -> bool,
) -> Vec<BillingCall> {
    for _ in 0..100 {
        let calls = stub.calls();
        if predicate(&calls) {
            return calls;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let calls = stub.calls();
    panic!("billing calls did not arrive; seen: {calls:?}");
}

/// Poll until the failed push has cleared the licence's verified stamp.
async fn wait_for_verified_cleared(db: &common::TestDb, license_id: Uuid) -> bool {
    for _ in 0..100 {
        let stamp: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT metering_verified_at FROM license WHERE id = $1")
                .bind(license_id)
                .fetch_one(&db.pool)
                .await
                .expect("stamp state");
        if stamp.is_none() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
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
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
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

/// Licence ids in the body, sorted so the assertion never depends on the server's row order
/// (neither listing adds an ORDER BY — the reference's `query.all()` leaves it to the DB).
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

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    for (owner, name) in [("owner-a", "Owner A"), ("owner-b", "Owner B")] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $2)")
            .bind(owner)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }
    for (id, name, owner) in [
        (TENANT, "tenant-a", "owner-a"),
        (TENANT2, "tenant-b", "owner-b"),
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
    for (id, key, name) in [
        (APP, "demo", "Demo"),
        (APP2, "demo-2", "Demo 2"),
        (APP3, "demo-3", "Demo 3"),
    ] {
        sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $3, now())")
            .bind(id)
            .bind(key)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("app");
    }
    // APP is issued by owner-a, APP2 by owner-b, APP3 by owner-a only.
    for (app, owner) in [(APP, "owner-a"), (APP2, "owner-b"), (APP3, "owner-a")] {
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(app)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    for (id, app, code, name, rpm, burst, quota, budget) in [
        (
            PLAN,
            APP,
            "basic",
            "Basic",
            60,
            10,
            Some(1000),
            Some(50_000_000),
        ),
        (PLAN2, APP2, "basic-2", "Basic 2", 120, 20, None, None),
        (PLAN3, APP3, "basic-3", "Basic 3", 90, 15, None, None),
    ] {
        sqlx::query(
            "INSERT INTO plan
                 (id, app_id, code, name, created_at, rate_limit_rpm, rate_limit_burst,
                  quota_monthly, token_budget_monthly)
             VALUES ($1, $2, $3, $4, now(), $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(app)
        .bind(code)
        .bind(name)
        .bind(rpm)
        .bind(burst)
        .bind(quota)
        .bind(budget)
        .execute(&db.pool)
        .await
        .expect("plan");
    }
    for (id, sub) in [(SUBJECT, "sub-1"), (SUBJECT2, "sub-2")] {
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

    for (id, subject, tenant, app, plan, status, created_at, deleted_at) in [
        // The caller's tenant.
        (L1, SUBJECT, TENANT, APP, PLAN, "active", "2026-01-02", None),
        (
            L2,
            SUBJECT2,
            TENANT,
            APP2,
            PLAN2,
            "active",
            "2026-01-03",
            None,
        ),
        (
            L3,
            SUBJECT2,
            TENANT,
            APP,
            PLAN,
            "revoked",
            "2026-01-04",
            None,
        ),
        // A foreign tenant: never listed by GET /licenses.
        (
            L4,
            SUBJECT,
            TENANT2,
            APP,
            PLAN,
            "active",
            "2026-01-05",
            None,
        ),
        // Soft-deleted (retained): hidden unless include_deleted=true.
        (
            L5,
            SUBJECT,
            TENANT,
            APP,
            PLAN,
            "revoked",
            "2026-01-06",
            Some("2026-09-01"),
        ),
        // The foreign tenant's second licence, for the admin read.
        (
            L6,
            SUBJECT2,
            TENANT2,
            APP2,
            PLAN2,
            "active",
            "2026-01-07",
            None,
        ),
        // owner-a ISSUES APP3 but does not own TENANT2: tenant_not_accessible.
        (
            L7,
            SUBJECT,
            TENANT2,
            APP3,
            PLAN3,
            "active",
            "2026-01-08",
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO license
                 (id, subject_id, app_id, plan_id, tenant_id, status, starts_at,
                  created_at, deleted_at, deleted_from_status)
             VALUES ($1, $2, $3, $4, $5, $6, '2026-01-01T00:00:00Z', $7, $8,
                     CASE WHEN $8::timestamptz IS NULL THEN NULL ELSE 'active' END)",
        )
        .bind(id)
        .bind(subject)
        .bind(app)
        .bind(plan)
        .bind(tenant)
        .bind(status)
        .bind(at(&format!("{created_at}T00:00:00Z")))
        .bind(deleted_at.map(|value| at(&format!("{value}T00:00:00Z"))))
        .execute(&db.pool)
        .await
        .expect("license");
    }

    db
}

fn reader(tenant: Uuid) -> User {
    user(&["license-reader"], "reader-sub", Some(tenant), None)
}

fn operator(owner: Option<&str>) -> User {
    user(&["license-admin"], "operator-sub", None, owner)
}

#[tokio::test]
async fn the_listing_is_tenant_scoped_filtered_and_fully_enriched() {
    let db = seeded().await;

    let (status, body) = get(route(&db, reader(TENANT)), "/licenses").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The foreign tenant's licences (L4, L7) and the soft-deleted L5 are absent.
    assert_eq!(
        ids(&body),
        vec![L1.to_string(), L2.to_string(), L3.to_string()]
    );

    // The captured 22-property LicenseResponse, with BOTH nested objects populated.
    let row = body
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == L1.to_string())
        .expect("L1");
    assert_eq!(row.as_object().expect("object").len(), 22);
    assert_eq!(row["subject_id"], SUBJECT.to_string());
    assert_eq!(row["tenant_id"], TENANT.to_string());
    assert_eq!(row["app_id"], APP.to_string());
    assert_eq!(row["plan_id"], PLAN.to_string());
    assert_eq!(row["status"], "active");
    assert_eq!(row["effective_status"], "active");
    assert_eq!(row["starts_at"], "2026-01-01T00:00:00+00:00");
    assert!(row["region_id"].is_null());
    assert!(row["expires_at"].is_null());
    assert!(row["revoked_at"].is_null());
    assert!(row["metadata"].is_null());
    assert!(row["metering_provisioned_at"].is_null());
    assert!(row["metering_verified_at"].is_null());
    assert!(row["token_budget_override"].is_null());
    assert!(row["quota_override"].is_null());
    assert_eq!(row["is_dependency"], false);
    assert!(row["bundle_id"].is_null());
    assert_eq!(
        row["app"],
        json!({"id": APP.to_string(), "key": "demo", "name": "Demo"})
    );
    assert_eq!(
        row["plan"],
        json!({
            "id": PLAN.to_string(),
            "code": "basic",
            "name": "Basic",
            "rate_limit_rpm": 60,
            "rate_limit_burst": 10,
            "quota_monthly": 1000,
            "token_budget_monthly": 50_000_000,
        })
    );

    // A terminal stored status is reported verbatim by effective_status.
    let revoked = body
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == L3.to_string())
        .expect("L3");
    assert_eq!(revoked["status"], "revoked");
    assert_eq!(revoked["effective_status"], "revoked");

    // Each filter, alone and combined.
    for (query, expected) in [
        ("?status=revoked".to_owned(), vec![L3.to_string()]),
        (
            "?app_id=".to_owned() + &APP2.to_string(),
            vec![L2.to_string()],
        ),
        (
            "?plan_id=".to_owned() + &PLAN2.to_string(),
            vec![L2.to_string()],
        ),
        (
            "?subject_id=".to_owned() + &SUBJECT2.to_string(),
            vec![L2.to_string(), L3.to_string()],
        ),
        (
            "?subject_id=".to_owned() + &SUBJECT2.to_string() + "&status=revoked",
            vec![L3.to_string()],
        ),
        // An empty filter value is the reference's falsy "no filter".
        (
            "?status=".to_owned(),
            vec![L1.to_string(), L2.to_string(), L3.to_string()],
        ),
    ] {
        let (status, body) = get(route(&db, reader(TENANT)), &format!("/licenses{query}")).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(ids(&body), expected, "{query}: {body}");
    }

    // A malformed query UUID is the reference's 400, not FastAPI's 422.
    let (status, body) = get(route(&db, reader(TENANT)), "/licenses?app_id=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body, json!({"detail": "Invalid UUID"}));

    // No tenant context is a 401 BEFORE the role refusal (the reference resolves the tenant
    // dependency first).
    let tenantless = user(&["license-reader"], "operator-sub", None, None);
    let (status, body) = get(route(&db, tenantless), "/licenses").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body, json!({"detail": "Missing tenant context"}));

    // A caller with neither read role is refused with the reference's exact detail.
    let (status, body) = get(
        route(&db, user(&["license-manager"], "m-sub", Some(TENANT), None)),
        "/licenses",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-reader"})
    );
}

#[tokio::test]
async fn the_admin_read_crosses_tenants_and_toggles_soft_deleted_rows() {
    let db = seeded().await;

    let (status, body) = get(route(&db, operator(None)), "/licenses/admin/all").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Every tenant, minus soft-deleted L5.
    assert_eq!(
        ids(&body),
        vec![
            L1.to_string(),
            L2.to_string(),
            L3.to_string(),
            L4.to_string(),
            L6.to_string(),
            L7.to_string(),
        ]
    );
    // The foreign tenant's rows are present, with their own tenant_id.
    let foreign = body
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == L4.to_string())
        .expect("L4");
    assert_eq!(foreign["tenant_id"], TENANT2.to_string());

    // include_deleted=true is the audit view: the retained row reappears.
    let (status, body) = get(
        route(&db, operator(None)),
        "/licenses/admin/all?include_deleted=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body).len(), 7);
    assert!(ids(&body).contains(&L5.to_string()));

    // The status filter composes with the flag.
    let (_, body) = get(
        route(&db, operator(None)),
        "/licenses/admin/all?status=revoked",
    )
    .await;
    assert_eq!(ids(&body), vec![L3.to_string()]);
    let (_, body) = get(
        route(&db, operator(None)),
        "/licenses/admin/all?status=revoked&include_deleted=true",
    )
    .await;
    assert_eq!(ids(&body), vec![L3.to_string(), L5.to_string()]);

    // license-admin ALONE: a reader is refused with the reference's detail.
    let (status, body) = get(route(&db, reader(TENANT)), "/licenses/admin/all").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin"})
    );

    // A malformed boolean is FastAPI's 422, not a 400.
    let (status, body) = get(
        route(&db, operator(None)),
        "/licenses/admin/all?include_deleted=maybe",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["detail"][0]["loc"],
        json!(["query", "include_deleted"])
    );
    assert_eq!(body["detail"][0]["type"], "bool_parsing");
}

#[tokio::test]
async fn the_budget_patch_writes_commits_and_returns_the_enriched_licence() {
    let db = seeded().await;

    let (status, body) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 5000, "reason": "cap"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The full enriched LicenseResponse, app AND plan populated.
    assert_eq!(body.as_object().expect("object").len(), 22);
    assert_eq!(body["id"], L1.to_string());
    assert_eq!(body["token_budget_override"], 5000);
    assert_eq!(body["app"]["key"], "demo");
    assert_eq!(body["plan"]["code"], "basic");
    assert_eq!(body["effective_status"], "active");
    // The write advanced updated_at past the seeded created_at.
    let created: DateTime<Utc> = body["created_at"]
        .as_str()
        .expect("created_at")
        .parse()
        .expect("rfc3339");
    let updated: DateTime<Utc> = body["updated_at"]
        .as_str()
        .expect("updated_at")
        .parse()
        .expect("rfc3339");
    assert!(updated > created, "{updated} <= {created}");

    // Committed, not just returned.
    let stored: Option<i32> =
        sqlx::query_scalar("SELECT token_budget_override FROM license WHERE id = $1")
            .bind(L1)
            .fetch_one(&db.pool)
            .await
            .expect("read back");
    assert_eq!(stored, Some(5000));

    // Setting the SAME value is a no-op in the reference (SQLAlchemy's dirty-check), so
    // updated_at must not advance — it is part of the response.
    let (status, same) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 5000})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{same}");
    assert_eq!(same["updated_at"], body["updated_at"]);

    // `null` CLEARS the override and returns the licence to the plan's budget.
    let (status, cleared) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert!(cleared["token_budget_override"].is_null());

    // A soft-deleted licence is still reachable by id: the reference's `get_license` and
    // the UPDATE carry no `deleted_at` arm.
    let (status, deleted) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        &format!("/licenses/{L5}/budget"),
        Some(json!({"token_budget_override": 7})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["token_budget_override"], 7);
}

#[tokio::test]
async fn the_budget_patch_refusals_are_the_references() {
    let db = seeded().await;

    // `require_role("license-admin")` first, in the reference's exact wording.
    let (status, body) = send(
        route(&db, reader(TENANT)),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));

    // An unknown licence is 404 even for an owner that would fail the ownership rule.
    let (status, body) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        &format!("/licenses/{}/budget", Uuid::nil()),
        Some(json!({"token_budget_override": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "License not found"}));

    // A client that is not a registered owning app.
    let (status, body) = send(
        route(&db, user(&["license-admin"], "s", Some(TENANT), None)),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 999})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "unknown_api_client");

    // owner-b does not issue APP: nothing written.
    let (status, body) = send(
        route(&db, operator(Some("owner-b"))),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 999})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_issuable_by_caller");

    // owner-a issues APP3 but neither owns nor is granted TENANT2.
    let (status, body) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        &format!("/licenses/{L7}/budget"),
        Some(json!({"token_budget_override": 999})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_not_accessible");

    // Refusals wrote nothing.
    let stored: Option<i32> =
        sqlx::query_scalar("SELECT token_budget_override FROM license WHERE id = $1")
            .bind(L1)
            .fetch_one(&db.pool)
            .await
            .expect("read back");
    assert_eq!(stored, None);

    // `cross_owner` bypasses the two ownership locks, audited.
    let mut crossing = user(
        &["license-admin"],
        "break-glass",
        Some(TENANT),
        Some("stranger"),
    );
    crossing.cross_owner = true;
    let (status, body) = send(
        route(&db, crossing),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 42})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["token_budget_override"], 42);
}

#[tokio::test]
async fn the_budget_body_and_path_validation_is_fastapis_shape() {
    let db = seeded().await;

    // A malformed path UUID is a typed path parameter: FastAPI's 422, not the listing's 400.
    let (status, body) = send(
        route(&db, operator(Some("owner-a"))),
        Method::PATCH,
        "/licenses/not-a-uuid/budget",
        Some(json!({"token_budget_override": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "uuid_parsing");
    assert_eq!(body["detail"][0]["loc"], json!(["path", "license_id"]));

    for (payload, entry_type, loc) in [
        // An omitted key is a 422, never a clear.
        (
            json!({}),
            "missing",
            json!(["body", "token_budget_override"]),
        ),
        (
            json!({"token_budget_override": -1}),
            "greater_than_equal",
            json!(["body", "token_budget_override"]),
        ),
        (
            json!({"token_budget_override": "many"}),
            "int_type",
            json!(["body", "token_budget_override"]),
        ),
        (
            json!({"token_budget_override": 1, "reason": 5}),
            "string_type",
            json!(["body", "reason"]),
        ),
        (json!([]), "model_attributes_type", json!(["body"])),
    ] {
        let (status, body) = send(
            route(&db, operator(Some("owner-a"))),
            Method::PATCH,
            &format!("/licenses/{L1}/budget"),
            Some(payload.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{payload}: {body}"
        );
        assert_eq!(body["detail"][0]["type"], entry_type, "{payload}: {body}");
        assert_eq!(body["detail"][0]["loc"], loc, "{payload}: {body}");
    }

    // Nothing was written by any of the invalid requests.
    let stored: Option<i32> =
        sqlx::query_scalar("SELECT token_budget_override FROM license WHERE id = $1")
            .bind(L1)
            .fetch_one(&db.pool)
            .await
            .expect("read back");
    assert_eq!(stored, None);
}

#[tokio::test]
async fn a_budget_change_pushes_the_new_effective_value() {
    let db = seeded().await;
    let stub = BillingStub::new();
    let base = start_billing_stub(stub.clone()).await;

    let (status, body) = send(
        route_with_billing(&db, operator(Some("owner-a")), &base),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 5000, "reason": "cap"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["token_budget_override"], 5000);

    let calls = wait_for_billing(&stub, |calls| {
        calls.iter().filter(|call| call.path == "/api/budget").count() == 1
    })
    .await;
    let push = calls
        .iter()
        .find(|call| call.path == "/api/budget")
        .expect("the budget push");
    assert_eq!(push.method, "POST");
    assert_eq!(push.tenant, Some(TENANT.to_string()));
    assert_eq!(
        push.body,
        Some(json!({
            "tenant_id": TENANT.to_string(),
            "license_id": L1.to_string(),
            "budget_tokens": 5000,
            "period": "monthly",
            "source": "license_sync",
        }))
    );

    // The SAME value again: the reference's push is unconditional — the dirty-check makes the
    // UPDATE a no-op, not the POST — so a repeat PATCH pushes again.
    let (status, same) = send(
        route_with_billing(&db, operator(Some("owner-a")), &base),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 5000})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{same}");
    let calls = wait_for_billing(&stub, |calls| {
        calls.iter().filter(|call| call.path == "/api/budget").count() == 2
    })
    .await;
    let pushes: Vec<&BillingCall> = calls
        .iter()
        .filter(|call| call.path == "/api/budget")
        .collect();
    assert_eq!(pushes[1].body, pushes[0].body);
}

#[tokio::test]
async fn a_cleared_override_with_no_plan_budget_tears_the_cap_down() {
    let db = seeded().await;
    // L1's plan carries NO token budget, so clearing the override leaves the licence unlimited.
    sqlx::query("UPDATE plan SET token_budget_monthly = NULL WHERE id = $1")
        .bind(PLAN)
        .execute(&db.pool)
        .await
        .expect("clear the plan budget");
    let stub = BillingStub::new();
    // Only the active `license_sync` row for L1 is the teardown's business.
    stub.set_rows(vec![
        json!({"id": "budget-l1", "source": "license_sync", "active": true, "license_id": L1.to_string()}),
        json!({"id": "budget-manual", "source": "manual", "active": true, "license_id": L1.to_string()}),
    ]);
    let base = start_billing_stub(stub.clone()).await;

    // Effective `None` is the reference's teardown leg.
    let (status, body) = send(
        route_with_billing(&db, operator(Some("owner-a")), &base),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["token_budget_override"].is_null());

    let calls = wait_for_billing(&stub, |calls| {
        calls.iter().any(|call| call.method == "DELETE")
    })
    .await;
    let list = calls
        .iter()
        .find(|call| call.method == "GET")
        .expect("the budget list read");
    assert_eq!(list.path, format!("/api/budget/list/{TENANT}"));
    assert_eq!(list.tenant, Some(TENANT.to_string()));
    let delete = calls
        .iter()
        .find(|call| call.method == "DELETE")
        .expect("the teardown");
    assert_eq!(delete.path, "/api/budget/budget-l1");
    assert_eq!(delete.tenant, Some(TENANT.to_string()));
    assert!(
        calls.iter().all(|call| call.path != "/api/budget"),
        "an unlimited licence owes no push: {calls:?}"
    );

    // The NEGATIVE effective value shares that leg: a route body cannot carry one (422), so a
    // negative PLAN value is the only way this route sees it — it must be torn down, never
    // pushed as a negative cap.
    sqlx::query("UPDATE plan SET token_budget_monthly = -5 WHERE id = $1")
        .bind(PLAN)
        .execute(&db.pool)
        .await
        .expect("a negative plan budget");
    let (status, body) = send(
        route_with_billing(&db, operator(Some("owner-a")), &base),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let calls = wait_for_billing(&stub, |calls| {
        calls
            .iter()
            .filter(|call| call.method == "DELETE")
            .count()
            == 2
    })
    .await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.path == "/api/budget")
            .count(),
        0,
        "a negative budget must never be pushed: {calls:?}"
    );
}

#[tokio::test]
async fn a_billing_failure_leaves_the_response_unchanged_and_clears_the_verified_stamp() {
    let db = seeded().await;
    sqlx::query("UPDATE license SET metering_verified_at = '2026-05-01T00:00:00Z' WHERE id = $1")
        .bind(L1)
        .execute(&db.pool)
        .await
        .expect("seed the verified stamp");
    let stub = BillingStub::new();
    stub.set_budget_status(500);
    let base = start_billing_stub(stub.clone()).await;

    let (status, body) = send(
        route_with_billing(&db, operator(Some("owner-a")), &base),
        Method::PATCH,
        &format!("/licenses/{L1}/budget"),
        Some(json!({"token_budget_override": 6000, "reason": "raise"})),
    )
    .await;
    // The failure contract: the edit succeeded and the response is untouched — the push runs
    // after the response and can never fail it.
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["token_budget_override"], 6000);
    assert_eq!(body["metering_verified_at"], "2026-05-01T00:00:00+00:00");

    // A 5xx is retried exactly once, like every other billing push in this service.
    let calls = wait_for_billing(&stub, |calls| calls.len() >= 2).await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.path == "/api/budget")
            .count(),
        2,
        "{calls:?}"
    );

    // The reconcile sweep's repair signal.
    assert!(
        wait_for_verified_cleared(&db, L1).await,
        "a failed push must clear metering_verified_at"
    );
    let stored: Option<i32> =
        sqlx::query_scalar("SELECT token_budget_override FROM license WHERE id = $1")
            .bind(L1)
            .fetch_one(&db.pool)
            .await
            .expect("read back");
    assert_eq!(
        stored,
        Some(6000),
        "the edit itself is committed and must not be rolled back by the push"
    );
}
