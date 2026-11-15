//! The media-discovery database: a bounded pool, the embedded migrations and the row types.
//!
//! One service = one database = one role. The schema is `migrations/0001_media_baseline.sql`, a
//! `pg_dump --schema-only` of the reference's own database squashed from its three alembic
//! revisions — see that file's header for what is reproduced and what the reference does not have
//! (no row-level security, no triggers).
//!
//! Tenant scoping is **application-level**, exactly as in the reference: every query names
//! `tenant_id` in its predicate. There is no policy behind it, and this module does not invent one.

pub mod collections;
pub mod items;
pub mod sessions;

use sqlx::migrate::Migrator;
use sqlx::{PgPool, Postgres, Transaction};

/// The migrations compiled into the binary.
///
/// `sqlx::migrate!` resolves relative to the **crate root** (`apps/research/`), which is where
/// `migrations/` lives — the same shape `apps/licenses/src/db/mod.rs` uses.
pub const MIGRATOR: Migrator = sqlx::migrate!("./migrations");

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

/// Begin a transaction. The callers that need one own their commit; everything else runs
/// statement-at-a-time, as the reference does.
pub async fn begin(pool: &PgPool) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    pool.begin().await
}

/// Render a timestamp the way the reference's JSON does: RFC 3339 with `Z` for UTC, and a
/// fractional part exactly when the value has one (pydantic omits it for whole seconds).
pub fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

/// A `jsonb` column as the reference serialises it.
///
/// `NULL` and SQL `NULL` and JSON `null` are three different things in the reference's answers
/// (`quality_components` is `null` until the row is scored), so the conversions here never
/// collapse them.
pub fn json(value: sqlx::types::Json<serde_json::Value>) -> serde_json::Value {
    value.0
}
