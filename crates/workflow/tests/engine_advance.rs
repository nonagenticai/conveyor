//! The pure executor of S13 §5.3: deterministic serial planning, joins, branches, loops, retries and
//! gates. No database and no clock — the caller reports every outcome.

use conveyor_workflow::{Definition, Engine, EngineError, NodeOutcome, NodeStatus, RunStatus};
use serde_json::{Value, json};

fn definition(raw: Value) -> Definition {
    Definition::decode(&serde_json::to_vec(&raw).expect("serialises")).expect("decodes")
}

fn action(id: &str) -> Value {
    json!({"id": id, "kind": "action", "action": "cards.claim"})
}

/// Advance once and return the emitted node id; panics when the plan is empty.
fn step(engine: &mut Engine, definition: &Definition) -> String {
    let advance = engine.advance(definition);
    assert_eq!(
        advance.steps.len(),
        1,
        "expected one step, status {:?}",
        advance.status
    );
    advance.steps[0].node_id.clone()
}

fn linear() -> Definition {
    definition(json!({
        "schema": 1,
        "nodes": [action("a"), action("b"), action("c")],
        "edges": [{"from": "a", "to": "b"}, {"from": "b", "to": "c"}]
    }))
}

#[test]
fn a_linear_graph_runs_in_order_to_success() {
    let definition = linear();
    let mut engine = Engine::new(&definition).expect("starts");
    for expected in ["a", "b", "c"] {
        assert_eq!(step(&mut engine, &definition), expected);
        engine
            .succeeded(&definition, expected, None)
            .expect("succeeds");
    }
    assert_eq!(engine.advance(&definition).status, RunStatus::Succeeded);
    assert_eq!(engine.status(), RunStatus::Succeeded);
}

#[test]
fn fan_out_runs_in_document_order_and_the_join_waits_for_every_edge() {
    let definition = definition(json!({
        "schema": 1,
        // `join` sits BEFORE `c` in document order: without the join guard it would run as soon as `b`
        // finished, and this test would catch it.
        "nodes": [action("start"), action("b"), action("join"), action("c")],
        "edges": [
            {"from": "start", "to": "b"},
            {"from": "start", "to": "c"},
            {"from": "b", "to": "join"},
            {"from": "c", "to": "join"}
        ]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    let mut order = Vec::new();
    while engine.status() == RunStatus::Running {
        let advance = engine.advance(&definition);
        if advance.steps.is_empty() {
            break;
        }
        let id = advance.steps[0].node_id.clone();
        engine.succeeded(&definition, &id, None).expect("succeeds");
        order.push(id);
    }
    assert_eq!(order, vec!["start", "b", "c", "join"]);
    assert_eq!(engine.status(), RunStatus::Succeeded);
}

#[test]
fn a_condition_takes_exactly_one_branch() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "cond", "kind": "condition",
             "predicate": {"op": "eq", "left": "{{card.status}}", "right": "done"}},
            action("t"),
            action("f")
        ],
        "edges": [
            {"from": "cond", "to": "t", "when": "true"},
            {"from": "cond", "to": "f", "when": "false"}
        ]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut engine, &definition), "cond");
    engine
        .succeeded(&definition, "cond", Some(NodeOutcome::Condition(true)))
        .expect("verdict");
    assert_eq!(step(&mut engine, &definition), "t");
    engine.succeeded(&definition, "t", None).expect("succeeds");
    assert_eq!(engine.advance(&definition).status, RunStatus::Succeeded);
    assert_eq!(
        engine.node_state("f").map(|state| state.status),
        Some(NodeStatus::Pending),
        "the untaken branch never runs"
    );
}

#[test]
fn a_condition_without_its_verdict_is_a_typed_error() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "cond", "kind": "condition", "predicate": {"op": "exists", "left": "{{card.id}}"}},
            action("t"),
            action("f")
        ],
        "edges": [
            {"from": "cond", "to": "t", "when": "true"},
            {"from": "cond", "to": "f", "when": "false"}
        ]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut engine, &definition), "cond");
    let error = engine
        .succeeded(&definition, "cond", None)
        .expect_err("a verdict is required");
    assert!(
        matches!(error, EngineError::MissingOutcome { .. }),
        "{error}"
    );
}

#[test]
fn a_loop_runs_once_per_item_then_takes_done() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "L", "kind": "loop", "items": [], "max_iterations": 5},
            action("B"),
            action("D")
        ],
        "edges": [
            {"from": "L", "to": "B", "when": "each"},
            {"from": "B", "to": "L"},
            {"from": "L", "to": "D", "when": "done"}
        ]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut engine, &definition), "L");
    engine
        .succeeded(&definition, "L", Some(NodeOutcome::LoopItems(3)))
        .expect("items resolve");

    let mut body_runs = 0;
    let mut order = Vec::new();
    while engine.status() == RunStatus::Running {
        let advance = engine.advance(&definition);
        if advance.steps.is_empty() {
            break;
        }
        let id = advance.steps[0].node_id.clone();
        if id == "B" {
            body_runs += 1;
        }
        order.push(id.clone());
        engine.succeeded(&definition, &id, None).expect("succeeds");
    }
    assert_eq!(body_runs, 3, "one body run per item: {order:?}");
    assert_eq!(order, vec!["B", "B", "B", "D"]);
    assert_eq!(engine.status(), RunStatus::Succeeded);
}

#[test]
fn a_loop_over_zero_items_takes_done_immediately() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "L", "kind": "loop", "items": [], "max_iterations": 5},
            action("B"),
            action("D")
        ],
        "edges": [
            {"from": "L", "to": "B", "when": "each"},
            {"from": "B", "to": "L"},
            {"from": "L", "to": "D", "when": "done"}
        ]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut engine, &definition), "L");
    engine
        .succeeded(&definition, "L", Some(NodeOutcome::LoopItems(0)))
        .expect("items resolve");
    assert_eq!(step(&mut engine, &definition), "D");
}

#[test]
fn a_loop_cannot_exceed_its_declared_cap_at_run_time() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "L", "kind": "loop", "items": [], "max_iterations": 2},
            action("B"),
            action("D")
        ],
        "edges": [
            {"from": "L", "to": "B", "when": "each"},
            {"from": "B", "to": "L"},
            {"from": "L", "to": "D", "when": "done"}
        ]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut engine, &definition), "L");
    let error = engine
        .succeeded(&definition, "L", Some(NodeOutcome::LoopItems(3)))
        .expect_err("above the cap");
    assert!(
        matches!(
            error,
            EngineError::LoopOverIteration {
                items: 3,
                max: 2,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn retry_appends_an_attempt_and_fails_typed_when_exhausted() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "a", "kind": "action", "action": "cards.claim",
             "on_error": "retry", "max_attempts": 3}
        ],
        "edges": []
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    for attempt in 1..=3 {
        let advance = engine.advance(&definition);
        assert_eq!(advance.steps.len(), 1);
        assert_eq!(advance.steps[0].attempt, attempt);
        engine
            .failed(&definition, "a", "upstream_500")
            .expect("records the failure");
    }
    assert_eq!(engine.status(), RunStatus::Failed);
    assert_eq!(
        engine.node_state("a").map(|state| state.status),
        Some(NodeStatus::Failed)
    );
}

#[test]
fn continue_skips_the_node_and_keeps_the_graph_moving() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            action("a"),
            {"id": "b", "kind": "action", "action": "cards.comment", "on_error": "continue"},
            action("c")
        ],
        "edges": [{"from": "a", "to": "b"}, {"from": "b", "to": "c"}]
    }));
    let mut engine = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut engine, &definition), "a");
    engine.succeeded(&definition, "a", None).expect("succeeds");
    assert_eq!(step(&mut engine, &definition), "b");
    engine
        .failed(&definition, "b", "not_found")
        .expect("records the failure");
    assert_eq!(
        engine.node_state("b").map(|state| state.status),
        Some(NodeStatus::Skipped)
    );
    assert_eq!(step(&mut engine, &definition), "c");
}

#[test]
fn a_gate_parks_the_run_until_it_is_decided() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            action("start"),
            {"id": "G", "kind": "gate", "gate_kind": "human_approval"},
            action("end")
        ],
        "edges": [{"from": "start", "to": "G"}, {"from": "G", "to": "end"}]
    }));

    let mut approved = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut approved, &definition), "start");
    approved
        .succeeded(&definition, "start", None)
        .expect("succeeds");
    let advance = approved.advance(&definition);
    assert!(advance.steps.is_empty());
    assert_eq!(advance.status, RunStatus::WaitingApproval);
    approved
        .gate_decided(&definition, "G", true)
        .expect("approve");
    assert_eq!(step(&mut approved, &definition), "end");

    let mut refused = Engine::new(&definition).expect("starts");
    assert_eq!(step(&mut refused, &definition), "start");
    refused
        .succeeded(&definition, "start", None)
        .expect("succeeds");
    refused.advance(&definition);
    refused
        .gate_decided(&definition, "G", false)
        .expect("refuse");
    assert_eq!(refused.status(), RunStatus::Failed);
}

#[test]
fn an_outcome_for_a_node_that_is_not_running_is_refused() {
    let definition = linear();
    let mut engine = Engine::new(&definition).expect("starts");
    let error = engine
        .succeeded(&definition, "a", None)
        .expect_err("not emitted yet");
    assert!(matches!(error, EngineError::NotRunning { .. }), "{error}");
    let error = engine
        .succeeded(&definition, "nope", None)
        .expect_err("unknown node");
    assert!(matches!(error, EngineError::UnknownNode { .. }), "{error}");
}
