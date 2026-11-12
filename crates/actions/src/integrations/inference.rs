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
    #[serde(default)]
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
fn chat_spec(name: &'static str) -> ActionSpec {
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
        timeout_ms: 60_000,
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
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/api/chat/completions",
            auth: Some(INFERENCE_TOKEN_SECRET),
            auth_optional: true,
        },
    }
}

/// The copilot's model call (S10 §5.5).
fn completions_create() -> ActionSpec {
    chat_spec("completions.create")
}

/// The workflow engine's model call (S13 step 8). A workflow step reaches a model only through the
/// capped `agent.run` loop, which invokes this action — `validate_publish` refuses a node that names
/// it directly (`crates/workflow/src/definition.rs`, `DirectModelCall`).
fn inference_chat() -> ActionSpec {
    chat_spec("inference.chat")
}

inventory::submit! { ActionRegistration::new(completions_create) }
inventory::submit! { ActionRegistration::new(inference_chat) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::HandlerRef;

    #[test]
    fn the_spec_is_a_declared_external_write_with_the_router_timeout() {
        let spec = completions_create();
        assert_eq!(spec.name, "completions.create");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Write);
        assert_eq!(spec.timeout_ms, 60_000);
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
            HandlerRef::Internal(_) | HandlerRef::Workflow => {
                panic!("completions.create must be a declared external action")
            }
        }
    }

    #[test]
    fn the_workflow_model_call_is_the_same_contract_as_the_copilots() {
        let spec = inference_chat();
        assert_eq!(spec.name, "inference.chat");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Write);
        assert_eq!(spec.timeout_ms, 60_000);
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
            HandlerRef::Internal(_) | HandlerRef::Workflow => {
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

    fn spec_schema() -> Value {
        serde_json::to_value((completions_create().input)()).expect("a schema serializes")
    }
}
