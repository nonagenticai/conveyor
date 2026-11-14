//! Behaviour of `conveyor-licenses expire-stale`, the Rust port of
//! `scripts/expire_stale_licences.py`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline. The
//! three environment knobs are supplied through `Settings` (or `run_from`'s lookup), never read
//! from this process's environment, so a developer's own `EXPIRE_*` exports cannot change what
//! these tests prove.
//!
//! The estate seeded below is the reference's own population: two stale `active` licences (the
//! sweep target) whose ages differ so the oldest-first ordering is observable, one live `active`
//! licence, one perpetual `active` licence (`expires_at IS NULL`), a soft-deleted row (the soft
//! delete sets `status='revoked'`), a plain `revoked` row, and a future-dated `pending` row whose
//! `expires_at` is already past — proving the status leg and the deliberate absence of
//! `starts_at`.

mod common;

use chrono::{DateTime, Duration, Utc};
use conveyor_licenses::commands::expire_stale::{Settings, run, run_from, run_with};
use conveyor_licenses::db;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0003);

const SUBJ_STALE_OLD: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0011);
const SUBJ_STALE_NEW: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0012);
const SUBJ_LIVE: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0013);
const SUBJ_PERPETUAL: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0014);
const SUBJ_SOFT_DELETED: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0015);
const SUBJ_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0016);
const SUBJ_PENDING: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0017);

const STALE_OLD: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0021);
const STALE_NEW: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0022);
const LIVE: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0023);
const PERPETUAL: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0024);
const SOFT_DELETED: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0025);
const REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0026);
const PENDING: Uuid = Uuid::from_u128(0x0000_0000_0000_4600_8000_0000_0000_0027);

const ACTOR: &str = "test-sweeper";
const DEFAULT_ACTOR: &str = "cronjob:expire-stale-licences";
/// Whole-second so the timestamptz round trip is exact and comparable.
const PRE_UPDATED_AT_SECONDS: i64 = 1_577_836_800; // 2020-01-01T00:00:00Z

/// One seeded licence: every column a sweep could touch is explicit.
struct Licence {
    id: Uuid,
    subject: Uuid,
    status: &'static str,
    starts_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    deleted_at: Option<DateTime<Utc>>,
    deleted_from_status: Option<&'static str>,
}

fn settings(dry_run: bool, limit: i64) -> Settings {
    Settings {
        actor: ACTOR.to_owned(),
        dry_run,
        limit,
    }
}

fn pre_updated_at() -> DateTime<Utc> {
    DateTime::from_timestamp(PRE_UPDATED_AT_SECONDS, 0).expect("timestamp")
}

async fn insert_license(pool: &PgPool, licence: &Licence) {
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, expires_at,
                              created_at, updated_at, tenant_id, deleted_at, deleted_from_status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(licence.id)
    .bind(licence.subject)
    .bind(APP)
    .bind(PLAN)
    .bind(licence.status)
    .bind(licence.starts_at)
    .bind(licence.expires_at)
    .bind(pre_updated_at())
    .bind(pre_updated_at())
    .bind(TENANT)
    .bind(licence.deleted_at)
    .bind(licence.deleted_from_status)
    .execute(pool)
    .await
    .expect("license");
}

/// A migrated database carrying the seven-licence population described in the module docs.
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
        SUBJ_STALE_OLD,
        SUBJ_STALE_NEW,
        SUBJ_LIVE,
        SUBJ_PERPETUAL,
        SUBJ_SOFT_DELETED,
        SUBJ_REVOKED,
        SUBJ_PENDING,
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
        // The sweep target, oldest first.
        Licence {
            id: STALE_OLD,
            subject: SUBJ_STALE_OLD,
            status: "active",
            starts_at: now - Duration::days(10),
            expires_at: Some(now - Duration::days(2)),
            deleted_at: None,
            deleted_from_status: None,
        },
        Licence {
            id: STALE_NEW,
            subject: SUBJ_STALE_NEW,
            status: "active",
            starts_at: now - Duration::days(10),
            expires_at: Some(now - Duration::days(1)),
            deleted_at: None,
            deleted_from_status: None,
        },
        // Live: an expiry still in the future.
        Licence {
            id: LIVE,
            subject: SUBJ_LIVE,
            status: "active",
            starts_at: now - Duration::days(1),
            expires_at: Some(now + Duration::days(1)),
            deleted_at: None,
            deleted_from_status: None,
        },
        // Perpetual: NULL expires_at must never be swept.
        Licence {
            id: PERPETUAL,
            subject: SUBJ_PERPETUAL,
            status: "active",
            starts_at: now - Duration::days(1),
            expires_at: None,
            deleted_at: None,
            deleted_from_status: None,
        },
        // Soft-deleted (status is already 'revoked', as the soft delete leaves it).
        Licence {
            id: SOFT_DELETED,
            subject: SUBJ_SOFT_DELETED,
            status: "revoked",
            starts_at: now - Duration::days(10),
            expires_at: Some(now - Duration::days(3)),
            deleted_at: Some(now - Duration::days(1)),
            deleted_from_status: Some("active"),
        },
        // Plain revoked, past expiry.
        Licence {
            id: REVOKED,
            subject: SUBJ_REVOKED,
            status: "revoked",
            starts_at: now - Duration::days(10),
            expires_at: Some(now - Duration::days(4)),
            deleted_at: None,
            deleted_from_status: None,
        },
        // Future-dated: the status leg excludes it and `starts_at` is deliberately not a predicate.
        Licence {
            id: PENDING,
            subject: SUBJ_PENDING,
            status: "pending",
            starts_at: now + Duration::days(1),
            expires_at: Some(now - Duration::days(1)),
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

fn sorted_keys(value: &serde_json::Value) -> Vec<String> {
    let object = value.as_object().expect("a JSON object");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort_unstable();
    keys
}

#[tokio::test]
async fn a_dry_run_is_the_default_and_reports_the_selected_ids() {
    // The parser's defaults, including the two that must never be guessed: dry-run TRUE and the
    // script's actor. A BLANK EXPIRE_DRY_RUN reads as "unset", so a manifest key cannot arm it.
    let defaults = Settings::from_lookup(|_| None).expect("defaults");
    assert_eq!(
        defaults,
        Settings {
            actor: DEFAULT_ACTOR.to_owned(),
            dry_run: true,
            limit: 500,
        }
    );
    let blank = Settings::from_lookup(|name| (name == "EXPIRE_DRY_RUN").then(|| "   ".to_owned()))
        .expect("a blank value is the default");
    assert!(
        blank.dry_run,
        "a blank EXPIRE_DRY_RUN must not arm the sweep"
    );
    let armed =
        Settings::from_lookup(|name| (name == "EXPIRE_DRY_RUN").then(|| "false".to_owned()))
            .expect("an explicit false");
    assert!(!armed.dry_run);

    let db = seed_estate().await;
    let outcome = run_with(Some(db.url.clone()), settings(true, 500)).await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["event"], "expire_stale_licenses");
    assert_eq!(payload["actor"], ACTOR);
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["limit"], json!(500));
    assert_eq!(payload["stale_before"], json!(2));
    assert_eq!(payload["stale_after"], json!(2));
    assert_eq!(payload["selected"], json!(2));
    assert_eq!(
        payload["selected_license_ids"],
        json!([STALE_OLD.to_string(), STALE_NEW.to_string()])
    );
    assert_eq!(payload["transitioned"], json!(0));
    assert_eq!(payload["exit_code"], json!(1));
    // Oldest first, in the reference's Python list-repr form.
    assert_eq!(
        payload["reason"],
        format!(
            "NOT CONVERGED (measure-only): 2 licence(s) are structurally active with a past \
             expires_at and a dry run writes nothing, so all of them remain. This is the answer \
             about the estate, not a list of failures. Would transition: ['{STALE_OLD}', \
             '{STALE_NEW}']. Re-run with EXPIRE_DRY_RUN=false to retire them."
        )
    );
    assert!(
        payload["now"]
            .as_str()
            .is_some_and(|at| at.ends_with("+00:00")),
        "{payload}"
    );

    // A dry run measured; it must not have written a single row.
    for id in [STALE_OLD, STALE_NEW, LIVE, PERPETUAL, PENDING] {
        assert_eq!(
            updated_at_of(&db.pool, id).await,
            pre_updated_at(),
            "a dry run wrote to {id}"
        );
    }
    assert_eq!(status_of(&db.pool, STALE_OLD).await, "active");
    assert_eq!(status_of(&db.pool, STALE_NEW).await, "active");
}

#[tokio::test]
async fn an_armed_run_expires_exactly_the_stale_set_and_converges() {
    let db = seed_estate().await;
    let outcome = run_with(Some(db.url.clone()), settings(false, 500)).await;

    assert_eq!(outcome.code, 0, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["stale_before"], json!(2));
    assert_eq!(payload["selected"], json!(2));
    assert_eq!(
        payload["selected_license_ids"],
        json!([STALE_OLD.to_string(), STALE_NEW.to_string()])
    );
    assert_eq!(payload["transitioned"], json!(2));
    assert_eq!(payload["stale_after"], json!(0));
    assert_eq!(payload["exit_code"], json!(0));
    assert_eq!(payload["reason"], "converged");
    assert_eq!(
        sorted_keys(payload),
        [
            "actor",
            "dry_run",
            "event",
            "exit_code",
            "limit",
            "now",
            "reason",
            "selected",
            "selected_license_ids",
            "stale_after",
            "stale_before",
            "transitioned",
        ]
    );

    // Exactly the stale pair moved to 'expired', stamped with the run's own instant.
    for id in [STALE_OLD, STALE_NEW] {
        assert_eq!(status_of(&db.pool, id).await, "expired", "{id}");
        assert!(
            updated_at_of(&db.pool, id).await > pre_updated_at(),
            "{id}: updated_at was not stamped"
        );
    }
    // Live, perpetual, soft-deleted, revoked and pending rows are untouched.
    for id in [LIVE, PERPETUAL] {
        assert_eq!(status_of(&db.pool, id).await, "active", "{id} was retired");
        assert_eq!(updated_at_of(&db.pool, id).await, pre_updated_at(), "{id}");
    }
    for (id, status) in [
        (SOFT_DELETED, "revoked"),
        (REVOKED, "revoked"),
        (PENDING, "pending"),
    ] {
        assert_eq!(status_of(&db.pool, id).await, status, "{id} changed status");
        assert_eq!(updated_at_of(&db.pool, id).await, pre_updated_at(), "{id}");
    }
}

#[tokio::test]
async fn a_limit_smaller_than_the_stale_count_truncates_the_sweep() {
    let db = seed_estate().await;
    let outcome = run_with(Some(db.url.clone()), settings(false, 1)).await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["stale_before"], json!(2));
    assert_eq!(payload["selected"], json!(1));
    assert_eq!(
        payload["selected_license_ids"],
        json!([STALE_OLD.to_string()])
    );
    assert_eq!(payload["transitioned"], json!(1));
    assert_eq!(payload["stale_after"], json!(1));
    assert_eq!(payload["exit_code"], json!(1));
    assert_eq!(
        payload["reason"],
        "TRUNCATED SWEEP: transitioned exactly the limit (1) and 1 stale licence(s) remain. \
         Repeating this schedule chews through the backlog 1 at a time, but until it drains, every \
         run leaves corpses holding their unique-index slots. Raise EXPIRE_LIMIT above 2."
    );

    // Oldest first: the 2-day-old corpse was retired; the 1-day-old one waits for the next run.
    assert_eq!(status_of(&db.pool, STALE_OLD).await, "expired");
    assert_eq!(status_of(&db.pool, STALE_NEW).await, "active");
}

#[tokio::test]
async fn a_missing_database_url_is_a_config_failure() {
    // `run_from` with an empty environment is the binary's path when nothing is exported: the URL
    // is a parameter and the knobs come from the lookup, never from this process's environment.
    let outcome = run_from(None, |_| None).await;
    assert_eq!(outcome.code, 2, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["event"], "expire_stale_licenses");
    assert_eq!(payload["actor"], DEFAULT_ACTOR);
    assert_eq!(
        payload["error"],
        "DATABASE_URL is required (it lives in the licenses-env Secret)"
    );
    assert_eq!(payload["exit_code"], json!(2));
    assert_eq!(payload["error_kind"], "bad_expire_env");
    assert_eq!(
        sorted_keys(payload),
        ["actor", "error", "error_kind", "event", "exit_code"]
    );

    // Blank is absent, exactly like the script's `if not url or not url.strip()`.
    let blank = run_with(Some("   ".to_owned()), settings(true, 500)).await;
    assert_eq!(blank.code, 2, "{}", blank.payload);
    assert_eq!(
        blank.payload["error"],
        "DATABASE_URL is required (it lives in the licenses-env Secret)"
    );

    // And the binary's own entry point agrees when DATABASE_URL is unset for the child process.
    let direct = run(None).await;
    assert_eq!(direct.code, 2, "{}", direct.payload);
}

#[tokio::test]
async fn a_bad_expire_env_is_a_config_failure_with_the_scripts_message() {
    let bad_bool =
        Settings::from_lookup(|name| (name == "EXPIRE_DRY_RUN").then(|| "ture".to_owned()))
            .expect_err("a typo must refuse, never mean false");
    assert_eq!(bad_bool.actor, None, "the script reads the actor last");
    assert_eq!(
        bad_bool.error,
        "EXPIRE_DRY_RUN='ture' is not a boolean (use one of \
         ['', '0', '1', 'false', 'no', 'off', 'on', 'true', 'yes'])"
    );
    assert_eq!(
        sorted_keys(&bad_bool.outcome().payload),
        ["actor", "error", "error_kind", "event", "exit_code"]
    );
    assert_eq!(bad_bool.outcome().code, 2);

    let outcome = run_from(None, |name| {
        (name == "EXPIRE_DRY_RUN").then(|| "ture".to_owned())
    })
    .await;
    assert_eq!(outcome.code, 2, "{}", outcome.payload);
    assert_eq!(outcome.payload["error_kind"], "bad_expire_env");
    assert_eq!(outcome.payload["actor"], serde_json::Value::Null);

    let bad_int = Settings::from_lookup(|name| (name == "EXPIRE_LIMIT").then(|| "abc".to_owned()))
        .expect_err("a non-integer must refuse");
    assert_eq!(bad_int.error, "EXPIRE_LIMIT='abc' is not an integer");

    let out_of_bounds =
        Settings::from_lookup(|name| (name == "EXPIRE_LIMIT").then(|| "0".to_owned()))
            .expect_err("a bound must refuse");
    assert_eq!(out_of_bounds.error, "EXPIRE_LIMIT=0 is outside 1..5000");

    let too_big = Settings::from_lookup(|name| (name == "EXPIRE_LIMIT").then(|| "5001".to_owned()))
        .expect_err("a bound must refuse");
    assert_eq!(too_big.error, "EXPIRE_LIMIT=5001 is outside 1..5000");

    let honoured = Settings::from_lookup(|name| (name == "EXPIRE_LIMIT").then(|| "10".to_owned()))
        .expect("10 is in bounds");
    assert_eq!(honoured.limit, 10);
}

#[tokio::test]
async fn a_clean_estate_converges_without_firing_a_guard() {
    // No licences at all: "found nothing" IS convergence here, and the anti-vacuity guards must
    // not fire just because the sweep acted on nothing.
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    let outcome = run_with(Some(db.url.clone()), settings(false, 500)).await;
    assert_eq!(outcome.code, 0, "{}", outcome.payload);
    assert_eq!(outcome.payload["stale_before"], json!(0));
    assert_eq!(outcome.payload["selected"], json!(0));
    assert_eq!(outcome.payload["selected_license_ids"], json!([]));
    assert_eq!(outcome.payload["transitioned"], json!(0));
    assert_eq!(outcome.payload["stale_after"], json!(0));
    assert_eq!(outcome.payload["reason"], "converged");
}
