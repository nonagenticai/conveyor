#![forbid(unsafe_code)]
//! The run's supervisor: `eval.run.tick` finalises, `eval.reap` abandons (S19 Part B step 10;
//! CONTRACT §20).
//!
//! ⛔ **N×R jobs finalise nothing.** Without the tick a run stays `running` forever after its
//! last case, because no single case knows it was the last. The tick answers the only question
//! that can be answered centrally — *is anything still outstanding for this run?* — and answers
//! it from two derived counts, never from a stored counter that a lost driver could leave stale.
//!
//! **The heartbeat and the reaper key on `last_polled_at`, never on case silence.** A run whose
//! cases are quiet is a run whose cases are slow; only a run nothing has ticked is abandoned.
//! That distinction is the S17 lesson, and it is why the tick writes `last_polled_at` on every
//! pass that finds work outstanding.
//!
//! The three terminal outcomes:
//!
//! - **`complete`** — every slot holds a result.
//! - **`partial`** — the work is over and some slots do not: those cases exhausted their
//!   attempts and their evidence is in `eval_attempt_error`, which is append-only and survives.
//! - **`void`** — nothing could be measured at all.
//!
//! Since migration 0034 it also runs `eval.grade`: one `pending` grade row — one output judged
//! once against an eval's current instrument — is settled here, because an `inspection.kind =
//! 'eval'` SOP has no case set to measure.

use chrono::{Duration, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_store as store;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// A database failure while running a grade: worth retrying.
fn db(error: sqlx::Error) -> JobFailure {
    JobFailure::retryable(format!("database: {error}"))
}

/// A store failure while running a grade.
fn eval_db(error: store::eval::EvalStoreError) -> JobFailure {
    JobFailure::retryable(format!("database: {error}"))
}

/// How long a run may go unpolled before the reaper abandons it.
///
/// Five minutes of tick at `* * * * *` leaves twelve missed passes; a run that quiet is not slow,
/// it is unattended.
pub const STALE_AFTER: Duration = Duration::minutes(5);

/// Finalise every in-flight run of one tenant and heartbeat the ones still working.
pub async fn tick(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let now = Utc::now();
    let runs = {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let runs = store::eval::in_flight_runs_tx(&mut tx, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        runs
    };

    for run in runs {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

        let counts = store::eval::slot_counts_tx(&mut tx, tenant, run.id)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let outstanding = store::eval::outstanding_jobs_tx(&mut tx, tenant, run.id)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

        let next = if outstanding > 0 {
            // Work remains: heartbeat, and mark it running once anything has landed.
            store::eval::touch_polled_tx(&mut tx, tenant, run.id, now)
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            if counts.filled > 0 && run.status == RunStatus::Queued {
                Some(RunStatus::Running)
            } else {
                None
            }
        } else if counts.is_full() {
            Some(RunStatus::Complete)
        } else if !counts.is_empty() {
            Some(RunStatus::Partial)
        } else if counts.expected == 0 {
            Some(RunStatus::Void)
        } else {
            // Nothing outstanding, nothing measured: every case failed permanently. `partial`
            // says "the work is over and the slots are empty", which is exactly the truth, and
            // the errors stay readable.
            Some(RunStatus::Partial)
        };

        if let Some(target) = next {
            // ⛔ A run may not jump from `queued` to a terminal state: the domain's table admits
            // `queued → running`, and the tick is what makes that step. Without it a fast run —
            // every case landed before the first tick ever saw it — fails every tick it is given,
            // for ever, because the migration it needs is the one it is refusing to make.
            if run.status == RunStatus::Queued
                && matches!(target, RunStatus::Partial | RunStatus::Complete)
            {
                store::eval::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Running)
                    .await
                    .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            }
            store::eval::transition_run_tx(&mut tx, tenant, run.id, target)
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        }
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    }
    Ok(())
}

/// Abandon every run nothing has polled since `now - STALE_AFTER`.
pub async fn reap(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let cutoff = Utc::now() - STALE_AFTER;
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let stale = store::eval::stale_runs_tx(&mut tx, tenant, cutoff)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    for run in stale {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        match store::eval::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Void).await {
            Ok(row) => {
                tracing::warn!(run = %row.id, "an eval run was abandoned: nothing polled it");
                tx.commit()
                    .await
                    .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            }
            Err(error) => {
                // The run moved underneath us (a tick finalised it); nothing to do.
                tracing::debug!(run = %run.id, %error, "the eval run left the in-flight set");
            }
        }
    }
    Ok(())
}

// ── the one-shot per-output grade (`eval.grade`, migration 0034) ───────────────────────────

/// The action name this job is (used only in the grader's own error text).
const GRADE_ACTION: &str = "eval.grade";

/// Run one `eval.grade` job: settle one `pending` `eval_grade` row.
///
/// The row is written `pending` by `eval_grades.create` with the output, the caller's answer key
/// and the threshold on the job payload; this job grades it with the **same** loop the case
/// driver uses ([`crate::eval_runner::grade_metrics`]) and writes the terminal row.
///
/// Three rules are structural:
///
/// - **The row is the record and the exactly-once guard.** A terminal row is returned from
///   immediately, so a replayed job cannot write a second verdict; `set_terminal_tx`'s own
///   `WHERE status = 'pending'` makes that true even under two workers.
/// - **A grader failure is `failed`, never `rejected`.** A rejected output was judged and lost;
///   a failed one was never judged. The row's `error` names the reason.
/// - **The threshold is the row's, not the payload's.** Both were written from one call, but the
///   stored value is what a dispute replays; the payload copy is provenance only.
pub async fn grade_output(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    payload: &Value,
) -> Result<(), JobFailure> {
    let grade_id = payload
        .get("grade_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent("eval.grade needs a grade_id"))?;

    let Some(grade) = read_grade(pool, tenant, grade_id).await? else {
        // The tenant was restored or the row was deleted: there is nothing to grade.
        return Ok(());
    };
    if grade.status != "pending" {
        tracing::debug!(grade = %grade_id, status = %grade.status, "the eval grade is settled");
        return Ok(());
    }

    let metrics = {
        let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
        let metrics = store::eval::list_metrics_tx(&mut tx, tenant, grade.version_id)
            .await
            .map_err(eval_db)?;
        tx.commit().await.map_err(db)?;
        metrics
    };

    // The payload carries the values the row deliberately does not store: the output itself
    // (identity only on the row) and the caller's answer key.
    let output = payload.get("output").cloned().unwrap_or(Value::Null);
    let case_input = conveyor_actions::eval::grades::case_input_text(payload.get("case_input"));
    let expected = payload.get("expected").filter(|value| !value.is_null());
    // The model that produced the output, when the output names one — the judge guard refuses a
    // judge that is the subject, and an unnamed subject cannot be compared at all.
    let subject_model = output.get("model").and_then(Value::as_str);

    match crate::eval_runner::grade_metrics(
        pool,
        app_key,
        tenant,
        &metrics,
        &case_input,
        expected,
        &output,
        subject_model,
        GRADE_ACTION,
        &format!("eval.grade:{grade_id}"),
        &format!("eval_grade:{grade_id}"),
    )
    .await
    {
        Ok(grades) => {
            let (status, score, reason) = match verdict(&metrics, &grades, grade.threshold) {
                Ok(verdict) => verdict,
                Err(error) => {
                    record_failure(pool, tenant, grade_id, &error).await?;
                    return Err(JobFailure::permanent(format!("eval.grade: {error}")));
                }
            };
            let grade_value = Value::Object(grades.grade);
            let detail_value = Value::Object(grades.detail);
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
            store::eval::set_terminal_tx(
                &mut tx,
                tenant,
                grade_id,
                &store::eval::TerminalGrade {
                    status,
                    grade: &grade_value,
                    detail: &detail_value,
                    score,
                    reason: Some(reason.as_str()),
                    error: None,
                },
            )
            .await
            .map_err(eval_db)?;
            tx.commit().await.map_err(db)?;
            tracing::info!(grade = %grade_id, status, ?score, "eval grade recorded");
            Ok(())
        }
        Err(failure) => {
            let message = format!("{}: {}", failure.class.as_str(), failure.message);
            record_failure(pool, tenant, grade_id, &message).await?;
            Err(if failure.class.is_retryable() {
                JobFailure::retryable(message)
            } else {
                JobFailure::permanent(message)
            })
        }
    }
}

/// The verdict a complete grade yields: `accepted | rejected`, the mean over the quality
/// metrics, and the reason a reader is shown.
///
/// ⛔ **A quality metric that was not graded has no verdict, and is not a rejection.** A `human`
/// metric cannot be scored by a machine, and a metric whose grader produced nothing is a
/// configuration failure — both return an error, so the job writes `failed` and says which metric.
fn verdict(
    metrics: &[store::eval::EvalMetric],
    grades: &crate::eval_runner::MetricGrades,
    threshold: rust_decimal::Decimal,
) -> Result<(&'static str, Option<rust_decimal::Decimal>, String), String> {
    let quality: Vec<&store::eval::EvalMetric> = metrics
        .iter()
        .filter(|metric| metric.role == "quality")
        .collect();
    if quality.is_empty() {
        return Err(
            "the eval version has no quality metric, so there is nothing to accept or reject"
                .to_owned(),
        );
    }

    let mut scored: Vec<(&str, f64)> = Vec::new();
    let mut unscored: Vec<&str> = Vec::new();
    for metric in &quality {
        match grades.grade.get(&metric.key).and_then(Value::as_f64) {
            Some(score) => scored.push((metric.key.as_str(), score)),
            None => unscored.push(metric.key.as_str()),
        }
    }
    if !unscored.is_empty() {
        return Err(format!(
            "no verdict can be reached: the metric(s) {} were not graded",
            unscored.join(", ")
        ));
    }

    let bar = threshold.to_f64().unwrap_or_default();
    let below: Vec<&str> = scored
        .iter()
        .filter(|(_, score)| *score < bar)
        .map(|(key, _)| *key)
        .collect();
    let mean = scored.iter().map(|(_, score)| *score).sum::<f64>() / scored.len() as f64;
    let score = rust_decimal::Decimal::try_from(mean).ok();

    if below.is_empty() {
        Ok((
            "accepted",
            score,
            format!("every quality metric is at or above {threshold}"),
        ))
    } else {
        Ok((
            "rejected",
            score,
            format!("below {threshold}: {}", below.join(", ")),
        ))
    }
}

/// Write `failed` on the row with the reason, leaving a replay a terminal row to find.
async fn record_failure(
    pool: &PgPool,
    tenant: TenantId,
    grade_id: Uuid,
    message: &str,
) -> Result<(), JobFailure> {
    let empty = json!({});
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    store::eval::set_terminal_tx(
        &mut tx,
        tenant,
        grade_id,
        &store::eval::TerminalGrade {
            status: "failed",
            grade: &empty,
            detail: &empty,
            score: None,
            reason: None,
            error: Some(message),
        },
    )
    .await
    .map_err(eval_db)?;
    tx.commit().await.map_err(db)?;
    tracing::warn!(grade = %grade_id, %message, "the eval grade could not be reached");
    Ok(())
}

async fn read_grade(
    pool: &PgPool,
    tenant: TenantId,
    grade_id: Uuid,
) -> Result<Option<store::eval::EvalGrade>, JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let row = store::eval::get_grade_tx(&mut tx, tenant, grade_id)
        .await
        .map_err(eval_db)?;
    tx.commit().await.map_err(db)?;
    Ok(row)
}
