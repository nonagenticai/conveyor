# T5 — Inception analysis

Project: T5 — Kata · Track: v2 · Tranche: 10 · Plan: docs/SPEC_V2.md §12
Unit: #77 (this document is dependency-shaping work; blocked-by T2, S16–S20 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T2", "S16", "S17", "S18", "S19", "S20", "P0"]

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T5 — Kata · issue #77 · live document `01a1ee4b-fbb1-7433-8ad7-2e75199c733f`
- **Rationale:** Establish what exists and what is reused for Kata: current state, build-vs-reuse decisions and the risks the architecture must answer.
- **Published document:** `01a1ee4b-fbb1-7433-8ad7-2e75199c733f` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** current-state rows, reuse sources, build-vs-reuse decisions, risks, open questions.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §6.3, §9.3–§9.4, §12 T5 and P0 §5; realized by #77; reuse sources cited below; project dependencies T2, S16–S20, P0.
- **Links:** issue #77; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Current state

State labels: `built | partial | planned | absent | proposed`. Every citation resolves at `origin/main`.

- **built** — capability and flow: `seed/capability.json:kata` is active; `seed/catalogue.json:kata` is a project type and `seed/catalogue.json:kata_wip` caps it at one. `seed/processes/named.yaml:kata.open` is one agent step with `kata.open.sop`, called as a child by the coach (`seed/processes/named.yaml:child_process_key`); only a replayable flow opens a kata, and a failed kata keeps the cord.
- **built** — the V1 verdict: `migrations/0034_s18_kata_arm.sql:benchmark_run` carries `subject_version`, `improvement_verdict ∈ keep|revert|inconclusive` and `improvement_of_run_id`; `apps/server/src/benchmark_improvement.rs:run` observes with `benchmark_runs.get`, judges with `benchmark_runs.compare` (controls passed, McNemar p < 0.05) and never promotes.
- **built** — the instrument: `migrations/0026_s19_eval.sql:eval_case` has `split ∈ train|test|none`; `eval_version` pins a drawn split by `split_sha256` and `split_locked_at`; `eval_run` carries `variant ∈ baseline|vN`, `change_md` and `change_patch`; `migrations/0036_s19_output_grade.sql:eval_grade` scores output; after `migrations/0035_process_tree.sql:benchmark_run` §8, `benchmark_run.subject_kind` is `process` (with `process_id`) and `eval.subject_kind` is `process`; the SOP is the separate `procedure` table.
- **built** — sign-off: `crates/actions/src/eval/signoffs.rs:signoff_requires_human` refuses every non-user principal; a sign-off is an `approval` row carrying the instrument digest, and editing a case voids it.
- **built** — support: `migrations/0035_process_tree.sql:procedure` (versioned SOP), `migrations/0002_domain.sql:project_type_code`, `migrations/0050_approval_policies.sql:approval_policy`, `crates/actions/src/control/andon.rs:ANDON_ENGAGED` (tenant-wide cord).
- **partial** — the seeded kata type is V1's replay-flow kata (phases setup/preliminary/execution, "Kata K1-K21"), not §6.3's six Gets; no code reads a six-Gets state.
- **partial** — measurement is benchmark-shaped: an eval run can carry a change patch and a variant, but no row links a candidate to an adoption decision, and `improvement_verdict` is a judgement, not the revert.
- **absent** — no action beginning `kata.` in `docs/design/data/actions.json`; the measurement entry points are the built `benchmark_runs.start|get|compare` and `eval_runs.start`.
- **absent** — no candidate, measurement, propagation or outcome table in `migrations/`.
- **absent** — no decision-time held-out enforcement: the split label exists, but no action refuses an adoption measured on `train`.
- **partial** — `migrations/0056_cadence_tranche.sql:10` creates the `lesson` table and `lessons.propose`/`lessons.list` are registered, so a lesson can be recorded and read; the §7.6 **promotion mechanism** — an evidence-gated promotion of a lesson into a job, prompt or knowledge change — is absent.
- **absent** — no kata-tenant policy of D45: tenancy exists, but nothing gives a kata its own approvals and budget.
- **refused** — a kata may not edit its instrument, approve its own adoption, measure on the training split only, or open a second kata for an open signature (§6.3, §9.3).

## Reuse sources

| Source | Analysis question |
|---|---|
| In repo — S18 verdict | Does `improvement_verdict` generalise from a benchmark pair to an eval run and a procedure version, and what is added for revert? |
| In repo — eval and benchmark | Which version, split and sign-off fields must a kata pin, and can measurement reuse `eval_runs.start`? |
| In repo — `procedure`, `job_release` | How is the target version identified, and what restores it on revert? |
| In repo — `approval`, `approval_policy`, `andon` | Can adoption reuse the policy row and the human-only refusal pattern, and who releases the cord? |
| Wider estate — knowledge/retrieval capability | What does promotion publish, and is the kata a proposer only? |
| Wider estate — system-one and coding fleet capabilities | Which change class may L1 select, and how does an adopted code candidate become a development project? |
| Upstream — Postgres, sqlx, workspace crates | Can every rule stay a constraint or action in `crates/actions`, with no second runtime? |

## Build vs reuse

- **Kata lifecycle: build** as `kata.*` actions over four new tables, reusing `project`, `card`, `run`, `approval` and `event`; no second runtime.
- **Verdict: reuse** the comparability rule in `benchmark_improvement` for benchmark measurements and apply it to eval measurements.
- **Instrument: reuse** `eval`/`benchmark` rows and `eval_signoffs`; kata never defines a metric, draws a split, or signs an instrument.
- **Adoption: reuse** `approval` plus `approval_policy`; the kata proposes only and the deciding principal is a person.
- **Revert: build** one outcome that restores the recorded base version through the target's own publish action.
- **Lessons: defer** to organisational memory's `lessons.propose`; T5 emits candidates at propagation.
- **Licence/contract:** workspace crates only; public repository, so no private host, path, token or credential enters code or this document; no new service or port.

## Risks

- A train-split measurement is adopted → **check:** an adoption refusal test on `split <> 'test'`.
- The instrument is edited between prediction and measurement → **check:** pin `eval_version.digest` and `split_sha256`; a mismatch voids the measurement.
- The kata approves its own change → **check:** the approver principal is compared to the kata run's executor.
- Revert is recorded but nothing restored → **check:** the base version is read back after `kata.close`.
- A second kata opens on the same signature → **check:** partial unique index on the open state.
- `kata_executor` is mistaken for the kata job → **check:** `crates/auth/src/principal.rs:is_kata_executor` is a machine tenant-admin role, not a job.
- A failed kata releases the cord on a timer → **check:** no release path from `kata.close`.

## Open questions

1. Which held-out label counts for adoption: `test` only, or a separately named holdout split that S19 must add?
2. Does an eval-based kata measure a procedure version through a new SOP subject kind, since `migrations/0035_process_tree.sql` moves the measured subject to `process`?
3. Who owns the kata tenant's read access to a target tenant's version: a scoped grant at open, or a pinned reference?
4. Is propagation one approval per target job, or one programme-level approval?

## Links

- Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-7d05-75d4-ae9d-4e2d485271b4.html
- Issue: #77
- Spec: `docs/SPEC_V2.md` §6.3, §7.6, §9.3, §9.4, §10.4, §12 T5; decisions D31 and D45
- Inception standard: `docs/plans/P0-programme-definition.md` §5
- P0 published references: blueprint, business case, architecture principles, artefact standard and reuse map at the public design site
