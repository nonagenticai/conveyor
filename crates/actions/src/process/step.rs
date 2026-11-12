#![forbid(unsafe_code)]
//! S11 §6 step 8 — the `process.step` executor.
//!
//! One job drives one step attempt. The job is enqueued by `process_runs.start` for position 1
//! and, after every record, by this executor for the next position (or the next attempt) **in the
//! same transaction as the record** — so a committed `run_step` always has the job that continues
//! from it, and a crash replays exactly one step.
//!
//! # The crash window, and why the input is persisted at claim
//!
//! The action invocation commits in its own transaction (`invoke`), because it cannot join ours:
//! a remote call must not hold a database transaction open, and the engine already owns
//! validation, idempotency and bookkeeping. So the sequence is:
//!
//! 1. **tx1** — read the run, resolve the template, claim the step (`queued → running`) and write
//!    the resolved input on the same row;
//! 2. **no transaction** — invoke the step's action with `Idempotency-Key = v5(run:position:attempt)`;
//! 3. **tx2** — record the result, emit `run_step.changed`, enqueue the next position/attempt, or
//!    close the run.
//!
//! A crash after step 1 leaves the row `running`; the reclaimed job's claim returns nothing and the
//! executor re-invokes with the **stored** input, which is the same input the previous attempt
//! presented (same idempotency hash). Re-resolving would be wrong: `card.*` and earlier outputs are
//! live values, and a changed card can produce a different document — a different hash, or a second
//! effect with different data. A crash after step 3 is a no-op: the row is terminal, and the record
//! and the chain committed together.
//!
//! # Gates (S11 step 9)
//!
//! A step that names a `gate_kind` is authorised BEFORE it may claim: the executor opens an
//! `approval` row (`kind = 'process'`, `gate = gate_kind`, `meta = {position, attempt,
//! process_step_id}`) and returns without claiming or invoking anything. The run is
//! `waiting_approval`; the step stays `queued`, which is what makes a re-delivered job a no-op
//! (the run-status guard above) and a duplicate job unable to bypass the gate.
//!
//! [`resolve_gate`] is the only path that wakes a gated step: `approved` enqueues the same
//! `(position, attempt)` under the same idempotency key; `rejected`/`expired` applies the step's
//! `on_result` through the ordinary record-and-chain, so `fail`/`continue`/`retry` mean exactly what
//! they mean everywhere else. The approval decision is an S06 gate; its quorum comes from the
//! catalogue's `human_approval.meta.dual_control`.

use chrono::{DateTime, Utc};
use conveyor_auth::Role;
use conveyor_domain::template::{StepOutcome, TemplateCtx, resolve};
use conveyor_domain::{CatalogueKind, Code, RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::process::{ProcessRecord, ProcessStepRecord, RunStepRecord};
use conveyor_store::run::{RunContext, RunStoreError};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::invoke::{Authority, InvokeRequest, invoke};
use crate::process::judge;
use crate::registry::registry;

/// The executor's failure, mapped to a job failure by the worker.
///
/// `retryable` means "nothing was decided": a database hiccup or a 5xx from the invoked action.
/// The claim stays `running`, the job retries on its lease, and the replay path re-invokes under
/// the same idempotency key. A permanent failure is data this job names and cannot act on (a run
/// or step row that does not exist); it dead-letters immediately.
#[derive(Debug)]
pub struct StepFailure {
    /// What went wrong, for the job's `last_error`.
    pub message: String,
    /// Whether the job should retry.
    pub retryable: bool,
}

impl StepFailure {
    /// A transient failure: the job retries.
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }

    /// A failure the same job can never get past.
    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

/// How a claim attempt ended.
enum Claimed {
    /// This call won the `queued → running` race and owns the step.
    Fresh(RunStepRecord),
    /// A previous attempt claimed the step and never recorded: re-invoke with the stored input.
    Running(RunStepRecord),
    /// The row is terminal; the record and the chain already committed.
    Done,
}

/// One step that fails before it can invoke (a gate or an unresolved template).
struct PreparedFailure {
    /// The exact text recorded on the step.
    message: String,
}

/// Execute one step attempt of one process run.
pub async fn execute_step(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    run_id: Uuid,
    position: i32,
    attempt: i32,
) -> Result<(), StepFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;

    // ── the run and its frozen version ─────────────────────────────────────────────────────
    let Some(context) = store::run::get_run_context_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
    else {
        return abandon(
            tx,
            StepFailure::permanent(format!(
                "run {run_id} does not exist in this tenant; the step job names nothing"
            )),
        )
        .await;
    };
    if context.status != RunStatus::Running {
        // Cancelled, failed, waiting on a gate or already finished: the run wins. A step job must
        // never revive it, and a terminal run is a replay the chain already accounted for.
        tx.commit()
            .await
            .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    let Some(process_id) = context.process_id else {
        return abandon(
            tx,
            StepFailure::permanent(format!(
                "run {run_id} is not process-driven; a process.step job cannot advance it"
            )),
        )
        .await;
    };
    let Some(process) = store::process::get_process_tx(&mut tx, tenant, process_id)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
    else {
        return abandon(
            tx,
            StepFailure::permanent(format!(
                "the process version {process_id} of run {run_id} no longer exists"
            )),
        )
        .await;
    };
    let steps = store::process::list_steps_tx(&mut tx, tenant, process_id)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
    let Some(step) = steps.iter().find(|step| step.position == position).cloned() else {
        return abandon(
            tx,
            StepFailure::permanent(format!(
                "the frozen process has no step at position {position}"
            )),
        )
        .await;
    };
    let next_position = steps
        .iter()
        .filter(|candidate| candidate.position > position)
        .map(|candidate| candidate.position)
        .min();

    // ── the replay read, before any resolution ─────────────────────────────────────────────
    //
    // This must come first. A replayed terminal step must not fail because the card, the list or an
    // earlier output moved on since it ran; and a replayed running step must use its stored input.
    if let Some(row) =
        store::process::get_run_step_by_triple_tx(&mut tx, tenant, run_id, position, attempt)
            .await
            .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
    {
        match row.status.as_str() {
            "running" => {
                tx.commit()
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                return invoke_and_record(
                    pool,
                    tenant,
                    app_key,
                    &context,
                    &process,
                    &step,
                    &row,
                    next_position,
                )
                .await;
            }
            "succeeded" | "failed" | "skipped" => {
                tx.commit()
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                return Ok(());
            }
            _ => {}
        }
    }

    // ── a gated step parks on its approval before it is claimed ───────────────────────────
    //
    // The check is BEFORE the claim on purpose: a claimed step is invokable, so a gate opened
    // after the claim — or a duplicate job that took another path — would let the step run
    // ungated. A parked step stays `queued`; the run's `waiting_approval` status makes a
    // re-delivered job a no-op before it ever reaches this branch.
    if let Some(gate_kind) = step.gate_kind.as_deref() {
        match ensure_gate_open(
            &mut tx,
            tenant,
            &context,
            &step,
            gate_kind,
            position,
            attempt,
            process.owner_user_id,
        )
        .await
        {
            Ok(true) => {
                // Parked: opened here, or a concurrent worker won the insert. Nothing is
                // claimed and nothing is invoked.
                tx.commit()
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                return Ok(());
            }
            // An approved gate for this exact attempt: the approval IS the step's work. A `gate`
            // step names no action (the publish validator refuses one), so falling through to the
            // action path would invoke `""` and fail a step the approver just unblocked. Claim it,
            // record the decision as its output, and chain — no judge, because the human decision
            // is the inspection.
            Ok(false) => {
                let next = claim_then(
                    &mut tx,
                    tenant,
                    &context,
                    &step,
                    attempt,
                    None,
                    None,
                    next_position,
                )
                .await?;
                if let Next::Invoke(claimed) = next {
                    record_and_chain(
                        &mut tx,
                        tenant,
                        &context,
                        &step,
                        &claimed,
                        Outcome::succeeded(json!({
                            "gate": gate_kind,
                            "decision": "approved",
                        })),
                        next_position,
                    )
                    .await?;
                }
                tx.commit()
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                return Ok(());
            }
            Err(failure) if failure.retryable => return abandon(tx, failure).await,
            Err(failure) => {
                // The catalogue no longer defines the gate this step was published with. Fail
                // closed — claim, record the refusal, never invoke — rather than run ungated.
                let next = claim_then(
                    &mut tx,
                    tenant,
                    &context,
                    &step,
                    attempt,
                    None,
                    Some(PreparedFailure {
                        message: failure.message,
                    }),
                    next_position,
                )
                .await?;
                tx.commit()
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                return continue_after(
                    pool,
                    tenant,
                    app_key,
                    &context,
                    &process,
                    &step,
                    next,
                    next_position,
                )
                .await;
            }
        }
    }

    // ── resolve the template against card / run / earlier step outputs ─────────────────────
    let resolution: Result<Value, String> =
        match build_template_ctx(&mut tx, tenant, &context, &process, position).await {
            Ok(ctx) => resolve(&step.input_template, &ctx).map_err(|error| error.to_string()),
            Err(message) => Err(message),
        };
    let input = match resolution {
        Ok(input) => input,
        Err(message) => {
            let next = claim_then(
                &mut tx,
                tenant,
                &context,
                &step,
                attempt,
                None,
                Some(PreparedFailure {
                    message: format!("process.template_unresolved: {message}"),
                }),
                next_position,
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
            return continue_after(
                pool,
                tenant,
                app_key,
                &context,
                &process,
                &step,
                next,
                next_position,
            )
            .await;
        }
    };

    // A judge reject reruns this position and hands its reason to the new attempt, so the rerun
    // can see what the last output was refused for (SPEC D28).
    let carried = carry_judge_reason(&mut tx, tenant, &context, position, attempt, input).await;
    let input = match carried {
        Ok(input) => input,
        Err(failure) => return abandon(tx, failure).await,
    };

    let next = claim_then(
        &mut tx,
        tenant,
        &context,
        &step,
        attempt,
        Some(&input),
        None,
        next_position,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
    continue_after(
        pool,
        tenant,
        app_key,
        &context,
        &process,
        &step,
        next,
        next_position,
    )
    .await
}

/// Commit-and-return a failure without writing anything else.
async fn abandon(tx: Transaction<'_, Postgres>, failure: StepFailure) -> Result<(), StepFailure> {
    tx.commit()
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
    Err(failure)
}

/// What the caller must do after [`claim_then`] has written tx1.
// The claimed row is deliberately carried by value: it is consumed once, immediately, and boxing
// it would add an indirection to the executor's hottest path for no live benefit.
#[allow(clippy::large_enum_variant)]
enum Next {
    /// Commit tx1, then invoke the action with this claimed row.
    Invoke(RunStepRecord),
    /// Commit tx1 and stop.
    Done,
}

/// Commit tx1, then invoke if the claim requires it.
#[allow(clippy::too_many_arguments)]
async fn continue_after(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    process: &ProcessRecord,
    step: &ProcessStepRecord,
    next: Next,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    match next {
        Next::Invoke(row) => {
            invoke_and_record(
                pool,
                tenant,
                app_key,
                context,
                process,
                step,
                &row,
                next_position,
            )
            .await
        }
        Next::Done => Ok(()),
    }
}

/// Claim the step, then either finish it in tx1 (a failure that never invokes) or report that the
/// caller must commit and invoke.
///
/// `input` is `None` for a step that failed before resolution (gate/template); the row is then
/// claimed with `{}` so the failure is still a first-class, claim-guarded transition rather than a
/// write to a row another worker owns.
#[allow(clippy::too_many_arguments)]
async fn claim_then(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    attempt: i32,
    input: Option<&Value>,
    prepared: Option<PreparedFailure>,
    next_position: Option<i32>,
) -> Result<Next, StepFailure> {
    let empty = Value::Object(serde_json::Map::new());
    let claimed_input = input.unwrap_or(&empty);
    let claimed = claim_or_replay(
        tx,
        tenant,
        context.id,
        step.position,
        attempt,
        claimed_input,
    )
    .await?;
    match claimed {
        Claimed::Fresh(row) => {
            emit_step_changed(tx, tenant, context, row.position, row.attempt, "running").await?;
            match prepared {
                Some(failure) => {
                    record_and_chain(
                        tx,
                        tenant,
                        context,
                        step,
                        &row,
                        Outcome::failed(failure.message),
                        next_position,
                    )
                    .await?;
                    Ok(Next::Done)
                }
                None => Ok(Next::Invoke(row)),
            }
        }
        // A concurrent attempt claimed the step between our read and our claim. Nothing was
        // resolved for it here, so the stored input is the only honest one — a prepared failure is
        // discarded for the same reason: the row's input may be fine.
        Claimed::Running(row) => Ok(Next::Invoke(row)),
        Claimed::Done => Ok(Next::Done),
    }
}

async fn claim_or_replay(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
    input: &Value,
) -> Result<Claimed, StepFailure> {
    match store::process::claim_run_step_tx(tx, tenant, run_id, position, attempt, input)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
    {
        Some(row) => Ok(Claimed::Fresh(row)),
        None => {
            match store::process::get_run_step_by_triple_tx(tx, tenant, run_id, position, attempt)
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
            {
                None => Err(StepFailure::permanent(format!(
                    "run {run_id} has no run_step at position {position} attempt {attempt}"
                ))),
                Some(row) if row.status == "running" => Ok(Claimed::Running(row)),
                Some(_) => Ok(Claimed::Done),
            }
        }
    }
}

/// Build the template context for `position`.
async fn build_template_ctx(
    tx: &mut Transaction<'_, Postgres>,
    tenant: TenantId,
    context: &RunContext,
    process: &ProcessRecord,
    position: i32,
) -> Result<TemplateCtx, String> {
    let Some(card_id) = context.card_id else {
        return Err("the run has no card, so `card.*` cannot resolve".to_owned());
    };
    let card = store::card::get_card_record_tx(tx, tenant, card_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("card {card_id} does not exist in this tenant"))?;
    let list = store::list::get_list_tx(tx, tenant, card.card.list_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("list {} does not exist in this tenant", card.card.list_id))?;

    // One entry per earlier position, in order: `steps.<n>.output` is `steps[n-1]`, and the value
    // is the LATEST attempt's output — a retried position's earlier attempts are history, not the
    // step's result.
    let rows = store::process::list_run_steps_tx(tx, tenant, context.id)
        .await
        .map_err(|error| error.to_string())?;
    let mut earlier = Vec::with_capacity((position - 1).max(0) as usize);
    for index in 1..position {
        let latest = rows
            .iter()
            .filter(|row| row.position == index)
            .max_by_key(|row| row.attempt);
        earlier.push(StepOutcome {
            output: latest.map(|row| row.output.clone()).unwrap_or(Value::Null),
        });
    }

    // A computed move names its destination lane by KIND, never a uuid: the catalogue is content
    // and cannot know an id. `cards.move` takes a `list_id`, so the board's lanes are exposed here.
    let lists = store::list::list_records_tx(tx, tenant, card.card.board_id)
        .await
        .map_err(|error| error.to_string())?;
    let mut lanes = serde_json::Map::with_capacity(lists.len());
    for record in &lists {
        lanes.insert(record.list.kind.as_str().to_owned(), json!(record.list.id));
    }

    Ok(TemplateCtx {
        run: json!({
            "id": context.id,
            "card_id": context.card_id,
            "project_id": context.project_id,
            "correlation_id": context.correlation_id,
            "inputs": context.inputs,
        }),
        steps: earlier,
        card: json!({
            "id": card.card.id,
            "name": card.name,
            "description": card.description,
            "list_id": card.card.list_id,
            "list_kind": list.kind.as_str(),
            "card_type_code": card.card_type_code,
            "assignee_id": card.card.assignee_id,
            "version": card.card.version.get(),
            "meta": card.meta,
        }),
        process: json!({ "key": process.key, "version": process.version }),
        board: json!({ "lists": lanes }),
        position: position.max(0) as usize,
    })
}

/// Invoke the step's action outside any transaction, then record the outcome in a new one.
#[allow(clippy::too_many_arguments)]
async fn invoke_and_record(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    process: &ProcessRecord,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    // ── a stored output awaiting its judge is judged, never re-invoked (D12) ────────────────
    //
    // A parked attempt (an eval still grading, or a judge outage) left its output on the row and
    // a job to come back to it. Re-entry must read that output: calling the action again would be
    // a second effect for a value we already own.
    if let Some(output) = awaiting_judge(row) {
        return judge_and_record(
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
        .await;
    }

    // ── a `call` step starts a child run and polls it (CONTRACT §21) ─────────────────────────
    //
    // Checked AFTER the judge read above: a child that succeeded may itself be waiting on a
    // judge, and that park must be settled against the output it stored, never re-polled as a
    // child. A live child's own document uses `decision = "waiting_child"`, so `awaiting_judge`
    // leaves it to this branch.
    if step.kind == "call" {
        return crate::process::call::execute(
            pool,
            tenant,
            app_key,
            context,
            process,
            step,
            row,
            next_position,
        )
        .await;
    }

    // ── `agent.run` is a step primitive, not a normal action ────────────────────────────────
    //
    // Its handler refuses by construction: an internal action holds the invocation transaction and
    // no pool, so it cannot invoke the model or the tools. CONTRACT §17 makes the **step executor**
    // the resolver, and S11 is a step executor exactly as S13's `workflow.node` is — so drive the
    // same loop through the same seam, under this step attempt's idempotency key.
    //
    // Dispatch is on the KIND, not on the stored action string: publish normalises an agent step's
    // action to the primitive (`AGENT_STEP_ACTION`), but the kind is what the step *is*, and a
    // dispatch keyed on the spelling is how an API-authored agent step came to invoke `""`.
    if step.kind == "agent" {
        // The step's own executor wins over the run-level one: a process may bind different
        // agents to different steps, and `process_agent_for_process_tx` answers `None` for a
        // process that names more than one (which publish accepts).
        let bound_agent = step.process_agent_id.or(context.capability_agent_id);
        let idempotency_key =
            store::process::process_step_idempotency_key(row.run_id, row.position, row.attempt);
        let outcome = match crate::workflow::agent::run_step(
            pool,
            tenant,
            row.run_id,
            Some(row.id),
            bound_agent,
            app_key,
            Some(idempotency_key.as_str()),
            1,
            &row.input,
        )
        .await
        {
            Ok(out) => Outcome::succeeded(serde_json::to_value(out).unwrap_or(Value::Null)),
            Err(error) if error.status >= 500 => {
                // Nothing was decided: leave the step `running` and let the job retry under the
                // same key, exactly as a normal action's 5xx does.
                return Err(StepFailure::retryable(format!(
                    "{}: {}",
                    error.code, error.detail
                )));
            }
            // A typed refusal (a cap, an unresolved prompt, a bad config) is a step failure, so
            // the step's `on_result` policy applies rather than the run dying on a 500.
            Err(error) => Outcome::failed(format!("{}: {}", error.code, error.detail)),
        };
        if outcome.status == "succeeded" {
            return judge_and_record(
                pool,
                tenant,
                app_key,
                context,
                process,
                step,
                row,
                outcome.output,
                next_position,
            )
            .await;
        }
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
        record_and_chain(&mut tx, tenant, context, step, row, outcome, next_position).await?;
        tx.commit()
            .await
            .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }

    let request = InvokeRequest {
        action: row.action.clone(),
        input: row.input.clone(),
        // The precondition is the input's reserved `_version` member; `invoke` strips it itself
        // (CONTRACT §6), so the executor never touches it.
        precondition: None,
        idempotency_key: Some(store::process::process_step_idempotency_key(
            row.run_id,
            row.position,
            row.attempt,
        )),
        actor_id: process.owner_user_id.unwrap_or(Uuid::nil()),
        // The executor is the engine, not the caller: `service` is what makes the audit row say so.
        // Its role is the synthetic tenant-admin (`kata_executor`, George 2026-11-09), which holds
        // every tenant scope an admin does — the publisher's ability to invoke each step is
        // checked at publish time (see `processes.publish`), so the executor confers nothing a
        // publisher could not already do.
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        // The executor is a machine admin; a tenant's Member/Viewer overrides do not apply to it.
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some(format!(
            "process.step:{}:{}:{}",
            row.run_id, row.position, row.attempt
        )),
        trace_id: Some(format!("run:{}", row.run_id)),
        depth: 1,
        app_key: app_key.to_owned(),
        usage: Some(crate::invoke::UsageContext {
            caller_kind: conveyor_domain::inference::CallerKind::ProcessStep,
            run_id: Some(row.run_id),
            run_step_id: Some(row.id),
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some(format!("process:{}", process.key)),
            prompt_version: Some(format!("{}:{}", process.version, step.position)),
            correlation_id: Some(format!(
                "run:{}:{}:{}",
                row.run_id, row.position, row.attempt
            )),
        }),
    };

    let outcome = match invoke(pool, registry(), tenant, request).await {
        Ok(outcome) => {
            Outcome::succeeded(outcome.body.get("output").cloned().unwrap_or(Value::Null))
        }
        Err(error) if error.status >= 500 => {
            // Nothing was decided: leave the step `running` and let the job retry. The replay path
            // re-invokes under the same key, so a retry cannot double-execute.
            return Err(StepFailure::retryable(format!(
                "{}: {}",
                error.code, error.detail
            )));
        }
        Err(error) => Outcome::failed(format!("{}: {}", error.code, error.detail)),
    };

    if outcome.status == "succeeded" {
        return judge_and_record(
            pool,
            tenant,
            app_key,
            context,
            process,
            step,
            row,
            outcome.output,
            next_position,
        )
        .await;
    }

    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
    record_and_chain(&mut tx, tenant, context, step, row, outcome, next_position).await?;
    tx.commit()
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
    Ok(())
}

// ── the judge phase (SPEC D28) ─────────────────────────────────────────────────────────────

/// How long a parked judge waits before asking again; doubled per park, up to the ceiling.
const JUDGE_PARK_SECONDS: i64 = 10;

/// The ceiling a judge poll backs off to (a laya outage, or a grade that is slow to settle).
const JUDGE_PARK_CEILING_SECONDS: i64 = 320;

/// The most times one attempt may park on a judge before the run fails CLOSED.
///
/// An outage waits (D28), but not forever: a permanently dead judge would hold the card's live
/// run slot (`uq_run_live`) with no bound and no signal. Past this bound the step is recorded
/// failed with `judge_unavailable`, which takes the ordinary `on_result` path: with the default
/// `fail` the run fails and `err.card` blocks the card and starts the coach; `continue` skips
/// the step; `retry` reruns it under the step's own cap. It never accepts an unjudged output.
pub const JUDGE_MAX_PARKS: u32 = 64;

/// The output a parked attempt stored, when the row is `running` and awaiting its judge.
///
/// `pending` (an eval still grading) and `unavailable` (a judge outage) are both "the action is
/// done, the decision is not". Every other verdict is terminal and the row is not re-entered.
fn awaiting_judge(row: &RunStepRecord) -> Option<Value> {
    let decision = row.judge.as_ref()?.get("decision")?.as_str()?;
    matches!(decision, "pending" | "unavailable").then(|| row.output.clone())
}

/// The eval grade this attempt already created, when it parked on one.
fn stored_grade_id(judge: Option<&Value>) -> Option<Uuid> {
    judge?
        .get("grade_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
}

/// How many times this attempt has parked, from the document the previous park wrote.
fn stored_parks(judge: Option<&Value>) -> u32 {
    judge
        .and_then(|doc| doc.get("parks"))
        .and_then(Value::as_u64)
        .and_then(|parks| u32::try_from(parks).ok())
        .unwrap_or(0)
}

/// The wait before the next judge poll: 10s, doubling per park to a 320s ceiling.
pub(crate) fn park_backoff_seconds(parks: u32) -> i64 {
    let doublings = parks.saturating_sub(1).min(5);
    (JUDGE_PARK_SECONDS * (1_i64 << doublings)).min(JUDGE_PARK_CEILING_SECONDS)
}

/// A database failure that decided nothing: the job retries on its lease.
pub(crate) fn database(error: impl std::fmt::Display) -> StepFailure {
    StepFailure::retryable(format!("database: {error}"))
}

/// What a judge call produced.
enum JudgeCall {
    /// A readable verdict.
    Verdict(judge::Verdict),
    /// No decision — an unreachable or unconfigured judge, or an answer with no verdict in it.
    /// Never a reject, and never an accept.
    Unavailable(String),
}

/// What an eval inspection produced.
enum EvalJudge {
    /// The grade settled; its status is the verdict.
    Verdict(judge::Verdict),
    /// The grade exists and is still running; the caller parks this attempt on it.
    Pending(Uuid),
    /// This call created the grade and parked the attempt in one transaction — there is nothing
    /// left for the caller to write.
    Parked,
    /// No grade could decide: the grader failed, or a grade this attempt owns is gone.
    Unavailable {
        /// The grade this attempt already created, when there is one. A park that drops it makes
        /// the next re-entry re-create the grade — one grading model call per park, forever.
        grade_id: Option<Uuid>,
        /// Why the grade is not a verdict.
        detail: String,
    },
}

/// Judge one successful step output, then record the attempt (SPEC D28).
///
/// Only a **readable** verdict may accept or refuse an output: a checker rule, a laya answer, or a
/// settled eval grade. Everything else — an unreachable laya, an unreadable answer, a grade still
/// running or one that failed to grade, an SOP that no longer parses — **parks** the attempt: the
/// output is stored, the row stays `running`, and a job returns to this same attempt later. A judge
/// outage therefore never fails open and is never counted as a reject.
///
/// No transaction is open here. The SOP read, the judge call and the record each own a short
/// scoped transaction, because a remote decision must never hold row locks (D13).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn judge_and_record(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    context: &RunContext,
    process: &ProcessRecord,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    output: Value,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    // ── a judge that never decides fails CLOSED at the bound ───────────────────────────────
    //
    // An outage waits (D28), so a parked attempt re-enters here with its stored output. It must
    // not wait forever: past the bound the step is recorded failed with `judge_unavailable` and
    // the ordinary `on_result` chain runs — `fail` (the default) fails the run and lets
    // `err.card` block the card and start the coach; `continue` skips the step; `retry` reruns
    // it under the step's cap. This never accepts the unjudged output and never spends a reject.
    if stored_parks(row.judge.as_ref()) >= JUDGE_MAX_PARKS {
        let detail = row
            .judge
            .as_ref()
            .and_then(|document| document.get("detail"))
            .and_then(Value::as_str)
            .unwrap_or("the judge kept answering without a verdict");
        let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
        record_and_chain(
            &mut tx,
            tenant,
            context,
            step,
            row,
            Outcome::failed(format!(
                "judge_unavailable: no verdict after {JUDGE_MAX_PARKS} parks ({detail})"
            )),
            next_position,
        )
        .await?;
        tx.commit().await.map_err(database)?;
        return Ok(());
    }

    let Some(procedure_id) = step.procedure_id else {
        // A step with no SOP names no judge: record the success exactly as before D28.
        return record_success(
            pool,
            tenant,
            context,
            step,
            row,
            output,
            None,
            next_position,
        )
        .await;
    };
    let Some(sop) = read_sop(pool, tenant, procedure_id).await? else {
        // The step pins a SOP that is gone. That is not a verdict on the output, so the step
        // waits rather than deciding anything by default.
        return park_judge(
            pool,
            tenant,
            row,
            output,
            json!({
                "decision": "unavailable",
                "detail": format!("the SOP {procedure_id} this step pins does not exist"),
            }),
            store::process_judge::JUDGE_WAIT_TAG,
        )
        .await;
    };
    let inspection = match conveyor_domain::procedure::validate_inspection(&sop.inspection) {
        Ok(inspection) => inspection,
        // Publish validated this same document, so a SOP that no longer parses is a data fault:
        // an outage-shaped error, never a silent accept.
        Err(error) => {
            return park_judge(
                pool,
                tenant,
                row,
                output,
                json!({
                    "decision": "unavailable",
                    "detail": format!("the SOP's inspection does not parse: {error}"),
                }),
                store::process_judge::JUDGE_WAIT_TAG,
            )
            .await;
        }
    };
    let max_rejects = inspection.max_rejects();

    match judge::judge_of(&inspection) {
        judge::JudgeKind::Checker { rule } => {
            let verdict = judge::checker_verdict(rule, &output);
            settle(
                pool,
                tenant,
                context,
                step,
                row,
                output,
                verdict,
                &sop.acceptance_criteria,
                max_rejects,
                next_position,
            )
            .await
        }
        judge::JudgeKind::Laya { threshold } => {
            match laya_verdict(
                pool,
                tenant,
                app_key,
                process,
                row,
                &output,
                &sop.acceptance_criteria,
                threshold,
            )
            .await?
            {
                JudgeCall::Verdict(verdict) => {
                    settle(
                        pool,
                        tenant,
                        context,
                        step,
                        row,
                        output,
                        verdict,
                        &sop.acceptance_criteria,
                        max_rejects,
                        next_position,
                    )
                    .await
                }
                JudgeCall::Unavailable(detail) => {
                    park_judge(
                        pool,
                        tenant,
                        row,
                        output,
                        json!({"decision": "unavailable", "detail": detail}),
                        store::process_judge::JUDGE_WAIT_TAG,
                    )
                    .await
                }
            }
        }
        judge::JudgeKind::Eval { eval_id, threshold } => {
            match eval_verdict(pool, tenant, row, &output, eval_id, threshold).await? {
                EvalJudge::Verdict(verdict) => {
                    settle(
                        pool,
                        tenant,
                        context,
                        step,
                        row,
                        output,
                        verdict,
                        &sop.acceptance_criteria,
                        max_rejects,
                        next_position,
                    )
                    .await
                }
                EvalJudge::Pending(grade_id) => {
                    park_judge(
                        pool,
                        tenant,
                        row,
                        output,
                        json!({"decision": "pending", "grade_id": grade_id}),
                        store::process_judge::JUDGE_GRADE_TAG,
                    )
                    .await
                }
                // The grade and the park committed together inside `eval_verdict`.
                EvalJudge::Parked => Ok(()),
                EvalJudge::Unavailable { grade_id, detail } => {
                    // A grade that ended `failed` is an outage AND a grade this attempt already
                    // owns, so the park MUST carry its id: without it the next re-entry cannot
                    // tell it already graded this output and creates a SECOND grade and a second
                    // `eval.grade` job — one grading model call per park, forever.
                    let mut document = json!({"decision": "unavailable", "detail": detail});
                    if let Some(grade_id) = grade_id {
                        document["grade_id"] = json!(grade_id);
                    }
                    park_judge(
                        pool,
                        tenant,
                        row,
                        output,
                        document,
                        store::process_judge::JUDGE_WAIT_TAG,
                    )
                    .await
                }
            }
        }
    }
}

/// The step's SOP, read in a short scoped transaction of its own.
async fn read_sop(
    pool: &PgPool,
    tenant: TenantId,
    procedure_id: Uuid,
) -> Result<Option<store::procedure::SopRecord>, StepFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    let sop = store::procedure::get_sop_tx(&mut tx, tenant, procedure_id)
        .await
        .map_err(database)?;
    tx.commit().await.map_err(database)?;
    Ok(sop)
}

/// Ask laya to judge one output through the registered `laya.judge` action.
#[allow(clippy::too_many_arguments)]
async fn laya_verdict(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    process: &ProcessRecord,
    row: &RunStepRecord,
    output: &Value,
    acceptance_criteria: &Value,
    threshold: Option<f64>,
) -> Result<JudgeCall, StepFailure> {
    let request = InvokeRequest {
        action: "laya.judge".to_owned(),
        input: judge::request(None, output, acceptance_criteria, Some(&row.input)),
        precondition: None,
        // No `Idempotency-Key`. A failed call keeps its claim live for the TTL, which would turn
        // an outage into a `409` that cannot clear; and the call decides nothing and changes
        // nothing of ours, so asking again is the honest retry.
        idempotency_key: None,
        actor_id: process.owner_user_id.unwrap_or(Uuid::nil()),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some(format!(
            "process.step.judge:{}:{}:{}",
            row.run_id, row.position, row.attempt
        )),
        trace_id: Some(format!("run:{}", row.run_id)),
        depth: 1,
        app_key: app_key.to_owned(),
        usage: None,
    };
    match invoke(pool, registry(), tenant, request).await {
        Ok(outcome) => {
            // `invoke` answers the invocation envelope (`{output, request_id, duration_ms}`);
            // laya's answer document is its `output`. Parsing the envelope would find no
            // `answers` and turn every judge call into a silent, permanent park.
            let answer = outcome.body.get("output").unwrap_or(&outcome.body);
            match judge::parse_verdict(answer, threshold) {
                Ok(verdict) => Ok(JudgeCall::Verdict(verdict)),
                // The judge answered, but not in a shape that carries a verdict. A garbled answer
                // is evidence of nothing: the step waits.
                Err(error) => {
                    tracing::warn!(
                        run = %row.run_id,
                        position = row.position,
                        attempt = row.attempt,
                        %error,
                        "the laya judge answered without a readable verdict"
                    );
                    Ok(JudgeCall::Unavailable(error.to_string()))
                }
            }
        }
        // A 5xx, a timeout, an unconfigured service or a missing `laya.token`: the judge is
        // unavailable. It is an outage, never a rejection (D28).
        Err(error) => {
            tracing::warn!(
                run = %row.run_id,
                position = row.position,
                attempt = row.attempt,
                code = error.code,
                "the laya judge could not decide"
            );
            Ok(JudgeCall::Unavailable(format!(
                "{}: {}",
                error.code, error.detail
            )))
        }
    }
}

/// The verdict of an eval inspection: re-read a parked grade, or create the one-shot grade.
async fn eval_verdict(
    pool: &PgPool,
    tenant: TenantId,
    row: &RunStepRecord,
    output: &Value,
    eval_id: Uuid,
    threshold: f64,
) -> Result<EvalJudge, StepFailure> {
    // A parked attempt re-reads its grade; it never creates a second one for the same output.
    if let Some(grade_id) = stored_grade_id(row.judge.as_ref()) {
        let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
        let grade = store::eval::get_grade_tx(&mut tx, tenant, grade_id)
            .await
            .map_err(database)?;
        tx.commit().await.map_err(database)?;
        let Some(grade) = grade else {
            // The grade row is the record; without it there is nothing left to wait for. The
            // park still names it, so a re-entry cannot mistake "gone" for "not created yet" and
            // write a second grade for the same output.
            return Ok(EvalJudge::Unavailable {
                grade_id: Some(grade_id),
                detail: format!("eval grade {grade_id} no longer exists"),
            });
        };
        let score = grade.score.and_then(|score| score.to_f64());
        let threshold = grade.threshold.to_f64().unwrap_or(threshold);
        let status = grade.status.clone();
        return Ok(match status.as_str() {
            "pending" => EvalJudge::Pending(grade_id),
            "accepted" | "rejected" => EvalJudge::Verdict(judge::Verdict {
                accepted: status == "accepted",
                score,
                reason: grade.reason.unwrap_or_default(),
                raw: json!({
                    "grade_id": grade_id,
                    "status": status,
                    "score": score,
                    "threshold": threshold,
                }),
            }),
            // A grader failure was never a verdict on the output (eval_grade's own rule): the
            // step waits for a judge that can decide — on the grade it already owns, never a
            // fresh one.
            _ => EvalJudge::Unavailable {
                grade_id: Some(grade_id),
                detail: grade
                    .error
                    .unwrap_or_else(|| format!("the eval grade ended {status}")),
            },
        });
    }

    let Ok(threshold) = Decimal::try_from(threshold) else {
        return Ok(EvalJudge::Unavailable {
            grade_id: None,
            detail: "the SOP's eval threshold is not a number between 0 and 1".to_owned(),
        });
    };
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    let eval = match store::eval::get_eval_tx(&mut tx, tenant, eval_id).await {
        Ok(eval) => eval,
        // The SOP pins an eval that does not exist in this tenant: the judge cannot decide.
        Err(store::eval::EvalStoreError::NotFound { .. }) => {
            let _ = tx.rollback().await;
            return Ok(EvalJudge::Unavailable {
                grade_id: None,
                detail: format!("the eval {eval_id} this SOP pins does not exist"),
            });
        }
        Err(error) => return Err(database(error)),
    };
    let Some(version_id) = eval.current_version_id else {
        let _ = tx.rollback().await;
        return Ok(EvalJudge::Unavailable {
            grade_id: None,
            detail: format!("the eval {eval_id} has no version to grade against"),
        });
    };
    let output_hash = conveyor_domain::compare::content_hash(output);
    let grade_id = store::eval::insert_pending_tx(
        &mut tx,
        tenant,
        &store::eval::NewEvalGrade {
            eval_id,
            version_id,
            output_hash: &output_hash,
            threshold,
        },
    )
    .await
    .map_err(database)?;
    // The grade row and the job that settles it commit together: a grade with no job would stay
    // `pending` forever, and a job whose row rolled back would grade nothing.
    store::jobs::enqueue_tx(
        &mut tx,
        tenant,
        crate::eval::GRADE_JOB_KIND,
        &grade_id.to_string(),
        json!({
            "grade_id": grade_id,
            "output": output,
            "case_input": row.input,
            "expected": Value::Null,
            "threshold": threshold.to_f64().unwrap_or_default(),
        }),
        None,
        conveyor_domain::eval::RetryPolicy::default().max_attempts as i32,
    )
    .await
    .map_err(database)?;
    // ⛔ The grade row, its `eval.grade` job, the park document and the park job commit
    // TOGETHER. Splitting them would let a transient database failure re-enter the attempt with
    // no stored `grade_id`, invoke the action again and create a SECOND grade — a second grading
    // model call for one output.
    let parks = stored_parks(row.judge.as_ref()) + 1;
    let run_after: DateTime<Utc> =
        Utc::now() + chrono::Duration::seconds(park_backoff_seconds(parks));
    store::process_judge::record_output_tx(
        &mut tx,
        tenant,
        row.id,
        output,
        &json!({"decision": "pending", "grade_id": grade_id, "parks": parks}),
    )
    .await
    .map_err(database)?;
    store::process_judge::enqueue_judge_park_tx(
        &mut tx,
        tenant,
        row.run_id,
        row.position,
        row.attempt,
        store::process_judge::JUDGE_GRADE_TAG,
        parks,
        Some(run_after),
    )
    .await
    .map_err(database)?;
    tx.commit().await.map_err(database)?;
    Ok(EvalJudge::Parked)
}

/// The names of the SOP's acceptance criteria, as the judge document records them.
///
/// The criteria are stored as JSON on the SOP (`{name, text}` entries); the verdict keeps the
/// NAMES — what was judged against — while the text stays on the published SOP, which a run pins
/// by `procedure_id`. A malformed entry contributes nothing rather than failing a settled step.
fn criteria_names(criteria: &Value) -> Vec<String> {
    criteria
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    // The catalogue writes `{name, text}`; a caller may write the bare text.
                    Value::String(text) => Some(text.clone()),
                    other => other.get("name").and_then(Value::as_str).map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Record what a readable verdict decided: accept, or the reject ladder.
#[allow(clippy::too_many_arguments)]
async fn settle(
    pool: &PgPool,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    output: Value,
    verdict: judge::Verdict,
    criteria: &Value,
    max_rejects: u32,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    // S13 §5.5: the verdict records WHAT it judged against and the hash of the output it judged.
    // Without them "accepted" is an opinion with no reproducible subject: the criteria could be
    // edited afterwards and the output could not be re-hashed from the run.
    let document = json!({
        "decision": if verdict.accepted { "accept" } else { "reject" },
        "score": verdict.score,
        "reason": verdict.reason,
        "criteria": criteria_names(criteria),
        "output_hash": conveyor_domain::compare::content_hash(&output),
        "raw": verdict.raw,
    });
    if verdict.accepted {
        return record_success(
            pool,
            tenant,
            context,
            step,
            row,
            output,
            Some(&document),
            next_position,
        )
        .await;
    }
    record_reject(
        pool,
        tenant,
        context,
        step,
        row,
        &output,
        &document,
        max_rejects,
    )
    .await
}

/// Write the verdict and record the attempt as succeeded, in one transaction.
#[allow(clippy::too_many_arguments)]
async fn record_success(
    pool: &PgPool,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    output: Value,
    verdict: Option<&Value>,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    if let Some(verdict) = verdict {
        store::process_judge::set_verdict_tx(&mut tx, tenant, row.id, verdict)
            .await
            .map_err(database)?;
    }
    record_and_chain(
        &mut tx,
        tenant,
        context,
        step,
        row,
        Outcome::succeeded(output),
        next_position,
    )
    .await?;
    tx.commit().await.map_err(database)?;
    Ok(())
}

/// Record a rejected attempt: rerun the same position with the reason, or block the run.
///
/// The rerun budget is the SOP's `max_rejects`, not the step's `max_attempts`: a judge reject is
/// a refusal of the output, not a transient failure. An exhausted budget leaves the ordinary
/// `fail_run` path to fire `run.failed` and start the card's coach (D28).
#[allow(clippy::too_many_arguments)]
async fn record_reject(
    pool: &PgPool,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    output: &Value,
    verdict: &Value,
    max_rejects: u32,
) -> Result<(), StepFailure> {
    let reason = verdict
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty());
    let message = match reason {
        Some(reason) => format!("process.judge_rejected: {reason}"),
        None => "process.judge_rejected: the judge rejected the step output".to_owned(),
    };
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(database)?;
    store::process_judge::set_verdict_tx(&mut tx, tenant, row.id, verdict)
        .await
        .map_err(database)?;
    // Counted after the verdict above, on this transaction's own connection, so the count
    // includes the reject being recorded now.
    let rejects =
        store::process_judge::count_judge_rejects_tx(&mut tx, tenant, row.run_id, row.position)
            .await
            .map_err(database)?;
    let Some(recorded) = store::process::record_run_step_tx(
        &mut tx,
        tenant,
        row.id,
        "failed",
        &row.input,
        output,
        Some(&message),
    )
    .await
    .map_err(database)?
    else {
        // A cancel or the reaper recorded this attempt first; its transaction owns the chain.
        return Ok(());
    };
    emit_step_changed(
        &mut tx,
        tenant,
        context,
        recorded.position,
        recorded.attempt,
        &recorded.status,
    )
    .await?;
    if !run_is_running(&mut tx, tenant, context.id).await? {
        // Cancelled while the judge ran: the record is honest and stays, but the run owns its
        // terminal state and no rerun or block follows.
        tx.commit().await.map_err(database)?;
        return Ok(());
    }
    if (rejects.max(0) as u64) < u64::from(max_rejects) {
        // The budget is not spent: rerun the SAME position. The reason reaches the next attempt
        // through `carry_judge_reason`, so the rerun can see what was refused.
        let next = store::process::append_attempt_tx(&mut tx, tenant, recorded.id)
            .await
            .map_err(database)?;
        store::process::enqueue_process_step_tx(
            &mut tx,
            tenant,
            context.id,
            next.position,
            next.attempt,
        )
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;
        return Ok(());
    }
    // Out of budget: the card blocks and the coach takes over (D28).
    fail_run(&mut tx, tenant, context, step, &recorded).await?;
    tx.commit().await.map_err(database)?;
    Ok(())
}

/// Store the output and park the attempt, then queue the job that comes back to it.
///
/// The row is left `running` and the job carries its own dedupe slot: the attempt is neither
/// decided nor dropped, it is waiting, and the wait is bounded by the backoff rather than by a
/// lease.
async fn park_judge(
    pool: &PgPool,
    tenant: TenantId,
    row: &RunStepRecord,
    output: Value,
    mut document: Value,
    tag: &str,
) -> Result<(), StepFailure> {
    let parks = stored_parks(row.judge.as_ref()) + 1;
    if let Value::Object(doc) = &mut document {
        doc.insert("parks".to_owned(), json!(parks));
    }
    let run_after: DateTime<Utc> =
        Utc::now() + chrono::Duration::seconds(park_backoff_seconds(parks));
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
        tag,
        parks,
        Some(run_after),
    )
    .await
    .map_err(database)?;
    tx.commit().await.map_err(database)?;
    Ok(())
}

/// The reason a judge rejected the previous attempt, carried into the next attempt's input.
///
/// A rerun exists because the last output was refused; the reason is the only thing that tells it
/// what to change. It rides the input under `_judge_reason` and is absent everywhere else.
async fn carry_judge_reason(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    position: i32,
    attempt: i32,
    input: Value,
) -> Result<Value, StepFailure> {
    if attempt <= 1 {
        return Ok(input);
    }
    let Some(previous) =
        store::process::get_run_step_by_triple_tx(conn, tenant, context.id, position, attempt - 1)
            .await
            .map_err(database)?
    else {
        return Ok(input);
    };
    let Some(verdict) = previous
        .judge
        .as_ref()
        .filter(|doc| doc.get("decision").and_then(Value::as_str) == Some("reject"))
    else {
        return Ok(input);
    };
    let reason = verdict
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut value = input;
    if let Value::Object(map) = &mut value {
        map.insert("_judge_reason".to_owned(), Value::String(reason.to_owned()));
    }
    Ok(value)
}

/// What the step row records, and the `on_result` branch that follows it.
pub(crate) struct Outcome {
    /// `succeeded` or `failed`.
    status: &'static str,
    /// The action's output document (null for a failure).
    output: Value,
    /// The failure text, when there is one.
    error: Option<String>,
}

impl Outcome {
    fn succeeded(output: Value) -> Self {
        Self {
            status: "succeeded",
            output,
            error: None,
        }
    }

    pub(crate) fn failed(error: impl Into<String>) -> Self {
        Self {
            status: "failed",
            output: Value::Null,
            error: Some(error.into()),
        }
    }
}

/// Record the outcome, emit the transition, and move the run on — all on the caller's transaction.
pub(crate) async fn record_and_chain(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    row: &RunStepRecord,
    outcome: Outcome,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    // `continue` is the schema's word for "record the failure, do not stop the run": the row's
    // status is `skipped` and the error text is kept, so the trace says both things at once.
    let status = if outcome.error.is_some() && step.on_result == "continue" {
        "skipped"
    } else {
        outcome.status
    };
    let Some(recorded) = store::process::record_run_step_tx(
        conn,
        tenant,
        row.id,
        status,
        &row.input,
        &outcome.output,
        outcome.error.as_deref(),
    )
    .await
    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
    else {
        // A cancel or the reaper recorded this step first; its transaction owns the chain.
        return Ok(());
    };
    emit_step_changed(
        conn,
        tenant,
        context,
        recorded.position,
        recorded.attempt,
        &recorded.status,
    )
    .await?;
    chain_after_record(conn, tenant, context, step, &recorded, next_position).await
}

/// Continue from a recorded step: enqueue the next position, retry the attempt, or close the run.
///
/// The run-status guard is what stops a cancel that landed while the action was in flight from
/// being resurrected by the chain: the record is honest and stays, but the follow-on (a next job,
/// a retry, a transition) is suppressed. Runs whose chain the cancel already rewrote are untouched
/// because their rows are no longer `running`.
async fn chain_after_record(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    recorded: &RunStepRecord,
    next_position: Option<i32>,
) -> Result<(), StepFailure> {
    if !run_is_running(conn, tenant, context.id).await? {
        return Ok(());
    }
    match recorded.status.as_str() {
        "succeeded" | "skipped" => {
            if let Some(next) = next_position {
                store::process::enqueue_process_step_tx(conn, tenant, context.id, next, 1)
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                Ok(())
            } else {
                finish_run_succeeded(conn, tenant, context).await
            }
        }
        "failed" => {
            let cap = step.max_attempts.clamp(1, 10);
            if step.on_result == "retry" && recorded.attempt < cap {
                let next = store::process::append_attempt_tx(conn, tenant, recorded.id)
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                store::process::enqueue_process_step_tx(
                    conn,
                    tenant,
                    context.id,
                    next.position,
                    next.attempt,
                )
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
                return Ok(());
            }
            fail_run(conn, tenant, context, step, recorded).await
        }
        other => Err(StepFailure::permanent(format!(
            "run_step {} recorded an unknown status {other:?}",
            recorded.id
        ))),
    }
}

/// True while the run is still `running`; a terminal or `waiting_approval` run means the chain
/// must not act (the state that owns the run wins).
async fn run_is_running(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<bool, StepFailure> {
    Ok(store::run::get_run_context_tx(conn, tenant, run_id)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
        .is_some_and(|context| context.status == RunStatus::Running))
}

/// The catalogue gate's quorum and expiry.
///
/// `pub(crate)` because S13's `workflow.node` opens a gate by the same catalogue rules (a workflow
/// gate must mean exactly what a process gate means), and duplicating the read would be a second
/// source of truth for `dual_control` and `expires_at`.
pub(crate) struct GateRequirements {
    /// The decisions the gate needs before it resolves.
    pub(crate) required_decisions: u16,
    /// When the gate expires, per the catalogue entry.
    pub(crate) expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Read `catalogue_entry(kind = 'gate').meta` for a step's gate code.
///
/// `dual_control` is the seed's only field today and means a two-person quorum; an explicit
/// `required_decisions` wins when present. A malformed `expires_at` is a refusal, not "no expiry":
/// a gate published to expire must not silently hold the card's live-run slot forever.
pub(crate) async fn gate_requirements(
    conn: &mut PgConnection,
    tenant: TenantId,
    gate_kind: &str,
) -> Result<GateRequirements, StepFailure> {
    let code = Code::parse(gate_kind).map_err(|error| {
        StepFailure::permanent(format!(
            "process.gate_invalid: {gate_kind:?} is not a catalogue code: {error}"
        ))
    })?;
    let entry = store::catalogue::get_catalogue_entry_tx(conn, tenant, CatalogueKind::Gate, &code)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
        .ok_or_else(|| {
            StepFailure::permanent(format!(
                "process.gate_unknown: {gate_kind:?} is not a gate in this tenant's catalogue"
            ))
        })?;
    let dual = entry
        .meta
        .get("dual_control")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let required_decisions = match entry.meta.get("required_decisions").and_then(Value::as_u64) {
        Some(value) => u16::try_from(value.clamp(1, 255)).unwrap_or(1),
        None if dual => 2,
        None => 1,
    };
    let expires_at = match entry.meta.get("expires_at").and_then(Value::as_str) {
        None => None,
        Some(raw) => Some(
            chrono::DateTime::parse_from_rfc3339(raw)
                .map_err(|error| {
                    StepFailure::permanent(format!(
                        "process.gate_invalid: gate {gate_kind:?} expires_at {raw:?}: {error}"
                    ))
                })?
                .with_timezone(&chrono::Utc),
        ),
    };
    Ok(GateRequirements {
        required_decisions,
        expires_at,
    })
}

/// Park a gated step until its approval resolves.
///
/// `Ok(false)` means an approved gate already exists for this exact attempt — the caller runs the
/// step. `Ok(true)` means the step is parked: the gate was opened here, or a concurrent worker
/// won the `uq_appr_pending` race, or the run had already left `running` (the cancel/expiry owns
/// it). `Err` is reserved for a catalogue/meta problem (permanent) or a database failure (retry).
#[allow(clippy::too_many_arguments)]
async fn ensure_gate_open(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    gate_kind: &str,
    position: i32,
    attempt: i32,
    requested_by: Option<Uuid>,
) -> Result<bool, StepFailure> {
    if store::approval::approved_process_gate_tx(conn, tenant, context.id, position, attempt)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
        .is_some()
    {
        return Ok(false);
    }
    let requirements = gate_requirements(conn, tenant, gate_kind).await?;
    let meta = json!({
        "position": position,
        "attempt": attempt,
        "process_step_id": step.id,
    });
    match store::approval::open_gate_with_meta_tx(
        conn,
        tenant,
        context.id,
        gate_kind,
        "process",
        requirements.required_decisions,
        requirements.expires_at,
        requested_by.filter(|id| *id != Uuid::nil()),
        meta,
    )
    .await
    {
        Ok(_) => Ok(true),
        Err(store::ApprovalStoreError::Conflict) => Ok(true),
        Err(store::ApprovalStoreError::Run(
            RunStoreError::Illegal(_) | RunStoreError::NotFound,
        )) => Ok(true),
        Err(store::ApprovalStoreError::Database(error)) => {
            Err(StepFailure::retryable(format!("database: {error}")))
        }
        Err(error) => Err(StepFailure::permanent(format!(
            "process.gate_invalid: {error}"
        ))),
    }
}

/// Resolve one decided or expired process gate, on the caller's transaction.
///
/// Called by `approvals.decide` (approved/rejected) and the `approval.expire` worker (expired).
/// `pending`/`voided` are no-ops: the first is a quorum still filling, the second is a cancel that
/// already owns the run. `approved` re-enqueues the SAME `(position, attempt)` under the same
/// dedupe key; `rejected`/`expired` applies the step's `on_result` through the ordinary chain, so
/// `fail`/`continue`/`retry` mean exactly what they mean everywhere else.
pub async fn resolve_gate(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<(), StepFailure> {
    let Some(gate) = store::approval::process_gate_tx(conn, tenant, approval_id)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
    else {
        return Ok(());
    };
    match gate.status.as_str() {
        "approved" => {
            let (Some(run_id), Some(position), Some(attempt)) =
                (gate.run_id, gate.position, gate.attempt)
            else {
                return Err(StepFailure::permanent(format!(
                    "process gate {approval_id} is malformed: run/position/attempt missing"
                )));
            };
            // `waiting_approval -> running`; a run that moved (cancelled, reaped) wins.
            match store::run::transition_run_tx(conn, tenant, run_id, RunStatus::Running).await {
                Ok(_) => {}
                Err(RunStoreError::Illegal(_) | RunStoreError::NotFound) => return Ok(()),
                Err(error) => {
                    return Err(StepFailure::retryable(format!("database: {error}")));
                }
            }
            // Same step job, own dedupe slot: the job that opened the gate may still be `running`
            // when an automated approval lands, and collapsing onto it would stall the run (see
            // `enqueue_process_gate_resume_tx`). The executor sees the approved gate and claims.
            store::process::enqueue_process_gate_resume_tx(conn, tenant, run_id, position, attempt)
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
            Ok(())
        }
        "rejected" | "expired" => {
            let (Some(run_id), Some(position), Some(attempt)) =
                (gate.run_id, gate.position, gate.attempt)
            else {
                return Err(StepFailure::permanent(format!(
                    "process gate {approval_id} is malformed: run/position/attempt missing"
                )));
            };
            let Some(context) = store::run::get_run_context_tx(conn, tenant, run_id)
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
            else {
                return Ok(());
            };
            let Some(process_id) = context.process_id else {
                return Ok(());
            };
            if store::process::get_process_tx(conn, tenant, process_id)
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
                .is_none()
            {
                return Err(StepFailure::permanent(format!(
                    "the process version {process_id} of run {run_id} no longer exists"
                )));
            }
            let steps = store::process::list_steps_tx(conn, tenant, process_id)
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
            let Some(step) = steps.iter().find(|step| step.position == position).cloned() else {
                return Err(StepFailure::permanent(format!(
                    "the frozen process has no step at position {position}"
                )));
            };
            let next_position = steps
                .iter()
                .filter(|candidate| candidate.position > position)
                .map(|candidate| candidate.position)
                .min();
            let Some(row) =
                store::process::get_run_step_by_triple_tx(conn, tenant, run_id, position, attempt)
                    .await
                    .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
            else {
                return Err(StepFailure::permanent(format!(
                    "run {run_id} has no run_step at position {position} attempt {attempt}"
                )));
            };
            // Back to `running` first: the domain has no `waiting_approval -> failed` edge, and
            // the chain (including `fail_run`) is written against a running run.
            match store::run::transition_run_tx(conn, tenant, run_id, RunStatus::Running).await {
                Ok(_) => {}
                Err(RunStoreError::Illegal(_) | RunStoreError::NotFound) => return Ok(()),
                Err(error) => {
                    return Err(StepFailure::retryable(format!("database: {error}")));
                }
            }
            let status = if step.on_result == "continue" {
                "skipped"
            } else {
                "failed"
            };
            let reason = if gate.status == "expired" {
                format!("process.gate_expired: gate {} expired", gate.gate)
            } else {
                format!("process.gate_rejected: gate {} was rejected", gate.gate)
            };
            let Some(recorded) = store::process::record_queued_run_step_tx(
                conn,
                tenant,
                row.id,
                status,
                Some(reason.as_str()),
            )
            .await
            .map_err(|error| StepFailure::retryable(format!("database: {error}")))?
            else {
                // Cancelled or reaped first; that transaction owns the chain.
                return Ok(());
            };
            emit_step_changed(
                conn,
                tenant,
                &context,
                recorded.position,
                recorded.attempt,
                &recorded.status,
            )
            .await?;
            chain_after_record(conn, tenant, &context, &step, &recorded, next_position).await
        }
        _ => Ok(()),
    }
}

/// `running → succeeded` and `run.succeeded`, or nothing when the run moved underneath.
async fn finish_run_succeeded(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
) -> Result<(), StepFailure> {
    match store::run::transition_run_tx(conn, tenant, context.id, RunStatus::Succeeded).await {
        Ok(run) => {
            emit_run_event(
                conn,
                tenant,
                context,
                "run.succeeded",
                json!({ "run_id": run.id, "outcome": "succeeded", "attempt": run.attempt }),
            )
            .await
        }
        // Cancelled while the step ran: the cancel owns the terminal state and its event.
        Err(RunStoreError::Illegal(_) | RunStoreError::NotFound) => Ok(()),
        Err(error) => Err(StepFailure::retryable(format!("database: {error}"))),
    }
}

/// Fail the step's run: skip every remaining queued step, record the failure, emit both events.
async fn fail_run(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    step: &ProcessStepRecord,
    recorded: &RunStepRecord,
) -> Result<(), StepFailure> {
    let skipped = store::process::skip_queued_run_steps_tx(conn, tenant, context.id)
        .await
        .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
    for row in &skipped {
        emit_step_changed(conn, tenant, context, row.position, row.attempt, "skipped").await?;
    }

    let message = format!(
        "step {} ({}) failed{}",
        step.position,
        step.kind,
        recorded
            .error
            .as_deref()
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    );
    match store::run::fail_run_tx(
        conn,
        tenant,
        context.id,
        Some("process_step_failed"),
        Some(&message),
    )
    .await
    {
        Ok(Some(run)) => {
            emit_run_event(
                conn,
                tenant,
                context,
                "run.failed",
                json!({
                    "run_id": run.id,
                    "error_class": "process_step_failed",
                    "attempt": run.attempt,
                }),
            )
            .await?;
            // The old `err.card` error trigger: a failed run starts the card's recovery flow.
            store::run::enqueue_run_failed_trigger_tx(conn, tenant, run.id)
                .await
                .map_err(|error| StepFailure::retryable(format!("database: {error}")))?;
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(error) => Err(StepFailure::retryable(format!("database: {error}"))),
    }
}

/// One `run_step.changed`, on the run's topic.
async fn emit_step_changed(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    position: i32,
    attempt: i32,
    status: &str,
) -> Result<(), StepFailure> {
    emit_run_event(
        conn,
        tenant,
        context,
        "run_step.changed",
        json!({
            "run_id": context.id,
            "position": position,
            "attempt": attempt,
            "status": status,
        }),
    )
    .await
}

/// Append one run event in the caller's transaction.
async fn emit_run_event(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &RunContext,
    event_type: &str,
    payload: Value,
) -> Result<(), StepFailure> {
    store::outbox::append(
        conn,
        tenant,
        &store::outbox::NewEvent {
            topic: format!("run:{}", context.id),
            event_type: event_type.to_owned(),
            aggregate_type: "run".to_owned(),
            aggregate_id: context.id,
            payload,
            correlation_id: context.correlation_id,
            causation_id: None,
        },
    )
    .await
    .map(|_| ())
    .map_err(|error| match error {
        store::outbox::OutboxError::Database(error) => {
            StepFailure::retryable(format!("database: {error}"))
        }
        other => StepFailure::permanent(format!("event: {other}")),
    })
}
