#![forbid(unsafe_code)]
//! Claim actions: `cards.claim`, `cards.renew`, `cards.release` (S06 §5.2).
//!
//! The claim is one conditional `UPDATE` on the card row (`store::card::claim_card_tx`); renew
//! re-checks the `claim_fence` and never the version (`store::card::renew_card_tx`); release
//! clears the lease only when the supplied fence still matches
//! (`store::card::release_card_tx`). Each writes its `card.claimed` / `card.released` event in
//! the same transaction (CONTRACT §7).
//!
//! Four rules shape the classification of a refused claim:
//!
//! * **The expected version is the request's precondition, never a body field** (CONTRACT §3).
//!   `ClaimIn` carries no version: the executor folds `If-Match` (or `_version`) into
//!   `ctx.version`, and its absence is a `412` carrying the current card.
//! * **The attempt is one statement with no fallback.** A database error is a refusal, never a
//!   grant: a lease that grants itself when the arbiter is down is not a lease.
//! * **A refused claim is classified from a re-read in the same transaction.** A live lease is
//!   a `409` conflict — the card is somebody's work in flight — and registers the claimant as
//!   a waiter. A card that moved with no lease held is a `412` with the current
//!   representation: the caller's read is stale and a re-read is all it needs.
//! * **The waiter is a deduped job, not a table** (S06 §5.3): `job(kind='card.claim',
//!   dedupe_key='card:<id>')`, and the live-dedupe index (`job_live_dedupe_key`, partial on
//!   `status IN ('pending','running')`) collapses N blocked claimants into ONE row.
//!
//! The blocked claim answers `409` as an **outcome**, not as an escaping [`ActionError`]: the
//! waiter row is the durable effect of the refusal and must commit with the response, while
//! the executor rolls the invocation transaction back on an error. The same reason is why the
//! waiter insert is written on this transaction rather than through `store::jobs::enqueue`,
//! which takes a `PgPool` and opens its own transaction; a handler holds only the invocation's
//! `PgConnection`.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_domain::{TenantId, Version};
use conveyor_store as store;
use conveyor_store::card::CardRecord;

use crate::board::events::{BoardEvent, append_event};
use crate::board::views::card_out;
use crate::control::views::{ClaimIn, ClaimOut, ReleaseIn, RenewIn};
use crate::control::{card_error, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// A card was claimed (`cards.claim`, S06 §5.6's register).
pub const CARD_CLAIMED: &str = "card.claimed";
/// A card's claim was released (`cards.release`; `card.lease_sweep` emits
/// `card.lease_expired`).
pub const CARD_RELEASED: &str = "card.released";

/// The waiter job a blocked claimant enqueues (S06 §5.3).
pub const CLAIM_WAITER_KIND: &str = "card.claim";

/// The lease a claim is granted for when `ttl_s` is absent (fifteen minutes — the horizon the
/// store's claim tests exercise).
pub const DEFAULT_CLAIM_TTL_S: i64 = 900;

/// How many times a `card.claim` waiter is retried; the `job` column's own default.
pub const CLAIM_WAITER_MAX_ATTEMPTS: i32 = 5;

/// Claim a card for the caller, gated on the caller's `If-Match`.
///
/// Granted: the claim is written, `card.claimed` is appended in the same transaction and the
/// response is `200` with the new `ETag`. Not granted: see the module doc — a live lease is a
/// `409` that registers a waiter, a card that moved is a `412` with the current card.
#[crate::action(
    name = "cards.claim",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_claim(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClaimIn,
) -> Result<HandlerOutput<ClaimOut>, ActionError> {
    let current = read_card(tx, ctx.tenant, input.card_id).await?;
    let expected = require_precondition_with(&ctx, "cards.claim", card_json(&current))?;
    let ttl_s = input.ttl_s.unwrap_or(DEFAULT_CLAIM_TTL_S);

    match store::card::claim_card_tx(
        &mut *tx,
        ctx.tenant,
        input.card_id,
        ctx.actor_id,
        ttl_s,
        expected,
    )
    .await
    .map_err(|error| card_error(error, "cards.claim"))?
    {
        Some(granted) => {
            // The grant is the store's; re-read it once so the response carries the lease
            // deadline the DATABASE wrote (never a Rust clock's `now() + ttl`).
            let record = read_card(tx, ctx.tenant, input.card_id).await?;
            append_event(
                &mut *tx,
                &ctx,
                &BoardEvent::card(
                    record.card.board_id,
                    record.card.id,
                    CARD_CLAIMED,
                    json!({
                        "card_id": record.card.id,
                        "claim_fence": granted.claim_fence,
                    }),
                ),
            )
            .await?;
            Ok(HandlerOutput::ok(claim_out(&record)).with_etag(granted.version.etag()))
        }
        None => {
            let latest = read_card(tx, ctx.tenant, input.card_id).await?;
            if !live_lease(&latest, Utc::now()) && latest.card.version != expected {
                // The card genuinely moved and nothing is holding it: the caller's
                // representation is stale (CONTRACT §3), so it re-reads and retries.
                return Err(ActionError::precondition_with(
                    card_json(&latest),
                    format!("the card moved to version {}", latest.card.version.get()),
                ));
            }
            // A live lease is the refusal: register the waiter so N blocked claimants
            // collapse into one job row, and answer the conflict as the committed outcome
            // that carries it.
            enqueue_waiter(
                &mut *tx,
                ctx.tenant,
                input.card_id,
                ctx.actor_id,
                expected,
                ttl_s,
                latest.card.lease_expires_at,
            )
            .await?;
            Ok(HandlerOutput {
                value: claim_out(&latest),
                status: 409,
                etag: None,
            })
        }
    }
}

/// Extend the caller's live claim, fenced by the `claim_fence` the claim granted.
///
/// A renew is fenced by the claim fence, never by the version (S06 §8 trap 5): an unrelated
/// `cards.update` bumps the version and must not cost the holder its lease. Zero rows means
/// the fence moved (a new claim) or the lease was released or swept — both are `409`. There
/// is no `card.renewed` event: the register (S06 §5.6) has no such type, and a renew is the
/// same claim held longer.
#[crate::action(
    name = "cards.renew",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_renew(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RenewIn,
) -> Result<HandlerOutput<ClaimOut>, ActionError> {
    store::card::renew_card_tx(
        &mut *tx,
        ctx.tenant,
        input.card_id,
        input.claim_fence,
        DEFAULT_CLAIM_TTL_S,
    )
    .await
    .map_err(|error| card_error(error, "cards.renew"))?
    .ok_or_else(|| {
        ActionError::new(
            409,
            "claim-conflict",
            format!(
                "cards.renew: the card's claim fence is no longer {} (a new claim advanced it, \
                 or the lease was released or swept)",
                input.claim_fence
            ),
        )
    })?;
    let record = read_card(tx, ctx.tenant, input.card_id).await?;
    Ok(HandlerOutput::ok(claim_out(&record)).with_etag(record.card.version.etag()))
}

/// Release the caller's claim.
///
/// Read first, then write: an explicit fence that does not match the card's is `409` (the
/// fence exists to close the ABA window), and an absent fence releases only when the caller is
/// the assignee. The store write is fenced by the current fence either way, so a released
/// card writes nothing. `card.released` carries the fence the holder was granted; the fence
/// itself is never reset, so the next claim keeps increasing it.
#[crate::action(
    name = "cards.release",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn cards_release(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReleaseIn,
) -> Result<HandlerOutput<ClaimOut>, ActionError> {
    let current = read_card(tx, ctx.tenant, input.card_id).await?;
    let fence = match input.claim_fence {
        Some(fence) => {
            if fence != current.card.claim_fence {
                return Err(ActionError::new(
                    409,
                    "claim-conflict",
                    format!(
                        "cards.release: the card's claim fence is {}, not the presented {fence}",
                        current.card.claim_fence
                    ),
                ));
            }
            fence
        }
        None => {
            if current.card.assignee_id != Some(ctx.actor_id) {
                return Err(ActionError::new(
                    409,
                    "claim-conflict",
                    "cards.release: the caller is not the card's assignee",
                ));
            }
            current.card.claim_fence
        }
    };

    let released = store::card::release_card_tx(&mut *tx, ctx.tenant, input.card_id, fence)
        .await
        .map_err(|error| store_error(error, "cards.release"))?;
    if !released {
        // The row is fenced by (claim_fence, lease_token IS NOT NULL): zero rows here means a
        // concurrent release or sweep won after our read, so nothing of ours was written.
        return Err(ActionError::new(
            409,
            "claim-conflict",
            "cards.release: the lease moved beneath the caller",
        ));
    }

    let record = read_card(tx, ctx.tenant, input.card_id).await?;
    append_event(
        &mut *tx,
        &ctx,
        &BoardEvent::card(
            record.card.board_id,
            record.card.id,
            CARD_RELEASED,
            json!({
                "card_id": record.card.id,
                "claim_fence": fence,
            }),
        ),
    )
    .await?;
    Ok(HandlerOutput::ok(claim_out(&record)).with_etag(record.card.version.etag()))
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────

/// Register a blocked claimant as the card's waiter.
///
/// The statement is `store::jobs::enqueue`'s, restricted to the claim waiter: the partial
/// unique index (`job_live_dedupe_key` on `(tenant_id, kind, dedupe_key) WHERE status IN
/// ('pending','running')`) makes the second blocked claimant a no-op, so N blocked claimants
/// collapse into one row without a lock or a read.
async fn enqueue_waiter(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    assignee_id: Uuid,
    expected_version: Version,
    ttl_s: i64,
    run_after: Option<DateTime<Utc>>,
) -> Result<(), ActionError> {
    let payload = json!({
        "card_id": card_id,
        "assignee_id": assignee_id,
        "expected_version": i64::try_from(expected_version.get()).unwrap_or(i64::MAX),
        "ttl_s": ttl_s,
    });
    // `run_after` is the current lease's deadline: a waiter that fires while the lease is
    // still live can only lose, and firing immediately would burn its attempts before the
    // card is free. A card with no lease yet is claimable now.
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, run_after, max_attempts)
         VALUES ($1, $2, $3, $4, coalesce($5, now()), $6)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(CLAIM_WAITER_KIND)
    .bind(format!("card:{card_id}"))
    .bind(&payload)
    .bind(run_after)
    .bind(CLAIM_WAITER_MAX_ATTEMPTS)
    .execute(&mut *conn)
    .await
    .map_err(|error| store_error(error, "cards.claim"))?;
    Ok(())
}

/// Whether the card is under a live lease right now.
///
/// The deadline is compared on the application clock: this is a READ-side classification, and
/// an answer skewing by milliseconds only chooses between two refusals (`409` and `412`),
/// never between a refusal and a grant.
fn live_lease(record: &CardRecord, now: DateTime<Utc>) -> bool {
    record.card.lease_token.is_some()
        && record
            .card
            .lease_expires_at
            .is_some_and(|expires| expires > now)
}

/// The claim state after a write.
fn claim_out(record: &CardRecord) -> ClaimOut {
    ClaimOut {
        claim_fence: record.card.claim_fence,
        version: i64::try_from(record.card.version.get()).unwrap_or(i64::MAX),
        lease_expires_at: record.card.lease_expires_at,
    }
}

/// The `current` representation a `412` carries (CONTRACT §3).
fn card_json(record: &CardRecord) -> Value {
    serde_json::to_value(card_out(record)).unwrap_or(Value::Null)
}

/// One card, resolved in the caller's tenant; a miss is `404`, never an empty claim.
async fn read_card(
    tx: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<CardRecord, ActionError> {
    store::card::get_card_record_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "cards.claim"))?
        .ok_or_else(|| ActionError::resource_not_found("card"))
}
