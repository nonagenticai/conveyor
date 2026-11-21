# T6 — Architecture design

Project: T6 — Development project · Track: v2 · Tranche: 11 · Plan: docs/SPEC_V2.md §12
Unit: #80 (this document is dependency-shaping work; blocked-by T5, T13 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T5", "T13", "P0"]

- **Status:** draft
- **Version:** 0.1
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** pending / pending
- **Related project:** T6 — Development project · issue #80 · live document `01a1ee4e-b842-70a4-81ee-aacce62c31a9`
- **Rationale:** Define the to-be architecture for Development project: aggregates, tables and actions, decisions, acceptance and the artefact set.
- **Published document:** `01a1ee4e-b842-70a4-81ee-aacce62c31a9` — `published_at` null (draft; not published)
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** context and boundary; aggregate/table/action rows; decisions; acceptance scenarios; PSA set; typed artefacts; rollout.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §6.4, §9.1, §12 T6 and P0 §5; realized by #80; reuse sources cited below; project dependencies T5, T13, P0.
- **Links:** issue #80; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Context and boundary

T6 owns the development project type and the actions that land a change on conveyor and one dependency repo under the platform's gates (§9.1, §5.2). T5 supplies the trigger, T13 the coding tool; both absent. T6 owns `dev.*`, the gate-protection and release records, the `development` type and its seed — not the PM (T3), plan spine (T1), broker (T13), ops writes (T7), the CI workflow or the manifests (Flux, human-applied).

## Aggregates, tables and actions

**Rows.** Built: `migrations/0002_domain.sql` (`project`, `project_type_code` column); `migrations/0044_plan_spine.sql` (`wbs_node`, `project_baseline`, `project_register_item`, `project_dependency`); `migrations/0013_documents.sql` (`repository`, `project_repository`; `credential_secret_id` is a repository column); `migrations/0040_organisation.sql` (`job_catalogue`, `job_release`, `job_binding`); `migrations/0041_runner.sql` (`runner_run`, `runner_artefact`); `approval` with typed evidence (`crates/actions/src/pm/mod.rs:resolve_evidence`). Partial: `migrations/0050_approval_policies.sql:approval_policy` — per action only, not §10.3's per-gate policy. Planned: `tool_call` (T13). Proposed (to create at #80, the next free migration number at implementation): `dev_gate_set` for the merge refusal; `dev_release` for deploy, live verify and rollback.

**Invariants.**
I1 one lane, at most one live PR.
I2 the implementing binding never merges (P4).
I3 red CI is a refusal, never a retry.
I4 a file intersecting `dev_gate_set` needs a `gate_set_change` approval.
I5 an absent gate set refuses the merge (P8).
I6 a deploy without a tested rollback is refused; a rollback names its predecessor.
I7 a release is immutable.
I8 the token is opened only inside the action, by secret id (P9).
I9 a dev job performs no cluster write.

**Actions — existing:** `pm.charter|plan|route_gate|close`, `process_runs.start`, `repositories.create|get|list|link`, `job_releases.publish`, `job_bindings.create`, `coding_jobs.create|get`, `ops.ci_status|deployments|rollback`, `publish.start`, `registers.create` — all resolve in `docs/design/data/actions.json`.

**Actions — (to create at #80):** `dev.open`, `dev.implement`, `dev.verify`, `dev.merge`, `dev.release`, `dev.rollback` — §10.4's family, traced to §12 T6's gate; `dev.gate_set.get|set` — D3/D8, traced to "the project cannot change its own gates".

**Events — (to create at #80):** `dev.opened`, `dev.pr_opened`, `dev.verified`, `dev.merged`, `dev.released`, `dev.rolled_back`, `dev.gate_refused`.

## Decisions

D1 **A project type and process on existing aggregates.** Alternative: a `dev_project` table. Consequence: charter, WBS and closure come from T1/T3; no fact is stored twice (P1, P10).

D2 **Implementation is a coding tool call through the T13 broker.** Alternative: the conveyor drives git. Consequence: every call is a `tool_call` with a ceiling; `409 tool_not_ready` without the binding; `coding_jobs.*` is not a dev path.

D3 **Gate protection is a refusal at `dev.merge`.** It lists the PR's files (a `list_pull_files` verb to create) and refuses `409 dev_gate_protected`. Alternative: a workflow filter or CODEOWNERS, both editable by the PR they judge. Consequence: the refusal sits where the merge verb is held.

D4 **Merge and deploy are separate approvals.** Alternative: one release that merges and deploys. Consequence: green CI never authorises a deploy (§9.3); auto-merge waits for `approver_kind`.

D5 **`dev_release` records deploy, live check and rollback.** Alternative: evidence on the closure asset. Consequence: one row for gate, closure and metrics; rollback is a new row.

D6 **The project proposes a deploy; it does not apply one.** Alternative: `dev.release` writes the manifest. Consequence: the shared-infrastructure rule holds; T7 keeps cluster writes.

D7 **The reviewer job produces evidence; a person decides.** Alternative: the reviewer's verdict is the approval. Consequence: P4 holds; independence compares `job_binding_id`.

D8 **Gate-set changes need their own approver set.** `dev.gate_set.set` refuses a decision from the governed project's binding. Alternative: the PM decides. Consequence: separation of duties is a row check.

## Acceptance

Gate, verbatim (§12 T6): "a real feature lands: lane → PR → CI → review → merge → deploy → live verify; the project cannot change its own gates; rollback tested".

- **A1 — a real feature lands.** Scenario: lane → coding tool → review artefact and CI run URL → merge approval → release with digest and live check. Verify: `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify` (to create at #80), plus `curl -fsS "$CONVEYOR_BASE_URL/health"` and the project's own acceptance command, both recorded in the closure record; the real run on the public repository is #80's evidence.
- **A2 — red CI is refused.** Verify: `crates/actions/tests/development_project.rs::a_red_ci_run_refuses_the_merge_gate` (to create at #80).
- **A3 — review is independent.** Verify: `crates/actions/tests/development_project.rs::the_implementer_cannot_decide_its_own_merge_gate` (to create at #80), mirroring `crates/actions/tests/destructive_gate.rs`.
- **A4 — the project cannot change its own gates.** Scenario: a PR touching `.github/workflows/ci-public.yml`. Verify: `crates/actions/tests/development_project.rs::a_development_project_cannot_change_the_gates_that_judge_it` (to create at #80); the sensitive-path list is `.github/CODEOWNERS`, armed by the gate set, never a parallel list.
- **A5 — deploy and live verify.** Verify: `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify` (to create at #80) covers the digest and the live check; the live check itself is the `curl` above.
- **A6 — rollback tested.** Verify: `crates/actions/tests/development_project.rs::a_deploy_without_a_tested_rollback_is_refused` (to create at #80), plus `curl -fsS "$CONVEYOR_BASE_URL/health"` at the restored digest, with both digests in the closure record.
- **A7 — auto-merge is tenant policy.** Verify: `crates/actions/tests/development_project.rs::auto_merge_defaults_off_and_reads_the_tenant_policy_row` (to create at #80).

## Architecture artefacts

**PSA set at inception.** Principles applied (SPEC §3): P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11. Requirements R1–R10, the stakeholder matrix, driver/goal/objective map and solution concept follow.

Common metadata for PSA-1 to PSA-5: owner the T6 delivery lead; reviewer this review, recorded on #347; review date 2026-11-18; version 1; project T6 (#80); published document this document, to publish on T6; links per artefact; stakeholders default to PSA-1's rows; purpose and scope are stated once here and per artefact only where they differ.

**PSA-1 · Stakeholder map** — `matrix`, phase A, plateau `psa`; purpose who decides or is consulted at each §9.1 gate. Links: reuses PF-1, derives from the brief, realizes PSA-3. R responsible, A approves, C consulted, I informed, X must not act.

| Stakeholder \ concern | Lane scope | Gate integrity | Evidence | Rollback | Secret custody | Status truth |
|---|---|---|---|---|---|---|
| Dev job | R | X | R | R | X | R |
| Reviewer job | C | C | R | C | I | C |
| Merge approver | A | A | A | I | I | A |
| Deploy approver | I | I | A | A | A | A |
| PM job | A charter | X | C | I | I | R closure |
| Architecture Board | I | R | C | C | I | A gate tier |
| Programme SRO | I | A gate change | I | I | I | I |

**PSA-2 · Requirements catalog** — `catalog`, phase A, plateau `psa`, purpose the requirements the actions satisfy; links: derives from the brief and PF-1, realized by `dev.*`.

| Id | Requirement | Type | Source |
|---|---|---|---|
| R1 | one lane, one PR | constraint | §9.1 |
| R2 | local gate first | condition | §9.1 |
| R3 | independent reviewer | constraint | §9.1 |
| R4 | human merge; red CI refused | condition | §9.1, §6.7 |
| R5 | deploy by digest; live check | condition | §9.1 |
| R6 | tested rollback before deploy | constraint | §9.3 |
| R7 | gate set immutable to the project | constraint | §9.3, §12 T6 |
| R8 | dependency repo is a linked project | delta | §9.1 |
| R9 | no cluster write from a dev job | constraint | §9.1 |
| R10 | closure links PR, pipeline, deploy | condition | §9.1 |

**PSA-3 · Driver/goal/objective map** — `matrix`, axes driver/goal/objective, phase A, plateau `psa`, purpose the change case; links: reuses PF-1, realizes the blueprint's "self-development is gated".

| Driver | Goal | Objective |
|---|---|---|
| DRV-1 self-development is manual | GOAL-1 the conveyor changes itself under its gates | OBJ-1 a feature lands through all seven steps, evidenced |
| DRV-2 green CI is mistaken for approval | GOAL-2 merge and deploy are separate decisions | OBJ-2 both refuse without evidence, proven negatively |
| DRV-3 a bad deploy is unrecoverable | GOAL-3 every deploy is reversible | OBJ-3 every release records a tested rollback |

**PSA-4 · Solution concept** — `diagram`, phase A, plateau `psa`, building block the dev delivery chain, viewpoint the merge approver: where the change stops and what is decided; links: realizes PSA-2, reuses the T13 broker. dev job → `dev.*` → (rows | broker → coding fleet | GitHub PR + CI) → evidence → approval → merge → `dev_release` → `ops.*` reads → live check.

**PSA-5 · Protected gate set** — `catalog`, phase A, plateau `psa`, purpose the paths a dev PR may not change alone; links: realizes R7, reuses PF-1. Each needs a `gate_set_change` approval.

| Path | Why protected |
|---|---|
| `.github/workflows/**` | the pipeline that judges the PR |
| `crates/actions/src/invoke.rs` | the action boundary enforcing approvals |
| `crates/actions/src/pm/**` | gate routing and typed evidence |
| `crates/actions/src/dev/**` | the dev family itself (to create at #80) |
| `migrations/0050_approval_policies.sql` | the approval-policy store |
| `seed/benchmark/**`, `crates/actions/tests/*gate*.rs` | the instrument (P5) |
| `docs/design/data/actions.json` | the served-spec lock |

HLD and SD follow the first dependency-repo contract.

## Migration and rollout

The next free migration number at implementation (to create at #80) adds `dev_gate_set` and `dev_release`, tenant-scoped with forced RLS and named constraints; forward-only, no backfill. Seed (to create at #80): the `development` type, a `dev.delivery` process, and a `development` job, release and binding. Order: T13's broker, the dev module, the Board's gate set, the proving feature on conveyor (its release is the first `dev_release`), then the dependency repo. Feature rollback is `dev.rollback`; schema rollback is a forward migration.

## Links

- Project brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-8640-73e4-a90a-6bf8c250a0af.html
- Issue #80: https://github.com/nonagenticai/conveyor/issues/80
- SPEC_V2 §12 T6 (gate), §9.1 (procedure), §9.3 (guardrails), §5.2 (types), §5.4 (gates), §10.3, §10.4 (actions), §10.6, §3 (P1–P11).
- P0 artefact standard: https://conveyor.nonagentic.ai/design/docs/output-01a1e9d1-2720-776c-9741-a5deb322086e.html
- P0 blueprint: https://conveyor.nonagentic.ai/design/docs/output-01a1e006-dacb-76e0-a45b-4f94de2bde4c.html
