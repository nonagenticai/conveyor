#![forbid(unsafe_code)]
//! How a tenant wants its destructive actions gated (conveyor#282, SPEC_V2 §10.4 / §11.4 / §5.4).
//!
//! Two questions per (tenant, action), and nothing else:
//!
//! * `approval` — `required` (the default, and §11.4's rule) or `waived` (this tenant's opt-out);
//! * `self_approval` — `forbidden` (the default, and §5.4's rule) or `allowed`.
//!
//! ⛔ **An absent row is the safe default, never "no policy".** Every reader in this module
//! answers with `required` / `forbidden` when the row is missing, so a tenant that has decided
//! nothing gets the strict behaviour it never opted out of. Only `set` writes a row.
//!
//! `action` is the action's own name, which is the same string `approval.gate` carries for a
//! destructive gate — that equality is what lets the decide path read this policy for the gate it
//! is about to resolve without any caller naming the action again.

use std::str::FromStr;

use conveyor_domain::TenantId;
use conveyor_domain::Timestamp;
use conveyor_domain::development::ApproverKind;
use serde_json::Value;
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

/// Whether the person who requested a gate may decide it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelfApproval {
    /// §5.4: the requester cannot decide the gate that judges their own work. The default.
    Forbidden,
    /// This tenant has opted out for this action.
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

/// One tenant's decision about one action.
///
/// The first two fields are #282's destructive-action policy. The second three are T6's
/// auto-merge extension on the same row (§6.4): an approver kind (`human` by default, so
/// auto-merge is off until a tenant records it), the typed conditions that earn it and its
/// expiry. A row with `approver_kind = 'human'` keeps `conditions = {}` and `expires_at = NULL`.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalPolicy {
    /// The action the policy governs.
    pub action: String,
    /// Whether the action is gated.
    pub approval: ApprovalRequirement,
    /// Whether its requester may decide its gate.
    pub self_approval: SelfApproval,
    /// Who decides the gate: a person (the default) or the tenant's earned policy.
    pub approver_kind: ApproverKind,
    /// The typed conditions an auto policy carries (§6.4).
    pub conditions: Value,
    /// When an auto policy stops applying. An expired policy falls back to a human.
    pub expires_at: Option<Timestamp>,
}

impl ApprovalPolicy {
    /// The pair a tenant gets when it has decided nothing.
    pub fn default_for(action: &str) -> Self {
        Self {
            action: action.to_owned(),
            approval: ApprovalRequirement::Required,
            self_approval: SelfApproval::Forbidden,
            approver_kind: ApproverKind::Human,
            conditions: Value::Object(serde_json::Map::new()),
            expires_at: None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PolicyRow {
    action: String,
    approval: String,
    self_approval: String,
    approver_kind: String,
    conditions: Value,
    expires_at: Option<Timestamp>,
}

impl PolicyRow {
    fn into_policy(self) -> Result<ApprovalPolicy, ApprovalPolicyStoreError> {
        Ok(ApprovalPolicy {
            approval: ApprovalRequirement::from_str(&self.approval)?,
            self_approval: SelfApproval::from_str(&self.self_approval)?,
            action: self.action,
            approver_kind: ApproverKind::from_str(&self.approver_kind).map_err(|_| {
                ApprovalPolicyStoreError::Vocabulary {
                    what: "approval_policy.approver_kind",
                    value: self.approver_kind,
                }
            })?,
            conditions: self.conditions,
            expires_at: self.expires_at,
        })
    }
}

const COLUMNS: &str = "action, approval, self_approval, approver_kind, conditions, expires_at";

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
pub async fn set(
    conn: &mut PgConnection,
    tenant: TenantId,
    policy: &ApprovalPolicy,
    updated_by: Option<Uuid>,
) -> Result<ApprovalPolicy, ApprovalPolicyStoreError> {
    sqlx::query(
        "INSERT INTO approval_policy (tenant_id, action, approval, self_approval, approver_kind,
                                      conditions, expires_at, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (tenant_id, action) DO UPDATE
            SET approval = excluded.approval,
                self_approval = excluded.self_approval,
                approver_kind = excluded.approver_kind,
                conditions = excluded.conditions,
                expires_at = excluded.expires_at,
                updated_by = excluded.updated_by,
                updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(&policy.action)
    .bind(policy.approval.as_str())
    .bind(policy.self_approval.as_str())
    .bind(policy.approver_kind.as_str())
    .bind(&policy.conditions)
    .bind(policy.expires_at)
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

/// Whether the requester of a gate on `action` may decide it themselves. **The decide path's read.**
///
/// A missing row answers `false`: §5.4 is the default, not a permission.
pub async fn allows_self_approval(
    conn: &mut PgConnection,
    tenant: TenantId,
    action: &str,
) -> Result<bool, ApprovalPolicyStoreError> {
    match get(conn, tenant, action).await {
        Ok(Some(policy)) => Ok(policy.self_approval == SelfApproval::Allowed),
        Ok(None) => Ok(false),
        Err(ApprovalPolicyStoreError::Vocabulary { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}
