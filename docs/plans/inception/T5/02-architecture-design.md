# T5 — Architecture design

Project: T5 — Kata · Track: v2 · Tranche: 10 · Plan: docs/SPEC_V2.md §12
Unit: #77 (this document is dependency-shaping work; blocked-by T2, S16–S20 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T2", "S16", "S17", "S18", "S19", "S20", "P0"]

- **Status:** draft
- **Version:** 0.1
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** pending / pending
- **Related project:** T5 — Kata · issue #77 · live document `01a1ee4c-04d8-757b-ad3d-1807a55e0ae1`
- **Rationale:** Define the to-be architecture for Kata: aggregates, tables and actions, decisions, acceptance and the artefact set.
- **Published document:** `01a1ee4c-04d8-757b-ad3d-1807a55e0ae1` — `published_at` null (draft; not published)
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** context and boundary; aggregate/table/action rows; decisions; acceptance scenarios; PSA set; typed artefacts; rollout.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §6.3, §9.3–§9.4, §12 T5 and P0 §5; realized by #77; reuse sources cited below; project dependencies T2, S16–S20, P0.
- **Links:** issue #77; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Context and boundary

T5 is the system-improvement job of §6.3: observe, state the problem with data, choose a candidate, standardise, sustain, propagate. It owns the kata project, the candidate, the measurement contract, the adoption gate, propagation and revert. It is one link of §9.4's single loop, not a parallel loop.

In scope: the `kata` job binding, a kata project type, the kata rows, the `kata.*` actions and events. Out: the instruments (S16–S20), lesson promotion (organisational memory), the development project (T6), the andon cord (S06) and the tool platform (T13). Under D45 the kata is its own tenant; that tenant, its approvals, budget and autonomy policy are absent today (no `kata` tenant row — the analysis records it), and this design proposes them, never inheriting the target tenant's write scope.

## Aggregates, tables and actions

Existing names resolve in `migrations/` and `docs/design/data/actions.json`; proposed names are marked `(to create)` and trace to §12 T5 or a decision below.

| Aggregate | Table | Invariants | Actions |
|---|---|---|---|
| Kata | `project` (existing) + `kata` (to create) | one kata row per kata project; at most one open kata per `(tenant_id, signature)`; instrument and prediction fixed at open | `kata.open`, `kata.observe`, `kata.close` (to create) |
| Candidate | `kata_candidate` (to create) | ordinal monotonic per kata; `target_ref` records tenant, kind, id and `base_version`; a measured candidate is immutable | `kata.analyse`, `kata.propose` (to create) |
| Measurement | `kata_measurement` (to create) | instrument version, split and noise band pinned; baseline and candidate from the same instrument version | `kata.measure` (to create) |
| Adoption | `approval` (existing) | one approval row; `approval_decision.decided_by_kind='human'` (every non-human decision refuses); approver is not the kata executor or assignee; `improvement_verdict` stays the built `keep|revert|inconclusive`; split held out | `kata.adopt` (to create) |
| Propagation | `kata_propagation` (to create) | one row per target; each target carries its own approval; no target inherits the kata tenant's autonomy | `kata.propagate` (to create) |
| Revert | `kata` outcome + `event` (existing) | the recorded `base_version` is restored; the reason is recorded; cord release is never a kata action | `kata.close` (to create) |

A partial unique index on the open states enforces the signature rule; RLS is enabled in the same migration.

Reused actions: `projects.create`, `projects.close`, `cards.create`, `runs.create`, `approvals.request`, `approvals.decide`, `eval_runs.start`, `eval_signoffs.request`, `eval_signoffs.decide`, `benchmark_runs.start`, `benchmark_runs.get`, `benchmark_runs.compare`, `andon.engage`, `andon.release`, `procedures.publish`, `job_releases.publish`. Proposed: the §10.4 family `kata.open`, `kata.observe`, `kata.analyse`, `kata.propose`, `kata.measure`, `kata.adopt`, `kata.propagate`, `kata.close` (to create). Events `kata.opened`, `kata.candidate_proposed`, `kata.measured`, `kata.adopted`, `kata.reverted`, `kata.propagated`, `kata.closed` (to create) travel on the existing outbox. The seeded process key `kata.open` (built) and the proposed action `kata.open` are different registries; the existing catalogue tests assert only the process key (`apps/server/tests/catalogue_install.rs`), and a test that both resolve is to create at #77.

## Decisions

**D-T5-1 — Kata is a project type, not a workflow engine.** A `project` with `project_type_code='kata_v2'`, cards, runs and approvals. Alternative: a dedicated kata service. Consequence: no second runtime; it inherits T1's project types.

**D-T5-2 — The measurement is an existing instrument.** `kata.measure` calls `eval_runs.start` or `benchmark_runs.start` and records the run id. Alternative: a kata-local harness. Consequence: frozen versions and sign-offs apply; a suite that cannot express the change blocks the kata.

**D-T5-3 — Held out means the instrument's locked split.** Adoption requires `split='test'` on a version whose `split_sha256` is locked and unchanged since prediction. Alternative: a separate holdout store. Consequence: the kata never calls `evals.draw_split`; a re-drawn split voids the measurement.

**D-T5-4 — Adoption is a human approval.** `kata.adopt` opens an `approval` and may not decide it; the decision refuses a non-user principal, reusing the `signoff_requires_human` pattern. Alternative: auto-adopt on a metric win. Consequence: adoption waits for a person.

**D-T5-5 — Revert is an explicit outcome.** `kata.close{outcome:'reverted'}` re-publishes `base_version` through the target's own action and writes `kata.reverted`. Alternative: record the verdict and leave the change live. Consequence: the target's gate runs on the restore.

**D-T5-6 — The kata tenant references a target, never writes across tenants.** A pinned `(target_tenant_id, kind, id, base_version)`; every target write is the target tenant's publish action under its own gate. Alternative: a cross-tenant service account. Consequence: the read grant is a tenant-configurable policy row.

**D-T5-7 — One open kata per signature.** A partial unique index plus a `kata.open` refusal (`409 kata_signature_open`); `kata_wip` stays a WIP limit. Alternative: rely on the WIP limit. Consequence: duplicate katas are structurally impossible.

**D-T5-8 — The published seed row is not edited.** A new code `kata_v2` (to create) is seeded and V1 `kata` retired for new projects. Alternative: edit `seed/catalogue.json:kata` in place. Consequence: the seed gate stays green.

## Acceptance

Gate, verbatim from §12 T5: "a procedure change is measured on a held-out instrument; a regression is reverted; self-approval refused; an unmeasurable kata is not opened".

1. **Held-out measurement.** Given a kata changing a procedure version, when `kata.measure` runs, then `kata_measurement` records the pinned instrument version and `split='test'`; `kata.adopt` refuses a `train` or `none` split with `422 kata_measurement_not_held_out`. Verify: `crates/actions/tests/kata_measure.rs::a_procedure_change_is_measured_on_the_held_out_split` (to create at #77).
2. **Regression reverted.** Given `improvement_verdict='revert'`, when the kata closes with `outcome='reverted'`, then `base_version` is published again and `kata.reverted` is written; a close without a restore refuses `422 kata_revert_missing`. Verify: (to create at crates/actions/tests/kata_revert.rs).
3. **Self-approval refused.** Given an adoption approval, when the kata's executor or a machine principal calls `approvals.decide`, then the decision refuses `403 kata_self_approval`. Verify: `crates/actions/tests/kata_adopt.rs::the_kata_cannot_approve_its_own_adoption` (to create at #77); pattern at `crates/actions/tests/eval_signoff_gate.rs`.
4. **Unmeasurable kata not opened.** Given an open request with no instrument id, version, split or prediction, when `kata.open` runs, then it refuses `422 kata_unmeasurable` and writes no kata row. Verify: `crates/actions/tests/kata_open.rs::a_kata_without_an_instrument_is_not_opened` (to create at #77).
5. **Prediction before measurement.** Given a kata, when `kata.measure` runs before a prediction row exists, then it refuses `422 kata_prediction_missing`. Verify: `crates/actions/tests/kata_gets.rs::the_prediction_is_recorded_before_the_measurement` (to create at #77).

## Architecture artefacts

Common metadata: owner the `kata` job owner; reviewer this review, recorded on #347; review date 2026-11-18; version 1; project T5 — Kata; published document this architecture design, to be published under #77; links derive from SPEC_V2 §6.3/§12 T5. Purpose/stakeholders/scope: how the kata loop is measured, adopted and reverted, for the kata job, coach, PM, adopting human and target job owner.

**PSA-1 — Principles applied (catalogue).** P3 no work without card/evidence — every kata is a project card and every gate carries run and instrument ids; P4 executor never approves its own work — adoption excludes the kata's principal; P5 measurement before adoption — prediction and held-out measurement are mandatory; P6 meta-jobs are ordinary jobs — the same project, card, run and approval primitives; P10 no speculative aggregates — four tables, each read by an action; P11 tenant configuration over code — adoption policy, read grant and WIP limit are rows.

**PSA-2 — Requirements (catalogue).** R1 six Gets with an evidence set (§6.3); R2 pre-registered prediction before the candidate (§6.3); R3 held-out measurement (§12 T5); R4 revert on regression (§12 T5, §9.3); R5 adoption by a person, never self-approval (§9.3); R6 no kata without an instrument and threshold (§6.3); R7 propagation per target (§6.3); R8 lessons need two independent evidence refs at promotion (§7.6, owned by organisational memory).

**PSA-3 — Stakeholder map (matrix).** Axes stakeholder × concern; each cell is accountable (A), consulted (C), informed (I) or deliberately empty.

| Stakeholder \ Concern | Evidence set | Adoption | Priority and WIP | Publish | Propagation | Measurement |
|---|---|---|---|---|---|---|
| Kata facilitator | A | C | I | I | I | A |
| Coach | C (raises the candidate) | C | A | — | C | I |
| PM | I | C | A | — | I | C |
| Human approver | C | A | — | I | C | C |
| Target job owner | I | C | I | A | A | C |
| Operations | C (postmortem origin) | I | I | — | I | C |
| Architecture Board | I | — | — | — | A (job-family change) | I |

**PSA-4 — Driver/Goal/Objective catalog.** D1 improvements are asserted, not proven. G1 every adopted change wins on a held-out instrument. G2 every regression returns to its base version. G3 every improvement can reach another job. O1 one open kata per signature. O2 adoption refuses a non-held-out measurement. O3 revert restores a recorded version. O4 propagation is a per-target approval.

**PSA-5 — Solution concept (catalogue of edges).** Viewpoint: the adopting human asking what proves a kata change.

| # | From | To | The gate on the edge |
|---|---|---|---|
| 1 | source card or finding | kata project | `kata.open` refuses without an instrument id, version and pre-registered prediction |
| 2 | kata project | observation and problem statement | the six-Gets evidence set |
| 3 | problem statement | candidate | the prediction is recorded first |
| 4 | candidate | measurement | pinned instrument version, `split='test'` |
| 5 | measurement | adoption approval | a human decides; the kata cannot |
| 6 | adoption approval | production version | the target's own publish action and gate |
| 7 | measurement (regression) | base version | revert restores the recorded version |
| 8 | production version | propagation rows | per-target approval |

**TS-1 — Kata action catalogue (catalogue).** Metadata: type catalogue · phase/plateau A / PSA · status draft · version 1 · owner the `kata` job owner · reviewer this review, recorded on #347 · review date 2026-11-18 · project T5 — Kata · purpose the action surface and each gate it carries · stakeholders the kata job, coach, PM, adopting human and target job owner · scope §10.4's kata family plus the read actions this design adds · published document this architecture design, to be published under #77 · links §10.4, §6.3.

| Action | Kind | Refusal it carries | Event |
|---|---|---|---|
| `kata.open` | write | `422 kata_unmeasurable` without instrument and prediction; `409 kata_signature_open` for a second open kata on one signature | `kata.opened` |
| `kata.observe` | write | refuses an observation with no evidence ref | `kata.observed` |
| `kata.analyse` | write | refuses a problem statement without measurement data | `kata.analysed` |
| `kata.propose` | write | refuses a candidate with no pre-registered prediction (`422 kata_prediction_missing`) | `kata.candidate_proposed` |
| `kata.measure` | write | refuses a non-held-out split (`422 kata_measurement_not_held_out`) | `kata.measured` |
| `kata.adopt` | write | refuses a non-human decision (`403 kata_self_approval`) | `kata.adopted` |
| `kata.propagate` | write | refuses a target without its own approval | `kata.propagated` |
| `kata.close` | write | refuses `outcome='reverted'` without a restore (`422 kata_revert_missing`) | `kata.closed`, `kata.reverted` |
| `kata.get`, `kata.list` | read | n/a | — |

## Migration and rollout

One forward-only migration, `0052_t5_kata.sql` (to create): four tables with RLS, the signature index; no backfill, no edit to an existing table. Seed changes add `kata_v2` (to create), the kata actions and a capability description as a new seed release, never an edit to the published `kata` row. Rollout follows T2 and S16–S20: register actions, seed the type, open one kata on a low-risk procedure, prove measurement and revert locally, then enable the coach's escalation. The feature is dark until a kata project exists. The adoption gate defaults to human dual control through `approval_policy`; the failed-kata cord stays on `andon`.

## Links

- Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-7d05-75d4-ae9d-4e2d485271b4.html
- Issue: #77
- Spec: `docs/SPEC_V2.md` §6.3, §7.6, §8.1, §9.3, §9.4, §10.4, §12 T5; decisions D31, D45
- Inception standard: `docs/plans/P0-programme-definition.md` §5
- P0 published references: architecture principles, artefact standard, reuse map and blueprint at the public design site
