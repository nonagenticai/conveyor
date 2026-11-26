# T17 evidence — the published output

`documents.compose` of `docs/plans/T17/evidence.md` (category `evidence`, unit `T17`), then `documents.publish`; the audit's declared-output rule reads this row.

```
== write: documents.compose ==
{
  "body": "---\ndocument: T17/evidence\nversion: 1\nstatus: published\nproject: T17\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T17 \u2014 Tenant asset authoring: evidence\n\n## Claim\n\nT17 delivers tenant asset authoring: a short brief plus documents drafts a reviewed catalogue of\ndepartments, jobs, procedures and policies, which publishes with its decisions; no asset can\npublish itself and none can widen a tool grant.\n\n## Evidence\n\n- `crates/actions/tests/authoring.rs::a_chat_brief_drafts_the_org_and_job_catalogue`,\n  `::documents_yield_linked_procedure_and_policy_drafts`,\n  `::a_draft_that_references_an_unpublished_asset_is_refused`,\n  `::a_reviewed_catalogue_publishes_with_its_decisions`\n- `crates/actions/tests/authoring_standard.rs` (the output-standard checks)\n- `crates/actions/tests/asset_review.rs::the_authoring_job_cannot_publish_its_own_asset`,\n  `::a_non_human_decision_cannot_complete_an_asset_gate`,\n  `::an_asset_never_widens_a_tool_grant`\n- Merges `c29c7f52d939` (PR #632), `38334552e9b8` (PR #659), `ca392a97d7d8` (PR #687);\n  `actions:authoring` = `exists` in `docs/evidence/v2-coverage.md`.\n\n## Verification\n\n`cargo test -p conveyor-actions --test authoring --test asset_review --test authoring_standard`\nand `npx playwright test apps/web/e2e/authoring.spec.ts` on a checked-out main; the CI equivalent is\nthe PRs' `preflight`, `ci-public` and `web` at the merge commits.\n\n## Links\n\n- `docs/plans/inception/T17/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issues #87, #629, #655; PRs #632, #659, #687; `docs/evidence/v2-coverage.md`.\n\n## Gap\n\nThe dependency order is refused at the draft attempt, not at publish \u2014 no publish-time check is\nclaimed. The live procedure/policy ingestion is T16's capability; this project proves the linked\ndrafts and the refusals. No tool-grant widening is possible and none is claimed.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "e8e4885f7f92881ce3a78502c39ef64e8fce95488f26c5417200fd6698fc767f",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:59.890703Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121b-c6e2-75c4-98b0-bc2265acf711",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "e8e4885f7f92881ce3a78502c39ef64e8fce95488f26c5417200fd6698fc767f",
    "size_bytes": 1978,
    "unit": "T17"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 1978,
  "status": "active",
  "title": "Evidence \u2014 T17 Tenant asset authoring",
  "unit": "T17",
  "updated_at": "2026-11-25T09:48:59.890703Z",
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
      "category": "evidence",
      "content_hash": "e8e4885f7f92881ce3a78502c39ef64e8fce95488f26c5417200fd6698fc767f",
      "content_type": "text/markdown",
      "created_at": "2026-11-25T09:48:59.890703Z",
      "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "created_by_name": "FR Test",
      "external_ref": null,
      "id": "01a2121b-c6e2-75c4-98b0-bc2265acf711",
      "kind": "document",
      "meta": {
        "category": "evidence",
        "content_type": "text/markdown",
        "sha256": "e8e4885f7f92881ce3a78502c39ef64e8fce95488f26c5417200fd6698fc767f",
        "size_bytes": 1978,
        "unit": "T17"
      },
      "parent_document_id": null,
      "project_id": "01a1d039-0d38-7db1-b685-d76bd4cebf47",
      "publish_requested_at": null,
      "publishable": false,
      "published_at": null,
      "published_by": null,
      "published_by_name": null,
      "size_bytes": 1978,
      "status": "active",
      "title": "Evidence \u2014 T17 Tenant asset authoring",
      "unit": "T17",
      "updated_at": "2026-11-25T09:48:59.890703Z",
      "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "updated_by_name": "FR Test",
      "version": 1
    }
  ]
}
```
