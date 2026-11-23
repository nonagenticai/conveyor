#![forbid(unsafe_code)]
//! `approval_policies.list|get|set` — the tenant's own decision about how a destructive action is
//! gated (conveyor#282, SPEC_V2 §10.4).
//!
//! The mechanism lives in [`super::approvals::enforce_destructive_gate`]; this family is the only
//! way a tenant changes what that gate does. `approval` and `approver_kind` are the settings:
//! whether an approval is required at all, and which approver kind (human by default) the tenant
//! declares non-governance for the gate. Every default is the strict §11.4 / §5.4 rule, so a
//! tenant that never calls `set` is
//! already correct.
//!
//! ⛔ `self_approval` is **not** a setting: §5.4 / P4 make the requester's exclusion an invariant,
//! so `set` refuses `allowed` (ledger v1-w4b-4) and `forbidden` is the only accepted spelling. A
//! historical `allowed` row still reads, but authorises nothing.
//!
//! ⛔ A policy can only ever **lower** a requirement for one named action. It cannot grant a
//! scope, widen an effect ceiling or authorise an action that the caller's role does not already
//! carry — the scope check happens before this gate is ever reached.

use std::str::FromStr;

use conveyor_domain::DeciderKind;
use conveyor_store as store;
use conveyor_store::approval_policy::{ApprovalPolicy, ApprovalRequirement, SelfApproval};
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
            // A historical `allowed` row still parses — [`SelfApproval`] keeps the variant so old
            // data never breaks a read — but the answer is §5.4's invariant, never the stored
            // value: no read path may report a permission the engine refuses (ledger v1-w4b-4).
            self_approval: SelfApproval::Forbidden.as_str().to_owned(),
            approver_kind: policy.approver_kind.as_str().to_owned(),
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

/// The approver kind a `set` may declare. `human` is the default; every other accepted value is
/// the tenant's explicit non-governance declaration for this gate (SPEC_V2 §5.4).
fn parse_approver_kind(text: &str) -> Result<DeciderKind, ActionError> {
    match text {
        "human" => Ok(DeciderKind::Human),
        "triage_agent" => Ok(DeciderKind::TriageAgent),
        "rule" => Ok(DeciderKind::Rule),
        "agent" => Ok(DeciderKind::Agent),
        "service" => Ok(DeciderKind::Service),
        _ => Err(invalid(
            "approver_kind",
            "invalid_value",
            "must be `human`, `triage_agent`, `rule`, `agent` or `service`",
        )),
    }
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
    // gate only ever consults policies for destructive actions, so anything else is refused here
    // rather than stored and silently ignored.
    if spec.effect != Effect::Destructive {
        return Err(invalid(
            "action",
            "invalid_value",
            format!(
                "{} is not a destructive action, so it has no approval to waive",
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
            "must be `forbidden`: SPEC_V2 §5.4 makes self-approval an invariant, not a tenant \
             permission",
        )
    })?;
    // ⛔ §5.4 / P4 is an invariant, not a permission (ledger v1-w4b-4). The historical `allowed`
    // spelling still parses so old rows read, but no caller may record it as a decision.
    if self_approval == SelfApproval::Allowed {
        return Err(invalid(
            "self_approval",
            "invalid_value",
            "SPEC_V2 §5.4: the requester can never decide their own gate, so `allowed` cannot be \
             stored; use `forbidden`",
        ));
    }
    let approver_kind = parse_approver_kind(&input.approver_kind)?;
    let policy = ApprovalPolicy {
        action: spec.name.to_owned(),
        approval,
        self_approval,
        approver_kind,
    };
    store::approval_policy::set(&mut *tx, ctx.tenant, &policy, Some(ctx.actor_id))
        .await
        .map_err(|error| policy_error(error, "approval_policies.set"))?;
    Ok(ApprovalPolicyOut::from(&policy))
}

/// A policy store failure is a scrubbed `500`, except the store's own refusal of `allowed`
/// self-approval, which stays the client-correctable `422` the handler would have answered.
fn policy_error(
    error: store::approval_policy::ApprovalPolicyStoreError,
    action: &str,
) -> ActionError {
    match error {
        // Defense in depth: the handler refuses `allowed` before it reaches the store, but the
        // store's own refusal is still a client-correctable value error, never a `500`.
        store::approval_policy::ApprovalPolicyStoreError::SelfApprovalAllowed => invalid(
            "self_approval",
            "invalid_value",
            "SPEC_V2 §5.4: the requester can never decide their own gate, so `allowed` cannot be \
             stored; use `forbidden`",
        ),
        store::approval_policy::ApprovalPolicyStoreError::Database(error) => {
            store_error(error, action)
        }
        other => {
            tracing::error!(%other, action, "approval policy");
            ActionError::internal()
        }
    }
}
