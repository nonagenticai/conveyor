//! Behaviour of `conveyor-licenses reconcile-metering`, the Rust port of
//! `scripts/reconcile_metering.py`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, plus
//! a stub billing server. The knobs and the billing client are supplied through `Settings` /
//! `run_with`, never read from this process's environment, so a developer's own `RECONCILE_*`
//! exports cannot change what these tests prove.
//!
//! The reference semantics under test: the independent `live_budget_owing_licenses` count runs
//! BEFORE the sweep; both metering legs read billing before writing; a differing quota is LEFT
//! ALONE unless `force_plan_quota`; a dry run reads but writes nothing; `revalidate` re-examines
//! stamped licences; a single `RECONCILE_LICENSE_ID` narrows (never widens) the selection; a
//! billing transport failure is exit 2; and config failures keep the one-line payload shape.

mod common;

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State as AxumState};
use axum::http::HeaderMap;
use axum::routing::{get, post as route_post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::commands::reconcile_metering::{Settings, run_from, run_with};
use conveyor_licenses::db;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0003);

const LICENSE_A: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0011);
const LICENSE_B: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0012);

const ACTOR: &str = "test-reconcile";
const SWEEP_FAILED: &str = "the sweep could not complete; see the traceback on stderr";

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

fn record(stub: &Stub, method: &'static str, path: String, headers: &HeaderMap, body: Option<Value>) {
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
    record(&stub, "POST", "/api/budget".to_owned(), &headers, Some(body));
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
        .route("/api/budget", route_post(budget_push))
        .route("/api/quota/config", route_post(quota_push))
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

/// One LIVE licence (status='active', NULL expiry). `verified` seeds `metering_verified_at`;
/// the overrides are the per-licence effective values.
async fn add_license(
    db: &common::TestDb,
    id: Uuid,
    verified: Option<DateTime<Utc>>,
    budget_override: Option<i32>,
    quota_override: Option<i32>,
) {
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
             updated_at, tenant_id, token_budget_override, quota_override, metering_verified_at
         ) VALUES ($1, $2, $3, $4, 'active', now(), NULL, now(), now(), $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(subject)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .bind(budget_override)
    .bind(quota_override)
    .bind(verified)
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

/// The row billing holds for a covered licence: an ACTIVE monthly `license_sync` budget.
fn covered_budget(license_id: Uuid, tokens: i64) -> Value {
    json!({
        "id": Uuid::from_u128(license_id.as_u128() + 0x2000).to_string(),
        "license_id": license_id.to_string(),
        "source": "license_sync",
        "active": true,
        "period": "monthly",
        "budget_tokens": tokens,
    })
}

fn settings() -> Settings {
    Settings {
        limit: 500,
        revalidate: false,
        force_plan_quota: false,
        dry_run: false,
        license_id: None,
        actor: ACTOR.to_owned(),
    }
}

async fn run_with_stub(
    db: &common::TestDb,
    settings: Settings,
    stub: &Stub,
) -> conveyor_licenses::commands::RunOutcome {
    let base = start_stub(stub.clone()).await;
    run_with(Some(db.url.clone()), settings, Some(provisioner(&base))).await
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let object = value.as_object().expect("a JSON object");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort_unstable();
    keys
}

fn calls_at(stub: &Stub, method: &str, path: &str) -> Vec<Call> {
    stub.calls()
        .into_iter()
        .filter(|call| call.method == method && call.path == path)
        .collect()
}

fn failure_keys() -> Vec<String> {
    vec![
        "actor".to_owned(),
        "error".to_owned(),
        "error_kind".to_owned(),
        "event".to_owned(),
        "exit_code".to_owned(),
        "license_id".to_owned(),
    ]
}

fn payload_keys() -> Vec<String> {
    [
        "actor",
        "already_covered",
        "budget_drift_license_ids",
        "budgets_created",
        "budgets_skipped_money_tenant",
        "budgets_skipped_sentinel",
        "budgets_updated",
        "dry_run",
        "event",
        "exit_code",
        "failed",
        "failed_license_ids",
        "force_plan_quota",
        "license_id",
        "limit",
        "live_budget_owing_licenses",
        "quota_drift_license_ids",
        "quotas_already_configured",
        "quotas_overwritten",
        "quotas_pushed",
        "reason",
        "repaired",
        "repaired_license_ids",
        "revalidate",
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
async fn an_uncovered_licence_gets_both_rows_created_and_is_stamped() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    let stub = Stub::new(Vec::new(), Value::Null);

    let outcome = run_with_stub(&db, settings(), &stub).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 0);
    assert_eq!(
        sorted_keys(payload),
        payload_keys(),
        "the payload is the script's key set"
    );
    assert_eq!(payload["event"], "metering_reconcile");
    assert_eq!(payload["actor"], ACTOR);
    assert_eq!(payload["license_id"], Value::Null);
    assert_eq!(payload["live_budget_owing_licenses"], json!(1));
    assert_eq!(payload["scanned"], json!(1));
    assert_eq!(payload["repaired"], json!(1));
    assert_eq!(payload["failed"], json!(0));
    assert_eq!(payload["repaired_license_ids"], json!([LICENSE_A.to_string()]));
    assert_eq!(
        payload["uncovered_budget_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(
        payload["uncovered_quota_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(payload["budgets_created"], json!(1));
    assert_eq!(payload["quotas_pushed"], json!(1));
    assert_eq!(payload["already_covered"], json!(0));
    assert_eq!(payload["exit_code"], json!(0));
    assert_eq!(payload["reason"], "converged");

    let budget_pushes = calls_at(&stub, "POST", "/api/budget");
    assert_eq!(budget_pushes.len(), 1);
    assert_eq!(budget_pushes[0].tenant, Some(TENANT.to_string()));
    assert_eq!(
        budget_pushes[0].body.as_ref().expect("body")["budget_tokens"],
        json!(10_000)
    );
    assert_eq!(
        budget_pushes[0].body.as_ref().expect("body")["source"],
        "license_sync"
    );

    let quota_pushes = calls_at(&stub, "POST", "/api/quota/config");
    assert_eq!(quota_pushes.len(), 1);
    let body = quota_pushes[0].body.as_ref().expect("body");
    assert_eq!(body["quota_monthly"], json!(500));
    assert_eq!(body["app_id"], APP.to_string());
    assert_eq!(body["app_name"], "Demo");

    assert_eq!(calls_at(&stub, "GET", "/api/quota/check").len(), 1);
    assert_eq!(
        calls_at(&stub, "GET", &format!("/api/budget/list/{TENANT}")).len(),
        1,
        "one budget-list read per tenant"
    );
    assert!(
        verified_at(&db.pool, LICENSE_A).await.is_some(),
        "billing accepted both pushes, so the licence is stamped"
    );
}

#[tokio::test]
async fn a_differing_billing_quota_is_left_alone_but_still_converges() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    let licence = LICENSE_A.to_string();
    let stub = Stub::new(vec![covered_budget(LICENSE_A, 10_000)], json!(700));

    let outcome = run_with_stub(&db, settings(), &stub).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 0);
    assert_eq!(payload["scanned"], json!(1));
    assert_eq!(payload["repaired"], json!(1), "a deliberate override is not a failure");
    assert_eq!(payload["quota_drift_license_ids"], json!([licence]));
    assert_eq!(payload["quotas_already_configured"], json!(0));
    assert_eq!(payload["quotas_pushed"], json!(0));
    assert_eq!(payload["quotas_overwritten"], json!(0));
    assert_eq!(payload["already_covered"], json!(1));
    assert_eq!(payload["budgets_created"], json!(0));
    assert!(
        calls_at(&stub, "POST", "/api/quota/config").is_empty(),
        "the differing billing quota must not be overwritten without force_plan_quota"
    );
    assert!(calls_at(&stub, "POST", "/api/budget").is_empty());
    assert!(verified_at(&db.pool, LICENSE_A).await.is_some());
}

#[tokio::test]
async fn force_plan_quota_overwrites_a_differing_quota() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    let stub = Stub::new(vec![covered_budget(LICENSE_A, 10_000)], json!(700));
    let mut knobs = settings();
    knobs.force_plan_quota = true;

    let outcome = run_with_stub(&db, knobs, &stub).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 0);
    assert_eq!(
        payload["quota_drift_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(payload["quotas_overwritten"], json!(1));
    let pushes = calls_at(&stub, "POST", "/api/quota/config");
    assert_eq!(pushes.len(), 1);
    assert_eq!(
        pushes[0].body.as_ref().expect("body")["quota_monthly"],
        json!(500)
    );
    assert!(verified_at(&db.pool, LICENSE_A).await.is_some());
}

#[tokio::test]
async fn a_null_effective_budget_skips_the_budget_leg_entirely() {
    let db = seeded(None, Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    let stub = Stub::new(Vec::new(), Value::Null);

    let outcome = run_with_stub(&db, settings(), &stub).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 0);
    assert_eq!(payload["scanned"], json!(1));
    assert_eq!(payload["repaired"], json!(1));
    assert_eq!(payload["uncovered_budget_license_ids"], json!([]));
    assert_eq!(payload["already_covered"], json!(0));
    assert_eq!(payload["budgets_created"], json!(0));
    assert_eq!(payload["budgets_skipped_sentinel"], json!(0));
    assert_eq!(payload["budgets_skipped_money_tenant"], json!(0));
    assert!(
        calls_at(&stub, "GET", &format!("/api/budget/list/{TENANT}")).is_empty(),
        "an unlimited budget owes nothing, so billing is not even read"
    );
    assert!(calls_at(&stub, "POST", "/api/budget").is_empty());
    assert_eq!(calls_at(&stub, "POST", "/api/quota/config").len(), 1);
    assert!(verified_at(&db.pool, LICENSE_A).await.is_some());
}

#[tokio::test]
async fn revalidate_re_examines_a_stamped_licence() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, Some(Utc::now()), None, None).await;
    let stub = Stub::new(vec![covered_budget(LICENSE_A, 10_000)], json!(500));

    // Without revalidate the stamped licence is invisible, and the independent count still sees
    // it: that is the VACUOUS SWEEP the guard exists for.
    let outcome = run_with_stub(&db, settings(), &stub).await;
    assert_eq!(outcome.code, 1);
    assert_eq!(outcome.payload["scanned"], json!(0));
    assert_eq!(outcome.payload["live_budget_owing_licenses"], json!(1));
    assert!(
        outcome.payload["reason"]
            .as_str()
            .expect("reason")
            .starts_with("VACUOUS SWEEP"),
        "reason was {:?}",
        outcome.payload["reason"]
    );
    assert!(stub.calls().is_empty(), "a zero-row sweep makes no billing call");

    let mut knobs = settings();
    knobs.revalidate = true;
    let outcome = run_with_stub(&db, knobs, &stub).await;
    assert_eq!(outcome.code, 0);
    assert_eq!(outcome.payload["scanned"], json!(1));
    assert_eq!(outcome.payload["repaired"], json!(1));
    assert_eq!(outcome.payload["already_covered"], json!(1));
    assert_eq!(outcome.payload["quotas_already_configured"], json!(1));
    assert_eq!(outcome.payload["reason"], "converged");
}

#[tokio::test]
async fn a_dry_run_reads_but_pushes_nothing_and_reports_the_same_counts() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    let stub = Stub::new(Vec::new(), Value::Null);
    let mut knobs = settings();
    knobs.dry_run = true;

    let outcome = run_with_stub(&db, knobs, &stub).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 1, "a measure-only run over uncovered licences is not converged");
    assert_eq!(payload["dry_run"], json!(true));
    assert_eq!(payload["scanned"], json!(1));
    assert_eq!(
        payload["uncovered_budget_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(
        payload["uncovered_quota_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(payload["budgets_created"], json!(0));
    assert_eq!(payload["quotas_pushed"], json!(0));
    assert!(
        payload["reason"]
            .as_str()
            .expect("reason")
            .starts_with("NOT CONVERGED (measure-only)"),
        "reason was {:?}",
        payload["reason"]
    );
    assert!(
        stub.calls().iter().all(|call| call.method == "GET"),
        "a dry run does not put a write on the wire"
    );
    assert_eq!(calls_at(&stub, "GET", "/api/quota/check").len(), 1);
    assert_eq!(
        calls_at(&stub, "GET", &format!("/api/budget/list/{TENANT}")).len(),
        1
    );
    assert!(verified_at(&db.pool, LICENSE_A).await.is_none());

    // The armed run then writes exactly what the dry run reported.
    let outcome = run_with_stub(&db, settings(), &stub).await;
    assert_eq!(outcome.code, 0);
    assert_eq!(outcome.payload["budgets_created"], json!(1));
    assert_eq!(outcome.payload["quotas_pushed"], json!(1));
    assert!(verified_at(&db.pool, LICENSE_A).await.is_some());
}

#[tokio::test]
async fn a_license_id_narrows_the_sweep_and_the_independent_count() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    add_license(&db, LICENSE_B, None, None, None).await;
    let stub = Stub::new(Vec::new(), Value::Null);
    let mut knobs = settings();
    knobs.license_id = Some(LICENSE_A.to_string());

    let outcome = run_with_stub(&db, knobs, &stub).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 0);
    assert_eq!(
        payload["live_budget_owing_licenses"],
        json!(1),
        "the independent count is scoped to the canary"
    );
    assert_eq!(payload["scanned"], json!(1));
    assert_eq!(
        payload["repaired_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert!(verified_at(&db.pool, LICENSE_A).await.is_some());
    assert!(
        verified_at(&db.pool, LICENSE_B).await.is_none(),
        "the unnamed licence is never swept"
    );
    for call in calls_at(&stub, "POST", "/api/budget") {
        assert_eq!(
            call.body.as_ref().expect("body")["license_id"],
            LICENSE_A.to_string()
        );
    }
}

#[tokio::test]
async fn a_billing_transport_failure_is_exit_two_with_the_reference_reason() {
    let db = seeded(Some(10_000), Some(500)).await;
    add_license(&db, LICENSE_A, None, None, None).await;
    // Port 1 answers nothing: every read is a transport failure that survives the one retry.
    let dead = MeteringProvisioner::new("http://127.0.0.1:1", None).expect("provisioner");

    let outcome = run_with(Some(db.url.clone()), settings(), Some(dead)).await;
    let payload = &outcome.payload;

    assert_eq!(outcome.code, 2);
    assert_eq!(payload["exit_code"], json!(2));
    assert_eq!(
        payload["unknown_budget_license_ids"],
        json!([LICENSE_A.to_string()])
    );
    assert_eq!(
        payload["reason"],
        "billing would not disclose coverage for 1 licence(s); nothing was created for them and \
         their coverage remains UNKNOWN"
    );
    assert!(verified_at(&db.pool, LICENSE_A).await.is_none());
}

#[tokio::test]
async fn a_bad_environment_is_exit_two_with_the_failure_payload_shape() {
    let outcome = run_from(None, |name| match name {
        "RECONCILE_LIMIT" => Some("many".to_owned()),
        _ => None,
    })
    .await;
    assert_eq!(outcome.code, 2);
    assert_eq!(sorted_keys(&outcome.payload), failure_keys());
    assert_eq!(outcome.payload["event"], "metering_reconcile");
    assert_eq!(outcome.payload["actor"], Value::Null);
    assert_eq!(outcome.payload["license_id"], Value::Null);
    assert_eq!(outcome.payload["exit_code"], json!(2));
    assert_eq!(outcome.payload["error_kind"], "bad_reconcile_env");
    assert_eq!(
        outcome.payload["error"],
        "RECONCILE_LIMIT='many' is not an integer"
    );

    let outcome = run_from(None, |name| match name {
        "RECONCILE_LIMIT" => Some("0".to_owned()),
        _ => None,
    })
    .await;
    assert_eq!(outcome.code, 2);
    assert_eq!(
        outcome.payload["error"],
        "RECONCILE_LIMIT=0 is outside 1..5000"
    );

    let outcome = run_from(None, |name| match name {
        "RECONCILE_DRY_RUN" => Some("ture".to_owned()),
        _ => None,
    })
    .await;
    assert_eq!(outcome.code, 2);
    assert_eq!(
        outcome.payload["error"],
        "RECONCILE_DRY_RUN='ture' is not a boolean (use one of \
         ['', '0', '1', 'false', 'no', 'off', 'on', 'true', 'yes'])"
    );
}

#[tokio::test]
async fn missing_database_url_and_missing_billing_are_exit_two_with_static_errors() {
    let base = start_stub(Stub::new(Vec::new(), Value::Null)).await;

    // Missing DATABASE_URL is the reference's generic sweep failure.
    let outcome = run_with(None, settings(), Some(provisioner(&base))).await;
    assert_eq!(outcome.code, 2);
    assert_eq!(sorted_keys(&outcome.payload), failure_keys());
    assert_eq!(outcome.payload["actor"], ACTOR);
    assert_eq!(outcome.payload["error"], SWEEP_FAILED);
    assert_eq!(outcome.payload["error_kind"], "sqlx::Error");
    assert_eq!(outcome.payload["exit_code"], json!(2));

    // A missing billing client is the port's startup_config leg.
    let outcome = run_with(
        Some("postgres://unused".to_owned()),
        settings(),
        None,
    )
    .await;
    assert_eq!(outcome.code, 2);
    assert_eq!(sorted_keys(&outcome.payload), failure_keys());
    assert_eq!(outcome.payload["error_kind"], "startup_config");
    assert!(
        outcome.payload["error"]
            .as_str()
            .expect("error")
            .contains("METERING_SERVICE_URL")
    );

    // A malformed canary id fails before any sweep, never as "already fine".
    let mut knobs = settings();
    knobs.license_id = Some("not-a-uuid".to_owned());
    let outcome = run_with(Some("postgres://unused".to_owned()), knobs, Some(provisioner(&base))).await;
    assert_eq!(outcome.code, 2);
    assert_eq!(outcome.payload["license_id"], "not-a-uuid");
    assert_eq!(outcome.payload["error_kind"], "DataError");
}
