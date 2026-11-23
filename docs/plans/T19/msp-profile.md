---
document: T19/msp-profile
version: 1
status: draft
project: T19
category: benefit-profile
owner: BCM
reviewer: SRO
review_date: 2026-11-30
---

# T19 — MSP profile

## 1. Business case addendum

**Why now.** V2's high-level work — programme management, architecture, analysis and project
management — is decided in cross-department meetings, but only the resulting approval row survives.
The programme cannot show the trade-offs, conditions, dissent or commitments that made the
decision, and it cannot tell whether the forum that was supposed to decide actually met.

**Options.**

| Option | What it buys | What it costs |
|---|---|---|
| Do nothing — approvals only | the decision row | no deliberation, no cross-department positions, no dissent, no commitments; R20 unmitigated |
| A chat/transcript room | conversation | no typed decision, no quorum, no arithmetic, a second store; rejected by §6.8 |
| **Typed meeting forum (T19)** | decision + deliberation, one decision store, measurable forums | five tables, twelve actions, one console destination, one participant extension |

**Cost and effort.** One delivery slice (D1–D8), one migration set, the `meetings.*` family and the
Meetings destination. The ongoing cost is participant time, metered as participant-hours from
attendance × duration in the D52 roll-up. The change is a tenant policy, not a new runtime.

**Investment appraisal.** The benefit is measured by the instruments in §3, not by the capability's
existence. The case is accepted provisionally: the baseline is unmeasured until the first meeting
rows exist, and the tranche-4 review decides whether the instrument is producing a measurable
improvement. An unmeasured baseline is reported `unmeasured`, never zero.

## 2. Benefit profile B7

| Group | Fields |
|---|---|
| Identification and purpose | **reference** B7 · **description** Deliberated cross-department decisions · **category** strategic · **objectives** G1 every multi-position decision carries its positions, conditions and dissent; G2 a meeting never weakens a gate · **observable outcome** a decision that needed several departments has a meeting record with one position per required department, the dissent verbatim and the commitments dated |
| Performance | **KPIs** decision latency, agenda conversion, quorum-miss rate, commitment slippage, action closure, person-hours per decision, decisions per meeting · **baseline** `unmeasured — no meeting rows exist before T19; the current record is the approval row only` · **target** every tranche-4 cross-department decision has a meeting record; quorum-miss and commitment slippage are reported at the tranche review with a target fixed at the first measured cycle · **measurement method** read-time aggregates over `meeting`, `meeting_agenda_item`, `meeting_attendance`, `meeting_position`, `approval_decision` and `project_dependency`; a metric reads rows and writes nothing · **financial value** not expressed; participant-hours are the cost side |
| Cost | the T19 build (D1–D8) and the business change: chairs adopt agendas, departments staff required seats, the owner uses the room or the async queue |
| Conditions | **capabilities required** T1 plan spine and registers, T2 ladder and policy, T3 gate routing, T19 meeting forum; **outcomes that must already exist** the approval primitive, `app_user`, `card`, `governed_asset`, `project_dependency`; **changes to operating practice** decisions that need several positions are convened, not settled in a thread; minutes publish behind the chair |
| Uncertainty and dependencies | **risks** R20 meeting theatre; a forum that meets but decides nothing; a required department that never staffs its seat; the T9/T10 binding follows T19; **dependencies outside the programme** none beyond the tenant's department staffing |
| Responsibility and attribution | **who realizes it** the programme architect and the delivery team; **benefit owner** BCM; **operational area** programme management, architecture and project management — the areas whose decisions become the record |

The profile is approved only when the BCM and SRO accept it and the baseline is measured while the
old practice (approval-only) is still observable. Until then the profile is `draft` and the
benefit is reported unmeasured.

## 3. Benefits map

| Field | Value |
|---|---|
| output_ref | T19 decision forums |
| capability_ref | multi-party decisions carry their deliberation |
| operational_outcome | O7 Meetings decide with the affected departments |
| benefit | B7 |
| strategic_objective | decision ladder (rule → laya → generation → human) |

The edge is one path: T19's output enables the capability; the capability produces O7; O7 is
valued as B7 and contributes to the ladder objective. A slip in T19 puts B7 at risk; a missing
required department is an O7 risk, not a T19 build failure.

## 4. Risk R20 — meeting theatre

| Field | Value |
|---|---|
| Cause | a calendar that grows, decisions claimed in conversation, courteous attendance, minutes that do not match |
| Event | a meeting exists but the decision record is not its product |
| Effect | B7 is not realized; the programme pays participant time for no deliberation |
| Owner | programme manager (operational), BCM (benefit) |
| Appetite | low; the forum's controls are the mitigation, not a tolerance |
| Mitigation | every item resolves a typed object and needs evidence; the closed outcome set; quorum and rule are policy; commitments need owner+date; a minute line must match a row; participant-hours are metered; a decision-free forum is retired (P10) |
| Review date | 2026-11-30 (tranche-4 planning) and every tranche review |

## 5. Stakeholder profiles

| Stakeholder | Concerns / sensitivities | Importance | Influence | Interest | Exposure | Support | Engagement target |
|---|---|---|---|---|---|---|---|
| Owner / sponsoring group | the decision and dissent are the record; can join any forum | high | high | high | high | support | seat in the room and the async queue; decides items |
| Chair | the agenda is adopted and minutes publish behind its gate | high | high | high | medium | support | chair seat and minutes gate |
| Operations | its position is not outvoted silently; conditions are visible | high | high | high | high | support | required department on operational items |
| Security | evidence and P4 conflict rules are not waived | high | high | high | high | support | required department on security items |
| Delivery | commitments have owners and dates | high | medium | high | medium | support | required department on delivery items |
| Job bindings | their position and evidence are typed | medium | medium | high | medium | support | member/presenter seats |
| Observers | recorded but never counted | low | low | medium | low | neutral | observer seat |
| Architecture Board | board sittings are meetings | high | high | medium | medium | support | `governance_body` convener |
| Programme cadence | PI planning, demo and I&A are meeting instances | high | high | medium | medium | support | cadence agenda source |

## 6. Tranche authorisation

T19 is authorised under **tranche 4**. Its entry is the tranche-3 exit (T1, T2, T3 and P0 built);
its exit is the §12 T19 gate with the D1–D8 evidence. The tranche review's evidence is the
baseline for B7 (unmeasured at entry), the R20 posture, the meeting instruments from D7 and the
blueprint alignment from the architecture design. The review decides continue, pause or stop; a
re-baseline goes through change control, never through the review.

## 7. Transition and measurement

The old practice is approval-only; the baseline is measured while it is still observable (zero
meeting rows, decision latency recorded only on the approval). T19 enacts the new practice:
convene the forum, record positions, derive outcomes, publish minutes. Post-transition the BCM
measures the seven instruments, reports the delta against the approval-only baseline and keeps
measuring until the realization plan names who continues after programme closure. The BCM is the
benefit owner; the programme manager is accountable for the forums' operation.

## 8. Links

- Brief: `docs/plans/T19/project-brief.md`
- Architecture: `docs/plans/T19/architecture-design.md`
- Plan: `docs/plans/T19-decision-forums.md`
- SAFe: `docs/plans/T19/safe-epic.md`
- Seed: `seed/programme-definition.json` (B7 and the tranche-4 exit)
- Spec: `docs/SPEC_V2.md` §6.8, §8.3, §12 T19, D66; R20
