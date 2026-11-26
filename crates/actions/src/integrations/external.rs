//! The external dispatches that reach another service (S09 §5.1, completed by #450).
//!
//! Every one of these is a **declared action**, not an HTTP call a handler makes: the spec names the service, the
//! method and the path, and the engine's `invoke_external` owns the timeout, the process-local breaker, the
//! idempotency claim and the audit row. A direct `reqwest::` call anywhere else is therefore a defect, and
//! `externals_declared` asserts there is none outside the engine.
//!
//! The base URL is per service: `CONVEYOR_EXTERNAL_<SERVICE>_URL`. An unset service is `ActionError::unconfigured`
//! — a visible refusal, never a call to a guessable default.
//!
//! Every destination left here is a Rust service: the coding fleet, the billing shim, a mac-mini
//! model server (speech, transcription, images, model calls), the strata retriever, the third-party
//! laya judge, and the local fixtures transport. The five V1 displays that named a Python
//! destination or a service that no longer exists were moved onto conveyor state in #450 — see the
//! note above `coding_jobs.create`.
//!
//! ⚠️ **`HandlerRef` has no path templating**: the path is static and a `GET` carries no body. So `coding_jobs.get`
//! is a POST with the id in the body against `/api/jobs/status`, and the coding gateway must serve that route.
//! That contract is written here because it is the one thing about these actions a reader cannot infer from the
//! external service's own OpenAPI.

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, SchemaRef,
    Scope, Tenancy,
};
use std::borrow::Cow;

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

/// The body `coding_jobs.create` sends to the coding service.
///
/// The engine passes an external action's input through **verbatim**, so this is the coding
/// service's own create document, not a conveyor model. It exists so the OpenAPI operation and the
/// MCP tool name the fields a flow or an agent must send; the service validates them again.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct CodingJobCreateIn {
    /// The job kind: `test`, `lint`, `build`, `check`, `scan`, … (the coding service's JobKind).
    kind: String,
    /// Where the code comes from: inline files, a git ref, or an uploaded archive.
    input: serde_json::Value,
    /// Run against an existing workspace instead of naming one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    /// A one-shot workspace named by its repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace: Option<CodingWorkspaceIn>,
    /// Caller-supplied dedupe key, unique per tenant; a replay returns the original job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    idempotency_key: Option<String>,
    /// How long the job may run inside its Pod, in seconds (1..=86400; the service defaults to 900).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_seconds: Option<u32>,
}

/// The one-shot workspace inside a coding create body.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct CodingWorkspaceIn {
    repo_url: String,
    /// The ref to check out; the coding service defaults it to `main`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    toolchain: Option<String>,
}

/// The body `coding_jobs.get` sends: the job id, carried in the body because the engine has no
/// path templating and a GET carries no body.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct CodingJobGetIn {
    /// The job id returned by `coding_jobs.create`.
    id: String,
}

fn coding_job_create_input() -> Schema {
    schemars::schema_for!(CodingJobCreateIn)
}

fn coding_job_get_input() -> Schema {
    schemars::schema_for!(CodingJobGetIn)
}

/// The body `billing.rollup` sends to the billing shim (SPEC §12 T11).
///
/// The engine passes an external action's input through **verbatim**, so this is the shim's own
/// roll-up request document: the window and, optionally, the dimensions to group by.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct BillingRollupIn {
    /// First day of the window (inclusive).
    since: chrono::NaiveDate,
    /// Last day of the window (inclusive).
    until: chrono::NaiveDate,
    /// The fields the shim groups the rows by; absent means the shim's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group_by: Option<Vec<String>>,
}

fn billing_rollup_input() -> Schema {
    schemars::schema_for!(BillingRollupIn)
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
        name: Cow::Borrowed(name),
        version: 1,
        kind: ActionKind::External,
        input: SchemaRef::Compiled(empty_input),
        output: SchemaRef::Compiled(passthrough_output),
        scopes: Cow::Borrowed(scopes),
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
        artefact: None,
        approval_target: None,
        handler: HandlerRef::External {
            service: Cow::Borrowed(service),
            method: Cow::Borrowed(method),
            path: Cow::Borrowed(path),
            auth: None,
            auth_optional: false,
            upload: None,
        },
    }
}

// The V1 displays that named a Python destination — `workers.list`, `providers.list`,
// `graphs.list`, `artifacts.list`, `billing.summary` — are no longer externals (issue #450):
// the worker inventory is `crate::workers` over the durable queue and the host registry
// (§11.2), the artifact catalogue is `documents::artifacts` over the document store (§11.3,
// D57), and the billing figure is `metrics::billing_summary` over the ledger (#46).

/// `coding_jobs.create` — dispatch a coding task; the run keeps the returned ref (§5.3).
fn coding_jobs_create() -> ActionSpec {
    let mut action = spec(Target {
        name: "coding_jobs.create",
        service: "coding",
        method: "POST",
        path: "/api/jobs",
        scopes: WRITE,
        timeout_ms: 30_000,
        effect: Effect::Write,
        idempotent: false,
    });
    // The only two externals that are a *job submission* rather than a display: their bodies are
    // the coding service's own documents, so the manifest names the fields instead of saying
    // nothing.
    action.input = SchemaRef::Compiled(coding_job_create_input);
    action
}
inventory::submit! { ActionRegistration::new(coding_jobs_create) }

/// `coding_jobs.get` — poll a dispatched job.
///
/// A POST, not a GET, because the engine has no path templating and a GET carries no body.
fn coding_jobs_get() -> ActionSpec {
    let mut action = spec(Target {
        name: "coding_jobs.get",
        service: "coding",
        method: "POST",
        path: "/api/jobs/status",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    });
    action.input = SchemaRef::Compiled(coding_job_get_input);
    action
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

/// `billing.rollup` — the billing shim's usage roll-up for one period (SPEC §12 T11).
///
/// A read, and a passthrough: `metrics.reconcile` sums the returned `rows[].cost_usd`; the
/// conveyor does not re-model the shim's document.
fn billing_rollup() -> ActionSpec {
    let mut action = spec(Target {
        name: "billing.rollup",
        service: "billing_shim",
        method: "POST",
        path: "/api/usage/rollup",
        scopes: READ,
        timeout_ms: 5_000,
        effect: Effect::Read,
        idempotent: true,
    });
    action.input = SchemaRef::Compiled(billing_rollup_input);
    action
}
inventory::submit! { ActionRegistration::new(billing_rollup) }
