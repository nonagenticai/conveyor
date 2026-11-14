//! The embedded migrations must produce the Python service's schema, and `migrate --baseline`
//! must stamp an already-schema'd database without touching it.

mod common;

use conveyor_inference_router::db;

const TABLES: [&str; 8] = [
    "canary_monitor_lease",
    "inference_call",
    "policy_epoch",
    "prompt_canary_routing",
    "replay_invocation_log",
    "tenant_inference_policy",
    "tenant_inference_policy_version",
    "tenant_policy_audit",
];

async fn applied(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(pool)
        .await
        .expect("read the migration ledger")
}

#[tokio::test]
async fn migrate_creates_the_python_schema() {
    let database = common::fresh().await;
    db::migrate(&database.pool).await.expect("migrate");

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
          WHERE table_schema = 'public' ORDER BY 1",
    )
    .fetch_all(&database.pool)
    .await
    .expect("read the table list");

    for table in TABLES {
        assert!(
            tables.iter().any(|found| found == table),
            "missing {table}: {tables:?}"
        );
    }
    assert_eq!(applied(&database.pool).await, 16);
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let database = common::fresh().await;
    db::migrate(&database.pool).await.expect("first migrate");
    db::migrate(&database.pool).await.expect("second migrate");
    assert_eq!(applied(&database.pool).await, 16);
}

#[tokio::test]
async fn the_ledger_keeps_its_constraints() {
    let database = common::fresh().await;
    db::migrate(&database.pool).await.expect("migrate");

    // The stream-status check is NOT VALID but still binds new rows.
    let rejected = sqlx::query(
        "INSERT INTO inference_call (requested_model, request_sha256, latency_ms, status, stream_status)
         VALUES ('m', repeat('a', 64), 1, 'ok', 'bogus')",
    )
    .execute(&database.pool)
    .await;
    assert!(
        rejected.is_err(),
        "an unknown stream_status must be refused"
    );

    // The request-id index is unique where the id is present.
    let duplicate = async {
        for _ in 0..2 {
            sqlx::query(
                "INSERT INTO inference_call (requested_model, request_sha256, latency_ms, status, request_id)
                 VALUES ('m', repeat('b', 64), 1, 'ok', '018f3d5e-0000-7000-8000-000000000000')",
            )
            .execute(&database.pool)
            .await?;
        }
        Ok::<(), sqlx::Error>(())
    }
    .await;
    assert!(duplicate.is_err(), "request_id must be unique");
}

#[tokio::test]
async fn baseline_refuses_a_database_without_alembic_version() {
    let database = common::fresh().await;
    db::migrate(&database.pool).await.expect("migrate");
    let error = db::baseline(&database.pool)
        .await
        .expect_err("a Rust-schema database is not a Python-schema database");
    assert!(
        matches!(error, db::BaselineError::NoAlembicVersion),
        "{error}"
    );
}

#[tokio::test]
async fn baseline_refuses_a_different_alembic_head() {
    let database = common::fresh().await;
    sqlx::query("CREATE TABLE alembic_version (version_num varchar(32) PRIMARY KEY)")
        .execute(&database.pool)
        .await
        .expect("alembic_version");
    sqlx::query("INSERT INTO alembic_version (version_num) VALUES ('0012_call_traffic_source')")
        .execute(&database.pool)
        .await
        .expect("insert the head");

    let error = db::baseline(&database.pool)
        .await
        .expect_err("a schema behind the translated head must be refused");
    assert!(
        matches!(error, db::BaselineError::NotAtHead { .. }),
        "{error}"
    );
}

#[tokio::test]
async fn baseline_stamps_the_live_schema_without_running_ddl() {
    let database = common::fresh().await;
    sqlx::query("CREATE TABLE alembic_version (version_num varchar(32) PRIMARY KEY)")
        .execute(&database.pool)
        .await
        .expect("alembic_version");
    sqlx::query("INSERT INTO alembic_version (version_num) VALUES ('0016_inference_call_policy')")
        .execute(&database.pool)
        .await
        .expect("insert the head");

    let stamped = db::baseline(&database.pool).await.expect("baseline");
    assert_eq!(stamped, 16);

    // The stamp is what makes the cutover safe: migrate must now be a no-op that neither
    // re-runs DDL nor fails on tables it did not create.
    db::migrate(&database.pool)
        .await
        .expect("a stamped database is already migrated");
    assert_eq!(applied(&database.pool).await, 16);

    // Baseline ran no DDL: the app tables are still absent (they belong to the live database).
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
          WHERE table_schema = 'public' ORDER BY 1",
    )
    .fetch_all(&database.pool)
    .await
    .expect("read the table list");
    assert!(!tables.iter().any(|table| table == "inference_call"));
}
