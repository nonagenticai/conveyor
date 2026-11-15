//! `search_sessions` and `session_items` — what `GET /api/score/report/{session_id}` reads.

use sqlx::PgPool;
use uuid::Uuid;

use super::items;
use super::items::columns as item_columns;

/// The session's items, in a stable order.
///
/// The reference walks `session_obj.items` with no `ORDER BY`, so its order is whatever the
/// relationship load returns. Ordering by `media_item_id` is the deterministic reading of the same
/// set, and it is what the captured report shows.
pub async fn report_items(
    pool: &PgPool,
    session_id: &str,
    tenant_id: Uuid,
) -> Result<Option<Vec<items::Row>>, sqlx::Error> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM search_sessions WHERE id = $1 AND tenant_id = $2")
            .bind(session_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await?;
    if exists.is_none() {
        return Ok(None);
    }

    // A subquery rather than a join: `session_items` carries its own `quality_score`, so an
    // unqualified join makes that column ambiguous and the answer a 500.
    let rows = sqlx::query_as::<_, items::Row>(concat!(
        "SELECT ",
        item_columns!(),
        " FROM media_items
          WHERE tenant_id = $2
            AND id IN (SELECT media_item_id FROM session_items WHERE session_id = $1)
          ORDER BY id"
    ))
    .bind(session_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(rows))
}
