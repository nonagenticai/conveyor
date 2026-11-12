//! S13 step 9 — the trace record and its masking rules (§5.4).
//!
//! These are pure tests: no database, no registry, no clock.

use conveyor_workflow::trace::{
    MASKED_ID, MASKED_TIMESTAMP, MASKED_UUID, TraceDiffError, TraceKind, TraceOutcome, TraceRecord,
    effects, hash_value, mask_value, parse_jsonl, project_effect, to_jsonl,
};
use serde_json::json;

fn record(position: u32, node_id: &str, attempt: u32, outcome: TraceOutcome) -> TraceRecord {
    TraceRecord {
        run_id: "b3f7c0de-0000-4000-8000-000000000001".to_owned(),
        workflow_key: "cap.x".to_owned(),
        workflow_version: 1,
        node_id: node_id.to_owned(),
        position,
        attempt,
        kind: TraceKind::Action,
        effect: "cards.move".to_owned(),
        input_hash: format!("sha256:input-{position}-{attempt}"),
        output_hash: format!("sha256:output-{position}-{attempt}"),
        duration_ms: Some(4),
        outcome,
    }
}

#[test]
fn a_record_round_trips_through_jsonl() {
    let records = vec![
        record(1, "a", 1, TraceOutcome::Succeeded),
        record(2, "b", 1, TraceOutcome::Skipped),
    ];
    let jsonl = to_jsonl(&records).expect("serialise");
    assert_eq!(jsonl.lines().count(), 2);
    let parsed = parse_jsonl(&jsonl).expect("parse");
    assert_eq!(parsed, records);
}

#[test]
fn mask_value_masks_uuids_timestamps_and_keyed_ids() {
    let value = json!({
        "card_id": "b3f7c0de-0000-4000-8000-000000000001",
        "at": "2026-11-09T16:24:00Z",
        "request_id": "whatever-shape",
        "usage": {"turns": 2},
        "name": "s13 gate card",
        "nested": [{"id": "9c0a1f2e-1111-4222-8333-444455556666"}],
    });
    let masked = mask_value(&value);
    assert_eq!(masked["card_id"], json!(MASKED_UUID));
    assert_eq!(masked["at"], json!(MASKED_TIMESTAMP));
    assert_eq!(masked["request_id"], json!(MASKED_ID));
    assert_eq!(masked["usage"], json!(MASKED_ID));
    assert_eq!(masked["name"], json!("s13 gate card"));
    assert_eq!(masked["nested"][0]["id"], json!(MASKED_UUID));
}

#[test]
fn hashes_ignore_masked_values_but_not_real_ones() {
    let left = json!({"card_id": "b3f7c0de-0000-4000-8000-000000000001", "name": "a"});
    let right = json!({"card_id": "ffffffff-0000-4000-8000-000000000099", "name": "a"});
    assert_eq!(hash_value(&left), hash_value(&right));

    let changed = json!({"card_id": "b3f7c0de-0000-4000-8000-000000000001", "name": "b"});
    assert_ne!(hash_value(&left), hash_value(&changed));

    // Key order is canonicalised, so two objects built in different order agree.
    let reordered = json!({"name": "a", "card_id": "b3f7c0de-0000-4000-8000-000000000001"});
    assert_eq!(hash_value(&left), hash_value(&reordered));
}

#[test]
fn effects_use_the_closed_vocabulary() {
    assert_eq!(
        project_effect(TraceKind::Action, "cards.move", &json!({})),
        "cards.move"
    );
    assert_eq!(
        project_effect(TraceKind::Procedure, "procedure:open", &json!({})),
        "procedure:open"
    );
    assert_eq!(
        project_effect(TraceKind::Condition, "condition", &json!(true)),
        "true"
    );
    assert_eq!(
        project_effect(TraceKind::Condition, "condition", &json!(false)),
        "false"
    );
    assert_eq!(
        project_effect(TraceKind::Gate, "gate", &json!(true)),
        "true"
    );
    assert_eq!(
        project_effect(TraceKind::Loop, "loop", &json!({"items": 0})),
        "done"
    );
    assert_eq!(
        project_effect(TraceKind::Loop, "loop", &json!({"items": 3})),
        "each"
    );
}

#[test]
fn effects_order_by_position_and_keep_the_last_attempt() {
    let records = vec![
        record(2, "b", 1, TraceOutcome::Succeeded),
        record(1, "a", 2, TraceOutcome::Succeeded),
        record(1, "a", 1, TraceOutcome::Failed),
    ];
    let effects = effects(&records).expect("complete trace");
    assert_eq!(effects.len(), 2, "a retry is one effect");
    assert_eq!(effects[0].position, 1);
    assert_eq!(effects[0].node_id, "a");
    assert_eq!(effects[0].outcome, TraceOutcome::Succeeded);
    assert_eq!(
        effects[0].input_hash, "sha256:input-1-2",
        "last attempt wins"
    );
    assert_eq!(effects[1].position, 2);
}

#[test]
fn a_running_position_makes_the_trace_incomplete() {
    let records = vec![
        record(1, "a", 1, TraceOutcome::Succeeded),
        record(2, "b", 1, TraceOutcome::Running),
    ];
    assert_eq!(
        effects(&records),
        Err(TraceDiffError::Incomplete { position: 2 })
    );
}

#[test]
fn parse_refuses_the_n8n_envelope() {
    let envelope = r#"{"data":{"resultData":{"runData":{}}}}"#;
    let error = parse_jsonl(envelope).expect_err("the n8n envelope is not a trace");
    match error {
        TraceDiffError::Parse { line, .. } => assert_eq!(line, 1),
        other => panic!("unexpected error: {other:?}"),
    }

    let bad = "not json";
    assert!(parse_jsonl(bad).is_err());
}
