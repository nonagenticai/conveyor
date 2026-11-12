//! The runtime pool: ten connections, a five-second acquire timeout.
//!
//! Ten is a default, not a measurement for a process that also runs the worker and the
//! scheduler (S02b/S14 size that). Five seconds is what turns a pooler outage into a
//! `503`/error instead of a request that hangs until the client gives up — a handler that
//! blocks forever on `acquire` looks like a slow request, not a broken pool.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Connections in a process's runtime pool.
pub const MAX_CONNECTIONS: u32 = 10;

/// How long an `acquire` waits before failing.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Connect now: fails fast when the database (or pooler) is unreachable.
pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect(url)
        .await
}

/// Connect lazily. Used by HTTP tests whose routes must not touch a database; a lazy pool
/// succeeds against a dead host and only fails when a query actually runs.
pub fn connect_lazy(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_lazy(url)
}

/// One round-trip to the database: the single "did Postgres answer" statement.
///
/// `/health/ready` and the `conveyor_db_up` scrape gauge both call this, so the service has
/// exactly one definition of readiness rather than two copies of `SELECT 1` drifting apart.
pub async fn ping(db: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(db).await?;
    Ok(())
}
