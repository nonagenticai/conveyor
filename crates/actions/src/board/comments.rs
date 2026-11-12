#![forbid(unsafe_code)]
//! `comments.update` · `comments.delete` — the edits a comment allows.
//!
//! Creation is `cards.comment` (a card is what a comment is attached to); these two address
//! the comment itself, so their input is CONTRACT §6's `IdIn` shape plus the body. Both are
//! edits and therefore require `If-Match` (CONTRACT §3), and both carry the new `ETag` (an
//! update) or no representation at all (a delete is `204`).
//!
//! The event topic needs the comment's BOARD, which lives on the card: the helper resolves
//! card → board inside the same transaction, so an event can never name a board the caller
//! cannot see.

use conveyor_domain::Card;
use conveyor_store as store;
use conveyor_store::comment::CommentRecord;
use sqlx::PgConnection;

use crate::board::events::{BoardEvent, COMMENT_DELETED, COMMENT_UPDATED, append_event};
use crate::board::views::{CommentOut, CommentUpdateIn, IdIn, comment_out};
use crate::board::{etag_of, invalid, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Replace a comment's body.
#[crate::action(
    name = "comments.update",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn comments_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CommentUpdateIn,
) -> Result<HandlerOutput<CommentOut>, ActionError> {
    let body = input.body.trim();
    if body.is_empty() {
        return Err(invalid("body", "required", "must not be empty"));
    }
    let current = read_comment(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "comments.update", comment_json(&current))?;
    let card = read_card(tx, ctx.tenant, current.card_id).await?;

    match store::comment::update_comment_tx(&mut *tx, ctx.tenant, current.id, body, expected)
        .await
        .map_err(|error| store_error(error, "comments.update"))?
    {
        Some(comment) => {
            append_event(
                &mut *tx,
                &ctx,
                &BoardEvent {
                    board_id: card.board_id,
                    aggregate_type: "comment",
                    aggregate_id: comment.id,
                    event_type: COMMENT_UPDATED,
                    payload: serde_json::json!({
                        "comment_id": comment.id,
                        "card_id": comment.card_id,
                    }),
                    data: comment_json(&comment),
                    prev_data: comment_json(&current),
                },
            )
            .await?;
            Ok(HandlerOutput::ok(comment_out(&comment)).with_etag(etag_of(comment.version)))
        }
        None => Err(stale_comment(tx, ctx.tenant, current.id, "comments.update").await?),
    }
}

/// Delete a comment.
#[crate::action(
    name = "comments.delete",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn comments_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_comment(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "comments.delete", comment_json(&current))?;
    let card = read_card(tx, ctx.tenant, current.card_id).await?;

    let deleted = store::comment::delete_comment_tx(&mut *tx, ctx.tenant, current.id, expected)
        .await
        .map_err(|error| store_error(error, "comments.delete"))?;
    if !deleted {
        return Err(stale_comment(tx, ctx.tenant, current.id, "comments.delete").await?);
    }
    append_event(
        &mut *tx,
        &ctx,
        &BoardEvent {
            board_id: card.board_id,
            aggregate_type: "comment",
            aggregate_id: current.id,
            event_type: COMMENT_DELETED,
            payload: serde_json::json!({
                "comment_id": current.id,
                "card_id": current.card_id,
            }),
            data: comment_json(&current),
            prev_data: serde_json::Value::Null,
        },
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

fn comment_json(record: &CommentRecord) -> serde_json::Value {
    serde_json::to_value(comment_out(record)).unwrap_or(serde_json::Value::Null)
}

async fn read_comment(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: uuid::Uuid,
) -> Result<CommentRecord, ActionError> {
    store::comment::get_comment_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "comments.get"))?
        .ok_or_else(|| ActionError::resource_not_found("comment"))
}

async fn read_card(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: uuid::Uuid,
) -> Result<Card, ActionError> {
    store::card::get_card_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "comments.get"))?
        .ok_or_else(|| ActionError::resource_not_found("card"))
}

/// A zero-row edit is a vanished comment (`404`) or a stale version (`412` with the current
/// representation, CONTRACT §3).
async fn stale_comment(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: uuid::Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::comment::get_comment_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            comment_json(&record),
            format!("the comment moved to version {}", record.version.max(1)),
        )),
        None => Ok(ActionError::resource_not_found("comment")),
    }
}
