//! S13 step 6's gate: the scheduler plans one attempt per call, the trace is the run's only state,
//! and a resumed run continues exactly where its rows stopped. No database, no clock.

use std::collections::VecDeque;

use conveyor_domain::template::TemplateCtx;
use conveyor_workflow::engine::{RecordedStatus, RecordedStep};
use conveyor_workflow::scheduler::{
    GateState, HostResult, HostStatus, NodeRequest, Plan, Positions,
};
use conveyor_workflow::{Definition, Engine, EngineError, RunStatus};
use serde_json::{Value, json};
use uuid::Uuid;

fn definition(raw: Value) -> Definition {
    Definition::decode(&serde_json::to_vec(&raw).expect("serialises")).expect("decodes")
}

fn action(id: &str) -> Value {
    json!({"id": id, "kind": "action", "action": "cards.claim"})
}

fn ctx() -> TemplateCtx {
    TemplateCtx {
        run: json!({"id": "run-1", "inputs": {}}),
        steps: Vec::new(),
        card: json!({"id": "card-1"}),
        // A workflow has no board (CONTRACT §21).
        board: json!({}),
        process: json!({"key": "wf", "version": 1}),
        position: 1,
    }
}

fn row(
    position: u32,
    node: &str,
    attempt: u32,
    status: RecordedStatus,
    output: Value,
) -> RecordedStep {
    RecordedStep {
        position,
        node_id: node.to_owned(),
        attempt,
        status,
        input: json!({}),
        output,
        duration_ms: 0,
        error: None,
    }
}

/// A host with a scripted result queue; every call is recorded so order and keys are assertable.
struct FakeHost {
    calls: Vec<(String, u32, String)>,
    results: VecDeque<HostResult>,
    gate: GateState,
}

impl FakeHost {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            results: VecDeque::new(),
            gate: GateState::Pending,
        }
    }

    fn scripted(results: impl IntoIterator<Item = HostResult>) -> Self {
        let mut host = Self::new();
        host.results.extend(results);
        host
    }
}

impl FakeHost {
    fn execute(&mut self, request: &NodeRequest) -> HostResult {
        self.calls.push((
            request.node_id.clone(),
            request.attempt,
            request.idempotency_key.clone(),
        ));
        self.results
            .pop_front()
            .unwrap_or_else(|| HostResult::succeeded(json!({})))
    }

    fn gate(&mut self, _request: &NodeRequest) -> GateState {
        self.gate
    }
}

/// What one plan+apply round did, for assertions.
#[derive(Debug)]
enum Drive {
    Executed(RecordedStep),
    Waiting { node_id: String },
    Deferred,
    Finished(RunStatus),
}

/// The test driver: plan, run the fake host, apply. Exactly what the actions layer does with real I/O.
fn drive_once(
    definition: &Definition,
    engine: &mut Engine,
    rows: &[RecordedStep],
    host: &mut FakeHost,
) -> Drive {
    let positions = Positions::new(rows);
    let plan =
        conveyor_workflow::scheduler::plan(definition, engine, Uuid::nil(), &ctx(), &positions)
            .expect("plans");
    match plan {
        Plan::Recorded(row) => Drive::Executed(row),
        Plan::Execute(request) | Plan::Poll(request) => {
            let result = host.execute(&request);
            if result.status == HostStatus::Deferred {
                return Drive::Deferred;
            }
            Drive::Executed(
                conveyor_workflow::scheduler::apply(definition, engine, &request, result)
                    .expect("applies"),
            )
        }
        Plan::Gate(request) => {
            let state = host.gate(&request);
            match conveyor_workflow::scheduler::apply_gate(definition, engine, &request, state)
                .expect("applies")
            {
                Some(row) => Drive::Executed(row),
                None => Drive::Waiting {
                    node_id: request.node_id,
                },
            }
        }
        Plan::Waiting { node_id } => Drive::Waiting { node_id },
        Plan::Finished(status) => Drive::Finished(status),
    }
}

#[test]
fn the_trace_is_the_state_a_resumed_engine_continues_exactly_where_the_rows_stop() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [action("a"), action("b"), action("c")],
        "edges": [{"from": "a", "to": "b"}, {"from": "b", "to": "c"}]
    }));
    let rows = [row(1, "a", 1, RecordedStatus::Succeeded, json!({}))];
    let mut resumed = Engine::resume(&definition, &rows).expect("replays");

    let mut host = FakeHost::new();
    let Drive::Executed(next) = drive_once(&definition, &mut resumed, &rows, &mut host) else {
        panic!("expected one executed step");
    };
    assert_eq!(next.node_id, "b");
    assert_eq!(next.position, 2, "the next node takes the next position");
    assert_eq!(
        host.calls.len(),
        1,
        "the replayed node is never re-executed"
    );
}

#[test]
fn resume_refuses_a_trace_that_is_not_the_planned_prefix() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [action("a"), action("b")],
        "edges": [{"from": "a", "to": "b"}]
    }));
    // The trace says `b` ran first; the document says `a` is the start node.
    let rows = [row(1, "b", 1, RecordedStatus::Succeeded, json!({}))];
    let error = match Engine::resume(&definition, &rows) {
        Ok(_) => panic!("the trace must diverge"),
        Err(error) => error,
    };
    assert!(
        matches!(error, EngineError::TraceDiverged { .. }),
        "{error:?}"
    );
}

#[test]
fn a_running_row_resumes_the_in_flight_attempt_and_the_host_reports_its_outcome() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [action("a"), action("b")],
        "edges": [{"from": "a", "to": "b"}]
    }));
    let rows = [row(1, "a", 1, RecordedStatus::Running, Value::Null)];
    let mut resumed = Engine::resume(&definition, &rows).expect("replays");

    let mut host = FakeHost::scripted([HostResult::succeeded(json!({"done": true}))]);
    let Drive::Executed(finished) = drive_once(&definition, &mut resumed, &rows, &mut host) else {
        panic!("expected the in-flight attempt to finish");
    };
    assert_eq!(finished.node_id, "a");
    assert_eq!(finished.status, RecordedStatus::Succeeded);
    assert_eq!(finished.position, 1, "a resumed attempt keeps its position");
    assert_eq!(
        host.calls[0].1, 1,
        "the same attempt is polled, not a new one"
    );
}

#[test]
fn drive_runs_one_step_per_call_and_finishes_only_after_the_last_row() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [action("a"), action("b"), action("c")],
        "edges": [{"from": "a", "to": "b"}, {"from": "b", "to": "c"}]
    }));
    let mut rows: Vec<RecordedStep> = Vec::new();
    let mut host = FakeHost::new();
    let mut engine = Engine::resume(&definition, &rows).expect("starts");
    let mut seen = Vec::new();
    loop {
        match drive_once(&definition, &mut engine, &rows, &mut host) {
            Drive::Executed(step) => {
                seen.push(step.node_id.clone());
                rows.push(step);
                engine = Engine::resume(&definition, &rows).expect("replays");
            }
            Drive::Finished(status) => {
                assert_eq!(status, RunStatus::Succeeded);
                break;
            }
            other => panic!("unexpected drive: {other:?}"),
        }
        assert!(rows.len() <= 3, "the driver must terminate");
    }
    assert_eq!(seen, vec!["a", "b", "c"]);
    assert_eq!(
        rows.iter().map(|row| row.position).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn a_retry_appends_a_row_at_the_same_position_with_a_new_attempt() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "a", "kind": "action", "action": "cards.claim",
             "on_error": "retry", "max_attempts": 2},
            action("b")
        ],
        "edges": [{"from": "a", "to": "b"}]
    }));
    let mut host = FakeHost::scripted([
        HostResult::failed("timeout"),
        HostResult::succeeded(json!({"ok": true})),
    ]);
    let mut rows: Vec<RecordedStep> = Vec::new();

    let mut engine = Engine::resume(&definition, &rows).expect("starts");
    let Drive::Executed(first) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the failed attempt");
    };
    assert_eq!((first.position, first.attempt), (1, 1));
    assert_eq!(first.status, RecordedStatus::Failed);
    rows.push(first);

    let mut engine = Engine::resume(&definition, &rows).expect("replays the failure");
    let Drive::Executed(second) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the retry");
    };
    assert_eq!(
        (second.position, second.attempt),
        (1, 2),
        "a retry appends attempt 2 at the same position"
    );
    assert_eq!(second.status, RecordedStatus::Succeeded);
    assert_ne!(
        host.calls[0].2, host.calls[1].2,
        "each attempt has its own key"
    );
}

#[test]
fn a_continue_node_is_recorded_as_skipped_and_the_run_goes_on() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "a", "kind": "action", "action": "cards.claim", "on_error": "continue"},
            action("b")
        ],
        "edges": [{"from": "a", "to": "b"}]
    }));
    let mut host = FakeHost::scripted([HostResult::failed("boom")]);
    let mut rows: Vec<RecordedStep> = Vec::new();
    let mut engine = Engine::resume(&definition, &rows).expect("starts");

    let Drive::Executed(step) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the skipped node");
    };
    assert_eq!(step.status, RecordedStatus::Skipped);
    rows.push(step);

    let mut engine = Engine::resume(&definition, &rows).expect("replays");
    let Drive::Executed(next) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the next node");
    };
    assert_eq!(next.node_id, "b");
}

#[test]
fn a_gate_waits_and_a_decided_gate_row_resumes_the_run() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            action("start"),
            {"id": "g", "kind": "gate", "gate_kind": "review"},
            action("end")
        ],
        "edges": [{"from": "start", "to": "g"}, {"from": "g", "to": "end"}]
    }));
    let mut rows: Vec<RecordedStep> = Vec::new();
    let mut host = FakeHost::new();
    let mut engine = Engine::resume(&definition, &rows).expect("starts");

    let Drive::Executed(first) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the start node");
    };
    rows.push(first);

    let mut engine = Engine::resume(&definition, &rows).expect("replays");
    let Drive::Waiting { node_id } = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the gate to wait");
    };
    assert_eq!(node_id, "g");

    // The decision is recorded as the gate's row; the next job replays it and continues.
    rows.push(row(2, "g", 1, RecordedStatus::Succeeded, json!(true)));
    let mut engine = Engine::resume(&definition, &rows).expect("replays the decision");
    let Drive::Executed(next) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the node after the gate");
    };
    assert_eq!(next.node_id, "end");
    assert_eq!(next.position, 3);
}

#[test]
fn a_refused_gate_fails_the_run() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [
            {"id": "g", "kind": "gate", "gate_kind": "review"},
            action("end")
        ],
        "edges": [{"from": "g", "to": "end"}]
    }));
    let rows: Vec<RecordedStep> = Vec::new();
    let mut host = FakeHost::new();
    host.gate = GateState::Refused;
    let mut engine = Engine::resume(&definition, &rows).expect("starts");

    let Drive::Executed(step) = drive_once(&definition, &mut engine, &rows, &mut host) else {
        panic!("expected the refusal to be recorded");
    };
    assert_eq!(step.node_id, "g");
    assert_eq!(step.status, RecordedStatus::Failed);
    assert_eq!(engine.status(), RunStatus::Failed);
}

#[test]
fn fan_out_runs_in_document_order_and_positions_are_execution_order() {
    let definition = definition(json!({
        "schema": 1,
        "nodes": [action("start"), action("b"), action("join"), action("c")],
        "edges": [
            {"from": "start", "to": "b"},
            {"from": "start", "to": "c"},
            {"from": "b", "to": "join"},
            {"from": "c", "to": "join"}
        ]
    }));
    let mut rows: Vec<RecordedStep> = Vec::new();
    let mut host = FakeHost::new();
    let mut engine = Engine::resume(&definition, &rows).expect("starts");
    let mut order = Vec::new();
    loop {
        match drive_once(&definition, &mut engine, &rows, &mut host) {
            Drive::Executed(step) => {
                order.push(step.node_id.clone());
                rows.push(step);
                engine = Engine::resume(&definition, &rows).expect("replays");
            }
            Drive::Finished(status) => {
                assert_eq!(status, RunStatus::Succeeded);
                break;
            }
            other => panic!("unexpected drive: {other:?}"),
        }
        assert!(rows.len() <= 4, "the driver must terminate");
    }
    assert_eq!(order, vec!["start", "b", "c", "join"]);
    assert_eq!(
        rows.iter().map(|row| row.position).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "positions are execution order, not document order"
    );
}
