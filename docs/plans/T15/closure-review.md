---
document: T15/closure-review
version: 1
status: published
project: T15
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: closure-review
phase: G
plateau: capability
---

# T15 — Tool authoring and BYO: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| A tool generated from a spec is published and called by a job | `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called`; `::a_scaffolded_service_registers_its_own_openapi` | pass |
| Publication approval defaults to a human and is tenant-configurable | `tool_authoring.rs::publication_waits_for_a_human_by_default`, `::a_tenant_policy_changes_the_publication_gate_without_a_code_change` | pass |
| A BYO MCP server is bound, narrowed and audited | `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited`, `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped` | pass |
| A tenant cannot exceed its effect ceiling | `tool_authoring.rs::the_publishing_job_cannot_widen_its_own_effect_ceiling`, `::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read`, `::a_destructive_tool_needs_a_named_approver` | pass |
| The delivery is on main and gated | issue #86 closed by PR #666, merge `a2ef9194eb2c`; `actions:tools` is `exists` in `docs/evidence/v2-coverage.md` | pass |

## Evidence

- Delivery merge `a2ef9194eb2c` (PR #666); the `tool_authoring.rs` and `byo_tools.rs` scenarios.
- The inception set: `docs/plans/inception/T15/` (brief, analysis, design).
- Published inception documents: brief `01a1e008-4866-73b3-aff3-4b81913d1e23`, design
  `01a1ee5f-5b42-70c5-9c35-86f977d14e85`.

## Open items

- `/tools` is the V1 legacy surface; it is **not** evidence for T15 (`v2-coverage.md`).
- A promotion returns `requested`; it does **not** stamp a platform version — not claimed.
- Caller-supplied URLs are refused by design; an archived-kag binding is not claimed.
- The tranche-12 authorisation is **not** claimed here; it is the #526 programme chain's act.

## Decision

Close. Acceptance is met on the evidence above; the listed items are either design refusals or
other units' scope, and no tranche authorisation is manufactured.
