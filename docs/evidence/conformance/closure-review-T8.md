# T8 closure-review — the published output

`documents.compose` of `docs/plans/T8/closure-review.md` (category `closure-review`, unit `T8`), then `documents.publish`; the audit's declared-output rule reads this row.

```
== write: documents.compose ==
{
  "body": "---\ndocument: T8/closure-review\nversion: 1\nstatus: published\nproject: T8\ncategory: closure-review\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: closure-review\nphase: G\nplateau: capability\n---\n\n# T8 \u2014 Organisational memory: closure review\n\nWhether the project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\n| Acceptance criterion | Evidence | Result |\n|---|---|---|\n| A lesson with one evidence reference is refused | `crates/actions/tests/lessons.rs::one_evidence_ref_is_refused`; `crates/store/tests/lesson.rs::the_database_refuses_a_lesson_with_one_independent_ref`; migration `0069_lesson_lifecycle.sql` | pass |\n| Two independent references and a human review promote a source draft | `crates/actions/tests/lessons.rs::two_independent_refs_and_a_human_review_promote_a_source_draft` | pass |\n| A bad run records a lesson that is not doctrine | `crates/actions/tests/lessons.rs::a_failed_run_records_a_lesson_that_is_not_doctrine` | pass |\n| The delivery is on main and gated | issue #79 closed by PR #664, merge `57d8ef8fe07de413e6a73430584ddd2cf2d59d7f` (head `9e2b7f57`); preflight run 45393424722 success | pass |\n\n## Evidence\n\n- Delivery merge `57d8ef8fe07de413e6a73430584ddd2cf2d59d7f`; migration\n  `migrations/0069_lesson_lifecycle.sql`.\n- Behaviour: the named `lessons.rs` and `store/tests/lesson.rs` scenarios.\n- The brief: `docs/plans/T8/project-brief.md` (authored at closure from SPEC \u00a712 T8 and D36).\n\n## Open items\n\n- This closure review is the project's first; no earlier closure record existed.\n- Incident-reference lessons are a typed `422` until T7's table \u2014 T7's scope, not a T8 gap.\n- `strata_rag` publishing/embeddings were never in T8's scope; #84's promotion rate and #82's\n  retro authoring are separate units; the live loop is #88/T12.\n- The tranche-11 authorisation is **not** claimed here; it is the #526 programme chain's act.\n\n## Decision\n\nClose. Acceptance is met on the evidence above; the listed items are other projects' scope, and\nthis review does not manufacture a tranche authorisation.\n",
  "card_id": null,
  "category": "closure-review",
  "content_hash": "f20f70081bc2ba1382f0e7acd1f05aaf9b7b190adc4f3922bf1e3dbbbe7cad7a",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:42.160617Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-9742-7122-920e-f4a051cff757",
  "kind": "document",
  "meta": {
    "category": "closure-review",
    "content_type": "text/markdown",
    "sha256": "f20f70081bc2ba1382f0e7acd1f05aaf9b7b190adc4f3922bf1e3dbbbe7cad7a",
    "size_bytes": 2132,
    "unit": "T8"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d35-76bb-a22e-be49e3e95d16",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 2132,
  "status": "active",
  "title": "Closure review \u2014 T8 Organisational memory",
  "unit": "T8",
  "updated_at": "2026-11-25T09:47:42.160617Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.list ==
{
  "items": [
    {
      "body": null,
      "card_id": null,
      "category": "closure-review",
      "content_hash": "f20f70081bc2ba1382f0e7acd1f05aaf9b7b190adc4f3922bf1e3dbbbe7cad7a",
      "content_type": "text/markdown",
      "created_at": "2026-11-25T09:47:42.160617Z",
      "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "created_by_name": "FR Test",
      "external_ref": null,
      "id": "01a2121a-9742-7122-920e-f4a051cff757",
      "kind": "document",
      "meta": {
        "category": "closure-review",
        "content_type": "text/markdown",
        "sha256": "f20f70081bc2ba1382f0e7acd1f05aaf9b7b190adc4f3922bf1e3dbbbe7cad7a",
        "size_bytes": 2132,
        "unit": "T8"
      },
      "parent_document_id": null,
      "project_id": "01a1d039-0d35-76bb-a22e-be49e3e95d16",
      "publish_requested_at": null,
      "publishable": false,
      "published_at": null,
      "published_by": null,
      "published_by_name": null,
      "size_bytes": 2132,
      "status": "active",
      "title": "Closure review \u2014 T8 Organisational memory",
      "unit": "T8",
      "updated_at": "2026-11-25T09:47:42.160617Z",
      "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "updated_by_name": "FR Test",
      "version": 1
    }
  ]
}
```
