# Definition of Done — S04 Action registry

## Acceptance criteria

The `acceptance/programme/S04.toml` gate executes green: the ActionSpec derive crate exists and
registers its actions; the registry and spec vocabularies are generated; HTTP and MCP parity
holds; the served-spec lock matches; the external ingest fold round-trips.

## Evidence at completion

The acceptance receipt (`verify-project.py --project S04`) at the deployed sha; the published
project brief, evidence and closure review; the two deliverables accepted under change
`01a2054b-3f86-7959-b2ad-f1edc5b0bb55`; the tranche-3 demo credits on the tranche-3 cadence.

## Repo gate

`cargo fmt --all -- --check`, `cargo metadata --locked`, and the repository's preflight are the
compile verdict; the PR's CI and its merge are the final gate. No local compile.

## Merge and closure

One issue (#3), one branch, one PR that closes it; merged only on the full suite; the closure
gate decided by an actor other than its requester; the tranche review reads the receipt at the
snapshot's head sha.
