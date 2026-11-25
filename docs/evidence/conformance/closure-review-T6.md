# T6 closure-review — the published output

`documents.compose` of `docs/plans/T6/closure-review.md` (category `closure-review`, unit `T6`), then `documents.publish`; the audit's declared-output rule reads this row.

```
== write: documents.compose ==
{
  "body": "---\ndocument: T6/closure-review\nversion: 1\nstatus: published\nproject: T6\ncategory: closure-review\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: closure-review\nphase: G\nplateau: capability\n---\n\n# T6 \u2014 Development project: closure review\n\nWhether the project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\n| Acceptance criterion | Evidence | Result |\n|---|---|---|\n| A development project records every step from lane to live verify | `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify`; migration `0068_development_project.sql`; issue #80 closed by PR #658 (`00094194`) | pass |\n| A red CI run refuses the merge gate | `development_project.rs::a_red_ci_run_refuses_the_merge_gate` | pass |\n| The implementer cannot decide the gates that judge it | `development_project.rs::the_implementer_cannot_decide_its_own_merge_gate`, `::a_development_project_cannot_change_the_gates_that_judge_it` | pass |\n| A deploy without a tested rollback is refused; an exercised rollback records both digests and the schema generation | `development_project.rs::a_deploy_without_a_tested_rollback_is_refused`, `::an_exercised_rollback_records_both_digests_and_the_schema_generation`, `::an_application_rollback_across_a_non_additive_migration_is_refused` | pass |\n| Auto-merge defaults off and reads the tenant policy row | `development_project.rs::auto_merge_defaults_off_and_reads_the_tenant_policy_row` | pass |\n| The project row, coverage and programme agree | #80 closed; PR #672 (`a8c22f8e`) carries the schema-aware evidence correction (#669); `build-coverage.py --check` green at `7d5ef094` | pass |\n\n## Evidence\n\n- Delivery merges: `00094194130b2e652a2c62fe1f0c11ccfa3e3fdb` (PR #658) and\n  `a8c22f8e3401d66f2e2f00d579ee00d73b7775b3` (PR #672, the post-closure evidence correction).\n- Schema: `migrations/0068_development_project.sql`.\n- Behaviour: `crates/actions/tests/development_project.rs` (the scenarios above).\n- Deployment record: deployed digest `sha256:3ce4f31f\u2026` (tag `sha-a2ef919` = main `a2ef9194`)\n  containing the merge.\n\n## Open items\n\n- The tranche-11 authorisation is **not** claimed here: the tranche is `planned` and its\n  authorisation is the #526 programme chain.\n- The rollback evidence was not final at #658 and was corrected by #672; this review cites the\n  corrected record.\n- No production deploy applier is claimed; the record proves the refusals and the recorded steps.\n\n## Decision\n\nClose. Acceptance is met on the evidence above; the tranche authorisation remains a separate\nprogramme act (#526), and this review does not manufacture it.\n",
  "card_id": null,
  "category": "closure-review",
  "content_hash": "be036ce33841406082c1d800059b28560bfdb652391fe4400f4b774320d07475",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:16.135941Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-3199-77ba-8325-11aec48b9a1c",
  "kind": "document",
  "meta": {
    "category": "closure-review",
    "content_type": "text/markdown",
    "sha256": "be036ce33841406082c1d800059b28560bfdb652391fe4400f4b774320d07475",
    "size_bytes": 2723,
    "unit": "T6"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d34-7973-8989-0b83246fc888",
  "publish_requested_at": null,
  "publishable": false,
  "published_at": null,
  "published_by": null,
  "published_by_name": null,
  "size_bytes": 2723,
  "status": "active",
  "title": "Closure review \u2014 T6 Development project",
  "unit": "T6",
  "updated_at": "2026-11-25T09:47:16.135941Z",
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
      "content_hash": "be036ce33841406082c1d800059b28560bfdb652391fe4400f4b774320d07475",
      "content_type": "text/markdown",
      "created_at": "2026-11-25T09:47:16.135941Z",
      "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "created_by_name": "FR Test",
      "external_ref": null,
      "id": "01a2121a-3199-77ba-8325-11aec48b9a1c",
      "kind": "document",
      "meta": {
        "category": "closure-review",
        "content_type": "text/markdown",
        "sha256": "be036ce33841406082c1d800059b28560bfdb652391fe4400f4b774320d07475",
        "size_bytes": 2723,
        "unit": "T6"
      },
      "parent_document_id": null,
      "project_id": "01a1d039-0d34-7973-8989-0b83246fc888",
      "publish_requested_at": null,
      "publishable": false,
      "published_at": null,
      "published_by": null,
      "published_by_name": null,
      "size_bytes": 2723,
      "status": "active",
      "title": "Closure review \u2014 T6 Development project",
      "unit": "T6",
      "updated_at": "2026-11-25T09:47:16.135941Z",
      "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
      "updated_by_name": "FR Test",
      "version": 1
    }
  ]
}
```
