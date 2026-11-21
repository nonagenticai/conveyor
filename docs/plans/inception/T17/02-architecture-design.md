# T17 — Architecture design

Project: T17 — Tenant asset authoring · Track: v2 · Tranche: 5 · Plan: docs/SPEC_V2.md §12
Unit: #87 (this document is dependency-shaping work; blocked-by T16, T3 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T16", "T3", "P0"]

- **Status:** draft
- **Version:** 0.1
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** pending / pending
- **Related project:** T17 — Tenant asset authoring · issue #87 · live document `01a1ee42-3aca-7179-867c-000d986a865e`
- **Rationale:** Define the to-be architecture for Tenant asset authoring: aggregates, tables and actions, decisions, acceptance and the artefact set.
- **Published document:** `01a1ee42-3aca-7179-867c-000d986a865e` — `published_at` null (draft; not published)
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** context and boundary; aggregate/table/action rows; decisions; acceptance scenarios; PSA set; typed artefacts; rollout.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §5.7, §10.4–§10.5, §12 T17 and P0 §5; realized by #87; reuse sources cited below; project dependencies T16, T3, P0.
- **Links:** issue #87; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Context and boundary

T17 turns a tenant's chat brief and documents into reviewed, published tenant assets. Downstream of
T0, T2, T3, T13 and T16, it owns intake, drafts, the publication orchestrator, a template store and
an inert tool request. It does
not own published assets' tables (§10.1), SOP/RDF/BPMN conversion and PKO (T16, #85, OPEN
`status:blocked`, absent), the tool model (T13, #76, OPEN `status:claimed`, absent), or policy
evaluation (T2). Publication calls the owning aggregate's action in one transaction; a missing
action refuses, never degrades.

## Aggregates, tables and actions

| Aggregate | Table | Invariant | Actions and events |
|---|---|---|---|
| Authoring intake | `authoring_intake` (to create) | one row per brief/document set; channel, source ids, requester; never writes a published asset | `authoring.intake` (to create, §10.4); `authoring.intake.recorded` (to create) |
| Authoring draft | `authoring_draft` (to create) | one row per proposed asset (`asset_kind`, `target_key`, `payload`, `revision`, `content_hash`, `depends_on[]`); `draft\|in_review\|approved\|rejected\|superseded`; never mutates a published row | `authoring.draft`, `authoring.assets` (to create, §10.4); `authoring.draft.proposed|reviewed` (to create) |
| Review | reuse `approval` + `approval_policy` | one approval row per draft revision; decider ≠ requester while `self_approval='forbidden'` | `approvals.request`, `approvals.decide` (existing; `crates/actions/src/control/approvals.rs:approvals_request`) |
| Publication | no table | calls the owning aggregate's publish action in the same transaction; writes no target table | `authoring.publish` (to create, §10.4); `authoring.asset.published` (to create) |
| Template | `asset_template` (to create) | tenant-scoped, versioned, keyed by `project_type_code`; data, never a runtime fork | published via `authoring.publish` |
| Tool request | `tool_request` (to create) | names tool/toolkit intent, requested effect class and justification; never a binding, credential, ceiling or `approval_policy` | published via `authoring.publish`; feeds T13's `tool_authoring.draft` (to create) |

`intake` means `authoring.intake`. Every existing action name resolves in
`docs/design/data/actions.json`. The T16 names `procedures.draft`, `pko.import_rdf`, `pko.edit`
and `pko.publish`, and T13's `tool_authoring.draft`, are absent today and are marked `(to create)`.

| Asset kind | Owning table / action | Drafted by | Publish action | Approval |
|---|---|---|---|---|
| industry | `industry` / `industries.create` | intake | `industries.create` | human |
| department | `department` / `departments.create` | intake | `departments.create` | human |
| job | `job_catalogue` + `job_release` | intake | `job_catalogue.create` then `job_releases.publish` | human |
| procedure | `procedure` / `procedures.publish` (existing; T16 adds PDDL text and the PKO pin) | `procedures.draft` (to create, T16) | `procedures.publish` | human |
| PKO rows | `pko_release`/`pko_entity` (to create, T16) | `pko.import_rdf`/`pko.edit` (to create, T16) | `pko.publish` (to create, T16) | human |
| policy | `approval_policy` / `approval_policies.set` | intake | `approval_policies.set` | human; self-approval forbidden |
| template | `asset_template` (to create) | intake | `authoring.publish` | human |
| tool request | `tool_request` (to create) | intake | `authoring.publish` | human |
| tool grant | none — never authored | — | — | refused |

Refusals (to create, 409 per P8): `asset_not_published`, `draft_not_reviewed`,
`tool_grant_widening`, `authoring_not_ready` naming the missing action.

## Decisions

**D1 — new draft aggregate.** Alternative: extend `governed_asset`. Consequence: a typed target
per proposal for one table; the alternative overloads a lifecycle whose `kind` CHECK
(`migrations/0044_plan_spine.sql:governed_asset`) already carries fifteen kinds, and it cannot
carry a typed organisation, procedure or policy target.

**D2 — review reuses `approval`.** Alternative: `authoring_review`. Consequence: one record per
fact and the existing `self_approval` machinery; depth becomes `approval_policy` rows (P11).

**D3 — publication delegates.** Alternative: write target tables. Consequence: release-hash, ETag
and RLS invariants cannot be bypassed; cost is a dependency on T16/T13.

**D4 — authoring is an ordinary job.** Alternative: a hard-coded wizard. Consequence: a
`job_binding` on an `internal_agent` release (P6), measured like any job; intake cannot transition
a card (P2).

**D5 — tool requests are inert.** Alternative: auto-create a T13 draft toolkit. Consequence: no
authoring path can grant a tool; a request goes to T13's gate, costing a second review.

**D6 — dependency ordering enforced.** Alternative: sorted advice. Consequence: `depends_on` is
refused at publish (`asset_not_published`); a batch publishes bottom-up.

**D7 — first wave.** The gate's four kinds first; templates and tool requests follow in the same
slice (P10).

## Acceptance

Gate, verbatim: "a tenant supplies a short brief plus documents, and a reviewed catalogue of
departments, jobs, procedures and policies is published; no asset can publish itself or widen a
tool grant".

**Scenario 1 — brief + documents produce a reviewed catalogue.** Given a tenant supplies a brief
and two documents; when intake drafts the four kinds and each is approved; then
`authoring.publish` creates departments, jobs with published `job_release` rows carrying a
`release_hash`, and policies through `approval_policies.set` with the tenant's `self_approval`
unchanged; procedures are asserted only once T16 exists (blocked on #85). `job_catalogue.list`
and `approval_policies.list` return them. Verify: `crates/actions/tests/authoring.rs::a_chat_brief_drafts_the_org_and_job_catalogue` and `::a_reviewed_catalogue_publishes_with_its_decisions` (to create, #87).

**Scenario 2 — no asset publishes itself.** Given the principal that authored a draft; when it
publishes it or decides its own approval; then both are refused and no target row exists.
Verify: `crates/actions/tests/asset_review.rs::the_authoring_job_cannot_publish_its_own_asset` (to create, #87); default
`self_approval='forbidden'` (`migrations/0050_approval_policies.sql:approval_policy`).

**Scenario 3 — no asset widens a tool grant.** Given a `tool_request` draft; when it publishes;
then exactly one `tool_request` row exists, no `tool`/`toolkit`/`tool_binding`/`tool_call` row is
written, and no `approval_policy` row naming a `tools.*` action changes.
Verify: `crates/actions/tests/asset_review.rs::an_asset_never_widens_a_tool_grant` (to create, #87); precondition
`git grep 'CREATE TABLE tool_binding' origin/main -- migrations` returns nothing.

## Architecture artefacts

PSA set at inception (P# from SPEC_V2 §3; template per §8.1). Common metadata: owner — project #87
(`meta_procedure`); reviewer — the independent review recorded on #347; review date 2026-11-18;
version 1; project #87; published document — this document, pending publication on #87; links —
derivation from SPEC §12 T17 and §5.6–§5.7.

**Principles applied.** P1 one record per fact; P2 rule → laya → generation → human; P3 no
transition without an action; P4 no self-approval; P5 measurement before adoption; P6 ordinary
jobs; P8 fail closed; P10 no speculative aggregates; P11 tenant policy over code.

**Requirements catalogue — purpose: bind each element to its contract; scope: project #87.**

| uid | Requirement | Source |
|---|---|---|
| R1 | Brief plus documents yield a reviewed catalogue of departments, jobs, procedures and policies | §12 T17 gate |
| R2 | Every output is a draft behind a review gate; publication is a release and a gate | §5.7 |
| R3 | Publication is dependency-ordered; an unpublished reference is refused | §5.7 |
| R4 | The authoring job family is ordinary jobs on the two executors | §5.7 |
| R5 | Chat and documents cannot approve their own artefact or change an approval policy | §5.7 |
| R6 | No source or draft grants a tool or widens an effect ceiling | §5.6, §11.4 |
| R7 | One record per fact; review depth and policy values are tenant rows | P1, P11, §10.1 |

**Stakeholder map (matrix; axes: stakeholder × concern) — purpose: who holds which duty.**

| Stakeholder | Publish authority | Review obligation | Tenancy | Audit evidence |
|---|---|---|---|---|
| tenant administrator | grants publish rights | approves policy assets | own tenant | approval rows |
| asset author (job) | none | drafts, cannot approve | own tenant | `inference_call`, run trace |
| reviewer / approver | the only publisher | decides each draft | own tenant | `approval` row |
| Architecture Board | none in T17 | reviews principle-level assets | n/a | artefact links |
| platform operator | none | none | cross-tenant by role, audited | `audit_event` |
| auditor | read-only | none | cross-tenant by role | full chain |

**Driver/goal/objective catalogue — purpose: why T17 exists; scope: first wave.**

| uid | Type | Statement |
|---|---|---|
| DR1 | driver | A tenant's operating model is assembled by hand and by code changes |
| G1 | goal | A tenant publishes its operating model from its own documents |
| O1 | objective | One brief plus documents publishes departments, jobs, procedures and policies |
| O2 | objective | Every published asset is reviewed, attributable and dependency-ordered |
| O3 | objective | No authoring path can grant a tool or raise an effect ceiling |

**Solution concept (diagram; viewpoint: delivery lead; question "how does a document become a
published asset without authority?").** Intake records provenance and orders a draft set;
each enters the approval gate; an approved draft publishes by calling the owning action; T16
conversion is invoked, not rebuilt; tool requests stop at an inert row. Links: derivation → R1–R7;
realisation → D1–D7.

## Migration and rollout

One migration (the next free number at build time), forward-only: four tables
(`authoring_intake`, `authoring_draft`, `asset_template`, `tool_request`), RLS forced with
same-file policies (mirroring `migrations/0050_approval_policies.sql`), named constraints, version
columns, unique `(tenant_id, content_hash)` on drafts. No backfill; seed rows are inputs.
Order: migration → ActionSpecs → job release and binding → first intake. While
T16 or T13 is absent, intake refuses `409 authoring_not_ready`, so tables ship dark. Rejected
drafts are retained; retirement never cascades. Acceptance is the three scenarios, then one tenant
intake with accepted/rejected and revision counts.

## Links

- Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-58e9-772a-87a3-ad2e9a1e8354.html
- Issue: `#87`
- Spec: `docs/SPEC_V2.md` §12 T17; §3; §5.6; §5.7; §8.1; §10.1; §10.3; §10.4; §11.4
- Inception standard: `docs/plans/P0-programme-definition.md` §5
