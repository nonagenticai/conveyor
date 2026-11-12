#![forbid(unsafe_code)]
//! S06c — the MCP tasks extension (`io.modelcontextprotocol/tasks`, SEP-2663) over the run.
//!
//! A task IS a run: `projects.execute` creates (or returns) the project's live card-less run,
//! and the task id is that run's id. The three hooks read and mutate the SAME run/approval rows
//! the HTTP surface does — there is no second store, no task table, and no in-memory manager.
//!
//! **One code for every ownership failure.** A non-uuid, an unknown id, a cross-tenant id, an
//! expired task and a card-scoped run all answer the same `-32602 unknown task` — never an
//! oracle (S06c trap 7). Tenancy is the scoped read: RLS plus the `tenant_id` column.
//!
//! **Terminal precedence.** The run is checked first; a stale pending approval must not revive
//! a finished task (trap 1). `failed` is never a task status on the wire: a failed run is
//! `completed` with `isError: true` (trap 2).
//!
//! **Decisions are made through the engine**, not by writing `approval_decision` here:
//! `tasks/update` invokes `approvals.decide` with the authenticated principal, so the gate's
//! state machine, events, run resume and the `(approval_id, approver_id)` unique index all
//! apply unchanged. A repeated answer is a replay, not an error (S06c §7 f).

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use conveyor_auth::Session;
use conveyor_store as store;
use rmcp::model::{
    ElicitRequest, ElicitRequestParams, ElicitationSchema, EnumSchema, ErrorData as McpError,
    GetTaskResult, InputRequest, MetaObject, PrimitiveSchemaDefinition, Task, TaskPayload,
    TaskStatus,
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::invoke::{Authority, InvokeRequest, invoke};
use crate::registry::registry;

/// Seven days from creation (S06c §5). The TTL is recomputed on every read: an older task is
/// simply not found, so no sweeper is needed and no row is deleted.
const TASK_TTL: Duration = Duration::days(7);
/// 7 days in milliseconds — the wire value.
const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// The suggested poll interval.
const POLL_INTERVAL_MS: u64 = 5_000;
/// How many pending gates one task surfaces. The approvals list is index-served by
/// `approval_project_idx`; a project with more open gates than this is over-loaded by design.
const MAX_INPUT_REQUESTS: i64 = 50;

/// The one refusal for every "not your task" case.
fn not_found(task_id: &str) -> McpError {
    McpError::invalid_params(format!("unknown task: {task_id}"), None)
}

fn internal(what: &str) -> McpError {
    McpError::internal_error(format!("tasks: {what}"), None)
}

fn parse_task_id(task_id: &str) -> Result<Uuid, McpError> {
    Uuid::parse_str(task_id).map_err(|_| not_found(task_id))
}

/// The project id out of a `projects.execute` output document.
fn output_run_id(body: &Value) -> Option<Uuid> {
    body.get("output")?
        .get("id")?
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
}

fn output_created_at(body: &Value) -> DateTime<Utc> {
    body.get("output")
        .and_then(|output| output.get("created_at"))
        .and_then(|value| serde_json::from_value::<DateTime<Utc>>(value.clone()).ok())
        .unwrap_or_else(Utc::now)
}

/// Build the `resultType:"task"` seed from a successful `projects.execute` invocation.
///
/// It does not touch the database: the action's own output already carries the run id and its
/// creation instant, and the task is a projection of that row.
pub fn start_result(body: &Value) -> Result<rmcp::model::CreateTaskResult, McpError> {
    let Some(run_id) = output_run_id(body) else {
        return Err(internal("projects.execute returned no run id"));
    };
    let created_at = output_created_at(body).to_rfc3339();
    let task = Task::new(
        run_id.to_string(),
        TaskStatus::Working,
        created_at.clone(),
        created_at,
    )
    .with_ttl_ms(TASK_TTL_MS)
    .with_poll_interval_ms(POLL_INTERVAL_MS);
    Ok(rmcp::model::CreateTaskResult::new(task))
}

/// The run and its project's pending gates, or the one not-found refusal.
async fn load(
    pool: &PgPool,
    session: &Session,
    task_id: &str,
) -> Result<(store::run::RunTaskRow, Vec<store::approval::ApprovalRecord>), McpError> {
    let run_id = parse_task_id(task_id)?;
    let tenant = session.tenant;
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|_| internal("database"))?;
    let Some(row) = store::run::get_run_task_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|_| internal("read run"))?
    else {
        return Err(not_found(task_id));
    };
    // A card-scoped run is not a project task: the id must not reveal that it exists.
    if row.run.card_id.is_some() {
        return Err(not_found(task_id));
    }
    if row.created_at + TASK_TTL <= Utc::now() {
        return Err(not_found(task_id));
    }
    let approvals = store::approval::list_approvals_tx(
        &mut tx,
        tenant,
        Some("pending"),
        None,
        None,
        Some(row.run.project_id),
        None,
        None,
        MAX_INPUT_REQUESTS,
    )
    .await
    .map_err(|_| internal("read approvals"))?;
    Ok((row, approvals))
}

/// One pending gate as an elicitation request: the human picks approved/rejected and may
/// explain why. Keyed by the approval id on the wire.
fn input_request(record: &store::approval::ApprovalRecord) -> Result<InputRequest, McpError> {
    let approval = &record.approval;
    let schema = ElicitationSchema::builder()
        .property(
            "decision",
            PrimitiveSchemaDefinition::Enum(
                EnumSchema::builder(vec!["approved".to_owned(), "rejected".to_owned()]).build(),
            ),
        )
        .optional_string("reason")
        .optional_string("comment")
        .build()
        .map_err(|_| internal("elicitation schema"))?;
    Ok(InputRequest::Elicitation(ElicitRequest::new(
        ElicitRequestParams::FormElicitationParams {
            meta: None,
            message: format!(
                "Approval required: {} ({}) — approve or reject, and give a reason when rejecting",
                approval.gate, approval.kind
            ),
            requested_schema: schema,
        },
    )))
}

/// The tool-shaped terminal result: `{content, structuredContent, isError}` (S06c §5).
fn completed_result(row: &store::run::RunTaskRow, is_error: bool) -> Map<String, Value> {
    let run = &row.run;
    let text = run
        .error_message
        .clone()
        .unwrap_or_else(|| format!("run {} {}", run.id, run.status.as_str()));
    let mut structured = Map::new();
    structured.insert("run_id".to_owned(), json!(run.id));
    structured.insert("project_id".to_owned(), json!(run.project_id));
    structured.insert("status".to_owned(), json!(run.status.as_str()));
    structured.insert("result".to_owned(), json!({}));
    structured.insert("error".to_owned(), json!(run.error_message));
    let mut result = Map::new();
    result.insert(
        "content".to_owned(),
        json!([{ "type": "text", "text": text }]),
    );
    result.insert("structuredContent".to_owned(), Value::Object(structured));
    result.insert("isError".to_owned(), json!(is_error));
    result
}

/// `_meta["ai.nonagentic/conveyor"]`: the ids and the last events on the run's topic.
async fn task_meta(
    tx: &mut sqlx::PgConnection,
    row: &store::run::RunTaskRow,
) -> Result<MetaObject, McpError> {
    let topic = format!("run:{}", row.run.id);
    let events = store::actions::events_list(tx, Some(std::slice::from_ref(&topic)), None, 5)
        .await
        .map_err(|_| internal("read events"))?;
    let events: Vec<Value> = events
        .iter()
        .map(|event| {
            json!({
                "seq": event.seq,
                "type": event.event_type,
                "payload": event.payload,
                "created_at": event.created_at,
            })
        })
        .collect();
    let mut conveyor = Map::new();
    conveyor.insert("project_id".to_owned(), json!(row.run.project_id));
    conveyor.insert("run_id".to_owned(), json!(row.run.id));
    conveyor.insert("status".to_owned(), json!(row.run.status.as_str()));
    conveyor.insert("events".to_owned(), Value::Array(events));
    let mut meta = Map::new();
    meta.insert("ai.nonagentic/conveyor".to_owned(), Value::Object(conveyor));
    Ok(MetaObject(meta))
}

/// `tasks/get`.
pub async fn get(
    pool: &PgPool,
    session: &Session,
    task_id: &str,
) -> Result<GetTaskResult, McpError> {
    let (row, approvals) = load(pool, session, task_id).await?;
    let payload = if matches!(row.run.status, conveyor_domain::RunStatus::Succeeded) {
        TaskPayload::Completed {
            result: completed_result(&row, false),
        }
    } else if matches!(row.run.status, conveyor_domain::RunStatus::Failed) {
        TaskPayload::Completed {
            result: completed_result(&row, true),
        }
    } else if matches!(row.run.status, conveyor_domain::RunStatus::Cancelled) {
        TaskPayload::Cancelled
    } else if !approvals.is_empty() {
        let mut requests: BTreeMap<String, InputRequest> = BTreeMap::new();
        for record in &approvals {
            requests.insert(record.approval.id.to_string(), input_request(record)?);
        }
        TaskPayload::InputRequired {
            input_requests: requests,
        }
    } else {
        TaskPayload::Working
    };
    let status_message = match &payload {
        TaskPayload::InputRequired { input_requests } => Some(format!(
            "{} approval(s) awaiting a decision",
            input_requests.len()
        )),
        TaskPayload::Completed { .. } => row
            .run
            .error_message
            .clone()
            .or_else(|| Some(row.run.status.as_str().to_owned())),
        TaskPayload::Cancelled => Some("cancelled".to_owned()),
        _ => Some(row.run.status.as_str().to_owned()),
    };
    let mut task = Task::new(
        row.run.id.to_string(),
        TaskStatus::Working,
        row.created_at.to_rfc3339(),
        row.updated_at.to_rfc3339(),
    )
    .with_ttl_ms(TASK_TTL_MS)
    .with_poll_interval_ms(POLL_INTERVAL_MS);
    if let Some(message) = status_message {
        task = task.with_status_message(message);
    }
    let mut tx = store::begin_scoped(pool, session.tenant)
        .await
        .map_err(|_| internal("database"))?;
    let meta = task_meta(&mut tx, &row).await?;
    let mut result = GetTaskResult::new(rmcp::model::DetailedTask::new(task, payload));
    result.meta = Some(meta);
    Ok(result)
}

/// The decision an input response carries: `Some("approve"|"reject")`, or `None` when the
/// client declined/cancelled or sent an unknown shape (lenient — S06c trap 9).
fn response_decision(value: &Value) -> Option<&'static str> {
    let action = value.get("action").and_then(Value::as_str)?;
    if action != "accept" {
        return None;
    }
    match value
        .get("content")
        .and_then(|content| content.get("decision"))
        .and_then(Value::as_str)?
    {
        "approved" => Some("approve"),
        "rejected" => Some("reject"),
        _ => None,
    }
}

fn response_reason(value: &Value) -> String {
    value
        .get("content")
        .and_then(|content| content.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// `tasks/update` — answer the outstanding input requests.
///
/// Every key is an approval id; an unknown key is ignored. A rejection without a non-blank
/// reason leaves the gate open (S06c §7 g). The decision itself is the ordinary
/// `approvals.decide` invocation, so a repeat is a replay.
pub async fn update(
    pool: &PgPool,
    session: &Session,
    task_id: &str,
    responses: &BTreeMap<String, Value>,
    _meta: Option<rmcp::model::RequestMetaObject>,
) -> Result<(), McpError> {
    let (row, approvals) = load(pool, session, task_id).await?;
    if matches!(
        row.run.status,
        conveyor_domain::RunStatus::Succeeded
            | conveyor_domain::RunStatus::Failed
            | conveyor_domain::RunStatus::Cancelled
    ) {
        // A terminal replay acks: the client may retry after the task finished.
        return Ok(());
    }
    let tenant = session.tenant;
    for (key, value) in responses {
        let Ok(approval_id) = Uuid::parse_str(key) else {
            continue;
        };
        if !approvals
            .iter()
            .any(|record| record.approval.id == approval_id)
        {
            continue;
        }
        let Some(decision) = response_decision(value) else {
            continue;
        };
        let reason = response_reason(value);
        if decision == "reject" && reason.is_empty() {
            // The gate stays open; the next `tasks/get` still shows it as input_required.
            continue;
        }
        let approver = session.principal.user_id.as_uuid();
        {
            let mut tx = store::begin_scoped(pool, tenant)
                .await
                .map_err(|_| internal("database"))?;
            let already = store::approval::decision_by_tx(&mut tx, tenant, approval_id, approver)
                .await
                .map_err(|_| internal("read decision"))?;
            if already {
                continue;
            }
        }
        let request = InvokeRequest {
            action: "approvals.decide".to_owned(),
            input: json!({
                "approval_id": approval_id,
                "decision": decision,
                "note": reason,
            }),
            precondition: None,
            idempotency_key: Some(format!("mcp-task:{task_id}:{approval_id}")),
            actor_id: approver,
            actor_kind: session.principal.kind.as_actor_kind().to_owned(),
            role: session.role,
            policy: session.policy.clone(),
            authority: Authority::of(session),
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: String::new(),
            usage: None,
        };
        match invoke(pool, registry(), tenant, request).await {
            Ok(outcome) if (200..300).contains(&outcome.status) => {}
            Ok(outcome) if outcome.status == 409 => {}
            Ok(outcome) => {
                let detail = outcome
                    .body
                    .get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or("the decision was refused");
                return Err(McpError::invalid_params(detail.to_owned(), None));
            }
            Err(error) if error.status == 409 => {}
            Err(error) => {
                return Err(McpError::invalid_params(error.detail, None));
            }
        }
    }
    Ok(())
}

/// `tasks/cancel` — cancel the run and void the project's remaining pending gates.
pub async fn cancel(pool: &PgPool, session: &Session, task_id: &str) -> Result<(), McpError> {
    let (row, _) = load(pool, session, task_id).await?;
    let tenant = session.tenant;
    if !matches!(
        row.run.status,
        conveyor_domain::RunStatus::Queued
            | conveyor_domain::RunStatus::Running
            | conveyor_domain::RunStatus::WaitingApproval
    ) {
        return Ok(());
    }
    let request = InvokeRequest {
        action: "runs.cancel".to_owned(),
        input: json!({
            "id": row.run.id,
            "reason": "cancelled by the MCP task client",
        }),
        precondition: None,
        idempotency_key: None,
        actor_id: session.principal.user_id.as_uuid(),
        actor_kind: session.principal.kind.as_actor_kind().to_owned(),
        role: session.role,
        policy: session.policy.clone(),
        authority: Authority::of(session),
        request_id: None,
        trace_id: None,
        depth: 0,
        app_key: String::new(),
        usage: None,
    };
    match invoke(pool, registry(), tenant, request).await {
        Ok(outcome) if (200..300).contains(&outcome.status) => {}
        Ok(outcome) if outcome.status == 409 => {}
        Ok(_) => return Err(McpError::invalid_params("cancel was refused", None)),
        Err(error) if error.status == 409 => {}
        Err(error) => return Err(McpError::invalid_params(error.detail, None)),
    }
    // `runs.cancel` voids the run's own gates; the project may hold card-level gates opened by
    // other runs, and a cancelled task must not leave them pending (S06c §7 h).
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|_| internal("database"))?;
    store::approval::void_project_pending_tx(&mut tx, tenant, row.run.project_id)
        .await
        .map_err(|_| internal("void approvals"))?;
    tx.commit().await.map_err(|_| internal("commit"))?;
    Ok(())
}
