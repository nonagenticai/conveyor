# T17 — Inception analysis

Project: T17 — Tenant asset authoring · Track: v2 · Tranche: 5 · Plan: docs/SPEC_V2.md §12
Unit: #87 (this document is dependency-shaping work; blocked-by T16, T3 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T16", "T3", "P0"]

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T17 — Tenant asset authoring · issue #87 · live document `01a1ee42-312b-770c-bdae-36a1b077d2e6`
- **Rationale:** Establish what exists and what is reused for Tenant asset authoring: current state, build-vs-reuse decisions and the risks the architecture must answer.
- **Published document:** `01a1ee42-312b-770c-bdae-36a1b077d2e6` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** current-state rows, reuse sources, build-vs-reuse decisions, risks, open questions.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §5.7, §10.4–§10.5, §12 T17 and P0 §5; realized by #87; reuse sources cited below; project dependencies T16, T3, P0.
- **Links:** issue #87; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Current state

**Built.** `industry`, `job_catalogue`, `job_department`, `job_release` and `job_binding` exist
with RLS and version columns (`migrations/0040_organisation.sql:job_catalogue`,
`migrations/0040_organisation.sql:job_release`, `migrations/0040_organisation.sql:job_binding`);
`department` predates them (`migrations/0001_platform.sql:department`). The release hash is
canonical (`crates/domain/src/organisation.rs:release_hash`), and a binding copies hash and
snapshot, so a changed release is a new binding rather than an edit.

**Built.** `job_catalogue.create/update/retire/get/list`
(`crates/actions/src/organisation.rs:job_catalogue_create`),
`job_releases.publish/retire` (`crates/actions/src/organisation.rs:job_releases_publish`),
departments (`crates/actions/src/handlers.rs:departments_create`), industries and job bindings all
appear in `docs/design/data/actions.json`.

**Built, not wired to assets.** `approval_policy` holds one tenant-scoped decision per action with
`approval ∈ required|waived` and `self_approval ∈ forbidden|allowed`, defaulting to forbidden
(`migrations/0050_approval_policies.sql:approval_policy`);
`crates/actions/src/control/policies.rs:approval_policies_set` writes it and
`crates/actions/src/control/approvals.rs:approvals_request` opens a gate. No asset publication uses
them.

**Built.** `seed/organisation.json` transcribes industries, departments and jobs from SPEC §4.6 as
seed rows, not tenant-authored ones.

**Absent — publication.** No action can publish an authored asset: `docs/design/data/actions.json`
has zero `authoring.*` names and `crates/actions/src/` has no authoring module. A stale binding
refuses `409 job_binding_stale`; `binding_required` (409) and a missing binding are also served
(`crates/actions/src/control/mod.rs`).

**Partial.** `documents.ingest` hands one document to the knowledge service
(`crates/actions/src/documents/ingest.rs:documents_ingest`) — an intake seam, not a converter; it
writes no draft. `governed_asset`'s `kind` CHECK was widened by `migrations/0044_plan_spine.sql:governed_asset`
to admit the §10.3 kinds including `sop_source`; what the table cannot carry is a typed
organisation, procedure or policy target.
The executable procedure tables were renamed by `migrations/0035_process_tree.sql:process`
(`procedure` → `process`, `procedure_step` → `process_step`), and that migration's new
`procedure` (`migrations/0035_process_tree.sql:procedure`) is the SOP text; neither carries PDDL
text or a PKO pin.

**Dependencies — measured.** T16 (#85): the PKO surface is absent — no
`pko_release`/`pko_entity`/`pko_relation` in `migrations/` — so the PKO-pinned conversion path T17
intends to generalise does not exist; that remains the blocker. `procedures.ingest_sop` and its
siblings **do** resolve (`docs/design/data/actions.json`), so the SOP intake seam exists; what T17
adds is the draft/PDDL conversion on top, not the action. T13 (#76): the tool estate exists
(`migrations/0055_tool_platform.sql:102`; `tools.*` registered), so a tool request has a target and
a ceiling; T13's remaining scope is its BYO publication surface (v1-w4a-6). T3 is present as
`migrations/0049_pm_job.sql` and `crates/actions/src/pm/`; its acceptance is outside this unit.

**Planned — this unit.** T17 (#87) is OPEN `status:blocked`; this is inception work, not build
evidence.

## Reuse sources

| Source | Kind | Analysis question |
|---|---|---|
| `crates/{domain,store,actions,api}` | repo | Which existing table/action does an asset extend, and which must it not duplicate? |
| `migrations/0040_organisation.sql` | repo | Is the fact already a row? |
| `migrations/0050_approval_policies.sql` + `crates/actions/src/control/approvals.rs` | repo | Does the per-action gate carry asset publication? |
| `migrations/0002_domain.sql:governed_asset` + document model | repo | Which row owns a draft, and which the published asset? |
| `crates/actions-derive` | repo | Action, job, or UI-only read? |
| `seed/organisation.json` | repo | May a seed ever update a published row? |
| knowledge ingest/retrieval | estate (capability name only) | Which result is evidence, and how is it redacted? |
| workspace crates | upstream | Does new code need a crate the workspace lacks? |

## Build vs reuse

| Candidate | Decision | Boundary / constraint |
|---|---|---|
| Draft store | build `authoring_draft` | proposals only; the published asset stays owned by its table (§10.1); no fork of `governed_asset` |
| Review | reuse `approval` + `approval_policy` | one review record per fact; depth is a policy row naming the publish action (P11) |
| Publication | reuse the target's publish action | T17 never writes target tables |
| Intake | reuse `document`; build `authoring_intake` | bytes stay in the document store; drafts cite document ids |
| SOP/RDF/BPMN conversion | reuse T16 | no second converter; a missing action refuses |
| Templates | build `asset_template` | no template table exists |
| Tool requests | build `tool_request` only | T13 owns tools/bindings; a request is never a grant |
| Upstream | reuse workspace crates | no new dependency without a workspace decision |

## Risks

| Risk | What breaks | Check |
|---|---|---|
| Two stores | P1: draft and published row diverge | one-owning-table contract test per kind |
| Self-publication | author run publishes its own draft | `self_approval` default forbidden (migration 0050); refusal test (to create at `crates/actions/tests/asset_review.rs`) |
| Tool-grant widening | a policy draft waives its own gate | (to create at `crates/actions/tests/asset_review.rs`) |
| Unordered publication | job references unpublished department | `depends_on` refusal `crates/actions/tests/authoring.rs::a_draft_that_references_an_unpublished_asset_is_refused` (to create) |
| T16 slip | no procedure/PKO drafts; gate unmeetable | `git grep pko_release origin/main -- migrations` empty |
| T13 BYO slip | a tool request can name the estate but has no tenant publication path | v1-w4a-6 open in the conformance ledger (`tool_binding` exists — 0055 — so the old grep precondition is retired) |
| Orphan assets | zero-use catalogue inflation (P10) | zero-use report |
| Public leak | permanent PUBLIC exposure | P9 scan before publication |

## Open questions

1. First-wave asset matrix beyond the gate's four kinds (§5.7 leaves it to T17).
2. Review depth per asset type: single reviewer or dual control.
3. Queue surface: `approvals.list` by gate, or `authoring.assets`? UI only.
4. Templates in the first wave, or deferred until a procedure uses one (P10)?
5. Chat intake as a card (P3) or an `authoring.intake` call over MCP?
6. Dual control for an authored `approval_policy` beyond `self_approval = forbidden`?

## Links

- Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-58e9-772a-87a3-ad2e9a1e8354.html
- Issue: `#87`
- Spec: `docs/SPEC_V2.md` §12 T17; §5.6; §5.7; §10.1; §10.3; §10.4; §11.1; §11.4; §11.8
- Inception standard: `docs/plans/P0-programme-definition.md` §5
