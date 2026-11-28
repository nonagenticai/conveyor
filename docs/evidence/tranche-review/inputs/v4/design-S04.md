# Architecture design — S04 action registry (retrospective, 2026-11-28)

## Context and boundary

This design owns the action declaration and its four projections, the invocation path and the
external-action ingest. It must not duplicate the store, the tenancy model, the router or the auth
path; it sits above them and consumes them. The blueprint it fits is the one the requirements
catalogue and the solution concept diagram name: one declaration, one resolver, one audit ledger.

## Aggregates, tables and actions

- Aggregates: the action specification (name, version, kind, scopes, effect, confirm, timeout and
  tenancy) and the invocation and audit rows it writes.
- Tables: the registry and audit rows created by `migrations/0003_action_registry.sql`; the
  invocation ledger is the same ledger every governed call uses.
- Actions: `actions.list` and `actions.describe` for reading the registry; every internally
  declared action reaches its operation through the resolver; an ingested external action is
  invoked through the same surface once its binding exists.
- Events: one audit row per invocation; the lock check at build; the boot-time collision refusal.

## Decisions

- One `spec_hash` per action across the manifest, the OpenAPI operation and the MCP tool; a
  mismatch fails the boot rather than publishing a divergent projection.
- Names fail closed: duplicate names, MCP collisions and out-of-charset names are refused at boot.
- A schema-invalid input returns RFC 9457 with a populated `errors[]` and never a 500.
- An idempotent replay yields one invocation row: the idempotency key is a property of the call,
  not of the payload.
- `served-spec.lock.json` catches drift; a hand-edited OpenAPI document fails the check.

## Acceptance

`acceptance/programme/S04.toml` carries the gate. The proof is `crates/actions/tests/parity_http_mcp.rs`
(20 actions identical over HTTP and MCP), `crates/actions/tests/action_resolution.rs` (resolution
and refusal), the served-spec lock check and the external fixtures under
`crates/actions/tests/fixtures/external/`. The close gate reads the declared outputs project-brief
and evidence, the closure review, the live delivery contract and its Phase-G review, the approved
scope baseline and the seven A–D artefacts.

## Architecture artefacts

The design's record is the seven Phase A–D artefacts: the stakeholder map, the driver, goal and
objective catalogue, the requirements catalogue, the solution concept diagram, the business
capability map, the data entity catalogue and the technology standards catalogue. Each is a typed
row with its entries and links, and each is published as its own page by the artefact standard.
