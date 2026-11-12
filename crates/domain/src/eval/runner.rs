#![forbid(unsafe_code)]
//! The runner's four properties, as pure functions (S19 Part B step 9; CONTRACT §20).
//!
//! The slice names exactly four, and each exists because its absence was a named failure of the
//! method it ports:
//!
//! 1. **A per-attempt deadline, not an inactivity timer** — [`CaseDeadline`]. A hung stream can
//!    emit keepalives forever; only a total ceiling reclaims the slot, and an expiry is a
//!    classified attempt rather than a zero.
//! 2. **Jittered backoff with the attempt visible** — [`RetryPolicy::backoff_jittered`] (already
//!    in [`super::state`]) plus [`FailureClass`] on every failed attempt, written to
//!    `eval_attempt_error` with its `attempt` and `retry_count`.
//! 3. **Strict-per-attempt scoring** — [`scoring_attempt`]: a row's numbers come from **one**
//!    attempt, never an average or a merge across retries.
//! 4. **A failure class on every failed attempt** — [`FailureClass`], mirroring
//!    `eval_attempt_error.failure_class`'s CHECK verbatim, so the classification cannot be
//!    spelled one way here and refused there.
//!
//! Plus the two assertions that make a comparison trustworthy: [`served_model_matches`] (the
//! model in the response is the model the eval named) and [`subject_request`] (what a case sends
//! to its entry action — built from the input alone, so there is no answer key in scope).
//!
//! Nothing here performs IO.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::state::{EvalError, SubjectInput};

/// Every way an attempt can fail, in `eval_attempt_error.failure_class`'s CHECK order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The subject declined to answer. Its output said so; the call succeeded.
    Refusal,
    /// The runner could not build or read the exchange — our fault, not the model's.
    Harness,
    /// The subject's service failed (5xx, 429, transport).
    Serving,
    /// The per-attempt ceiling fired.
    Timeout,
    /// The response's model is not the one the eval named.
    ServedModelMismatch,
    /// The grader could not decide (see [`super::grade::GradingError`]).
    Grader,
}

impl FailureClass {
    /// The vocabulary in CHECK order.
    pub const ALL: [FailureClass; 6] = [
        FailureClass::Refusal,
        FailureClass::Harness,
        FailureClass::Serving,
        FailureClass::Timeout,
        FailureClass::ServedModelMismatch,
        FailureClass::Grader,
    ];

    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FailureClass::Refusal => "refusal",
            FailureClass::Harness => "harness",
            FailureClass::Serving => "serving",
            FailureClass::Timeout => "timeout",
            FailureClass::ServedModelMismatch => "served_model_mismatch",
            FailureClass::Grader => "grader",
        }
    }

    /// True when a later attempt may succeed without anyone changing the instrument.
    ///
    /// A `refusal`, a `served_model_mismatch` and a `grader` failure all repeat identically on
    /// retry, and a `harness` failure is a bug in our own request; retrying any of them burns
    /// budget and hides the real signal. Only `serving` and `timeout` are retryable.
    pub fn is_retryable(self) -> bool {
        matches!(self, FailureClass::Serving | FailureClass::Timeout)
    }

    /// Classify a non-2xx outcome from the subject's action.
    ///
    /// A 4xx is ours (a case input the action refused, a missing scope, an unknown action); a 5xx
    /// or a 429 is the service's. A 408 is the ceiling by another name.
    pub fn from_status(status: u16) -> Self {
        match status {
            408 => FailureClass::Timeout,
            429 => FailureClass::Serving,
            s if s >= 500 => FailureClass::Serving,
            _ => FailureClass::Harness,
        }
    }
}

impl fmt::Display for FailureClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FailureClass {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|class| class.as_str() == raw.trim())
            .ok_or_else(|| EvalError::Vocabulary {
                what: "failure_class",
                value: raw.to_owned(),
            })
    }
}

/// One attempt's total ceiling.
///
/// ⛔ This is **not** an inactivity timer. `is_expired` compares against a timestamp fixed at the
/// first byte of the attempt, so a response that trickles keepalives for an hour still expires
/// exactly when it was always going to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseDeadline {
    /// When the attempt started.
    pub started_at: DateTime<Utc>,
    /// How long the whole attempt may take.
    pub timeout: Duration,
}

impl CaseDeadline {
    /// A deadline starting now.
    pub fn starting(now: DateTime<Utc>, timeout: Duration) -> Self {
        Self {
            started_at: now,
            timeout,
        }
    }

    /// The instant the attempt is abandoned.
    pub fn deadline_at(&self) -> DateTime<Utc> {
        let millis = i64::try_from(self.timeout.as_millis()).unwrap_or(i64::MAX);
        self.started_at + chrono::Duration::milliseconds(millis)
    }

    /// True once `now` is at or past the ceiling. Never resets.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.deadline_at()
    }
}

/// True when the model the response reports is the model the eval named.
///
/// An exact match, or the configured id followed by a separator and a build suffix — a router
/// that answers `claude-sonnet-5-5-20260101` for `claude-sonnet-5-5` served the model that was
/// asked for. Any other difference is a substitution, and a substituted model invalidates the
/// comparison the eval exists to make (slice trap 7).
pub fn served_model_matches(configured: &str, served: &str) -> bool {
    let configured = configured.trim();
    let served = served.trim();
    if configured.is_empty() || served.is_empty() {
        return false;
    }
    served == configured
        || served
            .strip_prefix(configured)
            .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with('@'))
}

/// One attempt's contribution to a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttemptOutcome {
    /// The attempt number, from 1.
    pub attempt: u32,
    /// True when this attempt produced a scorable output.
    pub scored: bool,
}

/// The attempt whose output the result row describes.
///
/// Strict per attempt: exactly one attempt's numbers reach the row. The highest scored attempt
/// wins, which is the one whose trace and grade were written together; an earlier attempt that
/// also scored means the slot was written twice, which `eval_case_result_slot` refuses before
/// this function is ever reached.
pub fn scoring_attempt(outcomes: &[AttemptOutcome]) -> Option<u32> {
    outcomes
        .iter()
        .filter(|outcome| outcome.scored)
        .map(|outcome| outcome.attempt)
        .max()
}

/// An `eval_run`'s lifecycle, mirroring `eval_run.status`'s CHECK.
///
/// `queued → running → partial ⇄ running → complete | void`, plus `cancelled` from any
/// non-terminal state. `complete` is **not** terminal: a pilot is a `mode='pilot'` run on the
/// baseline variant, and the full pass that supersedes it resumes the same row rather than
/// inserting a second one (`eval_run_variant_unique` forbids that, and the slice says the pilot's
/// rows are kept).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Created, jobs enqueued, nothing run yet.
    Queued,
    /// At least one case is running.
    Running,
    /// Some slots are filled and none is in flight.
    Partial,
    /// Every slot is filled.
    Complete,
    /// Abandoned by the reaper or by an operator; never resumes.
    Void,
    /// Cancelled by a person; the driver no-ops from here on.
    Cancelled,
}

impl RunStatus {
    /// The vocabulary in CHECK order.
    pub const ALL: [RunStatus; 6] = [
        RunStatus::Queued,
        RunStatus::Running,
        RunStatus::Partial,
        RunStatus::Complete,
        RunStatus::Void,
        RunStatus::Cancelled,
    ];

    /// The states a run never leaves.
    pub const TERMINAL: [RunStatus; 2] = [RunStatus::Void, RunStatus::Cancelled];

    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::Partial => "partial",
            RunStatus::Complete => "complete",
            RunStatus::Void => "void",
            RunStatus::Cancelled => "cancelled",
        }
    }

    /// True when the run cannot change again.
    pub fn is_terminal(self) -> bool {
        Self::TERMINAL.contains(&self)
    }

    /// True when the run's work is over and its rows can be read as a result.
    ///
    /// ⛔ **The complement of [`is_terminal`](Self::is_terminal) for `complete`**, and the
    /// question both the climb and a regrade actually ask. S19 defines terminal as *cannot
    /// change again*, because a `complete` run is resumable through `eval_runs.start`; a reader
    /// that asked `is_terminal` therefore saw every finished run as absent. `void` and
    /// `cancelled` are over but unmeasurable, and `queued`/`running` are still moving.
    pub fn finished(self) -> bool {
        matches!(self, RunStatus::Complete | RunStatus::Partial)
    }

    /// True while the tick is responsible for heartbeating this run.
    pub fn is_in_flight(self) -> bool {
        matches!(
            self,
            RunStatus::Queued | RunStatus::Running | RunStatus::Partial
        )
    }

    /// True when the driver should skip a case of this run.
    pub fn stops_cases(self) -> bool {
        matches!(
            self,
            RunStatus::Cancelled | RunStatus::Complete | RunStatus::Void
        )
    }

    /// Whether the documented table allows `self → to`.
    pub fn may_transition_to(self, to: RunStatus) -> bool {
        if self == to {
            return false;
        }
        match self {
            RunStatus::Queued => matches!(
                to,
                RunStatus::Running | RunStatus::Cancelled | RunStatus::Void
            ),
            RunStatus::Running => matches!(
                to,
                RunStatus::Partial | RunStatus::Complete | RunStatus::Cancelled | RunStatus::Void
            ),
            RunStatus::Partial => matches!(
                to,
                RunStatus::Running | RunStatus::Complete | RunStatus::Cancelled | RunStatus::Void
            ),
            // A pilot's completion is superseded by a full start on the same row.
            RunStatus::Complete => matches!(to, RunStatus::Running | RunStatus::Cancelled),
            RunStatus::Void | RunStatus::Cancelled => false,
        }
    }

    /// Apply the table, or refuse with a named error.
    pub fn transition(self, to: RunStatus) -> Result<Self, EvalError> {
        if !self.may_transition_to(to) {
            return Err(EvalError::IllegalRunTransition { from: self, to });
        }
        Ok(to)
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunStatus {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == raw.trim())
            .ok_or_else(|| EvalError::Vocabulary {
                what: "eval_run.status",
                value: raw.to_owned(),
            })
    }
}

/// How much of the case set a run covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// A small first pass; its rows are kept and superseded by the full run.
    Pilot,
    /// The whole case set.
    Full,
}

impl RunMode {
    /// The vocabulary.
    pub const ALL: [RunMode; 2] = [RunMode::Pilot, RunMode::Full];

    /// The column spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RunMode::Pilot => "pilot",
            RunMode::Full => "full",
        }
    }
}

impl fmt::Display for RunMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunMode {
    type Err = EvalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == raw.trim())
            .ok_or_else(|| EvalError::Vocabulary {
                what: "eval_run.mode",
                value: raw.to_owned(),
            })
    }
}

/// Why a case input could not be turned into a subject request.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    /// `inputs_schema` is non-empty, so the case input must be a JSON object — and is not.
    #[error("this eval declares a structured input, so a case input must be a JSON object: {why}")]
    NotAnObject {
        /// What went wrong reading it.
        why: String,
    },
}

/// Build the request a case sends to its entry action.
///
/// One rule, and it is gated on an **authored** field rather than on the text (see the plan's
/// D1): when `inputs_schema` is empty — the default — the case input is a prompt and the request
/// is `{"input": <text>}`. When it is a non-empty object the case input must itself be a JSON
/// object and *is* the request, verbatim. Sniffing the text instead would read a prompt that
/// happens to contain braces as a document.
///
/// `model_ref`, when set and the request does not already carry one, is injected as `model`.
/// Nothing here can see an answer key: [`SubjectInput`] has no such field to pass.
pub fn subject_request(
    input: &SubjectInput<'_>,
    inputs_schema: &Value,
    model_ref: Option<&str>,
) -> Result<Value, RequestError> {
    let structured = inputs_schema.as_object().is_some_and(|map| !map.is_empty());

    let mut request = if structured {
        match serde_json::from_str::<Value>(input.input.trim()) {
            Ok(Value::Object(map)) => map,
            Ok(_) => {
                return Err(RequestError::NotAnObject {
                    why: "it parses, but it is not an object".to_owned(),
                });
            }
            Err(error) => {
                return Err(RequestError::NotAnObject {
                    why: error.to_string(),
                });
            }
        }
    } else {
        let mut map = Map::new();
        map.insert("input".to_owned(), Value::String(input.input.to_owned()));
        map
    };

    if let Some(model) = model_ref.map(str::trim).filter(|model| !model.is_empty()) {
        request
            .entry("model".to_owned())
            .or_insert_with(|| Value::String(model.to_owned()));
    }

    Ok(Value::Object(request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_failure_class_round_trips() {
        for class in FailureClass::ALL {
            assert_eq!(class.as_str().parse::<FailureClass>(), Ok(class));
        }
        assert!(matches!(
            "plumbing".parse::<FailureClass>(),
            Err(EvalError::Vocabulary { .. })
        ));
    }

    #[test]
    fn only_serving_and_timeout_are_retryable() {
        for class in FailureClass::ALL {
            assert_eq!(
                class.is_retryable(),
                matches!(class, FailureClass::Serving | FailureClass::Timeout),
                "{class}"
            );
        }
    }

    #[test]
    fn statuses_classify_by_whose_fault_it_is() {
        assert_eq!(FailureClass::from_status(408), FailureClass::Timeout);
        assert_eq!(FailureClass::from_status(429), FailureClass::Serving);
        assert_eq!(FailureClass::from_status(500), FailureClass::Serving);
        assert_eq!(FailureClass::from_status(503), FailureClass::Serving);
        assert_eq!(FailureClass::from_status(400), FailureClass::Harness);
        assert_eq!(FailureClass::from_status(403), FailureClass::Harness);
        assert_eq!(FailureClass::from_status(422), FailureClass::Harness);
    }

    #[test]
    fn a_deadline_does_not_move_when_bytes_keep_arriving() {
        let started = Utc::now();
        let deadline = CaseDeadline::starting(started, Duration::from_secs(60));
        let at = deadline.deadline_at();
        assert_eq!(at, started + chrono::Duration::seconds(60));

        // Every "keepalive" is a fresh observation of the same fixed ceiling.
        assert!(!deadline.is_expired(started + chrono::Duration::seconds(59)));
        assert!(deadline.is_expired(at));
        assert!(deadline.is_expired(at + chrono::Duration::hours(4)));
        assert_eq!(deadline.deadline_at(), at, "the ceiling never moved");
    }

    #[test]
    fn a_build_suffix_is_the_same_model_and_anything_else_is_not() {
        assert!(served_model_matches(
            "claude-sonnet-5-5",
            "claude-sonnet-5-5"
        ));
        assert!(served_model_matches(
            "claude-sonnet-5-5",
            "claude-sonnet-5-5-20260101"
        ));
        assert!(served_model_matches(
            "claude-sonnet-5-5",
            "claude-sonnet-5-5@20260101"
        ));
        assert!(!served_model_matches(
            "claude-sonnet-5-5",
            "claude-opus-5-5"
        ));
        assert!(!served_model_matches("claude-sonnet-5-5", "gpt-4"));
        assert!(!served_model_matches("claude-sonnet-5-5", ""));
        assert!(!served_model_matches("", "claude-sonnet-5-5"));
    }

    #[test]
    fn scoring_picks_one_attempt_and_never_a_merge() {
        assert_eq!(scoring_attempt(&[]), None);
        assert_eq!(
            scoring_attempt(&[
                AttemptOutcome {
                    attempt: 1,
                    scored: false
                },
                AttemptOutcome {
                    attempt: 2,
                    scored: true
                },
            ]),
            Some(2)
        );
        assert_eq!(
            scoring_attempt(&[
                AttemptOutcome {
                    attempt: 1,
                    scored: false
                },
                AttemptOutcome {
                    attempt: 2,
                    scored: false
                },
            ]),
            None
        );
    }

    #[test]
    fn a_plain_prompt_becomes_an_input_field() {
        let request = subject_request(
            &SubjectInput { input: "why?" },
            &json!({}),
            Some("claude-sonnet-5-5"),
        )
        .expect("built");
        assert_eq!(
            request,
            json!({"input": "why?", "model": "claude-sonnet-5-5"})
        );
    }

    #[test]
    fn a_prompt_that_looks_like_json_stays_a_prompt() {
        let request = subject_request(
            &SubjectInput {
                input: "{\"not\": \"a request\"}",
            },
            &json!({}),
            None,
        )
        .expect("built");
        assert_eq!(request, json!({"input": "{\"not\": \"a request\"}"}));
    }

    #[test]
    fn a_structured_eval_sends_the_case_input_verbatim() {
        let request = subject_request(
            &SubjectInput {
                input: r#"{"messages":[{"role":"user","content":"hi"}]}"#,
            },
            &json!({"type": "object"}),
            None,
        )
        .expect("built");
        assert_eq!(
            request,
            json!({"messages": [{"role": "user", "content": "hi"}]})
        );
    }

    #[test]
    fn a_structured_eval_refuses_a_non_object_case_input() {
        for bad in ["not json", "[1,2,3]", "42"] {
            let refused = subject_request(
                &SubjectInput { input: bad },
                &json!({"type": "object"}),
                None,
            )
            .expect_err("refused");
            assert!(matches!(refused, RequestError::NotAnObject { .. }), "{bad}");
        }
    }

    #[test]
    fn a_model_already_in_the_request_is_not_overwritten() {
        let request = subject_request(
            &SubjectInput {
                input: r#"{"model":"kept"}"#,
            },
            &json!({"type": "object"}),
            Some("ignored"),
        )
        .expect("built");
        assert_eq!(request["model"], json!("kept"));
    }

    #[test]
    fn every_run_status_round_trips() {
        for status in RunStatus::ALL {
            assert_eq!(status.as_str().parse::<RunStatus>(), Ok(status));
        }
        assert!(matches!(
            "paused".parse::<RunStatus>(),
            Err(EvalError::Vocabulary { .. })
        ));
    }

    #[test]
    fn the_run_transition_table_is_exactly_the_documented_one() {
        use RunStatus::*;
        let legal = [
            (Queued, Running),
            (Queued, Cancelled),
            (Queued, Void),
            (Running, Partial),
            (Running, Complete),
            (Running, Cancelled),
            (Running, Void),
            (Partial, Running),
            (Partial, Complete),
            (Partial, Cancelled),
            (Partial, Void),
            (Complete, Running),
            (Complete, Cancelled),
        ];
        for from in RunStatus::ALL {
            for to in RunStatus::ALL {
                let expected = legal.contains(&(from, to));
                assert_eq!(from.may_transition_to(to), expected, "{from} -> {to}");
            }
            assert!(!from.may_transition_to(from), "{from} -> itself");
        }
        assert!(matches!(
            RunStatus::Void.transition(RunStatus::Running),
            Err(EvalError::IllegalRunTransition { .. })
        ));
        assert_eq!(
            RunStatus::Queued.transition(RunStatus::Running),
            Ok(RunStatus::Running)
        );
    }

    #[test]
    fn only_the_terminal_run_states_stop_the_driver() {
        for status in RunStatus::ALL {
            let expected = matches!(status, RunStatus::Void | RunStatus::Cancelled);
            assert_eq!(status.is_terminal(), expected, "{status}");
        }
        // `complete` is not terminal: a full start resumes a finished pilot.
        assert!(!RunStatus::Complete.is_terminal());
        assert!(RunStatus::Complete.stops_cases());
        assert!(RunStatus::Queued.is_in_flight());
        assert!(!RunStatus::Complete.is_in_flight());
    }

    #[test]
    fn every_run_mode_round_trips() {
        for mode in RunMode::ALL {
            assert_eq!(mode.as_str().parse::<RunMode>(), Ok(mode));
        }
        assert!(matches!(
            "sampled".parse::<RunMode>(),
            Err(EvalError::Vocabulary { .. })
        ));
    }
}
