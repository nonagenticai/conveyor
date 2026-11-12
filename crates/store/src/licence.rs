//! `app_licence`: the locally-synced entitlement the request path reads.
//!
//! CONTRACT §9: `RequireLicence` reads THIS TABLE only. No core request ever calls the
//! licences service; `licence.sync` (a job) is the only writer and the only network path.
//! One row per `(tenant, app_key)`; `checked_at` is what makes a stale row refusable.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgPool;

use crate::scope::begin_scoped;

/// The default `app_key` (CONTRACT §9).
pub const DEFAULT_APP_KEY: &str = "conveyor";

/// One synced entitlement row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LicenceRow {
    pub tenant_id: uuid::Uuid,
    pub app_key: String,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
    pub raw: Value,
}

/// Read this tenant's entitlement for `app_key`, inside the tenant scope.
pub async fn get(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
) -> Result<Option<LicenceRow>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: Option<LicenceRow> = sqlx::query_as(
        "SELECT tenant_id, app_key, status, expires_at, checked_at, raw
           FROM app_licence
          WHERE tenant_id = $1 AND app_key = $2",
    )
    .bind(tenant.as_uuid())
    .bind(app_key)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// What `licence.sync` writes.
#[derive(Clone, Debug)]
pub struct LicenceUpsert {
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub raw: Value,
}

/// Upsert the entitlement, stamping `checked_at = now()`.
pub async fn upsert(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    update: &LicenceUpsert,
) -> Result<(), sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    sqlx::query(
        "INSERT INTO app_licence (tenant_id, app_key, status, expires_at, checked_at, raw)
         VALUES ($1, $2, $3, $4, now(), $5)
         ON CONFLICT (tenant_id, app_key) DO UPDATE
             SET status = excluded.status,
                 expires_at = excluded.expires_at,
                 checked_at = excluded.checked_at,
                 raw = excluded.raw,
                 version = app_licence.version + 1,
                 updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(app_key)
    .bind(&update.status)
    .bind(update.expires_at)
    .bind(&update.raw)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
