//! `reap-expired`: archive terminally-expired licences to `status='revoked'`, estate-wide, and
//! tear down billing's two mirrors for each archived licence.
//!
//! Ported from `scripts/reap_expired_licences.py` (licenses @ 4e8a08d). The contract is the
//! script's, not a redesign:
//!
//! - configuration is ALL environment (`REAP_DRY_RUN`, `REAP_LIMIT`, `REAP_GRACE_DAYS`,
//!   `REAP_ACTOR`, plus the process's `DATABASE_URL` / `METERING_SERVICE_URL`), so a CronJob
//!   manifest stays the only knob and the command takes no arguments;
//! - ONE JSON line on stdout on EVERY path, config failures included — a CronJob pod is not
//!   scraped and there is no audit table, so `kubectl logs` IS the audit trail;
//! - exit 0 converged, 1 not converged, 2 config/DB failure.
//!
//! ⚠️ It ships DRY-RUN, deliberately: `REAP_DRY_RUN` defaults to TRUE and a BLANK value reads as
//! the default, so a `REAP_DRY_RUN: ""` manifest key cannot arm the sweep by accident. Arming is
//! an explicit `REAP_DRY_RUN=false`.
//!
//! THE PREDICATE is the reference's exactly:
//! `status = 'expired' AND expires_at IS NOT NULL AND expires_at < cutoff`, where
//! `cutoff = now - REAP_GRACE_DAYS` is computed ONCE and reused for the count before the sweep,
//! the sweep itself and the recount after it. `starts_at` is ABSENT (irrelevant to an already
//! terminal row), `<` is not `<=`, and a NULL `expires_at` means PERPETUAL — never swept.
//!
//! THE GRACE WINDOW is load-bearing: `'expired'` means "your licence lapsed, renew it" while
//! `'revoked'` means "somebody killed it". Without a grace window the daily chain would erase
//! that distinction overnight, so only rows expired LONGER than `REAP_GRACE_DAYS` are archived.
//! `REAP_GRACE_DAYS=0` reduces this to the token reaper's behaviour.
//!
//! ⚠️ THE ANTI-VACUITY GUARDS are the point of this command, not decoration. A sweep's happy
//! path is "found nothing", which is also what a broken selection and a dry run over a latched
//! estate look like. `reapable_before` comes from HAND-WRITTEN SQL (`models.py::
//! _REAPABLE_LICENSE_COUNT_SQL`), deliberately not the sweep's own query, which could only ever
//! agree with itself; `reapable_after` is the same count after the sweep. The `reason` strings
//! name each way of being green about nothing: OVER-FIRED, DRY RUN WROTE, VACUOUS SWEEP,
//! TRUNCATED SWEEP, NOT CONVERGED, the skipped-teardown manifest defect and BILLING RESIDUE.
//!
//! ⚠️ THE BILLING TEARDOWN RUNS AFTER THE ROWS ARE COMMITTED and NEVER changes the reap verdict;
//! it only adds fields to the payload (unconditionally, on every path) and can only push the run
//! to exit 1. A dry run REPORTS the pairs it would call billing about and puts nothing on the
//! wire. `METERING_SERVICE_URL` unset maps to `teardown_mode: "skipped"` — the rows ARE archived
//! and their budgets ARE still live, so an armed sweep that skipped the teardown is NOT
//! converged. The reference distinguishes "unset" from "set but unusable"; this port builds the
//! client with [`BillingTeardown::from_env`], which returns `None` for both, so an unusable URL
//! is reported as `skipped` rather than as residue. That is the one deliberate divergence.
//!
//! ⚠️ ARGUMENTS: the reference's `main_cli` rejects any argv with a JSON failure line. Clap
//! rejects them first here (usage on stderr, no stdout line, still exit 2), so the CronJob's
//! no-argument invocation — the only one that exists — is byte-for-byte the reference's.

use std::collections::HashMap;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tokio::task::JoinSet;
use uuid::Uuid;

use super::RunOutcome;
use crate::billing::BillingTeardown;

/// The event name the script reports in every payload.
const EVENT: &str = "reap_expired_licenses";
/// The script's `DEFAULT_ACTOR`.
const DEFAULT_ACTOR: &str = "cronjob:reap-expired-licences";
/// ONE greppable line for the one thing this job cannot report as either success or failure.
const TEARDOWN_SKIPPED_MARKER: &str = "REAP_TEARDOWN_SKIPPED_UNCONFIGURED";
/// The script's `EXIT_CONVERGED` / `EXIT_NOT_CONVERGED` / `EXIT_FAILURE`.
const EXIT_CONVERGED: i32 = 0;
const EXIT_NOT_CONVERGED: i32 = 1;
const EXIT_FAILURE: i32 = 2;
/// `_env_int("REAP_LIMIT", 500, minimum=1, maximum=5000)`.
const DEFAULT_LIMIT: i64 = 500;
const MIN_LIMIT: i64 = 1;
const MAX_LIMIT: i64 = 5000;
/// `_env_int("REAP_GRACE_DAYS", 30, minimum=0, maximum=3650)`.
const DEFAULT_GRACE_DAYS: i64 = 30;
const MIN_GRACE_DAYS: i64 = 0;
const MAX_GRACE_DAYS: i64 = 3650;
/// How many licences may have their billing mirrors torn down concurrently — the reference's
/// `TEARDOWN_CONCURRENCY`, neither 1 (an hour-long sweep) nor `REAP_LIMIT` (an outage).
const TEARDOWN_CONCURRENCY: usize = 4;

/// `_db_manager`'s refusal, verbatim — it names the Secret, never a DSN.
const DATABASE_URL_REQUIRED: &str =
    "DATABASE_URL is required (it lives in the licenses-env Secret)";
/// `main_cli`'s generic-exception error: STATIC, never a database exception or DSN.
const SWEEP_FAILED: &str = "the sweep could not complete; see the traceback on stderr";
/// `type(e).__name__` is a Python class; this is its stable Rust counterpart.
const DB_ERROR_KIND: &str = "sqlx::Error";
/// The `error_kind` the script attaches to every configuration refusal.
const BAD_ENV_KIND: &str = "bad_reap_env";

/// The independent second opinion: `models.py::_REAPABLE_LICENSE_COUNT_SQL`, verbatim. Its `$1`
/// is the ONCE-captured `cutoff`, never `now()`.
const REAPABLE_COUNT_SQL: &str = "SELECT count(*) FROM license \
     WHERE status = 'expired' AND expires_at IS NOT NULL AND expires_at < $1";

/// `models.py::reap_all_expired_licenses`'s ORM query in its own words: oldest first, limited.
const SELECT_REAPABLE_SQL: &str = "SELECT id FROM license \
     WHERE status = 'expired' AND expires_at IS NOT NULL AND expires_at < $1 \
     ORDER BY expires_at ASC, id ASC LIMIT $2";
/// Armed only: the same selection, locked.
const SELECT_REAPABLE_FOR_UPDATE_SQL: &str = "SELECT id, status FROM license \
     WHERE status = 'expired' AND expires_at IS NOT NULL AND expires_at < $1 \
     ORDER BY expires_at ASC, id ASC LIMIT $2 FOR UPDATE";
/// The sweep's bulk UPDATE, keyed on exactly the ids just locked. `revoked_at` is written ONLY
/// IF UNSET (it is the audit record of when the credential was killed; a second sweep must not
/// rewrite it), and `AND status = 'expired'` is the script's explicit re-qualification under
/// READ COMMITTED.
const REAP_IDS_SQL: &str = "UPDATE license SET status = 'revoked', \
     revoked_at = COALESCE(revoked_at, $1), updated_at = $1 \
     WHERE id = ANY($2) AND status = 'expired'";
/// `models.py::tenants_for_licenses`: the tenant billing knows each licence by, read from the
/// licence's OWN row after the archive committed.
const TENANTS_FOR_LICENSES_SQL: &str = "SELECT id, tenant_id FROM license WHERE id = ANY($1)";

/// The four environment knobs, resolved once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub actor: String,
    pub dry_run: bool,
    pub limit: i64,
    pub grace_days: i64,
}

/// A malformed `REAP_*` value. The script parses `REAP_LIMIT`, `REAP_GRACE_DAYS` and
/// `REAP_DRY_RUN` before it ever reads `REAP_ACTOR`, so a bad knob leaves the payload's `actor`
/// NULL — reproduced here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub actor: Option<String>,
    pub error: String,
}

impl ConfigError {
    /// The script's `_fail(actor, str(e), error_kind="bad_reap_env")`, as an outcome.
    pub fn outcome(&self) -> RunOutcome {
        failure(self.actor.as_deref(), &self.error, BAD_ENV_KIND)
    }
}

impl Settings {
    /// Read `REAP_LIMIT`, `REAP_GRACE_DAYS`, `REAP_DRY_RUN` and `REAP_ACTOR` from the process
    /// environment, in the script's order.
    pub fn from_env() -> Result<Settings, ConfigError> {
        Settings::from_lookup(|name| std::env::var(name).ok())
    }

    /// The testable core of `from_env`: the environment is supplied instead of read.
    pub fn from_lookup<F: Fn(&str) -> Option<String>>(lookup: F) -> Result<Settings, ConfigError> {
        let limit = env_int(&lookup, "REAP_LIMIT", DEFAULT_LIMIT, MIN_LIMIT, MAX_LIMIT)
            .map_err(|error| ConfigError { actor: None, error })?;
        let grace_days = env_int(
            &lookup,
            "REAP_GRACE_DAYS",
            DEFAULT_GRACE_DAYS,
            MIN_GRACE_DAYS,
            MAX_GRACE_DAYS,
        )
        .map_err(|error| ConfigError { actor: None, error })?;
        // SHIPS DRY-RUN: arming is an explicit REAP_DRY_RUN=false.
        let dry_run = env_bool(&lookup, "REAP_DRY_RUN", true)
            .map_err(|error| ConfigError { actor: None, error })?;
        let actor = env_str(&lookup, "REAP_ACTOR").unwrap_or_else(|| DEFAULT_ACTOR.to_owned());
        Ok(Settings {
            actor,
            dry_run,
            limit,
            grace_days,
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
    match Settings::from_lookup(lookup) {
        // THE SEAM the reference's tests replace: the billing client is resolved once, here.
        Ok(settings) => run_with(database_url, settings, BillingTeardown::from_env()).await,
        Err(bad) => {
            eprintln!("reap-expired: refusing to run: {}", bad.error);
            bad.outcome()
        }
    }
}

/// The testable core: the knobs AND the billing client are supplied instead of resolved, so a
/// test can drive the stub without a network and without this process's environment. `None`
/// billing is `METERING_SERVICE_URL` unset — the reference's `teardown_mode: "skipped"`.
pub async fn run_with(
    database_url: Option<String>,
    settings: Settings,
    billing: Option<BillingTeardown>,
) -> RunOutcome {
    let Settings {
        actor,
        dry_run,
        limit,
        grace_days,
    } = settings;
    let Some(url) = database_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
    else {
        return failure(Some(&actor), DATABASE_URL_REQUIRED, BAD_ENV_KIND);
    };

    // ONE instant, ONE cutoff, for the count, the sweep and the recount — see the module docs.
    let now = Utc::now();
    let cutoff = now - Duration::days(grace_days);
    match sweep_and_teardown(&url, now, cutoff, grace_days, limit, dry_run, billing).await {
        Ok((sweep, teardown)) => outcome(&actor, sweep, teardown),
        Err(error) => {
            // The exception goes to stderr; stdout keeps the one static JSON line.
            eprintln!("reap-expired: the sweep could not complete: {error}");
            failure(Some(&actor), SWEEP_FAILED, DB_ERROR_KIND)
        }
    }
}

/// `reapable_before` → sweep → `reapable_after` → billing teardown, exactly the script's order.
async fn sweep_and_teardown(
    url: &str,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    grace_days: i64,
    limit: i64,
    dry_run: bool,
    billing: Option<BillingTeardown>,
) -> Result<(Sweep, Value), sqlx::Error> {
    let pool = crate::db::connect(url, 1).await?;
    let before = count_reapable(&pool, cutoff).await?;
    let selected = sweep(&pool, now, cutoff, limit, dry_run).await?;
    let after = count_reapable(&pool, cutoff).await?;
    // AFTER the rows are committed, exactly like the HTTP revoke path: revocation is the
    // security-critical half and a billing outage must never delay or veto it. Never raises.
    let teardown = teardown_billing_mirrors(&pool, &selected, dry_run, billing.as_ref()).await;
    pool.close().await;
    Ok((
        Sweep {
            dry_run,
            limit,
            grace_days,
            now,
            cutoff,
            before,
            selected,
            after,
        },
        teardown,
    ))
}

/// One sweep's facts, as `_classify` and the payload need them.
struct Sweep {
    dry_run: bool,
    limit: i64,
    grace_days: i64,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    before: i64,
    selected: Vec<Uuid>,
    after: i64,
}

async fn count_reapable(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(REAPABLE_COUNT_SQL)
        .bind(cutoff)
        .fetch_one(pool)
        .await
}

/// Select the oldest reapable licences; write only when armed. A dry run takes no locks, writes
/// nothing, and returns the ids it WOULD have archived.
async fn sweep(
    pool: &PgPool,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    limit: i64,
    dry_run: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if dry_run {
        // Read-only: no `FOR UPDATE`, no UPDATE, and the read transaction is discarded.
        let ids: Vec<Uuid> = sqlx::query_scalar(SELECT_REAPABLE_SQL)
            .bind(cutoff)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?;
        tx.rollback().await?;
        return Ok(ids);
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(SELECT_REAPABLE_FOR_UPDATE_SQL)
        .bind(cutoff)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
    // The script's explicit re-check: a row a concurrent writer already archived is skipped.
    let ids: Vec<Uuid> = rows
        .into_iter()
        .filter(|(_, status)| status == "expired")
        .map(|(id, _)| id)
        .collect();
    sqlx::query(REAP_IDS_SQL)
        .bind(now)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ids)
}

// ---------------------------------------------------------------------------------------------
// The billing teardown: `_teardown_billing_mirrors`, ported verbatim. It NEVER raises — the rows
// are already committed, so a billing problem must be REPORTED, never allowed to turn a
// successful archive into an unexplained exit 2.
// ---------------------------------------------------------------------------------------------

/// The teardown fields every audit line carries, in their nothing-happened state.
#[derive(Debug, Clone)]
struct TeardownSummary {
    mode: &'static str,
    attempted: usize,
    quota_failed: Vec<String>,
    budget_failed: Vec<String>,
    unresolved_tenant: Vec<String>,
    would_call: Option<Vec<Value>>,
}

impl TeardownSummary {
    fn nothing() -> Self {
        Self {
            mode: "not_applicable",
            attempted: 0,
            quota_failed: Vec::new(),
            budget_failed: Vec::new(),
            unresolved_tenant: Vec::new(),
            would_call: None,
        }
    }

    fn into_value(self) -> Value {
        let mut map = Map::new();
        map.insert("teardown_mode".to_owned(), json!(self.mode));
        map.insert("teardown_attempted".to_owned(), json!(self.attempted));
        map.insert("teardown_quota_failed".to_owned(), json!(self.quota_failed));
        map.insert("teardown_budget_failed".to_owned(), json!(self.budget_failed));
        map.insert(
            "teardown_unresolved_tenant".to_owned(),
            json!(self.unresolved_tenant),
        );
        if let Some(would_call) = self.would_call {
            map.insert("teardown_would_call".to_owned(), Value::Array(would_call));
        }
        Value::Object(map)
    }
}

async fn teardown_billing_mirrors(
    pool: &PgPool,
    license_ids: &[Uuid],
    dry_run: bool,
    billing: Option<&BillingTeardown>,
) -> Value {
    // Nothing was selected, so there is nothing to tear down — and no tenant read is needed.
    if license_ids.is_empty() {
        return TeardownSummary::nothing().into_value();
    }

    // ⛔ A licence whose `tenant_id` could not be resolved is NEVER torn down under a guessed
    // tenant. Resolving the tenants is a database read, so its failure is the reference's
    // "could not run at all" case: every archived licence is reported as residue on BOTH halves,
    // because this cannot know which calls, if any, got out.
    let tenants = match tenants_for(pool, license_ids).await {
        Ok(tenants) => tenants,
        Err(error) => {
            eprintln!(
                "reap-expired: the billing teardown could not run at all for this sweep — the \
                 {} archived licence(s) keep their quota mirrors and their license_sync token \
                 budgets: {error}",
                license_ids.len()
            );
            let failed = sorted_ids(license_ids);
            let mut summary = TeardownSummary::nothing();
            summary.mode = "error";
            summary.quota_failed = failed.clone();
            summary.budget_failed = failed;
            return summary.into_value();
        }
    };

    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    for id in license_ids {
        match tenants.get(id) {
            Some(tenant) => pairs.push((id.to_string(), tenant.clone())),
            None => unresolved.push(id.to_string()),
        }
    }
    unresolved.sort();

    let mut summary = TeardownSummary::nothing();
    summary.unresolved_tenant = unresolved;

    if dry_run {
        // REPORT, DO NOT PERFORM. A dry run that put a DELETE on the wire would not be a dry run.
        summary.mode = "dry_run";
        pairs.sort();
        summary.would_call = Some(
            pairs
                .into_iter()
                .map(|(license_id, tenant_id)| {
                    json!({"license_id": license_id, "tenant_id": tenant_id})
                })
                .collect(),
        );
        return summary.into_value();
    }

    let Some(billing) = billing else {
        // The rows ARE archived and their budgets ARE still live: loud, greppable, and NOT a
        // billing verdict — nothing was attempted.
        summary.mode = "skipped";
        eprintln!(
            "{TEARDOWN_SKIPPED_MARKER}: archived {} licence(s) with METERING_SERVICE_URL unset, \
             so billing was never told: their quota mirrors and their license_sync token budgets \
             are STILL LIVE and nothing on billing's side runs on a timer to notice. This is a \
             CronJob manifest defect, not a billing fault — give the job METERING_SERVICE_URL. \
             Affected: {}",
            license_ids.len(),
            python_list(&sorted_ids(license_ids))
        );
        return summary.into_value();
    };

    summary.mode = "performed";
    summary.attempted = pairs.len();
    let (quota_failed, budget_failed) = run_teardowns(billing, &pairs).await;
    summary.quota_failed = quota_failed;
    summary.budget_failed = budget_failed;
    summary.into_value()
}

/// `models.py::tenants_for_licenses`: licence id → the tenant STORED ON ITS OWN ROW. A missing
/// row is simply absent; the caller reports that as unresolvable, never as "nothing to do".
async fn tenants_for(
    pool: &PgPool,
    license_ids: &[Uuid],
) -> Result<HashMap<Uuid, String>, sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(TENANTS_FOR_LICENSES_SQL)
        .bind(license_ids)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, tenant)| (id, tenant.to_string()))
        .collect())
}

/// Every licence's two halves, at most [`TEARDOWN_CONCURRENCY`] licences in flight. Quota and
/// budget go out CONCURRENTLY and are graded SEPARATELY — a quota config caps request RATE, a
/// token budget caps SPEND, and one failing must never suppress the other's attempt.
async fn run_teardowns(
    billing: &BillingTeardown,
    pairs: &[(String, String)],
) -> (Vec<String>, Vec<String>) {
    let mut set: JoinSet<(String, bool, bool)> = JoinSet::new();
    let mut in_flight: HashMap<tokio::task::Id, String> = HashMap::new();
    let mut queue = pairs.iter().cloned();
    let mut quota_failed: Vec<String> = Vec::new();
    let mut budget_failed: Vec<String> = Vec::new();

    for _ in 0..TEARDOWN_CONCURRENCY {
        let Some(pair) = queue.next() else { break };
        spawn_teardown(&mut set, &mut in_flight, billing, pair);
    }
    while let Some(joined) = set.join_next_with_id().await {
        match joined {
            Ok((task_id, (license_id, quota_ok, budget_ok))) => {
                in_flight.remove(&task_id);
                if !quota_ok {
                    quota_failed.push(license_id.clone());
                }
                if !budget_ok {
                    budget_failed.push(license_id);
                }
            }
            Err(error) => {
                // `return_exceptions=True` belt-and-braces: an escaped failure says nothing
                // about which half ran, so it is residue on BOTH. `unknown` is reported as
                // unknown and graded as failure, never as "nothing to do".
                if let Some(license_id) = in_flight.remove(&error.id()) {
                    eprintln!(
                        "reap-expired: a billing teardown task failed for licence {license_id}: \
                         {error}; treating as residue"
                    );
                    quota_failed.push(license_id.clone());
                    budget_failed.push(license_id);
                }
            }
        }
        if let Some(pair) = queue.next() {
            spawn_teardown(&mut set, &mut in_flight, billing, pair);
        }
    }
    quota_failed.sort();
    budget_failed.sort();
    (quota_failed, budget_failed)
}

fn spawn_teardown(
    set: &mut JoinSet<(String, bool, bool)>,
    in_flight: &mut HashMap<tokio::task::Id, String>,
    billing: &BillingTeardown,
    (license_id, tenant_id): (String, String),
) {
    let billing = billing.clone();
    let tracked = license_id.clone();
    let handle = set.spawn(async move {
        let (quota_ok, budget_ok) = billing.teardown(&tenant_id, &license_id).await;
        (license_id, quota_ok, budget_ok)
    });
    in_flight.insert(handle.id(), tracked);
}

// ---------------------------------------------------------------------------------------------
// The verdict and the audit line.
// ---------------------------------------------------------------------------------------------

/// The script's `_classify`, verbatim: `(exit_code, reason)`. Order matters — the over-fire and
/// dry-run-wrote legs describe a broken sweep, so they come first; the residue leg is last but
/// its fields are on the payload unconditionally, so it is never invisible.
fn classify(sweep: &Sweep, teardown: &Value) -> (i32, String) {
    let before = sweep.before;
    let after = sweep.after;
    let selected_len = sweep.selected.len();
    let selected_repr = python_list(&sweep.selected.iter().map(Uuid::to_string).collect::<Vec<_>>());

    if selected_len as i64 > before {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "OVER-FIRED: the sweep selected {selected_len} licence(s) but only {before} are \
                 reapable by the independent rule (status='expired' AND expires_at IS NOT NULL \
                 AND expires_at < now-{}d). Both are evaluated against the same cutoff, so this \
                 is predicate drift — a widened selection archives live or perpetual licences \
                 and every row it touched looks like a success. Selected: {selected_repr}",
                sweep.grace_days
            ),
        );
    }
    if sweep.dry_run && after != before {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "DRY RUN WROTE: the reapable count moved from {before} to {after} during a run \
                 that is defined to write nothing. Do not trust this run, and do not arm this \
                 job until that is explained."
            ),
        );
    }
    if sweep.dry_run && before > 0 {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "NOT CONVERGED (measure-only): {before} licence(s) are status='expired' with an \
                 expiry older than the {}-day grace window, and a dry run writes nothing, so all \
                 of them remain — each one latching \
                 nonagentic_license_expired_rows{{kind=\"license\",revoked=\"false\"}} and \
                 pinning LicenseAlreadyExpired. This is the answer about the estate, not a list \
                 of failures. Would archive: {selected_repr}. Re-run with REAP_DRY_RUN=false to \
                 archive them.",
                sweep.grace_days
            ),
        );
    }
    if !sweep.dry_run && selected_len == 0 && before > 0 {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "VACUOUS SWEEP: archived 0 licence(s) while {before} are reapable by the \
                 independent rule. A run that changes nothing over a latched estate must never \
                 report success — the selection is not doing what this job claims."
            ),
        );
    }
    if !sweep.dry_run && after > 0 {
        if selected_len as i64 == sweep.limit {
            return (
                EXIT_NOT_CONVERGED,
                format!(
                    "TRUNCATED SWEEP: archived exactly the limit ({}) and {after} reapable \
                     licence(s) remain. Repeating this schedule chews through the backlog {} at \
                     a time, but until it drains the gauge stays latched. Raise REAP_LIMIT above \
                     {before}.",
                    sweep.limit, sweep.limit
                ),
            );
        }
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "NOT CONVERGED: {after} reapable licence(s) remain after archiving \
                 {selected_len} — under the limit ({}), so they were selected and not written, \
                 or were created during the sweep.",
                sweep.limit
            ),
        );
    }
    // BEFORE the residue leg: residue names a teardown that RAN and was refused, this names one
    // that never ran at all. A skipped run can also carry `teardown_unresolved_tenant`, so
    // without this leg first the residue reason would blame billing for a missing env var.
    if !sweep.dry_run
        && selected_len > 0
        && teardown.get("teardown_mode").and_then(Value::as_str) == Some("skipped")
    {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "{TEARDOWN_SKIPPED_MARKER}: archived {selected_len} licence(s) with \
                 METERING_SERVICE_URL unset, so billing was never told — every one of them keeps \
                 its quota mirror and its license_sync token budget, and a revoked licence with \
                 a live token budget is SPENDABLE. This run did HALF its job and must not report \
                 convergence for the half it skipped: armed on a pod without that variable it \
                 would otherwise be green every night while manufacturing exactly the orphaned \
                 budgets it exists to prevent (19 such rows / 430M tokens were measured on \
                 production 2026-09-09). This is a CronJob MANIFEST defect — give the job \
                 METERING_SERVICE_URL; nothing on billing's side runs on a timer to notice. \
                 Affected: {selected_repr}"
            ),
        );
    }
    let quota_failed = string_list(teardown, "teardown_quota_failed");
    let budget_failed = string_list(teardown, "teardown_budget_failed");
    let unresolved = string_list(teardown, "teardown_unresolved_tenant");
    if !sweep.dry_run && (!quota_failed.is_empty() || !budget_failed.is_empty() || !unresolved.is_empty())
    {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "BILLING RESIDUE: the licence rows ARE archived, but billing still holds a \
                 mirror for some of them — quota teardown failed for {}, token budget teardown \
                 failed for {}, tenant unresolvable for {}. A revoked licence with a live \
                 license_sync token budget is SPENDABLE, and billing's own reconciler \
                 (POST /api/budget/sync-from-licenses) runs on NO timer, so this does not heal \
                 on its own. The per-licence cause is on stderr under \
                 QUOTA_CONFIG_TEARDOWN_FAILED / TOKEN_BUDGET_TEARDOWN_FAILED (billing refused \
                 us or was unreachable) or ..._ABANDONED (this pod was terminated mid-call — \
                 NOT a billing fault).",
                python_list(&quota_failed),
                python_list(&budget_failed),
                python_list(&unresolved)
            ),
        );
    }
    (EXIT_CONVERGED, "converged".to_owned())
}

/// The script's success payload, key for key. `selected` is what the sweep picked; `archived`
/// is what it WROTE, zero on a dry run BY DEFINITION. The teardown fields are merged
/// unconditionally, before `exit_code`/`reason`.
fn outcome(actor: &str, sweep: Sweep, teardown: Value) -> RunOutcome {
    let (code, reason) = classify(&sweep, &teardown);
    let mut payload = Map::new();
    payload.insert("event".to_owned(), json!(EVENT));
    payload.insert("actor".to_owned(), json!(actor));
    payload.insert("dry_run".to_owned(), json!(sweep.dry_run));
    payload.insert("limit".to_owned(), json!(sweep.limit));
    payload.insert("grace_days".to_owned(), json!(sweep.grace_days));
    payload.insert(
        "now".to_owned(),
        json!(sweep.now.to_rfc3339_opts(SecondsFormat::Micros, false)),
    );
    payload.insert(
        "cutoff".to_owned(),
        json!(sweep.cutoff.to_rfc3339_opts(SecondsFormat::Micros, false)),
    );
    payload.insert("reapable_before".to_owned(), json!(sweep.before));
    payload.insert("reapable_after".to_owned(), json!(sweep.after));
    payload.insert("selected".to_owned(), json!(sweep.selected.len()));
    payload.insert(
        "selected_license_ids".to_owned(),
        json!(sweep
            .selected
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()),
    );
    payload.insert(
        "archived".to_owned(),
        json!(if sweep.dry_run { 0 } else { sweep.selected.len() }),
    );
    if let Value::Object(teardown) = teardown {
        payload.extend(teardown);
    }
    payload.insert("exit_code".to_owned(), json!(code));
    payload.insert("reason".to_owned(), json!(reason));
    RunOutcome {
        code,
        payload: Value::Object(payload),
    }
}

/// The script's `_fail`: one JSON line, exit 2, a STATIC `error` (never a database exception or
/// DSN — that line is what gets shipped, pasted and alerted on).
fn failure(actor: Option<&str>, error: &str, error_kind: &str) -> RunOutcome {
    RunOutcome {
        code: EXIT_FAILURE,
        payload: json!({
            "event": EVENT,
            "actor": actor,
            "error": error,
            "exit_code": EXIT_FAILURE,
            "error_kind": error_kind,
        }),
    }
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn sorted_ids(license_ids: &[Uuid]) -> Vec<String> {
    let mut ids: Vec<String> = license_ids.iter().map(Uuid::to_string).collect();
    ids.sort();
    ids
}

/// Python's list repr for the reason strings: `['id', 'id']`.
fn python_list(ids: &[String]) -> String {
    let items: Vec<String> = ids.iter().map(|id| format!("'{id}'")).collect();
    format!("[{}]", items.join(", "))
}

/// `_env_bool`: STRICT. A typo raises instead of quietly meaning false, and a BLANK value means
/// "unset" — i.e. the default — so `REAP_DRY_RUN: ""` cannot arm the job.
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
