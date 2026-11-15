//! Behaviour of the dependency-release engine (`db::dependencies`) against a real Postgres 18.
//!
//! Each case runs on a fresh database created by `common` and the embedded baseline, so the
//! predicates meet the captured schema (`app_requires`, `tenant_dependency_grant`,
//! `license.is_dependency`, `token.revoked`) rather than a mock. The engine is included by path
//! until the parent wires `pub mod dependencies;` into `db/mod.rs`; compiling it a second time in
//! the test crate is deliberate and harmless.

mod common;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use conveyor_licenses::db::dependencies::{
    DEPENDENCY_EXPLICIT_REVOKE_REASON, DEPENDENCY_RELEASE_REASON, Outcome, RequiredBy,
    revoke_license_releasing_dependencies,
};

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0001);
const OWNER: &str = "owner-a";
const APP_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0002);
const PLAN_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0003);
const APP_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0004);
const PLAN_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0005);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0006);
const LICENSE_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0007);
const LICENSE_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0008);
const TOKEN_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_0009);
const TOKEN_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_000a);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_000b);
const APP_C: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_000c);
const PLAN_C: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_000d);
const LICENSE_C: Uuid = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_000e);

const ACTOR: &str = "tester";

type LicenceState = (String, Option<DateTime<Utc>>, Option<serde_json::Value>);
type TokenState = (bool, Option<DateTime<Utc>>);

async fn fresh() -> PgPool {
    let db = common::fresh().await;
    conveyor_licenses::db::migrate(&db.pool)
        .await
        .expect("migrate");
    db.pool
}

/// The tenant, its owner and one subject: the rows every licence needs.
async fn seed_world(pool: &PgPool) {
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, 'Owner A')")
        .bind(OWNER)
        .execute(pool)
        .await
        .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key) VALUES ($1, 'tenant-a', now(), $2)",
    )
    .bind(TENANT)
    .bind(OWNER)
    .execute(pool)
    .await
    .expect("tenant");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, '0195f48b-d000-7000-8000-000000000001', 'portal', now())",
    )
    .bind(SUBJECT)
    .execute(pool)
    .await
    .expect("subject");
}

async fn seed_app(pool: &PgPool, app_id: Uuid, plan_id: Uuid, key: &str) {
    sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $3, now())")
        .bind(app_id)
        .bind(key)
        .bind(key)
        .execute(pool)
        .await
        .expect("app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, $3, $4, now())",
    )
    .bind(plan_id)
    .bind(app_id)
    .bind(key)
    .bind(key)
    .execute(pool)
    .await
    .expect("plan");
}

async fn seed_licence(
    pool: &PgPool,
    licence_id: Uuid,
    app_id: Uuid,
    plan_id: Uuid,
    is_dependency: bool,
    bundle_id: Option<Uuid>,
) {
    sqlx::query(
        "INSERT INTO license
             (id, subject_id, app_id, plan_id, status, starts_at, tenant_id, is_dependency, bundle_id)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, $6, $7)",
    )
    .bind(licence_id)
    .bind(SUBJECT)
    .bind(app_id)
    .bind(plan_id)
    .bind(TENANT)
    .bind(is_dependency)
    .bind(bundle_id)
    .execute(pool)
    .await
    .expect("license");
}

async fn seed_token(pool: &PgPool, token_id: Uuid, licence_id: Uuid, raw: &str) {
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at)
         VALUES ($1, $2, $3, 'api', now())",
    )
    .bind(token_id)
    .bind(licence_id)
    .bind(conveyor_licenses::db::tokens::hash_token(raw))
    .execute(pool)
    .await
    .expect("token");
}

async fn seed_edge(pool: &PgPool, app_id: Uuid, required_app_id: Uuid) {
    sqlx::query(
        "INSERT INTO app_requires (app_id, required_app_id, caller_owner_key)
         VALUES ($1, $2, $3)",
    )
    .bind(app_id)
    .bind(required_app_id)
    .bind(OWNER)
    .execute(pool)
    .await
    .expect("app_requires");
}

async fn seed_grant(pool: &PgPool, app_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenant_dependency_grant (tenant_id, owner_key, app_id, granted_by)
         VALUES ($1, $2, $3, 'test')",
    )
    .bind(TENANT)
    .bind(OWNER)
    .bind(app_id)
    .execute(pool)
    .await
    .expect("tenant_dependency_grant");
}

async fn seed_bundle(pool: &PgPool) {
    sqlx::query("INSERT INTO bundle (id, key, name) VALUES ($1, 'bundle-x', 'Bundle X')")
        .bind(BUNDLE)
        .execute(pool)
        .await
        .expect("bundle");
}

async fn licence_state(pool: &PgPool, licence_id: Uuid) -> LicenceState {
    sqlx::query_as("SELECT status, revoked_at, metadata FROM license WHERE id = $1")
        .bind(licence_id)
        .fetch_one(pool)
        .await
        .expect("licence state")
}

async fn token_state(pool: &PgPool, token_id: Uuid) -> TokenState {
    sqlx::query_as("SELECT revoked, revoked_at FROM token WHERE id = $1")
        .bind(token_id)
        .fetch_one(pool)
        .await
        .expect("token state")
}

async fn grant_count(pool: &PgPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM tenant_dependency_grant
          WHERE tenant_id = $1 AND owner_key = $2 AND app_id = $3",
    )
    .bind(TENANT)
    .bind(OWNER)
    .bind(app_id)
    .fetch_one(pool)
    .await
    .expect("grant count")
}

/// Root (app A) + dependency (app B) + the edge A -> B, the shape every release case needs.
async fn seed_root_and_dependency(pool: &PgPool) {
    seed_world(pool).await;
    seed_app(pool, APP_A, PLAN_A, "app-a").await;
    seed_app(pool, APP_B, PLAN_B, "app-b").await;
    seed_licence(pool, LICENSE_A, APP_A, PLAN_A, false, None).await;
    seed_licence(pool, LICENSE_B, APP_B, PLAN_B, true, None).await;
    seed_edge(pool, APP_A, APP_B).await;
}

#[tokio::test]
async fn a_plain_licence_revokes_and_releases_nothing() {
    let pool = fresh().await;
    seed_world(&pool).await;
    seed_app(&pool, APP_A, PLAN_A, "app-a").await;
    seed_licence(&pool, LICENSE_A, APP_A, PLAN_A, false, None).await;
    seed_token(&pool, TOKEN_A, LICENSE_A, "raw-token-a").await;

    let outcome = revoke_license_releasing_dependencies(&pool, LICENSE_A, ACTOR, false)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        Outcome::Revoked {
            released_ids: Vec::new()
        }
    );
    let (status, revoked_at, metadata) = licence_state(&pool, LICENSE_A).await;
    assert_eq!(status, "revoked");
    assert!(revoked_at.is_some(), "the revoke is stamped");
    assert!(
        metadata.is_none(),
        "a plain revoke is not a dependency release"
    );
    let (revoked, token_revoked_at) = token_state(&pool, TOKEN_A).await;
    assert!(revoked, "every token of the revoked licence dies");
    assert!(token_revoked_at.is_some());
}

#[tokio::test]
async fn a_required_dependency_refuses_and_writes_nothing() {
    let pool = fresh().await;
    seed_root_and_dependency(&pool).await;
    seed_token(&pool, TOKEN_B, LICENSE_B, "raw-token-b").await;

    let outcome = revoke_license_releasing_dependencies(&pool, LICENSE_B, ACTOR, true)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        Outcome::DependencyStillRequired {
            required_by: vec![RequiredBy {
                license_id: LICENSE_A,
                tenant_id: TENANT,
                app_key: "app-a".to_owned(),
            }],
        }
    );
    let (status, revoked_at, metadata) = licence_state(&pool, LICENSE_B).await;
    assert_eq!(status, "active", "the refused revoke wrote nothing");
    assert!(revoked_at.is_none());
    assert!(metadata.is_none());
    let (revoked, token_revoked_at) = token_state(&pool, TOKEN_B).await;
    assert!(!revoked, "the dependency's tokens are untouched");
    assert!(token_revoked_at.is_none());
    let (root_status, _, _) = licence_state(&pool, LICENSE_A).await;
    assert_eq!(root_status, "active", "the requiring licence is untouched");
}

#[tokio::test]
async fn revoking_the_last_requiring_licence_releases_the_orphan() {
    let pool = fresh().await;
    seed_root_and_dependency(&pool).await;
    seed_grant(&pool, APP_B).await;

    let outcome = revoke_license_releasing_dependencies(&pool, LICENSE_A, ACTOR, false)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        Outcome::Revoked {
            released_ids: vec![LICENSE_B]
        }
    );
    let (status, revoked_at, metadata) = licence_state(&pool, LICENSE_B).await;
    assert_eq!(status, "revoked");
    assert!(revoked_at.is_some());
    let metadata = metadata.expect("the release is audited in metadata");
    assert_eq!(metadata["released_by"], ACTOR);
    assert_eq!(metadata["release_reason"], DEPENDENCY_RELEASE_REASON);
    assert_eq!(
        metadata["released_by_revoke_of"],
        LICENSE_A.to_string(),
        "the audit names the licence whose revoke released it"
    );
    assert!(metadata["released_at"].is_string());
    assert_eq!(
        grant_count(&pool, APP_B).await,
        0,
        "the released app's dependency grant is removed"
    );
}

#[tokio::test]
async fn a_released_dependency_has_its_tokens_revoked() {
    let pool = fresh().await;
    seed_root_and_dependency(&pool).await;
    seed_token(&pool, TOKEN_B, LICENSE_B, "raw-token-b").await;

    let outcome = revoke_license_releasing_dependencies(&pool, LICENSE_A, ACTOR, false)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        Outcome::Revoked {
            released_ids: vec![LICENSE_B]
        }
    );

    let (revoked, revoked_at) = token_state(&pool, TOKEN_B).await;
    assert!(revoked, "a released dependency's tokens die with it");
    assert!(revoked_at.is_some());
}

#[tokio::test]
async fn a_missing_licence_is_not_found() {
    let pool = fresh().await;
    seed_world(&pool).await;
    let missing = Uuid::from_u128(0x0000_0000_0000_7d00_8000_0000_0000_00ff);

    let outcome = revoke_license_releasing_dependencies(&pool, missing, ACTOR, false)
        .await
        .unwrap();

    assert_eq!(outcome, Outcome::NotFound);
}

#[tokio::test]
async fn an_unrequired_dependency_revoked_by_id_is_audited_without_being_listed() {
    let pool = fresh().await;
    seed_world(&pool).await;
    seed_app(&pool, APP_B, PLAN_B, "app-b").await;
    seed_licence(&pool, LICENSE_B, APP_B, PLAN_B, true, None).await;
    seed_token(&pool, TOKEN_B, LICENSE_B, "raw-token-b").await;
    seed_grant(&pool, APP_B).await;

    let outcome = revoke_license_releasing_dependencies(&pool, LICENSE_B, ACTOR, false)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        Outcome::Revoked {
            released_ids: Vec::new()
        },
        "the explicitly revoked licence is not in the released list"
    );
    let (status, _, metadata) = licence_state(&pool, LICENSE_B).await;
    assert_eq!(status, "revoked");
    let metadata = metadata.expect("an explicit dependency revoke is audited");
    assert_eq!(
        metadata["release_reason"],
        DEPENDENCY_EXPLICIT_REVOKE_REASON
    );
    assert_eq!(metadata["released_by"], ACTOR);
    assert_eq!(metadata["released_by_revoke_of"], LICENSE_B.to_string());
    let (revoked, _) = token_state(&pool, TOKEN_B).await;
    assert!(revoked);
    assert_eq!(
        grant_count(&pool, APP_B).await,
        0,
        "its grant row is cleaned up like a released one"
    );
}

#[tokio::test]
async fn a_bundle_callers_edge_keeps_its_dependency_grant() {
    let pool = fresh().await;
    seed_world(&pool).await;
    seed_app(&pool, APP_A, PLAN_A, "app-a").await;
    seed_app(&pool, APP_B, PLAN_B, "app-b").await;
    seed_app(&pool, APP_C, PLAN_C, "app-c").await;
    seed_bundle(&pool).await;
    // TWO bundle-minted roots share the bundle: revoking C leaves A active, so `bundle_ids` is
    // still populated when the release runs (the reference reads roots AFTER the revoke).
    seed_licence(&pool, LICENSE_A, APP_A, PLAN_A, false, Some(BUNDLE)).await;
    seed_licence(&pool, LICENSE_C, APP_C, PLAN_C, false, Some(BUNDLE)).await;
    seed_licence(&pool, LICENSE_B, APP_B, PLAN_B, true, None).await;
    // A held bundle's caller edge keeps owner-a -> app-b alive even though no licence walks it.
    sqlx::query("INSERT INTO bundle_caller (bundle_id, owner_key, app_id) VALUES ($1, $2, $3)")
        .bind(BUNDLE)
        .bind(OWNER)
        .bind(APP_B)
        .execute(&pool)
        .await
        .expect("bundle_caller");
    seed_grant(&pool, APP_B).await;

    let outcome = revoke_license_releasing_dependencies(&pool, LICENSE_C, ACTOR, false)
        .await
        .unwrap();

    assert_eq!(
        outcome,
        Outcome::Revoked {
            released_ids: vec![LICENSE_B]
        },
        "no remaining licence walks A -> B, so B is released"
    );
    assert_eq!(
        grant_count(&pool, APP_B).await,
        1,
        "a held bundle's bundle_caller edge keeps the grant"
    );
}
