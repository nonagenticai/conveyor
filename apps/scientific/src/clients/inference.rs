//! The inference-router client: one chat completion, and the headers the old service sent.
//!
//! `X-Schema-Id` is not part of the router's shared contract — it selects the routing rule (provider
//! chain, JSON schema, system prompt) by name — so it rides only when a caller names one. The
//! tenant header is the caller's data scope, and it is omitted when the request carried none: an
//! invented tenant id would be attributed to a tenant that never called.

use std::time::Duration;

use serde_json::{Value, json};

/// A model answer is slower than a database read and faster than a proof; this is the bound that
/// keeps a hung router from holding a request open.
const CHAT_TIMEOUT: Duration = Duration::from_secs(90);
/// Attempts one chat call makes: the first try plus two retries.
const CHAT_ATTEMPTS: u32 = 3;
/// The longest a server's `Retry-After` may make a retry wait.
const RETRY_AFTER_CAP: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error(
        "inference-router is not configured: SCIENTIFIC_PROCESSOR_INFERENCE_ROUTER_URL is empty"
    )]
    Unconfigured,
    #[error("inference-router call failed: {0}")]
    Upstream(String),
    #[error("inference-router returned no message content: {0}")]
    Empty(String),
}

/// A configured model router.
#[derive(Debug, Clone)]
pub struct InferenceClient {
    base: String,
    http: reqwest::Client,
}

impl InferenceClient {
    /// `None` when the endpoint is empty.
    pub fn new(base: Option<&str>, http: reqwest::Client) -> Option<Self> {
        let base = base?.trim_end_matches('/').to_owned();
        if base.is_empty() {
            return None;
        }
        Some(Self { base, http })
    }

    /// One chat completion, returning the first choice's text content.
    ///
    /// `tenant` is only ever the caller's own; `schema_id` names the routing rule when a route has
    /// one, because the old service pinned one per prompt and the router's behaviour depends on it.
    pub async fn chat(
        &self,
        prompt: &str,
        max_tokens: u32,
        schema_id: &str,
        tenant: Option<&str>,
    ) -> Result<String, InferenceError> {
        let body = json!({
            "model": "default",
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.0,
            "max_tokens": max_tokens,
        });
        // The router sheds a full provider as 503 and reports an upstream 429/5xx as 502/503; a
        // single blip must not fail a proof. A READ TIMEOUT is never retried: the call may already
        // have been served (and billed). Every attempt carries the same body and headers.
        let mut attempt = 1;
        let response = loop {
            let mut request = self
                .http
                .post(format!("{}/api/chat/completions", self.base))
                .timeout(CHAT_TIMEOUT)
                .header("X-Schema-Id", schema_id)
                .json(&body);
            if let Some(tenant) = tenant.filter(|value| !value.is_empty()) {
                request = request.header("X-Tenant-Id", tenant);
            }
            match request.send().await {
                Ok(response) if attempt >= CHAT_ATTEMPTS || !transient(response.status()) => {
                    break response;
                }
                Ok(response) => {
                    let wait = retry_after(&response).unwrap_or_else(|| backoff(attempt));
                    tracing::warn!(
                        status = response.status().as_u16(),
                        attempt,
                        "scientific_inference_retry"
                    );
                    tokio::time::sleep(wait).await;
                }
                Err(error)
                    if attempt < CHAT_ATTEMPTS && error.is_connect() && !error.is_timeout() =>
                {
                    tracing::warn!(attempt, "scientific_inference_connect_retry");
                    tokio::time::sleep(backoff(attempt)).await;
                }
                Err(error) => return Err(InferenceError::Upstream(error.to_string())),
            }
            attempt += 1;
        };
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(InferenceError::Upstream(format!(
                "{status}: {}",
                body.trim()
            )));
        }
        let document: Value = response
            .json()
            .await
            .map_err(|error| InferenceError::Upstream(error.to_string()))?;
        first_content(&document).ok_or_else(|| InferenceError::Empty(document.to_string()))
    }
}

/// The old service's own read of a completion: `choices[0].message.content`, and an empty string is
/// a failure rather than an answer.
pub fn first_content(document: &Value) -> Option<String> {
    let content = document
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()?;
    if content.trim().is_empty() {
        return None;
    }
    Some(content.to_owned())
}

/// Whether `status` is one a retry may turn into a success.
fn transient(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504)
}

/// The backoff before retry `attempt` (250 ms, then 500 ms).
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(250_u64 << (attempt - 1))
}

/// The server's `Retry-After` in whole seconds or an HTTP-date, capped at [`RETRY_AFTER_CAP`].
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let value = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds).min(RETRY_AFTER_CAP));
    }
    let when = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let wait = when.with_timezone(&chrono::Utc) - chrono::Utc::now();
    Some(Duration::from_secs(wait.num_seconds().max(0) as u64).min(RETRY_AFTER_CAP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_choices_content_is_the_answer() {
        let document = json!({"choices": [{"message": {"content": " hello "}}]});
        assert_eq!(first_content(&document).as_deref(), Some(" hello "));
    }

    #[test]
    fn an_empty_content_is_not_an_answer() {
        let document = json!({"choices": [{"message": {"content": "  "}}]});
        assert!(first_content(&document).is_none());
        assert!(first_content(&json!({"choices": []})).is_none());
        assert!(first_content(&json!({})).is_none());
    }

    #[test]
    fn an_empty_endpoint_is_not_a_client() {
        let http = reqwest::Client::new();
        assert!(InferenceClient::new(Some(""), http.clone()).is_none());
        assert!(InferenceClient::new(None, http).is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_transient_failure_is_retried_and_a_refusal_is_not() {
        use std::collections::VecDeque;
        use std::sync::Arc;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::extract::State;
        use axum::routing::post;
        use axum::{Json, Router};

        #[derive(Clone)]
        struct StubState {
            statuses: Arc<Mutex<VecDeque<u16>>>,
            hits: Arc<AtomicUsize>,
        }

        async fn answer(State(state): State<StubState>) -> (axum::http::StatusCode, Json<Value>) {
            state.hits.fetch_add(1, Ordering::SeqCst);
            let status = state
                .statuses
                .lock()
                .expect("statuses")
                .pop_front()
                .unwrap_or(200);
            let body = if status == 200 {
                json!({"choices": [{"message": {"content": "the answer"}}]})
            } else {
                json!({"error": {"message": "stub"}})
            };
            (
                axum::http::StatusCode::from_u16(status).expect("status"),
                Json(body),
            )
        }

        async fn start(statuses: Vec<u16>) -> (String, Arc<AtomicUsize>) {
            let hits = Arc::new(AtomicUsize::new(0));
            let state = StubState {
                statuses: Arc::new(Mutex::new(statuses.into())),
                hits: hits.clone(),
            };
            let app = Router::new()
                .route("/api/chat/completions", post(answer))
                .with_state(state);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind the stub");
            let addr = listener.local_addr().expect("stub addr");
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            (format!("http://{addr}"), hits)
        }

        // A shed 502 then a 200: the retry returns the answer.
        let (base, hits) = start(vec![502]).await;
        let client = InferenceClient::new(Some(&base), reqwest::Client::new()).expect("client");
        let answer = client
            .chat("hi", 16, "schema", None)
            .await
            .expect("the retry succeeds");
        assert_eq!(answer, "the answer");
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "one failed attempt, one served"
        );

        // A 400 is the request's own fault: sent once, never retried.
        let (base, hits) = start(vec![400]).await;
        let client = InferenceClient::new(Some(&base), reqwest::Client::new()).expect("client");
        assert!(client.chat("hi", 16, "schema", None).await.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
