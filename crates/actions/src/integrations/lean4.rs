#![forbid(unsafe_code)]
//! The Lean prover's read actions (T14, SPEC_V2 §11.2/§11.9; ADR 0006 §4).
//!
//! The prover (`apps/lean4-prover`) serves three POST routes — `/api/check`, `/api/elaborate`
//! and `/api/prove` — behind its licence layer. The operations are typechecks: the prover reads
//! a source document and answers diagnostics, never a state change, so every action is a read
//! and idempotent.
//!
//! The licence bearer is the tenant's sealed [`LEAN4_TOKEN_SECRET`], the same shape the
//! inference client uses (`inference.token`): a missing secret is a typed `503` before the
//! call, never an anonymous request that the service answers `401`. The health path is public,
//! so the toolkit's probe does not need the credential.
//!
//! Every input is the app's own request body, closed: an unknown field is a `422` from the
//! engine's schema validation before any call reaches the service. The app validates the same
//! constraints again (source length, a 1..=120 second timeout), so the declaration and the
//! implementation cannot drift silently.

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, Scope,
    Tenancy,
};

/// Every Lean read carries the integrations read scope.
const READ: &[Scope] = &[Scope::IntegrationsRead];

/// The service alias; its base URL is `CONVEYOR_EXTERNAL_LEAN4_URL`.
const SERVICE: &str = "lean4";

/// The tenant's sealed licence bearer for the prover.
pub const LEAN4_TOKEN_SECRET: &str = "lean4.token";

/// The upstream document, passed through unchanged.
///
/// The conveyor does not re-model another service's answer: it validates nothing, rewrites
/// nothing and stores nothing. A typed copy would drift the day the service adds a field.
fn passthrough_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The Lean prover's document, passed through unchanged.",
        "additionalProperties": true
    })
}

/// Build one read spec. The input is the operation's own request body.
const fn spec(
    name: &'static str,
    path: &'static str,
    timeout_ms: u32,
    input: fn() -> Schema,
) -> ActionSpec {
    ActionSpec {
        name,
        version: 1,
        kind: ActionKind::External,
        input,
        output: passthrough_output,
        scopes: READ,
        idempotent: true,
        timeout_ms,
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
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path,
            auth: Some(LEAN4_TOKEN_SECRET),
            auth_optional: false,
            upload: None,
        },
    }
}

/// `POST /api/check` (the app's `CheckRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CheckIn {
    /// The Lean source to check (1..=100000 characters).
    source: String,
    /// The prover's own timeout in seconds (1..=120); the app clamps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

fn check_input() -> Schema {
    schemars::schema_for!(CheckIn)
}

/// `POST /api/elaborate` (the app's `ElaborateRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ElaborateIn {
    /// The term to elaborate (1..=50000 characters).
    term: String,
    /// Optional local context (at most 100000 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    /// The prover's own timeout in seconds (1..=120); the app clamps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

fn elaborate_input() -> Schema {
    schemars::schema_for!(ElaborateIn)
}

/// `POST /api/prove` (the app's `ProveRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ProveIn {
    /// The theorem statement (1..=50000 characters).
    theorem: String,
    /// Tactics to try, in order (at most 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tactics: Option<Vec<String>>,
    /// Imports the proof needs (at most 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    imports: Option<Vec<String>>,
    /// The prover's own timeout in seconds (1..=120); the app clamps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

fn prove_input() -> Schema {
    schemars::schema_for!(ProveIn)
}

/// `lean4.check` — typecheck a source document; the app answers diagnostics on a 200.
fn check() -> ActionSpec {
    spec("lean4.check", "/api/check", 125_000, check_input)
}
inventory::submit! { ActionRegistration::new(check) }

/// `lean4.elaborate` — elaborate a term and return its type.
fn elaborate() -> ActionSpec {
    spec(
        "lean4.elaborate",
        "/api/elaborate",
        125_000,
        elaborate_input,
    )
}
inventory::submit! { ActionRegistration::new(elaborate) }

/// `lean4.prove` — attempt a proof; a saturation refusal is the app's 503, not a retry.
fn prove() -> ActionSpec {
    spec("lean4.prove", "/api/prove", 125_000, prove_input)
}
inventory::submit! { ActionRegistration::new(prove) }
