#![forbid(unsafe_code)]
//! The read documents of S16: the ledger's page, its filters and the usage report.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::views::PageIn;

/// `inference_calls.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct InferenceCallsListIn {
    /// `copilot | process_step | workflow_step | agent_run | eval | other`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_kind: Option<String>,
    /// One run's calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// One eval run's calls (S19 §9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_run_id: Option<Uuid>,
    /// The requested model alias (exact).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// `hit | write | miss | unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_status: Option<String>,
    /// Inclusive lower bound on the call time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// One ledger row.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InferenceCallView {
    /// The row id.
    pub id: Uuid,
    /// The `action_invocation` it was made inside.
    pub action_invocation_id: Option<Uuid>,
    /// The request trace.
    pub trace_id: Option<String>,
    /// The provider correlation id.
    pub correlation_id: Option<String>,
    /// Who made the call.
    pub caller_kind: String,
    /// The owning run.
    pub run_id: Option<Uuid>,
    /// The owning step.
    pub run_step_id: Option<Uuid>,
    /// The owning copilot turn.
    pub conversation_turn_id: Option<Uuid>,
    /// The owning eval run, when the caller is an eval.
    pub eval_run_id: Option<Uuid>,
    /// The prompt family.
    pub prompt_ref: Option<String>,
    /// The prompt version.
    pub prompt_version: Option<String>,
    /// SHA-256 of the rendered prompt; never the body.
    pub prompt_sha256: Option<String>,
    /// The requested model alias.
    pub model: String,
    /// The model the provider reports having served.
    pub served_model: Option<String>,
    /// Input tokens.
    pub prompt_tokens: i32,
    /// Output tokens.
    pub completion_tokens: i32,
    /// Total tokens.
    pub total_tokens: i32,
    /// Tokens written into the prompt cache.
    pub cache_creation_input_tokens: i32,
    /// Tokens read from the prompt cache.
    pub cache_read_input_tokens: i32,
    /// `hit | write | miss | unknown`.
    pub cache_status: String,
    /// Wall time.
    pub latency_ms: i32,
    /// `ok | refused | error | timeout`.
    pub outcome: String,
    /// When the call was recorded.
    pub created_at: DateTime<Utc>,
}

/// `inference_usage.report` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InferenceUsageReportIn {
    /// Inclusive lower bound on the call time.
    pub since: DateTime<Utc>,
    /// Exclusive upper bound on the call time.
    pub until: DateTime<Utc>,
    /// `day | model | caller_kind | cache_status | prompt_sha`.
    pub group_by: String,
}

/// The usage report.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UsageReportOut {
    /// The dimension the rows are grouped by.
    pub group_by: String,
    /// The window's start.
    pub since: DateTime<Utc>,
    /// The window's end.
    pub until: DateTime<Utc>,
    /// One row per bucket.
    pub rows: Vec<UsageRowView>,
}

/// One report row (the domain's [`conveyor_domain::inference::UsageRow`] carries no schema).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UsageRowView {
    /// The group's value.
    pub bucket: String,
    /// Calls in the bucket.
    pub calls: i64,
    /// Input tokens summed.
    pub prompt_tokens: i64,
    /// Output tokens summed.
    pub completion_tokens: i64,
    /// Calls whose cache class was `hit`.
    pub hits: i64,
    /// Calls whose cache class was `write`.
    pub writes: i64,
    /// Calls whose cache class was `miss`.
    pub misses: i64,
    /// Calls whose cache class was `unknown`.
    pub unknown: i64,
    /// `hits / calls`.
    pub hit_ratio: f64,
}

impl From<conveyor_domain::inference::UsageRow> for UsageRowView {
    fn from(row: conveyor_domain::inference::UsageRow) -> Self {
        Self {
            bucket: row.bucket,
            calls: row.calls,
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            hits: row.hits,
            writes: row.writes,
            misses: row.misses,
            unknown: row.unknown,
            hit_ratio: row.hit_ratio,
        }
    }
}
