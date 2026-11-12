---
slice: S18
title: Kata continuous improvement
status: planned
depends_on: [S13, S15, S17]
spec_sections: ["§3 D24", "§5.7", "§6", "§8 S18"]
adds_tables: []
adds_actions: [benchmark_runs.trend]
adds_events: []
adds_modules: [benchmark]
parity_ids: PENDING-S0   # net-new: this slice ports no old surface, so S0 has no id for it
---

# S18 — Kata continuous improvement

Programme project: S18 · issues #22, #45

⚠️ **Process model 2026-11-09:** the arm pins the **process** (`process_id`) and the run's **process agent**; the old
`procedure_id` pin migrates in `0033`. Nothing else in this slice changes.

## 1. Objective

The benchmark becomes the kata loop's measuring instrument. A `conveyor`-arm run measures one pinned
`capability_agent` or `procedure` version against the suite; two such runs are compared with S17's exact McNemar;
one of them is promoted as the suite's **baseline**; and the improvement iteration — start a candidate, wait,
compare, keep or revert — is a durable job that invokes exactly the actions a person would. The improvement
screen shows the version-to-version trend and flags a significant regression; it invents no new runtime.

The loop only means something because the comparison is **paired and controlled**. Every candidate is measured on
the same 50 instances, with the same suite hashes, the same grader and the same prompt/oracle pair; the run's
`subject_kind` and id say which executor version produced it; and the promoted baseline is the fixed point every
later run is read against. The old harness measured that two byte-identical baseline runs can differ by four
instances, so a candidate that "wins" by one or two instances has not won anything — the exact test must say
whether the discordant pairs are more than noise, and a run that is not comparable is refused rather than averaged
into a trend. That is the whole reason the benchmark is being pulled into the conveyor: the improvement loop and
the instrument that judges it must not be allowed to drift apart.

## 2. Starting state

- S17: `benchmark_run`/`benchmark_case`, the driver, `compare`, `promote`, the gold/empty/baseline arms, and the
  columns `subject_kind`, `capability_agent_id`, `procedure_id` (forward references, no FK).
- S13: `agent.run` — the bounded internal loop, taking its resolved `capability_agent` in the step input (D22/§17).
- S15: `capability`, `capability_agent`, and the readiness check that refuses an unusable binding before a run.

## 3. Scope

**IN** — the `conveyor` arm's producer (start one conveyor run per instance through the ordinary action surface,
poll for the patch), the version pinning on the run, `benchmark_runs.trend`, the improvement job, the baseline /
regression semantics, and screen 14's data contract.

**OUT** — a new agent runtime or a second model loop (the arm calls S13's `agent.run` exactly as a card run
does); automatic code changes (the loop proposes and measures; a person or the kata's own procedures apply); any
new table or event; a second benchmark driver.

The producer is a **consumer of the public surface**, and that is a constraint, not a style note. It may start a
run, read a run and compare runs; it may not read `benchmark_instance` (the answers), it may not write a case
verdict, and it may not run the instance in a different tenant from the one that owns the run. If the producer
needed a private path, the arm would be measuring a second implementation of the conveyor rather than the one the
product ships — which is exactly the failure this integration exists to prevent.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `crates/actions/src/procedure/step.rs` | how a step invokes an action internally with a resolved template | the invocation path | a second executor |
| `docs/plans/01-CONTRACT.md` §17 | the executor slot, the caps, the readiness rules | the rules | — |
| `slices/S15-capability-knowledge-memory.md` | `capability_agent`, `prompt_ref`, readiness | the shape | S15's build |
| `slices/S17-benchmark-harness.md` | the arms, the compare/refusal, `promote` | the API | the harness build |

## 5. Deliverables

- **The `conveyor` arm producer**: per instance, start a conveyor run through the same public action a person
  uses (a `swebench-fix` procedure or an `agent.run` step carrying the resolved `capability_agent`), wait for its
  terminal state, and take the patch from the run's output. The run row pins `subject_kind` and the id; a missing
  or unready binding is `409 arm_not_ready` before the first case, and scope/prompt/cap failures are recorded as
  `no_attempt`, never as a model failure.
- **`benchmark_runs.trend`** (`benchmarks:read`): `{suite_id, arm?, limit?}` →
  `{baseline_run_id?, points: [{run_id, subject_kind, subject_id, subject_version, started_at, n_graded,
  n_resolved, rate, control_ok, vs_baseline: {discordant, a_only, b_only, p_value, direction}}]}` — computed from
  the stored runs/cases, no new table.
- **The improvement job** `benchmark.improvement.run`: `{suite_id, candidate_subject, baseline_run_id}` → starts a
  run, waits on `benchmark_run.changed`/`get`, compares with the baseline, and records `keep | revert |
  inconclusive` with the McNemar delta. It invokes `benchmark_runs.start`, `benchmark_runs.get`,
  `benchmark_runs.compare` — never SQL and never a private path.
- **Screen 14** declared for S12: version → rate, baseline line, control badges, regression flags, and the
  Start-candidate control (`improvement.spec.ts`).

Tests: `apps/server/tests/benchmark_arm.rs`, `crates/api/tests/benchmark_trend.rs`,
`apps/server/tests/benchmark_improvement.rs` — the gate's three named targets.

## 6. Build order

1. The producer seam: a function that, given an instance and a subject, starts the run through the action layer and
   returns the patch; unit tests for the three failure classes (`no_attempt`, `fail`, typed refusal).
2. `benchmark_runs.trend` — the query and its read model; the regression rule (McNemar p < 0.05 with a negative
   delta); no new table.
3. The improvement job: enqueue, wait, compare, record; a dead job is reaped by S17's `benchmark.reap`, never a
   second state machine.
4. The `conveyor` arm wiring in the S17 driver (the arm value already exists; the producer is the only addition).
5. S12's screen-14 rows and spec; the `--write-lock` delta (one action, one operation).
6. Gate below.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env
cd $HOME/Movies/conveyor

# the arm refuses before S13/S15 data exists, and runs when it does
cargo test -p conveyor-server --test benchmark_arm
# two versions of one subject compare with an exact McNemar delta
cargo test -p conveyor-api --test benchmark_trend
# the improvement job performs the same calls a person does
cargo test -p conveyor-server --test benchmark_improvement
# the workspace is still green and the lock still matches
cargo run -q -p conveyor-server -- --print-openapi --check-lock >/dev/null
cargo test --workspace 2>&1 | tee /tmp/s18-tests.log | grep -E '^test result|FAILED' | tail -5
! grep -q 'FAILED' /tmp/s18-tests.log
cargo clippy --workspace --all-targets -- -D warnings
printf '\nGATE_GREEN\n'
```

The end-to-end proof is one real katas-tenant iteration: a candidate `capability_agent` version, one run, a
comparison against the promoted baseline, and a recorded keep/revert — with the screen showing the same numbers.

## 8. Traps

1. **A benchmark run is not a card run.** The arm starts a conveyor run through the public action surface; do not
   wire a private executor or pass the gold patch into the model's context.
2. **Version pinning is the measurement.** A run without `capability_agent_id`/`procedure_id` measures nothing
   comparable; `subject_kind='suite'` with `arm='conveyor'` is refused by S17's CHECK.
3. **Do not compare across oracle/prompt.** S17's `compare` refuses; the trend must surface the refusal, not
   average over it.
4. **A regression flag is not an automatic revert.** The job records the verdict; the kata's own procedure decides,
   and a person can always read the same comparison.
5. **A dead improvement job must not hold a run.** The run keeps its own lifecycle; the job is replaceable and
   S17's reaper owns the stale case.

## 9. Decisions closed here

1. **The improvement loop is a consumer of the action registry.** It has no private store, no second state machine
   and no privileged path; the UI and the loop call the same actions.
2. **The baseline is per suite and promoted explicitly.** `promote` is a person's (or a procedure's) act, recorded
   on the run; a newer run does not silently become the baseline.
3. **The trend is a query, not a table.** Rates and deltas are derived from `benchmark_run`/`benchmark_case`;
   nothing is rolled up and left to drift.
4. **A trend point is only a run of the same suite and arm.** A `conveyor` run of a different suite, or a
   `baseline` run, is displayed but never plotted against the capability baseline; mixing them is the visual form
   of the comparison S17 refuses.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | May a kata **agent** invoke `benchmark_runs.start` directly? | no — the loop runs as a procedure/job as a service principal; `AGENT_TOOLS` stays unchanged |
| 2 | How many candidate versions may be in flight per suite? | one; a second start while a candidate is `queued/running` is a 409 |
| 3 | What counts as a regression worth flagging? | McNemar p < 0.05 with a negative delta, and both runs `control_ok=true`; otherwise "inconclusive" |
| 4 | Does the improvement job auto-start the next candidate? | no — it records the verdict; the kata procedure chooses the next candidate |
