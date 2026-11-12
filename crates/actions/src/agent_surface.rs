#![forbid(unsafe_code)]
//! The copilot's projection of the action registry (S10 §5.2).
//!
//! Not a second registry. The surface is a **query** over the one `ActionSpec` set — scopes, kind
//! and tenancy — so `GET /api/copilot/actions` and `/mcp`'s `tools/list` are two renderings of the
//! same answer, and a change to an action changes both or neither (S10-D1).
//!
//! Three filters, each with a reason:
//!
//! * **Scope** — a machine channel principal is authorized by the tenant's grant set (S06d),
//!   not by its membership role; every other principal by its role's scopes. This mirrors
//!   `/mcp` (`crates/actions/src/mcp.rs`) exactly, which is what makes the equality test possible.
//! * **Kind** — `workflow` actions are declared but have no handler yet (S13); offering one to a
//!   model would be a tool that cannot run.
//! * **Tenancy** — a tenant-bound caller cannot satisfy `Tenancy::Platform`.
//!
//! The projection **omits** rather than refuses (fr's `SurfaceRefusal`): an action outside the
//! surface is absent from the list, and calling it is `action-not-found`, never `403` (S04-D6).

use conveyor_auth::{ChannelAuthority, PrincipalKind, Role};
use serde::{Deserialize, Serialize};

use crate::registry::Registry;
use crate::spec::{ActionKind, ActionSpec, Tenancy};

/// One action as the copilot sees it: the manifest entry plus the hints and the effect words the
/// proposal path needs.
#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SurfaceEntry {
    /// `<resource>.<verb>`.
    pub name: String,
    /// The spec version.
    pub version: u32,
    /// `internal | external`.
    pub kind: String,
    /// sha256 of the canonical spec projection — the same value `/mcp` carries.
    pub spec_hash: String,
    /// The scopes a caller must hold.
    pub scopes: Vec<String>,
    /// Whether a replay with the same key is safe.
    pub idempotent: bool,
    /// The caller's deadline.
    pub timeout_ms: u32,
    /// `read | write | destructive`.
    pub effect: String,
    /// `none | card | card-typed`.
    pub confirm: String,
    /// The copilot hints declared with the action.
    pub agent: AgentHintsView,
}

/// [`crate::AgentHints`] as the wire spells it.
#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentHintsView {
    /// `none | core | searchable`.
    pub tier: String,
    /// Whether a tool call must become a proposal even when the effect alone would not.
    pub proposal_required: bool,
}

/// Whether this caller may see `spec` at all.
///
/// A machine caller is judged by its channel grants; a human by its role. The `Service` kind
/// without a channel is an engine-style caller and falls back to the role, exactly as
/// `invoke`'s `Internal` lane does.
pub fn exposes(
    spec: &ActionSpec,
    kind: PrincipalKind,
    role: Role,
    channel: Option<&ChannelAuthority>,
) -> bool {
    matches!(spec.kind, ActionKind::Internal | ActionKind::External)
        && satisfiable(spec.tenancy)
        && match channel {
            Some(authority) => spec.permits_channel(authority),
            None if kind.is_machine() => false,
            None => spec.permits(role),
        }
}

/// Whether a tenant-bound caller can satisfy this resolution scope.
pub fn satisfiable(tenancy: Tenancy) -> bool {
    !matches!(tenancy, Tenancy::Platform)
}

/// The surface for one caller, in registry order.
pub fn surface<'a>(
    registry: &'a Registry,
    kind: PrincipalKind,
    role: Role,
    channel: Option<&ChannelAuthority>,
) -> Vec<&'a ActionSpec> {
    registry
        .iter()
        .filter(|spec| exposes(spec, kind, role, channel))
        .collect()
}

/// The one declared exemption from the coverage rule (S10-D6): the copilot's chat transport is a
/// route, not an operation — streaming does not fit an invocation.
pub const TRANSPORT_EXEMPTIONS: [&str; 1] = ["copilot.chat"];

/// The coverage gate: every operation a screen can trigger must resolve to a surface action or a
/// declared exemption.
///
/// Returns the names that resolve to neither. Returning them — rather than a bool — is what lets the
/// test report an injected unmapped operation **by name**, so a new screen element that forgets its
/// action fails loudly instead of silently widening the exemption list.
pub fn unmapped(operations: &[&str], surface: &[&ActionSpec]) -> Vec<String> {
    let names: std::collections::BTreeSet<&str> = surface.iter().map(|spec| spec.name).collect();
    operations
        .iter()
        .filter(|operation| {
            !names.contains(*operation) && !TRANSPORT_EXEMPTIONS.contains(operation)
        })
        .map(|operation| (*operation).to_owned())
        .collect()
}

/// The wire form of one action.
pub fn entry(spec: &ActionSpec) -> SurfaceEntry {
    SurfaceEntry {
        name: spec.name.to_owned(),
        version: spec.version,
        kind: spec.kind.as_str().to_owned(),
        spec_hash: spec.spec_hash(),
        scopes: spec.scope_names().into_iter().map(str::to_owned).collect(),
        idempotent: spec.idempotent,
        timeout_ms: spec.timeout_ms,
        effect: spec.effect.as_str().to_owned(),
        confirm: spec.confirm.as_str().to_owned(),
        agent: AgentHintsView {
            tier: spec.agent.tier.as_str().to_owned(),
            proposal_required: spec.agent.proposal_required,
        },
    }
}

/// The surface as the route returns it.
pub fn entries(
    registry: &Registry,
    kind: PrincipalKind,
    role: Role,
    channel: Option<&ChannelAuthority>,
) -> Vec<SurfaceEntry> {
    surface(registry, kind, role, channel)
        .into_iter()
        .map(entry)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::registry;

    #[test]
    fn the_surface_is_the_scope_projection_and_never_a_second_registry() {
        // A member is a subset of an admin, and nothing appears twice.
        let member = surface(registry(), PrincipalKind::Human, Role::Member, None);
        let admin = surface(registry(), PrincipalKind::Human, Role::Admin, None);
        assert!(!member.is_empty());
        assert!(member.len() <= admin.len());
        for spec in &member {
            assert!(admin.iter().any(|other| other.name == spec.name));
        }
        // The kind filter excludes nothing today (no workflow action is registered), which is
        // exactly why it is asserted rather than assumed: the first S13 action must change this.
        assert!(admin.iter().all(|spec| spec.kind != ActionKind::Workflow));
    }

    #[test]
    fn a_machine_sees_exactly_its_channel_default() {
        let authority = ChannelAuthority::new("mcp", None, Default::default());
        let machine = surface(
            registry(),
            PrincipalKind::Agent,
            Role::Member,
            Some(&authority),
        );
        assert_eq!(machine.len(), crate::channel::MCP_DEFAULT_ACTIONS.len());
        assert!(machine.iter().all(|spec| spec.permits_channel(&authority)));
    }

    #[test]
    fn hints_default_to_none_and_are_not_in_the_contract_hash() {
        let spec = registry()
            .iter()
            .find(|spec| spec.name == "cards.get")
            .expect("cards.get is registered");
        let before = spec.spec_hash();
        let mut hinted = ActionSpec {
            agent: crate::AgentHints {
                tier: crate::AgentTier::Core,
                proposal_required: true,
            },
            ..spec.clone()
        };
        assert_eq!(
            hinted.spec_hash(),
            before,
            "a hint must not move the contract hash"
        );
        hinted.agent.tier = crate::AgentTier::Searchable;
        assert_eq!(hinted.spec_hash(), before);
    }
}
