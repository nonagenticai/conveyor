#![forbid(unsafe_code)]
//! Copilot conversation vocabulary (S10 §5.4).
//!
//! A conversation is per user; a turn is one message. Only the turn's `meta` is a state machine — the
//! assistant turn starts `streaming` and becomes terminal in one transaction that also writes the
//! `conversation_turn.created` event, so a crash is `interrupted` and never a turn claiming an effect that
//! did not commit (S10-D4). Tool calls are never stored here: they are `action_invocation` rows and a turn
//! records only their ids.
//!
//! This module is pure: no database, no registry. The store writes what these values agreed to, and the
//! migration's CHECK constraints refuse the vocabulary at the other end.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tool-call rounds one turn may run before it is refused (S10 §6 step 9).
pub const MAX_ROUNDS: u8 = 8;

/// A `streaming` turn older than this is marked `interrupted` by `copilot.turn.reap` (S10 §5.4).
pub const REAP_AFTER_SECS: i64 = 300;

/// A conversation's lifecycle. Archiving is a display choice; turns are never deleted by it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    /// The default; new turns may be appended.
    #[default]
    Active,
    /// Hidden from the default list; still readable and replayable.
    Archived,
}

impl ConversationStatus {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    /// Parse the column value; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// Who wrote a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    /// The person.
    User,
    /// The copilot's turn: inserted at start `streaming`, updated terminal once.
    Assistant,
    /// A system note (a refusal, a resync marker); never produced by the model.
    System,
}

impl TurnRole {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    /// Parse the column value; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// An assistant turn's terminal outcome (S10 §5.4's `meta.status`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    /// Inserted, not yet terminal. The only non-terminal state.
    #[default]
    Streaming,
    /// The turn finished; any tool calls it ran are in `tool_calls`.
    Completed,
    /// The turn ended in a proposal; executing it is the approved `copilot.action` approval's job.
    Proposed,
    /// The model asked for something the surface does not expose, or the input was invalid.
    Refused,
    /// The model or an invoked action failed; `error` says why.
    Failed,
    /// The process died mid-turn (or `copilot.turn.reap` found it older than [`REAP_AFTER_SECS`]).
    Interrupted,
}

impl TurnStatus {
    /// The wire/column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Completed => "completed",
            Self::Proposed => "proposed",
            Self::Refused => "refused",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    /// Parse the wire value; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "streaming" => Some(Self::Streaming),
            "completed" => Some(Self::Completed),
            "proposed" => Some(Self::Proposed),
            "refused" => Some(Self::Refused),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    /// Whether nothing may change this turn again. A terminal turn is immutable — a late writer is a
    /// bug, not a second outcome (S10 trap 4).
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Streaming)
    }

    /// Only a `streaming` turn may become a terminal one; a terminal turn never moves.
    pub fn may_finish(self, next: Self) -> bool {
        matches!(self, Self::Streaming) && next.is_terminal()
    }
}

/// The model call that produced the turn (S10 §5.4 `meta.model`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRun {
    /// Always `completions.create`: the action, not a free-form provider name.
    pub action: String,
    /// The model the router chose, as the router reported it.
    pub model: String,
    /// The router's request id, for a support trail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// One tool call the turn ran (S10 §5.4 `meta.tool_calls`).
///
/// The invocation row is the record; this is the turn's index into it. `invocation_id` is `None`
/// when the action refused before writing one (a validation error still writes an invocation row,
/// a refused scope does not), and `outcome` then carries `error:<code>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// The registry action name.
    pub action: String,
    /// The `action_invocation` id — written only after the action committed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<Uuid>,
    /// `ok`, `error:<code>`, or `proposed`.
    pub outcome: String,
}

/// The RFC 9457 problem a failed turn ended with (S10 §5.4 `meta.error`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnError {
    /// The problem type, e.g. `/problems/inference-unavailable`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The HTTP status the same failure would carry.
    pub status: u16,
    /// A human-readable detail.
    pub detail: String,
}

/// The whole `conversation_turn.meta` record (S10 §5.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TurnMeta {
    /// The turn's outcome; `streaming` until the terminal update.
    pub status: TurnStatus,
    /// The model call, once one happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRun>,
    /// The tool calls, in the order they ran.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallRecord>,
    /// The failure, when `status` is `failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<TurnError>,
    /// The proposal's approval id, when `status` is `proposed` — the id S12's proposal card decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
}

impl TurnMeta {
    /// A turn that has just been inserted, still running.
    pub fn streaming() -> Self {
        Self {
            status: TurnStatus::Streaming,
            ..Self::default()
        }
    }

    /// Parse a stored `meta` value. A turn whose `meta.status` is missing or unknown reads as
    /// `streaming` — which is what the reaper expects for a row it must be able to finish.
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// The stored form.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a TurnMeta always serializes")
    }
}
