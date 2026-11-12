#![forbid(unsafe_code)]
//! The two model-calling graders and the guard that keeps the judge and the subject apart
//! (S19 Part B steps 11–12; CONTRACT §20).
//!
//! `rubric` and `pairwise` ask a model for `{score, explanation}` with **structured output** —
//! free-text JSON fails on one unescaped quote in the explanation often enough to matter. Both
//! are rendered here and parsed here; the call itself is the driver's, because an `invoke` from
//! inside a handler's transaction would open a second connection behind the same row locks.
//!
//! ⛔ **The judge may not be the subject.** If the model that answers is the model that grades,
//! the number measures its agreement with itself. [`assert_judge_is_not_subject`] refuses that
//! with `422 judge_is_subject` **before** any token is spent, and the refusal names both models
//! so the fix is obvious.
//!
//! The judge model is declared per metric, in `eval_metric.grader.judge_model`: one eval can
//! grade one metric with a cheap model and another with a careful one, and the choice rides the
//! version digest with everything else.

use conveyor_domain::eval::{Grade, GraderKind, GradingError, JudgeVerdict, parse_verdict};
use serde_json::{Value, json};

use crate::invoke::ActionError;

/// The rubric judge's system prompt.
pub const RUBRIC_PROMPT_V1: &str = include_str!("../prompts/eval_judge_rubric_v1.md");

/// The pairwise judge's system prompt.
pub const PAIRWISE_PROMPT_V1: &str = include_str!("../prompts/eval_judge_pairwise_v1.md");

/// How many output tokens a judge may spend. Generous for two sentences, small enough that a
/// runaway judge fails fast rather than burning a run's budget on one case.
const JUDGE_MAX_TOKENS: u32 = 512;

/// The judge model a metric declares, if any.
pub fn judge_model(grader: &Value) -> Option<&str> {
    grader
        .get("judge_model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
}

/// Refuse a judge that is the subject.
///
/// An eval with no `model_ref` (a process or an external subject) cannot be compared this way;
/// the pair is only checked when the subject names a model, because that is the only case where
/// "the same model graded itself" is a fact rather than a guess.
pub fn assert_judge_is_not_subject(
    subject_model: Option<&str>,
    judge_model: Option<&str>,
    action: &str,
) -> Result<(), ActionError> {
    let (Some(subject), Some(judge)) = (subject_model, judge_model) else {
        return Ok(());
    };
    if conveyor_domain::eval::served_model_matches(subject.trim(), judge.trim())
        || conveyor_domain::eval::served_model_matches(judge.trim(), subject.trim())
    {
        return Err(ActionError::new(
            422,
            "judge_is_subject",
            format!(
                "{action}: the judge {judge:?} and the subject {subject:?} are the same model; \
                 a model grading its own answer measures agreement, not quality"
            ),
        ));
    }
    Ok(())
}

/// Build the `completions.create` request for one metric.
///
/// `pairwise` puts the reference answer first (A) and the candidate second (B), and the prompt
/// says so; `rubric` sends the metric's rubric text. Anything else is a caller bug — the two
/// programmatic modes never reach here.
pub fn judge_request(
    kind: GraderKind,
    metric_key: &str,
    rubric: Option<&str>,
    case_input: &str,
    expected: Option<&Value>,
    output: &Value,
    judge_model: &str,
) -> Result<Value, ActionError> {
    let (system, user) = match kind {
        GraderKind::Rubric => {
            let rubric = rubric.map(str::trim).filter(|text| !text.is_empty());
            let Some(rubric) = rubric else {
                return Err(ActionError::new(
                    422,
                    "rubric_missing",
                    format!("the metric {metric_key:?} has no rubric text to grade against"),
                ));
            };
            (
                RUBRIC_PROMPT_V1,
                format!(
                    "## Case input\n\n{case_input}\n\n## Rubric\n\n{rubric}\n\n## Candidate answer\n\n{}",
                    render(output)
                ),
            )
        }
        GraderKind::Pairwise => {
            let Some(reference) = expected else {
                return Err(ActionError::new(
                    422,
                    "answer_key_missing",
                    format!("the metric {metric_key:?} is pairwise and the case has no answer key"),
                ));
            };
            (
                PAIRWISE_PROMPT_V1,
                format!(
                    "## Question\n\n{case_input}\n\n## A (reference)\n\n{}\n\n## B (candidate)\n\n{}",
                    render(reference),
                    render(output)
                ),
            )
        }
        other => {
            return Err(ActionError::new(
                500,
                "wrong_grader",
                format!("{other} does not call a model"),
            ));
        }
    };

    Ok(json!({
        "model": judge_model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_tokens": JUDGE_MAX_TOKENS,
        "temperature": 0,
        "response_format": {"type": "json_object"},
    }))
}

/// The judge's `Grade` from a `completions.create` outcome body.
///
/// Two failures are distinguished and both are `grader` failures rather than zeros: a response
/// with no message content, and content that is not the declared object. The score only reaches
/// the row once it has been validated.
pub fn grade_from_judge(outcome_body: &Value) -> Result<Grade, GradingError> {
    let content = outcome_body
        .get("output")
        .and_then(|output| output.get("choices"))
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or(GradingError::MalformedVerdict {
            why: "the response has no choices[0].message.content",
        })?;
    let parsed: Value =
        serde_json::from_str(content.trim()).map_err(|_| GradingError::MalformedVerdict {
            why: "the message content is not a JSON object",
        })?;
    let JudgeVerdict { score, explanation } = parse_verdict(&parsed)?;
    Ok(Grade {
        score,
        detail: json!({"judge": true, "explanation": explanation}),
    })
}

/// Render an answer for the judge's prompt: a string is itself, anything else its canonical JSON.
fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => conveyor_domain::compare::canonical_json(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_judge_that_is_the_subject_is_refused_before_the_call() {
        let refused =
            assert_judge_is_not_subject(Some("claude-sonnet-5-5"), Some("claude-sonnet-5-5"), "t")
                .expect_err("refused");
        assert_eq!(refused.status, 422);
        assert!(refused.code.contains("judge_is_subject"), "{refused:?}");

        // A build suffix is the same model.
        assert!(
            assert_judge_is_not_subject(
                Some("claude-sonnet-5-5"),
                Some("claude-sonnet-5-5-20260101"),
                "t"
            )
            .is_err()
        );

        // Different models are fine, and an unnamed subject cannot be compared at all.
        assert!(
            assert_judge_is_not_subject(Some("claude-sonnet-5-5"), Some("claude-opus-5-5"), "t")
                .is_ok()
        );
        assert!(assert_judge_is_not_subject(None, Some("claude-opus-5-5"), "t").is_ok());
    }

    #[test]
    fn a_rubric_request_carries_the_rubric_and_the_answer() {
        let request = judge_request(
            GraderKind::Rubric,
            "tone",
            Some("Names the failing component."),
            "why did it fail?",
            None,
            &json!("the cache"),
            "claude-opus-5-5",
        )
        .expect("built");
        assert_eq!(request["model"], json!("claude-opus-5-5"));
        assert_eq!(request["response_format"]["type"], json!("json_object"));
        let user = request["messages"][1]["content"].as_str().expect("content");
        assert!(user.contains("Names the failing component."));
        assert!(user.contains("why did it fail?"));
        assert!(user.contains("the cache"));

        let missing = judge_request(
            GraderKind::Rubric,
            "tone",
            Some("   "),
            "q",
            None,
            &json!("a"),
            "m",
        )
        .expect_err("refused");
        assert_eq!(missing.status, 422);
    }

    #[test]
    fn a_pairwise_request_needs_an_answer_key() {
        let request = judge_request(
            GraderKind::Pairwise,
            "better",
            Some("r"),
            "q",
            Some(&json!("reference")),
            &json!("candidate"),
            "m",
        )
        .expect("built");
        let user = request["messages"][1]["content"].as_str().expect("content");
        assert!(user.contains("A (reference)"));
        assert!(user.contains("B (candidate)"));
        assert!(user.contains("reference"));
        assert!(user.contains("candidate"));

        let missing = judge_request(
            GraderKind::Pairwise,
            "better",
            Some("r"),
            "q",
            None,
            &json!("c"),
            "m",
        )
        .expect_err("refused");
        assert_eq!(missing.status, 422);
    }

    #[test]
    fn a_judge_body_is_parsed_or_refused() {
        let good = json!({"output": {"choices": [{"message": {"content":
            "{\"score\": 0.5, \"explanation\": \"misses the rubric line\"}"}}]}});
        let grade = grade_from_judge(&good).expect("graded");
        assert_eq!(grade.score, 0.5);
        assert_eq!(grade.detail["explanation"], json!("misses the rubric line"));

        for bad in [
            json!({}),
            json!({"output": {"choices": []}}),
            json!({"output": {"choices": [{"message": {"content": "not json"}}]}}),
            json!({"output": {"choices": [{"message": {"content": "{\"score\": 2}"}}]}}),
        ] {
            assert!(
                matches!(
                    grade_from_judge(&bad),
                    Err(GradingError::MalformedVerdict { .. })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_judge_model_is_read_from_the_metrics_grader() {
        assert_eq!(
            judge_model(&json!({"judge_model": " claude-opus-5-5 "})),
            Some("claude-opus-5-5")
        );
        assert_eq!(judge_model(&json!({"judge_model": "  "})), None);
        assert_eq!(judge_model(&json!({})), None);
    }
}
