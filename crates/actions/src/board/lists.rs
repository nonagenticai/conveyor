#![forbid(unsafe_code)]
//! `lists.create` · `.update` · `.move`.
//!
//! A lane is a place, and `kind` is the only lifecycle vocabulary (CONTRACT §4). Two rules
//! are enforced here rather than in the database:
//!
//! * a `kind` edit that would leave stamped cards on a non-archive lane is `409` — the
//!   invariant `archived_at ⇒ list.kind = 'archive'` (S3-D2) cannot be broken by an update;
//! * positions are computed under the board's advisory lock and are never supplied by the
//!   caller: `after_id` names a slot, the server computes the exact midpoint (trap 2).

use conveyor_store as store;
use conveyor_store::list::{ListUpdate, NewList};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::views::ListCreateIn;
use crate::board::views::{ListMoveIn, ListOut, ListUpdateIn, list_out};
use crate::board::{invalid, parse_kind, require_precondition_with, row_version, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Create a lane, appended or immediately after `after_id`.
#[crate::action(
    name = "lists.create",
    version = 1,
    kind = "internal",
    scopes = ["lists:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lists_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ListCreateIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let kind = parse_kind(&input.kind, "kind")?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    if let Some(limit) = input.wip_limit
        && limit <= 0
    {
        return Err(invalid(
            "wip_limit",
            "invalid_value",
            "must be greater than zero",
        ));
    }
    store::board::get_board_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "lists.create"))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    store::position::lock_board_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "lists.create"))?;
    let position = match input.after_id {
        Some(after_id) => {
            store::position::position_after_list_tx(&mut *tx, ctx.tenant, input.board_id, after_id)
                .await
                .map_err(|error| store_error(error, "lists.create"))?
                .ok_or_else(|| ActionError::resource_not_found("such lane on this board"))?
        }
        None => store::position::next_list_position_tx(&mut *tx, ctx.tenant, input.board_id)
            .await
            .map_err(|error| store_error(error, "lists.create"))?,
    };

    let created = store::list::insert_list_tx(
        &mut *tx,
        ctx.tenant,
        &NewList {
            board_id: input.board_id,
            name: name.to_owned(),
            position,
            kind,
            color: input.color,
            wip_limit: input.wip_limit,
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "insert lane");
        ActionError::internal()
    })?;

    let record = store::list::get_list_record_tx(&mut *tx, ctx.tenant, created.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read new lane");
            ActionError::internal()
        })?
        .ok_or_else(ActionError::internal)?;
    Ok(HandlerOutput::created(list_out(&record)).with_etag(record.list.version.etag()))
}

/// Change a lane's name, kind, colour or WIP cap.
#[crate::action(
    name = "lists.update",
    version = 1,
    kind = "internal",
    scopes = ["lists:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lists_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ListUpdateIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let current = load_list(tx, ctx.tenant, input.id).await?;
    let current_json = serde_json::to_value(list_out(&current)).unwrap_or(serde_json::Value::Null);
    let expected = require_precondition_with(&ctx, "lists.update", current_json)?;

    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }
    if let Some(limit) = input.wip_limit
        && limit <= 0
    {
        return Err(invalid(
            "wip_limit",
            "invalid_value",
            "must be greater than zero",
        ));
    }

    let kind = match input.kind.as_deref() {
        Some(raw) => Some(parse_kind(raw, "kind")?),
        None => None,
    };
    if let Some(new_kind) = kind
        && new_kind != current.list.kind
        && new_kind != conveyor_domain::ListKind::Archive
    {
        // A stamped card on a non-archive lane breaks the one invariant the hot indexes and
        // the restore screen both lean on, so the edit is refused rather than migrated.
        let stamped = store::list::stamped_card_count_tx(&mut *tx, ctx.tenant, current.list.id)
            .await
            .map_err(|error| store_error(error, "lists.update"))?;
        if stamped > 0 {
            return Err(ActionError::new(
                409,
                "conflict",
                format!("the lane holds {stamped} archived card(s); it must stay an archive lane"),
            ));
        }
    }

    let patch = ListUpdate {
        name: input.name.map(|name| name.trim().to_owned()),
        kind,
        color: input.color,
        wip_limit: input.wip_limit,
    };
    match store::list::update_list_tx(&mut *tx, ctx.tenant, current.list.id, &patch, expected)
        .await
        .map_err(|error| {
            tracing::error!(%error, "update lane");
            ActionError::internal()
        })? {
        Some(record) => {
            Ok(HandlerOutput::ok(list_out(&record)).with_etag(record.list.version.etag()))
        }
        None => Err(stale(tx, ctx.tenant, current.list.id, "lists.update").await?),
    }
}

/// Move a lane, appended or immediately after `after_id`.
#[crate::action(
    name = "lists.move",
    version = 1,
    kind = "internal",
    scopes = ["lists:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lists_move(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ListMoveIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let current = load_list(tx, ctx.tenant, input.id).await?;
    let current_json = serde_json::to_value(list_out(&current)).unwrap_or(serde_json::Value::Null);
    let expected = require_precondition_with(&ctx, "lists.move", current_json)?;
    let board_id = current.list.board_id;

    store::position::lock_board_tx(&mut *tx, ctx.tenant, board_id)
        .await
        .map_err(|error| store_error(error, "lists.move"))?;
    let position = match input.after_id {
        Some(after_id) => {
            store::position::position_after_list_tx(&mut *tx, ctx.tenant, board_id, after_id)
                .await
                .map_err(|error| store_error(error, "lists.move"))?
                .ok_or_else(|| ActionError::resource_not_found("such lane on this board"))?
        }
        None => store::position::next_list_position_tx(&mut *tx, ctx.tenant, board_id)
            .await
            .map_err(|error| store_error(error, "lists.move"))?,
    };

    match store::list::move_list_tx(&mut *tx, ctx.tenant, current.list.id, position, expected)
        .await
        .map_err(|error| {
            tracing::error!(%error, "move lane");
            ActionError::internal()
        })? {
        Some(record) => {
            Ok(HandlerOutput::ok(list_out(&record)).with_etag(record.list.version.etag()))
        }
        None => Err(stale(tx, ctx.tenant, current.list.id, "lists.move").await?),
    }
}

async fn load_list(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<store::list::ListRecord, ActionError> {
    store::list::get_list_record_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read lane");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("lane"))
}

/// A zero-row update is either a vanished row (`404`) or a stale version (`412` with the
/// current representation) — the re-read is what tells them apart.
async fn stale(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    let current = match store::list::get_list_record_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| crate::board::list_error(error, action))?
    {
        Some(record) => record,
        None => return Ok(ActionError::resource_not_found("lane")),
    };
    let version = row_version(current.list.version.get() as i64);
    Ok(ActionError::precondition_with(
        serde_json::to_value(list_out(&current)).unwrap_or(serde_json::Value::Null),
        format!("the lane moved to version {version}"),
    ))
}
