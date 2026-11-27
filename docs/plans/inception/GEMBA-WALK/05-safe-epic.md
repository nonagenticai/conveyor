---
document: GEMBA-WALK/safe-epic
version: 1
status: draft
project: GEMBA-WALK
category: epic
owner: programme-manager
reviewer: SRO
review_date: 2026-12-03
---

# GEMBA-WALK — SAFe epic

## Epic

**Hypothesis.** If the conveyor serves a second, contract-faithful client at `/walk/` whose every
read and write goes through the existing registry, then the programme can measure whether the
action contract is UI-independent, and the owner can decide primacy on a frozen census and
per-destination evidence instead of a mock.

**Outcome.** The P6 gate: every §10.5 destination and retained V1 route has a walk route and a
Playwright spec asserting empty, error, happy and one refusal or write; every action in the frozen
census is reachable and schema-rendered; the named write flows round-trip with the correct headers;
the accessibility and performance budgets are met; the owner signs the primacy decision — or the
walk remains an alternative client.

**Non-goals.** A second backend, decision store or read model; SSR; native mobile; chat, audio or
video; customer care; a replacement for `/design/`; a public unauthenticated mode; pixel parity
with the mock; retiring `apps/web` before the gate; any new table, action, event or port.

## Features

| ID | Feature | Parts | Acceptance (the plan's `Verify:`) |
|---|---|---|---|
| F1 | The served static sub-site and its fallback | W2 | `apps/walk` static build + the `web.rs` fallback test |
| F2 | Read parity across the §10.5 destinations | W3 | per-destination specs: loading, empty, partial, error, denied |
| F3 | Write parity and the server's gates | W4 | contract test per write: envelope, `If-Match`, `Idempotency-Key`, approval, refusal |
| F4 | Retained V1 surfaces | W5 | every retained route and its spec |
| F5 | Slice-dependent surfaces mirror the owning slice | W6 | a gate flips live only when the owning slice does |
| F6 | The frozen census and the parity instrument | W0 | `python3 scripts/build-walk-coverage.py --check` |
| F7 | Seats, world and accessibility | W2–W3 | keyboard-only spec; focus return; contrast; 390 px / 200 % |
| F8 | The P6 gate and the primacy runbook | W7 | the instrument green, a11y/perf met, the owner's decision |

## Enablers

| ID | Enabler | Serves |
|---|---|---|
| E1 | `docs/evidence/walk-census.json` + `scripts/build-walk-coverage.py` + exemptions | F6, F8 |
| E2 | The PKCE identity flow, the dev cookie and the external `/walk/` redirect URI | F1 |
| E3 | The schema-driven action sheet over `actions.describe` | F2, F3 |
| E4 | The `WorldView` port and the canvas-2D adapter | F7 |
| E8 | `crates/walk-world` — the `bevy_ecs` simulation core (entities, components, schedules) and its headless tests | F7 |
| E9 | The transport bridge: the manifest, intents, receipts and SSE deltas at the wasm boundary | F2, F7 |
| E10 | The replay and determinism harness, including the schedule-ambiguity test | F7, F8 |
| E11 | The `FrameView` adapter contract shared by canvas-2D and WebGL2 | F7 |
| E5 | The scoped `/walk/*` fallback in `crates/api/src/web.rs` and its Rust test | F1 |
| E6 | The Playwright harness and the scripted decision-task traces | F2–F4, F8 |
| E7 | The walk CI workflow, the Dockerfile copy and the static build | F1, F8 |

## WSJF

The score is computed from four typed integer inputs; the score itself is never stored.

| Epic | Business value | Time criticality | Risk reduction | Job size | Computed |
|---|---:|---:|---:|---:|---:|
| GEMBA-WALK second client | 8 | 5 | 8 | 13 | (8 + 5 + 8) / 13 = **1.6** |

The inputs are the ranking record. A change to any input re-ranks the epic; an LLM may draft the
narrative, never the numbers. The five build issues are ranked by the same four inputs when they
are created; S1–S3 are parallel and equal-sized by construction, S4 carries the gate.

## PI objectives — inception PI

| ID | Objective | Committed | Business value | Demo evidence |
|---|---|---|---:|---|
| PI-1 | The inception set is published and the project is in the live register | committed | 8 | document ids, contract id, register row |
| PI-2 | The census and instrument reproduce 52 routes and 597 actions without a server | committed | 8 | `build-walk-coverage.py --check` |
| PI-3 | The spike proves PKCE, SSE, schema forms and a deep link on the dev origin | committed | 13 | the spike record; ADR 0022 |
| PI-4 | The foundation serves `/walk/` and completes a Portfolio read + one write | committed | 13 | Playwright foundation spec |
| PI-5 | Read parity across the seven wings | uncommitted | 8 | per-destination specs |
| PI-6 | Write parity with headers, approval and refusals | uncommitted | 13 | contract tests |
| PI-7 | Retained V1 and the slice-dependent gates | uncommitted | 5 | the V1 specs and the gate audit |
| PI-8 | P6: the instrument green, budgets met, primacy decided | uncommitted | 8 | the P6 runbook and the owner's decision |

The committed set is the inception and foundation minimum. The uncommitted set is pulled in stage
order; no objective is dropped from the P6 gate.

## Dependencies

| Relation | Artefact | Why |
|---|---|---|
| blocked-by | #792 | the inception set must be published before G0 is claimed |
| blocks | G0 (to create) | the build unit's W0–W2 |
| blocks | S1–S4 (to create), file-disjoint | the wings and the P6 gate |
| related | #395 | the delivery contract the design gate reads |
| related | #526 | the programme publish transport this project's artefacts use |
| acceptance-blocked-by | external (owner/ops) | the `/walk/` OIDC redirect registration |

## Demo evidence

The system demo exhibits: the census reproducing the baseline; the instrument failing on an
unreachable action and passing after the fix; the spike's PKCE redirect, SSE update, a read form, a
write with `If-Match`, a destructive action refused without approval, and a deep link; the
foundation's Portfolio read and one real write; one read and one write per wing; the retained V1
routes; the a11y and performance measurements; the P6 decision. No demo, no credit.

## Inspect & adapt

The retro reads the five scripted decision-task times in both clients, the census's absent/
unreachable counts, the spec pass rate, the accessibility findings and the performance budgets. A
systemic contract finding becomes a change request on both clients; a UX finding becomes a kata
candidate (T5), not a walk-only patch. A destination that cannot reach parity without backend
surface is a stop-and-escalate, never a backend addition.

## Links

- Plan: `docs/plans/GEMBA-WALK.md`
- MSP: `04-msp-profile.md`
- Architecture: `02-architecture-design.md`
- Spec: `docs/design/GEMBA-WALK-SPEC.md` §11–§13
