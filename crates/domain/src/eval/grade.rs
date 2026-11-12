#![forbid(unsafe_code)]
//! The four grader kinds and the one grade shape they all produce (S19 Part B step 11;
//! CONTRACT §20).
//!
//! - [`GraderKind`] mirrors `eval_metric.grader_kind`'s CHECK — `programmatic | pairwise |
//!   rubric | human | none`. The vocabulary is duplicated in the database on purpose: the CHECK
//!   is what refuses a typo at the other end, and this enum is what refuses it here.
//! - [`Expectation`] is the programmatic grader's mode, read out of `eval_metric.grader.expect`:
//!   `exact | contains | schema | label`.
//! - [`grade_programmatic`] is pure and total: every case either yields a [`Grade`] or a named
//!   [`GradingError`], never a silent zero. **A grader that cannot decide is a `grader`
//!   failure, not a wrong answer** — scoring an unparseable verdict as 0 would inflate every
//!   regression the instrument exists to find (CONTRACT §20, slice trap 6's sibling).
//! - [`parse_verdict`] is the shared shape for the two model-calling graders (`rubric` and
//!   `pairwise`): `{score ∈ [0,1], explanation non-empty}`. Both call the model with structured
//!   output, because free-text JSON fails on one unescaped quote often enough to matter.
//!
//! ## Where the output comes from
//!
//! `eval_metric.grader.field` is a dotted path into the subject's output document
//! (`choices.0.message.content`). **Absent means the whole output.** Nothing here guesses: a
//! subject whose answer is nested and whose metric names no field compares its whole response —
//! visibly, in `detail`, rather than by a prefix rule that would differ per subject kind.
//!
//! ## Normalisation
//!
//! One function, used by `exact`, `contains` and `label`: trim, collapse internal whitespace runs
//! to a single space, case-fold. It is deliberately the whole of it — no unicode folding, no
//! punctuation stripping. A metric that wants more than that is a `rubric` metric, and saying so
//! here is what keeps the report's numbers explainable.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::state::EvalError;

/// The four grader kinds, in the database's CHECK order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraderKind {
    /// No model: [`Expectation`] compared against the answer key.
    Programmatic,
    /// A model compares the reference answer and the candidate (A vs B).
    Pairwise,
    /// A model scores the candidate against the metric's rubric text.
    Rubric,
    /// A person scores it; the result parks at `pending_label` until they do.
    Human,
    /// A `perf` metric is measured, never graded.
    None,
}

impl GraderKind {
    /// The vocabulary in CHECK order.
    pub const ALL: [GraderKind; 5] = [
        GraderKind::Programmatic,
        GraderKind::Pairwise,
        GraderKind::Rubric,
        GraderKind::Human,
        GraderKind::None,
    ];

    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            GraderKind::Programmatic => "programmatic",
            GraderKind::Pairwise => "pairwise",
            GraderKind::Rubric => "rubric",
            GraderKind::Human => "human",
            GraderKind::None => "none",
        }
    }

    /// True when grading needs a model call the driver makes.
    pub fn calls_model(self) -> bool {
        matches!(self, GraderKind::Pairwise | GraderKind::Rubric)
    }

    /// True when grading needs `eval_case.expected`.
    pub fn needs_answer_key(self) -> bool {
        matches!(
            self,
            GraderKind::Programmatic | GraderKind::Pairwise | GraderKind::Rubric
        )
    }

    /// True when the result is parked for a person rather than scored.
    pub fn parks_for_human(self) -> bool {
        matches!(self, GraderKind::Human)
    }
}

impl fmt::Display for GraderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GraderKind {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw.trim())
            .ok_or_else(|| EvalError::Vocabulary {
                what: "grader_kind",
                value: raw.to_owned(),
            })
    }
}

/// The programmatic grader's mode, from `eval_metric.grader.expect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expectation {
    /// The output equals the answer key after [`normalise`].
    Exact,
    /// The answer key appears inside the output after [`normalise`].
    Contains,
    /// The output's JSON *shape* matches `grader.schema` (see [`matches_shape`]).
    Schema,
    /// The output equals one of `grader.labels`, and equals the answer key.
    Label,
}

impl Expectation {
    /// The vocabulary.
    pub const ALL: [Expectation; 4] = [
        Expectation::Exact,
        Expectation::Contains,
        Expectation::Schema,
        Expectation::Label,
    ];

    /// The wire spelling, and the key read out of `eval_metric.grader`.
    pub fn as_str(self) -> &'static str {
        match self {
            Expectation::Exact => "exact",
            Expectation::Contains => "contains",
            Expectation::Schema => "schema",
            Expectation::Label => "label",
        }
    }

    /// Read `grader.expect`, defaulting to [`Expectation::Exact`].
    ///
    /// The default is the strictest comparison, so an author who forgot to name one gets a
    /// number that is too low rather than one that is too high.
    pub fn from_grader(grader: &Value) -> Result<Self, EvalError> {
        match grader.get("expect").and_then(Value::as_str) {
            None => Ok(Expectation::Exact),
            Some(raw) => raw.parse(),
        }
    }
}

impl fmt::Display for Expectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Expectation {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == raw.trim())
            .ok_or_else(|| EvalError::Vocabulary {
                what: "expect",
                value: raw.to_owned(),
            })
    }
}

/// Why a grade could not be produced.
///
/// Every variant is a `grader` attempt failure in the driver's vocabulary — never a zero.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GradingError {
    /// The metric needs an answer key and the case has none.
    #[error("metric {key:?} needs an answer key and case has none")]
    MissingAnswerKey {
        /// The metric key.
        key: String,
    },
    /// The judge returned a document that is not `{score, explanation}`.
    #[error("the judge's verdict is malformed: {why}")]
    MalformedVerdict {
        /// What was wrong, in a sentence a person can act on.
        why: &'static str,
    },
    /// The output has no value at `grader.field`.
    #[error("the subject's output has no value at {path:?}")]
    MissingField {
        /// The dotted path that resolved to nothing.
        path: String,
    },
    /// The output is not the JSON shape `grader.schema` declares.
    #[error("the subject's output does not match the declared shape: {why}")]
    ShapeMismatch {
        /// Which declared property failed.
        why: String,
    },
}

/// One metric's score and the evidence behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grade {
    /// `0.0..=1.0`.
    pub score: f64,
    /// Why that score: mode, matched value, and what was compared.
    pub detail: Value,
}

/// The one normalisation rule: trim, collapse whitespace runs, case-fold.
pub fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Render a JSON value as the text `exact`/`contains` compare.
///
/// A string is itself; anything else is its canonical JSON text, so an object answer key and an
/// object output compare structurally rather than by key order.
pub fn as_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => crate::compare::canonical_json(other),
    }
}

/// Resolve `grader.field` against an output document.
///
/// The path is dotted, and a segment that parses as an index walks arrays. `None` means the
/// field is absent — which is a [`GradingError::MissingField`], never an empty score.
pub fn output_field<'a>(output: &'a Value, field: Option<&str>) -> Option<&'a Value> {
    let Some(path) = field.map(str::trim).filter(|path| !path.is_empty()) else {
        return Some(output);
    };
    let mut cursor = output;
    for segment in path.split('.') {
        cursor = match cursor {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cursor)
}

/// True when `output` matches the declared shape: `type` and `required` only.
///
/// ⛔ This is **not** a JSON-Schema implementation and must not be described as one. It checks
/// the two properties that catch a subject returning prose where an object was promised — the
/// `type` at the root and the presence of each `required` key — and ignores everything else.
pub fn matches_shape(output: &Value, schema: &Value) -> Result<(), GradingError> {
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        let actual = match output {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        let matches = match kind {
            "integer" => output.as_i64().is_some() || output.as_u64().is_some(),
            "number" => output.is_number(),
            other => other == actual,
        };
        if !matches {
            return Err(GradingError::ShapeMismatch {
                why: format!("expected type {kind}, found {actual}"),
            });
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let Some(map) = output.as_object() else {
            return Err(GradingError::ShapeMismatch {
                why: "required keys declared but the output is not an object".to_owned(),
            });
        };
        for key in required.iter().filter_map(Value::as_str) {
            if !map.contains_key(key) {
                return Err(GradingError::ShapeMismatch {
                    why: format!("required key {key:?} is missing"),
                });
            }
        }
    }
    Ok(())
}

/// Grade one case programmatically.
///
/// Returns `1.0` or `0.0` — the programmatic modes are binary by construction, and a partial
/// credit scheme would be a rubric metric wearing the wrong `grader_kind`.
pub fn grade_programmatic(
    key: &str,
    grader: &Value,
    expected: Option<&Value>,
    output: &Value,
) -> Result<Grade, GradingError> {
    let expect = Expectation::from_grader(grader).map_err(|_| GradingError::MalformedVerdict {
        why: "the metric's `expect` is not one of exact|contains|schema|label",
    })?;
    let field = grader.get("field").and_then(Value::as_str);
    let at = output_field(output, field).ok_or_else(|| GradingError::MissingField {
        path: field.unwrap_or_default().to_owned(),
    })?;

    if expect == Expectation::Schema {
        let declared = grader.get("schema").ok_or(GradingError::MalformedVerdict {
            why: "a `schema` metric must declare `grader.schema`",
        })?;
        matches_shape(at, declared)?;
        return Ok(Grade {
            score: 1.0,
            detail: serde_json::json!({ "expect": expect.as_str(), "matched": true }),
        });
    }

    let expected = expected.ok_or_else(|| GradingError::MissingAnswerKey {
        key: key.to_owned(),
    })?;
    let wanted = normalise(&as_text(expected));
    let found = normalise(&as_text(at));

    let matched = match expect {
        Expectation::Exact => found == wanted,
        Expectation::Contains => found.contains(&wanted),
        // ⛔ An answer outside the metric's vocabulary is **wrong**, not ungradeable: a label
        // metric declares the answer space, and anything else in it is a 0. Returning an error
        // here would make the audit's null control (an empty answer, ≈0 %) impossible to run.
        Expectation::Label => found == wanted,
        Expectation::Schema => unreachable!("handled before the answer key is read"),
    };

    let known = match (expect, grader.get("labels").and_then(Value::as_array)) {
        (Expectation::Label, Some(labels)) => labels
            .iter()
            .any(|label| normalise(&as_text(label)) == found),
        _ => true,
    };

    Ok(Grade {
        score: if matched { 1.0 } else { 0.0 },
        detail: serde_json::json!({
            "expect": expect.as_str(),
            "matched": matched,
            "known": known,
            "expected": wanted,
            "found": found,
        }),
    })
}

/// A judge's verdict, after [`parse_verdict`] has validated it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgeVerdict {
    /// `0.0..=1.0`.
    pub score: f64,
    /// Why, non-empty.
    pub explanation: String,
}

/// Validate a judge's structured-output verdict.
///
/// The two model-calling graders ask for `{"score": <number>, "explanation": <string>}`. Anything
/// else — a missing key, a score outside `[0,1]`, an empty explanation — is
/// [`GradingError::MalformedVerdict`]: the call completed, so this is the grader's failure to
/// classify, not an answer of zero.
pub fn parse_verdict(output: &Value) -> Result<JudgeVerdict, GradingError> {
    let score =
        output
            .get("score")
            .and_then(Value::as_f64)
            .ok_or(GradingError::MalformedVerdict {
                why: "`score` is missing or is not a number",
            })?;
    if !(0.0..=1.0).contains(&score) {
        return Err(GradingError::MalformedVerdict {
            why: "`score` is outside 0.0..=1.0",
        });
    }
    let explanation = output
        .get("explanation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or(GradingError::MalformedVerdict {
            why: "`explanation` is missing or empty",
        })?;
    Ok(JudgeVerdict {
        score,
        explanation: explanation.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer(value: Value) -> Value {
        value
    }

    #[test]
    fn every_grader_kind_round_trips() {
        for kind in GraderKind::ALL {
            assert_eq!(kind.as_str().parse::<GraderKind>(), Ok(kind));
        }
        assert!(matches!(
            "bogus".parse::<GraderKind>(),
            Err(EvalError::Vocabulary { .. })
        ));
    }

    #[test]
    fn model_callers_are_exactly_rubric_and_pairwise() {
        for kind in GraderKind::ALL {
            let expected = matches!(kind, GraderKind::Pairwise | GraderKind::Rubric);
            assert_eq!(kind.calls_model(), expected, "{kind}");
        }
        assert!(GraderKind::Human.parks_for_human());
        assert!(!GraderKind::None.needs_answer_key());
    }

    #[test]
    fn exact_ignores_case_and_whitespace_runs() {
        let grader = json!({"expect": "exact"});
        let grade = grade_programmatic(
            "answer",
            &grader,
            Some(&answer(json!("  42  "))),
            &json!("42"),
        )
        .expect("graded");
        assert_eq!(grade.score, 1.0);
        assert_eq!(grade.detail["matched"], json!(true));
    }

    #[test]
    fn exact_is_still_exact() {
        let grade = grade_programmatic(
            "answer",
            &json!({}),
            Some(&answer(json!("42"))),
            &json!("43"),
        )
        .expect("graded");
        assert_eq!(grade.score, 0.0);
    }

    #[test]
    fn contains_finds_the_key_inside_a_sentence() {
        let grade = grade_programmatic(
            "cause",
            &json!({"expect": "contains"}),
            Some(&answer(json!("cache invalidation"))),
            &json!("The root cause is a cache invalidation bug."),
        )
        .expect("graded");
        assert_eq!(grade.score, 1.0);
    }

    #[test]
    fn schema_checks_type_and_required_keys_only() {
        let grader = json!({"expect": "schema", "schema": {"type": "object", "required": ["ok"]}});
        let good = grade_programmatic("shape", &grader, None, &json!({"ok": true, "extra": 1}))
            .expect("graded");
        assert_eq!(good.score, 1.0);

        let bad =
            grade_programmatic("shape", &grader, None, &json!({"nope": 1})).expect_err("refused");
        assert!(matches!(bad, GradingError::ShapeMismatch { .. }));

        let wrong_type =
            grade_programmatic("shape", &grader, None, &json!("prose")).expect_err("refused");
        assert_eq!(
            wrong_type,
            GradingError::ShapeMismatch {
                why: "expected type object, found string".to_owned()
            }
        );
    }

    #[test]
    fn label_scores_a_value_outside_its_vocabulary_as_wrong() {
        let grader = json!({"expect": "label", "labels": ["yes", "no"]});
        let grade = grade_programmatic(
            "verdict",
            &grader,
            Some(&answer(json!("yes"))),
            &json!(" YES "),
        )
        .expect("graded");
        assert_eq!(grade.score, 1.0);
        assert_eq!(grade.detail["known"], json!(true));

        // Outside the vocabulary is a 0, not an error: the null control must be gradeable.
        let outside = grade_programmatic(
            "verdict",
            &grader,
            Some(&answer(json!("yes"))),
            &json!("maybe"),
        )
        .expect("graded");
        assert_eq!(outside.score, 0.0);
        assert_eq!(outside.detail["known"], json!(false));

        let empty = grade_programmatic("verdict", &grader, Some(&answer(json!("yes"))), &json!(""))
            .expect("graded");
        assert_eq!(empty.score, 0.0, "the null control scores zero");
    }

    #[test]
    fn a_missing_answer_key_is_an_error_not_a_zero() {
        let refused = grade_programmatic("answer", &json!({}), None, &json!("anything"))
            .expect_err("refused");
        assert_eq!(
            refused,
            GradingError::MissingAnswerKey {
                key: "answer".to_owned()
            }
        );
    }

    #[test]
    fn a_field_path_walks_objects_and_arrays() {
        let output = json!({"choices": [{"message": {"content": "hello"}}]});
        let grade = grade_programmatic(
            "answer",
            &json!({"expect": "exact", "field": "choices.0.message.content"}),
            Some(&answer(json!("hello"))),
            &output,
        )
        .expect("graded");
        assert_eq!(grade.score, 1.0);

        let missing = grade_programmatic(
            "answer",
            &json!({"expect": "exact", "field": "choices.9.message.content"}),
            Some(&answer(json!("hello"))),
            &output,
        )
        .expect_err("refused");
        assert!(matches!(missing, GradingError::MissingField { .. }));
    }

    #[test]
    fn an_oracle_answers_every_case_and_a_null_answers_none() {
        let grader = json!({"expect": "exact"});
        let keys = ["alpha", "beta", "gamma"];
        for word in keys {
            let oracle =
                grade_programmatic("answer", &grader, Some(&answer(json!(word))), &json!(word))
                    .expect("graded");
            assert_eq!(oracle.score, 1.0, "oracle on {word}");
            let null =
                grade_programmatic("answer", &grader, Some(&answer(json!(word))), &json!(""))
                    .expect("graded");
            assert_eq!(null.score, 0.0, "null on {word}");
        }
    }

    #[test]
    fn a_verdict_must_be_a_number_in_range_with_a_reason() {
        let good =
            parse_verdict(&json!({"score": 0.75, "explanation": "mostly right"})).expect("parsed");
        assert_eq!(good.score, 0.75);

        for bad in [
            json!({"explanation": "no score"}),
            json!({"score": 1.5, "explanation": "too high"}),
            json!({"score": -0.1, "explanation": "too low"}),
            json!({"score": 0.5, "explanation": "   "}),
            json!({"score": "0.5", "explanation": "a string"}),
        ] {
            assert!(
                matches!(
                    parse_verdict(&bad),
                    Err(GradingError::MalformedVerdict { .. })
                ),
                "{bad}"
            );
        }
    }
}
