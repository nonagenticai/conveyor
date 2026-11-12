#![forbid(unsafe_code)]
//! The answer-key isolation test (S19 Part A step 2; CONTRACT §20 trap 6).
//!
//! The isolation is a **type**, not a filter and not an instruction in a prompt: the struct a
//! model is handed has exactly one field. This test asserts the *serialised shape*, because that
//! shape is what a prompt builder, an MCP tool and an SSE payload all derive from — a field that
//! does not exist here cannot be forgotten somewhere else.

use conveyor_domain::eval::SubjectInput;
use serde_json::json;

#[test]
fn subject_input_carries_no_expected_bytes() {
    // The string itself says "expected" — several times, in several spellings. Data that names
    // an answer key must not become an answer-key *field*.
    let input = SubjectInput {
        input: "expected: 42; expected_output: null; {\"expected\":[1]}",
    };

    let value = serde_json::to_value(input).expect("SubjectInput serialises");
    let object = value.as_object().expect("SubjectInput is a JSON object");

    assert_eq!(
        object.len(),
        1,
        "exactly one field crosses the boundary: {value}"
    );
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<&str>>(),
        vec!["input"],
        "the only key is `input`"
    );
    assert!(
        !object.keys().any(|key| key.contains("expected")),
        "no key may name an answer key: {value}"
    );
    assert_eq!(object["input"], json!(input.input));
    assert_eq!(value, json!({ "input": input.input }));
}
