//! DB reads/writes for the seven catalogue PLAN and REGION operations, plus the plan update's
//! billing re-push.
//!
//! Ported from the pinned reference (`4e8a08d`):
//!
//! | Rust | reference |
//! |---|---|
//! | [`create_plan`] | `models.py:6862` `DatabaseManager.create_plan` |
//! | [`update_plan`] | `models.py:6901` `DatabaseManager.update_plan` |
//! | [`get_plan`] | `models.py:6927` |
//! | [`list_plans`] | `models.py:6935` |
//! | [`delete_plan`] | `models.py:8329` |
//! | [`create_region`] / [`get_region`] / [`get_region_by_code`] / [`list_regions`] | `models.py:4219`-`4256` |
//! | [`push_effective_token_budgets`] | `main.py:2174` |
//!
//! The predicates, orderings and side effects are the reference's, not improvements on them:
//!
//! - `list_plans` excludes revoked rows (the handler passes no `include_revoked`), filters only
//!   the ids it was given, and adds **no** `ORDER BY` (the reference ends in `query.all()`);
//! - `list_regions` adds no `ORDER BY` either, and filters `active` only when asked;
//! - `create_plan` reproduces a subtle ORM behaviour: the reference omits `rate_limit_rpm` /
//!   `rate_limit_burst` from its INSERT kwargs when they are NULL, so the COLUMN defaults (60/10)
//!   apply. An explicit `null` therefore yields 60/10, not NULL — this port's `COALESCE` is that
//!   fact, not a convenience. `quota_monthly` and `token_budget_monthly` have no column default,
//!   so NULL passes through;
//! - `update_plan` writes only the fields the caller SENT (`exclude_unset`), in the reference's
//!   allowed set;
//! - `delete_plan` refuses with a conflict while any licence references the plan (the FK is
//!   RESTRICT), checked before the DELETE exactly as the reference checks it;
//! - the update-time billing re-push selects the reference's exact licence set and reproduces its
//!   failure contract: a failed push is recorded in the result and `metering_verified_at` is
//!   cleared, never raised into the caller's request.
//!
//! `app` and `region` are GLOBAL catalogue rows; nothing here scopes by tenant.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

use crate::billing::BillingTeardown;
use crate::billing::metering::{BILLING_SENTINEL_TENANT_IDS, MeteringProvisioner};
use crate::db::catalogue_apps::PlanRow;

/// One `region` row, the columns `RegionResponse.to_dict()` publishes.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RegionRow {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

/// The columns `PlanResponse.to_dict()` publishes, in the order [`PlanRow`] reads them.
const PLAN_COLUMNS: &str = "\
       id, app_id, code, name, description, region_id, duration_days,
       rate_limit_rpm, rate_limit_burst, quota_monthly, token_budget_monthly,
       revoked, created_at";

const REGION_COLUMNS: &str = "id, code, name, active, created_at";

/// Everything `create_plan` writes. `None` on the two rate-limit fields means "the column
/// default", exactly as an omitted kwarg does in the reference.
#[derive(Debug, Clone)]
pub struct NewPlan {
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
}

/// `PlanUpdate` with the SET state kept: `None` = the key was absent (do not write),
/// `Some(None)` = the key was present and null (write NULL), `Some(Some(v))` = write `v`.
#[derive(Debug, Clone, Default)]
pub struct PlanChanges {
    pub name: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub duration_days: Option<Option<i32>>,
    pub rate_limit_rpm: Option<Option<i32>>,
    pub rate_limit_burst: Option<Option<i32>>,
    pub quota_monthly: Option<Option<i32>>,
    pub token_budget_monthly: Option<Option<i32>>,
    pub revoked: Option<Option<bool>>,
}

impl PlanChanges {
    /// `if not updates` in the reference: no key at all was sent.
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.duration_days.is_none()
            && self.rate_limit_rpm.is_none()
            && self.rate_limit_burst.is_none()
            && self.quota_monthly.is_none()
            && self.token_budget_monthly.is_none()
            && self.revoked.is_none()
    }
}

/// What deleting a plan did — the reference's `(found, reason)` tuple, named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletePlanOutcome {
    Deleted,
    NotFound,
    /// At least one licence still references the plan.
    Conflict,
}

/// `DatabaseManager.create_region`.
pub async fn create_region(
    pool: &PgPool,
    code: &str,
    name: &str,
    active: bool,
) -> Result<RegionRow, sqlx::Error> {
    let sql = format!(
        "INSERT INTO region (code, name, active) VALUES ($1, $2, $3) RETURNING {REGION_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(code)
        .bind(name)
        .bind(active)
        .fetch_one(pool)
        .await
}

/// `DatabaseManager.get_region`.
pub async fn get_region(pool: &PgPool, region_id: Uuid) -> Result<Option<RegionRow>, sqlx::Error> {
    let sql = format!("SELECT {REGION_COLUMNS} FROM region WHERE id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(region_id)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.get_region_by_code`.
pub async fn get_region_by_code(
    pool: &PgPool,
    code: &str,
) -> Result<Option<RegionRow>, sqlx::Error> {
    let sql = format!("SELECT {REGION_COLUMNS} FROM region WHERE code = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(code)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.list_regions(active_only=…)`: no `ORDER BY`, exactly as the reference.
pub async fn list_regions(pool: &PgPool, active_only: bool) -> Result<Vec<RegionRow>, sqlx::Error> {
    let sql = if active_only {
        format!("SELECT {REGION_COLUMNS} FROM region WHERE active = true")
    } else {
        format!("SELECT {REGION_COLUMNS} FROM region")
    };
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.create_plan`.
///
/// `COALESCE($7, 60)` / `COALESCE($8, 10)` are the reference's `if rate_limit_rpm is not None`
/// kwarg guard: an omitted OR explicitly null rate limit lands on the column default.
pub async fn create_plan(pool: &PgPool, plan: &NewPlan) -> Result<PlanRow, sqlx::Error> {
    let sql = format!(
        "INSERT INTO plan
             (app_id, code, name, description, region_id, duration_days,
              rate_limit_rpm, rate_limit_burst, quota_monthly, token_budget_monthly)
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, 60), COALESCE($8, 10), $9, $10)
         RETURNING {PLAN_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(plan.app_id)
        .bind(&plan.code)
        .bind(&plan.name)
        .bind(plan.description.as_deref())
        .bind(plan.region_id)
        .bind(plan.duration_days)
        .bind(plan.rate_limit_rpm)
        .bind(plan.rate_limit_burst)
        .bind(plan.quota_monthly)
        .bind(plan.token_budget_monthly)
        .fetch_one(pool)
        .await
}

/// `DatabaseManager.get_plan`.
pub async fn get_plan(pool: &PgPool, plan_id: Uuid) -> Result<Option<PlanRow>, sqlx::Error> {
    let sql = format!("SELECT {PLAN_COLUMNS} FROM plan WHERE id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(plan_id)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.list_plans(app_id=…, region_id=…)`: revoked excluded, optional id filters,
/// and deliberately no `ORDER BY`.
pub async fn list_plans(
    pool: &PgPool,
    app_id: Option<Uuid>,
    region_id: Option<Uuid>,
) -> Result<Vec<PlanRow>, sqlx::Error> {
    let mut builder = QueryBuilder::new(format!(
        "SELECT {PLAN_COLUMNS} FROM plan WHERE revoked = false"
    ));
    if let Some(app_id) = app_id {
        builder.push(" AND app_id = ").push_bind(app_id);
    }
    if let Some(region_id) = region_id {
        builder.push(" AND region_id = ").push_bind(region_id);
    }
    builder
        .build_query_as::<PlanRow>()
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.update_plan`: write only the fields that were SENT.
///
/// An explicit null on a NOT NULL column (`name`, `rate_limit_rpm`, `rate_limit_burst`,
/// `revoked`) raises here, which the route turns into the reference's 500 — the handler has
/// already refused the one null it names (`revoked`).
pub async fn update_plan(
    pool: &PgPool,
    plan_id: Uuid,
    changes: &PlanChanges,
) -> Result<Option<PlanRow>, sqlx::Error> {
    let mut builder = QueryBuilder::new("UPDATE plan SET ");
    let mut wrote = false;
    {
        let mut sets = builder.separated(", ");

        macro_rules! set_column {
            ($field:expr, $column:literal) => {
                if let Some(value) = $field {
                    sets.push(concat!($column, " = "));
                    sets.push_bind_unseparated(value.clone());
                    wrote = true;
                }
            };
        }
        set_column!(&changes.name, "name");
        set_column!(&changes.description, "description");
        set_column!(&changes.duration_days, "duration_days");
        set_column!(&changes.rate_limit_rpm, "rate_limit_rpm");
        set_column!(&changes.rate_limit_burst, "rate_limit_burst");
        set_column!(&changes.quota_monthly, "quota_monthly");
        set_column!(&changes.token_budget_monthly, "token_budget_monthly");
        set_column!(&changes.revoked, "revoked");
    }

    if !wrote {
        // The route refuses an empty update before reaching here; returning None keeps this
        // function from emitting an invalid statement if that guard ever moves.
        return Ok(None);
    }
    builder.push(" WHERE id = ").push_bind(plan_id);
    builder.push(" RETURNING ").push(PLAN_COLUMNS);
    builder.build_query_as::<PlanRow>().fetch_optional(pool).await
}

/// `DatabaseManager.delete_plan`: a hard delete, refused while a licence references the plan.
///
/// The reference checks the licence count and then deletes (there is no transaction); a licence
/// inserted between the two makes the DELETE raise the FK's `23503`, which the route reports as
/// its 500 — the same shape the reference produces.
pub async fn delete_plan(
    pool: &PgPool,
    plan_id: Uuid,
) -> Result<DeletePlanOutcome, sqlx::Error> {
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM plan WHERE id = $1")
        .bind(plan_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Ok(DeletePlanOutcome::NotFound);
    }
    let referencing: i64 = sqlx::query_scalar("SELECT count(*) FROM license WHERE plan_id = $1")
        .bind(plan_id)
        .fetch_one(pool)
        .await?;
    if referencing > 0 {
        return Ok(DeletePlanOutcome::Conflict);
    }
    sqlx::query("DELETE FROM plan WHERE id = $1")
        .bind(plan_id)
        .execute(pool)
        .await?;
    Ok(DeletePlanOutcome::Deleted)
}

/// `main.py::TokenBudgetRepushResult`: one entry per examined licence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenBudgetRepushResult {
    pub pushed: Vec<String>,
    pub torn_down: Vec<String>,
    pub failed: Vec<String>,
    pub skipped_money_tenant: Vec<String>,
    pub skipped_sentinel: Vec<String>,
}

/// `main.py::push_effective_token_budgets`, the UPDATE-TIME counterpart of licence-time metering.
///
/// Selection is the reference's, with the one caller's `plan_driven_only=true` fixed: LIVE
/// licences on the plan (`status='active'`, not soft-deleted, not past `expires_at`) that carry
/// NO `token_budget_override`. Ordered by `created_at`, as the reference orders it.
///
/// Per licence: a money tenant and billing's sentinel tenant are SKIPPED (not failed); an
/// effective budget of NULL/negative is TORN DOWN (a stale cap would keep enforcing a limit the
/// plan no longer has); anything else is pushed as billing's `license_sync` upsert.
///
/// Failure contract — the reference's: this never raises and never fails the caller's request.
/// A selection failure returns an empty result; a failed push is recorded in [`failed`], logs
/// [`crate::billing::metering::METERING_FAILURE_MARKER`], and clears `metering_verified_at` so the
/// reconcile sweep repairs it.
///
/// Deliberate narrowing of the reference signature: `license_ids` and a variable
/// `plan_driven_only` are not carried because the plan update is the only ported caller and it
/// passes `plan_driven_only=true`; adding unused knobs would be untested surface. The service
/// bearer is minted ONCE for the run (the reference resolves it per licence through a cached
/// helper); the call it authenticates is identical.
pub async fn push_effective_token_budgets(
    pool: &PgPool,
    metering: Option<&MeteringProvisioner>,
    billing: Option<&BillingTeardown>,
    plan_id: Uuid,
    reason: &str,
) -> TokenBudgetRepushResult {
    type Row = (Uuid, Uuid, Option<i32>, Option<i32>, Option<String>);

    let rows: Vec<Row> = match sqlx::query_as(
        "SELECT l.id, l.tenant_id, l.token_budget_override, p.token_budget_monthly,
                t.budget_currency::text
           FROM license l
           JOIN plan p ON p.id = l.plan_id
           LEFT JOIN tenants t ON t.id = l.tenant_id
          WHERE l.plan_id = $1
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND (l.expires_at IS NULL OR l.expires_at > now())
            AND l.token_budget_override IS NULL
          ORDER BY l.created_at",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(
                %error,
                %plan_id,
                reason,
                marker = crate::billing::metering::METERING_FAILURE_MARKER,
                repair = crate::billing::metering::METERING_RECONCILE_PATH,
                "token budget re-push could not select licences — nothing pushed"
            );
            return TokenBudgetRepushResult::default();
        }
    };

    let authorization = match metering {
        Some(metering) => metering.authorization().await,
        None => None,
    };

    let mut result = TokenBudgetRepushResult::default();
    for (license_id, tenant_id, override_budget, plan_budget, budget_currency) in rows {
        let license = license_id.to_string();
        let tenant = tenant_id.to_string();
        let budget = override_budget.or(plan_budget);

        if budget_currency.is_some() {
            result.skipped_money_tenant.push(license);
            continue;
        }
        if BILLING_SENTINEL_TENANT_IDS.contains(&tenant.as_str()) {
            result.skipped_sentinel.push(license);
            continue;
        }

        // The reference: NULL (unlimited) or negative → tear the stale cap down; anything
        // else → billing's licence-keyed `license_sync` upsert.
        let owed_teardown = match budget {
            None => true,
            Some(value) => value < 0,
        };
        let ok = if owed_teardown {
            match billing {
                Some(billing) => billing.teardown_budget(&tenant, &license).await,
                None => false,
            }
        } else {
            match (metering, budget) {
                (Some(metering), Some(budget)) => {
                    metering
                        .push_token_budget(
                            license_id,
                            tenant_id,
                            i64::from(budget),
                            authorization.as_deref(),
                        )
                        .await
                }
                _ => false,
            }
        };

        if ok {
            if owed_teardown {
                result.torn_down.push(license);
            } else {
                result.pushed.push(license);
            }
            continue;
        }

        result.failed.push(license.clone());
        tracing::error!(
            marker = crate::billing::metering::METERING_FAILURE_MARKER,
            license = %license,
            tenant = %tenant,
            effective_token_budget = ?budget,
            reason,
            repair = crate::billing::metering::METERING_RECONCILE_PATH,
            "token budget change NOT applied in billing"
        );
        if let Err(error) =
            sqlx::query("UPDATE license SET metering_verified_at = NULL WHERE id = $1")
                .bind(license_id)
                .execute(pool)
                .await
        {
            tracing::warn!(
                %error,
                license = %license,
                "could not clear metering_verified_at after a failed token budget push"
            );
        }
    }

    tracing::info!(
        reason,
        pushed = result.pushed.len(),
        torn_down = result.torn_down.len(),
        failed = result.failed.len(),
        skipped_money_tenant = result.skipped_money_tenant.len(),
        skipped_sentinel = result.skipped_sentinel.len(),
        "token budget re-push"
    );
    result
}
