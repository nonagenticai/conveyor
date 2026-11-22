//! The organisation spine's store: industries, the job catalogue, immutable releases, bindings,
//! the run's release pin, and the `organisation` section of the public snapshot
//! (SPEC_V2 §4, §10.3; T0, issue #34).
//!
//! The rules this module enforces (not merely stores):
//!
//! - a job's departments must belong to the job's industry, ordered contiguously from 1, with
//!   exactly one primary (§4.1, P1.3);
//! - a release is **compiled**, never authored: the snapshot is the job's catalogue definition
//!   plus its placement, hashed by [`conveyor_domain::organisation::release_hash`] — the same
//!   canonical form twice produces the same hash, and publishing the same content again returns
//!   the existing row (P2.1);
//! - the compile walks the knowledge layers **base → industry → department archetype → each
//!   department in declared order → job** (§4.1). `required` can only be tightened downward; a
//!   required kind with no resolvable value is [`OrganisationStoreError::ReleaseNotReady`], and
//!   sibling departments that set one kind to different values are
//!   [`OrganisationStoreError::DepartmentConflict`] unless a job-layer value resolves them. The
//!   snapshot records every effective value with the layer it came from; the job layer is the
//!   catalogue `definition`, so the anatomy is never duplicated;
//! - a binding pins one release and must satisfy its executor's contract (§4.4). The executor
//!   `Registry` is passed in, never looked up globally, so a test can register a third kind and
//!   prove it resolves without touching the run path (P6.2);
//! - a run resolves its binding here; a retired/draft binding or a requested release that is not
//!   the binding's pin is [`OrganisationStoreError::StaleRelease`], which the run store turns
//!   into `409 job_binding_stale` before the INSERT (P2.2).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::executor::{BindingConfig, BindingError, ExecutorKind, Registry};
use conveyor_domain::organisation::{BindingStatus, release_hash};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Why an organisation write or read failed.
#[derive(Debug, thiserror::Error)]
pub enum OrganisationStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// No such row in this tenant.
    #[error("the {0} does not exist in this tenant")]
    NotFound(&'static str),
    /// A placement names a department of another industry (or another tenant).
    #[error("every department of a job must belong to the job's industry")]
    DepartmentNotInIndustry,
    /// The ordered placement is not 1..=n, or does not name exactly one primary.
    #[error("the job's departments must be ordered 1..=n with exactly one primary")]
    InvalidPlacement,
    /// A kind the layers require has no resolvable value (SPEC_V2 §4.1: `409 release_not_ready`).
    #[error(
        "release_not_ready: the required kind {kind:?} has no resolvable value for department \
         {department:?}"
    )]
    ReleaseNotReady {
        /// The unresolved kind.
        kind: String,
        /// The department the requirement is attributed to.
        department: String,
    },
    /// Two sibling departments set one kind to different values and no job-layer value resolves
    /// them (SPEC_V2 §4.1: `422 department_conflict`).
    #[error(
        "department_conflict: department {left_department:?} sets {kind:?} to {left_value} while \
         department {right_department:?} sets it to {right_value}"
    )]
    DepartmentConflict {
        /// The contested kind.
        kind: String,
        /// The first department, in declared order.
        left_department: String,
        /// Its value (boxed: two inline JSON values push the error past clippy's
        /// `result_large_err` threshold, which every seed function returning it pays for).
        left_value: Box<Value>,
        /// The second department, in declared order.
        right_department: String,
        /// Its value, boxed for the same reason.
        right_value: Box<Value>,
    },
    /// The release does not belong to the job the binding names.
    #[error("the release does not belong to that job")]
    ReleaseJobMismatch,
    /// A mutable row moved underneath the caller.
    #[error("the {0} moved; the caller's version is stale")]
    VersionMismatch(&'static str),
    /// The binding's executor contract refused it.
    #[error(transparent)]
    Executor(#[from] BindingError),
    /// A stored status the domain does not know.
    #[error("the database holds status {0:?}, which the domain does not know")]
    UnknownStatus(String),
    /// The run named a retired (or draft) binding, or a release that is not its pin.
    #[error(
        "job_binding_stale: the binding pins release {binding_hash:?} but the run requested \
         {requested_hash:?}"
    )]
    StaleRelease {
        /// The release hash the binding pins.
        binding_hash: String,
        /// The hash the caller named (the binding's own when the caller named none).
        requested_hash: String,
    },
}

/// One industry.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct IndustryRow {
    /// Row id.
    pub id: Uuid,
    /// Stable slug (`software`).
    pub key: String,
    /// Display name.
    pub name: String,
    /// `active | retired`.
    pub status: String,
    /// Free metadata.
    pub meta: Value,
    /// Optimistic-concurrency version.
    pub version: i64,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One catalogue position.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct JobRow {
    /// Row id.
    pub id: Uuid,
    /// Stable job key (`backend_engineer`).
    pub job_key: String,
    /// The industry the job belongs to.
    pub industry_id: Uuid,
    /// The job family (`engineering`).
    pub family: String,
    /// Display title.
    pub title: String,
    /// `draft | active | retired`.
    pub status: String,
    /// The editable draft anatomy (§4.2).
    pub definition: Value,
    /// Free metadata.
    pub meta: Value,
    /// Optimistic-concurrency version.
    pub version: i64,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One ordered placement of a job in a department.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PlacementRow {
    /// The job.
    pub job_id: Uuid,
    /// The department.
    pub department_id: Uuid,
    /// The department's display name.
    pub department_name: String,
    /// The department's archetype layer, when it declares one (§4.1).
    pub archetype: Option<String>,
    /// 1-based order inside the job.
    pub ordinal: i32,
    /// The job's primary department.
    pub is_primary: bool,
}

/// One immutable compiled release.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ReleaseRow {
    /// Row id.
    pub id: Uuid,
    /// The job it compiles.
    pub job_id: Uuid,
    /// SHA-256 of the canonical snapshot.
    pub release_hash: String,
    /// The frozen snapshot.
    pub release_snapshot: Value,
    /// `published | retired`.
    pub status: String,
    /// Bookkeeping version (the snapshot itself never changes).
    pub version: i64,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
}

/// One binding: a job × release pin × executor.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BindingRow {
    /// Row id.
    pub id: Uuid,
    /// The job.
    pub job_id: Uuid,
    /// The pinned release row.
    pub release_id: Uuid,
    /// The pinned release hash.
    pub release_hash: String,
    /// The frozen release snapshot copied onto the binding.
    pub release_snapshot: Value,
    /// The capability, when the binding narrows one.
    pub capability_id: Option<Uuid>,
    /// The executable process (`procedure` executor).
    pub process_id: Option<Uuid>,
    /// The agent configuration (`internal_agent` executor).
    pub capability_agent_id: Option<Uuid>,
    /// The executor kind slug.
    pub executor_kind: String,
    /// `draft | active | retired`.
    pub status: String,
    /// `department_scope`, authority, limits override.
    pub meta: Value,
    /// Optimistic-concurrency version.
    pub version: i64,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

const INDUSTRY_COLUMNS: &str = "id, key, name, status, meta, version, created_at, updated_at";
const JOB_COLUMNS: &str = "id, job_key, industry_id, family, title, status, definition, meta, \
                           version, created_at, updated_at";
const RELEASE_COLUMNS: &str =
    "id, job_id, release_hash, release_snapshot, status, version, created_at";
const BINDING_COLUMNS: &str = "id, job_id, release_id, release_hash, release_snapshot, \
                               capability_id, process_id, capability_agent_id, executor_kind, \
                               status, meta, version, created_at, updated_at";
const PLACEMENT_COLUMNS: &str = "jd.job_id, jd.department_id, d.name AS department_name, d.archetype, jd.ordinal, \
     jd.is_primary";

/// One industry to create.
#[derive(Clone, Debug)]
pub struct NewIndustry {
    /// Stable slug.
    pub key: String,
    /// Display name.
    pub name: String,
    /// Free metadata.
    pub meta: Value,
}

/// One job to create.
#[derive(Clone, Debug)]
pub struct NewJob {
    /// Stable job key.
    pub job_key: String,
    /// The industry (must resolve in this tenant).
    pub industry_id: Uuid,
    /// The job family.
    pub family: String,
    /// Display title.
    pub title: String,
    /// `draft | active`.
    pub status: String,
    /// The draft anatomy.
    pub definition: Value,
    /// Free metadata.
    pub meta: Value,
    /// The ordered placement, 1..=n, exactly one primary.
    pub departments: Vec<PlacementIn>,
}

/// One row of the ordered placement a create/update carries.
#[derive(Clone, Copy, Debug)]
pub struct PlacementIn {
    /// The department (must belong to the job's industry).
    pub department_id: Uuid,
    /// 1-based order.
    pub ordinal: i32,
    /// Whether this is the job's primary department.
    pub is_primary: bool,
}

/// The mutable half of a job update.
#[derive(Clone, Debug, Default)]
pub struct JobPatch {
    /// New title.
    pub title: Option<String>,
    /// New family.
    pub family: Option<String>,
    /// New draft anatomy.
    pub definition: Option<Value>,
    /// New metadata.
    pub meta: Option<Value>,
    /// A full replacement placement; `None` keeps the current rows.
    pub departments: Option<Vec<PlacementIn>>,
}

/// One binding to create.
#[derive(Clone, Debug)]
pub struct NewBinding {
    /// The job.
    pub job_id: Uuid,
    /// The release to pin (must belong to the job).
    pub release_id: Uuid,
    /// The executor kind.
    pub executor_kind: ExecutorKind,
    /// The executable process, for a `procedure` binding.
    pub process_id: Option<Uuid>,
    /// The agent configuration, for an `internal_agent` binding.
    pub capability_agent_id: Option<Uuid>,
    /// An optional capability the binding narrows.
    pub capability_id: Option<Uuid>,
    /// `department_scope`, authority, limits override.
    pub meta: Value,
    /// The status to write (`active` for a usable pin).
    pub status: BindingStatus,
}

// ── industries ────────────────────────────────────────────────────────────────────────────

/// Insert one industry.
pub async fn industry_create_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewIndustry,
) -> Result<IndustryRow, OrganisationStoreError> {
    let sql = format!(
        "INSERT INTO industry (tenant_id, key, name, meta)
         VALUES ($1, $2, $3, $4)
         RETURNING {INDUSTRY_COLUMNS}"
    );
    let row = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.key)
        .bind(&input.name)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one industry.
pub async fn industry_get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<IndustryRow, OrganisationStoreError> {
    let sql = format!("SELECT {INDUSTRY_COLUMNS} FROM industry WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::NotFound("industry"))
}

/// Every industry of the tenant, by key.
pub async fn industries_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<IndustryRow>, sqlx::Error> {
    let sql =
        format!("SELECT {INDUSTRY_COLUMNS} FROM industry WHERE tenant_id = $1 ORDER BY key, id");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await
}

// ── job catalogue ─────────────────────────────────────────────────────────────────────────

/// Insert one job and its ordered placement.
pub async fn job_create_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewJob,
) -> Result<JobRow, OrganisationStoreError> {
    // The industry must resolve in THIS tenant: the FK bypasses RLS.
    let industry: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM industry WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(input.industry_id)
            .fetch_optional(&mut *conn)
            .await?;
    if industry.is_none() {
        return Err(OrganisationStoreError::NotFound("industry"));
    }
    ensure_placement_tx(conn, tenant, input.industry_id, &input.departments).await?;

    let sql = format!(
        "INSERT INTO job_catalogue
                (tenant_id, job_key, industry_id, family, title, status, definition, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {JOB_COLUMNS}"
    );
    let row: JobRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.job_key)
        .bind(input.industry_id)
        .bind(&input.family)
        .bind(&input.title)
        .bind(&input.status)
        .bind(&input.definition)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    replace_placement_tx(conn, tenant, row.id, &input.departments).await?;
    Ok(row)
}

/// Update a job's mutable half, with an optimistic-concurrency precondition.
pub async fn job_update_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: i64,
    patch: &JobPatch,
) -> Result<JobRow, OrganisationStoreError> {
    let current = job_get_tx(conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(OrganisationStoreError::VersionMismatch("job"));
    }
    if let Some(departments) = &patch.departments {
        ensure_placement_tx(conn, tenant, current.industry_id, departments).await?;
        replace_placement_tx(conn, tenant, id, departments).await?;
    }
    let sql = format!(
        "UPDATE job_catalogue
            SET title = COALESCE($3, title),
                family = COALESCE($4, family),
                definition = COALESCE($5, definition),
                meta = COALESCE($6, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $7
          RETURNING {JOB_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.title.as_deref())
        .bind(patch.family.as_deref())
        .bind(patch.definition.as_ref())
        .bind(patch.meta.as_ref())
        .bind(expected_version)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::VersionMismatch("job"))
}

/// Retire a job (it stays readable; new releases refuse it).
pub async fn job_retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: i64,
) -> Result<JobRow, OrganisationStoreError> {
    let sql = format!(
        "UPDATE job_catalogue
            SET status = 'retired', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {JOB_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected_version)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::VersionMismatch("job"))
}

/// Read one job.
pub async fn job_get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<JobRow, OrganisationStoreError> {
    let sql = format!("SELECT {JOB_COLUMNS} FROM job_catalogue WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::NotFound("job"))
}

/// The tenant's jobs, by key.
pub async fn jobs_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<JobRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {JOB_COLUMNS} FROM job_catalogue WHERE tenant_id = $1 ORDER BY job_key, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await
}

/// The ordered placement of one job.
pub async fn job_placements_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    job_id: Uuid,
) -> Result<Vec<PlacementRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PLACEMENT_COLUMNS}
           FROM job_department jd
           JOIN department d ON d.id = jd.department_id AND d.tenant_id = jd.tenant_id
          WHERE jd.tenant_id = $1 AND jd.job_id = $2
          ORDER BY jd.ordinal"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(job_id)
        .fetch_all(&mut *conn)
        .await
}

/// The rules a placement must satisfy: 1..=n, exactly one primary, and the primary is FIRST
/// (SPEC_V2 §4.1: an industry's departments are ordered, primary first).
fn placement_is_contiguous(departments: &[PlacementIn]) -> bool {
    if departments.is_empty() {
        return false;
    }
    let mut ordinals: Vec<i32> = departments.iter().map(|row| row.ordinal).collect();
    ordinals.sort_unstable();
    let numbered = ordinals
        .iter()
        .enumerate()
        .all(|(index, ordinal)| *ordinal == i32::try_from(index).unwrap_or(i32::MAX) + 1);
    let primaries: Vec<&PlacementIn> = departments.iter().filter(|row| row.is_primary).collect();
    numbered && primaries.len() == 1 && primaries[0].ordinal == 1
}

async fn ensure_placement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    industry_id: Uuid,
    departments: &[PlacementIn],
) -> Result<(), OrganisationStoreError> {
    if !placement_is_contiguous(departments) {
        return Err(OrganisationStoreError::InvalidPlacement);
    }
    for placement in departments {
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM department
                  WHERE tenant_id = $1 AND id = $2 AND industry_id = $3
             )",
        )
        .bind(tenant.as_uuid())
        .bind(placement.department_id)
        .bind(industry_id)
        .fetch_one(&mut *conn)
        .await?;
        if !present {
            return Err(OrganisationStoreError::DepartmentNotInIndustry);
        }
    }
    Ok(())
}

async fn replace_placement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    job_id: Uuid,
    departments: &[PlacementIn],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM job_department WHERE tenant_id = $1 AND job_id = $2")
        .bind(tenant.as_uuid())
        .bind(job_id)
        .execute(&mut *conn)
        .await?;
    for placement in departments {
        sqlx::query(
            "INSERT INTO job_department (tenant_id, job_id, department_id, ordinal, is_primary)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(tenant.as_uuid())
        .bind(job_id)
        .bind(placement.department_id)
        .bind(placement.ordinal)
        .bind(placement.is_primary)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

// ── releases ──────────────────────────────────────────────────────────────────────────────

/// One layer of the compile chain (SPEC_V2 §4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    /// The tenant's platform defaults.
    Base,
    /// The job's industry.
    Industry,
    /// A department archetype a placed department inherits.
    DepartmentArchetype,
    /// One placed department.
    Department,
    /// The job itself (the catalogue `definition`, plus any explicit job-layer row).
    Job,
}

/// One kind declaration in the compile chain, with the source the snapshot records.
#[derive(Clone, Debug)]
struct LayerDeclaration {
    layer: Layer,
    kind: String,
    value: Option<Value>,
    required: bool,
    source: Value,
    department_name: Option<String>,
}

/// One effective value the release snapshot records.
#[derive(Clone, Debug)]
struct ResolvedLayerValue {
    value: Value,
    required: bool,
    source: Value,
}

/// One layer's declaration row, as the compile reads it.
#[derive(Debug, sqlx::FromRow)]
struct LayerSettingRow {
    kind: String,
    value: Option<Value>,
    required: bool,
}

/// Read one layer's rows for the scope named by the non-null filters.
async fn layer_settings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    layer: &str,
    industry_id: Option<Uuid>,
    department_id: Option<Uuid>,
    job_id: Option<Uuid>,
    archetype: Option<&str>,
) -> Result<Vec<LayerSettingRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT kind, value, required
           FROM organisation_layer_setting
          WHERE tenant_id = $1 AND layer = $2
            AND ($3::uuid IS NULL OR industry_id = $3)
            AND ($4::uuid IS NULL OR department_id = $4)
            AND ($5::uuid IS NULL OR job_id = $5)
            AND ($6::text IS NULL OR archetype = $6)
          ORDER BY kind",
    )
    .bind(tenant.as_uuid())
    .bind(layer)
    .bind(industry_id)
    .bind(department_id)
    .bind(job_id)
    .bind(archetype)
    .fetch_all(&mut *conn)
    .await
}

/// Resolve one effective value per kind, or refuse.
///
/// A later layer overrides an earlier one. Sibling departments are peers rather than an order,
/// so two different values among them are a [`OrganisationStoreError::DepartmentConflict`] unless
/// a job-layer value resolves the kind (§4.1). `required` is the OR of every declaration in the
/// chain: a child layer can add a requirement and can never remove its parent's.
fn resolve_layer_values(
    chain: &[LayerDeclaration],
    primary_department: &str,
) -> Result<BTreeMap<String, ResolvedLayerValue>, OrganisationStoreError> {
    let kinds: BTreeSet<&str> = chain
        .iter()
        .map(|declaration| declaration.kind.as_str())
        .collect();
    let mut resolved = BTreeMap::new();
    for kind in kinds {
        let declarations: Vec<&LayerDeclaration> = chain
            .iter()
            .filter(|declaration| declaration.kind == kind)
            .collect();
        let required = declarations.iter().any(|declaration| declaration.required);
        let job_value = declarations
            .iter()
            .rev()
            .find(|declaration| declaration.layer == Layer::Job && declaration.value.is_some());

        // Sibling departments at different values: only a job-layer value resolves the kind.
        let mut sibling_values: Vec<(&str, &Value)> = Vec::new();
        for declaration in &declarations {
            if declaration.layer != Layer::Department {
                continue;
            }
            let Some(value) = declaration.value.as_ref() else {
                continue;
            };
            if sibling_values
                .iter()
                .any(|(_, existing)| *existing == value)
            {
                continue;
            }
            sibling_values.push((
                declaration
                    .department_name
                    .as_deref()
                    .unwrap_or(primary_department),
                value,
            ));
        }
        if sibling_values.len() > 1 && job_value.is_none() {
            let (left_department, left_value) = sibling_values[0];
            let (right_department, right_value) = sibling_values[1];
            return Err(OrganisationStoreError::DepartmentConflict {
                kind: kind.to_owned(),
                left_department: left_department.to_owned(),
                left_value: Box::new(left_value.clone()),
                right_department: right_department.to_owned(),
                right_value: Box::new(right_value.clone()),
            });
        }

        let Some(effective) = declarations
            .iter()
            .rev()
            .find(|declaration| declaration.value.is_some())
        else {
            if required {
                // Name the department the requirement is attributed to: the declaring one when
                // the layer is a department or archetype, else the job's primary department.
                let department = declarations
                    .iter()
                    .find(|declaration| declaration.required)
                    .and_then(|declaration| declaration.department_name.clone())
                    .unwrap_or_else(|| primary_department.to_owned());
                return Err(OrganisationStoreError::ReleaseNotReady {
                    kind: kind.to_owned(),
                    department,
                });
            }
            continue;
        };
        resolved.insert(
            kind.to_owned(),
            ResolvedLayerValue {
                value: effective.value.clone().unwrap_or(Value::Null),
                required,
                source: effective.source.clone(),
            },
        );
    }
    Ok(resolved)
}

/// Compile the job's release snapshot: the catalogue definition, its placement, and every
/// effective layer value with its source.
///
/// The compile walks **base → industry → department archetype → each department in declared
/// order → job** (§4.1). Deterministic by construction — no timestamp, no id, the placement is
/// ordered and the resolved values are keyed by kind — so the same job and pins hash identically
/// on every compile (P2.1). The `anatomy` key carries the design's job anatomy verbatim (§4.2),
/// which is what P4.2 asserts.
pub async fn compile_release_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    job_id: Uuid,
) -> Result<Value, OrganisationStoreError> {
    let job = job_get_tx(conn, tenant, job_id).await?;
    let industry = industry_get_tx(conn, tenant, job.industry_id).await?;
    let placements = job_placements_tx(conn, tenant, job_id).await?;

    let mut chain: Vec<LayerDeclaration> = Vec::new();

    // base
    let source = json!({"layer": "base"});
    for row in layer_settings_tx(conn, tenant, "base", None, None, None, None).await? {
        chain.push(LayerDeclaration {
            layer: Layer::Base,
            kind: row.kind,
            value: row.value.filter(|value| !value.is_null()),
            required: row.required,
            source: source.clone(),
            department_name: None,
        });
    }

    // industry
    let source = json!({"layer": "industry", "industry": industry.key.clone()});
    for row in layer_settings_tx(
        conn,
        tenant,
        "industry",
        Some(job.industry_id),
        None,
        None,
        None,
    )
    .await?
    {
        chain.push(LayerDeclaration {
            layer: Layer::Industry,
            kind: row.kind,
            value: row.value.filter(|value| !value.is_null()),
            required: row.required,
            source: source.clone(),
            department_name: None,
        });
    }

    // department archetypes, in declared order, each archetype once
    let mut seen_archetypes: Vec<&str> = Vec::new();
    for placement in &placements {
        let Some(archetype) = placement.archetype.as_deref() else {
            continue;
        };
        if seen_archetypes.contains(&archetype) {
            continue;
        }
        seen_archetypes.push(archetype);
        let source = json!({
            "layer": "department_archetype",
            "industry": industry.key.clone(),
            "archetype": archetype,
        });
        let rows = layer_settings_tx(
            conn,
            tenant,
            "department_archetype",
            Some(job.industry_id),
            None,
            None,
            Some(archetype),
        )
        .await?;
        for row in rows {
            chain.push(LayerDeclaration {
                layer: Layer::DepartmentArchetype,
                kind: row.kind,
                value: row.value.filter(|value| !value.is_null()),
                required: row.required,
                source: source.clone(),
                // The first placed department of this archetype: what `release_not_ready`
                // names when the archetype layer requires a kind nobody resolves.
                department_name: Some(placement.department_name.clone()),
            });
        }
    }

    // each department in declared order
    for placement in &placements {
        let source = json!({
            "layer": "department",
            "department": placement.department_name.clone(),
            "ordinal": placement.ordinal,
        });
        let rows = layer_settings_tx(
            conn,
            tenant,
            "department",
            None,
            Some(placement.department_id),
            None,
            None,
        )
        .await?;
        for row in rows {
            chain.push(LayerDeclaration {
                layer: Layer::Department,
                kind: row.kind,
                value: row.value.filter(|value| !value.is_null()),
                required: row.required,
                source: source.clone(),
                department_name: Some(placement.department_name.clone()),
            });
        }
    }

    // job: the catalogue definition is the authored layer; an explicit job row overrides it
    let source = json!({"layer": "job", "job": job.job_key.clone()});
    if let Some(definition) = job.definition.as_object() {
        for (kind, value) in definition {
            if value.is_null() {
                continue;
            }
            chain.push(LayerDeclaration {
                layer: Layer::Job,
                kind: kind.clone(),
                value: Some(value.clone()),
                required: false,
                source: source.clone(),
                department_name: None,
            });
        }
    }
    for row in layer_settings_tx(conn, tenant, "job", None, None, Some(job_id), None).await? {
        chain.push(LayerDeclaration {
            layer: Layer::Job,
            kind: row.kind,
            value: row.value.filter(|value| !value.is_null()),
            required: row.required,
            source: source.clone(),
            department_name: None,
        });
    }

    let primary_department = placements
        .iter()
        .find(|placement| placement.is_primary)
        .or_else(|| placements.first())
        .map(|placement| placement.department_name.clone())
        .unwrap_or_default();
    let resolved = resolve_layer_values(&chain, &primary_department)?;

    let departments: Vec<Value> = placements
        .iter()
        .map(|row| {
            json!({
                "name": row.department_name,
                "ordinal": row.ordinal,
                "is_primary": row.is_primary,
            })
        })
        .collect();
    let layer_values: Vec<Value> = resolved
        .iter()
        .map(|(kind, value)| {
            json!({
                "kind": kind,
                "value": value.value.clone(),
                "required": value.required,
                "source": value.source.clone(),
            })
        })
        .collect();
    Ok(json!({
        "job_key": job.job_key,
        "title": job.title,
        "family": job.family,
        "industry": {"key": industry.key, "name": industry.name},
        "departments": departments,
        "anatomy": job.definition,
        "layer_values": layer_values,
    }))
}

/// Publish the job's current definition as an immutable release.
///
/// Idempotent by content: publishing unchanged content returns the existing row rather than
/// minting a duplicate (the unique key is `(tenant, job, hash)`).
pub async fn job_release_publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    job_id: Uuid,
) -> Result<ReleaseRow, OrganisationStoreError> {
    let job = job_get_tx(conn, tenant, job_id).await?;
    if job.status == "retired" {
        return Err(OrganisationStoreError::NotFound("live job"));
    }
    let snapshot = compile_release_tx(conn, tenant, job_id).await?;
    let hash = release_hash(&snapshot);
    let sql = format!(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, job_id, release_hash) DO NOTHING
         RETURNING {RELEASE_COLUMNS}"
    );
    let inserted: Option<ReleaseRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(job_id)
        .bind(&hash)
        .bind(&snapshot)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = inserted {
        return Ok(row);
    }
    let sql = format!(
        "SELECT {RELEASE_COLUMNS} FROM job_release
          WHERE tenant_id = $1 AND job_id = $2 AND release_hash = $3"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(job_id)
        .bind(&hash)
        .fetch_one(&mut *conn)
        .await
        .map_err(OrganisationStoreError::Database)
}

/// Retire a release. The frozen snapshot never changes (grants withhold UPDATE on it).
pub async fn job_release_retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ReleaseRow, OrganisationStoreError> {
    let sql = format!(
        "UPDATE job_release
            SET status = 'retired', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {RELEASE_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::NotFound("release"))
}

/// One release.
pub async fn job_release_get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ReleaseRow, OrganisationStoreError> {
    let sql = format!("SELECT {RELEASE_COLUMNS} FROM job_release WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::NotFound("release"))
}

/// A job's releases, newest first.
pub async fn job_releases_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    job_id: Uuid,
) -> Result<Vec<ReleaseRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {RELEASE_COLUMNS} FROM job_release
          WHERE tenant_id = $1 AND job_id = $2
          ORDER BY created_at DESC, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(job_id)
        .fetch_all(&mut *conn)
        .await
}

// ── bindings ──────────────────────────────────────────────────────────────────────────────

/// Create a binding: pin the release and satisfy the executor contract.
pub async fn job_binding_create_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewBinding,
    registry: &Registry,
) -> Result<BindingRow, OrganisationStoreError> {
    let release = job_release_get_tx(conn, tenant, input.release_id).await?;
    if release.job_id != input.job_id {
        return Err(OrganisationStoreError::ReleaseJobMismatch);
    }
    registry.validate(&BindingConfig {
        executor_kind: input.executor_kind.clone(),
        process_id: input.process_id,
        capability_agent_id: input.capability_agent_id,
    })?;
    ensure_executor_refs_tx(conn, tenant, input).await?;

    let sql = format!(
        "INSERT INTO job_binding
                (tenant_id, job_id, release_id, release_hash, release_snapshot,
                 capability_id, process_id, capability_agent_id, executor_kind, status, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING {BINDING_COLUMNS}"
    );
    let row = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.job_id)
        .bind(release.id)
        .bind(&release.release_hash)
        .bind(&release.release_snapshot)
        .bind(input.capability_id)
        .bind(input.process_id)
        .bind(input.capability_agent_id)
        .bind(input.executor_kind.as_str())
        .bind(input.status.as_str())
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// The FK checks the constraints cannot make tenant-safe on their own.
async fn ensure_executor_refs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewBinding,
) -> Result<(), OrganisationStoreError> {
    for (id, table, label) in [
        (input.process_id, "process", "process"),
        (
            input.capability_agent_id,
            "capability_agent",
            "capability agent",
        ),
        (input.capability_id, "capability", "capability"),
    ] {
        let Some(id) = id else { continue };
        let sql = format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE tenant_id = $1 AND id = $2)");
        let present: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        if !present {
            return Err(OrganisationStoreError::NotFound(label));
        }
    }
    Ok(())
}

/// Retire a binding.
pub async fn job_binding_retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: i64,
) -> Result<BindingRow, OrganisationStoreError> {
    let sql = format!(
        "UPDATE job_binding
            SET status = 'retired', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING {BINDING_COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(expected_version)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::VersionMismatch("binding"))
}

/// One binding.
pub async fn job_binding_get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<BindingRow, OrganisationStoreError> {
    let sql = format!("SELECT {BINDING_COLUMNS} FROM job_binding WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(OrganisationStoreError::NotFound("binding"))
}

/// A job's bindings, newest first.
pub async fn job_bindings_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    job_id: Uuid,
) -> Result<Vec<BindingRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BINDING_COLUMNS} FROM job_binding
          WHERE tenant_id = $1 AND job_id = $2
          ORDER BY created_at DESC, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(job_id)
        .fetch_all(&mut *conn)
        .await
}

/// The pin a run gets from a binding (P2.2).
///
/// Refuses a binding that is not `active` and a `requested_hash` that is not the binding's pin,
/// naming both hashes. The caller (the run store) turns this into
/// `RunStoreError::JobBindingStale` **before** the run INSERT.
pub async fn resolve_binding_pin_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    binding_id: Uuid,
    requested_hash: Option<&str>,
) -> Result<(Uuid, String), OrganisationStoreError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT status, release_hash FROM job_binding WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(binding_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (status, binding_hash) = row.ok_or(OrganisationStoreError::NotFound("binding"))?;
    let status: BindingStatus = status
        .parse()
        .map_err(OrganisationStoreError::UnknownStatus)?;
    let requested = requested_hash.unwrap_or(&binding_hash).to_owned();
    if !status.is_usable_for_new_runs() || requested != binding_hash {
        return Err(OrganisationStoreError::StaleRelease {
            binding_hash,
            requested_hash: requested,
        });
    }
    Ok((binding_id, binding_hash))
}

// ── the public snapshot section ───────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct DepartmentSectionRow {
    id: Uuid,
    name: String,
    industry_id: Option<Uuid>,
    ordinal: Option<i32>,
    is_primary: bool,
}

/// The `organisation` section of `design-snapshot/v1` (P5.1).
///
/// Structural rows only: no secret values, no emails. The job rows carry the keys the design
/// site already reads (`docs/design/v2.js` `F.jobs`) — the latest release's `anatomy` flattened
/// onto the row — so the site flips from fixtures to live without a rewrite.
pub async fn organisation_json_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Value, sqlx::Error> {
    let industries: Vec<IndustryRow> = industries_list_tx(&mut *conn, tenant).await?;
    let departments: Vec<DepartmentSectionRow> = sqlx::query_as(
        "SELECT id, name, industry_id, ordinal, is_primary
           FROM department
          WHERE tenant_id = $1
          ORDER BY industry_id NULLS LAST, ordinal NULLS LAST, name, id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;
    let jobs: Vec<JobRow> = jobs_list_tx(&mut *conn, tenant).await?;
    let releases: Vec<ReleaseRow> = {
        let sql = format!(
            "SELECT {RELEASE_COLUMNS} FROM job_release
              WHERE tenant_id = $1
              ORDER BY job_id, created_at DESC, id"
        );
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .fetch_all(&mut *conn)
            .await?
    };
    let bindings: Vec<BindingRow> = {
        let sql = format!(
            "SELECT {BINDING_COLUMNS} FROM job_binding
              WHERE tenant_id = $1
              ORDER BY job_id, created_at DESC, id"
        );
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .fetch_all(&mut *conn)
            .await?
    };

    let mut job_rows = Vec::with_capacity(jobs.len());
    for job in &jobs {
        let placements = job_placements_tx(&mut *conn, tenant, job.id).await?;
        let latest_release = releases.iter().find(|row| row.job_id == job.id);
        let active_binding = bindings
            .iter()
            .find(|row| row.job_id == job.id && row.status == "active");
        let mut row = json!({
            "id": job.id,
            "key": job.job_key,
            "name": job.title,
            "family": job.family,
            "status": job.status,
            "industry_id": job.industry_id,
            "department_ids": placements.iter().map(|p| p.department_id).collect::<Vec<_>>(),
            "department_names": placements.iter().map(|p| p.department_name.clone()).collect::<Vec<_>>(),
            "release_hash": latest_release.map(|release| release.release_hash.clone()),
        });
        if let (Some(release), Some(object)) = (latest_release, row.as_object_mut())
            && let Some(anatomy) = release
                .release_snapshot
                .get("anatomy")
                .and_then(Value::as_object)
        {
            for (key, value) in anatomy {
                object.insert(key.clone(), value.clone());
            }
        }
        // The live binding is the authority on the executor kind; without one the release's
        // declared intent (anatomy.executor_kind) is what the page shows.
        if let (Some(binding), Some(object)) = (active_binding, row.as_object_mut()) {
            object.insert(
                "executor_kind".to_owned(),
                Value::String(binding.executor_kind.clone()),
            );
        }
        job_rows.push(row);
    }

    let job_keys: std::collections::BTreeMap<Uuid, &str> = jobs
        .iter()
        .map(|job| (job.id, job.job_key.as_str()))
        .collect();

    Ok(json!({
        "industries": industries.iter().map(|row| json!({
            "id": row.id, "key": row.key, "name": row.name, "status": row.status,
        })).collect::<Vec<_>>(),
        "departments": departments.iter().map(|row| json!({
            "id": row.id,
            "name": row.name,
            "industry_id": row.industry_id,
            "ordinal": row.ordinal,
            "is_primary": row.is_primary,
        })).collect::<Vec<_>>(),
        "jobs": job_rows,
        "releases": releases.iter().map(|row| json!({
            "id": row.id,
            "job_id": row.job_id,
            "job_key": job_keys.get(&row.job_id),
            "release_hash": row.release_hash,
            "status": row.status,
            "version": row.version,
            "created_at": row.created_at,
        })).collect::<Vec<_>>(),
        "bindings": bindings.iter().map(|row| json!({
            "id": row.id,
            "job_id": row.job_id,
            "job_key": job_keys.get(&row.job_id),
            "release_hash": row.release_hash,
            "executor_kind": row.executor_kind,
            "status": row.status,
            "version": row.version,
            "process_id": row.process_id,
            "capability_agent_id": row.capability_agent_id,
            "capability_id": row.capability_id,
        })).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(ordinal: i32, primary: bool) -> PlacementIn {
        PlacementIn {
            department_id: Uuid::nil(),
            ordinal,
            is_primary: primary,
        }
    }

    #[test]
    fn a_contiguous_placement_needs_one_primary() {
        assert!(placement_is_contiguous(&[
            placement(1, true),
            placement(2, false)
        ]));
        assert!(!placement_is_contiguous(&[placement(1, false)]));
        assert!(!placement_is_contiguous(&[
            placement(1, true),
            placement(3, false)
        ]));
        assert!(!placement_is_contiguous(&[
            placement(2, true),
            placement(1, false)
        ]));
        assert!(!placement_is_contiguous(&[]));
    }

    #[test]
    fn the_compiled_snapshot_is_deterministic() {
        // The compile path is DB-backed; the pure property is that the canonical hash of the
        // same JSON is stable and includes the anatomy verbatim.
        let snapshot = json!({
            "job_key": "backend_engineer",
            "title": "Backend engineer",
            "family": "engineering",
            "industry": {"key": "software", "name": "Software"},
            "departments": [{"name": "delivery", "ordinal": 1, "is_primary": true}],
            "anatomy": {"charter": "Build the service", "limits": {"max_iterations": 3}},
        });
        assert_eq!(release_hash(&snapshot), release_hash(&snapshot.clone()));
        assert_eq!(
            snapshot.get("anatomy").and_then(|a| a.get("charter")),
            Some(&json!("Build the service"))
        );
    }
}
