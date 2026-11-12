#![forbid(unsafe_code)]
//! `eval_run` — the measurement's header row (S19 Part B step 10; CONTRACT §20).
//!
//! A run names a **version**, a variant and a repetition count; its cases are ordinary `job`
//! rows (`eval.case.run`, one per `(case, rep)`), and its scores are `eval_case_result` rows.
//! Three facts are deliberate:
//!
//! - **Nothing here is a counter that could drift.** `case_count` is the size of the set the run
//!   was started over; how many slots are *filled* is a `COUNT` over the result rows
//!   ([`slot_counts_tx`]), and how much work is *outstanding* is a `COUNT` over the job rows
//!   ([`outstanding_jobs_tx`]). The tick finalises from those two numbers, never from a column a
//!   previous driver wrote ([`super::super::benchmark`]'s §2.17 lesson).
//! - **`complete` means every slot is filled; `partial` means the work is over and some slots are
//!   not.** A permanently failed case leaves its slot empty by design, so a run whose cases all
//!   failed finalises `void` with the evidence in `eval_attempt_error` rather than sitting
//!   `running` forever.
//! - **`last_polled_at` is the reaper's key, never case silence.** [`stale_runs_tx`] selects on
//!   it, so a run whose cases are quiet is running, and only a run nothing has ticked is void.
//!
//! `eval_run_variant_unique` is `(tenant_id, eval_id, variant)`: a second `start` on the same
//! variant **resumes** the row ([`widen_run_tx`] widens a pilot into a full pass), it does not
//! insert a second one.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::eval::{RunMode, RunStatus, Variant};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// The public projection of an `eval_run` row.
#[derive(Clone, Debug)]
pub struct Run {
    /// The row id.
    pub id: Uuid,
    /// The eval this run measures.
    pub eval_id: Uuid,
    /// The frozen instrument the run names.
    pub version_id: Uuid,
    /// `baseline | v<N>` — the report's column.
    pub variant: Variant,
    /// The climb round (S20); `0` outside a climb.
    pub round: i32,
    /// `pilot | full`.
    pub mode: RunMode,
    /// Repetitions per case.
    pub reps: i32,
    /// How many cases the run was started over.
    pub case_count: i32,
    /// The lifecycle state.
    pub status: RunStatus,
    /// What changed, in prose.
    pub change_md: Option<String>,
    /// The patch under test, when there is one.
    pub change_patch: Option<String>,
    /// `{description, target, suspicious}`.
    pub summary: Value,
    /// The report's narrative, once one is written.
    pub narrative_md: Option<String>,
    /// Repo sha, model ids, stubs, flags.
    pub environment: Value,
    /// The noise floor, once the report computes one.
    pub noise_floor: Option<Decimal>,
    /// The spend ceiling a climb set.
    pub cost_ceiling_cents: Option<i32>,
    /// When the tick last heartbeat this run.
    pub last_polled_at: Option<DateTime<Utc>>,
    /// When the first case began.
    pub started_at: Option<DateTime<Utc>>,
    /// When the work finished (`complete`, `partial`, `void` or `cancelled`).
    pub finished_at: Option<DateTime<Utc>>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
}

/// An eval run to create.
#[derive(Clone, Debug)]
pub struct NewRun<'a> {
    /// The eval being measured.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// `baseline | v<N>`, already parsed by the action.
    pub variant: &'a str,
    /// The climb round; `0` outside a climb.
    pub round: i32,
    /// `pilot | full`.
    pub mode: &'a str,
    /// Repetitions per case (≥ 1).
    pub reps: i32,
    /// How many cases the run covers.
    pub case_count: i32,
    /// Repo sha, model ids, stubs, flags.
    pub environment: &'a Value,
    /// The spend ceiling, when a climb set one.
    pub cost_ceiling_cents: Option<i32>,
}

/// How many slots a run has, and how many are filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotCounts {
    /// `case_count × reps` — what a complete run needs.
    pub expected: i64,
    /// Rows in `eval_case_result` for this run.
    pub filled: i64,
    /// Rows in `eval_attempt_error` for this run.
    pub failed: i64,
    /// Results parked for a person (`status = 'pending_label'`).
    pub pending_label: i64,
    /// Results cut at `max_tokens` — counted, excluded from the mean, never scored zero.
    pub truncated: i64,
}

impl SlotCounts {
    /// True when every slot holds a result.
    pub fn is_full(&self) -> bool {
        self.expected > 0 && self.filled >= self.expected
    }

    /// True when nothing has been measured yet.
    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }
}

const RUN_COLUMNS: &str = "id, eval_id, version_id, variant, round, mode, reps, case_count, \
                           status, change_md, change_patch, summary, narrative_md, environment, \
                           noise_floor, cost_ceiling_cents, last_polled_at, started_at, \
                           finished_at, created_at";

#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    eval_id: Uuid,
    version_id: Uuid,
    variant: String,
    round: i32,
    mode: String,
    reps: i32,
    case_count: i32,
    status: String,
    change_md: Option<String>,
    change_patch: Option<String>,
    summary: Value,
    narrative_md: Option<String>,
    environment: Value,
    noise_floor: Option<Decimal>,
    cost_ceiling_cents: Option<i32>,
    last_polled_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl RunRow {
    fn into_run(self) -> Result<Run, EvalStoreError> {
        let variant = Variant::parse(&self.variant).map_err(|_| EvalStoreError::Vocabulary {
            what: "eval_run.variant",
            value: self.variant.clone(),
        })?;
        let mode = self
            .mode
            .parse::<RunMode>()
            .map_err(|_| EvalStoreError::Vocabulary {
                what: "eval_run.mode",
                value: self.mode.clone(),
            })?;
        let status = self
            .status
            .parse::<RunStatus>()
            .map_err(|_| EvalStoreError::Vocabulary {
                what: "eval_run.status",
                value: self.status.clone(),
            })?;
        Ok(Run {
            id: self.id,
            eval_id: self.eval_id,
            version_id: self.version_id,
            variant,
            round: self.round,
            mode,
            reps: self.reps,
            case_count: self.case_count,
            status,
            change_md: self.change_md,
            change_patch: self.change_patch,
            summary: self.summary,
            narrative_md: self.narrative_md,
            environment: self.environment,
            noise_floor: self.noise_floor,
            cost_ceiling_cents: self.cost_ceiling_cents,
            last_polled_at: self.last_polled_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
            created_at: self.created_at,
        })
    }
}

const RUN_SORT: &str = "ORDER BY created_at DESC, id DESC";

/// Insert a run inside the caller's scoped transaction.
///
/// A second row for the same `(tenant, eval, variant)` is [`EvalStoreError::Conflict`] carrying
/// `eval_run_variant_unique` — resume the existing row instead of inserting.
pub async fn insert_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewRun<'_>,
) -> Result<Run, EvalStoreError> {
    let sql = format!(
        "INSERT INTO eval_run (tenant_id, eval_id, version_id, variant, round, mode, reps,
                               case_count, environment, cost_ceiling_cents)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {RUN_COLUMNS}"
    );
    let row: RunRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.eval_id)
        .bind(input.version_id)
        .bind(input.variant)
        .bind(input.round)
        .bind(input.mode)
        .bind(input.reps)
        .bind(input.case_count)
        .bind(input.environment)
        .bind(input.cost_ceiling_cents)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.into_run()
}

/// One run by id in this tenant; [`EvalStoreError::NotFound`] when it does not exist.
pub async fn get_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Run, EvalStoreError> {
    let sql = format!("SELECT {RUN_COLUMNS} FROM eval_run WHERE tenant_id = $1 AND id = $2");
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotFound { what: "eval run" })?
        .into_run()
}

/// The run for one `(eval, variant)`, if it exists.
///
/// This is the resume lookup: `eval_run_variant_unique` means there is at most one.
pub async fn find_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    variant: &str,
) -> Result<Option<Run>, EvalStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM eval_run
          WHERE tenant_id = $1 AND eval_id = $2 AND variant = $3"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(variant)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(RunRow::into_run).transpose()
}

/// The run a regrade of `source_run_id` already produced under `version_id`, if any.
///
/// This is the resume lookup for `eval_runs.regrade`: a repeat call must widen and re-enqueue the
/// run it made before, not mint a second variant for the same source. The **version is part of
/// the key** — if the eval has moved on to a third instrument since, a regrade of the same source
/// is a new measurement under that version, not a continuation of the old one.
pub async fn find_regrade_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    source_run_id: Uuid,
    version_id: Uuid,
) -> Result<Option<Run>, EvalStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM eval_run
          WHERE tenant_id = $1 AND eval_id = $2 AND version_id = $3
            AND summary->>'regraded_from' = $4"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(version_id)
        .bind(source_run_id.to_string())
        .fetch_optional(&mut *conn)
        .await?;
    row.map(RunRow::into_run).transpose()
}

/// One keyset page of runs, newest first.
pub async fn list_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Option<Uuid>,
    status: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Run>, EvalStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS}
           FROM eval_run
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR eval_id = $2)
            AND ($3::text IS NULL OR status = $3)
            AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
          {RUN_SORT}
          LIMIT $6"
    );
    let rows: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(status)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(RunRow::into_run).collect()
}

/// Move a run through the documented table, stamping the times each state implies.
///
/// The row is locked first, so two tickers cannot both observe `queued` and both transition.
/// `started_at` is stamped on the first entry into `running`; `finished_at` on `complete`,
/// `partial`, `void` and `cancelled`.
pub async fn transition_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    to: RunStatus,
) -> Result<Run, EvalStoreError> {
    let current = lock_run_tx(conn, tenant, run_id).await?;
    let next = current.status.transition(to)?;
    let sql = format!(
        "UPDATE eval_run
            SET status = $3,
                started_at = CASE WHEN $3 IN ('running','partial','complete') AND started_at IS NULL
                                  THEN now() ELSE started_at END,
                finished_at = CASE WHEN $3 IN ('complete','partial','void','cancelled')
                                   THEN COALESCE(finished_at, now()) ELSE NULL END
          WHERE tenant_id = $1 AND id = $2
          RETURNING {RUN_COLUMNS}"
    );
    let row: RunRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(next.as_str())
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.into_run()
}

/// Widen (or narrow) a run's plan without leaving its lifecycle.
///
/// This is how a completed pilot becomes the full pass: the same row, a bigger `mode` and `reps`,
/// every result it already wrote kept. A run that is `void` or `cancelled` is not widened —
/// [`EvalStoreError::Illegal`] carries the refused pair.
pub async fn widen_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    mode: RunMode,
    reps: i32,
    case_count: i32,
) -> Result<Run, EvalStoreError> {
    let current = lock_run_tx(conn, tenant, run_id).await?;
    if current.status.is_terminal() {
        return Err(EvalStoreError::Illegal(
            conveyor_domain::eval::EvalError::IllegalRunTransition {
                from: current.status,
                to: RunStatus::Running,
            },
        ));
    }
    let sql = format!(
        "UPDATE eval_run SET mode = $3, reps = $4, case_count = $5
          WHERE tenant_id = $1 AND id = $2
          RETURNING {RUN_COLUMNS}"
    );
    let row: RunRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(mode.as_str())
        .bind(reps.max(1))
        .bind(case_count.max(0))
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.into_run()
}

/// Heartbeat a run. Returns `false` when it no longer exists in this tenant.
pub async fn touch_polled_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    now: DateTime<Utc>,
) -> Result<bool, EvalStoreError> {
    let done = sqlx::query(
        "UPDATE eval_run SET last_polled_at = $3
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Every run the tick is responsible for, oldest first.
///
/// A run is in flight while its status is `queued`, `running` or `partial`. The tick walks them
/// per tenant and finalises each one against [`slot_counts_tx`] and [`outstanding_jobs_tx`].
pub async fn in_flight_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<Run>, EvalStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS}
           FROM eval_run
          WHERE tenant_id = $1 AND status IN ('queued','running','partial')
          ORDER BY created_at ASC, id ASC"
    );
    let rows: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(RunRow::into_run).collect()
}

/// Runs nothing has polled since `cutoff` — the reaper's selection.
///
/// A run that has never been polled is judged from its `created_at`, so a start whose tick never
/// ran is still reaped. Terminal runs are never returned.
pub async fn stale_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cutoff: DateTime<Utc>,
) -> Result<Vec<Run>, EvalStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS}
           FROM eval_run
          WHERE tenant_id = $1
            AND status IN ('queued','running','partial')
            AND COALESCE(last_polled_at, created_at) < $2
          ORDER BY created_at ASC, id ASC"
    );
    let rows: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(cutoff)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(RunRow::into_run).collect()
}

/// How many slots a run has, how many are filled, and how many failed or await a person.
///
/// `expected` is `case_count × reps`; a run created before a `widen` is measured against the
/// current plan, which is what the resume wrote.
pub async fn slot_counts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<SlotCounts, EvalStoreError> {
    let row: Option<(i32, i32, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT r.case_count,
                r.reps,
                (SELECT count(*) FROM eval_case_result c
                  WHERE c.tenant_id = r.tenant_id AND c.eval_run_id = r.id),
                (SELECT count(*) FROM eval_case_result c
                  WHERE c.tenant_id = r.tenant_id AND c.eval_run_id = r.id
                    AND c.status = 'pending_label'),
                (SELECT count(*) FROM eval_attempt_error e
                  WHERE e.tenant_id = r.tenant_id AND e.eval_run_id = r.id),
                (SELECT count(*) FROM eval_case_result c
                  WHERE c.tenant_id = r.tenant_id AND c.eval_run_id = r.id
                    AND c.status = 'truncated')
           FROM eval_run r
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (case_count, reps, filled, pending_label, failed, truncated) =
        row.ok_or(EvalStoreError::NotFound { what: "eval run" })?;
    Ok(SlotCounts {
        expected: i64::from(case_count.max(0)) * i64::from(reps.max(1)),
        filled,
        failed,
        pending_label,
        truncated,
    })
}

/// How many eval jobs for this run are still pending or running.
///
/// The dedupe key is `<run_id>:<case_id>:<rep>`, so the run's jobs are exactly those whose key
/// starts with the run id and the separator. `succeeded` and `dead` are both finished: a dead job
/// leaves its slot empty on purpose (the evidence is in `eval_attempt_error`), which is why
/// "nothing outstanding" — not "every slot filled" — is what finalises a run.
///
/// ⛔ **Both kinds count.** A regrade run's work is `eval.regrade`, never `eval.case.run`, so a
/// query filtered to the latter reported zero outstanding jobs for a run that had just been
/// created and the tick finalised it `void` before a single regrade job ran. The run is the unit
/// (its id is the dedupe prefix), not the kind.
pub async fn outstanding_jobs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<i64, EvalStoreError> {
    let prefix = format!("{run_id}:%");
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1
            AND kind IN ('eval.case.run', 'eval.regrade')
            AND dedupe_key LIKE $2
            AND status IN ('pending','running')",
    )
    .bind(tenant.as_uuid())
    .bind(prefix)
    .fetch_one(&mut *conn)
    .await?;
    Ok(count)
}

async fn lock_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Run, EvalStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM eval_run
          WHERE tenant_id = $1 AND id = $2
          FOR UPDATE"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotFound { what: "eval run" })?
        .into_run()
}
