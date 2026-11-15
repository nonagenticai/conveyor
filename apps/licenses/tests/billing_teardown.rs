//! billing's two mirrors of a revoked licence, torn down by the revoke route.
//!
//! The contract under test is the reference's (`licenses/services/metering_teardown.py`):
//! after the revocation commits, quota + budget teardowns are spawned for the licence AND for
//! every dependency licence the revoke released; nothing is awaited; a billing failure changes
//! neither the status nor the body; and the budget list is filtered to active `license_sync`
//! rows for exactly this licence.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, State as AxumState};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post as route_post};
use axum::{Extension, Json, Router};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::BillingTeardown;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::tokens::revoke;
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0003);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0004);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0005);
const DEP_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0006);
const DEP_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0007);
const DEP_LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0008);
const TOKEN: Uuid = Uuid::from_u128(0x0000_0000_0000_7100_8000_0000_0000_0009);
const RAW: &str = "billing-teardown-token";

// ---------------------------------------------------------------------------------------------
// The billing stub: it records every call and answers with configured statuses.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    method: &'static str,
    path: String,
    tenant: Option<String>,
}

#[derive(Clone)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
    rows: Arc<Mutex<Vec<Value>>>,
    quota_status: Arc<Mutex<u16>>,
    list_status: Arc<Mutex<u16>>,
    budget_delete_status: Arc<Mutex<u16>>,
}

impl Stub {
    fn new(rows: Vec<Value>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            rows: Arc::new(Mutex::new(rows)),
            quota_status: Arc::new(Mutex::new(200)),
            list_status: Arc::new(Mutex::new(200)),
            budget_delete_status: Arc::new(Mutex::new(200)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }

    fn set_quota_status(&self, status: u16) {
        *self.quota_status.lock().expect("stub lock") = status;
    }

    fn set_list_status(&self, status: u16) {
        *self.list_status.lock().expect("stub lock") = status;
    }

    fn set_budget_delete_status(&self, status: u16) {
        *self.budget_delete_status.lock().expect("stub lock") = status;
    }
}

fn record(stub: &Stub, method: &'static str, path: String, headers: &HeaderMap) {
    let tenant = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    stub.calls.lock().expect("stub lock").push(Call {
        method,
        path,
        tenant,
    });
}

async fn quota_delete(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> StatusCode {
    record(
        &stub,
        "DELETE",
        format!("/api/quota/config/{license_id}"),
        &headers,
    );
    StatusCode::from_u16(*stub.quota_status.lock().expect("stub lock")).expect("status")
}

async fn budget_list(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Path(tenant): Path<String>,
) -> axum::response::Response {
    record(&stub, "GET", format!("/api/budget/list/{tenant}"), &headers);
    let status = *stub.list_status.lock().expect("stub lock");
    if status != 200 {
        return StatusCode::from_u16(status)
            .expect("status")
            .into_response();
    }
    let rows = stub.rows.lock().expect("stub lock").clone();
    Json(json!({"data": rows})).into_response()
}

async fn budget_delete(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Path(budget_id): Path<String>,
) -> StatusCode {
    record(
        &stub,
        "DELETE",
        format!("/api/budget/{budget_id}"),
        &headers,
    );
    StatusCode::from_u16(*stub.budget_delete_status.lock().expect("stub lock")).expect("status")
}

/// Start the stub on an ephemeral port; the task lives as long as the test's runtime.
async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/quota/config/{license_id}", delete(quota_delete))
        .route("/api/budget/list/{tenant_id}", get(budget_list))
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

// ---------------------------------------------------------------------------------------------
// The service under test.
// ---------------------------------------------------------------------------------------------

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

fn state(pool: sqlx::PgPool, billing: Option<Arc<BillingTeardown>>) -> AppState {
    let state = AppState::new(
        pool,
        config(),
        JwtVerifier::new(Vec::new(), Arc::new(UnavailableJwks)),
    );
    match billing {
        Some(billing) => state.with_billing(billing),
        None => state,
    }
}

fn revoke_app(
    db: &common::TestDb,
    user: User,
    billing: Option<Arc<BillingTeardown>>,
) -> axum::Router {
    Router::new()
        .route("/license/revoke", route_post(revoke))
        .layer(Extension(user))
        .with_state(state(db.pool.clone(), billing))
}

fn admin(tenant: Uuid) -> User {
    User {
        sub: "0195f48b-d000-7000-8000-0000000000aa".to_owned(),
        preferred_username: "operator".to_owned(),
        realm_roles: vec!["license-admin".to_owned()],
        tenant_id: Some(tenant.to_string()),
        owner_key: Some("owner-a".to_owned()),
        ..User::default()
    }
}

async fn post(app: axum::Router, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::post("/license/revoke")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// One tenant, one active licence on app `demo`, and a DEPENDENCY licence on app `dep` that
/// `demo` requires — so revoking `demo` releases the dependency and both owe teardowns.
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
        "INSERT INTO app (id, key, name, created_at) VALUES ($1, 'dep', 'Dependency', now())",
    )
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("dep app");
    sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, 'owner-a')")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("app_issuer");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, 'basic', 'Basic', now())",
    )
    .bind(PLAN)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("plan");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, 'dep', 'Dependency', now())",
    )
    .bind(DEP_PLAN)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("dep plan");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, '0195f48b-d000-7000-8000-000000000002', 'portal', now())",
    )
    .bind(SUBJECT)
    .execute(&db.pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id)
         VALUES ($1, $2, $3, $4, 'active', now(), $5)",
    )
    .bind(LICENSE)
    .bind(SUBJECT)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("license");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id, is_dependency)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, true)",
    )
    .bind(DEP_LICENSE)
    .bind(SUBJECT)
    .bind(DEP_APP)
    .bind(DEP_PLAN)
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("dep license");
    // `demo` requires `dep`: revoking demo leaves dep required by nothing, so it is released.
    sqlx::query(
        "INSERT INTO app_requires (app_id, required_app_id, caller_owner_key)
         VALUES ($1, $2, 'owner-a')",
    )
    .bind(APP)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("app_requires");
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at)
         VALUES ($1, $2, $3, 'api', now())",
    )
    .bind(TOKEN)
    .bind(LICENSE)
    .bind(db::tokens::hash_token(RAW))
    .execute(&db.pool)
    .await
    .expect("token");
    db
}

/// Budget rows the stub answers with: two the teardown must delete, three it must not.
fn budget_rows() -> Vec<Value> {
    vec![
        json!({"id": "budget-lic", "source": "license_sync", "active": true, "license_id": LICENSE.to_string()}),
        json!({"id": "budget-dep", "source": "license_sync", "active": true, "license_id": DEP_LICENSE.to_string()}),
        json!({"id": "budget-manual", "source": "manual", "active": true, "license_id": LICENSE.to_string()}),
        json!({"id": "budget-topup", "source": "topup", "active": true, "license_id": LICENSE.to_string()}),
        json!({"id": "budget-inactive", "source": "license_sync", "active": false, "license_id": LICENSE.to_string()}),
        json!({"id": "budget-other", "source": "license_sync", "active": true, "license_id": Uuid::nil().to_string()}),
    ]
}

/// Poll the stub until `predicate` holds (the teardowns are spawned, so they complete after the
/// response).
async fn wait_for(stub: &Stub, predicate: impl Fn(&[Call]) -> bool) -> Vec<Call> {
    for _ in 0..100 {
        let calls = stub.calls();
        if predicate(&calls) {
            return calls;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let calls = stub.calls();
    panic!("teardown calls did not arrive; seen: {calls:?}");
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn revoke_spawns_both_teardowns_for_the_licence_and_each_released_dependency() {
    let db = seeded().await;
    let stub = Stub::new(budget_rows());
    let base = start_stub(stub.clone()).await;
    let billing = Arc::new(BillingTeardown::new(&base, None).expect("billing client"));

    let (status, body) = post(
        revoke_app(&db, admin(TENANT), Some(billing)),
        json!({"license_id": LICENSE.to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);
    assert_eq!(
        body["released_dependency_ids"],
        json!([DEP_LICENSE.to_string()])
    );

    let calls = wait_for(&stub, |calls| {
        calls.iter().filter(|call| call.method == "DELETE").count() >= 4
    })
    .await;

    // Quota: the licence AND the released dependency.
    for id in [LICENSE.to_string(), DEP_LICENSE.to_string()] {
        assert!(
            calls
                .iter()
                .any(|call| call.method == "DELETE"
                    && call.path == format!("/api/quota/config/{id}")),
            "no quota teardown for {id}: {calls:?}"
        );
    }
    // Budget: exactly the two active license_sync rows — never manual, topup, inactive or another
    // licence's row.
    for id in ["budget-lic", "budget-dep"] {
        assert!(
            calls
                .iter()
                .any(|call| call.method == "DELETE" && call.path == format!("/api/budget/{id}")),
            "no budget teardown for {id}: {calls:?}"
        );
    }
    for id in [
        "budget-manual",
        "budget-topup",
        "budget-inactive",
        "budget-other",
    ] {
        assert!(
            !calls
                .iter()
                .any(|call| call.method == "DELETE" && call.path == format!("/api/budget/{id}")),
            "tore down a row it must not: {id}"
        );
    }
    // Every call carries the tenant the caller proved it holds.
    let tenant_header = TENANT.to_string();
    assert!(
        calls
            .iter()
            .all(|call| call.tenant.as_deref() == Some(tenant_header.as_str())),
        "a billing call was sent without the caller's tenant: {calls:?}"
    );
}

#[tokio::test]
async fn billing_failure_changes_neither_the_status_nor_the_body() {
    let db = seeded().await;
    let stub = Stub::new(budget_rows());
    stub.set_quota_status(503);
    stub.set_list_status(503);
    let base = start_stub(stub.clone()).await;
    let billing = Arc::new(BillingTeardown::new(&base, None).expect("billing client"));

    let (status, body) = post(
        revoke_app(&db, admin(TENANT), Some(billing)),
        json!({"license_id": LICENSE.to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);

    // The revocation itself is committed regardless.
    let licence_status: String = sqlx::query_scalar("SELECT status FROM license WHERE id = $1")
        .bind(LICENSE)
        .fetch_one(&db.pool)
        .await
        .expect("licence state");
    assert_eq!(licence_status, "revoked");

    // Each of the four tasks was attempted TWICE: one retry on a retryable status. The wait
    // must name the whole condition — waiting on the first half and then asserting the second
    // is a race, and it failed the full-suite run after passing in isolation.
    let quota = |id: Uuid| format!("/api/quota/config/{id}");
    let calls = wait_for(&stub, |calls| {
        let count = |path: &str| calls.iter().filter(|call| call.path == path).count();
        count(&quota(LICENSE)) >= 2
            && count(&quota(DEP_LICENSE)) >= 2
            // One list per licence, each retried once.
            && count(&format!("/api/budget/list/{TENANT}")) >= 4
    })
    .await;

    let count = |path: &str| calls.iter().filter(|call| call.path == path).count();
    assert_eq!(
        count(&quota(LICENSE)),
        2,
        "the licence's quota teardown was not retried exactly once: {calls:?}"
    );
    assert_eq!(
        count(&format!("/api/budget/list/{TENANT}")),
        4,
        "each licence's budget list was not retried exactly once: {calls:?}"
    );
}

#[tokio::test]
async fn a_missing_billing_client_leaves_the_revoke_unchanged() {
    let db = seeded().await;
    let (status, body) = post(
        revoke_app(&db, admin(TENANT), None),
        json!({"license_id": LICENSE.to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);
}

#[tokio::test]
async fn a_404_on_a_budget_delete_counts_as_success() {
    let stub = Stub::new(budget_rows());
    stub.set_budget_delete_status(404);
    let base = start_stub(stub.clone()).await;
    let billing = BillingTeardown::new(&base, None).expect("billing client");

    let ok = billing
        .teardown_budget(&TENANT.to_string(), &LICENSE.to_string())
        .await;
    assert!(ok, "billing's idempotent delete answered 404");

    let calls = stub.calls();
    assert!(
        calls
            .iter()
            .any(|call| call.path == "/api/budget/budget-lic"),
        "the matching row was not deleted: {calls:?}"
    );
}

#[tokio::test]
async fn a_404_on_a_quota_delete_is_residue_not_success() {
    // Unlike billing's idempotent budget delete, the quota route has no "already gone" answer:
    // its delete says 200 `{deleted:"0"}` when no row existed, so a 404 is a version skew.
    let stub = Stub::new(Vec::new());
    stub.set_quota_status(404);
    let base = start_stub(stub.clone()).await;
    let billing = BillingTeardown::new(&base, None).expect("billing client");

    let ok = billing
        .teardown_quota(&TENANT.to_string(), &LICENSE.to_string())
        .await;
    assert!(!ok, "a 404 on the quota route must not read as success");
}

#[tokio::test]
async fn an_unreadable_budget_list_is_a_failure_not_an_empty_success() {
    let stub = Stub::new(budget_rows());
    stub.set_list_status(503);
    let base = start_stub(stub.clone()).await;
    let billing = BillingTeardown::new(&base, None).expect("billing client");

    let ok = billing
        .teardown_budget(&TENANT.to_string(), &LICENSE.to_string())
        .await;
    assert!(
        !ok,
        "an unanswered list must not read as 'nothing to tear down'"
    );
}
