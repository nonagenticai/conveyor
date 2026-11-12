//! The prompt audit's vocabulary and state machines (S16, D23).
//!
//! Pure: the audit's four statuses, the finding's decision machine, the tenant's cadence policy and
//! the surface selection. The store writes rows the functions here agreed to; the database CHECKs
//! guard the same vocabulary.
//!
//! **Findings are inert without a person.** A finding moves `open → accepted | rejected`; only an
//! ordinary `processes.update` that applies an accepted rewrite may later mark it `applied`. The
//! audit never holds `processes:write` over the surface it audits (CONTRACT §18).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The audit's four statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStatus {
    /// Accepted, not started (the row and its job exist).
    Queued,
    /// The one bounded model call is in flight.
    Running,
    /// Findings persisted; terminal.
    Succeeded,
    /// The run failed; `prompt_audit.error` says why; terminal.
    Failed,
}

impl AuditStatus {
    /// Every status.
    pub const ALL: [AuditStatus; 4] = [
        AuditStatus::Queued,
        AuditStatus::Running,
        AuditStatus::Succeeded,
        AuditStatus::Failed,
    ];

    /// The stored `prompt_audit.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            AuditStatus::Queued => "queued",
            AuditStatus::Running => "running",
            AuditStatus::Succeeded => "succeeded",
            AuditStatus::Failed => "failed",
        }
    }

    /// Read a stored status.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == raw)
    }

    /// True for `succeeded | failed`.
    pub fn is_terminal(self) -> bool {
        matches!(self, AuditStatus::Succeeded | AuditStatus::Failed)
    }

    /// The documented transition table.
    pub fn may_transition_to(self, to: AuditStatus) -> bool {
        matches!(
            (self, to),
            (AuditStatus::Queued, AuditStatus::Running)
                | (AuditStatus::Queued, AuditStatus::Failed)
                | (AuditStatus::Running, AuditStatus::Succeeded)
                | (AuditStatus::Running, AuditStatus::Failed)
        )
    }
}

/// One prompt surface the audit can inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Process step templates and the process's own title/description.
    Processes,
    /// Action descriptions — **declared, not yet auditable**: `ActionSpec` carries no authored
    /// description today, so this surface yields no findings (S16 CCR-7).
    Actions,
    /// The copilot's system prompt.
    Copilot,
}

impl Surface {
    /// Every surface.
    pub const ALL: [Surface; 3] = [Surface::Processes, Surface::Actions, Surface::Copilot];

    /// The stored spelling (also the action's input).
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Processes => "processes",
            Surface::Actions => "actions",
            Surface::Copilot => "copilot",
        }
    }

    /// Read a requested surface.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|surface| surface.as_str() == raw)
    }
}

/// `prompt_audits.start`'s `scope` object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditScope {
    /// The surfaces to inventory; never empty.
    pub surfaces: Vec<Surface>,
    /// The process versions to restrict the process surface to; empty means every draft and
    /// published version the tenant holds.
    pub process_ids: Vec<Uuid>,
}

/// Why a scope was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScopeError {
    /// No surfaces were named.
    #[error("scope.surfaces must name at least one surface")]
    Empty,
    /// A surface outside the closed vocabulary.
    #[error("{0:?} is not a prompt surface")]
    UnknownSurface(String),
    /// A `process_ids` member that is not a UUID.
    #[error("{0:?} is not a process id")]
    BadProcessId(String),
}

impl AuditScope {
    /// Parse `{"surfaces": [...], "process_ids": [...]}`.
    pub fn from_value(value: &Value) -> Result<Self, ScopeError> {
        let surfaces = value
            .get("surfaces")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .and_then(Surface::parse)
                            .ok_or_else(|| ScopeError::UnknownSurface(item.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        if surfaces.is_empty() {
            return Err(ScopeError::Empty);
        }
        let process_ids = value
            .get("process_ids")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .and_then(|raw| Uuid::parse_str(raw).ok())
                            .ok_or_else(|| ScopeError::BadProcessId(item.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            surfaces,
            process_ids,
        })
    }

    /// The stored `prompt_audit.scope` document.
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "surfaces": self.surfaces.iter().map(|surface| surface.as_str()).collect::<Vec<_>>(),
            "process_ids": self.process_ids,
        })
    }
}

/// `tenant.settings.prompt_audit`, the cadence policy.
///
/// `target_model` is an S16 addition: an event-created audit has no caller to supply the model the
/// DDL requires, so the policy names it. An enabled policy without one is *not armed* — the
/// capture seam logs and skips; it never guesses a model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditSettings {
    /// Whether the after-N-executions trigger is on.
    pub enabled: bool,
    /// The threshold, in captured calls since the tenant's last audit.
    pub after_executions: Option<u32>,
    /// The surfaces an automatic audit inventories.
    pub surfaces: Vec<Surface>,
    /// The model the automatic audit calls.
    pub target_model: Option<String>,
}

impl AuditSettings {
    /// Read the policy out of a tenant's `settings` document. Absent or malformed is disabled.
    pub fn from_settings(settings: &Value) -> Self {
        let Some(policy) = settings.get("prompt_audit").and_then(Value::as_object) else {
            return Self::default();
        };
        let enabled = policy
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let after_executions = policy
            .get("after_executions")
            .and_then(Value::as_u64)
            .and_then(|raw| u32::try_from(raw).ok())
            .filter(|raw| *raw > 0);
        let surfaces = policy
            .get("surfaces")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(Surface::parse)
                    .collect()
            })
            .unwrap_or_default();
        let target_model = policy
            .get("target_model")
            .and_then(Value::as_str)
            .filter(|raw| !raw.trim().is_empty())
            .map(str::to_owned);
        Self {
            enabled,
            after_executions,
            surfaces,
            target_model,
        }
    }

    /// The model an automatic audit should call, when the policy is armed and `calls` have been
    /// captured since the last audit. `None` is "do not start one" — never an error.
    pub fn due(&self, calls: i64) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        let threshold = i64::from(self.after_executions?);
        if calls < threshold {
            return None;
        }
        self.target_model.as_deref()
    }

    /// The scope an automatic audit records; `processes` is the surface that can be audited today.
    pub fn automatic_scope(&self) -> AuditScope {
        let surfaces = if self.surfaces.is_empty() {
            vec![Surface::Processes]
        } else {
            self.surfaces.clone()
        };
        AuditScope {
            surfaces,
            process_ids: Vec::new(),
        }
    }
}

/// The `claude-api` prompt-audit method's pattern vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pattern {
    /// Group 1a: a dated or obsolete instruction.
    #[serde(rename = "1a")]
    DatedInstruction,
    /// Group 1b.
    #[serde(rename = "1b")]
    RedundantConstraint,
    /// Group 1c.
    #[serde(rename = "1c")]
    StaleModelName,
    /// Group 1d.
    #[serde(rename = "1d")]
    ObsoleteApiFeature,
    /// Group 1e.
    #[serde(rename = "1e")]
    ContradictoryRule,
    /// Group 1f.
    #[serde(rename = "1f")]
    OverSpecifiedFormat,
    /// Group 2.
    #[serde(rename = "2")]
    WrongAltitude,
    /// Group 3.
    #[serde(rename = "3")]
    MissingStructure,
    /// Group 4.
    #[serde(rename = "4")]
    CacheHostileOrder,
}

impl Pattern {
    /// Every pattern.
    pub const ALL: [Pattern; 9] = [
        Pattern::DatedInstruction,
        Pattern::RedundantConstraint,
        Pattern::StaleModelName,
        Pattern::ObsoleteApiFeature,
        Pattern::ContradictoryRule,
        Pattern::OverSpecifiedFormat,
        Pattern::WrongAltitude,
        Pattern::MissingStructure,
        Pattern::CacheHostileOrder,
    ];

    /// The stored `prompt_audit_finding.pattern`.
    pub fn as_str(self) -> &'static str {
        match self {
            Pattern::DatedInstruction => "1a",
            Pattern::RedundantConstraint => "1b",
            Pattern::StaleModelName => "1c",
            Pattern::ObsoleteApiFeature => "1d",
            Pattern::ContradictoryRule => "1e",
            Pattern::OverSpecifiedFormat => "1f",
            Pattern::WrongAltitude => "2",
            Pattern::MissingStructure => "3",
            Pattern::CacheHostileOrder => "4",
        }
    }

    /// Read a stored pattern.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|pattern| pattern.as_str() == raw)
    }
}

/// The finding's confidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Blocks publishing unless acknowledged.
    High,
    /// Reported, never blocking.
    Medium,
    /// Reported, never blocking.
    Low,
}

impl Confidence {
    /// Every confidence.
    pub const ALL: [Confidence; 3] = [Confidence::High, Confidence::Medium, Confidence::Low];

    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }

    /// Read a stored confidence.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == raw)
    }
}

/// What the audit proposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingAction {
    /// Delete the text.
    Remove,
    /// Replace it with `replacement`.
    Rewrite,
    /// Move it where it belongs.
    Move,
    /// Replace it with a provider feature.
    ReplaceWithApiFeature,
    /// Add the missing text in `replacement`.
    Add,
    /// Flag it for a person; no proposed diff.
    Flag,
}

impl FindingAction {
    /// Every action.
    pub const ALL: [FindingAction; 6] = [
        FindingAction::Remove,
        FindingAction::Rewrite,
        FindingAction::Move,
        FindingAction::ReplaceWithApiFeature,
        FindingAction::Add,
        FindingAction::Flag,
    ];

    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingAction::Remove => "remove",
            FindingAction::Rewrite => "rewrite",
            FindingAction::Move => "move",
            FindingAction::ReplaceWithApiFeature => "replace_with_api_feature",
            FindingAction::Add => "add",
            FindingAction::Flag => "flag",
        }
    }

    /// Read a stored action.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == raw)
    }

    /// `rewrite` and `add` always carry replacement text (the DDL's CHECK).
    pub fn needs_replacement(self) -> bool {
        matches!(self, FindingAction::Rewrite | FindingAction::Add)
    }
}

/// The finding's decision state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    /// Nobody has decided.
    Open,
    /// A person accepted the proposal; an ordinary `processes.update` may apply it.
    Accepted,
    /// A person rejected it.
    Rejected,
    /// The proposed text has been applied to the surface.
    Applied,
}

impl FindingStatus {
    /// Every status.
    pub const ALL: [FindingStatus; 4] = [
        FindingStatus::Open,
        FindingStatus::Accepted,
        FindingStatus::Rejected,
        FindingStatus::Applied,
    ];

    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingStatus::Open => "open",
            FindingStatus::Accepted => "accepted",
            FindingStatus::Rejected => "rejected",
            FindingStatus::Applied => "applied",
        }
    }

    /// Read a stored status.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == raw)
    }

    /// `open → accepted | rejected`, `accepted → applied`. Nothing leaves a terminal state.
    pub fn may_transition_to(self, to: FindingStatus) -> bool {
        matches!(
            (self, to),
            (FindingStatus::Open, FindingStatus::Accepted)
                | (FindingStatus::Open, FindingStatus::Rejected)
                | (FindingStatus::Accepted, FindingStatus::Applied)
        )
    }
}

/// One finding the scanner (or the model) produced, before it has an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingDraft {
    /// `process:<id>` | `process_step:<id>` | `action:<name>` | `copilot:SYSTEM_PROMPT`.
    pub location: String,
    /// The text the finding is about.
    pub evidence: String,
    /// The anti-pattern code.
    pub pattern: Pattern,
    /// Why the text is obsolete.
    pub why_obsolete: String,
    /// How sure the audit is.
    pub confidence: Confidence,
    /// What it proposes.
    pub action: FindingAction,
    /// The replacement, required for `rewrite`/`add`.
    pub replacement: Option<String>,
}

impl FindingDraft {
    /// The DDL's replacement invariant, enforced before the insert so a bad model answer is a
    /// refusal rather than a 500.
    pub fn validate(&self) -> Result<(), FindingError> {
        if self.action.needs_replacement()
            && self
                .replacement
                .as_deref()
                .is_none_or(|value| value.is_empty())
        {
            return Err(FindingError::MissingReplacement);
        }
        Ok(())
    }
}

/// Why a finding draft was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FindingError {
    /// A `rewrite`/`add` proposal with no replacement text.
    #[error("a rewrite/add finding needs replacement text")]
    MissingReplacement,
    /// A location outside the four documented prefixes.
    #[error("{0:?} is not a finding location")]
    BadLocation(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audit_transitions_are_the_four_state_machine() {
        assert!(AuditStatus::Queued.may_transition_to(AuditStatus::Running));
        assert!(AuditStatus::Queued.may_transition_to(AuditStatus::Failed));
        assert!(AuditStatus::Running.may_transition_to(AuditStatus::Succeeded));
        assert!(AuditStatus::Running.may_transition_to(AuditStatus::Failed));
        assert!(!AuditStatus::Succeeded.may_transition_to(AuditStatus::Running));
        assert!(!AuditStatus::Failed.may_transition_to(AuditStatus::Queued));
        assert!(!AuditStatus::Queued.may_transition_to(AuditStatus::Succeeded));
    }

    #[test]
    fn finding_decisions_never_leave_a_terminal_state() {
        assert!(FindingStatus::Open.may_transition_to(FindingStatus::Accepted));
        assert!(FindingStatus::Open.may_transition_to(FindingStatus::Rejected));
        assert!(FindingStatus::Accepted.may_transition_to(FindingStatus::Applied));
        assert!(!FindingStatus::Rejected.may_transition_to(FindingStatus::Applied));
        assert!(!FindingStatus::Applied.may_transition_to(FindingStatus::Open));
    }

    #[test]
    fn settings_are_disabled_by_default_and_armed_only_with_a_model() {
        let absent = AuditSettings::from_settings(&json!({}));
        assert!(!absent.enabled);
        assert_eq!(absent.due(1_000), None);

        let incomplete = AuditSettings::from_settings(&json!({
            "prompt_audit": {"enabled": true, "after_executions": 5}
        }));
        assert_eq!(incomplete.due(10), None, "no target_model is not armed");

        let armed = AuditSettings::from_settings(&json!({
            "prompt_audit": {
                "enabled": true,
                "after_executions": 5,
                "surfaces": ["processes"],
                "target_model": "audit-model"
            }
        }));
        assert_eq!(armed.due(4), None);
        assert_eq!(armed.due(5), Some("audit-model"));
        assert_eq!(armed.automatic_scope().surfaces, vec![Surface::Processes]);
    }

    #[test]
    fn scope_requires_a_surface_and_rejects_unknown_ones() {
        assert_eq!(
            AuditScope::from_value(&json!({"surfaces": []})),
            Err(ScopeError::Empty)
        );
        assert!(matches!(
            AuditScope::from_value(&json!({"surfaces": ["nope"]})),
            Err(ScopeError::UnknownSurface(_))
        ));
        let scope = AuditScope::from_value(&json!({
            "surfaces": ["processes", "copilot"]
        }))
        .expect("scope");
        assert_eq!(scope.surfaces, vec![Surface::Processes, Surface::Copilot]);
    }

    #[test]
    fn rewrite_and_add_require_replacement_text() {
        let mut draft = FindingDraft {
            location: "process_step:00000000-0000-0000-0000-000000000000".to_owned(),
            evidence: "old".to_owned(),
            pattern: Pattern::DatedInstruction,
            why_obsolete: "dated".to_owned(),
            confidence: Confidence::High,
            action: FindingAction::Rewrite,
            replacement: None,
        };
        assert!(draft.validate().is_err());
        draft.replacement = Some("new".to_owned());
        assert!(draft.validate().is_ok());
        draft.action = FindingAction::Flag;
        draft.replacement = None;
        assert!(draft.validate().is_ok());
    }
}
