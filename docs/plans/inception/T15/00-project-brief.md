# T15 — Tool authoring and BYO

- **Status:** published
- **Version:** 1.0
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** #347 independent review / 2026-11-18; owner publication / 2026-11-21
- **Related project:** T15 — Tool authoring and BYO · issue #86 · live document `01a1e008-4866-73b3-aff3-4b81913d1e23`
- **Rationale:** state the inception basis for T15: a tool generated from an OpenAPI spec or a BYO MCP server is published, called by a job, and bounded by its effect ceiling and the tenant approval policy.
- **Published document:** `01a1e008-4866-73b3-aff3-4b81913d1e23` — published 2026-11-21
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** outcome, boundary, reuse sources, inception units, dependencies, acceptance, capabilities, plan, risks, links.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §11.5–§11.6 and §12 T15, and P0 §4; realized by #86; reuse sources below; depends on T13, T6 and P0.
- **Links:** issue #86; `docs/SPEC_V2.md` §12; `01-inception-analysis.md`; `02-architecture-design.md`; `seed/programme.json`.

## Outcome

A tool generated from an OpenAPI spec or a BYO MCP server is published, called by a job, and bounded by its effect ceiling and tenant approval policy.

## Boundary

In scope: toolkit import (draft `toolkit`/`tool` rows), remote-MCP discovery as a client, tenant-binding resolution in the broker, and the promotion request — as toolkit rows, never as new actions.

Out of scope: executing tenant code; a caller-supplied URL at call time (§11.5); the agent platform's MCP server as a dependency (§11.7); binding any archived kag component (R19); a second control plane or a second OpenAPI reader; T13's broker and tool tables (reused, not forked).

## Reuse sources

- `crates/actions/src/ingest.rs` — OpenAPI parsing, `DiscoveredAction`, the breaking-change diff; reuse both, never a second reader.
- `crates/actions/src/mcp.rs` + `rmcp` 3.9.0 — the streamable-HTTP transport and sessions; reuse as a client.
- `migrations/0050_approval_policies.sql` — the per-tenant, per-action approval policy; the publication key.
- `migrations/0013_documents.sql:secret` — the sealed credential store; alias-only, no reveal path.
- `migrations/0002_domain.sql:governed_asset` — the draft-lifecycle parent; decide whether a tool draft extends it or a new aggregate is needed.
- Estate capability, by role: the coding fleet's OpenAPI 3.1 documents and effect-classed operations — import fixtures, not a second control plane.
- Estate capability, by role: the agent-platform MCP server's catalog shape — immutable versions, binding-scoped discovery.
- Upstream: the workspace crates — no new dependency without a workspace decision.

## Inception units

1. `inception-analysis` — `01-inception-analysis.md`; live `01a1ee5f-51f8-7613-8d20-35d470d53ecc` (published 2026-11-21).
2. `architecture-design` — `02-architecture-design.md`; live `01a1ee5f-5b42-70c5-9c35-86f977d14e85` (published 2026-11-21).

Both are published (owner publication, 2026-11-21); the inception gate is met.

## Dependencies

Typed, from `seed/programme.json`: blocked-by T13, T6 and P0; related #86 (delivery). T13 owns `tool`/`toolkit`/`tool_binding`/`tool_call`; T6 owns the approval-policy use. The delivery unit is `status:blocked` because tranche 12 is not authorised.

## Acceptance

The project gate: a tool generated from a spec is published and called by a job; publication approval defaults to a human and is tenant-configurable; a BYO MCP server is bound, narrowed and audited; a tenant cannot exceed its effect ceiling.

Verify: #86's scenarios (`crates/actions/tests/` targets, to create there); at inception, the published analysis+design pair, not yet satisfied.

## Capabilities

`development`, `design` — the tool authoring surfaces; no other capability may grant a tool binding.

## Plan

`docs/SPEC_V2.md` §11.5–§11.6 and §12 row T15. Delivery unit #86; no separate `S##` slice file. Target tranche **12 — not authorised**.

## Risks

- A ceiling gap between actions and tools until T13/T15 land → check: neither exists today.
- OpenAPI 3.1 fidelity: `openapiv3` 2.2.0 may not parse 3.1 → check: a 3.1 fixture (to create at `crates/actions/tests/openapi31.rs`).
- Tool-as-action namespace leak → check: toolkit import writes no `action_registry` row.
- MCP discovery widening → check: a BYO test (to create at `apps/server/tests/byo_mcp.rs`).
- Unconfigurable approval: `approval_policy` refuses an action name no registered action carries → check: `tools.publish` is absent from `actions.json` today.
- Credential leakage into the ledger → check: the `tool_call` schema has no credential field (to create with T13).
- Seed claims read as built → check: `git grep 'toolkit coding@4'` matches only `seed/organisation.json`.

## Links

- Issue #86 (delivery; blocked).
- `docs/SPEC_V2.md` §11.5–§11.6, §12 T15.
- `seed/programme.json` row T15.
- `01-inception-analysis.md`, `02-architecture-design.md`.
