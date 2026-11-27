---
document: GEMBA-WALK/msp-profile
version: 2
status: draft
project: GEMBA-WALK
category: benefit-profile
owner: BCM
reviewer: SRO
review_date: 2026-12-03
---

# GEMBA-WALK — MSP profile

> **Amendment 2026-11-27 (CR-WALK-001, ADR 0024).** The client is served at
> `https://game.nonagentic.ai/` with its own public client `conveyor-game`; the identity is owned
> delivery work, not an external redirect. The business case and the risk profile below are
> updated: R-WALK-4 (external redirect) is closed, and R-WALK-6..9 carry the host and identity
> risks.

## 1. Business case addendum

**Why now.** The conveyor has one operator UI, a Leptos console, and one local mock of a different
interaction model. The owner is considering the walk as the primary UI, but the mock has no auth,
no registry and no live rows: it cannot be measured, and it cannot be promoted. The programme also
cannot answer a basic architectural question — is the action contract genuinely UI-independent? — by
assertion. A second client built on the same contract answers it, and the parity instrument makes
the answer falsifiable.

**Options.**

| Option | What it buys | What it costs |
|---|---|---|
| Do nothing — the mock stays local | a design reference | no evidence, no live client, the primacy question stays open |
| Promote the mock | speed | no auth, no registry, fixtures — the coverage record would lie; rejected by ADR 0013/D64 |
| A game mode inside `apps/web` | one bundle | a frame loop (and possibly a WASM engine) in the operator bundle; one release cadence for two surfaces; the engine spike unmeasurable |
| **A separate client at `game.nonagentic.ai` (GEMBA-WALK)** | a real, measurable second client with its own product URL and identity client; a reversible primacy path | a second shell, auth flow and browser suite; the parity instrument and specs are the control |

**Cost and effort.** One static tree, one router registry, one sheet/action-sheet system, one
world port and canvas baseline, one Playwright suite, one instrument, one public Keycloak client
and one host route on the existing service. The work is staged P0–P6
and split across five build issues; no new backend, table, action or port. The recurring cost is
the maintenance of two clients, owned by the same contract and the same registry.

**Investment appraisal.** The benefit is measured by the decision-task instrument in §2, not by the
capability's existence. The case is accepted provisionally: the baseline is `unmeasured` until the
P0 spike measures the same scripted task in `apps/web`; the P6 review decides whether the
instrument shows a real improvement. An unmeasured baseline is reported `unmeasured`, never zero.
No token-spend KPI is used (owner decision 2026-11-25).

## 2. Benefit profile B-WALK-1

| Group | Fields |
|---|---|
| Identification and purpose | **reference** B-WALK-1 · **description** Faster, contract-faithful governance tasks in the operator surface · **category** operational · **objectives** G1 every operator task is reachable in the walk; G2 the action contract is proven UI-independent · **observable outcome** a seat completes the named decision tasks (find the item, read its evidence, record the decision) in the walk in the same or less time than in `apps/web`, with the same refusals, and the owner can choose either client without a backend change |
| Performance | **KPIs** scripted-task completion time for the five named decision flows; actions reachable and schema-rendered (census); destinations with a green spec; first-contentful paint and interaction latency per destination · **baseline** `unmeasured — the same scripted tasks have not been timed in apps/web; the P0 spike measures them before P1` · **target** fixed at the first measured cycle; the P6 gate requires no regression and the owner's primacy decision weighs the delta · **measurement method** the Playwright task harness times the same scripted flow against both clients and records the trace of action invocations; the instrument reads the census, not a live manifest · **financial value** not expressed; the cost side is the maintenance of two clients |
| Cost | the GEMBA-WALK build (P0–P6) and the business change: seats learn a second surface, the delivery team maintains two clients, the team operates one more public client and host route |
| Conditions | **capabilities required** the action registry and `actions.describe`, the PKCE identity flow, the read routes, SSE, the Playwright harness; **outcomes that must already exist** `apps/web` as the normative client, the design pack's serving model, the frozen census; **changes to operating practice** the owner and seats evaluate a task in both clients before primacy; a contract change is tested in both |
| Uncertainty and dependencies | **risks** R-WALK-1 parity theatre, R-WALK-2 maintenance cost, R-WALK-3 engine budgets, R-WALK-6 host boundary, R-WALK-7 `azp` allowlist, R-WALK-8 client drift, R-WALK-9 DNS/tunnel; **dependencies outside the programme** none — the game client and host are delivered in the unit |
| Responsibility and attribution | **who realizes it** the programme architect and the delivery sessions; **benefit owner** BCM; **operational area** programme governance and operator tooling — the areas whose tasks are measured |

## 3. Benefits map

| Field | Value |
|---|---|
| output_ref | GEMBA-WALK — the game client at `game.nonagentic.ai` and its parity instrument |
| capability_ref | any operator task is reachable through a second, contract-faithful client |
| operational_outcome | O-WALK-1 the primary UI can change without a backend change; governance tasks complete in the walk with the same gates |
| benefit | B-WALK-1; disbenefit D-WALK-1 (two clients to maintain) |
| strategic_objective | a contract-faithful, replaceable operator experience |

The edge is one path: the walk's output enables the capability; the capability produces O-WALK-1;
O-WALK-1 is valued as B-WALK-1 and carries the disbenefit D-WALK-1. A slip in the walk puts
B-WALK-1 at risk; a contract change untested in both clients is an O-WALK-1 risk, not a walk build
failure.

## 4. Risks

### R-WALK-1 — parity theatre

| Field | Value |
|---|---|
| Cause | a client that looks finished while destinations or actions are omitted; prose credit |
| Event | the walk is promoted while the census still has `absent` rows or unreachable actions |
| Effect | B-WALK-1 is not realized; the owner decides on appearance, not evidence |
| Owner | programme architect (design), BCM (benefit) |
| Appetite | low; the P6 gate is the control |
| Mitigation | the frozen census and `build-walk-coverage.py --check`; empty/error/happy/refusal/write per destination; an expired exemption fails; no credit without a spec |
| Review date | 2026-12-03 and every P-stage gate |

### R-WALK-2 — two clients, one contract

| Field | Value |
|---|---|
| Cause | a change lands in one client and not the other; the shell, auth and harness drift |
| Event | a contract change passes one client's specs and breaks the other |
| Effect | the parity claim is false; maintenance cost rises |
| Owner | programme architect, delivery team |
| Appetite | medium; the instrument and the trace parity are the control |
| Mitigation | one census; behavioural trace parity on the named flows; a contract test per write; a contract change updates both or is a change request |
| Review date | every contract-changing issue |

### R-WALK-3 — engine budgets

| Field | Value |
|---|---|
| Cause | Bevy's WASM bundle and startup, WebGL requirements, limited text/table widgets |
| Event | the candidate misses the budgets and the spike chooses it anyway |
| Effect | the client is slower or less accessible than the baseline |
| Owner | programme architect, platform lead |
| Appetite | low; the canvas baseline stays |
| Mitigation | the P0 spike measures canvas vs Bevy; ADR 0022 records the winner and budgets; the `WorldView` port keeps the baseline |
| Review date | 2026-12-03 (ADR 0022) |

### R-WALK-6 — the game host leaks an operator surface

| Field | Value |
|---|---|
| Cause | the host boundary is enforced inside the static fallback while `/mcp` or `/design` are router-level surfaces |
| Event | the game host answers an operator surface or the operator index |
| Effect | the client's isolation is false; a user sees the wrong application |
| Owner | programme architect, platform lead |
| Appetite | low |
| Mitigation | a router-level host gate before `mcp_auth`, a host-aware static dispatch, `app()` matrix tests, and live `/mcp` and `/design/*` 404 probes |
| Review date | the G0 gate and every serving change |

### R-WALK-7 — the `azp` allowlist widens the replay guard

| Field | Value |
|---|---|
| Cause | the game client id is defaulted from the audience, or the allowlist accepts more than the two first-party clients |
| Event | an unexpected client's token is accepted, or a correct game token is rejected |
| Effect | a security regression or a broken sign-in |
| Owner | platform lead |
| Appetite | low |
| Mitigation | an explicit optional game client id, exactly two accepted `azp` values, acceptance and rejection tests, the `conveyor` audience unchanged |
| Review date | every auth change |

### R-WALK-8 — the Keycloak client drifts or the Job is skipped

| Field | Value |
|---|---|
| Cause | the realm is edited outside the git-managed patch, or the manual Job is not run |
| Event | the client's redirect, web origin or audience mapper does not match the declaration |
| Effect | the production sign-in fails or the API refuses the token |
| Owner | platform lead / identity operator |
| Appetite | low |
| Mitigation | a create-or-refuse preflight, the manual Job, and an admin-API read-back of the client, redirects, web origins, PKCE and the audience mapper before the live proof |
| Review date | the G0 gate |

### R-WALK-9 — DNS, tunnel or VirtualService lag

| Field | Value |
|---|---|
| Cause | the host is merged but the tunnel ConfigMap, VirtualService or dns-ensure has not reconciled |
| Event | the host does not resolve or answers the wrong origin after the merge |
| Effect | the live proof cannot run; the game appears down |
| Owner | platform lead |
| Appetite | medium |
| Mitigation | read Flux's applied revision and the changed objects back from the cluster; wait for dns-ensure; probe with `getent` and `curl`; no cert change is needed |
| Review date | the G0 gate |

## 5. Stakeholder profiles

| Stakeholder | Concerns / sensitivities | Importance | Influence | Interest | Exposure | Support | Engagement target |
|---|---|---|---|---|---|---|---|
| Owner / sponsoring group | the walk may become primary; the parity claim is measurable and reversible | high | high | high | high | support | P6 decision; reviews the instrument |
| Product owner seat | board and delivery tasks are complete and fast | high | high | high | medium | support | Portfolio/plan wing; write flows |
| Programme office seat | programme, tranche, benefits and ADM tasks are visible | high | high | high | medium | support | Programme/design wings |
| Tenant owner seat | approvals, members and settings are governed; destructive actions refuse | high | high | high | high | support | approvals/settings; refusal tests |
| Reviewer seat | read-only evidence and decisions are legible | medium | medium | high | low | support | read parity; no write affordance |
| Architecture Board | the walk fits the blueprint and its contract | high | high | medium | medium | support | SoAW, artefact set, compliance review |
| Platform / security | same origin, host boundary, the game client, no secret leakage | high | high | high | high | support | host matrix, client read-back, auth tests |
| Identity operator (owner/platform) | the conveyor realm stays the single identity boundary; the game client is git-managed and read back | high | high | high | high | support | realm patch, Job, admin-API read-back |
| Accessibility / UX | the game UI does not regress keyboard or contrast | medium | high | high | medium | support | a11y budget and keyboard spec |
| External stakeholder | reads the public programme pack, not the walk | low | low | medium | low | neutral | `/design/` remains the public surface |

## 6. Tranche authorisation

GEMBA-WALK is a **cross-cutting programme unit under tranche 0** (the `CROSS_CUTTING` convention
used by P0, PLATFORM and DESIGN-PUBLISH), not a new tranche. Its entry is the accepted ADR 0021 and
the published inception set; its exit is the P6 parity gate and the closure review, not a tranche
review. The P6 decision is the owner's primacy decision; a re-baseline of the census is a signed
commit that states what changed, and a scope change is a change request (§7).

## 7. Transition and measurement

The old practice is `apps/web` only; the baseline is measured while it is still the only live
client (the P0 spike times the five named decision flows in `apps/web`, and records the census).
GEMBA-WALK enacts the new practice: the walk reaches parity, the seats use it for the named flows,
and the same scripted tasks are timed in both clients. Post-transition the BCM measures the five
flows and the census, reports the delta against the `apps/web` baseline, and keeps measuring until
the realization plan names who continues after programme closure. The BCM is the benefit owner; the
programme manager is accountable for the instrument's operation. Primacy does not retire
`apps/web`: retirement is a separate decision after a stabilisation period.

## 8. Links

- Brief: `00-project-brief.md`
- Architecture: `02-architecture-design.md`
- Plan: `docs/plans/GEMBA-WALK.md`
- SAFe: `05-safe-epic.md`
- Seed: `seed/programme-definition.json` (the benefit profile and the cross-cutting project)
- Spec: `docs/SPEC_V2.md` §8.3, §10.5.11, D67/D68
- ADR 0024: `docs/decisions/0024-gemba-walk-host-and-identity.md`
- Change request: `08-change-request-001-host-and-identity.md`
