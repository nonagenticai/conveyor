#![forbid(unsafe_code)]
//! How a tenant wants its destructive actions gated (conveyor#282, SPEC_V2 §10.4 / §11.4 / §5.4).
//!
//! One setting per (tenant, action):
//!
//! * `approval` — `required` (the default, and §11.4's rule) or `waived` (this tenant's opt-out).
//!
//! `self_approval` is **not** a setting: §5.4 / P4 make the requester's exclusion an invariant,
//! so [`SelfApproval::Forbidden`] is the only storable value and no read path returns permission.
//! The `allowed` spelling survives only so a row written before ledger v1-w4b-4 still reads; such
//! a row is inert data.
//!
//! ⛔ **An absent row is the safe default, never "no policy".** Every reader in this module
//! answers with `required` / `forbidden` when the row is missing, so a tenant that has decided
//! nothing gets the strict behaviour it never opted out of. Only `set` writes a row.
//!
//! `action` is the action's own name, which is the same string `approval.gate` carries for a
//! destructive gate — that equality is what lets [`waives`] read the policy for the gate it is
//! about to admit without any caller naming the action again.

use std::str::FromStr;

use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// Why a policy read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalPolicyStoreError {
    /// A stored value is outside the vocabulary the migration's CHECK constraints allow. The
    /// constraint makes this unreachable in practice; it exists so a corrupted row surfaces as a
    /// typed error rather than being silently coerced to a default.
    #[error("{what} is {value:?}, which is not a {what} this build knows")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// `self_approval = allowed` was offered for storage. §5.4 / P4 make the requester's
    /// exclusion an invariant, so `forbidden` is the only storable spelling; a historical
    /// `allowed` row still reads (ledger v1-w4b-4).
    #[error(
        "self_approval cannot be `allowed`: the requester can never decide their own gate \
         (SPEC_V2 §5.4)"
    )]
    SelfApprovalAllowed,
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Whether a destructive action needs an approval at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalRequirement {
    /// An approved approval naming the exact target is required. The default.
    Required,
    /// This tenant has opted out for this action; the call runs without one.
    Waived,
}

impl ApprovalRequirement {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Waived => "waived",
        }
    }
}

impl std::str::FromStr for ApprovalRequirement {
    type Err = ApprovalPolicyStoreError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "required" => Ok(Self::Required),
            "waived" => Ok(Self::Waived),
            other => Err(ApprovalPolicyStoreError::Vocabulary {
                what: "approval_policy.approval",
                value: other.to_owned(),
            }),
        }
    }
}

/// The `self_approval` column's vocabulary. `Forbidden` is the only storable value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelfApproval {
    /// §5.4 / P4: the requester cannot decide the gate that judges their own work. The default
    /// and the only value `set` accepts.
    Forbidden,
    /// A **historical** value only. It exists so a row written before ledger v1-w4b-4 still
    /// reads, and it never authorises a decision: the writer refuses a self-decision
    /// unconditionally and [`set`] refuses to store this variant.
    Allowed,
}

impl SelfApproval {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Forbidden => "forbidden",
            Self::Allowed => "allowed",
        }
    }
}

impl std::str::FromStr for SelfApproval {
    type Err = ApprovalPolicyStoreError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "forbidden" => Ok(Self::Forbidden),
            "allowed" => Ok(Self::Allowed),
            other => Err(ApprovalPolicyStoreError::Vocabulary {
                what: "approval_policy.self_approval",
                value: other.to_owned(),
            }),
        }
    }
}

/// A tenant's gating policy for one action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalPolicy {
    /// The action the policy governs.
    pub action: String,
    /// Whether the action is gated.
    pub approval: ApprovalRequirement,
    /// The requester's exclusion; `Forbidden` for every policy `set` accepts.
    pub self_approval: SelfApproval,
}

impl ApprovalPolicy {
    /// The pair a tenant gets when it has decided nothing.
    pub fn default_for(action: &str) -> Self {
        Self {
            action: action.to_owned(),
            approval: ApprovalRequirement::Required,
            self_approval: SelfApproval::Forbidden,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PolicyRow {
    action: String,
    approval: String,
    self_approval: String,
}

impl PolicyRow {
    fn into_policy(self) -> Result<ApprovalPolicy, ApprovalPolicyStoreError> {
        Ok(ApprovalPolicy {
            approval: ApprovalRequirement::from_str(&self.approval)?,
            self_approval: SelfApproval::from_str(&self.self_approval)?,
            action: self.action,
        })
    }
}

const COLUMNS: &str = "action, approval, self_approval";

/// The tenant's policy for one action, or `None` when it has set none.
///
/// The caller applies the safe default (`ApprovalPolicy::default_for`) — a read never invents a
/// row, and an absent policy must not read as a waiver.
pub async fn get(
    conn: &mut PgConnection,
    tenant: TenantId,
    action: &str,
) -> Result<Option<ApprovalPolicy>, ApprovalPolicyStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM approval_policy WHERE tenant_id = $1 AND action = $2");
    let row: Option<PolicyRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(action)
        .fetch_optional(conn)
        .await?;
    row.map(PolicyRow::into_policy).transpose()
}

/// Every policy the tenant has set, by action name.
pub async fn list(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<ApprovalPolicy>, ApprovalPolicyStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM approval_policy WHERE tenant_id = $1 ORDER BY action");
    let rows: Vec<PolicyRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(conn)
        .await?;
    rows.into_iter().map(PolicyRow::into_policy).collect()
}

/// Insert or replace the tenant's policy for one action.
///
/// ⛔ `self_approval = allowed` is refused (ledger v1-w4b-4). §5.4 / P4 is an invariant, not a
/// permission: reads still parse a historical `allowed` row, but no write may record the value,
/// so `forbidden` is the only storable spelling.
pub async fn set(
    conn: &mut PgConnection,
    tenant: TenantId,
    policy: &ApprovalPolicy,
    updated_by: Option<Uuid>,
) -> Result<ApprovalPolicy, ApprovalPolicyStoreError> {
    if policy.self_approval == SelfApproval::Allowed {
        return Err(ApprovalPolicyStoreError::SelfApprovalAllowed);
    }
    sqlx::query(
        "INSERT INTO approval_policy (tenant_id, action, approval, self_approval, updated_by)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, action) DO UPDATE
            SET approval = excluded.approval,
                self_approval = excluded.self_approval,
                updated_by = excluded.updated_by,
                updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(&policy.action)
    .bind(policy.approval.as_str())
    .bind(policy.self_approval.as_str())
    .bind(updated_by)
    .execute(conn)
    .await?;
    Ok(policy.clone())
}

/// Whether this action runs without an approval. **The gate's read.**
///
/// A missing row, or a row that cannot be parsed, answers `false`: the strict behaviour. A
/// destructive call is never admitted because the policy could not be read.
pub async fn waives(
    conn: &mut PgConnection,
    tenant: TenantId,
    action: &str,
) -> Result<bool, ApprovalPolicyStoreError> {
    match get(conn, tenant, action).await {
        Ok(Some(policy)) => Ok(policy.approval == ApprovalRequirement::Waived),
        Ok(None) => Ok(false),
        Err(ApprovalPolicyStoreError::Vocabulary { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

// ⛔ There is deliberately no `allows_self_approval` reader (ledger v1-w4b-4): §5.4 / P4 is an
// invariant, so no decision path may turn a stored row into a permission. The decision writer
// refuses a requester's own decision unconditionally; a historical `allowed` row reads but is
// inert. Reintroducing a permission read would reopen the finding.
