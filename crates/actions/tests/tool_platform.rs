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
/// returns the path, which is how a test tells the services apart.
struct Stub {
    base: String,
    hits: Arc<AtomicUsize>,
    answer: Arc<std::sync::Mutex<Value>>,
}

#[derive(Clone)]
struct StubState {
    hits: Arc<AtomicUsize>,
    answer: Arc<std::sync::Mutex<Value>>,
}

async fn answer_fn(
    axum::extract::State(state): axum::extract::State<StubState>,
    uri: axum::http::Uri,
) -> axum::Json<Value> {
    state.hits.fetch_add(1, Ordering::SeqCst);
    let mut body = state.answer.lock().expect("the stub answer").clone();
    if let Some(map) = body.as_object_mut() {
        // The path is how a test tells the services apart.
        map.insert("path".to_owned(), json!(uri.path()));
    }
    axum::Json(body)
}

impl Stub {
    async fn start() -> Stub {
        Stub::start_with(json!({"stub": true})).await
    }

    async fn start_with(answer: Value) -> Stub {
        let hits = Arc::new(AtomicUsize::new(0));
        let shared = Arc::new(std::sync::Mutex::new(answer));
        let app = axum::Router::new()
            .fallback(axum::routing::any(answer_fn))
            .with_state(StubState {
                hits: hits.clone(),
                answer: shared.clone(),
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
            answer: shared,
        }
    }

    /// Answer every later call with `body` (the path is still filled in).
    fn answer_with(&self, body: Value) {
        *self.answer.lock().expect("the stub answer") = body;
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
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let stub = Stub::start().await;
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
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, owner, status)
         VALUES ($1, $2, 1, $3, $4, 'platform', 'published') RETURNING id",
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

// ── #797 P3 (v1-w4a-2): a 2xx is judged by the call's own read-back ─────────────────────

/// A `200` whose own verification says the write did not happen is not `ok` (SPEC_V2 §11.3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_two_hundred_with_a_failing_verdict_is_check_failed() {
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
    fixture.stub.answer_with(json!({
        "verdict": "fail",
        "checks": [{"ok": false, "detail": "the write is not visible on read-back"}]
    }));

    let (status, body) = fixture
        .call(
            "coding.jobs.create",
            json!({"kind": "test", "input": {"files": []}}),
        )
        .await;
    assert_eq!(status, 422, "a failed read-back is not ok: {body}");
    assert_eq!(
        body["type"], "write_not_verified",
        "the refusal names the verification: {body}"
    );

    let rows = fixture.ledger().await;
    assert_eq!(rows.len(), 1, "one call is one ledger row: {rows:?}");
    assert_eq!(
        rows[0].status, "check_failed",
        "the ledger records the failed check, not ok: {rows:?}"
    );
    assert_eq!(
        rows[0].error_class.as_deref(),
        Some("write_not_verified"),
        "the ledger names why: {rows:?}"
    );
    assert!(
        rows[0]
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("verdict")),
        "the ledger carries the destination's own verdict: {rows:?}"
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
