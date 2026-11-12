#![forbid(unsafe_code)]
//! `prompt_audits.start` · `.get` · `.list` · `.check`.

use conveyor_domain::prompt_audit::{AuditScope, Confidence, FindingDraft};
use conveyor_store as store;
use conveyor_store::prompt_audit::{
    ListFindings, NewPromptAudit, PROMPT_REF_V1, get_audit_tx, insert_audit_tx, list_audits_tx,
    list_findings_tx,
};
use sqlx::PgConnection;

use super::{
    CheckFindingView, PromptAuditCheckIn, PromptAuditCheckOut, PromptAuditGetIn, PromptAuditGetOut,
    PromptAuditListIn, PromptAuditStartIn, PromptAuditView, audit_view, decode_cursor, emit_event,
    finding_view, invalid, scan, store_error,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::Page;

/// Start a manual audit and enqueue its one model call.
#[crate::action(
    name = "prompt_audits.start",
    version = 1,
    kind = "internal",
    scopes = ["inference:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_audits_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptAuditStartIn,
) -> Result<PromptAuditView, ActionError> {
    let target_model = input.target_model.trim();
    if target_model.is_empty() {
        return Err(invalid(
            "target_model",
            "required",
            "the audit needs a model to call",
        ));
    }
    let scope_value = serde_json::json!({
        "surfaces": input.scope.surfaces,
        "process_ids": input.scope.process_ids,
    });
    let scope = AuditScope::from_value(&scope_value)
        .map_err(|error| invalid("scope", "invalid", error.to_string()))?;
    // Resolve every named process in the caller's tenant before anything is written: an id from
    // another tenant must be absent, not silently audited as nothing.
    for id in &scope.process_ids {
        store::process::get_process_tx(&mut *tx, ctx.tenant, *id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "resolve a process in the audit scope");
                ActionError::internal()
            })?
            .ok_or_else(|| ActionError::resource_not_found("process"))?;
    }

    let audit_id = insert_audit_tx(
        &mut *tx,
        ctx.tenant,
        &NewPromptAudit {
            scope: scope.to_value(),
            target_model: target_model.to_owned(),
            prompt_ref: PROMPT_REF_V1.to_owned(),
            requested_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| store_error(error, "prompt_audits.start"))?;
    store::jobs::enqueue_tx(
        &mut *tx,
        ctx.tenant,
        "prompt_audit.run",
        &format!("prompt_audit:{audit_id}"),
        serde_json::json!({"audit_id": audit_id}),
        None,
        3,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "enqueue prompt_audit.run");
        ActionError::internal()
    })?;
    emit_event(
        &mut *tx,
        &ctx,
        "prompt_audit.started",
        audit_id,
        serde_json::json!({"id": audit_id, "prompt_ref": PROMPT_REF_V1, "requested_by": ctx.actor_id}),
    )
    .await?;
    let row = get_audit_tx(&mut *tx, ctx.tenant, audit_id)
        .await
        .map_err(|error| store_error(error, "prompt_audits.start"))?
        .ok_or_else(ActionError::internal)?;
    Ok(audit_view(row))
}

/// One audit and its findings.
#[crate::action(
    name = "prompt_audits.get",
    version = 1,
    kind = "internal",
    scopes = ["inference:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_audits_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptAuditGetIn,
) -> Result<PromptAuditGetOut, ActionError> {
    let audit = get_audit_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_error(error, "prompt_audits.get"))?
        .ok_or_else(|| ActionError::resource_not_found("prompt_audit"))?;
    let findings = list_findings_tx(
        &mut *tx,
        ctx.tenant,
        &ListFindings {
            audit_id: Some(input.id),
            limit: 200,
            ..ListFindings::default()
        },
    )
    .await
    .map_err(|error| store_error(error, "prompt_audits.get"))?;
    Ok(PromptAuditGetOut {
        audit: audit_view(audit),
        findings: findings.into_iter().map(finding_view).collect(),
    })
}

/// One page of audits, newest first.
#[crate::action(
    name = "prompt_audits.list",
    version = 1,
    kind = "internal",
    scopes = ["inference:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_audits_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptAuditListIn,
) -> Result<Page<PromptAuditView>, ActionError> {
    let limit = store::actions::clamp_limit(input.page.limit);
    let before = decode_cursor(input.page.cursor.as_deref())?;
    let mut rows = list_audits_tx(&mut *tx, ctx.tenant, before, limit + 1)
        .await
        .map_err(|error| store_error(error, "prompt_audits.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| super::cursor_of(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.into_iter().map(audit_view).collect(),
        next_cursor,
    })
}

/// The deterministic, synchronous draft check. Persists nothing (S16 CCR-2); only a `high` finding
/// blocks, and `processes.publish` is the caller that enforces it.
#[crate::action(
    name = "prompt_audits.check",
    version = 1,
    kind = "internal",
    scopes = ["inference:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn prompt_audits_check(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromptAuditCheckIn,
) -> Result<PromptAuditCheckOut, ActionError> {
    let process = store::process::get_process_tx(&mut *tx, ctx.tenant, input.process_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the process to check");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("process"))?;
    let steps = store::process::list_steps_tx(&mut *tx, ctx.tenant, process.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the process steps to check");
            ActionError::internal()
        })?;
    Ok(check_draft(&process, &steps))
}

/// The shared check, used by the action and by `processes.publish`.
pub(crate) fn check_draft(
    process: &conveyor_store::process::ProcessRecord,
    steps: &[conveyor_store::process::ProcessStepRecord],
) -> PromptAuditCheckOut {
    let items = scan::process_items(process, steps);
    let findings = scan::scan(&items);
    let blocking = findings
        .iter()
        .any(|finding| finding.confidence == Confidence::High);
    PromptAuditCheckOut {
        findings: findings.into_iter().map(check_view).collect(),
        blocking,
    }
}

fn check_view(finding: FindingDraft) -> CheckFindingView {
    CheckFindingView {
        location: finding.location,
        evidence: finding.evidence,
        pattern: finding.pattern.as_str().to_owned(),
        why_obsolete: finding.why_obsolete,
        confidence: finding.confidence.as_str().to_owned(),
        action: finding.action.as_str().to_owned(),
        replacement: finding.replacement,
    }
}
