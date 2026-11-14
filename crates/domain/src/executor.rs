//! The internal Executor seam (SPEC_V2 §4.5, D61; T0, issue #34 P6).
//!
//! T0 ships the **contract only**. Two live kinds — `procedure` and `internal_agent` — implement
//! one [`Executor`] trait and validate the configuration a binding must carry; execution
//! behaviour arrives with T2. A new kind is an implementation plus a [`Registry::register`]
//! entry, never an edit to the run path: `run` records its binding by FK, and the run store
//! contains no `match` on the kind at all.
//!
//! [`ExecutorKind`] is an open slug, not a closed enum. `job_binding.executor_kind` is `TEXT`
//! with no value CHECK for the same reason: the registry is the authority, so "register a third
//! kind without editing the run path" is expressible — and testable — without a migration.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// `procedure` — a deterministic sequence: the executable `process` of migration 0035.
pub const PROCEDURE: &str = "procedure";
/// `internal_agent` — generation over a compiled prompt: a `capability_agent` row.
pub const INTERNAL_AGENT: &str = "internal_agent";

/// The longest slug the database accepts (`^[a-z][a-z0-9_]{1,31}$`).
const MAX_KIND_LEN: usize = 32;

/// One executor-kind slug.
///
/// The set is open: [`ExecutorKind::parse`] enforces the database's shape, and
/// [`Registry::require`] decides whether this build serves the kind.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExecutorKind(String);

impl ExecutorKind {
    /// Parse a slug, enforcing the same shape as `job_binding_executor_kind_format`.
    pub fn parse(slug: &str) -> Result<Self, BindingError> {
        if is_valid_slug(slug) {
            Ok(Self(slug.to_owned()))
        } else {
            Err(BindingError::InvalidKind(slug.to_owned()))
        }
    }

    /// The built-in `procedure` kind.
    pub fn procedure() -> Self {
        Self(PROCEDURE.to_owned())
    }

    /// The built-in `internal_agent` kind.
    pub fn internal_agent() -> Self {
        Self(INTERNAL_AGENT.to_owned())
    }

    /// The slug as stored.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExecutorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ExecutorKind {
    type Err = BindingError;

    fn from_str(slug: &str) -> Result<Self, Self::Err> {
        Self::parse(slug)
    }
}

impl Serialize for ExecutorKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ExecutorKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let slug = String::deserialize(deserializer)?;
        Self::parse(&slug).map_err(serde::de::Error::custom)
    }
}

/// `^[a-z][a-z0-9_]{1,31}$` — the migration's CHECK, in Rust.
fn is_valid_slug(slug: &str) -> bool {
    let mut chars = slug.chars();
    let starts = matches!(chars.next(), Some(first) if first.is_ascii_lowercase());
    starts
        && (2..=MAX_KIND_LEN).contains(&slug.len())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// What a binding must carry for its executor.
#[derive(Clone, Debug)]
pub struct BindingConfig {
    /// The kind the binding names.
    pub executor_kind: ExecutorKind,
    /// The executable sequence, for a `procedure` binding.
    pub process_id: Option<Uuid>,
    /// The agent configuration, for an `internal_agent` binding.
    pub capability_agent_id: Option<Uuid>,
}

/// One executor kind's contract.
///
/// T0 has validation only. The method that will execute a step is deliberately absent: T2 adds
/// it to this trait, so no caller can grow a second dispatch path in the meantime.
pub trait Executor: Send + Sync {
    /// The kind this implementation serves.
    fn kind(&self) -> ExecutorKind;

    /// Refuse a binding that does not carry what this executor needs.
    fn validate_binding(&self, binding: &BindingConfig) -> Result<(), BindingError>;
}

/// Why a binding or a kind registration was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    /// The slug does not match the database's shape.
    #[error("{0:?} is not a valid executor kind (^[a-z][a-z0-9_]{{1,31}}$)")]
    InvalidKind(String),
    /// The slug is well formed but this build serves no implementation for it.
    #[error("{0:?} is not a registered executor kind")]
    UnknownKind(String),
    /// Two implementations claimed one kind.
    #[error("the {0:?} executor kind is already registered")]
    DuplicateKind(String),
    /// A binding with neither executor is meaningless (SPEC §4.4).
    #[error("a binding names a process or a capability agent")]
    NoExecutor,
    /// A `procedure` binding without its executable sequence.
    #[error("a procedure binding must name a process")]
    MissingProcess,
    /// An `internal_agent` binding without its agent configuration.
    #[error("an internal_agent binding must name a capability agent")]
    MissingAgent,
}

/// The built-in `procedure` executor: a deterministic sequence of steps (0035's `process`).
struct ProcedureExecutor;

impl Executor for ProcedureExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::procedure()
    }

    fn validate_binding(&self, binding: &BindingConfig) -> Result<(), BindingError> {
        if binding.process_id.is_none() {
            return Err(BindingError::MissingProcess);
        }
        Ok(())
    }
}

/// The built-in `internal_agent` executor: generation over a compiled prompt.
struct InternalAgentExecutor;

impl Executor for InternalAgentExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::internal_agent()
    }

    fn validate_binding(&self, binding: &BindingConfig) -> Result<(), BindingError> {
        if binding.capability_agent_id.is_none() {
            return Err(BindingError::MissingAgent);
        }
        Ok(())
    }
}

/// The kinds this build serves, keyed by slug.
pub struct Registry {
    executors: BTreeMap<ExecutorKind, Box<dyn Executor>>,
}

impl Registry {
    /// The two live kinds (SPEC §4.5).
    pub fn builtin() -> Self {
        let mut registry = Self {
            executors: BTreeMap::new(),
        };
        registry
            .register(Box::new(ProcedureExecutor))
            .expect("the built-in kinds are distinct");
        registry
            .register(Box::new(InternalAgentExecutor))
            .expect("the built-in kinds are distinct");
        registry
    }

    /// Add one implementation. A duplicate kind is refused, not silently replaced.
    pub fn register(&mut self, executor: Box<dyn Executor>) -> Result<(), BindingError> {
        let kind = executor.kind();
        if self.executors.contains_key(&kind) {
            return Err(BindingError::DuplicateKind(kind.as_str().to_owned()));
        }
        self.executors.insert(kind, executor);
        Ok(())
    }

    /// The implementation for `kind`, or [`BindingError::UnknownKind`].
    pub fn require(&self, kind: &ExecutorKind) -> Result<&dyn Executor, BindingError> {
        self.executors
            .get(kind)
            .map(Box::as_ref)
            .ok_or_else(|| BindingError::UnknownKind(kind.as_str().to_owned()))
    }

    /// Every served kind, in slug order.
    pub fn kinds(&self) -> impl Iterator<Item = &str> {
        self.executors.keys().map(ExecutorKind::as_str)
    }

    /// Resolve the binding's kind and run its validation.
    pub fn validate(&self, binding: &BindingConfig) -> Result<(), BindingError> {
        if binding.process_id.is_none() && binding.capability_agent_id.is_none() {
            return Err(BindingError::NoExecutor);
        }
        self.require(&binding.executor_kind)?
            .validate_binding(binding)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ThirdExecutor;

    impl Executor for ThirdExecutor {
        fn kind(&self) -> ExecutorKind {
            ExecutorKind::parse("third").expect("valid slug")
        }

        fn validate_binding(&self, _binding: &BindingConfig) -> Result<(), BindingError> {
            Ok(())
        }
    }

    #[test]
    fn the_builtin_registry_serves_exactly_the_two_live_kinds() {
        let registry = Registry::builtin();
        assert_eq!(
            registry.kinds().collect::<Vec<_>>(),
            vec![INTERNAL_AGENT, PROCEDURE]
        );
        assert!(registry.require(&ExecutorKind::procedure()).is_ok());
        assert!(registry.require(&ExecutorKind::internal_agent()).is_ok());
    }

    #[test]
    fn a_third_kind_registers_without_touching_the_builtins() {
        let mut registry = Registry::builtin();
        registry
            .register(Box::new(ThirdExecutor))
            .expect("a new kind registers");
        assert_eq!(
            registry
                .require(&ExecutorKind::parse("third").expect("valid"))
                .expect("registered")
                .kind()
                .as_str(),
            "third"
        );
        assert!(matches!(
            registry.require(&ExecutorKind::parse("nope").expect("valid")),
            Err(BindingError::UnknownKind(kind)) if kind == "nope"
        ));
    }

    #[test]
    fn a_duplicate_kind_is_refused() {
        let mut registry = Registry::builtin();
        assert_eq!(
            registry.register(Box::new(ProcedureExecutor)),
            Err(BindingError::DuplicateKind(PROCEDURE.to_owned()))
        );
    }

    #[test]
    fn a_binding_with_neither_executor_is_refused_before_the_kind_lookup() {
        let registry = Registry::builtin();
        let binding = BindingConfig {
            executor_kind: ExecutorKind::parse("third").expect("valid"),
            process_id: None,
            capability_agent_id: None,
        };
        assert_eq!(registry.validate(&binding), Err(BindingError::NoExecutor));
    }

    #[test]
    fn each_live_kind_requires_its_configuration() {
        let registry = Registry::builtin();
        let agent = Uuid::nil();
        let procedure = BindingConfig {
            executor_kind: ExecutorKind::procedure(),
            process_id: None,
            capability_agent_id: Some(agent),
        };
        assert_eq!(
            registry.validate(&procedure),
            Err(BindingError::MissingProcess)
        );
        let internal = BindingConfig {
            executor_kind: ExecutorKind::internal_agent(),
            process_id: Some(agent),
            capability_agent_id: None,
        };
        assert_eq!(
            registry.validate(&internal),
            Err(BindingError::MissingAgent)
        );
    }

    #[test]
    fn slugs_match_the_database_shape() {
        for good in ["procedure", "internal_agent", "third", "a1"] {
            assert!(ExecutorKind::parse(good).is_ok(), "{good}");
        }
        for bad in ["", "A", "1x", "with-dash", "with space", &"x".repeat(33)] {
            assert!(ExecutorKind::parse(bad).is_err(), "{bad}");
        }
    }
}
