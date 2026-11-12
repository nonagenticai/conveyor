//! Transaction-local tenant scope — the single mechanism RLS trusts.
//!
//! [`begin_scoped`] opens a transaction and sets `app.current_tenant` with
//! `set_config(..., true)`, the bind-parameter form of `SET LOCAL`. The third argument is
//! what makes the value die at COMMIT/ROLLBACK: under pgbouncer's transaction pooling a
//! session-level `SET` sticks to the server connection and is inherited by the next client
//! (measured on the estate 2026-09-22). A session `SET` must never appear in this workspace.

use conveyor_domain::TenantId;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

/// Set a GUC for the rest of the transaction. `is_local = true` is the whole point.
pub(crate) async fn set_guc(
    conn: &mut PgConnection,
    name: &str,
    value: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config($1, $2, true)")
        .bind(name)
        .bind(value)
        .execute(conn)
        .await?;
    Ok(())
}

/// Open a transaction scoped to `tenant`; every statement in it is confined by RLS.
pub async fn begin_scoped(
    db: &PgPool,
    tenant: TenantId,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = db.begin().await?;
    set_guc(&mut tx, "app.current_tenant", &tenant.to_string()).await?;
    Ok(tx)
}

/// Open a **read-only** transaction scoped to `tenant`.
///
/// The read-only attribute lives in the transaction, never in a session-level `SET`: under
/// pgbouncer's transaction pooling a session GUC is inherited by the next client (measured
/// 2026-09-22). `SET TRANSACTION READ ONLY` is the first statement, before the tenant GUC.
pub async fn begin_scoped_read_only(
    db: &PgPool,
    tenant: TenantId,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    set_guc(&mut tx, "app.current_tenant", &tenant.to_string()).await?;
    Ok(tx)
}

/// Scope an already-open connection/transaction to `tenant` (for the rest of it).
pub async fn set_tenant(conn: &mut PgConnection, tenant: TenantId) -> Result<(), sqlx::Error> {
    set_guc(conn, "app.current_tenant", &tenant.to_string()).await
}
