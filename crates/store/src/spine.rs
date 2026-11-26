#![forbid(unsafe_code)]
//! T1 — the plan spine's queries (SPEC_V2 §5.1, §5.3–§5.5, §8.2, §8.3; issue #72).
//!
//! Every function is `_tx`: the caller has already opened a tenant-scoped transaction, and the
//! write shares it with the action's outbox event (CONTRACT §7). No function here speaks the
//! action layer's vocabulary; a refusal the domain owns is raised as [`PlanStoreError::Illegal`]
//! and mapped once, by the action.
//!
//! The one read model is [`in_flight_tx`]: it selects projects **and** programmes by the
//! tenant's configured in-flight set (`tenant.settings -> 'portfolio' -> 'in_flight'`, default
//! `active,paused`). There is no portfolio row (D-T1-7).

use chrono::{DateTime, NaiveDate, Utc};
use conveyor_domain::spine::{
    BenefitMapFields, BenefitProfileFields, PlanError, WsjfInputs, benefit_map_missing,
    benefit_profile_missing, evidence_present, tranche_review_has_evidence,
    validate_benefit_reading, validate_change,
};
use conveyor_domain::{Approval, TenantId};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

use crate::approval::{ApprovalStoreError, NewApproval, get_approval_tx, insert_approval_tx};
use crate::parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};

/// The plan spine's failures.
#[derive(Debug, thiserror::Error)]
pub enum PlanStoreError {
    /// No such row in this tenant.
    #[error("the {what} does not exist in this tenant")]
    NotFound {
        /// The row's noun.
        what: &'static str,
    },
    /// The `If-Match` version is stale.
    #[error("the {what} changed since it was read")]
    Stale {
        /// The row's noun.
        what: &'static str,
    },
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The domain refused the write.
    #[error(transparent)]
    Illegal(#[from] PlanError),
    /// A work-package write named no card, so nothing executes it (SPEC_V2 §5.1, v1-p5-5).
    /// Raised by the follow-up slice that turns the chain into a hard rule; the variant is the
    /// action layer's `422 card_required`.
    #[error("a work package must name the card that executes it")]
    CardRequired,
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// The row cannot be removed while a dependent row names it.
    #[error("the {what} cannot be removed: {why}")]
    Conflict {
        /// The row's noun.
        what: &'static str,
        /// Why the removal is refused.
        why: &'static str,
    },
    /// A PI plan has no decision a person made, so nothing may materialise.
    #[error("pi_plan_not_approved")]
    PiPlanNotApproved,
    /// The approval surface refused the plan's gate or its decision.
    #[error(transparent)]
    Approval(#[from] ApprovalStoreError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

// ── WBS ───────────────────────────────────────────────────────────────────────────────────

/// One WBS node as the actions read it. The WSJF score is deliberately absent: it is computed
/// from [`inputs`](Self::inputs) on read (D-T1-4).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct WbsRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning project.
    pub project_id: Uuid,
    /// The parent node, if any.
    pub parent_id: Option<Uuid>,
    /// Sibling order.
    pub position: f64,
    /// `phase | deliverable | work_package | milestone`.
    pub kind: String,
    /// The node's title.
    pub title: String,
    /// The node's body.
    pub body: String,
    /// `open | accepted | cancelled`.
    pub status: String,
    /// What must be exhibited to accept the node.
    pub acceptance: Value,
    /// What met the acceptance.
    pub evidence: Value,
    /// The work package's card, if any.
    pub card_id: Option<Uuid>,
    /// WSJF business value.
    pub business_value: i32,
    /// WSJF time criticality.
    pub time_criticality: i32,
    /// WSJF risk reduction.
    pub risk_reduction: i32,
    /// WSJF job size.
    pub job_size: i32,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

impl WbsRecord {
    /// The four stored inputs.
    pub fn inputs(&self) -> WsjfInputs {
        WsjfInputs {
            business_value: self.business_value,
            time_criticality: self.time_criticality,
            risk_reduction: self.risk_reduction,
            job_size: self.job_size,
        }
    }
}

const WBS_COLUMNS: &str = "id, project_id, parent_id, position::float8 AS position, kind, title, body, \
                           status, acceptance, evidence, card_id, business_value, time_criticality, \
                           risk_reduction, job_size, meta, version, created_at, updated_at";

/// A new WBS node.
#[derive(Clone, Debug, Default)]
pub struct NewWbsNode {
    /// The owning project.
    pub project_id: Uuid,
    /// The parent node.
    pub parent_id: Option<Uuid>,
    /// Sibling order; absent appends after the current siblings.
    pub position: Option<f64>,
    /// The kind (validated against the domain vocabulary by the action).
    pub kind: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// Acceptance criteria.
    pub acceptance: Value,
    /// The card that executes this work package. `None` is an unlinked node; a `work_package`
    /// node owes one (the follow-up slice turns that into the store's refusal). When named, the
    /// card must belong to this project in this tenant.
    pub card_id: Option<Uuid>,
    /// The four WSJF inputs.
    pub inputs: WsjfInputs,
    /// Free-form metadata.
    pub meta: Value,
}

/// The editable fields of a WBS node.
#[derive(Clone, Debug, Default)]
pub struct WbsPatch {
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
    /// The four inputs, when all four are supplied (never a partial score).
    pub inputs: Option<WsjfInputs>,
    /// A new sibling order.
    pub position: Option<f64>,
    /// A new parent.
    pub parent_id: Option<Uuid>,
}

/// Resolve a job catalogue row inside the caller's tenant.
async fn ensure_job_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<(), PlanStoreError> {
    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM job_catalogue WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if !present {
        return Err(PlanStoreError::NotFound { what: "job" });
    }
    Ok(())
}

/// Resolve a project inside the caller's tenant.
pub async fn require_project_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<(), PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Project, id)
        .await
        .map_err(PlanStoreError::Parent)
}

/// Insert a WBS node.
pub async fn insert_wbs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewWbsNode,
) -> Result<WbsRecord, PlanStoreError> {
    require_project_tx(&mut *conn, tenant, new.project_id).await?;
    // The card is the work package's executor: when one is named it must be a card of THIS
    // project in THIS tenant. `card` carries no project column, so the project is reached the
    // only way it exists — card → board → project. The FK alone would accept another tenant's
    // card because referential checks bypass RLS (CONTRACT §2).
    if let Some(card) = new.card_id {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                   FROM card c
                   JOIN board b ON b.id = c.board_id
                  WHERE c.tenant_id = $1 AND c.id = $2 AND b.tenant_id = $1
                    AND b.project_id = $3
             )",
        )
        .bind(tenant.as_uuid())
        .bind(card)
        .bind(new.project_id)
        .fetch_one(&mut *conn)
        .await?;
        if !owned {
            return Err(PlanStoreError::Parent(ParentError::NotFound {
                parent: Parent::Card,
            }));
        }
    }
    if let Some(parent) = new.parent_id {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM wbs_node WHERE tenant_id = $1 AND id = $2 AND project_id = $3)",
        )
        .bind(tenant.as_uuid())
        .bind(parent)
        .bind(new.project_id)
        .fetch_one(&mut *conn)
        .await?;
        if !owned {
            return Err(PlanStoreError::Parent(ParentError::NotFound {
                parent: Parent::Project,
            }));
        }
    }
    let position = match new.position {
        Some(position) => position,
        None => {
            sqlx::query_scalar(
                "SELECT coalesce(max(position::float8), 0) + 1 FROM wbs_node
                  WHERE tenant_id = $1 AND project_id = $2 AND parent_id IS NOT DISTINCT FROM $3",
            )
            .bind(tenant.as_uuid())
            .bind(new.project_id)
            .bind(new.parent_id)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    let sql = format!(
        "INSERT INTO wbs_node (tenant_id, project_id, parent_id, position, kind, title, body,
                               acceptance, card_id, business_value, time_criticality,
                               risk_reduction, job_size, meta)
         VALUES ($1, $2, $3, $4::float8::numeric, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING {WBS_COLUMNS}"
    );
    let row: WbsRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.project_id)
        .bind(new.parent_id)
        .bind(position)
        .bind(&new.kind)
        .bind(&new.title)
        .bind(&new.body)
        .bind(&new.acceptance)
        .bind(new.card_id)
        .bind(new.inputs.business_value)
        .bind(new.inputs.time_criticality)
        .bind(new.inputs.risk_reduction)
        .bind(new.inputs.job_size)
        .bind(&new.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one WBS node inside the caller's tenant.
pub async fn get_wbs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<WbsRecord>, PlanStoreError> {
    let sql = format!("SELECT {WBS_COLUMNS} FROM wbs_node WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Update one WBS node under an `If-Match` version.
#[allow(clippy::too_many_arguments)]
pub async fn update_wbs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    patch: &WbsPatch,
) -> Result<WbsRecord, PlanStoreError> {
    let current = get_wbs_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "WBS node" })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale { what: "WBS node" });
    }
    let inputs = patch.inputs.unwrap_or_else(|| current.inputs());
    let position = patch.position.unwrap_or(current.position);
    let sql = format!(
        "UPDATE wbs_node SET
            title = coalesce($4, title), body = coalesce($5, body),
            status = coalesce($6, status), acceptance = coalesce($7, acceptance),
            evidence = coalesce($8, evidence), card_id = coalesce($9, card_id),
            business_value = $10, time_criticality = $11, risk_reduction = $12,
            job_size = $13, position = $14::float8::numeric,
            parent_id = coalesce($15, parent_id),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {WBS_COLUMNS}"
    );
    let row: Option<WbsRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(&patch.title)
        .bind(&patch.body)
        .bind(&patch.status)
        .bind(&patch.acceptance)
        .bind(&patch.evidence)
        .bind(patch.card_id)
        .bind(inputs.business_value)
        .bind(inputs.time_criticality)
        .bind(inputs.risk_reduction)
        .bind(inputs.job_size)
        .bind(position)
        .bind(patch.parent_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale { what: "WBS node" })
}

/// List a project's WBS nodes, parents first, in sibling order.
pub async fn list_wbs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<WbsRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {WBS_COLUMNS} FROM wbs_node
          WHERE tenant_id = $1 AND project_id = $2
          ORDER BY position, created_at, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Remove a WBS node. A node with children is refused: removing a parent must not silently
/// remove its subtree.
pub async fn delete_wbs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let current = get_wbs_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "WBS node" })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale { what: "WBS node" });
    }
    let has_children: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM wbs_node WHERE tenant_id = $1 AND parent_id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if has_children {
        return Err(PlanStoreError::Conflict {
            what: "WBS node",
            why: "it has child nodes",
        });
    }
    let changed =
        sqlx::query("DELETE FROM wbs_node WHERE tenant_id = $1 AND id = $2 AND version = $3")
            .bind(tenant.as_uuid())
            .bind(id)
            .bind(expected)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale { what: "WBS node" });
    }
    Ok(())
}

// ── project_baseline ─────────────────────────────────────────────────────────────────────

/// A frozen baseline as the actions read it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BaselineRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning project.
    pub project_id: Uuid,
    /// `scope | schedule | cost`.
    pub kind: String,
    /// The frozen snapshot.
    pub snapshot: Value,
    /// When it was taken.
    pub taken_at: DateTime<Utc>,
    /// Who took it.
    pub taken_by: Uuid,
    /// Who approved it, if anyone.
    pub approved_by: Option<Uuid>,
    /// The approved change request that advanced to this baseline (T3, #74), if any.
    pub change_id: Option<Uuid>,
    /// The If-Match version (append-only rows keep 1).
    pub version: i64,
}

const BASELINE_COLUMNS: &str =
    "id, project_id, kind, snapshot, taken_at, taken_by, approved_by, change_id, version";

/// Take a baseline: snapshot the current WBS + register facts and freeze them.
///
/// The snapshot is built from a read of the project's rows and then inserted. A later row
/// change never touches it (D-T1-2) — that is why the test can change a WBS row and read the
/// same snapshot back.
pub async fn take_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    kind: &str,
    taken_by: Uuid,
    change_id: Option<Uuid>,
) -> Result<BaselineRecord, PlanStoreError> {
    require_project_tx(&mut *conn, tenant, project_id).await?;
    ensure_parent_tx(&mut *conn, tenant, Parent::AppUser, taken_by)
        .await
        .map_err(PlanStoreError::Parent)?;
    let wbs: Vec<WbsRecord> = list_wbs_tx(&mut *conn, tenant, project_id).await?;
    let register: Vec<RegisterRecord> =
        list_register_tx(&mut *conn, tenant, project_id, None).await?;
    let snapshot = serde_json::json!({
        "project_id": project_id,
        "wbs": wbs.iter().map(|node| serde_json::json!({
            "id": node.id, "parent_id": node.parent_id, "position": node.position,
            "kind": node.kind, "title": node.title, "status": node.status,
            "business_value": node.business_value, "time_criticality": node.time_criticality,
            "risk_reduction": node.risk_reduction, "job_size": node.job_size,
        })).collect::<Vec<_>>(),
        "register": register.iter().map(|item| serde_json::json!({
            "id": item.id, "kind": item.kind, "code": item.code, "title": item.title,
            "status": item.status, "severity": item.severity, "impact": item.impact,
        })).collect::<Vec<_>>(),
        "frozen": true,
    });
    let sql = format!(
        "INSERT INTO project_baseline (tenant_id, project_id, kind, snapshot, taken_by, change_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {BASELINE_COLUMNS}"
    );
    let row: BaselineRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(kind)
        .bind(&snapshot)
        .bind(taken_by)
        .bind(change_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// The newest approved baseline of `kind` for a project, or `None`.
///
/// T3's change guard: a project that already holds an approved baseline of a kind cannot
/// advance past it without a change request, so this is the row the guard reads.
pub async fn latest_approved_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    kind: &str,
) -> Result<Option<BaselineRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {BASELINE_COLUMNS} FROM project_baseline
          WHERE tenant_id = $1 AND project_id = $2 AND kind = $3 AND approved_by IS NOT NULL
          ORDER BY taken_at DESC, id DESC LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(kind)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Mark a baseline approved by `approver`. Only an unapproved row moves; a second approval is a
/// no-op that returns the row unchanged (the gate effect is idempotent under re-decision).
pub async fn approve_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    approver: Uuid,
) -> Result<Option<BaselineRecord>, PlanStoreError> {
    let sql = format!(
        "UPDATE project_baseline SET approved_by = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND approved_by IS NULL
          RETURNING {BASELINE_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(approver)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Mark a change register item approved by `approver`.
pub async fn approve_register_item_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    approver: Uuid,
) -> Result<Option<RegisterRecord>, PlanStoreError> {
    let sql = format!(
        "UPDATE project_register_item
            SET status = 'approved', meta = meta || jsonb_build_object('approved_by', $3::text),
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND kind = 'change' AND status <> 'approved'
          RETURNING {REGISTER_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(approver.to_string())
        .fetch_optional(&mut *conn)
        .await?)
}

/// Write the reciprocal citation from a change request to the baseline it advanced.
pub async fn link_change_to_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    change_id: Uuid,
    baseline_id: Uuid,
) -> Result<Option<RegisterRecord>, PlanStoreError> {
    let sql = format!(
        "UPDATE project_register_item
            SET meta = meta || jsonb_build_object('baseline_id', $3::text), updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND kind = 'change'
          RETURNING {REGISTER_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(change_id)
        .bind(baseline_id.to_string())
        .fetch_optional(&mut *conn)
        .await?)
}

/// Read one baseline.
pub async fn get_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<BaselineRecord>, PlanStoreError> {
    let sql =
        format!("SELECT {BASELINE_COLUMNS} FROM project_baseline WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a project's baselines, newest first.
pub async fn list_baselines_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<BaselineRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {BASELINE_COLUMNS} FROM project_baseline
          WHERE tenant_id = $1 AND project_id = $2
          ORDER BY taken_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?)
}

// ── project_register_item ───────────────────────────────────────────────────────────────

/// One register row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RegisterRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning project; null for a programme-level row.
    pub project_id: Option<Uuid>,
    /// The owning programme; null for a project-level row.
    pub programme_id: Option<Uuid>,
    /// `risk | issue | assumption | dependency | benefit | change`.
    pub kind: String,
    /// The human code.
    pub code: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// The owning job.
    pub owner_job_id: Option<Uuid>,
    /// The status.
    pub status: String,
    /// The severity.
    pub severity: Option<String>,
    /// The probability.
    pub probability: Option<String>,
    /// The impact (0–10, or null).
    pub impact: Option<i32>,
    /// Related links.
    pub links: Value,
    /// The baseline a change changes.
    pub baseline_ref: Option<Uuid>,
    /// When the row is next reviewed (a risk's review date).
    pub review_at: Option<NaiveDate>,
    /// The recorded response (a risk's mitigation).
    pub response: Option<String>,
    /// Free-form metadata (a change's impact block lives here).
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const REGISTER_COLUMNS: &str = "id, project_id, programme_id, kind, code, title, body, \
                                owner_job_id, status, severity, probability, impact, links, \
                                baseline_ref, review_at, response, meta, version, created_at, \
                                updated_at";

/// The declared §5.5 link document with every array empty: the column's default, and the value
/// an omitted `links` stores.
pub fn empty_register_links() -> Value {
    serde_json::json!({ "cards": [], "requirements": [], "decisions": [] })
}

/// The document the register writers persist (SPEC_V2 §5.5). A legacy bare array is the old
/// `[]` default or the benefit citation; its entries move under `requirements`, exactly as 0098
/// normalises stored rows, and each entry keeps its own `kind`. Every other non-conforming
/// value becomes the empty document: an internal caller has no client to refuse, and the
/// column's CHECK would otherwise fail the whole transaction.
fn register_links_document(links: &Value) -> Value {
    let Some(map) = links.as_object() else {
        let mut document = empty_register_links();
        if let Some(items) = links.as_array() {
            document["requirements"] = Value::Array(items.clone());
        }
        return document;
    };
    let mut document = empty_register_links();
    for key in ["cards", "requirements", "decisions"] {
        if let Some(items) = map.get(key).and_then(Value::as_array) {
            document[key] = Value::Array(items.clone());
        }
    }
    document
}

/// A new register row.
#[derive(Clone, Debug, Default)]
pub struct NewRegisterItem {
    /// The owning project; exactly one of this and `programme_id` is set.
    pub project_id: Option<Uuid>,
    /// The owning programme; exactly one of this and `project_id` is set.
    pub programme_id: Option<Uuid>,
    /// The kind.
    pub kind: String,
    /// The human code.
    pub code: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// The owning job.
    pub owner_job_id: Option<Uuid>,
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
    pub baseline_ref: Option<Uuid>,
    /// When the row is next reviewed.
    pub review_at: Option<NaiveDate>,
    /// The recorded response.
    pub response: Option<String>,
    /// Metadata; a change's impact block lives here.
    pub meta: Value,
}

/// Insert a register row. A change is validated before the insert (D-T1-3).
pub async fn insert_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewRegisterItem,
) -> Result<RegisterRecord, PlanStoreError> {
    if let Some(project) = new.project_id {
        require_project_tx(&mut *conn, tenant, project).await?;
    }
    if let Some(programme) = new.programme_id {
        ensure_parent_tx(&mut *conn, tenant, Parent::Programme, programme)
            .await
            .map_err(PlanStoreError::Parent)?;
    }
    if let Some(owner) = new.owner_job_id {
        ensure_job_tx(&mut *conn, tenant, owner).await?;
    }
    if let Some(baseline) = new.baseline_ref {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM project_baseline WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(tenant.as_uuid())
        .bind(baseline)
        .fetch_one(&mut *conn)
        .await?;
        if !owned {
            return Err(PlanStoreError::Parent(ParentError::NotFound {
                parent: Parent::Project,
            }));
        }
    }
    if new.kind == "change" {
        validate_change(new.meta.get("impact"), new.baseline_ref)?;
    }
    let links = register_links_document(&new.links);
    let sql = format!(
        "INSERT INTO project_register_item
            (tenant_id, project_id, programme_id, kind, code, title, body, owner_job_id, status,
             severity, probability, impact, links, baseline_ref, review_at, response, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         RETURNING {REGISTER_COLUMNS}"
    );
    let row: RegisterRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.project_id)
        .bind(new.programme_id)
        .bind(&new.kind)
        .bind(&new.code)
        .bind(&new.title)
        .bind(&new.body)
        .bind(new.owner_job_id)
        .bind(&new.status)
        .bind(&new.severity)
        .bind(&new.probability)
        .bind(new.impact)
        .bind(&links)
        .bind(new.baseline_ref)
        .bind(new.review_at)
        .bind(&new.response)
        .bind(&new.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one register row.
pub async fn get_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RegisterRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {REGISTER_COLUMNS} FROM project_register_item WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a project's register rows. Programme-level rows have a null `project_id` and are
/// deliberately not returned here: they belong to [`list_programme_register_tx`].
pub async fn list_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
    kind: Option<&str>,
) -> Result<Vec<RegisterRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {REGISTER_COLUMNS} FROM project_register_item
          WHERE tenant_id = $1 AND project_id = $2
            AND ($3::text IS NULL OR kind = $3)
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(kind)
        .fetch_all(&mut *conn)
        .await?)
}

/// List a programme's register rows. The null `project_id` is the scope, not a defect, so this
/// read filters on `programme_id` and never on the project column.
pub async fn list_programme_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    kind: Option<&str>,
) -> Result<Vec<RegisterRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {REGISTER_COLUMNS} FROM project_register_item
          WHERE tenant_id = $1 AND programme_id = $2
            AND ($3::text IS NULL OR kind = $3)
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(kind)
        .fetch_all(&mut *conn)
        .await?)
}

/// Update a register row under `If-Match`.
#[allow(clippy::too_many_arguments)]
pub async fn update_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    programme_id: Option<Uuid>,
    status: Option<&str>,
    title: Option<&str>,
    body: Option<&str>,
    severity: Option<&str>,
    probability: Option<&str>,
    owner_job_id: Option<Uuid>,
    impact: Option<i32>,
    links: Option<&Value>,
    review_at: Option<NaiveDate>,
    response: Option<&str>,
    meta: Option<&Value>,
) -> Result<RegisterRecord, PlanStoreError> {
    let current =
        get_register_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "register item",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "register item",
        });
    }
    if let Some(programme) = programme_id
        && current.programme_id != Some(programme)
    {
        ensure_parent_tx(&mut *conn, tenant, Parent::Programme, programme)
            .await
            .map_err(PlanStoreError::Parent)?;
    }
    // The create rule is not a one-time admission: an update that replaces `meta` replaces the
    // impact block too, so it is validated against the same §5.5 rule. The row's baseline is
    // immutable here, so the stored `baseline_ref` is the one the new block belongs with.
    if current.kind == "change"
        && let Some(meta) = meta
    {
        validate_change(meta.get("impact"), current.baseline_ref)?;
    }
    let links = links.map(register_links_document);
    let sql = format!(
        "UPDATE project_register_item SET
            programme_id = coalesce($4, programme_id), status = coalesce($5, status),
            title = coalesce($6, title), body = coalesce($7, body),
            severity = coalesce($8, severity), probability = coalesce($9, probability),
            owner_job_id = coalesce($10, owner_job_id), impact = coalesce($11, impact),
            links = coalesce($12, links),
            review_at = coalesce($13, review_at), response = coalesce($14, response),
            meta = coalesce($15, meta),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {REGISTER_COLUMNS}"
    );
    let row: Option<RegisterRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(programme_id)
        .bind(status)
        .bind(title)
        .bind(body)
        .bind(severity)
        .bind(probability)
        .bind(owner_job_id)
        .bind(impact)
        .bind(links.as_ref())
        .bind(review_at)
        .bind(response)
        .bind(meta)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale {
        what: "register item",
    })
}

/// Remove a register item. An item a benefit profile indexes is refused: the measurement record
/// must keep its register row.
pub async fn delete_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let current =
        get_register_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "register item",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "register item",
        });
    }
    let referenced: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM benefit_profile WHERE tenant_id = $1 AND register_item_id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if referenced {
        return Err(PlanStoreError::Conflict {
            what: "register item",
            why: "a benefit profile indexes it",
        });
    }
    let changed = sqlx::query(
        "DELETE FROM project_register_item WHERE tenant_id = $1 AND id = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale {
            what: "register item",
        });
    }
    Ok(())
}

// ── project_dependency ──────────────────────────────────────────────────────────────────

/// A dependency edge.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DependencyRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme, when the edge belongs to one.
    pub programme_id: Option<Uuid>,
    /// The source project.
    pub from_project: Uuid,
    /// The target project.
    pub to_project: Uuid,
    /// `blocks | needs | supersedes | informs`.
    pub kind: String,
    /// The edge's status.
    pub status: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
}

const DEPENDENCY_COLUMNS: &str =
    "id, programme_id, from_project, to_project, kind, status, meta, version";

/// Insert an edge, refusing a cycle in the same transaction (D-T1-6).
pub async fn insert_dependency_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Option<Uuid>,
    from: Uuid,
    to: Uuid,
    kind: &str,
    meta: &Value,
) -> Result<DependencyRecord, PlanStoreError> {
    if from == to {
        return Err(PlanStoreError::Illegal(PlanError::SelfDependency));
    }
    require_project_tx(&mut *conn, tenant, from).await?;
    require_project_tx(&mut *conn, tenant, to).await?;
    if let Some(programme) = programme_id {
        ensure_parent_tx(&mut *conn, tenant, Parent::Programme, programme)
            .await
            .map_err(PlanStoreError::Parent)?;
    }
    // Serialise edge creation per tenant: without it two concurrent inserts can each see an
    // acyclic graph and commit a cycle. The lock is transaction-scoped and released with the
    // insert, so it never outlives the mutation.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(tenant.as_uuid())
        .execute(&mut *conn)
        .await?;
    // A cycle exists when `to` can already reach `from` through the existing edges.
    let reaches: bool = sqlx::query_scalar(
        "WITH RECURSIVE reach(id) AS (
             SELECT to_project FROM project_dependency WHERE tenant_id = $1 AND from_project = $2
             UNION
             SELECT d.to_project FROM project_dependency d
               JOIN reach r ON d.from_project = r.id
              WHERE d.tenant_id = $1
         )
         SELECT EXISTS (SELECT 1 FROM reach WHERE id = $3)",
    )
    .bind(tenant.as_uuid())
    .bind(to)
    .bind(from)
    .fetch_one(&mut *conn)
    .await?;
    if reaches {
        return Err(PlanStoreError::Illegal(PlanError::DependencyCycle {
            from,
            to,
        }));
    }
    let sql = format!(
        "INSERT INTO project_dependency
            (tenant_id, programme_id, from_project, to_project, kind, meta)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {DEPENDENCY_COLUMNS}"
    );
    let row: DependencyRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(from)
        .bind(to)
        .bind(kind)
        .bind(meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Delete an edge.
pub async fn delete_dependency_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<(), PlanStoreError> {
    let changed = sqlx::query("DELETE FROM project_dependency WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::NotFound { what: "dependency" });
    }
    Ok(())
}

/// List a project's edges (both directions).
pub async fn list_dependencies_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<DependencyRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {DEPENDENCY_COLUMNS} FROM project_dependency
          WHERE tenant_id = $1 AND (from_project = $2 OR to_project = $2)
          ORDER BY created_at, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?)
}

// ── portfolio.in_flight (a read model, never a row) ─────────────────────────────────────

/// One in-flight project.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct InFlightRecord {
    /// The project id.
    pub id: Uuid,
    /// The project key.
    pub key: String,
    /// The name.
    pub name: String,
    /// The status.
    pub status: String,
    /// The programme, if any.
    pub programme_id: Option<Uuid>,
    /// The If-Match version.
    pub version: i64,
}

/// One in-flight programme. A programme with no projects is still in flight.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct InFlightProgrammeRecord {
    /// The programme id.
    pub id: Uuid,
    /// The name.
    pub name: String,
    /// The status.
    pub status: String,
    /// The If-Match version.
    pub version: i64,
}

/// The computed portfolio: every project and programme in the tenant's in-flight set.
#[derive(Clone, Debug, Default)]
pub struct InFlight {
    /// The in-flight projects.
    pub projects: Vec<InFlightRecord>,
    /// The in-flight programmes.
    pub programmes: Vec<InFlightProgrammeRecord>,
}

/// The tenant's in-flight set: `tenant.settings -> 'portfolio' -> 'in_flight'`, default
/// `["active", "paused"]` (D-T1-7). A malformed or empty value falls back to the default.
pub async fn in_flight_set_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<String>, PlanStoreError> {
    let raw: Option<Value> = sqlx::query_scalar(
        "SELECT settings -> 'portfolio' -> 'in_flight' FROM tenant WHERE id = $1",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    let mut set: Vec<String> = raw
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|status| !status.trim().is_empty())
        .collect();
    if set.is_empty() {
        set = vec!["active".to_owned(), "paused".to_owned()];
    }
    Ok(set)
}

/// List the portfolio in flight: the projects and the programmes whose status is in the
/// tenant's configured in-flight set. Membership is derived for both kinds (SPEC_V2 §5.1), so
/// an in-flight programme appears on its own status even with zero projects. Read only.
pub async fn in_flight_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<InFlight, PlanStoreError> {
    let set = in_flight_set_tx(&mut *conn, tenant).await?;
    let projects_sql = "SELECT id, key, name, status, programme_id, version FROM project
                WHERE tenant_id = $1 AND status = ANY($2)
                ORDER BY updated_at DESC, id DESC";
    let projects: Vec<InFlightRecord> = sqlx::query_as(AssertSqlSafe(projects_sql))
        .bind(tenant.as_uuid())
        .bind(&set)
        .fetch_all(&mut *conn)
        .await?;
    let programmes_sql = "SELECT id, name, status, version FROM programme
                WHERE tenant_id = $1 AND status = ANY($2)
                ORDER BY updated_at DESC, id DESC";
    let programmes: Vec<InFlightProgrammeRecord> = sqlx::query_as(AssertSqlSafe(programmes_sql))
        .bind(tenant.as_uuid())
        .bind(&set)
        .fetch_all(&mut *conn)
        .await?;
    Ok(InFlight {
        projects,
        programmes,
    })
}

// ── programme_tranche ───────────────────────────────────────────────────────────────────

/// A tranche.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TrancheRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme.
    pub programme_id: Uuid,
    /// The tranche number.
    pub number: i32,
    /// The state.
    pub state: String,
    /// Entry criteria.
    pub entry_criteria: Value,
    /// Exit criteria.
    pub exit_criteria: Value,
    /// The baseline it is managed against.
    pub baseline_ref: Option<Uuid>,
    /// The review date.
    pub review_at: Option<DateTime<Utc>>,
    /// Why the tranche was retired; empty unless `state='retired'`.
    pub retired_reason: String,
    /// When the tranche was authorised, if it has been.
    pub authorised_at: Option<DateTime<Utc>>,
    /// The review that authorised it.
    pub authorisation_review_id: Option<Uuid>,
    /// The If-Match version.
    pub version: i64,
}

const TRANCHE_COLUMNS: &str = "id, programme_id, number, state, entry_criteria, exit_criteria, \
    baseline_ref, review_at, retired_reason, authorised_at, authorisation_review_id, version";

/// Create a tranche.
pub async fn insert_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    number: i32,
    entry_criteria: &Value,
    exit_criteria: &Value,
    review_at: Option<DateTime<Utc>>,
) -> Result<TrancheRecord, PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Programme, programme_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    let sql = format!(
        "INSERT INTO programme_tranche
            (tenant_id, programme_id, number, entry_criteria, exit_criteria, review_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {TRANCHE_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(number)
        .bind(entry_criteria)
        .bind(exit_criteria)
        .bind(review_at)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one tranche.
pub async fn get_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<TrancheRecord>, PlanStoreError> {
    let sql =
        format!("SELECT {TRANCHE_COLUMNS} FROM programme_tranche WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a programme's tranches. A retired tranche leaves the default view (#303) but remains
/// readable by id and is returned again with `include_retired`.
pub async fn list_tranches_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    include_retired: bool,
) -> Result<Vec<TrancheRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {TRANCHE_COLUMNS} FROM programme_tranche
          WHERE tenant_id = $1 AND programme_id = $2 AND ($3 OR state <> 'retired')
          ORDER BY number"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(include_retired)
        .fetch_all(&mut *conn)
        .await?)
}

/// The ids a validated evidence class cites.
fn tranche_evidence_ids(evidence: &Value, class: &str) -> Vec<Uuid> {
    evidence
        .get(class)
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter_map(|text| Uuid::parse_str(text).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Refuse review evidence that names no live row of the reviewed tranche's programme.
///
/// A UUID in the evidence is a citation only when it resolves: a baseline is an approved
/// management/governance/boundary baseline of the programme, a risk is a `risk` register item
/// of a project in the programme, and a benefit status is a benefit row of the programme. A
/// well-formed id that names nothing is an assertion dressed as evidence, which is exactly what
/// this rule refuses.
async fn resolve_tranche_review_evidence_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    evidence: &Value,
) -> Result<(), PlanStoreError> {
    for id in tranche_evidence_ids(evidence, "baselines") {
        let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
            "SELECT kind, approved_by FROM programme_baseline
              WHERE tenant_id = $1 AND programme_id = $2 AND id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((kind, approved_by)) = row else {
            return Err(PlanStoreError::Illegal(
                PlanError::TrancheReviewEvidenceUnresolved { class: "baselines" },
            ));
        };
        if approved_by.is_none()
            || !matches!(kind.as_str(), "management" | "governance" | "boundary")
        {
            return Err(PlanStoreError::Illegal(
                PlanError::TrancheBaselineNotApproved { kind },
            ));
        }
    }
    for id in tranche_evidence_ids(evidence, "risk_register") {
        let resolves: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                  FROM project_register_item r
                  JOIN project p ON p.tenant_id = r.tenant_id AND p.id = r.project_id
                 WHERE r.tenant_id = $1 AND r.id = $2 AND r.kind = 'risk'
                   AND p.programme_id = $3
            )",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(programme_id)
        .fetch_one(&mut *conn)
        .await?;
        if !resolves {
            return Err(PlanStoreError::Illegal(
                PlanError::TrancheReviewEvidenceUnresolved {
                    class: "risk_register",
                },
            ));
        }
    }
    let benefits = tranche_evidence_ids(evidence, "benefit_status");
    if !benefits.is_empty() {
        // A measurement row is history behind a profile; a deployment without the measurement
        // table cites profiles only.
        let has_measurements: bool =
            sqlx::query_scalar("SELECT to_regclass('benefit_measurement') IS NOT NULL")
                .fetch_one(&mut *conn)
                .await?;
        for id in benefits {
            let profile: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM benefit_profile
                     WHERE tenant_id = $1 AND programme_id = $2 AND id = $3
                )",
            )
            .bind(tenant.as_uuid())
            .bind(programme_id)
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
            let measurement = if has_measurements {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (
                        SELECT 1 FROM benefit_measurement
                         WHERE tenant_id = $1 AND programme_id = $2 AND id = $3
                    )",
                )
                .bind(tenant.as_uuid())
                .bind(programme_id)
                .bind(id)
                .fetch_one(&mut *conn)
                .await?
            } else {
                false
            };
            if !profile && !measurement {
                return Err(PlanStoreError::Illegal(
                    PlanError::TrancheReviewEvidenceUnresolved {
                        class: "benefit_status",
                    },
                ));
            }
        }
    }
    Ok(())
}

/// Record a tranche review decision, refusing an evidence-less review (D-T1-10).
///
/// The review is an MSP record, not a status: the decision and the evidence are inserted before
/// the state transition in the caller's transaction, so a review can be read back and a failure
/// leaves neither row changed.
pub async fn review_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    decision: &str,
    evidence: &Value,
    reviewed_by: Uuid,
) -> Result<TrancheRecord, PlanStoreError> {
    if !tranche_review_has_evidence(evidence) {
        return Err(PlanStoreError::Illegal(
            PlanError::TrancheReviewEvidenceMissing,
        ));
    }
    let state = match decision {
        "continue" => "closed",
        "pause" => "paused",
        "stop" => "stopped",
        other => {
            return Err(PlanStoreError::Vocabulary {
                what: "tranche decision",
                value: other.to_owned(),
            });
        }
    };
    // Lock the reviewed tranche: two concurrent reviews of the same close must not both pass
    // the one-review check. The lock is held until the transaction commits, so the second
    // reviewer waits, sees the first review and is refused.
    let sql = format!(
        "SELECT {TRANCHE_COLUMNS} FROM programme_tranche
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let reviewed: TrancheRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "tranche" })?;
    // Evidence is citation, not prose: every id must resolve inside the reviewed programme.
    resolve_tranche_review_evidence_tx(&mut *conn, tenant, reviewed.programme_id, evidence).await?;
    // A tranche close has one review, with ONE exception (#526 v1-tr2-1): a tranche whose
    // honest review PAUSED it and which was then resumed under its approved `tranche.resume`
    // gate — back in `in_review` — records the second, dated review that can authorise its
    // successor. The pause stays on record, so the dead end an unreviewable resume would leave
    // (a programme that can never advance past a paused tranche) is closed without letting a
    // second review of any other row through. The check runs before the state guard so a second
    // review of a row the first review closed reports "already reviewed", not "not reviewable".
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM programme_tranche_review
             WHERE tenant_id = $1 AND tranche_id = $2
               AND NOT (decision = 'pause' AND $3 = 'in_review')
        )",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(&reviewed.state)
    .fetch_one(&mut *conn)
    .await?;
    if already {
        return Err(PlanStoreError::Illegal(PlanError::TrancheAlreadyReviewed));
    }
    // The first tranche starts `planned`: its review is what closes it and authorises the next
    // one, so `planned` is reviewable. A terminal row with no review is not — reviewing it is a
    // second decision over a tranche that is no longer open.
    if matches!(reviewed.state.as_str(), "closed" | "retired") {
        return Err(PlanStoreError::Illegal(PlanError::TrancheNotReviewable {
            state: reviewed.state.clone(),
        }));
    }
    // A `continue` authorises the next tranche in the same write: the review's outcome is what
    // authorises it, and the start gate reads the state (and the review that set it), never a
    // second document. The next row is locked and checked BEFORE the review is inserted, so a
    // `continue` with nothing to authorise leaves no review row behind.
    let next = if decision == "continue" {
        let sql = format!(
            "SELECT {TRANCHE_COLUMNS} FROM programme_tranche
              WHERE tenant_id = $1 AND programme_id = $2 AND number = $3 FOR UPDATE"
        );
        let row: Option<TrancheRecord> = sqlx::query_as(AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(reviewed.programme_id)
            .bind(reviewed.number + 1)
            .fetch_optional(&mut *conn)
            .await?;
        match row {
            None => return Err(PlanStoreError::Illegal(PlanError::NextTrancheMissing)),
            Some(row) if row.state != "planned" => {
                return Err(PlanStoreError::Illegal(PlanError::NextTrancheNotPlanned {
                    state: row.state,
                }));
            }
            Some(row) => Some(row),
        }
    } else {
        None
    };
    ensure_parent_tx(&mut *conn, tenant, Parent::AppUser, reviewed_by)
        .await
        .map_err(PlanStoreError::Parent)?;
    let sql = format!(
        "INSERT INTO programme_tranche_review (tenant_id, tranche_id, decision, evidence, reviewed_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {TRANCHE_REVIEW_COLUMNS}"
    );
    let review: TrancheReviewRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(decision)
        .bind(evidence)
        .bind(reviewed_by)
        .fetch_one(&mut *conn)
        .await?;
    if let Some(next) = next {
        let updated = sqlx::query(
            "UPDATE programme_tranche
                SET state = 'authorised', authorised_at = now(),
                    authorisation_review_id = $3, version = version + 1, updated_at = now()
              WHERE tenant_id = $1 AND id = $2 AND state = 'planned'",
        )
        .bind(tenant.as_uuid())
        .bind(next.id)
        .bind(review.id)
        .execute(&mut *conn)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(PlanStoreError::Illegal(PlanError::NextTrancheNotPlanned {
                state: "changed".to_owned(),
            }));
        }
    }
    let sql = format!(
        "UPDATE programme_tranche SET state = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {TRANCHE_COLUMNS}"
    );
    let row: Option<TrancheRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(state)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::NotFound { what: "tranche" })
}

/// Authorise the tranche after `review_id`'s `continue` decision.
///
/// The normal path performs this in the same write as the review; this is the **repair** path for
/// a review whose authorisation never landed. It links the existing review and never creates one,
/// and a repeat call returns the same link without writing.
pub async fn authorise_tranche_from_review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    review_id: Uuid,
) -> Result<TrancheRecord, PlanStoreError> {
    let sql = format!(
        "SELECT {TRANCHE_REVIEW_COLUMNS} FROM programme_tranche_review
          WHERE tenant_id = $1 AND id = $2"
    );
    let review: Option<TrancheReviewRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(review_id)
        .fetch_optional(&mut *conn)
        .await?;
    let review = review.ok_or(PlanStoreError::NotFound {
        what: "tranche review",
    })?;
    if review.decision != "continue" {
        return Err(PlanStoreError::Illegal(PlanError::TrancheReviewNotContinue));
    }
    let reviewed = get_tranche_tx(&mut *conn, tenant, review.tranche_id)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "tranche" })?;
    let number = reviewed.number + 1;
    let next_sql = format!(
        "SELECT {TRANCHE_COLUMNS} FROM programme_tranche
          WHERE tenant_id = $1 AND programme_id = $2 AND number = $3"
    );
    let next: Option<TrancheRecord> = sqlx::query_as(AssertSqlSafe(next_sql))
        .bind(tenant.as_uuid())
        .bind(reviewed.programme_id)
        .bind(number)
        .fetch_optional(&mut *conn)
        .await?;
    let next = next.ok_or(PlanStoreError::NotFound {
        what: "next tranche",
    })?;
    if next.state == "authorised" && next.authorisation_review_id == Some(review.id) {
        return Ok(next);
    }
    if next.state != "planned" {
        return Err(PlanStoreError::Conflict {
            what: "next tranche",
            why: "the next tranche is not planned and was not authorised by this review",
        });
    }
    let sql = format!(
        "UPDATE programme_tranche
            SET state = 'authorised', authorised_at = now(),
                authorisation_review_id = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND programme_id = $2 AND number = $3 AND state = 'planned'
         RETURNING {TRANCHE_COLUMNS}"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(reviewed.programme_id)
        .bind(number)
        .bind(review.id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(PlanStoreError::Conflict {
            what: "next tranche",
            why: "the tranche changed between the read and the write",
        })
}

/// The programme tranche that governs a project's start: its number and state, when the
/// project belongs to a programme with tranche rows.
///
/// The project's `meta.tranche` names its tranche; a project that declares none is governed by
/// the first delivery tranche (number 1).
pub async fn project_tranche_state_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Option<(i32, String)>, PlanStoreError> {
    let project: Option<(Option<Uuid>, Value)> =
        sqlx::query_as("SELECT programme_id, meta FROM project WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(project_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (programme, meta) = project.ok_or(PlanStoreError::NotFound { what: "project" })?;
    let Some(programme) = programme else {
        return Ok(None);
    };
    let number = meta
        .get("tranche")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(1);
    let state: Option<String> = sqlx::query_scalar(
        "SELECT state FROM programme_tranche
          WHERE tenant_id = $1 AND programme_id = $2 AND number = $3",
    )
    .bind(tenant.as_uuid())
    .bind(programme)
    .bind(number)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(state.map(|state| (number, state)))
}

/// A recorded tranche review.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TrancheReviewRecord {
    /// The row id.
    pub id: Uuid,
    /// The reviewed tranche.
    pub tranche_id: Uuid,
    /// `continue | pause | stop`.
    pub decision: String,
    /// The evidence the decision was taken on.
    pub evidence: Value,
    /// The acting user.
    pub reviewed_by: Uuid,
    /// When the review was recorded.
    pub reviewed_at: DateTime<Utc>,
    /// The If-Match version.
    pub version: i64,
}

const TRANCHE_REVIEW_COLUMNS: &str =
    "id, tranche_id, decision, evidence, reviewed_by, reviewed_at, version";

/// List a programme's tranche reviews, newest first.
pub async fn list_tranche_reviews_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<TrancheReviewRecord>, PlanStoreError> {
    let sql = "SELECT r.id, r.tranche_id, r.decision, r.evidence, r.reviewed_by, r.reviewed_at,
                      r.version
                 FROM programme_tranche_review r
                 JOIN programme_tranche t ON t.id = r.tranche_id AND t.tenant_id = r.tenant_id
                WHERE r.tenant_id = $1 AND t.programme_id = $2
                ORDER BY r.reviewed_at DESC, r.id DESC";
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// The editable fields of a tranche. `number` and `state` are not here: the number is the
/// programme's order and the state is the review's outcome.
#[derive(Clone, Debug, Default)]
pub struct TranchePatch {
    /// Entry criteria.
    pub entry_criteria: Option<Value>,
    /// Exit criteria.
    pub exit_criteria: Option<Value>,
    /// The baseline it is managed against.
    pub baseline_ref: Option<Uuid>,
    /// The review date.
    pub review_at: Option<DateTime<Utc>>,
    /// The approved change request that authorises a baseline change, if one is being made.
    pub change_ref: Option<Uuid>,
}

/// Edit a tranche's plan fields; the state changes only through [`review_tranche_tx`].
pub async fn update_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    patch: &TranchePatch,
) -> Result<TrancheRecord, PlanStoreError> {
    let current = get_tranche_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "tranche" })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale { what: "tranche" });
    }
    if let Some(baseline_ref) = patch.baseline_ref {
        // A cited baseline is a citation only when it is this programme's, approved, and one
        // of the three MSP programme-baseline kinds. The review resolver applies the same rule
        // to the review's `baselines` evidence; this is the tranche row's own pointer.
        let row: Option<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT programme_id, kind, approved_by FROM programme_baseline
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(baseline_ref)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((programme_id, kind, approved_by)) = row else {
            return Err(PlanStoreError::NotFound {
                what: "programme baseline",
            });
        };
        if programme_id != current.programme_id {
            return Err(PlanStoreError::NotFound {
                what: "programme baseline",
            });
        }
        if approved_by.is_none()
            || !matches!(kind.as_str(), "management" | "governance" | "boundary")
        {
            return Err(PlanStoreError::Illegal(
                PlanError::TrancheBaselineNotApproved { kind },
            ));
        }
        // A re-baseline is a change, never an edit: the new baseline cites an approved change
        // request of a project in the same programme (SPEC §8.3's tranche rule).
        if current.baseline_ref != Some(baseline_ref) {
            let change_ref = patch
                .change_ref
                .ok_or(PlanStoreError::Illegal(PlanError::RebaselineRequiresChange))?;
            let approved: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1
                      FROM project_register_item r
                      JOIN project p ON p.tenant_id = r.tenant_id AND p.id = r.project_id
                     WHERE r.tenant_id = $1 AND r.id = $2 AND r.kind = 'change'
                       AND r.status = 'approved' AND p.programme_id = $3
                 )",
            )
            .bind(tenant.as_uuid())
            .bind(change_ref)
            .bind(current.programme_id)
            .fetch_one(&mut *conn)
            .await?;
            if !approved {
                return Err(PlanStoreError::Illegal(PlanError::ChangeNotApproved));
            }
        }
    }
    let sql = format!(
        "UPDATE programme_tranche SET
            entry_criteria = coalesce($4, entry_criteria),
            exit_criteria = coalesce($5, exit_criteria),
            baseline_ref = coalesce($6, baseline_ref),
            review_at = coalesce($7, review_at),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {TRANCHE_COLUMNS}"
    );
    let row: Option<TrancheRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(&patch.entry_criteria)
        .bind(&patch.exit_criteria)
        .bind(patch.baseline_ref)
        .bind(patch.review_at)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale { what: "tranche" })
}

/// Remove a tranche. A recorded review is history: a reviewed tranche is refused.
pub async fn delete_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let current = get_tranche_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "tranche" })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale { what: "tranche" });
    }
    let reviewed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM programme_tranche_review WHERE tenant_id = $1 AND tranche_id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if reviewed {
        return Err(PlanStoreError::Conflict {
            what: "tranche",
            why: "it carries a recorded review",
        });
    }
    let changed = sqlx::query(
        "DELETE FROM programme_tranche WHERE tenant_id = $1 AND id = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale { what: "tranche" });
    }
    Ok(())
}

pub async fn retire_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    reason: &str,
) -> Result<TrancheRecord, PlanStoreError> {
    let current = get_tranche_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "tranche" })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale { what: "tranche" });
    }
    if current.state == "retired" {
        return Err(PlanStoreError::Illegal(PlanError::TrancheRetired));
    }
    if current.state == "in_review" {
        return Err(PlanStoreError::Illegal(PlanError::TrancheUnderReview));
    }
    let sql = format!(
        "UPDATE programme_tranche
            SET state = 'retired', retired_reason = $4, version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {TRANCHE_COLUMNS}"
    );
    let row: Option<TrancheRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(reason)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale { what: "tranche" })
}

/// Resume a paused tranche under the cited `tranche.resume` gate (v1-tr2-1).
///
/// A `pause` review maps the tranche to `paused`, a tranche is reviewed only once, and the
/// start/close gate accepts only `authorised | in_review | closed`: without this path a paused
/// tranche froze every project in it and no action could move it back. The gate is the only
/// authorisation — approved, opened for `tranche.resume`, naming this tranche in `meta.target`
/// (the PI-plan gate's identity pattern), and carrying at least one human `approve` decision.
/// A machine decision is not a resume authorisation.
///
/// `programme_tranche` has no `meta`/evidence column (0044, 0052, 0056), so the resume record is
/// merged into the cited gate's own `meta` under `resume` — the row that authorises the move is
/// the row that records it, and the action's outbox event names the tranche.
pub async fn resume_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    tranche_id: Uuid,
    approval_id: Uuid,
    note: Option<&str>,
) -> Result<TrancheRecord, PlanStoreError> {
    let sql = format!(
        "SELECT {TRANCHE_COLUMNS} FROM programme_tranche
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let current: TrancheRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(tranche_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(PlanStoreError::NotFound { what: "tranche" })?;
    if current.state != "paused" {
        return Err(PlanStoreError::Illegal(PlanError::TrancheNotPaused {
            state: current.state,
        }));
    }
    let authorised: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM approval a
             WHERE a.tenant_id = $1 AND a.id = $2
               AND a.status = 'approved' AND a.gate = 'tranche.resume'
               AND a.meta->>'target' = $3
               AND EXISTS (
                   SELECT 1 FROM approval_decision d
                    WHERE d.tenant_id = a.tenant_id AND d.approval_id = a.id
                      AND d.decision = 'approve'
                      AND (d.decided_by_kind = 'human'
                           OR (d.decided_by_kind = 'rule'
                               AND d.via->>'delegation_ref' IS NOT NULL
                               AND EXISTS (
                                   SELECT 1 FROM document doc
                                    WHERE doc.tenant_id = a.tenant_id
                                      AND doc.id::text = d.via->>'delegation_ref'
                                      AND doc.status = 'active'
                                      AND doc.published_at IS NOT NULL
                                      AND doc.category = 'decision')))))",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(tranche_id.to_string())
    .fetch_one(&mut *conn)
    .await?;
    if !authorised {
        return Err(PlanStoreError::Illegal(
            PlanError::TrancheResumeApprovalMissing { approval_id },
        ));
    }
    sqlx::query(
        "UPDATE approval
            SET meta = meta || jsonb_build_object(
                    'resume', jsonb_build_object(
                        'resumed_at', to_jsonb(now()),
                        'resume_approval_id', $3::text,
                        'tranche_id', $4::text,
                        'note', $5::text)),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(approval_id.to_string())
    .bind(tranche_id.to_string())
    .bind(note)
    .execute(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE programme_tranche
            SET state = 'in_review', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = 'paused'
          RETURNING {TRANCHE_COLUMNS}"
    );
    let row: Option<TrancheRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(tranche_id)
        .fetch_optional(&mut *conn)
        .await?;
    // The row was locked above, so this is unreachable while the lock is held; a changed row
    // is still refused rather than reported as a successful resume.
    row.ok_or(PlanStoreError::Illegal(PlanError::TrancheNotPaused {
        state: "changed".to_owned(),
    }))
}

// ── benefit_profile / benefit_link ─────────────────────────────────────────────────────

/// A benefit profile.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BenefitRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme.
    pub programme_id: Uuid,
    /// The project, if any.
    pub project_id: Option<Uuid>,
    /// The register row that indexes it.
    pub register_item_id: Option<Uuid>,
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
    pub owner_job_id: Option<Uuid>,
    /// The BCM job.
    pub benefit_owner_job_id: Option<Uuid>,
    /// The operational area.
    pub operational_area: String,
    /// The status.
    pub status: String,
    /// Why the profile was retired; empty unless `status='retired'`.
    pub retired_reason: String,
    /// The approved change request that last advanced the profile's measurement.
    pub change_ref: Option<Uuid>,
    /// The If-Match version.
    pub version: i64,
}

const BENEFIT_COLUMNS: &str = "id, programme_id, project_id, register_item_id, reference, \
    description, category, objectives, observable_outcome, kpis, baseline, target, \
    measurement_method, financial_value, cost, capabilities, preconditions, risks, owner_job_id, \
    benefit_owner_job_id, operational_area, status, retired_reason, change_ref, version";

/// A new benefit profile.
#[derive(Clone, Debug, Default)]
pub struct NewBenefitProfile {
    /// The programme.
    pub programme_id: Uuid,
    /// The project.
    pub project_id: Option<Uuid>,
    /// The register row.
    pub register_item_id: Option<Uuid>,
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
    /// The baseline.
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
    pub owner_job_id: Option<Uuid>,
    /// The BCM job.
    pub benefit_owner_job_id: Option<Uuid>,
    /// The operational area.
    pub operational_area: String,
    /// The status.
    pub status: String,
}

impl NewBenefitProfile {
    /// The fields the completeness rule reads.
    fn fields(&self) -> BenefitProfileFields {
        BenefitProfileFields {
            baseline: self.baseline.clone(),
            target: self.target.clone(),
            measurement_method: self.measurement_method.clone(),
            benefit_owner_job_id: self.benefit_owner_job_id,
            capabilities: self.capabilities.clone(),
            operational_area: self.operational_area.clone(),
        }
    }
}

/// Insert a benefit profile, refusing an incomplete measurement record (D-T1-9).
pub async fn insert_benefit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewBenefitProfile,
) -> Result<BenefitRecord, PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Programme, new.programme_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Project, new.project_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    for owner in [new.owner_job_id, new.benefit_owner_job_id]
        .into_iter()
        .flatten()
    {
        ensure_job_tx(&mut *conn, tenant, owner).await?;
    }
    let missing = benefit_profile_missing(&new.fields());
    if !missing.is_empty() {
        return Err(PlanStoreError::Illegal(
            PlanError::BenefitProfileIncomplete { missing },
        ));
    }
    if new.status == "realized" {
        // §8.3: the realization record is what proves an improvement. A row that does not exist
        // yet cannot carry one, so a profile is never born realized.
        return Err(PlanStoreError::Illegal(PlanError::BenefitBaselineMissing));
    }
    let sql = format!(
        "INSERT INTO benefit_profile
            (tenant_id, programme_id, project_id, register_item_id, reference, description,
             category, objectives, observable_outcome, kpis, baseline, target,
             measurement_method, financial_value, cost, capabilities, preconditions, risks,
             owner_job_id, benefit_owner_job_id, operational_area, status)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)
         RETURNING {BENEFIT_COLUMNS}"
    );
    let row: BenefitRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.programme_id)
        .bind(new.project_id)
        .bind(new.register_item_id)
        .bind(&new.reference)
        .bind(&new.description)
        .bind(&new.category)
        .bind(&new.objectives)
        .bind(&new.observable_outcome)
        .bind(&new.kpis)
        .bind(&new.baseline)
        .bind(&new.target)
        .bind(&new.measurement_method)
        .bind(&new.financial_value)
        .bind(&new.cost)
        .bind(&new.capabilities)
        .bind(&new.preconditions)
        .bind(&new.risks)
        .bind(new.owner_job_id)
        .bind(new.benefit_owner_job_id)
        .bind(&new.operational_area)
        .bind(&new.status)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one benefit profile.
pub async fn get_benefit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<BenefitRecord>, PlanStoreError> {
    let sql =
        format!("SELECT {BENEFIT_COLUMNS} FROM benefit_profile WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a programme's benefit profiles. A retired profile leaves the default list and map
/// (#303) but remains readable by id and is returned again with `include_retired`.
pub async fn list_benefits_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    include_retired: bool,
) -> Result<Vec<BenefitRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {BENEFIT_COLUMNS} FROM benefit_profile
          WHERE tenant_id = $1 AND programme_id = $2 AND ($3 OR status <> 'retired')
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(include_retired)
        .fetch_all(&mut *conn)
        .await?)
}

/// The editable fields of a benefit profile. The programme is the row's parent, not a field.
#[derive(Clone, Debug, Default)]
pub struct BenefitPatch {
    /// The project.
    pub project_id: Option<Uuid>,
    /// The register row.
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
    /// The status.
    pub status: Option<String>,
}

impl BenefitPatch {
    /// True when the patch supplies no field at all.
    pub fn is_empty(&self) -> bool {
        self.project_id.is_none()
            && self.register_item_id.is_none()
            && self.reference.is_none()
            && self.description.is_none()
            && self.category.is_none()
            && self.objectives.is_none()
            && self.observable_outcome.is_none()
            && self.kpis.is_none()
            && self.baseline.is_none()
            && self.target.is_none()
            && self.measurement_method.is_none()
            && self.financial_value.is_none()
            && self.cost.is_none()
            && self.capabilities.is_none()
            && self.preconditions.is_none()
            && self.risks.is_none()
            && self.owner_job_id.is_none()
            && self.benefit_owner_job_id.is_none()
            && self.operational_area.is_none()
            && self.status.is_none()
    }

    /// True when the patch supplies one of the four measurement fields §8.3 makes mandatory.
    /// Those fields are what an approval protects, so an approved profile may change them only
    /// through an approved change request (#303 P1).
    fn changes_measurement(&self) -> bool {
        self.baseline.is_some()
            || self.target.is_some()
            || self.measurement_method.is_some()
            || self.benefit_owner_job_id.is_some()
    }
}

/// True when an approved `benefit_profile` gate names this profile. The issue's default
/// (`#303` Q1) reads approval as the existing primitive; the profile's own status is the other
/// record of the same decision, and either one protects the measurement fields.
pub async fn benefit_is_approved_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM approval
             WHERE tenant_id = $1 AND gate = 'benefit_profile' AND status = 'approved'
               AND meta->>'benefit_id' = $2::text
        )",
    )
    .bind(tenant.as_uuid())
    .bind(benefit_id)
    .fetch_one(&mut *conn)
    .await
}

/// True when `change_id` names a `kind='change'` register row of this project whose `change`
/// gate is approved. The approval is the second principal's decision, not the row's status.
pub async fn change_is_approved_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    change_id: Uuid,
    project_id: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM project_register_item r
              JOIN approval a
                ON a.tenant_id = r.tenant_id
               AND a.gate = 'change'
               AND a.status = 'approved'
               AND a.meta->>'change_id' = r.id::text
             WHERE r.tenant_id = $1 AND r.id = $2 AND r.kind = 'change'
               AND ($3::uuid IS NULL OR r.project_id = $3)
        )",
    )
    .bind(tenant.as_uuid())
    .bind(change_id)
    .bind(project_id)
    .fetch_one(&mut *conn)
    .await
}

/// True when `change_id` is the change request that authorises an edit under `baseline_id`.
///
/// A change names the baseline it changes in the register's own `baseline_ref` column (the FK
/// `project_register_item_baseline_fk`; every `kind='change'` row carries one — `validate_change`
/// refuses a change without it). The change must belong to the same project as the baseline, its
/// `meta.impact` block must be present (the `validate_change` rule: a §5.5 object carrying at
/// least one of scope/schedule/cost/risk with a NON-NULL value — the 0065 CHECK is the same
/// rule), and its `change` gate must be approved — the row's own
/// `status` is never an approval (`change_is_approved_tx`'s rule).
///
/// The reciprocal `meta.baseline_id` that `link_change_to_baseline_tx` writes names the baseline
/// the change *produced*, so it is not the authorisation for edits under the new baseline; an
/// edit after a re-baseline cites a change whose `baseline_ref` is that new baseline.
pub async fn approved_change_for_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    change_id: Uuid,
    baseline_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM project_register_item r
              JOIN project_baseline b
                ON b.tenant_id = r.tenant_id
               AND b.id = r.baseline_ref
               AND b.project_id = r.project_id
              JOIN approval a
                ON a.tenant_id = r.tenant_id
               AND a.gate = 'change'
               AND a.status = 'approved'
               AND a.meta->>'change_id' = r.id::text
             WHERE r.tenant_id = $1 AND r.id = $2 AND r.kind = 'change'
               AND b.id = $3
               AND r.meta->'impact' IS NOT NULL
               AND r.meta->'impact' <> 'null'::jsonb
        )",
    )
    .bind(tenant.as_uuid())
    .bind(change_id)
    .bind(baseline_id)
    .fetch_one(&mut *conn)
    .await
}

/// Append the profile to the change row's links, so the citation reads from both sides. The
/// entry lives under the declared `requirements` array (§5.5); each entry keeps its `kind`, and
/// the `@>` guard makes a repeated update idempotent. The row's version moves with it.
async fn cite_benefit_on_change_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    change_id: Uuid,
    benefit_id: Uuid,
) -> Result<(), sqlx::Error> {
    let link = serde_json::json!([{ "kind": "benefit", "id": benefit_id.to_string() }]);
    sqlx::query(
        "UPDATE project_register_item
            SET links = jsonb_set(
                    links,
                    '{requirements}',
                    coalesce(links->'requirements', '[]'::jsonb) || $3::jsonb,
                    true),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
            AND NOT (coalesce(links->'requirements', '[]'::jsonb) @> $3::jsonb)",
    )
    .bind(tenant.as_uuid())
    .bind(change_id)
    .bind(link)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Edit a benefit profile, re-validating the merged measurement record (D-T1-9); an approved
/// profile's measurement fields additionally require an approved change request (#303 P1).
pub async fn update_benefit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    patch: &BenefitPatch,
    change_id: Option<Uuid>,
) -> Result<BenefitRecord, PlanStoreError> {
    let current =
        get_benefit_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "benefit profile",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "benefit profile",
        });
    }
    if current.status == "retired" {
        return Err(PlanStoreError::Illegal(PlanError::BenefitRetired));
    }
    if patch.status.as_deref() == Some("realized") {
        // §8.3: a benefit whose baseline was never measured cannot claim an improvement, and a
        // realization is proved by readings, not timestamps. The claim must cite a measured
        // baseline and a measured post reading taken after the transition was enacted, and each
        // reading must carry the evidence behind it.
        let baseline = latest_measurement_tx(&mut *conn, tenant, id, "baseline")
            .await?
            .ok_or(PlanStoreError::Illegal(PlanError::BenefitBaselineMissing))?;
        validate_benefit_reading(&baseline.kind, baseline.actual_value, &baseline.unit)
            .map_err(PlanStoreError::Illegal)?;
        if !evidence_present(&baseline.evidence) {
            return Err(PlanStoreError::Illegal(
                PlanError::BenefitMeasurementEvidenceMissing,
            ));
        }
        let enacted_at = get_transition_tx(&mut *conn, tenant, id)
            .await?
            .and_then(|transition| transition.enacted_at);
        let post = match enacted_at {
            Some(enacted_at) => {
                latest_post_measurement_after_tx(&mut *conn, tenant, id, enacted_at).await?
            }
            None => None,
        }
        .ok_or(PlanStoreError::Illegal(
            PlanError::BenefitPostMeasurementMissing,
        ))?;
        validate_benefit_reading(&post.kind, post.actual_value, &post.unit)
            .map_err(PlanStoreError::Illegal)?;
        if !evidence_present(&post.evidence) {
            return Err(PlanStoreError::Illegal(
                PlanError::BenefitMeasurementEvidenceMissing,
            ));
        }
    }
    let approved =
        current.status != "draft" || benefit_is_approved_tx(&mut *conn, tenant, id).await?;
    if approved && patch.changes_measurement() && change_id.is_none() {
        return Err(PlanStoreError::Illegal(PlanError::BenefitChangeRequired));
    }
    if let Some(change) = change_id
        && !change_is_approved_tx(&mut *conn, tenant, change, current.project_id).await?
    {
        return Err(PlanStoreError::Illegal(PlanError::ChangeNotApproved));
    }
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Project, patch.project_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    for owner in [patch.owner_job_id, patch.benefit_owner_job_id]
        .into_iter()
        .flatten()
    {
        ensure_job_tx(&mut *conn, tenant, owner).await?;
    }
    let fields = BenefitProfileFields {
        baseline: patch.baseline.clone().or_else(|| current.baseline.clone()),
        target: patch.target.clone().or_else(|| current.target.clone()),
        measurement_method: patch
            .measurement_method
            .clone()
            .unwrap_or_else(|| current.measurement_method.clone()),
        benefit_owner_job_id: patch.benefit_owner_job_id.or(current.benefit_owner_job_id),
        capabilities: patch
            .capabilities
            .clone()
            .unwrap_or_else(|| current.capabilities.clone()),
        operational_area: patch
            .operational_area
            .clone()
            .unwrap_or_else(|| current.operational_area.clone()),
    };
    let missing = benefit_profile_missing(&fields);
    if !missing.is_empty() {
        return Err(PlanStoreError::Illegal(
            PlanError::BenefitProfileIncomplete { missing },
        ));
    }
    let sql = format!(
        "UPDATE benefit_profile SET
            project_id = coalesce($4, project_id),
            register_item_id = coalesce($5, register_item_id),
            reference = coalesce($6, reference),
            description = coalesce($7, description),
            category = coalesce($8, category),
            objectives = coalesce($9, objectives),
            observable_outcome = coalesce($10, observable_outcome),
            kpis = coalesce($11, kpis),
            baseline = coalesce($12, baseline),
            target = coalesce($13, target),
            measurement_method = coalesce($14, measurement_method),
            financial_value = coalesce($15, financial_value),
            cost = coalesce($16, cost),
            capabilities = coalesce($17, capabilities),
            preconditions = coalesce($18, preconditions),
            risks = coalesce($19, risks),
            owner_job_id = coalesce($20, owner_job_id),
            benefit_owner_job_id = coalesce($21, benefit_owner_job_id),
            operational_area = coalesce($22, operational_area),
            status = coalesce($23, status),
            change_ref = coalesce($24, change_ref),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {BENEFIT_COLUMNS}"
    );
    let row: Option<BenefitRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(patch.project_id)
        .bind(patch.register_item_id)
        .bind(&patch.reference)
        .bind(&patch.description)
        .bind(&patch.category)
        .bind(&patch.objectives)
        .bind(&patch.observable_outcome)
        .bind(&patch.kpis)
        .bind(&patch.baseline)
        .bind(&patch.target)
        .bind(&patch.measurement_method)
        .bind(&patch.financial_value)
        .bind(&patch.cost)
        .bind(&patch.capabilities)
        .bind(&patch.preconditions)
        .bind(&patch.risks)
        .bind(patch.owner_job_id)
        .bind(patch.benefit_owner_job_id)
        .bind(&patch.operational_area)
        .bind(&patch.status)
        .bind(change_id)
        .fetch_optional(&mut *conn)
        .await?;
    let record = row.ok_or(PlanStoreError::Stale {
        what: "benefit profile",
    })?;
    if let Some(change) = change_id {
        cite_benefit_on_change_tx(&mut *conn, tenant, change, id).await?;
    }
    Ok(record)
}

/// Remove a benefit profile; its links and transitions cascade with it.
pub async fn delete_benefit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let current =
        get_benefit_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "benefit profile",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "benefit profile",
        });
    }
    let changed = sqlx::query(
        "DELETE FROM benefit_profile WHERE tenant_id = $1 AND id = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale {
            what: "benefit profile",
        });
    }
    Ok(())
}

/// Retire a benefit profile with a reason (#303 P2). A profile in an open transition is refused
/// until that transition retires, and the row survives as the readable history.
pub async fn retire_benefit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    reason: &str,
) -> Result<BenefitRecord, PlanStoreError> {
    let current =
        get_benefit_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "benefit profile",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "benefit profile",
        });
    }
    if current.status == "retired" {
        return Err(PlanStoreError::Illegal(PlanError::BenefitRetired));
    }
    let open: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM benefit_transition
             WHERE tenant_id = $1 AND benefit_id = $2 AND retired_at IS NULL
        )",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if open {
        return Err(PlanStoreError::Illegal(PlanError::BenefitInTransition));
    }
    let sql = format!(
        "UPDATE benefit_profile
            SET status = 'retired', retired_reason = $4, version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {BENEFIT_COLUMNS}"
    );
    let row: Option<BenefitRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(reason)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale {
        what: "benefit profile",
    })
}

/// A benefits-map edge.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BenefitLinkRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme.
    pub programme_id: Uuid,
    /// The output.
    pub output_ref: String,
    /// The capability.
    pub capability_ref: String,
    /// The operational outcome.
    pub operational_outcome: String,
    /// The benefit.
    pub benefit_id: Uuid,
    /// The strategic objective.
    pub strategic_objective: String,
    /// `benefit | disbenefit`.
    pub kind: String,
    /// The If-Match version.
    pub version: i64,
}

/// A new benefits-map edge.
#[derive(Clone, Debug, Default)]
pub struct NewBenefitLink {
    /// The programme.
    pub programme_id: Uuid,
    /// The output.
    pub output_ref: String,
    /// The capability.
    pub capability_ref: String,
    /// The operational outcome.
    pub operational_outcome: String,
    /// The benefit.
    pub benefit_id: Uuid,
    /// The strategic objective.
    pub strategic_objective: String,
    /// `benefit | disbenefit`.
    pub kind: String,
}

impl NewBenefitLink {
    fn fields(&self) -> BenefitMapFields {
        BenefitMapFields {
            output_ref: self.output_ref.clone(),
            capability_ref: self.capability_ref.clone(),
            operational_outcome: self.operational_outcome.clone(),
            strategic_objective: self.strategic_objective.clone(),
        }
    }
}

/// Insert a benefits-map edge, refusing one that does not name all five levels.
pub async fn insert_benefit_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewBenefitLink,
) -> Result<BenefitLinkRecord, PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Programme, new.programme_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    let missing = benefit_map_missing(&new.fields());
    if !missing.is_empty() {
        return Err(PlanStoreError::Illegal(PlanError::BenefitMapIncomplete {
            missing,
        }));
    }
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM benefit_profile
                         WHERE tenant_id = $1 AND id = $2 AND programme_id = $3)",
    )
    .bind(tenant.as_uuid())
    .bind(new.benefit_id)
    .bind(new.programme_id)
    .fetch_one(&mut *conn)
    .await?;
    if !owned {
        return Err(PlanStoreError::NotFound {
            what: "benefit profile",
        });
    }
    // The output is a WBS node, not a label: `record_dependency_slip_tx` matches `output_ref`
    // to `wbs_node.id`, so an edge naming anything else is a row that can never participate.
    // The profile check above pins the profile's programme to `new.programme_id`, so resolving
    // the node to this programme is resolving it to the profile's programme. The id is stored
    // canonically (lowercase, hyphenated) so the slip's text join is exact.
    let output_node = Uuid::parse_str(new.output_ref.trim()).map_err(|_| {
        PlanStoreError::Illegal(PlanError::BenefitOutputUnresolved {
            output_ref: new.output_ref.clone(),
        })
    })?;
    let output_owned: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM wbs_node w
              JOIN project p ON p.tenant_id = w.tenant_id AND p.id = w.project_id
             WHERE w.tenant_id = $1 AND w.id = $2 AND p.programme_id = $3
        )",
    )
    .bind(tenant.as_uuid())
    .bind(output_node)
    .bind(new.programme_id)
    .fetch_one(&mut *conn)
    .await?;
    if !output_owned {
        return Err(PlanStoreError::Illegal(
            PlanError::BenefitOutputUnresolved {
                output_ref: new.output_ref.clone(),
            },
        ));
    }
    let output_ref = output_node.to_string();
    let sql = "INSERT INTO benefit_link
                 (tenant_id, programme_id, output_ref, capability_ref, operational_outcome,
                  benefit_id, strategic_objective, kind)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               RETURNING id, programme_id, output_ref, capability_ref, operational_outcome,
                         benefit_id, strategic_objective, kind, version";
    let row: BenefitLinkRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.programme_id)
        .bind(&output_ref)
        .bind(&new.capability_ref)
        .bind(&new.operational_outcome)
        .bind(new.benefit_id)
        .bind(&new.strategic_objective)
        .bind(&new.kind)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one benefits-map edge.
pub async fn get_benefit_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<BenefitLinkRecord>, PlanStoreError> {
    let sql = "SELECT id, programme_id, output_ref, capability_ref, operational_outcome,
                      benefit_id, strategic_objective, kind, version
                 FROM benefit_link WHERE tenant_id = $1 AND id = $2";
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a programme's benefits-map edges.
pub async fn list_benefit_links_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<BenefitLinkRecord>, PlanStoreError> {
    let sql = "SELECT id, programme_id, output_ref, capability_ref, operational_outcome,
                      benefit_id, strategic_objective, kind, version
                 FROM benefit_link
                WHERE tenant_id = $1 AND programme_id = $2
                ORDER BY created_at, id";
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Remove one benefits-map edge.
pub async fn delete_benefit_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM benefit_link WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    match version {
        None => {
            return Err(PlanStoreError::NotFound {
                what: "benefits-map edge",
            });
        }
        Some(version) if version != expected => {
            return Err(PlanStoreError::Stale {
                what: "benefits-map edge",
            });
        }
        Some(_) => {}
    }
    let changed =
        sqlx::query("DELETE FROM benefit_link WHERE tenant_id = $1 AND id = $2 AND version = $3")
            .bind(tenant.as_uuid())
            .bind(id)
            .bind(expected)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale {
            what: "benefits-map edge",
        });
    }
    Ok(())
}

// ── benefit_transition ──────────────────────────────────────────────────────────────────
// The realization record: baseline measured → readiness → enacted → post measured → legacy
// retired. One row per benefit; a correction updates it in place.

/// A benefit's realization record.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TransitionRecord {
    /// The row id.
    pub id: Uuid,
    /// The benefit.
    pub benefit_id: Uuid,
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

const TRANSITION_COLUMNS: &str = "id, benefit_id, baseline_measured_at, readiness, enacted_at, \
    post_measured_at, retired_at, evidence, version";

/// The editable fields of a realization record. `None` leaves the stored value in place.
#[derive(Clone, Debug, Default)]
pub struct TransitionPatch {
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

/// Read one benefit's realization record.
pub async fn get_transition_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
) -> Result<Option<TransitionRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {TRANSITION_COLUMNS} FROM benefit_transition
          WHERE tenant_id = $1 AND benefit_id = $2 ORDER BY created_at, id LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(benefit_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a programme's realization records.
pub async fn list_transitions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<TransitionRecord>, PlanStoreError> {
    let sql = "SELECT t.id, t.benefit_id, t.baseline_measured_at, t.readiness, t.enacted_at,
                      t.post_measured_at, t.retired_at, t.evidence, t.version
                 FROM benefit_transition t
                 JOIN benefit_profile b ON b.id = t.benefit_id AND b.tenant_id = t.tenant_id
                WHERE t.tenant_id = $1 AND b.programme_id = $2
                ORDER BY t.benefit_id, t.created_at, t.id";
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Create or correct a benefit's realization record; a post-transition measurement without a
/// measured baseline is refused (D-T1-9).
pub async fn upsert_transition_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
    expected: Option<i64>,
    patch: &TransitionPatch,
) -> Result<TransitionRecord, PlanStoreError> {
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM benefit_profile WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(benefit_id)
    .fetch_one(&mut *conn)
    .await?;
    if !owned {
        return Err(PlanStoreError::NotFound {
            what: "benefit profile",
        });
    }
    let current = get_transition_tx(&mut *conn, tenant, benefit_id).await?;
    if let Some(current) = &current
        && Some(current.version) != expected
    {
        return Err(PlanStoreError::Stale {
            what: "benefit transition",
        });
    }
    let baseline = patch
        .baseline_measured_at
        .or(current.as_ref().and_then(|row| row.baseline_measured_at));
    let post = patch
        .post_measured_at
        .or(current.as_ref().and_then(|row| row.post_measured_at));
    if post.is_some() && baseline.is_none() {
        return Err(PlanStoreError::Illegal(PlanError::BenefitBaselineMissing));
    }
    // A post-transition timestamp (or a legacy retirement) claims the post reading exists. The
    // reading is the `benefit_measurement(kind='post')` row taken after the enactment; without
    // it the transition would carry an assertion the evaluation cannot reproduce.
    let enacted = patch
        .enacted_at
        .or(current.as_ref().and_then(|row| row.enacted_at));
    if post.is_some() || patch.retired_at.is_some() {
        let measured = match enacted {
            Some(enacted_at) => {
                post_measurement_after_tx(&mut *conn, tenant, benefit_id, enacted_at).await?
            }
            None => false,
        };
        if !measured {
            return Err(PlanStoreError::Illegal(
                PlanError::BenefitPostMeasurementMissing,
            ));
        }
    }
    match current {
        Some(current) => {
            let sql = format!(
                "UPDATE benefit_transition SET
                    baseline_measured_at = coalesce($3, baseline_measured_at),
                    readiness = coalesce($4, readiness),
                    enacted_at = coalesce($5, enacted_at),
                    post_measured_at = coalesce($6, post_measured_at),
                    retired_at = coalesce($7, retired_at),
                    evidence = coalesce($8, evidence),
                    version = version + 1, updated_at = now()
                  WHERE tenant_id = $1 AND id = $2 AND version = $9
                  RETURNING {TRANSITION_COLUMNS}"
            );
            let row: Option<TransitionRecord> = sqlx::query_as(AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(current.id)
                .bind(patch.baseline_measured_at)
                .bind(&patch.readiness)
                .bind(patch.enacted_at)
                .bind(patch.post_measured_at)
                .bind(patch.retired_at)
                .bind(&patch.evidence)
                .bind(current.version)
                .fetch_optional(&mut *conn)
                .await?;
            row.ok_or(PlanStoreError::Stale {
                what: "benefit transition",
            })
        }
        None => {
            let sql = format!(
                "INSERT INTO benefit_transition
                    (tenant_id, benefit_id, baseline_measured_at, readiness, enacted_at,
                     post_measured_at, retired_at, evidence)
                 VALUES ($1, $2, $3, coalesce($4, '{{}}'::jsonb), $5, $6, $7,
                         coalesce($8, '[]'::jsonb))
                 RETURNING {TRANSITION_COLUMNS}"
            );
            let row: TransitionRecord = sqlx::query_as(AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(benefit_id)
                .bind(patch.baseline_measured_at)
                .bind(&patch.readiness)
                .bind(patch.enacted_at)
                .bind(patch.post_measured_at)
                .bind(patch.retired_at)
                .bind(&patch.evidence)
                .fetch_one(&mut *conn)
                .await?;
            Ok(row)
        }
    }
}

// ── benefit_measurement ────────────────────────────────────────────────────────────────
// §8.3's readings: the baseline measured while the old practice is observable, the interim
// monitoring readings, and the post-transition measurement the sponsor compares. A realization
// claim cites these rows, never a timestamp alone.

/// One measurement of a benefit.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BenefitMeasurementRecord {
    /// The row id.
    pub id: Uuid,
    /// The benefit measured.
    pub benefit_id: Uuid,
    /// The programme the benefit belongs to.
    pub programme_id: Uuid,
    /// The tranche the reading is attributed to, if any.
    pub tranche_id: Option<Uuid>,
    /// `baseline | interim | post`.
    pub kind: String,
    /// The target the reading is compared with.
    pub target_value: Option<f64>,
    /// The value actually observed.
    pub actual_value: Option<f64>,
    /// The unit of the values.
    pub unit: String,
    /// How the reading was taken.
    pub method: String,
    /// When the reading was taken.
    pub measured_at: DateTime<Utc>,
    /// The evidence behind the reading.
    pub evidence: Value,
}

const BENEFIT_MEASUREMENT_COLUMNS: &str = "id, benefit_id, programme_id, tranche_id, kind, \
    target_value::float8 AS target_value, actual_value::float8 AS actual_value, unit, method, \
    measured_at, evidence";

/// A new benefit measurement.
#[derive(Clone, Debug)]
pub struct NewBenefitMeasurement {
    /// The benefit measured.
    pub benefit_id: Uuid,
    /// The tranche the reading is attributed to, if any.
    pub tranche_id: Option<Uuid>,
    /// `baseline | interim | post`.
    pub kind: String,
    /// The target the reading is compared with.
    pub target_value: Option<f64>,
    /// The value actually observed.
    pub actual_value: Option<f64>,
    /// The unit of the values.
    pub unit: String,
    /// How the reading was taken.
    pub method: String,
    /// When the reading was taken.
    pub measured_at: DateTime<Utc>,
    /// The evidence behind the reading.
    pub evidence: Value,
}

/// Insert one measurement of a benefit. The programme is denormalised from the profile, so a
/// reading cannot be filed under a programme the benefit does not belong to.
pub async fn insert_benefit_measurement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewBenefitMeasurement,
) -> Result<BenefitMeasurementRecord, PlanStoreError> {
    // §8.3: a baseline or post reading is a number and a unit. Prose, a missing value, a blank
    // unit and a placeholder unit are refused here; the `benefit_measurement` CHECK is the
    // second lock.
    validate_benefit_reading(&new.kind, new.actual_value, &new.unit)
        .map_err(PlanStoreError::Illegal)?;
    let programme_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT programme_id FROM benefit_profile WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(new.benefit_id)
    .fetch_optional(&mut *conn)
    .await?;
    let programme_id = programme_id.ok_or(PlanStoreError::NotFound {
        what: "benefit profile",
    })?;
    let sql = format!(
        "INSERT INTO benefit_measurement
            (tenant_id, benefit_id, programme_id, tranche_id, kind, target_value, actual_value,
             unit, method, measured_at, evidence)
         VALUES ($1, $2, $3, $4, $5, $6::float8::numeric, $7::float8::numeric, $8, $9, $10,
                 $11)
         RETURNING {BENEFIT_MEASUREMENT_COLUMNS}"
    );
    let row: BenefitMeasurementRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.benefit_id)
        .bind(programme_id)
        .bind(new.tranche_id)
        .bind(&new.kind)
        .bind(new.target_value)
        .bind(new.actual_value)
        .bind(&new.unit)
        .bind(&new.method)
        .bind(new.measured_at)
        .bind(&new.evidence)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// True when a baseline carries the five measurement fields SPEC_V2 §8.3 requires: value, unit,
/// when it was taken, how, and the evidence behind it. A baseline missing any of them is a
/// declaration, not a measurement.
fn benefit_baseline_complete(baseline: Option<&Value>) -> bool {
    let Some(Value::Object(fields)) = baseline else {
        return false;
    };
    ["value", "unit", "measured_at", "method", "evidence"]
        .iter()
        .all(|key| match fields.get(*key) {
            None | Some(Value::Null) => false,
            Some(Value::String(text)) => !text.trim().is_empty(),
            Some(Value::Object(map)) => !map.is_empty(),
            Some(Value::Array(items)) => !items.is_empty(),
            Some(_) => true,
        })
}

/// The reading re-records the recorded baseline: same value and unit. A different value or unit
/// is a re-baseline, which an approved profile changes only through an approved change request.
fn benefit_reading_agrees(baseline: Option<&Value>, record: &BenefitMeasurementRecord) -> bool {
    let Some(Value::Object(fields)) = baseline else {
        return false;
    };
    let value = fields.get("value").and_then(Value::as_f64);
    let unit = fields.get("unit").and_then(Value::as_str).map(str::trim);
    value == record.actual_value && unit == Some(record.unit.trim())
}

/// The measured reading, merged over the declared fields a profile carries beside it (a
/// definition, a KPI list), so the measurement record keeps both.
fn benefit_baseline_merged(baseline: Option<&Value>, record: &BenefitMeasurementRecord) -> Value {
    let mut fields = match baseline {
        Some(Value::Object(existing)) => existing.clone(),
        _ => serde_json::Map::new(),
    };
    fields.insert("value".to_owned(), serde_json::json!(record.actual_value));
    fields.insert("unit".to_owned(), Value::String(record.unit.clone()));
    fields.insert(
        "measured_at".to_owned(),
        Value::String(record.measured_at.to_rfc3339()),
    );
    fields.insert("method".to_owned(), Value::String(record.method.clone()));
    fields.insert("evidence".to_owned(), record.evidence.clone());
    Value::Object(fields)
}

/// Apply a baseline reading to the profile it belongs to (SPEC_V2 §8.3: the benefit profile is
/// the measurement record).
///
/// The reading row and the profile are written in one transaction, so `benefits.get` and the
/// published snapshot carry the measurement rather than a declaration beside it. Completeness is
/// the change-control boundary: an incomplete baseline is completed by the measurement; a
/// complete one on an approved profile changes only through an approved change request.
pub async fn record_measured_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
    record: &BenefitMeasurementRecord,
) -> Result<(), PlanStoreError> {
    let current = get_benefit_tx(&mut *conn, tenant, benefit_id)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "benefit profile",
        })?;
    let approved =
        current.status != "draft" || benefit_is_approved_tx(&mut *conn, tenant, benefit_id).await?;
    if approved
        && benefit_baseline_complete(current.baseline.as_ref())
        && !benefit_reading_agrees(current.baseline.as_ref(), record)
    {
        return Err(PlanStoreError::Illegal(PlanError::BenefitChangeRequired));
    }
    let baseline = benefit_baseline_merged(current.baseline.as_ref(), record);
    let updated = sqlx::query(
        "UPDATE benefit_profile
            SET baseline = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(benefit_id)
    .bind(baseline)
    .execute(&mut *conn)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(PlanStoreError::NotFound {
            what: "benefit profile",
        });
    }
    Ok(())
}

/// List a benefit's measurements, newest reading first.
pub async fn list_benefit_measurements_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
) -> Result<Vec<BenefitMeasurementRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {BENEFIT_MEASUREMENT_COLUMNS} FROM benefit_measurement
          WHERE tenant_id = $1 AND benefit_id = $2
          ORDER BY measured_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(benefit_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// List a programme's measurements, newest reading first; `benefit_id` narrows the page to one
/// benefit when given.
pub async fn list_programme_benefit_measurements_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    benefit_id: Option<Uuid>,
) -> Result<Vec<BenefitMeasurementRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {BENEFIT_MEASUREMENT_COLUMNS} FROM benefit_measurement
          WHERE tenant_id = $1 AND programme_id = $2
            AND ($3::uuid IS NULL OR benefit_id = $3)
          ORDER BY measured_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(benefit_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// True when the benefit has a `post` measurement taken strictly after `enacted_at`.
pub async fn post_measurement_after_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
    enacted_at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM benefit_measurement
             WHERE tenant_id = $1 AND benefit_id = $2 AND kind = 'post'
               AND measured_at > $3
        )",
    )
    .bind(tenant.as_uuid())
    .bind(benefit_id)
    .bind(enacted_at)
    .fetch_one(&mut *conn)
    .await
}

/// The benefit's latest reading of one kind, when one exists.
pub async fn latest_measurement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
    kind: &str,
) -> Result<Option<BenefitMeasurementRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {BENEFIT_MEASUREMENT_COLUMNS} FROM benefit_measurement
          WHERE tenant_id = $1 AND benefit_id = $2 AND kind = $3
          ORDER BY measured_at DESC, id DESC LIMIT 1"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(benefit_id)
        .bind(kind)
        .fetch_optional(&mut *conn)
        .await
}

/// The benefit's latest `post` reading taken strictly after `enacted_at`, when one exists.
pub async fn latest_post_measurement_after_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
    enacted_at: DateTime<Utc>,
) -> Result<Option<BenefitMeasurementRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {BENEFIT_MEASUREMENT_COLUMNS} FROM benefit_measurement
          WHERE tenant_id = $1 AND benefit_id = $2 AND kind = 'post' AND measured_at > $3
          ORDER BY measured_at DESC, id DESC LIMIT 1"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(benefit_id)
        .bind(enacted_at)
        .fetch_optional(&mut *conn)
        .await
}

// ── programme_cadence ───────────────────────────────────────────────────────────────────
// T1 stores the cadence row; T10 materialises its cards. No scheduler is built here.

/// A cadence row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CadenceRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme.
    pub programme_id: Uuid,
    /// `pi | sprint | demo | retro | refinement`.
    pub kind: String,
    /// The period label.
    pub period: String,
    /// The state.
    pub state: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The forum type this cadence instance materialises; NULL until bound.
    pub meeting_type_key: Option<String>,
    /// The If-Match version.
    pub version: i64,
}

const CADENCE_COLUMNS: &str =
    "id, programme_id, kind, period, state, meta, meeting_type_key, version";

/// The closed set of `programme_cadence.state` values (0064's CHECK), in lifecycle order.
///
/// `active` is carried because the existing MSP editor test corrects a cadence to it through
/// `cadences.update`; no worker, gate or materialiser writes it.
pub const CADENCE_STATES: [&str; 6] = [
    "planned",
    "awaiting_approval",
    "rejected",
    "materialised",
    "closed",
    "active",
];

/// The writer a cadence state move belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CadenceWriter {
    /// The edit and approval-gate paths: `cadences.update` and the PI-plan gate.
    Edit,
    /// The materialiser, which sets the state in the transaction that drafts its cards.
    Materialiser,
}

/// The one `programme_cadence.state` transition table: every legal `(from, to)` move and the
/// writer allowed to make it. A same-state write is an idempotent metadata edit and needs no
/// entry.
///
/// `materialised` is the materialiser's own outcome — a cadence becomes materialised only
/// together with the cards it drafts — so an edit may never set it. `closed` is terminal.
fn cadence_move(from: &str, to: &str) -> Option<CadenceWriter> {
    match (from, to) {
        ("planned", "awaiting_approval" | "rejected" | "closed" | "active") => {
            Some(CadenceWriter::Edit)
        }
        ("planned", "materialised") => Some(CadenceWriter::Materialiser),
        ("awaiting_approval", "rejected") => Some(CadenceWriter::Edit),
        ("awaiting_approval", "materialised") => Some(CadenceWriter::Materialiser),
        ("rejected", "awaiting_approval") => Some(CadenceWriter::Edit),
        ("materialised", "closed") => Some(CadenceWriter::Edit),
        _ => None,
    }
}

/// Insert a cadence row.
pub async fn insert_cadence_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    kind: &str,
    period: &str,
    meta: &Value,
    meeting_type_key: Option<&str>,
) -> Result<CadenceRecord, PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Programme, programme_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    let sql = format!(
        "INSERT INTO programme_cadence
            (tenant_id, programme_id, kind, period, meta, meeting_type_key)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {CADENCE_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .bind(kind)
        .bind(period)
        .bind(meta)
        .bind(meeting_type_key)
        .fetch_one(&mut *conn)
        .await?)
}

/// List a programme's cadence rows.
pub async fn list_cadences_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<CadenceRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {CADENCE_COLUMNS} FROM programme_cadence
          WHERE tenant_id = $1 AND programme_id = $2 ORDER BY created_at, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Read one cadence row.
pub async fn get_cadence_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CadenceRecord>, PlanStoreError> {
    let sql =
        format!("SELECT {CADENCE_COLUMNS} FROM programme_cadence WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// The editable fields of a cadence row. The kind is the row's identity in the rhythm; a
/// correction changes the period, the state or the metadata.
#[derive(Clone, Debug, Default)]
pub struct CadencePatch {
    /// The period label.
    pub period: Option<String>,
    /// The state.
    pub state: Option<String>,
    /// Free-form metadata.
    pub meta: Option<Value>,
    /// The forum type the cadence instance materialises.
    pub meeting_type_key: Option<String>,
}

/// Edit a cadence row.
pub async fn update_cadence_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    patch: &CadencePatch,
) -> Result<CadenceRecord, PlanStoreError> {
    let current =
        get_cadence_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "cadence row",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "cadence row",
        });
    }
    // The state is a move in the one table, not a string the caller rewrites. `materialised` is
    // the materialiser's outcome and is refused here even though the table has it.
    if let Some(to) = &patch.state
        && to != &current.state
        && cadence_move(&current.state, to) != Some(CadenceWriter::Edit)
    {
        return Err(PlanStoreError::Illegal(
            PlanError::CadenceIllegalTransition {
                from: current.state.clone(),
                to: to.clone(),
            },
        ));
    }
    let sql = format!(
        "UPDATE programme_cadence SET
            period = coalesce($4, period),
            state = coalesce($5, state),
            meta = coalesce($6, meta),
            meeting_type_key = coalesce($7, meeting_type_key),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {CADENCE_COLUMNS}"
    );
    let row: Option<CadenceRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(&patch.period)
        .bind(&patch.state)
        .bind(&patch.meta)
        .bind(&patch.meeting_type_key)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale {
        what: "cadence row",
    })
}

/// Remove a cadence row.
pub async fn delete_cadence_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let current =
        get_cadence_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "cadence row",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "cadence row",
        });
    }
    let changed = sqlx::query(
        "DELETE FROM programme_cadence WHERE tenant_id = $1 AND id = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale {
            what: "cadence row",
        });
    }
    Ok(())
}

// ── PI plan approval ────────────────────────────────────────────────────────────────────

/// The gate a PI plan's sign-off is recorded under.
///
/// A plan gate has no run, card or project to hang on, so `approval.run_id` is NULL and
/// `uq_appr_pending` cannot see a second pending row. The gate's identity is
/// `meta.target` — the cadence id — which is what every read below matches on.
pub const PI_PLAN_APPROVAL_GATE: &str = "cadences.approve_plan";

/// The `approval.kind` a PI plan's gate carries.
pub const PI_PLAN_APPROVAL_KIND: &str = "plan";

/// The cadence state while its plan waits for a person's decision.
pub const CADENCE_AWAITING_APPROVAL: &str = "awaiting_approval";

/// The cadence state after a person rejected its plan.
pub const CADENCE_REJECTED: &str = "rejected";

/// The pending PI-plan gate for `cadence_id`, if one is open.
pub async fn pending_pi_plan_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<Option<Uuid>, PlanStoreError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND gate = $2 AND status = 'pending'
            AND meta->>'target' = $3
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(PI_PLAN_APPROVAL_GATE)
    .bind(cadence_id.to_string())
    .fetch_optional(&mut *conn)
    .await?)
}

/// The approved PI-plan gate whose approving decision is a person's or a delegated rule's.
///
/// A `human` approve always qualifies; a `rule` approve qualifies only when
/// `via.delegation_ref` names an active, published `category='decision'` document (the same
/// shape `tranches.resume` accepts). A service or an undelegated agent can decide a gate
/// through the generic approvals surface, and that decision must not turn an unapproved plan
/// into cards. [`materialise_pi_tx`] refuses without this row.
pub async fn pi_plan_human_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<Option<Uuid>, PlanStoreError> {
    Ok(sqlx::query_scalar(
        "SELECT a.id
           FROM approval a
          WHERE a.tenant_id = $1 AND a.gate = $2 AND a.status = 'approved'
            AND a.meta->>'target' = $3
            AND EXISTS (
                SELECT 1 FROM approval_decision d
                 WHERE d.tenant_id = a.tenant_id AND d.approval_id = a.id
                   AND d.decision = 'approve'
                   AND (d.decided_by_kind = 'human'
                        OR (d.decided_by_kind = 'rule'
                            AND d.via->>'delegation_ref' IS NOT NULL
                            AND EXISTS (
                                SELECT 1 FROM document doc
                                 WHERE doc.tenant_id = a.tenant_id
                                   AND doc.id::text = d.via->>'delegation_ref'
                                   AND doc.status = 'active'
                                   AND doc.published_at IS NOT NULL
                                   AND doc.category = 'decision'))))
          ORDER BY a.created_at, a.id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(PI_PLAN_APPROVAL_GATE)
    .bind(cadence_id.to_string())
    .fetch_optional(&mut *conn)
    .await?)
}

/// Open — or return — the pending gate for a PI plan.
///
/// The cadence row is locked first, so two requesters serialise and the second sees the first's
/// pending row. The insert names the cadence id in `meta.target`; that string, not a parent
/// column, is the gate's identity. A cadence already materialised has nothing left to approve.
pub async fn open_pi_plan_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
    requested_by: Option<Uuid>,
) -> Result<Approval, PlanStoreError> {
    let row: Option<(String, String, String, Uuid)> = sqlx::query_as(
        "SELECT kind, state, period, programme_id
           FROM programme_cadence
          WHERE tenant_id = $1 AND id = $2
          FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (kind, state, period, programme_id) = row.ok_or(PlanStoreError::NotFound {
        what: "cadence row",
    })?;
    if kind != "pi" {
        return Err(PlanStoreError::Illegal(PlanError::CadenceWrongKind {
            expected: "pi",
            actual: kind,
        }));
    }
    if state == "materialised" {
        return Err(PlanStoreError::Conflict {
            what: "cadence row",
            why: "the plan is already materialised and cannot be approved again",
        });
    }
    if let Some(existing) = pending_pi_plan_approval_tx(&mut *conn, tenant, cadence_id).await? {
        return get_approval_tx(&mut *conn, tenant, existing)
            .await?
            .map(|record| record.approval)
            .ok_or(PlanStoreError::NotFound { what: "approval" });
    }
    let mut input = NewApproval::new(PI_PLAN_APPROVAL_GATE, PI_PLAN_APPROVAL_KIND);
    input.requested_by = requested_by;
    input.meta = serde_json::json!({
        "target": cadence_id.to_string(),
        "cadence_id": cadence_id.to_string(),
        "programme_id": programme_id.to_string(),
        "period": period,
        "description": "Approve this PI plan before its backlog becomes cards",
    });
    insert_approval_tx(conn, tenant, &input)
        .await
        .map_err(PlanStoreError::from)
}

/// Record the state a PI plan's approval leaves the cadence in.
///
/// `awaiting_approval` means "nothing may be drafted until a person approves"; `rejected` is
/// the person's refusal. A materialised cadence is never moved — its cards are the record — and
/// every other move is checked against [`cadence_move`], so a closed cadence cannot re-open.
pub async fn set_pi_cadence_approval_state_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
    state: &str,
) -> Result<(), PlanStoreError> {
    let current: Option<String> = sqlx::query_scalar(
        "SELECT state FROM programme_cadence
          WHERE tenant_id = $1 AND id = $2 AND kind = 'pi' FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(from) = current else {
        return Ok(());
    };
    if from == "materialised" {
        return Ok(());
    }
    if state != from && cadence_move(&from, state) != Some(CadenceWriter::Edit) {
        return Err(PlanStoreError::Illegal(
            PlanError::CadenceIllegalTransition {
                from,
                to: state.to_owned(),
            },
        ));
    }
    sqlx::query(
        "UPDATE programme_cadence
            SET state = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND kind = 'pi' AND state <> 'materialised'",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id)
    .bind(state)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// What a PI materialisation wrote.
#[derive(Clone, Debug, Default)]
pub struct PiMaterialisation {
    /// The cards created, in WSJF order.
    pub cards: Vec<Uuid>,
    /// The backlog rows the WIP bound left untouched.
    pub backlog_left: i64,
}

/// Materialise a `kind='pi'` cadence: draft the next cadence's cards from the standing backlog.
///
/// **A plan is a person's decision.** Nothing is drafted until an `approval` for this cadence
/// carries a human `approve` (`cadences.approve_plan` is the writer): an unapproved plan is
/// [`PlanStoreError::PiPlanNotApproved`], never a silent draft, so the unattended worker tick
/// cannot commit a plan nobody signed off. The tick reports the refusal and leaves the cadence
/// `awaiting_approval`.
///
/// The backlog is a `wbs_node(kind='deliverable', status='open')` row with no card yet, across
/// every project whose `programme_id` is the cadence's programme (the ART's standing backlog).
/// The score is `(business_value + time_criticality + risk_reduction) / job_size`, computed in
/// the ordering and never stored (D-T1-4). The drafts land in the board named by
/// `meta.board_id`'s intake lane, bounded by the active lane's `wip_limit` minus the cards live
/// there; the cadence then moves `planned → materialised` in the same transaction, so a second
/// tick creates nothing. A tick with no free slot writes nothing and leaves the cadence
/// un-materialised, so a later tick still drafts.
pub async fn materialise_pi_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<PiMaterialisation, PlanStoreError> {
    let cadence = get_cadence_tx(&mut *conn, tenant, cadence_id)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "cadence row",
        })?;
    if cadence.kind != "pi" {
        return Err(PlanStoreError::Illegal(PlanError::CadenceWrongKind {
            expected: "pi",
            actual: cadence.kind.clone(),
        }));
    }
    if cadence.state == "materialised" {
        return Ok(PiMaterialisation::default());
    }
    if pi_plan_human_approval_tx(&mut *conn, tenant, cadence_id)
        .await?
        .is_none()
    {
        return Err(PlanStoreError::PiPlanNotApproved);
    }
    // The materialising move is the table's own: a planned or awaiting-approval cadence becomes
    // materialised with the cards drafted below. A rejected, active or closed cadence has no
    // path to cards.
    if cadence_move(&cadence.state, "materialised") != Some(CadenceWriter::Materialiser) {
        return Err(PlanStoreError::Illegal(
            PlanError::CadenceIllegalTransition {
                from: cadence.state.clone(),
                to: "materialised".to_owned(),
            },
        ));
    }
    let board_id: Uuid = cadence
        .meta
        .get("board_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(PlanStoreError::Illegal(PlanError::CadenceBoardMissing))?;
    let card_type_code = cadence
        .meta
        .get("card_type_code")
        .and_then(Value::as_str)
        .ok_or(PlanStoreError::Illegal(PlanError::CadenceCardTypeMissing))?
        .to_owned();
    // The board names the lanes the drafts land in and must exist; the BACKLOG is the cadence's
    // programme, not the board's one project (v1-p82-5).
    let board_project: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM board WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(board_id)
            .fetch_optional(&mut *conn)
            .await?;
    board_project.ok_or(PlanStoreError::NotFound { what: "board" })?;
    let card_type_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM card_type WHERE tenant_id = $1 AND code = $2")
            .bind(tenant.as_uuid())
            .bind(&card_type_code)
            .fetch_optional(&mut *conn)
            .await?;
    let card_type_id =
        card_type_id.ok_or(PlanStoreError::Illegal(PlanError::CadenceCardTypeMissing))?;

    let lanes: Vec<(Uuid, String, Option<i32>)> = sqlx::query_as(
        "SELECT id, kind, wip_limit FROM list WHERE tenant_id = $1 AND board_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_all(&mut *conn)
    .await?;
    let intake = lanes
        .iter()
        .find(|(_, kind, _)| kind == "intake")
        .map(|(id, _, _)| *id)
        .ok_or(PlanStoreError::NotFound {
            what: "intake lane",
        })?;
    let active = lanes.iter().find(|(_, kind, _)| kind == "active");
    let slots: i64 = match active {
        None => 0,
        Some((active_id, _, wip)) => {
            let live: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM card WHERE tenant_id = $1 AND list_id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(active_id)
            .fetch_one(&mut *conn)
            .await?;
            match wip {
                Some(limit) => (i64::from(*limit) - live).max(0),
                None => i64::MAX,
            }
        }
    };
    if slots == 0 {
        return Ok(PiMaterialisation::default());
    }

    let backlog_sql = "SELECT n.id, n.title, n.business_value, n.time_criticality,
                              n.risk_reduction, n.job_size
           FROM wbs_node n
           JOIN project p ON p.tenant_id = n.tenant_id AND p.id = n.project_id
          WHERE n.tenant_id = $1 AND p.programme_id = $2 AND n.kind = 'deliverable'
            AND n.status = 'open' AND n.card_id IS NULL";
    let rows: Vec<(Uuid, String, i32, i32, i32, i32)> = sqlx::query_as(AssertSqlSafe(format!(
        "{backlog_sql}
         ORDER BY ((n.business_value + n.time_criticality + n.risk_reduction)::numeric
                   / n.job_size) DESC,
                  n.position, n.id
         LIMIT $3"
    )))
    .bind(tenant.as_uuid())
    .bind(cadence.programme_id)
    .bind(slots)
    .fetch_all(&mut *conn)
    .await?;
    let backlog_total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT count(*) FROM ({backlog_sql}) backlog"
    )))
    .bind(tenant.as_uuid())
    .bind(cadence.programme_id)
    .fetch_one(&mut *conn)
    .await?;

    let mut cards = Vec::with_capacity(rows.len());
    for (index, (node_id, title, business_value, time_criticality, risk_reduction, job_size)) in
        rows.iter().enumerate()
    {
        // The domain is the arithmetic's single owner: a row whose inputs do not score is not
        // drafted, even if a CHECK were ever loosened.
        conveyor_domain::spine::wsjf(WsjfInputs {
            business_value: *business_value,
            time_criticality: *time_criticality,
            risk_reduction: *risk_reduction,
            job_size: *job_size,
        })?;
        let card_id: Uuid = sqlx::query_scalar(
            "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description,
                               position, meta)
             SELECT $1, l.board_id, l.id, $3, $4, '', $5, $6
               FROM list l
              WHERE l.tenant_id = $1 AND l.id = $2
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(intake)
        .bind(card_type_id)
        .bind(title)
        .bind(rust_decimal::Decimal::from(index as i64))
        .bind(serde_json::json!({
            "wbs_node_id": node_id,
            "cadence_id": cadence_id,
        }))
        .fetch_one(&mut *conn)
        .await?;
        sqlx::query(
            "UPDATE wbs_node SET card_id = $3, version = version + 1, updated_at = now()
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(node_id)
        .bind(card_id)
        .execute(&mut *conn)
        .await?;
        cards.push(card_id);
    }

    sqlx::query(
        "UPDATE programme_cadence
            SET state = 'materialised', version = version + 1, updated_at = now(),
                meta = meta || jsonb_build_object('materialised_at', now(),
                                                  'drafted', $3::int)
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id)
    .bind(i32::try_from(cards.len()).unwrap_or(i32::MAX))
    .execute(&mut *conn)
    .await?;

    Ok(PiMaterialisation {
        backlog_left: backlog_total - i64::try_from(cards.len()).unwrap_or(i64::MAX),
        cards,
    })
}

/// What a backlog refinement revalidated and flagged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BacklogRefinement {
    /// The open deliverables whose four WSJF inputs were revalidated.
    pub revalidated: i64,
    /// The rows that cannot score or have not moved inside the refinement window.
    pub flagged: i64,
}

/// Refine the programme's standing backlog for a `kind='refinement'` cadence.
///
/// Every open `deliverable` of every project in the cadence's programme is revalidated against
/// the four typed WSJF inputs. A row is flagged stale, with its reason on `meta.refinement`,
/// when the inputs do not score (a negative value or a zero-size job), when every value input
/// is zero (it has no WSJF inputs at all), or when it has not moved for `meta.stale_after_days`
/// (default 30). A row that scores again loses a previous flag. The score itself is never
/// stored (D-T1-4); only the flag is, so the board can say why an item needs attention.
///
/// The cadence move to `materialised` belongs to the caller (the worker tick), so a failed
/// refinement never marks the period done.
pub async fn refine_backlog_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<BacklogRefinement, PlanStoreError> {
    let cadence = get_cadence_tx(&mut *conn, tenant, cadence_id)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "cadence row",
        })?;
    if cadence.kind != "refinement" {
        return Err(PlanStoreError::Illegal(PlanError::CadenceWrongKind {
            expected: "refinement",
            actual: cadence.kind.clone(),
        }));
    }
    if cadence.state == "materialised" {
        return Ok(BacklogRefinement::default());
    }
    if cadence_move(&cadence.state, "materialised") != Some(CadenceWriter::Materialiser) {
        return Err(PlanStoreError::Illegal(
            PlanError::CadenceIllegalTransition {
                from: cadence.state.clone(),
                to: "materialised".to_owned(),
            },
        ));
    }
    let stale_after_days = cadence
        .meta
        .get("stale_after_days")
        .and_then(Value::as_i64)
        .unwrap_or(30)
        .clamp(1, 3650);
    let rows: Vec<(Uuid, i32, i32, i32, i32, bool, Value)> = sqlx::query_as(
        "SELECT w.id, w.business_value, w.time_criticality, w.risk_reduction, w.job_size,
                (w.updated_at < now() - make_interval(days => $3::int)) AS stale_by_age,
                w.meta
           FROM wbs_node w
           JOIN project p ON p.tenant_id = w.tenant_id AND p.id = w.project_id
          WHERE w.tenant_id = $1 AND p.programme_id = $2
            AND w.kind = 'deliverable' AND w.status = 'open'
          ORDER BY w.id",
    )
    .bind(tenant.as_uuid())
    .bind(cadence.programme_id)
    .bind(i32::try_from(stale_after_days).unwrap_or(30))
    .fetch_all(&mut *conn)
    .await?;
    let revalidated = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    let mut flagged = 0i64;
    for (id, business_value, time_criticality, risk_reduction, job_size, stale_by_age, meta) in rows
    {
        let score = conveyor_domain::spine::wsjf(WsjfInputs {
            business_value,
            time_criticality,
            risk_reduction,
            job_size,
        })
        .ok();
        let reason = match score {
            None => Some("the four WSJF inputs do not score"),
            Some(value) if value <= 0.0 => Some("every WSJF value input is zero"),
            Some(_) if stale_by_age => Some("no change in the refinement window"),
            Some(_) => None,
        };
        let mut next = if meta.is_object() {
            meta.clone()
        } else {
            serde_json::json!({})
        };
        let object = next.as_object_mut().expect("just built an object");
        match reason {
            Some(reason) => {
                object.insert(
                    "refinement".to_owned(),
                    serde_json::json!({
                        "stale": true,
                        "reason": reason,
                        "cadence_id": cadence_id,
                        "stale_after_days": stale_after_days,
                    }),
                );
                flagged += 1;
            }
            None => {
                if object.remove("refinement").is_none() {
                    continue;
                }
            }
        }
        sqlx::query(
            "UPDATE wbs_node
                SET meta = $3, version = version + 1, updated_at = now()
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&next)
        .execute(&mut *conn)
        .await?;
    }
    Ok(BacklogRefinement {
        revalidated,
        flagged,
    })
}

/// Record a demo credit for one completed deliverable, or return the existing one.
///
/// A credit needs the deliverable accepted (`wbs_node.status = 'accepted'`) and exhibiting at
/// least one evidence ref — §5.4's `gate_evidence_missing` otherwise. The evidence is copied
/// onto the credit, so the credit stands alone when the node later changes.
pub async fn credit_demo_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
    deliverable_id: Uuid,
) -> Result<Uuid, PlanStoreError> {
    let cadence = get_cadence_tx(&mut *conn, tenant, cadence_id)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "cadence row",
        })?;
    if cadence.kind != "demo" {
        return Err(PlanStoreError::Illegal(PlanError::CadenceWrongKind {
            expected: "demo",
            actual: cadence.kind.clone(),
        }));
    }
    let row: Option<(String, Value)> = sqlx::query_as(
        "SELECT status, evidence FROM wbs_node
          WHERE tenant_id = $1 AND id = $2 AND kind = 'deliverable'",
    )
    .bind(tenant.as_uuid())
    .bind(deliverable_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (status, evidence) = row.ok_or(PlanStoreError::NotFound {
        what: "deliverable",
    })?;
    if status != "accepted" {
        return Err(PlanStoreError::Illegal(
            PlanError::DemoDeliverableNotComplete,
        ));
    }
    if evidence.as_array().is_none_or(|refs| refs.is_empty()) {
        return Err(PlanStoreError::Illegal(PlanError::DemoEvidenceMissing));
    }
    if !typed_evidence(&evidence) {
        return Err(PlanStoreError::Illegal(PlanError::DemoEvidenceUntyped));
    }
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO programme_demo_credit (tenant_id, cadence_id, deliverable_id, evidence)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, cadence_id, deliverable_id) DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id)
    .bind(deliverable_id)
    .bind(&evidence)
    .fetch_optional(&mut *conn)
    .await?;
    match inserted {
        Some(id) => Ok(id),
        None => sqlx::query_scalar(
            "SELECT id FROM programme_demo_credit
              WHERE tenant_id = $1 AND cadence_id = $2 AND deliverable_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(cadence_id)
        .bind(deliverable_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(PlanStoreError::Database),
    }
}

/// Whether an evidence array carries typed refs: every item is a non-empty string or an object
/// with at least one non-empty string value. The live rework refs are path strings; a blank
/// string, null or an empty object is not something a demo can exhibit.
fn typed_evidence(evidence: &Value) -> bool {
    evidence.as_array().is_some_and(|refs| {
        !refs.is_empty()
            && refs.iter().all(|item| match item {
                Value::String(text) => !text.trim().is_empty(),
                Value::Object(fields) => fields
                    .values()
                    .any(|value| value.as_str().is_some_and(|text| !text.trim().is_empty())),
                _ => false,
            })
    })
}

/// A demo credit row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DemoCreditRecord {
    /// The row id.
    pub id: Uuid,
    /// The demo cadence the credit was recorded at.
    pub cadence_id: Uuid,
    /// The accepted deliverable the credit names.
    pub deliverable_id: Uuid,
    /// The deliverable's evidence, copied when the credit was recorded.
    pub evidence: Value,
    /// When the credit was recorded.
    pub credited_at: DateTime<Utc>,
    /// The If-Match version.
    pub version: i64,
}

const DEMO_CREDIT_COLUMNS: &str = "id, cadence_id, deliverable_id, evidence, credited_at, version";

/// Read one demo credit.
pub async fn get_demo_credit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DemoCreditRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {DEMO_CREDIT_COLUMNS} FROM programme_demo_credit
          WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Read the credit for one (cadence, deliverable), if it was recorded.
pub async fn find_demo_credit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
    deliverable_id: Uuid,
) -> Result<Option<DemoCreditRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {DEMO_CREDIT_COLUMNS} FROM programme_demo_credit
          WHERE tenant_id = $1 AND cadence_id = $2 AND deliverable_id = $3"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(cadence_id)
        .bind(deliverable_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a demo cadence's credits.
pub async fn list_demo_credits_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<Vec<DemoCreditRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {DEMO_CREDIT_COLUMNS} FROM programme_demo_credit
          WHERE tenant_id = $1 AND cadence_id = $2 ORDER BY credited_at, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(cadence_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Propose a lesson for one retro finding; the table is the organisational-memory record.
///
/// The evidence refs are stored as given (T8 owns the independent-ref rule); the cadence is the
/// retro the lesson came from, kept so a later review can read the finding beside its lesson.
pub async fn propose_lesson_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
    cadence_id: Option<Uuid>,
    finding_key: &str,
    statement: &str,
    evidence: &Value,
) -> Result<Uuid, PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Programme, programme_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    if statement.trim().is_empty() {
        return Err(PlanStoreError::Conflict {
            what: "lesson",
            why: "the statement is required",
        });
    }
    // A lesson is evidence first: an empty array (or a non-array) references nothing at all.
    if evidence.as_array().is_none_or(|refs| refs.is_empty()) {
        return Err(PlanStoreError::Illegal(PlanError::LessonEvidenceMissing));
    }
    // T8's rule reaches the retro too: the count is the distinct referenced objects, and a
    // single independent reference is never stored (the table CHECK is the backstop).
    let independent_count = crate::lesson::independent_count(evidence);
    if independent_count == 1 {
        return Err(PlanStoreError::Conflict {
            what: "lesson",
            why: "a lesson needs at least two independent evidence references; one reference \
                  is not evidence",
        });
    }
    sqlx::query_scalar(
        "INSERT INTO lesson (tenant_id, programme_id, cadence_id, finding_key, statement,
                             evidence, independent_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(programme_id)
    .bind(cadence_id)
    .bind(finding_key.trim())
    .bind(statement.trim())
    .bind(evidence)
    .bind(independent_count)
    .fetch_one(&mut *conn)
    .await
    .map_err(PlanStoreError::Database)
}

/// A lesson row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LessonRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme whose memory holds the lesson.
    pub programme_id: Uuid,
    /// The retro cadence the finding came from, when known.
    pub cadence_id: Option<Uuid>,
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

const LESSON_COLUMNS: &str = "id, programme_id, cadence_id, finding_key, statement, evidence, \
                              independent_count, state, source, meta, version, created_at, \
                              updated_at";

/// Read one lesson.
pub async fn get_lesson_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<LessonRecord>, PlanStoreError> {
    let sql = format!("SELECT {LESSON_COLUMNS} FROM lesson WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a programme's lessons.
pub async fn list_lessons_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<LessonRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {LESSON_COLUMNS} FROM lesson
          WHERE tenant_id = $1 AND programme_id = $2 ORDER BY created_at, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Open the retro cadence's one card (in the board named by `meta.board_id`), or return it.
///
/// The card is the coach's: its metadata names the coach as the owner kind and carries the
/// findings the coach records. The lookup is by `meta.cadence_id`, so a second call returns the
/// same card rather than a duplicate.
pub async fn open_retro_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<Uuid, PlanStoreError> {
    let cadence = get_cadence_tx(&mut *conn, tenant, cadence_id)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "cadence row",
        })?;
    if cadence.kind != "retro" {
        return Err(PlanStoreError::Illegal(PlanError::CadenceWrongKind {
            expected: "retro",
            actual: cadence.kind.clone(),
        }));
    }
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM card
          WHERE tenant_id = $1 AND meta->>'cadence_id' = $2 LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(card) = existing {
        return Ok(card);
    }
    let board_id: Uuid = cadence
        .meta
        .get("board_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(PlanStoreError::Illegal(PlanError::CadenceBoardMissing))?;
    let card_type_code = cadence
        .meta
        .get("card_type_code")
        .and_then(Value::as_str)
        .ok_or(PlanStoreError::Illegal(PlanError::CadenceCardTypeMissing))?
        .to_owned();
    let card_type_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM card_type WHERE tenant_id = $1 AND code = $2")
            .bind(tenant.as_uuid())
            .bind(&card_type_code)
            .fetch_optional(&mut *conn)
            .await?;
    let card_type_id =
        card_type_id.ok_or(PlanStoreError::Illegal(PlanError::CadenceCardTypeMissing))?;
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description,
                           position, meta)
         SELECT $1, l.board_id, l.id, $3, $4, '', 0, $5
           FROM list l
          WHERE l.tenant_id = $1 AND l.board_id = $2 AND l.kind = 'intake'
          LIMIT 1
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(card_type_id)
    .bind(format!("Retro — {}", cadence.period))
    .bind(serde_json::json!({
        "cadence_id": cadence_id,
        "owner_kind": "coach",
        "findings": [],
    }))
    .fetch_one(&mut *conn)
    .await
    .map_err(PlanStoreError::Database)
}

/// Record the findings the coach gathered on the retro card (`meta.findings`).
pub async fn record_retro_findings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
    findings: &Value,
) -> Result<(), PlanStoreError> {
    if findings.as_array().is_none_or(|rows| rows.is_empty()) {
        return Err(PlanStoreError::Conflict {
            what: "retro",
            why: "at least one finding is required",
        });
    }
    let changed = sqlx::query(
        "UPDATE card
            SET meta = meta || jsonb_build_object('findings', $2::jsonb),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND meta->>'cadence_id' = $3",
    )
    .bind(tenant.as_uuid())
    .bind(findings)
    .bind(cadence_id.to_string())
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::NotFound { what: "retro card" });
    }
    Ok(())
}

/// Close the retro: every finding it recorded must have a lesson proposal.
///
/// The findings live on the coach's card; the lessons are `lesson` rows carrying the cadence and
/// the finding key. A missing lesson is named, and nothing is written.
pub async fn close_retro_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cadence_id: Uuid,
) -> Result<(), PlanStoreError> {
    let cadence = get_cadence_tx(&mut *conn, tenant, cadence_id)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "cadence row",
        })?;
    if cadence.kind != "retro" {
        return Err(PlanStoreError::Illegal(PlanError::CadenceWrongKind {
            expected: "retro",
            actual: cadence.kind.clone(),
        }));
    }
    let card: Option<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, meta FROM card WHERE tenant_id = $1 AND meta->>'cadence_id' = $2 LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    let (card_id, meta) = card.ok_or(PlanStoreError::NotFound { what: "retro card" })?;
    let findings = meta
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for finding in &findings {
        let key = finding
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM lesson
                 WHERE tenant_id = $1 AND cadence_id = $2 AND finding_key = $3
            )",
        )
        .bind(tenant.as_uuid())
        .bind(cadence_id)
        .bind(&key)
        .fetch_one(&mut *conn)
        .await?;
        if present {
            continue;
        }
        // A finding carries its lesson: the statement and the evidence refs. A close with no
        // lesson content is refused, and the whole transaction writes nothing.
        let statement = finding
            .get("statement")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let evidence = finding.get("evidence").cloned().unwrap_or(Value::Null);
        // Two different defects, two different answers: a finding with no lesson row and no
        // inline proposal has NO lesson (named as such), while one that carries a partial
        // proposal — a statement without evidence refs, or evidence without a statement —
        // is an INCOMPLETE lesson. Collapsing them hid the missing-lesson case behind the
        // wrong error (measured: retro_cannot_close_without_lessons expected
        // RetroFindingWithoutLesson and got RetroFindingIncomplete).
        let empty_evidence = !evidence.as_array().is_some_and(|refs| !refs.is_empty());
        if statement.is_empty() && empty_evidence {
            return Err(PlanStoreError::Illegal(
                PlanError::RetroFindingWithoutLesson(key),
            ));
        }
        if statement.is_empty() || empty_evidence {
            return Err(PlanStoreError::Illegal(PlanError::RetroFindingIncomplete(
                key,
            )));
        }
        let systemic = finding
            .get("systemic")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || finding.get("kind").and_then(Value::as_str) == Some("systemic");
        let lesson_meta = if systemic {
            // A systemic finding names the improvement it believes is worth opening. When it also
            // names the binding the kata would run on and a pre-registered prediction, the close
            // emits the typed candidate — one kata, one `proposed` candidate row — in this
            // transaction. A marker without them stays the lesson's note; `kata.open` remains the
            // only path that makes a kata measurable.
            let kata = finding.get("kata").cloned().unwrap_or(Value::Null);
            let signature = kata
                .get("signature")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let title = kata
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let rationale = kata
                .get("rationale")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let target_ref = kata.get("target_ref").cloned().unwrap_or(Value::Null);
            if signature.is_empty() || title.is_empty() || !target_ref.is_object() {
                return Err(PlanStoreError::Illegal(PlanError::RetroFindingIncomplete(
                    key,
                )));
            }
            let prediction = kata.get("prediction").cloned().unwrap_or(Value::Null);
            let binding_id = kata
                .get("job_binding_id")
                .and_then(Value::as_str)
                .and_then(|raw| Uuid::parse_str(raw).ok());
            if let (Some(binding_id), true) = (
                binding_id,
                prediction
                    .as_object()
                    .is_some_and(|object| !object.is_empty()),
            ) {
                let threshold = prediction.get("threshold").and_then(Value::as_f64);
                crate::kata::open_retro_candidate_tx(
                    &mut *conn,
                    tenant,
                    &signature,
                    &title,
                    &rationale,
                    &target_ref,
                    &prediction,
                    binding_id,
                    threshold,
                )
                .await
                .map_err(|error| {
                    tracing::error!(
                        ?error,
                        finding = %key,
                        "close the retro: open its kata candidate"
                    );
                    PlanStoreError::Illegal(PlanError::RetroFindingIncomplete(key.clone()))
                })?;
            }
            serde_json::json!({
                "kata_candidate": {
                    "signature": signature,
                    "title": title,
                    "rationale": rationale,
                    "target_ref": target_ref,
                    "prediction": prediction,
                }
            })
        } else {
            serde_json::json!({})
        };
        let independent_count = i32::try_from(
            evidence
                .as_array()
                .map(|refs| refs.len())
                .unwrap_or_default(),
        )
        .unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO lesson (tenant_id, programme_id, cadence_id, finding_key, statement,
                                 evidence, independent_count, state, source, meta)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'proposed', 'retro', $8)",
        )
        .bind(tenant.as_uuid())
        .bind(cadence.programme_id)
        .bind(cadence_id)
        .bind(&key)
        .bind(&statement)
        .bind(&evidence)
        .bind(independent_count)
        .bind(&lesson_meta)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query(
        "UPDATE card
            SET meta = meta || jsonb_build_object('retro_closed', true),
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Refuse a project start when the programme's tranche has not been authorised.
///
/// The rule is the domain's (`PlanError::TrancheNotAuthorised`); the action maps it to
/// `409 tranche_not_authorised`.
///
/// [`project_tranche_state_tx`] answers `None` for two different facts, and only one of them is
/// allowed: a project outside a programme keeps its legacy allowance, while a project inside a
/// programme that HAS a tranche plan but no row for the project's tranche is sitting in an
/// undeclared tranche and is refused. A programme that has declared no tranche plan at all (the
/// pre-tranche programmes) also keeps the legacy allowance — the rule binds the first tranche to
/// a plan that exists, it does not invent one. An authorised-looking state is only an
/// authorisation when the row names the review that set it and that review is a review of this
/// tranche — except for `closed`, which is terminal: the engine reaches it only through a
/// `continue` review of the tranche itself, and the seeded genesis tranche is closed from birth.
pub async fn assert_project_tranche_authorised_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), PlanStoreError> {
    match project_tranche_state_tx(&mut *conn, tenant, project_id).await? {
        // A closed tranche is past authorisation by construction: the only engine path to
        // `closed` is a review of that tranche whose decision is `continue`
        // (`record_tranche_review_tx`), and the programme's genesis tranche is seeded closed.
        // Its `authorisation_review_id` names the review that authorised *it*, which for the
        // genesis row is never written — so requiring the link here refused every close in the
        // programme's first tranche (measured 2026-11-26: DESIGN-PUBLISH's rework halted at act
        // 111, `projects.close` 409 `closure_tranche_not_authorised`, while the seed itself
        // declares tranche 0 closed).
        Some((_number, state)) if state == "closed" => Ok(()),
        Some((number, state)) if matches!(state.as_str(), "authorised" | "in_review") => {
            let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
                "SELECT t.id, t.authorisation_review_id
                   FROM programme_tranche t
                   JOIN project p
                     ON p.tenant_id = t.tenant_id AND p.programme_id = t.programme_id
                  WHERE p.tenant_id = $1 AND p.id = $2 AND t.number = $3",
            )
            .bind(tenant.as_uuid())
            .bind(project_id)
            .bind(number)
            .fetch_optional(&mut *conn)
            .await?;
            let Some((tranche_id, Some(_))) = row else {
                return Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised));
            };
            let linked: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1
                      FROM programme_tranche t
                      JOIN programme_tranche_review r
                        ON r.tenant_id = t.tenant_id AND r.id = t.authorisation_review_id
                     WHERE t.tenant_id = $1 AND t.id = $2
                       AND r.decision = 'continue'
                       AND (
                            r.tranche_id = t.id
                            OR EXISTS (
                                SELECT 1 FROM programme_tranche prev
                                 WHERE prev.tenant_id = r.tenant_id
                                   AND prev.id = r.tranche_id
                                   AND prev.programme_id = t.programme_id
                                   AND prev.number = t.number - 1)
                       )
                )",
            )
            .bind(tenant.as_uuid())
            .bind(tranche_id)
            .fetch_one(&mut *conn)
            .await?;
            if linked {
                Ok(())
            } else {
                Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised))
            }
        }
        Some((_number, _state)) => Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised)),
        None => {
            let planned: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM project p
                      JOIN programme_tranche t
                        ON t.tenant_id = p.tenant_id AND t.programme_id = p.programme_id
                     WHERE p.tenant_id = $1 AND p.id = $2
                )",
            )
            .bind(tenant.as_uuid())
            .bind(project_id)
            .fetch_one(&mut *conn)
            .await?;
            if planned {
                // The programme has a tranche plan; the project's tranche number is not in it.
                Err(PlanStoreError::Illegal(PlanError::TrancheNotAuthorised))
            } else {
                Ok(())
            }
        }
    }
}

/// Record a dependency slip: the slipped project's output is late, so the benefits mapped to
/// that output are at risk.
///
/// The benefits come from the map's edges — `benefit_link.output_ref` names a `wbs_node` id —
/// never from prose: the response carries the ids the register and the console render.
pub struct DependencySlip {
    /// The dependency edge that slipped.
    pub dependency_id: Uuid,
    /// The benefits the map puts at risk, deduplicated.
    pub benefits: Vec<Uuid>,
}

pub async fn record_dependency_slip_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    dependency_id: Uuid,
) -> Result<DependencySlip, PlanStoreError> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT from_project, to_project FROM project_dependency
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(dependency_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (_from, to_project) = row.ok_or(PlanStoreError::NotFound {
        what: "project dependency",
    })?;
    // An edge that names no node cannot be attributed to a project, so it might have been this
    // slip's output: report it rather than silently under-reporting the benefits at risk. The
    // scope is the slipped project's programme, where the map lives.
    let programme: Option<Uuid> =
        sqlx::query_scalar("SELECT programme_id FROM project WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(to_project)
            .fetch_one(&mut *conn)
            .await?;
    if let Some(programme) = programme {
        let unresolved: Option<String> = sqlx::query_scalar(
            "SELECT l.output_ref
               FROM benefit_link l
              WHERE l.tenant_id = $1 AND l.programme_id = $2
                AND NOT EXISTS (
                    SELECT 1 FROM wbs_node w
                     WHERE w.tenant_id = l.tenant_id AND w.id::text = l.output_ref)
              ORDER BY l.output_ref
              LIMIT 1",
        )
        .bind(tenant.as_uuid())
        .bind(programme)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(output_ref) = unresolved {
            return Err(PlanStoreError::Illegal(
                PlanError::BenefitOutputUnresolved { output_ref },
            ));
        }
    }
    let benefits: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT l.benefit_id
           FROM benefit_link l
           JOIN wbs_node w ON w.tenant_id = l.tenant_id AND w.id::text = l.output_ref
          WHERE l.tenant_id = $1 AND w.project_id = $2
          ORDER BY l.benefit_id",
    )
    .bind(tenant.as_uuid())
    .bind(to_project)
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE project_dependency
            SET status = 'slipped', version = version + 1, updated_at = now(),
                meta = meta || jsonb_build_object('slipped_at', now())
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(dependency_id)
    .execute(&mut *conn)
    .await?;
    Ok(DependencySlip {
        dependency_id,
        benefits,
    })
}

/// A benefit's realization: the measured baseline, the post-transition measurement and the
/// delta, attributed to the operational area the profile names.
pub struct BenefitRealization {
    /// The benefit.
    pub benefit_id: Uuid,
    /// When the baseline was measured.
    pub baseline_measured_at: Option<DateTime<Utc>>,
    /// When the post-transition measurement was taken.
    pub post_measured_at: Option<DateTime<Utc>>,
    /// The operational area the improvement is attributed to.
    pub operational_area: String,
    /// The baseline value, when the profile records one.
    pub baseline_value: Option<f64>,
    /// The post-transition value, when the transition's evidence records one.
    pub post_value: Option<f64>,
    /// `post - baseline`, when both values exist.
    pub delta: Option<f64>,
    /// When the legacy practice was retired.
    pub retired_at: Option<DateTime<Utc>>,
}

pub async fn benefit_realization_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    benefit_id: Uuid,
) -> Result<BenefitRealization, PlanStoreError> {
    let profile: Option<(Value, String)> = sqlx::query_as(
        "SELECT coalesce(baseline, '{}'::jsonb), operational_area FROM benefit_profile
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(benefit_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (baseline, operational_area) = profile.ok_or(PlanStoreError::NotFound {
        what: "benefit profile",
    })?;
    let transition = get_transition_tx(&mut *conn, tenant, benefit_id)
        .await?
        .ok_or(PlanStoreError::Conflict {
            what: "benefit realization",
            why: "the transition has not started",
        })?;
    if transition.post_measured_at.is_some() && transition.baseline_measured_at.is_none() {
        return Err(PlanStoreError::Illegal(PlanError::BenefitBaselineMissing));
    }
    let baseline_value = baseline.get("value").and_then(Value::as_f64).or_else(|| {
        transition
            .evidence
            .get("baseline_value")
            .and_then(Value::as_f64)
    });
    let post_value = transition
        .evidence
        .get("post_value")
        .and_then(Value::as_f64);
    let delta = match (baseline_value, post_value) {
        (Some(baseline), Some(post)) => Some(post - baseline),
        _ => None,
    };
    Ok(BenefitRealization {
        benefit_id,
        baseline_measured_at: transition.baseline_measured_at,
        post_measured_at: transition.post_measured_at,
        operational_area,
        baseline_value,
        post_value,
        delta,
        retired_at: transition.retired_at,
    })
}

/// The conditions a programme meets before it may reach `closed`.
///
/// A programme closes on its decision record and its measurement plan: at least one of its
/// projects holds an approved `closure` governed asset, and the newest approved (or published)
/// `benefits_realization_plan` names a `job` row as `meta.post_closure_owner` — a job id in this
/// tenant, never free text. Every path that writes `closed` runs this first, so
/// `programmes.update` and [`close_programme_tx`] cannot diverge; a refusal writes nothing.
pub async fn assert_programme_can_close_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<(), PlanStoreError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM programme WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(programme_id)
    .fetch_one(&mut *conn)
    .await?;
    if !exists {
        return Err(PlanStoreError::NotFound { what: "programme" });
    }
    let closure_asset: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM governed_asset a
              JOIN project p ON p.tenant_id = a.tenant_id AND p.id = a.project_id
             WHERE a.tenant_id = $1 AND p.programme_id = $2
               AND a.kind = 'closure' AND a.status = 'approved'
        )",
    )
    .bind(tenant.as_uuid())
    .bind(programme_id)
    .fetch_one(&mut *conn)
    .await?;
    if !closure_asset {
        return Err(PlanStoreError::Illegal(PlanError::ClosureAssetMissing));
    }
    // "No plan" and "a plan with no owner" are different refusals, because they need different
    // work. A plan that is not approved (or published) is no closure plan at all.
    let owner: Option<Option<String>> = sqlx::query_scalar(
        "SELECT a.meta->>'post_closure_owner'
           FROM governed_asset a
           JOIN project p ON p.tenant_id = a.tenant_id AND p.id = a.project_id
          WHERE a.tenant_id = $1 AND p.programme_id = $2
            AND a.kind = 'benefits_realization_plan'
            AND a.status IN ('approved', 'published')
          ORDER BY a.updated_at DESC, a.id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(programme_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(owner) = owner else {
        return Err(PlanStoreError::Illegal(PlanError::RealizationPlanMissing));
    };
    let Some(owner_id) = owner
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return Err(PlanStoreError::Illegal(PlanError::PostClosureOwnerMissing));
    };
    let job: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM job WHERE tenant_id = $1 AND id = $2)")
            .bind(tenant.as_uuid())
            .bind(owner_id)
            .fetch_one(&mut *conn)
            .await?;
    if !job {
        return Err(PlanStoreError::Illegal(PlanError::PostClosureOwnerMissing));
    }
    Ok(())
}

/// Close a programme: its decision record and its measurement owner must both exist.
///
/// The conditions are [`assert_programme_can_close_tx`]'s, shared with every `programmes.update`
/// that writes `closed`; the refusal writes nothing.
pub async fn close_programme_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<(), PlanStoreError> {
    assert_programme_can_close_tx(&mut *conn, tenant, programme_id).await?;
    sqlx::query(
        "UPDATE programme SET status = 'closed', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(programme_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// ── stakeholder_profile ─────────────────────────────────────────────────────────────────
// The MSP profile: concerns, influence, exposure, support and engagement. Distinct from
// `project_stakeholder` (0008), which records who is involved, not what they care about.

/// A stakeholder profile.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct StakeholderRecord {
    /// The row id.
    pub id: Uuid,
    /// The programme.
    pub programme_id: Uuid,
    /// The project, if the profile is project-scoped.
    pub project_id: Option<Uuid>,
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

const STAKEHOLDER_COLUMNS: &str = "id, programme_id, project_id, name, concerns, sensitivities, \
    importance, influence, interest, exposure, support_level, support_trend, engagement_target, \
    benefits, disbenefits, version";

/// A new stakeholder profile.
#[derive(Clone, Debug, Default)]
pub struct NewStakeholderProfile {
    /// The programme.
    pub programme_id: Uuid,
    /// The project, if project-scoped.
    pub project_id: Option<Uuid>,
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
}

/// Insert a stakeholder profile.
pub async fn insert_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewStakeholderProfile,
) -> Result<StakeholderRecord, PlanStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Programme, new.programme_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Project, new.project_id)
        .await
        .map_err(PlanStoreError::Parent)?;
    let sql = format!(
        "INSERT INTO stakeholder_profile
            (tenant_id, programme_id, project_id, name, concerns, sensitivities, importance,
             influence, interest, exposure, support_level, support_trend, engagement_target,
             benefits, disbenefits)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
         RETURNING {STAKEHOLDER_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.programme_id)
        .bind(new.project_id)
        .bind(&new.name)
        .bind(&new.concerns)
        .bind(&new.sensitivities)
        .bind(&new.importance)
        .bind(&new.influence)
        .bind(&new.interest)
        .bind(&new.exposure)
        .bind(&new.support_level)
        .bind(&new.support_trend)
        .bind(&new.engagement_target)
        .bind(&new.benefits)
        .bind(&new.disbenefits)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one stakeholder profile inside the caller's tenant.
pub async fn get_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<StakeholderRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {STAKEHOLDER_COLUMNS} FROM stakeholder_profile WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// The editable fields of a stakeholder profile.
#[derive(Clone, Debug, Default)]
pub struct StakeholderPatch {
    /// The name.
    pub name: Option<String>,
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

/// Edit a stakeholder profile under `If-Match`.
pub async fn update_stakeholder_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
    patch: &StakeholderPatch,
) -> Result<StakeholderRecord, PlanStoreError> {
    let current =
        get_stakeholder_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "stakeholder profile",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "stakeholder profile",
        });
    }
    let sql = format!(
        "UPDATE stakeholder_profile SET
            name = coalesce($4, name),
            concerns = coalesce($5, concerns),
            sensitivities = coalesce($6, sensitivities),
            importance = coalesce($7, importance),
            influence = coalesce($8, influence),
            interest = coalesce($9, interest),
            exposure = coalesce($10, exposure),
            support_level = coalesce($11, support_level),
            support_trend = coalesce($12, support_trend),
            engagement_target = coalesce($13, engagement_target),
            benefits = coalesce($14, benefits),
            disbenefits = coalesce($15, disbenefits),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {STAKEHOLDER_COLUMNS}"
    );
    let row: Option<StakeholderRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(&patch.name)
        .bind(&patch.concerns)
        .bind(&patch.sensitivities)
        .bind(&patch.importance)
        .bind(&patch.influence)
        .bind(&patch.interest)
        .bind(&patch.exposure)
        .bind(&patch.support_level)
        .bind(&patch.support_trend)
        .bind(&patch.engagement_target)
        .bind(&patch.benefits)
        .bind(&patch.disbenefits)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale {
        what: "stakeholder profile",
    })
}

/// Remove a stakeholder profile.
pub async fn delete_stakeholder_profile_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: i64,
) -> Result<(), PlanStoreError> {
    let current =
        get_stakeholder_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(PlanStoreError::NotFound {
                what: "stakeholder profile",
            })?;
    if current.version != expected {
        return Err(PlanStoreError::Stale {
            what: "stakeholder profile",
        });
    }
    let changed = sqlx::query(
        "DELETE FROM stakeholder_profile WHERE tenant_id = $1 AND id = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(PlanStoreError::Stale {
            what: "stakeholder profile",
        });
    }
    Ok(())
}

/// List a programme's stakeholder profiles.
pub async fn list_stakeholders_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<StakeholderRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {STAKEHOLDER_COLUMNS} FROM stakeholder_profile
          WHERE tenant_id = $1 AND programme_id = $2 ORDER BY created_at, id"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}
