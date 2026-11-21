# T12 — Architecture design

Project: T12 — One organisation loop · Track: v2 · Tranche: 13 · Plan: docs/SPEC_V2.md §12
Unit: #88 (this document is dependency-shaping work; T12 depends on every T slice and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = all T slices + P0

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T12 — One organisation loop · issue #88 · live document `01a1ee5f-c33a-70bd-8cb2-a8ca3b379634`
- **Rationale:** Define the to-be architecture for One organisation loop: aggregates, tables and actions, decisions, acceptance and the artefact set.
- **Published document:** `01a1ee5f-c33a-70bd-8cb2-a8ca3b379634` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** context and boundary; aggregate/table/action rows; decisions; acceptance scenarios; PSA set; typed artefacts; rollout.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §12 T12, §9.4 and P0 §5; realized by #88; reuse sources cited below; project dependencies every T slice, P0.
- **Links:** issue #88; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Context and boundary

T12 is V2's acceptance. Its own surface is the acceptance harness plus assembly-only fixture rows where a refusal needs one. **It adds no second store**: no table, read model, event stream, migration or action family. Every fact is read from its owning slice, joined by pinned refs (`migrations/0002_domain.sql:303` `trace_link`).

T12 proves T3–T8, T11, T13, the architecture capability (T9) and the cadence (T10); it does not build them. An absent stage fails its scenario. P6/P7 live halves wait on the deployment slice #8 (S14); #18 is closed; origin/main is observed at `b439d506`. A green run that cannot name the proving row is not evidence.

## Aggregates, tables and actions

| Aggregate | Owner | Table or symbol | Actions driven | State |
|---|---|---|---|---|
| WBS, baseline, benefits | T1/T10 | `migrations/0044_plan_spine.sql:28` `wbs_node`, `:70` `project_baseline`, `:264` `benefit_profile` | `pm.*`, `benefits.*` | built |
| Job position, binding, decision | T0/T2 | `migrations/0040_organisation.sql:72` `job_catalogue`, `:162` `job_binding`, `:290` `decision`; `migrations/0046_decision_ladder.sql:17` | `jobs.*`, `job_bindings.*`, `ladder.*` | built |
| Card, run, approval, audit, evidence | V1 | `migrations/0002_domain.sql:114` `card`, `:166` `run`, `:196` `approval`; `migrations/0001_platform.sql:167` `audit_event`; `migrations/0041_runner.sql:21`; `migrations/0024_s16_inference_audit.sql:10` | `cards.*`, `runs.*`, `approvals.*` | built |
| Coach, kata, development, lessons | T4/T5/T6/T8 | no table; `seed/processes/named.yaml:110` `kata.open` | `coach.*`, `kata.*`, `dev.*`, `lessons.*` | planned |
| Incident and architecture | T7/T9 | no incident or `architecture_artifact` table; `migrations/0048_host_registry.sql:24` `host_registry` | `ops.*`, `architecture.*` | partial / planned |

Its only writes are fixtures and the actions under test.

## Decisions

**D1 — Drive the action registry, not HTTP.** The harness calls `conveyor_actions::invoke`, following `apps/server/tests/organisation_gate.rs:1`. Alternative: a black-box HTTP suite. Consequence: registry, wire and store proved in one process; UI not.

**D2 — One ordered scenario per part on one seeded tenant; no loop table.** Assertions join project → `wbs_node` → card → run → `runner_artefact` → approval → `audit_event`. Alternative: independent tests; a `loop_run` aggregate. Consequence: observable transitions; P1 holds by construction; P10 cannot pass until a conveyor-side budget refusal exists (the analysis records `budget_exceeded` absent); a missing join is a finding.

**D3 — Budget is derived, never stored.** P10 sums `inference_call` at call grain with a pinned price version. Alternative: stored cost. Consequence: exposes the absent refusal rather than masking it.

**D4 — Live halves are separate, visibly blocked tests.** Not-run with #8, never skipped. Alternative: a pretending test. Consequence: honest evidence while #8 is open.

## Acceptance

The gate, verbatim from `seed/programme.json` T12 and `docs/SPEC_V2.md` §12:

> one tenant runs request → PM → delivery jobs → coach interruption → kata → development project → deploy → operations incident → lesson, all audited, within budget; one benefit is baselined before its transition and measured after it, with the delta attributable to the operational area

#88's P1–P11 names align. #88 adds **P12** (buildout under the capability), not in the quoted gate and marked issue-only below; its baseline is `5cf3d75`/2026-11-13, this document's `61edfcf8`/2026-11-18.

| Clause (part) | Scenario | Verify |
|---|---|---|
| request → PM (P1) | governed project with charter, WBS, baseline; PM approval refused | `apps/server/tests/organisation_loop.rs::request_to_pm` (to create) |
| PM → delivery (P2) | each card names binding, inputs, acceptance, one live run | `apps/server/tests/organisation_loop.rs::pm_to_delivery_jobs` (to create at `apps/server/tests/organisation_loop.rs`) |
| delivery → coach (P3) | one coach card, one triage, one remedy; repeat escalates | `apps/server/tests/organisation_loop.rs::delivery_to_one_coach_card` (to create at `apps/server/tests/organisation_loop.rs`) |
| coach → kata (P4) | six Gets, evidence set, prediction; second kata refused | `apps/server/tests/organisation_loop.rs::coach_escalation_opens_kata` (to create at `apps/server/tests/organisation_loop.rs`) |
| kata → development (P5) | dev project or config change; no self-approval | `apps/server/tests/organisation_loop.rs::kata_adoption_to_development_project` (to create at `apps/server/tests/organisation_loop.rs`) |
| development → deploy (P6) | lane, CI, review, merge, digest deploy, rollback, health | `apps/server/tests/organisation_loop.rs::development_project_deploy_and_live_verify` (to create at `apps/server/tests/organisation_loop.rs`; live half gated by #8) |
| deploy → incident (P7) | one incident card; approved `ops:write` after a dry run | `apps/server/tests/organisation_loop.rs::deploy_failure_to_incident` (to create at `apps/server/tests/organisation_loop.rs`; live half gated by #8) |
| incident → lesson (P8) | two refs and review promote; one-off stays a runbook change | `apps/server/tests/organisation_loop.rs::incident_to_promoted_lesson` (to create at `apps/server/tests/organisation_loop.rs`) |
| all audited (P9) | one `audit_event` per transition with tenant, actor kind, evidence ref | `apps/server/tests/organisation_loop.rs::every_loop_transition_audited` (to create at `apps/server/tests/organisation_loop.rs`) |
| within budget (P10) | over-budget refused `429 budget_exceeded` before the call; cost from ledger | `apps/server/tests/organisation_loop.rs::loop_within_budget` (to create at `apps/server/tests/organisation_loop.rs`) |
| architecture + benefit (P11) | P11 has two halves: the architecture project passes Preliminary and A–F with its artefacts approved and its contract live (a delivery project without a contract is refused), and a benefit is baselined before its transition with an attributable post-transition delta | `apps/server/tests/organisation_loop.rs::architecture_and_benefits_acceptance` (to create at `apps/server/tests/organisation_loop.rs`) |
| issue-only (P12) | Preliminary rows live; every T project carries its contract | `apps/server/tests/organisation_loop.rs::the_buildout_runs_under_the_capability` (to create at `apps/server/tests/organisation_loop.rs`) |

No test is claimed to pass; a grep is not evidence.

## Architecture artefacts

### PSA set at inception

**Principles applied (SPEC_V2 §3).**

| Principle | Application |
|---|---|
| P1 one record per fact | joins owner rows; no loop store |
| P2 rule → laya → generation → human | asserts L0/L1/L3 rows; L2 has no transition |
| P3 / P4 | card, action, evidence; the executor never approves its own work |
| P9 / P10 | every call ledgered and budgeted; zero new tables and migrations |

**Requirements:** R1 loop (§12, §9.4); R2 audit (§8.5); R3 budget (§7.5); R4 benefit (§8.3); R5 no second store (§10.1, P1).

**Stakeholder map** (R responsible, A accountable, C consulted, I informed).

| Stakeholder | Traceability | Audit | Budget | Benefit | Gate integrity |
|---|---|---|---|---|---|
| Tenant sponsor | A | I | A | A | A |
| PM job | R | C | C | I | C |
| Coach and kata jobs | R | C | I | C | C |
| Delivery and ops jobs | R | R | R | I | C |
| BCM | I | I | C | R/A | I |

**Driver / goal / objective.**

| Driver | Goal | Objective |
|---|---|---|
| V2 is unproven until one loop runs | one traceable organisation loop | P1–P10 green against one sha |
| Benefits are claimed, not measured | one baseline → transition → delta | one benefit row with both measurements |
| Acceptance could become a parallel system | acceptance joins owner rows | zero new tables and migrations |

**Solution concept — catalogue of edges.**

| Source | Edge | Target |
|---|---|---|
| tenant request | becomes → decomposes into | project → charter approval → `wbs_node` |
| `wbs_node` / card | assigned → bound → executed | card → `job_binding` → run → `runner_artefact` / `inference_call` |
| interruption | becomes → escalates | coach card → kata |
| kata | proposes | development project → deploy |
| deploy failure | opens → promotes | incident card → lesson |
| every transition | recorded in | `audit_event` |

### Typed catalogue

**T12 acceptance coverage catalogue** — type `catalog`; phase: programme acceptance; plateau: `psa`; status: `draft`; version `1`; owner: T12 inception; author: T12 inception, 2026-11-18; reviewer: this review, recorded on #347; review date: 2026-11-18.

Purpose: map each clause to one scenario and owner. Stakeholders: tenant sponsor, PM, coach, kata, delivery/ops, BCM, Architecture Board. Scope: acceptance of V2 in one tenant. Project: T12 (#88). Published document: this architecture design, as the project's `architecture` document, to be published on #88. Links: derives from SPEC_V2 §12 T12; realises §9.4; depends on T3–T17 (#74–#87), P0 and the deployment slice #8; reuses #88.

| ID | Item | Description of use | Verify |
|---|---|---|---|
| A1 | request→PM (T3) | project, charter, WBS, baseline | `request_to_pm` |
| A2 | PM→delivery (T3) | cards with binding and acceptance | `pm_to_delivery_jobs` |
| A3 | delivery→coach (T4) | one interruption card | `delivery_to_one_coach_card` |
| A4 | coach→kata (T5) | six Gets and evidence | `coach_escalation_opens_kata` |
| A5 | kata→development (T5/T6) | adoption cannot self-approve | `kata_adoption_to_development_project` |
| A6 | development→deploy (T6) | lane, CI, review, rollback | `development_project_deploy_and_live_verify` |
| A7 | deploy→incident (T7) | incident card and gated mitigation | `deploy_failure_to_incident` |
| A8 | incident→lesson (T8) | two refs and review | `incident_to_promoted_lesson` |
| A9 | audit coverage (V1/T11) | one row per transition | `every_loop_transition_audited` |
| A10 | budget (T11) | derived cost and refusal | `loop_within_budget` |
| A11 | benefit (T1/T10) | baseline and measured delta | `architecture_and_benefits_acceptance` |
| A12 | buildout, issue-only (P0/T9) | live contracts and Preliminary rows | `the_buildout_runs_under_the_capability` |

## Migration and rollout

No migration, schema change, seed change or deploy. The harness is one new file family under `apps/server/tests/`, plus a fixture if required. It lands only after its blocked-by slices merge; until then no green suite may be claimed.

Order: T0–T17 merge except T12 → harness PR → `rbuild --gate` → one non-draft PR → merge. P6/P7 live halves re-run after #8 and are recorded, not assumed. Rollback: delete the file family; no production state. Forward-only: no shim, no flag.

## Links

- Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-2e2f-71fa-8714-2c4802220b43.html
- Issue: https://github.com/nonagenticai/conveyor/issues/88
- Spec: `docs/SPEC_V2.md` §12 (T12 row), §9.4, §8.1, §8.3, §8.5, §7.5, §5.4, §10.1, §3
