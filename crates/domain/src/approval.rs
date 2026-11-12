//! The approval gate and its decisions.
//!
//! At most one *pending* approval exists per `(run, gate)` — enforced by the partial unique
//! index, not by a check-then-insert. Dual control is `required_decisions`, never a second
//! gate: a gate with `required_decisions = 2` stays pending until two distinct approvers
//! approve, and any reject resolves it immediately. `uq (approval_id, approver_id)` is what
//! stops one person satisfying dual control twice.
//!
//! `decided_by_kind` and `via` are carried from fr migrations 211/219 (SPEC D18): an agent
//! decision must be distinguishable from a human one after the fact.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Timestamp;
use crate::ids::Version;

/// The five approval statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Awaiting decisions.
    Pending,
    /// The quorum was reached.
    Approved,
    /// Any reject resolves the gate.
    Rejected,
    /// The expiry job ran at `expires_at`.
    Expired,
    /// Withdrawn by `runs.cancel`; it will never be answered.
    Voided,
}

impl ApprovalStatus {
    /// Every status.
    pub const ALL: [ApprovalStatus; 5] = [
        ApprovalStatus::Pending,
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::Expired,
        ApprovalStatus::Voided,
    ];

    /// The stored `approval.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Expired => "expired",
            ApprovalStatus::Voided => "voided",
        }
    }

    /// True unless the gate is still awaiting decisions.
    pub fn is_resolved(self) -> bool {
        self != ApprovalStatus::Pending
    }
}

impl fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not an approval status.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown approval status {0:?}")]
pub struct ApprovalStatusError(pub String);

impl FromStr for ApprovalStatus {
    type Err = ApprovalStatusError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        ApprovalStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
            .ok_or_else(|| ApprovalStatusError(raw.to_owned()))
    }
}

/// What one approver decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// One step towards the quorum.
    Approve,
    /// Resolves the gate immediately.
    Reject,
}

impl DecisionKind {
    /// The stored `approval_decision.decision`.
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionKind::Approve => "approve",
            DecisionKind::Reject => "reject",
        }
    }
}

impl fmt::Display for DecisionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who (or what) decided. Carried from fr 211/219 (SPEC D18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeciderKind {
    /// A human approver.
    Human,
    /// The triage agent.
    TriageAgent,
    /// A deterministic rule.
    Rule,
    /// Another agent.
    Agent,
    /// A per-tenant channel service account (S06d): the Telegram/webhook connector identity.
    /// A machine, not a rule — keeping it distinct is what lets an operator tell a person's
    /// delegated chat decision from an automated one.
    Service,
}

impl DeciderKind {
    /// Every kind.
    pub const ALL: [DeciderKind; 5] = [
        DeciderKind::Human,
        DeciderKind::TriageAgent,
        DeciderKind::Rule,
        DeciderKind::Agent,
        DeciderKind::Service,
    ];

    /// The stored `approval_decision.decided_by_kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            DeciderKind::Human => "human",
            DeciderKind::TriageAgent => "triage_agent",
            DeciderKind::Rule => "rule",
            DeciderKind::Agent => "agent",
            DeciderKind::Service => "service",
        }
    }
}

impl fmt::Display for DeciderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One approver's decision, as it is about to be inserted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// The approver (`app_user.id`). Unique per approval, which is what dual control means.
    pub approver_id: uuid::Uuid,
    /// Approve or reject.
    pub kind: DecisionKind,
    /// A free-text note; empty by default.
    pub note: String,
    /// Human, triage agent, rule or agent.
    pub decided_by_kind: DeciderKind,
    /// How an agent/rule decision was reached (model, prompt hash, rule id), when applicable.
    pub via: Option<Value>,
    /// Per-item verdicts for a multi-item gate (a checklist approval), when the caller sent any.
    pub item_decisions: Option<Value>,
}

impl Decision {
    /// A human decision with no note.
    pub fn approve(approver_id: uuid::Uuid) -> Self {
        Self {
            approver_id,
            kind: DecisionKind::Approve,
            note: String::new(),
            decided_by_kind: DeciderKind::Human,
            via: None,
            item_decisions: None,
        }
    }

    /// A human rejection.
    pub fn reject(approver_id: uuid::Uuid) -> Self {
        Self {
            approver_id,
            kind: DecisionKind::Reject,
            note: String::new(),
            decided_by_kind: DeciderKind::Human,
            via: None,
            item_decisions: None,
        }
    }
}

/// One approval gate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    /// The row id.
    pub id: uuid::Uuid,
    /// The run being gated, when the gate is run-scoped.
    pub run_id: Option<uuid::Uuid>,
    /// The card being gated, when the gate is card-scoped.
    pub card_id: Option<uuid::Uuid>,
    /// The project the gate belongs to.
    pub project_id: Option<uuid::Uuid>,
    /// The gate name (`catalogue_entry(kind='gate').code`).
    pub gate: String,
    /// The gate's kind.
    pub kind: String,
    /// The current status.
    pub status: ApprovalStatus,
    /// The quorum: this many distinct approvals resolve it.
    pub required_decisions: u16,
    /// Approvals recorded so far (a read of `approval_decision`, never a column).
    pub approvals: u16,
    /// When the expiry job may resolve it.
    pub expires_at: Option<Timestamp>,
    /// Where a channel delivery reports the result.
    pub callback_url: Option<String>,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// Why a decision or expiry is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ApprovalError {
    /// Only a pending gate accepts a decision, expiry or void.
    #[error("approval is {status}; only a pending gate can change")]
    NotPending {
        /// The status the gate holds.
        status: ApprovalStatus,
    },
    /// A quorum of zero is not a gate.
    #[error("required_decisions must be at least 1")]
    InvalidRequirement,
    /// `expire` was called before `expires_at` (or the gate has no expiry at all).
    #[error("the approval is not yet due to expire (expires_at={expires_at:?}, at={at})")]
    NotDue {
        /// The gate's expiry, if any.
        expires_at: Option<Timestamp>,
        /// The instant the job ran.
        at: Timestamp,
    },
}

impl Approval {
    /// Record one decision.
    ///
    /// A reject resolves the gate immediately. An approve resolves it only when this
    /// decision reaches the quorum; otherwise the gate stays pending and the caller keeps the
    /// returned value (the next decision re-reads the count). `required` is the row's
    /// `required_decisions`; it is passed explicitly because a decision carries the quorum it
    /// was evaluated under into the audit trail.
    pub fn decide(mut self, d: Decision, required: u16) -> Result<Self, ApprovalError> {
        if self.status != ApprovalStatus::Pending {
            return Err(ApprovalError::NotPending {
                status: self.status,
            });
        }
        if required == 0 {
            return Err(ApprovalError::InvalidRequirement);
        }
        match d.kind {
            DecisionKind::Reject => self.status = ApprovalStatus::Rejected,
            DecisionKind::Approve => {
                // `d` is one of the approvals counted; `self.approvals` is the count before it.
                let approvals = self.approvals.saturating_add(1);
                self.approvals = approvals;
                if approvals >= required {
                    self.status = ApprovalStatus::Approved;
                }
            }
        }
        self.version = self.version.next();
        Ok(self)
    }

    /// Let the expiry job resolve a gate whose `expires_at` has passed.
    ///
    /// The comparison uses the instant the job ran, not a Rust clock at construction; a gate
    /// with no expiry never expires. A non-pending gate is refused, so a late job cannot
    /// overwrite a decision.
    pub fn expire(mut self, at: Timestamp) -> Result<Self, ApprovalError> {
        if self.status != ApprovalStatus::Pending {
            return Err(ApprovalError::NotPending {
                status: self.status,
            });
        }
        match self.expires_at {
            Some(expires_at) if expires_at <= at => {
                self.status = ApprovalStatus::Expired;
                self.version = self.version.next();
                Ok(self)
            }
            _ => Err(ApprovalError::NotDue {
                expires_at: self.expires_at,
                at,
            }),
        }
    }

    /// Withdraw a pending gate (`runs.cancel`). It will never be answered.
    pub fn void(mut self) -> Result<Self, ApprovalError> {
        if self.status != ApprovalStatus::Pending {
            return Err(ApprovalError::NotPending {
                status: self.status,
            });
        }
        self.status = ApprovalStatus::Voided;
        self.version = self.version.next();
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn at(seconds: i64) -> Timestamp {
        chrono::DateTime::from_timestamp(seconds, 0).expect("a valid instant")
    }

    fn approval(required: u16) -> Approval {
        Approval {
            id: uuid::Uuid::now_v7(),
            run_id: Some(uuid::Uuid::now_v7()),
            card_id: None,
            project_id: None,
            gate: "review".to_owned(),
            kind: "gate".to_owned(),
            status: ApprovalStatus::Pending,
            required_decisions: required,
            approvals: 0,
            expires_at: Some(at(100)),
            callback_url: None,
            version: Version::INITIAL,
        }
    }

    #[test]
    fn dual_control_needs_two_distinct_approvals_and_a_reject_wins() {
        let gate = approval(2);
        let after_one = gate
            .clone()
            .decide(Decision::approve(uuid::Uuid::now_v7()), 2)
            .expect("first approval");
        assert_eq!(after_one.status, ApprovalStatus::Pending);
        assert_eq!(after_one.approvals, 1);

        let after_two = after_one
            .decide(Decision::approve(uuid::Uuid::now_v7()), 2)
            .expect("second approval");
        assert_eq!(after_two.status, ApprovalStatus::Approved);
        assert_eq!(after_two.approvals, 2);

        let rejected = gate
            .decide(Decision::reject(uuid::Uuid::now_v7()), 2)
            .expect("a reject resolves immediately");
        assert_eq!(rejected.status, ApprovalStatus::Rejected);
        assert_eq!(
            rejected.decide(Decision::approve(uuid::Uuid::now_v7()), 2),
            Err(ApprovalError::NotPending {
                status: ApprovalStatus::Rejected
            })
        );
    }

    #[test]
    fn expire_needs_a_due_pending_gate_and_void_withdraws() {
        let gate = approval(1);
        assert_eq!(
            gate.clone().expire(at(99)),
            Err(ApprovalError::NotDue {
                expires_at: Some(at(100)),
                at: at(99)
            })
        );
        assert_eq!(
            gate.clone().expire(at(100)).expect("due").status,
            ApprovalStatus::Expired
        );
        assert_eq!(
            gate.clone().void().expect("voids").status,
            ApprovalStatus::Voided
        );

        let no_expiry = Approval {
            expires_at: None,
            ..approval(1)
        };
        assert!(matches!(
            no_expiry.expire(at(1_000_000)),
            Err(ApprovalError::NotDue { .. })
        ));

        let approved = approval(1)
            .decide(Decision::approve(uuid::Uuid::now_v7()), 1)
            .expect("approved");
        assert_eq!(
            approved.clone().void(),
            Err(ApprovalError::NotPending {
                status: ApprovalStatus::Approved
            })
        );
        assert_eq!(
            approved.expire(at(1_000)),
            Err(ApprovalError::NotPending {
                status: ApprovalStatus::Approved
            })
        );
    }

    #[test]
    fn a_zero_quorum_is_refused() {
        assert_eq!(
            approval(0).decide(Decision::approve(uuid::Uuid::now_v7()), 0),
            Err(ApprovalError::InvalidRequirement)
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// For any quorum, a run of approve decisions resolves to `approved` exactly on the
        /// decision that reaches it and never earlier.
        #[test]
        fn a_quorum_resolves_on_the_decision_that_reaches_it(
            required in 1u16..=8,
            decisions in 1usize..=10,
        ) {
            let mut gate = approval(required);
            let mut previous = ApprovalStatus::Pending;
            for index in 0..decisions {
                // A resolved gate accepts no further decision; the loop models a caller that
                // stops when the gate resolves.
                if gate.status != ApprovalStatus::Pending {
                    break;
                }
                let next = gate
                    .clone()
                    .decide(Decision::approve(uuid::Uuid::now_v7()), required)
                    .map_err(|e| TestCaseError::fail(format!("{e}")))?;
                if next.status == ApprovalStatus::Approved {
                    prop_assert!(index as u16 + 1 >= required);
                    prop_assert_eq!(previous, ApprovalStatus::Pending);
                }
                previous = next.status;
                gate = next;
            }
            if decisions as u16 >= required {
                prop_assert_eq!(gate.status, ApprovalStatus::Approved);
            } else {
                prop_assert_eq!(gate.status, ApprovalStatus::Pending);
            }
        }
    }
}
