//! Behaviour of the six tenant grant / offering operations (#45).
//!
//! Every case runs against a real Postgres 18 created by `common` plus a stub billing server that
//! records the path, headers and JSON body of every push. The assertions pin the reference's exact
//! statuses, refusal bodies and wire shapes (explicit nulls included), and the billing push
//! semantics: the markup route/body, one bounded retry for a retryable status, no retry for a 4xx
//! verdict, a rejected or unreachable markup write that restores the posture and answers 502, and
//! an access-charge failure whose 502 must still name the markup rule it left behind.

mod common;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::routing::{get, post as route_post};
use axum::{Json, Router};
use conveyor_auth::oauth::ClientCredentials;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::billing::metering::MeteringProvisioner;
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::tenant_offerings::{
    delete_tenant_grant, list_tenant_grants, list_tenant_offerings, provision_tenant_offering,
    put_tenant_grant, unprovision_tenant_offering,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const T1: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0001); // acme / fr
const T2: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0002); // beta / commonpractice
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0010); // demo
const APP_BUNDLED: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0011);
const APP_OTHER: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0012);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0020);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0030);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0040);
const BUNDLE_LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0041);

// ---------------------------------------------------------------------------------------------
// The billing stub
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Call {
    path: String,
    tenant: Option<String>,
    authorization: Option<String>,
    body: Value,
}

#[derive(Clone)]
struct Stub {
    calls: Arc<Mutex<Vec<Call>>>,
    markup_status: Arc<Mutex<u16>>,
    access_status: Arc<Mutex<u16>>,
}

impl Stub {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            markup_status: Arc::new(Mutex::new(200)),
            access_status: Arc::new(Mutex::new(201)),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("stub lock").clone()
    }

    fn set_markup_status(&self, status: u16) {
        *self.markup_status.lock().expect("stub lock") = status;
    }

    fn set_access_status(&self, status: u16) {
        *self.access_status.lock().expect("stub lock") = status;
    }
}

fn record(stub: &Stub, path: &str, headers: &HeaderMap, body: Value) {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    stub.calls.lock().expect("stub lock").push(Call {
        path: path.to_owned(),
        tenant: header("x-tenant-id"),
        authorization: header("authorization"),
        body,
    });
}

async fn markup_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record(&stub, "/api/config/markup-rules", &headers, body);
    StatusCode::from_u16(*stub.markup_status.lock().expect("stub lock")).expect("status")
}

async fn access_push(
    AxumState(stub): AxumState<Stub>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> StatusCode {
    record(&stub, "/api/config/access-charges", &headers, body);
    StatusCode::from_u16(*stub.access_status.lock().expect("stub lock")).expect("status")
}

async fn token() -> Json<Value> {
    Json(json!({"access_token": "stub-token", "token_type": "Bearer", "expires_in": 3600}))
}

async fn start_stub(stub: Stub) -> String {
    let app = Router::new()
        .route("/api/config/markup-rules", route_post(markup_push))
        .route("/api/config/access-charges", route_post(access_push))
        .route("/token", route_post(token))
        .with_state(stub);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let address = listener.local_addr().expect("stub address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}")
}

fn provisioner(base: &str) -> MeteringProvisioner {
    let creds = ClientCredentials::new(&format!("{base}/token"), "licenses", "secret")
        .expect("client credentials");
    MeteringProvisioner::new(base, Some(creds)).expect("metering provisioner")
}

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

fn state(pool: PgPool, metering: Option<Arc<MeteringProvisioner>>) -> AppState {
    let state = AppState::new(
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
    );
    match metering {
        Some(metering) => state.with_metering(metering),
        None => state,
    }
}

fn user(roles: &[&str], owner: Option<&str>) -> User {
    User {
        sub: "caller-sub".to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        owner_key: owner.map(str::to_owned),
        azp: Some("fr-backend".to_owned()),
        iss: Some("https://kc.example/realms/secure-apps".to_owned()),
        ..User::default()
    }
}

fn reader(tenant: Uuid) -> User {
    User {
        realm_roles: vec!["license-reader".to_owned()],
        tenant_id: Some(tenant.to_string()),
        ..User::default()
    }
}

fn route(db: &common::TestDb, user: User, metering: Option<Arc<MeteringProvisioner>>) -> Router {
    Router::new()
        .route("/tenants/{name_or_id}/grants", get(list_tenant_grants))
        .route(
            "/tenants/{name_or_id}/grants/{owner_key}",
            axum::routing::put(put_tenant_grant).delete(delete_tenant_grant),
        )
        .route(
            "/tenants/{name_or_id}/offerings",
            get(list_tenant_offerings),
        )
        .route(
            "/tenants/{name_or_id}/offerings/{app_key}/{code}",
            route_post(provision_tenant_offering).delete(unprovision_tenant_offering),
        )
        .layer(axum::Extension(user))
        .with_state(state(db.pool.clone(), metering))
}

async fn send(app: Router, method: Method, path: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .expect("request");
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

// ---------------------------------------------------------------------------------------------
// Seed
// ---------------------------------------------------------------------------------------------

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    for (key, display) in [
        ("platform", "Platform"),
        ("fr", "FR"),
        ("w2v", "W2V"),
        ("commonpractice", "Common Practice"),
        ("other", "Other"),
    ] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $2)")
            .bind(key)
            .bind(display)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }
    for (id, name, owner, byo) in [
        (T1, "acme", "fr", "byo_required"),
        (T2, "beta", "commonpractice", "byo_required"),
    ] {
        sqlx::query(
            "INSERT INTO tenants (id, name, owner_key, kind, byo_mode, created_at)
             VALUES ($1, $2, $3, 'customer', $4, now())",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .bind(byo)
        .execute(&db.pool)
        .await
        .expect("tenant");
    }
    for (id, key, name) in [
        (APP, "demo", "Demo"),
        (APP_BUNDLED, "bundled", "Bundled"),
        (APP_OTHER, "other-app", "Other App"),
    ] {
        sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $3, now())")
            .bind(id)
            .bind(key)
            .bind(name)
            .execute(&db.pool)
            .await
            .expect("app");
    }
    // demo is issued by three owners: fr (T1's owner), w2v (grantee, issuer) and commonpractice
    // (issuer without a grant on T1 -> tenant_not_accessible).
    for owner in ["fr", "w2v", "commonpractice"] {
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(APP)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    for (app, owner) in [(APP_BUNDLED, "other"), (APP_OTHER, "other")] {
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(app)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    // An ACTIVE grant for w2v on T1 (the successful grantee), an ACTIVE one for `other` (a
    // grantee that does NOT issue demo), and a REVOKED one for commonpractice.
    for (grantee, revoked) in [
        ("w2v", None),
        ("other", None),
        ("commonpractice", Some("2026-03-01T00:00:00Z")),
    ] {
        sqlx::query(
            "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by, revoked_at)
             VALUES ($1, $2, 'test', $3::timestamptz)",
        )
        .bind(T1)
        .bind(grantee)
        .bind(revoked)
        .execute(&db.pool)
        .await
        .expect("grant");
    }
    // Offerings on demo: one carrying both axes, one carrying neither.
    sqlx::query(
        "INSERT INTO offering
             (id, app_id, code, name, byo_mode, markup_pct, surcharge_input_per_1m_usd,
              access_price_usd, access_cadence, created_at, updated_at)
         VALUES ($1, $2, 'gold', 'Gold', 'byo_required', 12.50, 1.25000000,
                 49.00000000, 'monthly', now(), now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0050))
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("gold offering");
    sqlx::query(
        "INSERT INTO offering
             (id, app_id, code, name, byo_mode, created_at, updated_at)
         VALUES ($1, $2, 'free', 'Free', 'local_allowed', now(), now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0051))
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("free offering");
    sqlx::query(
        "INSERT INTO offering
             (id, app_id, code, name, byo_mode, access_price_usd, created_at, updated_at)
         VALUES ($1, $2, 'access-only', 'Access Only', 'platform_only', 10.0, now(), now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0052))
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("access offering");
    sqlx::query(
        "INSERT INTO offering
             (id, app_id, code, name, byo_mode, created_at, updated_at)
         VALUES ($1, $2, 'plan1', 'Plan One', 'byo_required', now(), now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0053))
    .bind(APP_BUNDLED)
    .execute(&db.pool)
    .await
    .expect("bundled offering");
    sqlx::query(
        "INSERT INTO offering
             (id, app_id, code, name, byo_mode, created_at, updated_at)
         VALUES ($1, $2, 'basic', 'Basic', 'byo_required', now(), now())",
    )
    .bind(Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_0054))
    .bind(APP_OTHER)
    .execute(&db.pool)
    .await
    .expect("other offering");

    // The bundle licence that lets fr administer APP_BUNDLED on T1 without issuing it.
    sqlx::query(
        "INSERT INTO bundle (id, key, name, created_at) VALUES ($1, 'b1', 'Bundle One', now())",
    )
    .bind(BUNDLE)
    .execute(&db.pool)
    .await
    .expect("bundle");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at)
         VALUES ($1, $2, 'basic', 'Basic', now())",
    )
    .bind(PLAN)
    .bind(APP_BUNDLED)
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
             (id, subject_id, app_id, plan_id, tenant_id, status, starts_at, bundle_id, created_at)
         VALUES ($1, $2, $3, $4, $5, 'active', now(), $6, now())",
    )
    .bind(BUNDLE_LICENCE)
    .bind(SUBJECT)
    .bind(APP_BUNDLED)
    .bind(PLAN)
    .bind(T1)
    .bind(BUNDLE)
    .execute(&db.pool)
    .await
    .expect("bundle licence");

    db
}

async fn posture(pool: &PgPool, tenant: Uuid, app_key: &str) -> Option<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT t.byo_mode, t.offering_code
           FROM tenant_app_byo_mode t JOIN app a ON a.id = t.app_id
          WHERE t.tenant_id = $1 AND a.key = $2",
    )
    .bind(tenant)
    .bind(app_key)
    .fetch_optional(pool)
    .await
    .expect("posture")
}

// ---------------------------------------------------------------------------------------------
// Grants
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn grants_are_idempotent_listable_and_revocable() {
    let db = seeded().await;
    let fr = user(&["license-admin"], Some("fr"));

    // 201 the first time, 200 on the retry, with the reference's five keys. `platform` is a
    // real tenant_owner with no grant on T1 (the seeded `other` grant is used by the matrix test).
    let (status, first) = send(
        route(&db, fr.clone(), None),
        Method::PUT,
        "/tenants/acme/grants/platform",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(
        first,
        json!({
            "tenant_id": T1.to_string(),
            "grantee_owner_key": "platform",
            "granted_by": "secure-apps/fr-backend",
            "created_at": first["created_at"].clone(),
            "revoked_at": serde_json::Value::Null,
        })
    );
    let (status, second) = send(
        route(&db, fr.clone(), None),
        Method::PUT,
        "/tenants/acme/grants/platform",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(
        second, first,
        "an existing active grant is returned unchanged"
    );

    let (status, listed) = send(
        route(&db, fr.clone(), None),
        Method::GET,
        "/tenants/acme/grants",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let keys: std::collections::BTreeSet<&str> = listed
        .as_array()
        .expect("array")
        .iter()
        .map(|row| row["grantee_owner_key"].as_str().expect("grantee"))
        .collect();
    assert_eq!(
        keys,
        ["w2v", "other", "platform"].into_iter().collect(),
        "only ACTIVE grants"
    );

    // Revoke: 204 with no body, then the same call is 404 grant_not_found.
    let (status, body) = send(
        route(&db, fr.clone(), None),
        Method::DELETE,
        "/tenants/acme/grants/platform",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let revoked: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT revoked_at FROM tenant_app_grant WHERE tenant_id = $1 AND grantee_owner_key = 'platform'",
    )
    .bind(T1)
    .fetch_one(&db.pool)
    .await
    .expect("revoked_at");
    assert!(revoked.is_some(), "revoking stamps the history row");
    let (status, body) = send(
        route(&db, fr.clone(), None),
        Method::DELETE,
        "/tenants/acme/grants/platform",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body,
        json!({"detail": {"error": "grant_not_found", "owner_key": "platform"}})
    );

    // A grantee may not grant onward (owner-only resolution). A grantee reaches a foreign tenant
    // BY ID only; a name is 404 first.
    let w2v = user(&["license-admin"], Some("w2v"));
    let (status, body) = send(
        route(&db, w2v, None),
        Method::PUT,
        &format!("/tenants/{T1}/grants/ghost"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_owner_only");

    // The tenant's own owner needs no grant.
    let (status, body) = send(
        route(&db, fr.clone(), None),
        Method::PUT,
        "/tenants/acme/grants/fr",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body,
        json!({"detail": {
            "error": "grant_to_owner",
            "message": "the tenant's owning app needs no grant on its own tenant",
        }})
    );

    // An unknown grantee owner, and an unknown tenant.
    let (status, body) = send(
        route(&db, fr.clone(), None),
        Method::PUT,
        "/tenants/acme/grants/ghost",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body,
        json!({"detail": {"error": "owner_not_found", "owner_key": "ghost"}})
    );
    let (status, body) = send(route(&db, fr, None), Method::GET, "/tenants/nope/grants").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
}

// ---------------------------------------------------------------------------------------------
// Offerings: listing
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_offering_listing_reports_provenance_and_explicit_nulls() {
    let db = seeded().await;
    // One row with recorded provenance, one direct posture write with UNKNOWN provenance.
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, 'byo_required', 'gold', '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z')",
    )
    .bind(T1)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("posture");
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, 'platform_only', NULL, '2026-01-03T00:00:00Z', '2026-01-04T00:00:00Z')",
    )
    .bind(T1)
    .bind(APP_OTHER)
    .execute(&db.pool)
    .await
    .expect("posture");

    let (status, body) = send(
        route(&db, reader(T1), None),
        Method::GET,
        "/tenants/acme/offerings",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("array");
    assert_eq!(rows.len(), 2, "{body}");
    assert_eq!(rows[0]["app_key"], "other-app");
    assert_eq!(rows[0]["offering_provenance"], "unknown");
    assert_eq!(rows[0]["offering_code"], Value::Null);
    assert_eq!(rows[0]["created_at"], "2026-01-03T00:00:00+00:00");
    assert_eq!(rows[0]["updated_at"], "2026-01-04T00:00:00+00:00");
    assert_eq!(
        rows[0].as_object().expect("row").len(),
        7,
        "app_key, app_name, byo_mode, offering_code, offering_provenance, created_at, updated_at"
    );
    for key in [
        "app_key",
        "app_name",
        "byo_mode",
        "offering_code",
        "offering_provenance",
        "created_at",
        "updated_at",
    ] {
        assert!(rows[0].get(key).is_some(), "missing {key} in {}", rows[0]);
    }
    assert_eq!(rows[1]["app_key"], "demo");
    assert_eq!(rows[1]["offering_code"], "gold");
    assert_eq!(rows[1]["offering_provenance"], "recorded");

    // A foreign tenant read by a non-admin is 404 (existence is not leaked).
    let mut foreign = reader(T2);
    foreign.tenant_id = Some(T2.to_string());
    let (status, body) = send(
        route(&db, foreign, None),
        Method::GET,
        "/tenants/acme/offerings",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

// ---------------------------------------------------------------------------------------------
// Offerings: provisioning and the billing pushes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn provisioning_writes_the_posture_and_pushes_the_exact_markup_and_access_bodies() {
    let db = seeded().await;
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;
    let (status, body) = send(
        route(
            &db,
            user(&["license-admin"], Some("fr")),
            Some(Arc::new(provisioner(&base))),
        ),
        Method::POST,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "tenant_id": T1.to_string(),
            "tenant_name": "acme",
            "app_key": "demo",
            "code": "gold",
            "byo_mode": "byo_required",
            "markup_rule_provisioned": true,
            "access_charge_provisioned": true,
        })
    );
    assert_eq!(
        posture(&db.pool, T1, "demo").await,
        Some(("byo_required".to_owned(), Some("gold".to_owned())))
    );

    let calls = stub.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0].path, "/api/config/markup-rules");
    assert_eq!(calls[0].tenant.as_deref(), Some(T1.to_string().as_str()));
    assert_eq!(calls[0].authorization.as_deref(), Some("Bearer stub-token"));
    assert_eq!(
        calls[0].body,
        json!({
            "tenant_id": T1.to_string(),
            "service_key": "demo",
            "markup_pct": 12.5,
            "surcharge_input_per_1m_usd": 1.25,
        }),
        "only the non-NULL usage fields, in the reference's shape"
    );
    assert_eq!(calls[1].path, "/api/config/access-charges");
    assert_eq!(
        calls[1].body,
        json!({
            "tenant_id": T1.to_string(),
            "service_key": "demo",
            "amount_usd": 49.0,
            "cadence": "monthly",
        })
    );
}

#[tokio::test]
async fn a_rejected_markup_restores_the_posture_and_answers_502() {
    // A prior posture this request OVERWROTE must be written back, not destroyed.
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, 'local_allowed', 'old-code', now(), now())",
    )
    .bind(T1)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("prior posture");

    let stub = Stub::new();
    stub.set_markup_status(403);
    let base = start_stub(stub.clone()).await;
    let (status, body) = send(
        route(
            &db,
            user(&["license-admin"], Some("fr")),
            Some(Arc::new(provisioner(&base))),
        ),
        Method::POST,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "the reference fails the request on a markup refusal: {body}"
    );
    assert_eq!(
        body,
        json!({"detail": "Billing rejected the markup-rule write (HTTP 403); provisioning rolled back"})
    );
    assert_eq!(
        posture(&db.pool, T1, "demo").await,
        Some(("local_allowed".to_owned(), Some("old-code".to_owned()))),
        "the overwritten posture is written back"
    );
    let pushed = stub.calls();
    assert_eq!(
        pushed
            .iter()
            .filter(|call| call.path == "/api/config/markup-rules")
            .count(),
        1,
        "a 4xx is a verdict, not retried"
    );
    assert!(
        !pushed
            .iter()
            .any(|call| call.path == "/api/config/access-charges"),
        "the request stops at the failed markup write"
    );

    // A RETRYABLE 5xx on a row this request CREATED: one bounded retry, then the row is deleted.
    let fresh = seeded().await;
    let retry_stub = Stub::new();
    retry_stub.set_markup_status(500);
    let retry_base = start_stub(retry_stub.clone()).await;
    let (status, body) = send(
        route(
            &fresh,
            user(&["license-admin"], Some("fr")),
            Some(Arc::new(provisioner(&retry_base))),
        ),
        Method::POST,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(
        body,
        json!({"detail": "Billing rejected the markup-rule write (HTTP 500); provisioning rolled back"})
    );
    assert_eq!(
        posture(&fresh.pool, T1, "demo").await,
        None,
        "a row this request created is deleted on rollback"
    );
    let pushed = retry_stub.calls();
    assert_eq!(
        pushed
            .iter()
            .filter(|call| call.path == "/api/config/markup-rules")
            .count(),
        2,
        "one bounded retry for a retryable status"
    );
    assert!(
        !pushed
            .iter()
            .any(|call| call.path == "/api/config/access-charges"),
        "the request stops at the failed markup write"
    );

    // Transport / no client: the `None` arm, with the reference's other exact detail.
    let unreachable_db = seeded().await;
    let (status, body) = send(
        route(&unreachable_db, user(&["license-admin"], Some("fr")), None),
        Method::POST,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(
        body,
        json!({"detail": "Could not reach billing to write the markup rule; provisioning rolled back"})
    );
    assert_eq!(
        posture(&unreachable_db.pool, T1, "demo").await,
        None,
        "a row this request created is deleted on rollback"
    );
}

#[tokio::test]
async fn an_access_refusal_restores_the_prior_posture_and_answers_502() {
    let db = seeded().await;
    // A deliberate prior posture must be restored, not deleted.
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, 'local_allowed', 'old-code', now(), now())",
    )
    .bind(T1)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("prior posture");

    let stub = Stub::new();
    stub.set_access_status(500);
    let base = start_stub(stub.clone()).await;
    let (status, body) = send(
        route(
            &db,
            user(&["license-admin"], Some("fr")),
            Some(Arc::new(provisioner(&base))),
        ),
        Method::POST,
        "/tenants/acme/offerings/demo/access-only",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(
        body,
        json!({"detail": "Billing rejected the access-charge write (HTTP 500); the posture override was rolled back."})
    );
    assert_eq!(
        posture(&db.pool, T1, "demo").await,
        Some(("local_allowed".to_owned(), Some("old-code".to_owned()))),
        "the overwritten posture is written back, not destroyed"
    );
    let access_calls = stub
        .calls()
        .into_iter()
        .filter(|call| call.path == "/api/config/access-charges")
        .count();
    assert_eq!(access_calls, 2, "500 is retryable, then the 502");

    // A markup rule accepted moments earlier SURVIVES the access failure — billing has no
    // markup-rule delete — and the 502 says so instead of claiming a clean rollback.
    let with_markup = seeded().await;
    let stub = Stub::new();
    stub.set_access_status(500);
    let base = start_stub(stub.clone()).await;
    let (status, body) = send(
        route(
            &with_markup,
            user(&["license-admin"], Some("fr")),
            Some(Arc::new(provisioner(&base))),
        ),
        Method::POST,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(
        body,
        json!({"detail": "Billing rejected the access-charge write (HTTP 500); the posture override was rolled back. The markup rule written earlier in this request was NOT removed (billing has no markup-rule delete) — re-provision to overwrite it."})
    );
    assert_eq!(
        posture(&with_markup.pool, T1, "demo").await,
        None,
        "the posture row this request created is still deleted"
    );
    let pushed = stub.calls();
    assert_eq!(
        pushed
            .iter()
            .filter(|call| call.path == "/api/config/markup-rules")
            .count(),
        1,
        "the accepted markup rule is left in place: the 502 tells the truth"
    );
    assert_eq!(
        pushed
            .iter()
            .filter(|call| call.path == "/api/config/access-charges")
            .count(),
        2,
        "500 is retryable, then the 502"
    );

    // The rollback arm for a row this request CREATED is a delete.
    let fresh = seeded().await;
    let unreachable = send(
        route(&fresh, user(&["license-admin"], Some("fr")), None),
        Method::POST,
        "/tenants/acme/offerings/demo/access-only",
    )
    .await;
    assert_eq!(unreachable.0, StatusCode::BAD_GATEWAY, "{}", unreachable.1);
    assert_eq!(
        unreachable.1,
        json!({"detail": "Could not reach billing to write the access charge; the posture override was rolled back."})
    );
    assert_eq!(
        posture(&fresh.pool, T1, "demo").await,
        None,
        "a created row is deleted"
    );
}

// ---------------------------------------------------------------------------------------------
// Offerings: unprovision
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn unprovisioning_clears_the_posture_and_leaves_the_markup_in_place() {
    let db = seeded().await;
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, 'byo_required', 'gold', now(), now())",
    )
    .bind(T1)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("posture");
    let stub = Stub::new();
    let base = start_stub(stub.clone()).await;

    let (status, body) = send(
        route(
            &db,
            user(&["license-manager"], Some("fr")),
            Some(Arc::new(provisioner(&base))),
        ),
        Method::DELETE,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "tenant": "acme",
            "app_key": "demo",
            "code": "gold",
            "posture_cleared": true,
            "note": "billing markup rule left in place (no billing delete endpoint)",
        })
    );
    assert_eq!(posture(&db.pool, T1, "demo").await, None);
    assert!(
        stub.calls().is_empty(),
        "unprovision makes no billing call: the markup rule survives by design"
    );

    let (status, body) = send(
        route(&db, user(&["license-manager"], Some("fr")), None),
        Method::DELETE,
        "/tenants/acme/offerings/demo/gold",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["posture_cleared"], Value::Bool(false), "idempotent");
}

// ---------------------------------------------------------------------------------------------
// Offerings: the per-app authorization matrix
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_per_app_authorization_matrix_is_the_reference_rule() {
    let db = seeded().await;

    // The tenant's owner that ISSUES the app: admitted.
    let (status, body) = send(
        route(&db, user(&["license-admin"], Some("fr")), None),
        Method::POST,
        "/tenants/acme/offerings/demo/free",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The tenant's owner that does NOT issue the app and holds no bundle licence for it: 403.
    let (status, body) = send(
        route(&db, user(&["license-admin"], Some("fr")), None),
        Method::POST,
        "/tenants/acme/offerings/other-app/basic",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_administrable_by_caller");

    // The tenant's owner whose own bundle brought the app onto the tenant: admitted.
    let (status, body) = send(
        route(&db, user(&["license-admin"], Some("fr")), None),
        Method::POST,
        "/tenants/acme/offerings/bundled/plan1",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A grantee that ISSUES the app and holds an ACTIVE grant: admitted. A grantee resolves a
    // foreign tenant BY ID only (a name never resolves outside the caller's own owner).
    let (status, body) = send(
        route(&db, user(&["license-manager"], Some("w2v")), None),
        Method::POST,
        &format!("/tenants/{T1}/offerings/demo/free"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A grantee that issues the app but does NOT hold an active grant: 403 tenant_not_accessible.
    // (The seeded grant for commonpractice is revoked.)
    let (status, body) = send(
        route(
            &db,
            user(&["license-manager"], Some("commonpractice")),
            None,
        ),
        Method::POST,
        &format!("/tenants/{T1}/offerings/demo/free"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "tenant_not_accessible");

    // A grantee that holds a grant but does NOT issue the app: 403 app_not_issuable_by_caller.
    let (status, body) = send(
        route(&db, user(&["license-manager"], Some("other")), None),
        Method::POST,
        &format!("/tenants/{T1}/offerings/demo/free"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["detail"]["error"], "app_not_issuable_by_caller");

    // A NAME resolves only within the caller's own owner: 404, not 403.
    let (status, body) = send(
        route(
            &db,
            user(&["license-manager"], Some("commonpractice")),
            None,
        ),
        Method::POST,
        "/tenants/acme/offerings/demo/free",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));

    // Cross-owner break-glass reaches any tenant.
    let cross = User {
        cross_owner: true,
        ..user(&["license-admin"], Some("other"))
    };
    let (status, body) = send(
        route(&db, cross, None),
        Method::POST,
        "/tenants/acme/offerings/demo/free",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn unknown_apps_and_offerings_are_404() {
    let db = seeded().await;
    let fr = user(&["license-admin"], Some("fr"));

    let (status, body) = send(
        route(&db, fr.clone(), None),
        Method::POST,
        "/tenants/acme/offerings/demo/nope",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Offering not found"}));

    // Unknown app on the caller's own tenant: no write can happen, so the route's own 404.
    let (status, body) = send(
        route(&db, fr.clone(), None),
        Method::POST,
        "/tenants/acme/offerings/nope/gold",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Offering not found"}));

    // Unknown tenant.
    let absent = Uuid::from_u128(0x0000_0000_0000_4500_8000_0000_0000_9999);
    let (status, body) = send(
        route(&db, fr, None),
        Method::POST,
        &format!("/tenants/{absent}/offerings/demo/gold"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, json!({"detail": "Tenant not found"}));
}
