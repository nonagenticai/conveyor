#![forbid(unsafe_code)]
//! The prompt-audit surface's request and response documents (S16).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::views::PageIn;

/// `prompt_audits.start`'s scope.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuditScopeIn {
    /// `processes | actions | copilot`; at least one.
    pub surfaces: Vec<String>,
    /// Restrict the process surface to these versions; empty means every version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process_ids: Vec<Uuid>,
}

/// `prompt_audits.start` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditStartIn {
    /// What to inventory.
    pub scope: AuditScopeIn,
    /// The model the audit calls.
    pub target_model: String,
}

/// One audit.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditView {
    /// The row id.
    pub id: Uuid,
    /// The requested scope.
    pub scope: Value,
    /// The model the audit calls.
    pub target_model: String,
    /// The prompt family (`prompt_audit@1`).
    pub prompt_ref: String,
    /// `queued | running | succeeded | failed`.
    pub status: String,
    /// Findings persisted by a successful run.
    pub finding_count: i32,
    /// Why a failed run failed.
    pub error: Option<String>,
    /// The human who started it; `null` for the event-created audit.
    pub requested_by: Option<Uuid>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// `prompt_audits.get` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditGetIn {
    /// The audit id.
    pub id: Uuid,
}

/// One audit plus its findings.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditGetOut {
    /// The audit row.
    pub audit: PromptAuditView,
    /// Its findings, newest first.
    pub findings: Vec<FindingView>,
}

/// `prompt_audits.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditListIn {
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `prompt_audits.check` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditCheckIn {
    /// The draft to scan; the same identity `processes.publish` uses (S16 CCR-1).
    pub process_id: Uuid,
}

/// One deterministic finding, before it has an id or a decision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CheckFindingView {
    /// `process:<id>` | `process_step:<id>`.
    pub location: String,
    /// The text the finding is about.
    pub evidence: String,
    /// The anti-pattern code (`1a`–`1f`, `2`, `3`, `4`).
    pub pattern: String,
    /// Why the text is obsolete.
    pub why_obsolete: String,
    /// `high | medium | low`; only `high` blocks.
    pub confidence: String,
    /// The proposed action.
    pub action: String,
    /// The replacement, for `rewrite`/`add`.
    pub replacement: Option<String>,
}

/// The synchronous draft check's answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptAuditCheckOut {
    /// Every deterministic finding on the draft.
    pub findings: Vec<CheckFindingView>,
    /// True when at least one finding is `high` (publishing refuses without acknowledgement).
    pub blocking: bool,
}

/// One persisted finding.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FindingView {
    /// The row id.
    pub id: Uuid,
    /// The owning audit.
    pub audit_id: Uuid,
    /// Where the finding applies.
    pub location: String,
    /// The text it is about.
    pub evidence: String,
    /// The anti-pattern code.
    pub pattern: String,
    /// Why it is obsolete.
    pub why_obsolete: String,
    /// `high | medium | low`.
    pub confidence: String,
    /// What the audit proposes.
    pub action: String,
    /// The proposed replacement.
    pub replacement: Option<String>,
    /// `open | accepted | rejected | applied`.
    pub status: String,
    /// The person who decided.
    pub decided_by: Option<Uuid>,
    /// When they decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// `prompt_findings.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PromptFindingListIn {
    /// Restrict to one audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<Uuid>,
    /// `open | accepted | rejected | applied`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// `high | medium | low`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `prompt_findings.decide` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromptFindingDecideIn {
    /// The finding to decide.
    pub finding_id: Uuid,
    /// `accept | reject`.
    pub decision: String,
}
