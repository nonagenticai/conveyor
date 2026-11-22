#![forbid(unsafe_code)]
//! The run-level executor runtime (SPEC_V2 §4.4/§4.5; S21 step 4, issue #581, ADR 0010).
//!
//! `run.execute` used to be a bare `queued → running` hook. It is now the one dispatch point.
//! A run's binding names its `executor_kind`; the T0 [`Registry`] validates the binding, and a
//! [`RunExecutor`] implementation serves the kind. Adding a kind is one implementation plus one
//! [`ExecutorRuntime::register`] call — this module, the worker and the run store never match on
//! a kind.
//!
//! **Every refusal is terminal and recorded before any model, tool or laya call**: a retired
//! binding, a stale pin, a retired release, a tampered snapshot, an unknown kind or a binding
//! that lacks its executor's configuration finishes the run `failed` with a typed `error_class`
//! and returns `Ok`, so the job is never retried into the same refusal. Only a database or
//! transport failure is retryable, under the same idempotency prefix.

pub mod internal_agent;
pub mod procedure;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use conveyor_auth::Role;
use conveyor_domain::RunStatus;
use conveyor_domain::TenantId;
use conveyor_domain::executor::{BindingConfig, ExecutorKind, Registry};
use conveyor_domain::organisation::{ReleaseAnatomy, parse_release_anatomy, release_hash};
use conveyor_store as store;
use conveyor_store::organisation::BindingRow;
use conveyor_store::run::RunContext;
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::control::runs::emit_run_event;
use crate::invoke::HandlerCtx;

/// Why `run.execute` could not finish.
///
/// `retryable` is true only for a database or transport failure: the run stays `running` and the
/// job is retried under the same idempotency prefix. Every typed refusal is recorded on the run
/// and returns success.
#[derive(Clone, Debug)]
pub struct ExecutorFailure {
    /// Whether the queue may retry the same job.
    pub retryable: bool,
    /// What happened, for the job ledger.
    pub detail: String,
}

impl ExecutorFailure {
    /// A failure the queue should retry (the run stays `running`).
    pub fn retryable(detail: impl Into<String>) -> Self {
        Self {
            retryable: true,
            detail: detail.into(),
        }
    }

    /// A permanent failure (the queue dead-letters; the run's record is authoritative).
    pub fn permanent(detail: impl Into<String>) -> Self {
        Self {
            retryable: false,
            detail: detail.into(),
        }
    }
}

/// One bound run handed to its executor, after every pin check has passed.
pub struct RunRequest<'a> {
    /// The pool the handler uses outside its resolution transaction.
    pub pool: &'a PgPool,
    /// The tenant.
    pub tenant: TenantId,
    /// `APP_KEY` — the entitlement key the handler's invocations carry.
    pub app_key: &'a str,
    /// The run, already moved to `running`.
    pub run: RunContext,
    /// The binding that pinned it.
    pub binding: BindingRow,
    /// The binding's frozen release anatomy.
    pub anatomy: ReleaseAnatomy,
    /// The tenant's service actor (the loop acts as it, exactly as `agent.run` does).
    pub actor_id: Option<Uuid>,
}

/// One executor kind's run-level implementation.
pub trait RunExecutor: Send + Sync {
    /// The kind this implementation serves.
    fn kind(&self) -> ExecutorKind;

    /// Execute one bound run. A typed refusal must finish the run itself and return `Ok`.
    fn execute<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorFailure>> + Send + 'a>>;
}

/// The kinds this build serves, keyed by slug.
pub struct ExecutorRuntime {
    handlers: BTreeMap<ExecutorKind, Box<dyn RunExecutor>>,
}

impl ExecutorRuntime {
    /// The two live kinds (SPEC_V2 §4.5).
    pub fn builtin() -> Self {
        let mut runtime = Self {
            handlers: BTreeMap::new(),
        };
        runtime
            .register(Box::new(procedure::ProcedureExecutor))
            .expect("the built-in kinds are distinct");
        runtime
            .register(Box::new(internal_agent::InternalAgentExecutor))
            .expect("the built-in kinds are distinct");
        runtime
    }

    /// Add one implementation. A duplicate kind is refused, not silently replaced.
    pub fn register(&mut self, executor: Box<dyn RunExecutor>) -> Result<(), ExecutorKind> {
        let kind = executor.kind();
        if self.handlers.contains_key(&kind) {
            return Err(kind);
        }
        self.handlers.insert(kind, executor);
        Ok(())
    }

    /// The implementation for `kind`, or `None` when this build serves none.
    pub fn require(&self, kind: &ExecutorKind) -> Option<&dyn RunExecutor> {
        self.handlers.get(kind).map(Box::as_ref)
    }

    /// Every served kind, in slug order.
    pub fn kinds(&self) -> impl Iterator<Item = &str> {
        self.handlers.keys().map(ExecutorKind::as_str)
    }
}

/// The service context an event appended by the runtime carries.
pub(crate) fn service_ctx(
    tenant: TenantId,
    actor_id: Option<Uuid>,
    app_key: &str,
    action: &'static str,
) -> HandlerCtx {
    HandlerCtx {
        tenant,
        actor_id: actor_id.unwrap_or(Uuid::nil()),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        channel: None,
        request_id: None,
        version: None,
        if_match: None,
        action,
        app_key: app_key.to_owned(),
        approval_id: None,
    }
}

/// Finish a run `failed` with one typed class and announce it, in the caller's transaction.
///
/// The `err.card` trigger is the existing failed-run path (the coach and the card block); a
/// refusal is a real failure of the run, not an outage, so it must feed that path.
pub(crate) async fn finish_failed(
    conn: &mut PgConnection,
    tenant: TenantId,
    app_key: &str,
    run_id: Uuid,
    error_class: &str,
    detail: &str,
) -> Result<(), ExecutorFailure> {
    let ctx = service_ctx(tenant, None, app_key, "run.execute");
    store::run::fail_run_tx(&mut *conn, tenant, run_id, Some(error_class), Some(detail))
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    emit_run_event(
        &mut *conn,
        &ctx,
        run_id,
        "run.failed",
        json!({ "run_id": run_id, "error_class": error_class, "detail": detail }),
        serde_json::Value::Null,
    )
    .await
    .map_err(|error| ExecutorFailure::retryable(error.detail))?;
    if let Err(error) = store::run::enqueue_run_failed_trigger_tx(&mut *conn, tenant, run_id).await
    {
        tracing::warn!(%error, run = %run_id, "run.execute: enqueue the err.card trigger");
    }
    Ok(())
}

/// Finish a run `succeeded` and announce it, in the caller's transaction.
pub(crate) async fn finish_succeeded(
    conn: &mut PgConnection,
    tenant: TenantId,
    app_key: &str,
    run_id: Uuid,
) -> Result<(), ExecutorFailure> {
    let ctx = service_ctx(tenant, None, app_key, "run.execute");
    match store::run::transition_run_tx(&mut *conn, tenant, run_id, RunStatus::Succeeded).await {
        Ok(_) => {
            emit_run_event(
                &mut *conn,
                &ctx,
                run_id,
                "run.succeeded",
                json!({ "run_id": run_id }),
                serde_json::Value::Null,
            )
            .await
            .map_err(|error| ExecutorFailure::retryable(error.detail))?;
        }
        Err(store::run::RunStoreError::Illegal(_) | store::run::RunStoreError::NotFound) => {
            tracing::info!(run = %run_id, "run.execute: the run moved before it could succeed");
        }
        Err(error) => {
            return Err(ExecutorFailure::retryable(format!("database: {error}")));
        }
    }
    Ok(())
}

/// Execute one `run.execute` job.
///
/// Reads the run, keeps the T0 start hook for an unbound run, and for a bound run re-checks every
/// pin before dispatching. See the module docs for the failure taxonomy.
pub async fn execute_run(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    run_id: Uuid,
) -> Result<(), ExecutorFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let Some(run) = store::run::get_run_context_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?
    else {
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        tracing::warn!(run = %run_id, "run.execute: the run no longer exists");
        return Ok(());
    };
    if run.status != RunStatus::Queued {
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        tracing::info!(
            run = %run_id,
            status = run.status.as_str(),
            "run.execute: the run already left queued; nothing to start"
        );
        return Ok(());
    }

    // The T0 start hook: a run with no organisation pin is a bare `queued → running` task, which
    // is what `projects.execute` creates. It keeps working exactly as before this unit.
    let Some(binding_id) = run.job_binding_id else {
        let ctx = service_ctx(tenant, None, app_key, "run.execute");
        match store::run::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Running).await {
            Ok(started) => {
                emit_run_event(
                    &mut tx,
                    &ctx,
                    run_id,
                    "run.started",
                    json!({ "run_id": run_id, "attempt": started.attempt }),
                    serde_json::Value::Null,
                )
                .await
                .map_err(|error| ExecutorFailure::retryable(error.detail))?;
            }
            Err(store::run::RunStoreError::Illegal(_) | store::run::RunStoreError::NotFound) => {}
            Err(error) => {
                return Err(ExecutorFailure::retryable(format!("database: {error}")));
            }
        }
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    };

    // ── the binding's pin, re-checked before anything is dispatched ─────────────────────────
    let binding = match store::organisation::job_binding_get_tx(&mut tx, tenant, binding_id).await {
        Ok(binding) => binding,
        Err(store::organisation::OrganisationStoreError::NotFound(_)) => {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "job_binding_stale",
                &format!("the run pins binding {binding_id}, which no longer exists"),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
        Err(error) => {
            return Err(ExecutorFailure::retryable(format!("database: {error}")));
        }
    };
    let refusal = if binding.status != "active" {
        Some((
            "job_binding_stale",
            format!("the binding is {:?}, not active", binding.status),
        ))
    } else if run.release_hash.as_deref() != Some(binding.release_hash.as_str()) {
        Some((
            "job_binding_stale",
            "the run's release pin is not the binding's".to_owned(),
        ))
    } else if release_hash(&binding.release_snapshot) != binding.release_hash {
        Some((
            "job_binding_stale",
            "the binding's snapshot does not hash to its release hash".to_owned(),
        ))
    } else {
        None
    };
    if let Some((class, detail)) = refusal {
        finish_failed(&mut tx, tenant, app_key, run_id, class, &detail).await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    match store::organisation::job_release_get_tx(&mut tx, tenant, binding.release_id).await {
        Ok(release) if release.status == "published" => {}
        Ok(release) => {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "job_binding_stale",
                &format!("the pinned release is {:?}", release.status),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
        Err(store::organisation::OrganisationStoreError::NotFound(_)) => {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "job_binding_stale",
                "the pinned release no longer exists",
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
        Err(error) => {
            return Err(ExecutorFailure::retryable(format!("database: {error}")));
        }
    }
    let anatomy = match parse_release_anatomy(&binding.release_snapshot) {
        Ok(anatomy) => anatomy,
        Err(error) => {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "capability_not_ready",
                &format!("the pinned release's anatomy is unusable: {error}"),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
    };
    if let Some(declared) = anatomy.executor_kind.as_ref()
        && declared.as_str() != binding.executor_kind
    {
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "job_binding_stale",
            &format!(
                "the binding names {:?} but its release declares {:?}",
                binding.executor_kind,
                declared.as_str()
            ),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    let kind = match ExecutorKind::parse(&binding.executor_kind) {
        Ok(kind) => kind,
        Err(_) => {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "executor_unknown_kind",
                &format!("{:?} is not a valid executor kind", binding.executor_kind),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
    };
    if let Err(error) = Registry::builtin().validate(&BindingConfig {
        executor_kind: kind.clone(),
        process_id: binding.process_id,
        capability_agent_id: binding.capability_agent_id,
    }) {
        let class = match error {
            conveyor_domain::executor::BindingError::UnknownKind(_) => "executor_unknown_kind",
            conveyor_domain::executor::BindingError::MissingProcess => "executor_kind_unsupported",
            _ => "capability_not_ready",
        };
        finish_failed(&mut tx, tenant, app_key, run_id, class, &error.to_string()).await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    // The work package is the card (SPEC_V2 §10.2): an internal agent with no card has no request
    // and no memory scope to compile from.
    if kind == ExecutorKind::internal_agent() && run.card_id.is_none() {
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "capability_not_ready",
            "an internal_agent run must execute on a card",
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    let runtime = ExecutorRuntime::builtin();
    if runtime.require(&kind).is_none() {
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "executor_unknown_kind",
            &format!("this build serves no {kind} executor"),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }

    // The run is valid: start it, then dispatch outside the resolution transaction.
    let ctx = service_ctx(tenant, None, app_key, "run.execute");
    match store::run::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Running).await {
        Ok(started) => {
            emit_run_event(
                &mut tx,
                &ctx,
                run_id,
                "run.started",
                json!({ "run_id": run_id, "attempt": started.attempt }),
                serde_json::Value::Null,
            )
            .await
            .map_err(|error| ExecutorFailure::retryable(error.detail))?;
        }
        Err(store::run::RunStoreError::Illegal(_) | store::run::RunStoreError::NotFound) => {
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
        Err(error) => {
            return Err(ExecutorFailure::retryable(format!("database: {error}")));
        }
    }
    tx.commit()
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;

    let actor_id = store::identity::tenant_actor(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let handler = runtime.require(&kind).expect("the kind was required above");
    handler
        .execute(RunRequest {
            pool,
            tenant,
            app_key,
            run,
            binding,
            anatomy,
            actor_id,
        })
        .await
}
