#![forbid(unsafe_code)]
//! The control plane's action surface: claims, runs, approvals, andon, messages, notifications
//! and channels (S06).
//!
//! Every action is one `#[action]` declaration in [`cards`], [`runs`], [`approvals`],
//! [`andon`], [`messages`] or [`channels`]; the registry, the OpenAPI operation, the MCP tool
//! and the invocation route are projections of it (S04 §5.1). Nothing here speaks SQL
//! directly — the queries live in `crates/store` (CONTRACT §1) — and every id is resolved in
//! the caller's tenant before a write, because a single-column FK does not enforce tenancy
//! (CONTRACT §2).
//!
//! **Wave A ships the skeleton.** The six family files declare no actions yet; wave B fills
//! them against the store primitives (`renew_card_tx`, `sweep_expired_leases_tx`,
//! `fail_run_tx`, `list_runs_tx`, `live_run_for_card_tx`, `reap_dead_runs_tx`, `open_gate_tx`,
//! `void_pending_for_run_tx`, `list_approvals_tx`, `gauge_snapshot`) and the documents in
//! [`views`]. The mappers below are the one place a store failure becomes an RFC 9457
//! problem, so every family refuses the same way.

use std::str::FromStr;

use conveyor_domain::{RunError, Version};
use conveyor_store as store;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

pub mod andon;
pub mod approvals;
pub mod cards;
pub mod channels;
pub mod messages;
pub mod runs;
pub mod views;

pub use views::*;

/// A store failure: a unique violation is `409`, everything else a scrubbed `500`.
pub fn store_error(error: sqlx::Error, action: &str) -> ActionError {
    if let Some(database) = error.as_database_error()
        && database.is_unique_violation()
    {
        return ActionError::new(
            409,
            "conflict",
            format!("{action:?} would create a duplicate row in this tenant"),
        );
    }
    tracing::error!(%error, action, "store failure");
    ActionError::internal()
}

/// A parent id that did not resolve in this tenant is a `404`, never an FK violation.
pub fn parent_error(error: store::ParentError, _action: &str) -> ActionError {
    match error {
        store::ParentError::NotFound { .. } => ActionError::resource_not_found("such parent"),
        store::ParentError::Database(error) => store_error(error, _action),
    }
}

/// [`store_error`] for the card queries the claim actions use.
pub fn card_error(error: store::CardStoreError, action: &str) -> ActionError {
    match error {
        store::CardStoreError::Database(error) => store_error(error, action),
        store::CardStoreError::Parent(parent) => parent_error(parent, action),
        store::CardStoreError::ListNotFound => ActionError::resource_not_found("such list"),
        store::CardStoreError::ParentCardOnAnotherBoard => invalid(
            "parent_card_id",
            "invalid_value",
            "the parent card is on another board",
        ),
    }
}

/// [`store_error`] for the run queries.
///
/// A transition the domain refuses is a `409`: the state moved underneath the caller, and
/// CONTRACT §3 asks for the current state in the problem body rather than a precondition
/// failure (a run's lifecycle is not version-addressed by the client).
pub fn run_error(error: store::RunStoreError, action: &str) -> ActionError {
    match error {
        store::RunStoreError::NotFound => ActionError::resource_not_found("such run"),
        store::RunStoreError::LiveRunExists => ActionError::new(
            409,
            "conflict",
            format!("{action:?}: the card already has a live run"),
        ),
        store::RunStoreError::Illegal(illegal) => run_illegal(illegal),
        store::RunStoreError::UnknownStatus(raw) => {
            tracing::error!(status = %raw, "a stored run.status is outside the six the domain knows");
            ActionError::internal()
        }
        store::RunStoreError::ParentNotLive => ActionError::new(
            409,
            "run.parent_not_live",
            "the parent run is not live; a child may only start under a live parent",
        ),
        store::RunStoreError::ChildCardMismatch => invalid(
            "card_id",
            "invalid_value",
            "a child run must be on its parent's card",
        ),
        store::RunStoreError::DepthExceeded => ActionError::new(
            422,
            "run.depth_exceeded",
            "the run tree is at most 3 levels deep",
        ),
        store::RunStoreError::ChildWithoutStep => invalid(
            "step_id",
            "required",
            "a child run must name the step that started it",
        ),
        store::RunStoreError::ChildStepAmbiguous => invalid(
            "workflow_step_id",
            "invalid_value",
            "a child names either step_id (a process parent) or workflow_step_id (a workflow \
             parent), not both",
        ),
        store::RunStoreError::StepNotFound => ActionError::resource_not_found("such step"),
        store::RunStoreError::LiveChildExists => ActionError::new(
            409,
            "run.child_already_live",
            "this parent step already has a live child run",
        ),
        store::RunStoreError::ChildAttemptExists => ActionError::new(
            409,
            "run.child_attempt_exists",
            "this parent step already has a run at that attempt",
        ),
        store::RunStoreError::JobBindingStale {
            binding_hash,
            requested_hash,
        } => ActionError::new(
            409,
            "job_binding_stale",
            format!(
                "the binding pins release {binding_hash} but the run requested {requested_hash}; \
                 publish a new release and bind it"
            ),
        ),
        store::RunStoreError::ReleaseWithoutBinding => invalid(
            "release_hash",
            "binding_required",
            "a run that names a release must name the job binding that pins it",
        ),
        store::RunStoreError::BindingNotFound => ActionError::resource_not_found("such binding"),
        store::RunStoreError::Organisation(error) => {
            tracing::error!(%error, action, "the organisation spine refused the run's binding");
            ActionError::internal()
        }
        store::RunStoreError::Parent(parent) => parent_error(parent, action),
        // The run's own bookkeeping (its step skip / descendant sweep) refused. The action layer
        // never names this directly, so it is an internal failure, not a caller's mistake.
        store::RunStoreError::Process(error) => {
            tracing::error!(%error, action, "the run's step bookkeeping failed");
            ActionError::internal()
        }
        store::RunStoreError::Database(error) => store_error(error, action),
    }
}

/// A refused run transition (`409`, CONTRACT §3).
pub fn run_illegal(error: RunError) -> ActionError {
    ActionError::new(409, "conflict", error.to_string())
}

/// [`store_error`] for the approval queries.
///
/// Both unique violations are `409`: `uq_appr_pending` (a second pending gate for one
/// `(run, gate)`) and `(approval_id, approver_id)` (one person deciding twice — the guarantee
/// dual control rests on).
pub fn approval_error(error: store::ApprovalStoreError, action: &str) -> ActionError {
    match error {
        store::ApprovalStoreError::Conflict => ActionError::new(
            409,
            "conflict",
            format!("{action:?}: a pending approval already exists for this (run, gate)"),
        ),
        store::ApprovalStoreError::AlreadyDecided => ActionError::new(
            409,
            "conflict",
            format!("{action:?}: this approver already decided this approval"),
        ),
        store::ApprovalStoreError::NotFound => ActionError::resource_not_found("such approval"),
        store::ApprovalStoreError::Illegal(illegal) => {
            ActionError::new(409, "conflict", illegal.to_string())
        }
        store::ApprovalStoreError::Vocabulary { what, value } => {
            tracing::error!(what, value = %value, "a stored approval value is outside the domain");
            ActionError::internal()
        }
        store::ApprovalStoreError::Parent(parent) => parent_error(parent, action),
        store::ApprovalStoreError::Run(error) => run_error(error, action),
        store::ApprovalStoreError::HumanOnlyGate => ActionError::new(
            403,
            "signoff_requires_human",
            format!("{action:?}: an eval sign-off must be decided by a person"),
        ),
        store::ApprovalStoreError::EvalScopeRequired => ActionError::new(
            403,
            "eval_signoff_scope_required",
            format!(
                "{action:?}: deciding an eval sign-off requires the evals:approve scope, \
                 not approvals:write alone"
            ),
        ),
        store::ApprovalStoreError::Database(error) => store_error(error, action),
    }
}

/// [`store_error`] for the andon writes.
pub fn andon_error(error: store::AndonStoreError, action: &str) -> ActionError {
    match error {
        store::AndonStoreError::Parent(parent) => parent_error(parent, action),
        store::AndonStoreError::Database(error) => store_error(error, action),
    }
}

/// A 422 on one field.
pub fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// The `ETag` of a row whose `version` column was read as `i64`.
pub fn etag_of(raw: i64) -> String {
    let version = Version::new(u64::try_from(raw).unwrap_or(1)).unwrap_or(Version::INITIAL);
    version.etag()
}

/// The precondition a claim requires (CONTRACT §3): `cards.claim` replaces the card's claim
/// state, so it needs `If-Match`; absence is a `412`, never a blind claim.
pub fn require_precondition_with(
    ctx: &HandlerCtx,
    action: &str,
    current: serde_json::Value,
) -> Result<Version, ActionError> {
    match ctx.version {
        // A version is a positive integer. Coercing `0` or a negative value to `1` would let
        // `If-Match: "-1"` satisfy the precondition on every row still at version 1 — a bogus
        // precondition would GRANT a write instead of refusing it. A malformed value is the
        // same refusal as a missing one: `412` with the current representation.
        Some(raw) if raw > 0 => Ok(Version::new(raw as u64).unwrap_or(Version::INITIAL)),
        Some(raw) => Err(ActionError::precondition_with(
            current,
            format!("{action}: If-Match must be a positive version number, got {raw}"),
        )),
        None => Err(ActionError::precondition_with(
            current,
            format!("{action} replaces an existing row: If-Match (or `_version`) is required"),
        )),
    }
}

/// Parse a run status from the wire (`runs.list`'s filter).
pub fn parse_run_status(raw: &str) -> Result<conveyor_domain::RunStatus, ActionError> {
    conveyor_domain::RunStatus::from_str(raw).map_err(|_| {
        invalid(
            "status",
            "invalid_value",
            "must be queued, running, waiting_approval, succeeded, failed or cancelled",
        )
    })
}
