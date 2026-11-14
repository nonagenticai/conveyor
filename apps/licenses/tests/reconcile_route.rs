//! Behaviour of `POST /licenses/admin/metering/reconcile`
//! (`reconcile_metering_provisioning`), the HTTP face of the same sweep the
//! `reconcile-metering` command runs.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline,
//! plus a stub billing server reached through an `Extension<Arc<MeteringProvisioner>>` — never a
//! process environment variable, so a developer's own `METERING_SERVICE_URL` cannot change what
//! these tests prove.
//!
//! The reference semantics under test: `license-admin` is required (realm role only, its exact
//! detail); `limit` is `1..=5000` with default 500 and a non-integer is FastAPI's 422, not a 400;
//! a non-UUID `license_id` is the runner's 400 before any scan; a dry run returns the exact
//! `MeteringReconcileResult` shape and writes nothing; an armed run repairs (and stamps) one
//! licence; and `license_id` narrows the sweep rather than widening it.

mod common;

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State as AxumState};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::licence_admin::reconcile_metering_provisioning;
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4601_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4601_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4601_8000_0000_0000_0003);

const LICENSE_A: Uuid = Uuid::from_u128(0x0000_0000_0000_4601_8000_0000_0000_0011);
const LICENSE_B: Uuid = Uuid::from_u128(0x0000_0000_0000_4601_8000_0000_0000_0012);

const PATH: &str = "/licenses/admin/metering/reconcile";

// ---------------------------------------------------------------------------------------------
// The billing stub: records every call and serves the two reads plus the two pushes.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Call {
    method: &'static str,
    path: String,
    tenant: Option<String>,
    body: Option<Value>,
}

#[derive(Clone)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
    budget_rows: Arc<Mutex<Vec<Value>>>,
    quota_monthly: Arc<Mutex<Value>>,
}

impl Stub {
    fn new(budget_rows: Vec<Value>, quota_monthly: Value) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            budget_rows: Arc::new(Mutex::new(budget_rows)),
            quota_monthly: Arc::new(Mutex::new(quota_monthly)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }
}

fn record(
    stub: &Stub,
    method: &'static str,
    path: String,
    headers: &HeaderMap,
    body: Option<Value>,
) {
    let tenant = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    stub.calls.lock().expect("stub lock").push(Call {
        method,
        path,
        tenant,
        body,
    });
}

async fn budget_list(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Path(tenant): Path<String>,
) -> Json<Value> {
    record(
        &stub,
        "GET",
        format!("/api/budget/list/{tenant}"),
        &headers,
        None,
    );
    let rows = stub.budget_rows.lock().expect("stub lock").clone();
    Json(json!({"data": rows}))
}

async fn quota_check(AxumState(stub): AxumState<Stub>, headers: HeaderMap) -> Json<Value> {
    record(&stub, "GET", "/api/quota/check".to_owned(), &headers, None);
    let quota = stub.quota_monthly.lock().expect("stub lock").clone();
    Json(json!({"data": {"quota_monthly": quota}}))
}

async fn budget_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::http::StatusCode {
    record(
        &stub,
        "POST",
        "/api/budget".to_owned(),
        &headers,
        Some(body),
    );
    axum::http::StatusCode::CREATED
}

async fn quota_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::http::StatusCode {
    record(
        &stub,
        "POST",
        "/api/quota/config".to_owned(),
        &headers,
        Some(body),
    );
    axum::http::StatusCode::CREATED
}

/// Start the stub on an ephemeral port; the task lives as long as the test's runtime.
async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/budget/list/{tenant}", get(budget_list))
        .route("/api/quota/check", get(quota_check))
        .route("/api/budget", post(budget_push))
        .route("/api/quota/config", post(quota_push))
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
    MeteringProvisioner::new(base, None).expect("metering provisioner")
}

// ---------------------------------------------------------------------------------------------
// Router and requests
// ---------------------------------------------------------------------------------------------

fn user(roles: &[&str]) -> User {
    User {
        sub: "operator-sub".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        ..User::default()
    }
}

fn admin() -> User {
    user(&["license-admin"])
}

fn state(pool: PgPool) -> AppState {
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

/// The reconcile route alone, with the caller the middleware would have built and the billing
/// client the binary attaches as an extension.
fn route(db: &common::TestDb, user: User, base: &str) -> Router {
    Router::new()
        .route(PATH, post(reconcile_metering_provisioning))
        .layer(axum::Extension(user))
        .layer(axum::Extension(Arc::new(provisioner(base))))
        .with_state(state(db.pool.clone()))
}

async fn send(app: Router, target: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(target)
        .body(Body::empty())
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

// ---------------------------------------------------------------------------------------------
// Seed
// ---------------------------------------------------------------------------------------------

/// One tenant, one app and one plan. `plan_budget` / `plan_quota` are the plan's effective
/// metering values; `None` means that leg is unlimited and owes billing nothing.
async fn seeded(plan_budget: Option<i32>, plan_quota: Option<i32>) -> common::TestDb {
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
        "INSERT INTO plan (id, app_id, code, name, created_at, token_budget_monthly, quota_monthly)
         VALUES ($1, $2, 'basic', 'Basic', now(), $3, $4)",
    )
    .bind(PLAN)
    .bind(APP)
    .bind(plan_budget)
    .bind(plan_quota)
    .execute(&db.pool)
    .await
    .expect("plan");
    db
}

/// One LIVE licence (status='active', NULL expiry), unstamped.
async fn add_license(db: &common::TestDb, id: Uuid) {
    let subject = Uuid::from_u128(id.as_u128() + 0x1000);
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(subject)
    .bind(subject.to_string())
    .execute(&db.pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (
             id, subject_id, app_id, plan_id, status, starts_at, expires_at, created_at,
             updated_at, tenant_id
         ) VALUES ($1, $2, $3, $4, 'active', now(), NULL, now(), now(), $5)",
    )
    .bind(id)
    .bind(subject)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("license");
}

async fn verified_at(pool: &PgPool, id: Uuid) -> Option<DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT metering_verified_at FROM license WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("metering_verified_at")
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let object = value.as_object().expect("a JSON object");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort_unstable();
    keys
}

/// The captured `MeteringReconcileResult` property set — and NOTHING from the command payload
/// (`event`, `actor`, `limit`, `reason`, `exit_code`, `live_budget_owing_licenses`).
fn result_keys() -> Vec<String> {
    [
        "already_covered",
        "budget_drift_license_ids",
        "budgets_created",
        "budgets_skipped_money_tenant",
        "budgets_skipped_sentinel",
        "budgets_updated",
        "dry_run",
        "failed",
        "failed_license_ids",
        "quota_drift_license_ids",
        "quotas_already_configured",
        "quotas_overwritten",
        "quotas_pushed",
        "repaired",
        "repaired_license_ids",
        "scanned",
        "uncovered_budget_license_ids",
        "uncovered_quota_license_ids",
        "unknown_budget_license_ids",
        "unknown_quota_license_ids",
        "unknown_quota_reasons",
    ]
    .map(str::to_owned)
    .to_vec()
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_non_admin_is_refused_with_the_references_detail() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A).await;
    let stub = Stub::new(Vec::new(), Value::Null);
    let base = start_stub(stub.clone()).await;

    // `require_any_role("license-admin")` — realm roles only, the reference's exact wording.
    let (status, body) = send(route(&db, user(&["license-reader"]), &base), PATH).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin"})
    );
    assert!(stub.calls().is_empty(), "a refused caller sweeps nothing");
    assert!(
        verified_at(&db.pool, LICENSE_A).await.is_none(),
        "a refused caller stamps nothing"
    );
}

#[tokio::test]
async fn the_query_parameters_are_fastapis_and_the_canary_is_a_400() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A).await;
    let stub = Stub::new(Vec::new(), Value::Null);
    let base = start_stub(stub.clone()).await;

    for (query, kind) in [
        ("?limit=0", "greater_than_equal"),
        ("?limit=5001", "less_than_equal"),
        ("?limit=many", "int_parsing"),
        ("?revalidate=maybe", "bool_parsing"),
        ("?dry_run=maybe", "bool_parsing"),
        ("?force_plan_quota=maybe", "bool_parsing"),
    ] {
        let (status, body) = send(route(&db, admin(), &base), &format!("{PATH}{query}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
        assert_eq!(body["detail"][0]["type"], kind, "{query}: {body}");
        assert_eq!(body["detail"][0]["loc"][0], "query", "{query}: {body}");
    }

    // A non-UUID canary is the runner's 400 (raised BEFORE the scan), not FastAPI's 422.
    let (status, body) = send(
        route(&db, admin(), &base),
        &format!("{PATH}?license_id=not-a-uuid"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body, json!({"detail": "license_id must be a UUID"}));

    // None of the refused requests reached billing or the database.
    assert!(stub.calls().is_empty());
    assert!(verified_at(&db.pool, LICENSE_A).await.is_none());
}

#[tokio::test]
async fn a_dry_run_returns_the_exact_result_shape_and_writes_nothing() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A).await;
    let stub = Stub::new(Vec::new(), Value::Null);
    let base = start_stub(stub.clone()).await;

    let (status, body) = send(route(&db, admin(), &base), &format!("{PATH}?dry_run=true")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        sorted_keys(&body),
        result_keys(),
        "the body is `MeteringReconcileResult` and nothing else"
    );
    assert_eq!(body["dry_run"], json!(true));
    assert_eq!(body["scanned"], json!(1));
    // A dry run is measure-only: nothing is repaired, so the failed list stays empty.
    assert_eq!(body["repaired"], json!(0));
    assert_eq!(body["failed"], json!(0));
    assert_eq!(body["repaired_license_ids"], json!([]));
    assert_eq!(body["failed_license_ids"], json!([]));
    // The measurement, before any repair, is the coverage answer.
    assert_eq!(
        body["uncovered_budget_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(
        body["uncovered_quota_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    // Every defaulted counter and list is the captured default.
    for field in [
        "already_covered",
        "budgets_created",
        "budgets_skipped_money_tenant",
        "budgets_skipped_sentinel",
        "budgets_updated",
        "quotas_already_configured",
        "quotas_overwritten",
        "quotas_pushed",
    ] {
        assert_eq!(body[field], json!(0), "{field}: {body}");
    }
    for field in [
        "budget_drift_license_ids",
        "quota_drift_license_ids",
        "unknown_budget_license_ids",
        "unknown_quota_license_ids",
    ] {
        assert_eq!(body[field], json!([]), "{field}: {body}");
    }
    assert_eq!(body["unknown_quota_reasons"], json!({}));

    // Reads only, and no stamp: a dry run puts no write on the wire and writes no row.
    assert!(
        stub.calls().iter().all(|call| call.method == "GET"),
        "{:?}",
        stub.calls()
    );
    assert_eq!(stub.calls().len(), 2, "one budget list, one quota check");
    assert!(verified_at(&db.pool, LICENSE_A).await.is_none());
}

#[tokio::test]
async fn an_armed_run_repairs_and_stamps_the_named_licence_only() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A).await;
    add_license(&db, LICENSE_B).await;
    let stub = Stub::new(Vec::new(), Value::Null);
    let base = start_stub(stub.clone()).await;

    let (status, body) = send(
        route(&db, admin(), &base),
        &format!("{PATH}?license_id={LICENSE_A}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(sorted_keys(&body), result_keys());
    assert_eq!(body["dry_run"], json!(false));
    assert_eq!(
        body["scanned"],
        json!(1),
        "license_id narrows the sweep, never widens it"
    );
    assert_eq!(body["repaired"], json!(1));
    assert_eq!(body["failed"], json!(0));
    assert_eq!(body["repaired_license_ids"], json!([LICENSE_A.to_string()]));
    assert_eq!(body["budgets_created"], json!(1));
    assert_eq!(body["quotas_pushed"], json!(1));

    let pushes = stub
        .calls()
        .into_iter()
        .filter(|call| call.method == "POST")
        .collect::<Vec<_>>();
    assert_eq!(pushes.len(), 2, "one budget push, one quota push");
    for call in &pushes {
        assert_eq!(
            call.body.as_ref().expect("body")["license_id"],
            LICENSE_A.to_string(),
            "the unnamed licence is never swept"
        );
    }
    assert!(
        verified_at(&db.pool, LICENSE_A).await.is_some(),
        "billing accepted both pushes, so the licence is stamped"
    );
    assert!(
        verified_at(&db.pool, LICENSE_B).await.is_none(),
        "the unnamed licence is never swept"
    );
}

/// The lock check is the contract gate: this route closes exactly one of the captured
/// operations, so the "not registered" gap shrinks from 40 to 39, this route is never named,
/// and the operation's query parameters and result schema are the capture's byte for byte.
#[test]
fn the_lock_check_names_39_unported_operations_after_this_route() {
    use conveyor_licenses::contract::normalize::normalize;

    let captured = conveyor_licenses::contract::lock().expect("the embedded capture parses");
    let served = serde_json::to_value(conveyor_licenses::router::openapi()).expect("openapi");
    let error = conveyor_licenses::contract::check(&served, conveyor_licenses::router::ROUTES)
        .expect_err("the port is incomplete by construction");
    let message = error.to_string();
    assert!(
        !message.contains("not registered: POST /licenses/admin/metering/reconcile"),
        "{message}"
    );
    assert!(
        !message.contains("documented but not registered: POST /licenses/admin/metering/reconcile"),
        "{message}"
    );
    // The first problem is prefixed by the error's own sentence, so strip it before splitting.
    let problems = message
        .strip_prefix("the served contract drifted from the capture: ")
        .unwrap_or(&message);
    let missing = problems
        .split("; ")
        .filter(|problem| problem.starts_with("not registered: "))
        .count();
    assert_eq!(missing, 39, "{message}");

    // The operation itself: the five query parameters, the wire-typed defaults and the
    // `MeteringReconcileResult` schema must be exactly the capture's (headers and prose aside).
    let expected = normalize(&captured);
    let actual = normalize(&served);
    let expected_operation = &expected["paths"]["/licenses/admin/metering/reconcile"]["post"];
    let actual_operation = &actual["paths"]["/licenses/admin/metering/reconcile"]["post"];
    let query_params = |operation: &Value| -> Vec<Value> {
        let mut params: Vec<Value> = operation["parameters"]
            .as_array()
            .expect("parameters")
            .iter()
            .filter(|param| param["in"] == "query")
            .cloned()
            .collect();
        params.sort_by_key(Value::to_string);
        params
    };
    assert_eq!(
        query_params(expected_operation),
        query_params(actual_operation),
        "query parameter drift"
    );
    assert_eq!(
        expected_operation["responses"]["200"], actual_operation["responses"]["200"],
        "result reference drift"
    );
    assert_eq!(
        expected["components"]["MeteringReconcileResult"],
        actual["components"]["MeteringReconcileResult"],
        "MeteringReconcileResult drift"
    );
}
