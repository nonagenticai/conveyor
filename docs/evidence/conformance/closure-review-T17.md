# T17 closure-review — the published output

`documents.compose` of `docs/plans/T17/closure-review.md` (category `closure-review`, unit `T17`), then `documents.publish`; the audit's declared-output rule reads this row.

```
== write: documents.compose ==
{
  "body": "---\ndocument: T17/closure-review\nversion: 1\nstatus: published\nproject: T17\ncategory: closure-review\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: closure-review\nphase: G\nplateau: capability\n---\n\n# T17 \u2014 Tenant asset authoring: closure review\n\nWhether the project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\n| Acceptance criterion | Evidence | Result |\n|---|---|---|\n| A short brief plus documents drafts a catalogue of departments and jobs | `crates/actions/tests/authoring.rs::a_chat_brief_drafts_the_org_and_job_catalogue` | pass |\n| Documents yield linked procedure and policy drafts | `authoring.rs::documents_yield_linked_procedure_and_policy_drafts`; `authoring.rs::a_draft_that_references_an_unpublished_asset_is_refused` | pass |\n| A reviewed catalogue publishes with its decisions | `authoring.rs::a_reviewed_catalogue_publishes_with_its_decisions`; `crates/actions/tests/authoring_standard.rs`; `apps/web/e2e/authoring.spec.ts` | pass |\n| No asset publishes itself | `crates/actions/tests/asset_review.rs::the_authoring_job_cannot_publish_its_own_asset`, `::a_non_human_decision_cannot_complete_an_asset_gate` | pass |\n| No asset widens a tool grant | `asset_review.rs::an_asset_never_widens_a_tool_grant` | pass |\n| The delivery is on main and gated | issues #87 (PR #632, `c29c7f52`), #629 (PR #659, `38334552`), #655 (PR #687, `ca392a97`); `actions:authoring` = `exists` (`v2-coverage.md`) | pass |\n\n## Evidence\n\n- Delivery merges: `c29c7f52d939` (PR #632), `38334552e9b8` (PR #659), `ca392a97d7d8` (PR #687).\n- Behaviour: the `authoring.rs`, `authoring_standard.rs` and `asset_review.rs` scenarios; the\n  real-app authoring E2E.\n- Dependencies: T3 (#74) and T16 (#85) are closed; the live procedure/policy ingestion is T16's\n  capability, and this project's tests prove the linked drafts and the refused dependency order.\n- The inception set: `docs/plans/inception/T17/`.\n\n## Open items\n\n- The dependency order is refused at the draft attempt (`v2-coverage.md`), not at publish \u2014 this\n  review does not claim a publish-time check.\n- The intake brief's readability before #655 is corrected by #655/PR #687; this review cites the\n  corrected record.\n- The tranche-5 authorisation is **not** claimed here; it is the #526 programme chain's act.\n\n## Decision\n\nClose. Acceptance is met on the evidence above; the listed items are recorded as scoped refusals or\nanother unit's capability, and no tranche authorisation is manufactured.\n",
  "card_id": null,
  "category": "closure-review",
  "content_hash": "7c5c190ffd4dd8da55e963725636bd12c1715f3edf3ba31dc6b420ecf15b494c",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:46.622917Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121b-930f-7634-b3a6-2344a165371a",
  "kind": "document",
  "meta": {
    "category": "closure-review",
    "content_type": "text/markdown",
    "sha256": "7c5c190ffd4dd8da55e963725636bd12c1715f3edf3ba31dc6b420ecf15b494c",
    "size_bytes": 2550,
    "unit": "T17"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 2550,
  "status": "active",
  "title": "Closure review \u2014 T17 Tenant asset authoring",
  "unit": "T17",
  "updated_at": "2026-11-25T09:48:46.622917Z",
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
      "content_hash": "7c5c190ffd4dd8da55e963725636bd12c1715f3edf3ba31dc6b420ecf15b494c",
      "content_type": "text/markdown",
      "created_at": "2026-11-25T09:48:46.622917Z",
      "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "created_by_name": "FR Test",
      "external_ref": null,
      "id": "01a2121b-930f-7634-b3a6-2344a165371a",
      "kind": "document",
      "meta": {
        "category": "closure-review",
        "content_type": "text/markdown",
        "sha256": "7c5c190ffd4dd8da55e963725636bd12c1715f3edf3ba31dc6b420ecf15b494c",
        "size_bytes": 2550,
        "unit": "T17"
      },
      "parent_document_id": null,
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "publish_requested_at": null,
      "publishable": false,
      "published_at": null,
      "published_by": null,
      "published_by_name": null,
      "size_bytes": 2550,
      "status": "active",
      "title": "Closure review \u2014 T17 Tenant asset authoring",
      "unit": "T17",
      "updated_at": "2026-11-25T09:48:46.622917Z",
      "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "updated_by_name": "FR Test",
      "version": 1
    }
  ]
}
```
