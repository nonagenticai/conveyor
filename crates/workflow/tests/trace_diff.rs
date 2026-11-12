//! S13 step 9 — the trace diff (`admin trace-diff`'s pure half).
//!
//! Exit 0 means identical; any added, missing, reordered or changed effect is a delta.

use conveyor_workflow::trace::{
    DiffMode, TraceDelta, TraceKind, TraceOutcome, TraceRecord, diff_records, diff_records_with,
    hash_value, normalise_node_id,
};
use serde_json::json;

fn record(position: u32, node_id: &str, effect: &str, output: &serde_json::Value) -> TraceRecord {
    TraceRecord {
        run_id: "b3f7c0de-0000-4000-8000-000000000001".to_owned(),
        workflow_key: "cap.x".to_owned(),
        workflow_version: 1,
        node_id: node_id.to_owned(),
        position,
        attempt: 1,
        kind: TraceKind::Action,
        effect: effect.to_owned(),
        input_hash: hash_value(&json!({"node": node_id})),
        output_hash: hash_value(output),
        duration_ms: Some(1),
        outcome: TraceOutcome::Succeeded,
    }
}

fn trace() -> Vec<TraceRecord> {
    vec![
        record(1, "claim", "cards.claim", &json!({"ok": true})),
        record(2, "move", "cards.move", &json!({"ok": true})),
    ]
}

#[test]
fn identical_traces_have_no_deltas() {
    let report = diff_records(&trace(), &trace()).expect("both complete");
    assert!(report.identical());
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn a_mutated_output_hash_is_a_mismatch() {
    let mut newer = trace();
    newer[1].output_hash = "sha256:tampered".to_owned();
    let report = diff_records(&trace(), &newer).expect("both complete");
    assert!(!report.identical());
    assert_eq!(report.exit_code(), 1);
    assert_eq!(
        report.deltas,
        vec![TraceDelta::Mismatch {
            position: 2,
            node_id: "move".to_owned(),
            field: "output_hash",
            old: trace()[1].output_hash.clone(),
            new: "sha256:tampered".to_owned(),
        }]
    );
}

#[test]
fn an_added_effect_is_a_delta() {
    let mut newer = trace();
    newer.push(record(3, "comment", "cards.comment", &json!({"ok": true})));
    let report = diff_records(&trace(), &newer).expect("both complete");
    assert_eq!(
        report.deltas,
        vec![TraceDelta::Added {
            position: 3,
            node_id: "comment".to_owned(),
        }]
    );
}

#[test]
fn a_missing_effect_is_a_delta() {
    let mut newer = trace();
    newer.pop();
    let report = diff_records(&trace(), &newer).expect("both complete");
    assert_eq!(
        report.deltas,
        vec![TraceDelta::Missing {
            position: 2,
            node_id: "move".to_owned(),
        }]
    );
}

#[test]
fn a_reordering_is_a_delta() {
    // The trace is ordered by `position`, so a reorder is a change of positions, not of file order.
    let mut newer = trace();
    newer[0].position = 2;
    newer[1].position = 1;
    let report = diff_records(&trace(), &newer).expect("both complete");
    assert!(!report.identical());
    assert!(matches!(
        report.deltas.first(),
        Some(TraceDelta::Mismatch {
            field: "node_id",
            ..
        })
    ));
}

#[test]
fn a_changed_outcome_is_a_delta() {
    let mut newer = trace();
    newer[0].outcome = TraceOutcome::Skipped;
    let report = diff_records(&trace(), &newer).expect("both complete");
    assert_eq!(
        report.deltas,
        vec![TraceDelta::Mismatch {
            position: 1,
            node_id: "claim".to_owned(),
            field: "outcome",
            old: "succeeded".to_owned(),
            new: "skipped".to_owned(),
        }]
    );
}

// ── the migration rule (owner ruling 2026-11-09: cross-system hashes are thrown out) ─────────

#[test]
fn a_migration_ignores_a_hash_difference() {
    // The two systems hash different schemas; the ruling is that this is not a delta.
    let mut newer = trace();
    newer[1].output_hash = "sha256:the-other-systems-row".to_owned();
    newer[1].input_hash = "sha256:the-other-systems-request".to_owned();
    let report = diff_records_with(&trace(), &newer, DiffMode::Migration).expect("both complete");
    assert!(report.identical(), "{:?}", report.deltas);
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn a_migration_compares_normalised_node_ids() {
    // An n8n node name carries spaces; a conveyor definition id may not. `Agent consult` and
    // `agent_consult` are the same node.
    let older = vec![record(
        1,
        "Agent consult",
        "agent.run",
        &json!({"ok": true}),
    )];
    let newer = vec![record(
        1,
        "agent_consult",
        "agent.run",
        &json!({"ok": true}),
    )];
    let same = diff_records_with(&older, &newer, DiffMode::Migration).expect("both complete");
    assert!(same.identical(), "{:?}", same.deltas);

    // The same pair is a delta under the same-system rule, which compares ids verbatim.
    let strict = diff_records(&older, &newer).expect("both complete");
    assert_eq!(
        strict.deltas,
        vec![TraceDelta::Mismatch {
            position: 1,
            node_id: "agent_consult".to_owned(),
            field: "node_id",
            old: "Agent consult".to_owned(),
            new: "agent_consult".to_owned(),
        }]
    );
}

#[test]
fn a_migration_still_fails_on_a_changed_effect_or_outcome() {
    let mut newer = trace();
    newer[0].effect = "cards.comment".to_owned();
    newer[1].outcome = TraceOutcome::Failed;
    let report = diff_records_with(&trace(), &newer, DiffMode::Migration).expect("both complete");
    assert_eq!(report.exit_code(), 1);
    let fields: Vec<&str> = report
        .deltas
        .iter()
        .filter_map(|delta| match delta {
            TraceDelta::Mismatch { field, .. } => Some(*field),
            _ => None,
        })
        .collect();
    assert_eq!(fields, ["effect", "outcome"], "{:?}", report.deltas);
}

#[test]
fn a_non_ascii_node_id_normalises_before_comparison() {
    assert_eq!(normalise_node_id("Agent consult"), "agent_consult");
    assert_eq!(
        normalise_node_id("  Project type  card types!! "),
        "project_type_card_types"
    );
    assert_eq!(normalise_node_id("ÉTAPE—deux"), "étape_deux");
}

#[test]
fn inputs_that_differ_only_by_a_masked_id_are_identical() {
    let left = hash_value(&json!({
        "card_id": "b3f7c0de-0000-4000-8000-000000000001",
        "name": "s13 gate card"
    }));
    let right = hash_value(&json!({
        "card_id": "ffffffff-0000-4000-8000-000000000099",
        "name": "s13 gate card"
    }));
    assert_eq!(left, right, "the mask is what makes a replay comparable");
}
