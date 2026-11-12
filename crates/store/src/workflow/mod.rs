#![forbid(unsafe_code)]
//! The workflow engine's store (S13 step 3; 02-DATA-ARCHITECTURE §2.13).
//!
//! One table, one document. [`workflow`] is `workflow`'s CRUD and its publish freeze:
//!
//! - **A row is a version, not a key.** `key` is the stable name (`cap.x`) and every fork is a new
//!   row at `max(version) + 1`; `uq_workflow_published` is a partial unique index over the key, so
//!   "one published version" is the database's rule and not the action's discipline.
//! - **Publish freezes a revision before it flips the status.** The document is hashed with
//!   [`conveyor_domain::compare::content_hash`] — the same canonical-JSON hash every other family
//!   uses — and written through [`crate::governance::write_snapshot_revision`], the one revision
//!   writer. The previous published row of the same key is retired in the same transaction,
//!   *before* this row enters `published`, because the index is immediate.
//! - **The 256 KiB ceiling is checked in Rust first and by the table's CHECK second.** The Rust
//!   check refuses with a named error; the CHECK is what a hand-written SQL statement meets.
//!
//! Writes are `*_tx` free functions: they take an open `&mut PgConnection` the caller scoped with
//! [`crate::begin_scoped`], plus the [`TenantId`] they bind explicitly, so the row and the caller's
//! outbox event share one transaction (CONTRACT §7). Every statement is a runtime `sqlx::query` /
//! `query_as` with binds — never `query!`.
//!
//! A refused write is reported by the database's own name: 23505 becomes
//! [`WorkflowStoreError::Conflict`] carrying the constraint (or unique index) name, and 23514
//! becomes [`WorkflowStoreError::Invalid`]. "The insert failed" is never the assertion — the name
//! is. `run_step`'s trace writes are not here yet: they land with the executor in S13 step 6.

// The slice names this file `workflow/workflow.rs`. Clippy reads the repetition as taste, not as
// a defect; the path is the contract's, so the lint is silenced here rather than by renaming.
#[allow(clippy::module_inception)]
pub mod workflow;

/// S13 steps 6–7: a workflow run's rows, its driving job and the trace replay reads.
pub mod run;
pub mod run_step;

use conveyor_domain::Version;

/// The `revision.entity_type` every workflow publish writes.
pub const WORKFLOW_ENTITY_TYPE: &str = "workflow";

/// The document ceiling S13 §5.2 sets — 256 KiB, measured over the serialised document.
pub const MAX_DEFINITION_BYTES: usize = 262_144;

/// Everything a workflow store call can refuse.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowStoreError {
    /// No such row in this tenant.
    #[error("the workflow does not exist in this tenant")]
    NotFound,
    /// The row is not a draft, so it cannot be edited or published.
    #[error("the workflow is {0}, not a draft")]
    NotDraft(String),
    /// The row is not published, so it cannot be retired.
    #[error("the workflow is not published")]
    NotPublished,
    /// `If-Match` named a version the row is not at.
    #[error("the workflow is at a different version than the caller supplied")]
    VersionMismatch {
        /// The row's current version.
        current: Version,
    },
    /// The document exceeds [`MAX_DEFINITION_BYTES`].
    #[error("the workflow definition is {bytes} bytes; the ceiling is {MAX_DEFINITION_BYTES}")]
    DefinitionTooLarge {
        /// The serialised length the caller supplied.
        bytes: usize,
    },
    /// A unique constraint refused the row (SQLSTATE 23505).
    #[error("a conflicting workflow row already exists ({constraint})")]
    Conflict {
        /// The refusing constraint or unique index.
        constraint: String,
    },
    /// A `CHECK` constraint refused the value (SQLSTATE 23514).
    #[error("the database refused the workflow value ({constraint})")]
    Invalid {
        /// The refusing constraint.
        constraint: String,
    },
    /// The database refused the statement for any other reason.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The two database refusals this module reports by name.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Violation {
    /// SQLSTATE 23505, carrying the unique constraint or index name.
    Unique { constraint: String },
    /// SQLSTATE 23514, carrying the check constraint name.
    Check { constraint: String },
}

/// Classify a database refusal as a unique or check violation, by name.
fn violation_kind(error: &sqlx::Error) -> Option<Violation> {
    let database = error.as_database_error()?;
    let constraint = database.constraint().unwrap_or_default().to_owned();
    match database.code().as_deref() {
        Some("23505") => Some(Violation::Unique { constraint }),
        Some("23514") => Some(Violation::Check { constraint }),
        _ => None,
    }
}

/// Map a write's error onto [`WorkflowStoreError`], naming the constraint when there is one.
pub(crate) fn map_write_error(error: sqlx::Error) -> WorkflowStoreError {
    match violation_kind(&error) {
        Some(Violation::Unique { constraint }) => WorkflowStoreError::Conflict { constraint },
        Some(Violation::Check { constraint }) => WorkflowStoreError::Invalid { constraint },
        None => WorkflowStoreError::Database(error),
    }
}

/// Refuse a document the table's CHECK would refuse anyway, with the caller's byte count.
pub(crate) fn ensure_definition_fits(
    definition: &serde_json::Value,
) -> Result<(), WorkflowStoreError> {
    let bytes = serde_json::to_vec(definition)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if bytes > MAX_DEFINITION_BYTES {
        return Err(WorkflowStoreError::DefinitionTooLarge { bytes });
    }
    Ok(())
}
