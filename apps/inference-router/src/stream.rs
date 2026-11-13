//! The in-memory capture behind a streamed `inference_call` row.
//!
//! The semantics are the reference's `services/stream_ledger.py`: every relayed chunk is folded
//! into a fingerprint, and the row hashes **that** rather than the SSE bytes, so an identical
//! generation hashes identically streamed and non-streamed. Captured text is capped at 4 Mi
//! characters; past the cap the capture stops accumulating and `response_json_valid` becomes
//! unknown (`None`), never false. The client-visible stream is never blocked, delayed or failed
//! by the capture.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::contract::chunks::ChatCompletionChunk;
use crate::contract::schemas::Usage;

/// `inference_call.stream_status` values (the reference's migration 0014 CHECK).
pub const STREAM_COMPLETED: &str = "completed";
pub const STREAM_CLIENT_DISCONNECT: &str = "client_disconnect";
pub const STREAM_UPSTREAM_ERROR: &str = "upstream_error";
pub const STREAM_TIMEOUT: &str = "timeout";

/// The reference's `STREAM_CAPTURE_MAX_CHARS`.
const MAX_CHARS: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Budget {
    chars: usize,
    truncated: bool,
}

impl Budget {
    /// Count `n` characters, or refuse once the cap is crossed. A refused fragment is dropped
    /// and the capture is marked truncated — the row hashes the prefix that was seen.
    fn take(&mut self, n: usize) -> bool {
        if self.truncated {
            return false;
        }
        if self.chars + n > MAX_CHARS {
            self.truncated = true;
            return false;
        }
        self.chars += n;
        true
    }
}

#[derive(Default)]
struct ToolCallParts {
    kind: Option<String>,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ChoiceParts {
    content: Option<String>,
    finish_reason: Option<String>,
    tool_calls: BTreeMap<i32, ToolCallParts>,
}

#[derive(Default)]
pub struct StreamCapture {
    budget: Budget,
    /// A frame could not be parsed: the hash would describe less than was sent, so validity is
    /// reported unknown.
    broken: bool,
    choices: BTreeMap<i32, ChoiceParts>,
    pub served_provider: Option<String>,
    pub served_model: Option<String>,
    /// The terminal usage-bearing chunk's usage, for the cost event.
    pub usage: Option<Usage>,
    pub provider_request_id: Option<String>,
}

impl StreamCapture {
    /// Fold one relayed chunk into the capture. Never fails the stream.
    pub fn feed(&mut self, chunk: &ChatCompletionChunk) {
        // The router stamps served_provider on the terminal usage chunk only; the capture takes
        // the last non-null value it sees, exactly as the reference does.
        if chunk.usage.is_some() {
            if chunk.served_provider.is_some() {
                self.served_provider = chunk.served_provider.clone();
            }
            if chunk.served_model.is_some() {
                self.served_model = chunk.served_model.clone();
            }
            if chunk.provider_request_id.is_some() {
                self.provider_request_id = chunk.provider_request_id.clone();
            }
            self.usage = chunk.usage.clone();
        }

        // Disjoint field borrows: the budget and the choice map are separate fields, so the
        // `take` calls below do not fight the `entry` borrow.
        let budget = &mut self.budget;
        let choices = &mut self.choices;
        for choice in &chunk.choices {
            let parts = choices.entry(choice.index).or_default();
            if let Some(reason) = &choice.finish_reason {
                parts.finish_reason = Some(reason.clone());
            }
            if let Some(content) = &choice.delta.content {
                if parts.content.is_none() {
                    parts.content = Some(String::new());
                }
                if !content.is_empty() && budget.take(content.chars().count()) {
                    parts
                        .content
                        .get_or_insert_with(String::new)
                        .push_str(content);
                }
            }
            if let Some(calls) = &choice.delta.tool_calls {
                for call in calls {
                    let call_parts = parts.tool_calls.entry(call.index).or_default();
                    if let Some(kind) = &call.kind {
                        call_parts.kind = Some(kind.clone());
                    }
                    if let Some(function) = call.function.as_ref().and_then(Value::as_object) {
                        for (key, sink) in [
                            ("name", &mut call_parts.name),
                            ("arguments", &mut call_parts.arguments),
                        ] {
                            if let Some(fragment) = function.get(key).and_then(Value::as_str)
                                && !fragment.is_empty()
                                && budget.take(fragment.chars().count())
                            {
                                sink.push_str(fragment);
                            }
                        }
                    }
                }
            }
        }
    }

    /// An upstream frame that could not be parsed: validity is unknown, the stream continues.
    pub fn mark_broken(&mut self) {
        self.broken = true;
    }

    pub fn finish_reason(&self) -> Option<&str> {
        self.choices
            .get(&0)
            .and_then(|parts| parts.finish_reason.as_deref())
    }

    /// Choice 0's concatenated assistant content, `None` when the stream sent none.
    pub fn content(&self) -> Option<&str> {
        self.choices
            .get(&0)
            .and_then(|parts| parts.content.as_deref())
    }

    /// The `response_fingerprint` shape, rebuilt from the deltas.
    fn fingerprint(&self) -> Value {
        let choices: Vec<Value> = self
            .choices
            .iter()
            .map(|(index, parts)| {
                let mut object = Map::new();
                object.insert("index".to_owned(), Value::from(*index));
                object.insert(
                    "content".to_owned(),
                    parts.content.clone().map_or(Value::Null, Value::String),
                );
                // The chunk schema carries no reasoning delta (the reference documents the gap).
                object.insert("reasoning_content".to_owned(), Value::Null);
                object.insert(
                    "finish_reason".to_owned(),
                    parts
                        .finish_reason
                        .clone()
                        .map_or(Value::Null, Value::String),
                );
                object.insert(
                    "tool_calls".to_owned(),
                    if parts.tool_calls.is_empty() {
                        Value::Null
                    } else {
                        Value::Array(
                            parts
                                .tool_calls
                                .values()
                                .map(|call| {
                                    let mut function = Map::new();
                                    function.insert(
                                        "name".to_owned(),
                                        Value::String(call.name.clone()),
                                    );
                                    function.insert(
                                        "arguments".to_owned(),
                                        Value::String(call.arguments.clone()),
                                    );
                                    let mut entry = Map::new();
                                    entry.insert(
                                        "type".to_owned(),
                                        Value::String(
                                            call.kind
                                                .clone()
                                                .unwrap_or_else(|| "function".to_owned()),
                                        ),
                                    );
                                    entry.insert("function".to_owned(), Value::Object(function));
                                    Value::Object(entry)
                                })
                                .collect(),
                        )
                    },
                );
                Value::Object(object)
            })
            .collect();
        Value::Array(choices)
    }

    pub fn response_sha256(&self) -> Option<String> {
        if self.choices.is_empty() {
            return None;
        }
        Some(crate::ledger::sha256_hex(&crate::ledger::canonical_json(
            &self.fingerprint(),
        )))
    }

    pub fn response_json_valid(&self) -> Option<bool> {
        if self.budget.truncated || self.broken {
            return None;
        }
        let content = self.content()?;
        match serde_json::from_str::<Value>(content) {
            Ok(_) => Some(true),
            Err(error) if error.to_string().contains("recursion limit") => None,
            Err(_) => Some(false),
        }
    }
}
