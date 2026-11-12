#![forbid(unsafe_code)]
//! S17's run driver and reaper (`benchmark.run.poll`, `benchmark.reap`).
//!
//! **One driver, one bounded tick.** `poll` starts at most `concurrency` sandbox Jobs, reads the
//! state of the ones already running, grades their logs, writes the cases, and re-enqueues the
//! next tick as a NEW job (`max_attempts = 1`, per-tick dedupe key). It never loops and never
//! sleeps: a tick that overran the job lease would be reclaimed and delivered twice.
//!
//! **Producers today are the two controls.** `gold` submits the instance's gold patch and `empty`
//! submits an empty patch; both are grader-adjacent controls whose logs exercise the whole eval
//! path. The model arms are `baseline` (the `baseline.py` producer port, not built yet) and
//! `conveyor` (S18); both are refused rather than silently producing nothing.
//!
//! **Logs are evidence.** A finished Job's log is stored as an S09 document before the case is
//! finalised; an unconfigured object store is a permanent driver fault, not a reason to record a
//! verdict whose evidence was thrown away.

use bytes::Bytes;
use chrono::{Duration as ChronoDuration, Utc};
use conveyor_actions::integrations::objects::{Objects, key_for};
use conveyor_domain::TenantId;
use conveyor_domain::benchmark::grade::{
    Grade, GradeStatus, InstanceSpec, TestReport, TestStatuses, grade_log,
};
use conveyor_store as store;
use conveyor_store::benchmark::{BenchmarkStoreError, CaseOutcome, Run, RunningCase};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::sandbox::{JobState, Sandbox, SandboxError};
use crate::worker::JobFailure;

/// How often a live run gets its next tick.
const TICK: ChronoDuration = ChronoDuration::seconds(30);
/// A driver with no heartbeat for this long is dead, not slow (CONTRACT §19).
const DRIVER_STALE_AFTER: ChronoDuration = ChronoDuration::minutes(5);
/// The most jobs one tick may start when a run's options carry no concurrency.
const DEFAULT_CONCURRENCY: usize = 5;

/// `benchmark.run.poll` — one bounded driver tick.
pub async fn poll(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let sandbox = sandbox()?;
    let objects = Objects::from_env().ok_or_else(|| {
        JobFailure::permanent(
            "the object store is not configured (CONVEYOR_S3_*), so the eval log cannot be kept",
        )
    })?;
    poll_with(pool, tenant, payload, &sandbox, &objects).await
}

/// One bounded driver tick with the sandbox and the log sink supplied by the caller.
///
/// Split from [`poll`] so a test can drive the exact tick against a stub Kubernetes API and the
/// in-memory object store; production always passes the env-configured pair.
pub async fn poll_with(
    pool: &PgPool,
    tenant: TenantId,
    payload: &Value,
    sandbox: &Sandbox,
    objects: &Objects,
) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id", store::benchmark::POLL_KIND)?;

    // ── 1. load, cancel, heartbeat (one short transaction) ──────────────────────────────
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let run = store::benchmark::get_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(store_failure)?;
    if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Ok(());
    }
    if run.cancel_requested_at.is_some() {
        let refs = store::benchmark::sandbox_refs_tx(&mut tx, tenant, run_id)
            .await
            .map_err(store_failure)?;
        let run = store::benchmark::finish_run_tx(&mut tx, tenant, run_id, "cancelled", None, None)
            .await
            .map_err(store_failure)?;
        store::benchmark::append_run_event_tx(&mut tx, tenant, &run, "benchmark_run.changed")
            .await
            .map_err(store_failure)?;
        tx.commit().await.map_err(db)?;
        delete_jobs(sandbox, &refs).await;
        return Ok(());
    }
    let run = store::benchmark::touch_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(store_failure)?;
    store::benchmark::append_run_event_tx(&mut tx, tenant, &run, "benchmark_run.changed")
        .await
        .map_err(store_failure)?;
    let running = store::benchmark::running_cases_tx(&mut tx, tenant, run_id)
        .await
        .map_err(store_failure)?;
    tx.commit().await.map_err(db)?;

    // ── 2. poll the Jobs already running (never hold a transaction across HTTP) ─────────
    let mut completed = 0usize;
    for case in &running {
        match sandbox.state(&case.sandbox_ref).await {
            Ok(JobState::Running) => {}
            Ok(JobState::Succeeded) => {
                let log = sandbox
                    .logs(&case.sandbox_ref)
                    .await
                    .map_err(|error| JobFailure::retryable(format!("sandbox log: {error}")))?;
                finalise_graded(pool, tenant, &run, case, &log, objects).await?;
                completed += 1;
            }
            Ok(JobState::Failed { reason }) => {
                finalise_infra(pool, tenant, &run, case, &reason).await?;
                completed += 1;
            }
            Err(SandboxError::NotFound { .. } | SandboxError::NoPod(_)) => {
                finalise_infra(pool, tenant, &run, case, "sandbox job disappeared").await?;
                completed += 1;
            }
            Err(error) => {
                return Err(JobFailure::retryable(format!(
                    "sandbox state {}: {error}",
                    case.sandbox_ref
                )));
            }
        }
    }

    // ── 3. start new cases while there is room ──────────────────────────────────────────
    let concurrency = run
        .options
        .get("concurrency")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_CONCURRENCY);
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let still_running = store::benchmark::running_cases_tx(&mut tx, tenant, run_id)
        .await
        .map_err(store_failure)?;
    let slots = concurrency.saturating_sub(still_running.len());
    let pending = if slots > 0 {
        store::benchmark::pending_cases_tx(&mut tx, tenant, run_id, slots as i64)
            .await
            .map_err(store_failure)?
    } else {
        Vec::new()
    };
    tx.commit().await.map_err(db)?;

    if !pending.is_empty() {
        // **A `conveyor` case is produced before it is graded** (S18 step 1). Production is an
        // `agent.run`, which is bounded but long, so it can never happen inline in a tick; the
        // driver enqueues one producer job per case and moves on. Every other arm already holds
        // its patch, so it goes straight to a grading Job.
        let (to_grade, to_produce): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .partition(|case| run.arm != "conveyor" || case.produce_status == "ok");
        if !to_produce.is_empty() {
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
            for case in &to_produce {
                // The producer's budget bounds the run: a case that spent every attempt is written
                // `no_attempt` here, so a broken arm cannot hold the run live for ever.
                if case.attempts >= store::benchmark::MAX_PRODUCE_ATTEMPTS {
                    store::benchmark::record_produced_tx(
                        &mut tx,
                        tenant,
                        case.id,
                        "no_attempt",
                        None,
                        None,
                    )
                    .await
                    .map_err(store_failure)?;
                    continue;
                }
                // Dedupe by case: a live producer makes the enqueue a no-op, and a dead one is
                // replaced on the next tick rather than stalling the run forever.
                store::jobs::enqueue_tx(
                    &mut tx,
                    tenant,
                    conveyor_actions::benchmark::runs::PRODUCE_KIND,
                    &format!("benchmark:produce:{}", case.id),
                    json!({"run_id": run_id, "instance_id": case.instance_id}),
                    None,
                    3,
                )
                .await
                .map_err(|error| {
                    JobFailure::retryable(format!("enqueue the conveyor producer: {error}"))
                })?;
            }
            tx.commit().await.map_err(db)?;
        }

        // Resolve the answers in one short scoped transaction, then produce and start the Jobs
        // outside it: `set_config(..., true)` is transaction-local, so a bare pool connection
        // would lose the tenant before the next statement.
        let mut resolved = Vec::with_capacity(to_grade.len());
        let mut answers = store::begin_scoped(pool, tenant).await.map_err(db)?;
        for case in &to_grade {
            resolved.push((
                case.clone(),
                store::benchmark::grader_instance_tx(
                    &mut answers,
                    tenant,
                    run.suite_id,
                    &case.instance_id,
                )
                .await
                .map_err(store_failure)?,
            ));
        }
        answers.commit().await.map_err(db)?;

        let mut started = Vec::with_capacity(resolved.len());
        for (case, instance) in &resolved {
            let patch = match run.arm.as_str() {
                "gold" => instance.gold_patch.clone(),
                "empty" => String::new(),
                "conveyor" => match &case.patch {
                    Some(patch) => patch.clone(),
                    // `to_grade` only carries produced cases, so this is a driver invariant, not
                    // a model failure: grading a phantom patch would publish a false verdict.
                    None => {
                        return Err(JobFailure::permanent(
                            "a conveyor case reached the grader with no produced patch",
                        ));
                    }
                },
                "baseline" => {
                    return Err(JobFailure::permanent(
                        "the baseline producer (the baseline.py port) is not built yet",
                    ));
                }
                other => {
                    return Err(JobFailure::permanent(format!(
                        "unknown benchmark arm {other}"
                    )));
                }
            };
            if patch.len() > store::benchmark::MAX_PATCH_BYTES {
                finalise_infra(
                    pool,
                    tenant,
                    &run,
                    &RunningCase {
                        instance_id: instance.instance_id.clone(),
                        sandbox_ref: String::new(),
                        attempts: 0,
                        patch: None,
                    },
                    &format!("the produced patch is {} bytes, over the cap", patch.len()),
                )
                .await?;
                completed += 1;
                continue;
            }
            let hash = sha256_hex(&patch);
            let name = sandbox
                .start(
                    run.id,
                    &instance.instance_id,
                    &instance.image,
                    &instance.eval_script,
                    &patch,
                )
                .await
                .map_err(|error| JobFailure::retryable(format!("sandbox start: {error}")))?;
            started.push((instance.instance_id.clone(), name, patch, hash));
        }

        let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
        for (instance_id, name, patch, hash) in &started {
            store::benchmark::attach_sandbox_tx(
                &mut tx,
                tenant,
                run_id,
                instance_id,
                name,
                Some(patch),
                Some(hash),
            )
            .await
            .map_err(store_failure)?;
        }
        tx.commit().await.map_err(db)?;
    }

    // ── 4. finish, or schedule the next tick ────────────────────────────────────────────
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let run = store::benchmark::refresh_run_counts_tx(&mut tx, tenant, run_id)
        .await
        .map_err(store_failure)?;
    let pending_left = store::benchmark::pending_cases_tx(&mut tx, tenant, run_id, 1)
        .await
        .map_err(store_failure)?;
    let running_left = store::benchmark::running_cases_tx(&mut tx, tenant, run_id)
        .await
        .map_err(store_failure)?;
    if pending_left.is_empty() && running_left.is_empty() {
        let control_ok = control_ok(&run);
        let run =
            store::benchmark::finish_run_tx(&mut tx, tenant, run_id, "succeeded", None, control_ok)
                .await
                .map_err(store_failure)?;
        store::benchmark::append_run_event_tx(&mut tx, tenant, &run, "benchmark_run.changed")
            .await
            .map_err(store_failure)?;
        tx.commit().await.map_err(db)?;
        tracing::info!(run = %run_id, completed, "benchmark run finished");
        return Ok(());
    }
    store::benchmark::enqueue_poll_tx(
        &mut tx,
        tenant,
        run_id,
        &format!("benchmark:{run_id}:{}", Utc::now().timestamp_millis()),
        Some(Utc::now() + TICK),
    )
    .await
    .map_err(store_failure)?;
    tx.commit().await.map_err(db)?;
    Ok(())
}

/// `benchmark.reap` — fail runs whose driver is dead and delete their orphaned Jobs.
pub async fn reap(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let reaped = store::benchmark::reap_stale_runs_tx(&mut tx, tenant, DRIVER_STALE_AFTER)
        .await
        .map_err(store_failure)?;
    let mut refs = Vec::new();
    for run in &reaped {
        refs.extend(
            store::benchmark::sandbox_refs_tx(&mut tx, tenant, run.id)
                .await
                .map_err(store_failure)?,
        );
        store::benchmark::append_run_event_tx(&mut tx, tenant, run, "benchmark_run.changed")
            .await
            .map_err(store_failure)?;
    }
    tx.commit().await.map_err(db)?;
    if !reaped.is_empty() {
        tracing::warn!(
            runs = reaped.len(),
            "benchmark: driver stalled, runs failed"
        );
    }
    if let Ok(sandbox) = Sandbox::from_env() {
        delete_jobs(&sandbox, &refs).await;
    }
    Ok(())
}

/// Grade a finished Job's log and write the case.
async fn finalise_graded(
    pool: &PgPool,
    tenant: TenantId,
    run: &Run,
    case: &RunningCase,
    log: &str,
    objects: &Objects,
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let instance =
        store::benchmark::grader_instance_tx(&mut tx, tenant, run.suite_id, &case.instance_id)
            .await
            .map_err(store_failure)?;
    let spec = InstanceSpec {
        instance_id: &instance.instance_id,
        repo: &instance.repo,
        fail_to_pass: &instance.fail_to_pass,
        pass_to_pass: &instance.pass_to_pass,
        parser: &instance.parser,
    };
    let grade = grade_log(&spec, case.patch.as_deref(), log)
        .map_err(|error| JobFailure::permanent(format!("grade: {error}")))?;
    let log_document_id =
        store_log(&mut tx, tenant, run.id, &case.instance_id, log, objects).await?;
    let detail = grade_json(&grade, log_document_id.is_none());
    let verdict = grade.status.as_str();
    store::benchmark::finalise_case_tx(
        &mut tx,
        tenant,
        run.id,
        &case.instance_id,
        &CaseOutcome {
            produce_status: "ok",
            patch: case.patch.as_deref(),
            patch_sha256: None,
            verdict,
            attempts: case.attempts,
            seconds: None,
            sandbox_ref: Some(&case.sandbox_ref),
            log_document_id,
            detail: &detail,
        },
    )
    .await
    .map_err(store_failure)?;
    store::benchmark::append_case_event_tx(&mut tx, tenant, run.id, &case.instance_id, verdict)
        .await
        .map_err(store_failure)?;
    let refreshed = store::benchmark::refresh_run_counts_tx(&mut tx, tenant, run.id)
        .await
        .map_err(store_failure)?;
    store::benchmark::append_run_event_tx(&mut tx, tenant, &refreshed, "benchmark_run.changed")
        .await
        .map_err(store_failure)?;
    tx.commit().await.map_err(db)?;
    Ok(())
}

/// A case whose sandbox failed: `infra_error`, never a model verdict (CONTRACT §19).
async fn finalise_infra(
    pool: &PgPool,
    tenant: TenantId,
    run: &Run,
    case: &RunningCase,
    reason: &str,
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let detail = json!({
        "status": GradeStatus::InfraError.as_str(),
        "resolved": false,
        "infra_failure": true,
        "infra_failure_reason": reason,
    });
    store::benchmark::finalise_case_tx(
        &mut tx,
        tenant,
        run.id,
        &case.instance_id,
        &CaseOutcome {
            produce_status: "ok",
            patch: case.patch.as_deref(),
            patch_sha256: None,
            verdict: GradeStatus::InfraError.as_str(),
            attempts: case.attempts,
            seconds: None,
            sandbox_ref: (!case.sandbox_ref.is_empty()).then_some(case.sandbox_ref.as_str()),
            log_document_id: None,
            detail: &detail,
        },
    )
    .await
    .map_err(store_failure)?;
    store::benchmark::append_case_event_tx(
        &mut tx,
        tenant,
        run.id,
        &case.instance_id,
        GradeStatus::InfraError.as_str(),
    )
    .await
    .map_err(store_failure)?;
    let refreshed = store::benchmark::refresh_run_counts_tx(&mut tx, tenant, run.id)
        .await
        .map_err(store_failure)?;
    store::benchmark::append_run_event_tx(&mut tx, tenant, &refreshed, "benchmark_run.changed")
        .await
        .map_err(store_failure)?;
    tx.commit().await.map_err(db)?;
    Ok(())
}

/// Store the log as an S09 document; an unconfigured store is a permanent driver fault.
async fn store_log(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    instance_id: &str,
    log: &str,
    objects: &Objects,
) -> Result<Option<Uuid>, JobFailure> {
    let id = Uuid::now_v7();
    let hash = sha256_hex(log);
    let key = key_for(tenant.as_uuid(), id, &hash);
    objects
        .put(&key, Bytes::from(log.as_bytes().to_vec()))
        .await
        .map_err(|error| JobFailure::retryable(format!("object store: {error}")))?;
    let document = store::document::create_document_tx(
        &mut *conn,
        tenant,
        &store::document::NewDocument {
            id,
            project_id: None,
            kind: "document".to_owned(),
            title: format!("benchmark log {instance_id}"),
            status: "active".to_owned(),
            object_key: Some(key),
            content_hash: Some(hash),
            external_ref: None,
            meta: json!({ "benchmark_run_id": run_id, "instance_id": instance_id }),
        },
    )
    .await
    .map_err(document_failure)?;
    Ok(Some(document.id))
}

fn grade_json(grade: &Grade, log_was_unstored: bool) -> Value {
    json!({
        "status": grade.status.as_str(),
        "resolved": grade.resolved,
        "tests_status": grade.tests_status.as_ref().map(tests_status_json),
        "infra_failure": grade.infra_failure,
        "infra_failure_reason": grade.infra_failure_reason,
        "patch_is_none": grade.patch_is_none,
        "patch_exists": grade.patch_exists,
        "patch_successfully_applied": grade.patch_successfully_applied,
        "detail": grade.detail,
        "log_unstored": log_was_unstored,
    })
}

fn tests_status_json(report: &TestReport) -> Value {
    json!({
        "fail_to_pass": statuses_json(&report.fail_to_pass),
        "pass_to_pass": statuses_json(&report.pass_to_pass),
        "fail_to_fail": statuses_json(&report.fail_to_fail),
        "pass_to_fail": statuses_json(&report.pass_to_fail),
    })
}

fn statuses_json(statuses: &TestStatuses) -> Value {
    json!({ "success": statuses.success, "failure": statuses.failure })
}

/// `None` for a non-control arm; a control that missed is recorded `false`, never hidden.
fn control_ok(run: &Run) -> Option<bool> {
    match run.arm.as_str() {
        "gold" => Some(run.n_graded > 0 && run.n_resolved == run.n_graded),
        "empty" => Some(run.n_graded > 0 && run.n_resolved == 0),
        _ => None,
    }
}

fn sandbox() -> Result<Sandbox, JobFailure> {
    Sandbox::from_env().map_err(|error| match error {
        SandboxError::NotConfigured => {
            JobFailure::permanent("the sandbox is not configured (SANDBOX_NAMESPACE is unset)")
        }
        other => JobFailure::permanent(format!("sandbox: {other}")),
    })
}

async fn delete_jobs(sandbox: &Sandbox, names: &[String]) {
    for name in names {
        if let Err(error) = sandbox.delete(name).await {
            tracing::warn!(job = %name, %error, "benchmark: could not delete sandbox job");
        }
    }
}

fn db(error: sqlx::Error) -> JobFailure {
    JobFailure::retryable(format!("database: {error}"))
}

fn store_failure(error: BenchmarkStoreError) -> JobFailure {
    match error {
        BenchmarkStoreError::Database(error) => JobFailure::retryable(format!("database: {error}")),
        other => JobFailure::permanent(other.to_string()),
    }
}

fn document_failure(error: store::document::DocumentStoreError) -> JobFailure {
    match error {
        store::document::DocumentStoreError::Database(error) => {
            JobFailure::retryable(format!("database: {error}"))
        }
        other => JobFailure::permanent(other.to_string()),
    }
}

fn want_uuid(payload: &Value, field: &str, kind: &str) -> Result<Uuid, JobFailure> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent(format!("{kind} payload has no usable {field}")))
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
