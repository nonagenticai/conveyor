#![forbid(unsafe_code)]
//! `inference_calls.list` — one page of the ledger, newest first, keyset-paged.

use chrono::{DateTime, Utc};
use conveyor_domain::inference::{CacheStatus, CallerKind};
use conveyor_store as store;
use conveyor_store::inference::{InferenceCallRow, InferenceStoreError, ListCalls};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{InferenceCallView, InferenceCallsListIn, invalid};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::Page;

#[crate::action(
    name = "inference_calls.list",
    version = 1,
    kind = "internal",
    scopes = ["inference:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn inference_calls_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: InferenceCallsListIn,
) -> Result<Page<InferenceCallView>, ActionError> {
    let caller_kind = match input.caller_kind.as_deref() {
        None => None,
        Some(raw) => Some(CallerKind::parse(raw).ok_or_else(|| {
            invalid(
                "caller_kind",
                "unknown",
                "one of copilot, process_step, workflow_step, agent_run, eval, other",
            )
        })?),
    };
    let cache_status = match input.cache_status.as_deref() {
        None => None,
        Some(raw) => Some(CacheStatus::parse(raw).ok_or_else(|| {
            invalid(
                "cache_status",
                "unknown",
                "one of hit, write, miss, unknown",
            )
        })?),
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    let before = decode_cursor(input.page.cursor.as_deref())?;
    // One extra row decides whether a next page exists; it is not returned.
    let mut rows = store::inference::list_calls_tx(
        &mut *tx,
        ctx.tenant,
        &ListCalls {
            caller_kind: caller_kind.map(|value| value.as_str().to_owned()),
            run_id: input.run_id,
            eval_run_id: input.eval_run_id,
            model: input.model,
            cache_status: cache_status.map(|value| value.as_str().to_owned()),
            since: input.since,
            before,
            limit: limit + 1,
        },
    )
    .await
    .map_err(store_error)?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(cursor_of)
    } else {
        None
    };
    Ok(Page {
        items: rows.into_iter().map(view_of).collect(),
        next_cursor,
    })
}

fn cursor_of(row: &InferenceCallRow) -> String {
    serde_json::to_string(&(row.created_at, row.id)).unwrap_or_else(|_| String::from("null"))
}

fn decode_cursor(cursor: Option<&str>) -> Result<Option<(DateTime<Utc>, Uuid)>, ActionError> {
    let Some(raw) = cursor else {
        return Ok(None);
    };
    serde_json::from_str::<(DateTime<Utc>, Uuid)>(raw)
        .map(Some)
        .map_err(|_| {
            invalid(
                "cursor",
                "invalid_cursor",
                "an inference-calls cursor is the [created_at, id] pair the last page returned",
            )
        })
}

fn view_of(row: InferenceCallRow) -> InferenceCallView {
    InferenceCallView {
        id: row.id,
        action_invocation_id: row.action_invocation_id,
        trace_id: row.trace_id,
        correlation_id: row.correlation_id,
        caller_kind: row.caller_kind,
        run_id: row.run_id,
        run_step_id: row.run_step_id,
        conversation_turn_id: row.conversation_turn_id,
        eval_run_id: row.eval_run_id,
        prompt_ref: row.prompt_ref,
        prompt_version: row.prompt_version,
        prompt_sha256: row.prompt_sha256,
        model: row.model,
        served_model: row.served_model,
        prompt_tokens: row.prompt_tokens,
        completion_tokens: row.completion_tokens,
        total_tokens: row.total_tokens,
        cache_creation_input_tokens: row.cache_creation_input_tokens,
        cache_read_input_tokens: row.cache_read_input_tokens,
        cache_status: row.cache_status,
        latency_ms: row.latency_ms,
        outcome: row.outcome,
        created_at: row.created_at,
    }
}

fn store_error(error: InferenceStoreError) -> ActionError {
    tracing::error!(%error, "read the inference ledger");
    ActionError::internal()
}
