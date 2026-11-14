#![forbid(unsafe_code)]
//! The control plane's request and response documents.
//!
//! Every type derives `serde` **and** `schemars::JsonSchema` from the same fields, so the
//! input validator, the OpenAPI operation and the MCP tool cannot disagree (S04 §5.1).
//!
//! The three shared envelopes are CONTRACT §6's, not this module's: [`PageIn`]/[`Page`] come
//! from [`crate::views`] and [`IdIn`] is re-exported from the board surface that first shipped
//! it. A slice names them; it never re-specifies their fields.
//!
//! Ids on the wire are strings (they serialise identically from `Uuid`, and an output
//! document is a representation, not a bind list); ids in an **input** are `Uuid`, so a
//! malformed id is a schema error at the edge rather than a string the store has to parse.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use crate::board::views::IdIn;
pub use crate::views::{EmptyIn, Page, PageIn};

// ── claims ──────────────────────────────────────────────────────────────────────────────

/// `cards.claim` — the `If-Match` version is the request's precondition, never a body field.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClaimIn {
    /// The card to claim, resolved under the caller's tenant.
    pub card_id: Uuid,
    /// The lease length in seconds; the store's default applies when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_s: Option<i64>,
}

/// `cards.renew` — the fence from the claim response, not the card's version.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RenewIn {
    /// The claimed card.
    pub card_id: Uuid,
    /// The `claim_fence` the holder was granted.
    pub claim_fence: i64,
}

/// `cards.release` — an absent fence releases unconditionally.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseIn {
    /// The claimed card.
    pub card_id: Uuid,
    /// The holder's fence; when present a stale fence releases nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_fence: Option<i64>,
}

/// `cards.claim` / `.renew` / `.release` — the claim state after the write.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClaimOut {
    /// The card's monotonic claim fence; a completion must present it.
    pub claim_fence: i64,
    /// The card's new version (the `ETag` of its representation).
    pub version: i64,
    /// The lease deadline; `null` when the claim was released.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<DateTime<Utc>>,
}

// ── runs ────────────────────────────────────────────────────────────────────────────────

/// `runs.create` — the card whose work this run executes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunCreateIn {
    /// The card the run executes.
    pub card_id: Uuid,
    /// Links the run to the trigger that created it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    /// The organisation binding the run executes under (T0, SPEC_V2 §4.4). When present, the
    /// run records the binding and the release it pins; a retired binding or a stale pin is
    /// refused `409 job_binding_stale` before the run is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_binding_id: Option<Uuid>,
    /// The release the caller expects the binding to pin. Must equal the binding's pin; omit to
    /// accept the binding's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_hash: Option<String>,
}

/// `runs.finish` — how the run ended.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunFinishIn {
    /// The run id.
    pub id: Uuid,
    /// `succeeded` or `failed`.
    pub outcome: String,
    /// A stable machine-readable failure class, when the run failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
    /// A human-readable failure message, when the run failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// `runs.cancel` — withdraw a run before it finishes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunCancelIn {
    /// The run id.
    pub id: Uuid,
    /// Why it was cancelled; recorded on the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `runs.list` — the queue's filters plus the shared page envelope.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunListIn {
    /// Only runs in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only runs of this card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// One run as every run action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunOut {
    /// The run id.
    pub id: String,
    /// The card it executes, when it is card-scoped.
    pub card_id: Option<String>,
    /// The owning project.
    pub project_id: String,
    /// `queued | running | waiting_approval | succeeded | failed | cancelled`.
    pub status: String,
    /// The attempt number, starting at 1.
    pub attempt: i32,
    /// A stable machine-readable failure class, when failed.
    pub error_class: Option<String>,
    /// A human-readable failure message, when failed.
    pub error_message: Option<String>,
    /// When execution started.
    pub started_at: Option<DateTime<Utc>>,
    /// When execution finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// The row version.
    pub version: i64,
    /// The process step this run is a child of, when its parent runs a process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_step_id: Option<String>,
    /// The workflow node this run is a child of, when its parent runs a workflow (0037).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_step_id: Option<String>,
    /// When the run row was created.
    pub created_at: DateTime<Utc>,
}

/// `runs.get` — the run to read.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunGetIn {
    /// The run id.
    pub id: Uuid,
}

/// One `run_step` row as `runs.get` returns it: the trace, one row per attempt.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunStepOut {
    /// The row id.
    pub id: String,
    /// The process step this row came from, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_step_id: Option<String>,
    /// 1-based order within the run.
    pub position: i32,
    /// The attempt number; a retry appends a new row.
    pub attempt: i32,
    /// The action this step invokes.
    pub action: String,
    /// `queued | running | succeeded | failed | skipped`.
    pub status: String,
    /// The resolved input.
    pub input: Value,
    /// The action's output, once it succeeded.
    pub output: Value,
    /// The failure message, when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The judge's verdict document (S13 §5.5): decision, score, reason, the criteria names it
    /// judged against and the hash of the output it judged. Absent for a step with no SOP or a
    /// gate (the human decision IS the inspection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<Value>,
    /// When the step started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When the step finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

/// The process a run executes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunProcessRef {
    /// The process version's row id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// The version within its key family.
    pub version: i64,
}

/// `runs.get` — one run with its steps, gates, card, process and inputs.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunGetOut {
    /// The run itself, in the queue's projection.
    pub run: RunOut,
    /// The run's trace, ordered by position then attempt.
    pub steps: Vec<RunStepOut>,
    /// Gates scoped to this run.
    pub approvals: Vec<ApprovalOut>,
    /// The process, when the run is process-driven.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<RunProcessRef>,
    /// This run's edge in the S13 run tree and its direct children.
    pub tree: RunTreeOut,
    /// The caller-supplied run inputs.
    pub inputs: Value,
}

/// One run's place in the run tree (S13): its parent edge and its direct children.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunTreeOut {
    /// The run that started this one; `None` for a card/project root.
    pub parent_run_id: Option<String>,
    /// The `process_step` that started this run, when it is a child.
    pub step_id: Option<String>,
    /// 0 for a root, up to 3.
    pub depth: i32,
    /// Direct children, oldest first.
    pub children: Vec<RunTreeChildOut>,
}

/// One direct child edge of a run.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunTreeChildOut {
    /// The child run.
    pub id: String,
    /// Its status.
    pub status: String,
    /// The parent's `process_step` that started it.
    pub step_id: Option<String>,
    /// Its depth (the parent's + 1).
    pub depth: i32,
}

// ── approvals ───────────────────────────────────────────────────────────────────────────

/// `approvals.decide` — one approver's decision. There is no version: a gate is decided by a
/// quorum count, so the write is status-guarded (CONTRACT §3).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalDecideIn {
    /// The approval id.
    pub approval_id: Uuid,
    /// `approve` or `reject`.
    pub decision: String,
    /// A free-text note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Per-item decisions, when the gate covers several items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_decisions: Option<Value>,
}

/// `approvals.request` — a question posed for a human to decide (S06b §5).
///
/// This is the agent's only path to a person, and it is how a project mid-run asks something
/// that no `run → waiting_approval` transition produced. At least one of `project_id`,
/// `card_id` or `run_id` is required: a gate with no parent belongs to no list anybody reads,
/// so it could only ever expire.
///
/// `gate` and `kind` are free text on the row. The convention is a
/// `catalogue_entry(kind='gate')` code, but nothing seeds that catalogue and every existing
/// gate (`open_gate_tx`'s callers) is opened with a plain name, so this action refuses a
/// **blank** name and does not fail closed on a vocabulary nobody populates.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRequestIn {
    /// The project the question belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The card the question belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// The run the question belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The gate name.
    pub gate: String,
    /// The gate's kind.
    pub kind: String,
    /// What is being asked; stored in `meta.description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The answer the requester proposes; stored in `meta.proposed_answer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_answer: Option<String>,
    /// The decisions a responder may give; stored in `meta.allowed_decisions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_decisions: Option<Value>,
    /// How many distinct approvals resolve it. Defaults to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_decisions: Option<u16>,
    /// When the expiry job may resolve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// `approvals.list` — the gate filters plus the shared page envelope.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalListIn {
    /// Only gates in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only gates of this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// Only gates of this card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// Only gates of this project — includes its cards' gates and its runs' gates (S06c's
    /// `inputRequests` reads a project's pending approvals through this filter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Only gates that sign off this eval (S19). An eval gate carries no run, card or project,
    /// so this is the only filter that can list a sign-off tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_id: Option<Uuid>,
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// One approval gate as `approvals.decide` and `.list` return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalOut {
    /// The approval id.
    pub id: String,
    /// The gated run, when run-scoped.
    pub run_id: Option<String>,
    /// The gated card, when card-scoped.
    pub card_id: Option<String>,
    /// The owning project, when known.
    pub project_id: Option<String>,
    /// The eval this gate signs off (S19), when it is a sign-off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_id: Option<String>,
    /// The gate name (`catalogue_entry(kind='gate').code`).
    pub gate: String,
    /// The gate's kind.
    pub kind: String,
    /// `pending | approved | rejected | expired | voided`.
    pub status: String,
    /// The quorum: this many distinct approvals resolve the gate.
    pub required_decisions: u16,
    /// Approvals recorded so far.
    pub approvals: u16,
    /// When the expiry job may resolve it.
    pub expires_at: Option<DateTime<Utc>>,
    /// The row version.
    pub version: i64,
    /// When the gate was requested.
    pub created_at: DateTime<Utc>,
    /// Every decision recorded on the gate, oldest first. A count cannot show who decided or
    /// through which channel, which is what the cockpit must render.
    #[serde(default)]
    pub decisions: Vec<ApprovalDecisionOut>,
}

/// One recorded decision on a gate (`approval_decision` is immutable: this is a fact).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalDecisionOut {
    /// The deciding `app_user`.
    pub approver_id: String,
    /// The approver's label: display name, else email.
    pub approver: String,
    /// `approve | reject`.
    pub decision: String,
    /// The note the decider left (`''` when absent).
    pub note: String,
    /// `human | triage_agent | rule | agent` — an agent's decision is visibly not a person's.
    pub decided_by_kind: String,
    /// The transport attribution recorded by the writer (`via.channel` and its siblings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<Value>,
    /// When the decision was recorded.
    pub decided_at: DateTime<Utc>,
}

// ── andon ───────────────────────────────────────────────────────────────────────────────

/// `andon.engage` — why the cord is being pulled.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AndonEngageIn {
    /// The reason, recorded on the row and the event.
    pub reason: String,
}

/// `andon.engage` / `.release` — the tenant's singleton andon state.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AndonOut {
    /// Whether the cord is currently pulled.
    pub engaged: bool,
    /// Who pulled it.
    pub engaged_by: Option<String>,
    /// When it was last pulled.
    pub engaged_at: Option<DateTime<Utc>>,
    /// When it was last released.
    pub released_at: Option<DateTime<Utc>>,
    /// Why it was pulled.
    pub reason: String,
    /// The row version.
    pub version: i64,
}

// ── messages and notifications ──────────────────────────────────────────────────────────

/// `messages.send` — one piece of content, fanned out to N delivery rows.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MessageSendIn {
    /// The recipients, in the channel's own address form. A send that resolves to nobody is
    /// refused.
    #[serde(default)]
    pub recipients: Vec<String>,
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// The content.
    pub body: String,
    /// `text` unless the channel defines another kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Channel-specific payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `messages.send` — the message and one delivery per recipient.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MessageSendOut {
    /// The message id.
    pub message_id: String,
    /// One id per recipient; each is a `delivery.send` dedupe key.
    pub delivery_ids: Vec<String>,
}

/// `notifications.list` — the caller's deliveries, unread first when asked.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NotificationListIn {
    /// Only deliveries the caller has not acknowledged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread: Option<bool>,
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// One delivery row as the caller's notification inbox sees it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeliveryOut {
    /// The delivery id.
    pub id: String,
    /// The message it delivers.
    pub message_id: String,
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// The channel's address for this recipient.
    pub recipient_ref: String,
    /// `pending | sent | delivered | failed | expired`.
    pub state: String,
    /// How many send attempts were made.
    pub attempts: i32,
    /// The last failure, when one was recorded.
    pub last_error: Option<String>,
    /// When the channel accepted it.
    pub sent_at: Option<DateTime<Utc>>,
    /// When the recipient was known to have received it.
    pub delivered_at: Option<DateTime<Utc>>,
    /// When an undelivered row stops being retried.
    pub expires_at: Option<DateTime<Utc>>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
}

/// `notifications.update` — one channel preference for the calling user.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NotificationUpdateIn {
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// Turn this channel on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The quiet-hours window, when the channel honours one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours: Option<Value>,
}

/// `notifications.update` — the stored preference.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NotificationPreference {
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// Whether the channel is enabled.
    pub enabled: bool,
    /// The quiet-hours window.
    pub quiet_hours: Value,
}

// ── channels ────────────────────────────────────────────────────────────────────────────

/// `channels.bind` — a tenant's channel route (platform-admin, audited).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelBindIn {
    /// The tenant the binding belongs to.
    pub tenant_id: Uuid,
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// The route selector (a project key, `default`, or the channel's own id).
    pub route_key: String,
    /// Chat id, topic id, bot reference — secrets by reference only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

/// One channel binding.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelBindingOut {
    /// The binding id.
    pub id: String,
    /// The tenant that owns it.
    pub tenant_id: String,
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// The route selector.
    pub route_key: String,
    /// The channel configuration.
    pub config: Value,
    /// Whether the binding currently receives traffic.
    pub active: bool,
}

/// `channels.unbind` — the binding to deactivate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelUnbindIn {
    /// The tenant the binding belongs to.
    pub tenant_id: Uuid,
    /// `web | telegram | mcp`.
    pub channel_kind: String,
    /// The route selector.
    pub route_key: String,
}

// ── S06d: the connector catalogue and the grant matrix ─────────────────────────────────

/// `channels.kinds` — one connector kind this build serves.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKindOut {
    /// `telegram`, `webhook`, `web`, `mcp`, …
    pub kind: String,
    /// The human label.
    pub label: String,
    /// Whether the dispatcher may offer it requests.
    pub can_send: bool,
    /// Whether the generic inbound route can receive from it.
    pub can_receive: bool,
    /// `provisioned | credential | human`.
    pub identity: String,
    /// The typed config schema the web form renders.
    pub config_schema: Value,
    /// Config fields whose value is the name of a sealed secret.
    pub secret_fields: Vec<String>,
    /// The compiled grant default for a machine kind.
    pub default_actions: Vec<String>,
    /// Onboarding instructions for the operator.
    pub instructions: String,
}

/// `channels.kinds` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelKindsOut {
    /// Every built-in kind.
    pub kinds: Vec<ChannelKindOut>,
}

/// One binding in the admin projection.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelBindingAdminOut {
    /// The binding id.
    pub id: String,
    /// The tenant that owns it.
    pub tenant_id: String,
    /// The connector kind.
    pub channel_kind: String,
    /// The route selector.
    pub route_key: String,
    /// The non-secret config (secret fields hold names, never values).
    pub config: Value,
    /// Whether the binding currently receives traffic.
    pub active: bool,
    /// The generic inbound path, when the kind can receive.
    pub inbound_url: Option<String>,
    /// Each declared secret field and whether its named secret exists.
    pub secrets_set: std::collections::BTreeMap<String, bool>,
}

/// `channels.get` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelGetIn {
    /// The binding id.
    pub binding_id: Uuid,
}

/// `channels.list` takes nothing.
pub type ChannelListIn = crate::views::EmptyIn;

/// `channels.list` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelListOut {
    /// Every binding of the tenant.
    pub bindings: Vec<ChannelBindingAdminOut>,
}

/// `channels.test` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelTestIn {
    /// The binding to exercise.
    pub binding_id: Uuid,
}

/// `channels.test` output: the queued message and delivery.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelTestOut {
    /// The queued message.
    pub message_id: String,
    /// Its delivery; its state is the proof the config works.
    pub delivery_id: String,
}

/// One cell of the connector × action matrix.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyAction {
    /// The action name.
    pub action: String,
    /// The kind's compiled default.
    pub default: bool,
    /// The effective answer (override or default), after the floor.
    pub allowed: bool,
    /// True when the tenant stored this cell.
    pub overridden: bool,
    /// False when the floor or the action's shape closes it to every machine.
    pub grantable: bool,
    /// Why it is not grantable (`human sign-off`, `administration`, …).
    pub denial: Option<String>,
}

/// One resource group in the matrix.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyGroup {
    /// The action-name resource half.
    pub resource: String,
    /// Its actions.
    pub actions: Vec<ChannelPolicyAction>,
}

/// One binding summarised beside its kind's matrix.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyBinding {
    /// The route selector.
    pub route_key: String,
    /// Whether it is live.
    pub active: bool,
    /// The inbound path, when the kind can receive.
    pub inbound_url: Option<String>,
}

/// The provisioned identity shown beside a kind.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelIdentityOut {
    /// The `app_user` id.
    pub user_id: String,
    /// The deterministic subject.
    pub subject: String,
    /// The display name.
    pub display_name: String,
}

/// One machine kind's authorization card.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyChannel {
    /// The connector kind.
    pub channel_kind: String,
    /// The human label.
    pub label: String,
    /// Whether any active binding exists.
    pub bound: bool,
    /// Its bindings.
    pub bindings: Vec<ChannelPolicyBinding>,
    /// The provisioned identity (absent for a credential-identity kind).
    pub identity: Option<ChannelIdentityOut>,
    /// The action matrix, grouped by resource.
    pub groups: Vec<ChannelPolicyGroup>,
}

/// `channels.policy_get|set|reset` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyView {
    /// One card per machine connector kind.
    pub channels: Vec<ChannelPolicyChannel>,
}

/// One override in `channels.policy_set`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyEntryIn {
    /// The action name.
    pub action: String,
    /// Whether the channel may invoke it.
    pub allowed: bool,
}

/// `channels.policy_set` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicySetIn {
    /// The machine connector kind.
    pub channel_kind: String,
    /// The complete override set for that kind; an omitted action uses the compiled default.
    pub entries: Vec<ChannelPolicyEntryIn>,
}

/// `channels.policy_reset` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPolicyResetIn {
    /// The machine connector kind.
    pub channel_kind: String,
}
