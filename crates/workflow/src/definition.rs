#![forbid(unsafe_code)]
//! A workflow is **one document**: `{"schema": 1, "nodes": [...], "edges": [...]}` (S13 §5.2).
//!
//! Deliberately not a language. There is no `code` node, no script, no user expression: the closed node
//! kinds are `action`, `procedure`, `condition`, `loop` and `gate`, and the closed predicate set is
//! `eq ne lt le gt ge in empty exists` with `and`/`or`/`not` (S13 §5.2, trap "the engine grows into a
//! programming language").
//!
//! This module owns the **document model and the publish-time rules**. It is pure: the caller supplies the
//! live registry's facts, the tenant's published procedures and gate codes, so nothing here reads a database
//! or a registry — the dependency direction stays `workflow → domain` (CONTRACT §1).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use conveyor_domain::process::{ActionFacts, MAX_ATTEMPTS_CAP, OnError};
use conveyor_domain::template::{self, TemplateError};

/// The only document schema this build understands.
pub const SCHEMA_VERSION: u8 = 1;
/// The publish cap on node count (S13 §5.2).
pub const MAX_NODES: usize = 200;
/// The publish cap on the serialised document (the table also carries it as a CHECK).
pub const MAX_BYTES: usize = 262_144;
/// The publish cap on one loop's iterations (S13 §5.2).
pub const MAX_LOOP_ITERATIONS: u32 = 1_000;
/// The publish cap on loop nesting (S13 §8, trap "a visual format that cannot express a loop").
pub const MAX_LOOP_NESTING: usize = 3;
/// A node's timeout when it declares none (S13 §5.3).
pub const DEFAULT_TIMEOUT_MS: i64 = 30_000;
/// The publish cap on a node's timeout (S13 §5.3).
pub const MAX_TIMEOUT_MS: i64 = 600_000;
/// The external action that reaches the model. A node may not name it directly: a procedure or
/// workflow step reaches a model only through the capped `agent.run` loop (CONTRACT §17, S13 §8
/// trap "a second loop beside `agent.run`").
pub const MODEL_ACTION: &str = "inference.chat";
/// The one action a step may use to reach a model.
///
/// Re-exported from the domain, where publish validation reads it: the workflow engine and the
/// process executor must agree on the spelling or a step validates in one and is dead in the other.
pub const AGENT_ACTION: &str = conveyor_domain::process::AGENT_STEP_ACTION;

/// The closed node vocabulary (S13 §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKindTag {
    /// Invokes one registry action (including `agent.run`).
    Action,
    /// Starts one published procedure and waits for it.
    Procedure,
    /// A closed predicate with two out-edges, `true` and `false`.
    Condition,
    /// The only cycle: `each` into the body, `done` after it.
    Loop,
    /// Parks the run in `waiting_approval` until the gate is decided.
    Gate,
}

impl NodeKindTag {
    /// The document spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Procedure => "procedure",
            Self::Condition => "condition",
            Self::Loop => "loop",
            Self::Gate => "gate",
        }
    }
}

/// Which out-edge a node takes. `true`/`false` belong to a condition, `each`/`done` to a loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeWhen {
    /// The condition's predicate held.
    True,
    /// The condition's predicate did not hold.
    False,
    /// One loop iteration enters the body.
    Each,
    /// The loop is exhausted; continue after it.
    Done,
}

impl EdgeWhen {
    /// The document spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Each => "each",
            Self::Done => "done",
        }
    }
}

/// The closed predicate set. Operands are strings (templates allowed) or JSON scalars; there is no
/// expression syntax and no user code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Predicate {
    /// `left == right`.
    Eq {
        /// Left operand.
        left: Value,
        /// Right operand.
        right: Value,
    },
    /// `left != right`.
    Ne {
        /// Left operand.
        left: Value,
        /// Right operand.
        right: Value,
    },
    /// `left < right`.
    Lt {
        /// Left operand.
        left: Value,
        /// Right operand.
        right: Value,
    },
    /// `left <= right`.
    Le {
        /// Left operand.
        left: Value,
        /// Right operand.
        right: Value,
    },
    /// `left > right`.
    Gt {
        /// Left operand.
        left: Value,
        /// Right operand.
        right: Value,
    },
    /// `left >= right`.
    Ge {
        /// Left operand.
        left: Value,
        /// Right operand.
        right: Value,
    },
    /// `left` is a member of the array `right`.
    In {
        /// The operand tested for membership.
        left: Value,
        /// The array operand.
        right: Value,
    },
    /// `left` is absent, null, `""` or `[]`.
    Empty {
        /// The operand tested.
        left: Value,
    },
    /// `left` is present and not null.
    Exists {
        /// The operand tested.
        left: Value,
    },
    /// Every child holds.
    And {
        /// The children.
        of: Vec<Predicate>,
    },
    /// At least one child holds.
    Or {
        /// The children.
        of: Vec<Predicate>,
    },
    /// The child does not hold.
    Not {
        /// The child.
        of: Box<Predicate>,
    },
}

/// One node. Kind-specific fields are optional here and required by the validator for that kind — an
/// `action` node without `action` is refused by name, not silently accepted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// The node's id, unique within the document. Edges name it.
    pub id: String,
    /// Which kind this node is.
    pub kind: NodeKindTag,
    /// `action` only: the registry action to invoke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// `action`/`procedure`: the input template (S11 §5.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_template: Option<Value>,
    /// `action` only: `fail` (default), `continue` or `retry`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<String>,
    /// `action` only: attempts `retry` may make; default 1, cap [`MAX_ATTEMPTS_CAP`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<i32>,
    /// `action` only: timeout in milliseconds; default [`DEFAULT_TIMEOUT_MS`], cap [`MAX_TIMEOUT_MS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    /// `procedure` only: the published procedure key to start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_key: Option<String>,
    /// `procedure` only: the procedure's inputs (a template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Value>,
    /// `condition` only: the predicate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<Predicate>,
    /// `loop` only: the items template (resolved to an array at run time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Value>,
    /// `loop` only: the iteration cap, `1..=MAX_LOOP_ITERATIONS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    /// `gate` only: a `catalogue_entry(kind='gate')` code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_kind: Option<String>,
}

impl Node {
    /// The timeout a run applies: the declared value or the default.
    pub fn timeout_ms(&self) -> i64 {
        self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
    }

    /// The error policy a run applies: `fail` when the node declares none.
    pub fn on_error(&self) -> OnError {
        self.on_error
            .as_deref()
            .and_then(OnError::parse)
            .unwrap_or(OnError::Fail)
    }

    /// The attempts a run may make: `1` when the node declares none.
    pub fn max_attempts(&self) -> i32 {
        self.max_attempts.unwrap_or(1)
    }
}

/// One directed edge. `when` is absent on every edge that leaves an `action`/`procedure`/`gate` node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// The source node id.
    pub from: String,
    /// The target node id.
    pub to: String,
    /// Which branch this edge is, for a `condition` (`true`/`false`) or a `loop` (`each`/`done`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<EdgeWhen>,
}

/// The whole graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    /// [`SCHEMA_VERSION`].
    pub schema: u8,
    /// The nodes, in authoring order (the deterministic tie-break for ready nodes).
    pub nodes: Vec<Node>,
    /// The edges.
    pub edges: Vec<Edge>,
}

/// What publish needs to know from the live tenant: the registry's actions, its published procedure keys
/// and its gate codes. Nothing here is read by this crate.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublishFacts<'a> {
    /// The live manifest: name and idempotency per action.
    pub actions: &'a [ActionFacts],
    /// The tenant's published procedure keys.
    pub procedures: &'a [String],
    /// The tenant's active `catalogue_entry(kind='gate')` codes.
    pub gates: &'a [String],
}

/// Why a workflow document cannot be published.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PublishError {
    /// The bytes are not a document of this schema.
    #[error("the workflow document is malformed: {detail}")]
    Malformed {
        /// The parser's message.
        detail: String,
    },
    /// The document is larger than the table and the engine accept.
    #[error("the workflow document is {bytes} bytes; the cap is {cap}")]
    TooLarge {
        /// Measured size.
        bytes: usize,
        /// [`MAX_BYTES`].
        cap: usize,
    },
    /// An unknown schema version.
    #[error("workflow schema {schema} is not supported (this build reads {expected})")]
    UnsupportedSchema {
        /// What the document declared.
        schema: u8,
        /// [`SCHEMA_VERSION`].
        expected: u8,
    },
    /// An empty graph does nothing.
    #[error("a workflow needs at least one node")]
    NoNodes,
    /// Too many nodes.
    #[error("the workflow has {count} nodes; the cap is {cap}")]
    TooManyNodes {
        /// Measured count.
        count: usize,
        /// [`MAX_NODES`].
        cap: usize,
    },
    /// A node id is blank.
    #[error("node {index}: the id is blank")]
    BlankNodeId {
        /// The node's position in the document.
        index: usize,
    },
    /// Two nodes share an id.
    #[error("node id {id:?} appears more than once")]
    DuplicateNodeId {
        /// The repeated id.
        id: String,
    },
    /// A node id has an unusable shape.
    #[error("node id {id:?} must be 1..=64 printable characters")]
    BadNodeId {
        /// The offending id.
        id: String,
    },
    /// A kind-specific field is missing.
    #[error("node {node:?}: a {kind} node needs `{field}`")]
    MissingField {
        /// The node id.
        node: String,
        /// Its kind.
        kind: &'static str,
        /// The missing field.
        field: &'static str,
    },
    /// The action name is not `<resource>.<verb>`.
    #[error("node {node:?}: {action:?} is not a `<resource>.<verb>` action name")]
    BadActionName {
        /// The node id.
        node: String,
        /// The name it declared.
        action: String,
    },
    /// The action is not in the live manifest.
    #[error("node {node:?}: {action:?} is not a registered action")]
    UnknownAction {
        /// The node id.
        node: String,
        /// The name it declared.
        action: String,
    },
    /// `retry` on an action that is not idempotent would repeat a side effect.
    #[error("node {node:?}: {action:?} is not idempotent, so it cannot be retried")]
    RetryOnNonIdempotent {
        /// The node id.
        node: String,
        /// The action it names.
        action: String,
    },
    /// `max_attempts` outside `1..=MAX_ATTEMPTS_CAP`.
    #[error("node {node:?}: max_attempts must be 1..={cap}, not {max_attempts}")]
    AttemptsOutOfRange {
        /// The node id.
        node: String,
        /// What it declared.
        max_attempts: i32,
        /// The cap that applies.
        cap: i32,
    },
    /// `on_error` is outside `fail | continue | retry`.
    #[error("node {node:?}: on_error must be fail, continue or retry, not {value:?}")]
    BadOnError {
        /// The node id.
        node: String,
        /// What it declared.
        value: String,
    },
    /// A node would start another run of the surface it lives on.
    #[error("node {node:?}: {action:?} starts a run; a workflow may not invoke the run surface")]
    SelfReference {
        /// The node id.
        node: String,
        /// The action it names.
        action: String,
    },
    /// A node would reach a model without the capped loop.
    #[error(
        "node {node:?}: {action:?} reaches a model directly; a step reaches a model only through `agent.run`"
    )]
    DirectModelCall {
        /// The node id.
        node: String,
        /// The action it names.
        action: String,
    },
    /// A timeout outside `1..=MAX_TIMEOUT_MS`.
    #[error("node {node:?}: timeout_ms must be 1..={cap}, not {timeout_ms}")]
    TimeoutOutOfRange {
        /// The node id.
        node: String,
        /// What it declared.
        timeout_ms: i64,
        /// The cap that applies.
        cap: i64,
    },
    /// A template reference that cannot be legal at any run.
    #[error("node {node:?}: {source}")]
    Template {
        /// The node id.
        node: String,
        /// The resolver's own message.
        source: TemplateError,
    },
    /// The procedure key is not published in this tenant.
    #[error("node {node:?}: procedure {key:?} is not published")]
    UnknownProcedure {
        /// The node id.
        node: String,
        /// The key it declared.
        key: String,
    },
    /// The gate code is not an active `catalogue_entry(kind='gate')` row.
    #[error("node {node:?}: {gate:?} is not a gate code in this tenant's catalogue")]
    UnknownGate {
        /// The node id.
        node: String,
        /// The gate it declared.
        gate: String,
    },
    /// A loop's iteration cap is outside `1..=MAX_LOOP_ITERATIONS`.
    #[error("node {node:?}: max_iterations must be 1..={cap}, not {max_iterations}")]
    BadIterations {
        /// The node id.
        node: String,
        /// What it declared.
        max_iterations: u32,
        /// The cap that applies.
        cap: u32,
    },
    /// An `and`/`or` with no children is a constant, not a predicate.
    #[error("node {node:?}: an `and`/`or` predicate needs at least one child")]
    EmptyPredicate {
        /// The node id.
        node: String,
    },
    /// An edge names a node that does not exist.
    #[error("edge {index}: {id:?} is not a node in this document")]
    UnknownEndpoint {
        /// The edge's position in the document.
        index: usize,
        /// The unknown id.
        id: String,
    },
    /// An edge points at its own node.
    #[error("edge {index}: node {node:?} may not point at itself")]
    SelfEdge {
        /// The edge's position.
        index: usize,
        /// The node id.
        node: String,
    },
    /// A branch edge left a node kind that has no branches.
    #[error("edge {index}: node {node:?} is a {kind} node and has no `{when}` branch")]
    WhenOnBranchlessNode {
        /// The edge's position.
        index: usize,
        /// The source node.
        node: String,
        /// The source's kind.
        kind: &'static str,
        /// The declared branch.
        when: &'static str,
    },
    /// A condition or loop is missing one of its two branches.
    #[error("node {node:?}: a {kind} node needs exactly one `{missing}` out-edge")]
    MissingBranch {
        /// The node id.
        node: String,
        /// The node's kind.
        kind: &'static str,
        /// The missing branch.
        missing: &'static str,
    },
    /// One branch is declared twice.
    #[error("node {node:?}: more than one `{when}` out-edge")]
    DuplicateBranch {
        /// The node id.
        node: String,
        /// The repeated branch.
        when: &'static str,
    },
    /// A condition or loop declared an out-edge outside its two branches.
    #[error("node {node:?}: a {kind} node's out-edges must be `{first}` and `{second}` only")]
    ForeignBranch {
        /// The node id.
        node: String,
        /// The node's kind.
        kind: &'static str,
        /// The first legal branch.
        first: &'static str,
        /// The second legal branch.
        second: &'static str,
    },
    /// No node enters the graph.
    #[error("the workflow has no start node (every node has an inbound edge)")]
    NoStart,
    /// More than one node enters the graph.
    #[error("the workflow has {count} start nodes; it needs exactly one")]
    MultipleStarts {
        /// The measured count.
        count: usize,
    },
    /// A node cannot be reached from the start.
    #[error("node {node:?} is unreachable from the start")]
    Unreachable {
        /// The node id.
        node: String,
    },
    /// A cycle that no loop node bounds would run forever.
    #[error("nodes {nodes:?} form a cycle with no `loop` node")]
    CycleWithoutLoop {
        /// The cycle's members, sorted.
        nodes: Vec<String>,
    },
    /// A loop's `done` edge re-enters its own body.
    #[error("node {node:?}: the `done` edge must leave the loop's cycle")]
    LoopDoneInsideCycle {
        /// The loop node.
        node: String,
    },
    /// Loops nested deeper than the publish cap.
    #[error("node {node:?}: loops nest {depth} deep; the cap is {cap}")]
    LoopNestingTooDeep {
        /// The innermost loop.
        node: String,
        /// Measured depth.
        depth: usize,
        /// [`MAX_LOOP_NESTING`].
        cap: usize,
    },
}

impl Definition {
    /// Decode a document: the size cap first (cheap, and it is also a table CHECK), then the schema.
    ///
    /// The DB's `pg_column_size(definition) <= 262144` is the authoritative size guard; this one refuses
    /// an obviously oversized payload before a transaction is opened.
    pub fn decode(bytes: &[u8]) -> Result<Self, PublishError> {
        if bytes.len() > MAX_BYTES {
            return Err(PublishError::TooLarge {
                bytes: bytes.len(),
                cap: MAX_BYTES,
            });
        }
        let definition: Self =
            serde_json::from_slice(bytes).map_err(|error| PublishError::Malformed {
                detail: error.to_string(),
            })?;
        if definition.schema != SCHEMA_VERSION {
            return Err(PublishError::UnsupportedSchema {
                schema: definition.schema,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(definition)
    }

    /// Serialise for storage or for `content_hash = sha256(canonical JSON)`.
    ///
    /// Field order is the struct order, so the same document always yields the same bytes.
    pub fn encode(&self) -> Result<Vec<u8>, PublishError> {
        serde_json::to_vec(self).map_err(|error| PublishError::Malformed {
            detail: error.to_string(),
        })
    }

    /// The node named `id`.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

/// Validate a document against the live tenant's facts. All-or-nothing: a draft may hold anything, a
/// published version must be runnable.
pub fn validate_publish(
    definition: &Definition,
    facts: &PublishFacts<'_>,
) -> Result<(), PublishError> {
    if definition.schema != SCHEMA_VERSION {
        return Err(PublishError::UnsupportedSchema {
            schema: definition.schema,
            expected: SCHEMA_VERSION,
        });
    }
    if definition.nodes.is_empty() {
        return Err(PublishError::NoNodes);
    }
    if definition.nodes.len() > MAX_NODES {
        return Err(PublishError::TooManyNodes {
            count: definition.nodes.len(),
            cap: MAX_NODES,
        });
    }

    let mut by_id: BTreeMap<&str, &Node> = BTreeMap::new();
    for (index, node) in definition.nodes.iter().enumerate() {
        if node.id.trim().is_empty() {
            return Err(PublishError::BlankNodeId { index });
        }
        if node.id.chars().count() > 64 || !node.id.chars().all(|c| !c.is_control() && c != ' ') {
            return Err(PublishError::BadNodeId {
                id: node.id.clone(),
            });
        }
        if by_id.insert(&node.id, node).is_some() {
            return Err(PublishError::DuplicateNodeId {
                id: node.id.clone(),
            });
        }
    }

    for node in &definition.nodes {
        validate_node(node, facts)?;
    }

    // Edges resolve, do not self-loop, and carry a branch only from the kind that owns it.
    for (index, edge) in definition.edges.iter().enumerate() {
        let Some(from) = by_id.get(edge.from.as_str()) else {
            return Err(PublishError::UnknownEndpoint {
                index,
                id: edge.from.clone(),
            });
        };
        if !by_id.contains_key(edge.to.as_str()) {
            return Err(PublishError::UnknownEndpoint {
                index,
                id: edge.to.clone(),
            });
        }
        if edge.from == edge.to {
            return Err(PublishError::SelfEdge {
                index,
                node: edge.from.clone(),
            });
        }
        if let Some(when) = edge.when {
            match from.kind {
                NodeKindTag::Condition if !matches!(when, EdgeWhen::True | EdgeWhen::False) => {
                    return Err(PublishError::ForeignBranch {
                        node: from.id.clone(),
                        kind: from.kind.as_str(),
                        first: "true",
                        second: "false",
                    });
                }
                NodeKindTag::Loop if !matches!(when, EdgeWhen::Each | EdgeWhen::Done) => {
                    return Err(PublishError::ForeignBranch {
                        node: from.id.clone(),
                        kind: from.kind.as_str(),
                        first: "each",
                        second: "done",
                    });
                }
                NodeKindTag::Condition | NodeKindTag::Loop => {}
                kind => {
                    return Err(PublishError::WhenOnBranchlessNode {
                        index,
                        node: from.id.clone(),
                        kind: kind.as_str(),
                        when: when.as_str(),
                    });
                }
            }
        }
    }

    // Every condition has exactly `true`/`false`; every loop exactly `each`/`done`.
    let mut branches: BTreeMap<&str, BTreeSet<EdgeWhen>> = BTreeMap::new();
    for edge in &definition.edges {
        if let Some(when) = edge.when {
            let set = branches.entry(edge.from.as_str()).or_default();
            if !set.insert(when) {
                return Err(PublishError::DuplicateBranch {
                    node: edge.from.clone(),
                    when: when.as_str(),
                });
            }
        }
    }
    for node in &definition.nodes {
        let declared = branches.get(node.id.as_str()).cloned().unwrap_or_default();
        match node.kind {
            NodeKindTag::Condition => {
                require_branches(node, &declared, EdgeWhen::True, EdgeWhen::False)?;
            }
            NodeKindTag::Loop => {
                require_branches(node, &declared, EdgeWhen::Each, EdgeWhen::Done)?;
            }
            _ => {
                if !declared.is_empty() {
                    return Err(PublishError::ForeignBranch {
                        node: node.id.clone(),
                        kind: node.kind.as_str(),
                        first: "true",
                        second: "false",
                    });
                }
            }
        }
    }

    // Every cycle must pass through a `loop`: remove the loop nodes and the graph must be acyclic.
    // This is the rule that actually admits **nested** loops — an SCC that contains two loops is legal
    // precisely because every cycle through it is bounded; a cycle that survives the removal is not.
    let mut without_loops = definition.clone();
    without_loops
        .nodes
        .retain(|node| node.kind != NodeKindTag::Loop);
    if let Some(members) = cycles(&without_loops).into_iter().next() {
        return Err(PublishError::CycleWithoutLoop { nodes: members });
    }
    // A loop's `done` edge must leave its body; otherwise `done` re-enters the region it just repeated.
    for loop_node in definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKindTag::Loop)
    {
        let body = body_nodes(definition, &loop_node.id);
        if let Some(done) = definition
            .edges
            .iter()
            .find(|edge| edge.from == loop_node.id && edge.when == Some(EdgeWhen::Done))
            && body.contains(&done.to)
        {
            return Err(PublishError::LoopDoneInsideCycle {
                node: loop_node.id.clone(),
            });
        }
    }

    // Start: the one node with no inbound edge once a loop's body→loop back-edges are set aside. A loop
    // is entered at the loop node; the edge that closes its cycle is not an entry.
    let starts = start_nodes(definition);
    match starts.len() {
        0 => return Err(PublishError::NoStart),
        1 => {}
        count => return Err(PublishError::MultipleStarts { count }),
    }
    let start = starts[0].as_str();

    // Reachability over the same edges (back-edges included: the body is reachable through `each`).
    let mut reached: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        if !reached.insert(id) {
            continue;
        }
        for edge in definition.edges.iter().filter(|edge| edge.from == id) {
            stack.push(edge.to.as_str());
        }
    }
    if let Some(missing) = definition
        .nodes
        .iter()
        .find(|node| !reached.contains(node.id.as_str()))
    {
        return Err(PublishError::Unreachable {
            node: missing.id.clone(),
        });
    }

    // Loop nesting: a loop inside another loop's body must stay within the cap.
    for node in definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKindTag::Loop)
    {
        let depth = loop_nesting(definition, &node.id);
        if depth > MAX_LOOP_NESTING {
            return Err(PublishError::LoopNestingTooDeep {
                node: node.id.clone(),
                depth,
                cap: MAX_LOOP_NESTING,
            });
        }
    }

    Ok(())
}

fn require_branches(
    node: &Node,
    declared: &BTreeSet<EdgeWhen>,
    first: EdgeWhen,
    second: EdgeWhen,
) -> Result<(), PublishError> {
    for expected in [first, second] {
        if !declared.contains(&expected) {
            return Err(PublishError::MissingBranch {
                node: node.id.clone(),
                kind: node.kind.as_str(),
                missing: expected.as_str(),
            });
        }
    }
    if declared
        .iter()
        .any(|when| *when != first && *when != second)
    {
        return Err(PublishError::ForeignBranch {
            node: node.id.clone(),
            kind: node.kind.as_str(),
            first: first.as_str(),
            second: second.as_str(),
        });
    }
    Ok(())
}

fn validate_node(node: &Node, facts: &PublishFacts<'_>) -> Result<(), PublishError> {
    match node.kind {
        NodeKindTag::Action => {
            let action = require(node, node.action.as_deref(), "action")?;
            let dots = action.matches('.').count();
            if dots != 1 || action.starts_with('.') || action.ends_with('.') {
                return Err(PublishError::BadActionName {
                    node: node.id.clone(),
                    action: action.to_owned(),
                });
            }
            if action.starts_with("process_runs.") || action.starts_with("workflow_runs.") {
                return Err(PublishError::SelfReference {
                    node: node.id.clone(),
                    action: action.to_owned(),
                });
            }
            if action == MODEL_ACTION {
                return Err(PublishError::DirectModelCall {
                    node: node.id.clone(),
                    action: action.to_owned(),
                });
            }
            let Some(known) = facts.actions.iter().find(|known| known.name == action) else {
                return Err(PublishError::UnknownAction {
                    node: node.id.clone(),
                    action: action.to_owned(),
                });
            };
            if let Some(text) = node.on_error.as_deref()
                && OnError::parse(text).is_none()
            {
                return Err(PublishError::BadOnError {
                    node: node.id.clone(),
                    value: text.to_owned(),
                });
            }
            if node.on_error() == OnError::Retry && !known.idempotent {
                return Err(PublishError::RetryOnNonIdempotent {
                    node: node.id.clone(),
                    action: action.to_owned(),
                });
            }
            let attempts = node.max_attempts();
            if !(1..=MAX_ATTEMPTS_CAP).contains(&attempts) {
                return Err(PublishError::AttemptsOutOfRange {
                    node: node.id.clone(),
                    max_attempts: attempts,
                    cap: MAX_ATTEMPTS_CAP,
                });
            }
            let timeout = node.timeout_ms();
            if !(1..=MAX_TIMEOUT_MS).contains(&timeout) {
                return Err(PublishError::TimeoutOutOfRange {
                    node: node.id.clone(),
                    timeout_ms: timeout,
                    cap: MAX_TIMEOUT_MS,
                });
            }
            if let Some(template) = node.input_template.as_ref() {
                validate_template(node, template)?;
            }
        }
        NodeKindTag::Procedure => {
            let key = require(node, node.procedure_key.as_deref(), "procedure_key")?;
            if !facts.procedures.iter().any(|known| known == key) {
                return Err(PublishError::UnknownProcedure {
                    node: node.id.clone(),
                    key: key.to_owned(),
                });
            }
            if let Some(inputs) = node.inputs.as_ref() {
                validate_template(node, inputs)?;
            }
        }
        NodeKindTag::Condition => {
            let Some(predicate) = node.predicate.as_ref() else {
                return Err(PublishError::MissingField {
                    node: node.id.clone(),
                    kind: node.kind.as_str(),
                    field: "predicate",
                });
            };
            validate_predicate(node, predicate)?;
        }
        NodeKindTag::Loop => {
            let items = node
                .items
                .as_ref()
                .ok_or_else(|| PublishError::MissingField {
                    node: node.id.clone(),
                    kind: node.kind.as_str(),
                    field: "items",
                })?;
            validate_template(node, items)?;
            let iterations = node
                .max_iterations
                .ok_or_else(|| PublishError::MissingField {
                    node: node.id.clone(),
                    kind: node.kind.as_str(),
                    field: "max_iterations",
                })?;
            if !(1..=MAX_LOOP_ITERATIONS).contains(&iterations) {
                return Err(PublishError::BadIterations {
                    node: node.id.clone(),
                    max_iterations: iterations,
                    cap: MAX_LOOP_ITERATIONS,
                });
            }
        }
        NodeKindTag::Gate => {
            let gate = require(node, node.gate_kind.as_deref(), "gate_kind")?;
            if !facts.gates.iter().any(|known| known == gate) {
                return Err(PublishError::UnknownGate {
                    node: node.id.clone(),
                    gate: gate.to_owned(),
                });
            }
        }
    }
    Ok(())
}

fn require<'a>(
    node: &Node,
    value: Option<&'a str>,
    field: &'static str,
) -> Result<&'a str, PublishError> {
    match value {
        Some(text) if !text.trim().is_empty() => Ok(text),
        _ => Err(PublishError::MissingField {
            node: node.id.clone(),
            kind: node.kind.as_str(),
            field,
        }),
    }
}

/// Templates are checked for their **root vocabulary** only. A workflow's execution order is not fixed
/// until it runs (a loop repeats its body, a branch may skip one), so `steps.<n>` cannot be checked
/// against a static position here; a genuinely forward reference fails typed at run time. S11's
/// positional check applies to `procedure_step`, which has a fixed order.
fn validate_template(node: &Node, template: &Value) -> Result<(), PublishError> {
    template::validate_template(template, usize::MAX).map_err(|source| PublishError::Template {
        node: node.id.clone(),
        source,
    })
}

fn validate_predicate(node: &Node, predicate: &Predicate) -> Result<(), PublishError> {
    match predicate {
        Predicate::And { of } | Predicate::Or { of } => {
            if of.is_empty() {
                return Err(PublishError::EmptyPredicate {
                    node: node.id.clone(),
                });
            }
            for child in of {
                validate_predicate(node, child)?;
            }
        }
        Predicate::Not { of } => validate_predicate(node, of)?,
        Predicate::Eq { left, right }
        | Predicate::Ne { left, right }
        | Predicate::Lt { left, right }
        | Predicate::Le { left, right }
        | Predicate::Gt { left, right }
        | Predicate::Ge { left, right }
        | Predicate::In { left, right } => {
            validate_template(node, left)?;
            validate_template(node, right)?;
        }
        Predicate::Empty { left } | Predicate::Exists { left } => validate_template(node, left)?,
    }
    Ok(())
}

/// The nodes with no inbound edge once each loop's body→loop back-edge is set aside: a loop is entered
/// at the loop node, and the edge that closes its cycle is not an entry.
pub(crate) fn start_nodes(definition: &Definition) -> Vec<String> {
    let bodies: BTreeMap<&str, BTreeSet<String>> = definition
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKindTag::Loop)
        .map(|node| (node.id.as_str(), body_nodes(definition, &node.id)))
        .collect();
    let mut inbound: BTreeMap<&str, usize> = definition
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), 0))
        .collect();
    for edge in &definition.edges {
        // An edge into a loop is a cycle-closing return — not an entry — exactly when it comes from
        // inside that loop's own body. This is what gives nested loops one start: `L1.each -> L2` is an
        // entry (L1 is outside L2's body) while `L3.done -> L2` is the return to L2.
        let back_edge = definition
            .node(&edge.to)
            .is_some_and(|target| target.kind == NodeKindTag::Loop)
            && bodies
                .get(edge.to.as_str())
                .is_some_and(|body| body.contains(&edge.from));
        if !back_edge {
            *inbound.entry(edge.to.as_str()).or_default() += 1;
        }
    }
    let mut starts: Vec<String> = inbound
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| (*id).to_owned())
        .collect();
    starts.sort();
    starts
}

/// The one start node, when the document has exactly one.
pub(crate) fn start_node(definition: &Definition) -> Option<String> {
    let starts = start_nodes(definition);
    (starts.len() == 1).then(|| starts[0].clone())
}

/// Strongly connected components with more than one node — the cycles. A self-edge is refused before
/// this runs, so a singleton is never a cycle.
pub(crate) fn cycles(definition: &Definition) -> Vec<Vec<String>> {
    let ids: Vec<&str> = definition
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    let index_of: BTreeMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    for edge in &definition.edges {
        if let (Some(from), Some(to)) = (
            index_of.get(edge.from.as_str()),
            index_of.get(edge.to.as_str()),
        ) {
            adjacency[*from].push(*to);
        }
    }

    // Iterative Tarjan: 200 nodes, but one stack frame per node is not worth the recursion risk.
    let mut next_index = 0usize;
    let mut indices: Vec<Option<usize>> = vec![None; ids.len()];
    let mut lowlink: Vec<usize> = vec![0; ids.len()];
    let mut on_stack: Vec<bool> = vec![false; ids.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut components: Vec<Vec<String>> = Vec::new();

    for root in 0..ids.len() {
        if indices[root].is_some() {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((node, child)) = work.pop() {
            if child == 0 {
                indices[node] = Some(next_index);
                lowlink[node] = next_index;
                next_index += 1;
                stack.push(node);
                on_stack[node] = true;
            }
            let mut descended = false;
            for next in adjacency[node].iter().skip(child) {
                if indices[*next].is_none() {
                    work.push((node, child + 1));
                    work.push((*next, 0));
                    descended = true;
                    break;
                } else if on_stack[*next] {
                    lowlink[node] = lowlink[node].min(indices[*next].unwrap_or(usize::MAX));
                }
            }
            if descended {
                continue;
            }
            if lowlink[node] == indices[node].unwrap_or(usize::MAX) {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(ids[member].to_owned());
                    if member == node {
                        break;
                    }
                }
                if component.len() > 1 {
                    component.sort();
                    components.push(component);
                }
            }
            if let Some((parent, _)) = work.last().copied() {
                lowlink[parent] = lowlink[parent].min(lowlink[node]);
            }
        }
    }
    components.sort();
    components
}

/// The nodes one loop repeats: everything reachable from its `each` target without entering the loop
/// again. A nested loop is inside its parent's body, and this is the region the engine resets per
/// iteration.
pub(crate) fn body_nodes(definition: &Definition, loop_id: &str) -> BTreeSet<String> {
    let Some(node) = definition.node(loop_id) else {
        return BTreeSet::new();
    };
    if node.kind != NodeKindTag::Loop {
        return BTreeSet::new();
    }
    let Some(each) = definition
        .edges
        .iter()
        .find(|edge| edge.from == loop_id && edge.when == Some(EdgeWhen::Each))
    else {
        return BTreeSet::new();
    };
    let mut body = BTreeSet::new();
    let mut stack = vec![each.to.as_str()];
    while let Some(id) = stack.pop() {
        if id == loop_id || !body.insert(id.to_owned()) {
            continue;
        }
        for edge in definition.edges.iter().filter(|edge| edge.from == id) {
            stack.push(edge.to.as_str());
        }
    }
    body
}

/// How deep loops nest inside `loop_id`, counting `loop_id` itself as one. Bodies strictly shrink as
/// the recursion descends, so this terminates.
fn loop_nesting(definition: &Definition, loop_id: &str) -> usize {
    1 + body_nodes(definition, loop_id)
        .iter()
        .filter(|id| {
            definition
                .node(id)
                .is_some_and(|node| node.kind == NodeKindTag::Loop)
        })
        .map(|id| loop_nesting(definition, id))
        .max()
        .unwrap_or(0)
}
