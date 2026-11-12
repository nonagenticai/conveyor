#![forbid(unsafe_code)]
//! The connector standard's authorization policy (S06d).
//!
//! Three facts live here, each in one place because HTTP, `/mcp`, the inbound channel route
//! and the admin screen must not be able to disagree about them:
//!
//! - **Compiled defaults.** A machine channel with no stored override may invoke exactly the
//!   actions its kind's [`default_actions`] names. The MCP default is the former `AGENT_TOOLS`
//!   list, unchanged, so the S06b surface is not widened by this slice; the Telegram/webhook
//!   default is the operator lifecycle the owner specified (create, run, approve, answer,
//!   close, rerun) and nothing administrative.
//! - **The floor cannot be granted.** [`ActionSpec::machine_forbidden`] is computed from the
//!   action's declared scopes/resource, never from a role. A tenant-admin cannot grant an eval
//!   sign-off, a held-out read, tenant/channel/member administration or workflow authoring to
//!   any machine channel. This is what keeps `evals:approve` a person's act even though the
//!   channel's membership role is `kata_executor`.
//! - **Absent row = compiled default.** `channel_action_grant` stores only the tenant's
//!   differences, so an untouched tenant has zero rows and a new deployment does not need a
//!   data migration to change a default.
//!
//! A connector adapter declares its own [`ConnectorSpec`](connectors::ConnectorSpec) in
//! `channel::connectors`; it supplies its default list and config schema. Nothing in this
//! module knows Telegram's wire format.

use conveyor_auth::ChannelAuthority;

pub mod command;
pub mod connectors;

/// The MCP channel's compiled default: exactly the former `AGENT_TOOLS`, so this slice is
/// capability-neutral for the agent until a tenant chooses otherwise. A tenant-admin may add
/// or remove entries through `channels.policy_set`; the floor above still wins.
///
/// Every entry is a deliberate elevation over `kata_executor`'s role surface: the credential is
/// long-lived and unattended, so "an admin may do it" is not a reason to expose it. The
/// human-only actions (`eval_signoffs.*`, `eval_runs.start|cancel`, `eval_results.label`) and
/// the held-out reads stay out — see [`crate::spec::ActionSpec::machine_forbidden`].
pub const MCP_DEFAULT_ACTIONS: [&str; 43] = [
    "approvals.decide",
    "approvals.list",
    "approvals.request",
    "boards.get",
    "capabilities.get",
    "capabilities.list",
    "capability_agents.get",
    "capability_agents.list",
    "cards.create",
    "cards.get",
    "eval_audits.get",
    "eval_cases.create",
    "eval_cases.list",
    "eval_findings.list",
    "eval_results.list",
    "eval_runs.get",
    "eval_runs.report",
    "evals.get",
    "evals.list",
    "events.list",
    "inference_calls.list",
    "inference_usage.report",
    "knowledge.query",
    "memory.recall",
    "process_runs.start",
    "processes.create",
    "processes.get",
    "processes.list",
    "processes.publish",
    "processes.retire",
    "processes.update",
    "projects.close",
    "projects.create",
    "projects.execute",
    "projects.get",
    "prompt_audits.get",
    "prompt_audits.list",
    "prompt_audits.start",
    "prompt_findings.list",
    "runs.cancel",
    "runs.create",
    "runs.finish",
    "runs.list",
];

/// The operator lifecycle a provisioned text connector may invoke by default: create, run,
/// approve, answer, close, rerun — and only the reads needed to address them. Everything
/// administrative is both absent here and refused by the floor.
pub const CONNECTOR_DEFAULT_ACTIONS: [&str; 18] = [
    "approvals.decide",
    "approvals.list",
    "boards.get",
    "cards.get",
    "cards.list",
    "events.list",
    "process_runs.start",
    "processes.get",
    "processes.list",
    "projects.close",
    "projects.create",
    "projects.get",
    "runs.cancel",
    "runs.create",
    "runs.finish",
    "runs.get",
    "runs.list",
    "runs.start",
];

/// The resources no machine channel may ever touch, whatever a tenant stores.
///
/// `channels` is the important one for self-escalation: a channel that could call
/// `channels.policy_set` could grant itself anything the floor still allows, so the
/// administration surface is closed to it outright. `workflows` is here because workflow
/// publish has no publisher-authority check yet (procedures does); it reopens when that gate
/// exists.
pub const MACHINE_DENIED_RESOURCES: [&str; 14] = [
    "actions",
    "catalogues",
    "channels",
    "departments",
    "invites",
    "jobs",
    "licences",
    "members",
    // The role matrix is administration: a channel that could edit it could widen every
    // human role, which is the same escalation as editing its own grant table.
    "roles",
    "schedules",
    "secrets",
    "tenants",
    // Outbound webhook configuration is a data-exfiltration primitive; keep it a person's act.
    "webhooks",
    "workflows",
];

/// Actions closed to every machine for a reason the scope rule cannot express: the held-out
/// reads declare `evals:read`, and opening an eval sign-off gate is kept a person's act too.
pub const MACHINE_DENIED_ACTIONS: [&str; 4] = [
    // A held-out read and an eval sign-off gate are a person's.
    "eval_results.get",
    "eval_runs.export",
    "eval_signoffs.request",
    // Board membership is access control, not project content.
    "boards.set_member",
];

/// The one scope whose presence makes an action a person's act (CONTRACT §20).
pub const MACHINE_DENIED_SCOPE: &str = "evals:approve";

/// The compiled default action set for a connector kind.
///
/// The adapter registry is the source: a kind this build does not serve has an empty default
/// (fail closed — the tenant may still grant actions explicitly, but only through an action
/// that exists).
pub fn default_actions(kind: &str) -> &'static [&'static str] {
    connectors::find(kind)
        .map(|connector| connector.default_actions)
        .unwrap_or(&[])
}

/// Whether a channel authority may invoke `action` (the floor is checked separately by
/// [`crate::spec::ActionSpec::permits_channel`]).
pub fn allows(authority: &ChannelAuthority, action: &str) -> bool {
    authority
        .overrides()
        .get(action)
        .copied()
        .unwrap_or_else(|| default_actions(authority.kind()).contains(&action))
}

/// The resource half of an action name (`runs.create` → `runs`).
pub fn resource_of(action: &str) -> &str {
    action
        .split_once('.')
        .map(|(resource, _)| resource)
        .unwrap_or(action)
}

/// Whether an action's resource is closed to machine channels.
pub fn is_denied_resource(resource: &str) -> bool {
    MACHINE_DENIED_RESOURCES.contains(&resource)
}

/// Whether an action is explicitly held out of every machine grant.
pub fn is_denied_action(action: &str) -> bool {
    MACHINE_DENIED_ACTIONS.contains(&action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_domain::channel::MCP;

    #[test]
    fn every_default_is_inside_the_floor_and_known_to_the_registry() {
        let registry = crate::registry();
        for kind in [
            conveyor_domain::channel::MCP,
            conveyor_domain::channel::TELEGRAM,
            conveyor_domain::channel::WEBHOOK,
        ] {
            for action in default_actions(kind) {
                let spec = registry.get(action).unwrap_or_else(|| {
                    panic!("{kind} default {action} is not a registered action")
                });
                assert!(
                    !spec.machine_forbidden(),
                    "{kind} default {action} is machine-forbidden and must not be a default"
                );
                assert!(
                    spec.permits(conveyor_auth::Role::KataExecutor),
                    "{kind} default {action} declares scopes kata_executor does not hold"
                );
            }
        }
    }

    #[test]
    fn an_override_beats_the_default_and_an_unknown_kind_has_none() {
        let authority = |kind: &str, action: &str, allowed: bool| {
            let mut overrides = std::collections::BTreeMap::new();
            overrides.insert(action.to_owned(), allowed);
            ChannelAuthority::new(kind, None, overrides)
        };
        assert!(!allows(
            &authority(MCP, "runs.create", false),
            "runs.create"
        ));
        assert!(allows(&authority(MCP, "runs.create", true), "runs.create"));
        assert!(!allows(
            &ChannelAuthority::new("unregistered", None, Default::default()),
            "runs.create"
        ));
    }

    #[test]
    fn the_kind_spelling_rule_is_the_database_check() {
        use conveyor_domain::channel::is_valid_kind;
        assert!(is_valid_kind("telegram"));
        assert!(is_valid_kind("whats_app2"));
        assert!(!is_valid_kind("Telegram"));
        assert!(!is_valid_kind("a"));
        assert!(!is_valid_kind("with-dash"));
    }
}
