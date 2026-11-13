//! `POST /api/chat/completions` — the one route every consumer's inference goes through.
//!
//! Non-streaming dispatch only in this slice. The request carries a long list of attribution
//! headers; every one is optional and a malformed value is simply absent — a bad header must
//! never fail a customer's inference.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::auth::CallerIdentity;
use crate::contract::schemas::{
    ChatCompletionRequest, ChatCompletionResponse, HTTPValidationError,
};
use crate::ledger::{self, CallContext, LedgerRow};
use crate::metering::{CostEvent, ZERO_COST_USD};
use crate::routing::{CompletionOutcome, ProviderAttempt, Refusal};
use crate::state::AppState;

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
    let outcome = state.router().complete(&request, x_provider).await;
    // The write path runs on EVERY outcome, not only on success: a request that failed over spent
    // money on the attempts that did not serve it, and the cost bus is billed per attempt.
    record(&state, &identity, &headers, &request, &outcome).await;
    match outcome.response {
        Ok(response) => Json(response).into_response(),
        Err(refusal) => refusal.into_response(),
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

/// One ledger row for the served attempt, and one cost event per attempt.
///
/// Both failures are logged and dropped: the reference never turns a write-path problem into a
/// failed inference, and neither does this port.
async fn record(
    state: &AppState,
    identity: &CallerIdentity,
    headers: &HeaderMap,
    request: &ChatCompletionRequest,
    outcome: &CompletionOutcome,
) {
    let context = CallContext {
        tenant_id: identity.tenant_id.clone(),
        license_id: identity.license_id.clone(),
        correlation_id: attribution(headers, "x-correlation-id"),
        worker_name: attribution(headers, "x-worker-name"),
        stage: attribution(headers, "x-stage"),
        trace_id: attribution(headers, "x-aa-trace-id"),
        card_id: attribution(headers, "x-aa-card-id"),
        project_id: attribution(headers, "x-project-id"),
        parent_execution_id: attribution(headers, "x-aa-parent-exec"),
        ..CallContext::default()
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
        let event = cost_event(&context, request, served, attempt, index > 0);
        state.deps().cost.emit(&event).await;
    }
}

fn cost_event(
    context: &CallContext,
    request: &ChatCompletionRequest,
    served: Option<&ChatCompletionResponse>,
    attempt: &ProviderAttempt,
    fallback_used: bool,
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
        project_id: context.project_id.clone(),
        card_id: context.card_id.clone(),
        execution_id: context.parent_execution_id.clone(),
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
