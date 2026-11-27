//! The command path: a write is an intent with a state machine, never a local mutation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::entity::CallId;

/// The wire call the shell posts. `approval_id` travels beside `input`, never inside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActionCall {
    pub id: CallId,
    pub action: String,
    pub input: Value,
    pub idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_match: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
}

/// RFC 9457's problem, in the shape the sheet shows verbatim.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

impl Problem {
    pub fn new(
        status: u16,
        kind: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            title: title.into(),
            detail: detail.into(),
            kind: kind.into(),
        }
    }
}

/// The only outcomes a call can produce.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Receipt(Value),
    Refused(Problem),
    Failed(String),
    TimedOut,
}

/// The documented command state machine; the ECS stores the presentation pieces
/// (`PendingCall`, `LastProblem`), never an optimistic domain value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallState {
    Draft,
    Submitted,
    Pending,
    Confirmed,
    Refused,
    Failed,
    TimedOut,
}

impl CallState {
    /// The only legal terminal state for this outcome.
    pub fn terminal(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::Receipt(_) => Self::Confirmed,
            Outcome::Refused(_) => Self::Refused,
            Outcome::Failed(_) => Self::Failed,
            Outcome::TimedOut => Self::TimedOut,
        }
    }
}
