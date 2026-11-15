//! `media_collections` and `collection_items`.
//!
//! `item_count` is a denormalised counter the reference maintains in the same transaction as the
//! membership change, and `position` is a per-collection ordinal it recomputes from `MAX(position)`
//! under a `FOR UPDATE` row lock. Both are reproduced here: without the lock two concurrent adds
//! read the same maximum and write duplicate positions, and the lost update on `item_count` is
//! silent.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::{items, timestamp};

/// One `media_collections` row, as the response needs it.
#[derive(Debug, sqlx::FromRow)]
pub struct Row {
    pub id: String,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub collection_type: String,
    pub source_url: Option<String>,
    pub item_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

use super::items::columns as item_columns;

/// A literal, so every statement below is a compile-time `&'static str` (see `items::columns`).
macro_rules! columns {
    () => {
        "id, tenant_id, name, description, collection_type, source_url, item_count,
         created_at, updated_at"
    };
}

impl Row {
    /// `items` is the caller's decision, because the capture distinguishes all three states:
    /// `[]` from a create, `null` from a list or an import, and the members from a get.
    pub fn into_response(
        self,
        items: Option<Vec<crate::media::models::MediaItem>>,
    ) -> crate::media::models::Collection {
        crate::media::models::Collection {
            id: self.id,
            tenant_id: self.tenant_id.to_string(),
            name: self.name,
            description: self.description,
            collection_type: self.collection_type,
            source_url: self.source_url,
            item_count: self.item_count,
            items,
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        }
    }
}

/// Insert an empty collection. The reference validates nothing here: an empty name is accepted and
/// `collection_type` defaults to `playlist` at the schema layer.
pub async fn create(
    pool: &PgPool,
    tenant_id: Uuid,
    name: &str,
    description: Option<&str>,
    collection_type: &str,
) -> Result<Row, sqlx::Error> {
    sqlx::query_as::<_, Row>(concat!(
        "INSERT INTO media_collections
             (id, tenant_id, name, description, collection_type, item_count)
         VALUES ($1, $2, $3, $4, $5, 0)
         RETURNING ",
        columns!()
    ))
    .bind(Uuid::now_v7().to_string())
    .bind(tenant_id)
    .bind(name)
    .bind(description)
    .bind(collection_type)
    .fetch_one(pool)
    .await
}

/// Every collection this tenant owns, newest first.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Row>, sqlx::Error> {
    sqlx::query_as::<_, Row>(concat!(
        "SELECT ",
        columns!(),
        " FROM media_collections WHERE tenant_id = $1 ORDER BY created_at DESC"
    ))
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// One collection with its members.
///
/// The reference's `joinedload` carries no `ORDER BY`, so its member order is whatever the heap
/// returns. This port orders by `position`, which is the order the reference's own `add_items`
/// assigns — deterministic, and the only order a caller could depend on.
pub async fn get(
    pool: &PgPool,
    collection_id: &str,
    tenant_id: Uuid,
) -> Result<Option<(Row, Vec<items::Row>)>, sqlx::Error> {
    let Some(collection) = sqlx::query_as::<_, Row>(concat!(
        "SELECT ",
        columns!(),
        " FROM media_collections WHERE id = $1 AND tenant_id = $2"
    ))
    .bind(collection_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    // The two tables share no column name, so the unprefixed list is unambiguous here.
    let members = sqlx::query_as::<_, items::Row>(concat!(
        "SELECT ",
        item_columns!(),
        " FROM media_items m
           JOIN collection_items ci ON ci.media_item_id = m.id
          WHERE ci.collection_id = $1 AND m.tenant_id = $2
          ORDER BY ci.position"
    ))
    .bind(collection_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(Some((collection, members)))
}

/// Delete a collection. `collection_items` goes with it through the schema's `ON DELETE CASCADE`.
pub async fn delete(
    pool: &PgPool,
    collection_id: &str,
    tenant_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM media_collections WHERE id = $1 AND tenant_id = $2")
        .bind(collection_id)
        .bind(tenant_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

/// Link `item_ids` to a collection, returning how many were actually added.
///
/// An id that does not exist for this tenant is skipped, as is one already linked — including one
/// repeated inside the same request, which would otherwise collide on the
/// `(collection_id, media_item_id)` primary key and fail the whole call.
pub async fn add_items(
    pool: &PgPool,
    collection_id: &str,
    tenant_id: Uuid,
    item_ids: &[String],
) -> Result<i64, sqlx::Error> {
    let mut tx: Transaction<'static, Postgres> = pool.begin().await?;

    let collection = sqlx::query_as::<_, Row>(concat!(
        "SELECT ",
        columns!(),
        " FROM media_collections
          WHERE id = $1 AND tenant_id = $2 FOR UPDATE"
    ))
    .bind(collection_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    // The row lock is held by the SELECT itself; the value is not otherwise needed.
    let Some(_locked) = collection else {
        return Ok(0);
    };

    let max_position: Option<i32> =
        sqlx::query_scalar("SELECT MAX(position) FROM collection_items WHERE collection_id = $1")
            .bind(collection_id)
            .fetch_one(&mut *tx)
            .await?;
    let mut position = max_position.unwrap_or(-1);

    let existing: Vec<String> =
        sqlx::query_scalar("SELECT id FROM media_items WHERE id = ANY($1) AND tenant_id = $2")
            .bind(item_ids)
            .bind(tenant_id)
            .fetch_all(&mut *tx)
            .await?;

    let already: Vec<String> =
        sqlx::query_scalar("SELECT media_item_id FROM collection_items WHERE collection_id = $1 AND media_item_id = ANY($2)")
            .bind(collection_id)
            .bind(item_ids)
            .fetch_all(&mut *tx)
            .await?;

    let mut added = 0i64;
    let mut linked: Vec<&String> = Vec::new();
    for item_id in item_ids {
        if !existing.contains(item_id) || already.contains(item_id) || linked.contains(&item_id) {
            continue;
        }
        linked.push(item_id);
        position += 1;
        sqlx::query(
            "INSERT INTO collection_items (collection_id, media_item_id, position)
             VALUES ($1, $2, $3)",
        )
        .bind(collection_id)
        .bind(item_id)
        .bind(position)
        .execute(&mut *tx)
        .await?;
        added += 1;
    }

    if added > 0 {
        sqlx::query("UPDATE media_collections SET item_count = item_count + $1 WHERE id = $2")
            .bind(added as i32)
            .bind(collection_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(added)
}

/// Unlink one item, decrementing `item_count` and never letting it go below zero.
pub async fn remove_item(
    pool: &PgPool,
    collection_id: &str,
    item_id: &str,
    tenant_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx: Transaction<'static, Postgres> = pool.begin().await?;

    let owned: Option<String> = sqlx::query_scalar(
        "SELECT id FROM media_collections WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(collection_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    if owned.is_none() {
        return Ok(false);
    }

    let affected =
        sqlx::query("DELETE FROM collection_items WHERE collection_id = $1 AND media_item_id = $2")
            .bind(collection_id)
            .bind(item_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if affected == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE media_collections SET item_count = GREATEST(item_count - 1, 0) WHERE id = $1",
    )
    .bind(collection_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}
