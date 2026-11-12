#![forbid(unsafe_code)]
//! The built-in prompt texts (SPEC D27).
//!
//! They are **seed material**, not a runtime registry: once seeded, the catalogue row is the
//! truth, and changing any text here means publishing a new `<name>@<version>` — never editing a
//! version in place, because `inference_call.prompt_ref`/`prompt_version` name the version that
//! produced a past call.

/// `copilot_system@1` — the copilot's one system message.
pub const COPILOT_SYSTEM_V1: &str = include_str!("copilot_system_v1.md");

/// `prompt_audit@1` — the prompt audit's own instruction. Changing it means `prompt_audit@2`.
pub const PROMPT_AUDIT_V1: &str = include_str!("prompt_audit_v1.md");

/// `agent@1` — the generic executor instruction for a capability agent.
pub const AGENT_V1: &str = include_str!("agent_v1.md");
