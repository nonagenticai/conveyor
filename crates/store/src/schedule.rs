//! Schedules: five-field cron, materialised jobs, and the one advisory lock.
//!
//! Data-architecture §4: `schedule` rows materialise `job` rows under
//! `pg_advisory_xact_lock(hashtext('conveyor:schedule'))`, so two replicas cannot
//! double-materialise. The lock is taken FIRST in the transaction, before the due rows are
//! read; the second run then reads the already-advanced `next_run_at` and creates nothing.
//!
//! `next_run_at` is computed from `now`, not from the previous `next_run_at`: a worker that
//! was down for three ticks creates ONE job per schedule and skips the backlog, which is
//! what a maintenance kind (`event.prune`, `licence.sync`) wants. A backlog would instead
//! queue stale ticks nobody needs.
//!
//! **Cron subset, stated rather than implied.** The parser accepts the standard five-field
//! form with `*`, `*/n`, `a`, `a-b`, `a-b/n` and comma lists. Day-of-month and day-of-week
//! must not BOTH be restricted — standard cron ORs them, and this slice refuses an
//! expression whose meaning would depend on that rule instead of guessing. `timezone` is
//! carried in the table; this build resolves every schedule in UTC and refuses a non-UTC
//! row (a named timezone needs a tz database, which arrives with the slice that needs it).

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Timelike, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::scope::begin_scoped;
use crate::tenants;

/// The maintenance kinds and their default cron (CONTRACT §7: schedules replace per-process
/// timers, never a JS-style interval).
///
/// `licence.sync` every 5 minutes keeps `app_licence.checked_at` inside the one-hour
/// staleness window (`crates/auth`'s `LICENCE_MAX_AGE_SECS`) with twelve missed runs to
/// spare — the licence extractor refuses a row older than that.
pub const DEFAULT_SCHEDULES: [(&str, &str); 21] = [
    ("event.prune", "15 3 * * *"),
    ("job.prune", "45 3 * * *"),
    // The three rules §6 stated and nothing produced until 0012: the invocation trace (30 d), terminal
    // runs (180 d) and terminal deliveries (30 d). They run after the platform prunes, in the same
    // early-morning window, and each is a schedule row so a stopped worker resumes them (CONTRACT §7).
    ("action_invocation.prune", "05 4 * * *"),
    ("run.prune", "20 4 * * *"),
    ("delivery.prune", "35 4 * * *"),
    // S08's retention producers (data-architecture §2.4 and §5.3): a model keeps its newest
    // 50 revisions, and a validation run is kept 90 days. Both sit in the early-morning
    // maintenance window beside the platform prunes.
    ("model.prune", "25 3 * * *"),
    ("validation.prune", "35 3 * * *"),
    ("idempotency.sweep", "10 * * * *"),
    ("licence.sync", "*/5 * * * *"),
    // S06's 60 s sweepers. A crashed worker leaves a run whose `run.execute` job went dead;
    // an expired lease must be swept before the card can be claimed again; a gate past its
    // deadline must close. Each is a schedule row, never a timer (CONTRACT §7).
    ("card.lease_sweep", "* * * * *"),
    ("run.reap", "* * * * *"),
    // S19: finalise the eval runs whose cases have all landed, and abandon the ones nothing
    // has ticked. The reaper keys on `eval_run.last_polled_at`, never on case silence.
    ("eval.run.tick", "* * * * *"),
    ("eval.reap", "*/5 * * * *"),
    // S19 Part C: the eval transcript's 90-day retention (data-architecture §6).
    ("eval.prune", "50 3 * * *"),
    ("approval.expire", "* * * * *"),
    // The close hand-off to the signed mcp channel: every closed gate is queued once.
    ("approval.closed.emit", "* * * * *"),
    // S06d: multicast an interaction to every eligible channel, then close the losing
    // recipients once one answer resolves it. The dispatch row is the idempotency witness.
    ("interaction.dispatch", "* * * * *"),
    ("interaction.close", "* * * * *"),
    // S10's copilot pair: a `streaming` turn older than 300 s is interrupted, and an approved
    // proposal is queued for execution. Both are schedule rows, never process timers.
    ("copilot.turn.reap", "* * * * *"),
    ("copilot.proposal.sweep", "* * * * *"),
    // S17's benchmark reaper: a run whose driver has not polled for five minutes is failed and
    // its orphaned sandbox Jobs are deleted. The driver itself is a per-tick job, not a cron.
    ("benchmark.reap", "*/5 * * * *"),
];

/// The furthest ahead the next-run search looks. A cron with no occurrence inside a year
/// is a bug (`0 0 31 2 *`), and an unbounded search would spin.
pub const MAX_LOOKAHEAD_DAYS: i64 = 366;

/// Why a cron expression or a schedule row is unusable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CronError {
    #[error(
        "cron {0:?} must have exactly five fields (minute hour day-of-month month day-of-week)"
    )]
    FieldCount(String),
    #[error("cron field {field} is invalid in {spec:?}: {value:?}")]
    Field {
        field: &'static str,
        spec: String,
        value: String,
    },
    #[error("cron {0:?} restricts both day-of-month and day-of-week; use one")]
    DayConflict(String),
    #[error("cron {0:?} has no occurrence within {MAX_LOOKAHEAD_DAYS} days")]
    NoOccurrence(String),
}

/// A parsed five-field cron expression, resolved in UTC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cron {
    spec: String,
    minute: u64,
    hour: u64,
    day_of_month: u64,
    month: u64,
    day_of_week: u64,
    dom_restricted: bool,
    dow_restricted: bool,
}

impl Cron {
    /// Parse `minute hour day-of-month month day-of-week`.
    pub fn parse(spec: &str) -> Result<Self, CronError> {
        let fields: Vec<&str> = spec.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(CronError::FieldCount(spec.to_owned()));
        }
        let minute = parse_field(fields[0], 0, 59, "minute", spec)?;
        let hour = parse_field(fields[1], 0, 23, "hour", spec)?;
        let day_of_month = parse_field(fields[2], 1, 31, "day-of-month", spec)?;
        let month = parse_field(fields[3], 1, 12, "month", spec)?;
        let day_of_week = parse_field(fields[4], 0, 6, "day-of-week", spec)?;
        let dom_restricted = fields[2] != "*";
        let dow_restricted = fields[4] != "*";
        if dom_restricted && dow_restricted {
            return Err(CronError::DayConflict(spec.to_owned()));
        }
        Ok(Self {
            spec: spec.to_owned(),
            minute,
            hour,
            day_of_month,
            month,
            day_of_week,
            dom_restricted,
            dow_restricted,
        })
    }

    /// The first occurrence strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>, CronError> {
        // Cron has minute resolution: start at the next whole minute.
        let mut candidate = Utc
            .with_ymd_and_hms(
                after.year(),
                after.month(),
                after.day(),
                after.hour(),
                after.minute(),
                0,
            )
            .single()
            .ok_or_else(|| CronError::NoOccurrence(self.spec.clone()))?
            + ChronoDuration::minutes(1);
        for _ in 0..MAX_LOOKAHEAD_DAYS * 24 * 60 {
            if self.matches(&candidate) {
                return Ok(candidate);
            }
            candidate += ChronoDuration::minutes(1);
        }
        Err(CronError::NoOccurrence(self.spec.clone()))
    }

    fn matches(&self, at: &DateTime<Utc>) -> bool {
        let day_ok = if self.dom_restricted {
            bit(self.day_of_month, at.day())
        } else if self.dow_restricted {
            bit(self.day_of_week, at.weekday().num_days_from_sunday())
        } else {
            true
        };
        day_ok
            && bit(self.month, at.month())
            && bit(self.hour, at.hour())
            && bit(self.minute, at.minute())
    }
}

fn bit(mask: u64, value: u32) -> bool {
    value < 64 && mask & (1u64 << value) != 0
}

fn parse_field(
    spec: &str,
    min: u32,
    max: u32,
    name: &'static str,
    whole: &str,
) -> Result<u64, CronError> {
    let invalid = || CronError::Field {
        field: name,
        spec: whole.to_owned(),
        value: spec.to_owned(),
    };
    let mut mask = 0u64;
    for part in spec.split(',') {
        if part.is_empty() {
            return Err(invalid());
        }
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (
                range,
                step.parse::<u32>()
                    .ok()
                    .filter(|step| *step > 0)
                    .ok_or_else(invalid)?,
            ),
            None => (part, 1),
        };
        if range.is_empty() {
            return Err(invalid());
        }
        let (low, high) = if range == "*" {
            (min, max)
        } else if let Some((low, high)) = range.split_once('-') {
            (
                parse_value(low, min, max).ok_or_else(invalid)?,
                parse_value(high, min, max).ok_or_else(invalid)?,
            )
        } else {
            let value = parse_value(range, min, max).ok_or_else(invalid)?;
            // `5/10` means "from 5 to the maximum, every 10" (the common reading).
            (value, if step > 1 { max } else { value })
        };
        if low > high {
            return Err(invalid());
        }
        let mut value = low;
        while value <= high {
            mask |= 1u64 << value;
            value += step;
        }
    }
    if mask == 0 {
        return Err(invalid());
    }
    Ok(mask)
}

fn parse_value(raw: &str, min: u32, max: u32) -> Option<u32> {
    raw.parse::<u32>().ok().filter(|v| (min..=max).contains(v))
}

/// One `schedule` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ScheduleRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: String,
    pub cron: String,
    pub timezone: String,
    pub payload: Value,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
}

/// Insert the default schedule rows for every active tenant that lacks them.
///
/// Idempotent (`ON CONFLICT (tenant_id, kind) DO NOTHING`) and safe to run on every
/// `migrate`: a tenant added later gets its rows on the next migrate, and an existing row is
/// never rewritten (an operator's cron edit survives).
pub async fn seed_defaults(pool: &PgPool) -> Result<usize, sqlx::Error> {
    let now = Utc::now();
    let mut inserted = 0usize;
    for tenant in tenants::active_ids(pool).await? {
        let mut tx = begin_scoped(pool, tenant).await?;
        for (kind, cron) in DEFAULT_SCHEDULES {
            let next = Cron::parse(cron)
                .expect("the seeded crons are constants and are unit-tested")
                .next_after(now)
                .expect("the seeded crons have an occurrence");
            inserted += sqlx::query(
                "INSERT INTO schedule (tenant_id, kind, cron, timezone, next_run_at)
                 VALUES ($1, $2, $3, 'UTC', $4)
                 ON CONFLICT (tenant_id, kind) DO NOTHING",
            )
            .bind(tenant.as_uuid())
            .bind(kind)
            .bind(cron)
            .bind(next)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;
        }
        tx.commit().await?;
    }
    Ok(inserted)
}

/// Materialise every due schedule of `tenant` into jobs.
///
/// Returns the ids of the jobs created. Taking the advisory lock first is what makes two
/// concurrent calls create ONE job: the second waits, then reads the advanced
/// `next_run_at` and finds nothing due.
pub async fn materialise(
    pool: &PgPool,
    tenant: TenantId,
    now: DateTime<Utc>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    // Session-independent, transaction-scoped: the lock dies at COMMIT/ROLLBACK, so a
    // crashed materialiser cannot wedge the next one.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('conveyor:schedule'))")
        .execute(&mut *tx)
        .await?;

    let due: Vec<ScheduleRow> = sqlx::query_as(
        "SELECT id, tenant_id, kind, cron, timezone, payload, enabled, next_run_at, last_run_at
           FROM schedule
          WHERE enabled AND next_run_at <= $1
          ORDER BY next_run_at, id
          FOR UPDATE",
    )
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;

    let mut created = Vec::new();
    for row in due {
        if !row.timezone.eq_ignore_ascii_case("UTC") {
            // Not fatal: the row stays due and every tick logs. Silently treating a named
            // zone as UTC would fire at the wrong local hour.
            tracing::warn!(
                schedule = %row.id,
                kind = row.kind,
                timezone = row.timezone,
                "only UTC schedules are supported in this build; skipping"
            );
            continue;
        }
        let cron = match Cron::parse(&row.cron) {
            Ok(cron) => cron,
            Err(error) => {
                tracing::warn!(schedule = %row.id, kind = row.kind, %error, "invalid cron; skipping");
                continue;
            }
        };
        let next = match cron.next_after(now) {
            Ok(next) => next,
            Err(error) => {
                tracing::warn!(schedule = %row.id, kind = row.kind, %error, "no next occurrence; skipping");
                continue;
            }
        };

        // One live job per schedule: the dedupe key is the schedule's id, so a job that is
        // still pending from the previous tick suppresses a second one.
        let job: Option<(Uuid,)> = sqlx::query_as(
            "INSERT INTO job (tenant_id, kind, dedupe_key, payload)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
             DO NOTHING
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(&row.kind)
        .bind(row.id.to_string())
        .bind(&row.payload)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((job_id,)) = job {
            created.push(job_id);
        }

        // Advance even when the insert was deduped: the tick was materialised either way.
        sqlx::query(
            "UPDATE schedule
                SET last_run_at = $1, next_run_at = $2, updated_at = now()
              WHERE id = $3",
        )
        .bind(now)
        .bind(next)
        .bind(row.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(created)
}

/// Materialise every active tenant's due schedules (the worker's tick).
pub async fn materialise_all(pool: &PgPool, now: DateTime<Utc>) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut created = Vec::new();
    for tenant in tenants::active_ids(pool).await? {
        created.extend(materialise(pool, tenant, now).await?);
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
    }

    #[test]
    fn the_seeded_crons_parse_and_resolve() {
        for (kind, spec) in DEFAULT_SCHEDULES {
            let cron = Cron::parse(spec).unwrap_or_else(|e| panic!("{kind} {spec}: {e}"));
            let next = cron.next_after(at(2026, 11, 7, 12, 0)).unwrap();
            assert!(next > at(2026, 11, 7, 12, 0), "{kind} {spec} -> {next}");
        }
    }

    #[test]
    fn every_five_minutes_advances_five_minutes() {
        let cron = Cron::parse("*/5 * * * *").unwrap();
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 12, 1)),
            Ok(at(2026, 11, 7, 12, 5))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 12, 5)),
            Ok(at(2026, 11, 7, 12, 10))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 23, 58)),
            Ok(at(2026, 11, 8, 0, 0))
        );
    }

    #[test]
    fn a_daily_cron_rolls_to_the_next_day() {
        let cron = Cron::parse("15 3 * * *").unwrap();
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 2, 0)),
            Ok(at(2026, 11, 7, 3, 15))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 3, 15)),
            Ok(at(2026, 11, 8, 3, 15))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 3, 16)),
            Ok(at(2026, 11, 8, 3, 15))
        );
    }

    #[test]
    fn lists_ranges_and_steps_are_supported() {
        let cron = Cron::parse("0,30 9-17 * * *").unwrap();
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 8, 59)),
            Ok(at(2026, 11, 7, 9, 0))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 9, 0)),
            Ok(at(2026, 11, 7, 9, 30))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 9, 30)),
            Ok(at(2026, 11, 7, 10, 0))
        );
        assert_eq!(
            cron.next_after(at(2026, 11, 7, 17, 30)),
            Ok(at(2026, 11, 8, 9, 0))
        );
    }

    #[test]
    fn a_weekday_cron_is_honoured() {
        // 2026-11-07 is a Saturday; 0 = Sunday, 6 = Saturday.
        let weekdays = Cron::parse("0 9 * * 1-5").unwrap();
        assert_eq!(
            weekdays.next_after(at(2026, 11, 7, 10, 0)),
            Ok(at(2026, 11, 9, 9, 0)),
            "the next weekday after Saturday is Monday"
        );
    }

    #[test]
    fn ambiguous_and_invalid_crons_are_refused() {
        assert_eq!(
            Cron::parse("0 0 1 * 1").unwrap_err(),
            CronError::DayConflict("0 0 1 * 1".into())
        );
        assert!(matches!(
            Cron::parse("* * * *").unwrap_err(),
            CronError::FieldCount(_)
        ));
        assert!(Cron::parse("60 * * * *").is_err());
        assert!(Cron::parse("* 24 * * *").is_err());
        assert!(Cron::parse("*/0 * * * *").is_err());
        assert!(Cron::parse("* * * * 7").is_err());
        assert!(Cron::parse("1-2-3 * * * *").is_err());
        assert!(Cron::parse("5-1 * * * *").is_err());
    }
}
