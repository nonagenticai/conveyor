#![forbid(unsafe_code)]
//! `capabilities.list` · `.get` · `.bind_knowledge`.
//!
//! Reads never widen what a caller may see: every query is tenant-scoped by the store and RLS is
//! forced on all five capability tables, so another tenant's row is a `404`, not a hidden hit.

use sqlx::PgConnection;

use conveyor_store as store;

use crate::board::events::{KNOWLEDGE_BOUND, RawEvent, append_raw_event};
use crate::capability::{
    BindKnowledgeIn, CapabilityDetailOut, CapabilityGetIn, CapabilityListIn, CapabilityOut,
    KnowledgeOut, capability_out, decode_key_cursor, invalid, key_cursor, knowledge_out,
    parse_min_score, parse_status, store_error,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// One page of capabilities, ordered by code.
#[crate::action(
    name = "capabilities.list",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 15000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capabilities_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CapabilityListIn,
) -> Result<Page<CapabilityOut>, ActionError> {
    let status = input.status.as_deref().map(parse_status).transpose()?;
    let cursor = decode_key_cursor(input.page.cursor.as_deref(), "capabilities")?;
    let limit = store::actions::clamp_limit(input.page.limit);
    // One extra row decides whether a next page exists; it is not returned and the store does not
    // clamp on its own (a hidden clamp is a silent truncation).
    let mut rows = store::capability::list_capabilities_tx(
        &mut *tx,
        ctx.tenant,
        status.map(|status| status.as_str()),
        cursor.as_deref(),
        limit + 1,
    )
    .await
    .map_err(|error| store_error(error, "capabilities.list", "capability"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(|row| key_cursor(&row.code))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(capability_out).collect(),
        next_cursor,
    })
}

/// One capability with its knowledge bindings, by id or by code.
#[crate::action(
    name = "capabilities.get",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capabilities_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CapabilityGetIn,
) -> Result<CapabilityDetailOut, ActionError> {
    let action = "capabilities.get";
    let record = match (input.capability_id, input.code.as_deref()) {
        (Some(id), None) => store::capability::get_capability_tx(&mut *tx, ctx.tenant, id)
            .await
            .map_err(|error| store_error(error, action, "capability"))?
            .ok_or_else(|| ActionError::resource_not_found("capability"))?,
        (None, Some(code)) => {
            store::capability::get_capability_by_code_tx(&mut *tx, ctx.tenant, code.trim())
                .await
                .map_err(|error| store_error(error, action, "capability"))?
                .ok_or_else(|| ActionError::resource_not_found("capability"))?
        }
        (Some(_), Some(_)) => {
            return Err(invalid(
                "code",
                "invalid_value",
                "name either capability_id or code, not both",
            ));
        }
        (None, None) => {
            return Err(invalid(
                "capability_id",
                "required",
                "name a capability_id or a code",
            ));
        }
    };
    let readiness =
        super::readiness::capability_readiness_tx(&mut *tx, ctx.tenant, record.id, &[]).await?;
    let knowledge = store::capability::packs_for_capability_tx(&mut *tx, ctx.tenant, record.id)
        .await
        .map_err(|error| store_error(error, action, "capability"))?;
    Ok(CapabilityDetailOut {
        capability: capability_out(&record),
        knowledge: knowledge.iter().map(knowledge_out).collect(),
        ready: readiness.is_ready(),
        blocking: super::readiness::readiness_fields(&readiness),
    })
}

/// Pin a strata pack to a capability. The binding is an upsert on `(capability, pack, version)`;
/// `knowledge.bound` is appended in the same transaction, so a subscriber never sees a binding the
/// transaction rolled back.
#[crate::action(
    name = "capabilities.bind_knowledge",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 15000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capabilities_bind_knowledge(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindKnowledgeIn,
) -> Result<HandlerOutput<KnowledgeOut>, ActionError> {
    let action = "capabilities.bind_knowledge";
    let min_score = match input.min_score {
        Some(raw) => Some(parse_min_score(raw)?),
        None => None,
    };
    let record = store::capability::bind_knowledge_tx(
        &mut *tx,
        ctx.tenant,
        input.capability_id,
        input.pack_key.trim(),
        input.pack_version,
        min_score,
        input.required,
    )
    .await
    .map_err(|error| store_error(error, action, "capability"))?;

    let out = knowledge_out(&record);
    append_raw_event(
        &mut *tx,
        &ctx,
        &RawEvent {
            topic: format!("capability:{}", record.capability_id),
            board_id: None,
            aggregate_type: "capability",
            aggregate_id: record.capability_id,
            event_type: KNOWLEDGE_BOUND,
            payload: serde_json::json!({
                "capability_id": record.capability_id,
                "pack_key": record.pack_key,
                "pack_version": record.pack_version,
                "required": record.required,
                "min_score": out.min_score,
            }),
            data: serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
            prev_data: serde_json::Value::Null,
        },
    )
    .await?;

    Ok(HandlerOutput::ok(out))
}
