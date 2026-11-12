#![forbid(unsafe_code)]
//! `custom_fields.create` · `.update` · `.delete` · `.set_value` · `.clear_value`.
//!
//! A field belongs to ONE board (`(tenant_id, board_id, name)` is unique) and a value is one
//! row per `(card_id, custom_field_id)` — the pair the upsert leans on, so setting a value
//! twice never duplicates it.
//!
//! The KIND vocabulary is `text|number|date|select|checkbox` (data-arch §2.5). It is
//! validated here against the same list the column's `CHECK` carries, and the value's JSON
//! type must match the kind — a `number` field that accepted `"twelve"` would make every
//! later report a guess.
//!
//! Events: `custom_field.changed` on every field mutation and
//! `custom_field_value.changed` on every value write, both on `board:<board_id>`.

use conveyor_domain::Card;
use conveyor_store as store;
use conveyor_store::board::CustomFieldRecord;
use conveyor_store::custom_field::KINDS;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{
    BoardEvent, CUSTOM_FIELD_CHANGED, CUSTOM_FIELD_VALUE_CHANGED, append_event,
};
use crate::board::views::{
    CustomFieldCreateIn, CustomFieldOut, CustomFieldUpdateIn, CustomFieldValueIdIn,
    CustomFieldValueIn, CustomFieldValueOut, IdIn, custom_field_out, custom_field_value_out,
};
use crate::board::{etag_of, invalid, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Create a field on a board.
#[crate::action(
    name = "custom_fields.create",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn custom_fields_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CustomFieldCreateIn,
) -> Result<HandlerOutput<CustomFieldOut>, ActionError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let kind = parse_kind(&input.kind)?;
    let options = input.options.unwrap_or_else(|| Value::Array(Vec::new()));
    if !options.is_array() {
        return Err(invalid("options", "invalid_type", "must be an array"));
    }
    store::board::get_board_tx(&mut *tx, ctx.tenant, input.board_id)
        .await
        .map_err(|error| store_error(error, "custom_fields.create"))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    let position =
        store::custom_field::next_custom_field_position_tx(&mut *tx, ctx.tenant, input.board_id)
            .await
            .map_err(|error| store_error(error, "custom_fields.create"))?;

    let created = store::custom_field::insert_custom_field_tx(
        &mut *tx,
        ctx.tenant,
        &store::custom_field::NewCustomField {
            board_id: input.board_id,
            group_name: input.group_name.unwrap_or_default(),
            group_position: decimal(input.group_position.unwrap_or(0.0)),
            name: name.to_owned(),
            kind,
            options,
            position,
        },
    )
    .await
    .map_err(|error| match error {
        store::custom_field::CustomFieldStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "custom_fields.create")
        }
        store::custom_field::CustomFieldStoreError::Database(error) => {
            store_error(error, "custom_fields.create")
        }
    })?;

    custom_field_event(
        &mut *tx,
        &ctx,
        created.board_id,
        created.id,
        "create",
        serde_json::Value::Null,
        field_json(&created),
    )
    .await?;
    Ok(HandlerOutput::created(custom_field_out(&created)).with_etag(etag_of(created.version)))
}

/// Change a field's name, kind, group, options or position.
#[crate::action(
    name = "custom_fields.update",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn custom_fields_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CustomFieldUpdateIn,
) -> Result<HandlerOutput<CustomFieldOut>, ActionError> {
    let current = read_field(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "custom_fields.update", field_json(&current))?;
    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let kind = match input.kind.as_deref() {
        Some(raw) => Some(parse_kind(raw)?),
        None => None,
    };
    if let Some(options) = input.options.as_ref()
        && !options.is_array()
    {
        return Err(invalid("options", "invalid_type", "must be an array"));
    }

    let patch = store::custom_field::CustomFieldUpdate {
        name: input.name.map(|name| name.trim().to_owned()),
        kind,
        group_name: input.group_name,
        group_position: input.group_position.map(decimal),
        options: input.options,
        position: None,
    };
    match store::custom_field::update_custom_field_tx(
        &mut *tx, ctx.tenant, current.id, &patch, expected,
    )
    .await
    .map_err(|error| store_error(error, "custom_fields.update"))?
    {
        Some(record) => {
            custom_field_event(
                &mut *tx,
                &ctx,
                record.board_id,
                record.id,
                "update",
                field_json(&current),
                field_json(&record),
            )
            .await?;
            Ok(HandlerOutput::ok(custom_field_out(&record)).with_etag(etag_of(record.version)))
        }
        None => Err(stale_field(tx, ctx.tenant, current.id, "custom_fields.update").await?),
    }
}

/// Delete a field and every value recorded against it.
#[crate::action(
    name = "custom_fields.delete",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn custom_fields_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = read_field(tx, ctx.tenant, input.id).await?;
    let expected = require_precondition_with(&ctx, "custom_fields.delete", field_json(&current))?;
    let deleted =
        store::custom_field::delete_custom_field_tx(&mut *tx, ctx.tenant, current.id, expected)
            .await
            .map_err(|error| store_error(error, "custom_fields.delete"))?;
    if !deleted {
        return Err(stale_field(tx, ctx.tenant, current.id, "custom_fields.delete").await?);
    }
    custom_field_event(
        &mut *tx,
        &ctx,
        current.board_id,
        current.id,
        "delete",
        field_json(&current),
        serde_json::Value::Null,
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

/// Record a card's value for a board field (the `(card_id, custom_field_id)` upsert).
#[crate::action(
    name = "custom_fields.set_value",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn custom_fields_set_value(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CustomFieldValueIn,
) -> Result<HandlerOutput<CustomFieldValueOut>, ActionError> {
    let field = read_field(tx, ctx.tenant, input.custom_field_id).await?;
    let card = read_card(tx, ctx.tenant, input.card_id).await?;
    if field.board_id != card.board_id {
        return Err(invalid(
            "custom_field_id",
            "invalid_value",
            "the field belongs to another board",
        ));
    }
    validate_content(&field.kind, &input.content)?;

    let previous = store::custom_field::get_value_tx(&mut *tx, ctx.tenant, card.id, field.id)
        .await
        .map_err(|error| store_error(error, "custom_fields.set_value"))?;
    let record = store::custom_field::upsert_value_tx(
        &mut *tx,
        ctx.tenant,
        card.id,
        field.id,
        &input.content,
    )
    .await
    .map_err(|error| match error {
        store::custom_field::CustomFieldStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "custom_fields.set_value")
        }
        store::custom_field::CustomFieldStoreError::Database(error) => {
            store_error(error, "custom_fields.set_value")
        }
    })?;
    value_event(
        &mut *tx,
        &ctx,
        &card,
        record.custom_field_id,
        previous
            .as_ref()
            .map(value_json)
            .unwrap_or(serde_json::Value::Null),
        value_json(&record),
    )
    .await?;
    Ok(HandlerOutput::ok(custom_field_value_out(&record)).with_etag(etag_of(record.version)))
}

/// Remove a card's value for a board field.
#[crate::action(
    name = "custom_fields.clear_value",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn custom_fields_clear_value(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CustomFieldValueIdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let field = read_field(tx, ctx.tenant, input.custom_field_id).await?;
    let card = read_card(tx, ctx.tenant, input.card_id).await?;
    if field.board_id != card.board_id {
        return Err(invalid(
            "custom_field_id",
            "invalid_value",
            "the field belongs to another board",
        ));
    }
    let previous = store::custom_field::get_value_tx(&mut *tx, ctx.tenant, card.id, field.id)
        .await
        .map_err(|error| store_error(error, "custom_fields.clear_value"))?;
    store::custom_field::clear_value_tx(&mut *tx, ctx.tenant, card.id, field.id)
        .await
        .map_err(|error| store_error(error, "custom_fields.clear_value"))?;
    // The invocation is the mutation: the event is written even when no row existed, so a
    // webhook or SSE consumer sees the same one-event-per-invocation shape as every other
    // action (S05 §8 trap 5).
    value_event(
        &mut *tx,
        &ctx,
        &card,
        field.id,
        previous
            .as_ref()
            .map(value_json)
            .unwrap_or(serde_json::Value::Null),
        serde_json::Value::Null,
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

// ── validation and shared helpers ───────────────────────────────────────────────────────

/// The five kinds, validated against the list the column's `CHECK` carries.
fn parse_kind(raw: &str) -> Result<String, ActionError> {
    if KINDS.contains(&raw) {
        Ok(raw.to_owned())
    } else {
        Err(invalid(
            "kind",
            "invalid_value",
            "must be one of text, number, date, select, checkbox",
        ))
    }
}

/// The value's JSON type must match the field's kind (`null` clears, for every kind).
fn validate_content(kind: &str, content: &Value) -> Result<(), ActionError> {
    if content.is_null() {
        return Ok(());
    }
    let ok = match kind {
        "text" | "date" | "select" => content.is_string(),
        "number" => content.is_number(),
        "checkbox" => content.is_boolean(),
        // The database refuses an unknown kind, so this is unreachable for a stored row.
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        let expected = match kind {
            "number" => "a number",
            "checkbox" => "a boolean",
            _ => "a string",
        };
        Err(invalid(
            "content",
            "invalid_type",
            format!("a {kind} field takes {expected} (or null)"),
        ))
    }
}

/// A wire f64 as the exact decimal the column stores.
fn decimal(raw: f64) -> rust_decimal::Decimal {
    rust_decimal::Decimal::try_from(raw).unwrap_or_default()
}

fn field_json(record: &CustomFieldRecord) -> Value {
    serde_json::to_value(custom_field_out(record)).unwrap_or(Value::Null)
}

fn value_json(record: &conveyor_store::custom_field::CustomFieldValueRecord) -> Value {
    serde_json::to_value(custom_field_value_out(record)).unwrap_or(Value::Null)
}

async fn read_field(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<CustomFieldRecord, ActionError> {
    store::custom_field::get_custom_field_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "custom_fields.get"))?
        .ok_or_else(|| ActionError::resource_not_found("custom_field"))
}

async fn read_card(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<Card, ActionError> {
    store::card::get_card_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "custom_fields.get"))?
        .ok_or_else(|| ActionError::resource_not_found("card"))
}

async fn custom_field_event(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    board_id: Uuid,
    custom_field_id: Uuid,
    op: &'static str,
    prev_data: Value,
    data: Value,
) -> Result<(), ActionError> {
    append_event(
        &mut *tx,
        ctx,
        &BoardEvent {
            board_id,
            aggregate_type: "custom_field",
            aggregate_id: custom_field_id,
            event_type: CUSTOM_FIELD_CHANGED,
            payload: serde_json::json!({
                "custom_field_id": custom_field_id,
                "board_id": board_id,
                "op": op,
            }),
            data,
            prev_data,
        },
    )
    .await
}

/// `custom_field_value.changed`. The value's primary key is `(card_id, custom_field_id)` and
/// it has no surrogate id, so the CARD is the addressable aggregate instance: that is also
/// what makes the event visible to a `card:{id}` subscriber, which is the screen that cares.
async fn value_event(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card: &Card,
    custom_field_id: Uuid,
    prev_data: Value,
    data: Value,
) -> Result<(), ActionError> {
    append_event(
        &mut *tx,
        ctx,
        &BoardEvent {
            board_id: card.board_id,
            aggregate_type: "custom_field_value",
            aggregate_id: card.id,
            event_type: CUSTOM_FIELD_VALUE_CHANGED,
            payload: serde_json::json!({
                "card_id": card.id,
                "custom_field_id": custom_field_id,
            }),
            data,
            prev_data,
        },
    )
    .await
}

async fn stale_field(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::custom_field::get_custom_field_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            field_json(&record),
            format!(
                "the custom_field moved to version {}",
                record.version.max(1)
            ),
        )),
        None => Ok(ActionError::resource_not_found("custom_field")),
    }
}
