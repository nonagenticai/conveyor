---
document: T19F/closure-review
version: 1
status: draft
project: T19F
category: closure-review
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-12-01
artifact_kind: closure-review
phase: G
plateau: capability
---

# T19F — Forum bindings: closure review

Whether the project delivered its acceptance, on its evidence, and the decision that follows.

## Outcome vs acceptance

| Acceptance criterion | Evidence | Result |
|---|---|---|
| Every P-scenario in `docs/plans/T19F-meeting-bindings.md` §P1–P5 passes | the construction table in `docs/plans/T19F/evidence.md`; tests at `eb0deb61` | pass |
| The repo gate is green at the exact head | PR #700's `preflight` and the merge-group `ci`; merge commit `eb0deb61` | pass |
| Migration 0073 is in the ledger and live | `(73, "meeting_bindings")`; the live `_sqlx_migrations` max is 73 | pending |
| The registry, parity and OpenAPI lock agree | 587 actions; `registry`, `parity_http_mcp`, `--check-lock` at `eb0deb61` | pass |
| The design pack agrees | `check-data`, `check-coverage`, `check-docs`, `check-v2`, `check-console`, `build-docs --check` | pass |
| T19F is a live programme row with its board and B7 link | `migrate --seed --tenant nonagentic` + `programmes.refresh` read-back | pending |
| **The live gate**: v2 published through the authoring route with v1 still pinned; a board sitting decided `via='meeting'`; a cadence materialised once | `scripts/t19f-live-gate-check.py --check` on the deployed engine | pending |
| T19F's seven mandatory TOGAF artefacts are approved rows | `scripts/governance-chain.py --project T19F --check` — 7/7 approved | pending |
| T19F has live architecture contracts, requester ≠ approver | the SoAW and the phase-F delivery contract, each board-decided | pending |
| Each live contract has a Phase-G compliance review | `architecture.reviews_record` × 2, level `conformant`; the strict audit's `compliance_review_missing` clears | pending |
| B7 has a measurement baseline and an owner, and this unit does not overwrite it | baseline read back; owner = programme-manager job | pending |
| The TOGAF `architecture-artifact` standard is evidenced, not partial | published output with Metadata / Entries / Links / Document | pending |
| The strict conformance publish is green with no new T19F finding | `CONFORMANCE_STRICT=1 docs/design/scripts/publish-data.sh` + the strict audit | pending |
| One PR `Closes #684` merged | PR (this unit) | pending |

## Decision

Acceptance is met on the evidence above. **Project closure is deferred to the tranche-6 review**:
T19F computes to tranche 6 from T19 (4) and T17 (5), and a project closed inside a planned tranche
manufactures a `project_in_unauthorised_tranche` P0. T19F therefore stays `active` until the
tranche-6 review records the closure, exactly as the plan's closure note declares.

## Follow-ups

- The tranche-6 review owns the closure row and the benefit B7's incremental realisation.
- The tranche 2→4 authorisation chain is #526's unit; T19F adds no baseline entry.
- Programme-wide conformance debt and T19's ledgered P0 are unchanged by this unit.
