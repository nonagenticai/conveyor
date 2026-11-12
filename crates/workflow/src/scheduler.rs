#![forbid(unsafe_code)]
//! The scheduler (S13 step 6): plan **one** node attempt, and apply its outcome.
//!
//! The split is deliberate. [`plan`] is pure and synchronous; it says what the caller must do
//! (`Execute`, `Poll`, `Gate`) or that nothing can be done (`Waiting`, `Finished`). The caller
//! performs the effect asynchronously and calls [`apply`] / [`apply_gate`], which reports the
//! outcome to the engine and produces the `run_step` row to persist. Nothing here reads a
//! database, a clock or a registry, and the crate gains no async dependency.
//!
//! One plan drives one attempt. The caller persists the returned [`RecordedStep`] and re-enqueues
//! the run's single driving job; that is what makes v1 serial and crash-safe — the trace is the
//! state, and [`Engine::resume`] rebuilds the engine from it on every job.

use std::collections::BTreeMap;

use conveyor_domain::template::TemplateCtx;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::definition::{Definition, Node, NodeKindTag};
use crate::engine::{
    Engine, EngineError, NodeOutcome, NodeStatus, RecordedStatus, RecordedStep, RunStatus, Step,
};
use crate::nodes::{self, NodeError};

/// Where the next row goes, and the position each node's first attempt took.
#[derive(Clone, Debug, Default)]
pub struct Positions {
    next: u32,
    first: BTreeMap<String, u32>,
}

impl Positions {
    /// Build from the run's rows, in any order.
    pub fn new(rows: &[RecordedStep]) -> Self {
        let mut positions = Positions {
            next: 1,
            first: BTreeMap::new(),
        };
        for row in rows {
            positions.next = positions.next.max(row.position.saturating_add(1));
            positions
                .first
                .entry(row.node_id.clone())
                .or_insert(row.position);
        }
        positions
    }

    /// The position a new node attempt takes.
    pub fn next(&self) -> u32 {
        self.next
    }

    /// The position a node's first attempt recorded.
    pub fn first(&self, node_id: &str) -> Option<u32> {
        self.first.get(node_id).copied()
    }

    /// The position for `step`: a retry reuses its node's first position (S13 §5.3: retries append
    /// a row at the same position, a new node takes the next one).
    pub fn for_step(&self, step: &Step) -> Result<u32, DriveError> {
        if step.attempt <= 1 {
            return Ok(self.next);
        }
        self.first(&step.node_id)
            .ok_or_else(|| DriveError::Position {
                node: step.node_id.clone(),
                attempt: step.attempt,
            })
    }
}

/// Everything the caller needs about the attempt it must run.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeRequest {
    /// The run.
    pub run_id: Uuid,
    /// The execution-order position this attempt records.
    pub position: u32,
    /// The node.
    pub node_id: String,
    /// The node's kind.
    pub kind: NodeKindTag,
    /// 1 for the first attempt.
    pub attempt: u32,
    /// The registry action an `action` node invokes.
    pub action: Option<String>,
    /// The procedure key a `procedure` node starts.
    pub procedure_key: Option<String>,
    /// The catalogue gate code a `gate` node opens.
    pub gate_kind: Option<String>,
    /// The resolved input.
    pub input: Value,
    /// `uuidv5(run_id:node_id:attempt)` (S13 §5.3).
    pub idempotency_key: String,
    /// The declared timeout, already checked against [`crate::definition::MAX_TIMEOUT_MS`].
    pub timeout_ms: i64,
}

/// What the host reported for one attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostStatus {
    /// The node completed.
    Succeeded,
    /// The attempt failed; the engine decides retry/continue/fail from the node's policy.
    Failed {
        /// The S13 error class (`timeout`, the problem code, …).
        error_class: String,
    },
    /// Not finished yet (a procedure child is still running, or the action returned 5xx and the
    /// job must retry without consuming a node attempt).
    Deferred,
}

/// One attempt's result.
#[derive(Clone, Debug, PartialEq)]
pub struct HostResult {
    /// What happened.
    pub status: HostStatus,
    /// What the node produced (the `run_step.output` value).
    pub output: Value,
    /// Wall time in milliseconds.
    pub duration_ms: i32,
}

impl HostResult {
    /// A completed node.
    pub fn succeeded(output: Value) -> Self {
        Self {
            status: HostStatus::Succeeded,
            output,
            duration_ms: 0,
        }
    }

    /// A failed attempt with its error class.
    pub fn failed(error_class: impl Into<String>) -> Self {
        Self {
            status: HostStatus::Failed {
                error_class: error_class.into(),
            },
            output: Value::Null,
            duration_ms: 0,
        }
    }

    /// Not finished yet.
    pub fn deferred() -> Self {
        Self {
            status: HostStatus::Deferred,
            output: Value::Null,
            duration_ms: 0,
        }
    }
}

/// What a gate node's decision is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateState {
    /// Still open; the run waits.
    Pending,
    /// Approved; the run resumes.
    Approved,
    /// Refused or expired; the run fails.
    Refused,
}

/// What one [`plan`] says to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// A control node ran in-process; persist this row.
    Recorded(RecordedStep),
    /// Run this attempt; then call [`apply`].
    Execute(NodeRequest),
    /// Report the outcome of an in-flight attempt; then call [`apply`].
    Poll(NodeRequest),
    /// Open or read this gate; then call [`apply_gate`].
    Gate(NodeRequest),
    /// A gate is open; persist the run as `waiting_approval` and stop.
    Waiting {
        /// The gate node.
        node_id: String,
    },
    /// The run is over.
    Finished(RunStatus),
}

/// Why a step could not be planned or applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriveError {
    /// The engine refused the replay or the event.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// The node could not be planned.
    #[error(transparent)]
    Node(#[from] NodeError),
    /// A retry has no recorded position for its node.
    #[error("node {node:?}: attempt {attempt} has no recorded position")]
    Position {
        /// The node id.
        node: String,
        /// The attempt.
        attempt: u32,
    },
    /// The engine reached a state the scheduler cannot act on.
    #[error("the engine is in an impossible state: {detail}")]
    State {
        /// What was impossible.
        detail: String,
    },
}

/// Plan one step of `run_id`.
pub fn plan(
    definition: &Definition,
    engine: &mut Engine,
    run_id: Uuid,
    ctx: &TemplateCtx,
    positions: &Positions,
) -> Result<Plan, DriveError> {
    // (1) An attempt in flight (a resumed run): the caller reports its outcome, never re-plans it.
    if let Some(node_id) = engine.running_node().map(str::to_owned) {
        let node = node(definition, &node_id)?;
        let attempt = engine
            .node_state(&node_id)
            .map(|state| state.attempts)
            .unwrap_or(1);
        let position = positions
            .first(&node_id)
            .ok_or_else(|| DriveError::Position {
                node: node_id.clone(),
                attempt,
            })?;
        return Ok(Plan::Poll(request(run_id, position, attempt, node, ctx)?));
    }

    let advance = engine.advance(definition);
    if advance.status.is_terminal() {
        return Ok(Plan::Finished(advance.status));
    }
    if advance.status == RunStatus::WaitingApproval {
        let Some(node_id) = engine.waiting_node().map(str::to_owned) else {
            return Err(DriveError::State {
                detail: "waiting_approval with no waiting node".to_owned(),
            });
        };
        let node = node(definition, &node_id)?;
        let attempt = engine
            .node_state(&node_id)
            .map(|state| state.attempts)
            .unwrap_or(1);
        return Ok(Plan::Gate(request(
            run_id,
            positions.next(),
            attempt,
            node,
            ctx,
        )?));
    }

    let Some(step) = advance.steps.first().cloned() else {
        return Err(DriveError::State {
            detail: format!("no step is ready and the run is {:?}", advance.status),
        });
    };
    let node = node(definition, &step.node_id)?;
    let position = positions.for_step(&step)?;

    match node.kind {
        NodeKindTag::Condition => {
            let verdict = nodes::evaluate(node, ctx)?;
            engine.succeeded(
                definition,
                &step.node_id,
                Some(NodeOutcome::Condition(verdict)),
            )?;
            Ok(Plan::Recorded(row(
                &request(run_id, position, step.attempt, node, ctx)?,
                RecordedStatus::Succeeded,
                json!(verdict),
                0,
            )))
        }
        NodeKindTag::Loop => {
            let count = nodes::loop_items(node, ctx)?;
            engine.succeeded(
                definition,
                &step.node_id,
                Some(NodeOutcome::LoopItems(count)),
            )?;
            Ok(Plan::Recorded(row(
                &request(run_id, position, step.attempt, node, ctx)?,
                RecordedStatus::Succeeded,
                json!({"items": count}),
                0,
            )))
        }
        NodeKindTag::Gate => Err(DriveError::State {
            detail: format!("gate {:?} emitted a step", step.node_id),
        }),
        NodeKindTag::Action | NodeKindTag::Procedure => Ok(Plan::Execute(request(
            run_id,
            position,
            step.attempt,
            node,
            ctx,
        )?)),
    }
}

/// Report an executed (or polled) attempt's outcome to the engine, producing its row.
pub fn apply(
    definition: &Definition,
    engine: &mut Engine,
    request: &NodeRequest,
    result: HostResult,
) -> Result<RecordedStep, DriveError> {
    match result.status {
        HostStatus::Deferred => Err(DriveError::State {
            detail: "apply was called for a deferred attempt".to_owned(),
        }),
        HostStatus::Succeeded => {
            engine.succeeded(definition, &request.node_id, None)?;
            Ok(row(
                request,
                RecordedStatus::Succeeded,
                result.output,
                result.duration_ms,
            ))
        }
        HostStatus::Failed { error_class } => {
            engine.failed(definition, &request.node_id, &error_class)?;
            let status = match engine
                .node_state(&request.node_id)
                .map(|state| state.status)
            {
                Some(NodeStatus::Skipped) => RecordedStatus::Skipped,
                _ => RecordedStatus::Failed,
            };
            // The failure class is the trace's only record of WHY: `run_step.error` carries it
            // (`workflow_failed` on the run names the run, not the refusal). A typed refusal —
            // `agent.config_conflict`, `agent.cap_exceeds_bound`, a 4xx action code — must be
            // visible on the step that produced it, or the trace cannot explain the run.
            let mut recorded = row(request, status, result.output, result.duration_ms);
            recorded.error = Some(error_class);
            Ok(recorded)
        }
    }
}

/// Report a gate's decision. `None` means it is still pending; the caller stops the run as
/// `waiting_approval`.
pub fn apply_gate(
    definition: &Definition,
    engine: &mut Engine,
    request: &NodeRequest,
    state: GateState,
) -> Result<Option<RecordedStep>, DriveError> {
    match state {
        GateState::Pending => Ok(None),
        GateState::Approved => {
            engine.gate_decided(definition, &request.node_id, true)?;
            Ok(Some(row(
                request,
                RecordedStatus::Succeeded,
                json!(true),
                0,
            )))
        }
        GateState::Refused => {
            engine.gate_decided(definition, &request.node_id, false)?;
            Ok(Some(row(request, RecordedStatus::Failed, json!(false), 0)))
        }
    }
}

fn node<'a>(definition: &'a Definition, node_id: &str) -> Result<&'a Node, DriveError> {
    definition.node(node_id).ok_or_else(|| DriveError::State {
        detail: format!("the engine names node {node_id:?}, which the document does not contain"),
    })
}

fn request(
    run_id: Uuid,
    position: u32,
    attempt: u32,
    node: &Node,
    ctx: &TemplateCtx,
) -> Result<NodeRequest, DriveError> {
    let input = match node.kind {
        NodeKindTag::Procedure => nodes::procedure_inputs(node, ctx)?,
        NodeKindTag::Action => nodes::input(node, ctx)?,
        _ => json!({}),
    };
    Ok(NodeRequest {
        run_id,
        position,
        node_id: node.id.clone(),
        kind: node.kind,
        attempt,
        action: node.action.clone(),
        procedure_key: node.procedure_key.clone(),
        gate_kind: node.gate_kind.clone(),
        input,
        idempotency_key: nodes::idempotency_key(run_id, &node.id, attempt),
        timeout_ms: nodes::timeout_ms(node)?,
    })
}

fn row(
    request: &NodeRequest,
    status: RecordedStatus,
    output: Value,
    duration_ms: i32,
) -> RecordedStep {
    RecordedStep {
        position: request.position,
        node_id: request.node_id.clone(),
        attempt: request.attempt,
        status,
        input: request.input.clone(),
        output,
        duration_ms,
        error: None,
    }
}
