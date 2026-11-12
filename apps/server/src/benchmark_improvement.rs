#![forbid(unsafe_code)]
//! `benchmark.improvement.run` — the kata loop's verdict (S18 step 3).
//!
//! **The loop is a consumer of the action registry, not a second runtime** (S18 §9.1). It calls
//! `benchmark_runs.get` to observe the candidate and `benchmark_runs.compare` to judge it — the
//! same two actions a person reading screen 14 would call — and records `keep | revert |
//! inconclusive` on the candidate run. It never starts a second candidate (S18 §10 Q4) and never
//! promotes (Q2: that stays a person's act).
//!
//! **The rule is the slice's** (S18 §10 Q3): a verdict is `keep` or `revert` only when the two runs
//! are comparable, both controls passed, and the exact McNemar p is below 0.05; anything else is
//! `inconclusive` with the reason recorded. An inconclusive verdict is data, not a failure.

use chrono::{Duration as ChronoDuration, Utc};
use conveyor_actions::invoke::{Authority, InvokeRequest, invoke};
use conveyor_actions::registry::registry;
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// How often a live candidate is re-checked.
const TICK: ChronoDuration = ChronoDuration::seconds(60);
/// The significance bar (S18 §10 Q3).
const ALPHA: f64 = 0.05;

fn want_uuid(payload: &Value, field: &str) -> Result<Uuid, JobFailure> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent(format!("the job payload has no {field}")))
}

/// One call of the loop: observe, or compare and record.
pub async fn run(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    payload: &Value,
) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id")?;
    let baseline_run_id = want_uuid(payload, "baseline_run_id")?;

    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let candidate = store::benchmark::get_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    if candidate.improvement_verdict.is_some() {
        return Ok(()); // already judged; a re-delivered job writes nothing
    }
    if !matches!(
        candidate.status.as_str(),
        "succeeded" | "failed" | "cancelled"
    ) {
        // Still measuring: schedule the next look. The key carries the tick so this job's own
        // `running` row does not swallow the next one.
        store::jobs::enqueue_tx(
            &mut tx,
            tenant,
            conveyor_actions::benchmark::runs::IMPROVEMENT_KIND,
            &format!(
                "benchmark:improvement:{run_id}:{}",
                Utc::now().timestamp_millis()
            ),
            json!({"run_id": run_id, "baseline_run_id": baseline_run_id}),
            Some(Utc::now() + TICK),
            3,
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    // A run that never measured anything has nothing to keep or revert.
    if candidate.status != "succeeded" {
        let note = format!(
            "the candidate run {}: {}",
            candidate.status,
            candidate.error.as_deref().unwrap_or("no error recorded")
        );
        return record(pool, tenant, run_id, baseline_run_id, "inconclusive", &note).await;
    }

    // ── the comparison, through the action a person would call ─────────────────────────
    let compare = invoke_action(
        pool,
        tenant,
        app_key,
        "benchmark_runs.compare",
        json!({"a_run_id": run_id, "b_run_id": baseline_run_id}),
        "improvement",
    )
    .await?;
    let comparable = compare
        .get("comparable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !comparable {
        let reason = compare
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the runs are not comparable");
        return record(
            pool,
            tenant,
            run_id,
            baseline_run_id,
            "inconclusive",
            reason,
        )
        .await;
    }
    let a_only = compare.get("a_only").and_then(Value::as_u64).unwrap_or(0);
    let b_only = compare.get("b_only").and_then(Value::as_u64).unwrap_or(0);
    let p = compare
        .get("p_value")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let (verdict, note) = if p >= ALPHA {
        (
            "inconclusive",
            format!("p = {p:.4} is not below {ALPHA}: the candidate has not moved the result"),
        )
    } else if a_only > b_only {
        (
            "keep",
            format!("p = {p:.4} with a positive delta ({a_only} candidate-only vs {b_only})"),
        )
    } else {
        (
            "revert",
            format!("p = {p:.4} with a negative delta ({b_only} baseline-only vs {a_only})"),
        )
    };
    record(pool, tenant, run_id, baseline_run_id, verdict, &note).await
}

async fn record(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    baseline_run_id: Uuid,
    verdict: &str,
    note: &str,
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    store::benchmark::set_improvement_tx(&mut tx, tenant, run_id, verdict, baseline_run_id, note)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tracing::info!(run = %run_id, baseline = %baseline_run_id, verdict, note, "improvement verdict");
    Ok(())
}

/// Invoke one read action as the service principal and return its output document.
async fn invoke_action(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    action: &str,
    input: Value,
    request_id: &str,
) -> Result<Value, JobFailure> {
    let outcome = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: None,
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!("{request_id}:{action}")),
            trace_id: None,
            depth: 1,
            app_key: app_key.to_owned(),
            usage: None,
        },
    )
    .await
    .map_err(|error| JobFailure::retryable(format!("{action}: {error}")))?;
    if !(200..300).contains(&outcome.status) {
        return Err(JobFailure::permanent(format!(
            "{action} answered {}",
            outcome.status
        )));
    }
    Ok(outcome.body.get("output").cloned().unwrap_or(outcome.body))
}
