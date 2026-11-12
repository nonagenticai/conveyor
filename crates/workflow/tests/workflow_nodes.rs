//! S13 step 7's gate: each node kind's planning, the closed predicate set, retry/timeout
//! classification and the idempotency key — all pure, no database and no clock.

use std::collections::VecDeque;

use conveyor_domain::template::TemplateCtx;
use conveyor_workflow::definition::{MAX_TIMEOUT_MS, Node};
use conveyor_workflow::engine::{RecordedStatus, RecordedStep};
use conveyor_workflow::nodes::{self, NodeError};
use conveyor_workflow::scheduler::{
    DriveError, GateState, HostResult, HostStatus, NodeRequest, Plan, Positions,
};
use conveyor_workflow::{Definition, Engine};
use serde_json::{Value, json};
use uuid::Uuid;

fn definition(raw: Value) -> Definition {
    Definition::decode(&serde_json::to_vec(&raw).expect("serialises")).expect("decodes")
}

fn node<'a>(definition: &'a Definition, id: &str) -> &'a Node {
    definition.node(id).expect("the node exists")
}

fn ctx() -> TemplateCtx {
    TemplateCtx {
        run: json!({"id": "run-1", "inputs": {"who": "george", "items": [1, 2, 3]}}),
        steps: Vec::new(),
        card: json!({"id": "card-1"}),
        // A workflow has no board (CONTRACT §21); the call step is the only board-aware seam.
        board: json!({}),
        process: json!({"key": "wf", "version": 1}),
        position: 1,
    }
}

struct CaptureHost {
    calls: Vec<(String, Value, String, i64)>,
    results: VecDeque<HostResult>,
}

impl CaptureHost {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            results: VecDeque::new(),
        }
    }
}

impl CaptureHost {
    fn execute(&mut self, request: &NodeRequest) -> HostResult {
        self.calls.push((
            request.node_id.clone(),
            request.input.clone(),
            request.idempotency_key.clone(),
            request.timeout_ms,
        ));
        self.results
            .pop_front()
            .unwrap_or_else(|| HostResult::succeeded(json!({})))
    }

    fn gate(&mut self, _request: &NodeRequest) -> GateState {
        GateState::Pending
    }
}

/// What one plan+apply round did.
#[derive(Debug)]
enum Drive {
    Executed(RecordedStep),
    Waiting,
    Finished,
}

fn drive_once(definition: &Definition, host: &mut CaptureHost) -> Result<Drive, DriveError> {
    let rows: Vec<RecordedStep> = Vec::new();
    let mut engine = Engine::resume(definition, &rows)?;
    let positions = Positions::new(&rows);
    let plan = conveyor_workflow::scheduler::plan(
        definition,
        &mut engine,
        Uuid::nil(),
        &ctx(),
        &positions,
    )?;
    Ok(match plan {
        Plan::Recorded(row) => Drive::Executed(row),
        Plan::Execute(request) | Plan::Poll(request) => {
            let result = host.execute(&request);
            if result.status == HostStatus::Deferred {
                return Err(DriveError::State {
                    detail: "deferred".to_owned(),
                });
            }
            Drive::Executed(conveyor_workflow::scheduler::apply(
                definition,
                &mut engine,
                &request,
                result,
            )?)
        }
        Plan::Gate(request) => match conveyor_workflow::scheduler::apply_gate(
            definition,
            &mut engine,
            &request,
            host.gate(&request),
        )? {
            Some(row) => Drive::Executed(row),
            None => Drive::Waiting,
        },
        Plan::Waiting { .. } => Drive::Waiting,
        Plan::Finished(_) => Drive::Finished,
    })
}

#[test]
fn the_idempotency_key_is_per_attempt_and_stable_across_a_replay() {
    let run = Uuid::from_u128(7);
    let other = Uuid::from_u128(8);
    assert_eq!(
        nodes::idempotency_key(run, "a", 1),
        nodes::idempotency_key(run, "a", 1),
        "the same attempt replays into the same key"
    );
    assert_ne!(
        nodes::idempotency_key(run, "a", 1),
        nodes::idempotency_key(run, "a", 2),
        "a retry is a new invocation"
    );
    assert_ne!(
        nodes::idempotency_key(run, "a", 1),
        nodes::idempotency_key(run, "b", 1)
    );
    assert_ne!(
        nodes::idempotency_key(run, "a", 1),
        nodes::idempotency_key(other, "a", 1)
    );
}

#[test]
fn a_timeout_above_the_cap_is_refused_at_run_time_not_only_at_publish() {
    let over_cap = definition(json!({
        "schema": 1,
        "nodes": [{"id": "a", "kind": "action", "action": "cards.claim",
                   "timeout_ms": MAX_TIMEOUT_MS + 1}],
        "edges": []
    }));
    let error = nodes::timeout_ms(node(&over_cap, "a")).expect_err("over the cap");
    assert!(matches!(error, NodeError::Timeout { .. }), "{error:?}");

    // The scheduler refuses it too, so a stored document that bypassed publish cannot run.
    let mut host = CaptureHost::new();
    let error = drive_once(&over_cap, &mut host).expect_err("must refuse");
    assert!(
        matches!(error, DriveError::Node(NodeError::Timeout { .. })),
        "{error:?}"
    );
    assert!(host.calls.is_empty(), "nothing is invoked");

    let defaulted = definition(json!({
        "schema": 1,
        "nodes": [{"id": "a", "kind": "action", "action": "cards.claim"}],
        "edges": []
    }));
    assert_eq!(
        nodes::timeout_ms(node(&defaulted, "a")).expect("default"),
        conveyor_workflow::definition::DEFAULT_TIMEOUT_MS
    );
}

#[test]
fn an_action_node_runs_with_its_rendered_input_and_its_own_key() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [{"id": "a", "kind": "action", "action": "cards.claim",
                   "input_template": {"who": "{{run.inputs.who}}", "n": 2}}],
        "edges": []
    }));
    let mut host = CaptureHost::new();
    let Drive::Executed(step) = drive_once(&definition, &mut host).expect("drives") else {
        panic!("expected an executed step");
    };
    assert_eq!(host.calls.len(), 1);
    assert_eq!(host.calls[0].1, json!({"who": "george", "n": 2}));
    assert_eq!(step.input, json!({"who": "george", "n": 2}));
    assert_eq!(host.calls[0].2, nodes::idempotency_key(Uuid::nil(), "a", 1));
    assert_eq!(host.calls[0].3, 30_000);
}

#[test]
fn the_closed_predicate_set_evaluates() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "eq", "kind": "condition", "predicate": {"op": "eq", "left": "{{run.inputs.who}}", "right": "george"}},
            {"id": "ne", "kind": "condition", "predicate": {"op": "ne", "left": 1, "right": 2}},
            {"id": "lt", "kind": "condition", "predicate": {"op": "lt", "left": 1, "right": 2}},
            {"id": "le", "kind": "condition", "predicate": {"op": "le", "left": 2, "right": 2}},
            {"id": "gt", "kind": "condition", "predicate": {"op": "gt", "left": 3, "right": 2}},
            {"id": "ge", "kind": "condition", "predicate": {"op": "ge", "left": 2, "right": 2}},
            {"id": "in", "kind": "condition", "predicate": {"op": "in", "left": "george", "right": ["a", "george"]}},
            {"id": "empty", "kind": "condition", "predicate": {"op": "empty", "left": "{{run.inputs.missing}}"}},
            {"id": "exists", "kind": "condition", "predicate": {"op": "exists", "left": "{{run.inputs.who}}"}},
            {"id": "and", "kind": "condition", "predicate": {"op": "and", "of": [
                {"op": "eq", "left": 1, "right": 1}, {"op": "eq", "left": 2, "right": 2}]}},
            {"id": "or", "kind": "condition", "predicate": {"op": "or", "of": [
                {"op": "eq", "left": 1, "right": 2}, {"op": "eq", "left": 2, "right": 2}]}},
            {"id": "not", "kind": "condition", "predicate": {"op": "not", "of": {"op": "eq", "left": 1, "right": 2}}}
        ],
        "edges": []
    }));
    let ctx = ctx();
    for (id, expected) in [
        ("eq", true),
        ("ne", true),
        ("lt", true),
        ("le", true),
        ("gt", true),
        ("ge", true),
        ("in", true),
        ("empty", true),
        ("exists", true),
        ("and", true),
        ("or", true),
        ("not", true),
    ] {
        assert_eq!(
            nodes::evaluate(node(&definition, id), &ctx).expect(id),
            expected,
            "predicate {id}"
        );
    }
    // The same nodes against a value that makes them false.
    let false_ctx = TemplateCtx {
        run: json!({"id": "run-1", "inputs": {"who": ""}}),
        steps: Vec::new(),
        card: json!({"id": "card-1"}),
        // A workflow has no board (CONTRACT §21); the call step is the only board-aware seam.
        board: json!({}),
        process: json!({"key": "wf", "version": 1}),
        position: 1,
    };
    // Only `eq` reads the context here; `in`'s operands are literals and stay true by construction.
    assert!(!nodes::evaluate(node(&definition, "eq"), &false_ctx).expect("eq"));
    // `exists` is "present and not null" (definition.rs): "" is present, so it stays true.
    assert!(nodes::evaluate(node(&definition, "exists"), &false_ctx).expect("exists"));
    assert!(nodes::evaluate(node(&definition, "empty"), &false_ctx).expect("empty"));
}

#[test]
fn loop_items_must_resolve_to_an_array_and_its_length_is_the_count() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "ok", "kind": "loop", "items": "{{run.inputs.items}}", "max_iterations": 10},
            {"id": "bad", "kind": "loop", "items": "{{run.inputs.who}}", "max_iterations": 10}
        ],
        "edges": []
    }));
    let ctx = ctx();
    assert_eq!(
        nodes::loop_items(node(&definition, "ok"), &ctx).expect("counts"),
        3
    );
    let error = nodes::loop_items(node(&definition, "bad"), &ctx).expect_err("not an array");
    assert!(matches!(error, NodeError::Items { .. }), "{error:?}");
}

#[test]
fn a_condition_node_records_its_verdict_and_a_loop_records_its_count() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "c", "kind": "condition", "predicate": {"op": "eq", "left": 1, "right": 1}},
            {"id": "l", "kind": "loop", "items": "{{run.inputs.items}}", "max_iterations": 10},
            {"id": "after", "kind": "action", "action": "cards.claim"}
        ],
        "edges": [{"from": "c", "to": "l", "when": "true"}, {"from": "l", "to": "after", "when": "done"}]
    }));
    let rows: Vec<RecordedStep> = Vec::new();
    let positions = Positions::new(&rows);
    let ctx = ctx();
    let mut engine = Engine::resume(&definition, &rows).expect("starts");

    let verdict = match conveyor_workflow::scheduler::plan(
        &definition,
        &mut engine,
        Uuid::nil(),
        &ctx,
        &positions,
    )
    .expect("plans")
    {
        Plan::Recorded(row) => row,
        other => panic!("expected the condition, got {other:?}"),
    };
    assert_eq!(verdict.node_id, "c");
    assert_eq!(verdict.output, json!(true));

    let mut rows = vec![verdict];
    let mut engine = Engine::resume(&definition, &rows).expect("replays");
    let positions = Positions::new(&rows);
    let count = match conveyor_workflow::scheduler::plan(
        &definition,
        &mut engine,
        Uuid::nil(),
        &ctx,
        &positions,
    )
    .expect("plans")
    {
        Plan::Recorded(row) => row,
        other => panic!("expected the loop, got {other:?}"),
    };
    assert_eq!(count.node_id, "l");
    assert_eq!(count.output, json!({"items": 3}));
    assert_eq!(count.status, RecordedStatus::Succeeded);
    rows.push(count);
    // Both rows replay: the condition's verdict and the loop's count are all the state there is.
    assert!(Engine::resume(&definition, &rows).is_ok());
}
