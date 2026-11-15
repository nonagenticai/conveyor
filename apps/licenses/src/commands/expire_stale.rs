//! `expire-stale`: retire time-expired-but-still-`status='active'` licences, estate-wide.
//!
//! Ported from `scripts/expire_stale_licences.py` (licenses @ 4e8a08d). The contract is the
//! script's, not a redesign:
//!
//! - configuration is ALL environment (`EXPIRE_DRY_RUN`, `EXPIRE_LIMIT`, `EXPIRE_ACTOR`,
//!   `DATABASE_URL`), so a CronJob manifest stays the only knob;
//! - ONE JSON line on stdout on EVERY path, config failures included;
//! - exit 0 converged, 1 not converged, 2 config/DB failure.
//!
//! ⚠️ It ships DRY-RUN, deliberately: `EXPIRE_DRY_RUN` defaults to TRUE and a BLANK value reads
//! as the default, so an `EXPIRE_DRY_RUN: ""` manifest key cannot arm the sweep by accident.
//! Arming is an explicit `EXPIRE_DRY_RUN=false`.
//!
//! ⚠️ The anti-vacuity guards are the point of this command, not decoration. A sweep whose happy
//! path is "found nothing" cannot tell a clean estate from a broken selection, so:
//!
//! - a dry run that found stale rows is NOT converged (it wrote nothing, so they remain);
//! - an armed run that transitioned nothing while stale rows exist is a broken selection;
//! - an armed run that left rows behind outgrew `EXPIRE_LIMIT` (truncation);
//! - a sweep that selected MORE rows than the independent rule calls stale over-fired.
//!
//! The count is HAND-WRITTEN SQL on purpose (`models.py::_STALE_LICENSE_COUNT_SQL`), never the
//! sweep's own selection: a count reusing the sweep query could only ever agree with itself.
//! Count, sweep and recount are all evaluated at ONE captured instant, so a licence crossing its
//! `expires_at` mid-run, or clock skew between this process and postgres, cannot be mistaken for
//! predicate drift.
//!
//! ⚠️ The predicate is the reference's exactly: `status = 'active' AND expires_at IS NOT NULL
//! AND expires_at < now`. `starts_at` is ABSENT (a future-dated row is genuinely the licence that
//! key will have), and `<` is not `<=`. There is deliberately NO `deleted_at` leg: the reference
//! does not consult it, and a soft delete sets `status='revoked'` to free the partial unique
//! index, so soft-deleted rows are already excluded by the status leg. NULL `expires_at` means
//! PERPETUAL and must never be swept.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::RunOutcome;

/// The event name the script reports in every payload.
const EVENT: &str = "expire_stale_licenses";
/// The script's `DEFAULT_ACTOR`.
const DEFAULT_ACTOR: &str = "cronjob:expire-stale-licences";
/// The script's `EXIT_CONVERGED` / `EXIT_NOT_CONVERGED` / `EXIT_FAILURE`.
const EXIT_CONVERGED: i32 = 0;
const EXIT_NOT_CONVERGED: i32 = 1;
const EXIT_FAILURE: i32 = 2;
/// `_env_int("EXPIRE_LIMIT", 500, minimum=1, maximum=5000)`.
const MIN_LIMIT: i64 = 1;
const MAX_LIMIT: i64 = 5000;
const DEFAULT_LIMIT: i64 = 500;

/// `_db_manager`'s refusal, verbatim — it names the Secret, never a DSN.
const DATABASE_URL_REQUIRED: &str =
    "DATABASE_URL is required (it lives in the licenses-env Secret)";
/// `main_cli`'s generic-exception error: STATIC, never a database exception or DSN.
const SWEEP_FAILED: &str = "the sweep could not complete; see the traceback on stderr";
/// `type(e).__name__` is a Python class; this is its stable Rust counterpart.
const DB_ERROR_KIND: &str = "sqlx::Error";
/// The `error_kind` the script attaches to every configuration refusal.
const BAD_ENV_KIND: &str = "bad_expire_env";

/// The independent second opinion: `models.py::_STALE_LICENSE_COUNT_SQL`, verbatim. Its `$1` is
/// the ONCE-captured `now`, never `now()` — see the module docs.
const STALE_COUNT_SQL: &str = "SELECT count(*) FROM license \
     WHERE status = 'active' AND expires_at IS NOT NULL AND expires_at < $1";

/// `models.py::expire_all_stale_licenses`'s ORM query in its own words: oldest first, limited,
/// and — armed only — with `FOR UPDATE`.
const SELECT_STALE_SQL: &str = "SELECT id FROM license \
     WHERE status = 'active' AND expires_at IS NOT NULL AND expires_at < $1 \
     ORDER BY expires_at ASC, id ASC LIMIT $2";
const SELECT_STALE_FOR_UPDATE_SQL: &str = "SELECT id, status FROM license \
     WHERE status = 'active' AND expires_at IS NOT NULL AND expires_at < $1 \
     ORDER BY expires_at ASC, id ASC LIMIT $2 FOR UPDATE";

/// The sweep's bulk UPDATE, keyed on exactly the ids just locked. `AND status = 'active'` is the
/// script's explicit re-qualification under READ COMMITTED, belt-and-braces.
const EXPIRE_IDS_SQL: &str = "UPDATE license SET status = 'expired', updated_at = $1 WHERE id = ANY($2) AND status = 'active'";

/// The three environment knobs, resolved once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub actor: String,
    pub dry_run: bool,
    pub limit: i64,
}

/// A malformed `EXPIRE_*` value. The script parses `EXPIRE_LIMIT` and `EXPIRE_DRY_RUN` before it
/// ever reads `EXPIRE_ACTOR`, so a bad knob leaves the payload's `actor` NULL — reproduced here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub actor: Option<String>,
    pub error: String,
}

impl ConfigError {
    /// The script's `_fail(actor, str(e), error_kind="bad_expire_env")`, as an outcome.
    pub fn outcome(&self) -> RunOutcome {
        failure(self.actor.as_deref(), &self.error, BAD_ENV_KIND)
    }
}

impl Settings {
    /// Read `EXPIRE_LIMIT`, `EXPIRE_DRY_RUN` and `EXPIRE_ACTOR` from the process environment, in
    /// the script's order (limit, then dry-run, then actor).
    pub fn from_env() -> Result<Settings, ConfigError> {
        Settings::from_lookup(|name| std::env::var(name).ok())
    }

    /// The testable core of `from_env`: the environment is supplied instead of read, so a test
    /// can prove the defaults and the refusals without mutating this process's environment.
    pub fn from_lookup<F: Fn(&str) -> Option<String>>(lookup: F) -> Result<Settings, ConfigError> {
        let limit = env_int(&lookup, "EXPIRE_LIMIT", DEFAULT_LIMIT, MIN_LIMIT, MAX_LIMIT)
            .map_err(|error| ConfigError { actor: None, error })?;
        // SHIPS DRY-RUN: arming is an explicit EXPIRE_DRY_RUN=false.
        let dry_run = env_bool(&lookup, "EXPIRE_DRY_RUN", true)
            .map_err(|error| ConfigError { actor: None, error })?;
        let actor = lookup("EXPIRE_ACTOR")
            .map(|raw| raw.trim().to_owned())
            .filter(|actor| !actor.is_empty())
            .unwrap_or_else(|| DEFAULT_ACTOR.to_owned());
        Ok(Settings {
            actor,
            dry_run,
            limit,
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
        Ok(settings) => run_with(database_url, settings).await,
        Err(bad) => {
            eprintln!("expire-stale: refusing to run: {}", bad.error);
            bad.outcome()
        }
    }
}

/// The testable core: the knobs are supplied instead of read from the process environment, so a
/// test can never be perturbed by the developer's own `EXPIRE_*` exports.
pub async fn run_with(database_url: Option<String>, settings: Settings) -> RunOutcome {
    let Settings {
        actor,
        dry_run,
        limit,
    } = settings;
    let Some(url) = database_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
    else {
        return failure(Some(&actor), DATABASE_URL_REQUIRED, BAD_ENV_KIND);
    };

    // ONE instant for count, sweep and recount — see the module docs.
    let now = Utc::now();
    match sweep_and_count(&url, now, limit, dry_run).await {
        Ok((before, selected, after)) => {
            outcome(&actor, dry_run, limit, now, before, &selected, after)
        }
        Err(error) => {
            // The exception goes to stderr; stdout keeps the one static JSON line.
            eprintln!("expire-stale: the sweep could not complete: {error}");
            failure(Some(&actor), SWEEP_FAILED, DB_ERROR_KIND)
        }
    }
}

/// `before` count → sweep → `after` count, mirroring the script's three transactions.
async fn sweep_and_count(
    url: &str,
    now: DateTime<Utc>,
    limit: i64,
    dry_run: bool,
) -> Result<(i64, Vec<Uuid>, i64), sqlx::Error> {
    let pool = crate::db::connect(url, 1).await?;
    let before = count_stale(&pool, now).await?;
    let selected = sweep(&pool, now, limit, dry_run).await?;
    let after = count_stale(&pool, now).await?;
    pool.close().await;
    Ok((before, selected, after))
}

async fn count_stale(pool: &PgPool, now: DateTime<Utc>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(STALE_COUNT_SQL)
        .bind(now)
        .fetch_one(pool)
        .await
}

/// Select the oldest stale licences; write only when armed. A dry run takes no locks, writes
/// nothing, and returns the ids it WOULD have expired.
async fn sweep(
    pool: &PgPool,
    now: DateTime<Utc>,
    limit: i64,
    dry_run: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if dry_run {
        // Read-only: no `FOR UPDATE`, no UPDATE, and the read transaction is discarded.
        let ids: Vec<Uuid> = sqlx::query_scalar(SELECT_STALE_SQL)
            .bind(now)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?;
        tx.rollback().await?;
        return Ok(ids);
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(SELECT_STALE_FOR_UPDATE_SQL)
        .bind(now)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
    // The script's explicit re-check: a row a concurrent writer already retired is skipped.
    let ids: Vec<Uuid> = rows
        .into_iter()
        .filter(|(_, status)| status == "active")
        .map(|(id, _)| id)
        .collect();
    sqlx::query(EXPIRE_IDS_SQL)
        .bind(now)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ids)
}

/// The script's `_classify`, verbatim: `(exit_code, reason)`. Order matters — the over-fire and
/// dry-run-wrote legs describe a broken sweep, so they come first.
fn classify(
    dry_run: bool,
    limit: i64,
    stale_before: i64,
    stale_after: i64,
    selected: &[Uuid],
) -> (i32, String) {
    let selected_repr = python_list(selected);
    let selected_len = selected.len() as i64;
    if selected_len > stale_before {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "OVER-FIRED: the sweep selected {selected_len} licence(s) but only \
                 {stale_before} are stale by the independent rule (status='active' AND \
                 expires_at IS NOT NULL AND expires_at < now). The two are evaluated at the \
                 same instant, so this is predicate drift — a widened selection mass-expires \
                 a healthy estate and every row it touched looks like a success. \
                 Selected: {selected_repr}"
            ),
        );
    }
    if dry_run && stale_after != stale_before {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "DRY RUN WROTE: the stale count moved from {stale_before} to {stale_after} \
                 during a run that is defined to write nothing. Do not trust this run, and do \
                 not arm this job until that is explained."
            ),
        );
    }
    if dry_run && stale_before > 0 {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "NOT CONVERGED (measure-only): {stale_before} licence(s) are structurally \
                 active with a past expires_at and a dry run writes nothing, so all of them \
                 remain. This is the answer about the estate, not a list of failures. \
                 Would transition: {selected_repr}. Re-run with EXPIRE_DRY_RUN=false to \
                 retire them."
            ),
        );
    }
    if !dry_run && selected.is_empty() && stale_before > 0 {
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "VACUOUS SWEEP: transitioned 0 licence(s) while {stale_before} are stale by \
                 the independent rule. A run that changes nothing over a broken estate must \
                 never report success — the selection is not doing what this job claims."
            ),
        );
    }
    if !dry_run && stale_after > 0 {
        if selected_len == limit {
            return (
                EXIT_NOT_CONVERGED,
                format!(
                    "TRUNCATED SWEEP: transitioned exactly the limit ({limit}) and \
                     {stale_after} stale licence(s) remain. Repeating this schedule chews \
                     through the backlog {limit} at a time, but until it drains, every run \
                     leaves corpses holding their unique-index slots. Raise EXPIRE_LIMIT \
                     above {stale_before}."
                ),
            );
        }
        return (
            EXIT_NOT_CONVERGED,
            format!(
                "NOT CONVERGED: {stale_after} stale licence(s) remain after transitioning \
                 {selected_len} — under the limit ({limit}), so they were selected and \
                 not retired, or were re-created during the sweep."
            ),
        );
    }
    (EXIT_CONVERGED, "converged".to_owned())
}

/// The script's success payload, key for key. `selected` is what the sweep picked;
/// `transitioned` is what it WROTE, zero on a dry run BY DEFINITION.
fn outcome(
    actor: &str,
    dry_run: bool,
    limit: i64,
    now: DateTime<Utc>,
    stale_before: i64,
    selected: &[Uuid],
    stale_after: i64,
) -> RunOutcome {
    let (code, reason) = classify(dry_run, limit, stale_before, stale_after, selected);
    RunOutcome {
        code,
        payload: json!({
            "event": EVENT,
            "actor": actor,
            "dry_run": dry_run,
            "limit": limit,
            "now": now.to_rfc3339_opts(SecondsFormat::Micros, false),
            "stale_before": stale_before,
            "stale_after": stale_after,
            "selected": selected.len(),
            "selected_license_ids": selected.iter().map(Uuid::to_string).collect::<Vec<_>>(),
            "transitioned": if dry_run { 0 } else { selected.len() },
            "exit_code": code,
            "reason": reason,
        }),
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

/// Python's list repr for the reason strings: `['id', 'id']`.
fn python_list(ids: &[Uuid]) -> String {
    let items: Vec<String> = ids.iter().map(|id| format!("'{id}'")).collect();
    format!("[{}]", items.join(", "))
}

/// `_env_bool`: STRICT. A typo raises instead of quietly meaning false, and a BLANK value means
/// "unset" — i.e. the default — so `EXPIRE_DRY_RUN: ""` cannot arm the job.
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

/// `_env_int`: absent, non-UTF-8 and blank read as the default; a non-integer or an out-of-bounds
/// value refuses.
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
