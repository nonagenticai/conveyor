#![forbid(unsafe_code)]
//! The reviewed expectation for a re-authored flow (S13 §5.4).
//!
//! The old recording is **evidence, not an oracle**: a human reads the effect path it produced and
//! commits it as `fixtures/<x>.expected.json`. The gate then runs the re-authored flow and compares
//! the exported trace to that file. Payloads never participate — `input_hash`/`output_hash` stay in
//! the trace as forensics, and [`DiffMode::Migration`] ignores them.
//!
//! A step names both ids: `node_id` is the conveyor flow's id (what the comparison uses) and
//! `old_node_id` is the n8n node name (provenance, never compared). Derivation normalises the old
//! name and refuses it by name when the flow has no such node, so a mapping is never skipped.

use conveyor_domain::compare::content_hash;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::definition::Definition;
use crate::trace::{
    DiffMode, DiffReport, TraceDiffError, TraceKind, TraceOutcome, TraceRecord, diff_records_with,
    effects, normalise_node_id,
};

/// Where an expectation came from. A reader can re-run the derivation and get the same steps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The recorded n8n execution id.
    pub execution_id: i64,
    /// `sha256:` of the **projected records** the steps were derived from (canonical JSON of the
    /// projection, not of the recorded file). This hash pins what the derivation actually read.
    pub projection_sha256: String,
    /// The authored flow the node ids were validated against.
    pub flow: String,
}

/// One expected position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedStep {
    /// Execution order within the run (1-based).
    pub position: u32,
    /// The conveyor flow's node id — the comparison value.
    pub node_id: String,
    /// The n8n node name it came from — provenance, never compared.
    pub old_node_id: String,
    /// The consequential effect after the S0 effect map.
    pub effect: String,
    /// How the position ended.
    pub outcome: TraceOutcome,
}

/// A reviewed expectation file (`fixtures/<x>.expected.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    /// `a`, `b`, `c`.
    pub fixture: String,
    /// The old workflow's name (`cap.analysis.consult`, `wip.pull`, …).
    pub workflow_key: String,
    /// Where the steps came from.
    pub provenance: Provenance,
    /// The ordered path the gate compares.
    pub steps: Vec<ExpectedStep>,
    /// The business result the flow exists to produce. Authored, never derived; asserted through
    /// the conveyor's own read actions by the fixture test (`end_state_matches`).
    pub end_state: Map<String, Value>,
}

impl Expected {
    /// Derive the expected path from a projected recording, validating every node id against the
    /// authored flow. The `end_state` is left empty: it is the one part a human must author, and
    /// [`Expected::validate`] refuses a file that does not carry it.
    pub fn derive(
        old: &[TraceRecord],
        flow: &Definition,
        flow_path: &str,
        fixture: &str,
        execution_id: i64,
    ) -> Result<Self, TraceDiffError> {
        let old_effects = effects(old)?;
        let mut steps = Vec::with_capacity(old_effects.len());
        for effect in old_effects {
            let node_id = normalise_node_id(&effect.node_id);
            if flow.node(&node_id).is_none() {
                return Err(TraceDiffError::ExpectedInvalid {
                    fixture: fixture.to_owned(),
                    detail: format!(
                        "recorded node {:?} normalises to {node_id:?}, which is not a node in {flow_path}",
                        effect.node_id
                    ),
                });
            }
            steps.push(ExpectedStep {
                position: effect.position,
                node_id,
                old_node_id: effect.node_id,
                effect: effect.effect,
                outcome: effect.outcome,
            });
        }
        let projection_sha256 = format!(
            "sha256:{}",
            content_hash(&serde_json::to_value(old).map_err(|error| {
                TraceDiffError::ExpectedInvalid {
                    fixture: fixture.to_owned(),
                    detail: format!("cannot hash the recorded projection: {error}"),
                }
            })?)
        );
        let workflow_key = old
            .first()
            .map(|record| record.workflow_key.clone())
            .unwrap_or_default();
        Ok(Self {
            fixture: fixture.to_owned(),
            workflow_key,
            provenance: Provenance {
                execution_id,
                projection_sha256,
                flow: flow_path.to_owned(),
            },
            steps,
            end_state: Map::new(),
        })
    }

    /// Parse a committed expectation file.
    pub fn parse(text: &str) -> Result<Self, TraceDiffError> {
        let expected: Self =
            serde_json::from_str(text).map_err(|error| TraceDiffError::ExpectedInvalid {
                fixture: "<file>".to_owned(),
                detail: format!("not an expectation: {error}"),
            })?;
        Ok(expected)
    }

    /// Serialise for writing (pretty, trailing newline).
    pub fn to_json_pretty(&self) -> Result<String, TraceDiffError> {
        let mut text = serde_json::to_string_pretty(self).map_err(|error| {
            TraceDiffError::ExpectedInvalid {
                fixture: self.fixture.clone(),
                detail: format!("cannot serialise: {error}"),
            }
        })?;
        text.push('\n');
        Ok(text)
    }

    /// Fail closed: a file with no steps, no end state or no workflow key is invalid, named.
    pub fn validate(&self) -> Result<(), TraceDiffError> {
        if self.fixture.is_empty() {
            return Err(TraceDiffError::ExpectedInvalid {
                fixture: "<file>".to_owned(),
                detail: "the fixture name is empty".to_owned(),
            });
        }
        if self.workflow_key.is_empty() {
            return Err(TraceDiffError::ExpectedInvalid {
                fixture: self.fixture.clone(),
                detail: "the workflow key is empty".to_owned(),
            });
        }
        if self.steps.is_empty() {
            return Err(TraceDiffError::ExpectedInvalid {
                fixture: self.fixture.clone(),
                detail: "there is no expected step".to_owned(),
            });
        }
        if self.end_state.is_empty() {
            return Err(TraceDiffError::ExpectedInvalid {
                fixture: self.fixture.clone(),
                detail: "there is no end_state; a path without a business result is not a proof"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Compare a run's exported trace to this expectation, through the one diff path.
    ///
    /// `DiffMode::Migration` is the cross-system mode: node ids are normalised (the expected ids are
    /// already the conveyor's, so they normalise to themselves) and payload hashes are ignored.
    pub fn compare(&self, actual: &[TraceRecord]) -> Result<DiffReport, TraceDiffError> {
        self.validate()?;
        diff_records_with(&self.records(), actual, DiffMode::Migration)
    }

    /// Assert the authored business result against facts the caller read from the run.
    ///
    /// Returns one line per mismatch (absent counts as a mismatch); an empty vector means the run
    /// produced the declared result. Keys present in `observed` but not declared are ignored.
    pub fn end_state_matches(&self, observed: &Map<String, Value>) -> Vec<String> {
        let mut mismatches = Vec::new();
        for (key, want) in &self.end_state {
            match observed.get(key) {
                Some(got) if got == want => {}
                Some(got) => mismatches.push(format!(
                    "end_state.{key}: expected {}, observed {}",
                    compact(want),
                    compact(got)
                )),
                None => mismatches.push(format!(
                    "end_state.{key}: expected {}, observed <absent>",
                    compact(want)
                )),
            }
        }
        mismatches
    }

    /// The expected path as trace records, so the comparison is the trace's own (one diff path).
    ///
    /// `kind` is not part of the pass definition and is filled with [`TraceKind::Action`]; the
    /// hashes are empty and [`DiffMode::Migration`] never reads them.
    fn records(&self) -> Vec<TraceRecord> {
        self.steps
            .iter()
            .map(|step| TraceRecord {
                run_id: String::new(),
                workflow_key: self.workflow_key.clone(),
                workflow_version: 0,
                node_id: step.node_id.clone(),
                position: step.position,
                attempt: 1,
                kind: TraceKind::Action,
                effect: step.effect.clone(),
                input_hash: String::new(),
                output_hash: String::new(),
                duration_ms: None,
                outcome: step.outcome,
            })
            .collect()
    }
}

/// A JSON value on one line, for a readable mismatch.
fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_owned())
}
