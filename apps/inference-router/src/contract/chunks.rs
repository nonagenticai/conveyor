//! The streaming wire types.
//!
//! These are deliberately **not** part of the OpenAPI lock: the reference documents the
//! streaming response as `ChatCompletionResponse`, so adding a chunk schema to the served
//! document would drift `--check-lock`. Serde only — no `ToSchema`, no path reference.
//!
//! Field presence is contract, not preference. Pydantic serialises every field, so an
//! intermediate chunk carries `"tool_calls":null` and `"usage":null`; a serde `Option` with no
//! `skip_serializing_if` reproduces that. The golden stream compares byte-for-byte after the
//! normaliser sorts keys.
//!
//! `ChunkDelta::reasoning_content` is the ONE exception, and the exception is what parity
//! requires: the oracle's captured stream (`contract/golden/completion_stream.body`) predates
//! the reasoning channel, so a chunk carrying no reasoning must serialise exactly as it always
//! has. A chunk that DOES carry reasoning gains the field; that is strictly additive, and it is
//! the only way a streaming caller can see the model's thinking at all.

use serde::{Deserialize, Serialize};

use super::schemas::Usage;

/// One incremental tool-call fragment inside a chunk delta.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolCallDelta {
    pub index: i32,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub function: Option<serde_json::Value>,
}

/// The delta an OpenAI-compatible stream carries per choice.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChunkDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    /// The model's thinking for this step, when the upstream exposes it.
    ///
    /// Upstreams spell it two ways — OpenAI-style `reasoning_content` and vLLM's `reasoning`
    /// (which is what the Inkling build streams) — and the provider normalises both onto this
    /// field, exactly as the non-streamed message does.
    #[serde(default, alias = "reasoning", skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// One choice in a streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: i32,
    pub delta: ChunkDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// One SSE `data:` frame of `/api/chat/completions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    #[serde(default)]
    pub served_model: Option<String>,
    #[serde(default)]
    pub served_provider: Option<String>,
    #[serde(default)]
    pub provider_request_id: Option<String>,
    pub choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

impl ChatCompletionChunk {
    /// The frame the client receives, exactly as the reference serialises it.
    pub fn to_sse(&self) -> String {
        format!(
            "data: {}\n\n",
            serde_json::to_string(self).expect("a chunk always serialises")
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn a_delta_without_reasoning_omits_the_key_entirely() {
        let delta = ChunkDelta {
            content: Some("hi".to_owned()),
            ..ChunkDelta::default()
        };
        let value = serde_json::to_value(&delta).expect("serialises");
        assert_eq!(value["content"], "hi");
        assert!(value.get("reasoning_content").is_none());
        // Every other delta field keeps the pydantic convention: present, and null.
        assert!(value.get("tool_calls").is_some_and(Value::is_null));
    }

    #[test]
    fn the_upstream_reasoning_spelling_lands_on_reasoning_content() {
        let delta: ChunkDelta =
            serde_json::from_str(r#"{"reasoning":"thinking"}"#).expect("parses");
        assert_eq!(delta.reasoning_content.as_deref(), Some("thinking"));
        let value = serde_json::to_value(&delta).expect("serialises");
        assert_eq!(value["reasoning_content"], "thinking");
        assert!(value.get("reasoning").is_none());
    }
}
