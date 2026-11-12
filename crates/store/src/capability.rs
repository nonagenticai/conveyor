#![forbid(unsafe_code)]
//! S15's capability store: the aggregate, its configured executor, the knowledge bindings, the
//! relation edges and the two queries a run resolves through.
//!
//! Every function is `_tx` and takes the caller's connection: a capability write and the event it
//! emits belong to one transaction, and readiness is resolved before a run row exists. Every query
//! filters `tenant_id = $1` even though RLS is FORCE-enabled — the explicit predicate is the first
//! line of defence, the policy the second.
//!
//! `capability_agent` is configuration, never a loop (CONTRACT §17): nothing here calls a model.

use chrono::{DateTime, Utc};
use conveyor_domain::capability::{CapabilityStatus, RelationKind};
use conveyor_domain::{IdError, TenantId};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::catalogue::ImportCounts;
use crate::parent::{Parent, ParentError, ensure_parent_tx};

/// Why a capability operation failed.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityStoreError {
    /// No such row in this tenant.
    #[error("the capability does not exist in this tenant")]
    NotFound,
    /// A capability with this code already exists (`capability_tenant_code_key`).
    #[error("a capability with that code already exists")]
    DuplicateCode,
    /// An agent with this key already exists (`capability_agent_tenant_key_key`).
    #[error("a capability agent with that key already exists")]
    DuplicateKey,
    /// The row moved on since the caller read it.
    #[error("the capability agent is at version {current}, not the version the caller supplied")]
    VersionMismatch {
        /// The row's current version.
        current: i64,
    },
    /// A referenced capability does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// A stored version was not a valid [`conveyor_domain::Version`] (impossible under the schema).
    #[error(transparent)]
    Id(#[from] IdError),
    /// A status the column does not accept (capability or agent — one vocabulary).
    #[error("{0:?} is not a capability status")]
    UnknownStatus(String),
    /// A required text field is blank; the column's CHECK would otherwise surface as a 500.
    #[error("{0} must not be blank")]
    Blank(&'static str),
    /// A knowledge pack version must be a positive integer.
    #[error("pack_version must be positive, got {0}")]
    BadPackVersion(i32),
    /// A seed row names a relation kind the column does not accept.
    #[error("the seed relation {0:?} -> {1:?} has an unknown kind {2:?}")]
    UnknownRelation(String, String, String),
    /// A seed relation names a capability that is not in the document.
    #[error("the seed relation names unknown capability {0:?}")]
    UnknownCapability(String),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const CAPABILITY_COLUMNS: &str =
    "id, code, label, description, status, meta, version, created_at, updated_at";

const AGENT_COLUMNS: &str = "id, capability_id, key, status, model_ref, prompt_ref, params, \
     allowed_actions, max_iterations, timeout_secs, version, created_at, updated_at";

/// A capability row as stored.
#[derive(Clone, Debug)]
pub struct CapabilityRecord {
    /// The row id.
    pub id: Uuid,
    /// The tenant-wide unique code (`analysis`, `architecture`, …).
    pub code: String,
    /// The display label.
    pub label: String,
    /// The description (`''` when absent).
    pub description: String,
    /// `draft | active | retired`.
    pub status: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change time.
    pub updated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct CapabilityRow {
    id: Uuid,
    code: String,
    label: String,
    description: String,
    status: String,
    meta: Value,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<CapabilityRow> for CapabilityRecord {
    fn from(row: CapabilityRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            label: row.label,
            description: row.description,
            status: row.status,
            meta: row.meta,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// A configured executor for one capability.
#[derive(Clone, Debug)]
pub struct AgentRecord {
    /// The row id.
    pub id: Uuid,
    /// The capability it executes.
    pub capability_id: Uuid,
    /// Tenant-wide unique key.
    pub key: String,
    /// `draft | active | retired`.
    pub status: String,
    /// The model registry reference.
    pub model_ref: String,
    /// The built-in prompt, `<name>@<version>`.
    pub prompt_ref: Option<String>,
    /// Free-form parameters carried into the step input.
    pub params: Value,
    /// The actions the executor may invoke.
    pub allowed_actions: Vec<String>,
    /// The loop's iteration ceiling (`1..=20`).
    pub max_iterations: i32,
    /// The loop's wall-clock ceiling (`1..=300`).
    pub timeout_secs: i32,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change time.
    pub updated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct AgentRow {
    id: Uuid,
    capability_id: Uuid,
    key: String,
    status: String,
    model_ref: String,
    prompt_ref: Option<String>,
    params: Value,
    allowed_actions: Vec<String>,
    max_iterations: i32,
    timeout_secs: i32,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<AgentRow> for AgentRecord {
    fn from(row: AgentRow) -> Self {
        Self {
            id: row.id,
            capability_id: row.capability_id,
            key: row.key,
            status: row.status,
            model_ref: row.model_ref,
            prompt_ref: row.prompt_ref,
            params: row.params,
            allowed_actions: row.allowed_actions,
            max_iterations: row.max_iterations,
            timeout_secs: row.timeout_secs,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// One capability → pack binding.
#[derive(Clone, Debug)]
pub struct KnowledgeRecord {
    /// The row id.
    pub id: Uuid,
    /// The capability it binds.
    pub capability_id: Uuid,
    /// The strata pack key.
    pub pack_key: String,
    /// The pinned pack version.
    pub pack_version: i32,
    /// The minimum score a hit must reach (when set).
    pub min_score: Option<Decimal>,
    /// Whether the pack is required (an unreachable required pack fails the run).
    pub required: bool,
}

#[derive(FromRow)]
struct KnowledgeRow {
    id: Uuid,
    capability_id: Uuid,
    pack_key: String,
    pack_version: i32,
    min_score: Option<Decimal>,
    required: bool,
}

impl From<KnowledgeRow> for KnowledgeRecord {
    fn from(row: KnowledgeRow) -> Self {
        Self {
            id: row.id,
            capability_id: row.capability_id,
            pack_key: row.pack_key,
            pack_version: row.pack_version,
            min_score: row.min_score,
            required: row.required,
        }
    }
}

/// One capability → capability edge.
#[derive(Clone, Debug, FromRow)]
pub struct RelationRecord {
    /// The capability that has the edge.
    pub from_capability_id: Uuid,
    /// The capability it points at.
    pub to_capability_id: Uuid,
    /// `requires | enables | concern`.
    pub kind: String,
}

/// One row of `seed/capability.json`, before it has an id.
#[derive(Clone, Debug)]
pub struct CapabilityImport {
    /// The unique code.
    pub code: String,
    /// The display label.
    pub label: String,
    /// The description.
    pub description: String,
    /// `draft | active | retired`.
    pub status: String,
    /// Free-form metadata.
    pub meta: Value,
}

/// One relation of `seed/capability.json`, by code.
#[derive(Clone, Debug)]
pub struct RelationImport {
    /// The requiring/enabling capability's code.
    pub from_code: String,
    /// The target capability's code.
    pub to_code: String,
    /// `requires | enables | concern`.
    pub kind: String,
}

/// What an idempotent seed import did with one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportOutcome {
    /// The row did not exist.
    Created,
    /// The row existed with at least one changed field.
    Updated,
    /// The row existed and was already identical.
    Skipped,
}

/// Read one capability by id.
pub async fn get_capability_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CapabilityRecord>, CapabilityStoreError> {
    let sql =
        format!("SELECT {CAPABILITY_COLUMNS} FROM capability WHERE tenant_id = $1 AND id = $2");
    let row: Option<CapabilityRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Read one capability by its tenant-unique code.
pub async fn get_capability_by_code_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    code: &str,
) -> Result<Option<CapabilityRecord>, CapabilityStoreError> {
    let sql =
        format!("SELECT {CAPABILITY_COLUMNS} FROM capability WHERE tenant_id = $1 AND code = $2");
    let row: Option<CapabilityRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(code)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// List capabilities, keyset-ordered by code. The caller asks for `limit + 1` rows to know whether
/// a next page exists; the store never clamps, because a hidden clamp is a silent truncation.
pub async fn list_capabilities_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    after_code: Option<&str>,
    limit: i64,
) -> Result<Vec<CapabilityRecord>, CapabilityStoreError> {
    let sql = format!(
        "SELECT {CAPABILITY_COLUMNS} FROM capability
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::text IS NULL OR code > $3)
          ORDER BY code
          LIMIT $4"
    );
    let rows: Vec<CapabilityRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(after_code)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// A capability to create. The code is unique per tenant, so a second create is a typed conflict
/// rather than an upsert: a caller that meant to update must say so with an If-Match.
#[derive(Clone, Debug)]
pub struct NewCapability {
    /// The tenant-unique code.
    pub code: String,
    /// The display label.
    pub label: String,
    /// The description.
    pub description: String,
    /// `draft | active | retired`.
    pub status: String,
    /// Free-form metadata.
    pub meta: Value,
}

/// Create one capability.
pub async fn create_capability_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability: &NewCapability,
) -> Result<CapabilityRecord, CapabilityStoreError> {
    let status = CapabilityStatus::parse(&capability.status)
        .ok_or_else(|| CapabilityStoreError::UnknownStatus(capability.status.clone()))?;
    if capability.code.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("code"));
    }
    if capability.label.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("label"));
    }
    let sql = format!(
        "INSERT INTO capability (tenant_id, code, label, description, status, meta)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {CAPABILITY_COLUMNS}"
    );
    let row: CapabilityRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&capability.code)
        .bind(&capability.label)
        .bind(&capability.description)
        .bind(status.as_str())
        .bind(&capability.meta)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_unique)?;
    Ok(row.into())
}

/// Import one capability from the seed document. Idempotent by `(tenant_id, code)`; a change bumps
/// `version`, so a seed refresh is visible to an If-Match caller rather than silently applied.
pub async fn import_capability_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    row: &CapabilityImport,
) -> Result<ImportOutcome, CapabilityStoreError> {
    let status = CapabilityStatus::parse(&row.status)
        .ok_or_else(|| CapabilityStoreError::UnknownStatus(row.status.clone()))?;
    if row.code.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("code"));
    }
    if row.label.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("label"));
    }
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, description, status, meta)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, code) DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(&row.code)
    .bind(&row.label)
    .bind(&row.description)
    .bind(status.as_str())
    .bind(&row.meta)
    .fetch_optional(&mut *conn)
    .await?;
    if inserted.is_some() {
        return Ok(ImportOutcome::Created);
    }

    let current = get_capability_by_code_tx(conn, tenant, &row.code)
        .await?
        .ok_or_else(|| CapabilityStoreError::UnknownCapability(row.code.clone()))?;
    let unchanged = current.label == row.label
        && current.description == row.description
        && current.status == status.as_str()
        && current.meta == row.meta;
    if unchanged {
        return Ok(ImportOutcome::Skipped);
    }
    sqlx::query(
        "UPDATE capability
            SET label = $3, description = $4, status = $5, meta = $6,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND code = $2",
    )
    .bind(tenant.as_uuid())
    .bind(&row.code)
    .bind(&row.label)
    .bind(&row.description)
    .bind(status.as_str())
    .bind(&row.meta)
    .execute(&mut *conn)
    .await?;
    Ok(ImportOutcome::Updated)
}

/// Import every capability of the seed document, summing the outcomes.
pub async fn import_capabilities_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    rows: &[CapabilityImport],
) -> Result<ImportCounts, CapabilityStoreError> {
    let mut counts = ImportCounts {
        created: 0,
        updated: 0,
        skipped: 0,
    };
    for row in rows {
        match import_capability_tx(conn, tenant, row).await? {
            ImportOutcome::Created => counts.created += 1,
            ImportOutcome::Updated => counts.updated += 1,
            ImportOutcome::Skipped => counts.skipped += 1,
        }
    }
    Ok(counts)
}

/// Import the seed document's relation edges, resolving both codes inside the tenant. An edge whose
/// endpoint is not in the document is refused: a silent skip would hide a typo'd code.
pub async fn import_relations_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    rows: &[RelationImport],
) -> Result<ImportCounts, CapabilityStoreError> {
    let mut counts = ImportCounts {
        created: 0,
        updated: 0,
        skipped: 0,
    };
    for row in rows {
        let from = get_capability_by_code_tx(conn, tenant, &row.from_code)
            .await?
            .ok_or_else(|| CapabilityStoreError::UnknownCapability(row.from_code.clone()))?;
        let to = get_capability_by_code_tx(conn, tenant, &row.to_code)
            .await?
            .ok_or_else(|| CapabilityStoreError::UnknownCapability(row.to_code.clone()))?;
        let kind = RelationKind::parse(&row.kind).ok_or_else(|| {
            CapabilityStoreError::UnknownRelation(
                row.from_code.clone(),
                row.to_code.clone(),
                row.kind.clone(),
            )
        })?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO capability_relation
                 (tenant_id, from_capability_id, to_capability_id, kind)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (from_capability_id, to_capability_id, kind) DO NOTHING
             RETURNING from_capability_id",
        )
        .bind(tenant.as_uuid())
        .bind(from.id)
        .bind(to.id)
        .bind(kind.as_str())
        .fetch_optional(&mut *conn)
        .await?;
        if inserted.is_some() {
            counts.created += 1;
        } else {
            counts.skipped += 1;
        }
    }
    Ok(counts)
}

/// The executor spec to create. The action layer validates it with
/// [`conveyor_domain::capability::validate_agent_config`] first.
#[derive(Clone, Debug)]
pub struct NewAgent {
    /// The capability it executes.
    pub capability_id: Uuid,
    /// The tenant-unique key.
    pub key: String,
    /// `draft | active | retired`.
    pub status: String,
    /// The model registry reference.
    pub model_ref: String,
    /// The built-in prompt reference.
    pub prompt_ref: Option<String>,
    /// Free-form parameters.
    pub params: Value,
    /// The actions the executor may invoke.
    pub allowed_actions: Vec<String>,
    /// The iteration ceiling.
    pub max_iterations: i32,
    /// The wall-clock ceiling.
    pub timeout_secs: i32,
}

/// The mutable fields of an existing executor spec. `capability_id` is not among them: moving an
/// agent between capabilities would change what every recorded run resolved.
#[derive(Clone, Debug)]
pub struct AgentPatch {
    /// `draft | active | retired`.
    pub status: String,
    /// The model registry reference.
    pub model_ref: String,
    /// The built-in prompt reference.
    pub prompt_ref: Option<String>,
    /// Free-form parameters.
    pub params: Value,
    /// The actions the executor may invoke.
    pub allowed_actions: Vec<String>,
    /// The iteration ceiling.
    pub max_iterations: i32,
    /// The wall-clock ceiling.
    pub timeout_secs: i32,
}

/// Create one executor spec. The capability is resolved inside the tenant first, so a foreign id is
/// a `ParentError`, never an FK violation.
pub async fn create_agent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    agent: &NewAgent,
) -> Result<AgentRecord, CapabilityStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Capability, agent.capability_id).await?;
    CapabilityStatus::parse(&agent.status)
        .ok_or_else(|| CapabilityStoreError::UnknownStatus(agent.status.clone()))?;
    if agent.key.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("key"));
    }
    if agent.model_ref.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("model_ref"));
    }
    // 0035 makes `process_key` mandatory for a `process_agent`, and every executor created through
    // this path is one: its key IS the process key (`process_key = key`, the same convention the
    // migration's backfill used). A tool agent is created by the catalogue seed, not here.
    let sql = format!(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, status, model_ref, prompt_ref, params,
              allowed_actions, max_iterations, timeout_secs, role, process_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'process_agent', $3)
         RETURNING {AGENT_COLUMNS}"
    );
    let row: AgentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(agent.capability_id)
        .bind(&agent.key)
        .bind(&agent.status)
        .bind(&agent.model_ref)
        .bind(&agent.prompt_ref)
        .bind(&agent.params)
        .bind(&agent.allowed_actions)
        .bind(agent.max_iterations)
        .bind(agent.timeout_secs)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_unique)?;
    Ok(row.into())
}

/// Read one executor spec.
pub async fn get_agent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<AgentRecord>, CapabilityStoreError> {
    let sql =
        format!("SELECT {AGENT_COLUMNS} FROM capability_agent WHERE tenant_id = $1 AND id = $2");
    let row: Option<AgentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// List the executor specs, optionally for one capability.
pub async fn list_agents_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<AgentRecord>, CapabilityStoreError> {
    list_agents_after_tx(conn, tenant, capability_id, None, limit).await
}

/// List executor specs in key order, after a keyset cursor. The caller asks for `limit + 1` rows to
/// know whether a next page exists; the store never clamps, because a hidden clamp is a silent
/// truncation.
pub async fn list_agents_after_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability_id: Option<Uuid>,
    after_key: Option<&str>,
    limit: i64,
) -> Result<Vec<AgentRecord>, CapabilityStoreError> {
    let sql = format!(
        "SELECT {AGENT_COLUMNS} FROM capability_agent
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR capability_id = $2)
            AND ($3::text IS NULL OR key > $3)
          ORDER BY key
          LIMIT $4"
    );
    let rows: Vec<AgentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(capability_id)
        .bind(after_key)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Update an executor spec with an `If-Match` version. A stale version is a typed conflict; a
/// missing row is [`CapabilityStoreError::NotFound`].
pub async fn update_agent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: i64,
    patch: &AgentPatch,
) -> Result<AgentRecord, CapabilityStoreError> {
    CapabilityStatus::parse(&patch.status)
        .ok_or_else(|| CapabilityStoreError::UnknownStatus(patch.status.clone()))?;
    if patch.model_ref.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("model_ref"));
    }
    let sql = format!(
        "UPDATE capability_agent
            SET status = $3, model_ref = $4, prompt_ref = $5, params = $6,
                allowed_actions = $7, max_iterations = $8, timeout_secs = $9,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $10
          RETURNING {AGENT_COLUMNS}"
    );
    let row: Option<AgentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.status)
        .bind(&patch.model_ref)
        .bind(&patch.prompt_ref)
        .bind(&patch.params)
        .bind(&patch.allowed_actions)
        .bind(patch.max_iterations)
        .bind(patch.timeout_secs)
        .bind(expected_version)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_unique)?;
    if let Some(row) = row {
        return Ok(row.into());
    }
    let current: Option<i64> =
        sqlx::query_scalar("SELECT version FROM capability_agent WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    match current {
        None => Err(CapabilityStoreError::NotFound),
        Some(current) => Err(CapabilityStoreError::VersionMismatch { current }),
    }
}

/// Bind a knowledge pack to a capability. The binding is an upsert on the pinned version; the
/// capability is resolved inside the tenant first.
#[allow(clippy::too_many_arguments)]
pub async fn bind_knowledge_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability_id: Uuid,
    pack_key: &str,
    pack_version: i32,
    min_score: Option<Decimal>,
    required: bool,
) -> Result<KnowledgeRecord, CapabilityStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Capability, capability_id).await?;
    if pack_key.trim().is_empty() {
        return Err(CapabilityStoreError::Blank("pack_key"));
    }
    if pack_version <= 0 {
        return Err(CapabilityStoreError::BadPackVersion(pack_version));
    }
    let row: KnowledgeRow = sqlx::query_as(
        "INSERT INTO capability_knowledge
             (tenant_id, capability_id, pack_key, pack_version, min_score, required)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, capability_id, pack_key, pack_version)
         DO UPDATE SET min_score = EXCLUDED.min_score, required = EXCLUDED.required,
                       updated_at = now()
         RETURNING id, capability_id, pack_key, pack_version, min_score, required",
    )
    .bind(tenant.as_uuid())
    .bind(capability_id)
    .bind(pack_key)
    .bind(pack_version)
    .bind(min_score)
    .bind(required)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.into())
}

/// The packs a capability is bound to, in pack-key order. This is the query a run resolves through;
/// `knowledge.query` (S15 step 7) must not invent a second one.
pub async fn packs_for_capability_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability_id: Uuid,
) -> Result<Vec<KnowledgeRecord>, CapabilityStoreError> {
    let rows: Vec<KnowledgeRow> = sqlx::query_as(
        "SELECT id, capability_id, pack_key, pack_version, min_score, required
           FROM capability_knowledge
          WHERE tenant_id = $1 AND capability_id = $2
          ORDER BY pack_key, pack_version",
    )
    .bind(tenant.as_uuid())
    .bind(capability_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Insert or keep one relation edge. Both endpoints are resolved inside the tenant.
pub async fn upsert_relation_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    from: Uuid,
    to: Uuid,
    kind: RelationKind,
) -> Result<RelationRecord, CapabilityStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Capability, from).await?;
    ensure_parent_tx(conn, tenant, Parent::Capability, to).await?;
    let row: RelationRecord = sqlx::query_as(
        "INSERT INTO capability_relation
             (tenant_id, from_capability_id, to_capability_id, kind)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (from_capability_id, to_capability_id, kind)
         DO UPDATE SET kind = EXCLUDED.kind
         RETURNING from_capability_id, to_capability_id, kind",
    )
    .bind(tenant.as_uuid())
    .bind(from)
    .bind(to)
    .bind(kind.as_str())
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

/// The edges leaving one capability.
pub async fn relations_from_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    from: Uuid,
) -> Result<Vec<RelationRecord>, CapabilityStoreError> {
    let rows: Vec<RelationRecord> = sqlx::query_as(
        "SELECT from_capability_id, to_capability_id, kind
           FROM capability_relation
          WHERE tenant_id = $1 AND from_capability_id = $2
          ORDER BY kind, to_capability_id",
    )
    .bind(tenant.as_uuid())
    .bind(from)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The capability a card type resolves to (the default path when a process names none). `None`
/// when the card type is unbound — the readiness step treats that as a block, not an error.
pub async fn capability_for_card_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_type_id: Uuid,
) -> Result<Option<CapabilityRecord>, CapabilityStoreError> {
    let row: Option<CapabilityRow> = sqlx::query_as(
        "SELECT c.id, c.code, c.label, c.description, c.status, c.meta, c.version,
                c.created_at, c.updated_at
           FROM card_type ct
           JOIN capability c
             ON c.id = ct.capability_id AND c.tenant_id = ct.tenant_id
          WHERE ct.tenant_id = $1 AND ct.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_type_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Into::into))
}

/// Map a duplicate key to the typed conflict; anything else is a database error.
fn map_unique(error: sqlx::Error) -> CapabilityStoreError {
    if let sqlx::Error::Database(database) = &error
        && database.code().as_deref() == Some("23505")
    {
        let constraint = database.constraint().unwrap_or("");
        return if constraint == "capability_agent_tenant_key_key" {
            CapabilityStoreError::DuplicateKey
        } else if constraint == "capability_tenant_code_key" {
            CapabilityStoreError::DuplicateCode
        } else {
            CapabilityStoreError::Database(error)
        };
    }
    CapabilityStoreError::Database(error)
}
