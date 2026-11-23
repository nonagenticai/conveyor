---
document: T19/safe-epic
version: 1
status: draft
project: T19
category: epic
owner: programme-manager
reviewer: SRO
review_date: 2026-11-30
---

# T19 — SAFe epic

## Epic

**Hypothesis.** If a cross-department decision is convened as a typed meeting whose agenda items
resolve approvals and whose participants record positions, then the programme can show why a
decision was made, who dissented and what was promised — so decision latency falls, rework from
unrecorded objections falls, and a forum that decides nothing is visible and retired.

**Outcome.** The §12 T19 gate: a three-department majority with recorded dissent; an `inquorate`
deferral; no lowering of dual control; the owner decides live and over MCP; minutes behind the
chair's gate; every action a card; a decision-free forum flagged.

**Non-goals.** A chat product; an AV or calendar integration; an LLM participant; a second
decision store; a rebuild of T9's board or T10's cadence.

## Features

| ID | Feature | Parts | Acceptance (the plan's `Verify:`) |
|---|---|---|---|
| F1 | A meeting type is policy; a meeting is a card | D1 | `meetings_policy` tests |
| F2 | The agenda is an L0 query with an evidence minimum | D2 | `meetings_agenda` tests |
| F3 | Attendance, seats, positions and quorum | D3 | `meetings_attendance` tests |
| F4 | The outcome arithmetic — per-seat decisions, stricter wins | D4 | `meetings_decision` tests |
| F5 | Outcomes, commitments, actions and minutes | D5 | `meetings_outcomes` tests |
| F6 | The human seat live and asynchronous | D6 | `meetings_human_seat` + Playwright |
| F7 | Metering and the never-decided forum | D7 | `meetings_metering` tests |
| F8 | The Meetings destination and the T9/T10 seams | D8 | `meetings_console` + Playwright |

## Enablers

| ID | Enabler | Serves |
|---|---|---|
| E1 | Five meeting tables; the `approval_decision` participant extension; the `minutes` kind | F1, F4, F5 |
| E2 | The `meetings.*` action family and four events | F1–F8 |
| E3 | The approval arithmetic and the `via` surface record | F4 |
| E4 | The `governance_body` convener seam (T9 built) | F8 |
| E5 | The `programme_cadence` slot seam (T10 built) | F8 |
| E6 | The `/meetings` route and its Playwright spec (T18 shell, T19 screen) | F8 |

## WSJF

The score is computed from four typed integer inputs; the score itself is never stored.

| Epic | Business value | Time criticality | Risk reduction | Job size | Computed |
|---|---:|---:|---:|---:|---:|
| T19 decision forums | 8 | 5 | 8 | 8 | (8 + 5 + 8) / 8 = **2.6** |

The inputs are the ranking record. A change to any input re-ranks the epic; an LLM may draft the
narrative, never the numbers.

## PI objectives — tranche 4 PI

| ID | Objective | Committed | Business value | Demo evidence |
|---|---|---|---:|---|
| PI-1 | A meeting type and a meeting card convene one live forum per slot | committed | 8 | D1 tests + a scheduled meeting |
| PI-2 | The agenda is an L0 query with evidence and one approval | committed | 5 | D2 tests + the materialised agenda |
| PI-3 | Every participant has one live typed position; a missing department defers | committed | 8 | D3 tests + an `inquorate` deferral |
| PI-4 | The outcome arithmetic writes per-seat decisions and the stricter policy wins | committed | 13 | D4 tests + a three-department majority |
| PI-5 | Commitments, actions and minutes are rows behind the chair's gate | committed | 8 | D5 tests + a published minute |
| PI-6 | The owner decides live and asynchronously, first wins | uncommitted | 8 | D6 API + Playwright |
| PI-7 | Participant-hours and the seven instruments recompute; a dead forum is flagged | uncommitted | 5 | D7 tests + the roll-up |
| PI-8 | The Meetings destination and the T9/T10 seams render and resolve | uncommitted | 5 | D8 tests + Playwright |

The committed set is the decision-record minimum. The uncommitted set may be pulled if the
migration and arithmetic land early; no objective is dropped from the gate.

## Dependencies

| Relation | Artefact | Why |
|---|---|---|
| blocked-by | #72 (T1), #73 (T2), #74 (T3), #120 (P0) | spine, ladder, gate routing, programme gate |
| related | #81 (T9), #82 (T10) | built consumer seams; binding is a follow-up |
| related | #562–#566 | T18 UI lane; the shell lists the route, T19 owns the screen |
| blocks | #606 | the implementation issue carries D1–D8 |

## Demo evidence

The system demo exhibits: a scheduled meeting; an agenda materialised from an approval and a
register item; an `inquorate` deferral; a three-department majority with a verbatim dissent; a
commitment that becomes a dependency; an action that becomes a card; a minute published behind the
chair's approval; the owner's two decisions (web room and MCP) as one row each; and the
participant-hours figure. No demo, no credit.

## Inspect & adapt

The retro reads decision latency, agenda conversion, quorum-miss, commitment slippage, action
closure, person-hours per decision and decisions per meeting. A systemic finding becomes a kata
candidate (T5); a policy finding becomes a `meeting_type` change through the governance route, not
a code edit. A forum that decided nothing over the cycle is the first candidate for retirement.

## Links

- Plan: `docs/plans/T19-decision-forums.md`
- MSP: `docs/plans/T19/msp-profile.md`
- Architecture: `docs/plans/T19/architecture-design.md`
- Spec: `docs/SPEC_V2.md` §8.2, §12 T19, D66
