#![forbid(unsafe_code)]
//! Phase-2 workflow engine (S13).
//!
//! Two halves, both pure:
//! - [`definition`] — the one-document graph model and its **publish** rules (S13 §5.2). No database,
//!   no registry: the caller passes the live manifest's facts.
//! - [`engine`] — the **pure** executor, `advance` and the event methods (S13 §5.3). No I/O, no clock,
//!   no transaction: the caller invokes each node and reports its outcome.
//! - [`agent`] — the **pure** bounded tool loop behind `agent.run` (S13 step 8, CONTRACT §17). Same
//!   split: the caller calls the model and the tools, the loop decides what is next and when a cap
//!   is hit.
//!
//! The dependency direction is `workflow → domain` only (CONTRACT §1); this crate never depends on
//! `conveyor-actions`, so a registry fact reaches it as data.

pub mod agent;
pub mod definition;
pub mod engine;
pub mod expect;
pub mod n8n;
pub mod nodes;
pub mod scheduler;
pub mod trace;

pub use agent::{
    Agent, AgentConfig, AgentError, AgentOutput, Caps as AgentCaps, DEFAULT_TIMEOUT_MS,
    DEFAULT_TOOL_CALLS, DEFAULT_TURNS, MAX_DEPTH, MAX_TIMEOUT_MS as AGENT_MAX_TIMEOUT_MS,
    MAX_TOOL_CALLS, MAX_TURNS, Message, ModelReply, Reply, ToolCall, ToolCallRecord, ToolReply,
};

pub use definition::{
    AGENT_ACTION, Definition, Edge, EdgeWhen, MAX_BYTES, MAX_LOOP_ITERATIONS, MAX_LOOP_NESTING,
    MAX_NODES, MAX_TIMEOUT_MS, MODEL_ACTION, Node, NodeKindTag, Predicate, PublishError,
    PublishFacts, SCHEMA_VERSION, validate_publish,
};
pub use engine::{
    Advance, Engine, EngineError, NodeOutcome, NodeState, NodeStatus, RecordedStatus, RecordedStep,
    RunStatus, Step,
};
pub use nodes::{NodeError, WORKFLOW_NODE_NS, idempotency_key};
pub use scheduler::{
    DriveError, GateState, HostResult, HostStatus, NodeRequest, Plan, Positions, apply, apply_gate,
    plan,
};
