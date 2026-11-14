//! DB reads/writes for the six catalogue APP operations.
//!
//! Ported from the pinned reference (`4e8a08d`):
//!
//! | Rust | reference |
//! |---|---|
//! | [`list_apps`] | `models.py:6844` `DatabaseManager.list_apps` |
//! | [`get_app`] / [`get_app_by_key`] | `models.py:6518` / `6534` |
//! | [`list_plans`] | `models.py:6935` `DatabaseManager.list_plans` |
//! | [`list_app_licences`] | `models.py:7661` `DatabaseManager.list_licenses` |
//! | [`create_app`] | `models.py:6455` `DatabaseManager.create_app` |
//! | [`app_cascade_radius`] | `models.py:8249` |
//! | [`delete_app`] | `models.py:8293` |
//! | [`owner_exists`] | `models.py:5601` |
//!
//! The predicates and orderings are the reference's, not improvements on them:
//!
//! - `list_apps` is `revoked = false` unless asked otherwise, `ORDER BY key`;
//! - `list_plans` filters `revoked` unless `include_revoked`, and adds **no** `ORDER BY`
//!   (the reference ends in `query.all()`);
//! - the licence listing is the same statement the two licence-admin routes use — this
//!   module only supplies `app_id` + the caller's tenant filter;
//! - `delete_app` deletes the licences FIRST (their `plan_id` FK is RESTRICT) and then the
//!   app, whose `ON DELETE CASCADE` reaches the plans; the tokens cascade through
//!   `license_id`, exactly as the reference documents.
//!
//! `app` is a GLOBAL row (no `tenant_id`), so nothing here scopes by tenant except the
//! licence listing, where the caller's tenancy is the documented read filter.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::licence_admin::{self, LicenceFilters, LicenceRow};

/// One `app` row, the columns `AppResponse.to_dict()` publishes.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AppRow {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub revoked: bool,
    pub audience: String,
    pub created_at: DateTime<Utc>,
}

/// One `plan` row, the columns `PlanResponse.to_dict()` publishes.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlanRow {
    pub id: Uuid,
    pub app_id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub region_id: Option<Uuid>,
    pub duration_days: Option<i32>,
    pub rate_limit_rpm: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub quota_monthly: Option<i32>,
    pub token_budget_monthly: Option<i32>,
    pub revoked: bool,
    pub created_at: DateTime<Utc>,
}

/// Exactly what deleting one app would destroy — `app_cascade_radius`.
#[derive(Debug, Clone)]
pub struct CascadeRadius {
    pub licenses: i64,
    pub tokens: i64,
    pub plans: i64,
    /// Canonical UUID strings, sorted, as the reference renders the set.
    pub tenants: Vec<String>,
}

const APP_COLUMNS: &str = "id, key, name, description, revoked, audience, created_at";

const PLAN_COLUMNS: &str = "\
       id, app_id, code, name, description, region_id, duration_days,
       rate_limit_rpm, rate_limit_burst, quota_monthly, token_budget_monthly,
       revoked, created_at";

/// `DatabaseManager.list_apps()`: non-revoked only (the handler's no-argument call),
/// `ORDER BY key`.
pub async fn list_apps(pool: &PgPool) -> Result<Vec<AppRow>, sqlx::Error> {
    let sql = format!("SELECT {APP_COLUMNS} FROM app WHERE revoked = false ORDER BY key");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.get_app`: by primary key.
pub async fn get_app(pool: &PgPool, app_id: Uuid) -> Result<Option<AppRow>, sqlx::Error> {
    let sql = format!("SELECT {APP_COLUMNS} FROM app WHERE id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(app_id)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.get_app_by_key`: the unique registry key.
pub async fn get_app_by_key(pool: &PgPool, key: &str) -> Result<Option<AppRow>, sqlx::Error> {
    let sql = format!("SELECT {APP_COLUMNS} FROM app WHERE key = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(key)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.list_plans(app_id=…, include_revoked=…)`: revoked excluded by default,
/// and deliberately no `ORDER BY`.
pub async fn list_plans(
    pool: &PgPool,
    app_id: Uuid,
    include_revoked: bool,
) -> Result<Vec<PlanRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PLAN_COLUMNS}
           FROM plan
          WHERE app_id = $1
            AND ($2::boolean OR revoked = false)"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(app_id)
        .bind(include_revoked)
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.list_licenses(app_id=…, tenant_id=…)` for `GET /apps/{id}/licenses`.
///
/// Exactly the reference's defaults: `deleted_at IS NULL`, no status filter, no ordering.
pub async fn list_app_licences(
    pool: &PgPool,
    app_id: Uuid,
    tenant_id: Option<Uuid>,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    licence_admin::list_licenses(
        pool,
        &LicenceFilters {
            tenant_id,
            subject_id: None,
            app_id: Some(app_id),
            plan_id: None,
            status: None,
            include_deleted: false,
        },
    )
    .await
}

/// `DatabaseManager.owner_exists`: is this a `tenant_owner` key?
pub async fn owner_exists(pool: &PgPool, owner_key: &str) -> Result<bool, sqlx::Error> {
    let id: Option<String> = sqlx::query_scalar("SELECT key FROM tenant_owner WHERE key = $1")
        .bind(owner_key)
        .fetch_optional(pool)
        .await?;
    Ok(id.is_some())
}

/// `DatabaseManager.create_app`: the app AND its `app_issuer` rows in ONE transaction.
///
/// `issuers` is required and de-duplicated (the reference's `dict.fromkeys`); the caller
/// has already refused an empty list and every unknown owner key.
pub async fn create_app(
    pool: &PgPool,
    key: &str,
    name: &str,
    description: Option<&str>,
    audience: &str,
    issuers: &[String],
) -> Result<AppRow, sqlx::Error> {
    let mut seen = std::collections::BTreeSet::new();
    let mut tx = pool.begin().await?;
    let sql = format!(
        "INSERT INTO app (key, name, description, audience)
         VALUES ($1, $2, $3, $4)
         RETURNING {APP_COLUMNS}"
    );
    let row: AppRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(key)
        .bind(name)
        .bind(description)
        .bind(audience)
        .fetch_one(&mut *tx)
        .await?;
    for owner in issuers {
        if seen.insert(owner.as_str()) {
            sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
                .bind(row.id)
                .bind(owner)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(row)
}

/// `DatabaseManager.app_cascade_radius`: count what a delete would destroy, BEFORE doing it.
///
/// `None` means the app does not exist (the route's 404). The tenant set is sorted and
/// rendered as strings because it lands in the 409 body and the audit log line.
pub async fn app_cascade_radius(
    pool: &PgPool,
    app_id: Uuid,
) -> Result<Option<CascadeRadius>, sqlx::Error> {
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM app WHERE id = $1")
        .bind(app_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Ok(None);
    }

    let licenses: i64 = sqlx::query_scalar("SELECT count(*) FROM license WHERE app_id = $1")
        .bind(app_id)
        .fetch_one(pool)
        .await?;
    let tokens: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM token
          WHERE license_id IN (SELECT id FROM license WHERE app_id = $1)",
    )
    .bind(app_id)
    .fetch_one(pool)
    .await?;
    let plans: i64 = sqlx::query_scalar("SELECT count(*) FROM plan WHERE app_id = $1")
        .bind(app_id)
        .fetch_one(pool)
        .await?;
    let tenants: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT tenant_id FROM license WHERE app_id = $1 ORDER BY tenant_id",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(CascadeRadius {
        licenses,
        tokens,
        plans,
        tenants: tenants.into_iter().map(|id| id.to_string()).collect(),
    }))
}

/// `DatabaseManager.delete_app`: hard delete, licences first, then the app.
///
/// Deleting the licences first is load-bearing: `license.plan_id` is `ON DELETE RESTRICT`,
/// so the plans cannot go until no licence references them. The app delete then cascades to
/// the plans (and any other app-owned rows); the tokens go with their licences.
///
/// Returns `false` when the app does not exist. The WARNING line is the only record of a
/// confirmed cross-tenant destruction — this service has no audit table.
pub async fn delete_app(pool: &PgPool, app_id: Uuid) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let app_key: Option<String> = sqlx::query_scalar("SELECT key FROM app WHERE id = $1")
        .bind(app_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(app_key) = app_key else {
        return Ok(false);
    };
    let deleted = sqlx::query("DELETE FROM license WHERE app_id = $1")
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app WHERE id = $1")
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::warn!(
        app_id = %app_id,
        key = %app_key,
        licenses_deleted = deleted.rows_affected(),
        "app_deleted (tokens cascade via license_id FK; irreversible, no audit table)"
    );
    Ok(true)
}
