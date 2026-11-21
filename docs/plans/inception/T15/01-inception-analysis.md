# T15 — Inception analysis

Project: T15 — Tool authoring and BYO · Track: v2 · Tranche: 12 · Plan: docs/SPEC_V2.md §12
Unit: #86 (this document is dependency-shaping work; blocked-by T13, T6 and P0)
Baseline: main@70d31773, 2026-11-21 (content reviewed at main@61edfcf8, 2026-11-18; citations re-verified at this baseline) · Project row: active, meta.depends_on = ["T13", "T6", "P0"]

- **Status:** draft
- **Version:** 0.1
- **Author / date:** conveyor programme session / 2026-11-21
- **Reviewer / review date:** pending / pending
- **Related project:** T15 — Tool authoring and BYO · issue #86 · live document `01a1ee5f-51f8-7613-8d20-35d470d53ecc`
- **Rationale:** Establish what exists and what is reused for Tool authoring and BYO: current state, build-vs-reuse decisions and the risks the architecture must answer.
- **Published document:** `01a1ee5f-51f8-7613-8d20-35d470d53ecc` — `published_at` null (draft; not published)
- **Contract:** pending
- **ADM phase / plateau:** A / PSA for the artefact (project row: G / capability)
- **Entries:** current-state rows, reuse sources, build-vs-reuse decisions, risks, open questions.
- **Derivation / realization / reuse / dependency:** derived from SPEC_V2 §11.5–§11.6, §12 T15 and P0 §5; realized by #86; reuse sources cited below; project dependencies T13, T6, P0.
- **Links:** issue #86; SPEC_V2 §12; the sibling inception document; `seed/programme.json`.

## Current state

- **absent — the tool-estate aggregates.** SPEC_V2 §11.8 names `tool`, `toolkit`, `tool_binding` and `tool_call`; no migration creates them, and no `tools.*` action resolves in `docs/design/data/actions.json`. T13 (#76) owns them; T15 is blocked by T13 and T6.
- **built — the action-level effect ceiling.** `crates/actions/src/spec.rs:Effect` defines read < write < spend < destructive (four ranks; `rank()` orders them); `crates/actions/src/invoke.rs:binding_ceiling` reads a ceiling from binding meta and only narrows; `apps/server/tests/tool_effect.rs:effect_ceiling_is_never_raised` proves a `read` ceiling refuses a `write` action with `403 effect_ceiling`. It guards action bindings, not tool bindings.
- **built — tenant approval policy.** `migrations/0050_approval_policies.sql:approval_policy` stores `approval required|waived` and `self_approval forbidden|allowed` per tenant and action name; an absent row is the safe default. `approvals.request|decide|list` and `approval_policies.get|list|set` resolve in `docs/design/data/actions.json`; `crates/domain/src/approval.rs:decided_by_kind` records who decided.
- **built — sealed credentials.** `migrations/0013_documents.sql:secret` stores ciphertext, key id, scope and version; `secrets.create|get|list|rotate|delete` are registered, and no action reveals a value.
- **partial — OpenAPI ingestion.** `crates/actions/src/ingest.rs:discover` parses a service's `openapi.json` into `DiscoveredAction` values; `ingest_production` and `apply` write them as `kind: external` rows in the action registry. This is action import, not toolkit import: it has no path to the tool estate.
- **built as server projection only — MCP.** `crates/actions/src/mcp.rs` serves `rmcp`'s streamable HTTP at `/mcp`; `Cargo.toml` enables features `server` and `transport-streamable-http-server` only. There is no MCP client, discovery or remote binding.
- **claimed — seed toolkit bindings.** `seed/organisation.json` names five bindings ("toolkit coding@4 · ceiling write · credential coding.tenant") but no table can hold one; these are design claims, not built paths.
- **absent/proposed — service scaffold and promotion.** SPEC_V2 §11.6 path 2 names `template-rust-service`, which has no path in this repo; §11.5's promotion path is prose only. Both are planned under #86.
- **built, unrelated — name collision.** `crates/actions/src/model/tools.rs` holds four architecture-model tools (`models.traceability`, `.gaps`, `.impact`, `.completeness`); it is not the tool estate and must not be reused.

## Reuse sources

| Source | Reuse candidate | Boundary | Analysis question |
|---|---|---|---|
| `crates/actions/src/ingest.rs` | OpenAPI parsing, `DiscoveredAction`, breaking-change diff | emits external actions, never toolkit rows | Which parser and diff does T15 reuse without duplicating the action namespace? |
| `crates/actions/src/mcp.rs` + `rmcp` 3.9.0 | streamable-HTTP transport and sessions | server at `/mcp`; a remote binding is an outbound destination | Can the transport be reused as a client without exposing the conveyor's server? |
| `migrations/0050_approval_policies.sql` | per-tenant, per-action approval policy | keys on a registered action name | Does `tools.publish` suffice as the key, or does BYO need a distinct one? |
| `migrations/0013_documents.sql:secret` | sealed credential store | alias-only; no reveal path | Can a tool credential alias resolve inside the broker with no read action? |
| `migrations/0002_domain.sql:governed_asset` | draft lifecycle parent | `kind` CHECK admits only requirement, principle, adr | Is `tool_draft` an extension of this row or a new aggregate? |
| wider estate — coding fleet gateway | OpenAPI 3.1 documents, effect-classed operations | a toolkit, not a second control plane | Which operations are import fixtures versus bound tools? |
| wider estate — agent-platform MCP server | catalog shape, immutable versions, binding-scoped discovery, credential broker, write-verify | reference only; never a runtime dependency (D44/D59) | Which concepts are adopted, which refused? |
| upstream — `openapiv3` 2.2.0, `rmcp` 3.9.0 | parsers and transport | closed schema subset; the client feature must be enabled | Which versions and licences are acceptable in an Apache-2.0 repo? |

## Build vs reuse

Reuse the approval, secret, document and governed-asset rows; reuse the `ingest.rs` parser and its breaking-change rule rather than a second OpenAPI reader; reuse `rmcp` transport. Build the toolkit importer (draft `toolkit`/`tool` rows, not actions), the remote-MCP discovery adapter, tenant-binding resolution in the broker, and the promotion request. Constraints: the repository is Apache-2.0 (`LICENSE`); third-party licences are checked at adoption; no tenant code executes and no caller supplies a URL at call time (§11.5); the agent platform's MCP server is never a dependency (§11.7); archived kag components are never bound (R19).

## Risks

- **Ceiling gap between actions and tools.** Until T13 and T15 land, a tool call has no ceiling; check: neither exists today.
- **OpenAPI 3.1 fidelity.** `openapiv3` 2.2.0 may not parse 3.1 documents, while §11.5 requires 3.1; check: a 3.1 fixture (to create at `crates/actions/tests/openapi31.rs`).
- **Tool-as-action namespace leak.** Extending `ingest.rs` to write toolkit rows would make a tool an action; check: review that toolkit import writes no `action_registry` row.
- **MCP discovery widening.** A full remote tool list would escape the binding; check: a BYO test (to create at `apps/server/tests/byo_mcp.rs`).
- **Unconfigurable approval.** `approval_policy` refuses an action name no registry action carries; check: `tools.publish` is absent from `actions.json` today.
- **Credential leakage into the ledger.** Check: the `tool_call` schema has no credential field (to create with T13).
- **Seed claims read as built.** Check: `git grep 'toolkit coding@4'` matches only `seed/organisation.json`.

## Open questions

1. Is `tools.publish` the right approval-policy key for both OpenAPI and MCP publication, or does BYO-MCP need its own?
2. Who is the "named approver" a destructive tool publication requires (§11.6) — tenant admin or platform security?
3. Does a tool's effect ceiling distinguish `spend` as `Effect` does, or is tool spend a budget tag beside the effect rank?
4. Does `openapiv3` accept 3.1, or is a second parser needed?
5. Does MCP discovery write draft `tool` rows immediately, or a document pinned at publish?

## Links

Brief: https://conveyor.nonagentic.ai/design/docs/output-01a1e008-4866-73b3-aff3-4b81913d1e23.html · Issue: https://github.com/nonagenticai/conveyor/issues/86 · SPEC_V2 §11.5, §11.6, §11.8, §12 T15, §10.4, §5.4, §8.1 · P0 reuse map: https://conveyor.nonagentic.ai/design/docs/output-01a1e006-fa29-756e-a710-90db4a232343.html
