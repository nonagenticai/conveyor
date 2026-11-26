# T15 closure-review — publish

`documents.publish` for `01a2121b-2e58-7407-84a3-3329dc2c1a8c` (admin act, If-Match). The standard's required headings are checked on this transition.

```
== write: documents.publish ==
{
  "body": "---\ndocument: T15/closure-review\nversion: 1\nstatus: published\nproject: T15\ncategory: closure-review\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: closure-review\nphase: G\nplateau: capability\n---\n\n# T15 \u2014 Tool authoring and BYO: closure review\n\nWhether the project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\n| Acceptance criterion | Evidence | Result |\n|---|---|---|\n| A tool generated from a spec is published and called by a job | `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called`; `::a_scaffolded_service_registers_its_own_openapi` | pass |\n| Publication approval defaults to a human and is tenant-configurable | `tool_authoring.rs::publication_waits_for_a_human_by_default`, `::a_tenant_policy_changes_the_publication_gate_without_a_code_change` | pass |\n| A BYO MCP server is bound, narrowed and audited | `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited`, `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped` | pass |\n| A tenant cannot exceed its effect ceiling | `tool_authoring.rs::the_publishing_job_cannot_widen_its_own_effect_ceiling`, `::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read`, `::a_destructive_tool_needs_a_named_approver` | pass |\n| The delivery is on main and gated | issue #86 closed by PR #666, merge `a2ef9194eb2c`; `actions:tools` is `exists` in `docs/evidence/v2-coverage.md` | pass |\n\n## Evidence\n\n- Delivery merge `a2ef9194eb2c` (PR #666); the `tool_authoring.rs` and `byo_tools.rs` scenarios.\n- The inception set: `docs/plans/inception/T15/` (brief, analysis, design).\n- Published inception documents: brief `01a1e008-4866-73b3-aff3-4b81913d1e23`, design\n  `01a1ee5f-5b42-70c5-9c35-86f977d14e85`.\n\n## Open items\n\n- `/tools` is the V1 legacy surface; it is **not** evidence for T15 (`v2-coverage.md`).\n- A promotion returns `requested`; it does **not** stamp a platform version \u2014 not claimed.\n- Caller-supplied URLs are refused by design; an archived-kag binding is not claimed.\n- The tranche-12 authorisation is **not** claimed here; it is the #526 programme chain's act.\n\n## Decision\n\nClose. Acceptance is met on the evidence above; the listed items are either design refusals or\nother units' scope, and no tranche authorisation is manufactured.\n",
  "card_id": null,
  "category": "closure-review",
  "content_hash": "80a42675eab96067f328650d9a9bb5a4343b1c8fc636d18399b1c3eb5f48c860",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:20.840002Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121b-2e58-7407-84a3-3329dc2c1a8c",
  "kind": "document",
  "meta": {
    "category": "closure-review",
    "content_type": "text/markdown",
    "sha256": "80a42675eab96067f328650d9a9bb5a4343b1c8fc636d18399b1c3eb5f48c860",
    "size_bytes": 2413,
    "unit": "T15"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d37-7f20-9959-979c31daeb5e",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:26.030415Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2413,
  "status": "active",
  "title": "Closure review \u2014 T15 Tool authoring and BYO",
  "unit": "T15",
  "updated_at": "2026-11-25T09:48:26.030415Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
== read-back: documents.get ==
{
  "body": "---\ndocument: T15/closure-review\nversion: 1\nstatus: published\nproject: T15\ncategory: closure-review\nowner: programme-manager\nreviewer: Architecture Board\nreview_date: 2026-11-25\nartifact_kind: closure-review\nphase: G\nplateau: capability\n---\n\n# T15 \u2014 Tool authoring and BYO: closure review\n\nWhether the project delivered its acceptance, on its evidence, and the decision that follows.\n\n## Outcome vs acceptance\n\n| Acceptance criterion | Evidence | Result |\n|---|---|---|\n| A tool generated from a spec is published and called by a job | `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called`; `::a_scaffolded_service_registers_its_own_openapi` | pass |\n| Publication approval defaults to a human and is tenant-configurable | `tool_authoring.rs::publication_waits_for_a_human_by_default`, `::a_tenant_policy_changes_the_publication_gate_without_a_code_change` | pass |\n| A BYO MCP server is bound, narrowed and audited | `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited`, `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped` | pass |\n| A tenant cannot exceed its effect ceiling | `tool_authoring.rs::the_publishing_job_cannot_widen_its_own_effect_ceiling`, `::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read`, `::a_destructive_tool_needs_a_named_approver` | pass |\n| The delivery is on main and gated | issue #86 closed by PR #666, merge `a2ef9194eb2c`; `actions:tools` is `exists` in `docs/evidence/v2-coverage.md` | pass |\n\n## Evidence\n\n- Delivery merge `a2ef9194eb2c` (PR #666); the `tool_authoring.rs` and `byo_tools.rs` scenarios.\n- The inception set: `docs/plans/inception/T15/` (brief, analysis, design).\n- Published inception documents: brief `01a1e008-4866-73b3-aff3-4b81913d1e23`, design\n  `01a1ee5f-5b42-70c5-9c35-86f977d14e85`.\n\n## Open items\n\n- `/tools` is the V1 legacy surface; it is **not** evidence for T15 (`v2-coverage.md`).\n- A promotion returns `requested`; it does **not** stamp a platform version \u2014 not claimed.\n- Caller-supplied URLs are refused by design; an archived-kag binding is not claimed.\n- The tranche-12 authorisation is **not** claimed here; it is the #526 programme chain's act.\n\n## Decision\n\nClose. Acceptance is met on the evidence above; the listed items are either design refusals or\nother units' scope, and no tranche authorisation is manufactured.\n",
  "card_id": null,
  "category": "closure-review",
  "content_hash": "80a42675eab96067f328650d9a9bb5a4343b1c8fc636d18399b1c3eb5f48c860",
  "content_type": "text/markdown",
  "created_at": "2026-11-25T09:48:20.840002Z",
  "created_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "created_by_name": "FR Test",
  "external_ref": null,
  "id": "01a2121b-2e58-7407-84a3-3329dc2c1a8c",
  "kind": "document",
  "meta": {
    "category": "closure-review",
    "content_type": "text/markdown",
    "sha256": "80a42675eab96067f328650d9a9bb5a4343b1c8fc636d18399b1c3eb5f48c860",
    "size_bytes": 2413,
    "unit": "T15"
  },
  "parent_document_id": null,
  "project_id": "01a1d039-0d37-7f20-9959-979c31daeb5e",
  "publish_requested_at": null,
  "publishable": true,
  "published_at": "2026-11-25T09:48:26.030415Z",
  "published_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "published_by_name": "FR Test",
  "size_bytes": 2413,
  "status": "active",
  "title": "Closure review \u2014 T15 Tool authoring and BYO",
  "unit": "T15",
  "updated_at": "2026-11-25T09:48:26.030415Z",
  "updated_by": "01a1cf1e-09dd-7580-99cc-de395f3e86cc",
  "updated_by_name": "FR Test",
  "version": 1
}
```
