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

## Measured at the branch head

_Filled at completion._

| Suite | Result |
|---|---|

## Reversal proofs

_Filled at completion._

| Guard removed | Test that turned red |
|---|---|

## Generated

_Filled at completion._

## Residuals

_Filled at completion._
