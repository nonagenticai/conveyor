---
document: GEMBA-WALK/artifact-register
version: 2
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

> **Amendment 2026-11-27 (CR-WALK-001, ADR 0024).** The host and the game's public client
> change the solution concept, the requirements catalog and the technology view; those typed rows
> supersede to **version 2** (the live rows are v1; no v2 was recorded for the world layer, so the
> new instances supersede v1 directly), and the GW-A2/A5/A6 documents move to
> version 2. The data entity catalog is unchanged: no new entity is introduced.

The typed catalogue of the GEMBA-WALK set. Each row carries the T9 standard's common metadata:
title, status, version, owner, reviewer, review date, purpose, stakeholders, scope, related project
and its published document. Links use `derivation | realization | reuse | dependency`.

## Document catalogue

| ID | Artefact | Type | Phase / plateau | Status | Version | Owner | Reviewer | Review date | Document |
|---|---|---|---|---|---|---|---|---|---|
| GW-A0 | Project brief | project-brief | Preliminary / definition | draft | 1 | business-analyst | SRO | 2026-12-03 | `00-project-brief.md` |
| GW-A1 | Inception analysis | inception-analysis | Preliminary / definition | draft | 1 | business-analyst | programme-architect | 2026-12-03 | `01-inception-analysis.md` |
| GW-A2 | Architecture design (PSA + HLD) | architecture-design | A–F / PSA+HLD | draft | 2 | programme-architect | Architecture Board | 2026-12-03 | `02-architecture-design.md` |
| GW-A3 | MSP profile (business case, benefit, stakeholders, tranche) | benefit-profile | Preliminary / definition | draft | 1 | BCM | SRO | 2026-12-03 | `04-msp-profile.md` |
| GW-A4 | SAFe epic (features, enablers, WSJF, PI) | epic | F / HLD | draft | 1 | programme-manager | SRO | 2026-12-03 | `05-safe-epic.md` |
| GW-A5 | Statement of Architecture Work | statement-of-architecture-work | A / PSA | draft | 2 | programme-architect | SRO | 2026-12-03 | `06-statement-of-architecture-work.md` |
| GW-A6 | WBS, baseline, registers, change control | project-wbs | E–F / HLD | draft | 2 | programme-manager | SRO | 2026-12-03 | `07-governance-and-evidence.md` |
| GW-A7 | This register | catalog | A–F / PSA+HLD | draft | 2 | programme-architect | Architecture Board | 2026-12-03 | this file |
| GW-A8 | Implementation evidence | evidence | G / capability | draft | 1 | programme-manager | Architecture Board | 2026-12-03 | `07-governance-and-evidence.md` §Evidence |
| GW-A9 | Closure review | closure-review | G / capability | draft | 1 | programme-manager | Architecture Board | 2026-12-03 | to create at P6 |

## Typed TOGAF rows

The ten types the default ADM profile marks mandatory for the phases this project walks. The typed
rows are created on the project at publication; this table is their source.

| ID | `artifact_type.key` | Name | Type | Phase / plateau | Purpose | Stakeholders | Scope | Status |
|---|---|---|---|---|---|---|---|---|
| GW-T1 | `principles_catalog` | Principles Catalog | catalog | Preliminary / PSA | the principles the client is built to | Board, delivery team | A1 | approved |
| GW-T2 | `principle_derivation` | Principle Derivation Diagram | diagram | Preliminary / PSA | how each principle derives from a driver/goal | Board | A1 ↔ A4 | approved |
| GW-T3 | `principle_realization` | Principle Realization Diagram | diagram | Preliminary / PSA | where each principle shows in the design | Board, delivery team | A1 ↔ A5/C2 | approved |
| GW-T4 | `stakeholder_map` | Stakeholder Map | matrix | A / PSA | who the client serves and who may veto it | SRO | A3 | approved |
| GW-T5 | `driver_goal_objective` | Driver/Goal/Objective Catalog | catalog | A / PSA | why the client exists, measured | SRO | A4 | approved |
| GW-T6 | `requirements_catalog` | Requirements Catalog | catalog | A / PSA | the typed requirements the gate checks | SRO, Board | A2 | approved |
| GW-T7 | `solution_concept` | Solution Concept Diagram | diagram | A / PSA | the client and its contract, at a glance | Board | A5 | approved |
| GW-T8 | `business_capability_map` | Business Capability Map | catalog | B / HLD | the wings and seats the client serves | SRO | B1/B2 | approved |
| GW-T9 | `data_entity_catalog` | Data Entity/Data Component Catalog | catalog | C / HLD | the entities read and written; no new table | Board | C1 | approved |
| GW-T10 | `technology_standards` | Technology Standards Catalog | catalog | D / HLD | the serving, build, test and engine standards | platform lead | D1 | approved |

Each row's document is `02-architecture-design.md` except the register (this file); each derives
from A1–A6 and realises into C1/C2/D1. A row is approved only by the Architecture Board after the
P0 spike; the ten rows below were approved live on 2026-11-27 and are read back in the next
subsection.

### Live typed rows (read back 2026-11-27)

`architecture.artifacts_list {project_id: 01a21af4-f064-79b3-b302-7d9d81f9d072}` returned thirteen
rows: ten `approved` and three `superseded` (the superseded originals are history, not the current
row).

| Type | Current row | Version | State |
|---|---|---|---|
| `principles_catalog` | `01a21af6-72f2-73b0-ae5f-a3fb44f0280f` | 2 | approved |
| `principle_derivation` | `01a21af6-52e2-7a3c-b96d-16737ec0957e` | 2 | approved |
| `principle_realization` | `01a21af6-62d5-7d0f-b6c0-34554fbfba10` | 2 | approved |
| `stakeholder_map` | `01a21af6-e936-745f-bc14-daba1c2a9634` | 2 | approved |
| `driver_goal_objective` | `01a21af6-417c-7f50-969b-1b28cf8c3745` | 2 | approved |
| `business_capability_map` | `01a21af6-21b7-7221-a3b2-0d2523d791a0` | 2 | approved |
| `data_entity_catalog` | `01a21af6-31c7-75e2-94d7-ba6f911d65a7` | 2 | approved |
| `requirements_catalog` | `01a21bfa-9804-7a50-bbea-d6f669c7a9a0` | 3 | approved |
| `solution_concept` | `01a21bfa-9e5a-7f80-b0b9-20244317b4c9` | 3 | approved |
| `technology_standards` | `01a21bfa-a48d-7e4a-a537-acb72da5e609` | 3 | approved |

The superseded rows are `01a21af6-83d5-7763-8cdb-253291aba8a7` (requirements),
`01a21af6-95ef-75fe-b947-9b12ed9083e9` (solution) and `01a21af6-ac39-7881-b532-07e13e1e6af7`
(technology), each at version 3 with state `superseded`. The approvals were recorded with the
inception publication (#792 close-out); the documents' own review date remains 2026-12-03.

### Version 2 rows — host and identity (CR-WALK-001, ADR 0024, 2026-11-27)

The three rows below change because the client is now served at `https://game.nonagentic.ai/` with
its own public client; the new row supersedes the live v1 row (`architecture.artifacts_supersede`).
Executed 2026-11-27: the three v3 rows are approved and the v1 rows read `superseded` (ids above).

| Key | Version 2 change | Source |
|---|---|---|
| `solution_concept` | the game host, the host boundary and the 308 replace the `/walk/` sub-site in the deployment view | ADR 0024; `02-architecture-design.md` D1 |
| `requirements_catalog` | adds R17 (host boundary), R18 (`azp` allowlist), R19 (live sign-in) | ADR 0024 |
| `technology_standards` | `conveyor-game` (public, PKCE S256, redirect `https://game.nonagentic.ai/*`, web origin, `conveyor` audience), Keycloak realm-patch Job, cloudflared/VS host, `CONVEYOR_GAME_*` env | ADR 0024; CR-WALK-001 |

### v2 rows — the world layer (#806, ADR 0022)

Four rows move to version 2 when the `bevy_ecs` world layer is accepted, because the world changes
what they say; the other six are unchanged. The v2 row supersedes the v1 row rather than editing it
(`architecture.artifacts_supersede`), so the v1 text stays readable as history.

Executed 2026-11-27: `data_entity_catalog` is the approved v2 row above; the other three were then
superseded by the CR-WALK-001 v3 rows, so their world-layer text is readable on the superseded rows,
not on the current ones.

| Key | v2 change | Source |
|---|---|---|
| `solution_concept` | the world is a `bevy_ecs` simulation crate behind `WorldView`, not an undecided renderer | `GEMBA-WALK-ECS.md` §1–§2 |
| `requirements_catalog` | adds `R-ECS-1..10`: state classes, intent-only writes, no persisted programme copy, idempotent projection, determinism, wasm constraints, budgets, adapter boundary, no tokens, extension rules | `GEMBA-WALK-ECS.md` §1, §8–§16 |
| `data_entity_catalog` | the entity/component mapping to the existing tables; no new table | `GEMBA-WALK-ECS.md` §3–§5 |
| `technology_standards` | `bevy_ecs`/`bevy_app`/`bevy_time` `=0.20.1`, wasm32, cargo-leptos for `apps/web` only, `panic=abort` | `GEMBA-WALK-ECS.md` §2, §11 |

`GEMBA-WALK-ECS.md` also becomes its own `architecture-artifact` row (phase D / HLD, owner
programme-architect, reviewer Architecture Board), linked `realization` from `solution_concept` and
`derivation` into `technology_standards`.

## Contract kinds

| Kind | Phase | Subject | Status | Document |
|---|---|---|---|---|
| `statement_of_architecture_work` | A | Assignment, scope, responsibilities, acceptance | **live** v2, `01a21bf7-f6f5-7a41-a88e-12389b89b42d`, approved 2026-11-27 by `01a1e8a7-8b23-76a9-9114-aafb95f7a545` (gate `01a21bf8-0bd5-779e-9a2d-9824bb1e2b41`) | `06-statement-of-architecture-work.md` |
| `delivery_contract` | F | Delivery commitments and the compliance method | **live** v2, `01a21bfb-1241-7db1-9874-eacca12914b4`, approved 2026-11-27 by `01a1e8a7-8b23-76a9-9114-aafb95f7a545` (gate `01a21bfb-2710-7e9c-b872-90ccac7f68ec`) | plan §11 |
| `statement_of_intent` | G | Intended use, ownership, measures of effectiveness | to create at P6 | plan §7 |
| `request_for_architecture` | H | The change request that follows the closure | to create if needed | — |

The design gate and the Phase-G compliance review use the live **delivery contract**, never the
SoAW (programme-architect, #395). The SoAW is the Phase-A assignment and this register's contract
row.

## Common metadata

- **Status:** the ten typed rows are approved (read back 2026-11-27); this catalogue's own document
  review remains open until the Board's document review, dated 2026-12-03.
- **Version:** a change after approval is a new version and re-opens the gate; the three superseded
  originals stay readable as history.
- **Owner / reviewer / review date:** per row above.
- **Purpose:** give the client a typed architecture record that imports into the T9 repository.
- **Stakeholders and scope:** the GEMBA-WALK project; no other project is changed.
- **Published documents (read back 2026-11-27, all `active`):** project brief
  `01a21af5-3413-736b-935b-fdfff5968e78` v2; inception analysis
  `01a21af5-53ff-70b5-a706-686a3cd4ba9c` v2; architecture design
  `01a21af5-652e-7739-b294-7bb662e13d36` v2; CR-WALK-001
  `01a21bf9-ea76-7286-9cd8-e3127df29d09` v2; ADR 0024
  `01a21bf9-e2d7-735d-9780-ea3529249c9c` v1; world layer
  `01a21afb-16c3-7140-81e4-93ef872ea6fb` v1 (from #813 and #806 close-outs; `documents.list`).

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
