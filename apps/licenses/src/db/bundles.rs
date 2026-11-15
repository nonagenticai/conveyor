//! The bundle-token route's reads, ported from `models.py` and `services/bundle_ensure.py`.
//!
//! Every query here is the reference's own predicate, byte for byte:
//!
//! | Rust | reference |
//! |---|---|
//! | [`tenant`] | `DatabaseManager.get_tenant` (:5410) |
//! | [`dependency_grant_apps`] | `DatabaseManager.dependency_grant_apps` (:5807) |
//! | [`held_bundles`] | `DatabaseManager.list_tenant_bundles` (:6797) |
//! | [`bundle_member_keys`] | `DatabaseManager.list_bundle_apps` (:6604) |
//! | [`app_requirements`] | `DatabaseManager.list_app_requirements` (:6618) |
//! | [`app_by_key`] | `DatabaseManager.get_app_by_key` (:6526) |
//! | [`bundle_app_licences`] | `DatabaseManager.find_bundle_app_licences` (:6814) |
//! | [`tenant_level_app_licences`] | `list_tenant_app_licences` (:6709) + `shared_tenant_licences` (`bundle_ensure.py:604`) |
//! | [`create_api_token`] | `DatabaseManager.create_token` (:8786), explicit-expiry path |
//!
//! The Python functions filter `License.is_active()` in memory after the SQL; the equivalent
//! time-window predicate is spelled out here (`status='active'`, `deleted_at IS NULL`,
//! `starts_at <= now`, `expires_at IS NULL OR expires_at >= now`). `License.effective_status`
//! treats `expires_at == now` as ACTIVE (it expires only when `< now`), which is why the bound is
//! inclusive.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// The tenant fields the mint decides on. `owner_key` is only read for the cross-owner audit.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantRow {
    pub id: Uuid,
    pub owner_key: String,
    pub archived_at: Option<DateTime<Utc>>,
}

/// `(id, revoked)` — the only app facts the dependency path needs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AppRow {
    pub id: Uuid,
    pub revoked: bool,
}

/// One non-revoked bundle the tenant holds.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HeldBundle {
    pub id: Uuid,
    pub key: String,
}

/// A licence candidate: what the route mints on and how far the token may live.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LicenceRow {
    pub id: Uuid,
    pub expires_at: Option<DateTime<Utc>>,
}

/// `DatabaseManager.get_tenant` restricted to the id form the route always passes.
pub async fn tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Option<TenantRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, owner_key, archived_at FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
}

/// `dependency_grant_apps`: the non-revoked app keys `owner_key` holds an APP-SCOPED dependency
/// grant for on this tenant (migration 059).
pub async fn dependency_grant_apps(
    pool: &PgPool,
    tenant_id: Uuid,
    owner_key: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT a.key
           FROM tenant_dependency_grant g
           JOIN app a ON a.id = g.app_id
          WHERE g.tenant_id = $1
            AND g.owner_key = $2
            AND NOT a.revoked",
    )
    .bind(tenant_id)
    .bind(owner_key)
    .fetch_all(pool)
    .await
}

/// `list_tenant_bundles`: the NON-revoked bundles the tenant holds, by key.
pub async fn held_bundles(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<HeldBundle>, sqlx::Error> {
    sqlx::query_as(
        "SELECT b.id, b.key
           FROM bundle b
           JOIN tenant_bundle tb ON tb.bundle_id = b.id
          WHERE tb.tenant_id = $1
            AND NOT b.revoked
          ORDER BY b.key",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// `list_bundle_apps`: the non-revoked apps in the bundle, by key. Only the keys are read.
pub async fn bundle_member_keys(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT a.key
           FROM bundle_app ba
           JOIN app a ON a.id = ba.app_id
          WHERE ba.bundle_id = $1
            AND NOT a.revoked
          ORDER BY a.key",
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await
}

/// `list_app_requirements`: `(app key, required app key, caller owner key)` for every edge between
/// NON-revoked apps, exactly the reference's ordering.
pub async fn app_requirements(pool: &PgPool) -> Result<Vec<(String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.key, r.key, ar.caller_owner_key
           FROM app_requires ar
           JOIN app a ON a.id = ar.app_id
           JOIN app r ON r.id = ar.required_app_id
          WHERE NOT a.revoked AND NOT r.revoked
          ORDER BY 1, 2",
    )
    .fetch_all(pool)
    .await
}

/// `get_app_by_key`.
pub async fn app_by_key(pool: &PgPool, key: &str) -> Result<Option<AppRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, revoked FROM app WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
}

/// `find_bundle_app_licences`: the tenant's effectively-active per-app licences for `app_key` in
/// `bundle_id`, oldest first. The app must be non-revoked.
pub async fn bundle_app_licences(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_id: Uuid,
    app_key: &str,
    now: DateTime<Utc>,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id, l.expires_at
           FROM license l
           JOIN app a ON a.id = l.app_id
          WHERE l.tenant_id = $1
            AND l.bundle_id = $2
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND a.key = $3
            AND NOT a.revoked
            AND l.starts_at <= $4
            AND (l.expires_at IS NULL OR l.expires_at >= $4)
          ORDER BY l.created_at, l.id",
    )
    .bind(tenant_id)
    .bind(bundle_id)
    .bind(app_key)
    .bind(now)
    .fetch_all(pool)
    .await
}

/// `bundle_ensure.shared_tenant_licences`: the tenant's effectively-active TENANT-LEVEL licences
/// for `app_id`, oldest first.
///
/// A tenant-level holder is a `svc:` subject or a subject in the `service` realm, never an
/// externally-held `cp-tenant` subject (`is_tenant_level_subject` + `is_externally_held_subject`).
/// The reference deliberately does NOT filter on `bundle_id`: a licence "wherever it sits"
/// satisfies the dependency (`plan_dependency`: "any existing one satisfies the dependency
/// (`exists`, nothing written), wherever it sits").
pub async fn tenant_level_app_licences(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id, l.expires_at
           FROM license l
           JOIN subject s ON s.id = l.subject_id
          WHERE l.tenant_id = $1
            AND l.app_id = $2
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND l.starts_at <= $3
            AND (l.expires_at IS NULL OR l.expires_at >= $3)
            AND s.keycloak_realm <> 'cp-tenant'
            AND (s.keycloak_sub LIKE 'svc:%' OR s.keycloak_realm = 'service')
          ORDER BY l.created_at, l.id",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(now)
    .fetch_all(pool)
    .await
}

/// `create_token` for the route's explicit-expiry path: one `api` row with NO granted roles,
/// returning its id. The raw token never reaches this layer — only its hash.
pub async fn create_api_token(
    pool: &PgPool,
    license_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO token (license_id, token_hash, token_type, expires_at, granted_roles)
         VALUES ($1, $2, 'api', $3, '{}'::text[])
         RETURNING id",
    )
    .bind(license_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(pool)
    .await
}
