//! `POST /api/chat/completions` — the one route every consumer's inference goes through.
//!
//! Non-streaming dispatch only in this slice. The request carries a long list of attribution
//! headers; every one is optional and a malformed value is simply absent — a bad header must
//! never fail a customer's inference.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::Stream;

use crate::auth::{self, CallerIdentity, EffectiveTenant};
use crate::config::{GateMode, PolicyMode};
use crate::contract::chunks::ChatCompletionChunk;
use crate::contract::schemas::{
    ChatCompletionRequest, ChatCompletionResponse, HTTPValidationError, Usage,
};
use crate::ledger::{self, CallContext, LedgerRow, StreamRow};
use crate::metering::{
    CostEvent, ZERO_COST_USD, licence_is_absent, tenant_id_rejection_reason, tenant_is_unattributed,
};
use crate::policy::{self, ResolvedHop};
use crate::provider::{BoxFuture, ChunkStream};
use crate::routing::{
    CompletionOutcome, OUTCOME_EMPTY, OUTCOME_EMPTY_RESULT, OUTCOME_ERROR, OUTCOME_OK,
    ProviderAttempt, Refusal, ServedHop, ServedStream, StreamOutcome,
};
use crate::state::AppState;
use crate::stream::{
    STREAM_CLIENT_DISCONNECT, STREAM_COMPLETED, STREAM_UPSTREAM_ERROR, StreamCapture,
};

/// FastAPI's body-validation document. The common case is a missing required field, where the
/// reference answers `{type: "missing", loc: ["body", field], msg: "Field required", input}`;
/// anything else is the same envelope with a generic message (no golden covers it).
fn validation_refusal(body: &[u8], error: &serde_json::Error) -> Refusal {
    let message = error.to_string();
    let (kind, msg, field) = match message.strip_prefix("missing field `") {
        Some(rest) => ("missing", "Field required", rest.split('`').next()),
        None => ("value_error", "Input should be valid", None),
    };
    let mut loc = vec![serde_json::json!("body")];
    if let Some(field) = field {
        loc.push(serde_json::json!(field));
    }
    let input: serde_json::Value = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    Refusal {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        body: serde_json::json!({
            "detail": [{"type": kind, "loc": loc, "msg": msg, "input": input}]
        }),
        headers: Vec::new(),
    }
}

/// The reference's fixed 400 body for a request its policy cannot serve.
const CUSTOMER_UNAVAILABLE: &str = "The requested provider or model is not available to this \
                                    tenant. Request model 'local-llm'.";

/// The measured 401 body when a policy demands a proven tenant and the caller is not.
const TENANT_NOT_PROVEN: &str =
    "Caller licence could not be introspected; caller is not authorised";

/// What a policy decided for this request, beyond the hops themselves.
struct PolicyDecision {
    hops: Vec<ResolvedHop>,
    version: i32,
    llm_setup: i16,
}

#[utoipa::path(
    post,
    path = "/api/chat/completions",
    tag = "completions",
    summary = "Create a chat completion",
    description = "OpenAI-compatible chat completion endpoint with intelligent routing and \
                   failover across vLLM, DeepSeek, and Claude.",
    params(
        ("X-Job-Id" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Actor-Id" = Option<String>, Header),
        ("X-Project-Id" = Option<String>, Header),
        ("X-Spend-Scope" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Provider" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-Correlation-Id" = Option<String>, Header),
        ("X-AA-Trace-Id" = Option<String>, Header),
        ("X-AA-Card-Id" = Option<String>, Header),
        ("X-AA-Parent-Exec" = Option<String>, Header),
        ("X-AA-Prompt-Version" = Option<String>, Header),
        ("X-AA-Prompt-Sha256" = Option<String>, Header),
        ("X-AA-Context" = Option<String>, Header),
        ("X-Stage" = Option<String>, Header),
        ("X-Replay-Mode" = Option<String>, Header),
        ("X-Worker-Version" = Option<String>, Header),
        ("X-Capture" = Option<String>, Header),
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    request_body = ChatCompletionRequest,
    responses(
        (status = 200, body = ChatCompletionResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn create_chat_completion(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // Parsed here, not by axum's `Json` extractor: FastAPI answers an invalid body with a 422
    // JSON document (`detail[0] = {type, loc, msg, input}`), while axum's rejection is a
    // text/plain 400. The captured `completion_validation` golden is the former.
    let request: ChatCompletionRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => return validation_refusal(&body, &error).into_response(),
    };
    // The licence gate runs before anything is dialled: an unverifiable caller must never reach a
    // provider or the ledger.
    // Whose request this is: the caller's own tenant, or one a vouched allow-listed subject
    // named. Resolved once, before any policy, budget or ledger decision, so all three agree.
    let (identity, effective) = match state.caller(&headers).await {
        Ok(caller) => caller,
        Err(refusal) => return refusal.into_response(),
    };
    // The admission counter, booked here for the same reason the reference books it inside the
    // handler: the licence gate above already ran, so a refused caller is NEVER counted, while both
    // the streaming and the non-streaming paths below are. Only a real UUID reaches the ledger;
    // the metric is what tells absent, valid and malformed apart.
    let project_outcome = match project_id(&headers) {
        Some(_) => "valid",
        None if attribution(&headers, "x-project-id").is_some() => "malformed",
        None => "absent",
    };
    let x_app_id = headers
        .get("x-app-id")
        .and_then(|value| value.to_str().ok());
    crate::metrics::record_project_id(project_outcome, &crate::metrics::caller_label(x_app_id));
    if request.stream {
        return stream_completion(state, headers, request, identity, effective).await;
    }
    let x_provider = headers
        .get("x-provider")
        .and_then(|value| value.to_str().ok());
    // Policy routing is resolved AFTER the tenant is final and BEFORE anything can spend.
    let decision = match tenant_policy(&state, &identity, &effective, &request, x_provider).await {
        Ok(decision) => decision,
        Err(refusal) => return refusal.into_response(),
    };
    let hops = decision.as_ref().map(|decision| decision.hops.as_slice());
    // The pre-spend gate is at ADMISSION: every path that can spend money passes through here
    // first, including the failover attempts and the (future) canary forward.
    let tenant = effective.tenant_id.as_deref().unwrap_or_default();
    let gate = state
        .deps()
        .budget
        .reserve(tenant, estimate_tokens(&request))
        .await;
    if state.config().budget_gate_mode == GateMode::Enforce && gate.reason.deniable() {
        return budget_refusal(&gate).into_response();
    }
    let outcome = state.router().complete(&request, x_provider, hops).await;
    // The write path runs on EVERY outcome, not only on success: a request that failed over spent
    // money on the attempts that did not serve it, and the cost bus is billed per attempt.
    // The "who" half of the context is decided here; `record` fills in the attribution and
    // policy fields. Seven arguments is clippy's limit — bundling the identity fields keeps the
    // write path inside it without an allow.
    let context = CallContext {
        tenant_id: effective.tenant_id.clone(),
        license_id: identity.license_id.clone(),
        delegated_by: effective.delegated_by.clone(),
        actor_id: actor_id(&headers, state.config().service_actor_id.as_deref()),
        ..CallContext::default()
    };
    record(
        &state,
        context,
        &headers,
        &request,
        &outcome,
        decision.as_ref(),
        gate.reservation_id.as_deref(),
    )
    .await;
    match outcome.response {
        Ok(response) => Json(response).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

/// The reference's token estimate: `max(1, chars // 4)` over the prompt text, where a
/// multi-part message contributes the sum of its `text` parts.
fn estimate_tokens(request: &ChatCompletionRequest) -> i64 {
    let mut chars = 0_i64;
    for message in &request.messages {
        match &message.content {
            serde_json::Value::String(text) => chars += text.chars().count() as i64,
            serde_json::Value::Array(parts) => {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                        chars += text.chars().count() as i64;
                    }
                }
            }
            _ => {}
        }
    }
    (chars / 4).max(1)
}

/// The measured 429 body: FastAPI wraps the detail, and the detail carries the numbers so the
/// caller can see how far over its cap it is rather than having to ask an operator.
fn budget_refusal(decision: &crate::billing::GateDecision) -> Refusal {
    Refusal {
        status: StatusCode::TOO_MANY_REQUESTS,
        body: serde_json::json!({
            "detail": {
                "error": {
                    "message": "token budget exhausted for this tenant; the request was refused \
                                before any provider was called",
                    "type": "budget_exceeded",
                    "reason": decision.reason.as_str(),
                    "remaining_tokens": decision.remaining_tokens,
                    "cap_tokens": decision.cap_tokens,
                }
            }
        }),
        headers: vec![("X-IR-Error-Classification", "budget_exceeded".to_owned())],
    }
}

/// The tenant's active policy, if one applies to this request.
///
/// A policy applies only when `POLICY_MODE=on`, the caller is not a PROVEN platform tenant, and
/// the tenant actually has one. An asserted platform id is a customer: its policy applies.
async fn tenant_policy(
    state: &AppState,
    identity: &CallerIdentity,
    effective: &EffectiveTenant,
    request: &ChatCompletionRequest,
    x_provider: Option<&str>,
) -> Result<Option<PolicyDecision>, Refusal> {
    if state.config().policy_mode != PolicyMode::On {
        return Ok(None);
    }
    let Some(tenant) = effective.tenant_id.as_deref() else {
        return Ok(None);
    };
    if auth::has_commercial_access(
        &state.config().commercial_platform_tenant_ids,
        identity,
        Some(tenant),
    ) {
        return Ok(None);
    }
    let Some(active) = state.deps().policy.get_active(tenant).await else {
        return Ok(None);
    };
    if active.requires_proven_tenant() && !auth::proven_tenant(identity, tenant) {
        let mut refusal = Refusal::detail(
            StatusCode::UNAUTHORIZED,
            TENANT_NOT_PROVEN,
            "tenant_not_proven",
        );
        refusal
            .headers
            .push(("WWW-Authenticate", "Bearer".to_owned()));
        return Err(refusal);
    }
    let model_explicit = request.model != policy::DEFAULT_MODEL;
    match policy::resolve(&active.doc, &request.model, model_explicit, x_provider) {
        Some(hops) => Ok(Some(PolicyDecision {
            hops,
            version: active.version,
            llm_setup: active.llm_setup(),
        })),
        None => Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            CUSTOMER_UNAVAILABLE,
            "outside_policy",
        )),
    }
}

/// Read an attribution header. Absent, non-UTF-8 and blank all mean "the caller sent none" — the
/// ledger distinguishes that from an empty value, and a bad header never fails the request.
fn attribution(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The caller's `X-Project-Id`, when it is a UUID. Absent, non-UTF-8 and blank all mean "the caller
/// sent none", and a present value that is not a UUID means the same thing — a bad header never
/// fails a request. The admission metric records which of the three happened; the ledger stores
/// only a real identifier.
fn project_id(headers: &HeaderMap) -> Option<String> {
    let value = attribution(headers, "x-project-id")?;
    uuid::Uuid::parse_str(&value).ok().map(|_| value)
}

/// The actor to stamp on a cost event: the caller's `X-Actor-Id` when it is a UUID, else the
/// service's own subject (`SERVICE_ACTOR_ID`).
///
/// A call no human made belongs to the calling service, which is what the billing attribution
/// rule means by `actor_id` and what its triage instructs (`printenv SERVICE_ACTOR_ID` in the
/// emitter pod). This is the estate-wide convention; the Python reference instead classified the
/// introspected licence holder as `tenant`/`member` and never defaulted a person. That
/// classification is not ported here because the Rust introspection client does not parse the
/// subject kind — the explicit env value is the port's equivalent and is set on the Deployment.
///
/// Absent, blank, non-UTF-8 and non-UUID header values all mean "no actor was sent": a bad header
/// never fails a request, and an unset service id leaves the field empty rather than invented.
fn actor_id(headers: &HeaderMap, service_actor: Option<&str>) -> Option<String> {
    let sent =
        attribution(headers, "x-actor-id").filter(|value| uuid::Uuid::parse_str(value).is_ok());
    sent.or_else(|| {
        service_actor
            .map(str::trim)
            .filter(|value| uuid::Uuid::parse_str(value).is_ok())
            .map(str::to_owned)
    })
}

/// Add the attribution headers and the policy hop to a call context. Shared by the
/// non-streaming and the streaming write paths so the two rows cannot drift.
fn attributed(
    context: CallContext,
    headers: &HeaderMap,
    decision: Option<&PolicyDecision>,
    served_hop: Option<&ServedHop>,
) -> CallContext {
    CallContext {
        correlation_id: attribution(headers, "x-correlation-id"),
        worker_name: attribution(headers, "x-worker-name"),
        app_id: attribution(headers, "x-app-id"),
        stage: attribution(headers, "x-stage"),
        trace_id: attribution(headers, "x-aa-trace-id"),
        card_id: attribution(headers, "x-aa-card-id"),
        project_id: project_id(headers),
        parent_execution_id: attribution(headers, "x-aa-parent-exec"),
        // Only a policy-routed request carries these; a global-routed one leaves them null.
        policy_version: decision.map(|decision| decision.version),
        llm_setup: decision.map(|decision| decision.llm_setup),
        hop_index: served_hop.map(|hop| hop.index),
        hop_reason: served_hop.map(|hop| hop.reason.to_owned()),
        key_owner: served_hop.map(|hop| hop.key_owner.to_owned()),
        ..context
    }
}

/// The reference's `capture_allowed`: `X-Capture: full` is honoured only when `X-App-Id` is on
/// `INFERENCE_CAPTURE_ALLOWED_CALLERS`. Every identity header here is caller-asserted, which is
/// exactly why the allowlist — not the header alone — is the gate. A refused request is served
/// normally and stores hashes only.
fn capture_allowed(headers: &HeaderMap, allowlist: &[String]) -> bool {
    let requested = headers
        .get("x-capture")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("full"));
    if !requested {
        return false;
    }
    let app_id = headers
        .get("x-app-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim();
    !app_id.is_empty() && allowlist.iter().any(|caller| caller.trim() == app_id)
}

/// Write one ledger row, booking a refused write onto the write-failure family.
///
/// `db_error` is the reference's insert/commit failure (`inference_ledger.py:717`);
/// `db_not_initialised` is its missing-session-factory outcome (`:619`), which in this port is the
/// absent pool only unit and health builds have. The reference's other two reasons are NOT
/// simulated, because their mechanisms do not exist here: `backpressure` needs its bounded
/// in-flight task set, and `stream_capture` needs its swallowed `on_finish` finaliser. Both writes
/// in this port are awaited inline in the request path, so neither can fail independently; the two
/// pre-seeded series stay, so "always zero" is distinguishable from "never registered".
async fn write_ledger(state: &AppState, row: &LedgerRow) {
    let Some(pool) = state.deps().ledger.as_ref() else {
        crate::metrics::record_write_failed("db_not_initialised");
        return;
    };
    if let Err(error) = ledger::insert(pool, row).await {
        crate::metrics::record_write_failed("db_error");
        tracing::warn!(%error, "inference ledger write failed");
    }
}

/// One ledger row for the served attempt, and one cost event per attempt.
///
/// Both failures are booked, logged and dropped: the reference never turns a write-path problem
/// into a failed inference, and neither does this port.
async fn record(
    state: &AppState,
    context: CallContext,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
    outcome: &CompletionOutcome,
    decision: Option<&PolicyDecision>,
    reservation_id: Option<&str>,
) {
    let served_hop = outcome.served_hop.as_ref();
    let mut context = attributed(context, headers, decision, served_hop);
    context.capture_full =
        capture_allowed(headers, &state.config().inference_capture_allowed_callers);

    let served = outcome.response.as_ref().ok();
    // The served row is the answer the caller actually received: the LAST attempt, whatever its
    // outcome. A blank 200 (a ground-truth provider, or before the retry lands) is still a call the
    // caller saw; the attempt's `outcome` feeds the metric and the retry decision, not this
    // selection.
    let served_attempt = served.and(outcome.attempts.last());

    if let (Some(response), Some(attempt)) = (served, served_attempt) {
        let row = LedgerRow::served(
            &context,
            request,
            response,
            attempt.bodies.as_ref(),
            attempt.latency.as_millis() as i64,
        );
        write_ledger(state, &row).await;
    } else if let Err(refusal) = &outcome.response {
        // A request that reached a provider is a call the estate paid for, even when the answer is
        // refused — the reference records it (measured 2026-11-15 from the contract oracle: the
        // second-blank refusal lands a row with status '502' and no response columns). A refusal
        // taken BEFORE dispatch — policy, budget, auth, validation — has no attempt and writes
        // none, so the row is gated on the attempts rather than on the outcome alone.
        if !outcome.attempts.is_empty() {
            let latency_ms: i64 = outcome
                .attempts
                .iter()
                .map(|attempt| attempt.latency.as_millis() as i64)
                .sum();
            let row = LedgerRow::refused(
                &context,
                request,
                &refusal.status.as_u16().to_string(),
                latency_ms,
            );
            write_ledger(state, &row).await;
        }
    }

    let caller = crate::metrics::caller_label(context.app_id.as_deref());
    for (index, attempt) in outcome.attempts.iter().enumerate() {
        crate::metrics::record_attempt(
            &attempt.provider,
            attempt.path,
            attempt.outcome,
            &caller,
            attempt.latency,
            attempt_reason(attempt.outcome, attempt),
        );
        let event = cost_event(
            &context,
            request,
            served,
            attempt,
            index > 0,
            reservation_id,
            state.router().catalog().provider(&attempt.provider),
        );
        emit_cost(state, &event).await;
    }
}

/// The `provider_errors_total` reason for one attempt.
///
/// Only two populations carry one: a failed attempt (its classification) and a completed-but-blank
/// one (`upstream_empty_response`). `empty_result` is an observation, not an error — it deliberately
/// books nothing, so it cannot be paged on as a failure.
fn attempt_reason(outcome: &str, attempt: &ProviderAttempt) -> Option<&'static str> {
    match outcome {
        OUTCOME_EMPTY => Some("upstream_empty_response"),
        OUTCOME_ERROR => attempt
            .failure
            .as_ref()
            .map(|failure| failure.classification),
        _ => None,
    }
}

/// The unlicensed trio's inputs for one event, or `None` when the event carries a licence.
///
/// Split out of [`emit_cost`] so the classification is unit-testable without a live recorder. The
/// reference decides "unlicensed" on the licence identifier alone ([`licence_is_absent`] — none,
/// blank, or the literal `unknown`); the caller and worker labels go through the same bounded
/// normalisers as every other family, so a raw header can never reach a series.
fn unlicensed_trio(event: &CostEvent) -> Option<(String, String, f64, i64)> {
    if !licence_is_absent(&event.license_id) {
        return None;
    }
    let caller = crate::metrics::caller_label(event.app_id.as_deref());
    let worker = crate::metrics::worker_label(event.worker_name.as_deref());
    // A malformed decimal books zero dollars; it never fails the request.
    let cost_usd = event.cost_usd.parse::<f64>().unwrap_or(0.0);
    let tokens = event.input_tokens.max(0) + event.output_tokens.max(0);
    Some((caller, worker, cost_usd, tokens))
}

/// Book one cost event: refuse an unattributable tenant FIRST, then the unlicensed trio, then the
/// XADD, then the token totals ONLY when the sink confirms delivery.
///
/// The order is the reference's own (`metering.py` :574, :679, :797-835, then :1011-1035). Billing
/// dead-letters an event with no usable tenant, so the reference never emits one — it counts the
/// refusal on its own alert-keyed family and returns before the trio, the XADD and the tokens. The
/// trio then records what was spent without a licence whether or not the stream write succeeds
/// (the spend happened either way), while `tokens_total` is booked after a successful XADD, so a
/// dropped event cannot inflate the token families. Every emit site goes through this one helper
/// so none can drift.
async fn emit_cost(state: &AppState, event: &CostEvent) {
    if tenant_is_unattributed(&event.tenant_id) {
        crate::metrics::record_cost_unattributed(&event.service_key, &event.provider);
        return;
    }
    if let Some(reason) = event
        .tenant_id
        .as_deref()
        .and_then(tenant_id_rejection_reason)
    {
        crate::metrics::record_cost_malformed_tenant(&event.service_key, &event.provider, reason);
        return;
    }
    if let Some((caller, worker, cost_usd, tokens)) = unlicensed_trio(event) {
        crate::metrics::record_cost_unlicensed(
            &event.service_key,
            &event.provider,
            &caller,
            &worker,
            cost_usd,
            tokens,
        );
    }
    if state.deps().cost.emit(event).await {
        crate::metrics::record_tokens(
            &event.service_key,
            &event.provider,
            "prompt",
            event.input_tokens,
        );
        crate::metrics::record_tokens(
            &event.service_key,
            &event.provider,
            "completion",
            event.output_tokens,
        );
    }
}

/// The terminal outcome of a streamed attempt, as the reference's `_record_stream_attempt`
/// computes it.
///
/// A stream that never reached `[DONE]` — client disconnect or an upstream error mid-stream — is
/// `aborted`: we do not know what it would have produced, so counting it `empty` would let a
/// flaky caller manufacture the signal the empty-completion alert exists to detect. Only a
/// completed stream is classified, and a tool-call reply is a complete answer with no text.
fn stream_outcome(status: &'static str, capture: &StreamCapture) -> &'static str {
    if status != STREAM_COMPLETED {
        return "aborted";
    }
    match crate::empty::text_reason(capture.content(), capture.saw_tool_call()) {
        Some(crate::empty::EmptyReason::EmptyResult) => OUTCOME_EMPTY_RESULT,
        Some(crate::empty::EmptyReason::Blank) => OUTCOME_EMPTY,
        None => OUTCOME_OK,
    }
}

/// The reference's `calculate_cost`: tokens × the per-million catalog price ÷ 10^6, rendered at
/// the fixed 8 decimals the cost event carries (`metering.py:411-429`). A provider the catalog
/// does not know prices at zero, and negative token counts never price negative — the reference's
/// `max(0.0, …)` clamp.
fn cost_usd(
    input_tokens: i64,
    output_tokens: i64,
    entry: Option<&crate::catalog::ProviderEntry>,
) -> String {
    let Some(entry) = entry else {
        return ZERO_COST_USD.to_owned();
    };
    let input_tokens = input_tokens.max(0) as f64;
    let output_tokens = output_tokens.max(0) as f64;
    let usd = (input_tokens * entry.cost_input_per_1m + output_tokens * entry.cost_output_per_1m)
        / 1_000_000.0;
    format!("{:.8}", usd.max(0.0))
}

fn cost_event(
    context: &CallContext,
    request: &ChatCompletionRequest,
    served: Option<&ChatCompletionResponse>,
    attempt: &ProviderAttempt,
    fallback_used: bool,
    reservation_id: Option<&str>,
    catalog_entry: Option<&crate::catalog::ProviderEntry>,
) -> CostEvent {
    // Tokens are per ATTEMPT, never the served response's: every attempt spends its own tokens,
    // and a failed attempt (no usage) must bill zero rather than inherit the served usage.
    let usage = attempt.usage.as_ref();
    let input_tokens = usage.map_or(0, |usage| usage.prompt_tokens);
    let output_tokens = usage.map_or(0, |usage| usage.completion_tokens);
    CostEvent {
        tenant_id: context.tenant_id.clone(),
        license_id: context.license_id.clone(),
        correlation_id: context.correlation_id.clone(),
        stage: context.stage.clone(),
        worker_name: context.worker_name.clone(),
        service_key: crate::SERVICE_NAME.to_owned(),
        cost_source: crate::SERVICE_NAME.to_owned(),
        provider: attempt.provider.clone(),
        model: attempt
            .model_override
            .clone()
            .unwrap_or_else(|| request.model.clone()),
        served_model: served.and_then(|response| response.served_model.clone()),
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: usage.map_or(0, |usage| usage.cache_creation_input_tokens),
        cache_read_input_tokens: usage.map_or(0, |usage| usage.cache_read_input_tokens),
        cost_usd: cost_usd(input_tokens, output_tokens, catalog_entry),
        latency_ms: attempt.latency.as_millis() as i64,
        schema_id: Some("default".to_owned()),
        fallback_used,
        provider_request_id: served.and_then(|response| response.provider_request_id.clone()),
        usage_extra: Some(
            usage
                .and_then(|usage| usage.usage_extra.as_ref())
                .map(|extra| serde_json::to_string(extra).unwrap_or_else(|_| "{}".to_owned()))
                .unwrap_or_else(|| "{}".to_owned()),
        ),
        // Billing releases a reservation only by its handle; naming it is what stops a
        // settlement draining another caller's hold.
        reservation_id: reservation_id.map(str::to_owned),
        project_id: context.project_id.clone(),
        actor_id: context.actor_id.clone(),
        card_id: context.card_id.clone(),
        execution_id: context.parent_execution_id.clone(),
        delegated_by: context.delegated_by.clone(),
        ..CostEvent::default()
    }
}

// ── streaming ─────────────────────────────────────────────────────────────────────────────────

/// The streamed completion path: policy, budget, dispatch, then an SSE body that relays every
/// chunk unchanged and books exactly one row when the stream ends (completed, upstream error or
/// client disconnect). A stream that never opens still books one row, with `stream_status` null
/// and the HTTP code as its status.
///
/// The head is flushed before the upstream's first chunk when the open takes longer than
/// `stream_headers_grace_ms`: a callers' first-byte budget must not be spent on a prefill.
/// Failures that arrive inside the grace keep their HTTP status, because the grace covers the
/// provider slot wait — a shed is still a 503, never a mid-stream surprise.
async fn stream_completion(
    state: AppState,
    headers: HeaderMap,
    request: ChatCompletionRequest,
    identity: CallerIdentity,
    effective: EffectiveTenant,
) -> Response {
    let started = Instant::now();
    let x_provider = headers
        .get("x-provider")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let decision = match tenant_policy(
        &state,
        &identity,
        &effective,
        &request,
        x_provider.as_deref(),
    )
    .await
    {
        Ok(decision) => decision,
        Err(refusal) => return refusal.into_response(),
    };
    let tenant = effective.tenant_id.as_deref().unwrap_or_default();
    let gate = state
        .deps()
        .budget
        .reserve(tenant, estimate_tokens(&request))
        .await;
    if state.config().budget_gate_mode == GateMode::Enforce && gate.reason.deniable() {
        return budget_refusal(&gate).into_response();
    }

    // Owned so the response body can keep waiting after the head has been sent.
    let open_router = state.router_shared();
    let open_request = request.clone();
    let open_hops = decision.as_ref().map(|decision| decision.hops.clone());
    let mut open: BoxFuture<'static, StreamOutcome> = Box::pin(async move {
        open_router
            .stream(&open_request, x_provider.as_deref(), open_hops.as_deref())
            .await
    });

    let grace_ms = state.config().stream_headers_grace_ms;
    let outcome = if grace_ms == 0 {
        Some(open.as_mut().await)
    } else {
        // The head goes out now; the open future stays alive in the body.
        tokio::time::timeout(Duration::from_millis(grace_ms), open.as_mut())
            .await
            .ok()
    };

    match outcome {
        Some(outcome) => {
            let context = stream_context(
                &state,
                &headers,
                &identity,
                &effective,
                decision.as_ref(),
                outcome.served_hop.as_ref(),
            );
            match outcome.stream {
                Ok(served) => sse_response(Body::from_stream(RelayStream::new(
                    state,
                    context,
                    request,
                    outcome.attempts,
                    gate.reservation_id.clone(),
                    served,
                    started,
                ))),
                Err(refusal) => {
                    record_unopened_stream(
                        &state,
                        context,
                        &request,
                        &outcome.attempts,
                        refusal.status,
                        gate.reservation_id.as_deref(),
                        started,
                    )
                    .await;
                    refusal.into_response()
                }
            }
        }
        None => {
            let ticker =
                tokio::time::interval(Duration::from_millis(state.config().stream_keepalive_ms));
            let body = GracefulStream {
                state,
                headers,
                request,
                identity,
                effective,
                decision,
                gate,
                started,
                open,
                ticker,
                stage: Stage::Pending,
            };
            sse_response(Body::from_stream(body))
        }
    }
}

/// A legal SSE comment: parsers ignore it, but it is a body byte, so the caller's idle watchdog
/// and every proxy between here and it see progress while the upstream is still prefilling.
const SSE_KEEPALIVE: &[u8] = b": keepalive\n\n";

fn sse_response(body: Body) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream; charset=utf-8")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .header("x-accel-buffering", "no")
        .body(body)
        .expect("a static response head is valid")
}

fn stream_context(
    state: &AppState,
    headers: &HeaderMap,
    identity: &CallerIdentity,
    effective: &EffectiveTenant,
    decision: Option<&PolicyDecision>,
    served_hop: Option<&ServedHop>,
) -> CallContext {
    attributed(
        CallContext {
            tenant_id: effective.tenant_id.clone(),
            license_id: identity.license_id.clone(),
            delegated_by: effective.delegated_by.clone(),
            actor_id: actor_id(headers, state.config().service_actor_id.as_deref()),
            ..CallContext::default()
        },
        headers,
        decision,
        served_hop,
    )
}

/// The body of a stream whose head went out before the upstream opened. It yields keepalives
/// while the open is pending, then relays everything exactly as the buffered path does. An open
/// that fails after the head becomes an OpenAI-style `{"error": …}` frame plus `[DONE]`, with the
/// same never-opened ledger row the buffered path writes (the refusal's HTTP code is the row's
/// status even though the client already has a 200).
struct GracefulStream {
    state: AppState,
    headers: HeaderMap,
    request: ChatCompletionRequest,
    identity: CallerIdentity,
    effective: EffectiveTenant,
    decision: Option<PolicyDecision>,
    gate: crate::billing::GateDecision,
    started: Instant,
    open: BoxFuture<'static, StreamOutcome>,
    ticker: tokio::time::Interval,
    stage: Stage,
}

enum Stage {
    Pending,
    Open(Box<RelayStream>),
    Frames(VecDeque<Bytes>),
    Done,
}

impl Stream for GracefulStream {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match std::mem::replace(&mut this.stage, Stage::Done) {
                Stage::Done => return Poll::Ready(None),
                Stage::Frames(mut frames) => {
                    if let Some(frame) = frames.pop_front() {
                        this.stage = Stage::Frames(frames);
                        return Poll::Ready(Some(Ok(frame)));
                    }
                    return Poll::Ready(None);
                }
                Stage::Open(mut relay) => match Pin::new(relay.as_mut()).poll_next(cx) {
                    Poll::Ready(Some(item)) => {
                        this.stage = Stage::Open(relay);
                        return Poll::Ready(Some(item));
                    }
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Pending => {
                        this.stage = Stage::Open(relay);
                        return Poll::Pending;
                    }
                },
                Stage::Pending => {
                    if let Poll::Ready(outcome) = this.open.as_mut().poll(cx) {
                        open_graceful(this, outcome);
                        continue;
                    }
                    match this.ticker.poll_tick(cx) {
                        Poll::Ready(_) => {
                            this.stage = Stage::Pending;
                            return Poll::Ready(Some(Ok(Bytes::from_static(SSE_KEEPALIVE))));
                        }
                        Poll::Pending => {
                            this.stage = Stage::Pending;
                            return Poll::Pending;
                        }
                    }
                }
            }
        }
    }
}

/// The open resolved after the head was sent: hand over to the normal relay, or end the stream
/// with an error frame and the never-opened ledger row.
fn open_graceful(this: &mut GracefulStream, outcome: StreamOutcome) {
    let context = stream_context(
        &this.state,
        &this.headers,
        &this.identity,
        &this.effective,
        this.decision.as_ref(),
        outcome.served_hop.as_ref(),
    );
    match outcome.stream {
        Ok(served) => {
            this.stage = Stage::Open(Box::new(RelayStream::new(
                this.state.clone(),
                context,
                this.request.clone(),
                outcome.attempts,
                this.gate.reservation_id.clone(),
                served,
                this.started,
            )));
        }
        Err(refusal) => {
            let status = refusal.status;
            let attempts = outcome.attempts;
            let frame = stream_error_frame(&refusal);
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let state = this.state.clone();
                let request = this.request.clone();
                let reservation_id = this.gate.reservation_id.clone();
                let started = this.started;
                handle.spawn(async move {
                    record_unopened_stream(
                        &state,
                        context,
                        &request,
                        &attempts,
                        status,
                        reservation_id.as_deref(),
                        started,
                    )
                    .await;
                });
            }
            this.stage = Stage::Frames(VecDeque::from([
                frame,
                Bytes::from_static(b"data: [DONE]\n\n"),
            ]));
        }
    }
}

/// An OpenAI-shaped error frame for a failure that happened after the head went out. A `detail`
/// refusal is wrapped; an `error` refusal is sent as-is.
fn stream_error_frame(refusal: &Refusal) -> Bytes {
    let payload = if refusal.body.get("error").is_some() {
        refusal.body.clone()
    } else {
        serde_json::json!({
            "error": {
                "message": refusal
                    .body
                    .get("detail")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("the upstream stream could not be opened"),
                "type": "upstream_error",
                "code": refusal.status.as_u16(),
            }
        })
    };
    Bytes::from(format!("data: {payload}\n\n"))
}

impl Drop for GracefulStream {
    fn drop(&mut self) {
        // A consumer that stops polling before the open is a disconnect, exactly as it is for
        // RelayStream; the row is written here because no stream was ever opened.
        if !matches!(self.stage, Stage::Pending) {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!("stream ledger write skipped: no runtime at disconnect");
            return;
        };
        let context = stream_context(
            &self.state,
            &self.headers,
            &self.identity,
            &self.effective,
            self.decision.as_ref(),
            None,
        );
        let state = self.state.clone();
        let request = self.request.clone();
        let reservation_id = self.gate.reservation_id.clone();
        let started = self.started;
        handle.spawn(async move {
            finalise_stream(
                state,
                context,
                request,
                Vec::new(),
                reservation_id,
                started,
                StreamCapture::default(),
                STREAM_CLIENT_DISCONNECT,
            )
            .await;
        });
    }
}

/// The SSE body: relays each chunk unchanged, stamps the terminal chunk's `served_provider`,
/// captures what was generated, and finalises the write once.
struct RelayStream {
    state: AppState,
    context: CallContext,
    request: ChatCompletionRequest,
    attempts: Option<Vec<ProviderAttempt>>,
    reservation_id: Option<String>,
    first: Option<ChatCompletionChunk>,
    rest: ChunkStream,
    provider: String,
    capture: StreamCapture,
    started: Instant,
    finished: bool,
    sent_done: bool,
}

impl RelayStream {
    fn new(
        state: AppState,
        context: CallContext,
        request: ChatCompletionRequest,
        attempts: Vec<ProviderAttempt>,
        reservation_id: Option<String>,
        served: ServedStream,
        started: Instant,
    ) -> Self {
        Self {
            state,
            context,
            request,
            attempts: Some(attempts),
            reservation_id,
            first: Some(served.first),
            rest: served.rest,
            provider: served.provider,
            capture: StreamCapture::default(),
            started,
            finished: false,
            sent_done: false,
        }
    }

    /// Capture and serialise one relayed chunk. The `served_provider` stamp happens BEFORE both,
    /// because the client's frame and the row must agree on who served the call.
    fn emit(&mut self, mut chunk: ChatCompletionChunk) -> Bytes {
        if chunk.usage.is_some() {
            chunk.served_provider = Some(self.provider.clone());
        }
        self.capture.feed(&chunk);
        Bytes::from(chunk.to_sse())
    }

    /// Schedule the one write for this stream. Idempotent; the reference's `finalised` flag.
    fn finalise(&mut self, status: &'static str) {
        if self.finished {
            return;
        }
        self.finished = true;
        let Some(attempts) = self.attempts.take() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!("stream ledger write skipped: no runtime at finalise");
            return;
        };
        let state = self.state.clone();
        let context = self.context.clone();
        let request = self.request.clone();
        let reservation_id = self.reservation_id.clone();
        let started = self.started;
        let capture = std::mem::take(&mut self.capture);
        handle.spawn(async move {
            finalise_stream(
                state,
                context,
                request,
                attempts,
                reservation_id,
                started,
                capture,
                status,
            )
            .await;
        });
    }
}

impl Stream for RelayStream {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Some(first) = this.first.take() {
            return Poll::Ready(Some(Ok(this.emit(first))));
        }
        if this.sent_done {
            return Poll::Ready(None);
        }
        let polled = this.rest.as_mut().poll_next(cx);
        match polled {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => Poll::Ready(Some(Ok(this.emit(chunk)))),
            Poll::Ready(Some(Err(_))) => {
                // The reference lets the provider error end the iterator; the capture records
                // the hash of what did arrive and the row says `upstream_error`.
                this.capture.mark_broken();
                this.finalise(STREAM_UPSTREAM_ERROR);
                Poll::Ready(None)
            }
            Poll::Ready(None) => {
                // Set before the frame is yielded: reaching this line IS completion.
                this.sent_done = true;
                this.finalise(STREAM_COMPLETED);
                Poll::Ready(Some(Ok(Bytes::from_static(b"data: [DONE]\n\n"))))
            }
        }
    }
}

impl Drop for RelayStream {
    fn drop(&mut self) {
        // A consumer that stops polling is what "aborted" means.
        self.finalise(STREAM_CLIENT_DISCONNECT);
    }
}

/// The one write for a finished stream: the ledger row, then one cost event per attempt.
#[allow(clippy::too_many_arguments)] // the eight values are the stream's whole final state
async fn finalise_stream(
    state: AppState,
    context: CallContext,
    request: ChatCompletionRequest,
    attempts: Vec<ProviderAttempt>,
    reservation_id: Option<String>,
    started: Instant,
    capture: StreamCapture,
    status: &'static str,
) {
    let latency = started.elapsed().as_millis() as i64;
    let response_sha256 = capture.response_sha256();
    let row = LedgerRow::streamed(
        &context,
        &request,
        StreamRow {
            served_provider: capture.served_provider.as_deref(),
            served_model: capture.served_model.as_deref(),
            finish_reason: capture.finish_reason(),
            response_sha256: response_sha256.as_deref(),
            response_json_valid: capture.response_json_valid(),
            status: if status == STREAM_COMPLETED {
                "ok"
            } else {
                "error"
            }
            .to_owned(),
            stream_status: Some(status),
        },
        latency,
    );
    write_ledger(&state, &row).await;
    let served_attempt = attempts
        .iter()
        .rposition(|attempt| attempt.failure.is_none());
    let caller = crate::metrics::caller_label(context.app_id.as_deref());
    for (index, attempt) in attempts.iter().enumerate() {
        let served = Some(index) == served_attempt;
        let outcome = if served {
            stream_outcome(status, &capture)
        } else {
            attempt.outcome
        };
        crate::metrics::record_attempt(
            &attempt.provider,
            attempt.path,
            outcome,
            &caller,
            attempt.latency,
            attempt_reason(outcome, attempt),
        );
        let event = stream_cost_event(
            &context,
            &request,
            attempt,
            index > 0,
            reservation_id.as_deref(),
            if served { capture.usage.as_ref() } else { None },
            if served {
                capture.served_model.as_deref()
            } else {
                None
            },
            if served {
                capture.provider_request_id.as_deref()
            } else {
                None
            },
            state.router().catalog().provider(&attempt.provider),
        );
        emit_cost(&state, &event).await;
    }
}

/// One row for a stream that never opened: `streamed=true`, `stream_status` null, the HTTP code
/// as its status. Failed attempts are still billed.
async fn record_unopened_stream(
    state: &AppState,
    context: CallContext,
    request: &ChatCompletionRequest,
    attempts: &[ProviderAttempt],
    status: StatusCode,
    reservation_id: Option<&str>,
    started: Instant,
) {
    let latency = started.elapsed().as_millis() as i64;
    let row = LedgerRow::streamed(
        &context,
        request,
        StreamRow {
            served_provider: None,
            served_model: None,
            finish_reason: None,
            response_sha256: None,
            response_json_valid: None,
            status: status.as_u16().to_string(),
            stream_status: None,
        },
        latency,
    );
    write_ledger(state, &row).await;
    let caller = crate::metrics::caller_label(context.app_id.as_deref());
    for (index, attempt) in attempts.iter().enumerate() {
        crate::metrics::record_attempt(
            &attempt.provider,
            attempt.path,
            attempt.outcome,
            &caller,
            attempt.latency,
            attempt_reason(attempt.outcome, attempt),
        );
        let event = stream_cost_event(
            &context,
            request,
            attempt,
            index > 0,
            reservation_id,
            None,
            None,
            None,
            state.router().catalog().provider(&attempt.provider),
        );
        emit_cost(state, &event).await;
    }
}

/// The streaming twin of `cost_event`: the metered usage comes from the terminal chunk rather
/// than from a response body.
#[allow(clippy::too_many_arguments)] // the served/total split is clearer than a bag struct here
fn stream_cost_event(
    context: &CallContext,
    request: &ChatCompletionRequest,
    attempt: &ProviderAttempt,
    fallback_used: bool,
    reservation_id: Option<&str>,
    usage: Option<&Usage>,
    served_model: Option<&str>,
    provider_request_id: Option<&str>,
    catalog_entry: Option<&crate::catalog::ProviderEntry>,
) -> CostEvent {
    let input_tokens = usage.map_or(0, |usage| usage.prompt_tokens);
    let output_tokens = usage.map_or(0, |usage| usage.completion_tokens);
    CostEvent {
        tenant_id: context.tenant_id.clone(),
        license_id: context.license_id.clone(),
        correlation_id: context.correlation_id.clone(),
        stage: context.stage.clone(),
        worker_name: context.worker_name.clone(),
        service_key: crate::SERVICE_NAME.to_owned(),
        cost_source: crate::SERVICE_NAME.to_owned(),
        provider: attempt.provider.clone(),
        model: attempt
            .model_override
            .clone()
            .unwrap_or_else(|| request.model.clone()),
        served_model: served_model.map(str::to_owned),
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: usage.map_or(0, |usage| usage.cache_creation_input_tokens),
        cache_read_input_tokens: usage.map_or(0, |usage| usage.cache_read_input_tokens),
        cost_usd: cost_usd(input_tokens, output_tokens, catalog_entry),
        latency_ms: attempt.latency.as_millis() as i64,
        schema_id: Some("default".to_owned()),
        fallback_used,
        provider_request_id: provider_request_id.map(str::to_owned),
        usage_extra: Some(
            usage
                .and_then(|usage| usage.usage_extra.as_ref())
                .map(|extra| serde_json::to_string(extra).unwrap_or_else(|_| "{}".to_owned()))
                .unwrap_or_else(|| "{}".to_owned()),
        ),
        reservation_id: reservation_id.map(str::to_owned),
        project_id: context.project_id.clone(),
        actor_id: context.actor_id.clone(),
        card_id: context.card_id.clone(),
        execution_id: context.parent_execution_id.clone(),
        delegated_by: context.delegated_by.clone(),
        ..CostEvent::default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use serde_json::{Value, json};
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    use crate::catalog::Catalog;
    use crate::routing::RouterService;
    use crate::state::AppState;

    struct Stub {
        body: Value,
        last: Mutex<Option<Value>>,
        delay: std::time::Duration,
        fail_status: Option<u16>,
    }

    async fn handler(
        axum::extract::State(stub): axum::extract::State<Arc<Stub>>,
        axum::Json(body): axum::Json<Value>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;

        *stub.last.lock().await = Some(body.clone());
        if !stub.delay.is_zero() {
            tokio::time::sleep(stub.delay).await;
        }
        if let Some(status) = stub.fail_status {
            return (
                axum::http::StatusCode::from_u16(status).expect("valid stub status"),
                axum::Json(json!({
                    "error": {"message": "stub provider failure", "type": "server_error"}
                })),
            )
                .into_response();
        }
        if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
            // The same five frames the oracle stub sends: four content/finish frames and the
            // usage-bearing terminal frame, then [DONE].
            let frames = [
                r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub-model","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
                r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub-model","choices":[{"index":0,"delta":{"content":"stub "},"finish_reason":null}]}"#,
                r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub-model","choices":[{"index":0,"delta":{"content":"completion"},"finish_reason":null}]}"#,
                r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
                r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub-model","choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}"#,
                "data: [DONE]",
            ];
            let mut text = String::new();
            for frame in frames {
                text.push_str(frame);
                text.push_str("\n\n");
            }
            return (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                text,
            )
                .into_response();
        }
        axum::Json(stub.body.clone()).into_response()
    }

    async fn state_with_stub() -> (AppState, Arc<Stub>) {
        state_with_stub_options(std::time::Duration::ZERO, None, &[]).await
    }

    async fn state_with_stub_options(
        delay: std::time::Duration,
        fail_status: Option<u16>,
        env: &[(&str, &str)],
    ) -> (AppState, Arc<Stub>) {
        let stub = Arc::new(Stub {
            body: json!({
                "id": "chatcmpl-test",
                "model": "local-llm",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "hello back"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
            }),
            last: Mutex::new(None),
            delay,
            fail_status,
        });
        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(stub.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("stub serves") });

        let yaml = format!(
            "providers:\n  - name: pgx-local\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{address}/v1\n    model: local-llm\n    model_aliases:\n      local-llm: local-llm\ndefault_chain: [pgx-local]\n"
        );
        let catalog = Catalog::from_yaml(&yaml).expect("catalog");
        let map: std::collections::HashMap<String, String> = env
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        let config =
            crate::config::Config::from_lookup(|key| map.get(key).cloned()).expect("config");
        let router =
            RouterService::new(catalog, std::time::Duration::from_secs(1)).expect("router");
        (AppState::new(config, router), stub)
    }

    fn body(stream: bool) -> Body {
        Body::from(
            json!({
                "model": "local-llm",
                "messages": [{"role": "user", "content": "hi"}],
                "max_tokens": 8,
                "stream": stream
            })
            .to_string(),
        )
    }

    #[tokio::test]
    async fn a_completion_is_served_end_to_end() {
        let (state, stub) = state_with_stub().await;
        let response = crate::router::router(state)
            .oneshot(
                Request::post("/api/chat/completions")
                    .header("content-type", "application/json")
                    .body(body(false))
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let body: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(body["model"], "local-llm");
        assert_eq!(body["served_provider"], "pgx-local");
        assert_eq!(body["choices"][0]["message"]["content"], "hello back");

        let sent = stub.last.lock().await.clone().expect("upstream saw it");
        assert_eq!(sent["model"], "local-llm");
    }

    #[tokio::test]
    async fn a_stream_is_relayed_chunk_for_chunk_and_terminated() {
        let (state, _stub) = state_with_stub().await;
        let response = crate::router::router(state)
            .oneshot(
                Request::post("/api/chat/completions")
                    .header("content-type", "application/json")
                    .body(body(true))
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream; charset=utf-8")
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let text = String::from_utf8(bytes.to_vec()).expect("utf8");

        // Five data frames plus the terminator, in order.
        assert_eq!(text.matches("data: ").count(), 6);
        assert!(text.ends_with("data: [DONE]\n\n"));
        assert!(text.contains("\"content\":\"stub \""));
        // The router stamps the terminal usage chunk, so the client's frame and the row agree.
        assert!(text.contains("\"served_provider\":\"pgx-local\""));
        assert!(text.contains("\"served_model\":\"stub-model\""));
        assert!(text.contains("\"provider_request_id\":\"chatcmpl-stub\""));
    }

    fn stream_request() -> Request<Body> {
        Request::post("/api/chat/completions")
            .header("content-type", "application/json")
            .body(body(true))
            .expect("request")
    }

    async fn body_text(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    /// The head must go out inside the caller's first-byte budget, not after a long prefill.
    #[tokio::test]
    async fn a_slow_open_flushes_headers_and_keepalives() {
        let (state, _stub) = state_with_stub_options(
            std::time::Duration::from_millis(400),
            None,
            &[
                ("INFERENCE_ROUTER_STREAM_HEADERS_GRACE_MS", "50"),
                ("INFERENCE_ROUTER_STREAM_KEEPALIVE_MS", "20"),
                ("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "0.05"),
            ],
        )
        .await;
        let started = std::time::Instant::now();
        let response = crate::router::router(state)
            .oneshot(stream_request())
            .await
            .expect("router answers");
        let head = started.elapsed();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert!(
            head < std::time::Duration::from_millis(300),
            "the head waited for the first chunk: {head:?}"
        );
        let text = body_text(response).await;
        assert!(text.starts_with(": keepalive\n\n"), "{text}");
        assert_eq!(text.matches("data: ").count(), 6);
        assert!(text.ends_with("data: [DONE]\n\n"));
        assert!(text.contains("\"content\":\"stub \""));
    }

    #[tokio::test]
    async fn keepalives_are_emitted_while_the_open_is_pending() {
        let (state, _stub) = state_with_stub_options(
            std::time::Duration::from_millis(300),
            None,
            &[
                ("INFERENCE_ROUTER_STREAM_HEADERS_GRACE_MS", "20"),
                ("INFERENCE_ROUTER_STREAM_KEEPALIVE_MS", "20"),
                ("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "0.02"),
            ],
        )
        .await;
        let response = crate::router::router(state)
            .oneshot(stream_request())
            .await
            .expect("router answers");
        let text = body_text(response).await;
        assert!(
            text.matches(": keepalive").count() >= 2,
            "one keepalive is not a cadence: {text}"
        );
    }

    #[tokio::test]
    async fn a_refusal_after_the_grace_ends_with_an_error_frame() {
        let (state, _stub) = state_with_stub_options(
            std::time::Duration::from_millis(300),
            Some(500),
            &[
                ("INFERENCE_ROUTER_STREAM_HEADERS_GRACE_MS", "50"),
                ("INFERENCE_ROUTER_STREAM_KEEPALIVE_MS", "20"),
                ("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "0.05"),
            ],
        )
        .await;
        let response = crate::router::router(state)
            .oneshot(stream_request())
            .await
            .expect("router answers");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let text = body_text(response).await;
        assert!(text.starts_with(": keepalive\n\n"), "{text}");
        assert!(text.contains("data: {\"error\""), "{text}");
        assert!(text.ends_with("data: [DONE]\n\n"), "{text}");
    }

    /// A failure inside the grace is still an HTTP refusal: the early-flush path must not have
    /// started, so shed/connect statuses keep their meaning for retrying callers.
    #[tokio::test]
    async fn a_refusal_inside_the_grace_keeps_its_status() {
        let (baseline, _) = state_with_stub_options(
            std::time::Duration::from_millis(30),
            Some(500),
            &[
                ("INFERENCE_ROUTER_STREAM_HEADERS_GRACE_MS", "0"),
                ("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "0.05"),
            ],
        )
        .await;
        let baseline_status = crate::router::router(baseline)
            .oneshot(stream_request())
            .await
            .expect("router answers")
            .status();
        let (early, _) = state_with_stub_options(
            std::time::Duration::from_millis(30),
            Some(500),
            &[
                ("INFERENCE_ROUTER_STREAM_HEADERS_GRACE_MS", "5000"),
                ("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "0.05"),
            ],
        )
        .await;
        let status = crate::router::router(early)
            .oneshot(stream_request())
            .await
            .expect("router answers")
            .status();
        assert_eq!(status, baseline_status);
        assert!(!status.is_success());
    }

    #[test]
    fn the_unlicensed_trio_is_classified_on_the_licence_alone() {
        let mut event = crate::metering::CostEvent {
            service_key: "inference-router".to_owned(),
            provider: "trio-provider".to_owned(),
            // Negative input is clamped: the trio books max(0,input) + max(0,output).
            input_tokens: -4,
            output_tokens: 3,
            cost_usd: "0.25000000".to_owned(),
            app_id: Some("w2v".to_owned()),
            worker_name: Some("core-app".to_owned()),
            ..crate::metering::CostEvent::default()
        };
        assert_eq!(
            super::unlicensed_trio(&event),
            Some(("w2v".to_owned(), "core-app".to_owned(), 0.25, 3)),
            "no licence at all: the trio moves, labelled through the bounded normalisers"
        );

        event.license_id = Some("unknown".to_owned());
        assert!(
            super::unlicensed_trio(&event).is_some(),
            "the licence server's unresolved literal is unlicensed"
        );

        event.license_id = Some("lic-0123456789abcdef".to_owned());
        assert_eq!(
            super::unlicensed_trio(&event),
            None,
            "a licence present means no unlicensed cost"
        );

        event.license_id = None;
        event.cost_usd = "not-a-decimal".to_owned();
        assert_eq!(
            super::unlicensed_trio(&event).map(|trio| trio.2),
            Some(0.0),
            "a malformed decimal books zero dollars rather than failing"
        );
    }

    #[tokio::test]
    async fn an_invalid_body_is_the_reference_422_document() {
        let (state, _stub) = state_with_stub().await;
        let response = crate::router::router(state)
            .oneshot(
                Request::post("/api/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"model":"stub-model"}"#))
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let body: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(body["detail"][0]["type"], "missing");
        assert_eq!(body["detail"][0]["loc"], json!(["body", "messages"]));
        assert_eq!(body["detail"][0]["msg"], "Field required");
        assert_eq!(body["detail"][0]["input"], json!({"model": "stub-model"}));
    }

    #[test]
    fn the_actor_is_the_sent_header_then_the_service_subject() {
        use axum::http::{HeaderMap, HeaderValue};

        use super::actor_id;

        let service = "98a72edd-06ee-408f-9f08-f88ce35897a5";
        let human = "01a1681e-81f2-7487-af71-b888fed32b37";
        let mut headers = HeaderMap::new();
        // No header: a call no human made belongs to the calling service.
        assert_eq!(
            actor_id(&headers, Some(service)),
            Some(service.to_owned()),
            "the service subject is the fallback"
        );
        // A valid header wins over the service subject.
        headers.insert("x-actor-id", HeaderValue::from_static(human));
        assert_eq!(
            actor_id(&headers, Some(service)),
            Some(human.to_owned()),
            "the caller's own actor wins"
        );
        // A malformed or blank header never fails a request and never wins.
        for sent in ["not-a-uuid", "   "] {
            headers.insert("x-actor-id", HeaderValue::from_str(sent).expect("header"));
            assert_eq!(
                actor_id(&headers, Some(service)),
                Some(service.to_owned()),
                "a bad header falls back to the service subject"
            );
        }
        // Neither present, or a malformed fallback: absent, never invented.
        assert_eq!(actor_id(&HeaderMap::new(), None), None);
        assert_eq!(actor_id(&HeaderMap::new(), Some("not-a-uuid")), None);
    }
}
