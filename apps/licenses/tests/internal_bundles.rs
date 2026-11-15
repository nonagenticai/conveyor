//! Behaviour of `POST /internal/tenants/{tenant_id}/bundle-token`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline. The
//! route is mounted on its own router with an injected `Extension<User>`: these tests exercise the
//! mint's authorization and selection, while the auth middleware has its own battery in
//! `tests/introspect.rs`. The happy path ALSO goes through the real router's `/license/introspect`,
//! because "the raw token works" is the whole point of the route.

mod common;

use std::sync::Arc;

use axum::Extension;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post as route_post;
use chrono::{DateTime, Utc};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router;
use conveyor_licenses::router::internal_bundles::internal_mint_tenant_bundle_token;
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
const OTHER_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0002);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
const BUNDLE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0005);
const DEP_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0006);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0007);
const DEP_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0008);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0009);
const SUBJECT2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000a);
const SVC_SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000b);
const LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000c);
const LICENCE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000d);
const DEP_LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000e);

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

fn user_with(roles: &[&str], owner: Option<&str>, cross_owner: bool) -> User {
    User {
        sub: "0195f48b-d000-7000-8000-0000000000aa".to_owned(),
        preferred_username: "minter".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: Some(TENANT.to_string()),
        owner_key: owner.map(str::to_owned),
        cross_owner,
        ..User::default()
    }
}

fn minter() -> User {
    user_with(&["bundle-token-minter"], Some("owner-a"), false)
}

/// The route-level app: no auth middleware, the caller is the injected `Extension<User>`.
fn app(db: &common::TestDb, user: User) -> Router {
    Router::new()
        .route(
            "/internal/tenants/{tenant_id}/bundle-token",
            route_post(internal_mint_tenant_bundle_token),
        )
        .layer(Extension(user))
        .with_state(state(db.pool.clone()))
}

fn mint_path(tenant: Uuid, query: &str) -> String {
    format!("/internal/tenants/{tenant}/bundle-token?{query}")
}

async fn mint(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::post(path).body(Body::empty()).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// One tenant owned by owner-a, one foreign tenant owned by owner-a, both apps, their plans and
/// the two member subjects. Every per-test fixture (bundle, membership, licence) is added on top.
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
    .bind(OTHER_TENANT)
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
        "INSERT INTO plan (id, app_id, code, name, created_at)
         VALUES ($1, $2, 'basic', 'Basic', now()), ($3, $4, 'basic', 'Basic', now())",
    )
    .bind(PLAN)
    .bind(APP)
    .bind(DEP_PLAN)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("plans");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, '0195f48b-d000-7000-8000-000000000001', 'portal', now()),
                ($2, '0195f48b-d000-7000-8000-000000000002', 'portal', now()),
                ($3, $4, 'service', now())",
    )
    .bind(SUBJECT)
    .bind(SUBJECT2)
    .bind(SVC_SUBJECT)
    .bind(format!("svc:{TENANT}:workers"))
    .execute(&db.pool)
    .await
    .expect("subjects");
    db
}

async fn hold(db: &common::TestDb, tenant: Uuid, bundle: Uuid, key: &str) {
    sqlx::query("INSERT INTO bundle (id, key, name, created_at) VALUES ($1, $2, $2, now())")
        .bind(bundle)
        .bind(key)
        .execute(&db.pool)
        .await
        .expect("bundle");
    sqlx::query(
        "INSERT INTO tenant_bundle (tenant_id, bundle_id, created_at) VALUES ($1, $2, now())",
    )
    .bind(tenant)
    .bind(bundle)
    .execute(&db.pool)
    .await
    .expect("tenant_bundle");
}

async fn bundle_app(db: &common::TestDb, bundle: Uuid, app: Uuid) {
    sqlx::query("INSERT INTO bundle_app (bundle_id, app_id, created_at) VALUES ($1, $2, now())")
        .bind(bundle)
        .bind(app)
        .execute(&db.pool)
        .await
        .expect("bundle_app");
}

/// An ACTIVE, started, unexpired licence. `bundle` NULL = tenant-level.
async fn licence(
    db: &common::TestDb,
    licence: Uuid,
    tenant: Uuid,
    app: Uuid,
    plan: Uuid,
    subject: Uuid,
    bundle: Option<Uuid>,
) {
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id, bundle_id)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, $6)",
    )
    .bind(licence)
    .bind(subject)
    .bind(app)
    .bind(plan)
    .bind(tenant)
    .bind(bundle)
    .execute(&db.pool)
    .await
    .expect("license");
}

async fn requires(db: &common::TestDb, app: Uuid, required: Uuid) {
    sqlx::query(
        "INSERT INTO app_requires (app_id, required_app_id, caller_owner_key)
         VALUES ($1, $2, 'owner-a')",
    )
    .bind(app)
    .bind(required)
    .execute(&db.pool)
    .await
    .expect("app_requires");
}

/// The default happy-path fixture: one held bundle containing `demo`, with its bundle licence.
async fn seeded_with_bundle() -> common::TestDb {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "non-agentic-ai").await;
    bundle_app(&db, BUNDLE, APP).await;
    licence(&db, LICENCE, TENANT, APP, PLAN, SUBJECT, Some(BUNDLE)).await;
    db
}

#[tokio::test]
async fn a_mint_returns_the_seven_fields_and_the_raw_token_introspects() {
    let db = seeded_with_bundle().await;
    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let raw = body["token"].as_str().expect("token");
    // `secrets.token_urlsafe(32)`: 32 bytes, base64url without padding.
    assert_eq!(raw.len(), 43, "{raw}");
    assert!(!raw.contains('='), "{raw}");
    assert_eq!(body["tenant_id"], TENANT.to_string());
    assert_eq!(body["bundle"], "non-agentic-ai");
    assert_eq!(body["app"], "demo");
    assert_eq!(body["license_id"], LICENCE.to_string());

    let token_id = Uuid::parse_str(body["token_id"].as_str().expect("token_id")).expect("uuid");
    let expires_at = DateTime::parse_from_rfc3339(body["expires_at"].as_str().expect("expires_at"))
        .expect("an ISO-8601 instant");
    let remaining = expires_at.with_timezone(&Utc) - Utc::now();
    assert!(
        remaining.num_minutes() >= 14 && remaining.num_minutes() <= 15,
        "{remaining:?}"
    );

    // Only the hash is stored; the raw token exists in the response and nowhere else.
    let (hash, token_type, roles, stored_expiry): (String, String, Vec<String>, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT token_hash, token_type, granted_roles, expires_at
               FROM token WHERE id = $1",
        )
        .bind(token_id)
        .fetch_one(&db.pool)
        .await
        .expect("token row");
    assert_eq!(hash, db::tokens::hash_token(raw));
    assert_ne!(hash, raw);
    assert_eq!(token_type, "api");
    assert!(roles.is_empty(), "{roles:?}");
    assert_eq!(some_expiry(stored_expiry), body["expires_at"]);

    // The point of the mint: the raw token is a live credential on THIS licence.
    let full = router::router(state(db.pool.clone()));
    let response = full
        .oneshot(
            Request::post("/license/introspect")
                .header("content-type", "application/json")
                .body(Body::from(json!({"token": raw}).to_string()))
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    let introspected: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(introspected["active"], true, "{introspected}");
    assert_eq!(introspected["license_id"], LICENCE.to_string());
    assert_eq!(introspected["tenant_id"], TENANT.to_string());
    assert_eq!(introspected["app"]["key"], "demo");
}

fn some_expiry(value: DateTime<Utc>) -> Value {
    json!(conveyor_licenses::db::tokens::py_iso(value))
}

#[tokio::test]
async fn an_unknown_tenant_is_a_404_and_so_is_one_holding_no_bundle() {
    let db = seeded().await;
    let unknown = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_00ff);

    let (status, body) = mint(&app(&db, minter()), &mint_path(unknown, "app=demo")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "no_bundle");
    assert_eq!(body["detail"]["reason"], "tenant_not_found");
    assert_eq!(body["detail"]["app"], "demo");
    assert_eq!(body["detail"]["tenant_id"], unknown.to_string());

    // An owned tenant that holds no bundle at all has its own reason.
    let (status, body) = mint(&app(&db, minter()), &mint_path(OTHER_TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "no_bundle");
    assert_eq!(body["detail"]["reason"], "no_bundle_held");
}

#[tokio::test]
async fn an_archived_tenant_is_a_404_no_bundle() {
    let db = seeded_with_bundle().await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "no_bundle");
    assert_eq!(body["detail"]["reason"], "tenant_archived");
}

#[tokio::test]
async fn an_app_outside_every_held_bundle_is_a_404_app_not_in_bundle() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "non-agentic-ai").await;
    bundle_app(&db, BUNDLE, DEP_APP).await;

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_in_bundle");
    assert_eq!(body["detail"]["reason"], "app_not_in_any_held_bundle");
    assert_eq!(body["detail"]["bundles"], json!(["non-agentic-ai"]));
}

#[tokio::test]
async fn a_foreign_tenant_is_a_404_exactly_like_an_absent_one() {
    let db = seeded_with_bundle().await;
    let foreign = user_with(&["bundle-token-minter"], Some("owner-b"), false);

    let (status, body) = mint(&app(&db, foreign), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "no_bundle");
    assert_eq!(body["detail"]["reason"], "tenant_not_found");
}

#[tokio::test]
async fn two_held_bundles_with_different_licences_are_ambiguous() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "bundle-a").await;
    hold(&db, TENANT, BUNDLE2, "bundle-b").await;
    bundle_app(&db, BUNDLE, APP).await;
    bundle_app(&db, BUNDLE2, APP).await;
    licence(&db, LICENCE, TENANT, APP, PLAN, SUBJECT, Some(BUNDLE)).await;
    licence(&db, LICENCE2, TENANT, APP, PLAN, SUBJECT2, Some(BUNDLE2)).await;

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "ambiguous_bundle");
    assert_eq!(body["detail"]["reason"], "app_in_several_held_bundles");
    assert_eq!(body["detail"]["bundles"], json!(["bundle-a", "bundle-b"]));
}

#[tokio::test]
async fn two_held_bundles_sharing_one_licence_are_not_ambiguous() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "bundle-a").await;
    hold(&db, TENANT, BUNDLE2, "bundle-b").await;
    bundle_app(&db, BUNDLE, APP).await;
    bundle_app(&db, BUNDLE2, APP).await;
    // Only bundle-a has a licence: bundle-b's empty candidate set must not turn one licence
    // into a 409.
    licence(&db, LICENCE, TENANT, APP, PLAN, SUBJECT, Some(BUNDLE)).await;

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["license_id"], LICENCE.to_string());
    assert_eq!(body["bundle"], "bundle-a");
}

#[tokio::test]
async fn two_licences_for_one_app_in_one_bundle_are_ambiguous() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "non-agentic-ai").await;
    bundle_app(&db, BUNDLE, APP).await;
    licence(&db, LICENCE, TENANT, APP, PLAN, SUBJECT, Some(BUNDLE)).await;
    licence(&db, LICENCE2, TENANT, APP, PLAN, SUBJECT2, Some(BUNDLE)).await;

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "ambiguous_app_licence");
    assert_eq!(
        body["detail"]["reason"],
        "several_per_app_licences_in_bundle"
    );
    assert_eq!(body["detail"]["bundle"], "non-agentic-ai");
    let mut ids: Vec<String> = body["detail"]["license_ids"]
        .as_array()
        .expect("license_ids")
        .iter()
        .map(|id| id.as_str().expect("id").to_owned())
        .collect();
    ids.sort();
    let mut expected = vec![LICENCE.to_string(), LICENCE2.to_string()];
    expected.sort();
    assert_eq!(ids, expected);
}

#[tokio::test]
async fn a_held_bundle_without_a_licence_is_a_404_no_app_licence() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "non-agentic-ai").await;
    bundle_app(&db, BUNDLE, APP).await;

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "no_app_licence");
    assert_eq!(
        body["detail"]["reason"],
        "no_active_per_app_licence_in_bundle"
    );
    assert_eq!(body["detail"]["bundle"], "non-agentic-ai");
}

#[tokio::test]
async fn a_bundle_the_tenant_does_not_hold_is_a_404_bundle_not_held() {
    let db = seeded_with_bundle().await;
    let (status, body) = mint(
        &app(&db, minter()),
        &mint_path(TENANT, "app=demo&bundle=absent"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"]["error"], "no_bundle");
    assert_eq!(body["detail"]["reason"], "bundle_not_held");
}

#[tokio::test]
async fn the_minter_role_is_required_and_a_client_role_does_not_count() {
    let db = seeded_with_bundle().await;

    // A realm role that is NOT the minter: the fleet-wide internal roles must not admit.
    let internal = user_with(&["service-internal"], Some("owner-a"), false);
    let (status, body) = mint(&app(&db, internal), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"], "Role required: bundle-token-minter");

    // The same role held as a CLIENT role is not the realm role either.
    let mut client_only = user_with(&[], Some("owner-a"), false);
    client_only.client_roles = vec!["bundle-token-minter".to_owned()];
    let (status, body) = mint(&app(&db, client_only), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"], "Role required: bundle-token-minter");
}

#[tokio::test]
async fn an_unregistered_client_is_a_403_unknown_api_client() {
    let db = seeded_with_bundle().await;
    let unknown = user_with(&["bundle-token-minter"], None, false);
    let (status, body) = mint(&app(&db, unknown), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "unknown_api_client");
}

#[tokio::test]
async fn ttl_outside_one_to_sixty_is_a_422() {
    let db = seeded_with_bundle().await;
    let app = app(&db, minter());

    for (query, kind) in [
        ("app=demo&ttl_minutes=0", "greater_than_equal"),
        ("app=demo&ttl_minutes=61", "less_than_equal"),
    ] {
        let (status, body) = mint(&app, &mint_path(TENANT, query)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
        assert_eq!(body["detail"][0]["type"], kind);
        assert_eq!(body["detail"][0]["loc"], json!(["query", "ttl_minutes"]));
    }

    // `app` is required, and an empty key is too short.
    let (status, body) = mint(&app, &mint_path(TENANT, "ttl_minutes=5")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"][0]["type"], "missing");
    assert_eq!(body["detail"][0]["loc"], json!(["query", "app"]));
}

#[tokio::test]
async fn too_long_an_app_key_is_a_400_app_key_too_long() {
    let db = seeded_with_bundle().await;
    let too_long = "a".repeat(215);
    let (status, body) = mint(
        &app(&db, minter()),
        &mint_path(TENANT, &format!("app={too_long}")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"]["error"], "app_key_too_long");
    assert_eq!(body["detail"]["app"], too_long);
}

#[tokio::test]
async fn a_dependency_app_mints_on_the_shared_tenant_licence() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "non-agentic-ai").await;
    bundle_app(&db, BUNDLE, APP).await;
    requires(&db, APP, DEP_APP).await;
    // The dependency licence is tenant-level (bundle_id NULL) and held by a `svc:` subject.
    licence(
        &db,
        DEP_LICENCE,
        TENANT,
        DEP_APP,
        DEP_PLAN,
        SVC_SUBJECT,
        None,
    )
    .await;

    let (status, body) = mint(&app(&db, minter()), &mint_path(TENANT, "app=workers")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["license_id"], DEP_LICENCE.to_string());
    assert_eq!(body["bundle"], "non-agentic-ai");
    assert_eq!(body["app"], "workers");
}

#[tokio::test]
async fn a_dependency_grant_admits_only_its_own_apps() {
    let db = seeded().await;
    hold(&db, TENANT, BUNDLE, "non-agentic-ai").await;
    bundle_app(&db, BUNDLE, APP).await;
    requires(&db, APP, DEP_APP).await;
    licence(
        &db,
        DEP_LICENCE,
        TENANT,
        DEP_APP,
        DEP_PLAN,
        SVC_SUBJECT,
        None,
    )
    .await;
    sqlx::query(
        "INSERT INTO tenant_dependency_grant (tenant_id, owner_key, app_id, granted_by, created_at)
         VALUES ($1, 'owner-b', $2, 'test', now())",
    )
    .bind(TENANT)
    .bind(DEP_APP)
    .execute(&db.pool)
    .await
    .expect("dependency grant");

    let caller = user_with(&["bundle-token-minter"], Some("owner-b"), false);
    // owner-b reaches the tenant ONLY through the dependency grant, and only for `workers`.
    let (status, body) = mint(&app(&db, caller.clone()), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "dependency_grant_scope");
    assert_eq!(body["detail"]["reason"], "app_outside_dependency_grant");
    assert_eq!(body["detail"]["granted_apps"], json!(["workers"]));

    let (status, body) = mint(&app(&db, caller), &mint_path(TENANT, "app=workers")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["license_id"], DEP_LICENCE.to_string());
}

#[tokio::test]
async fn a_cross_owner_reaches_a_foreign_tenant() {
    let db = seeded_with_bundle().await;
    let cross_owner = user_with(&["bundle-token-minter"], Some("owner-b"), true);
    let (status, body) = mint(&app(&db, cross_owner), &mint_path(TENANT, "app=demo")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["license_id"], LICENCE.to_string());
}
