//! The bundle-ensure APPLY orchestration against a real Postgres 18 and a stub billing server.
//!
//! Every write path the reference's `apply` performs: the hold and grants before any licence, the
//! insert (subject + licence + attribution + metering), the adopt compare-and-set, the shared
//! dependency licence, both lost-race outcomes, the recorded metering failure, a dry run that
//! takes no lock and writes nothing, and the tenant advisory lock (same tenant refused, another
//! tenant proceeds).

mod common;

use std::sync::{Arc, Mutex, Once};

use axum::extract::State as AxumState;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post as route_post;
use axum::{Json, Router};
use conveyor_auth::oauth::ClientCredentials;
use conveyor_licenses::billing::metering::{METERING_FAILURE_MARKER, MeteringProvisioner};
use conveyor_licenses::db;
use conveyor_licenses::db::bundle_ensure::{
    self, AppAction, AppPlan, EnsureError, EnsureErrorCode, EnsurePlan, RefuseReason,
};
use conveyor_licenses::db::bundle_ensure_apply::{self, TenantEnsureLock};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const KEY: &str = "non-agentic-ai";
const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0001);
const TENANT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0002);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0003);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0004);
const DEP_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0005);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0006);
const DEP_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0007);
const SVC: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0008);
const LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_4501_8000_0000_0000_0009);

// ---------------------------------------------------------------------------------------------
// The billing stub: records every push and answers with configured statuses.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Call {
    path: String,
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
            quota_status: Arc::new(Mutex::new(200)),
        }
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

    fn set_budget_status(&self, status: u16) {
        *self.budget_status.lock().expect("stub lock") = status;
    }
}

fn record(stub: &Stub, path: &str, body: Value) {
    stub.calls.lock().expect("stub lock").push(Call {
        path: path.to_owned(),
        body: Some(body),
    });
}

async fn budget_push(
    AxumState(stub): AxumState<Stub>,
    _headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record(&stub, "/api/budget", body);
    StatusCode::from_u16(*stub.budget_status.lock().expect("stub lock")).expect("status")
}

async fn quota_push(
    AxumState(stub): AxumState<Stub>,
    _headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record(&stub, "/api/quota/config", body);
    StatusCode::from_u16(*stub.quota_status.lock().expect("stub lock")).expect("status")
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
/// in this file calls this first, and `Once` gives the happens-before edge: the single write
/// completes before any later `std::env::var` from any other test thread.
fn short_lock_timeout() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        std::env::set_var("ENSURE_LOCK_TIMEOUT", "1s");
    });
}

/// A pool wide enough to hold TWO tenant locks plus the work each is doing.
///
/// The shared harness pool is bounded at 2 connections; a held lock occupies one for the whole
/// ensure, so the lock tests would starve their own plan/apply queries on it.
async fn roomy(db: &common::TestDb) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&db.url)
        .await
        .expect("roomy pool")
}

/// One owner (`owner-a`), two tenants, two apps with plans, and one bundle holding `demo`.
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
         VALUES ($1, 'tenant-a', now(), 'owner-a'), ($2, 'tenant-b', now(), 'owner-a')",
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
    db
}

async fn issuer(db: &common::TestDb, app: Uuid, owner: &str) {
    sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
        .bind(app)
        .bind(owner)
        .execute(&db.pool)
        .await
        .expect("app_issuer");
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

/// `(bundle_id, is_dependency, metadata, metered)` of one licence.
async fn licence_facts(db: &common::TestDb, id: Uuid) -> (Option<Uuid>, bool, Option<Value>, bool) {
    sqlx::query_as(
        "SELECT bundle_id, is_dependency, metadata, metering_provisioned_at IS NOT NULL
           FROM license WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .expect("licence facts")
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

async fn tenant_licences(db: &common::TestDb, tenant: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM license WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.pool)
        .await
        .expect("tenant licences")
}

async fn subject_id(db: &common::TestDb, keycloak_sub: &str) -> Option<Uuid> {
    sqlx::query_scalar("SELECT id FROM subject WHERE keycloak_sub = $1")
        .bind(keycloak_sub)
        .fetch_optional(&db.pool)
        .await
        .expect("subject id")
}

fn decision<'a>(plan: &'a EnsurePlan, app_key: &str) -> &'a AppPlan {
    plan.apps
        .iter()
        .find(|app| app.app_key == app_key)
        .unwrap_or_else(|| panic!("no decision for {app_key} in {:?}", plan.apps))
}

async fn ensure(
    pool: &PgPool,
    tenant: Uuid,
    apply: bool,
    meter: &MeteringProvisioner,
) -> Result<EnsurePlan, EnsureError> {
    bundle_ensure_apply::plan_and_apply(
        pool,
        tenant,
        KEY,
        &["basic".to_owned()],
        None,
        apply,
        None,
        "tester",
        "bundle_ensure",
        meter,
    )
    .await
}

/// The reference's `base_metering`: a money tenant is `quota_only`, otherwise `token_and_quota`.
fn base_metering(plan: &EnsurePlan) -> &'static str {
    if plan.budget_currency.is_some() {
        "quota_only"
    } else {
        "token_and_quota"
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_insert_path_writes_hold_grants_subject_licence_and_metering_once() {
    short_lock_timeout();
    let db = seeded().await;
    issuer(&db, APP, "owner-b").await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let ep = ensure(&db.pool, TENANT, true, &meter).await.expect("apply");
    assert!(ep.applied);
    assert!(!ep.holds);
    assert_eq!(ep.grants_created, vec!["owner-b".to_owned()]);
    assert!(ep.lost_race.is_empty());
    assert!(ep.unmetered.is_empty());

    let app = decision(&ep, "demo");
    assert_eq!(app.action, AppAction::Insert);
    assert_eq!(app.plan_id, Some(PLAN));
    let license_id = app.license_id.expect("the insert set its licence id");
    assert_eq!(app.metering_pending, "");
    // The route reports `metering` from these two: no currency -> `token_and_quota`, and an empty
    // `metering_pending` keeps it from being overridden with `pending`.
    assert_eq!(base_metering(&ep), "token_and_quota");

    // The hold and the grant are written.
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM tenant_bundle WHERE tenant_id = $1 AND bundle_id = $2"
        )
        .bind(TENANT)
        .bind(BUNDLE)
        .fetch_one(&db.pool)
        .await
        .expect("hold"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM tenant_app_grant
              WHERE tenant_id = $1 AND grantee_owner_key = 'owner-b' AND revoked_at IS NULL"
        )
        .bind(TENANT)
        .fetch_one(&db.pool)
        .await
        .expect("grant"),
        1
    );

    // The subject is the deterministic service subject, and the licence names the bundle.
    let svc = subject_id(&db, &format!("svc:{TENANT}:demo"))
        .await
        .expect("a service subject");
    let (bundle, dependency, metadata, metered) = licence_facts(&db, license_id).await;
    assert_eq!(bundle, Some(BUNDLE));
    assert!(!dependency);
    assert_eq!(
        metadata,
        Some(json!({"provisioned_by": "tester", "bundle": KEY}))
    );
    assert!(metered, "the accepted pushes stamp the licence");

    // Metering: the budget and the quota, once each, for this licence and app.
    let budgets = stub.calls_at("/api/budget");
    let quotas = stub.calls_at("/api/quota/config");
    assert_eq!(budgets.len(), 1);
    assert_eq!(quotas.len(), 1);
    assert_eq!(
        budgets[0].body.as_ref().expect("body")["budget_tokens"],
        json!(1000)
    );
    assert_eq!(
        quotas[0].body.as_ref().expect("body")["license_id"],
        json!(license_id.to_string())
    );
    assert_eq!(
        quotas[0].body.as_ref().expect("body")["quota_monthly"],
        json!(100)
    );

    // A second identical call is a no-op: every app `exists`, nothing new is written or pushed.
    let before = counts(&db).await;
    let calls_before = stub.calls().len();
    let again = ensure(&db.pool, TENANT, true, &meter)
        .await
        .expect("second");
    assert!(again.applied);
    let again_app = decision(&again, "demo");
    assert_eq!(again_app.action, AppAction::Exists);
    assert_eq!(again_app.license_id, Some(license_id));
    assert_eq!(again_app.existing_plan_id, Some(PLAN));
    assert_eq!(again_app.metering_pending, "");
    assert!(again.grants_created.is_empty());
    assert_eq!(counts(&db).await, before, "nothing new may be written");
    assert_eq!(stub.calls().len(), calls_before, "no second metering push");
    assert_eq!(
        subject_id(&db, &format!("svc:{TENANT}:demo")).await,
        Some(svc)
    );
}

#[tokio::test]
async fn the_adopt_path_adopts_the_sole_unbundled_licence() {
    short_lock_timeout();
    let db = seeded().await;
    subject_row(&db, SVC, &format!("svc:{TENANT}:demo")).await;
    licence_row(&db, LICENCE, SVC, None).await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let ep = ensure(&db.pool, TENANT, true, &meter).await.expect("apply");
    assert!(ep.applied);
    let app = decision(&ep, "demo");
    assert_eq!(app.action, AppAction::Adopt);
    assert_eq!(app.license_id, Some(LICENCE));

    let (bundle, dependency, _, _) = licence_facts(&db, LICENCE).await;
    assert_eq!(
        bundle,
        Some(BUNDLE),
        "the sole unbundled licence is adopted"
    );
    assert!(!dependency);
    assert_eq!(counts(&db).await.0, 1, "an adopt mints no second licence");
    assert!(stub.calls().is_empty(), "an adopt pushes no metering");
}

#[tokio::test]
async fn the_dependency_path_writes_an_unbundled_shared_licence() {
    short_lock_timeout();
    let db = seeded().await;
    // The dependency grant survives only because the caller is NOT the tenant's owner:
    // `ensure_bundle_grants` skips a grant whose owner owns the tenant.
    requires(&db, APP, DEP_APP, "owner-b").await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let ep = ensure(&db.pool, TENANT, true, &meter).await.expect("apply");
    assert!(ep.applied);
    assert_eq!(
        ep.dependency_grants,
        vec![("owner-b".to_owned(), "workers".to_owned())]
    );

    let dependency = decision(&ep, "workers");
    assert_eq!(dependency.action, AppAction::Insert);
    assert!(dependency.dependency);
    let license_id = dependency.license_id.expect("its licence id");

    let (bundle, is_dependency, metadata, metered) = licence_facts(&db, license_id).await;
    assert_eq!(
        bundle, None,
        "a dependency licence is shared: bundle_id NULL"
    );
    assert!(is_dependency);
    assert_eq!(
        metadata,
        Some(json!({"provisioned_by": "tester", "dependency_of": KEY}))
    );
    assert!(metered);

    // The sold app keeps its bundle; the dependency grant is written app-scoped.
    let sold = decision(&ep, "demo").license_id.expect("demo licence");
    assert_eq!(licence_facts(&db, sold).await.0, Some(BUNDLE));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM tenant_dependency_grant
              WHERE tenant_id = $1 AND owner_key = 'owner-b' AND app_id = $2"
        )
        .bind(TENANT)
        .bind(DEP_APP)
        .fetch_one(&db.pool)
        .await
        .expect("dependency grant"),
        1
    );
    // Both inserts owe metering.
    assert_eq!(stub.calls_at("/api/quota/config").len(), 2);
}

#[tokio::test]
async fn a_lost_insert_to_a_winner_carrying_the_bundle_is_exists() {
    short_lock_timeout();
    let db = seeded().await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);
    let mut ep = bundle_ensure::plan(&db.pool, TENANT, KEY, &["basic".to_owned()], None, None)
        .await
        .expect("plan");
    assert_eq!(decision(&ep, "demo").action, AppAction::Insert);

    // The winner appears between plan and apply, carrying this bundle for the same subject.
    subject_row(&db, SVC, &format!("svc:{TENANT}:demo")).await;
    licence_row(&db, LICENCE, SVC, Some(BUNDLE)).await;

    bundle_ensure_apply::apply(&db.pool, &mut ep, "tester", "bundle_ensure", &meter)
        .await
        .expect("apply");

    assert!(ep.applied);
    assert!(ep.lost_race.is_empty());
    assert!(ep.unmetered.is_empty());
    let app = decision(&ep, "demo");
    assert_eq!(app.action, AppAction::Exists);
    assert_eq!(app.license_id, Some(LICENCE));
    assert_eq!(app.existing_plan_id, Some(PLAN));
    assert_eq!(app.plan_id, None);
    assert_eq!(app.plan_code, "");
    assert_eq!(
        app.note,
        "insert lost a concurrent race; the winner's licence carries this bundle"
    );
    assert_eq!(counts(&db).await.0, 1, "the loser writes no licence");
    assert!(
        stub.calls().is_empty(),
        "a lost insert owes no metering push"
    );
}

#[tokio::test]
async fn a_lost_insert_to_a_winner_not_carrying_the_bundle_refuses_lost_race() {
    short_lock_timeout();
    let db = seeded().await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);
    let mut ep = bundle_ensure::plan(&db.pool, TENANT, KEY, &["basic".to_owned()], None, None)
        .await
        .expect("plan");

    // An unbundled, non-dependency winner does NOT satisfy the inserted app.
    subject_row(&db, SVC, &format!("svc:{TENANT}:demo")).await;
    licence_row(&db, LICENCE, SVC, None).await;

    bundle_ensure_apply::apply(&db.pool, &mut ep, "tester", "bundle_ensure", &meter)
        .await
        .expect("apply");

    let app = decision(&ep, "demo");
    assert_eq!(app.action, AppAction::Refuse);
    assert_eq!(app.refuse_reason, Some(RefuseReason::LostRace));
    assert_eq!(app.refuse_reason.expect("reason").as_str(), "lost_race");
    assert_eq!(
        app.note,
        "insert lost a concurrent race to a licence not carrying this bundle"
    );
    assert_eq!(ep.lost_race, vec!["demo".to_owned()]);
    assert!(ep.unmetered.is_empty());
    assert_eq!(counts(&db).await.0, 1);
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_failed_metering_push_is_recorded_and_never_fails_the_call() {
    short_lock_timeout();
    let db = seeded().await;
    let stub = Stub::new();
    stub.set_budget_status(500);
    let meter = provisioner(&start_stub(stub.clone()).await);

    let ep = ensure(&db.pool, TENANT, true, &meter)
        .await
        .expect("a metering failure must never fail the request");
    assert!(ep.applied);

    let app = decision(&ep, "demo");
    assert_eq!(app.action, AppAction::Insert);
    let license_id = app.license_id.expect("the licence was still written");
    assert!(
        app.metering_pending.contains(METERING_FAILURE_MARKER),
        "{}",
        app.metering_pending
    );
    assert_eq!(ep.unmetered.len(), 1);
    assert!(ep.unmetered[0].starts_with("demo "), "{:?}", ep.unmetered);
    assert!(
        ep.unmetered[0].contains(&license_id.to_string()),
        "{:?}",
        ep.unmetered
    );

    let (_, _, _, metered) = licence_facts(&db, license_id).await;
    assert!(
        !metered,
        "an unmetered licence must stay unstamped for the reconcile sweep"
    );
}

#[tokio::test]
async fn a_dry_run_takes_no_lock_and_writes_nothing() {
    short_lock_timeout();
    let db = seeded().await;
    let pool = roomy(&db).await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    // Hold the tenant lock: a dry run that took the lock would now fail `ensure_in_progress`.
    let lock = TenantEnsureLock::acquire(&pool, TENANT)
        .await
        .expect("acquire the tenant lock");
    let before = counts(&db).await;

    let ep = ensure(&pool, TENANT, false, &meter)
        .await
        .expect("a dry run must not queue behind a live ensure");
    lock.release().await;

    assert!(!ep.applied);
    assert_eq!(decision(&ep, "demo").action, AppAction::Insert);
    assert_eq!(counts(&db).await, before, "a dry run writes nothing");
    assert!(stub.calls().is_empty(), "a dry run pushes no metering");
}

#[tokio::test]
async fn a_held_tenant_lock_refuses_the_same_tenant_and_lets_another_through() {
    short_lock_timeout();
    let db = seeded().await;
    let pool = roomy(&db).await;
    let stub = Stub::new();
    let meter = provisioner(&start_stub(stub.clone()).await);

    let lock = TenantEnsureLock::acquire(&pool, TENANT)
        .await
        .expect("acquire the tenant lock");

    let error = ensure(&pool, TENANT, true, &meter)
        .await
        .expect_err("a held same-tenant lock must refuse");
    match error {
        EnsureError::Refused { code, message } => {
            assert_eq!(code, EnsureErrorCode::EnsureInProgress);
            assert_eq!(code.as_str(), "ensure_in_progress");
            assert!(message.contains("still running"), "{message}");
        }
        other => panic!("expected ensure_in_progress, got {other:?}"),
    }
    assert_eq!(tenant_licences(&db, TENANT).await, 0, "nothing was written");

    // A DIFFERENT tenant's ensure is not blocked by this tenant's lock.
    let other = ensure(&pool, TENANT_B, true, &meter)
        .await
        .expect("another tenant proceeds");
    assert!(other.applied);
    assert_eq!(tenant_licences(&db, TENANT_B).await, 1);

    lock.release().await;
    let retried = ensure(&pool, TENANT, true, &meter)
        .await
        .expect("the released tenant proceeds");
    assert!(retried.applied);
    assert_eq!(tenant_licences(&db, TENANT).await, 1);
}
