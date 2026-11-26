# T6 evidence — publish

`documents.publish` for `01a2121a-64a9-7241-bb3e-371b55d94602` (admin act, If-Match). The standard's required headings are checked on this transition.

```
== write: documents.publish ==
{
  "body": "---\ndocument: T6/evidence\nversion: 1\nstatus: published\nproject: T6\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T6 \u2014 Development project: evidence\n\n## Claim\n\nT6 delivers the development project's governed chain: every step from lane to live verify is\nrecorded, a red CI run refuses the merge gate, the implementer cannot decide the gates that judge\nit, a deploy without a tested rollback is refused and an exercised rollback records both digests\nand the schema generation.\n\n## Evidence\n\n- `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify`\n- `::a_red_ci_run_refuses_the_merge_gate`\n- `::the_implementer_cannot_decide_its_own_merge_gate`,\n  `::a_development_project_cannot_change_the_gates_that_judge_it`\n- `::a_deploy_without_a_tested_rollback_is_refused`,\n  `::an_exercised_rollback_records_both_digests_and_the_schema_generation`,\n  `::an_application_rollback_across_a_non_additive_migration_is_refused`\n- `::auto_merge_defaults_off_and_reads_the_tenant_policy_row`\n- Migration `0068_development_project.sql`; merges `00094194` (PR #658) and `a8c22f8e` (PR #672).\n\n## Verification\n\nRun `cargo test -p conveyor-actions --test development_project` on a checked-out main; the CI\nequivalent is the PRs' `preflight` and `ci-public` at the merge commits above.\n\n## Links\n\n- `docs/plans/inception/T6/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issue #80; PRs #658 and #672; issue #669 (the evidence correction).\n\n## Gap\n\nThe rollback evidence was not final at #658 and was corrected by #672 \u2014 this document cites the\ncorrected record. No production deploy applier is evidenced; the tests prove the refusals and the\nstep records. The tranche-11 authorisation is not claimed.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "cfb3159221b32a0e62a374ceed71c0eff18f31b97afbcb469c24bb171a340b79",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:29.208446Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-64a9-7241-bb3e-371b55d94602",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "cfb3159221b32a0e62a374ceed71c0eff18f31b97afbcb469c24bb171a340b79",
    "size_bytes": 1885,
    "unit": "T6"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d34-7973-8989-0b83246fc888",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:47:34.392087Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 1885,
  "status": "active",
  "title": "Evidence \u2014 T6 Development project",
  "unit": "T6",
  "updated_at": "2026-11-25T09:47:34.392087Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "---\ndocument: T6/evidence\nversion: 1\nstatus: published\nproject: T6\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T6 \u2014 Development project: evidence\n\n## Claim\n\nT6 delivers the development project's governed chain: every step from lane to live verify is\nrecorded, a red CI run refuses the merge gate, the implementer cannot decide the gates that judge\nit, a deploy without a tested rollback is refused and an exercised rollback records both digests\nand the schema generation.\n\n## Evidence\n\n- `crates/actions/tests/development_project.rs::a_development_project_records_every_step_from_lane_to_live_verify`\n- `::a_red_ci_run_refuses_the_merge_gate`\n- `::the_implementer_cannot_decide_its_own_merge_gate`,\n  `::a_development_project_cannot_change_the_gates_that_judge_it`\n- `::a_deploy_without_a_tested_rollback_is_refused`,\n  `::an_exercised_rollback_records_both_digests_and_the_schema_generation`,\n  `::an_application_rollback_across_a_non_additive_migration_is_refused`\n- `::auto_merge_defaults_off_and_reads_the_tenant_policy_row`\n- Migration `0068_development_project.sql`; merges `00094194` (PR #658) and `a8c22f8e` (PR #672).\n\n## Verification\n\nRun `cargo test -p conveyor-actions --test development_project` on a checked-out main; the CI\nequivalent is the PRs' `preflight` and `ci-public` at the merge commits above.\n\n## Links\n\n- `docs/plans/inception/T6/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issue #80; PRs #658 and #672; issue #669 (the evidence correction).\n\n## Gap\n\nThe rollback evidence was not final at #658 and was corrected by #672 \u2014 this document cites the\ncorrected record. No production deploy applier is evidenced; the tests prove the refusals and the\nstep records. The tranche-11 authorisation is not claimed.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "cfb3159221b32a0e62a374ceed71c0eff18f31b97afbcb469c24bb171a340b79",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:47:29.208446Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121a-64a9-7241-bb3e-371b55d94602",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "cfb3159221b32a0e62a374ceed71c0eff18f31b97afbcb469c24bb171a340b79",
    "size_bytes": 1885,
    "unit": "T6"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d34-7973-8989-0b83246fc888",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:47:34.392087Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 1885,
  "status": "active",
  "title": "Evidence \u2014 T6 Development project",
  "unit": "T6",
  "updated_at": "2026-11-25T09:47:34.392087Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```
