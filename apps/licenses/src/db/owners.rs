//! The two ownership lookups the auth middleware needs (migrations 050 and 072).
//!
//! Ported from `models.py::DatabaseManager.resolve_api_client` /
//! `owner_can_access_tenant` / `resolve_realm_owner`. Every one of them fails CLOSED on a
//! database fault: the caller decides how to report it, but `false`/`None` is what a missing
//! row and an error have in common, and neither grants anything.

use sqlx::PgPool;
use uuid::Uuid;

/// `(owner_key, cross_owner)` for a VERIFIED `(realm, azp)`, or `None` for an unknown client.
pub async fn resolve_api_client(
    pool: &PgPool,
    realm: &str,
    client_id: &str,
) -> Result<Option<(String, bool)>, sqlx::Error> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT owner_key, cross_owner FROM api_client WHERE realm = $1 AND client_id = $2",
    )
    .bind(realm)
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The owner of a verified token's Keycloak realm, or `None` when the realm has no row.
pub async fn resolve_realm_owner(
    pool: &PgPool,
    realm: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT owner_key FROM realm_owner WHERE realm = $1")
        .bind(realm)
        .fetch_optional(pool)
        .await
}

/// True iff `owner_key` OWNS `tenant_id` or holds an ACTIVE (`revoked_at IS NULL`) grant on it.
///
/// A non-UUID tenant id is `false` — the reference parses through `uuid.UUID` first.
pub async fn owner_can_access_tenant(
    pool: &PgPool,
    owner_key: &str,
    tenant_id: &str,
) -> Result<bool, sqlx::Error> {
    let Ok(tenant_id) = Uuid::try_parse(tenant_id) else {
        return Ok(false);
    };
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT t.owner_key,
                EXISTS (
                    SELECT 1 FROM tenant_app_grant g
                     WHERE g.tenant_id = t.id
                       AND g.grantee_owner_key = $2
                       AND g.revoked_at IS NULL
                ) AS granted
           FROM tenants t
          WHERE t.id = $1",
    )
    .bind(tenant_id)
    .bind(owner_key)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((owner, granted)) => owner == owner_key || granted,
        None => false,
    })
}

/// The tenant id when one exists with this id (the reference's self-tenant fallback).
pub async fn tenant_exists(pool: &PgPool, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
