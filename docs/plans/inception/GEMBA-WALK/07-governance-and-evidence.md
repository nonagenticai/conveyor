---
document: GEMBA-WALK/governance-and-evidence
version: 1
status: draft
project: GEMBA-WALK
category: project-wbs
phase: E–G
plateau: HLD + capability
owner: programme-manager
reviewer: SRO
review_date: 2026-12-03
---

# GEMBA-WALK — WBS, baseline, registers, change control and evidence

## Work packages

| ID | Work package | Stage | Owner | Acceptance (the plan's `Verify:`) | Status |
|---|---|---|---|---|---|
| W0 | Governance, census and parity instrument | P0 | programme-manager | `python3 scripts/build-walk-coverage.py --check` | in progress (#813) |
| W1 | Renderer spike: auth, SSE, schema forms, deep link, canvas-2D vs WebGL2; ADR 0023 | P0 | programme-architect | `docs/evidence/walk-spike.md`; ADR 0023 | done 2026-11-27 (run 45953201576) |
| W1a | `crates/walk-world`: entities, components, resources, schedules, projection (§1–§9 of the ECS design) | P1 | delivery (G0) | the headless tests in `walk-world` | planned |
| W1b | The command path and the wasm boundary: manifest, intents, receipts, deltas (§8, §11) | P1 | delivery (G0) | refusal + same-frame ordering tests | planned |
| W1c | The replay/determinism harness and the ambiguity test (§10, §14) | P1 | delivery (G0) | `replay_is_deterministic`, `no_schedule_ambiguity` | planned |
| W1d | The `FrameView` adapter and the canvas-2D renderer (§13) | P1 | delivery (G0) | the world Playwright spec | planned |
| W2 | Foundation + Portfolio end-to-end + serving fallback | P1 | delivery (G0) | Playwright foundation spec; `web.rs` test | planned |
| W3 | Read parity across the seven wings | P2 | S1–S3 | per-destination specs | planned |
| W4 | Write parity and gates | P3 | S1–S3 | contract tests per write | planned |
| W5 | Retained V1 surfaces | P4 | S4 | every retained route and spec | planned |
| W6 | Slice-dependent surfaces and gate audit | P5 | S4 | the gate audit matches `apps/web` | planned |
| W7 | P6 parity gate and the primacy runbook | P6 | programme-manager + owner | instrument green; a11y/perf; the owner's decision | planned |
| W8 | Hardening and the closure evidence | P6 | delivery (S4) | the closure review | planned |

The WBS is the build order; the issue split is by file ownership: G0 = W0–W2, S1–S3 = W3–W4 by
wing, S4 = W5–W8. The five build issues are created only after the inception set is published
(#792); the split is the written exception approved for this plan.

## Baseline

| Field | Value |
|---|---|
| Scope | the 52 `apps/web` route patterns, the 43 screens, the 46 specs and the 597 actions frozen in the census at P0; the seven wings; the retained V1 surfaces; the serving fallback; the instrument; the P6 gate |
| Schedule | stage order and gates, not calendar dates: P0 (census, spike, ADR 0022) → P1 (foundation + Portfolio) → P2 (reads) → P3 (writes) → P4 (V1) → P5 (slice-dependent) → P6 (gate). Each stage starts only when the previous gate is green. |
| Cost | five build issues (G0 + S1–S4) plus the inception issue #792; one static sub-site, one browser suite, one instrument. No new runtime or service cost. No token-spend KPI (owner decision 2026-11-25). |
| Approval | this baseline is approved with the inception set; a scope change is a change request (§Change control), never a silent edit |

The baseline is re-measured only by a signed commit that states what changed; the census is the
frozen comparator, not a live manifest.

## Project registers

### Risks

| ID | Risk | Owner | Appetite | Mitigation | Review |
|---|---|---|---|---|---|
| R-WALK-1 | parity theatre: a client that looks finished while destinations/actions are omitted | programme architect | low | census + instrument + per-destination specs; no credit without a spec | each stage gate |
| R-WALK-2 | two clients drift; a contract change lands in one | programme architect | medium | one census; behavioural trace parity; a contract test per write | every contract change |
| R-WALK-3 | the engine misses bundle/startup/accessibility budgets | platform lead | low | canvas baseline; the P0 spike; ADR 0022 | P0 |
| R-WALK-4 | the `/walk/` OIDC redirect is not registered | owner/ops | medium | dev cookie; `acceptance-blocked-by`; no production sign-in claim | each gate |
| R-WALK-5 | no local Rust builds slow the wasm loop | platform lead | medium | GitHub preflight for the compile verdict; the baseline needs no Rust | each build issue |

### Issues

| ID | Issue | Raised | Owner | Status |
|---|---|---|---|---|
| I-WALK-1 | the inception set is not live until #792 publishes it | 2026-11-26 | programme-manager | open (#792) |
| I-WALK-2 | `/walk/*` deep links currently render the operator app | 2026-11-26 | delivery (G0) | open (W2) |
| I-WALK-3 | no walk route, app or spec exists | 2026-11-26 | delivery (G0) | open (W2) |

### Assumptions

| ID | Assumption | Validation |
|---|---|---|
| A-WALK-1 | the action registry and `actions.describe` are the sole authority for reachability | census + instrument at P0 |
| A-WALK-2 | the same image and origin can serve the static sub-site | `web.rs` fallback test in W2 |
| A-WALK-3 | the identity operator will register the `/walk/` redirect | the dev cookie until it is; `acceptance-blocked-by` |
| A-WALK-4 | the Playwright harness can time the same scripted task in both clients | the P0 spike's trace |

### Dependencies

| ID | Dependency | Type | Owner | Status |
|---|---|---|---|---|
| D-WALK-1 | #792 inception publication | blocked-by (build) | programme-manager | open |
| D-WALK-2 | the `/walk/` OIDC redirect registration | acceptance-blocked-by (external) | owner/ops | open |
| D-WALK-3 | #395 the live delivery contract for the design gate | related | programme-architect | open |
| D-WALK-4 | #526 the programme publish transport | related | programme-manager | open |
| D-WALK-5 | slice-dependent surfaces (T13–T15, T19, T7, T9, T17) | related (W6 audit) | the owning slices | planned |

### Benefits

| ID | Benefit | Profile | Baseline | Status |
|---|---|---|---|---|
| B-WALK-1 | faster, contract-faithful governance tasks in the operator surface | `04-msp-profile.md` §2 | `unmeasured` until the P0 spike times the same tasks in `apps/web` | draft |

### Changes

| ID | Change | Decision | Date |
|---|---|---|---|
| C-WALK-1 | GEMBA-WALK admitted to the programme as a cross-cutting unit under tranche 0 | ADR 0021 accepted; this inception (#792) | 2026-11-26 |
| C-WALK-2 | the five-issue build split approved as the written exception | ADR 0021 consequences; plan §8 | 2026-11-26 |

## Change control

A change to scope, baseline, the census or the architecture goes through the `change-control`
template: the request, its impact (scope/schedule/cost/benefit/risk), the affected contract and the
decision. A re-baseline is a signed commit stating what changed. A deviation from the architecture
(a second backend, a fixture, a canvas-only work surface, a seat that grants authority) is a change
request; a silent fork is refused. A change after the SoAW approval is a new contract version and
re-opens the gate.

## Evidence plan

| Issue | Stage | Evidence (one row per scenario) | Record |
|---|---|---|---|
| #792 | inception | document ids and hash-equal bodies; `contracts_get`; the 10 artefact rows; the benefit profile; the re-audit | this issue's `Evidence at completion` |
| G0 (to create) | P0–P1 | the census reproduction (52/597); the instrument's four failure modes; the spike record; ADR 0022; the fallback Rust test; the foundation Playwright spec | the PR body + issue |
| S1–S3 (to create) | P2–P3 | per-destination specs (empty/error/happy/refusal/write); the contract tests; the instrument's rows | each PR body |
| S4 (to create) | P4–P6 | all 52 routes; the gate audit; the a11y/perf measurements; the P6 runbook; the owner's decision | the PR body + the closure review |

A live or end-to-end part is accepted only by a fresh verifier (`validator-deep`/`aa-pr-auditor`),
never by the builder. The evidence table names the command and the SHA; a claim without a record is
absent.

## Closure review

The project closes only on the P6 gate, not on "the UI looks finished": the instrument green with
no open exemption; every destination `verified`; the named flows round-trip; a11y and performance
met; the owner's primacy decision recorded (or the walk recorded as an alternative client); the
delivery contract's compliance review at one level; the benefit profile's post-measurement recorded
or `unmeasured`. A closed project updates the slice's coverage row and the programme register; a
retirement of `apps/web` is a separate, later decision.

## Links

- Brief: `00-project-brief.md`
- Architecture: `02-architecture-design.md`
- MSP: `04-msp-profile.md`
- SAFe: `05-safe-epic.md`
- SoAW: `06-statement-of-architecture-work.md`
- Plan: `docs/plans/GEMBA-WALK.md` §7–§8, §11
- Templates: `seed/output-standard.json` (`project-wbs`, `project-baseline`, `project-registers`, `change-control`, `evidence`, `closure-review`)
