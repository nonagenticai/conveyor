//! The portfolio surface that survives: `project_stakeholder` and the S03 `programme` rows the
//! `programmes.*` actions own (S08 Part A, data-arch §2.9).
//!
//! ⚠️ 2026-11-08: `project_capability`, `deliverable` and `register_item` lived here until migration 0011
//! dropped them — the S00 usage inventory measured all three at zero. Their store families, their catalogue
//! resolution and their `UnknownKind`/`UnknownCode` refusals went with the tables.
//!
//! Every query here is a `_tx` function: it takes an open `&mut PgConnection` the caller has
//! already scoped with [`crate::begin_scoped`], plus the [`TenantId`] it binds explicitly, so
//! the write and its `portfolio.changed` outbox row commit together (CONTRACT §7). **This
//! module writes no event** — the action layer emits `portfolio.changed` in that same
//! transaction, through `conveyor_actions::portfolio::emit_portfolio_event`, so a caller
//! still cannot separate a row from its announcement.
//!
//! **Every `project_id` is resolved in the caller's tenant before a write.** A single-column
//! FK does not enforce tenancy — referential-integrity checks bypass RLS, so a `tenant_id=A`
//! row can name tenant B's project and the FK accepts it (CONTRACT §2). [`crate::parent`] is
//! the one resolver; a miss is [`PortfolioStoreError::Parent`], never a row.
//!
//! **`meta` is read and written whole.** Nothing filters on a jsonb path, so no index is
//! implied and no caller can build a query the table cannot serve.
//!
//! **This module writes no event; the action layer emits.** Every create, update and delete
//! here returns the row it wrote and announces nothing. The one `portfolio.changed` row per
//! mutation is appended by `conveyor_actions::portfolio::emit_portfolio_event` in the same
//! transaction, so one pattern holds across the workspace: actions emit, the store never does
//! (S08 §5.11, CONTRACT §7). The payload is `{entity, id, op}`; the topic is
//! `<aggregate>:<id>`; the event type is [`PORTFOLIO_CHANGED`].
//!
//! The version is optimistic-concurrency, exactly as every other mutable aggregate in this
//! workspace: every `update_*_tx` and `delete_*_tx` takes the version the caller read and a
//! mismatch writes nothing (CONTRACT §3).

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};
use crate::project::{decode_cursor, encode_cursor};
use crate::version_of;

// 2026-11-08: `REGISTER_KINDS`, `is_register_kind`, `DELIVERABLE_KINDS` and `CAPABILITY_KINDS` lived here.
// They described the three families that migration 0011 dropped (S00 day-2 verdicts), so they went with the
// tables; a vocabulary for a table that does not exist is a second source of truth.

/// The one event type every portfolio mutation emits (S08 §5.11).
pub const PORTFOLIO_CHANGED: &str = "portfolio.changed";

/// Why a portfolio read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum PortfolioStoreError {
    /// No such row in this tenant.
    #[error("the portfolio row does not exist in this tenant")]
    NotFound,
    /// The row moved; the caller's `If-Match` precondition is stale.
    #[error("the row is at version {current}, not the expected version")]
    VersionMismatch {
        /// The version the row holds now.
        current: Version,
    },
    /// The page cursor is not one this module issued.
    #[error("the page cursor is malformed")]
    BadCursor,
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

// ── pagination ──────────────────────────────────────────────────────────────────────────

/// A portfolio page request: an opaque `(updated_at, id)` cursor and a limit.
///
/// `limit` is clamped by the caller (the action layer uses [`crate::actions::clamp_limit`]);
/// `cursor` is the opaque value the previous page returned and is never interpreted by a
/// client (CONTRACT §5).
#[derive(Clone, Debug, Default)]
pub struct PortfolioPage {
    /// The previous page's cursor, or `None` for the first page.
    pub cursor: Option<String>,
    /// The page size.
    pub limit: i64,
}

impl PortfolioPage {
    /// A first page of `limit` rows with no cursor.
    pub fn new(limit: i64) -> Self {
        Self {
            cursor: None,
            limit,
        }
    }
}

/// Decode a `(updated_at, id)` cursor: base64url of `"<rfc3339>|<uuid>"`.
///
/// This is [`crate::project::encode_cursor`]'s pair, read by the same decoder the governed
/// assets use; a malformed value is a refusal, never a silent first page.
fn decode_time_cursor(raw: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let (at, id) = decode_cursor(raw)?;
    Some((
        DateTime::parse_from_rfc3339(&at).ok()?.with_timezone(&Utc),
        id,
    ))
}

/// The bindings a keyset list needs: the cursor's two halves and `limit + 1`.
///
/// One row more than the caller asked for is what lets a full page say whether another one
/// exists without a second query.
struct PageBindings {
    /// The cursor's `updated_at`, or `None` for the first page.
    at: Option<DateTime<Utc>>,
    /// The cursor's `id`, or `None` for the first page.
    after_id: Option<Uuid>,
    /// `limit + 1`.
    fetch: i64,
}

/// Decode the page's cursor and compute the fetch size.
fn page_bindings(page: &PortfolioPage) -> Result<PageBindings, PortfolioStoreError> {
    let limit = page.limit.max(1);
    let cursor = match page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(decode_time_cursor(raw).ok_or(PortfolioStoreError::BadCursor)?),
    };
    Ok(PageBindings {
        at: cursor.map(|(at, _)| at),
        after_id: cursor.map(|(_, id)| id),
        fetch: limit.saturating_add(1),
    })
}

/// Split `limit + 1` rows into the page and its `next_cursor`.
fn page_split<Row>(
    mut rows: Vec<Row>,
    limit: i64,
    sort_key: impl Fn(&Row) -> (DateTime<Utc>, Uuid),
) -> (Vec<Row>, Option<String>) {
    if rows.len() as i64 > limit {
        rows.pop();
        let next = rows.last().map(|row| {
            let (at, id) = sort_key(row);
            encode_cursor(&at.to_rfc3339(), id)
        });
        (rows, next)
    } else {
        (rows, None)
    }
}

fn version_i64(version: Version) -> i64 {
    i64::try_from(version.get()).unwrap_or(i64::MAX)
}

// ── programme (S03's table, the `programmes.*` actions) ─────────────────────────────────

/// A programme as every `programmes.*` action returns it.
///
/// S03's [`conveyor_domain::Programme`] is the slim aggregate; this is the full row, because
/// the action representation carries the timestamps and the `ETag`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgrammeRecord {
    /// The row id.
    pub id: Uuid,
    /// Display name; unique per tenant.
    pub name: String,
    /// Free-text status (`active` unless the caller said otherwise).
    pub status: String,
    /// The accountable user.
    pub owner_user_id: Option<Uuid>,
    /// The optimistic-concurrency version (`ETag`).
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A programme to create.
#[derive(Clone, Debug)]
pub struct NewProgrammeRow {
    /// Display name; unique per tenant.
    pub name: String,
    /// Free-text status; `active` when absent.
    pub status: Option<String>,
    /// The accountable user.
    pub owner_user_id: Option<Uuid>,
}

/// The editable fields of a programme. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct ProgrammePatch {
    /// The new display name.
    pub name: Option<String>,
    /// The new status.
    pub status: Option<String>,
    /// The new accountable user.
    pub owner_user_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProgrammeRow {
    id: Uuid,
    name: String,
    status: String,
    owner_user_id: Option<Uuid>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ProgrammeRow> for ProgrammeRecord {
    fn from(row: ProgrammeRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            status: row.status,
            owner_user_id: row.owner_user_id,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const PROGRAMME_COLUMNS: &str = "id, name, status, owner_user_id, version, created_at, updated_at";

/// Insert a programme inside the caller's transaction.
pub async fn create_programme_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewProgrammeRow,
) -> Result<ProgrammeRecord, PortfolioStoreError> {
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::AppUser, input.owner_user_id).await?;
    let sql = format!(
        "INSERT INTO programme (tenant_id, name, status, owner_user_id)
         VALUES ($1, $2, coalesce($3, 'active'), $4)
         RETURNING {PROGRAMME_COLUMNS}"
    );
    let row: ProgrammeRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.name)
        .bind(&input.status)
        .bind(input.owner_user_id)
        .fetch_one(&mut *conn)
        .await?;
    let record: ProgrammeRecord = row.into();
    Ok(record)
}

/// Read one programme inside the caller's tenant; another tenant's id is `None`.
pub async fn get_programme_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProgrammeRecord>, PortfolioStoreError> {
    let sql = format!("SELECT {PROGRAMME_COLUMNS} FROM programme WHERE tenant_id = $1 AND id = $2");
    let row: Option<ProgrammeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_programme_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &ProgrammePatch,
) -> Result<ProgrammeRecord, PortfolioStoreError> {
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::AppUser, patch.owner_user_id).await?;
    let sql = format!(
        "SELECT {PROGRAMME_COLUMNS} FROM programme WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ProgrammeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let current: ProgrammeRecord = row.ok_or(PortfolioStoreError::NotFound)?.into();
    if current.version != expected_version {
        return Err(PortfolioStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let sql = format!(
        "UPDATE programme
            SET name = COALESCE($3, name),
                status = COALESCE($4, status),
                owner_user_id = COALESCE($5, owner_user_id),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {PROGRAMME_COLUMNS}"
    );
    let row: Option<ProgrammeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.status.as_deref())
        .bind(patch.owner_user_id)
        .fetch_optional(&mut *conn)
        .await?;
    let record: ProgrammeRecord = row.ok_or(PortfolioStoreError::NotFound)?.into();
    Ok(record)
}

/// One keyset page of programmes, newest `updated_at` first.
pub async fn list_programmes_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    page: &PortfolioPage,
) -> Result<(Vec<ProgrammeRecord>, Option<String>), PortfolioStoreError> {
    let limit = page.limit.max(1);
    let bindings = page_bindings(page)?;
    let sql = format!(
        "SELECT {PROGRAMME_COLUMNS} FROM programme
          WHERE tenant_id = $1
            AND ($2::timestamptz IS NULL OR (updated_at, id) < ($2::timestamptz, $3::uuid))
          ORDER BY updated_at DESC, id DESC
          LIMIT $4"
    );
    let rows: Vec<ProgrammeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(bindings.at)
        .bind(bindings.after_id)
        .bind(bindings.fetch)
        .fetch_all(&mut *conn)
        .await?;
    let (rows, next) = page_split(rows, limit, |row| (row.updated_at, row.id));
    Ok((rows.into_iter().map(Into::into).collect(), next))
}

// ── project_stakeholder ─────────────────────────────────────────────────────────────────

/// A stakeholder as `stakeholders.*` returns it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectStakeholderRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning project.
    pub project_id: Uuid,
    /// The linked `app_user`, when the stakeholder has an account.
    pub user_id: Option<Uuid>,
    /// The display name (empty is legal: a stakeholder may be recorded before they are known).
    pub name: String,
    /// The contact email.
    pub email: Option<String>,
    /// Their role on the project.
    pub role: String,
    /// The optimistic-concurrency version (`ETag`).
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A stakeholder to create.
#[derive(Clone, Debug)]
pub struct NewStakeholder {
    /// The owning project, resolved in the caller's tenant.
    pub project_id: Uuid,
    /// The linked `app_user`.
    pub user_id: Option<Uuid>,
    /// The display name.
    pub name: String,
    /// The contact email.
    pub email: Option<String>,
    /// Their role on the project.
    pub role: String,
}

/// The editable fields of a stakeholder. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct StakeholderPatch {
    /// The new linked user.
    pub user_id: Option<Uuid>,
    /// The new display name.
    pub name: Option<String>,
    /// The new contact email.
    pub email: Option<String>,
    /// The new role.
    pub role: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct StakeholderRow {
    id: Uuid,
    project_id: Uuid,
    user_id: Option<Uuid>,
    name: String,
    email: Option<String>,
    role: String,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<StakeholderRow> for ProjectStakeholderRecord {
    fn from(row: StakeholderRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            user_id: row.user_id,
            name: row.name,
            email: row.email,
            role: row.role,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const STAKEHOLDER_COLUMNS: &str = "id, project_id, user_id, name, email, role, version, \
                                   created_at, updated_at";

/// Insert a stakeholder, resolving the project and the linked user in the caller's tenant.
pub async fn create_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewStakeholder,
) -> Result<ProjectStakeholderRecord, PortfolioStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Project, input.project_id).await?;
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::AppUser, input.user_id).await?;
    let sql = format!(
        "INSERT INTO project_stakeholder (tenant_id, project_id, user_id, name, email, role)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {STAKEHOLDER_COLUMNS}"
    );
    let row: StakeholderRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.project_id)
        .bind(input.user_id)
        .bind(&input.name)
        .bind(&input.email)
        .bind(&input.role)
        .fetch_one(&mut *conn)
        .await?;
    let record: ProjectStakeholderRecord = row.into();
    Ok(record)
}

/// Read one stakeholder inside the caller's tenant; another tenant's id is `None`.
pub async fn get_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProjectStakeholderRecord>, PortfolioStoreError> {
    let sql = format!(
        "SELECT {STAKEHOLDER_COLUMNS} FROM project_stakeholder WHERE tenant_id = $1 AND id = $2"
    );
    let row: Option<StakeholderRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &StakeholderPatch,
) -> Result<ProjectStakeholderRecord, PortfolioStoreError> {
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::AppUser, patch.user_id).await?;
    let sql = format!(
        "SELECT {STAKEHOLDER_COLUMNS} FROM project_stakeholder
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<StakeholderRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let current: ProjectStakeholderRecord = row.ok_or(PortfolioStoreError::NotFound)?.into();
    if current.version != expected_version {
        return Err(PortfolioStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let sql = format!(
        "UPDATE project_stakeholder
            SET user_id = COALESCE($3, user_id),
                name = COALESCE($4, name),
                email = COALESCE($5, email),
                role = COALESCE($6, role),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {STAKEHOLDER_COLUMNS}"
    );
    let row: Option<StakeholderRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.user_id)
        .bind(patch.name.as_deref())
        .bind(patch.email.as_deref())
        .bind(patch.role.as_deref())
        .fetch_optional(&mut *conn)
        .await?;
    let record: ProjectStakeholderRecord = row.ok_or(PortfolioStoreError::NotFound)?.into();
    Ok(record)
}

/// Delete one stakeholder, returning the row as it was.
///
/// The parent is resolved first (CONTRACT §2 — a delete is not an exemption), then the row is
/// removed with the caller's version in the predicate, so a concurrent edit is a
/// [`PortfolioStoreError::VersionMismatch`] rather than a lost update.
pub async fn delete_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<ProjectStakeholderRecord, PortfolioStoreError> {
    let prior = get_stakeholder_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(PortfolioStoreError::NotFound)?;
    ensure_parent_tx(&mut *conn, tenant, Parent::Project, prior.project_id).await?;
    if prior.version != expected_version {
        return Err(PortfolioStoreError::VersionMismatch {
            current: prior.version,
        });
    }
    let sql = format!(
        "DELETE FROM project_stakeholder
          WHERE tenant_id = $1 AND id = $2 AND version = $3
         RETURNING {STAKEHOLDER_COLUMNS}"
    );
    let row: Option<StakeholderRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(version_i64(expected_version))
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => {
            let record: ProjectStakeholderRecord = row.into();
            Ok(record)
        }
        None => Err(stale_or_missing(
            get_stakeholder_tx(&mut *conn, tenant, id).await?,
        )),
    }
}

/// The filters `stakeholders.list` accepts.
#[derive(Clone, Debug, Default)]
pub struct StakeholderFilter {
    /// Only stakeholders of this project.
    pub project_id: Option<Uuid>,
}

/// One keyset page of stakeholders, newest `updated_at` first.
pub async fn list_stakeholders_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &StakeholderFilter,
    page: &PortfolioPage,
) -> Result<(Vec<ProjectStakeholderRecord>, Option<String>), PortfolioStoreError> {
    let limit = page.limit.max(1);
    let bindings = page_bindings(page)?;
    let sql = format!(
        "SELECT {STAKEHOLDER_COLUMNS} FROM project_stakeholder
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR project_id = $2)
            AND ($3::timestamptz IS NULL OR (updated_at, id) < ($3::timestamptz, $4::uuid))
          ORDER BY updated_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<StakeholderRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(filter.project_id)
        .bind(bindings.at)
        .bind(bindings.after_id)
        .bind(bindings.fetch)
        .fetch_all(&mut *conn)
        .await?;
    let (rows, next) = page_split(rows, limit, |row| (row.updated_at, row.id));
    Ok((rows.into_iter().map(Into::into).collect(), next))
}

/// `(current)` when a concurrent change beat the caller's delete, `NotFound` when the row is
/// gone. One expression, used by every family's delete so the two outcomes cannot diverge.
fn stale_or_missing<Record>(current: Option<Record>) -> PortfolioStoreError
where
    Record: HasVersion,
{
    match current {
        Some(record) => PortfolioStoreError::VersionMismatch {
            current: record.version(),
        },
        None => PortfolioStoreError::NotFound,
    }
}

/// The version accessor `stale_or_missing` needs; implemented by every record above.
pub trait HasVersion {
    /// The row's optimistic-concurrency version.
    fn version(&self) -> Version;
}

macro_rules! has_version {
    ($($record:ty),+ $(,)?) => {
        $(
            impl HasVersion for $record {
                fn version(&self) -> Version {
                    self.version
                }
            }
        )+
    };
}

has_version!(ProgrammeRecord, ProjectStakeholderRecord,);
