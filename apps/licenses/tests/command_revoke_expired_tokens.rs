//! Behaviour of `conveyor-licenses revoke-expired-tokens`, the Rust port of
//! `scripts/revoke_expired_tokens.py`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline. The
//! three environment knobs are supplied through `Settings`, never read from this process's
//! environment, so a developer's own `REVOKE_*` exports cannot change what these tests prove.
//!
//! The estate seeded below is the script's own population: two expired-and-unrevoked tokens (the
//! sweep target), one unexpired token, one long-lived token with no `expires_at` at all, and one
//! already-revoked token whose historical `revoked_at` a sweep must never overwrite.

mod common;

use chrono::{DateTime, Duration, Utc};
use conveyor_licenses::commands::revoke_expired_tokens::{run, run_with, Settings};
use conveyor_licenses::db;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0003);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0004);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0005);
const EXPIRED_OLD: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0006);
const EXPIRED_NEW: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0007);
const LIVE: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0008);
const LONG_LIVED: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0009);
const ALREADY_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_000a);

const ACTOR: &str = "test-sweeper";
/// Whole-second so the timestamptz round trip is exact and comparable.
const PRE_REVOKED_AT_SECONDS: i64 = 1_577_836_800; // 2020-01-01T00:00:00Z

fn settings(dry_run: bool, limit: i64) -> Settings {
    Settings {
        actor: ACTOR.to_owned(),
        dry_run,
        limit,
    }
}

async fn insert_token(
    pool: &PgPool,
    id: Uuid,
    expires_at: Option<DateTime<Utc>>,
    revoked: bool,
    revoked_at: Option<DateTime<Utc>>,
) {
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at, expires_at, revoked, revoked_at)
         VALUES ($1, $2, $3, 'api', now(), $4, $5, $6)",
    )
    .bind(id)
    .bind(LICENSE)
    .bind(format!("hash-{id}"))
    .bind(expires_at)
    .bind(revoked)
    .bind(revoked_at)
    .execute(pool)
    .await
    .expect("token");
}

/// A migrated database carrying the five-token population described in the module docs.
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
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, '0195f48b-d000-7000-8000-000000000001', 'portal', now())",
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

    let now = Utc::now();
    insert_token(&db.pool, EXPIRED_OLD, Some(now - Duration::days(2)), false, None).await;
    insert_token(&db.pool, EXPIRED_NEW, Some(now - Duration::days(1)), false, None).await;
    insert_token(&db.pool, LIVE, Some(now + Duration::days(1)), false, None).await;
    insert_token(&db.pool, LONG_LIVED, None, false, None).await;
    insert_token(
        &db.pool,
        ALREADY_REVOKED,
        Some(now - Duration::days(3)),
        true,
        Some(DateTime::from_timestamp(PRE_REVOKED_AT_SECONDS, 0).expect("timestamp")),
    )
    .await;
    db
}

async fn revoked(pool: &PgPool, id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT revoked FROM token WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("token state")
}

async fn revoked_at(pool: &PgPool, id: Uuid) -> Option<DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT revoked_at FROM token WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("revoked_at")
}

fn sorted_keys(value: &serde_json::Value) -> Vec<String> {
    let object = value.as_object().expect("a JSON object");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort_unstable();
    keys
}

#[tokio::test]
async fn a_dry_run_writes_nothing_and_reports_the_scripts_problems() {
    let db = seed_estate().await;
    let outcome = run_with(Some(db.url.clone()), settings(true, 500)).await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["job"], "revoke_expired_tokens");
    assert_eq!(payload["actor"], ACTOR);
    assert_eq!(payload["status"], "not_converged");
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["limit"], json!(500));
    assert_eq!(payload["expired_before"], json!(2));
    assert_eq!(payload["acted"], json!(2));
    assert_eq!(payload["expired_after"], json!(2));
    assert_eq!(payload["converged"], false);
    assert_eq!(
        payload["problems"],
        json!(["dry_run_found_expired_tokens: 2 still unrevoked"])
    );
    assert!(
        payload["at"]
            .as_str()
            .is_some_and(|at| at.ends_with("+00:00")),
        "{payload}"
    );

    // A dry run measured; it must not have written.
    for id in [EXPIRED_OLD, EXPIRED_NEW, LIVE, LONG_LIVED] {
        assert!(!revoked(&db.pool, id).await, "a dry run wrote to {id}");
        assert!(revoked_at(&db.pool, id).await.is_none(), "{id}");
    }
}

#[tokio::test]
async fn an_armed_run_revokes_exactly_the_expired_set_and_converges() {
    let db = seed_estate().await;
    let pre_revoked_at = revoked_at(&db.pool, ALREADY_REVOKED)
        .await
        .expect("the seeded revoked_at");

    let outcome = run_with(Some(db.url.clone()), settings(false, 500)).await;

    assert_eq!(outcome.code, 0, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["dry_run"], false);
    assert_eq!(payload["limit"], json!(500));
    assert_eq!(payload["expired_before"], json!(2));
    assert_eq!(payload["acted"], json!(2));
    assert_eq!(payload["expired_after"], json!(0));
    assert_eq!(payload["converged"], true);
    assert_eq!(payload["problems"], json!([]));

    // Exactly the expired, unrevoked pair, each with the sweep's audit timestamp.
    for id in [EXPIRED_OLD, EXPIRED_NEW] {
        assert!(revoked(&db.pool, id).await, "{id} was not revoked");
        assert!(revoked_at(&db.pool, id).await.is_some(), "{id}: revoked_at");
    }
    // Neither the unexpired nor the long-lived (NULL expires_at) token was touched.
    for id in [LIVE, LONG_LIVED] {
        assert!(!revoked(&db.pool, id).await, "{id} was revoked");
        assert!(revoked_at(&db.pool, id).await.is_none(), "{id}: revoked_at");
    }
    // The historical audit record survives a sweep.
    assert_eq!(revoked_at(&db.pool, ALREADY_REVOKED).await, Some(pre_revoked_at));
}

#[tokio::test]
async fn a_limit_smaller_than_the_expired_count_leaves_some_behind() {
    let db = seed_estate().await;
    let outcome = run_with(Some(db.url.clone()), settings(false, 1)).await;

    assert_eq!(outcome.code, 1, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["status"], "not_converged");
    assert_eq!(payload["limit"], json!(1));
    assert_eq!(payload["expired_before"], json!(2));
    assert_eq!(payload["acted"], json!(1));
    assert_eq!(payload["expired_after"], json!(1));
    assert_eq!(payload["converged"], false);
    assert_eq!(
        payload["problems"],
        json!(["left_behind: 1 still unrevoked (limit=1)"])
    );

    // Oldest first: the 2-day-old token was swept, the 1-day-old one was left for the next run.
    assert!(revoked(&db.pool, EXPIRED_OLD).await);
    assert!(!revoked(&db.pool, EXPIRED_NEW).await);
}

#[tokio::test]
async fn a_missing_database_url_is_a_config_error() {
    // `run(None)` is the binary's path when DATABASE_URL is unset: the URL is a parameter, never
    // read from this process's environment, so this is deterministic.
    let outcome = run(None).await;
    assert_eq!(outcome.code, 2, "{}", outcome.payload);
    let payload = &outcome.payload;
    assert_eq!(payload["job"], "revoke_expired_tokens");
    assert_eq!(payload["status"], "config_error");
    assert_eq!(payload["error"], "DATABASE_URL is required");
    assert!(payload["actor"].is_string(), "{payload}");
    assert!(payload["dry_run"].is_boolean(), "{payload}");
    assert_eq!(
        sorted_keys(payload),
        ["actor", "dry_run", "error", "job", "status"]
    );

    // Blank is absent, exactly like the script's `(os.getenv("DATABASE_URL") or "").strip()`.
    let blank = run_with(Some("   ".to_owned()), settings(true, 500)).await;
    assert_eq!(blank.code, 2, "{}", blank.payload);
    assert_eq!(blank.payload["status"], "config_error");
    assert_eq!(blank.payload["error"], "DATABASE_URL is required");
}

#[tokio::test]
async fn a_genuinely_clean_estate_converges_with_the_scripts_exact_keys() {
    // No tokens at all: "found nothing" IS convergence here, and the guards must not fire just
    // because the sweep acted on nothing.
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    let outcome = run_with(Some(db.url.clone()), settings(false, 500)).await;
    assert_eq!(outcome.code, 0, "{}", outcome.payload);
    assert_eq!(outcome.payload["status"], "ok");
    assert_eq!(outcome.payload["expired_before"], json!(0));
    assert_eq!(outcome.payload["acted"], json!(0));
    assert_eq!(outcome.payload["expired_after"], json!(0));
    assert_eq!(outcome.payload["converged"], true);
    assert_eq!(
        sorted_keys(&outcome.payload),
        [
            "acted",
            "actor",
            "at",
            "converged",
            "dry_run",
            "expired_after",
            "expired_before",
            "job",
            "limit",
            "problems",
            "status",
        ]
    );
}
