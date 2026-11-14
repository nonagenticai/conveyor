//! Behaviour of `GET /internal/tenants/{tenant_id}/seat-count`.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, so
//! the seat predicate is exercised against the captured schema rather than a mock. The tests pin
//! the number billing reads (`seats`) and the human/service/unresolved partition of `subjects`.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get as route_get;
use chrono::{DateTime, Utc};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::internal_seat::internal_tenant_seat_count;
use conveyor_licenses::state::AppState;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0011);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0012);
const OTHER_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0013);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0014);
const OTHER_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0015);
const HUMAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0016);
const ROBOT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0017);
const HUMAN2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0018);
const HUMAN3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0019);
const HUMAN4: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_001a);
const HUMAN5: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_001b);

const LICENSE_1: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0021);
const LICENSE_2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0022);
const LICENSE_3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0023);
const LICENSE_4: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0024);
const LICENSE_5: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0025);

const EARLY: &str = "2026-01-01T00:00:00Z";
const AS_OF: &str = "2026-06-15T00:00:00Z";

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("rfc3339 instant")
        .with_timezone(&Utc)
}

fn seat_path(tenant: &str) -> String {
    format!("/internal/tenants/{tenant}/seat-count")
}

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

fn route(state: AppState, roles: &[&str]) -> Router {
    let user = User {
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        ..User::default()
    };
    Router::new()
        .route(
            "/internal/tenants/{tenant_id}/seat-count",
            route_get(internal_tenant_seat_count),
        )
        .layer(axum::Extension(user))
        .with_state(state)
}

fn internal_route(db: &common::TestDb) -> Router {
    route(state(db.pool.clone()), &["service-internal"])
}

async fn get(app: Router, path: &str) -> (StatusCode, Value) {
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

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-a', 'Owner A')")
        .execute(&db.pool)
        .await
        .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, kind, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-a', 'customer', now(), 'owner-a', 'local_allowed')",
    )
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("tenant");
    for (id, key, name) in [(APP, "demo", "Demo"), (OTHER_APP, "other", "Other")] {
        sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $3, now())")
            .bind(id)
            .bind(key)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("app");
    }
    for (id, app, code, name) in [
        (PLAN, APP, "basic", "Basic"),
        (OTHER_PLAN, OTHER_APP, "other-basic", "Other Basic"),
    ] {
        sqlx::query(
            "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, $3, $4, now())",
        )
        .bind(id)
        .bind(app)
        .bind(code)
        .bind(name)
        .execute(&db.pool)
        .await
        .expect("plan");
    }
    // One licence per subject everywhere: the partial unique index on stored-active licences
    // frees a row only by changing its status, so distinct subjects keep the seeds independent
    // of that index's exact column list.
    for (id, sub, realm) in [
        (HUMAN, "0195f48b-d000-7000-8000-000000000011", "portal"),
        (ROBOT, "0195f48b-d000-7000-8000-000000000012", "service"),
        (HUMAN2, "0195f48b-d000-7000-8000-000000000013", "portal"),
        (HUMAN3, "0195f48b-d000-7000-8000-000000000014", "portal"),
        (HUMAN4, "0195f48b-d000-7000-8000-000000000015", "portal"),
        (HUMAN5, "0195f48b-d000-7000-8000-000000000016", "portal"),
    ] {
        sqlx::query(
            "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
             VALUES ($1, $2, $3, now())",
        )
        .bind(id)
        .bind(sub)
        .bind(realm)
        .execute(&db.pool)
        .await
        .expect("subject");
    }
    db
}

#[allow(clippy::too_many_arguments)]
async fn licence(
    db: &common::TestDb,
    id: Uuid,
    subject: Uuid,
    app: Uuid,
    plan: Uuid,
    status: &str,
    starts_at: Option<&str>,
    expires_at: Option<&str>,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO license
             (id, subject_id, app_id, plan_id, status, starts_at, expires_at, created_at, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(subject)
    .bind(app)
    .bind(plan)
    .bind(status)
    .bind(starts_at.map(at))
    .bind(expires_at.map(at))
    .bind(at(created_at))
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("license");
}

#[tokio::test]
async fn point_in_time_counts_only_effective_licences() {
    let db = seeded().await;
    // A seat: started, unexpired, perpetual.
    licence(
        &db,
        LICENSE_1,
        HUMAN,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        None,
        EARLY,
    )
    .await;
    // A seat: started, expiring after as_of, and a robot (excluded from human_subjects).
    licence(
        &db,
        LICENSE_2,
        ROBOT,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        Some("2026-07-01T00:00:00Z"),
        EARLY,
    )
    .await;
    // NOT a seat: stored-active but expired before as_of.
    licence(
        &db,
        LICENSE_3,
        HUMAN2,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        Some("2026-05-01T00:00:00Z"),
        EARLY,
    )
    .await;
    // NOT a seat: stored-active but not started yet (pending).
    licence(
        &db,
        LICENSE_4,
        HUMAN3,
        APP,
        PLAN,
        "active",
        Some("2026-08-01T00:00:00Z"),
        None,
        EARLY,
    )
    .await;
    // NOT a seat: revoked.
    licence(
        &db,
        LICENSE_5,
        HUMAN4,
        APP,
        PLAN,
        "revoked",
        Some(EARLY),
        None,
        EARLY,
    )
    .await;

    let (status, body) = get(
        internal_route(&db),
        &format!("{}?as_of={AS_OF}", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tenant_id"], TENANT.to_string());
    assert_eq!(body["seats"], 2);
    assert_eq!(body["subjects"], 2);
    assert_eq!(body["human_subjects"], 1);
    assert_eq!(body["service_subjects"], 1);
    assert_eq!(body["unresolved_subjects"], 0);
    assert_eq!(body["basis"], "active_licences");
    assert_eq!(body["tenant_kind"], "customer");
    assert_eq!(body["tenant_archived"], false);
    assert!(body["app_key"].is_null());
    assert!(body["period_start"].is_null());
    assert!(body["period_end"].is_null());
    assert!(
        body["as_of"]
            .as_str()
            .expect("a datetime")
            .starts_with("2026-06-15T00:00:00"),
        "{body}"
    );
    // The three person-basis fields partition `subjects` exactly.
    let human = body["human_subjects"].as_i64().expect("an integer");
    let service = body["service_subjects"].as_i64().expect("an integer");
    let unresolved = body["unresolved_subjects"].as_i64().expect("an integer");
    assert_eq!(
        human + service + unresolved,
        body["subjects"].as_i64().unwrap()
    );
}

#[tokio::test]
async fn a_row_deleted_after_as_of_was_still_a_seat_then() {
    let db = seeded().await;
    // The soft delete retains the row, stamps `deleted_at` and forces status='revoked';
    // `deleted_from_status` is what says it used to be a seat.
    sqlx::query(
        "INSERT INTO license
             (id, subject_id, app_id, plan_id, status, starts_at, created_at,
              deleted_at, deleted_from_status, tenant_id)
         VALUES ($1, $2, $3, $4, 'revoked', $5, $6, $7, 'active', $8)",
    )
    .bind(LICENSE_1)
    .bind(HUMAN5)
    .bind(APP)
    .bind(PLAN)
    .bind(at(EARLY))
    .bind(at(EARLY))
    .bind(at("2026-07-01T00:00:00Z"))
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("soft-deleted license");

    // At an instant before the delete it was a seat.
    let (status, body) = get(
        internal_route(&db),
        &format!("{}?as_of={AS_OF}", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seats"], 1);
    assert_eq!(body["human_subjects"], 1);
    assert_eq!(body["unresolved_subjects"], 0);

    // At an instant after it, the same row is not.
    let (status, body) = get(
        internal_route(&db),
        &format!(
            "{}?as_of=2026-07-15T00:00:00Z",
            seat_path(&TENANT.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seats"], 0);
    assert_eq!(body["subjects"], 0);
}

#[tokio::test]
async fn an_archived_tenant_answers_with_the_archived_basis() {
    let db = seeded().await;
    licence(
        &db,
        LICENSE_1,
        HUMAN,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        None,
        EARLY,
    )
    .await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");

    let (status, body) = get(
        internal_route(&db),
        &format!("{}?app=demo&as_of={AS_OF}", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["basis"], "tenant_archived");
    assert_eq!(body["tenant_archived"], true);
    assert_eq!(body["tenant_kind"], "customer");
    assert_eq!(body["seats"], 0);
    assert_eq!(body["subjects"], 0);
    assert_eq!(body["human_subjects"], 0);
    assert_eq!(body["service_subjects"], 0);
    assert_eq!(body["unresolved_subjects"], 0);
    // The echo fields stay present: the response shape never varies by branch.
    assert_eq!(body["app_key"], "demo");
    assert!(body["as_of"].is_string());
}

#[tokio::test]
async fn app_scoping_excludes_other_products_and_an_unknown_app_is_422() {
    let db = seeded().await;
    licence(
        &db,
        LICENSE_1,
        HUMAN,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        None,
        EARLY,
    )
    .await;
    licence(
        &db,
        LICENSE_2,
        HUMAN2,
        OTHER_APP,
        OTHER_PLAN,
        "active",
        Some(EARLY),
        None,
        EARLY,
    )
    .await;

    // Without `app`, every product counts.
    let (status, body) = get(
        internal_route(&db),
        &format!("{}?as_of={AS_OF}", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seats"], 2);

    // `app=demo` narrows to the demo licence and echoes its own basis.
    let (status, body) = get(
        internal_route(&db),
        &format!("{}?app=demo&as_of={AS_OF}", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seats"], 1);
    assert_eq!(body["subjects"], 1);
    assert_eq!(body["app_key"], "demo");

    // An unknown key is 422 app_not_found — deliberately NOT 404 (which billing reads as 0).
    let (status, body) = get(
        internal_route(&db),
        &format!("{}?app=nope", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"], "app_not_found: no app with key 'nope'");
}

#[tokio::test]
async fn a_period_counts_every_seat_held_at_any_instant() {
    let db = seeded().await;
    // P1: a mid-period joiner.
    licence(
        &db,
        LICENSE_1,
        HUMAN,
        APP,
        PLAN,
        "active",
        Some("2026-06-15T00:00:00Z"),
        None,
        EARLY,
    )
    .await;
    // P2: a mid-period leaver — invisible at period end.
    licence(
        &db,
        LICENSE_2,
        ROBOT,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        Some("2026-06-10T00:00:00Z"),
        EARLY,
    )
    .await;
    // P3: expired before the period began — never a seat inside it.
    licence(
        &db,
        LICENSE_3,
        HUMAN2,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        Some("2026-05-01T00:00:00Z"),
        EARLY,
    )
    .await;
    // P4: revoked DURING the period — the revocation arm is why this predicate exists.
    sqlx::query(
        "INSERT INTO license
             (id, subject_id, app_id, plan_id, status, starts_at, created_at, revoked_at, tenant_id)
         VALUES ($1, $2, $3, $4, 'revoked', $5, $6, $7, $8)",
    )
    .bind(LICENSE_4)
    .bind(HUMAN3)
    .bind(APP)
    .bind(PLAN)
    .bind(at(EARLY))
    .bind(at(EARLY))
    .bind(at("2026-06-20T00:00:00Z"))
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("revoked license");
    // P5: existed only after the period ended.
    licence(
        &db,
        LICENSE_5,
        HUMAN4,
        APP,
        PLAN,
        "active",
        Some(EARLY),
        None,
        "2026-08-01T00:00:00Z",
    )
    .await;

    let window = "period_start=2026-06-01T00:00:00Z&period_end=2026-07-01T00:00:00Z";
    let (status, body) = get(
        internal_route(&db),
        &format!("{}?{window}", seat_path(&TENANT.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seats"], 3);
    assert_eq!(body["subjects"], 3);
    assert_eq!(body["human_subjects"], 2);
    assert_eq!(body["service_subjects"], 1);
    assert_eq!(body["unresolved_subjects"], 0);
    assert_eq!(body["basis"], "active_during_period");
    assert!(
        body["period_start"]
            .as_str()
            .expect("a datetime")
            .starts_with("2026-06-01T00:00:00"),
        "{body}"
    );
    assert!(
        body["period_end"]
            .as_str()
            .expect("a datetime")
            .starts_with("2026-07-01T00:00:00"),
        "{body}"
    );

    // `as_of` is IGNORED for the count in period mode: a wildly different instant changes
    // nothing but the echo.
    let (status, body) = get(
        internal_route(&db),
        &format!(
            "{}?{window}&as_of=2020-01-01T00:00:00Z",
            seat_path(&TENANT.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seats"], 3);
    assert_eq!(body["subjects"], 3);
    assert_eq!(body["basis"], "active_during_period");
}

#[tokio::test]
async fn period_bounds_must_arrive_together_and_in_order() {
    let db = seeded().await;

    let (status, body) = get(
        internal_route(&db),
        &format!(
            "{}?period_start=2026-06-01T00:00:00Z",
            seat_path(&TENANT.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["detail"],
        "period_start and period_end must be given together"
    );

    let (status, body) = get(
        internal_route(&db),
        &format!(
            "{}?period_start=2026-07-01T00:00:00Z&period_end=2026-06-01T00:00:00Z",
            seat_path(&TENANT.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"], "period_start must be before period_end");
}

#[tokio::test]
async fn an_unknown_tenant_is_404_and_a_live_name_resolves() {
    let db = seeded().await;

    let unknown = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_00ff);
    let (status, body) = get(internal_route(&db), &seat_path(&unknown.to_string())).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"], "Tenant not found");

    let (status, body) = get(internal_route(&db), &seat_path("no-such-tenant")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["detail"], "Tenant not found");

    // A non-UUID path segment resolves by NAME, as the reference's fallback does.
    let (status, body) = get(internal_route(&db), &seat_path("tenant-a")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tenant_id"], TENANT.to_string());
    assert_eq!(body["seats"], 0);
    assert_eq!(body["basis"], "active_licences");
}

#[tokio::test]
async fn a_caller_without_an_internal_service_role_is_refused() {
    let db = seeded().await;
    let app = route(state(db.pool.clone()), &["license-reader"]);
    let (status, body) = get(app, &seat_path(&TENANT.to_string())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"], "Internal service role required");
}
