//! T15 — BYO MCP: register, discover, bind a narrowed subset and audit every call
//! (SPEC_V2 §11.3 steps 5–7, §11.5 item 2, D42; issue #86).
//!
//! The MCP endpoint is a real streamable-HTTP server started in the test — the same `rmcp`
//! transport the conveyor serves, exercised as a client. The issue's default is applied:
//! `rmcp`'s client transport on the existing pin, no second MCP stack.
//!
//! Scenario names are the issue's `Verify:` targets.

mod support;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use conveyor_domain::TenantId;
use conveyor_store as store;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData as McpError,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::StreamableHttpServerConfig;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use serde_json::{Value, json};
use uuid::Uuid;

/// The BYO server's two tools and its call counter.
#[derive(Clone)]
struct ByoServer {
    calls: Arc<AtomicUsize>,
}

fn input_schema(properties: Value) -> Arc<serde_json::Map<String, Value>> {
    Arc::new(
        json!({
            "type": "object",
            "properties": properties,
            "required": ["message"],
            "additionalProperties": false
        })
        .as_object()
        .expect("an object schema")
        .clone(),
    )
}

impl ServerHandler for ByoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("a tenant's own MCP server")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: vec![
                Tool::new(
                    "echo",
                    "Echo a message",
                    input_schema(json!({"message": {"type": "string"}})),
                ),
                Tool::new(
                    "now",
                    "Return the time",
                    input_schema(json!({"message": {"type": "string"}})),
                ),
            ],
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
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let arguments = request.arguments.unwrap_or_default();
        let message = arguments.get("message").cloned().unwrap_or(Value::Null);
        let body = json!({"tool": request.name, "echo": message});
        let mut result = CallToolResult::success(vec![ContentBlock::text(body.to_string())]);
        result.structured_content = Some(body);
        Ok(result.into())
    }
}

/// A locally served streamable-HTTP MCP endpoint, plus the bearer it saw.
struct ByoEndpoint {
    base: String,
    calls: Arc<AtomicUsize>,
    bearer: Arc<Mutex<Vec<String>>>,
}

impl ByoEndpoint {
    async fn start() -> ByoEndpoint {
        let calls = Arc::new(AtomicUsize::new(0));
        let bearer = Arc::new(Mutex::new(Vec::new()));
        let handler = ByoServer {
            calls: calls.clone(),
        };
        let mut config = StreamableHttpServerConfig::default();
        config.legacy_session_mode = true;
        config.json_response = true;
        let service = StreamableHttpService::new(
            move || Ok(handler.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let state = bearer.clone();
        async fn capture(
            axum::extract::State(state): axum::extract::State<Arc<Mutex<Vec<String>>>>,
            request: axum::extract::Request,
            next: axum::middleware::Next,
        ) -> axum::response::Response {
            if let Some(value) = request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok())
            {
                state.lock().expect("lock").push(value.to_owned());
            }
            next.run(request).await
        }
        let app = axum::Router::new()
            .route_service("/mcp", service)
            .layer(axum::middleware::from_fn_with_state(state, capture));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the BYO endpoint");
        let addr = listener.local_addr().expect("endpoint addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        ByoEndpoint {
            base: format!("http://{addr}/mcp"),
            calls,
            bearer,
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn bearers(&self) -> Vec<String> {
        self.bearer.lock().expect("lock").clone()
    }
}

/// One tenant, the shared server, the registered source and a run.
struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    tenant_id: Uuid,
    running: support::Running,
    endpoint: ByoEndpoint,
    source_id: Uuid,
    reviewer: String,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let running = support::Running::start(&pg).await;
        let endpoint = ByoEndpoint::start().await;

        // The tenant's sealed credential; the registration binds it by alias.
        let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
            .await
            .expect("scoped tx");
        store::secret::create_secret_tx(
            &mut tx,
            seeded.tenant,
            "tenant_mcp.token",
            b"secret-token",
            "tenant",
        )
        .await
        .expect("create the credential");
        tx.commit().await.expect("commit the secret");

        let (status, registered) = running
            .invoke(
                "toolkits.register_mcp",
                "dev:alice",
                json!({
                    "key": "tenant_mcp",
                    "title": "Tenant MCP",
                    "url": endpoint.base,
                    "credential_alias": "tenant_mcp.token"
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "register: {registered}");
        let registered = registered.get("output").cloned().unwrap_or(registered);
        let source_id =
            Uuid::parse_str(registered["source_id"].as_str().expect("source id")).expect("uuid");
        Fixture {
            pg,
            tenant: seeded.tenant,
            tenant_id: seeded.tenant_id,
            running,
            endpoint,
            source_id,
            reviewer: support::REVIEWER.to_owned(),
        }
    }

    async fn invoke(&self, action: &str, token: &str, input: Value) -> (u16, Value) {
        let (status, body) = self
            .running
            .invoke(action, token, input, None, None, None)
            .await;
        // A success is the invocation envelope; a refusal is the problem document.
        let body = body.get("output").cloned().unwrap_or(body);
        (status, body)
    }

    /// Discover, validate, publish behind the human gate.
    async fn publish_discovered(&self) -> (Uuid, String) {
        let (status, discovered) = self
            .invoke(
                "toolkits.discover_mcp",
                "dev:alice",
                json!({"source_id": self.source_id}),
            )
            .await;
        assert_eq!(status, 200, "discover: {discovered}");
        assert_eq!(discovered["operations"], 2, "{discovered}");
        let draft_id = discovered["draft_id"]
            .as_str()
            .expect("draft id")
            .to_owned();
        let (status, validated) = self
            .invoke(
                "tool_authoring.validate",
                "dev:alice",
                json!({"draft_id": draft_id}),
            )
            .await;
        assert_eq!(status, 200, "validate: {validated}");
        assert_eq!(validated["valid"], true, "{validated}");
        let (status, pending) = self
            .invoke(
                "tool_authoring.publish",
                "dev:alice",
                json!({"draft_id": draft_id}),
            )
            .await;
        assert_eq!(status, 200, "publish: {pending}");
        let approval_id = pending["approval_id"].as_str().expect("gate").to_owned();
        let (status, decided) = self
            .invoke(
                "approvals.decide",
                &self.reviewer,
                json!({"approval_id": approval_id, "decision": "approve"}),
            )
            .await;
        assert_eq!(status, 200, "decide: {decided}");
        let (status, published) = self
            .invoke(
                "tool_authoring.publish",
                "dev:alice",
                json!({"draft_id": draft_id, "approval_id": approval_id}),
            )
            .await;
        assert_eq!(status, 200, "publish after approval: {published}");
        assert_eq!(published["outcome"], "published", "{published}");
        (draft_id.parse().expect("uuid"), draft_id)
    }

    async fn run(&self) -> (Uuid, Uuid) {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let project_id: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(self.tenant_id)
        .bind(format!("BYO-{}", Uuid::now_v7().simple()))
        .bind("BYO proof")
        .fetch_one(&mut *tx)
        .await
        .expect("insert the project");
        let run_id: Uuid = sqlx::query_scalar(
            "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
        )
        .bind(self.tenant_id)
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the run");
        tx.commit().await.expect("commit");
        (project_id, run_id)
    }

    /// The binding names the bound subset and the credential alias — binding-scoped discovery.
    async fn bind(&self, run_id: Uuid, tool_keys: Vec<String>) -> Uuid {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let toolkit_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM toolkit WHERE tenant_id = $1 AND key = 'tenant_mcp'
              ORDER BY toolkit_version DESC LIMIT 1",
        )
        .bind(self.tenant_id)
        .fetch_one(&mut *tx)
        .await
        .expect("the toolkit is published");
        let binding_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling,
                                       credential_alias, tool_keys)
             VALUES ($1, $2, $3, 'read', 'tenant_mcp.token', $4) RETURNING id",
        )
        .bind(self.tenant_id)
        .bind(run_id)
        .bind(toolkit_id)
        .bind(&tool_keys)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the binding");
        tx.commit().await.expect("commit");
        binding_id
    }

    async fn ledger(&self, run_id: Uuid) -> Vec<(Option<Uuid>, String, String)> {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let rows = sqlx::query_as(
            "SELECT binding_id, tool_key, status FROM tool_call
              WHERE tenant_id = $1 AND run_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant_id)
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await
        .expect("read the ledger");
        tx.commit().await.expect("commit");
        rows
    }
}

#[tokio::test]
async fn only_the_bound_subset_is_callable_and_each_call_is_audited() {
    let fixture = Fixture::new().await;
    fixture.publish_discovered().await;

    let (_project, run_id) = fixture.run().await;
    let binding_id = fixture.bind(run_id, vec!["echo".to_owned()]).await;

    // The bound tool is the job's tool set — and nothing else is.
    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let context = store::tool::run_context(&mut tx, fixture.tenant, run_id)
        .await
        .expect("read the run")
        .expect("the run exists");
    let bound = store::tool::bound_tools(&mut tx, fixture.tenant, &context)
        .await
        .expect("the bound set");
    tx.commit().await.expect("commit");
    let keys: Vec<&str> = bound.iter().map(|tool| tool.tool_key.as_str()).collect();
    assert_eq!(keys, vec!["echo"], "only the bound subset is projected");

    let (status, called) = fixture
        .invoke(
            "tools.call",
            "dev:alice",
            json!({
                "run_id": run_id,
                "tool": "tenant_mcp.echo",
                "input": {"message": "hello"}
            }),
        )
        .await;
    assert_eq!(status, 200, "the bound call: {called}");
    assert_eq!(called["echo"], "hello", "{called}");
    assert_eq!(fixture.endpoint.calls(), 1);
    assert!(
        fixture
            .endpoint
            .bearers()
            .iter()
            .any(|value| value == "Bearer secret-token"),
        "the broker used the binding's credential alias: {:?}",
        fixture.endpoint.bearers()
    );

    let ledger = fixture.ledger(run_id).await;
    assert_eq!(ledger.len(), 1, "one tool_call row for the bound call");
    assert_eq!(ledger[0].0, Some(binding_id), "the row names the binding");
    assert_eq!(ledger[0].1, "tenant_mcp.echo");
    assert_eq!(ledger[0].2, "ok");
    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND type = 'tool.called'
            AND payload ->> 'run_id' = $2",
    )
    .bind(fixture.tenant_id)
    .bind(run_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .expect("count the events");
    tx.commit().await.expect("commit");
    assert_eq!(events, 1, "one tool.called event per call");

    // The unbound tool is absent from the job's set and refused before the destination.
    let (status, refused) = fixture
        .invoke(
            "tools.call",
            "dev:alice",
            json!({
                "run_id": run_id,
                "tool": "tenant_mcp.now",
                "input": {"message": "hello"}
            }),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "tool_not_ready", "{refused}");
    assert_eq!(
        fixture.endpoint.calls(),
        1,
        "the refused call never reached MCP"
    );
    assert_eq!(fixture.ledger(run_id).await.len(), 1);
}

#[tokio::test]
async fn a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped() {
    let fixture = Fixture::new().await;
    fixture.publish_discovered().await;
    let (_project, run_id) = fixture.run().await;
    fixture.bind(run_id, vec!["echo".to_owned()]).await;

    // A caller-supplied URL at call time: the closed `tools.call` schema refuses it.
    let (status, refused) = fixture
        .invoke(
            "tools.call",
            "dev:alice",
            json!({
                "run_id": run_id,
                "tool": "tenant_mcp.echo",
                "input": {"message": "hello"},
                "url": "http://127.0.0.1:9/evil"
            }),
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error["field"] == "url"),
        "{refused}"
    );
    assert_eq!(fixture.endpoint.calls(), 0, "no destination call");
    assert!(fixture.ledger(run_id).await.is_empty(), "no ledger row");

    // Another tenant cannot see the source, the toolkit, or the run.
    let other = support::seed(&fixture.pg, "dev-b", "bob", "Bob").await;
    let (status, refused) = fixture
        .invoke(
            "tools.call",
            "dev:bob",
            json!({
                "run_id": run_id,
                "tool": "tenant_mcp.echo",
                "input": {"message": "hello"}
            }),
        )
        .await;
    assert_eq!(
        status, 409,
        "another tenant's run is not visible: {refused}"
    );
    let (status, refused) = fixture
        .invoke(
            "toolkits.discover_mcp",
            "dev:bob",
            json!({"source_id": fixture.source_id}),
        )
        .await;
    assert_eq!(
        status, 404,
        "another tenant's source is not visible: {refused}"
    );

    let mut tx = store::begin_scoped(&fixture.pg.owner, other.tenant)
        .await
        .expect("scoped tx");
    let toolkits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM toolkit WHERE tenant_id = $1 AND key = 'tenant_mcp'",
    )
    .bind(other.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count");
    tx.commit().await.expect("commit");
    assert_eq!(toolkits, 0, "the BYO toolkit is tenant-scoped");
    assert_eq!(fixture.endpoint.calls(), 0, "still no destination call");
}
