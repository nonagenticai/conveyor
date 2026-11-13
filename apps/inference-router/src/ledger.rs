//! The inference ledger: one `inference_call` row per served request.
//!
//! The row is the audit trail the internal reads aggregate over, so its shape is contract, not
//! implementation. Two fields are easy to get subtly wrong and both are pinned by the oracle:
//!
//! * `request_sha256` is **not** a hash of the request body. It is the hash of a curated
//!   fingerprint (`request_fingerprint` in the reference) under canonical JSON — sorted keys,
//!   compact separators, UTF-8. `serde_json`'s default map is ordered, so the canonical form falls
//!   out of `to_string` and matches Python's `json.dumps(..., sort_keys=True, separators=(",",":"))`.
//! * the two penalties enter the fingerprint **only when set**. Adding them as nulls would change
//!   the hash for every request that never set them.
//!
//! `contract/golden/ledger_*.json` holds the oracle's own rows for the same requests; the tests
//! compare field for field rather than trusting this comment.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::contract::schemas::{ChatCompletionRequest, ChatCompletionResponse};

/// What the handler knows about the caller and the request, beyond the body itself.
#[derive(Debug, Clone, Default)]
pub struct CallContext {
    pub tenant_id: Option<String>,
    pub license_id: Option<String>,
    pub correlation_id: Option<String>,
    pub worker_name: Option<String>,
    /// The caller's stage label (`X-Stage`). Carried by the cost event; the ledger has no column.
    pub stage: Option<String>,
    pub trace_id: Option<String>,
    pub card_id: Option<String>,
    pub project_id: Option<String>,
    pub parent_execution_id: Option<String>,
    /// Written by the policy slice; `None` when no policy routed the request.
    pub policy_version: Option<i32>,
    pub llm_setup: Option<i16>,
    pub hop_index: Option<i16>,
    pub hop_reason: Option<String>,
    pub key_owner: Option<String>,
}

/// One row's values, already shaped for the INSERT.
#[derive(Debug, Clone)]
pub struct LedgerRow {
    pub tenant_id: Option<Uuid>,
    pub correlation_id: Option<String>,
    pub caller_worker: Option<String>,
    pub requested_model: String,
    pub served_provider: Option<String>,
    pub served_model: Option<String>,
    pub served_model_root: Option<String>,
    pub temperature: f64,
    pub seed: Option<i64>,
    pub max_tokens: Option<i32>,
    pub repetition_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub finish_reason: Option<String>,
    pub licence_id: Option<String>,
    pub licence_source: Option<&'static str>,
    pub request_sha256: String,
    pub response_sha256: Option<String>,
    pub latency_ms: i64,
    pub status: &'static str,
    pub request_text_sha256: Option<String>,
    pub traffic_source: &'static str,
    pub trace_id: Option<Uuid>,
    pub card_id: Option<String>,
    pub project_id: Option<Uuid>,
    pub parent_execution_id: Option<String>,
    pub response_json_valid: Option<bool>,
    pub policy_version: Option<i32>,
    pub llm_setup: Option<i16>,
    pub hop_index: Option<i16>,
    pub hop_reason: Option<String>,
    pub key_owner: Option<String>,
}

/// Parse an optional uuid the way the reference does: an unparseable value is absent, not an
/// error. A ledger write must never fail a customer's inference.
fn uuid_of(value: Option<&str>) -> Option<Uuid> {
    value.and_then(|raw| Uuid::parse_str(raw.trim()).ok())
}

/// Canonical JSON: sorted keys (serde_json's map is ordered) and compact separators.
fn canonical_json(value: &Value) -> String {
    serde_json::to_string(value).expect("a JSON value always serialises")
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// The part of a request that determines what the model samples (`request_fingerprint`).
pub fn request_fingerprint(request: &ChatCompletionRequest) -> Value {
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(|message| {
            // Pydantic's `exclude_none=True`: absent fields are omitted, not nulled.
            let mut object = Map::new();
            object.insert("content".to_owned(), message.content.clone());
            if let Some(reasoning) = &message.reasoning_content {
                object.insert(
                    "reasoning_content".to_owned(),
                    Value::String(reasoning.clone()),
                );
            }
            object.insert(
                "role".to_owned(),
                serde_json::to_value(message.role).expect("a role serialises"),
            );
            if let Some(tool_call_id) = &message.tool_call_id {
                object.insert(
                    "tool_call_id".to_owned(),
                    Value::String(tool_call_id.clone()),
                );
            }
            if let Some(tool_calls) = &message.tool_calls {
                object.insert(
                    "tool_calls".to_owned(),
                    serde_json::to_value(tool_calls).expect("tool calls serialise"),
                );
            }
            Value::Object(object)
        })
        .collect();

    let mut fingerprint = Map::new();
    fingerprint.insert("messages".to_owned(), Value::Array(messages));
    fingerprint.insert("temperature".to_owned(), number(request.temperature));
    fingerprint.insert("seed".to_owned(), optional_number(request.seed));
    fingerprint.insert("max_tokens".to_owned(), optional_number(request.max_tokens));
    fingerprint.insert(
        "response_format".to_owned(),
        match &request.response_format {
            Some(format) => serde_json::to_value(format).expect("a response format serialises"),
            None => Value::Null,
        },
    );
    fingerprint.insert(
        "tools".to_owned(),
        match &request.tools {
            Some(tools) => serde_json::to_value(tools).expect("tools serialise"),
            None => Value::Null,
        },
    );
    fingerprint.insert("tool_choice".to_owned(), request.tool_choice.clone());
    fingerprint.insert(
        "enable_reasoning".to_owned(),
        Value::Bool(request.enable_reasoning),
    );
    // ONLY when set — see the module comment.
    if let Some(value) = request.repetition_penalty {
        fingerprint.insert("repetition_penalty".to_owned(), number(value));
    }
    if let Some(value) = request.frequency_penalty {
        fingerprint.insert("frequency_penalty".to_owned(), number(value));
    }
    Value::Object(fingerprint)
}

/// The generated output of a response, minus per-call noise (`response_fingerprint`).
pub fn response_fingerprint(response: &ChatCompletionResponse) -> Value {
    let choices: Vec<Value> = response
        .choices
        .iter()
        .map(|choice| {
            let mut object = Map::new();
            object.insert("index".to_owned(), Value::from(choice.index));
            object.insert("content".to_owned(), choice.message.content.clone());
            object.insert(
                "reasoning_content".to_owned(),
                match &choice.message.reasoning_content {
                    Some(value) => Value::String(value.clone()),
                    None => Value::Null,
                },
            );
            object.insert(
                "finish_reason".to_owned(),
                Value::String(choice.finish_reason.clone()),
            );
            object.insert(
                "tool_calls".to_owned(),
                match &choice.message.tool_calls {
                    Some(calls) => serde_json::to_value(calls).expect("tool calls serialise"),
                    None => Value::Null,
                },
            );
            Value::Object(object)
        })
        .collect();
    Value::Array(choices)
}

fn number<T: Into<f64>>(value: T) -> Value {
    serde_json::Number::from_f64(value.into())
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn optional_number<T: Into<i64>>(value: Option<T>) -> Value {
    match value {
        Some(value) => Value::from(value.into()),
        None => Value::Null,
    }
}

pub fn request_sha256(request: &ChatCompletionRequest) -> String {
    sha256_hex(&canonical_json(&request_fingerprint(request)))
}

pub fn response_sha256(response: &ChatCompletionResponse) -> String {
    sha256_hex(&canonical_json(&response_fingerprint(response)))
}

/// `json_valid`: does choice 0's content parse as strict JSON? `None` when there is no text.
///
/// Strict means `NaN`/`Infinity` are refused (serde_json refuses them by default, as the
/// reference's `parse_constant` hook does). Past the parser's recursion limit the answer is
/// unknown rather than false, matching the reference's `RecursionError` branch.
pub fn response_json_valid(response: &ChatCompletionResponse) -> Option<bool> {
    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_str())?;
    match serde_json::from_str::<Value>(content) {
        Ok(_) => Some(true),
        Err(error) if error.to_string().contains("recursion limit") => None,
        Err(_) => Some(false),
    }
}

impl LedgerRow {
    /// Build the row for a SERVED request.
    pub fn served(
        context: &CallContext,
        request: &ChatCompletionRequest,
        response: &ChatCompletionResponse,
        request_text: Option<&str>,
        latency_ms: i64,
    ) -> Self {
        Self {
            tenant_id: uuid_of(context.tenant_id.as_deref()),
            correlation_id: context.correlation_id.clone(),
            caller_worker: context.worker_name.clone(),
            requested_model: request.model.clone(),
            served_provider: response.served_provider.clone(),
            served_model: response.served_model.clone(),
            served_model_root: response.served_model_root.clone(),
            temperature: request.temperature,
            seed: request.seed,
            max_tokens: request.max_tokens.map(|value| value as i32),
            repetition_penalty: request.repetition_penalty,
            frequency_penalty: request.frequency_penalty,
            finish_reason: response.choices.first().map(|c| c.finish_reason.clone()),
            licence_id: context.license_id.clone(),
            // A source with no id would claim the licence came from somewhere when none is known.
            licence_source: context.license_id.as_ref().map(|_| "header"),
            request_sha256: request_sha256(request),
            response_sha256: Some(response_sha256(response)),
            latency_ms,
            status: "ok",
            request_text_sha256: request_text.map(sha256_hex),
            traffic_source: "live",
            trace_id: uuid_of(context.trace_id.as_deref()),
            card_id: context.card_id.clone(),
            project_id: uuid_of(context.project_id.as_deref()),
            parent_execution_id: context.parent_execution_id.clone(),
            response_json_valid: response_json_valid(response),
            policy_version: context.policy_version,
            llm_setup: context.llm_setup,
            hop_index: context.hop_index,
            hop_reason: context.hop_reason.clone(),
            key_owner: context.key_owner.clone(),
        }
    }
}

/// Insert one row. Idempotent on `request_id`; a duplicate is not an error.
///
/// The caller logs and counts failures; this never propagates one to the response.
pub async fn insert(pool: &PgPool, row: &LedgerRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO inference_call (
            tenant_id, correlation_id, caller_worker, requested_model,
            served_provider, served_model, served_model_root,
            temperature, seed, max_tokens, repetition_penalty, frequency_penalty, finish_reason,
            licence_id, licence_source, request_sha256, response_sha256, latency_ms, status,
            request_text_sha256, traffic_source,
            trace_id, card_id, project_id, parent_execution_id,
            streamed, response_json_valid,
            policy_version, llm_setup, hop_index, hop_reason, key_owner
         ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7,
            $8, $9, $10, $11, $12, $13,
            $14, $15, $16, $17, $18, $19,
            $20, $21,
            $22, $23, $24, $25,
            false, $26,
            $27, $28, $29, $30, $31
         )
         ON CONFLICT (request_id) WHERE request_id IS NOT NULL DO NOTHING",
    )
    .bind(row.tenant_id)
    .bind(&row.correlation_id)
    .bind(&row.caller_worker)
    .bind(&row.requested_model)
    .bind(&row.served_provider)
    .bind(&row.served_model)
    .bind(&row.served_model_root)
    .bind(row.temperature)
    .bind(row.seed)
    .bind(row.max_tokens)
    .bind(row.repetition_penalty)
    .bind(row.frequency_penalty)
    .bind(&row.finish_reason)
    .bind(&row.licence_id)
    .bind(row.licence_source)
    .bind(&row.request_sha256)
    .bind(&row.response_sha256)
    .bind(row.latency_ms)
    .bind(row.status)
    .bind(&row.request_text_sha256)
    .bind(row.traffic_source)
    .bind(row.trace_id)
    .bind(&row.card_id)
    .bind(row.project_id)
    .bind(&row.parent_execution_id)
    .bind(row.response_json_valid)
    .bind(row.policy_version)
    .bind(row.llm_setup)
    .bind(row.hop_index)
    .bind(&row.hop_reason)
    .bind(&row.key_owner)
    .execute(pool)
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schemas::{ChatMessage, Role};
    use serde_json::json;

    fn request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            enable_reasoning: false,
            frequency_penalty: None,
            max_tokens: Some(8),
            messages: vec![ChatMessage {
                content: json!("attributed"),
                reasoning_content: None,
                role: Role::User,
                tool_call_id: None,
                tool_calls: None,
            }],
            model: "stub-model".to_owned(),
            repetition_penalty: None,
            response_format: None,
            seed: None,
            stream: false,
            temperature: 0.1,
            tool_choice: Value::Null,
            tools: None,
        }
    }

    #[test]
    fn the_fingerprint_is_canonical_and_omits_null_messages_fields() {
        let value = request_fingerprint(&request());
        let text = canonical_json(&value);
        // Sorted keys, compact separators, and nulls for the fields the reference always keeps.
        assert!(text.starts_with(r#"{"enable_reasoning":false,"max_tokens":8,"messages":[{"#));
        assert!(text.contains(r#"{"content":"attributed","role":"user"}"#));
        assert!(text.contains(r#""response_format":null"#));
        assert!(text.contains(r#""tool_choice":null"#));
        // The penalties are ABSENT, not null.
        assert!(!text.contains("repetition_penalty"));
        assert!(!text.contains("frequency_penalty"));
    }

    #[test]
    fn the_fingerprint_hash_matches_the_oracle() {
        // `contract/golden/ledger_completion_attributed.json`, captured from the pinned image.
        assert_eq!(
            request_sha256(&request()),
            "86e40348206a3f554dd6bf74ed35eef822db7ffe9e84f731b33da1dfd6f45813"
        );
    }
}
