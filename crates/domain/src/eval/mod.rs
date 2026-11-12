#![forbid(unsafe_code)]
//! The eval instrument's pure core (S19 Part A; SPEC D25/D26, CONTRACT §20).
//!
//! - [`state`] — the instrument's vocabulary and predicates: subject kinds, the status machine,
//!   variants, gates, readiness, the retry policy, the answer-key-free subject input and the
//!   instrument digest.
//! - [`split`] — the deterministic, tag-stratified train/test draw, its canonical hash, and the
//!   noise predicate.
//! - [`grade`] — the four grader kinds and the single grade shape they produce; a grader that
//!   cannot decide is a `grader` failure, never a zero.
//! - [`runner`] — the four runner properties (a total per-attempt ceiling, a failure class on
//!   every failed attempt, strict-per-attempt scoring, the served-model assertion) and the one
//!   rule that turns a case input into the request its entry action receives.
//!
//! Nothing here performs IO: the store writes what these functions resolved, the driver (Part B)
//! enforces the retry schedule, and the actions (Part A steps 4–7) are the only callers.
//!
//! One rule bears restating at this level: an answer key never reaches a subject. It lives in
//! [`state::CaseMaterial`] — where only the digest and the grader read it — and in no other type
//! in this module. [`state::SubjectInput`] is the whole of what a model is shown, and
//! `tests/eval_isolation.rs` pins its serialised shape.

pub mod audit;
pub mod climb;
pub mod grade;
pub mod runner;
pub mod split;
pub mod state;

pub use audit::{
    CaseFacts, Controls, FindingDraft, InstrumentFacts, MetricFacts, RunFacts, Section, Severity,
    findings, resolution_line, resolution_shortfall,
};

pub use climb::{
    Bucket, BucketCount, BucketReport, ClimbError, Decision, Direction, Goal, Hold, HoldReading,
    LEVERS, Movement, NotComparable, Prediction, PriorPattern, RevertReason, RoundOutcome,
    RunSignature, StopReason, Stopping, comparable, content_round_refused, delta_above_floor,
    keep_or_revert, noise_floor, paired_delta_ci, resolved, stopping_reached, triage_required,
};
pub use grade::{
    Expectation, Grade, GraderKind, GradingError, JudgeVerdict, grade_programmatic, normalise,
    parse_verdict,
};
pub use runner::{
    AttemptOutcome, CaseDeadline, FailureClass, RequestError, RunMode, RunStatus, scoring_attempt,
    served_model_matches, subject_request,
};
pub use split::{
    CaseSplit, SplitAssignment, SplitCase, TagCounts, draw_split, means_within_noise, split_sha256,
};
pub use state::{
    CaseMaterial, EvalError, EvalGate, EvalStatus, Instrument, MetricMaterial, MissingGate,
    Readiness, ReadinessFacts, RetryPolicy, SubjectInput, SubjectKind, Variant, instrument_digest,
};
