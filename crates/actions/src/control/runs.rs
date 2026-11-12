#![forbid(unsafe_code)]
//! Run actions: `runs.create`, `.start`, `.finish`, `.cancel`, `.list` (S06 §5.2).
//!
//! Creation is refused `409 andon-engaged` (S06 §5.5) and deduped by `uq_run_live`; the
//! queued row, its `run.execute` job and its `run.created` event commit in one transaction,
//! so a committed run is always driving and a rolled-back one leaves no job behind
//! (CONTRACT §7). Start/finish/cancel are status-guarded transitions through
//! `store::run::transition_run_tx` (CONTRACT §3): a run whose state moved underneath the
//! caller is a `409` carrying the current status, never a precondition failure. Finish
//! records the failure class through `store::run::fail_run_tx`; list pages on
//! `(created_at, id)` with the decoded cursor from `store::actions::decode_time_cursor`.

use conveyor_domain::RunStatus;
use conveyor_store as store;
use conveyor_store::run::{NewRun, RunRecord};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{RawEvent, append_raw_event};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::views::{
    IdIn, RunCancelIn, RunCreateIn, RunFinishIn, RunGetIn, RunGetOut, RunListIn, RunOut,
    RunProcessRef, RunStepOut, RunTreeChildOut, RunTreeOut,
};
use super::{invalid, parse_run_status, run_error, store_error};

/// One run as every run action returns it.
pub(crate) fn run_out(record: &RunRecord) -> RunOut {
    let run = &record.run;
    RunOut {
        id: run.id.to_string(),
        card_id: run.card_id.map(|id| id.to_string()),
        project_id: run.project_id.to_string(),
        status: run.status.as_str().to_owned(),
        attempt: run.attempt,
        error_class: run.error_class.clone(),
        error_message: run.error_message.clone(),
        started_at: run.started_at,
        finished_at: run.finished_at,
        version: i64::try_from(run.version.get()).unwrap_or(i64::MAX),
        process_step_id: record.process_step_id.map(|id| id.to_string()),
        workflow_step_id: record.workflow_step_id.map(|id| id.to_string()),
        created_at: record.created_at,
    }
}

/// The `run:<id>` aggregate topic (CONTRACT §7).
pub(crate) fn run_topic(run_id: Uuid) -> String {
    format!("run:{run_id}")
}

/// Append one run event, in the caller's transaction.
///
/// The aggregate is `run`, so the topic is `run:<id>` and a `run:{id}` subscriber is a
/// topic match, not a scan. `board_id` is `None`: run events are not board-scoped, so only a
/// tenant-wide subscription can select them.
pub(crate) async fn emit_run_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    aggregate_id: Uuid,
    event_type: &'static str,
    payload: serde_json::Value,
    data: serde_json::Value,
) -> Result<(), ActionError> {
    append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: run_topic(aggregate_id),
            board_id: None,
            aggregate_type: "run",
            aggregate_id,
            event_type,
            payload,
            data,
            prev_data: serde_json::Value::Null,
        },
    )
    .await
}

/// Re-read the run as its full representation (the row has no `created_at` on [`Run`]).
pub(crate) async fn read_record(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    run_id: Uuid,
    action: &str,
) -> Result<RunRecord, ActionError> {
    store::run::get_run_record_tx(&mut *tx, ctx.tenant, run_id)
        .await
        .map_err(|error| run_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("run"))
}

/// Withdraw every pending gate of `run_id` and announce each one.
///
/// Shared by `runs.cancel` and `workflow_runs.cancel`: a pending gate on a cancelled run will never
/// be answered, and leaving it pending keeps `uq_appr_pending` occupied and the expiry sweep busy
/// (S06 open question 4).
pub(crate) async fn void_run_approvals(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    run_id: Uuid,
) -> Result<(), ActionError> {
    let voided = store::approval::void_pending_for_run_tx(conn, ctx.tenant, run_id)
        .await
        .map_err(|error| store_error(error, ctx.action))?;
    for approval_id in voided {
        append_raw_event(
            conn,
            ctx,
            &RawEvent {
                topic: format!("approval:{approval_id}"),
                board_id: None,
                aggregate_type: "approval",
                aggregate_id: approval_id,
                event_type: "approval.voided",
                payload: serde_json::json!({
                    "approval_id": approval_id,
                    "run_id": run_id,
                }),
                data: serde_json::Value::Null,
                prev_data: serde_json::Value::Null,
            },
        )
        .await?;
    }
    Ok(())
}

/// Create a queued run for a card and enqueue its driving job.
///
/// Order matters: the card resolves in-tenant first (a foreign id is a `404`, never an FK
/// acceptance — CONTRACT §2), its board supplies `project_id`, and only then is the andon
/// consulted, because the refusal is about *this* tenant's work existing at all. The attempt
/// continues the card's run history, and `uq_run_live` makes a second concurrent create a
/// `409` rather than a second live run.
#[crate::action(
    name = "runs.create",
    version = 1,
    kind = "internal",
    scopes = ["runs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn runs_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunCreateIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let action = "runs.create";
    let card = store::card::get_card_tx(&mut *tx, ctx.tenant, input.card_id)
        .await
        .map_err(|error| store_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.board_id)
        .await
        .map_err(|error| store_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    let andon = store::andon::get_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read andon state");
            ActionError::internal()
        })?;
    if andon.is_some_and(|state| state.engaged) {
        return Err(ActionError::new(
            409,
            "andon-engaged",
            "the tenant's andon cord is pulled; no run may be created while it is engaged",
        ));
    }

    let attempt = store::run::next_attempt_for_card_tx(&mut *tx, ctx.tenant, card.id)
        .await
        .map_err(|error| store_error(error, action))?;

    let run = store::run::insert_run_tx(
        &mut *tx,
        ctx.tenant,
        &NewRun {
            card_id: Some(card.id),
            project_id: board.project_id,
            attempt,
            correlation_id: input.correlation_id,
            process_id: None,
            inputs: serde_json::Value::Object(serde_json::Map::new()),
            capability_id: None,
            capability_agent_id: None,
            // `runs.create` is a ROOT: a plain run has no parent, no step and pins no process.
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
            depth: 0,
            process_version: 1,
        },
    )
    .await
    .map_err(|error| run_error(error, action))?;

    store::run::enqueue_run_execute_tx(&mut *tx, ctx.tenant, run.id)
        .await
        .map_err(|error| store_error(error, action))?;

    let record = read_record(tx, &ctx, run.id, action).await?;
    let out = run_out(&record);
    emit_run_event(
        &mut *tx,
        &ctx,
        run.id,
        "run.created",
        serde_json::json!({
            "run_id": run.id,
            "card_id": run.card_id,
            "attempt": run.attempt,
        }),
        serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    Ok(HandlerOutput::created(out).with_etag(run.version.etag()))
}

/// Move a queued run to `running`.
///
/// Only `queued → running` is this action's edge. A run in `waiting_approval` resumes
/// through the approval, not through `runs.start`, so the status is read first and anything
/// else is a `409` naming the status that is actually stored.
#[crate::action(
    name = "runs.start",
    version = 1,
    kind = "internal",
    scopes = ["runs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn runs_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let action = "runs.start";
    let current = store::run::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| run_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    if current.status != RunStatus::Queued {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "run {} is {}; only a queued run can start",
                current.id, current.status
            ),
        ));
    }
    let started = store::run::transition_run_tx(&mut *tx, ctx.tenant, input.id, RunStatus::Running)
        .await
        .map_err(|error| run_error(error, action))?;

    let record = read_record(tx, &ctx, started.id, action).await?;
    let out = run_out(&record);
    emit_run_event(
        &mut *tx,
        &ctx,
        started.id,
        "run.started",
        serde_json::json!({ "run_id": started.id, "attempt": started.attempt }),
        serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    Ok(HandlerOutput::ok(out).with_etag(started.version.etag()))
}

/// Finish a running run as `succeeded` or `failed`.
///
/// The success arm is the domain's `running → succeeded` edge; the failure arm writes through
/// [`store::run::fail_run_tx`], which also records `error_class`/`error_message` in the same
/// status-guarded statement. A run that is not `running` is a `409`: a queued run has not
/// begun, and a terminal one cannot be revised.
#[crate::action(
    name = "runs.finish",
    version = 1,
    kind = "internal",
    scopes = ["runs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn runs_finish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunFinishIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let action = "runs.finish";
    let outcome = input.outcome.trim();
    if outcome != "succeeded" && outcome != "failed" {
        return Err(invalid(
            "outcome",
            "invalid_value",
            "must be succeeded or failed",
        ));
    }

    let current = store::run::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| run_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    if current.status != RunStatus::Running {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "run {} is {}; only a running run can finish",
                current.id, current.status
            ),
        ));
    }
    // A process run is driven by the step executor, which owns its steps and its children. A
    // caller finishing it by hand would leave queued steps and live child runs behind — the
    // "second writer" the executor cannot see (audit 2026-11-10). `runs.cancel` is the verb for
    // stopping one: it skips the steps and cascades to the tree.
    if store::run::process_of_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| run_error(error, action))?
        .is_some()
    {
        return Err(ActionError::new(
            409,
            "run.process_owned",
            format!(
                "run {} is executing a process; cancel it with runs.cancel instead of finishing it",
                current.id
            ),
        ));
    }

    let (finished, out) = if outcome == "succeeded" {
        let run =
            store::run::transition_run_tx(&mut *tx, ctx.tenant, input.id, RunStatus::Succeeded)
                .await
                .map_err(|error| run_error(error, action))?;
        let record = read_record(tx, &ctx, run.id, action).await?;
        let out = run_out(&record);
        emit_run_event(
            &mut *tx,
            &ctx,
            run.id,
            "run.succeeded",
            serde_json::json!({ "run_id": run.id, "outcome": "succeeded" }),
            serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
        )
        .await?;
        (run, out)
    } else {
        let run = store::run::fail_run_tx(
            &mut *tx,
            ctx.tenant,
            input.id,
            input.error_class.as_deref(),
            input.error_message.as_deref(),
        )
        .await
        .map_err(|error| run_error(error, action))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "conflict",
                format!("run {} is no longer running", input.id),
            )
        })?;
        let record = read_record(tx, &ctx, run.id, action).await?;
        let out = run_out(&record);
        emit_run_event(
            &mut *tx,
            &ctx,
            run.id,
            "run.failed",
            serde_json::json!({
                "run_id": run.id,
                "error_class": run.error_class.clone().unwrap_or_else(|| "failed".to_owned()),
                "attempt": run.attempt,
            }),
            serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
        )
        .await?;
        // The old `err.card` error trigger: a failed run starts the card's recovery flow.
        store::run::enqueue_run_failed_trigger_tx(&mut *tx, ctx.tenant, run.id)
            .await
            .map_err(|error| crate::control::store_error(error, action))?;
        (run, out)
    };

    Ok(HandlerOutput::ok(out).with_etag(finished.version.etag()))
}

/// Cancel a live run and void the gates it abandons.
///
/// The gate writes happen in this transaction (S06 open question 4): a pending gate on a
/// cancelled run can never be answered, and leaving it pending would hold `uq_appr_pending`
/// and keep `approval.expire` busy. One `approval.voided` per voided id, then the run's own
/// `run.cancelled`, all on their aggregate topics (CONTRACT §7).
#[crate::action(
    name = "runs.cancel",
    version = 1,
    kind = "internal",
    scopes = ["runs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn runs_cancel(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunCancelIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let action = "runs.cancel";
    let current = store::run::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| run_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    if !matches!(
        current.status,
        RunStatus::Queued | RunStatus::Running | RunStatus::WaitingApproval
    ) {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "run {} is {}; only a live run can be cancelled",
                current.id, current.status
            ),
        ));
    }

    let cancelled =
        store::run::transition_run_tx(&mut *tx, ctx.tenant, input.id, RunStatus::Cancelled)
            .await
            .map_err(|error| run_error(error, action))?;

    void_run_approvals(&mut *tx, &ctx, cancelled.id).await?;

    // S11: a cancelled process run must not leave its remaining steps `queued` forever. This is
    // a no-op for a run with no `run_step` rows; each skipped row is announced on the run's own
    // topic exactly as the executor's skip path does.
    let skipped = store::process::skip_queued_run_steps_tx(&mut *tx, ctx.tenant, cancelled.id)
        .await
        .map_err(|error| crate::process::process_error(error, action))?;
    for row in skipped {
        emit_run_event(
            &mut *tx,
            &ctx,
            cancelled.id,
            "run_step.changed",
            serde_json::json!({
                "run_id": cancelled.id,
                "position": row.position,
                "attempt": row.attempt,
                "status": "skipped",
            }),
            serde_json::Value::Null,
        )
        .await?;
    }

    // The run tree is one unit of work (CONTRACT §21): a `call` child keeps its own driving job,
    // so without this sweep it would run to completion under a parent nobody is waiting for and
    // hold `uq_run_child_live` until it finished. Each moved child gets its own `run.cancelled`
    // on its own topic, exactly as if it had been cancelled directly.
    let children = store::run::cancel_descendants_tx(&mut *tx, ctx.tenant, cancelled.id)
        .await
        .map_err(|error| run_error(error, action))?;
    for child in &children {
        // A cancelled child must not leave `queued` rows behind either: its own steps are as
        // unreachable as the parent's. The child's `run.cancelled` event is the story; the
        // skipped rows are its trace.
        store::process::skip_queued_run_steps_tx(&mut *tx, ctx.tenant, child.id)
            .await
            .map_err(|error| run_error(store::RunStoreError::Process(error), action))?;
        emit_run_event(
            &mut *tx,
            &ctx,
            child.id,
            "run.cancelled",
            serde_json::json!({ "run_id": child.id, "reason": "parent_cancelled" }),
            serde_json::Value::Null,
        )
        .await?;
    }

    let record = read_record(tx, &ctx, cancelled.id, action).await?;
    let out = run_out(&record);
    emit_run_event(
        &mut *tx,
        &ctx,
        cancelled.id,
        "run.cancelled",
        serde_json::json!({ "run_id": cancelled.id, "reason": input.reason }),
        serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    Ok(HandlerOutput::ok(out).with_etag(cancelled.version.etag()))
}

/// One page of runs, newest first.
#[crate::action(
    name = "runs.list",
    version = 1,
    kind = "internal",
    scopes = ["runs:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn runs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunListIn,
) -> Result<crate::views::Page<RunOut>, ActionError> {
    let status = match input.status.as_deref() {
        None => None,
        Some(raw) => Some(parse_run_status(raw)?.as_str()),
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(store::actions::decode_time_cursor(raw).ok_or_else(|| {
            invalid(
                "cursor",
                "invalid_value",
                "not a cursor this service issued",
            )
        })?),
    };
    let mut rows = store::run::list_runs_tx(
        &mut *tx,
        ctx.tenant,
        status,
        input.card_id,
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| run_error(error, "runs.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|record| store::actions::encode_time_cursor(record.created_at, record.run.id))
    } else {
        None
    };
    Ok(crate::views::Page {
        items: rows.iter().map(run_out).collect(),
        next_cursor,
    })
}

/// One run with its trace, gates, card, process and inputs.
///
/// The queue's projection (`runs.list`) is deliberately thin; a detail screen needs the
/// `run_step` trace and the gates, and three round trips to compose them would let the parts
/// disagree. Everything here is a tenant-scoped read: a run of another tenant is a 404 before
/// any child row is fetched.
#[crate::action(
    name = "runs.get",
    version = 1,
    kind = "internal",
    scopes = ["runs:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn runs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunGetIn,
) -> Result<HandlerOutput<RunGetOut>, ActionError> {
    const ACTION: &str = "runs.get";
    let record = read_record(tx, &ctx, input.id, ACTION).await?;
    let detail = store::run::get_run_detail_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| run_error(error, ACTION))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    let steps = store::process::list_run_steps_detail_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| crate::process::process_error(error, ACTION))?;
    let approvals = store::approval::list_approvals_tx(
        &mut *tx,
        ctx.tenant,
        None,
        Some(input.id),
        None,
        None,
        None,
        None,
        store::actions::MAX_LIMIT,
    )
    .await
    .map_err(|error| crate::control::approval_error(error, ACTION))?;

    let process = match detail.process_id {
        Some(id) => store::process::get_process_tx(&mut *tx, ctx.tenant, id)
            .await
            .map_err(|error| crate::process::process_error(error, ACTION))?
            .map(|record| RunProcessRef {
                id: record.id.to_string(),
                title: record.title,
                version: record.version,
            }),
        None => None,
    };

    let mut decisions =
        crate::control::approvals::decisions_by_approval(&mut *tx, ctx.tenant, &approvals).await?;

    Ok(HandlerOutput::ok(RunGetOut {
        run: run_out(&record),
        steps: steps
            .into_iter()
            .map(|step| RunStepOut {
                id: step.id.to_string(),
                process_step_id: step.process_step_id.map(|id| id.to_string()),
                position: step.position,
                attempt: step.attempt,
                action: step.action,
                status: step.status,
                input: step.input,
                output: step.output,
                error: step.error,
                judge: step.judge,
                started_at: step.started_at,
                finished_at: step.finished_at,
            })
            .collect(),
        approvals: approvals
            .iter()
            .map(|row| {
                let mut out = crate::control::approvals::approval_out(row);
                out.decisions = decisions.remove(&row.approval.id).unwrap_or_default();
                out
            })
            .collect(),
        process,
        tree: RunTreeOut {
            parent_run_id: detail.parent_run_id.map(|id| id.to_string()),
            step_id: detail.step_id.map(|id| id.to_string()),
            depth: detail.depth,
            children: detail
                .children
                .into_iter()
                .map(|child| RunTreeChildOut {
                    id: child.id.to_string(),
                    status: child.status,
                    step_id: child.step_id.map(|id| id.to_string()),
                    depth: child.depth,
                })
                .collect(),
        },
        inputs: detail.inputs,
    }))
}
