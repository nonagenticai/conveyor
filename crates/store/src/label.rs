//! Labels and the `card_label` join (S05 §5, data-arch §2.5).
//!
//! A label belongs to ONE board and `(tenant_id, board_id, name)` is unique; a card is
//! labelled through `card_label`, a fact that is inserted and deleted, never edited. Adding a
//! label therefore never rewrites a card row — but it does change the card's REPRESENTATION
//! (`CardView` carries its labels), so the action layer bumps the card's `version` through
//! [`crate::card::bump_card_version_tx`] and answers with the new `ETag`.
//!
//! Colour is free text (S05 §10 Q2 default); the named palette is vocabulary, not a schema.
//! Positions are the same exact `(max + 1024)` append as every other ordered table.

use conveyor_domain::{TenantId, Version};
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::LabelRecord;
use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::position::append_after;

/// A label to create.
#[derive(Clone, Debug)]
pub struct NewLabel {
    /// The owning board.
    pub board_id: Uuid,
    /// Display name; unique per board.
    pub name: String,
    /// Free-text colour token.
    pub color: String,
    /// Order within the board.
    pub position: Decimal,
}

/// The fields a `labels.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct LabelUpdate {
    /// Display name.
    pub name: Option<String>,
    /// Free-text colour token.
    pub color: Option<String>,
    /// Order within the board.
    pub position: Option<Decimal>,
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum LabelStoreError {
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The projection [`LabelRecord`] is decoded from.
const COLUMNS: &str = "id, board_id, name, color, position, version, created_at, updated_at";

/// Insert a label inside the caller's scoped transaction.
pub async fn insert_label_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewLabel,
) -> Result<LabelRecord, LabelStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Board, input.board_id).await?;
    let sql = format!(
        "INSERT INTO label (tenant_id, board_id, name, color, position)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.board_id)
        .bind(&input.name)
        .bind(&input.color)
        .bind(input.position)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one label inside the caller's scoped transaction.
pub async fn get_label_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<LabelRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM label WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// The position that appends a label to a board (`max + 1024`).
pub async fn next_label_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Decimal, sqlx::Error> {
    let max: Option<Decimal> = sqlx::query_scalar(
        "SELECT max(position) FROM label WHERE tenant_id = $1 AND board_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(append_after(max))
}

/// Update a label, guarded by its version. `None` means absent or stale.
pub async fn update_label_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &LabelUpdate,
    expected_version: Version,
) -> Result<Option<LabelRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE label
            SET name = coalesce($3, name),
                color = coalesce($4, color),
                position = coalesce($5, position),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $6
          RETURNING {COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.color.as_deref())
        .bind(patch.position)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await
}

/// Delete a label and every `card_label` row that names it.
///
/// The join rows go first (`card_label.label_id` has no cascade) and the version-guarded
/// delete goes last; a stale version returns `false` and the caller's transaction rolls the
/// join deletes back with it.
pub async fn delete_label_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<bool, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    sqlx::query("DELETE FROM card_label WHERE tenant_id = $1 AND label_id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let rows = sqlx::query("DELETE FROM label WHERE tenant_id = $1 AND id = $2 AND version = $3")
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(rows == 1)
}

/// Label a card. `true` when the pair was not already present (the insert happened).
pub async fn add_card_label_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    label_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "INSERT INTO card_label (tenant_id, card_id, label_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (card_id, label_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(label_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// Unlabel a card. `false` means the card did not carry the label.
pub async fn remove_card_label_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    label_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "DELETE FROM card_label WHERE tenant_id = $1 AND card_id = $2 AND label_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(label_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// Every label a card carries, in board position order.
pub async fn list_card_labels_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Vec<LabelRecord>, sqlx::Error> {
    let sql = "SELECT l.id, l.board_id, l.name, l.color, l.position, l.version,
                l.created_at, l.updated_at
           FROM label l
           JOIN card_label cl ON cl.tenant_id = l.tenant_id AND cl.label_id = l.id
          WHERE l.tenant_id = $1 AND cl.card_id = $2
          ORDER BY l.position, l.id";
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_all(&mut *conn)
        .await
}

/// Copy every label of one card onto another card (`cards.duplicate`).
pub async fn copy_card_labels_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    from_card: Uuid,
    to_card: Uuid,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "INSERT INTO card_label (tenant_id, card_id, label_id)
         SELECT tenant_id, $2, label_id
           FROM card_label WHERE tenant_id = $1 AND card_id = $3
         ON CONFLICT (card_id, label_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(to_card)
    .bind(from_card)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows)
}
