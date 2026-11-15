//! Behaviour of the six catalogue APP routes, against a real Postgres 18 built by `common`.
//!
//! Each case pins the reference's observable contract: the exact response JSON (explicit
//! nulls included), the role and owner guards, the create-time issuer rules, and the
//! delete cascade's blast-radius gate and side effects. The wire documents are the
//! capture's (`contract/openapi.lock.json`), not a rendering of the queries.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::get as route_get;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::catalogue_apps::{
    create_app, delete_app, get_app, list_app_licenses, list_app_plans, list_apps,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

// Tenant A is lexicographically before tenant B, so the 409's sorted tenant set is stable.
const TENANT_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2101);
const TENANT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2102);
const SUBJECT_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2201);
const SUBJECT_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2202);
const APP_ALPHA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2301);
const APP_ZETA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2302);
const APP_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2303);
const APP_DELETE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2304);
const APP_EMPTY: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2305);
const PLAN_ALPHA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2401);
const PLAN_ALPHA_REVOKED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2402);
const PLAN_ZETA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2403);
const PLAN_DELETE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2404);
const LIC_ALPHA_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2501);
const LIC_ALPHA_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2502);
const LIC_ZETA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2503);
const LIC_DELETE_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2504);
const LIC_DELETE_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2505);
const TOKEN_DELETE_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2601);
const TOKEN_DELETE_B: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_2602);

const CREATED: &str = "2026-01-02T03:04:05+00:00";

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

fn user(roles: &[&str], sub: &str, tenant: Option<Uuid>, owner: Option<&str>) -> User {
    User {
        sub: sub.to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        tenant_id: tenant.map(|tenant| tenant.to_string()),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

fn admin(owner: &str) -> User {
    user(&["license-admin"], "admin", None, Some(owner))
}

fn cross_owner() -> User {
    User {
        cross_owner: true,
        ..admin("owner-c")
    }
}

fn reader(tenant: Uuid) -> User {
    user(&["license-reader"], "reader", Some(tenant), Some("owner-a"))
}

/// The six routes alone, with the caller the middleware would have built.
fn route(db: &common::TestDb, caller: User) -> Router {
    Router::new()
        .route("/apps", route_get(list_apps).post(create_app))
        .route("/apps/{app_id}", route_get(get_app).delete(delete_app))
        .route("/apps/{app_id}/plans", route_get(list_app_plans))
        .route("/apps/{app_id}/licenses", route_get(list_app_licenses))
        .layer(axum::Extension(caller))
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
    let bytes = axum::body::to_bytes(response.into_body(), 512 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn post(app: Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, Method::POST, path, Some(body)).await
}

async fn get(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::GET, path, None).await
}

async fn delete(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::DELETE, path, None).await
}

/// Ids in a bare JSON array, sorted so an assertion never depends on row order where the
/// reference has none.
fn ids(body: &Value) -> Vec<String> {
    let mut ids: Vec<String> = body
        .as_array()
        .expect("a bare JSON array")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect();
    ids.sort();
    ids
}

fn row_with_id(body: &Value, id: Uuid) -> Value {
    body.as_array()
        .expect("a bare JSON array")
        .iter()
        .find(|row| row["id"] == id.to_string())
        .unwrap_or_else(|| panic!("row {id} in {body}"))
        .clone()
}

async fn count(db: &common::TestDb, sql: &'static str, id: Uuid) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .expect("count")
}

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    for (owner, name) in [
        ("owner-a", "Owner A"),
        ("owner-b", "Owner B"),
        ("owner-c", "Owner C"),
    ] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $2)")
            .bind(owner)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }
    for (id, name, owner) in [
        (TENANT_A, "tenant-a", "owner-a"),
        (TENANT_B, "tenant-b", "owner-b"),
    ] {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
             VALUES ($1, $2, now(), $3, 'local_allowed')",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .execute(&db.pool)
        .await
        .expect("tenant");
    }
    for (id, sub) in [(SUBJECT_A, "sub-a"), (SUBJECT_B, "sub-b")] {
        sqlx::query(
            "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
             VALUES ($1, $2, 'portal', now())",
        )
        .bind(id)
        .bind(sub)
        .execute(&db.pool)
        .await
        .expect("subject");
    }
    for (id, key, name, description, revoked, audience) in [
        (APP_ALPHA, "alpha", "Alpha", Some("first"), false, "human"),
        (APP_ZETA, "zeta", "Zeta", None, false, "platform"),
        (APP_REVOKED, "beta", "Beta", None, true, "automation"),
        (APP_DELETE, "doomed", "Doomed", None, false, "platform"),
        (APP_EMPTY, "empty", "Empty", None, false, "platform"),
    ] {
        sqlx::query(
            "INSERT INTO app (id, key, name, description, revoked, audience, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(key)
        .bind(name)
        .bind(description)
        .bind(revoked)
        .bind(audience)
        .bind(
            chrono::DateTime::parse_from_rfc3339(CREATED)
                .expect("instant")
                .with_timezone(&chrono::Utc),
        )
        .execute(&db.pool)
        .await
        .expect("app");
    }
    for (app, owner) in [
        (APP_ALPHA, "owner-a"),
        (APP_ZETA, "owner-a"),
        (APP_REVOKED, "owner-a"),
        (APP_DELETE, "owner-a"),
        (APP_EMPTY, "owner-a"),
    ] {
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(app)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    for (id, app, code, name, revoked, quota, budget) in [
        (
            PLAN_ALPHA,
            APP_ALPHA,
            "basic",
            "Basic",
            false,
            Some(1000),
            Some(50_000_000),
        ),
        (
            PLAN_ALPHA_REVOKED,
            APP_ALPHA,
            "legacy",
            "Legacy",
            true,
            None,
            None,
        ),
        (PLAN_ZETA, APP_ZETA, "plat", "Plat", false, None, None),
        (
            PLAN_DELETE,
            APP_DELETE,
            "doomed",
            "Doomed",
            false,
            None,
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO plan
                 (id, app_id, code, name, created_at, rate_limit_rpm, rate_limit_burst,
                  quota_monthly, token_budget_monthly, revoked)
             VALUES ($1, $2, $3, $4, $5, 60, 10, $6, $7, $8)",
        )
        .bind(id)
        .bind(app)
        .bind(code)
        .bind(name)
        .bind(
            chrono::DateTime::parse_from_rfc3339(CREATED)
                .expect("instant")
                .with_timezone(&chrono::Utc),
        )
        .bind(quota)
        .bind(budget)
        .bind(revoked)
        .execute(&db.pool)
        .await
        .expect("plan");
    }
    for (id, subject, tenant, app, plan, status) in [
        (
            LIC_ALPHA_A,
            SUBJECT_A,
            TENANT_A,
            APP_ALPHA,
            PLAN_ALPHA,
            "active",
        ),
        (
            LIC_ALPHA_B,
            SUBJECT_B,
            TENANT_B,
            APP_ALPHA,
            PLAN_ALPHA,
            "active",
        ),
        (LIC_ZETA, SUBJECT_A, TENANT_A, APP_ZETA, PLAN_ZETA, "active"),
        (
            LIC_DELETE_A,
            SUBJECT_A,
            TENANT_A,
            APP_DELETE,
            PLAN_DELETE,
            "active",
        ),
        (
            LIC_DELETE_B,
            SUBJECT_B,
            TENANT_B,
            APP_DELETE,
            PLAN_DELETE,
            "active",
        ),
    ] {
        let instant = chrono::DateTime::parse_from_rfc3339(CREATED)
            .expect("instant")
            .with_timezone(&chrono::Utc);
        sqlx::query(
            "INSERT INTO license
                 (id, subject_id, app_id, plan_id, tenant_id, status, starts_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $7)",
        )
        .bind(id)
        .bind(subject)
        .bind(app)
        .bind(plan)
        .bind(tenant)
        .bind(status)
        .bind(instant)
        .execute(&db.pool)
        .await
        .expect("license");
    }
    for (id, license, hash) in [
        (TOKEN_DELETE_A, LIC_DELETE_A, "hash-a"),
        (TOKEN_DELETE_B, LIC_DELETE_B, "hash-b"),
    ] {
        sqlx::query(
            "INSERT INTO token (id, license_id, token_hash, token_type, created_at)
             VALUES ($1, $2, $3, 'api', now())",
        )
        .bind(id)
        .bind(license)
        .bind(hash)
        .execute(&db.pool)
        .await
        .expect("token");
    }
    db
}

#[tokio::test]
async fn list_apps_is_the_non_revoked_catalogue_ordered_by_key() {
    let db = seeded().await;
    let (status, body) = get(route(&db, reader(TENANT_A)), "/apps").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([
            {
                "id": APP_ALPHA.to_string(),
                "key": "alpha",
                "name": "Alpha",
                "description": "first",
                "revoked": false,
                "audience": "human",
                "created_at": CREATED
            },
            {
                "id": APP_DELETE.to_string(),
                "key": "doomed",
                "name": "Doomed",
                "description": null,
                "revoked": false,
                "audience": "platform",
                "created_at": CREATED
            },
            {
                "id": APP_EMPTY.to_string(),
                "key": "empty",
                "name": "Empty",
                "description": null,
                "revoked": false,
                "audience": "platform",
                "created_at": CREATED
            },
            {
                "id": APP_ZETA.to_string(),
                "key": "zeta",
                "name": "Zeta",
                "description": null,
                "revoked": false,
                "audience": "platform",
                "created_at": CREATED
            }
        ])
    );
    // The revoked row is hidden and no role at all is refused.
    assert!(!body.to_string().contains("beta"));
    let (status, body) = get(route(&db, user(&[], "nobody", None, None)), "/apps").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": "One of roles required: license-admin, license-reader"})
    );
}

#[tokio::test]
async fn get_app_resolves_a_uuid_or_a_key_and_404s_otherwise() {
    let db = seeded().await;
    for path in [format!("/apps/{APP_ALPHA}"), "/apps/alpha".to_owned()] {
        let (status, body) = get(route(&db, admin("owner-a")), &path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(
            body,
            json!({
                "id": APP_ALPHA.to_string(),
                "key": "alpha",
                "name": "Alpha",
                "description": "first",
                "revoked": false,
                "audience": "human",
                "created_at": CREATED
            })
        );
    }
    for path in [
        format!("/apps/{}", Uuid::from_u128(9)),
        "/apps/missing".to_owned(),
    ] {
        let (status, body) = get(route(&db, admin("owner-a")), &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body, json!({"detail": "App not found"}));
    }
}

#[tokio::test]
async fn list_app_plans_hides_revoked_unless_asked() {
    let db = seeded().await;
    let (status, body) = get(route(&db, admin("owner-a")), "/apps/alpha/plans").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([{
            "id": PLAN_ALPHA.to_string(),
            "app_id": APP_ALPHA.to_string(),
            "code": "basic",
            "name": "Basic",
            "description": null,
            "region_id": null,
            "duration_days": null,
            "rate_limit_rpm": 60,
            "rate_limit_burst": 10,
            "quota_monthly": 1000,
            "token_budget_monthly": 50_000_000,
            "revoked": false,
            "created_at": CREATED
        }])
    );

    let (status, body) = get(
        route(&db, admin("owner-a")),
        "/apps/alpha/plans?include_revoked=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), {
        let mut expected = vec![PLAN_ALPHA.to_string(), PLAN_ALPHA_REVOKED.to_string()];
        expected.sort();
        expected
    });

    let (status, _) = get(
        route(&db, admin("owner-a")),
        "/apps/alpha/plans?include_revoked=maybe",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = get(route(&db, admin("owner-a")), "/apps/nope/plans").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "App not found"}));
}

#[tokio::test]
async fn list_app_licenses_is_cross_tenant_for_admin_and_scoped_for_a_reader() {
    let db = seeded().await;
    let (status, body) = get(route(&db, admin("owner-a")), "/apps/alpha/licenses").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), {
        let mut expected = vec![LIC_ALPHA_A.to_string(), LIC_ALPHA_B.to_string()];
        expected.sort();
        expected
    });
    assert_eq!(
        row_with_id(&body, LIC_ALPHA_A),
        json!({
            "id": LIC_ALPHA_A.to_string(),
            "subject_id": SUBJECT_A.to_string(),
            "tenant_id": TENANT_A.to_string(),
            "app_id": APP_ALPHA.to_string(),
            "plan_id": PLAN_ALPHA.to_string(),
            "region_id": null,
            "status": "active",
            "starts_at": CREATED,
            "expires_at": null,
            "created_at": CREATED,
            "updated_at": CREATED,
            "revoked_at": null,
            "effective_status": "active",
            "metadata": null,
            "metering_provisioned_at": null,
            "metering_verified_at": null,
            "token_budget_override": null,
            "quota_override": null,
            "is_dependency": false,
            "bundle_id": null,
            "app": {
                "id": APP_ALPHA.to_string(),
                "key": "alpha",
                "name": "Alpha"
            },
            "plan": {
                "id": PLAN_ALPHA.to_string(),
                "code": "basic",
                "name": "Basic",
                "rate_limit_rpm": 60,
                "rate_limit_burst": 10,
                "quota_monthly": 1000,
                "token_budget_monthly": 50_000_000
            }
        })
    );

    // A license-reader sees its own tenant only — the app id itself authorizes nothing.
    let (status, body) = get(route(&db, reader(TENANT_A)), "/apps/alpha/licenses").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![LIC_ALPHA_A.to_string()]);
    let (status, body) = get(route(&db, reader(TENANT_B)), "/apps/alpha/licenses").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![LIC_ALPHA_B.to_string()]);

    // The app resolves by key too, and an unknown app 404s before the tenant check.
    let (status, body) = get(route(&db, reader(TENANT_A)), "/apps/zeta/licenses").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![LIC_ZETA.to_string()]);
    let (status, body) = get(route(&db, reader(TENANT_A)), "/apps/nope/licenses").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "App not found"}));
}

#[tokio::test]
async fn create_app_is_idempotent_and_carries_the_callers_owner_as_issuer() {
    let db = seeded().await;
    let payload = json!({"key": "new-one", "name": "New One"});
    let (status, body) = post(route(&db, admin("owner-a")), "/apps", payload.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = body["id"].as_str().expect("id").to_owned();
    assert_eq!(
        body,
        json!({
            "id": id,
            "key": "new-one",
            "name": "New One",
            "description": null,
            "revoked": false,
            "audience": "platform",
            "created_at": body["created_at"]
        })
    );
    let created = Uuid::try_parse(&id).expect("a uuid");
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM app_issuer WHERE app_id = $1",
            created
        )
        .await,
        1
    );
    let issuer: Option<String> =
        sqlx::query_scalar("SELECT owner_key FROM app_issuer WHERE app_id = $1")
            .bind(created)
            .fetch_optional(&db.pool)
            .await
            .expect("issuer");
    assert_eq!(issuer.as_deref(), Some("owner-a"));

    // The same key returns the EXISTING row unchanged (200, same id, original name).
    let (status, again) = post(route(&db, admin("owner-a")), "/apps", payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["id"], id);
    assert_eq!(again["name"], "New One");

    // Explicit fields, including the closed audience enum.
    let (status, body) = post(
        route(&db, admin("owner-a")),
        "/apps",
        json!({
            "key": "for-people",
            "name": "For People",
            "description": "end-user",
            "audience": "human",
            "issuers": ["owner-a"]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["audience"], "human");
    assert_eq!(body["description"], "end-user");
}

#[tokio::test]
async fn create_app_refuses_foreign_issuers_unknown_owners_and_unknown_clients() {
    let db = seeded().await;

    let (status, body) = post(
        route(&db, admin("owner-a")),
        "/apps",
        json!({"key": "squat", "name": "Squat", "issuers": ["owner-b"]}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "issuer_not_caller_owner",
            "issuers": ["owner-b"],
            "message": "owner 'owner-a' may only name itself as an issuer"
        }})
    );

    // An unknown issuer reaches the 400 only through the cross_owner bypass: a plain caller
    // naming ANY other owner — existing or not — is refused as an issuer squatter first.
    let (status, body) = post(
        route(&db, admin("owner-a")),
        "/apps",
        json!({"key": "lost", "name": "Lost", "issuers": ["nobody"]}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"]["error"], "issuer_not_caller_owner");

    let (status, body) = post(
        route(&db, cross_owner()),
        "/apps",
        json!({"key": "lost", "name": "Lost", "issuers": ["nobody"]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "owner_not_found",
            "owner_key": "nobody",
            "message": "issuer(s) are not tenant_owner keys: nobody"
        }})
    );

    // A client with no api_client owner cannot create at all.
    let (status, body) = post(
        route(&db, user(&["license-admin"], "anon", None, None)),
        "/apps",
        json!({"key": "anon", "name": "Anon"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"]["error"], "unknown_api_client");

    // A license-reader does not hold the create role.
    let (status, body) = post(
        route(&db, reader(TENANT_A)),
        "/apps",
        json!({"key": "ro", "name": "Ro"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));

    // cross_owner may name another owner, audited; the row lands with that issuer.
    let (status, body) = post(
        route(&db, cross_owner()),
        "/apps",
        json!({"key": "break-glass", "name": "Break Glass", "issuers": ["owner-b"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::try_parse(body["id"].as_str().expect("id")).expect("uuid");
    let issuer: Option<String> =
        sqlx::query_scalar("SELECT owner_key FROM app_issuer WHERE app_id = $1")
            .bind(id)
            .fetch_optional(&db.pool)
            .await
            .expect("issuer");
    assert_eq!(issuer.as_deref(), Some("owner-b"));
}

#[tokio::test]
async fn create_app_rejects_an_invalid_body_with_fastapis_422() {
    let db = seeded().await;
    for payload in [
        json!({"name": "No Key"}),
        json!({"key": "bad-audience", "name": "X", "audience": "robot"}),
        json!({"key": "no-issuers", "name": "X", "issuers": []}),
        json!({"key": "bad-issuer", "name": "X", "issuers": [7]}),
        json!({"key": 7, "name": "X"}),
    ] {
        let (status, body) = post(route(&db, admin("owner-a")), "/apps", payload.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{payload} -> {body}"
        );
        assert!(body["detail"].is_array(), "{body}");
    }
}

#[tokio::test]
async fn delete_app_refuses_until_the_licence_count_is_confirmed() {
    let db = seeded().await;
    let (status, body) = delete(route(&db, admin("owner-a")), &format!("/apps/{APP_DELETE}")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "cascade_requires_confirmation",
            "app_id": APP_DELETE.to_string(),
            "licenses": 2,
            "tokens": 2,
            "plans": 1,
            "tenants": [TENANT_A.to_string(), TENANT_B.to_string()],
            "remedy": "re-issue with ?confirm_licenses=2 to destroy 2 licence(s) across 2 tenant(s)"
        }})
    );

    let (status, body) = delete(
        route(&db, admin("owner-a")),
        &format!("/apps/{APP_DELETE}?confirm_licenses=1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["detail"]["error"], "cascade_count_mismatch");

    // The owner rule runs BEFORE the 409, so a non-issuer learns no blast radius.
    let (status, body) = delete(route(&db, admin("owner-b")), &format!("/apps/{APP_DELETE}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "app_not_administrable_by_caller",
            "message": "owner 'owner-b' does not issue this app and may not administer it"
        }})
    );

    // Nothing was destroyed by any of the refusals.
    assert_eq!(
        count(&db, "SELECT count(*) FROM app WHERE id = $1", APP_DELETE).await,
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM license WHERE id = $1",
            LIC_DELETE_A
        )
        .await,
        1
    );
}

#[tokio::test]
async fn delete_app_is_a_204_and_cascades_to_plans_licenses_and_tokens() {
    let db = seeded().await;
    let (status, body) = delete(
        route(&db, admin("owner-a")),
        &format!("/apps/{APP_DELETE}?confirm_licenses=2"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(body, Value::Null);

    for (sql, id) in [
        ("SELECT count(*) FROM app WHERE id = $1", APP_DELETE),
        ("SELECT count(*) FROM plan WHERE id = $1", PLAN_DELETE),
        ("SELECT count(*) FROM license WHERE id = $1", LIC_DELETE_A),
        ("SELECT count(*) FROM license WHERE id = $1", LIC_DELETE_B),
        ("SELECT count(*) FROM token WHERE id = $1", TOKEN_DELETE_A),
        ("SELECT count(*) FROM token WHERE id = $1", TOKEN_DELETE_B),
    ] {
        assert_eq!(count(&db, sql, id).await, 0, "{sql} {id}");
    }

    // An app with no licences deletes without the confirmation; an unknown one 404s.
    let (status, _) = delete(route(&db, admin("owner-a")), &format!("/apps/{APP_EMPTY}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = delete(
        route(&db, admin("owner-a")),
        &format!("/apps/{}", Uuid::from_u128(9)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"detail": "App not found"}));
}

#[tokio::test]
async fn delete_app_guards_ids_roles_and_the_confirmation_type() {
    let db = seeded().await;
    let (status, body) = delete(route(&db, admin("owner-a")), "/apps/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({"detail": "Invalid app_id format"}));

    let (status, _) = delete(route(&db, reader(TENANT_A)), &format!("/apps/{APP_DELETE}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = delete(
        route(&db, admin("owner-a")),
        &format!("/apps/{APP_DELETE}?confirm_licenses=-1"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = delete(
        route(&db, admin("owner-a")),
        &format!("/apps/{APP_DELETE}?confirm_licenses=many"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // cross_owner is the documented break-glass path.
    let (status, _) = delete(
        route(&db, cross_owner()),
        &format!("/apps/{APP_DELETE}?confirm_licenses=2"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
