#![forbid(unsafe_code)]
//! S15 — the capability aggregate's pure rules (SPEC D20, CONTRACT §17).
//!
//! A capability is a business capability (`analysis`, `architecture`, …) that may be bound to a
//! knowledge pack and executed by a configured agent. **`capability_agent` is configuration, not a
//! loop**: this module validates the executor spec, it never calls a model. S13's `agent.run` is
//! the only loop, and it receives the resolved spec in its step input.
//!
//! Nothing here reads a database or the registry. `allowed_actions` is checked for *grammar* only;
//! whether an action is registered, and whether its pack is published, is readiness — a run-time
//! check (S15 step 12), not a write refusal, so a capability may name `agent.run` before S13 lands.

use serde::{Deserialize, Serialize};

/// The slice's ceiling on `capability_agent.max_iterations` (S15 §5). The column's CHECK is the
/// database's copy; this is the one the action layer refuses with a 422.
pub const MAX_ITERATIONS: i32 = 20;

/// The slice's ceiling on `capability_agent.timeout_secs`.
pub const MAX_TIMEOUT_SECS: i32 = 300;

/// A capability's lifecycle. `retired` is terminal: a retired capability is history, and a new
/// capability that reuses the code would silently inherit its runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Editable; its agents and knowledge bindings may change.
    Draft,
    /// Resolvable by a run.
    Active,
    /// No longer resolvable; live runs finish against what they resolved.
    Retired,
}

impl CapabilityStatus {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }

    /// Parse the column value; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "retired" => Some(Self::Retired),
            _ => None,
        }
    }

    /// Whether `self → next` is legal. Draft may be activated or retired; active may be retired;
    /// retired is terminal. Active does **not** go back to draft: unpublishing a capability that
    /// runs already resolved would make the recorded `run.capability_id` point at a draft.
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Active)
                | (Self::Draft, Self::Retired)
                | (Self::Active, Self::Retired)
        )
    }
}

/// The ontology edge between two capabilities. Only [`RelationKind::Requires`] is acyclic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// `from` cannot be executed without `to`.
    Requires,
    /// `from` makes `to` easier or possible.
    Enables,
    /// `from` is a concern for `to`.
    Concern,
}

impl RelationKind {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requires => "requires",
            Self::Enables => "enables",
            Self::Concern => "concern",
        }
    }

    /// Parse the column value; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "requires" => Some(Self::Requires),
            "enables" => Some(Self::Enables),
            "concern" => Some(Self::Concern),
            _ => None,
        }
    }
}

/// Why a capability, an agent spec or a relation set was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityError {
    /// The status is not one of the three column values.
    #[error("unknown capability status {0:?}")]
    UnknownStatus(String),
    /// The relation kind is not one of the three column values.
    #[error("unknown relation kind {0:?}")]
    UnknownRelation(String),
    /// The status machine refuses the move.
    #[error("a {from} capability cannot become {to}")]
    IllegalTransition {
        /// The current status.
        from: &'static str,
        /// The requested status.
        to: &'static str,
    },
    /// A capability cannot relate to itself (the database's CHECK agrees).
    #[error("capability {0:?} cannot relate to itself")]
    SelfRelation(String),
    /// The `requires` edges form a cycle, so no capability in it can ever be ready.
    #[error("requires-cycle: {0:?} reaches itself through {1:?}")]
    RelationCycle(String, String),
    /// The action name is not `<resource>.<verb>`.
    #[error("{0:?} is not an action name of the form <resource>.<verb>")]
    BadActionName(String),
    /// The executor spec names no model.
    #[error("model_ref must not be blank")]
    BlankModelRef,
    /// The same action appears twice in `allowed_actions`.
    #[error("{0:?} appears twice in allowed_actions")]
    DuplicateAction(String),
    /// `max_iterations` is outside `1..=20`.
    #[error("max_iterations must be between 1 and {MAX_ITERATIONS}, got {0}")]
    IterationsOutOfRange(i32),
    /// `timeout_secs` is outside `1..=300`.
    #[error("timeout_secs must be between 1 and {MAX_TIMEOUT_SECS}, got {0}")]
    TimeoutOutOfRange(i32),
    /// `prompt_ref` is not `<name>@<version>`.
    #[error("prompt_ref must be <name>@<version>, got {0:?}")]
    BadPromptRef(String),
}

/// The parsed `<name>@<version>` of a built-in prompt. v1 resolves built-ins only; a tenant-authored
/// prompt store is a later slice (S15 §8 trap 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptRef {
    /// The prompt's name (`prompt_audit`, `copilot_system`, …).
    pub name: String,
    /// The built-in version, dense from 1.
    pub version: u32,
}

impl PromptRef {
    /// Parse `<name>@<version>`; the name is lower-case ASCII, digits and underscores, the version
    /// is a positive integer with no leading zero.
    pub fn parse(text: &str) -> Result<Self, CapabilityError> {
        let (name, version) = text
            .split_once('@')
            .ok_or_else(|| CapabilityError::BadPromptRef(text.to_owned()))?;
        let name_ok = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        let version_ok = !version.is_empty()
            && version.bytes().all(|b| b.is_ascii_digit())
            && version.len() == version.trim_start_matches('0').len()
            && version != "0";
        if !name_ok || !version_ok {
            return Err(CapabilityError::BadPromptRef(text.to_owned()));
        }
        let version = version
            .parse::<u32>()
            .map_err(|_| CapabilityError::BadPromptRef(text.to_owned()))?;
        Ok(Self {
            name: name.to_owned(),
            version,
        })
    }

    /// The canonical spelling.
    pub fn as_string(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// A capability agent's configurable surface, as it arrives from a `create`/`update` body.
#[derive(Clone, Debug)]
pub struct AgentConfig<'a> {
    /// The model's registry reference (`model_ref`); the store/registry decides what resolves.
    pub model_ref: &'a str,
    /// A built-in `<name>@<version>`, or `None` to use the action's default.
    pub prompt_ref: Option<&'a str>,
    /// The actions the executor may invoke. Grammar only here; registration is readiness.
    pub allowed_actions: &'a [String],
    /// The loop's iteration ceiling.
    pub max_iterations: i32,
    /// The loop's wall-clock ceiling.
    pub timeout_secs: i32,
}

/// Validate an executor spec. The database's CHECKs are the same bounds; this runs first so a bad
/// body is a typed `422`, not a constraint violation surfacing as a 500.
pub fn validate_agent_config(config: &AgentConfig<'_>) -> Result<(), CapabilityError> {
    if config.model_ref.trim().is_empty() {
        return Err(CapabilityError::BlankModelRef);
    }
    if !(1..=MAX_ITERATIONS).contains(&config.max_iterations) {
        return Err(CapabilityError::IterationsOutOfRange(config.max_iterations));
    }
    if !(1..=MAX_TIMEOUT_SECS).contains(&config.timeout_secs) {
        return Err(CapabilityError::TimeoutOutOfRange(config.timeout_secs));
    }
    if let Some(prompt_ref) = config.prompt_ref {
        PromptRef::parse(prompt_ref)?;
    }
    let mut seen = std::collections::BTreeSet::new();
    for action in config.allowed_actions {
        if !is_action_name(action) {
            return Err(CapabilityError::BadActionName(action.clone()));
        }
        if !seen.insert(action.as_str()) {
            return Err(CapabilityError::DuplicateAction(action.clone()));
        }
    }
    Ok(())
}

/// `<resource>.<verb>`: one dot, both sides non-empty, lower-case ASCII/digits/underscore.
fn is_action_name(text: &str) -> bool {
    let Some((resource, verb)) = text.split_once('.') else {
        return false;
    };
    let part_ok = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    part_ok(resource) && part_ok(verb)
}

/// One `requires` edge as `(from, to)`, both capability codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationEdge {
    /// The capability that requires.
    pub from: String,
    /// The capability it requires.
    pub to: String,
    /// The edge kind.
    pub kind: RelationKind,
}

/// Refuse a relation set whose `requires` edges form a cycle (or an edge to itself). A cycle makes
/// every capability in it permanently unready; it is a write refusal, not a readiness block.
pub fn validate_relations(edges: &[RelationEdge]) -> Result<(), CapabilityError> {
    let mut adjacency: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for edge in edges {
        if edge.from == edge.to {
            return Err(CapabilityError::SelfRelation(edge.from.clone()));
        }
        if edge.kind == RelationKind::Requires {
            adjacency
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }
    }

    // DFS with a colour map: visiting a grey node is a back edge, i.e. a cycle. The graph is a
    // handful of capability codes, so recursion depth is bounded by the vocabulary, not by input.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Colour {
        White,
        Grey,
        Black,
    }

    fn visit<'a>(
        node: &'a str,
        adjacency: &std::collections::BTreeMap<&'a str, Vec<&'a str>>,
        colour: &mut std::collections::BTreeMap<&'a str, Colour>,
        start: &str,
    ) -> Result<(), CapabilityError> {
        colour.insert(node, Colour::Grey);
        for next in adjacency.get(node).into_iter().flatten() {
            match colour.get(next).copied() {
                Some(Colour::Grey) => {
                    return Err(CapabilityError::RelationCycle(
                        start.to_owned(),
                        (*next).to_owned(),
                    ));
                }
                Some(Colour::Black) => {}
                _ => visit(next, adjacency, colour, start)?,
            }
        }
        colour.insert(node, Colour::Black);
        Ok(())
    }

    let mut colour: std::collections::BTreeMap<&str, Colour> = std::collections::BTreeMap::new();
    for node in adjacency.keys() {
        colour.entry(node).or_insert(Colour::White);
        for target in adjacency.get(node).into_iter().flatten() {
            colour.entry(target).or_insert(Colour::White);
        }
    }
    for start in colour.keys().copied().collect::<Vec<_>>() {
        if colour.get(start) == Some(&Colour::White) {
            visit(start, &adjacency, &mut colour, start)?;
        }
    }
    Ok(())
}

/// Why a capability is not ready to start a run. S15 step 12 computes these; the same projection is
/// returned by `capabilities.get` and carried in the `409 capability_not_ready` the start guard
/// refuses with before writing a run or step row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ReadinessBlocker {
    /// The resolved capability row no longer exists.
    CapabilityMissing,
    /// The capability exists but is not `active`.
    CapabilityNotActive {
        /// The stored status.
        status: String,
    },
    /// No `capability_agent` row is active for the capability.
    AgentMissing,
    /// The process step's bound executor exists but is not `active`.
    AgentNotActive {
        /// The stored status.
        status: String,
    },
    /// More than one active executor is configured; v1 requires exactly one.
    AgentAmbiguous,
    /// An `allowed_actions` entry is not in the live registry (e.g. `agent.run` before S13).
    ActionNotRegistered {
        /// The unregistered action name.
        action: String,
    },
    /// A `required` knowledge pack is not published at the pinned version (or has no leaves).
    PackNotPublished {
        /// The pack key.
        pack_key: String,
        /// The pinned version.
        pack_version: i32,
    },
    /// A `required` knowledge pack could not be reached: transport, auth or a server fault.
    PackUnavailable {
        /// The pack key.
        pack_key: String,
        /// The pinned version.
        pack_version: i32,
    },
    /// The tenant has no usable strata configuration (no token secret, or no base URL).
    KnowledgeNotConfigured,
    /// The agent's `prompt_ref` does not resolve to an active catalogue prompt.
    PromptMissing {
        /// The unresolved ref.
        prompt_ref: String,
    },
}

impl ReadinessBlocker {
    /// The stable machine code the 409 body and `capabilities.get` carry.
    pub fn code(&self) -> &'static str {
        match self {
            Self::CapabilityMissing => "capability_missing",
            Self::CapabilityNotActive { .. } => "capability_not_active",
            Self::AgentMissing => "agent_missing",
            Self::AgentNotActive { .. } => "agent_not_active",
            Self::AgentAmbiguous => "agent_ambiguous",
            Self::ActionNotRegistered { .. } => "action_not_registered",
            Self::PackNotPublished { .. } => "pack_not_published",
            Self::PackUnavailable { .. } => "pack_unavailable",
            Self::KnowledgeNotConfigured => "knowledge_not_configured",
            // The catalogue's own refusal vocabulary (crates/actions/src/prompt/mod.rs), so the
            // pre-run guard and the executor's `prompt_unresolved` refusal agree.
            Self::PromptMissing { .. } => "prompt_unresolved",
        }
    }
}

/// The pre-run readiness projection (`capabilities.get` returns it; the start guard refuses on it).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readiness {
    /// The blocking reasons; empty means ready.
    pub blocking: Vec<ReadinessBlocker>,
}

impl Readiness {
    /// Whether the capability may start a run.
    pub fn is_ready(&self) -> bool {
        self.blocking.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: &str, to: &str, kind: RelationKind) -> RelationEdge {
        RelationEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            kind,
        }
    }

    #[test]
    fn capability_status_machine_has_one_way_transitions() {
        assert!(CapabilityStatus::Draft.can_transition_to(CapabilityStatus::Active));
        assert!(CapabilityStatus::Draft.can_transition_to(CapabilityStatus::Retired));
        assert!(CapabilityStatus::Active.can_transition_to(CapabilityStatus::Retired));
        assert!(!CapabilityStatus::Active.can_transition_to(CapabilityStatus::Draft));
        assert!(!CapabilityStatus::Retired.can_transition_to(CapabilityStatus::Draft));
        assert!(!CapabilityStatus::Retired.can_transition_to(CapabilityStatus::Active));
        assert!(!CapabilityStatus::Draft.can_transition_to(CapabilityStatus::Draft));
    }

    #[test]
    fn capability_statuses_and_relations_round_trip() {
        for status in [
            CapabilityStatus::Draft,
            CapabilityStatus::Active,
            CapabilityStatus::Retired,
        ] {
            assert_eq!(CapabilityStatus::parse(status.as_str()), Some(status));
        }
        for kind in [
            RelationKind::Requires,
            RelationKind::Enables,
            RelationKind::Concern,
        ] {
            assert_eq!(RelationKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(CapabilityStatus::parse("published"), None);
        assert_eq!(RelationKind::parse("depends_on"), None);
    }

    #[test]
    fn capability_iteration_and_timeout_caps_are_inclusive() {
        let actions: Vec<String> = vec!["documents.list".to_owned()];
        let ok = AgentConfig {
            model_ref: "model:primary",
            prompt_ref: None,
            allowed_actions: &actions,
            max_iterations: MAX_ITERATIONS,
            timeout_secs: MAX_TIMEOUT_SECS,
        };
        assert_eq!(validate_agent_config(&ok), Ok(()));

        let too_many = AgentConfig {
            max_iterations: MAX_ITERATIONS + 1,
            ..ok.clone()
        };
        assert_eq!(
            validate_agent_config(&too_many),
            Err(CapabilityError::IterationsOutOfRange(21))
        );

        let too_slow = AgentConfig {
            timeout_secs: MAX_TIMEOUT_SECS + 1,
            ..ok.clone()
        };
        assert_eq!(
            validate_agent_config(&too_slow),
            Err(CapabilityError::TimeoutOutOfRange(301))
        );

        let zero = AgentConfig {
            max_iterations: 0,
            ..ok
        };
        assert_eq!(
            validate_agent_config(&zero),
            Err(CapabilityError::IterationsOutOfRange(0))
        );
    }

    #[test]
    fn capability_prompt_ref_is_a_builtin_name_and_a_positive_version() {
        assert_eq!(
            PromptRef::parse("prompt_audit@1"),
            Ok(PromptRef {
                name: "prompt_audit".to_owned(),
                version: 1
            })
        );
        assert!(PromptRef::parse("Prompt_Audit@1").is_err());
        assert!(PromptRef::parse("prompt_audit").is_err());
        assert!(PromptRef::parse("prompt_audit@0").is_err());
        assert!(PromptRef::parse("prompt_audit@01").is_err());
        assert!(PromptRef::parse("prompt_audit@1@2").is_err());
        assert!(PromptRef::parse("@1").is_err());
    }

    #[test]
    fn capability_allowed_actions_are_checked_for_grammar_and_duplicates() {
        let ok: Vec<String> = vec![
            "agent.run".to_owned(),
            "documents.list".to_owned(),
            "runs.cancel".to_owned(),
        ];
        let config = AgentConfig {
            model_ref: "model:primary",
            prompt_ref: Some("prompt_audit@1"),
            allowed_actions: &ok,
            max_iterations: 1,
            timeout_secs: 1,
        };
        assert_eq!(validate_agent_config(&config), Ok(()));

        for bad in [
            "",
            "documents",
            "documents.",
            ".list",
            "documents.list.extra",
            "docs.list two",
        ] {
            let actions = vec![bad.to_owned()];
            let config = AgentConfig {
                allowed_actions: &actions,
                ..config.clone()
            };
            assert!(
                matches!(
                    validate_agent_config(&config),
                    Err(CapabilityError::BadActionName(_))
                ),
                "{bad:?} must be refused"
            );
        }

        let duplicate = vec!["runs.cancel".to_owned(), "runs.cancel".to_owned()];
        let config = AgentConfig {
            allowed_actions: &duplicate,
            ..config
        };
        assert_eq!(
            validate_agent_config(&config),
            Err(CapabilityError::DuplicateAction("runs.cancel".to_owned()))
        );
    }

    #[test]
    fn capability_requires_cycle_is_refused_but_enables_may_cycle() {
        let acyclic = [
            edge("analysis", "design", RelationKind::Requires),
            edge("design", "development", RelationKind::Requires),
        ];
        assert_eq!(validate_relations(&acyclic), Ok(()));

        let cyclic = [
            edge("analysis", "design", RelationKind::Requires),
            edge("design", "development", RelationKind::Requires),
            edge("development", "analysis", RelationKind::Requires),
        ];
        assert!(matches!(
            validate_relations(&cyclic),
            Err(CapabilityError::RelationCycle(_, _))
        ));

        let enabling_cycle = [
            edge("analysis", "design", RelationKind::Enables),
            edge("design", "analysis", RelationKind::Enables),
        ];
        assert_eq!(validate_relations(&enabling_cycle), Ok(()));

        let self_edge = [edge("analysis", "analysis", RelationKind::Requires)];
        assert_eq!(
            validate_relations(&self_edge),
            Err(CapabilityError::SelfRelation("analysis".to_owned()))
        );
    }

    #[test]
    fn capability_readiness_is_ready_exactly_when_nothing_blocks() {
        let mut readiness = Readiness::default();
        assert!(readiness.is_ready());
        readiness
            .blocking
            .push(ReadinessBlocker::ActionNotRegistered {
                action: "agent.run".to_owned(),
            });
        assert!(!readiness.is_ready());
        assert_eq!(readiness.blocking[0].code(), "action_not_registered");
    }

    #[test]
    fn every_readiness_blocker_has_a_stable_snake_case_code() {
        let cases = [
            (ReadinessBlocker::CapabilityMissing, "capability_missing"),
            (
                ReadinessBlocker::CapabilityNotActive {
                    status: "retired".to_owned(),
                },
                "capability_not_active",
            ),
            (ReadinessBlocker::AgentMissing, "agent_missing"),
            (ReadinessBlocker::AgentAmbiguous, "agent_ambiguous"),
            (
                ReadinessBlocker::ActionNotRegistered {
                    action: "agent.run".to_owned(),
                },
                "action_not_registered",
            ),
            (
                ReadinessBlocker::PackNotPublished {
                    pack_key: "capability-analysis".to_owned(),
                    pack_version: 2,
                },
                "pack_not_published",
            ),
            (
                ReadinessBlocker::PackUnavailable {
                    pack_key: "capability-analysis".to_owned(),
                    pack_version: 2,
                },
                "pack_unavailable",
            ),
            (
                ReadinessBlocker::KnowledgeNotConfigured,
                "knowledge_not_configured",
            ),
            (
                ReadinessBlocker::PromptMissing {
                    prompt_ref: "missing@1".to_owned(),
                },
                "prompt_unresolved",
            ),
        ];
        for (blocker, expected) in cases {
            assert_eq!(blocker.code(), expected, "{blocker:?}");
        }
    }
}
