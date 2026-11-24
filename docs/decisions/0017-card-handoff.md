# 0017 — The card handoff: assign, remember, start

Status: Accepted · Date: 2026-11-24 · Issue: #649 · Resolves: S21 §21, SPEC_V2 §6.6.1–6.6.5, §4.4,
§5.1

## Decision

1. **A handoff is one explicit action, `cards.assign`.** SPEC_V2 §6.6.2 names it; `cards.handoff`
   is rejected as a second name for an assignment. It is not a bare `cards.move` or claim: a lane
   move keeps its board meaning and starts nothing (ADR 0014's rejected alternative stands).

2. **It assigns the binding, records the note and starts the run in one transaction.** The input is
   `{card_id, job_binding_id?, note, kind?, meta?, correlation_id?}`. The target binding is the
   request's, else the card's; neither is `422 card_unassigned`. It must resolve in the tenant and
   be `active` (`422 job_binding_unusable`, the same `ensure_card_binding` `cards.update` uses). The
   note is one hot `work_package_memory` row written through the same writer `memory.remember`
   uses — `kind` defaults to `fact`, `source` comes from the actor kind, `run_id` is set — in the
   same transaction as the card update, the run insert, the `run.execute` enqueue and the events.

3. **The run start is the same path `runs.create` uses.** The run-creation body is factored into one
   `pub(crate)` helper (board read, andon, attempt, `insert_run_tx`, `enqueue_run_execute_tx`,
   `run.created`); `runs.create` keeps its `card_binding_mismatch` resolution and becomes a caller.
   One path, so the handoff cannot drift from the andon, the binding's pin or the event.

4. **One live root run per card (SPEC_V2 §6.6.5).** The handoff reads `live_run_for_card_tx` first
   and refuses `409 card_has_live_run` naming the run before any write; `uq_run_live` remains the
   race backstop — a concurrent loser gets the store's typed 409, never a 500. A re-handoff after a
   terminal run is the card's next attempt.

5. **The note is memory, not a message.** It is a hot row, so the existing selector renders it in
   the next run's Memory section (`[fact/<source>#<id>] …`) and its id is inside `prompt_sha256`.
   There is no separate handoff store and no second writer: `memory.remember`'s body is extracted
   as a shared `remember_tx`, and the handoff calls it.

6. **Events are existing kinds.** `card.updated` (the binding), `memory.recorded` (the note),
   `run.created` (the run). No new vocabulary and no `S05_EVENT_TYPES` change.

7. **`cards.update`'s `job_binding_id` stays a plain edit.** It never starts a run and writes no
   memory; it is not live-run guarded (a caller may reassign a card whose run is live; the run keeps
   its own pin). The handoff is the guarded, run-starting verb; the edit is an authoring act. If the
   owner wants the edit guarded too, that is a card-authoring decision, not this unit.

8. **Refusals are typed and precede the first write:** 404 a foreign card; 409 archived; 422
   `card_unassigned`; 422 `job_binding_unusable`; 409 `card_has_live_run`; 422 `invalid_input` (the
   note); 409 `andon-engaged`.

9. **No migration.** `card.job_binding_id` (`0040`), `work_package_memory` (`0020`), `run`, `job`
   and the outbox (`0002`), and `uq_run_live` (`0035`) all exist.

10. **No new scope.** The action declares the two existing scopes `cards:write` and `runs:write`;
    the macro requires **all** declared scopes, so it cannot be used to reassign work without run
    permission, nor to start compute without card permission.

## Why

Steps 3–8 built the prompt, the executor, the gate, the card's binding, the episodic memory and the
request, but nothing on the board ever starts the bound run: the work-package level is reachable
only by an explicit `runs.create`. SPEC_V2 §6.6 defines the missing protocol in five sentences —
create and assign a card, hand off with an action, record what the receiver must know, one live run
per card — and `memory.remember` has been registered since S15 with zero production callers. The
handoff is the one verb that closes all of it at the level where work actually starts.

## Alternatives rejected

- **Auto-start from a claim or a lane move.** It changes every board's semantics (ADR 0014), and a
  move is not a handoff: the spec's handoff names inputs, acceptance and memory, not a list change.
- **A second action for the memory note.** The handoff would then be two transactions; a crash
  between them loses the note for a handoff that already started work. One transaction is the point.
- **Call `memory.remember` as a nested action.** It declares `capabilities:write` and runs its own
  transaction; the shared `remember_tx` body gives the same one writer inside the handoff's
  transaction without a second scope.
- **A new `cards.handoff` name or a new event kind.** SPEC_V2 §6.6.2 already names `cards.assign`;
  the three existing events say exactly what changed.
- **A handoff that starts no run.** Then the level still has no entry, and the note has nowhere to
  be read.
- **Guard `cards.update` too.** Out of this unit (decision 7); it would change the #599 contract.

## Consequences

- A board member now needs both `cards:write` and `runs:write` to hand off; a card edit alone cannot
  start compute.
- A handoff adds one hot memory row per invocation; the card's consolidation job drains it into the
  digest as for any hot row.
- The served spec gains one operation (`POST /api/actions/cards.assign/invocations`) and
  `GET /api/actions` moves; `served-spec.lock.json` is regenerated.
- `runs.create`'s body moves into a helper; its behaviour, error codes, events and tests are
  unchanged.
- A retry after a committed handoff answers 409 `card_has_live_run` (the run exists); the caller
  reconciles with `cards.get`.
- No new action scope, no new event kind, no migration.
