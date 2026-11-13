//! The organisation spine's actions (SPEC_V2 §4.1–§4.4, §10.4; T0, issue #34 P3).
//!
//! Five families, one declaration each — HTTP, MCP and OpenAPI are projections of the same
//! `#[crate::action]`:
//!
//! - `industries.*` — the top of the spine;
//! - `departments.*` — extended in `handlers.rs` with the industry placement;
//! - `job_catalogue.*` — the position, its draft anatomy and its ordered departments;
//! - `job_releases.*` — publish compiles and hashes; a release is never edited;
//! - `job_bindings.*` — pin one release to one executor; the registry refuses an unserved kind.
//!
//! Writes are `organisation:write` (admin/kata_executor); reads are `organisation:read` (open).
//! No action here writes a raw SQL update: the store functions own the invariants and the
//! optimistic-concurrency preconditions.

use conveyor_domain::executor::{BindingError, ExecutorKind, Registry};
use conveyor_domain::organisation::BindingStatus;
use conveyor_store as store;
use conveyor_store::organisation::OrganisationStoreError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{invalid, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::EmptyIn;

// ── views ────────────────────────────────────────────────────────────────────────────────

/// Read one row by id.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OrganisationIdIn {
    /// The row id, resolved under the caller's tenant.
    pub id: Uuid,
}

/// One industry.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IndustryOut {
    /// Row id.
    pub id: String,
    /// Stable slug.
    pub key: String,
    /// Display name.
    pub name: String,
    /// `active | retired`.
    pub status: String,
    /// Free metadata.
    pub meta: Value,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// `industries.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IndustryCreateIn {
    /// Stable slug (`^[a-z][a-z0-9_-]{1,63}$`).
    pub key: String,
    /// Display name.
    pub name: String,
    /// Free metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// One ordered placement of a job in a department.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobDepartmentOut {
    /// The department id.
    pub department_id: String,
    /// The department's display name.
    pub name: String,
    /// 1-based order.
    pub ordinal: i32,
    /// The job's primary department.
    pub is_primary: bool,
}

/// One catalogue position.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobOut {
    /// Row id.
    pub id: String,
    /// Stable job key.
    pub key: String,
    /// The industry.
    pub industry_id: String,
    /// The job family.
    pub family: String,
    /// Display title.
    pub title: String,
    /// `draft | active | retired`.
    pub status: String,
    /// The editable draft anatomy.
    pub definition: Value,
    /// Free metadata.
    pub meta: Value,
    /// The ordered placement.
    pub departments: Vec<JobDepartmentOut>,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One placement in a create/update input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobDepartmentIn {
    /// The department (must belong to the job's industry).
    pub department_id: Uuid,
    /// 1-based order; the set must be 1..=n.
    pub ordinal: i32,
    /// Exactly one placement is primary.
    pub is_primary: bool,
}

/// `job_catalogue.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobCreateIn {
    /// Stable job key (`^[a-z][a-z0-9_]{1,63}$`).
    pub job_key: String,
    /// The industry the job belongs to.
    pub industry_id: Uuid,
    /// The job family (`engineering`).
    #[serde(default)]
    pub family: String,
    /// Display title.
    pub title: String,
    /// The draft anatomy (§4.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
    /// Free metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// The ordered placement, 1..=n, exactly one primary.
    pub departments: Vec<JobDepartmentIn>,
}

/// `job_catalogue.update` — the mutable half, with an If-Match style version.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobUpdateIn {
    /// The job id.
    pub id: Uuid,
    /// The version the caller last saw.
    pub version: i64,
    /// New title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// New family.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// New draft anatomy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
    /// New metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// A replacement placement; omit to keep the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub departments: Option<Vec<JobDepartmentIn>>,
}

/// `job_catalogue.retire`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobRetireIn {
    /// The job id.
    pub id: Uuid,
    /// The version the caller last saw.
    pub version: i64,
}

/// One immutable release.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseOut {
    /// Row id.
    pub id: String,
    /// The job it compiles.
    pub job_id: String,
    /// SHA-256 of the canonical snapshot.
    pub release_hash: String,
    /// The frozen snapshot.
    pub release_snapshot: Value,
    /// `published | retired`.
    pub status: String,
    /// The bookkeeping version (the snapshot never changes).
    pub version: i64,
    /// When the release was compiled.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `job_releases.publish`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReleasePublishIn {
    /// The job whose current definition is compiled.
    pub job_id: Uuid,
}

/// `job_releases.retire`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseRetireIn {
    /// The release id.
    pub id: Uuid,
}

/// `job_releases.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseListIn {
    /// The job.
    pub job_id: Uuid,
}

/// One binding.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BindingOut {
    /// Row id.
    pub id: String,
    /// The job.
    pub job_id: String,
    /// The pinned release row.
    pub release_id: String,
    /// The pinned release hash.
    pub release_hash: String,
    /// The executor kind slug.
    pub executor_kind: String,
    /// `draft | active | retired`.
    pub status: String,
    /// The executable process (`procedure`).
    pub process_id: Option<String>,
    /// The agent configuration (`internal_agent`).
    pub capability_agent_id: Option<String>,
    /// The capability the binding narrows.
    pub capability_id: Option<String>,
    /// `department_scope`, authority, limits override.
    pub meta: Value,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// `job_bindings.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BindingCreateIn {
    /// The job.
    pub job_id: Uuid,
    /// The release to pin; it must belong to the job.
    pub release_id: Uuid,
    /// The executor kind (`procedure` or `internal_agent`, or any registered kind).
    pub executor_kind: String,
    /// The executable process, for a `procedure` binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<Uuid>,
    /// The agent configuration, for an `internal_agent` binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
    /// The capability the binding narrows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// `department_scope`, authority, limits override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// `draft | active`; defaults to `active`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// `job_bindings.retire`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BindingRetireIn {
    /// The binding id.
    pub id: Uuid,
    /// The version the caller last saw.
    pub version: i64,
}

/// `job_bindings.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BindingListIn {
    /// The job.
    pub job_id: Uuid,
}

// ── mapping ──────────────────────────────────────────────────────────────────────────────

fn industry_view(row: &store::organisation::IndustryRow) -> IndustryOut {
    IndustryOut {
        id: row.id.to_string(),
        key: row.key.clone(),
        name: row.name.clone(),
        status: row.status.clone(),
        meta: row.meta.clone(),
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// One job plus its ordered placement.
async fn job_view(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    row: &store::organisation::JobRow,
) -> Result<JobOut, ActionError> {
    let placements = store::organisation::job_placements_tx(&mut *conn, tenant, row.id)
        .await
        .map_err(|error| store_error(error, "job_catalogue"))?;
    Ok(JobOut {
        id: row.id.to_string(),
        key: row.job_key.clone(),
        industry_id: row.industry_id.to_string(),
        family: row.family.clone(),
        title: row.title.clone(),
        status: row.status.clone(),
        definition: row.definition.clone(),
        meta: row.meta.clone(),
        departments: placements
            .iter()
            .map(|p| JobDepartmentOut {
                department_id: p.department_id.to_string(),
                name: p.department_name.clone(),
                ordinal: p.ordinal,
                is_primary: p.is_primary,
            })
            .collect(),
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn release_view(row: &store::organisation::ReleaseRow) -> ReleaseOut {
    ReleaseOut {
        id: row.id.to_string(),
        job_id: row.job_id.to_string(),
        release_hash: row.release_hash.clone(),
        release_snapshot: row.release_snapshot.clone(),
        status: row.status.clone(),
        version: row.version,
        created_at: row.created_at,
    }
}

fn binding_view(row: &store::organisation::BindingRow) -> BindingOut {
    BindingOut {
        id: row.id.to_string(),
        job_id: row.job_id.to_string(),
        release_id: row.release_id.to_string(),
        release_hash: row.release_hash.clone(),
        executor_kind: row.executor_kind.clone(),
        status: row.status.clone(),
        process_id: row.process_id.map(|id| id.to_string()),
        capability_agent_id: row.capability_agent_id.map(|id| id.to_string()),
        capability_id: row.capability_id.map(|id| id.to_string()),
        meta: row.meta.clone(),
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Map a store refusal onto the wire.
fn organisation_error(error: OrganisationStoreError, action: &str) -> ActionError {
    match error {
        OrganisationStoreError::NotFound(what) => {
            ActionError::resource_not_found(&format!("such {what}"))
        }
        OrganisationStoreError::DepartmentNotInIndustry => invalid(
            "departments",
            "department_industry_mismatch",
            "every department of a job must belong to the job's industry",
        ),
        OrganisationStoreError::InvalidPlacement => invalid(
            "departments",
            "invalid_placement",
            "the placement must be ordered 1..=n with exactly one primary",
        ),
        OrganisationStoreError::ReleaseJobMismatch => invalid(
            "release_id",
            "release_job_mismatch",
            "the release does not belong to that job",
        ),
        OrganisationStoreError::VersionMismatch(what) => {
            ActionError::precondition(format!("the {what} moved; re-read it and retry"))
        }
        OrganisationStoreError::Executor(error) => executor_error(error),
        OrganisationStoreError::UnknownStatus(raw) => {
            tracing::error!(status = %raw, "a stored organisation status is outside the domain");
            ActionError::internal()
        }
        OrganisationStoreError::StaleRelease {
            binding_hash,
            requested_hash,
        } => ActionError::new(
            409,
            "job_binding_stale",
            format!(
                "the binding pins release {binding_hash} but the run requested {requested_hash}"
            ),
        ),
        OrganisationStoreError::Database(error) => store_error(error, action),
    }
}

/// The executor contract's refusals are `422`s that name the field.
fn executor_error(error: BindingError) -> ActionError {
    match error {
        BindingError::InvalidKind(kind) => invalid(
            "executor_kind",
            "invalid_executor_kind",
            format!("{kind:?} is not a valid executor kind"),
        ),
        BindingError::UnknownKind(kind) => invalid(
            "executor_kind",
            "unknown_executor_kind",
            format!("{kind:?} is not a registered executor kind"),
        ),
        BindingError::NoExecutor => invalid(
            "executor_kind",
            "binding_without_executor",
            "a binding names a process or a capability agent",
        ),
        BindingError::MissingProcess => invalid(
            "process_id",
            "missing_process",
            "a procedure binding must name a process",
        ),
        BindingError::MissingAgent => invalid(
            "capability_agent_id",
            "missing_capability_agent",
            "an internal_agent binding must name a capability agent",
        ),
        BindingError::DuplicateKind(kind) => {
            tracing::error!(kind, "the executor registry refused a duplicate kind");
            ActionError::internal()
        }
    }
}

fn placement_in(input: &[JobDepartmentIn]) -> Vec<store::organisation::PlacementIn> {
    input
        .iter()
        .map(|row| store::organisation::PlacementIn {
            department_id: row.department_id,
            ordinal: row.ordinal,
            is_primary: row.is_primary,
        })
        .collect()
}

fn status_in(raw: Option<&str>) -> Result<BindingStatus, ActionError> {
    match raw.unwrap_or("active") {
        "draft" => Ok(BindingStatus::Draft),
        "active" => Ok(BindingStatus::Active),
        "retired" => Err(invalid(
            "status",
            "invalid_value",
            "a new binding is draft or active; retire is its own action",
        )),
        other => Err(invalid(
            "status",
            "invalid_value",
            format!("{other:?} is not draft or active"),
        )),
    }
}

// ── industries ───────────────────────────────────────────────────────────────────────────

/// Create one industry.
#[crate::action(
    name = "industries.create",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn industries_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IndustryCreateIn,
) -> Result<IndustryOut, ActionError> {
    let key = input.key.trim();
    let name = input.name.trim();
    if key.is_empty() || name.is_empty() {
        return Err(invalid(
            if key.is_empty() { "key" } else { "name" },
            "required",
            "must not be empty",
        ));
    }
    let row = store::organisation::industry_create_tx(
        &mut *tx,
        ctx.tenant,
        &store::organisation::NewIndustry {
            key: key.to_owned(),
            name: name.to_owned(),
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
        },
    )
    .await
    .map_err(|error| organisation_error(error, "industries.create"))?;
    Ok(industry_view(&row))
}

/// Read one industry.
#[crate::action(
    name = "industries.get",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn industries_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OrganisationIdIn,
) -> Result<IndustryOut, ActionError> {
    let row = store::organisation::industry_get_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| organisation_error(error, "industries.get"))?;
    Ok(industry_view(&row))
}

/// Every industry of the tenant.
#[crate::action(
    name = "industries.list",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn industries_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<Vec<IndustryOut>, ActionError> {
    let rows = store::organisation::industries_list_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "industries.list"))?;
    Ok(rows.iter().map(industry_view).collect())
}

// ── job catalogue ────────────────────────────────────────────────────────────────────────

/// Create one catalogue position and its ordered placement.
#[crate::action(
    name = "job_catalogue.create",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_catalogue_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: JobCreateIn,
) -> Result<JobOut, ActionError> {
    let job_key = input.job_key.trim();
    let title = input.title.trim();
    if job_key.is_empty() || title.is_empty() {
        return Err(invalid(
            if job_key.is_empty() { "job_key" } else { "title" },
            "required",
            "must not be empty",
        ));
    }
    let row = store::organisation::job_create_tx(
        &mut *tx,
        ctx.tenant,
        &store::organisation::NewJob {
            job_key: job_key.to_owned(),
            industry_id: input.industry_id,
            family: input.family.trim().to_owned(),
            title: title.to_owned(),
            // A created position starts as a draft; activation is an update.
            status: "draft".to_owned(),
            definition: input.definition.unwrap_or_else(|| serde_json::json!({})),
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
            departments: placement_in(&input.departments),
        },
    )
    .await
    .map_err(|error| organisation_error(error, "job_catalogue.create"))?;
    job_view(&mut *tx, ctx.tenant, &row).await
}

/// Update the mutable half of a catalogue position.
#[crate::action(
    name = "job_catalogue.update",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_catalogue_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: JobUpdateIn,
) -> Result<JobOut, ActionError> {
    let patch = store::organisation::JobPatch {
        title: input.title.map(|title| title.trim().to_owned()),
        family: input.family.map(|family| family.trim().to_owned()),
        definition: input.definition,
        meta: input.meta,
        departments: input.departments.as_deref().map(placement_in),
    };
    let row = store::organisation::job_update_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        input.version,
        &patch,
    )
    .await
    .map_err(|error| organisation_error(error, "job_catalogue.update"))?;
    job_view(&mut *tx, ctx.tenant, &row).await
}

/// Retire a catalogue position.
#[crate::action(
    name = "job_catalogue.retire",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_catalogue_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: JobRetireIn,
) -> Result<JobOut, ActionError> {
    let row =
        store::organisation::job_retire_tx(&mut *tx, ctx.tenant, input.id, input.version)
            .await
            .map_err(|error| organisation_error(error, "job_catalogue.retire"))?;
    job_view(&mut *tx, ctx.tenant, &row).await
}

/// Read one catalogue position.
#[crate::action(
    name = "job_catalogue.get",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_catalogue_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OrganisationIdIn,
) -> Result<JobOut, ActionError> {
    let row = store::organisation::job_get_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| organisation_error(error, "job_catalogue.get"))?;
    job_view(&mut *tx, ctx.tenant, &row).await
}

/// Every catalogue position of the tenant.
#[crate::action(
    name = "job_catalogue.list",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_catalogue_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<Vec<JobOut>, ActionError> {
    let rows = store::organisation::jobs_list_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "job_catalogue.list"))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(job_view(&mut *tx, ctx.tenant, row).await?);
    }
    Ok(out)
}

// ── releases ─────────────────────────────────────────────────────────────────────────────

/// Compile and publish the job's current definition as an immutable release.
#[crate::action(
    name = "job_releases.publish",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_releases_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReleasePublishIn,
) -> Result<ReleaseOut, ActionError> {
    let row = store::organisation::job_release_publish_tx(&mut *tx, ctx.tenant, input.job_id)
        .await
        .map_err(|error| organisation_error(error, "job_releases.publish"))?;
    Ok(release_view(&row))
}

/// Retire a release (its frozen snapshot is untouched).
#[crate::action(
    name = "job_releases.retire",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_releases_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReleaseRetireIn,
) -> Result<ReleaseOut, ActionError> {
    let row = store::organisation::job_release_retire_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| organisation_error(error, "job_releases.retire"))?;
    Ok(release_view(&row))
}

/// A job's releases, newest first.
#[crate::action(
    name = "job_releases.list",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_releases_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReleaseListIn,
) -> Result<Vec<ReleaseOut>, ActionError> {
    let rows = store::organisation::job_releases_list_tx(&mut *tx, ctx.tenant, input.job_id)
        .await
        .map_err(|error| store_error(error, "job_releases.list"))?;
    Ok(rows.iter().map(release_view).collect())
}

// ── bindings ─────────────────────────────────────────────────────────────────────────────

/// Pin one release to one executor.
#[crate::action(
    name = "job_bindings.create",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_bindings_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingCreateIn,
) -> Result<BindingOut, ActionError> {
    let executor_kind = ExecutorKind::parse(input.executor_kind.trim())
        .map_err(executor_error)?;
    let status = status_in(input.status.as_deref())?;
    let row = store::organisation::job_binding_create_tx(
        &mut *tx,
        ctx.tenant,
        &store::organisation::NewBinding {
            job_id: input.job_id,
            release_id: input.release_id,
            executor_kind,
            process_id: input.process_id,
            capability_agent_id: input.capability_agent_id,
            capability_id: input.capability_id,
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
            status,
        },
        &Registry::builtin(),
    )
    .await
    .map_err(|error| organisation_error(error, "job_bindings.create"))?;
    Ok(binding_view(&row))
}

/// Retire a binding; new runs naming it are refused `409 job_binding_stale`.
#[crate::action(
    name = "job_bindings.retire",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_bindings_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingRetireIn,
) -> Result<BindingOut, ActionError> {
    let row = store::organisation::job_binding_retire_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        input.version,
    )
    .await
    .map_err(|error| organisation_error(error, "job_bindings.retire"))?;
    Ok(binding_view(&row))
}

/// Read one binding.
#[crate::action(
    name = "job_bindings.get",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_bindings_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OrganisationIdIn,
) -> Result<BindingOut, ActionError> {
    let row = store::organisation::job_binding_get_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| organisation_error(error, "job_bindings.get"))?;
    Ok(binding_view(&row))
}

/// A job's bindings, newest first.
#[crate::action(
    name = "job_bindings.list",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn job_bindings_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BindingListIn,
) -> Result<Vec<BindingOut>, ActionError> {
    let rows = store::organisation::job_bindings_list_tx(&mut *tx, ctx.tenant, input.job_id)
        .await
        .map_err(|error| store_error(error, "job_bindings.list"))?;
    Ok(rows.iter().map(binding_view).collect())
}
