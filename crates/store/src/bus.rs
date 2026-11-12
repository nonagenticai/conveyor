//! The outbox relay's cursor (CONTRACT §7): the `event` rows the bus has not carried yet.
//!
//! The `event` row is the publish source and Postgres is the record. `published_at IS NULL`
//! means "not yet on the bus"; the relay reads those rows by `seq`, XADDs one envelope each,
//! and records the stream id [`mark_published_tx`]. A Redis loss is therefore recoverable by
//! re-publishing exactly the rows nothing marked — the bus is a nudge, never the record.
//!
//! Both functions are `_tx`: they take the caller's already-scoped connection, so one page is
//! read and marked inside one transaction. A crash between the XADD and the commit leaves the
//! rows unpublished and the envelope duplicated on the stream, which is the at-least-once
//! delivery the consumer de-duplicates on [`crate::outbox::EventRow::id`].
//!
//! `mark_published_tx` writes only the two cursor columns, and the runtime role is granted
//! UPDATE on exactly those (`scripts/grants-runtime.sql`): the rest of an event stays an
//! immutable fact.

use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::outbox::{EVENT_COLUMNS, EventRow};

/// The unpublished rows of one tenant, oldest first, at most `limit`.
///
/// `event_unpublished_idx (tenant_id, seq) WHERE published_at IS NULL` is the index this read
/// is shaped for. The tenant is bound explicitly as well as by RLS: the policy would confine
/// the rows anyway, and the explicit predicate makes the intent readable in the query.
pub async fn unpublished_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<EventRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {EVENT_COLUMNS} FROM event
          WHERE tenant_id = $1 AND published_at IS NULL
          ORDER BY seq
          LIMIT $2"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await
}

/// Record that `event_id` is on the bus as `stream_id`, so the relay never publishes it again.
///
/// The row stays in place: `bus_stream_id` and `published_at` are a cursor, not a tombstone,
/// and `event.prune` remains the only deleter. A re-mark of an already-published row is
/// harmless (the stream id is refreshed); the transaction that a missing row belongs to simply
/// affects nothing, and the next page no longer selects it.
pub async fn mark_published_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    event_id: Uuid,
    stream_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE event
            SET published_at = now(), bus_stream_id = $3
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(event_id)
    .bind(stream_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
