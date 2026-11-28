# Definition of Ready — S04 Action registry

## Independent

Each deliverable ships alone: the ActionSpec derive crate and the registry/spec vocabularies
have separate acceptance and neither waits on the other's internals.

## Negotiable

The scope is the declaration path and the served spec; the screens (S12), the board (S05) and
the tool platform (T13) are named out of scope, not pre-designed here.

## Valuable

One declaration serves every transport: 549 actions, 580 served operations locked by hash, and
a single invocation path with idempotency and audit.

## Estimable

The work is bounded by the `acceptance/programme/S04.toml` gate checks — derive, manifest,
OpenAPI, MCP, invocation, external ingest — each executable against live rows.

## Small

Two deliverables under one change request: the derive crate and the registry and spec
vocabularies. No third deliverable is invented to justify the slice.

## Testable

Every clause names its check: the manifest's checks, `crates/actions/tests/parity_http_mcp.rs`,
and the served-spec lock check.

## Dependencies

S02 (the skeleton) and S03 (the domain schema) are delivered; tranche 3 is authorised; issue #3
is the unit. The closure depends on the tranche-3 review (this review) and the frozen baseline
`01a20256-44d0-731d-b345-fffc5f3ac9e8`.
