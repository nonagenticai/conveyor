#![forbid(unsafe_code)]
//! A process is **executable documentation**: ordered registry actions on one card, with an input template
//! and an optional gate (S11 §5.3).
//!
//! Deliberately not a language. There is no precondition, no effect, no condition, no branch and no sub-step:
//! the step order IS the control flow, the action's JSON Schema decides what a step may be given, and the
//! board matrix decides where a card may move (S11 trap "PDL creeping back").
//!
//! This module owns the **publish-time** rules. It is pure: the caller supplies the live manifest's facts and
//! the tenant's gate codes, so nothing here reads a database or a registry.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::template::{self, TemplateError};

/// The application cap on `process_step.max_attempts`. The column only refuses `< 1`; the cap is policy and
/// lives here, where a publish can refuse it with a message (S11 §5.1).
pub const MAX_ATTEMPTS_CAP: i32 = 10;

/// The step primitive an `agent` step runs (S13 §5.4: "agent → `agent.run` with the resolved
/// process agent").
///
/// It is stored on `process_step.action` rather than left blank, because `run_step.action` is the
/// trace's record of what a step actually did and the executor dispatches the agent loop on it. One
/// spelling, three readers: publish validation, the seed loader and the executor. A `computed` step
/// may never name it — that would be a second way to say `agent`, and two representations is how the
/// catalogue became un-republishable (2026-11-10).
pub const AGENT_STEP_ACTION: &str = "agent.run";

/// How a step executes (CONTRACT §21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// The process agent runs the bounded `agent.run` loop over its tools.
    Agent,
    /// One registered action, invoked by the tool agent.
    Computed,
    /// An approval parks the run in `waiting_approval`; no action runs.
    Gate,
    /// A child run: a published process, or a published workflow (a tool-call sequence).
    Call,
}

impl StepKind {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Computed => "computed",
            Self::Gate => "gate",
            Self::Call => "call",
        }
    }

    /// Parse the stored spelling; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "agent" => Some(Self::Agent),
            "computed" => Some(Self::Computed),
            "gate" => Some(Self::Gate),
            "call" => Some(Self::Call),
            _ => None,
        }
    }
}

/// Who owns a step's work (CONTRACT §21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// The process agent (an `agent` step).
    ProcessAgent,
    /// A tool agent (the `computed` step's invoke path).
    ToolAgent,
    /// The engine itself (a gate).
    System,
    /// A person (a gate a human answers).
    Human,
}

impl Actor {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProcessAgent => "process_agent",
            Self::ToolAgent => "tool_agent",
            Self::System => "system",
            Self::Human => "human",
        }
    }

    /// Parse the stored spelling; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "process_agent" => Some(Self::ProcessAgent),
            "tool_agent" => Some(Self::ToolAgent),
            "system" => Some(Self::System),
            "human" => Some(Self::Human),
            _ => None,
        }
    }
}

/// What a failed step does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// End the run as failed.
    Fail,
    /// Mark the step `skipped` and continue with the next position.
    Continue,
    /// Append a new attempt at the same position, up to `max_attempts`.
    Retry,
}

impl OnError {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fail => "fail",
            Self::Continue => "continue",
            Self::Retry => "retry",
        }
    }

    /// Parse the column value; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fail" => Some(Self::Fail),
            "continue" => Some(Self::Continue),
            "retry" => Some(Self::Retry),
            _ => None,
        }
    }
}

/// A process's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    /// Editable; only one draft per `(tenant, key)`.
    Draft,
    /// Frozen; runs resolve against this version only.
    Published,
    /// Superseded by a newer published version, or explicitly retired. Live runs finish.
    Retired,
}

impl ProcessStatus {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Retired => "retired",
        }
    }
}

/// One step as it arrives from the caller, before it has a row.
///
/// `kind` is inferred when absent: a step with a `gate_kind` is a `gate`, anything else is
/// `computed`. An explicit kind always wins, and the validator refuses the combinations the
/// schema allows but the model does not (a gate with no code, a computed step with no action).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepInput {
    /// The registry action a `computed` step invokes; blank for every other kind.
    #[serde(default)]
    pub action: String,
    /// `agent | computed | gate | call`; absent = inferred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// `process_agent | tool_agent | system | human`; absent = the kind's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// The capability the step belongs to (a process agent's capability).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The `capability_agent(role='process_agent')` that runs an `agent` step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_agent_id: Option<Uuid>,
    /// The published SOP this step is judged against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_id: Option<Uuid>,
    /// The published process (or published workflow) a `call` step starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_process_key: Option<String>,
    /// The input template (S11 §5.4); `{}` when the action needs nothing.
    #[serde(default)]
    pub input_template: Value,
    /// A `catalogue_entry(kind='gate')` code; the step parks before running until it is approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_kind: Option<String>,
    /// `fail` (default), `continue` or `retry`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_result: Option<String>,
    /// How many attempts `retry` may make; default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<i32>,
    /// The dot-paths into this step's output that later steps may reference. Empty = it exposes
    /// nothing (S21 §6; the plan projection refuses a reference the producer does not declare).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
}

impl StepInput {
    /// The step's kind, inferred when the caller did not name one.
    pub fn kind(&self) -> Option<StepKind> {
        match self.kind.as_deref() {
            Some(text) => StepKind::parse(text),
            None if self.gate_kind.is_some() => Some(StepKind::Gate),
            None => Some(StepKind::Computed),
        }
    }

    /// The step's actor, defaulted by kind.
    pub fn actor(&self, kind: StepKind) -> Option<Actor> {
        match self.actor.as_deref() {
            Some(text) => Actor::parse(text),
            None => Some(match kind {
                StepKind::Agent => Actor::ProcessAgent,
                StepKind::Computed => Actor::ToolAgent,
                StepKind::Gate => Actor::System,
                StepKind::Call => Actor::System,
            }),
        }
    }
}

/// What the live manifest says about one action — the only fact publish needs from the registry.
#[derive(Clone, Debug)]
pub struct ActionFacts {
    /// The action name.
    pub name: String,
    /// Whether a second invocation with the same idempotency key is safe.
    pub idempotent: bool,
}

/// Why a process cannot be published.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PublishError {
    /// A process with no steps does nothing; publishing it would be a silent no-op run.
    #[error("a process must have at least one step")]
    NoSteps,
    /// Positions must be 1..n with no gap or repeat.
    #[error("step positions must be 1..n in order; position {position} is out of place")]
    PositionOutOfOrder {
        /// The position that broke the sequence.
        position: i32,
    },
    /// The action is blank.
    #[error("step {position}: the action name is blank")]
    BlankAction {
        /// The step's position.
        position: i32,
    },
    /// The action is not in the live manifest.
    #[error("step {position}: `{action}` is not a registered action")]
    UnknownAction {
        /// The step's position.
        position: i32,
        /// The name the process asked for.
        action: String,
    },
    /// The gate code is not an active `catalogue_entry(kind='gate')` row.
    #[error("step {position}: `{gate}` is not a gate code in this tenant's catalogue")]
    UnknownGate {
        /// The step's position.
        position: i32,
        /// The gate the process asked for.
        gate: String,
    },
    /// `retry` on an action that is not idempotent would repeat a side effect.
    #[error("step {position}: `{action}` is not idempotent, so it cannot be retried")]
    RetryOnNonIdempotent {
        /// The step's position.
        position: i32,
        /// The action the step names.
        action: String,
    },
    /// A step that starts a run would recurse.
    #[error("step {position}: `{action}` starts a run; a process may not invoke the run surface")]
    SelfReference {
        /// The step's position.
        position: i32,
        /// The action the step names.
        action: String,
    },
    /// `max_attempts` outside `1..=MAX_ATTEMPTS_CAP`.
    #[error("step {position}: max_attempts must be 1..={cap}, not {max_attempts}")]
    AttemptsOutOfRange {
        /// The step's position.
        position: i32,
        /// What the process asked for.
        max_attempts: i32,
        /// The cap that applies.
        cap: i32,
    },
    /// The action's name has the wrong shape for a registry key.
    #[error("step {position}: `{action}` is not a `<resource>.<verb>` action name")]
    BadActionName {
        /// The step's position.
        position: i32,
        /// The name the process asked for.
        action: String,
    },
    /// `on_result` is outside `fail | continue | retry`.
    ///
    /// This used to be reported as [`Self::BadActionName`], which told a caller their ACTION was wrong
    /// when the fault was in the error policy — and the same value is a database CHECK, so the message is
    /// the only thing that tells a client which member to fix.
    #[error("step {position}: on_result must be fail, continue or retry, not {value:?}")]
    BadOnResult {
        /// The step's position.
        position: i32,
        /// What the process asked for.
        value: String,
    },
    /// A template reference that cannot be legal at any run.
    #[error("step {position}: {source}")]
    Template {
        /// The step's position.
        position: i32,
        /// The resolver's own message.
        source: TemplateError,
    },
    /// `kind` is outside `agent | computed | gate | call`.
    #[error("step {position}: kind must be agent, computed, gate or call, not {value:?}")]
    BadKind {
        /// The step's position.
        position: i32,
        /// What the process asked for.
        value: String,
    },
    /// `actor` is outside the four roles.
    #[error(
        "step {position}: actor must be process_agent, tool_agent, system or human, not {value:?}"
    )]
    BadActor {
        /// The step's position.
        position: i32,
        /// What the process asked for.
        value: String,
    },
    /// A `computed` step names no action.
    #[error("step {position}: a computed step must name a registered action")]
    ComputedWithoutAction {
        /// The step's position.
        position: i32,
    },
    /// An `agent` step names no action: the agent loop is only ever spelled `agent.run`.
    #[error("step {position}: an agent step must name `agent.run`")]
    AgentWithoutAction {
        /// The step's position.
        position: i32,
    },
    /// An `agent` step named something other than the agent primitive.
    #[error("step {position}: an agent step runs `agent.run`, not `{action}`")]
    WrongAgentAction {
        /// The step's position.
        position: i32,
        /// The name the process asked for.
        action: String,
    },
    /// The step's SOP pins an eval this tenant does not have.
    #[error("step {position}: the SOP pins eval {eval_id}, which does not exist in this tenant")]
    UnknownEval {
        /// The step's position.
        position: i32,
        /// The eval the SOP pins.
        eval_id: Uuid,
    },
    /// A non-agent step claimed the agent primitive; `agent` is the only kind that runs it.
    #[error(
        "step {position}: `agent.run` is the agent step's primitive; a `{kind}` step cannot name it"
    )]
    AgentPrimitiveOutsideAgentStep {
        /// The step's position.
        position: i32,
        /// The kind that was declared.
        kind: &'static str,
    },
    /// A non-computed step names an action; only `computed` invokes one.
    #[error("step {position}: a `{kind}` step must not name an action")]
    ActionOnNonComputed {
        /// The step's position.
        position: i32,
        /// The kind that was declared.
        kind: &'static str,
    },
    /// A `gate` step names no gate code.
    #[error("step {position}: a gate step must name a catalogue gate code")]
    GateWithoutCode {
        /// The step's position.
        position: i32,
    },
    /// An `agent` step names no process agent.
    #[error("step {position}: an agent step must name a capability_agent(role='process_agent')")]
    AgentWithoutProcessAgent {
        /// The step's position.
        position: i32,
    },
    /// A `call` step names no child process/workflow.
    #[error("step {position}: a call step must name a published process or workflow")]
    CallWithoutTarget {
        /// The step's position.
        position: i32,
    },
    /// The process agent does not exist, or is not a process agent.
    #[error("step {position}: capability agent {agent} is not a live process agent")]
    UnknownProcessAgent {
        /// The step's position.
        position: i32,
        /// The id the process asked for.
        agent: uuid::Uuid,
    },
    /// The SOP does not exist as a published procedure.
    #[error("step {position}: procedure {procedure} is not published")]
    UnknownProcedure {
        /// The step's position.
        position: i32,
        /// The id the process asked for.
        procedure: uuid::Uuid,
    },
    /// The call target is neither a published process nor a published workflow.
    #[error("step {position}: `{key}` is neither a published process nor a published workflow")]
    UnknownCallTarget {
        /// The step's position.
        position: i32,
        /// The key the process asked for.
        key: String,
    },
    /// A call step that starts its own process recurses.
    #[error("step {position}: a call step may not start its own process")]
    SelfCall {
        /// The step's position.
        position: i32,
    },
}

/// The facts a publish resolves the process against: the live action manifest, the tenant's
/// gate codes, its live process agents, its published SOPs and the process/workflow keys a
/// `call` step may target.
#[derive(Clone, Debug)]
pub struct PublishFacts {
    /// Every registered action.
    pub actions: Vec<ActionFacts>,
    /// The tenant's live `catalogue_entry(kind='gate')` codes.
    pub gates: Vec<String>,
    /// `capability_agent` ids with `role='process_agent'`.
    pub process_agents: BTreeSet<Uuid>,
    /// Published `procedure` (SOP) ids.
    pub procedures: BTreeSet<Uuid>,
    /// Published process keys **and** published workflow keys — the `call` targets.
    pub call_targets: BTreeSet<String>,
    /// The process's own key, so a `call` cannot start itself.
    pub self_key: String,
    /// The evals that exist in this tenant. An `eval` inspection pins one, and a pin that names a
    /// missing eval would only surface after a step had already parked on it (`JUDGE_MAX_PARKS`
    /// times) — publish is where it is a refusal instead (S13 §5.5).
    pub evals: BTreeSet<Uuid>,
    /// Every published SOP's `eval` pin: `(procedure_id, eval_id)`. A step is judged against its
    /// SOP, so its SOP's pin is the eval that must exist.
    pub procedure_evals: Vec<(Uuid, Uuid)>,
}

/// Validate a process's steps against [`PublishFacts`].
///
/// The checks are deliberately all-or-nothing: a draft may hold anything, a published version must be
/// runnable. Ordering matters only for the error a caller sees first; every rule is independent.
pub fn validate_publish(steps: &[StepInput], facts: &PublishFacts) -> Result<(), PublishError> {
    if steps.is_empty() {
        return Err(PublishError::NoSteps);
    }
    for (index, step) in steps.iter().enumerate() {
        let position = i32::try_from(index + 1).unwrap_or(i32::MAX);

        let Some(kind) = step.kind() else {
            return Err(PublishError::BadKind {
                position,
                value: step.kind.clone().unwrap_or_default(),
            });
        };
        if let Some(explicit) = step.actor.as_deref()
            && Actor::parse(explicit).is_none()
        {
            return Err(PublishError::BadActor {
                position,
                value: explicit.to_owned(),
            });
        }
        let actor = step.actor(kind).unwrap_or(Actor::System);

        // The SOP is what judges this step. A step with no published SOP is a step whose
        // output nobody inspects — the silent accept D28 exists to remove.
        match step.procedure_id {
            Some(procedure) if facts.procedures.contains(&procedure) => {}
            Some(procedure) => {
                return Err(PublishError::UnknownProcedure {
                    position,
                    procedure,
                });
            }
            None => {
                return Err(PublishError::UnknownProcedure {
                    position,
                    procedure: Uuid::nil(),
                });
            }
        }
        // The SOP this step is judged against may pin an eval; a pin that names no eval in this
        // tenant is unrunnable, and the executor would only discover it by parking 64 times.
        if let Some(procedure) = step.procedure_id
            && let Some((_, eval_id)) = facts
                .procedure_evals
                .iter()
                .find(|(pinned, _)| *pinned == procedure)
            && !facts.evals.contains(eval_id)
        {
            return Err(PublishError::UnknownEval {
                position,
                eval_id: *eval_id,
            });
        }

        match kind {
            StepKind::Computed => {
                if step.action.trim().is_empty() {
                    return Err(PublishError::ComputedWithoutAction { position });
                }
                // `agent.run` is the agent step's primitive, not an action a computed step may
                // invoke: two spellings for one behaviour is exactly the ambiguity that made a
                // saved catalogue process unpublishable (2026-11-10).
                if step.action.trim() == AGENT_STEP_ACTION {
                    return Err(PublishError::AgentPrimitiveOutsideAgentStep {
                        position,
                        kind: kind.as_str(),
                    });
                }
                // A registry key is exactly `<resource>.<verb>`: one dot, no spaces.
                let dots = step.action.matches('.').count();
                if dots != 1 || step.action.starts_with('.') || step.action.ends_with('.') {
                    return Err(PublishError::BadActionName {
                        position,
                        action: step.action.clone(),
                    });
                }
                if step.action.starts_with("process_runs.") {
                    return Err(PublishError::SelfReference {
                        position,
                        action: step.action.clone(),
                    });
                }
                let Some(action) = facts.actions.iter().find(|a| a.name == step.action) else {
                    return Err(PublishError::UnknownAction {
                        position,
                        action: step.action.clone(),
                    });
                };
                let on_result = match step.on_result.as_deref() {
                    None => OnError::Fail,
                    Some(text) => {
                        OnError::parse(text).ok_or_else(|| PublishError::BadOnResult {
                            position,
                            value: text.to_owned(),
                        })?
                    }
                };
                if on_result == OnError::Retry && !action.idempotent {
                    return Err(PublishError::RetryOnNonIdempotent {
                        position,
                        action: step.action.clone(),
                    });
                }
            }
            StepKind::Agent => {
                // One representation, and it is the one the executor dispatches on: the step
                // stores the primitive it runs, so `run_step.action` is truthful after the fact.
                let action = step.action.trim();
                if action.is_empty() {
                    return Err(PublishError::AgentWithoutAction { position });
                }
                if action != AGENT_STEP_ACTION {
                    return Err(PublishError::WrongAgentAction {
                        position,
                        action: action.to_owned(),
                    });
                }
                let Some(agent) = step.process_agent_id else {
                    return Err(PublishError::AgentWithoutProcessAgent { position });
                };
                if !facts.process_agents.contains(&agent) {
                    return Err(PublishError::UnknownProcessAgent { position, agent });
                }
                if actor != Actor::ProcessAgent {
                    return Err(PublishError::BadActor {
                        position,
                        value: actor.as_str().to_owned(),
                    });
                }
            }
            StepKind::Gate => {
                if !step.action.trim().is_empty() {
                    return Err(PublishError::ActionOnNonComputed {
                        position,
                        kind: kind.as_str(),
                    });
                }
                let Some(gate) = step.gate_kind.as_deref() else {
                    return Err(PublishError::GateWithoutCode { position });
                };
                if !facts.gates.iter().any(|code| code == gate) {
                    return Err(PublishError::UnknownGate {
                        position,
                        gate: gate.to_owned(),
                    });
                }
            }
            StepKind::Call => {
                if !step.action.trim().is_empty() {
                    return Err(PublishError::ActionOnNonComputed {
                        position,
                        kind: kind.as_str(),
                    });
                }
                let Some(key) = step.child_process_key.as_deref() else {
                    return Err(PublishError::CallWithoutTarget { position });
                };
                if key == facts.self_key {
                    return Err(PublishError::SelfCall { position });
                }
                if !facts.call_targets.iter().any(|target| target == key) {
                    return Err(PublishError::UnknownCallTarget {
                        position,
                        key: key.to_owned(),
                    });
                }
            }
        }

        // `fail | continue | retry` is a typed outcome on every kind, not only computed.
        if let Some(text) = step.on_result.as_deref()
            && OnError::parse(text).is_none()
        {
            return Err(PublishError::BadOnResult {
                position,
                value: text.to_owned(),
            });
        }
        let max_attempts = step.max_attempts.unwrap_or(1);
        if !(1..=MAX_ATTEMPTS_CAP).contains(&max_attempts) {
            return Err(PublishError::AttemptsOutOfRange {
                position,
                max_attempts,
                cap: MAX_ATTEMPTS_CAP,
            });
        }
        template::validate_template(&step.input_template, usize::try_from(position).unwrap_or(0))
            .map_err(|source| PublishError::Template { position, source })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROC: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00aa);
    const AGENT: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00ab);

    /// Every step in these tests is judged by the one published SOP, unless the case is about a
    /// missing one — so the helper fills it in rather than repeating it in every literal.
    fn parse_steps(raw: serde_json::Value) -> Vec<StepInput> {
        let mut steps: Vec<StepInput> = serde_json::from_value(raw).expect("steps deserialize");
        for step in &mut steps {
            if step.procedure_id.is_none() {
                step.procedure_id = Some(PROC);
            }
        }
        steps
    }

    fn manifest() -> Vec<ActionFacts> {
        vec![
            ActionFacts {
                name: "cards.claim".to_owned(),
                idempotent: true,
            },
            ActionFacts {
                name: "cards.move".to_owned(),
                idempotent: false,
            },
        ]
    }

    fn facts(manifest: &[ActionFacts]) -> PublishFacts {
        PublishFacts {
            actions: manifest.to_vec(),
            gates: vec!["human_approval".to_owned()],
            process_agents: BTreeSet::from([AGENT]),
            procedures: BTreeSet::from([PROC]),
            call_targets: BTreeSet::from(["child.process".to_owned(), "tool.setup".to_owned()]),
            self_key: "self.process".to_owned(),
            // The fixture SOP (`PROC`) has no eval pin; the eval set exists so a test can add one.
            evals: BTreeSet::new(),
            procedure_evals: Vec::new(),
        }
    }

    #[test]
    fn a_clean_process_publishes() {
        let steps = parse_steps(json!([
            {"action": "cards.claim", "input_template": {"id": "{{card.id}}"}},
            {"kind": "gate", "gate_kind": "human_approval"},
        ]));
        let manifest = manifest();
        validate_publish(&steps, &facts(&manifest)).expect("a legal process");
    }

    #[test]
    fn retry_needs_an_idempotent_action() {
        let steps =
            parse_steps(json!([{"action": "cards.move", "on_result": "retry", "max_attempts": 3}]));
        let manifest = manifest();
        let error = validate_publish(&steps, &facts(&manifest)).expect_err("not idempotent");
        assert!(
            matches!(error, PublishError::RetryOnNonIdempotent { .. }),
            "{error}"
        );

        let steps = parse_steps(
            json!([{"action": "cards.claim", "on_result": "retry", "max_attempts": 3}]),
        );
        validate_publish(&steps, &facts(&manifest)).expect("claim is idempotent");
    }

    #[test]
    fn the_attempt_cap_and_the_floor_are_both_enforced() {
        let manifest = manifest();
        for bad in [0, MAX_ATTEMPTS_CAP + 1] {
            let steps = parse_steps(
                json!([{"action": "cards.claim", "on_result": "retry", "max_attempts": bad}]),
            );
            let error = validate_publish(&steps, &facts(&manifest)).expect_err("out of range");
            assert!(
                matches!(error, PublishError::AttemptsOutOfRange { .. }),
                "{bad}: {error}"
            );
        }
    }

    #[test]
    fn an_unknown_gate_or_action_is_refused() {
        let manifest = manifest();
        let unknown_gate = parse_steps(json!([{"kind": "gate", "gate_kind": "nope"}]));
        assert!(matches!(
            validate_publish(&unknown_gate, &facts(&manifest)),
            Err(PublishError::UnknownGate { .. })
        ));

        let unknown_action = parse_steps(json!([{"action": "cards.nope"}]));
        assert!(matches!(
            validate_publish(&unknown_action, &facts(&manifest)),
            Err(PublishError::UnknownAction { .. })
        ));
    }

    #[test]
    fn a_bad_on_result_names_its_own_member() {
        let steps = parse_steps(json!([{"action": "cards.claim", "on_result": "banana"}]));
        let manifest = manifest();
        let error = validate_publish(&steps, &facts(&manifest)).expect_err("bad on_result");
        assert!(
            matches!(error, PublishError::BadOnResult { position: 1, .. }),
            "{error}"
        );
    }

    #[test]
    fn a_process_may_not_start_a_run() {
        let mut manifest = manifest();
        manifest.push(ActionFacts {
            name: "process_runs.start".to_owned(),
            idempotent: false,
        });
        let steps = parse_steps(json!([{"action": "process_runs.start"}]));
        let error = validate_publish(&steps, &facts(&manifest)).expect_err("recursion");
        assert!(
            matches!(error, PublishError::SelfReference { .. }),
            "{error}"
        );
    }

    #[test]
    fn a_forward_step_reference_is_refused_at_publish() {
        let steps = parse_steps(json!([
            {"action": "cards.claim", "input_template": {"id": "{{steps.2.output.id}}"}},
            {"action": "cards.move", "input_template": {}},
        ]));
        let manifest = manifest();
        let error = validate_publish(&steps, &facts(&manifest)).expect_err("forward");
        assert!(matches!(error, PublishError::Template { .. }), "{error}");
    }

    #[test]
    fn an_empty_process_is_refused() {
        let manifest = manifest();
        assert_eq!(
            validate_publish(&[], &facts(&manifest)),
            Err(PublishError::NoSteps)
        );
    }

    #[test]
    fn a_computed_step_needs_an_action_and_a_non_computed_one_must_not_name_it() {
        let manifest = manifest();
        let blank = parse_steps(json!([{"kind": "computed"}]));
        assert!(matches!(
            validate_publish(&blank, &facts(&manifest)),
            Err(PublishError::ComputedWithoutAction { .. })
        ));

        let gate_with_action = parse_steps(
            json!([{"kind": "gate", "gate_kind": "human_approval", "action": "cards.claim"}]),
        );
        assert!(matches!(
            validate_publish(&gate_with_action, &facts(&manifest)),
            Err(PublishError::ActionOnNonComputed { .. })
        ));
    }

    #[test]
    fn an_agent_step_needs_a_live_process_agent() {
        let manifest = manifest();
        let steps = parse_steps(json!([
            {"kind": "agent", "action": AGENT_STEP_ACTION, "process_agent_id": AGENT},
        ]));
        validate_publish(&steps, &facts(&manifest)).expect("live process agent");

        let unknown = parse_steps(json!([
            {"kind": "agent", "action": AGENT_STEP_ACTION,
             "process_agent_id": "00000000-0000-0000-0000-0000000000ff"},
        ]));
        assert!(matches!(
            validate_publish(&unknown, &facts(&manifest)),
            Err(PublishError::UnknownProcessAgent { .. })
        ));

        let unset = parse_steps(json!([
            {"kind": "agent", "action": AGENT_STEP_ACTION},
        ]));
        assert!(matches!(
            validate_publish(&unset, &facts(&manifest)),
            Err(PublishError::AgentWithoutProcessAgent { .. })
        ));
    }

    /// The agent primitive is the only action an agent step may name, and no other kind may
    /// claim it — the two halves of the rule that made a saved catalogue process unpublishable.
    #[test]
    fn an_agent_step_names_exactly_the_agent_primitive() {
        let manifest = manifest();

        let blank = parse_steps(json!([
            {"kind": "agent", "process_agent_id": AGENT},
        ]));
        assert!(matches!(
            validate_publish(&blank, &facts(&manifest)),
            Err(PublishError::AgentWithoutAction { position: 1 })
        ));

        let foreign = parse_steps(json!([
            {"kind": "agent", "action": "cards.move", "process_agent_id": AGENT},
        ]));
        assert!(matches!(
            validate_publish(&foreign, &facts(&manifest)),
            Err(PublishError::WrongAgentAction { position: 1, .. })
        ));

        let computed = parse_steps(json!([
            {"kind": "computed", "action": AGENT_STEP_ACTION},
        ]));
        assert!(matches!(
            validate_publish(&computed, &facts(&manifest)),
            Err(PublishError::AgentPrimitiveOutsideAgentStep { position: 1, .. })
        ));
    }

    #[test]
    fn a_gate_step_needs_a_code() {
        let manifest = manifest();
        let steps = parse_steps(json!([{"kind": "gate"}]));
        assert!(matches!(
            validate_publish(&steps, &facts(&manifest)),
            Err(PublishError::GateWithoutCode { .. })
        ));
    }

    #[test]
    fn a_call_step_needs_a_published_process_or_workflow_and_may_not_call_itself() {
        let manifest = manifest();
        let ok = parse_steps(json!([
            {"kind": "call", "child_process_key": "child.process"},
            {"kind": "call", "child_process_key": "tool.setup"},
        ]));
        validate_publish(&ok, &facts(&manifest))
            .expect("a process and a workflow are both targets");

        let missing = parse_steps(json!([{"kind": "call", "child_process_key": "nope"}]));
        assert!(matches!(
            validate_publish(&missing, &facts(&manifest)),
            Err(PublishError::UnknownCallTarget { .. })
        ));

        let self_call = parse_steps(json!([{"kind": "call", "child_process_key": "self.process"}]));
        assert!(matches!(
            validate_publish(&self_call, &facts(&manifest)),
            Err(PublishError::SelfCall { .. })
        ));

        let no_target = parse_steps(json!([{"kind": "call"}]));
        assert!(matches!(
            validate_publish(&no_target, &facts(&manifest)),
            Err(PublishError::CallWithoutTarget { .. })
        ));
    }

    #[test]
    fn every_step_needs_a_published_sop() {
        let manifest = manifest();
        let mut steps = parse_steps(json!([{"action": "cards.claim"}]));
        steps[0].procedure_id = None;
        assert!(matches!(
            validate_publish(&steps, &facts(&manifest)),
            Err(PublishError::UnknownProcedure { .. })
        ));

        let mut steps = parse_steps(json!([{"action": "cards.claim"}]));
        steps[0].procedure_id = Some(Uuid::from_u128(1));
        assert!(matches!(
            validate_publish(&steps, &facts(&manifest)),
            Err(PublishError::UnknownProcedure { .. })
        ));
    }

    #[test]
    fn an_unknown_kind_or_actor_is_refused_by_name() {
        let manifest = manifest();
        let steps = parse_steps(json!([{"action": "cards.claim", "kind": "sequence"}]));
        assert!(matches!(
            validate_publish(&steps, &facts(&manifest)),
            Err(PublishError::BadKind { .. })
        ));

        let steps = parse_steps(json!([{"action": "cards.claim", "actor": "robot"}]));
        assert!(matches!(
            validate_publish(&steps, &facts(&manifest)),
            Err(PublishError::BadActor { .. })
        ));
    }

    #[test]
    fn the_kind_is_inferred_when_absent() {
        let computed: StepInput = serde_json::from_value(json!({"action": "cards.claim"})).unwrap();
        assert_eq!(computed.kind(), Some(StepKind::Computed));
        assert_eq!(computed.actor(StepKind::Computed), Some(Actor::ToolAgent));

        let gate: StepInput =
            serde_json::from_value(json!({"gate_kind": "human_approval"})).unwrap();
        assert_eq!(gate.kind(), Some(StepKind::Gate));
        assert_eq!(gate.actor(StepKind::Gate), Some(Actor::System));
    }
}
