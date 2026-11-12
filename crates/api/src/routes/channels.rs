//! `POST /api/channels/{binding_id}/inbound` — the one inbound path for every connector (S06d).
//!
//! This is a **channel callback**, not a bearer route: the connector authenticates with its
//! per-binding secret (from `channel_binding.config`, resolved from the sealed `secret` store),
//! so there is no `Caller` and no licence gate. The path names a binding, never a tenant; a
//! binding belongs to exactly one active tenant.
//!
//! The route does four things and nothing else: resolve the binding, ask the adapter to
//! authenticate and parse, hand the text/structured command to the shared grammar, and invoke
//! with the channel's service identity + tenant grants. **No authorization lives here** — the
//! same `invoke` gate the web and MCP use decides, so a reply cannot carry a capability the
//! tenant-admin did not grant.
//!
//! **Everything a well-formed-but-uninteresting update can do is `200`.** Telegram and most
//! gateways retry a non-2xx indefinitely, so an unknown binding, a message for another chat, a
//! no-op command or a business refusal is acknowledged and dropped. Only a `5xx` asks for a
//! retry, because only a `5xx` means the action may not have been recorded.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use conveyor_actions::channel::connectors;
use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::{ChannelAuthority, Role};
use conveyor_store as store;
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, ProblemDetail};
use crate::state::AppState;

/// One inbound acknowledgement. Always `200` unless the platform must retry.
#[derive(Clone, Serialize, ToSchema)]
pub struct InboundAck {
    /// Always `true`: the update was consumed (it may have been a no-op).
    pub ok: bool,
    /// The invocation's status, when the reply reached an action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

/// One inbound connector callback.
#[utoipa::path(
    post,
    path = "/api/channels/{binding_id}/inbound",
    tag = "channels",
    params(("binding_id" = Uuid, Path, description = "The channel binding this route represents")),
    request_body = inline(Value),
    responses(
        (status = 200, description = "Consumed; an unrecognised reply is a no-op", body = InboundAck),
        (status = 401, description = "The per-binding inbound credential is missing or wrong", body = ProblemDetail),
        (status = 503, description = "The binding is not configured (secret or identity missing)", body = ProblemDetail)
    )
)]
pub async fn channel_inbound(
    State(state): State<AppState>,
    Path(binding_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<InboundAck>, ApiError> {
    let Some((tenant, binding)) = store::channel::find_binding_by_id(&state.pool, binding_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, binding = %binding_id, "resolve the channel binding");
            ApiError::internal("the channel binding could not be read")
        })?
    else {
        tracing::info!(binding = %binding_id, "an inbound update named an unknown binding; acknowledged");
        return Ok(Json(ack(None)));
    };
    if !binding.active {
        tracing::info!(binding = %binding_id, "an inbound update named an inactive binding; acknowledged");
        return Ok(Json(ack(None)));
    }
    let Some(spec) = connectors::find(&binding.channel_kind) else {
        tracing::warn!(binding = %binding_id, kind = %binding.channel_kind, "this build does not serve the binding's connector kind");
        return Ok(Json(ack(None)));
    };
    if !spec.capabilities.can_receive {
        tracing::info!(binding = %binding_id, kind = %binding.channel_kind, "the connector does not receive; acknowledged");
        return Ok(Json(ack(None)));
    }

    // Resolve the binding's secrets and authenticate in one tenant-scoped read.
    let mut tx = store::begin_scoped(&state.pool, tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "begin the inbound channel transaction");
            ApiError::internal("the channel binding could not be read")
        })?;
    let secrets = match connectors::resolve_secrets_tx(&mut tx, tenant, spec, &binding.config).await
    {
        Ok(secrets) => secrets,
        Err(error) => {
            tracing::error!(binding = %binding_id, %error, "resolve the connector secrets");
            return Err(ApiError::service_unavailable(
                "the channel binding is not fully configured; retry once it is",
            ));
        }
    };
    tx.commit().await.map_err(|error| {
        tracing::error!(%error, "commit the inbound channel read");
        ApiError::internal("the channel binding could not be read")
    })?;
    if !(spec.authenticate)(&headers, &binding.config, &secrets) {
        return Err(ApiError::unauthorized(
            "the channel inbound credential is missing or does not match",
        ));
    }

    let Some(inbound) = (spec.parse)(&body, &binding.config) else {
        tracing::info!(binding = %binding_id, "an inbound update was consumed as a no-op");
        return Ok(Json(ack(None)));
    };

    // The tenant's current grants, read per message so a revocation applies immediately.
    let overrides = store::rbac::channel_policy_for(&state.pool, tenant, spec.kind)
        .await
        .map_err(|error| {
            tracing::error!(%error, binding = %binding_id, "read the channel grants");
            ApiError::internal("the channel authorization could not be read")
        })?;
    let authority = ChannelAuthority::new(spec.kind, inbound.sender_ref.clone(), overrides);

    let (action, input) = match resolve_command(&inbound, &authority) {
        Resolved::Invoke { action, input } => (action, input),
        Resolved::Help => {
            reply(
                spec,
                &binding.config,
                &secrets,
                conveyor_actions::channel::command::help_text(&authority),
                binding_id,
            )
            .await;
            return Ok(Json(ack(None)));
        }
        Resolved::NoOp => return Ok(Json(ack(None))),
    };

    // A provisioned connector acts as its tenant service account. A missing membership is an
    // operator error: retrying cannot conjure it, but a 200 would silently drop the reply, so
    // answer 503 and let the operator see it.
    let Some(service_user) = store::channel::service_account(&state.pool, tenant, spec.kind)
        .await
        .map_err(|error| {
            tracing::error!(%error, binding = %binding_id, "resolve the channel identity");
            ApiError::internal("the channel identity could not be read")
        })?
    else {
        tracing::error!(binding = %binding_id, kind = %spec.kind, "the channel service identity is absent");
        return Err(ApiError::service_unavailable(
            "the channel service identity is not provisioned; re-bind the connector",
        ));
    };

    let request = InvokeRequest {
        action: action.clone(),
        input,
        precondition: None,
        idempotency_key: inbound
            .idempotency_key
            .as_ref()
            .map(|key| format!("channel:{binding_id}:{key}")),
        actor_id: service_user,
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        authority: Authority::Channel(authority),
        request_id: inbound.request_id.clone(),
        trace_id: None,
        depth: 0,
        app_key: state.config.app_key.clone(),
        usage: None,
    };
    let reply_text = match invoke(&state.pool, registry(), tenant, request).await {
        Ok(outcome) => {
            let body = outcome.body.get("output").cloned().unwrap_or(Value::Null);
            format!("ok {action}: {}", summarize(&body))
        }
        // A business refusal is an outcome, not a webhook failure: acknowledge it so the
        // platform does not retry, and tell the sender what happened when it can receive.
        Err(error) if (400..500).contains(&error.status) => {
            format!("refused {} {}: {}", error.status, error.code, error.detail)
        }
        // Only a server-side failure may mean the reply was not recorded.
        Err(error) => {
            tracing::error!(binding = %binding_id, action = %action, status = error.status, code = error.code, "the channel reply could not be recorded");
            return Err(ApiError::service_unavailable(
                "the channel reply could not be recorded; retry the update",
            ));
        }
    };
    reply(spec, &binding.config, &secrets, reply_text, binding_id).await;
    Ok(Json(ack(Some(200))))
}

/// Best-effort acknowledgement to the sender. The action has committed; a failed reply must
/// not make the platform re-send and duplicate the work.
async fn reply(
    spec: &connectors::ConnectorSpec,
    config: &Value,
    secrets: &std::collections::BTreeMap<String, String>,
    text: String,
    binding_id: Uuid,
) {
    let outcome = (spec.send)(connectors::SendRequest {
        config: config.clone(),
        secrets: secrets.clone(),
        text,
    })
    .await;
    if !outcome.ok {
        tracing::warn!(binding = %binding_id, error = ?outcome.error, "the channel acknowledgement could not be sent");
    }
}

enum Resolved {
    Invoke { action: String, input: Value },
    Help,
    NoOp,
}

/// The shared grammar: structured command, or text, or help/no-op.
fn resolve_command(inbound: &connectors::Inbound, _authority: &ChannelAuthority) -> Resolved {
    if let Some(structured) = &inbound.structured {
        let Some(action) = structured.get("action").and_then(Value::as_str) else {
            return Resolved::NoOp;
        };
        let input = structured
            .get("input")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !input.is_object() {
            return Resolved::NoOp;
        }
        return Resolved::Invoke {
            action: action.to_owned(),
            input,
        };
    }
    let Some(text) = inbound.text.as_deref() else {
        return Resolved::NoOp;
    };
    match conveyor_actions::channel::command::parse(text) {
        conveyor_actions::channel::command::Parsed::Command { action, input } => {
            Resolved::Invoke { action, input }
        }
        conveyor_actions::channel::command::Parsed::Help => Resolved::Help,
        conveyor_actions::channel::command::Parsed::NoOp => Resolved::NoOp,
    }
}

fn summarize(body: &Value) -> String {
    for key in ["id", "run_id", "project_id", "approval_id"] {
        if let Some(value) = body.get(key).and_then(Value::as_str) {
            return format!("{key}={value}");
        }
    }
    if let Some(status) = body.get("status").and_then(Value::as_str) {
        return status.to_owned();
    }
    "done".to_owned()
}

fn ack(status: Option<u16>) -> InboundAck {
    InboundAck { ok: true, status }
}
