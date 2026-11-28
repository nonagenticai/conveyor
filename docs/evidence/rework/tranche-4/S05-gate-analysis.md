# S05 gate analysis — retrospective (2026-11-24)

The six functional gate properties of `docs/plans/slices/S05-board.md` §7 have no recorded run in the
programme record. The plan's W1 rule is explicit: a project whose scenario cannot execute is reworked but
not closed. S05 is therefore reworked and kept active; this analysis is the open verification item.

# Closure review — S05 Board (retrospective, 2026-11-24)

## Outcome vs acceptance

The slice gate is defined in plan §7 and summarised in `acceptance/programme/S05.toml`; the snapshot records no Phase-G review of any clause, so each is recorded honestly:

- **Property (a) replay fidelity.** Not evidenced. The plan requires 1,000 cards/10 lists re-imported through the real actions with `admin diff` printing zero deltas; no such run is recorded.
- **Property (b) one winner.** Not evidenced. The plan requires one 200 and one 412 with the current representation from two clients on the same `If-Match`; no `race_move` run is recorded.
- **Property (c) 1k load.** Not evidenced. The plan requires p95 ≤ 120 ms at 20 rps and 1,000 cards in five pages ≤ 500 ms; no k6 run is recorded.
- **Property (d) webhook shape.** Not evidenced. The plan requires `{event,data,prevData,user}` with the internal register event and `*Out` payloads; no `webhook_shape` run is recorded.
- **Property (e) SSE.** Not evidenced. The plan requires `card.moved` ≤ 200 ms to a second client live and on replay; no `sse_board` run is recorded.
- **Property (f) no second store.** Not evidenced. The catalog read forbidding `card_membership` and `attachment` is defined; no run is recorded.
- **Record and governance.** Red at the close: the project closed at 2026-11-21T01:05:43Z before tranche 4 was authorised (2026-11-24T17:04:34Z) and before the charter, contract, artefact set, Phase-G review and closure review existed. The accepted P0 issue 01a1fbad-0be3-7557-9402-cfb405a3147c records the out-of-sequence closure.

## Evidence

- Project 01a1d039-0d29-796d-967b-1e819a4dd40a, version 7, `status: closed` at 2026-11-21T01:05:43Z; `meta.measured_status: built`, `meta.plan_status: planned`, `meta.issues: []`, `meta.expected_outputs: [project-brief, evidence]`, `meta.closure_outputs: []`.
- Accepted P0 exception 01a1fbad-0be3-7557-9402-cfb405a3147c (2026-11-21T01:16:24Z).
- Tranche 4 01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce, created 2026-11-17T14:19:56Z, authorised 2026-11-24T17:04:34Z by the tranche-3 review 01a20e84-325b-7fef-8f40-50b5e53550e9 (continue).
- Tranche-4 refinement cadence 01a202a2-2705-7392-a2d1-03ca2b3a174e (2026-11-22T09:41:50Z) recording WSJF 10/8/10/5; the tranche-4 PI cadence lists S05 in its capacity.
- The project's only publication is the evidence page 01a1e686-f3ec-70d9-92f9-7dd6daef0c0f (published 2026-11-16T22:42:48Z).
- This rework records the charter, the Statement of Architecture Work, the delivery contract, the seven A–D artefacts, the WBS deliverable with its WSJF inputs, the Phase-G review, the expected outputs (`project-brief`, `evidence`) and this closure review.

## Open items

- **The six gate properties remain unproven.** Run the plan §7 gate and `acceptance/programme/S05.toml`, and record the outputs; until then the slice is non_conformant.
- **P1 checks.** `S05-lessons` and `S05-artefact-standard` are P1 in the acceptance instrument; they do not block the P0 closure gate but remain open.
- **Plan status.** The plan frontmatter and the project meta still say `planned` while `measured_status` says `built`; reconcile the plan when the gate is recorded.
- **Planka source gaps.** The fork is absent; the per-event envelope, `List.type` transitions and position seeding are UNVERIFIED and must be measured (plan trap 1). The live DB is five test cards and never evidence.
- **Scope exclusions.** Attachments and card-level memberships stay out; claims, assignee and stopwatch writes stay in S06; the UI stays in S12.

## Decision

Record the compliance level **non_conformant** and re-close S05 as a retrospective re-closure under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 and the accepted P0 exception 01a1fbad-0be3-7557-9402-cfb405a3147c, through `acceptance/programme/S05.toml` on its P0 checks, with the closure gate decided by an approver other than its requester. This decision records the record the project lacked; it does not claim the board slice's acceptance gate passed, and the six unproven properties are carried as open items to be verified through the same instrument.


## Retrospective closure drafted for the record-level rework

# Closure — S05 Board (retrospective, 2026-11-24)

- **Delivered (as recorded):** the board slice of `docs/plans/slices/S05-board.md` — nine tables, 39 actions, 15 events, the SSE topics, the webhook emitter and the board-export/v1 CLI. The project row records `meta.measured_status: built` but `meta.plan_status: planned` and `meta.issues: []`; no Phase-G review or gate run is recorded.
- **Closed:** 2026-11-21T01:05:43Z, version 7, in an unauthorised tranche before the tranche review; the accepted P0 issue 01a1fbad-0be3-7557-9402-cfb405a3147c records the retrospective exception under the standing delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14.
- **Tranche:** tranche 4 (01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce) was authorised at 2026-11-24T17:04:34Z by the tranche-3 review 01a20e84-325b-7fef-8f40-50b5e53550e9 (continue); the tranche-4 refinement cadence 01a202a2-2705-7392-a2d1-03ca2b3a174e records the WSJF inputs 10/8/10/5.
- **Rework:** publishes the charter, the Statement of Architecture Work, the delivery contract, the seven Phase A–D artefacts, the one WBS deliverable, the Phase-G review and this closure review, and re-publishes the expected outputs `project-brief` and `evidence`; the declared closure output is `closure-review`.
- **Decision:** re-close on the P0 checks of `acceptance/programme/S05.toml`, with the closure gate decided by an approver other than its requester, recording the level non_conformant because the board slice's six gate properties are not evidenced.
- **Not recoverable:** no contemporaneous lifecycle set existed and the close preceded the tranche-4 authorisation; this closure is retrospective and does not attest contemporaneous intent.

