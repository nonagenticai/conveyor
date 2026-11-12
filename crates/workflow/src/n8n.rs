#![forbid(unsafe_code)]
//! Projection of a recorded n8n execution into the S13 §5.4 trace (step 10).
//!
//! The old side of the trace gate is a real execution recorded from the live n8n. A fixture was
//! an `<x>.old.jsonl` recording plus its `<x>.workflow.json` snapshot. **The recorded `.old.jsonl`
//! files and their trace-diff tests were removed from the public tree at the 2026-11-12 flip** —
//! the n8n flows never ran successfully and the proof is retired. The projection code is retained
//! for the recorded workflow/trace executor; `--old` still reads a recording supplied locally.
//!
//! What is an effect (§5.4): an action, a procedure, a condition's verdict, a loop's `each`/`done`,
//! a gate's decision. `code`, `noOp`, `stickyNote`, `wait`, triggers and the langchain model/tool
//! sub-nodes are **not** effects — their result is proved by the next action's input hash. A node
//! whose effect cannot be named is returned in [`Projected::unmapped`] rather than guessed: a diff
//! over an incomplete old side would be a number with no meaning.

use serde_json::{Map, Value, json};

use crate::trace::{
    TraceDiffError, TraceKind, TraceOutcome, TraceRecord, hash_value, project_effect,
};

/// The deepest flatted indirection accepted before the document is called malformed.
const MAX_DEPTH: usize = 64;

/// Node types (the segment after the last `.`) that execute but are **not** effects.
const NOT_AN_EFFECT: [&str; 13] = [
    "code",
    "noOp",
    "stickyNote",
    "wait",
    "webhook",
    "executeWorkflowTrigger",
    "manualTrigger",
    "scheduleTrigger",
    "formTrigger",
    "respondToWebhook",
    "lmChatOpenAi",
    "toolWorkflow",
    "merge",
];

/// A projected old trace, plus every executed node the effect map does not cover.
#[derive(Clone, Debug)]
pub struct Projected {
    /// One record per effect, ordered by the execution's own `executionIndex`.
    pub records: Vec<TraceRecord>,
    /// Executed nodes with no nameable conveyor effect; non-empty means the old side is
    /// incomplete and must not be diffed.
    pub unmapped: Vec<Unmapped>,
}

/// One executed node that could not be projected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmapped {
    /// The node's name in the flow (its trace id).
    pub node_id: String,
    /// Its n8n type.
    pub node_type: String,
    /// Why it has no effect.
    pub detail: String,
}

/// Resolve n8n's flatted serialization into ordinary JSON.
///
/// The document is an array; index 0 is the root. A string member that parses as an index into
/// the same array is a reference. A numeric string that addresses nothing is ordinary data, not a
/// broken reference — flatted cannot tell them apart and guessing would corrupt real payloads. A
/// cycle or excessive nesting is refused: a malformed envelope must not silently produce a short
/// trace.
pub fn flatted_decode(raw: &Value) -> Result<Value, TraceDiffError> {
    let array = raw
        .as_array()
        .ok_or_else(|| envelope("a flatted document is an array"))?;
    if array.is_empty() {
        return Err(envelope("a flatted document has at least a root entry"));
    }
    let mut memo: Vec<Option<Value>> = vec![None; array.len()];
    let mut resolving: Vec<bool> = vec![false; array.len()];
    resolve_at(0, array, &mut memo, &mut resolving, 0)
}

fn resolve_at(
    index: usize,
    array: &[Value],
    memo: &mut [Option<Value>],
    resolving: &mut [bool],
    depth: usize,
) -> Result<Value, TraceDiffError> {
    if depth > MAX_DEPTH {
        return Err(envelope(format!(
            "flatted nesting is deeper than {MAX_DEPTH} at index {index}"
        )));
    }
    if index >= array.len() {
        return Err(envelope(format!("flatted index {index} is out of range")));
    }
    if let Some(value) = &memo[index] {
        return Ok(value.clone());
    }
    if resolving[index] {
        return Err(envelope(format!("flatted index {index} is cyclic")));
    }
    resolving[index] = true;
    let value = walk(&array[index], array, memo, resolving, depth + 1)?;
    resolving[index] = false;
    memo[index] = Some(value.clone());
    Ok(value)
}

fn walk(
    value: &Value,
    array: &[Value],
    memo: &mut [Option<Value>],
    resolving: &mut [bool],
    depth: usize,
) -> Result<Value, TraceDiffError> {
    match value {
        Value::String(text) => match text.parse::<usize>() {
            Ok(index) if index < array.len() => resolve_at(index, array, memo, resolving, depth),
            _ => Ok(value.clone()),
        },
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(walk(item, array, memo, resolving, depth)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(fields) => {
            let mut out = Map::with_capacity(fields.len());
            for (key, field) in fields {
                out.insert(key.clone(), walk(field, array, memo, resolving, depth)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// Read a workflow snapshot (`workflow_entity`'s `id`/`name`/`versionId`/`nodes`).
pub fn read_snapshot(text: &str) -> Result<Value, TraceDiffError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| envelope(format!("workflow snapshot: {error}")))?;
    nodes_of(&value)?;
    Ok(value)
}

/// Project one recorded execution.
///
/// `run_id` is the execution's id: the envelope does not carry it, and the caller recorded it
/// beside the payload.
pub fn project_execution(
    snapshot: &Value,
    execution: &Value,
    run_id: &str,
) -> Result<Projected, TraceDiffError> {
    let nodes = nodes_of(snapshot)?;
    let workflow_key = snapshot
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("n8n")
        .to_owned();

    let mut definitions: std::collections::BTreeMap<&str, &Value> =
        std::collections::BTreeMap::new();
    for node in nodes {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| envelope("a workflow node has no name"))?;
        definitions.insert(name, node);
    }

    let document = flatted_decode(execution)?;
    let run_data = document
        .pointer("/resultData/runData")
        .and_then(Value::as_object)
        .ok_or_else(|| envelope("the execution has no resultData.runData"))?;

    let mut ordered: Vec<(i64, i64, String, TraceRecord)> = Vec::new();
    let mut unmapped = Vec::new();

    for (name, runs) in run_data {
        let definition = definitions.get(name.as_str()).copied().ok_or_else(|| {
            envelope(format!(
                "node {name:?} ran but is not in the workflow snapshot"
            ))
        })?;
        let node_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let suffix = node_type.rsplit('.').next().unwrap_or_default();
        if NOT_AN_EFFECT.contains(&suffix) || suffix.ends_with("Tool") {
            continue;
        }
        let runs = runs
            .as_array()
            .ok_or_else(|| envelope(format!("node {name:?} has no run array")))?;

        for (index, run) in runs.iter().enumerate() {
            let attempt = u32::try_from(index + 1).unwrap_or(u32::MAX);
            let kind = kind_of(suffix);
            let output = run_output(run);
            let effect = match kind {
                TraceKind::Condition => project_effect(
                    TraceKind::Condition,
                    "",
                    &Value::Bool(condition_verdict(run)),
                ),
                _ => match effect_of(suffix, definition) {
                    Some(action) => action,
                    None => {
                        unmapped.push(Unmapped {
                            node_id: name.clone(),
                            node_type: node_type.to_owned(),
                            detail: describe_missing(suffix, definition),
                        });
                        continue;
                    }
                },
            };
            let record = TraceRecord {
                run_id: run_id.to_owned(),
                workflow_key: workflow_key.clone(),
                workflow_version: 0,
                node_id: name.clone(),
                position: 0,
                attempt,
                kind,
                effect,
                input_hash: hash_value(&run_input(run, run_data)),
                output_hash: hash_value(&output),
                duration_ms: duration_ms(run),
                outcome: outcome_of(run),
            };
            ordered.push((execution_index(run), start_time(run), name.clone(), record));
        }
    }

    ordered.sort_by(|left, right| {
        (left.0, left.1, &left.2, left.3.attempt).cmp(&(
            right.0,
            right.1,
            &right.2,
            right.3.attempt,
        ))
    });
    let mut records = Vec::with_capacity(ordered.len());
    for (position, (_, _, _, mut record)) in (1_u32..).zip(ordered) {
        record.position = position;
        records.push(record);
    }
    Ok(Projected { records, unmapped })
}

fn nodes_of(snapshot: &Value) -> Result<&Vec<Value>, TraceDiffError> {
    if let Some(nodes) = snapshot.get("nodes").and_then(Value::as_array) {
        return Ok(nodes);
    }
    if let Some(nodes) = snapshot.as_array() {
        return Ok(nodes);
    }
    Err(envelope(
        "the workflow snapshot has no nodes array (use {name, nodes} or the bare nodes array)",
    ))
}

fn kind_of(suffix: &str) -> TraceKind {
    match suffix {
        "if" | "switch" => TraceKind::Condition,
        "executeWorkflow" => TraceKind::Procedure,
        _ => TraceKind::Action,
    }
}

/// The conveyor action that replaces this node, or `None` when the map does not cover it.
///
/// Only operations observed in the recorded fixtures are mapped; growing this table is a
/// deliberate change with a fixture behind it, never an inference from a similar name.
fn effect_of(suffix: &str, node: &Value) -> Option<String> {
    let parameters = node.get("parameters");
    let text = |key: &str| {
        parameters
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let effect = match suffix {
        "agent" => "agent.run",
        "planka" => match (text("resource"), text("operation")) {
            ("board", "get") => "boards.get",
            ("card", "get") => "cards.get",
            ("comment", "create") => "cards.comment",
            ("card", "update") if moves_only(parameters) => "cards.move",
            ("card", "update") => "cards.update",
            _ => return None,
        },
        "requirements" => match (text("resource"), text("operation")) {
            ("pmProjects", "getProject") | ("pmProjects", "getProjectByPlankaBoard") => {
                "projects.get"
            }
            ("pmProjects", "getProjectBriefVersion") => "requirements.get",
            ("pmProjects", "listProjectBriefVersions") => "requirements.list",
            ("pmProjectTypes", "getProjectTypeCardTypes") | ("pmCardTypes", "listCardTypes") => {
                "catalogues.list"
            }
            _ => return None,
        },
        _ => return None,
    };
    Some(effect.to_owned())
}

fn describe_missing(suffix: &str, node: &Value) -> String {
    let parameters = node.get("parameters");
    let text = |key: &str| {
        parameters
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    format!(
        "{suffix} resource={:?} operation={:?} has no mapped conveyor action",
        text("resource"),
        text("operation")
    )
}

/// Whether a `card` update touches nothing but the list — the successor of a n8n card move.
fn moves_only(parameters: Option<&Value>) -> bool {
    match parameters
        .and_then(|value| value.get("updateFields"))
        .and_then(Value::as_object)
    {
        Some(fields) => fields.len() == 1 && fields.contains_key("listId"),
        None => false,
    }
}

/// The `if`/`switch` verdict: which of the two outputs carried items.
fn condition_verdict(run: &Value) -> bool {
    run.pointer("/data/main/0")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
}

/// The first item a node produced, as JSON.
fn run_output(run: &Value) -> Value {
    let Some(outputs) = run.pointer("/data/main").and_then(Value::as_array) else {
        return json!({});
    };
    for output in outputs {
        if let Some(item) = output.as_array().and_then(|items| items.first()) {
            return item.get("json").cloned().unwrap_or_else(|| json!({}));
        }
    }
    json!({})
}

/// The input a node received: the first item of the node that fed it.
///
/// n8n records results, not inputs; `source` names the producing node, output slot and run, which
/// is the same item the engine passed downstream. Measured shapes in recorded payloads:
/// `[{previousNode, previousNodeOutput, previousNodeRun}]` (an ARRAY wrapping the object) and, on
/// older payloads, `[[name, output, run]]`. Both are unwrapped; anything else means "no input".
fn run_input(run: &Value, run_data: &Map<String, Value>) -> Value {
    let Some(source) = run.get("source") else {
        return json!({});
    };
    let Some((name, output_index, run_index)) = source_ref(source) else {
        return json!({});
    };
    let produced = run_data
        .get(name)
        .and_then(Value::as_array)
        .and_then(|runs| runs.get(run_index))
        .and_then(|producer| producer.pointer("/data/main"))
        .and_then(Value::as_array)
        .and_then(|outputs| outputs.get(output_index))
        .and_then(Value::as_array)
        .and_then(|items| items.first());
    match produced.and_then(|item| item.get("json")) {
        Some(json) => json.clone(),
        None => json!({}),
    }
}

/// `(node name, output slot, run index)` from any recorded `source` shape.
fn source_ref(source: &Value) -> Option<(&str, usize, usize)> {
    match source {
        Value::Object(fields) => {
            let name = fields.get("previousNode").and_then(Value::as_str)?;
            let output = fields
                .get("previousNodeOutput")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let run = fields
                .get("previousNodeRun")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            Some((name, output, run))
        }
        Value::Array(items) => {
            let first = items.first()?;
            match first {
                Value::Object(_) => source_ref(first),
                Value::Array(triple) => {
                    let name = triple.first().and_then(Value::as_str)?;
                    Some((
                        name,
                        triple.get(1).and_then(Value::as_u64).unwrap_or(0) as usize,
                        triple.get(2).and_then(Value::as_u64).unwrap_or(0) as usize,
                    ))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn duration_ms(run: &Value) -> Option<i32> {
    let millis = run.get("executionTime").and_then(Value::as_i64)?;
    Some(i32::try_from(millis.max(0)).unwrap_or(i32::MAX))
}

fn outcome_of(run: &Value) -> TraceOutcome {
    let failed = run.get("error").is_some_and(|error| !error.is_null())
        || run.get("executionStatus").and_then(Value::as_str) == Some("error");
    if failed {
        TraceOutcome::Failed
    } else {
        TraceOutcome::Succeeded
    }
}

fn execution_index(run: &Value) -> i64 {
    run.get("executionIndex")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX)
}

fn start_time(run: &Value) -> i64 {
    run.get("startTime").and_then(Value::as_i64).unwrap_or(0)
}

fn envelope(detail: impl Into<String>) -> TraceDiffError {
    TraceDiffError::Envelope {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flatted_document_resolves_its_indices() {
        let raw = json!([{"a": "1", "b": "2"}, "plain", {"deep": "1"}]);
        let decoded = flatted_decode(&raw).expect("decodes");
        assert_eq!(decoded, json!({"a": "plain", "b": {"deep": "plain"}}));
    }

    #[test]
    fn a_numeric_string_beyond_the_array_is_a_literal() {
        // flatted uses an index only when it addresses an entry; a value like "9" in a 1-entry
        // document is data. Reading it as a broken reference would corrupt ordinary payloads.
        let raw = json!([{"a": "9"}]);
        let decoded = flatted_decode(&raw).expect("decodes");
        assert_eq!(decoded, json!({"a": "9"}));
    }

    #[test]
    fn a_non_array_document_is_refused() {
        let error = flatted_decode(&json!({"a": 1})).expect_err("refused");
        assert!(error.to_string().contains("array"), "{error}");
    }

    #[test]
    fn a_cycle_is_refused() {
        let raw = json!(["0"]);
        let error = flatted_decode(&raw).expect_err("refused");
        assert!(error.to_string().contains("cyclic"), "{error}");
    }
}
