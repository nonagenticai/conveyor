# T2 — the superseding delivery contract and its Phase-G re-review

**Recorded 2026-11-22; corrected 2026-11-26.** The original note recorded a compliant re-review
that carried `v1-w1c-3` and `v1-w1c-4` as deviations. Both are now resolved on the lane
(`864e71c4` and `78cdf4eb`), and every one of the six findings the 2026-11-21 review admitted has
a resolution commit and a test. The 2026-11-26 re-review therefore records `compliant` with no open
finding — the earlier note's deviation table is superseded, not re-dated.

A compliance review is unique per contract and immutable (`review_for_contract_tx` reads one), so a
stale level cannot be replaced in place. T2 held two live contracts from earlier passes
(`01a2005a`, the retrospective SoAW, and `01a217f8`, a delivery contract carrying the stale
`non_conformant` review). Both were superseded through `architecture.contracts_supersede`; the
factory then recreated the SoAW and the delivery contract from the corrected pack and recorded the
re-review on the new contract.

## What the re-review records

**Level:** `compliant`. Every admitted finding is resolved at its cited commit and test:

| Finding | Disposition | Commit | Test |
|---|---|---|---|
| `v1-t2-1` (P0) | fixed | `920a1dcb` | `documents_compose_gate.rs` |
| `v1-t2-2` | duplicate of `v1-w1c-3`, resolved | `864e71c4` | `decision_ladder::a_confidence_posted_directly_cannot_produce_a_laya_decision` |
| `v1-t2-3` (P1) | fixed | `fcb96fa9` | `decision_ladder.rs` |
| `v1-t2-4` | duplicate of `v1-w1c-4`, resolved | `78cdf4eb` | `decision_ladder::a_caller_cannot_supply_its_own_options` |
| `v1-t2-5` (P2) | fixed | `624abeb4` | `tool_budget.rs` |
| `v1-t2-6` (P1) | fixed | `1167127d` | `ladder_human_gate.rs` |

The re-review also found and fixed one defect of its own at `c6341a8a`:
`decision_policies.update` could not clear `fire_when` because serde read JSON `null` as an absent
member (`decision_ladder::a_policy_fire_when_is_validated_and_can_be_cleared`).

**Evidence:** `acceptance/programme/T2.toml`; `docs/evidence/v1-conformance-findings.json`
(`resolved_by` per finding); the four cited targets re-run at `c6341a8a` — decision_ladder 26/26,
ladder_human_gate 10/10, documents_compose_gate 7/7, tool_budget 14/14 — and the independent
receipt re-run on a fresh post-run snapshot.

## The rows (2026-11-26)

| Act | Action | Row |
|---|---|---|
| supersede the stale retrospective SoAW | `architecture.contracts_supersede` | `01a2005a` → superseded |
| supersede the stale delivery contract | `architecture.contracts_supersede` | `01a217f8` → superseded |
| fresh retrospective SoAW | `architecture.contracts_request` / `_approve` | `01a2180f-15fe` (live) |
| first fresh delivery contract | `architecture.contracts_request` / `_approve` | `01a2180f-1c53` (superseded by the revision) |
| delivery contract carrying `commitments` | `architecture.contracts_request` / `_approve` | `01a21810-707f` (live) |
| Phase-G re-review | `architecture.reviews_record` | `01a21810-7eab` (level `compliant`) |

The stale `non_conformant` review remains attached to the superseded `01a217f8`; that is history
and is not read by the live close gate.

## The chain

1. `architecture.contracts_supersede` on every live contract carrying a stale review.
2. The factory's step 6 requests a superseding delivery contract when the live body lacks a field
   the standard's page reads (measured here: `commitments`) and approves it with a board decision.
3. `architecture.reviews_record` on the new contract, level `compliant`, the dispositions above.
4. `projects.close` — closure decision by a distinct approver, then the close gate.
