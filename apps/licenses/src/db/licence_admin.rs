//! DB reads/writes for the licence-admin surface: `GET /licenses`,
//! `GET /licenses/admin/all` and `PATCH /licenses/{license_id}/budget`.
//!
//! Ported from the pinned reference (`models.py::DatabaseManager.list_licenses`,
//! `get_license` and `set_license_token_budget_override`). The predicates are the
//! reference's, not improvements on them:
//!
//! - the id/status filters are applied with the reference's falsy semantics (`?status=`
//!   is "no filter"); the handler validates the UUIDs before they arrive here;
//! - `deleted_at IS NULL` unless `include_deleted` — the read half that let
//!   `LICENSE_SOFT_DELETE` be armed; the reference deliberately does not offer the flag
//!   on the per-tenant listing, only on the cross-tenant admin audit view;
//! - **no `ORDER BY`**: the reference finishes with `query.all()`, so the row order is
//!   the server's choice here too.
//!
//! The app and plan joins replace the reference's two `joinedload`s: same enrichment in
//! one statement, and a missing row leaves the corresponding nested object null.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// One licence row plus the app and plan fields `_enrich_license_response` reads.
///
/// `starts_at`, `created_at` and `updated_at` are non-optional on purpose: the reference's
/// `to_dict()` emits `None` for a NULL one and Pydantic then refuses the `LicenseResponse`
/// field, which the handler's top-level `except` turns into a 500. Decoding into a
/// non-optional type reproduces that fail-closed behaviour.
#[derive(Debug, sqlx::FromRow)]
pub struct LicenceRow {
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
    pub plan_code: Option<String>,
    pub plan_name: Option<String>,
    pub plan_rate_limit_rpm: Option<i32>,
    pub plan_rate_limit_burst: Option<i32>,
    pub plan_quota_monthly: Option<i32>,
    pub plan_token_budget_monthly: Option<i32>,
}

/// The shared column list (both enrichment joins expose `l`/`a`/`p`).
const LICENCE_COLUMNS: &str = "\
       l.id,
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
       a.name AS app_name,
       p.code AS plan_code,
       p.name AS plan_name,
       p.rate_limit_rpm AS plan_rate_limit_rpm,
       p.rate_limit_burst AS plan_rate_limit_burst,
       p.quota_monthly AS plan_quota_monthly,
       p.token_budget_monthly AS plan_token_budget_monthly";

/// The two joins, ending as the FROM clause the read statements append predicates to.
const LICENCE_FROM: &str = "\
  FROM license l
  LEFT JOIN app a ON a.id = l.app_id
  LEFT JOIN plan p ON p.id = l.plan_id";

/// The filters the two listing handlers resolved before the read.
#[derive(Debug, Clone)]
pub struct LicenceFilters<'a> {
    /// `None` only on the cross-tenant admin surface; `GET /licenses` always scopes.
    pub tenant_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    pub app_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    /// Empty is "no filter", exactly like the reference's falsy `if status:`.
    pub status: Option<&'a str>,
    pub include_deleted: bool,
}

/// `DatabaseManager.list_licenses` for the two listing routes.
///
/// `include_deleted=false` is the unconditional `deleted_at IS NULL` arm; the
/// `effective`/`as_of` mode the reference also carries belongs to the internal reads
/// (billing) and is not reachable from either of these routes.
pub async fn list_licenses(
    pool: &PgPool,
    filters: &LicenceFilters<'_>,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {LICENCE_COLUMNS}
         {LICENCE_FROM}
          WHERE ($1::uuid IS NULL OR l.tenant_id = $1)
            AND ($2::uuid IS NULL OR l.subject_id = $2)
            AND ($3::uuid IS NULL OR l.app_id = $3)
            AND ($4::uuid IS NULL OR l.plan_id = $4)
            AND ($5::text IS NULL OR l.status = $5)
            AND ($6::boolean OR l.deleted_at IS NULL)"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(filters.tenant_id)
        .bind(filters.subject_id)
        .bind(filters.app_id)
        .bind(filters.plan_id)
        .bind(filters.status)
        .bind(filters.include_deleted)
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.get_license`: one row, both joins, **no soft-delete filter** — the
/// budget write must be able to reach a retained (soft-deleted) licence exactly as the
/// reference does before authorizing the caller.
pub async fn get_license(pool: &PgPool, license_id: Uuid) -> Result<Option<LicenceRow>, sqlx::Error> {
    let sql = format!("SELECT {LICENCE_COLUMNS} {LICENCE_FROM} WHERE l.id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(license_id)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.set_license_token_budget_override`: UPDATE-only, one column.
///
/// `None` CLEARS the override (back to the plan's value). `updated_at` advances only when
/// the value actually changes: SQLAlchemy's dirty-check means the reference issues no
/// UPDATE at all when the same value is set again, and the row's `updated_at` is part of
/// the response, so the difference would be visible. A missing row is `None` (the route's
/// race-with-a-hard-delete 404).
pub async fn set_token_budget_override(
    pool: &PgPool,
    license_id: Uuid,
    token_budget_override: Option<i32>,
) -> Result<Option<LicenceRow>, sqlx::Error> {
    let sql = format!(
        "WITH updated AS (
             UPDATE license
                SET token_budget_override = $2,
                    updated_at = CASE
                        WHEN token_budget_override IS DISTINCT FROM $2 THEN now()
                        ELSE updated_at
                    END
              WHERE id = $1
             RETURNING *
         )
         SELECT {LICENCE_COLUMNS}
           FROM updated l
           LEFT JOIN app a ON a.id = l.app_id
           LEFT JOIN plan p ON p.id = l.plan_id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(license_id)
        .bind(token_budget_override)
        .fetch_optional(pool)
        .await
}
