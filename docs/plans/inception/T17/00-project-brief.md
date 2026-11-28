# T17 — Tenant asset authoring

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T17 — Tenant asset authoring · issue #87 · live document `01a1e008-58e9-772a-87a3-ad2e9a1e8354`
- **Rationale:** state the inception basis for T17: a tenant's brief and documents become a reviewed catalogue of departments, jobs, procedures and policies, and no asset publishes itself or widens a tool grant.
- **Published document:** `01a1e008-58e9-772a-87a3-ad2e9a1e8354` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** outcome, boundary, reuse sources, inception units, dependencies, acceptance, capabilities, plan, risks, links.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §5.7 and §10.4–§10.5, §12 T17, and P0 §4; realized by #87; reuse sources below; depends on T16, T3 and P0.
- **Links:** issue #87; `docs/SPEC_V2.md` §12; `01-inception-analysis.md`; `02-architecture-design.md`; `seed/programme.json`.

## Outcome

A tenant supplies a brief and documents, and a reviewed catalogue of departments, jobs, procedures and policies is published; no asset can publish itself or widen a tool grant.

## Boundary

In scope: the authoring lifecycle — intake a brief and documents, draft organisation/department/job/procedure/policy assets with their sources, review each, and publish through the target's own publish action — with the draft store, the review record and the templates.

Out of scope: writing the target tables (departments, jobs, procedures, policies publish themselves); a second converter for SOP/RDF/BPMN (T16 owns it); tools and bindings (T13 owns them; T17 may request, never grant); a fork of `governed_asset`; a new template table is in scope only as `asset_template`.

## Reuse sources

- `crates/{domain,store,actions,api}` — the existing table/action surfaces an asset extends; the analysis names which must not be duplicated.
- `migrations/0040_organisation.sql` — is the fact already a row?
- `migrations/0050_approval_policies.sql` + `crates/actions/src/control/approvals.rs` — the per-action review gate.
- `migrations/0002_domain.sql:governed_asset` + the document model — which row owns a draft, which the published asset.
- `crates/actions-derive` — action, job, or UI-only read.
- `seed/organisation.json` — a seed may never update a published row.
- Estate capability, by role: knowledge ingest/retrieval — which result is evidence, and how it is redacted.
- Upstream: the workspace crates — no new dependency without a workspace decision.

## Inception units

1. `inception-analysis` — `01-inception-analysis.md`; live `01a1ee42-312b-770c-bdae-36a1b077d2e6` (published 2026-11-21).
2. `architecture-design` — `02-architecture-design.md`; live `01a1ee42-3aca-7179-867c-000d986a865e` (published 2026-11-21).

Both are published (owner publication, 2026-11-21); the inception gate is met. #87 is claimed by its delivery session; this brief does not change that issue's status.

## Dependencies

Typed, from `seed/programme.json`: blocked-by T16, T3 and P0; related #87 (delivery, claimed). T16 owns ingestion and the SOP human gate; T3 owns the decision ladder. The delivery unit is claimed, not authorised by this brief.

## Acceptance

The project gate: a tenant supplies a short brief plus documents, and a reviewed catalogue of departments, jobs, procedures and policies is published; no asset can publish itself or widen a tool grant.

Verify: #87's scenarios (`crates/actions/tests/authoring.rs`, `asset_review.rs`, `apps/web/e2e/authoring.spec.ts`, to create there); at inception, the published analysis+design pair, not yet satisfied.

## Capabilities

`meta_procedure`, `design` — the authoring surfaces; no other capability may publish an authored asset.

## Plan

`docs/SPEC_V2.md` §5.7, §10.4–§10.5 and §12 row T17. Delivery unit #87; no separate `S##` slice file. Target tranche **5 — not authorised**.

## Risks

- Two stores: draft and published row diverge → one-owning-table contract test per kind.
- Self-publication → `self_approval` default forbidden (migration 0050); refusal test at `crates/actions/tests/asset_review.rs` (to create).
- Tool-grant widening → a policy draft waives its own gate; refusal test at `asset_review.rs` (to create).
- Unordered publication → job references an unpublished department; `depends_on` refusal test at `authoring.rs::a_draft_that_references_an_unpublished_asset_is_refused` (to create).
- T16 or T13 slip → the gate is unmeetable; check `procedures.draft` in `docs/design/data/actions.json` (the re-scoped T16 surface) and `CREATE TABLE tool_binding` on main.
- Orphan assets inflate the catalogue → zero-use report.
- Public leak → P9 scan before publication.

## Links

- Issue #87 (delivery; claimed).
- `docs/SPEC_V2.md` §5.7, §10.4–§10.5, §12 T17.
- `seed/programme.json` row T17.
- `01-inception-analysis.md`, `02-architecture-design.md`.
