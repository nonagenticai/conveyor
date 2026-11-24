---
document: T19F/architecture-design
version: 1
status: draft
project: T19F
category: architecture-design
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-12-01
artifact_kind: architecture-design
phase: A–F
plateau: PSA + HLD
---

# T19F — Architecture design (PSA + HLD)

The design follows T9's tailored ADM profile and the artefact standard in
`docs/plans/T9-architecture-capability.md`. Each artefact carries the standard's common metadata
and is linked with `derivation | realization | reuse | dependency`.

## PSA — Preliminary, A

### Principles

| Id | Principle | Application |
|---|---|---|
| P1 | One decision store | a policy version is still a `meeting_type` row; a sitting decision is still an `approval_decision` row |
| P2 | Rows are the record | the authored policy, the approval, the positions and the minutes are rows; no transcript is evidence |
| P3 | The consumer names its binding | the body and the cadence row name their meeting type explicitly; no key coincidence |
| P4 | Stricter policy wins | the linked approval's dual control, evidence and conflict rules outrank the meeting's rule |

### Requirements

| Id | Requirement | Source |
|---|---|---|
| R1 | A policy change publishes a new immutable version | §6.8 "versioned (P11)"; T19 closure review |
| R2 | A policy change passes a human review gate | T17 `authoring.review` |
| R3 | An in-flight meeting keeps its pinned version | §6.8; `meeting.meeting_type_id/version` |
| R4 | A board subject decision can be taken in a sitting | §6.8; SPEC §12 T19F |
| R5 | A sitting writes one `approval_decision` per qualifying seat with `via='meeting'` | §6.8; T19 D4 |
| R6 | A cadence row names its meeting type and materialises one meeting | §6.8; SPEC §12 T19F |
| R7 | The direct board path is unchanged | T9; the scope boundary |
| R8 | The policy and cadence bindings are tenant-scoped and RLS-guarded | P11; the existing tables |

### Stakeholders

| Stakeholder | Concern | Engagement |
|---|---|---|
| SRO / sponsoring group | the board's decisions are deliberated and evidenced | tranche review; the programme board |
| Programme manager | the tranche-6 evidence and B7 | the meeting record |
| Architecture Board | the sitting path, contracts and artefacts | the sitting; the phase-G review |
| BCM | B7's measurement | the benefit profile and the review |
| Department heads | seats are invited and counted | the attendance rows |
| Delivery teams | no change to the direct paths | the release note |

### Drivers, goals and objectives

| Driver | Goal | Objective |
|---|---|---|
| A policy change today has no route | a governed policy lifecycle | a reviewed asset publishes a version; the schedule refuses an inline change |
| The board's deliberation is not recorded as a meeting | a seated board decision | a sitting produces `via='meeting'` rows |
| A cadence slot is scheduled by hand | a cadence-driven forum | the row materialises its meeting once |

### Solution concept

The authoring asset is the draft; the published asset is the release note; `meeting_type` is the
runtime policy. The board sitting is an approval plus a meeting plus an agenda item, decided by
T19's arithmetic. The cadence materialisation is a schedule with a slot and an idempotent key.
Nothing else is added.

## HLD — B–D

### Business capability map

| Capability | Realisation |
|---|---|
| B1 Governed tenant assets | T17's authoring route, extended with the `policy` payload |
| B2 Deliberated decisions | T19's meeting, reused by the board sitting |
| B3 Cadence-driven forums | T10's cadence row, binding to a meeting type |

### Data entity catalog

| Entity | Change |
|---|---|
| `meeting_type` | no schema change; the writer and the reads are new |
| `meeting` | no column; the cadence-slot partial unique index is new |
| `governance_body` | `meeting_type_key` |
| `programme_cadence` | `meeting_type_key` |
| `authoring_asset` | `published_meeting_type_id` |
| `approval`, `approval_decision`, `meeting_agenda_item`, `meeting_attendance`, `meeting_position` | reused unchanged |

### Application view

| Component | Responsibility |
|---|---|
| `crates/store/src/authoring.rs` | publish hook: validate the policy payload, write the next version, link the asset |
| `crates/store/src/meetings.rs` | version writer, sitting open, sit decide, cadence materialise |
| `crates/store/src/architecture.rs` | extract the board approval insert; set the body's key |
| `crates/store/src/spine.rs` | cadence key in create/update/list; the derived meeting |
| `crates/actions/src/*` | five new actions and the changed cadence inputs |
| `docs/design/v2.js`, `apps/web` | the policy, sitting and cadence surfaces |

### Technology standards

| Standard | Use |
|---|---|
| Rust + sqlx, AdditiveForward migrations | migration 0068 |
| One ActionSpec per action | HTTP + MCP + OpenAPI + generated client |
| RLS and tenant scoping | every altered table keeps `ENABLE`+`FORCE ROW LEVEL SECURITY` |
| Deterministic generators | seed, coverage and docs are regenerated, never hand-edited |

### Migration

Tranche 6: the migration, the actions, the tests and the surfaces land in #683; the live chain and
the live gate run in #684 on the deployed engine. No backfill: the bindings are nullable and the
direct paths keep working for rows without a key.

## Links

| From | Kind | To |
|---|---|---|
| Requirements catalog | derivation | Solution concept |
| Drivers/goals | derivation | Requirements catalog |
| Stakeholder map | dependency | Solution concept |
| Business capability map | realization | Application view |
| Data entity catalog | realization | Application view |
| Technology standards | realization | Application view |

## Statement of Architecture Work and compliance

The SoAW for T19F is requested, board-decided and approved in the live governance chain
(`scripts/governance-chain.py --project T19F`), followed by a phase-F delivery contract. Two
Phase-G compliance reviews record the level and the follow-ups. The review checks: every policy
version is published through the authoring gate; every sitting decision is a `via='meeting'`
decision row; every materialised cadence has one non-cancelled meeting; the direct board path is
unchanged.
