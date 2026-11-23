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

## Measured at the branch head (a9381a76)

| Suite | Result |
|---|---|
| `crates/domain/tests/prompt_request.rs` | **4 passed** (the document renders; a synthetic step declares none; the request is in the hash; an over-budget request refuses with the measurement) |
| `crates/store/tests/prompt_catalogue.rs` | **6 passed** (the 5 existing + `only_a_process_step_declares_a_request`) |
| `crates/actions/tests/process_agent_resolved.rs` | **6 passed** (the extended first-step test and `the_judge_state_carries_the_step_request`) |
| `crates/actions/tests/internal_agent_runtime.rs` | **18 passed, 2 ignored** (the ignored live model/laya proofs are #599's, unchanged) |
| `served-spec.lock.json` | no action, scope or route changed; `--check-lock` runs in the PR's `image` job |
| preflight | run 45258289874 on `26d8e513` **success** (fmt + clippy `-D warnings`, all targets) |

The two new run-level tests failed once on their first CI run and were fixed on this branch: the
post-run recompile now uses the binding's release anatomy (the same pins the executor used) and
clears the run's own terminal attempt memory row first (the step-6 technique), and the release
anatomy fixture carries `job_key` — the parser reads it from the job definition, not the job row.

## Reversal proofs

Each guard was disabled, its test turned red, then restored and re-run green.

| Guard removed | Test that turned red |
|---|---|
| A — the resolved input's render (`StepFacts::request`) | domain `the_declared_step_renders_as_one_request_document`, `the_request_is_in_the_hash` |
| B — the declared outputs' read (`read_step`) | store `only_a_process_step_declares_a_request` (the declared row's outputs) |
| C — the `process_step_id` gate on the input (the synthetic envelope rendered) | store `only_a_process_step_declares_a_request` (the synthetic row's `input.is_none()`) |
| D — the judge's request document (`Some(&request_document)` → `Some(&row.input)`) | actions `the_judge_state_carries_the_step_request` |

## Generated

Regenerated at the merge point: the document pack (`check-docs` 12/12), the architecture pack's
embedded diagram source (`design-pack` green), and the programme seed/coverage checks (current). No
action, scope, route or migration changed; the seed's S21 `issues` list gains #624 when the
automated issue-registry refresh adds it to `docs/design/data/issues.json`.

## Residuals

- A routinely large resolved input can make the process section refuse. The measured refusal is the
  contract; the remedy is a larger `budget_chars` or a shorter step input, never a silent clip.
- A workflow node without a `process_step_id` keeps today's rendering; its declaration is a
  different authoring surface and is a later unit.
- No migration: `run_step.input` is `0015`'s, `process_step.outputs` is `0032`'s, `action_text` is
  `0058`'s, and `gate_kind`/`on_result` are `0035`'s.
