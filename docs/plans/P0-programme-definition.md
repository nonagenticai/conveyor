---
slice: P0
title: Programme definition — vision, blueprint, business case, plan, governance
status: planned
depends_on: []
spec_sections: ["§1", "§4", "§5", "§12", "MSP"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: []
kind: programme-definition
---

# P0 — Programme definition

Programme project: P0 · issues #120, #121, #122

**Track:** programme · **Capabilities:** architecture, programme management, project management,
business analysis · **Gate:** the definition set is published and adopted; no V2 delivery project
starts before it.

## 1. Objective

Design the programme that delivers the full conveyor, following MSP's *Defining a Programme*: the
vision, blueprint, business case, programme plan, governance, reuse map and project briefs. P0 is a
project of the programme it defines, so every artefact in this set is an intermediate output stored
on P0 in the console and published on the design site.

The programme does not start T1–T17 until this set is accepted. The gate is the programme's, not a
document's: a delivery project's brief is the contract it is held to.

## 2. Starting state (measured at the P0 baseline)

- V1 is far implemented: 25 measured-built slices, 2 partial (S14, S21); the Rust workspace, the
  action registry, the board/card/run/approval/document aggregates and the public design pack.
- T0's organisation spine merged (#65, migration 0040, `organisation.*` actions, release hash and
  binding pin); its measured coverage row is corrected by this unit's seed change.
- The programme seed has 47 projects and no programme-definition project. T1–T17 are proposed and
  blocked by their SPEC_V2 §12 edges.
- `docs/SPEC_V2.md` is the design record; `docs/SPEC.md` is the V1 build record. Neither is a
  programme plan; P0 produces the plan that adopts them.

## 3. The definition set (the project's outputs)

1. **Vision / mandate** — what the conveyor programme is for, the outcomes, the boundary.
2. **Blueprint** — the target architecture and operating model: aggregates, services, the action
   registry, the data model, the deployment estate. The to-be the projects deliver.
3. **Business case** — why now, the benefit map, cost/risk posture, the public/opt-in licensing
   frame.
4. **Programme plan** — tranches, the definitive project list and sequencing. It adopts and
   updates the existing seed; it does not invent a parallel list.
5. **Governance & organisation** — roles, decision ladder, assurance, the unit protocol.
6. **Reuse map** — what exists and may be reused: the conveyor V1 tree (tables, actions, modules,
   processes), the estate's absorbed services and modules, the seeded catalogues, the upstream
   crates. For each: reuse candidate, boundary, licence/contract constraint.
7. **Project briefs** — one per delivery project: outcome, boundaries, dependencies, acceptance,
   capabilities, plan link, reuse sources to analyse, and the two inception units below.
8. **Architecture / analysis pack** — current-state analysis, options, risks, dependency map.

## 4. The project brief template (mandatory)

Every delivery project's brief carries:

| Field | Content |
|---|---|
| Outcome | One sentence: what is true after the project that is not true now |
| Boundary | Out of scope, and which existing surfaces it must not fork |
| Reuse sources | Paths inside conveyor; estate services/modules; upstream crates. Each with the analysis question |
| Inception units | `analysis` and `architecture`, the project's first two claims |
| Dependencies | Typed: blocked-by, acceptance-blocked-by, blocks, related |
| Acceptance | BDD scenario per part with a `Verify:` command |
| Capabilities | The organisation positions/capabilities that own it |
| Plan | The slice/S## plan path, or the T## SPEC_V2 section |

## 5. The inception standard (per delivery project)

**Analysis** — current state and reuse: for the project's capability, inventory the code that
already exists (inside the conveyor repo, in the wider estate, and upstream), the contract and
licence constraints, the build-vs-reuse decision, and the risks. It cites `path:symbol` inside the
repo; outside, it names the capability, not a private host or URL.

**Architecture** — the project's design: aggregates, tables, actions, modules, integrations, the
decision record, and acceptance. It must fit the blueprint; a deviation is a decision record, not a
silent fork.

Both are documents on the project with `meta.category` set. They are published when accepted. A
project without both published is an inception gap; its build units are not claimed.

## 6. Acceptance

- The eight artefacts are published as documents on P0.
- The programme plan lists 48 projects (P0 + 47) with tranches and dependencies.
- Every delivery brief carries reuse sources and the two inception units.
- The Validation page shows P0 at tranche 0 and T1–T17 `blocked_by: [P0]`.
- `check-coverage` passes; the seed regen is idempotent; this plan is indexed by `build-docs`.
