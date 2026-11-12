//! The estate-wide stop: one lazily created row per tenant, primary key `tenant_id`.
//!
//! `andon_state` has no surrogate id and no read action to learn a version from, which is
//! why `andon.engage`/`.release` are status-guarded rather than `If-Match` (CONTRACT §3).
//! The row is created on the first engage, so a tenant that has never pulled the cord has no
//! row and `get_tx` returns `None`.
//!
//! `engaged_by` is resolved against `app_user` before the upsert: `app_user` is global, so
//! the FK alone would accept an id that never existed (CONTRACT §2).

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use sqlx::PgConnection;
use sqlx::PgPool;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};
use crate::scope::begin_scoped;
use crate::version_of;

/// Why an andon write failed.
#[derive(Debug, thiserror::Error)]
pub enum AndonStoreError {
    /// `engaged_by` names an `app_user` that does not exist.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The andon row's state.
#[derive(Clone, Debug, PartialEq)]
pub struct AndonState {
    /// The tenant; the primary key.
    pub tenant_id: Uuid,
    /// Whether the cord is currently pulled.
    pub engaged: bool,
    /// Who pulled it.
    pub engaged_by: Option<Uuid>,
    /// When it was last pulled.
    pub engaged_at: Option<DateTime<Utc>>,
    /// When it was last released.
    pub released_at: Option<DateTime<Utc>>,
    /// Why it was pulled.
    pub reason: String,
    /// The optimistic-concurrency version.
    pub version: Version,
}

#[derive(Debug, sqlx::FromRow)]
struct AndonRow {
    tenant_id: Uuid,
    engaged: bool,
    engaged_by: Option<Uuid>,
    engaged_at: Option<DateTime<Utc>>,
    released_at: Option<DateTime<Utc>>,
    reason: String,
    version: i64,
}

impl From<AndonRow> for AndonState {
    fn from(row: AndonRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            engaged: row.engaged,
            engaged_by: row.engaged_by,
            engaged_at: row.engaged_at,
            released_at: row.released_at,
            reason: row.reason,
            version: version_of(row.version),
        }
    }
}

const COLUMNS: &str = "tenant_id, engaged, engaged_by, engaged_at, released_at, reason, version";

/// Read the tenant's andon row, if it exists.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<AndonState>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM andon_state WHERE tenant_id = $1");
    let row: Option<AndonRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Pull the cord, creating the tenant's row on first use.
pub async fn engage_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    engaged_by: Option<Uuid>,
    reason: &str,
) -> Result<AndonState, AndonStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, engaged_by).await?;
    let sql = format!(
        "INSERT INTO andon_state (tenant_id, engaged, engaged_by, engaged_at, reason)
         VALUES ($1, true, $2, now(), $3)
         ON CONFLICT (tenant_id) DO UPDATE
            SET engaged = true, engaged_by = excluded.engaged_by,
                engaged_at = now(), released_at = NULL, reason = excluded.reason,
                version = andon_state.version + 1, updated_at = now()
         RETURNING {COLUMNS}"
    );
    let row: AndonRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(engaged_by)
        .bind(reason)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// The andon state of every active tenant, for the worker's scrape path.
///
/// One scoped read per tenant, because `andon_state` is RLS'd and `tenant` is the only
/// unscoped table (data-arch §2.1). This is the same shape as the outbox gauge in
/// `crates/api/src/metrics.rs`: a `BYPASSRLS` reader would break the tenancy model, and an
/// unscoped read returns zero rows for the runtime role, which would look exactly like "no
/// tenant has ever pulled the cord".
///
/// A tenant with no row is **not** an error: the cord is lazily created, so absence means
/// `engaged = false`, and the sample must still exist. A tenant whose read **failed** is
/// omitted rather than reported as `false` — a failed read reported as "released" is a
/// safety lie, whereas a missing sample is what S14's `absent()` alarm is for.
///
/// `engaged_at` is the last engage instant; it is only meaningful while `engaged` is true.
pub async fn gauge_snapshot(pool: &PgPool) -> Vec<(TenantId, bool, Option<DateTime<Utc>>)> {
    let tenants = match crate::tenants::active_ids(pool).await {
        Ok(tenants) => tenants,
        Err(error) => {
            tracing::warn!(%error, "andon gauge: tenant enumeration failed");
            return Vec::new();
        }
    };
    let mut samples = Vec::with_capacity(tenants.len());
    for tenant in tenants {
        match scoped_state(pool, tenant).await {
            Ok(state) => {
                let engaged = state.as_ref().is_some_and(|state| state.engaged);
                let engaged_at = state.as_ref().and_then(|state| state.engaged_at);
                samples.push((tenant, engaged, engaged_at));
            }
            Err(error) => {
                tracing::warn!(%error, tenant = %tenant, "andon gauge: read failed");
            }
        }
    }
    samples
}

/// Read one tenant's andon row inside its own scoped transaction.
async fn scoped_state(pool: &PgPool, tenant: TenantId) -> Result<Option<AndonState>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let state = get_tx(&mut tx, tenant).await?;
    tx.commit().await?;
    Ok(state)
}

/// Release the cord. `None` means it was not engaged (or the tenant never used it).
pub async fn release_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<AndonState>, sqlx::Error> {
    let sql = format!(
        "UPDATE andon_state
            SET engaged = false, released_at = now(),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND engaged
         RETURNING {COLUMNS}"
    );
    let row: Option<AndonRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}
