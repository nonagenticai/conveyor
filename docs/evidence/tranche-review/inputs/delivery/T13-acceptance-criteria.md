# Acceptance Criteria — T13 Tool platform

## Scenarios

- **T13-1 broker path.** Given a bound job, when it calls coding, then the declared
  `coding_jobs.create` destination is reached once and no ad-hoc backend call is made.
- **T13-2 ledger.** Given a completed call, when the record is read, then exactly one `tool_call`
  row and one `tool.called` event exist.
- **T13-3 destructive gate.** Given a destructive call without the tenant's approvals, when it is
  invoked, then 403 `approval_required` is returned with zero destination hits and no ledger row.
- **T13-4 schema.** Given input violating a closed schema, when the tool is called, then 422
  `tool_input_invalid` is returned before the backend call (zero hits).
- **T13-5 storage.** Given a job object, when it is put and read back, then the key is
  tenant-prefixed, the hash verifies, and the compiled prompt carries no S3 key (partial:
  adapter-level, in-memory by default).

## Verify commands

- `python3 scripts/verify-project.py --project T13 --snapshot <snap>/data`
- `cargo test -p conveyor-actions --test tool_platform` (in CI)
- `cargo test -p conveyor-actions --test s3_wire` (in CI)

## Results

Measured at the tranche-3 review's snapshot: clauses 1-4 pass on merged tests; clause 5 is
partial (adapter-level only, no job-facing action, static prompt proof). The verdicts are the
closure review's table.

## Status

Accepted and closed: issue #76 closed 2026-11-18 by PR #352 at `a409efe3`; the seven open items
are recorded in the project register and carried by T14/T15 — none is treated as satisfied.
