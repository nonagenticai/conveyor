//! Card lanes (`list`). `kind` is the only lifecycle vocabulary in the system.
//!
//! Inserting a lane resolves `board_id` in the caller's tenant first (CONTRACT §2): the FK
//! alone would accept another tenant's board id.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::{List, ListKind, TenantId, Version};
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::version_of;

/// A list to create.
#[derive(Clone, Debug)]
pub struct NewList {
    /// The owning board.
    pub board_id: Uuid,
    /// Display name.
    pub name: String,
    /// Position within the board.
    pub position: Decimal,
    /// The lane's lifecycle meaning.
    pub kind: ListKind,
    /// Optional colour token.
    pub color: Option<String>,
    /// A soft cap; the instance of the `wip_limit` catalogue template.
    pub wip_limit: Option<i32>,
}

impl NewList {
    /// A list at the given position with no colour and no WIP limit.
    pub fn new(board_id: Uuid, name: impl Into<String>, position: Decimal, kind: ListKind) -> Self {
        Self {
            board_id,
            name: name.into(),
            position,
            kind,
            color: None,
            wip_limit: None,
        }
    }
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum ListStoreError {
    /// A stored `list.kind` is not one of the five kinds.
    #[error("the database holds list.kind = {0:?}, which the domain does not know")]
    UnknownKind(String),
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct ListRow {
    id: Uuid,
    board_id: Uuid,
    name: String,
    kind: String,
    wip_limit: Option<i32>,
    version: i64,
}

impl TryFrom<ListRow> for List {
    type Error = ListStoreError;
    fn try_from(row: ListRow) -> Result<Self, Self::Error> {
        let kind =
            ListKind::from_str(&row.kind).map_err(|_| ListStoreError::UnknownKind(row.kind))?;
        Ok(Self {
            id: row.id,
            board_id: row.board_id,
            name: row.name,
            kind,
            wip_limit: row.wip_limit,
            version: version_of(row.version),
        })
    }
}

const COLUMNS: &str = "id, board_id, name, kind, wip_limit, version";

/// Insert a list inside the caller's scoped transaction.
pub async fn insert_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewList,
) -> Result<List, ListStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Board, input.board_id).await?;
    let sql = format!(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, color, wip_limit)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {COLUMNS}"
    );
    let row: ListRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.board_id)
        .bind(&input.name)
        .bind(input.position)
        .bind(input.kind.as_str())
        .bind(&input.color)
        .bind(input.wip_limit)
        .fetch_one(&mut *conn)
        .await?;
    row.try_into()
}

/// Read one list inside the caller's scoped transaction.
pub async fn get_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<List>, ListStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM list WHERE tenant_id = $1 AND id = $2");
    let row: Option<ListRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(List::try_from).transpose()
}

/// One `list` row with the columns the domain value does not carry (`position`, `color` and
/// the timestamps a read projection returns).
#[derive(Clone, Debug)]
pub struct ListRecord {
    /// The domain value (`id`, `board_id`, `name`, `kind`, `wip_limit`, `version`).
    pub list: List,
    /// Order within the board; exact decimal.
    pub position: Decimal,
    /// Optional colour token.
    pub color: Option<String>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ListRecordRow {
    id: Uuid,
    board_id: Uuid,
    name: String,
    kind: String,
    position: Decimal,
    color: Option<String>,
    wip_limit: Option<i32>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ListRecordRow> for ListRecord {
    type Error = ListStoreError;
    fn try_from(row: ListRecordRow) -> Result<Self, Self::Error> {
        let kind =
            ListKind::from_str(&row.kind).map_err(|_| ListStoreError::UnknownKind(row.kind))?;
        Ok(Self {
            list: List {
                id: row.id,
                board_id: row.board_id,
                name: row.name,
                kind,
                wip_limit: row.wip_limit,
                version: version_of(row.version),
            },
            position: row.position,
            color: row.color,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

const RECORD_COLUMNS: &str = "id, board_id, name, kind, position, color, wip_limit, version, \
                              created_at, updated_at";

/// Read one list as a record inside the caller's scoped transaction.
pub async fn get_list_record_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ListRecord>, ListStoreError> {
    let sql = format!("SELECT {RECORD_COLUMNS} FROM list WHERE tenant_id = $1 AND id = $2");
    let row: Option<ListRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(ListRecord::try_from).transpose()
}

/// Every lane of a board in position order.
pub async fn list_records_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Vec<ListRecord>, ListStoreError> {
    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM list
          WHERE tenant_id = $1 AND board_id = $2
          ORDER BY position, id"
    );
    let rows: Vec<ListRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(board_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(ListRecord::try_from).collect()
}

/// Every lane of a board with its live-card count, in position order.
pub async fn list_records_with_counts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Vec<(ListRecord, i64)>, ListStoreError> {
    #[derive(Debug, sqlx::FromRow)]
    struct CountedRow {
        id: Uuid,
        board_id: Uuid,
        name: String,
        kind: String,
        position: Decimal,
        color: Option<String>,
        wip_limit: Option<i32>,
        version: i64,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        card_count: i64,
    }
    let rows: Vec<CountedRow> = sqlx::query_as(
        "SELECT l.id, l.board_id, l.name, l.kind, l.position, l.color, l.wip_limit,
                l.version, l.created_at, l.updated_at,
                count(c.id) FILTER (WHERE c.archived_at IS NULL) AS card_count
           FROM list l
           LEFT JOIN card c ON c.tenant_id = l.tenant_id AND c.list_id = l.id
          WHERE l.tenant_id = $1 AND l.board_id = $2
          GROUP BY l.id
          ORDER BY l.position, l.id",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let record = ListRecord::try_from(ListRecordRow {
            id: row.id,
            board_id: row.board_id,
            name: row.name,
            kind: row.kind,
            position: row.position,
            color: row.color,
            wip_limit: row.wip_limit,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })?;
        out.push((record, row.card_count));
    }
    Ok(out)
}

/// The first lane of `kind` on a board (position order), if one exists.
///
/// `archive` and `intake` are addressed by kind, never by a caller-supplied list id: a
/// caller that could name the archive lane could archive into a live lane, and the stamp
/// invariant `archived_at ⇒ list.kind = 'archive'` would break.
pub async fn list_of_kind_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    kind: ListKind,
) -> Result<Option<ListRecord>, ListStoreError> {
    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM list
          WHERE tenant_id = $1 AND board_id = $2 AND kind = $3
          ORDER BY position, id
          LIMIT 1"
    );
    let row: Option<ListRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(board_id)
        .bind(kind.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    row.map(ListRecord::try_from).transpose()
}

/// How many stamped cards sit in a lane.
///
/// Used before a `kind` edit: a lane that holds stamped cards can only stay an archive lane,
/// because `archived_at ⇒ list.kind = 'archive'` is the one invariant the schema leans on
/// (S3-D2). Zero means the edit cannot strand a card.
pub async fn stamped_card_count_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    list_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM card
          WHERE tenant_id = $1 AND list_id = $2 AND archived_at IS NOT NULL",
    )
    .bind(tenant.as_uuid())
    .bind(list_id)
    .fetch_one(&mut *conn)
    .await
}

/// The fields a `lists.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct ListUpdate {
    /// Display name.
    pub name: Option<String>,
    /// The lane's lifecycle meaning.
    pub kind: Option<ListKind>,
    /// Colour token.
    pub color: Option<String>,
    /// Soft WIP cap; must be > 0 when present.
    pub wip_limit: Option<i32>,
}

/// Update a lane, guarded by its version.
///
/// `None` means the row is absent **or** the version is stale; the caller re-reads once to
/// tell 404 from 412 (the same contract `move_card_tx` uses).
pub async fn update_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &ListUpdate,
    expected_version: Version,
) -> Result<Option<ListRecord>, ListStoreError> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE list
            SET name = coalesce($3, name),
                kind = coalesce($4, kind),
                color = coalesce($5, color),
                wip_limit = coalesce($6, wip_limit),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $7
          RETURNING {RECORD_COLUMNS}"
    );
    let row: Option<ListRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.kind.map(ListKind::as_str))
        .bind(patch.color.as_deref())
        .bind(patch.wip_limit)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(ListRecord::try_from).transpose()
}

/// Move a lane to `position`, guarded by its version.
pub async fn move_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    position: Decimal,
    expected_version: Version,
) -> Result<Option<ListRecord>, ListStoreError> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE list
            SET position = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $4
          RETURNING {RECORD_COLUMNS}"
    );
    let row: Option<ListRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(position)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(ListRecord::try_from).transpose()
}
