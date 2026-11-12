#![forbid(unsafe_code)]
//! S13 §5.8's two computed handlers: `wip.pull` and `err.card`.
//!
//! **`wip.pull`** — the platform job behind S13's re-authored `wip.pull` flow (R346). The old
//! n8n flow selected TODO cards against a WIP limit and moved them to READY. The conveyor's
//! board already carries the cap (`list.wip_limit`) and the move invariants
//! (`domain::board::may_move`), so the selection is the only new code: the **intake** lane's
//! live cards in position order, up to `wip_limit − active_count`. Every move goes through the
//! `cards.move` action — the same matrix check, position arithmetic, event and precondition a
//! person's move uses — so this job cannot invent a transition the board refuses.
//!
//! **`err.card`** — the old error trigger, now a `run.failed` consumer. One `err.card` job per
//! failed run reaches [`err_card`], which moves the card to the board's waiting lane (the old
//! flow's BLOCKED), writes one failure comment and starts the coach as a root run. Every step
//! is a real action invocation with a per-run `Idempotency-Key`, so an at-least-once delivery
//! cannot comment or start the coach twice.
//!
//! Recorded deltas vs. the old flows (`parity/n8n-dispositions.tsv`): the conveyor has no
//! per-card `requires` graph, no structural/closure card-type classes and no `DROPPED` lane;
//! the kata-board exclusion is not ported (a kata project uses its own board). A card the
//! matrix refuses is skipped and counted, never treated as a job failure.

use conveyor_auth::Role;
use conveyor_domain::{ListKind, RunStatus, TenantId};
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::invoke::{ActionError, Authority, InvokeRequest, invoke};
use crate::registry::registry;

use super::{list_error, store_error};

/// What one pull did.
#[derive(Clone, Debug, Default)]
pub struct PullOutcome {
    /// Cards already in the active lane.
    pub busy: i64,
    /// The active lane's cap (`None` = no cap).
    pub limit: Option<i32>,
    /// Cards moved to active.
    pub moved: usize,
    /// Cards the matrix refused (logged, never a job failure).
    pub skipped: usize,
}

/// Pull eligible intake cards into the active lane for one board.
pub async fn wip_pull(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<PullOutcome, ActionError> {
    let action = "wip.pull";
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(|error| {
        tracing::error!(%error, "wip.pull: open the scoped transaction");
        ActionError::internal()
    })?;
    let intake = store::list::list_of_kind_tx(&mut tx, tenant, board_id, ListKind::Intake)
        .await
        .map_err(|error| list_error(error, action))?
        .ok_or_else(|| ActionError::new(409, "no_intake_lane", "the board has no intake lane"))?;
    let active = store::list::list_of_kind_tx(&mut tx, tenant, board_id, ListKind::Active)
        .await
        .map_err(|error| list_error(error, action))?
        .ok_or_else(|| ActionError::new(409, "no_active_lane", "the board has no active lane"))?;
    let busy = store::card::count_live_cards_in_list_tx(&mut tx, tenant, active.list.id)
        .await
        .map_err(|error| store_error(error, action))?;
    let limit = active.list.wip_limit;
    // `None` is the old RM `effective_wip_limit = null`: no limit. A cap that is already met
    // selects nothing, so the job is a no-op rather than an error.
    let slots = match limit {
        None => i64::MAX,
        Some(cap) => (i64::from(cap) - busy).max(0),
    };
    let mut selected = Vec::new();
    if slots > 0 {
        let page = store::card::card_page_in_list_tx(
            &mut tx,
            tenant,
            board_id,
            Some(intake.list.id),
            None,
            slots.min(200),
        )
        .await
        .map_err(|error| store_error(error, action))?;
        selected = page
            .into_iter()
            .map(|record| (record.card.id, record.card.version.get()))
            .collect::<Vec<_>>();
    }
    tx.commit().await.map_err(|error| {
        tracing::error!(%error, "wip.pull: commit the selection");
        ActionError::internal()
    })?;

    let mut outcome = PullOutcome {
        busy,
        limit,
        ..PullOutcome::default()
    };
    for (card_id, version) in selected {
        let request = InvokeRequest {
            action: "cards.move".to_owned(),
            input: json!({
                "id": card_id,
                "list_id": active.list.id,
                "_version": version,
            }),
            precondition: None,
            idempotency_key: None,
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: app_key.to_owned(),
            usage: None,
        };
        match invoke(pool, registry(), tenant, request).await {
            Ok(_) => outcome.moved += 1,
            Err(error) => {
                // A matrix refusal is a real answer about that card (e.g. a phase gate);
                // skipping it is correct. A transport failure is the job's problem and the
                // worker maps the status.
                if error.status >= 500 {
                    return Err(error);
                }
                tracing::warn!(%error, %card_id, "wip.pull: the move was refused");
                outcome.skipped += 1;
            }
        }
    }
    Ok(outcome)
}

/// The process `err.card` starts on the failed run's card (S13 §5.8).
///
/// It is a **root** run, not a child: the failed run is terminal, so `uq_run_live`'s root slot
/// is free. The old flow's coach child could not start under the card-wide index; since `0035`
/// the index is root-only and a coach for a dead run has no live parent to hang from.
pub const COACH_ERROR_PROCESS: &str = "cap.kanban_coach.handle_error";

/// The failure comment's marker: the old flow's `BLOCKED/flow_error` line, kept verbatim so a
/// person (and the parity ledger) can still recognise it.
pub const ERR_CARD_COMMENT_MARKER: &str = "BLOCKED/flow_error";

/// What one `err.card` handler did.
#[derive(Clone, Debug, Default)]
pub struct ErrCardOutcome {
    /// The card the failed run belongs to; `None` when the run is gone, not failed, or
    /// project-scoped (no card to move).
    pub card_id: Option<Uuid>,
    /// True when this call moved the card to the waiting lane.
    pub moved: bool,
    /// True when the failure comment exists after this call (written or replayed).
    pub commented: bool,
    /// The live root run occupying the card's slot after this call: the coach this handler
    /// started, or the one that was already there (then `coach_skipped` is true).
    pub coach_run_id: Option<Uuid>,
    /// True when a live root run already held the card, so no coach was started.
    pub coach_skipped: bool,
}

/// Run the `err.card` computed handler for one failed run.
///
/// In order, and each through the real action layer:
///
/// 1. `cards.move` the card to the board's **waiting** lane (the old BLOCKED);
/// 2. `cards.comment` with one sentence naming the failed run;
/// 3. `process_runs.start` the coach ([`COACH_ERROR_PROCESS`]) as a **root** run.
///
/// **At-least-once safe.** Each invoke carries its own per-run `Idempotency-Key`
/// (`err.card:<step>:<run_id>`), so a redelivery replays the recorded outcome instead of
/// writing a second comment or starting a second coach. The move also skips when the card is
/// already in waiting, and the coach skips when the card already holds a live root run — the
/// two effects a replay would otherwise duplicate.
///
/// A matrix refusal of the move is a real answer (not every card type may enter waiting); it is
/// logged and the comment/coach still run. A transport failure is returned so the job retries.
pub async fn err_card(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<ErrCardOutcome, ActionError> {
    let action = "err.card";
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(|error| {
        tracing::error!(%error, "err.card: open the scoped transaction");
        ActionError::internal()
    })?;
    let run = store::run::get_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| match error {
            store::run::RunStoreError::Database(error) => store_error(error, action),
            other => {
                tracing::error!(error = %other, "err.card: read the run");
                ActionError::internal()
            }
        })?;
    let Some(run) = run else {
        tx.commit().await.map_err(|error| {
            tracing::error!(%error, "err.card: close the read transaction");
            ActionError::internal()
        })?;
        tracing::warn!(%run_id, "err.card: the run no longer exists; nothing to do");
        return Ok(ErrCardOutcome::default());
    };
    // Only a failed run has an err.card. A stale payload that names a live or succeeded run
    // must not move a card out from under it.
    if run.status != RunStatus::Failed {
        tx.commit().await.map_err(|error| {
            tracing::error!(%error, "err.card: close the read transaction");
            ActionError::internal()
        })?;
        tracing::info!(
            %run_id,
            status = run.status.as_str(),
            "err.card: the run is not failed; nothing to do"
        );
        return Ok(ErrCardOutcome::default());
    }
    let Some(card_id) = run.card_id else {
        tx.commit().await.map_err(|error| {
            tracing::error!(%error, "err.card: close the read transaction");
            ActionError::internal()
        })?;
        tracing::info!(%run_id, "err.card: the run is project-scoped; no card to block");
        return Ok(ErrCardOutcome::default());
    };
    let Some(card) = store::card::get_card_tx(&mut tx, tenant, card_id)
        .await
        .map_err(|error| store_error(error, action))?
    else {
        tx.commit().await.map_err(|error| {
            tracing::error!(%error, "err.card: close the read transaction");
            ActionError::internal()
        })?;
        // `run_card_fk` makes this unreachable; a run whose card vanished is a data-integrity
        // fault, not a reason to quietly do nothing.
        tracing::error!(%run_id, %card_id, "err.card: the run's card does not exist");
        return Err(ActionError::internal());
    };
    let waiting = store::list::list_of_kind_tx(&mut tx, tenant, card.board_id, ListKind::Waiting)
        .await
        .map_err(|error| list_error(error, action))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "no_waiting_lane",
                "the card's board has no waiting lane to block it in",
            )
        })?;
    // The root slot. In the ordinary case the failed run is terminal, so this is `None`; a
    // redelivery after this handler started the coach sees the coach and skips it.
    let live = store::run::live_run_for_card_tx(&mut tx, tenant, card_id)
        .await
        .map_err(|error| match error {
            store::run::RunStoreError::Database(error) => store_error(error, action),
            other => {
                tracing::error!(error = %other, "err.card: read the card's live run");
                ActionError::internal()
            }
        })?;
    tx.commit().await.map_err(|error| {
        tracing::error!(%error, "err.card: close the read transaction");
        ActionError::internal()
    })?;

    let mut outcome = ErrCardOutcome {
        card_id: Some(card_id),
        coach_run_id: live.as_ref().map(|run| run.id),
        coach_skipped: live.is_some(),
        ..ErrCardOutcome::default()
    };

    // (a) The card enters the waiting lane. A card already there is the replay case: moving it
    // again would be a no-op at best and a matrix refusal at worst.
    if card.list_id != waiting.list.id {
        let mut version = card.version.get();
        for attempt in 0..2 {
            let request = service_request(
                app_key,
                "cards.move",
                json!({
                    "id": card_id,
                    "list_id": waiting.list.id,
                    "_version": version,
                }),
                format!("err.card:move:{run_id}"),
            );
            match invoke(pool, registry(), tenant, request).await {
                Ok(_) => {
                    outcome.moved = true;
                    break;
                }
                // A stale card is a race with a concurrent edit (the message may have changed
                // the version without moving the lane), not a refusal: re-read the version once
                // and try again. Every later step replays on its own key, so this cannot
                // duplicate anything.
                Err(error) if error.status == 412 && attempt == 0 => {
                    tracing::warn!(%run_id, %card_id, "err.card: the card changed; re-reading");
                    match lane_and_version(pool, tenant, card_id).await? {
                        // Someone else already blocked it: the move is done, not ours.
                        Some((list_id, _)) if list_id == waiting.list.id => break,
                        Some((_, fresh)) => version = fresh,
                        None => break,
                    }
                }
                // The matrix is the guard: a card type that may not enter waiting stays where
                // it is. The comment and the coach still record and route the failure.
                Err(error) if error.status < 500 => {
                    tracing::warn!(%error, %run_id, %card_id, "err.card: the move was refused");
                    break;
                }
                Err(error) => return Err(error),
            }
        }
    }

    // (b) One sentence naming the failed run.
    let body = format!("[{ERR_CARD_COMMENT_MARKER}] run {run_id} failed");
    let request = service_request(
        app_key,
        "cards.comment",
        json!({ "id": card_id, "body": body }),
        format!("err.card:comment:{run_id}"),
    );
    invoke(pool, registry(), tenant, request).await?;
    outcome.commented = true;

    // (c) The coach, as a root run. The failed run is terminal, so the root slot is free.
    if !outcome.coach_skipped {
        let request = service_request(
            app_key,
            "process_runs.start",
            json!({
                "card_id": card_id,
                "process_key": COACH_ERROR_PROCESS,
            }),
            format!("err.card:coach:{run_id}"),
        );
        let started = invoke(pool, registry(), tenant, request).await?;
        let coach = started
            .body
            .get("output")
            .and_then(|output| output.get("id"))
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or_else(|| {
                tracing::error!(%run_id, "err.card: process_runs.start returned no run id");
                ActionError::internal()
            })?;
        outcome.coach_run_id = Some(coach);
    }

    Ok(outcome)
}

/// The card's lane and version, read in its own scoped transaction.
///
/// Only the `err.card` retry uses it: after a `412` the handler owes the move a fresh
/// precondition, and these are the two facts that precondition needs (the lane decides whether
/// a retry is still meaningful).
async fn lane_and_version(
    pool: &PgPool,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<(Uuid, u64)>, ActionError> {
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(|error| {
        tracing::error!(%error, "err.card: open the retry read");
        ActionError::internal()
    })?;
    let card = store::card::get_card_tx(&mut tx, tenant, card_id)
        .await
        .map_err(|error| store_error(error, "err.card"))?;
    tx.commit().await.map_err(|error| {
        tracing::error!(%error, "err.card: close the retry read");
        ActionError::internal()
    })?;
    Ok(card.map(|card| (card.list_id, card.version.get())))
}

/// The executor's invocation for one computed step (the same principal a process step uses).
fn service_request(
    app_key: &str,
    action: &str,
    input: Value,
    idempotency_key: String,
) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: Some(idempotency_key),
        actor_id: Uuid::nil(),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: None,
        trace_id: None,
        depth: 0,
        app_key: app_key.to_owned(),
        usage: None,
    }
}
