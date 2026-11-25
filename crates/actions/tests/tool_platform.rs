//! T13 — the tool platform (SPEC_V2 §11.3–§11.10; issue #76).
//!
//! Every test here runs against a fresh Postgres 18 and a real HTTP stub. The stub's hit counter is
//! the instrument for "refused before the backend": a refusal that still called upstream would leave
//! the counter moved, so the assertion is on the wire, not on the error text alone.
//!
//! The tool rows are written directly (this slice ships no catalogue actions; the estate import is
//! T14). That is the point of the test: the broker's authority is the binding row, not a compiled
//! constant, so a fixture that binds a tool is exactly what a tenant's published toolkit will be.

mod support;

use bytes::Bytes;
use conveyor_actions::storage::{JobStorage, TENANT_PREFIX};
use conveyor_domain::TenantId;
use conveyor_store::{begin_scoped, tool};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// The external-service base URL is process-global (`support::set_env`), and every broker test
/// points it at its own stub. The suite therefore runs one test at a time: serialising on this
/// lock keeps each test's stub its own, and every scenario keeps the name its `Verify:` row uses.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A counting destination stub.
///
/// `support::Stub` counts only the routes its breaker test needs; the broker's instrument is
/// "did ANY request reach the destination", so this stub counts every request it answers. It
/// returns the path, which is how a test tells the services apart, and echoes the caller's
/// `Authorization` header, which is how a credential test tells WHICH sealed secret the engine
/// opened.
struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct StubState {
    hits: Arc<AtomicUsize>,
    /// The answer every request gets; `None` is the path-and-authorization echo the refusal
    /// and credential tests rely on.
    reply: Option<Value>,
    /// How long every answer waits before it is sent. The long-operation test needs a
    /// destination that outlives the tool's own timeout but still answers.
    delay_ms: u64,
}

async fn answer(
    axum::extract::State(state): axum::extract::State<StubState>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Option<axum::Json<Value>>,
) -> axum::Json<Value> {
    state.hits.fetch_add(1, Ordering::SeqCst);
    // The long-operation test needs a destination that is actually slow: the stub's configured
    // delay (and any `delay_ms` in the input) is honoured before answering, so the broker's own
    // timeout can fire while the call is in flight.
    let body_delay = body
        .as_ref()
        .and_then(|axum::Json(value)| value.get("delay_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let delay = state.delay_ms.max(body_delay).min(5_000);
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    }
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    axum::Json(state.reply.clone().unwrap_or_else(
        || json!({"stub": true, "path": uri.path(), "authorization": authorization}),
    ))
}

impl Stub {
    /// A stub that answers every request with the same document: the destination's half of a
    /// write-verification test, where the answer itself is what the broker judges.
    async fn start_with(reply: Value) -> Stub {
        Stub::start_with_reply(Some(reply), 0).await
    }

    /// A stub that waits `delay_ms` before every answer: the slow destination the broker's
    /// long-operation path needs.
    async fn start_with_delay(delay_ms: u64) -> Stub {
        Stub::start_with_reply(None, delay_ms).await
    }

    async fn start_with_reply(reply: Option<Value>, delay_ms: u64) -> Stub {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .fallback(axum::routing::any(answer))
            .with_state(StubState {
                hits: hits.clone(),
                reply,
                delay_ms,
            });
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

/// One run, one toolkit, one tool and one run-scoped binding — the fixture every broker test uses.
struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    run_id: Uuid,
    stub: Stub,
    running: support::Running,
}

impl Fixture {
    async fn new(
        toolkit_key: &str,
        tool_key: &str,
        handler_action: &str,
        effect: &str,
        ceiling: &str,
        input_schema: Value,
    ) -> Fixture {
        Fixture::build(
            toolkit_key,
            tool_key,
            handler_action,
            effect,
            ceiling,
            input_schema,
            None,
            0,
        )
        .await
    }

    /// [`Fixture::new`] with a destination that waits `delay_ms` before answering: the slow
    /// backend the broker's long-operation path needs, outliving the tool's timeout while its
    /// own action timeout still admits the call.
    async fn new_with_delay(
        toolkit_key: &str,
        tool_key: &str,
        handler_action: &str,
        effect: &str,
        ceiling: &str,
        input_schema: Value,
        delay_ms: u64,
    ) -> Fixture {
        Fixture::build(
            toolkit_key,
            tool_key,
            handler_action,
            effect,
            ceiling,
            input_schema,
            None,
            delay_ms,
        )
        .await
    }

    /// [`Fixture::new`] with the destination's answer pinned: a write-verification test needs the
    /// answer to be its own input, because the broker judges the document it got back.
    #[allow(clippy::too_many_arguments)]
    async fn new_with_reply(
        toolkit_key: &str,
        tool_key: &str,
        handler_action: &str,
        effect: &str,
        ceiling: &str,
        input_schema: Value,
        reply: Value,
    ) -> Fixture {
        Fixture::build(
            toolkit_key,
            tool_key,
            handler_action,
            effect,
            ceiling,
            input_schema,
            Some(reply),
            0,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn build(
        toolkit_key: &str,
        tool_key: &str,
        handler_action: &str,
        effect: &str,
        ceiling: &str,
        input_schema: Value,
        reply: Option<Value>,
        delay_ms: u64,
    ) -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let stub = match reply {
            Some(reply) => Stub::start_with(reply).await,
            None => Stub::start_with_delay(delay_ms).await,
        };
        // Both services the broker tests reach: `fixtures.*` and the coding action (SPEC §12 T13's
        // "a development job calls coding through the broker").
        support::set_env(support::STUB_ENV, &stub.base);
        support::set_env("CONVEYOR_EXTERNAL_CODING_URL", &stub.base);
        let (_project_id, run_id) = seed_run(&pg, seeded.tenant).await;
        seed_tool(
            &pg,
            seeded.tenant,
            toolkit_key,
            tool_key,
            handler_action,
            effect,
            ceiling,
            input_schema,
            run_id,
        )
        .await;
        let running = support::Running::start(&pg).await;
        Fixture {
            pg,
            tenant: seeded.tenant,
            run_id,
            stub,
            running,
        }
    }

    async fn call(&self, tool_name: &str, input: Value) -> (u16, Value) {
        self.running
            .invoke(
                "tools.call",
                "dev:alice",
                json!({"run_id": self.run_id, "tool": tool_name, "input": input}),
                None,
                None,
                None,
            )
            .await
    }

    /// [`Fixture::call`] with an `Idempotency-Key` header: the call-level key §11.3 names, which
    /// the route folds into `InvokeRequest.idempotency_key` (the middle of the three options).
    async fn call_with_key(&self, tool_name: &str, input: Value, key: &str) -> (u16, Value) {
        self.running
            .invoke(
                "tools.call",
                "dev:alice",
                json!({"run_id": self.run_id, "tool": tool_name, "input": input}),
                None,
                Some(key),
                None,
            )
            .await
    }

    /// Invoke one action through the same engine as the HTTP route, as the session's actor.
    async fn invoke_action(&self, action: &str, input: Value) -> (u16, Value) {
        self.running
            .invoke(action, "dev:alice", input, None, None, None)
            .await
    }

    /// Declare the tool version's idempotency policy (§11.3). The fixture writes the tool row
    /// directly, so the policy is test input exactly like the effect and the schema.
    async fn set_idempotency(&self, value: &str) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        sqlx::query(
            "UPDATE tool SET idempotency = $2 WHERE tenant_id = $1
              AND toolkit_id IN (SELECT id FROM toolkit WHERE tenant_id = $1)",
        )
        .bind(self.tenant.as_uuid())
        .bind(value)
        .execute(&mut *tx)
        .await
        .expect("set the tool's idempotency policy");
        tx.commit().await.expect("commit the idempotency policy");
    }

    /// Declare the binding's sealed credential alias (§11.8). The fixture inserts the binding
    /// with no alias — the state the credential-required refusal tests — so a test that wants a
    /// credentialed destination names the tenant secret here.
    async fn set_credential_alias(&self, alias: Option<&str>) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        sqlx::query(
            "UPDATE tool_binding SET credential_alias = $2, updated_at = now()
              WHERE tenant_id = $1",
        )
        .bind(self.tenant.as_uuid())
        .bind(alias)
        .execute(&mut *tx)
        .await
        .expect("set the binding's credential alias");
        tx.commit().await.expect("commit the credential alias");
    }

    /// Declare the toolkit's auth adapter (§11.8). The fixture seeds `none` — a destination that
    /// needs no tenant credential; `bearer` marks one that a binding must credential.
    async fn set_auth_adapter(&self, adapter: &str) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        sqlx::query(
            "UPDATE toolkit SET auth_adapter = $2, updated_at = now() WHERE tenant_id = $1",
        )
        .bind(self.tenant.as_uuid())
        .bind(adapter)
        .execute(&mut *tx)
        .await
        .expect("set the toolkit's auth adapter");
        tx.commit().await.expect("commit the auth adapter");
    }

    /// Declare the result fields the broker redacts (§11.3). The fixture seeds `[]` — the
    /// state a tool published before the declaration had a home — so a test that wants a
    /// redacted result names the fields here.
    async fn set_redact(&self, fields: &[&str]) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        sqlx::query("UPDATE tool SET redact = $2, updated_at = now() WHERE tenant_id = $1")
            .bind(self.tenant.as_uuid())
            .bind(json!(fields))
            .execute(&mut *tx)
            .await
            .expect("set the tool's redact fields");
        tx.commit().await.expect("commit the redact fields");
    }

    /// Seal one tenant secret, exactly as a tenant's BYO credential is written. Two named
    /// secrets let a test prove WHICH one the engine opened.
    async fn seal_secret(&self, name: &str, value: &str) {
        let mut tx = begin_scoped(&self.pg.runtime, self.tenant)
            .await
            .expect("a scoped secret write");
        conveyor_store::secret::create_secret_tx(
            &mut tx,
            self.tenant,
            name,
            value.as_bytes(),
            "tenant",
        )
        .await
        .expect("seal the tenant secret");
        tx.commit().await.expect("commit the sealed secret");
    }

    async fn ledger(&self) -> Vec<tool::ToolCallRow> {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped read");
        let rows = tool::calls_for_run(&mut tx, self.tenant, self.run_id)
            .await
            .expect("read the ledger");
        tx.commit().await.expect("commit the read");
        rows
    }
}

/// A published owner decision document — the authorisation a retirement names.
async fn owner_decision(fixture: &Fixture) -> Uuid {
    let id = Uuid::now_v7();
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, kind, title, status, category, body, publishable,
                               published_at)
         VALUES ($1, $2, 'document', 'Owner decision — retire the coding toolkit', 'active',
                 'decision', 'owner decision', true, now())",
    )
    .bind(id)
    .bind(fixture.tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect("insert the owner decision");
    tx.commit().await.expect("commit");
    id
}

/// A project and a running run. The run is the binding's first-class target (`tool_binding.run_id`),
/// so a test needs no job ladder to grant a tool.
async fn seed_run(pg: &support::Pg, tenant: TenantId) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("TOOL-{}", Uuid::now_v7().simple()))
    .bind("Tool platform proof")
    .fetch_one(&mut *tx)
    .await
    .expect("insert the project");
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the fixture");
    (project_id, run_id)
}

/// One published toolkit version, its tool, and a run-scoped binding with the given ceiling.
#[allow(clippy::too_many_arguments)]
async fn seed_tool(
    pg: &support::Pg,
    tenant: TenantId,
    toolkit_key: &str,
    tool_key: &str,
    handler_action: &str,
    effect: &str,
    ceiling: &str,
    input_schema: Value,
    run_id: Uuid,
) {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let toolkit_id: Uuid = sqlx::query_scalar(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, auth_adapter,
                              owner, status)
         VALUES ($1, $2, 1, $3, $4, 'none', 'platform', 'published') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_key)
    .bind(format!("{toolkit_key} toolkit"))
    .bind(toolkit_key)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the toolkit");
    let _tool_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, timeout_secs,
                           handler_action, status)
         VALUES ($1, $2, $3, $4, $5, 30, $6, 'published') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .bind(tool_key)
    .bind(&input_schema)
    .bind(effect)
    .bind(handler_action)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the tool");
    sqlx::query(
        "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
         VALUES ($1, $2, $3, $4, true)",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(toolkit_id)
    .bind(ceiling)
    .execute(&mut *tx)
    .await
    .expect("insert the binding");
    tx.commit().await.expect("commit the tool fixture");
}

fn coding_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {"type": "string"},
            "input": {"type": "object"}
        },
        "required": ["kind", "input"],
        "additionalProperties": false
    })
}

fn echo_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"message": {"type": "string"}},
        "required": ["message"],
        "additionalProperties": false
    })
}

/// `coding_jobs.get`'s input (`CodingJobGetIn`): the long-operation test's destination, whose
/// own action timeout (5 s) is longer than the tool timeout the test sets (1 s).
fn coding_get_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"id": {"type": "string"}},
        "required": ["id"],
        "additionalProperties": false
    })
}

// ── P1.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_job_calls_coding_through_the_broker() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    let before = fixture.stub.hits();

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;

    assert_eq!(
        status, 200,
        "the broker must dispatch the bound tool: {body}"
    );
    assert_eq!(
        fixture.stub.hits(),
        before + 1,
        "exactly one destination call"
    );
    assert_eq!(
        body["output"]["path"], "/api/jobs",
        "the call reached the registered coding destination alias: {body}"
    );

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declared_redact_field_is_replaced() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    fixture.set_redact(&["output"]).await;

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;

    assert_eq!(status, 200, "the broker must dispatch: {body}");
    assert_eq!(
        body["output"], "[redacted]",
        "the tool's declared field is replaced in the result: {body}"
    );

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_registered_binding_writer_creates_the_grant() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    // The fixture seeds its grant directly; remove it so the registered action is the only writer.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query("DELETE FROM tool_binding WHERE tenant_id = $1")
        .bind(fixture.tenant.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("clear the seeded grant");
    tx.commit().await.expect("commit");

    let (status, body) = fixture
        .invoke_action(
            "tool_bindings.create",
            json!({"toolkit_key": "coding", "run_id": fixture.run_id, "effect_ceiling": "write"}),
        )
        .await;
    assert_eq!(
        status, 200,
        "the binding writer must accept a published toolkit: {body}"
    );
    assert_eq!(body["output"]["toolkit_key"], "coding", "{body}");
    assert_eq!(body["output"]["enabled"], true, "{body}");

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;
    assert_eq!(
        status, 200,
        "the created grant must resolve the call: {body}"
    );
    assert_eq!(fixture.ledger().await.len(), 1, "one ledger row");

    // A draft toolkit cannot be bound.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query("UPDATE toolkit SET status = 'draft' WHERE tenant_id = $1")
        .bind(fixture.tenant.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("draft the toolkit");
    sqlx::query("DELETE FROM tool_binding WHERE tenant_id = $1")
        .bind(fixture.tenant.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("clear the grant again");
    tx.commit().await.expect("commit");
    let (status, body) = fixture
        .invoke_action(
            "tool_bindings.create",
            json!({"toolkit_key": "coding", "run_id": fixture.run_id, "effect_ceiling": "write"}),
        )
        .await;
    assert_eq!(status, 409, "a draft toolkit is refused: {body}");
    assert!(
        body.to_string().contains("toolkit_not_published"),
        "the refusal names the reason: {body}"
    );

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tools_retire_unbinds_a_zero_use_toolkit() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    let decision = owner_decision(&fixture).await;

    let (status, body) = fixture
        .invoke_action(
            "tools.retire",
            json!({"toolkit_key": "coding", "decision_ref": decision}),
        )
        .await;
    assert_eq!(status, 200, "a zero-use toolkit retires: {body}");
    assert_eq!(body["output"]["calls"], 0, "{body}");
    assert_eq!(body["output"]["bindings_disabled"], 1, "{body}");

    let (status, enabled): (String, Option<bool>) = {
        let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
            .await
            .expect("a scoped read");
        let row = sqlx::query_as(
            "SELECT k.status, b.enabled FROM toolkit k
               LEFT JOIN tool_binding b ON b.toolkit_id = k.id AND b.tenant_id = k.tenant_id
              WHERE k.tenant_id = $1 AND k.key = 'coding'",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("the toolkit row");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(status, "retired");
    assert_eq!(enabled, Some(false), "the binding is disabled");

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tools_retire_refuses_a_toolkit_used_in_the_window() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;
    assert_eq!(status, 200, "the call lands first: {body}");
    let decision = owner_decision(&fixture).await;

    let (status, body) = fixture
        .invoke_action(
            "tools.retire",
            json!({"toolkit_key": "coding", "decision_ref": decision}),
        )
        .await;
    assert_eq!(status, 409, "a used toolkit is not retired: {body}");
    assert!(body.to_string().contains("tool_in_use"), "{body}");

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_over_the_tool_timeout_is_pending_and_resumes() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new_with_delay(
        "coding",
        "jobs.get",
        "coding_jobs.get",
        "read",
        "read",
        coding_get_schema(),
        1_500,
    )
    .await;
    // The tool's own timeout is 1 s; the destination waits 1.5 s and its own action timeout
    // (5 s) admits that, so the tool timeout — not a destination failure — is what fires.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query("UPDATE tool SET timeout_secs = 1 WHERE tenant_id = $1")
        .bind(fixture.tenant.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("set the tool timeout");
    tx.commit().await.expect("commit");

    let (status, body) = fixture
        .call("coding.jobs.get", json!({"id": "job-1"}))
        .await;
    assert_eq!(
        status, 202,
        "a slow destination is pending, not failed: {body}"
    );
    assert_eq!(body["status"], "pending", "{body}");
    let token = body["resume_token"]
        .as_str()
        .expect("a resume token")
        .to_owned();

    let (ledger_status, ledger_token): (String, Option<String>) = {
        let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
            .await
            .expect("a scoped read");
        let row = sqlx::query_as(
            "SELECT status, resume_token FROM tool_call
              WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("the pending ledger row");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(ledger_status, "pending");
    assert_eq!(ledger_token.as_deref(), Some(token.as_str()));

    // The operator gives the operation room; the status tool resumes and completes it.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query("UPDATE tool SET timeout_secs = 5 WHERE tenant_id = $1")
        .bind(fixture.tenant.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("raise the tool timeout");
    tx.commit().await.expect("commit");

    let (status, body) = fixture
        .invoke_action(
            "tools.resume",
            json!({"resume_token": token, "input": {"id": "job-1"}}),
        )
        .await;
    assert_eq!(status, 200, "the resumed call completes: {body}");
    assert_eq!(
        body["output"]["path"], "/api/jobs/status",
        "the resumed call completes at the destination: {body}"
    );

    fixture.running.shutdown().await;
}

// ── P2.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_call_is_one_ledger_row() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let rows = fixture.ledger().await;
    assert_eq!(
        rows.len(),
        1,
        "exactly one ledger row for one call: {rows:?}"
    );
    let row = &rows[0];
    assert_eq!(row.tool_key, "coding.jobs.create");
    assert_eq!(row.toolkit_key, "coding");
    assert_eq!(row.toolkit_version, 1);
    assert_eq!(row.status, "ok");
    assert_eq!(row.http_status, Some(200));
    assert_eq!(
        row.input_hash.len(),
        64,
        "the input hash is a sha256: {}",
        row.input_hash
    );
    assert!(
        row.input_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "the input hash is hex: {}",
        row.input_hash
    );
    assert!(row.latency_ms >= 0);

    // The event is what per-tool metrics read (§11.9).
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped read");
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'tool.called'",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("count the tool events");
    tx.commit().await.expect("commit the read");
    assert_eq!(events, 1, "one tool.called event per call");

    fixture.running.shutdown().await;
}

// ── P3.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destructive_call_without_approvals_is_refused() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "fixtures",
        "echo.destructive",
        "fixtures.echo",
        "destructive",
        "destructive",
        echo_schema(),
    )
    .await;
    let before = fixture.stub.hits();

    let (status, body) = fixture
        .call("fixtures.echo.destructive", json!({"message": "delete it"}))
        .await;

    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "approval_required", "{body}");
    assert_eq!(
        fixture.stub.hits(),
        before,
        "a refused destructive tool must not reach the destination"
    );
    assert!(
        fixture.ledger().await.is_empty(),
        "a refusal before dispatch writes no tool_call row"
    );

    fixture.running.shutdown().await;
}

// ── P3.2 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binding_ceiling_cannot_be_raised_at_run_time() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "fixtures",
        "echo.write",
        "fixtures.echo",
        "write",
        "read",
        echo_schema(),
    )
    .await;
    let before = fixture.stub.hits();

    let (status, body) = fixture
        .call("fixtures.echo.write", json!({"message": "write it"}))
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "tool_effect_forbidden", "{body}");
    assert_eq!(
        fixture.stub.hits(),
        before,
        "the ceiling refuses before the destination"
    );

    // The ceiling is the binding's, not the call's: raising it in SQL lets the same call through.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    sqlx::query(
        "UPDATE tool_binding SET effect_ceiling = 'write', updated_at = now()
          WHERE tenant_id = $1 AND run_id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .execute(&mut *tx)
    .await
    .expect("raise the binding's ceiling");
    tx.commit().await.expect("commit the ceiling");

    let (status, body) = fixture
        .call("fixtures.echo.write", json!({"message": "write it"}))
        .await;
    assert_eq!(
        status, 200,
        "a widened binding admits the same call: {body}"
    );
    assert_eq!(fixture.stub.hits(), before + 1);

    fixture.running.shutdown().await;
}

// ── P4.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_schema_refuses_before_the_call() {
    let _serial = SERIAL.lock().await;
    let schema = json!({
        "type": "object",
        "properties": {
            "message": {"type": "string", "pattern": "^[a-z]+$", "minLength": 3},
            "count": {"type": "integer", "minimum": 1, "maximum": 10}
        },
        "required": ["message"],
        "additionalProperties": false
    });
    let fixture = Fixture::new(
        "fixtures",
        "echo.closed",
        "fixtures.echo",
        "read",
        "read",
        schema,
    )
    .await;
    let before = fixture.stub.hits();

    // An undeclared property, a pattern violation and a numeric bound, all at once.
    let (status, body) = fixture
        .call(
            "fixtures.echo.closed",
            json!({"message": "ABC", "count": 0, "extra": true}),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "tool_input_invalid", "{body}");
    let errors = body["errors"].as_array().expect("field errors");
    assert!(!errors.is_empty(), "{body}");

    // The numeric bound alone: a document the pattern accepts and the schema still refuses.
    let (status, body) = fixture
        .call(
            "fixtures.echo.closed",
            json!({"message": "abc", "count": 0}),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "tool_input_invalid", "{body}");

    assert_eq!(
        fixture.stub.hits(),
        before,
        "schema-invalid input must make zero network calls"
    );

    // The same call with a valid document reaches the destination — the contrast that proves the
    // refusal is the schema and not a blanket failure. `count` is optional, and the destination's
    // own schema does not accept it, so the valid document names only the field both schemas share.
    let (status, body) = fixture
        .call("fixtures.echo.closed", json!({"message": "abc"}))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(fixture.stub.hits(), before + 1);

    fixture.running.shutdown().await;
}

// ── P5.1 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_round_trips_a_tenant_prefixed_object() {
    let _serial = SERIAL.lock().await;
    // `Pg::fresh` puts the suite on the in-memory object store when no endpoint is configured.
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (project_id, run_id) = seed_run(&pg, seeded.tenant).await;
    let storage = JobStorage::from_env().expect("the test store is configured");

    let bytes = Bytes::from_static(b"the job's artefact, one tenant only");
    let stored = storage
        .put(
            seeded.tenant_id,
            Some(project_id),
            Some(run_id),
            "report.txt",
            bytes.clone(),
        )
        .await
        .expect("write the object");

    assert!(
        stored
            .key
            .starts_with(&format!("{TENANT_PREFIX}/{}/", seeded.tenant_id)),
        "the key must start tenant/<tenant_id>/: {}",
        stored.key
    );
    assert_eq!(stored.bytes, bytes.len());
    assert_eq!(stored.sha256, conveyor_actions::storage::sha256_hex(&bytes));

    let read_back = storage
        .get_verified(
            seeded.tenant_id,
            Some(project_id),
            Some(run_id),
            "report.txt",
            &stored.sha256,
        )
        .await
        .expect("read the object back");
    assert_eq!(read_back, bytes, "the read-back bytes match");

    let wrong = storage
        .get_verified(
            seeded.tenant_id,
            Some(project_id),
            Some(run_id),
            "report.txt",
            &"0".repeat(64),
        )
        .await;
    assert!(wrong.is_err(), "a hash mismatch is refused, not ignored");

    assert!(
        storage
            .head(
                seeded.tenant_id,
                Some(project_id),
                Some(run_id),
                "report.txt"
            )
            .await
            .expect("head the object"),
        "the object exists at the same key"
    );

    // The adapter builds every key itself; a caller cannot escape the prefix with a name.
    let escaped = storage.key(seeded.tenant_id, None, None, "../another-tenant/secret");
    assert!(escaped.is_err(), "a traversal name is refused");
}

// ── v1-w4a-3 — the tool version's idempotency policy (SPEC_V2 §11.3) ────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_required_key_is_refused_without_one() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    fixture.set_idempotency("required").await;

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;

    assert_eq!(status, 422, "a required key is not optional: {body}");
    assert_eq!(body["type"], "tool_input_invalid", "{body}");
    let errors = body["errors"].as_array().expect("field errors");
    assert!(
        errors
            .iter()
            .any(|error| error["field"] == "idempotency_key"),
        "the refusal names the header: {body}"
    );
    assert_eq!(
        fixture.stub.hits(),
        0,
        "a missing key must make zero destination calls"
    );

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_required_key_is_forwarded_and_replays() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    fixture.set_idempotency("required").await;
    let input = json!({"kind": "test", "input": {"files": []}});

    let (status, body) = fixture
        .call_with_key("coding.jobs.create", input.clone(), "k-1")
        .await;
    assert_eq!(status, 200, "a keyed call is admitted: {body}");
    assert_eq!(
        fixture.stub.hits(),
        1,
        "the first call reaches the destination"
    );

    let (status, body) = fixture
        .call_with_key("coding.jobs.create", input, "k-1")
        .await;
    assert_eq!(
        status, 200,
        "the same key replays the stored response: {body}"
    );
    assert_eq!(
        fixture.stub.hits(),
        1,
        "a replay must not make a second destination call"
    );

    fixture.running.shutdown().await;
}

// ── v1-w4a-4 — the binding's sealed credential (SPEC_V2 §11.3) ──────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_uses_the_bindings_sealed_credential() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    fixture.set_credential_alias(Some("svc.binding")).await;
    // Two sealed secrets: the binding names one, and the other is the tenant-wide value a
    // broken resolution would send instead.
    fixture.seal_secret("svc.default", "default-token").await;
    fixture.seal_secret("svc.binding", "binding-token").await;

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;

    assert_eq!(status, 200, "a credentialed binding is admitted: {body}");
    assert_eq!(
        body["output"]["authorization"], "Bearer binding-token",
        "the destination must receive the binding's sealed credential: {body}"
    );
    assert_ne!(
        body["output"]["authorization"], "Bearer default-token",
        "the tool's tenant-wide secret must not be sent instead: {body}"
    );
    assert_eq!(fixture.stub.hits(), 1, "exactly one destination call");

    fixture.running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binding_without_a_credential_cannot_call_a_credentialed_toolkit() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new(
        "coding",
        "jobs.create",
        "coding_jobs.create",
        "write",
        "write",
        coding_schema(),
    )
    .await;
    // The toolkit requires a credential; the binding (as seeded) carries no alias.
    fixture.set_auth_adapter("bearer").await;
    let before = fixture.stub.hits();

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;

    assert_eq!(
        status, 409,
        "a binding with no credential is not ready: {body}"
    );
    assert_eq!(body["type"], "tool_not_ready", "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("credential_alias"),
        "the refusal names the missing binding field: {body}"
    );
    assert_eq!(
        fixture.stub.hits(),
        before,
        "a refused binding must make zero destination calls"
    );
    assert!(
        fixture.ledger().await.is_empty(),
        "a refusal before dispatch writes no tool_call row"
    );

    fixture.running.shutdown().await;
}

// ── P5.2 ────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_compiled_prompt_carries_no_s3_key() {
    let _serial = SERIAL.lock().await;
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("CONVEYOR_S3_ACCESS_KEY", "AKIA-SENTINEL-NEVER-IN-A-PROMPT");
    support::set_env(
        "CONVEYOR_S3_SECRET_KEY",
        "secret-sentinel-never-in-a-prompt",
    );

    // `copilot_system@1` is the seeded recipe that compiles with no run facts; `agent@1` would
    // refuse (its process and capability components are required), which is a different test.
    let compiled =
        conveyor_actions::prompt::compile_for_audit(&pg.owner, seeded.tenant, "copilot_system@1")
            .await
            .expect("the seeded copilot prompt compiles");

    assert!(!compiled.text.is_empty(), "a prompt was compiled");
    for sentinel in [
        "AKIA-SENTINEL-NEVER-IN-A-PROMPT",
        "secret-sentinel-never-in-a-prompt",
        "CONVEYOR_S3_",
    ] {
        assert!(
            !compiled.text.contains(sentinel),
            "the compiled prompt carries {sentinel}"
        );
    }
}
