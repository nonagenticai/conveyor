#![forbid(unsafe_code)]
//! `prompt_findings.list` and `prompt_findings.decide`.
//!
//! `decide` is the person's gate: `open → accepted | rejected`. `accepted → applied` is not an
//! action here — it belongs to the ordinary `processes.update` that applies the text, so the audit
//! never writes the surface it audits (CONTRACT §18). A `note` the slice names has no column to
//! persist it, so it is not accepted rather than silently discarded (S16 CCR-8).

use conveyor_domain::prompt_audit::{Confidence, FindingStatus};
use conveyor_store as store;
use conveyor_store::prompt_audit::{ListFindings, decide_finding_tx, list_findings_tx};
use sqlx::PgConnection;

use super::{
    FindingView, PromptFindingDecideIn, PromptFindingListIn, decode_cursor, emit_event,
    finding_view, invalid, store_error,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::Page;

/// One page of findings, newest first.
#[crate::action(
    name = "prompt_findings.list",
    version = 1,
    kind = "internal",
    scopes = ["inference:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_findings_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptFindingListIn,
) -> Result<Page<FindingView>, ActionError> {
    let status = match input.status.as_deref() {
        None => None,
        Some(raw) => Some(FindingStatus::parse(raw).ok_or_else(|| {
            invalid(
                "status",
                "unknown",
                "one of open, accepted, rejected, applied",
            )
        })?),
    };
    let confidence = match input.confidence.as_deref() {
        None => None,
        Some(raw) => Some(
            Confidence::parse(raw)
                .ok_or_else(|| invalid("confidence", "unknown", "one of high, medium, low"))?,
        ),
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    let before = decode_cursor(input.page.cursor.as_deref())?;
    let mut rows = list_findings_tx(
        &mut *tx,
        ctx.tenant,
        &ListFindings {
            audit_id: input.audit_id,
            status: status.map(|value| value.as_str().to_owned()),
            confidence: confidence.map(|value| value.as_str().to_owned()),
            before,
            limit: limit + 1,
        },
    )
    .await
    .map_err(|error| store_error(error, "prompt_findings.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| super::cursor_of(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.into_iter().map(finding_view).collect(),
        next_cursor,
    })
}

/// Accept or reject one finding. A person's decision, never the audit's own.
#[crate::action(
    name = "prompt_findings.decide",
    version = 1,
    kind = "internal",
    scopes = ["inference:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_findings_decide(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptFindingDecideIn,
) -> Result<FindingView, ActionError> {
    let decision = match input.decision.as_str() {
        "accept" => FindingStatus::Accepted,
        "reject" => FindingStatus::Rejected,
        _ => {
            return Err(invalid(
                "decision",
                "unknown",
                "accept or reject; `applied` follows an ordinary processes.update",
            ));
        }
    };
    let row = decide_finding_tx(
        &mut *tx,
        ctx.tenant,
        input.finding_id,
        decision,
        ctx.actor_id,
    )
    .await
    .map_err(|error| store_error(error, "prompt_findings.decide"))?;
    emit_event(
        &mut *tx,
        &ctx,
        "prompt_audit_finding.decided",
        row.audit_id,
        serde_json::json!({
            "finding_id": row.id,
            "status": row.status,
            "decided_by": ctx.actor_id,
        }),
    )
    .await?;
    Ok(finding_view(row))
}
