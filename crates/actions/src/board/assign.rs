#![forbid(unsafe_code)]
//! `cards.assign` — the work package's handoff (S21 step 9).
//!
//! SPEC_V2 §6.6.1–5: a job that needs work from another job assigns the card and records what
//! the receiver must know; the card has one live root run. One action, one transaction: the
//! card names the active binding, the note becomes one hot `work_package_memory` row, and the
//! bound run starts through the same path `runs.create` uses. The note is an ordinary hot row,
//! so the next run's compiled prompt selects it.
//!
//! Refusal order — every refusal precedes the first write: a foreign card (`404`), an archived
//! card, the binding (`422 card_unassigned` / `422 job_binding_unusable`), a live root run
//! (`409 card_has_live_run`), the note (`422 invalid_input`), the andon cord (`409`).

use conveyor_domain::memory::{MemoryKind, validate_remember};
use conveyor_store as store;
use conveyor_store::card::CardUpdate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::cards::{card_json, ensure_card_binding, read_card, stale_card};
use crate::board::events::{CARD_UPDATED, card_event};
use crate::board::views::{CardOut, card_out};
use crate::board::{require_precondition_with, store_error};
use crate::control::run_create::{create_bound_run_tx, ensure_andon_tx};
use crate::control::run_error;
use crate::control::views::RunOut;
use crate::invoke::{ActionError, HandlerCtx};
use crate::memory::remember::{MemoryRememberIn, remember_tx};
use crate::memory::{MemoryOut, parse_kind, source_from_actor};
use crate::spec::HandlerOutput;

/// `cards.assign`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardAssignIn {
    /// The work package being handed off.
    pub card_id: Uuid,
    /// The job binding the card is handed to. Absent keeps the card's current binding; with
    /// neither, the handoff is refused `422 card_unassigned`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// What the receiver must know. Trimmed; empty is refused.
    pub note: String,
    /// `fact | attempt | decision | blocker` (default `fact`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Free structured detail, stored on the memory row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// Links the started run to the trigger that handed the card off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
}

/// `cards.assign`'s output: the card as it now stands, the note's memory row and the run.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardAssignOut {
    /// The card, now naming the binding.
    pub card: CardOut,
    /// The handoff note, as one hot memory row.
    pub memory: MemoryOut,
    /// The bound run that was started.
    pub run: RunOut,
}

/// Hand a card to a job: assign the binding, record what the receiver must know, start the run.
///
/// The run is created through [`create_bound_run_tx`], the same helper `runs.create` uses, so
/// the andon cord, the binding's release pin, the card's next attempt, the `run.execute` job
/// and the `run.created` event cannot drift between the two entries. The memory row is written
/// through [`remember_tx`], the same body `memory.remember` runs, with `run_id` naming the run
/// that just started.
#[crate::action(
    name = "cards.assign",
    version = 1,
    kind = "internal",
    scopes = ["cards:write", "runs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_assign(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardAssignIn,
) -> Result<HandlerOutput<CardAssignOut>, ActionError> {
    let action = "cards.assign";
    let current = read_card(&mut *tx, ctx.tenant, input.card_id).await?;
    if current.card.archived_at.is_some() {
        return Err(ActionError::new(
            409,
            "card_archived",
            "an archived card cannot be handed off; restore it first",
        ));
    }
    let expected = require_precondition_with(&ctx, action, card_json(&current))?;

    let job_binding_id = match (input.job_binding_id, current.job_binding_id) {
        (Some(requested), _) => requested,
        (None, Some(on_card)) => on_card,
        (None, None) => {
            return Err(ActionError::new(
                422,
                "card_unassigned",
                format!(
                    "card {} names no job binding and the handoff names none; a handoff needs a job",
                    current.card.id
                ),
            ));
        }
    };
    ensure_card_binding(&mut *tx, ctx.tenant, job_binding_id).await?;

    if let Some(live) = store::run::live_run_for_card_tx(&mut *tx, ctx.tenant, current.card.id)
        .await
        .map_err(|error| run_error(error, action))?
    {
        return Err(ActionError::new(
            409,
            "card_has_live_run",
            format!(
                "card {} already has a live run {} ({}); a card has one live root run at a time",
                current.card.id,
                live.id,
                live.status.as_str()
            ),
        ));
    }

    // The note is checked before any write; `remember_tx` applies the same rules when it lands.
    let kind = parse_kind(input.kind.as_deref())
        .map_err(|detail| ActionError::new(422, "invalid_input", format!("{action}: {detail}")))?
        .unwrap_or(MemoryKind::Fact);
    validate_remember(kind, source_from_actor(&ctx.actor_kind), &input.note)
        .map_err(|error| ActionError::new(422, "invalid_input", format!("{action}: {error}")))?;
    ensure_andon_tx(&mut *tx, ctx.tenant, action).await?;

    let patch = CardUpdate {
        name: None,
        description: None,
        due_at: None,
        due_done: None,
        job_binding_id: Some(job_binding_id),
    };
    let record =
        match store::card::update_card_tx(&mut *tx, ctx.tenant, current.card.id, &patch, expected)
            .await
            .map_err(|error| store_error(error, action))?
        {
            Some(record) => record,
            None => return Err(stale_card(&mut *tx, ctx.tenant, current.card.id, action).await?),
        };
    card_event(
        &mut *tx,
        &ctx,
        &record,
        CARD_UPDATED,
        serde_json::json!({ "card_id": record.card.id, "changed": ["job_binding_id"] }),
        card_json(&record),
        card_json(&current),
    )
    .await?;

    let (run_record, run) = create_bound_run_tx(
        &mut *tx,
        &ctx,
        &record.card,
        Some(job_binding_id),
        input.correlation_id,
        None,
        action,
    )
    .await?;

    let memory = remember_tx(
        &mut *tx,
        &ctx,
        MemoryRememberIn {
            card_id: record.card.id,
            body: input.note,
            kind: input.kind,
            source: None,
            run_id: Some(run_record.run.id),
            capability_id: None,
            supersedes_id: None,
            meta: input.meta,
        },
    )
    .await?;

    Ok(HandlerOutput::created(CardAssignOut {
        card: card_out(&record),
        memory,
        run,
    })
    .with_etag(record.card.version.etag()))
}
