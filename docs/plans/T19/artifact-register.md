---
document: T19/artifact-register
version: 2
status: draft
project: T19
category: artifact-register
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-11-30
artifact_kind: catalog
phase: A–F
plateau: PSA + HLD
---

# T19 — Artefact register

The typed catalogue of the T19 project set. Each row carries the T9 standard's common metadata:
title, status, version, owner, reviewer, review date, purpose, stakeholders, scope, related project
and its published document. Links use `derivation | realization | reuse | dependency`.

| ID | Artefact | Type | Phase / plateau | Status | Version | Owner | Reviewer | Review date | Document |
|---|---|---|---|---|---|---|---|---|---|
| T19-A0 | Project brief | project-brief | Preliminary / definition | draft | 1 | programme-manager | SRO | 2026-11-30 | `docs/plans/T19/project-brief.md` |
| T19-A1 | Inception analysis | inception-analysis | Preliminary / definition | draft | 1 | business-analyst | programme-architect | 2026-11-30 | `docs/plans/T19/inception-analysis.md` |
| T19-A2 | Architecture design (PSA + HLD) | architecture-design | A–F / PSA+HLD | draft | 1 | programme-architect | Architecture Board | 2026-11-30 | `docs/plans/T19/architecture-design.md` |
| T19-A3 | MSP profile (B7, R20, stakeholders, tranche) | benefit-profile | Preliminary / definition | draft | 1 | BCM | SRO | 2026-11-30 | `docs/plans/T19/msp-profile.md` |
| T19-A4 | SAFe epic (features, enablers, WSJF, PI) | epic | F / HLD | draft | 1 | programme-manager | SRO | 2026-11-30 | `docs/plans/T19/safe-epic.md` |
| T19-A5 | Implementation plan | plan | E–F / HLD | draft | 1 | programme-architect | Architecture Board | 2026-11-30 | `docs/plans/T19-decision-forums.md` |
| T19-A6 | This register | catalog | A–F / PSA+HLD | draft | 1 | programme-architect | Architecture Board | 2026-11-30 | this file |
| T19-A7 | Implementation evidence | evidence | G / capability | published | 3 | programme-manager | Architecture Board | 2026-11-24 | `docs/plans/T19/evidence.md` |
| T19-A8 | Closure review | closure-review | G / capability | published | 2 | programme-manager | Architecture Board | 2026-11-24 | `docs/plans/T19/closure-review.md` |

## Typed TOGAF rows (live, T9 repository, deployed `7e617673`, 2026-11-24)

The seven mandatory artefacts are now rows in `architecture_artifact`, approved by the tenant's
`conveyor-approver` seat under the standing delegation `01a1fb9f-ac2d-732d-b274-1079be3dd2b3`;
entries are the tables in `docs/plans/T19/architecture-design.md`. Read-back:
`python3 scripts/t19-governance-chain.py --check`; per-act evidence in `docs/evidence/t19/gov/`.

| Type | Artefact id | Phase / plateau | Status |
|---|---|---|---|
| `requirements_catalog` | `01a20cde-371c-7d85-ae2d-780453958bef` | A / PSA | approved |
| `stakeholder_map` | `01a20cdd-e71f-7d06-8ca5-701ff6b7d69e` | A / PSA | approved |
| `driver_goal_objective` | `01a20cde-0f97-7f74-afd8-7cd74e0990e3` | A / PSA | approved |
| `solution_concept` | `01a20cde-5f86-7720-9a16-36a964ef255d` | A / PSA | approved |
| `business_capability_map` | `01a20cde-86d7-7257-a45e-b4731ab423a4` | B / HLD | approved |
| `data_entity_catalog` | `01a20cde-adbf-7626-9b2b-58faf7164755` | C / HLD | approved |
| `technology_standards` | `01a20cde-d6bd-7cfe-88ad-42a1e6db30a7` | D / HLD | approved |

Seven `architecture_artifact_link` edges connect them (derivation from the goals to the requirements
and the solution concept; realization of the data entities and the capability map; dependency of the
stakeholder map; realization of both on the technology standards).

| Artefact | Id | Status |
|---|---|---|
| Statement of Architecture Work — T19 (contract, phase A) | `01a20ce0-5183-776c-84bd-327f2301b19b` | live, board decision `01a20ce0-644d-7340-a696-1af10fa8c61e` |
| Delivery contract — T19 (contract, phase F) | `01a20cea-91bb-77f4-a880-044c90bb5f0e` | live, board decision `01a20cea-a68f-7e08-8d0c-399759ef16e0` |
| Phase-G compliance review (SoAW) | recorded 2026-11-24 | `conformant` |
| Phase-G compliance review (delivery contract) | recorded 2026-11-24 | `conformant` |
| Architecture artifact — the typed TOGAF set (published output) | `01a20ce4-73e4-7009-9ec7-eb2265e9e84e` | published (Metadata / Entries / Links / Document) |

## Purpose, stakeholders and scope

**Purpose.** Give the T19 project one navigable set of decision, architecture, programme and
delivery artefacts, each with an owner, a reviewer and a linked source, so the design gate and the
tranche review read the same record.

**Stakeholders.** The sponsoring group/SRO, the BCM, the programme manager and architect, the
Architecture Board, the departments that staff meeting seats, and the delivery team.

**Scope.** T19 only; the T18 UI lane, the T9 repository rows and the T10 cadence rows are
consumers or related sets, not part of this register.

## Links

| From | Kind | To | Note |
|---|---|---|---|
| T19-A1 analysis | derivation | T19-A2 architecture | the measured gaps shape the PSA/HLD |
| T19-A2 architecture | realization | T19-A5 plan | every D-part realises an entity/action in the design |
| T19-A3 MSP | dependency | T19-A5 plan | B7 depends on the T19 output |
| T19-A4 SAFe | derivation | T19-A5 plan | features F1–F8 map to D1–D8 |
| T19-A2 architecture | reuse | `governance_body` (T9), `programme_cadence` (T10) | consumers, not rebuilds |
| T19-A0 brief | dependency | T19-A5 plan | the brief's boundary governs the plan |
| T19-A7 evidence | realization | T19-A5 plan | the measured acceptance of every D-part |

## Review

The set is reviewed together on 2026-11-30. An artefact missing its owner, reviewer, review date or
document is refused at the review; an approved artefact is superseded, never edited in place. The
T9 import has happened: the seven mandatory rows above are live and approved, the documents are
published, and the remaining register rows are the repository's markdown sources until each is
published as its own output.
