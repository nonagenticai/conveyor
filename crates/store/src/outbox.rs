//! The transactional outbox: the `event` row AND the doorbell, in the caller's transaction.
//!
//! CONTRACT §7 and data-architecture §2.2/§4:
//! - [`append`] inserts the row and runs `pg_notify('conveyor_event', seq::text)` **in the
//!   same transaction**, so a committed event is always announced and a rolled-back one
//!   never is. The payload is `seq` alone: PostgreSQL rejects a NOTIFY payload above 8,000
//!   bytes, and a JSON payload would fail exactly when an event grows (S02 trap 4).
//! - The notification is a DOORBELL only (S2-D4). A subscriber re-queries its own tenant
//!   scope with its own topics, so there is no cross-tenant read path through the
//!   notification channel — the channel carries no tenant, no topic and no payload.
//! - [`replay`] is the one read: `seq > $last AND topic = ANY($topics) ORDER BY seq LIMIT`.
//!   `event.seq` is the only ordering authority; a timestamp would tie and could regress.
//!
//! Scoping is explicit in every signature: [`replay`], [`bounds`] and [`max_seq`] take a
//! [`TenantId`] and open their own [`begin_scoped`] transaction; [`append`] takes a
//! [`TenantId`] and runs on the caller's already-scoped connection, so its row joins the
//! caller's transaction. The grammar checks ([`valid_event_type`], [`valid_topic`]) and
//! [`id_of`] touch no database at all.

use chrono::{DateTime, Utc};
use conveyor_domain::{Id, IdError, TenantId};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::scope::begin_scoped;

/// The LISTEN/NOTIFY channel. One name, one doorbell.
pub const NOTIFY_CHANNEL: &str = "conveyor_event";

/// One replay page. A client that needs more is told to `resync` (CONTRACT §7).
pub const REPLAY_LIMIT: i64 = 500;

/// Data-architecture §5: an event payload above this is a modelling error.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

/// Topic length cap. The grammar itself is [`valid_topic`].
pub const MAX_TOPIC_CHARS: usize = 128;

/// The projection every read uses. `type` is a SQL keyword, hence the rename on [`EventRow`].
pub const EVENT_COLUMNS: &str = "seq, id, tenant_id, topic, type, aggregate_type, aggregate_id, payload, \
     correlation_id, causation_id, created_at";

/// An event to append. Owned so callers can build it and move it into [`append`].
#[derive(Clone, Debug)]
pub struct NewEvent {
    /// `<aggregate>:{uuid}` or `<aggregate>` (CONTRACT §7).
    pub topic: String,
    /// `<aggregate>.<past_tense_verb>`, exactly one dot (CONTRACT §7).
    pub event_type: String,
    /// The table that owns the emitter (`card`, `run`, `conversation_turn`, …).
    pub aggregate_type: String,
    /// The id of the aggregate this event is about.
    pub aggregate_id: Uuid,
    /// The event document, read whole by consumers.
    pub payload: Value,
    /// Ties events of one operation together.
    pub correlation_id: Option<Uuid>,
    /// The event that caused this one.
    pub causation_id: Option<Uuid>,
}

/// One `event` row as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EventRow {
    /// The global ordering authority and the SSE `Last-Event-ID`.
    pub seq: i64,
    /// The event's own id; consumers dedupe on it (at-least-once delivery).
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// `<aggregate>:{uuid}` or `<aggregate>`.
    pub topic: String,
    /// `<aggregate>.<past_tense_verb>`.
    #[sqlx(rename = "type")]
    pub event_type: String,
    /// The table that owns the emitter.
    pub aggregate_type: String,
    /// The aggregate this event is about.
    pub aggregate_id: Uuid,
    /// The event document.
    pub payload: Value,
    /// Caller-supplied correlation id.
    pub correlation_id: Option<Uuid>,
    /// Causal predecessor, if any.
    pub causation_id: Option<Uuid>,
    /// When the row was written.
    pub created_at: DateTime<Utc>,
}

/// Why an event could not be appended.
#[derive(Debug, thiserror::Error)]
pub enum OutboxError {
    /// The insert or the NOTIFY failed; the transaction is the caller's to roll back.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The event type is not `<aggregate>.<past_tense_verb>`.
    #[error("event type {0:?} must be exactly <aggregate>.<past_tense_verb> in lower_snake_case")]
    InvalidEventType(String),
    /// The topic is not `<aggregate>` or `<aggregate>:<uuid>`.
    #[error(
        "topic {0:?} must be <aggregate> or <aggregate>:<uuid>, at most {MAX_TOPIC_CHARS} chars"
    )]
    InvalidTopic(String),
    /// The serialized payload exceeds the data-architecture cap.
    #[error("event payload is {size} bytes; the cap is {max}")]
    PayloadTooLarge {
        /// Measured serialized size.
        size: usize,
        /// The cap that was exceeded.
        max: usize,
    },
    /// A row decoded to a nil id, which no row can hold.
    #[error("decoded an unusable id: {0}")]
    Id(#[from] IdError),
}

fn is_word(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

/// `<aggregate>.<past_tense_verb>`: exactly one dot, both halves lower_snake_case.
pub fn valid_event_type(event_type: &str) -> bool {
    let Some((aggregate, verb)) = event_type.split_once('.') else {
        return false;
    };
    !aggregate.is_empty()
        && !verb.is_empty()
        && !verb.contains('.')
        && aggregate.bytes().all(is_word)
        && verb.bytes().all(is_word)
}

/// CONTRACT §7's topic grammar: `<aggregate>` or `<aggregate>:<uuid>`.
///
/// The aggregate segment is lower_snake_case (`board`, `governed_asset`); the id, when
/// present, must be a bare hyphenated non-nil UUID. Anything else — `board:notauuid`, an
/// uppercase aggregate, a third segment — is refused at the HTTP edge and again at the
/// writer, so a mistyped subscription fails loudly instead of matching nothing forever.
pub fn valid_topic(topic: &str) -> bool {
    if topic.is_empty() || topic.len() > MAX_TOPIC_CHARS {
        return false;
    }
    let mut parts = topic.splitn(3, ':');
    let aggregate = parts.next().unwrap_or_default();
    if aggregate.is_empty()
        || !aggregate
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return false;
    }
    match (parts.next(), parts.next()) {
        (None, None) => true,
        (Some(id), None) => Id::parse(id).is_ok(),
        _ => false,
    }
}

/// Append an event and ring the doorbell — both inside the caller's transaction.
///
/// The caller passes the transaction's connection, so the row and every mutation of the
/// same request commit or roll back together. `seq` comes back for the SSE id.
pub async fn append(
    conn: &mut PgConnection,
    tenant: TenantId,
    event: &NewEvent,
) -> Result<EventRow, OutboxError> {
    if !valid_event_type(&event.event_type) {
        return Err(OutboxError::InvalidEventType(event.event_type.clone()));
    }
    if !valid_topic(&event.topic) {
        return Err(OutboxError::InvalidTopic(event.topic.clone()));
    }
    let serialized = serde_json::to_vec(&event.payload)
        .map_err(|e| OutboxError::Database(sqlx::Error::Encode(Box::new(e))))?;
    if serialized.len() > MAX_PAYLOAD_BYTES {
        return Err(OutboxError::PayloadTooLarge {
            size: serialized.len(),
            max: MAX_PAYLOAD_BYTES,
        });
    }

    let row: EventRow = sqlx::query_as(
        "INSERT INTO event
             (tenant_id, topic, type, aggregate_type, aggregate_id, payload,
              correlation_id, causation_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING seq, id, tenant_id, topic, type, aggregate_type, aggregate_id, payload,
                   correlation_id, causation_id, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(&event.topic)
    .bind(&event.event_type)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id)
    .bind(&event.payload)
    .bind(event.correlation_id)
    .bind(event.causation_id)
    .fetch_one(&mut *conn)
    .await?;

    // The doorbell. `seq::text` only: the payload cap is 8,000 bytes and a JSON payload
    // would fail exactly when events get large.
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(NOTIFY_CHANNEL)
        .bind(row.seq.to_string())
        .execute(conn)
        .await?;

    Ok(row)
}

/// Replay `seq > after_seq` for `topics`, oldest first, at most `limit` rows.
///
/// Runs in the subscriber's tenant scope; RLS is what keeps another tenant's rows out even
/// when both tenants use the same topic string.
pub async fn replay(
    pool: &PgPool,
    tenant: TenantId,
    topics: &[String],
    after_seq: i64,
    limit: i64,
) -> Result<Vec<EventRow>, sqlx::Error> {
    replay_scoped(
        pool,
        tenant,
        &ReplayScope::topics(topics.to_vec()),
        after_seq,
        limit,
    )
    .await
}

/// What one subscription listens to: whole topics, plus per-aggregate topics.
///
/// A `card:{id}` subscription is **resolved** to the card's board topic plus an
/// `aggregate_id` filter (S5-D4) — one outbox row is written per card event and the filter
/// is what narrows it, never a second row and never a payload inspection. A `board:{id}`
/// subscription is a plain topic.
///
/// The union is `topic = ANY(topics) OR (topic = ANY(aggregate_topics) AND aggregate_id =
/// ANY(aggregate_ids))`. That is exact for card filters because a card's events always carry
/// their own board's topic: a card cannot move between boards (`move_card_tx` refuses it),
/// so `(board_of(card_x), card_y)` for two different cards never occurs.
#[derive(Clone, Debug, Default)]
pub struct ReplayScope {
    /// Topics subscribed in full.
    pub topics: Vec<String>,
    /// Topics a per-aggregate filter applies to (a resolved `card:{id}`'s board topic).
    pub aggregate_topics: Vec<String>,
    /// The aggregates the filter admits (`aggregate_id`).
    pub aggregate_ids: Vec<Uuid>,
}

impl ReplayScope {
    /// A scope over whole topics, with no aggregate filter.
    pub fn topics(topics: Vec<String>) -> Self {
        Self {
            topics,
            aggregate_topics: Vec::new(),
            aggregate_ids: Vec::new(),
        }
    }

    /// True when nothing is subscribed at all (an empty scope must never read the table).
    pub fn is_empty(&self) -> bool {
        self.topics.is_empty() && self.aggregate_ids.is_empty()
    }
}

/// [`replay`] with the S5-D4 aggregate filter (`card:{id}` → board topic + `aggregate_id`).
pub async fn replay_scoped(
    pool: &PgPool,
    tenant: TenantId,
    scope: &ReplayScope,
    after_seq: i64,
    limit: i64,
) -> Result<Vec<EventRow>, sqlx::Error> {
    if scope.is_empty() {
        return Ok(Vec::new());
    }
    let mut tx = begin_scoped(pool, tenant).await?;
    let sql = format!(
        "SELECT {EVENT_COLUMNS} FROM event
          WHERE seq > $1
            AND ( topic = ANY($2)
                  OR (topic = ANY($3) AND aggregate_id = ANY($4)) )
          ORDER BY seq
          LIMIT $5"
    );
    let rows: Vec<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(after_seq)
        .bind(scope.topics.clone())
        .bind(scope.aggregate_topics.clone())
        .bind(scope.aggregate_ids.clone())
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows)
}

/// The tenant's first and last retained `seq`, for the resync decision.
///
/// No rows at all is `(None, None)`: a tenant that never wrote an event has no gap to
/// report. A cursor below `min - 1` means rows were pruned under the client; a cursor above
/// `max` means it did not come from this database.
pub async fn bounds(
    pool: &PgPool,
    tenant: TenantId,
) -> Result<(Option<i64>, Option<i64>), sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: (Option<i64>, Option<i64>) = sqlx::query_as("SELECT min(seq), max(seq) FROM event")
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(row)
}

/// The highest committed `seq` for this tenant (the `/metrics` outbox gauge).
pub async fn max_seq(pool: &PgPool, tenant: TenantId) -> Result<Option<i64>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let seq: Option<i64> = sqlx::query_scalar("SELECT max(seq) FROM event")
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(seq)
}

/// Decode a raw uuid into [`Id`] (used by callers that read `aggregate_id`).
pub fn id_of(raw: Uuid) -> Result<Id, IdError> {
    Id::from_uuid(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_grammar_is_one_dot_and_lower_snake_case() {
        for ok in [
            "card.moved",
            "governed_asset.created",
            "conversation_turn.created",
        ] {
            assert!(valid_event_type(ok), "{ok}");
        }
        for bad in [
            "",
            "card",
            "Card.moved",
            "card.Moved",
            "card.moved.now",
            "card..moved",
            ".moved",
            "card.",
            "card moved.x",
            "card.moved-now",
        ] {
            assert!(!valid_event_type(bad), "{bad:?}");
        }
    }

    #[test]
    fn topic_grammar_accepts_aggregate_and_aggregate_id() {
        let id = "0195f48b-d000-7000-8000-000000000099";
        let accepted = [
            "board".to_owned(),
            "governed_asset".to_owned(),
            format!("board:{id}"),
            format!("governed_asset:{id}"),
            format!("run:{id}"),
        ];
        for ok in &accepted {
            assert!(valid_topic(ok), "{ok}");
        }

        let nil = "00000000-0000-0000-0000-000000000000";
        let too_long = "a".repeat(MAX_TOPIC_CHARS + 1);
        let rejected = [
            String::new(),
            "Board".to_owned(),
            "board:notauuid".to_owned(),
            "board:".to_owned(),
            "board:a".to_owned(),
            "board:a:b".to_owned(),
            "board:x y".to_owned(),
            "board:a,b".to_owned(),
            "board/%".to_owned(),
            "governed-asset".to_owned(),
            "governed.asset".to_owned(),
            format!("board:{}", &id[..35]),
            "board:0195f48bd00070008000000000000099".to_owned(),
            format!("board:{nil}"),
            format!("board:{{{id}}}"),
            too_long,
        ];
        for bad in &rejected {
            assert!(!valid_topic(bad), "{bad:?}");
        }
        println!(
            "accepted {} topics {accepted:?}; rejected {} including board:notauuid and a nil uuid",
            accepted.len(),
            rejected.len()
        );
    }
}
