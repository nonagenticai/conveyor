# S21 step 11 — the level closes: apply-on-accept, prompt receipts and the recipe budget

Issue #697 · branch `issue/697-s21-execution-loop` · ADR `0019-accepted-draft-and-prompt-receipts`
· CONTRACT §29 · S21 §23 · resolves rework `v1-p77-1`, `v1-p77-2`

**State: planned.** This file is the skeleton the issue's Definition of Done fills at the exact PR
head. Nothing here is claimed until the measured tables carry the head's rows.

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

## Measured suites (exact head `PENDING`)

| Suite | Result |
|---|---|
| `crates/actions/tests/internal_agent_runtime.rs` | PENDING |
| `crates/actions/tests/draft_apply.rs` | PENDING |
| `crates/actions/tests/decision_ladder.rs` | PENDING |
| `crates/actions/tests/capability.rs` | PENDING |
| `crates/actions/tests/agent_run_resolved.rs` | PENDING |
| `crates/actions/tests/prompt_recipes.rs` | PENDING |
| `crates/actions/tests/prompt_compile.rs` | PENDING |
| `crates/actions/tests/registry.rs` | PENDING |
| `crates/actions/tests/parity_http_mcp.rs` | PENDING |
| `clippy -D warnings` / `fmt --check` | PENDING |
| `served-spec.lock.json` `--check-lock` | PENDING |
| design-pack gate | PENDING |
| rework audit `--rework-check v1-p77-1`, `-2` | PENDING |

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P1.1 an accepted draft is applied under `on_accept=apply` | `internal_agent_runtime::an_accepted_draft_is_applied_when_the_policy_says_apply` | PENDING |
| P1.2 `park` leaves the draft unattached | `internal_agent_runtime::an_accepted_draft_parks_by_default` | PENDING |
| P1.3 an unknown `on_accept` is refused at authoring | `decision_ladder::an_unknown_on_accept_value_is_refused` | PENDING |
| P1.4 a cardless accepted run applies nothing | `internal_agent_runtime::a_cardless_run_applies_nothing` | PENDING |
| P2.1 an active agent without a prompt is refused | `capability::an_active_agent_without_a_prompt_is_refused` | PENDING |
| P2.2 activation without a prompt is refused | `capability::activating_an_agent_without_a_prompt_is_refused` | PENDING |
| P2.3 readiness blocks it | `capability::readiness_blocks_an_agent_with_no_prompt` | PENDING |
| P2.4 `agent.run` makes zero calls without compiled text | `agent_run_resolved::a_run_with_no_compiled_prompt_makes_no_call` | PENDING |
| P2.5 every call carries a 64-hex hash | `internal_agent_runtime::every_internal_agent_call_records_a_64_hex_hash` | PENDING |
| P3.1 a recipe budget refuses measured | `prompt_recipes::a_recipe_budget_is_written_and_refuses_measured` | PENDING |
| P3.2 a new version changes the ceiling | `prompt_recipes::a_new_version_changes_the_ceiling` | PENDING |
| P3.3 a zero budget is refused | `prompt_recipes::a_zero_budget_is_refused` | PENDING |

## Reversal proofs

| # | What is reverted | Test that must go red | Result |
|---|---|---|---|
| 1 | the apply call in the accepted arm | P1.1 | PENDING |
| 2 | the `on_accept` validation | P1.3 | PENDING |
| 3 | the domain's `prompt_ref` requirement | P2.1 | PENDING |
| 4 | the execution guard | P2.4 | PENDING |
| 5 | the recipe `budget_chars` write | P3.1 | PENDING |

## Live proof (P5)

PENDING — the two `#[ignore]`d proofs run with the owner-provisioned licensed test caller; the
exact command and the measured `inference_call`, `decision`, `run_step` and document rows are
recorded here, or the exact refusal and its class. A stub is not the proof.

## Residuals

- Automatic apply remains opt-in by policy; a tenant without the declaration keeps the parked draft.
- `v1-p66-1`'s lane-refusal clause is not this unit's.
