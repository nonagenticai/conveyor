---
document: T17/closure-review
version: 1
status: published
project: T17
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: closure-review
phase: G
plateau: capability
---

# T17 — Tenant asset authoring: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| A short brief plus documents drafts a catalogue of departments and jobs | `crates/actions/tests/authoring.rs::a_chat_brief_drafts_the_org_and_job_catalogue` | pass |
| Documents yield linked procedure and policy drafts | `authoring.rs::documents_yield_linked_procedure_and_policy_drafts`; `authoring.rs::a_draft_that_references_an_unpublished_asset_is_refused` | pass |
| A reviewed catalogue publishes with its decisions | `authoring.rs::a_reviewed_catalogue_publishes_with_its_decisions`; `crates/actions/tests/authoring_standard.rs`; `apps/web/e2e/authoring.spec.ts` | pass |
| No asset publishes itself | `crates/actions/tests/asset_review.rs::the_authoring_job_cannot_publish_its_own_asset`, `::a_non_human_decision_cannot_complete_an_asset_gate` | pass |
| No asset widens a tool grant | `asset_review.rs::an_asset_never_widens_a_tool_grant` | pass |
| The delivery is on main and gated | issues #87 (PR #632, `c29c7f52`), #629 (PR #659, `38334552`), #655 (PR #687, `ca392a97`); `actions:authoring` = `exists` (`v2-coverage.md`) | pass |

## Evidence

- Delivery merges: `c29c7f52d939` (PR #632), `38334552e9b8` (PR #659), `ca392a97d7d8` (PR #687).
- Behaviour: the `authoring.rs`, `authoring_standard.rs` and `asset_review.rs` scenarios; the
  real-app authoring E2E.
- Dependencies: T3 (#74) and T16 (#85) are closed; the live procedure/policy ingestion is T16's
  capability, and this project's tests prove the linked drafts and the refused dependency order.
- The inception set: `docs/plans/inception/T17/`.

## Open items

- The dependency order is refused at the draft attempt (`v2-coverage.md`), not at publish — this
  review does not claim a publish-time check.
- The intake brief's readability before #655 is corrected by #655/PR #687; this review cites the
  corrected record.
- The tranche-5 authorisation is **not** claimed here; it is the #526 programme chain's act.

## Decision

Close. Acceptance is met on the evidence above; the listed items are recorded as scoped refusals or
another unit's capability, and no tranche authorisation is manufactured.
