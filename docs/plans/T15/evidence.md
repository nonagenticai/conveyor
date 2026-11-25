---
document: T15/evidence
version: 1
status: published
project: T15
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-25
artifact_kind: evidence
phase: G
plateau: capability
---

# T15 — Tool authoring and BYO: evidence

## Claim

T15 delivers the tool-authoring route: a tool generated from a spec is published and called, the
publication approval defaults to a human and is tenant-configurable, a BYO MCP server is bound,
narrowed and audited, and no publishing job can exceed its effect ceiling.

## Evidence

- `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called`,
  `::a_scaffolded_service_registers_its_own_openapi`
- `::publication_waits_for_a_human_by_default`,
  `::a_tenant_policy_changes_the_publication_gate_without_a_code_change`
- `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited`,
  `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped`
- `::the_publishing_job_cannot_widen_its_own_effect_ceiling`,
  `::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read`,
  `::a_destructive_tool_needs_a_named_approver`
- PR #666, merge `a2ef9194eb2c`; `actions:tools` = `exists` in `docs/evidence/v2-coverage.md`.

## Verification

`cargo test -p conveyor-actions --test tool_authoring --test byo_tools` on a checked-out main; the
CI equivalent is the PR's `preflight` and `ci-public` at the merge commit.

## Links

- `docs/plans/inception/T15/00-project-brief.md`, `01-inception-analysis.md`,
  `02-architecture-design.md`
- Issue #86; PR #666; `docs/evidence/v2-coverage.md`.

## Gap

`/tools` is the V1 legacy surface and is not evidence for this project. A promotion returns
`requested`; no platform version is stamped. Caller-supplied URLs and an archived-kag binding are
not claimed. No other delivery gap is named by the record.
