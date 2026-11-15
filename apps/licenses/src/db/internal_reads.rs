//! The five internal read queries other services use.
//!
//! Ported from the pinned Python reference (commit `4e8a08d`):
//!
//! - `main.py::internal_list_licenses` → [`list_licenses`] over
//!   `models.py::DatabaseManager.list_licenses`;
//! - `main.py::internal_list_tenants` → [`list_tenants`] over `DatabaseManager.list_tenants`;
//! - `main.py::internal_list_apps` → [`list_apps`] over `DatabaseManager.list_apps`;
//! - `main.py::internal_get_subject` → [`subject_by_keycloak_sub`] / [`subject_by_id`];
//! - `main.py::internal_get_license` → [`get_license`].
//!
//! The predicates and orderings are the reference's, not improvements on them:
//!
//! - `list_licenses` adds **no `ORDER BY`** (`query.all()`), so neither does this one — the row
//!   order stays the server's choice;
//! - `list_tenants` orders by `created_at DESC` and ALWAYS passes
//!   `include_service_actors=True`: this endpoint is an identity map, not a pick-list, so the 33
//!   service-actor rows billing has already metered must remain nameable;
//! - `list_apps` orders by `key` and includes revoked rows only on request;
//! - `get_license` has no soft-delete filter (it is a by-id read of the retained row).
//!
//! ⚠️ The licence listing's `effective=true` + `status=active` pair applies
//! `models.effectively_active_clauses` — the SAME three-part predicate (and soft-delete arms) as
//! `/internal/tenants/{id}/seat-count`, because billing reads both for one invoice. `effective`
//! only changes what `status=active` means; the terminal statuses are time-invariant, exactly as
//! in the reference.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

/// `License.effective_status`: stored status resolved against the time window.
///
/// A stored status outside `active` is returned VERBATIM, never normalised. For a stored
/// `active` row `pending` (starts_at in the future) is reported before `expired`, so a row that
/// is somehow both reads as the earlier defect in its own lifecycle. The returned verdict is
/// evaluated at `now`, never at the listing's `as_of` — the reference calls the method on each
/// row, and only the WHERE clause honours `as_of`.
pub fn effective_status(
    stored: &str,
    starts_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if stored != "active" {
        return stored.to_owned();
    }
    if starts_at > now {
        return "pending".to_owned();
    }
    if expires_at.is_some_and(|expires_at| expires_at < now) {
        return "expired".to_owned();
    }
    "active".to_owned()
}

/// One licence row with the app and plan fields both internal licence responses enrich with.
#[derive(Debug, sqlx::FromRow)]
pub struct InternalLicenseRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub subject_id: Uuid,
    pub status: String,
    /// Non-optional on purpose: the reference emits `None` for a NULL one and Pydantic then
    /// refuses the required `starts_at` field, which the handler's top-level `except` turns into
    /// a 500. Decoding into a non-optional type reproduces that fail-closed behaviour.
    pub starts_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub license_metadata: Option<Value>,
    pub is_dependency: bool,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    pub app_id: Option<Uuid>,
    pub app_key: Option<String>,
    pub app_name: Option<String>,
    pub plan_id: Option<Uuid>,
    pub plan_code: Option<String>,
    pub plan_name: Option<String>,
    pub plan_rate_limit_rpm: Option<i32>,
    pub plan_rate_limit_burst: Option<i32>,
    pub plan_quota_monthly: Option<i32>,
    pub plan_token_budget_monthly: Option<i32>,
}

/// The shared SELECT (both licence reads enrich identically).
const LICENSE_SELECT: &str = "\
SELECT l.id,
       l.tenant_id,
       l.subject_id,
       l.status,
       l.starts_at,
       l.expires_at,
       l.metadata AS license_metadata,
       l.is_dependency,
       l.token_budget_override,
       l.quota_override,
       a.id AS app_id,
       a.key AS app_key,
       a.name AS app_name,
       p.id AS plan_id,
       p.code AS plan_code,
       p.name AS plan_name,
       p.rate_limit_rpm AS plan_rate_limit_rpm,
       p.rate_limit_burst AS plan_rate_limit_burst,
       p.quota_monthly AS plan_quota_monthly,
       p.token_budget_monthly AS plan_token_budget_monthly
  FROM license l
  LEFT JOIN app a ON a.id = l.app_id
  LEFT JOIN plan p ON p.id = l.plan_id";

/// `effectively_active_clauses($4)` plus the three id filters.
///
/// ⚠️ The soft-delete arm is required: a soft-deleted row's CURRENT status is `revoked` by
/// construction, so `status` alone cannot say what the row was, and the timestamp alone cannot
/// either without `deleted_from_status`. The `include_deleted` flag is DELIBERATELY not stacked
/// on top here — the reference skips it on this path so a row deleted AFTER `as_of` is still a
/// seat at `as_of`, and a flat `deleted_at IS NULL` would shrink a closed period's answer.
const EFFECTIVE_ACTIVE_SQL: &str = " WHERE ($1::uuid IS NULL OR l.tenant_id = $1)
   AND ($2::uuid IS NULL OR l.app_id = $2)
   AND ($3::uuid IS NULL OR l.plan_id = $3)
   AND (
         (l.deleted_at IS NULL AND l.status = 'active')
      OR (l.deleted_at IS NOT NULL AND l.deleted_at > $4 AND l.deleted_from_status = 'active')
       )
   AND (l.starts_at IS NULL OR l.starts_at <= $4)
   AND (l.expires_at IS NULL OR l.expires_at > $4)";

/// The stored-column path: `status` verbatim, and `deleted_at IS NULL` unless asked otherwise
/// (`$5` carries `include_deleted`).
const STORED_STATUS_SQL: &str = " WHERE ($1::uuid IS NULL OR l.tenant_id = $1)
   AND ($2::uuid IS NULL OR l.app_id = $2)
   AND ($3::uuid IS NULL OR l.plan_id = $3)
   AND ($4::text IS NULL OR l.status = $4)
   AND ($5::boolean OR l.deleted_at IS NULL)";

/// The filters `internal_list_licenses` resolved before the read.
#[derive(Debug, Clone)]
pub struct LicenseFilters {
    /// Validated by the handler; a non-UUID value is the reference's empty list, not a filter.
    pub tenant_id: Option<Uuid>,
    pub app_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    /// Empty string is "no status filter", exactly like the reference's falsy check.
    pub status: Option<String>,
    pub effective: bool,
    pub as_of: DateTime<Utc>,
    pub include_deleted: bool,
}

/// `DatabaseManager.list_licenses`, with the reference's two status modes.
///
/// `effective=true` only changes what `status='active'` MEANS; with any other (or no) status the
/// stored column is filtered verbatim, because the terminal statuses are time-invariant.
pub async fn list_licenses(
    pool: &PgPool,
    filters: &LicenseFilters,
) -> Result<Vec<InternalLicenseRow>, sqlx::Error> {
    let effective_active = filters.effective && filters.status.as_deref() == Some("active");
    if effective_active {
        let sql = format!("{LICENSE_SELECT}{EFFECTIVE_ACTIVE_SQL}");
        sqlx::query_as(AssertSqlSafe(sql))
            .bind(filters.tenant_id)
            .bind(filters.app_id)
            .bind(filters.plan_id)
            .bind(filters.as_of)
            .fetch_all(pool)
            .await
    } else {
        let sql = format!("{LICENSE_SELECT}{STORED_STATUS_SQL}");
        sqlx::query_as(AssertSqlSafe(sql))
            .bind(filters.tenant_id)
            .bind(filters.app_id)
            .bind(filters.plan_id)
            .bind(filters.status.as_deref())
            .bind(filters.include_deleted)
            .fetch_all(pool)
            .await
    }
}

/// `DatabaseManager.get_license`: one row, both joins, NO soft-delete filter.
pub async fn get_license(
    pool: &PgPool,
    license_id: Uuid,
) -> Result<Option<InternalLicenseRow>, sqlx::Error> {
    let sql = format!("{LICENSE_SELECT} WHERE l.id = $1");
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(license_id)
        .fetch_optional(pool)
        .await
}

/// One tenant row, as the identity map discloses it.
#[derive(Debug, sqlx::FromRow)]
pub struct InternalTenantRow {
    pub id: Uuid,
    pub name: String,
    pub keycloak_realm: Option<String>,
    pub archived: bool,
    pub kind: String,
    pub cost_centre: Option<String>,
    pub budget_currency: Option<String>,
    pub owner_key: String,
    pub external_ref: Option<String>,
}

/// `DatabaseManager.list_tenants(include_archived=…, include_service_actors=True)`.
///
/// Service actors are NEVER filtered here: this is an identity map, not a pick-list, and the
/// rows billing has already billed must stay nameable.
pub async fn list_tenants(
    pool: &PgPool,
    include_archived: bool,
) -> Result<Vec<InternalTenantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id,
                name,
                keycloak_realm,
                (archived_at IS NOT NULL) AS archived,
                kind,
                cost_centre,
                budget_currency,
                owner_key,
                external_ref
           FROM tenants
          WHERE ($1::boolean OR archived_at IS NULL)
          ORDER BY created_at DESC",
    )
    .bind(include_archived)
    .fetch_all(pool)
    .await
}

/// One app row, as the identity map discloses it.
#[derive(Debug, sqlx::FromRow)]
pub struct InternalAppRow {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub revoked: bool,
    pub audience: String,
}

/// `DatabaseManager.list_apps`: non-revoked only unless requested, ordered by key.
pub async fn list_apps(
    pool: &PgPool,
    include_revoked: bool,
) -> Result<Vec<InternalAppRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, key, name, revoked, audience
           FROM app
          WHERE ($1::boolean OR revoked = false)
          ORDER BY key",
    )
    .bind(include_revoked)
    .fetch_all(pool)
    .await
}

/// One subject row (`Subject.to_dict()`'s fields, minus the `actor_sub` the response model drops).
#[derive(Debug, sqlx::FromRow)]
pub struct InternalSubjectRow {
    pub id: Uuid,
    pub keycloak_sub: String,
    pub keycloak_realm: String,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// `DatabaseManager.get_subject_by_keycloak_sub`: the Keycloak `sub` claim, the form a real
/// caller sends. `.first()` in the reference; `LIMIT 1` is the same arbitrary first row.
pub async fn subject_by_keycloak_sub(
    pool: &PgPool,
    keycloak_sub: &str,
) -> Result<Option<InternalSubjectRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, keycloak_sub, keycloak_realm, tenant_id, created_at
           FROM subject
          WHERE keycloak_sub = $1
          LIMIT 1",
    )
    .bind(keycloak_sub)
    .fetch_optional(pool)
    .await
}

/// `DatabaseManager.get_subject`: the locally minted uuid7 primary key (the fallback lookup).
pub async fn subject_by_id(
    pool: &PgPool,
    subject_id: Uuid,
) -> Result<Option<InternalSubjectRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, keycloak_sub, keycloak_realm, tenant_id, created_at
           FROM subject
          WHERE id = $1",
    )
    .bind(subject_id)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("rfc3339 instant")
            .with_timezone(&Utc)
    }

    #[test]
    fn effective_status_is_the_reference_chain() {
        let now = at("2026-06-15T00:00:00Z");
        assert_eq!(
            effective_status("revoked", at("2026-01-01T00:00:00Z"), None, now),
            "revoked"
        );
        assert_eq!(
            effective_status("active", at("2026-07-01T00:00:00Z"), None, now),
            "pending"
        );
        assert_eq!(
            effective_status(
                "active",
                at("2026-01-01T00:00:00Z"),
                Some(at("2026-06-01T00:00:00Z")),
                now
            ),
            "expired"
        );
        assert_eq!(
            effective_status("active", at("2026-01-01T00:00:00Z"), None, now),
            "active"
        );
        // pending is reported before expired, exactly as the reference resolves a row that is both.
        assert_eq!(
            effective_status(
                "active",
                at("2026-07-01T00:00:00Z"),
                Some(at("2026-06-01T00:00:00Z")),
                now
            ),
            "pending"
        );
    }
}
