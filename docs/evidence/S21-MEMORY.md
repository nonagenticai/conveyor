# S21 step 7 — the run's episodic memory

**Issue:** #612 · **Branch:** `issue/612-s21-run-memory` · **Date:** 2026-11-23 ·
**Baseline:** `origin/main@d1bdf569` (the #599 merge)

## What this step builds

- The run's own prior steps render in the one Memory section, ahead of the card's hot rows and the
  digest: `[step/<status>#<position>.<attempt>] <action> <status>: <body>`. Selection is the run's
  own `run_step` rows (`succeeded`/`failed`, `(position, attempt)` before the current step), bounded
  by the selector's `step_limit` (default 8, max 32) with `include_steps` (default true).
- Bodies render whole; an over-budget section refuses with the measured size. No truncation path.
- `memory.recall`'s `hot` excludes `digest`; the digest stays reachable through the prompt
  selector's `include_digest` and the cold read.
- The process run's terminal attempt is proved to reach the next run's prompt (the writer is #599's;
  this step adds no second writer).

## Not this step

No card-driven run creation, no `memory.remember` write-back at handoff, no automatic apply of an
L1-accepted draft, no step `input_template`/`outputs` as Request, no budget mechanism (the #526
lane's), no parent-run memory in a child's prompt, no UI, no PDDL storage, no second compiler, no
migration.

## Measured at the branch head (2026-11-23)

| Suite | Result |
|---|---|
| `crates/domain/tests/prompt_memory.rs` | **8 passed** (5 existing + order, determinism/hash and over-budget refusal) |
| `crates/store/tests/prompt_catalogue.rs` | **5 passed** (prior-step selection, the failed earlier attempt, `include_steps = false`, child isolation) |
| `crates/store/tests/memory.rs` | **4 passed** (the digest-in-hot assertion flipped to two hot rows; the selector still appends exactly one digest) |
| `crates/actions/tests/process_agent_resolved.rs` | **5 passed** (a two-step run's step 2 reads step 1; the next run reads the process attempt) |
| regressions | `internal_agent_runtime` **16** (+2 ignored), `prompt_compile` **7**, `prompt_recipes` **7**, `agent_run_resolved` **15**, `process_step` **20**, `decision_ladder` **7**, `draft_apply` **7**, store `s11_process` **14** |
| `served-spec.lock.json` | `--check-lock` exit **0** (no served schema changed) |

## Reversal proofs

Each guard was disabled, its tests turned red, then restored and re-run green.

| Guard removed | Test that turned red |
|---|---|
| the prior-step render arm | domain `the_runs_prior_steps_render_before_the_card_rows`, `prior_steps_are_deterministic_and_hashed` |
| the prior-step selection in `facts_for_run_tx` | store `the_runs_prior_steps_are_selected_and_isolated`; actions `a_second_step_reads_the_first_steps_output` |
| the `kind <> 'digest'` filter in `hot_for_card_tx` | store `a_digest_covers_the_unconsolidated_rows_once` (its hot assertion) |

## Generated

Regenerated at the merge point: the document pack (`check-docs` 12/12), `served-spec.lock.json`
(`--check-lock` exit 0), the programme seed and both coverage checks. No action, scope, route or
migration changed; the seed's S21 `issues` list gains #612 when the automated issue-registry
refresh adds it to `docs/design/data/issues.json`.

## Residuals

- A routinely large step output can make the Memory section refuse. The measured refusal is the
  contract; the remedy is a larger `budget_chars` or a shorter step output, never a silent clip.
- A parent run's steps never enter a child's prompt (ADR 0015); cross-run memory is a different
  unit.
- No migration: the selector's new members live in the existing `prompt_section.selector` jsonb.
