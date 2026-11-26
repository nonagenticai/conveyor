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
// ADR 0007 §5: the `analyse.*` family — the analysis library's invocable surface.
pub mod analysis;
// T9 (SPEC_V2 §8.1, §10.3, §10.4, D63): the TOGAF ADM action family.
pub mod architecture;
// T17 (#87, SPEC_V2 §5.7, §10.4): the tenant asset authoring family.
pub mod authoring;
// S17: the benchmark harness — suites, runs, cases.
pub mod benchmark;
pub mod board;
// T13 (SPEC_V2 §11.3): the tool broker — resolve, gate, validate, dispatch and ledger one call.
pub mod broker;
// S15: the capability aggregate — `capabilities.*` and `capability_agents.*`.
pub mod capability;
// S06d: the connector standard — grants, compiled defaults, the machine floor and adapters.
pub mod channel;
pub mod coach;
pub mod control;
// #161 P3: the deck toolkit — one action rendering the model to .pptx, PDF or PNG.
pub mod deck;
// #366: the spreadsheet toolkit — produce a .xlsx from a workbook, read one back.
pub mod documents;
pub mod eval;
pub mod executor;
pub mod fixtures;
pub mod governance;
pub mod handlers;
pub mod sheets;
// T14 (#83): the estate import wave, its probe and the zero-use report — the toolkit catalogue
// `tools.call` resolves against.
pub mod toolkits;
// S16: the inference-ledger read surface.
pub mod inference;
pub mod ingest;
pub mod integrations;
pub mod invoke;

// T5 (SPEC_V2 §6.3, §9.3–§9.4; issue #77): the kata — six Gets, the pre-registered
// prediction and the held-out measurement, on an ordinary V2 binding.
pub mod kata;
// T8 (SPEC_V2 §7.6, §10.3, §10.4, §12 T8, D36; issue #79): organisational memory — the
// `lessons.*` family, independent evidence, a person's review and promotion to a source draft.
pub mod lesson;
// T6 (SPEC_V2 §5.4, §6.4, §9.1, §9.3; issue #80): the development project — the charter, the
// immutable step evidence, the review/merge/deploy gates, the earned auto-merge policy and the
// closure record.
pub mod development;

// S15: capability-bound retrieval from strata platform packs.
pub mod knowledge;
pub mod lock;
pub mod mcp;
// #139 P3.1: the media group's toolkit — one action per capability kind, plus the probe.
pub mod media;
pub mod produce;
// S15: work-package memory — remember (the only writing action) and recall.
pub mod memory;
// T19 (SPEC_V2 §6.8, D66; issue #606): the meetings family — the decision forum's typed record.
pub mod meetings;
// T11 (SPEC_V2 §7.5, §11.9; issue #84): the metrics family — recompute-from-rows reads, the
// versioned threshold configuration and the programme's MSP/governance metrics.
pub mod metrics;
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
pub mod pm;
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
pub mod resolve;
// T1 (SPEC_V2 §5, §8.2, §8.3): the plan spine and the MSP rows.
pub mod spec;
pub mod spine;
// T13 (SPEC_V2 §11.10): the job object-storage adapter — tenant-prefixed, hashed, capped.
pub mod storage;
pub mod storage_actions;
// T13 (SPEC_V2 §11.3): the `tools.call` action declaration (the broker enforces it).
pub mod tools;
// T15 (#86, SPEC_V2 §11.3–§11.6): tool authoring and BYO — OpenAPI draft import, the
// publication gate, MCP discovery and the registered-destination dispatch.
pub mod tool_authoring;
// T2 (SPEC_V2 §7): the decision ladder — `ladder.*` over `decision_policies.*`,
// `refinements.*` and `laya_configs.*`.
pub mod ladder;
pub mod views;
// #450 (SPEC_V2 §11.2): the worker and provider inventory — the durable queue and the host
// registry, served where the retired `worker-management` external used to be read.
pub mod workers;
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
    ActionKind, ActionSpec, AgentHints, AgentTier, ApprovalTarget, Confirm, Effect, FieldError,
    HandlerFn, HandlerFuture, HandlerOutput, HandlerRef, Schema, SchemaRef, Scope, Tenancy,
    validate_input,
};
pub use views::{
    ActionManifest, ActionManifestEntry, ActionSpecView, InvocationBody, InvocationResponse, Page,
    PageIn,
};
