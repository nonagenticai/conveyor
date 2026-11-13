//! Board and card-type rows.
//!
//! `card_type` is deliberately not a `catalogue_entry` (data-arch §10 Q1): it carries its own
//! `definition`, and the project-type matrix is keyed by this table's `code`.
//!
//! Inserting a board resolves `project_id` in the caller's tenant first (CONTRACT §2): the
//! FK alone would accept another tenant's project id.

use chrono::{DateTime, Utc};
use conveyor_domain::{Board, CardType, CatalogueKind, Code, CodeError, List, ListKind, TenantId};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::catalogue::{CatalogueStoreError, get_catalogue_entry_tx};
use crate::list::{ListStoreError, NewList, insert_list_tx};
use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::position::APPEND_STEP;
use crate::version_of;

/// A board to create.
#[derive(Clone, Debug)]
pub struct NewBoard {
    /// The owning project.
    pub project_id: Uuid,
    /// Display name; unique per project.
    pub name: String,
    /// Position within the project; `0` is the first board.
    pub position: Decimal,
    /// Display flags and defaults (`default_card_type_code`, `default_view`).
    pub settings: Value,
}

impl NewBoard {
    /// A board with position `0` and empty settings.
    pub fn new(project_id: Uuid, name: impl Into<String>) -> Self {
        Self {
            project_id,
            name: name.into(),
            position: Decimal::ZERO,
            settings: Value::Object(serde_json::Map::new()),
        }
    }
}

/// A card type to create.
#[derive(Clone, Debug)]
pub struct NewCardType {
    /// The stable code the matrix uses.
    pub code: Code,
    /// The display label.
    pub label: String,
    /// Type-specific fields.
    pub definition: Value,
    /// Whether the type may be chosen.
    pub is_active: bool,
}

impl NewCardType {
    /// An active card type with an empty definition.
    pub fn new(code: Code, label: impl Into<String>) -> Self {
        Self {
            code,
            label: label.into(),
            definition: Value::Object(serde_json::Map::new()),
            is_active: true,
        }
    }
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum BoardStoreError {
    /// A stored `card_type.code` no longer parses.
    #[error("a card type code in the database is invalid: {0}")]
    InvalidCode(#[from] CodeError),
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// A board template is absent, inactive, or declares no lane.
    #[error("no active board template names a lane")]
    TemplateMissing,
    /// A board template lists a lane kind the domain does not know.
    #[error("board template lists an unknown lane kind {0:?}")]
    UnknownLane(String),
    /// A lane insert failed.
    #[error(transparent)]
    List(#[from] ListStoreError),
    /// The catalogue read failed.
    #[error(transparent)]
    Catalogue(#[from] CatalogueStoreError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Failures from the membership upsert (`boards.set_member`).
#[derive(Debug, thiserror::Error)]
pub enum BoardMemberStoreError {
    /// A referenced parent (the board, or the global `app_user`) does not exist.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The upsert committed but the row did not read back — only a concurrent delete can
    /// produce this inside one transaction.
    #[error("the membership row did not read back after its upsert")]
    Missing,
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct BoardRow {
    id: Uuid,
    project_id: Uuid,
    name: String,
    version: i64,
}

impl From<BoardRow> for Board {
    fn from(row: BoardRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            name: row.name,
            version: version_of(row.version),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CardTypeRow {
    id: Uuid,
    code: String,
    label: String,
    definition: Value,
    is_active: bool,
    version: i64,
}

impl TryFrom<CardTypeRow> for CardType {
    type Error = CodeError;
    fn try_from(row: CardTypeRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            code: Code::parse(&row.code)?,
            label: row.label,
            definition: row.definition,
            is_active: row.is_active,
            version: version_of(row.version),
        })
    }
}

const CARD_TYPE_COLUMNS: &str = "id, code, label, definition, is_active, version";

/// Insert a board inside the caller's scoped transaction.
pub async fn insert_board_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewBoard,
) -> Result<Board, BoardStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Project, input.project_id).await?;
    let row: BoardRow = sqlx::query_as(
        "INSERT INTO board (tenant_id, project_id, name, position, settings)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, project_id, name, version",
    )
    .bind(tenant.as_uuid())
    .bind(input.project_id)
    .bind(&input.name)
    .bind(input.position)
    .bind(&input.settings)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.into())
}

/// Insert a card type inside the caller's scoped transaction.
pub async fn insert_card_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewCardType,
) -> Result<CardType, BoardStoreError> {
    let sql = format!(
        "INSERT INTO card_type (tenant_id, code, label, definition, is_active)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {CARD_TYPE_COLUMNS}"
    );
    let row: CardTypeRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.code.as_str())
        .bind(&input.label)
        .bind(&input.definition)
        .bind(input.is_active)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.try_into()?)
}

/// Read one card type inside the caller's scoped transaction.
pub async fn get_card_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CardType>, BoardStoreError> {
    let sql = format!("SELECT {CARD_TYPE_COLUMNS} FROM card_type WHERE tenant_id = $1 AND id = $2");
    let row: Option<CardTypeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(CardType::try_from).transpose().map_err(Into::into)
}

/// Read one card type by its code inside the caller's scoped transaction.
pub async fn get_card_type_by_code_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    code: &str,
) -> Result<Option<CardType>, BoardStoreError> {
    let sql =
        format!("SELECT {CARD_TYPE_COLUMNS} FROM card_type WHERE tenant_id = $1 AND code = $2");
    let row: Option<CardTypeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(code)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(CardType::try_from).transpose().map_err(Into::into)
}

/// Every card type in the tenant, in code order.
///
/// One query serves a whole page's `card_type_code`: a list of rows would otherwise resolve a code per
/// row. A tenant's card-type vocabulary is small and bounded (the seed carries 74), so reading it whole
/// is cheaper than the round trips.
pub async fn list_card_types_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<CardType>, BoardStoreError> {
    let sql =
        format!("SELECT {CARD_TYPE_COLUMNS} FROM card_type WHERE tenant_id = $1 ORDER BY code");
    let rows: Vec<CardTypeRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(CardType::try_from(row)?);
    }
    Ok(out)
}

/// Read one board inside the caller's scoped transaction.
pub async fn get_board_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Board>, sqlx::Error> {
    let row: Option<BoardRow> = sqlx::query_as(
        "SELECT id, project_id, name, version FROM board WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Into::into))
}

/// Every board of one project, in position order.
pub async fn list_boards_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<Board>, sqlx::Error> {
    let rows: Vec<BoardRow> = sqlx::query_as(
        "SELECT id, project_id, name, version FROM board
          WHERE tenant_id = $1 AND project_id = $2
          ORDER BY position, id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// The lane kinds a live `board_template` row declares.
///
/// `None` means the template is absent, inactive, or names no lane — the caller refuses rather
/// than inventing lanes (S5-D5). The kind strings are validated by the caller so the HTTP error
/// stays `422 invalid_value` there, not a store error.
pub async fn template_lanes_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    code: &Code,
) -> Result<Option<Vec<String>>, BoardStoreError> {
    let template = get_catalogue_entry_tx(conn, tenant, CatalogueKind::BoardTemplate, code)
        .await?
        .filter(|entry| entry.is_active);
    let Some(entry) = template else {
        return Ok(None);
    };
    let kinds: Vec<String> = entry
        .meta
        .get("lists")
        .and_then(Value::as_array)
        .map(|lists| {
            lists
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok((!kinds.is_empty()).then_some(kinds))
}

/// A board and its lanes, as created (or found) by [`ensure_board_from_template_tx`].
#[derive(Clone, Debug)]
pub struct BoardLanes {
    /// The board row.
    pub board: Board,
    /// Its lanes, in template order.
    pub lists: Vec<List>,
}

/// Ensure a project has the named board and every lane the template declares.
///
/// Idempotent by the schema's own keys: a board is `(tenant, project, name)`-unique, and each
/// lane is found by `(board, kind)` before it is inserted. A partially created board is
/// repaired lane by lane instead of being skipped. The caller owns the template resolution so
/// `projects.create` keeps its exact `409`/`422` contract.
pub async fn ensure_board_from_template_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    name: &str,
    kinds: &[String],
    settings: Value,
) -> Result<BoardLanes, BoardStoreError> {
    let existing = list_boards_tx(conn, tenant, project_id).await?;
    let board = match existing.into_iter().find(|board| board.name == name) {
        Some(board) => board,
        None => {
            insert_board_tx(
                conn,
                tenant,
                &NewBoard {
                    project_id,
                    name: name.to_owned(),
                    position: Decimal::ZERO,
                    settings,
                },
            )
            .await?
        }
    };

    let mut lists = Vec::with_capacity(kinds.len());
    for (index, raw) in kinds.iter().enumerate() {
        let kind = raw
            .parse::<ListKind>()
            .map_err(|_| BoardStoreError::UnknownLane(raw.clone()))?;
        let list = match crate::list::list_of_kind_tx(conn, tenant, board.id, kind).await? {
            Some(record) => record.list,
            None => {
                let position = Decimal::from(APPEND_STEP) * Decimal::from(index as u64 + 1);
                insert_list_tx(
                    conn,
                    tenant,
                    &NewList::new(board.id, title_case(kind.as_str()), position, kind),
                )
                .await?
            }
        };
        lists.push(list);
    }
    Ok(BoardLanes { board, lists })
}

/// The lane display name from its kind (`intake` → `Intake`).
fn title_case(raw: &str) -> String {
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The project type's card-type matrix (`catalogue_entry.meta.matrix`), when the row is
/// live. `None` is a denial at every caller, never a default allow (S3-D3).
pub async fn project_type_matrix_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    code: &str,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT meta -> 'matrix' FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = $2 AND is_active",
    )
    .bind(tenant.as_uuid())
    .bind(code)
    .fetch_optional(&mut *conn)
    .await
}

/// One `label` row as a board read needs it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LabelRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning board.
    pub board_id: Uuid,
    /// Display name; unique per board.
    pub name: String,
    /// Free-text colour token.
    pub color: String,
    /// Order within the board.
    pub position: Decimal,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One `custom_field` row (the group is flattened into two columns, S5-D3).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CustomFieldRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning board.
    pub board_id: Uuid,
    /// The flattened group label (`''` when ungrouped).
    pub group_name: String,
    /// Order of the group within the board.
    pub group_position: Decimal,
    /// The field's own name; unique per board.
    pub name: String,
    /// `text | number | date | select | checkbox`.
    pub kind: String,
    /// The `select` options, read whole.
    pub options: Value,
    /// Order within the group.
    pub position: Decimal,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One `board_membership` row joined with the member's display name.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BoardMemberRecord {
    /// The membership id.
    pub id: Uuid,
    /// The board.
    pub board_id: Uuid,
    /// `app_user.id`.
    pub user_id: Uuid,
    /// The member's display name.
    pub display_name: String,
    /// `owner | admin | member | viewer`.
    pub role: String,
    /// Whether the member may comment.
    pub can_comment: bool,
    /// Whether the member watches the board.
    pub watching: bool,
    /// The row version.
    pub version: i64,
    /// When the membership was created.
    pub created_at: DateTime<Utc>,
}

/// Every label of a board, in position order.
pub async fn list_labels_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Vec<LabelRecord>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, board_id, name, color, position, version, created_at, updated_at
           FROM label WHERE tenant_id = $1 AND board_id = $2
          ORDER BY position, id",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_all(&mut *conn)
    .await
}

/// Every custom field of a board, in group then field order.
pub async fn list_custom_fields_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Vec<CustomFieldRecord>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, board_id, group_name, group_position, name, kind, options, position,
                version, created_at, updated_at
           FROM custom_field WHERE tenant_id = $1 AND board_id = $2
          ORDER BY group_position, position, id",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_all(&mut *conn)
    .await
}

/// Every member of a board, in role then name order.
pub async fn list_board_members_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Vec<BoardMemberRecord>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.id, m.board_id, m.user_id, u.display_name, m.role, m.can_comment, m.watching,
                m.version, m.created_at
           FROM board_membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1 AND m.board_id = $2
          ORDER BY m.role, u.display_name, m.id",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_all(&mut *conn)
    .await
}

/// Read one board membership inside the caller's scoped transaction.
pub async fn get_board_member_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    user_id: Uuid,
) -> Result<Option<BoardMemberRecord>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.id, m.board_id, m.user_id, u.display_name, m.role, m.can_comment, m.watching,
                m.version, m.created_at
           FROM board_membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1 AND m.board_id = $2 AND m.user_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await
}

/// Add a member to a board, or change the one membership that already exists.
///
/// One row per `(board, user)` — the table's unique key is the contract (S05 §5: Planka's
/// `board_membership` + `project_manager` + `board_subscription` fold into this one row).
/// `can_comment`/`watching` present mean "set"; absent means "keep the stored value" on an
/// update and the column default on an insert.
pub async fn upsert_board_member_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    user_id: Uuid,
    role: &str,
    can_comment: Option<bool>,
    watching: Option<bool>,
) -> Result<BoardMemberRecord, BoardMemberStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Board, board_id).await?;
    ensure_parent_tx(conn, tenant, Parent::AppUser, user_id).await?;
    sqlx::query(
        "INSERT INTO board_membership (tenant_id, board_id, user_id, role, can_comment, watching)
         VALUES ($1, $2, $3, $4, coalesce($5, true), coalesce($6, true))
         ON CONFLICT (tenant_id, board_id, user_id) DO UPDATE
             SET role = excluded.role,
                 can_comment = coalesce($5, board_membership.can_comment),
                 watching = coalesce($6, board_membership.watching),
                 version = board_membership.version + 1,
                 updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(user_id)
    .bind(role)
    .bind(can_comment)
    .bind(watching)
    .execute(&mut *conn)
    .await?;
    get_board_member_tx(conn, tenant, board_id, user_id)
        .await?
        .ok_or(BoardMemberStoreError::Missing)
}

/// The project type code of the project a board belongs to.
///
/// `Ok(None)` when the board, the project, or the project's type is absent — every caller
/// treats all three the same way (a restore cannot be legal), so the distinction is not
/// carried.
pub async fn project_type_code_for_board_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let code: Option<Option<String>> = sqlx::query_scalar(
        "SELECT p.project_type_code
           FROM board b
           JOIN project p ON p.tenant_id = b.tenant_id AND p.id = b.project_id
          WHERE b.tenant_id = $1 AND b.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(code.flatten())
}

/// Resolve a tenant member by display name, for `board-export/v1`'s human keys.
///
/// The export format carries references, never uuids (S5-D1); a member is named by the
/// `app_user.display_name` the board read already carries. More than one match is the
/// caller's ambiguity to report — this returns every match rather than guessing.
pub async fn resolve_member_users_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    display_name: &str,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT m.user_id
           FROM membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1 AND u.display_name = $2 AND u.status = 'active'
          ORDER BY m.user_id",
    )
    .bind(tenant.as_uuid())
    .bind(display_name)
    .fetch_all(&mut *conn)
    .await
}
