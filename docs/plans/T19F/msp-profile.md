---
document: T19F/msp-profile
version: 1
status: draft
project: T19F
category: benefit-profile
owner: BCM
reviewer: SRO
review_date: 2026-12-01
artifact_kind: benefit-profile
phase: Preliminary
plateau: definition
---

# T19F — MSP profile

## Business case

T19 delivered the meeting as the decision forum; its policy could not be changed through a
governance route and its two consumers — the T9 board and the T10 cadence — did not name the type
they use. T19F closes both seams by reusing the built authoring route and T19's arithmetic. The
cost is one migration, five actions, two test files and two UI panels; the alternative is a policy
that can only be changed by a redeploy and a board whose decisions are not seated.

## Benefit B7 — Deliberated cross-department decisions

B7's existing baseline (`seed/programme-definition.json`) is a measurement record: zero deliberated
decision rows before T19. T19F does **not** overwrite it. It adds the realisation increment:

| Measure | Before T19F | After T19F |
|---|---|---|
| Board subject decisions with per-seat positions | 0 (direct approval rows only) | a query over `approval_decision.via` for `surface='meeting'` |
| Cadence slots with a materialised meeting | 0 | a query over `meeting.cadence_slot` |
| Policy versions with an authoring provenance | 0 | `authoring_asset.published_meeting_type_id` |

The measurement method is the existing read-time aggregate over `meeting`, `meeting_agenda_item`,
`meeting_attendance`, `meeting_position`, `approval_decision` and `project_dependency`; T19F adds
the two joins (the authored policy and the cadence slot) and writes nothing at read time.

## Benefits map link

| Output | Capability | Outcome | Benefit |
|---|---|---|---|
| T19F forum governance | a forum's policy is versioned and its decisions are seated | O7 Meetings decide with the affected departments | B7 |

## Risk

| Id | Risk | Response |
|---|---|---|
| R20 (T19, carried) | a never-decided forum | the T19 retirement flag; T19F adds the sitting's own decision evidence |
| R21 | a policy change published without evidence | `authoring.review` requires an approved approval; the plan tests the refusal |
| R22 | a cadence slot gets a second meeting | the partial unique index; the action is idempotent |
| R23 | the board's direct path is broken by the sitting | the direct path is unchanged; the test proves it |

## Stakeholders

The SRO, the programme manager, the Architecture Board, the BCM, the department heads that staff
the board's seats, and the delivery teams that use the direct board path.

## Tranche evidence

T19F is at tranche 6 by the coverage graph (T19 tranche 4, T17 tranche 5). The tranche-6 review
reads: the plan and artefacts (#682), the implementation evidence (#683), the live chain and gate
(#684), and the closure review's deferral. The project stays `active` until that review; closing it
in a planned tranche would manufacture an unledgered P0.
