#![forbid(unsafe_code)]
//! The MCP projection: `rmcp`'s streamable HTTP transport at `/mcp`.
//!
//! `tools/list` and `tools/call` read the SAME [`crate::registry::Registry`] the HTTP
//! operations do, and `tools/call` runs the SAME [`crate::invoke::invoke`]. A tool a role
//! cannot call is **absent** from `tools/list` (S4-D6), not listed-then-refused.
//!
//! Credentials fail closed at the transport: the route layer resolves the bearer before the
//! rmcp service sees the request and answers `401` when it cannot, so an unresolvable
//! tenant never reaches `tools/list`. The resolved [`conveyor_auth::Session`] rides in the
//! request extensions, which rmcp copies into `RequestContext::parts`.

use std::sync::Arc;

use conveyor_auth::Session;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams, ContentBlock,
    ErrorData as McpError, GetTaskParams, GetTaskResult, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, UpdateTaskParams,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::StreamableHttpServerConfig;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::invoke::{Authority, InvokeRequest, invoke};
use crate::registry::registry;
use crate::spec::ActionSpec;

/// The tasks extension (S06c): `tasks/get|update|cancel`, backed by the run.
pub mod tasks;

/// Everything the MCP handler needs that is not per-request.
#[derive(Clone)]
pub struct McpDeps {
    /// The runtime pool (tenants are resolved per request, not per session).
    pub pool: PgPool,
    /// `APP_KEY`, for `licences.get`.
    pub app_key: String,
}

/// One MCP tool, projected from an [`ActionSpec`] (the same value the OpenAPI operation is
/// built from).
pub fn tool_json(spec: &ActionSpec) -> Value {
    json!({
        "name": spec.mcp_name(),
        "description": format!(
            "{} — {} action, effect {}.",
            spec.name,
            spec.kind.as_str(),
            spec.effect.as_str()
        ),
        "inputSchema": spec.input_schema(),
        "outputSchema": spec.output_schema(),
        "annotations": {
            "readOnlyHint": spec.effect.as_str() == "read",
            "destructiveHint": spec.effect.as_str() == "destructive",
            "idempotentHint": spec.idempotent,
            "openWorldHint": spec.kind.as_str() == "external",
        },
        "_meta": {
            // The agent imports an external MCP system's tool as `object.verb` from this key and
            // falls back to `tool.<snake>` when it is absent (`agent`'s `mcp_import.rs:199-211`).
            // Without it a conveyor tool would import as `tool.projects_create`, so the job's
            // operation names would never match this registry (S06b).
            "operation": spec.name,
            "x-action-name": spec.name,
            "x-action-version": spec.version,
            "x-spec-hash": spec.spec_hash(),
        },
    })
}

fn session_of(context: &RequestContext<RoleServer>) -> Option<Session> {
    let parts = context.extensions.get::<http::request::Parts>()?;
    parts.extensions.get::<Session>().cloned()
}

fn request_id_of(context: &RequestContext<RoleServer>) -> Option<String> {
    let parts = context.extensions.get::<http::request::Parts>()?;
    parts
        .headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The `Idempotency-Key` HTTP header of this MCP request.
///
/// It is the transport's key, exactly as on `POST /api/actions/...` — not a member of the
/// action's input (which the schema would reject) and not `_meta` (which rmcp drops when
/// it deserializes `CallToolRequestParams`; measured 2026-11-08).
fn idempotency_key_of(context: &RequestContext<RoleServer>) -> Option<String> {
    let parts = context.extensions.get::<http::request::Parts>()?;
    parts
        .headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn trace_id_of(context: &RequestContext<RoleServer>) -> Option<String> {
    let parts = context.extensions.get::<http::request::Parts>()?;
    parts
        .headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('-').nth(1))
        .map(str::to_owned)
}

/// The MCP server. One instance per session; the tenant comes from the request's
/// credential, which is fixed for the life of the session.
#[derive(Clone)]
pub struct McpServer {
    deps: McpDeps,
}

impl McpServer {
    /// Build a handler over the shared dependencies.
    pub fn new(deps: McpDeps) -> Self {
        Self { deps }
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks()
                .build(),
        )
        .with_instructions(
            "conveyor actions. Every tool is one action of the HTTP registry; \
                 `tools/call` takes the action's input document as its arguments. \
                 A task-capable action (projects.execute) returns a task whose id is its run; \
                 poll `tasks/get`, answer `inputRequests` with `tasks/update`, stop with \
                 `tasks/cancel`.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let Some(session) = session_of(&context) else {
            return Err(McpError::invalid_request("unauthenticated", None));
        };
        // A machine principal sees its tenant's channel grants, NOT its role's surface (S06d).
        // The role is `kata_executor` for every intra-action check; the grant set is where the
        // elevation lives, so intersecting it with the role would hide `projects.create` — the
        // very action the agent exists to call.
        let tools = if let Some(authority) = &session.channel {
            registry()
                .iter()
                .filter(|spec| spec.permits_channel(authority))
                .map(tool_json)
                .filter_map(|value| serde_json::from_value(value).ok())
                .collect()
        } else {
            registry()
                .tools(session.role)
                .into_iter()
                .filter_map(|value| serde_json::from_value(value).ok())
                .collect()
        };
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            result_type: None,
            ttl_ms: None,
            cache_scope: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let Some(session) = session_of(&context) else {
            return Err(McpError::invalid_request("unauthenticated", None));
        };
        let Some(spec) = registry()
            .iter()
            .find(|spec| spec.mcp_name() == request.name.as_ref())
        else {
            // An unknown NAME is a JSON-RPC -32602. A tool that exists but is outside this
            // role's set is NOT handled here: it resolves to its spec and the shared
            // `invoke` refuses it with the same `403 action-forbidden` HTTP returns. The
            // S4-D6 guarantee is the `tools/list` absence above, not indistinguishability
            // at `tools/call`.
            return Err(McpError::invalid_params(
                format!("unknown tool {:?}", request.name),
                None,
            ));
        };

        // A machine principal is different: its channel grants ARE its contract, so an action
        // outside them (or under the floor) is refused in the same words as an unknown name —
        // the refusal must not tell a long-lived unattended credential what else exists.
        if let Some(authority) = &session.channel
            && !spec.permits_channel(authority)
        {
            return Err(McpError::invalid_params(
                format!("unknown tool {:?}", request.name),
                None,
            ));
        }

        let input = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| json!({}));
        let invoke_request = InvokeRequest {
            action: spec.name.to_owned(),
            input,
            precondition: None,
            idempotency_key: idempotency_key_of(&context),
            actor_id: session.principal.user_id.as_uuid(),
            actor_kind: session.principal.kind.as_actor_kind().to_owned(),
            role: session.role,
            policy: session.policy.clone(),
            authority: Authority::of(&session),
            request_id: request_id_of(&context),
            trace_id: trace_id_of(&context),
            depth: 0,
            app_key: self.deps.app_key.clone(),
            usage: None,
        };

        let (status, body) =
            match invoke(&self.deps.pool, registry(), session.tenant, invoke_request).await {
                Ok(outcome) => (outcome.status, outcome.body),
                Err(error) => {
                    let instance = spec.route_path();
                    (
                        error.status,
                        error.problem(Some(&instance), request_id_of(&context).as_deref()),
                    )
                }
            };
        let text = serde_json::to_string(&body).unwrap_or_default();
        // S06c: a task-capable action's successful call is the task's START. The task id is
        // the run the action created; a failed call stays an ordinary error result, so the
        // client never gets a task for work that never started.
        if spec.task_capable && (200..300).contains(&status) {
            return Ok(CallToolResponse::Task(tasks::start_result(&body)?));
        }
        let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
        result.structured_content = Some(body);
        result.is_error = Some(!(200..300).contains(&status));
        Ok(result.into())
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let Some(session) = session_of(&context) else {
            return Err(McpError::invalid_request("unauthenticated", None));
        };
        tasks::get(&self.deps.pool, &session, &request.task_id).await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let Some(session) = session_of(&context) else {
            return Err(McpError::invalid_request("unauthenticated", None));
        };
        tasks::update(
            &self.deps.pool,
            &session,
            &request.task_id,
            &request.input_responses,
            request.meta,
        )
        .await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let Some(session) = session_of(&context) else {
            return Err(McpError::invalid_request("unauthenticated", None));
        };
        tasks::cancel(&self.deps.pool, &session, &request.task_id).await
    }
}

/// The `/mcp` tower service. Mount it with `route_service`; the caller's auth middleware
/// must run first (it inserts the [`Session`] the handler reads).
pub fn service(deps: McpDeps) -> StreamableHttpService<McpServer, LocalSessionManager> {
    // Non-exhaustive in rmcp: start from Default and set the two fields this service owns.
    // One session per client (the credential, and so the tenant, is fixed for its life);
    // a tool call is a bounded request/response, so JSON is simpler than SSE and identical
    // on the wire for the actions this slice serves.
    let mut config = StreamableHttpServerConfig::default();
    config.legacy_session_mode = true;
    config.json_response = true;
    let config = match std::env::var("CONVEYOR_MCP_ALLOWED_HOSTS") {
        Ok(hosts) if !hosts.trim().is_empty() => {
            config.with_allowed_hosts(hosts.split(',').map(str::trim).filter(|h| !h.is_empty()))
        }
        _ => config,
    };
    let server = McpServer::new(deps);
    StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}

/// A helper for the parity test and S10: invoke through the registry as MCP would, but
/// without a transport. It is the same function `call_tool` calls.
pub async fn mcp_tool_body(
    pool: &PgPool,
    spec: &ActionSpec,
    session: &Session,
    arguments: Value,
    request_id: Option<String>,
    app_key: String,
) -> (u16, Value) {
    let request = InvokeRequest {
        action: spec.name.to_owned(),
        input: arguments,
        precondition: None,
        idempotency_key: None,
        actor_id: session.principal.user_id.as_uuid(),
        actor_kind: session.principal.kind.as_actor_kind().to_owned(),
        role: session.role,
        policy: session.policy.clone(),
        authority: Authority::of(session),
        request_id,
        trace_id: None,
        depth: 0,
        app_key,
        usage: None,
    };
    match invoke(pool, registry(), session.tenant, request).await {
        Ok(outcome) => (outcome.status, outcome.body),
        Err(error) => {
            let instance = spec.route_path();
            (error.status, error.problem(Some(&instance), None))
        }
    }
}
