//! The resolved caller: a principal, the one tenant it acts in, and its memberships.
//!
//! S02 has no tenant switch. `Session` carries the membership list anyway because
//! `/api/me` is a read model (CONTRACT §11 declares `shell/login`), and because a later
//! slice's switch path must not have to reshape the session type.

use std::collections::BTreeMap;

use conveyor_domain::{Id, TenantId};
use serde::{Deserialize, Serialize};

/// `membership.role`.
///
/// There is no `owner`: the tenant owns itself and `admin` is the tenant-admin (George
/// 2026-11-09). A single-user tenant's user is `admin` by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
    Viewer,
    /// The conveyor's synthetic tenant-admin: a machine principal (agent or service) that
    /// creates and runs the system's shared assets. It holds every tenant scope an `admin`
    /// does and is never a human's membership role — `members.set_role` refuses it for a
    /// `human` app_user.
    ///
    /// `rename_all = "lowercase"` would spell it `kataexecutor`; the vocabulary is one word
    /// with an underscore in the database and on the wire.
    #[serde(rename = "kata_executor")]
    KataExecutor,
}

impl Role {
    /// The vocabulary in CHECK order.
    pub const ALL: [Role; 4] = [Role::Admin, Role::Member, Role::Viewer, Role::KataExecutor];

    /// The database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Viewer => "viewer",
            Role::KataExecutor => "kata_executor",
        }
    }

    /// True for an admin. Kept as a method so the two admin predicates read the same.
    pub fn is_admin(self) -> bool {
        matches!(self, Role::Admin)
    }

    /// True for the synthetic tenant-admin. A machine principal only: the role holds every
    /// tenant scope an `admin` does, and a human must not acquire it by a role change.
    pub fn is_kata_executor(self) -> bool {
        matches!(self, Role::KataExecutor)
    }

    /// Parse the database spelling; `None` is a vocabulary the build does not know.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == raw)
    }
}

/// `app_user.kind` — who is behind the credential, not how it was presented.
///
/// A licence bearer and an OIDC login are two ways to *authenticate*; neither may decide
/// the kind. The value is read from the provisioned row, so a machine credential that
/// resolves to a `human` row is refused rather than believed (S06b).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalKind {
    Human,
    Agent,
    Service,
}

impl PrincipalKind {
    /// The vocabulary in CHECK order (`migrations/0001_platform.sql:48-49`).
    pub const ALL: [PrincipalKind; 3] = [
        PrincipalKind::Human,
        PrincipalKind::Agent,
        PrincipalKind::Service,
    ];

    /// The database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalKind::Human => "human",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Service => "service",
        }
    }

    /// Parse the database spelling; `None` is a vocabulary this build does not know.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == raw)
    }

    /// True for a principal that is not a person (an agent or a service account).
    pub fn is_machine(self) -> bool {
        matches!(self, PrincipalKind::Agent | PrincipalKind::Service)
    }

    /// The `action_invocation.actor_kind` spelling.
    ///
    /// A person is `"user"` there — that column predates this enum and its vocabulary is
    /// `user | agent | service` (`actions::invoke::sanitize_actor_kind`), so the mapping is
    /// deliberate rather than a second name for `human`.
    pub fn as_actor_kind(self) -> &'static str {
        match self {
            PrincipalKind::Human => "user",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Service => "service",
        }
    }
}

/// The authenticated human or agent. `subject` is `app_user.oidc_subject` — for a dev
/// principal that is literally `dev:<login>`; for a licence principal it is
/// `licence:<subject_id>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Principal {
    pub user_id: Id,
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
    /// Read from `app_user.kind`; the credential never controls it.
    pub kind: PrincipalKind,
}

/// One tenant the principal belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Membership {
    pub tenant_id: TenantId,
    pub slug: String,
    pub name: String,
    pub role: Role,
}

/// A tenant's overrides of the compiled role→scope table.
///
/// Only a stored decision is carried, keyed by the scope's wire spelling (`projects:write`);
/// absence of an entry is the compiled default (`actions::spec::Scope::granted_to`). The map is
/// loaded once in `authenticate` and is immutable for the request, so a change applies on the
/// next request with nothing to invalidate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopePolicy {
    overrides: BTreeMap<String, bool>,
}

impl ScopePolicy {
    /// Build a policy from stored rows (`scope -> allowed`).
    pub fn new(overrides: BTreeMap<String, bool>) -> Self {
        Self { overrides }
    }

    /// The tenant's decision for `scope`, or `default` when it has made none.
    pub fn allows(&self, scope: &str, default: bool) -> bool {
        self.overrides.get(scope).copied().unwrap_or(default)
    }

    /// Every stored override, for the admin screen and the manifest.
    pub fn overrides(&self) -> &BTreeMap<String, bool> {
        &self.overrides
    }

    /// True when the tenant has made no decision at all.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

/// A machine connector's authority in one tenant (S06d).
///
/// The kind is an open vocabulary (`telegram`, `mcp`, `webhook`, …); the overrides are the
/// tenant's stored differences from that kind's compiled default grant set. This value is
/// **constructed only by the authenticator or the channel loader from `channel_action_grant`** —
/// it is deliberately not `Deserialize` and its fields are private, so no request payload can
/// name or widen a grant. The transport attribution (`sender_ref`) is evidence, never authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelAuthority {
    kind: String,
    sender_ref: Option<String>,
    overrides: BTreeMap<String, bool>,
}

impl ChannelAuthority {
    /// Build an authority from a loaded override map.
    pub fn new(
        kind: impl Into<String>,
        sender_ref: Option<String>,
        overrides: BTreeMap<String, bool>,
    ) -> Self {
        Self {
            kind: kind.into(),
            sender_ref,
            overrides,
        }
    }

    /// The connector kind (`telegram`, `mcp`, …).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The transport's sender reference, for audit only.
    pub fn sender_ref(&self) -> Option<&str> {
        self.sender_ref.as_deref()
    }

    /// The tenant's stored grant overrides; absence is the compiled default.
    pub fn overrides(&self) -> &BTreeMap<String, bool> {
        &self.overrides
    }
}

/// The caller of one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub principal: Principal,
    /// The ONE tenant this request acts in (resolved from `membership`; a principal with several
    /// memberships selects one with `X-Active-Tenant`).
    pub tenant: TenantId,
    pub tenant_slug: String,
    pub tenant_name: String,
    pub role: Role,
    /// Every membership the principal may act in. The request's active one is `tenant`; more than
    /// one row means the caller may switch with `X-Active-Tenant`.
    pub memberships: Vec<Membership>,
    /// `app_user.id` holds a `platform_admin` row. A platform fact, not a tenant role: it gates
    /// the provisioning surface and nothing inside a tenant.
    pub is_super_admin: bool,
    /// The active tenant's stored scope overrides.
    pub policy: ScopePolicy,
    /// The machine connector this credential acts as (`None` for a human session). S06d: a
    /// licence bearer is the tenant's MCP channel, and its authorization is the tenant's
    /// channel grants — not `kata_executor`'s role surface.
    pub channel: Option<ChannelAuthority>,
}

impl Session {
    /// The active tenant's membership, when the principal holds one.
    ///
    /// The list is built from the same rows `authenticate` selected from, so this is `Some` for
    /// every `Session` the authenticator builds; it is `Option` so no caller has to trust that.
    pub fn active(&self) -> Option<&Membership> {
        self.memberships
            .iter()
            .find(|membership| membership.tenant_id == self.tenant)
    }

    /// The tenant's decision for `scope`, falling back to the compiled default.
    pub fn allows(&self, scope: &str, default: bool) -> bool {
        self.policy.allows(scope, default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_vocabulary_round_trips_and_unknown_is_rejected() {
        for role in Role::ALL {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        assert_eq!(Role::parse("Owner"), None, "the vocabulary is lowercase");
        assert_eq!(Role::parse("superadmin"), None);
        // The serde spelling is the database spelling, not `rename_all`'s concatenation.
        assert_eq!(
            serde_json::to_string(&Role::KataExecutor).expect("serialize"),
            "\"kata_executor\""
        );
        assert_eq!(
            serde_json::from_str::<Role>("\"kata_executor\"").expect("deserialize"),
            Role::KataExecutor
        );
        assert!(serde_json::from_str::<Role>("\"kataexecutor\"").is_err());
    }
}
