# T2 — the superseding delivery contract and its Phase-G re-review

**Recorded 2026-11-22.** T2's live delivery contract `01a2005c-18e4-75a7-9c20-2d83728194e0` carries
a Phase-G review at level `non_conformant` (`01a2005d-20eb-72af-a5de-4606c0b16008`). That level was
honest when it was recorded: it named `v1-t2-1` (P0) — `documents.compose` attached a generated draft
to a card with no gate. The P0 is fixed at `920a1dcb` with `crates/actions/tests/documents_compose_gate.rs`.
A level is fixed per contract and `review_for_contract_tx` reads one review per contract, so the
correction is a superseding contract with the re-review at the new head — never a second review
attached to the old one.

## What the re-review records

**Level:** `compliant`, with the deviations below recorded in its findings. The policy applied is the
one written in the programme plan: `compliant` when no P0 is open and no mandatory artefact is
absent; `non_conformant` only for an open P0 or an absent mandatory artefact.

**Evidence:** the acceptance manifest `acceptance/programme/T2.toml` (which now asserts
`status = "closed"`), the full-fidelity snapshot built from the live database, the fix commit
`920a1dcb` and its test, and `docs/evidence/rework/tranche-2/lessons.md`.

## The deviations carried, not waived away

| Finding | Severity | What is still true |
|---|---|---|
| `v1-w1c-3` | P1 | L1 never calls laya: `ladder.evaluate` stamps `rung=laya` on a caller-supplied answer; no T2 path issues `POST /v1/systemone`, so the recorded L1 decision is a shape, not an inference. |
| `v1-w1c-4` | P1 | Options and questions are inline request text, not authored, versioned rows; the answer is never checked against a stored option set. |
| `v1-p101-1` | P1 | A per-step judge verdict is a second decision store beside the ladder. |
| `v1-t2-5` | P2 | The budget refusal code is `tool_budget_exceeded`, not the spec's `budget_exceeded`. |
| `v1-w1c-5` | P2 | `fail_policy` and the Unavailable outcome are stored but never applied; a laya outage does not take the declared failure action. |
| `v1-p102-1` | P2 | laya's 3-second hard budget is enforced as 30 seconds. |
| `std-pmp-project-brief-t2` | P2 | The project brief is partial: the `Plan` section is missing. |
| `std-togaf-architecture-artifact-t2` | P2 | The architecture artefact is partial: `Metadata`, `Entries` and `Document` sections are missing. |

No finding above is P0. None is waived: they are recorded as deviations of a compliant
implementation, which is what a TOGAF Phase-G review exists to say. The two artefact findings are
the next artefact work on this project, tracked in the standards register.

## The chain

1. `architecture.contracts_request` — `delivery_contract`, phase `f`, `supersedes_id` = the old
   contract, body carrying this assessment.
2. `architecture.board_decide` — `approved`, `subject_kind = contract`, `subject_id` = the new
   contract, evidence naming the delegation.
3. `architecture.contracts_approve` — the board decision, acted by an actor other than the
   requester.
4. `architecture.reviews_record` — level `compliant`, the deviations above in `findings`, the
   manifest and snapshot in `evidence`, acted by an actor other than the requester.

Each write is read back; the rows are cited in the tranche-2 acts record.

## The rows

| Act | Action | Row |
|---|---|---|
| superseding contract | `architecture.contracts_request` | `01a2009a-7bce-7a8f-96fd-f5159286a09e` (live; `01a2005c` superseded) |
| board decision | `architecture.board_decide` | `01a2009a-a7fa-7a68-a539-32dc78e5a221` (approved, subject = the contract) |
| contract approval | `architecture.contracts_approve` | the same contract, approved by `conveyor-approver` ≠ requester |
| Phase-G review | `architecture.reviews_record` | `01a2009b-0768-72c1-bba1-2a8a3a8dae59` (level `compliant`) |
