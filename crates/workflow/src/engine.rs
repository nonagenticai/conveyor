#![forbid(unsafe_code)]
//! The **pure** executor for a published workflow graph (S13 §5.3).
//!
//! One executor serves both a procedure and a workflow: S11's `procedure.step` and S13's
//! `workflow.node` call [`Engine::advance`] and are the only code that invokes actions. This module
//! does no I/O, reads no clock and opens no transaction — the caller supplies each outcome and persists
//! the `run_step` row.
//!
//! v1 is **deterministic and serial**: at most one node is emitted per advance, and when several are
//! ready the document order decides (S13 §5.3, "ready nodes run in `(position, node_id)` order even on
//! fan-out"). A join waits for every inbound edge; a branch that was not taken leaves its nodes
//! `Pending` for ever, and the run still terminates.

use std::collections::BTreeMap;

use conveyor_domain::process::OnError;
use serde_json::Value;

use crate::definition::{Definition, Edge, EdgeWhen, Node, NodeKindTag, body_nodes, start_node};

/// Where a run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    /// A node is ready or in flight.
    Running,
    /// A gate node is open; the run resumes when it is decided.
    WaitingApproval,
    /// Every reachable node finished; the run is done.
    Succeeded,
    /// A node failed under `on_error = fail` (or a gate was refused).
    Failed,
}

impl RunStatus {
    /// Whether the run is over.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// Where one node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeStatus {
    /// Not yet ready (an inbound edge is undecided, or its branch was not taken).
    Pending,
    /// Emitted by [`Engine::advance`]; the caller is invoking it.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Failed and `on_error` was `fail`, or its attempts are exhausted.
    Failed,
    /// Failed under `on_error = continue`.
    Skipped,
    /// A gate node waiting for its approval.
    Waiting,
}

impl NodeStatus {
    /// Whether the node will not change again.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Skipped)
    }
}

/// What a successful node reports beyond "done". Most nodes report nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeOutcome {
    /// A condition's verdict.
    Condition(bool),
    /// A loop's resolved item count (the caller resolved `items`).
    LoopItems(u32),
}

/// One node to execute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The node's id.
    pub node_id: String,
    /// `1` for the first attempt; a retry appends `attempt + 1`.
    pub attempt: u32,
}

/// What the caller should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advance {
    /// At most one step in v1; empty when the run is waiting or terminal.
    pub steps: Vec<Step>,
    /// The run's status after planning.
    pub status: RunStatus,
}

/// One node's engine-side state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeState {
    /// Where the node is.
    pub status: NodeStatus,
    /// Attempts started (1 while the first is in flight).
    pub attempts: u32,
}

/// Why an event is not legal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    /// The document has no single start node; publish should have refused it.
    #[error("the workflow has no single start node")]
    NoStart,
    /// The event names a node that does not exist.
    #[error("node {node:?} is not in this workflow")]
    UnknownNode {
        /// The event's node id.
        node: String,
    },
    /// The event names a node that is not in flight.
    #[error("node {node:?} is {status:?}, not Running")]
    NotRunning {
        /// The event's node id.
        node: String,
        /// What the engine holds.
        status: NodeStatus,
    },
    /// A condition or loop finished without the outcome the caller must supply.
    #[error("node {node:?} needs its outcome: {expected}")]
    MissingOutcome {
        /// The node id.
        node: String,
        /// Which outcome the node kind requires.
        expected: &'static str,
    },
    /// A gate was decided but the node is not a waiting gate.
    #[error("node {node:?} is not a gate waiting for a decision")]
    NotWaitingGate {
        /// The node id.
        node: String,
    },
    /// A loop's resolved items exceed its publish cap.
    #[error("node {node:?}: {items} iterations exceed the declared cap of {max}")]
    LoopOverIteration {
        /// The loop node.
        node: String,
        /// What the caller resolved.
        items: u32,
        /// The node's `max_iterations`.
        max: u32,
    },
    /// A recorded trace is not a legal prefix of this definition's execution.
    ///
    /// The trace is the run's only state: a resumed run whose rows do not replay to the same plan is
    /// corrupt or was recorded against another definition, and guessing would execute the wrong node.
    #[error("run trace diverges at position {position}: {detail}")]
    TraceDiverged {
        /// The row's `(position, attempt)`-ordered index.
        position: u32,
        /// What the engine planned versus what the row says.
        detail: String,
    },
}

struct LoopState {
    /// Resolved when the loop node itself succeeds; `None` until then.
    items: Option<u32>,
    /// How many body iterations have been started.
    started: u32,
}

/// The run's whole in-memory state. The caller persists what it needs (`run`, `run_step`); the engine
/// itself is rebuilt from those rows on a worker restart.
pub struct Engine {
    status: RunStatus,
    start: String,
    nodes: BTreeMap<String, NodeState>,
    outcomes: BTreeMap<String, NodeOutcome>,
    loops: BTreeMap<String, LoopState>,
    /// loop id -> the ids in its cycle (the body), excluding the loop node itself.
    bodies: BTreeMap<String, Vec<String>>,
}

impl Engine {
    /// Start a run of `definition`.
    ///
    /// The document must have passed `validate_publish`; the engine still refuses a graph with no
    /// single start because it will not guess an entry point.
    pub fn new(definition: &Definition) -> Result<Self, EngineError> {
        let start = start_node(definition).ok_or(EngineError::NoStart)?;
        let nodes = definition
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    NodeState {
                        status: NodeStatus::Pending,
                        attempts: 0,
                    },
                )
            })
            .collect();
        let mut bodies: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in definition
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKindTag::Loop)
        {
            bodies.insert(
                node.id.clone(),
                body_nodes(definition, &node.id).into_iter().collect(),
            );
        }
        Ok(Self {
            status: RunStatus::Running,
            start,
            nodes,
            outcomes: BTreeMap::new(),
            loops: BTreeMap::new(),
            bodies,
        })
    }

    /// The run's status.
    pub fn status(&self) -> RunStatus {
        self.status
    }

    /// One node's state, for persistence and tests.
    pub fn node_state(&self, node_id: &str) -> Option<&NodeState> {
        self.nodes.get(node_id)
    }

    /// Plan the next step.
    ///
    /// Returns at most one step (v1 is serial). A gate changes the run to
    /// [`RunStatus::WaitingApproval`] and emits no step; a terminal run returns no step.
    pub fn advance(&mut self, definition: &Definition) -> Advance {
        if self.status.is_terminal() {
            return self.snapshot();
        }
        if self
            .nodes
            .values()
            .any(|state| state.status == NodeStatus::Waiting)
        {
            self.status = RunStatus::WaitingApproval;
            return self.snapshot();
        }
        // An invocation is in flight; the caller has not reported its outcome yet.
        if self
            .nodes
            .values()
            .any(|state| state.status == NodeStatus::Running)
        {
            return self.snapshot();
        }

        // Loop boundaries move without a step: the first body iteration, the next one, or `done`.
        self.advance_loops(definition);

        let Some(node_id) = self.ready_nodes(definition).into_iter().next() else {
            self.status = if self
                .nodes
                .values()
                .any(|state| state.status == NodeStatus::Failed)
            {
                RunStatus::Failed
            } else {
                RunStatus::Succeeded
            };
            return self.snapshot();
        };

        let Some(node) = definition.node(&node_id) else {
            self.status = RunStatus::Failed;
            return self.snapshot();
        };
        if node.kind == NodeKindTag::Gate {
            if let Some(state) = self.nodes.get_mut(&node_id) {
                state.status = NodeStatus::Waiting;
                state.attempts = state.attempts.max(1);
            }
            self.status = RunStatus::WaitingApproval;
            return self.snapshot();
        }

        let attempt = {
            let Some(state) = self.nodes.get_mut(&node_id) else {
                self.status = RunStatus::Failed;
                return self.snapshot();
            };
            state.status = NodeStatus::Running;
            state.attempts += 1;
            state.attempts
        };
        Advance {
            steps: vec![Step { node_id, attempt }],
            status: self.status,
        }
    }

    /// Report that the in-flight node succeeded.
    pub fn succeeded(
        &mut self,
        definition: &Definition,
        node_id: &str,
        outcome: Option<NodeOutcome>,
    ) -> Result<(), EngineError> {
        let node = definition
            .node(node_id)
            .ok_or_else(|| EngineError::UnknownNode {
                node: node_id.to_owned(),
            })?;
        self.expect_running(node_id)?;
        match node.kind {
            NodeKindTag::Condition => {
                if !matches!(outcome, Some(NodeOutcome::Condition(_))) {
                    return Err(EngineError::MissingOutcome {
                        node: node_id.to_owned(),
                        expected: "one Condition(bool)",
                    });
                }
            }
            NodeKindTag::Loop => {
                let Some(NodeOutcome::LoopItems(items)) = outcome else {
                    return Err(EngineError::MissingOutcome {
                        node: node_id.to_owned(),
                        expected: "LoopItems(count)",
                    });
                };
                let max = node.max_iterations.unwrap_or(0);
                if items > max {
                    return Err(EngineError::LoopOverIteration {
                        node: node_id.to_owned(),
                        items,
                        max,
                    });
                }
                self.loops.insert(
                    node_id.to_owned(),
                    LoopState {
                        items: Some(items),
                        started: 0,
                    },
                );
            }
            NodeKindTag::Action | NodeKindTag::Procedure | NodeKindTag::Gate => {}
        }
        if let Some(recorded) = outcome {
            self.outcomes.insert(node_id.to_owned(), recorded);
        }
        if let Some(state) = self.nodes.get_mut(node_id) {
            state.status = NodeStatus::Succeeded;
        }
        Ok(())
    }

    /// Report that the in-flight node failed; `on_error` decides what happens next.
    pub fn failed(
        &mut self,
        definition: &Definition,
        node_id: &str,
        _error_class: &str,
    ) -> Result<(), EngineError> {
        let node = definition
            .node(node_id)
            .ok_or_else(|| EngineError::UnknownNode {
                node: node_id.to_owned(),
            })?;
        self.expect_running(node_id)?;
        let attempts = self
            .nodes
            .get(node_id)
            .map(|state| state.attempts)
            .unwrap_or(0);
        let retryable = i64::from(node.max_attempts()) > i64::from(attempts);
        let next = match node.on_error() {
            OnError::Continue => NodeStatus::Skipped,
            OnError::Retry if retryable => NodeStatus::Pending,
            OnError::Fail | OnError::Retry => NodeStatus::Failed,
        };
        if let Some(state) = self.nodes.get_mut(node_id) {
            state.status = next;
        }
        if next == NodeStatus::Failed {
            self.status = RunStatus::Failed;
        }
        Ok(())
    }

    /// Decide a waiting gate. A refusal fails the run; a gate is not retried.
    pub fn gate_decided(
        &mut self,
        _definition: &Definition,
        node_id: &str,
        approved: bool,
    ) -> Result<(), EngineError> {
        let Some(state) = self.nodes.get(node_id) else {
            return Err(EngineError::UnknownNode {
                node: node_id.to_owned(),
            });
        };
        if state.status != NodeStatus::Waiting {
            return Err(EngineError::NotWaitingGate {
                node: node_id.to_owned(),
            });
        }
        if let Some(state) = self.nodes.get_mut(node_id) {
            state.status = if approved {
                NodeStatus::Succeeded
            } else {
                NodeStatus::Failed
            };
        }
        if approved {
            if self.status == RunStatus::WaitingApproval {
                self.status = RunStatus::Running;
            }
        } else {
            self.status = RunStatus::Failed;
        }
        Ok(())
    }

    /// A snapshot of the run's status.
    pub fn snapshot(&self) -> Advance {
        Advance {
            steps: Vec::new(),
            status: self.status,
        }
    }

    /// The node ids a loop repeats, excluding the loop node itself.
    pub fn body(&self, loop_id: &str) -> Option<&Vec<String>> {
        self.bodies.get(loop_id)
    }

    /// The gate node currently waiting for a decision, if any.
    pub fn waiting_node(&self) -> Option<&str> {
        self.nodes
            .iter()
            .find(|(_, state)| state.status == NodeStatus::Waiting)
            .map(|(id, _)| id.as_str())
    }

    /// The node whose invocation is in flight, if any.
    ///
    /// A resumed run reads its rows, replays them, and stops on a `running` row; the scheduler then
    /// asks the host for that node's outcome instead of planning a new step.
    pub fn running_node(&self) -> Option<&str> {
        self.nodes
            .iter()
            .find(|(_, state)| state.status == NodeStatus::Running)
            .map(|(id, _)| id.as_str())
    }

    fn expect_running(&self, node_id: &str) -> Result<(), EngineError> {
        let Some(state) = self.nodes.get(node_id) else {
            return Err(EngineError::UnknownNode {
                node: node_id.to_owned(),
            });
        };
        if state.status != NodeStatus::Running {
            return Err(EngineError::NotRunning {
                node: node_id.to_owned(),
                status: state.status,
            });
        }
        Ok(())
    }

    /// Move every loop that is at a boundary: start its first iteration, the next one, or take `done`.
    fn advance_loops(&mut self, definition: &Definition) {
        let loops: Vec<String> = self.bodies.keys().cloned().collect();
        for loop_id in loops {
            let Some(loop_node) = definition.node(&loop_id) else {
                continue;
            };
            if loop_node.kind != NodeKindTag::Loop {
                continue;
            }
            let Some(state) = self.nodes.get(&loop_id) else {
                continue;
            };
            if state.status != NodeStatus::Succeeded {
                continue;
            }
            let Some(loop_state) = self.loops.get(&loop_id) else {
                continue;
            };
            let Some(items) = loop_state.items else {
                continue;
            };
            if items == 0 {
                continue;
            }
            if loop_state.started == 0 {
                self.reset_body(&loop_id);
                if let Some(loop_state) = self.loops.get_mut(&loop_id) {
                    loop_state.started = 1;
                }
                continue;
            }
            if loop_state.started < items && self.body_settled(definition, &loop_id) {
                self.reset_body(&loop_id);
                if let Some(loop_state) = self.loops.get_mut(&loop_id) {
                    loop_state.started += 1;
                }
            }
        }
    }

    fn reset_body(&mut self, loop_id: &str) {
        let Some(body) = self.bodies.get(loop_id).cloned() else {
            return;
        };
        for id in body {
            if let Some(state) = self.nodes.get_mut(&id) {
                state.status = NodeStatus::Pending;
                state.attempts = 0;
            }
            self.outcomes.remove(&id);
        }
    }

    /// Whether one body iteration has no runnable node left — every body node is terminal, or Pending
    /// with all its inbound edges decided and none taken (an untaken branch inside the body).
    fn body_settled(&self, definition: &Definition, loop_id: &str) -> bool {
        let Some(body) = self.bodies.get(loop_id) else {
            return true;
        };
        for id in body {
            let Some(state) = self.nodes.get(id) else {
                continue;
            };
            match state.status {
                NodeStatus::Running | NodeStatus::Waiting => return false,
                NodeStatus::Pending => {
                    let inbound: Vec<&Edge> = definition
                        .edges
                        .iter()
                        .filter(|edge| &edge.to == id)
                        .collect();
                    let all_decided = inbound.iter().all(|edge| {
                        (edge.from == *loop_id && self.iteration_active(loop_id))
                            || self.source_terminal(edge)
                    });
                    if !all_decided {
                        return false;
                    }
                    let any_taken = inbound.iter().any(|edge| {
                        if edge.from == *loop_id {
                            self.iteration_active(loop_id)
                        } else {
                            self.edge_taken(definition, edge)
                        }
                    });
                    if !any_taken {
                        continue;
                    }
                    return false;
                }
                _ => {}
            }
        }
        true
    }

    fn iteration_active(&self, loop_id: &str) -> bool {
        self.loops
            .get(loop_id)
            .is_some_and(|state| state.started >= 1 && state.items.unwrap_or(0) >= state.started)
    }

    /// The ready nodes in deterministic document order.
    fn ready_nodes(&self, definition: &Definition) -> Vec<String> {
        let mut ready = Vec::new();
        for node in &definition.nodes {
            let Some(state) = self.nodes.get(&node.id) else {
                continue;
            };
            if state.status != NodeStatus::Pending {
                continue;
            }
            if node.id != self.start {
                let inbound: Vec<&Edge> = definition
                    .edges
                    .iter()
                    .filter(|edge| edge.to == node.id)
                    .collect();
                if inbound.is_empty() {
                    continue;
                }
                // A join waits for every inbound edge; at least one must have been taken.
                if !inbound.iter().all(|edge| self.source_terminal(edge)) {
                    continue;
                }
                if !inbound.iter().any(|edge| self.edge_taken(definition, edge)) {
                    continue;
                }
            }
            ready.push(node.id.clone());
        }
        // Document order is the deterministic tie-break; ids break a same-index tie (none today).
        ready.sort_by(|left, right| {
            let left_index = definition
                .nodes
                .iter()
                .position(|node| &node.id == left)
                .unwrap_or(usize::MAX);
            let right_index = definition
                .nodes
                .iter()
                .position(|node| &node.id == right)
                .unwrap_or(usize::MAX);
            left_index.cmp(&right_index).then_with(|| left.cmp(right))
        });
        ready
    }

    fn source_terminal(&self, edge: &Edge) -> bool {
        self.nodes
            .get(&edge.from)
            .is_some_and(|state| state.status.is_terminal())
    }

    /// Whether one edge was the branch the source actually took.
    fn edge_taken(&self, definition: &Definition, edge: &Edge) -> bool {
        let Some(source) = definition.node(&edge.from) else {
            return false;
        };
        let Some(state) = self.nodes.get(&edge.from) else {
            return false;
        };
        if !state.status.is_terminal() {
            return false;
        }
        match (source.kind, edge.when) {
            // `on_error = continue` skips the node but keeps the graph moving along its default edges.
            (NodeKindTag::Condition, Some(when)) => {
                let Some(NodeOutcome::Condition(verdict)) = self.outcomes.get(&edge.from) else {
                    return false;
                };
                match when {
                    EdgeWhen::True => *verdict,
                    EdgeWhen::False => !*verdict,
                    EdgeWhen::Each | EdgeWhen::Done => false,
                }
            }
            (NodeKindTag::Condition, None) => false,
            (NodeKindTag::Loop, Some(EdgeWhen::Each)) => {
                let Some(items) = self.loops.get(&edge.from).and_then(|state| state.items) else {
                    return false;
                };
                let started = self
                    .loops
                    .get(&edge.from)
                    .map(|state| state.started)
                    .unwrap_or(0);
                started >= 1 && started <= items && !self.body_settled(definition, &edge.from)
            }
            (NodeKindTag::Loop, Some(EdgeWhen::Done)) => {
                let Some(items) = self.loops.get(&edge.from).and_then(|state| state.items) else {
                    return false;
                };
                if items == 0 {
                    return true;
                }
                let started = self
                    .loops
                    .get(&edge.from)
                    .map(|state| state.started)
                    .unwrap_or(0);
                started == items && self.body_settled(definition, &edge.from)
            }
            (NodeKindTag::Loop, None) => false,
            _ => edge.when.is_none(),
        }
    }
}

/// One `run_step` row as the engine needs it to replay a run.
///
/// The row is the run's state: the engine holds nothing the trace does not carry (S13 §5.3, "each
/// executed node appends `run_step`"). `output` is the row's `output` column — a condition records
/// its verdict as a boolean, a loop its count as `{"items": n}`.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedStep {
    /// Execution order within the run (1-based), not graph position.
    pub position: u32,
    /// The node the attempt ran.
    pub node_id: String,
    /// 1 for the first attempt; a retry records 2, 3, ...
    pub attempt: u32,
    /// The row's final status.
    pub status: RecordedStatus,
    /// The row's resolved input (the `run_step.input` column); ignored by replay.
    pub input: Value,
    /// The row's recorded output.
    pub output: Value,
    /// The attempt's wall time in milliseconds; ignored by replay.
    pub duration_ms: i32,
    /// The failure message when the attempt failed; ignored by replay.
    pub error: Option<String>,
}

/// The `run_step.status` values a replay understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordedStatus {
    /// The attempt started and has not finished; replay stops here.
    Running,
    /// The node finished successfully.
    Succeeded,
    /// The attempt failed (a retry, if any, is a later row).
    Failed,
    /// The node failed under `on_error = continue`.
    Skipped,
}

impl Engine {
    /// Rebuild a run's engine by replaying its recorded trace.
    ///
    /// Every row must be the step the engine would plan next — the trace is the only state, so a row
    /// that does not match the plan is refused ([`EngineError::TraceDiverged`]) rather than guessed at.
    /// Replay stops at a `Running` row (an attempt in flight) or after the last row.
    pub fn resume(definition: &Definition, rows: &[RecordedStep]) -> Result<Self, EngineError> {
        let mut engine = Self::new(definition)?;
        let mut ordered: Vec<&RecordedStep> = rows.iter().collect();
        ordered.sort_by_key(|row| (row.position, row.attempt));
        for row in ordered {
            let node = definition
                .node(&row.node_id)
                .ok_or_else(|| EngineError::TraceDiverged {
                    position: row.position,
                    detail: format!("node {:?} is not in this workflow", row.node_id),
                })?;
            let advance = engine.advance(definition);
            if node.kind == NodeKindTag::Gate {
                // A gate emits no step: `advance` parks it in Waiting, and the row records the decision.
                let waiting = engine.waiting_node().map(str::to_owned);
                if waiting.as_deref() != Some(row.node_id.as_str()) {
                    return Err(EngineError::TraceDiverged {
                        position: row.position,
                        detail: format!(
                            "a gate row for {:?}, but the engine is waiting on {waiting:?}",
                            row.node_id
                        ),
                    });
                }
                let approved = row.output.as_bool().unwrap_or(false);
                engine.gate_decided(definition, &row.node_id, approved)?;
                continue;
            }
            let Some(step) = advance.steps.first() else {
                return Err(EngineError::TraceDiverged {
                    position: row.position,
                    detail: format!(
                        "no step was planned, but the row records node {:?} attempt {}",
                        row.node_id, row.attempt
                    ),
                });
            };
            if step.node_id != row.node_id || step.attempt != row.attempt {
                return Err(EngineError::TraceDiverged {
                    position: row.position,
                    detail: format!(
                        "planned node {:?} attempt {}, the row records node {:?} attempt {}",
                        step.node_id, step.attempt, row.node_id, row.attempt
                    ),
                });
            }
            match row.status {
                RecordedStatus::Running => break,
                RecordedStatus::Succeeded => {
                    let outcome = outcome_for(node, &row.output);
                    engine.succeeded(definition, &row.node_id, outcome)?;
                }
                RecordedStatus::Failed | RecordedStatus::Skipped => {
                    engine.failed(definition, &row.node_id, "recorded")?;
                }
            }
        }
        Ok(engine)
    }
}

/// The engine-side outcome a row's output encodes (the trace is the state, S13 §5.3).
fn outcome_for(node: &Node, output: &Value) -> Option<NodeOutcome> {
    match node.kind {
        NodeKindTag::Condition => Some(NodeOutcome::Condition(output.as_bool().unwrap_or(false))),
        NodeKindTag::Loop => Some(NodeOutcome::LoopItems(
            output
                .get("items")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as u32,
        )),
        _ => None,
    }
}
