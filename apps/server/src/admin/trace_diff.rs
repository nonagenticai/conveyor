#![forbid(unsafe_code)]
//! `conveyor admin trace-diff --old <file> [--old-workflow <snapshot>] --new <file>` — the S13
//! §5.4 diff gate. A reviewed `--expect <expected.json>` replaces `--old` for a migrated flow.
//!
//! The workflow mode is pure file work: no configuration, no pool, no registry. Exit 0 when the
//! ordered effects are identical, 1 when anything differs, 2 when either file cannot be compared.
//!
//! The **process** mode (`--process-run <uuid> --expect <file.jsonl> [--write <file>]`) is the S13
//! process engine's half of the same gate. It is not pure file work: the new side is the run's
//! stored `run_step` rows, exported through [`conveyor_actions::process::trace`], so the caller
//! supplies a connection opened inside a tenant scope (`conveyor_store::begin_scoped`). `--write`
//! records the export (which becomes the next `--expect`); `--expect` compares against a recorded
//! one. The exit codes are the same: 0 identical (or recorded), 1 differences, 2 when the run
//! cannot be exported or either file cannot be read.
//!
//! Three old-side forms, and the first two select the comparison:
//!
//! - a reviewed expectation (`--expect`, `fixtures/<x>.expected.json`) — the ordered effect path a
//!   human read out of the recording and committed. Compared under the **migration** rule; the
//!   `end_state` block is not checked here (it needs the run's facts, so the fixture test asserts
//!   it through `Expected::end_state_matches`).
//! - a raw recorded n8n execution (`execution_data.data`, the flatted envelope) together with
//!   `--old-workflow <snapshot>`. An execution does not carry node types, so the snapshot is not
//!   optional; without it the old side cannot name a single effect. Two systems' payloads are
//!   different schemas, so the **migration** rule applies: the ordered effect and outcome with
//!   normalised node ids; hashes are recorded but never compared (owner ruling 2026-11-09).
//! - JSONL records — the exported form, and what `--new` always is. Both sides are this engine,
//!   so the same-system rule applies: the full effect tuple, hashes included.

use std::path::{Path, PathBuf};

use conveyor_actions::process::trace as process_trace;
use conveyor_actions::workflow::trace::{
    DiffMode, Expected, N8nUnmapped, TraceDelta, TraceRecord, diff_records_with, parse_jsonl,
    project_n8n_execution, read_n8n_snapshot,
};
use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// What a comparison found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceDiffOutcome {
    /// The two traces have the same ordered effects.
    Identical,
    /// They differ; the value is the number of deltas printed.
    Delta(usize),
}

/// Compare two traces, printing one `delta: ...` line per difference.
///
/// `expect` replaces `old`/`old_workflow` with a reviewed expectation file. `old` is `None` only
/// when `expect` is present; the CLI enforces that, and this function refuses neither being given.
pub fn trace_diff(
    old: Option<&Path>,
    new: &Path,
    old_workflow: Option<&Path>,
    expect: Option<&Path>,
) -> Result<TraceDiffOutcome, String> {
    let new_text = read(new)?;
    let new_records =
        parse_jsonl(&new_text).map_err(|error| format!("{}: {error}", new.display()))?;

    let report = if let Some(expect_path) = expect {
        let expected_text = read(expect_path)?;
        let expected = Expected::parse(&expected_text)
            .map_err(|error| format!("{}: {error}", expect_path.display()))?;
        expected.compare(&new_records).map_err(|error| {
            format!(
                "cannot compare {} with {}: {error}",
                expect_path.display(),
                new.display()
            )
        })?
    } else {
        let old = old.ok_or_else(|| "trace-diff needs --old or --expect".to_owned())?;
        let old_records = match old_workflow {
            Some(snapshot_path) => project(old, snapshot_path)?,
            None => {
                let old_text = read(old)?;
                parse_jsonl(&old_text).map_err(|error| format!("{}: {error}", old.display()))?
            }
        };

        // `--old-workflow` exists only for a recorded n8n execution, so its presence is the mode.
        let mode = if old_workflow.is_some() {
            DiffMode::Migration
        } else {
            DiffMode::SameSystem
        };
        diff_records_with(&old_records, &new_records, mode).map_err(|error| {
            format!(
                "cannot compare {} with {}: {error}",
                old.display(),
                new.display()
            )
        })?
    };

    for delta in &report.deltas {
        println!("delta: {}", describe(delta));
    }
    if report.deltas.is_empty() {
        Ok(TraceDiffOutcome::Identical)
    } else {
        Ok(TraceDiffOutcome::Delta(report.deltas.len()))
    }
}

/// Project a raw recorded n8n execution into trace records.
///
/// An executed node with no mapped conveyor effect makes the old side incomplete; a diff over it
/// would be a number with no meaning, so it is refused rather than shortened.
fn project(old: &Path, snapshot_path: &Path) -> Result<Vec<TraceRecord>, String> {
    let snapshot_text = read(snapshot_path)?;
    let snapshot = read_n8n_snapshot(&snapshot_text)
        .map_err(|error| format!("{}: {error}", snapshot_path.display()))?;
    let execution_text = read(old)?;
    let execution: serde_json::Value = serde_json::from_str(&execution_text)
        .map_err(|error| format!("{}: not JSON: {error}", old.display()))?;
    let projected = project_n8n_execution(&snapshot, &execution, &old.display().to_string())
        .map_err(|error| format!("{}: {error}", old.display()))?;
    if !projected.unmapped.is_empty() {
        return Err(format!(
            "{}: {} executed node(s) have no mapped conveyor effect: {}",
            old.display(),
            projected.unmapped.len(),
            projected
                .unmapped
                .iter()
                .map(describe_unmapped)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    Ok(projected.records)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn describe_unmapped(node: &N8nUnmapped) -> String {
    format!("{:?} ({}) — {}", node.node_id, node.node_type, node.detail)
}

/// One delta as the operator reads it.
fn describe(delta: &TraceDelta) -> String {
    match delta {
        TraceDelta::Added { position, node_id } => {
            format!("added position {position} node {node_id:?}")
        }
        TraceDelta::Missing { position, node_id } => {
            format!("missing position {position} node {node_id:?}")
        }
        TraceDelta::Mismatch {
            position,
            node_id,
            field,
            old,
            new,
        } => format!("position {position} node {node_id:?} {field}: {old:?} -> {new:?}"),
    }
}

// ── the process mode (S13's process gate) ────────────────────────────────────────────────────

/// What the process mode did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessTraceOutcome {
    /// The export was written to `path` and no comparison was asked for.
    Recorded {
        /// Where the trace was written.
        path: PathBuf,
        /// How many records the file holds (the header included).
        records: usize,
    },
    /// The exported trace and the expectation have the same effects.
    Identical,
    /// They differ; the value is the number of deltas printed.
    Delta(usize),
}

impl ProcessTraceOutcome {
    /// `0` when the trace was recorded or identical, `1` when anything differs (a refusal is the
    /// caller's `Err` and exits `2`).
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Recorded { .. } | Self::Identical => 0,
            Self::Delta(_) => 1,
        }
    }
}

/// Export one process run's stored trace, write it when asked, and diff it against a recorded one.
///
/// `conn` must be scoped to `tenant` (`conveyor_store::begin_scoped`), because `run` and `run_step`
/// are under FORCE RLS. At least one of `expect` and `write` must be given: a comparison needs a
/// recorded side, and a recording needs somewhere to go.
pub async fn process_trace_diff(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    expect: Option<&Path>,
    write: Option<&Path>,
) -> Result<ProcessTraceOutcome, String> {
    let records = process_trace::export_tx(conn, tenant, run_id)
        .await
        .map_err(|error| format!("run {run_id}: {error}"))?;
    let jsonl =
        process_trace::to_jsonl(&records).map_err(|error| format!("run {run_id}: {error}"))?;

    if let Some(path) = write {
        write_file(path, &jsonl)?;
        println!("recorded: {} ({} records)", path.display(), records.len());
    }

    let Some(expect_path) = expect else {
        let Some(path) = write else {
            return Err("process trace-diff needs --expect or --write".to_owned());
        };
        return Ok(ProcessTraceOutcome::Recorded {
            path: path.to_path_buf(),
            records: records.len(),
        });
    };

    let expected_text = read(expect_path)?;
    let report = process_trace::diff_text(&expected_text, &jsonl).map_err(|error| {
        let path = expect_path.display();
        format!("cannot compare {path} with run {run_id}: {error}")
    })?;
    for delta in &report.deltas {
        println!("delta: {}", describe_process(delta));
    }
    if report.deltas.is_empty() {
        Ok(ProcessTraceOutcome::Identical)
    } else {
        Ok(ProcessTraceOutcome::Delta(report.deltas.len()))
    }
}

/// One process delta as the operator reads it: the position and attempt are the identity, the
/// field is what changed.
fn describe_process(delta: &process_trace::ProcessTraceDelta) -> String {
    match delta {
        process_trace::ProcessTraceDelta::Added { position, attempt } => {
            format!("added position {position} attempt {attempt}")
        }
        process_trace::ProcessTraceDelta::Missing { position, attempt } => {
            format!("missing position {position} attempt {attempt}")
        }
        process_trace::ProcessTraceDelta::Mismatch {
            position,
            attempt,
            field,
            old,
            new,
        } => format!("position {position} attempt {attempt} {field}: {old:?} -> {new:?}"),
    }
}

/// Write one trace file, creating its directory when it has one.
fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
}
