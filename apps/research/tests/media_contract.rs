//! The schema and the tenancy model, measured against the capture.
//!
//! `migrations/0001_media_baseline.sql` is a `pg_dump --schema-only` of the reference's database.
//! These tests hold the port to it: the same tables, the same unique constraint that makes
//! re-importing a source safe, and the same absence of row-level security — the reference has
//! none, so the port invents none, and tenant scoping is an application predicate that these
//! tests prove is actually applied.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use conveyor_research::media::{self, Config};

const SEED: &str = include_str!("../contract/media/seed.sql");
const TENANT: &str = "00000000-0000-7000-8000-000000000001";
const OTHER_TENANT: &str = "00000000-0000-7000-8000-000000000002";

fn app(pool: sqlx::PgPool) -> axum::Router {
    let config = Config {
        bind_host: "127.0.0.1".to_string(),
        bind_port: 0,
        enable_docs: false,
        service_name: "media-discovery".to_string(),
        version: "0.1.0".to_string(),
        database_url: None,
        max_connections: 4,
    };
    media::build_app(media::AppState::with_pool(config, pool))
}

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    media::db::migrate(&db.pool)
        .await
        .expect("apply the embedded migrations");
    sqlx::raw_sql(SEED)
        .execute(&db.pool)
        .await
        .expect("load the capture's pre-state");
    db
}

/// The seven tables the capture records.
const CAPTURED_TABLES: [&str; 7] = [
    "collection_items",
    "download_jobs",
    "media_collections",
    "media_items",
    "search_sessions",
    "session_items",
    "tenant_id_purge_audit",
];

#[tokio::test]
async fn migrations_create_the_captured_schema() {
    let db = common::fresh().await;
    media::db::migrate(&db.pool)
        .await
        .expect("apply the embedded migrations");

    let mut tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
          WHERE table_schema = 'public' AND table_name <> '_sqlx_migrations'
          ORDER BY table_name",
    )
    .fetch_all(&db.pool)
    .await
    .expect("read the table inventory");

    tables.sort();
    let mut expected: Vec<String> = CAPTURED_TABLES
        .iter()
        .map(|name| name.to_string())
        .collect();
    expected.sort();
    assert_eq!(tables, expected, "the table set differs from the capture");

    // The unique index that makes `ix_media_items_url` load-bearing for imports.
    let unique: Vec<String> = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes
          WHERE schemaname = 'public' AND indexname = 'ix_media_items_url'",
    )
    .fetch_all(&db.pool)
    .await
    .expect("read the unique index");
    assert_eq!(unique.len(), 1, "ix_media_items_url is missing");
    assert!(
        unique[0].contains("UNIQUE") && unique[0].contains("(tenant_id, url)"),
        "ix_media_items_url must be UNIQUE on (tenant_id, url): {}",
        unique[0]
    );

    // The reference has no row-level security, and the port must not quietly add one: a policy
    // here would make the application predicate untestable and the behaviour different.
    let policies: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_policies")
        .fetch_one(&db.pool)
        .await
        .expect("read pg_policies");
    assert_eq!(policies, 0, "the reference has no row-level security");
}

/// The DoD asks for idempotent migrations: a second `migrate` on a migrated database is a no-op,
/// not a re-run of the DDL.
#[tokio::test]
async fn migrations_are_idempotent() {
    let db = common::fresh().await;
    media::db::migrate(&db.pool)
        .await
        .expect("first migration pass");

    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&db.pool)
        .await
        .expect("read the sqlx ledger");

    media::db::migrate(&db.pool)
        .await
        .expect("second migration pass must be a no-op");

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&db.pool)
        .await
        .expect("read the sqlx ledger");
    assert_eq!(applied, after, "the second pass applied migrations again");
    assert!(applied > 0, "the first pass applied nothing");
}

#[tokio::test]
async fn a_second_tenant_sees_nothing() {
    let db = seeded().await;
    let app = app(db.pool.clone());

    // The row exists — for its own tenant.
    let owned = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/items/10000000-0000-7000-8000-000000000001")
                .header("x-tenant-id", TENANT)
                .body(Body::empty())
                .expect("build"),
        )
        .await
        .expect("the app answers");
    assert_eq!(owned.status(), StatusCode::OK);

    // The same id under another tenant is a 404, not that row.
    let foreign = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/items/10000000-0000-7000-8000-000000000001")
                .header("x-tenant-id", OTHER_TENANT)
                .body(Body::empty())
                .expect("build"),
        )
        .await
        .expect("the app answers");
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);

    // And a listing under the other tenant is empty rather than everyone's rows.
    let listing = app
        .oneshot(
            Request::builder()
                .uri("/api/collections")
                .header("x-tenant-id", OTHER_TENANT)
                .body(Body::empty())
                .expect("build"),
        )
        .await
        .expect("the app answers");
    assert_eq!(listing.status(), StatusCode::OK);
    let bytes = listing
        .into_body()
        .collect()
        .await
        .expect("read the body")
        .to_bytes();
    assert_eq!(&bytes[..], b"[]");
}

#[tokio::test]
async fn no_tenant_header_is_unauthorized() {
    let db = seeded().await;
    let response = app(db.pool.clone())
        .oneshot(
            Request::builder()
                .uri("/api/collections")
                .body(Body::empty())
                .expect("build"),
        )
        .await
        .expect("the app answers");

    // No default tenant exists: an unauthenticated call is refused, never answered from a
    // sentinel's rows.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
