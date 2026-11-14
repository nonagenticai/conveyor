//! Behaviour of the four catalogue OFFERING routes, against a real Postgres 18 built by `common`.
//!
//! Each case pins the reference's observable contract: the exact response JSON (explicit nulls
//! included), the `license-admin`-only role guard (a `license-reader` is refused on every route),
//! the issuer guard on create/delete (and its unknown-app bypass), the upsert's field semantics,
//! the DAL's string-detail 422s, the retry-once-then-409 conflict arm, the list ordering/filter,
//! and the delete's idempotence and its deliberately EMPTY side effects on tenant postures.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::get as route_get;
use conveyor_licenses::auth::{JwtVerifier, UnavailableJwks, User};
use conveyor_licenses::config::Config;
use conveyor_licenses::db;
use conveyor_licenses::router::catalogue_offerings::{
    create_offering, delete_offering, get_offering, list_offerings,
};
use conveyor_licenses::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT_A: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_4101);
const APP_ALPHA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_4301);
const APP_ZETA: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_4302);
const OFFER_EARLY: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_4401);
const OFFER_MIDDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_4402);
const OFFER_LATE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_4403);

const EARLY: &str = "2026-01-01T03:04:05+00:00";
const CREATED: &str = "2026-01-02T03:04:05+00:00";
const LATE: &str = "2026-01-03T03:04:05+00:00";

fn config() -> Config {
    Config {
        port: 0,
        environment: "test".to_owned(),
        version: "0.0.0".to_owned(),
        trusted_issuers: Vec::new(),
        introspect_require_auth: false,
        service_actor_id: None,
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    AppState::new(
        pool,
        config(),
        JwtVerifier::new(Vec::new(), Arc::new(UnavailableJwks)),
    )
}

fn user(roles: &[&str], sub: &str, owner: Option<&str>) -> User {
    User {
        sub: sub.to_owned(),
        realm_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

fn admin(owner: &str) -> User {
    user(&["license-admin"], "admin", Some(owner))
}

fn reader() -> User {
    user(&["license-reader"], "reader", Some("owner-a"))
}

fn cross_owner() -> User {
    User {
        cross_owner: true,
        ..admin("owner-c")
    }
}

/// The four routes alone, with the caller the middleware would have built.
fn route(state: AppState, caller: User) -> Router {
    Router::new()
        .route(
            "/offerings",
            route_get(list_offerings).post(create_offering),
        )
        .route(
            "/offerings/{app_key}/{code}",
            route_get(get_offering).delete(delete_offering),
        )
        .layer(axum::Extension(caller))
        .with_state(state)
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

async fn get(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::GET, path, None).await
}

async fn post(app: Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, Method::POST, path, Some(body)).await
}

async fn delete(app: Router, path: &str) -> (StatusCode, Value) {
    send(app, Method::DELETE, path, None).await
}

fn at(text: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(text)
        .expect("instant")
        .with_timezone(&chrono::Utc)
}

/// Two owners, two apps (alpha issued by owner-a, zeta by owner-b) and one tenant.
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
    for (id, key, name, owner) in [
        (APP_ALPHA, "alpha", "Alpha", "owner-a"),
        (APP_ZETA, "zeta", "Zeta", "owner-b"),
    ] {
        sqlx::query(
            "INSERT INTO app (id, key, name, description, revoked, audience, created_at)
             VALUES ($1, $2, $3, NULL, false, 'platform', $4)",
        )
        .bind(id)
        .bind(key)
        .bind(name)
        .bind(at(CREATED))
        .execute(&db.pool)
        .await
        .expect("app");
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(id)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-a', now(), 'owner-a', 'local_allowed')",
    )
    .bind(TENANT_A)
    .execute(&db.pool)
    .await
    .expect("tenant");
    db
}

/// One offering row with fixed timestamps, so a read can assert the whole JSON.
struct OfferingSeed<'a> {
    id: Uuid,
    app: Uuid,
    code: &'a str,
    name: &'a str,
    created_at: &'a str,
    byo_mode: &'a str,
    markup_pct: Option<f64>,
    access_price_usd: Option<f64>,
    included_allowance_usd: Option<f64>,
    access_cadence: &'a str,
    notes: Option<&'a str>,
}

async fn seed(db: &common::TestDb, seed: OfferingSeed<'_>) {
    sqlx::query(
        "INSERT INTO offering
             (id, app_id, code, name, byo_mode, markup_pct, access_price_usd,
              included_allowance_usd, access_cadence, notes, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6::float8, $7::float8, $8::float8, $9, $10, $11, $11)",
    )
    .bind(seed.id)
    .bind(seed.app)
    .bind(seed.code)
    .bind(seed.name)
    .bind(seed.byo_mode)
    .bind(seed.markup_pct)
    .bind(seed.access_price_usd)
    .bind(seed.included_allowance_usd)
    .bind(seed.access_cadence)
    .bind(seed.notes)
    .bind(at(seed.created_at))
    .execute(&db.pool)
    .await
    .expect("offering");
}

async fn offering_count(db: &common::TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM offering")
        .fetch_one(&db.pool)
        .await
        .expect("count")
}

/// Every `OfferingResponse` key, and nothing else, plus the two timestamp invariants.
fn assert_offering_shape(value: &Value) {
    let object = value.as_object().expect("one offering object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "access_cadence",
            "access_price_usd",
            "app_id",
            "app_key",
            "byo_mode",
            "code",
            "created_at",
            "id",
            "included_allowance_usd",
            "markup_pct",
            "name",
            "notes",
            "surcharge_cache_creation_per_1m_usd",
            "surcharge_cache_read_per_1m_usd",
            "surcharge_input_per_1m_usd",
            "surcharge_output_per_1m_usd",
            "surcharge_per_request_usd",
            "updated_at",
        ]
    );
    Uuid::try_parse(object["id"].as_str().expect("id string")).expect("id is a UUID");
    Uuid::try_parse(object["app_id"].as_str().expect("app_id string")).expect("app_id is a UUID");
    for field in ["created_at", "updated_at"] {
        let stamp = object[field].as_str().expect("timestamp string");
        assert!(stamp.ends_with("+00:00"), "{field}: {stamp}");
    }
}

const FULL_CREATE: fn() -> Value = || {
    json!({
        "app_key": "alpha",
        "code": "free-high-markup",
        "name": "Free high markup",
        "byo_mode": "local_allowed",
        "markup_pct": 12.5,
        "surcharge_input_per_1m_usd": 0.4,
        "surcharge_output_per_1m_usd": 1.6,
        "surcharge_cache_creation_per_1m_usd": 0.5,
        "surcharge_cache_read_per_1m_usd": 0.04,
        "surcharge_per_request_usd": 0.002,
        "access_price_usd": 99.0,
        "access_cadence": "one_time",
        "included_allowance_usd": 5.0,
        "notes": "seed"
    })
};

#[tokio::test]
async fn create_upserts_and_returns_the_exact_response_shape() {
    let db = seeded().await;
    let app = route(app_state(db.pool.clone()), admin("owner-a"));

    let (status, first) = post(app.clone(), "/offerings", FULL_CREATE()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_offering_shape(&first);
    assert_eq!(first["app_id"], APP_ALPHA.to_string());
    assert_eq!(first["app_key"], "alpha");
    assert_eq!(first["code"], "free-high-markup");
    assert_eq!(first["name"], "Free high markup");
    assert_eq!(first["byo_mode"], "local_allowed");
    assert_eq!(first["markup_pct"], json!(12.5));
    assert_eq!(first["surcharge_input_per_1m_usd"], json!(0.4));
    assert_eq!(first["surcharge_output_per_1m_usd"], json!(1.6));
    assert_eq!(first["surcharge_cache_creation_per_1m_usd"], json!(0.5));
    assert_eq!(first["surcharge_cache_read_per_1m_usd"], json!(0.04));
    assert_eq!(first["surcharge_per_request_usd"], json!(0.002));
    assert_eq!(first["access_price_usd"], json!(99.0));
    assert_eq!(first["access_cadence"], "one_time");
    assert_eq!(first["included_allowance_usd"], json!(5.0));
    assert_eq!(first["notes"], "seed");

    // The upsert: name/byo_mode/cadence/per-request are written, an omitted usage field KEEPS its
    // value, and the two access money fields plus notes are CLEARED (the reference's unconditional
    // setattr on those four).
    let (status, second) = post(
        app.clone(),
        "/offerings",
        json!({
            "app_key": "alpha",
            "code": "free-high-markup",
            "name": "Renamed",
            "byo_mode": "byo_required",
            "surcharge_per_request_usd": 0.5,
            "access_cadence": "monthly",
            "notes": null
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["id"], first["id"], "the upsert keeps the row");
    assert_eq!(second["created_at"], first["created_at"]);
    assert_eq!(second["name"], "Renamed");
    assert_eq!(second["byo_mode"], "byo_required");
    assert_eq!(second["access_cadence"], "monthly");
    assert_eq!(second["markup_pct"], json!(12.5), "omitted usage is kept");
    assert_eq!(second["surcharge_input_per_1m_usd"], json!(0.4));
    assert_eq!(second["surcharge_per_request_usd"], json!(0.5));
    assert_eq!(second["access_price_usd"], Value::Null);
    assert_eq!(second["included_allowance_usd"], Value::Null);
    assert_eq!(second["notes"], Value::Null);
    assert_eq!(offering_count(&db).await, 1);
}

#[tokio::test]
async fn create_requires_license_admin_and_app_ownership() {
    let db = seeded().await;

    let (status, body) = post(
        route(app_state(db.pool.clone()), reader()),
        "/offerings",
        FULL_CREATE(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));
    assert_eq!(offering_count(&db).await, 0, "nothing was written");

    // owner-b issues zeta, not alpha.
    let (status, body) = post(
        route(app_state(db.pool.clone()), admin("owner-b")),
        "/offerings",
        FULL_CREATE(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "app_not_administrable_by_caller",
            "message": "owner 'owner-b' does not issue this app and may not administer it"
        }})
    );
    assert_eq!(offering_count(&db).await, 0);

    // cross_owner bypasses the issuer rule, audited.
    let (status, body) = post(
        app_state_route(&db, cross_owner()),
        "/offerings",
        FULL_CREATE(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(offering_count(&db).await, 1);
}

fn app_state_route(db: &common::TestDb, caller: User) -> Router {
    route(app_state(db.pool.clone()), caller)
}

#[tokio::test]
async fn create_validates_like_pydantic_and_the_dal() {
    let db = seeded().await;

    // The DAL checks byo_mode BEFORE resolving the app: an unknown app is 422 here, not 404.
    let mut body = FULL_CREATE();
    body["app_key"] = json!("ghost");
    body["byo_mode"] = json!("chaos");
    let (status, payload) = post(app_state_route(&db, admin("owner-a")), "/offerings", body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload,
        json!({"detail": "invalid byo_mode: 'chaos'"}),
        "a DAL-authored refusal is a STRING detail"
    );

    let mut body = FULL_CREATE();
    body["markup_pct"] = json!(-1.5);
    let (status, payload) = post(app_state_route(&db, admin("owner-a")), "/offerings", body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload,
        json!({"detail": [{
            "type": "greater_than_equal",
            "loc": ["body", "markup_pct"],
            "msg": "Input should be greater than or equal to 0",
            "input": -1.5,
            "ctx": {"ge": 0.0}
        }]})
    );

    let mut body = FULL_CREATE();
    body["access_cadence"] = json!("weekly");
    let (status, payload) = post(app_state_route(&db, admin("owner-a")), "/offerings", body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload,
        json!({"detail": [{
            "type": "literal_error",
            "loc": ["body", "access_cadence"],
            "msg": "Input should be 'monthly' or 'one_time'",
            "input": "weekly",
            "ctx": {"expected": "'monthly' or 'one_time'"}
        }]})
    );

    let mut body = FULL_CREATE();
    body["code"] = json!("x".repeat(65));
    let (status, payload) = post(app_state_route(&db, admin("owner-a")), "/offerings", body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload,
        json!({"detail": [{
            "type": "string_too_long",
            "loc": ["body", "code"],
            "msg": "String should have at most 64 characters",
            "input": "x".repeat(65),
            "ctx": {"max_length": 64}
        }]})
    );

    let mut body = FULL_CREATE();
    body.as_object_mut().expect("object").remove("name");
    let (status, payload) = post(
        app_state_route(&db, admin("owner-a")),
        "/offerings",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload,
        json!({"detail": [{
            "type": "missing",
            "loc": ["body", "name"],
            "msg": "Field required",
            "input": body
        }]})
    );

    // A valid body on an unknown app is the DAL's 404.
    let mut body = FULL_CREATE();
    body["app_key"] = json!("ghost");
    let (status, payload) = post(app_state_route(&db, admin("owner-a")), "/offerings", body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(payload, json!({"detail": "App not found: ghost"}));
    assert_eq!(offering_count(&db).await, 0);
}

#[tokio::test]
async fn a_second_integrity_error_is_the_concurrent_conflict() {
    let db = seeded().await;
    // Make every INSERT hit the unique index, so the retry fails the same way. The attempt
    // counter is a SEQUENCE, not a table: a table's rows roll back with the failed statement,
    // while `nextval` is non-transactional and so survives it.
    for statement in [
        "CREATE SEQUENCE offering_insert_attempts",
        "CREATE FUNCTION refuse_offering_insert() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             PERFORM nextval('offering_insert_attempts');
             RAISE EXCEPTION 'simulated insert race' USING ERRCODE = '23505';
         END;
         $$",
        "CREATE TRIGGER refuse_offering_insert BEFORE INSERT ON offering
         FOR EACH ROW EXECUTE FUNCTION refuse_offering_insert()",
    ] {
        sqlx::query(statement)
            .execute(&db.pool)
            .await
            .expect("trigger");
    }

    let (status, body) = post(
        app_state_route(&db, admin("owner-a")),
        "/offerings",
        FULL_CREATE(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        json!({"detail": "Concurrent modification of offering 'alpha/free-high-markup'; retry the request"})
    );
    let attempts: i64 = sqlx::query_scalar("SELECT last_value FROM offering_insert_attempts")
        .fetch_one(&db.pool)
        .await
        .expect("attempts");
    assert_eq!(attempts, 2, "one retry, exactly as the reference does");
}

#[tokio::test]
async fn get_returns_the_row_or_404() {
    let db = seeded().await;
    seed(
        &db,
        OfferingSeed {
            id: OFFER_EARLY,
            app: APP_ALPHA,
            code: "free",
            name: "Free",
            created_at: CREATED,
            byo_mode: "local_allowed",
            markup_pct: Some(12.5),
            access_price_usd: None,
            included_allowance_usd: None,
            access_cadence: "monthly",
            notes: Some("operator note"),
        },
    )
    .await;

    let (status, body) = get(
        app_state_route(&db, admin("owner-a")),
        "/offerings/alpha/free",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_offering_shape(&body);
    assert_eq!(
        body,
        json!({
            "id": OFFER_EARLY.to_string(),
            "app_id": APP_ALPHA.to_string(),
            "app_key": "alpha",
            "code": "free",
            "name": "Free",
            "byo_mode": "local_allowed",
            "markup_pct": 12.5,
            "surcharge_input_per_1m_usd": null,
            "surcharge_output_per_1m_usd": null,
            "surcharge_cache_creation_per_1m_usd": null,
            "surcharge_cache_read_per_1m_usd": null,
            "surcharge_per_request_usd": null,
            "access_price_usd": null,
            "access_cadence": "monthly",
            "included_allowance_usd": null,
            "notes": "operator note",
            "created_at": CREATED,
            "updated_at": CREATED
        })
    );

    for path in ["/offerings/alpha/missing", "/offerings/ghost/free"] {
        let (status, body) = get(app_state_route(&db, admin("owner-a")), path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body, json!({"detail": "Offering not found"}));
    }
}

#[tokio::test]
async fn list_is_oldest_first_and_filters_to_one_app() {
    let db = seeded().await;
    for (id, app, code, created_at) in [
        (OFFER_LATE, APP_ALPHA, "late", LATE),
        (OFFER_EARLY, APP_ZETA, "early", EARLY),
        (OFFER_MIDDLE, APP_ALPHA, "middle", CREATED),
    ] {
        seed(
            &db,
            OfferingSeed {
                id,
                app,
                code,
                name: code,
                created_at,
                byo_mode: "platform_only",
                markup_pct: None,
                access_price_usd: None,
                included_allowance_usd: None,
                access_cadence: "monthly",
                notes: None,
            },
        )
        .await;
    }
    let app = app_state_route(&db, admin("owner-a"));

    let (status, body) = get(app.clone(), "/offerings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let codes: Vec<&str> = body["offerings"]
        .as_array()
        .expect("offerings array")
        .iter()
        .map(|row| row["code"].as_str().expect("code"))
        .collect();
    assert_eq!(codes, ["early", "middle", "late"], "created_at ascending");

    let (status, body) = get(app.clone(), "/offerings?app=alpha").await;
    assert_eq!(status, StatusCode::OK);
    let codes: Vec<&str> = body["offerings"]
        .as_array()
        .expect("offerings array")
        .iter()
        .map(|row| row["code"].as_str().expect("code"))
        .collect();
    assert_eq!(codes, ["middle", "late"]);

    for query in ["/offerings?app=ghost", "/offerings?app="] {
        let (status, body) = get(app.clone(), query).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        assert_eq!(body, json!({"offerings": []}), "{query}");
    }
}

#[tokio::test]
async fn delete_is_idempotent_and_leaves_tenant_postures_alone() {
    let db = seeded().await;
    for (id, app, code) in [
        (OFFER_EARLY, APP_ALPHA, "free"),
        (OFFER_MIDDLE, APP_ALPHA, "keep"),
    ] {
        seed(
            &db,
            OfferingSeed {
                id,
                app,
                code,
                name: code,
                created_at: CREATED,
                byo_mode: "local_allowed",
                markup_pct: None,
                access_price_usd: None,
                included_allowance_usd: None,
                access_cadence: "monthly",
                notes: None,
            },
        )
        .await;
    }
    // A tenant posture that names the offering by code. There is no FK, so the delete must neither
    // refuse nor clear it.
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, created_at, updated_at, offering_code)
         VALUES ($1, $2, 'local_allowed', now(), now(), 'free')",
    )
    .bind(TENANT_A)
    .bind(APP_ALPHA)
    .execute(&db.pool)
    .await
    .expect("posture");
    let app = app_state_route(&db, admin("owner-a"));

    let (status, body) = delete(app.clone(), "/offerings/alpha/free").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"app_key": "alpha", "code": "free", "deleted": true})
    );

    let (status, body) = delete(app.clone(), "/offerings/alpha/free").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"app_key": "alpha", "code": "free", "deleted": false}),
        "idempotent, never 404"
    );

    let (status, body) = delete(app.clone(), "/offerings/ghost/free").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"app_key": "ghost", "code": "free", "deleted": false})
    );

    // The other offering is untouched, and so is the tenant posture naming the deleted code.
    let (status, _) = get(app.clone(), "/offerings/alpha/keep").await;
    assert_eq!(status, StatusCode::OK);
    let posture: Option<String> =
        sqlx::query_scalar("SELECT offering_code FROM tenant_app_byo_mode WHERE tenant_id = $1")
            .bind(TENANT_A)
            .fetch_optional(&db.pool)
            .await
            .expect("posture");
    assert_eq!(posture.as_deref(), Some("free"));
}

#[tokio::test]
async fn delete_requires_license_admin_and_app_ownership() {
    let db = seeded().await;
    seed(
        &db,
        OfferingSeed {
            id: OFFER_EARLY,
            app: APP_ALPHA,
            code: "free",
            name: "Free",
            created_at: CREATED,
            byo_mode: "local_allowed",
            markup_pct: None,
            access_price_usd: None,
            included_allowance_usd: None,
            access_cadence: "monthly",
            notes: None,
        },
    )
    .await;

    let (status, body) = delete(app_state_route(&db, reader()), "/offerings/alpha/free").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));

    let (status, body) = delete(
        app_state_route(&db, admin("owner-b")),
        "/offerings/alpha/free",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"detail": {
            "error": "app_not_administrable_by_caller",
            "message": "owner 'owner-b' does not issue this app and may not administer it"
        }})
    );

    let (status, body) = delete(app_state_route(&db, cross_owner()), "/offerings/alpha/free").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    assert_eq!(offering_count(&db).await, 0);
}

#[tokio::test]
async fn both_reads_are_license_admin_only() {
    let db = seeded().await;
    let app = app_state_route(&db, reader());
    for path in ["/offerings", "/offerings/alpha/free"] {
        let (status, body) = get(app.clone(), path).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(body, json!({"detail": "Role 'license-admin' required"}));
    }
}

#[tokio::test]
async fn a_missing_pool_is_each_routes_static_500() {
    let app = route(conveyor_licenses::router::test_state(), admin("owner-a"));

    let (status, body) = post(app.clone(), "/offerings", FULL_CREATE()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"detail": "Failed to create offering"}));

    let (status, body) = get(app.clone(), "/offerings").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"detail": "Failed to list offerings"}));

    let (status, body) = get(app.clone(), "/offerings/alpha/free").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"detail": "Failed to get offering"}));

    let (status, body) = delete(app, "/offerings/alpha/free").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"detail": "Failed to delete offering"}));
}
