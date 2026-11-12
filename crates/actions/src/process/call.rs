#![forbid(unsafe_code)]
//! The `call` step kind — start a child run and poll it (CONTRACT §21, SPEC D22).
//!
//! A call step names `child_process_key`. Resolution is **process-first**: a published process
//! with that key starts a child process run through [`process_runs_start`] — the same start path
//! an external caller uses, so readiness, events, the run-tree edge and the depth cap are not
//! re-implemented here. Otherwise a published workflow with that key starts a child workflow run
//! through [`start_published_tx`] (the retained tool-sequence executor).
//!
//! # One commit, one child
//!
//! The start runs on the executor's own transaction and the child's id is written to the parent
//! `run_step.output` in that same transaction. A crash therefore leaves either no child (the
//! transaction rolled back) or a pollable id — never a started child nobody polls, and never a
//! second child on the retry. This is the crash window a "start, then record" pair would open.
//!
//! # Polling
//!
//! The child id on the row is the state; re-entry reads the child by id and never re-resolves the
//! key (a re-resolution could start a newer published version). A live child re-enqueues this
//! same attempt under the [`CALL_TAG`] dedupe slot with the judge's 10 s → 320 s backoff, using a
//! **separate `polls` counter** so child waiting never spends the judge's outage budget. A
//! succeeded child's last succeeded step output becomes the parent step's output and is judged
//! against the step's SOP; a failed or cancelled child goes through the step's ordinary
//! `on_result` chain.

use chrono::Utc;
use conveyor_auth::Role;
use conveyor_domain::{RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::process::{ProcessRecord, ProcessStepRecord, RunStepRecord};
use conveyor_store::run::RunContext;
use conveyor_store::workflow::WorkflowStoreError;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::process::runs::process_runs_start;
use crate::process::step::{
    Outcome, StepFailure, database, judge_and_record, park_backoff_seconds, record_and_chain,
};
use crate::process::views::ProcessRunStartIn;
use crate::workflow::runs::start_published_tx;

/// The dedupe tag of a child poll.
///
/// Distinct from the judge's tags on purpose: a call poll must not collapse onto a judge job and
/// must not increment the judge's `parks` budget.
const CALL_TAG: &str = "call";

/// Execute one `call` step attempt (the claimed row is `running`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    process: &ProcessRecord,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    let Some(key) = step.child_process_key.as_deref() else {
        return fail_step(
            pool,
            tenant,
            context,
            step,
            row,
            next_position,
            "process.call_target_missing: the step names no child_process_key".to_owned(),
        )
        .await;
    };
    match stored_child_id(row) {
        Some(child_id) => {
            poll_child(
                pool,
                tenant,
                app_key,
                context,
                process,
                step,
                row,
                child_id,
                next_position,
            )
            .await
        }
        None => {
            start_child(
                pool,
                tenant,
                app_key,
                context,
                process,
                step,
                row,
                key,
                next_position,
            )
            .await
        }
    }
}

/// The child run id the parent row already stores, if any.
fn stored_child_id(row: &RunStepRecord) -> Option<Uuid> {
    row.output
        .get("child_run_id")?
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
}

/// How many polls this attempt has parked, from the document the previous poll wrote.
fn stored_polls(judge: Option<&Value>) -> u32 {
    judge
        .and_then(|document| document.get("polls"))
        .and_then(Value::as_u64)
        .and_then(|polls| u32::try_from(polls).ok())
        .unwrap_or(0)
}

/// Why a child start did not produce a run.
enum StartFailure {
    /// Nothing was decided (a database error or a 5xx): the job retries.
    Retry(StepFailure),
    /// The start is refused for a reason the same job can never get past (an unknown target, a
    /// card-less parent, a depth cap): the step records the failure and `on_result` applies.
    Refused(String),
}

/// Start the child on the executor's transaction, then record its id and the first poll.
#[allow(clippy::too_many_arguments)]
async fn start_child(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    process: &ProcessRecord,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    key: &str,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    match start_in_tx(&mut tx, tenant, app_key, context, step, row, key).await {
        Ok(child_id) => {
            let output = json!({
                "child_run_id": child_id,
                "child_status": "queued",
                "polls": 1,
            });
            let document = json!({
                "decision": "waiting_child",
                "child_run_id": child_id,
                "child_status": "queued",
                "polls": 1,
            });
            store::process_judge::record_output_tx(&mut tx, tenant, row.id, &output, &document)
                .await
                .map_err(database)?;
            let run_after = Utc::now() + chrono::Duration::seconds(park_backoff_seconds(1));
            store::process_judge::enqueue_judge_park_tx(
                &mut tx,
                tenant,
                context.id,
                row.position,
                row.attempt,
                CALL_TAG,
                1,
                Some(run_after),
            )
            .await
            .map_err(database)?;
            tx.commit().await.map_err(database)?;
            Ok(())
        }
        Err(StartFailure::Retry(failure)) => {
            let _ = tx.rollback().await;
            Err(failure)
        }
        Err(StartFailure::Refused(message)) => {
            let _ = tx.rollback().await;
            // A duplicate delivery of this attempt can race the first: both claim the same
            // `run_step`, both see no child, and the loser's insert is refused by
            // `uq_run_child_live` / `uq_run_child_attempt`. That refusal is not this step's
            // failure — the winner's child is the step's child. Re-read the row and adopt it;
            // only a refusal with no child behind it is recorded as the step's failure.
            let mut check = store::begin_scoped(pool, tenant).await.map_err(database)?;
            let fresh = store::process::get_run_step_by_triple_tx(
                &mut check,
                tenant,
                row.run_id,
                row.position,
                row.attempt,
            )
            .await
            .map_err(database)?;
            check.commit().await.map_err(database)?;
            if let Some(fresh) = fresh
                && let Some(child_id) = stored_child_id(&fresh)
            {
                return poll_child(
                    pool,
                    tenant,
                    app_key,
                    context,
                    process,
                    step,
                    &fresh,
                    child_id,
                    next_position,
                )
                .await;
            }
            fail_step(pool, tenant, context, step, row, next_position, message).await
        }
    }
}

/// Resolve the key process-first, workflow-second, and start the child.
async fn start_in_tx(
    tx: &mut PgConnection,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    key: &str,
) -> Result<Uuid, StartFailure> {
    // Process-first: a published process the parent's card is allowed to run.
    let process = store::process::published_by_key_tx(&mut *tx, tenant, key)
        .await
        .map_err(|error| StartFailure::Retry(database(error)))?;
    if let Some(process) = process {
        let Some(card_id) = context.card_id else {
            return Err(StartFailure::Refused(format!(
                "process.call_project_scoped: the run has no card, so it cannot start the \
                 process {key:?}; only a workflow child can be project-scoped"
            )));
        };
        let ctx = service_ctx(
            tenant,
            app_key,
            "process_runs.start",
            process.owner_user_id.unwrap_or(Uuid::nil()),
        );
        let out = process_runs_start(
            &mut *tx,
            ctx,
            ProcessRunStartIn {
                card_id,
                process_id: None,
                process_key: Some(key.to_owned()),
                inputs: row.input.clone(),
                parent_run_id: Some(context.id),
                step_id: Some(step.id),
                // A process `call` step starts from a `process_step`; the workflow kind is for a
                // workflow parent's procedure node (0037).
                workflow_step_id: None,
            },
        )
        .await
        .map_err(refusal)?;
        return Uuid::parse_str(&out.value.id).map_err(|error| {
            StartFailure::Refused(format!(
                "process.call_child_unreadable: process_runs.start returned an unparseable run \
                 id ({error})"
            ))
        });
    }

    // Workflow-second: the retained tool-sequence executor.
    match store::workflow::workflow::get_published_by_key_tx(&mut *tx, tenant, key).await {
        Ok(workflow) => {
            let ctx = service_ctx(tenant, app_key, "workflow_runs.start", Uuid::nil());
            let (out, _etag) = start_published_tx(
                &mut *tx,
                &ctx,
                &workflow,
                context.project_id,
                context.card_id,
                row.input.clone(),
                context.capability_agent_id,
                Some((context.id, step.id)),
            )
            .await
            .map_err(refusal)?;
            Uuid::parse_str(&out.id).map_err(|error| {
                StartFailure::Refused(format!(
                    "process.call_child_unreadable: workflow_runs.start returned an unparseable \
                     run id ({error})"
                ))
            })
        }
        Err(WorkflowStoreError::NotFound) => Err(StartFailure::Refused(format!(
            "process.call_target_unknown: no published process or workflow has the key {key:?}"
        ))),
        Err(error) => Err(StartFailure::Retry(database(error))),
    }
}

/// The service principal a child start runs as — the same synthetic tenant admin the step
/// executor itself uses (CONTRACT §17).
fn service_ctx(
    tenant: TenantId,
    app_key: &str,
    action: &'static str,
    actor_id: Uuid,
) -> HandlerCtx {
    HandlerCtx {
        tenant,
        actor_id,
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        channel: None,
        request_id: None,
        version: None,
        if_match: None,
        action,
        app_key: app_key.to_owned(),
    }
}

/// A start refusal: a 5xx is a retry, every 4xx is the step's own failure.
fn refusal(error: ActionError) -> StartFailure {
    if error.status >= 500 {
        StartFailure::Retry(StepFailure::retryable(format!(
            "{}: {}",
            error.code, error.detail
        )))
    } else {
        StartFailure::Refused(format!("{}: {}", error.code, error.detail))
    }
}

/// Read the child by its stored id and either wait, finish or fail the step.
#[allow(clippy::too_many_arguments)]
async fn poll_child(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    process: &ProcessRecord,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    child_id: Uuid,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    let child = store::run::get_run_tx(&mut tx, tenant, child_id)
        .await
        .map_err(database)?;
    tx.commit().await.map_err(database)?;
    let Some(child) = child else {
        return fail_step(
            pool,
            tenant,
            context,
            step,
            row,
            next_position,
            format!("process.call_child_missing: the child run {child_id} no longer exists"),
        )
        .await;
    };

    match child.status {
        RunStatus::Queued | RunStatus::Running | RunStatus::WaitingApproval => {
            park(pool, tenant, row, child_id, child.status.as_str()).await
        }
        RunStatus::Succeeded => {
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
            let child_output = store::run::last_step_output_tx(&mut tx, tenant, child_id)
                .await
                .map_err(database)?;
            tx.commit().await.map_err(database)?;
            let output = json!({
                "child_run_id": child_id,
                "child_status": child.status.as_str(),
                "child_output": child_output,
            });
            judge_and_record(
                pool,
                tenant,
                app_key,
                context,
                process,
                step,
                row,
                output,
                next_position,
            )
            .await
        }
        RunStatus::Failed | RunStatus::Cancelled => {
            let detail = child
                .error_message
                .as_deref()
                .unwrap_or("no error recorded");
            fail_step(
                pool,
                tenant,
                context,
                step,
                row,
                next_position,
                format!(
                    "process.call_child_{}: the child run {child_id} ended {}: {detail}",
                    child.status.as_str(),
                    child.status.as_str()
                ),
            )
            .await
        }
    }
}

/// Store the running child on the row and re-enqueue this attempt after the backoff.
async fn park(
    pool: &PgPool,
    tenant: TenantId,
    row: &RunStepRecord,
    child_id: Uuid,
    child_status: &str,
) -> Result<(), StepFailure> {
    let polls = stored_polls(row.judge.as_ref()) + 1;
    let output = json!({
        "child_run_id": child_id,
        "child_status": child_status,
        "polls": polls,
    });
    let document = json!({
        "decision": "waiting_child",
        "child_run_id": child_id,
        "child_status": child_status,
        "polls": polls,
    });
    let run_after = Utc::now() + chrono::Duration::seconds(park_backoff_seconds(polls));
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    store::process_judge::record_output_tx(&mut tx, tenant, row.id, &output, &document)
        .await
        .map_err(database)?;
    store::process_judge::enqueue_judge_park_tx(
        &mut tx,
        tenant,
        row.run_id,
        row.position,
        row.attempt,
        CALL_TAG,
        polls,
        Some(run_after),
    )
    .await
    .map_err(database)?;
    tx.commit().await.map_err(database)?;
    Ok(())
}

/// Record the step as failed and let `on_result` decide (continue / retry / fail the run).
async fn fail_step(
    pool: &PgPool,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    next_position: Option<i32>,
    message: String,
) -> Result<(), StepFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    record_and_chain(
        &mut tx,
        tenant,
        context,
        step,
        row,
        Outcome::failed(message),
        next_position,
    )
    .await?;
    tx.commit().await.map_err(database)?;
    Ok(())
}
