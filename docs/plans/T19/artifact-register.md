---
document: T19/artifact-register
version: 1
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
| T19-A7 | Implementation evidence | evidence | G / capability | published | 1 | programme-manager | Architecture Board | 2026-11-23 | `docs/plans/T19/evidence.md` |

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
future T9 import maps each row to `architecture_artifact` + `architecture_artifact_entry` +
`architecture_artifact_link`; the document becomes the published body.
