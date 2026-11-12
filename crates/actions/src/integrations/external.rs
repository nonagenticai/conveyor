//! The eight external displays and dispatches (S09 §5.1).
//!
//! Every one of these is a **declared action**, not an HTTP call a handler makes: the spec names the service, the
//! method and the path, and the engine's `invoke_external` owns the timeout, the process-local breaker, the
//! idempotency claim and the audit row. A direct `reqwest::` call anywhere else is therefore a defect, and
//! `externals_declared` asserts there is none outside the engine.
//!
//! The base URL is per service: `CONVEYOR_EXTERNAL_<SERVICE>_URL`. An unset service is `ActionError::unconfigured`
//! — a visible refusal, never a call to a guessable default.
//!
//! What is deliberately NOT here: any local copy of a worker inventory, a billing engine or a model catalogue.
//! Reading `worker_management`'s database would make the conveyor its owner (S09 §8.4), so the only thing this
//! module knows about those services is their address.
//!
//! ⚠️ **`HandlerRef` has no path templating**: the path is static and a `GET` carries no body. So `coding_jobs.get`
//! is a POST with the id in the body against `/api/jobs/status`, and the coding gateway must serve that route.
//! That contract is written here because it is the one thing about these actions a reader cannot infer from the
//! external service's own OpenAPI.

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, Scope,
    Tenancy,
};

/// The scope every external read carries.
const READ: &[Scope] = &[Scope::IntegrationsRead];
/// The scope the one external write carries.
const WRITE: &[Scope] = &[Scope::IntegrationsWrite];

/// The input of a display: there is none, and the engine's GET sends no body anyway.
fn empty_input() -> Schema {
    schemars::schema_for!(crate::views::EmptyIn)
}

/// An external service's answer, passed through unchanged.
///
/// The conveyor does not re-model another service's document: it validates nothing, rewrites nothing and
/// stores nothing. A typed copy would drift the day the service adds a field.
fn passthrough_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The upstream service's document, passed through unchanged.",
        "additionalProperties": true
    })
}

/// One external target: where the call goes and what it is allowed to do.
///
/// Bundled into a struct rather than passed as eight positional arguments — `name, service, method, path` reads
/// the same in every call, and a struct makes the two that differ (the scope and the timeout) impossible to swap.
struct Target {
    name: &'static str,
    service: &'static str,
    method: &'static str,
    path: &'static str,
    scopes: &'static [Scope],
    timeout_ms: u32,
    effect: Effect,
    idempotent: bool,
}

/// Build one external spec.
const fn spec(target: Target) -> ActionSpec {
    let Target {
        name,
        service,
        method,
        path,
        scopes,
        timeout_ms,
        effect,
        idempotent,
    } = target;
    ActionSpec {
        name,
        version: 1,
        kind: ActionKind::External,
        input: empty_input,
        output: passthrough_output,
        scopes,
        idempotent,
        timeout_ms,
        effect,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: false,
        handler: HandlerRef::External {
            service,
            method,
            path,
            auth: None,
            auth_optional: false,
        },
    }
}

/// `workers.list` — the worker inventory, read-only (§5.5).
fn workers_list() -> ActionSpec {
    spec(Target {
        name: "workers.list",
        service: "worker_management",
        method: "GET",
        path: "/workers",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(workers_list) }

/// `providers.list` — the model providers, read-only.
fn providers_list() -> ActionSpec {
    spec(Target {
        name: "providers.list",
        service: "worker_management",
        method: "GET",
        path: "/providers",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(providers_list) }

/// `graphs.list` — the worker graph catalogue, read-only.
fn graphs_list() -> ActionSpec {
    spec(Target {
        name: "graphs.list",
        service: "worker_management",
        method: "GET",
        path: "/graphs",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(graphs_list) }

/// `artifacts.list` — the artifact catalogue, display only (§5.1).
fn artifacts_list() -> ActionSpec {
    spec(Target {
        name: "artifacts.list",
        service: "artifacts",
        method: "GET",
        path: "/api/artifacts",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(artifacts_list) }

/// `billing.summary` — the cost/budget overview, display only (D12: `billing_service` owns billing).
fn billing_summary() -> ActionSpec {
    spec(Target {
        name: "billing.summary",
        service: "billing_service",
        method: "GET",
        path: "/api/billing/overview",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(billing_summary) }

/// `coding_jobs.create` — dispatch a coding task; the run keeps the returned ref (§5.3).
fn coding_jobs_create() -> ActionSpec {
    spec(Target {
        name: "coding_jobs.create",
        service: "coding",
        method: "POST",
        path: "/api/jobs",
        scopes: WRITE,
        timeout_ms: 30_000,
        effect: Effect::Write,
        idempotent: false,
    })
}
inventory::submit! { ActionRegistration::new(coding_jobs_create) }

/// `coding_jobs.get` — poll a dispatched job.
///
/// A POST, not a GET, because the engine has no path templating and a GET carries no body.
fn coding_jobs_get() -> ActionSpec {
    spec(Target {
        name: "coding_jobs.get",
        service: "coding",
        method: "POST",
        path: "/api/jobs/status",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(coding_jobs_get) }

/// `retrieval.query` — a ranked retrieval over `strata_rag` (never a local index).
fn retrieval_query() -> ActionSpec {
    spec(Target {
        name: "retrieval.query",
        service: "strata_rag",
        method: "POST",
        // `/api/query`, not `/query`: the base URL (`CONVEYOR_EXTERNAL_STRATA_RAG_URL`) names the
        // service ROOT, and `strata_rag` mounts its router under `/api` (src/api/mod.rs:71). The
        // sibling services below declare their prefix the same way; only `worker_management` is
        // bare. A base without the prefix produced a 404 the stub-backed tests could not see.
        path: "/api/query",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    })
}
inventory::submit! { ActionRegistration::new(retrieval_query) }
