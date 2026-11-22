# S21 step 6 — the work package's run: card binding, terminal memory, live proof

**Issue:** #599 · **Branch:** `issue/599-s21-work-package-run` · **Date:** 2026-11-22 ·
**Baseline:** `origin/main@dd1b9b4a` (the #597 merge)

## What was built

- `cards.create` / `cards.update` accept `job_binding_id`; the id must resolve in the tenant and
  be `active` (`422 job_binding_unusable` otherwise); `CardOut` exposes it; `cards.duplicate` leaves
  it unset (an assignment is not content).
- `runs.create` inherits the card's binding when the request omits one; an explicit binding that
  differs is `422 card_binding_mismatch` before the INSERT; a card with no binding keeps the plain
  run path. The pin/stale checks stay in `insert_run_tx`.
- A terminal root run of a card inserts exactly one `work_package_memory` row (`kind=attempt`,
  `source=system`, `run_id` set) through the existing store writer, in the same transaction as the
  terminal status (`record_run_memory` in `crates/store/src/run.rs`); the existing per-card
  `memory.consolidate` job then has real rows. A child run and a card-less run write none.
- The compiled prompt of the next run on the card carries the prior attempt (the memory selector
  reads the hot rows; ids make the selection part of the hash).

## Not this step

The budget. `v1-p75-1`/`v1-w4a-1` are resolved on `origin/issue/526-u5a`
(commit `8f1e5770`, `crates/actions/tests/tool_budget.rs` 14 passed, `422 budget_missing` at run
start, an invoke precheck that refuses an unbudgeted capturing call). The token ceiling
(`ReleaseLimits.max_tokens`, recorded-only) is that gate's residual. See ADR 0013.

## Measured at the branch head (2026-11-22)

| Suite | Result |
|---|---|
| `crates/actions/tests/card_binding.rs` (new) | **7 passed** (4 scenarios + 3 support) |
| `crates/store/tests/run_memory.rs` (new) | **4 passed** |
| `internal_agent_runtime.rs` (extended) | **16 passed / 2 ignored** (the live proofs) |
| `board` | **15 passed** |
| `executor_runtime` | **8 passed** |
| `registry` | **7 passed** |
| `agent_run_resolved` | **7 passed** |
| `decision_ladder` | **8 passed** |
| `draft_apply` | **12 passed** |
| server `organisation_gate` / `run_retry` / `memory_consolidate` | **1 / 2 / 4 passed** |
| `clippy -p conveyor-store -p conveyor-actions -p conveyor-server --all-targets -- -D warnings` | **exit 0** |
| `served-spec.lock.json` | **10 operations re-hashed** (the card reads/writes and `runs.create`), `--check-lock` **exit 0** |

## Reversal proofs (each guard disabled, its own test red, then restored)

| Guard removed | Test that turned red |
|---|---|
| the card's binding inherited by `runs.create` | `card_binding::a_cards_binding_reaches_its_run` |
| the conflicting-binding refusal | `card_binding::a_conflicting_binding_is_refused` (the run was created 201) |
| the binding's `active` status check | `card_binding::an_unusable_binding_is_refused_on_a_card` (the draft was assigned) |
| the terminal memory insert | `run_memory::a_succeeded_root_run_records_one_attempt` (zero rows) |
| the root-only filter | `run_memory::only_root_card_runs_record_memory` (the child wrote a row) |

## Generated

- `served-spec.lock.json` — `--print-openapi --write-lock`, then `--check-lock` exit 0.
- `build-programme-seed.py --check` and `build-coverage.py --check` / `build-v2-coverage.py --check`
  current. ⚠️ The S21 `issues` list in `seed/programme.json` gains #599 only when the automated
  issue-registry refresh adds it to `docs/design/data/issues.json`; the generator's mapping is
  registry-driven, so hand-adding it makes `--check` fail. The seeded card-binding column and the
  attempt rows need no seed row.
- `docs/design/docs/**` regenerated; `scripts/ci/board.py --dry-run` clean (recorded at the merge
  point).

## Live proof

Two `#[ignore]`, env-gated tests in `internal_agent_runtime`:

- `the_live_model_executes_the_compiled_prompt` — the model leg: the compiled prompt reaches the
  estate's real inference-router, the reply becomes a draft, the terminal run records its attempt.
  Needs `CONVEYOR_EXTERNAL_INFERENCE_URL` and `CONVEYOR_LIVE_MODEL`.
- `the_live_run_executes_against_the_real_router_and_laya` — the full leg: the same, plus one real
  `gate.precheck` decision from the laya bridge. Needs `CONVEYOR_EXTERNAL_LAYA_URL` and
  `CONVEYOR_LIVE_LAYA_TOKEN` too.

**Attempted 2026-11-22 23:15Z — BLOCKED on a caller licence, not on reachability.**

- The router is reachable: a tunnel to `inference-router.inference-router.svc.cluster.local:8150`
  answers `/health` **200** from this box.
- A direct `POST /api/chat/completions` answers **401** with
  `{"detail":"Caller licence could not be introspected; caller is not authorised"}` and
  `x-ir-error-classification: unlicensed`; the executor records that as
  `action-upstream-failed: inference answered 502`.
- The conveyor resolves the bearer from the tenant's sealed `inference.token` (or `laya.token`);
  a throwaway test tenant has neither, and minting a licence for one is a production licensing act
  with no cause (`inference-router-unlicensed-rate-is-rising` warns against exactly that).
- So both live legs stay `#[ignore]`d and unrun. A stub is not the proof; neither is a forged token.

**Unblock:** provision a test tenant's `inference.token` (and `laya.token`) for the estate's router
and laya bridge, then run the two ignored tests with those values in the environment. The commands
are in the tests' doc comments.

## Residuals (recorded, not built)

- The token ceiling (`ReleaseLimits.max_tokens`) remains recorded-only; it belongs with the #526
  budget gate, not a second loop check.
- `v1-p66-1`'s lane-refusal clause ("moving a delivery card into a work lane without a binding is
  refused 422") needs a delivery-card marker that does not exist.
- Auto-start from a claim or lane move; prior-step memory in the prompt; procedure-process run
  memory (`process_runs.start`); `run.execute`'s retry after a handler DB fault (#581 residual).
