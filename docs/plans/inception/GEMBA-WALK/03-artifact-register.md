---
document: GEMBA-WALK/artifact-register
version: 1
status: draft
project: GEMBA-WALK
category: architecture-artifact
artifact_kind: catalog
phase: Preliminary–F
plateau: PSA + HLD
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-12-03
---

# GEMBA-WALK — Artefact register

The typed catalogue of the GEMBA-WALK set. Each row carries the T9 standard's common metadata:
title, status, version, owner, reviewer, review date, purpose, stakeholders, scope, related project
and its published document. Links use `derivation | realization | reuse | dependency`.

## Document catalogue

| ID | Artefact | Type | Phase / plateau | Status | Version | Owner | Reviewer | Review date | Document |
|---|---|---|---|---|---|---|---|---|---|
| GW-A0 | Project brief | project-brief | Preliminary / definition | draft | 1 | business-analyst | SRO | 2026-12-03 | `00-project-brief.md` |
| GW-A1 | Inception analysis | inception-analysis | Preliminary / definition | draft | 1 | business-analyst | programme-architect | 2026-12-03 | `01-inception-analysis.md` |
| GW-A2 | Architecture design (PSA + HLD) | architecture-design | A–F / PSA+HLD | draft | 1 | programme-architect | Architecture Board | 2026-12-03 | `02-architecture-design.md` |
| GW-A3 | MSP profile (business case, benefit, stakeholders, tranche) | benefit-profile | Preliminary / definition | draft | 1 | BCM | SRO | 2026-12-03 | `04-msp-profile.md` |
| GW-A4 | SAFe epic (features, enablers, WSJF, PI) | epic | F / HLD | draft | 1 | programme-manager | SRO | 2026-12-03 | `05-safe-epic.md` |
| GW-A5 | Statement of Architecture Work | statement-of-architecture-work | A / PSA | draft | 1 | programme-architect | SRO | 2026-12-03 | `06-statement-of-architecture-work.md` |
| GW-A6 | WBS, baseline, registers, change control | project-wbs | E–F / HLD | draft | 1 | programme-manager | SRO | 2026-12-03 | `07-governance-and-evidence.md` |
| GW-A7 | This register | catalog | A–F / PSA+HLD | draft | 1 | programme-architect | Architecture Board | 2026-12-03 | this file |
| GW-A8 | Implementation evidence | evidence | G / capability | draft | 1 | programme-manager | Architecture Board | 2026-12-03 | `07-governance-and-evidence.md` §Evidence |
| GW-A9 | Closure review | closure-review | G / capability | draft | 1 | programme-manager | Architecture Board | 2026-12-03 | to create at P6 |

## Typed TOGAF rows

The ten types the default ADM profile marks mandatory for the phases this project walks. The typed
rows are created on the project at publication; this table is their source.

| ID | `artifact_type.key` | Name | Type | Phase / plateau | Purpose | Stakeholders | Scope | Status |
|---|---|---|---|---|---|---|---|---|
| GW-T1 | `principles_catalog` | Principles Catalog | catalog | Preliminary / PSA | the principles the client is built to | Board, delivery team | A1 | draft |
| GW-T2 | `principle_derivation` | Principle Derivation Diagram | diagram | Preliminary / PSA | how each principle derives from a driver/goal | Board | A1 ↔ A4 | draft |
| GW-T3 | `principle_realization` | Principle Realization Diagram | diagram | Preliminary / PSA | where each principle shows in the design | Board, delivery team | A1 ↔ A5/C2 | draft |
| GW-T4 | `stakeholder_map` | Stakeholder Map | matrix | A / PSA | who the client serves and who may veto it | SRO | A3 | draft |
| GW-T5 | `driver_goal_objective` | Driver/Goal/Objective Catalog | catalog | A / PSA | why the client exists, measured | SRO | A4 | draft |
| GW-T6 | `requirements_catalog` | Requirements Catalog | catalog | A / PSA | the typed requirements the gate checks | SRO, Board | A2 | draft |
| GW-T7 | `solution_concept` | Solution Concept Diagram | diagram | A / PSA | the client and its contract, at a glance | Board | A5 | draft |
| GW-T8 | `business_capability_map` | Business Capability Map | catalog | B / HLD | the wings and seats the client serves | SRO | B1/B2 | draft |
| GW-T9 | `data_entity_catalog` | Data Entity/Data Component Catalog | catalog | C / HLD | the entities read and written; no new table | Board | C1 | draft |
| GW-T10 | `technology_standards` | Technology Standards Catalog | catalog | D / HLD | the serving, build, test and engine standards | platform lead | D1 | draft |

Each row's document is `02-architecture-design.md` except the register (this file); each derives
from A1–A6 and realises into C1/C2/D1. A row is approved only by the Architecture Board after the
P0 spike; until then it is `draft` and is evidence, not a claim.

## Contract kinds

| Kind | Phase | Subject | Status | Document |
|---|---|---|---|---|
| `statement_of_architecture_work` | A | Assignment, scope, responsibilities, acceptance | draft | `06-statement-of-architecture-work.md` |
| `delivery_contract` | F | Delivery commitments and the compliance method | to create at P1 | plan §11 |
| `statement_of_intent` | G | Intended use, ownership, measures of effectiveness | to create at P6 | plan §7 |
| `request_for_architecture` | H | The change request that follows the closure | to create if needed | — |

The design gate and the Phase-G compliance review use the live **delivery contract**, never the
SoAW (programme-architect, #395). The SoAW is the Phase-A assignment and this register's contract
row.

## Common metadata

- **Status:** draft; the Board's review date is 2026-12-03, after the P0 spike.
- **Version:** 1; a change after approval is a new version and re-opens the gate.
- **Owner / reviewer / review date:** per row above.
- **Purpose:** give the client a typed architecture record that imports into the T9 repository.
- **Stakeholders and scope:** the GEMBA-WALK project; no other project is changed.
- **Published document:** this file and `02-architecture-design.md`; live document ids are recorded
  at publication in the inception issue (#792).

## Links

| From | Kind | To | Evidence |
|---|---|---|---|
| GW-T1 principles | derivation | GW-T2 / GW-T3 | A1 ↔ A4 / A5 |
| GW-T4 stakeholders | dependency | GW-T8 capability map | A3 ↔ B1/B2 |
| GW-T5 drivers | derivation | GW-T6 requirements | A4 → A2 |
| GW-T6 requirements | realization | GW-T7 solution concept | A2 → A5 |
| GW-T8 capability map | realization | GW-T9 data entities | B1 → C1 |
| GW-T9 data entities | realization | GW-T10 technology standards | C1 → D1 |
| GW-A5 SoAW | dependency | the delivery contract | Phase A → F |
| `apps/web` route table | reuse | the census | plan §6 |
| `crates/api/src/web.rs` | reuse | D1 serving | plan §5.8 |

## Review

The programme architect owns the register; the Architecture Board reviews the set after the P0
spike and records one compliance level per review. A deviation is a change request, never a silent
fork. The register is imported as typed `architecture_artifact` rows at publication; no row here is
approved before the Board sits.

## Links

- Brief: `00-project-brief.md`
- Architecture: `02-architecture-design.md`
- SoAW: `06-statement-of-architecture-work.md`
- Standard: `seed/output-standard.json` templates `artifact-standard`, `architecture-artifact`
- Spec: `docs/SPEC_V2.md` §8.1, §8.2, D62/D63
