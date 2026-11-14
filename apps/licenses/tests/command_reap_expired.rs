//! Behaviour of `conveyor-licenses reap-expired`, the Rust port of
//! `scripts/reap_expired_licences.py`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, plus
//! the same stub billing server `tests/billing_teardown.rs` uses. The environment knobs and the
//! billing client are supplied through `Settings` / `run_with`, never read from this process's
//! environment, so a developer's own `REAP_*` exports cannot change what these tests prove.
//!
//! The estate seeded below is the reference's own population, all expired relative to a captured
//! `now` inside a 30-day grace window unless stated:
//!
//! - `REAP_OLD` (-50d), `REAP_NEW` (-40d) and `OUTSIDE` (-30d - 1h): REAPABLE under a 30-day
//!   grace. `REAP_OLD` already carries a `revoked_at`, proving the sweep stamps it only if unset.
//! - `INSIDE` (-30d + 1h) and `RECENT` (-5d): expired but INSIDE the grace window — the cutoff
//!   boundary in both directions, and both become reapable at `REAP_GRACE_DAYS=0`.
//! - `LIVE` (future expiry, `active`), `REVOKED` (past expiry, `revoked`), `SOFT_DELETED`
//!   (`revoked` + `deleted_at`) and `PERPETUAL_EXPIRED` (`expired`, NULL expiry): each excluded
//!   by exactly one leg of the predicate and left untouched.

mod common;

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use conveyor_licenses::billing::BillingTeardown;
use conveyor_licenses::commands::reap_expired::{Settings, run_from, run_with};
use conveyor_licenses::db;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0003);

const SUBJ_REAP_OLD: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0011);
const SUBJ_REAP_NEW: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0012);
const SUBJ_OUTSIDE: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0013);
const SUBJ_INSIDE: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0014);
const SUBJ_RECENT: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0015);
const SUBJ_LIVE: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0016);
const SUBJ_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0017);
const SUBJ_SOFT_DELETED: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0018);
const SUBJ_PERPETUAL: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0019);

const REAP_OLD: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0021);
const REAP_NEW: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0022);
const OUTSIDE: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0023);
const INSIDE: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0024);
const RECENT: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0025);
const LIVE: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0026);
const REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0027);
const SOFT_DELETED: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0028);
const PERPETUAL_EXPIRED: Uuid = Uuid::from_u128(0x0000_0000_0000_4599_8000_0000_0000_0029);

const ACTOR: &str = "test-reaper";
const DEFAULT_ACTOR: &str = "cronjob:reap-expired-licences";
/// Whole-second so the timestamptz round trip is exact and comparable.
const PRE_UPDATED_AT_SECONDS: i64 = 1_577_836_800; // 2020-01-01T00:00:00Z

fn settings(dry_run: bool, limit: i64, grace_days: i64) -> Settings {
    Settings {
        actor: ACTOR.to_owned(),
        dry_run,
        limit,
        grace_days,
    }
}

fn pre_updated_at() -> DateTime<Utc> {
    DateTime::from_timestamp(PRE_UPDATED_AT_SECONDS, 0).expect("timestamp")
}

struct Licence {
    id: Uuid,
    subject: Uuid,
    status: &'static str,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    deleted_at: Option<DateTime<Utc>>,
    deleted_from_status: Option<&'static str>,
}

async fn insert_license(pool: &PgPool, licence: &Licence) {
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, expires_at,
                              created_at, updated_at, revoked_at, tenant_id, deleted_at,
                              deleted_from_status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(licence.id)
    .bind(licence.subject)
    .bind(APP)
    .bind(PLAN)
    .bind(licence.status)
    .bind(pre_updated_at())
    .bind(licence.expires_at)
    .bind(pre_updated_at())
    .bind(pre_updated_at())
    .bind(licence.revoked_at)
    .bind(TENANT)
    .bind(licence.deleted_at)
    .bind(licence.deleted_from_status)
    .execute(pool)
    .await
    .expect("license");
}

/// A migrated database carrying the nine-licence population described in the module docs.
async fn seed_estate() -> common::TestDb {
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
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, 'basic', 'Basic', now())",
    )
    .bind(PLAN)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("plan");
    for subject in [
        SUBJ_REAP_OLD,
        SUBJ_REAP_NEW,
        SUBJ_OUTSIDE,
        SUBJ_INSIDE,
        SUBJ_RECENT,
        SUBJ_LIVE,
        SUBJ_REVOKED,
        SUBJ_SOFT_DELETED,
        SUBJ_PERPETUAL,
    ] {
        sqlx::query(
            "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
             VALUES ($1, $2, 'portal', now())",
        )
        .bind(subject)
        .bind(subject.to_string())
        .execute(&db.pool)
        .await
        .expect("subject");
    }

    let now = Utc::now();
    let licences = [
        // Reapable under a 30-day grace, oldest first. REAP_OLD already carries a revoked_at.
        Licence {
            id: REAP_OLD,
            subject: SUBJ_REAP_OLD,
            status: "expired",
            expires_at: Some(now - Duration::days(50)),
            revoked_at: Some(pre_updated_at()),
            deleted_at: None,
            deleted_from_status: None,
        },
        Licence {
            id: REAP_NEW,
            subject: SUBJ_REAP_NEW,
            status: "expired",
            expires_at: Some(now - Duration::days(40)),
            revoked_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
        // The cutoff boundary, both sides: 30d + 1h old is reapable, 30d - 1h old is not.
        Licence {
            id: OUTSIDE,
            subject: SUBJ_OUTSIDE,
            status: "expired",
            expires_at: Some(now - Duration::days(30) - Duration::hours(1)),
            revoked_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
        Licence {
            id: INSIDE,
            subject: SUBJ_INSIDE,
            status: "expired",
            expires_at: Some(now - Duration::days(30) + Duration::hours(1)),
            revoked_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
        Licence {
            id: RECENT,
            subject: SUBJ_RECENT,
            status: "expired",
            expires_at: Some(now - Duration::days(5)),
            revoked_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
        // Live: an expiry still in the future.
        Licence {
            id: LIVE,
            subject: SUBJ_LIVE,
            status: "active",
            expires_at: Some(now + Duration::days(10)),
            revoked_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
        // Already revoked: the status leg excludes it, and against a past expiry.
        Licence {
            id: REVOKED,
            subject: SUBJ_REVOKED,
            status: "revoked",
            expires_at: Some(now - Duration::days(40)),
            revoked_at: Some(pre_updated_at()),
            deleted_at: None,
            deleted_from_status: None,
        },
        // Soft-deleted (the soft delete leaves status='revoked').
        Licence {
            id: SOFT_DELETED,
            subject: SUBJ_SOFT_DELETED,
            status: "revoked",
            expires_at: Some(now - Duration::days(40)),
            revoked_at: Some(pre_updated_at()),
            deleted_at: Some(now - Duration::days(1)),
            deleted_from_status: Some("active"),
        },
        // status='expired' with NULL expires_at is PERPETUAL: `expires_at IS NOT NULL` excludes it.
        Licence {
            id: PERPETUAL_EXPIRED,
            subject: SUBJ_PERPETUAL,
            status: "expired",
            expires_at: None,
            revoked_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
    ];
    for licence in &licences {
        insert_license(&db.pool, licence).await;
    }
    db
}

async fn status_of(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM license WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("license status")
}

async fn updated_at_of(pool: &PgPool, id: Uuid) -> DateTime<Utc> {
    sqlx::query_scalar::<_, DateTime<Utc>>("SELECT updated_at FROM license WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("license updated_at")
}

async fn revoked_at_of(pool: &PgPool, id: Uuid) -> Option<DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT revoked_at FROM license WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("license revoked_at")
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let object = value.as_object().expect("a JSON object");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort_unstable();
    keys
}

fn python_list(ids: &[Uuid]) -> String {
    let items: Vec<String> = ids.iter().map(|id| format!("'{id}'")).collect();
    format!("[{}]", items.join(", "))
}

// ---------------------------------------------------------------------------------------------
// The billing stub: it records every call and answers with configured statuses.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    method: &'static str,
    path: String,
}

#[derive(Clone)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
    quota_status: Arc<Mutex<u16>>,
    list_status: Arc<Mutex<u16>>,
}

impl Stub {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            quota_status: Arc::new(Mutex::new(200)),
            list_status: Arc::new(Mutex::new(200)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }
}

async fn quota_delete(
    AxumState(stub): AxumState<Stub>,
    _headers: HeaderMap,
    Path(license_id): Path<String>,
) -> StatusCode {
    stub.calls.lock().expect("stub lock").push(Call {
        method: "DELETE",
        path: format!("/api/quota/config/{license_id}"),
    });
    StatusCode::from_u16(*stub.quota_status.lock().expect("stub lock")).expect("status")
}

async fn budget_list(
    AxumState(stub): AxumState<Stub>,
    _headers: HeaderMap,
    Path(tenant): Path<String>,
) -> axum::response::Response {
    stub.calls.lock().expect("stub lock").push(Call {
        method: "GET",
        path: format!("/api/budget/list/{tenant}"),
    });
    let status = *stub.list_status.lock().expect("stub lock");
    if status != 200 {
        return StatusCode::from_u16(status)
            .expect("status")
            .into_response();
    }
    // No active license_sync budgets: every teardown half succeeds.
    Json(json!({"data": []})).into_response()
}

/// Start the stub on an ephemeral port; the task lives as long as the test's runtime.
async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/quota/config/{license_id}", delete(quota_delete))
        .route("/api/budget/list/{tenant_id}", get(budget_list))
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

async fn billing(stub: &Stub) -> BillingTeardown {
    let base = start_stub(stub.clone()).await;
    BillingTeardown::new(&base, None).expect("billing client")
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_dry_run_is_the_default_and_reports_without_writing_or_calling_billing() {
    // The parser's defaults, including the two that must never be guessed: dry-run TRUE and the
    // script's actor. A BLANK value reads as "unset", so a manifest key cannot arm the sweep.
    let defaults = Settings::from_lookup(|_| None).expect("defaults");
    assert_eq!(
        defaults,
        Settings {
            actor: DEFAULT_ACTOR.to_owned(),
            dry_run: true,
            limit: 500,
            grace_days: 30,
        }
    );
    let blank = Settings::from_lookup(|name| {
        matches!(name, "REAP_DRY_RUN" | "REAP_GRACE_DAYS").then(|| "   ".to_owned())
    })
    .expect("a blank value is the default");
    assert!(blank.dry_run, "a blank REAP_DRY_RUN must not arm");
    assert_eq!(blank.grace_days, 30);
    let armed = Settings::from_lookup(|name| (name == "REAP_DRY_RUN").then(|| "false".to_owned()))
        .expect("an explicit false");
    assert!(!armed.dry_run);
    let zero = Settings::from_lookup(|name| (name == "REAP_GRACE_DAYS").then(|| "0".to_owned()))
        .expect("0 is in bounds");
    assert_eq!(zero.grace_days, 0);

    let db = seed_estate().await;
    let stub = Stub::new();
    let outcome = run_with(
        Some(db.url.clone()),
        settings(true, 500, 30),
        Some(billing(&stub).await),
    )
    .await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["event"], "reap_expired_licenses");
    assert_eq!(payload["actor"], ACTOR);
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["limit"], json!(500));
    assert_eq!(payload["grace_days"], json!(30));
    assert_eq!(payload["reapable_before"], json!(3));
    assert_eq!(payload["reapable_after"], json!(3));
    assert_eq!(payload["selected"], json!(3));
    assert_eq!(
        payload["selected_license_ids"],
        json!([REAP_OLD.to_string(), REAP_NEW.to_string(), OUTSIDE.to_string()])
    );
    assert_eq!(payload["archived"], json!(0), "a dry run writes nothing");
    assert_eq!(payload["exit_code"], json!(1));
    assert_eq!(payload["teardown_mode"], "dry_run");
    assert_eq!(payload["teardown_attempted"], json!(0));
    assert_eq!(payload["teardown_quota_failed"], json!([]));
    assert_eq!(payload["teardown_budget_failed"], json!([]));
    assert_eq!(payload["teardown_unresolved_tenant"], json!([]));
    assert_eq!(
        payload["teardown_would_call"],
        json!([
            {"license_id": REAP_OLD.to_string(), "tenant_id": TENANT.to_string()},
            {"license_id": REAP_NEW.to_string(), "tenant_id": TENANT.to_string()},
            {"license_id": OUTSIDE.to_string(), "tenant_id": TENANT.to_string()},
        ])
    );
    assert_eq!(
        payload["reason"],
        format!(
            "NOT CONVERGED (measure-only): 3 licence(s) are status='expired' with an expiry \
             older than the 30-day grace window, and a dry run writes nothing, so all of them \
             remain — each one latching \
             nonagentic_license_expired_rows{{kind=\"license\",revoked=\"false\"}} and pinning \
             LicenseAlreadyExpired. This is the answer about the estate, not a list of failures. \
             Would archive: {}. Re-run with REAP_DRY_RUN=false to archive them.",
            python_list(&[REAP_OLD, REAP_NEW, OUTSIDE])
        )
    );
    assert!(
        payload["now"]
            .as_str()
            .is_some_and(|at| at.ends_with("+00:00")),
        "{payload}"
    );
    assert!(
        payload["cutoff"]
            .as_str()
            .is_some_and(|at| at.ends_with("+00:00")),
        "{payload}"
    );
    assert!(
        stub.calls().is_empty(),
        "a dry run must put nothing on the wire: {:?}",
        stub.calls()
    );

    // Measured only: not a single row moved.
    for id in [
        REAP_OLD,
        REAP_NEW,
        OUTSIDE,
        INSIDE,
        RECENT,
        LIVE,
        REVOKED,
        SOFT_DELETED,
        PERPETUAL_EXPIRED,
    ] {
        assert_eq!(updated_at_of(&db.pool, id).await, pre_updated_at(), "{id}");
    }
    assert_eq!(status_of(&db.pool, REAP_OLD).await, "expired");
    assert_eq!(status_of(&db.pool, REAP_NEW).await, "expired");
}

#[tokio::test]
async fn an_armed_run_reaps_exactly_the_reapable_set_and_converges() {
    let db = seed_estate().await;
    let stub = Stub::new();
    let outcome = run_with(
        Some(db.url.clone()),
        settings(false, 500, 30),
        Some(billing(&stub).await),
    )
    .await;

    assert_eq!(outcome.code, 0, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["reapable_before"], json!(3));
    assert_eq!(payload["reapable_after"], json!(0));
    assert_eq!(payload["selected"], json!(3));
    assert_eq!(
        payload["selected_license_ids"],
        json!([REAP_OLD.to_string(), REAP_NEW.to_string(), OUTSIDE.to_string()])
    );
    assert_eq!(payload["archived"], json!(3));
    assert_eq!(payload["exit_code"], json!(0));
    assert_eq!(payload["reason"], "converged");
    assert_eq!(payload["teardown_mode"], "performed");
    assert_eq!(payload["teardown_attempted"], json!(3));
    assert_eq!(payload["teardown_quota_failed"], json!([]));
    assert_eq!(payload["teardown_budget_failed"], json!([]));
    assert_eq!(payload["teardown_unresolved_tenant"], json!([]));
    assert_eq!(
        sorted_keys(payload),
        [
            "actor",
            "archived",
            "cutoff",
            "dry_run",
            "event",
            "exit_code",
            "grace_days",
            "limit",
            "now",
            "reapable_after",
            "reapable_before",
            "reason",
            "selected",
            "selected_license_ids",
            "teardown_attempted",
            "teardown_budget_failed",
            "teardown_mode",
            "teardown_quota_failed",
            "teardown_unresolved_tenant",
        ]
    );

    // Exactly the reapable set moved to 'revoked'.
    for id in [REAP_OLD, REAP_NEW, OUTSIDE] {
        assert_eq!(status_of(&db.pool, id).await, "revoked", "{id}");
        assert!(
            updated_at_of(&db.pool, id).await > pre_updated_at(),
            "{id}: updated_at was not stamped"
        );
    }
    // `revoked_at` is written ONLY IF UNSET: the pre-set one survives, the unset one is stamped.
    assert_eq!(
        revoked_at_of(&db.pool, REAP_OLD).await,
        Some(pre_updated_at())
    );
    assert!(
        revoked_at_of(&db.pool, REAP_NEW)
            .await
            .is_some_and(|at| at > pre_updated_at()),
        "the sweep did not stamp revoked_at"
    );
    // Every other shape is untouched: inside the grace window, live, revoked, soft-deleted and
    // perpetually-expired rows stay exactly where they were.
    for (id, status) in [
        (INSIDE, "expired"),
        (RECENT, "expired"),
        (LIVE, "active"),
        (REVOKED, "revoked"),
        (SOFT_DELETED, "revoked"),
        (PERPETUAL_EXPIRED, "expired"),
    ] {
        assert_eq!(status_of(&db.pool, id).await, status, "{id} changed status");
        assert_eq!(updated_at_of(&db.pool, id).await, pre_updated_at(), "{id}");
    }

    // Both teardown halves were asked, once per archived licence.
    let calls = stub.calls();
    for id in [REAP_OLD, REAP_NEW, OUTSIDE] {
        assert!(
            calls
                .iter()
                .any(|call| call.method == "DELETE"
                    && call.path == format!("/api/quota/config/{id}")),
            "no quota teardown for {id}: {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|call| call.method == "GET"
                    && call.path == format!("/api/budget/list/{TENANT}")),
            "no budget teardown for {id}: {calls:?}"
        );
    }
}

#[tokio::test]
async fn a_limit_smaller_than_the_reapable_count_truncates_the_sweep() {
    let db = seed_estate().await;
    let stub = Stub::new();
    let outcome = run_with(
        Some(db.url.clone()),
        settings(false, 1, 30),
        Some(billing(&stub).await),
    )
    .await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["reapable_before"], json!(3));
    assert_eq!(payload["reapable_after"], json!(2));
    assert_eq!(payload["selected"], json!(1));
    assert_eq!(payload["selected_license_ids"], json!([REAP_OLD.to_string()]));
    assert_eq!(payload["archived"], json!(1));
    assert_eq!(payload["exit_code"], json!(1));
    assert_eq!(
        payload["reason"],
        "TRUNCATED SWEEP: archived exactly the limit (1) and 2 reapable licence(s) remain. \
         Repeating this schedule chews through the backlog 1 at a time, but until it drains the \
         gauge stays latched. Raise REAP_LIMIT above 3."
    );

    // Oldest first: the 50-day corpse was archived; the rest wait for the next run.
    assert_eq!(status_of(&db.pool, REAP_OLD).await, "revoked");
    assert_eq!(status_of(&db.pool, REAP_NEW).await, "expired");
    assert_eq!(status_of(&db.pool, OUTSIDE).await, "expired");
}

#[tokio::test]
async fn grace_days_zero_reaps_every_expired_row_with_a_past_expiry() {
    let db = seed_estate().await;
    let stub = Stub::new();
    let outcome = run_with(
        Some(db.url.clone()),
        settings(false, 500, 0),
        Some(billing(&stub).await),
    )
    .await;

    assert_eq!(outcome.code, 0, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["grace_days"], json!(0));
    // The token reaper's behaviour: the grace window is the only thing INSIDE and RECENT needed.
    assert_eq!(payload["reapable_before"], json!(5));
    assert_eq!(payload["reapable_after"], json!(0));
    assert_eq!(payload["archived"], json!(5));
    assert_eq!(
        payload["selected_license_ids"],
        json!([
            REAP_OLD.to_string(),
            REAP_NEW.to_string(),
            OUTSIDE.to_string(),
            INSIDE.to_string(),
            RECENT.to_string(),
        ])
    );
    for id in [REAP_OLD, REAP_NEW, OUTSIDE, INSIDE, RECENT] {
        assert_eq!(status_of(&db.pool, id).await, "revoked", "{id}");
    }
    // Still excluded by their own leg of the predicate.
    assert_eq!(status_of(&db.pool, LIVE).await, "active");
    assert_eq!(status_of(&db.pool, REVOKED).await, "revoked");
    assert_eq!(status_of(&db.pool, PERPETUAL_EXPIRED).await, "expired");
}

#[tokio::test]
async fn a_billing_failure_does_not_change_the_reap_but_is_visible_in_the_payload() {
    let db = seed_estate().await;
    let stub = Stub::new();
    *stub.quota_status.lock().expect("stub lock") = 503;
    *stub.list_status.lock().expect("stub lock") = 503;
    let outcome = run_with(
        Some(db.url.clone()),
        settings(false, 500, 30),
        Some(billing(&stub).await),
    )
    .await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    // The rows ARE archived: the reap verdict is unchanged by the billing failure.
    assert_eq!(payload["archived"], json!(3));
    assert_eq!(payload["reapable_after"], json!(0));
    for id in [REAP_OLD, REAP_NEW, OUTSIDE] {
        assert_eq!(status_of(&db.pool, id).await, "revoked", "{id}");
    }
    // The residue is unconditional fields on the audit line.
    assert_eq!(payload["teardown_mode"], "performed");
    assert_eq!(payload["teardown_attempted"], json!(3));
    let failed = json!([
        REAP_OLD.to_string(),
        REAP_NEW.to_string(),
        OUTSIDE.to_string()
    ]);
    assert_eq!(payload["teardown_quota_failed"], failed);
    assert_eq!(payload["teardown_budget_failed"], failed);
    assert_eq!(payload["teardown_unresolved_tenant"], json!([]));
    assert_eq!(
        payload["reason"],
        format!(
            "BILLING RESIDUE: the licence rows ARE archived, but billing still holds a mirror \
             for some of them — quota teardown failed for {list}, token budget teardown failed \
             for {list}, tenant unresolvable for []. A revoked licence with a live license_sync \
             token budget is SPENDABLE, and billing's own reconciler \
             (POST /api/budget/sync-from-licenses) runs on NO timer, so this does not heal on its \
             own. The per-licence cause is on stderr under QUOTA_CONFIG_TEARDOWN_FAILED / \
             TOKEN_BUDGET_TEARDOWN_FAILED (billing refused us or was unreachable) or \
             ..._ABANDONED (this pod was terminated mid-call — NOT a billing fault).",
            list = python_list(&[REAP_OLD, REAP_NEW, OUTSIDE])
        )
    );
}

#[tokio::test]
async fn a_missing_database_url_and_a_bad_env_value_are_config_failures() {
    // `run_with(None, ..)` is the binary's path when DATABASE_URL is unset: one JSON line, exit 2.
    let outcome = run_with(None, settings(true, 500, 30), None).await;
    assert_eq!(outcome.code, 2, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["event"], "reap_expired_licenses");
    assert_eq!(payload["actor"], ACTOR);
    assert_eq!(
        payload["error"],
        "DATABASE_URL is required (it lives in the licenses-env Secret)"
    );
    assert_eq!(payload["exit_code"], json!(2));
    assert_eq!(payload["error_kind"], "bad_reap_env");
    assert_eq!(
        sorted_keys(payload),
        ["actor", "error", "error_kind", "event", "exit_code"]
    );

    // `run_from` with an empty environment resolves the defaults, then refuses the URL.
    let direct = run_from(None, |_| None).await;
    assert_eq!(direct.code, 2, "{}", direct.payload);
    assert_eq!(direct.payload["actor"], DEFAULT_ACTOR);

    // A typo must refuse, never quietly mean False, and the actor is still NULL because the
    // script reads REAP_ACTOR LAST.
    let bad_bool = Settings::from_lookup(|name| {
        (name == "REAP_DRY_RUN").then(|| "ture".to_owned())
    })
    .expect_err("a typo must refuse");
    assert_eq!(bad_bool.actor, None);
    assert_eq!(
        bad_bool.error,
        "REAP_DRY_RUN='ture' is not a boolean (use one of \
         ['', '0', '1', 'false', 'no', 'off', 'on', 'true', 'yes'])"
    );
    assert_eq!(bad_bool.outcome().code, 2);
    assert_eq!(
        sorted_keys(&bad_bool.outcome().payload),
        ["actor", "error", "error_kind", "event", "exit_code"]
    );
    assert_eq!(bad_bool.outcome().payload["actor"], Value::Null);

    let outcome =
        run_from(None, |name| (name == "REAP_DRY_RUN").then(|| "ture".to_owned())).await;
    assert_eq!(outcome.code, 2, "{}", outcome.payload);
    assert_eq!(outcome.payload["actor"], Value::Null);
    assert_eq!(outcome.payload["error_kind"], "bad_reap_env");
    assert_eq!(
        outcome.payload["error"],
        "REAP_DRY_RUN='ture' is not a boolean (use one of \
         ['', '0', '1', 'false', 'no', 'off', 'on', 'true', 'yes'])"
    );

    let bad_int = Settings::from_lookup(|name| (name == "REAP_LIMIT").then(|| "abc".to_owned()))
        .expect_err("a non-integer must refuse");
    assert_eq!(bad_int.error, "REAP_LIMIT='abc' is not an integer");
    let too_small = Settings::from_lookup(|name| (name == "REAP_LIMIT").then(|| "0".to_owned()))
        .expect_err("a bound must refuse");
    assert_eq!(too_small.error, "REAP_LIMIT=0 is outside 1..5000");
    let too_big =
        Settings::from_lookup(|name| (name == "REAP_GRACE_DAYS").then(|| "3651".to_owned()))
            .expect_err("a bound must refuse");
    assert_eq!(
        too_big.error,
        "REAP_GRACE_DAYS=3651 is outside 0..3650"
    );
}

/// The CronJob path, end to end: the real binary, a cleared environment, `DATABASE_URL` absent.
/// Exactly one JSON line on stdout and exit 2.
#[test]
fn the_binary_prints_one_json_line_without_a_database_url() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_conveyor-licenses"))
        .arg("reap-expired")
        .env_clear()
        .output()
        .expect("run the binary");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "not exactly one line: {stdout:?}");
    assert_eq!(
        lines[0],
        "{\"actor\":\"cronjob:reap-expired-licences\",\"error\":\"DATABASE_URL is required (it \
         lives in the licenses-env Secret)\",\"error_kind\":\"bad_reap_env\",\"event\":\
         \"reap_expired_licenses\",\"exit_code\":2}"
    );
}
