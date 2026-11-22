# S21 step 4 — the `internal_agent` executor runtime and its tool grant

**Issue:** #581 · **Branch:** `issue/581-s21-internal-agent-runtime` · **Date:** 2026-11-22 ·
**Baseline:** `origin/main@6c1c47b5` (the #15 merge)

## What was built

- `crates/actions/src/executor/` — `execute_run` (the one dispatch point), `ExecutorRuntime`
  (kind → `RunExecutor`), the `internal_agent` handler (release resolution, readiness, one
  `run_step`, the one compiler, the one loop, the draft `document`, the laya pre-check, the
  terminal records) and the `procedure` refusal.
- `crates/domain/src/organisation.rs` — `ReleaseAnatomy`, `ReleaseGrant`, `SideEffect`,
  `ReleaseLimits` and the effective-pin helpers; `job_releases.publish` refuses a malformed
  anatomy before the hash, and `job_bindings.create` refuses a kind the release disagrees with.
- `crates/actions/src/workflow/agent.rs` — release pins and the grant filter in the bound path;
  `run_release_step`; `AgentRunOut` carries the compiled identity.
- `crates/actions/src/prompt/mod.rs` + `crates/store/src/prompt.rs` — the pins-aware compile and
  the synthesized Request source for a plain run.
- `crates/store/src/run.rs` — `RunContext` carries the binding pin; `bind_executor_tx` fills the
  run's capability/agent from the binding.
- `crates/store/src/workflow/run.rs` — `NewWorkflowStep.node_id: Option<String>` (the column is
  already nullable; no migration).
- `apps/server/src/worker.rs` — `run_execute` delegates to the executor.
- `seed/organisation.json` — typed grants and limits; the seeded jobs pin `agent@1`.
- Artifacts: S21 §14/§15, ADR 0010, CONTRACT §17 fix + §22, 02-DATA-ARCHITECTURE §2.22–§2.24,
  00-INDEX row, the architecture diagram caption, this receipt.

## Measured at the branch head

| Suite | Result |
|---|---|
| `executor_runtime` (new) | **5/5** |
| `internal_agent_runtime` (new) | **9/9** |
| `agent_run_resolved` | **15/15** |
| `prompt_compile` | **7/7** |
| `process_agent_resolved` | **4/4** |
| `decision_ladder` | **7/7** |
| `registry` | **12/12** |
| store `prompt_catalogue` | **4/4** |
| store `organisation` | **9/9** |
| domain `organisation` (unit) | **12/12** |
| server `run_retry` | **4/4** |
| server `andon` | **3/3** |

## Reversal proofs

| Change disabled | Test that turns red |
|---|---|
| the binding status re-check in `execute_run` | `a_retired_binding_refuses_before_any_model_call` (the run would proceed) |
| the grant filter in `resolve_bound` | `a_binding_never_widens_the_release_grant` (a non-granted tool is offered) |
| the grant-ceiling check | `a_granted_tool_above_the_grant_ceiling_is_not_offered` |
| the grant readiness check | `a_grant_op_with_no_binding_refuses_before_a_model_call` |
| the `run_step` insert | `a_bound_run_generates_a_draft` (no trace, and the loop has no step id) |
| the synthesized Request source | `a_bound_run_generates_a_draft` (`capability_not_ready` on `process`) |
| the decision record | `the_gate_records_a_laya_decision` (no `decision` row) |
| the outage branch | `an_unavailable_judge_fails_closed` (a counterfactual decision would be recorded) |

## Generated artifacts

- `docs/design/data/actions.json` — regenerated; **no new action** (508), but `agent.run`'s output
  schema names the compiled identity, so its operation changes.
- `docs/design/data/coverage.json` — S21 is built with **4/4** modules (the `executor` module now
  exists in `crates/actions`).
- `seed/programme.json` — S21 links #15 and #581.
- `docs/design/docs/**` — regenerated; only the edited documents move.
- `served-spec.lock.json` — **539 operations, one hash changed for `agent.run`**. The refresh also
  absorbed **four already-stale hashes on main** (`laya.judge`, `media.generate_image`,
  `media.transcribe`, `produce.speech`) left by #573's shared-host timeout change, which merged
  after #574's branch was cut and whose merge-queue run skips the image build. The new root-lock
  gate in the `image` smoke would have caught it on the next PR; this PR is the first to run it
  and carries the corrected lock.

## Residuals (recorded, not built)

- A run-resume worker kind for a `handover` gate — **built in S21 step 5 (#590)**: the run parks in
  `waiting_approval`, the person's approval records the bound L3 decision, and `run.resume` applies
  the draft. See `docs/evidence/S21-APPLY.md`.
- Mid-loop token/cost budget refusal (§7.5): the caps are validated and recorded, not enforced.
- A `tool_binding` provisioning action: tests insert raw rows; a live job needs an operator row.
- Broker-side grant enforcement for a direct `tools.call` caller (the agent surface is bounded; a
  direct caller is governed by the binding ceiling).

## Found by the PR's CI

The first suite run failed `cargo audit`, not a test: **RUSTSEC-2026-0592** (a stack overflow in
`Url::origin()` on nested `blob:` URLs, published 2026-11-20) against `url` **2.5.8**, which was
already in the lock on `main`. The fix is the advisory's: `url` **2.5.9** and its
`form_urlencoded` **1.2.3** dependency, and nothing else in `Cargo.lock` moves
(`cargo metadata --locked` accepts the trimmed lock). The PR is the messenger; the merge closes the
advisory for `main` too.
