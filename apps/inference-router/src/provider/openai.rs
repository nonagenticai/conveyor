//! The OpenAI-compatible provider — vLLM (pgx-local), DeepSeek, xAI, any
//! `/v1/chat/completions` backend.
//!
//! The upstream body is built to be byte-equivalent in *content* to the live service's
//! `_base_chat_payload`: `temperature` and `max_tokens` are always present (null when unset), the
//! sampling extensions appear only when the caller set them, and `response_format`/`tools` drop
//! their `None` members. An absent value must leave the upstream body as if the feature had never
//! been added.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Map, Value, json};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::catalog::ProviderEntry;
use crate::contract::schemas::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Choice, ResponseFormatType, Role,
    ToolCall, Usage,
};

use super::{BoxFuture, BuildError, Provider, UpstreamBodies, UpstreamError, UpstreamResponse};

/// One OpenAI-compatible provider.
pub struct OpenAiProvider {
    pub(super) name: String,
    pub(super) endpoint: String,
    pub(super) model_id: String,
    pub(super) api_key: Option<String>,
    pub(super) client: reqwest::Client,
    pub(super) timeout: Duration,
    pub(super) slot: Option<Arc<Semaphore>>,
    pub(super) slot_wait: Duration,
    pub(super) response_format_mode: Option<String>,
    pub(super) force_chat_template_kwargs: bool,
}

impl OpenAiProvider {
    pub fn new(entry: &ProviderEntry, slot_wait: Duration) -> Result<Self, BuildError> {
        let endpoint = entry
            .endpoint
            .clone()
            .ok_or_else(|| BuildError::MissingEndpoint {
                name: entry.name.clone(),
            })?;
        let api_key = match (&entry.api_key_env, &entry.upstream_auth_env) {
            // A priced provider's key: absent or blank means "send no bearer", exactly as the
            // live service treats an unset `api_key_env`.
            (Some(variable), _) => std::env::var(variable).ok().filter(|v| !v.is_empty()),
            // Owner-hardware credential: there is no unauthenticated fallback.
            (None, Some(variable)) => {
                let value = std::env::var(variable).unwrap_or_default();
                if value.trim().is_empty() {
                    return Err(BuildError::MissingCredential {
                        name: entry.name.clone(),
                        variable: variable.clone(),
                    });
                }
                Some(value)
            }
            (None, None) => None,
        };
        Ok(Self {
            name: entry.name.clone(),
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            model_id: entry.model.clone(),
            api_key,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs_f64(entry.timeout),
            slot: entry
                .max_concurrent
                .map(|slots| Arc::new(Semaphore::new(slots))),
            slot_wait,
            response_format_mode: entry.response_format_mode.clone(),
            force_chat_template_kwargs: entry.force_chat_template_kwargs,
        })
    }
}

impl Provider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn complete<'a>(
        &'a self,
        request: &'a ChatCompletionRequest,
        model_override: Option<&'a str>,
    ) -> BoxFuture<'a, Result<UpstreamResponse, UpstreamError>> {
        Box::pin(async move {
            let model = model_override.unwrap_or(&self.model_id);
            let payload = chat_payload(
                request,
                model,
                false,
                self.response_format_mode.as_deref(),
                self.force_chat_template_kwargs,
            );
            let body = serde_json::to_string(&payload).expect("the payload serializes");

            // The slot wait is OUTSIDE the HTTP timeout, exactly as the live service has it: a
            // saturated provider sheds with `queue_rejected` instead of holding the caller.
            let _permit = match &self.slot {
                Some(semaphore) => {
                    match tokio::time::timeout(self.slot_wait, semaphore.clone().acquire_owned())
                        .await
                    {
                        Ok(Ok(permit)) => Some(permit),
                        _ => {
                            return Err(UpstreamError::QueueRejected {
                                provider: self.name.clone(),
                            });
                        }
                    }
                }
                None => None,
            };

            let url = format!("{}/chat/completions", self.endpoint);
            let mut builder = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .body(body.clone())
                .timeout(self.timeout);
            if let Some(key) = &self.api_key {
                builder = builder.header("authorization", format!("Bearer {key}"));
            }
            let response = builder
                .send()
                .await
                .map_err(|error| UpstreamError::Transport {
                    provider: self.name.clone(),
                    message: describe_reqwest(&error),
                })?;

            let status = response.status();
            let text = response
                .text()
                .await
                .map_err(|error| UpstreamError::Transport {
                    provider: self.name.clone(),
                    message: describe_reqwest(&error),
                })?;
            if !status.is_success() {
                if status.as_u16() == 400 {
                    return Err(UpstreamError::BadRequest {
                        provider: self.name.clone(),
                        message: upstream_message(&text),
                    });
                }
                return Err(UpstreamError::Status {
                    provider: self.name.clone(),
                    status: status.as_u16(),
                    body: text,
                });
            }

            let parsed: Value =
                serde_json::from_str(&text).map_err(|_| UpstreamError::Malformed {
                    provider: self.name.clone(),
                    body: text.clone(),
                })?;
            let response = parse_openai_response(&parsed, &request.model, model, &self.name)?;
            Ok(UpstreamResponse {
                response,
                bodies: UpstreamBodies {
                    request_text: body,
                    response_text: text,
                },
            })
        })
    }

    fn stream<'a>(
        &'a self,
        request: &'a ChatCompletionRequest,
        model_override: Option<&'a str>,
    ) -> BoxFuture<'a, Result<super::ChunkStream, UpstreamError>> {
        Box::pin(async move {
            let model = model_override.unwrap_or(&self.model_id);
            let payload = chat_payload(
                request,
                model,
                true,
                self.response_format_mode.as_deref(),
                self.force_chat_template_kwargs,
            );
            let body = serde_json::to_string(&payload).expect("the payload serializes");

            // The slot is held for the whole stream, not just the open: dropping the permit
            // when the future returned would free a slot while the upstream was still
            // generating. It rides the stream state and releases when the stream is dropped.
            let permit = match &self.slot {
                Some(semaphore) => {
                    match tokio::time::timeout(self.slot_wait, semaphore.clone().acquire_owned())
                        .await
                    {
                        Ok(Ok(permit)) => Some(permit),
                        _ => {
                            return Err(UpstreamError::QueueRejected {
                                provider: self.name.clone(),
                            });
                        }
                    }
                }
                None => None,
            };

            let url = format!("{}/chat/completions", self.endpoint);
            let mut builder = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .body(body)
                .timeout(self.timeout);
            if let Some(key) = &self.api_key {
                builder = builder.header("authorization", format!("Bearer {key}"));
            }
            let response = builder
                .send()
                .await
                .map_err(|error| UpstreamError::Transport {
                    provider: self.name.clone(),
                    message: describe_reqwest(&error),
                })?;

            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                if status.as_u16() == 400 {
                    return Err(UpstreamError::BadRequest {
                        provider: self.name.clone(),
                        message: upstream_message(&text),
                    });
                }
                return Err(UpstreamError::Status {
                    provider: self.name.clone(),
                    status: status.as_u16(),
                    body: text,
                });
            }

            let source = response.bytes_stream().map(|item| {
                item.map(|bytes| bytes.to_vec())
                    .map_err(|error| describe_reqwest(&error))
            });
            Ok(sse_stream(
                source,
                self.name.clone(),
                self.model_id.clone(),
                request.model.clone(),
                permit,
            ))
        })
    }
}

/// One provider's SSE byte source, parsed into chunks.
///
/// The source is erased to `Result<Vec<u8>, String>` so the state is nameable without the
/// `reqwest`/`bytes` types. Frames are split on `\n\n` **at the byte level** before UTF-8
/// decoding: a multi-byte character split across two network reads must not corrupt the frame
/// that follows it.
fn sse_stream<S, B, E>(
    source: S,
    provider: String,
    default_served_model: String,
    requested_model: String,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
) -> super::ChunkStream
where
    S: futures_util::Stream<Item = Result<B, E>> + Send + 'static,
    B: AsRef<[u8]> + Send + 'static,
    E: Into<String> + Send + 'static,
{
    let source = source.map(|item| {
        item.map(|bytes| bytes.as_ref().to_vec())
            .map_err(Into::into)
    });
    let source: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<Vec<u8>, String>> + Send>,
    > = Box::pin(source);
    let state = SseState {
        source,
        buffer: Vec::new(),
        done: false,
        provider,
        default_served_model,
        requested_model,
        _permit: permit,
    };
    Box::pin(futures_util::stream::unfold(
        state,
        |mut state| async move {
            loop {
                if state.done {
                    return None;
                }
                if let Some(frame) = state.take_frame() {
                    match state.parse_frame(&frame) {
                        Frame::Skip => continue,
                        Frame::Done => {
                            return None;
                        }
                        Frame::Chunk(chunk) => return Some((Ok(chunk), state)),
                        Frame::Failure(error) => {
                            state.done = true;
                            return Some((Err(error), state));
                        }
                    }
                }
                match state.source.next().await {
                    Some(Ok(bytes)) => state.buffer.extend_from_slice(&bytes),
                    Some(Err(message)) => {
                        state.done = true;
                        return Some((
                            Err(UpstreamError::Transport {
                                provider: state.provider.clone(),
                                message,
                            }),
                            state,
                        ));
                    }
                    // An upstream that closes without `[DONE]` ends the stream, exactly as an
                    // exhausted iterator does in the reference; the router books it as a
                    // completion and the capture decides whether it looks complete.
                    None => {
                        return None;
                    }
                }
            }
        },
    ))
}

struct SseState {
    source: std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<Vec<u8>, String>> + Send>>,
    buffer: Vec<u8>,
    done: bool,
    provider: String,
    default_served_model: String,
    requested_model: String,
    /// Held for the stream's lifetime; see `stream()`.
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

enum Frame {
    Skip,
    Done,
    Chunk(crate::contract::chunks::ChatCompletionChunk),
    Failure(UpstreamError),
}

impl SseState {
    fn take_frame(&mut self) -> Option<String> {
        let position = self
            .buffer
            .windows(2)
            .position(|window| window == b"\n\n")?;
        let frame: Vec<u8> = self.buffer.drain(..position + 2).collect();
        Some(String::from_utf8_lossy(&frame[..position]).into_owned())
    }

    fn parse_frame(&self, frame: &str) -> Frame {
        let mut payload = String::new();
        for line in frame.split('\n') {
            let line = line.trim_end_matches('\r');
            if let Some(rest) = line.strip_prefix("data:") {
                if !payload.is_empty() {
                    payload.push('\n');
                }
                payload.push_str(rest.trim_start());
            }
        }
        if payload.is_empty() {
            return Frame::Skip; // an SSE comment (`: keepalive`) or an empty frame
        }
        if payload.trim() == "[DONE]" {
            return Frame::Done;
        }
        let Ok(value) = serde_json::from_str::<Value>(&payload) else {
            return Frame::Failure(UpstreamError::Malformed {
                provider: self.provider.clone(),
                body: payload,
            });
        };
        if !value.is_object() {
            return Frame::Failure(UpstreamError::Malformed {
                provider: self.provider.clone(),
                body: payload,
            });
        }
        match parse_openai_chunk(&value, &self.requested_model, &self.default_served_model) {
            Ok(chunk) => Frame::Chunk(chunk),
            Err(error) => Frame::Failure(error),
        }
    }
}

/// Parse one upstream chunk dict, with the reference's own rules.
///
/// `model` is the REQUESTED model (what the caller named); `served_model` and
/// `provider_request_id` are populated only on the usage-bearing terminal chunk, and
/// `served_provider` is left null for the router to stamp.
fn parse_openai_chunk(
    data: &Value,
    requested_model: &str,
    default_served_model: &str,
) -> Result<crate::contract::chunks::ChatCompletionChunk, UpstreamError> {
    use crate::contract::chunks::{ChatCompletionChunk, ChunkChoice, ChunkDelta, ToolCallDelta};

    let choices: Vec<ChunkChoice> = data
        .get("choices")
        .and_then(Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .map(|choice| {
                    let delta = choice.get("delta").cloned().unwrap_or(Value::Null);
                    ChunkChoice {
                        index: choice.get("index").and_then(Value::as_i64).unwrap_or(0) as i32,
                        delta: ChunkDelta {
                            role: delta.get("role").and_then(Value::as_str).map(str::to_owned),
                            content: delta
                                .get("content")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            tool_calls: delta.get("tool_calls").and_then(Value::as_array).map(
                                |calls| {
                                    calls
                                        .iter()
                                        .map(|call| ToolCallDelta {
                                            index: call
                                                .get("index")
                                                .and_then(Value::as_i64)
                                                .unwrap_or(0)
                                                as i32,
                                            id: call
                                                .get("id")
                                                .and_then(Value::as_str)
                                                .map(str::to_owned),
                                            kind: call
                                                .get("type")
                                                .and_then(Value::as_str)
                                                .map(str::to_owned),
                                            function: call.get("function").cloned(),
                                        })
                                        .collect()
                                },
                            ),
                        },
                        finish_reason: choice
                            .get("finish_reason")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let usage = data
        .get("usage")
        .filter(|usage| !usage.is_null())
        .map(|usage| Usage {
            prompt_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            completion_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            total_tokens: usage
                .get("total_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            ..Usage::default()
        });

    let served_model = if usage.is_some() {
        data.get("model")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(default_served_model.to_owned()).filter(|model| !model.is_empty()))
            .or_else(|| Some(requested_model.to_owned()))
    } else {
        None
    };
    let provider_request_id = if usage.is_some() {
        data.get("id").and_then(Value::as_str).map(str::to_owned)
    } else {
        None
    };

    Ok(ChatCompletionChunk {
        id: data
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default(),
        object: "chat.completion.chunk".to_owned(),
        created: data.get("created").and_then(Value::as_i64).unwrap_or(0),
        model: requested_model.to_owned(),
        served_model,
        served_provider: None,
        provider_request_id,
        choices,
        usage,
    })
}

fn describe_reqwest(error: &reqwest::Error) -> String {
    let text = error.to_string();
    if text.trim().is_empty() {
        "request failed without a message".to_owned()
    } else {
        text
    }
}

/// The upstream's own reason for a 400, from its OpenAI envelope when it has one.
fn upstream_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| body.to_owned())
}

fn format_type_str(format: ResponseFormatType) -> &'static str {
    match format {
        ResponseFormatType::Text => "text",
        ResponseFormatType::JsonObject => "json_object",
        ResponseFormatType::JsonSchema => "json_schema",
    }
}

/// The upstream `/chat/completions` body.
pub(crate) fn chat_payload(
    request: &ChatCompletionRequest,
    model: &str,
    stream: bool,
    response_format_mode: Option<&str>,
    force_chat_template_kwargs: bool,
) -> Value {
    let mut payload = Map::new();
    payload.insert("model".to_owned(), json!(model));
    payload.insert(
        "messages".to_owned(),
        serde_json::to_value(&request.messages).expect("messages serialize"),
    );
    payload.insert("temperature".to_owned(), json!(request.temperature));
    payload.insert("max_tokens".to_owned(), json!(request.max_tokens));
    if let Some(seed) = request.seed {
        payload.insert("seed".to_owned(), json!(seed));
    }
    if let Some(penalty) = request.repetition_penalty {
        payload.insert("repetition_penalty".to_owned(), json!(penalty));
    }
    if let Some(penalty) = request.frequency_penalty {
        payload.insert("frequency_penalty".to_owned(), json!(penalty));
    }
    if stream {
        payload.insert("stream".to_owned(), json!(true));
        payload.insert("stream_options".to_owned(), json!({"include_usage": true}));
    }

    if let Some(format) = &request.response_format {
        if response_format_mode == Some("json_object")
            && format.r#type == ResponseFormatType::JsonSchema
        {
            // The upstream cannot do guided JSON: basic JSON mode plus the schema in-prompt.
            payload.insert("response_format".to_owned(), json!({"type": "json_object"}));
            if let Some(spec) = &format.json_schema {
                inject_schema_prompt(
                    &mut payload,
                    &serde_json::to_string(&spec.schema).unwrap_or_default(),
                );
            }
        } else {
            let mut value = Map::new();
            value.insert("type".to_owned(), json!(format_type_str(format.r#type)));
            if let Some(spec) = &format.json_schema {
                let mut schema = Map::new();
                if let Some(name) = &spec.name {
                    schema.insert("name".to_owned(), json!(name));
                }
                schema.insert(
                    "schema".to_owned(),
                    serde_json::to_value(&spec.schema).unwrap_or(Value::Null),
                );
                if spec.strict {
                    schema.insert("strict".to_owned(), json!(true));
                }
                value.insert("json_schema".to_owned(), Value::Object(schema));
            }
            payload.insert("response_format".to_owned(), Value::Object(value));
        }
    }

    if let Some(tools) = &request.tools {
        let tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                let mut function = Map::new();
                function.insert("name".to_owned(), json!(tool.function.name));
                if let Some(description) = &tool.function.description {
                    function.insert("description".to_owned(), json!(description));
                }
                function.insert(
                    "parameters".to_owned(),
                    serde_json::to_value(&tool.function.parameters).unwrap_or(Value::Null),
                );
                json!({"type": tool.r#type, "function": function})
            })
            .collect();
        payload.insert("tools".to_owned(), json!(tools));
    }
    if !request.tool_choice.is_null() {
        payload.insert("tool_choice".to_owned(), request.tool_choice.clone());
    }
    if force_chat_template_kwargs {
        payload.insert(
            "chat_template_kwargs".to_owned(),
            json!({"enable_thinking": request.enable_reasoning}),
        );
    }
    Value::Object(payload)
}

fn inject_schema_prompt(payload: &mut Map<String, Value>, schema_json: &str) {
    let Some(Value::Array(messages)) = payload.get_mut("messages") else {
        return;
    };
    messages.insert(
        0,
        json!({
            "role": "system",
            "content": format!(
                "Respond with ONLY a single JSON object conforming to this JSON schema. \
                 Output no prose, markdown, or code fences. JSON schema: {schema_json}"
            ),
        }),
    );
}

/// Parse an OpenAI-compatible response into the contract type.
fn parse_openai_response(
    data: &Value,
    requested_model: &str,
    default_served_model: &str,
    provider: &str,
) -> Result<ChatCompletionResponse, UpstreamError> {
    let mut choices = Vec::new();
    if let Some(items) = data.get("choices").and_then(Value::as_array) {
        for (index, choice) in items.iter().enumerate() {
            let message = choice.get("message").cloned().unwrap_or(Value::Null);
            let content = message.get("content").cloned().unwrap_or(Value::Null);
            // vLLM's reasoning parser emits the trace under either `reasoning_content` or
            // `reasoning`; a reasoning-only answer becomes the content.
            let reasoning = message
                .get("reasoning_content")
                .or_else(|| message.get("reasoning"))
                .cloned()
                .filter(|value| !value.is_null());
            let tool_calls = message
                .get("tool_calls")
                .cloned()
                .filter(|value| !value.is_null());

            let content = if content.is_null() {
                match &reasoning {
                    Some(reasoning) if tool_calls.is_none() => reasoning.clone(),
                    // Tool-call replies keep content null so callers can branch on tool_calls.
                    _ if tool_calls.is_some() => Value::Null,
                    _ => json!(""),
                }
            } else {
                content
            };

            let role = message
                .get("role")
                .cloned()
                .map(|role| serde_json::from_value::<Role>(role).unwrap_or(Role::Assistant))
                .unwrap_or(Role::Assistant);
            let tool_calls: Option<Vec<ToolCall>> =
                match tool_calls {
                    Some(value) => Some(serde_json::from_value(value).map_err(|_| {
                        UpstreamError::Malformed {
                            provider: provider.to_owned(),
                            body: "tool_calls did not match the contract".to_owned(),
                        }
                    })?),
                    None => None,
                };
            let reasoning_content = reasoning.and_then(|value| value.as_str().map(str::to_owned));

            choices.push(Choice {
                index: index as i64,
                message: ChatMessage {
                    content,
                    reasoning_content,
                    role,
                    tool_call_id: message
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    tool_calls,
                },
                finish_reason: choice
                    .get("finish_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("stop")
                    .to_owned(),
            });
        }
    }

    if choices.is_empty() {
        // A backend that answered outside the OpenAI shape but DID produce an answer: salvage it.
        // An empty body is a FAILURE, never an empty-but-valid choice.
        let salvaged = data
            .get("content")
            .or_else(|| data.get("response"))
            .filter(|value| !value.is_null() && value.as_str().is_none_or(|text| !text.is_empty()));
        match salvaged {
            Some(salvaged) => choices.push(Choice {
                index: 0,
                message: ChatMessage {
                    content: salvaged.clone(),
                    reasoning_content: None,
                    role: Role::Assistant,
                    tool_call_id: None,
                    tool_calls: None,
                },
                finish_reason: "stop".to_owned(),
            }),
            None => {
                return Err(UpstreamError::Empty {
                    provider: provider.to_owned(),
                    body: crate::provider::truncate_json(data),
                });
            }
        }
    }

    let usage = data.get("usage").cloned().unwrap_or(Value::Null);
    let usage = Usage {
        prompt_tokens: usage
            .get("prompt_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        completion_tokens: usage
            .get("completion_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        total_tokens: usage
            .get("total_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        ..Usage::default()
    };
    let upstream_id = data.get("id").and_then(Value::as_str);
    let served_model = data
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(default_served_model);
    Ok(ChatCompletionResponse {
        choices,
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0),
        id: upstream_id.map_or_else(
            || format!("chatcmpl-{}", &Uuid::now_v7().simple().to_string()[..12]),
            str::to_owned,
        ),
        model: requested_model.to_owned(),
        object: "chat.completion".to_owned(),
        provider_request_id: upstream_id.map(str::to_owned),
        served_model: Some(served_model.to_owned()),
        served_model_root: None,
        served_provider: None,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Json, Router};
    use tokio::sync::Mutex;

    use super::*;

    struct Stub {
        status: u16,
        body: Value,
        last_request: Mutex<Option<Value>>,
    }

    async fn stub_handler(
        State(stub): State<Arc<Stub>>,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        *stub.last_request.lock().await = Some(body);
        let status = axum::http::StatusCode::from_u16(stub.status).expect("status");
        (status, Json(stub.body.clone()))
    }

    async fn start_stub(status: u16, body: Value) -> (Arc<Stub>, String) {
        let stub = Arc::new(Stub {
            status,
            body,
            last_request: Mutex::new(None),
        });
        let app = Router::new()
            .route("/v1/chat/completions", post(stub_handler))
            .with_state(stub.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("stub serves");
        });
        (stub, format!("http://{address}/v1"))
    }

    fn provider(endpoint: &str) -> OpenAiProvider {
        OpenAiProvider {
            name: "stub".to_owned(),
            endpoint: endpoint.to_owned(),
            model_id: "local-llm".to_owned(),
            api_key: None,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(10),
            slot: None,
            slot_wait: Duration::from_secs(1),
            response_format_mode: None,
            force_chat_template_kwargs: false,
        }
    }

    fn request(model: &str) -> ChatCompletionRequest {
        ChatCompletionRequest {
            enable_reasoning: false,
            frequency_penalty: None,
            max_tokens: Some(16),
            messages: vec![ChatMessage {
                content: json!("hello"),
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
            tool_choice: Value::Null,
            tools: None,
        }
    }

    fn upstream_body() -> Value {
        json!({
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "created": 1,
            "model": "local-llm",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
        })
    }

    #[tokio::test]
    async fn a_completion_round_trips_and_echoes_the_requested_alias() {
        let (stub, endpoint) = start_stub(200, upstream_body()).await;
        let provider = provider(&endpoint);
        let response = provider
            .complete(&request("alias-name"), Some("local-llm"))
            .await
            .expect("completion");

        // The upstream model is the resolved id; the caller's alias is echoed back.
        assert_eq!(response.response.model, "alias-name");
        assert_eq!(response.response.served_model.as_deref(), Some("local-llm"));
        assert_eq!(response.response.id, "chatcmpl-abc123");
        assert_eq!(
            response.response.provider_request_id.as_deref(),
            Some("chatcmpl-abc123")
        );
        assert_eq!(response.response.choices[0].message.content, json!("hi"));
        assert!(response.response.created > 0);

        let sent = stub
            .last_request
            .lock()
            .await
            .clone()
            .expect("the stub saw the request");
        assert_eq!(sent["model"], "local-llm");
        assert_eq!(sent["temperature"], 0.1);
        assert_eq!(sent["max_tokens"], 16);
        // An unset sampling extension is absent, not null.
        assert!(sent.get("seed").is_none());
        assert!(sent.get("stream").is_none());
        assert_eq!(sent["messages"][0]["role"], "user");
    }

    #[tokio::test]
    async fn an_upstream_400_passes_its_reason_through() {
        let (_stub, endpoint) = start_stub(
            400,
            json!({"error": {"message": "prompt too long", "type": "invalid_request_error"}}),
        )
        .await;
        let error = provider(&endpoint)
            .complete(&request("m"), None)
            .await
            .expect_err("400");
        assert!(
            matches!(error, UpstreamError::BadRequest { ref message, .. } if message == "prompt too long"),
            "{error}"
        );
        assert_eq!(error.classification(), "upstream_400");
    }

    #[tokio::test]
    async fn an_upstream_5xx_is_a_status_error() {
        let (_stub, endpoint) = start_stub(503, json!({"error": "overloaded"})).await;
        let error = provider(&endpoint)
            .complete(&request("m"), None)
            .await
            .expect_err("503");
        assert!(
            matches!(error, UpstreamError::Status { status: 503, .. }),
            "{error}"
        );
        assert_eq!(error.classification(), "upstream_5xx");
    }

    #[tokio::test]
    async fn empty_choices_are_a_failure_not_an_empty_answer() {
        let (_stub, endpoint) = start_stub(
            200,
            json!({"id": "x", "model": "m", "choices": [], "usage": {}}),
        )
        .await;
        let error = provider(&endpoint)
            .complete(&request("m"), None)
            .await
            .expect_err("empty");
        assert!(matches!(error, UpstreamError::Empty { .. }), "{error}");
        assert_eq!(error.classification(), "upstream_empty_response");
    }

    #[tokio::test]
    async fn a_reasoning_only_answer_becomes_the_content() {
        let (_stub, endpoint) = start_stub(
            200,
            json!({
                "id": "x",
                "model": "m",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": null, "reasoning": "the answer"}, "finish_reason": "stop"}],
                "usage": {}
            }),
        )
        .await;
        let response = provider(&endpoint)
            .complete(&request("m"), None)
            .await
            .expect("salvaged");
        assert_eq!(
            response.response.choices[0].message.content,
            json!("the answer")
        );
        assert_eq!(
            response.response.choices[0]
                .message
                .reasoning_content
                .as_deref(),
            Some("the answer")
        );
    }

    #[test]
    fn response_format_and_tools_omit_their_absent_members() {
        let mut request = request("m");
        request.response_format = Some(crate::contract::schemas::ResponseFormat {
            json_schema: Some(crate::contract::schemas::JsonSchemaSpec {
                name: None,
                schema: [("type".to_owned(), json!("object"))].into(),
                strict: false,
            }),
            r#type: ResponseFormatType::JsonSchema,
        });
        let payload = chat_payload(&request, "m", false, None, false);
        assert_eq!(payload["response_format"]["type"], "json_schema");
        assert_eq!(
            payload["response_format"]["json_schema"]["schema"]["type"],
            "object"
        );
        assert!(
            payload["response_format"]["json_schema"]
                .get("name")
                .is_none()
        );
        assert!(
            payload["response_format"]["json_schema"]
                .get("strict")
                .is_none()
        );
        assert!(payload.get("tools").is_none());
    }
}
