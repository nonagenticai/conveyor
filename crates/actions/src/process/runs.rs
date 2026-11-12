#![forbid(unsafe_code)]
//! `process_runs.start` — a published process version starts a run on a card.
//!
//! This is NOT a second run state machine (S11 §5.2): it composes S06's `runs.create` and `runs.start`
//! in one transaction — the same card/board resolution, the same `uq_run_live` one-live-run-per-card
//! guard, the same `run.created`/`run.started` events — and adds only what is process-specific:
//! the version check, the card-type match, the materialised `run_step` trace, and the job that drives
//! the first step.
//!
//! **Which step runs first.** Every step is materialised `queued` at attempt 1, and exactly ONE
//! `process.step` job is enqueued for position 1. The executor (S11 step 8) claims a step with
//! `claim_run_step_tx` — a status-guarded `queued → running` claim — so leaving position 1 queued is
//! what lets it claim at all; enqueuing every position would let a later step claim before an earlier
//! one finished, because the claim does not check order. A retry appends the next attempt at the same
//! position and enqueues THAT attempt.
//!
//! **No `run.execute` job.** `worker::run_execute` is a bare `queued → running` hook (S11/S13 replace
//! its body); the transition happens here, so enqueueing it would be a second driver — and a `run:`
//! job would feed `reap_dead_runs_tx`, which reads a dead `run.execute` as "the run crashed".

use conveyor_domain::{RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::process::ProcessRecord;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::runs::{emit_run_event, read_record, run_out};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::invalid;
use super::process_error;
use super::views::ProcessRunStartIn;

const ACTION: &str = "process_runs.start";

/// The execution context a run starts under (D22 / CONTRACT §17).
///
/// One slot for the three executors the contract unifies — a process step, `agent.run` and an
/// external MCP task — resolved once, before the first step. `Unresolved` is the only arm S11 can
/// produce: `card_type.capability_id`, `capability_agent` and their tables are S15's `0020`. S15
/// fills [`resolve_start_context`] and the `Resolved` arm of [`ensure_start_ready`]; the enum is
/// public and explicit so "no capability declared" is a visible state, never a check that silently
/// passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartContext {
    /// No capability is bound to the card or the process; legacy process runs execute with
    /// the publisher's certified role and no capability checks.
    Unresolved,
    /// A capability binding was resolved; the run records it and readiness applies.
    Resolved {
        /// The capability the run executes under.
        capability_id: Uuid,
        /// The agent configuration the run was resolved against, when exactly one is bound.
        capability_agent_id: Option<Uuid>,
        /// Every executor the process's steps name. Empty when the process binds none, in which
        /// case the capability's single-active-executor rule applies.
        bound_agents: Vec<Uuid>,
    },
}

impl StartContext {
    /// The capability id the run records, if any.
    pub fn capability_id(&self) -> Option<Uuid> {
        match self {
            Self::Unresolved => None,
            Self::Resolved { capability_id, .. } => Some(*capability_id),
        }
    }

    /// The capability-agent id the run records, if any.
    pub fn capability_agent_id(&self) -> Option<Uuid> {
        match self {
            Self::Unresolved => None,
            Self::Resolved {
                capability_agent_id,
                ..
            } => *capability_agent_id,
        }
    }
}

/// Resolve the capability/agent a run starts under (D22 / CONTRACT §17).
///
/// **S15 fills this.** `card_type.capability_id` is the default path and `process.capability_id`
/// the nullable override; neither column exists yet, so the honest answer today is `Unresolved`.
/// The parameters are the inputs that resolution will need; they are deliberately part of the
/// signature so S15 changes a body, not the start path.
/// The process's own capability override.
///
/// `ProcessRecord` predates 0020's column; this reads the one column the resolver needs rather
/// than widening the record and every view built from it in a shared slice.
async fn process_capability_tx(
    tx: &mut PgConnection,
    tenant: TenantId,
    process_id: Uuid,
) -> Result<Option<Uuid>, ActionError> {
    let row: Option<(Option<Uuid>,)> =
        sqlx::query_as("SELECT capability_id FROM process WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(process_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| {
                tracing::error!(%error, action = ACTION, "read process capability");
                ActionError::internal()
            })?;
    Ok(row.and_then(|(capability_id,)| capability_id))
}

async fn resolve_start_context(
    tx: &mut PgConnection,
    tenant: TenantId,
    card_type_id: Uuid,
    process: &ProcessRecord,
) -> Result<StartContext, ActionError> {
    let capability = match process_capability_tx(&mut *tx, tenant, process.id).await? {
        Some(capability_id) => {
            store::capability::get_capability_tx(&mut *tx, tenant, capability_id)
                .await
                .map_err(capability_error)?
        }
        None => store::capability::capability_for_card_type_tx(&mut *tx, tenant, card_type_id)
            .await
            .map_err(capability_error)?,
    };
    let Some(capability) = capability else {
        return Ok(StartContext::Unresolved);
    };

    // A process whose steps bind process agents (D29) names its executors directly: the
    // catalogue seeds one agent per process, but a tenant process may bind a different agent to
    // each step, so the set — not the "exactly one active executor" rule — is what is checked.
    let bound_agents = store::process::process_agent_ids_tx(&mut *tx, tenant, process.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = ACTION, "read the process's bound agents");
            ActionError::internal()
        })?;
    let capability_agent_id = match bound_agents.as_slice() {
        [agent_id] => Some(*agent_id),
        [] => {
            let agents =
                store::capability::list_agents_tx(&mut *tx, tenant, Some(capability.id), 100)
                    .await
                    .map_err(capability_error)?;
            let active: Vec<_> = agents
                .into_iter()
                .filter(|agent| agent.status == "active")
                .collect();
            (active.len() == 1).then(|| active[0].id)
        }
        // Several agents, each bound to its own step: the run records no single executor and
        // readiness checks every one of them.
        _ => None,
    };
    Ok(StartContext::Resolved {
        capability_id: capability.id,
        capability_agent_id,
        bound_agents,
    })
}

/// The pre-run readiness guard (D22 / CONTRACT §17).
///
/// Runs after resolution and **before** the run row, the `run_step` rows or the first job commit:
/// a refusal leaves zero `run_step` rows and no live-run slot held. `Unresolved` means no
/// capability was declared, so there is nothing to check; `Resolved` asks the one readiness
/// computation `capabilities.get` also returns. The refusal's `errors[]` is `readiness_fields`'s
/// output — the SAME array the GET returns as `blocking[]`, so the two surfaces cannot drift.
async fn ensure_start_ready(
    tx: &mut PgConnection,
    tenant: TenantId,
    context: &StartContext,
) -> Result<(), ActionError> {
    let StartContext::Resolved {
        capability_id,
        bound_agents,
        ..
    } = context
    else {
        return Ok(());
    };
    let readiness = crate::capability::readiness::capability_readiness_tx(
        &mut *tx,
        tenant,
        *capability_id,
        bound_agents,
    )
    .await?;
    if readiness.is_ready() {
        Ok(())
    } else {
        Err(ActionError::new(
            409,
            "capability_not_ready",
            format!("{ACTION}: the capability is not ready to run"),
        )
        .with_errors(crate::capability::readiness::readiness_fields(&readiness)))
    }
}

fn capability_error(error: conveyor_store::capability::CapabilityStoreError) -> ActionError {
    use conveyor_store::capability::CapabilityStoreError;
    match error {
        CapabilityStoreError::NotFound => ActionError::resource_not_found("such capability"),
        CapabilityStoreError::Database(error) => {
            tracing::error!(%error, action = ACTION, "capability readiness read failed");
            ActionError::internal()
        }
        other => ActionError::new(500, "internal", format!("{ACTION}: {other}")),
    }
}

/// The pre-run readiness guard (D22 / CONTRACT §17).
///
/// The version a caller named is not published, so it cannot run.
fn not_published() -> ActionError {
    ActionError::new(
        409,
        "process.not_published",
        "the process is not published; only a published version can start a run",
    )
}

/// The card is not of the type the process drives.
fn card_type_mismatch(process: &ProcessRecord) -> ActionError {
    ActionError::new(
        409,
        "process.card_type_mismatch",
        format!(
            "the card's type does not match the process's (process {} version {})",
            process.key, process.version
        ),
    )
}

/// Start a run of a published process on a card.
///
/// One transaction: card/board resolution, version + card-type checks, the queued run, its
/// materialised steps, the `queued → running` transition, the first step's job and both run events.
/// A caller-supplied `Idempotency-Key` replays the stored response instead of creating a second run;
/// without one, `uq_run_live` makes a second start on a live card a `409`.
#[crate::action(
    name = "process_runs.start",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
pub(crate) async fn process_runs_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessRunStartIn,
) -> Result<HandlerOutput<crate::control::RunOut>, ActionError> {
    // The tree edge is the parent's own step kind: a process parent names `step_id` (its
    // `process_step`), a workflow parent names `workflow_step_id` (its `run_step`). A root names
    // neither. Refusing the mixed cases here keeps the store's check a backstop (0037, W5).
    match (input.parent_run_id, input.step_id, input.workflow_step_id) {
        (None, None, None) | (Some(_), Some(_), None) | (Some(_), None, Some(_)) => {}
        (Some(_), Some(_), Some(_)) => {
            return Err(ActionError::new(
                422,
                "process.step_ambiguous",
                "a child names either step_id (a process parent) or workflow_step_id (a workflow \
                 parent), not both",
            ));
        }
        (Some(_), None, None) => {
            return Err(ActionError::new(
                422,
                "process.step_required",
                "a child must name the step that started it",
            ));
        }
        (None, Some(_), _) | (None, _, Some(_)) => {
            return Err(ActionError::new(
                422,
                "process.step_without_parent",
                "a step id is only meaningful with parent_run_id",
            ));
        }
    }
    let card = store::card::get_card_tx(&mut *tx, ctx.tenant, input.card_id)
        .await
        .map_err(|error| crate::control::store_error(error, ACTION))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.board_id)
        .await
        .map_err(|error| crate::control::store_error(error, ACTION))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    let process = resolve_version(&mut *tx, ctx.tenant, &input).await?;

    if let Some(card_type_id) = process.card_type_id
        && card.card_type_id != card_type_id
    {
        return Err(card_type_mismatch(&process));
    }

    let andon = store::andon::get_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = ACTION, "read andon state");
            ActionError::internal()
        })?;
    if andon.is_some_and(|state| state.engaged) {
        return Err(ActionError::new(
            409,
            "andon-engaged",
            "the tenant's andon cord is pulled; no run may be started while it is engaged",
        ));
    }

    let steps = store::process::list_steps_tx(&mut *tx, ctx.tenant, process.id)
        .await
        .map_err(|error| process_error(error, ACTION))?;
    // Publish refuses a step-less process (process.no_steps), so this is unreachable through the
    // API. Fail closed rather than commit a running run with nothing to execute.
    if steps.is_empty() {
        return Err(ActionError::new(
            409,
            "process.no_steps",
            "the published process has no steps; there is nothing to run",
        ));
    }

    // D22 / CONTRACT §17: resolve once, check readiness BEFORE anything is written, and record
    // the result on the run. Step 9 must not add a second start path; S15 fills the two functions.
    let start_context =
        resolve_start_context(&mut *tx, ctx.tenant, card.card_type_id, &process).await?;
    ensure_start_ready(&mut *tx, ctx.tenant, &start_context).await?;

    let attempt = store::run::next_attempt_for_card_tx(&mut *tx, ctx.tenant, card.id)
        .await
        .map_err(|error| crate::control::store_error(error, ACTION))?;
    let inputs = if input.inputs.is_null() {
        serde_json::json!({})
    } else {
        input.inputs.clone()
    };
    let new_run = store::run::NewRun {
        card_id: Some(card.id),
        project_id: board.project_id,
        attempt,
        correlation_id: None,
        process_id: Some(process.id),
        inputs,
        capability_id: start_context.capability_id(),
        capability_agent_id: start_context.capability_agent_id(),
        parent_run_id: input.parent_run_id,
        step_id: input.step_id,
        workflow_step_id: input.workflow_step_id,
        // `insert_child_run_tx` derives the depth from the parent; a root is 0.
        depth: 0,
        // The run pins the process version it executes (CONTRACT §21).
        process_version: process.version,
    };
    let run = match input.parent_run_id {
        Some(parent) => {
            store::run::insert_child_run_tx(&mut *tx, ctx.tenant, parent, &new_run).await
        }
        None => store::run::insert_run_tx(&mut *tx, ctx.tenant, &new_run).await,
    }
    .map_err(|error| crate::control::run_error(error, ACTION))?;

    // `run.created` carries the QUEUED representation, exactly as `runs.create` writes it — a
    // subscriber sees the same pair of events an ordinary run produces.
    let queued_record = read_record(tx, &ctx, run.id, ACTION).await?;
    let queued_out = run_out(&queued_record);
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
        serde_json::to_value(&queued_out).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    store::process::materialise_run_steps_tx(&mut *tx, ctx.tenant, run.id, &steps)
        .await
        .map_err(|error| process_error(error, ACTION))?;
    let started = store::run::transition_run_tx(&mut *tx, ctx.tenant, run.id, RunStatus::Running)
        .await
        .map_err(|error| crate::control::run_error(error, ACTION))?;
    store::process::enqueue_process_step_tx(&mut *tx, ctx.tenant, run.id, 1, 1)
        .await
        .map_err(|error| crate::control::store_error(error, ACTION))?;

    let record = read_record(tx, &ctx, started.id, ACTION).await?;
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

    Ok(HandlerOutput::created(out).with_etag(started.version.etag()))
}

/// The process version to run, by id or by published key.
///
/// A key resolves its PUBLISHED version: absent-but-present-in-another-status is a `409`
/// `process.not_published`, a key with no rows at all a `404`. An id may name any row; only a
/// published one runs.
async fn resolve_version(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &ProcessRunStartIn,
) -> Result<ProcessRecord, ActionError> {
    match (input.process_id, input.process_key.as_deref()) {
        (Some(id), None) => {
            let record = store::process::get_process_tx(conn, tenant, id)
                .await
                .map_err(|error| process_error(error, ACTION))?
                .ok_or_else(|| ActionError::resource_not_found("process"))?;
            if record.status != "published" {
                return Err(not_published());
            }
            Ok(record)
        }
        (None, Some(key)) => {
            let published = store::process::published_by_key_tx(conn, tenant, key)
                .await
                .map_err(|error| process_error(error, ACTION))?;
            match published {
                Some(record) => Ok(record),
                None => {
                    // Distinguish "this key exists as a draft/retired version" from "no such key".
                    let exists = store::process::get_process_by_key_tx(conn, tenant, key, None)
                        .await
                        .map_err(|error| process_error(error, ACTION))?
                        .is_some();
                    Err(if exists {
                        not_published()
                    } else {
                        ActionError::resource_not_found("process")
                    })
                }
            }
        }
        (Some(_), Some(_)) => Err(invalid(
            "process_key",
            "invalid_value",
            "name either process_id or process_key, not both",
        )),
        (None, None) => Err(invalid(
            "process_id",
            "required",
            "name a process_id or a process_key",
        )),
    }
}
