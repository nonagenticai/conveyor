//! `POST /api/chat/completions` — the one route every consumer's inference goes through.
//!
//! Non-streaming dispatch only in this slice. The request carries a long list of attribution
//! headers; most are ledger-only and are wired with the ledger slice (a malformed attribution
//! header must never fail a customer's inference).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::contract::schemas::{
    ChatCompletionRequest, ChatCompletionResponse, HTTPValidationError,
};
use crate::routing::Refusal;
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
    if let Err(refusal) = state.authorize(&headers).await {
        return refusal.into_response();
    }
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
    match state.router().complete(&request, x_provider).await {
        Ok(response) => Json(response).into_response(),
        Err(refusal) => refusal.into_response(),
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
