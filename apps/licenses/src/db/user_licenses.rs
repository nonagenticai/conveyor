//! DB reads for `GET /licenses/user/{subject_id}`.
//!
//! Ported from `models.py::DatabaseManager.get_subject`,
//! `get_subject_by_keycloak_sub` and `list_licenses` (subject + tenant filters,
//! `deleted_at IS NULL` default). The reference's `list_licenses` adds no ORDER BY, so
//! neither does this one: row order stays the server's choice, exactly as `query.all()`
//! leaves it.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// One subject row: the two identifiers the route's authorization needs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Subject {
    pub id: Uuid,
    pub keycloak_sub: Option<String>,
}

/// `get_subject_by_keycloak_sub`: the portal passes the Keycloak `sub` claim.
///
/// The reference uses `.first()`; `LIMIT 1` is the same arbitrary first row. A duplicate
/// `keycloak_sub` is a data defect, not a reason to widen the lookup.
pub async fn subject_by_keycloak_sub(
    pool: &PgPool,
    keycloak_sub: &str,
) -> Result<Option<Subject>, sqlx::Error> {
    sqlx::query_as("SELECT id, keycloak_sub FROM subject WHERE keycloak_sub = $1 LIMIT 1")
        .bind(keycloak_sub)
        .fetch_optional(pool)
        .await
}

/// `get_subject`: the internal `subject.id` UUID (the fallback lookup).
pub async fn subject_by_id(
    pool: &PgPool,
    subject_id: Uuid,
) -> Result<Option<Subject>, sqlx::Error> {
    sqlx::query_as("SELECT id, keycloak_sub FROM subject WHERE id = $1")
        .bind(subject_id)
        .fetch_optional(pool)
        .await
}

/// One licence row plus the app fields the response enriches with.
///
/// `starts_at`, `created_at` and `updated_at` are non-optional on purpose: the reference's
/// `to_dict()` emits `None` for a NULL one and Pydantic then refuses the `LicenseResponse`
/// field, which the handler's top-level `except` turns into a 500. Decoding into a
/// non-optional type reproduces that fail-closed behaviour instead of emitting a null the
/// schema forbids.
#[derive(Debug, sqlx::FromRow)]
pub struct LicenseRow {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub tenant_id: Uuid,
    pub app_id: Uuid,
    pub plan_id: Uuid,
    pub region_id: Option<Uuid>,
    pub status: String,
    pub starts_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub license_metadata: Option<Value>,
    pub metering_provisioned_at: Option<DateTime<Utc>>,
    pub metering_verified_at: Option<DateTime<Utc>>,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    pub is_dependency: Option<bool>,
    pub bundle_id: Option<Uuid>,
    pub app_key: Option<String>,
    pub app_name: Option<String>,
}

/// `DatabaseManager.list_licenses(subject_id=…, tenant_id=…)`.
///
/// `tenant_id` is `None` only for `license-admin`; every other caller passes the tenant the
/// handler resolved with `get_tenant_id`, so a NULL tenant filter is the operator surface and
/// never a widening of an ordinary read. Soft-deleted rows are excluded because the reference
/// calls this with `include_deleted=False`, whose unconditional arm is `deleted_at IS NULL`.
///
/// The app join replaces the reference's `list_apps` prefetch: same enrichment, one query, and
/// a missing app row yields `None` (`lic_dict["app"]` is then simply never set).
pub async fn list_for_subject(
    pool: &PgPool,
    subject_id: Uuid,
    tenant_id: Option<Uuid>,
) -> Result<Vec<LicenseRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id,
                l.subject_id,
                l.tenant_id,
                l.app_id,
                l.plan_id,
                l.region_id,
                l.status,
                l.starts_at,
                l.expires_at,
                l.created_at,
                l.updated_at,
                l.revoked_at,
                l.metadata AS license_metadata,
                l.metering_provisioned_at,
                l.metering_verified_at,
                l.token_budget_override,
                l.quota_override,
                l.is_dependency,
                l.bundle_id,
                a.key AS app_key,
                a.name AS app_name
           FROM license l
           LEFT JOIN app a ON a.id = l.app_id
          WHERE l.subject_id = $1
            AND l.deleted_at IS NULL
            AND ($2::uuid IS NULL OR l.tenant_id = $2)",
    )
    .bind(subject_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}
