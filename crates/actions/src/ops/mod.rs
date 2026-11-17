#![forbid(unsafe_code)]
//! `ops.*` — the cluster operations read family (T7, SPEC_V2 §10.4, SPEC_V2 §10.4.1).
//!
//! Six reads over the estate's own Kubernetes API, so an agent or a procedure can answer
//! "what is running, what changed, why did it fail" without a `kubectl` in the loop:
//!
//! | action | what it reads |
//! |---|---|
//! | `ops.pods` | pods in one namespace, by label |
//! | `ops.deployments` | deployments in one namespace, by label |
//! | `ops.events` | events in one namespace, by label |
//! | `ops.logs` | one pod's log tail (bounded, default 200 lines) |
//! | `ops.flux_status` | the Flux `Kustomization` objects — the estate's reconciliation state |
//! | `ops.ci_status` | recent GitHub Actions runs (platform `CONVEYOR_CI_TOKEN`) |
//!
//! #172 adds the host family (`host.rs`): `ops.host_status`, `ops.host_logs` and the destructive
//! `ops.host_restart_unit`, which reach a bare-metal host through its own fixed-verb agent rather
//! than the cluster API. The destination comes from `host_registry`, never from the caller.
//!
//! **Every input is closed** (`deny_unknown_fields`): a namespace and a label selector, never a
//! free-form URL — the operation is named, not constructed by the caller. Names are validated
//! here before they are interpolated into an API path.
//!
//! **The credential is the worker's.** `conveyor-worker` is the only pod template that
//! automounts a service-account token (`clusters/strato/services/conveyor/serviceaccount.yaml`
//! states ⛔ that the serve container must not carry one). A process without it answers
//! `503 action-service-unconfigured`, never a guessed default.
//!
//! **Reads only.** The write family is the next issue (#171); nothing here mutates the cluster.

use conveyor_ops::OpsError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::ActionError;

pub mod host;
pub mod reads;
pub mod writes;

/// The service key the refusals name.
pub const SERVICE: &str = "ops";

/// The default page for every list read.
pub const DEFAULT_LIMIT: u32 = 100;

/// The largest page a caller may ask for.
pub const MAX_LIMIT: u32 = 500;

/// The default log tail in lines.
pub const DEFAULT_TAIL: u32 = 200;

/// The largest log tail a caller may ask for — a log read is a sample, not a download.
pub const MAX_TAIL: u32 = 2_000;

/// One namespace's list of objects.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ListOut {
    /// The namespace asked for.
    pub namespace: String,
    /// The Kubernetes resource, e.g. `pods`.
    pub resource: String,
    /// How many objects came back.
    pub count: usize,
    /// The objects, exactly as the API returned them.
    pub items: Vec<Value>,
}

/// `ops.pods` / `ops.deployments` / `ops.events` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListIn {
    /// The namespace to read.
    pub namespace: String,
    /// A Kubernetes label selector (`app=conveyor,tier!=test`).
    #[serde(default)]
    pub label_selector: Option<String>,
    /// The page size (1..=500, default 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// `ops.flux_status` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FluxIn {
    /// The namespace holding the Flux objects — `flux-system` when omitted.
    #[serde(default = "flux_namespace")]
    pub namespace: String,
    /// One `Kustomization` by name; every one in the namespace when omitted.
    #[serde(default)]
    pub name: Option<String>,
    /// The page size (1..=500, default 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

fn flux_namespace() -> String {
    "flux-system".to_owned()
}

/// `ops.logs` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LogsIn {
    /// The pod's namespace.
    pub namespace: String,
    /// The pod name.
    pub pod: String,
    /// One container; the pod's only container when omitted.
    #[serde(default)]
    pub container: Option<String>,
    /// How many trailing lines (default 200, max 2000).
    #[serde(default)]
    pub tail: Option<u32>,
    /// The previous instance's log (`kubectl logs --previous`).
    #[serde(default)]
    pub previous: Option<bool>,
}

/// `ops.logs` output.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct LogsOut {
    /// The namespace read.
    pub namespace: String,
    /// The pod read.
    pub pod: String,
    /// The container, when one was named.
    pub container: Option<String>,
    /// The tail that was requested.
    pub tail: u32,
    /// The log text, as the API returned it.
    pub log: String,
}

/// `ops.ci_status` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CiIn {
    /// `owner/name`; the configured default repository when omitted.
    #[serde(default)]
    pub repo: Option<String>,
    /// One branch's runs.
    #[serde(default)]
    pub branch: Option<String>,
    /// How many runs, newest first (default 20, max 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// `ops.ci_status` output.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct CiOut {
    /// The repository read.
    pub repo: String,
    /// The branch filter, when one was given.
    pub branch: Option<String>,
    /// How many runs came back.
    pub count: usize,
    /// The run objects, exactly as GitHub returned them.
    pub runs: Vec<Value>,
}

/// `ops.restart` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestartIn {
    /// The namespace of the workload.
    pub namespace: String,
    /// The Deployment to restart.
    pub deployment: String,
    /// The approved approval that authorises this exact restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    /// Return the request that WOULD be made and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// `ops.scale` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScaleIn {
    /// The namespace of the workload.
    pub namespace: String,
    /// The Deployment to scale.
    pub deployment: String,
    /// The replica count to set.
    pub replicas: u32,
    /// The approved approval that authorises this exact scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    /// Return the request that WOULD be made and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// `ops.rollback` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RollbackIn {
    /// The namespace of the workload.
    pub namespace: String,
    /// The Deployment to roll back.
    pub deployment: String,
    /// The revision to restore; defaults to the revision before the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    /// The approved approval that authorises this exact rollback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    /// Return the request that WOULD be made and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// One check in a write's read-back.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CheckOut {
    /// What was checked.
    pub name: String,
    /// Whether the observed state satisfies it.
    pub ok: bool,
    /// The observation, as text (never a whole object).
    pub detail: String,
}

/// The request a write made (or, on a dry run, would make).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RequestOut {
    /// The HTTP method.
    pub method: String,
    /// The API path.
    pub path: String,
    /// The query pairs, `dryRun=All` included when it was a dry run.
    pub query: Vec<String>,
    /// The merge-patch body.
    pub body: Value,
}

/// What a destructive write answers.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WriteOut {
    /// The namespace written.
    pub namespace: String,
    /// The Deployment written.
    pub deployment: String,
    /// `restart`, `scale` or `rollback`.
    pub operation: String,
    /// The exact thing the approval authorised.
    pub target: String,
    /// The approval that authorised the call.
    pub approval_id: Option<Uuid>,
    /// Whether anything was written.
    pub dry_run: bool,
    /// The request made, or the one a dry run would make.
    pub request: RequestOut,
    /// The read-back observations.
    pub checks: Vec<CheckOut>,
}

/// Clamp a requested page size into the read family's bounds.
pub fn limit_of(requested: Option<u32>) -> u32 {
    requested.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Clamp a requested log tail; a sample, never a download.
pub fn tail_of(requested: Option<u32>) -> u32 {
    requested.unwrap_or(DEFAULT_TAIL).clamp(1, MAX_TAIL)
}

/// Validate a Kubernetes object name before it is interpolated into an API path.
///
/// DNS-label shape: lower case alphanumerics, `-` and `.`, no leading or trailing separator.
/// This is what keeps a caller from turning a "namespace" into an arbitrary path.
pub fn name_of(field: &str, value: &str) -> Result<String, ActionError> {
    let value = value.trim();
    let ok = !value.is_empty()
        && value.len() <= 253
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '.'
        })
        && !value.starts_with(['-', '.'])
        && !value.ends_with(['-', '.']);
    if !ok {
        return Err(invalid(
            field,
            "invalid_value",
            "must be a lowercase Kubernetes name (a-z, 0-9, '-', '.')",
        ));
    }
    Ok(value.to_owned())
}

/// A label selector is passed through as a query parameter and bounded in length.
pub fn selector_of(value: Option<&str>) -> Result<Option<String>, ActionError> {
    match value.map(str::trim).filter(|text| !text.is_empty()) {
        None => Ok(None),
        Some(selector) if selector.len() > 512 => Err(invalid(
            "label_selector",
            "invalid_value",
            "a label selector is at most 512 bytes",
        )),
        Some(selector) => Ok(Some(selector.to_owned())),
    }
}

/// The `items` array of a Kubernetes List, with the objects untouched.
pub fn items_of(document: Value) -> Vec<Value> {
    document
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Build the read client from the process environment, refusing visibly when it is unconfigured.
pub fn client() -> Result<conveyor_ops::Client, ActionError> {
    conveyor_ops::Client::from_env().map_err(refusal)
}

/// Map a client failure onto the action surface: `503` unconfigured, the API's own status when it
/// answered, `502` for a transport failure.
pub fn refusal(error: OpsError) -> ActionError {
    match error {
        OpsError::Unconfigured => ActionError::unconfigured(SERVICE),
        OpsError::Api { status, body } => ActionError::new(status, "ops-api", body),
        other => ActionError::new(502, "ops-unavailable", other.to_string()),
    }
}
