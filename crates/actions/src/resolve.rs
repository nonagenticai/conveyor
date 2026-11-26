//! The one resolution model for compiled and imported actions (S04 P1).
//!
//! A name resolves **compiled-first**, then through the caller tenant's *approved* registry
//! rows. The row is re-read on every call (inside a tenant-scoped transaction, so RLS is the
//! confinement), which is what makes restart, approval and revocation take effect without a
//! cache to invalidate: for imported actions the database is the authority.
//!
//! A compiled [`crate::spec::ActionSpec`] stays `&'static`; an imported row becomes an owned
//! [`ResolvedAction`]. Both carry the same fields the authorisation and dispatch need, so HTTP,
//! MCP, discovery and readiness read one shape.

use conveyor_domain::TenantId;
use conveyor_store::actions::{self, RegistryRow};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::invoke::{ActionError, Authority, InvokeRequest, MAX_DEPTH, Precondition};
use crate::spec::{ActionKind, Confirm, Effect, Scope, Tenancy, validate_input};

/// Where a resolved action came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionSource {
    /// A compiled `ActionSpec`.
    Compiled,
    /// One tenant's approved `action_registry` row.
    Registry {
        /// The row id.
        id: uuid::Uuid,
        /// The approved spec version.
        spec_version: u32,
        /// The ingest's schema hash.
        schema_hash: String,
    },
}

/// How an input member reaches the upstream request (S04 P3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bindings {
    /// Members substituted into `{name}` in the path, in path order.
    pub path: Vec<String>,
    /// Members sent as the query string.
    pub query: Vec<String>,
    /// Members sent as the JSON body; `None` sends the whole input object.
    pub body: Option<Vec<String>>,
}

/// The HTTP operation an imported action performs.
#[derive(Clone, Debug)]
pub struct ExternalAction {
    /// The service key whose base URL is configuration.
    pub service: String,
    /// The HTTP method.
    pub method: String,
    /// The operation path, `{name}` templates included.
    pub path: String,
    /// The name of a per-tenant sealed secret sent as the bearer.
    pub auth: Option<String>,
    /// When true, an absent secret sends no bearer instead of refusing.
    pub auth_optional: bool,
    /// How the input maps onto path, query and body.
    pub bindings: Bindings,
}

/// What an imported action dispatches to.
#[derive(Clone, Debug)]
pub enum ResolvedHandler {
    /// A compiled handler reference (internal, external, broker or tool destination).
    Compiled(crate::spec::HandlerRef),
    /// An imported HTTP operation.
    External(ExternalAction),
    /// A workflow (the engine does not exist yet).
    Workflow,
}

/// One action, ready to authorise and dispatch.
#[derive(Clone, Debug)]
pub struct ResolvedAction {
    /// `<resource>.<verb>`.
    pub name: String,
    /// The integer version.
    pub version: u32,
    /// Internal, external or workflow.
    pub kind: ActionKind,
    /// The request schema.
    pub input: Value,
    /// The response schema.
    pub output: Value,
    /// The scopes a caller must hold.
    pub scopes: Vec<Scope>,
    /// Whether replaying the same key is safe.
    pub idempotent: bool,
    /// The caller's deadline.
    pub timeout_ms: u32,
    /// Read, write, spend or destructive.
    pub effect: Effect,
    /// Confirmation mode for channels.
    pub confirm: Confirm,
    /// The resolution scope.
    pub tenancy: Tenancy,
    /// Where the work happens.
    pub handler: ResolvedHandler,
    /// Where the definition came from.
    pub source: ActionSource,
}

/// A row that cannot become a [`ResolvedAction`].
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The stored spec is missing a member or names an unknown vocabulary value.
    #[error("the action registry row {name:?} is malformed: {detail}")]
    Malformed {
        /// The row's name.
        name: String,
        /// What is wrong.
        detail: String,
    },
    /// The database refused the read.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl ResolvedAction {
    /// Project a compiled spec into the resolved shape.
    pub fn from_spec(spec: &crate::spec::ActionSpec) -> Self {
        Self {
            name: spec.name.to_owned(),
            version: spec.version,
            kind: spec.kind,
            input: spec.input_schema(),
            output: spec.output_schema(),
            scopes: spec.scopes.to_vec(),
            idempotent: spec.idempotent,
            timeout_ms: spec.timeout_ms,
            effect: spec.effect,
            confirm: spec.confirm,
            tenancy: spec.tenancy,
            handler: match spec.handler {
                crate::spec::HandlerRef::Workflow => ResolvedHandler::Workflow,
                handler => ResolvedHandler::Compiled(handler),
            },
            source: ActionSource::Compiled,
        }
    }

    /// Project an approved imported row into the resolved shape.
    ///
    /// The row's `spec` carries the operation (`method`, `path`, `input`, `output`) and an
    /// optional `runtime` block (`effect`, `confirm`, `tenancy`, `scopes`, `auth`,
    /// `auth_optional`); an absent member gets the method's conservative default. `bindings`
    /// is explicit when present and derived from the path template otherwise.
    pub fn from_row(row: &RegistryRow) -> Result<Self, ResolveError> {
        let bad = |detail: &str| ResolveError::Malformed {
            name: row.name.clone(),
            detail: detail.to_owned(),
        };
        let kind = match row.kind.as_str() {
            "external" => ActionKind::External,
            "workflow" => ActionKind::Workflow,
            other => return Err(bad(&format!("unknown kind {other:?}"))),
        };
        let method = row
            .spec
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("spec.method is missing"))?
            .to_ascii_uppercase();
        let path = row
            .spec
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("spec.path is missing"))?
            .to_owned();
        let input = row
            .spec
            .get("input")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"}));
        let output = row
            .spec
            .get("output")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"}));
        let runtime = row.spec.get("runtime").cloned().unwrap_or(Value::Null);

        let effect = match runtime.get("effect").and_then(Value::as_str) {
            Some(text) => Effect::parse(text).ok_or_else(|| bad("runtime.effect is unknown"))?,
            None => default_effect(&method),
        };
        let confirm = match runtime.get("confirm").and_then(Value::as_str) {
            Some("card") => Confirm::Card,
            Some("card-typed") => Confirm::CardTyped,
            Some("none") | None => Confirm::None,
            Some(other) => return Err(bad(&format!("runtime.confirm {other:?} is unknown"))),
        };
        let tenancy = match runtime.get("tenancy").and_then(Value::as_str) {
            Some("tenant") | None => Tenancy::Tenant,
            Some("self") => Tenancy::Self_,
            Some("project") => Tenancy::Project,
            Some("platform") => Tenancy::Platform,
            Some(other) => return Err(bad(&format!("runtime.tenancy {other:?} is unknown"))),
        };
        let scopes = match runtime.get("scopes").and_then(Value::as_array) {
            Some(items) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .and_then(Scope::parse)
                        .ok_or_else(|| bad("runtime.scopes names an unknown scope"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => default_scopes(&method),
        };
        let auth = runtime
            .get("auth")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let auth_optional = runtime
            .get("auth_optional")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let bindings = bindings_from(row, &path, &method, &input);

        let handler = match kind {
            ActionKind::Workflow => ResolvedHandler::Workflow,
            _ => ResolvedHandler::External(ExternalAction {
                service: row.service.clone(),
                method,
                path,
                auth,
                auth_optional,
                bindings,
            }),
        };
        Ok(Self {
            name: row.name.clone(),
            version: u32::try_from(row.spec_version).unwrap_or(0),
            kind,
            input,
            output,
            scopes,
            idempotent: row.idempotent,
            timeout_ms: u32::try_from(row.timeout_ms).unwrap_or(30_000),
            effect,
            confirm,
            tenancy,
            handler,
            source: ActionSource::Registry {
                id: row.id,
                spec_version: u32::try_from(row.spec_version).unwrap_or(0),
                schema_hash: row.schema_hash.clone(),
            },
        })
    }

    /// Whether a caller holding `role` may invoke it, with the tenant's overrides applied.
    pub fn permits_with(
        &self,
        role: conveyor_auth::Role,
        policy: &conveyor_auth::ScopePolicy,
    ) -> bool {
        self.scopes
            .iter()
            .all(|scope| policy.allows(scope.as_str(), scope.granted_to(role)))
    }

    /// Whether this action is closed to every machine channel, whatever a tenant stores.
    pub fn machine_forbidden(&self) -> bool {
        self.scopes
            .iter()
            .any(|scope| crate::channel::is_denied_scope(scope.as_str()))
            || crate::channel::is_denied_action(&self.name)
            || crate::channel::is_denied_resource(crate::channel::resource_of(&self.name))
    }

    /// The MCP tool name: `.` → `_`.
    pub fn mcp_name(&self) -> String {
        self.name.replace('.', "_")
    }
}

/// The conservative effect of an HTTP method when the row declares none.
fn default_effect(method: &str) -> Effect {
    match method {
        "GET" | "HEAD" | "OPTIONS" => Effect::Read,
        "DELETE" => Effect::Destructive,
        _ => Effect::Write,
    }
}

/// The conservative scope of an HTTP method when the row declares none.
fn default_scopes(method: &str) -> Vec<Scope> {
    match method {
        "GET" | "HEAD" | "OPTIONS" => vec![Scope::ActionsRead],
        _ => vec![Scope::ActionsWrite],
    }
}

/// Read explicit bindings, or derive them from the path template and the input schema.
fn bindings_from(row: &RegistryRow, path: &str, method: &str, input: &Value) -> Bindings {
    let template: Vec<String> = path
        .split('/')
        .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
        .map(str::to_owned)
        .collect();
    if let Some(explicit) = row.spec.get("bindings") {
        let list = |member: &str| -> Vec<String> {
            explicit
                .get(member)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let body = explicit.get("body").map(|body| match body {
            Value::Null => Vec::new(),
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        });
        return Bindings {
            path: list("path"),
            query: list("query"),
            body,
        };
    }
    let has_body = !matches!(method, "GET" | "HEAD" | "OPTIONS" | "DELETE");
    let query = if has_body {
        Vec::new()
    } else {
        input
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| {
                properties
                    .keys()
                    .filter(|key| !template.iter().any(|bound| bound == key.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    Bindings {
        path: template,
        query,
        body: if has_body { None } else { Some(Vec::new()) },
    }
}

/// One action by name: compiled first, then the tenant's approved imported row.
///
/// A name with registry versions but no approved row is **revoked**, not unknown: it answers
/// `404 action_unavailable` so a caller can tell "never existed" from "exists, not approved".
pub async fn get(
    pool: &PgPool,
    tenant: TenantId,
    name: &str,
) -> Result<ResolvedAction, ActionError> {
    if let Some(spec) = crate::registry::registry().get(name) {
        return Ok(ResolvedAction::from_spec(spec));
    }
    let mut tx = conveyor_store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = name, "resolve: begin tenant transaction");
            ActionError::internal()
        })?;
    let approved = actions::approved_registry_by_name(&mut tx, name)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = name, "resolve: approved row");
            ActionError::internal()
        })?;
    let versions = if approved.is_none() {
        actions::registry_versions(&mut tx, name)
            .await
            .map_err(|error| {
                tracing::error!(%error, action = name, "resolve: registry versions");
                ActionError::internal()
            })?
    } else {
        Vec::new()
    };
    let _ = tx.rollback().await;
    match approved {
        Some(row) => ResolvedAction::from_row(&row).map_err(|error| {
            tracing::error!(%error, action = name, "resolve: malformed row");
            ActionError::internal()
        }),
        None if !versions.is_empty() => Err(ActionError::new(
            404,
            "action_unavailable",
            format!("{name} exists but no version is approved for this tenant"),
        )),
        None => Err(ActionError::not_found(name)),
    }
}

/// Every action this tenant may resolve: the compiled table plus its approved imported rows,
/// ordered by name.
pub async fn list(pool: &PgPool, tenant: TenantId) -> Result<Vec<ResolvedAction>, ActionError> {
    let mut tx = conveyor_store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "resolve: begin tenant transaction");
            ActionError::internal()
        })?;
    let rows = actions::approved_registry(&mut tx).await.map_err(|error| {
        tracing::error!(%error, "resolve: approved registry");
        ActionError::internal()
    })?;
    let _ = tx.rollback().await;
    let mut actions: Vec<ResolvedAction> = crate::registry::registry()
        .iter()
        .map(ResolvedAction::from_spec)
        .collect();
    for row in rows {
        match ResolvedAction::from_row(&row) {
            Ok(action) => actions.push(action),
            Err(error) => {
                // One malformed row must not empty every surface; it is reported and skipped,
                // and a direct call to it still fails as malformed through `get`.
                tracing::warn!(%error, "resolve: skipping a malformed approved row");
            }
        }
    }
    actions.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(actions)
}

/// The authorisation and input checks an invocation runs before any work starts, against a
/// resolved action (compiled or imported).
pub fn precheck(
    action: &ResolvedAction,
    request: &InvokeRequest,
) -> Result<(Value, Option<Precondition>), ActionError> {
    if request.depth > MAX_DEPTH {
        return Err(ActionError::recursion(request.depth));
    }
    let permitted = match &request.authority {
        Authority::Channel(authority) => {
            !action.machine_forbidden() && crate::channel::allows(authority, &action.name)
        }
        Authority::Role => {
            request.actor_kind == "user" && action.permits_with(request.role, &request.policy)
        }
        Authority::Internal => action.permits_with(request.role, &request.policy),
    };
    if !permitted {
        return Err(ActionError::forbidden(&action.name));
    }
    if action.kind == ActionKind::Workflow {
        return Err(ActionError::workflow(&action.name));
    }

    let mut input = request.input.clone();
    let mut precondition = request.precondition.clone();
    if let Some(object) = input.as_object_mut()
        && let Some(raw) = object.remove("_version")
    {
        let from_body = match raw {
            Value::Number(_) => raw.as_i64().map(Precondition::Version),
            Value::String(text) => Some(Precondition::Tag(text)),
            _ => None,
        };
        if precondition.is_some() && from_body.is_some() && precondition != from_body {
            return Err(ActionError::new(
                400,
                "bad_request",
                "If-Match and _version name different preconditions",
            ));
        }
        precondition = precondition.or(from_body);
    }
    let field_errors = validate_input(&action.input, &input);
    if !field_errors.is_empty() {
        return Err(ActionError::invalid_input(field_errors));
    }
    Ok((input, precondition))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, method: &str, path: &str, spec_extra: Value) -> RegistryRow {
        let mut spec = json!({
            "method": method,
            "path": path,
            "input": {"type": "object", "properties": {"id": {"type": "string"}, "q": {"type": "string"}}, "required": ["id"]},
            "output": {"type": "object"},
            "required": ["id"],
            "schema_hash": "abc",
        });
        if let Some(extra) = spec_extra.as_object() {
            for (key, value) in extra {
                spec[key] = value.clone();
            }
        }
        RegistryRow {
            id: uuid::Uuid::now_v7(),
            name: name.to_owned(),
            spec_version: 3,
            kind: "external".to_owned(),
            service: "fixtures".to_owned(),
            spec,
            schema_hash: "abc".to_owned(),
            timeout_ms: 200,
            idempotent: false,
            approved_at: Some(chrono::Utc::now()),
            approved_by: None,
        }
    }

    #[test]
    fn a_get_row_defaults_to_read_and_a_query_binding() {
        let action =
            ResolvedAction::from_row(&row("fixtures.echo", "GET", "/api/echo/{id}", json!({})))
                .expect("a valid row");
        assert_eq!(action.effect, Effect::Read);
        assert_eq!(action.scopes, vec![Scope::ActionsRead]);
        assert_eq!(action.timeout_ms, 200);
        let ResolvedHandler::External(external) = &action.handler else {
            panic!("an external handler");
        };
        assert_eq!(external.bindings.path, vec!["id".to_owned()]);
        assert_eq!(external.bindings.query, vec!["q".to_owned()]);
        assert_eq!(external.bindings.body, Some(Vec::new()));
    }

    #[test]
    fn a_post_row_defaults_to_write_and_a_body() {
        let action =
            ResolvedAction::from_row(&row("fixtures.create", "POST", "/api/things", json!({})))
                .expect("a valid row");
        assert_eq!(action.effect, Effect::Write);
        assert_eq!(action.scopes, vec![Scope::ActionsWrite]);
        let ResolvedHandler::External(external) = &action.handler else {
            panic!("an external handler");
        };
        assert_eq!(external.bindings.body, None, "the whole input is the body");
    }

    #[test]
    fn a_runtime_block_overrides_the_defaults_and_an_unknown_scope_is_refused() {
        let action = ResolvedAction::from_row(&row(
            "fixtures.delete",
            "DELETE",
            "/api/things/{id}",
            json!({"runtime": {"effect": "spend", "confirm": "card", "tenancy": "tenant", "scopes": ["actions:write"], "auth": "fixture-token"}}),
        ))
        .expect("a valid row");
        assert_eq!(action.effect, Effect::Spend);
        assert_eq!(action.confirm, Confirm::Card);
        assert_eq!(action.scopes, vec![Scope::ActionsWrite]);
        let ResolvedHandler::External(external) = &action.handler else {
            panic!("an external handler");
        };
        assert_eq!(external.auth.as_deref(), Some("fixture-token"));
        assert!(!external.auth_optional);

        let bad = ResolvedAction::from_row(&row(
            "fixtures.bad",
            "GET",
            "/api/things",
            json!({"runtime": {"scopes": ["nonsense:read"]}}),
        ));
        assert!(matches!(bad, Err(ResolveError::Malformed { .. })));
    }

    #[test]
    fn a_workflow_row_resolves_to_workflow() {
        let mut workflow = row("fixtures.run", "POST", "/api/run", json!({}));
        workflow.kind = "workflow".to_owned();
        let action = ResolvedAction::from_row(&workflow).expect("a valid row");
        assert!(matches!(action.handler, ResolvedHandler::Workflow));
    }

    #[test]
    fn a_missing_method_is_malformed() {
        let mut broken = row("fixtures.broken", "GET", "/x", json!({}));
        broken
            .spec
            .as_object_mut()
            .expect("an object")
            .remove("method");
        assert!(matches!(
            ResolvedAction::from_row(&broken),
            Err(ResolveError::Malformed { .. })
        ));
    }

    #[test]
    fn scope_parse_round_trips_every_variant() {
        for scope in Scope::ALL {
            assert_eq!(Scope::parse(scope.as_str()), Some(scope));
        }
        assert_eq!(Scope::parse("nonsense:read"), None);
    }
}
