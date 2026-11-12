#![forbid(unsafe_code)]
//! AG-UI serialization of a conveyor turn (S10 §5.6, S10-D7).
//!
//! Pure: no IO, no registry. The native SSE stream stays the source of truth; this module maps one
//! terminal turn (with its tool calls and proposal) into fr's frozen wire names — eight types,
//! snake_case, no dots (`fr/backend/src/agui/protocol.types.ts`, measured 2026-11-07).
//!
//! ⭐ **Kept because S0 measured a live consumer**: `GET /api/agui/threads` is `used` (P-0121,
//! 6 and 7 hits in the two 24h samples) and the session endpoints are `rare`, so the AG-UI client is
//! real. The adapter is a serializer, not a second stream: it adds no route and no state.
//!
//! ⚠️ **`tool_call.input` and `tool_result.output` are `null` by construction.** The turn record
//! stores the invocation id and the outcome, never the tool's input or output document — a design
//! decision (S10 §5.4: tool calls land in `action_invocation`, not in the turn). A client that needs
//! the payload reads the invocation's own record; the frame is an index, and fabricating a payload
//! here would be a second copy that drifts.

use conveyor_domain::copilot::{ToolCallRecord, TurnError, TurnMeta, TurnStatus};
use serde_json::{Value, json};
use uuid::Uuid;

/// The wire types, in the order one turn can emit them. Asserted against fr's union.
pub const TYPES: [&str; 8] = [
    "tool_call",
    "tool_result",
    "tool_error",
    "approval_pending",
    "approval_resolved",
    "stream_delta",
    "error",
    "done",
];

/// Serialize one terminal turn into its AG-UI frames.
///
/// Exactly one `done` closes the stream; its `reason` is the turn's own status vocabulary, so a
/// client never has to infer the outcome from the absence of a frame.
pub fn frames(turn_id: Uuid, meta: &TurnMeta) -> Vec<Value> {
    let mut out = Vec::new();
    for (index, call) in meta.tool_calls.iter().enumerate() {
        let id = call
            .invocation_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| format!("{turn_id}:{index}"));
        out.push(json!({
            "type": "tool_call",
            "id": id,
            "name": call.action,
            "input": Value::Null,
        }));
        if call.outcome == "ok" {
            out.push(json!({
                "type": "tool_result",
                "id": id,
                "output": Value::Null,
            }));
        } else {
            out.push(json!({
                "type": "tool_error",
                "id": id,
                "error": {
                    "code": call.outcome.trim_start_matches("error:"),
                    "message": format!("{} failed: {}", call.action, call.outcome),
                },
            }));
        }
    }
    if let Some(approval_id) = meta.approval_id {
        out.push(approval_pending(approval_id, meta));
    }
    if let Some(error) = &meta.error {
        out.push(error_frame(error));
    }
    out.push(json!({
        "type": "done",
        "reason": done_reason(meta.status),
    }));
    out
}

/// `approval_pending`: the proposal card's frame. `actionId` and `approvalId` are both the gate a
/// decision closes — the conveyor's proposal id *is* its approval id (S10-D2).
pub fn approval_pending(approval_id: Uuid, meta: &TurnMeta) -> Value {
    let summary = meta
        .tool_calls
        .last()
        .map(|call| call.action.clone())
        .unwrap_or_default();
    json!({
        "type": "approval_pending",
        "actionId": approval_id,
        "approvalId": approval_id,
        "summary": summary,
    })
}

/// `approval_resolved`: emitted when a decision lands, by the channel that serializes approval events.
pub fn approval_resolved(approval_id: Uuid, approved: bool) -> Value {
    json!({
        "type": "approval_resolved",
        "actionId": approval_id,
        "status": if approved { "approved" } else { "rejected" },
    })
}

/// `stream_delta`: one chunk of assistant text. The native stream carries whole turns, so a caller
/// that emits deltas builds them here — the framing is shared, the cadence is not.
pub fn stream_delta(id: impl Into<String>, delta: impl Into<String>) -> Value {
    json!({ "type": "stream_delta", "id": id.into(), "delta": delta.into() })
}

/// `error`: the problem a failed turn ended with.
pub fn error_frame(error: &TurnError) -> Value {
    json!({
        "type": "error",
        "code": error.kind.trim_start_matches("/problems/"),
        "message": error.detail,
    })
}

fn done_reason(status: TurnStatus) -> &'static str {
    match status {
        TurnStatus::Completed | TurnStatus::Proposed => "complete",
        TurnStatus::Failed | TurnStatus::Refused => "error",
        TurnStatus::Interrupted | TurnStatus::Streaming => "interrupted",
    }
}

/// One tool call's frame pair, exposed for callers that frame as they run rather than at the end.
pub fn tool_frames(call: &ToolCallRecord, turn_id: Uuid, index: usize) -> (Value, Value) {
    let id = call
        .invocation_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("{turn_id}:{index}"));
    let call_frame = json!({
        "type": "tool_call",
        "id": id,
        "name": call.action,
        "input": Value::Null,
    });
    let result_frame = if call.outcome == "ok" {
        json!({ "type": "tool_result", "id": id, "output": Value::Null })
    } else {
        json!({
            "type": "tool_error",
            "id": id,
            "error": {
                "code": call.outcome.trim_start_matches("error:"),
                "message": format!("{} failed: {}", call.action, call.outcome),
            },
        })
    };
    (call_frame, result_frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_domain::copilot::ModelRun;

    fn meta(tool_calls: Vec<ToolCallRecord>, status: TurnStatus) -> TurnMeta {
        TurnMeta {
            status,
            model: Some(ModelRun {
                action: "completions.create".to_owned(),
                model: "test".to_owned(),
                request_id: None,
            }),
            tool_calls,
            error: None,
            approval_id: None,
        }
    }

    #[test]
    fn the_vocabulary_is_frs_eight_and_nothing_else() {
        assert_eq!(TYPES.len(), 8);
        let frames = frames(
            Uuid::nil(),
            &meta(
                vec![ToolCallRecord {
                    action: "cards.move".to_owned(),
                    invocation_id: Some(Uuid::nil()),
                    outcome: "ok".to_owned(),
                }],
                TurnStatus::Completed,
            ),
        );
        for frame in &frames {
            let kind = frame["type"].as_str().expect("a type");
            assert!(TYPES.contains(&kind), "unknown frame type {kind}");
        }
        // Exactly one done, always last.
        assert_eq!(
            frames
                .iter()
                .filter(|frame| frame["type"] == "done")
                .count(),
            1
        );
        assert_eq!(frames.last().expect("a frame")["type"], "done");
    }

    #[test]
    fn a_proposed_turn_carries_approval_pending_and_completes() {
        let mut meta = meta(Vec::new(), TurnStatus::Proposed);
        let approval = Uuid::nil();
        meta.approval_id = Some(approval);
        let frames = frames(Uuid::nil(), &meta);
        let pending = frames
            .iter()
            .find(|frame| frame["type"] == "approval_pending")
            .expect("a pending frame");
        assert_eq!(pending["approvalId"], approval.to_string());
        assert_eq!(frames.last().expect("a frame")["reason"], "complete");
    }

    #[test]
    fn a_failed_turn_carries_error_and_ends_with_error() {
        let mut meta = meta(Vec::new(), TurnStatus::Failed);
        meta.error = Some(TurnError {
            kind: "/problems/inference-unavailable".to_owned(),
            status: 503,
            detail: "the router is not configured".to_owned(),
        });
        let frames = frames(Uuid::nil(), &meta);
        let error = frames
            .iter()
            .find(|frame| frame["type"] == "error")
            .expect("an error frame");
        assert_eq!(error["code"], "inference-unavailable");
        assert_eq!(frames.last().expect("a frame")["reason"], "error");
    }
}
