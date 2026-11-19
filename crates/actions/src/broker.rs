#![forbid(unsafe_code)]
//! The tool broker — one governed call path from a run to a job backend (SPEC_V2 §11.3; T13).
//!
//! The order is the contract:
//!
//! 1. **resolve** the run's binding and the pinned tool version — missing is `409 tool_not_ready`
//!    before any network call;
//! 2. **check effect** against the binding's ceiling — `403 tool_effect_forbidden`;
//! 3. **validate** the input against the tool version's closed schema — `422 tool_input_invalid`
//!    with zero network calls;
//! 4. **gate** a destructive tool: an approved approval naming this exact tool, unless the
//!    tenant's policy for `tools.call` waives it — refused before the destination;
//! 5. **call** the declared external action the tool version names. The broker never invents a
//!    destination, a URL or a credential: the action's own `HandlerRef::External` is the one
//!    dispatch path, and its base URL is configuration (§11.3 step 5, the SSRF rule);
//! 6. **normalise and redact** the result (declared fields replaced, 1 MiB cap);
//! 7. **record** exactly one `tool_call` row on the pool — after the handler transaction, so a
//!    failed call still leaves its evidence — and one `tool.called` event. The call is never a
//!    card transition.
//!
//! A ledger write that fails after the destination answered is logged and does not turn a real
//! call into a fake one; the target action's own `action_invocation` row is still the audit of
//! the dispatch. Nothing here holds a transaction across the HTTP call.

use std::time::Instant;

use conveyor_store as store;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::invoke::{ActionError, Authority, HandlerCtx, InvokeOutcome, InvokeRequest, invoke};
use crate::registry::registry;
use crate::spec::{ActionKind, Effect, FieldError, validate_input};

/// The largest destination body the broker passes through. A bigger answer is refused rather
/// than carried into a run's context.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// The broker's input, mirroring `tools.rs`'s schema.
#[derive(serde::Deserialize)]
struct ToolCallIn {
    run_id: Uuid,
    tool: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    step_id: Option<Uuid>,
}

/// Refuse before any network call: the run has no usable binding or pinned version.
fn not_ready(detail: String) -> ActionError {
    ActionError::new(409, "tool_not_ready", detail)
}

/// A refusal that names the tool blocking the run (SPEC_V2 §11.9; T14).
///
/// The `409`'s `errors[]` is the wire form of the report's `blocking[]` — the same mapping
/// `capability::readiness` uses for its GET, so a caller reads one shape whether the block came
/// from readiness or from a probe that unbound the tool.
fn not_ready_tool(tool: &str, detail: String) -> ActionError {
    ActionError::new(409, "tool_not_ready", detail).with_errors(vec![field(
        "tool",
        format!("{tool} is not bound for this run"),
    )])
}

fn invalid_tool_input(detail: impl Into<String>, errors: Vec<FieldError>) -> ActionError {
    ActionError::new(422, "tool_input_invalid", detail).with_errors(errors)
}

fn field(field: &str, detail: impl Into<String>) -> FieldError {
    FieldError {
        field: field.to_owned(),
        code: "invalid_value".to_owned(),
        detail: detail.into(),
    }
}

/// Resolve, check, validate, gate, call and ledger one tool call.
pub(crate) async fn invoke_broker(
    pool: &PgPool,
    _spec: &crate::spec::ActionSpec,
    ctx: HandlerCtx,
    input: Value,
    request: &InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    let call: ToolCallIn = serde_json::from_value(input).map_err(|error| {
        invalid_tool_input(
            "the tools.call input is not a call document",
            vec![field("body", error.to_string())],
        )
    })?;
    let Some((toolkit_key, tool_key)) = call.tool.split_once('.') else {
        return Err(invalid_tool_input(
            "the tool name is not <toolkit>.<operation>",
            vec![field(
                "tool",
                format!("{:?} has no toolkit prefix", call.tool),
            )],
        ));
    };

    // ── 1–4: resolve and gate, in one short read-only transaction ─────────────────────────
    let mut tx = store::begin_scoped_read_only(pool, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "broker: begin the resolution transaction");
            ActionError::internal()
        })?;

    let mut run = store::tool::run_context(&mut tx, ctx.tenant, call.run_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, run = %call.run_id, "broker: read the run");
            ActionError::internal()
        })?
        .ok_or_else(|| not_ready(format!("no run {} in this tenant", call.run_id)))?;

    if let Some(step_id) = call.step_id {
        let belongs = store::tool::step_belongs_to_run(&mut tx, ctx.tenant, run.run_id, step_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, step = %step_id, "broker: read the step");
                ActionError::internal()
            })?;
        if !belongs {
            return Err(invalid_tool_input(
                "the step does not belong to the run",
                vec![field(
                    "step_id",
                    format!("{step_id} is not a step of {}", run.run_id),
                )],
            ));
        }
        run.run_step_id = Some(step_id);
    }

    let tool = store::tool::resolve(&mut tx, ctx.tenant, &run, toolkit_key, tool_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, tool = %call.tool, "broker: resolve the tool");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            not_ready_tool(
                &call.tool,
                format!(
                    "run {} has no published binding for {}",
                    run.run_id, call.tool
                ),
            )
        })?;

    let effect = Effect::parse(&tool.effect).ok_or_else(|| {
        tracing::error!(tool = %call.tool, effect = %tool.effect, "broker: unknown effect class");
        ActionError::new(
            500,
            "tool_misconfigured",
            format!("{} carries an unknown effect class", call.tool),
        )
    })?;
    let ceiling = Effect::parse(&tool.effect_ceiling).ok_or_else(|| {
        tracing::error!(tool = %call.tool, ceiling = %tool.effect_ceiling, "broker: unknown ceiling");
        ActionError::new(
            500,
            "tool_misconfigured",
            format!("the binding for {} carries an unknown ceiling", call.tool),
        )
    })?;
    if effect.rank() > ceiling.rank() {
        return Err(ActionError::new(
            403,
            "tool_effect_forbidden",
            format!(
                "{} is a {} tool and this binding allows at most {}",
                call.tool,
                effect.as_str(),
                ceiling.as_str()
            ),
        ));
    }

    let errors = validate_input(&tool.input_schema, &call.input);
    if !errors.is_empty() {
        return Err(invalid_tool_input(
            format!(
                "the input does not match {} version {}",
                call.tool, tool.toolkit_version
            ),
            errors,
        ));
    }

    if effect == Effect::Destructive {
        let waived = store::approval_policy::waives(&mut tx, ctx.tenant, "tools.call")
            .await
            .map_err(|error| {
                tracing::error!(%error, tool = %call.tool, "broker: read the approval policy");
                ActionError::internal()
            })?;
        if !waived {
            crate::control::approvals::require_approval_tx(
                &mut tx,
                &ctx,
                "tools.call",
                &call.tool,
                ctx.approval_id,
            )
            .await?;
        }
    }

    // The transaction was read-only; close it before the network call so no lock or connection
    // is held across the destination's latency.
    let _ = tx.rollback().await;

    // ── 5: the one external dispatch path ─────────────────────────────────────────────────
    let target = registry().get(&tool.handler_action).ok_or_else(|| {
        tracing::error!(tool = %call.tool, action = %tool.handler_action, "broker: unknown destination action");
        ActionError::new(
            500,
            "tool_destination_invalid",
            format!("{} names no registered destination", call.tool),
        )
    })?;
    if target.kind != ActionKind::External {
        tracing::error!(
            tool = %call.tool,
            action = %tool.handler_action,
            "broker: a tool destination must be a declared external action"
        );
        return Err(ActionError::new(
            500,
            "tool_destination_invalid",
            format!(
                "{} names a destination that is not an external action",
                call.tool
            ),
        ));
    }

    let request_bytes = serde_json::to_vec(&call.input)
        .ok()
        .and_then(|bytes| i32::try_from(bytes.len()).ok());
    let target_request = InvokeRequest {
        action: tool.handler_action.clone(),
        input: call.input.clone(),
        precondition: None,
        idempotency_key: request.idempotency_key.clone(),
        approval_id: ctx.approval_id,
        actor_id: ctx.actor_id,
        actor_kind: ctx.actor_kind.clone(),
        role: ctx.role,
        policy: ctx.policy.clone(),
        // The broker is the engine's own dispatch, not the caller's second identity: the grant
        // was the binding above, and the target action's scope check is a second belt.
        authority: Authority::Internal,
        request_id: ctx.request_id.clone(),
        trace_id: request.trace_id.clone(),
        depth: request.depth + 1,
        app_key: ctx.app_key.clone(),
        usage: request.usage.clone(),
    };

    let started = Instant::now();
    // `invoke` dispatches the broker when it sees `HandlerRef::Broker`, so this edge is recursion
    // the compiler cannot size. Boxing the callee is the standard indirection; the depth counter
    // (`MAX_DEPTH`) is what actually bounds the chain.
    let result = Box::pin(invoke(pool, registry(), ctx.tenant, target_request)).await;
    let latency_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);

    // ── 6: normalise, redact, cap ────────────────────────────────────────────────────────
    let redact: Vec<String> = tool
        .redact
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let outcome = match result {
        Ok(mut outcome) => {
            redact_fields(&mut outcome.body, &redact);
            let bytes = serde_json::to_vec(&outcome.body)
                .map(|bytes| bytes.len())
                .unwrap_or(0);
            if bytes > MAX_RESPONSE_BYTES {
                Err(ActionError::new(
                    502,
                    "tool_response_too_large",
                    format!(
                        "{} answered {bytes} bytes; the broker caps a tool result at {MAX_RESPONSE_BYTES}",
                        call.tool
                    ),
                ))
            } else {
                Ok(outcome)
            }
        }
        Err(error) => Err(error),
    };

    // The response size of a refusal that came from the broker itself is the problem document's.
    let (status, http_status, error_class, error_message) = match &outcome {
        Ok(outcome) => {
            let status = match outcome.status {
                200..=299 => "ok",
                412 => "precondition_failed",
                422 => "check_failed",
                _ => "backend_error",
            };
            (status, Some(i32::from(outcome.status)), None, None)
        }
        Err(error) => (
            match error.status {
                412 => "precondition_failed",
                422 => "check_failed",
                _ => "backend_error",
            },
            Some(i32::from(error.status)),
            Some(error.code),
            Some(error.detail.clone()),
        ),
    };
    let response_bytes = match &outcome {
        Ok(outcome) => serde_json::to_vec(&outcome.body)
            .ok()
            .and_then(|bytes| i32::try_from(bytes.len()).ok()),
        Err(_error) => error_message
            .as_ref()
            .and_then(|detail| i32::try_from(detail.len()).ok()),
    };

    // ── 7: the ledger and the event, after the call ──────────────────────────────────────
    let input_hash = crate::spec::sha256_hex(&serde_json::to_vec(&call.input).unwrap_or_default());
    let call_id = store::tool::NewToolCall {
        run_id: Some(run.run_id),
        run_step_id: run.run_step_id,
        card_id: run.card_id,
        tool_id: tool.tool_id,
        tool_key: &call.tool,
        toolkit_key: &tool.toolkit_key,
        toolkit_version: tool.toolkit_version,
        input_hash: &input_hash,
        status,
        http_status,
        latency_ms,
        request_bytes,
        response_bytes,
        cost: None,
        error_class,
        error_message: error_message.as_deref(),
    };
    match write_ledger(pool, ctx.tenant, &call_id).await {
        Ok(id) => tracing::debug!(
            tool = %call.tool, run = %run.run_id, call = %id, status, latency_ms,
            "broker: call ledgered"
        ),
        Err(error) => tracing::error!(
            error = %error, tool = %call.tool, run = %run.run_id, status,
            "broker: the destination answered but the tool_call ledger row could not be written"
        ),
    }

    outcome
}

/// Append the ledger row and the `tool.called` event in one short transaction.
///
/// The event's topic is the tool version, so a metrics reader can follow one tool's health
/// without scanning every run (§11.9).
async fn write_ledger(
    pool: &PgPool,
    tenant: conveyor_domain::TenantId,
    call: &store::tool::NewToolCall<'_>,
) -> Result<Uuid, String> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| error.to_string())?;
    let call_id = store::tool::insert_call(&mut tx, tenant, call)
        .await
        .map_err(|error| error.to_string())?;
    let event = store::NewEvent {
        topic: format!("tool:{}", call.tool_id),
        event_type: TOOL_CALLED.to_owned(),
        aggregate_type: "tool_call".to_owned(),
        aggregate_id: call_id,
        payload: serde_json::json!({
            "run_id": call.run_id,
            "tool": call.tool_key,
            "toolkit": call.toolkit_key,
            "toolkit_version": call.toolkit_version,
            "status": call.status,
            "http_status": call.http_status,
            "latency_ms": call.latency_ms,
        }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut tx, tenant, &event)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(call_id)
}

/// Replace every declared field's value with a marker, at any depth.
///
/// Redaction is by key name on the result document: a tool declares what it returns that must not
/// reach a prompt, and the broker applies the declaration to the answer it actually got.
fn redact_fields(value: &mut Value, fields: &[String]) {
    if fields.is_empty() {
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, member) in object.iter_mut() {
                if fields.iter().any(|field| field == key) {
                    *member = Value::String("[redacted]".to_owned());
                } else {
                    redact_fields(member, fields);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_fields(item, fields);
            }
        }
        _ => {}
    }
}

/// The event a successful or failed call emits (§11.3 step 7).
pub const TOOL_CALLED: &str = "tool.called";
