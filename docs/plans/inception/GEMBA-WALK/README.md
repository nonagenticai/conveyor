# GEMBA-WALK — inception record

The durable MSP/TOGAF/SAFe set for the Gemba Walk, the separate client authorised by ADR 0021 as a
candidate primary UI. It is modelled on the T19 precedent (`docs/plans/T19/`) and the standard in
`docs/plans/inception/README.md`.

## Status

Drafted 2026-11-26 under the owner's direction. ADR 0021 is **Accepted** as a delegated rule
decision (the build's approver under the published delegation, 2026-11-22, expiry 2027-02-18);
`apps/web` remains normative until the P6 gate. The documents are drafts until the Architecture
Board reviews them (target 2026-12-03, after the P0 spike) and this unit publishes them on the
project. The live publication ids are recorded on issue #792.

## Reading order

1. `00-project-brief.md` — outcome, boundary, reuse, acceptance.
2. `01-inception-analysis.md` — the measured current state and the build-vs-reuse decision.
3. `02-architecture-design.md` — the PSA and HLD set (A1–A6, B1–B2, C1–C3, D1, F1).
4. `03-artifact-register.md` — the typed TOGAF rows and the contract kinds.
5. `04-msp-profile.md` — business case, benefit, risks, stakeholders, tranche, transition.
6. `05-safe-epic.md` — epic, features, enablers, WSJF, PI objectives.
7. `06-statement-of-architecture-work.md` — the Phase-A contract.
8. `07-governance-and-evidence.md` — WBS, baseline, registers, change control, evidence, closure.

Then the plan (`docs/plans/GEMBA-WALK.md`) for stages and the issue split, and
`docs/design/GEMBA-WALK-SPEC.md` for the client architecture.

## Category map

| File | `seed/output-standard.json` category(ies) |
|---|---|
| `00-project-brief.md` | `project-brief` |
| `01-inception-analysis.md` | `inception-analysis` |
| `02-architecture-design.md` | `architecture-design`; carries the A–D artefact content |
| `03-artifact-register.md` | `artifact-standard`, `architecture-artifact`; the 10 mandatory types |
| `04-msp-profile.md` | `business-case`, `benefit-profile`, `benefits-map`, `stakeholder-profiles` |
| `05-safe-epic.md` | `safe-portfolio`, `safe-art`, `safe-cadence` (epic-level) |
| `06-statement-of-architecture-work.md` | `statement-of-architecture-work` |
| `07-governance-and-evidence.md` | `project-wbs`, `project-baseline`, `project-registers`, `change-control`, `evidence`, `closure-review` |

`candidate-solutions`, `gap-analysis` and the migration plan are sections of the phase-E/F views
inside `02-architecture-design.md`; they are not separate outputs. `statement-of-intent` and
`request-for-architecture-work` are Phase-G/H outputs, named in the register and authored at
completion.

## Rules

- A project output is not a benefit. B-WALK-1's baseline is `unmeasured` until the same scripted
  tasks are timed in `apps/web`; never zero, no token-spend KPI.
- A capability the server does not expose is an honest gate; no new table, action, event or port.
- No session writes a human approval, owner or SRO signature. The Board act is a rule decision
  under the published delegation, with a distinct approver identity.
- The mock (`docs/Conveyor-Gemba-Walk.html`) is a local reference and is not committed.

## Links

- Direction: `docs/decisions/0021-gemba-walk-client.md`
- Spec: `docs/design/GEMBA-WALK-SPEC.md`
- Plan: `docs/plans/GEMBA-WALK.md`
- Standard: `docs/plans/P0-programme-definition.md` §4–§6; `.claude/skills/{business-analyst,programme-architect,benefits-manager,quality-manager}`
