#![forbid(unsafe_code)]
//! The run-create-and-enqueue core, shared by `eval_runs.start` and `eval_climbs.round`
//! (S20; CONTRACT §20).
//!
//! It exists so a climb round does **not** need `evals:approve`. `eval_runs.start` requires
//! `evals:approve` because it is the caller who fixes what gets measured and at what cost; a
//! climb round runs under a plan a tenant-admin already signed, so it is `evals:write`. Copying
//! sixty lines of insert-widen-reopen-enqueue into the round handler would have given the estate
//! two enqueue paths that drift, and widening the round's scope to `evals:approve` would have
//! made the plan sign-off meaningless — so the core was extracted instead.
//!
//! Three behaviours are the ones a caller must keep:
//!
//! - **A start is a resume.** `eval_run_variant_unique (tenant, eval, variant)` means a second
//!   writer of the same variant widens the existing row rather than inserting a second one; the
//!   enqueue loop skips slots that are already filled, so a repeat call enqueues nothing.
//! - **A terminal run refuses.** `complete`, `partial`, `void` and `cancelled` are not resumable;
//!   the caller gets `409 run_terminal` rather than a silent no-op.
//! - **One `job` per `(case, rep)`**, `dedupe_key = <run>:<case>:<rep>`, enqueued inside the same
//!   transaction as the run row. A run that exists with no jobs waits forever.

use conveyor_domain::eval::{RunMode, RunStatus, Variant};
use conveyor_store as store;
use conveyor_store::eval::{NewRun, Run};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};

use super::{EVAL_RUN_CHANGED, emit, eval_error};

/// What to measure, and under which variant.
pub(crate) struct RunRequest<'a> {
    /// The eval the run measures.
    pub eval_id: Uuid,
    /// The frozen instrument.
    pub version_id: Uuid,
    /// `baseline | v<N>`, already parsed.
    pub variant: Variant,
    /// `pilot | full`.
    pub mode: RunMode,
    /// Repetitions per case (≥ 1).
    pub reps: i32,
    /// The cases to cover. Never empty — the caller refuses an empty set.
    pub case_ids: Vec<Uuid>,
    /// The climb round; `0` outside a climb.
    pub round: i32,
    /// The spend ceiling, when a climb sets one.
    pub cost_ceiling_cents: Option<i32>,
    /// Repo sha, model ids, stubs, flags.
    pub environment: &'a Value,
    /// Whether to enqueue the empty slots now.
    ///
    /// `false` is the `approve_each_round` path: the run row and its change exist so the approver
    /// can read what would run, but no job may execute until the round's gate is decided.
    pub enqueue: bool,
}

/// What the core did.
pub(crate) struct StartedRun {
    /// The run row, after the resume/widen/reopen step.
    pub run: Run,
    /// How many `eval.case.run` jobs this call enqueued (`0` on a repeat start).
    pub queued_jobs: usize,
    /// How many `(case, rep)` slots were already filled and therefore skipped.
    pub skipped_slots: usize,
}

/// Create, widen or reopen the run and enqueue its empty slots — in the caller's transaction.
///
/// The transaction is the caller's on purpose: a round writes its change and its prediction onto
/// the same run row, and a run that exists without them would be a round with no hypothesis.
pub(crate) async fn start_or_resume_run_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    request: &RunRequest<'_>,
) -> Result<StartedRun, ActionError> {
    let variant_str = request.variant.as_str();

    let mut run =
        match store::eval::find_run_tx(&mut *tx, ctx.tenant, request.eval_id, &variant_str)
            .await
            .map_err(|error| eval_error(error, ctx.action))?
        {
            Some(existing) if existing.status.is_terminal() => {
                return Err(ActionError::new(
                    409,
                    "run_terminal",
                    format!(
                        "{}: the {} run is {} and cannot be resumed",
                        ctx.action, variant_str, existing.status
                    ),
                ));
            }
            Some(existing) => store::eval::widen_run_tx(
                &mut *tx,
                ctx.tenant,
                existing.id,
                request.mode,
                request.reps,
                request.case_ids.len() as i32,
            )
            .await
            .map_err(|error| eval_error(error, ctx.action))?,
            None => {
                let new = NewRun {
                    eval_id: request.eval_id,
                    version_id: request.version_id,
                    variant: variant_str.as_str(),
                    round: request.round,
                    mode: request.mode.as_str(),
                    reps: request.reps,
                    case_count: request.case_ids.len() as i32,
                    environment: request.environment,
                    cost_ceiling_cents: request.cost_ceiling_cents,
                };
                store::eval::insert_run_tx(&mut *tx, ctx.tenant, &new)
                    .await
                    .map_err(|error| eval_error(error, ctx.action))?
            }
        };

    // A resume of a finished (or finished-with-gaps) run reopens it.
    if run.status == RunStatus::Complete || run.status == RunStatus::Partial {
        run = store::eval::transition_run_tx(&mut *tx, ctx.tenant, run.id, RunStatus::Running)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
    }

    let (queued_jobs, skipped_slots) = if request.enqueue {
        enqueue_missing_slots(tx, ctx, run.id, request).await?
    } else {
        (0, 0)
    };

    emit(
        tx,
        ctx,
        request.eval_id,
        run.id,
        run.status.as_str(),
        EVAL_RUN_CHANGED,
        json!({"status": run.status.as_str(), "variant": run.variant.as_str(),
               "queued_jobs": queued_jobs}),
    )
    .await?;

    Ok(StartedRun {
        run,
        queued_jobs,
        skipped_slots,
    })
}

/// Enqueue one job per empty slot, returning `(queued, skipped)`.
///
/// Split out so a test can prove the skip rule without a run row: the climb's resume path and the
/// start's resume path must skip identically, or the second climb round would re-run round 1's
/// cases.
pub(crate) async fn enqueue_missing_slots(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    run_id: Uuid,
    request: &RunRequest<'_>,
) -> Result<(usize, usize), ActionError> {
    let mut queued_jobs = 0_usize;
    let mut skipped_slots = 0_usize;
    for case_id in &request.case_ids {
        for rep in 0..request.reps {
            if store::eval::get_result_tx(&mut *tx, ctx.tenant, run_id, *case_id, rep)
                .await
                .map_err(|error| eval_error(error, ctx.action))?
                .is_some()
            {
                skipped_slots += 1;
                continue;
            }
            let dedupe = format!("{run_id}:{case_id}:{rep}");
            let payload = json!({
                "run_id": run_id,
                "case_id": case_id,
                "rep": rep,
            });
            let enqueued = store::jobs::enqueue_tx(
                &mut *tx,
                ctx.tenant,
                super::runs::CASE_JOB_KIND,
                &dedupe,
                payload,
                None,
                conveyor_domain::eval::RetryPolicy::default().max_attempts as i32,
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "enqueueing an eval case job failed");
                ActionError::internal()
            })?;
            if enqueued.is_some() {
                queued_jobs += 1;
            }
        }
    }
    Ok((queued_jobs, skipped_slots))
}

/// How many result rows are read per page when a run's means are computed.
const RESULT_PAGE: i64 = 500;

/// Every result row of one run, read to exhaustion.
///
/// One reader for the climb (means, spend, decisions) and the regrade (which slots exist), so the
/// two can never disagree about what a run contains.
pub(crate) async fn all_results(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    run_id: Uuid,
    action: &str,
) -> Result<Vec<store::eval::ResultRow>, ActionError> {
    let mut out = Vec::new();
    let mut cursor: Option<(Uuid, i32)> = None;
    loop {
        let page = store::eval::list_results_tx(
            &mut *tx,
            tenant,
            run_id,
            store::eval::ResultFilter {
                split: None,
                tag: None,
                status: None,
            },
            cursor,
            RESULT_PAGE,
        )
        .await
        .map_err(|error| eval_error(error, action))?;
        let short = (page.len() as i64) < RESULT_PAGE;
        if let Some(last) = page.last() {
            cursor = Some((last.case_id, last.rep));
        }
        out.extend(page);
        if short {
            break;
        }
    }
    Ok(out)
}

/// One source slot a regrade carries across.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RegradeSlot {
    /// The case in the **target** version — same ordinal, new id.
    pub case_id: Uuid,
    /// The case in the **source** version, where the stored transcript lives.
    pub source_case_id: Uuid,
    /// The repetition inside the run.
    pub rep: i32,
}

/// Enqueue one `eval.regrade` job per carried-over slot, skipping slots already filled.
///
/// Same dedupe shape as a case job — `<run_id>:<case_id>:<rep>` — so the tick's outstanding-count
/// (which keys on the run id prefix) sees both kinds, and a repeated regrade is a no-op rather
/// than a second queue entry.
pub(crate) async fn enqueue_regrade_slots(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    run_id: Uuid,
    source_run_id: Uuid,
    slots: &[RegradeSlot],
) -> Result<(usize, usize), ActionError> {
    let mut queued_jobs = 0_usize;
    let mut skipped_slots = 0_usize;
    for slot in slots {
        if store::eval::get_result_tx(&mut *tx, ctx.tenant, run_id, slot.case_id, slot.rep)
            .await
            .map_err(|error| eval_error(error, ctx.action))?
            .is_some()
        {
            skipped_slots += 1;
            continue;
        }
        let dedupe = format!("{run_id}:{}:{}", slot.case_id, slot.rep);
        let payload = json!({
            "run_id": run_id,
            "case_id": slot.case_id,
            "rep": slot.rep,
            "source_run_id": source_run_id,
            "source_case_id": slot.source_case_id,
        });
        let enqueued = store::jobs::enqueue_tx(
            &mut *tx,
            ctx.tenant,
            super::runs::REGRADE_JOB_KIND,
            &dedupe,
            payload,
            None,
            conveyor_domain::eval::RetryPolicy::default().max_attempts as i32,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "enqueueing an eval regrade job failed");
            ActionError::internal()
        })?;
        if enqueued.is_some() {
            queued_jobs += 1;
        }
    }
    Ok((queued_jobs, skipped_slots))
}
