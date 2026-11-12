#![forbid(unsafe_code)]
//! The execution trace and its diff (S13 §5.4, step 9).
//!
//! A trace is one record per executed `run_step`, ordered by `(position, attempt)`. The pass
//! definition compares **effects**, not rows: each position contributes its last attempt (a retry
//! is one effect), and masked hashes make two logically equal inputs compare equal.
//!
//! This module is pure. The caller supplies the records; parsing and diffing touch no database,
//! registry or clock.
//!
//! Masked before hashing (S13 §5.4): UUIDs, timestamps, and the request/trace/idempotency ids.
//! `usage` is masked too: an `agent.run` output carries token counts that vary between two runs
//! of the same work, and they are not a consequential effect (S13 step 8's owner, 2026-11-09).

use std::collections::BTreeMap;

use conveyor_domain::compare::content_hash;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What a masked UUID renders as.
pub const MASKED_UUID: &str = "<uuid>";
/// What a masked RFC 3339 timestamp renders as.
pub const MASKED_TIMESTAMP: &str = "<timestamp>";
/// What a masked request/trace/idempotency id renders as.
pub const MASKED_ID: &str = "<masked>";

/// Object keys whose value is an id or a time, masked regardless of shape.
const MASKED_KEYS: &[&str] = &[
    "request_id",
    "trace_id",
    "idempotency_key",
    "started_at",
    "finished_at",
    "created_at",
    "updated_at",
    "timestamp",
    // An `agent.run` output's token usage is volatile and not consequential (S13 step 8).
    "usage",
];

/// Which node produced an effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    /// A registry action (including `agent.run`).
    Action,
    /// A published procedure started and awaited.
    Procedure,
    /// A closed predicate; its effect is the verdict.
    Condition,
    /// The one cycle; its effect is `each` or `done`.
    Loop,
    /// An approval gate; its effect is the decision.
    Gate,
}

impl TraceKind {
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

/// How an attempt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceOutcome {
    /// The node finished successfully.
    Succeeded,
    /// The attempt failed.
    Failed,
    /// The node failed under `on_error = continue`.
    Skipped,
    /// The attempt is still in flight; exportable, not diffable.
    Running,
}

impl TraceOutcome {
    /// The document spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Running => "running",
        }
    }
}

/// One executed `run_step` as the trace exports it (S13 §5.4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceRecord {
    /// The run the step belongs to.
    pub run_id: String,
    /// The workflow's stable key.
    pub workflow_key: String,
    /// The workflow version the run executed.
    pub workflow_version: i64,
    /// The node id within the definition.
    pub node_id: String,
    /// Execution order within the run (1-based), not graph position.
    pub position: u32,
    /// 1 for the first attempt; a retry records 2, 3, ...
    pub attempt: u32,
    /// The node kind.
    pub kind: TraceKind,
    /// The consequential effect: an action name, `procedure:<key>`, or a control outcome.
    pub effect: String,
    /// `sha256:` hash of the masked resolved input, or empty when the row predates masking.
    pub input_hash: String,
    /// `sha256:` hash of the masked recorded output.
    pub output_hash: String,
    /// Wall time in milliseconds, when the row finished.
    pub duration_ms: Option<i32>,
    /// How the attempt ended.
    pub outcome: TraceOutcome,
}

/// One position's final effect — what the pass definition compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    /// Execution order within the run.
    pub position: u32,
    /// The node that produced it.
    pub node_id: String,
    /// The effect.
    pub effect: String,
    /// Masked input hash.
    pub input_hash: String,
    /// Masked output hash.
    pub output_hash: String,
    /// How the position ended.
    pub outcome: TraceOutcome,
}

/// One difference between two traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceDelta {
    /// The new trace has an effect the old one does not.
    Added {
        /// Where it appeared.
        position: u32,
        /// Which node produced it.
        node_id: String,
    },
    /// The old trace has an effect the new one does not.
    Missing {
        /// Where it appeared in the old trace.
        position: u32,
        /// Which node produced it.
        node_id: String,
    },
    /// The same position differs.
    Mismatch {
        /// The position.
        position: u32,
        /// The node at that position (the new one when they differ).
        node_id: String,
        /// Which field differs (`node_id`, `effect`, `input_hash`, `output_hash`, `outcome`).
        field: &'static str,
        /// The old value.
        old: String,
        /// The new value.
        new: String,
    },
}

/// Why a trace cannot be parsed or compared.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TraceDiffError {
    /// A line is not a trace record.
    #[error("trace line {line}: {detail}")]
    Parse {
        /// 1-based line number.
        line: usize,
        /// What went wrong.
        detail: String,
    },
    /// A position is still in flight, so the trace is not complete.
    #[error("position {position} is still running; a trace is diffed only when it has finished")]
    Incomplete {
        /// The position still running.
        position: u32,
    },
    /// A recorded n8n document (execution or workflow snapshot) cannot be projected.
    #[error("n8n envelope: {detail}")]
    Envelope {
        /// What is wrong with the document.
        detail: String,
    },
    /// An expectation file is invalid, or the recorded run cannot be derived into one.
    #[error("expectation {fixture}: {detail}")]
    ExpectedInvalid {
        /// The fixture name (`a`, `b`, `c`).
        fixture: String,
        /// What is wrong, named precisely enough to fix.
        detail: String,
    },
}

/// The result of comparing two traces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffReport {
    /// One entry per difference; empty means identical.
    pub deltas: Vec<TraceDelta>,
}

impl DiffReport {
    /// Whether the two traces are equal.
    pub fn identical(&self) -> bool {
        self.deltas.is_empty()
    }

    /// `0` when identical, `1` when anything differs.
    pub fn exit_code(&self) -> i32 {
        if self.identical() { 0 } else { 1 }
    }
}

/// Mask the values that must not make two logically equal traces differ.
///
/// UUID-shaped strings become [`MASKED_UUID`], RFC 3339 strings [`MASKED_TIMESTAMP`], and the
/// value of any [`MASKED_KEYS`] entry [`MASKED_ID`]. Recurses through arrays and objects.
pub fn mask_value(value: &Value) -> Value {
    match value {
        Value::String(text) if is_uuid(text) => Value::String(MASKED_UUID.to_owned()),
        Value::String(text) if is_rfc3339(text) => Value::String(MASKED_TIMESTAMP.to_owned()),
        Value::Array(items) => Value::Array(items.iter().map(mask_value).collect()),
        Value::Object(object) => {
            let mut out = Map::with_capacity(object.len());
            for (key, value) in object {
                let masked = if MASKED_KEYS.contains(&key.as_str()) {
                    Value::String(MASKED_ID.to_owned())
                } else {
                    mask_value(value)
                };
                out.insert(key.clone(), masked);
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// `sha256:` + the canonical-JSON hash of the masked value.
pub fn hash_value(value: &Value) -> String {
    format!("sha256:{}", content_hash(&mask_value(value)))
}

/// Project a node's recorded output into its effect.
///
/// An action's effect is its action name (the caller passes the row's `action`, which already
/// carries `procedure:<key>` for a procedure node). A condition or gate reports its boolean, and
/// a loop reports `each` when it entered the body and `done` when it had nothing to do.
pub fn project_effect(kind: TraceKind, action: &str, output: &Value) -> String {
    match kind {
        TraceKind::Action | TraceKind::Procedure => action.to_owned(),
        TraceKind::Condition | TraceKind::Gate => match output.as_bool() {
            Some(verdict) => verdict.to_string(),
            None => output.to_string(),
        },
        TraceKind::Loop => match output.get("items").and_then(Value::as_u64) {
            Some(0) => "done".to_owned(),
            _ => "each".to_owned(),
        },
    }
}

/// Serialise records as JSONL, one compact record per line, ordered as given.
pub fn to_jsonl(records: &[TraceRecord]) -> Result<String, TraceDiffError> {
    let mut out = String::new();
    for (index, record) in records.iter().enumerate() {
        let line = serde_json::to_string(record).map_err(|error| TraceDiffError::Parse {
            line: index + 1,
            detail: error.to_string(),
        })?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// Parse JSONL into records.
///
/// Blank lines are skipped. A line that is not a [`TraceRecord`] is refused with its number; the
/// n8n `execution_data` envelope is not a record and is refused the same way (steps 10–13 project
/// it before this point).
pub fn parse_jsonl(text: &str) -> Result<Vec<TraceRecord>, TraceDiffError> {
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: TraceRecord =
            serde_json::from_str(line).map_err(|error| TraceDiffError::Parse {
                line: index + 1,
                detail: error.to_string(),
            })?;
        records.push(record);
    }
    Ok(records)
}

/// Reduce records to the ordered effects the pass definition compares.
///
/// Orders by `(position, attempt)` and keeps only the last attempt at each position. A position
/// still running makes the trace incomplete rather than silently truncated.
pub fn effects(records: &[TraceRecord]) -> Result<Vec<Effect>, TraceDiffError> {
    let mut ordered: Vec<&TraceRecord> = records.iter().collect();
    ordered.sort_by_key(|record| (record.position, record.attempt));
    let mut last: BTreeMap<u32, &TraceRecord> = BTreeMap::new();
    for record in ordered {
        last.insert(record.position, record);
    }
    let mut out = Vec::with_capacity(last.len());
    for (position, record) in last {
        if record.outcome == TraceOutcome::Running {
            return Err(TraceDiffError::Incomplete { position });
        }
        out.push(Effect {
            position,
            node_id: record.node_id.clone(),
            effect: record.effect.clone(),
            input_hash: record.input_hash.clone(),
            output_hash: record.output_hash.clone(),
            outcome: record.outcome,
        });
    }
    Ok(out)
}

/// Which comparison rules a diff applies.
///
/// The two systems that produce traces store different payloads for the same effect: an old n8n
/// node returns Planka's or RM's row, the conveyor returns its own view. A cross-system
/// comparison therefore cannot compare `input_hash`/`output_hash` — they hash different schemas.
/// Owner ruling 2026-11-09: the cross-system hash rule is **thrown out**, not normalised; the
/// ordered effect and outcome are the comparison. Definition ids may not contain spaces
/// (`definition.rs`), so node ids are compared after [`normalise_node_id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    /// Both sides are this engine's traces: the full effect tuple, hashes included.
    SameSystem,
    /// The old side is a recording from another system: the ordered effect and outcome, with
    /// node ids compared after normalisation; hashes are recorded but never compared.
    Migration,
}

/// The node-id form a [`DiffMode::Migration`] diff compares: lowercase, every run of
/// non-alphanumerics collapsed to `_`, leading/trailing `_` trimmed.
pub fn normalise_node_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    let mut separator = false;
    for character in id.chars() {
        if character.is_alphanumeric() {
            out.extend(character.to_lowercase());
            separator = false;
        } else if !out.is_empty() && !separator {
            out.push('_');
            separator = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Compare two traces by their ordered effects, in the same-system mode.
pub fn diff_records(
    old: &[TraceRecord],
    new: &[TraceRecord],
) -> Result<DiffReport, TraceDiffError> {
    diff_records_with(old, new, DiffMode::SameSystem)
}

/// Compare two traces under `mode`.
pub fn diff_records_with(
    old: &[TraceRecord],
    new: &[TraceRecord],
    mode: DiffMode,
) -> Result<DiffReport, TraceDiffError> {
    let old = effects(old)?;
    let new = effects(new)?;
    let mut deltas = Vec::new();
    let len = old.len().max(new.len());
    for index in 0..len {
        match (old.get(index), new.get(index)) {
            (None, Some(newer)) => deltas.push(TraceDelta::Added {
                position: newer.position,
                node_id: newer.node_id.clone(),
            }),
            (Some(older), None) => deltas.push(TraceDelta::Missing {
                position: older.position,
                node_id: older.node_id.clone(),
            }),
            (Some(older), Some(newer)) => {
                let same_node = match mode {
                    DiffMode::SameSystem => older.node_id == newer.node_id,
                    DiffMode::Migration => {
                        normalise_node_id(&older.node_id) == normalise_node_id(&newer.node_id)
                    }
                };
                if !same_node {
                    deltas.push(TraceDelta::Mismatch {
                        position: newer.position,
                        node_id: newer.node_id.clone(),
                        field: "node_id",
                        old: older.node_id.clone(),
                        new: newer.node_id.clone(),
                    });
                    continue;
                }
                if older.position != newer.position {
                    deltas.push(TraceDelta::Mismatch {
                        position: newer.position,
                        node_id: newer.node_id.clone(),
                        field: "position",
                        old: older.position.to_string(),
                        new: newer.position.to_string(),
                    });
                }
                if older.effect != newer.effect {
                    deltas.push(TraceDelta::Mismatch {
                        position: newer.position,
                        node_id: newer.node_id.clone(),
                        field: "effect",
                        old: older.effect.clone(),
                        new: newer.effect.clone(),
                    });
                }
                if mode == DiffMode::SameSystem {
                    for (field, old_value, new_value) in [
                        ("input_hash", &older.input_hash, &newer.input_hash),
                        ("output_hash", &older.output_hash, &newer.output_hash),
                    ] {
                        if old_value != new_value {
                            deltas.push(TraceDelta::Mismatch {
                                position: newer.position,
                                node_id: newer.node_id.clone(),
                                field,
                                old: old_value.clone(),
                                new: new_value.clone(),
                            });
                        }
                    }
                }
                if older.outcome != newer.outcome {
                    deltas.push(TraceDelta::Mismatch {
                        position: newer.position,
                        node_id: newer.node_id.clone(),
                        field: "outcome",
                        old: older.outcome.as_str().to_owned(),
                        new: newer.outcome.as_str().to_owned(),
                    });
                }
            }
            (None, None) => {}
        }
    }
    Ok(DiffReport { deltas })
}

/// Whether `text` is a hyphenated UUID (the identity form this engine writes).
fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.as_bytes()[8] == b'-'
        && text.as_bytes()[13] == b'-'
        && text.as_bytes()[18] == b'-'
        && text.as_bytes()[23] == b'-'
        && text
            .chars()
            .filter(|c| *c != '-')
            .all(|c| c.is_ascii_hexdigit())
}

/// Whether `text` looks like an RFC 3339 timestamp (`YYYY-MM-DDTHH:MM:SS…`).
fn is_rfc3339(text: &str) -> bool {
    let bytes = text.as_bytes();
    text.len() >= 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && (bytes[10] == b'T' || bytes[10] == b' ')
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}
