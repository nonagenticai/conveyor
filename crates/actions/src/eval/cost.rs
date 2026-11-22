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
/// `prices` is the version's free-form jsonb; the two keys this understands are `input` and
/// `output`, US dollars per million tokens (the shape `eval_version.prices` carries in the
/// domain's own tests). Anything else — no prices, a missing rate, a usage object that names no
/// token counts — returns `None`, and the caller then reports **tokens** instead of cents rather
/// than inventing a rate.
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
    // Cents: (tokens / 1e6) dollars * 100.
    Some((input * input_rate + output * output_rate) / 10_000.0)
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
/// (`unknown`), the call was a non-token surface, or a priced version has no rate for it.
/// Cache-class rates are P3's refinement; a cache row still prices today because
/// `prompt_tokens` is the full-rate count.
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
