//! T15 — tool authoring: spec → draft → published tool, the publication gate and the effect
//! ceiling (SPEC_V2 §11.4–§11.6, §11.8, D43, D50; issue #86).
//!
//! Every test runs against a fresh Postgres 18 and a real HTTP destination. The destination's
//! counter is the instrument: a publication that is refused must write no version AND make no
//! call, so the assertion is on the rows and on the wire.
//!
//! Scenario names are the issue's `Verify:` targets, one test each.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A destination stub: counts every request and echoes the method and path.
struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct StubState {
    hits: Arc<AtomicUsize>,
}

async fn answer(
    axum::extract::State(state): axum::extract::State<StubState>,
    method: axum::http::Method,
    uri: axum::http::Uri,
) -> axum::Json<Value> {
    state.hits.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({"stub": true, "method": method.as_str(), "path": uri.path()}))
}

impl Stub {
    async fn start() -> Stub {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .fallback(axum::routing::any(answer))
            .with_state(StubState { hits: hits.clone() });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the destination stub");
        let addr = listener.local_addr().expect("stub addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Stub {
            base: format!("http://{addr}"),
            hits,
        }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// One tenant, the shared server and a run that can be bound.
struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    tenant_id: Uuid,
    running: support::Running,
    reviewer: String,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let running = support::Running::start(&pg).await;
        Fixture {
            pg,
            tenant: seeded.tenant,
            tenant_id: seeded.tenant_id,
            running,
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

    /// One project and one running run — the binding's first-class target.
    async fn run(&self) -> (Uuid, Uuid) {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let project_id: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(self.tenant_id)
        .bind(format!("TA-{}", Uuid::now_v7().simple()))
        .bind("Tool authoring proof")
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
        tx.commit().await.expect("commit the fixture");
        (project_id, run_id)
    }

    /// Bind one published toolkit version to a run, optionally narrowed to a tool subset.
    async fn bind(
        &self,
        run_id: Uuid,
        toolkit_key: &str,
        tool_keys: Vec<String>,
        ceiling: &str,
    ) -> Uuid {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let toolkit_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM toolkit WHERE tenant_id = $1 AND key = $2
              ORDER BY toolkit_version DESC LIMIT 1",
        )
        .bind(self.tenant_id)
        .bind(toolkit_key)
        .fetch_one(&mut *tx)
        .await
        .expect("the toolkit is published");
        let binding_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, tool_keys)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(self.tenant_id)
        .bind(run_id)
        .bind(toolkit_id)
        .bind(ceiling)
        .bind(&tool_keys)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the binding");
        tx.commit().await.expect("commit the binding");
        binding_id
    }

    async fn toolkit_rows(&self, key: &str) -> i64 {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let count =
            sqlx::query_scalar("SELECT count(*) FROM toolkit WHERE tenant_id = $1 AND key = $2")
                .bind(self.tenant_id)
                .bind(key)
                .fetch_one(&mut *tx)
                .await
                .expect("count the toolkit versions");
        tx.commit().await.expect("commit");
        count
    }

    async fn tool_effects(&self, key: &str) -> Vec<String> {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let effects = sqlx::query_scalar(
            "SELECT t.effect FROM tool t JOIN toolkit k ON k.id = t.toolkit_id
              WHERE t.tenant_id = $1 AND k.key = $2 ORDER BY t.key",
        )
        .bind(self.tenant_id)
        .bind(key)
        .fetch_all(&mut *tx)
        .await
        .expect("read the tool rows");
        tx.commit().await.expect("commit");
        effects
    }

    async fn approval_row(
        &self,
        approval_id: Uuid,
    ) -> (String, i32, Option<chrono::DateTime<chrono::Utc>>, Value) {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let row = sqlx::query_as(
            "SELECT status, required_decisions, expires_at, meta FROM approval
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant_id)
        .bind(approval_id)
        .fetch_one(&mut *tx)
        .await
        .expect("read the approval");
        tx.commit().await.expect("commit");
        row
    }

    async fn decision_kinds(&self, approval_id: Uuid) -> Vec<String> {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        let kinds = sqlx::query_scalar(
            "SELECT decided_by_kind FROM approval_decision
              WHERE tenant_id = $1 AND approval_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant_id)
        .bind(approval_id)
        .fetch_all(&mut *tx)
        .await
        .expect("read the decisions");
        tx.commit().await.expect("commit");
        kinds
    }

    async fn set_policy(&self, policy: store::approval_policy::ApprovalPolicy) {
        let mut tx = store::begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scoped tx");
        store::approval_policy::set(&mut tx, self.tenant, &policy, None)
            .await
            .expect("set the publication policy");
        tx.commit().await.expect("commit the policy");
    }

    /// Import, validate, request publication, decide it as the reviewer, publish.
    async fn publish_approved(&self, key: &str, document: Value, base_url: &str) -> Value {
        let (status, imported) = self
            .invoke(
                "toolkits.import_openapi",
                "dev:alice",
                json!({
                    "key": key,
                    "title": format!("{key} toolkit"),
                    "document": document,
                    "base_url": base_url,
                    // The stub destination declares no auth: under §11.3 a bearer toolkit
                    // needs a binding credential, and this fixture has nothing to seal.
                    "auth_adapter": "none",
                }),
            )
            .await;
        assert_eq!(status, 201, "import: {imported}");
        let draft_id = imported["draft_id"].as_str().expect("draft id");
        let (status, validated) = self
            .invoke(
                "tool_authoring.validate",
                "dev:alice",
                json!({"draft_id": draft_id}),
            )
            .await;
        assert_eq!(status, 200, "validate: {validated}");
        assert_eq!(
            validated["valid"], true,
            "the draft must validate: {validated}"
        );
        let (status, pending) = self
            .invoke(
                "tool_authoring.publish",
                "dev:alice",
                json!({"draft_id": draft_id}),
            )
            .await;
        assert_eq!(status, 200, "publish: {pending}");
        let approval_id = pending["approval_id"].as_str().expect("the gate");
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
        published
    }
}

/// A closed read operation over `POST /widgets`.
fn widgets_document(operation_id: &str) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {"title": "widgets", "version": "1.0.0"},
        "paths": {
            "/widgets": {
                "post": {
                    "operationId": operation_id,
                    "summary": "Create a widget",
                    "requestBody": {
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {"name": {"type": "string"}},
                                    "required": ["name"],
                                    "additionalProperties": false
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {"id": {"type": "string"}},
                                        "required": ["id"]
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[tokio::test]
async fn a_tool_generated_from_a_spec_is_published_and_called() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;

    let published = fixture
        .publish_approved("weather", widgets_document("createWidget"), &stub.base)
        .await;
    assert_eq!(published["toolkit_version"], 1, "{published}");
    assert_eq!(
        published["max_effect"], "read",
        "a missing effect class defaults to read: {published}"
    );

    // One immutable version: an unchanged draft publishes nothing, and a second published
    // version cannot appear under the same draft.
    let draft_id = published["draft_id"].as_str().expect("draft id");
    let (status, again) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id}),
        )
        .await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["outcome"], "unchanged", "{again}");
    assert_eq!(fixture.toolkit_rows("weather").await, 1);
    assert_eq!(
        fixture.tool_effects("weather").await,
        vec!["read".to_owned()]
    );

    // The published toolkit names the registered destination alias, not a URL.
    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let service: String =
        sqlx::query_scalar("SELECT service FROM toolkit WHERE tenant_id = $1 AND key = 'weather'")
            .bind(fixture.tenant_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the toolkit service");
    assert_eq!(service, "weather");
    tx.commit().await.expect("commit");

    // A bound job calls it by that pin.
    let (_project, run_id) = fixture.run().await;
    let binding_id = fixture.bind(run_id, "weather", Vec::new(), "read").await;
    let (status, called) = fixture
        .invoke(
            "tools.call",
            "dev:alice",
            json!({
                "run_id": run_id,
                "tool": "weather.createwidget",
                "input": {"name": "a widget"}
            }),
        )
        .await;
    assert_eq!(status, 200, "the bound call: {called}");
    assert_eq!(
        called["path"], "/widgets",
        "the destination's own path: {called}"
    );
    assert_eq!(called["method"], "POST");
    assert_eq!(stub.hits(), 1, "exactly one destination call");

    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let calls: Vec<(Option<Uuid>, i64, String)> = sqlx::query_as(
        "SELECT binding_id, toolkit_version, status FROM tool_call
          WHERE tenant_id = $1 AND run_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the ledger");
    tx.commit().await.expect("commit");
    assert_eq!(calls.len(), 1, "one tool_call row per call");
    assert_eq!(calls[0].0, Some(binding_id), "the ledger names the binding");
    assert_eq!(calls[0].1, 1, "the call pinned the published version");
    assert_eq!(calls[0].2, "ok");

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
}

#[tokio::test]
async fn a_scaffolded_service_registers_its_own_openapi() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;

    // The scaffolded service's own document: one operation, its own path. There is no wrapper
    // operation between the toolkit and the service.
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "widgets service", "version": "1.0.0"},
        "paths": {
            "/widgets": {
                "get": {
                    "operationId": "listWidgets",
                    "responses": {
                        "200": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {"widgets": {"type": "array"}},
                                        "required": ["widgets"]
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let published = fixture
        .publish_approved("widgets", document, &stub.base)
        .await;

    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let toolkits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM toolkit WHERE tenant_id = $1 AND key = 'widgets'")
            .bind(fixture.tenant_id)
            .fetch_one(&mut *tx)
            .await
            .expect("count the toolkits");
    let tools: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT t.key, k.service, t.destination FROM tool t
           JOIN toolkit k ON k.id = t.toolkit_id
          WHERE t.tenant_id = $1 AND k.key = 'widgets'",
    )
    .bind(fixture.tenant_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the tool");
    tx.commit().await.expect("commit");
    assert_eq!(toolkits, 1, "one toolkit, no wrapper around the service");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].0, "listwidgets");
    assert_eq!(
        tools[0].1, "widgets",
        "the toolkit is the service's own alias"
    );
    assert_eq!(
        tools[0].2["path"], "/widgets",
        "the operation is the service's own path"
    );

    let (_project, run_id) = fixture.run().await;
    fixture.bind(run_id, "widgets", Vec::new(), "read").await;
    let (status, called) = fixture
        .invoke(
            "tools.call",
            "dev:alice",
            json!({"run_id": run_id, "tool": "widgets.listwidgets", "input": {}}),
        )
        .await;
    assert_eq!(status, 200, "{called}");
    assert_eq!(called["method"], "GET");
    assert_eq!(called["path"], "/widgets", "no wrapper path: {called}");
    assert_eq!(stub.hits(), 1);
    assert_eq!(published["toolkit_version"], 1);
}

#[tokio::test]
async fn publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;
    let base = stub.base.clone();

    let mut wildcard = widgets_document("wildcardOp");
    wildcard["paths"]["/widgets"]["post"]["x-egress"] = json!(["*"]);
    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({"key": "wild", "title": "Wild", "document": wildcard, "base_url": base}),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let (status, refused) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": imported["draft_id"]}),
        )
        .await;
    assert_eq!(status, 422, "wildcard egress: {refused}");
    assert!(
        refused["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error["code"] == "egress_wildcard"),
        "{refused}"
    );
    assert_eq!(fixture.toolkit_rows("wild").await, 0, "no version written");

    let mut open = widgets_document("openOp");
    open["paths"]["/widgets"]["post"]["requestBody"]["content"]["application/json"]["schema"] =
        json!({"type": "object"});
    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({"key": "open", "title": "Open", "document": open, "base_url": &stub.base}),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let (status, refused) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": imported["draft_id"]}),
        )
        .await;
    assert_eq!(status, 422, "free-form schema: {refused}");
    assert!(
        refused["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error["code"] == "input_schema_open"),
        "{refused}"
    );
    assert_eq!(fixture.toolkit_rows("open").await, 0);

    let mut write = widgets_document("writeOp");
    write["paths"]["/widgets"]["post"]["x-effect"] = json!("write");
    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({"key": "writ", "title": "Write", "document": write, "base_url": &stub.base}),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let (status, refused) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": imported["draft_id"]}),
        )
        .await;
    assert_eq!(status, 422, "write without idempotency: {refused}");
    assert!(
        refused["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error["code"] == "idempotency_missing"),
        "{refused}"
    );
    assert_eq!(fixture.toolkit_rows("writ").await, 0);

    // No declared effect class: the publication defaults it to `read`.
    let published = fixture
        .publish_approved("defaulted", widgets_document("defaultedOp"), &stub.base)
        .await;
    assert_eq!(published["max_effect"], "read", "{published}");
    assert_eq!(
        fixture.tool_effects("defaulted").await,
        vec!["read".to_owned()]
    );
    assert_eq!(
        stub.hits(),
        0,
        "no refused publication made a destination call"
    );
}

#[tokio::test]
async fn publication_waits_for_a_human_by_default() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;

    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({
                "key": "human",
                "title": "Human gate",
                "document": widgets_document("humanOp"),
                "base_url": &stub.base,
            }),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let draft_id = imported["draft_id"].as_str().expect("draft id");

    let (status, pending) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id}),
        )
        .await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["outcome"], "pending_approval", "{pending}");
    assert_eq!(
        pending["approver_kind"], "human",
        "the default approver is a person"
    );
    assert_eq!(pending["required_decisions"], 1);
    let approval_id =
        Uuid::parse_str(pending["approval_id"].as_str().expect("gate")).expect("uuid");
    let (approval_status, required, _expires, _meta) = fixture.approval_row(approval_id).await;
    assert_eq!(approval_status, "pending");
    assert_eq!(required, 1);
    assert_eq!(
        fixture.toolkit_rows("human").await,
        0,
        "no version before the decision"
    );

    let (status, decided) = fixture
        .invoke(
            "approvals.decide",
            &fixture.reviewer,
            json!({"approval_id": approval_id, "decision": "approve"}),
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    assert_eq!(
        fixture.decision_kinds(approval_id).await,
        vec!["human".to_owned()],
        "the approval records a human decided_by_kind"
    );

    let (status, published) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id, "approval_id": approval_id}),
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(published["outcome"], "published", "{published}");
    let (_, _, _, meta) = fixture.approval_row(approval_id).await;
    assert_eq!(
        meta["published"]["toolkit_version"], 1,
        "the published version is recorded on the approval: {meta}"
    );
}

#[tokio::test]
async fn a_tenant_policy_changes_the_publication_gate_without_a_code_change() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;

    // (a) A different approver kind: an earned auto policy with a `max_effect` term and an
    // expiry. The row — not the code — decides.
    let expires = chrono::Utc::now() + chrono::Duration::hours(1);
    fixture
        .set_policy(store::approval_policy::ApprovalPolicy {
            action: "tool_authoring.publish".to_owned(),
            approval: store::approval_policy::ApprovalRequirement::Required,
            self_approval: store::approval_policy::SelfApproval::Forbidden,
            approver_kind: conveyor_domain::development::ApproverKind::Auto,
            conditions: json!({"max_effect": "read"}),
            expires_at: Some(expires),
            required_decisions: 1,
        })
        .await;

    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({
                "key": "auto",
                "title": "Auto gate",
                "document": widgets_document("autoOp"),
                "base_url": &stub.base,
            }),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let (status, published) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": imported["draft_id"]}),
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(
        published["outcome"], "published",
        "the auto policy publishes a read tool with no human decision: {published}"
    );
    assert_eq!(published["approver_kind"], "auto");
    let approval_id =
        Uuid::parse_str(published["approval_id"].as_str().expect("gate")).expect("uuid");
    let (approval_status, _, recorded_expiry, meta) = fixture.approval_row(approval_id).await;
    assert_eq!(approval_status, "approved");
    assert_eq!(
        recorded_expiry.map(|value| value.timestamp()),
        Some(expires.timestamp()),
        "the policy's expiry is the gate's expiry"
    );
    assert_eq!(meta["published"]["toolkit_version"], 1);
    assert_eq!(
        fixture.decision_kinds(approval_id).await,
        vec!["rule".to_owned()],
        "an auto decision is a rule, distinguishable after the fact"
    );

    // (b) A different quorum: two human decisions, the requester included.
    fixture
        .set_policy(store::approval_policy::ApprovalPolicy {
            action: "tool_authoring.publish".to_owned(),
            approval: store::approval_policy::ApprovalRequirement::Required,
            self_approval: store::approval_policy::SelfApproval::Forbidden,
            approver_kind: conveyor_domain::development::ApproverKind::Human,
            conditions: json!({}),
            expires_at: None,
            required_decisions: 2,
        })
        .await;
    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({
                "key": "quorum",
                "title": "Two decisions",
                "document": widgets_document("quorumOp"),
                "base_url": &stub.base,
            }),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let draft_id = imported["draft_id"].as_str().expect("draft id");
    let (status, pending) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id}),
        )
        .await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["required_decisions"], 2, "{pending}");
    let approval_id =
        Uuid::parse_str(pending["approval_id"].as_str().expect("gate")).expect("uuid");
    // §5.4 makes the requester's exclusion an invariant: a quorum of two is two OTHER
    // principals, never the requester plus one.
    support::seed(&fixture.pg, "dev-a", "carol", "Carol").await;
    for token in [fixture.reviewer.clone(), "dev:carol".to_owned()] {
        let (status, decided) = fixture
            .invoke(
                "approvals.decide",
                &token,
                json!({"approval_id": approval_id, "decision": "approve"}),
            )
            .await;
        assert_eq!(status, 200, "{decided}");
    }
    let (status, published) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id, "approval_id": approval_id}),
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(published["outcome"], "published", "{published}");
    let (_, required, _, _) = fixture.approval_row(approval_id).await;
    assert_eq!(required, 2, "the tenant's quorum is on the gate");
}

#[tokio::test]
async fn a_destructive_tool_needs_a_named_approver() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;

    let mut document = widgets_document("purgeOp");
    document["paths"]["/widgets"]["post"]["x-effect"] = json!("destructive");
    document["paths"]["/widgets"]["post"]["x-idempotency"] = json!("required");
    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({
                "key": "purge",
                "title": "Purge",
                "document": document,
                "base_url": &stub.base,
            }),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let draft_id = imported["draft_id"].as_str().expect("draft id");

    // The security review names the approver — the review half is satisfied.
    let (status, reviewed) = fixture
        .invoke(
            "tool_authoring.review",
            "dev:alice",
            json!({
                "draft_id": draft_id,
                "decision": "approve",
                "named_approver": "Alice"
            }),
        )
        .await;
    assert_eq!(status, 200, "{reviewed}");

    // The policy names no approver: a waiver is not a named approver for a destructive tool.
    fixture
        .set_policy(store::approval_policy::ApprovalPolicy {
            action: "tool_authoring.publish".to_owned(),
            approval: store::approval_policy::ApprovalRequirement::Waived,
            self_approval: store::approval_policy::SelfApproval::Forbidden,
            approver_kind: conveyor_domain::development::ApproverKind::Human,
            conditions: json!({}),
            expires_at: None,
            required_decisions: 1,
        })
        .await;

    let (status, refused) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id}),
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["type"], "approver_required", "{refused}");
    assert_eq!(fixture.toolkit_rows("purge").await, 0, "no version written");
    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND gate = 'tool_authoring.publish'",
    )
    .bind(fixture.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count the gates");
    tx.commit().await.expect("commit");
    assert_eq!(gates, 0, "the refusal opens no gate either");
}

#[tokio::test]
async fn the_publishing_job_cannot_widen_its_own_effect_ceiling() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;

    let mut document = widgets_document("purgeOp");
    document["paths"]["/widgets"]["post"]["x-effect"] = json!("destructive");
    document["paths"]["/widgets"]["post"]["x-idempotency"] = json!("required");
    let (status, imported) = fixture
        .invoke(
            "toolkits.import_openapi",
            "dev:alice",
            json!({
                "key": "widen",
                "title": "Widen",
                "document": document,
                "base_url": &stub.base,
            }),
        )
        .await;
    assert_eq!(status, 201, "{imported}");
    let draft_id = imported["draft_id"].as_str().expect("draft id");
    let (status, _reviewed) = fixture
        .invoke(
            "tool_authoring.review",
            "dev:alice",
            json!({"draft_id": draft_id, "decision": "approve", "named_approver": "Alice"}),
        )
        .await;
    assert_eq!(status, 200);

    // A published read toolkit exists so a binding has a toolkit to pin; the grant is what the
    // publishing job holds.
    fixture
        .publish_approved("authoring", widgets_document("authoringOp"), &stub.base)
        .await;
    let (_project, run_id) = fixture.run().await;
    let binding_id = fixture.bind(run_id, "authoring", Vec::new(), "write").await;

    let (status, refused) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id, "binding_id": binding_id}),
        )
        .await;
    assert_eq!(
        status, 403,
        "a write binding cannot publish destructive: {refused}"
    );
    assert_eq!(refused["type"], "effect_ceiling", "{refused}");
    assert_eq!(fixture.toolkit_rows("widen").await, 0, "no version written");

    // The same draft publishes once the job's grant is not the narrower one: the ceiling, not
    // the draft, was the refusal.
    let (status, pending) = fixture
        .invoke(
            "tool_authoring.publish",
            "dev:alice",
            json!({"draft_id": draft_id}),
        )
        .await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["outcome"], "pending_approval", "{pending}");
}

#[tokio::test]
async fn promotion_is_not_an_admin_toggle() {
    let fixture = Fixture::new().await;
    let stub = Stub::start().await;
    let published = fixture
        .publish_approved("tenant_tool", widgets_document("tenantOp"), &stub.base)
        .await;
    let toolkit_id =
        Uuid::parse_str(published["toolkit_id"].as_str().expect("toolkit id")).expect("uuid");

    // The tenant admin alone is refused.
    let (status, refused) = fixture
        .invoke(
            "tool_authoring.promote",
            "dev:alice",
            json!({"toolkit_id": toolkit_id}),
        )
        .await;
    assert_eq!(status, 403, "{refused}");
    assert_eq!(refused["type"], "platform_review_required", "{refused}");

    // A development project with no approved review is still refused.
    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(fixture.tenant_id)
    .bind(format!("DEV-{}", Uuid::now_v7().simple()))
    .bind("Promotion review")
    .fetch_one(&mut *tx)
    .await
    .expect("insert the project");
    let implementer = support::reviewer_id(&fixture.pg).await;
    let development_project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO development_project (tenant_id, project_id, repo, lane, implementer_id,
                                          acceptance_command)
         VALUES ($1, $2, 'nonagenticai/conveyor', 'issue/promotion', $3, 'cargo test')
         RETURNING id",
    )
    .bind(fixture.tenant_id)
    .bind(project_id)
    .bind(implementer)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the development project");
    tx.commit().await.expect("commit");

    let (status, refused) = fixture
        .invoke(
            "tool_authoring.promote",
            "dev:alice",
            json!({"toolkit_id": toolkit_id, "development_project_id": development_project_id}),
        )
        .await;
    assert_eq!(status, 403, "no approved review: {refused}");
    assert_eq!(refused["type"], "platform_review_required");

    // The sanctioned path: an approved platform review on the development project.
    let mut tx = store::begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scoped tx");
    let review_id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, project_id, gate, kind, status, required_decisions, meta)
         VALUES ($1, $2, 'development_projects.review', 'development', 'approved', 1, $3)
         RETURNING id",
    )
    .bind(fixture.tenant_id)
    .bind(project_id)
    .bind(json!({"development_project_id": development_project_id.to_string()}))
    .fetch_one(&mut *tx)
    .await
    .expect("insert the review gate");
    tx.commit().await.expect("commit");

    let (status, promoted) = fixture
        .invoke(
            "tool_authoring.promote",
            "dev:alice",
            json!({"toolkit_id": toolkit_id, "development_project_id": development_project_id}),
        )
        .await;
    assert_eq!(status, 200, "{promoted}");
    assert_eq!(promoted["status"], "requested", "{promoted}");
    assert_eq!(promoted["review_approval_id"], review_id.to_string());
}
