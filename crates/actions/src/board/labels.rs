#![forbid(unsafe_code)]
//! `labels.create` · `.update` · `.delete` — the board's label vocabulary.
//!
//! A label is per-board and `(tenant_id, board_id, name)` is unique, so a duplicate name is
//! a `409 conflict` (the store's unique violation, mapped by [`crate::board::store_error`]).
//! Attaching a label to a card is `cards.add_label`/`cards.remove_label`, which live with the
//! card because they bump its version.
//!
//! Deleting a label removes every `card_label` row that names it in the same transaction: a
//! dangling join row would be invisible through the board read and would come back if the
//! name were reused.

use conveyor_store as store;
use conveyor_store::board::LabelRecord;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{BoardEvent, LABEL_CHANGED, append_event};
use crate::board::views::{IdIn, LabelCreateIn, LabelOut, LabelUpdateIn, label_out};
use crate::board::{etag_of, invalid, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Create a label on a board.
#[crate::action(
    name = "labels.create",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn labels_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LabelCreateIn,
) -> Result<HandlerOutput<LabelOut>, ActionError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let color = input.color.trim();
    if color.is_empty() {
        return Err(invalid("color", "required", "must not be empty"));
    }
    store::board::get_board_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "labels.create"))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;
    let position = store::label::next_label_position_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "labels.create"))?;

    let created = store::label::insert_label_tx(
        &mut *tx,
        ctx.tenant,
        &store::label::NewLabel {
            board_id: input.board_id,
            name: name.to_owned(),
            color: color.to_owned(),
            position,
        },
    )
    .await
    .map_err(|error| match error {
        store::label::LabelStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "labels.create")
        }
        store::label::LabelStoreError::Database(error) => store_error(error, "labels.create"),
    })?;

    label_event(
        &mut *tx,
        &ctx,
        created.board_id,
        created.id,
        "create",
        serde_json::Value::Null,
        label_json(&created),
    )
    .await?;
    Ok(HandlerOutput::created(label_out(&created)).with_etag(etag_of(created.version)))
}

/// Change a label's name, colour or position.
#[crate::action(
    name = "labels.update",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn labels_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LabelUpdateIn,
) -> Result<HandlerOutput<LabelOut>, ActionError> {
    let current = read_label(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "labels.update", label_json(&current))?;
    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }
    if let Some(color) = input.color.as_deref()
        && color.trim().is_empty()
    {
        return Err(invalid("color", "required", "must not be empty"));
    }

    let patch = store::label::LabelUpdate {
        name: input.name.map(|name| name.trim().to_owned()),
        color: input.color.map(|color| color.trim().to_owned()),
        position: input
            .position
            .and_then(|raw| rust_decimal::Decimal::try_from(raw).ok()),
    };
    match store::label::update_label_tx(&mut *tx, ctx.tenant, current.id, &patch, expected)
        .await
        .map_err(|error| store_error(error, "labels.update"))?
    {
        Some(record) => {
            label_event(
                &mut *tx,
                &ctx,
                record.board_id,
                record.id,
                "update",
                label_json(&current),
                label_json(&record),
            )
            .await?;
            Ok(HandlerOutput::ok(label_out(&record)).with_etag(etag_of(record.version)))
        }
        None => Err(stale_label(tx, ctx.tenant, current.id, "labels.update").await?),
    }
}

/// Delete a label and every card that carried it.
#[crate::action(
    name = "labels.delete",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn labels_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_label(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "labels.delete", label_json(&current))?;
    let deleted = store::label::delete_label_tx(&mut *tx, ctx.tenant, current.id, expected)
        .await
        .map_err(|error| store_error(error, "labels.delete"))?;
    if !deleted {
        return Err(stale_label(tx, ctx.tenant, current.id, "labels.delete").await?);
    }
    label_event(
        &mut *tx,
        &ctx,
        current.board_id,
        current.id,
        "delete",
        label_json(&current),
        serde_json::Value::Null,
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

fn label_json(record: &LabelRecord) -> serde_json::Value {
    serde_json::to_value(label_out(record)).unwrap_or(serde_json::Value::Null)
}

async fn read_label(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<LabelRecord, ActionError> {
    store::label::get_label_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "labels.get"))?
        .ok_or_else(|| ActionError::resource_not_found("label"))
}

async fn label_event(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    board_id: Uuid,
    label_id: Uuid,
    op: &'static str,
    prev_data: serde_json::Value,
    data: serde_json::Value,
) -> Result<(), ActionError> {
    append_event(
        &mut *tx,
        ctx,
        &BoardEvent {
            board_id,
            aggregate_type: "label",
            aggregate_id: label_id,
            event_type: LABEL_CHANGED,
            payload: serde_json::json!({
                "label_id": label_id,
                "board_id": board_id,
                "op": op,
            }),
            data,
            prev_data,
        },
    )
    .await
}

async fn stale_label(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::label::get_label_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            label_json(&record),
            format!("the label moved to version {}", record.version.max(1)),
        )),
        None => Ok(ActionError::resource_not_found("label")),
    }
}
