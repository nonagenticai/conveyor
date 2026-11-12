//! S13 step 8's gate: the `agent.run` loop and its caps — all pure, no database, no clock and no
//! model. Every cap test is the contract's sentence made executable: a cap fails **typed**, never
//! silently (CONTRACT §17, S13 §8 trap 5).

use conveyor_workflow::agent::{
    Agent, AgentConfig, AgentError, Message, ModelReply, Plan, Reply, ToolCall, ToolReply,
};
use serde_json::json;
use uuid::Uuid;

fn call(id: &str, name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    }
}

fn config() -> AgentConfig {
    AgentConfig {
        model: "test-model".to_owned(),
        messages: vec![Message::text("system", "you are a test")],
        tools: vec![json!({"name": "cards.move"})],
        max_turns: None,
        max_tool_calls: None,
        timeout_ms: None,
        capability_agent_id: None,
        prompt_ref: None,
        run_id: None,
        run_step_id: None,
        depth: None,
    }
}

fn start(config: AgentConfig) -> (Agent, Plan) {
    Agent::start(config).expect("the config is inside the contract's ceilings")
}

fn expect_model(plan: Plan) -> (String, Vec<Message>, Vec<serde_json::Value>) {
    match plan {
        Plan::CallModel {
            model,
            messages,
            tools,
        } => (model, messages, tools),
        other => panic!("expected CallModel, got {other:?}"),
    }
}

fn expect_tool(plan: Plan) -> ToolCall {
    match plan {
        Plan::CallTool(call) => call,
        other => panic!("expected CallTool, got {other:?}"),
    }
}

fn expect_failed(plan: Plan) -> AgentError {
    match plan {
        Plan::Failed(error) => error,
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_two_turn_run_answers_after_one_tool() {
    let (mut agent, plan) = start(AgentConfig {
        max_turns: Some(3),
        ..config()
    });
    let (model, messages, _) = expect_model(plan);
    assert_eq!(model, "test-model");
    assert_eq!(
        messages.len(),
        1,
        "nothing is invented before the first reply"
    );

    let first = call("call-1", "cards.move", json!({"card_id": "c1"}));
    let plan = agent.apply(Reply::Model(ModelReply::calls(vec![first.clone()])));
    assert_eq!(expect_tool(plan), first);

    let plan = agent.apply(Reply::Tool(ToolReply {
        id: first.id.clone(),
        content: r#"{"ok":true}"#.to_owned(),
        is_error: false,
        invocation_id: Some(Uuid::now_v7()),
    }));
    let (_, messages, _) = expect_model(plan);
    // system + assistant(tool_call) + tool
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].tool_calls.len(), 1);
    assert_eq!(messages[2].tool_call_id.as_deref(), Some("call-1"));

    let plan = agent.apply(Reply::Model(ModelReply::text("done")));
    match plan {
        Plan::Finished(output) => {
            assert_eq!(output.output, "done");
            assert_eq!(output.turns, 2);
            assert_eq!(output.tool_calls.len(), 1);
            assert_eq!(output.tool_calls[0].name, "cards.move");
            assert!(!output.tool_calls[0].is_error);
        }
        other => panic!("expected Finished, got {other:?}"),
    }
    // Terminal: another reply cannot reopen the loop.
    let plan = agent.apply(Reply::Model(ModelReply::text("again")));
    assert!(matches!(plan, Plan::Finished(_)));
}

#[test]
fn the_turn_cap_fails_typed_and_does_not_run_the_last_tools() {
    let (mut agent, plan) = start(AgentConfig {
        max_turns: Some(2),
        ..config()
    });
    expect_model(plan);

    let first = call("call-1", "cards.move", json!({}));
    let plan = agent.apply(Reply::Model(ModelReply::calls(vec![first.clone()])));
    expect_tool(plan);
    let plan = agent.apply(Reply::Tool(ToolReply {
        id: first.id.clone(),
        content: "{}".to_owned(),
        is_error: false,
        invocation_id: None,
    }));
    expect_model(plan); // turn 2

    // Turn 2 is the last allowed turn; asking for a tool now cannot be answered, so the loop fails
    // instead of executing an effect it can never use.
    let second = call("call-2", "cards.move", json!({}));
    let plan = agent.apply(Reply::Model(ModelReply::calls(vec![second])));
    match expect_failed(plan) {
        AgentError::TurnLimit { turns, cap } => {
            assert_eq!((turns, cap), (2, 2));
        }
        other => panic!("expected TurnLimit, got {other:?}"),
    }
    assert_eq!(agent.tool_calls(), 1, "the over-cap tool was never run");
}

#[test]
fn the_tool_cap_fails_typed_before_any_call_in_the_batch_runs() {
    let (mut agent, plan) = start(AgentConfig {
        max_tool_calls: Some(2),
        ..config()
    });
    expect_model(plan);

    let batch = vec![
        call("c1", "cards.move", json!({})),
        call("c2", "cards.move", json!({})),
        call("c3", "cards.move", json!({})),
    ];
    let plan = agent.apply(Reply::Model(ModelReply::calls(batch)));
    match expect_failed(plan) {
        AgentError::ToolLimit { requested, cap } => assert_eq!((requested, cap), (3, 2)),
        other => panic!("expected ToolLimit, got {other:?}"),
    }
    assert_eq!(agent.tool_calls(), 0, "no call of the refused batch ran");
}

#[test]
fn a_config_outside_the_ceiling_is_refused_never_clamped() {
    let cases: [(AgentConfig, &str); 4] = [
        (
            AgentConfig {
                max_turns: Some(21),
                ..config()
            },
            "max_turns",
        ),
        (
            AgentConfig {
                max_tool_calls: Some(51),
                ..config()
            },
            "max_tool_calls",
        ),
        (
            AgentConfig {
                timeout_ms: Some(300_001),
                ..config()
            },
            "timeout_ms",
        ),
        (
            AgentConfig {
                depth: Some(1),
                ..config()
            },
            "depth",
        ),
    ];
    for (config, field) in cases {
        let error = Agent::start(config).expect_err("refused");
        if field == "depth" {
            assert!(matches!(error, AgentError::Depth { depth: 1 }), "{error:?}");
        } else {
            match error {
                AgentError::CapInvalid { field: named, .. } => assert_eq!(named, field),
                other => panic!("expected CapInvalid for {field}, got {other:?}"),
            }
        }
    }
}

#[test]
fn the_defaults_are_the_contract_defaults() {
    let (agent, _) = start(config());
    let caps = agent.caps();
    assert_eq!(caps.turns, 8);
    assert_eq!(caps.tool_calls, 20);
    assert_eq!(caps.timeout_ms, 300_000);
    assert_eq!(caps.depth, 0);
}

#[test]
fn the_wall_clock_is_typed() {
    let (mut agent, plan) = start(config());
    expect_model(plan);
    let plan = agent.apply(Reply::Elapsed {
        elapsed_ms: 300_001,
    });
    match expect_failed(plan) {
        AgentError::Timeout { elapsed_ms, cap } => {
            assert_eq!((elapsed_ms, cap), (300_001, 300_000))
        }
        other => panic!("expected Timeout, got {other:?}"),
    }
    // Under the cap the pending plan is re-emitted unchanged.
    let (mut agent, plan) = start(config());
    expect_model(plan);
    let plan = agent.apply(Reply::Elapsed {
        elapsed_ms: 299_999,
    });
    expect_model(plan);
}

#[test]
fn a_refused_tool_is_fed_back_and_the_loop_continues() {
    let (mut agent, plan) = start(AgentConfig {
        max_turns: Some(3),
        ..config()
    });
    expect_model(plan);
    let call = call("c1", "secrets.create", json!({}));
    expect_tool(agent.apply(Reply::Model(ModelReply::calls(vec![call.clone()]))));
    let plan = agent.apply(Reply::Tool(ToolReply {
        id: call.id.clone(),
        content: r#"{"type":"/problems/action-not-found"}"#.to_owned(),
        is_error: true,
        invocation_id: None,
    }));
    let (_, messages, _) = expect_model(plan);
    assert_eq!(messages.last().expect("a tool message").role, "tool");
    let plan = agent.apply(Reply::Model(ModelReply::text("understood")));
    let Plan::Finished(output) = plan else {
        panic!("expected Finished");
    };
    assert!(output.tool_calls[0].is_error, "the refusal is recorded");
}

#[test]
fn a_reply_for_another_call_is_ignored() {
    let (mut agent, plan) = start(config());
    expect_model(plan);
    let call = call("c1", "cards.move", json!({}));
    expect_tool(agent.apply(Reply::Model(ModelReply::calls(vec![call.clone()]))));

    let plan = agent.apply(Reply::Tool(ToolReply {
        id: "someone-else".to_owned(),
        content: "{}".to_owned(),
        is_error: false,
        invocation_id: None,
    }));
    assert_eq!(expect_tool(plan), call, "the loop still waits for c1");
    assert_eq!(agent.tool_calls(), 0);
}

#[test]
fn the_error_codes_are_the_contracts() {
    let cases = [
        (
            AgentError::TurnLimit { turns: 20, cap: 20 },
            "agent.turn_limit",
        ),
        (
            AgentError::ToolLimit {
                requested: 51,
                cap: 50,
            },
            "agent.tool_limit",
        ),
        (
            AgentError::Timeout {
                elapsed_ms: 1,
                cap: 1,
            },
            "agent.timeout",
        ),
        (
            AgentError::CapInvalid {
                field: "max_turns",
                value: 21,
                cap: 20,
            },
            "agent.cap_invalid",
        ),
        (AgentError::Depth { depth: 1 }, "agent.depth"),
    ];
    for (error, code) in cases {
        assert_eq!(error.code(), code);
    }
}

#[test]
fn the_resolved_ids_travel_into_the_output() {
    let agent_id = Uuid::now_v7();
    let (mut agent, plan) = start(AgentConfig {
        capability_agent_id: Some(agent_id),
        prompt_ref: Some("capability_agent@3".to_owned()),
        ..config()
    });
    expect_model(plan);
    let plan = agent.apply(Reply::Model(ModelReply::text("answer")));
    let Plan::Finished(output) = plan else {
        panic!("expected Finished");
    };
    assert_eq!(output.capability_agent_id, Some(agent_id));
    assert_eq!(output.prompt_ref.as_deref(), Some("capability_agent@3"));
}
