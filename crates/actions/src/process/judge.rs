#![forbid(unsafe_code)]
//! The judge: laya, an S19 eval, or a deterministic checker (SPEC D28, CONTRACT §21).
//!
//! This module owns the **pure** half — the laya wire document, the verdict parser and the
//! checker rules. The executor owns the asynchronous half (the action invocation, the eval
//! grade poll and the reject/rerun ladder), because a judge call must never hold a database
//! transaction and an outage must park the step rather than decide it.
//!
//! ⛔ **Only a valid response can reject.** A timeout, a 5xx, a missing `laya.token`, an
//! unreadable body or a grader failure is an **outage**: the step waits and is retried, and the
//! reject budget is untouched. Treating an outage as a reject would block a card on a network
//! blip.

use serde_json::{Map, Value, json};
use std::borrow::Cow;

use conveyor_domain::procedure::{CheckerRule, Inspection};

/// The sealed secret that holds a tenant's laya bearer token.
pub const LAYA_TOKEN_SECRET: &str = "laya.token";

/// The service key; the base URL is `CONVEYOR_EXTERNAL_LAYA_URL`.
pub const LAYA_SERVICE: &str = "laya";

/// The operation every judge call uses.
pub const LAYA_PATH: &str = "/v1/systemone";

/// The conveyor→laya seam's measured default (SPEC_V2 §10.2).
pub const LAYA_DEFAULT_TIMEOUT_MS: i32 = 1_500;

/// The seam's hard budget (SPEC_V2 §10.2): no conveyor→laya call outlives 3 s.
pub const LAYA_HARD_TIMEOUT_MS: i32 = 3_000;

/// The default model the estate's laya gateway serves.
pub const DEFAULT_LAYA_MODEL: &str = "system-1";

/// The question key the judge asks and reads.
pub const JUDGE_QUESTION: &str = "accept";

/// The threshold when the SOP names none: laya's confidence must beat a coin flip.
pub const DEFAULT_THRESHOLD: f64 = 0.5;

/// A judge's verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    /// Whether the output is accepted.
    pub accepted: bool,
    /// The judge's score, when it returned one.
    pub score: Option<f64>,
    /// Why — laya's own text, or the checker's rule.
    pub reason: String,
    /// The raw answer document, recorded on `run_step.judge`.
    pub raw: Value,
}

/// Why a judge could not decide.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JudgeError {
    /// The response carried no verdict the parser recognises.
    #[error("the judge's response has no readable verdict")]
    Unreadable,
}

/// The ordered score levels the judge asks laya for.
///
/// laya answers a `score` question with the EXPECTED LEVEL INDEX (`Σ index × probability`), so
/// two levels make the answer exactly `P(accepted)` in `[0, 1]` — which is what every SOP's
/// threshold already means. A three-level rubric would score 0..2 and silently loosen every
/// threshold by comparison.
pub const SCORE_LEVELS: [&str; 2] = ["rejected", "accepted"];

/// Build the laya request for one step output.
///
/// The bridge's wire contract (measured 2026-11-16, `/opt/laya-bridge/bridge.py` on pgx):
/// `state` is required and non-empty — the task text — and a `score` question's `criteria` is an
/// ORDERED list of 2–10 level labels, not the SOP's acceptance-criteria objects (those are the
/// instructions to the judge, not the scale). Sending `state: ""` and the criteria objects was
/// a live `422`: `state: field required`, `criteria: must have between 2 and 10 levels`.
pub fn request(
    model: Option<&str>,
    output: &Value,
    acceptance_criteria: &Value,
    case_input: Option<&Value>,
) -> Value {
    let mut state = format!(
        "Step output:\n{}",
        serde_json::to_string(output).unwrap_or_else(|_| "null".to_owned())
    );
    if let Some(input) = case_input {
        state.push_str(&format!(
            "\n\nStep input / card facts:\n{}",
            serde_json::to_string(input).unwrap_or_else(|_| "null".to_owned())
        ));
    }
    let instructions = format!(
        "Judge the step output in `state` against these acceptance criteria.\n\nAcceptance criteria:\n{}",
        serde_json::to_string(acceptance_criteria).unwrap_or_else(|_| "null".to_owned())
    );
    let mut questions = Map::new();
    questions.insert(
        JUDGE_QUESTION.to_owned(),
        json!({
            "type": "score",
            "instructions": instructions,
            "criteria": SCORE_LEVELS,
        }),
    );
    json!({
        "model": model.unwrap_or(DEFAULT_LAYA_MODEL),
        "state": state,
        "questions": questions,
    })
}

/// Build a `choice` question over an authored menu (S21 step 10, issue #656).
///
/// The wire shape is [`request`]'s; only the question type changes. `options` are the authored
/// `(key, label)` pairs: the **criteria are the keys** — an answer names a key — and the labelled
/// menu lives in the instructions, so a human reads `key — label` while the parser reads a stable
/// key. An empty menu is a caller error, not a free-text question.
pub fn request_choice(
    model: Option<&str>,
    output: &Value,
    acceptance_criteria: &Value,
    case_input: Option<&Value>,
    options: &[(String, String)],
) -> Value {
    let mut state = format!(
        "Step output:\n{}",
        serde_json::to_string(output).unwrap_or_else(|_| "null".to_owned())
    );
    if let Some(input) = case_input {
        state.push_str(&format!(
            "\n\nStep input / card facts:\n{}",
            serde_json::to_string(input).unwrap_or_else(|_| "null".to_owned())
        ));
    }
    let menu = options
        .iter()
        .map(|(key, label)| format!("- {key} — {label}"))
        .collect::<Vec<_>>()
        .join("\n");
    let instructions = format!(
        "Choose one option for the step output in `state` against these acceptance criteria.\n\n\
         Acceptance criteria:\n{}\n\nOptions:\n{menu}",
        serde_json::to_string(acceptance_criteria).unwrap_or_else(|_| "null".to_owned())
    );
    let keys = options
        .iter()
        .map(|(key, _)| json!(key))
        .collect::<Vec<_>>();
    let mut questions = Map::new();
    questions.insert(
        JUDGE_QUESTION.to_owned(),
        json!({
            "type": "choice",
            "instructions": instructions,
            "criteria": keys,
        }),
    );
    json!({
        "model": model.unwrap_or(DEFAULT_LAYA_MODEL),
        "state": state,
        "questions": questions,
    })
}

/// Read the authored key laya chose.
///
/// Unlike [`parse_verdict`], a key is not an accept/reject word: any non-empty string is a
/// candidate, and the caller checks it against the authored set. A document that reports its own
/// failure, or carries no key, is [`JudgeError::Unreadable`] — an outage, never a choice.
pub fn parse_choice(response: &Value) -> Result<(String, Option<f64>), JudgeError> {
    let answer = response
        .get("answers")
        .and_then(|answers| answers.get(JUDGE_QUESTION))
        .ok_or(JudgeError::Unreadable)?;
    if answer
        .get("error")
        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""))
    {
        return Err(JudgeError::Unreadable);
    }
    let choice = answer
        .get("choice")
        .or_else(|| answer.get("answer"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or(JudgeError::Unreadable)?
        .to_owned();
    let confidence = answer.get("confidence").and_then(Value::as_f64);
    Ok((choice, confidence))
}

/// Parse laya's answer document into a verdict.
///
/// Two answer shapes are accepted, because the gateway answers a `score` question with either
/// a number or a labelled choice: `{score}` and `{choice, confidence}`. Only a verdict is a
/// verdict: an explicit reject label, or a numeric score below the threshold. Everything else —
/// a missing answer, a `score` that is not a number (a failed call leaves `null` there), a label
/// such as `error`/`abstain`/`unknown`, or a failure the document reports itself — is
/// [`JudgeError::Unreadable`], which the caller treats as an outage and never as a rejection.
pub fn parse_verdict(response: &Value, threshold: Option<f64>) -> Result<Verdict, JudgeError> {
    let threshold = threshold.unwrap_or(DEFAULT_THRESHOLD);
    let answer = response
        .get("answers")
        .and_then(|answers| answers.get(JUDGE_QUESTION))
        .ok_or(JudgeError::Unreadable)?;
    // A document that reports its own failure is an outage whatever else it carries. A failed
    // call's placeholder score must never be read as a real zero and spend a reject.
    if answer
        .get("error")
        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""))
    {
        return Err(JudgeError::Unreadable);
    }
    let reason = answer
        .get("reason")
        .or_else(|| answer.get("explanation"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    // A label is a verdict only when it names one of the two outcomes. `error`, `abstain`,
    // `unknown` and anything else the gateway invents is the judge declining to decide — an
    // outage, never a `false` that the reject ladder would act on.
    let choice = match answer.get("choice") {
        None => None,
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "accept" | "accepted" | "yes" | "true" | "pass" => Some(true),
            "reject" | "rejected" | "no" | "false" | "fail" => Some(false),
            _ => return Err(JudgeError::Unreadable),
        },
        Some(_) => return Err(JudgeError::Unreadable),
    };
    if let Some(raw) = answer.get("score") {
        // A `score` that is not a number is not a zero.
        let score = raw.as_f64().ok_or(JudgeError::Unreadable)?;
        return Ok(Verdict {
            accepted: score >= threshold,
            score: Some(score),
            reason,
            raw: answer.clone(),
        });
    }
    if let Some(accepted_choice) = choice {
        let confidence = answer
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(threshold);
        return Ok(Verdict {
            accepted: accepted_choice && confidence >= threshold,
            score: Some(confidence),
            reason,
            raw: answer.clone(),
        });
    }
    Err(JudgeError::Unreadable)
}

/// The verdict of a deterministic checker.
pub fn checker_verdict(rule: CheckerRule, output: &Value) -> Verdict {
    let accepted = rule.accepts(output);
    Verdict {
        accepted,
        score: if accepted { Some(1.0) } else { Some(0.0) },
        reason: format!("checker `{}`", rule.as_str()),
        raw: json!({"rule": rule.as_str(), "accepted": accepted}),
    }
}

/// Which judge an inspection names, and what the executor needs to call it.
#[derive(Clone, Debug, PartialEq)]
pub enum JudgeKind {
    /// A laya call.
    Laya {
        /// The score bar.
        threshold: Option<f64>,
    },
    /// One S19 output grade.
    Eval {
        /// The eval the grade is created against.
        eval_id: uuid::Uuid,
        /// The score bar.
        threshold: f64,
    },
    /// A deterministic rule.
    Checker {
        /// The rule.
        rule: CheckerRule,
    },
}

/// Read the typed judge out of an SOP's `inspection`.
///
/// A published SOP has already passed `conveyor_domain::procedure::validate_publish`; this is
/// the runtime read, and a document that no longer parses is an **outage-shaped** error (the
/// step waits) rather than a rejection.
pub fn judge_of(inspection: &Inspection) -> JudgeKind {
    match inspection {
        Inspection::Laya { threshold, .. } => JudgeKind::Laya {
            threshold: *threshold,
        },
        Inspection::Eval {
            eval_id, threshold, ..
        } => JudgeKind::Eval {
            eval_id: *eval_id,
            threshold: *threshold,
        },
        Inspection::Checker { rule, .. } => JudgeKind::Checker { rule: *rule },
    }
}

// ── the action ────────────────────────────────────────────────────────────────────────────

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, Confirm, Effect, HandlerRef, Schema, SchemaRef, Scope,
    Tenancy,
};

/// The laya wire body. Deliberately the wire, not a conveyor-shaped DTO: the engine's external
/// invoker sends the input document as the request body, so a translation layer here would be a
/// second place the two shapes can disagree.
fn wire_input() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The laya System-1 request: {model, state, questions}.",
        "required": ["model", "state", "questions"],
        "properties": {
            "model": {"type": "string"},
            "state": {"type": "string"},
            "questions": {"type": "object", "additionalProperties": true}
        }
    })
}

/// Laya's answer document, read by [`parse_verdict`].
fn wire_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "Laya's answers document; the verdict lives under answers.accept.",
        "additionalProperties": true
    })
}

/// `laya.judge` — the estate's TypeSafe AI decision service (SPEC D28).
///
/// It is an **external action**: the engine owns the timeout, the idempotency claim and the audit
/// row, and the per-tenant bearer comes from the sealed `laya.token` secret named here. A missing
/// secret is a `503`, which the step executor treats as an outage (wait), never as a rejection.
fn laya_judge() -> ActionSpec {
    ActionSpec {
        name: Cow::Borrowed("laya.judge"),
        version: 1,
        kind: ActionKind::External,
        input: SchemaRef::Compiled(wire_input),
        output: SchemaRef::Compiled(wire_output),
        scopes: Cow::Borrowed(&[Scope::ProcessesWrite]),
        idempotent: true,
        // SPEC_V2 §10.2 fixes the conveyor→laya seam at a 1500 ms default with a 3 s HARD
        // budget: a stalled engine must be abandoned and recorded as an outage, never hold a
        // worker for half a minute (finding v1-p102-1).
        timeout_ms: u32::try_from(LAYA_HARD_TIMEOUT_MS).expect("the laya hard budget is positive"),
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
            service: Cow::Borrowed(LAYA_SERVICE),
            method: Cow::Borrowed("POST"),
            path: Cow::Borrowed(LAYA_PATH),
            auth: Some(Cow::Borrowed(LAYA_TOKEN_SECRET)),
            auth_optional: false,
            upload: None,
        },
    }
}

inventory::submit! { ActionRegistration::new(laya_judge) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_at_or_above_the_threshold_accepts() {
        let verdict = parse_verdict(&json!({"answers": {"accept": {"score": 0.8}}}), Some(0.7))
            .expect("a score");
        assert!(verdict.accepted);
        let verdict = parse_verdict(&json!({"answers": {"accept": {"score": 0.6}}}), Some(0.7))
            .expect("a score");
        assert!(!verdict.accepted);
    }

    #[test]
    fn a_choice_with_confidence_is_read() {
        let accept = json!({"answers": {"accept": {"choice": "accept", "confidence": 0.9}}});
        assert!(parse_verdict(&accept, Some(0.7)).expect("choice").accepted);
        let low = json!({"answers": {"accept": {"choice": "accept", "confidence": 0.3}}});
        assert!(!parse_verdict(&low, Some(0.7)).expect("choice").accepted);
        let reject = json!({"answers": {"accept": {"choice": "reject", "confidence": 0.95}}});
        assert!(!parse_verdict(&reject, None).expect("choice").accepted);
    }

    #[test]
    fn an_unreadable_answer_is_an_outage_not_a_rejection() {
        let missing = parse_verdict(&json!({"answers": {}}), None);
        assert_eq!(missing, Err(JudgeError::Unreadable));
        let garbage = parse_verdict(&json!({"answers": {"accept": {"note": "hm"}}}), None);
        assert_eq!(garbage, Err(JudgeError::Unreadable));
        let not_json = parse_verdict(&json!("upstream said no"), None);
        assert_eq!(not_json, Err(JudgeError::Unreadable));

        // A label that is not one of the two outcomes is the judge declining to decide. Reading
        // `error`/`abstain`/`unknown` as "not accept" spends a reject on an outage.
        for label in ["error", "abstain", "unknown", "maybe"] {
            let answer = json!({"answers": {"accept": {"choice": label, "reason": "no idea"}}});
            assert_eq!(
                parse_verdict(&answer, None),
                Err(JudgeError::Unreadable),
                "the {label:?} label is not a verdict"
            );
        }

        // A failed call's score is not a zero: it is null, or absent from a labelled failure,
        // or the document says outright that it failed.
        let null_score = json!({"answers": {"accept": {"score": null}}});
        assert_eq!(
            parse_verdict(&null_score, None),
            Err(JudgeError::Unreadable)
        );
        let textual_score = json!({"answers": {"accept": {"score": "failed"}}});
        assert_eq!(
            parse_verdict(&textual_score, None),
            Err(JudgeError::Unreadable)
        );
        let failed_call = json!({"answers": {"accept": {
            "score": 0,
            "error": "the model timed out",
        }}});
        assert_eq!(
            parse_verdict(&failed_call, None),
            Err(JudgeError::Unreadable),
            "a placeholder score on a failed call must never spend a reject"
        );
        // A non-string label is unreadable too, never a silent `false`.
        let numeric_label = json!({"answers": {"accept": {"choice": 0}}});
        assert_eq!(
            parse_verdict(&numeric_label, None),
            Err(JudgeError::Unreadable)
        );
    }

    #[test]
    fn the_default_threshold_is_a_coin_flip() {
        let verdict =
            parse_verdict(&json!({"answers": {"accept": {"score": 0.51}}}), None).expect("score");
        assert!(verdict.accepted);
        let verdict =
            parse_verdict(&json!({"answers": {"accept": {"score": 0.49}}}), None).expect("score");
        assert!(!verdict.accepted);
    }

    #[test]
    fn the_request_carries_the_output_criteria_and_a_typed_question() {
        let request = request(
            None,
            &json!({"summary": "done"}),
            &json!(["the tests pass"]),
            Some(&json!({"card": 1})),
        );
        assert_eq!(request["model"], DEFAULT_LAYA_MODEL);
        assert_eq!(request["questions"]["accept"]["type"], "score");
        // The bridge requires an ordered 2–10 level rubric; the SOP criteria are instructions,
        // not the scale. Two levels keep the score equal to P(accepted).
        assert_eq!(
            request["questions"]["accept"]["criteria"],
            json!(SCORE_LEVELS)
        );
        assert_eq!(SCORE_LEVELS.len(), 2, "two levels mean P(accepted)");
        assert!(
            request["questions"]["accept"]["instructions"]
                .as_str()
                .unwrap()
                .contains("the tests pass"),
            "the criteria are readable in the instructions"
        );
        // `state` is required, non-empty, and carries the material to judge.
        let state = request["state"].as_str().expect("state is a string");
        assert!(!state.is_empty(), "an empty state is a 422");
        assert!(
            state.contains("summary"),
            "the output is in the state: {state}"
        );
        assert!(
            state.contains("card"),
            "the case input is in the state: {state}"
        );
    }

    #[test]
    fn the_checker_rules_produce_a_verdict() {
        assert!(checker_verdict(CheckerRule::Always, &json!({})).accepted);
        assert!(checker_verdict(CheckerRule::NonEmpty, &json!({"a": 1})).accepted);
        assert!(!checker_verdict(CheckerRule::NonEmpty, &json!({})).accepted);
    }
}
