#![forbid(unsafe_code)]
//! The tool broker — one governed call path from a run to a job backend (SPEC_V2 §11.3; T13).
//!
//! The order is the contract:
//!
//! 1. **resolve** the run's binding and the pinned tool version — missing is `409 tool_not_ready`
//!    before any network call;
//! 2. **check effect** against the binding's ceiling — `403 tool_effect_forbidden` — and, for a
//!    `spend` tool, the run's spend budget — `429 tool_budget_exceeded` with zero network calls.
//!    The budget is declared by the job release (`anatomy.limits.cost_budget_usd`), the binding's
//!    `meta.limits` override or a lower `run.inputs.limits`; **a spend call with no declared
//!    budget is refused**, never treated as unlimited (D45: spend calls need a budget);
//! 3. **validate** the input against the tool version's closed schema — `422 tool_input_invalid`
//!    with zero network calls;
//! 4. **gate** a destructive tool: an approved approval naming this exact tool, unless the
//!    tenant's policy for `tools.call` waives it — refused before the destination;
//! 5. **call** the declared external action the tool version names. The broker never invents a
//!    destination, a URL or a credential: the action's own `HandlerRef::External` is the one
//!    dispatch path, and its base URL is configuration (§11.3 step 5, the SSRF rule);
//! 6. **normalise and redact** the result (declared fields replaced, 1 MiB cap);
//! 7. **record** exactly one `tool_call` row on the pool — after the handler transaction, so a
//!    failed call still leaves its evidence — with its `cost` derived from the handler's priced
//!    ledger rows, and one `tool.called` event. The call is never a card transition.
//!
//! A ledger write that fails after the destination answered is logged and does not turn a real
//! call into a fake one; the target action's own `action_invocation` row is still the audit of
//! the dispatch. Nothing here holds a transaction across the HTTP call.

use std::time::{Duration, Instant};

use conveyor_store as store;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::invoke::{
    ActionError, Authority, HandlerCtx, InvokeOutcome, InvokeRequest, UsageContext, invoke,
};
use crate::registry::registry;
use crate::spec::{ActionKind, Effect, FieldError, validate_input};
use conveyor_domain::organisation::{ReleaseAnatomy, parse_release_anatomy};

/// The largest destination body the broker passes through. A bigger answer is refused rather
/// than carried into a run's context.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// The broker's input, mirroring `tools.rs`'s schema. Unknown members are refused
/// (`deny_unknown_fields`): a call-time URL has no member to travel in.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

/// The SPEC's typed spend refusal (§11.3 step 2): nothing is dispatched, so nothing is charged.
fn budget_exceeded(detail: String) -> ActionError {
    ActionError::new(429, "tool_budget_exceeded", detail)
}

/// Refuse a `spend` call whose run has no budget or has already spent it.
///
/// The check reads two ledgers and counts each call once: derived `inference_call` cost (the
/// handlers that capture usage, including the ones a tool dispatches) plus `tool_call.cost` for
/// every other tool. The names come from the registry, so a new capturing action cannot silently
/// be charged twice.
async fn check_spend_budget(
    conn: &mut sqlx::PgConnection,
    tenant: conveyor_domain::TenantId,
    run: &store::tool::RunToolContext,
) -> Result<(), ActionError> {
    let captured: Vec<String> = registry()
        .iter()
        .filter(|spec| spec.captures_usage)
        .map(|spec| spec.name.to_string())
        .collect();
    let budget = store::inference::run_budget_tx(conn, tenant, run.run_id, &captured)
        .await
        .map_err(|error| {
            tracing::error!(%error, run = %run.run_id, "broker: read the spend budget");
            ActionError::internal()
        })?;
    let Some(limit) = budget.limit_usd else {
        return Err(budget_exceeded(format!(
            "run {} has no spend budget: declare `limits.cost_budget_usd` on the job release, \
             the job binding's meta or the run's inputs before a spend-class tool may be called",
            run.run_id
        )));
    };
    if budget.spent_usd >= limit {
        return Err(budget_exceeded(format!(
            "run {} has spent {} of its {} USD spend budget; the call was refused before dispatch",
            run.run_id, budget.spent_usd, limit
        )));
    }
    Ok(())
}

/// The declared verification a 2xx write failed, when the tool declares a synchronous path
/// (`verdict` + `checks`). A `read_back`-only tool verifies later, through the read tool.
///
/// A successful external call reaches the broker wrapped in the invocation envelope
/// (`{"output": <destination document>, …}`), so the declared members are read from the
/// destination's own document; a body with no `output` member is judged as itself.
fn write_verification_failure(tool: &store::tool::ResolvedTool, body: &Value) -> Option<String> {
    // The declaration is data on the pinned version: a read tool has nothing to judge here, and a
    // write tool that only names a `read_back` proves its effect through a later read.
    if tool.effect != "write" && tool.effect != "destructive" {
        return None;
    }
    let verdict = tool.verify.get("verdict").and_then(Value::as_str);
    let checks = tool.verify.get("checks").and_then(Value::as_str);
    if verdict.is_none() && checks.is_none() {
        return None;
    }
    let document = body.get("output").unwrap_or(body);
    if let Some(verdict) = verdict {
        let Some(value) = document.get(verdict) else {
            return Some(format!("the destination returned no `{verdict}` verdict"));
        };
        let passed = value.as_bool() == Some(true)
            || value.as_str().is_some_and(|word| {
                word.eq_ignore_ascii_case("ok")
                    || word.eq_ignore_ascii_case("pass")
                    || word.eq_ignore_ascii_case("passed")
            });
        if !passed {
            return Some(format!("the destination's `{verdict}` verdict is {value}"));
        }
    }
    if let Some(checks) = checks {
        let Some(entries) = document.get(checks).and_then(Value::as_array) else {
            return Some(format!("the destination returned no `{checks}` list"));
        };
        let mut failed = Vec::new();
        for entry in entries {
            // A check declares its outcome in whichever member its service uses; any of the three
            // is a failure, so a service cannot drift to a spelling the broker treats as a pass.
            let ok = entry.get("ok").and_then(Value::as_bool);
            let pass = entry.get("pass").and_then(Value::as_bool);
            let status = entry.get("status").and_then(Value::as_str);
            let entry_failed = ok == Some(false)
                || pass == Some(false)
                || status.is_some_and(|status| {
                    status.eq_ignore_ascii_case("fail") || status.eq_ignore_ascii_case("failed")
                });
            if entry_failed {
                failed.push(
                    entry
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unnamed")
                        .to_owned(),
                );
            }
        }
        if !failed.is_empty() {
            return Some(format!(
                "the write failed its read-back checks: {}",
                failed.join(", ")
            ));
        }
    }
    None
}

/// Resolve, check, validate, gate, call and ledger one tool call.
pub(crate) async fn invoke_broker(
    pool: &PgPool,
    spec: &crate::spec::ActionSpec,
    ctx: HandlerCtx,
    input: Value,
    request: &InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    if spec.name == crate::tools::TOOLS_RESUME {
        return invoke_resume(pool, ctx, input, request).await;
    }
    let call: ToolCallIn = serde_json::from_value(input).map_err(|error| {
        invalid_tool_input(
            "the tools.call input is not a call document",
            vec![field("body", error.to_string())],
        )
    })?;
    invoke_call(pool, ctx, request, call).await
}

/// The call path after the action shim: resolve, check, validate, call and ledger.
async fn invoke_call(
    pool: &PgPool,
    ctx: HandlerCtx,
    request: &InvokeRequest,
    call: ToolCallIn,
) -> Result<InvokeOutcome, ActionError> {
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

    // A destination whose toolkit declares an auth adapter other than `none` needs a tenant
    // credential, and the binding is where that credential is named. A binding with no
    // `credential_alias` cannot call it: the destination would otherwise receive the target
    // action's own tenant-wide secret (or none at all), which is exactly the credential leak
    // this refusal prevents. Typed `409` before any network call, like every other unready
    // binding (§11.3 step 1).
    if tool.credential_alias.is_none() && tool.auth_adapter != "none" {
        return Err(not_ready(format!(
            "{} requires a sealed credential (auth adapter {:?}) and this binding carries no \
             credential_alias: set tool_binding.credential_alias to the tenant secret's name",
            call.tool, tool.auth_adapter
        )));
    }

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

    // §11.1's second authorising row: the release's operation grant. A run pinned to a job
    // binding carries its release snapshot; the operation must be granted there and enabled, and
    // the grant's max_side_effect must cover the tool's effect. A release that declared no grants
    // allows none — absent is an empty grant, never "no bound".
    if let Some(job_binding_id) = run.job_binding_id {
        let snapshot =
            store::tool::release_snapshot_for_binding(&mut tx, ctx.tenant, job_binding_id)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "broker: read the run's release snapshot");
                    ActionError::internal()
                })?;
        if let Some(snapshot) = snapshot {
            let anatomy = parse_release_anatomy(&snapshot).map_err(|error| {
                tracing::error!(%error, tool = %call.tool, "broker: the release anatomy is invalid");
                ActionError::new(
                    403,
                    "tool_grant_forbidden",
                    format!("run {} pins a release with no valid operation grant", run.run_id),
                )
            })?;
            enforce_release_grant(&anatomy, &call.tool, effect)?;
        }
    }

    // §11.3 step 2's second half: a spend-class call is refused before the destination when the
    // run's budget is absent or exhausted. The refusal is typed and, because it happens here,
    // leaves neither a `tool_call` row nor a charge.
    if effect == Effect::Spend {
        check_spend_budget(&mut tx, ctx.tenant, &run).await?;
    }

    // SPEC_V2 §11.3: a tool whose version declares `required` may not be called without a
    // key. The refusal names the header before any destination is reached; `forbidden` is the
    // absence of the same contract for a tool that must not be replayed.
    if tool.idempotency == "required"
        && request
            .idempotency_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
    {
        return Err(invalid_tool_input(
            format!("{} requires an Idempotency-Key", call.tool),
            vec![field("idempotency_key", "required")],
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

    // §11.3 step 2's second half: a spend-class call is refused before the destination when the
    // run's budget is absent or exhausted. The refusal is typed and, because it happens here,
    // leaves neither a `tool_call` row nor a charge.
    if effect == Effect::Spend {
        check_spend_budget(&mut tx, ctx.tenant, &run).await?;
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
    // A destination is a declared external action (the estate toolkits) or the one registered-
    // destination dispatch (T15's authored/BYO tools). Anything else would be a second call path.
    let registered = matches!(target.handler, crate::spec::HandlerRef::ToolDestination);
    if target.kind != ActionKind::External && !registered {
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
    if registered && tool.source_id.is_none() {
        tracing::error!(tool = %call.tool, "broker: a registered tool carries no source");
        return Err(ActionError::new(
            500,
            "tool_destination_invalid",
            format!("{} carries no registered destination", call.tool),
        ));
    }

    let request_bytes = serde_json::to_vec(&call.input)
        .ok()
        .and_then(|bytes| i32::try_from(bytes.len()).ok());
    // A spend call must leave a cost-bearing ledger row, and the cost is derived from the
    // handler's `inference_call`. The row is attributed to the run being budgeted: a caller that
    // sent no usage context (a direct admin call) gets one, and a mis-named run is overridden.
    // Non-spend tools keep the caller's own context: they write no priced row.
    let mut usage = request.usage.clone();
    if effect == Effect::Spend {
        let usage = usage.get_or_insert(UsageContext {
            caller_kind: conveyor_domain::inference::CallerKind::Other,
            run_id: None,
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: None,
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: None,
        });
        usage.run_id = Some(run.run_id);
        usage.run_step_id = run.run_step_id;
    }
    // An estate tool's external action receives the tool's own input. A registered destination
    // receives the pinned source and operation — never a URL, never a caller's destination.
    let target_input = if registered {
        serde_json::json!({
            "source_id": tool.source_id,
            "toolkit_key": tool.toolkit_key,
            "tool_key": tool.tool_key,
            "destination": tool.destination,
            "arguments": call.input.clone(),
            "credential_alias": tool.credential_alias,
        })
    } else {
        call.input.clone()
    };
    // A spend call must leave a cost-bearing ledger row, and the cost is derived from the
    // handler's `inference_call`. The row is attributed to the run being budgeted: a caller that
    // sent no usage context (a direct admin call) gets one, and a mis-named run is overridden.
    // Non-spend tools keep the caller's own context: they write no priced row.
    let mut usage = request.usage.clone();
    if effect == Effect::Spend {
        let usage = usage.get_or_insert(UsageContext {
            caller_kind: conveyor_domain::inference::CallerKind::Other,
            run_id: None,
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: None,
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: None,
        });
        usage.run_id = Some(run.run_id);
        usage.run_step_id = run.run_step_id;
    }
    let target_request = InvokeRequest {
        action: tool.handler_action.clone(),
        input: target_input,
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
        usage,
        // The binding's sealed credential alias — the one thing the target action must use
        // instead of its own tenant-wide secret (§11.3).
        credential_alias: tool.credential_alias.clone(),
    };

    let started = Instant::now();
    // `invoke` dispatches the broker when it sees `HandlerRef::Broker`, so this edge is recursion
    // the compiler cannot size. Boxing the callee is the standard indirection; the depth counter
    // (`MAX_DEPTH`) is what actually bounds the chain.
    //
    // §11.3: the tool's own `timeout_secs` bounds the call. A destination that exceeds it is a
    // long operation, not a silent backend failure: the ledger row is `pending` with a resume
    // token, and `tools.resume` continues it.
    let destination_timeout = Duration::from_secs(u64::try_from(tool.timeout_secs).unwrap_or(300));
    let result = match tokio::time::timeout(
        destination_timeout,
        Box::pin(invoke(pool, registry(), ctx.tenant, target_request)),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            return pending_call(pool, &ctx, &run, &tool, &call, destination_timeout).await;
        }
    };
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

    // SPEC_V2 §11.3: a 2xx is not "ok" when the tool's own declared verdict or checks say the
    // effect did not happen. The ordinary 422 mapping below ledgers this `check_failed`.
    let outcome = match outcome {
        Ok(outcome) if (200..=299).contains(&outcome.status) => {
            match write_verification_failure(&tool, &outcome.body) {
                Some(detail) => Err(ActionError::new(
                    422,
                    "write_not_verified",
                    format!("{}: {detail}", call.tool),
                )),
                None => Ok(outcome),
            }
        }
        other => other,
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
    // The cost is the handler's own priced ledger rows, read at the same derived-cost formula
    // every report uses (D52). A replay has no new invocation and therefore no new charge.
    let input_hash = crate::spec::sha256_hex(&serde_json::to_vec(&call.input).unwrap_or_default());
    let call_id = store::tool::NewToolCall {
        run_id: Some(run.run_id),
        run_step_id: run.run_step_id,
        card_id: run.card_id,
        tool_id: tool.tool_id,
        binding_id: Some(tool.binding_id),
        tool_key: &call.tool,
        toolkit_key: &tool.toolkit_key,
        toolkit_version: tool.toolkit_version,
        input_hash: &input_hash,
        status,
        http_status,
        latency_ms,
        request_bytes,
        response_bytes,
        // Filled in `write_ledger` from the invocation's priced `inference_call` rows; `None`
        // only when the call produced no priced row at all (unpriced is not $0).
        cost: None,
        error_class,
        error_message: error_message.as_deref(),
        resume_token: None,
    };
    let invocation_id = outcome
        .as_ref()
        .ok()
        .and_then(|outcome| outcome.invocation_id);
    match write_ledger(pool, ctx.tenant, &call_id, invocation_id).await {
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

/// Record a long operation as `pending`: one ledger row, one resume token, a `202` envelope.
async fn pending_call(
    pool: &PgPool,
    ctx: &HandlerCtx,
    run: &store::tool::RunToolContext,
    tool: &store::tool::ResolvedTool,
    call: &ToolCallIn,
    timeout: Duration,
) -> Result<InvokeOutcome, ActionError> {
    let token = Uuid::now_v7().to_string();
    let input_hash = crate::spec::sha256_hex(&serde_json::to_vec(&call.input).unwrap_or_default());
    let latency_ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let row = store::tool::NewToolCall {
        run_id: Some(run.run_id),
        run_step_id: run.run_step_id,
        card_id: run.card_id,
        tool_id: tool.tool_id,
        binding_id: Some(tool.binding_id),
        tool_key: &call.tool,
        toolkit_key: &tool.toolkit_key,
        toolkit_version: tool.toolkit_version,
        input_hash: &input_hash,
        status: "pending",
        http_status: None,
        latency_ms,
        request_bytes: None,
        response_bytes: None,
        cost: None,
        error_class: None,
        error_message: Some("the destination exceeded the tool timeout"),
        resume_token: Some(&token),
    };
    let call_id = match write_ledger(pool, ctx.tenant, &row, None).await {
        Ok(id) => Some(id),
        Err(error) => {
            tracing::error!(%error, tool = %call.tool, "broker: write the pending ledger row");
            None
        }
    };
    let mut body = serde_json::json!({
        "status": "pending",
        "resume_token": token,
        "tool": call.tool,
        "timeout_secs": tool.timeout_secs,
    });
    if let Some(id) = call_id {
        body["tool_call_id"] = serde_json::json!(id);
    }
    Ok(InvokeOutcome::new(202, body))
}

/// `tools.resume` — continue a pending long operation (§11.3).
///
/// The token names a pending ledger row; the input must hash to the original. The run's binding
/// must still resolve the same tool version (a changed binding is a governance change, not a
/// resume), and a tool whose idempotency policy is `forbidden` is not resumable: retrying it
/// could repeat a side effect the tool never promised to make repeatable.
async fn invoke_resume(
    pool: &PgPool,
    ctx: HandlerCtx,
    input: Value,
    request: &InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    let resume: crate::tools::ResumeIn = serde_json::from_value(input).map_err(|error| {
        invalid_tool_input(
            "the tools.resume input is not a resume document",
            vec![field("body", error.to_string())],
        )
    })?;
    let mut tx = store::begin_scoped_read_only(pool, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "broker: begin the resume transaction");
            ActionError::internal()
        })?;
    let pending = store::tool::pending_call_by_token(&mut tx, ctx.tenant, &resume.resume_token)
        .await
        .map_err(|error| {
            tracing::error!(%error, "broker: read the pending call");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                404,
                "resume_token_unknown",
                "no pending call for that resume token",
            )
        })?;
    let input_hash =
        crate::spec::sha256_hex(&serde_json::to_vec(&resume.input).unwrap_or_default());
    if input_hash != pending.input_hash {
        return Err(ActionError::new(
            422,
            "resume_input_mismatch",
            "the supplied input does not hash to the pending call's input",
        ));
    }
    let run_id = pending.run_id.ok_or_else(|| {
        ActionError::new(
            409,
            "resume_not_possible",
            "the pending call carries no run to resume",
        )
    })?;
    let run = store::tool::run_context(&mut tx, ctx.tenant, run_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, run = %run_id, "broker: read the resume run");
            ActionError::internal()
        })?
        .ok_or_else(|| not_ready(format!("no run {run_id} in this tenant")))?;
    let Some((_toolkit, operation)) = pending.tool_key.split_once('.') else {
        return Err(ActionError::new(
            409,
            "resume_not_possible",
            "the pending tool name is not <toolkit>.<operation>",
        ));
    };
    let tool = store::tool::resolve(&mut tx, ctx.tenant, &run, &pending.toolkit_key, operation)
        .await
        .map_err(|error| {
            tracing::error!(%error, tool = %pending.tool_key, "broker: resolve the resume tool");
            ActionError::internal()
        })?
        .ok_or_else(|| not_ready(format!("{} is no longer bound", pending.tool_key)))?;
    let _ = tx.rollback().await;
    if tool.tool_id != pending.tool_id {
        return Err(ActionError::new(
            409,
            "tool_changed",
            "the binding now resolves a different tool version; re-issue the call",
        ));
    }
    if tool.idempotency == "forbidden" {
        return Err(ActionError::new(
            409,
            "tool_not_resumable",
            "this tool forbids retrying the call; it cannot be resumed",
        ));
    }
    let call = ToolCallIn {
        run_id,
        tool: pending.tool_key,
        input: resume.input,
        step_id: pending.run_step_id,
    };
    let mut nested = request.clone();
    nested.action = crate::tools::TOOLS_CALL.to_owned();
    nested.idempotency_key = Some(format!("resume:{}", resume.resume_token));
    invoke_call(pool, ctx, &nested, call).await
}

/// Append the ledger row and the `tool.called` event in one short transaction.
///
/// The event's topic is the tool version, so a metrics reader can follow one tool's health
/// without scanning every run (§11.9).
async fn write_ledger(
    pool: &PgPool,
    tenant: conveyor_domain::TenantId,
    call: &store::tool::NewToolCall<'_>,
    invocation_id: Option<Uuid>,
) -> Result<Uuid, String> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| error.to_string())?;
    // §11.3 step 7's cost, from the handler's actual ledger rows rather than a guess. A priced
    // call always writes a non-null cost; an unpriced one stays NULL by design.
    let mut call = *call;
    if let Some(invocation_id) = invocation_id
        && let Some(run_id) = call.run_id
    {
        let since = chrono::Utc::now() - chrono::Duration::minutes(10);
        call.cost =
            store::inference::cost_of_invocation_tx(&mut tx, tenant, run_id, invocation_id, since)
                .await
                .map_err(|error| error.to_string())?;
    }
    let call_id = store::tool::insert_call(&mut tx, tenant, &call)
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

/// The release grant's check (§11.1): the operation must be present and enabled, and the grant's
/// `max_side_effect` must cover the tool's effect. A release that declared no grants allows none.
fn enforce_release_grant(
    anatomy: &ReleaseAnatomy,
    op: &str,
    effect: Effect,
) -> Result<(), ActionError> {
    if !anatomy.grants_declared {
        return Err(ActionError::new(
            403,
            "tool_grant_forbidden",
            format!("the run's release declares no grants; {op} is not allowed"),
        ));
    }
    let Some(grant) = anatomy
        .grants
        .iter()
        .find(|grant| grant.op == op && grant.enabled)
    else {
        return Err(ActionError::new(
            403,
            "tool_grant_forbidden",
            format!("{op} is not granted by the run's release"),
        ));
    };
    if grant.max_side_effect.rank() < effect.rank() {
        return Err(ActionError::new(
            403,
            "tool_grant_forbidden",
            format!(
                "{op} is a {} tool and the release grants at most {}",
                effect.as_str(),
                grant.max_side_effect.as_str()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::Effect;
    use conveyor_domain::organisation::{ReleaseAnatomy, ReleaseGrant, ReleaseLimits, SideEffect};

    fn anatomy(grants: Vec<ReleaseGrant>, grants_declared: bool) -> ReleaseAnatomy {
        ReleaseAnatomy {
            job_key: "delivery".to_owned(),
            title: "Delivery".to_owned(),
            executor_kind: None,
            model_ref: None,
            prompt_recipe: None,
            procedure_ref: None,
            charter: None,
            authority: None,
            gate: None,
            grants,
            grants_declared,
            limits: ReleaseLimits::default(),
        }
    }

    fn grant(op: &str, max: SideEffect, enabled: bool) -> ReleaseGrant {
        ReleaseGrant {
            op: op.to_owned(),
            variant: None,
            max_side_effect: max,
            enabled,
        }
    }

    #[test]
    fn an_operation_absent_from_the_grant_is_refused() {
        let anatomy = anatomy(vec![grant("coding.jobs.get", SideEffect::Read, true)], true);
        let error = enforce_release_grant(&anatomy, "coding.jobs.create", Effect::Write)
            .expect_err("an ungranted operation must be refused");
        assert_eq!(error.status, 403);
        assert_eq!(error.code, "tool_grant_forbidden");
    }

    #[test]
    fn a_disabled_grant_is_refused() {
        let anatomy = anatomy(
            vec![grant("coding.jobs.create", SideEffect::Write, false)],
            true,
        );
        assert!(
            enforce_release_grant(&anatomy, "coding.jobs.create", Effect::Write).is_err(),
            "a disabled grant must not authorise a call"
        );
    }

    #[test]
    fn a_grant_below_the_tools_effect_is_refused() {
        let anatomy = anatomy(
            vec![grant("coding.jobs.create", SideEffect::Read, true)],
            true,
        );
        let error = enforce_release_grant(&anatomy, "coding.jobs.create", Effect::Write)
            .expect_err("a read grant must not cover a write tool");
        assert_eq!(error.status, 403);
        assert_eq!(error.code, "tool_grant_forbidden");
    }

    #[test]
    fn a_grant_at_the_tools_effect_is_accepted() {
        let anatomy = anatomy(
            vec![grant("coding.jobs.create", SideEffect::Write, true)],
            true,
        );
        assert!(
            enforce_release_grant(&anatomy, "coding.jobs.create", Effect::Write).is_ok(),
            "an equal-max grant authorises the call"
        );
    }

    #[test]
    fn an_undeclared_grant_allows_nothing() {
        let anatomy = anatomy(Vec::new(), false);
        let error = enforce_release_grant(&anatomy, "coding.jobs.read", Effect::Read)
            .expect_err("a release that declared no grants allows none");
        assert_eq!(error.status, 403);
        assert_eq!(error.code, "tool_grant_forbidden");
    }
}
