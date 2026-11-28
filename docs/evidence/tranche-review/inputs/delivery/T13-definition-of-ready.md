# Definition of Ready — T13 Tool platform

## Independent

The governed call path ships alone: the broker, the ledger, the destructive gate, the schema
validation and the storage adapter are one slice. The catalogue actions and the published
toolkits are named as carried by T14/T15, not designed here.

## Negotiable

The scope is the five gate clauses SPEC_V2 §12 T13 names; the storage clause is scoped to the
adapter and its proof, not to a job-facing action (recorded as that clause's partial verdict).

## Valuable

One governed path from a job to a backend: bound, broker-mediated, schema-checked, approval-gated,
ledgered, with a keyless tenant-prefixed object store.

## Estimable

Bounded by the manifest `acceptance/programme/T13.toml` and the seven scenarios in
`crates/actions/tests/tool_platform.rs`.

## Small

One project, five gate clauses, one proving suite; the catalogue and toolkit waves are separate
projects (T14 import, T15 authoring/publication).

## Testable

Every clause has a named test and the manifest's check; the close review records the verdicts
including the partial storage clause.

## Dependencies

S02 (the skeleton), S04 (the action registry the broker dispatches through) and the tranche-3
authorisation. The close depends on the Phase-G review and the closure gate; the open items are
recorded in the project register (R-T13-1..3, A-T13-1, D-T13-1).
