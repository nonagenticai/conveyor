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
// S15: work-package memory — remember (the only writing action) and recall.
pub mod memory;
pub mod model;
pub mod openapi;
// T0 (SPEC_V2 §4, §10.4): the organisation spine — industries, the job catalogue, immutable
// releases and bindings.
pub mod organisation;
pub mod portfolio;
pub mod process;
// SPEC D28 / CONTRACT §21: the SOP (procedure) that judges a process step.
pub mod procedure;
// SPEC D30 / CONTRACT §21: the user-facing process map — two read actions.
pub mod process_map;
// SPEC D27: prompt compilation — one catalogue, three surfaces (agent.run, copilot, prompt audit).
pub mod prompt;
// S16: the prompt-audit surface and its deterministic scan.
pub mod prompt_audit;
pub mod rbac;
pub mod registry;
pub mod spec;
pub mod views;
// S13: `workflows.*` — the engine's authoring surface (one document, versioned by key).
pub mod workflow;

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
