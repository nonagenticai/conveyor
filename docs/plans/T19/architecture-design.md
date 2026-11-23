---
document: T19/architecture-design
version: 1
status: draft
project: T19
category: architecture-design
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-11-30
phase: A–F
plateau: PSA + HLD
---

# T19 — Architecture design (PSA and HLD)

## 1. Context and boundary

T19 adds a decision forum over the existing four units. It does not add a fifth unit, a chat
system, a second decision store or a calendar dependency. The architecture covers the meeting
aggregate, the participant extension to `approval_decision`, the `minutes` governed-asset kind, the
`meetings.*` action family, the Meetings destination and the two consumer seams (`governance_body`
and `programme_cadence`). The T18 UI lane is a consumer of the destination row and is not rebuilt.

This is a tailored ADM set for a software delivery slice; it is a document set now and imports as
typed `architecture_artifact` rows when the T9 repository is used. Every artefact carries the
standard's common metadata: title, status, version, owner, reviewer, review date, purpose,
stakeholders, scope, related project and its published document; links use
`derivation | realization | reuse | dependency`.

## 2. Artefact catalogue (common metadata)

| ID | Artefact | Type | Phase / plateau | Version | Owner | Reviewer | Review date | Status |
|---|---|---|---|---|---|---|---|---|
| A1 | Principles applied | catalog | A / PSA | 1 | programme-architect | Architecture Board | 2026-11-30 | draft |
| A2 | Requirements catalog | catalog | A / PSA | 1 | business-analyst | programme-architect | 2026-11-30 | draft |
| A3 | Stakeholder map | matrix | A / PSA | 1 | business-analyst | SRO | 2026-11-30 | draft |
| A4 | Drivers, goals, objectives | catalog | A / PSA | 1 | programme-manager | SRO | 2026-11-30 | draft |
| A5 | Solution concept | diagram | A / PSA | 1 | programme-architect | Architecture Board | 2026-11-30 | draft |
| A6 | Statement of Architecture Work | contract | A / PSA | 1 | programme-architect | SRO | 2026-11-30 | draft |
| B1 | Business interaction matrix | matrix | B / HLD | 1 | business-analyst | SRO | 2026-11-30 | draft |
| B2 | Actor / role matrix | matrix | B / HLD | 1 | business-analyst | programme-architect | 2026-11-30 | draft |
| C1 | Data entity catalog | catalog | C / HLD | 1 | programme-architect | Architecture Board | 2026-11-30 | draft |
| C2 | Application / function matrix | matrix | C / HLD | 1 | programme-architect | Architecture Board | 2026-11-30 | draft |
| C3 | Conceptual data diagram | diagram | C / HLD | 1 | programme-architect | Architecture Board | 2026-11-30 | draft |
| D1 | Technology view | catalog | D / HLD | 1 | platform lead | Architecture Board | 2026-11-30 | draft |
| F1 | Migration and delivery diagram | diagram | F / HLD | 1 | programme-manager | SRO | 2026-11-30 | draft |

The published document is this file for every row; the future typed repository rows derive from it.
No row is approved yet; the review date is the Architecture Board's sitting in tranche 4.

## 3. Project Start Architecture (Phase A)

### A1 — Principles applied (catalog)

| ID | Principle | Rationale | Implication |
|---|---|---|---|
| P1 | Decisions are rows, not recollections | an audit needs the decision and its deliberation | the meeting writes approval_decision rows and typed positions; no transcript is a decision |
| P2 | One decision store | a second store makes the record ambiguous | `meeting_position` is never a decision; the linked approval remains authoritative |
| P3 | The stricter policy wins | a meeting must not weaken a gate | evaluate the approval's dual control, evidence and conflict rules after the meeting rule |
| P4 | Cross-department by construction | the purpose is the departments' positions | required participation is policy; a missing department defers `inquorate` |
| P5 | The record outlives the room | a room may be replaced | attendance is the record; an ICS feed is a read; no calendar dependency |
| P6 | Generation drafts, it never decides | the LLM is not a position | the LLM drafts wording, pre-reads and minutes; it has no selection or position action |
| P7 | Policy is data | a tenant's forums differ | `meeting_type` is versioned and tenant-scoped; who may convene is an authority row |

### A2 — Requirements catalog (typed)

| ID | Type | Requirement | Source |
|---|---|---|---|
| R1 | condition | A meeting is a card with a chair and a DoD; it is not a fifth unit | §6.8 |
| R2 | condition | A deciding item names exactly one pre-opened approval with evidence | §6.8, §10.3 |
| R3 | constraint | A meeting cannot lower dual control, waive evidence or bypass P4 | §6.8, D66 |
| R4 | constraint | A required department's absence defers the item `inquorate` | §6.8, §12 T19 |
| R5 | constraint | One live position per participant per item; immutable after close | §6.8, §10.3 |
| R6 | condition | A commitment needs an owner and a date; an action becomes a card | §6.8 |
| R7 | condition | Minutes are a chaired `governed_asset`; every line matches a row | §6.8 |
| R8 | condition | A human seat is an ordinary `app_user`; any seat role | §6.8 |
| R9 | condition | The owner can decide live or asynchronously over web/Telegram/MCP; first wins | §6.8, D66 |
| R10 | constraint | No calendar or AV dependency; ICS is a read | §6.8 |
| R11 | condition | Participant-hours enter the D52 roll-up as a third source | §6.8 |
| R12 | condition | A forum with no decision over a cycle is flagged and retired | §6.8, P10 |
| R13 | assumption | T9's `governance_body` and T10's `programme_cadence` remain the consumer seams | #81, #82 |
| R14 | delta | `approval_decision` must accept a job-binding participant | measured gap, §4.1 of the plan |

### A3 — Stakeholder map (matrix)

| Stakeholder | Concern | Influence | Interest | Engagement |
|---|---|---|---|---|
| Owner / sponsoring group | the decision and the dissent are the record; can join any forum | high | high | seat in the room or async; decides items |
| Chair | the agenda is adopted, minutes publish behind its gate | high | high | chair seat; adopts agenda; publishes minutes |
| Operations department | its position is not outvoted silently; conditions are visible | high | high | required department on operational items |
| Security department | evidence and conflict rules are not waived | high | high | required department on security items |
| Delivery department | commitments have owners and dates | high | high | required department on delivery items |
| Job bindings | their position and evidence are typed | medium | high | member/presenter seats |
| Observers | they can read and be recorded, never count | low | medium | observer seat |
| Architecture Board (T9) | board sittings are meetings | high | medium | `governance_body` convener |
| Programme cadence (T10) | PI planning/demo/I&A are meeting instances | high | medium | cadence agenda source |

### A4 — Drivers, goals, objectives (catalog)

| ID | Statement |
|---|---|
| DR1 | High-level work is decided in cross-department meetings; only the approval survives today |
| G1 | Every decision that needed several positions carries those positions, conditions and dissent |
| G2 | A meeting never weakens a gate; the stricter policy wins |
| O1 | T19's §12 gate passes with a three-department majority, an `inquorate` deferral and a live owner decision |
| O2 | Decision latency, agenda conversion, quorum-miss, commitment slippage and action closure recompute from rows |

### A5 — Solution concept (diagram)

```
card(meeting) 1──1 meeting ──* meeting_agenda_item ──1 approval ──* approval_decision
     │                │                    │
     │                │                    └──* meeting_position (participant, stance, evidence)
     │                └──* meeting_attendance (job_binding | app_user, seat, department)
     └── governed_asset(kind='minutes') ← drafted from the rows, published behind the chair's gate
commitment → project_dependency | card        action → card        decision → approval result
```

### A6 — Statement of Architecture Work (draft)

Assignment: build T19 to the §12 gate. Scope: §3 of the plan. Responsibilities: the programme
architect owns the set; the Architecture Board reviews; the SRO accepts the benefit and tranche
evidence. Acceptance: the six gate clauses plus D1–D8. The contract is a document now; T9's
`architecture.contracts_approve` makes it live when the repository is used.

## 4. High-Level Design (Phases B–D)

### B1 — Business interaction matrix

| Meeting type | Framework | Convener | Agenda sources | Rule | Outcome |
|---|---|---|---|---|---|
| Programme board | MSP | sponsoring group / SRO | programme register, benefit-at-risk, tranche decisions | chair | decision/commitment |
| Tranche review | MSP | sponsoring group | baselines, benefit status, risk register | unanimous or chair per type | continue/pause/stop |
| Architecture Board | TOGAF | `governance_body` | architecture contracts, compliance, waivers, change requests | majority | decision/action |
| Design Board | TOGAF | `governance_body` | solution designs, design gate, deviations | majority | decision/action |
| PI planning | SAFe | programme manager | backlog by WSJF, capacity, plan approval | majority | decision/commitment |
| System demo | SAFe | programme manager | completed deliverables and evidence | chair | information/decision |
| Inspect & adapt | SAFe | coach | retro findings, lessons, kata candidates | majority | action/commitment |
| Incident review | operations | incident commander | incident, postmortem, runbook findings | chair | decision/action |

### B2 — Actor / role matrix

| Seat | May invite | May add item | May position | Counts | May publish minutes |
|---|---|---|---|---|---|
| chair | yes | yes | yes | policy rule | yes (behind its approval) |
| member | no | yes (authority) | yes | yes | no |
| presenter | no | via chair | yes (on its items) | yes if the type says so | no |
| delegate | per type | per type | yes | per type | no |
| observer | no | no | recorded, never counts | no | no |

### C1 — Data entity catalog

| Entity | Key content | Relationships |
|---|---|---|
| `meeting_type` | policy, required participation, quorum, rule, evidence, window, outcomes, version | 1→* `meeting` |
| `meeting` | card, convener, type version, window, cadence slot | 1→* agenda items, attendance; 1→1 card |
| `meeting_agenda_item` | sequence, resolved object, departments, evidence, outcome | *→1 approval; 1→* positions |
| `meeting_attendance` | participant kind/id, seat, department, invitation/response | *→1 meeting |
| `meeting_position` | participant, stance, rationale, conditions, commitment, surface | *→1 item |
| `approval_decision` (extended) | human or job participant; `via` surface | *→1 approval |
| `governed_asset(kind='minutes')` | minutes body, status, approval | 1→1 meeting |

### C2 — Application / function matrix

| Action | Service | Writes | Emits |
|---|---|---|---|
| `meetings.schedule` | meetings | card, meeting | — |
| `meetings.open` | meetings | meeting status | `meeting.opened` |
| `meetings.invite` / `respond` | meetings | attendance | — |
| `meetings.agenda_add` | meetings | agenda item | — |
| `meetings.position` | meetings | position | — |
| `meetings.defer` | meetings | item outcome | — |
| `meetings.close_item` | meetings + approvals | decision rows, item outcome | `meeting_item.closed` |
| `meetings.close` | meetings | meeting status | `meeting.closed` |
| `meetings.minutes_publish` | meetings + governed assets | minutes asset | `meeting_minutes.published` |
| `meetings.list` / `get` | meetings | read only | — |

### C3 — Conceptual data diagram

`meeting_type` (policy) → `meeting` (instance) → `meeting_agenda_item` (resolved approval) →
`meeting_position` (participant stance) and `approval_decision` (one per qualifying seat);
`meeting_attendance` binds participants to seats; `governed_asset(minutes)` derives from the rows;
`project_dependency` receives commitments; `card` receives actions.

### D1 — Technology view

PostgreSQL 18 with RLS on every new table; the existing Rust action registry and HTTP/MCP surfaces;
the Leptos console route `/meetings`; the existing Telegram channel adapter and MCP task input
request; an ICS feed rendered from attendance. No Redis, no new port, no hosted model, no calendar
service, no AV service.

### F1 — Migration and delivery diagram

```
D1 policy + participant extension → D2 agenda → D3 attendance/positions → D4 arithmetic
→ D5 outcomes/minutes → D6 human surfaces → D7 metering → D8 console + seams
```

The order is also the build order: D1's migration must land before any action; D4 depends on D1–D3;
D5 on D4; D6–D8 consume the prior parts. The slice is one PR (#606). Tranche 4 authorises it; the
tranche review reads the D7 instruments.

## 5. Artefact links

| From | Kind | To | Evidence |
|---|---|---|---|
| A2 requirements | derivation | A5 solution concept | each requirement maps to an entity or action |
| A5 solution concept | realization | C1 data entities and C2 functions | the diagram names them |
| A4 goals | derivation | A2 requirements | R1–R12 serve G1–G2 |
| A1 principles | derivation | A5 solution concept | P1–P7 shape the model |
| A3 stakeholders | dependency | B1 interaction matrix | each forum names its stakeholders |
| B1 interaction matrix | realization | C2 functions | each forum uses the action family |
| C1 data entities | realization | D1 technology view | PostgreSQL tables and RLS |
| F1 migration | dependency | D1–D8 | the build order |
| T9 `governance_body` | reuse | A3/B1 | the Board is a convener, not a new body |
| T10 `programme_cadence` | reuse | B1 | the cadence supplies the slot, the meeting the record |

## 6. Compliance and contract

The delivery design gate: T19 is an ordinary delivery project under the P0 brief and the T9
capability. The set above is the PSA/HLD evidence; a deviation (a second decision table, a chat
transcript as a decision, a calendar dependency) is a change request, never a silent fork. The
compliance review records one level against the statement of architecture work when the project
closes. The full TOGAF repository rows are T9's; this set is publishable now and importable later.

## 7. Review and links

- Review date: 2026-11-30 (Architecture Board, tranche 4)
- Brief: `docs/plans/T19/project-brief.md`
- Analysis: `docs/plans/T19/inception-analysis.md`
- Plan: `docs/plans/T19-decision-forums.md`
- MSP: `docs/plans/T19/msp-profile.md`
- SAFe: `docs/plans/T19/safe-epic.md`
- Register: `docs/plans/T19/artifact-register.md`
