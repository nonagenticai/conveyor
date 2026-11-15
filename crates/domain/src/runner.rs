//! The `runner/v1` contract between the conveyor and a stateless executor (ADR 0007 §Decision 4).
//!
//! A media capability runs in its own pod with no database, no job queue and no store credential.
//! The pod receives one [`Instruction`], writes its bytes through conveyor's job routes, and posts
//! one [`RunResult`]. This module is the whole interface: the schemas in
//! `docs/contracts/runner/v1/` are **generated** from these types, never hand-maintained, and the
//! executor repository vendors them with a `sha256` pin.
//!
//! Three rules the types enforce rather than document:
//!
//! 1. **The declared output set is closed.** An executor may only produce a name the instruction
//!    declared, at most the declared size, with the declared content type. A result that invents an
//!    artefact is refused, not filtered.
//! 2. **A result belongs to one attempt.** `(job_id, attempt)` is the idempotency key; a stale
//!    attempt cannot finish a newer one (`crate::runner::RunResult::validate`).
//! 3. **Nothing here trusts the clock or the network.** Validation is pure; the lease, the token
//!    expiry and the deadline are checked by the caller that holds the job row.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The one schema tag this build speaks.
pub const CONTRACT: &str = "runner/v1";

/// The longest capability slug accepted (`media.render_html`, `media.compile`, …).
const MAX_KIND_LEN: usize = 64;

/// One unit of work for a stateless executor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    /// Always [`CONTRACT`]; a different tag is refused rather than version-negotiated.
    pub schema: String,
    /// The conveyor job this attempt executes.
    pub job_id: Uuid,
    /// The tenant the job belongs to. Every byte route re-checks it against the job row.
    pub tenant: Uuid,
    /// The capability slug, e.g. `media.render_html`.
    pub kind: String,
    /// 1 for the first dispatch; a retry after an infrastructure failure increments it.
    pub attempt: i32,
    /// Groups the attempts of one logical request for tracing.
    pub correlation_id: Uuid,
    /// Wall-clock deadline as Unix milliseconds. The pod's `activeDeadlineSeconds` is derived from
    /// it; the token expires with it.
    pub deadline_unix_ms: i64,
    /// Kind-versioned parameters. The executor validates its own shape; this contract does not
    /// pretend to know every kind's vocabulary.
    pub params: serde_json::Value,
    /// Inputs the executor may fetch through `GET /api/jobs/{id}/inputs/{name}`.
    pub inputs: Vec<Input>,
    /// The **closed** set of artefacts the executor may produce.
    pub outputs: Vec<Output>,
    /// Resource and size caps. The Job spec is built from these, not from a per-kind default.
    pub limits: Limits,
    /// In-cluster address of the conveyor API the executor calls.
    pub conveyor_base_url: String,
    /// Trace identifier for logs and events.
    pub trace_id: String,
}

/// One input document the executor may fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// Route segment: `GET /api/jobs/{job_id}/inputs/{name}`.
    pub name: String,
    /// The document row the bytes belong to; the route resolves it and re-checks the tenant.
    pub document_id: Uuid,
    /// SHA-256 of the bytes, hex. The executor may refuse early when a download does not match.
    pub sha256: String,
    pub content_type: String,
    pub bytes: i64,
}

/// One artefact the executor may produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Output {
    /// Route segment: `POST /api/jobs/{job_id}/outputs/{name}`.
    pub name: String,
    /// The executor must echo this exact type; a different one is refused by the route.
    pub content_type: String,
    /// The largest acceptable body for this artefact.
    pub max_bytes: i64,
}

/// Caps carried into the Job spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// CPU request/limit in millicores.
    pub cpu_millis: u32,
    /// Memory limit in bytes.
    pub memory_bytes: i64,
    /// Wall-clock cap in seconds. `activeDeadlineSeconds` on the Job.
    pub wall_secs: i64,
    /// The sum of every declared output's size is checked against this.
    pub max_output_bytes: i64,
}

/// The terminal state an executor reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Succeeded,
    Failed,
}

/// Why an executor failed, in the vocabulary the dispatcher retries on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// The instruction or its inputs were unusable; retrying cannot help.
    Input,
    /// An upstream provider refused or failed; the dispatcher decides by `code`.
    Provider,
    /// A transient condition; the dispatcher may retry once.
    Transient,
    /// A permanent defect; the job is dead-lettered.
    Permanent,
}

/// The failure detail carried by a `failed` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErrorInfo {
    pub class: ErrorClass,
    /// A stable machine code (`ffmpeg_missing`, `render_timeout`, `provider_5xx`, …).
    pub code: String,
    /// One human sentence; never a stack trace.
    pub message: String,
}

/// One produced artefact, as reported by the executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Artefact {
    /// Must match a declared [`Output::name`].
    pub name: String,
    /// Hex SHA-256. The route recomputes it while writing; a mismatch refuses the body.
    pub sha256: String,
    pub bytes: i64,
    /// Must equal the declared [`Output::content_type`].
    pub content_type: String,
}

/// What the executor posts to `POST /api/jobs/{id}/result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunResult {
    /// Always [`CONTRACT`].
    pub schema: String,
    /// Must equal the instruction's `job_id`.
    pub job_id: Uuid,
    /// Must equal the instruction's `attempt`.
    pub attempt: i32,
    pub status: Status,
    /// Present iff `status` is [`Status::Failed`].
    pub error: Option<ErrorInfo>,
    /// Every declared output appears exactly once on success; empty is legal on failure.
    pub artefacts: Vec<Artefact>,
    /// Free-form measurements (durations, counts). Never used for control flow.
    pub metrics: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

/// Why an instruction or result was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error("unsupported contract: {0}")]
    UnsupportedContract(String),
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} is out of range")]
    OutOfRange { field: &'static str },
    #[error("duplicate {field}: {name}")]
    Duplicate { field: &'static str, name: String },
    #[error("not a lowercase hex sha256: {0}")]
    BadSha256(String),
    #[error("output {0} was not declared by the instruction")]
    UndeclaredOutput(String),
    #[error("output {name} exceeds its declared size {max_bytes}")]
    OutputTooLarge { name: String, max_bytes: i64 },
    #[error("artefact {0} does not match the declared content type")]
    ContentTypeMismatch(String),
    #[error("a successful result must carry every declared output")]
    MissingOutputs,
    #[error("a successful result carries no error; a failed one must")]
    StatusErrorMismatch,
    #[error("the result does not belong to this attempt")]
    AttemptMismatch,
    #[error("the result does not belong to this job")]
    JobMismatch,
    #[error("finished_at precedes started_at")]
    TimeReversed,
    #[error("output names exceed max_output_bytes in total")]
    TotalOutputTooLarge,
}

/// `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$` over at most [`MAX_KIND_LEN`] bytes.
fn kind_is_well_formed(kind: &str) -> bool {
    if kind.is_empty() || kind.len() > MAX_KIND_LEN {
        return false;
    }
    let mut segments = kind.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    if !segment_is_well_formed(first) {
        return false;
    }
    let rest: Vec<&str> = segments.collect();
    !rest.is_empty() && rest.iter().all(|s| segment_is_well_formed(s))
}

fn segment_is_well_formed(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Instruction {
    /// Refuse an instruction the conveyor would never mint, and an executor must never accept.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema != CONTRACT {
            return Err(ContractError::UnsupportedContract(self.schema.clone()));
        }
        if !kind_is_well_formed(&self.kind) {
            return Err(ContractError::OutOfRange { field: "kind" });
        }
        if self.attempt < 1 {
            return Err(ContractError::OutOfRange { field: "attempt" });
        }
        if self.deadline_unix_ms <= 0 {
            return Err(ContractError::OutOfRange {
                field: "deadline_unix_ms",
            });
        }
        if self.trace_id.trim().is_empty() {
            return Err(ContractError::Empty { field: "trace_id" });
        }
        if !(self.conveyor_base_url.starts_with("http://")
            || self.conveyor_base_url.starts_with("https://"))
        {
            return Err(ContractError::OutOfRange {
                field: "conveyor_base_url",
            });
        }
        if self.limits.cpu_millis == 0 {
            return Err(ContractError::OutOfRange {
                field: "limits.cpu_millis",
            });
        }
        if self.limits.memory_bytes <= 0
            || self.limits.wall_secs <= 0
            || self.limits.max_output_bytes <= 0
        {
            return Err(ContractError::OutOfRange { field: "limits" });
        }

        let mut seen_inputs = std::collections::BTreeSet::new();
        for input in &self.inputs {
            if input.name.trim().is_empty() {
                return Err(ContractError::Empty {
                    field: "inputs[].name",
                });
            }
            if !seen_inputs.insert(input.name.as_str()) {
                return Err(ContractError::Duplicate {
                    field: "input",
                    name: input.name.clone(),
                });
            }
            if input.bytes < 0 {
                return Err(ContractError::OutOfRange {
                    field: "inputs[].bytes",
                });
            }
            if !is_hex_sha256(&input.sha256) {
                return Err(ContractError::BadSha256(input.sha256.clone()));
            }
            if input.content_type.trim().is_empty() {
                return Err(ContractError::Empty {
                    field: "inputs[].content_type",
                });
            }
        }

        let mut seen_outputs = std::collections::BTreeSet::new();
        let mut declared_total: i64 = 0;
        for output in &self.outputs {
            if output.name.trim().is_empty() {
                return Err(ContractError::Empty {
                    field: "outputs[].name",
                });
            }
            if !seen_outputs.insert(output.name.as_str()) {
                return Err(ContractError::Duplicate {
                    field: "output",
                    name: output.name.clone(),
                });
            }
            if output.max_bytes <= 0 {
                return Err(ContractError::OutOfRange {
                    field: "outputs[].max_bytes",
                });
            }
            if output.content_type.trim().is_empty() {
                return Err(ContractError::Empty {
                    field: "outputs[].content_type",
                });
            }
            declared_total = declared_total.saturating_add(output.max_bytes);
        }
        if declared_total > self.limits.max_output_bytes {
            return Err(ContractError::TotalOutputTooLarge);
        }
        Ok(())
    }

    /// The deadline as a wall-clock instant, for callers comparing it to `now()`.
    pub fn deadline(&self) -> Option<DateTime<Utc>> {
        DateTime::<Utc>::from_timestamp_millis(self.deadline_unix_ms)
    }
}

impl RunResult {
    /// Refuse a result that does not belong to `instruction`, or that reports bytes the
    /// instruction never allowed.
    pub fn validate(&self, instruction: &Instruction) -> Result<(), ContractError> {
        if self.schema != CONTRACT {
            return Err(ContractError::UnsupportedContract(self.schema.clone()));
        }
        if self.job_id != instruction.job_id {
            return Err(ContractError::JobMismatch);
        }
        if self.attempt != instruction.attempt {
            return Err(ContractError::AttemptMismatch);
        }
        match (self.status, self.error.is_some()) {
            (Status::Succeeded, false) | (Status::Failed, true) => {}
            _ => return Err(ContractError::StatusErrorMismatch),
        }
        if self.finished_at < self.started_at {
            return Err(ContractError::TimeReversed);
        }

        let mut seen = std::collections::BTreeSet::new();
        for artefact in &self.artefacts {
            let Some(declared) = instruction.outputs.iter().find(|o| o.name == artefact.name)
            else {
                return Err(ContractError::UndeclaredOutput(artefact.name.clone()));
            };
            if !seen.insert(artefact.name.as_str()) {
                return Err(ContractError::Duplicate {
                    field: "artefact",
                    name: artefact.name.clone(),
                });
            }
            if artefact.bytes < 0 || artefact.bytes > declared.max_bytes {
                return Err(ContractError::OutputTooLarge {
                    name: artefact.name.clone(),
                    max_bytes: declared.max_bytes,
                });
            }
            if artefact.content_type != declared.content_type {
                return Err(ContractError::ContentTypeMismatch(artefact.name.clone()));
            }
            if !is_hex_sha256(&artefact.sha256) {
                return Err(ContractError::BadSha256(artefact.sha256.clone()));
            }
        }

        if self.status == Status::Succeeded {
            for declared in &instruction.outputs {
                if !seen.contains(declared.name.as_str()) {
                    return Err(ContractError::MissingOutputs);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn instruction() -> Instruction {
        Instruction {
            schema: CONTRACT.to_owned(),
            job_id: Uuid::now_v7(),
            tenant: Uuid::now_v7(),
            kind: "media.render_html".to_owned(),
            attempt: 1,
            correlation_id: Uuid::now_v7(),
            deadline_unix_ms: 1_763_000_000_000,
            params: serde_json::json!({"html": "<p>hi</p>"}),
            inputs: vec![Input {
                name: "source".to_owned(),
                document_id: Uuid::now_v7(),
                sha256: sha('a'),
                content_type: "text/html".to_owned(),
                bytes: 11,
            }],
            outputs: vec![Output {
                name: "page".to_owned(),
                content_type: "image/png".to_owned(),
                max_bytes: 8 * 1024 * 1024,
            }],
            limits: Limits {
                cpu_millis: 500,
                memory_bytes: 512 * 1024 * 1024,
                wall_secs: 60,
                max_output_bytes: 16 * 1024 * 1024,
            },
            conveyor_base_url: "http://conveyor.conveyor.svc.cluster.local:8080".to_owned(),
            trace_id: "trace-1".to_owned(),
        }
    }

    fn result_for(instruction: &Instruction) -> RunResult {
        RunResult {
            schema: CONTRACT.to_owned(),
            job_id: instruction.job_id,
            attempt: instruction.attempt,
            status: Status::Succeeded,
            error: None,
            artefacts: vec![Artefact {
                name: "page".to_owned(),
                sha256: sha('b'),
                bytes: 1024,
                content_type: "image/png".to_owned(),
            }],
            metrics: serde_json::json!({"frames": 1}),
            started_at: DateTime::from_timestamp(1_762_999_000, 0).unwrap(),
            finished_at: DateTime::from_timestamp(1_762_999_030, 0).unwrap(),
        }
    }

    #[test]
    fn a_well_formed_instruction_validates() {
        instruction().validate().unwrap();
    }

    #[test]
    fn an_unknown_schema_tag_is_refused() {
        let mut i = instruction();
        i.schema = "runner/v2".to_owned();
        assert_eq!(
            i.validate(),
            Err(ContractError::UnsupportedContract("runner/v2".to_owned()))
        );
    }

    #[test]
    fn a_kind_without_a_dot_is_refused() {
        let mut i = instruction();
        i.kind = "render".to_owned();
        assert_eq!(
            i.validate(),
            Err(ContractError::OutOfRange { field: "kind" })
        );
    }

    #[test]
    fn duplicate_output_names_are_refused() {
        let mut i = instruction();
        i.outputs.push(i.outputs[0].clone());
        assert_eq!(
            i.validate(),
            Err(ContractError::Duplicate {
                field: "output",
                name: "page".to_owned()
            })
        );
    }

    #[test]
    fn declared_outputs_cannot_exceed_the_total_cap() {
        let mut i = instruction();
        i.limits.max_output_bytes = 1024;
        assert_eq!(i.validate(), Err(ContractError::TotalOutputTooLarge));
    }

    #[test]
    fn a_successful_result_must_carry_every_declared_output() {
        let i = instruction();
        let mut r = result_for(&i);
        r.artefacts.clear();
        assert_eq!(r.validate(&i), Err(ContractError::MissingOutputs));
    }

    #[test]
    fn an_undeclared_artefact_is_refused() {
        let i = instruction();
        let mut r = result_for(&i);
        r.artefacts[0].name = "other".to_owned();
        assert_eq!(
            r.validate(&i),
            Err(ContractError::UndeclaredOutput("other".to_owned()))
        );
    }

    #[test]
    fn an_artefact_over_its_declared_size_is_refused() {
        let i = instruction();
        let mut r = result_for(&i);
        r.artefacts[0].bytes = i.outputs[0].max_bytes + 1;
        assert_eq!(
            r.validate(&i),
            Err(ContractError::OutputTooLarge {
                name: "page".to_owned(),
                max_bytes: i.outputs[0].max_bytes
            })
        );
    }

    #[test]
    fn a_result_from_another_attempt_is_refused() {
        let i = instruction();
        let mut r = result_for(&i);
        r.attempt = 2;
        assert_eq!(r.validate(&i), Err(ContractError::AttemptMismatch));
    }

    #[test]
    fn a_successful_result_with_an_error_is_refused() {
        let i = instruction();
        let mut r = result_for(&i);
        r.error = Some(ErrorInfo {
            class: ErrorClass::Permanent,
            code: "x".to_owned(),
            message: "x".to_owned(),
        });
        assert_eq!(r.validate(&i), Err(ContractError::StatusErrorMismatch));
    }

    #[test]
    fn an_instruction_refuses_an_unknown_field() {
        let mut value = serde_json::to_value(instruction()).unwrap();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Instruction>(value).is_err());
    }
}
