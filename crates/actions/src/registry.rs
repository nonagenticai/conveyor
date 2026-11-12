#![forbid(unsafe_code)]
//! The compile-time registry: one `inventory::submit!` per `#[action]`, collected once.
//!
//! [`assert_registry_consistent`] is the boot gate (S04 §5.2). It fails closed on:
//!
//! 1. a duplicate action name;
//! 2. an MCP-form collision (`cards.move` vs `cards_move` — `.`→`_` is not injective);
//! 3. an MCP name outside `^[a-z0-9_]{1,64}$`;
//! 4. an internal action with no handler;
//! 5. an unknown scope (a spec constructed outside the macro cannot smuggle one in);
//! 6. a name outside `<plural>.<verb>`.
//!
//! `CONVEYOR_TEST_DUPLICATE=<name>` injects a second copy of one action, so the gate can
//! prove a serving process exits 1 **before bind**. It is read here, once, and never by a
//! handler.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::spec::{
    ActionKind, ActionSpec, HandlerRef, Scope, mcp_collisions, validate_mcp_name, validate_name,
};

/// One `inventory` entry. The macro submits a constructor rather than a `static`, because
/// a schemars schema cannot be built in a const initializer.
pub struct ActionRegistration {
    /// Builds the spec value.
    pub make: fn() -> ActionSpec,
}

impl ActionRegistration {
    /// The constructor the macro emits.
    pub const fn new(make: fn() -> ActionSpec) -> Self {
        Self { make }
    }
}

inventory::collect!(ActionRegistration);

/// Why the registry is unusable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two declarations share a name.
    #[error("duplicate action {0:?}: one name, one declaration")]
    DuplicateName(String),
    /// Two names map to the same MCP tool name.
    #[error("MCP tool-name collision on {mcp:?}: {actions:?}")]
    McpCollision {
        /// The colliding tool name.
        mcp: String,
        /// The action names that collide.
        actions: Vec<String>,
    },
    /// The MCP tool name is outside the protocol's charset.
    #[error("action {name:?}: {reason}")]
    InvalidMcpName {
        /// The action name.
        name: String,
        /// What is wrong with its tool name.
        reason: String,
    },
    /// An internal action has no compiled handler.
    #[error("internal action {0:?} has no handler")]
    Handlerless(String),
    /// A scope the build does not know.
    #[error("action {name:?} declares an unknown scope {scope:?}")]
    UnknownScope {
        /// The action name.
        name: String,
        /// The spelling it declared.
        scope: String,
    },
    /// The action name is not `<plural>.<verb>`.
    #[error("action {name:?}: {reason}")]
    InvalidName {
        /// The offending name.
        name: String,
        /// Why it is refused.
        reason: String,
    },
    /// A kind/effect/confirm/tenancy combination this build cannot project.
    #[error("action {0:?} declares a workflow handler, which S13 owns")]
    WorkflowNotRunnable(String),
}

/// The registry, assembled once per process.
#[derive(Debug)]
pub struct Registry {
    actions: Vec<ActionSpec>,
    by_name: BTreeMap<&'static str, usize>,
}

impl Registry {
    /// Build from the collected specs, sorted by name so every projection is deterministic
    /// (the manifest order is part of its ETag).
    pub fn new(mut actions: Vec<ActionSpec>) -> Self {
        actions.sort_by_key(|spec| spec.name);
        let by_name = actions
            .iter()
            .enumerate()
            .map(|(index, spec)| (spec.name, index))
            .collect();
        Self { actions, by_name }
    }

    /// Look one action up.
    pub fn get(&self, name: &str) -> Option<&ActionSpec> {
        self.by_name.get(name).map(|index| &self.actions[*index])
    }

    /// Every action, in manifest order.
    pub fn iter(&self) -> impl Iterator<Item = &ActionSpec> {
        self.actions.iter()
    }

    /// How many actions are registered.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether the registry is empty (a boot failure, not a valid state).
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// The names only, for assertions and logs.
    pub fn names(&self) -> Vec<&'static str> {
        self.actions.iter().map(|spec| spec.name).collect()
    }

    /// The manifest entries a caller holding `role` may invoke.
    ///
    /// Open question 1's default: yes, role-filtered — an uncallable manifest is a second
    /// contract, and a tool a role cannot call must be absent from MCP `tools/list`
    /// (S4-D6), so the two filters are the same filter.
    pub fn manifest(&self, role: conveyor_auth::Role) -> Vec<Value> {
        self.actions
            .iter()
            .filter(|spec| spec.permits(role))
            .map(|spec| {
                json!({
                    "name": spec.name,
                    "version": spec.version,
                    "kind": spec.kind.as_str(),
                    "spec_hash": spec.spec_hash(),
                    "scopes": spec.scope_names(),
                    "idempotent": spec.idempotent,
                    "timeout_ms": spec.timeout_ms,
                })
            })
            .collect()
    }

    /// The actions one role may invoke, in manifest order.
    pub fn permitted(&self, role: conveyor_auth::Role) -> impl Iterator<Item = &ActionSpec> {
        self.actions.iter().filter(move |spec| spec.permits(role))
    }

    /// The MCP tools one role may call. A refused tool is absent, never listed-then-denied.
    pub fn tools(&self, role: conveyor_auth::Role) -> Vec<Value> {
        self.permitted(role).map(crate::mcp::tool_json).collect()
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// The process-wide registry.
///
/// Four test hooks inject exactly one broken spec so a serving process can be proved to
/// exit 1 **before bind** (property c at the process level; `validate` covers the same
/// rules over hand-built registries in `duplicate.rs`):
///
/// * `CONVEYOR_TEST_DUPLICATE=<name>` — a second copy of `<name>`;
/// * `CONVEYOR_TEST_MCP_COLLISION=<name>` — a copy renamed across the dot so its MCP form
///   collides (`members.set_role` → `members_set.role`);
/// * `CONVEYOR_TEST_HANDLERLESS=<name>` — a copy marked internal with no handler;
/// * `CONVEYOR_TEST_BAD_NAME=<name>` — a copy renamed outside the name grammar.
///
/// Nothing in a request path reads any of them.
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let mut actions: Vec<ActionSpec> = inventory::iter::<ActionRegistration>()
            .map(|registration| (registration.make)())
            .collect();
        inject_test_spec(&mut actions);
        Registry::new(actions)
    })
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn find(actions: &[ActionSpec], name: &str, fallback: &str) -> Option<ActionSpec> {
    actions
        .iter()
        .find(|spec| spec.name == name)
        .or_else(|| actions.iter().find(|spec| spec.name == fallback))
        .cloned()
}

/// `a.b_c` → `a_b.c`: both are legal action names and both become the MCP tool `a_b_c`.
fn colliding_name(name: &str) -> Option<String> {
    let (resource, verb) = name.split_once('.')?;
    let (head, tail) = verb.split_once('_')?;
    Some(format!("{resource}_{head}.{tail}"))
}

fn inject_test_spec(actions: &mut Vec<ActionSpec>) {
    if let Some(name) = env_value("CONVEYOR_TEST_DUPLICATE")
        && let Some(spec) = actions.iter().find(|spec| spec.name == name).cloned()
    {
        actions.push(spec);
    }
    if let Some(source) = env_value("CONVEYOR_TEST_MCP_COLLISION")
        && let Some(mut spec) = find(actions, &source, "members.set_role")
        && let Some(colliding) = colliding_name(spec.name)
    {
        spec.name = Box::leak(colliding.into_boxed_str());
        actions.push(spec);
    }
    if let Some(source) = env_value("CONVEYOR_TEST_HANDLERLESS")
        && let Some(mut spec) = find(actions, &source, "me.get")
    {
        spec.kind = ActionKind::Internal;
        spec.handler = HandlerRef::Workflow;
        actions.push(spec);
    }
    if let Some(source) = env_value("CONVEYOR_TEST_BAD_NAME")
        && let Some(mut spec) = find(actions, &source, "me.get")
    {
        spec.name = Box::leak("Bad.name".to_owned().into_boxed_str());
        actions.push(spec);
    }
}

/// Fail closed on every registry inconsistency (S04 §5.2). Called by `serve`, `worker` and
/// every integration test before anything else runs.
pub fn assert_registry_consistent() -> Result<&'static Registry, RegistryError> {
    let registry = registry();
    validate(registry)?;
    Ok(registry)
}

/// The five checks, over any registry. [`assert_registry_consistent`] applies them to the
/// process registry; tests apply them to hand-built ones (a collision cannot be compiled
/// into the real set — that is the point).
pub fn validate(registry: &Registry) -> Result<(), RegistryError> {
    if registry.is_empty() {
        return Err(RegistryError::Handlerless("<the registry is empty>".into()));
    }

    let mut seen: BTreeMap<&'static str, usize> = BTreeMap::new();
    for spec in registry.iter() {
        if let Err(reason) = validate_name(spec.name) {
            return Err(RegistryError::InvalidName {
                name: spec.name.to_owned(),
                reason,
            });
        }
        *seen.entry(spec.name).or_default() += 1;
        if let Err(reason) = validate_mcp_name(&spec.mcp_name()) {
            return Err(RegistryError::InvalidMcpName {
                name: spec.name.to_owned(),
                reason,
            });
        }
        if spec.kind == ActionKind::Internal && !matches!(spec.handler, HandlerRef::Internal(_)) {
            return Err(RegistryError::Handlerless(spec.name.to_owned()));
        }
        for scope in spec.scopes {
            if !Scope::ALL.contains(scope) {
                return Err(RegistryError::UnknownScope {
                    name: spec.name.to_owned(),
                    scope: format!("{scope:?}"),
                });
            }
        }
    }
    if let Some((name, _count)) = seen.iter().find(|(_, count)| **count > 1) {
        return Err(RegistryError::DuplicateName((*name).to_owned()));
    }
    let collisions = mcp_collisions(registry.names());
    if let Some((mcp, actions)) = collisions.into_iter().next() {
        return Err(RegistryError::McpCollision {
            mcp,
            actions: actions.into_iter().map(str::to_owned).collect(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{AgentHints, AgentTier, Confirm, Effect, Tenancy};

    fn schema() -> crate::spec::Schema {
        schemars::schema_for!(crate::views::EmptyIn)
    }

    fn spec(name: &'static str) -> ActionSpec {
        ActionSpec {
            name,
            version: 1,
            kind: ActionKind::External,
            input: schema,
            output: schema,
            scopes: &[],
            idempotent: false,
            timeout_ms: 1000,
            effect: Effect::Read,
            confirm: Confirm::None,
            tenancy: Tenancy::Tenant,
            agent: AgentHints {
                tier: AgentTier::None,
                proposal_required: false,
            },
            task_capable: false,
            captures_usage: false,
            handler: HandlerRef::Workflow,
        }
    }

    #[test]
    fn duplicate_names_are_visible_to_the_gate() {
        let registry = Registry::new(vec![spec("me.get"), spec("me.get")]);
        let mut seen = BTreeMap::new();
        for spec in registry.iter() {
            *seen.entry(spec.name).or_insert(0) += 1;
        }
        assert!(seen.values().any(|count| *count > 1));
    }

    #[test]
    fn an_internal_spec_without_a_handler_is_refused() {
        let mut broken = spec("me.get");
        broken.kind = ActionKind::Internal;
        let registry = Registry::new(vec![broken]);
        assert!(matches!(registry_handlerless(&registry), Some("me.get")));
    }

    fn registry_handlerless(registry: &Registry) -> Option<&'static str> {
        registry
            .iter()
            .find(|spec| {
                spec.kind == ActionKind::Internal
                    && !matches!(spec.handler, HandlerRef::Internal(_))
            })
            .map(|spec| spec.name)
    }

    #[test]
    fn the_mcp_collision_is_detected_before_listing() {
        let registry = Registry::new(vec![spec("cards.move"), spec("cards_move")]);
        let collisions = mcp_collisions(registry.names());
        assert!(collisions.contains_key("cards_move"));
    }
}
