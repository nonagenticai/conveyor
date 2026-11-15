//! Behaviour of the three licence-token write routes.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline. The
//! routes are mounted with an injected `Extension<User>` (the auth middleware has its own battery
//! in `tests/introspect.rs`), so these tests exercise the handlers' role rules, the expiry policy,
//! the stored rows and the rotation transaction.

mod common;

use std::sync::Arc;

use axum::Extension;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post as route_post;
use chrono::{DateTime, Duration, Utc};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::licence_tokens::{
    create_license_token, create_operator_token, regenerate_license_token,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0005);
const TOKEN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0006);
const OTHER_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0007);
const DEP_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0008);
const DEP_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0009);
const DEP_LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_000a);

/// The subject's Keycloak `sub`; operator-token mints must be for this caller.
const USER_SUB: &str = "0195f48b-d000-7000-8000-0000000000aa";
const SECURE_APPS_ISSUER: &str = "https://auth.nonagentic.ai/realms/secure-apps";
const REGENERATE_MESSAGE: &str = "Save this token - it won't be shown again";

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

fn licence_app(db: &common::TestDb, user: User) -> axum::Router {
    axum::Router::new()
        .route(
            "/licenses/{license_id}/tokens",
            route_post(create_license_token),
        )
        .route(
            "/licenses/{license_id}/operator-token",
            route_post(create_operator_token),
        )
        .route(
            "/licenses/{license_id}/regenerate-token",
            route_post(regenerate_license_token),
        )
        .layer(Extension(user))
        .with_state(state(db.pool.clone()))
}

fn user(tenant: Uuid, roles: &[&str]) -> User {
    User {
        sub: USER_SUB.to_owned(),
        preferred_username: "operator".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: Some(tenant.to_string()),
        owner_key: Some("owner-a".to_owned()),
        iss: Some("https://auth.example/realms/portal".to_owned()),
        azp: Some("licensing-tests".to_owned()),
        ..User::default()
    }
}

fn admin(tenant: Uuid) -> User {
    user(tenant, &["license-admin"])
}

fn manager(tenant: Uuid) -> User {
    user(tenant, &["license-manager"])
}

fn reader(tenant: Uuid) -> User {
    user(tenant, &["license-reader"])
}

fn platform_admin(tenant: Uuid) -> User {
    user(tenant, &["platform-admin"])
}

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
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(SUBJECT)
    .bind(USER_SUB)
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
    db
}

#[allow(clippy::too_many_arguments)]
async fn insert_token(
    db: &common::TestDb,
    id: Uuid,
    raw: &str,
    expires_at: Option<DateTime<Utc>>,
    revoked: bool,
    long_lived_owner: Option<&str>,
    long_lived_reason: Option<&str>,
    roles: &[&str],
) {
    sqlx::query(
        "INSERT INTO token
             (id, license_id, token_hash, token_type, created_at, expires_at, revoked, revoked_at,
              long_lived_owner, long_lived_reason, granted_roles)
         VALUES ($1, $2, $3, 'api', now(), $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(LICENSE)
    .bind(db::tokens::hash_token(raw))
    .bind(expires_at)
    .bind(revoked)
    .bind(revoked.then(Utc::now))
    .bind(long_lived_owner)
    .bind(long_lived_reason)
    .bind(
        roles
            .iter()
            .map(|role| (*role).to_owned())
            .collect::<Vec<String>>(),
    )
    .execute(&db.pool)
    .await
    .expect("token");
}

async fn request(
    app: axum::Router,
    uri: &str,
    header: Option<(&str, &str)>,
) -> (StatusCode, Value) {
    let mut builder = Request::post(uri);
    if let Some((name, value)) = header {
        builder = builder.header(name, value);
    }
    let response = app
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn post(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    request(app, uri, None).await
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("an object body")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn tokens_uri() -> String {
    format!("/licenses/{LICENSE}/tokens")
}

fn operator_uri() -> String {
    format!("/licenses/{LICENSE}/operator-token")
}

fn regenerate_uri() -> String {
    format!("/licenses/{LICENSE}/regenerate-token")
}

async fn token_count(db: &common::TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM token WHERE license_id = $1")
        .bind(LICENSE)
        .fetch_one(&db.pool)
        .await
        .expect("count")
}

async fn stored_expiry(db: &common::TestDb, id: Uuid) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT expires_at FROM token WHERE id = $1")
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .expect("expires_at")
}

// ── create_license_token ────────────────────────────────────────────────────

#[tokio::test]
async fn an_api_token_without_expiry_gets_the_bounded_default() {
    let db = seeded().await;
    let before = Utc::now();
    let (status, body) = post(licence_app(&db, admin(TENANT)), &tokens_uri()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        sorted_keys(&body),
        [
            "created_at",
            "expires_at",
            "license_id",
            "long_lived_owner",
            "token",
            "token_id",
            "token_type"
        ]
    );
    assert_eq!(body["license_id"], LICENSE.to_string());
    assert_eq!(body["token_type"], "api");
    assert_eq!(body["long_lived_owner"], Value::Null);
    assert!(body["created_at"].as_str().unwrap().ends_with("+00:00"));

    let token_id = Uuid::try_parse(body["token_id"].as_str().expect("token_id")).expect("uuid");
    let (hash, token_type, granted_roles, owner): (String, String, Vec<String>, Option<String>) =
        sqlx::query_as(
            "SELECT token_hash, token_type, granted_roles, long_lived_owner
               FROM token WHERE id = $1",
        )
        .bind(token_id)
        .fetch_one(&db.pool)
        .await
        .expect("token row");
    assert_eq!(
        hash,
        db::tokens::hash_token(body["token"].as_str().expect("raw"))
    );
    assert_eq!(token_type, "api");
    assert!(granted_roles.is_empty());
    assert_eq!(owner, None);

    let expires_at = stored_expiry(&db, token_id).await.expect("an expiry");
    let days = expires_at.signed_duration_since(before).num_days();
    assert!((364..=366).contains(&days), "{days}");
}

#[tokio::test]
async fn an_explicit_expiry_wins_over_the_default() {
    let db = seeded().await;
    let before = Utc::now();
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!("{}?expires_days=2", tokens_uri()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token_id = Uuid::try_parse(body["token_id"].as_str().expect("token_id")).expect("uuid");
    let expires_at = stored_expiry(&db, token_id).await.expect("an expiry");
    let hours = expires_at.signed_duration_since(before).num_hours();
    assert!((47..=49).contains(&hours), "{hours}");
}

#[tokio::test]
async fn an_internal_token_without_expiry_is_refused_and_writes_nothing() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!("{}?token_type=internal", tokens_uri()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"]["error"], "perpetual_token_not_registered");
    assert_eq!(
        body["detail"]["remedy"],
        "pass expires_days, or long_lived=true with long_lived_owner and long_lived_reason"
    );
    assert_eq!(token_count(&db).await, 0);
}

#[tokio::test]
async fn a_registered_long_lived_internal_token_is_perpetual() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!(
            "{}?token_type=internal&long_lived=true&long_lived_owner=ops&long_lived_reason=permanent",
            tokens_uri()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["expires_at"], Value::Null);
    assert_eq!(body["long_lived_owner"], "ops");

    let token_id = Uuid::try_parse(body["token_id"].as_str().expect("token_id")).expect("uuid");
    let (expires_at, owner, reason): (Option<DateTime<Utc>>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT expires_at, long_lived_owner, long_lived_reason FROM token WHERE id = $1",
        )
        .bind(token_id)
        .fetch_one(&db.pool)
        .await
        .expect("token row");
    assert_eq!(expires_at, None);
    assert_eq!(owner.as_deref(), Some("ops"));
    assert_eq!(reason.as_deref(), Some("permanent"));
}

#[tokio::test]
async fn a_long_lived_mint_requires_license_admin() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, manager(TENANT)),
        &format!(
            "{}?token_type=internal&long_lived=true&long_lived_owner=ops&long_lived_reason=permanent",
            tokens_uri()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "long_lived_requires_license_admin");
    assert_eq!(token_count(&db).await, 0);

    // The same actor may still mint a bounded token.
    let (status, body) = post(
        licence_app(&db, manager(TENANT)),
        &format!("{}?expires_days=1", tokens_uri()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn a_long_lived_token_with_an_expiry_is_refused() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!(
            "{}?token_type=internal&long_lived=true&long_lived_owner=ops&long_lived_reason=permanent&expires_days=1",
            tokens_uri()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"]["error"], "long_lived_with_expiry");
}

#[tokio::test]
async fn registration_fields_without_long_lived_are_refused() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!("{}?long_lived_owner=ops", tokens_uri()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["detail"]["error"],
        "long_lived_fields_without_long_lived"
    );
}

#[tokio::test]
async fn a_long_lived_api_token_is_allowed_on_an_allowlisted_tenant() {
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET name = 'n8n-dev' WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("rename tenant");
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!(
            "{}?long_lived=true&long_lived_owner=ops&long_lived_reason=permanent",
            tokens_uri()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["expires_at"], Value::Null);
}

#[tokio::test]
async fn a_long_lived_api_token_off_the_allowlist_is_refused() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!(
            "{}?long_lived=true&long_lived_owner=ops&long_lived_reason=permanent",
            tokens_uri()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"]["error"], "long_lived_not_permitted");
}

#[tokio::test]
async fn the_registered_implicit_client_gets_a_perpetual_registered_token() {
    let db = seeded().await;
    let mut implicit = admin(TENANT);
    implicit.iss = Some(SECURE_APPS_ISSUER.to_owned());
    implicit.azp = Some("commonpractice-licenses-admin".to_owned());
    let (status, body) = post(licence_app(&db, implicit), &tokens_uri()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["expires_at"], Value::Null);
    assert_eq!(body["long_lived_owner"], "m4 / commonpractice");
}

#[tokio::test]
async fn create_requires_a_write_role() {
    let db = seeded().await;
    let (status, body) = post(licence_app(&db, reader(TENANT)), &tokens_uri()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"],
        "One of roles required: license-admin, license-manager"
    );
}

#[tokio::test]
async fn an_unknown_or_foreign_licence_is_one_404() {
    let db = seeded().await;
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!("/licenses/{}/tokens", Uuid::nil()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"], "License not found");

    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-b', now(), 'owner-a', 'local_allowed')",
    )
    .bind(OTHER_TENANT)
    .execute(&db.pool)
    .await
    .expect("other tenant");
    sqlx::query("UPDATE license SET tenant_id = $1 WHERE id = $2")
        .bind(OTHER_TENANT)
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("retenant");
    let (status, body) = post(licence_app(&db, admin(TENANT)), &tokens_uri()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"], "License not found");
}

#[tokio::test]
async fn an_archived_tenant_is_refused_with_a_400() {
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");
    let (status, body) = post(licence_app(&db, admin(TENANT)), &tokens_uri()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["detail"],
        "refusing to mint: tenant is archived — un-archive the tenant before creating a token"
    );
    assert_eq!(token_count(&db).await, 0);
}

#[tokio::test]
async fn a_dead_licence_is_a_409_naming_its_effective_state() {
    let db = seeded().await;
    sqlx::query("UPDATE license SET status = 'revoked' WHERE id = $1")
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("revoke licence");
    let (status, body) = post(licence_app(&db, admin(TENANT)), &tokens_uri()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "license_not_active");
    assert_eq!(body["detail"]["license_id"], LICENSE.to_string());
    assert_eq!(body["detail"]["status"], "revoked");
    assert_eq!(body["detail"]["stored_status"], "revoked");
    assert_eq!(token_count(&db).await, 0);
}

#[tokio::test]
async fn a_forward_dated_licence_may_still_be_minted_on() {
    let db = seeded().await;
    sqlx::query("UPDATE license SET starts_at = now() + interval '1 day' WHERE id = $1")
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("forward date");
    let (status, body) = post(licence_app(&db, admin(TENANT)), &tokens_uri()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn a_dependency_licence_is_bundle_token_only() {
    let db = seeded().await;
    sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, 'dep', 'Dep', now())")
        .bind(DEP_APP)
        .execute(&db.pool)
        .await
        .expect("dep app");
    sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, 'owner-a')")
        .bind(DEP_APP)
        .execute(&db.pool)
        .await
        .expect("dep issuer");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, 'dep', 'Dep', now())",
    )
    .bind(DEP_PLAN)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("dep plan");
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
    .expect("dep licence");
    let (status, body) = post(
        licence_app(&db, admin(TENANT)),
        &format!("/licenses/{DEP_LICENSE}/tokens"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "dependency_licence");
    assert_eq!(body["detail"]["license_id"], DEP_LICENSE.to_string());
}

#[tokio::test]
async fn a_caller_that_does_not_issue_the_app_is_forbidden() {
    let db = seeded().await;
    let mut outsider = admin(TENANT);
    outsider.owner_key = Some("owner-b".to_owned());
    let (status, body) = post(licence_app(&db, outsider), &tokens_uri()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_issuable_by_caller");
    assert_eq!(token_count(&db).await, 0);
}

// ── create_operator_token ───────────────────────────────────────────────────

#[tokio::test]
async fn an_operator_token_carries_platform_admin_and_a_short_expiry() {
    let db = seeded().await;
    let before = Utc::now();
    let (status, body) = post(
        licence_app(&db, platform_admin(TENANT)),
        &format!("{}?ttl_minutes=5", operator_uri()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        sorted_keys(&body),
        [
            "created_at",
            "expires_at",
            "license_id",
            "long_lived_owner",
            "roles",
            "token",
            "token_id",
            "token_type"
        ]
    );
    assert_eq!(body["roles"], json!(["platform-admin"]));
    assert_eq!(body["token_type"], "api");
    assert_eq!(body["long_lived_owner"], Value::Null);

    let token_id = Uuid::try_parse(body["token_id"].as_str().expect("token_id")).expect("uuid");
    let (hash, roles, expires_at, token_type): (
        String,
        Vec<String>,
        Option<DateTime<Utc>>,
        String,
    ) = sqlx::query_as(
        "SELECT token_hash, granted_roles, expires_at, token_type FROM token WHERE id = $1",
    )
    .bind(token_id)
    .fetch_one(&db.pool)
    .await
    .expect("token row");
    assert_eq!(
        hash,
        db::tokens::hash_token(body["token"].as_str().expect("raw"))
    );
    assert_eq!(roles, vec!["platform-admin".to_owned()]);
    assert_eq!(token_type, "api");
    let seconds = expires_at
        .expect("operator tokens always expire")
        .signed_duration_since(before)
        .num_seconds();
    assert!((290..=301).contains(&seconds), "{seconds}");
}

#[tokio::test]
async fn an_operator_token_requires_platform_admin() {
    let db = seeded().await;
    let (status, body) = post(licence_app(&db, admin(TENANT)), &operator_uri()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"], "Role 'platform-admin' required");
    assert_eq!(token_count(&db).await, 0);
}

#[tokio::test]
async fn the_operator_ttl_is_capped_at_sixty_minutes() {
    let db = seeded().await;
    for ttl in ["0", "61"] {
        let (status, body) = post(
            licence_app(&db, platform_admin(TENANT)),
            &format!("{}?ttl_minutes={ttl}", operator_uri()),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(body["detail"].is_array(), "{body}");
    }
    assert_eq!(token_count(&db).await, 0);

    // The ceiling itself is allowed.
    let (status, body) = post(
        licence_app(&db, platform_admin(TENANT)),
        &format!("{}?ttl_minutes=60", operator_uri()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn an_operator_cannot_mint_for_another_persons_licence() {
    let db = seeded().await;
    let mut other = platform_admin(TENANT);
    other.sub = "0195f48b-d000-7000-8000-0000000000bb".to_owned();
    let (status, body) = post(licence_app(&db, other), &operator_uri()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"], "License not found");
    assert_eq!(token_count(&db).await, 0);
}

#[tokio::test]
async fn the_operator_tenant_header_must_name_the_licences_tenant() {
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-b', now(), 'owner-a', 'local_allowed')",
    )
    .bind(OTHER_TENANT)
    .execute(&db.pool)
    .await
    .expect("other tenant");

    let (status, body) = request(
        licence_app(&db, platform_admin(TENANT)),
        &operator_uri(),
        Some(("x-tenant-id", &OTHER_TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"],
        "X-Tenant-Id does not match the licence's tenant"
    );

    // The licence's own tenant (any case — a UUID comparison) is accepted.
    let (status, body) = request(
        licence_app(&db, platform_admin(TENANT)),
        &operator_uri(),
        Some(("x-tenant-id", &TENANT.to_string().to_uppercase())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn an_inactive_licence_cannot_mint_an_operator_token() {
    let db = seeded().await;
    sqlx::query("UPDATE license SET expires_at = now() - interval '1 hour' WHERE id = $1")
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("expire");
    let (status, body) = post(licence_app(&db, platform_admin(TENANT)), &operator_uri()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"], "refusing to mint: licence is not active");
}

#[tokio::test]
async fn an_archived_tenant_cannot_mint_an_operator_token() {
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");
    let (status, body) = post(licence_app(&db, platform_admin(TENANT)), &operator_uri()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"], "refusing to mint: tenant is archived");
}

// ── regenerate_license_token ────────────────────────────────────────────────

#[tokio::test]
async fn regenerate_revokes_the_old_tokens_and_returns_a_usable_replacement() {
    let db = seeded().await;
    let old_expiry = Utc::now() + Duration::days(10);
    insert_token(
        &db,
        TOKEN,
        "old-token",
        Some(old_expiry),
        false,
        None,
        None,
        &[],
    )
    .await;

    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(sorted_keys(&body), ["expires_at", "message", "token"]);
    assert_eq!(body["message"], REGENERATE_MESSAGE);
    let raw = body["token"].as_str().expect("raw token").to_owned();

    let old_revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("old token");
    assert!(old_revoked);
    assert_eq!(token_count(&db).await, 2);

    // The replacement inherits the expiry of the token it replaces.
    let new_expiry: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT expires_at FROM token WHERE token_hash = $1")
            .bind(db::tokens::hash_token(&raw))
            .fetch_one(&db.pool)
            .await
            .expect("new token");
    // Same precision rule as `regenerate_inherits_the_latest_replaced_expiry`: the row holds
    // microseconds, `old_expiry` carries nanoseconds.
    assert_eq!(
        new_expiry.map(|expiry| expiry.timestamp_micros()),
        Some(old_expiry.timestamp_micros())
    );

    // And it is a real credential: introspection answers ACTIVE.
    match db::tokens::introspect(&db.pool, &raw)
        .await
        .expect("introspect")
    {
        db::tokens::Outcome::Active(active) => {
            assert_eq!(active.license_id, LICENSE.to_string());
            assert!(active.roles.is_empty());
        }
        db::tokens::Outcome::Inactive(reason) => panic!("replacement is inactive: {reason:?}"),
    }
}

#[tokio::test]
async fn regenerate_requires_a_write_role() {
    let db = seeded().await;
    let (status, body) = post(licence_app(&db, reader(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"],
        "One of roles required: license-admin, license-manager"
    );
}

#[tokio::test]
async fn regenerate_answers_not_your_license_for_a_foreign_tenant() {
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-b', now(), 'owner-a', 'local_allowed')",
    )
    .bind(OTHER_TENANT)
    .execute(&db.pool)
    .await
    .expect("other tenant");
    sqlx::query("UPDATE license SET tenant_id = $1 WHERE id = $2")
        .bind(OTHER_TENANT)
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("retenant");
    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"], "Not your license");
}

#[tokio::test]
async fn regenerate_refuses_a_dead_licence_before_destroying_anything() {
    let db = seeded().await;
    insert_token(
        &db,
        TOKEN,
        "live-token",
        Some(Utc::now() + Duration::days(5)),
        false,
        None,
        None,
        &[],
    )
    .await;
    sqlx::query("UPDATE license SET status = 'revoked' WHERE id = $1")
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("revoke licence");
    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "license_not_active");
    assert_eq!(body["detail"]["status"], "revoked");

    let revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("token");
    assert!(!revoked);
}

#[tokio::test]
async fn regenerate_refuses_when_every_token_is_revoked() {
    let db = seeded().await;
    insert_token(&db, TOKEN, "revoked-token", None, true, None, None, &[]).await;
    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "all_tokens_revoked");
    assert_eq!(body["detail"]["license_id"], LICENSE.to_string());
    assert_eq!(
        body["detail"]["remedy"],
        "mint a new token via POST /licenses/{license_id}/tokens"
    );
    assert_eq!(token_count(&db).await, 1);
}

#[tokio::test]
async fn regenerate_refuses_when_every_unrevoked_token_has_expired() {
    let db = seeded().await;
    insert_token(
        &db,
        TOKEN,
        "expired-token",
        Some(Utc::now() - Duration::days(1)),
        false,
        None,
        None,
        &[],
    )
    .await;
    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "all_tokens_expired");

    // The expired token was NOT revoked: the refusal writes nothing.
    let revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("token");
    assert!(!revoked);
}

#[tokio::test]
async fn regenerate_on_a_licence_that_never_had_a_token_applies_the_new_mint_policy() {
    let db = seeded().await;
    let before = Utc::now();
    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let raw = body["token"].as_str().expect("raw");
    let expires_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT expires_at FROM token WHERE token_hash = $1")
            .bind(db::tokens::hash_token(raw))
            .fetch_one(&db.pool)
            .await
            .expect("new token");
    let days = expires_at
        .expect("the bounded default, never perpetuity")
        .signed_duration_since(before)
        .num_days();
    assert!((364..=366).contains(&days), "{days}");
}

#[tokio::test]
async fn regenerate_inherits_the_latest_replaced_expiry() {
    let db = seeded().await;
    // Postgres stores timestamptz at MICROSECOND precision; the in-memory source carries
    // nanoseconds. The assertion below compares at microseconds for that reason (nextest
    // exposed the raw comparison in CI: the row's microsecond round-trip vs the nanosecond
    // source differed whenever Utc::now() ended in a non-zero sub-microsecond).
    let soon = Utc::now() + Duration::days(10);
    let later = Utc::now() + Duration::days(20);
    insert_token(&db, TOKEN, "ten-day", Some(soon), false, None, None, &[]).await;
    insert_token(
        &db,
        Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000b),
        "twenty-day",
        Some(later),
        false,
        None,
        None,
        &[],
    )
    .await;

    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let raw = body["token"].as_str().expect("raw");
    let new_expiry: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT expires_at FROM token WHERE token_hash = $1")
            .bind(db::tokens::hash_token(raw))
            .fetch_one(&db.pool)
            .await
            .expect("new token");
    // Postgres stores timestamptz at MICROSECOND precision while the in-memory value carries
    // nanoseconds; compare at the precision the row can hold. (nextest exposed the raw
    // comparison in CI: microsecond round-trip vs the nanosecond source.)
    assert_eq!(
        new_expiry.map(|expiry| expiry.timestamp_micros()),
        Some(later.timestamp_micros())
    );

    let live: i64 = sqlx::query_scalar("SELECT count(*) FROM token WHERE NOT revoked")
        .fetch_one(&db.pool)
        .await
        .expect("live count");
    assert_eq!(live, 1);
}

#[tokio::test]
async fn regenerate_keeps_a_perpetual_predecessors_registration() {
    let db = seeded().await;
    insert_token(
        &db,
        TOKEN,
        "perpetual-token",
        None,
        false,
        Some("ops"),
        Some("permanent"),
        &[],
    )
    .await;
    let (status, body) = post(licence_app(&db, admin(TENANT)), &regenerate_uri()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expires_at"], Value::Null);
    let raw = body["token"].as_str().expect("raw");
    let (owner, reason): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT long_lived_owner, long_lived_reason FROM token WHERE token_hash = $1",
    )
    .bind(db::tokens::hash_token(raw))
    .fetch_one(&db.pool)
    .await
    .expect("new token");
    assert_eq!(owner.as_deref(), Some("ops"));
    assert_eq!(reason.as_deref(), Some("permanent"));
}
