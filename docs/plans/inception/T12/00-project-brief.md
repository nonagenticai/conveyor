# T12 — One organisation loop

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T12 — One organisation loop · issue #88 · live document `01a1e008-2e2f-71fa-8714-2c4802220b43`
- **Rationale:** state the inception basis for T12 — the acceptance of V2: one tenant runs the whole loop, audited and within budget, proved by a harness rather than a new production aggregate.
- **Published document:** `01a1e008-2e2f-71fa-8714-2c4802220b43` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** outcome, boundary, reuse sources, inception units, dependencies, acceptance, capabilities, plan, risks, links.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §12 T12 and §9.4, and P0 §4; realized by #88; reuse sources below; depends on every T slice and P0.
- **Links:** issue #88; `docs/SPEC_V2.md` §12; `01-inception-analysis.md`; `02-architecture-design.md`; `seed/programme.json`.

## Outcome

One tenant runs request → PM → delivery jobs → coach interruption → kata → development project → deploy → operations incident → lesson, audited and within budget.

## Boundary

In scope: the end-to-end acceptance harness — one test-crate family under `apps/server/tests/` that drives the registered actions through every loop stage and asserts the audit row, the budget refusal and the benefit attribution.

Out of scope: any production table, action, module or migration; a loop aggregate; a second audit writer; a coach, kata or development implementation inside the test; an HTTP client that bypasses the action registry. Every production stage stays with its own slice; T12 drives it, never forks it.

## Reuse sources

- `crates/store/src/audit.rs:177` — the one audit writer; every loop transition must land a row.
- `migrations/0044_plan_spine.sql:28` + `crates/actions/src/spine/mod.rs:1889` — WBS, baselines and benefits rows.
- `migrations/0046_decision_ladder.sql:17` — the decision shape; every loop decision is an L0/L1/L3 row with pinned versions.
- `crates/actions/src/pm/mod.rs:256` — the PM family; `pm.plan` cards name a binding, inputs and acceptance.
- `crates/actions/src/board/pull.rs:149` — the coach wiring and the seeded process.
- `migrations/0041_runner.sql:21` — run evidence; the artefact chain the loop reads.
- `migrations/0024_s16_inference_audit.sql:10` — the call ledger; cost and budget at call grain.
- `crates/store/src/eval/runs.rs:2` + `migrations/0034_s18_kata_arm.sql:17` — held-out instruments; the kata and the benefit delta.
- `apps/server/tests/organisation_gate.rs:1` — the wire-gate fixture pattern, reused unchanged.
- `docs/design/data/actions.json:270` — the inventory; it lacks four loop families, so it is a source, not the plan.
- Estate capabilities, by role: coding fleet, knowledge ingest/retrieval, inference router, laya, the cluster.

## Inception units

1. `inception-analysis` — `01-inception-analysis.md`; live `01a1ee5f-b75b-76ee-93f3-62903c1661ba` (published 2026-11-21).
2. `architecture-design` — `02-architecture-design.md`; live `01a1ee5f-c33a-70bd-8cb2-a8ca3b379634` (published 2026-11-21).

Both are published (owner publication, 2026-11-21); the inception gate is met.

## Dependencies

Typed, from `seed/programme.json`: blocked-by every T slice (T0–T11, T13–T17) and P0; related #88 (delivery). The live halves of P6/P7 wait on the deployment slice #8 (S14), not #18 (closed). The delivery unit is `status:blocked` because tranche 13 is not authorised.

## Acceptance

The project gate: one tenant runs request → PM → delivery jobs → coach interruption → kata → development project → deploy → operations incident → lesson, all audited, within budget; one benefit is baselined before its transition and measured after it, with the delta attributable to the operational area.

Verify: the harness scenarios at #88 (`apps/server/tests/`, to create there); the live half names #8. At inception, the acceptance is the published analysis+design pair, not yet satisfied.

## Capabilities

`program_management` — owns the loop's acceptance; no slice may fork the harness.

## Plan

`docs/SPEC_V2.md` §12 row T12 and §9.4. Delivery unit #88; no separate `S##` slice file. Target tranche **13 — not authorised**.

## Risks

- A blocker is unmerged, so the harness cannot compile → check per part before writing: the expected action names and the loop-test listing.
- The audit primitive is mistaken for coverage → P9 must count `audit_event` per transition and fail on a missing row.
- No conveyor-side budget refusal → P10 fails, not passes; check `git grep -n budget_exceeded -- crates apps/server`.
- P6/P7 live halves wait on the deployment slice #8 (S14); #18 is closed and must not be cited as the blocker.
- Benefit attribution degrades to prose → check that "operational area" is a `benefit_profile` field and `benefits.transition_upsert` stores both measurements.

## Links

- Issue #88 (delivery; blocked).
- `docs/SPEC_V2.md` §12 T12, §9.4.
- `seed/programme.json` row T12.
- `01-inception-analysis.md`, `02-architecture-design.md`.
