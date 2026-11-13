//! Runs: create, read and transition.
//!
//! The database enforces *one live run per card* with a partial unique index; the transition
//! table lives in `conveyor_domain::run`. `transition_run_tx` reads the row `FOR UPDATE`,
//! asks the domain, then writes with the old status still in the `WHERE` — so a status that
//! moved underneath cannot be overwritten even if the row lock were somehow lost.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::{Run, RunError, RunStatus, TenantId};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};
use crate::version_of;

/// A run to create.
#[derive(Clone, Debug)]
pub struct NewRun {
    /// The card this run executes, when it is card-scoped.
    pub card_id: Option<Uuid>,
    /// The owning project.
    pub project_id: Uuid,
    /// The attempt number; `1` unless a caller is replaying an attempt.
    pub attempt: i32,
    /// A correlation id linking the run to its trigger.
    pub correlation_id: Option<Uuid>,
    /// The published process version this run executes, when it is process-driven.
    pub process_id: Option<Uuid>,
    /// The caller-supplied run inputs (`process_runs.start`); `{}` for an ordinary run.
    pub inputs: serde_json::Value,
    /// The capability the run executes under (D22/§17). `None` until S15 fills the resolver.
    pub capability_id: Option<Uuid>,
    /// The capability agent configuration the run was resolved against; `None` with the capability.
    pub capability_agent_id: Option<Uuid>,
    /// The run that started this one; `None` for a card/project ROOT (0033).
    pub parent_run_id: Option<Uuid>,
    /// The `process_step` that started this child, when the parent is a PROCESS run; required
    /// whenever `parent_run_id` is set and the parent is a process.
    pub step_id: Option<Uuid>,
    /// The `run_step` that started this child, when the parent is a WORKFLOW run (0037). Exactly
    /// one of `step_id` / `workflow_step_id` is set on a child.
    pub workflow_step_id: Option<Uuid>,
    /// 0 for a root, `parent.depth + 1` below. The database allows 0..=3.
    pub depth: i32,
    /// The process version the run pinned (CONTRACT §21); `1` for a non-process run.
    pub process_version: i64,
    /// The job binding this run names (T0, §4.4). `None` for a run with no organisation pin.
    pub job_binding_id: Option<Uuid>,
    /// The release the caller claims the binding pins. Must equal the binding's pin; the store
    /// refuses a mismatch `409 job_binding_stale` before the INSERT.
    pub release_hash: Option<String>,
}

impl NewRun {
    /// A first attempt for `project` (optionally on `card`).
    pub fn new(project_id: Uuid, card_id: Option<Uuid>) -> Self {
        Self {
            card_id,
            project_id,
            attempt: 1,
            correlation_id: None,
            process_id: None,
            inputs: serde_json::Value::Object(serde_json::Map::new()),
            capability_id: None,
            capability_agent_id: None,
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
            depth: 0,
            process_version: 1,
            job_binding_id: None,
            release_hash: None,
        }
    }
}

/// Why a run operation failed.
#[derive(Debug, thiserror::Error)]
pub enum RunStoreError {
    /// No such run in this tenant.
    #[error("the run does not exist in this tenant")]
    NotFound,
    /// The card already has a live run (`uq_run_live`).
    #[error("the card already has a live run")]
    LiveRunExists,
    /// The domain refused the transition.
    #[error(transparent)]
    Illegal(#[from] RunError),
    /// A stored vocabulary value no longer parses.
    #[error("the database holds run.status = {0:?}, which the domain does not know")]
    UnknownStatus(String),
    /// A referenced parent (`card_id`, `project_id`) does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The parent named for a child is absent or not live.
    #[error("the parent run is not live")]
    ParentNotLive,
    /// A child must run on its parent's card — the card is the work package.
    #[error("a child run must be on the same card as its parent")]
    ChildCardMismatch,
    /// The run tree is at most four levels (`depth` 0..=3).
    #[error("the run tree is at most 3 levels deep")]
    DepthExceeded,
    /// A child names no `process_step`; the parent alone is not a tree edge.
    #[error("a child run must name the process step that started it")]
    ChildWithoutStep,
    /// A child named BOTH a process step and a workflow step; exactly one is the edge.
    #[error("a child run names both a process step and a workflow step")]
    ChildStepAmbiguous,
    /// The named `process_step` does not exist in this tenant.
    #[error("the process step does not exist in this tenant")]
    StepNotFound,
    /// The `(parent, step)` already has a live child (`uq_run_child_live`).
    #[error("the parent step already has a live child run")]
    LiveChildExists,
    /// The `(parent, step, attempt)` already exists (`uq_run_child_attempt`).
    #[error("the parent step already has a run at that attempt")]
    ChildAttemptExists,
    /// The run named a retired (or draft) binding, or a release that is not its pin
    /// (`409 job_binding_stale`, SPEC_V2 §4.4).
    #[error(
        "job_binding_stale: the binding pins release {binding_hash:?} but the run requested \
         {requested_hash:?}"
    )]
    JobBindingStale {
        /// The release hash the binding pins.
        binding_hash: String,
        /// The hash the caller named (the binding's own when the caller named none).
        requested_hash: String,
    },
    /// A run that names a release must name the binding that pins it.
    #[error("a run that names a release must name its job binding")]
    ReleaseWithoutBinding,
    /// The named binding does not exist in this tenant.
    #[error("the job binding does not exist in this tenant")]
    BindingNotFound,
    /// The organisation spine refused the binding lookup.
    #[error(transparent)]
    Organisation(Box<crate::organisation::OrganisationStoreError>),
    /// The step bookkeeping the failure/cancel path also owes (one run tree is one unit of
    /// work, so failing a run skips its queued steps and cancels its live descendants).
    #[error(transparent)]
    Process(#[from] crate::process::ProcessStoreError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<crate::organisation::OrganisationStoreError> for RunStoreError {
    fn from(error: crate::organisation::OrganisationStoreError) -> Self {
        use crate::organisation::OrganisationStoreError as Organisation;
        match error {
            Organisation::StaleRelease {
                binding_hash,
                requested_hash,
            } => RunStoreError::JobBindingStale {
                binding_hash,
                requested_hash,
            },
            Organisation::NotFound("binding") => RunStoreError::BindingNotFound,
            Organisation::Database(error) => RunStoreError::Database(error),
            other => RunStoreError::Organisation(Box::new(other)),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    card_id: Option<Uuid>,
    project_id: Uuid,
    status: String,
    attempt: i32,
    error_class: Option<String>,
    error_message: Option<String>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    version: i64,
    parent_run_id: Option<Uuid>,
    step_id: Option<Uuid>,
    depth: i32,
    process_version: i64,
}

impl TryFrom<RunRow> for Run {
    type Error = RunStoreError;
    fn try_from(row: RunRow) -> Result<Self, Self::Error> {
        let status = RunStatus::from_str(&row.status)
            .map_err(|_| RunStoreError::UnknownStatus(row.status))?;
        Ok(Self {
            id: row.id,
            card_id: row.card_id,
            project_id: row.project_id,
            status,
            attempt: row.attempt,
            error_class: row.error_class,
            error_message: row.error_message,
            started_at: row.started_at,
            finished_at: row.finished_at,
            version: version_of(row.version),
        })
    }
}

const COLUMNS: &str = "id, card_id, project_id, status, attempt, error_class, error_message, \
                       started_at, finished_at, version, parent_run_id, step_id, depth, \
                       process_version, workflow_step_id";

/// One run row together with its position in the run tree (0033).
///
/// [`Run`] is the domain aggregate and deliberately carries no tree state; a caller that needs
/// the edge or the depth (a child's `parent_run_id`, the depth the insert derived) reads them
/// here.
#[derive(Clone, Debug)]
pub struct RunNode {
    /// The domain value.
    pub run: Run,
    /// The run that started this one; `None` for a ROOT.
    pub parent_run_id: Option<Uuid>,
    /// The `process_step` that started this child; `None` on a ROOT.
    pub step_id: Option<Uuid>,
    /// 0 for a root, `parent.depth + 1` below (the database allows 0..=3).
    pub depth: i32,
    /// The process version the run pinned.
    pub process_version: i64,
}

impl TryFrom<RunRow> for RunNode {
    type Error = RunStoreError;
    fn try_from(row: RunRow) -> Result<Self, Self::Error> {
        let parent_run_id = row.parent_run_id;
        let step_id = row.step_id;
        let depth = row.depth;
        let process_version = row.process_version;
        Ok(Self {
            run: Run::try_from(row)?,
            parent_run_id,
            step_id,
            depth,
            process_version,
        })
    }
}

/// Read one run with its position in the run tree.
pub async fn get_run_node_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RunNode>, RunStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM run WHERE tenant_id = $1 AND id = $2");
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(RunNode::try_from).transpose()
}

/// The output of a run's last SUCCEEDED step, when it has one.
///
/// This is the value a `call` step hands its parent: a child process's final step output (or a
/// child workflow's final node output) is the work the child produced, and `run` itself carries
/// no output column — the trace is the record. A run with no succeeded step answers `None`, and
/// the parent records a child result without an output rather than inventing one.
pub async fn last_step_output_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT output FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND status = 'succeeded'
          ORDER BY position DESC, attempt DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await
}

/// One `run` row with the read projection's sort key.
///
/// [`Run`] is the domain value and carries no `created_at`; the list projection sorts and
/// pages on it, so it travels beside the value rather than being tacked onto the aggregate.
#[derive(Clone, Debug)]
pub struct RunRecord {
    /// The domain value.
    pub run: Run,
    /// The process step that started this run, when it is a child of a process parent.
    pub process_step_id: Option<Uuid>,
    /// The workflow node that started this run, when it is a child of a workflow parent (0037).
    pub workflow_step_id: Option<Uuid>,
    /// When the row was created (the page's sort key).
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct RunRecordRow {
    id: Uuid,
    card_id: Option<Uuid>,
    project_id: Uuid,
    status: String,
    attempt: i32,
    error_class: Option<String>,
    error_message: Option<String>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    version: i64,
    /// The `run.step_id` edge (a process parent's step).
    step_id: Option<Uuid>,
    /// The `run.workflow_step_id` edge (a workflow parent's node, 0037).
    workflow_step_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl TryFrom<RunRecordRow> for RunRecord {
    type Error = RunStoreError;
    fn try_from(row: RunRecordRow) -> Result<Self, Self::Error> {
        let status = RunStatus::from_str(&row.status)
            .map_err(|_| RunStoreError::UnknownStatus(row.status))?;
        Ok(Self {
            run: Run {
                id: row.id,
                card_id: row.card_id,
                project_id: row.project_id,
                status,
                attempt: row.attempt,
                error_class: row.error_class,
                error_message: row.error_message,
                started_at: row.started_at,
                finished_at: row.finished_at,
                version: version_of(row.version),
            },
            process_step_id: row.step_id,
            workflow_step_id: row.workflow_step_id,
            created_at: row.created_at,
        })
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

fn constraint_of(error: &sqlx::Error) -> Option<&str> {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
}

/// Insert a queued run inside the caller's scoped transaction.
///
/// `project_id` is required and resolved in the caller's tenant; `card_id` is optional and
/// resolved when present (CONTRACT §2 — the FK alone does not enforce tenancy).
///
/// A run that names a `job_binding_id` resolves its pin HERE, before the INSERT: a retired or
/// draft binding, or a `release_hash` that is not the binding's pin, is
/// [`RunStoreError::JobBindingStale`] and nothing is written (SPEC_V2 §4.4, T0 P2.2).
pub async fn insert_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewRun,
) -> Result<Run, RunStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Project, input.project_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::Card, input.card_id).await?;
    let (job_binding_id, release_hash) = match input.job_binding_id {
        Some(binding_id) => {
            let (id, hash) = crate::organisation::resolve_binding_pin_tx(
                conn,
                tenant,
                binding_id,
                input.release_hash.as_deref(),
            )
            .await?;
            (Some(id), Some(hash))
        }
        None if input.release_hash.is_some() => return Err(RunStoreError::ReleaseWithoutBinding),
        None => (None, None),
    };
    let sql = format!(
        "INSERT INTO run (tenant_id, card_id, project_id, status, attempt, correlation_id,
                          process_id, inputs, capability_id, capability_agent_id,
                          parent_run_id, step_id, depth, process_version,
                          job_binding_id, release_hash)
         VALUES ($1, $2, $3, 'queued', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         RETURNING {COLUMNS}"
    );
    let row: RunRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.card_id)
        .bind(input.project_id)
        .bind(input.attempt)
        .bind(input.correlation_id)
        .bind(input.process_id)
        .bind(&input.inputs)
        .bind(input.capability_id)
        .bind(input.capability_agent_id)
        .bind(input.parent_run_id)
        .bind(input.step_id)
        .bind(input.depth)
        .bind(input.process_version)
        .bind(job_binding_id)
        .bind(release_hash.as_deref())
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                RunStoreError::LiveRunExists
            } else {
                RunStoreError::Database(error)
            }
        })?;
    row.try_into()
}

/// Insert a CHILD run under a live parent, inside the caller's scoped transaction.
///
/// The tree's rules are checked here and then enforced by the database: the parent must be
/// `queued | running | waiting_approval`, the child must be on the parent's card, the depth is
/// `parent.depth + 1` and may not pass 3 (`run.depth` CHECK), and `step_id` is required
/// (`run_parent_step_check`). The input's `parent_run_id`/`depth` are ignored — the parameter
/// and the parent's depth are authoritative.
///
/// The two partial unique indexes are mapped to their own errors rather than the root's
/// [`RunStoreError::LiveRunExists`]: a second live child for one `(parent, step)` is a refused
/// retry (`uq_run_child_live`), and a repeat of one attempt is `uq_run_child_attempt`.
pub async fn insert_child_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    parent_run_id: Uuid,
    input: &NewRun,
) -> Result<Run, RunStoreError> {
    // `FOR UPDATE` is what makes the parent's liveness and the child's insert one decision: a
    // concurrent `runs.cancel` (or the reaper) waits for this transaction, so a child can never
    // commit after its parent was cancelled — the insert then sees `cancelled` and refuses.
    let parent: Option<(Option<Uuid>, i32, String, Option<Uuid>, Option<String>)> = sqlx::query_as(
        "SELECT card_id, depth, status, job_binding_id, release_hash
           FROM run WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(parent_run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((parent_card_id, parent_depth, parent_status, parent_binding, parent_release)) = parent
    else {
        return Err(RunStoreError::NotFound);
    };
    let status = RunStatus::from_str(&parent_status)
        .map_err(|_| RunStoreError::UnknownStatus(parent_status))?;
    if status.is_terminal() {
        return Err(RunStoreError::ParentNotLive);
    }
    if input.card_id != parent_card_id {
        return Err(RunStoreError::ChildCardMismatch);
    }
    let depth = parent_depth.saturating_add(1);
    if depth > 3 {
        return Err(RunStoreError::DepthExceeded);
    }
    // The edge is one step of the PARENT's own kind: a process `call` names a `process_step`, a
    // workflow's procedure node names a `run_step` of the parent run (0037). A generic uuid was
    // refused before the fix, which is why a workflow could not start a process at all.
    let (step_id, workflow_step_id) = match (input.step_id, input.workflow_step_id) {
        (Some(_), Some(_)) => return Err(RunStoreError::ChildStepAmbiguous),
        (None, None) => return Err(RunStoreError::ChildWithoutStep),
        (Some(step_id), None) => {
            // The FK would accept another tenant's `process_step`; resolve it in scope first.
            let step_present: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM process_step WHERE tenant_id = $1 AND id = $2)",
            )
            .bind(tenant.as_uuid())
            .bind(step_id)
            .fetch_one(&mut *conn)
            .await?;
            if !step_present {
                return Err(RunStoreError::StepNotFound);
            }
            (Some(step_id), None)
        }
        (None, Some(workflow_step_id)) => {
            // A workflow step must be a row of THIS parent run, not merely an existing run_step.
            let step_present: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM run_step
                                 WHERE tenant_id = $1 AND id = $2 AND run_id = $3)",
            )
            .bind(tenant.as_uuid())
            .bind(workflow_step_id)
            .bind(parent_run_id)
            .fetch_one(&mut *conn)
            .await?;
            if !step_present {
                return Err(RunStoreError::StepNotFound);
            }
            (None, Some(workflow_step_id))
        }
    };
    ensure_parent_tx(conn, tenant, Parent::Project, input.project_id).await?;
    let sql = format!(
        "INSERT INTO run (tenant_id, card_id, project_id, status, attempt, correlation_id,
                          process_id, inputs, capability_id, capability_agent_id,
                          parent_run_id, step_id, workflow_step_id, depth, process_version,
                          job_binding_id, release_hash)
         VALUES ($1, $2, $3, 'queued', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                 $16)
         RETURNING {COLUMNS}"
    );
    let row: RunRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.card_id)
        .bind(input.project_id)
        .bind(input.attempt)
        .bind(input.correlation_id)
        .bind(input.process_id)
        .bind(&input.inputs)
        .bind(input.capability_id)
        .bind(input.capability_agent_id)
        .bind(parent_run_id)
        .bind(step_id)
        .bind(workflow_step_id)
        .bind(depth)
        .bind(input.process_version)
        // The parent's pin is authoritative: a child cannot change the binding mid-tree, and
        // `input.job_binding_id`/`release_hash` are ignored exactly like `depth`.
        .bind(parent_binding)
        .bind(parent_release.as_deref())
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if !is_unique_violation(&error) {
                return RunStoreError::Database(error);
            }
            match constraint_of(&error) {
                Some("uq_run_child_attempt" | "uq_run_child_attempt_workflow") => {
                    RunStoreError::ChildAttemptExists
                }
                Some("uq_run_child_live" | "uq_run_child_live_workflow") => {
                    RunStoreError::LiveChildExists
                }
                _ => RunStoreError::LiveRunExists,
            }
        })?;
    // The caller gets the domain value; the edge is on the returned row and in the input.
    Ok(RunNode::try_from(row)?.run)
}

/// Read one run inside the caller's scoped transaction.
pub async fn get_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Run>, RunStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM run WHERE tenant_id = $1 AND id = $2");
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Run::try_from).transpose()
}

/// Apply a run transition inside the caller's scoped transaction.
///
/// A terminal run is refused by the domain before any write. `started_at` is set by the
/// database clock on the transition to `running`; `finished_at` on the transition to a
/// terminal status. Both are `coalesce`d, so a replayed step does not overwrite the first
/// instant.
pub async fn transition_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    to: RunStatus,
) -> Result<Run, RunStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM run WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await?;
    let run: Run = row.ok_or(RunStoreError::NotFound)?.try_into()?;
    let from = run.status;
    let next = run.transition(to)?;

    let sql = format!(
        "UPDATE run
            SET status = $3, version = version + 1, updated_at = now(),
                started_at = CASE WHEN $3 = 'running'
                                  THEN coalesce(started_at, now()) ELSE started_at END,
                finished_at = CASE WHEN $3 IN ('succeeded', 'failed', 'cancelled')
                                   THEN coalesce(finished_at, now()) ELSE finished_at END
          WHERE tenant_id = $1 AND id = $2 AND status = $4
         RETURNING {COLUMNS}"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(next.status.as_str())
        .bind(from.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    // The row was locked and the domain agreed; a zero-row write would mean the lock did not
    // hold, which is a bug rather than a business outcome.
    let run = row
        .map(Run::try_from)
        .transpose()?
        .ok_or(RunStoreError::NotFound)?;
    enqueue_memory_consolidation(conn, tenant, &run).await?;
    Ok(run)
}

/// Enqueue the card's memory consolidation when a run reaches a terminal state (S15).
///
/// One job per card: the dedupe key is deliberately the card, not `(card, run)` — two runs that
/// finish together collapse into one consolidation, which reads every un-consolidated row anyway.
/// The job is idempotent, so a replay after a crash writes no second document.
async fn enqueue_memory_consolidation(
    conn: &mut PgConnection,
    tenant: TenantId,
    run: &Run,
) -> Result<(), RunStoreError> {
    if !run.status.is_terminal() {
        return Ok(());
    }
    let Some(card_id) = run.card_id else {
        // A project-scoped run has no work package.
        return Ok(());
    };
    crate::jobs::enqueue_tx(
        conn,
        tenant,
        "memory.consolidate",
        &format!("memory.consolidate:{card_id}"),
        serde_json::json!({"card_id": card_id, "run_id": run.id}),
        None,
        1,
    )
    .await?;
    Ok(())
}

/// The process a run executes, if any. The domain `Run` value carries no process id (it is a
/// lifecycle aggregate), so an action that must know whether the executor owns the run asks here.
pub async fn process_of_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<Uuid>, RunStoreError> {
    let row: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT process_id FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.flatten())
}

/// Fail a run and record why. `None` means the row is absent or already terminal.
///
/// The guard is `NOT terminal`, not version-based (CONTRACT §3: a status transition is
/// status-guarded). It deliberately admits `queued` as well as `running`: a run whose
/// `run.execute` job died before it started has to be fail-closed too, or the card's live-run
/// slot would be held by a run nobody is executing.
pub async fn fail_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    error_class: Option<&str>,
    error_message: Option<&str>,
) -> Result<Option<Run>, RunStoreError> {
    let sql = format!(
        "UPDATE run
            SET status = 'failed', error_class = $3, error_message = $4,
                version = version + 1, updated_at = now(),
                finished_at = coalesce(finished_at, now())
          WHERE tenant_id = $1 AND id = $2
            AND status NOT IN ('succeeded', 'failed', 'cancelled')
        RETURNING {COLUMNS}"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(error_class)
        .bind(error_message)
        .fetch_optional(&mut *conn)
        .await?;
    let run = row.map(Run::try_from).transpose()?;
    if let Some(run) = &run {
        // The run tree is one unit of work on the failure path too: a live child under a failed
        // root would keep running (and holding `uq_run_child_live`) for a parent nobody waits
        // for, and the root's own queued steps would read as "still to do" forever.
        crate::process::skip_queued_run_steps_tx(conn, tenant, run.id).await?;
        cancel_descendants_tx(conn, tenant, run.id).await?;
        enqueue_memory_consolidation(conn, tenant, run).await?;
    }
    Ok(run)
}

/// One keyset page of runs, newest first.
///
/// `cursor` is the decoded `(created_at, id)` of the previous page's last row — the action
/// layer encodes and decodes it with [`crate::actions::encode_time_cursor`] /
/// [`crate::actions::decode_time_cursor`] (CONTRACT §5). The predicate is the row-value form
/// `(created_at, id) < (…)`, so the `run_created_idx` / `run_card_created_idx` indexes serve
/// it and a concurrent insert cannot make a later page skip a row.
pub async fn list_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    card_id: Option<Uuid>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<RunRecord>, RunStoreError> {
    let sql = format!(
        "SELECT {COLUMNS}, created_at
           FROM run
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::uuid IS NULL OR card_id = $3)
            AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
          ORDER BY created_at DESC, id DESC
          LIMIT $6"
    );
    let rows: Vec<RunRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(card_id)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(RunRecord::try_from).collect()
}

/// The card's live ROOT run, if it has one.
///
/// "Live" is the database's definition — `queued | running | waiting_approval` — and the
/// partial unique index `uq_run_live` ensures there is at most one. Since 0033 that index is
/// **root-only**: a child run shares its parent's card, so it must never be reported as the
/// card's run (`parent_run_id IS NULL`). A terminal run is history and never matches, which is
/// what lets `runs.create` retry.
pub async fn live_run_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<Run>, RunStoreError> {
    let sql = format!(
        "SELECT {COLUMNS}
           FROM run
          WHERE tenant_id = $1 AND card_id = $2 AND parent_run_id IS NULL
            AND status IN ('queued', 'running', 'waiting_approval')"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Run::try_from).transpose()
}

/// The project's live card-less execution run, if one exists.
///
/// `card_id IS NULL` is the discriminator: a card-scoped run and a project execution can both
/// be live in the same project, and `uq_run_live` does not see the card-less one at all.
pub async fn live_project_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Option<Run>, RunStoreError> {
    let sql = format!(
        "SELECT {COLUMNS}
           FROM run
          WHERE tenant_id = $1 AND project_id = $2 AND card_id IS NULL
            AND parent_run_id IS NULL
            AND status IN ('queued', 'running', 'waiting_approval')"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Run::try_from).transpose()
}

/// Create-or-return the project's live execution run (`card_id IS NULL`).
///
/// Returns `(run, created)`. The partial unique index `uq_run_project_live` (migration 0023)
/// is the guarantee, not the read below: two concurrent `projects.execute` calls can both miss
/// the read, and exactly one insert survives. The loser returns the winner's row. The caller
/// enqueues `run.execute` in the same transaction only when `created` is true.
pub async fn create_project_execution_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(Run, bool), RunStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Project, project_id).await?;
    if let Some(run) = live_project_run_tx(&mut *conn, tenant, project_id).await? {
        return Ok((run, false));
    }
    let input = NewRun::new(project_id, None);
    match insert_run_tx(&mut *conn, tenant, &input).await {
        Ok(run) => Ok((run, true)),
        Err(RunStoreError::LiveRunExists) => {
            // The index fired between the read and the insert; the winner's row is the answer.
            let run = live_project_run_tx(&mut *conn, tenant, project_id)
                .await?
                .ok_or(RunStoreError::NotFound)?;
            Ok((run, false))
        }
        Err(error) => Err(error),
    }
}

/// One run as the MCP task projection sees it (S06c): the domain value plus both row
/// timestamps. `lastUpdatedAt` is the row's `updated_at`, not a derived guess.
#[derive(Clone, Debug)]
pub struct RunTaskRow {
    /// The domain value.
    pub run: Run,
    /// When the row was created (the task's `createdAt`).
    pub created_at: DateTime<Utc>,
    /// When the row was last written (the task's `lastUpdatedAt`).
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct RunTaskRowRow {
    id: Uuid,
    card_id: Option<Uuid>,
    project_id: Uuid,
    status: String,
    attempt: i32,
    error_class: Option<String>,
    error_message: Option<String>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Read one run with its row timestamps, for the MCP task projection.
pub async fn get_run_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RunTaskRow>, RunStoreError> {
    let sql = format!(
        "SELECT {COLUMNS}, created_at, updated_at FROM run WHERE tenant_id = $1 AND id = $2"
    );
    let row: Option<RunTaskRowRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let status =
        RunStatus::from_str(&row.status).map_err(|_| RunStoreError::UnknownStatus(row.status))?;
    let run = Run {
        id: row.id,
        card_id: row.card_id,
        project_id: row.project_id,
        status,
        attempt: row.attempt,
        error_class: row.error_class,
        error_message: row.error_message,
        started_at: row.started_at,
        finished_at: row.finished_at,
        version: version_of(row.version),
    };
    Ok(Some(RunTaskRow {
        run,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}

/// Everything a process step needs from the run row, in one read.
///
/// [`Run`] is the domain value and deliberately carries no `process_id`/`inputs`; the
/// executor resolves its template against them, so they travel together here rather than as
/// three queries that could observe three different instants.
#[derive(Clone, Debug)]
pub struct RunContext {
    /// The row id.
    pub id: Uuid,
    /// The lifecycle status.
    pub status: RunStatus,
    /// The attempt number, for the run-level events.
    pub attempt: i32,
    /// The card being executed.
    pub card_id: Option<Uuid>,
    /// The owning project.
    pub project_id: Uuid,
    /// The trigger correlation id (`{{run.correlation_id}}`).
    pub correlation_id: Option<Uuid>,
    /// The caller-supplied inputs (`{{run.inputs.<path>}}`).
    pub inputs: serde_json::Value,
    /// The published process version this run executes.
    pub process_id: Option<Uuid>,
    /// The capability this run executes under (D22/§17); `None` until S15 fills the resolver.
    pub capability_id: Option<Uuid>,
    /// The capability agent configuration this run was resolved against.
    pub capability_agent_id: Option<Uuid>,
}

/// The raw columns [`get_run_context_tx`] reads, before the status is parsed.
#[derive(Debug, sqlx::FromRow)]
struct RunContextRow {
    id: Uuid,
    status: String,
    attempt: i32,
    card_id: Option<Uuid>,
    project_id: Uuid,
    correlation_id: Option<Uuid>,
    inputs: serde_json::Value,
    process_id: Option<Uuid>,
    capability_id: Option<Uuid>,
    capability_agent_id: Option<Uuid>,
}

/// Read one run's executor-relevant facts inside the caller's scoped transaction.
pub async fn get_run_context_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<RunContext>, RunStoreError> {
    let row: Option<RunContextRow> = sqlx::query_as(
        "SELECT id, status, attempt, card_id, project_id, correlation_id, inputs, process_id,
                capability_id, capability_agent_id
               FROM run
              WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let status = RunStatus::from_str(&row.status)
        .map_err(|_| RunStoreError::UnknownStatus(row.status.clone()))?;
    Ok(Some(RunContext {
        id: row.id,
        status,
        attempt: row.attempt,
        card_id: row.card_id,
        project_id: row.project_id,
        correlation_id: row.correlation_id,
        inputs: row.inputs,
        process_id: row.process_id,
        capability_id: row.capability_id,
        capability_agent_id: row.capability_agent_id,
    }))
}

/// Fail every run whose driving `run.execute` job is dead.
///
/// Run liveness is the driving job's lease, not a `run.heartbeat_at` column (S06 §9 decision
/// 3). A worker killed with `SIGKILL` cannot write anything; its job row is reclaimed by the
/// lease and, once its attempts are spent, dead-lettered by
/// [`crate::jobs::finish_dead`]/`reap_exhausted`. This sweep closes the other half: the run
/// that job was driving must not stay `queued`/`running` forever, or the card's live-run slot
/// is held by a corpse and `runs.create` can never retry.
///
/// The run is failed **here**, in the caller's transaction, because the detection and the
/// write must agree: a run with a live (`pending`/`running`) `run.execute` job is never
/// touched, and "live" is checked with the same statement that selects the dead ones.
/// Returns `(run_id, dead_job_id)` so the caller can emit `run.failed` in-tx with the dead
/// job as its cause; this module never writes an outbox row itself.
pub async fn reap_dead_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "WITH latest AS (
             SELECT DISTINCT ON (j.dedupe_key) j.dedupe_key, j.id AS job_id, j.status
               FROM job j
              WHERE j.tenant_id = $1 AND j.kind = 'run.execute'
              ORDER BY j.dedupe_key, j.created_at DESC, j.id DESC
         )
         UPDATE run r
            SET status = 'failed',
                error_class = 'crashed',
                error_message = 'the run.execute job died',
                version = r.version + 1,
                updated_at = now(),
                finished_at = coalesce(r.finished_at, now())
           FROM latest
          WHERE r.tenant_id = $1
            AND r.status IN ('queued', 'running')
            -- A project execution is card-less and long-lived by design (S06c trap 10): a dead
            -- run.execute must never reap it. Its lifetime is the task TTL, not a job lease.
            AND r.card_id IS NOT NULL
            AND latest.dedupe_key = 'run:' || r.id::text
            AND latest.status = 'dead'
            AND NOT EXISTS (
                SELECT 1 FROM job live
                 WHERE live.tenant_id = $1
                   AND live.kind = 'run.execute'
                   AND live.dedupe_key = 'run:' || r.id::text
                   AND live.status IN ('pending', 'running')
            )
        RETURNING r.id, latest.job_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

// ── the run.execute job (S06 §5.3) ────────────────────────────────────────────────────────

/// The kind of the job that drives one run's execution.
pub const RUN_EXECUTE_KIND: &str = "run.execute";

/// How many attempts a `run.execute` job gets before it is dead-lettered.
///
/// Chosen to match the `job.max_attempts` default (0001_platform.sql): a handler bug that
/// fails deterministically must still spend its budget, while a transient database error
/// gets four more tries.
pub const RUN_EXECUTE_MAX_ATTEMPTS: i32 = 5;

/// The dedupe key that ties a run to its driving job.
///
/// `run.reap` matches the dead job back to its run with `'run:' || r.id::text`, so this is
/// the one spelling of the key — never a literal at a call site.
pub fn run_execute_dedupe_key(run_id: Uuid) -> String {
    format!("run:{run_id}")
}

/// The next attempt number for a card: `1 +` the runs it already has.
///
/// The attempt is derived, not stored on the card: history is the `run` rows, and a run that
/// was reaped (`failed`/`crashed`) or finished leaves its row behind, so the next create
/// continues the sequence. [`super::run::insert_run_tx`]'s `uq_run_live` still makes two
/// concurrent creates for one card a single live run.
pub async fn next_attempt_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<i32, sqlx::Error> {
    let next: i64 = sqlx::query_scalar(
        "SELECT (coalesce(max(attempt), 0) + 1)::bigint
               FROM run WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(i32::try_from(next).unwrap_or(i32::MAX))
}

/// The next attempt number for a child step: `1 +` the runs that `(parent, step)` already has.
///
/// Attempts are counted per TREE EDGE, not per card and not per parent: one parent may start
/// several different steps, and each `(parent_run_id, step_id)` has its own sequence — which is
/// exactly what `uq_run_child_attempt` enforces.
pub async fn next_attempt_for_child_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    parent_run_id: Uuid,
    step_id: Uuid,
) -> Result<i32, sqlx::Error> {
    let next: i64 = sqlx::query_scalar(
        "SELECT (coalesce(max(attempt), 0) + 1)::bigint
               FROM run WHERE tenant_id = $1 AND parent_run_id = $2 AND step_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(parent_run_id)
    .bind(step_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(i32::try_from(next).unwrap_or(i32::MAX))
}

/// Enqueue the driving `run.execute` job on the caller's transaction.
///
/// `runs.create` must commit the queued row, its event and its job together: a crash between
/// them would leave either a run nobody executes or a job naming a run that never existed
/// (CONTRACT §7). The statement is `crate::jobs::enqueue`'s, on the caller's connection; the
/// dedupe key is `run:<id>` so a replay collapses onto the live job.
pub async fn enqueue_run_execute_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    let payload = serde_json::json!({ "run_id": run_id });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(RUN_EXECUTE_KIND)
    .bind(run_execute_dedupe_key(run_id))
    .bind(payload)
    .bind(RUN_EXECUTE_MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// The job kind the `run.failed` handler is dispatched on (S13 §5.8).
///
/// The old n8n `err.card` flow is code now: the worker's `err.card` arm moves the card to the
/// waiting lane, comments and starts the coach. The kind is a constant so the emitter and the
/// dispatcher cannot spell it differently.
pub const ERR_CARD_KIND: &str = "err.card";

/// Enqueue the `err.card` handler for a run that just failed.
///
/// The old n8n `err.card` flow was the estate's error trigger: a failed execution moved the
/// card to BLOCKED and told the coach. Here the run's own `run.failed` emission enqueues one
/// [`ERR_CARD_KIND`] job, keyed by the run, so the several emitters that can fail one run (the
/// reaper, the step executor, the action layer) produce at most one delivery. The handler
/// resolves the card from the payload through the public actions, never a second write path.
pub async fn enqueue_run_failed_trigger_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    let Some(node) =
        get_run_node_tx(&mut *conn, tenant, run_id)
            .await
            .map_err(|error| match error {
                RunStoreError::Database(error) => error,
                other => sqlx::Error::Protocol(other.to_string()),
            })?
    else {
        return Ok(None);
    };
    // A CHILD's failure is its parent step's business: the parent's `on_result` decides, and a
    // `call` step that continues must not have moved the card to BLOCKED or started a coach
    // underneath itself (CONTRACT §21). Only a ROOT failure has an err.card.
    if node.parent_run_id.is_some() {
        return Ok(None);
    }
    // The old error trigger read the card out of the failed execution and stopped when there was
    // none. A project-scoped run has no card to move or comment on, so it has no err.card.
    let Some(card_id) = node.run.card_id else {
        return Ok(None);
    };
    let payload = serde_json::json!({
        "run_id": run_id,
        "card_id": card_id,
        "project_id": node.run.project_id,
    });
    crate::jobs::enqueue_tx(
        conn,
        tenant,
        ERR_CARD_KIND,
        &format!("err.card:{run_id}"),
        payload,
        None,
        1,
    )
    .await
}

/// Read one run with the read projection's sort key.
///
/// [`Run`] carries no `created_at`; a handler that has just written the row still owes the
/// caller the full [`RunRecord`] representation, and `now()` from the process would be a
/// second clock. This is the one-row twin of [`list_runs_tx`].
pub async fn get_run_record_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RunRecord>, RunStoreError> {
    let sql = format!("SELECT {COLUMNS}, created_at FROM run WHERE tenant_id = $1 AND id = $2");
    let row: Option<RunRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(RunRecord::try_from).transpose()
}

/// The two run columns the queue's projection does not carry (0015): the process a run
/// executes and the inputs it was created with.
///
/// Read only by `runs.get`; keeping it out of [`RunRecord`] leaves every existing caller of
/// [`get_run_record_tx`] untouched.
#[derive(Clone, Debug)]
pub struct RunDetail {
    /// The published process version this run executes, when it is process-driven.
    pub process_id: Option<Uuid>,
    /// The caller-supplied run inputs; `{}` for an ordinary run.
    pub inputs: serde_json::Value,
    /// The run that started this one; `None` for a card/project root (S13's run tree).
    pub parent_run_id: Option<Uuid>,
    /// The `process_step` that started this run, when it is a child.
    pub step_id: Option<Uuid>,
    /// 0 for a root, up to 3 (the tree's depth ceiling).
    pub depth: i32,
    /// Direct children, oldest first.
    pub children: Vec<RunTreeChild>,
}

/// One direct child edge of a run.
#[derive(Clone, Debug)]
pub struct RunTreeChild {
    /// The child run.
    pub id: Uuid,
    /// Its status (`queued | running | waiting_approval | succeeded | failed | cancelled`).
    pub status: String,
    /// The parent's `process_step` that started it.
    pub step_id: Option<Uuid>,
    /// Its depth (the parent's + 1).
    pub depth: i32,
}

/// One run's process, inputs and tree edges, or `None` when the row is absent in this tenant.
pub async fn get_run_detail_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RunDetail>, RunStoreError> {
    #[derive(Debug, sqlx::FromRow)]
    struct DetailRow {
        process_id: Option<Uuid>,
        inputs: serde_json::Value,
        parent_run_id: Option<Uuid>,
        step_id: Option<Uuid>,
        depth: i32,
    }
    let row: Option<DetailRow> = sqlx::query_as(
        "SELECT process_id, inputs, parent_run_id, step_id, depth
               FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let children: Vec<(Uuid, String, Option<Uuid>, i32)> = sqlx::query_as(
        "SELECT id, status, step_id, depth FROM run
          WHERE tenant_id = $1 AND parent_run_id = $2
          ORDER BY created_at, id",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Some(RunDetail {
        process_id: row.process_id,
        inputs: row.inputs,
        parent_run_id: row.parent_run_id,
        step_id: row.step_id,
        depth: row.depth,
        children: children
            .into_iter()
            .map(|(id, status, step_id, depth)| RunTreeChild {
                id,
                status,
                step_id,
                depth,
            })
            .collect(),
    }))
}

/// Cancel a non-terminal run and record why.
///
/// The approval-expiry sweep needs the transition **and** the failure class in one write
/// (`waiting_approval → cancelled` with `error_class='approval_expired'`); the domain has no
/// `waiting_approval → failed` edge, and leaving the run live would hold `uq_run_live`
/// forever. `None` means the row is absent or already terminal (nothing to announce).
///
/// Like [`fail_run_tx`] the guard is `NOT terminal`, not version-based (CONTRACT §3), and the
/// domain's transition table is consulted first so an illegal pair is a [`RunError`], never a
/// zero-row write that looks like a lost race.
pub async fn cancel_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    error_class: &str,
) -> Result<Option<Run>, RunStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM run WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let run: Run = row.try_into()?;
    if run.status.is_terminal() {
        return Ok(None);
    }
    // Ask the domain first: an illegal pair is a refused transition, not a silent zero-row
    // write. The statement below repeats the guard in SQL so a concurrent writer cannot slip
    // between the read and the write.
    run.transition(RunStatus::Cancelled)?;
    let sql = format!(
        "UPDATE run
            SET status = 'cancelled', error_class = $3,
                version = version + 1, updated_at = now(),
                finished_at = coalesce(finished_at, now())
          WHERE tenant_id = $1 AND id = $2 AND status NOT IN ('succeeded', 'failed', 'cancelled')
         RETURNING {COLUMNS}"
    );
    let row: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(error_class)
        .fetch_optional(&mut *conn)
        .await?;
    let cancelled = row.map(Run::try_from).transpose()?;
    // The run tree is one unit of work: an expired gate (or any caller of this store function)
    // must not leave a live child holding `uq_run_child_live` on a cancelled parent.
    if cancelled.is_some() {
        cancel_descendants_tx(conn, tenant, run_id).await?;
    }
    Ok(cancelled)
}

/// Cancel every LIVE descendant of a run, recursively.
///
/// The approval-expiry and reaper paths cancel the ROOT they own; a root's children are
/// separate runs with their own driving jobs, so without this sweep they would stay
/// `queued`/`running` forever after their parent is cancelled (a zombie subtree that holds
/// `uq_run_child_live` on its edges). Depth-first through the whole tree, live-only at the
/// write: a terminal descendant is history and is left alone.
///
/// Returns the rows that moved to `cancelled`, so the caller can emit one `run.cancelled` per
/// child in its own transaction. The named `run_id` itself is never touched — its own
/// transition belongs to the caller.
pub async fn cancel_descendants_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<Run>, RunStoreError> {
    let sql = format!(
        "WITH RECURSIVE descendants AS (
             SELECT id FROM run WHERE tenant_id = $1 AND parent_run_id = $2
             UNION
             SELECT child.id
               FROM run child
               JOIN descendants parent ON child.parent_run_id = parent.id
              WHERE child.tenant_id = $1
         )
         UPDATE run
            SET status = 'cancelled',
                version = version + 1,
                updated_at = now(),
                finished_at = coalesce(finished_at, now())
          WHERE tenant_id = $1
            AND id IN (SELECT id FROM descendants)
            AND status IN ('queued', 'running', 'waiting_approval')
         RETURNING {COLUMNS}"
    );
    let rows: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(Run::try_from).collect()
}
