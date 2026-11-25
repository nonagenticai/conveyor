//! The TOGAF ADM store: the artefact repository, the tailored profile, the board, the four
//! contract kinds, the phase gates, compliance reviews and waivers, maturity and skills
//! (SPEC_V2 §8.1, §10.3, D63; T9, issue #81).
//!
//! The rules this module enforces, not merely stores:
//!
//! - an approved artefact is immutable: an update is refused and a supersede writes a new
//!   version that cites the old one, so the approved body never changes;
//! - a phase advances only when every mandatory artefact its profile names is approved, the
//!   phase's contract is live, every earlier phase is accepted, and (for H) exactly one typed
//!   outcome is recorded — and a refusal writes nothing;
//! - one compliance level per delivery contract: a second level is a conflict, not an edit;
//! - a waiver without an accepting owner or a reassessment date is refused;
//! - a maturity assessment covers all nine dimensions, a skills assessment all seven
//!   categories, each exactly once.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::architecture as vocab;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

/// Why an architecture write or read failed.
#[derive(Debug, thiserror::Error)]
pub enum ArchitectureStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// No such row in this tenant.
    #[error("the {0} does not exist in this tenant")]
    NotFound(&'static str),
    /// The row moved underneath the caller.
    #[error("the {0} moved; the caller's version is stale")]
    Stale(&'static str),
    /// A rule of the capability refused the write.
    #[error(transparent)]
    Illegal(#[from] ArchitectureRule),
    /// A uniqueness rule refused the write.
    #[error("the {what} conflicts: {why}")]
    Conflict {
        /// The row kind.
        what: &'static str,
        /// Why.
        why: &'static str,
    },
    /// A meeting the board sitting delegates to refused the write.
    #[error(transparent)]
    Meeting(#[from] crate::meetings::MeetingStoreError),
}

/// The capability's refusal vocabulary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArchitectureRule {
    /// An approved artefact cannot be edited; supersede it instead.
    #[error("an approved artefact is immutable; supersede it with a new version")]
    ApprovedImmutable,
    /// A phase is missing a mandatory approved artefact.
    #[error("the phase is missing the mandatory approved artefact {0:?}")]
    MissingMandatory(String),
    /// The phase's contract is not live.
    #[error("the phase's contract {0:?} is not live")]
    ContractNotLive(String),
    /// The project has no live governing contract at its design gate.
    #[error("the delivery project has no live architecture contract")]
    DesignContractMissing,
    /// The phase is not the next one.
    #[error("phase {requested:?} cannot follow {current:?}")]
    PhaseOutOfOrder {
        /// The phase the project is on.
        current: String,
        /// The requested phase.
        requested: String,
    },
    /// A phase-H review has no typed outcome.
    #[error("the phase-H review has no recorded change or kata outcome")]
    OutcomeRequired,
    /// The outcome names a change that is not an approved change row of the project.
    #[error("the outcome's change {0} is not an approved change row of this project")]
    OutcomeNotApproved(Uuid),
    /// The score set is not the standard's full set.
    #[error("the {what} assessment is missing or repeats {value:?}")]
    AssessmentIncomplete {
        /// `maturity` or `skills`.
        what: &'static str,
        /// The missing or repeated key.
        value: String,
    },
    /// A score is outside 0–5.
    #[error("the score {0} is outside 0–5")]
    ScoreOutOfRange(i32),
    /// A waiver has no owner or no reassessment date.
    #[error("a waiver needs an accepting owner and a reassessment date")]
    WaiverNeedsOwnerAndDate,
    /// The tenant has no ADM profile to gate against.
    #[error("the tenant has no ADM profile; tailor one before advancing a phase")]
    NoProfile,
}

/// An artefact row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ArtifactRecord {
    /// Row id.
    pub id: Uuid,
    /// Owning project, if any.
    pub project_id: Option<Uuid>,
    /// The standard type, if any.
    pub artifact_type_id: Option<Uuid>,
    /// ADM phase.
    pub phase_code: String,
    /// Plateau.
    pub plateau: String,
    /// catalog | matrix | diagram | principles_catalog.
    pub artifact_kind: String,
    /// Title.
    pub title: String,
    /// draft | in_review | approved | superseded | archived.
    pub status: String,
    /// The artefact's own version (bumped by a supersede).
    pub artifact_version: i64,
    /// Author.
    pub author_id: Option<Uuid>,
    /// Authored at.
    pub authored_at: Option<DateTime<Utc>>,
    /// Reviewer.
    pub reviewer_id: Option<Uuid>,
    /// Reviewed at.
    pub reviewed_at: Option<DateTime<Utc>>,
    /// The related change initiative or project.
    pub change_initiative: String,
    /// Why it exists.
    pub purpose: String,
    /// Stakeholders.
    pub stakeholders: Value,
    /// Scope.
    pub scope: String,
    /// The published document.
    pub document_id: Option<Uuid>,
    /// The contract it sits under.
    pub contract_id: Option<Uuid>,
    /// The artefact this version supersedes.
    pub supersedes_id: Option<Uuid>,
    /// The artefact that supersedes this one.
    pub superseded_by_id: Option<Uuid>,
    /// Type-specific metadata (uid/name/use, axes, viewpoint).
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// One catalog item or matrix cell.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EntryRecord {
    /// Row id.
    pub id: Uuid,
    /// The artefact.
    pub artifact_id: Uuid,
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

/// One artefact-to-artefact edge.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ArtifactLinkRecord {
    /// Row id.
    pub id: Uuid,
    /// From.
    pub from_artifact_id: Uuid,
    /// To.
    pub to_artifact_id: Uuid,
    /// derivation | realization | reuse | dependency.
    pub kind: String,
    /// The row version.
    pub version: i64,
}

/// The seeded artefact standard's row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ArtifactTypeRecord {
    /// Row id.
    pub id: Uuid,
    /// Stable key.
    pub key: String,
    /// Display name.
    pub name: String,
    /// catalog | matrix | diagram | principles_catalog.
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

/// The tenant's tailored ADM.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ProfileRecord {
    /// Row id.
    pub id: Uuid,
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
    pub board_decision_id: Option<Uuid>,
    /// The If-Match version.
    pub version: i64,
}

/// A governance body.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct GovernanceBodyRecord {
    /// Row id.
    pub id: Uuid,
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
    /// The forum type this body's sittings use; NULL falls back to the body's own key.
    pub meeting_type_key: Option<String>,
    /// draft | active | retired.
    pub status: String,
    /// The If-Match version.
    pub version: i64,
}

/// An architecture contract.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ContractRecord {
    /// Row id.
    pub id: Uuid,
    /// The governing project.
    pub project_id: Uuid,
    /// The four kinds.
    pub kind: String,
    /// The ADM phase it closes.
    pub phase_code: String,
    /// Title.
    pub title: String,
    /// The contract body.
    pub body: Value,
    /// draft | live | superseded.
    pub status: String,
    /// The approving gate.
    pub approval_id: Option<Uuid>,
    /// Who approved it.
    pub approved_by: Option<Uuid>,
    /// When.
    pub approved_at: Option<DateTime<Utc>>,
    /// Who requested it; `None` on a contract that predates the column.
    pub requested_by: Option<Uuid>,
    /// Supersedes.
    pub supersedes_id: Option<Uuid>,
    /// Superseded by.
    pub superseded_by_id: Option<Uuid>,
    /// The If-Match version.
    pub version: i64,
}

/// One compliance review.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ReviewRecord {
    /// Row id.
    pub id: Uuid,
    /// The delivery project.
    pub project_id: Uuid,
    /// The governing contract.
    pub contract_id: Uuid,
    /// The one level.
    pub level: String,
    /// Findings.
    pub findings: Value,
    /// Evidence refs.
    pub evidence: Value,
    /// For phase H: `{ "kind": "change" | "kata", ... }`.
    pub outcome: Option<Value>,
    /// Who reviewed.
    pub reviewed_by: Uuid,
    /// When.
    pub reviewed_at: DateTime<Utc>,
    /// The If-Match version.
    pub version: i64,
}

/// A time-boxed accepted deviation.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct WaiverRecord {
    /// Row id.
    pub id: Uuid,
    /// The review it attaches to.
    pub review_id: Uuid,
    /// The accepting asset owner.
    pub owner_user_id: Uuid,
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
    /// The If-Match version.
    pub version: i64,
}

/// One maturity dimension's score.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct MaturityRecord {
    /// Row id.
    pub id: Uuid,
    /// The assessment group.
    pub assessment_id: Uuid,
    /// The stage.
    pub stage: String,
    /// The dimension.
    pub dimension: String,
    /// 0–5.
    pub score: i32,
    /// When measured.
    pub assessed_at: DateTime<Utc>,
    /// Evidence.
    pub evidence: Value,
}

/// One skills category's proficiency.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct SkillRecord {
    /// Row id.
    pub id: Uuid,
    /// The assessment group.
    pub assessment_id: Uuid,
    /// The role.
    pub role: String,
    /// The job key.
    pub job_key: String,
    /// The category.
    pub category: String,
    /// 0–5.
    pub proficiency: i32,
    /// When measured.
    pub assessed_at: DateTime<Utc>,
    /// Evidence.
    pub evidence: Value,
}

/// What a phase advance returns.
#[derive(Clone, Debug)]
pub struct PhaseAdvance {
    /// The phase that closed.
    pub closed_phase: String,
    /// The phase now open, if any.
    pub next_phase: Option<String>,
    /// The approved artefact ids the gate resolved.
    pub artifact_ids: Vec<Uuid>,
    /// The live contract that closed it, if one was required.
    pub contract_id: Option<Uuid>,
}

const ARTIFACT_COLUMNS: &str = "id, project_id, artifact_type_id, phase_code, plateau, \
    artifact_kind, title, status, artifact_version, author_id, authored_at, reviewer_id, \
    reviewed_at, change_initiative, purpose, stakeholders, scope, document_id, contract_id, \
    supersedes_id, superseded_by_id, meta, version, created_at, updated_at";
const ENTRY_COLUMNS: &str = "id, artifact_id, axis_a, axis_b, value, position, version";
const LINK_COLUMNS: &str = "id, from_artifact_id, to_artifact_id, kind, version";
const TYPE_COLUMNS: &str = "id, key, name, artifact_kind, phase_code, plateau, mandatory, position";
const PROFILE_COLUMNS: &str = "id, key, name, phase_set, artifact_requirements, contract_kinds, \
    status, adopted_at, board_decision_id, version";
const BODY_COLUMNS: &str = "id, kind, key, name, charter, commissioning_authority, \
    decision_limits, members, meeting_type_key, status, version";
const CONTRACT_COLUMNS: &str = "id, project_id, kind, phase_code, title, body, status, \
    approval_id, approved_by, approved_at, requested_by, supersedes_id, superseded_by_id, version";
const REVIEW_COLUMNS: &str = "id, project_id, contract_id, level, findings, evidence, outcome, \
    reviewed_by, reviewed_at, version";
const WAIVER_COLUMNS: &str = "id, review_id, owner_user_id, reason, cost_impact, risk_impact, \
    reassessment_at, status, version";
const MATURITY_COLUMNS: &str = "id, assessment_id, stage, dimension, score, assessed_at, evidence";
const SKILL_COLUMNS: &str = "id, assessment_id, role, job_key, category, proficiency, \
    assessed_at, evidence";

/// One entry supplied by a create/update.
#[derive(Clone, Debug)]
pub struct NewEntry {
    /// First axis.
    pub axis_a: String,
    /// Second axis (empty for a catalog).
    pub axis_b: String,
    /// The cell's value.
    pub value: Value,
    /// Display order.
    pub position: i32,
}

/// A new artefact.
#[derive(Clone, Debug)]
pub struct NewArtifact {
    /// The owning project.
    pub project_id: Option<Uuid>,
    /// The standard type.
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
    pub change_initiative: Option<String>,
    /// Why it exists.
    pub purpose: Option<String>,
    /// Stakeholders.
    pub stakeholders: Option<Value>,
    /// Scope.
    pub scope: Option<String>,
    /// The published document.
    pub document_id: Option<Uuid>,
    /// The contract it sits under.
    pub contract_id: Option<Uuid>,
    /// Type-specific metadata.
    pub meta: Option<Value>,
    /// The author.
    pub author_id: Option<Uuid>,
}

/// The fields an update may change.
#[derive(Clone, Debug, Default)]
pub struct ArtifactPatch {
    /// Title.
    pub title: Option<String>,
    /// The related change initiative.
    pub change_initiative: Option<String>,
    /// Why it exists.
    pub purpose: Option<String>,
    /// Stakeholders.
    pub stakeholders: Option<Value>,
    /// Scope.
    pub scope: Option<String>,
    /// The document.
    pub document_id: Option<Uuid>,
    /// The contract.
    pub contract_id: Option<Uuid>,
    /// Type-specific metadata.
    pub meta: Option<Value>,
    /// Replacement entries; `None` leaves them untouched.
    pub entries: Option<Vec<NewEntry>>,
}

/// Insert the starter artefact standard if the tenant has none; returns rows inserted.
pub async fn seed_standard_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<u64, ArchitectureStoreError> {
    let rows: [(&str, &str, &str, &str, &str, bool); 14] = [
        (
            "principles_catalog",
            "Principles Catalog",
            "principles_catalog",
            "preliminary",
            "psa",
            true,
        ),
        (
            "principle_derivation",
            "Principle Derivation Diagram",
            "diagram",
            "preliminary",
            "psa",
            true,
        ),
        (
            "principle_realization",
            "Principle Realization Diagram",
            "diagram",
            "preliminary",
            "psa",
            true,
        ),
        (
            "stakeholder_map",
            "Stakeholder Map",
            "matrix",
            "a",
            "psa",
            true,
        ),
        (
            "driver_goal_objective",
            "Driver/Goal/Objective Catalog",
            "catalog",
            "a",
            "psa",
            true,
        ),
        (
            "requirements_catalog",
            "Requirements Catalog",
            "catalog",
            "a",
            "psa",
            true,
        ),
        (
            "solution_concept",
            "Solution Concept Diagram",
            "diagram",
            "a",
            "psa",
            true,
        ),
        (
            "business_capability_map",
            "Business Capability Map",
            "diagram",
            "b",
            "hld",
            true,
        ),
        (
            "data_entity_catalog",
            "Data Entity/Data Component Catalog",
            "catalog",
            "c",
            "hld",
            true,
        ),
        (
            "technology_standards",
            "Technology Standards Catalog",
            "catalog",
            "d",
            "hld",
            true,
        ),
        // E and F close on the solution and migration artefacts §8.1 names. They were absent
        // from the first seeded standard, so a phase E/F gate had no mandatory artefact it
        // could require; the terminal acceptance (T12) needs every phase's set to be real.
        (
            "project_context",
            "Project Context Diagram",
            "diagram",
            "e",
            "sd",
            true,
        ),
        (
            "benefits_diagram",
            "Benefits Diagram",
            "diagram",
            "e",
            "sd",
            true,
        ),
        (
            "implementation_roadmap",
            "Implementation Roadmap",
            "catalog",
            "f",
            "sd",
            true,
        ),
        (
            "migration_plan",
            "Migration Plan",
            "catalog",
            "f",
            "sd",
            true,
        ),
    ];
    let mut inserted = 0_u64;
    for (position, (key, name, kind, phase, plateau, mandatory)) in rows.iter().enumerate() {
        let result = sqlx::query(
            "INSERT INTO artifact_type
                (tenant_id, key, name, artifact_kind, phase_code, plateau, mandatory, position)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (tenant_id, key, phase_code, plateau) DO NOTHING",
        )
        .bind(tenant.as_uuid())
        .bind(key)
        .bind(name)
        .bind(kind)
        .bind(phase)
        .bind(plateau)
        .bind(mandatory)
        .bind(position as i32)
        .execute(&mut *conn)
        .await?;
        inserted += result.rows_affected();
    }
    Ok(inserted)
}

/// List the seeded standard.
pub async fn list_standard_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<ArtifactTypeRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {TYPE_COLUMNS} FROM artifact_type
          WHERE tenant_id = $1 ORDER BY phase_code, plateau, position, key"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?)
}

async fn insert_entries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    artifact_id: Uuid,
    entries: &[NewEntry],
) -> Result<(), ArchitectureStoreError> {
    for entry in entries {
        if entry.axis_a.trim().is_empty() {
            return Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::MissingMandatory("entry axis_a".to_owned()),
            ));
        }
        sqlx::query(
            "INSERT INTO architecture_artifact_entry
                (tenant_id, artifact_id, axis_a, axis_b, value, position)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (tenant_id, artifact_id, axis_a, axis_b)
             DO UPDATE SET value = EXCLUDED.value, position = EXCLUDED.position,
                           version = architecture_artifact_entry.version + 1, updated_at = now()",
        )
        .bind(tenant.as_uuid())
        .bind(artifact_id)
        .bind(entry.axis_a.trim())
        .bind(entry.axis_b.trim())
        .bind(&entry.value)
        .bind(entry.position)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Create an artefact and its entries.
pub async fn create_artifact_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewArtifact,
    entries: &[NewEntry],
) -> Result<ArtifactRecord, ArchitectureStoreError> {
    vocab::require_phase(&new.phase_code).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "phase {}",
            new.phase_code
        )))
    })?;
    vocab::require_plateau(&new.plateau).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "plateau {}",
            new.plateau
        )))
    })?;
    vocab::require_artifact_kind(&new.artifact_kind).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "artefact kind {}",
            new.artifact_kind
        )))
    })?;
    let sql = format!(
        "INSERT INTO architecture_artifact
            (tenant_id, project_id, artifact_type_id, phase_code, plateau, artifact_kind,
             title, change_initiative, purpose, stakeholders, scope, document_id, contract_id,
             meta, author_id, authored_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
         RETURNING {ARTIFACT_COLUMNS}"
    );
    let record: ArtifactRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.project_id)
        .bind(new.artifact_type_id)
        .bind(&new.phase_code)
        .bind(&new.plateau)
        .bind(&new.artifact_kind)
        .bind(new.title.trim())
        .bind(new.change_initiative.clone().unwrap_or_default())
        .bind(new.purpose.clone().unwrap_or_default())
        .bind(new.stakeholders.clone().unwrap_or_else(|| json!([])))
        .bind(new.scope.clone().unwrap_or_default())
        .bind(new.document_id)
        .bind(new.contract_id)
        .bind(new.meta.clone().unwrap_or_else(|| json!({})))
        .bind(new.author_id)
        .fetch_one(&mut *conn)
        .await?;
    insert_entries_tx(&mut *conn, tenant, record.id, entries).await?;
    Ok(record)
}

/// Read one artefact.
pub async fn get_artifact_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ArtifactRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {ARTIFACT_COLUMNS} FROM architecture_artifact WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List artefacts, optionally filtered.
pub async fn list_artifacts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Option<Uuid>,
    phase_code: Option<&str>,
    plateau: Option<&str>,
) -> Result<Vec<ArtifactRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {ARTIFACT_COLUMNS} FROM architecture_artifact
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR project_id = $2)
            AND ($3::text IS NULL OR phase_code = $3)
            AND ($4::text IS NULL OR plateau = $4)
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(phase_code)
        .bind(plateau)
        .fetch_all(&mut *conn)
        .await?)
}

/// List an artefact's entries.
pub async fn list_entries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    artifact_id: Uuid,
) -> Result<Vec<EntryRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {ENTRY_COLUMNS} FROM architecture_artifact_entry
          WHERE tenant_id = $1 AND artifact_id = $2 ORDER BY position, axis_a, axis_b"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(artifact_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// List the edges touching an artefact, both directions, so the inspector can show what it
/// derives from, realises, reuses and depends on — and what derives from it.
pub async fn list_links_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    artifact_id: Uuid,
) -> Result<Vec<ArtifactLinkRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {LINK_COLUMNS} FROM architecture_artifact_link
          WHERE tenant_id = $1 AND (from_artifact_id = $2 OR to_artifact_id = $2)
          ORDER BY kind, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(artifact_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Edit a draft artefact; an approved row is refused.
pub async fn update_artifact_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    patch: &ArtifactPatch,
) -> Result<ArtifactRecord, ArchitectureStoreError> {
    let current = get_artifact_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(ArchitectureStoreError::NotFound("architecture artefact"))?;
    if current.version != expected {
        return Err(ArchitectureStoreError::Stale("architecture artefact"));
    }
    if current.status != "draft" && current.status != "in_review" {
        return Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::ApprovedImmutable,
        ));
    }
    let sql = format!(
        "UPDATE architecture_artifact SET
            title = coalesce($4, title),
            change_initiative = coalesce($5, change_initiative),
            purpose = coalesce($6, purpose),
            stakeholders = coalesce($7, stakeholders),
            scope = coalesce($8, scope),
            document_id = coalesce($9, document_id),
            contract_id = coalesce($10, contract_id),
            meta = coalesce($11, meta),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {ARTIFACT_COLUMNS}"
    );
    let record: Option<ArtifactRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(patch.title.as_deref().map(str::trim))
        .bind(&patch.change_initiative)
        .bind(&patch.purpose)
        .bind(&patch.stakeholders)
        .bind(&patch.scope)
        .bind(patch.document_id)
        .bind(patch.contract_id)
        .bind(&patch.meta)
        .fetch_optional(&mut *conn)
        .await?;
    let record = record.ok_or(ArchitectureStoreError::Stale("architecture artefact"))?;
    if let Some(entries) = &patch.entries {
        insert_entries_tx(&mut *conn, tenant, id, entries).await?;
    }
    Ok(record)
}

/// Approve an artefact.
pub async fn approve_artifact_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    reviewer: Uuid,
) -> Result<ArtifactRecord, ArchitectureStoreError> {
    let sql = format!(
        "UPDATE architecture_artifact
            SET status = 'approved', reviewer_id = $4, reviewed_at = now(),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
            AND status IN ('draft', 'in_review')
          RETURNING {ARTIFACT_COLUMNS}"
    );
    let record: Option<ArtifactRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(reviewer)
        .fetch_optional(&mut *conn)
        .await?;
    record.ok_or_else(|| ArchitectureStoreError::Illegal(ArchitectureRule::ApprovedImmutable))
}

/// Supersede an artefact with a new draft version; the approved body is unchanged.
pub async fn supersede_artifact_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    author: Option<Uuid>,
) -> Result<ArtifactRecord, ArchitectureStoreError> {
    let current = get_artifact_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(ArchitectureStoreError::NotFound("architecture artefact"))?;
    if current.version != expected {
        return Err(ArchitectureStoreError::Stale("architecture artefact"));
    }
    if current.status == "superseded" || current.status == "archived" {
        return Err(ArchitectureStoreError::Conflict {
            what: "architecture artefact",
            why: "it is already superseded or archived",
        });
    }
    let sql = format!(
        "INSERT INTO architecture_artifact
            (tenant_id, project_id, artifact_type_id, phase_code, plateau, artifact_kind,
             title, change_initiative, purpose, stakeholders, scope, document_id, contract_id,
             meta, author_id, authored_at, artifact_version, supersedes_id)
         SELECT tenant_id, project_id, artifact_type_id, phase_code, plateau, artifact_kind,
                title, change_initiative, purpose, stakeholders, scope, document_id, contract_id,
                meta, $4, now(), artifact_version + 1, id
           FROM architecture_artifact WHERE tenant_id = $1 AND id = $2
         RETURNING {ARTIFACT_COLUMNS}"
    );
    let record: ArtifactRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(author)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query(
        "UPDATE architecture_artifact
            SET status = 'superseded', superseded_by_id = $3, version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(record.id)
    .execute(&mut *conn)
    .await?;
    Ok(record)
}

/// Link two artefacts.
pub async fn link_artifacts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    from: Uuid,
    to: Uuid,
    kind: &str,
) -> Result<ArtifactLinkRecord, ArchitectureStoreError> {
    vocab::require_link_kind(kind).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "link kind {kind}"
        )))
    })?;
    if from == to {
        return Err(ArchitectureStoreError::Conflict {
            what: "architecture artefact link",
            why: "an artefact cannot link to itself",
        });
    }
    let sql = format!(
        "INSERT INTO architecture_artifact_link
            (tenant_id, from_artifact_id, to_artifact_id, kind)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, from_artifact_id, to_artifact_id, kind) DO NOTHING
         RETURNING {LINK_COLUMNS}"
    );
    let inserted: Option<ArtifactLinkRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(from)
        .bind(to)
        .bind(kind)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(record) = inserted {
        return Ok(record);
    }
    let sql = format!(
        "SELECT {LINK_COLUMNS} FROM architecture_artifact_link
          WHERE tenant_id = $1 AND from_artifact_id = $2 AND to_artifact_id = $3 AND kind = $4"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(from)
        .bind(to)
        .bind(kind)
        .fetch_one(&mut *conn)
        .await
        .map_err(ArchitectureStoreError::Database)
}

/// Search the repository for reuse.
pub async fn search_repository_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    query: Option<&str>,
    plateau: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<ArtifactRecord>, ArchitectureStoreError> {
    // The repository answers what an artefact reuses, not only what matches the text: an
    // artefact reached by a `reuse` edge from a match is returned too, across projects, phases
    // and plateaus. A search confined to the query's own plateau would hide exactly the
    // enterprise artefact a solution artefact was built to reuse (D1's reuse scenario).
    let sql = format!(
        "WITH matched AS (
            SELECT id FROM architecture_artifact
             WHERE tenant_id = $1
               AND status IN ('approved', 'in_review')
               AND ($2::text IS NULL OR title ILIKE '%' || $2 || '%'
                    OR purpose ILIKE '%' || $2 || '%'
                    OR meta::text ILIKE '%' || $2 || '%')
               AND ($3::text IS NULL OR plateau = $3)
               AND ($4::text IS NULL OR artifact_kind = $4)
         ), reused AS (
            SELECT l.to_artifact_id AS id FROM architecture_artifact_link l
             WHERE l.tenant_id = $1 AND l.kind = 'reuse'
               AND l.from_artifact_id IN (SELECT id FROM matched)
         )
         SELECT {ARTIFACT_COLUMNS} FROM architecture_artifact a
          WHERE a.tenant_id = $1
            AND a.status IN ('approved', 'in_review')
            AND (a.id IN (SELECT id FROM matched) OR a.id IN (SELECT id FROM reused))
          ORDER BY a.artifact_version DESC, a.created_at DESC LIMIT 100"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(query)
        .bind(plateau)
        .bind(kind)
        .fetch_all(&mut *conn)
        .await?)
}

/// A new tailored profile.
#[derive(Clone, Debug)]
pub struct NewProfile {
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
}

/// Read the tenant's profile: a named one, else the adopted one, else the newest.
pub async fn get_profile_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: Option<&str>,
) -> Result<Option<ProfileRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {PROFILE_COLUMNS} FROM adm_profile
          WHERE tenant_id = $1
            AND (($2::text IS NOT NULL AND key = $2) OR $2::text IS NULL)
          ORDER BY (status = 'adopted') DESC, updated_at DESC LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Create or replace a draft profile by key.
pub async fn upsert_profile_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewProfile,
) -> Result<ProfileRecord, ArchitectureStoreError> {
    if new.key.trim().is_empty() || new.name.trim().is_empty() {
        return Err(ArchitectureStoreError::Conflict {
            what: "adm profile",
            why: "a key and a name are required",
        });
    }
    let sql = format!(
        "INSERT INTO adm_profile
            (tenant_id, key, name, phase_set, artifact_requirements, contract_kinds)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, key) DO UPDATE SET
            name = EXCLUDED.name,
            phase_set = EXCLUDED.phase_set,
            artifact_requirements = EXCLUDED.artifact_requirements,
            contract_kinds = EXCLUDED.contract_kinds,
            version = adm_profile.version + 1,
            updated_at = now()
          WHERE adm_profile.status <> 'retired'
         RETURNING {PROFILE_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.key.trim())
        .bind(new.name.trim())
        .bind(&new.phase_set)
        .bind(&new.artifact_requirements)
        .bind(&new.contract_kinds)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ArchitectureStoreError::Conflict {
            what: "adm profile",
            why: "it is retired",
        })
}

/// Adopt a profile; the board decision is required.
pub async fn adopt_profile_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    board_decision_id: Uuid,
) -> Result<ProfileRecord, ArchitectureStoreError> {
    let sql = format!(
        "UPDATE adm_profile
            SET status = 'adopted', adopted_at = now(), board_decision_id = $4,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3 AND status = 'draft'
         RETURNING {PROFILE_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(board_decision_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ArchitectureStoreError::Stale("adm profile"))
}

/// A new governance body.
#[derive(Clone, Debug)]
pub struct NewBody {
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
    /// The forum type this body's sittings use; the body's key when omitted.
    pub meeting_type_key: Option<String>,
}

/// Create a governance body.
pub async fn create_body_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewBody,
) -> Result<GovernanceBodyRecord, ArchitectureStoreError> {
    let sql = format!(
        "INSERT INTO governance_body
            (tenant_id, kind, key, name, charter, commissioning_authority, decision_limits, members)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {BODY_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&new.kind)
        .bind(new.key.trim())
        .bind(new.name.trim())
        .bind(&new.charter)
        .bind(&new.commissioning_authority)
        .bind(&new.decision_limits)
        .bind(&new.members)
        .fetch_one(&mut *conn)
        .await
        .map_err(ArchitectureStoreError::Database)
}

/// Open a governance body: create it, or return the row that already holds its key.
///
/// The Board is a row, not a convention: `architecture.board_decide` needs a body id, and the
/// profile's adoption cites a board decision. A second open with the same key is not an error
/// and does not overwrite the charter — it returns the body as commissioned.
pub async fn open_body_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewBody,
) -> Result<GovernanceBodyRecord, ArchitectureStoreError> {
    if !matches!(
        new.kind.as_str(),
        "architecture_board" | "design_board" | "other"
    ) {
        return Err(ArchitectureStoreError::Conflict {
            what: "governance body",
            why: "the kind must be architecture_board, design_board or other",
        });
    }
    if new.key.trim().is_empty() || new.name.trim().is_empty() {
        return Err(ArchitectureStoreError::Conflict {
            what: "governance body",
            why: "the key and the name are required",
        });
    }
    let sql = format!(
        "INSERT INTO governance_body
            (tenant_id, kind, key, name, charter, commissioning_authority, decision_limits,
             members, meeting_type_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (tenant_id, key) DO NOTHING
         RETURNING {BODY_COLUMNS}"
    );
    let inserted: Option<GovernanceBodyRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&new.kind)
        .bind(new.key.trim())
        .bind(new.name.trim())
        .bind(&new.charter)
        .bind(&new.commissioning_authority)
        .bind(&new.decision_limits)
        .bind(&new.members)
        .bind(&new.meeting_type_key)
        .fetch_optional(&mut *conn)
        .await?;
    match inserted {
        Some(row) => Ok(row),
        None => get_body_by_key_tx(&mut *conn, tenant, new.key.trim())
            .await?
            .ok_or(ArchitectureStoreError::Conflict {
                what: "governance body",
                why: "the key is held by a row this tenant cannot read",
            }),
    }
}

/// Read a governance body by its stable key.
pub async fn get_body_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<GovernanceBodyRecord>, ArchitectureStoreError> {
    let sql =
        format!("SELECT {BODY_COLUMNS} FROM governance_body WHERE tenant_id = $1 AND key = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List the governance bodies, optionally one kind, by key.
pub async fn list_bodies_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: Option<&str>,
) -> Result<Vec<GovernanceBodyRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {BODY_COLUMNS} FROM governance_body
          WHERE tenant_id = $1 AND ($2::text IS NULL OR kind = $2)
          ORDER BY kind, key"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .fetch_all(&mut *conn)
        .await?)
}

/// A board sitting: the approval the seats decide and the meeting they decide it in.
#[derive(Clone, Copy, Debug)]
pub struct BoardSitting {
    /// The meeting card's row.
    pub meeting_id: Uuid,
    /// The pending approval the agenda item decides.
    pub approval_id: Uuid,
}

/// The subject tables a board decision can name.
fn board_subject_table(kind: &str) -> Option<&'static str> {
    match kind {
        "contract" => Some("architecture_contract"),
        "artifact" => Some("architecture_artifact"),
        "adm_profile" => Some("adm_profile"),
        "waiver" => Some("architecture_waiver"),
        _ => None,
    }
}

/// Open (or resume) the board's sitting over one subject: a pending approval, a live meeting
/// with the body's standing type, and the approval as its deciding agenda item. Idempotent:
/// an existing pending approval on a scheduled or live sitting is returned unchanged.
// The caller's transaction, tenant and actor plus the five subject/sitting fields; grouping
// them would only move the tuple.
#[allow(clippy::too_many_arguments)]
pub async fn open_board_sitting_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    actor: crate::meetings::MeetingActor<'_>,
    body_id: Uuid,
    subject_kind: &str,
    subject_id: Uuid,
    list_id: Uuid,
    evidence: &Value,
) -> Result<BoardSitting, ArchitectureStoreError> {
    let body = get_body_tx(&mut *conn, tenant, body_id)
        .await?
        .ok_or(ArchitectureStoreError::NotFound("governance body"))?;
    if body.status != "active" {
        return Err(ArchitectureStoreError::Conflict {
            what: "governance body",
            why: "the body is not active; approve it before it can sit",
        });
    }
    let table = board_subject_table(subject_kind).ok_or(ArchitectureStoreError::Conflict {
        what: "board decision",
        why: "the subject kind must be contract, artifact, adm_profile or waiver",
    })?;
    let exists: bool = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 FROM {table} WHERE tenant_id = $1 AND id = $2)"
    )))
    .bind(tenant.as_uuid())
    .bind(subject_id)
    .fetch_one(&mut *conn)
    .await?;
    if !exists {
        return Err(ArchitectureStoreError::NotFound("board decision subject"));
    }
    let meeting_type_key = body
        .meeting_type_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .unwrap_or_else(|| body.key.clone());

    let existing: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT a.id, i.meeting_id
           FROM approval a
           JOIN meeting_agenda_item i
             ON i.tenant_id = a.tenant_id AND i.resolved_kind = 'approval'
            AND i.resolved_id = a.id
           JOIN meeting m ON m.tenant_id = i.tenant_id AND m.id = i.meeting_id
          WHERE a.tenant_id = $1 AND a.status = 'pending'
            AND a.gate = 'architecture_board'
            AND a.meta ->> 'body_id' = $2
            AND a.meta ->> 'subject_kind' = $3
            AND a.meta ->> 'subject_id' = $4
            AND m.status IN ('scheduled', 'live')
          ORDER BY m.created_at DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(body.id.to_string())
    .bind(subject_kind)
    .bind(subject_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((approval_id, meeting_id)) = existing {
        return Ok(BoardSitting {
            meeting_id,
            approval_id,
        });
    }

    let approval_id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval
            (tenant_id, gate, kind, status, required_decisions, requested_by, meta)
         VALUES ($1, 'architecture_board', 'architecture_board', 'pending', 1, $2, $3)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(actor.id)
    .bind(json!({
        "body_id": body.id,
        "evidence": evidence,
        "subject_kind": subject_kind,
        "subject_id": subject_id,
    }))
    .fetch_one(&mut *conn)
    .await?;

    let existing_meeting: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM meeting
          WHERE tenant_id = $1 AND convener_governance_body_id = $2
            AND status IN ('scheduled', 'live')
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(body.id)
    .fetch_optional(&mut *conn)
    .await?;
    let meeting_id = match existing_meeting {
        Some(id) => id,
        None => {
            let title = format!("{} sitting", body.name);
            let new = crate::meetings::NewMeeting {
                meeting_type_key: &meeting_type_key,
                policy: None,
                title: &title,
                description: "A board sitting over its subject decisions.",
                list_id,
                board_id: None,
                convener_kind: "governance_body",
                convener_id: body.id,
                cadence_slot: None,
            };
            let meeting = crate::meetings::schedule_tx(&mut *conn, tenant, actor, &new).await?;
            crate::meetings::open_tx(&mut *conn, tenant, meeting.id, None).await?;
            meeting.id
        }
    };
    crate::meetings::agenda_add_tx(
        &mut *conn,
        tenant,
        meeting_id,
        &crate::meetings::NewAgendaItem {
            resolved_kind: "approval",
            resolved_id: approval_id,
            participating_departments: &[],
            evidence_refs: evidence,
        },
    )
    .await?;
    Ok(BoardSitting {
        meeting_id,
        approval_id,
    })
}

/// Decide the sitting's item through T19's arithmetic; the meeting must be the body's.
pub async fn decide_board_sitting_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    body_id: Uuid,
    meeting_id: Uuid,
    agenda_item_id: Uuid,
) -> Result<crate::meetings::CloseItemResult, ArchitectureStoreError> {
    let convener: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT convener_governance_body_id FROM meeting WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    match convener {
        None => return Err(ArchitectureStoreError::NotFound("meeting")),
        Some(Some(id)) if id == body_id => {}
        Some(_) => {
            return Err(ArchitectureStoreError::Conflict {
                what: "meeting",
                why: "the sitting was not convened by this body",
            });
        }
    }
    Ok(
        crate::meetings::close_item_tx(&mut *conn, tenant, meeting_id, agenda_item_id, None)
            .await?,
    )
}

/// Read a governance body.
pub async fn get_body_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<GovernanceBodyRecord>, ArchitectureStoreError> {
    let sql =
        format!("SELECT {BODY_COLUMNS} FROM governance_body WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Who took a board decision and under what authority.
#[derive(Clone, Copy, Debug)]
pub struct BoardDecider<'a> {
    /// The deciding actor.
    pub decided_by: Uuid,
    /// `human | agent | rule` — derived from the authenticated principal.
    pub decided_by_kind: &'a str,
    /// The published standing delegation the act cites, when it is a delegated rule decision.
    pub delegation_ref: Option<Uuid>,
}

/// What a board decision is about.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardSubject<'a> {
    /// `board | contract | artifact | adm_profile | waiver`.
    pub kind: Option<&'a str>,
    /// The subject's id; absent for a decision about the board itself.
    pub id: Option<Uuid>,
}

/// Record a board decision; the approval row is the decision's durable record.
pub async fn decide_board_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    body_id: Uuid,
    decision: &str,
    evidence: Value,
    subject: BoardSubject<'_>,
    decider: BoardDecider<'_>,
) -> Result<Uuid, ArchitectureStoreError> {
    if decision != "approved" && decision != "rejected" {
        return Err(ArchitectureStoreError::Conflict {
            what: "board decision",
            why: "the decision must be approved or rejected",
        });
    }
    let BoardSubject {
        kind: subject_kind,
        id: subject_id,
    } = subject;
    let subject_kind = match subject_kind {
        Some(kind)
            if matches!(
                kind,
                "board" | "contract" | "artifact" | "adm_profile" | "waiver"
            ) =>
        {
            Some(kind)
        }
        Some(_other) => {
            return Err(ArchitectureStoreError::Conflict {
                what: "board decision",
                why: "the subject kind must be board, contract, artifact, adm_profile or waiver",
            });
        }
        None => None,
    };
    match (subject_kind, subject_id) {
        (Some("board"), Some(_)) => {
            return Err(ArchitectureStoreError::Conflict {
                what: "board decision",
                why: "a decision about the board itself takes no subject id",
            });
        }
        (Some("board"), None) | (None, None) => {}
        (Some(_), None) | (None, Some(_)) => {
            return Err(ArchitectureStoreError::Conflict {
                what: "board decision",
                why: "a subject decision needs both subject_kind and subject_id",
            });
        }
        _ => {}
    }
    get_body_tx(&mut *conn, tenant, body_id)
        .await?
        .ok_or(ArchitectureStoreError::NotFound("governance body"))?;
    // A subject that does not exist would make the decision unreferenceable; refuse it here.
    if let (Some(kind), Some(subject)) = (subject_kind, subject_id) {
        let table = match kind {
            "contract" => "architecture_contract",
            "artifact" => "architecture_artifact",
            "adm_profile" => "adm_profile",
            "waiver" => "architecture_waiver",
            _ => unreachable!("subject_kind was validated above"),
        };
        let exists: bool = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT EXISTS (SELECT 1 FROM {table} WHERE tenant_id = $1 AND id = $2)"
        )))
        .bind(tenant.as_uuid())
        .bind(subject)
        .fetch_one(&mut *conn)
        .await?;
        if !exists {
            return Err(ArchitectureStoreError::NotFound("board decision subject"));
        }
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval
            (tenant_id, gate, kind, status, required_decisions, requested_by, meta)
         VALUES ($1, 'architecture_board', 'architecture_board', $2, 1, $3, $4)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(decision)
    .bind(decider.decided_by)
    .bind(json!({
        "body_id": body_id,
        "evidence": evidence,
        "subject_kind": subject_kind,
        "subject_id": subject_id,
    }))
    .fetch_one(&mut *conn)
    .await?;
    // A delegated act is a recorded decision, not only an approval row: the approver, the
    // derived kind (`rule` for the delegated service principal) and the standing delegation
    // the act cites are written in the same transaction as the approval and decision. This is
    // the type `approvals.decide` writes; the board macro had no decision row before, so a
    // delegated board act left no typed record of who decided it or under what authority.
    if let Some(delegation_ref) = decider.delegation_ref {
        sqlx::query(
            "INSERT INTO approval_decision
                (tenant_id, approval_id, approver_id, decision, decided_by_kind, via)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(decider.decided_by)
        .bind(if decision == "approved" {
            "approve"
        } else {
            "reject"
        })
        .bind(decider.decided_by_kind)
        .bind(json!({
            "channel": "internal",
            "delegation_ref": delegation_ref.to_string(),
        }))
        .execute(&mut *conn)
        .await?;
    }
    // A board convened by its own decision is active from that decision. Nothing writes the
    // status directly: this is the one writer, and it writes what the decision says.
    if subject_kind == Some("board") && decision == "approved" {
        sqlx::query(
            "UPDATE governance_body
                SET status = 'active', version = version + 1, updated_at = now()
              WHERE tenant_id = $1 AND id = $2 AND status = 'draft'",
        )
        .bind(tenant.as_uuid())
        .bind(body_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(id)
}

/// A new contract.
#[derive(Clone, Debug)]
pub struct NewContract {
    /// The governing project.
    pub project_id: Uuid,
    /// The kind.
    pub kind: String,
    /// The phase it closes.
    pub phase_code: String,
    /// Title.
    pub title: String,
    /// The body.
    pub body: Value,
    /// The actor that requested it; the approver must be a different actor.
    pub requested_by: Uuid,
    /// The contract it replaces, if any.
    pub supersedes_id: Option<Uuid>,
}

/// Request a contract (draft); a supersede retires the old version in the same write.
pub async fn request_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewContract,
) -> Result<ContractRecord, ArchitectureStoreError> {
    vocab::require_contract_kind(&new.kind).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "contract kind {}",
            new.kind
        )))
    })?;
    vocab::require_phase(&new.phase_code).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "phase {}",
            new.phase_code
        )))
    })?;
    if let Some(old) = new.supersedes_id {
        sqlx::query(
            "UPDATE architecture_contract
                SET status = 'superseded', version = version + 1, updated_at = now()
              WHERE tenant_id = $1 AND id = $2 AND status IN ('draft', 'live')",
        )
        .bind(tenant.as_uuid())
        .bind(old)
        .execute(&mut *conn)
        .await?;
    }
    let sql = format!(
        "INSERT INTO architecture_contract
            (tenant_id, project_id, kind, phase_code, title, body, supersedes_id, requested_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {CONTRACT_COLUMNS}"
    );
    let record: ContractRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.project_id)
        .bind(&new.kind)
        .bind(&new.phase_code)
        .bind(new.title.trim())
        .bind(&new.body)
        .bind(new.supersedes_id)
        .bind(new.requested_by)
        .fetch_one(&mut *conn)
        .await
        .map_err(ArchitectureStoreError::Database)?;
    // The chain is two-way: the retired row names its successor, so "what replaced this
    // contract" is a read, not a search. The successor already names its predecessor above.
    if let Some(old) = new.supersedes_id {
        sqlx::query(
            "UPDATE architecture_contract
                SET superseded_by_id = $3, updated_at = now()
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(old)
        .bind(record.id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(record)
}

/// Approve a draft contract into `live`; an existing live sibling must be superseded first.
pub async fn approve_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    approved_by: Uuid,
    approval_id: Option<Uuid>,
) -> Result<ContractRecord, ArchitectureStoreError> {
    let current = get_contract_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(ArchitectureStoreError::NotFound("architecture contract"))?;
    if current.version != expected {
        return Err(ArchitectureStoreError::Stale("architecture contract"));
    }
    if current.status != "draft" {
        return Err(ArchitectureStoreError::Conflict {
            what: "architecture contract",
            why: "only a draft contract can be approved",
        });
    }
    let live: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM architecture_contract
             WHERE tenant_id = $1 AND project_id = $2 AND kind = $3 AND phase_code = $4
               AND status = 'live' AND id <> $5
        )",
    )
    .bind(tenant.as_uuid())
    .bind(current.project_id)
    .bind(&current.kind)
    .bind(&current.phase_code)
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if live {
        return Err(ArchitectureStoreError::Conflict {
            what: "architecture contract",
            why: "a live contract of this kind exists; supersede it first",
        });
    }
    // A cited board decision must actually be about this contract: an approval row with the
    // board's gate, an `approved` outcome and this contract as its subject. Without that, the
    // contract's `approval_id` would name a decision about something else.
    if let Some(approval_id) = approval_id {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM approval
                 WHERE tenant_id = $1 AND id = $2 AND gate = 'architecture_board'
                   AND status = 'approved'
                   AND meta->>'subject_kind' = 'contract'
                   AND meta->>'subject_id' = $3
            )",
        )
        .bind(tenant.as_uuid())
        .bind(approval_id)
        .bind(id.to_string())
        .fetch_one(&mut *conn)
        .await?;
        if !valid {
            return Err(ArchitectureStoreError::Conflict {
                what: "architecture contract",
                why: "the cited board decision does not approve this contract",
            });
        }
    }
    let sql = format!(
        "UPDATE architecture_contract
            SET status = 'live', approved_by = $4, approved_at = now(),
                approval_id = COALESCE($5, approval_id),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3 AND status = 'draft'
         RETURNING {CONTRACT_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(approved_by)
        .bind(approval_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ArchitectureStoreError::Stale("architecture contract"))
}

/// Supersede a live or draft contract.
pub async fn supersede_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<ContractRecord, ArchitectureStoreError> {
    let sql = format!(
        "UPDATE architecture_contract
            SET status = 'superseded', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3 AND status IN ('draft', 'live')
         RETURNING {CONTRACT_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ArchitectureStoreError::Stale("architecture contract"))
}

/// Read a contract.
pub async fn get_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ContractRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {CONTRACT_COLUMNS} FROM architecture_contract
          WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a project's contracts.
pub async fn list_contracts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Option<Uuid>,
) -> Result<Vec<ContractRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {CONTRACT_COLUMNS} FROM architecture_contract
          WHERE tenant_id = $1 AND ($2::uuid IS NULL OR project_id = $2)
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Materialise the phase WBS nodes for an architecture project.
pub async fn materialise_phases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    phases: &[String],
) -> Result<Vec<Uuid>, ArchitectureStoreError> {
    let mut ids = Vec::with_capacity(phases.len());
    for (position, phase) in phases.iter().enumerate() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM wbs_node
                 WHERE tenant_id = $1 AND project_id = $2 AND kind = 'phase'
                   AND meta->>'phase_code' = $3
            )",
        )
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(phase)
        .fetch_one(&mut *conn)
        .await?;
        if exists {
            continue;
        }
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO wbs_node
                (tenant_id, project_id, position, kind, title, status, meta)
             VALUES ($1, $2, $3, 'phase', $4, 'open', $5)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(position as i64)
        .bind(format!("Phase {phase}"))
        .bind(json!({ "phase_code": phase }))
        .fetch_one(&mut *conn)
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

/// Materialise the ADM's nine phase nodes (Preliminary, then A–H) on an architecture project.
///
/// `projects.create` and `pm.charter` call this for `project_type_code = "architecture"`, so the
/// phase gate has nodes to advance; a project of any other type gets none.
pub async fn materialise_adm_phases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<Uuid>, ArchitectureStoreError> {
    let phases: Vec<String> = vocab::PHASES
        .iter()
        .map(|phase| (*phase).to_owned())
        .collect();
    materialise_phases_tx(conn, tenant, project_id, &phases).await
}

/// Advance an architecture project's current phase when the gate is satisfied.
pub async fn advance_phase_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    requested: &str,
    profile_key: Option<&str>,
) -> Result<PhaseAdvance, ArchitectureStoreError> {
    vocab::require_phase(requested).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "phase {requested}"
        )))
    })?;
    let profile = get_profile_tx(&mut *conn, tenant, profile_key)
        .await?
        .ok_or(ArchitectureStoreError::Illegal(ArchitectureRule::NoProfile))?;
    let nodes: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, meta->>'phase_code', status FROM wbs_node
          WHERE tenant_id = $1 AND project_id = $2 AND kind = 'phase'
          ORDER BY position, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_all(&mut *conn)
    .await?;
    if nodes.is_empty() {
        return Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::PhaseOutOfOrder {
                current: "none".to_owned(),
                requested: requested.to_owned(),
            },
        ));
    }
    let (node_id, current, _status) = nodes
        .iter()
        .find(|(_, phase, _)| phase == requested)
        .cloned()
        .ok_or_else(|| {
            ArchitectureStoreError::Illegal(ArchitectureRule::PhaseOutOfOrder {
                current: nodes
                    .iter()
                    .find(|(_, _, status)| status == "open")
                    .map(|(_, phase, _)| phase.clone())
                    .unwrap_or_else(|| "h".to_owned()),
                requested: requested.to_owned(),
            })
        })?;
    let first_open = nodes
        .iter()
        .find(|(_, _, status)| status == "open")
        .map(|(_, phase, _)| phase.clone())
        .unwrap_or(current.clone());
    if first_open != requested {
        return Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::PhaseOutOfOrder {
                current: first_open,
                requested: requested.to_owned(),
            },
        ));
    }
    // Mandatory artefacts: the profile names one set per phase and plateau, and the phase is
    // satisfied when ONE plateau's set is fully approved. The union was wrong: a phase carrying
    // PSA and HLD sets demanded both, refusing a project for artefacts its phase does not
    // require — the profile, not the gate, decides which plateaus a phase carries.
    let sets: Vec<Vec<String>> = match profile.artifact_requirements.get(requested) {
        Some(Value::Array(items)) => vec![
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ],
        Some(Value::Object(plateaus)) => plateaus
            .values()
            .filter_map(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut artifact_ids: Vec<Uuid> = Vec::new();
    let mut satisfied = sets.is_empty();
    let mut first_missing: Option<String> = None;
    for set in &sets {
        let mut ids = Vec::with_capacity(set.len());
        let mut missing = None;
        for key in set {
            let id: Option<Uuid> = sqlx::query_scalar(
                "SELECT a.id FROM architecture_artifact a
                   JOIN artifact_type t ON t.id = a.artifact_type_id AND t.tenant_id = a.tenant_id
                  WHERE a.tenant_id = $1 AND a.project_id = $2 AND a.phase_code = $3
                    AND a.status = 'approved' AND t.key = $4
                  LIMIT 1",
            )
            .bind(tenant.as_uuid())
            .bind(project_id)
            .bind(requested)
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?;
            match id {
                Some(id) => ids.push(id),
                None => {
                    missing = Some(key.clone());
                    break;
                }
            }
        }
        match missing {
            None => {
                artifact_ids = ids;
                satisfied = true;
                break;
            }
            Some(key) => {
                first_missing.get_or_insert(key);
            }
        }
    }
    if !satisfied {
        return Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::MissingMandatory(first_missing.unwrap_or_default()),
        ));
    }
    // The phase's contract, when the profile names one.
    let required_kind = profile
        .contract_kinds
        .get(requested)
        .and_then(|value| match value {
            Value::String(kind) if kind != "none" => Some(kind.clone()),
            _ => None,
        });
    let mut contract_id = None;
    if let Some(kind) = required_kind {
        let live: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM architecture_contract
              WHERE tenant_id = $1 AND project_id = $2 AND kind = $3 AND phase_code = $4
                AND status = 'live' LIMIT 1",
        )
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(&kind)
        .bind(requested)
        .fetch_optional(&mut *conn)
        .await?;
        match live {
            Some(id) => contract_id = Some(id),
            None => {
                return Err(ArchitectureStoreError::Illegal(
                    ArchitectureRule::ContractNotLive(kind),
                ));
            }
        }
    }
    // Phase H: exactly one typed outcome must be recorded.
    if requested == "h" {
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM architecture_compliance_review
                 WHERE tenant_id = $1 AND project_id = $2 AND outcome IS NOT NULL
            )",
        )
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_one(&mut *conn)
        .await?;
        if !recorded {
            return Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::OutcomeRequired,
            ));
        }
    }
    let evidence = json!({
        "artifacts": artifact_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "contract": contract_id.map(|id| id.to_string()),
    });
    // $3, not $4: three binds are supplied, and Postgres refuses a gap in the parameter list.
    sqlx::query(
        "UPDATE wbs_node
            SET status = 'accepted', evidence = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'open'",
    )
    .bind(tenant.as_uuid())
    .bind(node_id)
    .bind(evidence)
    .execute(&mut *conn)
    .await?;
    Ok(PhaseAdvance {
        closed_phase: requested.to_owned(),
        next_phase: vocab::next_phase(requested).map(str::to_owned),
        artifact_ids,
        contract_id,
    })
}

/// A new compliance review.
#[derive(Clone, Debug)]
pub struct NewReview {
    /// The delivery project.
    pub project_id: Uuid,
    /// The governing contract.
    pub contract_id: Uuid,
    /// The one level.
    pub level: String,
    /// Findings.
    pub findings: Value,
    /// Evidence refs.
    pub evidence: Value,
    /// Phase H's one typed outcome.
    pub outcome: Option<Value>,
    /// Who reviewed.
    pub reviewed_by: Uuid,
}

/// Record the one compliance level a delivery contract gets.
pub async fn record_review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewReview,
) -> Result<ReviewRecord, ArchitectureStoreError> {
    vocab::require_compliance_level(&new.level).map_err(|_| {
        ArchitectureStoreError::Illegal(ArchitectureRule::MissingMandatory(format!(
            "compliance level {}",
            new.level
        )))
    })?;
    let contract = get_contract_tx(&mut *conn, tenant, new.contract_id)
        .await?
        .ok_or(ArchitectureStoreError::NotFound("architecture contract"))?;
    if contract.project_id != new.project_id {
        return Err(ArchitectureStoreError::Conflict {
            what: "compliance review",
            why: "the contract does not govern that project",
        });
    }
    if let Some(outcome) = &new.outcome {
        let kind = outcome.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind != "change" && kind != "kata" {
            return Err(ArchitectureStoreError::Conflict {
                what: "phase-H outcome",
                why: "the outcome kind must be change or kata",
            });
        }
        if kind == "change" {
            let change_id = outcome
                .get("change_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or(ArchitectureStoreError::Conflict {
                    what: "phase-H outcome",
                    why: "a change outcome names its change_id",
                })?;
            let present: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM project_register_item
                     WHERE tenant_id = $1 AND id = $2 AND project_id = $3 AND kind = 'change'
                )",
            )
            .bind(tenant.as_uuid())
            .bind(change_id)
            .bind(new.project_id)
            .fetch_one(&mut *conn)
            .await?;
            if !present {
                return Err(ArchitectureStoreError::Illegal(
                    ArchitectureRule::OutcomeNotApproved(change_id),
                ));
            }
        }
    }
    let sql = format!(
        "INSERT INTO architecture_compliance_review
            (tenant_id, project_id, contract_id, level, findings, evidence, outcome, reviewed_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (tenant_id, project_id, contract_id) DO NOTHING
         RETURNING {REVIEW_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.project_id)
        .bind(new.contract_id)
        .bind(&new.level)
        .bind(&new.findings)
        .bind(&new.evidence)
        .bind(&new.outcome)
        .bind(new.reviewed_by)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ArchitectureStoreError::Conflict {
            what: "compliance review",
            why: "this delivery contract already records its one level",
        })
}

/// A new waiver.
#[derive(Clone, Debug)]
pub struct NewWaiver {
    /// The review.
    pub review_id: Uuid,
    /// The accepting asset owner.
    pub owner_user_id: Option<Uuid>,
    /// Why.
    pub reason: String,
    /// Additional cost.
    pub cost_impact: String,
    /// Additional risk.
    pub risk_impact: String,
    /// The reassessment date.
    pub reassessment_at: Option<DateTime<Utc>>,
}

/// Grant a time-boxed waiver; an owner and a date are mandatory.
pub async fn waive_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewWaiver,
) -> Result<WaiverRecord, ArchitectureStoreError> {
    let (Some(owner), Some(at)) = (new.owner_user_id, new.reassessment_at) else {
        return Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::WaiverNeedsOwnerAndDate,
        ));
    };
    if new.reason.trim().is_empty() {
        return Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::WaiverNeedsOwnerAndDate,
        ));
    }
    let review: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM architecture_compliance_review WHERE tenant_id = $1 AND id = $2
        )",
    )
    .bind(tenant.as_uuid())
    .bind(new.review_id)
    .fetch_one(&mut *conn)
    .await?;
    if !review {
        return Err(ArchitectureStoreError::NotFound("compliance review"));
    }
    let sql = format!(
        "INSERT INTO architecture_waiver
            (tenant_id, review_id, owner_user_id, reason, cost_impact, risk_impact, reassessment_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {WAIVER_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.review_id)
        .bind(owner)
        .bind(new.reason.trim())
        .bind(&new.cost_impact)
        .bind(&new.risk_impact)
        .bind(at)
        .fetch_one(&mut *conn)
        .await
        .map_err(ArchitectureStoreError::Database)
}

/// The project's live delivery contract, if it has one.
///
/// The close gate reads this row, not the project's prose: a delivery project closes only
/// against a contract the board approved, and the contract's `approved_by` is the independent
/// authority the close cites.
pub async fn live_delivery_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Option<ContractRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {CONTRACT_COLUMNS} FROM architecture_contract
          WHERE tenant_id = $1 AND project_id = $2 AND kind = 'delivery_contract'
            AND status = 'live'
          ORDER BY approved_at DESC NULLS LAST, id DESC LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// The one compliance review a delivery contract carries (the Phase-G judgement).
pub async fn review_for_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    contract_id: Uuid,
) -> Result<Option<ReviewRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {REVIEW_COLUMNS} FROM architecture_compliance_review
          WHERE tenant_id = $1 AND contract_id = $2 LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(contract_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Read one compliance review by id; the waiver path needs its contract, reviewer and evidence.
pub async fn get_review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ReviewRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {REVIEW_COLUMNS} FROM architecture_compliance_review
          WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// An active, unexpired waiver on a review. A waiver is the only way a non-conformant
/// compliance level may still close; its reassessment date is the forcing function.
pub async fn active_waiver_for_review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    review_id: Uuid,
) -> Result<Option<WaiverRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {WAIVER_COLUMNS} FROM architecture_waiver
          WHERE tenant_id = $1 AND review_id = $2 AND status = 'active'
            AND reassessment_at > now()
          ORDER BY reassessment_at DESC LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(review_id)
        .fetch_optional(&mut *conn)
        .await?)
}

fn require_scores(
    what: &'static str,
    expected: &[&str],
    scores: &[(String, i32)],
) -> Result<(), ArchitectureStoreError> {
    for (key, score) in scores {
        if !(0..=5).contains(score) {
            return Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::ScoreOutOfRange(*score),
            ));
        }
        if !expected.contains(&key.as_str()) {
            return Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::AssessmentIncomplete {
                    what,
                    value: key.clone(),
                },
            ));
        }
    }
    for key in expected {
        if !scores.iter().any(|(k, _)| k == key) {
            return Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::AssessmentIncomplete {
                    what,
                    value: (*key).to_owned(),
                },
            ));
        }
    }
    Ok(())
}

/// Record all nine maturity dimensions for one stage.
pub async fn assess_maturity_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    assessment_id: Uuid,
    stage: &str,
    scores: &[(String, i32)],
    evidence: Value,
) -> Result<Vec<MaturityRecord>, ArchitectureStoreError> {
    require_scores("maturity", &vocab::MATURITY_DIMENSIONS, scores)?;
    let mut rows = Vec::with_capacity(scores.len());
    for (dimension, score) in scores {
        let sql = format!(
            "INSERT INTO architecture_maturity
                (tenant_id, assessment_id, stage, dimension, score, evidence)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (tenant_id, assessment_id, dimension) DO NOTHING
             RETURNING {MATURITY_COLUMNS}"
        );
        let row: Option<MaturityRecord> = sqlx::query_as(AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(assessment_id)
            .bind(stage.trim())
            .bind(dimension)
            .bind(score)
            .bind(&evidence)
            .fetch_optional(&mut *conn)
            .await?;
        if let Some(row) = row {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// Record all seven skills categories for one role.
pub async fn assess_skills_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    assessment_id: Uuid,
    role: &str,
    job_key: &str,
    scores: &[(String, i32)],
    evidence: Value,
) -> Result<Vec<SkillRecord>, ArchitectureStoreError> {
    require_scores("skills", &vocab::SKILL_CATEGORIES, scores)?;
    let mut rows = Vec::with_capacity(scores.len());
    for (category, proficiency) in scores {
        let sql = format!(
            "INSERT INTO architecture_skill_assessment
                (tenant_id, assessment_id, role, job_key, category, proficiency, evidence)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (tenant_id, assessment_id, role, category) DO NOTHING
             RETURNING {SKILL_COLUMNS}"
        );
        let row: Option<SkillRecord> = sqlx::query_as(AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(assessment_id)
            .bind(role.trim())
            .bind(job_key)
            .bind(category)
            .bind(proficiency)
            .bind(&evidence)
            .fetch_optional(&mut *conn)
            .await?;
        if let Some(row) = row {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// List the maturity rows, newest assessment first; optionally one group or one stage.
pub async fn list_maturity_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    assessment_id: Option<Uuid>,
    stage: Option<&str>,
) -> Result<Vec<MaturityRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {MATURITY_COLUMNS} FROM architecture_maturity
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR assessment_id = $2)
            AND ($3::text IS NULL OR stage = $3)
          ORDER BY assessed_at DESC, assessment_id, dimension"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(assessment_id)
        .bind(stage)
        .fetch_all(&mut *conn)
        .await?)
}

/// List the skills rows, newest assessment first, then role and category.
pub async fn list_skills_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    assessment_id: Option<Uuid>,
    role: Option<&str>,
) -> Result<Vec<SkillRecord>, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {SKILL_COLUMNS} FROM architecture_skill_assessment
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR assessment_id = $2)
            AND ($3::text IS NULL OR role = $3)
          ORDER BY assessed_at DESC, role, category"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(assessment_id)
        .bind(role)
        .fetch_all(&mut *conn)
        .await?)
}

/// Record the maturity development plan as WBS milestones under the capability's project.
pub async fn plan_maturity_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    assessment_id: Option<Uuid>,
    milestones: &[String],
) -> Result<Vec<Uuid>, ArchitectureStoreError> {
    if let Some(id) = assessment_id {
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM architecture_maturity WHERE tenant_id = $1 AND assessment_id = $2
            )",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
        if !present {
            return Err(ArchitectureStoreError::NotFound("maturity assessment"));
        }
    }
    let mut ids = Vec::with_capacity(milestones.len());
    for (position, title) in milestones.iter().enumerate() {
        if title.trim().is_empty() {
            continue;
        }
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO wbs_node
                (tenant_id, project_id, position, kind, title, status, meta)
             VALUES ($1, $2, $3, 'milestone', $4, 'open', $5)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(position as i64)
        .bind(title.trim())
        .bind(json!({ "assessment_id": assessment_id.map(|id| id.to_string()) }))
        .fetch_one(&mut *conn)
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

/// The live contract that governs a delivery project's design gate.
///
/// A project with a contract that is not live (draft or superseded) is refused by NAME, not by
/// absence: the caller has to know whether to approve the draft or request a replacement, and
/// "no live contract" alone cannot say which.
pub async fn assert_design_contract_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<ContractRecord, ArchitectureStoreError> {
    let sql = format!(
        "SELECT {CONTRACT_COLUMNS} FROM architecture_contract
          WHERE tenant_id = $1 AND project_id = $2 AND status = 'live'
          ORDER BY CASE kind
                     WHEN 'delivery_contract' THEN 0
                     WHEN 'statement_of_architecture_work' THEN 1
                     ELSE 2
                   END, approved_at DESC
          LIMIT 1"
    );
    let live: Option<ContractRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(record) = live {
        return Ok(record);
    }
    let other: Option<(String, String)> = sqlx::query_as(
        "SELECT kind, status FROM architecture_contract
          WHERE tenant_id = $1 AND project_id = $2
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_optional(&mut *conn)
    .await?;
    match other {
        Some((kind, status)) => Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::ContractNotLive(format!("{kind} is {status}")),
        )),
        None => Err(ArchitectureStoreError::Illegal(
            ArchitectureRule::DesignContractMissing,
        )),
    }
}
