---
document: T19F/artifact-register
version: 1
status: draft
project: T19F
category: artifact-register
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-12-01
artifact_kind: catalog
phase: A–F
plateau: PSA + HLD
---

# T19F — Artefact register

The typed catalogue of the T19F project set. Each row carries T9's common metadata: title, status,
version, owner, reviewer, review date, purpose, stakeholders, scope, related project and its
published document. Links use `derivation | realization | reuse | dependency`.

| ID | Artefact | Type | Phase / plateau | Status | Version | Owner | Reviewer | Review date | Document |
|---|---|---|---|---|---|---|---|---|---|
| T19F-A0 | Project brief | project-brief | Preliminary / definition | draft | 1 | programme-manager | SRO | 2026-12-01 | `docs/plans/T19F/project-brief.md` |
| T19F-A1 | Inception analysis | inception-analysis | Preliminary / definition | draft | 1 | business-analyst | programme-architect | 2026-12-01 | `docs/plans/T19F/inception-analysis.md` |
| T19F-A2 | Architecture design (PSA + HLD) | architecture-design | A–F / PSA+HLD | draft | 1 | programme-architect | Architecture Board | 2026-12-01 | `docs/plans/T19F/architecture-design.md` |
| T19F-A3 | MSP profile (B7 realisation increment) | benefit-profile | Preliminary / definition | draft | 1 | BCM | SRO | 2026-12-01 | `docs/plans/T19F/msp-profile.md` |
| T19F-A4 | SAFe epic (features, enablers, WSJF, PI) | epic | F / HLD | draft | 1 | programme-manager | SRO | 2026-12-01 | `docs/plans/T19F/safe-epic.md` |
| T19F-A5 | Follow-on plan | plan | E–F / HLD | proposed | 1 | programme-architect | Architecture Board | 2026-12-01 | `docs/plans/T19F-meeting-bindings.md` |
| T19F-A6 | This register | catalog | A–F / PSA+HLD | draft | 1 | programme-architect | Architecture Board | 2026-12-01 | this file |
| T19F-A7 | Implementation evidence | evidence | G / capability | to create | 1 | programme-manager | Architecture Board | 2026-12-01 | `docs/plans/T19F/evidence.md` (to create in #684) |
| T19F-A8 | Closure review | closure-review | G / capability | to create | 1 | programme-manager | Architecture Board | 2026-12-01 | `docs/plans/T19F/closure-review.md` (to create in #684) |

## Typed TOGAF rows (to create in the live chain, #684)

The seven mandatory artefacts are created and approved through the deployed engine under the
published standing delegation, with entries from `docs/plans/T19F/architecture-design.md`:
`requirements_catalog`, `stakeholder_map`, `driver_goal_objective`, `solution_concept`,
`business_capability_map`, `data_entity_catalog`, `technology_standards`. The chain also creates the
seven links, the Statement of Architecture Work, the phase-F delivery contract, two Phase-G
compliance reviews and the published `architecture-artifact` document. Read-back:
`python3 scripts/governance-chain.py --project T19F --check`.

## Purpose, stakeholders and scope

**Purpose.** Give T19F one navigable set of decision, architecture, programme and delivery
artefacts, each with an owner, a reviewer and a linked source, so the design gate and the tranche-6
review read the same record.

**Stakeholders.** The SRO, the BCM, the programme manager and architect, the Architecture Board, the
departments that staff the board's seats, and the delivery teams that use the direct board path.

**Scope.** T19F only. T19's rows, T17's authoring assets and T9/T10's consumer rows are related
sets, not part of this register.

## Links

| From | Kind | To | Note |
|---|---|---|---|
| T19F-A1 analysis | derivation | T19F-A2 architecture | the measured seams shape the PSA/HLD |
| T19F-A2 architecture | realization | T19F-A5 plan | each plan part realises a requirement/entity/action |
| T19F-A3 MSP | dependency | T19F-A5 plan | B7's increment depends on the authored policy and the seated board |
| T19F-A4 SAFe | derivation | T19F-A5 plan | features F1–F4 map to P1–P4 |
| T19F-A2 architecture | reuse | `meeting_type`, `meeting`, `close_item_tx`, `authoring.publish` | T19/T17 consumers, not rebuilds |
| T19F-A0 brief | dependency | T19F-A5 plan | the brief's boundary governs the plan |
| T19F-A7 evidence | realization | T19F-A5 plan | the measured acceptance of every part |
