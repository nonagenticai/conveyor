---
document: T19F/closure-review
version: 3
status: published
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
| Migration 0073 is in the ledger and live | live `_sqlx_migrations` max 73, 72 rows (`docs/evidence/t19f/seed-readback.txt`) | pass |
| The registry, parity and OpenAPI lock agree | 587 actions; `registry`, `parity_http_mcp`, `--check-lock` at `eb0deb61` | pass |
| The design pack agrees | `check-data`, `check-coverage`, `check-docs`, `check-v2`, `check-console`, `build-docs --check` | pass |
| T19F is a live programme row with its board and B7 link | seed created 1 project + 1 board; `programmes.refresh` skipped 52; project `01a210ba-b6cb-76cc-82a3-a2b45cfb498f` | pass |
| **The live gate**: the v1 pin, the board sitting decided `via='meeting'`, and the cadence materialised once | `scripts/t19f-live-gate-check.py --check` — 13/13 assertions PASS (`docs/evidence/t19f/live-gate-check.txt`) | pass |
| **The policy v2 publish** through the reviewed authoring route | human gate `01a211d8-001f-7296-bbc1-ccb13835a3ef` decided by FR Test (`decided_by_kind='human'`, `via.channel='web'`); `authoring.review` + `authoring.publish` wrote v2 `01a211da-c391-7867-86dd-d22f9cfed5d5`; the v1 meeting still resolves v1 (`gate/05b-human-decision.md`, `gate/07-authoring-publish.md`) | pass |
| T19F's seven mandatory TOGAF artefacts are approved rows | `scripts/governance-chain.py --project T19F --check` — 7/7 approved, 24/24 checks pass | pass |
| T19F has live architecture contracts, requester ≠ approver | SoAW `01a210bd-ba25-78c1-a363-3eea5aa71335`, delivery contract `01a210bd-f408-778a-b67e-9b6cf36ca62a`, both board-decided | pass |
| Each live contract has a Phase-G compliance review | two `architecture.reviews_record` rows, level `conformant`, read back from the published `architecture.json` | pass |
| B7 has a measurement baseline and an owner, and this unit does not overwrite it | baseline read back unchanged (`gov/benefit-b7-baseline.md`) | pass |
| The TOGAF `architecture-artifact` standard is evidenced, not partial | published output `01a210be-6a72-77d4-ae4f-9ab9fa3c7f2a` with Metadata / Entries / Links / Document | pass |
| The strict conformance publish | red on pre-existing programme-wide debt (T5/T6/T8/T15/T17 P0s; S18/S21/SPEC-V2 ledger rows) and on T19F's declared active-in-planned-tranche P1; the first publish was report-only with the exact findings (`docs/evidence/t19f/publish-final.log`). The strict remediation and the strict publish are the follow-on conformance unit; this review does not claim them | **open (disclosed)** |
| One PR `Closes #684` merged | PR #711, merge commit `9ace4e81`; the human-review completion and the record refresh land on the close-out PR (#722) | pass |

## Evidence

- The live gate: `scripts/t19f-live-gate-check.py --check` — 13 PASS, 0 FAIL. The human gate
  `01a211d8-001f-7296-bbc1-ccb13835a3ef` was decided by FR Test (`decided_by_kind='human'`,
  `via.channel='web'`); `authoring.publish` wrote meeting_type v2
  `01a211da-c391-7867-86dd-d22f9cfed5d5`; the v1-pinned meeting still resolves v1
  (`docs/evidence/t19f/gate/05b-human-decision.md`, `07-authoring-publish.md`,
  `live-gate-check.txt`).
- The governance chain: `scripts/governance-chain.py --project T19F --check` — 24/24 (seven
  approved artefacts, two live contracts with a requester distinct from the approver, B7 read back
  and not overwritten, one Phase-G compliance review per contract, the published
  `architecture-artifact` document).
- The construction record: merges `bf0d9590` (PR #686), `eb0deb61` (PR #700), `9ace4e81` (PR
  #711), `ac115c81` (PR #715); `docs/plans/T19F/evidence.md` and `docs/evidence/t19f/`.
- The programme record: the live row (project `01a210ba-b6cb-76cc-82a3-a2b45cfb498f`, its board,
  B7) and the published pack.

## Open items

- The tranche-6 review owns the closure row and the benefit B7's incremental realisation.
- The strict conformance publish is the follow-on conformance unit (#726): it reconciles the
  programme-wide ledger and the declared records, and this review does not claim it.
- The tranche 2→4 authorisation chain is #526's unit; T19F adds no ledger entry of its own.

## Decision

Acceptance is met on every row except the strict conformance publish, which is disclosed above and belongs to the follow-on conformance unit (the product's own strict gate is red on programme-wide debt that predates T19F, not on T19F's evidence). The human review was the product's designed gate: the admin CLI records `service` and `authoring.review` refuses a machine-only review, so FR Test — the tenant's human `app_user` seat, never the owner's personal account — decided a fresh gate in the signed-in console. **Project closure is deferred to the tranche-6 review**:
T19F computes to tranche 6 from T19 (4) and T17 (5), and a project closed inside a planned tranche
manufactures a `project_in_unauthorised_tranche` P0. T19F therefore stays `active` until the
tranche-6 review records the closure, exactly as the plan's closure note declares.
