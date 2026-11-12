#![forbid(unsafe_code)]
//! `eval_climb` — the hillclimb's loop state (S20; SPEC D26, CONTRACT §20).
//!
//! The table shipped in `0026_s19_eval.sql`; this module is its only writer. Four properties are
//! load-bearing and are the reason this is not a plain CRUD layer:
//!
//! - **There is no spend column, and this module never adds one.** Cumulative spend is summed
//!   from the climb's `eval_case_result.usage` and the billed `usage` on `eval_attempt_error`
//!   rows by the action, from rows it already reads. A counter here would drift from those rows
//!   and nothing would notice.
//! - **A round locks the climb before it settles one.** [`lock_climb_tx`] is `SELECT … FOR
//!   UPDATE`, so two concurrent `eval_climbs.round` calls cannot both settle round N and both
//!   advance the counter — the second waits, then sees the advanced state.
//! - **Variant numbering continues across climbs.** [`next_variant_tx`] reads the highest existing
//!   `v<N>` for the eval rather than restarting at 1, because `eval_run_variant_unique` is
//!   `(tenant, eval, variant)`: a second climb on the same eval would otherwise collide on `v1`
//!   and the only alternative — one climb per eval forever — makes an applied winner a dead end.
//! - **The gates are `approval` rows** (`kind='eval_climb'`), read through the same partial unique
//!   indexes every other gate uses. `eval_climb_plan` carries `eval_id`, so `uq_appr_eval_pending`
//!   guarantees one pending plan per eval; `eval_round` carries `run_id`, so `uq_appr_pending`
//!   guarantees one pending round per run.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, evals::version_digest_tx, map_write_error};

/// The gate a climb's plan is signed through.
pub const PLAN_GATE: &str = "eval_climb_plan";

/// The gate one round is approved through, when `approve_each_round` is set.
pub const ROUND_GATE: &str = "eval_round";

/// The `approval.kind` every climb gate carries.
pub const GATE_KIND: &str = "eval_climb";

/// The public projection of an `eval_climb` row.
#[derive(Clone, Debug)]
pub struct Climb {
    /// The row id.
    pub id: Uuid,
    /// The eval being climbed.
    pub eval_id: Uuid,
    /// The frozen instrument every round runs under.
    pub version_id: Uuid,
    /// `{target, direction, hold[], lever, stop}` — the method's `_state.json`.
    pub goal: Value,
    /// `higher | lower`. Mirrors `goal.direction`; the column is what a query can read.
    pub direction: String,
    /// Repetitions per case, for every round.
    pub reps: i32,
    /// `plateau | rounds | manual`.
    pub stopping: String,
    /// How many flat rounds end the climb (>= 3).
    pub plateau_k: i32,
    /// The round count, when `stopping = rounds`.
    pub max_rounds: Option<i32>,
    /// The noise band, set above the measured floor.
    pub delta: Decimal,
    /// Patch path prefixes this climb must not touch.
    pub off_limits: Vec<String>,
    /// Whether every round needs its own sign-off.
    pub approve_each_round: bool,
    /// The spend ceiling, when the caller set one.
    pub cost_ceiling_cents: Option<i32>,
    /// How many rounds have been created.
    pub current_round: i32,
    /// The round with the best held-out score, once one is settled.
    pub best_round: Option<i32>,
    /// That round's held-out score.
    pub best_test_score: Option<Decimal>,
    /// `planned | running | plateaued | stopped`.
    pub status: String,
    /// The final report, written once by `eval_climbs.stop`.
    pub narrative_md: Option<String>,
    /// Who started it.
    pub created_by: Option<Uuid>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

/// An `eval_climb` to create.
#[derive(Clone, Debug)]
pub struct NewClimb<'a> {
    /// The eval being climbed.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// The canonical goal document.
    pub goal: &'a Value,
    /// `higher | lower`.
    pub direction: &'a str,
    /// Repetitions per case.
    pub reps: i32,
    /// `plateau | rounds | manual`.
    pub stopping: &'a str,
    /// Flat rounds that end a plateau.
    pub plateau_k: i32,
    /// The round count for `stopping = rounds`.
    pub max_rounds: Option<i32>,
    /// The noise band.
    pub delta: Decimal,
    /// Path prefixes the climb must not touch.
    pub off_limits: &'a [String],
    /// Whether each round needs its own sign-off.
    pub approve_each_round: bool,
    /// The spend ceiling.
    pub cost_ceiling_cents: Option<i32>,
    /// Who started it.
    pub created_by: Option<Uuid>,
}

const COLUMNS: &str = "id, eval_id, version_id, goal, direction, reps, stopping, plateau_k, \
                       max_rounds, delta, off_limits, approve_each_round, cost_ceiling_cents, \
                       current_round, best_round, best_test_score, status, narrative_md, \
                       created_by, created_at, updated_at";

#[derive(Debug, sqlx::FromRow)]
struct ClimbRow {
    id: Uuid,
    eval_id: Uuid,
    version_id: Uuid,
    goal: Value,
    direction: String,
    reps: i32,
    stopping: String,
    plateau_k: i32,
    max_rounds: Option<i32>,
    delta: Decimal,
    off_limits: Vec<String>,
    approve_each_round: bool,
    cost_ceiling_cents: Option<i32>,
    current_round: i32,
    best_round: Option<i32>,
    best_test_score: Option<Decimal>,
    status: String,
    narrative_md: Option<String>,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ClimbRow {
    fn into_climb(self) -> Climb {
        Climb {
            id: self.id,
            eval_id: self.eval_id,
            version_id: self.version_id,
            goal: self.goal,
            direction: self.direction,
            reps: self.reps,
            stopping: self.stopping,
            plateau_k: self.plateau_k,
            max_rounds: self.max_rounds,
            delta: self.delta,
            off_limits: self.off_limits,
            approve_each_round: self.approve_each_round,
            cost_ceiling_cents: self.cost_ceiling_cents,
            current_round: self.current_round,
            best_round: self.best_round,
            best_test_score: self.best_test_score,
            status: self.status,
            narrative_md: self.narrative_md,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Create a climb, `planned`, awaiting its plan sign-off.
pub async fn insert_climb_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewClimb<'_>,
) -> Result<Climb, EvalStoreError> {
    let sql = format!(
        "INSERT INTO eval_climb (tenant_id, eval_id, version_id, goal, direction, reps, stopping,
                                 plateau_k, max_rounds, delta, off_limits, approve_each_round,
                                 cost_ceiling_cents, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING {COLUMNS}"
    );
    let row: ClimbRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.eval_id)
        .bind(input.version_id)
        .bind(input.goal)
        .bind(input.direction)
        .bind(input.reps)
        .bind(input.stopping)
        .bind(input.plateau_k)
        .bind(input.max_rounds)
        .bind(input.delta)
        .bind(input.off_limits)
        .bind(input.approve_each_round)
        .bind(input.cost_ceiling_cents)
        .bind(input.created_by)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row.into_climb())
}

/// One climb by id, in this tenant.
pub async fn get_climb_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Climb>, EvalStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM eval_climb WHERE tenant_id = $1 AND id = $2");
    let row: Option<ClimbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(ClimbRow::into_climb))
}

/// The climb that is still open on `eval_id`, if any.
///
/// `stopped` is terminal and does not block a new climb; `planned`, `running` and `plateaued` all
/// do, because each of them owns the next variant number and a second writer would be a second
/// keeper of the same best-so-far.
pub async fn find_open_climb_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
) -> Result<Option<Climb>, EvalStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM eval_climb
          WHERE tenant_id = $1 AND eval_id = $2 AND status <> 'stopped'
          ORDER BY created_at DESC
          LIMIT 1"
    );
    let row: Option<ClimbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(ClimbRow::into_climb))
}

/// A page of climbs, newest first. `uuidv7` ids are time-ordered, so the id is the cursor.
pub async fn list_climbs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Option<Uuid>,
    status: Option<&str>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Climb>, EvalStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM eval_climb
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR eval_id = $2)
            AND ($3::text IS NULL OR status = $3)
            AND ($4::uuid IS NULL OR id < $4)
          ORDER BY id DESC
          LIMIT $5"
    );
    let rows: Vec<ClimbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(status)
        .bind(cursor)
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(ClimbRow::into_climb).collect())
}

/// Lock the climb row for the rest of the transaction.
///
/// Every round and every stop takes this first: the settle-read-advance sequence is a
/// read-modify-write over `current_round` and `best_round`, and two rounds that both read round 2
/// would both write round 3.
pub async fn lock_climb_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Climb>, EvalStoreError> {
    let sql =
        format!("SELECT {COLUMNS} FROM eval_climb WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<ClimbRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(ClimbRow::into_climb))
}

/// The loop state a settle writes back.
#[derive(Clone, Debug)]
pub struct ClimbAdvance<'a> {
    /// How many rounds have been created.
    pub current_round: i32,
    /// The round with the best held-out score.
    pub best_round: Option<i32>,
    /// That round's held-out score.
    pub best_test_score: Option<Decimal>,
    /// The status the climb is now in.
    pub status: &'a str,
    /// The goal, with any advanced lever or recorded stop.
    pub goal: &'a Value,
}

/// Write the loop state after a round settles (or the climb reaches its stopping condition).
pub async fn advance_climb_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    advance: &ClimbAdvance<'_>,
) -> Result<Climb, EvalStoreError> {
    let sql = format!(
        "UPDATE eval_climb
            SET current_round = $3, best_round = $4, best_test_score = $5, status = $6,
                goal = $7, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {COLUMNS}"
    );
    let row: ClimbRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(advance.current_round)
        .bind(advance.best_round)
        .bind(advance.best_test_score)
        .bind(advance.status)
        .bind(advance.goal)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row.into_climb())
}

/// Close the climb and write its report.
pub async fn stop_climb_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    goal: &Value,
    narrative_md: &str,
    best_round: Option<i32>,
    best_test_score: Option<Decimal>,
) -> Result<Climb, EvalStoreError> {
    let sql = format!(
        "UPDATE eval_climb
            SET status = 'stopped', goal = $3, narrative_md = $4, best_round = $5,
                best_test_score = $6, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {COLUMNS}"
    );
    let row: ClimbRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(goal)
        .bind(narrative_md)
        .bind(best_round)
        .bind(best_test_score)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row.into_climb())
}

/// The next `v<N>` for this eval, **continuing** the existing numbering.
///
/// `baseline` and any non-numeric variant are ignored; a gap in the numbering is not reused,
/// because a reused variant would silently merge two rounds' results under one row.
pub async fn next_variant_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
) -> Result<i32, EvalStoreError> {
    let highest: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(substring(variant from 2)::int)
           FROM eval_run
          WHERE tenant_id = $1 AND eval_id = $2 AND variant ~ '^v[0-9]+$'",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(highest.unwrap_or(0) + 1)
}

/// Write a round's change and prediction onto its run row, and mark it as a round.
///
/// The `summary` is merged rather than replaced, so a later settle can append the decision
/// without reading first.
pub async fn set_run_round_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    round: i32,
    change_md: &str,
    change_patch: Option<&str>,
    summary: &Value,
) -> Result<(), EvalStoreError> {
    sqlx::query(
        "UPDATE eval_run
            SET round = $3, change_md = $4, change_patch = $5, summary = summary || $6
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(round)
    .bind(change_md)
    .bind(change_patch)
    .bind(summary)
    .execute(&mut *conn)
    .await
    .map_err(map_write_error)?;
    Ok(())
}

/// Append to a run's `summary` (the settle's decision, the judge-jump flag, …).
pub async fn merge_run_summary_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    patch: &Value,
) -> Result<(), EvalStoreError> {
    sqlx::query("UPDATE eval_run SET summary = summary || $3 WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(patch)
        .execute(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(())
}

/// A run's measurement identity: the version it names, the instrument digest **as it is now**, and
/// the split it ran under.
pub struct RunSignatureRow {
    /// The eval the run belongs to.
    pub eval_id: Uuid,
    /// The version the run names.
    pub version_id: Uuid,
    /// The run's variant, for a refusal that names the rows.
    pub variant: String,
    /// Recomputed from the version's current rows by [`version_digest_tx`].
    pub digest: String,
    /// The version's locked split hash.
    pub split_sha256: Option<String>,
}

/// Read a run's signature, or `None` when the run is not in this tenant.
pub async fn run_signature_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<RunSignatureRow>, EvalStoreError> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT eval_id, version_id, variant FROM eval_run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((eval_id, version_id, variant)) = row else {
        return Ok(None);
    };
    let digest = version_digest_tx(&mut *conn, tenant, version_id).await?;
    let split: Option<(Option<String>,)> =
        sqlx::query_as("SELECT split_sha256 FROM eval_version WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(version_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(Some(RunSignatureRow {
        eval_id,
        version_id,
        variant,
        digest,
        split_sha256: split.and_then(|(hash,)| hash),
    }))
}

/// The split of every stored transcript among `ids`, as `(trace id, split)`.
///
/// The analyzer-never-reads-test rule is a **data access** wall, not a prompt instruction
/// (CONTRACT §20). A round hands over the trace ids it was given; this read is what turns them
/// into the split they belong to, so the round can be refused before it runs. An id that names no
/// trace in this tenant is simply absent — the caller cannot learn about another tenant's traces.
pub async fn trace_splits_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, EvalStoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT t.id, k.split
           FROM eval_trace t
           JOIN eval_case k ON k.tenant_id = t.tenant_id AND k.id = t.case_id
          WHERE t.tenant_id = $1 AND t.id = ANY($2)
          ORDER BY t.id",
    )
    .bind(tenant.as_uuid())
    .bind(ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

// ── gates ────────────────────────────────────────────────────────────────────────────────
//
// The two reads below are `approval.rs`'s `workflow_gate_tx` shape, scoped to `kind =
// 'eval_climb'`. They live here rather than in `approval.rs` so S20 adds no SQL to a file three
// other slices write; the index that guarantees one pending gate is the same one either way.

/// The latest status of one climb gate, if a gate was ever opened for it.
pub async fn plan_gate_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
) -> Result<Option<String>, EvalStoreError> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM approval
          WHERE tenant_id = $1 AND eval_id = $2 AND kind = $3 AND gate = $4
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(GATE_KIND)
    .bind(PLAN_GATE)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(status)
}

/// The latest status of one round's gate, if a gate was ever opened for it.
///
/// ⛔ **Keyed by `(eval_id, round)`, never by `run_id`.** `approval.run_id` is a foreign key to
/// the control-plane `run` table, not to `eval_run`, so an eval run's id cannot be stored there —
/// the insert is refused by the FK. `uq_appr_eval_pending (tenant, eval_id, gate)` is what
/// guarantees one pending round gate per eval, and `meta->>'round'` names which round it is.
pub async fn round_gate_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    round: i32,
) -> Result<Option<String>, EvalStoreError> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM approval
          WHERE tenant_id = $1 AND eval_id = $2 AND kind = $3 AND gate = $4
            AND meta->>'round' = $5
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(GATE_KIND)
    .bind(ROUND_GATE)
    .bind(round.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    Ok(status)
}

/// The approved round gate for one round, when it exists.
pub async fn approved_round_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    round: i32,
) -> Result<Option<Uuid>, EvalStoreError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND eval_id = $2 AND kind = $3 AND gate = $4
            AND meta->>'round' = $5 AND status = 'approved'
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(GATE_KIND)
    .bind(ROUND_GATE)
    .bind(round.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    Ok(id)
}

/// The approved climb gate for one eval, when it exists.
pub async fn approved_plan_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
) -> Result<Option<Uuid>, EvalStoreError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND eval_id = $2 AND kind = $3 AND gate = $4
            AND status = 'approved'
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(GATE_KIND)
    .bind(PLAN_GATE)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(id)
}
