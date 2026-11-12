#![forbid(unsafe_code)]
//! Andon actions: `andon.engage`, `andon.release` (S06 §5.2, §5.5).
//!
//! The state is a per-tenant singleton with no id, so both actions are status-guarded rather
//! than `If-Match` (CONTRACT §3); each writes its `andon.engaged` / `andon.released` event on
//! the tenant-wide topic in the same transaction. A cord already in the requested state is a
//! `409` carrying the current state, so a second pull cannot silently re-stamp `engaged_at`
//! and a release of a released cord cannot announce a change that did not happen.
//!
//! The gauge is NOT request-driven: it is `store::andon::gauge_snapshot` on the scrape path
//! (`crates/api/src/metrics.rs`, S06 decision 5).

use conveyor_store as store;
use sqlx::PgConnection;

use crate::board::events::{self, RawEvent};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::EmptyIn;

use super::views::{AndonEngageIn, AndonOut};
use super::{andon_error, store_error};

/// `andon.engaged` (S06 §5.6) — the cord was pulled.
pub const ANDON_ENGAGED: &str = "andon.engaged";
/// `andon.released` (S06 §5.6) — the cord was released.
pub const ANDON_RELEASED: &str = "andon.released";

/// Pull the cord.
///
/// `runs.create` refuses `409 andon-engaged` while this holds and the worker claims nothing
/// for the tenant except the two kinds S06 §5.5 exempts; both read the row, never a cache.
#[crate::action(
    name = "andon.engage",
    version = 1,
    kind = "internal",
    scopes = ["andon:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn andon_engage(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AndonEngageIn,
) -> Result<AndonOut, ActionError> {
    if let Some(current) = store::andon::get_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "andon.engage"))?
        && current.engaged
    {
        return Err(conflict_with(
            &current,
            "the cord is already engaged; release it before engaging again",
        ));
    }

    let state = store::andon::engage_tx(&mut *tx, ctx.tenant, Some(ctx.actor_id), &input.reason)
        .await
        .map_err(|error| andon_error(error, "andon.engage"))?;
    emit(
        &mut *tx,
        &ctx,
        ANDON_ENGAGED,
        serde_json::json!({ "reason": state.reason, "actor_id": ctx.actor_id }),
    )
    .await?;
    Ok(andon_out(&state))
}

/// Release the cord.
///
/// Releasing a cord that is not engaged is a `409` with the current state: there is no change
/// to announce, and a released event for a released cord would make `event` history lie.
#[crate::action(
    name = "andon.release",
    version = 1,
    kind = "internal",
    scopes = ["andon:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn andon_release(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<AndonOut, ActionError> {
    let Some(state) = store::andon::release_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "andon.release"))?
    else {
        // Not engaged — or the tenant never used the cord. The current representation is the
        // absent row's default, never a fabricated version.
        let current = store::andon::get_tx(&mut *tx, ctx.tenant)
            .await
            .map_err(|error| store_error(error, "andon.release"))?;
        return Err(match current {
            Some(state) => conflict_with(&state, "the cord is not engaged"),
            None => not_engaged(),
        });
    };
    emit(
        &mut *tx,
        &ctx,
        ANDON_RELEASED,
        serde_json::json!({ "actor_id": ctx.actor_id }),
    )
    .await?;
    Ok(andon_out(&state))
}

/// A `409` whose problem body carries the tenant's current andon state (CONTRACT §3).
fn conflict_with(state: &store::AndonState, detail: impl Into<String>) -> ActionError {
    let mut error = ActionError::new(409, "conflict", detail);
    error.current = serde_json::to_value(andon_out(state)).ok();
    error
}

/// The `409` for a tenant that has never engaged: the absent row is the default state.
fn not_engaged() -> ActionError {
    let mut error = ActionError::new(409, "conflict", "the cord is not engaged");
    error.current = serde_json::to_value(AndonOut {
        engaged: false,
        engaged_by: None,
        engaged_at: None,
        released_at: None,
        reason: String::new(),
        version: 0,
    })
    .ok();
    error
}

/// Append the andon event on the tenant-wide topic (CONTRACT §7, S06 §5.6).
async fn emit(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &'static str,
    payload: serde_json::Value,
) -> Result<(), ActionError> {
    events::append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: "tenant".to_owned(),
            // A tenant-wide topic has no board; the andon row is the aggregate.
            board_id: None,
            aggregate_type: "andon_state",
            aggregate_id: ctx.tenant.as_uuid(),
            event_type,
            payload,
            data: serde_json::Value::Null,
            prev_data: serde_json::Value::Null,
        },
    )
    .await
}

/// Map the stored singleton onto the wire document.
fn andon_out(state: &store::AndonState) -> AndonOut {
    AndonOut {
        engaged: state.engaged,
        engaged_by: state.engaged_by.map(|id| id.to_string()),
        engaged_at: state.engaged_at,
        released_at: state.released_at,
        reason: state.reason.clone(),
        version: i64::try_from(state.version.get()).unwrap_or(i64::MAX),
    }
}
