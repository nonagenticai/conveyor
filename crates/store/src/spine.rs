#![forbid(unsafe_code)]
//! T1 — the plan spine's queries (SPEC_V2 §5.1, §5.3–§5.5, §8.2, §8.3; issue #72).
//!
//! Every function is `_tx`: the caller has already opened a tenant-scoped transaction, and the
//! write shares it with the action's outbox event (CONTRACT §7). No function here speaks the
//! action layer's vocabulary; a refusal the domain owns is raised as [`PlanStoreError::Illegal`]
//! and mapped once, by the action.
//!
//! The one read model is [`in_flight_tx`]: it selects projects by the tenant's configured
//! in-flight set (`tenant.settings -> 'portfolio' -> 'in_flight'`, default `active,paused`).
//! There is no portfolio row (D-T1-7).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::spine::{
    BenefitMapFields, BenefitProfileFields, PlanError, WsjfInputs, benefit_map_missing,
    benefit_profile_missing, tranche_review_has_evidence, validate_change,
};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

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
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
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
                               acceptance, business_value, time_criticality, risk_reduction,
                               job_size, meta)
         VALUES ($1, $2, $3, $4::float8::numeric, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
    /// The If-Match version (append-only rows keep 1).
    pub version: i64,
}

const BASELINE_COLUMNS: &str =
    "id, project_id, kind, snapshot, taken_at, taken_by, approved_by, version";

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
) -> Result<BaselineRecord, PlanStoreError> {
    require_project_tx(&mut *conn, tenant, project_id).await?;
    ensure_parent_tx(&mut *conn, tenant, Parent::AppUser, taken_by)
        .await
        .map_err(PlanStoreError::Parent)?;
    let wbs: Vec<WbsRecord> = list_wbs_tx(&mut *conn, tenant, project_id).await?;
    let register: Vec<RegisterRecord> = list_register_tx(&mut *conn, tenant, project_id).await?;
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
        "INSERT INTO project_baseline (tenant_id, project_id, kind, snapshot, taken_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {BASELINE_COLUMNS}"
    );
    let row: BaselineRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(kind)
        .bind(&snapshot)
        .bind(taken_by)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
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
    /// The owning project.
    pub project_id: Uuid,
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
    /// Free-form metadata (a change's impact block lives here).
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const REGISTER_COLUMNS: &str = "id, project_id, kind, code, title, body, owner_job_id, status, \
                                severity, probability, impact, links, baseline_ref, meta, version, \
                                created_at, updated_at";

/// A new register row.
#[derive(Clone, Debug, Default)]
pub struct NewRegisterItem {
    /// The owning project.
    pub project_id: Uuid,
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
    /// Metadata; a change's impact block lives here.
    pub meta: Value,
}

/// Insert a register row. A change is validated before the insert (D-T1-3).
pub async fn insert_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewRegisterItem,
) -> Result<RegisterRecord, PlanStoreError> {
    require_project_tx(&mut *conn, tenant, new.project_id).await?;
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
    let sql = format!(
        "INSERT INTO project_register_item
            (tenant_id, project_id, kind, code, title, body, owner_job_id, status, severity,
             probability, impact, links, baseline_ref, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING {REGISTER_COLUMNS}"
    );
    let row: RegisterRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.project_id)
        .bind(&new.kind)
        .bind(&new.code)
        .bind(&new.title)
        .bind(&new.body)
        .bind(new.owner_job_id)
        .bind(&new.status)
        .bind(&new.severity)
        .bind(&new.probability)
        .bind(new.impact)
        .bind(&new.links)
        .bind(new.baseline_ref)
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

/// List a project's register rows.
pub async fn list_register_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<RegisterRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {REGISTER_COLUMNS} FROM project_register_item
          WHERE tenant_id = $1 AND project_id = $2
          ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
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
    status: Option<&str>,
    title: Option<&str>,
    body: Option<&str>,
    severity: Option<&str>,
    impact: Option<i32>,
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
    let sql = format!(
        "UPDATE project_register_item SET
            status = coalesce($4, status), title = coalesce($5, title),
            body = coalesce($6, body), severity = coalesce($7, severity),
            impact = coalesce($8, impact), meta = coalesce($9, meta),
            version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {REGISTER_COLUMNS}"
    );
    let row: Option<RegisterRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected)
        .bind(status)
        .bind(title)
        .bind(body)
        .bind(severity)
        .bind(impact)
        .bind(meta)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(PlanStoreError::Stale {
        what: "register item",
    })
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

/// List the projects in the tenant's in-flight set.
pub async fn in_flight_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<InFlightRecord>, PlanStoreError> {
    let set = in_flight_set_tx(&mut *conn, tenant).await?;
    let sql = "SELECT id, key, name, status, programme_id, version FROM project
                WHERE tenant_id = $1 AND status = ANY($2)
                ORDER BY updated_at DESC, id DESC";
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&set)
        .fetch_all(&mut *conn)
        .await?)
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
    /// The If-Match version.
    pub version: i64,
}

const TRANCHE_COLUMNS: &str = "id, programme_id, number, state, entry_criteria, exit_criteria, baseline_ref, review_at, version";

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

/// List a programme's tranches.
pub async fn list_tranches_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<TrancheRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {TRANCHE_COLUMNS} FROM programme_tranche
          WHERE tenant_id = $1 AND programme_id = $2 ORDER BY number"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Record a tranche review decision, refusing an evidence-less review (D-T1-10).
pub async fn review_tranche_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    decision: &str,
    evidence: &Value,
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
    /// The If-Match version.
    pub version: i64,
}

const BENEFIT_COLUMNS: &str = "id, programme_id, project_id, register_item_id, reference, \
    description, category, objectives, observable_outcome, kpis, baseline, target, \
    measurement_method, financial_value, cost, capabilities, preconditions, risks, owner_job_id, \
    benefit_owner_job_id, operational_area, status, version";

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

/// List a programme's benefit profiles.
pub async fn list_benefits_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    programme_id: Uuid,
) -> Result<Vec<BenefitRecord>, PlanStoreError> {
    let sql = format!(
        "SELECT {BENEFIT_COLUMNS} FROM benefit_profile
          WHERE tenant_id = $1 AND programme_id = $2 ORDER BY created_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(programme_id)
        .fetch_all(&mut *conn)
        .await?)
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
    let sql = "INSERT INTO benefit_link
                 (tenant_id, programme_id, output_ref, capability_ref, operational_outcome,
                  benefit_id, strategic_objective, kind)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               RETURNING id, programme_id, output_ref, capability_ref, operational_outcome,
                         benefit_id, strategic_objective, kind, version";
    let row: BenefitLinkRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.programme_id)
        .bind(&new.output_ref)
        .bind(&new.capability_ref)
        .bind(&new.operational_outcome)
        .bind(new.benefit_id)
        .bind(&new.strategic_objective)
        .bind(&new.kind)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}
