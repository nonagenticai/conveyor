#![forbid(unsafe_code)]
//! `cards.ledger_append` · `cards.ledger_list` — the per-card append-only claim/fact ledger.
//!
//! ADR 0008 decides the home and the shape: `card_ledger`, keyed by the conveyor card id, with
//! two kinds (`claim`, `fact`), a free-form `source` object and an optional laya `verdict`. The
//! old `planka_card_id` is not carried — nothing is copied at cutover (D7).
//!
//! The ledger is append-only by the module's surface: the append action is the only write, there
//! is no update and no delete action, and the table has no `version` because nothing edits a row.
//! An append writes exactly one `event` row in the same transaction (CONTRACT §7); the aggregate
//! is `card_ledger` because the card row itself does not change.
//!
//! Both actions resolve the card in the caller's tenant first, so another tenant's card is a
//! `404` — the same behaviour as the rest of the board module — never an empty list or an FK
//! acceptance.

use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_store as store;
use conveyor_store::ledger::{KINDS, LedgerStoreError, NewLedgerEntry};

use crate::board::events::{CARD_LEDGER_APPENDED, RawEvent, append_raw_event};
use crate::board::views::{CardLedgerAppendIn, CardLedgerListIn, LedgerEntryOut, ledger_out};
use crate::board::{invalid, parent_error, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Append one claim or fact to a card's ledger.
#[crate::action(
    name = "cards.ledger_append",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_ledger_append(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardLedgerAppendIn,
) -> Result<HandlerOutput<LedgerEntryOut>, ActionError> {
    let action = "cards.ledger_append";
    let kind = input.kind.trim();
    if !KINDS.contains(&kind) {
        return Err(invalid(
            "kind",
            "invalid_value",
            "must be one of claim, fact",
        ));
    }
    let body = input.body.trim();
    if body.is_empty() {
        return Err(invalid("body", "required", "must not be empty"));
    }
    let source = match input.source {
        None => Value::Object(serde_json::Map::new()),
        Some(value) if !value.is_object() => {
            return Err(invalid("source", "invalid_value", "must be a JSON object"));
        }
        Some(value) => value,
    };
    let verdict = match input.verdict {
        None => None,
        Some(value) if !value.is_object() => {
            return Err(invalid("verdict", "invalid_value", "must be a JSON object"));
        }
        Some(value) => Some(value),
    };

    read_card(tx, ctx.tenant, input.card_id, action).await?;

    let entry = store::ledger::append_ledger_tx(
        &mut *tx,
        ctx.tenant,
        &NewLedgerEntry {
            card_id: input.card_id,
            kind,
            body,
            source: &source,
            verdict: verdict.as_ref(),
            created_by: if ctx.actor_kind == "service" {
                None
            } else {
                Some(ctx.actor_id)
            },
        },
    )
    .await
    .map_err(|error| ledger_error(error, action))?;

    let out = ledger_out(&entry);
    append_raw_event(
        &mut *tx,
        &ctx,
        &RawEvent {
            topic: format!("card:{}", entry.card_id),
            board_id: None,
            aggregate_type: "card_ledger",
            aggregate_id: entry.id,
            event_type: CARD_LEDGER_APPENDED,
            payload: json!({
                "ledger_id": out.id,
                "card_id": out.card_id,
                "kind": out.kind,
            }),
            data: serde_json::to_value(&out).unwrap_or(Value::Null),
            prev_data: Value::Null,
        },
    )
    .await?;

    Ok(HandlerOutput::created(out))
}

/// One card's ledger, oldest first, optionally restricted to one kind.
#[crate::action(
    name = "cards.ledger_list",
    version = 1,
    kind = "internal",
    scopes = ["cards:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_ledger_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardLedgerListIn,
) -> Result<Vec<LedgerEntryOut>, ActionError> {
    let action = "cards.ledger_list";
    if let Some(kind) = input.kind.as_deref()
        && !KINDS.contains(&kind)
    {
        return Err(invalid(
            "kind",
            "invalid_value",
            "must be one of claim, fact",
        ));
    }
    read_card(tx, ctx.tenant, input.card_id, action).await?;

    let limit = store::actions::clamp_limit(input.limit);
    let rows = store::ledger::list_ledger_tx(
        &mut *tx,
        ctx.tenant,
        input.card_id,
        input.kind.as_deref(),
        limit,
    )
    .await
    .map_err(|error| ledger_error(error, action))?;
    Ok(rows.iter().map(ledger_out).collect())
}

/// A card that is not in this tenant is a `404`, before any row is written or read back.
async fn read_card(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    card_id: Uuid,
    action: &str,
) -> Result<(), ActionError> {
    let card = store::card::get_card_tx(&mut *tx, tenant, card_id)
        .await
        .map_err(|error| store_error(error, action))?;
    if card.is_none() {
        return Err(ActionError::resource_not_found("card"));
    }
    Ok(())
}

fn ledger_error(error: LedgerStoreError, action: &str) -> ActionError {
    match error {
        LedgerStoreError::Parent(parent) => parent_error(parent, action),
        LedgerStoreError::Database(error) => store_error(error, action),
    }
}
