#![forbid(unsafe_code)]
//! The **process map** store — the user-facing DAG bound to a project type (SPEC D30, CONTRACT §21).
//!
//! The shape is the same version family [`crate::procedure`] and [`crate::process`] use: rows share
//! `(tenant_id, key)`, each with a `version` and a `status`; `create` is the only way a version
//! appears; `update` edits a draft; `publish` retires the previous published row of the same
//! `(key, project_type_id)` and flips this one. The partial unique indexes — not a check-then-insert —
//! are what make "one draft and one published per key and project type" true under concurrency.
//!
//! A map is **declarative**: `definition` is the `{schema, nodes, edges}` document, and nothing here
//! executes it. Validation is `conveyor_domain::process_map::validate`, which the publish caller runs
//! before it gets here; the store only owns the rows.

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
/// `process_map_tenant_key_version_key` while `uq_process_map_draft` is the one the caller means.
fn is_draft_conflict(error: &sqlx::Error) -> bool {
    is_unique_violation(error, "uq_process_map_draft")
        || is_unique_violation(error, "process_map_tenant_key_version_key")
}

/// Why a map statement did not land.
#[derive(Debug, thiserror::Error)]
pub enum MapStoreError {
    /// No such row in this tenant.
    #[error("the process map does not exist in this tenant")]
    NotFound,
    /// A draft already exists for this key and project type (`uq_process_map_draft`).
    #[error("a draft of this process map key already exists")]
    DraftExists,
    /// The row is not a draft, so it cannot be edited or published.
    #[error("the process map is {0}, not a draft")]
    NotDraft(String),
    /// The row is not published, so it cannot be retired.
    #[error("the process map is not published")]
    NotPublished,
    /// The project type a map is drawn for does not exist in this tenant.
    #[error("the referenced project type does not exist in this tenant")]
    ProjectTypeNotFound,
    /// `If-Match` named a version the row is not at.
    #[error("the process map is at version {current}, not the version the caller supplied")]
    VersionMismatch {
        /// The row's current `lock_version`.
        current: i64,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The columns every read returns.
const COLUMNS: &str = "id, key, title, status, version, lock_version, project_type_id, definition, \
    created_at, updated_at";

/// A map row as stored.
#[derive(Clone, Debug)]
pub struct MapRecord {
    /// The row id.
    pub id: Uuid,
    /// The family key.
    pub key: String,
    /// What a person reads.
    pub title: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The family version.
    pub version: i64,
    /// The optimistic-concurrency counter (`If-Match` compares this, not `version`).
    pub lock_version: i64,
    /// The `catalogue_entry(kind='project_type')` the map is drawn for.
    pub project_type_id: Uuid,
    /// The `{schema, nodes, edges}` document.
    pub definition: Value,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct MapRow {
    id: Uuid,
    key: String,
    title: String,
    status: String,
    version: i64,
    lock_version: i64,
    project_type_id: Uuid,
    definition: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<MapRow> for MapRecord {
    fn from(row: MapRow) -> Self {
        Self {
            id: row.id,
            key: row.key,
            title: row.title,
            status: row.status,
            version: row.version,
            lock_version: row.lock_version,
            project_type_id: row.project_type_id,
            definition: row.definition,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// What a caller supplies to start a version.
#[derive(Clone, Debug)]
pub struct NewMap {
    /// The family key.
    pub key: String,
    /// What a person reads.
    pub title: String,
    /// The project type the map is drawn for.
    pub project_type_id: Uuid,
    /// The `{schema, nodes, edges}` document.
    pub definition: Value,
}

/// A partial edit of a draft; `None` leaves the column as it is.
#[derive(Clone, Debug, Default)]
pub struct MapPatch {
    /// New title.
    pub title: Option<String>,
    /// New project type (a draft may be re-bound before it is published).
    pub project_type_id: Option<Uuid>,
    /// The replacement document, whole.
    pub definition: Option<Value>,
}

/// Create the next version of a key as a draft.
pub async fn create_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewMap,
) -> Result<MapRecord, MapStoreError> {
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) + 1 FROM process_map WHERE tenant_id = $1 AND key = $2",
    )
    .bind(tenant.as_uuid())
    .bind(&input.key)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "INSERT INTO process_map (tenant_id, key, title, version, project_type_id, definition)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    );
    match sqlx::query_as::<_, MapRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.key)
        .bind(&input.title)
        .bind(next)
        .bind(input.project_type_id)
        .bind(&input.definition)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(row) => Ok(row.into()),
        Err(error) if is_draft_conflict(&error) => Err(MapStoreError::DraftExists),
        Err(error) => Err(MapStoreError::Database(error)),
    }
}

/// Read one row by id.
pub async fn get_map_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<MapRecord>, MapStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM process_map WHERE tenant_id = $1 AND id = $2");
    let row: Option<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// A key's version **for one project type**: the published one when it exists, else the latest.
///
/// The binding is part of the identity, so a key may be published for two project types at once and
/// the response never depends on which row happens to sort first.
pub async fn get_map_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    project_type_id: Uuid,
    version: Option<i64>,
) -> Result<Option<MapRecord>, MapStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM process_map
          WHERE tenant_id = $1 AND key = $2 AND project_type_id = $3
            AND ($4::bigint IS NULL OR version = $4)
          ORDER BY (status = 'published') DESC, version DESC
          LIMIT 1"
    );
    let row: Option<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .bind(project_type_id)
        .bind(version)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Every published map of one project type, in key order.
///
/// A project type is not itself unique per key — `uq_process_map_published` is
/// `(tenant_id, key, project_type_id)` — so this is a list, never a single row.
pub async fn published_by_project_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_type_id: Uuid,
) -> Result<Vec<MapRecord>, MapStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM process_map
          WHERE tenant_id = $1 AND project_type_id = $2 AND status = 'published'
          ORDER BY key"
    );
    let rows: Vec<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_type_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// A page of the tenant's maps, newest first, optionally filtered by status and project type.
pub async fn list_maps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    project_type_id: Option<Uuid>,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<MapRecord>, MapStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM process_map
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::uuid IS NULL OR project_type_id = $3)
            AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
          ORDER BY created_at DESC, id DESC
          LIMIT $6"
    );
    let (created_at, id) = match after {
        Some((created_at, id)) => (Some(created_at), Some(id)),
        None => (None, None),
    };
    let rows: Vec<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(project_type_id)
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
    patch: &MapPatch,
) -> Result<MapRecord, MapStoreError> {
    let sql = format!(
        "UPDATE process_map SET
             title = COALESCE($3, title),
             project_type_id = COALESCE($4, project_type_id),
             definition = COALESCE($5, definition),
             lock_version = lock_version + 1,
             updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'draft'
            AND ($6::bigint IS NULL OR lock_version = $6)
          RETURNING {COLUMNS}"
    );
    let row: Option<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.title.as_deref())
        .bind(patch.project_type_id)
        .bind(patch.definition.as_ref())
        .bind(expected_lock)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = row {
        return Ok(row.into());
    }
    let current = get_map_tx(&mut *conn, tenant, id).await?;
    match current {
        None => Err(MapStoreError::NotFound),
        Some(current) if current.status != "draft" => Err(MapStoreError::NotDraft(current.status)),
        Some(current) => Err(MapStoreError::VersionMismatch {
            current: current.lock_version,
        }),
    }
}

/// Publish a draft: retire the previous published row of the same key and project type, then flip this
/// one. The caller validates the document before it gets here — the store never re-reads the facts.
pub async fn publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<MapRecord, MapStoreError> {
    let current = lock_map_tx(&mut *conn, tenant, id).await?;
    if current.status != "draft" {
        return Err(MapStoreError::NotDraft(current.status));
    }
    sqlx::query(
        "UPDATE process_map SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND project_type_id = $3 AND status = 'published'",
    )
    .bind(tenant.as_uuid())
    .bind(&current.key)
    .bind(current.project_type_id)
    .execute(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE process_map SET status = 'published', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 RETURNING {COLUMNS}"
    );
    let row: MapRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Retire a published map.
pub async fn retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<MapRecord, MapStoreError> {
    let sql = format!(
        "UPDATE process_map SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'published'
          RETURNING {COLUMNS}"
    );
    let row: Option<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => Ok(row.into()),
        None => {
            let current = get_map_tx(&mut *conn, tenant, id).await?;
            match current {
                None => Err(MapStoreError::NotFound),
                Some(_) => Err(MapStoreError::NotPublished),
            }
        }
    }
}

async fn lock_map_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<MapRecord, MapStoreError> {
    let sql =
        format!("SELECT {COLUMNS} FROM process_map WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<MapRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(MapStoreError::NotFound)
}

/// The facts `conveyor_domain::process_map::validate` needs for one project type.
///
/// Both callers that publish a map — the seed loader and `process_maps.publish` — must validate
/// against the SAME facts, so this is one read, not two. The phases are the project type's own
/// `meta.phases` (there are no `project_type_phase` rows); a map may only place a node in a phase
/// the type actually walks.
pub async fn facts_for_project_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_type_id: Uuid,
) -> Result<conveyor_domain::process_map::MapFacts, MapStoreError> {
    let phases: Option<Value> = sqlx::query_scalar(
        "SELECT meta FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'project_type' AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(project_type_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(meta) = phases else {
        return Err(MapStoreError::ProjectTypeNotFound);
    };
    let capabilities: Vec<String> =
        sqlx::query_scalar("SELECT code FROM capability WHERE tenant_id = $1")
            .bind(tenant.as_uuid())
            .fetch_all(&mut *conn)
            .await?;
    let milestones: Vec<String> = sqlx::query_scalar(
        "SELECT code FROM catalogue_entry WHERE tenant_id = $1 AND kind = 'milestone_kind'",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;
    Ok(conveyor_domain::process_map::MapFacts {
        processes: crate::process::published_keys_tx(&mut *conn, tenant).await?,
        capabilities: capabilities.into_iter().collect(),
        phases: meta
            .get("phases")
            .and_then(Value::as_array)
            .map(|phases| {
                phases
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        milestones: milestones.into_iter().collect(),
    })
}
