//! Comments: the ONE comment store (S05 §5, data-arch §2.5).
//!
//! A comment hangs off a card. Every read and write resolves the card inside the caller's
//! tenant first (CONTRACT §2): a single-column FK does not enforce tenancy, so a stolen card
//! id from another tenant must fail here and never reach the database's referential check.
//!
//! `edited_at` is the edit marker: an insert leaves it NULL, an update stamps it. `version`
//! carries the `If-Match` precondition `comments.update`/`comments.delete` need (CONTRACT §3).
//!
//! The page order is `(created_at, id)` ascending, which is exactly
//! `comment (tenant_id, card_id, created_at, id)` — the index the cursor is designed for
//! (data-arch §4). The cursor is opaque (CONTRACT §5): base64url of `"<rfc3339>|<uuid>"`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, SecondsFormat, Utc};
use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};

/// How many comments `CardView` carries. The whole thread is `GET /api/cards/{id}/comments`;
/// a card read model with an unbounded thread would make one card open cost grow with the
/// argument, not with the card.
pub const CARD_VIEW_LIMIT: i64 = 50;

/// A comment to insert.
#[derive(Clone, Debug)]
pub struct NewComment {
    /// The card the comment belongs to.
    pub card_id: Uuid,
    /// The author, when one is known.
    pub author_id: Option<Uuid>,
    /// The body, verbatim.
    pub body: String,
}

/// One `comment` row plus the author's display name.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CommentRecord {
    /// The row id.
    pub id: Uuid,
    /// The card the comment belongs to.
    pub card_id: Uuid,
    /// The author, when one is recorded.
    pub author_id: Option<Uuid>,
    /// The author's display name (`app_user.display_name`), when the row has an author.
    pub author_name: Option<String>,
    /// The body.
    pub body: String,
    /// When the body was last edited; `None` on an unedited comment.
    pub edited_at: Option<DateTime<Utc>>,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum CommentStoreError {
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const COLUMNS: &str = "c.id, c.card_id, c.author_id, u.display_name AS author_name, c.body, \
                       c.edited_at, c.version, c.created_at, c.updated_at";

/// Insert a comment inside the caller's scoped transaction.
pub async fn insert_comment_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewComment,
) -> Result<CommentRecord, CommentStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Card, input.card_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.author_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO comment (tenant_id, card_id, author_id, body)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(input.card_id)
    .bind(input.author_id)
    .bind(&input.body)
    .fetch_one(&mut *conn)
    .await?;
    get_comment_tx(conn, tenant, id)
        .await?
        .ok_or(CommentStoreError::Database(sqlx::Error::RowNotFound))
}

/// Read one comment inside the caller's scoped transaction.
pub async fn get_comment_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CommentRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS}
           FROM comment c
           LEFT JOIN app_user u ON u.id = c.author_id
          WHERE c.tenant_id = $1 AND c.id = $2"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// One keyset page of a card's comments, oldest first.
pub async fn comment_page_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<CommentRecord>, sqlx::Error> {
    let limit = limit.max(1);
    match cursor {
        None => {
            let sql = format!(
                "SELECT {COLUMNS}
                   FROM comment c
                   LEFT JOIN app_user u ON u.id = c.author_id
                  WHERE c.tenant_id = $1 AND c.card_id = $2
                  ORDER BY c.created_at, c.id
                  LIMIT $3"
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(card_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await
        }
        Some((created_at, id)) => {
            let sql = format!(
                "SELECT {COLUMNS}
                   FROM comment c
                   LEFT JOIN app_user u ON u.id = c.author_id
                  WHERE c.tenant_id = $1 AND c.card_id = $2
                    AND (c.created_at, c.id) > ($3, $4)
                  ORDER BY c.created_at, c.id
                  LIMIT $5"
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(card_id)
                .bind(created_at)
                .bind(id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await
        }
    }
}

/// The newest [`CARD_VIEW_LIMIT`] comments of a card, oldest first (the `CardView` shape).
pub async fn card_view_comments_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Vec<CommentRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT * FROM (
             SELECT {COLUMNS}
               FROM comment c
               LEFT JOIN app_user u ON u.id = c.author_id
              WHERE c.tenant_id = $1 AND c.card_id = $2
              ORDER BY c.created_at DESC, c.id DESC
              LIMIT $3
         ) page
         ORDER BY created_at, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(CARD_VIEW_LIMIT)
        .fetch_all(&mut *conn)
        .await
}

/// Replace a comment's body, guarded by its version. `None` means absent or stale.
pub async fn update_comment_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    body: &str,
    expected_version: conveyor_domain::Version,
) -> Result<Option<CommentRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE comment
            SET body = $3, edited_at = now(), version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $4
          RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(body)
    .bind(expected)
    .fetch_optional(&mut *conn)
    .await?;
    match updated {
        None => Ok(None),
        Some(_) => get_comment_tx(conn, tenant, id).await,
    }
}

/// Delete a comment, guarded by its version. `false` means absent or stale.
pub async fn delete_comment_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: conveyor_domain::Version,
) -> Result<bool, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let rows = sqlx::query("DELETE FROM comment WHERE tenant_id = $1 AND id = $2 AND version = $3")
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(rows == 1)
}

/// A comment page cursor: `base64url("<rfc3339-micros>|<uuid>")`, unpadded (CONTRACT §5).
pub fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!(
        "{}|{id}",
        created_at.to_rfc3339_opts(SecondsFormat::Micros, true)
    ))
}

/// Decode a cursor produced by [`encode_cursor`]; anything else is `None`.
pub fn decode_cursor(raw: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(raw).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (created_at, id) = text.split_once('|')?;
    let created_at = DateTime::parse_from_rfc3339(created_at).ok()?;
    let id = Uuid::parse_str(id).ok()?;
    Some((created_at.with_timezone(&Utc), id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_round_trips_with_microsecond_precision() {
        let id = Uuid::now_v7();
        let at = DateTime::parse_from_rfc3339("2026-11-08T07:31:12.123456Z")
            .expect("instant")
            .with_timezone(&Utc);
        let cursor = encode_cursor(at, id);
        assert_eq!(decode_cursor(&cursor), Some((at, id)));
        // The microsecond precision is the ordering authority: a truncated value would
        // round DOWN and replay one comment twice.
        let (decoded, _) = decode_cursor(&cursor).expect("decodes");
        assert_eq!(decoded.timestamp_micros(), at.timestamp_micros());
        assert_eq!(decode_cursor("not-a-cursor"), None);
        assert_eq!(decode_cursor(""), None);
    }

    #[test]
    fn the_card_view_carries_a_bounded_thread() {
        assert_eq!(CARD_VIEW_LIMIT, 50);
    }
}
