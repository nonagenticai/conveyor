//! The `worker` subcommand: schedule materialisation, claim, dispatch, fenced finish.
//!
//! One poll loop, no timers (CONTRACT §7). Each tick:
//! 1. materialises due `schedule` rows into jobs (under the advisory lock, so two replicas
//!    create one job — see `conveyor_store::schedule`);
//! 2. iterates `active` tenants (the `tenant` table is the only unscoped one) and claims up
//!    to [`BATCH_PER_TENANT`] jobs per tenant, each claim in that tenant's scope;
//! 3. runs the handler and finishes with the lease token it claimed.
//!
//! A handler failure never kills the loop: a retryable failure goes back to `pending` with
//! a delay, a permanent one is dead-lettered, and a lost lease (the row was reclaimed by
//! another worker) is logged and discarded — the database already decided.
//!
//! **Andon pause (S06 §5.5).** Before claiming, the tick reads the tenant's `andon_state`.
//! While the cord is pulled the worker claims only [`ANDON_ALLOWED_KINDS`] — delivery and
//! the approval-closed emitter, which must keep running so the cord can be reported and
//! released — and every kind that starts work stays `pending` until it is released. The
//! state is a row read on every tick, never a process-local flag: the process that pulled
//! the cord is not the process that must honour it (S06 §8 trap 3).
//!
//! Kinds this build knows: `event.prune`, `job.prune`, `model.prune`, `validation.prune`,
//! `idempotency.sweep`, `licence.sync`, `webhook.deliver`, the S06-1 run/claim/gate kinds
//! `run.execute`, `run.reap`, `card.lease_sweep`, `card.claim` and `approval.expire`, the
//! S06-2 comms kinds `delivery.send` and `approval.closed.emit`, S07's `quality.score`,
//! S11's `process.step`, and S10's copilot kinds `copilot.turn.reap`,
//! `copilot.proposal.sweep` and `copilot.proposal.execute`, S16's `prompt_audit.run` and its two
//! prunes, S17's `benchmark.run.poll` (the per-tick driver) and `benchmark.reap`, S15's
//! `memory.consolidate` (one digest per card, on run completion only), S13 §5.8's `err.card` (a
//! failed run's move → comment → coach), and S19/S20's eval kinds `eval.case.run`,
//! `eval.regrade`, `eval.grade` (the one-shot per-output grade), `eval.run.tick` and
//! `eval.reap`.
//!
//! The bus consumer executes one notification: a `card.moved` into the ACTIVE lane runs
//! `wip.pull` for that board (§5.8). Everything else on the stream is either a `process.step`
//! request or acked.
//! An unknown kind is a permanent failure: a job of a kind no build handles would otherwise
//! retry forever.
//!
//! **`delivery.send` sends outside its transaction** (§5.6): it locks the row, stamps the
//! attempt and commits, then speaks to the channel, then re-locks to write the outcome. The
//! retry path re-uses the SAME job row — `fail` puts it back to `pending` with the delivery's
//! own backoff — because the dedupe key `delivery:<id>` would refuse a second live job while
//! this one is still running.

use std::collections::HashSet;
use std::time::Duration;

use chrono::Utc;
use conveyor_api::Config;
use conveyor_auth::licence::LicenceSyncClient;
use conveyor_auth::{ChannelAuthority, Role};
use conveyor_bus::{
    Bus, Envelope, GROUP_PROCESS, MAX_DELIVERIES, TOPIC_WORK_REQUESTED, stream_for,
};
use conveyor_domain::quality::{self, QualityScore, RubricInput};
use conveyor_domain::{Delivery, ListKind, RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::Job;
use conveyor_store::governance::GovernanceStoreError;
use conveyor_store::quality::{Decimal, NewQualityEvaluation, QualityStoreError};
use conveyor_store::revision::RevisionStoreError;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::shutdown;

/// How often the worker polls.
pub const TICK: Duration = Duration::from_secs(1);

/// The most jobs one tenant contributes per tick, so a backlog cannot starve other tenants.
pub const BATCH_PER_TENANT: usize = 10;

/// The retry delay after a retryable handler failure.
pub const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The only kinds a worker may claim while the tenant's andon cord is pulled (S06 §5.5,
/// open question 1). Everything that *starts* work stays pending; delivery and the interaction
/// sweeps keep running so a pulled cord can still be reported and released.
pub const ANDON_ALLOWED_KINDS: [&str; 4] = [
    "delivery.send",
    "approval.closed.emit",
    "interaction.dispatch",
    "interaction.close",
];

/// One `card.lease_sweep` pass clears at most this many expired leases.
pub const LEASE_SWEEP_LIMIT: i64 = 100;

/// The lease length a `card.claim` waiter grants when its payload carries no `ttl_s`.
pub const DEFAULT_CLAIM_TTL_SECS: i64 = 300;

/// The `message.kind` one closed approval's hand-off message carries.
///
/// It is also the idempotency witness: `approval.closed.emit` refuses to send a second time
/// for an approval that already has a message of this kind whose `correlation_id` is the
/// approval's id.
pub const APPROVAL_CLOSED_KIND: &str = "approval-closed";

/// The most closed gates one `approval.closed.emit` tick announces.
pub const APPROVAL_CLOSED_BATCH: i64 = 100;

/// The `message.kind` one multicast interaction offer carries (S06d).
pub const INTERACTION_REQUESTED_KIND: &str = "interaction-requested";
/// The `message.kind` the losing channels receive once an interaction resolves.
pub const INTERACTION_CLOSED_KIND: &str = "interaction-closed";
/// The most interactions or dispatches one sweep tick handles.
pub const INTERACTION_BATCH: i64 = 100;

/// The longest backoff a failed delivery send may schedule (`min(2^attempts · 5 s, 900 s)`).
pub const DELIVERY_BACKOFF_CAP_SECS: u64 = 900;

/// Why the worker could not start.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("config: {0}")]
    Config(#[from] conveyor_api::ConfigError),
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("licence client: {0}")]
    Licence(#[from] conveyor_auth::licence::SyncError),
}

/// A handler failure: retry with a delay, or dead-letter now.
#[derive(Clone, Debug)]
pub struct JobFailure {
    /// The message recorded in `job.last_error` (truncated by the store).
    pub message: String,
    /// True when retrying cannot help (a bug, an unknown kind, an unconfigured client).
    pub permanent: bool,
    /// The delay before the retry, when not permanent.
    pub retry_after: Duration,
}

impl JobFailure {
    /// Retry after [`RETRY_AFTER`].
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            permanent: false,
            retry_after: RETRY_AFTER,
        }
    }

    /// Dead-letter immediately.
    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            permanent: true,
            retry_after: RETRY_AFTER,
        }
    }
}

impl std::fmt::Display for JobFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Everything a handler needs. The pool is the runtime role; every query is scoped.
pub struct Worker {
    pool: PgPool,
    config: Config,
    licence: Option<LicenceSyncClient>,
}

impl Worker {
    /// Connect. `licence.sync` is unavailable (and its jobs dead-letter) without
    /// `LICENCES_BASE_URL`; every other kind runs unaffected.
    pub async fn connect(config: Config) -> Result<Self, WorkerError> {
        let pool = store::connect(&config.database_url).await?;
        // `/licenses/active` is protected: without the secure-apps bearer the sync 401s. The
        // credential is optional here only because a local licences instance may have auth
        // off — in the cluster it is always configured.
        let licence = match config.licences_base_url.as_deref() {
            Some(base) => Some(match config.service_credentials() {
                Ok(creds) => LicenceSyncClient::with_credentials(base, creds)?,
                Err(error) => {
                    tracing::warn!(%error, "licence.sync has no S2S credential; /licenses/active will 401");
                    LicenceSyncClient::new(base)?
                }
            }),
            None => {
                tracing::warn!(
                    "LICENCES_BASE_URL is not set: licence.sync jobs will be dead-lettered"
                );
                None
            }
        };
        Ok(Self {
            pool,
            config,
            licence,
        })
    }

    /// The runtime pool, so the bus consumer shares it instead of opening a second one.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// `APP_KEY` — the licence and the executor's action context.
    pub fn app_key(&self) -> &str {
        &self.config.app_key
    }

    /// Poll until SIGTERM/Ctrl-C.
    pub async fn run(mut self) -> Result<(), WorkerError> {
        tracing::info!(
            lease_secs = self.config.job_lease_secs,
            "worker polling (kinds: event.prune, job.prune, model.prune, validation.prune, \
             idempotency.sweep, licence.sync, webhook.deliver, run.execute, run.reap, \
             process.step, card.lease_sweep, card.claim, approval.expire, delivery.send, \
             approval.closed.emit, quality.score, model.validate, copilot.turn.reap, \
             copilot.proposal.sweep, copilot.proposal.execute, benchmark.run.poll, \
             benchmark.reap)"
        );
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {},
                () = shutdown::signal() => break,
            }
            if let Err(error) = self.tick().await {
                tracing::warn!(%error, "worker tick failed; retrying next tick");
            }
        }
        self.pool.close().await;
        tracing::info!("worker stopped");
        Ok(())
    }

    /// One tick: materialise schedules, then drain a bounded batch per tenant.
    async fn tick(&mut self) -> Result<(), sqlx::Error> {
        match store::schedule::materialise_all(&self.pool, Utc::now()).await {
            Ok(created) if !created.is_empty() => {
                tracing::info!(jobs = created.len(), "materialised scheduled jobs");
            }
            Ok(_) => {}
            Err(error) => {
                // Do not skip claiming because the scheduler hiccuped.
                tracing::warn!(%error, "schedule materialisation failed");
            }
        }

        let tenants = store::tenants::active_ids(&self.pool).await?;
        let lease_secs = self.config.job_lease_secs as i64;
        for tenant in tenants {
            // Read the cord before claiming, every tick: the state is a row, so a release
            // takes effect on the next pass without restarting any process.
            let engaged = andon_engaged(&self.pool, tenant).await;
            for _ in 0..BATCH_PER_TENANT {
                let claimed = if engaged {
                    store::jobs::claim_with(
                        &self.pool,
                        tenant,
                        lease_secs,
                        Some(&ANDON_ALLOWED_KINDS),
                    )
                    .await
                } else {
                    store::claim(&self.pool, tenant, lease_secs).await
                };
                let job = match claimed {
                    Ok(Some(job)) => job,
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(%error, tenant = %tenant, "claim failed");
                        break;
                    }
                };
                self.run_job(&job).await;
            }
        }
        Ok(())
    }

    /// Renew a claimed job's lease until the returned handle is aborted.
    ///
    /// The claim sets a `JOB_LEASE_SECS` lease; a handler that outlives it would be reclaimed by
    /// another worker and run twice. Renewal is fenced by the same token the claim returned, so a
    /// worker that has genuinely lost the job cannot extend it — and because the finish is fenced
    /// too, the loser writes nothing.
    fn spawn_renewal(
        &self,
        tenant: TenantId,
        job_id: Uuid,
        token: Uuid,
    ) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        let lease_secs = self.config.job_lease_secs.max(2);
        // Renew at a third of the lease: two consecutive failures still leave time to recover
        // before the lease actually expires.
        let every = Duration::from_secs((lease_secs / 3).max(1));
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // The first tick is immediate; the claim has just set the lease, so skip it.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                match store::jobs::renew(&pool, tenant, job_id, token, lease_secs as i64).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(
                            job = %job_id,
                            "the lease was lost while the handler ran; its finish will be fenced"
                        );
                        return;
                    }
                    Err(error) => tracing::warn!(%error, job = %job_id, "lease renewal failed"),
                }
            }
        })
    }

    async fn run_job(&self, job: &Job) {
        let tenant = match job.tenant() {
            Ok(tenant) => tenant,
            Err(error) => {
                tracing::error!(job = %job.id, %error, "claimed a job with an unusable tenant id");
                return;
            }
        };
        let Some(token) = job.lease_token() else {
            tracing::error!(job = %job.id, "claimed a job without a lease token");
            return;
        };

        tracing::info!(
            job = %job.id,
            kind = job.kind,
            tenant = %tenant,
            attempt = job.attempts,
            "running job"
        );

        // Keep the lease alive while the handler runs. A long step (`agent.run` can hold a model
        // call for minutes) used to outlive `JOB_LEASE_SECS`, at which point another worker
        // reclaimed the job and ran the same step — a duplicate paid call (audit V1). Renewal is
        // fenced by the same `lease_token`, so a worker that genuinely lost the job cannot extend
        // it, and its eventual finish writes nothing.
        let renewal = self.spawn_renewal(tenant, job.id, token);

        let outcome = run_kind(
            &self.pool,
            &self.config,
            self.licence.as_ref(),
            tenant,
            &job.kind,
            &job.payload,
        )
        .await;
        renewal.abort();

        match outcome {
            Ok(()) => match store::finish_succeeded(&self.pool, tenant, job.id, token).await {
                Ok(true) => tracing::info!(job = %job.id, kind = job.kind, "job succeeded"),
                Ok(false) => tracing::warn!(
                    job = %job.id,
                    kind = job.kind,
                    "the lease was lost before the finish; nothing written"
                ),
                Err(error) => tracing::warn!(%error, job = %job.id, "finish failed"),
            },
            Err(failure) => {
                tracing::warn!(
                    job = %job.id,
                    kind = job.kind,
                    permanent = failure.permanent,
                    %failure,
                    "job failed"
                );
                let outcome = if failure.permanent {
                    store::finish_dead(&self.pool, tenant, job.id, token, &failure.message)
                        .await
                        .map(|changed| changed.then_some("dead".to_owned()))
                } else {
                    store::fail(
                        &self.pool,
                        tenant,
                        job.id,
                        token,
                        &failure.message,
                        failure.retry_after,
                    )
                    .await
                    .map(|finish| finish.status)
                };
                match outcome {
                    Ok(Some(status)) => tracing::info!(
                        job = %job.id,
                        kind = job.kind,
                        status,
                        "job finished"
                    ),
                    Ok(None) => tracing::warn!(
                        job = %job.id,
                        kind = job.kind,
                        "the lease was lost before the finish; nothing written"
                    ),
                    Err(error) => tracing::warn!(%error, job = %job.id, "finish failed"),
                }
            }
        }
    }
}

/// Run one job kind. Public so tests exercise the exact dispatch the worker uses.
/// One uuid out of a job payload, or a permanent failure naming what is missing.
fn payload_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, JobFailure> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent(format!("the job payload has no usable `{key}`")))
}

pub async fn run_kind(
    pool: &PgPool,
    config: &Config,
    licence: Option<&LicenceSyncClient>,
    tenant: TenantId,
    kind: &str,
    payload: &serde_json::Value,
) -> Result<(), JobFailure> {
    let db = |error: sqlx::Error| JobFailure::retryable(format!("database: {error}"));
    match kind {
        "event.prune" => {
            let deleted = store::prune::event_prune(
                pool,
                tenant,
                store::prune::EVENT_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "event.prune");
        }
        "job.prune" => {
            let deleted = store::prune::job_prune(
                pool,
                tenant,
                store::prune::JOB_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "job.prune");
        }
        "model.prune" => {
            let deleted = store::prune::model_revision_prune(
                pool,
                tenant,
                store::prune::MODEL_REVISION_KEEP,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "model.prune");
        }
        "validation.prune" => {
            let deleted = store::prune::validation_run_prune(
                pool,
                tenant,
                store::prune::VALIDATION_RUN_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "validation.prune");
        }
        // The purge half of `documents.delete`: the action writes the tombstone and enqueues this; the job
        // removes the object and clears the pointer. Split this way, a bucket outage cannot make a delete
        // fail — it makes the purge retry.
        "document.purge" => {
            let document_id = payload
                .get("document_id")
                .and_then(Value::as_str)
                .and_then(|raw| uuid::Uuid::parse_str(raw).ok())
                .ok_or_else(|| JobFailure::permanent("document.purge needs a document_id"))?;
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
            let record = store::document::get_document_tx(&mut tx, tenant, document_id)
                .await
                .map_err(|error| match error {
                    store::document::DocumentStoreError::Database(error) => db(error),
                    other => JobFailure::permanent(format!("document.purge: {other}")),
                })?;
            // A row that is gone (tenant deleted, database restored) is not a failure: there are no bytes to
            // remove that anything still names.
            //
            // ⚠️ The `status == "deleted"` guard is load-bearing and was added after the test caught its
            // absence: a job payload naming a LIVE document must not remove its bytes. Without the guard the
            // object is deleted first and only then does `clear_object_tx` refuse the row, leaving an active
            // document pointing at nothing.
            if let Some(record) = record
                && record.status == "deleted"
            {
                let had_object = record.object_key.is_some();
                let had_body = record.body.is_some();
                if had_object || had_body {
                    // A text output has no object; only an object-backed document needs the store.
                    if let Some(key) = record.object_key.clone() {
                        let objects = conveyor_actions::integrations::objects::Objects::from_env()
                            .ok_or_else(|| {
                                JobFailure::permanent("object storage is not configured")
                            })?;
                        objects.delete(&key).await.map_err(|error| {
                            JobFailure::retryable(format!("object storage: {error}"))
                        })?;
                    }
                    store::document::clear_object_tx(&mut tx, tenant, document_id)
                        .await
                        .map_err(|error| {
                            JobFailure::retryable(format!("clear the document content: {error}"))
                        })?;
                    let mut changed = Vec::new();
                    if had_object {
                        changed.push("object_key");
                    }
                    if had_body {
                        changed.push("body");
                    }
                    let event = store::NewEvent {
                        topic: format!("document:{document_id}"),
                        event_type: "document.updated".to_owned(),
                        aggregate_type: "document".to_owned(),
                        aggregate_id: document_id,
                        payload: serde_json::json!({"id": document_id, "changed": changed}),
                        correlation_id: None,
                        causation_id: None,
                    };
                    store::outbox::append(&mut tx, tenant, &event)
                        .await
                        .map_err(|error| {
                            JobFailure::retryable(format!("append the purge event: {error}"))
                        })?;
                    tracing::info!(tenant = %tenant, document = %document_id, "document.purge");
                }
            }
            tx.commit().await.map_err(db)?;
        }
        // The console's Publish button: build the public pack and open the homepage PR. The job
        // has one attempt (the action enqueues `max_attempts = 1`), so a failure after the PR is
        // opened is dead with the URL in `last_error`, never a second PR.
        "publish.design" => {
            crate::publish::run(pool, tenant, payload).await?;
        }
        "action_invocation.prune" => {
            let deleted = store::prune::action_invocation_prune(
                pool,
                tenant,
                store::prune::ACTION_INVOCATION_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "action_invocation.prune");
        }
        "run.prune" => {
            let deleted = store::prune::run_prune(
                pool,
                tenant,
                store::prune::RUN_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "run.prune");
        }
        "delivery.prune" => {
            let deleted = store::prune::delivery_prune(
                pool,
                tenant,
                store::prune::DELIVERY_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "delivery.prune");
        }
        "idempotency.sweep" => {
            let deleted = store::prune::idempotency_sweep(pool, tenant, store::prune::PRUNE_BATCH)
                .await
                .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "idempotency.sweep");
        }
        "licence.sync" => {
            let Some(client) = licence else {
                return Err(JobFailure::permanent(
                    "LICENCES_BASE_URL is not set; licence.sync cannot run",
                ));
            };
            let outcome = client
                .fetch(tenant, &config.app_key)
                .await
                .map_err(|error| JobFailure::retryable(format!("licence sync: {error}")))?;
            store::licence::upsert(
                pool,
                tenant,
                &config.app_key,
                &store::LicenceUpsert {
                    status: outcome.status.clone(),
                    expires_at: outcome.expires_at,
                    raw: outcome.raw,
                },
            )
            .await
            .map_err(db)?;
            tracing::info!(
                tenant = %tenant,
                status = outcome.status,
                expires_at = ?outcome.expires_at,
                "licence.sync"
            );
        }
        "webhook.deliver" => {
            // The job carries `{subscription_id, body}`; URL and secret are read again at
            // send time, so an update or a deletion takes effect for queued jobs.
            let subscription_id = payload
                .get("subscription_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|raw| uuid::Uuid::parse_str(raw).ok())
                .ok_or_else(|| {
                    JobFailure::permanent("webhook.deliver payload has no usable subscription_id")
                })?;
            let body = payload
                .get("body")
                .cloned()
                .ok_or_else(|| JobFailure::permanent("webhook.deliver payload has no body"))?;
            let Some(target) = store::webhook::delivery_target(pool, tenant, subscription_id)
                .await
                .map_err(|error| match error {
                    store::webhook::WebhookStoreError::Database(error) => db(error),
                    // A missing KEK is configuration: fixable, so the delivery is worth retrying.
                    store::webhook::WebhookStoreError::Secret(
                        store::secret::SecretError::KeyUnavailable,
                    ) => JobFailure::retryable(
                        "webhook secret: the secret key is not configured".to_owned(),
                    ),
                    // An unopenable secret never opens on a retry.
                    other => JobFailure::permanent(format!("webhook secret: {other}")),
                })?
            else {
                return Err(JobFailure::permanent(
                    "the webhook subscription no longer exists in this tenant",
                ));
            };
            if !target.active {
                tracing::info!(
                    webhook = %target.id,
                    "the subscription is inactive; the queued delivery is dropped"
                );
                return Ok(());
            }
            let secret = target
                .secret
                .as_deref()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
            let outcome = conveyor_actions::board::webhooks::deliver_once(
                target.id,
                &target.url,
                secret.as_deref(),
                &body,
            )
            .await;
            if !outcome.ok {
                return Err(JobFailure::retryable(format!(
                    "webhook {} answered {:?}: {}",
                    target.id,
                    outcome.status_code,
                    outcome.error.unwrap_or_default()
                )));
            }
            tracing::info!(
                webhook = %target.id,
                status = ?outcome.status_code,
                duration_ms = outcome.duration_ms,
                "webhook delivered"
            );
        }
        "run.execute" => run_execute(pool, tenant, payload).await?,
        "run.reap" => run_reap(pool, tenant).await?,
        "process.step" => process_step(pool, tenant, &config.app_key, payload).await?,
        // S13: one node attempt of a workflow run (the run's single driving tick).
        "workflow.node" => workflow_node(pool, tenant, &config.app_key, payload).await?,
        // S13 named flows: a schedule starts a published workflow by key. This is the one
        // non-action start path, and it shares `start_published_tx`. (`run.failed` no longer
        // comes through here: S13 §5.8 enqueues `err.card` instead.)
        "workflow.start" => workflow_start(pool, config, tenant, payload).await?,
        // S13 named flows: the old `wip.pull` platform job (a schedule carries the board).
        "wip.pull" => wip_pull(pool, config, tenant, payload).await?,
        // S13 §5.8: a failed run's handler — move the card to waiting, comment, start the coach.
        store::run::ERR_CARD_KIND => err_card(pool, config, tenant, payload).await?,
        // S16: one bounded model call over the deterministic inventory.
        "prompt_audit.run" => {
            crate::prompt_audit_run::run(pool, tenant, &config.app_key, payload).await?
        }
        // S15: one digest per card, enqueued by the terminal run transition (no sweep).
        "memory.consolidate" => crate::memory_consolidate::run(pool, tenant, payload).await?,
        // S19 Part B: one `(case, rep)` of one eval run, and the two supervisors that finalise
        // and abandon it. The driver writes the slot or an `eval_attempt_error`, never both.
        "eval.case.run" => {
            let run_id = payload_uuid(payload, "run_id")?;
            let case_id = payload_uuid(payload, "case_id")?;
            let rep = payload
                .get("rep")
                .and_then(serde_json::Value::as_i64)
                .and_then(|rep| i32::try_from(rep).ok())
                .unwrap_or(0);
            crate::eval_runner::run_case(pool, &config.app_key, tenant, run_id, case_id, rep)
                .await?
        }
        // S20: re-grade one STORED slot under the run's (new) version. No pipeline call — the
        // output comes from the source transcript, so no subject tokens are billed.
        "eval.regrade" => {
            let run_id = payload_uuid(payload, "run_id")?;
            let case_id = payload_uuid(payload, "case_id")?;
            let source_run_id = payload_uuid(payload, "source_run_id")?;
            let source_case_id = payload_uuid(payload, "source_case_id")?;
            let rep = payload
                .get("rep")
                .and_then(serde_json::Value::as_i64)
                .and_then(|rep| i32::try_from(rep).ok())
                .unwrap_or(0);
            crate::eval_runner::regrade_case(
                pool,
                &config.app_key,
                tenant,
                run_id,
                case_id,
                rep,
                source_run_id,
                source_case_id,
            )
            .await?
        }
        // S19's per-output path (migration 0034): one stored output graded once against the
        // eval's current instrument. `eval_grades.create` writes the pending row and queues it.
        "eval.grade" => {
            crate::eval_worker::grade_output(pool, &config.app_key, tenant, payload).await?
        }
        "eval.run.tick" => crate::eval_worker::tick(pool, tenant).await?,
        "eval.reap" => crate::eval_worker::reap(pool, tenant).await?,
        // S19 Part C: one health audit; `eval_audits.start` enqueues it.
        "eval.audit.run" => crate::eval_audit::run(pool, tenant, &config.app_key, payload).await?,
        // The eval transcript is the bulk of a run; 90 days (data-architecture §6).
        "eval.prune" => {
            let deleted = store::prune::eval_trace_prune(
                pool,
                tenant,
                store::prune::EVAL_TRACE_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "eval.prune");
        }
        "inference_call.prune" => {
            let deleted = store::prune::inference_call_prune(
                pool,
                tenant,
                store::prune::INFERENCE_CALL_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "inference_call.prune");
        }
        "prompt_audit.prune" => {
            let deleted = store::prune::prompt_audit_prune(
                pool,
                tenant,
                store::prune::PROMPT_AUDIT_RETENTION_DAYS,
                store::prune::PRUNE_BATCH,
            )
            .await
            .map_err(db)?;
            tracing::info!(tenant = %tenant, deleted, "prompt_audit.prune");
        }
        "card.lease_sweep" => card_lease_sweep(pool, tenant).await?,
        "card.claim" => card_claim(pool, tenant, payload).await?,
        "approval.expire" => approval_expire(pool, tenant).await?,
        "delivery.send" => delivery_send(pool, tenant, payload).await?,
        "approval.closed.emit" => approval_closed_emit(pool, tenant).await?,
        "interaction.dispatch" => interaction_dispatch(pool, tenant).await?,
        "interaction.close" => interaction_close(pool, tenant).await?,
        "quality.score" => quality_score(pool, tenant, payload).await?,
        "model.validate" => model_validate(pool, tenant, payload).await?,
        // S10 — the copilot. The reap and the sweep are schedule rows; the execute kind is one job
        // per approved proposal, queued by the sweep with `dedupe_key = proposal:<id>`.
        "copilot.turn.reap" => copilot_turn_reap(pool, tenant).await?,
        "copilot.proposal.sweep" => copilot_proposal_sweep(pool, tenant).await?,
        "copilot.proposal.execute" => {
            copilot_proposal_execute(pool, config, tenant, payload).await?
        }
        // S17 — one bounded benchmark driver tick, and the reaper that fails a stalled one.
        "benchmark.run.poll" => crate::benchmark_worker::poll(pool, tenant, payload).await?,
        "benchmark.reap" => crate::benchmark_worker::reap(pool, tenant).await?,
        // S18 — the conveyor arm's producer (one case per job) and the improvement loop's verdict.
        "benchmark.case.produce" => {
            crate::benchmark_producer::produce(pool, tenant, &config.app_key, payload).await?
        }
        "benchmark.improvement.run" => {
            crate::benchmark_improvement::run(pool, tenant, &config.app_key, payload).await?
        }
        other => {
            return Err(JobFailure::permanent(format!(
                "unknown job kind {other:?}: this build registers event.prune, job.prune, \
                 model.prune, validation.prune, idempotency.sweep, licence.sync, \
                 webhook.deliver, run.execute, run.reap, process.step, workflow.node, card.lease_sweep, \
                 card.claim, approval.expire, delivery.send, approval.closed.emit, \
                 quality.score, model.validate, copilot.turn.reap, copilot.proposal.sweep, \
                 copilot.proposal.execute, benchmark.run.poll, benchmark.reap, \
                 eval.case.run, eval.regrade, eval.grade, eval.run.tick, eval.reap, \
                 document.purge, publish.design and memory.consolidate"
            )));
        }
    }
    Ok(())
}

// ── S06-1: runs, claims and gates ────────────────────────────────────────────────────────
//
// Each handler writes its state change and its outbox row in ONE tenant-scoped transaction
// (CONTRACT §7), and each is idempotent on `(kind, dedupe_key)` through the job queue: a
// reclaimed job repeats the handler, so a handler that finds the work already done must
// finish quietly rather than emit a second event.

/// True when the tenant's andon cord is pulled.
///
/// A tenant with no row has never pulled the cord (the row is created on first engage), so
/// absence is `false`. A failed read is treated as PULLED: the cord is a safety stop, and the
/// allowed kinds (`delivery.send`, `approval.closed.emit`) still drain, so a database blip
/// pauses ordinary work instead of resuming it behind a cord nobody can read. `runs.create`
/// fails closed on the same state, so the two paths agree.
async fn andon_engaged(pool: &PgPool, tenant: TenantId) -> bool {
    let mut tx = match store::begin_scoped(pool, tenant).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!(%error, tenant = %tenant, "andon: begin failed; pausing the tenant");
            return true;
        }
    };
    let state = store::andon::get_tx(&mut tx, tenant).await;
    let _ = tx.commit().await;
    match state {
        Ok(state) => state.is_some_and(|state| state.engaged),
        Err(error) => {
            tracing::warn!(%error, tenant = %tenant, "andon: read failed; pausing the tenant");
            true
        }
    }
}

/// `run.execute` — start a queued run.
///
/// This is a start hook, not an engine: S11/S13 replace the body with the process or
/// workflow executor. The transition is only `queued → running`; a run that is already
/// running, waiting on a gate, or terminal is finished with no transition and no second
/// `run.started`, which is what makes a reclaimed job safe.
async fn run_execute(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id", "run.execute")?;
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(run) = store::run::get_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(run_failure)?
    else {
        // The run is gone; replaying cannot bring it back.
        tx.commit().await.map_err(retryable)?;
        tracing::warn!(run = %run_id, "run.execute: the run no longer exists");
        return Ok(());
    };
    if run.status != RunStatus::Queued {
        tx.commit().await.map_err(retryable)?;
        tracing::info!(
            run = %run_id,
            status = run.status.as_str(),
            "run.execute: the run already left queued; nothing to start"
        );
        return Ok(());
    }
    match store::run::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Running).await {
        Ok(started) => {
            emit_event(
                &mut tx,
                tenant,
                &run_topic(run_id),
                "run",
                run_id,
                "run.started",
                serde_json::json!({ "run_id": run_id, "attempt": started.attempt }),
            )
            .await?;
        }
        // The state moved between the read and the write: another driver started it. The
        // work is done, so the job succeeds rather than retrying into the same refusal.
        Err(store::run::RunStoreError::Illegal(_)) | Err(store::run::RunStoreError::NotFound) => {
            tracing::info!(run = %run_id, "run.execute: the transition lost the race");
        }
        Err(error) => return Err(run_failure(error)),
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// `run.reap` — fail every run whose driving `run.execute` job died.
///
/// The store closes the runs in the caller's transaction and returns `(run_id, job_id)`;
/// this handler reads each run's attempt for the payload and announces `run.failed` with
/// `error_class='crashed'` (S06 §7 property c). A run is only here because no live job
/// remains, so the announcement cannot race a retry that would start it again.
async fn run_reap(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let reaped = store::run::reap_dead_runs_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    for (run_id, _dead_job_id) in reaped {
        let attempt = match store::run::get_run_tx(&mut tx, tenant, run_id)
            .await
            .map_err(run_failure)?
        {
            Some(run) => run.attempt,
            None => continue,
        };
        emit_event(
            &mut tx,
            tenant,
            &run_topic(run_id),
            "run",
            run_id,
            "run.failed",
            serde_json::json!({
                "run_id": run_id,
                "error_class": "crashed",
                "attempt": attempt,
            }),
        )
        .await?;
        // The estate's old `err.card` error trigger, now a run.failed consumer: the enqueue is
        // idempotent per run, so the several paths that can fail one run start one err.card.
        store::run::enqueue_run_failed_trigger_tx(&mut tx, tenant, run_id)
            .await
            .map_err(retryable)?;
    }

    // S11 step 8: a process run is driven by `process.step` jobs, never by `run.execute`, so
    // the sweep above cannot see one. Without this half a dead step job would leave the run
    // `running` forever and hold `uq_run_live` on its card.
    let reaped = store::process::reap_dead_process_runs_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    for reaped in reaped {
        tracing::warn!(
            run = %reaped.run_id,
            job = %reaped.dead_job_id,
            "run.reap: the process.step job died; the run is failed"
        );
        for step in &reaped.steps {
            emit_event(
                &mut tx,
                tenant,
                &run_topic(reaped.run_id),
                "run",
                reaped.run_id,
                "run_step.changed",
                serde_json::json!({
                    "run_id": reaped.run_id,
                    "position": step.position,
                    "attempt": step.attempt,
                    "status": step.status,
                }),
            )
            .await?;
        }
        emit_event(
            &mut tx,
            tenant,
            &run_topic(reaped.run_id),
            "run",
            reaped.run_id,
            "run.failed",
            serde_json::json!({
                "run_id": reaped.run_id,
                "error_class": "crashed",
                "attempt": reaped.run_attempt,
            }),
        )
        .await?;
        store::run::enqueue_run_failed_trigger_tx(&mut tx, tenant, reaped.run_id)
            .await
            .map_err(retryable)?;
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// `process.step` — advance one step of a process run (S11 §6 step 8).
///
/// The executor owns everything except dispatch: it claims, resolves, invokes and records. This
/// arm only parses the job payload and maps the executor's failure to the job's retry policy.
async fn process_step(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    payload: &Value,
) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id", "process.step")?;
    let position = want_i32(payload, "position", "process.step")?;
    let attempt = want_i32(payload, "attempt", "process.step")?;
    conveyor_actions::process::step::execute_step(pool, tenant, app_key, run_id, position, attempt)
        .await
        .map_err(|failure| {
            if failure.retryable {
                JobFailure::retryable(failure.message)
            } else {
                JobFailure::permanent(failure.message)
            }
        })
}

/// `workflow.start` — start a published workflow by key (the schedule seam).
///
/// The seam exists because a schedule row needs to name a workflow without going through the
/// action transport. The job carries the same fields the action does, and the start itself is
/// [`conveyor_actions::workflow::runs::start_by_key_tx`] — never a second run machine.
async fn workflow_start(
    pool: &PgPool,
    config: &Config,
    tenant: TenantId,
    payload: &Value,
) -> Result<(), JobFailure> {
    const KIND: &str = "workflow.start";
    let workflow_key = payload
        .get("workflow_key")
        .and_then(Value::as_str)
        .ok_or_else(|| JobFailure::permanent(format!("{KIND} payload has no workflow_key")))?;
    let project_id = want_uuid(payload, "project_id", KIND)?;
    let card_id = match payload.get("card_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(raw)) => Some(Uuid::parse_str(raw).map_err(|_| {
            JobFailure::permanent(format!("{KIND} payload has an unusable card_id"))
        })?),
        Some(_) => {
            return Err(JobFailure::permanent(format!(
                "{KIND} payload has an unusable card_id"
            )));
        }
    };
    let inputs = payload.get("inputs").cloned().unwrap_or(Value::Null);
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let ctx = conveyor_actions::workflow::runs::service_ctx(tenant, &config.app_key);
    conveyor_actions::workflow::runs::start_by_key_tx(
        &mut tx,
        &ctx,
        workflow_key,
        project_id,
        card_id,
        inputs,
        None,
    )
    .await
    .map_err(|error| {
        if error.status >= 500 {
            JobFailure::retryable(error.to_string())
        } else {
            JobFailure::permanent(error.to_string())
        }
    })?;
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    Ok(())
}

/// `wip.pull` — the old R346 WIP pull, as a platform job.
///
/// The payload names the board; the selection and every move live in
/// [`conveyor_actions::board::pull::wip_pull`], because a move must go through the action (the
/// matrix, the position and the event), not through a second writer.
async fn wip_pull(
    pool: &PgPool,
    config: &Config,
    tenant: TenantId,
    payload: &Value,
) -> Result<(), JobFailure> {
    let board_id = want_uuid(payload, "board_id", "wip.pull")?;
    let outcome = conveyor_actions::board::pull::wip_pull(pool, &config.app_key, tenant, board_id)
        .await
        .map_err(|error| {
            if error.status >= 500 {
                JobFailure::retryable(error.to_string())
            } else {
                JobFailure::permanent(error.to_string())
            }
        })?;
    tracing::info!(
        %board_id,
        busy = outcome.busy,
        limit = ?outcome.limit,
        moved = outcome.moved,
        skipped = outcome.skipped,
        "wip.pull"
    );
    Ok(())
}

/// `err.card` — the `run.failed` computed handler (S13 §5.8).
///
/// The payload names the failed run; the card, the lane and the coach all resolve from it. The
/// sequence (move → comment → coach) lives in [`conveyor_actions::board::pull::err_card`] so it
/// runs through the real action layer and is idempotent on its own per-run keys. This arm only
/// parses the payload and maps a refusal to the job's retry policy.
async fn err_card(
    pool: &PgPool,
    config: &Config,
    tenant: TenantId,
    payload: &Value,
) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id", store::run::ERR_CARD_KIND)?;
    let outcome = conveyor_actions::board::pull::err_card(pool, &config.app_key, tenant, run_id)
        .await
        .map_err(|error| {
            if error.status >= 500 {
                JobFailure::retryable(error.to_string())
            } else {
                JobFailure::permanent(error.to_string())
            }
        })?;
    tracing::info!(
        %run_id,
        card = ?outcome.card_id,
        moved = outcome.moved,
        commented = outcome.commented,
        coach = ?outcome.coach_run_id,
        coach_skipped = outcome.coach_skipped,
        "err.card"
    );
    Ok(())
}

/// `workflow.node` — advance one attempt of a workflow run (S13 steps 6–7).
///
/// Like `process.step`, the executor owns everything except dispatch: this arm parses the run
/// id and maps the executor's failure to the job's retry policy.
async fn workflow_node(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    payload: &Value,
) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id", "workflow.node")?;
    conveyor_actions::workflow::node::execute_node(pool, tenant, app_key, run_id)
        .await
        .map_err(|failure| {
            if failure.retryable {
                JobFailure::retryable(failure.message)
            } else {
                JobFailure::permanent(failure.message)
            }
        })
}

/// `card.lease_sweep` — clear expired claims and announce each one.
///
/// The store returns `(card_id, claim_fence)`; the board id that the topic needs is read per
/// row, so the event lands on the board's stream (`board:<id>`) and a `card` subscriber sees
/// the lease expire like any other card fact. The row count is bounded by
/// [`LEASE_SWEEP_LIMIT`], so the extra read is bounded too.
async fn card_lease_sweep(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let cleared = store::card::sweep_expired_leases_tx(&mut tx, tenant, LEASE_SWEEP_LIMIT)
        .await
        .map_err(retryable)?;
    for (card_id, claim_fence) in cleared {
        let Some(card) = store::card::get_card_tx(&mut tx, tenant, card_id)
            .await
            .map_err(retryable)?
        else {
            continue;
        };
        emit_event(
            &mut tx,
            tenant,
            &format!("board:{}", card.board_id),
            "card",
            card_id,
            "card.lease_expired",
            serde_json::json!({ "card_id": card_id, "claim_fence": claim_fence }),
        )
        .await?;
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// `card.claim` — the waiter a blocked claimant enqueued.
///
/// The payload is the claim attempt (`{card_id, assignee_id, expected_version, ttl_s}`). The
/// waiter expresses interest in the CARD, not in the state its caller saw: the caller's
/// `expected_version` is already stale (the lease grant that blocked it bumped the row), so
/// this handler re-reads the current version inside the claim transaction and claims against
/// that. A card still under a live lease is retried with backoff — the job was scheduled for
/// the recorded deadline, and a renewal can move it. Any other refusal is a business outcome:
/// the job completes, because someone else won the card.
async fn card_claim(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let card_id = want_uuid(payload, "card_id", "card.claim")?;
    let assignee_id = want_uuid(payload, "assignee_id", "card.claim")?;
    let ttl_s = payload
        .get("ttl_s")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_CLAIM_TTL_SECS);

    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(card) = store::card::get_card_tx(&mut tx, tenant, card_id)
        .await
        .map_err(retryable)?
    else {
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    };
    let live = card.lease_token.is_some()
        && card
            .lease_expires_at
            .is_some_and(|expires| expires > Utc::now());
    if live {
        return Err(JobFailure::retryable(
            "the card is still under a live lease",
        ));
    }
    let current_version = card.version;
    match store::card::claim_card_tx(
        &mut tx,
        tenant,
        card_id,
        assignee_id,
        ttl_s,
        current_version,
    )
    .await
    {
        Ok(Some(granted)) => {
            let Some(card) = store::card::get_card_tx(&mut tx, tenant, card_id)
                .await
                .map_err(retryable)?
            else {
                tx.commit().await.map_err(retryable)?;
                return Ok(());
            };
            emit_event(
                &mut tx,
                tenant,
                &format!("board:{}", card.board_id),
                "card",
                card_id,
                "card.claimed",
                serde_json::json!({ "card_id": card_id, "claim_fence": granted.claim_fence }),
            )
            .await?;
        }
        Ok(None) => {
            tracing::info!(card = %card_id, "card.claim: refused; the waiter completes");
        }
        Err(store::card::CardStoreError::Database(error)) => {
            return Err(JobFailure::retryable(format!("database: {error}")));
        }
        Err(error) => return Err(JobFailure::permanent(error.to_string())),
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// `approval.expire` — resolve due gates, then close the runs they leave waiting.
///
/// A `waiting_approval` run whose last gate expired can never move: the domain has no
/// `waiting_approval → failed` edge, and leaving it live would hold `uq_run_live` forever,
/// so it is cancelled with `error_class='approval_expired'` (S06 §5.4). Both announcements
/// are written in the same transaction as the writes.
async fn approval_expire(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let expired = store::approval::expire_due_ids_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    for (approval_id, run_id) in expired {
        emit_event(
            &mut tx,
            tenant,
            &format!("approval:{approval_id}"),
            "approval",
            approval_id,
            "approval.expired",
            serde_json::json!({ "approval_id": approval_id, "run_id": run_id }),
        )
        .await?;

        let Some(run_id) = run_id else {
            continue;
        };
        // S11: a process gate's expiry applies the gated step's `on_result` (which may continue
        // the run). Only a generic gate falls through to the cancel below.
        if store::approval::process_gate_tx(&mut tx, tenant, approval_id)
            .await
            .map_err(retryable)?
            .is_some()
        {
            conveyor_actions::process::step::resolve_gate(&mut tx, tenant, approval_id)
                .await
                .map_err(process_gate_failure)?;
            continue;
        }
        // S13: a workflow gate's expiry is terminal. Record the gate's row before the generic
        // still-waiting branch cancels the run, so the trace ends with the gate that failed it.
        if let Some(gate) = store::approval::workflow_gate_tx(&mut tx, tenant, approval_id)
            .await
            .map_err(retryable)?
            && let (Some(node_id), Some(attempt)) = (gate.node_id, gate.attempt)
        {
            conveyor_actions::workflow::node::record_gate_row(
                &mut tx, tenant, run_id, &node_id, attempt, false,
            )
            .await
            .map_err(|failure| {
                if failure.retryable {
                    JobFailure::retryable(failure.message)
                } else {
                    JobFailure::permanent(failure.message)
                }
            })?;
        }
        let still_waiting = store::run::get_run_tx(&mut tx, tenant, run_id)
            .await
            .map_err(run_failure)?
            .is_some_and(|run| run.status == RunStatus::WaitingApproval);
        if !still_waiting {
            continue;
        }
        match store::run::cancel_run_tx(&mut tx, tenant, run_id, "approval_expired").await {
            Ok(Some(_)) => {
                emit_event(
                    &mut tx,
                    tenant,
                    &run_topic(run_id),
                    "run",
                    run_id,
                    "run.cancelled",
                    serde_json::json!({ "run_id": run_id, "reason": "approval_expired" }),
                )
                .await?;
            }
            Ok(None) => {}
            Err(error) => return Err(run_failure(error)),
        }
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

// ── S06-2: comms and notifications ──────────────────────────────────────────────────────
//
// `delivery.send` is the one handler that deliberately splits its work across transactions:
// the attempt is stamped and committed, the channel is spoken to OUTSIDE any transaction,
// and the outcome is written afterwards (§5.6). Holding the row lock across the send would
// make one slow channel an estate-wide stall, and the dedupe key already makes a replay safe.

/// `delivery.send` — one attempt at one delivery.
///
/// The state machine is §3.6's: `pending → sent → delivered | failed | expired`, with retries
/// as re-attempts of a `sent` row. A terminal row is finished quietly (at-least-once). An
/// expired row is closed as `expired` before any transport is tried. `web` is `delivered` at
/// the SSE acknowledgement this handler stands in for; `mcp` stays `sent` because S06a's
/// signed channel owns the wire; `telegram` is posted to the Bot API and lands `delivered` on
/// a 2xx. Every other kind has no transport in this build, so each attempt fails and the
/// budget turns the row `failed` (§7 property e).
async fn delivery_send(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let delivery_id = want_uuid(payload, "delivery_id", "delivery.send")?;

    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(delivery) = store::delivery::lock_delivery_tx(&mut tx, tenant, delivery_id)
        .await
        .map_err(delivery_failure)?
    else {
        tx.commit().await.map_err(retryable)?;
        tracing::warn!(delivery = %delivery_id, "delivery.send: the delivery no longer exists");
        return Ok(());
    };
    if delivery.state.is_terminal() {
        tx.commit().await.map_err(retryable)?;
        tracing::info!(
            delivery = %delivery_id,
            state = delivery.state.as_str(),
            "delivery.send: the delivery is terminal; nothing to do"
        );
        return Ok(());
    }
    if delivery
        .expires_at
        .is_some_and(|expires_at| expires_at <= Utc::now())
    {
        let expired = store::delivery::mark_expired_tx(&mut tx, tenant, delivery_id)
            .await
            .map_err(delivery_failure)?
            .ok_or_else(|| JobFailure::permanent("delivery.send: the locked delivery vanished"))?;
        emit_delivery_changed(&mut tx, tenant, &expired).await?;
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    }
    let Some(sent) = store::delivery::record_send_attempt_tx(&mut tx, tenant, delivery_id)
        .await
        .map_err(delivery_failure)?
    else {
        // The lock was held, so this is a concurrent finisher that closed the row between the
        // read and the write; the job's work is done either way.
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    };
    tx.commit().await.map_err(retryable)?;

    match sent.channel_kind.as_str() {
        // The web channel is confirmed at the SSE acknowledgement (S06 §5.6); the drainer's
        // hand-off is the delivery.
        "web" => {
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
            let Some(delivered) = store::delivery::mark_delivered_tx(&mut tx, tenant, delivery_id)
                .await
                .map_err(delivery_failure)?
            else {
                tx.commit().await.map_err(retryable)?;
                return Ok(());
            };
            emit_delivery_changed(&mut tx, tenant, &delivered).await?;
            tx.commit().await.map_err(retryable)?;
            Ok(())
        }
        // MCP has no push transport: its pending interactions are read through `tasks/get`.
        // The row stays `sent`, which is exactly "handed to the channel" in §3.6.
        "mcp" => {
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
            emit_delivery_changed(&mut tx, tenant, &sent).await?;
            tx.commit().await.map_err(retryable)?;
            Ok(())
        }
        // Every other kind is served by a connector adapter (S06d).
        _ => connector_send(pool, tenant, &sent).await,
    }
}

/// `delivery.send` for a transport-backed connector (S06d).
///
/// The binding is named by `recipient_ref = binding:<id>`. A legacy delivery with a person
/// reference falls back to the first active binding of its kind (the shape `messages.send`
/// wrote before this slice). One scoped read resolves the binding, the message and every
/// named secret; the HTTP call runs outside that transaction, so no row lock is held across
/// the network.
async fn connector_send(
    pool: &PgPool,
    tenant: TenantId,
    delivery: &Delivery,
) -> Result<(), JobFailure> {
    // The kind's transport is a build fact, not tenant configuration: refuse it before any row
    // is read, so an unregistered kind fails with the honest reason.
    let Some(spec) = conveyor_actions::channel::connectors::find(&delivery.channel_kind) else {
        return fail_delivery_terminal(
            pool,
            tenant,
            delivery.id,
            format!(
                "no transport is registered for channel kind {:?}; the delivery cannot be sent \
                 by this build",
                delivery.channel_kind
            ),
        )
        .await;
    };
    if !spec.capabilities.can_send {
        return fail_delivery_terminal(
            pool,
            tenant,
            delivery.id,
            format!(
                "connector kind {:?} does not send; its interactions are read through the app",
                spec.kind
            ),
        )
        .await;
    }

    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let binding = match delivery.recipient_ref.strip_prefix("binding:") {
        Some(raw) => match Uuid::parse_str(raw) {
            Ok(id) => store::channel::get_binding_tx(&mut tx, tenant, id)
                .await
                .map_err(retryable)?,
            Err(_) => None,
        },
        None => {
            let bindings = store::channel::list_bindings_tx(&mut tx, tenant)
                .await
                .map_err(retryable)?;
            bindings
                .into_iter()
                .find(|b| b.active && b.channel_kind == delivery.channel_kind)
        }
    };
    let Some(binding) = binding.filter(|binding| binding.active) else {
        tx.commit().await.map_err(retryable)?;
        return fail_delivery_terminal(
            pool,
            tenant,
            delivery.id,
            format!(
                "the tenant has no active binding for channel {:?}; the delivery cannot be addressed",
                delivery.channel_kind
            ),
        )
        .await;
    };
    if spec.kind != binding.channel_kind {
        tx.commit().await.map_err(retryable)?;
        return fail_delivery_terminal(
            pool,
            tenant,
            delivery.id,
            format!(
                "the delivery's channel {:?} does not match its binding's kind {:?}",
                delivery.channel_kind, binding.channel_kind
            ),
        )
        .await;
    }
    let Some(text) = store::channel::message_body_tx(&mut tx, tenant, delivery.message_id)
        .await
        .map_err(retryable)?
    else {
        tx.commit().await.map_err(retryable)?;
        return fail_delivery_terminal(
            pool,
            tenant,
            delivery.id,
            "the delivery's message no longer exists; there is nothing to send".to_owned(),
        )
        .await;
    };
    let secrets = match conveyor_actions::channel::connectors::resolve_secrets_tx(
        &mut tx,
        tenant,
        spec,
        &binding.config,
    )
    .await
    {
        Ok(secrets) => secrets,
        Err(error) => {
            tx.commit().await.map_err(retryable)?;
            return fail_delivery_terminal(
                pool,
                tenant,
                delivery.id,
                format!("the binding's connector secrets are unavailable: {error}"),
            )
            .await;
        }
    };
    tx.commit().await.map_err(retryable)?;

    let outcome = (spec.send)(conveyor_actions::channel::connectors::SendRequest {
        config: binding.config.clone(),
        secrets,
        text,
    })
    .await;
    if !outcome.ok {
        let error = outcome
            .error
            .unwrap_or_else(|| "the connector send failed".to_owned());
        return fail_delivery_send(pool, tenant, delivery.id, error).await;
    }

    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(delivered) = store::delivery::mark_delivered_tx(&mut tx, tenant, delivery.id)
        .await
        .map_err(delivery_failure)?
    else {
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    };
    emit_delivery_changed(&mut tx, tenant, &delivered).await?;
    tx.commit().await.map_err(retryable)?;
    tracing::info!(
        delivery = %delivery.id,
        kind = %spec.kind,
        status = ?outcome.status_code,
        "connector delivery delivered"
    );
    Ok(())
}

/// Terminalise a delivery whose failure retrying cannot fix.
///
/// An unknown channel kind, a missing bot token, an unaddressable binding: no later attempt
/// can succeed, so the row must be closed here. Leaving it `sent` would strand it forever —
/// `sent` is not terminal, the retention sweep never prunes it, and no job remains to close
/// it — while the job itself dead-letters. One attempt against a budget of one forces the
/// terminal `failed`, and the state change is announced like any other.
async fn fail_delivery_terminal(
    pool: &PgPool,
    tenant: TenantId,
    delivery_id: Uuid,
    error: String,
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(row) =
        store::delivery::record_failed_attempt_tx(&mut tx, tenant, delivery_id, &error, 1)
            .await
            .map_err(delivery_failure)?
    else {
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    };
    emit_delivery_changed(&mut tx, tenant, &row).await?;
    tx.commit().await.map_err(retryable)?;
    tracing::warn!(delivery = %delivery_id, %error, "delivery.send: permanent failure");
    Err(JobFailure::permanent(error))
}

/// Record one failed attempt and schedule the retry on this same job row.
///
/// The attempt counter was stamped before the transport ran; this only writes
/// `last_error` (and the terminal `failed` once the budget is spent). Below the budget it
/// returns the retry as a [`JobFailure`] — `fail` puts the SAME job back to `pending` with
/// `run_after = now + backoff`, because the dedupe key is still held by the row that is
/// running and a second job would be refused.
async fn fail_delivery_send(
    pool: &PgPool,
    tenant: TenantId,
    delivery_id: Uuid,
    error: String,
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(row) = store::delivery::record_failed_attempt_tx(
        &mut tx,
        tenant,
        delivery_id,
        &error,
        store::delivery::DELIVERY_MAX_ATTEMPTS,
    )
    .await
    .map_err(delivery_failure)?
    else {
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    };
    if row.state.is_terminal() {
        emit_delivery_changed(&mut tx, tenant, &row).await?;
        tx.commit().await.map_err(retryable)?;
        tracing::warn!(
            delivery = %delivery_id,
            attempts = row.attempts,
            "delivery.send: the attempt budget is spent; the delivery is failed"
        );
        return Ok(());
    }
    let retry_after = delivery_backoff(row.attempts);
    tx.commit().await.map_err(retryable)?;
    Err(JobFailure {
        message: error,
        permanent: false,
        retry_after,
    })
}

/// `approval.closed.emit` — hand every newly closed gate to the tenant's mcp channel.
///
/// One message and one delivery per closed approval, deduped by the `NOT EXISTS` on a
/// `message(kind='approval-closed', correlation_id=approval.id)` — the message IS the record
/// that the close was announced. The advisory lock makes two replicas run the sweep one at a
/// time; the `NOT EXISTS` makes the second one a no-op even if they overlap.
async fn approval_closed_emit(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let lock_key = format!("conveyor:approval.closed:{}", tenant.as_uuid());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
        .map_err(retryable)?;

    let bindings = store::channel::list_bindings_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    let Some(binding) = bindings
        .into_iter()
        .find(|binding| binding.active && binding.channel_kind == "mcp")
    else {
        // No mcp route: nothing can be handed off, and writing a message anyway would leave
        // a row the tenant can never receive. The next tick re-checks the bindings.
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    };

    let candidates =
        store::delivery::approval_closed_candidates_tx(&mut tx, tenant, APPROVAL_CLOSED_BATCH)
            .await
            .map_err(retryable)?;
    for (approval_id, status) in candidates {
        let message = store::delivery::insert_message_tx(
            &mut tx,
            tenant,
            &store::delivery::NewMessage {
                channel_kind: "mcp".to_owned(),
                // No person sent this; the system did.
                sender_id: None,
                kind: APPROVAL_CLOSED_KIND.to_owned(),
                body: format!("approval {approval_id} closed: {status}"),
                meta: serde_json::json!({ "approval_id": approval_id, "status": status }),
                correlation_id: Some(approval_id),
            },
        )
        .await
        .map_err(delivery_failure)?;
        let inserted = store::delivery::insert_delivery_tx(
            &mut tx,
            tenant,
            &store::delivery::NewDelivery::new(message.id, "mcp", &binding.route_key),
        )
        .await
        .map_err(delivery_failure)?;
        store::delivery::enqueue_delivery_send_tx(&mut tx, tenant, inserted.delivery.id, None)
            .await
            .map_err(retryable)?;
        tracing::info!(
            approval = %approval_id,
            status,
            message = %message.id,
            "approval.closed.emit: the closed gate is queued to the mcp channel"
        );
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// `interaction.dispatch` — multicast one pending approval to every eligible channel (S06d).
///
/// An interaction is an approval. One tick fans it out to (a) active machine bindings whose
/// connector can send and whose tenant grant still allows `approvals.decide`, and (b) human
/// members whose role/policy holds the response scope (`approvals:write`, or `evals:approve`
/// for a sign-off). The `interaction_dispatch` primary key is the fan-out idempotency, and the
/// advisory lock serialises the sweep so two replicas cannot both decide a recipient is new.
async fn interaction_dispatch(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let lock_key = format!("conveyor:interaction.dispatch:{}", tenant.as_uuid());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
        .map_err(retryable)?;

    let approvals = store::interaction::pending_approvals_tx(&mut tx, tenant, INTERACTION_BATCH)
        .await
        .map_err(retryable)?;
    if approvals.is_empty() {
        tx.commit().await.map_err(retryable)?;
        return Ok(());
    }
    let bindings = store::channel::list_bindings_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    let web = store::interaction::web_recipients_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    let policy_rows = store::interaction::role_policy_tx(&mut tx, tenant)
        .await
        .map_err(retryable)?;
    let policy: BTreeMap<(String, String), bool> = policy_rows
        .into_iter()
        .map(|(role, scope, allowed)| ((role, scope), allowed))
        .collect();

    // One authority per machine kind, loaded once for this tick.
    let mut machine: Vec<(
        &'static conveyor_actions::channel::connectors::ConnectorSpec,
        ChannelAuthority,
    )> = Vec::new();
    for spec in conveyor_actions::channel::connectors::machine_kinds() {
        if !spec.capabilities.can_send {
            continue;
        }
        let overrides = store::rbac::channel_policy_tx(&mut tx, tenant, spec.kind)
            .await
            .map_err(retryable)?;
        machine.push((spec, ChannelAuthority::new(spec.kind, None, overrides)));
    }

    for approval in &approvals {
        let already: HashSet<(String, String)> =
            store::interaction::dispatched_recipients_tx(&mut tx, tenant, approval.id)
                .await
                .map_err(retryable)?
                .into_iter()
                .collect();

        // Machine connectors: a sign-off is human-only and is never offered to them.
        if approval.eval_id.is_none() {
            for (spec, authority) in &machine {
                let Some(decide) = conveyor_actions::registry().get("approvals.decide") else {
                    continue;
                };
                if !decide.permits_channel(authority) {
                    continue;
                }
                for binding in bindings
                    .iter()
                    .filter(|binding| binding.active && binding.channel_kind == spec.kind)
                {
                    let recipient = format!("binding:{}", binding.id);
                    if already.contains(&(spec.kind.to_owned(), recipient.clone())) {
                        continue;
                    }
                    offer_interaction(
                        &mut tx,
                        tenant,
                        approval,
                        spec.kind,
                        &recipient,
                        Some(binding.id),
                    )
                    .await?;
                }
            }
        }

        // Human web inboxes. A sign-off needs `evals:approve`; an ordinary gate
        // `approvals:write`. The tenant policy is applied, not just the compiled default.
        let scope = if approval.eval_id.is_some() {
            conveyor_actions::Scope::EvalsApprove
        } else {
            conveyor_actions::Scope::ApprovalsWrite
        };
        for user in &web {
            let Some(role) = Role::parse(&user.role) else {
                continue;
            };
            let allowed = policy
                .get(&(role.as_str().to_owned(), scope.as_str().to_owned()))
                .copied()
                .unwrap_or_else(|| scope.granted_to(role));
            if !allowed {
                continue;
            }
            // The web inbox's key is the bare `app_user.id` (`notifications.list` compares
            // `recipient_ref` to the caller id); a `user:` prefix would never match it.
            let recipient = user.user_id.to_string();
            if already.contains(&("web".to_owned(), recipient.clone())) {
                continue;
            }
            offer_interaction(&mut tx, tenant, approval, "web", &recipient, None).await?;
        }
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// Insert one offer's message, delivery and dispatch row in the caller's transaction.
async fn offer_interaction(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    approval: &store::interaction::PendingApproval,
    channel_kind: &str,
    recipient_ref: &str,
    binding_id: Option<Uuid>,
) -> Result<(), JobFailure> {
    let message = store::delivery::insert_message_tx(
        conn,
        tenant,
        &store::delivery::NewMessage {
            channel_kind: channel_kind.to_owned(),
            sender_id: None,
            kind: INTERACTION_REQUESTED_KIND.to_owned(),
            body: store::interaction::interaction_text(approval),
            meta: serde_json::json!({
                "approval_id": approval.id,
                "gate": approval.gate,
                "kind": approval.kind,
                "binding_id": binding_id,
            }),
            correlation_id: Some(approval.id),
        },
    )
    .await
    .map_err(delivery_failure)?;
    let inserted = store::delivery::insert_delivery_tx(
        conn,
        tenant,
        &store::delivery::NewDelivery::new(message.id, channel_kind, recipient_ref),
    )
    .await
    .map_err(delivery_failure)?;
    store::delivery::enqueue_delivery_send_tx(conn, tenant, inserted.delivery.id, None)
        .await
        .map_err(retryable)?;
    store::interaction::insert_dispatch_tx(
        conn,
        tenant,
        approval.id,
        channel_kind,
        recipient_ref,
        binding_id,
        message.id,
    )
    .await
    .map_err(retryable)?;
    Ok(())
}

/// `interaction.close` — expire the losing recipients and tell them it was answered elsewhere.
///
/// The decision itself already happened (the approval's status guard is the first-response
/// arbiter). This sweep only closes the remaining offers: every live delivery of the offer's
/// message becomes `expired` with the resolving channel in `last_error`, and the recipient
/// gets one closure notice. `closed_notified_at` is the per-dispatch idempotency.
async fn interaction_close(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let lock_key = format!("conveyor:interaction.close:{}", tenant.as_uuid());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
        .map_err(retryable)?;

    let dispatches = store::interaction::open_dispatches_tx(&mut tx, tenant, INTERACTION_BATCH)
        .await
        .map_err(retryable)?;
    for dispatch in dispatches {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_uuid())
                .bind(dispatch.approval_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(retryable)?;
        let Some(status) = status else {
            // The approval was pruned; the dispatch cascades with it on a real delete, so this
            // is only a read race. Leave the row for the next tick.
            continue;
        };
        let resolver = store::interaction::resolved_via_tx(&mut tx, tenant, dispatch.approval_id)
            .await
            .map_err(retryable)?;
        let reason = format!(
            "superseded: resolved via {}",
            resolver.as_deref().unwrap_or("another channel")
        );
        let expired = store::delivery::expire_live_for_message_tx(
            &mut tx,
            tenant,
            dispatch.message_id,
            &reason,
        )
        .await
        .map_err(delivery_failure)?;
        for delivery in &expired {
            emit_delivery_changed(&mut tx, tenant, delivery).await?;
        }
        if let Some(spec) = conveyor_actions::channel::connectors::find(&dispatch.channel_kind)
            && spec.capabilities.can_send
        {
            let message = store::delivery::insert_message_tx(
                &mut tx,
                tenant,
                &store::delivery::NewMessage {
                    channel_kind: dispatch.channel_kind.clone(),
                    sender_id: None,
                    kind: INTERACTION_CLOSED_KIND.to_owned(),
                    body: store::interaction::close_text(
                        dispatch.approval_id,
                        &status,
                        resolver.as_deref(),
                        true,
                    ),
                    meta: serde_json::json!({
                        "approval_id": dispatch.approval_id,
                        "status": status,
                        "binding_id": dispatch.binding_id,
                    }),
                    correlation_id: Some(dispatch.approval_id),
                },
            )
            .await
            .map_err(delivery_failure)?;
            let inserted = store::delivery::insert_delivery_tx(
                &mut tx,
                tenant,
                &store::delivery::NewDelivery::new(
                    message.id,
                    &dispatch.channel_kind,
                    &dispatch.recipient_ref,
                ),
            )
            .await
            .map_err(delivery_failure)?;
            store::delivery::enqueue_delivery_send_tx(&mut tx, tenant, inserted.delivery.id, None)
                .await
                .map_err(retryable)?;
        }
        store::interaction::mark_dispatch_notified_tx(
            &mut tx,
            tenant,
            dispatch.approval_id,
            &dispatch.channel_kind,
            &dispatch.recipient_ref,
        )
        .await
        .map_err(retryable)?;
    }
    tx.commit().await.map_err(retryable)?;
    Ok(())
}

/// The backoff after a failed attempt: `min(2^attempts · 5 s, 900 s)` (S06 §5.6).
fn delivery_backoff(attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.max(0)).unwrap_or(0).min(16);
    let secs = (1u64 << exponent)
        .saturating_mul(5)
        .min(DELIVERY_BACKOFF_CAP_SECS);
    Duration::from_secs(secs)
}

/// `delivery.changed` on the tenant-wide topic — the inbox subscription (§5.6).
async fn emit_delivery_changed(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    delivery: &Delivery,
) -> Result<(), JobFailure> {
    emit_event(
        conn,
        tenant,
        "tenant",
        "delivery",
        delivery.id,
        "delivery.changed",
        serde_json::json!({
            "delivery_id": delivery.id,
            "message_id": delivery.message_id,
            "state": delivery.state.as_str(),
        }),
    )
    .await
}

/// A delivery-store failure: a database error is retryable, a refused transition is not.
fn delivery_failure(error: store::DeliveryStoreError) -> JobFailure {
    match error {
        store::DeliveryStoreError::Database(error) => {
            JobFailure::retryable(format!("database: {error}"))
        }
        other => JobFailure::permanent(other.to_string()),
    }
}

// ── S07: quality scoring ────────────────────────────────────────────────────────────────
//
// The rubric is advisory: this handler reads an immutable revision and its asset, appends
// one `quality_evaluation`, and announces it. It never writes `governed_asset` — not its
// status, not its version, not its `updated_at` — so a score can never block a transition
// (S07 §5.5). Re-scoring appends a second row; the history is the point.

/// `quality.score` — rubric v1 over one revision, written only to `quality_evaluation`.
///
/// One scoped transaction: load the revision (a missing one cannot come back, so it is
/// permanent), refuse anything that is not a governed-asset revision, load the asset for the
/// similarity read, count the asset's trace links, run the pure rubric, append the
/// evaluation and its `quality.evaluated` event. The score binds as a [`Decimal`] because
/// the column is `numeric`.
async fn quality_score(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let revision_id = want_uuid(payload, "revision_id", "quality.score")?;
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;
    let Some(revision) = store::revision::get_revision_tx(&mut tx, tenant, revision_id)
        .await
        .map_err(revision_failure)?
    else {
        tx.rollback().await.map_err(retryable)?;
        return Err(JobFailure::permanent(format!(
            "quality.score: revision {revision_id} does not exist in this tenant"
        )));
    };
    if revision.entity_type != store::governance::GOVERNED_ASSET_ENTITY_TYPE {
        tx.rollback().await.map_err(retryable)?;
        return Err(JobFailure::permanent(format!(
            "quality.score: revision {revision_id} has entity type {:?}; S07 scores governed \
             assets only",
            revision.entity_type
        )));
    }
    let Some(asset) = store::governance::get_asset_tx(&mut tx, tenant, revision.entity_id)
        .await
        .map_err(governance_failure)?
    else {
        // A revision is immutable and an asset is never deleted, so this is a corrupt pair,
        // not a race: replaying cannot conjure the asset.
        tx.rollback().await.map_err(retryable)?;
        return Err(JobFailure::permanent(format!(
            "quality.score: the asset {} behind revision {revision_id} does not exist",
            revision.entity_id
        )));
    };

    let (trace_links, trace_links_to_card) =
        store::quality::count_for_asset_tx(&mut tx, tenant, asset.id)
            .await
            .map_err(quality_failure)?;
    let max_similarity = store::quality::max_similarity_tx(
        &mut tx,
        tenant,
        asset.kind.as_str(),
        &asset.title,
        asset.id,
    )
    .await
    .map_err(quality_failure)?;

    let input = RubricInput {
        title: snapshot_text(&revision.content, "title"),
        body: snapshot_text(&revision.content, "body"),
        acceptance: acceptance_entries(revision.content.get("acceptance")),
        trace_links,
        trace_links_to_card,
        max_similarity,
    };
    let QualityScore {
        score: value,
        criteria,
    } = quality::rubric(&input);

    let mut evaluation = NewQualityEvaluation::new("revision", revision.id);
    evaluation.score = Decimal::from_f64_retain(value);
    evaluation.criteria = criteria;
    evaluation.evaluator_kind = "heuristic".to_owned();
    let row = store::quality::insert_evaluation_tx(&mut tx, tenant, &evaluation)
        .await
        .map_err(quality_failure)?;

    emit_event(
        &mut tx,
        tenant,
        &format!("revision:{revision_id}"),
        "quality_evaluation",
        row.id,
        "quality.evaluated",
        serde_json::json!({
            "revision_id": revision_id,
            "score": value,
            "rubric": quality::RUBRIC_VERSION,
        }),
    )
    .await?;
    tx.commit().await.map_err(retryable)?;
    tracing::info!(
        revision = %revision_id,
        score = value,
        evaluation = %row.id,
        "quality.score"
    );
    Ok(())
}

/// `model.validate` — run the model's profile checks over its graph and record the run.
///
/// Post-commit by construction: `models.validate` enqueued this job in the same transaction as
/// the graph read that produced its dedupe key, and the whole graph walk happens here so it
/// never holds the model's row lock (slice trap 6). One run row, one `model.validated` event,
/// one transaction: a failed check still commits an `ok`/`findings` run, because the run IS
/// the result — only a database error leaves nothing behind.
async fn model_validate(
    pool: &PgPool,
    tenant: TenantId,
    payload: &Value,
) -> Result<(), JobFailure> {
    let model_id = want_uuid(payload, "model_id", "model.validate")?;
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(retryable)?;

    let Some(model) = store::model::get_model_tx(&mut tx, tenant, model_id)
        .await
        .map_err(model_failure)?
    else {
        tx.rollback().await.map_err(retryable)?;
        return Err(JobFailure::permanent(format!(
            "model.validate: model {model_id} does not exist in this tenant"
        )));
    };
    let profile = conveyor_domain::profile::for_language(&model.language, &model.language_version)
        .map_err(|error| JobFailure::permanent(format!("model.validate: {error}")))?;
    let snapshot = store::model::load_snapshot_tx(&mut tx, tenant, model_id)
        .await
        .map_err(model_failure)?;
    let content_hash = snapshot.content_hash();

    // Every governed asset the elements claim, resolved in one in-tenant query: the validator
    // is pure, so the worker hands it the tenant's truth rather than letting it ask.
    let mut asset_ids: Vec<Uuid> = snapshot
        .elements
        .iter()
        .filter_map(|element| element.meta.get("governed_asset_id"))
        .filter_map(Value::as_str)
        .filter_map(|raw| Uuid::parse_str(raw).ok())
        .collect();
    asset_ids.sort_unstable();
    asset_ids.dedup();
    let mut assets = std::collections::HashMap::new();
    if !asset_ids.is_empty() {
        let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, kind, title FROM governed_asset WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(tenant.as_uuid())
        .bind(&asset_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(retryable)?;
        for (id, kind, title) in rows {
            assets.insert(
                id,
                conveyor_domain::validate::GovernedAssetRef { kind, title },
            );
        }
    }

    let findings = conveyor_domain::validate::check(&snapshot, profile, &assets);
    let status = if findings
        .iter()
        .any(|finding| finding.severity == conveyor_domain::validate::Severity::Error)
    {
        "findings"
    } else {
        "ok"
    };
    let run = store::validation::insert_run_tx(
        &mut tx,
        tenant,
        &store::validation::NewValidationRun {
            model_id,
            revision_id: model.published_revision_id,
            ruleset: profile.ruleset.to_owned(),
            ruleset_version: profile.ruleset_version.to_owned(),
            status: status.to_owned(),
            findings: serde_json::to_value(&findings).unwrap_or(Value::Array(Vec::new())),
            content_hash: content_hash.clone(),
        },
    )
    .await
    .map_err(model_failure)?;

    emit_event(
        &mut tx,
        tenant,
        &format!("model:{model_id}"),
        "model",
        model_id,
        "model.validated",
        serde_json::json!({
            "model_id": model_id,
            "run_id": run.id,
            "status": status,
        }),
    )
    .await?;
    tx.commit().await.map_err(retryable)?;
    tracing::info!(
        model = %model_id,
        run = %run.id,
        status,
        findings = findings.len(),
        "model.validate"
    );
    Ok(())
}

/// A model-store failure: a database error is retryable, anything else is not.
fn model_failure(error: conveyor_store::model::ModelStoreError) -> JobFailure {
    match error {
        conveyor_store::model::ModelStoreError::Database(error) => {
            JobFailure::retryable(format!("database: {error}"))
        }
        other => JobFailure::permanent(other.to_string()),
    }
}

/// A string member of the revision snapshot; an absent or non-string one contributes
/// nothing rather than failing the job (the rubric treats an empty value as deficient).
fn snapshot_text(content: &Value, field: &str) -> String {
    content
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The rubric's `acceptance` entries from the revision snapshot.
///
/// `revision.content` is `jsonb` with no shape constraint; the documented shape is an array
/// of entry objects, and a lone object is one entry. A scalar or `null` contributes nothing,
/// which the rubric scores as "no testable criterion" rather than inventing one.
fn acceptance_entries(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(items)) => items.clone(),
        Some(other) if !other.is_null() => vec![other.clone()],
        _ => Vec::new(),
    }
}

/// A revision-store failure: a database error is retryable, an unparsable stored status is not.
fn revision_failure(error: RevisionStoreError) -> JobFailure {
    match error {
        RevisionStoreError::Database(error) => JobFailure::retryable(format!("database: {error}")),
        other => JobFailure::permanent(other.to_string()),
    }
}

/// A governance-store failure: a database error is retryable, everything else is not.
fn governance_failure(error: GovernanceStoreError) -> JobFailure {
    match error {
        GovernanceStoreError::Database(error) => {
            JobFailure::retryable(format!("database: {error}"))
        }
        other => JobFailure::permanent(other.to_string()),
    }
}

/// A quality-store failure: a database error is retryable, everything else is not.
fn quality_failure(error: QualityStoreError) -> JobFailure {
    match error {
        QualityStoreError::Database(error) => JobFailure::retryable(format!("database: {error}")),
        other => JobFailure::permanent(other.to_string()),
    }
}

/// The `run:<id>` aggregate topic (CONTRACT §7).
fn run_topic(run_id: Uuid) -> String {
    format!("run:{run_id}")
}

/// Read a uuid member from a job payload; a missing or malformed one is a permanent failure
/// (retrying cannot repair the stored JSON).
fn want_uuid(payload: &Value, field: &str, kind: &str) -> Result<Uuid, JobFailure> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent(format!("{kind} payload has no usable {field}")))
}

/// A positive integer payload field (`process.step`'s `position`/`attempt`).
fn want_i32(payload: &Value, field: &str, kind: &str) -> Result<i32, JobFailure> {
    payload
        .get(field)
        .and_then(Value::as_i64)
        .and_then(|raw| i32::try_from(raw).ok())
        .filter(|raw| *raw >= 1)
        .ok_or_else(|| JobFailure::permanent(format!("{kind} payload has no usable {field}")))
}

/// A database failure is retryable; every other store refusal is not.
fn retryable(error: sqlx::Error) -> JobFailure {
    JobFailure::retryable(format!("database: {error}"))
}

/// A gate-resume failure: the executor's own retryable/permanent split, preserved.
fn process_gate_failure(failure: conveyor_actions::process::step::StepFailure) -> JobFailure {
    if failure.retryable {
        JobFailure::retryable(failure.message)
    } else {
        JobFailure::permanent(failure.message)
    }
}

/// A run-store failure: a database error is retryable, a refused state transition is not.
fn run_failure(error: store::run::RunStoreError) -> JobFailure {
    match error {
        store::run::RunStoreError::Database(error) => {
            JobFailure::retryable(format!("database: {error}"))
        }
        other => JobFailure::permanent(other.to_string()),
    }
}

/// Append one event (and its `pg_notify` doorbell) on the caller's transaction.
async fn emit_event(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    topic: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), JobFailure> {
    store::outbox::append(
        conn,
        tenant,
        &store::outbox::NewEvent {
            topic: topic.to_owned(),
            event_type: event_type.to_owned(),
            aggregate_type: aggregate_type.to_owned(),
            aggregate_id,
            payload,
            correlation_id: None,
            causation_id: None,
        },
    )
    .await
    .map(|_| ())
    .map_err(|error| match error {
        store::outbox::OutboxError::Database(error) => {
            JobFailure::retryable(format!("database: {error}"))
        }
        other => JobFailure::permanent(format!("event: {other}")),
    })
}

// ── S10: the copilot ───────────────────────────────────────────────────────────────────────

/// `copilot.turn.reap` — a `streaming` turn older than 300 s becomes `interrupted`, and its
/// `conversation_turn.created` event is written with it (S10 §5.4). Nothing to reap is the normal
/// case: this runs every minute.
async fn copilot_turn_reap(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let reaped =
        conveyor_api::copilot::turns::reap(pool, tenant, conveyor_domain::copilot::REAP_AFTER_SECS)
            .await
            .map_err(JobFailure::retryable)?;
    if reaped > 0 {
        tracing::info!(reaped, "copilot: streaming turns marked interrupted");
    }
    Ok(())
}

/// `copilot.proposal.sweep` — queue one `copilot.proposal.execute` per approved, unexecuted
/// proposal. The job's dedupe key is `proposal:<id>`, so two sweeps in one tick queue one job.
async fn copilot_proposal_sweep(pool: &PgPool, tenant: TenantId) -> Result<(), JobFailure> {
    let queued = conveyor_api::copilot::proposal::sweep(pool, tenant, 100)
        .await
        .map_err(|error| match error {
            conveyor_api::copilot::proposal::ExecutionError::Database(error) => {
                JobFailure::retryable(format!("database: {error}"))
            }
            conveyor_api::copilot::proposal::ExecutionError::Store(error) => {
                JobFailure::retryable(format!("store: {error}"))
            }
            conveyor_api::copilot::proposal::ExecutionError::Action(error) => {
                JobFailure::permanent(format!("action: {error}"))
            }
            // A seal that does not open is a configuration/data fault, not a transient one:
            // retrying reproduces it, and a person must act.
            conveyor_api::copilot::proposal::ExecutionError::Secret(error) => {
                JobFailure::permanent(format!("proposal seal: {error}"))
            }
            conveyor_api::copilot::proposal::ExecutionError::Unreadable(detail) => {
                JobFailure::permanent(format!("proposal seal: {detail}"))
            }
        })?;
    if queued > 0 {
        tracing::info!(queued, "copilot: approved proposals queued for execution");
    }
    Ok(())
}

/// `copilot.proposal.execute` — run one approved proposal with the proposer's scopes and
/// `Idempotency-Key: proposal:<id>` (S10-D3). A 4xx from the action is permanent: retrying it
/// would reproduce the same refusal.
async fn copilot_proposal_execute(
    pool: &PgPool,
    config: &Config,
    tenant: TenantId,
    payload: &serde_json::Value,
) -> Result<(), JobFailure> {
    let Some(approval_id) = payload
        .get("approval_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| raw.parse().ok())
    else {
        return Err(JobFailure::permanent(
            "copilot.proposal.execute: payload needs approval_id",
        ));
    };
    match conveyor_api::copilot::proposal::execute(pool, &config.app_key, tenant, approval_id).await
    {
        Ok(_) => Ok(()),
        Err(error) if error.retryable() => Err(JobFailure::retryable(error.to_string())),
        Err(error) => Err(JobFailure::permanent(error.to_string())),
    }
}

// ── the bus consumer (CONTRACT §7) ───────────────────────────────────────────────────────────
//
// The second limb of the engine. The `process.step` job arm above is the fallback that survives
// a Redis loss; this task is the nudge that follows `run_step.requested` from the outbox to the
// executor. Both call the SAME `execute_step`, and it is idempotent on
// `(run_id, position, attempt)`, so a step delivered by both limbs runs its effect once: the
// claim lets exactly one caller own the `run_step` row and the loser is a replay of the stored
// input. Delivery is at-least-once and an unacked entry is re-delivered by `XAUTOCLAIM`, so the
// only durable outcome either limb may rely on is the one Postgres recorded.

/// How long the consumer sleeps between passes over the active tenants.
pub const BUS_TICK: Duration = Duration::from_secs(1);

/// The most entries one `XREADGROUP` or one `XAUTOCLAIM` takes.
pub const BUS_BATCH: usize = 10;

/// How often the consumer reclaims the entries a dead consumer left pending.
pub const BUS_AUTOCLAIM: Duration = Duration::from_secs(30);

/// The idle time after which a pending entry is reclaimable.
///
/// A step that legitimately runs longer than this can be claimed from a live consumer; that is
/// safe because the executor's claim makes the second delivery a replay, and it is preferable
/// to an entry that a crashed consumer left pending forever.
pub const BUS_MIN_IDLE_MS: u64 = 60_000;

/// `<hostname>-<pid>` — the id a consumer group records an unacked entry against.
///
/// A restart gets a new pid, so the entries the old process left pending become claimable by
/// `XAUTOCLAIM` instead of waiting behind one immortal consumer name.
pub fn bus_consumer_name() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .map(|host| host.trim().to_owned())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "conveyor".to_owned());
    format!("{host}-{}", std::process::id())
}

/// Start the bus consumer in the background — a no-op when `REDIS_URL` is unset.
///
/// Unset is the Postgres-only deployment: no task starts, and the `process.step` job arm in
/// [`Worker::run`] is the only driver. Set, both limbs run and the job remains the fallback.
pub fn spawn_bus_consumer(pool: PgPool, app_key: String) {
    let Some(url) = crate::bus_relay::redis_url() else {
        tracing::info!(
            kind = store::process::PROCESS_STEP_KIND,
            "REDIS_URL is not set: the bus consumer does not start; the job arm is the only driver"
        );
        return;
    };
    let consumer = bus_consumer_name();
    tokio::spawn(async move {
        tokio::select! {
            () = consume_forever(pool, app_key, url, consumer) => {},
            () = shutdown::signal() => tracing::info!("bus consumer stopped"),
        }
    });
}

/// Why one consumer pass stopped.
#[derive(Debug, thiserror::Error)]
enum BusConsumerError {
    /// The tenant enumeration or the executor failed; the next pass retries.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// Redis is unreachable; the task reconnects and the group's cursor is unchanged.
    #[error("bus: {0}")]
    Bus(#[from] conveyor_bus::BusError),
}

/// Connect, consume, reconnect — forever. The `select!` in [`spawn_bus_consumer`] ends it.
async fn consume_forever(pool: PgPool, app_key: String, url: String, consumer: String) {
    // Groups are ensured once per tenant per process; a tenant created later is picked up by the
    // next `active_ids` read and its group created then.
    let mut grouped: HashSet<Uuid> = HashSet::new();
    let mut last_claim = std::time::Instant::now();
    loop {
        let bus = match Bus::connect(&url).await {
            Ok(bus) => bus,
            Err(error) => {
                tracing::warn!(%error, "the bus consumer cannot reach Redis; retrying");
                tokio::time::sleep(crate::bus_relay::RELAY_RECONNECT).await;
                continue;
            }
        };
        tracing::info!(consumer = %consumer, "bus consumer connected");
        loop {
            let pass = consume_once(
                &pool,
                &app_key,
                &bus,
                &consumer,
                &mut grouped,
                &mut last_claim,
            )
            .await;
            match pass {
                Ok(()) => tokio::time::sleep(BUS_TICK).await,
                Err(BusConsumerError::Database(error)) => {
                    tracing::warn!(%error, "the bus consumer could not read; retrying");
                    tokio::time::sleep(BUS_TICK).await;
                }
                Err(BusConsumerError::Bus(error)) => {
                    tracing::warn!(%error, "the bus consumer lost Redis; reconnecting");
                    break;
                }
            }
        }
    }
}

/// One pass over every active tenant: new entries, then (every [`BUS_AUTOCLAIM`]) the entries a
/// dead consumer left pending.
async fn consume_once(
    pool: &PgPool,
    app_key: &str,
    bus: &Bus,
    consumer: &str,
    grouped: &mut HashSet<Uuid>,
    last_claim: &mut std::time::Instant,
) -> Result<(), BusConsumerError> {
    let tenants = store::tenants::active_ids(pool).await?;
    let claim = last_claim.elapsed() >= BUS_AUTOCLAIM;
    for tenant in tenants {
        let stream = stream_for(tenant.as_uuid(), GROUP_PROCESS);
        // Marked known only AFTER the group exists: an ensure that failed must be retried on the
        // next pass, or every later read answers NOGROUP and the consumer can never recover.
        if !grouped.contains(&tenant.as_uuid()) {
            bus.ensure_group(&stream, GROUP_PROCESS).await?;
            grouped.insert(tenant.as_uuid());
        }
        for (entry, envelope) in read_group(bus, &stream, consumer).await? {
            deliver(pool, app_key, bus, &stream, &entry, envelope, tenant).await?;
        }
        if claim {
            for (entry, envelope) in autoclaim(bus, &stream, consumer).await? {
                deliver(pool, app_key, bus, &stream, &entry, envelope, tenant).await?;
            }
        }
    }
    if claim {
        *last_claim = std::time::Instant::now();
    }
    Ok(())
}

/// One `XREADGROUP` of new entries, with an undecodable entry acked rather than returned.
async fn read_group(
    bus: &Bus,
    stream: &str,
    consumer: &str,
) -> Result<Vec<(String, Envelope)>, BusConsumerError> {
    let read = bus
        .read_group(stream, GROUP_PROCESS, consumer, BUS_BATCH, None)
        .await;
    match read {
        Ok(entries) => Ok(entries),
        Err(error) => discard_undecodable(bus, stream, error).await,
    }
}

/// One `XAUTOCLAIM` of idle pending entries, with the same undecodable-entry disposal.
async fn autoclaim(
    bus: &Bus,
    stream: &str,
    consumer: &str,
) -> Result<Vec<(String, Envelope)>, BusConsumerError> {
    let claimed = bus
        .autoclaim(stream, GROUP_PROCESS, consumer, BUS_MIN_IDLE_MS, BUS_BATCH)
        .await;
    match claimed {
        Ok(entries) => Ok(entries),
        Err(error) => discard_undecodable(bus, stream, error).await,
    }
}

/// An entry the bus cannot decode has no envelope to dead-letter, so it is acked and logged.
///
/// The alternative — returning the error forever — poisons every later read of the group: the
/// entry is already past `>` and sits in the PEL, so it would be reclaimed, fail, and reclaim
/// again. The bus's own docs put this decision with the caller precisely because only the
/// caller knows what it can do without an envelope.
async fn discard_undecodable(
    bus: &Bus,
    stream: &str,
    error: conveyor_bus::BusError,
) -> Result<Vec<(String, Envelope)>, BusConsumerError> {
    let entry = match &error {
        conveyor_bus::BusError::MissingEnvelope { entry_id, .. }
        | conveyor_bus::BusError::Envelope { entry_id, .. } => entry_id.clone(),
        _ => return Err(error.into()),
    };
    tracing::error!(stream, entry = %entry, %error, "an undecodable bus entry is discarded");
    bus.ack(stream, GROUP_PROCESS, &entry).await?;
    Ok(Vec::new())
}

/// Handle one delivered entry: execute the step it names, then ack — or leave it pending.
///
/// The ack rules, in the order they matter:
/// - a retryable executor failure is **never** acked, so `XAUTOCLAIM` re-delivers it and the
///   delivery ceiling below eventually dead-letters a poison entry;
/// - a permanent (handled) failure and a success are both acked: the executor is idempotent on
///   `(run_id, position, attempt)` and the record is Postgres;
/// - `card.moved` is followed by [`follow_card_move`], which acks everything except a transient
///   `wip.pull` failure (that one stays pending, under the same ceiling);
/// - any other topic is a notification this group does not execute, and is acked so the PEL
///   stays the retry queue rather than an ever-growing read log.
async fn deliver(
    pool: &PgPool,
    app_key: &str,
    bus: &Bus,
    stream: &str,
    entry: &str,
    envelope: Envelope,
    tenant: TenantId,
) -> Result<(), BusConsumerError> {
    if envelope.tenant_id != tenant.as_uuid() {
        tracing::error!(
            stream,
            entry,
            envelope_tenant = %envelope.tenant_id,
            tenant = %tenant,
            "an envelope names another tenant: dead-lettered, never executed"
        );
        bus.dead_letter(
            &envelope,
            "the envelope tenant_id disagrees with its stream",
        )
        .await?;
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    }

    // S13 §5.8: a card entering the ACTIVE lane is the old `wip.pull` trigger. This is the one
    // notification the consumer acts on; every other topic below is acked.
    if envelope.topic == crate::bus_relay::TOPIC_CARD_MOVED {
        return follow_card_move(pool, app_key, bus, stream, entry, &envelope, tenant).await;
    }

    let Some((run_id, position, attempt)) = step_target(&envelope) else {
        // A notification for this group (or a `work.requested` this build cannot dispatch).
        if envelope.topic == TOPIC_WORK_REQUESTED {
            tracing::error!(stream, entry, topic = %envelope.topic, "work.requested names no runnable process.step");
            bus.dead_letter(&envelope, "work.requested names no runnable process.step")
                .await?;
        }
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    };

    // Only a step request can stay pending across deliveries (everything else is acked on
    // first sight), so the ceiling is read where it can actually be reached.
    let deliveries = bus.deliveries(stream, GROUP_PROCESS, entry).await?;
    if deliveries >= MAX_DELIVERIES {
        tracing::error!(
            stream,
            entry,
            deliveries,
            "a bus entry hit the delivery ceiling"
        );
        let reason = format!("delivered {deliveries} times without an acknowledgement");
        bus.dead_letter(&envelope, &reason).await?;
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    }

    let outcome = conveyor_actions::process::step::execute_step(
        pool, tenant, app_key, run_id, position, attempt,
    )
    .await;
    match outcome {
        Ok(()) => {
            bus.ack(stream, GROUP_PROCESS, entry).await?;
            Ok(())
        }
        Err(failure) if failure.retryable => {
            tracing::warn!(
                run = %run_id,
                position,
                attempt,
                error = %failure.message,
                "a bus step failed retryably; the entry stays pending for the autoclaim path"
            );
            Ok(())
        }
        Err(failure) => {
            tracing::warn!(
                run = %run_id,
                position,
                attempt,
                error = %failure.message,
                "a bus step failed permanently"
            );
            bus.ack(stream, GROUP_PROCESS, entry).await?;
            Ok(())
        }
    }
}

/// `card.moved` → `wip.pull` (S13 §5.8): a card entering the ACTIVE lane is the old trigger.
///
/// The event names the lane it landed in, so the lane row supplies both the kind and the board
/// — nothing is guessed from the payload. Only an entry into `active` pulls: a move out, into
/// another kind, or a reorder inside one lane is a notification and is acked untouched. The pull
/// itself is the same [`conveyor_actions::board::pull::wip_pull`] the job arm runs, and it is
/// idempotent by construction (it moves at most `wip_limit − active_count` cards, so a replay
/// finds no slot).
async fn follow_card_move(
    pool: &PgPool,
    app_key: &str,
    bus: &Bus,
    stream: &str,
    entry: &str,
    envelope: &Envelope,
    tenant: TenantId,
) -> Result<(), BusConsumerError> {
    let Some(to_list_id) = envelope
        .payload
        .get("to_list_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        tracing::error!(
            stream,
            entry,
            "card.moved names no usable to_list_id; discarded"
        );
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    };
    // A reorder inside one lane is not an entry: the move names the same lane twice.
    if envelope.payload.get("from_list_id") == envelope.payload.get("to_list_id") {
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    }

    let mut tx = store::begin_scoped(pool, tenant).await?;
    let lane = store::list::get_list_record_tx(&mut tx, tenant, to_list_id)
        .await
        .map_err(|error| match error {
            store::list::ListStoreError::Database(error) => error,
            other => sqlx::Error::Protocol(other.to_string()),
        })?;
    tx.commit().await?;
    let Some(lane) = lane else {
        tracing::error!(
            stream,
            entry,
            %to_list_id,
            "card.moved names a lane that does not exist; discarded"
        );
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    };
    if lane.list.kind != ListKind::Active {
        bus.ack(stream, GROUP_PROCESS, entry).await?;
        return Ok(());
    }

    match conveyor_actions::board::pull::wip_pull(pool, app_key, tenant, lane.list.board_id).await {
        Ok(outcome) => {
            tracing::info!(
                board = %lane.list.board_id,
                busy = outcome.busy,
                limit = ?outcome.limit,
                moved = outcome.moved,
                skipped = outcome.skipped,
                "card.moved: wip.pull"
            );
            bus.ack(stream, GROUP_PROCESS, entry).await?;
        }
        // A transient failure is retried by the autoclaim path, bounded by the same ceiling a
        // step request has: a poison entry is dead-lettered instead of reclaimed forever.
        Err(error) if error.status >= 500 => {
            let deliveries = bus.deliveries(stream, GROUP_PROCESS, entry).await?;
            if deliveries >= MAX_DELIVERIES {
                tracing::error!(
                    stream,
                    entry,
                    deliveries,
                    "a card.moved entry hit the delivery ceiling"
                );
                let reason = format!("delivered {deliveries} times without an acknowledgement");
                bus.dead_letter(envelope, &reason).await?;
                bus.ack(stream, GROUP_PROCESS, entry).await?;
            } else {
                tracing::warn!(
                    %error,
                    board = %lane.list.board_id,
                    "wip.pull failed; the move stays pending for the autoclaim path"
                );
            }
        }
        // A refusal is a real answer (no intake lane, the andon cord): ack it.
        Err(error) => {
            tracing::warn!(
                %error,
                board = %lane.list.board_id,
                "wip.pull was refused"
            );
            bus.ack(stream, GROUP_PROCESS, entry).await?;
        }
    }
    Ok(())
}

/// `(run_id, position, attempt)` when the envelope is the `process.step` job this build runs.
///
/// The job document is the dispatch key (`{"kind":"process.step"}`) and the payload carries the
/// step's identity — never its resolved input, which the executor claims and records itself.
fn step_target(envelope: &Envelope) -> Option<(Uuid, i32, i32)> {
    let kind = envelope.job.as_ref()?.get("kind")?.as_str()?;
    if kind != store::process::PROCESS_STEP_KIND {
        return None;
    }
    let run_id = envelope
        .payload
        .get("run_id")?
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())?;
    let position = i32::try_from(envelope.payload.get("position")?.as_i64()?).ok()?;
    let attempt = i32::try_from(envelope.payload.get("attempt")?.as_i64()?).ok()?;
    Some((run_id, position, attempt))
}

#[cfg(test)]
mod bus_consumer_tests {
    use super::*;
    use serde_json::json;

    /// A `work.requested` envelope as the relay builds it from `run_step.requested`.
    fn step_envelope(payload: serde_json::Value, job: serde_json::Value) -> Envelope {
        Envelope {
            id: Uuid::now_v7(),
            topic: TOPIC_WORK_REQUESTED.to_owned(),
            from: "run".to_owned(),
            correlation_id: None,
            procedure: None,
            tenant_id: Uuid::now_v7(),
            job: Some(job),
            payload,
        }
    }

    #[test]
    fn a_process_step_job_yields_the_executor_triple() {
        let run_id = Uuid::now_v7();
        let envelope = step_envelope(
            json!({"run_id": run_id, "position": 2, "attempt": 1}),
            json!({"kind": "process.step"}),
        );
        assert_eq!(step_target(&envelope), Some((run_id, 2, 1)));
    }

    /// A job kind this build does not execute is not a step, even on `work.requested`.
    #[test]
    fn another_job_kind_is_not_a_step() {
        let envelope = step_envelope(
            json!({"run_id": Uuid::now_v7(), "position": 1, "attempt": 1}),
            json!({"kind": "tool.call"}),
        );
        assert_eq!(step_target(&envelope), None);
    }

    /// A notification carries no job document and must never be executed.
    #[test]
    fn an_envelope_without_a_job_is_not_a_step() {
        let mut envelope = step_envelope(
            json!({"run_id": Uuid::now_v7(), "position": 1, "attempt": 1}),
            json!({"kind": "process.step"}),
        );
        envelope.job = None;
        assert_eq!(step_target(&envelope), None);
    }

    /// A malformed step payload is not a step: the caller dead-letters it rather than
    /// executing a guessed position.
    #[test]
    fn an_incomplete_payload_is_not_a_step() {
        let envelope = step_envelope(
            json!({"run_id": Uuid::now_v7()}),
            json!({"kind": "process.step"}),
        );
        assert_eq!(step_target(&envelope), None);

        let bad_id = step_envelope(
            json!({"run_id": "not-a-uuid", "position": 1, "attempt": 1}),
            json!({"kind": "process.step"}),
        );
        assert_eq!(step_target(&bad_id), None);
    }

    /// The name is a host plus this process's pid, so a restart's leftovers are claimable.
    #[test]
    fn the_consumer_name_carries_the_pid() {
        let name = bus_consumer_name();
        assert!(
            name.ends_with(&format!("-{}", std::process::id())),
            "{name}"
        );
        assert!(!name.starts_with('-'), "{name}");
    }
}
