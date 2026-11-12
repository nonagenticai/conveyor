#![forbid(unsafe_code)]
//! `memory.remember` — one hot work-package memory row, written in the caller's transaction.
//!
//! The row is the record; a consolidation later derives a digest from it. The event
//! `memory.recorded` is appended in the same transaction, so a subscriber never sees a memory
//! that the transaction rolled back.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_domain::memory::{MemoryKind, validate_remember};
use conveyor_store::memory::{MemoryStoreError, NewMemory, insert_memory_tx};
use conveyor_store::parent::ParentError;

use crate::board::events::{MEMORY_RECORDED, RawEvent, append_raw_event};
use crate::invoke::{ActionError, HandlerCtx};
use crate::memory::{MemoryOut, meta_or_empty, parse_kind, parse_source, source_from_actor};
use crate::spec::HandlerOutput;

/// `memory.remember`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRememberIn {
    /// The work package (the card) the memory belongs to.
    pub card_id: Uuid,
    /// The text. Trimmed; empty is refused.
    pub body: String,
    /// `fact | attempt | decision | blocker` (default `fact`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// `agent | human | system` (default: the caller's actor kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The run that learned it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The capability the run executed under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The row this one supersedes (same card).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_id: Option<Uuid>,
    /// Free structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Write one memory row.
#[crate::action(
    name = "memory.remember",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = false,
    timeout_ms = 15000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn memory_remember(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MemoryRememberIn,
) -> Result<HandlerOutput<MemoryOut>, ActionError> {
    let action = "memory.remember";
    let kind = parse_kind(input.kind.as_deref())
        .map_err(|detail| ActionError::new(422, "invalid_input", format!("{action}: {detail}")))?
        .unwrap_or(MemoryKind::Fact);
    let source = parse_source(input.source.as_deref())
        .map_err(|detail| ActionError::new(422, "invalid_input", format!("{action}: {detail}")))?
        .unwrap_or_else(|| source_from_actor(&ctx.actor_kind));

    validate_remember(kind, source, &input.body)
        .map_err(|error| ActionError::new(422, "invalid_input", format!("{action}: {error}")))?;

    let record = insert_memory_tx(
        &mut *tx,
        ctx.tenant,
        &NewMemory {
            card_id: input.card_id,
            run_id: input.run_id,
            capability_id: input.capability_id,
            kind,
            source,
            body: &input.body,
            supersedes_id: input.supersedes_id,
            meta: meta_or_empty(input.meta),
        },
    )
    .await
    .map_err(|error| memory_error(error, action))?;

    let out = MemoryOut::from(record);
    append_raw_event(
        &mut *tx,
        &ctx,
        &RawEvent {
            topic: format!("card:{}", out.card_id),
            board_id: None,
            aggregate_type: "work_package_memory",
            aggregate_id: out.id,
            event_type: MEMORY_RECORDED,
            payload: json!({
                "memory_id": out.id,
                "card_id": out.card_id,
                "kind": out.kind,
                "source": out.source,
            }),
            data: serde_json::to_value(&out).unwrap_or(Value::Null),
            prev_data: Value::Null,
        },
    )
    .await?;

    Ok(HandlerOutput::created(out))
}

fn memory_error(error: MemoryStoreError, action: &str) -> ActionError {
    match error {
        MemoryStoreError::NotFound => ActionError::resource_not_found("such memory row"),
        MemoryStoreError::SupersedesOtherCard => ActionError::new(
            422,
            "invalid_input",
            format!("{action}: a memory row can only supersede a row of the same card"),
        ),
        MemoryStoreError::Parent(ParentError::NotFound { .. }) => {
            ActionError::resource_not_found("such card, run or capability")
        }
        MemoryStoreError::Database(error) => {
            tracing::error!(%error, "memory.remember: database write failed");
            ActionError::internal()
        }
        other => ActionError::new(500, "internal", format!("{action}: {other}")),
    }
}
