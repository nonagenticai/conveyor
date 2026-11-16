//! Blank / vacuous completion classification — the #140 P1 port of the reference router's
//! `_empty_completion_reason`, `_every_choice_blank`, `_is_vacuous_json` and `_is_empty_value`.
//!
//! Fixtures are built from the wire JSON rather than struct literals: the classifier is a consumer
//! of the captured contract, so its tests should read the same shapes a client receives.

use serde_json::{Value, json};

use conveyor_inference_router::contract::schemas::ChatCompletionResponse;
use conveyor_inference_router::empty::{
    EmptyReason, completion_reason, every_choice_blank, is_vacuous_json,
};

/// A one-choice assistant reply. `tool_calls` is sent as JSON null unless given.
fn response(content: Value) -> ChatCompletionResponse {
    let choices = json!([{"message": {"role": "assistant", "content": content}}]);
    serde_json::from_value(json!({"model": "gemma", "choices": choices}))
        .expect("one-choice response fixture")
}

/// A one-choice reply carrying a tool call; the provider keeps its content null.
fn tool_call_response(content: Value) -> ChatCompletionResponse {
    let choices = json!([{
        "message": {
            "role": "assistant",
            "content": content,
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "lookup", "arguments": "{}"},
            }],
        },
    }]);
    serde_json::from_value(json!({"model": "gemma", "choices": choices}))
        .expect("tool-call response fixture")
}

fn multi_choice(contents: &[Value]) -> ChatCompletionResponse {
    let choices: Vec<Value> = contents
        .iter()
        .map(|content| json!({"message": {"role": "assistant", "content": content}}))
        .collect();
    serde_json::from_value(json!({"model": "gemma", "choices": choices}))
        .expect("multi-choice response fixture")
}

/// `depth` wrapping objects around an innermost `{}`, for the vacuity depth cap.
fn nested_objects(depth: usize) -> Value {
    let mut value = json!({});
    for _ in 0..depth {
        value = json!({"a": value});
    }
    value
}

#[test]
fn a_null_content_is_blank() {
    assert_eq!(
        completion_reason(&response(Value::Null)),
        Some(EmptyReason::Blank)
    );
}

#[test]
fn a_whitespace_only_content_is_blank() {
    for text in ["", " ", "\n", "\n\n", " \t\r\n "] {
        assert_eq!(
            completion_reason(&response(json!(text))),
            Some(EmptyReason::Blank),
            "{text:?}"
        );
    }
}

#[test]
fn no_choices_at_all_is_blank() {
    let empty: ChatCompletionResponse =
        serde_json::from_value(json!({"model": "gemma", "choices": []})).expect("fixture");
    assert_eq!(completion_reason(&empty), Some(EmptyReason::Blank));
}

#[test]
fn empty_json_shapes_are_empty_results() {
    for text in [
        "{}",
        "[]",
        r#"{"a": []}"#,
        r#"{"a": ""}"#,
        r#"{"a": null}"#,
        r#"{"a": {"b": []}, "c": "  "}"#,
        r#"{"relationships": []}"#,
    ] {
        assert_eq!(
            completion_reason(&response(json!(text))),
            Some(EmptyReason::EmptyResult),
            "{text}"
        );
    }
}

#[test]
fn a_scalar_value_is_a_real_answer() {
    // 0 / false are answers: {"n": 0} answers "how many". Not vacuous, so not an empty result.
    for text in [
        r#"{"a": 0}"#,
        r#"{"a": false}"#,
        r#"{"n": 3}"#,
        r#"{"score": 0.0}"#,
    ] {
        assert_eq!(completion_reason(&response(json!(text))), None, "{text}");
        assert!(!is_vacuous_json(text), "{text}");
    }
}

#[test]
fn a_non_empty_list_of_hollow_objects_is_not_vacuous() {
    // The model asserted one relationship exists; second-guessing its contents is a schema
    // question, not an emptiness question.
    let text = r#"{"relationships": [{}]}"#;
    assert!(!is_vacuous_json(text));
    assert_eq!(completion_reason(&response(json!(text))), None);
}

#[test]
fn prose_and_top_level_scalars_are_not_vacuous() {
    for text in [
        "ok",
        "Sure, here is the answer.",
        "3",
        "\"done\"",
        "true",
        "null",
    ] {
        assert!(!is_vacuous_json(text), "{text}");
        assert_eq!(completion_reason(&response(json!(text))), None, "{text}");
    }
}

#[test]
fn a_tool_call_reply_is_never_blank_or_empty() {
    for content in [Value::Null, json!(""), json!("  "), json!("{}")] {
        let response = tool_call_response(content.clone());
        assert_eq!(completion_reason(&response), None, "{content}");
        assert!(!every_choice_blank(&response), "{content}");
    }
}

#[test]
fn non_string_content_is_usable() {
    let parts = json!([{"type": "text", "text": "hi"}]);
    let response = response(parts);
    assert_eq!(completion_reason(&response), None);
    assert!(!every_choice_blank(&response));
}

#[test]
fn a_fenced_json_block_is_classified_by_its_body() {
    let fenced_empty = "```json\n{\"a\": []}\n```";
    assert!(is_vacuous_json(fenced_empty));
    assert_eq!(
        completion_reason(&response(json!(fenced_empty))),
        Some(EmptyReason::EmptyResult)
    );

    let fenced_answer = "```json\n{\"a\": 1}\n```";
    assert!(!is_vacuous_json(fenced_answer));
    assert_eq!(completion_reason(&response(json!(fenced_answer))), None);
}

#[test]
fn the_vacuity_walk_stops_at_depth_twelve() {
    // Innermost {} at depth 11: still empty. One level deeper: past the cap, so not empty.
    assert!(is_vacuous_json(&nested_objects(12).to_string()));
    assert!(!is_vacuous_json(&nested_objects(13).to_string()));
}

#[test]
fn every_choice_blank_is_false_when_any_choice_is_usable() {
    let all_blank = multi_choice(&[Value::Null, json!("  ")]);
    assert!(every_choice_blank(&all_blank));

    let one_usable = multi_choice(&[Value::Null, json!("an answer")]);
    assert!(!every_choice_blank(&one_usable));

    let no_choices: ChatCompletionResponse =
        serde_json::from_value(json!({"model": "gemma", "choices": []})).expect("fixture");
    assert!(every_choice_blank(&no_choices));
}
