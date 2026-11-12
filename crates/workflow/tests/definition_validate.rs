//! The publish rules of S13 §5.2. Each test names the rule it proves; the reversal is in the slice's
//! evidence file (delete the rule, watch the test fail by name).

use conveyor_domain::process::ActionFacts;
use conveyor_workflow::{
    Definition, MAX_LOOP_NESTING, MAX_NODES, PublishError, PublishFacts, validate_publish,
};
use serde_json::{Value, json};

fn facts() -> (Vec<ActionFacts>, Vec<String>, Vec<String>) {
    let actions = vec![
        ActionFacts {
            name: "cards.claim".to_owned(),
            idempotent: true,
        },
        ActionFacts {
            name: "cards.move".to_owned(),
            idempotent: false,
        },
        ActionFacts {
            name: "cards.comment".to_owned(),
            idempotent: false,
        },
        ActionFacts {
            name: "agent.run".to_owned(),
            idempotent: false,
        },
        ActionFacts {
            name: "inference.chat".to_owned(),
            idempotent: true,
        },
    ];
    let procedures = vec!["cap.test".to_owned()];
    let gates = vec!["human_approval".to_owned()];
    (actions, procedures, gates)
}

fn publish(raw: Value) -> Result<(), PublishError> {
    let bytes = serde_json::to_vec(&raw).expect("serialises");
    let definition = Definition::decode(&bytes)?;
    let (actions, procedures, gates) = facts();
    let facts = PublishFacts {
        actions: &actions,
        procedures: &procedures,
        gates: &gates,
    };
    validate_publish(&definition, &facts)
}

fn linear() -> Value {
    json!({
        "schema": 1,
        "nodes": [
            {"id": "start", "kind": "action", "action": "cards.claim",
             "input_template": {"id": "{{card.id}}"}},
            {"id": "check", "kind": "condition",
             "predicate": {"op": "eq", "left": "{{card.status}}", "right": "done"}},
            {"id": "move", "kind": "action", "action": "cards.move"},
            {"id": "stop", "kind": "action", "action": "cards.comment"}
        ],
        "edges": [
            {"from": "start", "to": "check"},
            {"from": "check", "to": "move", "when": "true"},
            {"from": "check", "to": "stop", "when": "false"}
        ]
    })
}

/// `n` loops nested inside each other, one body and one done target.
fn nested(n: usize) -> Value {
    let mut nodes = Vec::new();
    for level in 1..=n {
        nodes.push(json!({"id": format!("L{level}"), "kind": "loop",
                          "items": [], "max_iterations": 5}));
    }
    nodes.push(json!({"id": "B", "kind": "action", "action": "cards.claim"}));
    nodes.push(json!({"id": "D", "kind": "action", "action": "cards.comment"}));
    let mut edges = Vec::new();
    for level in 1..=n {
        let each = if level == n {
            "B".to_owned()
        } else {
            format!("L{}", level + 1)
        };
        edges.push(json!({"from": format!("L{level}"), "to": each, "when": "each"}));
        let done = if level == 1 {
            "D".to_owned()
        } else {
            format!("L{}", level - 1)
        };
        edges.push(json!({"from": format!("L{level}"), "to": done, "when": "done"}));
    }
    edges.push(json!({"from": "B", "to": format!("L{n}")}));
    json!({"schema": 1, "nodes": nodes, "edges": edges})
}

#[test]
fn a_clean_workflow_publishes() {
    publish(linear()).expect("a legal graph");
}

#[test]
fn a_cycle_without_a_loop_is_refused() {
    let raw = json!({
        "schema": 1,
        "nodes": [
            {"id": "a", "kind": "action", "action": "cards.claim"},
            {"id": "b", "kind": "action", "action": "cards.comment"}
        ],
        "edges": [
            {"from": "a", "to": "b"},
            {"from": "b", "to": "a"}
        ]
    });
    let error = publish(raw).expect_err("an unbounded cycle");
    match error {
        PublishError::CycleWithoutLoop { nodes } => assert_eq!(nodes, vec!["a", "b"]),
        other => panic!("{other}"),
    }
}

#[test]
fn a_loop_needs_both_of_its_branches() {
    let raw = json!({
        "schema": 1,
        "nodes": [
            {"id": "L", "kind": "loop", "items": [], "max_iterations": 2},
            {"id": "B", "kind": "action", "action": "cards.claim"},
            {"id": "D", "kind": "action", "action": "cards.comment"}
        ],
        "edges": [
            {"from": "L", "to": "B", "when": "each"},
            {"from": "B", "to": "L"},
            {"from": "L", "to": "D", "when": "done"}
        ]
    });
    // The graph is well-formed; the loop only lacks its second branch when `done` is removed below.
    publish(raw.clone()).expect("both branches present");
    let mut missing = raw;
    missing["edges"] = json!([
        {"from": "L", "to": "B", "when": "each"},
        {"from": "B", "to": "L"}
    ]);
    let error = publish(missing).expect_err("done is missing");
    assert!(
        matches!(
            error,
            PublishError::MissingBranch {
                missing: "done",
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn the_iteration_cap_is_enforced() {
    for bad in [0, 1_001] {
        let raw = json!({
            "schema": 1,
            "nodes": [
                {"id": "L", "kind": "loop", "items": [], "max_iterations": bad},
                {"id": "B", "kind": "action", "action": "cards.claim"},
                {"id": "D", "kind": "action", "action": "cards.comment"}
            ],
            "edges": [
                {"from": "L", "to": "B", "when": "each"},
                {"from": "B", "to": "L"},
                {"from": "L", "to": "D", "when": "done"}
            ]
        });
        let error = publish(raw).expect_err("out of range");
        assert!(
            matches!(error, PublishError::BadIterations { .. }),
            "{bad}: {error}"
        );
    }
}

#[test]
fn nested_loops_publish_up_to_the_cap_and_no_deeper() {
    publish(nested(MAX_LOOP_NESTING)).expect("three levels are legal");
    let error = publish(nested(MAX_LOOP_NESTING + 1)).expect_err("four levels are not");
    assert!(
        matches!(
            error,
            PublishError::LoopNestingTooDeep {
                depth: 4,
                cap: MAX_LOOP_NESTING,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_loop_done_that_reenters_its_body_is_refused() {
    let raw = json!({
        "schema": 1,
        "nodes": [
            {"id": "L", "kind": "loop", "items": [], "max_iterations": 2},
            {"id": "B", "kind": "action", "action": "cards.claim"}
        ],
        "edges": [
            {"from": "L", "to": "B", "when": "each"},
            {"from": "B", "to": "L"},
            {"from": "L", "to": "B", "when": "done"}
        ]
    });
    let error = publish(raw).expect_err("done re-enters the body");
    assert!(
        matches!(error, PublishError::LoopDoneInsideCycle { .. }),
        "{error}"
    );
}

#[test]
fn retry_needs_an_idempotent_action() {
    let raw = json!({
        "schema": 1,
        "nodes": [
            {"id": "start", "kind": "action", "action": "cards.move",
             "on_error": "retry", "max_attempts": 3}
        ],
        "edges": []
    });
    let error = publish(raw).expect_err("move is not idempotent");
    assert!(
        matches!(error, PublishError::RetryOnNonIdempotent { .. }),
        "{error}"
    );
}

#[test]
fn an_unknown_action_is_refused_by_name() {
    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "start", "kind": "action", "action": "cards.nope"}],
        "edges": []
    });
    let error = publish(raw).expect_err("not registered");
    assert!(
        matches!(error, PublishError::UnknownAction { ref action, .. } if action.as_str() == "cards.nope"),
        "{error}"
    );
}

#[test]
fn a_workflow_may_not_start_a_run() {
    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "start", "kind": "action", "action": "workflow_runs.start"}],
        "edges": []
    });
    let error = publish(raw).expect_err("recursion");
    assert!(
        matches!(error, PublishError::SelfReference { .. }),
        "{error}"
    );
}

#[test]
fn a_step_cannot_reach_a_model_without_the_agent_loop() {
    // S13 §8 trap "a second loop beside `agent.run`": the model action is registered, but a node
    // may not name it — the only route is the capped loop.
    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "ask", "kind": "action", "action": "inference.chat"}],
        "edges": []
    });
    let error = publish(raw).expect_err("direct model call");
    assert!(
        matches!(error, PublishError::DirectModelCall { ref node, .. } if node == "ask"),
        "{error}"
    );

    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "ask", "kind": "action", "action": "agent.run"}],
        "edges": []
    });
    publish(raw).expect("the capped loop is the way through");
}

#[test]
fn a_procedure_must_be_published_and_a_gate_must_exist() {
    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "start", "kind": "procedure", "procedure_key": "cap.nope"}],
        "edges": []
    });
    let error = publish(raw).expect_err("not published");
    assert!(
        matches!(error, PublishError::UnknownProcedure { ref key, .. } if key.as_str() == "cap.nope"),
        "{error}"
    );

    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "start", "kind": "gate", "gate_kind": "nope"}],
        "edges": []
    });
    let error = publish(raw).expect_err("not a gate code");
    assert!(matches!(error, PublishError::UnknownGate { .. }), "{error}");
}

#[test]
fn a_node_kind_must_carry_its_own_field() {
    let raw = json!({
        "schema": 1,
        "nodes": [{"id": "start", "kind": "action"}],
        "edges": []
    });
    let error = publish(raw).expect_err("no action");
    assert!(
        matches!(
            error,
            PublishError::MissingField {
                field: "action",
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn the_node_cap_is_enforced() {
    let mut nodes = Vec::new();
    for index in 0..=MAX_NODES {
        nodes.push(json!({"id": format!("n{index}"), "kind": "action", "action": "cards.claim"}));
    }
    let mut edges = Vec::new();
    for index in 1..=MAX_NODES {
        edges.push(json!({"from": format!("n{}", index - 1), "to": format!("n{index}")}));
    }
    let raw = json!({"schema": 1, "nodes": nodes, "edges": edges});
    let error = publish(raw).expect_err("too many nodes");
    assert!(
        matches!(error, PublishError::TooManyNodes { cap, .. } if cap == MAX_NODES),
        "{error}"
    );
}

#[test]
fn a_document_that_does_not_parse_or_does_not_fit_is_refused_before_anything_else() {
    let error = Definition::decode(b"{not json").expect_err("malformed");
    assert!(matches!(error, PublishError::Malformed { .. }), "{error}");

    let oversized = vec![b' '; 262_145];
    let error = Definition::decode(&oversized).expect_err("oversized");
    assert!(
        matches!(error, PublishError::TooLarge { cap: 262_144, .. }),
        "{error}"
    );

    let error =
        Definition::decode(br#"{"schema":2,"nodes":[],"edges":[]}"#).expect_err("unknown schema");
    assert!(
        matches!(error, PublishError::UnsupportedSchema { schema: 2, .. }),
        "{error}"
    );
}

#[test]
fn an_empty_graph_or_a_second_start_is_refused() {
    let error = publish(json!({"schema": 1, "nodes": [], "edges": []})).expect_err("empty");
    assert!(matches!(error, PublishError::NoNodes), "{error}");

    let raw = json!({
        "schema": 1,
        "nodes": [
            {"id": "a", "kind": "action", "action": "cards.claim"},
            {"id": "b", "kind": "action", "action": "cards.comment"}
        ],
        "edges": []
    });
    let error = publish(raw).expect_err("two starts");
    assert!(
        matches!(error, PublishError::MultipleStarts { count: 2 }),
        "{error}"
    );
}
