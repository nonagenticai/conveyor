#![forbid(unsafe_code)]
//! T1 — the plan spine and the MSP rows (SPEC_V2 §5.1, §5.3–§5.5, §8.2, §8.3; issue #72).
//!
//! One declaration per action; the registry, the OpenAPI operation, the MCP tool and the
//! invocation route are projections of it (S04 §5.1). Nothing here speaks SQL directly — the
//! queries live [`conveyor_store::spine`] (CONTRACT §1).
//!
//! **The WSJF score never enters the database.** [`WbsOut::wsjf`] is computed on every read
//! from the four stored inputs (D-T1-4), so a score cannot go stale.
//!
//! **The one cross-cutting rule** is not here: the requester/executor exclusion lives in
//! `store::approval::record_decision_tx`, the one writer every decision path traverses
//! (D-T1-8). This module consumes it.
//!
//! Events: every mutation appends one `spine.changed` row in its own transaction, with the
//! payload `{entity, id, op}` and the topic `<entity>:<id>` — the same shape the portfolio
//! surface uses, so a subscriber watches one row or a family.

use chrono::{DateTime, Utc};
use conveyor_domain::spine::{PlanError, WsjfInputs, tranche_review_has_evidence, wsjf};
use conveyor_domain::{ApprovalError, DeciderKind, Decision, DecisionKind};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{etag_of, invalid, parent_error, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

pub use crate::board::views::IdIn;
pub use store::spine::{
    BaselineRecord, BenefitLinkRecord, BenefitPatch, BenefitRecord, CadencePatch, CadenceRecord,
    DemoCreditRecord, DependencyRecord, InFlightRecord, LessonRecord, NewBenefitLink,
    NewBenefitProfile, NewRegisterItem, NewStakeholderProfile, NewWbsNode, PlanStoreError,
    RegisterRecord, StakeholderPatch, StakeholderRecord, TranchePatch, TrancheRecord,
    TrancheReviewRecord, TransitionPatch, TransitionRecord, WbsPatch, WbsRecord,
};

/// The one event every spine mutation emits.
pub const SPINE_CHANGED: &str = "spine.changed";

/// Append one `spine.changed` row in the caller's transaction.
async fn emit_spine_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    entity: &'static str,
    id: Uuid,
    op: &'static str,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("{entity}:{id}"),
        event_type: SPINE_CHANGED.to_owned(),
        aggregate_type: entity.to_owned(),
        aggregate_id: id,
        payload: serde_json::json!({ "entity": entity, "id": id, "op": op }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, entity, op, action = ctx.action, "append the spine.changed event");
            ActionError::internal()
        })?;
    Ok(())
}

/// A plan store failure as the status the caller can act on.
fn spine_error(error: PlanStoreError, action: &str) -> ActionError {
    match error {
        PlanStoreError::NotFound { what } => ActionError::resource_not_found(what),
        PlanStoreError::Stale { what } => ActionError::new(
            412,
            "version_mismatch",
            format!("the {what} changed since it was read"),
        ),
        PlanStoreError::Parent(parent) => parent_error(parent, action),
        PlanStoreError::Illegal(PlanError::CadenceWrongKind { expected, actual }) => invalid(
            "cadence_id",
            "cadence_wrong_kind",
            format!("the cadence is {actual}; a credit needs a {expected} cadence"),
        ),
        PlanStoreError::Illegal(PlanError::CadenceIllegalTransition { from, to }) => {
            ActionError::new(
                409,
                "cadence_illegal_transition",
                format!("the cadence is {from}; it cannot move to {to}"),
            )
        }
        PlanStoreError::Illegal(PlanError::DemoDeliverableNotComplete) => invalid(
            "deliverable_id",
            "deliverable_not_accepted",
            "a credit needs an accepted deliverable",
        ),
        PlanStoreError::Illegal(PlanError::DemoEvidenceMissing) => invalid(
            "deliverable_id",
            "evidence_missing",
            "a credit needs a deliverable with at least one evidence ref",
        ),
        PlanStoreError::Illegal(PlanError::DemoEvidenceUntyped) => invalid(
            "deliverable_id",
            "evidence_untyped",
            "a credit needs typed evidence refs: a non-empty string or an object with a value",
        ),
        PlanStoreError::Illegal(PlanError::RetroFindingIncomplete(key)) => invalid(
            "cadence_id",
            "finding_incomplete",
            format!(
                "the retro finding {key:?} needs a statement, evidence and, when systemic, a kata block"
            ),
        ),
        PlanStoreError::Illegal(PlanError::DependencyCycle { from, to }) => ActionError::new(
            409,
            "dependency_cycle",
            format!("{from} -> {to} would close a dependency cycle"),
        ),
        PlanStoreError::Illegal(PlanError::SelfDependency) => invalid(
            "to_project",
            "self_dependency",
            "a project cannot depend on itself",
        ),
        PlanStoreError::Illegal(PlanError::ChangeImpactMissing) => invalid(
            "impact",
            "change_impact_missing",
            "a change needs an impact block and the baseline it changes",
        ),
        PlanStoreError::Illegal(PlanError::BenefitProfileIncomplete { missing }) => invalid(
            "baseline",
            "benefit_profile_incomplete",
            format!("a benefit profile is missing: {missing:?}"),
        ),
        PlanStoreError::Illegal(PlanError::BenefitMapIncomplete { missing }) => invalid(
            "output_ref",
            "benefit_map_incomplete",
            format!("a benefits-map edge is missing: {missing:?}"),
        ),
        PlanStoreError::Illegal(PlanError::BenefitOutputUnresolved { output_ref }) => invalid(
            "output_ref",
            "benefit_output_unresolved",
            format!(
                "output_ref {output_ref:?} names no wbs_node of this programme; pass the \
                 node's id"
            ),
        ),
        PlanStoreError::Illegal(PlanError::TrancheReviewEvidenceMissing) => invalid(
            "evidence",
            "tranche_evidence_missing",
            "a tranche review needs baselines, benefit status and the risk register",
        ),
        PlanStoreError::Illegal(PlanError::TrancheReviewNotContinue) => ActionError::new(
            409,
            "tranche_review_not_continue",
            "only a continue review authorises the next tranche",
        ),
        PlanStoreError::Illegal(PlanError::TrancheNotReviewable { state }) => ActionError::new(
            409,
            "tranche_not_reviewable",
            format!("the tranche is {state}; a closed or retired tranche cannot be reviewed"),
        ),
        PlanStoreError::Illegal(PlanError::TrancheAlreadyReviewed) => ActionError::new(
            409,
            "tranche_already_reviewed",
            "the tranche close already has its one review",
        ),
        PlanStoreError::Illegal(PlanError::NextTrancheMissing) => ActionError::new(
            409,
            "next_tranche_missing",
            "a continue review needs the next tranche in the plan",
        ),
        PlanStoreError::Illegal(PlanError::NextTrancheNotPlanned { state }) => ActionError::new(
            409,
            "next_tranche_not_planned",
            format!("the next tranche is {state}; only a planned tranche can be authorised"),
        ),
        PlanStoreError::Illegal(PlanError::BenefitBaselineMissing) => invalid(
            "baseline",
            "benefit_baseline_missing",
            "a benefit with no measured baseline cannot record a post-transition measurement",
        ),
        PlanStoreError::Illegal(PlanError::BenefitChangeRequired) => ActionError::new(
            409,
            "benefit_change_required",
            "an approved profile's measurement fields change only through an approved change \
             request",
        ),
        PlanStoreError::Illegal(PlanError::ChangeNotApproved) => ActionError::new(
            409,
            "change_not_approved",
            "the cited change is not an approved change request of this project",
        ),
        PlanStoreError::Illegal(PlanError::BenefitInTransition) => ActionError::new(
            409,
            "benefit_in_transition",
            "the benefit is in an open transition",
        ),
        PlanStoreError::Illegal(PlanError::BenefitRetired) => {
            ActionError::new(409, "benefit_retired", "the benefit profile is retired")
        }
        PlanStoreError::Illegal(PlanError::TrancheUnderReview) => {
            ActionError::new(409, "tranche_under_review", "the tranche is under review")
        }
        PlanStoreError::Illegal(PlanError::TrancheRetired) => {
            ActionError::new(409, "tranche_retired", "the tranche is retired")
        }
        PlanStoreError::Illegal(PlanError::TrancheNotPaused { state }) => ActionError::new(
            409,
            "tranche_not_paused",
            format!("the tranche is {state}; only a paused tranche can be resumed"),
        ),
        PlanStoreError::Illegal(PlanError::TrancheResumeApprovalMissing { approval_id }) => {
            invalid(
                "approval_id",
                "approval_missing",
                format!(
                    "approval {approval_id} is not an approved tranche.resume gate naming this \
                 tranche"
                ),
            )
        }
        PlanStoreError::Illegal(PlanError::RetroFindingWithoutLesson(key)) => invalid(
            "cadence_id",
            "retro_finding_without_lesson",
            format!("the retro finding {key:?} has no lesson; propose one before closing"),
        ),
        PlanStoreError::PiPlanNotApproved => ActionError::new(
            409,
            "pi_plan_not_approved",
            "the PI plan has no approved human decision: a person must approve it with \
             cadences.approve_plan before any card is drafted",
        ),
        PlanStoreError::Approval(error) => plan_approval_error(error, action),
        PlanStoreError::Illegal(other) => invalid("input", "plan_refused", other.to_string()),
        PlanStoreError::Conflict { what, why } => ActionError::new(
            409,
            "conflict",
            format!("the {what} cannot be removed: {why}"),
        ),
        PlanStoreError::Vocabulary { what, value } => {
            tracing::error!(what, value = %value, "a stored spine value is outside the domain");
            ActionError::internal()
        }
        PlanStoreError::Database(error) => store_error(error, action),
    }
}

/// A gate or decision refusal on the PI-plan approval path.
fn plan_approval_error(error: store::ApprovalStoreError, action: &str) -> ActionError {
    match error {
        store::ApprovalStoreError::NotFound => ActionError::resource_not_found("such approval"),
        store::ApprovalStoreError::Conflict => ActionError::new(
            409,
            "conflict",
            format!("{action:?}: a pending approval already gates this plan"),
        ),
        store::ApprovalStoreError::AlreadyDecided => ActionError::new(
            409,
            "conflict",
            format!("{action:?}: this approver already decided the gate"),
        ),
        store::ApprovalStoreError::Illegal(ApprovalError::RequesterCannotDecide) => {
            ActionError::new(
                409,
                "requester_cannot_decide",
                format!("{action:?}: the person who opened the plan's gate cannot decide it"),
            )
        }
        store::ApprovalStoreError::Illegal(error) => {
            ActionError::new(409, "conflict", error.to_string())
        }
        store::ApprovalStoreError::Vocabulary { what, value } => {
            tracing::error!(what, value = %value, "a stored approval value is outside the domain");
            ActionError::internal()
        }
        store::ApprovalStoreError::Parent(error) => parent_error(error, action),
        store::ApprovalStoreError::Run(error) => {
            tracing::error!(%error, action, "decide a PI plan gate");
            ActionError::internal()
        }
        store::ApprovalStoreError::HumanOnlyGate => ActionError::new(
            403,
            "human_only",
            format!("{action:?}: a PI plan is approved by a person"),
        ),
        store::ApprovalStoreError::EvalScopeRequired => ActionError::new(
            403,
            "eval_scope_required",
            format!("{action:?}: an eval sign-off needs evals:approve"),
        ),
        store::ApprovalStoreError::AnswerMissing => crate::control::invalid(
            "note",
            "missing",
            format!(
                "{action:?}: this gate requires an answer; a decision with no note answers nothing"
            ),
        ),
        store::ApprovalStoreError::Database(error) => store_error(error, action),
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// A WBS node as every `wbs.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WbsOut {
    /// The row id.
    pub id: String,
    /// The owning project.
    pub project_id: String,
    /// The parent node.
    pub parent_id: Option<String>,
    /// Sibling order.
    pub position: f64,
    /// The kind.
    pub kind: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// The status.
    pub status: String,
    /// The acceptance criteria.
    pub acceptance: Value,
    /// The evidence.
    pub evidence: Value,
    /// The card, if any.
    pub card_id: Option<String>,
    /// WSJF business value.
    pub business_value: i32,
    /// WSJF time criticality.
    pub time_criticality: i32,
    /// WSJF risk reduction.
    pub risk_reduction: i32,
    /// WSJF job size.
    pub job_size: i32,
    /// The computed score; never stored.
    pub wsjf: f64,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

fn wbs_out(record: &WbsRecord) -> Result<WbsOut, ActionError> {
    let score = wsjf(record.inputs()).map_err(|error| spine_error(error.into(), "wbs.read"))?;
    Ok(WbsOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        parent_id: record.parent_id.map(|id| id.to_string()),
        position: record.position,
        kind: record.kind.clone(),
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status.clone(),
        acceptance: record.acceptance.clone(),
        evidence: record.evidence.clone(),
        card_id: record.card_id.map(|id| id.to_string()),
        business_value: record.business_value,
        time_criticality: record.time_criticality,
        risk_reduction: record.risk_reduction,
        job_size: record.job_size,
        wsjf: score,
        meta: record.meta.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

/// `wbs.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WbsCreateIn {
    /// The owning project.
    pub project_id: Uuid,
    /// The parent node.
    pub parent_id: Option<Uuid>,
    /// Sibling order; absent appends.
    pub position: Option<f64>,
    /// `phase | deliverable | work_package | milestone`.
    pub kind: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: Option<String>,
    /// The acceptance criteria.
    pub acceptance: Option<Value>,
    /// WSJF business value.
    pub business_value: Option<i32>,
    /// WSJF time criticality.
    pub time_criticality: Option<i32>,
    /// WSJF risk reduction.
    pub risk_reduction: Option<i32>,
    /// WSJF job size; defaults to 1.
    pub job_size: Option<i32>,
    /// Free-form metadata.
    pub meta: Option<Value>,
    /// The approved change request that authorises this mutation under the project's approved
    /// scope baseline. Required once such a baseline exists (`409 rebaseline_requires_change`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<Uuid>,
}

/// `wbs.update`; every field is optional and `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WbsUpdateIn {
    /// The node to edit.
    pub id: Uuid,
    /// A new title.
    pub title: Option<String>,
    /// A new body.
    pub body: Option<String>,
    /// A new status.
    pub status: Option<String>,
    /// New acceptance criteria.
    pub acceptance: Option<Value>,
    /// New evidence.
    pub evidence: Option<Value>,
    /// A new card.
    pub card_id: Option<Uuid>,
    /// WSJF business value; all four inputs together.
    pub business_value: Option<i32>,
    /// WSJF time criticality.
    pub time_criticality: Option<i32>,
    /// WSJF risk reduction.
    pub risk_reduction: Option<i32>,
    /// WSJF job size.
    pub job_size: Option<i32>,
    /// A new sibling order.
    pub position: Option<f64>,
    /// A new parent.
    pub parent_id: Option<Uuid>,
    /// The approved change request that authorises this mutation under the project's approved
    /// scope baseline. Required once such a baseline exists (`409 rebaseline_requires_change`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<Uuid>,
}

/// `wbs.delete`; `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WbsDeleteIn {
    /// The node to remove.
    pub id: Uuid,
    /// The approved change request that authorises this mutation under the project's approved
    /// scope baseline. Required once such a baseline exists (`409 rebaseline_requires_change`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<Uuid>,
}

/// `wbs.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WbsListIn {
    /// The project whose tree is read.
    pub project_id: Uuid,
}

/// A baseline as `baselines.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BaselineOut {
    /// The row id.
    pub id: String,
    /// The owning project.
    pub project_id: String,
    /// `scope | schedule | cost`.
    pub kind: String,
    /// The frozen snapshot.
    pub snapshot: Value,
    /// When it was taken.
    pub taken_at: DateTime<Utc>,
    /// Who took it.
    pub taken_by: String,
    /// Who approved it.
    pub approved_by: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

fn baseline_out(record: &BaselineRecord) -> BaselineOut {
    BaselineOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        kind: record.kind.clone(),
        snapshot: record.snapshot.clone(),
        taken_at: record.taken_at,
        taken_by: record.taken_by.to_string(),
        approved_by: record.approved_by.map(|id| id.to_string()),
        version: record.version,
    }
}

/// `baselines.take`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BaselineTakeIn {
    /// The project to baseline.
    pub project_id: Uuid,
    /// `scope | schedule | cost`; `scope` when absent.
    pub kind: Option<String>,
    /// The approved change request that authorises advancing past an approved baseline
    /// (T3, #74). Required when an approved baseline of this `kind` already exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<Uuid>,
}

/// `baselines.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BaselineListIn {
    /// The project whose baselines are read.
    pub project_id: Uuid,
}

/// A register row as `registers.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterOut {
    /// The row id.
    pub id: String,
    /// The owning project.
    pub project_id: String,
    /// The kind.
    pub kind: String,
    /// The human code.
    pub code: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// The owning job.
    pub owner_job_id: Option<String>,
    /// The status.
    pub status: String,
    /// The severity.
    pub severity: Option<String>,
    /// The probability.
    pub probability: Option<String>,
    /// The impact.
    pub impact: Option<i32>,
    /// Related links.
    pub links: Value,
    /// The baseline a change changes.
    pub baseline_ref: Option<String>,
    /// Metadata; a change's impact block lives here.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

fn register_out(record: &RegisterRecord) -> RegisterOut {
    RegisterOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        kind: record.kind.clone(),
        code: record.code.clone(),
        title: record.title.clone(),
        body: record.body.clone(),
        owner_job_id: record.owner_job_id.map(|id| id.to_string()),
        status: record.status.clone(),
        severity: record.severity.clone(),
        probability: record.probability.clone(),
        impact: record.impact,
        links: record.links.clone(),
        baseline_ref: record.baseline_ref.map(|id| id.to_string()),
        meta: record.meta.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// `registers.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterCreateIn {
    /// The owning project.
    pub project_id: Uuid,
    /// `risk | issue | assumption | dependency | benefit | change`.
    pub kind: String,
    /// The human code.
    pub code: Option<String>,
    /// The title.
    pub title: String,
    /// The body.
    pub body: Option<String>,
    /// The owning job.
    pub owner_job_id: Option<Uuid>,
    /// The status.
    pub status: Option<String>,
    /// The severity.
    pub severity: Option<String>,
    /// The probability.
    pub probability: Option<String>,
    /// The impact.
    pub impact: Option<i32>,
    /// Related links.
    pub links: Option<Value>,
    /// The baseline a change changes.
    pub baseline_ref: Option<Uuid>,
    /// Metadata; a change's impact block lives here.
    pub meta: Option<Value>,
}

/// `registers.update`; `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterUpdateIn {
    /// The row to edit.
    pub id: Uuid,
    /// The new status.
    pub status: Option<String>,
    /// The new title.
    pub title: Option<String>,
    /// The new body.
    pub body: Option<String>,
    /// The new severity.
    pub severity: Option<String>,
    /// The new impact.
    pub impact: Option<i32>,
    /// New metadata.
    pub meta: Option<Value>,
}

/// `registers.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterListIn {
    /// The project whose register is read.
    pub project_id: Uuid,
}

/// A dependency edge as `project_dependencies.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DependencyOut {
    /// The row id.
    pub id: String,
    /// The programme, when the edge belongs to one.
    pub programme_id: Option<String>,
    /// The source project.
    pub from_project: String,
    /// The target project.
    pub to_project: String,
    /// `blocks | needs | supersedes | informs`.
    pub kind: String,
    /// The status.
    pub status: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
}

fn dependency_out(record: &DependencyRecord) -> DependencyOut {
    DependencyOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.map(|id| id.to_string()),
        from_project: record.from_project.to_string(),
        to_project: record.to_project.to_string(),
        kind: record.kind.clone(),
        status: record.status.clone(),
        meta: record.meta.clone(),
        version: record.version,
    }
}

/// `project_dependencies.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DependencyCreateIn {
    /// The programme, when the edge belongs to one.
    pub programme_id: Option<Uuid>,
    /// The source project.
    pub from_project: Uuid,
    /// The target project.
    pub to_project: Uuid,
    /// `blocks | needs | supersedes | informs`; `blocks` when absent.
    pub kind: Option<String>,
    /// Free-form metadata.
    pub meta: Option<Value>,
}

/// `project_dependencies.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DependencyListIn {
    /// The project whose edges are read.
    pub project_id: Uuid,
}

/// One in-flight project.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InFlightOut {
    /// The project id.
    pub id: String,
    /// The key.
    pub key: String,
    /// The name.
    pub name: String,
    /// The status.
    pub status: String,
    /// The programme, if any.
    pub programme_id: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

/// One in-flight programme. A programme with no projects is still in flight.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InFlightProgrammeOut {
    /// The programme id.
    pub id: String,
    /// The name.
    pub name: String,
    /// The status.
    pub status: String,
    /// The If-Match version.
    pub version: i64,
}

/// `portfolio.in_flight`'s output: the project page plus the in-flight programmes.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct InFlightPageOut {
    /// The in-flight projects — the page envelope's rows, unchanged.
    pub items: Vec<InFlightOut>,
    /// The programmes in the in-flight set; a programme with no projects still appears.
    pub programmes: Vec<InFlightProgrammeOut>,
    /// The cursor for the next page, if there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `portfolio.in_flight` — the shared page envelope, no filters.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct InFlightIn {
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// A tranche as `tranches.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheOut {
    /// The row id.
    pub id: String,
    /// The programme.
    pub programme_id: String,
    /// The tranche number.
    pub number: i32,
    /// The state.
    pub state: String,
    /// Entry criteria.
    pub entry_criteria: Value,
    /// Exit criteria.
    pub exit_criteria: Value,
    /// The baseline it is managed against.
    pub baseline_ref: Option<String>,
    /// The review date.
    pub review_at: Option<DateTime<Utc>>,
    /// Why the tranche was retired; empty unless `state='retired'`.
    pub retired_reason: String,
    /// When the tranche was authorised, if it has been.
    pub authorised_at: Option<DateTime<Utc>>,
    /// The review that authorised it.
    pub authorisation_review_id: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

fn tranche_out(record: &TrancheRecord) -> TrancheOut {
    TrancheOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        number: record.number,
        state: record.state.clone(),
        entry_criteria: record.entry_criteria.clone(),
        exit_criteria: record.exit_criteria.clone(),
        baseline_ref: record.baseline_ref.map(|id| id.to_string()),
        review_at: record.review_at,
        retired_reason: record.retired_reason.clone(),
        authorised_at: record.authorised_at,
        authorisation_review_id: record.authorisation_review_id.map(|id| id.to_string()),
        version: record.version,
    }
}

/// `tranches.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheCreateIn {
    /// The programme.
    pub programme_id: Uuid,
    /// The tranche number.
    pub number: i32,
    /// Entry criteria.
    pub entry_criteria: Option<Value>,
    /// Exit criteria.
    pub exit_criteria: Option<Value>,
    /// The review date.
    pub review_at: Option<DateTime<Utc>>,
}

/// `tranches.review`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheReviewIn {
    /// The tranche.
    pub id: Uuid,
    /// `continue | pause | stop`.
    pub decision: String,
    /// The evidence: baselines, benefit status and the risk register.
    pub evidence: Value,
    /// The published P0 standing-delegation decision output the reviewer acts under.
    pub delegation_ref: Uuid,
}

/// `tranches.review`'s approval target: what the gate authorises, computed from the input alone.
fn target_tranches_review(input: &TrancheReviewIn) -> Result<String, String> {
    Ok(format!("tranche/{}/review", input.id))
}

/// `tranches.update`; `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheUpdateIn {
    /// The tranche.
    pub id: Uuid,
    /// Entry criteria.
    pub entry_criteria: Option<Value>,
    /// Exit criteria.
    pub exit_criteria: Option<Value>,
    /// The programme baseline it is managed against.
    pub baseline_ref: Option<Uuid>,
    /// The review date.
    pub review_at: Option<DateTime<Utc>>,
    /// The approved change request that authorises a baseline change, if one is being made.
    #[serde(default)]
    pub change_ref: Option<Uuid>,
}

/// `tranches.review_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheReviewListIn {
    /// The programme whose reviews are read.
    pub programme_id: Uuid,
}

/// A recorded tranche review as `tranches.review_list` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheReviewOut {
    /// The row id.
    pub id: String,
    /// The reviewed tranche.
    pub tranche_id: String,
    /// `continue | pause | stop`.
    pub decision: String,
    /// The evidence the decision was taken on.
    pub evidence: Value,
    /// The acting user.
    pub reviewed_by: String,
    /// When the review was recorded.
    pub reviewed_at: DateTime<Utc>,
    /// The If-Match version.
    pub version: i64,
}

fn tranche_review_out(record: &TrancheReviewRecord) -> TrancheReviewOut {
    TrancheReviewOut {
        id: record.id.to_string(),
        tranche_id: record.tranche_id.to_string(),
        decision: record.decision.clone(),
        evidence: record.evidence.clone(),
        reviewed_by: record.reviewed_by.to_string(),
        reviewed_at: record.reviewed_at,
        version: record.version,
    }
}

/// `tranches.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheListIn {
    /// The programme whose tranches are read.
    pub programme_id: Uuid,
    /// Include retired tranches; false (the default) is the plan view.
    #[serde(default)]
    pub include_retired: bool,
}

/// A benefit profile as `benefits.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitOut {
    /// The row id.
    pub id: String,
    /// The programme.
    pub programme_id: String,
    /// The project, if any.
    pub project_id: Option<String>,
    /// The reference.
    pub reference: String,
    /// The description.
    pub description: String,
    /// The category.
    pub category: String,
    /// The objectives.
    pub objectives: Value,
    /// The observable outcome.
    pub observable_outcome: String,
    /// The KPIs.
    pub kpis: Value,
    /// The measured baseline.
    pub baseline: Option<Value>,
    /// The target.
    pub target: Option<Value>,
    /// The measurement method.
    pub measurement_method: String,
    /// The financial value.
    pub financial_value: String,
    /// The cost.
    pub cost: Value,
    /// Required capabilities.
    pub capabilities: Value,
    /// Preconditions.
    pub preconditions: Value,
    /// Risks.
    pub risks: Value,
    /// The owning job.
    pub owner_job_id: Option<String>,
    /// The BCM job.
    pub benefit_owner_job_id: Option<String>,
    /// The operational area.
    pub operational_area: String,
    /// The status.
    pub status: String,
    /// Why the profile was retired; empty unless `status='retired'`.
    pub retired_reason: String,
    /// The approved change request that last advanced the measurement.
    pub change_ref: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

fn benefit_out(record: &BenefitRecord) -> BenefitOut {
    BenefitOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        project_id: record.project_id.map(|id| id.to_string()),
        reference: record.reference.clone(),
        description: record.description.clone(),
        category: record.category.clone(),
        objectives: record.objectives.clone(),
        observable_outcome: record.observable_outcome.clone(),
        kpis: record.kpis.clone(),
        baseline: record.baseline.clone(),
        target: record.target.clone(),
        measurement_method: record.measurement_method.clone(),
        financial_value: record.financial_value.clone(),
        cost: record.cost.clone(),
        capabilities: record.capabilities.clone(),
        preconditions: record.preconditions.clone(),
        risks: record.risks.clone(),
        owner_job_id: record.owner_job_id.map(|id| id.to_string()),
        benefit_owner_job_id: record.benefit_owner_job_id.map(|id| id.to_string()),
        operational_area: record.operational_area.clone(),
        status: record.status.clone(),
        retired_reason: record.retired_reason.clone(),
        change_ref: record.change_ref.map(|id| id.to_string()),
        version: record.version,
    }
}

/// `benefits.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitCreateIn {
    /// The programme.
    pub programme_id: Uuid,
    /// The project.
    pub project_id: Option<Uuid>,
    /// The register row that indexes the benefit.
    pub register_item_id: Option<Uuid>,
    /// The reference.
    pub reference: String,
    /// The description.
    pub description: Option<String>,
    /// The category.
    pub category: Option<String>,
    /// The objectives.
    pub objectives: Option<Value>,
    /// The observable outcome.
    pub observable_outcome: Option<String>,
    /// The KPIs.
    pub kpis: Option<Value>,
    /// The measured baseline.
    pub baseline: Option<Value>,
    /// The target.
    pub target: Option<Value>,
    /// The measurement method.
    pub measurement_method: Option<String>,
    /// The financial value.
    pub financial_value: Option<String>,
    /// The cost.
    pub cost: Option<Value>,
    /// Required capabilities.
    pub capabilities: Option<Value>,
    /// Preconditions.
    pub preconditions: Option<Value>,
    /// Risks.
    pub risks: Option<Value>,
    /// The owning job.
    pub owner_job_id: Option<Uuid>,
    /// The BCM job.
    pub benefit_owner_job_id: Option<Uuid>,
    /// The operational area.
    pub operational_area: Option<String>,
    /// The status.
    pub status: Option<String>,
}

/// `benefits.link`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitLinkIn {
    /// The programme.
    pub programme_id: Uuid,
    /// The benefit.
    pub benefit_id: Uuid,
    /// The output level: a `wbs_node` id of a project in the same programme.
    #[serde(default)]
    pub output_ref: String,
    /// The capability level.
    #[serde(default)]
    pub capability_ref: String,
    /// The operational outcome level.
    #[serde(default)]
    pub operational_outcome: String,
    /// The strategic objective level.
    #[serde(default)]
    pub strategic_objective: String,
    /// `benefit | disbenefit`.
    pub kind: Option<String>,
}

/// A benefits-map edge as `benefits.link` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitLinkOut {
    /// The row id.
    pub id: String,
    /// The programme.
    pub programme_id: String,
    /// The output.
    pub output_ref: String,
    /// The capability.
    pub capability_ref: String,
    /// The operational outcome.
    pub operational_outcome: String,
    /// The benefit.
    pub benefit_id: String,
    /// The strategic objective.
    pub strategic_objective: String,
    /// `benefit | disbenefit`.
    pub kind: String,
    /// The If-Match version.
    pub version: i64,
}

/// `benefits.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitListIn {
    /// The programme whose benefits are read.
    pub programme_id: Uuid,
    /// Include retired profiles; false (the default) is the benefits map.
    #[serde(default)]
    pub include_retired: bool,
}

/// `benefits.update`; `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitUpdateIn {
    /// The profile to edit.
    pub id: Uuid,
    /// The project.
    pub project_id: Option<Uuid>,
    /// The register row that indexes the benefit.
    pub register_item_id: Option<Uuid>,
    /// The reference.
    pub reference: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// The category.
    pub category: Option<String>,
    /// The objectives.
    pub objectives: Option<Value>,
    /// The observable outcome.
    pub observable_outcome: Option<String>,
    /// The KPIs.
    pub kpis: Option<Value>,
    /// The measured baseline.
    pub baseline: Option<Value>,
    /// The target.
    pub target: Option<Value>,
    /// The measurement method.
    pub measurement_method: Option<String>,
    /// The financial value.
    pub financial_value: Option<String>,
    /// The cost.
    pub cost: Option<Value>,
    /// Required capabilities.
    pub capabilities: Option<Value>,
    /// Preconditions.
    pub preconditions: Option<Value>,
    /// Risks.
    pub risks: Option<Value>,
    /// The owning job.
    pub owner_job_id: Option<Uuid>,
    /// The BCM job.
    pub benefit_owner_job_id: Option<Uuid>,
    /// The operational area.
    pub operational_area: Option<String>,
    /// `draft | active | realized | disbenefit | closed`.
    pub status: Option<String>,
    /// The approved change request that authorises a measurement change.
    pub change_id: Option<Uuid>,
}

/// `benefits.link_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitLinkListIn {
    /// The programme whose edges are read.
    pub programme_id: Uuid,
}

/// `benefits.transition_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitTransitionListIn {
    /// The programme whose realization records are read.
    pub programme_id: Uuid,
}

/// `benefits.transition_upsert`; `If-Match` is required when the record exists.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitTransitionUpsertIn {
    /// The benefit.
    pub benefit_id: Uuid,
    /// When the baseline was measured.
    pub baseline_measured_at: Option<DateTime<Utc>>,
    /// The readiness assessment.
    pub readiness: Option<Value>,
    /// When the transition was enacted.
    pub enacted_at: Option<DateTime<Utc>>,
    /// When the post-transition measurement was taken.
    pub post_measured_at: Option<DateTime<Utc>>,
    /// When the legacy practice was retired.
    pub retired_at: Option<DateTime<Utc>>,
    /// The evidence.
    pub evidence: Option<Value>,
}

/// A realization record as `benefits.transition_*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitTransitionOut {
    /// The row id.
    pub id: String,
    /// The benefit.
    pub benefit_id: String,
    /// When the baseline was measured.
    pub baseline_measured_at: Option<DateTime<Utc>>,
    /// The readiness assessment.
    pub readiness: Value,
    /// When the transition was enacted.
    pub enacted_at: Option<DateTime<Utc>>,
    /// When the post-transition measurement was taken.
    pub post_measured_at: Option<DateTime<Utc>>,
    /// When the legacy practice was retired.
    pub retired_at: Option<DateTime<Utc>>,
    /// The evidence.
    pub evidence: Value,
    /// The If-Match version.
    pub version: i64,
}

fn benefit_link_out(record: &BenefitLinkRecord) -> BenefitLinkOut {
    BenefitLinkOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        output_ref: record.output_ref.clone(),
        capability_ref: record.capability_ref.clone(),
        operational_outcome: record.operational_outcome.clone(),
        benefit_id: record.benefit_id.to_string(),
        strategic_objective: record.strategic_objective.clone(),
        kind: record.kind.clone(),
        version: record.version,
    }
}

fn benefit_transition_out(record: &TransitionRecord) -> BenefitTransitionOut {
    BenefitTransitionOut {
        id: record.id.to_string(),
        benefit_id: record.benefit_id.to_string(),
        baseline_measured_at: record.baseline_measured_at,
        readiness: record.readiness.clone(),
        enacted_at: record.enacted_at,
        post_measured_at: record.post_measured_at,
        retired_at: record.retired_at,
        evidence: record.evidence.clone(),
        version: record.version,
    }
}

// ── WBS actions ─────────────────────────────────────────────────────────────────────────

/// Refuse a direct WBS mutation under an approved scope baseline unless the caller cites the
/// approved change request that names that baseline (v1-p5-4, T3's change guard).
///
/// `take_baseline_guarded` guards the baseline itself; without this guard a `wbs.*` call
/// would edit the scope a frozen, approved baseline records without the change control the
/// baseline exists to impose. A project with no approved scope baseline is untouched.
async fn wbs_change_guard(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    project_id: Uuid,
    change_id: Option<Uuid>,
) -> Result<(), ActionError> {
    let Some(baseline) =
        store::spine::latest_approved_baseline_tx(&mut *tx, ctx.tenant, project_id, "scope")
            .await
            .map_err(|error| spine_error(error, ctx.action))?
    else {
        return Ok(());
    };
    let refusal = |detail: String| ActionError::new(409, "rebaseline_requires_change", detail);
    let Some(change_id) = change_id else {
        return Err(refusal(format!(
            "project {project_id} already holds an approved scope baseline ({}); a WBS mutation \
             needs the approved change request that cites it in `change_id`",
            baseline.id
        )));
    };
    let approved =
        store::spine::approved_change_for_baseline_tx(&mut *tx, ctx.tenant, change_id, baseline.id)
            .await
            .map_err(|error| store_error(error, ctx.action))?;
    if !approved {
        return Err(refusal(format!(
            "`change_id` {change_id} is not an approved change request whose `baseline_ref` is \
             the approved scope baseline {}",
            baseline.id
        )));
    }
    Ok(())
}

/// Create a WBS node.
#[crate::action(
    name = "wbs.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn wbs_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WbsCreateIn,
) -> Result<HandlerOutput<WbsOut>, ActionError> {
    let kind = input.kind.trim().to_owned();
    if conveyor_domain::spine::WbsKind::parse(&kind).is_none() {
        return Err(invalid(
            "kind",
            "unknown_kind",
            "kind must be phase, deliverable, work_package or milestone",
        ));
    }
    let title = input.title.trim().to_owned();
    if title.is_empty() {
        return Err(invalid("title", "required", "a title is required"));
    }
    let inputs = WsjfInputs {
        business_value: input.business_value.unwrap_or(0),
        time_criticality: input.time_criticality.unwrap_or(0),
        risk_reduction: input.risk_reduction.unwrap_or(0),
        job_size: input.job_size.unwrap_or(1),
    };
    wsjf(inputs).map_err(|error| spine_error(error.into(), ctx.action))?;
    wbs_change_guard(&mut *tx, &ctx, input.project_id, input.change_id).await?;
    let record = store::spine::insert_wbs_tx(
        &mut *tx,
        ctx.tenant,
        &NewWbsNode {
            project_id: input.project_id,
            parent_id: input.parent_id,
            position: input.position,
            kind,
            title,
            body: input.body.unwrap_or_default(),
            acceptance: input.acceptance.unwrap_or_else(|| serde_json::json!([])),
            inputs,
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "wbs_node", record.id, "created").await?;
    Ok(HandlerOutput::created(wbs_out(&record)?).with_etag(etag_of(record.version)))
}

/// Edit a WBS node; `If-Match` is required.
#[crate::action(
    name = "wbs.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn wbs_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WbsUpdateIn,
) -> Result<HandlerOutput<WbsOut>, ActionError> {
    let current = store::spine::get_wbs_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such WBS node"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    wbs_change_guard(&mut *tx, &ctx, current.project_id, input.change_id).await?;
    let inputs = match (
        input.business_value,
        input.time_criticality,
        input.risk_reduction,
        input.job_size,
    ) {
        (None, None, None, None) => None,
        (business_value, time_criticality, risk_reduction, job_size) => Some(WsjfInputs {
            business_value: business_value.ok_or_else(|| {
                invalid(
                    "business_value",
                    "required",
                    "all four WSJF inputs together",
                )
            })?,
            time_criticality: time_criticality.ok_or_else(|| {
                invalid(
                    "time_criticality",
                    "required",
                    "all four WSJF inputs together",
                )
            })?,
            risk_reduction: risk_reduction.ok_or_else(|| {
                invalid(
                    "risk_reduction",
                    "required",
                    "all four WSJF inputs together",
                )
            })?,
            job_size: job_size
                .ok_or_else(|| invalid("job_size", "required", "all four WSJF inputs together"))?,
        }),
    };
    if let Some(inputs) = inputs {
        wsjf(inputs).map_err(|error| spine_error(error.into(), ctx.action))?;
    }
    if let Some(status) = input.status.as_deref()
        && !matches!(status, "open" | "accepted" | "cancelled")
    {
        return Err(invalid(
            "status",
            "unknown_status",
            "status must be open, accepted or cancelled",
        ));
    }
    if input.status.as_deref() == Some("accepted")
        && !input.evidence.as_ref().is_some_and(carries_evidence)
        && !carries_evidence(&current.evidence)
    {
        return Err(invalid(
            "evidence",
            "required",
            "an accepted deliverable must carry the evidence that met its acceptance",
        ));
    }
    let patch = WbsPatch {
        title: trimmed(input.title),
        body: input.body,
        status: input.status,
        acceptance: input.acceptance,
        evidence: input.evidence,
        card_id: input.card_id,
        inputs,
        position: input.position,
        parent_id: input.parent_id,
    };
    let record = store::spine::update_wbs_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        &patch,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "wbs_node", record.id, "updated").await?;
    Ok(HandlerOutput::ok(wbs_out(&record)?).with_etag(etag_of(record.version)))
}

/// Read one WBS node.
#[crate::action(
    name = "wbs.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn wbs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<WbsOut>, ActionError> {
    let record = store::spine::get_wbs_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such WBS node"))?;
    Ok(HandlerOutput::ok(wbs_out(&record)?).with_etag(etag_of(record.version)))
}

/// List a project's WBS nodes.
#[crate::action(
    name = "wbs.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn wbs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WbsListIn,
) -> Result<Page<WbsOut>, ActionError> {
    let records = store::spine::list_wbs_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    let items = records.iter().map(wbs_out).collect::<Result<Vec<_>, _>>()?;
    Ok(Page {
        items,
        next_cursor: None,
    })
}

/// `wbs.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_wbs_delete(input: &WbsDeleteIn) -> Result<String, String> {
    Ok(format!("wbs/{}", input.id))
}

/// Remove a WBS node; a node with children is refused.
#[crate::action(
    name = "wbs.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_wbs_delete",
)]
async fn wbs_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WbsDeleteIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_wbs_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such WBS node"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    wbs_change_guard(&mut *tx, &ctx, current.project_id, input.change_id).await?;
    store::spine::delete_wbs_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "wbs_node", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}

// ── baseline actions ────────────────────────────────────────────────────────────────────

/// Take a baseline.
#[crate::action(
    name = "baselines.take",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn baselines_take(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BaselineTakeIn,
) -> Result<HandlerOutput<BaselineOut>, ActionError> {
    let kind = input.kind.unwrap_or_else(|| "scope".to_owned());
    if !matches!(kind.as_str(), "scope" | "schedule" | "cost") {
        return Err(invalid(
            "kind",
            "unknown_kind",
            "kind must be scope, schedule or cost",
        ));
    }
    let record =
        take_baseline_guarded(&mut *tx, &ctx, input.project_id, &kind, input.change_id).await?;
    emit_spine_event(&mut *tx, &ctx, "project_baseline", record.id, "created").await?;
    Ok(HandlerOutput::created(baseline_out(&record)).with_etag(etag_of(record.version)))
}

/// Take a baseline under T3's change guard: an approved baseline of the same kind advances only
/// through a change request whose `change` gate an approver has decided. The change row and the
/// new baseline cite each other (`change_id` on the baseline, `meta.baseline_id` on the change).
///
/// The approval is the second principal's decision, NEVER the row's self-declared `status`:
/// `registers.create`/`update` write any status, so a status read would let any
/// `portfolio:write` caller approve its own change and re-baseline (§8.4). The guard reads
/// [`store::spine::change_is_approved_tx`], the same approval the decide path writes.
async fn take_baseline_guarded(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    project_id: Uuid,
    kind: &str,
    change_id: Option<Uuid>,
) -> Result<store::spine::BaselineRecord, ActionError> {
    let action = "baselines.take";
    if let Some(previous) =
        store::spine::latest_approved_baseline_tx(tx, ctx.tenant, project_id, kind)
            .await
            .map_err(|error| spine_error(error, action))?
    {
        let Some(change_id) = change_id else {
            return Err(ActionError::new(
                409,
                "change_request_required",
                format!(
                    "project {project_id} already holds an approved {kind} baseline ({}); a new \
                     baseline needs an approved change request in `change_id`",
                    previous.id
                ),
            ));
        };
        let change = store::spine::get_register_tx(tx, ctx.tenant, change_id)
            .await
            .map_err(|error| spine_error(error, action))?
            .ok_or_else(|| invalid("change_id", "not_found", "no such change request"))?;
        let approved =
            store::spine::change_is_approved_tx(tx, ctx.tenant, change_id, Some(project_id))
                .await
                .map_err(|error| store_error(error, action))?;
        if change.kind != "change" || !approved || change.baseline_ref != Some(previous.id) {
            return Err(ActionError::new(
                409,
                "change_request_invalid",
                format!(
                    "change {change_id} must be a change whose `change` gate is approved and \
                     whose baseline_ref is the approved baseline {}",
                    previous.id
                ),
            ));
        }
    } else if change_id.is_some() {
        return Err(invalid(
            "change_id",
            "not_applicable",
            "there is no approved baseline of this kind to change",
        ));
    }
    let record =
        store::spine::take_baseline_tx(tx, ctx.tenant, project_id, kind, ctx.actor_id, change_id)
            .await
            .map_err(|error| spine_error(error, action))?;
    if let Some(change_id) = change_id {
        store::spine::link_change_to_baseline_tx(tx, ctx.tenant, change_id, record.id)
            .await
            .map_err(|error| spine_error(error, action))?;
    }
    Ok(record)
}

/// Read one baseline.
#[crate::action(
    name = "baselines.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn baselines_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<BaselineOut>, ActionError> {
    let record = store::spine::get_baseline_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such baseline"))?;
    Ok(HandlerOutput::ok(baseline_out(&record)).with_etag(etag_of(record.version)))
}

/// List a project's baselines.
#[crate::action(
    name = "baselines.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn baselines_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BaselineListIn,
) -> Result<Page<BaselineOut>, ActionError> {
    let records = store::spine::list_baselines_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(baseline_out).collect(),
        next_cursor: None,
    })
}

// ── register actions ────────────────────────────────────────────────────────────────────

/// Create a register row.
#[crate::action(
    name = "registers.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn registers_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RegisterCreateIn,
) -> Result<HandlerOutput<RegisterOut>, ActionError> {
    let kind = input.kind.trim().to_owned();
    if !matches!(
        kind.as_str(),
        "risk" | "issue" | "assumption" | "dependency" | "benefit" | "change"
    ) {
        return Err(invalid(
            "kind",
            "unknown_kind",
            "kind must be risk, issue, assumption, dependency, benefit or change",
        ));
    }
    // §8.4: approval is a gate effect, never a row write. A change is approved by its `change`
    // gate's decision (the read `baselines.take` uses); a create that self-declares `approved`
    // would be the bypass that guard exists to close.
    if kind == "change" && input.status.as_deref() == Some("approved") {
        return Err(invalid(
            "status",
            "approval_gate_only",
            "a change is approved by its `change` gate decision, not by a status write",
        ));
    }
    let record = store::spine::insert_register_tx(
        &mut *tx,
        ctx.tenant,
        &NewRegisterItem {
            project_id: input.project_id,
            kind,
            code: input.code.unwrap_or_default(),
            title: input.title,
            body: input.body.unwrap_or_default(),
            owner_job_id: input.owner_job_id,
            status: input.status.unwrap_or_else(|| "open".to_owned()),
            severity: input.severity,
            probability: input.probability,
            impact: input.impact,
            links: input.links.unwrap_or_else(|| serde_json::json!([])),
            baseline_ref: input.baseline_ref,
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "project_register_item",
        record.id,
        "created",
    )
    .await?;
    Ok(HandlerOutput::created(register_out(&record)).with_etag(etag_of(record.version)))
}

/// Edit a register row; `If-Match` is required.
#[crate::action(
    name = "registers.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn registers_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RegisterUpdateIn,
) -> Result<HandlerOutput<RegisterOut>, ActionError> {
    let current = store::spine::get_register_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such register item"))?;
    // The same rule as `registers.create`: a change's approval is the `change` gate's decision,
    // so a status edit cannot manufacture one.
    if current.kind == "change" && input.status.as_deref() == Some("approved") {
        return Err(invalid(
            "status",
            "approval_gate_only",
            "a change is approved by its `change` gate decision, not by a status write",
        ));
    }
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let record = store::spine::update_register_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        input.status.as_deref(),
        trimmed(input.title).as_deref(),
        input.body.as_deref(),
        input.severity.as_deref(),
        input.impact,
        input.meta.as_ref(),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "project_register_item",
        record.id,
        "updated",
    )
    .await?;
    Ok(HandlerOutput::ok(register_out(&record)).with_etag(etag_of(record.version)))
}

/// Read one register row.
#[crate::action(
    name = "registers.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn registers_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<RegisterOut>, ActionError> {
    let record = store::spine::get_register_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such register item"))?;
    Ok(HandlerOutput::ok(register_out(&record)).with_etag(etag_of(record.version)))
}

/// List a project's register.
#[crate::action(
    name = "registers.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn registers_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RegisterListIn,
) -> Result<Page<RegisterOut>, ActionError> {
    let records = store::spine::list_register_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(register_out).collect(),
        next_cursor: None,
    })
}

/// `registers.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_registers_delete(input: &IdIn) -> Result<String, String> {
    Ok(format!("register/{}", input.id))
}

/// Remove a register item; an item a benefit profile indexes is refused.
#[crate::action(
    name = "registers.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_registers_delete",
)]
async fn registers_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_register_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such register item"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    store::spine::delete_register_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "project_register_item", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}

// ── dependency actions ─────────────────────────────────────────────────────────────────

/// Create a dependency edge; a cycle is refused in the same transaction.
#[crate::action(
    name = "project_dependencies.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_dependencies_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DependencyCreateIn,
) -> Result<HandlerOutput<DependencyOut>, ActionError> {
    let kind = input.kind.unwrap_or_else(|| "blocks".to_owned());
    if !matches!(kind.as_str(), "blocks" | "needs" | "supersedes" | "informs") {
        return Err(invalid(
            "kind",
            "unknown_kind",
            "kind must be blocks, needs, supersedes or informs",
        ));
    }
    let record = store::spine::insert_dependency_tx(
        &mut *tx,
        ctx.tenant,
        input.programme_id,
        input.from_project,
        input.to_project,
        &kind,
        &input.meta.unwrap_or_else(|| serde_json::json!({})),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "project_dependency", record.id, "created").await?;
    Ok(HandlerOutput::created(dependency_out(&record)).with_etag(etag_of(record.version)))
}

/// `project_dependencies.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_project_dependencies_delete(input: &IdIn) -> Result<String, String> {
    Ok(format!("project_dependency/{}", input.id))
}

/// Delete a dependency edge.
#[crate::action(
    name = "project_dependencies.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_project_dependencies_delete",
)]
async fn project_dependencies_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<Value>, ActionError> {
    store::spine::delete_dependency_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "project_dependency", input.id, "deleted").await?;
    Ok(HandlerOutput::ok(
        serde_json::json!({ "deleted": input.id }),
    ))
}

/// List a project's dependency edges.
#[crate::action(
    name = "project_dependencies.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_dependencies_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DependencyListIn,
) -> Result<Page<DependencyOut>, ActionError> {
    let records = store::spine::list_dependencies_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(dependency_out).collect(),
        next_cursor: None,
    })
}

// ── portfolio.in_flight ────────────────────────────────────────────────────────────────

/// The computed in-flight set; no portfolio row exists.
#[crate::action(
    name = "portfolio.in_flight",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn portfolio_in_flight(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: InFlightIn,
) -> Result<InFlightPageOut, ActionError> {
    let in_flight = store::spine::in_flight_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(InFlightPageOut {
        items: in_flight
            .projects
            .iter()
            .map(|record: &InFlightRecord| InFlightOut {
                id: record.id.to_string(),
                key: record.key.clone(),
                name: record.name.clone(),
                status: record.status.clone(),
                programme_id: record.programme_id.map(|id| id.to_string()),
                version: record.version,
            })
            .collect(),
        programmes: in_flight
            .programmes
            .iter()
            .map(|record| InFlightProgrammeOut {
                id: record.id.to_string(),
                name: record.name.clone(),
                status: record.status.clone(),
                version: record.version,
            })
            .collect(),
        next_cursor: None,
    })
}

// ── tranche actions ────────────────────────────────────────────────────────────────────

/// Create a tranche.
#[crate::action(
    name = "tranches.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheCreateIn,
) -> Result<HandlerOutput<TrancheOut>, ActionError> {
    let record = store::spine::insert_tranche_tx(
        &mut *tx,
        ctx.tenant,
        input.programme_id,
        input.number,
        &input
            .entry_criteria
            .unwrap_or_else(|| serde_json::json!([])),
        &input.exit_criteria.unwrap_or_else(|| serde_json::json!([])),
        input.review_at,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", record.id, "created").await?;
    Ok(HandlerOutput::created(tranche_out(&record)).with_etag(etag_of(record.version)))
}

/// A git commit id as review evidence names it: exactly 40 hexadecimal characters, either case.
fn is_git_sha(raw: &str) -> bool {
    raw.len() == 40 && raw.chars().all(|character| character.is_ascii_hexdigit())
}

/// The refusal for a review whose evidence does not name the revision it judged (v1-tr2-2).
fn tranche_review_unpinned() -> ActionError {
    ActionError::new(
        422,
        "tranche_review_unpinned",
        "a tranche review must name the revision it judged: evidence.snapshot_sha must be a \
         40-character hex commit id; evidence.snapshot.git_sha is accepted and recorded as \
         snapshot_sha",
    )
}

/// Record an end-of-tranche review; evidence is mandatory.
#[crate::action(
    name = "tranches.review",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_tranches_review"
)]
async fn tranches_review(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheReviewIn,
) -> Result<HandlerOutput<TrancheOut>, ActionError> {
    // The MSP decision is dual-controlled: the executor requests the gate, a distinct delegate
    // decides it, and this action records the delegate's decision. The approval target is the
    // exact tranche review the gate authorises.
    let target = target_tranches_review(&input)
        .map_err(|detail| invalid("input", "invalid_value", &detail))?;
    let approval_id = crate::control::approvals::require_approval_tx(
        &mut *tx,
        &ctx,
        "tranches.review",
        &target,
        ctx.approval_id,
    )
    .await?;
    crate::programme::assert_delegation_ref(&mut *tx, ctx.tenant, input.delegation_ref).await?;
    let reviewer = store::approval::deciding_approver_tx(&mut *tx, ctx.tenant, approval_id)
        .await
        .map_err(|error| store_error(error, ctx.action))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "tranche_review_not_approved",
                "the tranche review gate has no approving decision",
            )
        })?;
    let mut evidence = match input.evidence {
        Value::Object(mut map) => {
            map.insert(
                "delegation_ref".to_owned(),
                Value::String(input.delegation_ref.to_string()),
            );
            Value::Object(map)
        }
        Value::Null => serde_json::json!({
            "delegation_ref": input.delegation_ref.to_string(),
        }),
        _ => {
            return Err(invalid(
                "evidence",
                "evidence_object_required",
                "the review evidence must be a JSON object",
            ));
        }
    };
    // v1-tr2-2 — once the evidence is complete enough for the §8.3 check to pass, it must also
    // name the revision it judged, or the review is not pinned to anything reproducible. The
    // flat `snapshot_sha` is canonical; the nested `snapshot.git_sha` the pack emits is accepted
    // and normalised into it, so every stored review carries the same key. The pin is checked
    // only after the completeness predicate, so an empty evidence object still reports the more
    // fundamental `tranche_evidence_missing` from the store.
    if tranche_review_has_evidence(&evidence) {
        let flat = evidence.get("snapshot_sha").cloned();
        let nested = evidence
            .pointer("/snapshot/git_sha")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match flat {
            Some(Value::String(sha)) if is_git_sha(&sha) => {}
            Some(_) => return Err(tranche_review_unpinned()),
            None => match nested {
                Some(sha) if is_git_sha(&sha) => {
                    evidence["snapshot_sha"] = Value::String(sha);
                }
                _ => return Err(tranche_review_unpinned()),
            },
        }
    }
    let record = store::spine::review_tranche_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        input.decision.trim(),
        &evidence,
        reviewer,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", record.id, "reviewed").await?;
    Ok(HandlerOutput::ok(tranche_out(&record)).with_etag(etag_of(record.version)))
}

/// The gate a paused tranche's resume is authorised under.
pub const TRANCHE_RESUME_GATE: &str = "tranche.resume";

/// `tranches.resume` — move a paused tranche back to `in_review` under its approved gate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheResumeIn {
    /// The paused tranche.
    pub tranche_id: Uuid,
    /// The approved `tranche.resume` gate naming this tranche.
    pub approval_id: Uuid,
    /// Why the tranche resumes; recorded with the resume.
    #[serde(default)]
    pub note: Option<String>,
}

#[crate::action(
    name = "tranches.resume",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_resume(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheResumeIn,
) -> Result<HandlerOutput<TrancheOut>, ActionError> {
    let record = store::spine::resume_tranche_tx(
        &mut *tx,
        ctx.tenant,
        input.tranche_id,
        input.approval_id,
        input.note.as_deref(),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", record.id, "resumed").await?;
    Ok(HandlerOutput::ok(tranche_out(&record)).with_etag(etag_of(record.version)))
}

/// `tranches.authorise` — repair a `continue` review whose next-tranche authorisation never
/// landed, linking the existing review. It cannot manufacture a review.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheAuthoriseIn {
    /// The `continue` review whose next-tranche authorisation never landed.
    pub review_id: Uuid,
}

#[crate::action(
    name = "tranches.authorise",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_authorise(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheAuthoriseIn,
) -> Result<HandlerOutput<TrancheOut>, ActionError> {
    let record =
        store::spine::authorise_tranche_from_review_tx(&mut *tx, ctx.tenant, input.review_id)
            .await
            .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", record.id, "authorised").await?;
    Ok(HandlerOutput::ok(tranche_out(&record)).with_etag(etag_of(record.version)))
}

/// List a programme's tranches.
#[crate::action(
    name = "tranches.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheListIn,
) -> Result<Page<TrancheOut>, ActionError> {
    let records = store::spine::list_tranches_tx(
        &mut *tx,
        ctx.tenant,
        input.programme_id,
        input.include_retired,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(tranche_out).collect(),
        next_cursor: None,
    })
}

/// Edit a tranche's plan fields; the state changes only through the review.
#[crate::action(
    name = "tranches.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheUpdateIn,
) -> Result<HandlerOutput<TrancheOut>, ActionError> {
    let current = store::spine::get_tranche_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such tranche"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let record = store::spine::update_tranche_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        &TranchePatch {
            entry_criteria: input.entry_criteria,
            exit_criteria: input.exit_criteria,
            baseline_ref: input.baseline_ref,
            review_at: input.review_at,
            change_ref: input.change_ref,
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", record.id, "updated").await?;
    Ok(HandlerOutput::ok(tranche_out(&record)).with_etag(etag_of(record.version)))
}

/// `tranches.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_tranches_delete(input: &IdIn) -> Result<String, String> {
    Ok(format!("tranche/{}", input.id))
}

/// Remove a tranche. A reviewed tranche is refused: the review is history.
#[crate::action(
    name = "tranches.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_tranches_delete",
)]
async fn tranches_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_tranche_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such tranche"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    store::spine::delete_tranche_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}

/// List a programme's recorded tranche reviews.
#[crate::action(
    name = "tranches.review_list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_review_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheReviewListIn,
) -> Result<Page<TrancheReviewOut>, ActionError> {
    let records = store::spine::list_tranche_reviews_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(tranche_review_out).collect(),
        next_cursor: None,
    })
}

// ── benefit actions ────────────────────────────────────────────────────────────────────

/// Create a benefit profile; a missing baseline, target, method or owner is refused.
#[crate::action(
    name = "benefits.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitCreateIn,
) -> Result<HandlerOutput<BenefitOut>, ActionError> {
    let reference = input.reference.trim().to_owned();
    if reference.is_empty() {
        return Err(invalid(
            "reference",
            "required",
            "a benefit reference is required",
        ));
    }
    let record = store::spine::insert_benefit_tx(
        &mut *tx,
        ctx.tenant,
        &NewBenefitProfile {
            programme_id: input.programme_id,
            project_id: input.project_id,
            register_item_id: input.register_item_id,
            reference,
            description: input.description.unwrap_or_default(),
            category: input.category.unwrap_or_default(),
            objectives: input.objectives.unwrap_or_else(|| serde_json::json!([])),
            observable_outcome: input.observable_outcome.unwrap_or_default(),
            kpis: input.kpis.unwrap_or_else(|| serde_json::json!([])),
            baseline: input.baseline,
            target: input.target,
            measurement_method: input.measurement_method.unwrap_or_default(),
            financial_value: input.financial_value.unwrap_or_default(),
            cost: input.cost.unwrap_or_else(|| serde_json::json!({})),
            capabilities: input.capabilities.unwrap_or_else(|| serde_json::json!([])),
            preconditions: input.preconditions.unwrap_or_else(|| serde_json::json!([])),
            risks: input.risks.unwrap_or_else(|| serde_json::json!([])),
            owner_job_id: input.owner_job_id,
            benefit_owner_job_id: input.benefit_owner_job_id,
            operational_area: input.operational_area.unwrap_or_default(),
            status: input.status.unwrap_or_else(|| "draft".to_owned()),
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "benefit_profile", record.id, "created").await?;
    Ok(HandlerOutput::created(benefit_out(&record)).with_etag(etag_of(record.version)))
}

/// Read one benefit profile.
#[crate::action(
    name = "benefits.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<BenefitOut>, ActionError> {
    let record = store::spine::get_benefit_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such benefit profile"))?;
    Ok(HandlerOutput::ok(benefit_out(&record)).with_etag(etag_of(record.version)))
}

/// List a programme's benefit profiles.
#[crate::action(
    name = "benefits.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitListIn,
) -> Result<Page<BenefitOut>, ActionError> {
    let records = store::spine::list_benefits_tx(
        &mut *tx,
        ctx.tenant,
        input.programme_id,
        input.include_retired,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(benefit_out).collect(),
        next_cursor: None,
    })
}

/// Link a benefit into the benefits map; all five levels are mandatory.
#[crate::action(
    name = "benefits.link",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_link(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitLinkIn,
) -> Result<HandlerOutput<BenefitLinkOut>, ActionError> {
    let kind = input.kind.unwrap_or_else(|| "benefit".to_owned());
    if !matches!(kind.as_str(), "benefit" | "disbenefit") {
        return Err(invalid(
            "kind",
            "unknown_kind",
            "kind must be benefit or disbenefit",
        ));
    }
    let record = store::spine::insert_benefit_link_tx(
        &mut *tx,
        ctx.tenant,
        &NewBenefitLink {
            programme_id: input.programme_id,
            output_ref: input.output_ref,
            capability_ref: input.capability_ref,
            operational_outcome: input.operational_outcome,
            benefit_id: input.benefit_id,
            strategic_objective: input.strategic_objective,
            kind,
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "benefit_link", record.id, "created").await?;
    Ok(HandlerOutput::created(BenefitLinkOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        output_ref: record.output_ref,
        capability_ref: record.capability_ref,
        operational_outcome: record.operational_outcome,
        benefit_id: record.benefit_id.to_string(),
        strategic_objective: record.strategic_objective,
        kind: record.kind,
        version: record.version,
    })
    .with_etag(etag_of(record.version)))
}

/// Edit a benefit profile; the merged measurement record is re-validated.
#[crate::action(
    name = "benefits.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitUpdateIn,
) -> Result<HandlerOutput<BenefitOut>, ActionError> {
    if let Some(status) = input.status.as_deref()
        && !matches!(
            status,
            "draft" | "active" | "realized" | "disbenefit" | "closed"
        )
    {
        return Err(invalid(
            "status",
            "unknown_status",
            "status must be draft, active, realized, disbenefit or closed",
        ));
    }
    let reference = match input.reference {
        Some(reference) => {
            let trimmed = reference.trim().to_owned();
            if trimmed.is_empty() {
                return Err(invalid("reference", "required", "a reference is required"));
            }
            Some(trimmed)
        }
        None => None,
    };
    let current = store::spine::get_benefit_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such benefit profile"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let patch = store::spine::BenefitPatch {
        project_id: input.project_id,
        register_item_id: input.register_item_id,
        reference,
        description: trimmed(input.description),
        category: trimmed(input.category),
        objectives: input.objectives,
        observable_outcome: trimmed(input.observable_outcome),
        kpis: input.kpis,
        baseline: input.baseline,
        target: input.target,
        measurement_method: trimmed(input.measurement_method),
        financial_value: trimmed(input.financial_value),
        cost: input.cost,
        capabilities: input.capabilities,
        preconditions: input.preconditions,
        risks: input.risks,
        owner_job_id: input.owner_job_id,
        benefit_owner_job_id: input.benefit_owner_job_id,
        operational_area: trimmed(input.operational_area),
        status: input.status,
    };
    if patch.is_empty() && input.change_id.is_none() {
        return Err(invalid(
            "description",
            "invalid_value",
            "nothing to update: supply a field or a change_id",
        ));
    }
    let record = store::spine::update_benefit_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        &patch,
        input.change_id,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "benefit_profile", record.id, "updated").await?;
    Ok(HandlerOutput::ok(benefit_out(&record)).with_etag(etag_of(record.version)))
}

/// `benefits.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_benefits_delete(input: &IdIn) -> Result<String, String> {
    Ok(format!("benefit/{}", input.id))
}

/// Remove a benefit profile; its links and transition cascade.
#[crate::action(
    name = "benefits.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_benefits_delete",
)]
async fn benefits_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_benefit_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such benefit profile"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    store::spine::delete_benefit_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "benefit_profile", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}

/// List a programme's benefits-map edges.
#[crate::action(
    name = "benefits.link_list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_link_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitLinkListIn,
) -> Result<Page<BenefitLinkOut>, ActionError> {
    let records = store::spine::list_benefit_links_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(benefit_link_out).collect(),
        next_cursor: None,
    })
}

/// `benefits.unlink`'s approval target: what the gate authorises, computed from the input alone.
fn target_benefits_unlink(input: &IdIn) -> Result<String, String> {
    Ok(format!("benefit/{}/unlink", input.id))
}

/// Remove one benefits-map edge.
#[crate::action(
    name = "benefits.unlink",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_benefits_unlink",
)]
async fn benefits_unlink(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_benefit_link_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such benefits-map edge"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    store::spine::delete_benefit_link_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "benefit_link", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}

/// List a programme's benefit realization records.
#[crate::action(
    name = "benefits.transition_list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_transition_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitTransitionListIn,
) -> Result<Page<BenefitTransitionOut>, ActionError> {
    let records = store::spine::list_transitions_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(benefit_transition_out).collect(),
        next_cursor: None,
    })
}

/// Create or correct a benefit's realization record.
#[crate::action(
    name = "benefits.transition_upsert",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_transition_upsert(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitTransitionUpsertIn,
) -> Result<HandlerOutput<BenefitTransitionOut>, ActionError> {
    let current = store::spine::get_transition_tx(&mut *tx, ctx.tenant, input.benefit_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    let expected = match &current {
        Some(current) => {
            let expected = require_precondition_with(
                &ctx,
                ctx.action,
                serde_json::json!({ "version": current.version }),
            )?;
            Some(i64::try_from(expected.get()).unwrap_or(i64::MAX))
        }
        None => None,
    };
    let record = store::spine::upsert_transition_tx(
        &mut *tx,
        ctx.tenant,
        input.benefit_id,
        expected,
        &TransitionPatch {
            baseline_measured_at: input.baseline_measured_at,
            readiness: input.readiness,
            enacted_at: input.enacted_at,
            post_measured_at: input.post_measured_at,
            retired_at: input.retired_at,
            evidence: input.evidence,
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "benefit_transition",
        record.id,
        if current.is_some() {
            "updated"
        } else {
            "created"
        },
    )
    .await?;
    Ok(HandlerOutput::ok(benefit_transition_out(&record)).with_etag(etag_of(record.version)))
}

/// `benefits.retire`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitRetireIn {
    /// The profile.
    pub id: Uuid,
    /// Why it left the map. Required; retirement is never a delete.
    pub reason: String,
}

#[crate::action(
    name = "benefits.retire",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benefits_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BenefitRetireIn,
) -> Result<HandlerOutput<BenefitOut>, ActionError> {
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(invalid("reason", "required", "a retirement needs a reason"));
    }
    let current = store::spine::get_benefit_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such benefit profile"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let record = store::spine::retire_benefit_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        reason,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "benefit_profile", record.id, "retired").await?;
    Ok(HandlerOutput::ok(benefit_out(&record)).with_etag(etag_of(record.version)))
}

/// `tranches.retire`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheRetireIn {
    /// The tranche.
    pub id: Uuid,
    /// Why it left the plan. Required; retirement is never a delete.
    pub reason: String,
}

#[crate::action(
    name = "tranches.retire",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tranches_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TrancheRetireIn,
) -> Result<HandlerOutput<TrancheOut>, ActionError> {
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(invalid("reason", "required", "a retirement needs a reason"));
    }
    let current = store::spine::get_tranche_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such tranche"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let record = store::spine::retire_tranche_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        reason,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_tranche", record.id, "retired").await?;
    Ok(HandlerOutput::ok(tranche_out(&record)).with_etag(etag_of(record.version)))
}

// ── cadence and stakeholder views ───────────────────────────────────────────────────────

/// A cadence row as `cadences.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CadenceOut {
    /// The row id.
    pub id: String,
    /// The programme.
    pub programme_id: String,
    /// `pi | sprint | demo | retro | refinement`.
    pub kind: String,
    /// The period label.
    pub period: String,
    /// The state.
    pub state: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
}

fn cadence_out(record: &CadenceRecord) -> CadenceOut {
    CadenceOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        kind: record.kind.clone(),
        period: record.period.clone(),
        state: record.state.clone(),
        meta: record.meta.clone(),
        version: record.version,
    }
}

/// `cadences.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CadenceCreateIn {
    /// The programme.
    pub programme_id: Uuid,
    /// `pi | sprint | demo | retro | refinement`.
    pub kind: String,
    /// The period label (e.g. `PI-2027.1`).
    pub period: String,
    /// Free-form metadata. A cadence that belongs to a programme tranche names it here
    /// (`tranche_id`/`tranche_number`): `period` is a free-text label, so `meta` is the link
    /// the conformance audit and the project verifier read.
    pub meta: Option<Value>,
}

/// `cadences.update`; `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CadenceUpdateIn {
    /// The cadence row.
    pub id: Uuid,
    /// The period label.
    pub period: Option<String>,
    /// The state.
    pub state: Option<String>,
    /// Free-form metadata.
    pub meta: Option<Value>,
}

/// `cadences.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CadenceListIn {
    /// The programme whose cadence rows are read.
    pub programme_id: Uuid,
}

/// A stakeholder profile as `stakeholders.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderOut {
    /// The row id.
    pub id: String,
    /// The programme.
    pub programme_id: String,
    /// The project, if project-scoped.
    pub project_id: Option<String>,
    /// The name.
    pub name: String,
    /// Concerns.
    pub concerns: String,
    /// Sensitivities.
    pub sensitivities: String,
    /// Importance.
    pub importance: String,
    /// Influence.
    pub influence: String,
    /// Interest.
    pub interest: String,
    /// Exposure.
    pub exposure: String,
    /// Support level.
    pub support_level: String,
    /// Support trend.
    pub support_trend: String,
    /// Engagement target.
    pub engagement_target: String,
    /// Benefits distributed to the group.
    pub benefits: Value,
    /// Disbenefits distributed to the group.
    pub disbenefits: Value,
    /// The If-Match version.
    pub version: i64,
}

fn stakeholder_out(record: &StakeholderRecord) -> StakeholderOut {
    StakeholderOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        project_id: record.project_id.map(|id| id.to_string()),
        name: record.name.clone(),
        concerns: record.concerns.clone(),
        sensitivities: record.sensitivities.clone(),
        importance: record.importance.clone(),
        influence: record.influence.clone(),
        interest: record.interest.clone(),
        exposure: record.exposure.clone(),
        support_level: record.support_level.clone(),
        support_trend: record.support_trend.clone(),
        engagement_target: record.engagement_target.clone(),
        benefits: record.benefits.clone(),
        disbenefits: record.disbenefits.clone(),
        version: record.version,
    }
}

/// `stakeholders.profile_create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderCreateIn {
    /// The programme.
    pub programme_id: Uuid,
    /// The project, if project-scoped.
    pub project_id: Option<Uuid>,
    /// The name.
    pub name: String,
    /// Concerns.
    pub concerns: Option<String>,
    /// Sensitivities.
    pub sensitivities: Option<String>,
    /// Importance.
    pub importance: Option<String>,
    /// Influence.
    pub influence: Option<String>,
    /// Interest.
    pub interest: Option<String>,
    /// Exposure.
    pub exposure: Option<String>,
    /// Support level.
    pub support_level: Option<String>,
    /// Support trend.
    pub support_trend: Option<String>,
    /// Engagement target.
    pub engagement_target: Option<String>,
    /// Benefits distributed to the group.
    pub benefits: Option<Value>,
    /// Disbenefits distributed to the group.
    pub disbenefits: Option<Value>,
}

/// `stakeholders.profile_update`; `If-Match` is required.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderUpdateIn {
    /// The profile to edit.
    pub id: Uuid,
    /// The name.
    pub name: Option<String>,
    /// The new concerns.
    pub concerns: Option<String>,
    /// The new sensitivities.
    pub sensitivities: Option<String>,
    /// The new importance.
    pub importance: Option<String>,
    /// The new influence.
    pub influence: Option<String>,
    /// The new interest.
    pub interest: Option<String>,
    /// The new exposure.
    pub exposure: Option<String>,
    /// The new support level.
    pub support_level: Option<String>,
    /// The new support trend.
    pub support_trend: Option<String>,
    /// The new engagement target.
    pub engagement_target: Option<String>,
    /// The benefits distributed to the group.
    pub benefits: Option<Value>,
    /// The disbenefits distributed to the group.
    pub disbenefits: Option<Value>,
}

/// `stakeholders.profile_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderListIn {
    /// The programme whose profiles are read.
    pub programme_id: Uuid,
}

/// Create a cadence row.
#[crate::action(
    name = "cadences.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cadences_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CadenceCreateIn,
) -> Result<HandlerOutput<CadenceOut>, ActionError> {
    let kind = input.kind.trim().to_owned();
    if !matches!(
        kind.as_str(),
        "pi" | "sprint" | "demo" | "retro" | "refinement"
    ) {
        return Err(invalid(
            "kind",
            "unknown_kind",
            "kind must be pi, sprint, demo, retro or refinement",
        ));
    }
    let period = input.period.trim().to_owned();
    if period.is_empty() {
        return Err(invalid("period", "required", "a period label is required"));
    }
    let record = store::spine::insert_cadence_tx(
        &mut *tx,
        ctx.tenant,
        input.programme_id,
        &kind,
        &period,
        &input.meta.unwrap_or_else(|| serde_json::json!({})),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_cadence", record.id, "created").await?;
    Ok(HandlerOutput::created(cadence_out(&record)).with_etag(etag_of(record.version)))
}

/// List a programme's cadence rows.
#[crate::action(
    name = "cadences.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cadences_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CadenceListIn,
) -> Result<Page<CadenceOut>, ActionError> {
    let records = store::spine::list_cadences_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(cadence_out).collect(),
        next_cursor: None,
    })
}

/// Edit a cadence row.
#[crate::action(
    name = "cadences.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cadences_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CadenceUpdateIn,
) -> Result<HandlerOutput<CadenceOut>, ActionError> {
    let current = store::spine::get_cadence_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such cadence row"))?;
    // A retro closes through `retros.close`, which writes each finding's lesson and refuses a
    // finding with no lesson content. An edit that sets `closed` directly would bypass that gate.
    if current.kind == "retro" && input.state.as_deref() == Some("closed") {
        return Err(invalid(
            "state",
            "retro_close_required",
            "a retro closes through retros.close, which writes every finding's lesson",
        ));
    }
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let period = match input.period {
        Some(period) => {
            let trimmed = period.trim().to_owned();
            if trimmed.is_empty() {
                return Err(invalid("period", "required", "a period label is required"));
            }
            Some(trimmed)
        }
        None => None,
    };
    let record = store::spine::update_cadence_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        &CadencePatch {
            period,
            state: input.state,
            meta: input.meta,
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_cadence", record.id, "updated").await?;
    Ok(HandlerOutput::ok(cadence_out(&record)).with_etag(etag_of(record.version)))
}

/// `cadences.approve_plan` — the gate that stands between a PI plan and its cards.
///
/// `decision` absent opens (or returns) the plan's pending `approval`; `approve`/`reject`
/// records a **person's** decision on it. Nothing materialises until the gate carries a human
/// approve, so the unattended cadence tick cannot commit an unapproved plan.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CadenceApprovePlanIn {
    /// The PI cadence whose materialisation is gated.
    pub cadence_id: Uuid,
    /// `approve` or `reject`. Absent opens the gate and leaves it pending.
    pub decision: Option<String>,
    /// The note the deciding person records on the gate.
    pub note: Option<String>,
}

/// A PI plan's gate as `cadences.approve_plan` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CadencePlanApprovalOut {
    /// The cadence the gate belongs to.
    pub cadence_id: String,
    /// The approval row.
    pub approval_id: String,
    /// `pending | approved | rejected`.
    pub status: String,
    /// The decision this call recorded; absent when the call only opened the gate.
    pub decision: Option<String>,
    /// The cadence state after the call (`awaiting_approval` or `rejected`).
    pub state: String,
    /// The approval's If-Match version.
    pub version: i64,
}

fn plan_approval_out(
    cadence_id: Uuid,
    approval: &conveyor_domain::Approval,
    decision: Option<&str>,
    state: &str,
) -> CadencePlanApprovalOut {
    CadencePlanApprovalOut {
        cadence_id: cadence_id.to_string(),
        approval_id: approval.id.to_string(),
        status: approval.status.as_str().to_owned(),
        decision: decision.map(str::to_owned),
        state: state.to_owned(),
        version: i64::try_from(approval.version.get()).unwrap_or(i64::MAX),
    }
}

/// Open, approve or reject a PI plan's gate.
#[crate::action(
    name = "cadences.approve_plan",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cadences_approve_plan(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CadenceApprovePlanIn,
) -> Result<HandlerOutput<CadencePlanApprovalOut>, ActionError> {
    let current = store::spine::get_cadence_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such cadence row"))?;
    if current.kind != "pi" {
        return Err(invalid(
            "cadence_id",
            "cadence_wrong_kind",
            format!(
                "the cadence is {}; a plan approval needs a pi cadence",
                current.kind
            ),
        ));
    }
    if current.state == "materialised" {
        return Err(ActionError::new(
            409,
            "plan_already_materialised",
            "the plan's cards are already drafted; approval is closed",
        ));
    }
    let requested = match input.decision.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("approve") => Some(DecisionKind::Approve),
        Some("reject") => Some(DecisionKind::Reject),
        Some(other) => {
            return Err(invalid(
                "decision",
                "invalid_value",
                format!("{other:?} is not approve or reject"),
            ));
        }
    };
    let Some(kind) = requested else {
        // Opening the gate is a request, never an approval. The cadence is marked waiting so
        // the console and the worker read one state; an existing pending gate is reused, so a
        // retry stacks no second gate.
        if store::spine::pi_plan_human_approval_tx(&mut *tx, ctx.tenant, input.cadence_id)
            .await
            .map_err(|error| spine_error(error, ctx.action))?
            .is_some()
        {
            return Err(ActionError::new(
                409,
                "plan_already_approved",
                "a person already approved this plan; there is nothing left to request",
            ));
        }
        let approval = store::spine::open_pi_plan_approval_tx(
            &mut *tx,
            ctx.tenant,
            input.cadence_id,
            Some(ctx.actor_id),
        )
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
        store::spine::set_pi_cadence_approval_state_tx(
            &mut *tx,
            ctx.tenant,
            input.cadence_id,
            store::spine::CADENCE_AWAITING_APPROVAL,
        )
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
        emit_spine_event(
            &mut *tx,
            &ctx,
            "programme_cadence",
            input.cadence_id,
            "updated",
        )
        .await?;
        return Ok(HandlerOutput::ok(plan_approval_out(
            input.cadence_id,
            &approval,
            None,
            store::spine::CADENCE_AWAITING_APPROVAL,
        )));
    };
    // A decision is a person's act. The store's human-decision check is the backstop; the
    // refusal belongs here so a machine caller learns why nothing happened.
    if ctx.actor_kind != "user" || ctx.channel.is_some() {
        return Err(ActionError::new(
            403,
            "human_only",
            "a PI plan is approved by a person; a machine identity cannot decide it",
        ));
    }
    let existing = match kind {
        DecisionKind::Approve => {
            store::spine::pi_plan_human_approval_tx(&mut *tx, ctx.tenant, input.cadence_id)
                .await
                .map_err(|error| spine_error(error, ctx.action))?
        }
        DecisionKind::Reject => None,
    };
    if let Some(existing) = existing {
        let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, existing)
            .await
            .map_err(|error| plan_approval_error(error, ctx.action))?
            .ok_or_else(ActionError::internal)?;
        return Ok(HandlerOutput::ok(plan_approval_out(
            input.cadence_id,
            &record.approval,
            Some("approve"),
            &current.state,
        )));
    }
    let approval =
        store::spine::open_pi_plan_approval_tx(&mut *tx, ctx.tenant, input.cadence_id, None)
            .await
            .map_err(|error| spine_error(error, ctx.action))?;
    let decision = Decision {
        approver_id: ctx.actor_id,
        kind,
        note: input.note.unwrap_or_default(),
        decided_by_kind: DeciderKind::Human,
        via: Some(serde_json::json!({ "channel": "web" })),
        item_decisions: None,
    };
    let decided =
        store::approval::record_decision_tx(&mut *tx, ctx.tenant, approval.id, &decision, false)
            .await
            .map_err(|error| plan_approval_error(error, ctx.action))?;
    let state = match kind {
        DecisionKind::Approve => store::spine::CADENCE_AWAITING_APPROVAL,
        DecisionKind::Reject => store::spine::CADENCE_REJECTED,
    };
    store::spine::set_pi_cadence_approval_state_tx(&mut *tx, ctx.tenant, input.cadence_id, state)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "programme_cadence",
        input.cadence_id,
        "updated",
    )
    .await?;
    let recorded = match kind {
        DecisionKind::Approve => "approve",
        DecisionKind::Reject => "reject",
    };
    Ok(HandlerOutput::ok(plan_approval_out(
        input.cadence_id,
        &decided,
        Some(recorded),
        state,
    )))
}

/// `cadences.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_cadences_delete(input: &IdIn) -> Result<String, String> {
    Ok(format!("cadence/{}", input.id))
}

/// Remove a cadence row.
#[crate::action(
    name = "cadences.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_cadences_delete",
)]
async fn cadences_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_cadence_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such cadence row"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    store::spine::delete_cadence_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "programme_cadence", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}

// ── demo credits and lessons ────────────────────────────────────────────────────────────

/// A demo credit as `cadences.credit_*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DemoCreditOut {
    /// The credit id.
    pub id: String,
    /// The demo cadence the credit was recorded at.
    pub cadence_id: String,
    /// The accepted deliverable the credit names.
    pub deliverable_id: String,
    /// The deliverable's evidence, copied when the credit was recorded.
    pub evidence: Value,
    /// When the credit was recorded.
    pub credited_at: DateTime<Utc>,
    /// The If-Match version.
    pub version: i64,
}

fn demo_credit_out(record: &DemoCreditRecord) -> DemoCreditOut {
    DemoCreditOut {
        id: record.id.to_string(),
        cadence_id: record.cadence_id.to_string(),
        deliverable_id: record.deliverable_id.to_string(),
        evidence: record.evidence.clone(),
        credited_at: record.credited_at,
        version: record.version,
    }
}

/// `cadences.credit_demo`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreditDemoIn {
    /// The demo cadence.
    pub cadence_id: Uuid,
    /// The completed deliverable.
    pub deliverable_id: Uuid,
}

/// `cadences.credit_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreditListIn {
    /// The demo cadence whose credits are read.
    pub cadence_id: Uuid,
}

/// Record a demo credit; a repeat returns the credit already recorded.
#[crate::action(
    name = "cadences.credit_demo",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cadences_credit_demo(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CreditDemoIn,
) -> Result<HandlerOutput<DemoCreditOut>, ActionError> {
    let existing = store::spine::find_demo_credit_tx(
        &mut *tx,
        ctx.tenant,
        input.cadence_id,
        input.deliverable_id,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    if let Some(existing) = existing {
        return Ok(
            HandlerOutput::ok(demo_credit_out(&existing)).with_etag(etag_of(existing.version))
        );
    }
    let id =
        store::spine::credit_demo_tx(&mut *tx, ctx.tenant, input.cadence_id, input.deliverable_id)
            .await
            .map_err(|error| spine_error(error, ctx.action))?;
    let record = store::spine::get_demo_credit_tx(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(ActionError::internal)?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "programme_demo_credit",
        record.id,
        "created",
    )
    .await?;
    Ok(HandlerOutput::created(demo_credit_out(&record)).with_etag(etag_of(record.version)))
}

/// List a demo cadence's credits.
#[crate::action(
    name = "cadences.credit_list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cadences_credit_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CreditListIn,
) -> Result<Page<DemoCreditOut>, ActionError> {
    let records = store::spine::list_demo_credits_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(demo_credit_out).collect(),
        next_cursor: None,
    })
}

/// The findings the coach gathered on a retro.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RetroFindingsIn {
    /// The retro cadence.
    pub cadence_id: Uuid,
    /// The findings: one object per finding, each carrying its `key`.
    pub findings: Value,
}

/// Close a retro.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RetroCloseIn {
    /// The retro cadence.
    pub cadence_id: Uuid,
}

/// Record a retro's findings on its coach card.
///
/// The retro was openable only by the worker's tick and closeable only by a store call: the
/// inspect-and-adapt step had no API. The findings live on the retro card, and closing checks
/// one lesson per finding key, so a retro cannot be closed over an empty lesson set.
#[crate::action(
    name = "retros.record_findings",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn retros_record_findings(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RetroFindingsIn,
) -> Result<HandlerOutput<CadenceOut>, ActionError> {
    let cadence = store::spine::get_cadence_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such cadence row"))?;
    if cadence.kind != "retro" {
        return Err(invalid(
            "cadence_id",
            "cadence_wrong_kind",
            "the findings need a retro cadence",
        ));
    }
    // Recording findings opens the retro's one coach card first: the row is the retro's own
    // record, and a caller with a cadence but no tick yet should not have to know about the card.
    store::spine::open_retro_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    store::spine::record_retro_findings_tx(&mut *tx, ctx.tenant, input.cadence_id, &input.findings)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "programme_cadence",
        input.cadence_id,
        "findings_recorded",
    )
    .await?;
    let record = store::spine::get_cadence_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(ActionError::internal)?;
    Ok(HandlerOutput::ok(cadence_out(&record)).with_etag(etag_of(record.version)))
}

/// Close a retro once every recorded finding has its lesson.
#[crate::action(
    name = "retros.close",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn retros_close(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RetroCloseIn,
) -> Result<HandlerOutput<CadenceOut>, ActionError> {
    let cadence = store::spine::get_cadence_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such cadence row"))?;
    if cadence.kind != "retro" {
        return Err(invalid(
            "cadence_id",
            "cadence_wrong_kind",
            "a retro close needs a retro cadence",
        ));
    }
    store::spine::close_retro_tx(&mut *tx, ctx.tenant, input.cadence_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    let patch = store::spine::CadencePatch {
        period: None,
        state: Some("closed".to_owned()),
        meta: None,
    };
    let record = store::spine::update_cadence_tx(
        &mut *tx,
        ctx.tenant,
        input.cadence_id,
        cadence.version,
        &patch,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(
        &mut *tx,
        &ctx,
        "programme_cadence",
        input.cadence_id,
        "closed",
    )
    .await?;
    Ok(HandlerOutput::ok(cadence_out(&record)).with_etag(etag_of(record.version)))
}

/// A lesson as `lessons.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonOut {
    /// The row id.
    pub id: String,
    /// The programme whose memory holds the lesson.
    pub programme_id: String,
    /// The retro cadence the finding came from, when known.
    pub cadence_id: Option<String>,
    /// The finding's key within the retro.
    pub finding_key: String,
    /// The lesson itself.
    pub statement: String,
    /// The evidence refs behind the lesson.
    pub evidence: Value,
    /// How many of the refs are independent.
    pub independent_count: i32,
    /// `proposed | reviewed | promoted | rejected`.
    pub state: String,
    /// Where the lesson came from.
    pub source: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

fn lesson_out(record: &LessonRecord) -> LessonOut {
    LessonOut {
        id: record.id.to_string(),
        programme_id: record.programme_id.to_string(),
        cadence_id: record.cadence_id.map(|id| id.to_string()),
        finding_key: record.finding_key.clone(),
        statement: record.statement.clone(),
        evidence: record.evidence.clone(),
        independent_count: record.independent_count,
        state: record.state.clone(),
        source: record.source.clone(),
        meta: record.meta.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// `lessons.propose`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonProposeIn {
    /// The programme whose memory receives the lesson.
    pub programme_id: Uuid,
    /// The retro cadence the finding came from, when known.
    pub cadence_id: Option<Uuid>,
    /// The finding's key within the retro.
    pub finding_key: String,
    /// The lesson itself.
    pub statement: String,
    /// The evidence refs behind the lesson.
    pub evidence: Value,
}

/// `lessons.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonListIn {
    /// The programme whose lessons are read.
    pub programme_id: Uuid,
}

/// Propose a lesson for a retro finding.
#[crate::action(
    name = "lessons.propose",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lessons_propose(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LessonProposeIn,
) -> Result<HandlerOutput<LessonOut>, ActionError> {
    let statement = input.statement.trim().to_owned();
    if statement.is_empty() {
        return Err(invalid(
            "statement",
            "required",
            "a lesson needs a statement",
        ));
    }
    if !input.evidence.is_array() {
        return Err(invalid(
            "evidence",
            "array_required",
            "evidence must be an array of refs",
        ));
    }
    if let Some(cadence_id) = input.cadence_id {
        store::spine::get_cadence_tx(&mut *tx, ctx.tenant, cadence_id)
            .await
            .map_err(|error| spine_error(error, ctx.action))?
            .ok_or_else(|| ActionError::resource_not_found("such cadence row"))?;
    }
    let id = store::spine::propose_lesson_tx(
        &mut *tx,
        ctx.tenant,
        input.programme_id,
        input.cadence_id,
        input.finding_key.trim(),
        &statement,
        &input.evidence,
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    let record = store::spine::get_lesson_tx(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(ActionError::internal)?;
    emit_spine_event(&mut *tx, &ctx, "lesson", record.id, "created").await?;
    Ok(HandlerOutput::created(lesson_out(&record)).with_etag(etag_of(record.version)))
}

/// List a programme's lessons.
#[crate::action(
    name = "lessons.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lessons_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LessonListIn,
) -> Result<Page<LessonOut>, ActionError> {
    let records = store::spine::list_lessons_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(lesson_out).collect(),
        next_cursor: None,
    })
}

/// Create a stakeholder profile.
#[crate::action(
    name = "stakeholders.profile_create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_profile_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StakeholderCreateIn,
) -> Result<HandlerOutput<StakeholderOut>, ActionError> {
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        return Err(invalid(
            "name",
            "required",
            "a stakeholder name is required",
        ));
    }
    let record = store::spine::insert_stakeholder_tx(
        &mut *tx,
        ctx.tenant,
        &NewStakeholderProfile {
            programme_id: input.programme_id,
            project_id: input.project_id,
            name,
            concerns: input.concerns.unwrap_or_default(),
            sensitivities: input.sensitivities.unwrap_or_default(),
            importance: input.importance.unwrap_or_default(),
            influence: input.influence.unwrap_or_default(),
            interest: input.interest.unwrap_or_default(),
            exposure: input.exposure.unwrap_or_default(),
            support_level: input.support_level.unwrap_or_default(),
            support_trend: input.support_trend.unwrap_or_default(),
            engagement_target: input.engagement_target.unwrap_or_default(),
            benefits: input.benefits.unwrap_or_else(|| serde_json::json!([])),
            disbenefits: input.disbenefits.unwrap_or_else(|| serde_json::json!([])),
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "stakeholder_profile", record.id, "created").await?;
    Ok(HandlerOutput::created(stakeholder_out(&record)).with_etag(etag_of(record.version)))
}

/// Edit a stakeholder profile; `If-Match` is required.
#[crate::action(
    name = "stakeholders.profile_update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_profile_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StakeholderUpdateIn,
) -> Result<HandlerOutput<StakeholderOut>, ActionError> {
    let current = store::spine::get_stakeholder_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such stakeholder profile"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let record = store::spine::update_stakeholder_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        &StakeholderPatch {
            name: input.name,
            concerns: input.concerns,
            sensitivities: input.sensitivities,
            importance: input.importance,
            influence: input.influence,
            interest: input.interest,
            exposure: input.exposure,
            support_level: input.support_level,
            support_trend: input.support_trend,
            engagement_target: input.engagement_target,
            benefits: input.benefits,
            disbenefits: input.disbenefits,
        },
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "stakeholder_profile", record.id, "updated").await?;
    Ok(HandlerOutput::ok(stakeholder_out(&record)).with_etag(etag_of(record.version)))
}

/// List a programme's stakeholder profiles.
#[crate::action(
    name = "stakeholders.profile_list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_profile_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StakeholderListIn,
) -> Result<Page<StakeholderOut>, ActionError> {
    let records = store::spine::list_stakeholders_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(stakeholder_out).collect(),
        next_cursor: None,
    })
}

/// `stakeholders.profile_delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_stakeholders_profile_delete(input: &IdIn) -> Result<String, String> {
    Ok(format!("stakeholder/{}/profile", input.id))
}

/// Whether a WBS patch (or the stored row) carries evidence a person can read.
fn carries_evidence(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(map) => !map.is_empty(),
        serde_json::Value::String(text) => !text.trim().is_empty(),
        serde_json::Value::Null => false,
        _ => true,
    }
}

/// Remove a stakeholder profile.
#[crate::action(
    name = "stakeholders.profile_delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_stakeholders_profile_delete",
)]
async fn stakeholders_profile_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let current = store::spine::get_stakeholder_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| spine_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such stakeholder profile"))?;
    let expected = require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    store::spine::delete_stakeholder_profile_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| spine_error(error, ctx.action))?;
    emit_spine_event(&mut *tx, &ctx, "stakeholder_profile", input.id, "deleted").await?;
    Ok(HandlerOutput::no_content(()))
}
