//! Task lists and their tasks (S05 §5, data-arch §2.5).
//!
//! A `task_list` is a checklist container on ONE card; a `task` is one item. Both carry
//! `position numeric` and are ordered `(position, id)`, exactly like a lane or a card, so the
//! same exact-midpoint arithmetic applies ([`crate::position::append_after`]).
//!
//! Every parent id (`card_id`, `task_list_id`) is resolved inside the caller's tenant before
//! a write (CONTRACT §2). A delete is two statements — the items first, then the container —
//! because `task.task_list_id` carries no `ON DELETE CASCADE`; the caller's transaction is
//! what makes the pair atomic.

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::position::append_after;

/// A task list to create.
#[derive(Clone, Debug)]
pub struct NewTaskList {
    /// The card the checklist belongs to.
    pub card_id: Uuid,
    /// Display name.
    pub name: String,
    /// Order within the card.
    pub position: Decimal,
}

/// A task to create.
#[derive(Clone, Debug)]
pub struct NewTask {
    /// The checklist the item belongs to.
    pub task_list_id: Uuid,
    /// Display name.
    pub name: String,
    /// Order within the checklist.
    pub position: Decimal,
}

/// One `task_list` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TaskListRecord {
    /// The row id.
    pub id: Uuid,
    /// The card the checklist belongs to.
    pub card_id: Uuid,
    /// Display name.
    pub name: String,
    /// Order within the card.
    pub position: Decimal,
    /// Whether finished items are hidden.
    pub hide_completed: bool,
    /// Whether the checklist is shown on the card front.
    pub show_on_front: bool,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One `task` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TaskRecord {
    /// The row id.
    pub id: Uuid,
    /// The checklist the item belongs to.
    pub task_list_id: Uuid,
    /// Display name.
    pub name: String,
    /// Whether the item is finished.
    pub done: bool,
    /// Order within the checklist.
    pub position: Decimal,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// The fields a `task_lists.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct TaskListUpdate {
    /// Display name.
    pub name: Option<String>,
    /// Order within the card.
    pub position: Option<Decimal>,
    /// Whether finished items are hidden.
    pub hide_completed: Option<bool>,
    /// Whether the checklist is shown on the card front.
    pub show_on_front: Option<bool>,
}

/// The fields a `tasks.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct TaskUpdate {
    /// Display name.
    pub name: Option<String>,
    /// Whether the item is finished.
    pub done: Option<bool>,
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum TaskStoreError {
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const TASK_LIST_COLUMNS: &str = "id, card_id, name, position, hide_completed, show_on_front, \
                                 version, created_at, updated_at";
const TASK_COLUMNS: &str =
    "id, task_list_id, name, done, position, version, created_at, updated_at";

// ── task lists ──────────────────────────────────────────────────────────────────────────

/// Insert a checklist inside the caller's scoped transaction.
pub async fn insert_task_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTaskList,
) -> Result<TaskListRecord, TaskStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Card, input.card_id).await?;
    let sql = format!(
        "INSERT INTO task_list (tenant_id, card_id, name, position)
         VALUES ($1, $2, $3, $4)
         RETURNING {TASK_LIST_COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.card_id)
        .bind(&input.name)
        .bind(input.position)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one checklist inside the caller's scoped transaction.
pub async fn get_task_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<TaskListRecord>, sqlx::Error> {
    let sql = format!("SELECT {TASK_LIST_COLUMNS} FROM task_list WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// Every checklist of a card, in position order.
pub async fn list_task_lists_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Vec<TaskListRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {TASK_LIST_COLUMNS} FROM task_list
          WHERE tenant_id = $1 AND card_id = $2
          ORDER BY position, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_all(&mut *conn)
        .await
}

/// The position that appends a checklist to a card (`max + 1024`).
pub async fn next_task_list_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Decimal, sqlx::Error> {
    let max: Option<Decimal> = sqlx::query_scalar(
        "SELECT max(position) FROM task_list WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(append_after(max))
}

/// The position that places a checklist immediately after `after_id` on its card.
///
/// `Ok(None)` means `after_id` is not a checklist of this card — the caller answers a 404
/// rather than appending, because "after a checklist that does not exist" is a client
/// mistake.
pub async fn position_after_task_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    after_id: Uuid,
) -> Result<Option<Decimal>, sqlx::Error> {
    let Some(anchor): Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM task_list WHERE tenant_id = $1 AND card_id = $2 AND id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let next: Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM task_list
          WHERE tenant_id = $1 AND card_id = $2 AND (position, id) > ($3, $4)
          ORDER BY position, id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(anchor)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Some(crate::position::between(Some(anchor), next)))
}

/// The position that places an item immediately after `after_id` in its checklist.
pub async fn position_after_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    task_list_id: Uuid,
    after_id: Uuid,
) -> Result<Option<Decimal>, sqlx::Error> {
    let Some(anchor): Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM task WHERE tenant_id = $1 AND task_list_id = $2 AND id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(task_list_id)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let next: Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM task
          WHERE tenant_id = $1 AND task_list_id = $2 AND (position, id) > ($3, $4)
          ORDER BY position, id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(task_list_id)
    .bind(anchor)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Some(crate::position::between(Some(anchor), next)))
}

/// Update a checklist, guarded by its version. `None` means absent or stale.
pub async fn update_task_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &TaskListUpdate,
    expected_version: Version,
) -> Result<Option<TaskListRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE task_list
            SET name = coalesce($3, name),
                position = coalesce($4, position),
                hide_completed = coalesce($5, hide_completed),
                show_on_front = coalesce($6, show_on_front),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $7
          RETURNING {TASK_LIST_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.position)
        .bind(patch.hide_completed)
        .bind(patch.show_on_front)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await
}

/// Delete a checklist and its items, guarded by the checklist's version.
///
/// The items go first (`task.task_list_id` has no cascade) and the version-guarded delete
/// goes last; a stale version returns `false` and the caller's transaction rolls the item
/// deletes back with it.
pub async fn delete_task_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<bool, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    sqlx::query("DELETE FROM task WHERE tenant_id = $1 AND task_list_id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let rows =
        sqlx::query("DELETE FROM task_list WHERE tenant_id = $1 AND id = $2 AND version = $3")
            .bind(tenant.as_uuid())
            .bind(id)
            .bind(expected)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    Ok(rows == 1)
}

/// Copy every checklist (and its items) of one card onto another card.
///
/// Used by `cards.duplicate`. New ids are minted for the checklists, and each item is copied
/// onto the checklist the source item belonged to — so the copy's structure, not the source's
/// ids, is what is preserved.
pub async fn copy_task_lists_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    from_card: Uuid,
    to_card: Uuid,
) -> Result<u64, sqlx::Error> {
    let lists = list_task_lists_tx(&mut *conn, tenant, from_card).await?;
    let mut copied = 0u64;
    for list in lists {
        let new_list: Uuid = sqlx::query_scalar(
            "INSERT INTO task_list
                 (tenant_id, card_id, name, position, hide_completed, show_on_front)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(to_card)
        .bind(&list.name)
        .bind(list.position)
        .bind(list.hide_completed)
        .bind(list.show_on_front)
        .fetch_one(&mut *conn)
        .await?;
        let rows = sqlx::query(
            "INSERT INTO task (tenant_id, task_list_id, name, done, position)
             SELECT tenant_id, $2, name, done, position
               FROM task WHERE tenant_id = $1 AND task_list_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(new_list)
        .bind(list.id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
        copied += rows;
    }
    Ok(copied)
}

// ── tasks ───────────────────────────────────────────────────────────────────────────────

/// Insert a task inside the caller's scoped transaction.
pub async fn insert_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTask,
) -> Result<TaskRecord, TaskStoreError> {
    ensure_parent_tx(conn, tenant, Parent::TaskList, input.task_list_id).await?;
    let sql = format!(
        "INSERT INTO task (tenant_id, task_list_id, name, position)
         VALUES ($1, $2, $3, $4)
         RETURNING {TASK_COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.task_list_id)
        .bind(&input.name)
        .bind(input.position)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one task inside the caller's scoped transaction.
pub async fn get_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<TaskRecord>, sqlx::Error> {
    let sql = format!("SELECT {TASK_COLUMNS} FROM task WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// Every item of one checklist, in position order.
pub async fn list_tasks_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    task_list_id: Uuid,
) -> Result<Vec<TaskRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {TASK_COLUMNS} FROM task
          WHERE tenant_id = $1 AND task_list_id = $2
          ORDER BY position, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(task_list_id)
        .fetch_all(&mut *conn)
        .await
}

/// Every item of every checklist on one card, ordered by `(task_list_id, position, id)`.
pub async fn list_tasks_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Vec<TaskRecord>, sqlx::Error> {
    let sql = "SELECT t.id, t.task_list_id, t.name, t.done, t.position, t.version,
                t.created_at, t.updated_at
           FROM task t
           JOIN task_list l ON l.tenant_id = t.tenant_id AND l.id = t.task_list_id
          WHERE t.tenant_id = $1 AND l.card_id = $2
          ORDER BY t.task_list_id, t.position, t.id";
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_all(&mut *conn)
        .await
}

/// The position that appends an item to a checklist (`max + 1024`).
pub async fn next_task_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    task_list_id: Uuid,
) -> Result<Decimal, sqlx::Error> {
    let max: Option<Decimal> = sqlx::query_scalar(
        "SELECT max(position) FROM task WHERE tenant_id = $1 AND task_list_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(task_list_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(append_after(max))
}

/// Update a task, guarded by its version. `None` means absent or stale.
pub async fn update_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &TaskUpdate,
    expected_version: Version,
) -> Result<Option<TaskRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE task
            SET name = coalesce($3, name),
                done = coalesce($4, done),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $5
          RETURNING {TASK_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.done)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await
}

/// Move a task to a checklist and position, guarded by its version.
pub async fn move_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    task_list_id: Uuid,
    position: Decimal,
    expected_version: Version,
) -> Result<Option<TaskRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE task
            SET task_list_id = $3, position = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $5
          RETURNING {TASK_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(task_list_id)
        .bind(position)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await
}

/// Delete a task, guarded by its version. `false` means absent or stale.
pub async fn delete_task_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<bool, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let rows = sqlx::query("DELETE FROM task WHERE tenant_id = $1 AND id = $2 AND version = $3")
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(rows == 1)
}
