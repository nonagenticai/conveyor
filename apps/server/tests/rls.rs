//! The two-tenant gate (S02 §7) and the dev-login path through the real router.
//!
//! Run with `cargo test -p conveyor-server --test rls -- --nocapture`: the assertions print
//! `current_user` and the counts, so the evidence is in the output, not just a green tick.
//!
//! The isolation half runs as `conveyor_runtime` (NOSUPERUSER NOBYPASSRLS) with a SUPERUSER
//! positive control and an OWNER negative control:
//! - superuser sees both tenants' rows -> the table is not empty, RLS is doing the filtering;
//! - the owner sees zero unscoped -> `FORCE ROW LEVEL SECURITY` applies to the owner too;
//! - the runtime role scoped to A sees none of B's rows, and a forged `tenant_id = B` write
//!   fails `WITH CHECK`.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_api::{AppState, Authenticator, Config, Metrics, app};
use conveyor_auth::AuthConfig;
use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use tower::ServiceExt;
use uuid::Uuid;

fn tenant(uuid: Uuid) -> TenantId {
    TenantId::from_uuid(uuid).expect("a test tenant id is non-nil")
}

async fn seed_event(db: &common::TestDb, tenant_id: Uuid, topic: &str) {
    sqlx::query(
        "INSERT INTO event (tenant_id, topic, type, aggregate_type, aggregate_id)
         VALUES ($1, $2, 'card.moved', 'card', $3)",
    )
    .bind(tenant_id)
    .bind(topic)
    .bind(Uuid::now_v7())
    .execute(&db.super_pool)
    .await
    .expect("seed an event as superuser");
}

async fn current_user(pool: &sqlx::PgPool) -> String {
    sqlx::query_scalar("SELECT current_user")
        .fetch_one(pool)
        .await
        .expect("read current_user")
}

async fn scalar_one(pool: &sqlx::PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

#[tokio::test]
async fn tenant_isolation_is_forced_for_the_runtime_role() {
    let db = common::TestDb::fresh().await;

    // ── seed two tenants, one user and one event each (superuser bypasses RLS) ────────────
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    let user_a = Uuid::now_v7();
    let user_b = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO tenant (id, slug, name) VALUES ($1, 'tenant-a', 'Tenant A'), ($2, 'tenant-b', 'Tenant B')",
    )
    .bind(a)
    .bind(b)
    .execute(&db.super_pool)
    .await
    .expect("seed tenants");
    sqlx::query(
        "INSERT INTO app_user (id, oidc_subject, email, display_name)
         VALUES ($1, 'user:a', 'a@test.invalid', 'A'), ($2, 'user:b', 'b@test.invalid', 'B')",
    )
    .bind(user_a)
    .bind(user_b)
    .execute(&db.super_pool)
    .await
    .expect("seed users");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role)
         VALUES ($1, $2, 'admin'), ($3, $4, 'admin')",
    )
    .bind(a)
    .bind(user_a)
    .bind(b)
    .bind(user_b)
    .execute(&db.super_pool)
    .await
    .expect("seed memberships");
    seed_event(&db, a, "board:0195f48b-d000-7000-8000-000000000001").await;
    seed_event(&db, b, "board:0195f48b-d000-7000-8000-000000000002").await;

    // ── who is running ────────────────────────────────────────────────────────────────────
    let runtime_user = current_user(&db.runtime).await;
    let (runtime_super, runtime_bypass): (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&db.runtime)
            .await
            .expect("runtime role attributes");
    println!(
        "runtime current_user={runtime_user} rolsuper={runtime_super} rolbypassrls={runtime_bypass}"
    );
    assert_eq!(runtime_user, "conveyor_runtime");
    assert!(!runtime_super && !runtime_bypass);

    // ── scoped to A: A's row only, none of B's ────────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant(a))
        .await
        .expect("scope to A");
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM event")
        .fetch_one(&mut *tx)
        .await
        .expect("count events as runtime in A");
    let b_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1")
        .bind(b)
        .fetch_one(&mut *tx)
        .await
        .expect("count B's events as runtime in A");
    println!("runtime scoped to A: event count={total}, B's rows visible={b_rows}");
    assert_eq!(total, 1, "tenant A must see exactly its own event");
    assert_eq!(b_rows, 0, "tenant A must see ZERO of tenant B's rows");

    // ── a forged tenant_id write is refused by WITH CHECK ────────────────────────────────
    let forged = sqlx::query(
        "INSERT INTO event (tenant_id, topic, type, aggregate_type, aggregate_id)
         VALUES ($1, 'board:forged', 'card.moved', 'card', $2)",
    )
    .bind(b)
    .bind(Uuid::now_v7())
    .execute(&mut *tx)
    .await;
    let error = forged.expect_err("a forged tenant_id insert must fail");
    match &error {
        sqlx::Error::Database(database) => {
            println!(
                "forged insert refused: sqlstate={:?} message={}",
                database.code(),
                database.message()
            );
            assert_eq!(
                database.code().as_deref(),
                Some("42501"),
                "expected an RLS violation"
            );
            assert!(
                database.message().contains("row-level security"),
                "expected a row-level security message, got {}",
                database.message()
            );
        }
        other => panic!("expected an RLS database error, got {other:?}"),
    }
    tx.rollback().await.expect("rollback the forged write");

    // ── the SUPERUSER positive control: both rows exist, so RLS is the filter ────────────
    let super_user = current_user(&db.super_pool).await;
    let super_count = scalar_one(&db.super_pool, "SELECT count(*) FROM event").await;
    println!("superuser current_user={super_user}: event count={super_count}");
    assert_eq!(super_user, "postgres");
    assert_eq!(
        super_count, 2,
        "the positive control must see both rows; otherwise the table is simply empty"
    );

    // ── the OWNER negative control: FORCE, not an owner bypass ───────────────────────────
    let owner_user = current_user(&db.owner).await;
    let owner_unscoped = scalar_one(&db.owner, "SELECT count(*) FROM event").await;
    println!("owner current_user={owner_user}: unscoped event count={owner_unscoped}");
    assert_eq!(owner_user, "conveyor_migrate");
    assert_eq!(
        owner_unscoped, 0,
        "FORCE ROW LEVEL SECURITY must also bind the table owner"
    );
    let mut tx = begin_scoped(&db.owner, tenant(a))
        .await
        .expect("scope owner to A");
    let owner_scoped_a: i64 = sqlx::query_scalar("SELECT count(*) FROM event")
        .fetch_one(&mut *tx)
        .await
        .expect("owner scoped to A");
    assert_eq!(owner_scoped_a, 1, "a scoped owner sees A's row");
    tx.commit().await.expect("commit the owner read");

    // ── audit_event is append-only by GRANT (binding decision 5) ─────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant(a))
        .await
        .expect("scope to A");
    let update = sqlx::query("UPDATE audit_event SET status = 'ok'")
        .execute(&mut *tx)
        .await;
    let error = update.expect_err("the runtime role must not UPDATE audit_event");
    match &error {
        sqlx::Error::Database(database) => {
            println!(
                "audit UPDATE refused: sqlstate={:?} message={}",
                database.code(),
                database.message()
            );
            assert_eq!(database.code().as_deref(), Some("42501"));
        }
        other => panic!("expected a permission error, got {other:?}"),
    }
    tx.rollback().await.expect("rollback");

    // ── the login path: subject -> exactly one membership, through RLS ───────────────────
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed the dev tenants through the real seeder");
    let alice = conveyor_store::resolve_identity(&db.runtime, "dev:alice")
        .await
        .expect("resolve dev:alice")
        .expect("dev:alice exists");
    let bob = conveyor_store::resolve_identity(&db.runtime, "dev:bob")
        .await
        .expect("resolve dev:bob")
        .expect("dev:bob exists");
    println!(
        "resolve_identity: alice -> {:?} {:?}; bob -> {:?} {:?}",
        alice.subject,
        alice.memberships[0].tenant_slug,
        bob.subject,
        bob.memberships[0].tenant_slug
    );
    assert_eq!(alice.memberships.len(), 1);
    assert_eq!(alice.memberships[0].tenant_slug, "dev-a");
    assert_eq!(alice.memberships[0].role, "admin");
    assert_eq!(bob.memberships[0].tenant_slug, "dev-b");
    assert!(
        conveyor_store::resolve_identity(&db.runtime, "dev:nobody")
            .await
            .expect("resolve an unknown subject")
            .is_none(),
        "an unknown subject resolves to no identity"
    );
}

// ── the real router, dev auth, `/api/me` ─────────────────────────────────────────────────

async fn dev_state(db: &common::TestDb) -> AppState {
    let runtime_url = db.runtime_url.clone();
    let config = Config::from_lookup(|key| match key {
        "CONVEYOR_DEV_AUTH" => Some("1".to_owned()),
        "DATABASE_URL" => Some(runtime_url.clone()),
        _ => None,
    })
    .expect("dev configuration");
    // The Platform/billing clients: unconfigured in tests, which is the `None` per service
    // the state already models — the same shape `AppState::connect` builds.
    let (services, _warnings) = conveyor_api::services::Services::from_config(&config);
    AppState {
        pool: db.runtime.clone(),
        listen: conveyor_store::EventListen::connect(&db.runtime_url)
            .await
            .expect("the test LISTEN connection"),
        config: Arc::new(config),
        auth: Arc::new(Authenticator::new(AuthConfig::development()).expect("dev authenticator")),
        services,
        metrics: Metrics::install(),
    }
}

async fn get(
    state: &AppState,
    path: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, String, String) {
    let mut request = Request::builder().uri(path);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app(state.clone())
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("one request");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read the body");
    (
        status,
        content_type,
        String::from_utf8_lossy(&body).into_owned(),
    )
}

#[tokio::test]
async fn dev_auth_resolves_to_the_seeded_tenant_through_the_router() {
    let db = common::TestDb::fresh().await;
    conveyor_server::seed::dev(&db.owner)
        .await
        .expect("seed dev tenants");
    let state = dev_state(&db).await;

    // No credential: 401 problem+json carrying the request id and instance.
    let (status, content_type, body) = get(&state, "/api/me", &[]).await;
    println!("anonymous /api/me -> {status} {content_type} {body}");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(content_type, "application/problem+json");
    let problem: serde_json::Value = serde_json::from_str(&body).expect("problem json");
    assert_eq!(problem["type"], "unauthorized");
    assert_eq!(problem["instance"], "/api/me");
    assert_eq!(
        problem["request_id"].as_str().map(str::len),
        Some(36),
        "the middleware minted a uuid v7 request id"
    );

    // `Bearer dev:alice` -> 200, dev-a.
    let (status, _, body) = get(&state, "/api/me", &[("authorization", "Bearer dev:alice")]).await;
    println!("Bearer dev:alice -> {status} {body}");
    assert_eq!(status, StatusCode::OK);
    let me: serde_json::Value = serde_json::from_str(&body).expect("me json");
    assert_eq!(me["principal"]["subject"], "dev:alice");
    assert_eq!(me["principal"]["email"], "alice@dev.invalid");
    assert_eq!(me["active_tenant"]["slug"], "dev-a");
    assert_eq!(me["active_tenant"]["role"], "admin");
    assert_eq!(me["memberships"].as_array().map(Vec::len), Some(1));

    // The browser transport: the same principal through the cookie.
    let (status, _, body) = get(&state, "/api/me", &[("cookie", "conveyor_dev=bob")]).await;
    println!("cookie conveyor_dev=bob -> {status} {body}");
    assert_eq!(status, StatusCode::OK);
    let me: serde_json::Value = serde_json::from_str(&body).expect("me json");
    assert_eq!(me["principal"]["subject"], "dev:bob");
    assert_eq!(me["active_tenant"]["slug"], "dev-b");

    // An unseeded login is a 401, not an empty session.
    let (status, _, _) = get(&state, "/api/me", &[("authorization", "Bearer dev:carol")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A JWT in dev mode is not retried as a dev token: hard 401.
    let (status, _, _) = get(
        &state,
        "/api/me",
        &[("authorization", "Bearer eyJhbGciOiJSUzI1NiJ9.e30.c2ln")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Liveness needs no credential and no database.
    let (status, _, body) = get(&state, "/health/live", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"ok\""));

    // The scrape endpoint answers and reports the database up.
    let (status, content_type, body) = get(&state, "/metrics", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain"));
    assert!(
        body.contains("conveyor_db_up"),
        "the scrape must expose the database gauge: {body}"
    );
}
