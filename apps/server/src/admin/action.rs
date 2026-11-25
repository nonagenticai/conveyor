//! The operator action path.
//!
//! `conveyor admin action invoke` — and the admin commands built on it — call the **same**
//! engine as `POST /api/actions/<name>/invocations`: no second dispatcher and no raw SQL
//! mutation. The actor is **selectable**: `--actor-id` names an active member (a governance act
//! must be performed by a DISTINCT approver — the live tenant has `conveyor-approver` for
//! exactly that), and without it the caller acts as the tenant's first active admin/member (the
//! `board-export` importer's rule). A named actor is validated against `membership`, never
//! trusted. `actor_kind = "service"` and the synthetic machine role stay the same either way, so
//! validation, preconditions, events and audit rows are the ones a real caller gets.

use conveyor_actions::invoke::{Authority, InvokeRequest, UsageContext, invoke};
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
    /// Resolve the tenant's default actor once. A tenant with no active member has no operator.
    ///
    /// Delegates to [`new_as`] with no actor named, so the identity rules live in one place.
    pub async fn new(
        pool: &'a PgPool,
        tenant: TenantId,
        app_key: &'a str,
    ) -> Result<Self, CallError> {
        Self::new_as(pool, tenant, app_key, None).await
    }

    /// Resolve the actor the caller NAMES (`--actor-id`), or the tenant's default one.
    ///
    /// A named actor is validated: it must be an active member of THIS tenant, otherwise
    /// [`CallError::NotMember`]. That is what lets a governance act be performed by a distinct
    /// approver instead of the tenant's first member, without inventing an identity.
    pub async fn new_as(
        pool: &'a PgPool,
        tenant: TenantId,
        app_key: &'a str,
        actor_id: Option<Uuid>,
    ) -> Result<Self, CallError> {
        let actor = match actor_id {
            Some(id) => {
                if !store::identity::tenant_member(pool, tenant, id).await? {
                    return Err(CallError::NotMember(id));
                }
                id
            }
            None => store::identity::tenant_actor(pool, tenant)
                .await?
                .ok_or(CallError::NoActor)?,
        };
        Ok(Self {
            pool,
            tenant,
            actor,
            app_key,
        })
    }

    /// The member this caller acts as.
    pub fn actor_id(&self) -> Uuid {
        self.actor
    }

    /// Invoke one action and return its `output`. `_version` in the input is honoured by the
    /// engine exactly as it is on HTTP; `precondition` is the typed alternative.
    pub async fn call(
        &self,
        name: &str,
        input: Value,
        precondition: Option<i64>,
    ) -> Result<Value, CallError> {
        self.call_gated(name, input, precondition, None, None).await
    }

    /// As [`call`], for a destructive action whose approval the caller names (#282).
    ///
    /// The id is the CALL's, not the payload's: no action input carries an `approval_id` any more
    /// (the four that did lost it when the gate moved into the engine), so without this the CLI
    /// could invoke a destructive action and never satisfy its gate.
    pub async fn call_gated(
        &self,
        name: &str,
        input: Value,
        precondition: Option<i64>,
        approval_id: Option<Uuid>,
        idempotency_key: Option<String>,
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
                credential_alias: None,
                action: name.to_owned(),
                input,
                precondition: precondition.map(conveyor_actions::Precondition::Version),
                idempotency_key,
                approval_id,
                actor_id: self.actor,
                actor_kind: "service".to_owned(),
                role: Role::KataExecutor,
                policy: Default::default(),
                authority: Authority::Internal,
                request_id: None,
                trace_id: None,
                depth: 0,
                app_key: self.app_key.to_owned(),
                // The operator CLI is an ordinary caller of the same engine. A model-capturing
                // action invoked here must reach the ledger too; `other` is the kind the schema
                // allows to carry no join key (S16 W7).
                usage: Some(UsageContext {
                    caller_kind: conveyor_domain::inference::CallerKind::Other,
                    run_id: None,
                    run_step_id: None,
                    conversation_turn_id: None,
                    eval_run_id: None,
                    prompt_ref: None,
                    prompt_version: None,
                    prompt_sha256: None,
                    correlation_id: None,
                }),
            },
        )
        .await
        .map_err(|error| {
            let errors = field_errors(&error.errors);
            CallError::Action {
                name: name.to_owned(),
                status: error.status,
                detail: error.detail,
                errors,
            }
        })?;
        Ok(outcome.body.get("output").cloned().unwrap_or(Value::Null))
    }
}

/// The `errors[]` of a refused action, rendered for the operator.
///
/// The CLI printed only the RFC 9457 `detail`, which is the fixed sentence "the input does not
/// match the action schema" for every 422. Two record investigations (2026-11-25) could not
/// tell which field failed; the boundary already carries the field, code and detail, so it is
/// printed here rather than discarded.
fn field_errors(errors: &[conveyor_actions::spec::FieldError]) -> String {
    if errors.is_empty() {
        return String::new();
    }
    let rendered = errors
        .iter()
        .map(|error| format!("{}: {} ({})", error.field, error.detail, error.code))
        .collect::<Vec<_>>()
        .join("; ");
    format!(" [{rendered}]")
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
    /// The actor the caller named is not an active member of this tenant.
    #[error("user {0} is not an active member of this tenant")]
    NotMember(Uuid),
    /// The action name is not in the registry.
    #[error("no action {0:?}")]
    NotFound(String),
    /// The operator path only calls internal actions; an external action would reach a backend.
    #[error("{0:?} is not an internal action")]
    NotInternal(String),
    /// The action itself refused.
    #[error("{name} refused ({status}): {detail}{errors}")]
    Action {
        /// The action name.
        name: String,
        /// The HTTP status the same call would return.
        status: u16,
        /// The RFC 9457 detail.
        detail: String,
        /// The rendered `errors[]` (empty when the refusal carries none).
        errors: String,
    },
}
