#![forbid(unsafe_code)]
//! `task_lists.*` (3) and `tasks.*` (4) — the checklists on a card.
//!
//! A checklist belongs to ONE card and an item to ONE checklist, so every action resolves
//! card → board for its event topic inside its own transaction. Positions are the same exact
//! `(max + 1024)` append / midpoint insert as every other ordered table, computed under the
//! board's advisory lock (data-arch §4).
//!
//! All seven are edits of an existing card's content, so `If-Match` is required and every
//! success carries the row's new `ETag` (CONTRACT §3). One `event` row per mutation, on
//! `board:<board_id>`: `task_list.changed` / `task.changed` with the payload S05 §5 fixes.

use conveyor_domain::Card;
use conveyor_store as store;
use conveyor_store::task::{TaskListRecord, TaskListUpdate, TaskRecord, TaskUpdate};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{BoardEvent, TASK_CHANGED, TASK_LIST_CHANGED, append_event};
use crate::board::views::{
    IdIn, TaskCreateIn, TaskListCreateIn, TaskListOut, TaskListUpdateIn, TaskMoveIn, TaskOut,
    TaskUpdateIn, task_list_out, task_out,
};
use crate::board::{etag_of, invalid, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Create a checklist on a card.
#[crate::action(
    name = "task_lists.create",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn task_lists_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TaskListCreateIn,
) -> Result<HandlerOutput<TaskListOut>, ActionError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let card = read_card(tx, ctx.tenant, input.card_id).await?;
    store::position::lock_board_tx(&mut *tx, ctx.tenant, card.board_id)
        .await
        .map_err(|error| store_error(error, "task_lists.create"))?;
    let position = match input.after_id {
        Some(after_id) => {
            store::task::position_after_task_list_tx(&mut *tx, ctx.tenant, card.id, after_id)
                .await
                .map_err(|error| store_error(error, "task_lists.create"))?
                .ok_or_else(|| ActionError::resource_not_found("such checklist on this card"))?
        }
        None => store::task::next_task_list_position_tx(&mut *tx, ctx.tenant, card.id)
            .await
            .map_err(|error| store_error(error, "task_lists.create"))?,
    };

    let created = store::task::insert_task_list_tx(
        &mut *tx,
        ctx.tenant,
        &store::task::NewTaskList {
            card_id: card.id,
            name: name.to_owned(),
            position,
        },
    )
    .await
    .map_err(|error| match error {
        store::task::TaskStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "task_lists.create")
        }
        store::task::TaskStoreError::Database(error) => store_error(error, "task_lists.create"),
    })?;

    task_list_event(
        &mut *tx,
        &ctx,
        &card,
        created.id,
        "create",
        serde_json::Value::Null,
        serde_json::to_value(task_list_out(&created, &[])).unwrap_or(serde_json::Value::Null),
    )
    .await?;
    Ok(HandlerOutput::created(task_list_out(&created, &[])).with_etag(etag_of(created.version)))
}

/// Change a checklist's name or its two display flags.
#[crate::action(
    name = "task_lists.update",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn task_lists_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TaskListUpdateIn,
) -> Result<HandlerOutput<TaskListOut>, ActionError> {
    let current = read_task_list(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(
        &ctx,
        "task_lists.update",
        serde_json::to_value(task_list_out(&current, &[])).unwrap_or(serde_json::Value::Null),
    )?;
    let card = read_card(tx, ctx.tenant, current.card_id).await?;
    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }

    let patch = TaskListUpdate {
        name: input.name.map(|name| name.trim().to_owned()),
        position: None,
        hide_completed: input.hide_completed,
        show_on_front: input.show_on_front,
    };
    match store::task::update_task_list_tx(&mut *tx, ctx.tenant, current.id, &patch, expected)
        .await
        .map_err(|error| store_error(error, "task_lists.update"))?
    {
        Some(record) => {
            let items = store::task::list_tasks_tx(&mut *tx, ctx.tenant, record.id)
                .await
                .map_err(|error| store_error(error, "task_lists.update"))?;
            task_list_event(
                &mut *tx,
                &ctx,
                &card,
                record.id,
                "update",
                serde_json::to_value(task_list_out(&current, &items))
                    .unwrap_or(serde_json::Value::Null),
                serde_json::to_value(task_list_out(&record, &items))
                    .unwrap_or(serde_json::Value::Null),
            )
            .await?;
            Ok(
                HandlerOutput::ok(task_list_out(&record, &items))
                    .with_etag(etag_of(record.version)),
            )
        }
        None => Err(stale_task_list(tx, ctx.tenant, current.id, "task_lists.update").await?),
    }
}

/// Delete a checklist and its items.
#[crate::action(
    name = "task_lists.delete",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn task_lists_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_task_list(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(
        &ctx,
        "task_lists.delete",
        serde_json::to_value(task_list_out(&current, &[])).unwrap_or(serde_json::Value::Null),
    )?;
    let card = read_card(tx, ctx.tenant, current.card_id).await?;
    let deleted = store::task::delete_task_list_tx(&mut *tx, ctx.tenant, current.id, expected)
        .await
        .map_err(|error| store_error(error, "task_lists.delete"))?;
    if !deleted {
        return Err(stale_task_list(tx, ctx.tenant, current.id, "task_lists.delete").await?);
    }
    task_list_event(
        &mut *tx,
        &ctx,
        &card,
        current.id,
        "delete",
        serde_json::to_value(task_list_out(&current, &[])).unwrap_or(serde_json::Value::Null),
        serde_json::Value::Null,
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

/// Add an item to a checklist.
#[crate::action(
    name = "tasks.create",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tasks_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TaskCreateIn,
) -> Result<HandlerOutput<TaskOut>, ActionError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let list = read_task_list(tx, ctx.tenant, input.task_list_id).await?;
    let card = read_card(tx, ctx.tenant, list.card_id).await?;
    store::position::lock_board_tx(&mut *tx, ctx.tenant, card.board_id)
        .await
        .map_err(|error| store_error(error, "tasks.create"))?;
    let position = match input.after_id {
        Some(after_id) => {
            store::task::position_after_task_tx(&mut *tx, ctx.tenant, list.id, after_id)
                .await
                .map_err(|error| store_error(error, "tasks.create"))?
                .ok_or_else(|| ActionError::resource_not_found("such item in this checklist"))?
        }
        None => store::task::next_task_position_tx(&mut *tx, ctx.tenant, list.id)
            .await
            .map_err(|error| store_error(error, "tasks.create"))?,
    };

    let created = store::task::insert_task_tx(
        &mut *tx,
        ctx.tenant,
        &store::task::NewTask {
            task_list_id: list.id,
            name: name.to_owned(),
            position,
        },
    )
    .await
    .map_err(|error| match error {
        store::task::TaskStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "tasks.create")
        }
        store::task::TaskStoreError::Database(error) => store_error(error, "tasks.create"),
    })?;

    task_event(
        &mut *tx,
        &ctx,
        &card,
        created.id,
        created.task_list_id,
        "create",
        serde_json::Value::Null,
        serde_json::to_value(task_out(&created)).unwrap_or(serde_json::Value::Null),
    )
    .await?;
    Ok(HandlerOutput::created(task_out(&created)).with_etag(etag_of(created.version)))
}

/// Change an item's name or its done flag.
#[crate::action(
    name = "tasks.update",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tasks_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TaskUpdateIn,
) -> Result<HandlerOutput<TaskOut>, ActionError> {
    let current = read_task(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(
        &ctx,
        "tasks.update",
        serde_json::to_value(task_out(&current)).unwrap_or(serde_json::Value::Null),
    )?;
    let list = read_task_list(tx, ctx.tenant, current.task_list_id).await?;
    let card = read_card(tx, ctx.tenant, list.card_id).await?;
    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }

    let patch = TaskUpdate {
        name: input.name.map(|name| name.trim().to_owned()),
        done: input.done,
    };
    match store::task::update_task_tx(&mut *tx, ctx.tenant, current.id, &patch, expected)
        .await
        .map_err(|error| store_error(error, "tasks.update"))?
    {
        Some(record) => {
            task_event(
                &mut *tx,
                &ctx,
                &card,
                record.id,
                record.task_list_id,
                "update",
                serde_json::to_value(task_out(&current)).unwrap_or(serde_json::Value::Null),
                serde_json::to_value(task_out(&record)).unwrap_or(serde_json::Value::Null),
            )
            .await?;
            Ok(HandlerOutput::ok(task_out(&record)).with_etag(etag_of(record.version)))
        }
        None => Err(stale_task(tx, ctx.tenant, current.id, "tasks.update").await?),
    }
}

/// Move an item to another checklist on the same card, or to another slot.
#[crate::action(
    name = "tasks.move",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tasks_move(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TaskMoveIn,
) -> Result<HandlerOutput<TaskOut>, ActionError> {
    let current = read_task(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(
        &ctx,
        "tasks.move",
        serde_json::to_value(task_out(&current)).unwrap_or(serde_json::Value::Null),
    )?;
    let from_list = read_task_list(tx, ctx.tenant, current.task_list_id).await?;
    let card = read_card(tx, ctx.tenant, from_list.card_id).await?;
    let to_list = read_task_list(tx, ctx.tenant, input.task_list_id).await?;
    if to_list.card_id != from_list.card_id {
        return Err(invalid(
            "task_list_id",
            "invalid_value",
            "the target checklist belongs to another card",
        ));
    }

    store::position::lock_board_tx(&mut *tx, ctx.tenant, card.board_id)
        .await
        .map_err(|error| store_error(error, "tasks.move"))?;
    let position = match input.after_id {
        Some(after_id) => {
            store::task::position_after_task_tx(&mut *tx, ctx.tenant, to_list.id, after_id)
                .await
                .map_err(|error| store_error(error, "tasks.move"))?
                .ok_or_else(|| {
                    ActionError::resource_not_found("such item in the target checklist")
                })?
        }
        None => store::task::next_task_position_tx(&mut *tx, ctx.tenant, to_list.id)
            .await
            .map_err(|error| store_error(error, "tasks.move"))?,
    };

    match store::task::move_task_tx(
        &mut *tx, ctx.tenant, current.id, to_list.id, position, expected,
    )
    .await
    .map_err(|error| store_error(error, "tasks.move"))?
    {
        Some(record) => {
            task_event(
                &mut *tx,
                &ctx,
                &card,
                record.id,
                record.task_list_id,
                "move",
                serde_json::to_value(task_out(&current)).unwrap_or(serde_json::Value::Null),
                serde_json::to_value(task_out(&record)).unwrap_or(serde_json::Value::Null),
            )
            .await?;
            Ok(HandlerOutput::ok(task_out(&record)).with_etag(etag_of(record.version)))
        }
        None => Err(stale_task(tx, ctx.tenant, current.id, "tasks.move").await?),
    }
}

/// Delete an item.
#[crate::action(
    name = "tasks.delete",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tasks_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_task(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(
        &ctx,
        "tasks.delete",
        serde_json::to_value(task_out(&current)).unwrap_or(serde_json::Value::Null),
    )?;
    let list = read_task_list(tx, ctx.tenant, current.task_list_id).await?;
    let card = read_card(tx, ctx.tenant, list.card_id).await?;
    let deleted = store::task::delete_task_tx(&mut *tx, ctx.tenant, current.id, expected)
        .await
        .map_err(|error| store_error(error, "tasks.delete"))?;
    if !deleted {
        return Err(stale_task(tx, ctx.tenant, current.id, "tasks.delete").await?);
    }
    task_event(
        &mut *tx,
        &ctx,
        &card,
        current.id,
        current.task_list_id,
        "delete",
        serde_json::to_value(task_out(&current)).unwrap_or(serde_json::Value::Null),
        serde_json::Value::Null,
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

// ── shared resolution ───────────────────────────────────────────────────────────────────

async fn read_card(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<Card, ActionError> {
    store::card::get_card_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "tasks.get"))?
        .ok_or_else(|| ActionError::resource_not_found("card"))
}

async fn read_task_list(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<TaskListRecord, ActionError> {
    store::task::get_task_list_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "tasks.get"))?
        .ok_or_else(|| ActionError::resource_not_found("task_list"))
}

async fn read_task(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<TaskRecord, ActionError> {
    store::task::get_task_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "tasks.get"))?
        .ok_or_else(|| ActionError::resource_not_found("task"))
}

async fn task_list_event(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card: &Card,
    task_list_id: Uuid,
    op: &'static str,
    prev_data: serde_json::Value,
    data: serde_json::Value,
) -> Result<(), ActionError> {
    append_event(
        &mut *tx,
        ctx,
        &BoardEvent {
            board_id: card.board_id,
            aggregate_type: "task_list",
            aggregate_id: task_list_id,
            event_type: TASK_LIST_CHANGED,
            payload: serde_json::json!({
                "task_list_id": task_list_id,
                "card_id": card.id,
                "op": op,
            }),
            data,
            prev_data,
        },
    )
    .await
}

/// Append one `task.changed` event. The two representations are separate arguments because
/// a create has no previous row and a delete has no new one; the same shape as
/// [`task_list_event`].
#[allow(clippy::too_many_arguments)]
async fn task_event(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card: &Card,
    task_id: Uuid,
    task_list_id: Uuid,
    op: &'static str,
    prev_data: serde_json::Value,
    data: serde_json::Value,
) -> Result<(), ActionError> {
    append_event(
        &mut *tx,
        ctx,
        &BoardEvent {
            board_id: card.board_id,
            aggregate_type: "task",
            aggregate_id: task_id,
            event_type: TASK_CHANGED,
            payload: serde_json::json!({
                "task_id": task_id,
                "task_list_id": task_list_id,
                "op": op,
            }),
            data,
            prev_data,
        },
    )
    .await
}

async fn stale_task_list(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::task::get_task_list_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            serde_json::to_value(task_list_out(&record, &[])).unwrap_or(serde_json::Value::Null),
            format!("the task_list moved to version {}", record.version.max(1)),
        )),
        None => Ok(ActionError::resource_not_found("task_list")),
    }
}

async fn stale_task(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::task::get_task_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            serde_json::to_value(task_out(&record)).unwrap_or(serde_json::Value::Null),
            format!("the task moved to version {}", record.version.max(1)),
        )),
        None => Ok(ActionError::resource_not_found("task")),
    }
}
