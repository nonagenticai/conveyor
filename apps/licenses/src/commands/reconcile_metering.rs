//! `reconcile-metering`: measure each live licence's metering coverage AGAINST billing and
//! repair it, estate-wide.
//!
//! Ported from `scripts/reconcile_metering.py` (licenses @ 4e8a08d), which calls
//! `main.run_metering_reconcile` in-process. The contract is the script's, not a redesign:
//!
//! - configuration is ALL environment (`RECONCILE_LIMIT`, `RECONCILE_REVALIDATE`,
//!   `RECONCILE_FORCE_PLAN_QUOTA`, `RECONCILE_DRY_RUN`, `RECONCILE_LICENSE_ID`,
//!   `RECONCILE_ACTOR`, plus the process's `DATABASE_URL` / `METERING_SERVICE_URL`), so a
//!   CronJob manifest stays the only knob and the command takes no arguments;
//! - ONE JSON line on stdout on EVERY path, config failures included — a CronJob pod is not
//!   scraped and there is no audit table, so `kubectl logs` IS the audit trail;
//! - exit 0 converged, 1 not converged, 2 config / DB / billing transport failure.
//!
//! ⚠️ THE RUNNER IS SHARED WITH THE HTTP ROUTE. `POST /licenses/admin/metering/reconcile`
//! (`router::licence_admin::reconcile_metering_provisioning`) calls the SAME
//! [`run_metering_reconcile`], so the operator route and the CronJob cannot drift apart. The
//! route contributes exactly the `license-admin` guard, the query parameters and the actor name;
//! the sweep is one implementation.
//!
//! THE SWEEP is the reference's: the independent `live_budget_owing_licenses` count first (a fact
//! about the estate, not about what the sweep just did), then the live/owing selection ordered by
//! `created_at` and limited, skipped when `metering_verified_at` is set unless `revalidate`.
//!
//! ⚠️ BOTH METERING LEGS ARE READ-BEFORE-WRITE.
//!
//! * Budget: `GET /api/budget/list/{tenant}` once per tenant, cached even when it answers
//!   "unknown". Only an `active` `source='license_sync'` row counts as coverage. Absent → create;
//!   present with a DIFFERENT `budget_tokens` → re-push the licence's effective value (2026-10-15
//!   behaviour); present and equal (or with an unreadable monthly value) → no write. An unreadable
//!   list is UNKNOWN, never empty: nothing is created for it.
//! * Quota: `GET /api/quota/check` per licence. Absent → create; present and equal → no write;
//!   present and differing → LEAVE IT and report drift, unless `force_plan_quota` (DESTRUCTIVE,
//!   default off) explicitly asks for the effective value to be pushed over it; unreadable →
//!   UNKNOWN, nothing pushed, the licence fails closed. A 404 is `foreign_tenant`, not absent.
//!
//! ⚠️ THE ANTI-VACUITY GUARDS are the point of this command. After one repairing run every swept
//! licence is stamped, so a `revalidate=false` schedule selects 0 rows, repairs 0 and reports
//! success forever while measuring nothing. `_classify` therefore has four ways to be
//! un-converged (unknown coverage, failed, dry-run uncovereds, vacuous/truncated sweep), in the
//! reference's exact order and wording.
//!
//! ⚠️ ONE DELIBERATE DIVERGENCE: the reference's `startup_config` leg is a failed `import main`
//! (it needs `METERING_SERVICE_URL` and `CORS_ALLOWED_ORIGINS`). A Rust binary has no such
//! import; the equivalent precondition is a usable `METERING_SERVICE_URL`, and its absence is
//! reported with the same payload SHAPE and `error_kind: "startup_config"`. The static error
//! text names only `METERING_SERVICE_URL`, which is the one variable this port actually needs.
//! Everything else — payload keys, exit codes and reason strings — is the script's.
//!
//! ⚠️ ARGUMENTS: the reference's `main_cli` rejects any argv with a JSON failure line. Clap
//! rejects them first here (usage on stderr, no stdout line, still exit 2), so the CronJob's
//! no-argument invocation — the only one that exists — is byte-for-byte the reference's.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use super::RunOutcome;
use crate::billing::metering::{
    BILLING_SENTINEL_TENANT_IDS, METERING_FAILURE_MARKER, MeteringProvisioner, QuotaState,
    effective_metering_values,
};

/// The event name the script reports in every payload.
const EVENT: &str = "metering_reconcile";
/// The script's `DEFAULT_ACTOR`.
const DEFAULT_ACTOR: &str = "cronjob:reconcile-metering";
/// The script's `EXIT_CONVERGED` / `EXIT_NOT_CONVERGED` / `EXIT_TRANSPORT`.
const EXIT_CONVERGED: i32 = 0;
const EXIT_NOT_CONVERGED: i32 = 1;
const EXIT_TRANSPORT: i32 = 2;
/// `_env_int("RECONCILE_LIMIT", 500, minimum=1, maximum=5000)`.
const DEFAULT_LIMIT: i64 = 500;
const MIN_LIMIT: i64 = 1;
const MAX_LIMIT: i64 = 5_000;
/// `main_cli`'s generic-exception error: STATIC, never a database exception or DSN.
const SWEEP_FAILED: &str = "the sweep could not complete; see the traceback on stderr";
/// The reference's `error_kind` for every malformed `RECONCILE_*` value.
const BAD_ENV_KIND: &str = "bad_reconcile_env";
/// A stable counterpart of `type(e).__name__` for a database/transport failure.
const DB_ERROR_KIND: &str = "sqlx::Error";
/// psycopg/SQLAlchemy's class name for the reference's malformed-canonical-id failure.
const BAD_UUID_KIND: &str = "DataError";
/// The port's `_load_app()` failure — see the module's ONE DELIBERATE DIVERGENCE note.
const STARTUP_ERROR: &str = "METERING_SERVICE_URL is missing or unusable — this sweep reads and writes billing, so it \
     cannot start without it";
/// The `error_kind` the reference attaches to that startup refusal.
const STARTUP_KIND: &str = "startup_config";

/// The independent count the anti-vacuity guard compares `scanned` against.
///
/// ⚠️ Hand-written SQL ON PURPOSE, verbatim from the script: if it reused the sweep's own
/// selection the guard could not fail — any drift would move both numbers together.
///
/// `$1` is the licence-id narrowing: `NULL` is no filter, and an EMPTY array selects nothing
/// (the reference's `License.id.in_([])`), never silently "no filter".
const LIVE_BUDGET_OWING_SQL: &str = "SELECT count(*) FROM license l \
       JOIN plan p ON p.id = l.plan_id \
      WHERE l.status = 'active' \
        AND (l.expires_at IS NULL OR l.expires_at > now()) \
        AND (p.quota_monthly IS NOT NULL OR p.token_budget_monthly IS NOT NULL \
             OR l.quota_override IS NOT NULL OR l.token_budget_override IS NOT NULL) \
        AND ($1::uuid[] IS NULL OR l.id = ANY($1))";

/// The sweep's selection: LIVE, effective metering owes billing something, ordered by
/// `created_at` (the reference's `ORDER BY License.created_at`, no tie-breaker), limited.
/// `$4` is the `revalidate` flag: true re-examines already-stamped licences.
/// `$2` is the licence-id narrowing, with the same `NULL`/empty-array rule as the count.
const SELECT_GAPS_SQL: &str = "SELECT l.id, l.tenant_id, p.id, p.quota_monthly, \
            p.token_budget_monthly, l.token_budget_override, l.quota_override, \
            t.budget_currency::text, a.id, a.name \
       FROM license l \
       JOIN plan p ON p.id = l.plan_id \
       JOIN app a ON a.id = l.app_id \
       LEFT JOIN tenants t ON t.id = l.tenant_id \
      WHERE l.status = 'active' \
        AND (l.expires_at IS NULL OR l.expires_at > $1) \
        AND (p.quota_monthly IS NOT NULL OR p.token_budget_monthly IS NOT NULL \
             OR l.quota_override IS NOT NULL OR l.token_budget_override IS NOT NULL) \
        AND ($2::uuid[] IS NULL OR l.id = ANY($2)) \
        AND ($4::boolean OR l.metering_verified_at IS NULL) \
      ORDER BY l.created_at ASC \
      LIMIT $3";

/// `(id, tenant_id, plan_id, plan_quota_monthly, plan_token_budget_monthly,
/// token_budget_override, quota_override, budget_currency, app_id, app_name)`.
type GapRow = (
    Uuid,
    Uuid,
    Uuid,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<String>,
    Uuid,
    String,
);

/// One budget-list read per TENANT: licence id → monthly `budget_tokens`, or `None` for a
/// covered licence whose monthly value billing would not disclose. The OUTER `None` is an
/// unreadable list — an unknown, never an empty map.
type BudgetsByTenant = HashMap<String, Option<HashMap<String, Option<i64>>>>;

/// The six environment knobs, resolved once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub limit: i64,
    pub revalidate: bool,
    pub force_plan_quota: bool,
    pub dry_run: bool,
    pub license_id: Option<String>,
    pub actor: String,
}

/// A malformed `RECONCILE_*` value. `actor` and `license_id` are whatever had been parsed when
/// the refusal happened — in the script's order that is always `None`, because `actor` is last
/// and `RECONCILE_LICENSE_ID` cannot raise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub actor: Option<String>,
    pub license_id: Option<String>,
    pub error: String,
}

impl ConfigError {
    /// The script's `_fail(actor, license_id, str(e), error_kind="bad_reconcile_env")`.
    pub fn outcome(&self) -> RunOutcome {
        failure(
            self.actor.as_deref(),
            self.license_id.as_deref(),
            &self.error,
            BAD_ENV_KIND,
        )
    }
}

impl Settings {
    /// Read the six `RECONCILE_*` variables from the process environment, in the script's order.
    pub fn from_env() -> Result<Settings, ConfigError> {
        Settings::from_lookup(|name| std::env::var(name).ok())
    }

    /// The testable core of `from_env`: the environment is supplied instead of read.
    pub fn from_lookup<F: Fn(&str) -> Option<String>>(lookup: F) -> Result<Settings, ConfigError> {
        let bad = |error: String| ConfigError {
            actor: None,
            license_id: None,
            error,
        };
        let limit = env_int(
            &lookup,
            "RECONCILE_LIMIT",
            DEFAULT_LIMIT,
            MIN_LIMIT,
            MAX_LIMIT,
        )
        .map_err(bad)?;
        let revalidate = env_bool(&lookup, "RECONCILE_REVALIDATE", false).map_err(bad)?;
        // DEFAULT FALSE, and the scheduled manifest must keep it false: true pushes the
        // effective quota over a differing billing row, destroying an operator override.
        let force_plan_quota =
            env_bool(&lookup, "RECONCILE_FORCE_PLAN_QUOTA", false).map_err(bad)?;
        let dry_run = env_bool(&lookup, "RECONCILE_DRY_RUN", false).map_err(bad)?;
        let license_id = env_str(&lookup, "RECONCILE_LICENSE_ID");
        let actor = env_str(&lookup, "RECONCILE_ACTOR").unwrap_or_else(|| DEFAULT_ACTOR.to_owned());
        Ok(Settings {
            limit,
            revalidate,
            force_plan_quota,
            dry_run,
            license_id,
            actor,
        })
    }
}

/// The command as the CronJob runs it: every knob from the process environment.
pub async fn run(database_url: Option<String>) -> RunOutcome {
    run_from(database_url, |name| std::env::var(name).ok()).await
}

/// `run` with the environment supplied, so a test can exercise the config-failure payloads
/// without touching the process environment.
pub async fn run_from<F: Fn(&str) -> Option<String>>(
    database_url: Option<String>,
    lookup: F,
) -> RunOutcome {
    let settings = match Settings::from_lookup(lookup) {
        Ok(settings) => settings,
        Err(bad) => {
            eprintln!("reconcile-metering: refusing to run: {}", bad.error);
            return bad.outcome();
        }
    };
    // THE SEAM the reference's tests replace: the billing client is resolved once, here.
    match MeteringProvisioner::from_env() {
        Some(billing) => run_with(database_url, settings, Some(billing)).await,
        None => failure(
            Some(&settings.actor),
            settings.license_id.as_deref(),
            STARTUP_ERROR,
            STARTUP_KIND,
        ),
    }
}

/// The testable core: the knobs AND the billing client are supplied instead of resolved, so a
/// test can drive the stub without a network and without this process's environment.
pub async fn run_with(
    database_url: Option<String>,
    settings: Settings,
    billing: Option<MeteringProvisioner>,
) -> RunOutcome {
    let Some(billing) = billing else {
        return failure(
            Some(&settings.actor),
            settings.license_id.as_deref(),
            STARTUP_ERROR,
            STARTUP_KIND,
        );
    };
    let Some(url) = database_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
    else {
        return failure(
            Some(&settings.actor),
            settings.license_id.as_deref(),
            SWEEP_FAILED,
            DB_ERROR_KIND,
        );
    };
    // The script's count binds the raw env value and dies on a malformed UUID with a psycopg
    // `DataError`, i.e. the generic exit-2 path. Validate it here, before any sweep, so a
    // typo'd canary can never be read as "that licence is already fine".
    let wanted = match settings.license_id.as_deref() {
        None => None,
        Some(raw) => match Uuid::parse_str(raw) {
            Ok(id) => Some(id),
            Err(error) => {
                eprintln!(
                    "reconcile-metering: RECONCILE_LICENSE_ID={raw:?} is not a UUID: {error}"
                );
                return failure(
                    Some(&settings.actor),
                    settings.license_id.as_deref(),
                    SWEEP_FAILED,
                    BAD_UUID_KIND,
                );
            }
        },
    };
    let wanted_ids = wanted.map(|id| vec![id]);
    let pool = match crate::db::connect(&url, 1).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("reconcile-metering: the sweep could not complete: {error}");
            return failure(
                Some(&settings.actor),
                settings.license_id.as_deref(),
                SWEEP_FAILED,
                DB_ERROR_KIND,
            );
        }
    };
    // The script's independent count runs BEFORE the sweep: it must be a fact about the
    // estate, not about what the sweep just did to it.
    let live = match count_live_budget_owing(&pool, wanted_ids.as_deref()).await {
        Ok(live) => live,
        Err(error) => {
            eprintln!("reconcile-metering: the sweep could not complete: {error}");
            pool.close().await;
            return failure(
                Some(&settings.actor),
                settings.license_id.as_deref(),
                SWEEP_FAILED,
                DB_ERROR_KIND,
            );
        }
    };
    let result = match run_metering_reconcile(
        &pool,
        settings.limit,
        settings.revalidate,
        settings.dry_run,
        settings.force_plan_quota,
        wanted_ids.as_deref(),
        &settings.actor,
        &billing,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            // The exception goes to stderr; stdout keeps the one static JSON line.
            eprintln!("reconcile-metering: the sweep could not complete: {error}");
            pool.close().await;
            return failure(
                Some(&settings.actor),
                settings.license_id.as_deref(),
                SWEEP_FAILED,
                DB_ERROR_KIND,
            );
        }
    };
    pool.close().await;
    outcome(&settings, live, result)
}

/// `MeteringReconcileResult` (`main.py`): the outcome of one metering reconcile sweep.
///
/// The wire shape is the reference's: five REQUIRED fields (no default) and every other field
/// carrying a default. Both the HTTP route and the command payload are built from this one type.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MeteringReconcileResult {
    /// Licences the sweep examined.
    pub scanned: i64,
    /// Licences that came out VERIFIED — a CONVERGED count, not a count of writes.
    pub repaired: i64,
    /// Licences the sweep could not converge.
    pub failed: i64,
    pub repaired_license_ids: Vec<String>,
    pub failed_license_ids: Vec<String>,
    /// Licences that OWE billing a token budget and for which billing reported no active
    /// `source='license_sync'` row AT THE MOMENT IT WAS ASKED.
    #[schema(required = false, default = json!([]))]
    pub uncovered_budget_license_ids: Vec<String>,
    /// Licences whose coverage billing would not disclose. Nothing was created for these.
    #[schema(required = false, default = json!([]))]
    pub unknown_budget_license_ids: Vec<String>,
    /// Budgets this sweep actually created in billing.
    #[schema(required = false, default = 0)]
    pub budgets_created: i64,
    /// Licences whose active `license_sync` budget DIFFERS from the effective budget.
    #[schema(required = false, default = json!([]))]
    pub budget_drift_license_ids: Vec<String>,
    /// Budgets this sweep UPDATED in billing to the effective value.
    #[schema(required = false, default = 0)]
    pub budgets_updated: i64,
    /// Licences that owed a budget and billing already held one — no write, no repair.
    #[schema(required = false, default = 0)]
    pub already_covered: i64,
    /// Licences skipped because billing refuses budgets for its sentinel tenant by design.
    #[schema(required = false, default = 0)]
    pub budgets_skipped_sentinel: i64,
    /// Licences skipped because the tenant's budget is MONEY (migration 043).
    #[schema(required = false, default = 0)]
    pub budgets_skipped_money_tenant: i64,
    /// Quota configs this sweep CREATED in billing.
    #[schema(required = false, default = 0)]
    pub quotas_pushed: i64,
    /// Licences that owe a quota and for which billing held no row at the moment it was asked.
    #[schema(required = false, default = json!([]))]
    pub uncovered_quota_license_ids: Vec<String>,
    /// Licences where billing's quota DIFFERS and was LEFT ALONE (an operator override).
    #[schema(required = false, default = json!([]))]
    pub quota_drift_license_ids: Vec<String>,
    /// Licences whose configured quota billing would not disclose; they fail closed.
    #[schema(required = false, default = json!([]))]
    pub unknown_quota_license_ids: Vec<String>,
    /// WHY those quotas were unreadable, counted by cause.
    #[schema(required = false, default = json!({}))]
    pub unknown_quota_reasons: BTreeMap<String, i64>,
    /// Licences whose billing quota already equals the effective value — no write.
    #[schema(required = false, default = 0)]
    pub quotas_already_configured: i64,
    /// Quota rows this sweep OVERWROTE because `force_plan_quota` was set.
    #[schema(required = false, default = 0)]
    pub quotas_overwritten: i64,
    #[schema(required = false, default = false)]
    pub dry_run: bool,
}

/// The independent count the anti-vacuity guard compares `scanned` against.
///
/// Counted BEFORE the sweep (the script calls it before `run_metering_reconcile`), so it is a
/// fact about the estate, not about what the sweep just did to it.
pub(crate) async fn count_live_budget_owing(
    pool: &PgPool,
    license_ids: Option<&[Uuid]>,
) -> Result<i64, sqlx::Error> {
    let ids = license_ids.map(<[Uuid]>::to_vec);
    sqlx::query_scalar(LIVE_BUDGET_OWING_SQL)
        .bind(ids)
        .fetch_one(pool)
        .await
}

/// THE RUNNER — the reference's `run_metering_reconcile`.
///
/// It carries NO HTTP and NO auth: `POST /licenses/admin/metering/reconcile` and the
/// `reconcile-metering` CronJob both call it, so the two cannot drift apart.
///
/// `license_ids` NARROWS the selection and never widens it: `None` is no id filter, and an
/// EMPTY slice selects nothing — the reference's `License.id.in_([])` — never silently "no
/// filter".
// The reference's own signature is these eight keyword arguments, and the route and the command
// must pass exactly them; bundling them would hide the sweep's contract behind a struct.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_metering_reconcile(
    pool: &PgPool,
    limit: i64,
    revalidate: bool,
    dry_run: bool,
    force_plan_quota: bool,
    license_ids: Option<&[Uuid]>,
    actor: &str,
    billing: &MeteringProvisioner,
) -> Result<MeteringReconcileResult, sqlx::Error> {
    let wanted_ids = license_ids.map(<[Uuid]>::to_vec);
    let now = chrono::Utc::now();
    let gaps: Vec<GapRow> = sqlx::query_as(SELECT_GAPS_SQL)
        .bind(now)
        .bind(wanted_ids)
        .bind(limit)
        .bind(revalidate)
        .fetch_all(pool)
        .await?;
    // `len(gaps)` in the reference's result. Captured before the loop consumes the vector.
    let scanned = gaps.len();

    let mut repaired: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut uncovered_budget: Vec<String> = Vec::new();
    let mut unknown_budget: Vec<String> = Vec::new();
    let mut budget_drift: Vec<String> = Vec::new();
    let mut uncovered_quota: Vec<String> = Vec::new();
    let mut quota_drift: Vec<String> = Vec::new();
    let mut unknown_quota: Vec<String> = Vec::new();
    let mut unknown_quota_reasons: BTreeMap<String, i64> = BTreeMap::new();
    let mut budgets_created = 0_i64;
    let mut budgets_updated = 0_i64;
    let mut already_covered = 0_i64;
    let mut budgets_skipped_sentinel = 0_i64;
    let mut budgets_skipped_money_tenant = 0_i64;
    let mut quotas_pushed = 0_i64;
    let mut quotas_already_configured = 0_i64;
    let mut quotas_overwritten = 0_i64;

    if !gaps.is_empty() {
        eprintln!(
            "reconcile-metering: examining {} live licence(s) whose billing coverage has not \
             been verified (requested by {}, dry_run={}, revalidate={}, force_plan_quota={})",
            gaps.len(),
            actor,
            dry_run,
            revalidate,
            force_plan_quota
        );
        if force_plan_quota {
            eprintln!(
                "reconcile-metering: force_plan_quota=true — an existing billing quota that \
                 differs from the licence's effective value WILL be overwritten."
            );
        }

        // One budget-list read per TENANT, not per licence. `None` is cached too: a tenant
        // billing would not answer for must not be retried once per licence in one sweep.
        let mut budget_ids_by_tenant: BudgetsByTenant = HashMap::new();

        for (
            license_id,
            tenant_id,
            plan_id,
            plan_quota_monthly,
            plan_token_budget_monthly,
            token_budget_override,
            quota_override,
            budget_currency,
            app_id,
            app_name,
        ) in gaps
        {
            // From here on both values are EFFECTIVE (licence override, else plan) — the only
            // values this sweep may push or compare against.
            let (token_budget, quota) = effective_metering_values(
                token_budget_override,
                quota_override,
                plan_token_budget_monthly,
                plan_quota_monthly,
            );
            let lic = license_id.to_string();
            let tenant = tenant_id.to_string();
            let authorization = billing.authorization().await;
            let authorization = authorization.as_deref();

            let mut budget_ok = true;
            if let Some(budget) = token_budget.map(i64::from) {
                if let Some(currency) = budget_currency.as_deref() {
                    // Migration 043 / PLAN §2.2: a MONEY tenant. Billing holds its budget as
                    // money; a license_sync token budget would be a second pooled cap. No read,
                    // no push; the leg is converged.
                    budgets_skipped_money_tenant += 1;
                    eprintln!(
                        "token_budget_skipped_money_tenant license={lic} tenant={tenant} \
                         budget_currency={currency} — reconcile pushes no token budget"
                    );
                } else if BILLING_SENTINEL_TENANT_IDS.contains(&tenant.as_str()) {
                    // Billing refuses budgets for its sentinel tenant BY DESIGN. No read, no
                    // push; the budget leg is converged. Quota handling is unchanged.
                    budgets_skipped_sentinel += 1;
                    eprintln!(
                        "reconcile-metering: skipping token budget for license={lic} \
                         tenant={tenant} — sentinel tenant, billing refuses budgets by design"
                    );
                } else {
                    if !budget_ids_by_tenant.contains_key(&tenant) {
                        let fetched = billing
                            .fetch_license_sync_budget_ids(tenant_id, authorization)
                            .await;
                        budget_ids_by_tenant.insert(tenant.clone(), fetched);
                    }
                    let covered = budget_ids_by_tenant
                        .get_mut(&tenant)
                        .expect("just inserted");
                    if covered.is_none() {
                        // An unreadable list is UNKNOWN, not "nothing owed": creating on an
                        // unknown state is exactly how a duplicate row happens.
                        unknown_budget.push(lic.clone());
                        budget_ok = false;
                    } else {
                        let covered = covered.as_mut().expect("checked above");
                        let case = match covered.get(&lic) {
                            Some(Some(current)) if *current != budget => BudgetCase::Drift,
                            Some(_) => BudgetCase::Covered,
                            None => BudgetCase::Absent,
                        };
                        match case {
                            BudgetCase::Drift => {
                                // Billing holds a cap that disagrees with the licence's
                                // effective value; since 2026-10-15 the licence is the source
                                // of truth and the upsert re-pushes it in place.
                                budget_drift.push(lic.clone());
                                eprintln!(
                                    "reconcile-metering: billing budget for license={lic} \
                                     tenant={tenant} differs from the effective budget {budget}{}",
                                    if dry_run {
                                        " — dry run, nothing pushed"
                                    } else {
                                        " — pushing the upsert"
                                    }
                                );
                                if !dry_run {
                                    budget_ok = billing
                                        .push_token_budget(
                                            license_id,
                                            tenant_id,
                                            budget,
                                            authorization,
                                        )
                                        .await;
                                    if budget_ok {
                                        budgets_updated += 1;
                                        covered.insert(lic.clone(), Some(budget));
                                    }
                                }
                            }
                            BudgetCase::Covered => {
                                // Already at the effective value, or at a monthly value billing
                                // would not disclose. No write; counted separately so the sweep
                                // cannot claim converging an estate it merely inspected.
                                already_covered += 1;
                            }
                            BudgetCase::Absent => {
                                uncovered_budget.push(lic.clone());
                                if !dry_run {
                                    budget_ok = billing
                                        .push_token_budget(
                                            license_id,
                                            tenant_id,
                                            budget,
                                            authorization,
                                        )
                                        .await;
                                    if budget_ok {
                                        budgets_created += 1;
                                        // Keep the cache truthful within this sweep so a second
                                        // licence cannot re-read a stale "absent".
                                        covered.insert(lic.clone(), Some(budget));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let mut quota_ok = true;
            if let Some(quota) = quota.map(i64::from) {
                // READ BEFORE WRITE, on a dry run too: measuring is what a dry run is FOR.
                let read = billing
                    .fetch_quota_config_monthly(license_id, plan_id, tenant_id, authorization)
                    .await;
                match read.state {
                    QuotaState::Unknown => {
                        // Fail CLOSED: nothing is written on a state we could not see. The
                        // reason is RECORDED, never branched on.
                        unknown_quota.push(lic.clone());
                        let reason = read
                            .unknown_reason
                            .unwrap_or_else(|| "unspecified".to_owned());
                        *unknown_quota_reasons.entry(reason).or_insert(0) += 1;
                        quota_ok = false;
                    }
                    QuotaState::Absent => {
                        uncovered_quota.push(lic.clone());
                        if !dry_run {
                            quota_ok = billing
                                .push_quota_config(
                                    license_id,
                                    tenant_id,
                                    quota,
                                    Some(app_id),
                                    Some(&app_name),
                                    authorization,
                                )
                                .await;
                            if quota_ok {
                                quotas_pushed += 1;
                            }
                        }
                    }
                    QuotaState::Present => {
                        let current = read.quota_monthly.unwrap_or(quota);
                        if current == quota {
                            quotas_already_configured += 1;
                        } else if force_plan_quota {
                            quota_drift.push(lic.clone());
                            eprintln!(
                                "reconcile-metering OVERWRITING an operator quota for \
                                 license={lic} tenant={tenant}: billing held quota_monthly={current}, \
                                 forcing the plan default {quota} because force_plan_quota=true."
                            );
                            if !dry_run {
                                quota_ok = billing
                                    .push_quota_config(
                                        license_id,
                                        tenant_id,
                                        quota,
                                        Some(app_id),
                                        Some(&app_name),
                                        authorization,
                                    )
                                    .await;
                                if quota_ok {
                                    quotas_overwritten += 1;
                                }
                            }
                        } else {
                            // DIVERGENCE, MADE VISIBLE — not resolved. The sweep cannot tell
                            // an operator override from a stale plan edit; the licence still
                            // converges, because a deliberate override is not a failure.
                            quota_drift.push(lic.clone());
                            eprintln!(
                                "reconcile-metering LEFT a divergent quota alone for \
                                 license={lic} tenant={tenant}: billing holds quota_monthly={current}, \
                                 the licence's effective quota is {quota}. Nothing was pushed."
                            );
                        }
                    }
                }
            }

            if dry_run {
                continue;
            }
            let stamped = if budget_ok && quota_ok {
                stamp_verified(pool, license_id).await
            } else {
                false
            };
            if stamped {
                repaired.push(lic);
            } else {
                failed.push(lic.clone());
                eprintln!(
                    "{METERING_FAILURE_MARKER} license={lic} tenant={tenant} budget_ok={budget_ok} \
                     quota_ok={quota_ok} — reconcile could not converge this licence; it remains \
                     (partly) unmetered"
                );
            }
        }
    }
    eprintln!(
        "reconcile-metering complete: scanned={} repaired(converged)={} failed={} \
         uncovered_budgets={} unknown_budgets={} budgets_created={} budget_drift={} \
         budgets_updated={} already_covered={} budgets_skipped_sentinel={} \
         budgets_skipped_money_tenant={} uncovered_quotas={} unknown_quotas={} \
         unknown_quota_reasons={} quota_drift={} quotas_pushed={} \
         quotas_already_configured={} quotas_overwritten={} dry_run={}",
        scanned,
        repaired.len(),
        failed.len(),
        uncovered_budget.len(),
        unknown_budget.len(),
        budgets_created,
        budget_drift.len(),
        budgets_updated,
        already_covered,
        budgets_skipped_sentinel,
        budgets_skipped_money_tenant,
        uncovered_quota.len(),
        unknown_quota.len(),
        python_dict(&unknown_quota_reasons),
        quota_drift.len(),
        quotas_pushed,
        quotas_already_configured,
        quotas_overwritten,
        dry_run
    );

    Ok(MeteringReconcileResult {
        scanned: scanned as i64,
        repaired: repaired.len() as i64,
        failed: failed.len() as i64,
        repaired_license_ids: repaired,
        failed_license_ids: failed,
        uncovered_budget_license_ids: uncovered_budget,
        unknown_budget_license_ids: unknown_budget,
        budgets_created,
        budget_drift_license_ids: budget_drift,
        budgets_updated,
        already_covered,
        budgets_skipped_sentinel,
        budgets_skipped_money_tenant,
        uncovered_quota_license_ids: uncovered_quota,
        quota_drift_license_ids: quota_drift,
        unknown_quota_license_ids: unknown_quota,
        unknown_quota_reasons,
        quotas_pushed,
        quotas_already_configured,
        quotas_overwritten,
        dry_run,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BudgetCase {
    Drift,
    Covered,
    Absent,
}

/// `_mark_metering_verified`: stamp `metering_verified_at` and NOTHING ELSE.
///
/// A zero-row update or a SQL failure is `false` — the licence goes to `failed`, which is what
/// the reference does. It deliberately does not touch `metering_provisioned_at`: that column was
/// baselined by migration 018 and is not a coverage signal.
async fn stamp_verified(pool: &PgPool, license_id: Uuid) -> bool {
    match sqlx::query("UPDATE license SET metering_verified_at = now() WHERE id = $1")
        .bind(license_id)
        .execute(pool)
        .await
    {
        Ok(done) => done.rows_affected() > 0,
        Err(error) => {
            eprintln!(
                "{METERING_FAILURE_MARKER}: could not record metering_verified_at for \
                 license={license_id} — the work may have SUCCEEDED but is not recorded: {error}"
            );
            false
        }
    }
}

/// The script's `_classify`, verbatim: `(exit_code, reason)`. Order matters — the unknown legs
/// are transport failures, then real failures, then the measure-only legs, then the two
/// anti-vacuity legs. `live` is the independent count taken BEFORE the sweep.
fn classify(result: &MeteringReconcileResult, limit: i64, live: i64) -> (i32, String) {
    if !result.unknown_budget_license_ids.is_empty() {
        return (
            EXIT_TRANSPORT,
            format!(
                "billing would not disclose coverage for {} licence(s); nothing was created for \
                 them and their coverage remains UNKNOWN",
                result.unknown_budget_license_ids.len()
            ),
        );
    }
    if !result.unknown_quota_license_ids.is_empty() {
        // The reason breakdown rides in the exit message because THIS STRING is what the
        // failing CronJob shows an operator.
        let hint = if result.unknown_quota_reasons.contains_key("foreign_tenant") {
            " foreign_tenant means billing holds a request_quota_config row under a DIFFERENT \
             tenant, so the licence's tenant of record has diverged: compare \
             licenses.license.tenant_id against billing.request_quota_config.tenant_id for those \
             license_ids."
        } else {
            ""
        };
        return (
            EXIT_TRANSPORT,
            format!(
                "billing would not disclose the configured quota for {} licence(s); nothing was \
                 pushed for them and their quota remains UNKNOWN. By cause: {}.{hint}",
                result.unknown_quota_license_ids.len(),
                python_dict(&result.unknown_quota_reasons)
            ),
        );
    }
    if !result.failed_license_ids.is_empty() {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "{} licence(s) could not be converged: {}",
                result.failed_license_ids.len(),
                python_list(&result.failed_license_ids)
            ),
        );
    }
    if result.dry_run && !result.uncovered_quota_license_ids.is_empty() {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "NOT CONVERGED (measure-only): {} of {} scanned licence(s) have no \
                 request_quota_config row in billing: {}. Re-run without RECONCILE_DRY_RUN to \
                 create them.",
                result.uncovered_quota_license_ids.len(),
                result.scanned,
                python_list(&result.uncovered_quota_license_ids)
            ),
        );
    }
    if result.dry_run && !result.uncovered_budget_license_ids.is_empty() {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "NOT CONVERGED (measure-only): {} of {} scanned licence(s) still have no active \
                 license_sync budget in billing: {}. A dry run writes nothing, so this is the \
                 coverage answer, not a list of what was attempted. Re-run without \
                 RECONCILE_DRY_RUN to repair them.",
                result.uncovered_budget_license_ids.len(),
                result.scanned,
                python_list(&result.uncovered_budget_license_ids)
            ),
        );
    }
    if result.scanned == 0 && live > 0 {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "VACUOUS SWEEP: scanned 0 licences while {live} live budget-owing licence(s) \
                 exist. A run that measures nothing must never report success — that is the \
                 stamp-read-as-a-measurement failure this job exists to prevent. Set \
                 RECONCILE_REVALIDATE=true so the sweep re-measures the estate against billing \
                 instead of trusting its own stamps."
            ),
        );
    }
    if result.scanned == limit && live > limit {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "TRUNCATED SWEEP: scanned exactly the limit ({limit}) while {live} live \
                 budget-owing licence(s) exist, so at least {} were never examined. Selection is \
                 ordered by created_at, so repeating this run re-measures the same oldest {limit} \
                 and never reaches the tail. Raise RECONCILE_LIMIT above {live}.",
                live - limit
            ),
        );
    }
    (EXIT_CONVERGED, "converged".to_owned())
}

/// The script's payload: the literals, then `**result.model_dump()`, sorted by the serialiser.
fn outcome(
    settings: &Settings,
    live_budget_owing: i64,
    result: MeteringReconcileResult,
) -> RunOutcome {
    let (code, reason) = classify(&result, settings.limit, live_budget_owing);
    let mut payload = Map::new();
    payload.insert("event".to_owned(), json!(EVENT));
    payload.insert("actor".to_owned(), json!(settings.actor));
    payload.insert("limit".to_owned(), json!(settings.limit));
    payload.insert("revalidate".to_owned(), json!(settings.revalidate));
    payload.insert(
        "force_plan_quota".to_owned(),
        json!(settings.force_plan_quota),
    );
    payload.insert("license_id".to_owned(), json!(settings.license_id));
    payload.insert(
        "live_budget_owing_licenses".to_owned(),
        json!(live_budget_owing),
    );
    payload.insert("exit_code".to_owned(), json!(code));
    payload.insert("reason".to_owned(), json!(reason));

    // `MeteringReconcileResult`, key for key.
    payload.insert("scanned".to_owned(), json!(result.scanned));
    payload.insert("repaired".to_owned(), json!(result.repaired));
    payload.insert("failed".to_owned(), json!(result.failed));
    payload.insert(
        "repaired_license_ids".to_owned(),
        json!(result.repaired_license_ids),
    );
    payload.insert(
        "failed_license_ids".to_owned(),
        json!(result.failed_license_ids),
    );
    payload.insert(
        "uncovered_budget_license_ids".to_owned(),
        json!(result.uncovered_budget_license_ids),
    );
    payload.insert(
        "unknown_budget_license_ids".to_owned(),
        json!(result.unknown_budget_license_ids),
    );
    payload.insert("budgets_created".to_owned(), json!(result.budgets_created));
    payload.insert(
        "budget_drift_license_ids".to_owned(),
        json!(result.budget_drift_license_ids),
    );
    payload.insert("budgets_updated".to_owned(), json!(result.budgets_updated));
    payload.insert("already_covered".to_owned(), json!(result.already_covered));
    payload.insert(
        "budgets_skipped_sentinel".to_owned(),
        json!(result.budgets_skipped_sentinel),
    );
    payload.insert(
        "budgets_skipped_money_tenant".to_owned(),
        json!(result.budgets_skipped_money_tenant),
    );
    payload.insert("quotas_pushed".to_owned(), json!(result.quotas_pushed));
    payload.insert(
        "uncovered_quota_license_ids".to_owned(),
        json!(result.uncovered_quota_license_ids),
    );
    payload.insert(
        "quota_drift_license_ids".to_owned(),
        json!(result.quota_drift_license_ids),
    );
    payload.insert(
        "unknown_quota_license_ids".to_owned(),
        json!(result.unknown_quota_license_ids),
    );
    payload.insert(
        "unknown_quota_reasons".to_owned(),
        json!(result.unknown_quota_reasons),
    );
    payload.insert(
        "quotas_already_configured".to_owned(),
        json!(result.quotas_already_configured),
    );
    payload.insert(
        "quotas_overwritten".to_owned(),
        json!(result.quotas_overwritten),
    );
    payload.insert("dry_run".to_owned(), json!(result.dry_run));
    RunOutcome {
        code,
        payload: Value::Object(payload),
    }
}

/// The script's `_fail`: one JSON line, exit 2, a STATIC `error` (never a database exception or
/// DSN — that line is what gets shipped, pasted and alerted on).
fn failure(
    actor: Option<&str>,
    license_id: Option<&str>,
    error: &str,
    error_kind: &str,
) -> RunOutcome {
    RunOutcome {
        code: EXIT_TRANSPORT,
        payload: json!({
            "event": EVENT,
            "actor": actor,
            "license_id": license_id,
            "error": error,
            "exit_code": EXIT_TRANSPORT,
            "error_kind": error_kind,
        }),
    }
}

/// Python's list repr for the reason strings: `['id', 'id']`.
fn python_list(ids: &[String]) -> String {
    let items: Vec<String> = ids.iter().map(|id| format!("'{id}'")).collect();
    format!("[{}]", items.join(", "))
}

/// Python's sorted-dict repr: `{'foreign_tenant': 1, 'http_500': 2}`.
fn python_dict(counts: &BTreeMap<String, i64>) -> String {
    let items: Vec<String> = counts
        .iter()
        .map(|(reason, count)| format!("'{reason}': {count}"))
        .collect();
    format!("{{{}}}", items.join(", "))
}

/// `_env_bool`: STRICT. A typo raises instead of quietly meaning false, and a BLANK value means
/// "unset" — i.e. the default.
fn env_bool<F: Fn(&str) -> Option<String>>(
    lookup: &F,
    name: &str,
    default: bool,
) -> Result<bool, String> {
    let Some(raw) = lookup(name) else {
        return Ok(default);
    };
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(default);
    }
    if matches!(value.as_str(), "1" | "true" | "yes" | "on") {
        return Ok(true);
    }
    if matches!(value.as_str(), "0" | "false" | "no" | "off") {
        return Ok(false);
    }
    Err(format!(
        "{name}='{raw}' is not a boolean (use one of \
         ['', '0', '1', 'false', 'no', 'off', 'on', 'true', 'yes'])"
    ))
}

/// `_env_int`: absent and blank read as the default; a non-integer or an out-of-bounds value
/// refuses.
fn env_int<F: Fn(&str) -> Option<String>>(
    lookup: &F,
    name: &str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, String> {
    let Some(raw) = lookup(name) else {
        return Ok(default);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    let value: i64 = trimmed
        .parse()
        .map_err(|_| format!("{name}='{raw}' is not an integer"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{name}={value} is outside {minimum}..{maximum}"));
    }
    Ok(value)
}

/// `_env_str`: a trimmed value, or `None` when unset OR blank.
fn env_str<F: Fn(&str) -> Option<String>>(lookup: &F, name: &str) -> Option<String> {
    lookup(name)
        .map(|raw| raw.trim().to_owned())
        .filter(|value| !value.is_empty())
}
