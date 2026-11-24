---
document: T19/evidence
version: 3
status: published
project: T19
category: evidence
owner: programme-manager
reviewer: Architecture Board
review_date: 2026-11-24
artifact_kind: evidence
phase: G
plateau: capability
---

# T19 — Decision forums: evidence

## Claim

Issue #606's D1–D8 are implemented on `issue/606-t19-meetings` and exercised on the deployed
engine (`a8f67c3e`): the meeting is a typed card whose agenda resolves existing approvals, whose
seats record positions, and whose outcome is derived by arithmetic over those positions and the
linked approval's stricter policy — with the human seat available in the web room, over Telegram
and over MCP, and the minutes a chaired governed asset. The live gate is recorded below.

## Evidence

Every D-part has its named test file and every scenario has a passing row; the commands below were
run at the branch head (the PR's CI is the merge gate).

| Part | Scenario | Evidence | Result |
|---|---|---|---|
| D1 | one live meeting per cadence slot | `crates/api/tests/meetings_policy.rs::a_second_live_meeting_in_a_slot_is_refused` | pass |
| D1 | the meeting is a card with its DoD and `meeting.opened` | `meetings_policy.rs::a_meeting_is_a_card_with_its_dod` | pass |
| D1 | the type version is pinned | `meetings_policy.rs::a_meeting_pins_its_type_version` | pass |
| D1 | a job seat writes one participant decision | `meetings_policy.rs::a_job_seat_writes_a_participant_decision` | pass |
| D2 | the item set is L0 over the sources | `meetings_agenda.rs::the_item_set_is_l0_over_the_sources` | pass |
| D2 | an item without evidence is 422 | `meetings_agenda.rs::an_item_without_evidence_is_refused` | pass |
| D2 | one item, one approval | `meetings_agenda.rs::an_item_names_exactly_one_approval` | pass |
| D3 | one live position per participant | `meetings_attendance.rs::one_live_position_per_participant` | pass |
| D3 | a missing department defers `inquorate` | `meetings_attendance.rs::a_missing_department_defers_inquorate` | pass |
| D3 | a closed item's position is immutable | `meetings_attendance.rs::a_closed_item_position_is_immutable` | pass |
| D4 | majority records each seat and keeps dissent | `meetings_decision.rs::majority_records_each_seat_and_keeps_dissent` | pass |
| D4 | a meeting never lowers dual control | `meetings_decision.rs::a_meeting_never_lowers_dual_control` | pass |
| D4 | a tie defers; observers never count | `meetings_decision.rs::a_tie_defers_and_observers_do_not_count` | pass |
| D4 | P4 excludes the judged seat | `meetings_decision.rs::the_conflict_rule_excludes_the_judged_seat` | pass |
| D4 | a unanimous reject resolves rejected | `meetings_decision.rs::a_unanimous_reject_resolves_rejected` | pass |
| D5 | a commitment needs owner and date | `meetings_outcomes.rs::a_commitment_needs_owner_and_date` | pass |
| D5 | an action becomes a card | `meetings_outcomes.rs::an_action_becomes_a_card` | pass |
| D5 | minutes need the chair and match the rows | `meetings_outcomes.rs::minutes_need_the_chair_and_match_the_rows` | pass |
| D5 | close refuses an open item | `meetings_outcomes.rs::close_refuses_an_open_item` | pass |
| D6 | the first decision across surfaces counts | `meetings_human_seat.rs::the_first_decision_across_surfaces_counts` | pass |
| D6 | the owner decides on web and MCP via meeting | `meetings_human_seat.rs::the_owner_decides_on_web_and_mcp_via_meeting` | pass |
| D6 | the ICS feed is a read | `meetings_human_seat.rs::an_ics_feed_is_a_read` | pass |
| D7 | participant-hours enter the roll-up | `meetings_metering.rs::participant_hours_enter_the_rollup` | pass |
| D7 | a never-decided forum is flagged | `meetings_metering.rs::a_forum_with_no_decision_is_flagged` | pass |
| D8 | a governance body convenes its standing meeting | `meetings_console.rs::a_governance_body_convenes_its_standing_meeting` | pass |
| D8 | a cadence slot resolves to a meeting | `meetings_console.rs::a_cadence_slot_resolves_to_a_meeting` | pass |

Supporting gates at the same head:

| Gate | Evidence | Result |
|---|---|---|
| the registry and the committed action dump agree (530 actions, 512 internal) | `cargo test -p conveyor-actions --test registry` | pass |
| HTTP and MCP return the same body for all 530 actions | `cargo test -p conveyor-actions --test parity_http_mcp` | pass |
| the migration ledger has `(65, "meetings")` | `cargo test -p conveyor-store migrate` | pass |
| the OpenAPI lock matches the 12 new operations | `conveyor --print-openapi --write-lock` ran; `served-spec.lock.json` updated | pass |
| the mock UI registers and renders the Meetings destination | `node docs/design/scripts/check-v2.cjs`, `check-console.cjs` | pass |
| the real app's Meetings destination has a Playwright spec | `apps/web/e2e/meetings.spec.ts` (empty, happy, refusal and phone/tablet layout) | runs in `web.yml` on the PR |

### The live gate — deployed `a8f67c3e`, tenant `nonagentic`, 2026-11-24

The human seat was exercised by the tenant's admin identity `FR Test` (app_user
`01a1cf1e-09dd-7580-99cc-de395f3e86cc`), supplied by the owner for this proof; the evidence names
the identity exactly and does not claim the owner's personal account. The meeting
`01a20c8a-33ac-7f2a-b02f-0d2165ef05d5` was scheduled on the T19 board, opened, the chair invited
and accepted, and two deciding items added (approvals `…5d47…` and `…bc7e…`). The seat approved
item 1 in the web room and item 2 through the public `/mcp` transport; each item was closed from
its own surface. The chair's minutes approval (`…a182`) was decided, the minutes governed asset
`01a20ca3-7d7a-72f0-b63d-96a09064c6df` published, and the meeting closed at
`2026-11-24T08:19:39Z`.

`python3 scripts/t19-live-gate-check.py --state docs/evidence/t19/live-gate.json --seed` reads the
rows back through the deployed read actions — never SQL, never a captured fixture:

| Check | Result |
|---|---|
| the live project is T19 and its board is a row | pass |
| one active `meeting` card type; benefit B7 is a row | pass |
| the meeting is `closed`; two agenda items closed as `decision` | pass |
| web: one meeting decision row, approval `approved`, `via.channel='web'` | pass |
| mcp: one meeting decision row, approval `approved`, `via.channel='mcp'` | pass |
| both rows name one approver, and it is the seated principal | pass |
| the minutes approval is `approved` (and `meetings.close` refuses without the published asset) | pass |

The gate found and closed two deployment defects, both in `gitops-production`:
`CONVEYOR_MCP_ALLOWED_HOSTS` was unset, so rmcp's localhost-only allowlist answered every public
handshake `403 Host header is not allowed` (#5429); and the MCP session lives in one pod's memory
while the two-replica Service round-robins, so 6 of 8 post-initialize calls 404'd until a
DestinationRule pinned the session on `mcp-session-id` (#5432). After the rollout, the same probe
served 8 of 8.

**Re-run on the current deployment.** The check was run again on `7e617673` (the T5 release, which
carries T19 unchanged): all 15 assertions passed, with the same approver
`01a1cf1e-09dd-7580-99cc-de395f3e86cc` on both decision rows. The gate is therefore evidenced on
the deployment that is running now, not only on the one it was first taken on.

### The T19 conformance chain — deployed `7e617673`, tenant `nonagentic`, 2026-11-24

The conformance audit named four T19-owned gaps. Three are closed here through the engine's own
action surface, under the published standing delegation
`01a1fb9f-ac2d-732d-b274-1079be3dd2b3`; the fourth is the programme's tranche chain (see Gap).

| Gap | Act | Read-back |
|---|---|---|
| seven mandatory TOGAF artefacts | `architecture.artifacts_create` + `artifacts_approve` on each of `requirements_catalog`, `stakeholder_map`, `driver_goal_objective`, `solution_concept`, `business_capability_map`, `data_entity_catalog`, `technology_standards` | seven `approved` rows for T19, entries from `docs/plans/T19/architecture-design.md` |
| artefact links | `architecture.artifacts_link` × 7 (derivation / realization / dependency, §5 of the design) | seven edges among the T19 artefacts |
| the live architecture contract | `architecture.contracts_request` → `architecture.board_decide` (Architecture Board, citing the delegation) → `architecture.contracts_approve` (approver distinct from requester) | `statement_of_architecture_work` `01a20ce0-5183-776c-84bd-327f2301b19b` is `live`, `approved_at` set, board decision `01a20ce0-644d-7340-a696-1af10fa8c61e` |
| the phase-F delivery contract | the same request → board decision → approval flow, `kind=delivery_contract`, phase `f` | `01a20cea-91bb-77f4-a880-044c90bb5f0e` is `live`, board decision `01a20cea-a68f-7e08-8d0c-399759ef16e0` |
| the Phase-G compliance reviews | `architecture.reviews_record` × 2 (one per live contract), level `conformant`, recorded by `conveyor-approver` under the delegation | both reviews carry the delegation ref and the named follow-ups; the audit's `compliance_review_missing` clears |
| B7's baseline and owner | `benefits.update` | baseline `{value: 0, unit, measured_at, method}` (zero deliberated decision rows before T19) and the programme-manager benefit owner |
| the `architecture-artifact` standard | `documents.compose` + `documents.publish` | output `01a20ce4-73e4-7009-9ec7-eb2265e9e84e` is published; its page carries Metadata / Entries / Links / Document |

`python3 scripts/t19-governance-chain.py --check` reads every row back through the deployed read
actions and passes all 16 assertions. The approvals were performed by the tenant's
`conveyor-approver` seat under the standing delegation, not by the owner personally; the contract's
requester is the operator's default actor, so the requester is never its approver. Each act's
write and read-back is under `docs/evidence/t19/gov/`.

## Verification

```bash
cargo test -p conveyor-api --test meetings_policy
cargo test -p conveyor-api --test meetings_agenda
cargo test -p conveyor-api --test meetings_attendance
cargo test -p conveyor-api --test meetings_decision
cargo test -p conveyor-api --test meetings_outcomes
cargo test -p conveyor-api --test meetings_human_seat
cargo test -p conveyor-api --test meetings_metering
cargo test -p conveyor-api --test meetings_console
cargo test -p conveyor-actions --test registry
cargo test -p conveyor-actions --test parity_http_mcp
cargo test -p conveyor-store migrate
npx playwright test apps/web/e2e/meetings.spec.ts
python3 scripts/t19-live-gate-check.py --state docs/evidence/t19/live-gate.json --seed
python3 scripts/t19-governance-chain.py --check
```

The Rust suites were run through `~/.claude/scripts/cargo-guard.py` on m4 (the estate's sanctioned
interactive route); the compile/lint verdict is the branch's GitHub-hosted `preflight.yml`, and the
PR's CI is the merge gate. The Playwright suite runs in `web.yml`, which is the first local-capable
run of the real app (no `cargo-leptos` is installed on m4, and installing it would compile locally).

## Links

- Plan: `docs/plans/T19-decision-forums.md`
- Issue: #606 (`Closes #606` in the PR); close-out: #650
- Specification: `docs/SPEC_V2.md` §6.8, §8.1–§8.3, §10.3–§10.5, §12 T19, D66
- Programme: `docs/design/Program_Architecture_and_Benefits_Realization.md`; B7 in `seed/programme-definition.json`
- Architecture: `docs/plans/T19/architecture-design.md`; MSP: `docs/plans/T19/msp-profile.md`; SAFe: `docs/plans/T19/safe-epic.md`
- Console: `apps/web/src/screens/meetings.rs`; design reference: `docs/design/v2.js` `meetings`
- Live gate state and read-backs: `docs/evidence/t19/`; checker: `scripts/t19-live-gate-check.py`
- Conformance chain: `scripts/t19-governance-chain.py`; per-act read-backs: `docs/evidence/t19/gov/`
- Delegation: `docs/evidence/t19/gov/` names the standing delegation `01a1fb9f-ac2d-732d-b274-1079be3dd2b3`
- Deployment fixes the gate required: gitops-production #5429 (MCP allowed hosts) and #5432 (MCP session affinity)

## Gap

None against T19's acceptance. The audit's one remaining T19-labelled finding is the **P0
`project_in_unauthorised_tranche`**: T19 is closed while tranche 4 is `planned`, and tranche 4's
entry needs the tranche 3 review — the programme's retrospective tranche chain, owned by #526. This
unit does not backfill those reviews; the strict-gate disclosure in the PR and the closure review
state it. Two declared follow-ups (the `meeting_type` authoring route and the T9 board / T10 cadence
binding) stay open items with owners in `docs/plans/T19/closure-review.md`.
