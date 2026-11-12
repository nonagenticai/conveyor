//! Repositories as references, never clones (S09 §5.3).
//!
//! A repository row describes where code lives — `provider`, `url`, `default_branch`, the provider's
//! installation handle — and optionally points at a `secret` row for its credential. The conveyor does not
//! clone, mirror or index anything here: coding work is an external `coding_jobs.create`, and the repository
//! is what that call names.
//!
//! `project_repository` is the link. It has no id and no version: the identity is `(project, repository)`,
//! and the only writes are link and unlink.

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};

/// A repository row as the API returns it.
#[derive(Clone, Debug, PartialEq)]
pub struct RepositoryRecord {
    /// The row id.
    pub id: Uuid,
    /// `github` today; free text because the provider handle is the consumer's.
    pub provider: String,
    /// The clone URL.
    pub url: String,
    /// The branch a job uses when it names none.
    pub default_branch: String,
    /// The provider's installation id, opaque.
    pub installation_ref: Option<String>,
    /// The `secret` row that holds the credential, when one is configured.
    pub credential_secret_id: Option<Uuid>,
    /// The optimistic-concurrency version (`ETag`).
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// The fields a create supplies.
#[derive(Clone, Debug)]
pub struct NewRepository {
    /// `github` unless the caller says otherwise.
    pub provider: String,
    /// The clone URL.
    pub url: String,
    /// The default branch.
    pub default_branch: String,
    /// The provider installation handle.
    pub installation_ref: Option<String>,
    /// The credential secret, resolved in the caller's tenant.
    pub credential_secret_id: Option<Uuid>,
}

/// What an update may change. `url` is immutable: it is the repository's identity.
#[derive(Clone, Debug, Default)]
pub struct RepositoryPatch {
    /// The new default branch.
    pub default_branch: Option<String>,
    /// The new installation handle.
    pub installation_ref: Option<Option<String>>,
    /// The new credential secret (`Some(None)` clears it).
    pub credential_secret_id: Option<Option<Uuid>>,
}

/// Why a repository read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryStoreError {
    /// No such row in this tenant.
    #[error("the repository does not exist in this tenant")]
    NotFound,
    /// `(tenant_id, url)` is unique.
    #[error("a repository with that url already exists in this tenant")]
    UrlTaken,
    /// The row moved.
    #[error("the repository is at version {current}, not the expected version")]
    VersionMismatch {
        /// The version the row holds now.
        current: Version,
    },
    /// The link already exists.
    #[error("that repository is already linked to the project")]
    AlreadyLinked,
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const REPOSITORY_COLUMNS: &str = "id, provider, url, default_branch, installation_ref, \
                                  credential_secret_id, version, created_at, updated_at";

type RepositoryRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<Uuid>,
    i64,
    DateTime<Utc>,
    DateTime<Utc>,
);

/// One row of the linked-repositories join: the repository columns plus the link's role.
type ProjectRepositoryRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<Uuid>,
    i64,
    DateTime<Utc>,
    DateTime<Utc>,
    String,
);

impl From<RepositoryRow> for RepositoryRecord {
    fn from(row: RepositoryRow) -> Self {
        Self {
            id: row.0,
            provider: row.1,
            url: row.2,
            default_branch: row.3,
            installation_ref: row.4,
            credential_secret_id: row.5,
            version: crate::version_of(row.6),
            created_at: row.7,
            updated_at: row.8,
        }
    }
}

/// Insert one repository.
pub async fn create_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewRepository,
) -> Result<RepositoryRecord, RepositoryStoreError> {
    if let Some(secret) = input.credential_secret_id {
        ensure_parent_tx(&mut *conn, tenant, Parent::Secret, secret).await?;
    }
    let sql = format!(
        "INSERT INTO repository (tenant_id, provider, url, default_branch, installation_ref, \
                                 credential_secret_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {REPOSITORY_COLUMNS}"
    );
    let row: RepositoryRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.provider)
        .bind(&input.url)
        .bind(&input.default_branch)
        .bind(&input.installation_ref)
        .bind(input.credential_secret_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.is_unique_violation() => RepositoryStoreError::UrlTaken,
            _ => RepositoryStoreError::Database(error),
        })?;
    Ok(row.into())
}

/// One repository, or `None` when it is not this tenant's.
pub async fn get_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RepositoryRecord>, RepositoryStoreError> {
    let sql =
        format!("SELECT {REPOSITORY_COLUMNS} FROM repository WHERE tenant_id = $1 AND id = $2");
    let row: Option<RepositoryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// This tenant's repositories, ordered by url.
pub async fn list_repositories_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<RepositoryRecord>, RepositoryStoreError> {
    let sql = format!(
        "SELECT {REPOSITORY_COLUMNS} FROM repository WHERE tenant_id = $1 ORDER BY url, id LIMIT 1000"
    );
    let rows: Vec<RepositoryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Update a repository's mutable fields, at the caller's version.
pub async fn update_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &RepositoryPatch,
) -> Result<RepositoryRecord, RepositoryStoreError> {
    let current = lock_repository_tx(&mut *conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(RepositoryStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let secret = match patch.credential_secret_id {
        Some(value) => value,
        None => current.credential_secret_id,
    };
    if let Some(secret) = secret {
        ensure_parent_tx(&mut *conn, tenant, Parent::Secret, secret).await?;
    }
    let installation_ref = match &patch.installation_ref {
        Some(value) => value.clone(),
        None => current.installation_ref.clone(),
    };
    let sql = format!(
        "UPDATE repository
            SET default_branch = $3, installation_ref = $4, credential_secret_id = $5,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {REPOSITORY_COLUMNS}"
    );
    let row: RepositoryRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(
            patch
                .default_branch
                .clone()
                .unwrap_or(current.default_branch.clone()),
        )
        .bind(installation_ref)
        .bind(secret)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete one repository and every project link to it, guarded by the caller's version.
///
/// The versioned delete is what makes `If-Match` mean something on a destructive action: the
/// handler's read and this delete are two statements, and a concurrent update between them must be
/// refused (412) rather than destroyed. The link rows are removed only after the guard passes.
pub async fn delete_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<(), RepositoryStoreError> {
    let current: Option<(i64,)> = sqlx::query_as(
        "SELECT version FROM repository WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let (current,) = current.ok_or(RepositoryStoreError::NotFound)?;
    let current_version = crate::version_of(current);
    if current_version != expected_version {
        return Err(RepositoryStoreError::VersionMismatch {
            current: current_version,
        });
    }
    sqlx::query("DELETE FROM project_repository WHERE tenant_id = $1 AND repository_id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM repository WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Link a repository to a project. Idempotency is the caller's: a second link is `AlreadyLinked`.
pub async fn link_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    repository_id: Uuid,
    role: &str,
) -> Result<(), RepositoryStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Project, project_id).await?;
    ensure_parent_tx(&mut *conn, tenant, Parent::Repository, repository_id).await?;
    sqlx::query(
        "INSERT INTO project_repository (tenant_id, project_id, repository_id, role)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .bind(repository_id)
    .bind(role)
    .execute(&mut *conn)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            RepositoryStoreError::AlreadyLinked
        }
        _ => RepositoryStoreError::Database(error),
    })?;
    Ok(())
}

/// Unlink a repository from a project; the row must exist.
pub async fn unlink_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    repository_id: Uuid,
) -> Result<(), RepositoryStoreError> {
    let affected = sqlx::query(
        "DELETE FROM project_repository
          WHERE tenant_id = $1 AND project_id = $2 AND repository_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .bind(repository_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(RepositoryStoreError::NotFound);
    }
    Ok(())
}

/// The repositories linked to one project, as `(repository, role)`.
pub async fn list_project_repositories_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<(RepositoryRecord, String)>, RepositoryStoreError> {
    let sql = "SELECT r.id, r.provider, r.url, r.default_branch, r.installation_ref, \
                r.credential_secret_id, r.version, r.created_at, r.updated_at, pr.role
           FROM project_repository pr
           JOIN repository r ON r.tenant_id = pr.tenant_id AND r.id = pr.repository_id
          WHERE pr.tenant_id = $1 AND pr.project_id = $2
          ORDER BY pr.role, r.url";
    let rows: Vec<ProjectRepositoryRow> = sqlx::query_as(sql)
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let role = row.9.clone();
            let record = RepositoryRecord::from((
                row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8,
            ));
            (record, role)
        })
        .collect())
}

/// Read the row `FOR UPDATE` and fail with `NotFound` when it is absent.
async fn lock_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<RepositoryRecord, RepositoryStoreError> {
    let sql = format!(
        "SELECT {REPOSITORY_COLUMNS} FROM repository WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<RepositoryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(RepositoryStoreError::NotFound)
}
