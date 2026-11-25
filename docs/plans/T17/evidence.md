---
document: T17/evidence
version: 1
status: published
project: T17
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: evidence
phase: G
plateau: capability
---

# T17 — Tenant asset authoring: evidence

## Claim

T17 delivers tenant asset authoring: a short brief plus documents drafts a reviewed catalogue of
departments, jobs, procedures and policies, which publishes with its decisions; no asset can
publish itself and none can widen a tool grant.

## Evidence

- `crates/actions/tests/authoring.rs::a_chat_brief_drafts_the_org_and_job_catalogue`,
  `::documents_yield_linked_procedure_and_policy_drafts`,
  `::a_draft_that_references_an_unpublished_asset_is_refused`,
  `::a_reviewed_catalogue_publishes_with_its_decisions`
- `crates/actions/tests/authoring_standard.rs` (the output-standard checks)
- `crates/actions/tests/asset_review.rs::the_authoring_job_cannot_publish_its_own_asset`,
  `::a_non_human_decision_cannot_complete_an_asset_gate`,
  `::an_asset_never_widens_a_tool_grant`
- Merges `c29c7f52d939` (PR #632), `38334552e9b8` (PR #659), `ca392a97d7d8` (PR #687);
  `actions:authoring` = `exists` in `docs/evidence/v2-coverage.md`.

## Verification

`cargo test -p conveyor-actions --test authoring --test asset_review --test authoring_standard`
and `npx playwright test apps/web/e2e/authoring.spec.ts` on a checked-out main; the CI equivalent is
the PRs' `preflight`, `ci-public` and `web` at the merge commits.

## Links

- `docs/plans/inception/T17/00-project-brief.md`, `01-inception-analysis.md`,
  `02-architecture-design.md`
- Issues #87, #629, #655; PRs #632, #659, #687; `docs/evidence/v2-coverage.md`.

## Gap

The dependency order is refused at the draft attempt, not at publish — no publish-time check is
claimed. The live procedure/policy ingestion is T16's capability; this project proves the linked
drafts and the refusals. No tool-grant widening is possible and none is claimed.
