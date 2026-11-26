---
document: T8/project-brief
version: 1
status: published
project: T8
category: project-brief
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: project-brief
phase: G
plateau: capability
---

# T8 — Organisational memory: project brief

Authored at closure from `docs/SPEC_V2.md` §12 T8 and decision D36 (the project predates the
inception set under `docs/plans/inception/`). It records what the project was scoped to change and
what it reuses; it claims no more than the delivered record.

## Outcome

A lesson proposed from a run cannot become doctrine until it carries **two independent evidence
references** and a **person's review**; a lesson with one reference is refused, and a bad run
records a lesson that is not doctrine.

## Boundary

- No incident-reference lessons: they are a typed `422` until T7's table exists.
- No `strata_rag` publishing or embeddings; the memory is the conveyor's own governed rows.
- Not the promotion-rate metric (#84) nor retro authoring (#82); not the live organisation loop
  (#88 / T12).
- No second memory copy in the agent platform; the conveyor owns the lesson rows.

## Reuse sources

- The governed-asset / document rows (`docs/SPEC_V2.md` §7.6, §10.3–§10.4) — lessons are proposed,
  reviewed and promoted, not a new store.
- The approval primitive for the human review (the same `approval` rows every gate uses).
- `migrations/0069_lesson_lifecycle.sql` for the lifecycle and its constraints.

## Inception units

The scope was specified in SPEC_V2 §12 T8 (the gate sentence) and D36; the delivery unit was
issue #79 (PR #664). This brief and the closure review are the project's durable inception and
closure record.

## Dependencies

- T5 Kata supplies measured lessons from the execute → memory → lesson chain.
- T7 owns incident references (typed `422` until it lands).
- T12 owns the end-to-end organisation loop that exercises the memory in production.
- P0 programme definition.

## Acceptance

The three gate scenarios of SPEC_V2 §12 T8, each proven by a named test:

1. a lesson with one evidence reference is refused (`lessons.rs::one_evidence_ref_is_refused`,
   `store/tests/lesson.rs::the_database_refuses_a_lesson_with_one_independent_ref`);
2. two independent references and a human review promote a source draft
   (`lessons.rs::two_independent_refs_and_a_human_review_promote_a_source_draft`);
3. a failed run records a lesson that is not doctrine
   (`lessons.rs::a_failed_run_records_a_lesson_that_is_not_doctrine`).

## Capabilities

A lesson lifecycle (`proposed → reviewed → promoted/rejected`) enforced in the store and the
database; the human review gate; the evidence-ref independence rule.

## Plan

Delivered by issue #79 / PR #664 (`57d8ef8f`, head `9e2b7f57`) with migration
`0069_lesson_lifecycle.sql`; the closure record is `docs/plans/T8/closure-review.md`.
