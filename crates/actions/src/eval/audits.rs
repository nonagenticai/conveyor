#![forbid(unsafe_code)]
//! `eval_audits.*` and `eval_findings.*` — the health audit's action surface (S19 Part C
//! step 14; SPEC D25).
//!
//! The audit is a durable row plus a job: `eval_audits.start` writes it `queued` and enqueues
//! `eval.audit.run`, which runs the mechanical checklist and the oracle/null/judge controls and
//! writes the findings. The actions here never compute a finding themselves; they are the seam a
//! person and the kata read.
//!
//! **The audit proposes; a person disposes.** `eval_findings.decide` is the only transition a
//! finding has, it records who decided and when, and it is `evals:write` — the same scope that
//! starts an audit, not the sign-off scope. The audit never edits the instrument it audits.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::eval::{AuditRow, FindingRow};
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::{Page, PageIn};

use super::{EVAL_FINDING_DECIDED, emit, eval_error};

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `eval_audits.start` — audit one frozen instrument.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AuditStartIn {
    /// The eval.
    pub eval_id: Uuid,
    /// The frozen version to audit.
    pub version_id: Uuid,
    /// The run whose measured facts (truncation, failures, noise floor) the audit reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_run_id: Option<Uuid>,
}

/// `eval_audits.get`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AuditIdIn {
    /// The audit.
    pub id: Uuid,
}

/// `eval_audits.list`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AuditListIn {
    /// Only this eval's audits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_id: Option<Uuid>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `eval_findings.list`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct FindingListIn {
    /// Only this audit's findings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<Uuid>,
    /// `misleading | noise | generality`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// `open | accepted | fixed | dismissed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `eval_findings.decide`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct FindingDecideIn {
    /// The finding to decide.
    pub finding_id: Uuid,
    /// `accepted | fixed | dismissed`.
    pub decision: String,
    /// Why. Carried on the event; the row stores who decided and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One audit.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AuditView {
    /// The row id.
    pub id: Uuid,
    /// The eval audited.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// The run the measured facts came from.
    pub target_run_id: Option<Uuid>,
    /// `queued | running | succeeded | failed`.
    pub status: String,
    /// The answer key's score under the instrument's own programmatic graders.
    pub oracle_score: Option<f64>,
    /// An empty answer's score.
    pub null_score: Option<f64>,
    /// The headline metric's noise floor.
    pub noise_floor: Option<f64>,
    /// The headline metric's headroom.
    pub headroom: Option<f64>,
    /// Which controls ran, and the resolution line.
    pub summary: Value,
    /// Why a failed audit failed.
    pub error: Option<String>,
    /// Who asked.
    pub requested_by: Option<Uuid>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
    /// When it last moved.
    pub updated_at: DateTime<Utc>,
}

/// One finding.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct FindingView {
    /// The row id.
    pub id: Uuid,
    /// The audit it belongs to.
    pub audit_id: Uuid,
    /// The checklist section (`1`..`6`).
    pub section: String,
    /// The check's stable name.
    pub item: String,
    /// `misleading | noise | generality`.
    pub severity: String,
    /// What was observed.
    pub evidence: String,
    /// One concrete change worth considering.
    pub suggestion: String,
    /// `open | accepted | fixed | dismissed`.
    pub status: String,
    /// Who decided.
    pub decided_by: Option<Uuid>,
    /// When it was decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

/// One audit and its findings.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AuditGetOut {
    /// The audit.
    pub audit: AuditView,
    /// Every finding it produced.
    pub findings: Vec<FindingView>,
}

fn audit_view(row: AuditRow) -> AuditView {
    AuditView {
        id: row.id,
        eval_id: row.eval_id,
        version_id: row.version_id,
        target_run_id: row.target_run_id,
        status: row.status,
        oracle_score: row.oracle_score.and_then(|value| value.to_f64()),
        null_score: row.null_score.and_then(|value| value.to_f64()),
        noise_floor: row.noise_floor.and_then(|value| value.to_f64()),
        headroom: row.headroom.and_then(|value| value.to_f64()),
        summary: row.summary,
        error: row.error,
        requested_by: row.requested_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn finding_view(row: FindingRow) -> FindingView {
    FindingView {
        id: row.id,
        audit_id: row.audit_id,
        section: row.section,
        item: row.item,
        severity: row.severity,
        evidence: row.evidence,
        suggestion: row.suggestion,
        status: row.status,
        decided_by: row.decided_by,
        decided_at: row.decided_at,
        created_at: row.created_at,
    }
}

// ── eval_audits.start ───────────────────────────────────────────────────────────────────

/// The audit's job kind. One job per audit; the worker finalises the row.
pub const AUDIT_JOB_KIND: &str = "eval.audit.run";

/// Start a health audit over one frozen instrument.
#[crate::action(
    name = "eval_audits.start",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_audits_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuditStartIn,
) -> Result<AuditView, ActionError> {
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let version = store::eval::get_version_tx(&mut *tx, ctx.tenant, input.version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    if version.eval_id != eval.id {
        return Err(invalid(
            "version_id",
            "invalid_value",
            "the version belongs to a different eval",
        ));
    }
    if let Some(run_id) = input.target_run_id {
        let run = store::eval::get_run_tx(&mut *tx, ctx.tenant, run_id)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
        if run.eval_id != eval.id {
            return Err(invalid(
                "target_run_id",
                "invalid_value",
                "the run belongs to a different eval",
            ));
        }
        if run.version_id != version.id {
            return Err(invalid(
                "target_run_id",
                "invalid_value",
                "the run measures a different version than the one audited",
            ));
        }
    }

    let audit_id = store::eval::insert_audit_tx(
        &mut *tx,
        ctx.tenant,
        &store::eval::NewAudit {
            eval_id: eval.id,
            version_id: version.id,
            target_run_id: input.target_run_id,
            requested_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    store::jobs::enqueue_tx(
        &mut *tx,
        ctx.tenant,
        AUDIT_JOB_KIND,
        &format!("eval_audit:{audit_id}"),
        json!({"audit_id": audit_id}),
        None,
        3,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "enqueue eval.audit.run");
        ActionError::internal()
    })?;

    let row = store::eval::get_audit_tx(&mut *tx, ctx.tenant, audit_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(ActionError::internal)?;
    Ok(audit_view(row))
}

// ── eval_audits.get ────────────────────────────────────────────────────────────────────

/// One audit and its findings.
#[crate::action(
    name = "eval_audits.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_audits_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuditIdIn,
) -> Result<AuditGetOut, ActionError> {
    let audit = store::eval::get_audit_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("eval audit"))?;
    let findings = store::eval::list_findings_tx(
        &mut *tx,
        ctx.tenant,
        Some(audit.id),
        None,
        None,
        None,
        store::actions::clamp_limit(Some(200)),
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    Ok(AuditGetOut {
        audit: audit_view(audit),
        findings: findings.into_iter().map(finding_view).collect(),
    })
}

// ── eval_audits.list ───────────────────────────────────────────────────────────────────

/// One page of audits, newest first.
#[crate::action(
    name = "eval_audits.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_audits_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuditListIn,
) -> Result<Page<AuditView>, ActionError> {
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            store::actions::decode_time_cursor(raw)
                .ok_or_else(|| invalid("cursor", "invalid_value", "not a page cursor"))?,
        ),
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    let mut rows =
        store::eval::list_audits_tx(&mut *tx, ctx.tenant, input.eval_id, cursor, limit + 1)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.into_iter().map(audit_view).collect(),
        next_cursor,
    })
}

// ── eval_findings.list ─────────────────────────────────────────────────────────────────

/// One page of findings, most severe first.
#[crate::action(
    name = "eval_findings.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_findings_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: FindingListIn,
) -> Result<Page<FindingView>, ActionError> {
    let severity = match input.severity.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw @ ("misleading" | "noise" | "generality")) => Some(raw),
        Some(_) => {
            return Err(invalid(
                "severity",
                "invalid_value",
                "must be misleading, noise or generality",
            ));
        }
    };
    let status = match input.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw @ ("open" | "accepted" | "fixed" | "dismissed")) => Some(raw),
        Some(_) => {
            return Err(invalid(
                "status",
                "invalid_value",
                "must be open, accepted, fixed or dismissed",
            ));
        }
    };
    // The cursor is the last row's `(severity, id)` pair, encoded as JSON.
    let cursor: Option<(String, Uuid)> = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => serde_json::from_str(raw)
            .map_err(|_| invalid("cursor", "invalid_value", "not a page cursor"))?,
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    let mut rows = store::eval::list_findings_tx(
        &mut *tx,
        ctx.tenant,
        input.audit_id,
        severity,
        status,
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| serde_json::to_string(&(row.severity.clone(), row.id)).unwrap_or_default())
    } else {
        None
    };
    Ok(Page {
        items: rows.into_iter().map(finding_view).collect(),
        next_cursor,
    })
}

// ── eval_findings.decide ───────────────────────────────────────────────────────────────

/// Decide one open finding. A finding is decided once; a second decision is a `409`.
#[crate::action(
    name = "eval_findings.decide",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_findings_decide(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: FindingDecideIn,
) -> Result<FindingView, ActionError> {
    let decision = match input.decision.trim() {
        raw @ ("accepted" | "fixed" | "dismissed") => raw,
        _ => {
            return Err(invalid(
                "decision",
                "invalid_value",
                "must be accepted, fixed or dismissed",
            ));
        }
    };
    // The audit's eval is the event's aggregate; read it after the decision so a foreign id is
    // a 404 from the store before anything is written and the event still names its topic.
    let finding = store::eval::decide_finding_tx(
        &mut *tx,
        ctx.tenant,
        input.finding_id,
        decision,
        ctx.actor_id,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    let audit = store::eval::get_audit_tx(&mut *tx, ctx.tenant, finding.audit_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("eval audit"))?;
    emit(
        &mut *tx,
        &ctx,
        audit.eval_id,
        finding.id,
        decision,
        EVAL_FINDING_DECIDED,
        json!({
            "finding_id": finding.id,
            "audit_id": finding.audit_id,
            "decision": decision,
            "note": input.note,
            "decided_by": ctx.actor_id,
        }),
    )
    .await?;
    Ok(finding_view(finding))
}
