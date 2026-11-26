# T15 evidence — publish

`documents.publish` for `01a2121b-60eb-71db-a750-c48bfd0c5112` (admin act, If-Match). The standard's required headings are checked on this transition.

```
== write: documents.publish ==
{
  "body": "---\ndocument: T15/evidence\nversion: 1\nstatus: published\nproject: T15\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T15 \u2014 Tool authoring and BYO: evidence\n\n## Claim\n\nT15 delivers the tool-authoring route: a tool generated from a spec is published and called, the\npublication approval defaults to a human and is tenant-configurable, a BYO MCP server is bound,\nnarrowed and audited, and no publishing job can exceed its effect ceiling.\n\n## Evidence\n\n- `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called`,\n  `::a_scaffolded_service_registers_its_own_openapi`\n- `::publication_waits_for_a_human_by_default`,\n  `::a_tenant_policy_changes_the_publication_gate_without_a_code_change`\n- `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited`,\n  `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped`\n- `::the_publishing_job_cannot_widen_its_own_effect_ceiling`,\n  `::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read`,\n  `::a_destructive_tool_needs_a_named_approver`\n- PR #666, merge `a2ef9194eb2c`; `actions:tools` = `exists` in `docs/evidence/v2-coverage.md`.\n\n## Verification\n\n`cargo test -p conveyor-actions --test tool_authoring --test byo_tools` on a checked-out main; the\nCI equivalent is the PR's `preflight` and `ci-public` at the merge commit.\n\n## Links\n\n- `docs/plans/inception/T15/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issue #86; PR #666; `docs/evidence/v2-coverage.md`.\n\n## Gap\n\n`/tools` is the V1 legacy surface and is not evidence for this project. A promotion returns\n`requested`; no platform version is stamped. Caller-supplied URLs and an archived-kag binding are\nnot claimed. No other delivery gap is named by the record.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "1125d15c8701469263f7912d7e32220251ead5623db55fe05a0652e8c6b19a05",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:33.783785Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121b-60eb-71db-a750-c48bfd0c5112",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "1125d15c8701469263f7912d7e32220251ead5623db55fe05a0652e8c6b19a05",
    "size_bytes": 1904,
    "unit": "T15"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d37-7f20-9959-979c31daeb5e",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:39.033041Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 1904,
  "status": "active",
  "title": "Evidence \u2014 T15 Tool authoring and BYO",
  "unit": "T15",
  "updated_at": "2026-11-25T09:48:39.033041Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "---\ndocument: T15/evidence\nversion: 1\nstatus: published\nproject: T15\ncategory: evidence\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: evidence\nphase: G\nplateau: capability\n---\n\n# T15 \u2014 Tool authoring and BYO: evidence\n\n## Claim\n\nT15 delivers the tool-authoring route: a tool generated from a spec is published and called, the\npublication approval defaults to a human and is tenant-configurable, a BYO MCP server is bound,\nnarrowed and audited, and no publishing job can exceed its effect ceiling.\n\n## Evidence\n\n- `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called`,\n  `::a_scaffolded_service_registers_its_own_openapi`\n- `::publication_waits_for_a_human_by_default`,\n  `::a_tenant_policy_changes_the_publication_gate_without_a_code_change`\n- `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited`,\n  `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped`\n- `::the_publishing_job_cannot_widen_its_own_effect_ceiling`,\n  `::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read`,\n  `::a_destructive_tool_needs_a_named_approver`\n- PR #666, merge `a2ef9194eb2c`; `actions:tools` = `exists` in `docs/evidence/v2-coverage.md`.\n\n## Verification\n\n`cargo test -p conveyor-actions --test tool_authoring --test byo_tools` on a checked-out main; the\nCI equivalent is the PR's `preflight` and `ci-public` at the merge commit.\n\n## Links\n\n- `docs/plans/inception/T15/00-project-brief.md`, `01-inception-analysis.md`,\n  `02-architecture-design.md`\n- Issue #86; PR #666; `docs/evidence/v2-coverage.md`.\n\n## Gap\n\n`/tools` is the V1 legacy surface and is not evidence for this project. A promotion returns\n`requested`; no platform version is stamped. Caller-supplied URLs and an archived-kag binding are\nnot claimed. No other delivery gap is named by the record.\n",
  "card_id": null,
  "category": "evidence",
  "content_hash": "1125d15c8701469263f7912d7e32220251ead5623db55fe05a0652e8c6b19a05",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:33.783785Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121b-60eb-71db-a750-c48bfd0c5112",
  "kind": "document",
  "meta": {
    "category": "evidence",
    "content_type": "text/markdown",
    "sha256": "1125d15c8701469263f7912d7e32220251ead5623db55fe05a0652e8c6b19a05",
    "size_bytes": 1904,
    "unit": "T15"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d37-7f20-9959-979c31daeb5e",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:39.033041Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 1904,
  "status": "active",
  "title": "Evidence \u2014 T15 Tool authoring and BYO",
  "unit": "T15",
  "updated_at": "2026-11-25T09:48:39.033041Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```
