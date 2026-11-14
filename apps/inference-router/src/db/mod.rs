//! The service's own database: a bounded pool and its embedded migrations.
//!
//! The cutover reuses the Python service's database (same data, no migration of rows), so there
//! are two entry points:
//!
//! - [`migrate`] applies the embedded DDL to a fresh database;
//! - [`baseline`] does **no DDL**: it refuses unless `alembic_version` is the translated head, then
//!   stamps `_sqlx_migrations` so later runs are no-ops. That is the only way an existing schema
//!   and the Rust migrator can agree without re-running CREATE TABLE.

use sqlx::migrate::Migrator;
use sqlx::{PgPool, Postgres, Transaction};

/// The migrations compiled into the binary (one SQL file per alembic revision).
pub const MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// The alembic revision the translated migrations end at (measured live, 2026-11-13).
pub const ALEMBIC_HEAD: &str = "0016_inference_call_policy";

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
        "no alembic_version table: this is not a copy of the Python service's schema — run `migrate`"
    )]
    NoAlembicVersion,
    #[error(
        "alembic_version is {found}, not {expected}: refusing to stamp a schema that is not at the translated head"
    )]
    NotAtHead {
        found: String,
        expected: &'static str,
    },
}

/// Stamp every embedded migration as applied, without running DDL.
///
/// Returns how many rows were new (0 when the database was already stamped).
pub async fn baseline(pool: &PgPool) -> Result<u64, BaselineError> {
    let table: Option<String> = sqlx::query_scalar("SELECT to_regclass('alembic_version')::text")
        .fetch_one(pool)
        .await?;
    if table.is_none() {
        return Err(BaselineError::NoAlembicVersion);
    }
    let found: Option<String> = sqlx::query_scalar("SELECT version_num FROM alembic_version")
        .fetch_optional(pool)
        .await?;
    let found = found.ok_or(BaselineError::NoAlembicVersion)?;
    if found != ALEMBIC_HEAD {
        return Err(BaselineError::NotAtHead {
            found,
            expected: ALEMBIC_HEAD,
        });
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
