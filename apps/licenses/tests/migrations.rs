//! The embedded migrations must produce the Python service's schema, and `migrate --baseline`
//! must stamp an already-schema'd database without touching it.

mod common;

use conveyor_licenses::db;

const TABLES: [&str; 10] = [
    "api_client",
    "app",
    "bundle",
    "license",
    "offering",
    "plan",
    "schema_migrations",
    "subject",
    "tenants",
    "token",
];

async fn applied(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(pool)
        .await
        .expect("read the migration ledger")
}

/// Apply the baseline without the sqlx ledger, then create the Python ledger at the captured head.
/// This is the shape `migrate --baseline` exists for: an existing database cut over in place.
async fn python_shaped(database: &common::TestDb) {
    let sql = include_str!("../migrations/0001_baseline.sql");
    sqlx::raw_sql(sql)
        .execute(&database.pool)
        .await
        .expect("apply the baseline as the Python service would have");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            filename TEXT PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(&database.pool)
    .await
    .expect("create the Python ledger");
    sqlx::query("INSERT INTO schema_migrations (filename) VALUES ($1)")
        .bind(db::PYTHON_HEAD)
        .execute(&database.pool)
        .await
        .expect("record the Python head");
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
    assert_eq!(applied(&database.pool).await, 1);
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let database = common::fresh().await;
    db::migrate(&database.pool).await.expect("first migrate");
    db::migrate(&database.pool).await.expect("second migrate");
    assert_eq!(applied(&database.pool).await, 1);
}

#[tokio::test]
async fn baseline_stamps_a_python_shaped_database_without_ddl() {
    let database = common::fresh().await;
    python_shaped(&database).await;

    let stamped = db::baseline(&database.pool).await.expect("baseline");
    assert_eq!(stamped, 1);
    // A run after the stamp is a no-op: the ledger already carries the baseline.
    let stamped_again = db::baseline(&database.pool).await.expect("second baseline");
    assert_eq!(stamped_again, 0);
    db::migrate(&database.pool)
        .await
        .expect("migrate after baseline");
    assert_eq!(applied(&database.pool).await, 1);
}

#[tokio::test]
async fn baseline_refuses_a_database_with_no_python_ledger() {
    let database = common::fresh().await;
    let error = db::baseline(&database.pool)
        .await
        .expect_err("an empty database has no Python ledger");
    assert!(
        matches!(error, db::BaselineError::NoPythonLedger),
        "{error}"
    );
}

#[tokio::test]
async fn baseline_refuses_a_different_python_head() {
    let database = common::fresh().await;
    python_shaped(&database).await;
    sqlx::query("INSERT INTO schema_migrations (filename) VALUES ('999_not_the_head.sql')")
        .execute(&database.pool)
        .await
        .expect("record a different head");

    let error = db::baseline(&database.pool)
        .await
        .expect_err("a different head must be refused");
    assert!(
        matches!(error, db::BaselineError::NotAtHead { .. }),
        "{error}"
    );
}

#[tokio::test]
async fn baseline_refuses_a_partial_schema() {
    let database = common::fresh().await;
    python_shaped(&database).await;
    sqlx::query("DROP TABLE token CASCADE")
        .execute(&database.pool)
        .await
        .expect("remove a core table");

    let error = db::baseline(&database.pool)
        .await
        .expect_err("a partial schema must be refused");
    assert!(
        matches!(error, db::BaselineError::MissingTable(ref table) if table == "token"),
        "{error}"
    );
}
