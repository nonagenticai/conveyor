//! Database pool and migrations.

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};

/// Connect with a bounded pool.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Run this service's embedded migrations.
pub async fn migrate(pool: &PgPool, migrator: &Migrator) -> Result<(), MigrateError> {
    migrator.run(pool).await
}
