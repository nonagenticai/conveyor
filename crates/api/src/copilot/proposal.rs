#![forbid(unsafe_code)]
//! Proposals: a copilot tool call that a person must decide (S10 §5.3).
//!
//! A proposal **is** an `approval` row (`kind = 'copilot.action'`), created through S06's
//! `insert_approval_tx` in the same transaction that writes the turn's terminal record — one event
//! owner, one gate machine, no proposal table (S10-D2). Execution is a job with a conditional claim,
//! the proposer's re-checked scopes and `Idempotency-Key: proposal:<approval_id>` (S10-D3): restart
//! safe, exactly-once effect.
//!
//! ⛔ **A proposal never stores a secret-bearing input in the clear.** `approval.meta` is readable
//! by every member who may list approvals, so the exact input is sealed with the tenant's key
//! (`conveyor_store::secret::sealer` — the same KEK `secrets.*` uses) and stored as
//! `meta.sealed_input`; `meta.input` and `meta.diff` carry a **redacted projection** in which every
//! `x-secret` property, at any depth, is [`REDACTED`]. Execution replays the sealed input only; a
//! secret-bearing proposal whose seal cannot be opened is refused, never run from the projection.

use base64::Engine as _;
use chrono::{DateTime, Utc};
use conveyor_actions::spec::ActionSpec;
use conveyor_actions::{ActionError, Authority, Confirm, Effect, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::Approval;
use conveyor_store::TenantId;
use conveyor_store::approval::{ApprovalStoreError, NewApproval, insert_approval_tx};
use conveyor_store::copilot::{self, CopilotStoreError};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::state::AppState;

/// The approval kind a copilot proposal carries.
pub const KIND: &str = "copilot.action";

/// What a redacted field shows instead of the value.
pub const REDACTED: &str = "<redacted>";

/// Why a proposal could not be recorded.
#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    /// The approval row was refused.
    #[error(transparent)]
    Store(#[from] ApprovalStoreError),
    /// The input could not be sealed (the process has no KEK, or the cipher failed).
    #[error(transparent)]
    Secret(#[from] conveyor_store::secret::SecretError),
    /// The input could not be encoded as JSON for sealing.
    #[error("the action input could not be encoded: {0}")]
    Encode(String),
}

/// How long a claim on one proposal holds before another worker may retry it. The idempotency key
/// is what makes the retry a replay rather than a second effect.
pub const CLAIM_LEASE_SECS: i64 = 120;

/// Whether a tool call must become a proposal instead of running (S10 §5.3).
pub fn required(spec: &ActionSpec) -> bool {
    spec.effect == Effect::Destructive
        || spec.confirm != Confirm::None
        || spec.agent.proposal_required
}

/// What to propose.
pub struct ProposalRequest<'a> {
    /// The conversation the turn belongs to.
    pub conversation_id: Uuid,
    /// The assistant turn that raised it — the `uq_approval_copilot_turn` key.
    pub turn_id: Uuid,
    /// The conversation's project, when it has one.
    pub project_id: Option<Uuid>,
    /// The action the model asked for.
    pub spec: &'a ActionSpec,
    /// The exact, schema-validated action input (the version observed at proposal time).
    pub input: Value,
    /// A human sentence for the proposal card.
    pub summary: String,
    /// Who proposed it — execution re-checks this principal's scopes.
    pub proposer_id: Uuid,
    /// `user | agent | service`.
    pub proposer_kind: &'a str,
    /// The proposer's tenant role.
    pub proposer_role: Role,
    /// When the gate expires.
    pub expires_at: Option<DateTime<Utc>>,
}

/// The proposal document stored in `approval.meta` (S10 §5.3).
///
/// `diff` lists the top-level fields the proposal would set, with their proposed values. It omits
/// `from`: the conveyor does not read the aggregate to guess its current value — a guessed "before"
/// on a card that moved underneath is exactly the lost-update trap §8.6 exists to prevent, and the
/// proposal's `input` pins the observed version instead.
///
/// `input` and `diff` are the **redacted projection**; `sealed_input` is the exact input, sealed
/// for this tenant, and is what execution replays.
pub fn meta_document(
    tenant: TenantId,
    request: &ProposalRequest<'_>,
) -> Result<Value, ProposalError> {
    let schema = request.spec.input_schema();
    let visible = redact_input(&schema, &request.input);
    let plaintext = serde_json::to_vec(&request.input)
        .map_err(|error| ProposalError::Encode(error.to_string()))?;
    let (ciphertext, key_id) = conveyor_store::secret::sealer()?.seal(tenant, &plaintext)?;
    Ok(json!({
        "conversation_id": request.conversation_id,
        "turn_id": request.turn_id,
        "action": request.spec.name,
        "effect": request.spec.effect.as_str(),
        "input": visible,
        "diff": diff(&visible),
        "sealed_input": base64::engine::general_purpose::STANDARD.encode(&ciphertext),
        "sealed_input_key_id": key_id,
        "summary": request.summary,
        "decide_via": ["web", "telegram", "mcp"],
        "proposer": {
            "actor_id": request.proposer_id,
            "actor_kind": request.proposer_kind,
            "role": request.proposer_role.as_str(),
        },
    }))
}

/// The redacted projection of an action input: every `x-secret` property, at any depth, becomes
/// [`REDACTED`]. The schema is the source of truth, so a new secret property is covered without
/// changing this function.
pub fn redact_input(schema: &Value, input: &Value) -> Value {
    if let (Some(items), Some(values)) = (schema.get("items"), input.as_array()) {
        return Value::Array(
            values
                .iter()
                .map(|value| redact_input(items, value))
                .collect(),
        );
    }
    let Some(fields) = input.as_object() else {
        return input.clone();
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    let mut out = serde_json::Map::with_capacity(fields.len());
    for (key, value) in fields {
        let redacted = match properties.and_then(|properties| properties.get(key)) {
            Some(node) if node.get("x-secret") == Some(&Value::Bool(true)) => {
                Value::String(REDACTED.to_owned())
            }
            Some(node) => redact_input(node, value),
            None => value.clone(),
        };
        out.insert(key.clone(), redacted);
    }
    Value::Object(out)
}

/// Whether any property in this schema is `x-secret` — i.e. whether a proposal for it must carry a
/// seal. The walk is the same one [`redact_input`] performs.
pub fn carries_secret(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    if object.get("x-secret") == Some(&Value::Bool(true)) {
        return true;
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object)
        && properties.values().any(carries_secret)
    {
        return true;
    }
    if object.get("items").is_some_and(carries_secret) {
        return true;
    }
    ["$defs", "definitions"].iter().any(|key| {
        object
            .get(*key)
            .and_then(Value::as_object)
            .is_some_and(|definitions| definitions.values().any(carries_secret))
    })
}

/// The proposed field values, as the proposal card renders them.
pub fn diff(input: &Value) -> Value {
    let Some(fields) = input.as_object() else {
        return json!([]);
    };
    Value::Array(
        fields
            .iter()
            .map(|(key, value)| json!({ "path": format!("/{key}"), "to": value }))
            .collect(),
    )
}

/// Create the proposal in the caller's transaction.
///
/// The input is sealed here, before it ever reaches a row: from this point the only copy in the
/// database that can be executed is the ciphertext.
pub async fn create_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    request: &ProposalRequest<'_>,
) -> Result<Approval, ProposalError> {
    let mut new = NewApproval::new(request.spec.name, KIND);
    new.project_id = request.project_id;
    new.requested_by = Some(request.proposer_id);
    new.expires_at = request.expires_at;
    new.meta = meta_document(tenant, request)?;
    insert_approval_tx(conn, tenant, &new)
        .await
        .map_err(ProposalError::from)
}

/// What one execution attempt did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Execution {
    /// The action ran (or replayed its own earlier run) and the proposal is now terminal.
    Executed(Uuid),
    /// Nothing to do: not approved, already executed, or a live claim holds it.
    Skipped,
    /// The proposal could not run and never will without a person acting: the action is gone, or the
    /// proposer no longer holds the scopes.
    Refused,
}

/// Why an execution attempt failed transiently.
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    /// The database refused a statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// A copilot store operation failed.
    #[error(transparent)]
    Store(#[from] CopilotStoreError),
    /// The action itself refused or timed out. Retryable only for the 5xx classes.
    #[error(transparent)]
    Action(#[from] ActionError),
    /// The proposal's sealed input could not be opened (no KEK, wrong key, corrupt ciphertext).
    #[error(transparent)]
    Secret(#[from] conveyor_store::secret::SecretError),
    /// The proposal's sealed input is not the JSON this build encoded.
    #[error("a proposal's sealed input is unreadable: {0}")]
    Unreadable(String),
}

impl ExecutionError {
    /// Whether another attempt could succeed.
    pub fn retryable(&self) -> bool {
        match self {
            Self::Database(_) | Self::Store(_) => true,
            Self::Action(error) => error.status >= 500 || error.status == 429,
            // A seal that does not open does not open later; a person must act.
            Self::Secret(_) | Self::Unreadable(_) => false,
        }
    }
}

/// Open a proposal's sealed input for this tenant and parse the JSON the proposal was made from.
fn open_sealed_input(
    tenant: TenantId,
    encoded: &str,
    meta: &Value,
) -> Result<Value, ExecutionError> {
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| ExecutionError::Unreadable(format!("not base64: {error}")))?;
    let key_id = meta["sealed_input_key_id"].as_str().unwrap_or_default();
    let plaintext = conveyor_store::secret::sealer()?.open(tenant, &ciphertext, key_id)?;
    serde_json::from_slice(&plaintext)
        .map_err(|error| ExecutionError::Unreadable(format!("not JSON: {error}")))
}

/// Execute one approved proposal.
///
/// Order: claim (conditional `UPDATE … RETURNING` with a lease) → load → re-check the proposer's
/// scopes → invoke with the proposal's idempotency key → record the invocation. A killed worker's
/// claim expires after [`CLAIM_LEASE_SECS`] and the next attempt replays instead of re-running.
pub async fn execute(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Execution, ExecutionError> {
    {
        let mut tx = conveyor_store::begin_scoped(pool, tenant).await?;
        let claimed =
            copilot::claim_proposal_execution_tx(&mut tx, tenant, approval_id, CLAIM_LEASE_SECS)
                .await?;
        tx.commit().await?;
        if claimed.is_none() {
            return Ok(Execution::Skipped);
        }
    }

    let proposal = {
        let mut tx = conveyor_store::begin_scoped(pool, tenant).await?;
        let row = copilot::proposal(&mut tx, tenant, approval_id).await?;
        tx.commit().await?;
        row
    };
    let Some(proposal) = proposal else {
        // Claimed but vanished (or not a copilot proposal): nothing to do, and the claim will age out.
        return Ok(Execution::Skipped);
    };

    let action = proposal.meta["action"].as_str().unwrap_or_default();
    let proposer = &proposal.meta["proposer"];
    let proposer_id = proposer["actor_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok());
    let proposer_kind = proposer["actor_kind"].as_str().unwrap_or("user");
    let proposer_role = proposer["role"].as_str().and_then(Role::parse);

    let Some(spec) = registry().get(action) else {
        tracing::warn!(approval = %approval_id, action, "proposal names an action that is no longer registered");
        return Ok(Execution::Refused);
    };
    // The exact input, never the projection: a proposal for a secret-bearing action without a
    // seal is refused rather than run with `<redacted>` in place of the value.
    let input = match proposal.meta["sealed_input"].as_str() {
        Some(sealed) => open_sealed_input(tenant, sealed, &proposal.meta)?,
        None if carries_secret(&spec.input_schema()) => {
            tracing::warn!(approval = %approval_id, action, "a secret-bearing proposal carries no sealed input");
            return Ok(Execution::Refused);
        }
        None => proposal.meta["input"].clone(),
    };
    // The proposer's scopes, re-checked at execution: an approval is not a privilege escalation.
    // The tenant's stored policy is applied too — an override that revoked the scope while the
    // approval was pending must take effect here, or the approval would outlive the decision.
    let proposer_policy = match proposer_role {
        Some(role) => conveyor_auth::ScopePolicy::new(
            conveyor_store::rbac::policy_for(pool, tenant, role.as_str()).await?,
        ),
        None => Default::default(),
    };
    // A machine proposer is a connector: re-read the tenant's current grant set (an override
    // that revoked the action while the proposal was pending must take effect here), exactly
    // as `authenticate` would for a fresh request. A human proposer is judged by role+policy.
    let channel = if matches!(proposer_kind, "agent" | "service") {
        let overrides =
            conveyor_store::rbac::channel_policy_for(pool, tenant, conveyor_domain::channel::MCP)
                .await?;
        Some(conveyor_auth::ChannelAuthority::new(
            conveyor_domain::channel::MCP,
            None,
            overrides,
        ))
    } else {
        None
    };
    let permitted = match &channel {
        Some(authority) => spec.permits_channel(authority),
        None => {
            proposer_kind == "user"
                && proposer_role.is_some_and(|role| spec.permits_with(role, &proposer_policy))
        }
    };
    if !permitted {
        tracing::warn!(approval = %approval_id, action, "the proposer no longer holds the scopes");
        return Ok(Execution::Refused);
    }

    let key = format!("proposal:{approval_id}");
    let outcome = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: Some(key.clone()),
            actor_id: proposer_id.unwrap_or(approval_id),
            actor_kind: proposer_kind.to_owned(),
            role: proposer_role.unwrap_or(Role::Member),
            policy: proposer_policy,
            authority: match &channel {
                Some(authority) => Authority::Channel(authority.clone()),
                None => Authority::Role,
            },
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: app_key.to_owned(),
            usage: None,
        },
    )
    .await?;

    let invocation_id = match outcome.invocation_id {
        Some(id) => id,
        None => {
            // A replay writes no new invocation row; cite the one the first attempt wrote.
            let mut tx = conveyor_store::begin_scoped(pool, tenant).await?;
            let found =
                conveyor_store::actions::find_invocation_by_key(&mut tx, tenant, action, &key)
                    .await?
                    .ok_or_else(|| {
                        sqlx::Error::Protocol(
                            "an executed proposal has no action_invocation row".to_owned(),
                        )
                    })?;
            tx.commit().await?;
            found
        }
    };

    let mut tx = conveyor_store::begin_scoped(pool, tenant).await?;
    copilot::record_proposal_execution_tx(&mut tx, tenant, approval_id, invocation_id).await?;
    // S10 §7 (b): the turn that proposed this action ends `completed` with the executed tool call.
    if let Some(turn_id) = proposal.meta["turn_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        && let Some(turn) =
            copilot::complete_proposed_turn_tx(&mut tx, tenant, turn_id, action, invocation_id)
                .await?
    {
        conveyor_store::outbox::append(
            &mut tx,
            tenant,
            &conveyor_store::outbox::NewEvent {
                topic: format!("conversation:{}", turn.conversation_id),
                event_type: "conversation_turn.created".to_owned(),
                aggregate_type: "conversation_turn".to_owned(),
                aggregate_id: turn.id,
                payload: json!({
                    "conversation_id": turn.conversation_id,
                    "turn": super::conversations::turn_out(&turn),
                    "proposal_id": approval_id,
                }),
                correlation_id: None,
                causation_id: None,
            },
        )
        .await
        .map_err(|error| match error {
            conveyor_store::outbox::OutboxError::Database(error) => ExecutionError::Database(error),
            other => ExecutionError::Database(sqlx::Error::Protocol(other.to_string())),
        })?;
    }
    tx.commit().await?;
    Ok(Execution::Executed(invocation_id))
}

/// Enqueue `copilot.proposal.execute` for every approved, unexecuted proposal (the sweep).
///
/// One job per proposal: the retry policy is then per proposal, and the job's dedupe key
/// (`proposal:<id>`) makes two sweeps in one tick harmless (CONTRACT §7's job queue).
pub async fn sweep(pool: &PgPool, tenant: TenantId, limit: i64) -> Result<u64, ExecutionError> {
    let ready = {
        let mut tx = conveyor_store::begin_scoped(pool, tenant).await?;
        let ids = copilot::proposals_ready(&mut tx, tenant, limit).await?;
        tx.commit().await?;
        ids
    };
    let mut queued = 0;
    for approval_id in ready {
        let enqueued = conveyor_store::jobs::enqueue(
            pool,
            tenant,
            "copilot.proposal.execute",
            &format!("proposal:{approval_id}"),
            json!({ "approval_id": approval_id }),
            None,
            5,
        )
        .await?;
        if enqueued.is_some() {
            queued += 1;
        }
    }
    Ok(queued)
}

/// The app key the execution's licence read uses.
pub fn app_key(state: &AppState) -> &str {
    &state.config.app_key
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_actions::registry;

    #[test]
    fn a_destructive_or_confirming_action_proposes_and_so_does_a_hinted_one() {
        let close = registry()
            .iter()
            .find(|spec| spec.name == "projects.close")
            .expect("registered");
        assert!(required(close), "destructive must propose");

        let get = registry()
            .iter()
            .find(|spec| spec.name == "cards.get")
            .expect("registered");
        assert!(!required(get), "a read must run");

        let mv = registry()
            .iter()
            .find(|spec| spec.name == "cards.move")
            .expect("registered");
        assert!(required(mv), "cards.move declares proposal_required");
    }

    #[test]
    fn the_diff_lists_the_proposed_fields_without_inventing_a_before() {
        let diff = diff(&json!({"id": "c1", "list_id": "l2"}));
        let entries = diff.as_array().expect("an array");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.get("from").is_none()));
        assert_eq!(entries[0]["path"], "/id");
    }
}
