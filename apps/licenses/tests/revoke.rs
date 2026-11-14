//! Behaviour of `POST /license/revoke`, both arms.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline. The
//! route is mounted on its own router with an injected `Extension<User>`, because these tests
//! exercise the handler's authorization and the revoke write — the auth middleware has its own
//! battery in `tests/introspect.rs`.

mod common;

use std::sync::Arc;

use axum::Extension;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post as route_post;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::tokens::revoke;
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
const DEP_LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000a);

const RAW: &str = "raw-licence-token";

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

/// The route-level app: no auth middleware, the caller is the injected `Extension<User>`.
fn revoke_app(db: &common::TestDb, user: User) -> axum::Router {
    axum::Router::new()
        .route("/license/revoke", route_post(revoke))
        .layer(Extension(user))
        .with_state(state(db.pool.clone()))
}

fn user(tenant: Uuid, roles: &[&str]) -> User {
    User {
        sub: "0195f48b-d000-7000-8000-0000000000aa".to_owned(),
        preferred_username: "operator".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: Some(tenant.to_string()),
        owner_key: Some("owner-a".to_owned()),
        ..User::default()
    }
}

fn admin(tenant: Uuid) -> User {
    user(tenant, &["license-admin"])
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
    // The caller's owner must ISSUE the app for the issuance core to admit the revoke.
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

#[tokio::test]
async fn a_raw_token_is_revoked_and_reports_success() {
    let db = seeded().await;
    let (status, body) = post(revoke_app(&db, admin(TENANT)), json!({"token": RAW})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);
    assert_eq!(body["message"], "Token revoked successfully");
    // The reference model defaults this to [], never null.
    assert_eq!(body["released_dependency_ids"], json!([]));

    let revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("token state");
    assert!(revoked);
    let revoked_at: Option<String> =
        sqlx::query_scalar("SELECT revoked_at::text FROM token WHERE id = $1")
            .bind(TOKEN)
            .fetch_one(&db.pool)
            .await
            .expect("revoked_at");
    assert!(revoked_at.is_some());
}

#[tokio::test]
async fn revoking_an_already_revoked_token_is_idempotent() {
    let db = seeded().await;
    sqlx::query(
        "UPDATE token SET revoked = true, revoked_at = '2020-01-01T00:00:00Z' WHERE id = $1",
    )
    .bind(TOKEN)
    .execute(&db.pool)
    .await
    .expect("pre-revoked");

    let (status, body) = post(revoke_app(&db, admin(TENANT)), json!({"token": RAW})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);
    assert_eq!(body["message"], "Token was already revoked");
    assert_eq!(body["released_dependency_ids"], json!([]));

    // The original revoked_at is the audit record; a retry must not overwrite it.
    let revoked_at: String = sqlx::query_scalar("SELECT revoked_at::text FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("revoked_at");
    assert!(revoked_at.starts_with("2020-01-01"), "{revoked_at}");
}

#[tokio::test]
async fn an_unknown_token_and_a_foreign_tenant_token_share_one_answer() {
    let db = seeded().await;

    let (status, body) = post(
        revoke_app(&db, admin(TENANT)),
        json!({"token": "never-issued"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], false);
    assert_eq!(body["message"], "Token not found or already invalid");

    // The token exists, but on ANOTHER tenant: the answer must be identical, on purpose.
    let (status, body) = post(revoke_app(&db, admin(OTHER_TENANT)), json!({"token": RAW})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], false);
    assert_eq!(body["message"], "Token not found or already invalid");

    // Nothing was written on either answer.
    let revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("token state");
    assert!(!revoked);
}

#[tokio::test]
async fn a_licence_revoke_revokes_its_tokens() {
    let db = seeded().await;
    let (status, body) = post(
        revoke_app(&db, admin(TENANT)),
        json!({"license_id": LICENSE.to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);
    assert_eq!(
        body["message"],
        "License and all tokens revoked successfully"
    );
    assert_eq!(body["released_dependency_ids"], json!([]));

    let license_status: String = sqlx::query_scalar("SELECT status FROM license WHERE id = $1")
        .bind(LICENSE)
        .fetch_one(&db.pool)
        .await
        .expect("license state");
    assert_eq!(license_status, "revoked");
    let token_revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE id = $1")
        .bind(TOKEN)
        .fetch_one(&db.pool)
        .await
        .expect("token state");
    assert!(token_revoked);
}

#[tokio::test]
async fn a_required_dependency_is_a_409_naming_its_requirers() {
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO app (id, key, name, created_at) VALUES ($1, 'dep', 'Dependency', now())",
    )
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("dep app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at)
         VALUES ($1, $2, 'dep', 'Dependency', now())",
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
    .expect("dep license");
    // The tenant's remaining active licence ('demo') requires the dependency's app.
    sqlx::query(
        "INSERT INTO app_requires (app_id, required_app_id, caller_owner_key)
         VALUES ($1, $2, 'owner-a')",
    )
    .bind(APP)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("app_requires");

    let (status, body) = post(
        revoke_app(&db, admin(TENANT)),
        json!({"license_id": DEP_LICENSE.to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["code"], "dependency_still_required");
    assert_eq!(body["detail"]["error"], "dependency_still_required");
    assert_eq!(body["detail"]["license_id"], DEP_LICENSE.to_string());
    assert_eq!(
        body["detail"]["message"],
        "a remaining active licence on this tenant still requires this dependency; revoke that \
         licence and the dependency is released"
    );
    assert_eq!(
        body["detail"]["required_by"]
            .as_array()
            .expect("list")
            .len(),
        1
    );
    assert_eq!(
        body["detail"]["required_by"][0]["license_id"],
        LICENSE.to_string()
    );
    assert_eq!(
        body["detail"]["required_by"][0]["tenant_id"],
        TENANT.to_string()
    );
    assert_eq!(body["detail"]["required_by"][0]["app_key"], "demo");

    // Nothing was revoked.
    let license_status: String = sqlx::query_scalar("SELECT status FROM license WHERE id = $1")
        .bind(DEP_LICENSE)
        .fetch_one(&db.pool)
        .await
        .expect("dep state");
    assert_eq!(license_status, "active");
}

#[tokio::test]
async fn neither_field_is_a_400() {
    let db = seeded().await;
    let (status, body) = post(revoke_app(&db, admin(TENANT)), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["detail"],
        "Either 'token' or 'license_id' must be provided"
    );
}

#[tokio::test]
async fn a_role_other_than_admin_or_manager_is_forbidden() {
    let db = seeded().await;
    let (status, body) = post(
        revoke_app(&db, user(TENANT, &["license-reader"])),
        json!({"token": RAW}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"],
        "One of roles required: license-admin, license-manager"
    );
}
