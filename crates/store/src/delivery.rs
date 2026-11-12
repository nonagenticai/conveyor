//! Messages and their per-recipient deliveries (S06 §5.6, data-arch §2.6, §3.6).
//!
//! `messages.send` writes ONE `message` and N `delivery` rows in the caller's transaction.
//! The delivery is the unit the drainer works on, because "delivered once" is the unique key
//! `(message_id, channel_kind, recipient_ref)` and not a convention (§3.6, S06 trap 2).
//!
//! That key also makes a replayed fan-out idempotent: [`insert_delivery_tx`] returns the
//! existing row with `existed = true` instead of an error. The statement is `ON CONFLICT DO
//! NOTHING` rather than a caught `23505` on purpose — a unique violation aborts the whole
//! transaction, and the caller is in the middle of writing a message plus its recipients.
//!
//! The worker sends one `delivery.send` job per row (dedupe `delivery:<id>`) and writes
//! through the four `mark_*` functions; [`pending_deliveries_tx`] is the bounded batch read of
//! live rows (`FOR UPDATE SKIP LOCKED`) that the store test uses to prove the partial index
//! predicate, and that a future reconciler would sweep with — the drainer itself does not
//! batch-claim.
//!
//! Each `mark_*` locks the row, asks
//! `conveyor_domain::Delivery` whether the transition exists, and then writes with the OLD
//! state still in the `WHERE` — the run-transition pattern (data-arch §3, §4). A replay of
//! the same acknowledgement is a no-op, not a second transition: at-least-once delivery of
//! the `delivery.send` job is in the contract (§5.6).
//!
//! `delivery.attempts` counts send attempts; the budget is the caller's `max_attempts`
//! (§5.6: `attempts >= max_attempts` lands `failed`, backoff `min(2^attempts · 5 s, 900 s)`).
//! The row carries no budget column (data-arch §2.6), so the caller passes it.
//!
//! `message` is a tenant-owned parent that has no `parent::Parent` variant (the whitelist
//! predates S06). The same existence check is therefore spelled in [`ensure_message_tx`];
//! move it into `parent` the moment a second module needs it, rather than copying it again.

use chrono::{DateTime, Utc};
use conveyor_domain::{Delivery, DeliveryError, DeliveryState, TenantId};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::jobs::MAX_ERROR_CHARS;
use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};
use crate::version_of;

/// The send-attempt budget S06a's channel driver uses (`5 attempts`, §5.3).
pub const DELIVERY_MAX_ATTEMPTS: i32 = 5;

/// The largest batch [`pending_deliveries_tx`] will return, whatever the caller asks for.
///
/// A drainer that asks for everything holds row locks for the whole table and sends outside
/// the transaction; the bound is what keeps one worker's failure from becoming the queue's.
pub const MAX_DELIVERY_BATCH: i64 = 100;

/// The job kind that drains one delivery (§5.6).
pub const DELIVERY_SEND_KIND: &str = "delivery.send";

/// The `delivery.send` dedupe key for one delivery.
///
/// The delivery id is the idempotency key the whole drainer rests on (§3.6): a re-enqueue
/// while a job is live collapses onto it, and a replayed fan-out cannot start a second
/// worker racing the first.
pub fn delivery_send_dedupe_key(delivery_id: Uuid) -> String {
    format!("delivery:{delivery_id}")
}

/// A message to write. Append-only: there is no update path.
#[derive(Clone, Debug)]
pub struct NewMessage {
    /// `web`, `telegram` or `mcp`.
    pub channel_kind: String,
    /// The sending principal, when one is known.
    pub sender_id: Option<Uuid>,
    /// The message shape (`text` by default).
    pub kind: String,
    /// The content.
    pub body: String,
    /// Per-send content metadata, read whole with the row.
    pub meta: Value,
    /// The run/approval that caused the send.
    pub correlation_id: Option<Uuid>,
}

impl NewMessage {
    /// A plain text message from no recorded sender.
    pub fn new(channel_kind: &str, body: impl Into<String>) -> Self {
        Self {
            channel_kind: channel_kind.to_owned(),
            sender_id: None,
            kind: "text".to_owned(),
            body: body.into(),
            meta: Value::Object(serde_json::Map::new()),
            correlation_id: None,
        }
    }
}

/// One `message` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct MessageRecord {
    /// The row id.
    pub id: Uuid,
    /// `web`, `telegram` or `mcp`.
    pub channel_kind: String,
    /// The sending principal, when one is recorded.
    pub sender_id: Option<Uuid>,
    /// The message shape.
    pub kind: String,
    /// The content.
    pub body: String,
    /// Per-send content metadata.
    pub meta: Value,
    /// The run/approval that caused the send.
    pub correlation_id: Option<Uuid>,
    /// When the message was written.
    pub created_at: DateTime<Utc>,
}

/// A delivery to write for one recipient of `message_id`.
#[derive(Clone, Debug)]
pub struct NewDelivery {
    /// The message being fanned out.
    pub message_id: Uuid,
    /// The channel this recipient is reached on.
    pub channel_kind: String,
    /// The channel-native recipient.
    pub recipient_ref: String,
    /// When this delivery stops being worth attempting.
    pub expires_at: Option<DateTime<Utc>>,
}

impl NewDelivery {
    /// A delivery with no deadline.
    pub fn new(message_id: Uuid, channel_kind: &str, recipient_ref: &str) -> Self {
        Self {
            message_id,
            channel_kind: channel_kind.to_owned(),
            recipient_ref: recipient_ref.to_owned(),
            expires_at: None,
        }
    }
}

/// What [`insert_delivery_tx`] produced: the row, and whether it already existed.
#[derive(Clone, Debug)]
pub struct InsertedDelivery {
    /// The row (new or pre-existing).
    pub delivery: Delivery,
    /// True when the unique key already named a row — a replayed fan-out.
    pub existed: bool,
}

/// One inbox row: the delivery plus the message it carries.
#[derive(Clone, Debug)]
pub struct RecipientDelivery {
    /// The delivery state and its counters.
    pub delivery: Delivery,
    /// The message body.
    pub body: String,
    /// The message shape.
    pub message_kind: String,
    /// The sending principal, when one is recorded.
    pub sender_id: Option<Uuid>,
    /// When the message was written.
    pub message_created_at: DateTime<Utc>,
    /// When the delivery row was written — the inbox's cursor key.
    pub created_at: DateTime<Utc>,
}

/// Why a delivery operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryStoreError {
    /// The message named by a delivery does not exist in this tenant.
    ///
    /// The FK alone would accept another tenant's message id (referential checks bypass
    /// RLS), so this is a resolver refusal, never a constraint violation (CONTRACT §2).
    #[error("the message does not exist in this tenant")]
    MessageNotFound,
    /// A referenced `app_user` (a message sender) does not exist.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The domain refused the transition.
    #[error(transparent)]
    Illegal(#[from] DeliveryError),
    /// A stored vocabulary value no longer parses.
    #[error("the database holds delivery.state = {0:?}, which the domain does not know")]
    UnknownState(String),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const COLUMNS: &str = "id, message_id, channel_kind, recipient_ref, state, attempts, \
                       last_error, sent_at, delivered_at, expires_at, version";

#[derive(Debug, sqlx::FromRow)]
struct DeliveryRow {
    id: Uuid,
    message_id: Uuid,
    channel_kind: String,
    recipient_ref: String,
    state: String,
    attempts: i32,
    last_error: Option<String>,
    sent_at: Option<DateTime<Utc>>,
    delivered_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    version: i64,
}

impl TryFrom<DeliveryRow> for Delivery {
    type Error = DeliveryStoreError;
    fn try_from(row: DeliveryRow) -> Result<Self, Self::Error> {
        let state = row
            .state
            .parse::<DeliveryState>()
            .map_err(|_| DeliveryStoreError::UnknownState(row.state))?;
        Ok(Self {
            id: row.id,
            message_id: row.message_id,
            channel_kind: row.channel_kind,
            recipient_ref: row.recipient_ref,
            state,
            attempts: row.attempts,
            last_error: row.last_error,
            sent_at: row.sent_at,
            delivered_at: row.delivered_at,
            expires_at: row.expires_at,
            version: version_of(row.version),
        })
    }
}

/// Resolve a message id inside the caller's tenant before a delivery names it.
///
/// `message` has no `parent::Parent` variant; see the module docs for why the check is
/// spelled here and what to do when a second module needs it.
async fn ensure_message_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    message_id: Uuid,
) -> Result<(), DeliveryStoreError> {
    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM message WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(message_id)
    .fetch_one(&mut *conn)
    .await?;
    if present {
        Ok(())
    } else {
        Err(DeliveryStoreError::MessageNotFound)
    }
}

/// Read one delivery `FOR UPDATE` inside the caller's scoped transaction.
///
/// The drainer opens its attempt with this: the row lock is what makes "terminal wins" hold
/// across two workers, and it is released at the caller's COMMIT — never before the send,
/// because the worker sends **outside** the transaction (§5.6).
pub async fn lock_delivery_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM delivery WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Delivery::try_from).transpose()
}

/// Insert a message inside the caller's scoped transaction.
pub async fn insert_message_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewMessage,
) -> Result<MessageRecord, DeliveryStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.sender_id).await?;
    let row: MessageRecord = sqlx::query_as(
        "INSERT INTO message (tenant_id, channel_kind, sender_id, kind, body, meta, correlation_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, channel_kind, sender_id, kind, body, meta, correlation_id, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(&input.channel_kind)
    .bind(input.sender_id)
    .bind(&input.kind)
    .bind(&input.body)
    .bind(&input.meta)
    .bind(input.correlation_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

/// Insert one delivery, or return the row the unique key already names.
///
/// `existed = true` is the idempotent answer to a replayed fan-out: no second row, no error,
/// and the caller's transaction stays usable.
pub async fn insert_delivery_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewDelivery,
) -> Result<InsertedDelivery, DeliveryStoreError> {
    ensure_message_tx(conn, tenant, input.message_id).await?;
    let sql = format!(
        "INSERT INTO delivery (tenant_id, message_id, channel_kind, recipient_ref, expires_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT ON CONSTRAINT delivery_message_channel_recipient_key DO NOTHING
         RETURNING {COLUMNS}"
    );
    let inserted: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.message_id)
        .bind(&input.channel_kind)
        .bind(&input.recipient_ref)
        .bind(input.expires_at)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = inserted {
        return Ok(InsertedDelivery {
            delivery: row.try_into()?,
            existed: false,
        });
    }
    // The conflict fired: read the row it protected. The message id was already resolved in
    // this tenant, so the existing row is this tenant's (nothing else can own the key).
    let sql = format!(
        "SELECT {COLUMNS} FROM delivery
          WHERE tenant_id = $1 AND message_id = $2 AND channel_kind = $3 AND recipient_ref = $4"
    );
    let existing: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.message_id)
        .bind(&input.channel_kind)
        .bind(&input.recipient_ref)
        .fetch_optional(&mut *conn)
        .await?;
    let row = existing.ok_or(sqlx::Error::RowNotFound)?;
    Ok(InsertedDelivery {
        delivery: row.try_into()?,
        existed: true,
    })
}

/// Enqueue the `delivery.send` job for one delivery on the caller's transaction.
///
/// `messages.send` must commit the message, its N deliveries and their N jobs together
/// (CONTRACT §7): a crash between them would leave a delivery nobody drains or a job naming
/// a delivery that never existed. The statement is `jobs::enqueue`'s, on the caller's
/// connection, deduped on `delivery:<id>`; `None` means a live job already covers the row.
pub async fn enqueue_delivery_send_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    delivery_id: Uuid,
    run_after: Option<DateTime<Utc>>,
) -> Result<Option<Uuid>, sqlx::Error> {
    let payload = serde_json::json!({ "delivery_id": delivery_id });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, run_after, max_attempts)
         VALUES ($1, $2, $3, $4, coalesce($5, now()), $6)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(DELIVERY_SEND_KIND)
    .bind(delivery_send_dedupe_key(delivery_id))
    .bind(payload)
    .bind(run_after)
    .bind(DELIVERY_MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Resolve a message recipient as an `app_user` that belongs to the caller's tenant.
///
/// `app_user` is global (no `tenant_id`, no RLS), so existence alone would let one tenant
/// address another tenant's people; the membership predicate is the tenant half of the
/// resolution (CONTRACT §2). `false` is a resolver miss the action returns as `404`.
pub async fn ensure_recipient_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM app_user u
              WHERE u.id = $2
                AND EXISTS (
                    SELECT 1 FROM membership m
                     WHERE m.tenant_id = $1 AND m.user_id = u.id
                )
         )",
    )
    .bind(tenant.as_uuid())
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await
}

/// Claim up to `limit` live deliveries for the drainer.
///
/// `state IN ('pending', 'sent')` matches `delivery_live_idx`; `FOR UPDATE SKIP LOCKED`
/// lets several workers drain in parallel without handing one row to two of them. The locks
/// live until the caller's transaction ends, so the caller sends **outside** that
/// transaction (§5.6) and then writes the outcome through a `mark_*` function.
pub async fn pending_deliveries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<Delivery>, DeliveryStoreError> {
    let limit = limit.clamp(1, MAX_DELIVERY_BATCH);
    let sql = format!(
        "SELECT {COLUMNS} FROM delivery
          WHERE tenant_id = $1 AND state IN ('pending', 'sent')
          ORDER BY created_at, id
          FOR UPDATE SKIP LOCKED
          LIMIT $2"
    );
    let rows: Vec<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(Delivery::try_from).collect()
}

/// Record the first send attempt (`pending → sent`).
///
/// `None` means the row is absent or was already past `pending`: at-least-once means a
/// replayed attempt is a no-op, never an error. `sent_at` is coalesced, so the first attempt
/// keeps the instant.
pub async fn mark_sent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let sql = format!(
        "UPDATE delivery
            SET state = 'sent', sent_at = coalesce(sent_at, now()),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = 'pending'
         RETURNING {COLUMNS}"
    );
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Delivery::try_from).transpose()
}

/// Mark the channel's confirmation (`sent → delivered`).
///
/// A row that is already `delivered` is returned unchanged: replaying the same
/// acknowledgement is not a second transition.
pub async fn mark_delivered_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let Some(delivery) = lock_delivery_tx(conn, tenant, id).await? else {
        return Ok(None);
    };
    if delivery.state == DeliveryState::Delivered {
        return Ok(Some(delivery));
    }
    let from = delivery.state;
    delivery.deliver()?;
    let sql = format!(
        "UPDATE delivery
            SET state = 'delivered', delivered_at = coalesce(delivered_at, now()),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = $3
         RETURNING {COLUMNS}"
    );
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(from.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    // The row was locked and the domain agreed; a zero-row write is a bug, not an outcome.
    let row = row.ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(row.try_into()?))
}

/// Record a failed attempt: `attempts + 1`, `last_error`, and `failed` once the budget is
/// spent.
///
/// The budget is decided **in the statement's input**, from the row's own `attempts`, so two
/// racing finishers cannot both decide "one more try" (`jobs::fail` uses the same shape).
/// Below the budget the state is unchanged (the row stays live for the next attempt); at or
/// above it the domain's `fail` transition is applied and the row is terminal.
pub async fn mark_failed_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    last_error: &str,
    max_attempts: i32,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let Some(delivery) = lock_delivery_tx(conn, tenant, id).await? else {
        return Ok(None);
    };
    if delivery.state == DeliveryState::Failed {
        return Ok(Some(delivery));
    }
    let max_attempts = max_attempts.max(1);
    let from = delivery.state;
    let attempts = delivery.attempts.saturating_add(1);
    let next_state = if attempts >= max_attempts {
        delivery.fail()?.state
    } else {
        from
    };
    let error = last_error.chars().take(MAX_ERROR_CHARS).collect::<String>();
    let sql = format!(
        "UPDATE delivery
            SET state = $3, attempts = attempts + 1, last_error = $4,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = $5
         RETURNING {COLUMNS}"
    );
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(next_state.as_str())
        .bind(&error)
        .bind(from.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(row.try_into()?))
}

/// Record one send attempt against a live row (`pending | sent → sent`, `attempts + 1`).
///
/// This is the drainer's attempt counter, not a transition machine: `sent → sent` is not a
/// transition, so a retry re-enters `sent` and the budget reads the row's own `attempts`.
/// The caller holds the row lock from [`lock_delivery_tx`] in the same transaction, and the
/// transport runs after that transaction commits (§5.6). `None` means the row is absent or
/// terminal — another finisher already closed it.
pub async fn record_send_attempt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let sql = format!(
        "UPDATE delivery
            SET state = 'sent', sent_at = coalesce(sent_at, now()),
                attempts = attempts + 1, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state IN ('pending', 'sent')
         RETURNING {COLUMNS}"
    );
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Delivery::try_from).transpose()
}

/// Record a failed attempt that was already counted by [`record_send_attempt_tx`].
///
/// The attempt counter is NOT incremented here: the attempt was stamped before the transport
/// ran, and counting it twice would halve the budget. Below `max_attempts` the row keeps its
/// live state (`sent`) and only gains `last_error`; at or above it the domain's `fail`
/// transition lands `failed` and the row is terminal (§3.6). A terminal row is returned
/// unchanged — a replay writes nothing.
pub async fn record_failed_attempt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    last_error: &str,
    max_attempts: i32,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let Some(delivery) = lock_delivery_tx(conn, tenant, id).await? else {
        return Ok(None);
    };
    if delivery.state.is_terminal() {
        return Ok(Some(delivery));
    }
    let max_attempts = max_attempts.max(1);
    let from = delivery.state;
    let next_state = if delivery.attempts >= max_attempts {
        delivery.fail()?.state
    } else {
        from
    };
    let error = last_error.chars().take(MAX_ERROR_CHARS).collect::<String>();
    let sql = format!(
        "UPDATE delivery
            SET state = $3, last_error = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = $5
         RETURNING {COLUMNS}"
    );
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(next_state.as_str())
        .bind(&error)
        .bind(from.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(row.try_into()?))
}

/// Close a delivery that will never be confirmed (`pending | sent → expired`).
///
/// A row that is already `expired` is returned unchanged (replay); anything else terminal is
/// refused by the domain.
pub async fn mark_expired_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Delivery>, DeliveryStoreError> {
    let Some(delivery) = lock_delivery_tx(conn, tenant, id).await? else {
        return Ok(None);
    };
    if delivery.state == DeliveryState::Expired {
        return Ok(Some(delivery));
    }
    let from = delivery.state;
    delivery.expire()?;
    let sql = format!(
        "UPDATE delivery
            SET state = 'expired', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = $3
         RETURNING {COLUMNS}"
    );
    let row: Option<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(from.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(row.try_into()?))
}

#[derive(Debug, sqlx::FromRow)]
struct RecipientRow {
    id: Uuid,
    message_id: Uuid,
    channel_kind: String,
    recipient_ref: String,
    state: String,
    attempts: i32,
    last_error: Option<String>,
    sent_at: Option<DateTime<Utc>>,
    delivered_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    version: i64,
    body: String,
    message_kind: String,
    sender_id: Option<Uuid>,
    message_created_at: DateTime<Utc>,
    delivery_created_at: DateTime<Utc>,
}

impl TryFrom<RecipientRow> for RecipientDelivery {
    type Error = DeliveryStoreError;
    fn try_from(row: RecipientRow) -> Result<Self, Self::Error> {
        let delivery = Delivery::try_from(DeliveryRow {
            id: row.id,
            message_id: row.message_id,
            channel_kind: row.channel_kind,
            recipient_ref: row.recipient_ref,
            state: row.state,
            attempts: row.attempts,
            last_error: row.last_error,
            sent_at: row.sent_at,
            delivered_at: row.delivered_at,
            expires_at: row.expires_at,
            version: row.version,
        })?;
        Ok(Self {
            delivery,
            body: row.body,
            message_kind: row.message_kind,
            sender_id: row.sender_id,
            message_created_at: row.message_created_at,
            created_at: row.delivery_created_at,
        })
    }
}

/// One page of the caller's inbox: the newest deliveries addressed to `recipient_ref`, with
/// the message each carries.
///
/// This is `notifications.list`: a delivery row without its message body would force a
/// second bounded query per row. Newest first on `(created_at, id)` — the pair is the
/// cursor, so a row inserted between two pages cannot make the second page skip one. `unread`
/// restricts the page to the live states (`pending`/`sent`), which is exactly
/// `delivery_live_idx`'s predicate. Every other state is returned when `unread` is false; the
/// action layer decides whether a `failed` or `expired` row is worth showing.
pub async fn recipient_deliveries_page_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    recipient_ref: &str,
    unread: bool,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<RecipientDelivery>, DeliveryStoreError> {
    let limit = limit.clamp(1, MAX_DELIVERY_BATCH);
    let (cursor_at, cursor_id) = match cursor {
        Some((at, id)) => (Some(at), Some(id)),
        None => (None, None),
    };
    let sql = "SELECT d.id, d.message_id, d.channel_kind, d.recipient_ref, d.state, d.attempts,
                      d.last_error, d.sent_at, d.delivered_at, d.expires_at, d.version,
                      d.created_at AS delivery_created_at,
                      m.body, m.kind AS message_kind, m.sender_id,
                      m.created_at AS message_created_at
                 FROM delivery d
                 JOIN message m ON m.id = d.message_id
                WHERE d.tenant_id = $1 AND d.recipient_ref = $2
                  AND ($3::boolean IS FALSE OR d.state IN ('pending', 'sent'))
                  AND ($4::timestamptz IS NULL OR (d.created_at, d.id) < ($4, $5::uuid))
                ORDER BY d.created_at DESC, d.id DESC
                LIMIT $6";
    let rows: Vec<RecipientRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(recipient_ref)
        .bind(unread)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(RecipientDelivery::try_from).collect()
}

/// The newest deliveries addressed to `recipient_ref` — the first inbox page, all states.
///
/// A thin wrapper over [`recipient_deliveries_page_tx`] for callers that have no filters.
pub async fn recipient_deliveries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    recipient_ref: &str,
    limit: i64,
) -> Result<Vec<RecipientDelivery>, DeliveryStoreError> {
    recipient_deliveries_page_tx(conn, tenant, recipient_ref, false, None, limit).await
}

/// Terminal approvals that have no `approval-closed` message yet.
///
/// `approval.closed.emit` is idempotent on this `NOT EXISTS`: the message row with
/// `kind = 'approval-closed'` and `correlation_id = approval.id` IS the record that the close
/// was announced, so a replayed tick finds no candidate rather than sending twice. The
/// `message_correlation_idx` serves the anti-join.
pub async fn approval_closed_candidates_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let limit = limit.clamp(1, MAX_DELIVERY_BATCH);
    sqlx::query_as(
        "SELECT a.id, a.status
           FROM approval a
          WHERE a.tenant_id = $1
            AND a.status IN ('approved', 'rejected', 'expired', 'voided')
            AND NOT EXISTS (
                SELECT 1 FROM message m
                 WHERE m.tenant_id = a.tenant_id
                   AND m.kind = 'approval-closed'
                   AND m.correlation_id = a.id
            )
          ORDER BY a.id
          LIMIT $2",
    )
    .bind(tenant.as_uuid())
    .bind(limit)
    .fetch_all(&mut *conn)
    .await
}

/// Expire every live delivery of one message, recording why (S06d: the first-response loser).
///
/// A resolved approval closes the request on every channel that did not answer. `pending` and
/// `sent` are the only live states, and `→ expired` is a legal edge for both (the data model's
/// original first-response rule); terminal rows are left untouched.
pub async fn expire_live_for_message_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    message_id: Uuid,
    reason: &str,
) -> Result<Vec<Delivery>, DeliveryStoreError> {
    let sql = format!(
        "UPDATE delivery
            SET state = 'expired', last_error = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND message_id = $2 AND state IN ('pending', 'sent')
         RETURNING {COLUMNS}"
    );
    let rows: Vec<DeliveryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(message_id)
        .bind(reason)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(TryInto::try_into).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_contract_s() {
        let message = NewMessage::new("web", "hello");
        assert_eq!(message.kind, "text");
        assert_eq!(message.meta, Value::Object(serde_json::Map::new()));
        assert!(message.sender_id.is_none());
        assert!(message.correlation_id.is_none());

        let delivery = NewDelivery::new(Uuid::now_v7(), "telegram", "chat:1");
        assert!(delivery.expires_at.is_none());
    }

    #[test]
    fn the_batch_bound_is_the_drainers_ceiling() {
        assert_eq!(DELIVERY_MAX_ATTEMPTS, 5);
        assert_eq!(MAX_DELIVERY_BATCH, 100);
    }
}
