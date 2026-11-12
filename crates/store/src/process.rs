#![forbid(unsafe_code)]
//! Processes and their execution trace: the rows behind S11.
//!
//! A process is a **version family**: rows share `(tenant_id, key)`, each with its own `version` and `status`.
//! Two partial unique indexes carry the whole invariant — at most one `draft` and at most one `published` per
//! key (migrations `0015` and `0016`), so "publish a second draft" and "two drafts of one key" are refused by the
//! database rather than by a check two transactions can both pass.
//!
//! Editing is **copy-forward**: a published version is frozen, and an edit creates `version + 1` as a new draft.
//! `process_step` rows belong to exactly one version, which is what lets a run resolve against the version it
//! started on even after the key moves on.
//!
//! `run_step` is the trace: one row per attempt, never updated in place across attempts, so a retry APPENDS and
//! S13's diff gate can read what actually happened.

use chrono::{DateTime, Utc};
use conveyor_domain::process::{Actor, StepInput, StepKind};
use conveyor_domain::{TenantId, Version};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};

/// Why a process operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ProcessStoreError {
    /// No such row in this tenant.
    #[error("the process does not exist in this tenant")]
    NotFound,
    /// A draft already exists for this key (`uq_process_draft`).
    #[error("a draft of this process key already exists")]
    DraftExists,
    /// The row is not a draft, so it cannot be edited or published.
    #[error("the process is {0}, not a draft")]
    NotDraft(String),
    /// The row is not published.
    #[error("the process is not published")]
    NotPublished,
    /// Nothing is published under that key.
    #[error("no published process has that key")]
    NoPublished,
    /// `If-Match` named a version the row is not at.
    #[error("the process is at version {current}, not the version the caller supplied")]
    VersionMismatch {
        /// The row's current version.
        current: Version,
    },
    /// A referenced `card_type` does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// `project_type_id` is not a `catalogue_entry(kind='project_type')` of this tenant.
    ///
    /// The foreign key alone accepts another tenant's id (FK checks bypass RLS), so the
    /// create resolves it in the caller's scope first — CONTRACT §2.
    #[error("the referenced project type does not exist in this tenant")]
    ProjectTypeNotFound,
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// A process row as stored.
#[derive(Clone, Debug)]
pub struct ProcessRecord {
    /// The row id.
    pub id: Uuid,
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// The description (`''` when absent).
    pub description: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The version within the key (the FAMILY version; `uq (tenant_id, key, version)`).
    pub version: i64,
    /// The optimistic-concurrency counter `If-Match` compares. Bumped by every mutation.
    pub lock_version: i64,
    /// The card type the process applies to, when it names one.
    pub card_type_id: Option<Uuid>,
    /// The board list the process drives (`ready | doing | qa | done`), when it names one.
    pub list_kind: Option<String>,
    /// The project type the process applies to (`catalogue_entry(kind='project_type')`).
    pub project_type_id: Option<Uuid>,
    /// The user-facing deliverables a person sees when the process completes.
    pub outputs: Value,
    /// The capability the process belongs to, when it names one.
    pub capability_id: Option<Uuid>,
    /// The owner.
    pub owner_user_id: Option<Uuid>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One step of one process version.
#[derive(Clone, Debug)]
pub struct ProcessStepRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning process version.
    pub process_id: Uuid,
    /// 1-based order.
    pub position: i32,
    /// `agent | computed | gate | call` — how the step executes (0033).
    pub kind: String,
    /// `process_agent | tool_agent | system | human` — who owns the step's work (0033).
    pub actor: String,
    /// The registry action a `computed` step invokes; `None` for every other kind.
    pub action: Option<String>,
    /// The capability the step belongs to.
    pub capability_id: Option<Uuid>,
    /// The `capability_agent(role='process_agent')` that runs an `agent` step.
    pub process_agent_id: Option<Uuid>,
    /// The published SOP the step is judged against.
    pub procedure_id: Option<Uuid>,
    /// The published process (or workflow) a `call` step starts.
    pub child_process_key: Option<String>,
    /// The input template.
    pub input_template: Value,
    /// The gate code the step parks on.
    pub gate_kind: Option<String>,
    /// `fail | continue | retry`.
    pub on_result: String,
    /// The retry cap for this step.
    pub max_attempts: i32,
    /// The dot-paths into this step's output that later steps may reference (S21 §6).
    pub outputs: Vec<String>,
}

/// A process to create.
#[derive(Clone, Debug)]
pub struct NewProcess {
    /// The version-family key.
    pub key: String,
    /// The title.
    pub title: String,
    /// The description.
    pub description: String,
    /// The card type, when the process names one.
    pub card_type_id: Option<Uuid>,
    /// The board list the process drives (`ready | doing | qa | done`), when it names one.
    pub list_kind: Option<String>,
    /// The project type, when the process names one.
    pub project_type_id: Option<Uuid>,
    /// The user-facing deliverables; `[]` when the process declares none.
    pub outputs: Value,
    /// The capability, when the process names one.
    pub capability_id: Option<Uuid>,
    /// The owner.
    pub owner_user_id: Option<Uuid>,
}

/// One `run_step` row.
#[derive(Clone, Debug)]
pub struct RunStepRecord {
    /// The row id.
    pub id: Uuid,
    /// The run it belongs to.
    pub run_id: Uuid,
    /// The step of the published process version, when the row came from one.
    pub process_step_id: Option<Uuid>,
    /// 1-based order.
    pub position: i32,
    /// The attempt number (1-based; a retry appends).
    pub attempt: i32,
    /// The action this step invokes.
    pub action: String,
    /// `queued | running | succeeded | failed | skipped`.
    pub status: String,
    /// The resolved input, once it ran.
    pub input: Value,
    /// The action's output, once it succeeded.
    pub output: Value,
    /// The failure message, when it failed.
    pub error: Option<String>,
    /// The judge's verdict on this attempt's output (the SOP inspection, 0033).
    pub judge: Option<Value>,
    /// The version of the SOP the step pinned when it was claimed (0033).
    pub procedure_version: Option<i64>,
}

const PROCESS_COLUMNS: &str = "id, key, title, description, status, version, lock_version, \
                               card_type_id, list_kind, project_type_id, outputs, capability_id, \
                               owner_user_id, created_at, updated_at";
const STEP_COLUMNS: &str = "id, process_id, position, action, input_template, gate_kind, on_result, \
                            max_attempts, outputs, kind, actor, capability_id, process_agent_id, \
                            procedure_id, child_process_key";
const RUN_STEP_COLUMNS: &str = "id, run_id, process_step_id, position, attempt, action, status, \
                                input, output, error, judge, procedure_version";

#[derive(sqlx::FromRow)]
struct ProcessRow {
    id: Uuid,
    key: String,
    title: String,
    description: String,
    status: String,
    version: i64,
    lock_version: i64,
    card_type_id: Option<Uuid>,
    list_kind: Option<String>,
    project_type_id: Option<Uuid>,
    outputs: Value,
    capability_id: Option<Uuid>,
    owner_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ProcessRow> for ProcessRecord {
    fn from(row: ProcessRow) -> Self {
        Self {
            id: row.id,
            key: row.key,
            title: row.title,
            description: row.description,
            status: row.status,
            version: row.version,
            lock_version: row.lock_version,
            card_type_id: row.card_type_id,
            list_kind: row.list_kind,
            project_type_id: row.project_type_id,
            outputs: row.outputs,
            capability_id: row.capability_id,
            owner_user_id: row.owner_user_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct StepRow {
    id: Uuid,
    process_id: Uuid,
    position: i32,
    action: Option<String>,
    input_template: Value,
    gate_kind: Option<String>,
    on_result: String,
    max_attempts: i32,
    outputs: Value,
    kind: String,
    actor: String,
    capability_id: Option<Uuid>,
    process_agent_id: Option<Uuid>,
    procedure_id: Option<Uuid>,
    child_process_key: Option<String>,
}

impl From<StepRow> for ProcessStepRecord {
    fn from(row: StepRow) -> Self {
        Self {
            id: row.id,
            process_id: row.process_id,
            position: row.position,
            kind: row.kind,
            actor: row.actor,
            action: row.action,
            capability_id: row.capability_id,
            process_agent_id: row.process_agent_id,
            procedure_id: row.procedure_id,
            child_process_key: row.child_process_key,
            input_template: row.input_template,
            gate_kind: row.gate_kind,
            on_result: row.on_result,
            max_attempts: row.max_attempts,
            // The column only ever holds what `insert_steps_tx` wrote (an array of strings); a
            // hand-written row that is not one reads as "declares nothing", which is the safe side.
            outputs: serde_json::from_value(row.outputs).unwrap_or_default(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct RunStepRow {
    id: Uuid,
    run_id: Uuid,
    process_step_id: Option<Uuid>,
    position: i32,
    attempt: i32,
    action: String,
    status: String,
    input: Value,
    output: Value,
    error: Option<String>,
    judge: Option<Value>,
    procedure_version: Option<i64>,
}

impl From<RunStepRow> for RunStepRecord {
    fn from(row: RunStepRow) -> Self {
        Self {
            id: row.id,
            run_id: row.run_id,
            process_step_id: row.process_step_id,
            position: row.position,
            attempt: row.attempt,
            action: row.action,
            status: row.status,
            input: row.input,
            output: row.output,
            error: row.error,
            judge: row.judge,
            procedure_version: row.procedure_version,
        }
    }
}

// ── process ───────────────────────────────────────────────────────────────────────────────

/// Create `version 1` of a key, as a draft.
pub async fn create_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewProcess,
) -> Result<ProcessRecord, ProcessStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::CardType, input.card_type_id).await?;
    if let Some(project_type_id) = input.project_type_id {
        // `project_type_id` names a `catalogue_entry`, which is not a [`Parent`] variant; the
        // foreign key alone would accept another tenant's id, so resolve it in scope here.
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM catalogue_entry
                             WHERE tenant_id = $1 AND id = $2 AND kind = 'project_type')",
        )
        .bind(tenant.as_uuid())
        .bind(project_type_id)
        .fetch_one(&mut *conn)
        .await?;
        if !present {
            return Err(ProcessStoreError::ProjectTypeNotFound);
        }
    }
    // `outputs` is NOT NULL in the database; a caller that declares nothing sends `[]` (or a
    // JSON null, which would be refused by the column — normalize it here).
    let outputs = if input.outputs.is_null() {
        Value::Array(Vec::new())
    } else {
        input.outputs.clone()
    };
    let sql = format!(
        "INSERT INTO process (tenant_id, key, title, description, card_type_id, owner_user_id,
                              list_kind, project_type_id, outputs, capability_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {PROCESS_COLUMNS}"
    );
    match sqlx::query_as::<_, ProcessRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.key)
        .bind(&input.title)
        .bind(&input.description)
        .bind(input.card_type_id)
        .bind(input.owner_user_id)
        .bind(input.list_kind.as_deref())
        .bind(input.project_type_id)
        .bind(&outputs)
        .bind(input.capability_id)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(row) => Ok(row.into()),
        Err(error) if is_draft_conflict(&error) => Err(ProcessStoreError::DraftExists),
        Err(error) => Err(ProcessStoreError::Database(error)),
    }
}

/// Fork the next version of a key as a fresh draft, copying the source version's steps.
///
/// The source may be a published or retired version; the result is always a draft at `max(version) + 1`.
pub async fn fork_version_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    source_id: Uuid,
    owner_user_id: Option<Uuid>,
) -> Result<ProcessRecord, ProcessStoreError> {
    let source = lock_process_tx(&mut *conn, tenant, source_id).await?;
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) + 1 FROM process WHERE tenant_id = $1 AND key = $2",
    )
    .bind(tenant.as_uuid())
    .bind(&source.key)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "INSERT INTO process (tenant_id, key, title, description, version, card_type_id, owner_user_id,
                              list_kind, project_type_id, outputs, capability_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING {PROCESS_COLUMNS}"
    );
    let row = match sqlx::query_as::<_, ProcessRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&source.key)
        .bind(&source.title)
        .bind(&source.description)
        .bind(next)
        .bind(source.card_type_id)
        .bind(owner_user_id.or(source.owner_user_id))
        .bind(source.list_kind.as_deref())
        .bind(source.project_type_id)
        .bind(&source.outputs)
        .bind(source.capability_id)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(row) => row,
        Err(error) if is_draft_conflict(&error) => {
            return Err(ProcessStoreError::DraftExists);
        }
        Err(error) => return Err(ProcessStoreError::Database(error)),
    };
    // The steps are copied verbatim: an edit starts from the version that was running.
    sqlx::query(
        "INSERT INTO process_step
             (tenant_id, process_id, position, kind, actor, action, input_template, gate_kind,
              on_result, max_attempts, outputs, capability_id, process_agent_id, procedure_id,
              child_process_key)
         SELECT tenant_id, $2, position, kind, actor, action, input_template, gate_kind,
                on_result, max_attempts, outputs, capability_id, process_agent_id, procedure_id,
                child_process_key
           FROM process_step
          WHERE tenant_id = $1 AND process_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(row.id)
    .bind(source_id)
    .execute(&mut *conn)
    .await?;
    Ok(row.into())
}

/// Read one version.
pub async fn get_process_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProcessRecord>, ProcessStoreError> {
    let sql = format!("SELECT {PROCESS_COLUMNS} FROM process WHERE tenant_id = $1 AND id = $2");
    let row: Option<ProcessRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Read one version and hold its row lock until the transaction ends.
async fn lock_process_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ProcessRecord, ProcessStoreError> {
    let sql = format!(
        "SELECT {PROCESS_COLUMNS} FROM process WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ProcessRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(ProcessStoreError::NotFound)
}

/// The published version of a key.
pub async fn published_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<ProcessRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {PROCESS_COLUMNS} FROM process
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'"
    );
    let row: Option<ProcessRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Every published process key in the tenant — the `call` targets a publish resolves against.
pub async fn published_keys_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT key FROM process WHERE tenant_id = $1 AND status = 'published'")
            .bind(tenant.as_uuid())
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.into_iter().map(|(key,)| key).collect())
}

/// The live process agents (`role='process_agent'`, status `active`) a publish may name.
pub async fn live_process_agent_ids_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<std::collections::BTreeSet<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM capability_agent
          WHERE tenant_id = $1 AND status = 'active' AND role = 'process_agent'",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Every distinct process agent this process's steps bind (D29), in id order.
///
/// The catalogue seeds one agent per process and every agent step names it, so this is usually a
/// one-element slice. A tenant process may bind a different agent to each step; readiness then
/// checks each of them rather than demanding the capability's single-executor rule.
pub async fn process_agent_ids_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT DISTINCT process_agent_id FROM process_step
          WHERE tenant_id = $1 AND process_id = $2 AND process_agent_id IS NOT NULL
          ORDER BY process_agent_id",
    )
    .bind(tenant.as_uuid())
    .bind(process_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// The one process agent this process's steps bind (D29), when they bind exactly one.
///
/// The catalogue seeds one agent per process and every agent step names it. A process whose
/// steps name more than one agent is not resolvable here; the caller keeps the capability-level
/// rule rather than silently picking one.
pub async fn process_agent_for_process_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    Ok(
        match process_agent_ids_tx(conn, tenant, process_id)
            .await?
            .as_slice()
        {
            [agent_id] => Some(*agent_id),
            _ => None,
        },
    )
}

/// A key's version, latest by default.
///
/// With `version = None` this is the key's LATEST version of any status; `published_by_key_tx` remains the
/// published-only lookup.
pub async fn get_process_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: Option<i64>,
) -> Result<Option<ProcessRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {PROCESS_COLUMNS} FROM process
          WHERE tenant_id = $1 AND key = $2 AND ($3::bigint IS NULL OR version = $3)
          ORDER BY version DESC LIMIT 1"
    );
    let row: Option<ProcessRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .bind(version)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// One page of processes, newest version first within each key.
///
/// The cursor is `(key, version)`: keys are unique per version family, so the pair is a stable sort key even
/// while a key gains versions. `card_type_id` narrows the page to the processes that name that card type;
/// `None` lists every process.
///
/// `limit` is passed through unclamped, on purpose: a caller that decides "is there another page" by asking
/// for one row more (`limit + 1` and then popping the extra) loses that row to any clamp applied here, and a
/// page of exactly `MAX_LIMIT` would answer `next_cursor: None` while more rows existed. The clamp belongs to
/// the action (`store::actions::clamp_limit`), exactly as it does for `members_list`.
pub async fn list_processes_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    card_type_id: Option<Uuid>,
    cursor: Option<(&str, i64)>,
    limit: i64,
) -> Result<Vec<ProcessRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {PROCESS_COLUMNS}
           FROM process
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::uuid IS NULL OR card_type_id = $3)
            AND ($4::text IS NULL OR (key, version) > ($4, $5))
          ORDER BY key, version
          LIMIT $6"
    );
    let rows: Vec<ProcessRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(card_type_id)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1).unwrap_or(0))
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// What an edit may change. Unnamed members keep their value; a named `card_type_id: Some(None)` clears it.
#[derive(Clone, Debug, Default)]
pub struct DraftPatch<'a> {
    /// The `If-Match` precondition, compared against `lock_version`.
    pub expected: Option<Version>,
    /// The new title.
    pub title: Option<&'a str>,
    /// The new description.
    pub description: Option<&'a str>,
    /// `Some(None)` clears the card type; `None` keeps it.
    pub card_type_id: Option<Option<Uuid>>,
    /// Replaces the whole step list when supplied.
    pub steps: Option<&'a [StepInput]>,
}

/// Change a draft's title/description and, when supplied, replace its whole step list.
pub async fn update_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &DraftPatch<'_>,
) -> Result<ProcessRecord, ProcessStoreError> {
    let DraftPatch {
        expected,
        title,
        description,
        card_type_id,
        steps,
    } = *patch;
    let current = lock_process_tx(&mut *conn, tenant, id).await?;
    if expected
        .is_some_and(|expected| u64::try_from(current.lock_version).ok() != Some(expected.get()))
    {
        return Err(ProcessStoreError::VersionMismatch {
            current: version_of(current.lock_version),
        });
    }
    if current.status != "draft" {
        return Err(ProcessStoreError::NotDraft(current.status));
    }
    if let Some(Some(card_type_id)) = card_type_id {
        ensure_optional_parent_tx(&mut *conn, tenant, Parent::CardType, Some(card_type_id)).await?;
    }
    // A named `null` clears the column; an unnamed `None` keeps it (`$5` says which).
    let clear_card_type = card_type_id.is_some();
    let new_card_type = card_type_id.flatten();
    let sql = format!(
        "UPDATE process
            SET title = COALESCE($3, title),
                description = COALESCE($4, description),
                card_type_id = CASE WHEN $5 THEN $6 ELSE card_type_id END,
                lock_version = lock_version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {PROCESS_COLUMNS}"
    );
    let row: ProcessRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(title)
        .bind(description)
        .bind(clear_card_type)
        .bind(new_card_type)
        .fetch_one(&mut *conn)
        .await?;
    if let Some(steps) = steps {
        replace_steps_tx(&mut *conn, tenant, id, steps).await?;
    }
    Ok(row.into())
}

/// Insert one row per step, in order.
pub async fn insert_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
    steps: &[StepInput],
) -> Result<Vec<ProcessStepRecord>, ProcessStoreError> {
    let mut out = Vec::with_capacity(steps.len());
    let sql = format!(
        "INSERT INTO process_step
             (tenant_id, process_id, position, action, input_template, gate_kind, on_result,
              max_attempts, outputs, kind, actor, capability_id, process_agent_id, procedure_id,
              child_process_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         RETURNING {STEP_COLUMNS}"
    );
    for (index, step) in steps.iter().enumerate() {
        let position = i32::try_from(index + 1).unwrap_or(i32::MAX);
        // The column is nullable now (0033): "this step names no action" is NULL, not `''`.
        let action = Some(step.action.as_str())
            .filter(|action| !action.trim().is_empty())
            .map(str::to_owned);
        // `kind` is inferred when the caller omitted it (a `gate_kind` step is a gate, anything
        // else computed); an explicit spelling is stored as written and the column's CHECK is
        // the authority. Same for `actor`, defaulted by kind.
        let parsed_kind = step.kind();
        let kind = match step.kind.as_deref() {
            Some(text) => text.to_owned(),
            None => parsed_kind
                .map(StepKind::as_str)
                .unwrap_or("computed")
                .to_owned(),
        };
        let actor = match step.actor.as_deref() {
            Some(text) => text.to_owned(),
            None => parsed_kind
                .and_then(|kind| step.actor(kind))
                .map(Actor::as_str)
                .unwrap_or("system")
                .to_owned(),
        };
        let row: StepRow = sqlx::query_as(sqlx::AssertSqlSafe(sql.clone()))
            .bind(tenant.as_uuid())
            .bind(process_id)
            .bind(position)
            .bind(action.as_deref())
            .bind(&step.input_template)
            .bind(step.gate_kind.as_deref())
            .bind(step.on_result.as_deref().unwrap_or("fail"))
            .bind(step.max_attempts.unwrap_or(1))
            .bind(serde_json::to_value(&step.outputs).unwrap_or_else(|_| Value::Array(Vec::new())))
            .bind(kind)
            .bind(actor)
            .bind(step.capability_id)
            .bind(step.process_agent_id)
            .bind(step.procedure_id)
            .bind(step.child_process_key.as_deref())
            .fetch_one(&mut *conn)
            .await?;
        out.push(row.into());
    }
    Ok(out)
}

/// Replace a draft's whole step list (the plan's "whole-list replace on a draft").
pub async fn replace_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
    steps: &[StepInput],
) -> Result<(), ProcessStoreError> {
    sqlx::query("DELETE FROM process_step WHERE tenant_id = $1 AND process_id = $2")
        .bind(tenant.as_uuid())
        .bind(process_id)
        .execute(&mut *conn)
        .await?;
    insert_steps_tx(&mut *conn, tenant, process_id, steps).await?;
    Ok(())
}

/// A process version's steps, in order.
pub async fn list_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
) -> Result<Vec<ProcessStepRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {STEP_COLUMNS} FROM process_step
          WHERE tenant_id = $1 AND process_id = $2 ORDER BY position"
    );
    let rows: Vec<StepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(process_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// The steps of a whole page of process versions, in `(process_id, position)` order.
///
/// One query for every id in the page, so a list is not an N+1. An empty `ids` returns an empty list
/// without touching the database.
pub async fn list_steps_for_processes_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    ids: &[Uuid],
) -> Result<Vec<ProcessStepRecord>, ProcessStoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {STEP_COLUMNS} FROM process_step
          WHERE tenant_id = $1 AND process_id = ANY($2)
          ORDER BY process_id, position"
    );
    let rows: Vec<StepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(ids)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Publish a draft: retire the key's currently published version, then publish this one.
///
/// Both writes are in the caller's transaction. The order matters — `uq_process_published` is an immediate
/// unique index, so the previous row must leave `published` before this one enters it.
pub async fn publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: Option<Version>,
) -> Result<ProcessRecord, ProcessStoreError> {
    let current = lock_process_tx(&mut *conn, tenant, id).await?;
    if expected
        .is_some_and(|expected| u64::try_from(current.lock_version).ok() != Some(expected.get()))
    {
        return Err(ProcessStoreError::VersionMismatch {
            current: version_of(current.lock_version),
        });
    }
    if current.status != "draft" {
        return Err(ProcessStoreError::NotDraft(current.status));
    }
    sqlx::query(
        "UPDATE process SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'",
    )
    .bind(tenant.as_uuid())
    .bind(&current.key)
    .execute(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE process SET status = 'published', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {PROCESS_COLUMNS}"
    );
    let row: ProcessRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Retire a published version. A draft cannot be retired — it is deleted, not retired.
pub async fn retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: Option<Version>,
) -> Result<ProcessRecord, ProcessStoreError> {
    let current = lock_process_tx(&mut *conn, tenant, id).await?;
    if expected
        .is_some_and(|expected| u64::try_from(current.lock_version).ok() != Some(expected.get()))
    {
        return Err(ProcessStoreError::VersionMismatch {
            current: version_of(current.lock_version),
        });
    }
    if current.status != "published" {
        return Err(ProcessStoreError::NotPublished);
    }
    let sql = format!(
        "UPDATE process SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {PROCESS_COLUMNS}"
    );
    let row: ProcessRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

// ── the process.step job ──────────────────────────────────────────────────────────────────

/// The kind of the job that drives one step of a process run.
pub const PROCESS_STEP_KIND: &str = "process.step";

/// The event that announces a step is ready to run.
///
/// `<aggregate>.<verb>` (CONTRACT §7), owned by the `run_step` table but carried on the
/// **run's** topic: the relay turns this one type into the bus's `work.requested` topic with
/// the step's job document, while every other event travels under its own type.
pub const RUN_STEP_REQUESTED: &str = "run_step.requested";

/// How many attempts a `process.step` job gets before it is dead-lettered.
///
/// The same budget as `run.execute`: a handler bug that fails deterministically still spends its
/// budget, while a transient database error gets four more tries.
pub const PROCESS_STEP_MAX_ATTEMPTS: i32 = 5;

/// The one spelling of a step job's dedupe key: `<run_id>:<position>:<attempt>`.
///
/// The triple is the `run_step` identity (`UNIQUE (run_id, position, attempt)`), so a replay
/// collapses onto the live job while a retry — a new attempt — is a new job.
pub fn process_step_dedupe_key(run_id: Uuid, position: i32, attempt: i32) -> String {
    format!("{run_id}:{position}:{attempt}")
}

/// The UUIDv5 namespace every step's idempotency key is derived in.
///
/// Frozen: changing it silently changes every key, so a replay after a deploy would look like a
/// fresh invocation. The literal is `uuid5(NAMESPACE_URL, "https://conveyor.dev/process.step")`,
/// computed once and pasted here because `new_v5` is not a `const fn`.
pub const PROCESS_STEP_NS: Uuid = Uuid::from_u128(0x8c77_95c0_3d86_5fe1_a7cb_3214_a250_fbfa);

/// The `Idempotency-Key` one step attempt presents to the action it invokes.
///
/// `(run_id, position, attempt)` is the `run_step` identity, so a crash between the action's
/// commit and the record replays into the same key and the action returns its stored outcome
/// instead of running twice (S11 §5.5/§8).
pub fn process_step_idempotency_key(run_id: Uuid, position: i32, attempt: i32) -> String {
    Uuid::new_v5(
        &PROCESS_STEP_NS,
        process_step_dedupe_key(run_id, position, attempt).as_bytes(),
    )
    .to_string()
}

/// Enqueue one step's driving job on the caller's transaction.
///
/// The run row, its materialised steps and the job that starts the first of them must commit
/// together (CONTRACT §7): a crash between them would leave either a run nobody drives or a job
/// naming a run that never existed. The conflict clause is `crate::jobs::enqueue`'s, on the
/// caller's connection, so a replayed enqueue collapses onto the live job.
///
/// The **event is the bus's publish source**, so it is appended here, beside the job, in the same
/// transaction. Both limbs of the engine then run the same idempotent `execute_step`: the relay
/// nudges a consumer through `work.requested`, and the job row is the fallback that survives a
/// Redis loss. Emitting only when the INSERT returned a row is what keeps a replay from looking
/// like a second request — the dedupe key already refused the second job.
pub async fn enqueue_process_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let payload = serde_json::json!({
        "run_id": run_id,
        "position": position,
        "attempt": attempt,
    });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(PROCESS_STEP_KIND)
    .bind(process_step_dedupe_key(run_id, position, attempt))
    .bind(payload)
    .bind(PROCESS_STEP_MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id,)) = row else {
        return Ok(None);
    };
    announce_step_requested(conn, tenant, run_id, position, attempt).await?;
    Ok(Some(id))
}

/// Append the outbox event that wakes the bus for one step attempt.
///
/// The payload is the step's identity, never its resolved input: the executor claims and records
/// that, so a redelivered nudge re-runs the same idempotent attempt. Only transitions that are due
/// IMMEDIATELY announce — a judge park and a call poll enqueue with a backoff, and a bus nudge
/// would bypass it and re-call laya or a live child on every park.
pub async fn announce_step_requested(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
) -> Result<(), sqlx::Error> {
    let event = crate::outbox::NewEvent {
        topic: format!("run:{run_id}"),
        event_type: RUN_STEP_REQUESTED.to_owned(),
        aggregate_type: "run".to_owned(),
        aggregate_id: run_id,
        payload: serde_json::json!({
            "run_id": run_id,
            "position": position,
            "attempt": attempt,
        }),
        correlation_id: None,
        causation_id: None,
    };
    // `append`'s only reachable failures here are a database error (already fatal above) or a
    // vocabulary error on two constant strings, so the mapping keeps the caller's one error
    // channel instead of widening it with an event-only variant — the `approval` emitter's shape.
    crate::outbox::append(&mut *conn, tenant, &event)
        .await
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    Ok(())
}

/// The dedupe key of a gate's resume job: the step's own key with a `:gate` suffix.
pub fn process_gate_resume_dedupe_key(run_id: Uuid, position: i32, attempt: i32) -> String {
    format!(
        "{}:gate",
        process_step_dedupe_key(run_id, position, attempt)
    )
}

/// Enqueue the job that resumes a step whose gate was approved.
///
/// It is the same step job — same payload, same `Idempotency-Key` — but with a dedupe slot of its
/// own. Sharing the base key would collapse the resume onto a job that is still `running`: the job
/// that OPENED the gate, when an approval lands before that job is marked complete (an automated
/// approver reacting to `approval.requested`, or a crash between the park commit and completion).
/// That job never re-runs the step, so the collapse would stall the run and hold `uq_run_live`
/// forever. Two live jobs are harmless: `claim_run_step_tx` lets exactly one claim, and the loser
/// is a replay.
pub async fn enqueue_process_gate_resume_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let payload = serde_json::json!({
        "run_id": run_id,
        "position": position,
        "attempt": attempt,
    });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(PROCESS_STEP_KIND)
    .bind(process_gate_resume_dedupe_key(run_id, position, attempt))
    .bind(payload)
    .bind(PROCESS_STEP_MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id,)) = row else {
        return Ok(None);
    };
    // An approval makes the resume due NOW, so the bus is nudged exactly like a chained step.
    // Before this the resume was job-poller-only; a Redis-only deployment waited a full tick.
    announce_step_requested(conn, tenant, run_id, position, attempt).await?;
    Ok(Some(id))
}

// ── run_step ────────────────────────────────────────────────────────────────────────────────

/// Materialise one `run_step` per step of the process version, all `queued` at attempt 1.
pub async fn materialise_run_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    steps: &[ProcessStepRecord],
) -> Result<Vec<RunStepRecord>, ProcessStoreError> {
    let mut out = Vec::with_capacity(steps.len());
    let sql = format!(
        "INSERT INTO run_step
             (tenant_id, run_id, process_step_id, position, attempt, action, status)
         VALUES ($1, $2, $3, $4, 1, $5, 'queued')
         RETURNING {RUN_STEP_COLUMNS}"
    );
    for step in steps {
        // `run_step.action` is still NOT NULL, and it is the trace's record of what the step ran:
        // an `agent` step names the primitive it invokes (publish normalises it), a gate or call
        // step has no action and carries the empty string rather than failing the insert.
        let action = match (step.kind.as_str(), step.action.as_deref()) {
            ("agent", None | Some("")) => conveyor_domain::process::AGENT_STEP_ACTION,
            (_, other) => other.unwrap_or(""),
        };
        let row: RunStepRow = sqlx::query_as(sqlx::AssertSqlSafe(sql.clone()))
            .bind(tenant.as_uuid())
            .bind(run_id)
            .bind(step.id)
            .bind(step.position)
            .bind(action)
            .fetch_one(&mut *conn)
            .await?;
        out.push(row.into());
    }
    Ok(out)
}

/// Claim a queued step for execution. `None` means it was already claimed (a replay) or never materialised.
///
/// The resolved input is persisted in the same statement that flips `queued → running`. That is
/// what makes a crash replay safe: the template resolves against live state, so re-resolving
/// after a crash could produce a different document — a different idempotency hash, or a second
/// effect with different data. A replay reads this column, never a fresh resolution (S11 §5.4/§8).
///
/// The claim also pins the step's SOP version: `run_step.procedure_version` is filled from the
/// `procedure` row `process_step.procedure_id` names, so the version the step was judged against
/// survives even after the SOP is edited (`coalesce` keeps an already-pinned value).
pub async fn claim_run_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
    input: &Value,
) -> Result<Option<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "UPDATE run_step AS step
            SET status = 'running', input = $5, started_at = now(), updated_at = now(),
                procedure_version = coalesce(
                    step.procedure_version,
                    (SELECT procedure.version
                       FROM process_step
                       JOIN procedure
                         ON procedure.id = process_step.procedure_id
                        AND procedure.tenant_id = process_step.tenant_id
                      WHERE process_step.tenant_id = $1 AND process_step.id = step.process_step_id))
          WHERE step.tenant_id = $1 AND step.run_id = $2 AND step.position = $3 AND step.attempt = $4
            AND step.status = 'queued'
          RETURNING {RUN_STEP_COLUMNS}"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(position)
        .bind(attempt)
        .bind(input)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// One `run_step` by its natural key — the replay read.
pub async fn get_run_step_by_triple_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
) -> Result<Option<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {RUN_STEP_COLUMNS} FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(position)
        .bind(attempt)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// One step of a frozen process version, by position.
pub async fn get_step_by_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
    position: i32,
) -> Result<Option<ProcessStepRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {STEP_COLUMNS} FROM process_step
          WHERE tenant_id = $1 AND process_id = $2 AND position = $3"
    );
    let row: Option<StepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(process_id)
        .bind(position)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Mark every still-queued step of a run `skipped`, returning the rows for the events.
///
/// A terminal run must not leave `queued` rows behind: `skipped` is the trace's way of saying
/// "this position was never reached", whether the run failed, was cancelled or was reaped.
pub async fn skip_queued_run_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "UPDATE run_step
            SET status = 'skipped', finished_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND run_id = $2 AND status = 'queued'
          RETURNING {RUN_STEP_COLUMNS}"
    );
    let rows: Vec<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Record the outcome of a running step. Returns `None` when the row is not `running` any more.
pub async fn record_run_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    status: &str,
    input: &Value,
    output: &Value,
    error: Option<&str>,
) -> Result<Option<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "UPDATE run_step
            SET status = $3, input = $4, output = $5, error = $6,
                finished_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'running'
          RETURNING {RUN_STEP_COLUMNS}"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(status)
        .bind(input)
        .bind(output)
        .bind(error)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Record the outcome of a step that never claimed: the gate rejected or expired before the
/// executor could move it `queued → running`.
///
/// `record_run_step_tx` is `running`-guarded by design (a claim must own the row); this is its
/// `queued` twin, used only by the gate-resume path. Returns `None` when the row is no longer
/// `queued` — a cancel or a reaper got there first, and its transaction owns the chain.
pub async fn record_queued_run_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    status: &str,
    error: Option<&str>,
) -> Result<Option<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "UPDATE run_step
            SET status = $3, error = $4, finished_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'queued'
          RETURNING {RUN_STEP_COLUMNS}"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(status)
        .bind(error)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Write the judge's verdict onto one `run_step` row.
///
/// The judge (the SOP's `inspection`) runs AFTER the step recorded its output, so this path is
/// deliberately not status-guarded: it may land on a `succeeded`, `failed` or later-corrected
/// row, and a re-judge overwrites the previous verdict. `None` means the row is absent in this
/// tenant.
pub async fn record_run_step_judge_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_step_id: Uuid,
    judge: &Value,
) -> Result<Option<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "UPDATE run_step
            SET judge = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {RUN_STEP_COLUMNS}"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_step_id)
        .bind(judge)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Append the next attempt at a position — a retry never overwrites the previous attempt's row.
pub async fn append_attempt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    previous_step_id: Uuid,
) -> Result<RunStepRecord, ProcessStoreError> {
    let sql = format!(
        "INSERT INTO run_step (tenant_id, run_id, process_step_id, position, attempt, action, status)
         SELECT $1, run_id, process_step_id, position, attempt + 1, action, 'queued'
           FROM run_step
          WHERE tenant_id = $1 AND id = $2
         RETURNING {RUN_STEP_COLUMNS}"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(previous_step_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(ProcessStoreError::NotFound)
}

/// A run's steps, in execution order.
pub async fn list_run_steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {RUN_STEP_COLUMNS} FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 ORDER BY position, attempt"
    );
    let rows: Vec<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One `run_step` row with its timings, for the run-detail projection.
///
/// Deliberately not [`RunStepRecord`]: that value is carried inside the executor's `Next` enum,
/// and two extra timestamps push that variant over clippy's size budget. The detail read is the
/// only caller that needs the times.
#[derive(Clone, Debug)]
pub struct RunStepDetail {
    /// The row id.
    pub id: Uuid,
    /// The step of the published process version, when the row came from one.
    pub process_step_id: Option<Uuid>,
    /// 1-based order.
    pub position: i32,
    /// The attempt number (1-based; a retry appends).
    pub attempt: i32,
    /// The action this step invokes.
    pub action: String,
    /// `queued | running | succeeded | failed | skipped`.
    pub status: String,
    /// The resolved input, once it ran.
    pub input: Value,
    /// The action's output, once it succeeded.
    pub output: Value,
    /// The failure message, when it failed.
    pub error: Option<String>,
    /// The judge's verdict document, when the step was judged.
    pub judge: Option<Value>,
    /// When the step started.
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the step finished.
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct RunStepDetailRow {
    id: Uuid,
    process_step_id: Option<Uuid>,
    position: i32,
    attempt: i32,
    action: String,
    status: String,
    input: Value,
    output: Value,
    error: Option<String>,
    judge: Option<Value>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One run's trace with timings, ordered by position then attempt.
pub async fn list_run_steps_detail_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<RunStepDetail>, ProcessStoreError> {
    let rows: Vec<RunStepDetailRow> = sqlx::query_as(
        "SELECT id, process_step_id, position, attempt, action, status, input, output, error,
                judge, started_at, finished_at
           FROM run_step
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY position, attempt",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| RunStepDetail {
            id: row.id,
            process_step_id: row.process_step_id,
            position: row.position,
            attempt: row.attempt,
            action: row.action,
            status: row.status,
            input: row.input,
            output: row.output,
            error: row.error,
            judge: row.judge,
            started_at: row.started_at,
            finished_at: row.finished_at,
        })
        .collect())
}

/// The step a run is waiting to execute, if any: its lowest queued or running position.
pub async fn next_pending_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<RunStepRecord>, ProcessStoreError> {
    let sql = format!(
        "SELECT {RUN_STEP_COLUMNS} FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND status IN ('queued', 'running')
          ORDER BY position, attempt LIMIT 1"
    );
    let row: Option<RunStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// A run's `process_id`, read from the run row so the caller need not trust a job payload.
pub async fn run_process_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<Uuid>, ProcessStoreError> {
    let row: Option<(Option<Uuid>,)> =
        sqlx::query_as("SELECT process_id FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.and_then(|(id,)| id))
}

/// One step whose status the process reaper changed.
#[derive(Clone, Debug)]
pub struct ReapedProcessStep {
    /// 1-based position.
    pub position: i32,
    /// The attempt number.
    pub attempt: i32,
    /// The status it was moved to (`failed` for the driven step, `skipped` for the rest).
    pub status: String,
}

/// One process run the reaper closed because its driving `process.step` job died.
#[derive(Clone, Debug)]
pub struct ReapedProcessRun {
    /// The run that was failed.
    pub run_id: Uuid,
    /// The dead job that named it.
    pub dead_job_id: Uuid,
    /// The run's attempt number, for the `run.failed` payload.
    pub run_attempt: i32,
    /// Every step whose status changed, so the caller can emit one event per transition.
    pub steps: Vec<ReapedProcessStep>,
}

/// Fail every process run whose driving `process.step` job is dead.
///
/// The sibling of [`crate::run::reap_dead_runs_tx`], for the same reason: a worker killed with
/// `SIGKILL` cannot write anything, and once its job's attempts are spent the job is dead-lettered.
/// A process run is driven only by `process.step` jobs (start never enqueues `run.execute`), so
/// without this half the run would stay `running` forever and hold `uq_run_live` on its card — the
/// card could never start another run (S11 step 8, decision 2).
///
/// A run is only here when it has **no** live step job: a pending/running job for any position means
/// the run is still driven and is never touched. The dead job's dedupe key is matched to its
/// `run_step` by string equality — never by casting a jsonb value — so a malformed payload cannot
/// abort the sweep.
///
/// Returns the runs it closed, with every step transition for the caller's events. The run row is
/// failed FIRST: its status guard is the authority, and a run that moved underneath (cancelled,
/// finished) is left alone entirely.
pub async fn reap_dead_process_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<ReapedProcessRun>, sqlx::Error> {
    let candidates: Vec<(Uuid, Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT ON (r.id) r.id, j.id, j.dedupe_key, j.last_error
           FROM run r
           JOIN job j
             ON j.tenant_id = r.tenant_id
            AND j.kind = 'process.step'
            AND j.dedupe_key LIKE r.id::text || ':%'
          WHERE r.tenant_id = $1
            AND r.process_id IS NOT NULL
            AND r.status IN ('queued', 'running')
            AND j.status = 'dead'
            AND NOT EXISTS (
                SELECT 1 FROM job live
                 WHERE live.tenant_id = r.tenant_id
                   AND live.kind = 'process.step'
                   AND live.status IN ('pending', 'running')
                   AND live.dedupe_key LIKE r.id::text || ':%')
          ORDER BY r.id, j.created_at DESC, j.id DESC",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;

    let mut reaped = Vec::with_capacity(candidates.len());
    for (run_id, dead_job_id, dedupe_key, last_error) in candidates {
        // S13 §5.4: a step that timed out fails the run with `error_class = 'timeout'`, not the
        // generic `crashed`. The job's `last_error` is where the action's 504 landed, because a
        // timeout is retryable and the step row never records it until the attempts are spent.
        let timed_out = last_error
            .as_deref()
            .map(|error| error.contains("action-timeout"))
            .unwrap_or(false);
        let (error_class, error_message) = if timed_out {
            ("timeout", "a step action did not finish within its timeout")
        } else {
            ("crashed", "the process.step job died")
        };
        let failed_run: Option<(i32,)> = sqlx::query_as(
            "UPDATE run
                SET status = 'failed',
                    error_class = $3,
                    error_message = $4,
                    version = version + 1,
                    updated_at = now(),
                    finished_at = coalesce(finished_at, now())
              WHERE tenant_id = $1 AND id = $2 AND status IN ('queued', 'running')
              RETURNING attempt",
        )
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(error_class)
        .bind(error_message)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((run_attempt,)) = failed_run else {
            // The run moved between the candidate read and this write; it is not ours to fail.
            continue;
        };

        // The step the dead job names. The dedupe key carries a per-transition suffix —
        // `<run>:<pos>:<attempt>`, `…:wait:<n>` for a judge park, `…:gate` for a resume,
        // `…:call:<n>` for a call poll — so the match is PREFIX, not equality: with equality a
        // dead park/gate/call job named no step and left it `running` forever while the run was
        // failed around it (audit 2026-11-10). String matching, not a cast: a malformed key
        // simply matches nothing instead of failing the whole sweep.
        let driven: Option<(i32, i32)> = sqlx::query_as(
            "SELECT position, attempt FROM run_step
              WHERE tenant_id = $1 AND run_id = $2
                AND $3 LIKE $2::text || ':' || position::text || ':' || attempt::text || '%'
              ORDER BY position, attempt
              LIMIT 1",
        )
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(&dedupe_key)
        .fetch_optional(&mut *conn)
        .await?;

        let mut steps = Vec::new();
        if let Some((position, attempt)) = driven {
            let row: Option<(i32, i32, String)> = sqlx::query_as(
                "UPDATE run_step
                    SET status = 'failed', error = $5, finished_at = now(), updated_at = now()
                  WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4
                    AND status IN ('queued', 'running')
                  RETURNING position, attempt, status",
            )
            .bind(tenant.as_uuid())
            .bind(run_id)
            .bind(position)
            .bind(attempt)
            .bind("the process.step job died")
            .fetch_optional(&mut *conn)
            .await?;
            if let Some((position, attempt, status)) = row {
                steps.push(ReapedProcessStep {
                    position,
                    attempt,
                    status,
                });
            }
        }
        let skipped: Vec<(i32, i32, String)> = sqlx::query_as(
            "UPDATE run_step
                SET status = 'skipped', finished_at = now(), updated_at = now()
              WHERE tenant_id = $1 AND run_id = $2 AND status = 'queued'
              RETURNING position, attempt, status",
        )
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await?;
        steps.extend(
            skipped
                .into_iter()
                .map(|(position, attempt, status)| ReapedProcessStep {
                    position,
                    attempt,
                    status,
                }),
        );

        // A reaped run that started children must not leave them live: they are separate runs
        // with their own jobs and would hold `uq_run_child_live` under a failed parent.
        crate::run::cancel_descendants_tx(&mut *conn, tenant, run_id)
            .await
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;

        reaped.push(ReapedProcessRun {
            run_id,
            dead_job_id,
            run_attempt,
            steps,
        });
    }
    Ok(reaped)
}

fn is_unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
        .is_some_and(|name| name == constraint)
}

/// A create that collides with an existing row of the same key.
///
/// Postgres reports ONE violated constraint, and a second `version 1` of a key violates both
/// `process_tenant_key_version_key` and `uq_process_draft` — so matching only the draft index turns a
/// 409 into a 500. Both names mean "this key already has a draft".
fn is_draft_conflict(error: &sqlx::Error) -> bool {
    is_unique_violation(error, "uq_process_draft")
        || is_unique_violation(error, "process_tenant_key_version_key")
}

fn version_of(raw: i64) -> Version {
    Version::new(u64::try_from(raw).unwrap_or(1)).unwrap_or(Version::INITIAL)
}
