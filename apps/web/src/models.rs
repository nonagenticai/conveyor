//! Wire models for the screens.
//!
//! These mirror the server's view structs field for field; a field the UI does not read is
//! still parsed where it is cheap, so a response change is a compile error rather than a
//! silently empty panel. Every optional/default is deliberate: the server omits `None`.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

// ── identity ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct Me {
    pub principal: PrincipalView,
    /// `None` before a tenant is chosen: `/api/me` answers with the membership list so the
    /// switcher can exist, and the next request names a tenant.
    #[serde(default)]
    pub active_tenant: Option<MembershipView>,
    pub memberships: Vec<MembershipView>,
    /// Added with the RBAC delta; false against a server that predates it.
    #[serde(default)]
    pub is_super_admin: bool,
}

// ── platform (super-admin) ───────────────────────────────────────────────────────────────

/// `GET /api/platform/tenants` — one tenant row.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PlatformTenant {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub members: i64,
    pub admins: i64,
    #[serde(default)]
    pub licence: Option<PlatformLicence>,
}

/// The synced entitlement badge.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PlatformLicence {
    pub status: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
}

/// One admin of a tenant.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PlatformAdmin {
    pub user_id: String,
    #[serde(default)]
    pub email: Option<String>,
    pub display_name: String,
    pub role: String,
    pub user_status: String,
}

/// `GET /api/platform/tenants/{id}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PlatformTenantDetail {
    pub tenant: PlatformTenant,
    pub admins: Vec<PlatformAdmin>,
}

/// What provisioning did.
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformProvisioning {
    pub status: String,
    pub replayed: bool,
    pub tenant_id: String,
    #[serde(default)]
    pub license_id: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub partial_failures: Vec<String>,
}

/// `POST /api/platform/tenants` and `…/retry`.
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformCreated {
    pub tenant: PlatformTenant,
    pub admins: Vec<PlatformAdmin>,
    pub provisioning: PlatformProvisioning,
    pub email_sent: bool,
}

/// `POST …/invite-admin`.
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformInvite {
    pub invite_id: String,
    pub email: String,
    pub email_sent: bool,
}

// ── billing ──────────────────────────────────────────────────────────────────────────────

/// One upstream source's answer.
#[derive(Clone, Debug, Deserialize)]
pub struct BillingSource {
    pub available: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
}

/// `GET /api/tenants/{id}/billing`.
#[derive(Clone, Debug, Deserialize)]
pub struct TenantBilling {
    pub tenant_id: String,
    pub month: String,
    pub billing: BillingSource,
    pub costs: BillingSource,
    pub actors: BillingSource,
}

/// `invites.create|list|revoke` — one pending invitation.
#[derive(Clone, Debug, Deserialize)]
pub struct InviteView {
    pub id: String,
    pub email: String,
    pub role: String,
    pub status: String,
    #[serde(default)]
    pub invited_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub version: i64,
}

/// One cell of the role × scope matrix (`roles.policy_get`).
#[derive(Clone, Debug, Deserialize)]
pub struct PolicyCell {
    pub role: String,
    pub scope: String,
    pub allowed: bool,
    pub default: bool,
    pub overridden: bool,
}

/// `roles.policy_get|policy_set|policy_reset` — the whole matrix.
#[derive(Clone, Debug, Deserialize)]
pub struct PolicyView {
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub cells: Vec<PolicyCell>,
}

/// `members.remove` / `invites.revoke` — the acknowledgement.
#[derive(Clone, Debug, Deserialize)]
pub struct Removed {
    pub user_id: String,
    pub removed: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PrincipalView {
    pub id: String,
    pub subject: String,
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MembershipView {
    pub tenant_id: String,
    pub slug: String,
    pub name: String,
    pub role: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemberView {
    pub user_id: String,
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    pub role: String,
    #[serde(default)]
    pub department_id: Option<String>,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

// ── board ────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectOut {
    pub id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub status: String,
    #[serde(default)]
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardOut {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectView {
    pub project: ProjectOut,
    pub boards: Vec<BoardOut>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListOut {
    pub id: String,
    pub board_id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub wip_limit: Option<i32>,
    #[serde(default)]
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardListOut {
    pub list: ListOut,
    #[serde(default)]
    pub card_count: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LabelOut {
    pub id: String,
    pub board_id: String,
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardMemberOut {
    pub id: String,
    pub board_id: String,
    pub user_id: String,
    pub display_name: String,
    pub role: String,
    #[serde(default)]
    pub can_comment: bool,
    #[serde(default)]
    pub watching: bool,
    #[serde(default)]
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardView {
    pub board: BoardOut,
    pub lists: Vec<BoardListOut>,
    #[serde(default)]
    pub labels: Vec<LabelOut>,
    #[serde(default)]
    pub fields: Vec<Value>,
    #[serde(default)]
    pub members: Vec<BoardMemberOut>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CardOut {
    pub id: String,
    pub board_id: String,
    pub list_id: String,
    pub card_type_code: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub parent_card_id: Option<String>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due_done_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub assignee_id: Option<String>,
    #[serde(default)]
    pub prev_list_id: Option<String>,
    #[serde(default)]
    pub list_changed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CommentOut {
    pub id: String,
    pub card_id: String,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default)]
    pub author_name: Option<String>,
    pub body: String,
    #[serde(default)]
    pub edited_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CardView {
    pub card: CardOut,
    #[serde(default)]
    pub labels: Vec<LabelOut>,
    #[serde(default)]
    pub comments: Vec<CommentOut>,
    #[serde(default)]
    pub task_lists: Vec<Value>,
    #[serde(default)]
    pub custom_field_values: Vec<Value>,
}

// ── runs and approvals ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct RunOut {
    pub id: String,
    #[serde(default)]
    pub card_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub attempt: i32,
    #[serde(default)]
    pub error_class: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApprovalOut {
    pub id: String,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub card_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub eval_id: Option<String>,
    pub gate: String,
    pub kind: String,
    pub status: String,
    #[serde(default)]
    pub required_decisions: u16,
    #[serde(default)]
    pub approvals: u16,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Every decision recorded on the gate, oldest first (the S12 read).
    #[serde(default)]
    pub decisions: Vec<ApprovalDecision>,
}

/// One recorded decision on a gate: who, what kind of actor, and through which channel.
#[derive(Clone, Debug, Deserialize)]
pub struct ApprovalDecision {
    pub approver_id: String,
    /// The approver's label: display name, else email.
    pub approver: String,
    /// `approve | reject`.
    pub decision: String,
    #[serde(default)]
    pub note: String,
    /// `human | triage_agent | rule | agent`.
    pub decided_by_kind: String,
    /// The transport attribution the writer recorded.
    #[serde(default)]
    pub via: Option<Value>,
    pub decided_at: DateTime<Utc>,
}

// ── measure: the inference ledger (S16) ──────────────────────────────────────────────────

/// `inference_usage.report` — one row per bucket of the requested dimension.
#[derive(Clone, Debug, Deserialize)]
pub struct UsageReport {
    pub group_by: String,
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    #[serde(default)]
    pub rows: Vec<UsageRow>,
}

/// One report row. `hit_ratio` is the server's `hits / calls`; the screen shows the design's
/// `hits ÷ (hits + misses)` and keeps this field for the Advanced inspector.
#[derive(Clone, Debug, Deserialize)]
pub struct UsageRow {
    pub bucket: String,
    #[serde(default)]
    pub calls: i64,
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub hits: i64,
    #[serde(default)]
    pub writes: i64,
    #[serde(default)]
    pub misses: i64,
    #[serde(default)]
    pub unknown: i64,
    #[serde(default)]
    pub hit_ratio: f64,
}

/// One `inference_call` ledger row. The fields the screen does not show are still parsed where
/// they are cheap, so a response change is a compile error rather than a silent blank.
#[derive(Clone, Debug, Deserialize)]
pub struct InferenceCall {
    pub id: String,
    #[serde(default)]
    pub action_invocation_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub caller_kind: String,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub run_step_id: Option<String>,
    #[serde(default)]
    pub conversation_turn_id: Option<String>,
    #[serde(default)]
    pub prompt_ref: Option<String>,
    #[serde(default)]
    pub prompt_version: Option<String>,
    #[serde(default)]
    pub prompt_sha256: Option<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub served_model: Option<String>,
    #[serde(default)]
    pub prompt_tokens: i32,
    #[serde(default)]
    pub completion_tokens: i32,
    #[serde(default)]
    pub total_tokens: i32,
    #[serde(default)]
    pub cache_creation_input_tokens: i32,
    #[serde(default)]
    pub cache_read_input_tokens: i32,
    #[serde(default)]
    pub cache_status: String,
    #[serde(default)]
    pub latency_ms: i32,
    #[serde(default)]
    pub outcome: String,
    pub created_at: DateTime<Utc>,
}

// ── measure: prompt & process audit (S16) ──────────────────────────────────────────────

/// `tenants.get` — the tenant row carrying the settings document the cadence lives in.
#[derive(Clone, Debug, Deserialize)]
pub struct TenantView {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub settings: Value,
    #[serde(default)]
    pub version: i64,
}

/// `processes.list` — the fields the audit's process picker reads.
#[derive(Clone, Debug, Deserialize)]
pub struct ProcessSummary {
    pub id: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub version: i64,
}

/// `prompt_audits.list|get` — one audit.
#[derive(Clone, Debug, Deserialize)]
pub struct PromptAudit {
    pub id: String,
    /// `{surfaces: [...], process_ids: [...]}`.
    #[serde(default)]
    pub scope: Value,
    #[serde(default)]
    pub target_model: String,
    #[serde(default)]
    pub prompt_ref: String,
    pub status: String,
    #[serde(default)]
    pub finding_count: i64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub requested_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// `prompt_audits.get` — the audit and its findings (server-capped at 200).
#[derive(Clone, Debug, Deserialize)]
pub struct PromptAuditDetail {
    pub audit: PromptAudit,
    #[serde(default)]
    pub findings: Vec<PromptFinding>,
}

/// `prompt_findings.list|decide` — one finding.
#[derive(Clone, Debug, Deserialize)]
pub struct PromptFinding {
    pub id: String,
    #[serde(default)]
    pub audit_id: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub why_obsolete: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub action: String,
    pub status: String,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub decided_by: Option<String>,
    #[serde(default)]
    pub decided_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// `prompt_audits.check` — a synchronous pre-publish check; nothing is persisted.
#[derive(Clone, Debug, Deserialize)]
pub struct PromptCheck {
    #[serde(default)]
    pub findings: Vec<CheckFinding>,
    #[serde(default)]
    pub blocking: bool,
}

/// One finding of a pre-publish check.
#[derive(Clone, Debug, Deserialize)]
pub struct CheckFinding {
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub why_obsolete: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub replacement: Option<String>,
}

// ── benchmarks (S17) ─────────────────────────────────────────────────────────────────────

/// `benchmark_suites.list` — one suite.
#[derive(Clone, Debug, Deserialize)]
pub struct BenchmarkSuite {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub dataset: String,
    #[serde(default)]
    pub dataset_revision: String,
    #[serde(default)]
    pub seed_sha256: String,
    #[serde(default)]
    pub image_lock_sha256: String,
    #[serde(default)]
    pub swebench_version: String,
    #[serde(default)]
    pub parser_version: String,
    #[serde(default)]
    pub instance_count: i64,
    #[serde(default)]
    pub baseline_run_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// `benchmark_runs.*` — one run.
#[derive(Clone, Debug, Deserialize)]
pub struct BenchmarkRun {
    pub id: String,
    #[serde(default)]
    pub suite_id: String,
    #[serde(default)]
    pub arm: String,
    #[serde(default)]
    pub subject_kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub model: Option<String>,
    pub status: String,
    #[serde(default)]
    pub n: i32,
    #[serde(default)]
    pub n_graded: i32,
    #[serde(default)]
    pub n_resolved: i32,
    #[serde(default)]
    pub n_no_attempt: i32,
    #[serde(default)]
    pub counts: Value,
    #[serde(default)]
    pub control_ok: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// `benchmark_cases.list` — one case.
#[derive(Clone, Debug, Deserialize)]
pub struct BenchmarkCase {
    pub id: String,
    #[serde(default)]
    pub benchmark_run_id: String,
    #[serde(default)]
    pub instance_id: String,
    #[serde(default)]
    pub produce_status: String,
    #[serde(default)]
    pub attempts: i32,
    #[serde(default)]
    pub patch: Option<String>,
    #[serde(default)]
    pub patch_sha256: Option<String>,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default)]
    pub sandbox_ref: Option<String>,
    #[serde(default)]
    pub log_document_id: Option<String>,
    #[serde(default)]
    pub detail: Value,
}

/// `benchmark_runs.compare` — the paired answer, or the reason it refuses.
#[derive(Clone, Debug, Deserialize)]
pub struct BenchmarkCompare {
    pub comparable: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub both: Option<i64>,
    #[serde(default)]
    pub neither: Option<i64>,
    #[serde(default)]
    pub a_only: Option<i64>,
    #[serde(default)]
    pub b_only: Option<i64>,
    #[serde(default)]
    pub discordant: Option<i64>,
    #[serde(default)]
    pub p_value: Option<f64>,
    #[serde(default)]
    pub missing_in_one: Option<i64>,
}

// ── evals (S19/S20) ─────────────────────────────────────────────────────────────────────

/// `evals.list|get` — one eval.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub subject_kind: String,
    #[serde(default)]
    pub entry_action: String,
    #[serde(default)]
    pub model_ref: Option<String>,
    #[serde(default)]
    pub prompt_ref: Option<String>,
    #[serde(default)]
    pub inputs_schema: Value,
    #[serde(default)]
    pub trace_source: String,
    pub status: String,
    #[serde(default)]
    pub current_version_id: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// `eval_versions.list` — one frozen instrument version.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalVersion {
    pub id: String,
    #[serde(default)]
    pub eval_id: String,
    #[serde(default)]
    pub version: i32,
    #[serde(default)]
    pub digest: String,
    #[serde(default)]
    pub split_sha256: Option<String>,
    #[serde(default)]
    pub split_locked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub prices: Value,
    #[serde(default)]
    pub harness_paths: Vec<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// `eval_cases.list` — one case.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalCase {
    pub id: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub ordinal: i32,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub gold_provenance: String,
    #[serde(default)]
    pub gold_model: Option<String>,
    #[serde(default)]
    pub split: String,
    #[serde(default)]
    pub meta: Value,
}

/// `eval_runs.list|get` — one measurement.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalRun {
    pub id: String,
    #[serde(default)]
    pub eval_id: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub mode: String,
    pub status: String,
    #[serde(default)]
    pub round: i32,
    #[serde(default)]
    pub reps: i32,
    #[serde(default)]
    pub case_count: i32,
    #[serde(default)]
    pub cost_ceiling_cents: Option<i32>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// `eval_results.list` — one graded slot.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalResult {
    pub id: String,
    #[serde(default)]
    pub eval_run_id: String,
    #[serde(default)]
    pub case_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub split: String,
    #[serde(default)]
    pub rep: i32,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub latency_ms: i32,
    #[serde(default)]
    pub tool_calls: i32,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub judge_model: Option<String>,
    #[serde(default)]
    pub human_score: Option<f64>,
    /// `metric_key → score`; the runner writes one entry per metric.
    #[serde(default)]
    pub grade: Value,
    #[serde(default)]
    pub explanation: Value,
    #[serde(default)]
    pub usage: Value,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// `eval_results.get` — the result plus its recorded transcript.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalResultDetail {
    pub result: EvalResult,
    #[serde(default)]
    pub turns: Option<Value>,
    #[serde(default)]
    pub trace_sha256: Option<String>,
}

/// `eval_climbs.list|get` — one climb, with its rounds on `get`.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalClimb {
    pub id: String,
    #[serde(default)]
    pub eval_id: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub stopping: String,
    pub status: String,
    #[serde(default)]
    pub goal: Value,
    #[serde(default)]
    pub reps: i32,
    #[serde(default)]
    pub plateau_k: i32,
    #[serde(default)]
    pub current_round: i32,
    #[serde(default)]
    pub max_rounds: Option<i32>,
    #[serde(default)]
    pub best_round: Option<i32>,
    #[serde(default)]
    pub best_test_score: Option<f64>,
    #[serde(default)]
    pub spend_cents: Option<f64>,
    #[serde(default)]
    pub spend_unit: Option<String>,
    #[serde(default)]
    pub delta: f64,
    #[serde(default)]
    pub off_limits: Vec<String>,
    #[serde(default)]
    pub approve_each_round: bool,
    #[serde(default)]
    pub narrative_md: Option<String>,
    #[serde(default)]
    pub rounds: Vec<ClimbRound>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// One round of a climb (`eval_run` rows).
#[derive(Clone, Debug, Deserialize)]
pub struct ClimbRound {
    #[serde(default)]
    pub round: i32,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub train: Option<f64>,
    #[serde(default)]
    pub test: Option<f64>,
    #[serde(default)]
    pub delta: Option<f64>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub change_md: Option<String>,
}

// ── run detail (S12 part D) ─────────────────────────────────────────────────────────────

/// One `run_step` row.
#[derive(Clone, Debug, Deserialize)]
pub struct RunStep {
    pub id: String,
    #[serde(default)]
    pub process_step_id: Option<String>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub attempt: i32,
    #[serde(default)]
    pub action: String,
    pub status: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

/// The process a run executes.
#[derive(Clone, Debug, Deserialize)]
pub struct RunProcessRef {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub version: i64,
}

/// `runs.get` — one run with its trace, gates, process, tree and inputs.
#[derive(Clone, Debug, Deserialize)]
pub struct RunGet {
    pub run: RunOut,
    #[serde(default)]
    pub steps: Vec<RunStep>,
    #[serde(default)]
    pub approvals: Vec<ApprovalOut>,
    #[serde(default)]
    pub process: Option<RunProcessRef>,
    /// The run's place in the S13 run tree and its direct children.
    #[serde(default)]
    pub tree: RunTree,
    #[serde(default)]
    pub inputs: Value,
}

/// One run's tree edge: its parent, the step that started it, its depth and its children.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RunTree {
    #[serde(default)]
    pub parent_run_id: Option<String>,
    #[serde(default)]
    pub step_id: Option<String>,
    #[serde(default)]
    pub depth: i32,
    #[serde(default)]
    pub children: Vec<RunTreeChild>,
}

/// One direct child edge of a run.
#[derive(Clone, Debug, Deserialize)]
pub struct RunTreeChild {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub step_id: Option<String>,
    #[serde(default)]
    pub depth: i32,
}
