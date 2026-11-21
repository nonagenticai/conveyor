# T12 — Inception analysis

Project: T12 — One organisation loop · Track: v2 · Tranche: 13 · Plan: docs/SPEC_V2.md §12
Unit: #88 (this document is dependency-shaping work; T12 depends on every T slice and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = all T slices + P0

Measurement note: 61edfcf8 is an ancestor of the observed origin/main `b439d506` (2026-11-18); every citation resolves at origin/main. Labels describe rows and handlers, not a passing gate.

- **Status:** draft
- **Version:** 0.1
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** pending / pending
- **Related project:** T12 — One organisation loop · issue #88 · live document `01a1ee5f-b75b-76ee-93f3-62903c1661ba`
- **Rationale:** Establish what exists and what is reused for One organisation loop: current state, build-vs-reuse decisions and the risks the architecture must answer.
- **Published document:** `01a1ee5f-b75b-76ee-93f3-62903c1661ba` — `published_at` null (draft; not published)
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** current-state rows, reuse sources, build-vs-reuse decisions, risks, open questions.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §12 T12, §9.4 and P0 §5; realized by #88; reuse sources cited below; project dependencies every T slice, P0.
- **Links:** issue #88; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Current state

T12 adds no production slice. Its current state is the state of the seams it assembles.

**built (row level).**

- Plan spine and governance: `migrations/0044_plan_spine.sql:28` `wbs_node`, `:264` `benefit_profile`, `:350` `benefit_transition`; writers `crates/actions/src/spine/mod.rs:1889` `benefits.create`, `:2262` `benefits.transition_upsert`; `migrations/0050_approval_policies.sql:23` `approval_policy`; `migrations/0051_tranche_review.sql:14` `programme_tranche_review`.
- PM: `crates/actions/src/pm/mod.rs:256` `pm.charter`, `:396` `pm.plan`, `:533` `pm.route_gate`, `:614` `pm.close`; `migrations/0049_pm_job.sql:19` adds `project_baseline.change_id`.
- Organisation, decisions and core rows: `migrations/0040_organisation.sql:72` `job_catalogue`, `:162` `job_binding`, `:290` `decision`; `migrations/0046_decision_ladder.sql:17` adds the pinned recording columns; `migrations/0002_domain.sql:33` `project`, `:114` `card`, `:166` `run`, `:303` `trace_link`; `migrations/0001_platform.sql:167` `audit_event`; `crates/store/src/audit.rs:177` `record`.
- Evidence and tools: `migrations/0041_runner.sql:21` `runner_run`; `migrations/0039_document_outputs.sql:17` alters `document`; `migrations/0024_s16_inference_audit.sql:10` `inference_call`; `crates/actions/src/ops/reads.rs:26` `ops.pods`; `crates/actions/src/ops/writes.rs:147` `ops.restart`, `:320` `ops.rollback`.

**partial.**

- Coach: `crates/actions/src/board/pull.rs:149` `COACH_ERROR_PROCESS` and `crates/store/src/run.rs:1169` start a seeded process, but no `coach.*` action family exists (`name = "coach\.` → 0). T4's closed sets and per-signature uniqueness (#75) are unproven.
- Kata: `seed/processes/named.yaml:110` seeds `kata.open`; `apps/web/src/screens/improvement.rs:1` is the read half; `migrations/0034_s18_kata_arm.sql:17` adds `benchmark_run.improvement_verdict`. No six Gets, evidence set or pre-registered prediction; no `kata.*` action. T5 (#77).
- Audit coverage: the primitive exists; one row per loop transition with the same tenant, actor kind and evidence ref is P9 and is unproven.
- Budget: `migrations/0031_s21_prompt_budget.sql:1` is a character ceiling; `apps/inference-router/src/router/completions.rs:230` emits `budget_exceeded`, but no conveyor-side run refusal exists (`git grep -nE 'budget_exceeded' 61edfcf8 -- crates apps/server` → 0). §7.5's `429` is absent.
- `docs/design/data/actions.json:270` lists `pm.charter`, `:258` `ops.ci_status`, `:30` `benefits.create`, but no coach, kata, development or lessons entry. Intent, not implementation.

**absent.**

- Development project (T6, #80): `name = "dev\.` → 0; no development gate set.
- Lessons (T8, #79): `name = "lessons\.` → 0 and `CREATE TABLE lesson` → 0.
- Incident lifecycle (T7, #78): `CREATE TABLE incident` → 0 and `ops.incident|triage|mitigate|resolve|postmortem` → 0.
- Architecture capability (T9, #81): `CREATE TABLE architecture_artifact` → 0.
- Acceptance harness: `apps/server/tests/organisation_loop.rs` does not exist (listing → 0). Precedent: `apps/server/tests/organisation_gate.rs:1`, which proves the T0 wire gate through `conveyor_actions::invoke`.

## Reuse sources

| Source | What it supplies | Analysis question |
|---|---|---|
| `crates/store/src/audit.rs:177` | one audit writer | Does `record` cover every loop transition, or is a loop adapter needed? |
| `migrations/0044_plan_spine.sql:28` + `crates/actions/src/spine/mod.rs:1889` | WBS, baselines, benefits | Which rows exist, and which must PM actions create? |
| `migrations/0046_decision_ladder.sql:17` | decision shape | Is every loop decision an L0/L1/L3 row with pinned versions? |
| `crates/actions/src/pm/mod.rs:256` | PM family | Do `pm.plan` cards name a binding, inputs and acceptance? |
| `crates/actions/src/board/pull.rs:149` | coach wiring | Does the seeded process satisfy T4, or is T4 still required? |
| `migrations/0041_runner.sql:21` | run evidence | Is this the artefact chain P9 reads? |
| `migrations/0024_s16_inference_audit.sql:10` | call ledger | Can cost and budget be derived at call grain? |
| `crates/store/src/eval/runs.rs:2` + `migrations/0034_s18_kata_arm.sql:17` | held-out instruments | Which instrument proves the kata and the benefit delta? |
| `apps/server/tests/organisation_gate.rs:1` | wire-gate harness | Can its fixture pattern be reused unchanged? |
| `docs/design/data/actions.json:270` | design inventory | Source or plan? It lacks four loop families. |
| Wider estate, by capability: coding fleet, `strata_ingest`/`strata_rag`, inference-router, laya, kubernetes/git APIs, MinIO, `billing_service`/`licenses` | execution, retrieval, models, effects, storage, charging | Which capability is called through the broker and never owned by T12? |
| Upstream: sqlx, axum, tokio, serde, actions-derive | pinned stack, one-spec projection | Is a new crate needed? No. |

## Build vs reuse

- **Build:** the acceptance harness only, a test-crate file family under `apps/server/tests/`. Boundary: no production table, action, module or migration; fixture rows use existing actions or the test DB helper.
- **Reuse:** each production stage stays with its slice; T12 drives registered actions. Reuse the `organisation_gate.rs` fixture pattern and `conveyor_actions::invoke`; add no second DB harness.
- **Refuse:** a loop aggregate or table, a second audit writer, a coach/kata/dev implementation inside the test, or an HTTP client that bypasses the registry. The repo is PUBLIC, so fixtures carry no private host, tenant id or credential; actions are `ActionSpec` declarations; thresholds come from policy rows (P11).

## Risks

- A blocker is unmerged, so the harness cannot compile. Check per part before writing: `name = "kata\.` → 0 and the loop-test listing.
- The audit primitive is mistaken for coverage. P9 must count `audit_event` per transition and fail on a missing row.
- No conveyor-side budget refusal: P10 fails, not passes. Check `git grep -n budget_exceeded -- crates apps/server`.
- P6/P7 live halves wait on the deployment slice #8 (S14); #18, cited by an earlier draft, is CLOSED (docs reconciliation), so the blocker is #8 and must be re-checked at claim.
- Benefit attribution degrades to prose. Check that "operational area" is a `benefit_profile` field and that `benefits.transition_upsert` stores both measurements.

## Open questions

1. Does "within budget" bind the tenant, programme or each job binding? Default: tenant budget with per-binding defaults (issue #88, open question 1).
2. May T12 add assembly-only fixture rows to `seed/`? Default: test-only fixtures.
3. Who records "operational area" attribution: the BCM (T10) or programme config? Default: `benefit_profile` field, BCM-owned.
4. Is issue #88's P12 (buildout under the capability) part of the T12 gate or a separate check? It is not in the quoted gate.

## Links

- Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-2e2f-71fa-8714-2c4802220b43.html
- Issue: https://github.com/nonagenticai/conveyor/issues/88
- Spec: `docs/SPEC_V2.md` §12 (T12 row), §9.4, §7.5, §8.3, §8.5, §10.1, §3
- Inception standard: `docs/plans/P0-programme-definition.md` §5
- Artefact standard: https://conveyor.nonagentic.ai/design/docs/output-01a1e9d1-2720-776c-9741-a5deb322086e.html
- Reuse map: https://conveyor.nonagentic.ai/design/docs/output-01a1e006-fa29-756e-a710-90db4a232343.html
