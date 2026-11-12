#![forbid(unsafe_code)]
//! The capability surface's request and response documents.
//!
//! `capability_agent` is configuration, not a loop (CONTRACT §17): the documents here describe a
//! persisted executor spec, and the executor that consumes it is S13's `agent.run` step (S15 step
//! 11 wires the run to it). Caps are the contract's hard ceiling (`1..=20` turns, `1..=300` s);
//! `allowed_actions` is checked for grammar here and for registration at readiness, so a spec may
//! name `agent.run` before S13 registers it.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::views::PageIn;

/// One capability as every read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityOut {
    /// The row id.
    pub id: Uuid,
    /// The tenant-unique code (`analysis`, `architecture`, …).
    pub code: String,
    /// The display label.
    pub label: String,
    /// The description (`''` when absent).
    pub description: String,
    /// `draft | active | retired`.
    pub status: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version the `ETag` carries.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One capability with its pinned knowledge packs and its readiness.
///
/// `ready`/`blocking` are computed on read by `capability::readiness` — the SAME projection
/// `process_runs.start` refuses on (`409 capability_not_ready`). `blocking[]` is CONTRACT §17's
/// name for the set; its elements are the API-wide `FieldError` the 409 carries as `errors[]`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityDetailOut {
    /// The capability itself.
    #[serde(flatten)]
    pub capability: CapabilityOut,
    /// The packs bound to it, in pack-key order.
    pub knowledge: Vec<KnowledgeOut>,
    /// Whether the capability may start a run now.
    pub ready: bool,
    /// The blocking reasons; empty exactly when `ready` is true.
    pub blocking: Vec<crate::spec::FieldError>,
}

/// One capability → pack binding.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KnowledgeOut {
    /// The binding row id.
    pub id: Uuid,
    /// The capability it binds.
    pub capability_id: Uuid,
    /// The strata pack key.
    pub pack_key: String,
    /// The pinned pack version.
    pub pack_version: i32,
    /// The minimum score a hit must reach, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    /// Whether an unreachable pack blocks the run.
    pub required: bool,
}

/// One executor spec as every read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentOut {
    /// The row id.
    pub id: Uuid,
    /// The capability it executes.
    pub capability_id: Uuid,
    /// The tenant-unique key.
    pub key: String,
    /// `draft | active | retired`.
    pub status: String,
    /// The model registry reference.
    pub model_ref: String,
    /// The built-in prompt, `<name>@<version>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
    /// Free-form parameters carried into the step input.
    pub params: Value,
    /// The actions the executor may invoke.
    pub allowed_actions: Vec<String>,
    /// The loop's iteration ceiling (`1..=20`).
    pub max_iterations: i32,
    /// The loop's wall-clock ceiling (`1..=300`).
    pub timeout_secs: i32,
    /// The optimistic-concurrency version the `ETag` carries.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// `capabilities.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityListIn {
    /// `draft | active | retired`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `capabilities.get` input: exactly one of `capability_id` or `code`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityGetIn {
    /// The row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The tenant-unique code, when the caller does not know the row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// `capabilities.bind_knowledge` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BindKnowledgeIn {
    /// The capability the pack is bound to.
    pub capability_id: Uuid,
    /// The strata pack key.
    pub pack_key: String,
    /// The pinned pack version (positive).
    pub pack_version: i32,
    /// The minimum score a hit must reach, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    /// Whether the pack is required: an unreachable required pack blocks the run.
    #[serde(default)]
    pub required: bool,
}

/// `capability_agents.create` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentCreateIn {
    /// The capability the executor executes.
    pub capability_id: Uuid,
    /// The tenant-unique key.
    pub key: String,
    /// `draft | active | retired` (default `draft`, the column's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The model registry reference.
    pub model_ref: String,
    /// A built-in prompt, `<name>@<version>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
    /// Free-form parameters carried into the step input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// The actions the executor may invoke (default: none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_actions: Option<Vec<String>>,
    /// The loop's iteration ceiling (`1..=20`, default 8 — CONTRACT §17).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<i32>,
    /// The loop's wall-clock ceiling (`1..=300`, default 300).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
}

/// `capability_agents.update` input: a patch, applied to the row as it stands.
///
/// An absent member keeps its stored value; `prompt_ref: null` clears it. `capability_id` is not a
/// member: moving an agent between capabilities would change what every recorded run resolved.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentUpdateIn {
    /// The row to edit.
    pub agent_id: Uuid,
    /// A new status; the state machine refuses an illegal move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// A new model registry reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    /// A new built-in prompt; `null` clears it. Absent keeps it.
    #[serde(default, deserialize_with = "deserialize_present")]
    pub prompt_ref: Option<Option<String>>,
    /// Replacement parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// Replacement allowed actions, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_actions: Option<Vec<String>>,
    /// A new iteration ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<i32>,
    /// A new wall-clock ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
}

/// `capability_agents.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AgentListIn {
    /// Only agents of this capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `capability_agents.get` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentGetIn {
    /// The row id.
    pub agent_id: Uuid,
}

/// Distinguish an ABSENT member from an explicit `null` on a nullable patch field.
///
/// Plain `Option<Option<T>>` cannot do it: serde maps JSON `null` to the OUTER `None`, so a patch
/// that means "clear it" would read as "leave it". This wrapper keeps `null` as `Some(None)`.
fn deserialize_present<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
