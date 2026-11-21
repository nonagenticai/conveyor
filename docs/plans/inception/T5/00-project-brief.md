# T5 — Kata

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T5 — Kata · issue #77 · live document `01a1e008-7d05-75d4-ae9d-4e2d485271b4`
- **Rationale:** state the inception basis for T5 Kata: the outcome, the boundary it must not cross, the surfaces it reuses, and how the delivery unit is accepted.
- **Published document:** `01a1e008-7d05-75d4-ae9d-4e2d485271b4` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** outcome, boundary, reuse sources, inception units, dependencies, acceptance, capabilities, plan, risks, links.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §6.3, §9.3–§9.4 and §12 T5, and P0 §4; realized by #77; reuse sources below; depends on T2, S16–S20 and P0.
- **Links:** issue #77; `docs/SPEC_V2.md` §12; `01-inception-analysis.md`; `02-architecture-design.md`; `seed/programme.json`.

## Outcome

A procedure change is measured on a held-out instrument, adopted only when it wins, and reverted when it regresses.

## Boundary

In scope: the kata lifecycle — open on a replayable flow, measure a candidate change on a held-out instrument, adopt on a win, revert on a regression, close with the evidence — built over the existing `project`, `card`, `run`, `approval` and `event` surfaces.

Out of scope: the V1 built slices (not re-opened); the benchmark/eval instrument itself (reused, never redefined or edited by a kata); organisational-memory storage (T8 owns `lessons.propose`; T5 emits candidates); a second runtime, service or action client; any code path outside `crates/`.

## Reuse sources

- `apps/server/src/benchmark_improvement.rs:run` — the comparability rule (controls passed, McNemar p < 0.05); reuse it for eval measurements rather than writing a second verdict.
- `crates/domain/src/eval/climb.rs` — the eval hillclimb; the instrument a kata reads and signs, never defines.
- `migrations/0034_s18_kata_arm.sql:benchmark_run` — `subject_version`, `improvement_verdict ∈ keep|revert|inconclusive`, `improvement_of_run_id`; reuse the verdict vocabulary.
- `crates/store/src/eval/runs.rs` + `eval_signoffs` — the pinned version, split and sign-off a kata must carry.
- `seed/processes/named.yaml:kata.open` — the seeded one-step kata flow, called as a child by the coach.
- `crates/actions/src/control/approvals.rs` + `migrations/0050_approval_policies.sql` — approval rows and the per-action policy; adoption is a human decision.
- `crates/actions/src/pm/mod.rs` — the PM actions that gate a delivery project.
- Estate capabilities, by role: the coding fleet and system-one; an adopted code candidate becomes a development project (T6).
- Upstream: Postgres, sqlx and the workspace crates — every rule stays a constraint or an action in `crates/actions`.

## Inception units

1. `inception-analysis` — `01-inception-analysis.md`; live `01a1ee4b-fbb1-7433-8ad7-2e75199c733f` (published 2026-11-21).
2. `architecture-design` — `02-architecture-design.md`; live `01a1ee4c-04d8-757b-ad3d-1807a55e0ae1` (published 2026-11-21).

Both are published on the project with `meta.category` set (owner publication, 2026-11-21); the inception gate is met. This brief does not authorise the tranche.

## Dependencies

Typed, from `seed/programme.json`: blocked-by T2, S16, S17, S18, S19, S20 and P0; related #77 (delivery). The delivery unit is `status:blocked` because tranche 10 is not authorised, not because a code dependency is unmet.

## Acceptance

The project gate: a procedure change is measured on a held-out instrument; a regression is reverted; self-approval refused; an unmeasurable kata is not opened.

Verify: the gate is exercised by the delivery unit's own scenarios at #77 (`crates/actions/tests/` targets, to create there); at inception the acceptance is the published analysis+design pair, which this draft does not yet satisfy.

## Capabilities

`kata` — the capability that owns the outcome; no other capability may fork the kata lifecycle.

## Plan

`docs/SPEC_V2.md` §6.3, §9.3–§9.4 and §12 row T5. The delivery unit is #77; there is no separate `S##` slice file for T5, so #77's parts are the implementation plan. Target tranche **10 — not authorised**. The plan below is preparation only.

## Risks

- A train-split measurement is adopted → check: an adoption refusal test on `split <> 'test'`.
- The instrument is edited between prediction and measurement → check: pin `eval_version.digest` and `split_sha256`; a mismatch voids the measurement.
- The kata approves its own change → check: the approver principal is compared to the kata run's executor.
- Revert is recorded but nothing restored → check: the base version is read back after `kata.close`.
- A second kata opens on the same signature → check: a partial unique index on the open state.
- `kata_executor` is mistaken for the kata job → check: `crates/auth/src/principal.rs:is_kata_executor` is a machine tenant-admin role, not a job.
- A failed kata releases the andon cord on a timer → check: no release path from `kata.close`.

## Links

- Issue #77 (delivery; blocked).
- `docs/SPEC_V2.md` §6.3, §9.3–§9.4, §12 T5.
- `seed/programme.json` row T5.
- `01-inception-analysis.md`, `02-architecture-design.md`.
