//! Behaviour of `POST /license/introspect` and of the auth middleware's path decision.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, so
//! the predicates are exercised against the captured schema rather than a mock.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router;
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

async fn post(app: axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::post(path)
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
async fn an_active_token_reports_the_captured_payload() {
    let db = seeded().await;
    let (status, body) = post(
        router::router(state(db.pool.clone())),
        "/license/introspect",
        json!({"token": RAW}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["active"], true);
    assert_eq!(body["tenant_id"], TENANT.to_string());
    assert_eq!(body["license_id"], LICENSE.to_string());
    assert_eq!(body["subject_id"], SUBJECT.to_string());
    assert_eq!(body["status"], "active");
    assert_eq!(body["byo_mode"], "local_allowed");
    assert_eq!(body["app"]["key"], "demo");
    assert_eq!(body["plan"]["code"], "basic");
    // A Keycloak-shaped sub that names no subject row is emitted; the subject kind follows the
    // realm (portal = person).
    assert_eq!(
        body["subject_keycloak_sub"],
        "0195f48b-d000-7000-8000-000000000001"
    );
    assert_eq!(body["subject_kind"], "member");
    // Every token-only diagnostic is null on this route.
    assert!(body["reason"].is_null());
    assert!(body["candidate_count"].is_null());
    assert!(body["ambiguous"].is_null());
    assert!(body["subject_match"].is_null());
    // An ordinary token carries no granted roles, and the field is present as [].
    assert_eq!(body["roles"], json!([]));
    // The whole model is serialised, including nulls (Pydantic's default behaviour).
    assert_eq!(body.as_object().expect("object").len(), 21);
}

#[tokio::test]
async fn every_inactive_reason_is_reported() {
    let db = seeded().await;
    let app = router::router(state(db.pool.clone()));

    let (_, body) = post(app.clone(), "/license/introspect", json!({"token": "nope"})).await;
    assert_eq!(body["active"], false);
    assert_eq!(body["reason"], "token_not_found");

    sqlx::query("UPDATE token SET revoked = true WHERE id = $1")
        .bind(TOKEN)
        .execute(&db.pool)
        .await
        .expect("revoke");
    let (_, body) = post(app.clone(), "/license/introspect", json!({"token": RAW})).await;
    assert_eq!(body["reason"], "token_invalid");

    sqlx::query("UPDATE token SET revoked = false WHERE id = $1")
        .bind(TOKEN)
        .execute(&db.pool)
        .await
        .expect("unrevoke");
    sqlx::query("UPDATE license SET expires_at = now() - interval '1 day' WHERE id = $1")
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("expire");
    let (_, body) = post(app.clone(), "/license/introspect", json!({"token": RAW})).await;
    assert_eq!(body["reason"], "license_inactive");

    sqlx::query("UPDATE license SET expires_at = NULL WHERE id = $1")
        .bind(LICENSE)
        .execute(&db.pool)
        .await
        .expect("unexpire");
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");
    let (_, body) = post(app, "/license/introspect", json!({"token": RAW})).await;
    assert_eq!(body["reason"], "tenant_archived");
}

#[tokio::test]
async fn the_middleware_refuses_a_protected_path_before_routing() {
    let db = seeded().await;
    let app = router::router(state(db.pool.clone()));

    // `/licenses/active` has no handler yet — a 401 here proves the middleware ran first.
    let response = app
        .clone()
        .oneshot(
            Request::get("/licenses/active")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer")
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        body,
        json!({"error": {"message": "Missing authentication token", "status": 401}})
    );

    // A token shaped like a JWT that cannot be verified is a hard 401, never the opaque path.
    let response = app
        .oneshot(
            Request::get("/licenses/active")
                .header("authorization", "Bearer a.b.c")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        body,
        json!({"error": {"message": "Invalid or expired token", "status": 401}})
    );
}

// ── GET /licenses/active ────────────────────────────────────────────────────
// The opaque-token path makes these end-to-end: the middleware introspects the raw token, builds
// a `license-reader` user whose tenant is the introspection's tenant, and the route's own
// authorization runs on that identity.

async fn get(app: axum::Router, path: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::get(path);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .oneshot(request.body(Body::empty()).expect("request"))
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

fn active_path(tenant: &str, app: &str, extra: &str) -> String {
    format!("/licenses/active?tenant_id={tenant}&app_key={app}{extra}")
}

#[tokio::test]
async fn active_resolves_through_the_opaque_token_path() {
    let db = seeded().await;
    let app = router::router(state(db.pool.clone()));
    let (status, body) = get(
        app,
        &active_path(&TENANT.to_string(), "demo", ""),
        Some(RAW),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["active"], true);
    assert_eq!(body["tenant_id"], TENANT.to_string());
    assert_eq!(body["license_id"], LICENSE.to_string());
    assert_eq!(body["candidate_count"], 1);
    assert_eq!(body["ambiguous"], false);
    // No subject was asked for: "not applicable", not false.
    assert!(body["subject_match"].is_null());
    // The active builder leaves the token-only fields null.
    assert!(body["byo_mode"].is_null());
    assert!(body["roles"].is_null());
    assert!(body["token_expires_at"].is_null());
    assert_eq!(body["app"]["key"], "demo");
}

#[tokio::test]
async fn active_picks_the_requested_seat_out_of_several() {
    let db = seeded().await;
    // A second employee: same tenant and app, different subject. This is the NORMAL state the
    // reference refuses to treat as a conflict.
    let subject2 = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0007);
    let license2 = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0008);
    let sub2 = "0195f48b-d000-7000-8000-000000000002";
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(subject2)
    .bind(sub2)
    .execute(&db.pool)
    .await
    .expect("subject 2");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, expires_at, tenant_id)
         VALUES ($1, $2, $3, $4, 'active', now(), now() + interval '365 days', $5)",
    )
    .bind(license2)
    .bind(subject2)
    .bind(APP)
    .bind(PLAN)
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("license 2");

    let app = router::router(state(db.pool.clone()));
    let (status, body) = get(
        app.clone(),
        &active_path(&TENANT.to_string(), "demo", ""),
        Some(RAW),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["candidate_count"], 2);
    assert_eq!(body["ambiguous"], true);
    // Perpetual first: the original licence, despite the later row's further expiry.
    assert_eq!(body["license_id"], LICENSE.to_string());

    let (status, body) = get(
        app,
        &active_path(&TENANT.to_string(), "demo", &format!("&subject_id={sub2}")),
        Some(RAW),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["license_id"], license2.to_string());
    assert_eq!(body["subject_match"], true);
    assert_eq!(body["ambiguous"], false);
}

#[tokio::test]
async fn active_refuses_unknown_tenants_and_unknown_apps() {
    let db = seeded().await;
    let app = router::router(state(db.pool.clone()));

    let (status, body) = get(
        app.clone(),
        &active_path(&TENANT.to_string(), "nope", ""),
        Some(RAW),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["detail"]["error"], "no_active_license");
    assert_eq!(body["detail"]["tenant_id"], TENANT.to_string());
    assert_eq!(body["detail"]["app_key"], "nope");

    // A licence-reader may not read another tenant.
    let other = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0009);
    let (status, body) = get(
        app.clone(),
        &active_path(&other.to_string(), "demo", ""),
        Some(RAW),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"],
        "Not authorized to read another tenant's active license"
    );

    // FastAPI's 422 for a missing required query field.
    let (status, body) = get(app, "/licenses/active?app_key=demo", Some(RAW)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"][0]["loc"][1], "tenant_id");
}
