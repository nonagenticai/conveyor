//! Behaviour of `GET /licenses/user/{subject_id}`.
//!
//! The route-level router injects a `User` with `Extension`, so these cases exercise the
//! handler's own three-tier authorization without the JWT/opaque-token path (that path is
//! covered by `tests/introspect.rs`). Every case runs against a real Postgres 18 created by
//! `common` and the embedded baseline.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::{Extension, Router, routing::get};
use chrono::{Duration, Utc};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::user_licenses::list_user_licenses;
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0002);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0005);
const TENANT2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000a);
const LICENSE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000b);
const PENDING: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000c);
const EXPIRED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000d);
const REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000e);
// A stored-active licence is unique per (tenant, app, subject) — the partial index
// `uq_license_active_tenant_app_subject` — so the time-window rows each need their own app.
const APP2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000f);
const PLAN2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0010);
const APP3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0011);
const PLAN3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0012);

/// The Keycloak `sub` the portal passes as `subject_id`.
const SUB: &str = "0195f48b-d000-7000-8000-000000000001";

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

/// The route alone, with the caller the middleware would have built.
fn route(state: AppState, user: User) -> Router {
    Router::new()
        .route("/licenses/user/{subject_id}", get(list_user_licenses))
        .layer(Extension(user))
        .with_state(state)
}

fn user(roles: &[&str], sub: &str, tenant: Option<Uuid>, owner_key: Option<&str>) -> User {
    User {
        sub: sub.to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: tenant.map(|tenant| tenant.to_string()),
        owner_key: owner_key.map(str::to_owned),
        ..User::default()
    }
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
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(SUBJECT)
    .bind(SUB)
    .execute(&db.pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id, metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, NULL::jsonb)",
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

// A fixture: the nine columns are the row it inserts. Bundling them into a struct would add a
// type whose only purpose is to defeat this lint.
#[allow(clippy::too_many_arguments)]
async fn add_license_for(
    pool: &sqlx::PgPool,
    id: Uuid,
    subject: Uuid,
    tenant: Uuid,
    app: Uuid,
    plan: Uuid,
    status: &str,
    starts_at: chrono::DateTime<Utc>,
    expires_at: Option<chrono::DateTime<Utc>>,
) {
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, expires_at, tenant_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL::jsonb)",
    )
    .bind(id)
    .bind(subject)
    .bind(app)
    .bind(plan)
    .bind(status)
    .bind(starts_at)
    .bind(expires_at)
    .bind(tenant)
    .execute(pool)
    .await
    .expect("license");
}

async fn add_license(
    pool: &sqlx::PgPool,
    id: Uuid,
    subject: Uuid,
    tenant: Uuid,
    status: &str,
    starts_at: chrono::DateTime<Utc>,
    expires_at: Option<chrono::DateTime<Utc>>,
) {
    add_license_for(
        pool, id, subject, tenant, APP, PLAN, status, starts_at, expires_at,
    )
    .await;
}

async fn fetch(app: Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::get(path).body(Body::empty()).expect("request"))
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

fn find(rows: &[Value], id: Uuid) -> &Value {
    let id = id.to_string();
    rows.iter()
        .find(|row| row["id"].as_str() == Some(id.as_str()))
        .expect("a row for the licence id")
}

#[tokio::test]
async fn an_admin_reads_any_subject_with_the_full_captured_shape() {
    let db = seeded().await;
    // The admin's own sub and tenant are deliberately unrelated to the subject's.
    let app = route(
        state(db.pool.clone()),
        user(&["license-admin"], "operator-sub", None, None),
    );

    let (status, body) = fetch(app.clone(), &format!("/licenses/user/{SUB}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("a bare JSON array");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    // The captured `LicenseResponse` has 22 properties; every one is serialised, nulls included.
    assert_eq!(row.as_object().expect("object").len(), 22);
    assert_eq!(row["id"], LICENSE.to_string());
    assert_eq!(row["subject_id"], SUBJECT.to_string());
    assert_eq!(row["tenant_id"], TENANT.to_string());
    assert_eq!(row["app_id"], APP.to_string());
    assert_eq!(row["plan_id"], PLAN.to_string());
    assert!(row["region_id"].is_null());
    assert_eq!(row["status"], "active");
    assert!(row["starts_at"].as_str().is_some());
    // A null field, emitted rather than skipped.
    assert!(row["expires_at"].is_null());
    assert!(row["created_at"].as_str().is_some());
    assert!(row["updated_at"].as_str().is_some());
    assert!(row["revoked_at"].is_null());
    assert_eq!(row["effective_status"], "active");
    assert!(row["metadata"].is_null());
    assert!(row["metering_provisioned_at"].is_null());
    assert!(row["metering_verified_at"].is_null());
    assert!(row["token_budget_override"].is_null());
    assert!(row["quota_override"].is_null());
    assert_eq!(row["is_dependency"], false);
    assert!(row["bundle_id"].is_null());
    // `app` enrichment, and `plan` never populated on this route.
    assert_eq!(row["app"]["id"], APP.to_string());
    assert_eq!(row["app"]["key"], "demo");
    assert_eq!(row["app"]["name"], "Demo");
    assert!(row["plan"].is_null());

    // The internal UUID is the fallback identifier and resolves the same subject.
    let (status, body) = fetch(app, &format!("/licenses/user/{SUBJECT}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0]["id"], LICENSE.to_string());
}

#[tokio::test]
async fn a_reader_reads_its_own_subject() {
    let db = seeded().await;
    let app = route(
        state(db.pool.clone()),
        user(&["license-reader"], SUB, Some(TENANT), None),
    );
    let (status, body) = fetch(app, &format!("/licenses/user/{SUB}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().map(Vec::len), Some(1));
    assert_eq!(body[0]["id"], LICENSE.to_string());
}

#[tokio::test]
async fn a_reader_cannot_read_another_subject() {
    let db = seeded().await;
    let app = route(
        state(db.pool.clone()),
        user(&["license-reader"], "someone-else", Some(TENANT), None),
    );
    let (status, body) = fetch(app, &format!("/licenses/user/{SUB}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["detail"],
        "Not authorized to read another subject's licenses"
    );
}

#[tokio::test]
async fn an_owning_app_is_scoped_to_the_tenant_it_acts_for() {
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-b', now(), 'owner-a', 'local_allowed')",
    )
    .bind(TENANT2)
    .execute(&db.pool)
    .await
    .expect("tenant 2");
    // The SAME subject holds a second licence in another tenant.
    add_license(
        &db.pool,
        LICENSE2,
        SUBJECT,
        TENANT2,
        "active",
        Utc::now(),
        None,
    )
    .await;

    // The operator tier has no tenant filter: both licences are visible.
    let admin = route(
        state(db.pool.clone()),
        user(&["license-admin"], "operator-sub", Some(TENANT2), None),
    );
    let (status, body) = fetch(admin, &format!("/licenses/user/{SUB}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().map(Vec::len), Some(2));

    // An owning app may ask for another subject, but the result is the tenant it acts for.
    for (tenant, expected) in [(TENANT, LICENSE), (TENANT2, LICENSE2)] {
        let app = route(
            state(db.pool.clone()),
            user(
                &["license-reader"],
                "end-user-sub",
                Some(tenant),
                Some("fr-backend"),
            ),
        );
        let (status, body) = fetch(app, &format!("/licenses/user/{SUB}")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let rows = body.as_array().expect("array");
        assert_eq!(rows.len(), 1, "{body}");
        assert_eq!(rows[0]["id"], expected.to_string());
        assert_eq!(rows[0]["tenant_id"], tenant.to_string());
    }
}

#[tokio::test]
async fn an_unknown_subject_is_a_404() {
    let db = seeded().await;
    let app = route(
        state(db.pool.clone()),
        user(&["license-admin"], "operator-sub", None, None),
    );
    // Neither a keycloak_sub nor a parseable UUID.
    let (status, body) = fetch(app.clone(), "/licenses/user/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Subject not found"}));
    // Well-formed UUID that names no subject.
    let (status, body) = fetch(app, &format!("/licenses/user/{}", Uuid::nil())).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Subject not found"}));
}

#[tokio::test]
async fn effective_status_resolves_the_time_window_verbatim() {
    let db = seeded().await;
    let now = Utc::now();
    for (app, plan, key) in [(APP2, PLAN2, "demo-2"), (APP3, PLAN3, "demo-3")] {
        sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $2, now())")
            .bind(app)
            .bind(key)
            .execute(&db.pool)
            .await
            .expect("app");
        sqlx::query(
            "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, $3, $3, now())",
        )
        .bind(plan)
        .bind(app)
        .bind(key)
        .execute(&db.pool)
        .await
        .expect("plan");
    }
    add_license_for(
        &db.pool,
        PENDING,
        SUBJECT,
        TENANT,
        APP2,
        PLAN2,
        "active",
        now + Duration::days(1),
        None,
    )
    .await;
    add_license_for(
        &db.pool,
        EXPIRED,
        SUBJECT,
        TENANT,
        APP3,
        PLAN3,
        "active",
        now - Duration::days(10),
        Some(now - Duration::days(1)),
    )
    .await;
    add_license(
        &db.pool,
        REVOKED,
        SUBJECT,
        TENANT,
        "revoked",
        now - Duration::days(10),
        None,
    )
    .await;

    let app = route(
        state(db.pool.clone()),
        user(&["license-admin"], "operator-sub", None, None),
    );
    let (status, body) = fetch(app, &format!("/licenses/user/{SUB}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("array");
    assert_eq!(rows.len(), 4);

    // A stored `active` row is resolved against the window; `status` itself stays structural.
    assert_eq!(find(rows, PENDING)["status"], "active");
    assert_eq!(find(rows, PENDING)["effective_status"], "pending");
    assert_eq!(find(rows, EXPIRED)["status"], "active");
    assert_eq!(find(rows, EXPIRED)["effective_status"], "expired");
    assert_eq!(find(rows, LICENSE)["effective_status"], "active");
    // An unknown stored status is returned verbatim, never laundered into a valid one.
    assert_eq!(find(rows, REVOKED)["status"], "revoked");
    assert_eq!(find(rows, REVOKED)["effective_status"], "revoked");
}
