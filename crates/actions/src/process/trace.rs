#![forbid(unsafe_code)]
//! The process engine's trace and its diff (S13 §5.4, step 9).
//!
//! One record per `run_step`, ordered by `(position, attempt)`, behind a `{"kind":"run"}` header
//! naming the process and the version the run pinned. The export reads the stored rows rather than
//! re-deriving them: `run_step` already holds the resolved input, the recorded output, the judge's
//! verdict and the failure, so the trace says what happened instead of what should have.
//!
//! Hashing and masking are the **workflow trace's**, not a second implementation — [`hash_value`]
//! masks UUIDs, timestamps and the request/trace/idempotency ids before hashing the canonical
//! JSON, so two logically equal executions compare equal. A second hash would let the two S13
//! gates disagree about what "the same effect" means.
//!
//! **What the diff compares, and what it only carries.** `position` and `attempt` are identity;
//! `action`, `status`, `input_sha256`, `output_sha256`, `judge` and `error` are the effect. The
//! step id and the two timestamps are carried for the human reading the file and never compared:
//! a timestamp is volatile by definition, and every process version is a new row, so its step ids
//! differ between two versions that did identical work. The header is carried for provenance —
//! which run, which process, which version — and is never compared either: a version bump on its
//! own is not a behavioural difference. This is the same choice [`conveyor_workflow::trace::Effect`]
//! makes by dropping `duration_ms`, `workflow_key` and `workflow_version`.
//!
//! A step still `queued` or `running` has no effect yet, so a diff over it is **refused**
//! ([`ProcessTraceError::Incomplete`]) rather than silently shortened.
//!
//! The store reads are RLS-scoped: the caller opens the connection inside
//! `conveyor_store::begin_scoped(pool, tenant)` (or calls [`conveyor_store::set_tenant`]) and
//! passes the same tenant here.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_workflow::trace::{hash_value, mask_value};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// The header record: which run, which process, which version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTraceHeader {
    /// The run the steps belong to.
    pub run_id: String,
    /// The process row the run pinned.
    pub process_id: String,
    /// The process version the run pinned.
    pub process_version: i64,
}

/// One executed `run_step` as the trace exports it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepTraceRecord {
    /// 1-based execution order within the run.
    pub position: i32,
    /// 1 for the first attempt; a retry records 2, 3, ...
    pub attempt: i32,
    /// The `process_step` row, when the step came from a published process version.
    pub process_step_id: Option<String>,
    /// The action the step invoked.
    pub action: String,
    /// `queued | running | succeeded | failed | skipped`.
    pub status: String,
    /// `sha256:` hash of the masked resolved input.
    pub input_sha256: String,
    /// `sha256:` hash of the masked recorded output.
    pub output_sha256: String,
    /// The judge's verdict on this attempt, as stored; `null` when the step has none.
    pub judge: Option<Value>,
    /// The failure message, when the step failed.
    pub error: Option<String>,
    /// When the step started; carried, never compared.
    pub started_at: Option<DateTime<Utc>>,
    /// When the step finished; carried, never compared.
    pub finished_at: Option<DateTime<Utc>>,
}

/// One line of a process trace: the run header, then one record per step attempt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProcessTraceRecord {
    /// The run the trace belongs to.
    Run(RunTraceHeader),
    /// One executed step attempt.
    Step(StepTraceRecord),
}

/// One difference between two process traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessTraceDelta {
    /// The new trace has a step the old one does not.
    Added {
        /// Where it appeared.
        position: i32,
        /// The attempt.
        attempt: i32,
    },
    /// The old trace has a step the new one does not.
    Missing {
        /// Where it appeared in the old trace.
        position: i32,
        /// The attempt.
        attempt: i32,
    },
    /// The same `(position, attempt)` differs.
    Mismatch {
        /// The position.
        position: i32,
        /// The attempt.
        attempt: i32,
        /// Which field differs (`action`, `status`, `input_sha256`, `output_sha256`, `judge`,
        /// `error`).
        field: &'static str,
        /// The expectation's value.
        old: String,
        /// The exported value.
        new: String,
    },
}

/// Why a process trace cannot be exported, parsed or compared.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProcessTraceError {
    /// A line is not a process trace record.
    #[error("trace line {line}: {detail}")]
    Parse {
        /// 1-based line number.
        line: usize,
        /// What went wrong.
        detail: String,
    },
    /// The trace carries no `kind: run` header, so it cannot name a process version.
    #[error("the trace has no `kind: run` header record")]
    MissingHeader,
    /// The same `(position, attempt)` appears more than once.
    #[error("position {position} attempt {attempt} appears more than once")]
    Duplicate {
        /// The duplicated position.
        position: i32,
        /// The duplicated attempt.
        attempt: i32,
    },
    /// A step has not finished, so the trace has no effect to compare yet.
    #[error("position {position} attempt {attempt} is {status}; the trace has not finished")]
    Incomplete {
        /// The position still in flight.
        position: i32,
        /// The attempt still in flight.
        attempt: i32,
        /// The status it is stuck at (`queued` or `running`).
        status: String,
    },
    /// The run does not exist in this tenant.
    #[error("no run {0} in this tenant")]
    RunNotFound(Uuid),
    /// The run is not a process run, so it has no process trace.
    #[error("run {0} is not a process run; export it with workflow_traces.export")]
    NotAProcessRun(Uuid),
    /// The stored rows could not be read.
    #[error("read the run's trace: {0}")]
    Read(String),
    /// A record could not be serialised.
    #[error("serialise trace record {line}: {detail}")]
    Serialize {
        /// 1-based record number.
        line: usize,
        /// What went wrong.
        detail: String,
    },
}

/// The result of comparing two process traces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessTraceDiff {
    /// One entry per difference; empty means identical.
    pub deltas: Vec<ProcessTraceDelta>,
}

impl ProcessTraceDiff {
    /// Whether the two traces are equal.
    pub fn identical(&self) -> bool {
        self.deltas.is_empty()
    }

    /// `0` when identical, `1` when anything differs.
    pub fn exit_code(&self) -> i32 {
        if self.identical() { 0 } else { 1 }
    }
}

/// Export one run's process trace: the header record, then one record per `run_step` ordered by
/// `(position, attempt)`.
///
/// A run with no `process_id` is refused rather than exported as an empty process trace: it is a
/// workflow run, and `workflow_traces.export` is its exporter.
pub async fn export_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<Value>, ProcessTraceError> {
    let node = store::run::get_run_node_tx(&mut *conn, tenant, run_id)
        .await
        .map_err(read_failure)?
        .ok_or(ProcessTraceError::RunNotFound(run_id))?;
    let process_id = store::process::run_process_tx(&mut *conn, tenant, run_id)
        .await
        .map_err(read_failure)?
        .ok_or(ProcessTraceError::NotAProcessRun(run_id))?;

    // `RunStepRecord` carries the judge but not the timings; `RunStepDetail` carries the timings
    // but not the judge. Both are the store's reads of the same row, ordered the same way, and
    // this export wants both halves — so they are joined by the row id here rather than a third
    // column list being taught to the store. Two statements under READ COMMITTED can straddle a
    // commit, so a run still in flight may be read across two snapshots; that is harmless because
    // the diff refuses a trace with a step still in flight.
    let rows = store::process::list_run_steps_tx(&mut *conn, tenant, run_id)
        .await
        .map_err(read_failure)?;
    let detail = store::process::list_run_steps_detail_tx(&mut *conn, tenant, run_id)
        .await
        .map_err(read_failure)?;
    #[allow(clippy::type_complexity)]
    let mut timings: HashMap<Uuid, (Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = detail
        .into_iter()
        .map(|row| (row.id, (row.started_at, row.finished_at)))
        .collect();

    let mut records = Vec::with_capacity(rows.len() + 1);
    let header = ProcessTraceRecord::Run(RunTraceHeader {
        run_id: run_id.to_string(),
        process_id: process_id.to_string(),
        process_version: node.process_version,
    });
    records.push(record_value(header, 1)?);
    for (index, row) in rows.into_iter().enumerate() {
        let (started_at, finished_at) = timings.remove(&row.id).unwrap_or((None, None));
        let record = ProcessTraceRecord::Step(StepTraceRecord {
            position: row.position,
            attempt: row.attempt,
            process_step_id: row.process_step_id.map(|id| id.to_string()),
            action: row.action,
            status: row.status,
            input_sha256: hash_value(&row.input),
            output_sha256: hash_value(&row.output),
            judge: row.judge,
            error: row.error,
            started_at,
            finished_at,
        });
        // One based, counting the header at line 1.
        records.push(record_value(record, index + 2)?);
    }
    Ok(records)
}

/// Serialise records as JSONL, one compact record per line, ordered as given.
pub fn to_jsonl(records: &[Value]) -> Result<String, ProcessTraceError> {
    let mut out = String::new();
    for (index, record) in records.iter().enumerate() {
        let line = serde_json::to_string(record).map_err(|error| ProcessTraceError::Serialize {
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
/// Blank lines are skipped. The first record must be the `kind: run` header and it may appear
/// only once; a step before it, a second header, or a line that is not a record is refused with
/// its line number.
pub fn parse_jsonl(text: &str) -> Result<Vec<ProcessTraceRecord>, ProcessTraceError> {
    let mut records: Vec<ProcessTraceRecord> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: ProcessTraceRecord =
            serde_json::from_str(line).map_err(|error| ProcessTraceError::Parse {
                line: index + 1,
                detail: error.to_string(),
            })?;
        match (&record, records.is_empty()) {
            (ProcessTraceRecord::Step(_), true) => {
                return Err(ProcessTraceError::Parse {
                    line: index + 1,
                    detail: "the first record must be the `kind: run` header".to_owned(),
                });
            }
            (ProcessTraceRecord::Run(_), false) => {
                return Err(ProcessTraceError::Parse {
                    line: index + 1,
                    detail: "a second `kind: run` header".to_owned(),
                });
            }
            _ => {}
        }
        records.push(record);
    }
    if records.is_empty() {
        return Err(ProcessTraceError::MissingHeader);
    }
    Ok(records)
}

/// Compare two traces given as their JSONL text.
pub fn diff_text(expect: &str, new: &str) -> Result<ProcessTraceDiff, ProcessTraceError> {
    let expected = parse_jsonl(expect)?;
    let exported = parse_jsonl(new)?;
    diff_traces(&expected, &exported)
}

/// Compare two traces, step by step, each step keyed by `(position, attempt)`.
///
/// The header is required (a trace without one cannot name what it recorded) but not compared;
/// see the module docs.
pub fn diff_traces(
    old: &[ProcessTraceRecord],
    new: &[ProcessTraceRecord],
) -> Result<ProcessTraceDiff, ProcessTraceError> {
    header(old)?;
    header(new)?;
    let mut deltas = Vec::new();
    let old_steps = steps(old)?;
    let new_steps = steps(new)?;
    let mut keys: BTreeSet<(i32, i32)> = old_steps.keys().copied().collect();
    keys.extend(new_steps.keys().copied());
    for key in keys {
        match (old_steps.get(&key), new_steps.get(&key)) {
            (None, Some(step)) => deltas.push(ProcessTraceDelta::Added {
                position: step.position,
                attempt: step.attempt,
            }),
            (Some(step), None) => deltas.push(ProcessTraceDelta::Missing {
                position: step.position,
                attempt: step.attempt,
            }),
            (Some(older), Some(newer)) => compare_step(older, newer, &mut deltas),
            (None, None) => {}
        }
    }
    Ok(ProcessTraceDiff { deltas })
}

/// The header record, which every trace must start with.
fn header(records: &[ProcessTraceRecord]) -> Result<&RunTraceHeader, ProcessTraceError> {
    match records.first() {
        Some(ProcessTraceRecord::Run(header)) => Ok(header),
        _ => Err(ProcessTraceError::MissingHeader),
    }
}

/// The steps keyed by `(position, attempt)`; a step still in flight makes the trace incomplete.
fn steps(
    records: &[ProcessTraceRecord],
) -> Result<BTreeMap<(i32, i32), &StepTraceRecord>, ProcessTraceError> {
    let mut out = BTreeMap::new();
    for record in records {
        let ProcessTraceRecord::Step(step) = record else {
            continue;
        };
        if matches!(step.status.as_str(), "queued" | "running") {
            return Err(ProcessTraceError::Incomplete {
                position: step.position,
                attempt: step.attempt,
                status: step.status.clone(),
            });
        }
        if out.insert((step.position, step.attempt), step).is_some() {
            return Err(ProcessTraceError::Duplicate {
                position: step.position,
                attempt: step.attempt,
            });
        }
    }
    Ok(out)
}

/// Compare one step's effect. Order is fixed so a file diff of two reports is stable.
fn compare_step(
    older: &StepTraceRecord,
    newer: &StepTraceRecord,
    deltas: &mut Vec<ProcessTraceDelta>,
) {
    let pairs = [
        ("action", older.action.clone(), newer.action.clone()),
        ("status", older.status.clone(), newer.status.clone()),
        (
            "input_sha256",
            older.input_sha256.clone(),
            newer.input_sha256.clone(),
        ),
        (
            "output_sha256",
            older.output_sha256.clone(),
            newer.output_sha256.clone(),
        ),
        (
            "judge",
            render_judge(&older.judge),
            render_judge(&newer.judge),
        ),
        (
            "error",
            older.error.clone().unwrap_or_else(|| "null".to_owned()),
            newer.error.clone().unwrap_or_else(|| "null".to_owned()),
        ),
    ];
    for (field, old, new) in pairs {
        if old != new {
            deltas.push(ProcessTraceDelta::Mismatch {
                position: newer.position,
                attempt: newer.attempt,
                field,
                old,
                new,
            });
        }
    }
}

/// A judge document as the diff reports it: the same masking the hashes use, so an id or a
/// timestamp inside the verdict cannot make two equal executions differ.
fn render_judge(judge: &Option<Value>) -> String {
    match judge {
        Some(document) => serde_json::to_string(&mask_value(document))
            .unwrap_or_else(|_| "<unserialisable>".to_owned()),
        None => "null".to_owned(),
    }
}

/// One record as the JSON value a line of the file holds.
fn record_value(record: ProcessTraceRecord, line: usize) -> Result<Value, ProcessTraceError> {
    serde_json::to_value(record).map_err(|error| ProcessTraceError::Serialize {
        line,
        detail: error.to_string(),
    })
}

/// A store failure, with the store's own message.
fn read_failure(error: impl std::fmt::Display) -> ProcessTraceError {
    ProcessTraceError::Read(error.to_string())
}
