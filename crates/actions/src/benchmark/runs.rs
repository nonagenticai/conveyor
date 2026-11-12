#![forbid(unsafe_code)]
//! `benchmark_suites.list` · `benchmark_runs.start|get|list|cancel|compare|promote`.
//!
//! `start` is the only writer's door: it resolves the suite and the selected instances, creates
//! the run and its `pending` cases, enqueues the first driver tick and appends the first
//! `benchmark_run.changed` — all in the caller's transaction. The sandbox check happens **before**
//! anything is written, so an unconfigured deployment cannot accumulate runs nothing can execute.

use conveyor_store as store;
use conveyor_store::actions::clamp_limit;
use conveyor_store::benchmark::NewRun;
use serde_json::json;
use sqlx::PgConnection;

use crate::benchmark::{
    CompareView, RunCompareIn, RunIdIn, RunListIn, RunStartIn, RunView, SuiteView, SuitesListIn,
    benchmark_error, compare_two_tx, decode_cursor, encode_cursor, invalid, run_view,
    subject_read_error, suite_view,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// The arms a run may name. `conveyor` exists in the schema and is refused until S18.
pub(crate) const ARMS: [&str; 4] = ["gold", "empty", "baseline", "conveyor"];

/// The job that produces one `conveyor` case's patch (S18 step 1).
///
/// One job per case, like S19's `eval.case.run`: `agent.run` is a bounded but *long* call, so it
/// can never run inline in the driver's tick (CONTRACT §19, "one bounded tick").
pub const PRODUCE_KIND: &str = "benchmark.case.produce";

/// The job that compares a candidate run against its baseline and records the verdict (S18 step 3).
pub const IMPROVEMENT_KIND: &str = "benchmark.improvement.run";
/// The statuses a run list may filter on.
const STATUSES: [&str; 5] = ["queued", "running", "succeeded", "failed", "cancelled"];
/// How many Jobs one driver tick may start by default (the old harness's cap was 5).
const DEFAULT_CONCURRENCY: u32 = 5;
/// The largest tick the sandbox quota can absorb.
const MAX_CONCURRENCY: u32 = 10;

/// **Fail-closed at the door** (CONTRACT §19): no namespace ⇒ `503`, before any row is written.
fn sandbox_configured() -> bool {
    std::env::var("SANDBOX_NAMESPACE")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// One page of suites, newest first.
#[crate::action(
    name = "benchmark_suites.list",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_suites_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SuitesListIn,
) -> Result<HandlerOutput<Page<SuiteView>>, ActionError> {
    const ACTION: &str = "benchmark_suites.list";
    let limit = clamp_limit(input.page.limit);
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let mut rows = store::benchmark::list_suites_tx(&mut *tx, ctx.tenant, cursor, limit + 1)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        rows.last().map(|row| encode_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(suite_view).collect(),
        next_cursor,
    }))
}

/// Start a run.
#[crate::action(
    name = "benchmark_runs.start",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunStartIn,
) -> Result<HandlerOutput<RunView>, ActionError> {
    const ACTION: &str = "benchmark_runs.start";

    if !ARMS.contains(&input.arm.as_str()) {
        return Err(invalid(
            "arm",
            "invalid_value",
            "arm must be gold, empty, baseline or conveyor",
        ));
    }
    if input.arm == "baseline" {
        // S17 declares the baseline arm in scope, but its producer (`baseline.py`: the pinned
        // system prompt, raw.githubusercontent source fetch, the edit contract and the difflib
        // patch writer) has no build step in the slice plan. Refusing here is honest; a run
        // that could never produce a patch would fail only in the driver.
        return Err(ActionError::new(
            409,
            "arm_not_ready",
            "the baseline arm's producer (the baseline.py port) is not built yet",
        ));
    }
    let subject_kind = input.subject_kind.as_deref().unwrap_or("suite");
    if !matches!(subject_kind, "suite" | "capability_agent" | "process") {
        return Err(invalid(
            "subject_kind",
            "invalid_value",
            "subject_kind must be suite, capability_agent or process",
        ));
    }
    // S18 §3: the run pins the executor version it measured. Resolved here, once, at start — never
    // joined at read time, where an edit would silently relabel a past measurement.
    let (subject_version, capability_agent_id, process_id) = match subject_kind {
        "suite" => {
            if input.arm == "conveyor" {
                return Err(invalid(
                    "subject_kind",
                    "invalid_value",
                    "the conveyor arm measures a capability agent or a process, never a suite",
                ));
            }
            if input.capability_agent_id.is_some() || input.process_id.is_some() {
                return Err(invalid(
                    "subject_kind",
                    "invalid_value",
                    "a suite subject carries no capability_agent_id or process_id",
                ));
            }
            (None, None, None)
        }
        "capability_agent" => {
            let id = input.capability_agent_id.ok_or_else(|| {
                invalid(
                    "capability_agent_id",
                    "required",
                    "a capability_agent subject must name it",
                )
            })?;
            let agent = store::capability::get_agent_tx(&mut *tx, ctx.tenant, id)
                .await
                .map_err(|error| subject_read_error(error, ACTION))?;
            // S18 §5: a missing or unready binding is `409 arm_not_ready` **before the first case**,
            // never a run whose every case fails at the producer.
            let Some(agent) = agent else {
                return Err(ActionError::new(
                    409,
                    "arm_not_ready",
                    "the capability agent is not in this tenant",
                ));
            };
            if agent.status != "active" {
                return Err(ActionError::new(
                    409,
                    "arm_not_ready",
                    format!(
                        "the capability agent is {}; only an active one can be measured",
                        agent.status
                    ),
                ));
            }
            (Some(agent.version), Some(id), None)
        }
        "process" => {
            // S18 §5 allows a `swebench-fix` process as the producer. A process run is
            // card-scoped (`process_runs.start` takes a card), and a benchmark instance is not a
            // card, so the process producer needs a mapping that does not exist. Refusing here
            // is honest and matches S17's `baseline` arm: a run that could never produce a patch
            // would fail only in the driver, one case at a time.
            return Err(ActionError::new(
                409,
                "arm_not_ready",
                "the process arm's producer is not built; a capability_agent subject is what \
                 this build can measure",
            ));
        }
        _ => unreachable!("validated above"),
    };
    let concurrency = input.concurrency.unwrap_or(DEFAULT_CONCURRENCY);
    if !(1..=MAX_CONCURRENCY).contains(&concurrency) {
        return Err(invalid(
            "concurrency",
            "invalid_value",
            format!("concurrency must be between 1 and {MAX_CONCURRENCY}"),
        ));
    }
    if !sandbox_configured() {
        return Err(ActionError::new(
            503,
            "benchmark_sandbox_not_configured",
            "SANDBOX_NAMESPACE is unset, so no case could start; configure the sandbox first",
        ));
    }
    let label = input.label.trim();
    if label.is_empty() {
        return Err(invalid("label", "required", "must not be empty"));
    }

    let suite = store::benchmark::get_suite_tx(&mut *tx, ctx.tenant, input.suite_id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    let instances = store::benchmark::producer_instances_tx(
        &mut *tx,
        ctx.tenant,
        suite.id,
        input.only.as_deref(),
    )
    .await
    .map_err(|error| benchmark_error(error, ACTION))?;
    if let Some(only) = input.only.as_deref()
        && instances.len() != only.len()
    {
        return Err(ActionError::new(
            422,
            "benchmark_invalid",
            "only names an instance that is not in this suite",
        ));
    }
    if instances.is_empty() {
        return Err(invalid(
            "only",
            "invalid_value",
            "the run would measure no instance",
        ));
    }
    let instance_ids: Vec<String> = instances
        .iter()
        .map(|instance| instance.instance_id.clone())
        .collect();
    let options = json!({
        "concurrency": concurrency,
        "only": input.only,
        "with_failing_test": input.with_failing_test,
    });

    let run = store::benchmark::create_run_tx(
        &mut *tx,
        ctx.tenant,
        &NewRun {
            suite_id: suite.id,
            arm: &input.arm,
            subject_kind,
            capability_agent_id,
            process_id,
            subject_version,
            label,
            options: &options,
            instance_ids: &instance_ids,
        },
    )
    .await
    .map_err(|error| benchmark_error(error, ACTION))?;

    // S18 step 3: a candidate started against a baseline is the improvement loop's one entry
    // point. The job records `keep | revert | inconclusive` on the candidate run itself; it never
    // starts a second candidate (S18 §10 Q4) and never promotes (Q2 keeps that a person's act).
    if let Some(baseline_run_id) = input.baseline_run_id {
        let baseline = store::benchmark::get_run_tx(&mut *tx, ctx.tenant, baseline_run_id)
            .await
            .map_err(|error| benchmark_error(error, ACTION))?;
        if baseline.suite_id != suite.id {
            return Err(ActionError::new(
                422,
                "benchmark_invalid",
                "the baseline run measured a different suite",
            ));
        }
        store::jobs::enqueue_tx(
            &mut *tx,
            ctx.tenant,
            IMPROVEMENT_KIND,
            &format!("benchmark:improvement:{}", run.id),
            json!({"run_id": run.id, "baseline_run_id": baseline_run_id}),
            None,
            3,
        )
        .await
        .map_err(|error| subject_read_error(error, ACTION))?;
    }

    store::benchmark::enqueue_poll_tx(
        &mut *tx,
        ctx.tenant,
        run.id,
        &format!("benchmark:{}", run.id),
        None,
    )
    .await
    .map_err(|error| benchmark_error(error, ACTION))?;
    store::benchmark::append_run_event_tx(&mut *tx, ctx.tenant, &run, "benchmark_run.changed")
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;

    Ok(HandlerOutput::created(run_view(&run)))
}

/// One run.
#[crate::action(
    name = "benchmark_runs.get",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunIdIn,
) -> Result<HandlerOutput<RunView>, ActionError> {
    const ACTION: &str = "benchmark_runs.get";
    let run = store::benchmark::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    Ok(HandlerOutput::ok(run_view(&run)))
}

/// One page of runs, newest first.
#[crate::action(
    name = "benchmark_runs.list",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunListIn,
) -> Result<HandlerOutput<Page<RunView>>, ActionError> {
    const ACTION: &str = "benchmark_runs.list";
    if let Some(arm) = input.arm.as_deref()
        && !ARMS.contains(&arm)
    {
        return Err(invalid(
            "arm",
            "invalid_value",
            "arm must be gold, empty, baseline or conveyor",
        ));
    }
    if let Some(status) = input.status.as_deref()
        && !STATUSES.contains(&status)
    {
        return Err(invalid(
            "status",
            "invalid_value",
            "status must be queued, running, succeeded, failed or cancelled",
        ));
    }
    let limit = clamp_limit(input.page.limit);
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let mut rows = store::benchmark::list_runs_tx(
        &mut *tx,
        ctx.tenant,
        input.suite_id,
        input.arm.as_deref(),
        input.status.as_deref(),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| benchmark_error(error, ACTION))?;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        rows.last().map(|row| encode_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(run_view).collect(),
        next_cursor,
    }))
}

/// Request a cancel.
#[crate::action(
    name = "benchmark_runs.cancel",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_cancel(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunIdIn,
) -> Result<HandlerOutput<RunView>, ActionError> {
    const ACTION: &str = "benchmark_runs.cancel";
    let run = store::benchmark::cancel_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    store::benchmark::append_run_event_tx(&mut *tx, ctx.tenant, &run, "benchmark_run.changed")
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    Ok(HandlerOutput::ok(run_view(&run)))
}

/// Compare two runs with the exact two-sided McNemar test.
#[crate::action(
    name = "benchmark_runs.compare",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_compare(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunCompareIn,
) -> Result<HandlerOutput<CompareView>, ActionError> {
    const ACTION: &str = "benchmark_runs.compare";
    let a = store::benchmark::get_run_tx(&mut *tx, ctx.tenant, input.a_run_id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    let b = store::benchmark::get_run_tx(&mut *tx, ctx.tenant, input.b_run_id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    let view = compare_two_tx(&mut *tx, ctx.tenant, &a, &b, ACTION).await?;
    Ok(HandlerOutput::ok(view))
}

/// Promote a run as the suite's baseline (S18 reads it; S17 stores it).
#[crate::action(
    name = "benchmark_runs.promote",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_runs_promote(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunIdIn,
) -> Result<HandlerOutput<SuiteView>, ActionError> {
    const ACTION: &str = "benchmark_runs.promote";
    let run = store::benchmark::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    let suite = store::benchmark::promote_baseline_tx(&mut *tx, ctx.tenant, run.suite_id, run.id)
        .await
        .map_err(|error| benchmark_error(error, ACTION))?;
    Ok(HandlerOutput::ok(suite_view(&suite)))
}
