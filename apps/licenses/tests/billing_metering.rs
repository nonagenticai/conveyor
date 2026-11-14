//! Provisioning a new licence's metering in billing, against a stub HTTP server.
//!
//! The contract under test is the reference's (`licenses/main.py` `_provision_budget`): the quota
//! is always pushed; the token budget only for a tenant whose budget is not money (and never for
//! billing's sentinel tenant); a licence override beats the plan and `0` is a real BLOCK-ALL; a
//! plan owing nothing counts complete; and every failure returns the typed marker while leaving
//! `metering_provisioned_at` NULL for the reconcile sweep.
//!
//! The stub records the path, the `X-Tenant-Id` and `Authorization` headers, and the JSON body of
//! every push. It also serves the service-account token endpoint, so the bearer assertion exercises
//! the real `ClientCredentials` mint rather than a hand-built header.

mod common;

use std::sync::{Arc, Mutex};

use axum::extract::State as AxumState;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post as route_post;
use axum::{Json, Router};
use conveyor_auth::oauth::ClientCredentials;
use conveyor_licenses::billing::metering::{
    METERING_FAILURE_MARKER, METERING_RECONCILE_PATH, MeteringProvisioner,
};
use conveyor_licenses::db;
use serde_json::{Value, json};
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0003);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0004);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0005);
const ABSENT_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0006);

// ---------------------------------------------------------------------------------------------
// The billing stub: records every push and answers with configured statuses.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Call {
    method: &'static str,
    path: String,
    tenant: Option<String>,
    authorization: Option<String>,
    body: Option<Value>,
}

#[derive(Clone)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
    budget_status: Arc<Mutex<u16>>,
    quota_status: Arc<Mutex<u16>>,
}

impl Stub {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            budget_status: Arc::new(Mutex::new(200)),
            quota_status: Arc::new(Mutex::new(201)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }

    fn set_budget_status(&self, status: u16) {
        *self.budget_status.lock().expect("stub lock") = status;
    }
}

fn record(stub: &Stub, path: &str, headers: &HeaderMap, body: Value) {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    stub.calls.lock().expect("stub lock").push(Call {
        method: "POST",
        path: path.to_owned(),
        tenant: header("x-tenant-id"),
        authorization: header("authorization"),
        body: Some(body),
    });
}

async fn budget_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record(&stub, "/api/budget", &headers, body);
    StatusCode::from_u16(*stub.budget_status.lock().expect("stub lock")).expect("status")
}

async fn quota_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record(&stub, "/api/quota/config", &headers, body);
    StatusCode::from_u16(*stub.quota_status.lock().expect("stub lock")).expect("status")
}

/// The service-account token endpoint `ClientCredentials` mints against.
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

fn calls_at<'a>(calls: &'a [Call], path: &str) -> Vec<&'a Call> {
    calls.iter().filter(|call| call.path == path).collect()
}

// ---------------------------------------------------------------------------------------------
// Seed
// ---------------------------------------------------------------------------------------------

/// One tenant, one app, one plan and one active licence. `currency` decides the money-tenant leg;
/// the plan values and the licence overrides are the metering inputs under test.
async fn seeded(
    currency: Option<&str>,
    plan_budget: Option<i32>,
    plan_quota: Option<i32>,
    licence_budget: Option<i32>,
    licence_quota: Option<i32>,
) -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-a', 'Owner A')")
        .execute(&db.pool)
        .await
        .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode, budget_currency)
         VALUES ($1, 'tenant-a', now(), 'owner-a', 'local_allowed', $2::character(3))",
    )
    .bind(TENANT)
    .bind(currency)
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
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, '0195f48b-d000-7000-8000-000000000002', 'portal', now())",
    )
    .bind(SUBJECT)
    .execute(&db.pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (
             id, subject_id, app_id, plan_id, status, starts_at, tenant_id,
             token_budget_override, quota_override
         ) VALUES ($1, $2, $3, $4, 'active', now(), $5, $6, $7)",
    )
    .bind(LICENSE)
    .bind(SUBJECT)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .bind(licence_budget)
    .bind(licence_quota)
    .execute(&db.pool)
    .await
    .expect("license");
    db
}

async fn is_stamped(db: &common::TestDb) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT metering_provisioned_at IS NOT NULL FROM license WHERE id = $1",
    )
    .bind(LICENSE)
    .fetch_one(&db.pool)
    .await
    .expect("stamp state")
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_money_tenant_gets_the_quota_and_no_token_budget() {
    let db = seeded(Some("EUR"), Some(10_000), Some(500), None, None).await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, PLAN, Some(APP))
        .await
        .expect("provisioned");

    let calls = stub.calls();
    let quotas = calls_at(&calls, "/api/quota/config");
    assert_eq!(quotas.len(), 1, "{calls:?}");
    assert!(
        calls_at(&calls, "/api/budget").is_empty(),
        "a money tenant owes no token budget: {calls:?}"
    );

    let quota = quotas[0];
    assert_eq!(quota.tenant, Some(TENANT.to_string()));
    assert_eq!(quota.authorization.as_deref(), Some("Bearer stub-token"));
    assert_eq!(
        quota.body,
        Some(json!({
            "tenant_id": TENANT.to_string(),
            "license_id": LICENSE.to_string(),
            "quota_monthly": 500,
            "app_id": APP.to_string(),
            "app_name": "Demo",
        }))
    );
    assert!(
        is_stamped(&db).await,
        "the only owed push was accepted, so the licence is complete"
    );
}

#[tokio::test]
async fn a_null_currency_pushes_the_quota_and_the_token_budget() {
    let db = seeded(None, Some(10_000), Some(500), None, None).await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, PLAN, Some(APP))
        .await
        .expect("provisioned");

    let calls = stub.calls();
    let budgets = calls_at(&calls, "/api/budget");
    let quotas = calls_at(&calls, "/api/quota/config");
    assert_eq!(budgets.len(), 1, "{calls:?}");
    assert_eq!(quotas.len(), 1, "{calls:?}");

    let budget = budgets[0];
    assert_eq!(budget.method, "POST");
    assert_eq!(budget.tenant, Some(TENANT.to_string()));
    assert_eq!(budget.authorization.as_deref(), Some("Bearer stub-token"));
    assert_eq!(
        budget.body,
        Some(json!({
            "tenant_id": TENANT.to_string(),
            "license_id": LICENSE.to_string(),
            "budget_tokens": 10_000,
            "period": "monthly",
            "source": "license_sync",
        }))
    );

    let quota = quotas[0];
    assert_eq!(quota.tenant, Some(TENANT.to_string()));
    assert_eq!(quota.authorization.as_deref(), Some("Bearer stub-token"));
    assert_eq!(
        quota.body,
        Some(json!({
            "tenant_id": TENANT.to_string(),
            "license_id": LICENSE.to_string(),
            "quota_monthly": 500,
            "app_id": APP.to_string(),
            "app_name": "Demo",
        }))
    );
    assert!(is_stamped(&db).await);
}

#[tokio::test]
async fn licence_overrides_replace_the_plan_values() {
    let db = seeded(None, Some(1_000), Some(100), Some(70), Some(7)).await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, PLAN, Some(APP))
        .await
        .expect("provisioned");

    let calls = stub.calls();
    let budgets = calls_at(&calls, "/api/budget");
    let quotas = calls_at(&calls, "/api/quota/config");
    assert_eq!(budgets.len(), 1, "{calls:?}");
    assert_eq!(quotas.len(), 1, "{calls:?}");
    assert_eq!(
        budgets[0].body.as_ref().expect("budget body")["budget_tokens"],
        json!(70),
        "the licence's stored override must beat the plan"
    );
    assert_eq!(
        quotas[0].body.as_ref().expect("quota body")["quota_monthly"],
        json!(7),
        "the licence's stored override must beat the plan"
    );
    assert!(is_stamped(&db).await);
}

#[tokio::test]
async fn zero_is_a_block_all_value_not_an_absence() {
    // The reference had a documented bug where a falsy 0 was skipped, silently turning a
    // block-all licence into an unlimited one. NULL means unlimited; 0 must be pushed.
    let db = seeded(None, Some(0), Some(0), None, None).await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, PLAN, Some(APP))
        .await
        .expect("provisioned");

    let calls = stub.calls();
    let budgets = calls_at(&calls, "/api/budget");
    let quotas = calls_at(&calls, "/api/quota/config");
    assert_eq!(budgets.len(), 1, "{calls:?}");
    assert_eq!(quotas.len(), 1, "{calls:?}");
    assert_eq!(
        budgets[0].body.as_ref().expect("budget body")["budget_tokens"],
        json!(0)
    );
    assert_eq!(
        quotas[0].body.as_ref().expect("quota body")["quota_monthly"],
        json!(0)
    );
    assert!(is_stamped(&db).await);
}

#[tokio::test]
async fn a_plan_that_owes_nothing_counts_complete_and_is_stamped() {
    let db = seeded(None, None, None, None, None).await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, PLAN, None)
        .await
        .expect("nothing owed is complete");

    let calls = stub.calls();
    assert!(
        calls_at(&calls, "/api/budget").is_empty(),
        "{calls:?}"
    );
    assert!(
        calls_at(&calls, "/api/quota/config").is_empty(),
        "{calls:?}"
    );
    assert!(
        is_stamped(&db).await,
        "a plan owing nothing must be stamped, or the NULL stamp loses its meaning"
    );
}

#[tokio::test]
async fn a_billing_500_returns_the_marker_and_leaves_the_licence_unstamped() {
    let db = seeded(None, Some(10_000), Some(500), None, None).await;
    let stub = Stub::new();
    stub.set_budget_status(500);
    let base = start_stub(stub.clone()).await;

    let error = provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, PLAN, Some(APP))
        .await
        .expect_err("a billing 500 must fail the call");

    assert_eq!(error.marker, METERING_FAILURE_MARKER);
    assert!(error.to_string().contains(METERING_FAILURE_MARKER));
    assert!(error.to_string().contains(METERING_RECONCILE_PATH));

    let calls = stub.calls();
    assert_eq!(
        calls_at(&calls, "/api/budget").len(),
        2,
        "a 5xx is retried exactly once: {calls:?}"
    );
    assert_eq!(
        calls_at(&calls, "/api/quota/config").len(),
        1,
        "a failed token budget must not suppress the quota push: {calls:?}"
    );
    assert!(
        !is_stamped(&db).await,
        "a half-success must leave the licence repairable by the sweep"
    );
}

#[tokio::test]
async fn a_missing_plan_row_fails_without_any_http_call() {
    let db = seeded(None, Some(10_000), Some(500), None, None).await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    let error = provisioner(&base)
        .provision(&db.pool, LICENSE, TENANT, ABSENT_PLAN, Some(APP))
        .await
        .expect_err("a licence whose plan row is gone can never be metered");

    assert_eq!(error.marker, METERING_FAILURE_MARKER);
    assert!(
        stub.calls().is_empty(),
        "a missing plan must produce no HTTP calls: {:?}",
        stub.calls()
    );
    assert!(!is_stamped(&db).await);
}
