# T6 — Inception analysis

Project: T6 — Development project · Track: v2 · Tranche: 11 · Plan: docs/SPEC_V2.md §12
Unit: #80 (this document is dependency-shaping work; blocked-by T5, T13 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T5", "T13", "P0"]

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T6 — Development project · issue #80 · live document `01a1ee4e-ac03-76aa-8f54-81018d8bcd57`
- **Rationale:** Establish what exists and what is reused for Development project: current state, build-vs-reuse decisions and the risks the architecture must answer.
- **Published document:** `01a1ee4e-ac03-76aa-8f54-81018d8bcd57` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** current-state rows, reuse sources, build-vs-reuse decisions, risks, open questions.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §6.4, §9.1, §12 T6 and P0 §5; realized by #80; reuse sources cited below; project dependencies T5, T13, P0.
- **Links:** issue #80; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Current state

**Work primitives — `built`.** `project_type_code` exists (`migrations/0002_domain.sql:project_type_code`); so do the plan spine (`migrations/0044_plan_spine.sql:wbs_node`) and the PM job: `crates/actions/src/pm/mod.rs:PM_JOB_KEY` guards every `pm.*` action, `resolve_evidence` refuses a gate with a missing evidence ref, and the store refuses a requester's decision.

**Project types — `partial`.** A type is a `catalogue_entry(kind='project_type')` bound to a process (`migrations/0035_process_tree.sql:process_map`). Seven are seeded (`seed/catalogue.json:project_type`); none is `development`. The `development` capability (`seed/capability.json:development`, `seed/processes/named.yaml:cap.development.create`) groups card types, not a phase template. §5.2's type is `absent`.

**Dev actions — `absent`.** `dev.open|implement|verify|merge|release|rollback` (§10.4) do not resolve in `docs/design/data/actions.json`; no `crates/actions/src/dev/` exists.

**Repositories — `built`.** `migrations/0013_documents.sql:repository` stores a repo whose token is a `:credential_secret_id` pointer into the sealed store; `:project_repository` links it to a project; `repositories.create` is served. `crates/actions/src/integrations/github.rs:GitHub` keeps the token write-only and redacted, with `open_pull` and `create_blob`.

**PR-only publication — `built`.** `crates/actions/src/publish.rs:PUBLISH_KIND` enqueues `publish.design`; `apps/server/src/publish.rs:run` opens one PR, never merges, `max_attempts = 1`.

**Coding — `partial`.** V1's ad-hoc path is `crates/actions/src/integrations/external.rs:coding_jobs_create`; T13's broker is `absent` (no tool table, no `tools.*` action), so T6 is blocked. The pod substrate exists (`migrations/0041_runner.sql:runner_run`, `apps/server/src/runner_dispatch.rs:POLL_AFTER_SECS`) but runs capabilities, not CI.

**Gates — `partial` and `absent`.** `migrations/0050_approval_policies.sql:approval_policy` holds only `approval` and `self_approval` per action, not §10.3's per-gate decisions, approver kinds and expiry. A sensitive-path list exists (`.github/CODEOWNERS`) but is unarmed: the `main` ruleset keeps `require_code_owner_review: false`, so a PR can edit `.github/workflows/ci-public.yml` unrefused.

**CI — `built`.** `.github/workflows/ci-public.yml:checks` runs fmt, clippy, nextest and audit; `:dco`, `:secret-scan`, `:lint`, `:no-private-refs`, `:design-pack`, `:image` and `:service-image` complete it. Every job is gated on the repository not being private: a skipped run proves nothing.

**Deploy read-back — `partial`.** `crates/actions/src/ops/reads.rs:ops_ci_status` and `:ops_deployments`, `crates/actions/src/ops/writes.rs:ops_rollback` exist but are T7's; no `dev_release` records a digest, live check or rollback.

**Dependencies — `absent`.** No `kata.*` action and no tool table: T5 and T13 are open.

## Reuse sources

| Source | Analysis question |
|---|---|
| `crates/actions/src/integrations/github.rs` (repo) | Which verbs (pull files, merge, check runs) must be added without a second client or token path? |
| `apps/server/src/publish.rs` (repo) | Which PR-only disciplines carry over, and which must not (merge)? |
| `migrations/0013_documents.sql` + `migrations/0040_organisation.sql` (repo) | Is the credential per repository, and which binding pins the release a dev step runs? |
| `crates/actions/src/pm/mod.rs` (repo) | Which PM actions gate a dev project, and where does the independent-approver rule live? |
| `.github/workflows/ci-public.yml` (repo) | Which gate is repo-specific, and which belongs to the estate archetype? |
| Coding fleet (estate capability, own repository) | Which operations cover lane, commit and PR, at what effect ceiling? |
| GitHub REST API and merge queue (upstream) | Which required checks and merge semantics does the gate depend on? |

## Build vs reuse

- **GitHub client: reuse and extend.** It owns the token boundary and the HTTP stack — no new crate; a merge verb elsewhere would create a second credential path (P9). Constraint: a public repo may call only public actions, and no self-hosted runner may be reachable from a fork PR.
- **PR-only publisher: reuse the discipline, not the job.** `dev.implement` is a new job kind with the same sealed-secret, one-branch, one-PR, never-merge rules.
- **Coding fleet: reuse through T13's broker.** `coding_jobs.*` is migrated by T13 and must not be called directly from a dev step.
- **CI: reuse unchanged.** The local gate runs the same commands, so CI is not a debugger; T6 edits nothing in the workflow.
- **Gate-set protection: build.** CODEOWNERS and workflow path filters are editable by the PR they judge; the refusal must sit at the action that holds the merge verb.
- **Deploy: reuse as read-back.** The cluster write stays human/Flux; `dev.release` records digest, live check and rollback from `ops.*` reads.

## Risks

- **R1 a PR weakens its own gate.** Check: `dev.merge` refuses a changed file intersecting the protected set without a gate-change approval (to create at #80).
- **R2 green CI read as approval.** Check: merge refuses without an independent human approval naming the run evidence (to create at #80).
- **R3 the token leaks.** Check: `:secret-scan` and `:no-private-refs` on the PR, plus its debug-redaction test.
- **R4 a second coding path forms before T13.** Check: the served-set lock test (`crates/actions/tests/registry.rs`).
- **R5 deploy or rollback asserted, not evidenced.** Check: closure refuses a release with an empty live check or an untested rollback (to create at #80).
- **R6 a skipped private-repo run counts as green.** Check: the gate requires a run URL on a public repository.

## Open questions

1. **Which dependency repo?** §12 says "one dependency repo" without naming it; the charter must name it and confirm its change protocol permits an agent project.
2. **Who applies the deploy?** If Flux image automation is not enabled, `dev.release` must propose a manifest PR rather than record a completed deploy.
3. **Reviewer job or person?** §9.1 names an independent reviewer job, §6.7 makes merge approval human; the design assumes both — the job produces evidence, the person decides.
4. **Does T6 ship the earned-autonomy reader?** Today's `approval_policy` cannot carry `approver_kind` or conditions, so the reader may be a stub or a later slice.

## Links

- Project brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-8640-73e4-a90a-6bf8c250a0af.html
- Issue #80: https://github.com/nonagenticai/conveyor/issues/80
- SPEC_V2 §12 T6 (gate), §9.1 (procedure), §9.3 (guardrails), §5.2 (types), §5.4 (gates), §10.4 (actions), §3 (P1–P11).
- P0 artefact standard: https://conveyor.nonagentic.ai/design/docs/output-01a1e9d1-2720-776c-9741-a5deb322086e.html
