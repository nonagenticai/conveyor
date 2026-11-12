#![forbid(unsafe_code)]
//! The board action surface: projects, boards, lists, cards and their satellites.
//!
//! Every action is one `#[action]` declaration in [`projects`], [`boards`], [`lists`],
//! [`cards`], [`comments`], [`tasks`], [`custom_fields`] or [`labels`]; the registry, the
//! OpenAPI operation, the MCP tool and the invocation route are projections of it
//! (S04 §5.1). Nothing here speaks SQL directly — the queries live in `crates/store`
//! (CONTRACT §1) — and every parent id is resolved inside the caller's tenant before a
//! write, because a single-column FK does not enforce tenancy (CONTRACT §2).
//!
//! Two rules shape this module:
//!
//! * **the list is the state** (CONTRACT §4): a card's lifecycle is `list_id` +
//!   `archived_at`, so archiving resolves the board's `archive` lane by KIND, never by a
//!   caller-supplied list id;
//! * **one write path** (S5-D2): every mutation is an action. The GET routes in
//!   `conveyor_api::board` are reads and write nothing.

use std::str::FromStr;

use conveyor_domain::{ListKind, Version};
use conveyor_store as store;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

pub mod boards;
pub mod cards;
pub mod comments;
pub mod custom_fields;
pub mod events;
pub mod labels;
pub mod lists;
pub mod matrix;
pub mod projects;
pub mod pull;
pub mod read;
pub mod tasks;
pub mod views;
pub mod webhooks;

pub use views::*;

/// A store failure: a unique violation is `409`, everything else a scrubbed `500`.
pub(crate) fn store_error(error: sqlx::Error, action: &str) -> ActionError {
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
pub(crate) fn parent_error(error: store::ParentError, _action: &str) -> ActionError {
    match error {
        store::ParentError::NotFound { .. } => ActionError::resource_not_found("such parent"),
        store::ParentError::Database(error) => store_error(error, _action),
    }
}

/// A webhook-store failure. The signing secret is sealed under the S09 KEK, so a missing key is a named
/// `500` — never a plaintext write, and never a delivery signed with ciphertext.
pub(crate) fn webhook_error(error: store::webhook::WebhookStoreError, action: &str) -> ActionError {
    match error {
        store::webhook::WebhookStoreError::Parent(parent) => parent_error(parent, action),
        store::webhook::WebhookStoreError::Secret(error) => {
            crate::documents::secret_error(error, action)
        }
        store::webhook::WebhookStoreError::Database(error) => store_error(error, action),
    }
}

/// [`store_error`] for the lane queries, whose error type also names the parent and the
/// stored-kind refusal.
pub(crate) fn list_error(error: store::list::ListStoreError, action: &str) -> ActionError {
    match error {
        store::list::ListStoreError::Database(error) => store_error(error, action),
        store::list::ListStoreError::Parent(parent) => parent_error(parent, action),
        store::list::ListStoreError::UnknownKind(raw) => {
            tracing::error!(kind = %raw, "a stored list.kind is outside the five kinds");
            ActionError::internal()
        }
    }
}

/// A 422 on one field.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Parse a `list.kind` from the wire. The five kinds are the only vocabulary.
pub(crate) fn parse_kind(raw: &str, field: &str) -> Result<ListKind, ActionError> {
    ListKind::from_str(raw).map_err(|_| {
        invalid(
            field,
            "invalid_value",
            "must be one of intake, active, waiting, done, archive",
        )
    })
}

/// The version a row was read at, from the `version` column.
pub(crate) fn row_version(raw: i64) -> Version {
    Version::new(u64::try_from(raw).unwrap_or(1)).unwrap_or(Version::INITIAL)
}

/// The `ETag` of a row whose `version` column was read as `i64`.
pub(crate) fn etag_of(raw: i64) -> String {
    row_version(raw).etag()
}

/// The precondition the contract requires for an edit (CONTRACT §3).
///
/// Over HTTP it is `If-Match`; an internal step carries the same value as `_version` and the
/// executor folds both into `ctx.version` (CONTRACT §6). Absence is a `412`, never a blind
/// write.
/// The precondition an edit requires, refusing with the CURRENT representation.
///
/// CONTRACT §3: an edit needs `If-Match`; a refusal — missing or stale — carries the
/// representation the caller should now be looking at, so a client retries without a GET.
pub(crate) fn require_precondition_with(
    ctx: &HandlerCtx,
    action: &str,
    current: serde_json::Value,
) -> Result<Version, ActionError> {
    match ctx.version {
        Some(raw) => Ok(row_version(raw)),
        None => Err(ActionError::precondition_with(
            current,
            format!("{action} replaces an existing row: If-Match (or `_version`) is required"),
        )),
    }
}
