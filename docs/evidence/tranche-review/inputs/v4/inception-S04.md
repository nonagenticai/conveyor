# Inception analysis — S04 action registry (retrospective, 2026-11-28)

## Current state

Before the slice's own build, the service carried one hand-written route per operation and a
generated OpenAPI document, with no single declaration that could be projected into every surface.
The store and the tenancy model already existed under `crates/store`, the router and middleware
already carried the tenant bearer and the idempotency key, and the registry table the invocation
path writes was already created by `migrations/0003_action_registry.sql`. The slice's own record
(`docs/evidence/rework/S04.json`) was red before the retrospective: the live delivery contract was
absent and seven artefact-standard rows were partial. What existed was proven below the
declaration; what was missing was the declaration itself.

## Reuse sources

- The tenant-scoped store and its row-level security (`app_current_tenant()`) were reused unchanged
  for the registry and the audit ledger; the slice did not add a second persistence path.
- The router and middleware were reused as the invocation transport: the tenant bearer, the
  idempotency key, the deadline and the transaction come from the existing path, not a new one.
- The RFC 9457 error envelope was reused for schema-invalid input, so a refusal has one shape on
  every surface.
- `served-spec.lock.json` and the `--check-lock` path were reused as the drift gate for the OpenAPI
  projection.
- The MCP tool surface already present in the service was reused as the fourth projection rather
  than introducing a second tool registry.

## Build vs reuse

Built: the `ActionSpec` declaration and the derive crate (`crates/actions-derive`) that emits it,
the resolver (`crates/actions/src/resolve.rs`), the manifest, OpenAPI and MCP projections with
their boot-time collision checks, and the external-action ingest from a declared OpenAPI document.
Reused: everything below the declaration — the store, the tenancy, the idempotency, the error
envelope and the lock. The line was drawn at "one declaration per invocable operation": the slice
did not rebuild the transport, the store or the auth path, because those were proven by earlier
slices and must not fork.

## Risks

- Projection divergence: a change to one projection silently changing another. Mitigated by the
  parity test `crates/actions/tests/parity_http_mcp.rs`, which compares `http.body` with
  `mcp.structuredContent`, and by the one `spec_hash` carried across the projections.
- Boot-time collisions: duplicate names, MCP name collisions and out-of-charset names fail closed
  at boot rather than shadowing a route or a tool.
- External ingest is a trust boundary: a declared document carrying a schema the closed set cannot
  express is refused when the document is ingested, not when the action is first called.
- Drift in CI: the served spec and the lock are compared on every run, so a hand edit to the
  OpenAPI document fails the check instead of publishing a second source of truth.
