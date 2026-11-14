//! Behaviour of the five tenant BYO-mode operations against a real Postgres 18.
//!
//! Every case runs against a database created by `common` and the embedded baseline, so the
//! owner/grant/bundle predicates are exercised against the captured schema rather than a mock.
//! The assertions pin the EXACT response JSON the captured contract describes, including the
//! explicit nulls of `TenantResponse` and the `offering_provenance` field.
//!
//! The first test runs the FULL router (auth middleware included) because the exemption is the
//! contract: `worker_management` calls the tenant-level GET with NO token and FAILS OPEN on
//! anything that is not a 200, so a 401 there silently disables byo_required fleet-wide.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::get as route_get;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::tenant_byo::{
    clear_tenant_app_byo_mode, get_tenant_app_byo_mode, get_tenant_byo_mode,
    set_tenant_app_byo_mode, set_tenant_byo_mode,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const T1: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2101);
const T2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2102);
const APP_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2201);
const APP_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2202);
const APP_D: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2204);
const PLAN_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2301);
const PLAN_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2302);
const PLAN_D: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2304);
const SUBJECT_1: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2401);
const BUNDLE_LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2501);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2601);
const GRANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2701);

const CREATED_AT: &str = "2026-01-02T03:04:05Z";
const CREATED_AT_ISO: &str = "2026-01-02T03:04:05+00:00";

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

fn user(roles: &[&str], tenant: Option<Uuid>, owner: Option<&str>) -> User {
    User {
        sub: "test-subject".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: tenant.map(|tenant| tenant.to_string()),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

/// The five routes alone, with the caller the middleware would have built.
fn route_app(db: &common::TestDb, user: User) -> Router {
    Router::new()
        .route(
            "/tenants/{name_or_id}/byo-mode",
            route_get(get_tenant_byo_mode).patch(set_tenant_byo_mode),
        )
        .route(
            "/tenants/{name_or_id}/apps/{app_key}/byo-mode",
            route_get(get_tenant_app_byo_mode)
                .put(set_tenant_app_byo_mode)
                .delete(clear_tenant_app_byo_mode),
        )
        .layer(axum::Extension(user))
        .with_state(state(db.pool.clone()))
}

async fn send(app: Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(path);
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request"),
        None => request.body(Body::empty()).expect("request"),
    };
    let response = app.oneshot(request).await.expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::GET, path, None).await
}

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    for (key, name) in [
        ("owner-a", "Owner A"),
        ("owner-b", "Owner B"),
        ("owner-c", "Owner C"),
    ] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $2)")
            .bind(key)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }
    for (id, name, owner, byo_mode) in [
        (T1, "tenant-a", "owner-a", "local_allowed"),
        (T2, "tenant-b", "owner-b", "platform_only"),
    ] {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
             VALUES ($1, $2, '2026-01-02T03:04:05Z', $3, $4)",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .bind(byo_mode)
        .execute(&db.pool)
        .await
        .expect("tenant");
    }
    for (id, key, name) in [
        (APP_A, "demo", "Demo"),
        (APP_B, "workers", "Workers"),
        (APP_D, "extra", "Extra"),
    ] {
        sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $3, now())")
            .bind(id)
            .bind(key)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("app");
    }
    // APP_A is issued by owner-a; APP_B and APP_D by other owners. owner-a brought APP_B
    // onto T1 with its own bundle (the licence below).
    for (app, owner) in [(APP_A, "owner-a"), (APP_B, "owner-b"), (APP_D, "owner-c")] {
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(app)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    for (id, app, code) in [
        (PLAN_A, APP_A, "basic-a"),
        (PLAN_B, APP_B, "basic-b"),
        (PLAN_D, APP_D, "basic-d"),
    ] {
        sqlx::query(
            "INSERT INTO plan (id, app_id, code, name, created_at, rate_limit_rpm, rate_limit_burst)
             VALUES ($1, $2, $3, $3, now(), 60, 10)",
        )
        .bind(id)
        .bind(app)
        .bind(code)
        .execute(&db.pool)
        .await
        .expect("plan");
    }
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, 'sub-1', 'portal', now())",
    )
    .bind(SUBJECT_1)
    .execute(&db.pool)
    .await
    .expect("subject");
    // The bundle whose licence below brought APP_B onto T1.
    sqlx::query("INSERT INTO bundle (id, key, name, created_at) VALUES ($1, 'test-bundle', 'Test Bundle', now())")
        .bind(BUNDLE)
        .execute(&db.pool)
        .await
        .expect("bundle");
    // A bundle-minted licence for APP_B on T1: owner-a does not ISSUE APP_B, but its bundle
    // brought it onto its own tenant.
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, tenant_id, status, bundle_id)
         VALUES ($1, $2, $3, $4, $5, 'active', $6)",
    )
    .bind(BUNDLE_LICENCE)
    .bind(SUBJECT_1)
    .bind(APP_B)
    .bind(PLAN_B)
    .bind(T1)
    .bind(BUNDLE)
    .execute(&db.pool)
    .await
    .expect("bundle licence");
    // owner-b holds an ACTIVE grant on T1.
    sqlx::query(
        "INSERT INTO tenant_app_grant (id, tenant_id, grantee_owner_key, granted_by)
         VALUES ($1, $2, 'owner-b', 'test')",
    )
    .bind(GRANT)
    .bind(T1)
    .execute(&db.pool)
    .await
    .expect("grant");
    db
}

/// The unauthenticated worker_management shape, and the exemptions that must NOT exist.
#[tokio::test]
async fn tokenless_tenant_get_is_open_for_uuid_selectors_only() {
    let db = seeded().await;
    let app = conveyor_licenses::router::router(state(db.pool.clone()));

    let (status, body) = get(app.clone(), &format!("/tenants/{T1}/byo-mode")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "tenant_id": T1.to_string(),
            "name": "tenant-a",
            "byo_mode": "local_allowed",
            "app": null,
            "resolution": "row",
        })
    );

    // A NAME selector is not exempt: the middleware answers its own envelope.
    let (status, body) = get(app.clone(), "/tenants/tenant-a/byo-mode").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        body,
        json!({"error": {"message": "Missing authentication token", "status": 401}})
    );

    // Neither the per-app read nor any write is exempt.
    let (status, _) = get(app.clone(), &format!("/tenants/{T1}/apps/demo/byo-mode")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        app.clone(),
        Method::PUT,
        &format!("/tenants/{T1}/apps/demo/byo-mode"),
        Some(json!({"byo_mode": "platform_only"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        app.clone(),
        Method::PATCH,
        &format!("/tenants/{T1}/byo-mode"),
        Some(json!({"byo_mode": "platform_only"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // An over-broad exemption must not exist: a GET that merely ENDS in /byo-mode is not open.
    let (status, _) = get(app.clone(), &format!("/tenants/{T1}/apps/demo/byo-mode")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// The tenant-wide GET: stored value, per-app fallback, and the three failure statuses.
#[tokio::test]
async fn tenant_get_values_and_error_envelope() {
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, 'byo_required', 'free-high-markup', now(), now())",
    )
    .bind(T1)
    .bind(APP_A)
    .execute(&db.pool)
    .await
    .expect("override");
    let app = route_app(&db, user(&[], None, None));

    // Stored tenant-wide value.
    let (status, body) = get(app.clone(), &format!("/tenants/{T1}/byo-mode")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["byo_mode"], "local_allowed");
    assert_eq!(body["app"], Value::Null);
    assert_eq!(body["resolution"], "row");

    // The per-product override wins, and `app` echoes the query parameter.
    let (status, body) = get(app.clone(), &format!("/tenants/{T1}/byo-mode?app=demo")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["byo_mode"], "byo_required");
    assert_eq!(body["app"], "demo");
    assert_eq!(body["resolution"], "row");

    // No override (or an unknown app): fall back to the tenant-wide value.
    let (status, body) = get(
        app.clone(),
        &format!("/tenants/{T1}/byo-mode?app=unknown-app"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["byo_mode"], "local_allowed");

    // A NAME lookup (authenticated in production) resolves globally outside an owner scope.
    let (status, body) = get(app.clone(), "/tenants/tenant-a/byo-mode").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tenant_id"], T1.to_string());

    // An absent, well-formed tenant is the machine-readable 404.
    let absent = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2fff);
    let (status, body) = get(app.clone(), &format!("/tenants/{absent}/byo-mode")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "tenant_not_found_in_registry",
            "tenant_selector": absent.to_string(),
        }})
    );

    // A selector that can never name a tenant is a 400, never a commercial 404.
    let (status, body) = get(app.clone(), "/tenants/Bad_Name/byo-mode").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"]["error"], "malformed_tenant_selector");

    // The on_unresolved pattern is enforced where the reference enforces it.
    let (status, body) = get(
        app.clone(),
        &format!("/tenants/{T1}/byo-mode?on_unresolved=bogus"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "string_pattern_mismatch");
}

/// The tenant-wide PATCH: owner-only, role-gated and validated.
#[tokio::test]
async fn tenant_patch_is_owner_only_and_validates_the_three_modes() {
    let db = seeded().await;

    let admin = user(&["license-admin"], Some(T1), Some("owner-a"));
    let (status, body) = send(
        route_app(&db, admin),
        Method::PATCH,
        "/tenants/tenant-a/byo-mode",
        Some(json!({"byo_mode": "platform_only"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "id": T1.to_string(),
            "name": "tenant-a",
            "keycloak_realm": null,
            "byo_mode": "platform_only",
            "kind": "unclassified",
            "cost_centre": null,
            "budget_currency": null,
            "archived_at": null,
            "owner_key": "owner-a",
            "external_ref": null,
            "created_at": CREATED_AT_ISO,
        })
    );
    let stored: String = sqlx::query_scalar("SELECT byo_mode FROM tenants WHERE id = $1")
        .bind(T1)
        .fetch_one(&db.pool)
        .await
        .expect("stored mode");
    assert_eq!(stored, "platform_only");

    // No role: the reference's realm-role guard, before anything is resolved.
    let (status, body) = send(
        route_app(&db, user(&[], Some(T1), Some("owner-a"))),
        Method::PATCH,
        "/tenants/tenant-a/byo-mode",
        Some(json!({"byo_mode": "local_allowed"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-manager"})
    );

    // A GRANTEE reaches the tenant but not the tenant-wide write.
    let (status, body) = send(
        route_app(&db, user(&["license-manager"], Some(T1), Some("owner-b"))),
        Method::PATCH,
        &format!("/tenants/{T1}/byo-mode"),
        Some(json!({"byo_mode": "local_allowed"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owner_only");

    // A foreign tenant with no grant is 404, indistinguishable from absent.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T1), Some("owner-c"))),
        Method::PATCH,
        &format!("/tenants/{T1}/byo-mode"),
        Some(json!({"byo_mode": "local_allowed"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));

    // An unknown tenant is the same 404.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T1), Some("owner-a"))),
        Method::PATCH,
        "/tenants/no-such-tenant/byo-mode",
        Some(json!({"byo_mode": "local_allowed"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));

    // An unknown value is the reference's in-handler 422 (a string detail, not the array).
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T1), Some("owner-a"))),
        Method::PATCH,
        &format!("/tenants/{T1}/byo-mode"),
        Some(json!({"byo_mode": "nonsense"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body,
        json!({"detail": "byo_mode must be 'local_allowed', 'byo_required', or 'platform_only'"})
    );

    // A missing field is FastAPI's 422 array.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T1), Some("owner-a"))),
        Method::PATCH,
        &format!("/tenants/{T1}/byo-mode"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "missing");
}

/// The owner/bundle half of the per-app matrix, including clear-after-revoke.
#[tokio::test]
async fn per_app_owner_issuer_and_bundle_licence_matrix() {
    let db = seeded().await;
    let owner_a = || user(&["license-manager"], Some(T1), Some("owner-a"));

    // The owner ISSUES demo: set, then overwrite (the prior value comes back).
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::PUT,
        &format!("/tenants/{T1}/apps/demo/byo-mode"),
        Some(json!({"byo_mode": "platform_only"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "tenant_id": T1.to_string(),
            "tenant_name": "tenant-a",
            "app_id": APP_A.to_string(),
            "app_key": "demo",
            "byo_mode": "platform_only",
            "offering_code": null,
            "prior_offering_code": null,
            "prior_byo_mode": null,
        })
    );
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::PUT,
        &format!("/tenants/{T1}/apps/demo/byo-mode"),
        Some(json!({"byo_mode": "byo_required"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["prior_byo_mode"], "platform_only");

    // The READ of one override row, with the provenance said out loud.
    let (status, body) = get(
        route_app(&db, user(&["license-reader"], Some(T1), None)),
        &format!("/tenants/{T1}/apps/demo/byo-mode"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["app_key"], "demo");
    assert_eq!(body["app_name"], "Demo");
    assert_eq!(body["byo_mode"], "byo_required");
    assert_eq!(body["offering_code"], Value::Null);
    assert_eq!(body["offering_provenance"], "unknown");
    assert!(body["created_at"].is_string(), "{body}");
    assert!(body["updated_at"].is_string(), "{body}");

    // No override is a 404 with the fallback instruction.
    let (status, body) = get(
        route_app(&db, user(&["license-reader"], Some(T1), None)),
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body,
        json!({"detail": "No per-product BYO override for this tenant and app"})
    );

    // The owner does NOT issue workers, but its own bundle brought it in: an ACTIVE bundle
    // licence is enough to SET the posture.
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::PUT,
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
        Some(json!({"byo_mode": "local_allowed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["app_key"], "workers");

    // Revoke the bundle licence: a set is refused, a clear is still admitted (offboarding).
    sqlx::query("UPDATE license SET status = 'revoked', revoked_at = now() WHERE id = $1")
        .bind(BUNDLE_LICENCE)
        .execute(&db.pool)
        .await
        .expect("revoke bundle licence");
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::PUT,
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
        Some(json!({"byo_mode": "byo_required"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_administrable_by_caller");
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::DELETE,
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"tenant": T1.to_string(), "app_key": "workers", "cleared": true})
    );
    // Idempotent second clear.
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::DELETE,
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cleared"], json!(false));
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::PUT,
        &format!("/tenants/{T1}/apps/no-such-app/byo-mode"),
        Some(json!({"byo_mode": "platform_only"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant or app not found"}));
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::DELETE,
        &format!("/tenants/{T1}/apps/no-such-app/byo-mode"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cleared"], json!(false));

    // An unknown tenant is a 404 before any app is considered.
    let (status, body) = send(
        route_app(&db, owner_a()),
        Method::PUT,
        "/tenants/no-such-tenant/apps/demo/byo-mode",
        Some(json!({"byo_mode": "platform_only"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
}

/// The grantee half: issuer + active grant, the refusals, and the read scope.
#[tokio::test]
async fn per_app_grantee_matrix_and_read_scope() {
    let db = seeded().await;

    // owner-b ISSUES workers and holds an active grant on T1: the same rule as minting.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T2), Some("owner-b"))),
        Method::PUT,
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
        Some(json!({"byo_mode": "byo_required"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["prior_byo_mode"], Value::Null);

    // The same grantee does NOT issue demo.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T2), Some("owner-b"))),
        Method::PUT,
        &format!("/tenants/{T1}/apps/demo/byo-mode"),
        Some(json!({"byo_mode": "byo_required"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_issuable_by_caller");

    // It issues extra but holds NO grant on T1.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], None, Some("owner-c"))),
        Method::PUT,
        &format!("/tenants/{T1}/apps/extra/byo-mode"),
        Some(json!({"byo_mode": "byo_required"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_not_accessible");

    // A name never resolves outside the caller's own owner, so it is the plain 404.
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T2), Some("owner-b"))),
        Method::PUT,
        "/tenants/tenant-a/apps/workers/byo-mode",
        Some(json!({"byo_mode": "byo_required"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));

    // An invalid mode is validated after the authorization matrix (so the 200 above stands).
    let (status, body) = send(
        route_app(&db, user(&["license-admin"], Some(T2), Some("owner-b"))),
        Method::PUT,
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
        Some(json!({"byo_mode": "nonsense"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body,
        json!({"detail": "byo_mode must be 'local_allowed', 'byo_required', or 'platform_only'"})
    );

    // A non-admin reader may read only its OWN tenant: the foreign one is 404, never 403.
    let (status, body) = get(
        route_app(&db, user(&["license-reader"], Some(T1), None)),
        &format!("/tenants/{T2}/apps/workers/byo-mode"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));

    // The grantee's admin read reaches T1 through the grant.
    let (status, body) = get(
        route_app(&db, user(&["license-admin"], Some(T2), Some("owner-b"))),
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["byo_mode"], "byo_required");
    assert_eq!(body["offering_provenance"], "unknown");

    // A role is still required on the read.
    let (status, body) = get(
        route_app(&db, user(&[], Some(T1), None)),
        &format!("/tenants/{T1}/apps/workers/byo-mode"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-reader"})
    );
}

/// The seeded creation instant is pinned so the exact JSON above cannot drift silently.
#[tokio::test]
async fn seeded_creation_instant_is_the_pinned_one() {
    let db = seeded().await;
    let stored: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT created_at FROM tenants WHERE id = $1")
            .bind(T1)
            .fetch_one(&db.pool)
            .await
            .expect("created_at");
    assert_eq!(stored.to_rfc3339(), "2026-01-02T03:04:05+00:00");
    assert_eq!(
        conveyor_licenses::db::tokens::py_iso(stored),
        CREATED_AT_ISO
    );
    // The literal above is the instant this fixture inserts.
    assert!(CREATED_AT.starts_with("2026-01-02"));
}
