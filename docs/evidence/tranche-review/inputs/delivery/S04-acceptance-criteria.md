# Acceptance Criteria — S04 Action registry

## Scenarios

- **S04-1 derive.** Given the action crates, when the derive macro generates the registry, then
  every declared action appears once with its scopes, effect and approval target.
- **S04-2 manifest.** Given the registry, when the manifest is generated, then every action's
  input schema validates and no action is declared twice.
- **S04-3 OpenAPI.** Given the registry, when the served document is produced, then every
  action's operation is present and the lock hash matches.
- **S04-4 MCP.** Given the same registry, when the MCP surface is enumerated, then the actions
  and their schemas agree with HTTP.
- **S04-5 invocation.** Given an action call, when it is invoked through any transport, then the
  same idempotency and audit path runs.
- **S04-6 ingest.** Given an external payload, when it is folded, then the declared rows exist
  and a re-run is a no-op.

## Verify commands

- `python3 scripts/verify-project.py --project S04 --snapshot <snap>/data`
- `cargo test -p conveyor-actions --test parity_http_mcp` (in CI)
- `conveyor snapshot --check-lock` (in CI)

## Results

Measured at the tranche-3 review's snapshot: the six clauses are executed by the manifest's gate
checks and the named suite; the receipt's per-check verdicts are the record.

## Status

Accepted and closed: the two deliverables were accepted under change
`01a2054b-3f86-7959-b2ad-f1edc5b0bb55` and credited at the tranche-3 demo; the project is closed
against its frozen baseline.
