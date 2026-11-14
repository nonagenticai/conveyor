//! `POST /api/chat/completions` — the one route every consumer's inference goes through.
//!
//! Non-streaming dispatch only in this slice. The request carries a long list of attribution
//! headers; every one is optional and a malformed value is simply absent — a bad header must
//! never fail a customer's inference.

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use std::time::Instant;

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
use crate::metering::{CostEvent, ZERO_COST_USD};
use crate::policy::{self, ResolvedHop};
use crate::provider::ChunkStream;
use crate::routing::{CompletionOutcome, ProviderAttempt, Refusal, ServedHop, ServedStream};
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
        stage: attribution(headers, "x-stage"),
        trace_id: attribution(headers, "x-aa-trace-id"),
        card_id: attribution(headers, "x-aa-card-id"),
        project_id: attribution(headers, "x-project-id"),
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

/// One ledger row for the served attempt, and one cost event per attempt.
///
/// Both failures are logged and dropped: the reference never turns a write-path problem into a
/// failed inference, and neither does this port.
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
    let served_attempt = outcome.attempts.iter().rev().find(|a| a.failure.is_none());

    if let (Some(pool), Some(response), Some(attempt)) =
        (state.deps().ledger.as_ref(), served, served_attempt)
    {
        let row = LedgerRow::served(
            &context,
            request,
            response,
            attempt.bodies.as_ref(),
            attempt.latency.as_millis() as i64,
        );
        if let Err(error) = ledger::insert(pool, &row).await {
            tracing::warn!(%error, "inference ledger write failed");
        }
    }

    for (index, attempt) in outcome.attempts.iter().enumerate() {
        let event = cost_event(
            &context,
            request,
            served,
            attempt,
            index > 0,
            reservation_id,
        );
        state.deps().cost.emit(&event).await;
    }
}

fn cost_event(
    context: &CallContext,
    request: &ChatCompletionRequest,
    served: Option<&ChatCompletionResponse>,
    attempt: &ProviderAttempt,
    fallback_used: bool,
    reservation_id: Option<&str>,
) -> CostEvent {
    let usage = served.map(|response| &response.usage);
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
        input_tokens: usage.map_or(0, |usage| usage.prompt_tokens),
        output_tokens: usage.map_or(0, |usage| usage.completion_tokens),
        cache_creation_input_tokens: usage.map_or(0, |usage| usage.cache_creation_input_tokens),
        cache_read_input_tokens: usage.map_or(0, |usage| usage.cache_read_input_tokens),
        cost_usd: ZERO_COST_USD.to_owned(),
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
        .and_then(|value| value.to_str().ok());
    let decision = match tenant_policy(&state, &identity, &effective, &request, x_provider).await {
        Ok(decision) => decision,
        Err(refusal) => return refusal.into_response(),
    };
    let hops = decision.as_ref().map(|decision| decision.hops.as_slice());
    let tenant = effective.tenant_id.as_deref().unwrap_or_default();
    let gate = state
        .deps()
        .budget
        .reserve(tenant, estimate_tokens(&request))
        .await;
    if state.config().budget_gate_mode == GateMode::Enforce && gate.reason.deniable() {
        return budget_refusal(&gate).into_response();
    }
    let outcome = state.router().stream(&request, x_provider, hops).await;
    let context = attributed(
        CallContext {
            tenant_id: effective.tenant_id.clone(),
            license_id: identity.license_id.clone(),
            delegated_by: effective.delegated_by.clone(),
            ..CallContext::default()
        },
        &headers,
        decision.as_ref(),
        outcome.served_hop.as_ref(),
    );
    match outcome.stream {
        Ok(served) => {
            let relay = RelayStream::new(
                state,
                context,
                request,
                outcome.attempts,
                gate.reservation_id.clone(),
                served,
                started,
            );
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream; charset=utf-8")
                .header("cache-control", "no-cache")
                .header("connection", "keep-alive")
                .header("x-accel-buffering", "no")
                .body(Body::from_stream(relay))
                .expect("a static response head is valid")
        }
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
    if let Some(pool) = state.deps().ledger.as_ref() {
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
        if let Err(error) = ledger::insert(pool, &row).await {
            tracing::warn!(%error, "inference ledger write failed");
        }
    }
    let served_attempt = attempts
        .iter()
        .rposition(|attempt| attempt.failure.is_none());
    for (index, attempt) in attempts.iter().enumerate() {
        let served = Some(index) == served_attempt;
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
        );
        state.deps().cost.emit(&event).await;
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
    if let Some(pool) = state.deps().ledger.as_ref() {
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
        if let Err(error) = ledger::insert(pool, &row).await {
            tracing::warn!(%error, "inference ledger write failed");
        }
    }
    for (index, attempt) in attempts.iter().enumerate() {
        let event = stream_cost_event(
            &context,
            request,
            attempt,
            index > 0,
            reservation_id,
            None,
            None,
            None,
        );
        state.deps().cost.emit(&event).await;
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
) -> CostEvent {
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
        input_tokens: usage.map_or(0, |usage| usage.prompt_tokens),
        output_tokens: usage.map_or(0, |usage| usage.completion_tokens),
        cache_creation_input_tokens: usage.map_or(0, |usage| usage.cache_creation_input_tokens),
        cache_read_input_tokens: usage.map_or(0, |usage| usage.cache_read_input_tokens),
        cost_usd: ZERO_COST_USD.to_owned(),
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
    }

    async fn handler(
        axum::extract::State(stub): axum::extract::State<Arc<Stub>>,
        axum::Json(body): axum::Json<Value>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;

        *stub.last.lock().await = Some(body.clone());
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
        let stub = Arc::new(Stub {
            body: json!({
                "id": "chatcmpl-test",
                "model": "local-llm",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "hello back"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
            }),
            last: Mutex::new(None),
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
        let config = crate::config::Config::from_lookup(|_| None).expect("defaults");
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
}
