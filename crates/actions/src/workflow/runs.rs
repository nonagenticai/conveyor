//! `workflow_runs.*` — start, read and cancel one run of a published workflow (S13 step 6).
//!
//! Starting mirrors S11's `procedure_runs.start`: the run row, its first trace row and the job
//! that drives it commit together (CONTRACT §7), so a crash leaves either a run nobody drives or
//! a job naming a run that never existed — never one without the other. Cancelling reuses the
//! generic run machine ([`store::run::cancel_run_tx`]) so `uq_run_live` is released exactly as an
//! ordinary run's cancellation releases it.

use conveyor_auth::Role;
use conveyor_domain::{RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::workflow::run::{enqueue_workflow_node_tx, set_workflow_id_tx};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::RunOut;
use crate::control::runs::{emit_run_event, read_record, run_out};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::workflow::{invalid, workflow_error};

/// `workflow_runs.start` input. `card_id` scopes the run to a card (`uq_run_live` then allows one
/// live run per card); without it the run is project-scoped and `uq_run_project_live` applies.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRunStartIn {
    /// The published workflow to run.
    pub workflow_id: Uuid,
    /// The project the run belongs to.
    pub project_id: Uuid,
    /// The card the run executes, when it is card-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// The run's inputs (`{{run.inputs.*}}`); `{}` when omitted.
    #[serde(default)]
    pub inputs: Value,
    /// The `capability_agent` a model-driven node resolves its step input from (S13 step 8,
    /// CONTRACT §17). Omitted means the run declares no executor, and an `agent.run` node must
    /// carry its whole config in the node input. The FK (migration `0027`) refuses an unknown id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
}

/// `workflow_runs.get` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRunIn {
    /// The run.
    pub run_id: Uuid,
}

/// `workflow_runs.cancel` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRunCancelIn {
    /// The run.
    pub run_id: Uuid,
    /// Why it was cancelled; recorded on the run's `error_class`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── workflow_runs.start ──────────────────────────────────────────────────────────────────

/// Start a published workflow as a run and enqueue its first tick.
#[crate::action(
    name = "workflow_runs.start",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflow_runs_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowRunStartIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let workflow = store::workflow::workflow::get_by_id_tx(&mut *tx, ctx.tenant, input.workflow_id)
        .await
        .map_err(|error| workflow_error(error, "workflow_runs.start"))?;
    let (out, etag) = start_published_tx(
        &mut *tx,
        &ctx,
        &workflow,
        input.project_id,
        input.card_id,
        input.inputs,
        input.capability_agent_id,
        None,
    )
    .await?;
    Ok(HandlerOutput::created(out).with_etag(etag))
}

/// The service principal a trigger seam starts a run as.
pub fn service_ctx(tenant: TenantId, app_key: &str) -> HandlerCtx {
    HandlerCtx {
        tenant,
        actor_id: Uuid::nil(),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        channel: None,
        request_id: None,
        version: None,
        if_match: None,
        action: "workflow_runs.start",
        app_key: app_key.to_owned(),
    }
}

/// The one start path: validate the run's parents, then create, transition and enqueue it.
///
/// The workflow must already be resolved; a non-published row is refused here so no seam can
/// run a draft. Callers: the action (by id) and [`start_by_key_tx`] (the hook route and the
/// `workflow.start` worker kind).
#[allow(clippy::too_many_arguments)]
pub async fn start_published_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    workflow: &store::workflow::workflow::WorkflowRecord,
    project_id: Uuid,
    card_id: Option<Uuid>,
    inputs: Value,
    capability_agent_id: Option<Uuid>,
    // `(parent_run_id, process_step_id)` when a process `call` step starts this workflow. `None`
    // is the ordinary root (a schedule, a hook or `workflow_runs.start`); a child uses
    // `insert_child_run_tx` so the run tree's depth/one-live-child rules apply and the parent's
    // root slot is not collided with.
    parent: Option<(Uuid, Uuid)>,
) -> Result<(RunOut, String), ActionError> {
    let action = "workflow_runs.start";
    if workflow.status != "published" {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "{action}: the workflow is {}, not published",
                workflow.status
            ),
        ));
    }

    let card = match card_id {
        Some(card_id) => {
            let card = store::card::get_card_tx(&mut *tx, ctx.tenant, card_id)
                .await
                .map_err(|error| crate::control::store_error(error, action))?
                .ok_or_else(|| ActionError::resource_not_found("card"))?;
            let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.board_id)
                .await
                .map_err(|error| crate::control::store_error(error, action))?
                .ok_or_else(|| ActionError::resource_not_found("board"))?;
            if board.project_id != project_id {
                return Err(invalid(
                    "project_id",
                    "mismatch",
                    format!(
                        "the card belongs to project {}, not {}",
                        board.project_id, project_id
                    ),
                ));
            }
            Some(card)
        }
        None => None,
    };

    let andon = store::andon::get_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| crate::control::store_error(error, action))?;
    if andon.is_some_and(|state| state.engaged) {
        return Err(ActionError::new(
            409,
            "andon-engaged",
            "the tenant's andon cord is pulled; no run may be started while it is engaged",
        ));
    }

    let inputs = if inputs.is_null() { json!({}) } else { inputs };
    let attempt = match &card {
        Some(card) => store::run::next_attempt_for_card_tx(&mut *tx, ctx.tenant, card.id)
            .await
            .map_err(|error| crate::control::store_error(error, action))?,
        None => 1,
    };
    let new_run = store::run::NewRun {
        card_id,
        project_id,
        attempt,
        correlation_id: None,
        process_id: None,
        inputs,
        // S15's run wiring resolves a capability; until then the explicit-input path is the
        // whole gate (S13 §5.3 "Sequencing"). The executor id is the caller's: S13 step 8
        // added the column pass-through so a resolved `capability_agent` reaches the step
        // executor without S15 editing this action.
        capability_id: None,
        capability_agent_id,
        // A call step's workflow is a CHILD of the calling run; every other caller is a root.
        // `insert_child_run_tx` derives the depth and enforces the one-live-child rule.
        parent_run_id: parent.map(|(run_id, _)| run_id),
        step_id: parent.map(|(_, step_id)| step_id),
        // A workflow run started by a PROCESS `call` step keeps that process step as its edge;
        // a workflow step would only be used by a workflow parent (0037).
        workflow_step_id: None,
        depth: 0,
        process_version: 1,
        // Workflow runs carry no organisation pin of their own (T0); a child inherits its
        // parent's pin inside `insert_child_run_tx`.
        job_binding_id: None,
        release_hash: None,
    };
    let run = match parent {
        Some((parent_run_id, _)) => {
            store::run::insert_child_run_tx(&mut *tx, ctx.tenant, parent_run_id, &new_run).await
        }
        None => store::run::insert_run_tx(&mut *tx, ctx.tenant, &new_run).await,
    }
    .map_err(|error| crate::control::run_error(error, action))?;
    set_workflow_id_tx(&mut *tx, ctx.tenant, run.id, workflow.id)
        .await
        .map_err(|error| crate::control::store_error(error, action))?;

    let queued_record = read_record(tx, ctx, run.id, action).await?;
    let queued_out = run_out(&queued_record);
    emit_run_event(
        &mut *tx,
        ctx,
        run.id,
        "run.created",
        json!({"run_id": run.id, "card_id": run.card_id, "attempt": run.attempt}),
        serde_json::to_value(&queued_out).unwrap_or(Value::Null),
    )
    .await?;

    let started = store::run::transition_run_tx(&mut *tx, ctx.tenant, run.id, RunStatus::Running)
        .await
        .map_err(|error| crate::control::run_error(error, action))?;
    enqueue_workflow_node_tx(&mut *tx, ctx.tenant, run.id, 0)
        .await
        .map_err(|error| crate::control::store_error(error, action))?;

    let record = read_record(tx, ctx, started.id, action).await?;
    let out = run_out(&record);
    emit_run_event(
        &mut *tx,
        ctx,
        started.id,
        "run.started",
        json!({"run_id": started.id, "attempt": started.attempt}),
        serde_json::to_value(&out).unwrap_or(Value::Null),
    )
    .await?;

    Ok((out, started.version.etag()))
}

/// Resolve a **published** workflow by key and start it.
pub async fn start_by_key_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    workflow_key: &str,
    project_id: Uuid,
    card_id: Option<Uuid>,
    inputs: Value,
    capability_agent_id: Option<Uuid>,
) -> Result<(RunOut, String), ActionError> {
    let workflow =
        store::workflow::workflow::get_published_by_key_tx(&mut *tx, ctx.tenant, workflow_key)
            .await
            .map_err(|error| workflow_error(error, "workflow_runs.start"))?;
    start_published_tx(
        tx,
        ctx,
        &workflow,
        project_id,
        card_id,
        inputs,
        capability_agent_id,
        None,
    )
    .await
}

// ── workflow_runs.get ────────────────────────────────────────────────────────────────────

/// Read one workflow run.
#[crate::action(
    name = "workflow_runs.get",
    version = 1,
    kind = "internal",
    scopes = ["workflows:read"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflow_runs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowRunIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let record = read_record(tx, &ctx, input.run_id, "workflow_runs.get").await?;
    Ok(HandlerOutput::ok(run_out(&record)))
}

// ── workflow_runs.cancel ─────────────────────────────────────────────────────────────────

/// Cancel a live workflow run; pending gate approvals are voided with it.
#[crate::action(
    name = "workflow_runs.cancel",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflow_runs_cancel(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowRunCancelIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let action = "workflow_runs.cancel";
    let current = store::run::get_run_tx(&mut *tx, ctx.tenant, input.run_id)
        .await
        .map_err(|error| crate::control::run_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    if !matches!(
        current.status,
        RunStatus::Queued | RunStatus::Running | RunStatus::WaitingApproval
    ) {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "run {} is {}; only a live run can be cancelled",
                current.id, current.status
            ),
        ));
    }
    let reason = input.reason.unwrap_or_else(|| "cancelled".to_owned());
    let cancelled = store::run::cancel_run_tx(&mut *tx, ctx.tenant, current.id, &reason)
        .await
        .map_err(|error| crate::control::run_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    // A pending gate on a cancelled run will never be answered; withdraw it so it does not hold
    // `uq_appr_pending` and the expiry sweep.
    crate::control::runs::void_run_approvals(&mut *tx, &ctx, cancelled.id).await?;
    let record = read_record(tx, &ctx, cancelled.id, action).await?;
    let out = run_out(&record);
    emit_run_event(
        &mut *tx,
        &ctx,
        cancelled.id,
        "run.cancelled",
        json!({"run_id": cancelled.id, "reason": reason}),
        serde_json::to_value(&out).unwrap_or(Value::Null),
    )
    .await?;
    Ok(HandlerOutput::ok(out))
}
