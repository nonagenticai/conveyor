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
        let mut request = self
            .http
            .post(format!("{}/api/chat/completions", self.base))
            .timeout(CHAT_TIMEOUT)
            .header("X-Schema-Id", schema_id)
            .json(&body);
        if let Some(tenant) = tenant.filter(|value| !value.is_empty()) {
            request = request.header("X-Tenant-Id", tenant);
        }

        let response = request
            .send()
            .await
            .map_err(|error| InferenceError::Upstream(error.to_string()))?;
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
}
