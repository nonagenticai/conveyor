//! Inference usage and the prompt audit's pure shapes (S16, D23).
//!
//! Pure: the provider's `usage` parse, the four-way cache class, the model identity the ledger
//! stores, and the usage report's arithmetic. No IO — [`conveyor_store::inference`] owns the
//! rows and [`conveyor_store::prompt_audit`] the audit state.
//!
//! **Cache is four values, never two** (SPEC D23). A write is a deliberate warm priced above a
//! miss's saving, so folding it into "miss" would make every optimisation look like a regression.
//! The class is derived from the RESPONSE only: a caller that asked for a cache breakpoint still
//! misses on a cold prefix.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The router's `usage` block, as integers the ledger can store.
///
/// Missing fields are zero; a response with no `usage` object at all is
/// [`None`](Usage::from_response), which is the ledger's `unknown` class — the two are different
/// facts and the ledger keeps them apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens billed at the full rate.
    pub prompt_tokens: i32,
    /// Output tokens.
    pub completion_tokens: i32,
    /// The provider's total; `prompt + completion` when it omits one.
    pub total_tokens: i32,
    /// Tokens written into the prompt cache by this call.
    pub cache_creation_input_tokens: i32,
    /// Tokens read from the prompt cache by this call.
    pub cache_read_input_tokens: i32,
}

impl Usage {
    /// Parse the response's `usage` object. `None` means the response carried no usage at all
    /// (the ledger's `unknown`), never "all zeros".
    pub fn from_response(response: &Value) -> Option<Self> {
        let usage = response.get("usage")?.as_object()?;
        let prompt_tokens = tokens(usage.get("prompt_tokens"));
        let completion_tokens = tokens(usage.get("completion_tokens"));
        let total_tokens = usage
            .get("total_tokens")
            .and_then(Value::as_i64)
            .map_or(prompt_tokens.saturating_add(completion_tokens), |value| {
                value.clamp(0, i64::from(i32::MAX)) as i32
            });
        Some(Self {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cache_creation_input_tokens: tokens(usage.get("cache_creation_input_tokens")),
            cache_read_input_tokens: tokens(usage.get("cache_read_input_tokens")),
        })
    }
}

fn tokens(value: Option<&Value>) -> i32 {
    value
        .and_then(Value::as_i64)
        .map_or(0, |raw| raw.clamp(0, i64::from(i32::MAX)) as i32)
}

/// The four cache classes the ledger and its CHECK know (`inference_call_cache_status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    /// This call read the cache (`cache_read_input_tokens > 0`).
    Hit,
    /// This call warmed the cache and read nothing (`creation > 0`, `read = 0`).
    Write,
    /// Neither token count moved (`creation = 0`, `read = 0`).
    Miss,
    /// The response carried no usage at all.
    Unknown,
}

impl CacheStatus {
    /// Every class, in report order.
    pub const ALL: [CacheStatus; 4] = [
        CacheStatus::Hit,
        CacheStatus::Write,
        CacheStatus::Miss,
        CacheStatus::Unknown,
    ];

    /// The stored `inference_call.cache_status`.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheStatus::Hit => "hit",
            CacheStatus::Write => "write",
            CacheStatus::Miss => "miss",
            CacheStatus::Unknown => "unknown",
        }
    }

    /// Read a stored or requested cache class.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == raw)
    }

    /// The four-way rule, from the response's usage only.
    pub fn of(usage: Option<&Usage>) -> Self {
        match usage {
            None => CacheStatus::Unknown,
            Some(usage) if usage.cache_read_input_tokens > 0 => CacheStatus::Hit,
            Some(usage) if usage.cache_creation_input_tokens > 0 => CacheStatus::Write,
            Some(_) => CacheStatus::Miss,
        }
    }
}

/// Who made the call, matching `inference_call.caller_kind`'s CHECK.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallerKind {
    /// The copilot's turn runner.
    Copilot,
    /// A process step executor.
    ProcessStep,
    /// A workflow step (S13).
    WorkflowStep,
    /// An agent run (S15).
    AgentRun,
    /// An eval's subject or judge call (S19 §9); its join key is `eval_run_id`.
    Eval,
    /// Anything else (`other`); the one kind allowed to carry no join key.
    Other,
}

impl CallerKind {
    /// Every kind.
    pub const ALL: [CallerKind; 6] = [
        CallerKind::Copilot,
        CallerKind::ProcessStep,
        CallerKind::WorkflowStep,
        CallerKind::AgentRun,
        CallerKind::Eval,
        CallerKind::Other,
    ];

    /// The stored `inference_call.caller_kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            CallerKind::Copilot => "copilot",
            CallerKind::ProcessStep => "process_step",
            CallerKind::WorkflowStep => "workflow_step",
            CallerKind::AgentRun => "agent_run",
            CallerKind::Eval => "eval",
            CallerKind::Other => "other",
        }
    }

    /// Read a stored or requested caller kind.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == raw)
    }
}

/// What the engine stores for one successful model call.
///
/// The model identity is the requested `model` plus the provider's answer: the requested alias is
/// what the caller asked for, `served_model` what actually ran. Reporting groups on the served
/// model, falling back to the requested one when the router names none (owner decision,
/// 2026-11-09).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedUsage {
    /// The token counts; `None` when the response carried no `usage` object.
    pub usage: Option<Usage>,
    /// The requested model, or the served one when the request named none, or `"unrouted"`.
    pub model: String,
    /// The provider's answer, when it names one.
    pub served_model: Option<String>,
    /// The four-way cache class derived from `usage`.
    pub cache_status: CacheStatus,
}

/// Build the ledger's derived fields from the request, the response and the surface.
///
/// `surface` is `Some` only for the three non-token external actions. Their responses name no
/// model, so the surface is the pricing identity: the ledger stores it as `model` and leaves
/// `served_model` empty. A routed token call passes `None` and keeps the alias/served behaviour.
pub fn capture(
    requested_model: Option<&str>,
    response: &Value,
    surface: Option<&str>,
) -> CapturedUsage {
    let usage = Usage::from_response(response);
    let cache_status = CacheStatus::of(usage.as_ref());
    if let Some(surface) = surface.filter(|value| !value.is_empty()) {
        return CapturedUsage {
            usage,
            model: surface.to_owned(),
            served_model: None,
            cache_status,
        };
    }
    let served_model = response
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let model = requested_model
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| served_model.clone())
        .unwrap_or_else(|| "unrouted".to_owned());
    CapturedUsage {
        usage,
        model,
        served_model,
        cache_status,
    }
}

/// The rendered prompt's identity: SHA-256 of its JCS canonical JSON.
///
/// The ledger stores the hash, never the prompt body (CONTRACT §18). Grouping the cache report by
/// this value is what answers *which prompt* is cache-hostile (owner decision, 2026-11-09).
pub fn prompt_sha256(prompt: &Value) -> String {
    crate::compare::content_hash(prompt)
}

/// The commercial unit a non-token surface bills in.
///
/// The token surfaces bill per token; these three are the units the media hosts answer in, and the
/// price catalogue uses the same spelling. A call whose unit cannot be measured stays unpriced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageUnit {
    /// One rendered image.
    Image,
    /// One character of input text (speech synthesis).
    Character,
    /// One second of audio (transcription).
    Second,
}

impl UsageUnit {
    /// Every unit, in price-catalogue order.
    pub const ALL: [UsageUnit; 3] = [UsageUnit::Image, UsageUnit::Character, UsageUnit::Second];

    /// The stored spelling (also the price row's `unit`).
    pub fn as_str(self) -> &'static str {
        match self {
            UsageUnit::Image => "image",
            UsageUnit::Character => "character",
            UsageUnit::Second => "second",
        }
    }

    /// Read a stored or requested unit.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| unit.as_str() == raw)
    }
}

/// The dimensions `inference_usage.report` may group by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageGroup {
    /// UTC day of the call.
    Day,
    /// The served model, falling back to the requested one.
    Model,
    /// `caller_kind`.
    CallerKind,
    /// `cache_status`.
    CacheStatus,
    /// The rendered prompt's SHA-256.
    PromptSha,
    /// The run's project; a call with no run is the empty bucket.
    Project,
    /// The primary department of the run's job (via its binding); `unattributed` otherwise.
    Department,
    /// The run's project's programme; `unattributed` outside a programme.
    Programme,
    /// The run's job binding; `unattributed` for a run with no binding.
    Job,
    /// The run itself; `unattributed` for a call with no run.
    Run,
    /// The run step; `unattributed` for a call not made inside a step.
    Step,
}

impl UsageGroup {
    /// Every dimension.
    pub const ALL: [UsageGroup; 11] = [
        UsageGroup::Day,
        UsageGroup::Model,
        UsageGroup::CallerKind,
        UsageGroup::CacheStatus,
        UsageGroup::PromptSha,
        UsageGroup::Project,
        UsageGroup::Department,
        UsageGroup::Programme,
        UsageGroup::Job,
        UsageGroup::Run,
        UsageGroup::Step,
    ];

    /// The stored spelling (also the report's `group_by` input).
    pub fn as_str(self) -> &'static str {
        match self {
            UsageGroup::Day => "day",
            UsageGroup::Model => "model",
            UsageGroup::CallerKind => "caller_kind",
            UsageGroup::CacheStatus => "cache_status",
            UsageGroup::PromptSha => "prompt_sha",
            UsageGroup::Project => "project",
            UsageGroup::Department => "department",
            UsageGroup::Programme => "programme",
            UsageGroup::Job => "job",
            UsageGroup::Run => "run",
            UsageGroup::Step => "step",
        }
    }

    /// Read a requested dimension.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == raw)
    }
}

/// One report row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageRow {
    /// The group's value (a date, a model, a caller kind, a cache class, a SHA, or a
    /// project/department/programme/job/run/step id).
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
    /// `hits / calls`, `0.0` for an empty bucket.
    pub hit_ratio: f64,
    /// What the bucket's tokens would cost on the commercial equivalent, in USD. Derived at
    /// read time from `commercial_price` (SPEC_V2 D52) — tokens are stored, money is not.
    /// An unpriced model contributes zero here and is counted in `unpriced_calls`, never
    /// rendered as free.
    pub cost_usd: Decimal,
    /// Calls in the bucket whose model has a commercial price.
    pub priced_calls: i64,
    /// Calls in the bucket whose model has NO commercial price.
    pub unpriced_calls: i64,
    /// Wall time summed — the load side of the same window.
    pub latency_ms: i64,
    /// The commercial unit the bucket is priced in, when the bucket is uniformly non-token.
    /// `None` for a token bucket, a mixed bucket, or one with an unpriced call — a partial unit
    /// total would be a lie, so the report leaves it out rather than summing a subset.
    pub unit: Option<String>,
    /// The units summed for that kind, when [`Self::unit`] is `Some`.
    pub units: Option<Decimal>,
}

/// The share of calls served from cache; `0.0` when the bucket is empty.
pub fn hit_ratio(hits: i64, calls: i64) -> f64 {
    if calls <= 0 {
        0.0
    } else {
        hits as f64 / calls as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cache_class_is_exhaustive() {
        let hit = Usage {
            cache_read_input_tokens: 5,
            cache_creation_input_tokens: 0,
            ..Usage::default()
        };
        let write = Usage {
            cache_creation_input_tokens: 5,
            ..Usage::default()
        };
        let miss = Usage::default();
        assert_eq!(CacheStatus::of(Some(&hit)), CacheStatus::Hit);
        assert_eq!(CacheStatus::of(Some(&write)), CacheStatus::Write);
        assert_eq!(CacheStatus::of(Some(&miss)), CacheStatus::Miss);
        assert_eq!(CacheStatus::of(None), CacheStatus::Unknown);
        // read > 0 wins when both counters moved: the call was served from cache.
        let both = Usage {
            cache_read_input_tokens: 5,
            cache_creation_input_tokens: 5,
            ..Usage::default()
        };
        assert_eq!(CacheStatus::of(Some(&both)), CacheStatus::Hit);
    }

    #[test]
    fn a_response_without_usage_is_unknown_not_zero() {
        assert!(Usage::from_response(&json!({"model": "m"})).is_none());
        assert!(Usage::from_response(&json!({"usage": "nope"})).is_none());
        let zeroed = Usage::from_response(&json!({"usage": {}})).expect("an empty object is usage");
        assert_eq!(zeroed, Usage::default());
        assert_eq!(CacheStatus::of(Some(&zeroed)), CacheStatus::Miss);
    }

    #[test]
    fn usage_parses_the_router_fields_and_derives_a_missing_total() {
        let usage = Usage::from_response(&json!({"usage": {
            "prompt_tokens": 12,
            "completion_tokens": 3,
            "cache_read_input_tokens": 7,
            "cache_creation_input_tokens": 0
        }}))
        .expect("usage");
        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.completion_tokens, 3);
        assert_eq!(usage.total_tokens, 15);
        assert_eq!(usage.cache_read_input_tokens, 7);
    }

    #[test]
    fn model_identity_prefers_requested_and_reports_served() {
        let captured = capture(Some("alias"), &json!({"model": "served"}), None);
        assert_eq!(captured.model, "alias");
        assert_eq!(captured.served_model.as_deref(), Some("served"));
        let served_only = capture(None, &json!({"model": "served"}), None);
        assert_eq!(served_only.model, "served");
        let unnamed = capture(None, &json!({}), None);
        assert_eq!(unnamed.model, "unrouted");
        assert!(unnamed.served_model.is_none());
    }

    #[test]
    fn a_media_surface_is_the_pricing_identity() {
        let captured = capture(None, &json!({"provider": "kokoro"}), Some("tts"));
        assert_eq!(captured.model, "tts");
        assert!(
            captured.served_model.is_none(),
            "a media surface is not a routed model"
        );
        assert_eq!(captured.cache_status, CacheStatus::Unknown);
    }

    #[test]
    fn usage_units_round_trip() {
        for unit in UsageUnit::ALL {
            assert_eq!(UsageUnit::parse(unit.as_str()), Some(unit));
        }
        assert_eq!(UsageUnit::parse("token"), None);
    }

    #[test]
    fn hit_ratio_has_no_division_by_zero() {
        assert_eq!(hit_ratio(0, 0), 0.0);
        assert_eq!(hit_ratio(2, 4), 0.5);
    }

    #[test]
    fn the_prompt_hash_is_stable_across_key_order() {
        let a = json!({"messages": [{"role": "user", "content": "hi"}]});
        let b = json!({"messages": [{"content": "hi", "role": "user"}]});
        assert_eq!(prompt_sha256(&a), prompt_sha256(&b));
        assert_ne!(prompt_sha256(&a), prompt_sha256(&json!({"messages": []})));
    }
}
