# T8 project-brief — publish

`documents.publish` for `01a2121a-c9b6-7429-a96a-484fbd2c2799` (admin act, If-Match). The standard's required headings are checked on this transition.

```
== write: documents.publish ==
{
  "body": "---\ndocument: T8/project-brief\nversion: 1\nstatus: published\nproject: T8\ncategory: project-brief\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: project-brief\nphase: G\nplateau: capability\n---\n\n# T8 \u2014 Organisational memory: project brief\n\nAuthored at closure from `docs/SPEC_V2.md` \u00a712 T8 and decision D36 (the project predates the\ninception set under `docs/plans/inception/`). It records what the project was scoped to change and\nwhat it reuses; it claims no more than the delivered record.\n\n## Outcome\n\nA lesson proposed from a run cannot become doctrine until it carries **two independent evidence\nreferences** and a **person's review**; a lesson with one reference is refused, and a bad run\nrecords a lesson that is not doctrine.\n\n## Boundary\n\n- No incident-reference lessons: they are a typed `422` until T7's table exists.\n- No `strata_rag` publishing or embeddings; the memory is the conveyor's own governed rows.\n- Not the promotion-rate metric (#84) nor retro authoring (#82); not the live organisation loop\n  (#88 / T12).\n- No second memory copy in the agent platform; the conveyor owns the lesson rows.\n\n## Reuse sources\n\n- The governed-asset / document rows (`docs/SPEC_V2.md` \u00a77.6, \u00a710.3\u2013\u00a710.4) \u2014 lessons are proposed,\n  reviewed and promoted, not a new store.\n- The approval primitive for the human review (the same `approval` rows every gate uses).\n- `migrations/0069_lesson_lifecycle.sql` for the lifecycle and its constraints.\n\n## Inception units\n\nThe scope was specified in SPEC_V2 \u00a712 T8 (the gate sentence) and D36; the delivery unit was\nissue #79 (PR #664). This brief and the closure review are the project's durable inception and\nclosure record.\n\n## Dependencies\n\n- T5 Kata supplies measured lessons from the execute \u2192 memory \u2192 lesson chain.\n- T7 owns incident references (typed `422` until it lands).\n- T12 owns the end-to-end organisation loop that exercises the memory in production.\n- P0 programme definition.\n\n## Acceptance\n\nThe three gate scenarios of SPEC_V2 \u00a712 T8, each proven by a named test:\n\n1. a lesson with one evidence reference is refused (`lessons.rs::one_evidence_ref_is_refused`,\n   `store/tests/lesson.rs::the_database_refuses_a_lesson_with_one_independent_ref`);\n2. two independent references and a human review promote a source draft\n   (`lessons.rs::two_independent_refs_and_a_human_review_promote_a_source_draft`);\n3. a failed run records a lesson that is not doctrine\n   (`lessons.rs::a_failed_run_records_a_lesson_that_is_not_doctrine`).\n\n## Capabilities\n\nA lesson lifecycle (`proposed \u2192 reviewed \u2192 promoted/rejected`) enforced in the store and the\ndatabase; the human review gate; the evidence-ref independence rule.\n\n## Plan\n\nDelivered by issue #79 / PR #664 (`57d8ef8f`, head `9e2b7f57`) with migration\n`0069_lesson_lifecycle.sql`; the closure record is `docs/plans/T8/closure-review.md`.\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "730d496a0daccbfb8db9a74d951dc24e1b70a87874155d5134bdf3b672ca9ba3",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:55.076177Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-c9b6-7429-a96a-484fbd2c2799",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "730d496a0daccbfb8db9a74d951dc24e1b70a87874155d5134bdf3b672ca9ba3",
    "size_bytes": 2904,
    "unit": "T8"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d35-76bb-a22e-be49e3e95d16",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:00.156966Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2904,
  "status": "active",
  "title": "Project brief \u2014 T8 Organisational memory",
  "unit": "T8",
  "updated_at": "2026-11-25T09:48:00.156966Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "---\ndocument: T8/project-brief\nversion: 1\nstatus: published\nproject: T8\ncategory: project-brief\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: project-brief\nphase: G\nplateau: capability\n---\n\n# T8 \u2014 Organisational memory: project brief\n\nAuthored at closure from `docs/SPEC_V2.md` \u00a712 T8 and decision D36 (the project predates the\ninception set under `docs/plans/inception/`). It records what the project was scoped to change and\nwhat it reuses; it claims no more than the delivered record.\n\n## Outcome\n\nA lesson proposed from a run cannot become doctrine until it carries **two independent evidence\nreferences** and a **person's review**; a lesson with one reference is refused, and a bad run\nrecords a lesson that is not doctrine.\n\n## Boundary\n\n- No incident-reference lessons: they are a typed `422` until T7's table exists.\n- No `strata_rag` publishing or embeddings; the memory is the conveyor's own governed rows.\n- Not the promotion-rate metric (#84) nor retro authoring (#82); not the live organisation loop\n  (#88 / T12).\n- No second memory copy in the agent platform; the conveyor owns the lesson rows.\n\n## Reuse sources\n\n- The governed-asset / document rows (`docs/SPEC_V2.md` \u00a77.6, \u00a710.3\u2013\u00a710.4) \u2014 lessons are proposed,\n  reviewed and promoted, not a new store.\n- The approval primitive for the human review (the same `approval` rows every gate uses).\n- `migrations/0069_lesson_lifecycle.sql` for the lifecycle and its constraints.\n\n## Inception units\n\nThe scope was specified in SPEC_V2 \u00a712 T8 (the gate sentence) and D36; the delivery unit was\nissue #79 (PR #664). This brief and the closure review are the project's durable inception and\nclosure record.\n\n## Dependencies\n\n- T5 Kata supplies measured lessons from the execute \u2192 memory \u2192 lesson chain.\n- T7 owns incident references (typed `422` until it lands).\n- T12 owns the end-to-end organisation loop that exercises the memory in production.\n- P0 programme definition.\n\n## Acceptance\n\nThe three gate scenarios of SPEC_V2 \u00a712 T8, each proven by a named test:\n\n1. a lesson with one evidence reference is refused (`lessons.rs::one_evidence_ref_is_refused`,\n   `store/tests/lesson.rs::the_database_refuses_a_lesson_with_one_independent_ref`);\n2. two independent references and a human review promote a source draft\n   (`lessons.rs::two_independent_refs_and_a_human_review_promote_a_source_draft`);\n3. a failed run records a lesson that is not doctrine\n   (`lessons.rs::a_failed_run_records_a_lesson_that_is_not_doctrine`).\n\n## Capabilities\n\nA lesson lifecycle (`proposed \u2192 reviewed \u2192 promoted/rejected`) enforced in the store and the\ndatabase; the human review gate; the evidence-ref independence rule.\n\n## Plan\n\nDelivered by issue #79 / PR #664 (`57d8ef8f`, head `9e2b7f57`) with migration\n`0069_lesson_lifecycle.sql`; the closure record is `docs/plans/T8/closure-review.md`.\n",
  "card_id": null,
  "category": "project-brief",
  "content_hash": "730d496a0daccbfb8db9a74d951dc24e1b70a87874155d5134bdf3b672ca9ba3",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:55.076177Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-c9b6-7429-a96a-484fbd2c2799",
  "kind": "document",
  "meta": {
    "category": "project-brief",
    "content_type": "text/markdown",
    "sha256": "730d496a0daccbfb8db9a74d951dc24e1b70a87874155d5134bdf3b672ca9ba3",
    "size_bytes": 2904,
    "unit": "T8"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d35-76bb-a22e-be49e3e95d16",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:00.156966Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2904,
  "status": "active",
  "title": "Project brief \u2014 T8 Organisational memory",
  "unit": "T8",
  "updated_at": "2026-11-25T09:48:00.156966Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```
