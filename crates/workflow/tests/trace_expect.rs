#![forbid(unsafe_code)]
//! The reviewed expectation (S13 §5.4): derivation, comparison and the committed fixtures.
//!
//! The proof this file exists for: two runs whose **payloads differ completely** but whose effect
//! path is the same have **no** delta, and a changed effect does. That is the whole redesign — a
//! cross-system hash cannot be the pass definition when the two systems store different schemas.

use conveyor_workflow::Definition;
use conveyor_workflow::expect::Expected;
use conveyor_workflow::trace::{DiffMode, TraceKind, TraceOutcome, TraceRecord, diff_records_with};
use serde_json::{Map, Value, json};

fn record(
    node_id: &str,
    position: u32,
    effect: &str,
    input_hash: &str,
    output_hash: &str,
) -> TraceRecord {
    TraceRecord {
        run_id: "run".to_owned(),
        workflow_key: "cap.analysis.consult".to_owned(),
        workflow_version: 1,
        node_id: node_id.to_owned(),
        position,
        attempt: 1,
        kind: TraceKind::Action,
        effect: effect.to_owned(),
        input_hash: input_hash.to_owned(),
        output_hash: output_hash.to_owned(),
        duration_ms: None,
        outcome: TraceOutcome::Succeeded,
    }
}

fn flow(ids: &[&str]) -> Definition {
    let nodes: Vec<Value> = ids
        .iter()
        .map(|id| json!({"id": id, "kind": "action", "action": "projects.get"}))
        .collect();
    serde_json::from_value(json!({"schema": 1, "nodes": nodes, "edges": []}))
        .expect("the test flow parses")
}

fn expected_for(fixture: &str, ids: &[&str]) -> Expected {
    let old = [
        record(
            "Project",
            1,
            "projects.get",
            "sha256:old-in",
            "sha256:old-out",
        ),
        record(
            "Agent consult",
            2,
            "agent.run",
            "sha256:old-in",
            "sha256:old-out",
        ),
    ];
    let mut expected = Expected::derive(&old, &flow(ids), "fixtures/a.flow.json", fixture, 1218441)
        .expect("the recording derives");
    expected
        .end_state
        .insert("terminal_effect".to_owned(), json!("agent.run"));
    expected
}

#[test]
fn a_different_payload_is_not_a_delta_but_a_changed_effect_is() {
    let expected = expected_for("a", &["project", "agent_consult"]);

    // The new side's payload hashes share nothing with the recording's. Under the migration rule
    // they are not compared, so the paths are equal.
    let same_path = [
        record(
            "project",
            1,
            "projects.get",
            "sha256:new-in",
            "sha256:new-out",
        ),
        record(
            "agent_consult",
            2,
            "agent.run",
            "sha256:new-in",
            "sha256:new-out",
        ),
    ];
    let report = expected.compare(&same_path).expect("both complete");
    assert!(
        report.identical(),
        "a payload difference must not be a delta: {:?}",
        report.deltas
    );

    // Reversal of the rule: the same records compared as same-system traces DO differ — that is
    // what makes the assertion above a measurement of the migration rule, not of a diff that cannot
    // see. And a changed effect is a delta in both modes.
    let same_system = diff_records_with(
        &[record(
            "project",
            1,
            "projects.get",
            "sha256:old-in",
            "sha256:old-out",
        )],
        &[record(
            "project",
            1,
            "projects.get",
            "sha256:new-in",
            "sha256:new-out",
        )],
        DiffMode::SameSystem,
    )
    .expect("both complete");
    assert!(
        !same_system.identical(),
        "same-system hashes must still compare"
    );

    let mut changed = same_path;
    changed[1].effect = "inference.chat".to_owned();
    let report = expected.compare(&changed).expect("both complete");
    assert_eq!(report.deltas.len(), 1, "{:?}", report.deltas);
    assert!(matches!(
        &report.deltas[0],
        conveyor_workflow::trace::TraceDelta::Mismatch { field: "effect", old, new, .. }
            if old == "agent.run" && new == "inference.chat"
    ));
}

#[test]
fn an_extra_or_missing_step_is_a_named_delta() {
    let expected = expected_for("a", &["project", "agent_consult"]);

    // A step the expectation has and the run did not produce.
    let shorter = [record("project", 1, "projects.get", "", "")];
    let report = expected.compare(&shorter).expect("both complete");
    assert_eq!(report.deltas.len(), 1, "{:?}", report.deltas);
    assert!(
        matches!(
            &report.deltas[0],
            conveyor_workflow::trace::TraceDelta::Missing { node_id, .. }
                if node_id == "agent_consult"
        ),
        "{:?}",
        report.deltas
    );

    // A step the run produced that the expectation does not have — the "an extra effect is a
    // failure" limb of §5.4.
    let longer = [
        record("project", 1, "projects.get", "", ""),
        record("agent_consult", 2, "agent.run", "", ""),
        record("surprise", 3, "cards.move", "", ""),
    ];
    let report = expected.compare(&longer).expect("both complete");
    assert_eq!(report.deltas.len(), 1, "{:?}", report.deltas);
    assert!(
        matches!(
            &report.deltas[0],
            conveyor_workflow::trace::TraceDelta::Added { node_id, .. } if node_id == "surprise"
        ),
        "{:?}",
        report.deltas
    );
}

#[test]
fn a_recorded_node_with_no_flow_node_is_refused_by_name() {
    let old = [record("Agent consult", 1, "agent.run", "", "")];
    let error = Expected::derive(&old, &flow(&["project"]), "fixtures/a.flow.json", "a", 1)
        .expect_err("an unmapped node must be refused");
    let message = error.to_string();
    assert!(message.contains("Agent consult"), "{message}");
    assert!(message.contains("agent_consult"), "{message}");
    assert!(message.contains("fixtures/a.flow.json"), "{message}");
}

#[test]
fn an_expectation_without_an_end_state_is_refused() {
    let old = [record("Project", 1, "projects.get", "", "")];
    let expected = Expected::derive(&old, &flow(&["project"]), "fixtures/a.flow.json", "a", 1)
        .expect("derives");
    let error = expected
        .validate()
        .expect_err("an empty end_state is invalid");
    assert!(error.to_string().contains("a"), "{error}");
    assert!(error.to_string().contains("end_state"), "{error}");
}

#[test]
fn end_state_matches_names_what_differs() {
    let mut expected = expected_for("a", &["project", "agent_consult"]);
    expected
        .end_state
        .insert("card.list".to_owned(), json!("DOING"));

    let mut observed = Map::new();
    observed.insert("terminal_effect".to_owned(), json!("agent.run"));
    observed.insert("card.list".to_owned(), json!("TODO"));
    observed.insert("extra".to_owned(), json!("ignored"));

    let mismatches = expected.end_state_matches(&observed);
    assert_eq!(mismatches.len(), 1, "{mismatches:?}");
    assert!(mismatches[0].contains("card.list"), "{mismatches:?}");
    assert!(mismatches[0].contains("DOING") && mismatches[0].contains("TODO"));

    observed.insert("card.list".to_owned(), json!("DOING"));
    assert!(expected.end_state_matches(&observed).is_empty());
}
