---
document: GEMBA-WALK/statement-of-architecture-work
version: 1
status: draft
project: GEMBA-WALK
category: statement-of-architecture-work
phase: A
plateau: PSA
owner: programme-architect
reviewer: SRO
review_date: 2026-12-03
---

# GEMBA-WALK — Statement of Architecture Work

## Assignment

The Architecture Board assigns the GEMBA-WALK project to design and build a second, game-style
client at `/walk/` as a **candidate** primary UI, under ADR 0021 and the owner's 2026-11-26
direction. The assignment is the Phase-A contract: it authorises the architecture work; it does not
authorise primacy, a backend change or the retirement of `apps/web`.

## Scope

**In:** the static sub-site and its serving fallback; route parity with the 52 `apps/web` patterns;
reachability and schema-rendering of the 597 registered actions; the seven wings and their rooms;
seats as view presets; DOM sheets; the `WorldView` port and canvas-2D baseline; the frozen census and
parity instrument; per-destination Playwright specs; accessibility and performance budgets;
deployment in the existing image.

**Out:** any new table, action, event, port or decision store; a second backend or read model; SSR;
native mobile; chat/audio/video; a fixture or embedded snapshot; `/design/` changes; `apps/web`
retirement before the P6 gate.

**Boundary rule:** a capability the server does not expose is an honest gate. A need for new
backend surface stops the work and escalates; it is never added by this project.

## Responsibilities

| Role | Responsibility |
|---|---|
| Architecture Board | reviews this set and records one compliance level; approves the contract when the repository is used |
| SRO / sponsoring group | accepts the benefit and the P6 evidence; the owner makes the primacy decision |
| programme-architect | owns the architecture set, the artefact register and the contract; never signs its own contract |
| programme-manager | owns the WBS, baseline, registers, the issue queue and the instrument's operation |
| business-analyst | owns the brief and the analysis inception |
| BCM | owns the benefit profile, the baseline and the transition measurement |
| delivery sessions (G0, S1–S4) | own their files and specs; a seam change stops and asks the parent's owner |
| platform / security | owns the serving, identity, headers and secret hygiene |

## Acceptance criteria

1. The P6 gate in `docs/plans/GEMBA-WALK.md` §7: every §10.5 destination and retained V1 route has
   a walk route and a spec asserting empty, error, happy and one refusal or write.
2. Every action in the frozen census is reachable and schema-rendered; an expired exemption fails.
3. The named write flows round-trip with the standard envelope, `If-Match`, `Idempotency-Key` and
   approval; a refusal is shown verbatim with no optimistic mutation.
4. Accessibility: keyboard path for every capability, focus return, contrast, 200 % zoom and
   390 px with no horizontal scroll.
5. Performance: the budgets in the spec §9, with a 5-minute soak; the engine protocol recorded for
   ADR 0022.
6. The owner's primacy decision is recorded; without it `apps/web` remains the default entry.
7. The closure review records the outcome against this assignment; a deviation is a change request.

## Contract

This document is the **Phase-A Statement of Architecture Work**. When the T9 repository is used,
`architecture.contracts_request` records it, `architecture.board_decide` records the Board's
decision with a published `delegation_ref`, and `architecture.contracts_approve` is performed by a
**distinct approver** — never the requester, never a claimed human. The approved body is immutable;
a change is a new version and re-opens the gate.

The design gate and the Phase-G compliance review use a live **delivery contract**, not this SoAW
(programme-architect, #395). The delivery contract is created after Phase F, when the migration and
delivery plan is accepted.

## Links

- Brief: `00-project-brief.md`
- Architecture: `02-architecture-design.md`
- Register: `03-artifact-register.md`
- Plan: `docs/plans/GEMBA-WALK.md` §7, §11
- ADR: `docs/decisions/0021-gemba-walk-client.md`
- Standard: `docs/plans/T9-architecture-capability.md` §4.3–§4.5; `#395`
