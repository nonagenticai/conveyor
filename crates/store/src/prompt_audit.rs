#![forbid(unsafe_code)]
//! The prompt audit's store: the audit row, its findings, the decision gate and the cadence
//! helpers (S16).
//!
//! Findings are inert without a person: [`decide_finding_tx`] only moves `open → accepted |
//! rejected`, and the later `accepted → applied` belongs to an ordinary `processes.update`.
//! The audit never holds `processes:write` over the surface it audits (CONTRACT §18).
//!
//! The after-N-executions trigger reads the ledger, never a counter column: [`calls_since_last_audit_tx`]
//! counts `inference_call` rows since the tenant's newest audit and excludes the audit's own call
//! (`prompt_ref = 'prompt_audit@1'`), so an audit cannot re-trigger itself.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::prompt_audit::{
    AuditSettings, AuditStatus, FindingDraft, FindingError, FindingStatus,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// The built-in audit prompt's versioned reference.
pub const PROMPT_REF_V1: &str = "prompt_audit@1";

/// Why a prompt-audit operation failed.
#[derive(Debug, thiserror::Error)]
pub enum PromptAuditStoreError {
    /// No such row in this tenant.
    #[error("the prompt audit row does not exist in this tenant")]
    NotFound,
    /// A transition the domain's state machine does not allow.
    #[error("{0}")]
    Illegal(String),
    /// A finding was already decided, and the caller asked for a different decision.
    #[error("the finding is already {0}")]
    AlreadyDecided(String),
    /// A finding draft the DDL would refuse.
    #[error(transparent)]
    Invalid(#[from] FindingError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// One `prompt_audit` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PromptAuditRow {
    /// The row id.
    pub id: Uuid,
    /// The requested scope document.
    pub scope: Value,
    /// The model the audit calls.
    pub target_model: String,
    /// The prompt family (`prompt_audit@1`).
    pub prompt_ref: String,
    /// `queued | running | succeeded | failed`.
    pub status: String,
    /// Findings persisted by a successful run.
    pub finding_count: i32,
    /// Why a failed run failed.
    pub error: Option<String>,
    /// The human who started a manual audit; `NULL` for an event-created one.
    pub requested_by: Option<Uuid>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One `prompt_audit_finding` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct FindingRow {
    /// The row id.
    pub id: Uuid,
    /// The owning audit.
    pub audit_id: Uuid,
    /// Where the finding applies.
    pub location: String,
    /// The text it is about.
    pub evidence: String,
    /// The anti-pattern code.
    pub pattern: String,
    /// Why it is obsolete.
    pub why_obsolete: String,
    /// `high | medium | low`.
    pub confidence: String,
    /// `remove | rewrite | move | replace_with_api_feature | add | flag`.
    pub action: String,
    /// The proposed replacement.
    pub replacement: Option<String>,
    /// `open | accepted | rejected | applied`.
    pub status: String,
    /// The person who decided.
    pub decided_by: Option<Uuid>,
    /// When they decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// A new audit row.
#[derive(Clone, Debug)]
pub struct NewPromptAudit {
    /// The requested scope.
    pub scope: Value,
    /// The model the run calls.
    pub target_model: String,
    /// The prompt family; always [`PROMPT_REF_V1`] today.
    pub prompt_ref: String,
    /// The human who started it, or `None` for the cadence trigger.
    pub requested_by: Option<Uuid>,
}

const AUDIT_COLUMNS: &str = "id, scope, target_model, prompt_ref, status, finding_count, error,
    requested_by, created_at, updated_at";
const FINDING_COLUMNS: &str = "id, audit_id, location, evidence, pattern, why_obsolete, confidence,
    action, replacement, status, decided_by, decided_at, created_at, updated_at";

/// Insert an audit in the caller's transaction.
pub async fn insert_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    audit: &NewPromptAudit,
) -> Result<Uuid, PromptAuditStoreError> {
    let sql = format!(
        "INSERT INTO prompt_audit (tenant_id, scope, target_model, prompt_ref, requested_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {AUDIT_COLUMNS}"
    );
    let row: PromptAuditRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&audit.scope)
        .bind(&audit.target_model)
        .bind(&audit.prompt_ref)
        .bind(audit.requested_by)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.id)
}

/// Insert an event-created audit, unless one is already live for the tenant.
///
/// The partial unique index `prompt_audit_auto_live_idx` is the race guard: two captured calls
/// that both cross the threshold conflict on `(tenant_id)` and only one row is inserted. The
/// caller enqueues the job **only** when this returns `Some`.
pub async fn insert_auto_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    audit: &NewPromptAudit,
) -> Result<Option<Uuid>, PromptAuditStoreError> {
    let sql = format!(
        "INSERT INTO prompt_audit (tenant_id, scope, target_model, prompt_ref, requested_by)
         VALUES ($1, $2, $3, $4, NULL)
         ON CONFLICT (tenant_id) WHERE status IN ('queued', 'running') AND requested_by IS NULL
         DO NOTHING
         RETURNING {AUDIT_COLUMNS}"
    );
    let row: Option<PromptAuditRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&audit.scope)
        .bind(&audit.target_model)
        .bind(&audit.prompt_ref)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(|row| row.id))
}

/// Read one audit in the caller's tenant.
pub async fn get_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<PromptAuditRow>, PromptAuditStoreError> {
    let sql = format!("SELECT {AUDIT_COLUMNS} FROM prompt_audit WHERE tenant_id = $1 AND id = $2");
    let row: Option<PromptAuditRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// List the tenant's audits, newest first.
pub async fn list_audits_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<PromptAuditRow>, PromptAuditStoreError> {
    let sql = format!(
        "SELECT {AUDIT_COLUMNS}
           FROM prompt_audit
          WHERE tenant_id = $1
            AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3))
          ORDER BY created_at DESC, id DESC
          LIMIT $4"
    );
    let rows: Vec<PromptAuditRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(before.map(|(at, _)| at))
        .bind(before.map(|(_, id)| id))
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Advance an audit through the state machine. A same-status replay is a no-op, not an error.
pub async fn transition_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    to: AuditStatus,
    finding_count: Option<i32>,
    error: Option<&str>,
) -> Result<PromptAuditRow, PromptAuditStoreError> {
    let sql = format!(
        "SELECT {AUDIT_COLUMNS} FROM prompt_audit WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<PromptAuditRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let current = row.ok_or(PromptAuditStoreError::NotFound)?;
    let from = AuditStatus::parse(&current.status).ok_or_else(|| {
        PromptAuditStoreError::Illegal(format!("unknown audit status {}", current.status))
    })?;
    if from == to {
        return Ok(current);
    }
    if !from.may_transition_to(to) {
        return Err(PromptAuditStoreError::Illegal(format!(
            "an audit cannot move from {} to {}",
            from.as_str(),
            to.as_str()
        )));
    }
    let sql = format!(
        "UPDATE prompt_audit
            SET status = $3,
                finding_count = coalesce($4, finding_count),
                error = coalesce($5, error),
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {AUDIT_COLUMNS}"
    );
    let row: PromptAuditRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(to.as_str())
        .bind(finding_count)
        .bind(error)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Persist one finding under an audit resolved in the caller's tenant.
pub async fn insert_finding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    audit_id: Uuid,
    draft: &FindingDraft,
) -> Result<Uuid, PromptAuditStoreError> {
    draft.validate()?;
    // The FK alone would accept another tenant's audit: its RI check runs as the table owner and
    // bypasses RLS. Resolve the parent in-tenant first (data-arch §2.17, trap 7).
    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM prompt_audit WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(audit_id)
    .fetch_one(&mut *conn)
    .await?;
    if !exists {
        return Err(PromptAuditStoreError::NotFound);
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO prompt_audit_finding (
             tenant_id, audit_id, location, evidence, pattern, why_obsolete,
             confidence, action, replacement
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(audit_id)
    .bind(&draft.location)
    .bind(&draft.evidence)
    .bind(draft.pattern.as_str())
    .bind(&draft.why_obsolete)
    .bind(draft.confidence.as_str())
    .bind(draft.action.as_str())
    .bind(&draft.replacement)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// The finding list filters.
#[derive(Clone, Debug, Default)]
pub struct ListFindings {
    /// Restrict to one audit.
    pub audit_id: Option<Uuid>,
    /// Restrict to one status.
    pub status: Option<String>,
    /// Restrict to one confidence.
    pub confidence: Option<String>,
    /// The keyset cursor.
    pub before: Option<(DateTime<Utc>, Uuid)>,
    /// Page size.
    pub limit: i64,
}

/// List the tenant's findings, newest first.
pub async fn list_findings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &ListFindings,
) -> Result<Vec<FindingRow>, PromptAuditStoreError> {
    let sql = format!(
        "SELECT {FINDING_COLUMNS}
           FROM prompt_audit_finding
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR audit_id = $2)
            AND ($3::text IS NULL OR status = $3)
            AND ($4::text IS NULL OR confidence = $4)
            AND ($5::timestamptz IS NULL OR (created_at, id) < ($5, $6))
          ORDER BY created_at DESC, id DESC
          LIMIT $7"
    );
    let rows: Vec<FindingRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(filter.audit_id)
        .bind(filter.status.as_deref())
        .bind(filter.confidence.as_deref())
        .bind(filter.before.map(|(at, _)| at))
        .bind(filter.before.map(|(_, id)| id))
        .bind(filter.limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Decide one finding. A same-decision replay is a no-op; a different one is refused.
pub async fn decide_finding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    finding_id: Uuid,
    decision: FindingStatus,
    decided_by: Uuid,
) -> Result<FindingRow, PromptAuditStoreError> {
    let sql = format!(
        "SELECT {FINDING_COLUMNS} FROM prompt_audit_finding
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<FindingRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(finding_id)
        .fetch_optional(&mut *conn)
        .await?;
    let current = row.ok_or(PromptAuditStoreError::NotFound)?;
    let from = FindingStatus::parse(&current.status).ok_or_else(|| {
        PromptAuditStoreError::Illegal(format!("unknown finding status {}", current.status))
    })?;
    if from == decision {
        return Ok(current);
    }
    if !from.may_transition_to(decision) {
        return Err(PromptAuditStoreError::AlreadyDecided(current.status));
    }
    let sql = format!(
        "UPDATE prompt_audit_finding
            SET status = $3, decided_by = $4, decided_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {FINDING_COLUMNS}"
    );
    let row: FindingRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(finding_id)
        .bind(decision.as_str())
        .bind(decided_by)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// The tenant's cadence policy, read from `tenant.settings`.
pub async fn audit_settings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<AuditSettings, PromptAuditStoreError> {
    let row: Option<(Value,)> = sqlx::query_as("SELECT settings FROM tenant WHERE id = $1")
        .bind(tenant.as_uuid())
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row
        .map(|(settings,)| AuditSettings::from_settings(&settings))
        .unwrap_or_default())
}

/// How many captured calls have happened since the tenant's newest audit.
///
/// The ledger is the counter (no column can drift), and the audit's own `prompt_audit@1` call is
/// excluded so a run cannot re-trigger the cadence it is servicing.
pub async fn calls_since_last_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<i64, PromptAuditStoreError> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*)::bigint
           FROM inference_call
          WHERE tenant_id = $1
            AND created_at > coalesce(
                (SELECT max(created_at) FROM prompt_audit WHERE tenant_id = $1),
                '-infinity'::timestamptz)
            AND prompt_ref IS DISTINCT FROM $2",
    )
    .bind(tenant.as_uuid())
    .bind(PROMPT_REF_V1)
    .fetch_one(&mut *conn)
    .await?;
    Ok(count)
}
