#![forbid(unsafe_code)]
//! `tools.call` — the one action a job uses to reach a tool (SPEC_V2 §11.3; T13).
//!
//! This module declares the action; [`crate::broker`] enforces it. The split is deliberate: the
//! declaration is static (the manifest, the OpenAPI operation and the MCP tool are projections of
//! it), while the tool's effect class, schema and pinned version are **rows** written by the
//! estate import, so the broker — not this spec — decides what a call may do.
//!
//! Why the spec's own effect is `read` rather than the worst tool it could dispatch: a static
//! `destructive` would demand an approval for every read tool, and a static `write` would floor
//! every binding at write. §11.4's rule is that the mapping is data on the tool version; the
//! broker reads it, compares it to the binding's ceiling (`403 tool_effect_forbidden`) and applies
//! the destructive gate before the destination is called.
//!
//! The input is closed: `run_id`, `tool` (`<toolkit>.<operation>`), the tool's own `input`
//! document and an optional `step_id`. A caller never names a destination, a URL or a credential —
//! the binding resolves all three (§11.3 steps 1, 4 and 5).

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, Scope,
    Tenancy,
};

/// The one action a job uses to reach a tool (SPEC_V2 §11.3). It is a dispatch mechanism, not a
/// model-facing tool: an agent's bound tools replace it in the projected surface (#556).
pub const TOOLS_CALL: &str = "tools.call";

/// The broker's input.
///
/// Closed: a caller names a run, a tool and the tool's input. A `url`, a `destination` or any
/// other member is `422 unknown_field` before the broker resolves anything (§11.3 step 5).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ToolCallIn {
    /// The run the call is made for; its binding is what authorises the tool.
    run_id: uuid::Uuid,
    /// The canonical tool name, `<toolkit>.<operation>` (for example `coding.jobs.create`).
    tool: String,
    /// The tool's own input document, validated against the pinned version's closed schema.
    input: serde_json::Value,
    /// The run step the call belongs to, when the caller has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    step_id: Option<uuid::Uuid>,
}

fn tool_call_input() -> Schema {
    schemars::schema_for!(ToolCallIn)
}

/// The broker's answer: the destination's document, redacted and passed through.
fn broker_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The tool's result envelope `{status, data, checks[], next[]}`, redacted \
                        and size-capped by the broker.",
        "additionalProperties": true
    })
}

/// `tools.call` — resolve, check, validate, call and ledger one tool call (§11.3).
fn tools_call() -> ActionSpec {
    ActionSpec {
        name: TOOLS_CALL,
        version: 1,
        kind: ActionKind::Internal,
        input: tool_call_input,
        output: broker_output,
        // A tool call is part of a run's execution: whoever may write runs may make one. The
        // real grant is the tool binding, resolved per run and never widened by this scope.
        scopes: &[Scope::RunsWrite],
        // A read tool is safe to repeat; a write tool's own idempotency decides, and the caller
        // can supply an `Idempotency-Key` which the engine replays.
        idempotent: false,
        // A tool version's timeout is 1..=300 s (the schema's CHECK); the dispatcher waits for
        // the longest of them and the destination's own timeout is the narrower one.
        timeout_ms: 300_000,
        // The dispatcher's effect. The resolved tool's effect is data and is enforced by the
        // broker before this action ever reaches a destination (see the module docs).
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: false,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::Broker,
    }
}

inventory::submit! { ActionRegistration::new(tools_call) }

/// `tools.resume` — continue a long operation that exceeded the tool's timeout (§11.3).
pub const TOOLS_RESUME: &str = "tools.resume";

/// The status tool's input: the token the pending call returned and the original input.
#[derive(Clone, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ResumeIn {
    /// The `resume_token` the broker minted on the pending call.
    pub resume_token: String,
    /// The original tool input; its hash must match the pending row's.
    pub input: serde_json::Value,
}

fn resume_input() -> Schema {
    schemars::schema_for!(ResumeIn)
}

fn tools_resume() -> ActionSpec {
    ActionSpec {
        name: TOOLS_RESUME,
        version: 1,
        kind: ActionKind::Internal,
        input: resume_input,
        output: broker_output,
        // The resume continues a run's call; the run's binding and the pending row are the grant.
        scopes: &[Scope::RunsWrite],
        idempotent: false,
        timeout_ms: 300_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: false,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::Broker,
    }
}

inventory::submit! { ActionRegistration::new(tools_resume) }
