#![forbid(unsafe_code)]
//! The scientific processor's read actions (T14, SPEC_V2 §11.2/§11.9; ADR 0006 §4).
//!
//! The service (`apps/scientific`) is absorbed and its router is Rust; the live deployment is
//! still the Python image at zero replicas. These actions declare only the read operations a
//! caller actually holds: the three statistics reads and the two formal-method reads. The graph
//! surface, the graph-dependent operations and the sandbox are **not** ported (ADR 0006 §4) — a
//! `scientific_graph.*` or `scientific_sandbox.*` action does not exist, and the inventory test
//! asserts the absence.
//!
//! Every input is the app's own request body, closed: an unknown field is a `422` from the
//! engine's schema validation before any call reaches the service. The service validates the
//! same constraints again (its handlers answer `422` for a two-study floor, a bootstrap range,
//! an unknown language), so the declaration and the implementation cannot drift silently.

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, SchemaRef,
    Scope, Tenancy,
};
use std::borrow::Cow;

/// Every scientific read carries the integrations read scope.
const READ: &[Scope] = &[Scope::IntegrationsRead];

/// The service alias; its base URL is `CONVEYOR_EXTERNAL_SCIENTIFIC_URL`.
const SERVICE: &str = "scientific";

/// The upstream document, passed through unchanged.
///
/// The conveyor does not re-model another service's answer: it validates nothing, rewrites
/// nothing and stores nothing. A typed copy would drift the day the service adds a field.
fn passthrough_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The scientific service's document, passed through unchanged.",
        "additionalProperties": true
    })
}

/// Build one read spec. The input defaults to an empty object and is replaced by the operation's
/// own request body below.
const fn spec(
    name: &'static str,
    path: &'static str,
    timeout_ms: u32,
    input: fn() -> Schema,
) -> ActionSpec {
    ActionSpec {
        name: Cow::Borrowed(name),
        version: 1,
        kind: ActionKind::External,
        input: SchemaRef::Compiled(input),
        output: SchemaRef::Compiled(passthrough_output),
        scopes: Cow::Borrowed(READ),
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
            service: Cow::Borrowed(SERVICE),
            method: Cow::Borrowed("POST"),
            path: Cow::Borrowed(path),
            auth: None,
            auth_optional: false,
            upload: None,
        },
    }
}

// ── /api/stats ──────────────────────────────────────────────────────────────────────────

/// One study in a meta-analysis (the app's `StudyResult`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct StudyIn {
    /// The study's effect size.
    effect_size: f64,
    /// The study's standard error; the app requires a positive value.
    standard_error: f64,
    /// The study's sample size; the app requires a positive value.
    sample_size: i64,
    /// An optional weight (the app computes its own when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    weight: Option<f64>,
    /// An optional label passed through in the weights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// `POST /api/stats/meta-analysis` (the app's `MetaAnalysisRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct MetaAnalysisIn {
    /// The studies to pool; the app refuses fewer than two.
    studies: Vec<StudyIn>,
    /// `fixed` or `random`; the app defaults to `random`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    /// The confidence level in `(0, 1)`; the app defaults to 0.95.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confidence_level: Option<f64>,
}

/// `POST /api/stats/confidence-interval` (the app's `ConfidenceIntervalRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ConfidenceIntervalIn {
    /// The values; the app requires between 2 and 100000 entries.
    values: Vec<f64>,
    /// The confidence level in `(0, 1)`; the app defaults to 0.95.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confidence_level: Option<f64>,
    /// `bootstrap` or `normal`; the app defaults to `bootstrap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    /// The bootstrap resample count in 100..=100000; the app defaults to 10000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n_bootstrap: Option<u32>,
}

/// One series point (the app's `SeriesPoint`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct SeriesPointIn {
    /// The point's timestamp.
    timestamp: f64,
    /// The point's value.
    value: f64,
}

/// `POST /api/stats/trend` (the app's `TrendRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct TrendIn {
    /// The series; the app requires between 3 and 10000 points.
    data: Vec<SeriesPointIn>,
    /// `mann_kendall` or `linear_regression`; the app defaults to `mann_kendall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    test: Option<String>,
}

// ── /api/formal ─────────────────────────────────────────────────────────────────────────

/// The formal language the app supports (its `Language`).
#[derive(Clone, Copy, Debug, schemars::JsonSchema)]
#[allow(dead_code)]
#[serde(rename_all = "lowercase")]
enum Language {
    /// Lean 4.
    Lean4,
    /// Z3.
    Z3,
    /// SymPy.
    Sympy,
}

/// `POST /api/formal/translate` (the app's `FormalTranslateRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct FormalTranslateIn {
    /// The claim to translate; the app refuses more than 5000 characters.
    claim_text: String,
    /// The target language; the app defaults to `z3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_language: Option<Language>,
}

/// `POST /api/formal/verify` (the app's `FormalVerifyRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct FormalVerifyIn {
    /// The formal expression to check.
    formal_expression: String,
    /// The expression's language; the app defaults to `z3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<Language>,
}

fn meta_analysis_input() -> Schema {
    schemars::schema_for!(MetaAnalysisIn)
}
fn confidence_interval_input() -> Schema {
    schemars::schema_for!(ConfidenceIntervalIn)
}
fn trend_input() -> Schema {
    schemars::schema_for!(TrendIn)
}
fn translate_input() -> Schema {
    schemars::schema_for!(FormalTranslateIn)
}
fn verify_input() -> Schema {
    schemars::schema_for!(FormalVerifyIn)
}

/// `scientific_stats.meta_analysis` — pool study effect sizes (the app's `/api/stats/meta-analysis`).
fn meta_analysis() -> ActionSpec {
    spec(
        "scientific_stats.meta_analysis",
        "/api/stats/meta-analysis",
        30_000,
        meta_analysis_input,
    )
}
inventory::submit! { ActionRegistration::new(meta_analysis) }

/// `scientific_stats.confidence_interval` — an interval over a sample (bootstrap or normal).
fn confidence_interval() -> ActionSpec {
    spec(
        "scientific_stats.confidence_interval",
        "/api/stats/confidence-interval",
        30_000,
        confidence_interval_input,
    )
}
inventory::submit! { ActionRegistration::new(confidence_interval) }

/// `scientific_stats.trend` — a Mann-Kendall or linear-regression trend over a series.
fn trend() -> ActionSpec {
    spec(
        "scientific_stats.trend",
        "/api/stats/trend",
        10_000,
        trend_input,
    )
}
inventory::submit! { ActionRegistration::new(trend) }

/// `scientific_formal.translate` — claim text to a formal expression (Lean 4, Z3 or SymPy).
fn translate() -> ActionSpec {
    spec(
        "scientific_formal.translate",
        "/api/formal/translate",
        60_000,
        translate_input,
    )
}
inventory::submit! { ActionRegistration::new(translate) }

/// `scientific_formal.verify` — check a formal expression; the app answers the typed refusal for
/// an unsupported language rather than retrying it (ADR 0006 §4).
fn verify() -> ActionSpec {
    spec(
        "scientific_formal.verify",
        "/api/formal/verify",
        60_000,
        verify_input,
    )
}
inventory::submit! { ActionRegistration::new(verify) }
