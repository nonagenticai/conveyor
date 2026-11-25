//! The TOGAF ADM actions (SPEC_V2 §8.1, §10.3, §10.4, D63; T9, issue #81).
//!
//! One declaration per action — HTTP, MCP and OpenAPI are projections of the same
//! `#[crate::action]`. The store owns every invariant; this module validates input, maps the
//! store's refusals onto the action error vocabulary, and emits the lifecycle event.
//!
//! Writes are `projects:write` (admin/kata_executor); reads are `projects:read` (open).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use conveyor_domain::architecture as vocab;
use conveyor_store as store;
use conveyor_store::architecture::{
    ArchitectureRule, ArchitectureStoreError, ArtifactLinkRecord, ArtifactRecord,
    ArtifactTypeRecord, ContractRecord, EntryRecord, GovernanceBodyRecord, MaturityRecord,
    NewArtifact, NewBody, NewContract, NewEntry, NewProfile, NewReview, NewWaiver, ProfileRecord,
    ReviewRecord, SkillRecord, WaiverRecord,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{approval_error, etag_of, invalid, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

fn arch_error(error: ArchitectureStoreError, action: &str) -> ActionError {
    match error {
        ArchitectureStoreError::Database(error) => store_error(error, action),
        ArchitectureStoreError::NotFound(what) => ActionError::resource_not_found(what),
        ArchitectureStoreError::Stale(what) => {
            ActionError::new(412, "stale", format!("the {what} moved; re-read it"))
        }
        ArchitectureStoreError::Conflict { what, why } => {
            ActionError::new(409, "conflict", format!("the {what}: {why}"))
        }
        ArchitectureStoreError::Illegal(rule) => {
            let detail = rule.to_string();
            match rule {
                ArchitectureRule::ApprovedImmutable => {
                    ActionError::new(409, "architecture_approved_immutable", detail)
                }
                ArchitectureRule::MissingMandatory(_) => {
                    ActionError::new(409, "architecture_artifact_missing", detail)
                }
                ArchitectureRule::ContractNotLive(_) => {
                    ActionError::new(409, "architecture_contract_not_live", detail)
                }
                ArchitectureRule::DesignContractMissing => {
                    ActionError::new(409, "architecture_contract_missing", detail)
                }
                ArchitectureRule::PhaseOutOfOrder { .. } => {
                    ActionError::new(409, "architecture_phase_out_of_order", detail)
                }
                ArchitectureRule::OutcomeRequired => {
                    ActionError::new(409, "architecture_outcome_required", detail)
                }
                ArchitectureRule::OutcomeNotApproved(_) => {
                    ActionError::new(409, "architecture_outcome_not_approved", detail)
                }
                ArchitectureRule::OutcomeNotOnDeliveryContract => {
                    ActionError::new(409, "architecture_outcome_not_on_delivery_contract", detail)
                }
                ArchitectureRule::AssessmentIncomplete { .. } => {
                    invalid("scores", "assessment_incomplete", detail)
                }
                ArchitectureRule::ScoreOutOfRange(_) => {
                    invalid("scores", "score_out_of_range", detail)
                }
                ArchitectureRule::WaiverNeedsOwnerAndDate => {
                    invalid("owner_user_id", "owner_and_date_required", detail)
                }
                ArchitectureRule::NoProfile => {
                    ActionError::new(409, "architecture_profile_missing", detail)
                }
                ArchitectureRule::ProfileInvalid(_) => {
                    ActionError::new(409, "architecture_profile_invalid", detail)
                }
                ArchitectureRule::ProfileNotAdopted => {
                    ActionError::new(409, "architecture_profile_not_adopted", detail)
                }
                ArchitectureRule::BoardDecisionRequired => {
                    ActionError::new(409, "architecture_profile_decision_required", detail)
                }
                ArchitectureRule::ReviewMissing => {
                    ActionError::new(409, "architecture_review_missing", detail)
                }
                ArchitectureRule::ReviewNonConformant(_) => {
                    ActionError::new(409, "architecture_review_non_conformant", detail)
                }
                ArchitectureRule::KataCandidateRequired => {
                    ActionError::new(409, "architecture_kata_candidate_required", detail)
                }
                ArchitectureRule::KataCandidateNotFound(_) => {
                    ActionError::new(409, "architecture_kata_candidate_not_found", detail)
                }
                ArchitectureRule::KataDeferralNotApproved(_) => {
                    ActionError::new(409, "architecture_kata_deferral_not_approved", detail)
                }
            }
        }
        ArchitectureStoreError::Meeting(error) => crate::meetings::meeting_error(error, action),
    }
}

/// Read one artefact by id.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactIdIn {
    /// The artefact.
    pub id: Uuid,
}

/// An artefact as the actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactOut {
    /// Row id.
    pub id: String,
    /// Owning project.
    pub project_id: Option<String>,
    /// The standard type.
    pub artifact_type_id: Option<String>,
    /// ADM phase.
    pub phase_code: String,
    /// Plateau.
    pub plateau: String,
    /// Artefact kind.
    pub artifact_kind: String,
    /// Title.
    pub title: String,
    /// Lifecycle status.
    pub status: String,
    /// The artefact's own version.
    pub artifact_version: i64,
    /// Author.
    pub author_id: Option<String>,
    /// Reviewer.
    pub reviewer_id: Option<String>,
    /// Reviewed at.
    pub reviewed_at: Option<DateTime<Utc>>,
    /// The related change initiative.
    pub change_initiative: String,
    /// Why it exists.
    pub purpose: String,
    /// Stakeholders.
    pub stakeholders: Value,
    /// Scope.
    pub scope: String,
    /// The published document.
    pub document_id: Option<String>,
    /// The contract it sits under.
    pub contract_id: Option<String>,
    /// Supersedes.
    pub supersedes_id: Option<String>,
    /// Superseded by.
    pub superseded_by_id: Option<String>,
    /// Type-specific metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
}

fn artifact_out(record: &ArtifactRecord) -> ArtifactOut {
    ArtifactOut {
        id: record.id.to_string(),
        project_id: record.project_id.map(|id| id.to_string()),
        artifact_type_id: record.artifact_type_id.map(|id| id.to_string()),
        phase_code: record.phase_code.clone(),
        plateau: record.plateau.clone(),
        artifact_kind: record.artifact_kind.clone(),
        title: record.title.clone(),
        status: record.status.clone(),
        artifact_version: record.artifact_version,
        author_id: record.author_id.map(|id| id.to_string()),
        reviewer_id: record.reviewer_id.map(|id| id.to_string()),
        reviewed_at: record.reviewed_at,
        change_initiative: record.change_initiative.clone(),
        purpose: record.purpose.clone(),
        stakeholders: record.stakeholders.clone(),
        scope: record.scope.clone(),
        document_id: record.document_id.map(|id| id.to_string()),
        contract_id: record.contract_id.map(|id| id.to_string()),
        supersedes_id: record.supersedes_id.map(|id| id.to_string()),
        superseded_by_id: record.superseded_by_id.map(|id| id.to_string()),
        meta: record.meta.clone(),
        version: record.version,
    }
}

/// `architecture.artifacts_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactListIn {
    /// Filter by project.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// Filter by phase.
    #[serde(default)]
    pub phase_code: Option<String>,
    /// Filter by plateau.
    #[serde(default)]
    pub plateau: Option<String>,
}

#[crate::action(
    name = "architecture.artifacts_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactListIn,
) -> Result<HandlerOutput<Page<ArtifactOut>>, ActionError> {
    let rows = store::architecture::list_artifacts_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        input.phase_code.as_deref(),
        input.plateau.as_deref(),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(artifact_out).collect(),
        next_cursor: None,
    }))
}

/// `architecture.artifacts_get`.
#[crate::action(
    name = "architecture.artifacts_get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactIdIn,
) -> Result<HandlerOutput<ArtifactOut>, ActionError> {
    let record = store::architecture::get_artifact_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture artefact"))?;
    Ok(HandlerOutput::ok(artifact_out(&record)).with_etag(etag_of(record.version)))
}

/// An entry supplied with a create or update.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactEntryIn {
    /// The first axis (a catalog's unique identifier).
    pub axis_a: String,
    /// The second axis (a matrix's).
    #[serde(default)]
    pub axis_b: String,
    /// The cell's value.
    #[serde(default)]
    pub value: Value,
    /// Display order.
    #[serde(default)]
    pub position: i32,
}

/// `architecture.artifacts_create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactCreateIn {
    /// The owning project.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// The standard type row.
    #[serde(default)]
    pub artifact_type_id: Option<Uuid>,
    /// ADM phase.
    pub phase_code: String,
    /// Plateau.
    pub plateau: String,
    /// catalog | matrix | diagram | principles_catalog.
    pub artifact_kind: String,
    /// Title.
    pub title: String,
    /// The related change initiative.
    #[serde(default)]
    pub change_initiative: Option<String>,
    /// Why it exists.
    #[serde(default)]
    pub purpose: Option<String>,
    /// Stakeholders.
    #[serde(default)]
    pub stakeholders: Option<Value>,
    /// Scope.
    #[serde(default)]
    pub scope: Option<String>,
    /// The published document.
    #[serde(default)]
    pub document_id: Option<Uuid>,
    /// The contract it sits under.
    #[serde(default)]
    pub contract_id: Option<Uuid>,
    /// Type-specific metadata.
    #[serde(default)]
    pub meta: Option<Value>,
    /// Catalog items or matrix cells.
    #[serde(default)]
    pub entries: Vec<ArtifactEntryIn>,
}

#[crate::action(
    name = "architecture.artifacts_create",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactCreateIn,
) -> Result<HandlerOutput<ArtifactOut>, ActionError> {
    // An artefact is a typed row of a project, not a free-floating document: the phase gate
    // resolves it by its project and its standard type, so a row missing either can never
    // evidence a phase and would only hide in the repository. Refused before the store writes.
    let Some(project_id) = input.project_id else {
        return Err(invalid(
            "project_id",
            "required",
            "an artefact must name the project it belongs to",
        ));
    };
    let Some(artifact_type_id) = input.artifact_type_id else {
        return Err(invalid(
            "artifact_type_id",
            "required",
            "an artefact must name its artifact_type",
        ));
    };
    let entries: Vec<NewEntry> = input
        .entries
        .iter()
        .map(|entry| NewEntry {
            axis_a: entry.axis_a.clone(),
            axis_b: entry.axis_b.clone(),
            value: entry.value.clone(),
            position: entry.position,
        })
        .collect();
    let new = NewArtifact {
        project_id: Some(project_id),
        artifact_type_id: Some(artifact_type_id),
        phase_code: input.phase_code,
        plateau: input.plateau,
        artifact_kind: input.artifact_kind,
        title: input.title,
        change_initiative: input.change_initiative,
        purpose: input.purpose,
        stakeholders: input.stakeholders,
        scope: input.scope,
        document_id: input.document_id,
        contract_id: input.contract_id,
        meta: input.meta,
        author_id: Some(ctx.actor_id),
    };
    let record = store::architecture::create_artifact_tx(&mut *tx, ctx.tenant, &new, &entries)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(artifact_out(&record)))
}

/// `architecture.artifacts_update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactUpdateIn {
    /// The artefact.
    pub id: Uuid,
    /// Title.
    #[serde(default)]
    pub title: Option<String>,
    /// The related change initiative.
    #[serde(default)]
    pub change_initiative: Option<String>,
    /// Why it exists.
    #[serde(default)]
    pub purpose: Option<String>,
    /// Stakeholders.
    #[serde(default)]
    pub stakeholders: Option<Value>,
    /// Scope.
    #[serde(default)]
    pub scope: Option<String>,
    /// The published document.
    #[serde(default)]
    pub document_id: Option<Uuid>,
    /// The contract it sits under.
    #[serde(default)]
    pub contract_id: Option<Uuid>,
    /// Type-specific metadata.
    #[serde(default)]
    pub meta: Option<Value>,
    /// Replacement entries; omitted leaves them untouched.
    #[serde(default)]
    pub entries: Option<Vec<ArtifactEntryIn>>,
}

#[crate::action(
    name = "architecture.artifacts_update",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactUpdateIn,
) -> Result<HandlerOutput<ArtifactOut>, ActionError> {
    let current = store::architecture::get_artifact_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture artefact"))?;
    let expected =
        require_precondition_with(&ctx, ctx.action, json!({ "version": current.version }))?;
    let entries = input.entries.map(|entries| {
        entries
            .into_iter()
            .map(|entry| NewEntry {
                axis_a: entry.axis_a,
                axis_b: entry.axis_b,
                value: entry.value,
                position: entry.position,
            })
            .collect()
    });
    let patch = store::architecture::ArtifactPatch {
        title: input.title,
        change_initiative: input.change_initiative,
        purpose: input.purpose,
        stakeholders: input.stakeholders,
        scope: input.scope,
        document_id: input.document_id,
        contract_id: input.contract_id,
        meta: input.meta,
        entries,
    };
    let record = store::architecture::update_artifact_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        &patch,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(artifact_out(&record)).with_etag(etag_of(record.version)))
}

/// `architecture.artifacts_approve`; the approval decision is recorded on the row.
#[crate::action(
    name = "architecture.artifacts_approve",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_approve(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactIdIn,
) -> Result<HandlerOutput<ArtifactOut>, ActionError> {
    let current = store::architecture::get_artifact_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture artefact"))?;
    let expected =
        require_precondition_with(&ctx, ctx.action, json!({ "version": current.version }))?;
    let record = store::architecture::approve_artifact_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        ctx.actor_id,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(artifact_out(&record)).with_etag(etag_of(record.version)))
}

/// `architecture.artifacts_supersede` — writes a new draft version.
#[crate::action(
    name = "architecture.artifacts_supersede",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_supersede(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactIdIn,
) -> Result<HandlerOutput<ArtifactOut>, ActionError> {
    let current = store::architecture::get_artifact_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture artefact"))?;
    let expected =
        require_precondition_with(&ctx, ctx.action, json!({ "version": current.version }))?;
    let record = store::architecture::supersede_artifact_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(artifact_out(&record)))
}

/// `architecture.artifacts_link`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactLinkIn {
    /// From.
    pub from_artifact_id: Uuid,
    /// To.
    pub to_artifact_id: Uuid,
    /// derivation | realization | reuse | dependency.
    pub kind: String,
}

/// One edge as returned.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactLinkOut {
    /// Row id.
    pub id: String,
    /// From.
    pub from_artifact_id: String,
    /// To.
    pub to_artifact_id: String,
    /// The kind.
    pub kind: String,
    /// The row version.
    pub version: i64,
}

fn link_out(record: &ArtifactLinkRecord) -> ArtifactLinkOut {
    ArtifactLinkOut {
        id: record.id.to_string(),
        from_artifact_id: record.from_artifact_id.to_string(),
        to_artifact_id: record.to_artifact_id.to_string(),
        kind: record.kind.clone(),
        version: record.version,
    }
}

#[crate::action(
    name = "architecture.artifacts_link",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_link(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactLinkIn,
) -> Result<HandlerOutput<ArtifactLinkOut>, ActionError> {
    let record = store::architecture::link_artifacts_tx(
        &mut *tx,
        ctx.tenant,
        input.from_artifact_id,
        input.to_artifact_id,
        &input.kind,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(link_out(&record)))
}

/// One catalogue or matrix entry as the actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EntryOut {
    /// Row id.
    pub id: String,
    /// The artefact.
    pub artifact_id: String,
    /// First axis.
    pub axis_a: String,
    /// Second axis (empty for a catalog).
    pub axis_b: String,
    /// The cell's value.
    pub value: Value,
    /// Display order.
    pub position: i32,
    /// The row version.
    pub version: i64,
}

fn entry_out(record: &EntryRecord) -> EntryOut {
    EntryOut {
        id: record.id.to_string(),
        artifact_id: record.artifact_id.to_string(),
        axis_a: record.axis_a.clone(),
        axis_b: record.axis_b.clone(),
        value: record.value.clone(),
        position: record.position,
        version: record.version,
    }
}

/// `architecture.artifacts_entries` — the catalogue items or matrix cells of one artefact.
#[crate::action(
    name = "architecture.artifacts_entries",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_entries(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactIdIn,
) -> Result<HandlerOutput<Page<EntryOut>>, ActionError> {
    store::architecture::get_artifact_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture artefact"))?;
    let rows = store::architecture::list_entries_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(entry_out).collect(),
        next_cursor: None,
    }))
}

/// `architecture.artifacts_links` — the edges touching one artefact, both directions.
#[crate::action(
    name = "architecture.artifacts_links",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_artifacts_links(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactIdIn,
) -> Result<HandlerOutput<Page<ArtifactLinkOut>>, ActionError> {
    store::architecture::get_artifact_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture artefact"))?;
    let rows = store::architecture::list_links_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(link_out).collect(),
        next_cursor: None,
    }))
}

/// `architecture.repository_search`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositorySearchIn {
    /// Free text over title, purpose and metadata.
    #[serde(default)]
    pub query: Option<String>,
    /// Filter by plateau.
    #[serde(default)]
    pub plateau: Option<String>,
    /// Filter by artefact kind.
    #[serde(default)]
    pub kind: Option<String>,
}

#[crate::action(
    name = "architecture.repository_search",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_repository_search(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RepositorySearchIn,
) -> Result<HandlerOutput<Page<ArtifactOut>>, ActionError> {
    let rows = store::architecture::search_repository_tx(
        &mut *tx,
        ctx.tenant,
        input.query.as_deref(),
        input.plateau.as_deref(),
        input.kind.as_deref(),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(artifact_out).collect(),
        next_cursor: None,
    }))
}

/// `architecture.profile_get`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProfileGetIn {
    /// A named profile; omitted returns the adopted one.
    #[serde(default)]
    pub key: Option<String>,
}

/// The tenant's tailored ADM.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProfileOut {
    /// Row id.
    pub id: String,
    /// Stable key.
    pub key: String,
    /// Display name.
    pub name: String,
    /// The phase set.
    pub phase_set: Value,
    /// `phase → plateau → [artifact_type keys]`.
    pub artifact_requirements: Value,
    /// `phase → contract kind`.
    pub contract_kinds: Value,
    /// draft | adopted | retired.
    pub status: String,
    /// When adopted.
    pub adopted_at: Option<DateTime<Utc>>,
    /// The board decision.
    pub board_decision_id: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

fn profile_out(record: &ProfileRecord) -> ProfileOut {
    ProfileOut {
        id: record.id.to_string(),
        key: record.key.clone(),
        name: record.name.clone(),
        phase_set: record.phase_set.clone(),
        artifact_requirements: record.artifact_requirements.clone(),
        contract_kinds: record.contract_kinds.clone(),
        status: record.status.clone(),
        adopted_at: record.adopted_at,
        board_decision_id: record.board_decision_id.map(|id| id.to_string()),
        version: record.version,
    }
}

/// One row of the seeded standard.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactTypeOut {
    /// Row id — what `architecture.artifacts_create` names as `artifact_type_id`.
    pub id: String,
    /// Stable key.
    pub key: String,
    /// Display name.
    pub name: String,
    /// Artefact kind.
    pub artifact_kind: String,
    /// ADM phase.
    pub phase_code: String,
    /// Plateau.
    pub plateau: String,
    /// Whether the standard marks it mandatory.
    pub mandatory: bool,
    /// Display order.
    pub position: i32,
}

fn type_out(record: &ArtifactTypeRecord) -> ArtifactTypeOut {
    ArtifactTypeOut {
        id: record.id.to_string(),
        key: record.key.clone(),
        name: record.name.clone(),
        artifact_kind: record.artifact_kind.clone(),
        phase_code: record.phase_code.clone(),
        plateau: record.plateau.clone(),
        mandatory: record.mandatory,
        position: record.position,
    }
}

/// `architecture.profile_get`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProfileGetOut {
    /// The profile, if one exists.
    pub profile: Option<ProfileOut>,
    /// The tenant's artefact standard.
    pub standard: Vec<ArtifactTypeOut>,
}

#[crate::action(
    name = "architecture.profile_get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_profile_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProfileGetIn,
) -> Result<HandlerOutput<ProfileGetOut>, ActionError> {
    let profile = store::architecture::get_profile_tx(&mut *tx, ctx.tenant, input.key.as_deref())
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    let standard = store::architecture::list_standard_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(ProfileGetOut {
        profile: profile.as_ref().map(profile_out),
        standard: standard.iter().map(type_out).collect(),
    }))
}

/// `architecture.profile_tailor` — seeds the standard and writes the tailored profile.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProfileTailorIn {
    /// Stable key.
    pub key: String,
    /// Display name.
    pub name: String,
    /// The phase set.
    #[serde(default)]
    pub phase_set: Value,
    /// `phase → plateau → [artifact_type keys]`.
    #[serde(default)]
    pub artifact_requirements: Value,
    /// `phase → contract kind`.
    #[serde(default)]
    pub contract_kinds: Value,
    /// When set, the profile is adopted against this board decision.
    #[serde(default)]
    pub adopt_with_decision_id: Option<Uuid>,
}

/// Validate one profile_tailor payload against the tenant's seeded standard.
///
/// A tailored profile may add requirements, never subtract the standard's mandatory artefacts,
/// and it may only name phases it declares in `phase_set`. Refusing here is what stops an
/// adopted profile from starting every phase gate satisfied.
fn validate_profile_tailor(
    standard: &[ArtifactTypeRecord],
    input: &ProfileTailorIn,
) -> Result<(), ActionError> {
    let phases = profile_phase_set(input)?;
    let mut named: HashMap<String, Vec<String>> = HashMap::new();
    match &input.artifact_requirements {
        Value::Null => {}
        Value::Object(requirements) => {
            for (phase, value) in requirements {
                if !phases.contains(phase) {
                    return Err(invalid(
                        "artifact_requirements",
                        "invalid_value",
                        format!("phase {phase:?} is not in this profile's phase_set"),
                    ));
                }
                let keys = named.entry(phase.clone()).or_default();
                match value {
                    Value::Array(items) => {
                        collect_profile_keys(items, phase, keys, standard)?;
                    }
                    Value::Object(plateaus) if !plateaus.is_empty() => {
                        for (plateau, items) in plateaus {
                            if !vocab::is_plateau(plateau) {
                                return Err(invalid(
                                    "artifact_requirements",
                                    "invalid_value",
                                    format!(
                                        "phase {phase:?}'s plateau {plateau:?} is not one of \
                                         psa, hld or sd"
                                    ),
                                ));
                            }
                            let Value::Array(items) = items else {
                                return Err(invalid(
                                    "artifact_requirements",
                                    "invalid_value",
                                    format!(
                                        "phase {phase:?}'s plateau {plateau} must be an array \
                                         of artifact_type keys"
                                    ),
                                ));
                            };
                            collect_profile_keys(items, phase, keys, standard)?;
                        }
                    }
                    _ => {
                        return Err(invalid(
                            "artifact_requirements",
                            "invalid_value",
                            format!(
                                "phase {phase:?}'s requirements must be an array or a plateau map"
                            ),
                        ));
                    }
                }
                if keys.is_empty() {
                    return Err(invalid(
                        "artifact_requirements",
                        "invalid_value",
                        format!("phase {phase:?} names no artefact requirements"),
                    ));
                }
            }
        }
        _ => {
            return Err(invalid(
                "artifact_requirements",
                "invalid_value",
                "artifact_requirements must be an object keyed by phase",
            ));
        }
    }
    // Every mandatory row of a phase the profile claims to cover must still be named. A phase
    // dropped from phase_set is a tailoring decision; a mandatory artefact silently dropped from
    // a phase that stays in the set is a weakened gate.
    for row in standard
        .iter()
        .filter(|row| row.mandatory && phases.contains(&row.phase_code))
    {
        let retained = named
            .get(&row.phase_code)
            .is_some_and(|keys| keys.iter().any(|key| key == &row.key));
        if !retained {
            return Err(invalid(
                "artifact_requirements",
                "invalid_value",
                format!(
                    "the mandatory artifact_type {:?} for phase {:?} is missing from the profile",
                    row.key, row.phase_code
                ),
            ));
        }
    }
    match &input.contract_kinds {
        Value::Null => {}
        Value::Object(kinds) => {
            for (phase, kind) in kinds {
                if !phases.contains(phase) {
                    return Err(invalid(
                        "contract_kinds",
                        "invalid_value",
                        format!("phase {phase:?} is not in this profile's phase_set"),
                    ));
                }
                let Some(kind) = kind.as_str() else {
                    return Err(invalid(
                        "contract_kinds",
                        "invalid_value",
                        format!("phase {phase:?}'s contract kind must be a string"),
                    ));
                };
                if kind != "none" && !vocab::is_contract_kind(kind) {
                    return Err(invalid(
                        "contract_kinds",
                        "invalid_value",
                        format!("{kind:?} is not one of the four contract kinds or none"),
                    ));
                }
            }
        }
        _ => {
            return Err(invalid(
                "contract_kinds",
                "invalid_value",
                "contract_kinds must be an object keyed by phase",
            ));
        }
    }
    Ok(())
}

fn profile_phase_set(input: &ProfileTailorIn) -> Result<Vec<String>, ActionError> {
    let Value::Array(items) = &input.phase_set else {
        return Err(invalid(
            "phase_set",
            "invalid_value",
            "phase_set must be a non-empty array of ADM phase codes",
        ));
    };
    if items.is_empty() {
        return Err(invalid(
            "phase_set",
            "invalid_value",
            "phase_set must name at least one ADM phase",
        ));
    }
    let mut phases: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        let phase = item
            .as_str()
            .filter(|phase| vocab::is_phase(phase))
            .ok_or_else(|| {
                invalid(
                    "phase_set",
                    "invalid_value",
                    format!("{item} is not an ADM phase code"),
                )
            })?;
        if phases.iter().any(|declared| declared.as_str() == phase) {
            return Err(invalid(
                "phase_set",
                "invalid_value",
                format!("phase {phase:?} is repeated"),
            ));
        }
        phases.push(phase.to_owned());
    }
    Ok(phases)
}

fn collect_profile_keys(
    items: &[Value],
    phase: &str,
    keys: &mut Vec<String>,
    standard: &[ArtifactTypeRecord],
) -> Result<(), ActionError> {
    for item in items {
        let Some(key) = item.as_str().filter(|key| !key.trim().is_empty()) else {
            return Err(invalid(
                "artifact_requirements",
                "invalid_value",
                format!("phase {phase:?}'s requirements must be artifact_type keys"),
            ));
        };
        if !standard.iter().any(|row| row.key == key) {
            return Err(invalid(
                "artifact_requirements",
                "invalid_value",
                format!("{key:?} is not an artifact_type of this tenant's standard"),
            ));
        }
        if !keys.iter().any(|seen| seen.as_str() == key) {
            keys.push(key.to_owned());
        }
    }
    Ok(())
}

#[crate::action(
    name = "architecture.profile_tailor",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_profile_tailor(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProfileTailorIn,
) -> Result<HandlerOutput<ProfileOut>, ActionError> {
    store::architecture::seed_standard_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    let standard = store::architecture::list_standard_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    validate_profile_tailor(&standard, &input)?;
    // The two maps are NOT NULL columns; an omitted map is an empty tailoring, not a null write.
    let requirements = if input.artifact_requirements.is_null() {
        json!({})
    } else {
        input.artifact_requirements
    };
    let kinds = if input.contract_kinds.is_null() {
        json!({})
    } else {
        input.contract_kinds
    };
    let new = NewProfile {
        key: input.key,
        name: input.name,
        phase_set: input.phase_set,
        artifact_requirements: requirements,
        contract_kinds: kinds,
    };
    let mut record = store::architecture::upsert_profile_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    if let Some(decision) = input.adopt_with_decision_id {
        record = store::architecture::adopt_profile_tx(
            &mut *tx,
            ctx.tenant,
            record.id,
            record.version,
            decision,
        )
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    }
    Ok(HandlerOutput::ok(profile_out(&record)).with_etag(etag_of(record.version)))
}

/// `architecture.contracts_request`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContractRequestIn {
    /// The governing project.
    pub project_id: Uuid,
    /// statement_of_architecture_work | delivery_contract | statement_of_intent |
    /// request_for_architecture.
    pub kind: String,
    /// The ADM phase it closes.
    pub phase_code: String,
    /// Title.
    pub title: String,
    /// The contract body.
    #[serde(default)]
    pub body: Value,
    /// The contract this replaces, if any.
    #[serde(default)]
    pub supersedes_id: Option<Uuid>,
}

/// A contract as returned.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContractOut {
    /// Row id.
    pub id: String,
    /// The governing project.
    pub project_id: String,
    /// The kind.
    pub kind: String,
    /// The phase it closes.
    pub phase_code: String,
    /// Title.
    pub title: String,
    /// The body.
    pub body: Value,
    /// draft | live | superseded.
    pub status: String,
    /// The approving gate.
    pub approval_id: Option<String>,
    /// Who approved it.
    pub approved_by: Option<String>,
    /// When.
    pub approved_at: Option<DateTime<Utc>>,
    /// Who requested it; `None` on a contract that predates the column.
    pub requested_by: Option<String>,
    /// True when no requester is recorded (a legacy row needing a waiver decision).
    pub requester_unknown: bool,
    /// Supersedes.
    pub supersedes_id: Option<String>,
    /// Superseded by.
    pub superseded_by_id: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

fn contract_out(record: &ContractRecord) -> ContractOut {
    ContractOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        kind: record.kind.clone(),
        phase_code: record.phase_code.clone(),
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status.clone(),
        approval_id: record.approval_id.map(|id| id.to_string()),
        approved_by: record.approved_by.map(|id| id.to_string()),
        approved_at: record.approved_at,
        requested_by: record.requested_by.map(|id| id.to_string()),
        requester_unknown: record.requested_by.is_none(),
        supersedes_id: record.supersedes_id.map(|id| id.to_string()),
        superseded_by_id: record.superseded_by_id.map(|id| id.to_string()),
        version: record.version,
    }
}

#[crate::action(
    name = "architecture.contracts_request",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_contracts_request(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ContractRequestIn,
) -> Result<HandlerOutput<ContractOut>, ActionError> {
    let new = NewContract {
        project_id: input.project_id,
        kind: input.kind,
        phase_code: input.phase_code,
        title: input.title,
        body: input.body,
        requested_by: ctx.actor_id,
        supersedes_id: input.supersedes_id,
    };
    let record = store::architecture::request_contract_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(contract_out(&record)))
}

/// Read one contract.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContractIdIn {
    /// The contract.
    pub id: Uuid,
}

/// `architecture.contracts_get`.
#[crate::action(
    name = "architecture.contracts_get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_contracts_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ContractIdIn,
) -> Result<HandlerOutput<ContractOut>, ActionError> {
    let record = store::architecture::get_contract_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture contract"))?;
    Ok(HandlerOutput::ok(contract_out(&record)).with_etag(etag_of(record.version)))
}

/// `architecture.contracts_list` — the contracts of one project, or the tenant's.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContractListIn {
    /// Filter by the governing project.
    #[serde(default)]
    pub project_id: Option<Uuid>,
}

#[crate::action(
    name = "architecture.contracts_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_contracts_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ContractListIn,
) -> Result<HandlerOutput<Page<ContractOut>>, ActionError> {
    let rows = store::architecture::list_contracts_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(contract_out).collect(),
        next_cursor: None,
    }))
}

/// `architecture.contracts_approve` — approve a draft contract, by an actor other than its
/// requester.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContractApproveIn {
    /// The contract.
    pub id: Uuid,
    /// The board decision that approves it. Required for a delivery contract; a legacy
    /// non-delivery row may omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    /// The published P0 standing-delegation decision output the board acted under.
    pub delegation_ref: Uuid,
}

#[crate::action(
    name = "architecture.contracts_approve",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_contracts_approve(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ContractApproveIn,
) -> Result<HandlerOutput<ContractOut>, ActionError> {
    let current = store::architecture::get_contract_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture contract"))?;
    // The requester is recorded at request time; the approver must be a different actor. A
    // legacy row with no requester is allowed through and reads `requester_unknown: true`.
    if current
        .requested_by
        .is_some_and(|requester| requester == ctx.actor_id)
    {
        return Err(ActionError::new(
            409,
            "requester_is_approver",
            "a contract is approved by an actor other than the one that requested it",
        ));
    }
    // A legacy row has no requester to exclude. It is approved only against a board decision,
    // which is the independent authority the missing requester cannot provide. A delivery
    // contract always needs that decision, and the decision must be about this contract and
    // cite the same standing delegation.
    crate::programme::assert_delegation_ref(&mut *tx, ctx.tenant, input.delegation_ref).await?;
    if (current.requested_by.is_none() || current.kind == "delivery_contract")
        && input.approval_id.is_none()
    {
        return Err(ActionError::new(
            409,
            "board_approval_required",
            "a delivery contract (or a contract with no recorded requester) is approved only \
             against a board decision",
        ));
    }
    if let Some(approval_id) = input.approval_id {
        let board = store::approval::get_approval_tx(&mut *tx, ctx.tenant, approval_id)
            .await
            .map_err(|error| approval_error(error, ctx.action))?
            .ok_or_else(|| ActionError::resource_not_found("board decision"))?;
        if board.approval.gate != "architecture_board"
            || board.approval.status != conveyor_domain::ApprovalStatus::Approved
        {
            return Err(ActionError::new(
                409,
                "board_approval_invalid",
                "the approval is not an approved architecture board decision",
            ));
        }
        let subject_id = board
            .meta
            .get("subject_id")
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse::<Uuid>().ok());
        if board.meta.get("subject_kind").and_then(Value::as_str) != Some("contract")
            || subject_id != Some(input.id)
        {
            return Err(ActionError::new(
                409,
                "board_approval_wrong_subject",
                "the board decision is not about this contract",
            ));
        }
        let board_ref = board
            .meta
            .get("evidence")
            .and_then(|evidence| evidence.get("delegation_ref"))
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse::<Uuid>().ok());
        if board_ref != Some(input.delegation_ref) {
            return Err(ActionError::new(
                409,
                "delegation_ref_mismatch",
                "the board decision cites a different standing delegation",
            ));
        }
    }
    let expected =
        require_precondition_with(&ctx, ctx.action, json!({ "version": current.version }))?;
    let record = store::architecture::approve_contract_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
        ctx.actor_id,
        input.approval_id,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(contract_out(&record)).with_etag(etag_of(record.version)))
}

#[crate::action(
    name = "architecture.contracts_supersede",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_contracts_supersede(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ContractIdIn,
) -> Result<HandlerOutput<ContractOut>, ActionError> {
    let current = store::architecture::get_contract_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such architecture contract"))?;
    let expected =
        require_precondition_with(&ctx, ctx.action, json!({ "version": current.version }))?;
    let record = store::architecture::supersede_contract_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i64::try_from(expected.get()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(contract_out(&record)).with_etag(etag_of(record.version)))
}

/// `architecture.phases_advance`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhaseAdvanceIn {
    /// The architecture project.
    pub project_id: Uuid,
    /// The phase to close.
    pub phase_code: String,
    /// The profile key; omitted uses the adopted profile.
    #[serde(default)]
    pub profile_key: Option<String>,
}

/// The gate's result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PhaseAdvanceOut {
    /// The phase that closed.
    pub closed_phase: String,
    /// The phase now open.
    pub next_phase: Option<String>,
    /// The approved artefacts the gate resolved.
    pub artifact_ids: Vec<String>,
    /// The live contract that closed it.
    pub contract_id: Option<String>,
}

#[crate::action(
    name = "architecture.phases_advance",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_phases_advance(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PhaseAdvanceIn,
) -> Result<HandlerOutput<PhaseAdvanceOut>, ActionError> {
    let advance = store::architecture::advance_phase_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        &input.phase_code,
        input.profile_key.as_deref(),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(PhaseAdvanceOut {
        closed_phase: advance.closed_phase,
        next_phase: advance.next_phase,
        artifact_ids: advance
            .artifact_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        contract_id: advance.contract_id.map(|id| id.to_string()),
    }))
}

/// `architecture.reviews_record`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReviewRecordIn {
    /// The delivery project.
    pub project_id: Uuid,
    /// The governing contract.
    pub contract_id: Uuid,
    /// The one compliance level.
    pub level: String,
    /// Findings.
    #[serde(default)]
    pub findings: Value,
    /// Evidence refs.
    #[serde(default)]
    pub evidence: Value,
    /// Phase H's one typed outcome: `{"kind":"change","change_id":…}` or `{"kind":"kata",…}`.
    #[serde(default)]
    pub outcome: Option<Value>,
    /// The published P0 standing-delegation decision output the reviewer acts under.
    pub delegation_ref: Uuid,
}

/// A review as returned.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReviewOut {
    /// Row id.
    pub id: String,
    /// The project.
    pub project_id: String,
    /// The contract.
    pub contract_id: String,
    /// The level.
    pub level: String,
    /// Findings.
    pub findings: Value,
    /// Evidence.
    pub evidence: Value,
    /// The phase-H outcome.
    pub outcome: Option<Value>,
    /// Who reviewed.
    pub reviewed_by: String,
    /// When.
    pub reviewed_at: DateTime<Utc>,
    /// The If-Match version.
    pub version: i64,
}

fn review_out(record: &ReviewRecord) -> ReviewOut {
    ReviewOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        contract_id: record.contract_id.to_string(),
        level: record.level.clone(),
        findings: record.findings.clone(),
        evidence: record.evidence.clone(),
        outcome: record.outcome.clone(),
        reviewed_by: record.reviewed_by.to_string(),
        reviewed_at: record.reviewed_at,
        version: record.version,
    }
}

/// `architecture.reviews_list`.
///
/// A review is unique per `(project, contract)`, so the record's replay needs to read the row it
/// already wrote before it records one. Filter by project, by contract, or both.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReviewListIn {
    /// Filter by project.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// Filter by contract.
    #[serde(default)]
    pub contract_id: Option<Uuid>,
}

#[crate::action(
    name = "architecture.reviews_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_reviews_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReviewListIn,
) -> Result<HandlerOutput<Page<ReviewOut>>, ActionError> {
    let rows = store::architecture::list_reviews_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        input.contract_id,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(review_out).collect(),
        next_cursor: None,
    }))
}

#[crate::action(
    name = "architecture.reviews_record",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_reviews_record(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReviewRecordIn,
) -> Result<HandlerOutput<ReviewOut>, ActionError> {
    crate::programme::assert_delegation_ref(&mut *tx, ctx.tenant, input.delegation_ref).await?;
    let contract = store::architecture::get_contract_tx(&mut *tx, ctx.tenant, input.contract_id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("architecture contract"))?;
    if contract
        .requested_by
        .is_some_and(|requester| requester == ctx.actor_id)
    {
        return Err(ActionError::new(
            409,
            "requester_is_reviewer",
            "a compliance review is recorded by an actor other than the contract's requester",
        ));
    }
    let evidence = match input.evidence {
        Value::Object(mut map) => {
            map.insert(
                "delegation_ref".to_owned(),
                Value::String(input.delegation_ref.to_string()),
            );
            Value::Object(map)
        }
        Value::Null => json!({ "delegation_ref": input.delegation_ref.to_string() }),
        _ => {
            return Err(invalid(
                "evidence",
                "evidence_object_required",
                "the review evidence must be a JSON object",
            ));
        }
    };
    let new = NewReview {
        project_id: input.project_id,
        contract_id: input.contract_id,
        level: input.level,
        findings: input.findings,
        evidence,
        outcome: input.outcome,
        reviewed_by: ctx.actor_id,
    };
    let record = store::architecture::record_review_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(review_out(&record)))
}

/// `architecture.reviews_get`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReviewIdIn {
    /// The review.
    pub id: Uuid,
}

#[crate::action(
    name = "architecture.reviews_get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_reviews_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReviewIdIn,
) -> Result<HandlerOutput<ReviewOut>, ActionError> {
    let record = store::architecture::get_review_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("compliance review"))?;
    Ok(HandlerOutput::ok(review_out(&record)).with_etag(etag_of(record.version)))
}

/// `architecture.reviews_waive`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WaiverIn {
    /// The review.
    pub review_id: Uuid,
    /// The accepting asset owner.
    #[serde(default)]
    pub owner_user_id: Option<Uuid>,
    /// Why the deviation is accepted.
    pub reason: String,
    /// Additional cost.
    #[serde(default)]
    pub cost_impact: String,
    /// Additional risk.
    #[serde(default)]
    pub risk_impact: String,
    /// The reassessment date.
    #[serde(default)]
    pub reassessment_at: Option<DateTime<Utc>>,
}

/// A waiver as returned.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WaiverOut {
    /// Row id.
    pub id: String,
    /// The review.
    pub review_id: String,
    /// The accepting owner.
    pub owner_user_id: String,
    /// Why.
    pub reason: String,
    /// Additional cost.
    pub cost_impact: String,
    /// Additional risk.
    pub risk_impact: String,
    /// The reassessment date.
    pub reassessment_at: DateTime<Utc>,
    /// active | expired | closed.
    pub status: String,
    /// The row version.
    pub version: i64,
}

fn waiver_out(record: &WaiverRecord) -> WaiverOut {
    WaiverOut {
        id: record.id.to_string(),
        review_id: record.review_id.to_string(),
        owner_user_id: record.owner_user_id.to_string(),
        reason: record.reason.clone(),
        cost_impact: record.cost_impact.clone(),
        risk_impact: record.risk_impact.clone(),
        reassessment_at: record.reassessment_at,
        status: record.status.clone(),
        version: record.version,
    }
}

#[crate::action(
    name = "architecture.reviews_waive",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_reviews_waive(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WaiverIn,
) -> Result<HandlerOutput<WaiverOut>, ActionError> {
    let review = store::architecture::get_review_tx(&mut *tx, ctx.tenant, input.review_id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("compliance review"))?;
    let contract = store::architecture::get_contract_tx(&mut *tx, ctx.tenant, review.contract_id)
        .await
        .map_err(|error| arch_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("architecture contract"))?;
    if contract
        .requested_by
        .is_some_and(|requester| requester == ctx.actor_id)
    {
        return Err(ActionError::new(
            409,
            "requester_is_waiver_owner",
            "the contract's requester cannot waive its compliance review",
        ));
    }
    if review.reviewed_by == ctx.actor_id || input.owner_user_id == Some(review.reviewed_by) {
        return Err(ActionError::new(
            409,
            "reviewer_is_waiver_owner",
            "the reviewer cannot waive their own finding; the accepting owner must be a different actor",
        ));
    }
    let delegation_ref = review
        .evidence
        .get("delegation_ref")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<Uuid>().ok())
        .ok_or_else(|| {
            ActionError::new(
                409,
                "delegation_ref_missing",
                "the review does not cite the standing delegation the waiver acts under",
            )
        })?;
    crate::programme::assert_delegation_ref(&mut *tx, ctx.tenant, delegation_ref).await?;
    let new = NewWaiver {
        review_id: input.review_id,
        owner_user_id: input.owner_user_id,
        reason: input.reason,
        cost_impact: input.cost_impact,
        risk_impact: input.risk_impact,
        reassessment_at: input.reassessment_at,
    };
    let record = store::architecture::waive_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(waiver_out(&record)))
}

/// A governance body as the actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GovernanceBodyOut {
    /// Row id.
    pub id: String,
    /// architecture_board | design_board | other.
    pub kind: String,
    /// Stable key.
    pub key: String,
    /// Display name.
    pub name: String,
    /// The charter.
    pub charter: String,
    /// The commissioning authority.
    pub commissioning_authority: String,
    /// Decision limits.
    pub decision_limits: Value,
    /// Members.
    pub members: Value,
    /// The forum type this body's sittings use; `null` falls back to the body key.
    pub meeting_type_key: Option<String>,
    /// draft | active | retired.
    pub status: String,
    /// The If-Match version.
    pub version: i64,
}

fn body_out(record: &GovernanceBodyRecord) -> GovernanceBodyOut {
    GovernanceBodyOut {
        id: record.id.to_string(),
        kind: record.kind.clone(),
        key: record.key.clone(),
        name: record.name.clone(),
        charter: record.charter.clone(),
        commissioning_authority: record.commissioning_authority.clone(),
        decision_limits: record.decision_limits.clone(),
        members: record.members.clone(),
        meeting_type_key: record.meeting_type_key.clone(),
        status: record.status.clone(),
        version: record.version,
    }
}

fn default_body_kind() -> String {
    "architecture_board".to_owned()
}

fn empty_limits() -> Value {
    json!({})
}

fn empty_members() -> Value {
    json!([])
}

/// `architecture.board_open` — the Board (or another governance forum) becomes a row.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardOpenIn {
    /// architecture_board | design_board | other.
    #[serde(default = "default_body_kind")]
    pub kind: String,
    /// Stable key.
    pub key: String,
    /// Display name.
    pub name: String,
    /// The charter.
    #[serde(default)]
    pub charter: String,
    /// Who commissioned it.
    #[serde(default)]
    pub commissioning_authority: String,
    /// Decision limits.
    #[serde(default = "empty_limits")]
    pub decision_limits: Value,
    /// Members.
    #[serde(default = "empty_members")]
    pub members: Value,
    /// The forum type the body's sittings use; the body's key when omitted.
    #[serde(default)]
    pub meeting_type_key: Option<String>,
}

#[crate::action(
    name = "architecture.board_open",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_board_open(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardOpenIn,
) -> Result<HandlerOutput<GovernanceBodyOut>, ActionError> {
    let meeting_type_key = input.meeting_type_key.or_else(|| Some(input.key.clone()));
    let new = NewBody {
        kind: input.kind,
        key: input.key,
        name: input.name,
        charter: input.charter,
        commissioning_authority: input.commissioning_authority,
        decision_limits: input.decision_limits,
        members: input.members,
        meeting_type_key,
    };
    let record = store::architecture::open_body_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(body_out(&record)))
}

/// `architecture.board_get` — read the governance body by id or by its stable key.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardGetIn {
    /// The body row.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// The body's key; used when `id` is absent.
    #[serde(default)]
    pub key: Option<String>,
}

#[crate::action(
    name = "architecture.board_get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_board_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardGetIn,
) -> Result<HandlerOutput<GovernanceBodyOut>, ActionError> {
    let record = match (input.id, input.key.as_deref()) {
        (Some(id), _) => store::architecture::get_body_tx(&mut *tx, ctx.tenant, id).await,
        (None, Some(key)) => {
            store::architecture::get_body_by_key_tx(&mut *tx, ctx.tenant, key).await
        }
        (None, None) => {
            return Err(invalid(
                "key",
                "required",
                "supply the body's id or its stable key",
            ));
        }
    }
    .map_err(|error| arch_error(error, ctx.action))?
    .ok_or_else(|| ActionError::resource_not_found("such governance body"))?;
    Ok(HandlerOutput::ok(body_out(&record)))
}

/// `architecture.board_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardListIn {
    /// architecture_board | design_board | other; every kind when omitted.
    #[serde(default)]
    pub kind: Option<String>,
}

#[crate::action(
    name = "architecture.board_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_board_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardListIn,
) -> Result<HandlerOutput<Page<GovernanceBodyOut>>, ActionError> {
    let rows = store::architecture::list_bodies_tx(&mut *tx, ctx.tenant, input.kind.as_deref())
        .await
        .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(body_out).collect(),
        next_cursor: None,
    }))
}

/// `architecture.board_decide`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardDecideIn {
    /// The governance body.
    pub body_id: Uuid,
    /// approved | rejected.
    pub decision: String,
    /// The evidence the decision was taken on.
    #[serde(default)]
    pub evidence: Value,
    /// What the decision is about: `board`, `contract`, `artifact`, `adm_profile` or `waiver`.
    /// A decision about the board itself (`board`) with `approved` activates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_kind: Option<String>,
    /// The subject's id; absent only for a decision about the board itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<Uuid>,
    /// The published P0 standing-delegation decision output a delegated rule decision acts
    /// under. Present ⇒ the decision is recorded as an `approval_decision` row (approver,
    /// `decided_by_kind`, `via.delegation_ref`) exactly as `approvals.decide` records a
    /// decision; absent ⇒ the legacy board row only, with no decision row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_ref: Option<Uuid>,
}

/// The recorded decision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardDecisionOut {
    /// The approval row that records it.
    pub approval_id: String,
}

#[crate::action(
    name = "architecture.board_decide",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_board_decide(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardDecideIn,
) -> Result<HandlerOutput<BoardDecisionOut>, ActionError> {
    // A delegated decision cites the owner's published standing delegation. The reference is
    // validated against the real P0 decision document and copied into the decision's evidence,
    // so `contracts_approve` can prove the board acted under the same delegation it names.
    let mut evidence = input.evidence;
    if let Some(delegation_ref) = input.delegation_ref {
        crate::programme::assert_delegation_ref(&mut *tx, ctx.tenant, delegation_ref).await?;
        let cited = evidence
            .get("delegation_ref")
            .and_then(Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok());
        if cited.is_some_and(|cited| cited != delegation_ref) {
            return Err(ActionError::new(
                409,
                "delegation_ref_mismatch",
                "the evidence cites a different standing delegation",
            ));
        }
        if !evidence.is_object() {
            evidence = json!({});
        }
        evidence["delegation_ref"] = Value::String(delegation_ref.to_string());
    }
    let approval_id = store::architecture::decide_board_tx(
        &mut *tx,
        ctx.tenant,
        input.body_id,
        &input.decision,
        evidence,
        store::architecture::BoardSubject {
            kind: input.subject_kind.as_deref(),
            id: input.subject_id,
        },
        store::architecture::BoardDecider {
            decided_by: ctx.actor_id,
            decided_by_kind: &delegated_decider_kind(&ctx.actor_kind),
            delegation_ref: input.delegation_ref,
        },
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(BoardDecisionOut {
        approval_id: approval_id.to_string(),
    }))
}

/// The `approval_decision.decided_by_kind` of a delegated board act.
///
/// A service principal invoking the deterministic board macro under the standing delegation is
/// the delegated **rule** approver (the owner's ruling); an agent is an agent; a person is a
/// human. The kind is derived from the authenticated principal, never supplied by the caller.
fn delegated_decider_kind(actor_kind: &str) -> String {
    match actor_kind {
        "service" => "rule",
        "agent" => "agent",
        _ => "human",
    }
    .to_owned()
}

/// A board sitting opened (or resumed) over one subject.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardSitOpenIn {
    /// The governing body.
    pub body_id: Uuid,
    /// contract | artifact | adm_profile | waiver.
    pub subject_kind: String,
    /// The subject row.
    pub subject_id: Uuid,
    /// The lane the sitting card is placed in.
    pub list_id: Uuid,
    /// The evidence the subject is decided on, as an array.
    #[serde(default)]
    pub evidence: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardSitOpenOut {
    /// The sitting meeting.
    pub meeting_id: String,
    /// The pending approval its agenda item decides.
    pub approval_id: String,
}

/// The decision a sitting item resolved to.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardSitDecideIn {
    /// The governing body.
    pub body_id: Uuid,
    /// The sitting.
    pub meeting_id: Uuid,
    /// The item to close.
    pub agenda_item_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardSitDecideOut {
    /// The sitting.
    pub meeting_id: String,
    /// The closed item.
    pub agenda_item_id: String,
    /// The approval the item decided.
    pub approval_id: String,
    /// The approval's derived status.
    pub approval_status: Option<String>,
    /// The decision rows written, one per qualifying seat.
    pub decisions_written: usize,
    /// The positions that counted after the rule and the conflict exclusion.
    pub counted_positions: usize,
    /// The attendance quorum the type required.
    pub quorum: usize,
}

/// `architecture.board_sit_open` — the body sits over a subject: a pending approval, a live
/// meeting with the body's standing type, and the approval as its deciding item.
#[crate::action(
    name = "architecture.board_sit_open",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_board_sit_open(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardSitOpenIn,
) -> Result<HandlerOutput<BoardSitOpenOut>, ActionError> {
    let evidence = if input.evidence.is_null() {
        json!([])
    } else {
        input.evidence
    };
    let sitting = store::architecture::open_board_sitting_tx(
        &mut *tx,
        ctx.tenant,
        store::meetings::MeetingActor {
            id: ctx.actor_id,
            role: ctx.role.as_str(),
        },
        input.body_id,
        &input.subject_kind,
        input.subject_id,
        input.list_id,
        &evidence,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(BoardSitOpenOut {
        meeting_id: sitting.meeting_id.to_string(),
        approval_id: sitting.approval_id.to_string(),
    }))
}

/// `architecture.board_sit_decide` — close the sitting's item through T19's arithmetic.
#[crate::action(
    name = "architecture.board_sit_decide",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_board_sit_decide(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardSitDecideIn,
) -> Result<HandlerOutput<BoardSitDecideOut>, ActionError> {
    let result = store::architecture::decide_board_sitting_tx(
        &mut *tx,
        ctx.tenant,
        input.body_id,
        input.meeting_id,
        input.agenda_item_id,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(BoardSitDecideOut {
        meeting_id: input.meeting_id.to_string(),
        agenda_item_id: input.agenda_item_id.to_string(),
        approval_id: result.item.resolved_id.to_string(),
        approval_status: result.approval_status,
        decisions_written: result.decisions_written,
        counted_positions: result.counted_positions,
        quorum: result.quorum,
    }))
}

/// One dimension or category score.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScoreIn {
    /// The dimension or category key.
    pub key: String,
    /// 0–5.
    pub score: i32,
}

/// `architecture.maturity_assess`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityAssessIn {
    /// The assessment group; generated when omitted.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// The capability stage.
    pub stage: String,
    /// All nine dimensions, once each.
    pub scores: Vec<ScoreIn>,
    /// Evidence refs.
    #[serde(default)]
    pub evidence: Value,
}

/// The assessed dimensions.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityOut {
    /// The assessment group.
    pub assessment_id: String,
    /// The stage.
    pub stage: String,
    /// The dimensions.
    pub dimensions: Vec<ScoreOut>,
}

/// One recorded score.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScoreOut {
    /// The dimension or category.
    pub key: String,
    /// The score.
    pub score: i32,
}

#[crate::action(
    name = "architecture.maturity_assess",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_maturity_assess(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MaturityAssessIn,
) -> Result<HandlerOutput<MaturityOut>, ActionError> {
    let assessment_id = input.assessment_id.unwrap_or_else(Uuid::now_v7);
    let scores: Vec<(String, i32)> = input
        .scores
        .iter()
        .map(|score| (score.key.clone(), score.score))
        .collect();
    let rows = store::architecture::assess_maturity_tx(
        &mut *tx,
        ctx.tenant,
        assessment_id,
        &input.stage,
        &scores,
        input.evidence,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(MaturityOut {
        assessment_id: assessment_id.to_string(),
        stage: input.stage,
        dimensions: rows
            .iter()
            .map(|row: &MaturityRecord| ScoreOut {
                key: row.dimension.clone(),
                score: row.score,
            })
            .collect(),
    }))
}

/// `architecture.skills_assess`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SkillsAssessIn {
    /// The assessment group; generated when omitted.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// The role.
    pub role: String,
    /// The job key, if the role is catalogued.
    #[serde(default)]
    pub job_key: String,
    /// All seven categories, once each.
    pub scores: Vec<ScoreIn>,
    /// Evidence refs.
    #[serde(default)]
    pub evidence: Value,
}

/// The assessed categories.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SkillsOut {
    /// The assessment group.
    pub assessment_id: String,
    /// The role.
    pub role: String,
    /// The categories.
    pub categories: Vec<ScoreOut>,
}

#[crate::action(
    name = "architecture.skills_assess",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_skills_assess(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SkillsAssessIn,
) -> Result<HandlerOutput<SkillsOut>, ActionError> {
    let assessment_id = input.assessment_id.unwrap_or_else(Uuid::now_v7);
    let scores: Vec<(String, i32)> = input
        .scores
        .iter()
        .map(|score| (score.key.clone(), score.score))
        .collect();
    let rows = store::architecture::assess_skills_tx(
        &mut *tx,
        ctx.tenant,
        assessment_id,
        &input.role,
        &input.job_key,
        &scores,
        input.evidence,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(SkillsOut {
        assessment_id: assessment_id.to_string(),
        role: input.role,
        categories: rows
            .iter()
            .map(|row: &SkillRecord| ScoreOut {
                key: row.category.clone(),
                score: row.proficiency,
            })
            .collect(),
    }))
}

/// `architecture.maturity_plan`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityPlanIn {
    /// The capability's project.
    pub project_id: Uuid,
    /// The assessment the plan answers.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// The development milestones.
    pub milestones: Vec<String>,
}

/// The recorded milestones.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityPlanOut {
    /// The WBS milestone ids.
    pub node_ids: Vec<String>,
}

#[crate::action(
    name = "architecture.maturity_plan",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_maturity_plan(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MaturityPlanIn,
) -> Result<HandlerOutput<MaturityPlanOut>, ActionError> {
    let ids = store::architecture::plan_maturity_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        input.assessment_id,
        &input.milestones,
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    Ok(HandlerOutput::created(MaturityPlanOut {
        node_ids: ids.iter().map(ToString::to_string).collect(),
    }))
}

/// `architecture.maturity_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityListIn {
    /// One assessment group; every group when omitted.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// One stage; every stage when omitted.
    #[serde(default)]
    pub stage: Option<String>,
}

#[crate::action(
    name = "architecture.maturity_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_maturity_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MaturityListIn,
) -> Result<HandlerOutput<Page<MaturityOut>>, ActionError> {
    let rows = store::architecture::list_maturity_tx(
        &mut *tx,
        ctx.tenant,
        input.assessment_id,
        input.stage.as_deref(),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    let mut out: Vec<MaturityOut> = Vec::new();
    let mut index_of: HashMap<Uuid, usize> = HashMap::new();
    for row in rows {
        let index = match index_of.get(&row.assessment_id) {
            Some(index) => *index,
            None => {
                out.push(MaturityOut {
                    assessment_id: row.assessment_id.to_string(),
                    stage: row.stage.clone(),
                    dimensions: Vec::new(),
                });
                index_of.insert(row.assessment_id, out.len() - 1);
                out.len() - 1
            }
        };
        out[index].dimensions.push(ScoreOut {
            key: row.dimension,
            score: row.score,
        });
    }
    Ok(HandlerOutput::ok(Page {
        items: out,
        next_cursor: None,
    }))
}

/// `architecture.skills_list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SkillsListIn {
    /// One assessment group; every group when omitted.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// One role; every role when omitted.
    #[serde(default)]
    pub role: Option<String>,
}

#[crate::action(
    name = "architecture.skills_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn architecture_skills_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SkillsListIn,
) -> Result<HandlerOutput<Page<SkillsOut>>, ActionError> {
    let rows = store::architecture::list_skills_tx(
        &mut *tx,
        ctx.tenant,
        input.assessment_id,
        input.role.as_deref(),
    )
    .await
    .map_err(|error| arch_error(error, ctx.action))?;
    let mut out: Vec<SkillsOut> = Vec::new();
    let mut index_of: HashMap<(Uuid, String), usize> = HashMap::new();
    for row in rows {
        let key = (row.assessment_id, row.role.clone());
        let index = match index_of.get(&key) {
            Some(index) => *index,
            None => {
                out.push(SkillsOut {
                    assessment_id: row.assessment_id.to_string(),
                    role: row.role.clone(),
                    categories: Vec::new(),
                });
                index_of.insert(key, out.len() - 1);
                out.len() - 1
            }
        };
        out[index].categories.push(ScoreOut {
            key: row.category,
            score: row.proficiency,
        });
    }
    Ok(HandlerOutput::ok(Page {
        items: out,
        next_cursor: None,
    }))
}
