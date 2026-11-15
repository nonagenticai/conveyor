#![forbid(unsafe_code)]
//! `runner.dispatch` and the poll that follows it — how a heavy capability runs in a pod instead of
//! in this process (#144 P5, ADR 0007 §Decision 3–4).
//!
//! **One ledger row, two steps, one attempt.** The job row is claimed once to dispatch and claimed
//! again to poll. The second claim is a continuation: the handler returns
//! [`JobFailure::deferred`], the worker calls `store::jobs::defer`, and the claim statement skips
//! the `attempts` increment. `runner_run.attempt` therefore stays put, which matters because
//! `(job_id, attempt)` is the pod's idempotency key.
//!
//! **The pod is not trusted with the verdict.** It records its result through `POST /result`; this
//! poll re-validates that result against the instruction the attempt was dispatched with, checks
//! every artefact against the sha256 of the bytes it uploaded, and only then finishes the job. A
//! result that names bytes conveyor does not hold is refused, and a transient failure spends the
//! single retry the ledger allows.
//!
//! **The dispatch is where the credential is minted.** The job token lives only in the attempt's
//! Kubernetes Secret; it is never written to this database, never logged, and it expires with the
//! instruction's deadline plus a short grace period.
//!
//! Fail-closed throughout: no image for the kind, no token key, no base URL or no Kubernetes
//! configuration is a PERMANENT failure naming the missing setting, never a guess.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use conveyor_api::Config;
use conveyor_auth::job_token;
use conveyor_domain::runner::{
    CONTRACT, ErrorClass, Input, Instruction, Limits, Output, RunResult, Status,
};
use conveyor_runner::{Cluster, Dispatch};
use conveyor_store as store;
use serde_json::{Value, json};

use crate::worker::JobFailure;

/// How soon a dispatched job is polled again. Short, because the poll is what notices a pod that
/// died and spends the one retry; the ledger's `run_after` is what paces it.
pub const POLL_AFTER_SECS: u64 = 5;

/// Extra life on the token past the instruction's deadline: the pod must still be able to post its
/// result at the moment the deadline fires, or a job that finished exactly on time is unreportable.
pub const TOKEN_GRACE_SECS: i64 = 300;

/// How long after its deadline a live attempt is considered abandoned. The Job's own
/// `activeDeadlineSeconds` fires first; this is the backstop for a pod that never starts.
pub const DEADLINE_SLACK_SECS: i64 = 120;

/// Attempts older than this are swept by `runner.reap`, whatever their state. A media job's wall
/// clock is minutes, so a day-old live attempt has no reader.
pub const REAP_AFTER_HOURS: i64 = 24;

/// The default caps a runner job gets when its payload declares none. Deliberately modest: a
/// capability that needs more says so in the payload, and the instruction is the record.
fn default_limits() -> Limits {
    Limits {
        cpu_millis: 500,
        memory_bytes: 512 * 1024 * 1024,
        wall_secs: 900,
        max_output_bytes: 256 * 1024 * 1024,
    }
}

/// True when this job kind runs in a pod. The deployment's image map is the registry of runner
/// kinds — a kind absent from it is not one, and the ordinary handler dispatch handles it.
pub fn handles(config: &Config, kind: &str) -> bool {
    config.runner_image(kind).is_some()
}

/// Build the instruction a dispatch sends. The payload owns the capability's vocabulary; this
/// function only adds the identity and the deadline.
fn instruction_for(
    job: &store::jobs::Job,
    base_url: &str,
    now: DateTime<Utc>,
) -> Result<Instruction, JobFailure> {
    let payload = &job.payload;
    let limits: Limits = match payload.get("limits") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            JobFailure::permanent(format!("the job's limits are unusable: {error}"))
        })?,
        None => default_limits(),
    };
    let inputs: Vec<Input> = match payload.get("inputs") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            JobFailure::permanent(format!("the job's inputs are unusable: {error}"))
        })?,
        None => Vec::new(),
    };
    let outputs: Vec<Output> = match payload.get("outputs") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            JobFailure::permanent(format!("the job's outputs are unusable: {error}"))
        })?,
        None => Vec::new(),
    };
    let correlation_id = payload
        .get("correlation_id")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(job.id);
    let instruction = Instruction {
        schema: CONTRACT.to_owned(),
        job_id: job.id,
        tenant: job.tenant_id,
        kind: job.kind.clone(),
        attempt: job.attempts,
        correlation_id,
        deadline_unix_ms: (now + ChronoDuration::seconds(limits.wall_secs)).timestamp_millis(),
        params: payload.get("params").cloned().unwrap_or_else(|| json!({})),
        inputs,
        outputs,
        limits,
        conveyor_base_url: base_url.trim_end_matches('/').to_owned(),
        trace_id: job.id.to_string(),
    };
    instruction.validate().map_err(|error| {
        JobFailure::permanent(format!("the instruction is not runner/v1: {error}"))
    })?;
    Ok(instruction)
}

/// The production entry point: build the cluster client from the environment and step once.
pub async fn step(
    pool: &sqlx::PgPool,
    config: &Config,
    tenant: conveyor_domain::TenantId,
    job: &store::jobs::Job,
) -> Result<(), JobFailure> {
    let cluster = Cluster::from_env()
        .map_err(|error| JobFailure::permanent(format!("the runner is not configured: {error}")))?;
    step_with(pool, config, tenant, job, &cluster).await
}

/// One step of a runner job, with the cluster supplied so a test can drive the exact state machine
/// against a stub API server.
pub async fn step_with(
    pool: &sqlx::PgPool,
    config: &Config,
    tenant: conveyor_domain::TenantId,
    job: &store::jobs::Job,
    cluster: &Cluster,
) -> Result<(), JobFailure> {
    let image = config
        .runner_image(&job.kind)
        .ok_or_else(|| JobFailure::permanent(format!("no image is configured for {}", job.kind)))?;
    let base_url = config.runner_base_url.as_deref().ok_or_else(|| {
        JobFailure::permanent(
            "CONVEYOR_RUNNER_BASE_URL is not configured, so a pod cannot call back",
        )
    })?;
    let key = config.job_token_key.as_ref().ok_or_else(|| {
        JobFailure::permanent(
            "CONVEYOR_JOB_TOKEN_KEY is not configured, so no pod token can be minted",
        )
    })?;

    let run = store::runner::attempt(pool, tenant, job.id, job.attempts)
        .await
        .map_err(db)?;
    match run {
        None => {
            dispatch(pool, tenant, job, image, base_url, key, cluster).await?;
            Err(JobFailure::deferred(
                "dispatched; polling",
                std::time::Duration::from_secs(POLL_AFTER_SECS),
            ))
        }
        Some(run) => poll(pool, tenant, job, &run, cluster).await,
    }
}

async fn dispatch(
    pool: &sqlx::PgPool,
    tenant: conveyor_domain::TenantId,
    job: &store::jobs::Job,
    image: &str,
    base_url: &str,
    key: &job_token::JobTokenKey,
    cluster: &Cluster,
) -> Result<(), JobFailure> {
    let now = Utc::now();
    let instruction = instruction_for(job, base_url, now)?;
    let ttl = std::time::Duration::from_secs(
        (instruction.limits.wall_secs + TOKEN_GRACE_SECS).max(TOKEN_GRACE_SECS) as u64,
    );
    let token = job_token::mint(
        key,
        &job_token::JobClaims {
            job_id: job.id,
            tenant,
            attempt: job.attempts,
            expires_at: job_token::expires_at(now, ttl),
        },
    );

    let dispatch = Dispatch {
        namespace: cluster.namespace().to_owned(),
        image: image.to_owned(),
        pull_secret: cluster.pull_secret().to_owned(),
        node_selector: None,
        toleration: None,
    };
    let manifests = conveyor_runner::manifests(&instruction, &dispatch, &token)
        .map_err(|error| JobFailure::permanent(format!("the Job spec is unusable: {error}")))?;

    // ConfigMap, then Secret, then the Job that references both: a Job created before its payload
    // exists would start a pod that cannot read its instruction.
    cluster
        .create_config_map(&manifests.config_map, &manifests.name)
        .await
        .map_err(cluster_failure)?;
    cluster
        .create_secret(&manifests.secret, &manifests.name)
        .await
        .map_err(cluster_failure)?;
    cluster
        .create_job(&manifests.job, &manifests.name)
        .await
        .map_err(cluster_failure)?;

    let instruction = serde_json::to_value(&instruction).map_err(|error| {
        JobFailure::permanent(format!("the instruction does not serialise: {error}"))
    })?;
    store::runner::begin(
        pool,
        tenant,
        &store::runner::NewRun {
            job_id: job.id,
            attempt: job.attempts,
            image,
            namespace: cluster.namespace(),
            job_name: &manifests.name,
            instruction,
        },
    )
    .await
    .map_err(db)?;
    Ok(())
}

async fn poll(
    pool: &sqlx::PgPool,
    tenant: conveyor_domain::TenantId,
    job: &store::jobs::Job,
    run: &store::runner::Run,
    cluster: &Cluster,
) -> Result<(), JobFailure> {
    let run_id = run.id;
    let instruction: Instruction =
        serde_json::from_value(run.instruction.clone()).map_err(|error| {
            JobFailure::permanent(format!("the stored instruction is unreadable: {error}"))
        })?;

    if run.status == "reported" {
        return finish_from_result(pool, tenant, job, run_id, &instruction).await;
    }

    let deadline = instruction
        .deadline()
        .ok_or_else(|| JobFailure::permanent("the instruction has no deadline"))?;
    if Utc::now() > deadline + ChronoDuration::seconds(DEADLINE_SLACK_SECS) {
        abandon(cluster, &run.job_name).await;
        store::runner::finish(pool, tenant, run_id, "lost")
            .await
            .map_err(db)?;
        return Err(JobFailure::permanent(format!(
            "attempt {} passed its deadline without a result",
            run.attempt
        )));
    }

    match cluster.job_state(&run.job_name).await {
        Ok(conveyor_runner::JobState::Running) => {
            if run.status == "dispatched" {
                store::runner::mark_running(pool, tenant, job.id, run.attempt)
                    .await
                    .map_err(db)?;
            }
            Err(JobFailure::deferred(
                "still running",
                std::time::Duration::from_secs(POLL_AFTER_SECS),
            ))
        }
        // The pod exited 0 but has not posted its result yet; give it the grace period.
        Ok(conveyor_runner::JobState::Succeeded) => Err(JobFailure::deferred(
            "the pod finished; waiting for its result",
            std::time::Duration::from_secs(POLL_AFTER_SECS),
        )),
        Ok(conveyor_runner::JobState::Failed { reason }) => {
            abandon(cluster, &run.job_name).await;
            store::runner::finish(pool, tenant, run_id, "lost")
                .await
                .map_err(db)?;
            // A pod that died before reporting is exactly the infrastructure failure the ledger
            // retries once; `max_attempts` is what stops a second death from looping.
            Err(JobFailure::retryable(format!(
                "attempt {} died without a result: {reason}",
                run.attempt
            )))
        }
        Err(error) => Err(JobFailure::retryable(format!(
            "the Kubernetes API is not answering: {error}"
        ))),
    }
}

/// Consume a reported result: validate it, bind its artefacts to the bytes conveyor actually holds,
/// then finish the attempt and the job together.
async fn finish_from_result(
    pool: &sqlx::PgPool,
    tenant: conveyor_domain::TenantId,
    job: &store::jobs::Job,
    run_id: uuid::Uuid,
    instruction: &Instruction,
) -> Result<(), JobFailure> {
    let value = store::runner::attempt(pool, tenant, job.id, job.attempts)
        .await
        .map_err(db)?
        .and_then(|run| run.result)
        .ok_or_else(|| JobFailure::permanent("the attempt reports a result it does not hold"))?;
    let result: RunResult = serde_json::from_value(value).map_err(|error| {
        JobFailure::permanent(format!("the stored result is unreadable: {error}"))
    })?;
    if let Err(error) = result.validate(instruction) {
        store::runner::finish(pool, tenant, run_id, "failed")
            .await
            .map_err(db)?;
        return Err(JobFailure::permanent(format!(
            "the result does not satisfy runner/v1: {error}"
        )));
    }

    let stored = store::runner::artefacts(pool, tenant, run_id)
        .await
        .map_err(db)?;
    for artefact in &result.artefacts {
        let Some(row) = stored.iter().find(|row| row.name == artefact.name) else {
            store::runner::finish(pool, tenant, run_id, "failed")
                .await
                .map_err(db)?;
            return Err(JobFailure::permanent(format!(
                "the result names artefact {:?}, which was never uploaded",
                artefact.name
            )));
        };
        if row.sha256 != artefact.sha256 || row.bytes != artefact.bytes {
            store::runner::finish(pool, tenant, run_id, "failed")
                .await
                .map_err(db)?;
            return Err(JobFailure::permanent(format!(
                "artefact {:?} does not match the bytes conveyor stored",
                artefact.name
            )));
        }
    }

    match result.status {
        Status::Succeeded => {
            store::runner::finish(pool, tenant, run_id, "succeeded")
                .await
                .map_err(db)?;
            Ok(())
        }
        Status::Failed => {
            store::runner::finish(pool, tenant, run_id, "failed")
                .await
                .map_err(db)?;
            let Some(error) = result.error else {
                return Err(JobFailure::permanent("a failed result carried no error"));
            };
            let message = format!("{}: {}", error.code, error.message);
            match error.class {
                ErrorClass::Transient => Err(JobFailure::retryable(message)),
                ErrorClass::Input | ErrorClass::Provider | ErrorClass::Permanent => {
                    Err(JobFailure::permanent(message))
                }
            }
        }
    }
}

/// Cancel a runner job: delete whatever the live attempt created, mark it lost, and dead-letter the
/// row.
///
/// The token needs no separate revocation: the byte routes fence on `status = 'running'`, and the
/// row is `dead` before this returns, so a pod that keeps polling gets 403 on its next request.
/// Cluster cleanup is best-effort — the row must become uncallable even when the API server is
/// unreachable.
pub async fn cancel(
    pool: &sqlx::PgPool,
    tenant: conveyor_domain::TenantId,
    job_id: uuid::Uuid,
) -> Result<bool, JobFailure> {
    cancel_with(pool, tenant, job_id, Cluster::from_env().ok().as_ref()).await
}

/// [`cancel`] with the cluster supplied (or withheld), so a test can prove the Job is deleted and
/// a deployment without cluster credentials still makes the row uncallable.
pub async fn cancel_with(
    pool: &sqlx::PgPool,
    tenant: conveyor_domain::TenantId,
    job_id: uuid::Uuid,
    cluster: Option<&Cluster>,
) -> Result<bool, JobFailure> {
    if let Some(run) = store::runner::latest(pool, tenant, job_id)
        .await
        .map_err(db)?
        && !run.is_terminal()
    {
        if let Some(cluster) = cluster {
            abandon(cluster, &run.job_name).await;
        }
        store::runner::finish(pool, tenant, run.id, "lost")
            .await
            .map_err(db)?;
    }
    store::jobs::cancel(pool, tenant, job_id, "cancelled")
        .await
        .map_err(db)
}

/// `runner.reap` — delete the objects an abandoned attempt left behind.
///
/// The poll owns the job transitions; this is housekeeping for the cluster, so it never touches a
/// job row. Its own per-tick job row keeps it off the dispatch path.
pub async fn reap(
    pool: &sqlx::PgPool,
    _config: &Config,
    tenant: conveyor_domain::TenantId,
) -> Result<(), JobFailure> {
    let cluster = Cluster::from_env()
        .map_err(|error| JobFailure::permanent(format!("the runner is not configured: {error}")))?;
    let cutoff = Utc::now() - ChronoDuration::hours(REAP_AFTER_HOURS);
    for run in store::runner::stale_before(pool, tenant, cutoff)
        .await
        .map_err(db)?
    {
        abandon(&cluster, &run.job_name).await;
        store::runner::finish(pool, tenant, run.id, "lost")
            .await
            .map_err(db)?;
    }
    Ok(())
}

/// Delete the three objects of one attempt. A missing object is success — this is a reaper.
async fn abandon(cluster: &Cluster, name: &str) {
    if let Err(error) = cluster.delete_job(name).await {
        tracing::warn!(%error, job_name = name, "the reaper could not delete the Job");
    }
    if let Err(error) = cluster.delete_config_map(name).await {
        tracing::warn!(%error, job_name = name, "the reaper could not delete the ConfigMap");
    }
    if let Err(error) = cluster.delete_secret(name).await {
        tracing::warn!(%error, job_name = name, "the reaper could not delete the Secret");
    }
}

fn db(error: sqlx::Error) -> JobFailure {
    JobFailure::retryable(format!("database: {error}"))
}

fn cluster_failure(error: conveyor_runner::RunnerError) -> JobFailure {
    // A refused create is retryable: the API server may be restarting. A credential or
    // configuration fault is not, and `RunnerError` says which.
    match error {
        conveyor_runner::RunnerError::NotConfigured
        | conveyor_runner::RunnerError::Credential(_) => {
            JobFailure::permanent(format!("the runner is not configured: {error}"))
        }
        other => JobFailure::retryable(format!("the Kubernetes API refused the dispatch: {other}")),
    }
}
