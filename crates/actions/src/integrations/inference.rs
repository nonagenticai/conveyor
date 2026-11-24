#![forbid(unsafe_code)]
//! `completions.create` — the one external action the copilot's turn runner calls (S10 §5.5).
//!
//! **Declared, not implemented.** The spec names the service, the method and the path; the engine's
//! `invoke_external` owns the 60 s timeout, the process-local breaker, the idempotency claim and the
//! audit row. A `reqwest::` call in this file would bypass all four and fails
//! `crates/api/tests/externals_declared.rs` — that test is the reason this module has no client.
//!
//! The base URL is configuration only: `CONVEYOR_EXTERNAL_INFERENCE_URL`. Unset ⇒
//! `ActionError::unconfigured`, a visible refusal, never a guessable default. The path is the router's
//! own `POST /api/chat/completions` (`inference-router/src/routers/completions.py:68,712`); the
//! `/v1/chat/completions` alias exists but the service's native prefix is the one declared here.
//!
//! **D14:** the conveyor does not absorb inference-router. On outage, timeout or an open breaker the
//! turn ends `failed` with an RFC 9457 problem and **zero** tool invocations — the model never emits a
//! fabricated tool call (S10-D5).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, Scope,
    Tenancy,
};

/// The service key: `CONVEYOR_EXTERNAL_INFERENCE_URL`.
pub const SERVICE: &str = "inference";

/// The per-tenant sealed secret carrying this tenant's inference-router token (BYO).
///
/// There is no platform token. A tenant registers its own with `secrets.create`/`secrets.rotate`
/// under this name — or an operator fills it from the licences API — and the engine sends it as
/// the bearer. `auth_optional` keeps a tenant without one runnable: the router decides, and a
/// refusal is its own 401, recorded as a failed call, never a silent anonymous success.
pub const INFERENCE_TOKEN_SECRET: &str = "inference.token";

/// One message in the prompt or in the model's answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChatMessage {
    /// `system`, `user`, `assistant` or `tool`.
    pub role: String,
    /// The message text; `""` for an assistant turn that carries only tool calls.
    ///
    /// OpenAI-compatible clients send `null` for that assistant turn, and some omit the field
    /// entirely. Both mean the empty string here: the router accepts null, but a `String` field
    /// would reject the replay shape this action exists to pass through.
    #[serde(default, deserialize_with = "null_as_empty")]
    #[schemars(with = "Option<String>")]
    pub content: String,
    /// The model's tool calls, passed through unchanged (OpenAI shape).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    /// The call this message answers, for `role = "tool"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// An optional tool name, passed through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `completions.create`'s request: the router's OpenAI-compatible body, narrowed to what a turn uses.
///
/// Deliberately not `deny_unknown_fields`: the router accepts more parameters than a copilot turn
/// sends, and a schema that refused them would make the action stricter than the service it declares.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompletionIn {
    /// The model or schema id to route to; the router applies its own policy when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The conversation, oldest first.
    pub messages: Vec<ChatMessage>,
    /// Tool declarations (OpenAI shape), passed through unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    /// `auto`, `none` or a named tool; passed through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// The response cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// A structured-output request, passed through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,
}

/// A message `content` that is `null` (or absent) is the empty string; a string is itself.
fn null_as_empty<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn completion_input() -> Schema {
    schemars::schema_for!(CompletionIn)
}

/// The router's answer, passed through unchanged.
///
/// The conveyor does not re-model another service's document: it validates nothing, rewrites nothing
/// and stores nothing. A typed copy would drift the day the router adds a field.
fn passthrough_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The inference router's document, passed through unchanged.",
        "additionalProperties": true
    })
}

/// The declared spec shared by the two model calls: the copilot's `completions.create` (S10) and the
/// workflow engine's `inference.chat` (S13 step 8). They declare the same router path because it is
/// the same service call; what differs is the caller and therefore the ledger's `caller_kind` —
/// `copilot` for a turn, `agent_run` for the capped loop — and that is carried by the caller's
/// `UsageContext`, not by the action.
fn chat_spec(name: &'static str, timeout_ms: u32) -> ActionSpec {
    ActionSpec {
        name,
        version: 1,
        kind: ActionKind::External,
        input: completion_input,
        output: passthrough_output,
        // The scope the turn runner needs to ask the model anything.
        scopes: &[Scope::ModelWrite],
        // A replay with the same key returns the first body — one turn, one charge.
        idempotent: true,
        // The declaration is the deadline the engine grants a caller (`handlers.rs` passes
        // `spec.timeout_ms` through verbatim); there is no second caller input above it.
        // S10 §5.5 accepts 60 s for the copilot's call; the workflow loop keeps the agent
        // row's `timeout_secs` 1..=300 ceiling, which a shorter bound would make unreachable.
        timeout_ms,
        // An outbound, billable POST: declaring it `read` told every consumer it was free and
        // side-effect-free. When S16 lands it also writes the `inference_call` ledger row.
        effect: Effect::Write,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: true,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/api/chat/completions",
            auth: Some(INFERENCE_TOKEN_SECRET),
            auth_optional: true,
            upload: None,
        },
    }
}

/// The copilot's model call (S10 §5.5).
fn completions_create() -> ActionSpec {
    // S10 §5.5's accepted bound. `models:invoke` in that plan is not a scope the registry
    // admits; `model:write` is the admitted scope for this action, so the spec keeps it.
    chat_spec("completions.create", 60_000)
}

/// The workflow engine's model call (S13 step 8). A workflow step reaches a model only through the
/// capped `agent.run` loop, which invokes this action — `validate_publish` refuses a node that names
/// it directly (`crates/workflow/src/definition.rs`, `DirectModelCall`).
fn inference_chat() -> ActionSpec {
    // The agent row's own `timeout_secs` cap (1..=300); S13's loop reaches it through this
    // declaration, so the bound stays at the cap rather than S10's 60 s turn bound.
    chat_spec("inference.chat", 300_000)
}

inventory::submit! { ActionRegistration::new(completions_create) }
inventory::submit! { ActionRegistration::new(inference_chat) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::HandlerRef;

    #[test]
    fn a_null_or_absent_message_content_is_the_empty_string() {
        // The assistant turn that carries only tool calls: OpenAI-compatible callers send null,
        // some omit the field. Both are the empty string, never a rejected request.
        let null: ChatMessage = serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "c1",
                "type": "function",
                "function": {"name": "x", "arguments": "{}"}
            }]
        }))
        .expect("the OpenAI replay shape is accepted");
        assert_eq!(null.content, "");
        let absent: ChatMessage = serde_json::from_value(serde_json::json!({"role": "assistant"}))
            .expect("an absent content is empty");
        assert_eq!(absent.content, "");
        let text: ChatMessage =
            serde_json::from_value(serde_json::json!({"role": "user", "content": "hi"})).unwrap();
        assert_eq!(text.content, "hi", "a string passes through unchanged");
    }

    #[test]
    fn the_spec_is_a_declared_external_write_with_the_router_timeout() {
        let spec = completions_create();
        assert_eq!(spec.name, "completions.create");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Write);
        assert_eq!(spec.timeout_ms, 60_000, "S10 §5.5's accepted bound");
        // `models:invoke` is not a `Scope` variant; `model:write` is the scope the registry
        // admits, and the plan does not introduce a new scope vocabulary.
        assert_eq!(spec.scope_names(), vec!["model:write"]);
        assert!(spec.idempotent);
        match spec.handler {
            HandlerRef::External {
                service,
                method,
                path,
                ..
            } => {
                assert_eq!(service, "inference");
                assert_eq!(method, "POST");
                assert_eq!(path, "/api/chat/completions");
            }
            HandlerRef::Internal(_)
            | HandlerRef::Workflow
            | HandlerRef::Broker
            | HandlerRef::ToolDestination => {
                panic!("completions.create must be a declared external action")
            }
        }
    }

    #[test]
    fn a_caller_cannot_exceed_the_accepted_sixty_second_bound() {
        // The engine grants the caller exactly `spec.timeout_ms` (`handlers.rs` copies it, and
        // there is no caller-supplied deadline above it). The canonical projection is what every
        // channel publishes, so a consumer cannot advertise or request a longer turn than S10
        // §5.5 accepts.
        let spec = completions_create();
        assert_eq!(spec.timeout_ms, 60_000);
        let canonical = spec.canonical();
        assert_eq!(canonical["timeout_ms"], 60_000);
        assert_eq!(canonical["scopes"], serde_json::json!(["model:write"]));
    }

    #[test]
    fn the_workflow_model_call_is_the_same_contract_as_the_copilots() {
        let spec = inference_chat();
        assert_eq!(spec.name, "inference.chat");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Write);
        assert_eq!(spec.timeout_ms, 300_000);
        assert_eq!(spec.scope_names(), vec!["model:write"]);
        assert!(spec.idempotent);
        assert!(
            spec.captures_usage,
            "S16's ledger must record the loop's calls"
        );
        match spec.handler {
            HandlerRef::External {
                service,
                method,
                path,
                ..
            } => {
                assert_eq!(service, SERVICE);
                assert_eq!(method, "POST");
                assert_eq!(path, "/api/chat/completions");
            }
            HandlerRef::Internal(_)
            | HandlerRef::Workflow
            | HandlerRef::Broker
            | HandlerRef::ToolDestination => {
                panic!("inference.chat must be a declared external action")
            }
        }
        assert!(!spec.task_capable);
    }

    #[test]
    fn the_input_schema_requires_messages_and_allows_passthrough_fields() {
        let schema = spec_schema();
        let required = schema.get("required").and_then(Value::as_array);
        assert!(
            required.is_some_and(|names| names.iter().any(|name| name == "messages")),
            "messages must be required: {schema}"
        );
    }

    #[test]
    fn a_null_or_absent_replay_content_is_accepted_and_read_as_empty() {
        let schema = spec_schema();
        let input = serde_json::json!({
            "messages": [
                {"role": "user", "content": "What is the weather?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": "call_1", "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"Amsterdam\"}"}
                }]},
                {"role": "tool", "tool_call_id": "call_1", "content": "{\"temperature_c\": 12}"}
            ]
        });
        assert!(
            crate::spec::validate_input(&schema, &input).is_empty(),
            "the schema must accept the OpenAI replay shape it documents: {schema}"
        );
        let parsed: CompletionIn = serde_json::from_value(input).expect("null content parses");
        assert_eq!(parsed.messages[1].content, "", "null is the empty string");

        let absent = serde_json::json!({"messages": [{"role": "assistant"}]});
        assert!(
            crate::spec::validate_input(&schema, &absent).is_empty(),
            "an absent content is the empty string: {schema}"
        );
        let parsed: CompletionIn = serde_json::from_value(absent).expect("absent content parses");
        assert_eq!(parsed.messages[0].content, "");
    }

    fn spec_schema() -> Value {
        serde_json::to_value((completions_create().input)()).expect("a schema serializes")
    }
}
