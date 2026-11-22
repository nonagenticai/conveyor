//! The async invocation record (#579).
//!
//! An async submit writes one row and returns its id; the spawned invocation fills in the
//! response when it settles, and a poll reads it back. The row is the whole state — there is no
//! in-process map, so a restart mid-render leaves a `running` row that a later sweep can see
//! rather than an invocation the caller can never resolve.
//!
//! Tenant isolation is the table's RLS policy, exactly as every other tenant table: every
//! function here opens a scoped transaction, so a poll from another tenant reads no row and
//! answers `404`, never another tenant's image.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::scope::begin_scoped;

/// How long a finished invocation stays pollable. The sweep deletes past this.
pub const TTL_HOURS: i64 = 24;

/// What a poll reads.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AsyncInvocation {
    /// The invocation id the submit answered.
    pub id: Uuid,
    /// `running`, `done` or `failed`.
    pub status: String,
    /// The status the synchronous call would have answered, once settled.
    pub response_status: Option<i32>,
    /// The body the synchronous call would have answered, once settled.
    pub response_body: Option<Value>,
    /// When the sweep may remove it.
    pub expires_at: DateTime<Utc>,
}

/// Reserve an invocation for `action` and return its id.
///
/// The second element is `true` when this call created the row and `false` when an existing
/// row answered instead — an async submit with the same `Idempotency-Key` replays the first
/// invocation rather than starting a second render.
pub async fn start(
    pool: &PgPool,
    tenant: TenantId,
    action: &str,
    actor_id: Uuid,
    input_hash: &str,
    idempotency_key: Option<&str>,
) -> Result<(Uuid, bool), sqlx::Error> {
    let id = Uuid::now_v7();
    let mut tx = begin_scoped(pool, tenant).await?;
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO async_invocation
             (id, tenant_id, action, actor_id, input_hash, idempotency_key, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(hours => $7))
         ON CONFLICT (tenant_id, action, idempotency_key)
             WHERE idempotency_key IS NOT NULL
             DO NOTHING
         RETURNING id",
    )
    .bind(id)
    .bind(tenant.as_uuid())
    .bind(action)
    .bind(actor_id)
    .bind(input_hash)
    .bind(idempotency_key)
    .bind(TTL_HOURS as i32)
    .fetch_optional(&mut *tx)
    .await?;
    let created = inserted.is_some();
    let id = match inserted {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "SELECT id FROM async_invocation
              WHERE tenant_id = $1 AND action = $2 AND idempotency_key = $3",
            )
            .bind(tenant.as_uuid())
            .bind(action)
            .bind(idempotency_key)
            .fetch_one(&mut *tx)
            .await?
        }
    };
    tx.commit().await?;
    Ok((id, created))
}

/// Write the settled answer. A no-op when the row was swept mid-flight, which the caller has
/// no way to observe and no reason to care about.
pub async fn finish(
    pool: &PgPool,
    tenant: TenantId,
    id: Uuid,
    status: &str,
    response_status: i32,
    response_body: &Value,
) -> Result<(), sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    sqlx::query(
        "UPDATE async_invocation
            SET status = $2, response_status = $3, response_body = $4,
                updated_at = now(), version = version + 1
          WHERE tenant_id = $1 AND id = $5",
    )
    .bind(tenant.as_uuid())
    .bind(status)
    .bind(response_status)
    .bind(response_body)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Read one invocation under the caller's tenant. `None` is a `404` — it does not distinguish
/// "never existed" from "another tenant's", on purpose.
pub async fn read(
    pool: &PgPool,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<AsyncInvocation>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: Option<AsyncInvocation> = sqlx::query_as(
        "SELECT id, status, response_status, response_body, expires_at
           FROM async_invocation
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// Delete every invocation past its TTL. Returns how many went.
pub async fn sweep(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query("DELETE FROM async_invocation WHERE expires_at <= now()")
        .execute(pool)
        .await?
        .rows_affected();
    Ok(deleted)
}
