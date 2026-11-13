//! The operator action path.
//!
//! `conveyor admin action invoke` — and the admin commands built on it — call the **same**
//! engine as `POST /api/actions/<name>/invocations`: no second dispatcher and no raw SQL
//! mutation. The actor is the tenant's own first active member (the `board-export` importer's
//! rule), `actor_kind = "service"`, and the synthetic machine role. Validation, preconditions,
//! events and audit rows are therefore the ones a real caller gets.

use conveyor_actions::invoke::{Authority, InvokeRequest, invoke};
use conveyor_actions::{ActionKind, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// A caller bound to one tenant.
pub struct ActionCaller<'a> {
    pool: &'a PgPool,
    tenant: TenantId,
    actor: Uuid,
    app_key: &'a str,
}

impl<'a> ActionCaller<'a> {
    /// Resolve the tenant's actor once. A tenant with no active member has no operator.
    pub async fn new(
        pool: &'a PgPool,
        tenant: TenantId,
        app_key: &'a str,
    ) -> Result<Self, CallError> {
        let actor = store::identity::tenant_actor(pool, tenant)
            .await?
            .ok_or(CallError::NoActor)?;
        Ok(Self {
            pool,
            tenant,
            actor,
            app_key,
        })
    }

    /// Invoke one action and return its `output`. `_version` in the input is honoured by the
    /// engine exactly as it is on HTTP; `precondition` is the typed alternative.
    pub async fn call(
        &self,
        name: &str,
        input: Value,
        precondition: Option<i64>,
    ) -> Result<Value, CallError> {
        let spec = registry()
            .get(name)
            .ok_or_else(|| CallError::NotFound(name.to_owned()))?;
        if spec.kind != ActionKind::Internal {
            return Err(CallError::NotInternal(name.to_owned()));
        }
        let outcome = invoke(
            self.pool,
            conveyor_actions::registry(),
            self.tenant,
            InvokeRequest {
                action: name.to_owned(),
                input,
                precondition: precondition.map(conveyor_actions::Precondition::Version),
                idempotency_key: None,
                actor_id: self.actor,
                actor_kind: "service".to_owned(),
                role: Role::KataExecutor,
                policy: Default::default(),
                authority: Authority::Internal,
                request_id: None,
                trace_id: None,
                depth: 0,
                app_key: self.app_key.to_owned(),
                usage: None,
            },
        )
        .await
        .map_err(|error| CallError::Action {
            name: name.to_owned(),
            status: error.status,
            detail: error.detail,
        })?;
        Ok(outcome.body.get("output").cloned().unwrap_or(Value::Null))
    }
}

/// Why an operator action could not run.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// The database refused a read.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// No membership exists to attribute the action to.
    #[error("the tenant has no active member to act as")]
    NoActor,
    /// The action name is not in the registry.
    #[error("no action {0:?}")]
    NotFound(String),
    /// The operator path only calls internal actions; an external action would reach a backend.
    #[error("{0:?} is not an internal action")]
    NotInternal(String),
    /// The action itself refused.
    #[error("{name} refused ({status}): {detail}")]
    Action {
        /// The action name.
        name: String,
        /// The HTTP status the same call would return.
        status: u16,
        /// The RFC 9457 detail.
        detail: String,
    },
}
