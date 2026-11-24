#![forbid(unsafe_code)]
//! T2 — the decision ladder's pure vocabulary (SPEC_V2 §7.1–§7.5; issue #73).
//!
//! Everything here is a refusal or an arithmetic comparison. The store writes what these
//! functions agreed to; the database's CHECKs are the second lock, never the first
//! (CONTRACT §2).
//!
//! The slice's point is one shape: an L0 rule outcome, an L1 laya choice and an L3 human
//! approval each project the same columns, so the three are comparable without a join and
//! only their values differ (§7.1). L2 generation is **not** a rung here — a generation call
//! is an `inference_call` bound to a draft artefact, and the draft has no path to a card
//! transition (`GeneratedDraft`, §7.4).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The three rungs a decision row records (§7.1).
///
/// L2 generation is absent on purpose: it is an `inference_call` plus a draft artefact, never
/// a decision row. A `decision` row with `rung = 'generation'` cannot be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    /// L0 — deterministic Rust over typed facts.
    Rule,
    /// L1 — laya's calibrated choice/score/noul over a finite option set.
    Laya,
    /// L3 — a human approval, linked to its `approval` row.
    Human,
}

impl Rung {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Laya => "laya",
            Self::Human => "human",
        }
    }

    /// Parse a stored or posted spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rule" => Some(Self::Rule),
            "laya" => Some(Self::Laya),
            "human" => Some(Self::Human),
            _ => None,
        }
    }

    /// Whether the rung must name the policy it enforced and the input hash it consumed.
    ///
    /// A human decision is recorded from the approval's own evidence, so the policy and the
    /// hash are optional on that rung and mandatory on the other two (§7.2's "a decision is
    /// reproducible from the row").
    pub fn requires_reproducibility(self) -> bool {
        !matches!(self, Self::Human)
    }
}

/// The policy's fail behaviour when laya is unavailable (§7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailPolicy {
    /// A safety gate: unavailable is a refusal.
    Closed,
    /// A routing/quality gate: unavailable falls to the configured action (a human or a note).
    Open,
}

impl FailPolicy {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
        }
    }

    /// Parse a stored or posted spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "closed" => Some(Self::Closed),
            "open" => Some(Self::Open),
            _ => None,
        }
    }
}

/// What a below-threshold or unavailable decision does (§7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailAction {
    /// Escalate to a person — an approval row.
    Handover,
    /// Refuse the move.
    Block,
    /// Proceed, recording the note that the answer was below threshold.
    Annotate,
}

impl FailAction {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Handover => "handover",
            Self::Block => "block",
            Self::Annotate => "annotate",
        }
    }

    /// Parse a stored or posted spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "handover" => Some(Self::Handover),
            "block" => Some(Self::Block),
            "annotate" => Some(Self::Annotate),
            _ => None,
        }
    }
}

/// What the ladder concluded. The stored `decision.outcome` vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The answer was above threshold (or a below-threshold answer the policy annotates).
    Accepted,
    /// The policy refuses the move; nothing proceeds.
    Refused,
    /// The move needs a person; the caller opens an approval before applying anything.
    Escalated,
}

impl Outcome {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Refused => "refused",
            Self::Escalated => "escalated",
        }
    }

    /// Parse a stored spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "refused" => Some(Self::Refused),
            "escalated" => Some(Self::Escalated),
            _ => None,
        }
    }
}

/// The smallest and largest authored option set laya accepts (§7.2: a choice is 2–255
/// labelled options).
pub const MIN_OPTIONS: usize = 2;
/// The largest authored option set laya accepts.
pub const MAX_OPTIONS: usize = 255;

/// Where an authored option set came from.
///
/// The platform library is the base set; a tenant's own set for the same case shadows it
/// (the agent platform's library-first, tenant-key-shadow fold). Both origins live in the
/// tenant-scoped `decision_option_set` because every conveyor table is RLS-isolated; the
/// library rows are written by the platform seed, never by a tenant action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionOrigin {
    /// The platform's base option set.
    Library,
    /// The tenant's authored set; it shadows the library for the same `(kind, gate)`.
    Tenant,
}

impl OptionOrigin {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Tenant => "tenant",
        }
    }

    /// Parse a stored or posted spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "library" => Some(Self::Library),
            "tenant" => Some(Self::Tenant),
            _ => None,
        }
    }
}

/// One effective option as the set hash pins it.
#[derive(Clone, Copy, Debug)]
pub struct OptionPin<'a> {
    /// The option key the answer must equal.
    pub key: &'a str,
    /// The effective human label.
    pub label: &'a str,
    /// The effective description the laya wire reads.
    pub criteria: &'a str,
    /// The effective authored order.
    pub position: i32,
    /// The refinement's `base_hash`, or the base entry's own hash when unrefined.
    pub base_hash: &'a str,
}

/// The canonical hash of one library option's base content.
///
/// A refinement stores this value when it is published; the resolver recomputes it from the
/// entry it read and refuses a mismatch. `kind` and `gate` are part of the basis so a
/// refinement cannot be replayed across cases.
pub fn option_base_hash(
    kind: &str,
    gate: Option<&str>,
    key: &str,
    label: &str,
    criteria: &str,
    position: i32,
) -> String {
    crate::compare::content_hash(&serde_json::json!({
        "kind": kind,
        "gate": gate,
        "key": key,
        "label": label,
        "criteria": criteria,
        "position": position,
    }))
}

/// The canonical hash of the effective option set a decision was constrained to.
///
/// The decision row stores this value next to the set id and version; a later edit of the
/// set cannot rewrite what the decision actually offered.
pub fn option_set_hash(kind: &str, gate: Option<&str>, options: &[OptionPin<'_>]) -> String {
    let options: Vec<serde_json::Value> = options
        .iter()
        .map(|option| {
            serde_json::json!({
                "key": option.key,
                "label": option.label,
                "criteria": option.criteria,
                "position": option.position,
                "base_hash": option.base_hash,
            })
        })
        .collect();
    crate::compare::content_hash(&serde_json::json!({
        "kind": kind,
        "gate": gate,
        "options": options,
    }))
}

/// The canonical hash of a policy's base content.
///
/// A tenant refinement stores this when it is published; `effective_policy_tx` recomputes it
/// and refuses a refinement whose base changed. The basis deliberately excludes `version`:
/// republishing identical content does not invalidate a refinement.
pub fn policy_base_hash(
    kind: &str,
    gate: Option<&str>,
    threshold: Option<f64>,
    fail_policy: &str,
    action: &str,
    meta: &serde_json::Value,
) -> String {
    crate::compare::content_hash(&serde_json::json!({
        "kind": kind,
        "gate": gate,
        "threshold": threshold,
        "fail_policy": fail_policy,
        "action": action,
        "meta": meta,
    }))
}

/// The version pins a decision row carries (§7.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pins {
    /// The `decision_policy.version` the evaluator read.
    pub policy_version: Option<i64>,
    /// The `decision_policy_refinement.version` it applied; `None` when no refinement applied.
    pub refinement_version: Option<i64>,
    /// The `laya_inference_config.version` it used; `None` for a rule or a human rung.
    pub laya_config_version: Option<i64>,
    /// The `action_invocation.id` of the verified `laya.classify` call behind a laya answer.
    ///
    /// A laya decision is not reproducible from its own numbers: the answer was produced by an
    /// external call, and the row must name the recorded invocation that made it (§7.2). A
    /// rule or human rung names none.
    pub laya_invocation_id: Option<uuid::Uuid>,
}

/// The reasons the ladder refuses.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum LadderError {
    /// A rung spelling the domain does not know.
    #[error("{value:?} is not a decision rung")]
    UnknownRung {
        /// The posted value.
        value: String,
    },
    /// A fail-policy spelling the domain does not know.
    #[error("{value:?} is not a fail policy (closed|open)")]
    UnknownFailPolicy {
        /// The stored or posted value.
        value: String,
    },
    /// A fail-action spelling the domain does not know.
    #[error("{value:?} is not a fail action (handover|block|annotate)")]
    UnknownFailAction {
        /// The stored or posted value.
        value: String,
    },
    /// A confidence or threshold outside 0–1.
    #[error("{field} must be between 0 and 1")]
    OutOfRange {
        /// The offending field.
        field: &'static str,
    },
    /// A confidence was required and absent, or the reverse.
    #[error("{field} is required for the {rung} rung")]
    FieldMissing {
        /// The absent field.
        field: &'static str,
        /// The rung's spelling.
        rung: &'static str,
    },
    /// A decision answer is blank.
    #[error("a decision must carry its answer")]
    AnswerMissing,
    /// The input hash is not a sha256 hex digest.
    #[error("input_hash must be 64 lowercase hex characters")]
    InputHashShape,
    /// A rung's reproducibility pins are incomplete.
    #[error("a {rung} decision must pin: {missing:?}")]
    PinsIncomplete {
        /// The rung's spelling.
        rung: &'static str,
        /// The absent pin names, in the §7.2 order.
        missing: Vec<&'static str>,
    },
    /// A human decision has no approval to cite.
    #[error("a human decision must cite its approval")]
    ApprovalMissing,
    /// A rule or laya decision has no policy to enforce.
    #[error("no decision policy covers this case")]
    PolicyMissing,
    /// A policy already exists for this case.
    #[error("a policy already exists for {kind}/{gate:?}")]
    PolicyExists {
        /// The case kind.
        kind: String,
        /// The gate, when the policy is gate-specific.
        gate: Option<String>,
    },
    /// An authored option set is outside laya's 2–255 range.
    #[error("a choice question needs between 2 and 255 options, got {count}")]
    OptionSetSize {
        /// The number the caller supplied or the effective set holds.
        count: usize,
    },
    /// An option key is repeated inside one set.
    #[error("the option key {key:?} is repeated")]
    DuplicateOption {
        /// The repeated key.
        key: String,
    },
    /// Two options claim one position inside a set.
    #[error("the option position {position} is repeated")]
    DuplicatePosition {
        /// The repeated position.
        position: i32,
    },
    /// An option position is below 1.
    #[error("the option position must be at least 1")]
    InvalidPosition,
    /// An option origin spelling the domain does not know.
    #[error("{value:?} is not an option origin (library|tenant)")]
    UnknownOptionOrigin {
        /// The posted value.
        value: String,
    },
    /// A decision's answer is not one of the authored option keys.
    #[error("the answer {answer:?} is not a key of the authored option set")]
    AnswerNotInSet {
        /// The answer the rung produced.
        answer: String,
    },
    /// Hiding an option would leave fewer than two visible choices.
    #[error("a question keeps at least two visible options; {visible} would remain")]
    TooFewOptions {
        /// The visible options that would remain.
        visible: usize,
    },
}

/// Validate a confidence or a threshold.
pub fn in_unit_interval(field: &'static str, value: f64) -> Result<f64, LadderError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(LadderError::OutOfRange { field });
    }
    Ok(value)
}

/// The outcome of an answer at or above the threshold.
pub const fn at_or_above_threshold() -> Outcome {
    Outcome::Accepted
}

/// The outcome of an answer below the threshold, per the policy's action (§7.2).
///
/// Below threshold is an **outcome**, not an error: `handover` escalates, `block` refuses,
/// `annotate` proceeds with the note the caller records.
pub fn below_threshold(action: FailAction) -> Outcome {
    match action {
        FailAction::Handover => Outcome::Escalated,
        FailAction::Block => Outcome::Refused,
        FailAction::Annotate => Outcome::Accepted,
    }
}

/// The outcome when laya is unavailable, per the policy's fail policy (§7.2).
///
/// `Unavailable` is `gate_unavailable` and follows the fail policy; it is never silently
/// treated as a pass. A closed safety gate refuses; an open routing gate applies its action.
pub fn unavailable(fail_policy: FailPolicy, action: FailAction) -> Outcome {
    match fail_policy {
        FailPolicy::Closed => Outcome::Refused,
        FailPolicy::Open => below_threshold(action),
    }
}

/// Evaluate one answer against a threshold.
///
/// Returns `Outcome::Accepted` at or above the threshold and the action's outcome below it.
/// The caller that gets `Escalated` opens an approval; the caller that gets `Refused` applies
/// nothing; `Accepted` may proceed.
pub fn evaluate(
    confidence: f64,
    threshold: f64,
    action: FailAction,
) -> Result<Outcome, LadderError> {
    let confidence = in_unit_interval("confidence", confidence)?;
    let threshold = in_unit_interval("threshold", threshold)?;
    if confidence + f64::EPSILON >= threshold {
        Ok(at_or_above_threshold())
    } else {
        Ok(below_threshold(action))
    }
}

/// The fields a decision row must carry for its rung, in the §7.2 order.
///
/// * A rule or laya decision pins the policy version it enforced and a sha256 of the inputs it
///   consumed; a laya decision pins the laya config and the recorded `laya.classify`
///   invocation that produced its answer, and a refined one pins the refinement.
/// * A human decision cites the approval it records.
pub fn missing_pins(rung: Rung, pins: &Pins, approval_id: Option<uuid::Uuid>) -> Vec<&'static str> {
    missing_pins_for_kind("", rung, pins, approval_id)
}

/// The decision kinds the step executor records for its judge verdicts (v1-p101-1).
///
/// A step judge's verdict is one `decision` row — the same store every ladder decision uses —
/// but it is not a case the ladder's policy resolved: the SOP's inspection decided, and the row
/// pins the SOP version and the `laya.judge` invocation that answered, not a `decision_policy`
/// and its laya config. The per-rung pin contract therefore does not apply to this prefix; every
/// other kind keeps it.
pub const STEP_JUDGE_KIND_PREFIX: &str = "process.step.";

/// [`missing_pins`], with the decision kind in hand so the step judge's own store can be exempt.
///
/// v1-p101-1: a `process.step.` decision carries no ladder policy pins — there is no policy row
/// behind a step judge — so the usual per-rung fields are not owed. The exemption is scoped to
/// that prefix alone; a ladder case's pins are unchanged.
pub fn missing_pins_for_kind(
    kind: &str,
    rung: Rung,
    pins: &Pins,
    approval_id: Option<uuid::Uuid>,
) -> Vec<&'static str> {
    if kind.starts_with(STEP_JUDGE_KIND_PREFIX) {
        return Vec::new();
    }
    let mut missing = Vec::new();
    match rung {
        Rung::Rule => {
            if pins.policy_version.is_none() {
                missing.push("policy_version");
            }
        }
        Rung::Laya => {
            if pins.policy_version.is_none() {
                missing.push("policy_version");
            }
            if pins.laya_config_version.is_none() {
                missing.push("laya_config_version");
            }
            if pins.laya_invocation_id.is_none() {
                missing.push("laya_invocation_id");
            }
        }
        Rung::Human => {
            if approval_id.is_none() {
                missing.push("approval_id");
            }
        }
    }
    missing
}

/// Refuse a decision whose rung's reproducibility contract is incomplete.
pub fn require_pins(
    rung: Rung,
    pins: &Pins,
    approval_id: Option<uuid::Uuid>,
) -> Result<(), LadderError> {
    let missing = missing_pins(rung, pins, approval_id);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(LadderError::PinsIncomplete {
            rung: rung.as_str(),
            missing,
        })
    }
}

/// Whether a string is the sha256 hex shape the `decision.input_hash` CHECK accepts.
pub fn is_input_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// L2 generation's result. **A draft artefact, never a transition.**
///
/// `GeneratedDraft` has no `From`/`Into` impl and no method that yields a card transition; a
/// generation result can only be stored (a `governed_asset`, a `document` or a code patch) and
/// applied by an action whose gate is an accepted L3 human decision of the `draft.apply` gate
/// (§7.4). The `compile_fail` doctest below is
/// the type-level proof, and `apply_card_transition_tx` takes no such type.
///
/// ```compile_fail
/// use conveyor_domain::ladder::GeneratedDraft;
/// use conveyor_domain::Move;
/// let draft = GeneratedDraft::new("generated text");
/// // A generated draft cannot become a card transition.
/// let _: Move = draft.into();
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedDraft {
    /// The generated body; stored as a draft artefact, never applied directly.
    pub body: String,
    /// The model call that produced it.
    pub inference_call_id: Option<uuid::Uuid>,
    /// The prompt identity that produced it.
    pub prompt_sha256: Option<String>,
}

impl GeneratedDraft {
    /// Wrap a generation result. This constructor is the only way in: no transition function
    /// accepts the type, so wrapping it is safe and applying it requires an action.
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            inference_call_id: None,
            prompt_sha256: None,
        }
    }
}
