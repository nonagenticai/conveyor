//! Programme and project rows.
//!
//! Both are `_tx` functions: the caller opens a transaction with
//! [`crate::begin_scoped`] and passes the connection, so a write and its outbox event commit
//! together (CONTRACT §7). Every statement binds `tenant_id` explicitly as well as relying on
//! RLS, and every parent id (`programme_id`, `owner_user_id`) is resolved in the caller's
//! scope first — a single-column FK does not enforce tenancy (CONTRACT §2).

use conveyor_domain::{Programme, Project, TenantId};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};
use crate::version_of;

/// A programme to create.
#[derive(Clone, Debug, Default)]
pub struct NewProgramme {
    /// Display name; unique per tenant.
    pub name: String,
    /// Free-text status, `active` when omitted.
    pub status: Option<String>,
    /// The accountable user.
    pub owner_user_id: Option<Uuid>,
}

impl NewProgramme {
    /// A programme with the default status.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: None,
            owner_user_id: None,
        }
    }
}

/// A project to create.
#[derive(Clone, Debug, Default)]
pub struct NewProject {
    /// The short key; unique per tenant.
    pub key: String,
    /// Display name.
    pub name: String,
    /// Free text, empty by default.
    pub description: String,
    /// `draft | active | paused | closed | archived`; `active` when omitted.
    pub status: Option<String>,
    /// The programme this project rolls up to.
    pub programme_id: Option<Uuid>,
    /// The project-type code (`catalogue_entry(kind='project_type').code`).
    pub project_type_code: Option<String>,
    /// The current delivery phase.
    pub phase_code: Option<String>,
    /// The accountable user.
    pub owner_user_id: Option<Uuid>,
    /// The source repository.
    pub repo_url: Option<String>,
    /// Kind-specific fields.
    pub meta: Value,
}

impl NewProject {
    /// A project with the default status and empty payloads.
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            description: String::new(),
            status: None,
            programme_id: None,
            project_type_code: None,
            phase_code: None,
            owner_user_id: None,
            repo_url: None,
            meta: Value::Object(serde_json::Map::new()),
        }
    }
}

/// Why a programme/project write failed.
#[derive(Debug, thiserror::Error)]
pub enum ProjectStoreError {
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// Every suffix of the derived key is taken.
    #[error(
        "the derived project key {base:?} collides with every suffix tried \
         (at most {MAX_KEY_ATTEMPTS}); supply an explicit `key`"
    )]
    KeyExhausted {
        /// The base slug the caller's title produced.
        base: String,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct ProgrammeRow {
    id: Uuid,
    name: String,
    status: String,
    owner_user_id: Option<Uuid>,
    version: i64,
}

impl From<ProgrammeRow> for Programme {
    fn from(row: ProgrammeRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            status: row.status,
            owner_user_id: row.owner_user_id,
            version: version_of(row.version),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectRow {
    id: Uuid,
    key: String,
    name: String,
    description: String,
    status: String,
    programme_id: Option<Uuid>,
    project_type_code: Option<String>,
    phase_code: Option<String>,
    owner_user_id: Option<Uuid>,
    repo_url: Option<String>,
    meta: Value,
    version: i64,
}

impl From<ProjectRow> for Project {
    fn from(row: ProjectRow) -> Self {
        Self {
            id: row.id,
            key: row.key,
            name: row.name,
            description: row.description,
            status: row.status,
            programme_id: row.programme_id,
            project_type_code: row.project_type_code,
            phase_code: row.phase_code,
            owner_user_id: row.owner_user_id,
            repo_url: row.repo_url,
            meta: row.meta,
            version: version_of(row.version),
        }
    }
}

/// Insert a programme inside the caller's scoped transaction.
pub async fn insert_programme_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewProgramme,
) -> Result<Programme, ProjectStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.owner_user_id).await?;
    let row: ProgrammeRow = sqlx::query_as(
        "INSERT INTO programme (tenant_id, name, status, owner_user_id)
         VALUES ($1, $2, coalesce($3, 'active'), $4)
         RETURNING id, name, status, owner_user_id, version",
    )
    .bind(tenant.as_uuid())
    .bind(&input.name)
    .bind(&input.status)
    .bind(input.owner_user_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.into())
}

/// Insert a project inside the caller's scoped transaction.
pub async fn insert_project_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewProject,
) -> Result<Project, ProjectStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::Programme, input.programme_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.owner_user_id).await?;
    let row: ProjectRow = sqlx::query_as(
        "INSERT INTO project (tenant_id, key, name, description, status, programme_id,
                              project_type_code, phase_code, owner_user_id, repo_url, meta)
         VALUES ($1, $2, $3, $4, coalesce($5, 'active'), $6, $7, $8, $9, $10, $11)
         RETURNING id, key, name, description, status, programme_id, project_type_code,
                   phase_code, owner_user_id, repo_url, meta, version",
    )
    .bind(tenant.as_uuid())
    .bind(&input.key)
    .bind(&input.name)
    .bind(&input.description)
    .bind(&input.status)
    .bind(input.programme_id)
    .bind(&input.project_type_code)
    .bind(&input.phase_code)
    .bind(input.owner_user_id)
    .bind(&input.repo_url)
    .bind(&input.meta)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.into())
}

/// Read one project inside the caller's scoped transaction.
pub async fn get_project_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Project>, sqlx::Error> {
    let row: Option<ProjectRow> = sqlx::query_as(
        "SELECT id, key, name, description, status, programme_id, project_type_code,
                phase_code, owner_user_id, repo_url, meta, version
           FROM project WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Into::into))
}

// ── the S05 reads, the derived key and the close transition ─────────────────────────────

/// One keyset page of projects, ordered by `(key, id)`.
///
/// `key` is the natural order: it is unique per tenant and stable, so a cursor never
/// invalidates (an `updated_at` sort would let an edit move a row across a page boundary).
pub async fn list_projects_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    after: Option<(&str, Uuid)>,
    limit: i64,
) -> Result<Vec<Project>, sqlx::Error> {
    let limit = limit.max(1);
    let rows: Vec<ProjectRow> = match after {
        None => {
            sqlx::query_as(
                "SELECT id, key, name, description, status, programme_id, project_type_code,
                        phase_code, owner_user_id, repo_url, meta, version
                   FROM project
                  WHERE tenant_id = $1
                  ORDER BY key, id
                  LIMIT $2",
            )
            .bind(tenant.as_uuid())
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
        Some((key, id)) => {
            sqlx::query_as(
                "SELECT id, key, name, description, status, programme_id, project_type_code,
                        phase_code, owner_user_id, repo_url, meta, version
                   FROM project
                  WHERE tenant_id = $1 AND (key, id) > ($2, $3)
                  ORDER BY key, id
                  LIMIT $4",
            )
            .bind(tenant.as_uuid())
            .bind(key)
            .bind(id)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?
        }
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

/// The uppercase key derived from a title: ASCII letters and digits, `-` between runs,
/// at most 32 characters. An empty title still derives a usable key.
pub fn derive_key(title: &str) -> String {
    let mut key = String::with_capacity(title.len());
    let mut last_dash = true;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_uppercase());
            last_dash = false;
        } else if !last_dash {
            key.push('-');
            last_dash = true;
        }
    }
    while key.ends_with('-') {
        key.pop();
    }
    key.truncate(32);
    while key.ends_with('-') {
        key.pop();
    }
    if key.is_empty() {
        "PROJECT".to_owned()
    } else {
        key
    }
}

/// How many collision suffixes a derived key may try before the caller is refused.
pub const MAX_KEY_ATTEMPTS: u32 = 100;

/// `true` when the key is already taken in this tenant.
pub async fn project_key_taken_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM project WHERE tenant_id = $1 AND key = $2)")
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_one(&mut *conn)
        .await
}

/// A free key for `title`: the derived slug, then `-2`, `-3`, … until one is free.
pub async fn derive_project_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    title: &str,
) -> Result<String, ProjectStoreError> {
    let base = derive_key(title);
    for attempt in 1..=MAX_KEY_ATTEMPTS {
        let candidate = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        if !project_key_taken_tx(&mut *conn, tenant, &candidate).await? {
            return Ok(candidate);
        }
    }
    Err(ProjectStoreError::KeyExhausted { base })
}

/// Close a project. The transition is status-guarded, not version-guarded (CONTRACT §3):
/// `None` means the project is absent or already closed/archived.
///
/// The reason, when supplied, is recorded under `meta.close_reason` — `meta` is the one
/// jsonb column for kind-specific fields, and a close reason is one of them.
pub async fn close_project_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    reason: Option<&str>,
) -> Result<Option<Project>, sqlx::Error> {
    let row: Option<ProjectRow> = sqlx::query_as(
        "UPDATE project
            SET status = 'closed',
                meta = CASE
                    WHEN $3::text IS NULL THEN meta
                    ELSE meta || jsonb_build_object('close_reason', $3::text)
                END,
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status NOT IN ('closed', 'archived')
          RETURNING id, key, name, description, status, programme_id, project_type_code,
                    phase_code, owner_user_id, repo_url, meta, version",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(reason)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Into::into))
}

/// A project-page cursor: `base64url("<key>|<uuid>")`, unpadded (CONTRACT §5).
///
/// The pair is the sort key of the last row of the previous page: `key` is unique per
/// tenant and stable, so the cursor never invalidates.
pub fn encode_cursor(key: &str, id: Uuid) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{key}|{id}"))
}

/// Decode a cursor produced by [`encode_cursor`]; anything else is `None`.
pub fn decode_cursor(raw: &str) -> Option<(String, Uuid)> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (key, id) = text.rsplit_once('|')?;
    Some((key.to_owned(), Uuid::parse_str(id).ok()?))
}
