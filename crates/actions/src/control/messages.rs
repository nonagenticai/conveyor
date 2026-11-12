#![forbid(unsafe_code)]
//! Comms actions: `messages.send`, `notifications.list`, `notifications.update` (S06 §5.6).
//!
//! `messages.send` writes ONE `message` and one `delivery` per recipient, each enqueuing a
//! `delivery.send` job on `dedupe_key = delivery.id`, all in the caller's transaction
//! (CONTRACT §7). The recipients are resolved first: `app_user` is global, so a bare FK would
//! accept another tenant's person; the membership predicate is what makes "in the caller's
//! tenant" true, and a miss is a `404`, never an FK violation (CONTRACT §2).
//!
//! Notifications are the caller's OWN delivery rows and channel preferences — never another
//! user's. `notifications.list` reads `recipient_ref = ctx.actor_id` on the
//! `(created_at, id)` cursor; `notifications.update` upserts one preference on
//! `(tenant, user, channel_kind)` with partial semantics (an omitted field keeps its value).

use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{self, RawEvent};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::views::{
    DeliveryOut, MessageSendIn, MessageSendOut, NotificationListIn, NotificationPreference,
    NotificationUpdateIn,
};
use super::{invalid, parent_error, store_error};

/// `message.sent` (S06 §5.6) — one send was fanned out to its recipients.
pub const MESSAGE_SENT: &str = "message.sent";

/// `messages.send`'s input: the shared document plus the correlation id it does not carry.
///
/// The document is embedded with `flatten` rather than copied, so the two stay one shape:
/// `MessageSendIn` is the shared projection (S06 §5.2) and this wrapper only adds the
/// run/approval the send belongs to, which is stored on `message.correlation_id`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MessageSendRequest {
    /// The shared `messages.send` document.
    #[serde(flatten)]
    pub message: MessageSendIn,
    /// Links the send to the run or approval that caused it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
}

/// Send one message to every recipient, fanning out one delivery per address.
///
/// The message is written once; the recipients are deduplicated **before** the fan-out, so a
/// repeated address is one delivery row (the unique key would collapse it anyway, but the
/// response must not name the same id twice). Each delivery enqueues its `delivery.send` job
/// on the same transaction: a committed message is always drainable and a rolled-back one
/// leaves no job behind.
#[crate::action(
    name = "messages.send",
    version = 1,
    kind = "internal",
    scopes = ["messages:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn messages_send(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MessageSendRequest,
) -> Result<HandlerOutput<MessageSendOut>, ActionError> {
    let action = "messages.send";
    let request = input.message;
    validate_channel_kind(&request.channel_kind)?;

    // Recipients are canonicalised to the one uuid spelling (`to_string()` is lower-case
    // hyphenated) BEFORE the unique key is applied: `Uuid::parse_str` also accepts braced,
    // upper-case and URN forms, so raw strings would let one person become two deliveries —
    // and a non-canonical `recipient_ref` would never match the inbox query, which compares
    // against `ctx.actor_id.to_string()`. The same person twice in one send is one delivery.
    // 2026-11-08: the `audience_id` expansion lived here. `audience`/`audience_member` were dropped by
    // migration 0011 (S00 day 2 measured them unused), so a send names its recipients explicitly.
    let mut recipients: Vec<Uuid> = Vec::new();
    for raw in request.recipients {
        let recipient = Uuid::parse_str(&raw).map_err(|_| {
            invalid(
                "recipients",
                "invalid_value",
                format!("{raw:?} is not an app_user id"),
            )
        })?;
        push_recipient(&mut recipients, recipient);
    }
    if recipients.is_empty() {
        return Err(invalid(
            "recipients",
            "invalid_value",
            "a send needs at least one recipient",
        ));
    }

    // Resolve every recipient in the caller's tenant BEFORE the message is written: a foreign
    // id is a 404 here, and the delivery insert can then only fail on a real bug.
    for recipient in &recipients {
        if !store::delivery::ensure_recipient_tx(&mut *tx, ctx.tenant, *recipient)
            .await
            .map_err(|error| store_error(error, action))?
        {
            return Err(ActionError::resource_not_found("recipient"));
        }
    }

    let message = store::delivery::insert_message_tx(
        &mut *tx,
        ctx.tenant,
        &store::delivery::NewMessage {
            channel_kind: request.channel_kind.clone(),
            sender_id: Some(ctx.actor_id),
            kind: request.kind.unwrap_or_else(|| "text".to_owned()),
            body: request.body,
            meta: request.meta.unwrap_or_else(empty_object),
            correlation_id: input.correlation_id,
        },
    )
    .await
    .map_err(|error| delivery_error(error, action))?;

    let mut delivery_ids = Vec::with_capacity(recipients.len());
    for recipient in &recipients {
        let inserted = store::delivery::insert_delivery_tx(
            &mut *tx,
            ctx.tenant,
            &store::delivery::NewDelivery::new(
                message.id,
                &request.channel_kind,
                &recipient.to_string(),
            ),
        )
        .await
        .map_err(|error| delivery_error(error, action))?;
        store::delivery::enqueue_delivery_send_tx(&mut *tx, ctx.tenant, inserted.delivery.id, None)
            .await
            .map_err(|error| store_error(error, action))?;
        delivery_ids.push(inserted.delivery.id.to_string());
    }

    let out = MessageSendOut {
        message_id: message.id.to_string(),
        delivery_ids,
    };
    emit_message_sent(&mut *tx, &ctx, &message, &out).await?;
    Ok(HandlerOutput::created(out))
}

/// The caller's own notifications, newest first.
///
/// `recipient_ref` is the caller's `app_user.id`, so one person can never page another's
/// inbox. `unread` narrows to the live states (`pending`/`sent`); the cursor is the
/// `(created_at, id)` of the previous page's last row.
#[crate::action(
    name = "notifications.list",
    version = 1,
    kind = "internal",
    scopes = ["notifications:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn notifications_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: NotificationListIn,
) -> Result<Page<DeliveryOut>, ActionError> {
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(store::actions::decode_time_cursor(raw).ok_or_else(|| {
            invalid(
                "cursor",
                "invalid_cursor",
                "not a cursor this endpoint issued",
            )
        })?),
    };
    let limit = store::actions::clamp_limit(input.page.limit);

    // One extra row decides whether a next page exists; it is never returned.
    let mut rows = store::delivery::recipient_deliveries_page_tx(
        &mut *tx,
        ctx.tenant,
        &ctx.actor_id.to_string(),
        input.unread.unwrap_or(false),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| delivery_error(error, "notifications.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.delivery.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(delivery_out).collect(),
        next_cursor,
    })
}

/// Store one channel preference for the calling user.
///
/// The upsert is keyed on `(tenant, user, channel_kind)`, so a replayed call converges on one
/// row instead of a second. An omitted `enabled`/`quiet_hours` keeps the stored value: a
/// client that only toggles the channel must not reset its quiet window.
#[crate::action(
    name = "notifications.update",
    version = 1,
    kind = "internal",
    scopes = ["notifications:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn notifications_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: NotificationUpdateIn,
) -> Result<NotificationPreference, ActionError> {
    let action = "notifications.update";
    validate_channel_kind(&input.channel_kind)?;
    let preference = store::channel::upsert_preference_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        &input.channel_kind,
        input.enabled,
        input.quiet_hours.as_ref(),
    )
    .await
    .map_err(|error| channel_error(error, action))?;
    Ok(NotificationPreference {
        channel_kind: preference.channel_kind,
        enabled: preference.enabled,
        quiet_hours: preference.quiet_hours,
    })
}

/// The channel vocabulary the database CHECK owns; an unknown kind is a schema-shaped `422`.
fn validate_channel_kind(channel_kind: &str) -> Result<(), ActionError> {
    if store::CHANNEL_KINDS.contains(&channel_kind) {
        Ok(())
    } else {
        Err(invalid(
            "channel_kind",
            "invalid_value",
            "must be web, telegram or mcp",
        ))
    }
}

/// Append a recipient unless it is already present, preserving first-seen order.
fn push_recipient(recipients: &mut Vec<Uuid>, recipient: Uuid) {
    if !recipients.contains(&recipient) {
        recipients.push(recipient);
    }
}

/// The empty JSON object a `meta`/default document starts as.
fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Append `message.sent` on the message's own aggregate topic (S06 §5.6).
async fn emit_message_sent(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    message: &store::delivery::MessageRecord,
    out: &MessageSendOut,
) -> Result<(), ActionError> {
    events::append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: format!("message:{}", message.id),
            board_id: None,
            aggregate_type: "message",
            aggregate_id: message.id,
            event_type: MESSAGE_SENT,
            payload: serde_json::json!({
                "message_id": message.id,
                "channel_kind": message.channel_kind,
            }),
            data: serde_json::to_value(out).unwrap_or(Value::Null),
            prev_data: Value::Null,
        },
    )
    .await
}

/// Map one stored delivery/message row onto the inbox document.
fn delivery_out(row: &store::delivery::RecipientDelivery) -> DeliveryOut {
    let delivery = &row.delivery;
    DeliveryOut {
        id: delivery.id.to_string(),
        message_id: delivery.message_id.to_string(),
        channel_kind: delivery.channel_kind.clone(),
        recipient_ref: delivery.recipient_ref.clone(),
        state: delivery.state.as_str().to_owned(),
        attempts: delivery.attempts,
        last_error: delivery.last_error.clone(),
        sent_at: delivery.sent_at,
        delivered_at: delivery.delivered_at,
        expires_at: delivery.expires_at,
        created_at: row.created_at,
    }
}

/// A delivery-store failure: a resolver miss is a `404`, a refused transition a `409`.
fn delivery_error(error: store::DeliveryStoreError, action: &str) -> ActionError {
    match error {
        store::DeliveryStoreError::Parent(parent) => parent_error(parent, action),
        store::DeliveryStoreError::Illegal(illegal) => {
            ActionError::new(409, "conflict", illegal.to_string())
        }
        store::DeliveryStoreError::Database(error) => store_error(error, action),
        other @ (store::DeliveryStoreError::MessageNotFound
        | store::DeliveryStoreError::UnknownState(_)) => {
            tracing::error!(%other, action, "delivery store failure");
            ActionError::internal()
        }
    }
}

/// A channel-store failure: the actor is a resolved parent, so a miss is a bug, not a `404`.
fn channel_error(error: store::ChannelStoreError, action: &str) -> ActionError {
    match error {
        store::ChannelStoreError::Parent(parent) => parent_error(parent, action),
        store::ChannelStoreError::Database(error) => store_error(error, action),
    }
}
