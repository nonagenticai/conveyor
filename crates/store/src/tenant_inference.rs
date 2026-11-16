#![forbid(unsafe_code)]
//! The tenant's registered inference destination (conveyor#160, SPEC_V2 §11.2 / SPEC D31).
//!
//! One row per tenant. The destination is ordinary configuration — a console may show it — while
//! the credential stays the sealed `inference.token` secret and is never readable back. Every
//! function takes the caller's already-scoped connection plus the tenant it binds.

use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// Why a destination read failed.
#[derive(Debug, thiserror::Error)]
pub enum TenantInferenceStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The tenant's registered destination, or `None` when it has registered none.
///
/// The caller falls back to the platform `CONVEYOR_EXTERNAL_INFERENCE_URL` on `None` and refuses
/// `unconfigured` when that is absent too (P1.2) — this read never invents a default.
pub async fn destination(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<String>, TenantInferenceStoreError> {
    let destination = sqlx::query_scalar::<_, String>(
        "SELECT destination_url FROM tenant_inference_endpoint WHERE tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(conn)
    .await?;
    Ok(destination)
}

/// Register or replace the tenant's destination; one row per tenant.
pub async fn set_destination(
    conn: &mut PgConnection,
    tenant: TenantId,
    destination_url: &str,
    updated_by: Uuid,
) -> Result<(), TenantInferenceStoreError> {
    sqlx::query(
        "INSERT INTO tenant_inference_endpoint (tenant_id, destination_url, updated_by)
         VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id) DO UPDATE
            SET destination_url = excluded.destination_url,
                updated_by = excluded.updated_by,
                updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(destination_url)
    .bind(updated_by)
    .execute(conn)
    .await?;
    Ok(())
}
