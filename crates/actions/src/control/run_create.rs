#![forbid(unsafe_code)]
//! The one run-creation path for a card's root run.
//!
//! Two entry points create a bound card run — `runs.create` (an explicit start) and
//! `cards.assign` (the handoff) — and both commit the same four facts in the caller's
//! transaction: the andon gate, the card's next attempt, the `queued` row with its `run.execute`
//! job, and the `run.created` event. Keeping them here means a handoff cannot drift from the
//! cord, the binding's pin or the event.

use conveyor_domain::Card;
use conveyor_store as store;
use conveyor_store::run::{NewRun, RunRecord};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::capability::readiness::ensure_run_start_ready_tx;
use crate::invoke::{ActionError, HandlerCtx, ensure_binding_budget};

use super::runs::{emit_run_event, read_record, run_out};
use super::views::RunOut;
use super::{run_error, store_error};

/// Refuse while the tenant's andon cord is engaged.
///
/// Public inside the crate so a composing action (`cards.assign`) can take the refusal before
/// its own first write; the run creation below takes it again in the same transaction.
pub(crate) async fn ensure_andon_tx(
    conn: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    action: &str,
) -> Result<(), ActionError> {
    let andon = store::andon::get_tx(&mut *conn, tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, action, "read andon state");
            ActionError::internal()
        })?;
    if andon.is_some_and(|state| state.engaged) {
        return Err(ActionError::new(
            409,
            "andon-engaged",
            "the tenant's andon cord is pulled; no run may be created while it is engaged",
        ));
    }
    Ok(())
}

/// Insert the card's next root run, enqueue its driving job and announce it.
///
/// `job_binding_id` is already resolved by the caller: `runs.create` applies the card-binding
/// mismatch rule, and `cards.assign` validated the assignment. The store still refuses a retired
/// binding or a stale pin (`409`) before the INSERT, and `uq_run_live` refuses a second live run.
/// The run-specific fields the two creation paths (`runs.create`, `cards.assign`) pass to
/// [`create_bound_run_tx`].
pub(crate) struct BoundRun {
    /// The organisation binding the run executes under.
    pub job_binding_id: Option<Uuid>,
    /// Links the run to the trigger that created it.
    pub correlation_id: Option<Uuid>,
    /// The release pin the caller expects.
    pub release_hash: Option<String>,
    /// The run's own inputs; `limits.cost_budget_usd` is the spend ceiling (D45).
    pub inputs: serde_json::Value,
}

pub(crate) async fn create_bound_run_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card: &Card,
    run: BoundRun,
    action: &str,
) -> Result<(RunRecord, RunOut), ActionError> {
    let BoundRun {
        job_binding_id,
        correlation_id,
        release_hash,
        inputs,
    } = run;
    // An absent `inputs` is an empty object, never a SQL NULL: the column is jsonb and the
    // budget guard reads `inputs.limits`.
    let inputs = if inputs.is_null() {
        serde_json::json!({})
    } else {
        inputs
    };
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.board_id)
        .await
        .map_err(|error| store_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    ensure_andon_tx(&mut *tx, ctx.tenant, action).await?;

    // S15's run wiring: a plain run executes under the capability its card's type binds — the
    // same resolution `process_runs.start` uses. A card type that binds nothing is the
    // explicit-input path. The shared readiness guard runs before the run row, so a required
    // pack nobody published refuses `409 capability_not_ready` with zero run rows.
    let capability =
        store::capability::capability_for_card_type_tx(&mut *tx, ctx.tenant, card.card_type_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, action, "resolve the card type's capability");
                ActionError::internal()
            })?;
    let (capability_id, mut capability_agent_id) = match capability {
        Some(capability) => {
            let agents =
                store::capability::list_agents_tx(&mut *tx, ctx.tenant, Some(capability.id), 100)
                    .await
                    .map_err(|error| {
                        tracing::error!(%error, action, "list the capability's executors");
                        ActionError::internal()
                    })?;
            let active: Vec<_> = agents
                .into_iter()
                .filter(|agent| agent.status == "active")
                .collect();
            let agent_id = (active.len() == 1).then(|| active[0].id);
            (Some(capability.id), agent_id)
        }
        None => (None, None),
    };
    let bound_agents: Vec<Uuid> = capability_agent_id.into_iter().collect();
    ensure_run_start_ready_tx(&mut *tx, ctx.tenant, capability_id, &bound_agents, action).await?;

    let attempt = store::run::next_attempt_for_card_tx(&mut *tx, ctx.tenant, card.id)
        .await
        .map_err(|error| store_error(error, action))?;

    // v1-w1a-5 (SPEC_V2 §4.4): the kind comes from the run's claim. A request that names (or
    // inherits) a binding, or claims a release pin, is a job run; a plain request with neither is
    // the typed project run and keeps the plain-run path. A release claim with no binding is a
    // job run with no pin, which the store refuses `422 binding_required` before the INSERT.
    let kind = if job_binding_id.is_some() || release_hash.is_some() {
        store::run::RunKind::Job
    } else {
        store::run::RunKind::Project
    };

    // v1-p44-1 (SPEC_V2 §4.4): a ROOT run's executor is the agent its binding pins.
    if let Some(binding_id) = job_binding_id {
        let binding = store::organisation::job_binding_get_tx(&mut *tx, ctx.tenant, binding_id)
            .await
            .map_err(|error| run_error(store::RunStoreError::from(error), action))?;
        capability_agent_id = binding.capability_agent_id;
        // #526 / D45: an internal-agent binding must itself declare the ceiling, or the run
        // must declare one; the run's declaration is the `inputs` this caller passed, never
        // `Null` (which made every run look undeclared).
        ensure_binding_budget(&mut *tx, ctx.tenant, binding_id, &inputs).await?;
    }

    let run = store::run::insert_run_tx(
        &mut *tx,
        ctx.tenant,
        &NewRun {
            card_id: Some(card.id),
            project_id: board.project_id,
            attempt,
            correlation_id,
            process_id: None,
            inputs,
            capability_id,
            capability_agent_id,
            // A root run has no parent, no step and pins no process.
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
            depth: 0,
            process_version: 1,
            kind,
            job_binding_id,
            release_hash,
        },
    )
    .await
    .map_err(|error| run_error(error, action))?;

    store::run::enqueue_run_execute_tx(&mut *tx, ctx.tenant, run.id)
        .await
        .map_err(|error| store_error(error, action))?;

    let record = read_record(tx, ctx, run.id, action).await?;
    let out = run_out(&record);
    emit_run_event(
        &mut *tx,
        ctx,
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

    Ok((record, out))
}
