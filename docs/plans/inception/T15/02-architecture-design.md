# T15 — Architecture design

Project: T15 — Tool authoring and BYO · Track: v2 · Tranche: 12 · Plan: docs/SPEC_V2.md §12
Unit: #86 (this document is dependency-shaping work; blocked-by T13, T6 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T13", "T6", "P0"]

- **Status:** draft
- **Version:** 0.1
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** pending / pending
- **Related project:** T15 — Tool authoring and BYO · issue #86 · live document `01a1ee5f-5b42-70c5-9c35-86f977d14e85`
- **Rationale:** Define the to-be architecture for Tool authoring and BYO: aggregates, tables and actions, decisions, acceptance and the artefact set.
- **Published document:** `01a1ee5f-5b42-70c5-9c35-86f977d14e85` — `published_at` null (draft; not published)
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** context and boundary; aggregate/table/action rows; decisions; acceptance scenarios; PSA set; typed artefacts; rollout.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §11.5–§11.6, §12 T15 and P0 §5; realized by #86; reuse sources cited below; project dependencies T13, T6, P0.
- **Links:** issue #86; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Context and boundary

T15 adds the tool estate's authoring front door: OpenAPI draft import, service-scaffold registration, remote-MCP binding, tenant binding and promotion. T13 (#76) owns the four tool aggregates, the broker and `tools.call`; T14 imports the estate; T6 supplies service registration. T15 owns the draft lifecycle, both BYO adapters, tenant resolution and promotion.

Out of scope: tenant code execution, a second MCP server, caller-supplied URLs, re-opening V1 slices. One binary, one Postgres, no new port (8930); new tables, columns and actions only (§10.6).

## Aggregates, tables and actions

Table | State | Invariant | Source
---|---|---|---
`tool` | to create (T13 #76) | one operation version: closed schemas, effect, idempotency, timeout, redaction, destination | §11.8
`toolkit` | to create (T13) | immutable `key@version`; auth adapter, licence app, destination | §11.8
`toolkit.kind` (`native\|openapi\|mcp`), `scope`, `source_asset_id`, `source_hash` | to create (T15) | provenance pinned; a source change is a new version | §11.5, D2
`tool_binding` | to create (T13) | tenant-scoped; pinned versions; ceiling; credential alias | §11.8
`tool_binding.scope` (`job\|tenant`), nullable `job_binding_id` | to create (T15) | a tenant binding never widens a job binding | §12, D4
`tool_call` | to create (T13) | one row per call; no credential | §11.8
`approval_policy` | exists (`migrations/0050_approval_policies.sql:approval_policy`) | absent row = required + self-approval forbidden | §5.4
`approval`, `approval_decision` | exists (`migrations/0002_domain.sql:196,231`) | requester cannot decide its own gate | §5.4
`governed_asset` | exists (`migrations/0002_domain.sql:governed_asset`); kind CHECK to extend with `tool_draft` | a draft is not a published toolkit | §11.6, D2
`secret` | exists (`migrations/0013_documents.sql:secret`) | alias-only; no reveal | §11.8

Invariants: import writes no action row; publication creates an immutable version; an absent policy row requires a human; tenant toolkits stay scoped and promotion is no admin toggle; a binding ceiling is the minimum of job, tenant and operation effect, and no field raises it; remote MCP exposes only the bound subset and every call is one `tool_call`; publish refuses wildcard egress, free-form input, or a write without idempotency.

Action | State | Purpose | Trace
---|---|---|---
`approvals.request\|decide\|list` | exists (`docs/design/data/actions.json`) | gates | §5.4
`approval_policies.get\|list\|set` | exists | publication gate config | §5.4
`secrets.create\|get\|list\|rotate\|delete` | exists | credential alias sealing | §11.8
`tools.probe`, `tools.call` | to create (T13) | health; only call path | §11.3, §11.9
`toolkits.import_openapi` | to create | document → draft toolkit, closed schemas, effect default `read` | §10.4, §11.5(1)
`toolkits.import_mcp` | to create | remote endpoint → draft operations, bound subset | §11.5(2), D3
`tool_authoring.draft\|validate\|test\|review` | to create | schema/effect/egress/credential checks, replay, security review | §10.4, §11.6
`tools.publish\|retire` | to create | immutable publication behind the gate; retirement | §10.4
`tool_bindings.create\|update\|retire` | to create | job or tenant binding, ceiling, credential alias | §10.4, D4
`toolkits.promote` | to create | opens a kata/development request | §11.5, D5

Events (to create): `toolkit.imported`, `toolkit.published`, `tool_binding.created`; T13 adds `tool.called` (§11.3.7).

## Decisions

D1. Publication approval reuses `approval_policy`, keyed `tools.publish` (alternative: a toolkit policy table); consequence: human default, self-approval forbidden, per-action waiver.

D2. Drafts are `governed_asset(kind='tool_draft')` (alternative: `toolkit_draft`); consequence: one record per fact; publish writes toolkit and tools atomically.

D3. `toolkits.import_mcp` persists only bound operations, pinned as `source_hash` (alternative: proxy the full list); consequence: unbound tools invisible, drift is a new import plus kata.

D4. A tenant binding is `scope='tenant'`, `job_binding_id NULL`; the broker resolves job then tenant, lower ceiling wins (alternative: per-job clones); consequence: one grant, one-row revocation.

D5. Promotion is `toolkits.promote` → kata/development (alternative: admin copy); consequence: platform version reviewed and versioned; tenant version stays scoped.

D6a. "Tenant-configurable" is a policy-table extension, not a new store: `approval_policy` carries only `approval` and `self_approval` today (`migrations/0050_approval_policies.sql:approval_policy`), so approver kind, count and expiry are new columns on that table (to create at #86). Alternative: a second policy table — refused (P1).

D7. Promotion opens a kata/development request; the platform-catalogue promotion itself is out of scope per #86 and is never a toggle. Consequence: no authoring path stamps a platform version.

D8. OpenAPI 3.1 coverage is a risk, not an assumption: the pinned parser (`openapiv3` 2.2.0) may not parse every 3.1 document. Consequence: the import refuses an unsupported construct with a typed error, the fixture `crates/actions/tests/openapi31.rs` (to create at #86) proves parse-or-refuse on a 3.1 sample, and a 3.0 document is the fallback.

D6. Side-effect class reuses the built four-rank `Effect` (`read < write < spend < destructive`, `crates/actions/src/spec.rs:Effect`); a tool's spend ceiling is therefore the same rank as an action's. Alternative: a three-rank scale with spend as a budget tag — rejected because it would change a built enum and lose the ordering that stops a write ceiling permitting spend. Consequence: `apps/server/tests/tool_effect.rs:the_effect_scale_is_read_write_destructive` is the action-side proof and is not cited as tool-side proof; the tool-side proof is to create under #86.

## Acceptance

The gate (§12) reads verbatim: “a tool generated from a spec is published and called by a job; publication approval defaults to a human and is tenant-configurable; a BYO MCP server is bound, narrowed and audited; a tenant cannot exceed its effect ceiling”.

1. **A tool generated from a spec is published and called by a job.** A stored OpenAPI 3.1 document; `toolkits.import_openapi` drafts; `tool_authoring.validate` accepts two reads and one idempotent write; `tool_authoring.test` passes a replay; a human `approvals.decide` publishes `toolkit@1`; a bound job calls a read through `tools.call`, recording one `tool_call`.
   `Verify:` `crates/actions/tests/tool_authoring.rs::a_tool_generated_from_a_spec_is_published_and_called` (to create at #86).
2. **Publication approval defaults to a human and is tenant-configurable.** With no `approval_policy` row for `tools.publish`, publication needs `approvals.request` and a human `approvals.decide`; the author cannot decide. A tenant row waiving `tools.publish` publishes and audits.
   `Verify:` `crates/actions/tests/tool_authoring.rs::publication_waits_for_a_human_by_default` and `::a_tenant_policy_changes_the_publication_gate_without_a_code_change` (to create at #86); mechanism: `migrations/0050_approval_policies.sql:approval_policy`, `crates/actions/src/control/policies.rs:self_approval`, `crates/actions/src/control/approvals.rs:decided_by_kind`.
3. **A BYO MCP server is bound, narrowed and audited.** A remote endpoint exposing five tools; `toolkits.import_mcp` binds two and `tools.publish` publishes; discovery exposes two; an unbound call is refused before the network; each allowed call writes one `tool_call`, alias only, no secret.
   `Verify:` `crates/actions/tests/byo_tools.rs::only_the_bound_subset_is_callable_and_each_call_is_audited` and `::a_call_time_url_is_refused_and_a_byo_toolkit_is_tenant_scoped` (to create at #86).
4. **A tenant cannot exceed its effect ceiling.** A `tool_binding` with ceiling `read` and a tool classified `write`: the call is refused `403 tool_effect_forbidden` before the destination; an input field naming a higher ceiling changes nothing.
   `Verify:` `crates/actions/tests/tool_authoring.rs::the_publishing_job_cannot_widen_its_own_effect_ceiling` (to create at #86), mirroring `apps/server/tests/tool_effect.rs:effect_ceiling_is_never_raised`; the ceiling reader is `crates/actions/src/invoke.rs:binding_ceiling`.
5. **Refusals on publication.** An unbounded draft is refused, and a draft with no declared class defaults to `read`. `Verify:` `crates/actions/tests/tool_authoring.rs::publication_refuses_an_unbounded_draft_and_defaults_the_class_to_read` (to create at #86).
6. **A destructive tool needs a named approver.** `Verify:` `crates/actions/tests/tool_authoring.rs::a_destructive_tool_needs_a_named_approver` (to create at #86).
7. **Promotion is not a toggle.** `Verify:` `crates/actions/tests/tool_authoring.rs::promotion_is_not_an_admin_toggle` (to create at #86) — promotion opens a request; no authoring action stamps a platform version.
8. **A 3.1 document parses or is refused.** `Verify:` `crates/actions/tests/openapi31.rs` (to create at #86) — a known-3.1 sample either imports with closed schemas or refuses with a typed error naming the unsupported construct; never a silent partial import.

## Architecture artefacts

### PSA set at inception

Principles applied: P1 one record per fact; import writes no action row. P2 import produces a draft, never a decision. P3 draft/review/publish are actions with evidence. P4 publisher ≠ approver. P5 replay and probe precede publication. P8 unresolved binding, probe or policy refuses `409 tool_not_ready`. P10 no speculative aggregate; columns only on T13's rows. P11 approval and ceiling are tenant rows.

Requirements: R-T15-1 OpenAPI draft import, closed schemas, effect default `read` (§11.5(1), §11.6(1)). R2 service-scaffold registration (§11.6(2)). R3 remote MCP binding-scoped discovery (§11.5(2)). R4 tenant binding narrowed by job/tenant/operation (§11.1, §11.4). R5 human-default, tenant-configurable publication (§5.4, §12). R6 promotion via kata/development (§11.5). R7 one `tool_call`, no credential, drift opens a kata (§11.3, §11.9).

Stakeholder × concern matrix (Publication·Credential·Ceiling·Audit·Version·Tenancy; A/O/C/I):

Stakeholder | Cell
---|---
Tenant-admin | A/O·I·C·I·I·O
Job-author | C·I·C·I·C·I
Platform-security | A·C·C·C·C·I
Development-job | I·I·I·I·C·I
Operations | I·C·A·C·C·I
Architecture-Board | C·I·I·I·A·I

Driver/Goal/Objective: driver — tenant tools without a platform release, under platform control. Goal — self-service authoring under platform governance. O1 import a spec into a draft in one action. O2 bind a remote MCP subset with no credential exposure. O3 human-default, tenant-configurable publication approval.

Solution concept (edges): E1 source document → draft `governed_asset` (derivation); E2 draft → validation/replay (dependency); E3 review → `approvals.request|decide` (dependency); E4 approval → `toolkit@version` + tools (realization); E5 version → `tool_binding` (dependency); E6 binding → `tools.call` → `tool_call` (realization); E7 tenant toolkit → `toolkits.promote` → kata/development (dependency); E8 `approval_policy` → gate (reuse); E9 `secret` → alias resolution (reuse).

### Typed catalogue

TA-1 — T15 tool authoring capability catalogue. Type: catalog (one axis: capability) · Phase A · Plateau: PSA · Status: draft · Version: 1 · Author and date: T15 inception lane, 2026-11-18 · Reviewer and review date: this review, recorded on #347, 2026-11-18 · Owner: T15 project (#86) · Related project: T15 (#86), tranche 12 · Purpose, stakeholders, scope: one importable authoring capability per source kind under one approval gate, for tenant admins, job authors and platform security; covering OpenAPI, service registration, MCP, binding, promotion · Published document: this design, on acceptance to #86 · Links: derivation ← SPEC_V2 §11.5–§11.6; dependency → T13 (#76); reuse ← `approval_policy`, `secret`, `governed_asset`.

UID | Item | Description of use or structure | State | Unit
---|---|---|---|---
TA-1.1 | OpenAPI draft import | `toolkits.import_openapi` parses a document into draft tools, closed schemas, effect default `read` | to create | #86, needs T13
TA-1.2 | Service-scaffold registration | a workspace service registers its OpenAPI; the scaffold is T6's path | to create | #86, T6
TA-1.3 | Remote MCP binding | `toolkits.import_mcp` discovers a streamable-HTTP endpoint, bound subset only | to create | #86
TA-1.4 | Tenant binding and narrowing | `tool_bindings.create` with scope `job\|tenant`; minimum of job, tenant, operation ceiling | to create | #86, T13
TA-1.5 | Publication approval | `tools.publish` behind `approval_policy`; human default, tenant-configurable | to create | #86
TA-1.6 | Promotion path | `toolkits.promote` opens a kata/development request | to create | #86, T5/T6

## Migration and rollout

Sequence: T13 (#76) → T6 → T15. The T15 migration (to create, next after T13's) extends the `governed_asset.kind` CHECK with `tool_draft`, adds `toolkit.kind`, `scope`, `source_asset_id`, `source_hash`, and adds `tool_binding.scope` with nullable `job_binding_id`; forced RLS ships with it (P3). No policy seed: an absent `approval_policy` row is the safe default.

Rollout: register actions and regenerate the served-spec lock; publish one OpenAPI fixture; exercise service registration through a T6 job; enable BYO by policy row. Rollback: retire bindings then versions; drafts become `archived`/`superseded`; forward-only.

## Links

Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-4866-73b3-aff3-4b81913d1e23.html · Issue: https://github.com/nonagenticai/conveyor/issues/86 · SPEC_V2 §11.5, §11.6, §11.8, §12 T15, §10.4, §5.4, §8.1 · Artefact standard: https://conveyor.nonagentic.ai/design/docs/output-01a1e9d1-2720-776c-9741-a5deb322086e.html
