#![forbid(unsafe_code)]
//! `capability_agents.create` · `.update` · `.list` · `.get`.
//!
//! The spec is configuration (CONTRACT §17). `create` and `update` refuse a body the executor could
//! never honour — a blank model, a cap above the ceiling, a malformed `prompt_ref`, a malformed or
//! duplicated `allowed_actions` entry — with a `422` naming the field. `allowed_actions` **entries**
//! are not resolved here: an unregistered action is a readiness blocker (step 12), not a write
//! refusal, so a capability may name `agent.run` before S13 registers it.

use conveyor_domain::capability::{
    AgentConfig, CapabilityStatus, MAX_TIMEOUT_SECS, validate_agent_config,
};
use conveyor_store as store;
use conveyor_store::capability::{AgentPatch, NewAgent};
use sqlx::PgConnection;

use crate::board::{etag_of, require_precondition_with};
use crate::capability::{
    AgentCreateIn, AgentGetIn, AgentListIn, AgentOut, AgentUpdateIn, DEFAULT_ITERATIONS,
    agent_conflict, agent_out, config_error, decode_key_cursor, key_cursor, parse_status,
    store_error,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// Create one executor spec for a capability.
#[crate::action(
    name = "capability_agents.create",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = false,
    timeout_ms = 15000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capability_agents_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AgentCreateIn,
) -> Result<HandlerOutput<AgentOut>, ActionError> {
    let action = "capability_agents.create";
    let key = input.key.trim();
    if key.is_empty() {
        return Err(crate::capability::invalid(
            "key",
            "required",
            "must not be empty",
        ));
    }
    let status = match input.status.as_deref() {
        Some(raw) => parse_status(raw)?,
        None => CapabilityStatus::Draft,
    };
    let params = input.params.unwrap_or_else(|| serde_json::json!({}));
    let allowed_actions = input.allowed_actions.unwrap_or_default();
    let max_iterations = input.max_iterations.unwrap_or(DEFAULT_ITERATIONS);
    let timeout_secs = input.timeout_secs.unwrap_or(MAX_TIMEOUT_SECS);
    validate_agent_config(&AgentConfig {
        model_ref: &input.model_ref,
        prompt_ref: input.prompt_ref.as_deref(),
        allowed_actions: &allowed_actions,
        max_iterations,
        timeout_secs,
    })
    .map_err(config_error)?;

    let record = store::capability::create_agent_tx(
        &mut *tx,
        ctx.tenant,
        &NewAgent {
            capability_id: input.capability_id,
            key: key.to_owned(),
            status: status.as_str().to_owned(),
            model_ref: input.model_ref.trim().to_owned(),
            prompt_ref: input.prompt_ref,
            params,
            allowed_actions,
            max_iterations,
            timeout_secs,
        },
    )
    .await
    .map_err(|error| store_error(error, action, "capability agent"))?;

    Ok(HandlerOutput::created(agent_out(&record)).with_etag(etag_of(record.version)))
}

/// Change an executor spec. A patch: an absent member keeps its stored value, `prompt_ref: null`
/// clears it. `If-Match` is required and a stale one answers `412` with the current document.
#[crate::action(
    name = "capability_agents.update",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = false,
    timeout_ms = 15000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capability_agents_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AgentUpdateIn,
) -> Result<HandlerOutput<AgentOut>, ActionError> {
    let action = "capability_agents.update";
    let current = store::capability::get_agent_tx(&mut *tx, ctx.tenant, input.agent_id)
        .await
        .map_err(|error| store_error(error, action, "capability agent"))?
        .ok_or_else(|| ActionError::resource_not_found("capability agent"))?;
    let before = agent_out(&current);
    let expected = require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
    )?;

    let current_status = CapabilityStatus::parse(&current.status).ok_or_else(|| {
        store_error(
            store::capability::CapabilityStoreError::UnknownStatus(current.status.clone()),
            action,
            "capability agent",
        )
    })?;
    let next_status = match input.status.as_deref() {
        Some(raw) => parse_status(raw)?,
        None => current_status,
    };
    if next_status != current_status && !current_status.can_transition_to(next_status) {
        return Err(ActionError::new(
            409,
            "capability_agent.illegal_transition",
            format!(
                "a {} capability agent cannot become {}",
                current_status.as_str(),
                next_status.as_str()
            ),
        ));
    }

    let model_ref = input.model_ref.unwrap_or_else(|| current.model_ref.clone());
    let prompt_ref = match input.prompt_ref {
        Some(value) => value,
        None => current.prompt_ref.clone(),
    };
    let params = input.params.unwrap_or_else(|| current.params.clone());
    let allowed_actions = input
        .allowed_actions
        .unwrap_or_else(|| current.allowed_actions.clone());
    let max_iterations = input.max_iterations.unwrap_or(current.max_iterations);
    let timeout_secs = input.timeout_secs.unwrap_or(current.timeout_secs);
    validate_agent_config(&AgentConfig {
        model_ref: &model_ref,
        prompt_ref: prompt_ref.as_deref(),
        allowed_actions: &allowed_actions,
        max_iterations,
        timeout_secs,
    })
    .map_err(config_error)?;

    let expected_version = i64::try_from(expected.get()).map_err(|_| ActionError::internal())?;
    let record = store::capability::update_agent_tx(
        &mut *tx,
        ctx.tenant,
        current.id,
        expected_version,
        &AgentPatch {
            status: next_status.as_str().to_owned(),
            model_ref,
            prompt_ref,
            params,
            allowed_actions,
            max_iterations,
            timeout_secs,
        },
    )
    .await;
    let record = match record {
        Ok(record) => record,
        // CONTRACT §3: a stale precondition answers with the representation the caller should now
        // be looking at. `before` was read before the UPDATE, so a concurrent commit could have
        // moved the row again — re-read it, and answer the pre-read only if it vanished.
        Err(store::capability::CapabilityStoreError::VersionMismatch { current: moved }) => {
            let now = store::capability::get_agent_tx(&mut *tx, ctx.tenant, current.id)
                .await
                .map_err(|error| store_error(error, action, "capability agent"))?;
            return Err(match now {
                Some(row) => agent_conflict(&agent_out(&row), moved),
                None => agent_conflict(&before, moved),
            });
        }
        Err(other) => return Err(store_error(other, action, "capability agent")),
    };

    Ok(HandlerOutput::ok(agent_out(&record)).with_etag(etag_of(record.version)))
}

/// One page of executor specs, ordered by key.
#[crate::action(
    name = "capability_agents.list",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 15000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capability_agents_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AgentListIn,
) -> Result<Page<AgentOut>, ActionError> {
    let cursor = decode_key_cursor(input.page.cursor.as_deref(), "capability agents")?;
    let limit = store::actions::clamp_limit(input.page.limit);
    let mut rows = store::capability::list_agents_after_tx(
        &mut *tx,
        ctx.tenant,
        input.capability_id,
        cursor.as_deref(),
        limit + 1,
    )
    .await
    .map_err(|error| store_error(error, "capability_agents.list", "capability agent"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(|row| key_cursor(&row.key))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(agent_out).collect(),
        next_cursor,
    })
}

/// One executor spec.
#[crate::action(
    name = "capability_agents.get",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 15000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn capability_agents_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AgentGetIn,
) -> Result<AgentOut, ActionError> {
    let record = store::capability::get_agent_tx(&mut *tx, ctx.tenant, input.agent_id)
        .await
        .map_err(|error| store_error(error, "capability_agents.get", "capability agent"))?
        .ok_or_else(|| ActionError::resource_not_found("capability agent"))?;
    Ok(agent_out(&record))
}
