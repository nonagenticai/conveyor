# Definition of Done — T13 Tool platform

## Acceptance criteria

The `acceptance/programme/T13.toml` gate executes green: a bound job calls coding through the
broker; a `tool_call` is recorded; a destructive call without the configured approvals is
refused; schema-invalid input is refused before the backend call; the storage adapter round-trips
a tenant-prefixed object with no S3 key in the prompt (partial — adapter-level only).

## Evidence at completion

The acceptance receipt (`verify-project.py --project T13`) at the deployed sha; the published
closure review with its per-clause verdicts and its seven named open items; the three
architecture artefacts' evidence (catalogue suite, broker tests, storage tests); CI at the merge
sha (`ci` and `merge_group` runs).

## Repo gate

`cargo fmt --all -- --check`, `cargo metadata --locked`, the repository's preflight, and the
PR's CI suite; CodeQL, delivery, board and issue-registry on the merge.

## Merge and closure

Issue #76 closed by PR #352's `Closes`; the close gate decided by a distinct approver; the
closure review published before the close; the tranche-3 review reads the receipt at the
snapshot's head sha.
