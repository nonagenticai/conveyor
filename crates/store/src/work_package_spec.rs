//! The delivery-item standard's one home on the card (SPEC_V2 §5.8, decision D69; conveyor#935).
//!
//! A work package — a card of the `delivery` card type, or a card a
//! `wbs_node(kind='work_package')` links to ([`is_work_package_card_tx`]) — carries exactly one
//! spec row: its BDD user story, the Gherkin `feature` document and the INVEST
//! Definition-of-Ready evidence. `cards.spec_put` upserts it keyed by `(tenant_id, card_id)` and
//! bumps `version`; `cards.spec_get` reads it. The parsed scenarios and the acceptance DoD are
//! derived by the caller (from `feature` and the approved `card_done` gate's evidence), never
//! stored here — a second copy would be a second home.
//!
//! The card is resolved in the caller's tenant before the write (CONTRACT §2): the FK alone is a
//! single-column reference and does not enforce tenancy.

use conveyor_domain::{IdError, TenantId};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};

/// A work package spec as stored.
#[derive(Clone, Debug)]
pub struct WorkPackageSpecRecord {
    /// Row id.
    pub id: Uuid,
    /// The owning tenant, decoded back to its domain type.
    pub tenant_id: TenantId,
    /// The work package's card.
    pub card_id: Uuid,
    /// The story's `As a <role>` clause.
    pub story_persona: String,
    /// The story's `I want <capability>` clause.
    pub story_action: String,
    /// The story's `so that <value>` clause.
    pub story_benefit: String,
    /// The Gherkin feature document; the parsed scenarios are derived from it.
    pub feature: String,
    /// The INVEST Definition-of-Ready evidence, keyed by criterion.
    pub dor: Value,
    /// Bumped on every successful upsert, for the caller's `ETag`.
    pub version: i64,
}

/// A spec to write. The card is the caller's separate argument: it is the row's key.
#[derive(Clone, Debug)]
pub struct NewWorkPackageSpec {
    /// The story's `As a <role>` clause; non-blank (the domain validator refuses first).
    pub story_persona: String,
    /// The story's `I want <capability>` clause; non-blank.
    pub story_action: String,
    /// The story's `so that <value>` clause; non-blank.
    pub story_benefit: String,
    /// The Gherkin feature document.
    pub feature: String,
    /// The INVEST DoR evidence document (a JSON object).
    pub dor: Value,
}

/// Why a work-package-spec operation failed.
#[derive(Debug, thiserror::Error)]
pub enum WorkPackageSpecStoreError {
    /// The named card does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// A stored `tenant_id` could not be decoded (impossible under the column type).
    #[error(transparent)]
    Id(#[from] IdError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct WorkPackageSpecRow {
    id: Uuid,
    tenant_id: Uuid,
    card_id: Uuid,
    story_persona: String,
    story_action: String,
    story_benefit: String,
    feature: String,
    dor: Value,
    version: i64,
}

impl TryFrom<WorkPackageSpecRow> for WorkPackageSpecRecord {
    type Error = WorkPackageSpecStoreError;

    fn try_from(row: WorkPackageSpecRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            tenant_id: TenantId::from_uuid(row.tenant_id)?,
            card_id: row.card_id,
            story_persona: row.story_persona,
            story_action: row.story_action,
            story_benefit: row.story_benefit,
            feature: row.feature,
            dor: row.dor,
            version: row.version,
        })
    }
}

const COLUMNS: &str = "id, tenant_id, card_id, story_persona, story_action, story_benefit, \
                       feature, dor, version";

/// Read the spec of one card inside the caller's scoped transaction.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<WorkPackageSpecRecord>, WorkPackageSpecStoreError> {
    let sql =
        format!("SELECT {COLUMNS} FROM work_package_spec WHERE tenant_id = $1 AND card_id = $2");
    let row: Option<WorkPackageSpecRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(WorkPackageSpecRecord::try_from).transpose()
}

/// Write the spec of one card inside the caller's scoped transaction.
///
/// The card is resolved in this tenant first (CONTRACT §2), so a cross-tenant id is
/// [`WorkPackageSpecStoreError::Parent`], never a silent FK acceptance. The upsert keeps one row
/// per `(tenant, card)` and bumps `version` on every write, so a reader holding the previous
/// representation re-reads (CONTRACT §3). Whether the card is a work package at all is the
/// caller's rule ([`is_work_package_card_tx`]); this writer stores what the domain approved.
pub async fn upsert_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    input: &NewWorkPackageSpec,
) -> Result<WorkPackageSpecRecord, WorkPackageSpecStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Card, card_id).await?;
    let sql = format!(
        "INSERT INTO work_package_spec
             (tenant_id, card_id, story_persona, story_action, story_benefit, feature, dor)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (tenant_id, card_id) DO UPDATE
            SET story_persona = excluded.story_persona,
                story_action = excluded.story_action,
                story_benefit = excluded.story_benefit,
                feature = excluded.feature,
                dor = excluded.dor,
                version = work_package_spec.version + 1,
                updated_at = now()
         RETURNING {COLUMNS}"
    );
    let row: WorkPackageSpecRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(&input.story_persona)
        .bind(&input.story_action)
        .bind(&input.story_benefit)
        .bind(&input.feature)
        .bind(&input.dor)
        .fetch_one(&mut *conn)
        .await?;
    WorkPackageSpecRecord::try_from(row)
}

/// Whether an evidence object's named run or step belongs to `card_id` in this tenant.
///
/// This is the ownership half of the work-package Definition of Done: `domain::dod` only asks
/// that an object *names* a scenario key and carries a `run_id`/`step_id`, so without this check
/// a gate could cite another card's run and the scenario would read as verified. A `run_id`
/// resolves against `run` itself; a `step_id` resolves against `run_step` joined to the run that
/// owns it (the schema's step table, `run_step`, is what a run's trace writes). Either identity
/// that resolves on this card makes the evidence the card's own; neither, or a missing pair,
/// answers `false`.
pub async fn evidence_on_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    run_id: Option<Uuid>,
    step_id: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    if let Some(run_id) = run_id {
        let run_owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                        SELECT 1 FROM run
                         WHERE tenant_id = $1 AND id = $2 AND card_id = $3
                    )",
        )
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(card_id)
        .fetch_one(&mut *conn)
        .await?;
        if run_owned {
            return Ok(true);
        }
    }
    let Some(step_id) = step_id else {
        return Ok(false);
    };
    sqlx::query_scalar(
        "SELECT EXISTS (
                    SELECT 1
                      FROM run_step s
                      JOIN run r ON r.tenant_id = s.tenant_id AND r.id = s.run_id
                     WHERE s.tenant_id = $1 AND s.id = $2 AND r.card_id = $3
                )",
    )
    .bind(tenant.as_uuid())
    .bind(step_id)
    .bind(card_id)
    .fetch_one(&mut *conn)
    .await
}

/// Whether the card is a work package under SPEC_V2 §5.8.
///
/// True for a card of the `delivery` card type, and for a card a
/// `wbs_node(kind='work_package')` links to. A meeting (D66), coach or system card answers
/// `false`: the standard does not apply to it. This is the one scope predicate; the caller
/// decides what a non-work-package card may do and never re-spells the test.
pub async fn is_work_package_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
                    SELECT 1
                      FROM card c
                      JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
                     WHERE c.tenant_id = $1 AND c.id = $2 AND t.code = 'delivery'
                )
                OR EXISTS (
                    SELECT 1 FROM wbs_node w
                     WHERE w.tenant_id = $1 AND w.card_id = $2 AND w.kind = 'work_package'
                )",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *conn)
    .await
}
