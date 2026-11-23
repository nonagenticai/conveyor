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
use conveyor_domain::spine::{Measurement, PlanError, WsjfInputs, wsjf};
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
    DependencyRecord, InFlightRecord, NewBenefitLink, NewBenefitProfile, NewRegisterItem,
    NewStakeholderProfile, NewWbsNode, PlanStoreError, RegisterRecord, StakeholderPatch,
    StakeholderRecord, TranchePatch, TrancheRecord, TrancheReviewRecord, TransitionPatch,
    TransitionRecord, WbsPatch, WbsRecord,
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
        PlanStoreError::Illegal(PlanError::BenefitMeasurementInvalid { field }) => invalid(
            field,
            "benefit_measurement_invalid",
            "a measurement is a number and a unit; prose, blanks and placeholder units are not \
             measurements",
        ),
        PlanStoreError::Illegal(PlanError::BenefitRealizationUnmeasured) => invalid(
            "post_value",
            "benefit_realization_unmeasured",
            "a realized benefit needs a measured baseline value, a measured post value and \
             evidence; two timestamps are not a measurement",
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
    /// The output level.
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
fn target_wbs_delete(input: &IdIn) -> Result<String, String> {
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
    input: IdIn,
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
/// through an approved change request that names it. The change row and the new baseline cite
/// each other (`change_id` on the baseline, `meta.baseline_id` on the change).
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
        if change.kind != "change"
            || change.status != "approved"
            || change.baseline_ref != Some(previous.id)
        {
            return Err(ActionError::new(
                409,
                "change_request_invalid",
                format!(
                    "change {change_id} must be an approved change whose baseline_ref is the \
                     approved baseline {}",
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
) -> Result<Page<InFlightOut>, ActionError> {
    let records = store::spine::in_flight_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| spine_error(error, ctx.action))?;
    Ok(Page {
        items: records
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
    let evidence = match input.evidence {
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
    /// Free-form metadata.
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
