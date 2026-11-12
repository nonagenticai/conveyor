#![forbid(unsafe_code)]
//! The **procedure (SOP)** store — the version family a process step is judged against.
//!
//! The shape is the same one `process` uses (S11 §5.3): rows share `(tenant_id, key)`, each
//! with a `version` and a `status`; `create` is the only way a version appears; `update` edits a
//! draft; `publish` retires the previous published row and freezes this one. The partial unique
//! indexes — not a check-then-insert — are what make "one draft and one published per key" true
//! under concurrency.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

fn is_unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
        .is_some_and(|name| name == constraint)
}

/// A create that collides with an existing row of the same key.
///
/// Postgres reports ONE violated constraint, and a race on the version can violate
/// `procedure_tenant_key_version_key` while `uq_procedure_draft` is the one the caller means.
fn is_draft_conflict(error: &sqlx::Error) -> bool {
    is_unique_violation(error, "uq_procedure_draft")
        || is_unique_violation(error, "procedure_tenant_key_version_key")
}

/// Why an SOP statement did not land.
#[derive(Debug, thiserror::Error)]
pub enum SopStoreError {
    /// No such row in this tenant.
    #[error("the procedure does not exist in this tenant")]
    NotFound,
    /// A draft already exists for this key (`uq_procedure_draft`).
    #[error("a draft of this procedure key already exists")]
    DraftExists,
    /// The row is not a draft, so it cannot be edited or published.
    #[error("the procedure is {0}, not a draft")]
    NotDraft(String),
    /// The row is not published, so it cannot be retired.
    #[error("the procedure is not published")]
    NotPublished,
    /// `If-Match` named a version the row is not at.
    #[error("the procedure is at version {current}, not the version the caller supplied")]
    VersionMismatch {
        /// The row's current version.
        current: i64,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The columns every read returns.
///
/// `created_at`/`updated_at` are read because a page's keyset cursor is `(created_at, id)`: a
/// cursor the caller cannot derive from the rows it was handed is not a cursor.
const COLUMNS: &str = "id, key, title, description, status, version, lock_version, \
    definition_of_ready, definition_of_done, acceptance_criteria, inspection, outputs, \
    created_at, updated_at";

/// A procedure row as stored.
#[derive(Clone, Debug)]
pub struct SopRecord {
    /// The row id.
    pub id: Uuid,
    /// The family key.
    pub key: String,
    /// What a person reads.
    pub title: String,
    /// Free text.
    pub description: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The family version.
    pub version: i64,
    /// The optimistic-concurrency counter (`If-Match` compares this, not `version`).
    pub lock_version: i64,
    /// The definition of ready.
    pub definition_of_ready: Value,
    /// The definition of done.
    pub definition_of_done: Value,
    /// The acceptance criteria the judge reads.
    pub acceptance_criteria: Value,
    /// `{kind: laya|eval|checker, ...}`.
    pub inspection: Value,
    /// The declared user-facing outputs.
    pub outputs: Value,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct SopRow {
    id: Uuid,
    key: String,
    title: String,
    description: String,
    status: String,
    version: i64,
    lock_version: i64,
    definition_of_ready: Value,
    definition_of_done: Value,
    acceptance_criteria: Value,
    inspection: Value,
    outputs: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SopRow> for SopRecord {
    fn from(row: SopRow) -> Self {
        Self {
            id: row.id,
            key: row.key,
            title: row.title,
            description: row.description,
            status: row.status,
            version: row.version,
            lock_version: row.lock_version,
            definition_of_ready: row.definition_of_ready,
            definition_of_done: row.definition_of_done,
            acceptance_criteria: row.acceptance_criteria,
            inspection: row.inspection,
            outputs: row.outputs,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// What a caller supplies to start a version.
#[derive(Clone, Debug)]
pub struct NewSop {
    /// The family key.
    pub key: String,
    /// What a person reads.
    pub title: String,
    /// Free text.
    pub description: String,
    /// The definition of ready.
    pub definition_of_ready: Value,
    /// The definition of done.
    pub definition_of_done: Value,
    /// The acceptance criteria.
    pub acceptance_criteria: Value,
    /// The inspection.
    pub inspection: Value,
    /// The declared outputs.
    pub outputs: Value,
}

/// A partial edit of a draft; `None` leaves the column as it is.
#[derive(Clone, Debug, Default)]
pub struct SopPatch {
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New definition of ready.
    pub definition_of_ready: Option<Value>,
    /// New definition of done.
    pub definition_of_done: Option<Value>,
    /// New acceptance criteria.
    pub acceptance_criteria: Option<Value>,
    /// New inspection.
    pub inspection: Option<Value>,
    /// New outputs.
    pub outputs: Option<Value>,
}

/// Create the next version of a key as a draft.
pub async fn create_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewSop,
) -> Result<SopRecord, SopStoreError> {
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) + 1 FROM procedure WHERE tenant_id = $1 AND key = $2",
    )
    .bind(tenant.as_uuid())
    .bind(&input.key)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "INSERT INTO procedure (tenant_id, key, title, description, version, definition_of_ready,
                                definition_of_done, acceptance_criteria, inspection, outputs)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {COLUMNS}"
    );
    match sqlx::query_as::<_, SopRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.key)
        .bind(&input.title)
        .bind(&input.description)
        .bind(next)
        .bind(&input.definition_of_ready)
        .bind(&input.definition_of_done)
        .bind(&input.acceptance_criteria)
        .bind(&input.inspection)
        .bind(&input.outputs)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(row) => Ok(row.into()),
        Err(error) if is_draft_conflict(&error) => Err(SopStoreError::DraftExists),
        Err(error) => Err(SopStoreError::Database(error)),
    }
}

/// Read one row by id.
pub async fn get_sop_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<SopRecord>, SopStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM procedure WHERE tenant_id = $1 AND id = $2");
    let row: Option<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// A key's version: the published one when it exists, else the latest.
pub async fn get_sop_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: Option<i64>,
) -> Result<Option<SopRecord>, SopStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM procedure
          WHERE tenant_id = $1 AND key = $2 AND ($3::bigint IS NULL OR version = $3)
          ORDER BY (status = 'published') DESC, version DESC
          LIMIT 1"
    );
    let row: Option<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .bind(version)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// The published version of a key.
pub async fn published_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<SopRecord>, SopStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM procedure
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'"
    );
    let row: Option<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Every published SOP id — the fact a process publish resolves `procedure_id` against.
pub async fn published_ids_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<std::collections::BTreeSet<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM procedure WHERE tenant_id = $1 AND status = 'published'")
            .bind(tenant.as_uuid())
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// A page of the tenant's SOPs, newest first, optionally filtered by status.
pub async fn list_sops_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<SopRecord>, SopStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM procedure
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let (created_at, id) = match after {
        Some((created_at, id)) => (Some(created_at), Some(id)),
        None => (None, None),
    };
    let rows: Vec<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(created_at)
        .bind(id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Edit a draft. `expected_lock` is the `If-Match` value the caller saw.
pub async fn update_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_lock: Option<i64>,
    patch: &SopPatch,
) -> Result<SopRecord, SopStoreError> {
    let sql = format!(
        "UPDATE procedure SET
             title = COALESCE($3, title),
             description = COALESCE($4, description),
             definition_of_ready = COALESCE($5, definition_of_ready),
             definition_of_done = COALESCE($6, definition_of_done),
             acceptance_criteria = COALESCE($7, acceptance_criteria),
             inspection = COALESCE($8, inspection),
             outputs = COALESCE($9, outputs),
             lock_version = lock_version + 1,
             updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'draft'
            AND ($10::bigint IS NULL OR lock_version = $10)
          RETURNING {COLUMNS}"
    );
    let row: Option<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.title.as_deref())
        .bind(patch.description.as_deref())
        .bind(patch.definition_of_ready.as_ref())
        .bind(patch.definition_of_done.as_ref())
        .bind(patch.acceptance_criteria.as_ref())
        .bind(patch.inspection.as_ref())
        .bind(patch.outputs.as_ref())
        .bind(expected_lock)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = row {
        return Ok(row.into());
    }
    let current = get_sop_tx(&mut *conn, tenant, id).await?;
    match current {
        None => Err(SopStoreError::NotFound),
        Some(current) if current.status != "draft" => Err(SopStoreError::NotDraft(current.status)),
        Some(current) => Err(SopStoreError::VersionMismatch {
            current: current.lock_version,
        }),
    }
}

/// Publish a draft: retire the key's previous published version, then flip this one.
pub async fn publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<SopRecord, SopStoreError> {
    let current = lock_sop_tx(&mut *conn, tenant, id).await?;
    if current.status != "draft" {
        return Err(SopStoreError::NotDraft(current.status));
    }
    sqlx::query(
        "UPDATE procedure SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'",
    )
    .bind(tenant.as_uuid())
    .bind(&current.key)
    .execute(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE procedure SET status = 'published', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 RETURNING {COLUMNS}"
    );
    let row: SopRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Retire a published SOP.
pub async fn retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<SopRecord, SopStoreError> {
    let sql = format!(
        "UPDATE procedure SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'published'
          RETURNING {COLUMNS}"
    );
    let row: Option<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => Ok(row.into()),
        None => {
            let current = get_sop_tx(&mut *conn, tenant, id).await?;
            match current {
                None => Err(SopStoreError::NotFound),
                Some(_) => Err(SopStoreError::NotPublished),
            }
        }
    }
}

async fn lock_sop_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<SopRecord, SopStoreError> {
    let sql =
        format!("SELECT {COLUMNS} FROM procedure WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<SopRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(SopStoreError::NotFound)
}
