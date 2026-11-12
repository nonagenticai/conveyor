#![forbid(unsafe_code)]
//! The eval instrument's store (S19 Part A step 3; SPEC D25/D26, CONTRACT §20).
//!
//! The domain below this crate resolved the vocabulary, the status machine, the instrument
//! digest and the split purely; this module is where those resolutions are written down and read
//! back inside the caller's tenant. Four submodules, one per table group:
//!
//! - [`evals`] — `eval`, `eval_version`, the digest and the one-shot split lock, plus the two
//!   sign-off reads (`approval` rows carrying `eval_id`) that [`evals::eval_readiness_tx`] turns
//!   into [`conveyor_domain::eval::ReadinessFacts`].
//! - [`cases`] — `eval_case`. `expected` is write-only: it appears on no returned row.
//! - [`metrics`] — `eval_metric`, the graded and measured fields of the report.
//! - [`results`] — `eval_case_result` (one row per `(run, case, rep)` slot), the append-only
//!   `eval_attempt_error`, and the human label that releases a `pending_label` row.
//! - [`runs`] — `eval_run`: the measurement's header, its lifecycle, and the two derived counts
//!   the tick finalises from (filled slots and outstanding jobs).
//! - [`traces`] — `eval_trace`: one transcript per slot, hashed with the instrument digest's own
//!   `content_hash` and capped by the table's own CHECK.
//! - [`grades`] — `eval_grade`: one output graded once, the one-shot path an
//!   `inspection.kind = 'eval'` SOP uses (migration 0034).
//!
//! ⛔ Two properties are load-bearing, which is why this is not a plain CRUD layer:
//!
//! - **A sign-off is a digest match over the instrument as it is NOW, never a stored status.**
//!   `eval_version.digest` is the value frozen when the version was created;
//!   [`evals::version_digest_tx`] recomputes it from the current rows, so editing any case,
//!   metric, price or harness path voids both gates with no status write anywhere.
//! - **The split lock is one-shot.** [`evals::lock_split_tx`] is a conditional
//!   `UPDATE … WHERE split_sha256 IS NULL`; the second caller gets
//!   [`EvalStoreError::SplitLocked`], so two concurrent draws cannot both win.
//!
//! Writes are `*_tx` free functions: they take an open `&mut PgConnection` the caller scoped with
//! [`crate::begin_scoped`], plus the [`TenantId`] they bind explicitly, so the row and the
//! caller's outbox event share one transaction (CONTRACT §7). Every statement is a runtime
//! `sqlx::query` / `query_as` with binds — never `query!`.
//!
//! A refused write is reported by the database's own name: 23505 becomes
//! [`EvalStoreError::Conflict`] carrying the constraint (or unique index) name, and 23514
//! becomes [`EvalStoreError::Invalid`]. "The insert failed" is never the assertion — the name is.

pub mod audits;
pub mod cases;
pub mod climb;
pub mod evals;
pub mod grades;
pub mod metrics;
pub mod results;
pub mod runs;
pub mod traces;

pub use audits::{
    AuditRow, FindingRow, NewAudit, NewFinding, decide_finding_tx, get_audit_tx, insert_audit_tx,
    insert_finding_tx, list_audits_tx, list_findings_tx, transition_audit_tx,
};
pub use cases::{
    EvalCase, NewEvalCase, UpdateEvalCase, delete_case_tx, get_case_expected_tx, get_case_tx,
    insert_case_tx, list_cases_tx, update_case_tx,
};
pub use evals::{
    Eval, EvalApproval, EvalVersion, NewEval, UpdateEval, cases_for_split_tx, create_version_tx,
    eval_readiness_tx, get_eval_approval_tx, get_eval_tx, get_version_tx, insert_eval_tx,
    latest_eval_approval_tx, list_evals_tx, list_versions_tx, lock_split_tx, transition_eval_tx,
    update_eval_tx, version_digest_tx,
};
pub use grades::{
    EvalGrade, NewEvalGrade, TERMINAL_STATUSES, TerminalGrade, get_grade_tx, insert_pending_tx,
    set_terminal_tx,
};
pub use metrics::{
    EvalMetric, NewEvalMetric, UpdateEvalMetric, delete_metric_tx, insert_metric_tx,
    list_metrics_tx, update_metric_tx,
};
pub use results::{
    AttemptError, LabelRefusal, NewAttemptError, NewCaseResult, ResultFilter, ResultRow,
    attempt_errors_tx, get_result_tx, insert_attempt_error_tx, insert_case_result_tx,
    label_result_tx, list_results_tx, next_attempt_tx,
};
pub use runs::{
    NewRun, Run, SlotCounts, find_regrade_tx, find_run_tx, get_run_tx, in_flight_runs_tx,
    insert_run_tx, list_runs_tx, outstanding_jobs_tx, slot_counts_tx, stale_runs_tx,
    touch_polled_tx, transition_run_tx, widen_run_tx,
};
pub use traces::{NewTrace, Trace, get_trace_tx, insert_trace_tx};

use conveyor_domain::eval::EvalError;

/// Why an eval store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum EvalStoreError {
    /// No such row in this tenant.
    #[error("the {what} does not exist in this tenant")]
    NotFound {
        /// What was looked for (`eval`, `eval version`, `eval case`, `eval metric`,
        /// `eval approval`).
        what: &'static str,
    },
    /// A unique constraint refused the row (SQLSTATE 23505).
    ///
    /// `constraint` is the database's own name — an index name for a unique index,
    /// a constraint name for a `UNIQUE` constraint.
    #[error("a conflicting row already exists ({constraint})")]
    Conflict {
        /// The refusing constraint or unique index.
        constraint: String,
    },
    /// A `CHECK` constraint refused the value (SQLSTATE 23514).
    #[error("the database refused the value ({constraint})")]
    Invalid {
        /// The refusing constraint.
        constraint: String,
    },
    /// The version's split was already drawn and locked.
    #[error("the split is already locked for this eval version")]
    SplitLocked,
    /// A result was not in a state that admits the write.
    #[error("the eval case result is not awaiting a label")]
    Label(LabelRefusal),
    /// An audit or finding was not in a state that admits the write.
    #[error("the {what} is not in a state that admits this write")]
    NotOpen {
        /// What was being written (`eval audit`, `eval audit finding`).
        what: &'static str,
    },
    /// The domain refused the transition.
    #[error(transparent)]
    Illegal(#[from] EvalError),
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The two database refusals this module reports by name.
///
/// A unique violation and a check violation are the schema's own guarantees; the constraint
/// name is what a caller (and a test) asserts on, because "the insert failed" is satisfied by
/// any number of wrong reasons — a missing column, a missing grant, a typo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Violation {
    /// SQLSTATE 23505, carrying the unique constraint or index name.
    Unique {
        /// The refusing constraint or unique index.
        constraint: String,
    },
    /// SQLSTATE 23514, carrying the check constraint name.
    Check {
        /// The refusing constraint.
        constraint: String,
    },
}

/// Classify a database refusal as a unique or check violation, by name.
///
/// Returns `None` for every other error (a foreign-key refusal, a syntax error, a deadlock), so
/// a caller never mistakes one class for another: [`EvalStoreError::Invalid`] must not be
/// reported for a unique violation merely because both are "the database said no".
pub(crate) fn check_violation_kind(error: &sqlx::Error) -> Option<Violation> {
    let database = error.as_database_error()?;
    let constraint = database.constraint().unwrap_or_default().to_owned();
    match database.code().as_deref() {
        Some("23505") => Some(Violation::Unique { constraint }),
        Some("23514") => Some(Violation::Check { constraint }),
        _ => None,
    }
}

/// Map a write's error onto [`EvalStoreError`], naming the constraint when there is one.
///
/// Every write in this module funnels through this function, so a 23505 can never be reported
/// as a generic database error in one statement and a conflict in another.
pub(crate) fn map_write_error(error: sqlx::Error) -> EvalStoreError {
    match check_violation_kind(&error) {
        Some(Violation::Unique { constraint }) => EvalStoreError::Conflict { constraint },
        Some(Violation::Check { constraint }) => EvalStoreError::Invalid { constraint },
        None => EvalStoreError::Database(error),
    }
}
