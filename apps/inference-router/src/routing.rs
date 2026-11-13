//! Provider selection and the completion path.
//!
//! Two dispatch shapes, matching the live service:
//!
//! - **pinned** — an `X-Provider` header names the provider, or `request.model` is an alias exactly
//!   one enabled provider declares (auto-pin). Pinned dispatch has no failover: a provider failure
//!   is returned to the caller rather than answered by a different model;
//! - **chain** — every other request walks `default_chain` in order, skipping providers that are
//!   not configured, and fails with the last error when the chain is exhausted.
//!
//! A catalog alias that NO enabled provider declares is refused (503 `steering_alias_unavailable`)
//! instead of silently riding the chain: answering `tier-high` with local Gemma, HTTP 200, is the
//! failure this refusal exists to prevent.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;

use crate::catalog::Catalog;
use crate::contract::schemas::{ChatCompletionRequest, ChatCompletionResponse};
use crate::provider::{self, Provider, UpstreamError};

/// The `X-IR-Error-Classification` header, exactly as the live service spells it.
pub const CLASSIFICATION_HEADER: &str = "X-IR-Error-Classification";

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Provider(#[from] provider::BuildError),
}

/// A refusal the caller sees: either FastAPI's `{"detail": …}` or an OpenAI `{"error": …}`.
#[derive(Debug)]
pub struct Refusal {
    pub status: StatusCode,
    pub body: serde_json::Value,
    pub headers: Vec<(&'static str, String)>,
}

impl Refusal {
    pub fn detail(
        status: StatusCode,
        message: impl Into<String>,
        classification: &'static str,
    ) -> Self {
        Self {
            status,
            body: serde_json::json!({"detail": message.into()}),
            headers: vec![(CLASSIFICATION_HEADER, classification.to_owned())],
        }
    }

    pub fn openai(
        status: StatusCode,
        message: impl Into<String>,
        error_type: &'static str,
    ) -> Self {
        Self {
            status,
            body: serde_json::json!({"error": {"message": message.into(), "type": error_type}}),
            headers: Vec::new(),
        }
    }
}

impl axum::response::IntoResponse for Refusal {
    fn into_response(self) -> axum::response::Response {
        let mut response = (self.status, axum::Json(self.body)).into_response();
        for (name, value) in self.headers {
            if let Ok(value) = axum::http::HeaderValue::from_str(&value) {
                response.headers_mut().insert(name, value);
            }
        }
        response
    }
}

/// The providers, built once at boot.
pub struct RouterService {
    catalog: Catalog,
    providers: BTreeMap<String, Arc<dyn Provider>>,
    slot_wait: Duration,
}

enum Route {
    Pinned {
        provider: String,
        model_override: Option<String>,
    },
    Chain(Vec<String>),
    Refused(Refusal),
}

impl RouterService {
    pub fn new(catalog: Catalog, slot_wait: Duration) -> Result<Self, BuildError> {
        let mut providers: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
        for entry in catalog.enabled() {
            providers.insert(
                entry.name.clone(),
                Arc::from(provider::build(entry, slot_wait)?),
            );
        }
        Ok(Self {
            catalog,
            providers,
            slot_wait,
        })
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn slot_wait(&self) -> Duration {
        self.slot_wait
    }

    /// Serve one non-streaming completion.
    pub async fn complete(
        &self,
        request: &ChatCompletionRequest,
        x_provider: Option<&str>,
    ) -> Result<ChatCompletionResponse, Refusal> {
        match self.route(request, x_provider) {
            Route::Pinned {
                provider,
                model_override,
            } => {
                self.complete_pinned(&provider, model_override.as_deref(), request)
                    .await
            }
            Route::Chain(chain) => self.complete_chain(&chain, request).await,
            Route::Refused(refusal) => Err(refusal),
        }
    }

    fn route(&self, request: &ChatCompletionRequest, x_provider: Option<&str>) -> Route {
        if let Some(name) = x_provider.map(str::trim).filter(|name| !name.is_empty()) {
            let Some(entry) = self.catalog.provider(name) else {
                return Route::Refused(Refusal::detail(
                    StatusCode::BAD_REQUEST,
                    format!("Unknown provider: {name}"),
                    "unknown_provider",
                ));
            };
            // A pinned dispatch forwards the caller's model verbatim unless it is one of this
            // provider's registered aliases.
            let model_override = entry
                .model_aliases
                .get(&request.model)
                .cloned()
                .or_else(|| Some(request.model.clone()).filter(|model| !model.is_empty()));
            return Route::Pinned {
                provider: name.to_owned(),
                model_override,
            };
        }

        if let Some(owner) = self.catalog.alias_owner(&request.model) {
            let entry = self
                .catalog
                .provider(owner)
                .expect("an alias owner is an enabled provider");
            return Route::Pinned {
                provider: owner.to_owned(),
                model_override: entry.model_aliases.get(&request.model).cloned(),
            };
        }

        // A catalog alias nothing enabled can serve must not fall through to a different model.
        if self.catalog.any_provider_declares(&request.model) {
            return Route::Refused(Refusal::detail(
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "Model '{}' is a catalog alias no enabled provider can serve",
                    request.model
                ),
                "steering_alias_unavailable",
            ));
        }

        let chain: Vec<String> = self
            .catalog
            .default_chain
            .iter()
            .filter(|name| self.providers.contains_key(*name))
            .cloned()
            .collect();
        if chain.is_empty() {
            return Route::Refused(Refusal::detail(
                StatusCode::SERVICE_UNAVAILABLE,
                "No providers available for this request",
                "chain_empty",
            ));
        }
        Route::Chain(chain)
    }

    async fn complete_pinned(
        &self,
        name: &str,
        model_override: Option<&str>,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, Refusal> {
        let Some(provider) = self.providers.get(name) else {
            // `route` only pins an enabled provider; this is unreachable, but the refusal is the
            // same one the caller would have received for an unknown name.
            return Err(Refusal::detail(
                StatusCode::BAD_REQUEST,
                format!("Unknown provider: {name}"),
                "unknown_provider",
            ));
        };
        match provider.complete(request, model_override).await {
            Ok(upstream) => {
                let mut response = upstream.response;
                response.served_provider = Some(name.to_owned());
                Ok(response)
            }
            Err(error) => {
                if let UpstreamError::BadRequest { message, .. } = &error {
                    // A 400 is the REQUEST's fault: pass the upstream's own reason through.
                    return Err(Refusal::openai(
                        StatusCode::BAD_REQUEST,
                        message.clone(),
                        "invalid_request_error",
                    ));
                }
                let status = if matches!(error, UpstreamError::QueueRejected { .. }) {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_GATEWAY
                };
                Err(Refusal::detail(
                    status,
                    format!("Pinned provider '{name}' failed: {}", error.describe()),
                    error.classification(),
                ))
            }
        }
    }

    async fn complete_chain(
        &self,
        chain: &[String],
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, Refusal> {
        let mut last: Option<UpstreamError> = None;
        for name in chain {
            let Some(provider) = self.providers.get(name) else {
                continue;
            };
            let model_override = self
                .catalog
                .provider(name)
                .and_then(|entry| entry.model_aliases.get(&request.model))
                .map(String::as_str);
            match provider.complete(request, model_override).await {
                Ok(upstream) => {
                    let mut response = upstream.response;
                    response.served_provider = Some(name.clone());
                    return Ok(response);
                }
                Err(error) => {
                    last = Some(error);
                }
            }
        }
        match last {
            Some(error) => Err(Refusal::detail(
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "All providers failed. Last error: {}: {}",
                    error.provider(),
                    error.describe()
                ),
                error.classification(),
            )),
            None => Err(Refusal::detail(
                StatusCode::SERVICE_UNAVAILABLE,
                "No providers available for this request",
                "chain_empty",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::catalog::Catalog;
    use crate::contract::schemas::{ChatCompletionResponse, ChatMessage, Role, Usage};

    /// A provider that answers with a fixed response or a fixed error.
    struct Fake {
        name: String,
        answer: Result<(), (&'static str, &'static str)>,
    }

    impl Provider for Fake {
        fn name(&self) -> &str {
            &self.name
        }

        fn complete<'a>(
            &'a self,
            _request: &'a ChatCompletionRequest,
            _model_override: Option<&'a str>,
        ) -> provider::BoxFuture<'a, Result<provider::UpstreamResponse, UpstreamError>> {
            Box::pin(async move {
                match &self.answer {
                    Ok(()) => Ok(provider::UpstreamResponse {
                        response: ChatCompletionResponse {
                            choices: vec![],
                            created: 0,
                            id: "id".to_owned(),
                            model: "m".to_owned(),
                            object: "chat.completion".to_owned(),
                            provider_request_id: None,
                            served_model: Some("m".to_owned()),
                            served_model_root: None,
                            served_provider: None,
                            usage: Usage::default(),
                        },
                        bodies: provider::UpstreamBodies {
                            request_text: String::new(),
                            response_text: String::new(),
                        },
                    }),
                    Err((kind, message)) => match *kind {
                        "status" => Err(UpstreamError::Status {
                            provider: self.name.clone(),
                            status: 500,
                            body: (*message).to_owned(),
                        }),
                        "bad_request" => Err(UpstreamError::BadRequest {
                            provider: self.name.clone(),
                            message: (*message).to_owned(),
                        }),
                        "queue" => Err(UpstreamError::QueueRejected {
                            provider: self.name.clone(),
                        }),
                        _ => Err(UpstreamError::Empty {
                            provider: self.name.clone(),
                            body: (*message).to_owned(),
                        }),
                    },
                }
            })
        }
    }

    fn catalog() -> Catalog {
        Catalog::from_yaml(
            r#"
providers:
  - name: pgx-local
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: http://127.0.0.1:1/v1
    model: local-llm
    model_aliases:
      local-llm: local-llm
      tier-high: local-llm
  - name: deepseek
    enabled: false
    protocol: openai
    endpoint: https://api.example.com/v1
    model: deepseek-v4-flash
    api_key_env: DEEPSEEK_KEY
    model_aliases:
      local-llm: deepseek-v4-flash
default_chain: [pgx-local, deepseek]
"#,
        )
        .expect("catalog")
    }

    fn service(answer: Result<(), (&'static str, &'static str)>) -> RouterService {
        let catalog = catalog();
        let mut providers: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
        providers.insert(
            "pgx-local".to_owned(),
            Arc::new(Fake {
                name: "pgx-local".to_owned(),
                answer,
            }),
        );
        RouterService {
            catalog,
            providers,
            slot_wait: Duration::from_secs(1),
        }
    }

    fn request(model: &str) -> ChatCompletionRequest {
        ChatCompletionRequest {
            enable_reasoning: false,
            frequency_penalty: None,
            max_tokens: None,
            messages: vec![ChatMessage {
                content: serde_json::json!("hi"),
                reasoning_content: None,
                role: Role::User,
                tool_call_id: None,
                tool_calls: None,
            }],
            model: model.to_owned(),
            repetition_penalty: None,
            response_format: None,
            seed: None,
            stream: false,
            temperature: 0.1,
            tool_choice: serde_json::Value::Null,
            tools: None,
        }
    }

    #[tokio::test]
    async fn an_alias_auto_pins_to_its_owner() {
        let response = service(Ok(()))
            .complete(&request("tier-high"), None)
            .await
            .expect("completion");
        assert_eq!(response.served_provider.as_deref(), Some("pgx-local"));
    }

    #[tokio::test]
    async fn an_x_provider_pin_is_honoured_and_a_failure_is_not_failed_over() {
        let service = service(Err(("status", "boom")));
        let error = service
            .complete(&request("local-llm"), Some("pgx-local"))
            .await
            .expect_err("pinned failure");
        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert!(
            error.body["detail"]
                .as_str()
                .expect("detail")
                .contains("Pinned provider 'pgx-local' failed")
        );

        // Even with a chain that would retry, the call was pinned.
        let request = request("tier-high");
        let error = service
            .complete(&request, Some("pgx-local"))
            .await
            .expect_err("pinned");
        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn an_unknown_pin_is_a_400() {
        let error = service(Ok(()))
            .complete(&request("m"), Some("ghost"))
            .await
            .expect_err("unknown");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.body["detail"], "Unknown provider: ghost");
    }

    #[tokio::test]
    async fn an_unavailable_steering_alias_is_refused_not_silently_answered() {
        // `deepseek-v4-flash` is declared only by the disabled deepseek.
        let yaml = r#"
providers:
  - name: pgx-local
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: http://127.0.0.1:1/v1
    model: local-llm
  - name: deepseek
    enabled: false
    protocol: openai
    endpoint: https://api.example.com/v1
    model: deepseek-v4-flash
    model_aliases:
      deepseek-v4-flash: deepseek-v4-flash
default_chain: [pgx-local]
"#;
        let catalog = Catalog::from_yaml(yaml).expect("catalog");
        let mut providers: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
        providers.insert(
            "pgx-local".to_owned(),
            Arc::new(Fake {
                name: "pgx-local".to_owned(),
                answer: Ok(()),
            }),
        );
        let service = RouterService {
            catalog,
            providers,
            slot_wait: Duration::from_secs(1),
        };
        let error = service
            .complete(&request("deepseek-v4-flash"), None)
            .await
            .expect_err("nothing can serve it");
        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.headers[0].0, CLASSIFICATION_HEADER);
        assert_eq!(error.headers[0].1, "steering_alias_unavailable");
    }

    #[tokio::test]
    async fn a_non_alias_model_rides_the_chain_and_a_failure_is_a_503() {
        let service = service(Err(("status", "boom")));
        let error = service
            .complete(&request("some-worker-name"), None)
            .await
            .expect_err("chain");
        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            error.body["detail"]
                .as_str()
                .expect("detail")
                .starts_with("All providers failed. Last error:")
        );
        assert_eq!(error.headers[0].1, "upstream_5xx");
    }

    #[tokio::test]
    async fn a_bad_request_is_passed_through_as_a_400_envelope() {
        let service = service(Err(("bad_request", "prompt too long")));
        let error = service
            .complete(&request("local-llm"), Some("pgx-local"))
            .await
            .expect_err("400");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.body["error"]["message"], "prompt too long");
        assert_eq!(error.body["error"]["type"], "invalid_request_error");
    }
}
