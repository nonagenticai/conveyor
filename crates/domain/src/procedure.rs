#![forbid(unsafe_code)]
//! The **procedure (SOP)** a process step is judged against (CONTRACT §21, SPEC D28).
//!
//! A procedure is versioned and reusable: it carries the definition of **ready** (inputs and
//! preconditions), the definition of **done** (outputs and checks), the **acceptance
//! criteria** and the **inspection** — the judge. It is not the process: the process is the
//! ordered sequence of steps (see [`crate::process`]), and a step names the SOP it is judged
//! against by `procedure_id`.
//!
//! This module is pure: the publish validator takes the facts it needs and never reads a
//! database. The two rules that matter are enforced here rather than in a handler:
//!
//! - **Every SOP names its judge.** An empty `inspection` is refused — a step whose output is
//!   never inspected is the silent-accept this model exists to remove.
//! - **A rejected output reruns, bounded.** `max_rejects` is capped; an exhausted budget is a
//!   block, never an infinite loop (S13 §8 trap "a judge reject is not a retry storm").

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The default number of judge rejects before the step blocks and the coach is started.
pub const DEFAULT_MAX_REJECTS: u32 = 2;

/// The application cap on `inspection.max_rejects`; the column stores it, this refuses it.
pub const MAX_REJECTS_CAP: u32 = 10;

/// Which judge inspects the step's output (D28).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionKind {
    /// The estate's TypeSafe AI decision service, through `laya.judge`.
    Laya,
    /// An S19 eval, graded over one output by `eval_grades.create` (the per-output path).
    Eval,
    /// A deterministic rule with no model call.
    Checker,
}

impl InspectionKind {
    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Laya => "laya",
            Self::Eval => "eval",
            Self::Checker => "checker",
        }
    }

    /// Parse the stored spelling; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "laya" => Some(Self::Laya),
            "eval" => Some(Self::Eval),
            "checker" => Some(Self::Checker),
            _ => None,
        }
    }
}

/// The deterministic rules a `checker` inspection may name. Deliberately a closed set: a
/// checker that can run arbitrary code is a second engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckerRule {
    /// Accept any output. Used by a gate or a call step, whose "output" is the decision itself.
    Always,
    /// Accept a non-empty output document (not `{}`, `[]`, `null`, `""` or absent).
    NonEmpty,
}

impl CheckerRule {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::NonEmpty => "non_empty",
        }
    }

    /// Parse the stored spelling; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "always" => Some(Self::Always),
            "non_empty" => Some(Self::NonEmpty),
            _ => None,
        }
    }

    /// The rule's verdict on one output.
    pub fn accepts(self, output: &Value) -> bool {
        match self {
            Self::Always => true,
            Self::NonEmpty => match output {
                Value::Null => false,
                Value::Bool(_) | Value::Number(_) => true,
                Value::String(text) => !text.trim().is_empty(),
                Value::Array(items) => !items.is_empty(),
                Value::Object(map) => !map.is_empty(),
            },
        }
    }
}

/// The typed `inspection` document (`procedure.inspection`).
///
/// Un-tagged on the way in so an unknown `kind`, a missing `eval_id` or a missing rule is a
/// publish-time refusal naming the field — never a runtime surprise after the process is live.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Inspection {
    /// Laya decides; `threshold` is carried to the request as the acceptance bar.
    Laya {
        /// The score at or above which the output is accepted, when laya exposes one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        threshold: Option<f64>,
        /// Rejects allowed before the step blocks (default [`DEFAULT_MAX_REJECTS`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_rejects: Option<u32>,
    },
    /// An S19 eval grades this one output through `eval_grades.create`.
    Eval {
        /// The eval whose metrics grade the output.
        eval_id: Uuid,
        /// Every `quality` metric must reach this score.
        threshold: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_rejects: Option<u32>,
    },
    /// A deterministic rule.
    Checker {
        /// The rule.
        rule: CheckerRule,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_rejects: Option<u32>,
    },
}

impl Inspection {
    /// Which judge.
    pub fn kind(&self) -> InspectionKind {
        match self {
            Self::Laya { .. } => InspectionKind::Laya,
            Self::Eval { .. } => InspectionKind::Eval,
            Self::Checker { .. } => InspectionKind::Checker,
        }
    }

    /// Rejects allowed before the step blocks.
    pub fn max_rejects(&self) -> u32 {
        match self {
            Self::Laya { max_rejects, .. }
            | Self::Eval { max_rejects, .. }
            | Self::Checker { max_rejects, .. } => max_rejects.unwrap_or(DEFAULT_MAX_REJECTS),
        }
    }

    /// The acceptance threshold, when the judge has one.
    pub fn threshold(&self) -> Option<f64> {
        match self {
            Self::Laya { threshold, .. } => *threshold,
            Self::Eval { threshold, .. } => Some(*threshold),
            Self::Checker { .. } => None,
        }
    }
}

/// Why an SOP fails to publish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SopError {
    /// The inspection document is not one of the three kinds, or names a bad field.
    BadInspection(String),
    /// A laya-judged SOP has no acceptance criteria to judge against.
    NoAcceptanceCriteria,
    /// An eval-judged SOP names a threshold outside `[0, 1]`.
    BadThreshold,
    /// `max_rejects` is outside `1..=MAX_REJECTS_CAP`.
    BadMaxRejects,
    /// An output is not a non-empty dot path.
    BadOutput(String),
    /// The acceptance criteria are not an array.
    BadCriteria,
}

impl std::fmt::Display for SopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadInspection(detail) => write!(f, "inspection: {detail}"),
            Self::NoAcceptanceCriteria => {
                write!(
                    f,
                    "acceptance_criteria: a laya-judged SOP needs at least one criterion"
                )
            }
            Self::BadThreshold => write!(f, "inspection.threshold must be between 0 and 1"),
            Self::BadMaxRejects => write!(
                f,
                "inspection.max_rejects must be between 1 and {MAX_REJECTS_CAP}"
            ),
            Self::BadOutput(output) => {
                write!(f, "outputs: {output:?} is not a non-empty dot path")
            }
            Self::BadCriteria => write!(f, "acceptance_criteria must be a JSON array"),
        }
    }
}

/// Parse and validate one `inspection` document.
pub fn validate_inspection(raw: &Value) -> Result<Inspection, SopError> {
    let inspection: Inspection = serde_json::from_value(raw.clone())
        .map_err(|error| SopError::BadInspection(error.to_string()))?;
    let max = inspection.max_rejects();
    if max == 0 || max > MAX_REJECTS_CAP {
        return Err(SopError::BadMaxRejects);
    }
    if let Some(threshold) = inspection.threshold()
        && !(0.0..=1.0).contains(&threshold)
    {
        return Err(SopError::BadThreshold);
    }
    Ok(inspection)
}

/// The publish-time rules of an SOP (CONTRACT §21).
///
/// `acceptance_criteria` must be a non-empty array for every kind: laya is asked to judge
/// against them, and an empty list asks it to judge nothing; a checker or eval SOP still states
/// what "done" means to the person reading the process. `max_rejects` is capped, and an `eval`
/// threshold must be a probability.
pub fn validate_publish(
    acceptance_criteria: &Value,
    inspection: &Value,
    outputs: &Value,
) -> Result<Inspection, Vec<SopError>> {
    let mut errors = Vec::new();

    match acceptance_criteria.as_array() {
        Some(items) if items.is_empty() => errors.push(SopError::NoAcceptanceCriteria),
        Some(_) => {}
        None => errors.push(SopError::BadCriteria),
    }

    let parsed = validate_inspection(inspection);
    if let Err(error) = &parsed {
        errors.push(error.clone());
    }

    match outputs.as_array() {
        Some(items) => {
            for item in items {
                let ok = item.as_str().is_some_and(|path| {
                    !path.trim().is_empty() && !path.contains(char::is_whitespace)
                });
                if !ok {
                    errors.push(SopError::BadOutput(item.to_string()));
                }
            }
        }
        None => errors.push(SopError::BadOutput(outputs.to_string())),
    }

    match parsed {
        Ok(inspection) if errors.is_empty() => Ok(inspection),
        _ => Err(errors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_inspection_is_refused() {
        let error = validate_inspection(&json!({})).expect_err("no judge named");
        assert!(matches!(error, SopError::BadInspection(_)));
    }

    #[test]
    fn a_checker_rule_is_closed() {
        let parser = validate_inspection(&json!({"kind": "checker", "rule": "run_shell"}));
        assert!(parser.is_err(), "an unknown checker rule must not parse");
    }

    #[test]
    fn the_checker_rules_accept_and_refuse() {
        assert!(CheckerRule::Always.accepts(&json!({})));
        assert!(!CheckerRule::NonEmpty.accepts(&json!({})));
        assert!(!CheckerRule::NonEmpty.accepts(&json!([])));
        assert!(!CheckerRule::NonEmpty.accepts(&json!("")));
        assert!(CheckerRule::NonEmpty.accepts(&json!({"ok": true})));
        assert!(CheckerRule::NonEmpty.accepts(&json!(["x"])));
    }

    #[test]
    fn a_laya_sop_needs_criteria() {
        let errors = validate_publish(&json!([]), &json!({"kind": "laya"}), &json!([]))
            .expect_err("empty criteria");
        assert!(errors.contains(&SopError::NoAcceptanceCriteria));
    }

    #[test]
    fn an_eval_sop_needs_a_threshold_in_range() {
        let eval_id = Uuid::nil();
        let errors = validate_publish(
            &json!(["done"]),
            &json!({"kind": "eval", "eval_id": eval_id, "threshold": 1.5}),
            &json!([]),
        )
        .expect_err("threshold out of range");
        assert!(errors.contains(&SopError::BadThreshold));
        let ok = validate_publish(
            &json!(["done"]),
            &json!({"kind": "eval", "eval_id": eval_id, "threshold": 0.8}),
            &json!(["result.summary"]),
        );
        assert!(ok.is_ok(), "a valid eval inspection is accepted");
    }

    #[test]
    fn the_reject_budget_is_capped() {
        let errors = validate_publish(
            &json!(["done"]),
            &json!({"kind": "checker", "rule": "always", "max_rejects": 99}),
            &json!([]),
        )
        .expect_err("over the cap");
        assert!(errors.contains(&SopError::BadMaxRejects));
        let error = validate_publish(
            &json!(["done"]),
            &json!({"kind": "checker", "rule": "always", "max_rejects": 0}),
            &json!([]),
        )
        .expect_err("zero rejects is not a budget");
        assert!(error.contains(&SopError::BadMaxRejects));
    }

    #[test]
    fn outputs_must_be_dot_paths() {
        let errors = validate_publish(
            &json!(["done"]),
            &json!({"kind": "checker", "rule": "always"}),
            &json!(["result summary"]),
        )
        .expect_err("a space is not a dot path");
        assert!(errors.iter().any(|e| matches!(e, SopError::BadOutput(_))));
    }

    #[test]
    fn the_default_budget_is_two() {
        let inspection =
            validate_inspection(&json!({"kind": "checker", "rule": "always"})).expect("valid");
        assert_eq!(inspection.max_rejects(), DEFAULT_MAX_REJECTS);
        assert_eq!(inspection.kind(), InspectionKind::Checker);
    }
}
