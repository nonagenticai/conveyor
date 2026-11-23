# S21 step 8 — the Request source

**Issue:** #624 · **Branch:** `issue/624-s21-request` · **Date:** 2026-11-23 ·
**Baseline:** `origin/main@29cc3820`

## What this step builds

- One **request document** per process-driven step —
  `{step, action, action_text, input, outputs, on_result, gate}` — rendered in the process (Request)
  section after the current-step line and passed unchanged to `laya.judge` as its case input.
- The store reads the resolved `run_step.input` (persisted at claim) and the frozen
  `process_step.outputs`/`action_text`/`gate_kind` in one read. The compiler reads; it never
  re-resolves the template and never writes.
- A synthetic run-level `internal_agent` `agent.run` step contributes none of its executor envelope;
  its Request is the release definition, as before. The run-level judge now receives that release
  request too.
- The request is inside the compiled text, so it is inside `prompt_sha256`. Whole values; the
  section budget refuses with the measurement, never truncates. **No migration.**

## Not this step

No card-driven run creation, no `memory.remember` write-back at handoff, no automatic apply of an
L1-accepted draft, no budget mechanism (the #526 lane's), no prompt-budget authoring (`v1-p77-2`),
no NULL-hash guard (`v1-p77-1`), no release pinning (`v1-p42-1`), no workflow-node request (a
`run_step` with no `process_step_id`), no parent-run memory in a child's prompt, no UI, no PDDL
storage, no second compiler, no migration.

## Measured at the branch head

_To fill at completion._ The suites: domain `prompt_request` (new), domain `prompt_memory` and
`prompt_compile` (regression), store `prompt_catalogue`, actions `process_agent_resolved`,
`internal_agent_runtime`, `process_step`, and `--check-lock`.

| Suite | Result |
|---|---|

## Reversal proofs

_To fill at completion._ Each guard is disabled, its test turns red, then restored and re-run green:
the input render, the outputs read, the synthetic-envelope exclusion, and the judge's case input.

| Guard removed | Test that turned red |
|---|---|

## Generated

_To fill at completion._ The document pack, the served-spec lock (`--check-lock`), the programme
seed and both coverage checks. No action, scope, route or migration changed; the seed's S21 `issues`
list gains #624 when the automated issue-registry refresh adds it to
`docs/design/data/issues.json`.

## Residuals

- A routinely large resolved input can make the process section refuse. The measured refusal is the
  contract; the remedy is a larger `budget_chars` or a shorter step input, never a silent clip.
- A workflow node without a `process_step_id` keeps today's rendering; its declaration is a
  different authoring surface and is a later unit.
- No migration: `run_step.input` is `0015`'s, `process_step.outputs` is `0032`'s, `action_text` is
  `0058`'s, and `gate_kind`/`on_result` are `0035`'s.
