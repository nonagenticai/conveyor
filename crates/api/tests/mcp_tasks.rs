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

/// The one decision a gate carries: `(decision, note, decided_by_kind, via)`.
async fn decision_row(
    pg: &support::Pg,
    tenant: TenantId,
    approval: &str,
) -> Option<(String, String, String, Value)> {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    sqlx::query_as(
        "SELECT decision, note, decided_by_kind, via
           FROM approval_decision WHERE approval_id = $1::uuid",
    )
    .bind(approval)
    .fetch_optional(&mut *tx)
    .await
    .expect("the decision row")
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
    let approval = request_approval(&running, support::REVIEWER, &project_id).await;
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
    let gate = request_approval(&running, support::REVIEWER, &project_id).await;
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

/// WP17a — a free-text approval answered over MCP under a published delegation.
///
/// A `requires_answer` gate is an elicitation whose schema demands `answer`; accepting it
/// with the answer IS the approval, and the answer becomes the decision's note. The agent
/// platform's `_meta.delegation_ref` makes the engine record the answer as a `rule` decision
/// under the published owner document. A malformed ref is ignored (the caller's own kind
/// stands, never invented), and a repeated answer is the existing replay/409 sink, not a
/// second decision.
#[tokio::test]
async fn mcp_tasks_answers_a_question_under_a_delegation() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;

    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "WP17a answerable question", "project_type_code": "delivery"}),
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

    let mut mcp =
        support::Mcp::connect_with_capabilities(&running, "dev:alice", declaring_capabilities())
            .await;
    let task = mcp
        .call("projects_execute", json!({"project_id": project_id}), None)
        .await;
    let task_id = task["taskId"].as_str().expect("a task id").to_owned();

    // The owner's published decision the answer is delegated under.
    let delegation = support::delegation_ref(&running, "dev:alice").await;

    // A run-scoped free-text question, requested by the distinct reviewer.
    let (status, gate) = running
        .invoke(
            "approvals.request",
            support::REVIEWER,
            json!({
                "run_id": task_id,
                "project_id": project_id,
                "gate": "agent_question",
                "kind": "question",
                "description": "Which release channel should the agent use?",
                "requires_answer": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{gate}");
    let approval = gate["output"]["id"]
        .as_str()
        .expect("a question gate id")
        .to_owned();

    // ── a. a requires_answer gate is an elicitation that demands the answer ──────────────
    let view = task_get(&mut mcp, &task_id).await;
    assert_eq!(view["status"], "input_required", "{view}");
    let request = view["inputRequests"]
        .get(&approval)
        .unwrap_or_else(|| panic!("the question must be an inputRequest: {view}"));
    assert_eq!(
        request["method"], "elicitation/create",
        "a question is an elicitation: {request}"
    );
    let params = &request["params"];
    let message = params["message"].as_str().expect("a message");
    assert!(
        message.contains("Which release channel"),
        "the message carries the question: {message}"
    );
    assert!(
        message.contains("an answer is required"),
        "the message says an answer is required: {message}"
    );
    let conveyor = &params["_meta"]["ai.nonagentic/conveyor"];
    assert_eq!(conveyor["requires_answer"], true, "{params}");
    assert_eq!(
        conveyor["description"], "Which release channel should the agent use?",
        "{params}"
    );
    let schema = &params["requestedSchema"];
    assert_eq!(
        schema["properties"]["answer"]["type"], "string",
        "the schema carries the answer field: {params}"
    );
    assert!(
        schema["required"]
            .as_array()
            .is_some_and(|required| required.contains(&json!("answer"))),
        "the answer is required: {params}"
    );

    // An empty answer is the writer's 422, surfaced with the reason; the gate stays open.
    let refused = mcp
        .raw(
            "tasks/update",
            json!({
                "taskId": task_id,
                "inputResponses": {
                    approval.clone(): {"action": "accept", "content": {"answer": "   "}}
                }
            }),
        )
        .await;
    let error = refused
        .get("error")
        .unwrap_or_else(|| panic!("an empty answer must be refused: {refused}"));
    assert_eq!(error["code"], -32602, "{refused}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("requires an answer"),
        "the writer's refusal reaches the client: {refused}"
    );
    assert_eq!(
        approval_status(&pg, seeded.tenant, &approval).await,
        "pending",
        "a refused answer leaves the gate open"
    );

    // ── b/c. the answer + _meta.delegation_ref is one rule decision; a repeat replays ────
    let answer = json!({
        "taskId": task_id,
        "inputResponses": {
            approval.clone(): {
                "action": "accept",
                "content": {"answer": "Use the canary channel"}
            }
        },
        "_meta": {"delegation_ref": delegation.clone()},
    });
    let ack = mcp.raw("tasks/update", answer.clone()).await;
    assert!(ack.get("result").is_some(), "{ack}");
    assert_eq!(
        approval_status(&pg, seeded.tenant, &approval).await,
        "approved"
    );
    let (decision, note, kind, via) = decision_row(&pg, seeded.tenant, &approval)
        .await
        .expect("the delegated decision");
    assert_eq!(decision, "approve", "an answer is the approval");
    assert_eq!(note, "Use the canary channel", "the answer is the note");
    assert_eq!(
        kind, "rule",
        "a delegated answer is a rule, never the caller"
    );
    assert_eq!(via["delegation_ref"], json!(delegation), "{via}");

    let replay = mcp.raw("tasks/update", answer).await;
    assert!(
        replay.get("result").is_some(),
        "a second update walks the existing 409/replay sink: {replay}"
    );
    let count = scoped_scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*)::text FROM approval_decision WHERE approval_id = $1::uuid",
        &approval,
    )
    .await
    .expect("a decision count");
    assert_eq!(count, "1", "a repeated answer writes one decision");

    // ── d. an answer-only update works on a gate that does not require one ───────────────
    let plain = request_approval(&running, support::REVIEWER, &project_id).await;
    let ack = mcp
        .raw(
            "tasks/update",
            json!({
                "taskId": task_id,
                "inputResponses": {
                    plain.clone(): {"action": "accept", "content": {"answer": "Ship it"}}
                },
                // A malformed ref is ignored: the caller's own kind stands, never invented.
                "_meta": {"delegation_ref": "not-a-uuid"},
            }),
        )
        .await;
    assert!(ack.get("result").is_some(), "{ack}");
    assert_eq!(
        approval_status(&pg, seeded.tenant, &plain).await,
        "approved"
    );
    let (decision, note, kind, _) = decision_row(&pg, seeded.tenant, &plain)
        .await
        .expect("the plain decision");
    assert_eq!(decision, "approve");
    assert_eq!(note, "Ship it", "answer-only is the note on a plain gate");
    assert_eq!(
        kind, "human",
        "a malformed delegation is ignored, never invented"
    );
}
