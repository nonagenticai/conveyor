#![forbid(unsafe_code)]
//! The board outbox rows: exactly one `event` per mutation, in the mutation's transaction.
//!
//! CONTRACT §7: the row and the doorbell are written with the write, so a committed mutation
//! is always announced and a rolled-back one never is. Every board event's topic is
//! `board:<board_id>`; the aggregate is the table that owns the emitter (S05 §5's table), so
//! `aggregate_type='card'` with `aggregate_id=card_id` is what lets a `card:{id}` subscriber
//! resolve to the board's topic plus an `aggregate_id` filter (S5-D4).
//!
//! Trap 5 (S05 §8): a mutation that forgets this row loses webhook and SSE silently, which
//! is why every action test asserts `event_count = 1` for the action under test.
//!
//! This is also the webhook fan-out's ONE seat (S5-D7, "a webhook fires on the event, never
//! a second write path"): after the row is appended, every active subscription that selects
//! this type gets one `webhook.deliver` job in the same transaction. The job carries the
//! envelope `{event, data, prevData, user}` the plan fixes, with `data`/`prevData` the `*Out`
//! representation the emitter supplies — the previous representation cannot be reconstructed
//! after the update, so the emitter hands both in.
//!
//! The type spellings are constants here rather than literals at each call site: the names
//! are S05's contract (`card.moved`, never `cards.moved` — trap 7) and a typo must not be
//! able to invent a new one.

use conveyor_store as store;
use conveyor_store::card::CardRecord;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::eval::{
    EVAL_AUDIT_COMPLETED, EVAL_CASE_CHANGED, EVAL_CHANGED, EVAL_CLIMB_CHANGED,
    EVAL_CLIMB_ROUND_COMPLETED, EVAL_CREATED, EVAL_FINDING_DECIDED, EVAL_RESULT_RECORDED,
    EVAL_RUN_CHANGED,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::workflow::{WORKFLOW_CREATED, WORKFLOW_PUBLISHED, WORKFLOW_RETIRED, WORKFLOW_UPDATED};

/// A card was born (`cards.create`, `cards.duplicate`).
pub const CARD_CREATED: &str = "card.created";
/// A card's content or labels changed (`cards.update`, `cards.add_label`, `cards.remove_label`).
pub const CARD_UPDATED: &str = "card.updated";
/// A card changed lane (`cards.move`).
pub const CARD_MOVED: &str = "card.moved";
/// A card was stamped (`cards.archive`).
pub const CARD_ARCHIVED: &str = "card.archived";
/// A card's stamp was cleared (`cards.unarchive`).
pub const CARD_UNARCHIVED: &str = "card.unarchived";
/// A comment was written (`cards.comment`).
pub const COMMENT_CREATED: &str = "comment.created";
/// A comment's body was replaced (`comments.update`).
pub const COMMENT_UPDATED: &str = "comment.updated";
/// A comment was removed (`comments.delete`).
pub const COMMENT_DELETED: &str = "comment.deleted";
/// A checklist was created, updated or deleted (`task_lists.*`).
pub const TASK_LIST_CHANGED: &str = "task_list.changed";
/// A checklist item was created, updated, moved or deleted (`tasks.*`).
pub const TASK_CHANGED: &str = "task.changed";
/// A board field was created, updated or deleted (`custom_fields.*`).
pub const CUSTOM_FIELD_CHANGED: &str = "custom_field.changed";
/// A card's value for a board field changed (`custom_fields.set_value|clear_value`).
pub const CUSTOM_FIELD_VALUE_CHANGED: &str = "custom_field_value.changed";
/// A board label was created, updated or deleted (`labels.*`).
pub const LABEL_CHANGED: &str = "label.changed";
/// A board membership was set (`boards.set_member`).
pub const BOARD_MEMBER_CHANGED: &str = "board_member.changed";
/// A webhook subscription was created, updated or deleted (`webhooks.*`).
pub const WEBHOOK_CHANGED: &str = "webhook.changed";
/// A work-package memory row was written (`memory.remember`).
pub const MEMORY_RECORDED: &str = "memory.recorded";
/// A capability's knowledge binding changed (`capabilities.bind_knowledge`).
pub const KNOWLEDGE_BOUND: &str = "knowledge.bound";

/// Every event type a webhook subscription may name. S05's board emitters plus the slices
/// that append their own outbox types (S19's eval events); the vocabulary is the register,
/// so `list.changed` or a `project.*` spelling cannot be stored.
pub const S05_EVENT_TYPES: [&str; 30] = [
    CARD_CREATED,
    CARD_UPDATED,
    CARD_MOVED,
    CARD_ARCHIVED,
    CARD_UNARCHIVED,
    COMMENT_CREATED,
    COMMENT_UPDATED,
    COMMENT_DELETED,
    TASK_LIST_CHANGED,
    TASK_CHANGED,
    CUSTOM_FIELD_CHANGED,
    CUSTOM_FIELD_VALUE_CHANGED,
    LABEL_CHANGED,
    BOARD_MEMBER_CHANGED,
    WEBHOOK_CHANGED,
    EVAL_CREATED,
    EVAL_CHANGED,
    EVAL_CASE_CHANGED,
    EVAL_RUN_CHANGED,
    EVAL_RESULT_RECORDED,
    EVAL_AUDIT_COMPLETED,
    EVAL_FINDING_DECIDED,
    EVAL_CLIMB_CHANGED,
    EVAL_CLIMB_ROUND_COMPLETED,
    WORKFLOW_CREATED,
    WORKFLOW_UPDATED,
    WORKFLOW_PUBLISHED,
    WORKFLOW_RETIRED,
    MEMORY_RECORDED,
    KNOWLEDGE_BOUND,
];

/// One board event, ready to append.
pub struct BoardEvent {
    /// The board whose topic the event is published on.
    pub board_id: Uuid,
    /// The table that owns the emitter (`card`, `comment`, `task_list`, …).
    pub aggregate_type: &'static str,
    /// The aggregate's id.
    pub aggregate_id: Uuid,
    /// One of the constants above.
    pub event_type: &'static str,
    /// The event document (S05 §5's payload keys).
    pub payload: Value,
    /// The aggregate's `*Out` representation AFTER the mutation (`Null` for a delete).
    pub data: Value,
    /// The aggregate's `*Out` representation BEFORE the mutation (`Null` for a create).
    pub prev_data: Value,
}

impl BoardEvent {
    /// An event about a card.
    pub fn card(board_id: Uuid, card_id: Uuid, event_type: &'static str, payload: Value) -> Self {
        Self {
            board_id,
            aggregate_type: "card",
            aggregate_id: card_id,
            event_type,
            payload,
            data: Value::Null,
            prev_data: Value::Null,
        }
    }

    /// Attach the post-mutation representation.
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// Attach the pre-mutation representation.
    pub fn with_prev_data(mut self, prev_data: Value) -> Self {
        self.prev_data = prev_data;
        self
    }
}

/// One outbox event whose topic is not necessarily a board (`webhook.changed` on a
/// tenant-wide subscription). `board_id` selects the subscriptions to fan out to.
pub struct RawEvent<'a> {
    /// `board:<uuid>` or a tenant-wide `<aggregate>` topic (CONTRACT §7).
    pub topic: String,
    /// The board the event belongs to; `None` for a tenant-wide aggregate.
    pub board_id: Option<Uuid>,
    /// The table that owns the emitter.
    pub aggregate_type: &'a str,
    /// The aggregate's id.
    pub aggregate_id: Uuid,
    /// `<aggregate>.<past-tense verb>`.
    pub event_type: &'a str,
    /// The event document.
    pub payload: Value,
    /// The post-mutation `*Out` representation.
    pub data: Value,
    /// The pre-mutation `*Out` representation.
    pub prev_data: Value,
}

/// Append one event and enqueue every matching webhook delivery, in the caller's transaction.
pub async fn append_raw_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event: &RawEvent<'_>,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: event.topic.clone(),
        event_type: event.event_type.to_owned(),
        aggregate_type: event.aggregate_type.to_owned(),
        aggregate_id: event.aggregate_id,
        payload: event.payload.clone(),
        correlation_id: None,
        causation_id: None,
    };
    let stored = store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, event_type = event.event_type, "append board event");
            ActionError::internal()
        })?;
    enqueue_deliveries(conn, ctx, event, stored.id).await
}

/// Append one board event to the outbox in the caller's transaction.
pub async fn append_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event: &BoardEvent,
) -> Result<(), ActionError> {
    append_raw_event(
        &mut *conn,
        ctx,
        &RawEvent {
            topic: format!("board:{}", event.board_id),
            board_id: Some(event.board_id),
            aggregate_type: event.aggregate_type,
            aggregate_id: event.aggregate_id,
            event_type: event.event_type,
            payload: event.payload.clone(),
            data: event.data.clone(),
            prev_data: event.prev_data.clone(),
        },
    )
    .await
}

/// One `webhook.deliver` job per matching subscription (S5-D7).
///
/// The envelope is exactly `{event, data, prevData, user}`: `event` is the registered type,
/// `data`/`prevData` are the `*Out` documents the emitter supplied, and `user` is the actor
/// `{id, name}` — looked up here, once per event, because the job must carry the name as it
/// was at emission time.
async fn enqueue_deliveries(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event: &RawEvent<'_>,
    event_id: Uuid,
) -> Result<(), ActionError> {
    let targets = store::webhook::matching_webhooks_tx(
        &mut *conn,
        ctx.tenant,
        event.board_id,
        event.event_type,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, event_type = event.event_type, "read webhook subscriptions");
        ActionError::internal()
    })?;
    if targets.is_empty() {
        return Ok(());
    }
    let name = store::identity::display_name_tx(&mut *conn, ctx.actor_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the actor's display name");
            ActionError::internal()
        })?
        .unwrap_or_default();
    let body = serde_json::json!({
        "event": event.event_type,
        "data": event.data,
        "prevData": event.prev_data,
        "user": { "id": ctx.actor_id, "name": name },
    });
    for target in targets {
        let payload = serde_json::json!({
            "subscription_id": target.id,
            "body": body,
        });
        let dedupe_key = format!("{event_id}:{}", target.id);
        store::webhook::enqueue_delivery_tx(
            &mut *conn,
            ctx.tenant,
            &dedupe_key,
            &payload,
            store::webhook::DELIVERY_MAX_ATTEMPTS,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, webhook = %target.id, "enqueue webhook delivery");
            ActionError::internal()
        })?;
    }
    Ok(())
}

/// Append the card's event to the outbox in the caller's transaction.
pub async fn card_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    record: &CardRecord,
    event_type: &'static str,
    payload: Value,
    data: Value,
    prev_data: Value,
) -> Result<(), ActionError> {
    append_event(
        &mut *conn,
        ctx,
        &BoardEvent::card(record.card.board_id, record.card.id, event_type, payload)
            .with_data(data)
            .with_prev_data(prev_data),
    )
    .await
}
