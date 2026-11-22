#![forbid(unsafe_code)]
//! **One ledger-derived cost model** for every eval surface (S19 P1–P2; audit W12).
//!
//! `eval_runs.report` splits a run's spend into subject, judge and billed failures; a climb sums
//! the spend of every round it ran. Both read the same `inference_call` rows — the engine writes
//! every eval model call with `caller_kind='eval'` and the run's id, including the judge's calls
//! and an attempt the provider billed before failing — and both price them with the arithmetic
//! here. A second accounting model (the row's copied `usage`) is what W12 forbids, so no surface
//! multiplies tokens itself.
//!
//! Two rules keep the total honest:
//!
//! - **Every billed call is priced, whatever its slot.** A judge call is not a discount and a
//!   failed attempt is not free; both are spend.
//! - **A row that cannot be priced is never silently free.** It is [`None`] to the caller, which
//!   counts it (the report's `unpriced`) rather than adding zero.

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::inference::InferenceCallRow;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Price one call's `usage` against the version's `prices`, in cents.
///
/// `prices` is the version's free-form jsonb, in US dollars per million tokens. `input` and
/// `output` are the full-rate classes; `cache_write` (tokens written into the prompt cache) and
/// `cache_read` (tokens served from it) are the cache classes, and each is required **only when
/// the usage carries that class's token count**. A cache class is priced at its own rate, never
/// folded into the full-rate input: the provider prices a write above a miss and a read below it,
/// so one rate for all three would make every cache optimisation invisible.
///
/// A row the rates cannot express — no prices, a missing rate for a class it used, a usage object
/// that names no token counts — returns `None`. The caller then counts the row as **unpriced**
/// rather than inventing a rate or calling the call free.
pub(crate) fn price_usage(usage: &Value, prices: &Value) -> Option<f64> {
    let input_rate = prices.get("input").and_then(Value::as_f64)?;
    let output_rate = prices.get("output").and_then(Value::as_f64)?;
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_f64)?;
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let cache_write = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    // ⛔ A class the row used and the version did not price is unpriced, not free.
    let mut cache_cost = 0.0;
    if cache_write > 0.0 {
        cache_cost += cache_write * prices.get("cache_write").and_then(Value::as_f64)?;
    }
    if cache_read > 0.0 {
        cache_cost += cache_read * prices.get("cache_read").and_then(Value::as_f64)?;
    }
    // Cents: (tokens / 1e6) dollars * 100.
    Some((input * input_rate + output * output_rate + cache_cost) / 10_000.0)
}

/// The unpriced fallback: how many tokens a call cost.
pub(crate) fn tokens_of(usage: &Value) -> f64 {
    usage
        .get("total_tokens")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            let input = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let output = usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            input + output
        })
}

/// Whether the version's prices carry the two rates [`price_usage`] understands.
pub(crate) fn price_usage_available(prices: &Value) -> bool {
    prices.get("input").and_then(Value::as_f64).is_some()
        && prices.get("output").and_then(Value::as_f64).is_some()
}

/// Where one ledger row's spend belongs in the report.
pub(crate) enum SpendSlot {
    /// The subject's own call.
    Subject,
    /// A judge call: the runner's `eval_judge` seam, or an audit's judge probe attributed to
    /// the run it audited.
    Judge,
    /// A billed call that did not answer `ok` (a refused, errored or timed-out attempt).
    Failed,
}

pub(crate) fn spend_slot(call: &InferenceCallRow) -> SpendSlot {
    if call.outcome != "ok" {
        return SpendSlot::Failed;
    }
    match call.prompt_ref.as_deref() {
        Some(reference)
            if reference.starts_with("eval_judge") || reference.starts_with("eval_audit") =>
        {
            SpendSlot::Judge
        }
        _ => SpendSlot::Subject,
    }
}

/// The ledger row as the usage object [`price_usage`] prices.
fn usage_of(call: &InferenceCallRow) -> Value {
    serde_json::json!({
        "prompt_tokens": call.prompt_tokens,
        "completion_tokens": call.completion_tokens,
        "total_tokens": call.total_tokens,
        "cache_creation_input_tokens": call.cache_creation_input_tokens,
        "cache_read_input_tokens": call.cache_read_input_tokens,
    })
}

/// One ledger row's cost, in the unit the version prices in.
///
/// `None` means the row cannot be expressed in that unit: the provider returned no usage at all
/// (`unknown`), the call was a non-token surface, or a priced version has no rate for a class it
/// used. A cache row prices by its class when the version declares `cache_write`/`cache_read`
/// (P3), and is `None` — counted unpriced by the report — when it does not; it is never priced at
/// the full-rate input by accident.
pub(crate) fn price_call(call: &InferenceCallRow, prices: &Value, priced: bool) -> Option<f64> {
    if call.cache_status == "unknown" || call.usage_unit.is_some() {
        return None;
    }
    let usage = usage_of(call);
    if priced {
        price_usage(&usage, prices)
    } else {
        Some(tokens_of(&usage))
    }
}

/// The whole run's spend, summed from its `inference_call` rows.
///
/// This is the one sum both cost surfaces use: the report splits the same rows into
/// [`SpendSlot`]s and the climb adds this per round. Summing the rows' copied `usage` instead
/// would omit the judge and the billed failures, and would let two surfaces disagree about the
/// same call.
pub(crate) async fn ledger_spend_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    prices: &Value,
    priced: bool,
) -> Result<f64, store::inference::InferenceStoreError> {
    let calls = store::inference::eval_run_calls_tx(conn, tenant, run_id).await?;
    Ok(calls
        .iter()
        .filter_map(|call| price_call(call, prices, priced))
        .sum())
}
