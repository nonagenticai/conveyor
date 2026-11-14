//! The service's own database: a bounded pool and its embedded migrations.
//!
//! The cutover reuses the Python service's database (same data, no row migration), so there are two
//! entry points:
//!
//! - [`migrate`] applies the embedded baseline to a fresh database;
//! - [`baseline`] does **no DDL**: it refuses unless the Python migration ledger is at the captured
//!   head, then stamps `_sqlx_migrations` so later runs are no-ops. That is the only way an
//!   existing schema and the Rust migrator can agree without re-running CREATE TABLE.
//!
//! The Python service has no migration framework: `models.py` (`create_all`) builds a fresh schema
//! and `scripts/apply_migrations.py` only runs pending files. The live database is therefore the
//! truth, and `migrations/0001_baseline.sql` is its `pg_dump --schema-only`.

use sqlx::migrate::Migrator;
use sqlx::{PgPool, Postgres, Transaction};

/// The migrations compiled into the binary.
pub const MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// The Python migration ledger's head, measured on the live database 2026-11-14 (89 rows; the
/// repository has 90 up-migrations and 62 downgrades — the ledger, not the file list, is the state).
pub const PYTHON_HEAD: &str = "090_conveyor_service_issuer.sql";

/// Connect with a bounded pool.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Apply the embedded migrations. Idempotent: an already-migrated database is a no-op.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(
        "no schema_migrations table: this is not a copy of the Python service's schema — run `migrate`"
    )]
    NoPythonLedger,
    #[error(
        "schema_migrations head is {found:?}, not {expected}: refusing to stamp a schema that is not at the Python head"
    )]
    NotAtHead {
        found: String,
        expected: &'static str,
    },
    #[error(
        "the Python ledger is at the head but `{0}` is missing: refusing to stamp a partial schema"
    )]
    MissingTable(String),
}

/// Stamp every embedded migration as applied, without running DDL.
///
/// Returns how many rows were new (0 when the database was already stamped).
pub async fn baseline(pool: &PgPool) -> Result<u64, BaselineError> {
    let ledger: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.schema_migrations')::text")
            .fetch_one(pool)
            .await?;
    if ledger.is_none() {
        return Err(BaselineError::NoPythonLedger);
    }
    let found: Option<String> = sqlx::query_scalar("SELECT max(filename) FROM schema_migrations")
        .fetch_one(pool)
        .await?;
    let found = found.ok_or(BaselineError::NoPythonLedger)?;
    if found != PYTHON_HEAD {
        return Err(BaselineError::NotAtHead {
            found,
            expected: PYTHON_HEAD,
        });
    }
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM (VALUES ('license'), ('tenants'), ('token')) AS t(name)
          WHERE to_regclass('public.' || name) IS NULL",
    )
    .fetch_all(pool)
    .await?;
    if let Some(table) = missing.first() {
        return Err(BaselineError::MissingTable(table.clone()));
    }

    // sqlx's own ledger table, exactly as the migrator creates it.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;
    let mut stamped = 0;
    for migration in MIGRATOR.iter() {
        let result = sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES ($1, $2, true, $3, 0)
             ON CONFLICT (version) DO NOTHING",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref())
        .execute(&mut *tx)
        .await?;
        stamped += result.rows_affected();
    }
    tx.commit().await?;
    Ok(stamped)
}
