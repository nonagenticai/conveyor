//! Behaviour of the eight tenant CRUD/settings operations (#45, slice 45).
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded
//! baseline, so name resolution, per-owner uniqueness, the archive side effects and the
//! currency/cost-centre rules are exercised against the captured schema rather than a
//! mock. The assertions pin the EXACT JSON the capture describes (eleven properties,
//! explicit nulls included) and the reference's exact refusal bodies.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::{get, patch, put};
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::tenants_crud::{
    create_tenant, get_tenant, list_tenants, register_tenant, rename_tenant, set_tenant_archived,
    set_tenant_budget_currency, set_tenant_cost_centre,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const T1: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2101); // acme / fr
const T2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2102); // beta / commonpractice
const T3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2103); // retired / fr, archived
const T4: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2104); // svc-mesh / fr, service
const T5: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2105); // shared / fr
const T6: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2106); // shared / commonpractice
const T7: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2107); // delta / fr
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2201);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2301);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2401);
const LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2501);

fn state(pool: PgPool) -> AppState {
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
        sub: "caller-sub".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: tenant.map(|tenant| tenant.to_string()),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

fn crossing(owner: &str) -> User {
    User {
        cross_owner: true,
        ..user(&["license-admin"], None, Some(owner))
    }
}

/// The eight routes alone, with the caller the middleware would have built.
///
/// `/tenants/{name_or_id}` carries all of GET/PATCH/PUT: axum refuses two wildcard names at
/// one path position, and the capture's PUT spells that parameter `tenant_id`.
fn route(db: &common::TestDb, user: User) -> Router {
    Router::new()
        .route("/tenants", get(list_tenants).post(create_tenant))
        .route(
            "/tenants/{name_or_id}",
            get(get_tenant).patch(rename_tenant).put(register_tenant),
        )
        .route("/tenants/{name_or_id}/archive", patch(set_tenant_archived))
        .route(
            "/tenants/{name_or_id}/budget-currency",
            put(set_tenant_budget_currency),
        )
        .route(
            "/tenants/{name_or_id}/cost-centre",
            patch(set_tenant_cost_centre),
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

async fn get_json(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::GET, path, None).await
}

fn ids(body: &Value) -> Vec<String> {
    body.as_array()
        .expect("a bare JSON array")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect()
}

fn uuids(rows: &[Uuid]) -> Vec<String> {
    rows.iter().map(Uuid::to_string).collect()
}

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    for (key, display) in [
        ("platform", "Platform"),
        ("fr", "FR"),
        ("commonpractice", "Common Practice"),
        ("w2v", "W2V"),
    ] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $2)")
            .bind(key)
            .bind(display)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }
    // (id, name, owner, kind, byo_mode, created_at, archived_at, external_ref, cost_centre)
    for (id, name, owner, kind, byo_mode, created, archived, external_ref, cost_centre) in [
        (
            T1,
            "acme",
            "fr",
            "customer",
            "byo_required",
            "01-01",
            None,
            None,
            Some("service"),
        ),
        (
            T2,
            "beta",
            "commonpractice",
            "customer",
            "byo_required",
            "01-02",
            None,
            None,
            None,
        ),
        (
            T3,
            "retired",
            "fr",
            "customer",
            "byo_required",
            "01-03",
            Some("2026-02-01T00:00:00Z"),
            None,
            None,
        ),
        (
            T4,
            "svc-mesh",
            "fr",
            "service",
            "local_allowed",
            "01-04",
            None,
            None,
            None,
        ),
        (
            T5,
            "shared",
            "fr",
            "customer",
            "byo_required",
            "01-05",
            None,
            None,
            None,
        ),
        (
            T6,
            "shared",
            "commonpractice",
            "customer",
            "byo_required",
            "01-06",
            None,
            None,
            None,
        ),
        (
            T7,
            "delta",
            "fr",
            "customer",
            "byo_required",
            "01-07",
            None,
            Some("fr-delta"),
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO tenants
                 (id, name, owner_key, kind, created_at, archived_at, external_ref,
                  cost_centre, byo_mode)
             VALUES ($1, $2, $3, $4, $5::timestamptz, $6::timestamptz, $7, $8, $9)",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .bind(kind)
        .bind(format!("2026-{created}T00:00:00Z"))
        .bind(archived)
        .bind(external_ref)
        .bind(cost_centre)
        .bind(byo_mode)
        .execute(&db.pool)
        .await
        .expect("tenant");
    }
    // An ACTIVE grant on T1 to w2v (the grantee case) and a REVOKED one on T2 to w2v.
    for (tenant, grantee, revoked) in [(T1, "w2v", None), (T2, "w2v", Some("2026-03-01T00:00:00Z"))]
    {
        sqlx::query(
            "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by, revoked_at)
             VALUES ($1, $2, 'test', $3::timestamptz)",
        )
        .bind(tenant)
        .bind(grantee)
        .bind(revoked)
        .execute(&db.pool)
        .await
        .expect("grant");
    }

    // One licence + token on T1: archiving must leave all of them intact.
    sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, 'demo', 'Demo', now())")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at)
         VALUES ($1, $2, 'basic', 'Basic', now())",
    )
    .bind(PLAN)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("plan");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, 'sub-1', 'portal', now())",
    )
    .bind(SUBJECT)
    .execute(&db.pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license
             (id, subject_id, app_id, plan_id, tenant_id, status, starts_at, created_at)
         VALUES ($1, $2, $3, $4, $5, 'active', '2026-01-01T00:00:00Z', now())",
    )
    .bind(LICENCE)
    .bind(SUBJECT)
    .bind(APP)
    .bind(PLAN)
    .bind(T1)
    .execute(&db.pool)
    .await
    .expect("license");
    sqlx::query(
        "INSERT INTO token (license_id, token_hash, token_type) VALUES ($1, 'hash-1', 'api')",
    )
    .bind(LICENCE)
    .execute(&db.pool)
    .await
    .expect("token");

    db
}

fn shape(row: &Value) -> &serde_json::Map<String, Value> {
    row.as_object().expect("an object")
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn the_listing_is_role_scoped_filtered_and_ordered() {
    let db = seeded().await;

    // tenant-registry-reader: the full live roster, newest first; archived and service hidden.
    let registry = user(&["tenant-registry-reader"], None, None);
    let (status, body) = get_json(route(&db, registry.clone()), "/tenants").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), uuids(&[T7, T6, T5, T2, T1]));

    // Both filters.
    let (status, body) = get_json(
        route(&db, registry),
        "/tenants?include_archived=true&include_service_actors=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), uuids(&[T7, T6, T5, T4, T3, T2, T1]));

    // The captured eleven-property row, explicit nulls included.
    let acme = body
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == T1.to_string())
        .expect("acme");
    assert_eq!(shape(acme).len(), 11);
    assert_eq!(
        acme,
        &json!({
            "id": T1.to_string(),
            "name": "acme",
            "keycloak_realm": null,
            "byo_mode": "byo_required",
            "budget_currency": null,
            "kind": "customer",
            "cost_centre": "service",
            "owner_key": "fr",
            "external_ref": null,
            "archived_at": null,
            "created_at": "2026-01-01T00:00:00+00:00",
        })
    );

    // A license-admin owned by fr sees what fr may ACT ON: owned + actively granted only.
    let (status, body) = get_json(
        route(&db, user(&["license-admin"], None, Some("fr"))),
        "/tenants",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), uuids(&[T7, T5, T1]));
    // w2v owns nothing but holds the ACTIVE grant on T1; its revoked T2 grant is not access.
    let (status, body) = get_json(
        route(&db, user(&["license-admin"], None, Some("w2v"))),
        "/tenants",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), uuids(&[T1]));

    // A plain license-reader is its own tenant only — and needs a tenant context at all.
    let (status, body) = get_json(
        route(&db, user(&["license-reader"], Some(T1), None)),
        "/tenants",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), uuids(&[T1]));
    let (status, body) = get_json(
        route(&db, user(&["license-reader"], None, None)),
        "/tenants",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body, json!({"detail": "Missing tenant context"}));

    // license-manager is not a read role here.
    let (status, body) = get_json(
        route(&db, user(&["license-manager"], None, None)),
        "/tenants",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-reader, tenant-registry-reader"})
    );

    // A malformed boolean is FastAPI's 422.
    let (status, body) = get_json(
        route(&db, user(&["tenant-registry-reader"], None, None)),
        "/tenants?include_archived=maybe",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "bool_parsing");
}

#[tokio::test]
async fn create_mints_a_tenant_with_the_platform_grant_and_is_idempotent_by_external_ref() {
    let db = seeded().await;
    let fr = user(&["license-manager"], None, Some("fr"));

    let (status, body) = send(
        route(&db, fr.clone()),
        Method::POST,
        "/tenants",
        Some(json!({"name": "gamma"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(shape(&body).len(), 11);
    assert_eq!(body["name"], "gamma");
    assert_eq!(body["owner_key"], "fr");
    assert_eq!(body["kind"], "unclassified");
    assert_eq!(body["byo_mode"], "byo_required");
    assert!(body["keycloak_realm"].is_null());
    assert!(body["budget_currency"].is_null());
    assert!(body["cost_centre"].is_null());
    assert!(body["external_ref"].is_null());
    assert!(body["archived_at"].is_null());
    assert!(
        body["created_at"]
            .as_str()
            .expect("created_at")
            .ends_with("+00:00"),
        "{body}"
    );
    let id = Uuid::try_parse(body["id"].as_str().expect("id")).expect("uuid");
    // The platform grant was written in the SAME transaction.
    let grant: Option<String> = sqlx::query_scalar(
        "SELECT granted_by FROM tenant_app_grant
          WHERE tenant_id = $1 AND grantee_owner_key = 'platform' AND revoked_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await
    .expect("grant");
    assert_eq!(grant.as_deref(), Some("platform-infrastructure"));

    // external_ref is the retry-safe path: the SECOND call answers 200 with the same row.
    let (status, first) = send(
        route(&db, fr.clone()),
        Method::POST,
        "/tenants",
        Some(json!({"name": "epsilon", "external_ref": "fr-epsilon"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, second) = send(
        route(&db, fr.clone()),
        Method::POST,
        "/tenants",
        Some(json!({"name": "epsilon", "external_ref": "fr-epsilon"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["id"], first["id"]);

    // A name taken by a LIVE tenant of THIS app is the reference's 409 object.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::POST,
        "/tenants",
        Some(json!({"name": "acme"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "tenant_name_taken",
            "name": "acme",
            "message": "tenant name already in use by this app: 'acme'",
        }})
    );
    // Another app's name, and an ARCHIVED one's name, are NOT conflicts.
    for name in ["beta", "retired"] {
        let (status, body) = send(
            route(&db, fr.clone()),
            Method::POST,
            "/tenants",
            Some(json!({"name": name})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{name}: {body}");
    }

    // An unregistered client is refused, never given a default owner.
    let (status, body) = send(
        route(&db, user(&["license-manager"], None, None)),
        Method::POST,
        "/tenants",
        Some(json!({"name": "orphan"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "unknown_api_client");

    // Role gate and the Pydantic body.
    let (status, body) = send(
        route(&db, user(&["license-reader"], None, Some("fr"))),
        Method::POST,
        "/tenants",
        Some(json!({"name": "nope"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-manager"})
    );
    for (payload, entry_type, loc) in [
        (json!({}), "missing", json!(["body", "name"])),
        (
            json!({"name": "Bad_Name"}),
            "string_pattern_mismatch",
            json!(["body", "name"]),
        ),
        (
            json!({"name": "bad", "budget_currency": "GBP"}),
            "literal_error",
            json!(["body", "budget_currency"]),
        ),
    ] {
        let (status, body) = send(
            route(&db, fr.clone()),
            Method::POST,
            "/tenants",
            Some(payload.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{payload}: {body}"
        );
        assert_eq!(body["detail"][0]["type"], entry_type, "{payload}: {body}");
        assert_eq!(body["detail"][0]["loc"], loc, "{payload}: {body}");
    }
}

#[tokio::test]
async fn register_is_an_idempotent_upsert_scoped_to_the_owner() {
    let db = seeded().await;
    let fr = user(&["license-admin"], None, Some("fr"));
    let new_id = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2999);

    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PUT,
        &format!("/tenants/{new_id}"),
        Some(json!({"name": "registered", "kind": "service", "cost_centre": "ci"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["id"], new_id.to_string());
    assert_eq!(body["owner_key"], "fr");
    assert_eq!(body["kind"], "service");
    // migration 033: a service principal is born byo_mode='local_allowed'.
    assert_eq!(body["byo_mode"], "local_allowed");
    assert_eq!(body["cost_centre"], "ci");
    let grants = count(
        &db.pool,
        "SELECT count(*) FROM tenant_app_grant WHERE grantee_owner_key = 'platform'",
    )
    .await;
    assert_eq!(
        grants, 1,
        "one platform grant per non-platform tenant created"
    );

    // Idempotent: the same id returns 200 UNCHANGED; a grantee's body name is ignored.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PUT,
        &format!("/tenants/{new_id}"),
        Some(json!({"name": "renamed-by-register"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "registered");

    // Another app's id without a grant is a 409; with an ACTIVE grant it is a 200.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PUT,
        &format!("/tenants/{T2}"),
        Some(json!({"name": "beta"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owned_by_other_app");
    let (status, body) = send(
        route(&db, user(&["license-manager"], None, Some("w2v"))),
        Method::PUT,
        &format!("/tenants/{T1}"),
        Some(json!({"name": "acme-ignored"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "acme");
    // cross_owner re-registers any id, unchanged.
    let (status, body) = send(
        route(&db, crossing("stranger")),
        Method::PUT,
        &format!("/tenants/{T2}"),
        Some(json!({"name": "beta"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["owner_key"], "commonpractice");

    // The caller's own app already has a different LIVE tenant with this name.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PUT,
        &format!("/tenants/{}", Uuid::nil()),
        Some(json!({"name": "acme"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "tenant_name_taken",
            "name": "acme",
            "message": "tenant name already in use by this app: 'acme'",
        }})
    );

    // A non-UUID path id is the reference's 400 (not FastAPI's 422).
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PUT,
        "/tenants/not-a-uuid",
        Some(json!({"name": "whatever"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .expect("detail")
            .contains("Invalid path tenant_id 'not-a-uuid'")
    );

    // A Keycloak principal id may never be registered as a tenant.
    let principal = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2888);
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2887))
    .bind(principal.to_string())
    .execute(&db.pool)
    .await
    .expect("subject");
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PUT,
        &format!("/tenants/{principal}"),
        Some(json!({"name": "principal-as-tenant"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .expect("a string detail")
            .contains("Keycloak principal id")
    );

    // An invalid kind is FastAPI's literal_error.
    let (status, body) = send(
        route(&db, fr),
        Method::PUT,
        &format!("/tenants/{}", Uuid::nil()),
        Some(json!({"name": "k", "kind": "robot"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "literal_error");
    assert_eq!(body["detail"][0]["loc"], json!(["body", "kind"]));
}

#[tokio::test]
async fn get_resolves_by_id_or_name_and_never_leaks_a_foreign_tenant() {
    let db = seeded().await;
    let fr_admin = user(&["license-admin"], None, Some("fr"));

    let (status, body) = get_json(route(&db, fr_admin.clone()), "/tenants/acme").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], T1.to_string());
    let (status, body) = get_json(route(&db, fr_admin.clone()), &format!("/tenants/{T1}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "acme");

    // A foreign tenant is 404 by id, never 403.
    let (status, body) = get_json(route(&db, fr_admin.clone()), &format!("/tenants/{T2}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
    // A name is LIVE-only: the archived row is reachable by id.
    let (status, _) = get_json(route(&db, fr_admin.clone()), "/tenants/retired").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = get_json(route(&db, fr_admin.clone()), &format!("/tenants/{T3}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["archived_at"].is_string());

    // A bare name matching several owners is the reference's 409, never a pick.
    let (status, body) = get_json(route(&db, crossing("stranger")), "/tenants/shared").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "tenant_name_ambiguous",
            "name": "shared",
            "message": "this tenant name is used by several owning apps; use the id",
        }})
    );
    // Unique across owners: cross_owner resolves it.
    let (status, body) = get_json(route(&db, crossing("stranger")), "/tenants/beta").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], T2.to_string());

    // A non-admin resolves its OWN tenant only.
    let reader = user(&["license-reader"], Some(T1), None);
    let (status, _) = get_json(route(&db, reader.clone()), "/tenants/acme").await;
    assert_eq!(status, StatusCode::OK);
    for path in ["/tenants/beta".to_owned(), format!("/tenants/{T2}")] {
        let (status, body) = get_json(route(&db, reader.clone()), &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_eq!(body, json!({"detail": "Tenant not found"}));
    }
    // No tenant context at all is a 401.
    let (status, body) = get_json(
        route(&db, user(&["license-reader"], None, None)),
        "/tenants/acme",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body, json!({"detail": "Missing tenant context"}));

    // A role that may not read is refused with the reference's detail.
    let (status, body) = get_json(
        route(&db, user(&["license-manager"], Some(T1), None)),
        "/tenants/acme",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-reader"})
    );
}

#[tokio::test]
async fn rename_is_owner_only_and_conflict_checked() {
    let db = seeded().await;
    let fr = user(&["license-admin"], None, Some("fr"));

    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/acme",
        Some(json!({"name": "acme-two"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "acme-two");
    let stored: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
        .bind(T1)
        .fetch_one(&db.pool)
        .await
        .expect("name");
    assert_eq!(stored, "acme-two");

    // Idempotent: renaming to the current name is a 200 no-op.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T1}"),
        Some(json!({"name": "acme-two"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Another OWNER's name is not a clash (names are unique per owner).
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T1}"),
        Some(json!({"name": "beta"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // This owner's own live name IS a clash.
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_key, created_at)
         VALUES ($1, 'taken', 'fr', now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2777))
    .execute(&db.pool)
    .await
    .expect("taken");
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T1}"),
        Some(json!({"name": "taken"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "tenant_name_taken",
            "name": "taken",
            "message": "tenant name already in use: 'taken'",
        }})
    );

    // A grantee is refused owner-only; cross_owner is admitted; unknown client refused.
    let (status, body) = send(
        route(&db, user(&["license-manager"], None, Some("w2v"))),
        Method::PATCH,
        &format!("/tenants/{T1}"),
        Some(json!({"name": "grantee-rename"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owner_only");
    let (status, body) = send(
        route(&db, crossing("stranger")),
        Method::PATCH,
        &format!("/tenants/{T2}"),
        Some(json!({"name": "beta-two"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "beta-two");
    let (status, body) = send(
        route(&db, user(&["license-admin"], None, None)),
        Method::PATCH,
        &format!("/tenants/{T2}"),
        Some(json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "unknown_api_client");

    // An unknown tenant is 404; an archived one is renameable by id only.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{}", Uuid::nil()),
        Some(json!({"name": "ghost"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/retired",
        Some(json!({"name": "archived-name"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = send(
        route(&db, fr),
        Method::PATCH,
        &format!("/tenants/{T3}"),
        Some(json!({"name": "archived-name"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "archived-name");

    // The Pydantic name rule.
    let (status, body) = send(
        route(&db, user(&["license-admin"], None, Some("fr"))),
        Method::PATCH,
        &format!("/tenants/{T1}"),
        Some(json!({"name": "Not_Valid"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "string_pattern_mismatch");
}

#[tokio::test]
async fn archive_writes_only_archived_at_and_restores_or_conflicts() {
    let db = seeded().await;
    let fr = user(&["license-admin"], None, Some("fr"));
    let before = (
        count(&db.pool, "SELECT count(*) FROM license").await,
        count(&db.pool, "SELECT count(*) FROM token").await,
        count(&db.pool, "SELECT count(*) FROM tenant_app_grant").await,
    );

    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/acme/archive",
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let archived_at = body["archived_at"].clone();
    assert!(archived_at.is_string(), "{body}");
    // Revokes NOTHING: every dependent row survives, and the token stays usable.
    let after = (
        count(&db.pool, "SELECT count(*) FROM license").await,
        count(&db.pool, "SELECT count(*) FROM token").await,
        count(&db.pool, "SELECT count(*) FROM tenant_app_grant").await,
    );
    assert_eq!(before, after);
    let revoked: bool = sqlx::query_scalar("SELECT revoked FROM token WHERE license_id = $1")
        .bind(LICENCE)
        .fetch_one(&db.pool)
        .await
        .expect("token");
    assert!(!revoked);
    // The licence row is untouched too.
    let status_column: String = sqlx::query_scalar("SELECT status FROM license WHERE id = $1")
        .bind(LICENCE)
        .fetch_one(&db.pool)
        .await
        .expect("license");
    assert_eq!(status_column, "active");

    // Re-archiving keeps the ORIGINAL timestamp.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T1}/archive"),
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["archived_at"], archived_at);

    // Restore clears it.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T1}/archive"),
        Some(json!({"archived": false})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["archived_at"].is_null());

    // Restoring into a taken NAME is the reference's 409.
    send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T5}/archive"),
        Some(json!({"archived": true})),
    )
    .await;
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_key, created_at)
         VALUES ($1, 'shared', 'fr', now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2778))
    .execute(&db.pool)
    .await
    .expect("name clash");
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T5}/archive"),
        Some(json!({"archived": false})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "tenant_name_taken",
            "tenant_id": T5.to_string(),
            "message": "a live tenant of this app now holds this tenant's name; rename the archived tenant before restoring it",
        }})
    );

    // Restoring into a taken EXTERNAL_REF is its own 409.
    send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T7}/archive"),
        Some(json!({"archived": true})),
    )
    .await;
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_key, external_ref, created_at)
         VALUES ($1, 'delta-two', 'fr', 'fr-delta', now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2779))
    .execute(&db.pool)
    .await
    .expect("ref clash");
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        &format!("/tenants/{T7}/archive"),
        Some(json!({"archived": false})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_external_ref_taken");
    assert_eq!(body["detail"]["tenant_id"], T7.to_string());

    // Owner-only, and an unknown tenant is 404.
    let (status, body) = send(
        route(&db, user(&["license-manager"], None, Some("w2v"))),
        Method::PATCH,
        &format!("/tenants/{T1}/archive"),
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owner_only");
    let (status, body) = send(
        route(&db, fr),
        Method::PATCH,
        &format!("/tenants/{}/archive", Uuid::nil()),
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn budget_currency_is_owned_and_immutable_after_the_first_currency() {
    let db = seeded().await;
    let fr_manager = user(&["license-manager"], None, Some("fr"));

    // null -> USD, then the idempotent no-op.
    let (status, body) = send(
        route(&db, fr_manager.clone()),
        Method::PUT,
        "/tenants/delta/budget-currency",
        Some(json!({"budget_currency": "USD"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["budget_currency"], "USD");
    let (status, body) = send(
        route(&db, fr_manager.clone()),
        Method::PUT,
        "/tenants/delta/budget-currency",
        Some(json!({"budget_currency": "USD"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["budget_currency"], "USD");

    // A SWITCH needs the owner capability.
    let (status, body) = send(
        route(&db, fr_manager.clone()),
        Method::PUT,
        "/tenants/delta/budget-currency",
        Some(json!({"budget_currency": "EUR"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "budget_currency_immutable");
    let (status, body) = send(
        route(&db, user(&["platform-admin"], None, Some("fr"))),
        Method::PUT,
        "/tenants/delta/budget-currency",
        Some(json!({"budget_currency": "EUR"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["budget_currency"], "EUR");

    // An archived tenant is refused even when it holds no currency.
    let (status, body) = send(
        route(&db, user(&["license-admin"], None, Some("fr"))),
        Method::PUT,
        &format!("/tenants/{T3}/budget-currency"),
        Some(json!({"budget_currency": "CAD"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_archived");
    let unchanged: Option<String> =
        sqlx::query_scalar("SELECT budget_currency::text FROM tenants WHERE id = $1")
            .bind(T3)
            .fetch_one(&db.pool)
            .await
            .expect("row");
    assert_eq!(unchanged, None);

    // The body's Literal, the role gate, the grantee rule and an unknown tenant.
    let (status, body) = send(
        route(&db, fr_manager.clone()),
        Method::PUT,
        "/tenants/delta/budget-currency",
        Some(json!({"budget_currency": "GBP"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "literal_error");
    assert_eq!(body["detail"][0]["loc"], json!(["body", "budget_currency"]));
    let (status, body) = send(
        route(&db, user(&["license-reader"], None, Some("fr"))),
        Method::PUT,
        "/tenants/delta/budget-currency",
        Some(json!({"budget_currency": "EUR"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-manager, platform-admin"})
    );
    let (status, body) = send(
        route(&db, user(&["license-manager"], None, Some("w2v"))),
        Method::PUT,
        &format!("/tenants/{T1}/budget-currency"),
        Some(json!({"budget_currency": "EUR"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owner_only");
    let (status, body) = send(
        route(&db, fr_manager),
        Method::PUT,
        &format!("/tenants/{}/budget-currency", Uuid::nil()),
        Some(json!({"budget_currency": "EUR"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
}

#[tokio::test]
async fn cost_centre_is_validated_against_the_closed_vocabulary() {
    let db = seeded().await;
    let fr = user(&["license-manager"], None, Some("fr"));

    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/delta/cost-centre",
        Some(json!({"cost_centre": "ci"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cost_centre"], "ci");
    let stored: Option<String> =
        sqlx::query_scalar("SELECT cost_centre FROM tenants WHERE id = $1")
            .bind(T7)
            .fetch_one(&db.pool)
            .await
            .expect("row");
    assert_eq!(stored.as_deref(), Some("ci"));

    // `null` CLEARS the value deliberately.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/delta/cost-centre",
        Some(json!({"cost_centre": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["cost_centre"].is_null());

    // An unlisted centre is refused 422 with the DAL's actionable string detail.
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/delta/cost-centre",
        Some(json!({"cost_centre": "marketing"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .expect("a string detail")
            .contains("invalid cost_centre")
    );
    // A missing key is FastAPI's 422 array (cost_centre is REQUIRED, nullable).
    let (status, body) = send(
        route(&db, fr.clone()),
        Method::PATCH,
        "/tenants/delta/cost-centre",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["detail"][0]["type"], "missing");

    // No archived rule on this route, exactly as the reference.
    let (status, body) = send(
        route(&db, user(&["license-admin"], None, Some("fr"))),
        Method::PATCH,
        &format!("/tenants/{T3}/cost-centre"),
        Some(json!({"cost_centre": "tests"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cost_centre"], "tests");

    // Owner-only, and an unknown tenant is 404.
    let (status, body) = send(
        route(&db, user(&["license-manager"], None, Some("w2v"))),
        Method::PATCH,
        &format!("/tenants/{T1}/cost-centre"),
        Some(json!({"cost_centre": "ci"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owner_only");
    let (status, body) = send(
        route(&db, fr),
        Method::PATCH,
        &format!("/tenants/{}/cost-centre", Uuid::nil()),
        Some(json!({"cost_centre": "ci"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
}
