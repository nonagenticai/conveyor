#![forbid(unsafe_code)]
//! `cards.create` · `.get` · `.update` · `.move` · `.archive` · `.unarchive` · `.duplicate`
//! · `.comment` · `.add_label` · `.remove_label`.
//!
//! The lane is the state (CONTRACT §4): `cards.archive` resolves the board's archive lane by
//! KIND, and `cards.move` asks the project-type matrix (`domain::board::may_move`) before it
//! writes. A move inside one lane — or between two lanes of the same kind — is a reorder, not
//! a transition, so it is not a matrix question; a move between kinds is.
//!
//! `cards.duplicate` copies the card, its labels, its checklists and its field values, and
//! **never its comments** (S05 §8 trap 8: a copied thread is false history).
//!
//! Every mutation writes exactly one `event` row in the same transaction (CONTRACT §7).

use chrono::Utc;
use conveyor_domain::{ArchiveError, Card, ListKind, Version};
use conveyor_store as store;
use conveyor_store::card::{CardRecord, CardUpdate, NewCard};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{
    self, BoardEvent, CARD_ARCHIVED, CARD_CREATED, CARD_MOVED, CARD_UNARCHIVED, CARD_UPDATED,
    COMMENT_CREATED,
};
use crate::board::matrix;
use crate::board::read::card_view_tx;
use crate::board::views::{
    CardCreateIn, CardDuplicateIn, CardLabelIn, CardOut, CardUpdateIn, CardView, CardsCommentIn,
    CardsListIn, CardsMoveIn, CommentOut, IdIn, card_out, comment_out,
};
use crate::board::{etag_of, invalid, require_precondition_with, row_version, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::events::card_event;

/// Create a card in a live lane.
///
/// Birth is not a transition (S5-D6): the matrix has no from-state, so any lane except the
/// archive kind is accepted. The position is appended under the board's advisory lock.
#[crate::action(
    name = "cards.create",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardCreateIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }

    let meta = match input.meta {
        None => serde_json::Value::Object(serde_json::Map::new()),
        Some(value) if !value.is_object() => {
            return Err(invalid("meta", "invalid_value", "must be a JSON object"));
        }
        Some(value) => {
            let size = serde_json::to_vec(&value)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX);
            if size > 16 * 1024 {
                return Err(invalid(
                    "meta",
                    "too_large",
                    "meta must be at most 16 KiB when serialized",
                ));
            }
            value
        }
    };

    let lane = store::list::get_list_record_tx(&mut *tx, ctx.tenant, input.list_id)
        .await
        .map_err(|error| crate::board::list_error(error, "cards.create"))?
        .ok_or_else(|| ActionError::resource_not_found("lane"))?;
    if lane.list.board_id != input.board_id {
        return Err(invalid(
            "list_id",
            "invalid_value",
            "the lane is not on the given board",
        ));
    }
    if lane.list.kind == ListKind::Archive {
        return Err(ActionError::new(
            409,
            "card-create-not-allowed",
            "a card is not born in the archive lane; create it in a live lane",
        ));
    }

    let card_type =
        store::board::get_card_type_by_code_tx(&mut *tx, ctx.tenant, &input.card_type_code)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read card type");
                ActionError::internal()
            })?
            .ok_or_else(|| {
                invalid(
                    "card_type_code",
                    "invalid_value",
                    "no card type with this code exists in this tenant",
                )
            })?;

    store::position::lock_board_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "cards.create"))?;
    let position =
        store::position::next_card_position_tx(&mut *tx, ctx.tenant, input.board_id, input.list_id)
            .await
            .map_err(|error| store_error(error, "cards.create"))?;

    let created = store::card::insert_card_tx(
        &mut *tx,
        ctx.tenant,
        &NewCard {
            list_id: input.list_id,
            card_type_id: card_type.id,
            name: name.to_owned(),
            description: input.description.unwrap_or_default(),
            position,
            parent_card_id: input.parent_card_id,
            due_at: input.due_at,
            assignee_id: None,
            created_by: if ctx.actor_kind == "service" {
                None
            } else {
                Some(ctx.actor_id)
            },
            meta,
        },
    )
    .await
    .map_err(|error| match error {
        store::card::CardStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "cards.create")
        }
        store::card::CardStoreError::ListNotFound => ActionError::resource_not_found("lane"),
        store::card::CardStoreError::ParentCardOnAnotherBoard => invalid(
            "parent_card_id",
            "invalid_value",
            "the parent card is on another board",
        ),
        store::card::CardStoreError::Database(error) => store_error(error, "cards.create"),
    })?;

    let record = read_card(tx, ctx.tenant, created.id).await?;
    card_event(
        &mut *tx,
        &ctx,
        &record,
        CARD_CREATED,
        serde_json::json!({
            "card_id": record.card.id,
            "list_id": record.card.list_id,
            "position": crate::board::views::position_f64(record.position),
        }),
        card_json(&record),
        serde_json::Value::Null,
    )
    .await?;

    Ok(HandlerOutput::created(card_out(&record)).with_etag(record.card.version.etag()))
}

/// One card, with its labels, comments, checklists and field values.
#[crate::action(
    name = "cards.get",
    version = 1,
    kind = "internal",
    scopes = ["cards:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "core"
)]
async fn cards_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<CardView, ActionError> {
    card_view_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read card view");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("card"))
}

/// One page of a board's active cards, optionally restricted to one lane.
///
/// A workflow can only call actions; `wip.pull` therefore needs a page read, not the board
/// route. The cursor is the board route's own (`position, id`), so the two cannot disagree.
#[crate::action(
    name = "cards.list",
    version = 1,
    kind = "internal",
    scopes = ["cards:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardsListIn,
) -> Result<Page<CardOut>, ActionError> {
    store::board::get_board_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "cards.list"))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(store::position::decode_cursor(raw).ok_or_else(|| {
            invalid(
                "cursor",
                "invalid_cursor",
                "not a cursor this endpoint issued",
            )
        })?),
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    // One extra row decides whether a next page exists; it is never returned.
    let mut rows = store::card::card_page_in_list_tx(
        &mut *tx,
        ctx.tenant,
        input.board_id,
        input.list_id,
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| store_error(error, "cards.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|record| store::position::encode_cursor(record.position, record.card.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(card_out).collect(),
        next_cursor,
    })
}

/// Change a card's name, description or due date.
#[crate::action(
    name = "cards.update",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardUpdateIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    let current = read_card(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "cards.update", card_json(&current))?;

    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let mut changed = Vec::new();
    if input.name.is_some() {
        changed.push("name");
    }
    if input.description.is_some() {
        changed.push("description");
    }
    if input.due_at.is_some() {
        changed.push("due_at");
    }
    if input.due_done.is_some() {
        changed.push("due_done");
    }

    let patch = CardUpdate {
        name: input.name.map(|name| name.trim().to_owned()),
        description: input.description,
        due_at: input.due_at,
        due_done: input.due_done,
    };
    match store::card::update_card_tx(&mut *tx, ctx.tenant, current.card.id, &patch, expected)
        .await
        .map_err(|error| store_error(error, "cards.update"))?
    {
        Some(record) => {
            card_event(
                &mut *tx,
                &ctx,
                &record,
                CARD_UPDATED,
                serde_json::json!({ "card_id": record.card.id, "changed": changed }),
                card_json(&record),
                card_json(&current),
            )
            .await?;
            Ok(HandlerOutput::ok(card_out(&record)).with_etag(record.card.version.etag()))
        }
        None => Err(stale_card(tx, ctx.tenant, current.card.id, "cards.update").await?),
    }
}

/// Move a card to another lane (or another slot) on the same board.
///
/// The move is legal only when `domain::board::may_move` says so: the project declares a
/// `project_type`, that type's matrix has an entry for the card's type and the lane being
/// left, and the target kind is in it. A move that keeps the kind (a reorder inside a lane,
/// or between two lanes of one kind) is not a transition and is not a matrix question; a
/// stamped card is refused either way — it is restored, then moved.
#[crate::action(
    name = "cards.move",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant",
    // S10 §5.3: a copilot may not move a card on its own initiative. `effect = write` and
    // `confirm = none` alone would execute it; the hint is what turns the tool call into a
    // proposal a person decides.
    agent_tier = "core",
    proposal_required = true
)]
async fn cards_move(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardsMoveIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    let current = read_card(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "cards.move", card_json(&current))?;
    if current.card.archived_at.is_some() {
        return Err(matrix::refused(
            "an archived card must be restored before it can move",
        ));
    }

    let from = store::list::get_list_record_tx(&mut *tx, ctx.tenant, current.card.list_id)
        .await
        .map_err(|error| crate::board::list_error(error, "cards.move"))?
        .ok_or_else(|| ActionError::resource_not_found("lane"))?;
    let to = store::list::get_list_record_tx(&mut *tx, ctx.tenant, input.list_id)
        .await
        .map_err(|error| crate::board::list_error(error, "cards.move"))?
        .ok_or_else(|| ActionError::resource_not_found("target lane"))?;
    if to.list.board_id != current.card.board_id {
        return Err(matrix::refused(
            "the target lane is on another board; a card cannot change board by moving",
        ));
    }

    if from.list.kind != to.list.kind {
        let project_type_code = store::board::project_type_code_for_board_tx(
            &mut *tx,
            ctx.tenant,
            current.card.board_id,
        )
        .await
        .map_err(|error| store_error(error, "cards.move"))?
        .ok_or_else(|| {
            matrix::refused("the project declares no project_type, so no move is legal")
        })?;
        let card_type =
            store::board::get_card_type_tx(&mut *tx, ctx.tenant, current.card.card_type_id)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "read card type");
                    ActionError::internal()
                })?
                .ok_or_else(|| ActionError::resource_not_found("card type"))?;
        let rules = matrix::rules_tx(&mut *tx, ctx.tenant, &card_type, &project_type_code).await?;
        matrix::ensure_move(&current.card, from.list.kind, to.list.kind, &rules)?;
    }

    store::position::lock_board_tx(&mut *tx, ctx.tenant, current.card.board_id)
        .await
        .map_err(|error| store_error(error, "cards.move"))?;
    let position = match input.after_id {
        Some(after_id) => store::position::position_after_card_tx(
            &mut *tx,
            ctx.tenant,
            current.card.board_id,
            input.list_id,
            after_id,
        )
        .await
        .map_err(|error| store_error(error, "cards.move"))?
        .ok_or_else(|| {
            ActionError::resource_not_found("such card in the target lane to land after")
        })?,
        None => store::position::next_card_position_tx(
            &mut *tx,
            ctx.tenant,
            current.card.board_id,
            input.list_id,
        )
        .await
        .map_err(|error| store_error(error, "cards.move"))?,
    };

    match store::card::move_card_tx(
        &mut *tx,
        ctx.tenant,
        current.card.id,
        input.list_id,
        position,
        expected,
    )
    .await
    .map_err(|error| match error {
        store::card::CardStoreError::Database(error) => store_error(error, "cards.move"),
        store::card::CardStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "cards.move")
        }
        store::card::CardStoreError::ListNotFound => ActionError::resource_not_found("target lane"),
        store::card::CardStoreError::ParentCardOnAnotherBoard => {
            matrix::refused("the target lane is on another board")
        }
    })? {
        Some(_) => {
            // `move_card_tx` answers with the domain value; the read record carries the
            // content columns and the card-type code the wire document needs.
            let record = read_card(tx, ctx.tenant, current.card.id).await?;
            card_event(
                &mut *tx,
                &ctx,
                &record,
                CARD_MOVED,
                serde_json::json!({
                    "card_id": record.card.id,
                    "from_list_id": from.list.id,
                    "to_list_id": record.card.list_id,
                    "position": crate::board::views::position_f64(record.position),
                }),
                card_json(&record),
                card_json(&current),
            )
            .await?;
            Ok(HandlerOutput::ok(card_out(&record)).with_etag(record.card.version.etag()))
        }
        None => Err(stale_card(tx, ctx.tenant, current.card.id, "cards.move").await?),
    }
}

/// Archive a card: place it in the board's archive lane and stamp it.
///
/// CONTRACT §3 names `cards.archive` among the edits that require `If-Match`; the input is
/// CONTRACT §6's `IdIn`, so the precondition comes from the header (or `_version`).
#[crate::action(
    name = "cards.archive",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_archive(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_card(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "cards.archive", card_json(&current))?;

    let archive = store::list::list_of_kind_tx(
        &mut *tx,
        ctx.tenant,
        current.card.board_id,
        ListKind::Archive,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "read archive lane");
        ActionError::internal()
    })?
    .ok_or_else(|| {
        ActionError::new(
            409,
            "card-archive-not-allowed",
            "the board has no archive lane",
        )
    })?;

    let stamped = stamp(&current.card, &archive.list)?;
    match store::card::apply_card_transition_tx(&mut *tx, ctx.tenant, &stamped, expected)
        .await
        .map_err(|error| store_error(error, "cards.archive"))?
    {
        Some(record) => {
            card_event(
                &mut *tx,
                &ctx,
                &record,
                CARD_ARCHIVED,
                serde_json::json!({ "card_id": record.card.id, "list_id": record.card.list_id }),
                card_json(&record),
                card_json(&current),
            )
            .await?;
            Ok(HandlerOutput::no_content(()))
        }
        None => Err(stale_card(tx, ctx.tenant, current.card.id, "cards.archive").await?),
    }
}

/// Restore a stamped card to the board's intake lane.
///
/// The destination is the intake lane, and the move must be legal for the card's type
/// (`archive → intake` in the project type's matrix). An absent matrix entry is the same
/// `409 card-move-not-allowed` a drag would get.
#[crate::action(
    name = "cards.unarchive",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_unarchive(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    let current = read_card(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "cards.unarchive", card_json(&current))?;
    if current.card.archived_at.is_none() {
        return Err(ActionError::new(
            409,
            "card-unarchive-not-allowed",
            "the card is not archived",
        ));
    }

    let from = store::list::get_list_record_tx(&mut *tx, ctx.tenant, current.card.list_id)
        .await
        .map_err(|error| crate::board::list_error(error, "cards.unarchive"))?
        .ok_or_else(|| ActionError::resource_not_found("lane"))?;
    let intake = store::list::list_of_kind_tx(
        &mut *tx,
        ctx.tenant,
        current.card.board_id,
        ListKind::Intake,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "read intake lane");
        ActionError::internal()
    })?
    .ok_or_else(|| {
        ActionError::new(
            409,
            "card-unarchive-not-allowed",
            "the board has no intake lane to restore to",
        )
    })?;

    let project_type_code =
        store::board::project_type_code_for_board_tx(&mut *tx, ctx.tenant, current.card.board_id)
            .await
            .map_err(|error| store_error(error, "cards.unarchive"))?
            .ok_or_else(|| {
                ActionError::new(
                    409,
                    matrix::MOVE_REFUSED,
                    "the project declares no project_type, so no restore is legal",
                )
            })?;
    let card_type = store::board::get_card_type_tx(&mut *tx, ctx.tenant, current.card.card_type_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read card type");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("card type"))?;
    let rules = matrix::rules_tx(&mut *tx, ctx.tenant, &card_type, &project_type_code).await?;

    let restored = current
        .card
        .clone()
        .restore(&from.list, &intake.list, &rules, Utc::now())
        .map_err(map_archive_error)?;
    match store::card::apply_card_transition_tx(&mut *tx, ctx.tenant, &restored, expected)
        .await
        .map_err(|error| store_error(error, "cards.unarchive"))?
    {
        Some(record) => {
            card_event(
                &mut *tx,
                &ctx,
                &record,
                CARD_UNARCHIVED,
                serde_json::json!({ "card_id": record.card.id, "list_id": record.card.list_id }),
                card_json(&record),
                card_json(&current),
            )
            .await?;
            Ok(HandlerOutput::ok(card_out(&record)).with_etag(record.card.version.etag()))
        }
        None => Err(stale_card(tx, ctx.tenant, current.card.id, "cards.unarchive").await?),
    }
}

/// Copy a card, its labels, its checklists and its field values — never its comments.
///
/// The copy is born (not moved): the same rule as `cards.create` applies, so a target lane
/// of the archive kind is refused. `list_id` absent means "the source card's lane".
#[crate::action(
    name = "cards.duplicate",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_duplicate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardDuplicateIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    let source = read_card(tx, ctx.tenant, input.id).await?;
    let target_list_id = input.list_id.unwrap_or(source.card.list_id);
    let lane = store::list::get_list_record_tx(&mut *tx, ctx.tenant, target_list_id)
        .await
        .map_err(|error| crate::board::list_error(error, "cards.duplicate"))?
        .ok_or_else(|| ActionError::resource_not_found("target lane"))?;
    if lane.list.board_id != source.card.board_id {
        return Err(invalid(
            "list_id",
            "invalid_value",
            "the lane is not on the card's board",
        ));
    }
    if lane.list.kind == ListKind::Archive {
        return Err(ActionError::new(
            409,
            "card-create-not-allowed",
            "a copy is a birth; the archive lane cannot receive one",
        ));
    }

    let name = match input.name.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(invalid("name", "required", "must not be empty"));
            }
            trimmed.to_owned()
        }
        None => source.name.clone(),
    };

    store::position::lock_board_tx(&mut *tx, ctx.tenant, source.card.board_id)
        .await
        .map_err(|error| store_error(error, "cards.duplicate"))?;
    let position = store::position::next_card_position_tx(
        &mut *tx,
        ctx.tenant,
        source.card.board_id,
        target_list_id,
    )
    .await
    .map_err(|error| store_error(error, "cards.duplicate"))?;

    let created = store::card::insert_card_tx(
        &mut *tx,
        ctx.tenant,
        &NewCard {
            list_id: target_list_id,
            card_type_id: source.card.card_type_id,
            name,
            description: source.description.clone(),
            position,
            parent_card_id: source.parent_card_id,
            due_at: source.due_at,
            // A claim is not content: the copy is unclaimed (CONTRACT §4, S5-D8).
            assignee_id: None,
            created_by: if ctx.actor_kind == "service" {
                None
            } else {
                Some(ctx.actor_id)
            },
            meta: source.meta.clone(),
        },
    )
    .await
    .map_err(|error| match error {
        store::card::CardStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "cards.duplicate")
        }
        store::card::CardStoreError::ListNotFound => ActionError::resource_not_found("target lane"),
        store::card::CardStoreError::ParentCardOnAnotherBoard => invalid(
            "id",
            "invalid_value",
            "the source card's parent is on another board",
        ),
        store::card::CardStoreError::Database(error) => store_error(error, "cards.duplicate"),
    })?;

    store::label::copy_card_labels_tx(&mut *tx, ctx.tenant, source.card.id, created.id)
        .await
        .map_err(|error| store_error(error, "cards.duplicate"))?;
    store::custom_field::copy_values_tx(&mut *tx, ctx.tenant, source.card.id, created.id)
        .await
        .map_err(|error| store_error(error, "cards.duplicate"))?;
    store::task::copy_task_lists_tx(&mut *tx, ctx.tenant, source.card.id, created.id)
        .await
        .map_err(|error| store_error(error, "cards.duplicate"))?;
    // Deliberately absent: comments (S05 §8 trap 8). A copied thread is false history.

    let record = read_card(tx, ctx.tenant, created.id).await?;
    card_event(
        &mut *tx,
        &ctx,
        &record,
        CARD_CREATED,
        serde_json::json!({
            "card_id": record.card.id,
            "list_id": record.card.list_id,
            "position": crate::board::views::position_f64(record.position),
            "duplicated_from": source.card.id,
        }),
        card_json(&record),
        serde_json::Value::Null,
    )
    .await?;

    Ok(HandlerOutput::created(card_out(&record)).with_etag(record.card.version.etag()))
}

/// Comment on a card.
#[crate::action(
    name = "cards.comment",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_comment(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardsCommentIn,
) -> Result<HandlerOutput<CommentOut>, ActionError> {
    let body = input.body.trim();
    if body.is_empty() {
        return Err(invalid("body", "required", "must not be empty"));
    }
    let card = read_card(tx, ctx.tenant, input.id).await?;
    let comment = store::comment::insert_comment_tx(
        &mut *tx,
        ctx.tenant,
        &store::comment::NewComment {
            card_id: card.card.id,
            // A workflow/process node runs as a service principal (`actor_id` nil): it has no
            // `app_user` row, and the parent check would refuse the comment. The column is
            // nullable by design, so a service comment records no author; a user or agent keeps
            // the authored path and its parent check.
            author_id: if ctx.actor_kind == "service" {
                None
            } else {
                Some(ctx.actor_id)
            },
            body: body.to_owned(),
        },
    )
    .await
    .map_err(|error| match error {
        store::comment::CommentStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "cards.comment")
        }
        store::comment::CommentStoreError::Database(error) => store_error(error, "cards.comment"),
    })?;

    events::append_event(
        &mut *tx,
        &ctx,
        &BoardEvent {
            board_id: card.card.board_id,
            aggregate_type: "comment",
            aggregate_id: comment.id,
            event_type: COMMENT_CREATED,
            payload: serde_json::json!({
                "comment_id": comment.id,
                "card_id": card.card.id,
            }),
            data: serde_json::to_value(comment_out(&comment)).unwrap_or(serde_json::Value::Null),
            prev_data: serde_json::Value::Null,
        },
    )
    .await?;

    Ok(HandlerOutput::created(comment_out(&comment)).with_etag(etag_of(comment.version)))
}

/// Label a card.
///
/// The label must be on the card's board. The card's `version` is bumped because `CardView`
/// carries its labels: the `ETag` addresses the representation, and the representation just
/// changed (CONTRACT §3).
#[crate::action(
    name = "cards.add_label",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_add_label(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardLabelIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    label_card(tx, ctx, input, true).await
}

/// Unlabel a card.
#[crate::action(
    name = "cards.remove_label",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_remove_label(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardLabelIn,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    label_card(tx, ctx, input, false).await
}

async fn label_card(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardLabelIn,
    add: bool,
) -> Result<HandlerOutput<CardOut>, ActionError> {
    let action = if add {
        "cards.add_label"
    } else {
        "cards.remove_label"
    };
    let current = read_card(tx, ctx.tenant, input.card_id).await?;
    let expected = require_precondition_with(&ctx, action, card_json(&current))?;

    let label = store::label::get_label_tx(&mut *tx, ctx.tenant, input.label_id)
        .await
        .map_err(|error| store_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("label"))?;
    if label.board_id != current.card.board_id {
        return Err(invalid(
            "label_id",
            "invalid_value",
            "the label belongs to another board",
        ));
    }

    let bumped = store::card::bump_card_version_tx(&mut *tx, ctx.tenant, current.card.id, expected)
        .await
        .map_err(|error| store_error(error, action))?;
    let Some(record) = bumped else {
        return Err(stale_card(tx, ctx.tenant, current.card.id, action).await?);
    };

    if add {
        store::label::add_card_label_tx(&mut *tx, ctx.tenant, record.card.id, label.id)
            .await
            .map_err(|error| store_error(error, action))?;
    } else {
        store::label::remove_card_label_tx(&mut *tx, ctx.tenant, record.card.id, label.id)
            .await
            .map_err(|error| store_error(error, action))?;
    }

    card_event(
        &mut *tx,
        &ctx,
        &record,
        CARD_UPDATED,
        serde_json::json!({ "card_id": record.card.id, "changed": ["labels"] }),
        card_json(&record),
        card_json(&current),
    )
    .await?;

    Ok(HandlerOutput::ok(card_out(&record)).with_etag(record.card.version.etag()))
}

fn stamp(card: &Card, archive: &conveyor_domain::List) -> Result<Card, ActionError> {
    card.clone().archive(archive, Utc::now()).map_err(|error| {
        let detail = match &error {
            ArchiveError::AlreadyArchived => "the card is already archived".to_owned(),
            other => other.to_string(),
        };
        ActionError::new(409, "card-archive-not-allowed", detail)
    })
}

fn map_archive_error(error: ArchiveError) -> ActionError {
    match error {
        ArchiveError::NotArchived => ActionError::new(
            409,
            "card-unarchive-not-allowed",
            "the card is not archived",
        ),
        ArchiveError::NotArchiveList => ActionError::new(
            409,
            "card-unarchive-not-allowed",
            "the card does not sit in an archive lane",
        ),
        ArchiveError::AlreadyArchived => ActionError::new(
            409,
            "card-unarchive-not-allowed",
            "the card is already archived",
        ),
        ArchiveError::Move(move_error) => matrix::refused(move_error.to_string()),
    }
}

fn card_json(record: &CardRecord) -> serde_json::Value {
    serde_json::to_value(card_out(record)).unwrap_or(serde_json::Value::Null)
}

async fn read_card(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<CardRecord, ActionError> {
    store::card::get_card_record_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "cards.get"))?
        .ok_or_else(|| ActionError::resource_not_found("card"))
}

/// A zero-row edit is either a vanished card (`404`) or a stale version (`412` with the
/// current representation, CONTRACT §3).
async fn stale_card(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    let current = match store::card::get_card_record_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => record,
        None => return Ok(ActionError::resource_not_found("card")),
    };
    let version: Version = row_version(current.card.version.get() as i64);
    Ok(ActionError::precondition_with(
        card_json(&current),
        format!("the card moved to version {version}"),
    ))
}
