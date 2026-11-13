//! `POST /api/chat/completions` — the one route every consumer's inference goes through.
//!
//! Non-streaming dispatch only in this slice. The request carries a long list of attribution
//! headers; every one is optional and a malformed value is simply absent — a bad header must
//! never fail a customer's inference.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::auth::{CallerIdentity, EffectiveTenant, effective_tenant};
use crate::config::{GateMode, PolicyMode};
use crate::contract::schemas::{
    ChatCompletionRequest, ChatCompletionResponse, HTTPValidationError,
};
use crate::ledger::{self, CallContext, LedgerRow};
use crate::metering::{CostEvent, ZERO_COST_USD};
use crate::policy::{self, ResolvedHop};
use crate::routing::{CompletionOutcome, ProviderAttempt, Refusal};
use crate::state::AppState;

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
    Json(request): Json<ChatCompletionRequest>,
) -> Response {
    // The licence gate runs before anything is dialled: an unverifiable caller must never reach a
    // provider or the ledger.
    let identity = match state.authorize(&headers).await {
        Ok(identity) => identity,
        Err(refusal) => return refusal.into_response(),
    };
    // Whose request this is: the caller's own tenant, or one a vouched allow-listed subject
    // named. Resolved once, before any policy, budget or ledger decision, so all three agree.
    let effective = effective_tenant(
        headers
            .get("x-effective-tenant-id")
            .and_then(|value| value.to_str().ok()),
        identity.subject_id.as_deref(),
        identity.tenant_id.as_deref(),
        &state.config().auth_delegating_subjects,
    );
    if request.stream {
        return Refusal::detail(
            StatusCode::NOT_IMPLEMENTED,
            "Streaming is not implemented yet",
            "stream_unsupported",
        )
        .into_response();
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
    if proven_platform_tenant(state, identity, tenant) {
        return Ok(None);
    }
    let Some(active) = state.deps().policy.get_active(tenant).await else {
        return Ok(None);
    };
    if active.requires_proven_tenant() && !proven_tenant(identity, tenant) {
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

/// True when a verified licence on THIS request names exactly `tenant`.
///
/// Delegation deliberately does not help here: a licence for the delegating service names THAT
/// service's tenant, so a policy for the delegated tenant still requires a token for it.
fn proven_tenant(identity: &CallerIdentity, tenant: &str) -> bool {
    identity.verified
        && identity
            .tenant_id
            .as_deref()
            .is_some_and(|own| own.trim().eq_ignore_ascii_case(tenant.trim()))
}

/// The commercial-platform rule: the EFFECTIVE tenant is listed **and** proven to be itself on
/// this request. An asserted header is never proof.
fn proven_platform_tenant(state: &AppState, identity: &CallerIdentity, tenant: &str) -> bool {
    state
        .config()
        .commercial_platform_tenant_ids
        .iter()
        .any(|id| id.eq_ignore_ascii_case(tenant))
        && proven_tenant(identity, tenant)
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
    let context = CallContext {
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
    };

    let served = outcome.response.as_ref().ok();
    let served_attempt = outcome.attempts.iter().rev().find(|a| a.failure.is_none());

    if let (Some(pool), Some(response), Some(attempt)) =
        (state.deps().ledger.as_ref(), served, served_attempt)
    {
        let row = LedgerRow::served(
            &context,
            request,
            response,
            attempt
                .bodies
                .as_ref()
                .map(|bodies| bodies.request_text.as_str()),
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
    ) -> axum::Json<Value> {
        *stub.last.lock().await = Some(body);
        axum::Json(stub.body.clone())
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
    async fn streaming_is_refused_until_it_is_implemented() {
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
        assert_eq!(response.status(), axum::http::StatusCode::NOT_IMPLEMENTED);
    }
}
