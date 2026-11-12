//! Custom fields and their per-card values (S05 §5, data-arch §2.5).
//!
//! Planka models `group → field → value`; the group is **flattened** into
//! `group_name` + `group_position` (S5-D3) and is never an entity, so nothing here joins a
//! group table.
//!
//! A value is one row per `(card_id, custom_field_id)` — the primary key the upsert leans
//! on. `content` is JSON so one column carries every field kind; the KIND vocabulary
//! (`text|number|date|select|checkbox`) is validated by the domain layer before it reaches
//! this module, and the column's own `CHECK` refuses an unknown kind at the database.

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::CustomFieldRecord;
use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::position::append_after;

/// The five field kinds (data-arch §2.5).
pub const KINDS: [&str; 5] = ["text", "number", "date", "select", "checkbox"];

/// A custom field to create.
#[derive(Clone, Debug)]
pub struct NewCustomField {
    /// The owning board.
    pub board_id: Uuid,
    /// The flattened group label (`''` when ungrouped).
    pub group_name: String,
    /// Order of the group within the board.
    pub group_position: Decimal,
    /// The field's own name, unique per board.
    pub name: String,
    /// One of [`KINDS`].
    pub kind: String,
    /// The `select` options, read whole.
    pub options: Value,
    /// Order within the group.
    pub position: Decimal,
}

/// The fields a `custom_fields.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct CustomFieldUpdate {
    /// The field's own name.
    pub name: Option<String>,
    /// One of [`KINDS`].
    pub kind: Option<String>,
    /// The flattened group label.
    pub group_name: Option<String>,
    /// Order of the group within the board.
    pub group_position: Option<Decimal>,
    /// The `select` options.
    pub options: Option<Value>,
    /// Order within the group.
    pub position: Option<Decimal>,
}

/// One `custom_field_value` row. The primary key is `(card_id, custom_field_id)`; there is
/// no surrogate id, so the pair IS the identity.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CustomFieldValueRecord {
    /// The card the value belongs to.
    pub card_id: Uuid,
    /// The field the value belongs to.
    pub custom_field_id: Uuid,
    /// The value, read whole.
    pub content: Value,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum CustomFieldStoreError {
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const FIELD_COLUMNS: &str = "id, board_id, group_name, group_position, name, kind, options, \
                             position, version, created_at, updated_at";
const VALUE_COLUMNS: &str = "card_id, custom_field_id, content, version, created_at, updated_at";

// ── fields ──────────────────────────────────────────────────────────────────────────────

/// Insert a custom field inside the caller's scoped transaction.
pub async fn insert_custom_field_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewCustomField,
) -> Result<CustomFieldRecord, CustomFieldStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Board, input.board_id).await?;
    let sql = format!(
        "INSERT INTO custom_field
             (tenant_id, board_id, group_name, group_position, name, kind, options, position)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {FIELD_COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.board_id)
        .bind(&input.group_name)
        .bind(input.group_position)
        .bind(&input.name)
        .bind(&input.kind)
        .bind(&input.options)
        .bind(input.position)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one custom field inside the caller's scoped transaction.
pub async fn get_custom_field_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CustomFieldRecord>, sqlx::Error> {
    let sql = format!("SELECT {FIELD_COLUMNS} FROM custom_field WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// The position that appends a field to a board (`max + 1024`).
pub async fn next_custom_field_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Decimal, sqlx::Error> {
    let max: Option<Decimal> = sqlx::query_scalar(
        "SELECT max(position) FROM custom_field WHERE tenant_id = $1 AND board_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(append_after(max))
}

/// Update a custom field, guarded by its version. `None` means absent or stale.
pub async fn update_custom_field_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &CustomFieldUpdate,
    expected_version: Version,
) -> Result<Option<CustomFieldRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE custom_field
            SET name = coalesce($3, name),
                kind = coalesce($4, kind),
                group_name = coalesce($5, group_name),
                group_position = coalesce($6, group_position),
                options = coalesce($7, options),
                position = coalesce($8, position),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $9
          RETURNING {FIELD_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.kind.as_deref())
        .bind(patch.group_name.as_deref())
        .bind(patch.group_position)
        .bind(patch.options.as_ref())
        .bind(patch.position)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await
}

/// Delete a custom field and every value recorded against it.
///
/// The values go first (`custom_field_value.custom_field_id` has no cascade) and the
/// version-guarded delete goes last; a stale version returns `false` and the caller's
/// transaction rolls the value deletes back with it.
pub async fn delete_custom_field_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<bool, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    sqlx::query("DELETE FROM custom_field_value WHERE tenant_id = $1 AND custom_field_id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let rows =
        sqlx::query("DELETE FROM custom_field WHERE tenant_id = $1 AND id = $2 AND version = $3")
            .bind(tenant.as_uuid())
            .bind(id)
            .bind(expected)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    Ok(rows == 1)
}

// ── values ──────────────────────────────────────────────────────────────────────────────

/// Record (or replace) one value: the `(card_id, custom_field_id)` upsert.
pub async fn upsert_value_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    custom_field_id: Uuid,
    content: &Value,
) -> Result<CustomFieldValueRecord, CustomFieldStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Card, card_id).await?;
    ensure_parent_tx(conn, tenant, Parent::CustomField, custom_field_id).await?;
    let sql = format!(
        "INSERT INTO custom_field_value (tenant_id, card_id, custom_field_id, content)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (card_id, custom_field_id) DO UPDATE
             SET content = excluded.content,
                 version = custom_field_value.version + 1,
                 updated_at = now()
         RETURNING {VALUE_COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(custom_field_id)
        .bind(content)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one value inside the caller's scoped transaction.
pub async fn get_value_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    custom_field_id: Uuid,
) -> Result<Option<CustomFieldValueRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {VALUE_COLUMNS} FROM custom_field_value
          WHERE tenant_id = $1 AND card_id = $2 AND custom_field_id = $3"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(custom_field_id)
        .fetch_optional(&mut *conn)
        .await
}

/// Remove one value. `true` when a row existed.
pub async fn clear_value_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    custom_field_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "DELETE FROM custom_field_value
          WHERE tenant_id = $1 AND card_id = $2 AND custom_field_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(custom_field_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// Every value of one card, in field position order.
pub async fn list_values_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Vec<CustomFieldValueRecord>, sqlx::Error> {
    let sql =
        "SELECT v.card_id, v.custom_field_id, v.content, v.version, v.created_at, v.updated_at
           FROM custom_field_value v
           JOIN custom_field f ON f.tenant_id = v.tenant_id AND f.id = v.custom_field_id
          WHERE v.tenant_id = $1 AND v.card_id = $2
          ORDER BY f.group_position, f.position, f.id";
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_all(&mut *conn)
        .await
}

/// Copy every value of one card onto another card (`cards.duplicate`).
pub async fn copy_values_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    from_card: Uuid,
    to_card: Uuid,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "INSERT INTO custom_field_value (tenant_id, card_id, custom_field_id, content)
         SELECT tenant_id, $2, custom_field_id, content
           FROM custom_field_value WHERE tenant_id = $1 AND card_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(to_card)
    .bind(from_card)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows)
}
