//! Enumeration of tenants for the worker.
//!
//! `tenant` is the ONE table with no `tenant_id` and no RLS (data-architecture §2.1), so it
//! can be read unscoped. That is how the worker learns which scopes to enter: it iterates
//! these ids and runs every other query inside [`crate::begin_scoped`]. There is no
//! `BYPASSRLS` role to cross tenants with (S2-D3).

use conveyor_domain::{IdError, TenantId};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Every `active` tenant id, ordered for a deterministic poll.
pub async fn active_ids(pool: &PgPool) -> Result<Vec<TenantId>, sqlx::Error> {
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM tenant WHERE status = 'active' ORDER BY id")
            .fetch_all(pool)
            .await?;
    rows.into_iter()
        .map(|(id,)| TenantId::from_uuid(id).map_err(|e: IdError| sqlx::Error::Decode(Box::new(e))))
        .collect()
}

/// One tenant id by slug (`None` when absent). Used by tests and by the gate's reads.
pub async fn id_by_slug(pool: &PgPool, slug: &str) -> Result<Option<TenantId>, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM tenant WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await?;
    row.map(|(id,)| TenantId::from_uuid(id).map_err(|e: IdError| sqlx::Error::Decode(Box::new(e))))
        .transpose()
}

/// One tenant slug by id (`None` when absent).
///
/// `tenant` is the one table with no `tenant_id` and no RLS, so this read needs no scope. The
/// publish job carries a tenant id and must name the tenant slug in the snapshot it writes.
pub async fn slug_by_id(pool: &PgPool, id: Uuid) -> Result<Option<String>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    slug_by_id_tx(&mut conn, id).await
}

/// [`slug_by_id`] on the caller's connection (the action path already holds one).
pub async fn slug_by_id_tx(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT slug FROM tenant WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}
