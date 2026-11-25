---
document: T8/closure-review
version: 1
status: published
project: T8
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: closure-review
phase: G
plateau: capability
---

# T8 — Organisational memory: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| A lesson with one evidence reference is refused | `crates/actions/tests/lessons.rs::one_evidence_ref_is_refused`; `crates/store/tests/lesson.rs::the_database_refuses_a_lesson_with_one_independent_ref`; migration `0069_lesson_lifecycle.sql` | pass |
| Two independent references and a human review promote a source draft | `crates/actions/tests/lessons.rs::two_independent_refs_and_a_human_review_promote_a_source_draft` | pass |
| A bad run records a lesson that is not doctrine | `crates/actions/tests/lessons.rs::a_failed_run_records_a_lesson_that_is_not_doctrine` | pass |
| The delivery is on main and gated | issue #79 closed by PR #664, merge `57d8ef8fe07de413e6a73430584ddd2cf2d59d7f` (head `9e2b7f57`); preflight run 45393424722 success | pass |

## Evidence

- Delivery merge `57d8ef8fe07de413e6a73430584ddd2cf2d59d7f`; migration
  `migrations/0069_lesson_lifecycle.sql`.
- Behaviour: the named `lessons.rs` and `store/tests/lesson.rs` scenarios.
- The brief: `docs/plans/T8/project-brief.md` (authored at closure from SPEC §12 T8 and D36).

## Open items

- This closure review is the project's first; no earlier closure record existed.
- Incident-reference lessons are a typed `422` until T7's table — T7's scope, not a T8 gap.
- `strata_rag` publishing/embeddings were never in T8's scope; #84's promotion rate and #82's
  retro authoring are separate units; the live loop is #88/T12.
- The tranche-11 authorisation is **not** claimed here; it is the #526 programme chain's act.

## Decision

Close. Acceptance is met on the evidence above; the listed items are other projects' scope, and
this review does not manufacture a tranche authorisation.
