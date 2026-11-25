# S21 step 11 — the level closes: apply-on-accept, prompt receipts and the recipe budget

Issue #697 · branch `issue/697-s21-execution-loop` · ADR `0019-accepted-draft-and-prompt-receipts`
· CONTRACT §29 · S21 §23 · resolves rework `v1-p77-1`, `v1-p77-2`

**State: merged** (PR #713, `e7aa67b7`, 2026-11-25). Measured at `8eef79ba` on the branch that
merged with `main@9ace4e81`; the fixture-contract rewrite is `a21c1965` and the migration-ledger pin
`20efce1c`. The required CI contexts were green at the head `28401de2`.

## What changes at this level

Before: an L1-accepted draft was never applied — the executor finished the run `succeeded` and left
the document unattached, so the gate's pass changed no card. An `internal_agent` could be created
and activated with no `prompt_ref`; readiness skipped the check and `agent.run` called a model with
no compiled text and a NULL `prompt_sha256`. The recipe-level `budget_chars` had a reader and a
measured refusal but no writer.

After: the effective `gate.precheck` policy may declare `meta.on_accept = "apply"` (park is the
default), and an accepted decision applies the draft through the one `apply_draft_tx` gate in the
executor's finish transaction. An active `internal_agent` must resolve a prompt at creation,
activation, readiness and execution, and every recorded call carries a 64-hex `prompt_sha256`.
`prompt_recipes.create` writes the recipe's `budget_chars`, so the measured refusal is reachable
from an authored recipe.

## Measured suites (exact head `8eef79ba`)

| Suite | Result |
|---|---|
| `crates/actions/tests/internal_agent_runtime.rs` | **22 passed, 2 ignored** (the live proofs) |
| `crates/actions/tests/draft_apply.rs` | **7 passed** |
| `crates/actions/tests/decision_ladder.rs` | **8 passed** |
| `crates/actions/tests/capability.rs` | **15 passed** |
| `crates/actions/tests/agent_run_resolved.rs` | **16 passed** |
| `crates/actions/tests/prompt_recipes.rs` | **10 passed** |
| `crates/actions/tests/prompt_compile.rs` | **7 passed** |
| `crates/actions/tests/registry.rs` | **12 passed** — 589 actions (565 internal, 24 external) |
| `crates/actions/tests/parity_http_mcp.rs` | **7 passed** — both layer actions sampled over HTTP and MCP |
| `clippy -D warnings` (domain, store, actions, api, server; all targets) / `fmt --check` | **exit 0** |
| `served-spec.lock.json` | **620 operations**, `--check-lock` exit 0 |
| design-pack gate (coverage ×2, seed, docs, data, board dry-run) | **all pass** |
| rework audit `--rework-check v1-p77-1`, `-2` | **each prints "is cleared"** |

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P1.1 an accepted draft is applied under `on_accept=apply` | `internal_agent_runtime::an_accepted_draft_is_applied_when_the_policy_says_apply` | **pass** |
| P1.2 `park` leaves the draft unattached | `internal_agent_runtime::an_accepted_draft_parks_by_default` | **pass** |
| P1.3 an unknown `on_accept` is refused at authoring | `decision_ladder::an_unknown_on_accept_value_is_refused` | **pass** |
| P1.4 a cardless internal_agent run is refused before any call | `internal_agent_runtime::a_cardless_internal_agent_run_is_refused_before_any_call` | **pass** |
| P2.1 an active agent without a prompt is refused | `capability::an_active_agent_without_a_prompt_is_refused` | **pass** |
| P2.2 activation without a prompt is refused | `capability::activating_an_agent_without_a_prompt_is_refused` | **pass** |
| P2.3 readiness blocks it | `capability::readiness_blocks_an_agent_with_no_prompt` | **pass** |
| P2.4 `agent.run` makes zero calls without compiled text | `agent_run_resolved::a_run_with_no_compiled_prompt_makes_no_call` | **pass** |
| P2.5 every call carries a 64-hex hash | `internal_agent_runtime::every_internal_agent_call_records_a_64_hex_hash` | **pass** |
| P3.1 a recipe budget refuses measured | `prompt_recipes::a_recipe_budget_is_written_and_refuses_measured` | **pass** |
| P3.2 a new version changes the ceiling | `prompt_recipes::a_new_version_changes_the_ceiling` | **pass** |
| P3.3 a zero budget is refused | `prompt_recipes::a_zero_budget_is_refused` | **pass** |

## Reversal proofs

| # | What is reverted | Test that must go red | Result |
|---|---|---|---|
| 1 | the apply call in the accepted arm | P1.1 | **red** — the draft stays unattached |
| 2 | the `on_accept` validation | P1.3 | **red** — `on_accept: "later"` is stored |
| 3 | the domain's `prompt_ref` requirement | P2.1 | **red** — an active agent without a prompt is created |
| 4 | the execution guard | P2.4 | **red** — the run succeeds and the model is called |
| 5 | the recipe `budget_chars` write | P3.1 | **red** — the created version carries no ceiling |

## Live proof (P5)

PENDING — the two `#[ignore]`d proofs run with the owner-provisioned licensed test caller; the
exact command and the measured `inference_call`, `decision`, `run_step` and document rows are
recorded here, or the exact refusal and its class. A stub is not the proof.

## Residuals

- Automatic apply remains opt-in by policy; a tenant without the declaration keeps the parked draft.
- `v1-p66-1`'s lane-refusal clause is not this unit's.
