//! Behaviour of the five internal read routes.
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, so
//! the predicates and orderings are exercised against the captured schema rather than a mock.
//! The assertions pin the EXACT JSON the capture describes — explicit nulls included — because
//! billing parses these bodies field by field.

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
use conveyor_licenses::router::internal_reads::{
    internal_get_license, internal_get_subject, internal_list_apps, internal_list_licenses,
    internal_list_tenants,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0101);
const TENANT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0102);
const TENANT_ARCHIVED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0103);
const TENANT_SERVICE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0104);

const APP_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0201);
const APP_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0202);
const PLAN_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0301);
const PLAN_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0302);

const HUMAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0401);
const ROBOT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0402);
const OTHER: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0403);
const DELETED_SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0404);

const L_ACTIVE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0501);
const L_PENDING: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0502);
const L_EXPIRED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0503);
const L_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0504);
const L_DELETED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0505);

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("rfc3339 instant")
        .with_timezone(&Utc)
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

fn route(db: &common::TestDb, roles: &[&str]) -> Router {
    let user = User {
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        ..User::default()
    };
    Router::new()
        .route("/internal/apps", route_get(internal_list_apps))
        .route("/internal/licenses", route_get(internal_list_licenses))
        .route(
            "/internal/licenses/{license_id}",
            route_get(internal_get_license),
        )
        .route(
            "/internal/subjects/{subject_id}",
            route_get(internal_get_subject),
        )
        .route("/internal/tenants", route_get(internal_list_tenants))
        .layer(axum::Extension(user))
        .with_state(state(db.pool.clone()))
}

fn internal_route(db: &common::TestDb) -> Router {
    route(db, &["service-internal"])
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

    // created_at descending is the endpoint's documented order.
    for (id, name, kind, created_at, realm, cost_centre, currency, external_ref, archived_at) in [
        (
            TENANT,
            "tenant-a",
            "customer",
            "2026-01-01T00:00:00Z",
            Some("secure-apps"),
            Some("customers"),
            None,
            None,
            None,
        ),
        (
            TENANT_B,
            "tenant-b",
            "test",
            "2026-02-01T00:00:00Z",
            None,
            None,
            Some("EUR"),
            Some("ext-b"),
            None,
        ),
        (
            TENANT_ARCHIVED,
            "tenant-archived",
            "customer",
            "2026-03-01T00:00:00Z",
            None,
            None,
            None,
            None,
            Some("2026-04-01T00:00:00Z"),
        ),
        (
            TENANT_SERVICE,
            "svc-n8n",
            "service",
            "2026-05-01T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO tenants
                 (id, name, kind, created_at, owner_key, byo_mode, keycloak_realm,
                  cost_centre, budget_currency, external_ref, archived_at)
             VALUES ($1, $2, $3, $4, 'owner-a', 'local_allowed', $5, $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(name)
        .bind(kind)
        .bind(at(created_at))
        .bind(realm)
        .bind(cost_centre)
        .bind(currency)
        .bind(external_ref)
        .bind(archived_at.map(at))
        .execute(&db.pool)
        .await
        .expect("tenant");
    }

    for (id, key, name, audience, revoked) in [
        (APP_A, "alpha", "Alpha", "human", false),
        (APP_B, "beta", "Beta", "platform", true),
    ] {
        sqlx::query(
            "INSERT INTO app (id, key, name, created_at, audience, revoked)
             VALUES ($1, $2, $3, now(), $4, $5)",
        )
        .bind(id)
        .bind(key)
        .bind(name)
        .bind(audience)
        .bind(revoked)
        .execute(&db.pool)
        .await
        .expect("app");
    }

    for (id, app, code, name, rpm, burst, quota, budget) in [
        (
            PLAN_A,
            APP_A,
            "basic",
            "Basic",
            60,
            10,
            Some(1000),
            Some(50_000_000),
        ),
        (
            PLAN_B,
            APP_B,
            "beta-basic",
            "Beta Basic",
            120,
            20,
            None,
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO plan
                 (id, app_id, code, name, created_at, rate_limit_rpm, rate_limit_burst,
                  quota_monthly, token_budget_monthly)
             VALUES ($1, $2, $3, $4, now(), $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(app)
        .bind(code)
        .bind(name)
        .bind(rpm)
        .bind(burst)
        .bind(quota)
        .bind(budget)
        .execute(&db.pool)
        .await
        .expect("plan");
    }

    for (id, keycloak_sub, realm, tenant) in [
        (HUMAN, "sub-human", "portal", Some(TENANT)),
        (ROBOT, "sub-robot", "service", None),
        (OTHER, "sub-other", "portal", Some(TENANT_B)),
        (DELETED_SUBJECT, "sub-deleted", "portal", None),
    ] {
        sqlx::query(
            "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at, tenant_id)
             VALUES ($1, $2, $3, now(), $4)",
        )
        .bind(id)
        .bind(keycloak_sub)
        .bind(realm)
        .bind(tenant)
        .execute(&db.pool)
        .await
        .expect("subject");
    }

    licence(
        &db,
        Licence {
            id: L_ACTIVE,
            subject: HUMAN,
            tenant: TENANT,
            app: APP_A,
            plan: PLAN_A,
            status: "active",
            starts_at: "2026-01-01T00:00:00Z",
            expires_at: None,
            created_at: "2026-01-02T00:00:00Z",
            metadata: Some(json!({"source": "billing"})),
            budget: Some(123),
            quota: Some(456),
            deleted_at: None,
            deleted_from: None,
            revoked_at: None,
        },
    )
    .await;
    licence(
        &db,
        Licence {
            id: L_PENDING,
            subject: OTHER,
            tenant: TENANT_B,
            app: APP_B,
            plan: PLAN_B,
            status: "active",
            starts_at: "2027-01-01T00:00:00Z",
            expires_at: None,
            created_at: "2026-01-03T00:00:00Z",
            metadata: None,
            budget: None,
            quota: None,
            deleted_at: None,
            deleted_from: None,
            revoked_at: None,
        },
    )
    .await;
    licence(
        &db,
        Licence {
            id: L_EXPIRED,
            subject: ROBOT,
            tenant: TENANT,
            app: APP_A,
            plan: PLAN_A,
            status: "active",
            starts_at: "2026-01-01T00:00:00Z",
            expires_at: Some("2026-02-01T00:00:00Z"),
            created_at: "2026-01-04T00:00:00Z",
            metadata: None,
            budget: None,
            quota: None,
            deleted_at: None,
            deleted_from: None,
            revoked_at: None,
        },
    )
    .await;
    licence(
        &db,
        Licence {
            id: L_REVOKED,
            subject: HUMAN,
            tenant: TENANT,
            app: APP_A,
            plan: PLAN_A,
            status: "revoked",
            starts_at: "2026-01-01T00:00:00Z",
            expires_at: None,
            created_at: "2026-01-05T00:00:00Z",
            metadata: None,
            budget: None,
            quota: None,
            deleted_at: None,
            deleted_from: None,
            revoked_at: Some("2026-02-01T00:00:00Z"),
        },
    )
    .await;
    licence(
        &db,
        Licence {
            id: L_DELETED,
            subject: DELETED_SUBJECT,
            tenant: TENANT,
            app: APP_A,
            plan: PLAN_A,
            status: "revoked",
            starts_at: "2026-01-01T00:00:00Z",
            expires_at: None,
            created_at: "2026-01-06T00:00:00Z",
            metadata: None,
            budget: None,
            quota: None,
            deleted_at: Some("2026-09-01T00:00:00Z"),
            deleted_from: Some("active"),
            revoked_at: None,
        },
    )
    .await;

    db
}

struct Licence {
    id: Uuid,
    subject: Uuid,
    tenant: Uuid,
    app: Uuid,
    plan: Uuid,
    status: &'static str,
    starts_at: &'static str,
    expires_at: Option<&'static str>,
    created_at: &'static str,
    metadata: Option<Value>,
    budget: Option<i32>,
    quota: Option<i32>,
    deleted_at: Option<&'static str>,
    deleted_from: Option<&'static str>,
    revoked_at: Option<&'static str>,
}

async fn licence(db: &common::TestDb, row: Licence) {
    sqlx::query(
        "INSERT INTO license
             (id, subject_id, app_id, plan_id, tenant_id, status, starts_at, expires_at,
              created_at, metadata, token_budget_override, quota_override, deleted_at,
              deleted_from_status, revoked_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(row.id)
    .bind(row.subject)
    .bind(row.app)
    .bind(row.plan)
    .bind(row.tenant)
    .bind(row.status)
    .bind(at(row.starts_at))
    .bind(row.expires_at.map(at))
    .bind(at(row.created_at))
    .bind(row.metadata)
    .bind(row.budget)
    .bind(row.quota)
    .bind(row.deleted_at.map(at))
    .bind(row.deleted_from)
    .bind(row.revoked_at.map(at))
    .execute(&db.pool)
    .await
    .expect("license");
}

/// The exact row the capture describes — every key present, nulls explicit.
fn active_licence_json() -> Value {
    json!({
        "id": L_ACTIVE.to_string(),
        "tenant_id": TENANT.to_string(),
        "subject_id": HUMAN.to_string(),
        "status": "active",
        "effective_status": "active",
        "starts_at": "2026-01-01T00:00:00+00:00",
        "expires_at": null,
        "metadata": {"source": "billing"},
        "is_dependency": false,
        "token_budget_override": 123,
        "quota_override": 456,
        "app": {"id": APP_A.to_string(), "key": "alpha", "name": "Alpha"},
        "plan": {
            "id": PLAN_A.to_string(),
            "code": "basic",
            "name": "Basic",
            "rate_limit_rpm": 60,
            "rate_limit_burst": 10,
            "quota_monthly": 1000,
            "token_budget_monthly": 50000000,
        },
    })
}

fn ids(rows: &Value) -> Vec<String> {
    let mut out: Vec<String> = rows
        .as_array()
        .expect("array")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn the_billing_read_is_the_exact_captured_shape() {
    let db = seeded().await;
    let (status, body) = get(internal_route(&db), "/internal/licenses?status=active").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // ONE row: billing's `?status=active` applies the time-aware rule by default, so the stored
    // active-but-future and stored active-but-expired rows are not seats today.
    assert_eq!(body, json!([active_licence_json()]));
}

#[tokio::test]
async fn licence_filters_follow_the_reference() {
    let db = seeded().await;

    let (status, body) = get(
        internal_route(&db),
        &format!("/internal/licenses?tenant_id={TENANT}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        ids(&body),
        vec![
            L_ACTIVE.to_string(),
            L_EXPIRED.to_string(),
            L_REVOKED.to_string(),
        ]
    );

    // tenant_id is filtered against a UUID column; a non-UUID is the reference's empty list.
    let (status, body) = get(internal_route(&db), "/internal/licenses?tenant_id=nope").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
    let (status, body) = get(internal_route(&db), "/internal/licenses?tenant_id=").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    // app_id / plan_id filters.
    let (status, body) = get(
        internal_route(&db),
        &format!("/internal/licenses?app_id={APP_B}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![L_PENDING.to_string()]);
    let (status, body) = get(
        internal_route(&db),
        &format!("/internal/licenses?plan_id={PLAN_B}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![L_PENDING.to_string()]);

    // A malformed app_id is the reference's 400, not an empty list.
    let (status, body) = get(internal_route(&db), "/internal/licenses?app_id=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid UUID"}));

    // effective=false filters the STORED status column verbatim: the expired row is returned,
    // and its effective_status still says "expired".
    let (status, body) = get(
        internal_route(&db),
        "/internal/licenses?status=active&effective=false",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ids(&body),
        vec![
            L_ACTIVE.to_string(),
            L_PENDING.to_string(),
            L_EXPIRED.to_string(),
        ]
    );
    let expired = body
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == L_EXPIRED.to_string())
        .expect("expired row");
    assert_eq!(expired["status"], "active");
    assert_eq!(expired["effective_status"], "expired");
    let pending = body
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == L_PENDING.to_string())
        .expect("pending row");
    assert_eq!(pending["effective_status"], "pending");

    // include_deleted brings the retained soft-deleted row back in a stored-status listing.
    let (_, body) = get(internal_route(&db), "/internal/licenses").await;
    assert_eq!(
        ids(&body),
        vec![
            L_ACTIVE.to_string(),
            L_PENDING.to_string(),
            L_EXPIRED.to_string(),
            L_REVOKED.to_string(),
        ]
    );
    let (_, body) = get(
        internal_route(&db),
        "/internal/licenses?include_deleted=true",
    )
    .await;
    assert_eq!(
        ids(&body),
        vec![
            L_ACTIVE.to_string(),
            L_PENDING.to_string(),
            L_EXPIRED.to_string(),
            L_REVOKED.to_string(),
            L_DELETED.to_string(),
        ]
    );

    // A terminal status is time-invariant: `effective=true` does not change it.
    let (_, body) = get(
        internal_route(&db),
        "/internal/licenses?status=revoked&effective=true",
    )
    .await;
    assert_eq!(ids(&body), vec![L_REVOKED.to_string()]);

    // A malformed boolean is FastAPI's 422, not a 400.
    let (status, body) = get(internal_route(&db), "/internal/licenses?effective=maybe").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"][0]["loc"], json!(["query", "effective"]));
}

#[tokio::test]
async fn effective_active_at_a_past_instant_keeps_a_row_deleted_after_it() {
    let db = seeded().await;
    // The licence was deleted 2026-09-01; at 2026-08-15 it was still a seat. `include_deleted`
    // must NOT stack on top of the effective predicate — that would shrink a closed period.
    let (status, body) = get(
        internal_route(&db),
        "/internal/licenses?status=active&as_of=2026-08-15T00:00:00Z",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        ids(&body),
        vec![L_ACTIVE.to_string(), L_DELETED.to_string()]
    );
}

#[tokio::test]
async fn tenants_are_newest_first_with_service_actors_always_present() {
    let db = seeded().await;
    let (status, body) = get(internal_route(&db), "/internal/tenants").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Newest first. The service actor (33 of 59 live rows estate-wide) is NEVER filtered here:
    // this endpoint is an identity map, not a pick-list.
    assert_eq!(
        body,
        json!([
            {
                "id": TENANT_SERVICE.to_string(),
                "name": "svc-n8n",
                "keycloak_realm": null,
                "archived": false,
                "kind": "service",
                "cost_centre": null,
                "budget_currency": null,
                "owner_key": "owner-a",
                "external_ref": null,
            },
            {
                "id": TENANT_B.to_string(),
                "name": "tenant-b",
                "keycloak_realm": null,
                "archived": false,
                "kind": "test",
                "cost_centre": null,
                "budget_currency": "EUR",
                "owner_key": "owner-a",
                "external_ref": "ext-b",
            },
            {
                "id": TENANT.to_string(),
                "name": "tenant-a",
                "keycloak_realm": "secure-apps",
                "archived": false,
                "kind": "customer",
                "cost_centre": "customers",
                "budget_currency": null,
                "owner_key": "owner-a",
                "external_ref": null,
            },
        ])
    );

    let (status, body) = get(
        internal_route(&db),
        "/internal/tenants?include_archived=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().expect("array");
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["id"], TENANT_SERVICE.to_string());
    assert_eq!(rows[1]["id"], TENANT_ARCHIVED.to_string());
    assert_eq!(rows[1]["archived"], true);
}

#[tokio::test]
async fn apps_are_key_ordered_and_revoked_is_opt_in() {
    let db = seeded().await;
    let (status, body) = get(internal_route(&db), "/internal/apps").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!([{
            "id": APP_A.to_string(),
            "key": "alpha",
            "name": "Alpha",
            "revoked": false,
            "audience": "human",
        }])
    );

    let (status, body) = get(internal_route(&db), "/internal/apps?include_revoked=true").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([
            {
                "id": APP_A.to_string(),
                "key": "alpha",
                "name": "Alpha",
                "revoked": false,
                "audience": "human",
            },
            {
                "id": APP_B.to_string(),
                "key": "beta",
                "name": "Beta",
                "revoked": true,
                "audience": "platform",
            },
        ])
    );
}

#[tokio::test]
async fn subjects_resolve_by_keycloak_sub_then_id() {
    let db = seeded().await;
    let (status, body) = get(internal_route(&db), "/internal/subjects/sub-human").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let created_at = body["created_at"].clone();
    assert!(created_at.is_string(), "{body}");
    let human = json!({
        "id": HUMAN.to_string(),
        "keycloak_sub": "sub-human",
        "keycloak_realm": "portal",
        "tenant_id": TENANT.to_string(),
        "created_at": created_at,
    });
    assert_eq!(body, human);

    // The local uuid7 id resolves to the same row.
    let (status, body) = get(internal_route(&db), &format!("/internal/subjects/{HUMAN}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, human);

    // NULL tenant_id is a real answer: "nobody has stated it", emitted as an explicit null.
    let (status, body) = get(internal_route(&db), "/internal/subjects/sub-robot").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tenant_id"], Value::Null);

    // A non-UUID that matched no keycloak_sub is a 404, never a 422: the parameter is
    // deliberately polymorphic.
    let (status, body) = get(internal_route(&db), "/internal/subjects/not-a-subject").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "Subject not found"}));
    let (status, _) = get(
        internal_route(&db),
        "/internal/subjects/00000000-0000-7000-8000-0000000000ff",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_single_licence_is_the_list_row() {
    let db = seeded().await;
    let (status, body) = get(
        internal_route(&db),
        &format!("/internal/licenses/{L_ACTIVE}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, active_licence_json());

    let (status, body) = get(
        internal_route(&db),
        "/internal/licenses/00000000-0000-7000-8000-0000000000ff",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "License not found"}));

    // `get_license(UUID(license_id))`: a malformed id is the reference's 400.
    let (status, body) = get(internal_route(&db), "/internal/licenses/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid UUID"}));
}

#[tokio::test]
async fn the_role_guard_refuses_every_internal_route() {
    let db = seeded().await;
    let paths = [
        "/internal/licenses".to_owned(),
        format!("/internal/licenses/{L_ACTIVE}"),
        "/internal/tenants".to_owned(),
        "/internal/apps".to_owned(),
        "/internal/subjects/sub-human".to_owned(),
    ];
    for roles in [vec![], vec!["license-reader"]] {
        let app = route(&db, &roles);
        for path in &paths {
            let (status, body) = get(app.clone(), path).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path} {roles:?}");
            assert_eq!(body, json!({"detail": "Internal service role required"}));
        }
    }
}

/// The served document's projection for these five operations, against the capture.
///
/// The comparison runs through the SAME shrink-only projection `--check-lock` uses, so this pins
/// what that check will compare once the rest of the document is ported. Header parameters are
/// removed from both sides here: the capture encodes `X-Tenant-Id` as a `$ref` into
/// `components.parameters` while utoipa emits it inline (the projection resolves the reference, so
/// the two now compare equal regardless), and this test is about the five operations' own
/// parameters and response schemas.
#[test]
fn the_five_operations_match_the_captured_contract() {
    use conveyor_licenses::contract::normalize::normalize;

    let served = serde_json::to_value(conveyor_licenses::router::openapi()).expect("serialises");
    let captured = conveyor_licenses::contract::lock().expect("the capture parses");

    fn without_header_parameters(item: &Value) -> Value {
        let mut item = item.clone();
        if let Some(parameters) = item
            .get_mut("get")
            .and_then(|operation| operation.get_mut("parameters"))
            .and_then(Value::as_array_mut)
        {
            parameters.retain(|parameter| {
                parameter.get("in").and_then(Value::as_str) != Some("header")
                    && parameter.get("$ref").is_none()
            });
        }
        item
    }

    for path in [
        "/internal/apps",
        "/internal/licenses",
        "/internal/licenses/{license_id}",
        "/internal/subjects/{subject_id}",
        "/internal/tenants",
    ] {
        let mut paths = serde_json::Map::new();
        paths.insert(
            path.to_owned(),
            without_header_parameters(&served["paths"][path]),
        );
        let mine = normalize(&json!({"paths": paths}));
        let mut paths = serde_json::Map::new();
        paths.insert(
            path.to_owned(),
            without_header_parameters(&captured["paths"][path]),
        );
        let theirs = normalize(&json!({"paths": paths}));
        assert_eq!(mine, theirs, "{path} drifted from the capture");
    }

    for name in [
        "InternalAppResponse",
        "InternalLicenseResponse",
        "InternalSubjectResponse",
        "InternalTenantResponse",
    ] {
        let mut schemas = serde_json::Map::new();
        schemas.insert(
            name.to_owned(),
            served["components"]["schemas"][name].clone(),
        );
        let mine = normalize(&json!({"components": {"schemas": schemas}}));
        let mut schemas = serde_json::Map::new();
        schemas.insert(
            name.to_owned(),
            captured["components"]["schemas"][name].clone(),
        );
        let theirs = normalize(&json!({"components": {"schemas": schemas}}));
        assert_eq!(mine, theirs, "{name} drifted from the capture");
    }
}
