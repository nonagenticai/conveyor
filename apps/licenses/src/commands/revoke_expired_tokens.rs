//! `revoke-expired-tokens`: revoke time-expired-but-still-unrevoked tokens, estate-wide.
//!
//! Ported from `scripts/revoke_expired_tokens.py` (licenses @ 4e8a08d). The contract is the
//! script's, not a redesign:
//!
//! - configuration is ALL environment (`REVOKE_DRY_RUN`, `REVOKE_LIMIT`, `REVOKE_ACTOR`,
//!   `DATABASE_URL`), so a CronJob manifest stays the only knob;
//! - ONE JSON line on stdout on EVERY path, config failures included;
//! - exit 0 converged, 1 not converged, 2 config/DB failure.
//!
//! ⚠️ It ships DRY-RUN, deliberately: revoking a token is not reversible, so arming is an explicit
//! `REVOKE_DRY_RUN=false`. A BLANK value reads as the default, so `REVOKE_DRY_RUN: ""` cannot arm
//! the sweep by accident.
//!
//! ⚠️ The anti-vacuity guards are the point of this command, not decoration. A sweep whose happy
//! path is "found nothing" cannot tell a clean estate from a broken selection, so:
//!
//! - a DRY RUN that found expired tokens is NOT converged (it wrote nothing, so they remain);
//! - an armed run that revoked nothing while expired tokens exist is a broken selection;
//! - an armed run that left rows behind outgrew `REVOKE_LIMIT` (truncation);
//! - an armed run that touched more rows than the independent count says are expired over-fired.
//!
//! The count is HAND-WRITTEN SQL on purpose (`models.py::_EXPIRED_TOKEN_COUNT_SQL`), never the
//! sweep's own selection: a count reusing the sweep query could only ever agree with itself.
//! Count, sweep and recount are all evaluated at ONE captured instant, so a token crossing its
//! `expires_at` mid-run, or clock skew between this process and postgres, cannot be mistaken for
//! predicate drift.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::RunOutcome;

/// The job name the script reports in every payload.
const JOB: &str = "revoke_expired_tokens";
/// `_MIN_LIMIT`, `_MAX_LIMIT`, `_DEFAULT_LIMIT` in the script.
const MIN_LIMIT: i64 = 1;
const MAX_LIMIT: i64 = 5000;
const DEFAULT_LIMIT: i64 = 500;

/// The independent second opinion: `models.py::_EXPIRED_TOKEN_COUNT_SQL`, verbatim. Its `$1` is
/// the ONCE-captured `now`, never `now()` — see the module docs.
const EXPIRED_TOKEN_COUNT_SQL: &str = "SELECT count(*) FROM token \
     WHERE revoked = false AND expires_at IS NOT NULL AND expires_at < $1";

/// `models.py::revoke_all_expired_tokens`'s ORM query in its own words: oldest first, limited.
const SELECT_EXPIRED_SQL: &str = "SELECT id FROM token \
     WHERE revoked = false AND expires_at IS NOT NULL AND expires_at < $1 \
     ORDER BY expires_at ASC LIMIT $2";

/// The sweep's bulk UPDATE, keyed on exactly the ids just selected.
const REVOKE_IDS_SQL: &str = "UPDATE token SET revoked = true, revoked_at = $1 WHERE id = ANY($2)";

/// The three environment knobs, resolved once. All infallible except `REVOKE_LIMIT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub actor: String,
    pub dry_run: bool,
    pub limit: i64,
}

/// A `REVOKE_LIMIT` that cannot be honoured. Carries the other two knobs because the script's
/// `config_error` payload still reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitError {
    pub actor: String,
    pub dry_run: bool,
    pub error: String,
}

impl Settings {
    /// Read `REVOKE_ACTOR` (default `unknown`), `REVOKE_DRY_RUN` (default true) and
    /// `REVOKE_LIMIT` (default 500, bounds 1..=5000) from the process environment.
    pub fn from_env() -> Result<Settings, LimitError> {
        let actor = match var_trimmed("REVOKE_ACTOR") {
            Some(actor) if !actor.is_empty() => actor,
            _ => "unknown".to_owned(),
        };
        let dry_run = env_flag("REVOKE_DRY_RUN", true);
        let limit = env_limit().map_err(|error| LimitError {
            actor: actor.clone(),
            dry_run,
            error,
        })?;
        Ok(Settings {
            actor,
            dry_run,
            limit,
        })
    }
}

/// The command as the CronJob runs it: every knob from the process environment.
pub async fn run(database_url: Option<String>) -> RunOutcome {
    match Settings::from_env() {
        Ok(settings) => run_with(database_url, settings).await,
        Err(bad) => config_outcome(&bad.actor, bad.dry_run, &bad.error),
    }
}

/// The testable core: the knobs are supplied instead of read from the process environment, so a
/// test can never be perturbed by the developer's own `REVOKE_*` exports.
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
        return config_outcome(&actor, dry_run, "DATABASE_URL is required");
    };

    // ONE instant for count, sweep and recount — see the module docs.
    let now = Utc::now();
    match sweep_and_count(&url, now, limit, dry_run).await {
        Ok((before, acted, after)) => outcome(&actor, dry_run, limit, now, before, &acted, after),
        Err(error) => RunOutcome {
            code: 2,
            payload: json!({
                "job": JOB,
                "actor": actor,
                "status": "db_error",
                "error": format!("sqlx::Error: {error}"),
                "dry_run": dry_run,
            }),
        },
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
    let before: i64 = sqlx::query_scalar(EXPIRED_TOKEN_COUNT_SQL)
        .bind(now)
        .fetch_one(&pool)
        .await?;
    let acted = sweep(&pool, now, limit, dry_run).await?;
    let after: i64 = sqlx::query_scalar(EXPIRED_TOKEN_COUNT_SQL)
        .bind(now)
        .fetch_one(&pool)
        .await?;
    pool.close().await;
    Ok((before, acted, after))
}

/// Select the oldest expired tokens; write only when armed. A dry run rolls its transaction back
/// and returns the ids it WOULD have revoked.
async fn sweep(
    pool: &PgPool,
    now: DateTime<Utc>,
    limit: i64,
    dry_run: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(SELECT_EXPIRED_SQL)
        .bind(now)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
    if dry_run {
        tx.rollback().await?;
        return Ok(ids);
    }
    sqlx::query(REVOKE_IDS_SQL)
        .bind(now)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ids)
}

/// The script's guards verbatim, then the payload and the exit code they imply.
fn outcome(
    actor: &str,
    dry_run: bool,
    limit: i64,
    now: DateTime<Utc>,
    before: i64,
    acted: &[Uuid],
    after: i64,
) -> RunOutcome {
    let mut problems: Vec<String> = Vec::new();
    if dry_run {
        if after != before {
            problems.push(format!("dry_run_mutated: before={before} after={after}"));
        }
        if before != 0 {
            problems.push(format!(
                "dry_run_found_expired_tokens: {before} still unrevoked"
            ));
        }
    } else {
        if before != 0 && acted.is_empty() {
            problems.push(format!(
                "selected_nothing_while_{before}_expired_tokens_exist"
            ));
        }
        if acted.len() as i64 > before {
            problems.push(format!(
                "over_fire: revoked {} but only {before} were expired",
                acted.len()
            ));
        }
        if after != 0 {
            problems.push(format!(
                "left_behind: {after} still unrevoked (limit={limit})"
            ));
        }
    }

    let converged = problems.is_empty();
    if !converged {
        // Anything human goes to stderr; stdout stays exactly one JSON line.
        eprintln!("NOT CONVERGED: {}", problems.join("; "));
    }
    RunOutcome {
        code: if converged { 0 } else { 1 },
        payload: json!({
            "job": JOB,
            "actor": actor,
            "status": if converged { "ok" } else { "not_converged" },
            "dry_run": dry_run,
            "limit": limit,
            "expired_before": before,
            "acted": acted.len(),
            "expired_after": after,
            "converged": converged,
            "problems": problems,
            "at": now.to_rfc3339_opts(SecondsFormat::Micros, false),
        }),
    }
}

fn config_outcome(actor: &str, dry_run: bool, error: &str) -> RunOutcome {
    RunOutcome {
        code: 2,
        payload: json!({
            "job": JOB,
            "actor": actor,
            "status": "config_error",
            "error": error,
            "dry_run": dry_run,
        }),
    }
}

/// A trimmed environment value; absent, non-UTF-8 and blank all read as `None`.
fn var_trimmed(name: &str) -> Option<String> {
    let raw = std::env::var(name).ok()?;
    let raw = raw.trim();
    (!raw.is_empty()).then(|| raw.to_owned())
}

/// A BLANK value reads as the default, so `REVOKE_DRY_RUN: ""` cannot arm the sweep.
fn env_flag(name: &str, default: bool) -> bool {
    let Some(raw) = var_trimmed(name) else {
        return default;
    };
    !matches!(
        raw.to_ascii_lowercase().as_str(),
        "false" | "0" | "no" | "off"
    )
}

fn env_limit() -> Result<i64, String> {
    let Some(raw) = var_trimmed("REVOKE_LIMIT") else {
        return Ok(DEFAULT_LIMIT);
    };
    let value: i64 = raw
        .parse()
        .map_err(|_| format!("REVOKE_LIMIT is not an integer: '{raw}'"))?;
    if !(MIN_LIMIT..=MAX_LIMIT).contains(&value) {
        return Err(format!(
            "REVOKE_LIMIT must be {MIN_LIMIT}..{MAX_LIMIT}, got {value}"
        ));
    }
    Ok(value)
}
