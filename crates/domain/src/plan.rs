#![forbid(unsafe_code)]
//! The plan projection over a published process (S21 §6).
//!
//! A process is a linear list of action steps. The projection derives the dependency graph the
//! runtime already follows — a step's preconditions are its gate and the earlier steps whose outputs
//! its input template reads; its effects are the paths it declares in `outputs` — and checks what
//! publish could not see before the declaration existed:
//!
//! * a step whose input references an output **no earlier step produces** (the producer does not
//!   declare the path, or declares it but may be skipped, leaving `Null`);
//! * a step that is **unreachable** because one of its inputs cannot be produced;
//! * a **gate that can never open**, because the catalogue row it names carries an `expires_at`
//!   the runtime cannot read (not an RFC3339 string). A VALID timestamp — even one already past —
//!   is not a defect: the runtime opens the gate and the expiry applies the step's `on_result`.
//!
//! Deliberately not a planner. There is no execution here, no second state machine and nothing
//! stored: the plan is derived on demand from the frozen step list, exactly as S21 §6 says.
//!
//! Pure: the caller supplies the live manifest's output schemas and the tenant's gate facts, so
//! nothing here reads a database or a registry.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::{Map, Value};

use crate::process::{OnError, StepInput};
use crate::template;

/// What publish knows about one `catalogue_entry(kind='gate')` row.
///
/// Deliberately no parsed `expires_at`: a VALID timestamp — past or future — is not a defect. The
/// runtime opens the gate and lets an expiry apply the step's `on_result`
/// (`crates/actions/tests/process_step.rs::an_expired_gate_applies_the_steps_on_error`), so only
/// a value that is not an RFC3339 timestamp (the runtime silently ignores a non-string and refuses
/// to open an unparseable one) makes the gate unopenable.
#[derive(Clone, Debug)]
pub struct GateFacts {
    /// The gate code.
    pub code: String,
    /// The raw `meta.expires_at` when it is not an RFC3339 timestamp.
    pub bad_expiry: Option<String>,
}

/// One input a step reads from an earlier step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// The JSON pointer of the template node that holds the reference.
    pub pointer: String,
    /// The 1-based step the reference names.
    pub producer: i32,
    /// The dot-path into that step's output; empty means the whole output.
    pub path: String,
}

/// A step as the projection sees it: what it needs, what it exposes, what holds it open.
#[derive(Clone, Debug)]
pub struct PlanStep {
    /// 1-based order.
    pub position: i32,
    /// The registry action.
    pub action: String,
    /// The gate code the step parks on.
    pub gate: Option<String>,
    /// What a failure does next.
    pub on_result: OnError,
    /// The paths this step exposes to later steps.
    pub outputs: Vec<String>,
    /// The earlier steps this step reads from.
    pub requires: Vec<Requirement>,
    /// `steps.` references whose index is not a positive integer (JSON pointers of the nodes).
    pub bad_references: Vec<String>,
}

/// The read-only model. Derived from the frozen step list; never stored.
#[derive(Clone, Debug)]
pub struct Plan {
    /// One entry per step, in order.
    pub steps: Vec<PlanStep>,
}

/// One defect the plan projection found. Every variant is a publish-time `422`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// A declared output path is not a legal dot-path.
    BadOutput {
        /// The step that declares it.
        position: i32,
        /// The declared path.
        path: String,
        /// Why it is not legal.
        reason: &'static str,
    },
    /// A declared output path is not in the action's output schema.
    UndeclaredOutput {
        /// The step that declares it.
        position: i32,
        /// The action the step names.
        action: String,
        /// The declared path.
        path: String,
    },
    /// A reference to a path the producing step does not declare, or its action does not return.
    UnproducibleInput {
        /// The step whose input holds the reference.
        position: i32,
        /// The JSON pointer of the template node.
        pointer: String,
        /// The step the reference names.
        producer: i32,
        /// The referenced path (empty = the whole output).
        path: String,
    },
    /// A reference to a step whose failure does not stop the run.
    MaybeAbsentInput {
        /// The step whose input holds the reference.
        position: i32,
        /// The JSON pointer of the template node.
        pointer: String,
        /// The step the reference names.
        producer: i32,
        /// The referenced path.
        path: String,
    },
    /// A `steps.` reference whose step index is not a positive integer.
    BadInputReference {
        /// The step whose input holds the reference.
        position: i32,
        /// The JSON pointer of the template node.
        pointer: String,
    },
    /// A reference to the current or a later step, which cannot have run yet.
    FutureInput {
        /// The step whose input holds the reference.
        position: i32,
        /// The JSON pointer of the template node.
        pointer: String,
        /// The step the reference names.
        producer: i32,
    },
    /// A gate that can never open: its catalogue `expires_at` is not an RFC3339 timestamp.
    GateNeverOpens {
        /// The gated step.
        position: i32,
        /// The gate code.
        gate: String,
        /// The raw `meta.expires_at` value.
        expiry: String,
    },
    /// A step whose input cannot be produced when it runs.
    UnreachableStep {
        /// The step.
        position: i32,
    },
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadOutput {
                position,
                path,
                reason,
            } => write!(
                f,
                "step {position}: output {path:?} is not a legal path: {reason}"
            ),
            Self::UndeclaredOutput {
                position,
                action,
                path,
            } => write!(
                f,
                "step {position}: output {path:?} is not in {action}'s output schema"
            ),
            Self::UnproducibleInput {
                position,
                pointer,
                producer,
                path,
            } => write!(
                f,
                "step {position}: {pointer} references step {producer}.output.{path}, which step {producer} does not declare"
            ),
            Self::MaybeAbsentInput {
                position,
                pointer,
                producer,
                path,
            } => write!(
                f,
                "step {position}: {pointer} references step {producer}.output.{path}, but step {producer} has on_result=continue, so the value may be absent"
            ),
            Self::BadInputReference { position, pointer } => write!(
                f,
                "step {position}: {pointer} names a step by something that is not a positive integer"
            ),
            Self::FutureInput {
                position,
                pointer,
                producer,
            } => write!(
                f,
                "step {position}: {pointer} references step {producer}, which runs after it"
            ),
            Self::GateNeverOpens {
                position,
                gate,
                expiry,
            } => write!(
                f,
                "step {position}: gate {gate:?} can never open: its catalogue expires_at {expiry:?} is not an RFC3339 timestamp"
            ),
            Self::UnreachableStep { position } => write!(
                f,
                "step {position} is unreachable: one of its inputs cannot be produced"
            ),
        }
    }
}

/// Derive the plan from a step list. No validation, no database.
pub fn project(steps: &[StepInput]) -> Plan {
    Plan {
        steps: steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let (requires, bad_references) = requirements(step);
                PlanStep {
                    position: i32::try_from(index + 1).unwrap_or(i32::MAX),
                    action: step.action.clone(),
                    gate: step.gate_kind.clone(),
                    on_result: step
                        .on_result
                        .as_deref()
                        .and_then(OnError::parse)
                        .unwrap_or(OnError::Fail),
                    outputs: step.outputs.clone(),
                    requires,
                    bad_references,
                }
            })
            .collect(),
    }
}

/// Every earlier-step reference in one step's input template, split into the well-formed
/// requirements and the `steps.` references whose index cannot be a step.
fn requirements(step: &StepInput) -> (Vec<Requirement>, Vec<String>) {
    let Ok(references) = template::references(&step.input_template) else {
        // A malformed template is `validate_publish`'s refusal, reported there with its pointer.
        return (Vec::new(), Vec::new());
    };
    let mut wanted = Vec::new();
    let mut bad = Vec::new();
    for (pointer, reference) in references {
        match requirement(&pointer, &reference) {
            None => {}
            Some(Ok(requirement)) => wanted.push(requirement),
            Some(Err(())) => bad.push(pointer),
        }
    }
    (wanted, bad)
}

/// `steps.<n>[.output]<path>` → a requirement.
///
/// `None` for the other roots; `Some(Err(()))` for a `steps.` reference whose index is not a
/// positive integer, which the projection reports rather than silently dropping.
fn requirement(pointer: &str, reference: &str) -> Option<Result<Requirement, ()>> {
    let mut segments = reference.split('.');
    if segments.next()? != "steps" {
        return None;
    }
    let Some(raw) = segments.next() else {
        return Some(Err(()));
    };
    let Ok(producer) = raw.parse::<i32>() else {
        return Some(Err(()));
    };
    if producer < 1 {
        return Some(Err(()));
    }
    let mut rest: Vec<&str> = segments.collect();
    // `.output` is how the plan spells the reference; the resolver accepts it as a no-op member.
    if rest.first() == Some(&"output") {
        rest.remove(0);
    }
    Some(Ok(Requirement {
        pointer: pointer.to_owned(),
        producer,
        path: rest.join("."),
    }))
}

/// Check a plan against the manifest's output schemas and the tenant's gate facts.
pub fn findings(
    steps: &[StepInput],
    action_outputs: &BTreeMap<String, Value>,
    gates: &[GateFacts],
) -> Vec<Finding> {
    let plan = project(steps);
    let mut out = Vec::new();

    // 1. Every DECLARATION is a legal path and a path the action can actually return.
    for step in &plan.steps {
        for path in &step.outputs {
            if let Err(reason) = check_path(path) {
                out.push(Finding::BadOutput {
                    position: step.position,
                    path: path.clone(),
                    reason,
                });
                continue;
            }
            if let Some(schema) = action_outputs.get(&step.action)
                && !schema_has_path(schema, path)
            {
                out.push(Finding::UndeclaredOutput {
                    position: step.position,
                    action: step.action.clone(),
                    path: path.clone(),
                });
            }
        }
    }

    // 2. Every REFERENCE is declared by its producer and cannot be null at run time.
    for step in &plan.steps {
        let mut unreachable = false;
        for pointer in &step.bad_references {
            out.push(Finding::BadInputReference {
                position: step.position,
                pointer: pointer.clone(),
            });
            unreachable = true;
        }
        for requirement in &step.requires {
            // A step cannot read a step that has not run. `validate_publish` refuses this too, but
            // the projection is public and must not pass a plan whose edges point forward.
            if requirement.producer >= step.position {
                out.push(Finding::FutureInput {
                    position: step.position,
                    pointer: requirement.pointer.clone(),
                    producer: requirement.producer,
                });
                unreachable = true;
                continue;
            }
            let producer = plan
                .steps
                .get(usize::try_from(requirement.producer).unwrap_or(usize::MAX) - 1);
            let Some(producer) = producer else {
                continue;
            };
            if producer.position != requirement.producer {
                continue;
            }
            let covered = producer
                .outputs
                .iter()
                .any(|declared| covers(declared, &requirement.path));
            // Declaration is the author's contract; the action schema is the runtime's reality.
            // Both must produce the path — a declared prefix with a deeper typo is still a
            // reference the run cannot satisfy, and it is cheaper to say so at publish.
            let produced = match action_outputs.get(&producer.action) {
                Some(schema) if !requirement.path.is_empty() => {
                    schema_has_path(schema, &requirement.path)
                }
                // No schema fact for the action, or a whole-output reference (already refused by
                // the declaration rule): the declaration alone is the judge.
                _ => true,
            };
            if !covered || !produced {
                out.push(Finding::UnproducibleInput {
                    position: step.position,
                    pointer: requirement.pointer.clone(),
                    producer: requirement.producer,
                    path: requirement.path.clone(),
                });
                unreachable = true;
                continue;
            }
            if producer.on_result == OnError::Continue {
                out.push(Finding::MaybeAbsentInput {
                    position: step.position,
                    pointer: requirement.pointer.clone(),
                    producer: requirement.producer,
                    path: requirement.path.clone(),
                });
                unreachable = true;
            }
        }
        if unreachable {
            out.push(Finding::UnreachableStep {
                position: step.position,
            });
        }
    }

    // 3. Every GATE can actually open. An unknown code is `validate_publish`'s refusal; a VALID
    // expiry — past or future — is a supported `on_result` path (the runtime opens the gate and an
    // expiry applies the step's policy), so only an unreadable value is refused here.
    for step in &plan.steps {
        let Some(code) = &step.gate else {
            continue;
        };
        let Some(facts) = gates.iter().find(|facts| &facts.code == code) else {
            continue;
        };
        if let Some(raw) = &facts.bad_expiry {
            out.push(Finding::GateNeverOpens {
                position: step.position,
                gate: code.clone(),
                expiry: raw.clone(),
            });
        }
    }

    out
}

/// A declared path is a non-empty dot-path with no empty segment.
fn check_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("it is empty");
    }
    if path.trim() != path {
        return Err("it has leading or trailing whitespace");
    }
    if path.split('.').any(str::is_empty) {
        return Err("it has an empty segment");
    }
    Ok(())
}

/// `declared` covers `path` when it is the same path or a segment-prefix of it.
///
/// A whole-output reference (empty `path`) is never covered: the step must name what it reads.
fn covers(declared: &str, path: &str) -> bool {
    path == declared
        || path
            .strip_prefix(declared)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Does this JSON Schema declare `path`?
///
/// This is a **declared-path** reading of the schema, not full JSON-Schema validation: it answers
/// "could a value at this path exist, according to what the schema names". An object schema declares
/// the members under `properties`; a missing `additionalProperties` (schemars' usual shape) declares
/// nothing beyond them, `additionalProperties: true` or a value schema declares anything under it,
/// and `allOf` UNIONS its branches' declarations (a flattened schemars object adds members in a
/// second branch). `anyOf`/`oneOf` accept a path any branch declares. A local `$ref` is followed
/// into the root's `$defs`; an external `$ref` cannot be read here, so it does not refuse the path.
pub fn schema_has_path(schema: &Value, path: &str) -> bool {
    let segments: Vec<&str> = path.split('.').collect();
    resolve(schema, schema, &segments, 0)
}

/// The most `$ref`/branch hops one path may take. A cyclic `$ref` (root `#`, or a definition that
/// points at itself) never consumes a segment, so without a ceiling a malformed schema would
/// recurse until the stack dies. Refusing is the safe answer; schemars never emits a cycle.
const MAX_SCHEMA_DEPTH: usize = 64;

fn resolve(schema: &Value, root: &Value, segments: &[&str], depth: usize) -> bool {
    if depth > MAX_SCHEMA_DEPTH {
        return false;
    }
    if segments.is_empty() {
        return true;
    }
    let Some(object) = schema.as_object() else {
        // `true` allows anything; `false` and any non-object scalar allow nothing.
        return matches!(schema, Value::Bool(true));
    };
    if object.is_empty() {
        return true;
    }
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        // Only a local pointer can be followed. An external reference is not readable here, and
        // refusing a declaration because of a schema we cannot open would be a false refusal.
        let Some(pointer) = reference.strip_prefix('#') else {
            return true;
        };
        return root
            .pointer(pointer)
            .is_some_and(|target| resolve(target, root, segments, depth + 1));
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(branches) = object.get(keyword).and_then(Value::as_array) {
            return branches
                .iter()
                .any(|branch| resolve(branch, root, segments, depth + 1));
        }
    }
    if let Some(branches) = object.get("allOf").and_then(Value::as_array)
        && branches
            .iter()
            .any(|branch| resolve(branch, root, segments, depth + 1))
    {
        return true;
    }

    let segment = segments[0];
    let rest = &segments[1..];
    if type_includes(object, "object")
        || object.contains_key("properties")
        || object.contains_key("additionalProperties")
    {
        if let Some(child) = object
            .get("properties")
            .and_then(Value::as_object)
            .and_then(|properties| properties.get(segment))
        {
            return resolve(child, root, rest, depth + 1);
        }
        return match object.get("additionalProperties") {
            Some(Value::Bool(true)) => true,
            Some(additional @ Value::Object(_)) => resolve(additional, root, rest, depth + 1),
            _ => false,
        };
    }
    if type_includes(object, "array") || object.contains_key("items") {
        if segment.parse::<usize>().is_err() {
            return false;
        }
        return match object.get("items") {
            Some(Value::Array(_)) => false,
            Some(items) => resolve(items, root, rest, depth + 1),
            None => true,
        };
    }
    false
}

fn type_includes(object: &Map<String, Value>, name: &str) -> bool {
    match object.get("type") {
        Some(Value::String(text)) => text == name,
        Some(Value::Array(items)) => items.iter().any(|item| item.as_str() == Some(name)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn steps(raw: Value) -> Vec<StepInput> {
        serde_json::from_value(raw).expect("steps deserialize")
    }

    fn outputs() -> BTreeMap<String, Value> {
        let mut map = BTreeMap::new();
        map.insert(
            "cards.claim".to_owned(),
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "card": {"$ref": "#/$defs/Card"},
                },
                "$defs": {
                    "Card": {
                        "type": "object",
                        "properties": {"id": {"type": "string"}, "title": {"type": "string"}}
                    }
                }
            }),
        );
        map.insert(
            "cards.comment".to_owned(),
            json!({"type": "object", "properties": {"id": {"type": "string"}}}),
        );
        map
    }

    fn gate(code: &str, bad_expiry: Option<&str>) -> GateFacts {
        GateFacts {
            code: code.to_owned(),
            bad_expiry: bad_expiry.map(str::to_owned),
        }
    }

    #[test]
    fn a_declared_reference_publishes_clean() {
        let steps = steps(json!([
            {"action": "cards.claim", "outputs": ["id", "card.title"]},
            {"action": "cards.comment",
             "input_template": {"id": "{{steps.1.output.id}}",
                                "body": "{{steps.1.output.card.title}}"}}
        ]));
        assert_eq!(findings(&steps, &outputs(), &[]), vec![]);
    }

    #[test]
    fn a_reference_to_an_undeclared_path_is_unproducible() {
        let steps = steps(json!([
            {"action": "cards.claim", "outputs": ["id"]},
            {"action": "cards.comment", "input_template": {"id": "{{steps.1.output.card.id}}"}}
        ]));
        let found = findings(&steps, &outputs(), &[]);
        assert!(
            found.iter().any(|finding| matches!(
                finding,
                Finding::UnproducibleInput {
                    position: 2,
                    producer: 1,
                    ..
                }
            )),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|finding| matches!(finding, Finding::UnreachableStep { position: 2 })),
            "{found:?}"
        );
    }

    #[test]
    fn a_whole_output_reference_is_not_covered_by_a_named_path() {
        let steps = steps(json!([
            {"action": "cards.claim", "outputs": ["id"]},
            {"action": "cards.comment", "input_template": {"id": "{{steps.1.output}}"}}
        ]));
        assert!(
            findings(&steps, &outputs(), &[])
                .iter()
                .any(|finding| matches!(finding, Finding::UnproducibleInput { .. })),
            "a whole-output reference must name a declared path"
        );
    }

    #[test]
    fn a_reference_deeper_than_the_action_schema_is_unproducible() {
        let steps = steps(json!([
            {"action": "cards.claim", "outputs": ["card"]},
            {"action": "cards.comment", "input_template": {"id": "{{steps.1.output.card.nope}}"}}
        ]));
        assert!(
            findings(&steps, &outputs(), &[])
                .iter()
                .any(|finding| matches!(finding, Finding::UnproducibleInput { position: 2, .. })),
            "a declared prefix does not make a deeper typo producible"
        );
    }

    #[test]
    fn a_malformed_step_index_is_a_bad_input_reference() {
        for reference in ["steps.zero.output.id", "steps.0.output.id", "steps."] {
            let template = json!({"id": format!("{}{}{}", "{{", reference, "}}")});
            let steps = steps(json!([
                {"action": "cards.claim", "input_template": template},
            ]));
            assert!(
                findings(&steps, &outputs(), &[])
                    .iter()
                    .any(|finding| matches!(
                        finding,
                        Finding::BadInputReference { position: 1, .. }
                    )),
                "{reference:?} must be reported, not dropped"
            );
        }
    }

    #[test]
    fn a_cyclic_ref_is_refused_without_overflowing_the_stack() {
        assert!(!schema_has_path(&json!({"$ref": "#"}), "anything"));
    }

    #[test]
    fn a_forward_or_self_reference_is_a_future_input() {
        // Self: step 1 reading its own output. Forward: step 1 reading step 2.
        for raw in [
            json!([
                {"action": "cards.claim", "outputs": ["id"],
                 "input_template": {"id": "{{steps.1.output.id}}"}},
            ]),
            json!([
                {"action": "cards.claim", "input_template": {"id": "{{steps.2.output.id}}"}},
                {"action": "cards.comment", "outputs": ["id"]},
            ]),
        ] {
            let steps = steps(raw);
            assert!(
                findings(&steps, &outputs(), &[])
                    .iter()
                    .any(|finding| matches!(finding, Finding::FutureInput { .. })),
                "a reference to the current or a later step must not pass the projection"
            );
        }
    }

    #[test]
    fn an_external_ref_does_not_refuse_a_declaration() {
        let schema = json!({
            "type": "object",
            "properties": {"card": {"$ref": "https://example.invalid/card.json"}}
        });
        assert!(schema_has_path(&schema, "card.anything"));
    }

    #[test]
    fn a_continue_producer_may_be_absent() {
        let steps = steps(json!([
            {"action": "cards.claim", "outputs": ["id"], "on_result": "continue"},
            {"action": "cards.comment", "input_template": {"id": "{{steps.1.output.id}}"}}
        ]));
        let found = findings(&steps, &outputs(), &[]);
        assert!(
            found.iter().any(|finding| matches!(
                finding,
                Finding::MaybeAbsentInput {
                    position: 2,
                    producer: 1,
                    ..
                }
            )),
            "{found:?}"
        );
    }

    #[test]
    fn a_declaration_must_exist_in_the_action_output_schema() {
        let steps = steps(json!([
            {"action": "cards.claim", "outputs": ["nope"]},
        ]));
        assert!(
            findings(&steps, &outputs(), &[])
                .iter()
                .any(|finding| matches!(finding, Finding::UndeclaredOutput { position: 1, .. })),
            "a declaration cannot name a field the action never returns"
        );
    }

    #[test]
    fn a_bad_declaration_is_named() {
        for path in ["", " id", "card..id"] {
            let steps = steps(json!([
                {"action": "cards.claim", "outputs": [path]},
            ]));
            assert!(
                findings(&steps, &outputs(), &[])
                    .iter()
                    .any(|finding| matches!(finding, Finding::BadOutput { position: 1, .. })),
                "{path:?} must be refused"
            );
        }
    }

    #[test]
    fn an_unreadable_gate_expiry_is_refused() {
        let steps = steps(json!([
            {"action": "cards.claim", "gate_kind": "human_approval"},
        ]));
        // No expiry at all is the common case: nothing to refuse.
        assert_eq!(
            findings(&steps, &outputs(), &[gate("human_approval", None)]),
            vec![]
        );
        // A valid timestamp, past or future, is NOT a finding: the runtime opens the gate and an
        // expiry applies the step's `on_result` (proved by
        // `process_step::an_expired_gate_applies_the_steps_on_error`).
        let bad = findings(
            &steps,
            &outputs(),
            &[gate("human_approval", Some("tomorrow"))],
        );
        assert!(
            bad.iter()
                .any(|finding| matches!(finding, Finding::GateNeverOpens { position: 1, .. })),
            "{bad:?}"
        );
    }

    #[test]
    fn the_schema_walker_handles_arrays_and_anyof() {
        let schema = json!({
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"type": "object",
                    "properties": {"id": {"type": "string"}}}},
                "maybe": {"anyOf": [{"$ref": "#/$defs/Card"}, {"type": "null"}]},
            },
            "$defs": {"Card": {"type": "object", "properties": {"id": {"type": "string"}}}}
        });
        assert!(schema_has_path(&schema, "items.0.id"));
        assert!(schema_has_path(&schema, "maybe.id"));
        assert!(!schema_has_path(&schema, "items.0.nope"));
        assert!(!schema_has_path(&schema, "maybe.nope"));
        assert!(!schema_has_path(&schema, "nope"));
    }

    #[test]
    fn the_prefix_rule_is_segment_aware() {
        assert!(covers("card", "card.id"));
        assert!(covers("card.id", "card.id"));
        assert!(!covers("card.id", "card"));
        assert!(!covers("card.i", "card.id"));
        assert!(!covers("card", ""));
    }
}
