//! `project_request` — the CP→conveyor intake (S07, data-arch §2.7).
//!
//! The bridge writes a `draft`; a human approves or rejects it; `promote` creates the project
//! **in the same transaction** through [`crate::project::insert_project_tx`], never by
//! re-calling `projects.create`. Idempotency is `uq (tenant_id, external_ref)`.
//!
//! **Re-posting an existing `external_ref`** updates the payload in place when the row is
//! `draft` or `rejected` (a rejected row re-opens as `draft`), and is refused when the row
//! has moved past that: `promoted` is [`ProjectRequestStoreError::Promoted`] carrying the
//! project id, because S07 §5.4's contract makes that a `409` with the created project.
//! `approved` cannot be re-opened either, and is [`ProjectRequestStoreError::Locked`].
//!
//! Status changes are status-guarded, not version-guarded (CONTRACT §3); an edit is
//! version-guarded through `expected_version` (the caller's `If-Match`).

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::project::{NewProject, ProjectStoreError, insert_project_tx};
use crate::version_of;

/// The four `project_request.status` values (0007's CHECK), in lifecycle order.
pub const STATUSES: [&str; 4] = ["draft", "approved", "rejected", "promoted"];

/// An intake to create (or re-post, keyed by `external_ref`).
#[derive(Clone, Debug)]
pub struct NewProjectRequest {
    /// The bridge's stable id for this request; unique per tenant.
    pub external_ref: String,
    /// Which system sent it (`control-plane`, …).
    pub external_origin: String,
    /// The whole forwarded body; read whole, never filtered.
    pub payload: Value,
    /// The repository the request nominates; copied to the project on promote when the
    /// caller's [`NewProject`] does not name one.
    pub repository_url: Option<String>,
}

impl NewProjectRequest {
    /// A request with an empty payload.
    pub fn new(external_ref: impl Into<String>, external_origin: impl Into<String>) -> Self {
        Self {
            external_ref: external_ref.into(),
            external_origin: external_origin.into(),
            payload: Value::Object(serde_json::Map::new()),
            repository_url: None,
        }
    }
}

/// The editable fields of an existing request (a `PUT`/`PATCH` replacement).
///
/// `repository_url` is three-state because the column is nullable: the outer `None` leaves
/// it alone, `Some(None)` clears it, `Some(Some(url))` sets it.
#[derive(Clone, Debug, Default)]
pub struct ProjectRequestUpdate {
    /// The origin system; unchanged when `None`.
    pub external_origin: Option<String>,
    /// The whole forwarded body; unchanged when `None`.
    pub payload: Option<Value>,
    /// The nominated repository; see the type doc for the three states.
    pub repository_url: Option<Option<String>>,
}

/// One `project_request` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ProjectRequestRecord {
    /// The row id.
    pub id: Uuid,
    /// The bridge's stable id.
    pub external_ref: String,
    /// Which system sent it.
    pub external_origin: String,
    /// The forwarded body.
    pub payload: Value,
    /// One of [`STATUSES`].
    pub status: String,
    /// The project this request became; set by promote.
    pub project_id: Option<Uuid>,
    /// The nominated repository.
    pub repository_url: Option<String>,
    /// The optimistic-concurrency version (`If-Match`).
    pub version: Version,
    /// When the request first arrived.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

/// Why a project-request operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ProjectRequestStoreError {
    /// No such request in this tenant.
    #[error("the project request does not exist in this tenant")]
    NotFound,
    /// The request was already promoted; the caller answers `409` with the project id.
    #[error("the project request was already promoted to project {project_id:?}")]
    Promoted {
        /// The project the request became.
        project_id: Option<Uuid>,
    },
    /// The request is `approved` and cannot be re-opened by a re-post.
    #[error("the project request is {status} and cannot be updated by a re-post")]
    Locked {
        /// The status that blocked the write.
        status: String,
    },
    /// The status-guarded transition refused the current status.
    #[error("cannot move the project request from {from} to {to}")]
    Illegal {
        /// The stored status.
        from: String,
        /// The requested status.
        to: &'static str,
    },
    /// A status filter outside [`STATUSES`].
    #[error(
        "unknown project request status {0:?}: expected one of draft, approved, rejected, promoted"
    )]
    UnknownStatus(String),
    /// Creating the project refused the caller's input (or the database did).
    #[error(transparent)]
    Project(#[from] ProjectStoreError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const COLUMNS: &str = "id, external_ref, external_origin, payload, status, project_id, \
                       repository_url, version, created_at, updated_at";

#[derive(Debug, sqlx::FromRow)]
struct ProjectRequestRow {
    id: Uuid,
    external_ref: String,
    external_origin: String,
    payload: Value,
    status: String,
    project_id: Option<Uuid>,
    repository_url: Option<String>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ProjectRequestRow> for ProjectRequestRecord {
    fn from(row: ProjectRequestRow) -> Self {
        Self {
            id: row.id,
            external_ref: row.external_ref,
            external_origin: row.external_origin,
            payload: row.payload,
            status: row.status,
            project_id: row.project_id,
            repository_url: row.repository_url,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Create or re-post an intake, idempotent on `(tenant, external_ref)`.
///
/// One statement does the work: a fresh key inserts; an existing `draft`/`rejected` row
/// absorbs the new payload (and a `rejected` row re-opens as `draft`) and bumps its version;
/// an `approved`/`promoted` row matches the `DO UPDATE … WHERE` and returns no row, which is
/// then classified by a follow-up `SELECT` — `promoted` becomes
/// [`ProjectRequestStoreError::Promoted`] with the project id, `approved` becomes
/// [`ProjectRequestStoreError::Locked`].
pub async fn create_project_request_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewProjectRequest,
) -> Result<ProjectRequestRecord, ProjectRequestStoreError> {
    let sql = format!(
        "INSERT INTO project_request (tenant_id, external_ref, external_origin, payload,
                                      repository_url)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, external_ref) DO UPDATE
            SET payload = EXCLUDED.payload,
                external_origin = EXCLUDED.external_origin,
                repository_url = EXCLUDED.repository_url,
                status = CASE
                    WHEN project_request.status = 'rejected' THEN 'draft'
                    ELSE project_request.status
                END,
                version = project_request.version + 1,
                updated_at = now()
          WHERE project_request.status IN ('draft', 'rejected')
         RETURNING {COLUMNS}"
    );
    let row: Option<ProjectRequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.external_ref)
        .bind(&input.external_origin)
        .bind(&input.payload)
        .bind(&input.repository_url)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = row {
        return Ok(row.into());
    }

    // No row: either the existing row is `approved`/`promoted`, or a concurrent transaction
    // changed it between our statement and this read. Both are reported from what is stored.
    let existing: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, project_id FROM project_request
          WHERE tenant_id = $1 AND external_ref = $2",
    )
    .bind(tenant.as_uuid())
    .bind(&input.external_ref)
    .fetch_optional(&mut *conn)
    .await?;
    match existing {
        Some((status, project_id)) if status == "promoted" => {
            Err(ProjectRequestStoreError::Promoted { project_id })
        }
        Some((status, _)) => Err(ProjectRequestStoreError::Locked { status }),
        // The unique key guarantees this only if the row vanished under us.
        None => Err(ProjectRequestStoreError::NotFound),
    }
}

/// Read one intake inside the caller's scoped transaction.
pub async fn get_project_request_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProjectRequestRecord>, ProjectRequestStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM project_request WHERE tenant_id = $1 AND id = $2");
    let row: Option<ProjectRequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// One page of intakes, newest first, keyset on `(created_at, id)`.
///
/// `status` is validated against [`STATUSES`]; `cursor` is the pair from the last row of the
/// previous page — encode and decode it with
/// [`crate::actions::encode_time_cursor`] / [`crate::actions::decode_time_cursor`], the
/// workspace's one cursor codec (CONTRACT §5).
pub async fn list_project_requests_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<ProjectRequestRecord>, ProjectRequestStoreError> {
    if let Some(status) = status
        && !STATUSES.contains(&status)
    {
        return Err(ProjectRequestStoreError::UnknownStatus(status.to_owned()));
    }
    let sql = format!(
        "SELECT {COLUMNS}
           FROM project_request
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<ProjectRequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(cursor.map(|c| c.0))
        .bind(cursor.map(|c| c.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Replace the editable fields of an intake, guarded by its version.
///
/// `Ok(None)` means the row is absent **or** the version is stale; the caller re-reads once
/// to tell `404` from `412` (the contract [`crate::list::update_list_tx`] uses). The
/// `external_ref` is the idempotency key and is never edited.
pub async fn update_project_request_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    update: &ProjectRequestUpdate,
    expected_version: Version,
) -> Result<Option<ProjectRequestRecord>, ProjectRequestStoreError> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let (set_repository, repository_url) = match &update.repository_url {
        None => (false, None),
        Some(value) => (true, value.clone()),
    };
    let sql = format!(
        "UPDATE project_request
            SET external_origin = coalesce($3, external_origin),
                payload = coalesce($4, payload),
                repository_url = CASE WHEN $5::boolean THEN $6 ELSE repository_url END,
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $7
          RETURNING {COLUMNS}"
    );
    let row: Option<ProjectRequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(update.external_origin.as_deref())
        .bind(update.payload.as_ref())
        .bind(set_repository)
        .bind(repository_url.as_deref())
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Move a `draft`/`rejected` request to `to`, bumping its version. The one implementation of
/// the two status-guarded transitions.
async fn transition_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    to: &'static str,
) -> Result<ProjectRequestRecord, ProjectRequestStoreError> {
    let current: Option<String> = sqlx::query_scalar(
        "SELECT status FROM project_request WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let from = current.ok_or(ProjectRequestStoreError::NotFound)?;
    if from != "draft" && from != "rejected" {
        return Err(ProjectRequestStoreError::Illegal { from, to });
    }
    let sql = format!(
        "UPDATE project_request
            SET status = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {COLUMNS}"
    );
    let row: ProjectRequestRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(to)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Approve a `draft`/`rejected` request. Anything already `approved` or `promoted` is
/// [`ProjectRequestStoreError::Illegal`] (CONTRACT §3: a status transition is not
/// version-guarded).
pub async fn approve_project_request_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ProjectRequestRecord, ProjectRequestStoreError> {
    transition_tx(conn, tenant, id, "approved").await
}

/// Reject a `draft`/`rejected` request. A promoted request keeps its promotion.
pub async fn reject_project_request_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ProjectRequestRecord, ProjectRequestStoreError> {
    transition_tx(conn, tenant, id, "rejected").await
}

/// Promote an `approved` request by creating its project **in this transaction**.
///
/// The project is written through [`insert_project_tx`], so the same parent resolution and
/// key rules as `projects.create` apply; the request row then carries `status='promoted'`,
/// the new `project_id` and a bumped version. Nothing here calls an action or opens a second
/// transaction.
///
/// Two fields are filled from the request when the caller leaves them empty: `owner_user_id`
/// falls back to `actor`, and `repo_url` falls back to the request's `repository_url`. An
/// explicit value in `input` always wins.
pub async fn promote_project_request_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    input: &NewProject,
    actor: Option<Uuid>,
) -> Result<ProjectRequestRecord, ProjectRequestStoreError> {
    let current: Option<ProjectRequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM project_request WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    )))
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let request: ProjectRequestRecord = current.ok_or(ProjectRequestStoreError::NotFound)?.into();
    if request.status != "approved" {
        return Err(ProjectRequestStoreError::Illegal {
            from: request.status,
            to: "promoted",
        });
    }

    let mut project_input = input.clone();
    if project_input.owner_user_id.is_none() {
        project_input.owner_user_id = actor;
    }
    if project_input.repo_url.is_none() {
        project_input.repo_url = request.repository_url.clone();
    }
    let project = insert_project_tx(&mut *conn, tenant, &project_input).await?;

    let sql = format!(
        "UPDATE project_request
            SET status = 'promoted', project_id = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'approved'
          RETURNING {COLUMNS}"
    );
    let row: Option<ProjectRequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(project.id)
        .fetch_optional(&mut *conn)
        .await?;
    // The `FOR UPDATE` above serialises the transition, so a zero-row UPDATE here is
    // unreachable; report absence rather than unwrapping.
    row.map(Into::into)
        .ok_or(ProjectRequestStoreError::NotFound)
}

// The transition guard is tested against a live database in
// `crates/store/tests/s07_store.rs`; the vocabulary assertion below is the only check that
// needs no database.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_vocabulary_matches_the_check() {
        assert_eq!(STATUSES, ["draft", "approved", "rejected", "promoted"]);
    }
}
