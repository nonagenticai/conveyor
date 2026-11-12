#![forbid(unsafe_code)]
//! The portfolio action surface that survives the S00 verdicts: programmes and stakeholders
//! (S08 Part A §5.2).
//!
//! ⚠️ 2026-11-08: `project_capabilities.*`, `deliverables.*` and `registers.*` (14 actions) were deleted with
//! migration 0011; the usage inventory measured `pm_project_capabilities`, `project_deliverables`/
//! `project_milestones` and `pm_resources`/`pm_risks`/`pm_benefits` at zero rows and no consumer in the 348
//! flows. `project_stakeholder` survives on 21 live rows.
//!
//! Every action is one `#[action]` declaration in a family module under this one; the
//! registry, the OpenAPI operation, the MCP tool and the invocation route are projections of
//! it (S04 §5.1). Nothing here speaks SQL directly — the queries live
//! [`conveyor_store::portfolio`] (CONTRACT §1) — and every `project_id` is resolved in the
//! caller's tenant before a write, because a single-column FK does not enforce tenancy
//! (CONTRACT §2).
//!
//! **One event pattern: the action layer emits.** Every create, update and delete here
//! appends exactly one `portfolio.changed` row through [`emit_portfolio_event`], in the
//! mutation's transaction (CONTRACT §7). The store writes no event, so the row and its
//! doorbell still cannot be separated — a caller cannot commit the mutation without the
//! announcement — and the workspace keeps one pattern: actions emit, the store never does.
//! The payload is `{entity, id, op}` where `entity` is the family (`programme`, `stakeholder`)
//! and `op` is `created`, `updated` or `deleted`; the topic is `<aggregate>:<id>`, so a
//! subscriber can watch one row or every row of a table.
//!
//! **Preconditions.** Every `update` and every `delete` is `If-Match`-guarded (CONTRACT §3):
//! the record is read first so the `412` carries the current representation, and a missing,
//! zero, negative or malformed version is the same refusal as a stale one. Reads emit nothing.
//!
//! `programmes.*` is declared here but writes S03's `programme`; it needs no table of its own.
//! **All nine surviving actions — programmes included — are scoped `portfolio:read` /
//! `portfolio:write`**, and `portfolio:write` is granted to `member`/`admin`/`owner`. They are
//! *not* `projects:read`/`projects:write`: `projects:write` is admin-only, which would make
//! `stakeholders.create` admin work — a member owns those.

use conveyor_domain::Version;
use conveyor_store as store;
use conveyor_store::portfolio::PortfolioStoreError;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};

// 2026-11-08: `capabilities`, `deliverables` and `registers` were deleted with migration 0011 — the S00 usage
// inventory measured their tables at zero, so their 14 actions went too. Programmes and stakeholders stay.
pub mod programmes;
pub mod stakeholders;
pub mod views;

pub use views::*;

/// The problem mappers live in `control` and are re-exported rather than copied: one place
/// decides how a store failure, a parent miss or a refused precondition renders, so the
/// portfolio surface refuses exactly like the control plane (CONTRACT §14 — one rule, not two).
pub use crate::control::{etag_of, invalid, parent_error, require_precondition_with, store_error};

/// The one event type every portfolio mutation emits (S08 §5.11).
///
/// Defined in [`conveyor_store::portfolio`] and re-exported here: one spelling, so no action
/// can invent a second one.
pub const PORTFOLIO_CHANGED: &str = store::portfolio::PORTFOLIO_CHANGED;

/// Append one `portfolio.changed` row in the caller's transaction.
///
/// **The action layer is the one emitter.** The store writes no event (S08 Part A §5.11): a
/// mutation's announcement is appended here, in the same transaction as the row it describes,
/// so the row and its `pg_notify` doorbell still commit or roll back together and a caller
/// cannot separate a mutation from its announcement. `entity` is the family the payload names
/// and `aggregate_type` is the table that owns the emitter; the topic is `<aggregate>:<id>`.
///
/// This calls `store::outbox::append` directly and deliberately does **not** go through
/// [`crate::board::events::append_raw_event`], which also fans out webhook deliveries — a
/// portfolio event has never created a delivery and must not start.
pub async fn emit_portfolio_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    entity: &'static str,
    aggregate_type: &'static str,
    id: Uuid,
    op: &'static str,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("{aggregate_type}:{id}"),
        event_type: PORTFOLIO_CHANGED.to_owned(),
        aggregate_type: aggregate_type.to_owned(),
        aggregate_id: id,
        payload: serde_json::json!({ "entity": entity, "id": id, "op": op }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            // The outbox insert is part of the mutation's transaction, so this refusal rolls
            // the row back with it: a portfolio row is never written unannounced.
            tracing::error!(
                %error,
                entity,
                op,
                action = ctx.action,
                "append the portfolio.changed event"
            );
            ActionError::internal()
        })?;
    Ok(())
}

/// The row's `version` as the `i64` an output document carries.
pub fn version_i64(version: Version) -> i64 {
    i64::try_from(version.get()).unwrap_or(i64::MAX)
}

/// A trimmed optional string; absent and whitespace-only are the same "not supplied".
///
/// A blank `phase_code` or `severity` is not a value the table should hold — it is a caller
/// who meant to omit the field.
pub fn trimmed(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A portfolio store failure as the status the caller can act on.
///
/// A malformed cursor is a `422`; a stale version is a `412`, which `update`/`delete` normally
/// build themselves so the problem body carries the row; a missing parent is the `404`/`422`
/// [`parent_error`] decides.
pub fn portfolio_error(error: PortfolioStoreError, action: &str) -> ActionError {
    match error {
        PortfolioStoreError::NotFound => ActionError::resource_not_found("such portfolio row"),
        PortfolioStoreError::VersionMismatch { current } => ActionError::precondition(format!(
            "{action}: the row is at version {}, not the version the caller supplied",
            current.get()
        )),
        PortfolioStoreError::BadCursor => invalid(
            "cursor",
            "invalid_value",
            "not a cursor this service issued",
        ),
        PortfolioStoreError::Parent(error) => parent_error(error, action),
        PortfolioStoreError::Database(error) => store_error(error, action),
    }
}
