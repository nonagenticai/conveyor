#![forbid(unsafe_code)]
//! `workflow.node` — the job that drives one attempt of a workflow run (S13 steps 6–7).
//!
//! S11's `procedure.step` and this module are the only two executors: both call the pure engine in
//! `crates/workflow` (`plan` / `apply`), never a second state machine (S13 §5.3, decision 3).
//!
//! The job is the run's **only** driver and v1 is serial, so a run has one live `workflow.node`
//! job. Each tick rebuilds the engine from `run_step` (the trace is the state), plans one step,
//! performs it, records the row and re-enqueues the tick. A crash between the invocation and the
//! record is safe: the action's `Idempotency-Key` is `uuidv5(run_id, node_id, attempt)`, so the
//! retry replays the stored response instead of running twice.

use std::collections::BTreeMap;

use conveyor_domain::inference::CallerKind;
use conveyor_domain::template::{StepOutcome, TemplateCtx};
use conveyor_domain::{RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::workflow::run::{
    NewWorkflowStep, RunStepRow, enqueue_workflow_node_tx, finish_step_tx, list_run_steps_tx,
    next_position_tx, record_running_output_tx, start_step_tx, workflow_run_context_tx,
};
use conveyor_workflow::trace::hash_value;
use conveyor_workflow::{
    Definition, Engine, GateState, HostResult, NodeRequest, Plan, Positions, RecordedStatus,
    RecordedStep,
};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use conveyor_auth::Role;

use crate::control::runs::{emit_run_event, read_record, run_out};
use crate::invoke::{Authority, HandlerCtx, InvokeRequest, UsageContext, invoke};
use crate::registry::registry;

/// The executor's action name, as the audit row and a job failure spell it.
pub const ACTION: &str = "workflow.node";

/// How long a waiting tick waits before it polls a procedure child again.
const POLL_DELAY_SECS: i32 = 5;

/// The most nested procedures a workflow may start (`procedure/workflow nesting ≤ 3`, S13 §5.3).
const MAX_NESTING: u8 = 3;

/// The reserved input key that carries the nesting depth to a child procedure run.
const DEPTH_KEY: &str = "_workflow_depth";

/// A failed node job.
#[derive(Debug)]
pub struct NodeFailure {
    /// What went wrong.
    pub message: String,
    /// Whether the job should be retried rather than declared dead.
    pub retryable: bool,
}

impl NodeFailure {
    fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }

    fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

fn retryable(error: impl std::fmt::Display) -> NodeFailure {
    NodeFailure::retryable(format!("database: {error}"))
}

/// Drive one attempt of `run_id`. Called by the `workflow.node` worker arm.
pub async fn execute_node(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    run_id: Uuid,
) -> Result<(), NodeFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(context) = workflow_run_context_tx(&mut tx, tenant, run_id)
        .await
        .map_err(retryable)?
    else {
        return Err(NodeFailure::permanent(format!(
            "run {run_id} does not exist in this tenant; the job names nothing"
        )));
    };
    // A cancelled or terminal run owes the job nothing (the state that owns the run wins).
    if context.status != "running" && context.status != "queued" {
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    }
    let Some(workflow_id) = context.workflow_id else {
        return Err(NodeFailure::permanent(format!(
            "run {run_id} is not a workflow run"
        )));
    };
    let workflow = store::workflow::workflow::get_by_id_tx(&mut tx, tenant, workflow_id)
        .await
        .map_err(|error| NodeFailure::permanent(format!("workflow: {error}")))?;
    let bytes = serde_json::to_vec(&workflow.definition)
        .map_err(|error| NodeFailure::permanent(format!("workflow definition: {error}")))?;
    let definition = Definition::decode(&bytes)
        .map_err(|error| NodeFailure::permanent(format!("workflow definition: {error}")))?;
    let rows = list_run_steps_tx(&mut tx, tenant, run_id)
        .await
        .map_err(retryable)?;
    let recorded: Vec<RecordedStep> = rows.iter().filter_map(recorded_step).collect();
    let next = next_position_tx(&mut tx, tenant, run_id)
        .await
        .map_err(retryable)?;
    let card = match context.card_id {
        Some(card_id) => match store::card::get_card_tx(&mut tx, tenant, card_id)
            .await
            .map_err(retryable)?
        {
            Some(card) => json!({
                "id": card.id,
                "board_id": card.board_id,
                "card_type_id": card.card_type_id,
            }),
            None => json!({}),
        },
        None => json!({}),
    };
    let ctx = TemplateCtx {
        run: json!({
            "id": run_id,
            "card_id": context.card_id,
            "project_id": context.project_id,
            "inputs": context.inputs,
        }),
        steps: step_outcomes(&rows),
        card,
        // A workflow has no process; the template root carries the document's identity instead.
        process: json!({"key": workflow.key, "version": workflow.version}),
        // A workflow node is not card-scoped, so it has no board lanes to resolve.
        board: json!({}),
        position: usize::try_from(next).unwrap_or(usize::MAX),
    };
    let depth = context
        .inputs
        .get(DEPTH_KEY)
        .and_then(Value::as_u64)
        .unwrap_or(0) as u8;
    tx.commit().await.map_err(retryable)?;

    let mut engine = Engine::resume(&definition, &recorded)
        .map_err(|error| NodeFailure::permanent(format!("run {run_id} trace: {error}")))?;
    let positions = Positions::new(&recorded);
    let mut host = Host {
        pool,
        tenant,
        app_key,
        run_id,
        card_id: context.card_id,
        depth,
        steps: index_steps(&rows),
        workflow_key: workflow.key.clone(),
        capability_agent_id: context.capability_agent_id,
    };
    let plan =
        conveyor_workflow::scheduler::plan(&definition, &mut engine, run_id, &ctx, &positions)
            .map_err(|error| NodeFailure::permanent(format!("plan: {error}")))?;
    let action_ctx = handler_ctx(tenant, app_key);

    match plan {
        Plan::Finished(status) => {
            let status = match status {
                conveyor_workflow::RunStatus::Succeeded => RunStatus::Succeeded,
                _ => RunStatus::Failed,
            };
            finish_run(pool, tenant, &action_ctx, run_id, status).await
        }
        Plan::Waiting { .. } => Ok(()),
        Plan::Recorded(row) => {
            persist_tick(
                pool,
                tenant,
                &action_ctx,
                &definition,
                run_id,
                &row,
                engine.status().is_terminal(),
            )
            .await
        }
        Plan::Execute(request) => {
            let result = host.execute(&request).await?;
            if result.status == conveyor_workflow::HostStatus::Deferred {
                return rearm(pool, tenant, run_id).await;
            }
            let row =
                conveyor_workflow::scheduler::apply(&definition, &mut engine, &request, result)
                    .map_err(|error| NodeFailure::permanent(format!("apply: {error}")))?;
            persist_tick(
                pool,
                tenant,
                &action_ctx,
                &definition,
                run_id,
                &row,
                engine.status().is_terminal(),
            )
            .await
        }
        Plan::Poll(request) => {
            let result = host.poll(&request).await?;
            if result.status == conveyor_workflow::HostStatus::Deferred {
                return rearm(pool, tenant, run_id).await;
            }
            let row =
                conveyor_workflow::scheduler::apply(&definition, &mut engine, &request, result)
                    .map_err(|error| NodeFailure::permanent(format!("apply: {error}")))?;
            persist_tick(
                pool,
                tenant,
                &action_ctx,
                &definition,
                run_id,
                &row,
                engine.status().is_terminal(),
            )
            .await
        }
        Plan::Gate(request) => {
            let state = host.gate(&request).await?;
            let recorded =
                conveyor_workflow::scheduler::apply_gate(&definition, &mut engine, &request, state)
                    .map_err(|error| NodeFailure::permanent(format!("gate: {error}")))?;
            match recorded {
                None => Ok(()),
                Some(row) => {
                    persist_tick(
                        pool,
                        tenant,
                        &action_ctx,
                        &definition,
                        run_id,
                        &row,
                        engine.status().is_terminal(),
                    )
                    .await
                }
            }
        }
    }
}

/// Record a decided gate as its trace row; the caller settles the run's own status.
///
/// The executor records an approved gate on its next tick (the run resumes and replays the
/// decision). A rejection and an expiry are terminal for the run, so the decide/expire paths record
/// the row here — otherwise the trace would end without the gate that failed it.
pub async fn record_gate_row(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    node_id: &str,
    attempt: i32,
    approved: bool,
) -> Result<(), NodeFailure> {
    let position = next_position_tx(conn, tenant, run_id)
        .await
        .map_err(retryable)?;
    start_step_tx(
        conn,
        tenant,
        run_id,
        &NewWorkflowStep {
            position,
            attempt,
            node_id: node_id.to_owned(),
            action: "gate".to_owned(),
            input: json!({}),
            input_hash: Some(hash_value(&json!({}))),
        },
    )
    .await
    .map_err(|error| NodeFailure::retryable(format!("run_step: {error}")))?;
    let output = json!(approved);
    finish_step_tx(
        conn,
        tenant,
        run_id,
        position,
        attempt,
        if approved { "succeeded" } else { "failed" },
        &output,
        Some(&hash_value(&output)),
        Some(0),
        None,
    )
    .await
    .map_err(|error| NodeFailure::retryable(format!("run_step: {error}")))?;
    Ok(())
}

/// Re-enqueue the run's tick after a wait (a child run still executing).
async fn rearm(pool: &PgPool, tenant: TenantId, run_id: Uuid) -> Result<(), NodeFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    enqueue_workflow_node_tx(&mut tx, tenant, run_id, POLL_DELAY_SECS)
        .await
        .map_err(retryable)?;
    tx.commit().await.map_err(retryable)
}

/// Persist one attempt's row and either finish the run or re-enqueue the tick.
async fn persist_tick(
    pool: &PgPool,
    tenant: TenantId,
    _ctx: &HandlerCtx,
    definition: &Definition,
    run_id: Uuid,
    row: &RecordedStep,
    terminal: bool,
) -> Result<(), NodeFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let position = i32::try_from(row.position)
        .map_err(|_| NodeFailure::permanent("run_step position overflow"))?;
    let attempt = i32::try_from(row.attempt)
        .map_err(|_| NodeFailure::permanent("run_step attempt overflow"))?;
    let action = definition
        .node(&row.node_id)
        .map(node_action)
        .unwrap_or_else(|| ACTION.to_owned());
    start_step_tx(
        &mut tx,
        tenant,
        run_id,
        &NewWorkflowStep {
            position,
            attempt,
            node_id: row.node_id.clone(),
            action,
            input: row.input.clone(),
            input_hash: Some(hash_value(&row.input)),
        },
    )
    .await
    .map_err(|error| NodeFailure::retryable(format!("run_step: {error}")))?;
    finish_step_tx(
        &mut tx,
        tenant,
        run_id,
        position,
        attempt,
        status_str(row.status),
        &row.output,
        Some(&hash_value(&row.output)),
        Some(row.duration_ms),
        row.error.as_deref(),
    )
    .await
    .map_err(|error| NodeFailure::retryable(format!("run_step: {error}")))?;
    if !terminal {
        enqueue_workflow_node_tx(&mut tx, tenant, run_id, 0)
            .await
            .map_err(retryable)?;
    }
    tx.commit().await.map_err(retryable)
}

/// Settle a terminal run and announce it (the same pair an ordinary run emits).
async fn finish_run(
    pool: &PgPool,
    tenant: TenantId,
    ctx: &HandlerCtx,
    run_id: Uuid,
    status: RunStatus,
) -> Result<(), NodeFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    match status {
        RunStatus::Succeeded => {
            let run = store::run::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Succeeded)
                .await
                .map_err(|error| NodeFailure::retryable(format!("run: {error}")))?;
            let record = read_record(&mut tx, ctx, run.id, ACTION)
                .await
                .map_err(|error| NodeFailure::permanent(format!("run: {error}")))?;
            emit_run_event(
                &mut tx,
                ctx,
                run_id,
                "run.succeeded",
                json!({"run_id": run_id}),
                serde_json::to_value(run_out(&record)).unwrap_or(Value::Null),
            )
            .await
            .map_err(|error| NodeFailure::permanent(format!("event: {error}")))?;
        }
        RunStatus::Failed => {
            let run = store::run::fail_run_tx(
                &mut tx,
                tenant,
                run_id,
                Some("workflow_failed"),
                Some("a workflow node failed"),
            )
            .await
            .map_err(|error| NodeFailure::retryable(format!("run: {error}")))?;
            let Some(run) = run else {
                tx.commit().await.map_err(retryable)?;
                return Ok(());
            };
            emit_run_event(
                &mut tx,
                ctx,
                run_id,
                "run.failed",
                json!({"run_id": run_id, "error_class": "workflow_failed"}),
                json!({"run_id": run.id, "status": "failed"}),
            )
            .await
            .map_err(|error| NodeFailure::permanent(format!("event: {error}")))?;
            // The old `err.card` error trigger: a failed run starts the card's recovery flow.
            store::run::enqueue_run_failed_trigger_tx(&mut tx, tenant, run_id)
                .await
                .map_err(|error| NodeFailure::retryable(format!("run.failed trigger: {error}")))?;
        }
        _ => {
            return Err(NodeFailure::permanent(format!(
                "the engine finished a run as {status:?}, which is not terminal"
            )));
        }
    }
    tx.commit().await.map_err(retryable)
}

/// The executor's context for the events it appends.
fn handler_ctx(tenant: TenantId, app_key: &str) -> HandlerCtx {
    HandlerCtx {
        tenant,
        actor_id: Uuid::nil(),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        // The executor is a machine admin; a tenant's Member/Viewer overrides do not apply to it.
        policy: Default::default(),
        channel: None,
        request_id: None,
        version: None,
        if_match: None,
        action: ACTION,
        app_key: app_key.to_owned(),
    }
}

/// The effects the caller performs for a plan.
struct Host<'a> {
    pool: &'a PgPool,
    tenant: TenantId,
    app_key: &'a str,
    run_id: Uuid,
    card_id: Option<Uuid>,
    depth: u8,
    steps: BTreeMap<(String, u32), RunStepRow>,
    workflow_key: String,
    /// The `capability_agent` an `agent.run` node resolves its step input from (S13 step 8).
    capability_agent_id: Option<Uuid>,
}

impl Host<'_> {
    async fn execute(&mut self, request: &NodeRequest) -> Result<HostResult, NodeFailure> {
        let row_id = self.start_row(request).await?;
        match request.kind {
            conveyor_workflow::NodeKindTag::Action => self.invoke_action(request, row_id).await,
            conveyor_workflow::NodeKindTag::Procedure => {
                self.start_procedure(request, row_id).await
            }
            other => Err(NodeFailure::permanent(format!(
                "a {other:?} node is not executed by the host"
            ))),
        }
    }

    /// Report an in-flight attempt: an action is re-invoked under the same key (a replay); a
    /// procedure node polls its child run.
    async fn poll(&mut self, request: &NodeRequest) -> Result<HostResult, NodeFailure> {
        if request.kind == conveyor_workflow::NodeKindTag::Procedure {
            return self.poll_procedure(request).await;
        }
        self.execute(request).await
    }

    async fn gate(&mut self, request: &NodeRequest) -> Result<GateState, NodeFailure> {
        let gate_kind = request.gate_kind.clone().ok_or_else(|| {
            NodeFailure::permanent(format!("gate {:?} names no gate_kind", request.node_id))
        })?;
        let mut tx = store::begin_scoped(self.pool, self.tenant)
            .await
            .map_err(retryable)?;
        if store::approval::approved_workflow_gate_tx(
            &mut tx,
            self.tenant,
            self.run_id,
            &request.node_id,
            i32::try_from(request.attempt).unwrap_or(1),
        )
        .await
        .map_err(retryable)?
        .is_some()
        {
            tx.commit().await.map_err(retryable)?;
            return Ok(GateState::Approved);
        }
        if let Some(status) = store::approval::workflow_gate_status_tx(
            &mut tx,
            self.tenant,
            self.run_id,
            &request.node_id,
            i32::try_from(request.attempt).unwrap_or(1),
        )
        .await
        .map_err(retryable)?
        {
            let state = match status.as_str() {
                "pending" => Some(GateState::Pending),
                "rejected" | "expired" | "voided" => Some(GateState::Refused),
                _ => None,
            };
            if let Some(state) = state {
                tx.commit().await.map_err(retryable)?;
                return Ok(state);
            }
        }
        let requirements =
            crate::process::step::gate_requirements(&mut tx, self.tenant, &gate_kind)
                .await
                .map_err(|failure| NodeFailure {
                    message: failure.message,
                    retryable: failure.retryable,
                })?;
        let meta = json!({
            "node_id": request.node_id,
            "position": request.position,
            "attempt": request.attempt,
        });
        match store::approval::open_gate_with_meta_tx(
            &mut tx,
            self.tenant,
            self.run_id,
            &gate_kind,
            "workflow",
            requirements.required_decisions,
            requirements.expires_at,
            None,
            meta,
        )
        .await
        {
            Ok(_) | Err(store::ApprovalStoreError::Conflict) => {
                tx.commit().await.map_err(retryable)?;
                Ok(GateState::Pending)
            }
            Err(error) => Err(NodeFailure::retryable(format!(
                "open gate {gate_kind:?}: {error}"
            ))),
        }
    }

    /// Create (or restart) this attempt's row so a crash leaves a trace to resume from.
    async fn start_row(&mut self, request: &NodeRequest) -> Result<Uuid, NodeFailure> {
        let mut tx = store::begin_scoped(self.pool, self.tenant)
            .await
            .map_err(retryable)?;
        let id = start_step_tx(
            &mut tx,
            self.tenant,
            self.run_id,
            &NewWorkflowStep {
                position: i32::try_from(request.position).unwrap_or(i32::MAX),
                attempt: i32::try_from(request.attempt).unwrap_or(i32::MAX),
                node_id: request.node_id.clone(),
                action: request.action.clone().unwrap_or_else(|| {
                    request
                        .procedure_key
                        .clone()
                        .unwrap_or_else(|| "gate".to_owned())
                }),
                input: request.input.clone(),
                input_hash: Some(hash_value(&request.input)),
            },
        )
        .await
        .map_err(|error| NodeFailure::retryable(format!("run_step: {error}")))?;
        tx.commit().await.map_err(retryable)?;
        Ok(id)
    }

    async fn invoke_action(
        &mut self,
        request: &NodeRequest,
        row_id: Uuid,
    ) -> Result<HostResult, NodeFailure> {
        let action = request.action.clone().ok_or_else(|| {
            NodeFailure::permanent(format!("node {:?} names no action", request.node_id))
        })?;
        // `agent.run` is a step primitive, not a normal action: its loop calls other actions, and
        // an action handler holds the invocation transaction with no pool (see
        // `crate::workflow::agent`). The executor is the layer that already invokes actions
        // outside a transaction, so it drives the loop here.
        if action == conveyor_workflow::AGENT_ACTION {
            return self.run_agent(request, row_id).await;
        }
        let invoke_request = InvokeRequest {
            action,
            input: request.input.clone(),
            precondition: None,
            idempotency_key: Some(request.idempotency_key.clone()),
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!(
                "{ACTION}:{}:{}:{}",
                self.run_id, request.position, request.attempt
            )),
            trace_id: Some(format!("run:{}", self.run_id)),
            depth: 1,
            app_key: self.app_key.to_owned(),
            usage: Some(UsageContext {
                caller_kind: CallerKind::WorkflowStep,
                run_id: Some(self.run_id),
                run_step_id: Some(row_id),
                conversation_turn_id: None,
                eval_run_id: None,
                prompt_ref: Some(format!("workflow:{}", self.workflow_key)),
                prompt_version: Some(format!("{}:{}", self.workflow_key, request.node_id)),
                correlation_id: Some(format!(
                    "run:{}:{}:{}",
                    self.run_id, request.position, request.attempt
                )),
            }),
        };
        match invoke(self.pool, registry(), self.tenant, invoke_request).await {
            Ok(outcome) => Ok(HostResult::succeeded(
                outcome.body.get("output").cloned().unwrap_or(Value::Null),
            )),
            // Nothing was decided: the row stays `running` and the tick retries under the same key.
            Err(error) if error.status >= 500 => Ok(HostResult::deferred()),
            Err(error) => Ok(HostResult::failed(error.code)),
        }
    }

    /// Drive one `agent.run` node (S13 step 8): resolve the step input, run the capped loop, and
    /// report the outcome with the engine's own vocabulary. The seam is shared with S11's
    /// `procedure.step` ([`crate::workflow::agent::run_step`]) — both are step executors, and
    /// CONTRACT §17 puts resolution in the executor, not in the action.
    async fn run_agent(
        &mut self,
        request: &NodeRequest,
        row_id: Uuid,
    ) -> Result<HostResult, NodeFailure> {
        match crate::workflow::agent::run_step(
            self.pool,
            self.tenant,
            self.run_id,
            Some(row_id),
            self.capability_agent_id,
            self.app_key,
            Some(&request.idempotency_key),
            u32::from(self.depth),
            &request.input,
        )
        .await
        {
            Ok(out) => Ok(HostResult::succeeded(
                serde_json::to_value(out).unwrap_or(Value::Null),
            )),
            // A router 5xx or an open breaker: the tick retries without spending the attempt.
            Err(error) if error.status >= 500 => Ok(HostResult::deferred()),
            Err(error) => Ok(HostResult::failed(error.code)),
        }
    }

    /// Start a procedure child and record its id so a later tick can poll rather than restart it.
    ///
    /// The child is started **under this run and step** (`parent_run_id`/`step_id`, the run tree's
    /// own columns), never as a second root on the same card: `uq_run_live` is root-only, so a
    /// root start here would 409 against the parent that is running right now (S13 §5.3, W5).
    async fn start_procedure(
        &mut self,
        request: &NodeRequest,
        row_id: Uuid,
    ) -> Result<HostResult, NodeFailure> {
        let key = request.procedure_key.clone().ok_or_else(|| {
            NodeFailure::permanent(format!("node {:?} names no procedure", request.node_id))
        })?;
        let card_id = self.card_id.ok_or_else(|| {
            NodeFailure::permanent(format!(
                "node {:?} starts a procedure, so the run must be card-scoped",
                request.node_id
            ))
        })?;
        if self.depth >= MAX_NESTING {
            return Ok(HostResult::failed("workflow.nesting"));
        }
        let mut inputs = request.input.clone();
        if let Value::Object(map) = &mut inputs {
            map.insert(DEPTH_KEY.to_owned(), json!(self.depth + 1));
        }
        let invoke_request = InvokeRequest {
            action: "process_runs.start".to_owned(),
            input: json!({
                "card_id": card_id,
                "process_key": key,
                "parent_run_id": self.run_id,
                "workflow_step_id": row_id,
                "inputs": inputs
            }),
            precondition: None,
            idempotency_key: Some(request.idempotency_key.clone()),
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!(
                "{ACTION}:{}:{}:{}",
                self.run_id, request.position, request.attempt
            )),
            trace_id: Some(format!("run:{}", self.run_id)),
            depth: 1,
            app_key: self.app_key.to_owned(),
            usage: None,
        };
        let outcome = match invoke(self.pool, registry(), self.tenant, invoke_request).await {
            Ok(outcome) => outcome,
            Err(error) if error.status >= 500 => return Ok(HostResult::deferred()),
            Err(error) => return Ok(HostResult::failed(error.code)),
        };
        let Some(child) = outcome
            .body
            .get("output")
            .and_then(|output| output.get("id"))
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok())
        else {
            return Err(NodeFailure::permanent(
                "process_runs.start returned no run id",
            ));
        };
        let mut tx = store::begin_scoped(self.pool, self.tenant)
            .await
            .map_err(retryable)?;
        record_running_output_tx(
            &mut tx,
            self.tenant,
            self.run_id,
            i32::try_from(request.position).unwrap_or(i32::MAX),
            i32::try_from(request.attempt).unwrap_or(i32::MAX),
            &json!({"child_run_id": child}),
        )
        .await
        .map_err(retryable)?;
        tx.commit().await.map_err(retryable)?;
        Ok(HostResult::deferred())
    }

    async fn poll_procedure(&mut self, request: &NodeRequest) -> Result<HostResult, NodeFailure> {
        let key = (request.node_id.clone(), request.attempt);
        let Some(row) = self.steps.get(&key) else {
            // No row yet for this attempt: create it so the child can name its tree edge.
            let row_id = self.start_row(request).await?;
            return self.start_procedure(request, row_id).await;
        };
        let Some(child) = row
            .output
            .get("child_run_id")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok())
        else {
            let row_id = row.id;
            return self.start_procedure(request, row_id).await;
        };
        let mut tx = store::begin_scoped(self.pool, self.tenant)
            .await
            .map_err(retryable)?;
        let run = store::run::get_run_tx(&mut tx, self.tenant, child)
            .await
            .map_err(retryable)?;
        tx.commit().await.map_err(retryable)?;
        let Some(run) = run else {
            return Ok(HostResult::failed("procedure_not_found"));
        };
        match run.status {
            RunStatus::Succeeded => Ok(HostResult::succeeded(json!({"child_run_id": child}))),
            RunStatus::Failed | RunStatus::Cancelled => Ok(HostResult::failed("procedure_failed")),
            _ => Ok(HostResult::deferred()),
        }
    }
}

fn recorded_step(row: &RunStepRow) -> Option<RecordedStep> {
    let node_id = row.node_id.clone()?;
    let status = match row.status.as_str() {
        "queued" | "running" => RecordedStatus::Running,
        "succeeded" => RecordedStatus::Succeeded,
        "failed" => RecordedStatus::Failed,
        "skipped" => RecordedStatus::Skipped,
        _ => return None,
    };
    Some(RecordedStep {
        position: u32::try_from(row.position).unwrap_or(0),
        node_id,
        attempt: u32::try_from(row.attempt).unwrap_or(0),
        status,
        input: row.input.clone(),
        output: row.output.clone(),
        duration_ms: row.duration_ms.unwrap_or(0),
        error: row.error.clone(),
    })
}

/// The template's `steps.<n>` list: the final attempt of each position, in order.
fn step_outcomes(rows: &[RunStepRow]) -> Vec<StepOutcome> {
    let mut latest: BTreeMap<i32, &RunStepRow> = BTreeMap::new();
    for row in rows {
        latest.insert(row.position, row);
    }
    latest
        .into_values()
        .map(|row| StepOutcome {
            output: row.output.clone(),
        })
        .collect()
}

fn index_steps(rows: &[RunStepRow]) -> BTreeMap<(String, u32), RunStepRow> {
    let mut index = BTreeMap::new();
    for row in rows {
        if let Some(node_id) = &row.node_id {
            index.insert(
                (node_id.clone(), u32::try_from(row.attempt).unwrap_or(0)),
                row.clone(),
            );
        }
    }
    index
}

fn status_str(status: RecordedStatus) -> &'static str {
    match status {
        RecordedStatus::Running => "running",
        RecordedStatus::Succeeded => "succeeded",
        RecordedStatus::Failed => "failed",
        RecordedStatus::Skipped => "skipped",
    }
}

/// What the trace's `run_step.action` column says a node did: the action it invoked, the
/// procedure it started, or its kind for a control node.
fn node_action(node: &conveyor_workflow::Node) -> String {
    match node.kind {
        conveyor_workflow::NodeKindTag::Action => node
            .action
            .clone()
            .unwrap_or_else(|| node.kind.as_str().to_owned()),
        conveyor_workflow::NodeKindTag::Procedure => node
            .procedure_key
            .clone()
            .map(|key| format!("process:{key}"))
            .unwrap_or_else(|| node.kind.as_str().to_owned()),
        other => other.as_str().to_owned(),
    }
}
