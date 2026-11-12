//! S10 §5.6/§7: the AG-UI serializer's golden frames.
//!
//! The fixtures are checked in as inline golden JSON — the slice's "golden frames" — so a change to
//! a frame shape is a red test rather than a silently different wire. The eight type names are fr's
//! frozen union (`fr/backend/src/agui/protocol.types.ts`, measured 2026-11-07).

use conveyor_api::copilot::agui;
use conveyor_domain::copilot::{ModelRun, ToolCallRecord, TurnMeta, TurnStatus};
use serde_json::{Value, json};
use uuid::Uuid;

fn uuid(raw: &str) -> Uuid {
    Uuid::parse_str(raw).expect("a fixture uuid")
}

#[test]
fn one_completed_turn_serializes_to_the_golden_frames() {
    let meta = TurnMeta {
        status: TurnStatus::Completed,
        model: Some(ModelRun {
            action: "completions.create".to_owned(),
            model: "fixture-model".to_owned(),
            request_id: Some("req-1".to_owned()),
        }),
        tool_calls: vec![
            ToolCallRecord {
                action: "cards.move".to_owned(),
                invocation_id: Some(uuid("0197a0c0-7000-7000-8000-000000000001")),
                outcome: "ok".to_owned(),
            },
            ToolCallRecord {
                action: "cards.get".to_owned(),
                invocation_id: None,
                outcome: "error:action-forbidden".to_owned(),
            },
        ],
        error: None,
        approval_id: None,
    };

    let frames = agui::frames(uuid("0197a0c0-7000-7000-8000-0000000000ff"), &meta);
    let golden = json!([
        {
            "type": "tool_call",
            "id": "0197a0c0-7000-7000-8000-000000000001",
            "name": "cards.move",
            "input": null
        },
        {
            "type": "tool_result",
            "id": "0197a0c0-7000-7000-8000-000000000001",
            "output": null
        },
        {
            "type": "tool_call",
            "id": "0197a0c0-7000-7000-8000-0000000000ff:1",
            "name": "cards.get",
            "input": null
        },
        {
            "type": "tool_error",
            "id": "0197a0c0-7000-7000-8000-0000000000ff:1",
            "error": {
                "code": "action-forbidden",
                "message": "cards.get failed: error:action-forbidden"
            }
        },
        { "type": "done", "reason": "complete" }
    ]);
    assert_eq!(Value::Array(frames), golden);
}

#[test]
fn every_frame_type_is_one_of_frs_eight() {
    let proposed = TurnMeta {
        status: TurnStatus::Proposed,
        model: Some(ModelRun {
            action: "completions.create".to_owned(),
            model: "fixture-model".to_owned(),
            request_id: None,
        }),
        tool_calls: vec![ToolCallRecord {
            action: "cards.move".to_owned(),
            invocation_id: None,
            outcome: "proposed".to_owned(),
        }],
        error: None,
        approval_id: Some(uuid("0197a0c0-7000-7000-8000-000000000002")),
    };
    let failed = TurnMeta {
        status: TurnStatus::Failed,
        model: None,
        tool_calls: Vec::new(),
        error: Some(conveyor_domain::copilot::TurnError {
            kind: "/problems/action-service-unconfigured".to_owned(),
            status: 503,
            detail: "no base URL is configured for service \"inference\"".to_owned(),
        }),
        approval_id: None,
    };
    let mut frames = agui::frames(Uuid::nil(), &proposed);
    let proposed_last = frames.last().cloned().expect("a frame");
    assert_eq!(proposed_last["reason"], "complete");
    frames.extend(agui::frames(Uuid::nil(), &failed));

    for frame in &frames {
        let kind = frame["type"].as_str().expect("a type");
        assert!(
            agui::TYPES.contains(&kind),
            "{kind} is not one of fr's eight wire types"
        );
    }
    // One proposal frame, one error frame, and the failed turn closes `error`.
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["type"] == "approval_pending")
            .count(),
        1
    );
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["type"] == "error")
            .count(),
        1
    );
    assert_eq!(frames.last().expect("a frame")["reason"], "error");
}
