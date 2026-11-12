#![forbid(unsafe_code)]
//! The grader table and the subject-request rule, through the **public** API (S19 Part B
//! step 11; CONTRACT §20).
//!
//! `crates/domain/src/eval/grade.rs` asserts the modes individually as unit tests. This file
//! asserts the two properties a reader of the crate depends on and a unit test cannot: that the
//! oracle/ null controls hold for every mode *as the driver will call them*, and that the
//! request a case sends to its entry action is built from the input alone — an eval whose case
//! carries an answer key must produce a byte-identical request to one whose case does not.

use conveyor_domain::eval::{
    Expectation, GraderKind, GradingError, SubjectInput, grade_programmatic, parse_verdict,
    subject_request,
};
use serde_json::json;

const KEYS: [&str; 4] = [
    "The deploy was rolled back because the migration locked the table.",
    "Cache invalidation after the schema change.",
    "Retry with backoff; the upstream 503 is transient.",
    "Nothing changed.",
];

#[test]
fn the_oracle_scores_every_case_and_the_null_scores_none() {
    // The two controls the audit (Part C) reruns through the real runner: an answer that is the
    // key must score 1.0 everywhere, and an empty answer must score 0.0 everywhere. A mode that
    // fails either is a mode whose numbers cannot be read.
    for expect in ["exact", "contains", "label"] {
        let mut grader = json!({ "expect": expect });
        if expect == "label" {
            grader = json!({ "expect": "label", "labels": KEYS });
        }
        for key in KEYS {
            let oracle = grade_programmatic("answer", &grader, Some(&json!(key)), &json!(key))
                .expect("the oracle graded");
            assert_eq!(oracle.score, 1.0, "{expect}: oracle on {key:?}");

            let null = grade_programmatic("answer", &grader, Some(&json!(key)), &json!(""))
                .expect("the null graded");
            assert_eq!(null.score, 0.0, "{expect}: null on {key:?}");
        }
    }

    // `schema` needs no answer key at all: the shape is the whole grade.
    let schema = json!({"expect": "schema", "schema": {"type": "object", "required": ["cause"]}});
    for answer in [
        json!({"cause": "lock"}),
        json!({"cause": "lock", "blame": "x"}),
    ] {
        let grade = grade_programmatic("shape", &schema, None, &answer).expect("the oracle graded");
        assert_eq!(grade.score, 1.0);
    }
    let null =
        grade_programmatic("shape", &schema, None, &json!({})).expect_err("the null refused");
    assert!(matches!(null, GradingError::ShapeMismatch { .. }));
}

#[test]
fn every_expectation_is_reachable_by_its_wire_name() {
    for expect in Expectation::ALL {
        let grader = json!({ "expect": expect.as_str(), "schema": {"type": "string"} });
        assert_eq!(
            Expectation::from_grader(&grader).expect("parsed"),
            expect,
            "{expect}"
        );
    }
    // An absent `expect` is the strictest mode, so a forgetting author under-scores rather than
    // over-scores.
    assert_eq!(
        Expectation::from_grader(&json!({})).expect("defaulted"),
        Expectation::Exact
    );
}

#[test]
fn only_the_two_model_graders_call_a_model() {
    assert!(GraderKind::Rubric.calls_model());
    assert!(GraderKind::Pairwise.calls_model());
    assert!(!GraderKind::Programmatic.calls_model());
    assert!(!GraderKind::Human.calls_model());
    assert!(!GraderKind::None.calls_model());
}

#[test]
fn a_judge_verdict_outside_the_contract_is_a_failure_not_a_zero() {
    // A zero would read as "the model was wrong" and inflate a regression. The driver records
    // this as `failure_class = 'grader'` instead.
    for bad in [
        json!({}),
        json!({"score": 2, "explanation": "out of range"}),
        json!({"score": 0.4}),
        json!({"score": 0.4, "explanation": ""}),
    ] {
        assert!(
            matches!(
                parse_verdict(&bad),
                Err(GradingError::MalformedVerdict { .. })
            ),
            "{bad}"
        );
    }
    let good = parse_verdict(&json!({"score": 0.4, "explanation": "partial"})).expect("parsed");
    assert_eq!(good.score, 0.4);
}

#[test]
fn the_answer_key_never_reaches_the_subject_request() {
    // ⛔ The structural half of the isolation rule: `SubjectInput` has no `expected` field, so a
    // request built from it cannot carry the key — and this pins that by building the request
    // from an input that *looks* like a case and checking the secret is absent.
    let secret = "ROLL-BACK-THE-MIGRATION-7f3a";
    let plain = subject_request(
        &SubjectInput {
            input: "Why did the deploy fail?",
        },
        &json!({}),
        Some("claude-sonnet-5-5"),
    )
    .expect("built");
    assert_eq!(
        plain,
        json!({"input": "Why did the deploy fail?", "model": "claude-sonnet-5-5"})
    );
    assert!(!plain.to_string().contains(secret));

    let structured = subject_request(
        &SubjectInput {
            input: r#"{"messages":[{"role":"user","content":"why?"}]}"#,
        },
        &json!({"type": "object"}),
        None,
    )
    .expect("built");
    assert!(!structured.to_string().contains(secret));
    assert_eq!(structured["messages"][0]["content"], json!("why?"));
}
