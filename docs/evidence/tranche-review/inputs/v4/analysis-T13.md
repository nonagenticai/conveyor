# Architecture analysis — T13 tool platform (retrospective, 2026-11-28)

## Current state

Measured at the record, the tool platform gives every conveyor-internal run one governed way to
reach an external capability. The broker `tools.call` performs seven steps in order: resolve the
binding, check the effect class and budget, validate the closed input schema, resolve the
credential alias, call the registered destination, normalise and redact the result, and record and
emit the ledger row. The schema carries the tool, toolkit, tool_binding and tool_call tables;
effect classes `read`, `write`, `destructive` and `spend` drive tenant-configured approvals,
dry-run and dual control. Credential aliases resolve over sealed secrets, and the storage path is a
direct MinIO adapter writing tenant-, project- and run-prefixed objects. The proof is
`crates/actions/tests/tool_platform.rs`: a bound job calls coding through the broker, one call is
one ledger row, a destructive call without approvals is refused, a closed schema refuses before
the call, a job round-trips a tenant-prefixed object, and the compiled prompt carries no S3 key.

## Options considered

1. Let every run call its destination directly, with credentials in its own environment.
   Rejected: no single ledger, no effect gate, and credential use becomes unauditable.
2. Ad-hoc credential handling per tool versus aliases over sealed secrets. The alias layer was
   chosen so the call path never handles the raw secret and the compiled prompt cannot carry it.
3. Per-tool approval rules versus effect classes. Classes were chosen because the decision a
   caller faces is "what kind of effect is this", and a per-tool rule table drifts as tools are
   added.
4. A generic storage SDK versus the direct MinIO adapter with tenant prefixes. The adapter was
   chosen so the prefixing rule is in one place and the object round-trip is testable.

## Risks

- A tool binding pointing at an unregistered destination is refused before the call, so a broken
  binding fails closed rather than at the destination.
- A destructive call without its approvals is refused; a spend call carries dual control; both are
  proved by named tests rather than assumed.
- Credential leakage through the prompt is the sharpest risk; the compiled-prompt test is the
  standing check, and the alias resolves server-side only.
- Schema drift at the boundary: a closed schema refuses before the call, so an unexpected argument
  is a refusal the caller can see, not a partial call.
- The ledger is the only record: a call that bypasses the broker would be invisible, which is why
  the broker is the only path rather than a convention.

## Dependency map

The record declares `meta.depends_on: [T2, P0]`: T2's store carries the tables and the tenancy this
slice writes, and P0 supplies the programme definition. The tool platform is in turn a dependency
of the consumers that reach external capabilities — the SOP ingestion path
(`procedures.ingest_sop`), the coding service and the agent platform's external calls — all of
which go through the broker rather than a second call path. Inside the project the seven A–D
artefacts run from the stakeholder map through the requirements and the solution concept to the
data entity and technology standards catalogues.
