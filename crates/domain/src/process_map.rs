#![forbid(unsafe_code)]
//! The **process map** — the user-facing DAG (SPEC D30, CONTRACT §21).
//!
//! It is a versioned, declarative document bound to a project type. Nodes are processes,
//! capabilities, phases or milestones; edges are flow. It **cannot reorder a step** and nothing
//! executes from it: the process's step positions are its own order. The UI renders the map and
//! overlays each project's live position from its runs.
//!
//! The validator is pure and resolves every node's `ref` against the facts the caller supplies
//! (published process keys, capability codes, the project type's phases and milestones), so a
//! map that points at a retired process is refused at publish rather than rendered as a hole.

use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

/// A node's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A published process (`process.key`).
    Process,
    /// A capability (`capability.code`).
    Capability,
    /// One of the project type's phases: a code in
    /// `catalogue_entry(kind='project_type').meta.phases`, whose matching rows are
    /// `catalogue_entry(kind='phase')` (there are no `project_type_phase` rows).
    Phase,
    /// A milestone (`catalogue_entry(kind='milestone_kind').code`).
    Milestone,
}

impl NodeKind {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Capability => "capability",
            Self::Phase => "phase",
            Self::Milestone => "milestone",
        }
    }
}

/// One node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapNode {
    /// Unique within the document.
    pub id: String,
    /// What the node denotes.
    pub kind: NodeKind,
    /// The key/code the kind resolves against.
    #[serde(rename = "ref")]
    pub reference: String,
    /// What the user reads.
    #[serde(default)]
    pub label: String,
    /// Optional canvas coordinates `{x, y}`; the UI places unpositioned nodes.
    #[serde(default)]
    pub position: Option<serde_json::Value>,
}

/// One directed edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Optional edge label.
    #[serde(default)]
    pub label: String,
}

/// The whole document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct MapDoc {
    /// The document schema version.
    #[serde(default)]
    pub schema: String,
    /// The nodes.
    #[serde(default)]
    pub nodes: Vec<MapNode>,
    /// The edges.
    #[serde(default)]
    pub edges: Vec<MapEdge>,
}

/// The facts a publish resolves node references against.
#[derive(Clone, Debug, Default)]
pub struct MapFacts {
    /// Published process keys.
    pub processes: BTreeSet<String>,
    /// Capability codes.
    pub capabilities: BTreeSet<String>,
    /// The project type's phase codes.
    pub phases: BTreeSet<String>,
    /// The project type's milestone codes.
    pub milestones: BTreeSet<String>,
}

/// Why a map fails to publish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapError {
    /// No nodes: an empty map draws nothing.
    NoNodes,
    /// Two nodes share an id.
    DuplicateNode(String),
    /// A node id is empty.
    EmptyNodeId,
    /// A node's `ref` does not resolve for its kind.
    UnresolvedNode {
        id: String,
        kind: &'static str,
        reference: String,
    },
    /// An edge names a node that is not in the document.
    DanglingEdge { from: String, to: String },
    /// An edge starts and ends at the same node.
    SelfEdge(String),
    /// Two edges are identical.
    DuplicateEdge { from: String, to: String },
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoNodes => write!(f, "the map has no nodes"),
            Self::DuplicateNode(id) => write!(f, "two nodes share the id {id:?}"),
            Self::EmptyNodeId => write!(f, "a node id is empty"),
            Self::UnresolvedNode {
                id,
                kind,
                reference,
            } => {
                write!(f, "node {id:?} ({kind}) does not resolve: {reference:?}")
            }
            Self::DanglingEdge { from, to } => {
                write!(
                    f,
                    "edge {from:?} -> {to:?} names a node that is not in the map"
                )
            }
            Self::SelfEdge(id) => write!(f, "edge {id:?} -> {id:?} is a self-edge"),
            Self::DuplicateEdge { from, to } => {
                write!(f, "two edges {from:?} -> {to:?} are identical")
            }
        }
    }
}

/// Validate a document and resolve every node against `facts`.
pub fn validate(doc: &MapDoc, facts: &MapFacts) -> Result<(), Vec<MapError>> {
    let mut errors = Vec::new();
    if doc.nodes.is_empty() {
        errors.push(MapError::NoNodes);
    }

    let mut ids = HashSet::new();
    for node in &doc.nodes {
        if node.id.trim().is_empty() {
            errors.push(MapError::EmptyNodeId);
            continue;
        }
        if !ids.insert(node.id.clone()) {
            errors.push(MapError::DuplicateNode(node.id.clone()));
        }
        let (set, kind) = match node.kind {
            NodeKind::Process => (&facts.processes, "process"),
            NodeKind::Capability => (&facts.capabilities, "capability"),
            NodeKind::Phase => (&facts.phases, "phase"),
            NodeKind::Milestone => (&facts.milestones, "milestone"),
        };
        if !set.contains(&node.reference) {
            errors.push(MapError::UnresolvedNode {
                id: node.id.clone(),
                kind,
                reference: node.reference.clone(),
            });
        }
    }

    let mut seen = HashSet::new();
    for edge in &doc.edges {
        if edge.from == edge.to {
            errors.push(MapError::SelfEdge(edge.from.clone()));
        }
        if !ids.contains(&edge.from) || !ids.contains(&edge.to) {
            errors.push(MapError::DanglingEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            });
        }
        if !seen.insert((edge.from.clone(), edge.to.clone())) {
            errors.push(MapError::DuplicateEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facts() -> MapFacts {
        MapFacts {
            processes: BTreeSet::from(["analysis_epic.DOING".to_owned()]),
            capabilities: BTreeSet::from(["analysis".to_owned()]),
            phases: BTreeSet::from(["design".to_owned()]),
            milestones: BTreeSet::from(["m1".to_owned()]),
        }
    }

    fn doc(raw: serde_json::Value) -> MapDoc {
        serde_json::from_value(raw).expect("map document")
    }

    #[test]
    fn a_resolved_document_passes() {
        let d = doc(json!({
            "nodes": [
                {"id": "a", "kind": "process", "ref": "analysis_epic.DOING", "label": "Do"},
                {"id": "b", "kind": "capability", "ref": "analysis", "label": "Analysis"}
            ],
            "edges": [{"from": "a", "to": "b"}]
        }));
        assert!(validate(&d, &facts()).is_ok());
    }

    #[test]
    fn an_unresolved_node_is_refused() {
        let d = doc(json!({
            "nodes": [{"id": "a", "kind": "process", "ref": "retired.flow"}]
        }));
        let errors = validate(&d, &facts()).expect_err("unresolved");
        assert!(matches!(errors[0], MapError::UnresolvedNode { .. }));
    }

    #[test]
    fn the_map_is_a_dag_not_a_control_flow() {
        let d = doc(json!({
            "nodes": [
                {"id": "a", "kind": "process", "ref": "analysis_epic.DOING"},
                {"id": "b", "kind": "capability", "ref": "analysis"}
            ],
            "edges": [
                {"from": "a", "to": "a"},
                {"from": "a", "to": "b"},
                {"from": "a", "to": "b"},
                {"from": "a", "to": "ghost"}
            ]
        }));
        let errors = validate(&d, &facts()).expect_err("bad edges");
        assert!(errors.contains(&MapError::SelfEdge("a".to_owned())));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, MapError::DuplicateEdge { .. }))
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, MapError::DanglingEdge { .. }))
        );
    }

    #[test]
    fn an_empty_map_is_refused() {
        let errors = validate(&MapDoc::default(), &facts()).expect_err("empty");
        assert_eq!(errors, vec![MapError::NoNodes]);
    }

    #[test]
    fn an_unknown_kind_does_not_parse() {
        let parsed: Result<MapDoc, _> = serde_json::from_value(json!({
            "nodes": [{"id": "a", "kind": "phase_gate", "ref": "x"}]
        }));
        assert!(parsed.is_err(), "the node kinds are closed");
    }
}
