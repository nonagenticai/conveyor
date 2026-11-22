#![forbid(unsafe_code)]
//! The ladder's laya client: the wire document, the answer parser and the `laya.classify`
//! action (SPEC_V2 §7.2; finding v1-w1c-3).
//!
//! ⛔ **A ladder answer is never a caller's number.** The three-call path is:
//!
//! 1. `ladder.prepare` builds the document below from the effective policy's rows;
//! 2. `laya.classify` runs this external action — the engine owns the timeout, breaker,
//!    idempotency claim and audit row, and the per-tenant bearer comes from `laya.token`;
//! 3. `ladder.evaluate` rebuilds the same document, requires the same request hash to be a
//!    successful replay, and only then writes a `laya` decision.
//!
//! The wire contract is the one the agent platform's `job_design::gates::laya` client and the
//! Jev bridge's own validator (`gitops-production:scripts/host-configs/pgx/laya/bridge.py`)
//! measure: `state` is required and non-empty (a string, object or array), a `choice`
//! question's `criteria` is a MAP of option key → description with 2–255 entries, and a
//! `score` question's `criteria` is an ordered 2–10 level list. Two levels keep the score
//! equal to `P(accepted)` — which is exactly what a policy threshold means. A case with
//! authored options asks a `choice` question over those option keys.

use serde_json::{Map, Value, json};

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, Confirm, Effect, HandlerRef, Schema, Scope, Tenancy,
};

/// The sealed secret that holds a tenant's laya bearer token.
pub const LAYA_TOKEN_SECRET: &str = "laya.token";

/// The service key; the base URL is `CONVEYOR_EXTERNAL_LAYA_URL`.
pub const LAYA_SERVICE: &str = "laya";

/// The one operation the ladder calls.
pub const LAYA_PATH: &str = "/v1/systemone";

/// The action name; it is also the engine's idempotency scope for the call.
pub const LAYA_CLASSIFY: &str = "laya.classify";

/// The single question key the ladder asks: one case is one question.
pub const DECISION_QUESTION: &str = "decision";

/// The measured two-level score rubric (the same scale `process::judge` uses).
///
/// laya answers a `score` question with the expected level index (`Σ index × probability`), so
/// two levels make the answer exactly `P(accepted)` in `[0, 1]`.
pub const SCORE_LEVELS: [&str; 2] = ["rejected", "accepted"];

/// One authored option as the wire carries it.
#[derive(Clone, Copy, Debug)]
pub struct OptionWire<'a> {
    /// The option key the answer must equal.
    pub key: &'a str,
    /// The human label the model reads as the option's description.
    pub label: &'a str,
}

/// Build the laya wire document for one decision case, deterministically.
///
/// The same rows always produce the same document: the `state` carries the question and the
/// caller's facts, the `questions` map carries exactly one question, and a choice question's
/// `criteria` is a map of the authored option keys to their labels. `serde_json`'s object map
/// is ordered, so the document — and the idempotency hash computed over it — is stable.
pub fn request(
    model: &str,
    question: &str,
    facts: Option<&Value>,
    options: &[OptionWire<'_>],
) -> Value {
    let mut state = format!("Decision question: {question}");
    if let Some(facts) = facts {
        state.push_str("\n\nFacts:\n");
        state.push_str(&serde_json::to_string(facts).unwrap_or_else(|_| "null".to_owned()));
    }
    let question_spec = if options.is_empty() {
        json!({
            "type": "score",
            "instructions": "Score the answer to the question in `state` on the ordered scale in \
                 `criteria`."
                .to_owned(),
            "criteria": SCORE_LEVELS,
        })
    } else {
        // The bridge validates a choice's `criteria` as a map of option → description with
        // 2–255 entries (the agent platform's proven shape). The description is the authored
        // label; an empty label falls back to the key, exactly as the agent client does.
        let criteria: Map<String, Value> = options
            .iter()
            .map(|option| {
                let description = if option.label.trim().is_empty() {
                    option.key
                } else {
                    option.label
                };
                (option.key.to_owned(), Value::String(description.to_owned()))
            })
            .collect();
        json!({
            "type": "choice",
            "instructions": "Choose exactly one option key from `criteria` to answer the question in \
                 `state`."
                .to_owned(),
            "criteria": criteria,
        })
    };
    let mut questions = Map::new();
    questions.insert(DECISION_QUESTION.to_owned(), question_spec);
    json!({
        "model": model,
        "state": state,
        "questions": questions,
    })
}

/// The answer a verified laya response carries.
#[derive(Clone, Debug, PartialEq)]
pub struct LayaAnswer {
    /// The option key (choice) or the score rendered as text.
    pub answer: String,
    /// The confidence in 0–1 that the threshold is compared against.
    pub confidence: f64,
    /// The probabilities the answer carries; `{}` when the response provides none.
    pub probabilities: Value,
}

/// Why a laya response cannot be read as this decision's answer.
///
/// Every shape below is an **outage**, never a rejection: an unreadable answer must not be
/// turned into a low confidence that spends a threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayaError {
    /// The document carries no answer the question type recognises.
    #[error("the laya response has no readable answer")]
    Unreadable,
}

/// Read the answer for the one question [`request`] built.
///
/// A `choice` must name one of the authored option keys and carry a confidence; a `score`
/// must be a number in `[0, 1]` and is itself the confidence. A document that reports its own
/// error, a missing answer, a placeholder `null` score or a label outside the authored set is
/// [`LayaError::Unreadable`].
pub fn parse_answer(request: &Value, response: &Value) -> Result<LayaAnswer, LayaError> {
    let answer = response
        .get("answers")
        .and_then(Value::as_object)
        .and_then(|answers| answers.get(DECISION_QUESTION))
        .ok_or(LayaError::Unreadable)?;
    // A failed call's placeholder score must never be read as a real zero.
    if answer
        .get("error")
        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""))
    {
        return Err(LayaError::Unreadable);
    }
    let question = request
        .get("questions")
        .and_then(Value::as_object)
        .and_then(|questions| questions.get(DECISION_QUESTION))
        .ok_or(LayaError::Unreadable)?;
    match question.get("type").and_then(Value::as_str) {
        Some("choice") => {
            let choice = answer
                .get("choice")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|choice| !choice.is_empty())
                .ok_or(LayaError::Unreadable)?;
            let criteria = question
                .get("criteria")
                .and_then(Value::as_object)
                .ok_or(LayaError::Unreadable)?;
            if !criteria.contains_key(choice) {
                return Err(LayaError::Unreadable);
            }
            let confidence = answer
                .get("confidence")
                .and_then(Value::as_f64)
                .ok_or(LayaError::Unreadable)?;
            in_unit_interval(confidence)?;
            Ok(LayaAnswer {
                answer: choice.to_owned(),
                confidence,
                probabilities: probabilities_of(answer)?,
            })
        }
        Some("score") => {
            let score = answer
                .get("score")
                .and_then(Value::as_f64)
                .ok_or(LayaError::Unreadable)?;
            in_unit_interval(score)?;
            Ok(LayaAnswer {
                answer: score.to_string(),
                confidence: score,
                probabilities: probabilities_of(answer)?,
            })
        }
        _ => Err(LayaError::Unreadable),
    }
}

fn in_unit_interval(value: f64) -> Result<(), LayaError> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(LayaError::Unreadable)
    }
}

/// The probabilities the answer carries, each a probability in `[0, 1]`.
///
/// The bridge projects a distribution for both `choice` and `score` answers; a document with
/// no distribution is recorded as `{}`, and a member outside `[0, 1]` makes the whole answer
/// unreadable rather than a partially trusted one.
fn probabilities_of(answer: &Value) -> Result<Value, LayaError> {
    match answer.get("probabilities") {
        None | Some(Value::Null) => Ok(json!({})),
        Some(Value::Object(probabilities)) => {
            for value in probabilities.values() {
                let probability = value.as_f64().ok_or(LayaError::Unreadable)?;
                if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
                    return Err(LayaError::Unreadable);
                }
            }
            Ok(Value::Object(probabilities.clone()))
        }
        Some(_) => Err(LayaError::Unreadable),
    }
}

/// The laya wire body. Deliberately the wire, not a conveyor-shaped DTO: the engine's external
/// invoker sends the input document as the request body, so a translation layer here would be
/// a second place the two shapes can disagree.
fn wire_input() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The laya System-1 request built by ladder.prepare: {model, state, questions}.",
        "required": ["model", "state", "questions"],
        "properties": {
            "model": {"type": "string"},
            // The bridge accepts a string, object or array, but not an empty state.
            "state": {"type": ["string", "object", "array"], "minLength": 1},
            "questions": {"type": "object", "additionalProperties": true}
        }
    })
}

/// Laya's answer document, read by [`parse_answer`].
fn wire_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "Laya's answers document; the answer lives under answers.decision.",
        "additionalProperties": true
    })
}

/// `laya.classify` — the L1 engine call, outside every decision transaction (§7.2).
///
/// It is an **external action**: the engine owns the timeout, idempotency claim and audit row,
/// and the per-tenant bearer comes from the sealed `laya.token` secret. A missing secret is a
/// `503`, which `ladder.evaluate` reports as `gate_unavailable` and never as a pass.
fn laya_classify() -> ActionSpec {
    ActionSpec {
        name: LAYA_CLASSIFY,
        version: 1,
        kind: ActionKind::External,
        input: wire_input,
        output: wire_output,
        scopes: &[Scope::GovernanceWrite],
        idempotent: true,
        // SPEC_V2 §7.2 / §11: 1500 ms is the measured default and 3 s the hard budget. The
        // engine's external invoker has one static timeout, so the hard budget is the spec's;
        // the agent platform's `GATE_BUDGET` is the same 3 s around its client's 1500 ms.
        timeout_ms: 3_000,
        // It changes nothing here; it meters an external decision call.
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::External {
            service: LAYA_SERVICE,
            method: "POST",
            path: LAYA_PATH,
            auth: Some(LAYA_TOKEN_SECRET),
            auth_optional: false,
            upload: None,
        },
    }
}

inventory::submit! { ActionRegistration::new(laya_classify) }

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> [OptionWire<'static>; 2] {
        [
            OptionWire {
                key: "bug",
                label: "A bug",
            },
            OptionWire {
                key: "feature",
                label: "A feature",
            },
        ]
    }

    #[test]
    fn the_request_is_deterministic_and_carries_the_rows() {
        let facts = json!({"card": "card-1", "pin": "knowledge@7"});
        let first = request(
            "jev-latest",
            "What is this request?",
            Some(&facts),
            &options(),
        );
        let second = request(
            "jev-latest",
            "What is this request?",
            Some(&facts),
            &options(),
        );
        assert_eq!(first, second);
        assert_eq!(first["model"], "jev-latest");
        assert_eq!(first["questions"]["decision"]["type"], "choice");
        assert_eq!(
            first["questions"]["decision"]["criteria"],
            json!({"bug": "A bug", "feature": "A feature"}),
            "the bridge reads a choice's criteria as a map of option -> description"
        );
        let state = first["state"].as_str().expect("state is a string");
        assert!(state.contains("What is this request?"), "{state}");
        assert!(state.contains("card-1"), "{state}");
        assert!(state.contains("knowledge@7"), "{state}");
    }

    #[test]
    fn a_scored_case_is_the_measured_two_level_rubric() {
        let request = request("jev-latest", "Is it ready?", None, &[]);
        assert_eq!(request["questions"]["decision"]["type"], "score");
        assert_eq!(
            request["questions"]["decision"]["criteria"],
            json!(SCORE_LEVELS)
        );
        assert_eq!(SCORE_LEVELS.len(), 2, "two levels mean P(accepted)");
    }

    #[test]
    fn a_choice_answer_is_a_key_in_the_authored_set() {
        let request = request("jev-latest", "What is this?", None, &options());
        let answer = json!({"answers": {"decision": {
            "choice": "bug",
            "confidence": 0.93,
            "probabilities": {"bug": 0.93, "feature": 0.07},
        }}});
        let parsed = parse_answer(&request, &answer).expect("a readable choice");
        assert_eq!(parsed.answer, "bug");
        assert_eq!(parsed.confidence, 0.93);
        assert_eq!(parsed.probabilities, json!({"bug": 0.93, "feature": 0.07}));

        // A label outside the authored set is not this decision's answer.
        let outside = json!({"answers": {"decision": {
            "choice": "incident",
            "confidence": 0.99,
        }}});
        assert_eq!(parse_answer(&request, &outside), Err(LayaError::Unreadable));

        // A choice without a confidence cannot be compared to a threshold.
        let no_confidence = json!({"answers": {"decision": {"choice": "bug"}}});
        assert_eq!(
            parse_answer(&request, &no_confidence),
            Err(LayaError::Unreadable)
        );
    }

    #[test]
    fn a_score_is_the_confidence_and_an_out_of_range_score_is_unreadable() {
        let request = request("jev-latest", "Is it ready?", None, &[]);
        let parsed = parse_answer(&request, &json!({"answers": {"decision": {"score": 0.87}}}))
            .expect("a readable score");
        assert_eq!(parsed.confidence, 0.87);
        assert_eq!(parsed.probabilities, json!({}));

        // The bridge projects the level distribution; it is the decision's probabilities.
        let parsed = parse_answer(
            &request,
            &json!({"answers": {"decision": {
                "score": 0.87,
                "probabilities": {"0": 0.13, "1": 0.87},
            }}}),
        )
        .expect("a readable score with its distribution");
        assert_eq!(parsed.probabilities, json!({"0": 0.13, "1": 0.87}));

        for bad in [
            json!({"answers": {"decision": {"score": 1.5}}}),
            json!({"answers": {"decision": {"score": -0.1}}}),
            json!({"answers": {"decision": {"score": null}}}),
            json!({"answers": {"decision": {"score": "failed"}}}),
            json!({"answers": {"decision": {"score": 0.9, "error": "the model timed out"}}}),
            json!({"answers": {"decision": {"score": 0.9, "probabilities": {"0": 1.2}}}}),
            json!({"answers": {}}),
        ] {
            assert_eq!(
                parse_answer(&request, &bad),
                Err(LayaError::Unreadable),
                "{bad}"
            );
        }
    }
}
