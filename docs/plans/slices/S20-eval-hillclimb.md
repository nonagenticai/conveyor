---
slice: S20
title: Eval hillclimb
status: planned
depends_on: [S19]
spec_sections: ["§3 D26", "§5.7", "§6", "§8 S20"]
adds_tables: [eval_climb]
adds_actions:
  [
    eval_climbs.start,
    eval_climbs.get,
    eval_climbs.list,
    eval_climbs.stop,
    eval_climbs.round,
    eval_climbs.compare,
    eval_runs.regrade,
  ]
adds_events: [eval_climb.changed, eval_climb.round_completed]
adds_modules: [eval]
parity_ids: PENDING-S0   # net-new: this slice ports no old surface, so S0 has no id for it
---

# S20 — Eval hillclimb

Programme project: S20 · issues #24

## 1. Objective

The `eval-hillclimb` and `cost-hillclimb` methods become a loop over one eval: a goal, a stopping
condition, a fixed train/test split, and rounds that each propose a change, run the full set,
and are kept or reverted on the **held-out** number. At the end a person has the winning version
applied, a per-round table with train and test side by side, a paired delta with a significance
test, two before/after transcript pairs, and the failures nobody could fix classified by cause
rather than blamed on the model.

The loop is deliberately not autonomous. Every round returns to the caller, because the method's
check-in cadence is a person's, and because a change whose effect sits inside the noise floor is
kept or reverted by chance — spending a full pass to learn that is how a hillclimb burns a budget
without moving the score.

## 2. Starting state

- S19: `eval`/`eval_version`/`eval_case`/`eval_metric`, the case runner, the four grader kinds,
  `eval_runs` with `variant`, `round`, `change_md`, `change_patch`, `summary`, `narrative_md`,
  the report, the split and its digest, and `eval_runs.report` (mean, CI, noise floor, cost).
- The split is drawn once by S19 and cannot move; `eval_audit` already computes the noise floor
  against the headroom and the smallest shippable delta.
- Nothing spawns an analyzer: S19 has no notion of a proposed change, only of a run.

## 3. Scope

**IN.** One table (§2.20); the six actions, two events and screen **21** declared for S12; the
round protocol (analyze → apply → run → record → keep or revert) as one action per round; the
goal/guardrail model; the stopping conditions; the stall-triage round; the paired comparison with
exact McNemar and its refusal rule; the cost-search mode and its adoption gates; the final report.

**OUT.** Any UI — screen 21 is declared here for S12. A model loop of its own: the analyzer is an
`agent` principal over `/mcp` (D19/D22) or a person writing `change.md` directly — the conveyor
hosts no second runtime. Automatic application of a change to production code: S20 writes
`change_patch` and a snapshot into the round; applying it to a live procedure is an ordinary
`procedures.update` a person makes. Editing the eval, its cases or its metrics mid-climb — that
is a new `eval_version`, and a compare across versions is refused. Re-implementing the benchmark's
McNemar (S17's is the same function; S20 calls it, it does not port it twice).

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `…/claude-api/shared/evals/eval-hillclimb.md` | Steps 0.5–5, §4.5's bucket table, the failure modes | the **wall**, the **stopping rules**, the **bucket vocabulary** and the revert rules verbatim | `.claude/hillclimb/<flow>/` as the state store — `eval_climb` is |
| `…/claude-api/shared/evals/cost-hillclimb.md` | the search order, the adoption gates, the stopping rules | the order (caching health → prompt audit → model × effort → prompt climb → joint confirm) and "gates registered before round 1" | its file-based measurement discipline; `inference_call` measures |
| `…/claude-api/shared/evals/eval-audit.md` §5 | noise floor vs headroom vs smallest shippable delta | the arithmetic and the "say it before round 1" rule | — |
| `crates/domain/src/benchmark/mcnemar.rs` (S17) | exact two-sided McNemar and its refusal | the function, called — not copied | a second implementation |
| `docs/plans/slices/S18-kata-improvement.md` | how a measurement loop consumes an instrument | the "consumer of the public surface" constraint | its benchmark tables |

## 5. Deliverables

⚠️ **Corrected at build time.** `eval_climb` shipped in **`0026`** (S19's migration), so this
slice's only new DDL is `eval_climb.narrative_md` — **`0029`**. The next free number was read from
`ls migrations/` at build time, never assumed.
`migrations/0029_s20_eval_climb_report.sql`. Modules: `crates/domain/src/eval/climb.rs` (the loop state machine,
the keep/revert rule, the bucket vocabulary, the stopping predicate),
`crates/store/src/eval/climb.rs`, `crates/actions/src/eval/climbs.rs`. Plus `spec.rs` (no new
scope, no new table beyond `eval_climb`), the registry `EXPECTED` array, and a
`parity_http_mcp` sample per action.

| action | input | output | scope |
|---|---|---|---|
| `eval_climbs.start` | `{eval_id, goal:{target, direction, hold[]}, stopping: plateau\|rounds\|manual, plateau_k≥3, rounds?, delta, reps, cost_ceiling_cents?, approve_each_round, off_limits[]}` | `ClimbView` (`planned`, awaiting the plan sign-off) | `evals:approve` |
| `eval_climbs.get` / `list` | `{id}` / `{eval_id?, status?, page}` | `ClimbView` / `Page<ClimbView>` | `evals:read` |
| `eval_climbs.round` | `{id, change_md, change_patch?, target, prediction:{metric, direction, interval, falsifier}}` | `RoundView` (`queued`) | `evals:write` |
| `eval_climbs.compare` | `{a_run_id, b_run_id}` | `{comparable, reason?, both, neither, a_only, b_only, discordant, p_value, delta, ci, significant}` | `evals:read` |
| `eval_climbs.stop` | `{id, reason}` | `ClimbView` (`stopped`, winner named) | `evals:approve` |
| `eval_runs.regrade` | `{source_run_id, change_md?}` | new `eval_run` under the eval's **current** version, one `eval.regrade` job per stored slot | `evals:approve` |

`eval_climb` is the method's `_state.json`: `goal jsonb`, `direction`, `reps`, `stopping`,
`plateau_k`, `delta`, `cost_ceiling_cents`, `current_round`, `best_round`, `best_test_score`,
`approve_each_round`, `off_limits text[]`, `status ∈ planned|running|plateaued|stopped`. **Spend is
never a column** — it is `sum` over the climb's `eval_case_result.usage` at read time, because a
maintained counter is the one thing that can drift from the rows it counts.

Screen declared for S12: **#21 Eval hillclimb** (`eval_climbs.*`, `eval_runs.report/get/list`,
`eval_results.list`, `eval_signoffs.*`; SSE `eval_climb.changed`, `eval_climb.round_completed`).
Jobs: **one** new kind, `eval.regrade` (owner ruling 2026-11-09: a grader fix re-grades stored
transcripts into a new run). A *round* still adds no kind — it is an `eval_run` row plus the
existing `eval.run.tick`, so the climb inherits S19's reaper and needs no second driver. Scopes: no new one. **`eval_climbs.start` and `stop` require
`evals:approve`** — they fix the goal, the budget and the stopping condition — while **`round` requires only
`evals:write`**, because the plan was signed once and carrying it out is work, not a second sign-off. That split
is also what lets an `agent` principal propose a round over `/mcp` without being able to set its own budget.

## 6. Build order

**Part A — state and the round.** Steps 1–4. Proves: a two-round climb on a real eval produces a
kept round and a reverted one, with the reason recorded.
**Part B — comparison, triage, cost.** Steps 5–8. Proves: a paired delta recomputes by hand, and a
stalled loop produces a categorisation round instead of a fourth content round.

1. `0027_s20_eval_climb.sql`; touch `crates/store/src/migrate.rs`. `rbuild -- run -p conveyor-server -- migrate`
   and verify `_sqlx_migrations` version 27.
2. `domain::eval::climb` — the loop machine, `keep_or_revert` (train up/test flat ⇒ revert; both up
   ⇒ keep; train down/test up ⇒ keep only if it repeats on a second run; tie ⇒ later round), the
   stopping predicate (plateau K≥3, rounds, manual) at a delta above the noise floor.
   `rbuild -- test -p conveyor-domain eval_climb`.
3. `store::eval::climb` + `eval_climbs.start/get/list/stop`; the plan sign-off
   (`gate='eval_climb_plan'` carrying the goal, the will/won't-change table and the stop condition)
   is required before round 1. `rbuild -- test -p conveyor-actions eval_climb_start`.
4. `eval_climbs.round` — writes `eval_run` (`variant='v<N>'`, `round`, `change_md`, `change_patch`,
   `summary.suspicious`) and `prediction`, then starts the run; `approve_each_round` emits a
   `gate='eval_round'` approval and refuses the run until it is decided.
   `rbuild -- test -p conveyor-actions eval_round`.
5. `eval_climbs.compare` — paired per-case delta, CI, and S17's exact McNemar; **refuse** when the
   two runs name different versions, graders, or splits (`409 not_comparable`).
   `rbuild -- test -p conveyor-actions eval_compare`.
6. **Stall triage** — after two consecutive rounds whose test delta does not clear the band, `round`
   requires a `buckets` payload (`artifact_gap | grader_disagreement | harness | structural |
   variance`, counts, one line each) and writes it to `change_md` in place of a diff; a grader fix
   re-grades stored transcripts under a new `eval_version` and re-runs the comparison.
   `rbuild -- test -p conveyor-actions eval_stall`.
7. **Cost search** — `goal.target='cost_usd'` with `hold:[<quality metric>]`, the guide's search
   order as a state machine on `eval_climb.goal`, adoption gates registered before round 1, and
   the price screen computed from `inference_call`. `rbuild -- test -p conveyor-actions eval_cost_walk`.
8. Final report: rewrite `narrative_md` (status table + the four-part summary), name the winner,
   declare screen 21's read model. `rbuild -- test -p conveyor-actions --test eval_final`.

## 7. Gate

⚠️ **Rust tests, not `psql` probes, and `rbuild`, not `cargo`** — the tests run on the build host against a
throwaway Postgres it migrates itself (CONTRACT §16), so a local `psql` against compose `:5439` observes a
different database. Note that **S20 adds no new job and no new scope**, so this gate has no `--exact` filter on
the climb's own suite: it must pass on the whole `eval_climb` target.

```bash
set -euo pipefail
cd $HOME/Movies/conveyor

# one table RLS-forced, migration 0029 in the ledger, and no spend column anywhere
rbuild -- test -p conveyor-store --test eval_climb_schema -- --exact one_table_rls_forced_and_the_climb_exists
rbuild -- test -p conveyor-store --test eval_climb_schema -- --exact no_climb_spend_column

# no round before the plan is signed; no round runs without its own approval when asked,
# and the train/test split is S19's and is not movable by a climb
rbuild -- test -p conveyor-actions --test eval_climb_gate -- --exact a_climb_cannot_redraw_the_split
rbuild -- test -p conveyor-actions --test eval_climb_gate

# a grader fix re-grades stored transcripts into a NEW run, never in place
rbuild -- test -p conveyor-actions --test eval_regrade

# comparison refuses across versions, graders or splits, and the paired delta reproduces by hand
rbuild -- test -p conveyor-actions --test eval_compare

# the revert rules are the machine's, not a judgement call
rbuild -- test -p conveyor-domain --test eval_climb_rules

# a stalled loop must categorise before another content round
rbuild -- test -p conveyor-actions --test eval_stall -- --exact a_fourth_content_round_is_refused

# the cost walk registers its gates before round 1, and the guardrail is enforced
rbuild -- test -p conveyor-actions --test eval_cost_walk -- --exact no_round_without_registered_gates

# the final report names the winner (this is a conveyor-ACTIONS binary, not conveyor-server)
rbuild -- test -p conveyor-actions --test eval_final

# the served surface and the lock agree
rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock

# fmt --check + clippy -D warnings + the whole suite, once, on the host
rbuild --gate

printf '\nGATE_GREEN\n'
```

## 8. Traps

1. **A split chosen by score.** Hand-picking the worst cases for train guarantees regression to the
   mean and a flat test set. S19 draws it and locks it; a climb that could redraw it has no
   held-out signal at all.
2. **The analyzer reading test.** The wall is data access (`evals:approve`), not an instruction. A
   round whose proposal cites a test transcript is void — record the transcript ids the analyzer
   was handed and refuse the round if any is a test id.
3. **A change too small to see.** If the best case for a change sits inside the noise floor, the
   keep/revert decision is a coin flip. `delta` is set from the measured floor, and a proposed
   change that predicts a smaller gain is refused with the number.
4. **Stacking changes.** One round is one hypothesis, even when its patch touches several places.
   Bundling makes a win unattributable and a regression impossible to bisect.
5. **Trusting an implausible jump.** A judge-graded score that leaps further than the change can
   explain is a gamed judge until hand-checked; record `summary.suspicious` and show the outputs.
6. **Grinding content at a wall.** Three rounds that do not clear the band mean the failure is the
   grader, the harness, the artifact's structure, or variance — not more content.
7. **A maintained spend counter.** It drifts from the rows it counts. Sum the results.
8. **Re-grading without re-comparing.** A rubric fix changes every variant's number; comparing a
   new grade against an old run is two scales. Re-grade every variant in place, then compare, and
   say how many cases moved.

## 9. Decisions closed here

- **A round returns to the caller.** `eval_climbs.round` runs one round and stops, so the check-in
  cadence is a person's by default; `approve_each_round` additionally gates the change itself.
- **The split is not the climb's.** It belongs to S19's version, is drawn once, and a climb that
  cannot redraw it cannot overfit to the number it reports.
- **Comparison reuses S17's McNemar.** One implementation of the exact test in the estate; a second
  would be a silent divergence in a published number.
- **Stall triage is a round, not advice.** After two flat rounds the next round must carry buckets;
  a fourth content round against the same wall is refused.
- **Cost search is a goal, not a mode of the loop.** `goal.target='cost_usd'` with a quality
  guardrail reuses the same rounds and the same comparison; the search order is state on
  `eval_climb.goal`, so a resumed session does not restart at the top.
- **Applying the winner is a person's action.** S20 names the version and ships the patch; it never
  rewrites a published procedure.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Who proposes a change — an agent or a person? | either: `eval_climbs.round` takes `change_md`/`change_patch` from the caller, so an `agent` principal over `/mcp` and a person use the same action |
| 2 | Are test traces retained per round? | yes, under the eval's retention; the wall is the read scope, not deletion — the final report needs before/after pairs |
| 3 | Does a climb run unattended? | only with `approve_each_round=false` **and** a signed plan; the default is one round per call |
| 4 | What happens when the goal plateaus but another metric has headroom? | offer to change the target and run a round pointed at it under the constraint of not regressing what won — ask first, never silently retarget |
| 5 | Is a cost walk's winner promoted automatically? | no — it is named in the report with its guardrail intact; promotion is a person's `procedures.update` |
