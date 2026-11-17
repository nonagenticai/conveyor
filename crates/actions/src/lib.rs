#![forbid(unsafe_code)]
//! S04 — the action registry: one declaration, four projections.
//!
//! `api → actions → store → domain` (CONTRACT §1). The crate owns [`ActionSpec`], the
//! compile-time [`registry`], the invocation engine, the MCP transport, the OpenAPI fold,
//! the served-spec lock and the external ingest. Nothing here speaks SQL directly; every
//! query lives in `crates/store`.
//!
//! `extern crate self as conveyor_actions` makes the proc macro's absolute paths
//! (`::conveyor_actions::…`) resolve inside the crate that defines them.

extern crate self as conveyor_actions;

pub use conveyor_actions_derive::action;
pub use inventory;
pub use schemars;
pub use serde_json;
pub use sqlx;

pub mod agent_surface;
// S17: the benchmark harness — suites, runs, cases.
pub mod benchmark;
pub mod board;
// S15: the capability aggregate — `capabilities.*` and `capability_agents.*`.
pub mod capability;
// S06d: the connector standard — grants, compiled defaults, the machine floor and adapters.
pub mod channel;
pub mod control;
// #161 P3: the deck toolkit — one action rendering the model to .pptx, PDF or PNG.
pub mod deck;
pub mod documents;
pub mod eval;
pub mod fixtures;
pub mod governance;
pub mod handlers;
// S16: the inference-ledger read surface.
pub mod inference;
pub mod ingest;
pub mod integrations;
pub mod invoke;
// S15: capability-bound retrieval from strata platform packs.
pub mod knowledge;
pub mod lock;
pub mod mcp;
// #139 P3.1: the media group's toolkit — one action per capability kind, plus the probe.
pub mod media;
// S15: work-package memory — remember (the only writing action) and recall.
pub mod memory;
pub mod model;
pub mod openapi;
// T7 (#170): the cluster operations read family — `ops.pods|deployments|events|logs|flux_status|
// ci_status`, served by the `conveyor-ops` client.
pub mod ops;
// T0 (SPEC_V2 §4, §10.4): the organisation spine — industries, the job catalogue, immutable
// releases and bindings.
pub mod organisation;
pub mod portfolio;
pub mod process;
// SPEC D28 / CONTRACT §21: the SOP (procedure) that judges a process step.
pub mod procedure;
// SPEC D30 / CONTRACT §21: the user-facing process map — two read actions.
pub mod process_map;

/// Programme ordering: the dependency gate shared by the two run-start paths.
mod programme;
// SPEC D27: prompt compilation — one catalogue, three surfaces (agent.run, copilot, prompt audit).
pub mod prompt;
// S16: the prompt-audit surface and its deterministic scan.
pub mod prompt_audit;
pub mod publish;
pub mod rbac;
pub mod registry;
// T1 (SPEC_V2 §5, §8.2, §8.3): the plan spine and the MSP rows.
pub mod spec;
pub mod spine;
// T2 (SPEC_V2 §7): the decision ladder — `ladder.*` over `decision_policies.*`,
// `refinements.*` and `laya_configs.*`.
pub mod ladder;
pub mod views;
// S13: `workflows.*` — the engine's authoring surface (one document, versioned by key).
pub mod workflow;
// #182 slice A: the tenant's own YouTube channel — OAuth client, connect, status, disconnect.
pub mod youtube;

pub use invoke::{
    ActionError, Authority, HandlerCtx, InvokeOutcome, InvokeRequest, MAX_DEPTH, Precondition,
    UsageContext, invoke,
};
pub use registry::{
    ActionRegistration, Registry, RegistryError, assert_registry_consistent, registry,
};
pub use spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, FieldError, HandlerFn,
    HandlerFuture, HandlerOutput, HandlerRef, Schema, Scope, Tenancy, validate_input,
};
pub use views::{
    ActionManifest, ActionManifestEntry, ActionSpecView, InvocationBody, InvocationResponse, Page,
    PageIn,
};
