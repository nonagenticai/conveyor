#![forbid(unsafe_code)]
//! `eval_runs.*` — start, read, list and cancel a measurement (S19 Part B step 10;
//! CONTRACT §20).
//!
//! `start` is where the instrument's two sign-offs become load-bearing: it calls the same
//! readiness predicate Part A built and refuses with `409 eval_not_ready` naming **every** gate
//! that is missing, so a caller is never told one reason, fixes it, and is told the next.
//!
//! **A start is a resume.** `eval_run_variant_unique` is `(tenant, eval, variant)`, so a second
//! start on the same variant — the full pass that supersedes a pilot — widens the existing row
//! and enqueues only the slots that are still empty. The pilot's results are kept, which is what
//! the slice means by "superseded, never deleted".
//!
//! **One `job` per `(case, rep)`**, `dedupe_key = <run_id>:<case_id>:<rep>`, enqueued in the
//! same transaction as the run row: a run that exists with no jobs would wait forever, and the
//! `job` table's own `ON CONFLICT … WHERE status IN ('pending','running')` makes a repeated
//! start a no-op rather than a second queue entry.

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::eval::{Readiness, RunMode, RunStatus, SubjectKind, Variant};
use conveyor_store as store;
use conveyor_store::eval::Run;
use conveyor_store::eval::climb as climb_store;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

use super::{EVAL_RUN_CHANGED, emit, eval_error, not_ready};

/// The job kind a case runs under. One per `(case, rep)`, never one per run.
pub const CASE_JOB_KIND: &str = "eval.case.run";

/// The job kind a stored slot is re-graded under. One per stored `(case, rep)` of the source.
pub const REGRADE_JOB_KIND: &str = "eval.regrade";

/// How many cases a single start may cover. The slice's case sets are ≤ 100; the cap is what
/// stops a mis-specified `case_ids` from enqueueing a hundred thousand jobs.
const START_CASE_CAP: i64 = 1_000;

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `eval_runs.start` — what to measure and how hard.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunStartIn {
    /// The eval to measure; its current version is the instrument.
    pub eval_id: Uuid,
    /// The version to run; absent means the eval's current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<Uuid>,
    /// The report's column: `baseline` (default) or `v<N>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Repetitions per case, from 1. Default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reps: Option<i32>,
    /// `pilot | full`; default `full`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Only these cases. Absent means every case in the version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_ids: Option<Vec<Uuid>>,
    /// The spend ceiling a climb sets; recorded, never enforced by this action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_ceiling_cents: Option<i32>,
}

/// `eval_runs.get` / `eval_runs.cancel` — one run by id.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunIdIn {
    /// The run id, resolved in the caller's tenant.
    pub id: Uuid,
}

/// `eval_runs.list` — the filters plus the shared page envelope.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunListIn {
    /// Only runs of this eval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_id: Option<Uuid>,
    /// Only runs in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One eval run as every run action returns it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunView {
    /// The run id.
    pub id: String,
    /// The eval it measures.
    pub eval_id: String,
    /// The frozen instrument.
    pub version_id: String,
    /// `baseline | v<N>`.
    pub variant: String,
    /// The climb round; `0` outside a climb.
    pub round: i32,
    /// `pilot | full`.
    pub mode: String,
    /// Repetitions per case.
    pub reps: i32,
    /// How many cases the run covers.
    pub case_count: i32,
    /// `queued | running | partial | complete | void | cancelled`.
    pub status: String,
    /// The spend ceiling a climb set.
    pub cost_ceiling_cents: Option<i32>,
    /// When the tick last heartbeat the run.
    pub last_polled_at: Option<DateTime<Utc>>,
    /// When the first case began.
    pub started_at: Option<DateTime<Utc>>,
    /// When the work finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
}

/// `eval_runs.start`'s output: the run and how many jobs this call actually queued.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunStartOut {
    /// The run row (created or resumed).
    pub run: RunView,
    /// How many `eval.case.run` jobs this call enqueued; `0` on a repeat start.
    pub queued_jobs: usize,
    /// How many `(case, rep)` slots already held a result and were therefore skipped.
    pub skipped_slots: usize,
}

fn run_view(run: &Run) -> RunView {
    RunView {
        id: run.id.to_string(),
        eval_id: run.eval_id.to_string(),
        version_id: run.version_id.to_string(),
        variant: run.variant.as_str(),
        round: run.round,
        mode: run.mode.as_str().to_owned(),
        reps: run.reps,
        case_count: run.case_count,
        status: run.status.as_str().to_owned(),
        cost_ceiling_cents: run.cost_ceiling_cents,
        last_polled_at: run.last_polled_at,
        started_at: run.started_at,
        finished_at: run.finished_at,
        created_at: run.created_at,
    }
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Start (or resume) a measurement, refusing until both sign-offs hold.
#[crate::action(
    name = "eval_runs.start",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunStartIn,
) -> Result<HandlerOutput<RunStartOut>, ActionError> {
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    if eval.subject_kind == SubjectKind::AgentRun {
        // Declared so the vocabulary is stable; no action executes one yet (S13/S15).
        return Err(ActionError::new(
            409,
            "subject_not_ready",
            format!(
                "{}: an agent_run subject has no executing action yet; use completions, copilot, \
                 process or external",
                ctx.action
            ),
        ));
    }

    let version_id = input
        .version_id
        .or(eval.current_version_id)
        .ok_or_else(|| not_ready(ctx.action, "the eval has no version to run"))?;

    let facts = store::eval::eval_readiness_tx(&mut *tx, ctx.tenant, eval.id, version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let readiness = Readiness::evaluate(facts);
    if !readiness.is_ready() {
        let gates: Vec<&str> = readiness
            .missing()
            .iter()
            .map(|gate| gate.as_str())
            .collect();
        return Err(ActionError::new(
            409,
            "eval_not_ready",
            format!(
                "{}: the run is refused until every gate is satisfied: {}",
                ctx.action,
                gates.join(", ")
            ),
        ));
    }

    let variant = match input.variant.as_deref() {
        None => Variant::Baseline,
        Some(raw) => Variant::parse(raw).map_err(|_| {
            invalid(
                "variant",
                "invalid_value",
                "a variant is `baseline` or `v<N>`",
            )
        })?,
    };
    let mode = match input.mode.as_deref() {
        None => RunMode::Full,
        Some(raw) => RunMode::from_str(raw)
            .map_err(|_| invalid("mode", "invalid_value", "`mode` is `pilot` or `full`"))?,
    };
    let reps = input.reps.unwrap_or(1);
    if reps < 1 {
        return Err(invalid(
            "reps",
            "invalid_value",
            "`reps` must be at least 1",
        ));
    }

    // The case set: the caller's list, or every case in the version.
    let case_ids = match input.case_ids {
        Some(ids) if !ids.is_empty() => {
            if ids.len() as i64 > START_CASE_CAP {
                return Err(invalid(
                    "case_ids",
                    "invalid_value",
                    format!("a single start may cover at most {START_CASE_CAP} cases"),
                ));
            }
            ids
        }
        _ => all_cases(tx, ctx.tenant, version_id).await?,
    };
    if case_ids.is_empty() {
        return Err(not_ready(
            ctx.action,
            "the version has no cases, so there is nothing to measure",
        ));
    }

    let environment = json!({
        "model": eval.model_ref,
        "prompt": eval.prompt_ref,
        "entry_action": eval.entry_action,
        "trace_source": eval.trace_source,
    });

    let started = super::run_core::start_or_resume_run_tx(
        tx,
        &ctx,
        &super::run_core::RunRequest {
            eval_id: eval.id,
            version_id,
            variant,
            mode,
            reps,
            case_ids,
            round: 0,
            cost_ceiling_cents: input.cost_ceiling_cents,
            environment: &environment,
            enqueue: true,
        },
    )
    .await?;

    Ok(HandlerOutput::created(RunStartOut {
        run: run_view(&started.run),
        queued_jobs: started.queued_jobs,
        skipped_slots: started.skipped_slots,
    }))
}

/// One run by id.
#[crate::action(
    name = "eval_runs.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunIdIn,
) -> Result<RunView, ActionError> {
    let run = store::eval::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    Ok(run_view(&run))
}

/// One page of runs, newest first.
#[crate::action(
    name = "eval_runs.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunListIn,
) -> Result<Page<RunView>, ActionError> {
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);
    let mut rows = store::eval::list_runs_tx(
        &mut *tx,
        ctx.tenant,
        input.eval_id,
        input.status.as_deref(),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|run| format!("{}:{}", run.created_at.timestamp_micros(), run.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(run_view).collect(),
        next_cursor,
    })
}

/// Cancel a run. The driver stops writing; the jobs already queued drain as no-ops.
#[crate::action(
    name = "eval_runs.cancel",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_cancel(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunIdIn,
) -> Result<RunView, ActionError> {
    let run = store::eval::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let run = store::eval::transition_run_tx(&mut *tx, ctx.tenant, run.id, RunStatus::Cancelled)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    emit(
        tx,
        &ctx,
        run.eval_id,
        run.id,
        run.status.as_str(),
        EVAL_RUN_CHANGED,
        json!({"status": run.status.as_str(), "variant": run.variant.as_str()}),
    )
    .await?;
    Ok(run_view(&run))
}

// ── regrade ─────────────────────────────────────────────────────────────────────────────

/// `eval_runs.regrade` — re-grade a finished run's stored outputs under the eval's current
/// version.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RegradeIn {
    /// The finished run whose stored outputs are re-graded.
    pub source_run_id: Uuid,
    /// Why, and what changed in the instrument. Recorded on the new run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_md: Option<String>,
}

/// One source slot the regrade could not carry across, with the reason.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NotRegradable {
    /// The case's ordinal in the source version (`None` when the source case no longer exists).
    pub ordinal: Option<i32>,
    /// `ordinal_missing | input_changed | source_case_missing`.
    pub reason: String,
}

/// What a regrade created.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RegradeOut {
    /// The new run, under the eval's current version.
    pub run: RunView,
    /// `v<N>` — a new variant, because `eval_run_variant_unique` is per `(eval, variant)`.
    pub variant: String,
    /// How many stored slots were carried across and enqueued.
    pub regradable_slots: usize,
    /// The stored slots that were **not** carried across. Counted, never silently dropped.
    pub not_regradable: Vec<NotRegradable>,
    /// How many `eval.regrade` jobs this call enqueued (`0` when a repeat resumes a full run).
    pub queued_jobs: usize,
}

/// Re-grade a finished run's stored outputs under the eval's current version.
///
/// Three rules are structural:
///
/// - **It never re-invokes the subject.** The output is the one the source transcript captured;
///   the only new spend is a judge call, and the subject's `usage` is not copied (it was billed
///   on the source run, and climb spend is summed from these rows).
/// - **A new grade is always a new run.** Rewriting a result in place would make two scales
///   comparable, which is the trap `eval_climbs.compare` exists to refuse.
/// - **Slots are matched by `eval_case.ordinal`.** `eval_versions.create` clones cases with new
///   ids and the same ordinals, so the id is not the identity across versions. An ordinal whose
///   input changed is counted as `not_regradable`, never graded against the wrong case.
#[crate::action(
    name = "eval_runs.regrade",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_regrade(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RegradeIn,
) -> Result<HandlerOutput<RegradeOut>, ActionError> {
    let source = store::eval::get_run_tx(&mut *tx, ctx.tenant, input.source_run_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    if !source.status.finished() {
        return Err(ActionError::new(
            409,
            "source_run_open",
            format!(
                "{}: the source run is {} — only a finished run has stored outputs to re-grade",
                ctx.action, source.status
            ),
        ));
    }
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, source.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let Some(version_id) = eval.current_version_id else {
        return Err(not_ready(ctx.action, "the eval has no current version"));
    };
    if version_id == source.version_id {
        return Err(ActionError::new(
            409,
            "version_unchanged",
            format!(
                "{}: the eval's current version is the one the source run already used — a \
                 regrade carries stored outputs onto a NEW instrument, so create the version \
                 that carries the grader change first",
                ctx.action
            ),
        ));
    }

    // Cases by position, not by id: `eval_versions.create` gives every case a new id and keeps
    // its ordinal, so ordinal is the only identity that survives a version.
    let source_cases = cases_of(tx, ctx.tenant, source.version_id, ctx.action).await?;
    let target_cases = cases_of(tx, ctx.tenant, version_id, ctx.action).await?;
    let mut source_by_id: HashMap<Uuid, &store::eval::EvalCase> = HashMap::new();
    for case in &source_cases {
        source_by_id.insert(case.id, case);
    }
    let mut target_by_ordinal: HashMap<i32, &store::eval::EvalCase> = HashMap::new();
    for case in &target_cases {
        target_by_ordinal.insert(case.ordinal, case);
    }

    let mut slots: Vec<super::run_core::RegradeSlot> = Vec::new();
    let mut not_regradable: Vec<NotRegradable> = Vec::new();
    for result in super::run_core::all_results(tx, ctx.tenant, source.id, ctx.action).await? {
        let Some(source_case) = source_by_id.get(&result.case_id) else {
            not_regradable.push(NotRegradable {
                ordinal: None,
                reason: "source_case_missing".to_owned(),
            });
            continue;
        };
        let Some(target_case) = target_by_ordinal.get(&source_case.ordinal) else {
            not_regradable.push(NotRegradable {
                ordinal: Some(source_case.ordinal),
                reason: "ordinal_missing".to_owned(),
            });
            continue;
        };
        if target_case.input != source_case.input {
            not_regradable.push(NotRegradable {
                ordinal: Some(source_case.ordinal),
                reason: "input_changed".to_owned(),
            });
            continue;
        }
        slots.push(super::run_core::RegradeSlot {
            case_id: target_case.id,
            source_case_id: source_case.id,
            rep: result.rep,
        });
    }
    if slots.is_empty() {
        return Err(ActionError::new(
            409,
            "nothing_to_regrade",
            format!(
                "{}: none of the source run's stored slots maps onto the current version \
                 ({} skipped)",
                ctx.action,
                not_regradable.len()
            ),
        ));
    }

    // A repeat resumes the run it made before; only a first call mints a variant.
    let variant =
        match store::eval::find_regrade_tx(&mut *tx, ctx.tenant, eval.id, source.id, version_id)
            .await
            .map_err(|error| eval_error(error, ctx.action))?
        {
            Some(run) => run.variant,
            None => {
                let next = climb_store::next_variant_tx(&mut *tx, ctx.tenant, eval.id)
                    .await
                    .map_err(|error| eval_error(error, ctx.action))?;
                Variant::Numbered(u32::try_from(next).map_err(|_| {
                    ActionError::new(409, "variant_exhausted", "the eval has no variant left")
                })?)
            }
        };

    let environment = json!({
        "regrade": {
            "from_run": source.id,
            "from_version": source.version_id,
        },
        "model": eval.model_ref,
        "prompt": eval.prompt_ref,
        "entry_action": eval.entry_action,
        "trace_source": eval.trace_source,
    });
    let case_ids: Vec<Uuid> = slots.iter().map(|slot| slot.case_id).collect();
    let started = super::run_core::start_or_resume_run_tx(
        tx,
        &ctx,
        &super::run_core::RunRequest {
            eval_id: eval.id,
            version_id,
            variant,
            mode: source.mode,
            reps: source.reps,
            case_ids,
            round: 0,
            cost_ceiling_cents: None,
            environment: &environment,
            // The jobs are `eval.regrade`, not `eval.case.run`, so the shared case enqueue must
            // not run: it would queue a pipeline call for every slot.
            enqueue: false,
        },
    )
    .await?;

    let summary = json!({
        "regraded_from": source.id,
        "source_version_id": source.version_id,
        "change_md": input.change_md,
        "not_regradable": not_regradable,
    });
    climb_store::set_run_round_tx(
        &mut *tx,
        ctx.tenant,
        started.run.id,
        0,
        input.change_md.as_deref().unwrap_or(""),
        None,
        &summary,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    let (queued_jobs, _skipped) =
        super::run_core::enqueue_regrade_slots(tx, &ctx, started.run.id, source.id, &slots).await?;

    let run = store::eval::get_run_tx(&mut *tx, ctx.tenant, started.run.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    Ok(HandlerOutput::created(RegradeOut {
        variant: run.variant.as_str(),
        regradable_slots: slots.len(),
        not_regradable,
        queued_jobs,
        run: run_view(&run),
    }))
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────

/// Every case of a version, read to exhaustion.
async fn cases_of(
    tx: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
    action: &str,
) -> Result<Vec<store::eval::EvalCase>, ActionError> {
    let mut out = Vec::new();
    let mut cursor: Option<(i32, Uuid)> = None;
    loop {
        let page = store::eval::list_cases_tx(&mut *tx, tenant, version_id, cursor, 200)
            .await
            .map_err(|error| eval_error(error, action))?;
        let short = page.len() < 200;
        if let Some(last) = page.last() {
            cursor = Some((last.ordinal, last.id));
        }
        out.extend(page);
        if short {
            break;
        }
    }
    Ok(out)
}

/// Every case id in a version, or `409 eval_not_ready` when the list cannot be read whole.
async fn all_cases(
    tx: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
) -> Result<Vec<Uuid>, ActionError> {
    let mut ids = Vec::new();
    let mut cursor: Option<(i32, Uuid)> = None;
    loop {
        let page = store::eval::list_cases_tx(&mut *tx, tenant, version_id, cursor, 200)
            .await
            .map_err(|error| eval_error(error, "eval_runs.start"))?;
        if page.is_empty() {
            break;
        }
        cursor = page.last().map(|case| (case.ordinal, case.id));
        ids.extend(page.into_iter().map(|case| case.id));
        if ids.len() as i64 > START_CASE_CAP {
            return Err(invalid(
                "case_ids",
                "invalid_value",
                format!("this version has more than {START_CASE_CAP} cases; name a subset"),
            ));
        }
    }
    Ok(ids)
}

/// Decode `eval_runs.list`'s opaque cursor: `<epoch micros>:<uuid>`.
fn decode_cursor(raw: Option<&str>) -> Result<Option<(DateTime<Utc>, Uuid)>, ActionError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let (micros, id) = raw
        .split_once(':')
        .ok_or_else(|| invalid("cursor", "invalid_value", "the cursor is malformed"))?;
    let micros: i64 = micros.parse().map_err(|_| {
        invalid(
            "cursor",
            "invalid_value",
            "the cursor's timestamp is malformed",
        )
    })?;
    let id = Uuid::from_str(id)
        .map_err(|_| invalid("cursor", "invalid_value", "the cursor's id is malformed"))?;
    let at = DateTime::<Utc>::from_timestamp_micros(micros).ok_or_else(|| {
        invalid(
            "cursor",
            "invalid_value",
            "the cursor's timestamp is out of range",
        )
    })?;
    Ok(Some((at, id)))
}
