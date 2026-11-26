//! One resolution model for compiled and imported actions (S04 P1, issue #3).
//!
//! The compiled registry is the process's static set. An imported action is a tenant's approved
//! `action_registry` row: the ingester stored the operation's spec, the tenant approved it, and
//! this module turns that stored document into the same [`ActionSpec`] the compiled path uses.
//! There is no second registry and no private invocation path: [`precheck`](crate::invoke::precheck)
//! resolves one or the other, and every downstream surface reads the resulting spec.
//!
//! The compiled variant borrows the process-wide spec; the imported variant owns one built from
//! the row. [`ResolvedAction`] derefs to `&ActionSpec`, so a caller that only needs the spec does
//! not care which it holds.

use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use serde_json::Value;
use sqlx::PgConnection;

use crate::spec::{
    ActionKind, ActionSpec, AgentHints, Confirm, Effect, HandlerRef, SchemaRef, Tenancy,
};

/// Why a stored row cannot become a runtime action.
///
/// Every variant names the field and the rule; a malformed approved row is a serving failure
/// (the tenant approved a document this build cannot execute), never a silent 404.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The row's kind is not executable (a workflow row needs S13's engine).
    #[error("action {name:?} is stored as kind {kind:?}, which has no runtime handler")]
    UnsupportedKind {
        /// The action name.
        name: String,
        /// The stored kind.
        kind: String,
    },
    /// A required spec member is absent or of the wrong shape.
    #[error("action {name:?}: spec.{field} {reason}")]
    BadSpec {
        /// The action name.
        name: String,
        /// The spec member.
        field: &'static str,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// The stored method is not one this engine may call.
    #[error("action {name:?}: method {method:?} is not allowed")]
    BadMethod {
        /// The action name.
        name: String,
        /// The stored method.
        method: String,
    },
    /// The stored path is not an absolute path on the service (a scheme or `//` would escape it).
    #[error("action {name:?}: path {path:?} must be an absolute path on the service")]
    BadPath {
        /// The action name.
        name: String,
        /// The stored path.
        path: String,
    },
    /// The row could not be read.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// A spec resolved for one invocation: the compiled entry, or an owned imported one.
#[derive(Clone, Debug)]
pub enum ResolvedAction<'a> {
    /// A compiled action in this binary.
    Compiled(&'a ActionSpec),
    /// An approved imported action, built from its stored row.
    Imported(ActionSpec),
}

impl Deref for ResolvedAction<'_> {
    type Target = ActionSpec;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Compiled(spec) => spec,
            Self::Imported(spec) => spec,
        }
    }
}

impl ResolvedAction<'_> {
    /// The spec itself, whatever the variant.
    pub fn spec(&self) -> &ActionSpec {
        self
    }
}

/// The methods an imported operation may declare. GET/HEAD read; DELETE is destructive; every
/// other method writes.
const READ_METHODS: [&str; 2] = ["GET", "HEAD"];

fn stored<'a>(spec: &'a Value, field: &'static str) -> Option<&'a Value> {
    spec.get(field)
}

/// Turn one approved row into a runtime [`ActionSpec`].
pub fn resolve_row(row: &conveyor_store::actions::RegistryRow) -> Result<ActionSpec, ResolveError> {
    let name = row.name.clone();
    if row.kind != "external" {
        return Err(ResolveError::UnsupportedKind {
            name,
            kind: row.kind.clone(),
        });
    }
    let method = stored(&row.spec, "method")
        .and_then(Value::as_str)
        .map(str::to_ascii_uppercase)
        .ok_or(ResolveError::BadSpec {
            name: name.clone(),
            field: "method",
            reason: "must be a string",
        })?;
    if !matches!(
        method.as_str(),
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
    ) {
        return Err(ResolveError::BadMethod { name, method });
    }
    let path = stored(&row.spec, "path")
        .and_then(Value::as_str)
        .ok_or(ResolveError::BadSpec {
            name: name.clone(),
            field: "path",
            reason: "must be a string",
        })?;
    if !path.starts_with('/') || path.starts_with("//") || path.contains("://") {
        return Err(ResolveError::BadPath {
            name,
            path: path.to_owned(),
        });
    }
    let input = stored(&row.spec, "input").ok_or(ResolveError::BadSpec {
        name: name.clone(),
        field: "input",
        reason: "must be the operation's request schema",
    })?;
    let output = stored(&row.spec, "output").unwrap_or(&Value::Null);
    let auth = stored(&row.spec, "auth")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| Cow::Owned(value.to_owned()));
    let effect = if READ_METHODS.contains(&method.as_str()) {
        Effect::Read
    } else if method == "DELETE" {
        Effect::Destructive
    } else {
        Effect::Write
    };
    Ok(ActionSpec {
        name: Cow::Owned(row.name.clone()),
        version: u32::try_from(row.spec_version.max(1)).unwrap_or(1),
        kind: ActionKind::External,
        input: SchemaRef::Imported(Arc::new(input.clone())),
        output: SchemaRef::Imported(Arc::new(output.clone())),
        scopes: Cow::Borrowed(&[]),
        idempotent: row.idempotent,
        timeout_ms: u32::try_from(row.timeout_ms.max(1)).unwrap_or(10_000),
        effect,
        approval_target: None,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        artefact: None,
        handler: HandlerRef::External {
            service: Cow::Owned(row.service.clone()),
            method: Cow::Owned(method),
            path: Cow::Owned(path.to_owned()),
            auth,
            auth_optional: true,
            upload: None,
        },
    })
}

/// The tenant's approved row for one action name, if any.
///
/// `Ok(None)` means the tenant approved no such action; an `Err` means it approved one this
/// build cannot execute, which must be a typed serving failure, never a silent absence.
pub async fn approved_action_tx(
    conn: &mut PgConnection,
    name: &str,
) -> Result<Option<ActionSpec>, ResolveError> {
    let Some(row) = conveyor_store::actions::approved_action_tx(conn, name).await? else {
        return Ok(None);
    };
    resolve_row(&row).map(Some)
}

/// The tenant's approved imported actions, resolved for discovery.
///
/// Discovery merges these with the compiled registry; a row that cannot resolve is logged and
/// omitted from the list but still refuses at invocation (the typed 500 in `precheck`), so a
/// bad approval cannot silently disappear from the surface that would expose it.
pub async fn approved_actions(
    pool: &sqlx::PgPool,
    tenant: conveyor_store::TenantId,
) -> Result<Vec<ActionSpec>, ResolveError> {
    let mut tx = conveyor_store::begin_scoped_read_only(pool, tenant).await?;
    let rows = conveyor_store::actions::approved_registry(&mut tx).await?;
    let _ = tx.rollback().await;
    let mut specs = Vec::with_capacity(rows.len());
    for row in &rows {
        match resolve_row(row) {
            Ok(spec) => specs.push(spec),
            Err(error) => {
                tracing::error!(%error, action = %row.name, "omit an unresolvable imported action from discovery")
            }
        }
    }
    Ok(specs)
}

/// Intern an imported action name for the handler context.
///
/// `HandlerCtx.action` is `&'static str` across the handler surface; an imported name is owned
/// by its row. The number of distinct approved action names a process can serve is bounded by
/// the tenant's registry, so one leaked copy per **distinct** name is a bounded allocation —
/// never a per-invocation leak.
pub fn intern_name(name: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let names = NAMES.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guard = match names.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = guard.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    guard.insert(leaked);
    leaked
}

/// Why the boot gate refused.
#[derive(Debug, thiserror::Error)]
pub enum BootRefusal {
    /// An approved row exists that this build cannot turn into an action.
    #[error("the database names action {name:?}, which this binary cannot execute: {source}")]
    Unresolvable {
        /// The action name.
        name: String,
        /// Why it did not resolve.
        #[source]
        source: ResolveError,
    },
    /// An external action's service has no base URL configured.
    #[error("action {name:?} names service {service:?}, which has no configured base URL")]
    Unconfigured {
        /// The action name.
        name: String,
        /// The stored service key.
        service: String,
    },
    /// A workflow row needs S13's engine, which is not compiled in.
    #[error("action {name:?} is stored as a workflow but compiled without one")]
    Workflow {
        /// The action name.
        name: String,
    },
    /// The rows could not be read.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The boot gate (S04 §5.2, issue #3 P1.1): every approved row must be executable by this build.
///
/// A compiled action passes; an imported external action passes when its stored spec resolves
/// (that is the change S04 makes — a DB-only action is no longer a boot failure); the service's
/// base URL must still be configured, and a workflow row still requires a compiled handler.
///
/// Lives here rather than in the serve binary so the gate is testable: `serve` calls it before
/// bind, and `action_resolution.rs` calls it directly.
pub async fn assert_served_registry(
    pool: &sqlx::PgPool,
    compiled: &crate::registry::Registry,
) -> Result<(), BootRefusal> {
    for tenant in conveyor_store::tenants::active_ids(pool).await? {
        let mut tx = conveyor_store::begin_scoped_read_only(pool, tenant).await?;
        let rows = conveyor_store::actions::approved_registry(&mut tx).await?;
        let _ = tx.rollback().await;
        for row in rows {
            let resolved = match compiled.get(&row.name) {
                Some(spec) => ResolvedAction::Compiled(spec),
                None => match resolve_row(&row) {
                    Ok(spec) => ResolvedAction::Imported(spec),
                    Err(source) => {
                        return Err(BootRefusal::Unresolvable {
                            name: row.name,
                            source,
                        });
                    }
                },
            };
            if let HandlerRef::External { service, .. } = &resolved.handler
                && crate::invoke::service_url(service).is_none()
            {
                return Err(BootRefusal::Unconfigured {
                    name: row.name,
                    service: row.service,
                });
            }
            if row.kind == "workflow" && !matches!(&resolved.handler, HandlerRef::Workflow) {
                return Err(BootRefusal::Workflow { name: row.name });
            }
        }
    }
    Ok(())
}
