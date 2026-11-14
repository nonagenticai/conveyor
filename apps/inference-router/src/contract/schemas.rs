//! The wire types of the captured contract (`contract/openapi.lock.json`, 35 schemas).
//!
//! These are the *only* types that cross the HTTP boundary. Field names, requiredness, defaults,
//! bounds and formats are the capture's, not preferences: `--check-lock` compares the generated
//! document against the lock, so a shape change here is a contract change there.
//!
//! Two patterns recur because Pydantic and serde express the same thing differently:
//!
//! - a **defaulted** field is non-required (Pydantic says "has a default"; utoipa says "has a
//!   serde default"), so `#[serde(default)]` is what makes the document match;
//! - a field that is **required but nullable** needs `#[schema(required = true)]`, because serde's
//!   `Option` alone only makes it non-required.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{AdditionalProperties, AnyOf, Array, Object, Schema, SchemaType};

/// An object with arbitrary keys — Pydantic's `dict[str, Any]` response shapes.
///
/// A newtype rather than an alias: `inline(AnyObject)` needs `ComposeSchema`, which
/// `BTreeMap<String, serde_json::Value>` cannot satisfy (`Value` is not a `ComposeSchema`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnyObject(pub BTreeMap<String, serde_json::Value>);

// `ComposeSchema` is the only impl needed: utoipa provides `PartialSchema` from it (a blanket
// impl), and `inline(AnyObject)` needs exactly that.
impl utoipa::__dev::ComposeSchema for AnyObject {
    fn compose(_generics: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        Object::builder()
            .additional_properties(Some(AdditionalProperties::FreeForm(true)))
            .build()
            .into()
    }
}

impl utoipa::ToSchema for AnyObject {}

// ── schema helpers for shapes utoipa has no Rust form for ──────────────────────────────────────

fn any_object() -> Object {
    Object::builder()
        .additional_properties(Some(AdditionalProperties::FreeForm(true)))
        .build()
}

/// `ChatMessage.content`: text, a list of content parts, or null.
fn chat_content_schema() -> RefOr<Schema> {
    AnyOf::builder()
        .item(
            Object::builder().schema_type(SchemaType::Type(utoipa::openapi::schema::Type::String)),
        )
        .item(Array::builder().items(any_object()))
        .item(Object::builder().schema_type(SchemaType::Type(utoipa::openapi::schema::Type::Null)))
        .into()
}

/// `tool_choice`: a control word, a forced-function object, or null.
fn tool_choice_schema() -> RefOr<Schema> {
    AnyOf::builder()
        .item(
            Object::builder().schema_type(SchemaType::Type(utoipa::openapi::schema::Type::String)),
        )
        .item(any_object())
        .item(Object::builder().schema_type(SchemaType::Type(utoipa::openapi::schema::Type::Null)))
        .into()
}

/// `ValidationError.loc`: a list of strings and integers (a JSON pointer path).
fn loc_schema() -> RefOr<Schema> {
    let item: RefOr<Schema> = AnyOf::builder()
        .item(
            Object::builder().schema_type(SchemaType::Type(utoipa::openapi::schema::Type::String)),
        )
        .item(
            Object::builder().schema_type(SchemaType::Type(utoipa::openapi::schema::Type::Integer)),
        )
        .into();
    Array::builder().items(item).into()
}

// ── completions ────────────────────────────────────────────────────────────────────────────────

/// The role of a message; the capture makes it an enum, so an unknown role is a 422.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

fn default_model() -> String {
    "gemma".to_owned()
}

fn default_temperature() -> f64 {
    0.1
}

/// `POST /api/chat/completions` request — OpenAI-compatible, narrowed where the router validates.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatCompletionRequest {
    #[serde(default)]
    #[schema(default = false)]
    pub enable_reasoning: bool,
    #[serde(default)]
    #[schema(minimum = -2.0, maximum = 2.0)]
    pub frequency_penalty: Option<f64>,
    /// Also accepted as `max_completion_tokens` (the alias pydantic-ai and openai-python send).
    #[serde(default, alias = "max_completion_tokens")]
    #[schema(minimum = 1, maximum = 16384)]
    pub max_tokens: Option<u32>,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_model")]
    #[schema(default = "gemma")]
    pub model: String,
    #[serde(default)]
    #[schema(exclusive_minimum = 0.0, maximum = 10.0)]
    pub repetition_penalty: Option<f64>,
    #[serde(default)]
    pub response_format: Option<ResponseFormat>,
    #[serde(default)]
    #[schema(minimum = -9223372036854775808i64, maximum = 9223372036854775807i64)]
    pub seed: Option<i64>,
    #[serde(default)]
    #[schema(default = false)]
    pub stream: bool,
    #[serde(default = "default_temperature")]
    #[schema(default = 0.1, minimum = 0.0, maximum = 2.0)]
    pub temperature: f64,
    #[serde(default)]
    #[schema(schema_with = tool_choice_schema)]
    pub tool_choice: serde_json::Value,
    #[serde(default)]
    pub tools: Option<Vec<ToolSpec>>,
}

/// One message in the prompt or the model's answer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatMessage {
    #[schema(schema_with = chat_content_schema)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[schema(inline)]
    pub role: Role,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// One generated choice.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Choice {
    #[serde(default = "default_finish_reason")]
    #[schema(default = "stop")]
    pub finish_reason: String,
    #[serde(default)]
    #[schema(default = 0)]
    pub index: i64,
    pub message: ChatMessage,
}

fn default_finish_reason() -> String {
    "stop".to_owned()
}

/// `POST /api/chat/completions` response.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub id: String,
    pub model: String,
    #[serde(default = "default_completion_object")]
    #[schema(default = "chat.completion")]
    pub object: String,
    #[serde(default)]
    pub provider_request_id: Option<String>,
    #[serde(default)]
    pub served_model: Option<String>,
    #[serde(default)]
    pub served_model_root: Option<String>,
    #[serde(default)]
    pub served_provider: Option<String>,
    #[serde(default)]
    pub usage: Usage,
}

fn default_completion_object() -> String {
    "chat.completion".to_owned()
}

/// Token usage, including the classes the estate's metering needs beyond OpenAI's three.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct Usage {
    #[serde(default)]
    #[schema(default = 0)]
    pub cache_creation_input_tokens: i64,
    #[serde(default)]
    #[schema(default = 0)]
    pub cache_read_input_tokens: i64,
    #[serde(default)]
    #[schema(default = 0)]
    pub completion_tokens: i64,
    #[serde(default)]
    #[schema(default = 0)]
    pub prompt_tokens: i64,
    #[serde(default)]
    #[schema(default = 0)]
    pub total_tokens: i64,
    #[serde(default)]
    pub usage_extra: Option<BTreeMap<String, i64>>,
}

/// A tool call the model emitted.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolCall {
    pub function: ToolCallFunction,
    pub id: String,
    #[serde(default = "default_function")]
    #[schema(default = "function")]
    pub r#type: String,
}

fn default_function() -> String {
    "function".to_owned()
}

/// The function a tool call names and its serialized arguments.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolCallFunction {
    pub arguments: String,
    pub name: String,
}

/// A tool declaration (OpenAI shape).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolSpec {
    pub function: ToolFunctionSpec,
    #[serde(default = "default_function")]
    #[schema(default = "function")]
    pub r#type: String,
}

/// The declared function, its description and its JSON Schema parameters.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolFunctionSpec {
    #[serde(default)]
    pub description: Option<String>,
    pub name: String,
    pub parameters: BTreeMap<String, serde_json::Value>,
}

/// The structured-output selector.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResponseFormat {
    #[serde(default)]
    pub json_schema: Option<JsonSchemaSpec>,
    #[serde(default = "default_response_format")]
    #[schema(inline)]
    pub r#type: ResponseFormatType,
}

fn default_response_format() -> ResponseFormatType {
    ResponseFormatType::Text
}

/// The three structured-output modes the capture allows.
///
/// The schema is written by hand because the capture inlines it *with* its default, and utoipa's
/// `inline` + `default` combination emits an `allOf` whose second branch (`type: object`) is not
/// the shape any client can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormatType {
    Text,
    JsonObject,
    JsonSchema,
}

impl utoipa::PartialSchema for ResponseFormatType {
    fn schema() -> RefOr<Schema> {
        Object::builder()
            .schema_type(SchemaType::Type(utoipa::openapi::schema::Type::String))
            .enum_values(Some(["text", "json_object", "json_schema"]))
            .default(Some(serde_json::json!("text")))
            .build()
            .into()
    }
}

impl utoipa::ToSchema for ResponseFormatType {}

/// A named JSON Schema for guided output.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JsonSchemaSpec {
    #[serde(default)]
    pub name: Option<String>,
    pub schema: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    #[schema(default = false)]
    pub strict: bool,
}

// ── models and providers ──────────────────────────────────────────────────────────────────────

fn default_list_object() -> String {
    "list".to_owned()
}

fn default_model_object() -> String {
    "model".to_owned()
}

/// `GET /api/models` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelListResponse {
    pub data: Vec<ModelObject>,
    #[serde(default = "default_list_object")]
    #[schema(default = "list")]
    pub object: String,
}

/// One routable model id, pinned to the provider(s) that serve it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelObject {
    #[serde(default)]
    #[schema(default = false)]
    pub has_grammar: bool,
    #[serde(default)]
    #[schema(default = false)]
    pub has_json_schema: bool,
    pub id: String,
    #[serde(default = "default_model_object")]
    #[schema(default = "model")]
    pub object: String,
    #[serde(default = "default_owned_by")]
    #[schema(default = "inference-router")]
    pub owned_by: String,
    #[serde(default)]
    pub provider_chain: Option<Vec<String>>,
}

fn default_owned_by() -> String {
    "inference-router".to_owned()
}

/// One catalog provider as a UI sees it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderObject {
    pub cost_input_per_1m: f64,
    pub cost_output_per_1m: f64,
    pub default_model: String,
    pub name: String,
    pub protocol: String,
}

/// `GET /api/providers` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderListResponse {
    pub data: Vec<ProviderObject>,
    pub default_chain: Vec<String>,
    #[serde(default = "default_list_object")]
    #[schema(default = "list")]
    pub object: String,
}

/// One model id a provider serves.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderModelObject {
    pub id: String,
    #[serde(default = "default_model_object")]
    #[schema(default = "model")]
    pub object: String,
}

/// `GET /api/providers/{name}/models` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderModelListResponse {
    pub data: Vec<ProviderModelObject>,
    #[serde(default = "default_list_object")]
    #[schema(default = "list")]
    pub object: String,
    pub provider: String,
}

// ── whoami ────────────────────────────────────────────────────────────────────────────────────

/// `GET /api/whoami` — what the licence server says about the caller's token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WhoamiResponse {
    pub active: bool,
    #[schema(required = true)]
    pub app: Option<String>,
    #[schema(required = true)]
    pub expires_at: Option<String>,
    pub license_id: String,
    pub tenant_id: String,
}

// ── replay invocation log ────────────────────────────────────────────────────────────────────

/// `GET /api/replay/invocation-log` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvocationLogListResponse {
    pub count: i64,
    pub rows: Vec<InvocationLogRow>,
}

/// One replay invocation-log row.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvocationLogRow {
    #[serde(default)]
    pub actual: serde_json::Value,
    #[schema(format = "uuid")]
    pub correlation_id: String,
    #[serde(default)]
    pub expected: serde_json::Value,
    #[schema(format = "uuid")]
    pub id: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[schema(required = true)]
    pub judge_score: Option<f64>,
    pub latency_ms: i64,
    #[schema(format = "date-time")]
    pub started_at: String,
    #[schema(required = true)]
    pub validation_passed: Option<bool>,
    #[schema(required = true)]
    pub version: Option<String>,
    pub worker_name: String,
}

/// `PATCH /api/replay/invocation-log/*` request — only the supplied fields change.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct PatchInvocationLogRequest {
    #[serde(default)]
    pub expected: serde_json::Value,
    #[serde(default)]
    #[schema(minimum = 0.0, maximum = 1.0)]
    pub judge_score: Option<f64>,
    #[serde(default)]
    pub validation_passed: Option<bool>,
}

/// `PATCH /api/replay/invocation-log/*` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PatchInvocationLogResponse {
    #[schema(format = "uuid")]
    pub id: String,
    #[schema(required = true)]
    pub judge_score: Option<f64>,
    #[schema(required = true)]
    pub validation_passed: Option<bool>,
}

// ── prompt canary ────────────────────────────────────────────────────────────────────────────

/// The canary window for one worker.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromptCanaryState {
    pub active_version: i64,
    #[schema(required = true, format = "date-time")]
    pub canary_ends_at: Option<String>,
    pub canary_percent: i64,
    #[schema(required = true, format = "date-time")]
    pub canary_started_at: Option<String>,
    #[schema(required = true)]
    pub canary_version: Option<i64>,
    pub rollback_thresholds: BTreeMap<String, f64>,
    pub worker_name: String,
}

/// `POST /api/canary/prompt/configure` request.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurePromptCanaryRequest {
    #[schema(minimum = 1)]
    pub active_version: i64,
    #[schema(format = "date-time")]
    pub canary_ends_at: String,
    #[schema(minimum = 0, maximum = 100)]
    pub canary_percent: i64,
    #[schema(format = "date-time")]
    pub canary_started_at: String,
    #[schema(minimum = 1)]
    pub canary_version: i64,
    pub rollback_thresholds: BTreeMap<String, f64>,
    #[schema(min_length = 1, max_length = 255)]
    pub worker_name: String,
}

/// `POST /api/canary/prompt/configure` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigurePromptCanaryResponse {
    pub active_version: i64,
    #[schema(format = "date-time")]
    pub canary_ends_at: String,
    pub canary_percent: i64,
    #[schema(format = "date-time")]
    pub canary_started_at: String,
    pub canary_version: i64,
    pub rollback_thresholds: BTreeMap<String, f64>,
    pub worker_name: String,
}

/// `POST /api/canary/promote-prompt` request.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromotePromptRequest {
    #[schema(min_length = 1, max_length = 255)]
    pub worker_name: String,
}

/// `POST /api/canary/promote-prompt` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromotePromptResponse {
    pub active_version: i64,
    pub worker_name: String,
}

/// `POST /api/canary/rollback-prompt` request.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RollbackPromptRequest {
    #[schema(min_length = 1, max_length = 255)]
    pub worker_name: String,
}

/// `POST /api/canary/rollback-prompt` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RollbackPromptResponse {
    pub active_version: i64,
    pub worker_name: String,
}

// ── validation errors ────────────────────────────────────────────────────────────────────────

/// FastAPI's 422 envelope. The names are FastAPI's: clients already parse them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HTTPValidationError {
    #[serde(default)]
    pub detail: Vec<ValidationError>,
}

/// One 422 detail row.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidationError {
    #[serde(default)]
    pub ctx: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub input: serde_json::Value,
    #[schema(schema_with = loc_schema)]
    pub loc: Vec<serde_json::Value>,
    pub msg: String,
    pub r#type: String,
}

// ── admin ────────────────────────────────────────────────────────────────────────────────────

/// The activate-policy body. The name is FastAPI's generated `Body_*` component name; keeping it
/// verbatim is what lets the lock comparison be a name-for-name comparison.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Body_activate_inference_policy_api_admin_tenants__tenant_id__inference_policy_activate_post
{
    #[schema(minimum = 1)]
    pub version: i64,
}
