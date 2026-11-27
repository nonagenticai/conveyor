#![forbid(unsafe_code)]
//! Development projects — the self-development path (SPEC_V2 §5.2, §5.4, §6.4, §9.1, §9.3,
//! §12 T6; issue #80).
//!
//! A development project is an ordinary `project` row with a `development` type and a stricter
//! gate set. This family records what makes that path auditable:
//!
//! - [`development_projects.charter`] — one repo, one lane, the implementer and the gate set the
//!   project is judged by, written before any branch;
//! - [`development_projects.record_step`] — one immutable evidence row per step, in §9.1's order:
//!   branch → PR → CI → merge → rollback plan → deploy → live check. A predecessor that is
//!   missing is a refusal, never a skipped step;
//! - [`development_projects.review_gate`], [`.merge_gate`], [`.deploy_gate`] — the three gates,
//!   as ordinary `approval` rows of kind `development`. The implementing principal can never
//!   decide them (the store's one decision writer refuses that);
//! - [`development_projects.merge_gate`] reads the tenant's `approval_policy` row for the earned
//!   auto-merge decision (§6.4): the approver kind, the typed conditions and the expiry are
//!   data, never constants. A red CI verdict is a refusal, never a retry;
//! - [`development_projects.deploy_gate`] refuses a deploy with no tested rollback plan (§9.3).
//!   Every migration must be expand/contract (backward-compatible with the immediately prior
//!   release); a plan that declares the deploy's migrations non-additive must name the restore
//!   path, and an application-only rollback is then refused (issue #669);
//! - [`development_projects.close`] writes the closure record linking the lane, PR, CI run,
//!   reviewer, merge SHA, deployed digest, applied migration generation and live check;
//! - [`development_projects.rollback`] records an exercised rollback (the prior digest serves
//!   again), naming both digests and the applied migration generation at deploy time — the
//!   evidence P3.2's live scenario names;
//! - [`development_projects.change_judging_surface`] is the project's attempt to change the
//!   gates, policy or instrument that judge it: always refused, and recorded by the invoke
//!   layer's refusal audit (§9.3).
//!
//! Writes declare `projects:write`; opening a gate also declares `approvals:write`. The read
//! declares `projects:read`.

use chrono::{DateTime, Utc};
use conveyor_domain::development::{
    ApproverKind, AutoMergeConditions, AutoMergeInputs, CiVerdict, JUDGING_SURFACES, Step,
    auto_merge_in_force, validate_evidence,
};
use conveyor_store as store;
use conveyor_store::approval::ApprovalRecord;
use conveyor_store::development::{
    DevelopmentClosureRow, DevelopmentProjectRow, DevelopmentStepRow, DevelopmentStoreError,
    NewDevelopmentClosure, NewDevelopmentProject,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{invalid, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The review gate's name. A person, never the implementer, decides it.
pub const REVIEW_GATE: &str = "development_projects.review";
/// The merge gate's name. It is also the `approval_policy` key for the earned auto-merge row.
pub const MERGE_GATE: &str = "development_projects.merge_gate";
/// The deploy gate's name. §9.3: a separate decision from the merge, always a person's.
pub const DEPLOY_GATE: &str = "development_projects.deploy_gate";
/// The `approval.kind` every development gate carries. The decision writer uses it (together
/// with `meta.development_project_id`) to refuse the implementer.
pub const GATE_KIND: &str = "development";

/// The actions whose `approval_policy` row may carry T6's earned auto-merge terms.
///
/// The review and deploy gates stay human: §9.3 makes the deploy a separate decision, and the
/// reviewer is what makes review independent. Auto-merge is the merge gate only.
pub fn is_governed_gate(name: &str) -> bool {
    name == MERGE_GATE
}

// ── views ────────────────────────────────────────────────────────────────────────────────

/// `development_projects.charter` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentCharterIn {
    /// The project's display name.
    pub title: String,
    /// An explicit project key; derived from the title when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The one repository this project delivers through (`owner/name` or its URL).
    pub repo: String,
    /// The one lane (branch) the project delivers on.
    pub lane: String,
    /// The GitHub issue that carries this unit's intent (`owner/name#N`), when one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_ref: Option<String>,
    /// The implementing principal. It cannot decide the gates that judge its work.
    pub implementer_id: Uuid,
    /// The project's own acceptance command, run at the live check.
    pub acceptance_command: String,
    /// The eval/benchmark instrument that measures the project, when one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_ref: Option<String>,
    /// The gate set the project is judged by. Defaults to review/merge/deploy; it is written
    /// before any branch and the project's own actions never change it (§9.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_set: Option<Vec<String>>,
    /// The engine-derived unit certificate's policy (#837). Absent means the unit carries no
    /// pinned specification and its Definition of done stays advisory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_policy: Option<DevelopmentProofPolicyIn>,
}

/// The unit certificate's policy, as the charter writes it.
///
/// `spec_sha256` is the specification the certificate answers, frozen here: an implementer who
/// could re-pin it would certify against an easier document. `required` selects the Definition of
/// done gate — advisory when false, refusing when true — and only a principal other than the
/// implementer may loosen it (`change_judging_surface`).
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentProofPolicyIn {
    /// The specification's name (for example `docs/SPEC_V2.md`).
    pub spec_name: String,
    /// That specification's SHA-256. Empty is allowed only while the policy is advisory.
    #[serde(default)]
    pub spec_sha256: String,
    /// Whether the unit's Definition of done requires the certificate.
    #[serde(default)]
    pub required: bool,
}

/// The normalized policy, before the development project row exists.
struct ProofPolicySpec {
    spec_name: String,
    spec_sha256: String,
    required: bool,
}

/// One recorded step.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentStepOut {
    /// `branch | pull_request | ci | merge | rollback_plan | deploy | live_verify |
    /// rollback_exercise`.
    pub step: String,
    /// The step's evidence document, exactly as recorded.
    pub evidence: Value,
    /// Who recorded it.
    pub recorded_by: Option<String>,
    /// When it was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// One development gate as the actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentGateOut {
    /// The gate's name.
    pub gate: String,
    /// The `approval` row id.
    pub approval_id: String,
    /// `pending | approved | rejected | expired | voided`.
    pub status: String,
    /// The quorum.
    pub required_decisions: u16,
    /// Approvals recorded so far.
    pub approvals: u16,
    /// What the gate authorises (the project, gate and head/merge/digest).
    pub target: String,
    /// `human | auto` — read from the tenant's policy row, never from code.
    pub approver_kind: String,
    /// The typed conditions the policy row carries (empty for a human policy).
    pub conditions: Value,
    /// The policy's expiry, when it has one.
    pub expires_at: Option<DateTime<Utc>>,
    /// True when this call resolved the gate under the earned auto-merge policy.
    pub auto_merge: bool,
    /// True when every typed condition was satisfiable from the recorded evidence.
    pub conditions_satisfied: bool,
    /// Why a condition did not hold, when one did not.
    pub reasons: Vec<String>,
    /// True when this call created the gate; false when it returned an existing one.
    pub created: bool,
}

/// The closure record (P1.1): the links the issue's scenarios name.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentClosureOut {
    /// The closure row id.
    pub id: String,
    /// The lane the project delivered on.
    pub lane: String,
    /// The pull request's URL.
    pub pr_url: String,
    /// The CI run that gated the merge.
    pub ci_run_id: String,
    /// Always `green` — a red run never reaches closure.
    pub ci_verdict: String,
    /// The independent reviewer's principal id.
    pub reviewer_id: String,
    /// The reviewer's decision (`approve`).
    pub reviewer_decision: String,
    /// The merge commit.
    pub merge_sha: String,
    /// The image digest that was deployed.
    pub deployed_digest: String,
    /// The live check that passed at that digest.
    pub live_check: Value,
    /// The exercised rollback, when the deployment was rolled back (P3.2).
    pub rollback: Option<Value>,
    /// The rendered closure document.
    pub closure: Value,
    /// When the closure was written.
    pub closed_at: DateTime<Utc>,
}

/// The project as `development_projects.get` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentProjectOut {
    /// The development project row id.
    pub id: String,
    /// The ordinary project row it charters.
    pub project_id: String,
    /// The one repository.
    pub repo: String,
    /// The one lane.
    pub lane: String,
    /// The GitHub issue that carries this unit's intent, when one does.
    pub issue_ref: Option<String>,
    /// The implementing principal.
    pub implementer_id: String,
    /// The gate set the project is judged by.
    pub gate_set: Value,
    /// The measuring instrument, when one applies.
    pub instrument_ref: Option<String>,
    /// The project's own acceptance command.
    pub acceptance_command: String,
    /// `chartered | in_progress | merged | deployed | verified | closed | refused`.
    pub status: String,
    /// Every step recorded, in order.
    pub steps: Vec<DevelopmentStepOut>,
    /// The three gates, when they exist.
    pub gates: Vec<DevelopmentGateOut>,
    /// The closure record, once the project closes.
    pub closure: Option<DevelopmentClosureOut>,
    /// The charter's row version.
    pub version: i64,
    /// When the project was chartered.
    pub created_at: DateTime<Utc>,
    /// When the charter last changed.
    pub updated_at: DateTime<Utc>,
}

/// `development_projects.record_step` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentStepIn {
    /// The development project.
    pub development_project_id: Uuid,
    /// `branch | pull_request | ci | merge | rollback_plan | deploy | live_verify |
    /// rollback_exercise`.
    pub step: String,
    /// The step's evidence (typed by the step; see the module documentation).
    pub evidence: Value,
}

/// `development_projects.review_gate|merge_gate|deploy_gate` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentGateIn {
    /// The development project.
    pub development_project_id: Uuid,
}

/// `development_projects.close` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentCloseIn {
    /// The development project.
    pub development_project_id: Uuid,
}

/// `development_projects.get` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentGetIn {
    /// The development project.
    pub development_project_id: Uuid,
}

/// `development_projects.change_judging_surface` input — the project's own attempt.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentJudgingChangeIn {
    /// The development project whose judging surface would change.
    pub development_project_id: Uuid,
    /// `gate_set | approval_policy | instrument`.
    pub surface: String,
    /// The requested change (recorded nowhere: the refusal is the answer).
    pub change: Value,
}

/// The outcome of a `change_judging_surface` attempt.
///
/// The three original surfaces always refuse — a development project cannot change what judges
/// it. `proof_policy` admits one safe change (`required`), recorded by the wrapper; `refused` is
/// false only when that toggle was applied, and the refused calls remain refusals.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentJudgingChangeOut {
    /// Whether the attempt was refused. False only for an admitted `proof_policy.required`
    /// toggle.
    pub refused: bool,
}

/// `development_projects.rollback` input — an exercised rollback's evidence (P3.2).
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentRollbackIn {
    /// The development project.
    pub development_project_id: Uuid,
    /// The digest the deployed change was serving (what was rolled back from). It must equal the
    /// recorded deploy digest.
    pub deployed_digest: String,
    /// The prior release's digest that serves again after the rollback.
    pub restored_digest: String,
    /// The max applied migration version at deploy time — the schema generation the rollback
    /// names. It must equal the deploy step's recorded generation when that carries one.
    pub migration_generation: i64,
    /// `/health` at the restored digest. P3.2 requires 200.
    pub health_status: u16,
    /// What proves the rollback was exercised (commands, timestamps, links).
    pub evidence: Vec<String>,
}

// ── errors and mappers ───────────────────────────────────────────────────────────────────

fn development_error(error: DevelopmentStoreError, action: &str) -> ActionError {
    match error {
        DevelopmentStoreError::Database(error) => store_error(error, action),
        DevelopmentStoreError::NotFound(id) => {
            ActionError::resource_not_found(&format!("such development project ({id})"))
        }
        DevelopmentStoreError::ProjectAlreadyChartered(project) => ActionError::new(
            409,
            "development_project_exists",
            format!("{action}: project {project} already carries a development charter"),
        ),
        DevelopmentStoreError::IssueAlreadyChartered(issue) => ActionError::new(
            409,
            "issue_already_chartered",
            format!("{action}: issue {issue} already carries a development unit"),
        ),
        DevelopmentStoreError::ImplementerNotFound(id) => {
            ActionError::resource_not_found(&format!("such implementer ({id})"))
        }
        DevelopmentStoreError::StepAlreadyRecorded(step) => ActionError::new(
            409,
            "step_already_recorded",
            format!("{action}: the {step} step is evidence; it is recorded once"),
        ),
        DevelopmentStoreError::StepOutOfOrder { step, missing } => ActionError::new(
            409,
            "step_out_of_order",
            format!(
                "{action}: the {step} step cannot be recorded before {}",
                missing.join(", ")
            ),
        ),
        DevelopmentStoreError::ClosureExists(id) => ActionError::new(
            409,
            "already_closed",
            format!("{action}: development project {id} already has its closure record"),
        ),
        DevelopmentStoreError::ClosureGateMissing(gate) => ActionError::new(
            409,
            "gate_not_approved",
            format!("{action}: the {gate} gate is not approved"),
        ),
        DevelopmentStoreError::ProofPolicyExists(id) => ActionError::new(
            409,
            "proof_policy_exists",
            format!("{action}: development project {id} already carries a proof policy"),
        ),
        DevelopmentStoreError::ProofPolicyMissing(id) => ActionError::new(
            409,
            "proof_policy_missing",
            format!("{action}: development project {id} carries no proof policy"),
        ),
    }
}

fn policy_from_row(row: &DevelopmentProjectRow) -> DevelopmentProjectOut {
    DevelopmentProjectOut {
        id: row.id.to_string(),
        project_id: row.project_id.to_string(),
        repo: row.repo.clone(),
        lane: row.lane.clone(),
        issue_ref: row.issue_ref.clone(),
        implementer_id: row.implementer_id.to_string(),
        gate_set: row.gate_set.clone(),
        instrument_ref: row.instrument_ref.clone(),
        acceptance_command: row.acceptance_command.clone(),
        status: row.status.clone(),
        steps: Vec::new(),
        gates: Vec::new(),
        closure: None,
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn step_out(row: &DevelopmentStepRow) -> DevelopmentStepOut {
    DevelopmentStepOut {
        step: row.step.clone(),
        evidence: row.evidence.clone(),
        recorded_by: row.recorded_by.map(|id| id.to_string()),
        recorded_at: row.created_at,
    }
}

fn closure_out(row: &DevelopmentClosureRow) -> DevelopmentClosureOut {
    let rollback = row
        .closure
        .get("rollback")
        .cloned()
        .filter(|v| !v.is_null());
    DevelopmentClosureOut {
        id: row.id.to_string(),
        lane: row.lane.clone(),
        pr_url: row.pr_url.clone(),
        ci_run_id: row.ci_run_id.clone(),
        ci_verdict: row.ci_verdict.clone(),
        reviewer_id: row.reviewer_id.to_string(),
        reviewer_decision: row
            .closure
            .get("review")
            .and_then(|review| review.get("decision"))
            .and_then(Value::as_str)
            .unwrap_or("approve")
            .to_owned(),
        merge_sha: row.merge_sha.clone(),
        deployed_digest: row.deployed_digest.clone(),
        live_check: row.live_check.clone(),
        rollback,
        closure: row.closure.clone(),
        closed_at: row.created_at,
    }
}

fn gate_out(
    record: &ApprovalRecord,
    policy: &GatePolicy,
    created: bool,
    reasons: Vec<String>,
) -> DevelopmentGateOut {
    DevelopmentGateOut {
        gate: record.approval.gate.clone(),
        approval_id: record.approval.id.to_string(),
        status: record.approval.status.as_str().to_owned(),
        required_decisions: record.approval.required_decisions,
        approvals: record.approval.approvals,
        target: record
            .meta
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        approver_kind: policy.approver_kind.as_str().to_owned(),
        conditions: policy.conditions.clone(),
        expires_at: policy.expires_at,
        auto_merge: policy.auto_merge,
        conditions_satisfied: policy.conditions_satisfied,
        reasons,
        created,
    }
}

/// The evaluated policy for one gate.
struct GatePolicy {
    approver_kind: ApproverKind,
    conditions: Value,
    expires_at: Option<DateTime<Utc>>,
    auto_merge: bool,
    conditions_satisfied: bool,
    reasons: Vec<String>,
}

impl GatePolicy {
    fn human() -> Self {
        Self {
            approver_kind: ApproverKind::Human,
            conditions: json!({}),
            expires_at: None,
            auto_merge: false,
            conditions_satisfied: false,
            reasons: Vec::new(),
        }
    }
}

// ── shared reads ────────────────────────────────────────────────────────────────────────

fn require_project(
    row: Option<DevelopmentProjectRow>,
    id: Uuid,
) -> Result<DevelopmentProjectRow, ActionError> {
    row.ok_or_else(|| ActionError::resource_not_found(&format!("such development project ({id})")))
}

async fn load_project(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    id: Uuid,
) -> Result<DevelopmentProjectRow, ActionError> {
    let row = store::development::get_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| development_error(error, "development_projects.get"))?;
    require_project(row, id)
}

async fn load_step(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    development_project_id: Uuid,
    step: Step,
) -> Result<Option<DevelopmentStepRow>, ActionError> {
    store::development::step_tx(&mut *tx, tenant, development_project_id, step)
        .await
        .map_err(|error| development_error(error, "development_projects"))
}

fn parse_step(raw: &str) -> Result<Step, ActionError> {
    raw.parse::<Step>()
        .map_err(|error| invalid("step", "invalid_value", error.to_string()))
}

/// Refuse a step whose predecessors are missing before any gate or digest check runs, so the
/// caller is told about the skipped step rather than about a gate that cannot exist yet. The
/// store repeats the check under the project's row lock; this is the readable half.
async fn ensure_predecessors(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    development_project_id: Uuid,
    step: Step,
) -> Result<(), ActionError> {
    let steps = store::development::steps_tx(&mut *tx, tenant, development_project_id)
        .await
        .map_err(|error| development_error(error, "development_projects.record_step"))?;
    let recorded: Vec<&str> = steps.iter().map(|row| row.step.as_str()).collect();
    let missing: Vec<String> = step
        .requires()
        .iter()
        .filter(|required| !recorded.iter().any(|current| *current == required.as_str()))
        .map(|required| required.as_str().to_owned())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(development_error(
        DevelopmentStoreError::StepOutOfOrder {
            step: step.as_str().to_owned(),
            missing,
        },
        "development_projects.record_step",
    ))
}

/// The newest development gate for `(project, gate)`: a pending one first, then an approved one.
///
/// `pub(crate)` because the unit facts builder (`crate::proof::facts`) derives the same gate
/// observations the close path resolves; there is one gate reader, not a second one.
pub(crate) async fn find_gate(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    project_id: Uuid,
    gate: &str,
) -> Result<Option<ApprovalRecord>, ActionError> {
    let rows = store::approval::list_approvals_tx(
        &mut *tx,
        tenant,
        None,
        None,
        None,
        Some(project_id),
        None,
        None,
        store::actions::MAX_LIMIT,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, gate, "list the project's development gates");
        ActionError::internal()
    })?;
    let matching: Vec<ApprovalRecord> = rows
        .into_iter()
        .filter(|row| row.approval.gate == gate && row.approval.kind == GATE_KIND)
        .collect();
    // A pending gate is the current one; otherwise the newest approved gate is the resolved
    // decision. A rejected gate is history, not a gate the caller can still use.
    if let Some(pending) = matching
        .iter()
        .find(|row| row.approval.status == conveyor_domain::ApprovalStatus::Pending)
    {
        return Ok(Some(pending.clone()));
    }
    Ok(matching
        .into_iter()
        .filter(|row| row.approval.status == conveyor_domain::ApprovalStatus::Approved)
        .max_by_key(|row| row.created_at))
}

/// Request a development gate, or return the existing pending/approved one.
///
/// The partial unique index `uq_appr_development_pending` backs the "one pending" half; the
/// re-read after the insert means a race surfaces as the existing row rather than a second gate.
async fn request_gate(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    project: &DevelopmentProjectRow,
    gate: &'static str,
    target: String,
    extra_meta: Value,
) -> Result<(ApprovalRecord, bool), ActionError> {
    if let Some(existing) = find_gate(&mut *tx, ctx.tenant, project.project_id, gate).await? {
        return Ok((existing, false));
    }
    let mut meta = match extra_meta {
        Value::Object(object) => object,
        _ => serde_json::Map::new(),
    };
    meta.insert(
        "development_project_id".to_owned(),
        Value::String(project.id.to_string()),
    );
    meta.insert(
        "implementer_id".to_owned(),
        Value::String(project.implementer_id.to_string()),
    );
    meta.insert("target".to_owned(), Value::String(target));
    meta.insert("gate_set".to_owned(), project.gate_set.clone());
    let new = store::approval::NewApproval {
        project_id: Some(project.project_id),
        gate: gate.to_owned(),
        kind: GATE_KIND.to_owned(),
        required_decisions: 1,
        requested_by: Some(ctx.actor_id),
        meta: Value::Object(meta),
        ..store::approval::NewApproval::new(gate, GATE_KIND)
    };
    let approval = store::approval::insert_approval_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| match error {
            store::ApprovalStoreError::Conflict => ActionError::new(
                409,
                "gate_exists",
                format!("{gate} already has a pending gate for this project"),
            ),
            other => {
                tracing::error!(%other, gate, "insert a development gate");
                ActionError::internal()
            }
        })?;
    let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, approval.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the development gate just inserted");
            ActionError::internal()
        })?
        .ok_or_else(ActionError::internal)?;
    Ok((record, true))
}

pub(crate) fn gate_approved(record: &Option<ApprovalRecord>) -> bool {
    record
        .as_ref()
        .is_some_and(|record| record.approval.status == conveyor_domain::ApprovalStatus::Approved)
}

fn ci_verdict(evidence: &Value) -> Option<CiVerdict> {
    evidence
        .get("verdict")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse().ok())
}

// ── development_projects.charter ────────────────────────────────────────────────────────

/// Charter a development project: one repo, one lane, before any branch (§6.4, §9.1 1).
#[crate::action(
    name = "development_projects.charter",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_charter(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentCharterIn,
) -> Result<HandlerOutput<DevelopmentProjectOut>, ActionError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let repo = input.repo.trim();
    if repo.is_empty() || repo.chars().any(char::is_whitespace) {
        return Err(invalid(
            "repo",
            "invalid_value",
            "one repository, with no whitespace",
        ));
    }
    let lane = input.lane.trim();
    if lane.is_empty() || lane.chars().any(char::is_whitespace) {
        return Err(invalid(
            "lane",
            "invalid_value",
            "one lane (branch name), with no whitespace",
        ));
    }
    let issue_ref = match input.issue_ref.as_deref() {
        None => None,
        Some(raw) => {
            let candidate = raw.trim();
            let shape = candidate.rsplit_once('#').and_then(|(slug, number)| {
                let (owner, name) = slug.split_once('/')?;
                let part_ok = |value: &str| {
                    !value.is_empty()
                        && value
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
                };
                let number_ok = number.parse::<u64>().is_ok_and(|number| number > 0);
                (part_ok(owner) && part_ok(name) && number_ok).then_some(candidate)
            });
            match shape {
                Some(issue) => Some(issue.to_owned()),
                None => {
                    return Err(invalid(
                        "issue_ref",
                        "issue_ref_invalid",
                        "expected owner/name#N, a public repository slug and a positive issue number",
                    ));
                }
            }
        }
    };
    let acceptance = input.acceptance_command.trim();
    if acceptance.is_empty() {
        return Err(invalid(
            "acceptance_command",
            "required",
            "the project's own acceptance command is required",
        ));
    }
    let gate_set = match input.gate_set {
        None => json!(conveyor_domain::development::DEFAULT_GATE_SET),
        Some(gates) => {
            if gates.is_empty() {
                return Err(invalid("gate_set", "invalid_value", "must not be empty"));
            }
            for gate in &gates {
                if !matches!(gate.as_str(), "design" | "review" | "merge" | "deploy") {
                    return Err(invalid(
                        "gate_set",
                        "invalid_value",
                        format!(
                            "unknown gate {gate:?}; the development gates are design, review, \
                             merge and deploy"
                        ),
                    ));
                }
            }
            for required in ["merge", "deploy"] {
                if !gates.iter().any(|gate| gate == required) {
                    return Err(invalid(
                        "gate_set",
                        "invalid_value",
                        format!("the gate set must include {required:?}"),
                    ));
                }
            }
            json!(gates)
        }
    };

    // The unit certificate's policy is normalized here, before any row exists: the digest shape
    // and the required/blank-spec rule are the action's refusals, and the table's CHECKs are the
    // backstop for a row written by any other path.
    let proof_policy = match input.proof_policy.as_ref() {
        None => None,
        Some(policy) => {
            let spec_name = policy.spec_name.trim();
            if spec_name.is_empty() {
                return Err(invalid(
                    "proof_policy.spec_name",
                    "required",
                    "must not be empty",
                ));
            }
            let spec_sha256 = policy.spec_sha256.trim();
            let digest_ok = spec_sha256.is_empty()
                || (spec_sha256.len() == 64
                    && spec_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
            if !digest_ok {
                return Err(invalid(
                    "proof_policy.spec_sha256",
                    "invalid_value",
                    "must be a lower-case SHA-256 (64 hex characters) or empty",
                ));
            }
            if policy.required && spec_sha256.is_empty() {
                return Err(invalid(
                    "proof_policy.spec_sha256",
                    "required",
                    "a required unit gate must pin the specification digest",
                ));
            }
            Some(ProofPolicySpec {
                spec_name: spec_name.to_owned(),
                spec_sha256: spec_sha256.to_owned(),
                required: policy.required,
            })
        }
    };

    let key = match input.key.as_deref() {
        Some(raw) => {
            let key = conveyor_domain::Code::parse(raw)
                .map_err(|error| invalid("key", "invalid_value", error.to_string()))?
                .to_string();
            // A replay names the same charter and gets the same row. The key is the tenant's
            // project key, so the read is the unique index and cannot race the insert: two
            // first-time charters with one key serialise on the project insert itself.
            if let Some(project) = store::project::get_project_by_key_tx(&mut *tx, ctx.tenant, &key)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "read the development project by key");
                    ActionError::internal()
                })?
            {
                let existing = store::development::get_by_project_id_tx(
                    &mut *tx, ctx.tenant, project.id,
                )
                .await
                .map_err(|error| {
                    tracing::error!(%error, "read the development charter by project");
                    ActionError::internal()
                })?
                .ok_or_else(|| {
                    ActionError::new(
                        409,
                        "project_exists_without_charter",
                        format!(
                            "development_projects.charter: project {key} exists but carries no \
                             development charter"
                        ),
                    )
                })?;
                let mut mismatched = Vec::new();
                if existing.repo != repo {
                    mismatched.push("repo");
                }
                if existing.lane != lane {
                    mismatched.push("lane");
                }
                if existing.implementer_id != input.implementer_id {
                    mismatched.push("implementer_id");
                }
                if issue_ref.is_some() && existing.issue_ref != issue_ref {
                    mismatched.push("issue_ref");
                }
                let existing_policy =
                    store::development::proof_policy_tx(&mut *tx, ctx.tenant, existing.id)
                        .await
                        .map_err(|error| {
                            tracing::error!(?error, "read the development proof policy");
                            ActionError::internal()
                        })?;
                let policy_matches = match (&proof_policy, &existing_policy) {
                    (None, None) => true,
                    (Some(requested), Some(existing)) => {
                        requested.spec_name == existing.spec_name
                            && requested.spec_sha256 == existing.spec_sha256
                            && requested.required == existing.required
                    }
                    _ => false,
                };
                if !policy_matches {
                    mismatched.push("proof_policy");
                }
                if !mismatched.is_empty() {
                    return Err(ActionError::new(
                        409,
                        "charter_mismatch",
                        format!(
                            "development_projects.charter: key {key} is already chartered with a \
                             different {}; a replay must name the same charter",
                            mismatched.join(", ")
                        ),
                    ));
                }
                return Ok(HandlerOutput::ok(policy_from_row(&existing)));
            }
            key
        }
        None => store::project::derive_project_key_tx(&mut *tx, ctx.tenant, title)
            .await
            .map_err(|error| match error {
                store::project::ProjectStoreError::KeyExhausted { .. } => ActionError::new(
                    409,
                    "conflict",
                    "every derived key for this title is taken; supply an explicit `key`",
                ),
                other => {
                    tracing::error!(%other, "derive the development project key");
                    ActionError::internal()
                }
            })?,
    };

    let project = store::project::insert_project_tx(
        &mut *tx,
        ctx.tenant,
        &store::project::NewProject {
            key,
            name: title.to_owned(),
            description: input.description.unwrap_or_default(),
            status: Some("active".to_owned()),
            programme_id: None,
            project_type_code: Some("development".to_owned()),
            phase_code: None,
            owner_user_id: Some(ctx.actor_id),
            repo_url: Some(repo.to_owned()),
            meta: json!({"development_project": true, "lane": lane}),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "insert the development project row");
        ActionError::internal()
    })?;

    let row = store::development::insert_tx(
        &mut *tx,
        ctx.tenant,
        &NewDevelopmentProject {
            project_id: project.id,
            repo: repo.to_owned(),
            lane: lane.to_owned(),
            issue_ref,
            implementer_id: input.implementer_id,
            gate_set,
            instrument_ref: input.instrument_ref,
            acceptance_command: acceptance.to_owned(),
            created_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| development_error(error, "development_projects.charter"))?;

    if let Some(policy) = &proof_policy {
        store::development::insert_proof_policy_tx(
            &mut *tx,
            ctx.tenant,
            &store::development::NewProofPolicy {
                development_project_id: row.id,
                spec_name: policy.spec_name.clone(),
                spec_sha256: policy.spec_sha256.clone(),
                required: policy.required,
            },
        )
        .await
        .map_err(|error| development_error(error, "development_projects.charter"))?;
    }

    Ok(HandlerOutput::created(policy_from_row(&row)))
}

// ── development_projects.record_step ────────────────────────────────────────────────────

/// Record one step's immutable evidence, refusing a skipped predecessor (§9.1 3–8).
#[crate::action(
    name = "development_projects.record_step",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_record_step(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentStepIn,
) -> Result<HandlerOutput<DevelopmentStepOut>, ActionError> {
    let step = parse_step(&input.step)?;
    validate_evidence(step, &input.evidence)
        .map_err(|error| invalid("evidence", "invalid_value", error.to_string()))?;
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    ensure_predecessors(&mut *tx, ctx.tenant, project.id, step).await?;

    match step {
        Step::Merge => {
            let gate = find_gate(&mut *tx, ctx.tenant, project.project_id, MERGE_GATE).await?;
            if !gate_approved(&gate) {
                return Err(ActionError::new(
                    409,
                    "merge_gate_pending",
                    "the merge step requires an approved merge gate; a CI result is evidence, not \
                     an approval",
                ));
            }
        }
        Step::Deploy => {
            let gate = find_gate(&mut *tx, ctx.tenant, project.project_id, DEPLOY_GATE).await?;
            if !gate_approved(&gate) {
                return Err(ActionError::new(
                    409,
                    "deploy_gate_pending",
                    "the deploy step requires an approved deploy gate; the merge gate is a \
                     separate decision",
                ));
            }
        }
        Step::LiveVerify => {
            let deploy = load_step(&mut *tx, ctx.tenant, project.id, Step::Deploy)
                .await?
                .ok_or_else(|| {
                    ActionError::new(
                        409,
                        "step_out_of_order",
                        "the live check cannot be recorded before the deploy",
                    )
                })?;
            let deployed = deploy.evidence.get("digest").and_then(Value::as_str);
            let serving = input.evidence.get("serving_digest").and_then(Value::as_str);
            if deployed != serving {
                return Err(invalid(
                    "evidence.serving_digest",
                    "invalid_value",
                    format!(
                        "the live check reports {serving:?} but the deployed digest is \
                         {deployed:?}; the check must run against the deployed revision"
                    ),
                ));
            }
        }
        _ => {}
    }

    let row = store::development::record_step_tx(
        &mut *tx,
        ctx.tenant,
        project.id,
        step,
        &input.evidence,
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| development_error(error, "development_projects.record_step"))?;

    let status = match step {
        Step::Merge => "merged",
        Step::Deploy => "deployed",
        Step::LiveVerify => "verified",
        _ => "in_progress",
    };
    store::development::update_status_tx(&mut *tx, ctx.tenant, project.id, status)
        .await
        .map_err(|error| development_error(error, "development_projects.record_step"))?;

    Ok(HandlerOutput::created(step_out(&row)))
}

// ── the gates ───────────────────────────────────────────────────────────────────────────

/// Open (or return) the review gate. The reviewer is a person other than the implementer.
#[crate::action(
    name = "development_projects.review_gate",
    version = 1,
    kind = "internal",
    scopes = ["projects:write", "approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_review_gate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentGateIn,
) -> Result<HandlerOutput<DevelopmentGateOut>, ActionError> {
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    let ci = load_step(&mut *tx, ctx.tenant, project.id, Step::Ci)
        .await?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "ci_missing",
                "the review gate opens after the CI run's verdict is recorded",
            )
        })?;
    if ci_verdict(&ci.evidence) != Some(CiVerdict::Green) {
        return Err(ActionError::new(
            422,
            "ci_red",
            "the CI run is red; review and merge stop until a green run is recorded",
        ));
    }
    let head = ci
        .evidence
        .get("sha")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = format!("review:{}:{head}", project.project_id);
    let (record, created) = request_gate(
        &mut *tx,
        &ctx,
        &project,
        REVIEW_GATE,
        target,
        json!({"head_sha": head}),
    )
    .await?;
    let policy = GatePolicy::human();
    Ok(HandlerOutput::created(gate_out(
        &record,
        &policy,
        created,
        Vec::new(),
    )))
}

/// Read the tenant's merge-gate policy and evaluate its typed conditions against the project.
async fn merge_gate_policy(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    project: &DevelopmentProjectRow,
    ci: &DevelopmentStepRow,
) -> Result<GatePolicy, ActionError> {
    let policy = match store::approval_policy::get(&mut *tx, ctx.tenant, MERGE_GATE).await {
        Ok(Some(policy)) => policy,
        Ok(None) => store::approval_policy::ApprovalPolicy::default_for(MERGE_GATE),
        Err(store::approval_policy::ApprovalPolicyStoreError::Vocabulary { what, value }) => {
            tracing::error!(what, value = %value, "a stored merge-gate policy is outside the vocabulary");
            store::approval_policy::ApprovalPolicy::default_for(MERGE_GATE)
        }
        Err(error) => {
            tracing::error!(%error, "read the merge-gate policy");
            return Err(ActionError::internal());
        }
    };
    let mut gate_policy = GatePolicy {
        approver_kind: policy.approver_kind,
        conditions: policy.conditions.clone(),
        expires_at: policy.expires_at,
        auto_merge: false,
        conditions_satisfied: false,
        reasons: Vec::new(),
    };
    if policy.approver_kind != ApproverKind::Auto {
        return Ok(gate_policy);
    }

    let conditions = match AutoMergeConditions::parse(&policy.conditions) {
        Ok(conditions) => conditions,
        Err(error) => {
            // A corrupt/unknown condition set fails closed. It is never "no conditions".
            gate_policy.reasons.push(format!(
                "the tenant's auto-merge conditions are not usable: {error}"
            ));
            return Ok(gate_policy);
        }
    };
    let green_runs = u32::from(ci_verdict(&ci.evidence) == Some(CiVerdict::Green));
    // No revert evidence can exist on a development project that reaches its merge gate: a
    // revert is a new project. The tenant's ceiling is still read and evaluated.
    let reverts = 0;
    let pr = load_step(&mut *tx, ctx.tenant, project.id, Step::PullRequest).await?;
    let changed_paths: Option<Vec<String>> = pr.as_ref().and_then(|row| {
        row.evidence
            .get("paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
    });
    let review = find_gate(&mut *tx, ctx.tenant, project.project_id, REVIEW_GATE).await?;
    let independent_reviewer = match review {
        Some(record) if record.approval.status == conveyor_domain::ApprovalStatus::Approved => {
            let decisions = store::approval::list_decisions_for_approvals_tx(
                &mut *tx,
                ctx.tenant,
                &[record.approval.id],
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the review gate's decisions");
                ActionError::internal()
            })?;
            decisions
                .iter()
                .any(|d| d.decision == "approve" && d.approver_id != project.implementer_id)
        }
        _ => false,
    };
    let evaluation = conditions.evaluate(&AutoMergeInputs {
        green_runs,
        reverts,
        changed_paths: changed_paths.as_deref(),
        independent_reviewer,
    });
    gate_policy.conditions_satisfied = evaluation.satisfied;
    gate_policy.reasons = evaluation.reasons;
    gate_policy.auto_merge =
        auto_merge_in_force(policy.expires_at, Utc::now()) && evaluation.satisfied;
    if !auto_merge_in_force(policy.expires_at, Utc::now()) {
        gate_policy
            .reasons
            .push("the auto-merge policy has expired".to_owned());
    }
    Ok(gate_policy)
}

/// Open (or return) the merge gate. A red CI verdict is a refusal, never a retry (§9.1 7).
///
/// The tenant's `approval_policy` row decides whether a person or the earned auto-merge policy
/// resolves the gate; its kind, conditions and expiry are read, never hard-coded (§6.4).
#[crate::action(
    name = "development_projects.merge_gate",
    version = 1,
    kind = "internal",
    scopes = ["projects:write", "approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_merge_gate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentGateIn,
) -> Result<HandlerOutput<DevelopmentGateOut>, ActionError> {
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    let ci = load_step(&mut *tx, ctx.tenant, project.id, Step::Ci)
        .await?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "ci_missing",
                "a merge gate needs the CI run's verdict; a green run is evidence, not an approval",
            )
        })?;
    if ci_verdict(&ci.evidence) != Some(CiVerdict::Green) {
        return Err(ActionError::new(
            422,
            "ci_red",
            "the CI run is red: the merge gate is refused and recorded; it is never retried \
             until green",
        ));
    }
    let review = find_gate(&mut *tx, ctx.tenant, project.project_id, REVIEW_GATE).await?;
    if !gate_approved(&review) {
        return Err(ActionError::new(
            409,
            "review_gate_pending",
            "the merge gate waits for the independent reviewer's approved decision",
        ));
    }

    let policy = merge_gate_policy(&mut *tx, &ctx, &project, &ci).await?;
    let head = ci
        .evidence
        .get("sha")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = format!("merge:{}:{head}", project.project_id);
    let (mut record, created) = request_gate(
        &mut *tx,
        &ctx,
        &project,
        MERGE_GATE,
        target,
        json!({"head_sha": head}),
    )
    .await?;

    // The earned auto-merge path: resolve the pending gate with a recorded `rule` decision. The
    // decision writer is bypassed deliberately — a rule has no principal — and the row says
    // `decided_by_kind = 'rule'` with the policy in `via`, so the audit still tells the truth.
    if policy.auto_merge && record.approval.status == conveyor_domain::ApprovalStatus::Pending {
        let policy_document = json!({
            "action": MERGE_GATE,
            "approver_kind": policy.approver_kind.as_str(),
            "conditions": policy.conditions.clone(),
            "expires_at": policy.expires_at,
        });
        store::development::auto_approve_tx(
            &mut *tx,
            ctx.tenant,
            record.approval.id,
            ctx.actor_id,
            &policy_document,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "resolve the merge gate under the auto-merge policy");
            ActionError::internal()
        })?;
        record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, record.approval.id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "re-read the auto-approved merge gate");
                ActionError::internal()
            })?
            .ok_or_else(ActionError::internal)?;
    }

    Ok(HandlerOutput::created(gate_out(
        &record,
        &policy,
        created,
        policy.reasons.clone(),
    )))
}

/// Open (or return) the deploy gate. A deployable change with no tested rollback plan is refused
/// (§9.3); the refusal names the missing rollback evidence.
#[crate::action(
    name = "development_projects.deploy_gate",
    version = 1,
    kind = "internal",
    scopes = ["projects:write", "approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_deploy_gate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentGateIn,
) -> Result<HandlerOutput<DevelopmentGateOut>, ActionError> {
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    let merge = load_step(&mut *tx, ctx.tenant, project.id, Step::Merge)
        .await?
        .ok_or_else(|| {
            ActionError::new(409, "merge_missing", "the deploy gate follows the merge")
        })?;
    let merge_gate = find_gate(&mut *tx, ctx.tenant, project.project_id, MERGE_GATE).await?;
    if !gate_approved(&merge_gate) {
        return Err(ActionError::new(
            409,
            "merge_gate_pending",
            "the deploy gate follows an approved merge gate",
        ));
    }
    let plan = load_step(&mut *tx, ctx.tenant, project.id, Step::RollbackPlan)
        .await?
        .ok_or_else(|| {
            ActionError::new(
                422,
                "rollback_plan_missing",
                "the deploy gate is refused: no recorded rollback plan exists. §9.3 requires a \
                 tested rollback before a deployable change is deployed. Every migration must be \
                 expand/contract (backward-compatible with the immediately prior release); a \
                 non-additive migration requires a restore-based rollback through the gated ops \
                 path, never an application-only rollback",
            )
        })?;
    if plan.evidence.get("tested").and_then(Value::as_bool) != Some(true) {
        return Err(ActionError::new(
            422,
            "rollback_untested",
            "the deploy gate is refused: the recorded rollback plan is marked untested. A deploy \
             without a tested rollback is refused",
        ));
    }
    let restore_path = plan
        .evidence
        .get("restore_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty());
    if plan
        .evidence
        .get("backward_compatible")
        .and_then(Value::as_bool)
        == Some(false)
        && restore_path.is_none()
    {
        return Err(ActionError::new(
            422,
            "restore_path_missing",
            "the deploy gate is refused: the rollback plan declares the deploy's migrations \
             non-additive (`backward_compatible: false`) but names no restore path. A non-additive \
             migration requires a restore-based rollback through the gated ops path; an \
             application-only rollback is refused",
        ));
    }
    if plan
        .evidence
        .get("evidence")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        return Err(ActionError::new(
            422,
            "rollback_evidence_missing",
            "the deploy gate is refused: the rollback plan carries no evidence that it was \
             exercised",
        ));
    }

    let merge_sha = merge
        .evidence
        .get("merge_sha")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = format!("deploy:{}:{merge_sha}", project.project_id);
    let (record, created) = request_gate(
        &mut *tx,
        &ctx,
        &project,
        DEPLOY_GATE,
        target,
        json!({"merge_sha": merge_sha}),
    )
    .await?;
    let policy = GatePolicy::human();
    Ok(HandlerOutput::created(gate_out(
        &record,
        &policy,
        created,
        Vec::new(),
    )))
}

// ── development_projects.rollback ───────────────────────────────────────────────────────

/// Record an exercised rollback: the prior digest serves again (P3.2's offline-recordable half).
#[crate::action(
    name = "development_projects.rollback",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_rollback(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentRollbackIn,
) -> Result<HandlerOutput<DevelopmentStepOut>, ActionError> {
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    let evidence = json!({
        "deployed_digest": input.deployed_digest,
        "restored_digest": input.restored_digest,
        "migration_generation": input.migration_generation,
        "health_status": input.health_status,
        "evidence": input.evidence,
    });
    validate_evidence(Step::RollbackExercise, &evidence)
        .map_err(|error| invalid("evidence", "invalid_value", error.to_string()))?;
    let deploy = load_step(&mut *tx, ctx.tenant, project.id, Step::Deploy)
        .await?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "step_out_of_order",
                "a rollback is exercised against a deployed change",
            )
        })?;
    let deployed = deploy.evidence.get("digest").and_then(Value::as_str);
    if deployed != Some(&input.deployed_digest) {
        return Err(invalid(
            "deployed_digest",
            "invalid_value",
            format!("the deployed digest is {deployed:?}, not the rollback's deployed digest"),
        ));
    }
    if let Some(deployed_generation) = deploy
        .evidence
        .get("migration_generation")
        .and_then(Value::as_i64)
        && deployed_generation != input.migration_generation
    {
        return Err(invalid(
            "migration_generation",
            "invalid_value",
            format!(
                "the deploy recorded migration generation {deployed_generation}, not {}; the \
                 rollback must name the generation the deploy applied",
                input.migration_generation
            ),
        ));
    }
    // The rollback plan is schema-aware (issue #669): a plan that declares the deploy's
    // migrations non-additive forbids an application-only rollback. The prior release cannot be
    // trusted against a schema it was not built for; the plan names the restore path instead.
    let plan = load_step(&mut *tx, ctx.tenant, project.id, Step::RollbackPlan)
        .await?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "step_out_of_order",
                "a rollback plan exists before the deploy (§9.1 8)",
            )
        })?;
    if plan
        .evidence
        .get("backward_compatible")
        .and_then(Value::as_bool)
        == Some(false)
    {
        let restore = plan
            .evidence
            .get("restore_path")
            .and_then(Value::as_str)
            .unwrap_or("the gated ops restore path");
        return Err(ActionError::new(
            409,
            "schema_restore_required",
            format!(
                "the application rollback is refused: the recorded rollback plan declares the \
                 deploy's migrations non-additive, so the prior release cannot serve against the \
                 newer schema. Restore the database to the matching generation through the gated \
                 ops path and record that as the rollback evidence: {restore}"
            ),
        ));
    }
    let row = store::development::record_step_tx(
        &mut *tx,
        ctx.tenant,
        project.id,
        Step::RollbackExercise,
        &evidence,
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| development_error(error, "development_projects.rollback"))?;
    Ok(HandlerOutput::created(step_out(&row)))
}

// ── development_projects.close ──────────────────────────────────────────────────────────

/// Resolve the unit's Definition of done against the engine's own rows (#837).
///
/// The policy the charter pinned selects the mode. With no policy the unit is advisory and the
/// closure says so. With a policy, the engine derives the facts, finds the newest proven
/// certificate over exactly those facts and that specification pin, and resolves it through the
/// same `lean_proof` arm a PM gate uses. Required refuses the close when the certificate is
/// missing or invalid; advisory records the outcome in the closure and lets the close proceed.
async fn unit_dod(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    project: &DevelopmentProjectRow,
) -> Result<Value, ActionError> {
    use crate::proof::facts;

    let policy = store::development::proof_policy_tx(&mut *conn, tenant, project.id)
        .await
        .map_err(|error| development_error(error, "development_projects.close"))?;
    let Some(policy) = policy else {
        return Ok(json!({
            "required": false,
            "lean_proof": "missing",
            "reason": "the unit carries no proof_policy",
        }));
    };
    if policy.spec_sha256.trim().is_empty() {
        if policy.required {
            return Err(crate::pm::missing_required_kind("lean_proof"));
        }
        return Ok(json!({
            "required": false,
            "lean_proof": "missing",
            "reason": "the proof_policy pins no specification digest",
        }));
    }

    let facts = match facts::derive_unit_record(&mut *conn, tenant, project.id).await {
        Ok(facts) => facts,
        Err(error) if policy.required => return Err(error),
        Err(error) => {
            return Ok(json!({
                "required": false,
                "lean_proof": "unproven",
                "reason": error.code,
                "detail": error.detail,
            }));
        }
    };
    let canonical = facts.canonical()?;
    let facts_sha256 = conveyor_domain::compare::content_hash(&Value::String(canonical));
    let proof = facts::proof_on_facts(
        &mut *conn,
        tenant,
        project.id,
        &facts_sha256,
        &policy.spec_sha256,
    )
    .await?;
    let Some(proof) = proof else {
        if policy.required {
            return Err(crate::pm::missing_required_kind("lean_proof"));
        }
        return Ok(json!({
            "required": false,
            "lean_proof": "missing",
            "facts_sha256": facts_sha256,
        }));
    };

    match crate::pm::resolve_lean_proof(&mut *conn, tenant, proof.id, Some(&policy.spec_sha256))
        .await
    {
        Ok(evidence) => Ok(json!({
            "required": policy.required,
            "lean_proof": "proven",
            "proof_id": proof.id,
            "facts_sha256": facts_sha256,
            "evidence": evidence,
        })),
        Err(error) if policy.required => Err(error),
        Err(error) => Ok(json!({
            "required": false,
            "lean_proof": "unproven",
            "proof_id": proof.id,
            "facts_sha256": facts_sha256,
            "reason": error.code,
            "detail": error.detail,
        })),
    }
}

/// Write the closure record, after every step and gate passed (P1.1, §9.1 9).
#[crate::action(
    name = "development_projects.close",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_close(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentCloseIn,
) -> Result<HandlerOutput<DevelopmentClosureOut>, ActionError> {
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;

    // 1. Every required step exists. "No step may be skipped" is the issue's P1 rule.
    let steps = store::development::steps_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| development_error(error, "development_projects.close"))?;
    let recorded: Vec<&str> = steps.iter().map(|row| row.step.as_str()).collect();
    let missing: Vec<&str> = Step::REQUIRED_FOR_CLOSURE
        .iter()
        .map(|step| step.as_str())
        .filter(|step| !recorded.contains(step))
        .collect();
    if !missing.is_empty() {
        return Err(ActionError::new(
            409,
            "steps_missing",
            format!(
                "the closure record is refused: these steps were not recorded: {}",
                missing.join(", ")
            ),
        ));
    }

    // 2. The gates are approved, each with its evidence.
    let review = find_gate(&mut *tx, ctx.tenant, project.project_id, REVIEW_GATE).await?;
    let merge = find_gate(&mut *tx, ctx.tenant, project.project_id, MERGE_GATE).await?;
    let deploy = find_gate(&mut *tx, ctx.tenant, project.project_id, DEPLOY_GATE).await?;
    for (gate, record) in [
        (REVIEW_GATE, &review),
        (MERGE_GATE, &merge),
        (DEPLOY_GATE, &deploy),
    ] {
        if !gate_approved(record) {
            return Err(development_error(
                DevelopmentStoreError::ClosureGateMissing(gate),
                "development_projects.close",
            ));
        }
    }
    let review = review.expect("approved review gate");
    let merge = merge.expect("approved merge gate");
    let deploy = deploy.expect("approved deploy gate");

    // 3. The reviewer is a different principal from the implementer, and approved.
    let decisions = store::approval::list_decisions_for_approvals_tx(
        &mut *tx,
        ctx.tenant,
        &[review.approval.id],
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "read the closure's review decisions");
        ActionError::internal()
    })?;
    let decision = decisions
        .iter()
        .find(|d| d.decision == "approve" && d.approver_id != project.implementer_id)
        .ok_or_else(|| {
            ActionError::new(
                409,
                "reviewer_missing",
                "the closure record needs an approving review decision by a principal other than \
                 the implementer",
            )
        })?;

    // 4. The unit's Definition of done: the engine-derived certificate, when the charter pinned
    //    one. Required refuses the close; advisory reports the outcome in the closure. The same
    //    `lean_proof` evidence arm a PM gate resolves is reused — one path, not two.
    let dod = unit_dod(&mut *tx, ctx.tenant, &project).await?;

    // 5. The steps' typed evidence, read from the rows (never from the caller).
    let by_step = |step: Step| steps.iter().find(|row| row.step == step.as_str());
    let pr = by_step(Step::PullRequest).expect("required pull_request step");
    let ci = by_step(Step::Ci).expect("required ci step");
    let merged = by_step(Step::Merge).expect("required merge step");
    let plan = by_step(Step::RollbackPlan).expect("required rollback_plan step");
    let deployed = by_step(Step::Deploy).expect("required deploy step");
    let live = by_step(Step::LiveVerify).expect("required live_verify step");
    let rollback = by_step(Step::RollbackExercise);

    if ci_verdict(&ci.evidence) != Some(CiVerdict::Green) {
        return Err(ActionError::new(
            409,
            "ci_red",
            "the closure record is refused: the CI verdict is not green",
        ));
    }
    let deployed_digest = deployed
        .evidence
        .get("digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let serving = live
        .evidence
        .get("serving_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if deployed_digest != serving {
        return Err(ActionError::new(
            409,
            "digest_mismatch",
            format!(
                "the closure record is refused: the live check ran against {serving:?}, not the \
                 deployed digest {deployed_digest:?}"
            ),
        ));
    }

    let lane = project.lane.clone();
    let pr_url = pr
        .evidence
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let ci_run_id = ci
        .evidence
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let merge_sha = merged
        .evidence
        .get("merge_sha")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    // The schema generation the deploy applied (P3.2: the closure is schema-aware). Older
    // records, written before #669, carry no generation and render `null` honestly.
    let migration_generation = deployed
        .evidence
        .get("migration_generation")
        .cloned()
        .unwrap_or(Value::Null);

    let closure = json!({
        "development_project_id": project.id,
        "project_id": project.project_id,
        "repo": project.repo,
        "lane": lane,
        "pull_request": pr.evidence,
        "ci": {"run_id": ci_run_id, "verdict": "green", "evidence": ci.evidence},
        "review": {
            "approval_id": review.approval.id,
            "reviewer_id": decision.approver_id,
            "decision": "approve",
            "decided_by_kind": decision.decided_by_kind,
            "decided_at": decision.decided_at,
        },
        "merge": {"approval_id": merge.approval.id, "merge_sha": merge_sha},
        "rollback_plan": plan.evidence,
        "deploy": {
            "approval_id": deploy.approval.id,
            "digest": deployed_digest,
            "migration_generation": migration_generation,
        },
        "live_check": live.evidence,
        "rollback": rollback.map(|row| row.evidence.clone()),
        "dod": dod,
    });

    let row = store::development::insert_closure_tx(
        &mut *tx,
        ctx.tenant,
        &NewDevelopmentClosure {
            development_project_id: project.id,
            review_approval_id: review.approval.id,
            merge_approval_id: merge.approval.id,
            deploy_approval_id: deploy.approval.id,
            lane: lane.clone(),
            pr_url: pr_url.clone(),
            ci_run_id: ci_run_id.clone(),
            reviewer_id: decision.approver_id,
            merge_sha: merge_sha.clone(),
            deployed_digest: deployed_digest.to_owned(),
            live_check: live.evidence.clone(),
            closure: closure.clone(),
            closed_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| development_error(error, "development_projects.close"))?;
    store::development::update_status_tx(&mut *tx, ctx.tenant, project.id, "closed")
        .await
        .map_err(|error| development_error(error, "development_projects.close"))?;

    Ok(HandlerOutput::created(closure_out(&row)))
}

// ── development_projects.get ────────────────────────────────────────────────────────────

/// The project, its steps, its gates and its closure.
#[crate::action(
    name = "development_projects.get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentGetIn,
) -> Result<DevelopmentProjectOut, ActionError> {
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    let steps = store::development::steps_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| development_error(error, "development_projects.get"))?;
    let closure = store::development::closure_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| development_error(error, "development_projects.get"))?;

    let mut gates = Vec::new();
    for gate in [REVIEW_GATE, MERGE_GATE, DEPLOY_GATE] {
        if let Some(record) = find_gate(&mut *tx, ctx.tenant, project.project_id, gate).await? {
            let policy = if gate == MERGE_GATE {
                let ci = steps.iter().find(|row| row.step == Step::Ci.as_str());
                match ci {
                    Some(ci) => merge_gate_policy(&mut *tx, &ctx, &project, ci).await?,
                    None => GatePolicy::human(),
                }
            } else {
                GatePolicy::human()
            };
            gates.push(gate_out(&record, &policy, false, policy.reasons.clone()));
        }
    }

    let mut out = policy_from_row(&project);
    out.steps = steps.iter().map(step_out).collect();
    out.gates = gates;
    out.closure = closure.as_ref().map(closure_out);
    Ok(out)
}

// ── development_projects.list ───────────────────────────────────────────────────────────

/// `development_projects.list` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentListIn {
    /// Only units delivering through this repository (`owner/name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Only units in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only the unit carrying this GitHub issue (`owner/name#N`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_ref: Option<String>,
    /// At most this many units (default 100, capped at 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// One unit in `development_projects.list`: the charter and the latest step.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentUnitOut {
    /// The development project row id.
    pub id: String,
    /// The ordinary project row it charters.
    pub project_id: String,
    /// The ordinary project's tenant-unique key.
    pub project_key: String,
    /// The one repository.
    pub repo: String,
    /// The one lane.
    pub lane: String,
    /// The GitHub issue that carries the unit's intent, when one does.
    pub issue_ref: Option<String>,
    /// `chartered | in_progress | merged | deployed | verified | closed | refused`.
    pub status: String,
    /// The last step recorded, in the step order; `null` when none is.
    pub latest_step: Option<String>,
    /// When that step was recorded.
    pub latest_step_at: Option<DateTime<Utc>>,
    /// When the unit was chartered.
    pub created_at: DateTime<Utc>,
}

/// The tenant's delivery units, newest first.
#[crate::action(
    name = "development_projects.list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentListIn,
) -> Result<Vec<DevelopmentUnitOut>, ActionError> {
    let status = match input.status.as_deref() {
        None => None,
        Some(raw) => {
            let candidate = raw.trim();
            if !matches!(
                candidate,
                "chartered"
                    | "in_progress"
                    | "merged"
                    | "deployed"
                    | "verified"
                    | "closed"
                    | "refused"
            ) {
                return Err(invalid(
                    "status",
                    "invalid_value",
                    "unknown development status; expected one of chartered, in_progress, merged, \
                     deployed, verified, closed, refused",
                ));
            }
            Some(candidate)
        }
    };
    let repo = input
        .repo
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let issue_ref = input
        .issue_ref
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let limit = i64::from(input.limit.unwrap_or(100).clamp(1, 500));
    let rows = store::development::list_tx(&mut *tx, ctx.tenant, repo, status, issue_ref, limit)
        .await
        .map_err(|error| {
            development_error(
                DevelopmentStoreError::Database(error),
                "development_projects.list",
            )
        })?;
    Ok(rows
        .into_iter()
        .map(|row| DevelopmentUnitOut {
            id: row.id.to_string(),
            project_id: row.project_id.to_string(),
            project_key: row.project_key,
            repo: row.repo,
            lane: row.lane,
            issue_ref: row.issue_ref,
            status: row.status,
            latest_step: row.latest_step,
            latest_step_at: row.latest_step_at,
            created_at: row.created_at,
        })
        .collect())
}

// ── development_projects.change_judging_surface ────────────────────────────────────────

/// The development project's attempt to change a surface that judges it (§9.3).
///
/// The refusal is the feature for `gate_set`, `approval_policy` and `instrument`: the legitimate
/// path is a separate governance project with a different approver set, and the invoke layer
/// records the refused call as an `action_invocation` + `audit_event` (outcome `refused`).
///
/// `proof_policy` is the one exception, and only in the safe direction: a principal other than
/// the implementer may toggle `required`, and the implementer may tighten it. The specification
/// pin is never rewritten — an implementer who could re-pin to an easier specification would
/// defeat the certificate — and the implementer may never loosen the gate (#837).
#[crate::action(
    name = "development_projects.change_judging_surface",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn development_projects_change_judging_surface(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DevelopmentJudgingChangeIn,
) -> Result<HandlerOutput<DevelopmentJudgingChangeOut>, ActionError> {
    if !JUDGING_SURFACES.contains(&input.surface.as_str()) {
        return Err(invalid(
            "surface",
            "invalid_value",
            "must be gate_set, approval_policy, instrument or proof_policy",
        ));
    }
    if !input.change.is_object() {
        return Err(invalid("change", "invalid_value", "must be a JSON object"));
    }
    let project = load_project(&mut *tx, ctx.tenant, input.development_project_id).await?;
    if input.surface == "proof_policy" {
        return change_proof_required(&mut *tx, &ctx, &project, &input.change).await;
    }
    Err(ActionError::new(
        403,
        "judging_surface_immutable",
        format!(
            "the development project {} cannot change its own {}: that surface judges this \
             project (§9.3). It is changed only by a separate governance project with a \
             different approver set (architecture board, governance:write); the charter's gate \
             set, approval policy and instrument are unchanged",
            project.id, input.surface
        ),
    ))
}

/// The `proof_policy` surface's one admissible change: `required`.
///
/// The specification pin is frozen at charter. A principal other than the implementer may toggle
/// the requirement either way; the implementer may only tighten it — loosening the gate that
/// judges its own work is exactly what §9.3 refuses (#837).
async fn change_proof_required(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    project: &DevelopmentProjectRow,
    change: &Value,
) -> Result<HandlerOutput<DevelopmentJudgingChangeOut>, ActionError> {
    let object = change.as_object().expect("the caller checked an object");
    if let Some(key) = object.keys().find(|key| key.as_str() != "required") {
        return Err(ActionError::new(
            403,
            "judging_surface_immutable",
            format!(
                "the proof policy's {key:?} is frozen at charter; only `required` may be \
                 toggled. Re-pinning the specification would let an implementer certify against \
                 an easier document"
            ),
        ));
    }
    let required = object
        .get("required")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("change.required", "invalid_value", "must be a boolean"))?;
    let policy = store::development::proof_policy_tx(&mut *conn, ctx.tenant, project.id)
        .await
        .map_err(|error| development_error(error, "development_projects.change_judging_surface"))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "proof_policy_missing",
                format!("development project {} carries no proof policy", project.id),
            )
        })?;
    if required && policy.spec_sha256.trim().is_empty() {
        return Err(ActionError::new(
            422,
            "proof_spec_unpinned",
            "a required unit gate needs the charter's specification digest; this policy has none",
        ));
    }
    if !required && ctx.actor_id == project.implementer_id {
        return Err(ActionError::new(
            403,
            "judging_surface_immutable",
            format!(
                "the implementer of development project {} cannot loosen its proof requirement: \
                 only a principal other than the implementer may (the same separation-of-duties \
                 rule that keeps the implementer out of its gates)",
                project.id
            ),
        ));
    }
    store::development::set_proof_required_tx(&mut *conn, ctx.tenant, project.id, required)
        .await
        .map_err(|error| development_error(error, "development_projects.change_judging_surface"))?;
    Ok(HandlerOutput::ok(DevelopmentJudgingChangeOut {
        refused: false,
    }))
}
