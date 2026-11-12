#![forbid(unsafe_code)]
//! `workflow_traces.export` — one run's trace as JSONL (S13 step 9).
//!
//! The trace is the `run_step` rows, projected into the §5.4 record: the node kind comes from the
//! frozen definition, the effect from the row's action (or its recorded control outcome), and the
//! hashes are masked so two logically equal executions compare equal. A row written before the
//! masked-hash writer landed falls back to hashing its stored value here; a row written since
//! carries its own pinned hash, so a later mask change cannot rewrite history.

use conveyor_store::workflow::run_step::run_trace_tx;
use conveyor_workflow::trace::{TraceKind, TraceOutcome, hash_value, project_effect, to_jsonl};

// Re-exported for `conveyor admin trace-diff`: the server already depends on `conveyor-actions`,
// and the file tool must not pull the whole crate's dependencies into the command line.
pub use conveyor_workflow::trace::{
    DiffMode, DiffReport, TraceDelta, TraceDiffError, TraceRecord, diff_records, diff_records_with,
    parse_jsonl,
};
// The old side of the gate is a recorded n8n execution (S13 step 10); the projection lives in
// `conveyor-workflow` and reaches the CLI the same way.
pub use conveyor_workflow::n8n::{
    Projected as N8nProjected, Unmapped as N8nUnmapped, project_execution as project_n8n_execution,
    read_snapshot as read_n8n_snapshot,
};
// The reviewed expectation (S13 §5.4) is the gate's second entry point: the CLI reaches it here for
// the same reason as the projection — the server already depends on `conveyor-actions`.
pub use conveyor_workflow::expect::{Expected, ExpectedStep, Provenance};
use conveyor_workflow::{Definition, NodeKindTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// `workflow_traces.export` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowTracesExportIn {
    /// The run to export.
    pub run_id: Uuid,
}

/// `workflow_traces.export` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowTracesExportOut {
    /// One JSON record per line, ordered by `(position, attempt)`.
    pub jsonl: String,
}

/// Export one finished run's trace.
#[crate::action(
    name = "workflow_traces.export",
    version = 1,
    kind = "internal",
    scopes = ["workflows:read"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
pub async fn workflow_traces_export(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowTracesExportIn,
) -> Result<HandlerOutput<WorkflowTracesExportOut>, ActionError> {
    let trace = run_trace_tx(tx, ctx.tenant, input.run_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, run = %input.run_id, "workflow_traces.export: read");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;

    let definition: Definition = serde_json::from_value(trace.definition).map_err(|error| {
        tracing::error!(%error, run = %input.run_id, "workflow_traces.export: definition");
        ActionError::internal()
    })?;

    let mut records = Vec::with_capacity(trace.rows.len());
    for row in &trace.rows {
        let kind = kind_of(definition.node(&row.node_id).map(|node| node.kind));
        let outcome = outcome_of(&row.status).ok_or_else(|| {
            tracing::error!(
                status = %row.status,
                position = row.position,
                "workflow_traces.export: unknown run_step status"
            );
            ActionError::internal()
        })?;
        records.push(TraceRecord {
            run_id: input.run_id.to_string(),
            workflow_key: trace.workflow_key.clone(),
            workflow_version: trace.workflow_version,
            node_id: row.node_id.clone(),
            position: u32::try_from(row.position).unwrap_or(0),
            attempt: u32::try_from(row.attempt).unwrap_or(0),
            kind,
            effect: project_effect(kind, &row.action, &row.output),
            input_hash: row
                .input_hash
                .clone()
                .unwrap_or_else(|| hash_value(&row.input)),
            output_hash: row
                .output_hash
                .clone()
                .unwrap_or_else(|| hash_value(&row.output)),
            duration_ms: row.duration_ms,
            outcome,
        });
    }

    let jsonl = to_jsonl(&records).map_err(|error| {
        tracing::error!(%error, run = %input.run_id, "workflow_traces.export: serialise");
        ActionError::internal()
    })?;
    Ok(HandlerOutput::ok(WorkflowTracesExportOut { jsonl }))
}

/// The node kind, defaulting to an action for a row whose node left the definition.
fn kind_of(kind: Option<NodeKindTag>) -> TraceKind {
    match kind {
        Some(NodeKindTag::Action) | None => TraceKind::Action,
        Some(NodeKindTag::Procedure) => TraceKind::Procedure,
        Some(NodeKindTag::Condition) => TraceKind::Condition,
        Some(NodeKindTag::Loop) => TraceKind::Loop,
        Some(NodeKindTag::Gate) => TraceKind::Gate,
    }
}

/// The trace outcome for a `run_step.status`.
fn outcome_of(status: &str) -> Option<TraceOutcome> {
    match status {
        "succeeded" => Some(TraceOutcome::Succeeded),
        "failed" => Some(TraceOutcome::Failed),
        "skipped" => Some(TraceOutcome::Skipped),
        "running" | "queued" => Some(TraceOutcome::Running),
        _ => None,
    }
}
