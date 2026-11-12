//! S06c — the MCP tasks extension (`io.modelcontextprotocol/tasks`) over the conveyor's `/mcp`.
//!
//! The task IS a run: `projects.execute` creates the project's live card-less run, the task id
//! is that run's id, and `tasks/get|update|cancel` read and mutate the same rows the HTTP
//! surface does. This file proves the S06c §7 property table over the real transport:
//! the extension is advertised; a call starts a task; a pending approval surfaces as an
//! `inputRequest`; an answer decides it (idempotently); a blank-reason rejection stays open;
//! terminal runs map to `completed` / `cancelled`; and every ownership failure is one `-32602`
//! with no oracle (including a cross-tenant id).
//!
//! Claim boundary: the conveyor's side only. A client that does NOT declare the extension is
//! refused by rmcp itself (`-32021`), and a server restart mid-wait is a durability property
//! of Postgres, not of an in-memory manager — there is none here.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

/// The extension identifier (SEP-2663).
const TASKS_EXTENSION: &str = "io.modelcontextprotocol/tasks";

/// What a task-capable client declares at `initialize`.
fn declaring_capabilities() -> Value {
    json!({ "extensions": { TASKS_EXTENSION: {} } })
}

async fn task_get(mcp: &mut support::Mcp, task_id: &str) -> Value {
    let envelope = mcp.raw("tasks/get", json!({"taskId": task_id})).await;
    envelope
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("tasks/get {task_id} failed: {envelope}"))
}

async fn rpc_error(mcp: &mut support::Mcp, method: &str, params: Value) -> i64 {
    let envelope = mcp.raw(method, params).await;
    envelope
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("expected a JSON-RPC error, got {envelope}"))
}

async fn scoped_scalar(
    pg: &support::Pg,
    tenant: TenantId,
    sql: &'static str,
    id: &str,
) -> Option<String> {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .expect("scoped read")
}

async fn approval_status(pg: &support::Pg, tenant: TenantId, id: &str) -> String {
    scoped_scalar(
        pg,
        tenant,
        "SELECT status FROM approval WHERE id = $1::uuid",
        id,
    )
    .await
    .expect("the approval exists")
}

async fn request_approval(running: &support::Running, token: &str, project_id: &str) -> String {
    let (status, body) = running
        .invoke(
            "approvals.request",
            token,
            json!({
                "project_id": project_id,
                "gate": "human_approval",
                "kind": "human_approval",
                "description": "Ship it?",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    body["output"]["id"]
        .as_str()
        .expect("an approval id")
        .to_owned()
}

#[tokio::test]
async fn mcp_tasks_lifecycle_is_the_run_lifecycle() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let _other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    // `projects.create` builds the board from the catalogue.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;

    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "S06c tasks", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = project["output"]["id"]
        .as_str()
        .expect("a project id")
        .to_owned();

    // ── a. the extension is advertised ────────────────────────────────────────────────────
    let mut mcp =
        support::Mcp::connect_with_capabilities(&running, "dev:alice", declaring_capabilities())
            .await;
    assert!(
        mcp.initialize["result"]["capabilities"]["extensions"]
            .get(TASKS_EXTENSION)
            .is_some(),
        "initialize must advertise the tasks extension: {}",
        mcp.initialize
    );

    // ── b. start: the task id is the run id ───────────────────────────────────────────────
    let task = mcp
        .call("projects_execute", json!({"project_id": project_id}), None)
        .await;
    assert_eq!(task["resultType"], "task", "{task}");
    let task_id = task["taskId"].as_str().expect("a task id").to_owned();
    assert_eq!(task["status"], "working", "{task}");
    let (status, http_run) = running
        .invoke(
            "projects.execute",
            "dev:alice",
            json!({"project_id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{http_run}");
    assert_eq!(
        http_run["output"]["id"], task_id,
        "HTTP and the task start must name the same run"
    );

    // ── c. wait: a pending approval is an inputRequest keyed by its id ────────────────────
    let approval = request_approval(&running, "dev:alice", &project_id).await;
    let view = task_get(&mut mcp, &task_id).await;
    assert_eq!(view["status"], "input_required", "{view}");
    assert!(
        view["inputRequests"].get(&approval).is_some(),
        "the pending gate must be keyed by its approval id: {view}"
    );
    assert_eq!(
        view["_meta"]["ai.nonagentic/conveyor"]["run_id"], task_id,
        "{view}"
    );

    // ── d/f. answer, and the same answer again is a replay ────────────────────────────────
    let answer = json!({
        "taskId": task_id,
        "inputResponses": {
            approval.clone(): {"action": "accept", "content": {"decision": "approved", "reason": "looks right"}}
        }
    });
    let ack = mcp.raw("tasks/update", answer.clone()).await;
    assert!(ack.get("result").is_some(), "{ack}");
    assert_eq!(
        approval_status(&pg, seeded.tenant, &approval).await,
        "approved"
    );
    let replay = mcp.raw("tasks/update", answer).await;
    assert!(replay.get("result").is_some(), "{replay}");
    let decisions = scoped_scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*)::text FROM approval_decision WHERE approval_id = $1::uuid",
        &approval,
    )
    .await
    .expect("a decision count");
    assert_eq!(decisions, "1", "a repeated answer writes one decision");
    assert_eq!(task_get(&mut mcp, &task_id).await["status"], "working");

    // ── e/j. terminal mapping, and durability across a new session ───────────────────────
    // No worker runs in this test: start the run explicitly, as `run.execute` would.
    let (status, started) = running
        .invoke(
            "runs.start",
            "dev:alice",
            json!({"id": task_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{started}");
    let (status, finished) = running
        .invoke(
            "runs.finish",
            "dev:alice",
            json!({"id": task_id, "outcome": "succeeded"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{finished}");
    let terminal = task_get(&mut mcp, &task_id).await;
    assert_eq!(terminal["status"], "completed", "{terminal}");
    assert_eq!(terminal["result"]["isError"], false, "{terminal}");
    assert_eq!(
        terminal["result"]["structuredContent"]["run_id"], task_id,
        "{terminal}"
    );
    let mut second =
        support::Mcp::connect_with_capabilities(&running, "dev:alice", declaring_capabilities())
            .await;
    assert_eq!(
        task_get(&mut second, &task_id).await["status"],
        "completed",
        "a new session reads the same durable state"
    );

    // ── g/h. a blank-reason rejection stays open; cancel voids the project's gates ───────
    let cancelled_task = mcp
        .call("projects_execute", json!({"project_id": project_id}), None)
        .await["taskId"]
        .as_str()
        .expect("a second task id")
        .to_owned();
    assert_ne!(
        cancelled_task, task_id,
        "a finished run is not the live one"
    );
    let gate = request_approval(&running, "dev:alice", &project_id).await;
    let blank = mcp
        .raw(
            "tasks/update",
            json!({
                "taskId": cancelled_task,
                "inputResponses": {
                    gate.clone(): {"action": "accept", "content": {"decision": "rejected", "reason": "   "}}
                }
            }),
        )
        .await;
    assert!(blank.get("result").is_some(), "{blank}");
    assert_eq!(
        approval_status(&pg, seeded.tenant, &gate).await,
        "pending",
        "a rejection without a reason leaves the gate open"
    );
    let cancel = mcp
        .raw("tasks/cancel", json!({"taskId": cancelled_task}))
        .await;
    assert!(cancel.get("result").is_some(), "{cancel}");
    assert_eq!(
        task_get(&mut mcp, &cancelled_task).await["status"],
        "cancelled"
    );
    assert_eq!(
        approval_status(&pg, seeded.tenant, &gate).await,
        "voided",
        "tasks/cancel voids the project's pending gates"
    );

    // ── i. one code for every ownership failure — no oracle ──────────────────────────────
    assert_eq!(
        rpc_error(
            &mut mcp,
            "tasks/get",
            json!({"taskId": Uuid::now_v7().to_string()})
        )
        .await,
        -32602,
        "an unknown task id"
    );
    assert_eq!(
        rpc_error(&mut mcp, "tasks/get", json!({"taskId": "not-a-uuid"})).await,
        -32602,
        "a non-uuid id must not be a different code"
    );
    let (status, other_project) = running
        .invoke(
            "projects.create",
            "dev:bob",
            json!({"title": "S06c other tenant", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{other_project}");
    let other_project_id = other_project["output"]["id"]
        .as_str()
        .expect("a dev-b project id");
    let (status, other_run) = running
        .invoke(
            "projects.execute",
            "dev:bob",
            json!({"project_id": other_project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{other_run}");
    let other_task = other_run["output"]["id"].as_str().expect("a dev-b run id");
    assert_eq!(
        rpc_error(&mut mcp, "tasks/get", json!({"taskId": other_task})).await,
        -32602,
        "a cross-tenant task id is the same -32602"
    );

    // ── the extension gate: a client that did not declare it is refused by rmcp ──────────
    let mut plain = support::Mcp::connect(&running, "dev:alice").await;
    assert_eq!(
        rpc_error(&mut plain, "tasks/get", json!({"taskId": task_id})).await,
        -32021,
        "a non-declaring client gets MISSING_REQUIRED_CLIENT_CAPABILITY"
    );
}
