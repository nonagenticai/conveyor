#![forbid(unsafe_code)]
//! The host registry read (SPEC_V2 §10.4.1; migration `0048_host_registry.sql`).
//!
//! `host_registry` is a platform fact: no `tenant_id`, no RLS, the `commercial_price`
//! precedent. This module therefore takes no [`TenantId`] — the caller's scoped transaction
//! still binds the connection, and the table is simply outside tenant data.
//!
//! The inventory read deliberately does **not** select `address`, `agent_user` or `host_key`:
//! an inventory is a fact about what the conveyor may reach, and the credential surface is the
//! dispatcher's, not a display's.

use sqlx::{AssertSqlSafe, PgConnection};

/// Why a host read failed.
#[derive(Debug, thiserror::Error)]
pub enum HostStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// One registered host, as an inventory reports it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HostRecord {
    /// The action-visible name.
    pub name: String,
    /// What the host is for (operator documentation, never a dispatch).
    pub role: String,
    /// `launchctl` or `systemctl`.
    pub mechanism: String,
    /// The units the agent may act on.
    pub units: Vec<String>,
    /// A disabled row is retired, not deleted.
    pub enabled: bool,
}

/// Every registered host, enabled or not, in name order.
pub async fn list_tx(conn: &mut PgConnection) -> Result<Vec<HostRecord>, HostStoreError> {
    const SQL: &str =
        "SELECT name, role, mechanism, units, enabled FROM host_registry ORDER BY name";
    let rows = sqlx::query_as::<_, HostRecord>(AssertSqlSafe(SQL))
        .fetch_all(conn)
        .await?;
    Ok(rows)
}
