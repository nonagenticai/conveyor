# S21 step 6 — the work package's run: card binding, terminal memory, live proof

**Issue:** #599 · **Branch:** `issue/599-s21-work-package-run` · **Date:** 2026-11-22 ·
**Baseline:** `origin/main@dd1b9b4a`

> **State: in progress.** The scenario table and measured counts are filled at completion; a
> skeleton row is never evidence.

## What this step changes

- `cards.create` / `cards.update` accept `job_binding_id` (an active tenant binding; `CardOut`
  exposes it); `cards.duplicate` leaves it unset.
- `runs.create` inherits the card's binding when the request omits one; an explicit conflicting
  binding is `422 card_binding_mismatch` before the INSERT.
- A terminal root run of a card inserts one `work_package_memory` row (`kind=attempt`,
  `source=system`, `run_id` set) in the terminal transaction; the existing per-card
  `memory.consolidate` job then has real rows.
- The path is proved live against the real inference-router and laya (an `#[ignore]` test).

## Not this step

The budget. `v1-p75-1`/`v1-w4a-1` are resolved on `origin/issue/526-u5a` (commit `8f1e5770`,
`crates/actions/tests/tool_budget.rs` 14 passed). The token ceiling (`max_tokens`, recorded-only)
is that gate's residual. See ADR 0013.

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P1.1 | `card_binding::a_cards_binding_reaches_its_run` | pending |
| P1.2 | `card_binding::a_conflicting_binding_is_refused` | pending |
| P1.3 | `card_binding::an_unusable_binding_is_refused_on_a_card` | pending |
| P1.4 | `card_binding::card_out_carries_the_binding` | pending |
| P2.1 | `run_memory::a_succeeded_root_run_records_one_attempt` | pending |
| P2.2 | `run_memory::a_failed_run_records_its_class` | pending |
| P2.3 | `run_memory::only_root_card_runs_record_memory` | pending |
| P2.4 | `internal_agent_runtime::the_next_run_prompt_reads_the_prior_attempt` | pending |
| P3.1 | `internal_agent_live` (`--ignored`, real router + laya) | pending |

## Reversal evidence

Each negative scenario names the guard it exercises; the guard is removed and the scenario must
fail. Filled at completion.

## Generated

`docs/design/data/actions.json`, `coverage.json`, `rework.js`/`rework-register.json`,
`docs/design/docs/**`, `served-spec.lock.json`, `seed/programme.json`, the diagram SVGs — regenerated
at the merge point, never hand-merged. Filled with counts at completion.

## Residuals (recorded, not built)

- The token ceiling (`ReleaseLimits.max_tokens`) remains recorded-only; it belongs with the #526
  budget gate, not a second loop check.
- `v1-p66-1`'s lane-refusal clause ("moving a delivery card into a work lane without a binding is
  refused 422") needs a delivery-card marker that does not exist.
- Auto-start from a claim or lane move; prior-step memory in the prompt; procedure-process run
  memory (`process_runs.start`); `run.execute`'s retry after a handler DB fault (#581 residual).
