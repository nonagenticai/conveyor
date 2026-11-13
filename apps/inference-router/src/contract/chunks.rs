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
