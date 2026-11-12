#![forbid(unsafe_code)]
//! The eval instrument's vocabulary, its status machine and its pure predicates (S19 Part A;
//! SPEC D25/D26, CONTRACT §20).
//!
//! - [`SubjectKind`] — what an eval measures: a completion, a copilot turn, a process, an
//!   external endpoint, or an agent run (declared, not yet runnable).
//! - [`EvalStatus`] — `draft → running ⇄ climbing`, with `retired` terminal. **There is no
//!   `ready`**: readiness is a computed predicate ([`Readiness`]), never a stored state, so a
//!   decided sign-off cannot go stale in a status column (data-arch §2.19).
//! - [`Variant`] — the report's `baseline | v<N>` label, parsed under the database's own CHECK
//!   (`variant = 'baseline' OR variant ~ '^v[0-9]+$'`), so the two vocabularies cannot drift.
//! - [`EvalGate`], [`MissingGate`] and [`Readiness`] — the two sign-offs a run needs, and the
//!   ordered reasons a run is refused.
//! - [`RetryPolicy`] — exponential backoff whose jitter is a caller-supplied argument, so the
//!   function is pure and a replay gets the same schedule.
//! - [`SubjectInput`], [`Instrument`] and [`instrument_digest`] — the one shape a model is ever
//!   shown, and the hash that voids a sign-off when the instrument changes.
//!
//! ⛔ The answer key is **structural**, not a convention: [`SubjectInput`] has no `expected`
//! field, so no prompt builder can leak one by accident (CONTRACT §20 trap 6). The full case —
//! `expected` included — is [`CaseMaterial`], which only the grader and the digest ever see.
//!
//! Nothing here performs IO. The store writes what these functions agreed to, and the
//! migration's CHECK constraints refuse the vocabulary at the other end.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::compare::{canonical_json, content_hash};

/// Why an eval value, transition or split was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// The pair is not in the documented transition table (including every edge out of
    /// `retired`, which is terminal).
    #[error("an eval cannot transition from {from} to {to}")]
    IllegalTransition {
        /// The status being left.
        from: EvalStatus,
        /// The status being entered.
        to: EvalStatus,
    },
    /// The string is not in a column's or an enum's vocabulary.
    #[error("unknown {what} {value:?}")]
    Vocabulary {
        /// The column or vocabulary that refused the value.
        what: &'static str,
        /// The refused spelling.
        value: String,
    },
    /// `train_ratio` is not a finite fraction strictly inside `(0, 1)`.
    #[error("train ratio must be a finite number strictly between 0 and 1")]
    Ratio,
    /// A split needs at least one case.
    #[error("a split needs at least one case")]
    NoCases,
    /// The run pair is not in the documented table (including every edge out of `void` and
    /// `cancelled`, which are terminal).
    ///
    /// Carried as its own variant rather than reusing [`Self::IllegalTransition`] so an eval's
    /// lifecycle and a run's can never be confused in a `409`'s detail.
    #[error("an eval run cannot transition from {from} to {to}")]
    IllegalRunTransition {
        /// The state being left.
        from: crate::eval::runner::RunStatus,
        /// The state being entered.
        to: crate::eval::runner::RunStatus,
    },
}

/// What an eval measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    /// A raw completion against a model reference.
    Completions,
    /// A copilot turn (S10) replayed against the conversation vocabulary.
    Copilot,
    /// A process (S11) executed through the registry.
    Process,
    /// An external endpoint, called over HTTP.
    External,
    /// An agent run. Declared so the vocabulary is stable; not runnable yet (Part B).
    AgentRun,
}

impl SubjectKind {
    /// Every subject kind.
    pub const ALL: [SubjectKind; 5] = [
        SubjectKind::Completions,
        SubjectKind::Copilot,
        SubjectKind::Process,
        SubjectKind::External,
        SubjectKind::AgentRun,
    ];

    /// The stored `eval.subject_kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectKind::Completions => "completions",
            SubjectKind::Copilot => "copilot",
            SubjectKind::Process => "process",
            SubjectKind::External => "external",
            SubjectKind::AgentRun => "agent_run",
        }
    }

    /// True for every kind a run can execute today.
    ///
    /// `agent_run` is refused at run start because no registered action can execute one yet
    /// (CONTRACT §20); the refusal is Part B's, the fact is this predicate.
    pub fn is_runnable(self) -> bool {
        !matches!(self, SubjectKind::AgentRun)
    }
}

impl fmt::Display for SubjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SubjectKind {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| EvalError::Vocabulary {
                what: "subject_kind",
                value: raw.to_owned(),
            })
    }
}

/// The eval lifecycle: `draft → running ⇄ climbing`, `retired` terminal.
///
/// There is deliberately **no `ready`**: readiness is [`Readiness::evaluate`] over facts the
/// store recomputes on every read, so an edit that changes the instrument reopens a gate without
/// anyone writing a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalStatus {
    /// Being authored; no run may start.
    Draft,
    /// A run is in flight against the current version.
    Running,
    /// A climb (S20) is proposing changes between rounds.
    Climbing,
    /// Withdrawn; terminal.
    Retired,
}

impl EvalStatus {
    /// Every status.
    pub const ALL: [EvalStatus; 4] = [
        EvalStatus::Draft,
        EvalStatus::Running,
        EvalStatus::Climbing,
        EvalStatus::Retired,
    ];

    /// The terminal statuses. A retired eval accepts no transition, not even one naming itself.
    pub const TERMINAL: [EvalStatus; 1] = [EvalStatus::Retired];

    /// The stored `eval.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            EvalStatus::Draft => "draft",
            EvalStatus::Running => "running",
            EvalStatus::Climbing => "climbing",
            EvalStatus::Retired => "retired",
        }
    }

    /// True for `retired`.
    pub fn is_terminal(self) -> bool {
        Self::TERMINAL.contains(&self)
    }

    /// The documented transition table (data-arch §2.19).
    pub fn may_transition_to(self, to: EvalStatus) -> bool {
        matches!(
            (self, to),
            (EvalStatus::Draft, EvalStatus::Running)
                | (EvalStatus::Draft, EvalStatus::Retired)
                | (EvalStatus::Running, EvalStatus::Climbing)
                | (EvalStatus::Running, EvalStatus::Retired)
                | (EvalStatus::Climbing, EvalStatus::Running)
                | (EvalStatus::Climbing, EvalStatus::Retired)
        )
    }

    /// Apply a transition, or refuse it.
    ///
    /// `retired` has no outgoing edge at all, so every attempt from it — including to itself —
    /// is [`EvalError::IllegalTransition`]. The store turns that into a 409 and writes nothing.
    pub fn transition(self, to: EvalStatus) -> Result<Self, EvalError> {
        if self.may_transition_to(to) {
            Ok(to)
        } else {
            Err(EvalError::IllegalTransition { from: self, to })
        }
    }
}

impl fmt::Display for EvalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EvalStatus {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
            .ok_or_else(|| EvalError::Vocabulary {
                what: "eval_status",
                value: raw.to_owned(),
            })
    }
}

/// A run's label in the report: `baseline`, or the n-th proposed variant.
///
/// The grammar is the database's, not a second one: `baseline | ^v[0-9]+$`. Parsing normalises
/// the digits — `v007` is `v7` — which is the spelling the report legend shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Variant {
    /// The instrument's current behaviour, before any change.
    Baseline,
    /// A numbered proposed change.
    Numbered(u32),
}

impl Variant {
    /// Parse the `eval_run.variant` spelling.
    ///
    /// Rejects `v` (no digits), `V1` (wrong case), `baseline1`, `1` (no prefix), the empty string,
    /// anything with a non-ASCII-digit character, and a digit run that overflows `u32`
    /// ([`EvalError::Vocabulary`] in every case).
    pub fn parse(raw: &str) -> Result<Self, EvalError> {
        if raw == "baseline" {
            return Ok(Variant::Baseline);
        }
        let Some(digits) = raw.strip_prefix('v') else {
            return Err(variant_vocabulary(raw));
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(variant_vocabulary(raw));
        }
        digits
            .parse::<u32>()
            .map(Variant::Numbered)
            .map_err(|_| variant_vocabulary(raw))
    }

    /// The column spelling.
    ///
    /// A `String` rather than `&'static str` because `v<N>` is not static; `baseline` still
    /// costs one allocation, which is the price of one vocabulary instead of two spellings.
    pub fn as_str(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Variant::Baseline => f.write_str("baseline"),
            Variant::Numbered(number) => write!(f, "v{number}"),
        }
    }
}

impl FromStr for Variant {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

/// The refusal for a `variant` spelling the database's CHECK would also refuse.
fn variant_vocabulary(raw: &str) -> EvalError {
    EvalError::Vocabulary {
        what: "variant",
        value: raw.to_owned(),
    }
}

/// A sign-off gate an eval records.
///
/// The wire strings are the `approval.gate` values. `eval_inputs` and `eval_grading` gate a run
/// ([`EvalGate::gates_run`]); `eval_cost` and `eval_harness` are recorded and audited but never
/// block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EvalGate {
    /// The case inputs are approved.
    #[serde(rename = "eval_inputs")]
    Inputs,
    /// The metrics and graders are approved.
    #[serde(rename = "eval_grading")]
    Grading,
    /// The cost ceiling is approved (recorded, does not gate a run).
    #[serde(rename = "eval_cost")]
    Cost,
    /// The harness is approved (recorded, does not gate a run).
    #[serde(rename = "eval_harness")]
    Harness,
}

impl EvalGate {
    /// Every gate.
    pub const ALL: [EvalGate; 4] = [
        EvalGate::Inputs,
        EvalGate::Grading,
        EvalGate::Cost,
        EvalGate::Harness,
    ];

    /// The stored `approval.gate`.
    pub fn as_str(self) -> &'static str {
        match self {
            EvalGate::Inputs => "eval_inputs",
            EvalGate::Grading => "eval_grading",
            EvalGate::Cost => "eval_cost",
            EvalGate::Harness => "eval_harness",
        }
    }

    /// True for the two gates a run cannot start without.
    pub fn gates_run(self) -> bool {
        matches!(self, EvalGate::Inputs | EvalGate::Grading)
    }
}

impl fmt::Display for EvalGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EvalGate {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|gate| gate.as_str() == raw)
            .ok_or_else(|| EvalError::Vocabulary {
                what: "gate",
                value: raw.to_owned(),
            })
    }
}

/// One reason an eval is not ready to run.
///
/// The naming is the wire contract: the two sign-off gates reuse their `approval.gate` spelling,
/// and the three split reasons name exactly what is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MissingGate {
    /// No approved `eval_inputs` sign-off matches the current instrument digest.
    InputsSignoff,
    /// No approved `eval_grading` sign-off matches the current instrument digest.
    GradingSignoff,
    /// The version has no metric rows, so there is nothing to grade with.
    NoMetrics,
    /// No split has been drawn and locked.
    SplitMissing,
    /// The split is locked but some cases carry no `train`/`test` side.
    SplitIncomplete,
    /// The locked split no longer hashes to the version's `split_sha256` (a case was edited).
    SplitStale,
}

impl MissingGate {
    /// The wire string this gate is reported as.
    pub fn as_str(self) -> &'static str {
        match self {
            MissingGate::InputsSignoff => "eval_inputs",
            MissingGate::GradingSignoff => "eval_grading",
            MissingGate::NoMetrics => "metrics",
            MissingGate::SplitMissing => "split_missing",
            MissingGate::SplitIncomplete => "split_incomplete",
            MissingGate::SplitStale => "split_stale",
        }
    }
}

impl fmt::Display for MissingGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The facts readiness is computed from. The store derives them; the database stores none of
/// them (a stored "ready" would go stale the moment a case is edited).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadinessFacts {
    /// An approved `eval_inputs` sign-off whose `artifact_sha256` equals the version digest.
    pub inputs_approved: bool,
    /// An approved `eval_grading` sign-off whose `artifact_sha256` equals the version digest.
    pub grading_approved: bool,
    /// The number of metric rows on the version.
    pub metric_count: i64,
    /// A split hash is recorded and locked.
    pub split_locked: bool,
    /// Cases whose `eval_case.split` is still `none`.
    pub unsplit_cases: i64,
    /// The recomputed split hash equals the version's recorded `split_sha256`.
    pub split_matches: bool,
}

/// The ordered reasons an eval is not ready to run; empty means ready.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Readiness {
    missing: Vec<MissingGate>,
}

impl Readiness {
    /// Evaluate the facts.
    ///
    /// The order is fixed — inputs, grading, metrics, split-missing, split-incomplete,
    /// split-stale — so a refusal's body is stable enough to assert on. `SplitMissing` (no lock)
    /// and `SplitStale` (locked, hash mismatch) are mutually exclusive by construction.
    pub fn evaluate(facts: ReadinessFacts) -> Self {
        let mut missing = Vec::new();
        if !facts.inputs_approved {
            missing.push(MissingGate::InputsSignoff);
        }
        if !facts.grading_approved {
            missing.push(MissingGate::GradingSignoff);
        }
        if facts.metric_count == 0 {
            missing.push(MissingGate::NoMetrics);
        }
        if !facts.split_locked {
            missing.push(MissingGate::SplitMissing);
        }
        if facts.unsplit_cases > 0 {
            missing.push(MissingGate::SplitIncomplete);
        }
        if facts.split_locked && !facts.split_matches {
            missing.push(MissingGate::SplitStale);
        }
        Self { missing }
    }

    /// True when nothing is missing.
    pub fn is_ready(&self) -> bool {
        self.missing.is_empty()
    }

    /// The missing gates, in the documented order.
    pub fn missing(&self) -> &[MissingGate] {
        &self.missing
    }
}

/// The driver's retry schedule for one case attempt (Part B enforces it; the policy lives here
/// so the numbers are one value, not scattered constants).
///
/// Scoring is strict per attempt: a retried attempt never replaces an earlier attempt's grade.
/// The schedule only says when a retry may be issued.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Attempts per case, the first one included.
    pub max_attempts: u32,
    /// The delay before the second attempt.
    pub base_ms: u64,
    /// The ceiling every later delay saturates at.
    pub cap_ms: u64,
}

impl Default for RetryPolicy {
    /// The S19 defaults: 5 attempts (matching `job.max_attempts`), 500 ms base, 30 s cap.
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_ms: 500,
            cap_ms: 30_000,
        }
    }
}

impl RetryPolicy {
    /// The delay before retry number `attempt` (1-based): `min(base_ms * 2^(attempt-1), cap_ms)`.
    ///
    /// Total: attempt `0` is treated as attempt `1`, and a large attempt saturates at the cap
    /// instead of shifting past the width of `u64`.
    pub fn backoff(&self, attempt: u32) -> Duration {
        let exponent = attempt.max(1).saturating_sub(1).min(63);
        let raw = self.base_ms.saturating_mul(1u64 << exponent);
        Duration::from_millis(raw.min(self.cap_ms))
    }

    /// [`backoff`](Self::backoff) with jitter, derived from `entropy` rather than from a clock or
    /// an RNG.
    ///
    /// The result is in `[backoff/2, backoff]` and is a pure function of `(attempt, entropy)`, so
    /// the caller owns the randomness: supply a run id or a retry counter mixed with the case id,
    /// and a replayed run gets the same schedule. No RNG crate is a dependency.
    pub fn backoff_jittered(&self, attempt: u32, entropy: u64) -> Duration {
        let full_ms = u64::try_from(self.backoff(attempt).as_millis()).unwrap_or(u64::MAX);
        let half_ms = full_ms / 2;
        let span = full_ms - half_ms;
        // Never zero: `span + 1` is at least 1 even when the backoff is 0 ms.
        let width = span.saturating_add(1).max(1);
        Duration::from_millis(half_ms + entropy % width)
    }
}

/// The only thing a subject is ever shown: one input string.
///
/// ⛔ There is no `expected` field and there must never be one. The isolation is the type, not a
/// prompt instruction: a builder that has a [`SubjectInput`] has no answer key to leak (CONTRACT
/// §20 trap 6), and `tests/eval_isolation.rs` proves the serialised shape has exactly one key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SubjectInput<'a> {
    /// The case input, verbatim.
    pub input: &'a str,
}

/// One case row as the digest and the grader see it — `expected` included.
///
/// This type is never serialised into a prompt and never appears on a view; the store's list
/// rows have no `expected` at all, so the isolation does not depend on anyone remembering to drop
/// a field.
#[derive(Clone, Copy, Debug)]
pub struct CaseMaterial<'a> {
    /// The case's stable position in the version; the digest sorts by it.
    pub ordinal: i32,
    /// The input.
    pub input: &'a str,
    /// The tags, in order (the first tag is the split's stratum).
    pub tags: &'a [String],
    /// The answer key.
    pub expected: Option<&'a Value>,
    /// Structured attachments (files, context) the case needs.
    pub attachments: &'a Value,
    /// Where the case came from (`production | ticket | handwritten | synthesized`).
    pub source: &'a str,
    /// How the answer key was produced (`human_written | human_verified | model`).
    pub gold_provenance: &'a str,
    /// The model that produced the key, when `gold_provenance` is `model`.
    pub gold_model: Option<&'a str>,
    /// Free-form case metadata.
    pub meta: &'a Value,
}

/// One metric row as the digest sees it.
#[derive(Clone, Copy, Debug)]
pub struct MetricMaterial<'a> {
    /// The metric key (`^[a-z][a-z0-9_]*$`).
    pub key: &'a str,
    /// `quality | perf`.
    pub role: &'a str,
    /// `binary | judge | float`.
    pub kind: &'a str,
    /// `higher | lower`.
    pub better: &'a str,
    /// The report legend; at most 14 characters (`eval_metric.label` CHECK).
    pub label: &'a str,
    /// The unit shown next to the value, when the value has one.
    pub unit: Option<&'a str>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: &'a str,
    /// The grader's configuration object.
    pub grader: &'a Value,
    /// The rubric text, required by `rubric` and `pairwise` graders.
    pub rubric: Option<&'a str>,
    /// The metric's stable position in the version; the digest sorts by it.
    pub ordinal: i32,
}

/// Everything a version's instrument digest covers: the case rows, the metric rows, the prices
/// and the harness paths.
#[derive(Clone, Copy, Debug)]
pub struct Instrument<'a> {
    /// The version's cases.
    pub cases: &'a [CaseMaterial<'a>],
    /// The version's metrics.
    pub metrics: &'a [MetricMaterial<'a>],
    /// The price table the cost estimate uses.
    pub prices: &'a Value,
    /// The harness files the run needs, in order.
    pub harness_paths: &'a [String],
}

/// The version's `instrument_digest`: SHA-256 of the canonical JSON of the whole instrument.
///
/// Rows are sorted by `ordinal` before hashing, so the digest is a property of the instrument and
/// not of the order the store happened to return its rows in. A sign-off records this value; an
/// edit to any field — an input, an answer key, a tag's position, a metric label, the prices, the
/// harness — changes the digest and therefore voids the sign-off with no status write
/// (CONTRACT §20 trap 4).
pub fn instrument_digest(instrument: &Instrument<'_>) -> String {
    let mut cases: Vec<Value> = instrument.cases.iter().map(case_value).collect();
    let mut metrics: Vec<Value> = instrument.metrics.iter().map(metric_value).collect();
    sort_rows(&mut cases);
    sort_rows(&mut metrics);
    content_hash(&serde_json::json!({
        "prices": instrument.prices,
        "harness_paths": instrument.harness_paths,
        "cases": cases,
        "metrics": metrics,
    }))
}

/// One case as the JSON object the digest hashes.
fn case_value(case: &CaseMaterial<'_>) -> Value {
    serde_json::json!({
        "ordinal": case.ordinal,
        "input": case.input,
        "tags": case.tags,
        "expected": case.expected,
        "attachments": case.attachments,
        "source": case.source,
        "gold_provenance": case.gold_provenance,
        "gold_model": case.gold_model,
        "meta": case.meta,
    })
}

/// One metric as the JSON object the digest hashes.
fn metric_value(metric: &MetricMaterial<'_>) -> Value {
    serde_json::json!({
        "key": metric.key,
        "role": metric.role,
        "kind": metric.kind,
        "better": metric.better,
        "label": metric.label,
        "unit": metric.unit,
        "grader_kind": metric.grader_kind,
        "grader": metric.grader,
        "rubric": metric.rubric,
        "ordinal": metric.ordinal,
    })
}

/// Sort rows by `ordinal`, with the canonical form as the tie-break.
///
/// The tie-break makes the key a total order over the rows themselves, so even a duplicate
/// ordinal (which the schema refuses) could not make the digest depend on the caller's order.
fn sort_rows(rows: &mut [Value]) {
    rows.sort_by(|left, right| {
        let left_ordinal = left
            .get("ordinal")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let right_ordinal = right
            .get("ordinal")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        left_ordinal
            .cmp(&right_ordinal)
            .then_with(|| canonical_json(left).cmp(&canonical_json(right)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The documented table, spelled out so the matrix test compares against a list rather than
    /// against `may_transition_to` itself.
    const LEGAL: [(EvalStatus, EvalStatus); 6] = [
        (EvalStatus::Draft, EvalStatus::Running),
        (EvalStatus::Draft, EvalStatus::Retired),
        (EvalStatus::Running, EvalStatus::Climbing),
        (EvalStatus::Running, EvalStatus::Retired),
        (EvalStatus::Climbing, EvalStatus::Running),
        (EvalStatus::Climbing, EvalStatus::Retired),
    ];

    fn case<'a>(
        ordinal: i32,
        input: &'a str,
        tags: &'a [String],
        expected: Option<&'a Value>,
        attachments: &'a Value,
        meta: &'a Value,
    ) -> CaseMaterial<'a> {
        CaseMaterial {
            ordinal,
            input,
            tags,
            expected,
            attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta,
        }
    }

    fn metric<'a>(grader: &'a Value, label: &'a str) -> MetricMaterial<'a> {
        MetricMaterial {
            key: "pass",
            role: "quality",
            kind: "binary",
            better: "higher",
            label,
            unit: None,
            grader_kind: "programmatic",
            grader,
            rubric: None,
            ordinal: 0,
        }
    }

    fn digest_of(
        cases: &[CaseMaterial<'_>],
        metrics: &[MetricMaterial<'_>],
        prices: &Value,
        harness: &[String],
    ) -> String {
        instrument_digest(&Instrument {
            cases,
            metrics,
            prices,
            harness_paths: harness,
        })
    }

    #[test]
    fn eval_transitions_are_exactly_the_documented_table() {
        let mut accepted = 0_usize;
        for from in EvalStatus::ALL {
            for to in EvalStatus::ALL {
                let allowed = LEGAL.contains(&(from, to));
                assert_eq!(
                    from.may_transition_to(to),
                    allowed,
                    "may_transition_to disagreed for {from} -> {to}"
                );
                let outcome = from.transition(to);
                if allowed {
                    assert_eq!(outcome, Ok(to), "{from} -> {to} must be allowed");
                    accepted += 1;
                } else {
                    assert_eq!(
                        outcome,
                        Err(EvalError::IllegalTransition { from, to }),
                        "{from} -> {to} must be refused"
                    );
                }
            }
        }
        assert_eq!(accepted, LEGAL.len(), "exactly the documented edges");
        assert_eq!(EvalStatus::ALL.len(), 4);
        assert_eq!(EvalStatus::TERMINAL, [EvalStatus::Retired]);
    }

    #[test]
    fn eval_retired_refuses_every_target_including_itself() {
        for to in EvalStatus::ALL {
            assert!(EvalStatus::Retired.is_terminal());
            assert!(!EvalStatus::Retired.may_transition_to(to));
            assert_eq!(
                EvalStatus::Retired.transition(to),
                Err(EvalError::IllegalTransition {
                    from: EvalStatus::Retired,
                    to,
                }),
                "retired -> {to} must be refused"
            );
        }
        for status in [EvalStatus::Draft, EvalStatus::Running, EvalStatus::Climbing] {
            assert!(!status.is_terminal());
        }
    }

    #[test]
    fn eval_status_vocabulary_round_trips() {
        for status in EvalStatus::ALL {
            assert_eq!(status.as_str().parse::<EvalStatus>(), Ok(status));
            assert_eq!(status.to_string(), status.as_str());
        }
        assert_eq!(
            "ready".parse::<EvalStatus>(),
            Err(EvalError::Vocabulary {
                what: "eval_status",
                value: "ready".to_owned(),
            }),
            "there is no ready status"
        );
    }

    #[test]
    fn eval_subject_kind_vocabulary_and_runnability() {
        assert_eq!(SubjectKind::ALL.len(), 5);
        for kind in SubjectKind::ALL {
            assert_eq!(kind.as_str().parse::<SubjectKind>(), Ok(kind));
            assert_eq!(
                kind.is_runnable(),
                kind != SubjectKind::AgentRun,
                "{kind} runnability"
            );
        }
        assert_eq!(SubjectKind::AgentRun.as_str(), "agent_run");
        assert_eq!(
            "agent-run".parse::<SubjectKind>(),
            Err(EvalError::Vocabulary {
                what: "subject_kind",
                value: "agent-run".to_owned(),
            })
        );
    }

    #[test]
    fn eval_variant_grammar_matches_the_database_check() {
        assert_eq!(Variant::parse("baseline"), Ok(Variant::Baseline));
        assert_eq!(Variant::parse("v0"), Ok(Variant::Numbered(0)));
        assert_eq!(Variant::parse("v007"), Ok(Variant::Numbered(7)));
        assert_eq!(Variant::parse("v1"), Ok(Variant::Numbered(1)));
        assert_eq!(
            Variant::parse("v4294967295"),
            Ok(Variant::Numbered(u32::MAX))
        );

        for refused in [
            "",
            "v",
            "V1",
            "baseline1",
            "1",
            "v 1",
            "v-1",
            "v+1",
            "vv1",
            "v1x",
            "v4294967296",
            "v99999999999999999999999",
        ] {
            assert_eq!(
                Variant::parse(refused),
                Err(EvalError::Vocabulary {
                    what: "variant",
                    value: refused.to_owned(),
                }),
                "{refused:?} must be refused"
            );
            assert_eq!(refused.parse::<Variant>(), Variant::parse(refused));
        }
    }

    #[test]
    fn eval_variant_displays_its_column_spelling_and_orders_baseline_first() {
        assert_eq!(Variant::Baseline.as_str(), "baseline");
        assert_eq!(Variant::Baseline.to_string(), "baseline");
        assert_eq!(Variant::Numbered(7).as_str(), "v7");
        assert_eq!(Variant::Numbered(7).to_string(), "v7");
        assert_eq!("v007".parse::<Variant>(), Ok(Variant::Numbered(7)));

        let mut variants = vec![
            Variant::Numbered(2),
            Variant::Baseline,
            Variant::Numbered(0),
            Variant::Numbered(10),
        ];
        variants.sort();
        assert_eq!(
            variants,
            vec![
                Variant::Baseline,
                Variant::Numbered(0),
                Variant::Numbered(2),
                Variant::Numbered(10),
            ]
        );
        assert!(Variant::Baseline < Variant::Numbered(0));
    }

    #[test]
    fn eval_gate_vocabulary_and_the_two_that_gate_a_run() {
        assert_eq!(EvalGate::ALL.len(), 4);
        for gate in EvalGate::ALL {
            assert_eq!(gate.as_str().parse::<EvalGate>(), Ok(gate));
            assert_eq!(
                gate.gates_run(),
                matches!(gate, EvalGate::Inputs | EvalGate::Grading),
                "{gate} gating"
            );
        }
        assert_eq!(EvalGate::Inputs.as_str(), "eval_inputs");
        assert_eq!(EvalGate::Grading.as_str(), "eval_grading");
        assert_eq!(EvalGate::Cost.as_str(), "eval_cost");
        assert_eq!(EvalGate::Harness.as_str(), "eval_harness");
        assert_eq!(
            serde_json::to_value(EvalGate::Inputs).expect("gate serialises"),
            json!("eval_inputs")
        );
        assert_eq!(
            serde_json::from_value::<EvalGate>(json!("eval_harness")).expect("gate parses"),
            EvalGate::Harness
        );
        assert!("inputs".parse::<EvalGate>().is_err());
    }

    #[test]
    fn eval_missing_gate_wire_strings() {
        assert_eq!(MissingGate::InputsSignoff.as_str(), "eval_inputs");
        assert_eq!(MissingGate::GradingSignoff.as_str(), "eval_grading");
        assert_eq!(MissingGate::NoMetrics.as_str(), "metrics");
        assert_eq!(MissingGate::SplitMissing.as_str(), "split_missing");
        assert_eq!(MissingGate::SplitIncomplete.as_str(), "split_incomplete");
        assert_eq!(MissingGate::SplitStale.as_str(), "split_stale");
    }

    #[test]
    fn eval_readiness_names_every_missing_gate_in_order() {
        let readiness = Readiness::evaluate(ReadinessFacts {
            inputs_approved: false,
            grading_approved: false,
            metric_count: 0,
            split_locked: false,
            unsplit_cases: 3,
            split_matches: false,
        });
        assert!(!readiness.is_ready());
        assert_eq!(
            readiness.missing(),
            &[
                MissingGate::InputsSignoff,
                MissingGate::GradingSignoff,
                MissingGate::NoMetrics,
                MissingGate::SplitMissing,
                MissingGate::SplitIncomplete,
            ],
            "a locked split is a precondition for staleness, so SplitStale cannot also fire"
        );
    }

    #[test]
    fn eval_readiness_stale_split_is_the_last_gate() {
        let readiness = Readiness::evaluate(ReadinessFacts {
            inputs_approved: false,
            grading_approved: false,
            metric_count: 0,
            split_locked: true,
            unsplit_cases: 0,
            split_matches: false,
        });
        assert_eq!(
            readiness.missing(),
            &[
                MissingGate::InputsSignoff,
                MissingGate::GradingSignoff,
                MissingGate::NoMetrics,
                MissingGate::SplitStale,
            ]
        );

        let incomplete = Readiness::evaluate(ReadinessFacts {
            unsplit_cases: 3,
            ..ReadinessFacts::default()
        });
        let mut all: Vec<MissingGate> = incomplete.missing().to_vec();
        all.extend_from_slice(readiness.missing());
        let mut distinct: Vec<&str> = all.iter().map(|gate| gate.as_str()).collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            6,
            "all six gates are reachable: {distinct:?}"
        );
    }

    #[test]
    fn eval_readiness_is_empty_when_every_fact_is_satisfied() {
        let readiness = Readiness::evaluate(ReadinessFacts {
            inputs_approved: true,
            grading_approved: true,
            metric_count: 2,
            split_locked: true,
            unsplit_cases: 0,
            split_matches: true,
        });
        assert!(readiness.is_ready());
        assert_eq!(readiness.missing(), &[]);
    }

    #[test]
    fn eval_readiness_names_each_single_failure() {
        let satisfied = ReadinessFacts {
            inputs_approved: true,
            grading_approved: true,
            metric_count: 1,
            split_locked: true,
            unsplit_cases: 0,
            split_matches: true,
        };
        let single = |facts: ReadinessFacts, gate: MissingGate| {
            assert_eq!(
                Readiness::evaluate(facts).missing(),
                &[gate],
                "{gate} must be the only missing gate"
            );
        };
        single(
            ReadinessFacts {
                inputs_approved: false,
                ..satisfied
            },
            MissingGate::InputsSignoff,
        );
        single(
            ReadinessFacts {
                grading_approved: false,
                ..satisfied
            },
            MissingGate::GradingSignoff,
        );
        single(
            ReadinessFacts {
                metric_count: 0,
                ..satisfied
            },
            MissingGate::NoMetrics,
        );
        single(
            ReadinessFacts {
                split_locked: false,
                ..satisfied
            },
            MissingGate::SplitMissing,
        );
        single(
            ReadinessFacts {
                unsplit_cases: 4,
                ..satisfied
            },
            MissingGate::SplitIncomplete,
        );
        single(
            ReadinessFacts {
                split_matches: false,
                ..satisfied
            },
            MissingGate::SplitStale,
        );
    }

    #[test]
    fn eval_retry_policy_defaults_and_backoff() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.base_ms, 500);
        assert_eq!(policy.cap_ms, 30_000);

        assert_eq!(policy.backoff(0), Duration::from_millis(500));
        assert_eq!(policy.backoff(1), Duration::from_millis(500));
        assert_eq!(policy.backoff(2), Duration::from_millis(1_000));
        assert_eq!(policy.backoff(3), Duration::from_millis(2_000));
        assert_eq!(policy.backoff(4), Duration::from_millis(4_000));
        assert_eq!(policy.backoff(5), Duration::from_millis(8_000));
        // 500 * 2^6 = 32_000, above the 30 s cap.
        assert_eq!(policy.backoff(7), Duration::from_millis(30_000));
        // Total for absurd inputs: saturates, never panics or overflows.
        assert_eq!(policy.backoff(u32::MAX), Duration::from_millis(30_000));

        let tight = RetryPolicy {
            max_attempts: 3,
            base_ms: 1_000,
            cap_ms: 100,
        };
        assert_eq!(tight.backoff(1), Duration::from_millis(100));
    }

    #[test]
    fn eval_retry_backoff_jitter_is_bounded_and_deterministic() {
        let policy = RetryPolicy::default();
        for attempt in 1..=10 {
            let full = policy.backoff(attempt);
            for entropy in [0_u64, 1, 2, 3, 4, 1_000, u64::MAX] {
                let jittered = policy.backoff_jittered(attempt, entropy);
                assert_eq!(
                    jittered,
                    policy.backoff_jittered(attempt, entropy),
                    "pure function: same entropy, same delay"
                );
                assert!(
                    jittered >= full / 2 && jittered <= full,
                    "attempt {attempt}, entropy {entropy}: {jittered:?} outside [{:?}, {full:?}]",
                    full / 2
                );
            }
            assert_eq!(policy.backoff_jittered(attempt, 0), full / 2);
        }
        assert_eq!(policy.backoff_jittered(1, 0), Duration::from_millis(250));
        // A zero backoff jitters to zero rather than dividing by zero.
        let zeroed = RetryPolicy {
            max_attempts: 1,
            base_ms: 0,
            cap_ms: 0,
        };
        assert_eq!(zeroed.backoff_jittered(1, 7), Duration::ZERO);
    }

    #[test]
    fn eval_instrument_digest_is_insensitive_to_row_order() {
        let tags = vec!["suite".to_owned()];
        let expected_a = json!("alpha");
        let expected_b = json!("beta");
        let attachments = json!([]);
        let meta = json!({});
        let grader = json!({"kind": "exact"});
        let prices = json!({"input": 3, "output": 15});
        let harness = vec!["harness/base.py".to_owned()];

        let first = case(0, "alpha", &tags, Some(&expected_a), &attachments, &meta);
        let second = case(1, "beta", &tags, Some(&expected_b), &attachments, &meta);
        let metrics = [
            metric(&grader, "pass"),
            MetricMaterial {
                key: "latency",
                ordinal: 1,
                ..metric(&grader, "lat")
            },
        ];
        let reversed_metrics = [metrics[1], metrics[0]];

        let forward = digest_of(&[first, second], &metrics, &prices, &harness);
        let reverse = digest_of(&[second, first], &metrics, &prices, &harness);
        assert_eq!(forward, reverse, "row order must not reach the digest");
        assert_eq!(
            forward,
            digest_of(&[first, second], &reversed_metrics, &prices, &harness),
            "metric row order must not reach the digest"
        );
        assert_eq!(forward.len(), 64);
        assert!(
            forward
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "lowercase hex: {forward}"
        );
        assert_ne!(forward, digest_of(&[first], &metrics, &prices, &harness));
    }

    #[test]
    fn eval_instrument_digest_changes_with_every_field() {
        let tags = vec!["first".to_owned(), "second".to_owned()];
        let reordered_tags = vec!["second".to_owned(), "first".to_owned()];
        let expected_a = json!("alpha");
        let expected_b = json!("beta");
        let attachments = json!([]);
        let other_attachments = json!([{"path": "data.csv"}]);
        let meta = json!({});
        let other_meta = json!({"note": "x"});
        let grader = json!({"kind": "exact"});
        let other_grader = json!({"kind": "fuzzy"});
        let prices = json!({"input": 3, "output": 15});
        let other_prices = json!({"input": 4, "output": 15});
        let harness = vec!["harness/base.py".to_owned()];
        let other_harness = vec!["harness/base.py".to_owned(), "harness/extra.py".to_owned()];

        let first = case(0, "alpha", &tags, Some(&expected_a), &attachments, &meta);
        let second = case(1, "beta", &tags, Some(&expected_b), &attachments, &meta);
        let metrics = [metric(&grader, "pass")];
        let base = digest_of(&[first, second], &metrics, &prices, &harness);

        let mutations: Vec<(&str, String)> = vec![
            (
                "input",
                digest_of(
                    &[
                        case(0, "gamma", &tags, Some(&expected_a), &attachments, &meta),
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "expected",
                digest_of(
                    &[
                        case(0, "alpha", &tags, Some(&expected_b), &attachments, &meta),
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "expected absent",
                digest_of(
                    &[case(0, "alpha", &tags, None, &attachments, &meta), second],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "tags order",
                digest_of(
                    &[
                        case(
                            0,
                            "alpha",
                            &reordered_tags,
                            Some(&expected_a),
                            &attachments,
                            &meta,
                        ),
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "attachments",
                digest_of(
                    &[
                        case(
                            0,
                            "alpha",
                            &tags,
                            Some(&expected_a),
                            &other_attachments,
                            &meta,
                        ),
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "source",
                digest_of(
                    &[
                        CaseMaterial {
                            source: "production",
                            ..first
                        },
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "gold_provenance",
                digest_of(
                    &[
                        CaseMaterial {
                            gold_provenance: "human_verified",
                            ..first
                        },
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "gold_model",
                digest_of(
                    &[
                        CaseMaterial {
                            gold_provenance: "model",
                            gold_model: Some("mini"),
                            ..first
                        },
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "meta",
                digest_of(
                    &[
                        case(
                            0,
                            "alpha",
                            &tags,
                            Some(&expected_a),
                            &attachments,
                            &other_meta,
                        ),
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "case ordinal",
                digest_of(
                    &[
                        case(5, "alpha", &tags, Some(&expected_a), &attachments, &meta),
                        second,
                    ],
                    &metrics,
                    &prices,
                    &harness,
                ),
            ),
            (
                "metric label",
                digest_of(
                    &[first, second],
                    &[metric(&grader, "passing")],
                    &prices,
                    &harness,
                ),
            ),
            (
                "metric key",
                digest_of(
                    &[first, second],
                    &[MetricMaterial {
                        key: "score",
                        ..metric(&grader, "pass")
                    }],
                    &prices,
                    &harness,
                ),
            ),
            (
                "metric grader",
                digest_of(
                    &[first, second],
                    &[metric(&other_grader, "pass")],
                    &prices,
                    &harness,
                ),
            ),
            (
                "metric rubric",
                digest_of(
                    &[first, second],
                    &[MetricMaterial {
                        rubric: Some("score it 0..1"),
                        ..metric(&grader, "pass")
                    }],
                    &prices,
                    &harness,
                ),
            ),
            (
                "metric unit",
                digest_of(
                    &[first, second],
                    &[MetricMaterial {
                        unit: Some("s"),
                        ..metric(&grader, "pass")
                    }],
                    &prices,
                    &harness,
                ),
            ),
            (
                "metric ordinal",
                digest_of(
                    &[first, second],
                    &[MetricMaterial {
                        ordinal: 3,
                        ..metric(&grader, "pass")
                    }],
                    &prices,
                    &harness,
                ),
            ),
            (
                "prices",
                digest_of(&[first, second], &metrics, &other_prices, &harness),
            ),
            (
                "harness_paths",
                digest_of(&[first, second], &metrics, &prices, &other_harness),
            ),
        ];

        for (field, digest) in mutations {
            assert_ne!(base, digest, "changing {field} must change the digest");
        }
    }
}
