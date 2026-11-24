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
            required_decisions: policy.required_decisions,
        }
    }
}

/// The project gates `pm::open_gate` opens.
///
/// A gate is not an action: nothing in the registry carries its name, yet its decision is
/// tenant policy data all the same. These are the gate names the `open_gate` call sites use
/// (`charter`, `baseline`, `closure`, `change`, and `design` through `pm.route_gate`).
pub(crate) const GATE_POLICY_NAMES: &[&str] =
    &["charter", "baseline", "design", "closure", "change"];

fn is_gate_policy(name: &str) -> bool {
    GATE_POLICY_NAMES.contains(&name)
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
    // A project gate is a policy name even though the registry does not carry it.
    if !is_gate_policy(&input.action) {
        known_action(&input.action)?;
    }
    let policy = store::approval_policy::get(&mut *tx, ctx.tenant, &input.action)
        .await
        .map_err(|error| policy_error(error, "approval_policies.get"))?
        // ⛔ An unset policy answers with the strict default, never with "no policy": the reader
        // that reported `null` here would invite a caller to treat absence as a waiver.
        .unwrap_or_else(|| ApprovalPolicy::default_for(&input.action));
    Ok(ApprovalPolicyOut::from(&policy))
}

/// `approval_policies.set` — record this tenant's decision about one action.
///
/// A write that waives a gate or names an `auto` approver is itself gated: the caller must name
/// an approved `approval_policies.set` gate for exactly this action (SPEC_V2 §6.4 / §9.3, D34).
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
    // A project gate (`pm::open_gate`'s names) is policy data even though no registry action
    // carries it. It has its own narrower contract below; the action path is unchanged.
    let gate_policy = is_gate_policy(&input.action);
    let spec = if gate_policy {
        None
    } else {
        Some(known_action(&input.action)?)
    };
    // A waiver for an action nothing gates is dead configuration that reads like a decision. The
    // destructive gate only ever consults policies for destructive actions; T6's development
    // gates are the one other governed surface (their policy row is §6.4's auto-merge).
    let governed = spec.is_some_and(|spec| {
        crate::development::is_governed_gate(spec.name)
            || crate::tool_authoring::is_governed_gate(spec.name)
    });
    if let Some(spec) = spec
        && spec.effect != Effect::Destructive
        && !governed
    {
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
    let approver_kind = match input.approver_kind.as_deref() {
        None => ApproverKind::Human,
        Some(raw) => ApproverKind::from_str(raw).map_err(|_| {
            invalid(
                "approver_kind",
                "invalid_value",
                "must be `human`, `auto`, `triage_agent`, `rule`, `agent` or `service`",
            )
        })?,
    };
    if gate_policy {
        // A project gate's policy names who decides and how many decisions resolve it; it can
        // neither waive the gate nor carry an auto-merge condition set.
        if approval != ApprovalRequirement::Required {
            return Err(invalid(
                "approval",
                "invalid_value",
                "a project gate is always required; its policy sets the quorum, never a waiver",
            ));
        }
        if approver_kind == ApproverKind::Auto {
            return Err(invalid(
                "approver_kind",
                "invalid_value",
                "a project gate names a deciding kind; `auto` is a development-gate auto-merge \
                 policy",
            ));
        }
    }
    let conditions = input.conditions.unwrap_or_else(|| json!({}));
    if !conditions.is_object() {
        return Err(invalid(
            "conditions",
            "invalid_value",
            "must be a JSON object",
        ));
    }

    // ⛔ §9.3 / D34: a waiver is never an unconditional off switch. It is an EARNED auto policy —
    // the tenant's rule engine as approver, typed conditions and a future expiry. Anything less
    // would let one `approvals:write` holder disable a destructive gate permanently.
    if approval == ApprovalRequirement::Waived {
        if approver_kind != ApproverKind::Auto {
            return Err(invalid(
                "approver_kind",
                "invalid_value",
                "a waiver must be earned by an `auto` approver kind; a human policy cannot waive \
                 its gate (SPEC_V2 §9.3 / D34)",
            ));
        }
        AutoMergeConditions::parse(&conditions)
            .map_err(|error| invalid("conditions", "invalid_value", error.to_string()))?;
        let expires_at = input.expires_at.ok_or_else(|| {
            invalid(
                "expires_at",
                "missing",
                "an earned waiver is time-bounded and must carry an expiry",
            )
        })?;
        if expires_at <= Utc::now() {
            return Err(invalid(
                "expires_at",
                "invalid_value",
                "an earned waiver must expire in the future",
            ));
        }
    }

    match approver_kind {
        // A human (or the lane's explicit non-human decider) policy does not carry terms.
        // Silently accepting them would store a row that reads like an auto policy.
        ApproverKind::Human
        | ApproverKind::TriageAgent
        | ApproverKind::Rule
        | ApproverKind::Agent
        | ApproverKind::Service => {
            if conditions
                .as_object()
                .is_some_and(|object| !object.is_empty())
                || input.expires_at.is_some()
            {
                return Err(invalid(
                    "conditions",
                    "invalid_value",
                    "a non-auto policy carries no conditions and no expiry; use `auto`",
                ));
            }
        }
        // `waived` on an auto row is the earned waiver validated above. `required` is §6.4's
        // auto-merge policy and keeps its full validation set.
        ApproverKind::Auto if approval == ApprovalRequirement::Required => {
            if !governed {
                return Err(invalid(
                    "approver_kind",
                    "invalid_value",
                    format!(
                        "{} is not a governed gate; an auto policy applies only to a \
                         development gate or the tool-authoring publication gate",
                        input.action
                    ),
                ));
            }
            // A development gate carries §6.4's typed auto-merge conditions; the publication
            // gate carries `max_effect`, the highest effect a rule may publish. The action that
            // evaluates the policy reads the shape it owns.
            if crate::development::is_governed_gate(input.action.as_str()) {
                AutoMergeConditions::parse(&conditions)
                    .map_err(|error| invalid("conditions", "invalid_value", error.to_string()))?;
            } else if let Some(raw) = conditions
                .get("max_effect")
                .and_then(serde_json::Value::as_str)
                && Effect::parse(raw).is_none()
            {
                return Err(invalid(
                    "conditions",
                    "invalid_value",
                    "max_effect must be read, write, spend or destructive",
                ));
            }
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
        }
        ApproverKind::Auto => {}
    }

    let required_decisions = input.required_decisions.unwrap_or(1);
    if !(1..=255).contains(&required_decisions) {
        return Err(invalid(
            "required_decisions",
            "invalid_value",
            "a gate needs between 1 and 255 decisions",
        ));
    }
    if approver_kind == ApproverKind::Auto && required_decisions != 1 {
        return Err(invalid(
            "required_decisions",
            "invalid_value",
            "an auto policy is one rule decision; the quorum is 1",
        ));
    }
    // §6.4 / D34: the policy change is itself a gated change. Both a waiver and an auto policy
    // reach this point, so no `approvals:write` holder can switch a gate off — or hand it to the
    // rule engine — behind a plain write. The approval is opened by the tenant's governance path
    // (`approvals.request`, gate `approval_policies.set`) and its target is the action this
    // policy governs, so an approval for one gate can never authorise another.
    if approval == ApprovalRequirement::Waived || approver_kind == ApproverKind::Auto {
        super::approvals::require_approval_tx(
            &mut *tx,
            &ctx,
            "approval_policies.set",
            input.action.as_str(),
            ctx.approval_id,
        )
        .await?;
    }
    let policy = ApprovalPolicy {
        action: input.action.clone(),
        approval,
        self_approval,
        approver_kind,
        conditions,
        expires_at: input.expires_at,
        required_decisions,
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
