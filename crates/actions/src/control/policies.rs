#![forbid(unsafe_code)]
//! `approval_policies.list|get|set` — the tenant's own decision about how a destructive action is
//! gated (conveyor#282, SPEC_V2 §10.4).
//!
//! The mechanism lives in [`super::approvals::enforce_destructive_gate`]; this family is the only
//! way a tenant changes what that gate does. Two settings per action, and nothing else:
//! whether an approval is required at all, and whether the person who asked for a gate may decide
//! it. Both defaults are the strict §11.4 / §5.4 rule, so a tenant that never calls `set` is
//! already correct.
//!
//! ⛔ A policy can only ever **lower** a requirement for one named action. It cannot grant a
//! scope, widen an effect ceiling or authorise an action that the caller's role does not already
//! carry — the scope check happens before this gate is ever reached.

use std::str::FromStr;

use chrono::Utc;
use conveyor_domain::development::{ApproverKind, AutoMergeConditions};
use conveyor_store as store;
use conveyor_store::approval_policy::{ApprovalPolicy, ApprovalRequirement, SelfApproval};
use serde_json::json;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::Effect;

use super::views::{ApprovalPolicyGetIn, ApprovalPolicyOut, ApprovalPolicySetIn};
use super::{invalid, store_error};

impl From<&ApprovalPolicy> for ApprovalPolicyOut {
    fn from(policy: &ApprovalPolicy) -> Self {
        Self {
            action: policy.action.clone(),
            approval: policy.approval.as_str().to_owned(),
            self_approval: policy.self_approval.as_str().to_owned(),
            approver_kind: policy.approver_kind.as_str().to_owned(),
            conditions: policy.conditions.clone(),
            expires_at: policy.expires_at,
        }
    }
}

/// The action this policy names, refused unless the registry carries it.
///
/// A row for an unknown name is unreadable configuration: it would sit in the table looking like
/// a decision while no call ever consults it.
fn known_action(name: &str) -> Result<&'static crate::spec::ActionSpec, ActionError> {
    crate::registry::registry().get(name).ok_or_else(|| {
        invalid(
            "action",
            "invalid_value",
            format!("no action named {name:?}"),
        )
    })
}

/// `approval_policies.list` — every policy this tenant has set, by action name.
#[crate::action(
    name = "approval_policies.list",
    version = 1,
    kind = "internal",
    scopes = ["approvals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn approval_policies_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: crate::views::EmptyIn,
) -> Result<Vec<ApprovalPolicyOut>, ActionError> {
    let policies = store::approval_policy::list(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| policy_error(error, "approval_policies.list"))?;
    Ok(policies.iter().map(ApprovalPolicyOut::from).collect())
}

/// `approval_policies.get` — one action's policy, or the default when the tenant set none.
#[crate::action(
    name = "approval_policies.get",
    version = 1,
    kind = "internal",
    scopes = ["approvals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn approval_policies_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ApprovalPolicyGetIn,
) -> Result<ApprovalPolicyOut, ActionError> {
    let spec = known_action(&input.action)?;
    let policy = store::approval_policy::get(&mut *tx, ctx.tenant, spec.name)
        .await
        .map_err(|error| policy_error(error, "approval_policies.get"))?
        // ⛔ An unset policy answers with the strict default, never with "no policy": the reader
        // that reported `null` here would invite a caller to treat absence as a waiver.
        .unwrap_or_else(|| ApprovalPolicy::default_for(spec.name));
    Ok(ApprovalPolicyOut::from(&policy))
}

/// `approval_policies.set` — record this tenant's decision about one action.
#[crate::action(
    name = "approval_policies.set",
    version = 1,
    kind = "internal",
    scopes = ["approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn approval_policies_set(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ApprovalPolicySetIn,
) -> Result<ApprovalPolicyOut, ActionError> {
    let spec = known_action(&input.action)?;
    // A waiver for an action nothing gates is dead configuration that reads like a decision. The
    // destructive gate only ever consults policies for destructive actions; T6's development
    // gates are the one other governed surface (their policy row is §6.4's auto-merge).
    let governed = crate::development::is_governed_gate(spec.name);
    if spec.effect != Effect::Destructive && !governed {
        return Err(invalid(
            "action",
            "invalid_value",
            format!(
                "{} is not a destructive action or a governed gate, so it has no approval to \
                 waive",
                spec.name
            ),
        ));
    }
    let approval = ApprovalRequirement::from_str(&input.approval).map_err(|_| {
        invalid(
            "approval",
            "invalid_value",
            "must be `required` or `waived`",
        )
    })?;
    let self_approval = SelfApproval::from_str(&input.self_approval).map_err(|_| {
        invalid(
            "self_approval",
            "invalid_value",
            "must be `forbidden` or `allowed`",
        )
    })?;
    let approver_kind = match input.approver_kind.as_deref() {
        None => ApproverKind::Human,
        Some(raw) => ApproverKind::from_str(raw).map_err(|_| {
            invalid(
                "approver_kind",
                "invalid_value",
                "must be `human` or `auto`",
            )
        })?,
    };
    let conditions = input.conditions.unwrap_or_else(|| json!({}));
    if !conditions.is_object() {
        return Err(invalid(
            "conditions",
            "invalid_value",
            "must be a JSON object",
        ));
    }

    match approver_kind {
        ApproverKind::Human => {
            // A human policy does not carry terms. Silently accepting them would store a row
            // that reads like an auto policy while a person still decides.
            if conditions
                .as_object()
                .is_some_and(|object| !object.is_empty())
                || input.expires_at.is_some()
            {
                return Err(invalid(
                    "conditions",
                    "invalid_value",
                    "a `human` policy carries no conditions and no expiry; use `auto`",
                ));
            }
        }
        ApproverKind::Auto => {
            if !governed {
                return Err(invalid(
                    "approver_kind",
                    "invalid_value",
                    format!(
                        "{} is not a governed development gate; auto-merge applies only to \
                         development review/merge/deploy gates",
                        spec.name
                    ),
                ));
            }
            // Auto-merge lowers no approval: the row is the tenant's earned decision, not a
            // waiver. A waiver would bypass the gate and its conditions entirely.
            if approval != ApprovalRequirement::Required {
                return Err(invalid(
                    "approval",
                    "invalid_value",
                    "an auto-merge policy still requires the gate; it never waives it",
                ));
            }
            AutoMergeConditions::parse(&conditions)
                .map_err(|error| invalid("conditions", "invalid_value", error.to_string()))?;
            let expires_at = input.expires_at.ok_or_else(|| {
                invalid(
                    "expires_at",
                    "missing",
                    "an auto-merge policy is earned for a period and must carry an expiry",
                )
            })?;
            if expires_at <= Utc::now() {
                return Err(invalid(
                    "expires_at",
                    "invalid_value",
                    "an auto-merge policy must expire in the future",
                ));
            }
            // §6.4: the policy is itself a gated change. The approval is opened by the tenant's
            // governance path (`approvals.request`, gate `approval_policies.set`) and its target
            // is the action this policy governs, so an approval for one gate can never authorise
            // another.
            super::approvals::require_approval_tx(
                &mut *tx,
                &ctx,
                "approval_policies.set",
                spec.name,
                ctx.approval_id,
            )
            .await?;
        }
    }

    let policy = ApprovalPolicy {
        action: spec.name.to_owned(),
        approval,
        self_approval,
        approver_kind,
        conditions,
        expires_at: input.expires_at,
    };
    store::approval_policy::set(&mut *tx, ctx.tenant, &policy, Some(ctx.actor_id))
        .await
        .map_err(|error| policy_error(error, "approval_policies.set"))?;
    Ok(ApprovalPolicyOut::from(&policy))
}

/// A policy store failure is never a client error except for the vocabulary the action already
/// validated — so it is a scrubbed `500`, exactly like every other store failure here.
fn policy_error(
    error: store::approval_policy::ApprovalPolicyStoreError,
    action: &str,
) -> ActionError {
    match error {
        store::approval_policy::ApprovalPolicyStoreError::Database(error) => {
            store_error(error, action)
        }
        other => {
            tracing::error!(%other, action, "approval policy");
            ActionError::internal()
        }
    }
}
