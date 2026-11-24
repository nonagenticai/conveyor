# S21 step 9 — the work package's handoff: assign, remember, start

**Issue:** #649 · **Branch:** `issue/649-s21-handoff` · **Date:** 2026-11-24 ·
**Baseline:** `origin/main@a8f67c3e` (the #648 registry refresh)

## What is built

- `cards.assign` — one action, one transaction: it sets the card's active, tenant-scoped job
  binding, records what the receiver must know as one hot `work_package_memory` row, and starts the
  card's bound run through the same path `runs.create` uses (the andon, the binding's pin, the
  attempt, the `run.execute` enqueue and `run.created`).
- One live root run per card (SPEC_V2 §6.6.5): a card with a live run refuses `409
  card_has_live_run` naming it, before any write; `uq_run_live` is the race backstop.
- One run-creation path: `runs.create`'s body is factored into a `pub(crate)` helper and both
  actions call it; `runs.create`'s behaviour and tests are unchanged.
- One memory writer: `memory.remember`'s body is extracted as `remember_tx` and the handoff calls it;
  the note is an ordinary hot row the next run's Memory section selects.
- No migration; no new scope; no new event kind (`card.updated`, `memory.recorded`, `run.created`).

## Not this step

Auto-start on a bare `cards.move` or a claim; a claim/renew/release verb; automatic apply of an
L1-accepted draft; any budget mechanism (the #526 lane's); a handoff of a project-level (card-less)
run; parent-run memory; a typed inputs/acceptance-criteria authoring surface on the handoff; a UI;
PDDL storage; a second memory writer; the live model/laya proof; `v1-p66-1`'s lane-refusal clause.
`cards.update`'s `job_binding_id` remains a plain edit and is not live-run guarded (ADR 0017
decision 7).

## Measured at the branch head

| Suite | Result |
|---|---|
| `crates/actions/tests/card_handoff.rs` (new) | at completion |
| `crates/actions/tests/card_binding.rs` (the #599 regression) | at completion |
| `crates/actions/tests/executor_runtime.rs` (the `runs.create` regression) | at completion |
| `crates/store/tests/prompt_catalogue.rs` (extended) | at completion |
| `crates/actions/tests/registry.rs` | at completion |
| `crates/actions/tests/parity_http_mcp.rs` | at completion |
| `crates/actions/tests/internal_agent_runtime.rs` | at completion |
| `served-spec.lock.json` | at completion |
| `clippy -p conveyor-actions -p conveyor-store -p conveyor-server --all-targets -- -D warnings` | at completion |

## Reversal proofs

| Guard removed | Test that turned red | Reverted |
|---|---|---|
| the handoff's memory insert | at completion | at completion |
| the live-run pre-check | at completion | at completion |
| `ensure_card_binding` | at completion | at completion |
| the andon check in the shared run path | at completion | at completion |
| the note's source from the actor | at completion | at completion |

## Generated

- `served-spec.lock.json` — regenerated (`--print-openapi --write-lock`), then `--check-lock`.
- `docs/design/data/{actions,coverage,programme,issues}.json`, `docs/design/data/rework.js`,
  `docs/evidence/rework-register.json`, `docs/design/docs/**` — regenerated at the merge point.
- `docs/design/architecture/diagrams/run-sequence.{light,dark}.svg` — re-rendered from the updated
  source; `scripts/ci/board.py --dry-run` clean.

## Residuals (recorded, not built)

- Auto-start on a bare lane move or claim remains its own unit (ADR 0014).
- `cards.update` can still change a binding while a run is live; the handoff refuses in that state.
  Whether an edit should be guarded is a card-authoring decision (ADR 0017 decision 7).
- A retry after a committed handoff answers `409 card_has_live_run`; callers reconcile with
  `cards.get`.
