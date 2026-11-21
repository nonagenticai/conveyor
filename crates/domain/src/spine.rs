#![forbid(unsafe_code)]
//! T1 — the plan spine's pure vocabulary (SPEC_V2 §5.1, §5.3–§5.5, §8.2, §8.3).
//!
//! Everything here is arithmetic or a refusal. The store writes what these functions agreed
//! to; the database's CHECKs are the second lock, never the first (CONTRACT §2).
//!
//! Two rules are worth naming because they are the slice's whole point:
//!
//! * **WSJF is computed on read** from four typed, non-negative integers and `job_size > 0`.
//!   There is no score column; a stored score would become a stale opinion (D-T1-4).
//! * **A benefit profile without a measured baseline and target, a measurement method, an owner,
//!   its capabilities and its operational area is a wish.** [`benefit_profile_missing`] returns
//!   the exact fields that are absent, so the refusal can name them (D-T1-9).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

/// The WBS node kinds (§10.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WbsKind {
    /// A project phase (the ADM phases use this kind).
    Phase,
    /// A deliverable.
    Deliverable,
    /// A work package (the card-bearing unit).
    WorkPackage,
    /// A milestone.
    Milestone,
}

impl WbsKind {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Phase => "phase",
            Self::Deliverable => "deliverable",
            Self::WorkPackage => "work_package",
            Self::Milestone => "milestone",
        }
    }

    /// Parse a stored or posted spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "phase" => Some(Self::Phase),
            "deliverable" => Some(Self::Deliverable),
            "work_package" => Some(Self::WorkPackage),
            "milestone" => Some(Self::Milestone),
            _ => None,
        }
    }
}

/// The four WSJF inputs (§8.2). All non-negative; `job_size` must be at least 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsjfInputs {
    /// `business_value`.
    pub business_value: i32,
    /// `time_criticality`.
    pub time_criticality: i32,
    /// `risk_reduction`.
    pub risk_reduction: i32,
    /// `job_size`; divides the sum.
    pub job_size: i32,
}

impl Default for WsjfInputs {
    fn default() -> Self {
        Self {
            business_value: 0,
            time_criticality: 0,
            risk_reduction: 0,
            job_size: 1,
        }
    }
}

/// The reasons the plan spine refuses.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PlanError {
    /// `job_size` is zero or negative; the score divides by it.
    #[error("job_size must be at least 1")]
    JobSizeNotPositive,
    /// One of the four WSJF inputs is negative.
    #[error("{field} must be non-negative")]
    WsjfInputNegative {
        /// The offending field.
        field: &'static str,
    },
    /// A change request is missing its impact block or its baseline reference.
    #[error("a change request needs an impact block and the baseline it changes")]
    ChangeImpactMissing,
    /// A benefit profile is missing one of its mandatory measurement fields.
    #[error("a benefit profile is missing: {missing:?}")]
    BenefitProfileIncomplete {
        /// The absent field names, in the §8.3 order.
        missing: Vec<&'static str>,
    },
    /// A benefits-map edge is missing one of its five levels.
    #[error("a benefits-map edge is missing: {missing:?}")]
    BenefitMapIncomplete {
        /// The absent level names.
        missing: Vec<&'static str>,
    },
    /// A benefits-map edge's `output_ref` names no WBS node of the edge's programme.
    ///
    /// The dependency-slip read matches `benefit_link.output_ref` to `wbs_node.id`; an edge
    /// whose output is a label can never participate, so it is refused at write time.
    #[error("output_ref {output_ref:?} names no wbs_node of this programme")]
    BenefitOutputUnresolved {
        /// The caller's unchecked string.
        output_ref: String,
    },
    /// A tranche review with no baselines, benefit status or risk evidence is refused.
    #[error("a tranche review needs baselines, benefit status and the risk register as evidence")]
    TrancheReviewEvidenceMissing,
    /// Only a `continue` decision authorises the next tranche.
    #[error("only a continue review authorises the next tranche")]
    TrancheReviewNotContinue,
    /// A closed or retired tranche cannot be reviewed.
    #[error("the tranche is {state}; a closed or retired tranche cannot be reviewed")]
    TrancheNotReviewable {
        /// The state the row carries.
        state: String,
    },
    /// A tranche close already has its one review.
    #[error("the tranche close already has a review")]
    TrancheAlreadyReviewed,
    /// A `continue` review needs the next tranche in the plan.
    #[error("the tranche plan has no next tranche to authorise")]
    NextTrancheMissing,
    /// The next tranche is not `planned`, so a `continue` cannot authorise it.
    #[error("the next tranche is {state}; only a planned tranche can be authorised")]
    NextTrancheNotPlanned {
        /// The state the next row carries.
        state: String,
    },
    /// A dependency edge closed a cycle.
    #[error("the dependency {from} -> {to} would close a cycle")]
    DependencyCycle {
        /// The edge's source project.
        from: Uuid,
        /// The edge's target project.
        to: Uuid,
    },
    /// A project cannot depend on itself.
    #[error("a project cannot depend on itself")]
    SelfDependency,
    /// A benefit has no measured baseline, so it cannot claim a realized improvement.
    #[error("benefit_baseline_missing")]
    BenefitBaselineMissing,
    /// An approved benefit profile's measurement fields change only through an approved change.
    #[error("benefit_change_required")]
    BenefitChangeRequired,
    /// The cited change is not an approved change request of the profile's project.
    #[error("change_not_approved")]
    ChangeNotApproved,
    /// A benefit profile in an open transition cannot be retired.
    #[error("benefit_in_transition")]
    BenefitInTransition,
    /// The benefit profile is already retired.
    #[error("benefit_retired")]
    BenefitRetired,
    /// A tranche under review cannot be retired.
    #[error("tranche_under_review")]
    TrancheUnderReview,
    /// The tranche is already retired.
    #[error("tranche_retired")]
    TrancheRetired,
    /// The cadence's kind does not match the operation that reads it.
    #[error("the cadence is {actual:?}; this operation needs {expected:?}")]
    CadenceWrongKind {
        /// The kind the operation needs.
        expected: &'static str,
        /// The kind the row carries.
        actual: String,
    },
    /// A cadence state move the state machine does not allow.
    #[error("the cadence is {from}; it cannot move to {to}")]
    CadenceIllegalTransition {
        /// The state the row carries.
        from: String,
        /// The state the caller asked for.
        to: String,
    },
    /// A cadence names no target board in its metadata.
    #[error("the cadence names no target board in meta.board_id")]
    CadenceBoardMissing,
    /// A cadence names no card type in its metadata, or the code is unknown.
    #[error("the cadence names no known card type in meta.card_type_code")]
    CadenceCardTypeMissing,
    /// A demo credit names a deliverable that is not accepted.
    #[error("a demo credit needs a completed (accepted) deliverable")]
    DemoDeliverableNotComplete,
    /// A demo credit names a deliverable with no evidence ref — no demo, no credit.
    #[error("gate_evidence_missing")]
    DemoEvidenceMissing,
    /// A retro closes while one of its findings has no lesson.
    #[error("the retro finding {0:?} has no lesson")]
    RetroFindingWithoutLesson(String),
    /// A project in a programme starts before the first tranche is authorised.
    #[error("tranche_not_authorised")]
    TrancheNotAuthorised,
    /// A tranche baseline changes only through a cited, approved change request.
    #[error("rebaseline_requires_change")]
    RebaselineRequiresChange,
    /// The programme has no benefits realization plan.
    #[error("realization_plan_missing")]
    RealizationPlanMissing,
    /// The realization plan does not name who continues measurement after closure.
    #[error("post_closure_owner_missing")]
    PostClosureOwnerMissing,
}

/// Compute the WSJF score from the four stored inputs.
///
/// `(business_value + time_criticality + risk_reduction) / job_size`. The result is a
/// `f64` because the spec's arithmetic is a ratio; it is never persisted.
pub fn wsjf(inputs: WsjfInputs) -> Result<f64, PlanError> {
    if inputs.business_value < 0 {
        return Err(PlanError::WsjfInputNegative {
            field: "business_value",
        });
    }
    if inputs.time_criticality < 0 {
        return Err(PlanError::WsjfInputNegative {
            field: "time_criticality",
        });
    }
    if inputs.risk_reduction < 0 {
        return Err(PlanError::WsjfInputNegative {
            field: "risk_reduction",
        });
    }
    if inputs.job_size <= 0 {
        return Err(PlanError::JobSizeNotPositive);
    }
    let sum = i64::from(inputs.business_value)
        + i64::from(inputs.time_criticality)
        + i64::from(inputs.risk_reduction);
    Ok(sum as f64 / f64::from(inputs.job_size))
}

/// Refuse a change request that has no impact block or no baseline.
pub fn validate_change(
    impact: Option<&Value>,
    baseline_ref: Option<Uuid>,
) -> Result<(), PlanError> {
    let has_impact = impact.is_some_and(|value| !value.is_null());
    if !has_impact || baseline_ref.is_none() {
        return Err(PlanError::ChangeImpactMissing);
    }
    Ok(())
}

/// A measurement is a real reading, not a placeholder: a number or a non-empty structured
/// document, never null/blank/an empty container and never the literal "not measured".
fn is_measurement(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => {
            !text.trim().is_empty() && !text.eq_ignore_ascii_case("not measured")
        }
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => true,
    }
}

/// The mandatory fields a benefit profile must carry, in the §8.3 order.
pub fn benefit_profile_missing(profile: &BenefitProfileFields) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !profile.baseline.as_ref().is_some_and(is_measurement) {
        missing.push("baseline");
    }
    if !profile.target.as_ref().is_some_and(is_measurement) {
        missing.push("target");
    }
    if profile.measurement_method.trim().is_empty() {
        missing.push("measurement_method");
    }
    if profile.benefit_owner_job_id.is_none() {
        missing.push("benefit_owner");
    }
    if !profile
        .capabilities
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        missing.push("capabilities");
    }
    if profile.operational_area.trim().is_empty() {
        missing.push("operational_area");
    }
    missing
}

/// The subset of a profile the completeness rule reads.
#[derive(Clone, Debug, Default)]
pub struct BenefitProfileFields {
    /// The measured baseline; `None` is a gap, never a default.
    pub baseline: Option<Value>,
    /// The target.
    pub target: Option<Value>,
    /// How the KPI is measured.
    pub measurement_method: String,
    /// The BCM job that owns the benefit.
    pub benefit_owner_job_id: Option<Uuid>,
    /// The capabilities the benefit depends on (§8.3); an empty array is a gap.
    pub capabilities: Value,
    /// The operational area that must change (§8.3); blank is a gap.
    pub operational_area: String,
}

/// The five levels a benefits-map edge must name (§8.3).
pub fn benefit_map_missing(edge: &BenefitMapFields) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if edge.output_ref.trim().is_empty() {
        missing.push("output");
    }
    if edge.capability_ref.trim().is_empty() {
        missing.push("capability");
    }
    if edge.operational_outcome.trim().is_empty() {
        missing.push("operational_outcome");
    }
    if edge.strategic_objective.trim().is_empty() {
        missing.push("strategic_objective");
    }
    missing
}

/// The four text levels of a benefits-map edge (the benefit is its FK).
#[derive(Clone, Debug, Default)]
pub struct BenefitMapFields {
    /// The project output.
    pub output_ref: String,
    /// The capability the output enables.
    pub capability_ref: String,
    /// The operational outcome the capability changes.
    pub operational_outcome: String,
    /// The strategic objective the benefit serves.
    pub strategic_objective: String,
}

/// Whether a tranche review carries the three evidence classes §8.3 requires.
pub fn tranche_review_has_evidence(evidence: &Value) -> bool {
    ["baselines", "benefit_status", "risk_register"]
        .iter()
        .all(|key| {
            evidence
                .get(*key)
                .is_some_and(|value| !value.is_null() && value != &Value::Array(Vec::new()))
        })
}
