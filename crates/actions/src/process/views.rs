#![forbid(unsafe_code)]
//! The process surface's request and response documents.
//!
//! `version` is the **family** version: it moves only when a new version of the key is created, and
//! `uq (tenant_id, key, version)` gives it that job. `lock_version` is the optimistic-concurrency
//! counter `If-Match` compares and is bumped by every mutation. They are separate members on purpose
//! (migration `0017`): presenting the family version as a precondition would silently pass, because an
//! edit does not move it.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::views::PageIn;

/// One step as it arrives from the caller.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StepIn {
    /// The registry action a `computed` step invokes (`<resource>.<verb>`); blank otherwise.
    #[serde(default)]
    pub action: String,
    /// `agent | computed | gate | call`; absent = inferred (gate when `gate_kind` is set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// `process_agent | tool_agent | system | human`; absent = the kind's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// The capability the step belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The `capability_agent(role='process_agent')` that runs an `agent` step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_agent_id: Option<Uuid>,
    /// The published SOP this step is judged against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_id: Option<Uuid>,
    /// The published process or workflow a `call` step starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_process_key: Option<String>,
    /// The input template (S11 §5.4); `{}` when the action needs nothing.
    #[serde(default)]
    pub input_template: Value,
    /// A `catalogue_entry(kind='gate')` code: the step parks before running until it is approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_kind: Option<String>,
    /// `fail` (default), `continue` or `retry`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_result: Option<String>,
    /// How many attempts `retry` may make; default 1, cap 10 (enforced at publish).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<i32>,
    /// The dot-paths into this step's output that later steps may reference (S21 §6).
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// `processes.create` input.
///
/// A key that does not exist yet becomes version 1; a key that does forks `max(version) + 1` and copies
/// the source version's steps, which `steps` then replaces when supplied.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessCreateIn {
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The card type this process drives, when it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_type_code: Option<String>,
    /// The step list; absent means "no steps yet", which a draft may hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<StepIn>>,
}

/// `processes.update` input. Drafts only; `steps` replaces the whole list.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessUpdateIn {
    /// The version to edit.
    pub process_id: Uuid,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The replacement step list, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<StepIn>>,
}

/// `processes.get` input: exactly one of `process_id` or `key`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProcessGetIn {
    /// One version, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<Uuid>,
    /// A version family, by key. Alone this resolves the published version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// With `key`: an exact version. Not accepted with `process_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// `processes.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProcessListIn {
    /// `draft | published | retired`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only processes for this card type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_type_code: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `processes.publish` / `.retire` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessIdIn {
    /// The draft to publish, or the published version to retire.
    pub process_id: Uuid,
}

/// `processes.publish` input (S16): the draft plus the audit acknowledgement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessPublishIn {
    /// The draft to publish.
    pub process_id: Uuid,
    /// Publish despite `high`-confidence prompt-audit findings. Recorded on the publish's
    /// `audit_event`; a scoped, logged override, never a silent one.
    #[serde(default)]
    pub acknowledge_findings: bool,
}

/// `process_runs.start` input: one card and one published process version.
///
/// Exactly one of `process_id` or `process_key` names the version. A key resolves its
/// PUBLISHED version (a draft never runs); an id may name any row and is refused unless it is
/// published. `inputs` is the run's `{{run.inputs.*}}` scope, `{}` when omitted.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessRunStartIn {
    /// The card the run executes.
    pub card_id: Uuid,
    /// The exact process version to run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<Uuid>,
    /// A version family, by key. Alone this resolves the published version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_key: Option<String>,
    /// The run's inputs (S11 §5.4).
    #[serde(default)]
    pub inputs: Value,
    /// The parent run, when this is a CHILD (a `call` step or the coach). The parent must be live
    /// and on the same card, and `step_id` is then required (CONTRACT §21).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    /// The `process_step` that starts the child, when the PARENT is a process run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<Uuid>,
    /// The parent's `run_step` that starts the child, when the PARENT is a workflow run (0037).
    /// Exactly one of `step_id` / `workflow_step_id` is set on a child.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_step_id: Option<Uuid>,
}

/// `processes.publish` output. A second publish of the same version returns the same triple.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessPublishOut {
    /// The family version that was frozen.
    pub version: i64,
    /// The frozen `revision` row.
    pub revision_id: String,
    /// The SHA-256 of the frozen content's canonical JSON.
    pub content_hash: String,
}

/// One step as every process read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StepOut {
    /// 1-based order.
    pub position: i32,
    /// How the step executes.
    pub kind: String,
    /// Who owns the step's work.
    pub actor: String,
    /// The registry action (`computed` steps only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// The process agent an `agent` step runs under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_agent_id: Option<Uuid>,
    /// The SOP the step is judged against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub procedure_id: Option<Uuid>,
    /// The process or workflow a `call` step starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_process_key: Option<String>,
    /// The input template.
    pub input_template: Value,
    /// The gate the step parks on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_kind: Option<String>,
    /// `fail | continue | retry`.
    pub on_result: String,
    /// The retry cap.
    pub max_attempts: i32,
    /// The dot-paths this step exposes to later steps (S21 §6); empty = none.
    pub outputs: Vec<String>,
}

/// A process version as every read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcessOut {
    /// The row id.
    pub id: String,
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// Free text.
    pub description: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The version within the family (never the `ETag`).
    pub version: i64,
    /// The optimistic-concurrency counter the `ETag` carries.
    pub lock_version: i64,
    /// The card type the process drives, by code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_type_code: Option<String>,
    /// The owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    /// The user-facing deliverables a person sees when the process completes (D29).
    pub outputs: Value,
    /// The version's steps, in position order.
    pub steps: Vec<StepOut>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}
