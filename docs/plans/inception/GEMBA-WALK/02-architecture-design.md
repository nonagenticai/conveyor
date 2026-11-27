---
document: GEMBA-WALK/architecture-design
version: 2
status: draft
project: GEMBA-WALK
category: architecture-design
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-12-03
phase: A–F
plateau: PSA + HLD
---

# GEMBA-WALK — Architecture design (PSA and HLD)

> **Amendment 2026-11-27 (CR-WALK-001, ADR 0024).** The client is served at
> `https://game.nonagentic.ai/` at the root from the same image and service, with its own public
> client `conveyor-game` in the `conveyor` realm; the operator origin's `/walk/*` 308s to it. The
> host boundary, the `azp` allowlist and the live sign-in are added below (R17–R19, AD10, D1). The
> earlier `/walk/` sub-site is retained only as the no-host dev/CI shape.

## Context and boundary

GEMBA-WALK adds a **second client**, not a second system. It is a static client at `https://game.nonagentic.ai/`, served at the
root from the same image and service as the API (so the API is same-origin), with its own router,
sheets, world, identity client and Playwright suite. It adds no table, action, event, port, decision store or read model. Every read is a literal
`GET` route where one exists, or the owning read action (`programmes.list` is one); every write is `POST /api/actions/{name}/invocations` with the
standard envelope and headers; a capability the server does not expose is an honest gate.

The design covers the client architecture, the route/action parity model, the `WorldView` port, the
seats-as-presets rule, the serving fallback and the parity instrument. `apps/web` remains the
normative operator UI until the P6 gate. The design pack at `/design/` is unchanged.

This is a tailored ADM set for a software delivery project; it is a document set now and imports as
typed `architecture_artifact` rows when the T9 repository is used. Every artefact carries the
standard's common metadata: title, status, version, owner, reviewer, review date, purpose,
stakeholders, scope, related project and its published document; links use
`derivation | realization | reuse | dependency`.

## Architecture artefacts

| ID | Artefact | Type | Phase / plateau | Version | Owner | Reviewer | Review date | Status |
|---|---|---|---|---|---|---|---|---|
| A1 | Principles applied | catalog | Preliminary–A / PSA | 1 | programme-architect | Architecture Board | 2026-12-03 | draft |
| A2 | Requirements catalog | catalog | A / PSA | 1 | business-analyst | programme-architect | 2026-12-03 | draft |
| A3 | Stakeholder map | matrix | A / PSA | 1 | business-analyst | SRO | 2026-12-03 | draft |
| A4 | Drivers, goals, objectives | catalog | A / PSA | 1 | programme-manager | SRO | 2026-12-03 | draft |
| A5 | Solution concept | diagram | A / PSA | 1 | programme-architect | Architecture Board | 2026-12-03 | draft |
| A6 | Statement of Architecture Work | contract | A / PSA | 1 | programme-architect | SRO | 2026-12-03 | draft |
| B1 | Business interaction matrix | matrix | B / HLD | 1 | business-analyst | SRO | 2026-12-03 | draft |
| B2 | Actor / role matrix | matrix | B / HLD | 1 | business-analyst | programme-architect | 2026-12-03 | draft |
| C1 | Data entity catalog | catalog | C / HLD | 1 | programme-architect | Architecture Board | 2026-12-03 | draft |
| C2 | Application / function matrix | matrix | C / HLD | 1 | programme-architect | Architecture Board | 2026-12-03 | draft |
| C3 | Conceptual data diagram | diagram | C / HLD | 1 | programme-architect | Architecture Board | 2026-12-03 | draft |
| D1 | Technology view | catalog | D / HLD | 2 | platform lead | Architecture Board | 2026-12-03 | draft |
| F1 | Migration and delivery diagram | diagram | F / HLD | 1 | programme-manager | SRO | 2026-12-03 | draft |

The published document is this file for every row; the future typed repository rows derive from it.
No row is approved yet; the review date is the Architecture Board's sitting after the P0 spike.

## Aggregates, tables and actions

### Project Start Architecture (Phase A–Preliminary)

### A1 — Principles applied (catalog)

| ID | Principle | Rationale | Implication |
|---|---|---|---|
| P1 | The contract is the product | the walk exists to prove the action contract is UI-independent | every read/write is the existing route/action; no client-only capability |
| P2 | One backend, one decision store | a second store or backend makes the record ambiguous | no new table, action, event, port or read model |
| P3 | The sheets own the work | a canvas cannot carry tables, long text or audit-quality forms | every record, form and decision is a DOM sheet with HTML semantics |
| P4 | The world never gates a capability | not every user will walk or use a pointer | every room, record and action has a deep link and a keyboard path |
| P5 | Seats are presets, not privileges | authority lives in the manifest, scopes and server refusals | changing a seat changes the landing and emphasis only |
| P6 | No fixtures, no client store | a fixture makes the measurement lie | the client renders real rows or the honest gate; no snapshot as the app's source |
| P7 | Parity is measured | prose cannot prove a UI | the frozen census and per-destination specs are the only credit |
| P8 | Same identity, same origin | a second identity path doubles the attack surface | auth-code + PKCE, token in memory, tenant header, no `localStorage` token |
| P9 | The renderer is replaceable | a rendering choice must not trap the client | the simulation is `bevy_ecs` behind `WorldView` (ADR 0022); the P0 spike measured canvas-2D against WebGL2 and ADR 0023 selects canvas-2D, with WebGL2 behind the port |
| P10 | The walk is reversible | primacy must not be a one-way door | additive surface, redirect map and an `apps/web` retirement as a later decision |

### A2 — Requirements catalog (typed)

| ID | Type | Requirement | Source |
|---|---|---|---|
| R1 | condition | Every §10.5 destination and every retained V1 route has a walk route equal to the app path on the game host | spec §4, plan §7, ADR 0024 |
| R2 | condition | Every action in the frozen census is reachable and schema-rendered, with a labelled JSON fallback | spec §6.5, plan §6 |
| R3 | constraint | Every write uses the standard envelope, `If-Match`/`Idempotency-Key`, approval and the server's refusal verbatim | spec §5, §9 |
| R4 | constraint | No new table, action, event, port or decision store | ADR 0021 decision 3 |
| R5 | condition | Auth-code + PKCE S256, token in memory, tenant header, dev cookie locally | spec §9, ADR 0021 decision 4 |
| R6 | condition | SSE via fetch stream, `Last-Event-ID`, `resync`; a subscribed screen reflects its topic | spec §6.4 |
| R7 | constraint | On the game host a deep link serves the walk entry; the operator origin 308s `/walk/*`; both tested | spec §10, ADR 0024 |
| R8 | condition | The world is a `WorldView` port; the canvas baseline is always available | spec §6.2–§6.3 |
| R9 | constraint | Seats never change authority; a control absent from the manifest is not rendered | spec §3 |
| R10 | condition | Accessibility: keyboard path for every capability, focus return, contrast, 200 % zoom, 390 px | spec §8 |
| R11 | condition | Performance budgets: the entrance and each destination measured; a 5-minute soak | spec §9, plan §11 |
| R12 | constraint | No fixture, no snapshot URL fetched by a signed-in walk, no `localStorage` programme state | spec §14 |
| R13 | condition | The parity instrument runs with no server, no database and no network; an expired exemption fails | plan §6 |
| R14 | assumption | The action registry, read routes and identity provider remain the sole authority | spec §5 |
| R15 | delta | The game has its own public client `conveyor-game` (PKCE S256, redirect `https://game.nonagentic.ai/*`, web origin, `conveyor` audience); it is delivered and read back, not deferred | CR-WALK-001, ADR 0024 |
| R16 | condition | The walk adds no port, image or service; it is a second host on the same image; the static tree is copied into the image | spec §10 |
| R17 | constraint | On the game host the server serves only the walk tree, `/config.json` and the API; `/mcp` and `/design` are refused before their routes run; the operator index is never served | ADR 0024, spec §10 |
| R18 | constraint | The API accepts exactly the two first-party `azp` values (`conveyor-web`, `conveyor-game`) against the `conveyor` audience; an unknown or missing `azp` is rejected | ADR 0024 |
| R19 | condition | The production sign-in is proven by a real `conveyor-e2e` PKCE sign-in on the game host, not a redirect | ADR 0024, spec §11 |

### A3 — Stakeholder map (matrix)

| Stakeholder | Concern | Influence | Interest | Engagement |
|---|---|---|---|---|
| Owner / sponsoring group | the walk may become primary; the parity claim must be measurable | high | high | P6 primacy decision; reviews the instrument |
| Product owner seat | the board and delivery tasks are complete and fast | high | high | Portfolio/plan wing; write flows |
| Programme office seat | programme, tranche, benefits and ADM tasks are visible | high | high | Programme/design wings |
| Tenant owner seat | approvals, members, tenant settings are governed | high | high | Approvals/settings; destructive actions show refusals |
| Reviewer seat | read-only evidence and decisions are legible | medium | high | read parity; no write affordance |
| Architecture Board | the walk fits the blueprint and its contract | high | medium | SoAW, artefact set, compliance review |
| Platform / security | same origin, headers, identity, no secret leakage | high | high | serving fallback, CSP, auth tests |
| Accessibility / UX | the game UI does not regress keyboard or contrast | medium | high | a11y budget and keyboard spec |
| External stakeholder | reads the public programme pack, not the walk | low | medium | `/design/` remains the public surface (D48) |

### A4 — Drivers, goals, objectives (catalog)

| ID | Statement |
|---|---|
| DR1 | The only spatial interface is a local mock with no auth, registry or live rows |
| DR2 | The owner is considering the walk as the primary UI; a decision needs evidence |
| G1 | Every operator task is reachable in the walk through the existing contract |
| G2 | The contract's UI-independence is proven by a second client, not asserted |
| O1 | The P6 gate passes: 52 routes, 598 actions reachable, named flows round-trip, a11y/perf met |
| O2 | A contract change that breaks one client is caught by the parity instrument or a spec |
| O3 | The walk can be retired or promoted without a backend change |

### A5 — Solution concept (diagram)

```
browser (game.nonagentic.ai)
  shell ── seats (view presets) ── HUD (principal, tenant, queues, palette)
    └── router registry ── route row: walk path + app path + room + spec
          └── destination module ── read model ── GET /api/... | read action (e.g. programmes.list)
                                 └── sheet host ── action sheet ── actions.describe
                                                          └── POST /api/actions/{name}/invocations
                                                               (envelope, If-Match, Idempotency-Key, approval)
          └── world (WorldView port) ── canvas-2D (ADR 0023) · WebGL2 candidate
                                 └── rooms/figures/state; never a work surface
  events: GET /api/events?topics= (SSE) ── subscribed destination resync
```

### A6 — Statement of Architecture Work (draft)

Assignment: build the Gemba Walk to the P6 parity gate. Scope: the seven wings, the seven
destinations families, the retained V1 surfaces, the host dispatch and boundary, its own public
client, the instrument and the live sign-in proof.
Responsibilities: the programme architect owns the set; the Architecture Board reviews; the SRO
accepts the benefit and tranche evidence; the delivery sessions own their files. Acceptance: the P6
gate (52 routes, 598 actions, named flows, a11y/perf, owner sign-off). The contract is a document
now; `architecture.contracts_approve` makes it live. The full text is
`06-statement-of-architecture-work.md`.

### High-Level Design (Phases B–D)

### B1 — Business interaction matrix

| Wing | Destinations | Primary seats | Write flows | Gate surface |
|---|---|---|---|---|
| Portfolio & plan | portfolio, programme, plan/board | Product owner, Programme | plan/programme/card moves, decisions | approval, `If-Match` |
| Organisation | organisation, jobs, agents, tools, project types | Owner, Programme | create/update, bindings | manifest, scopes |
| Procedures & authoring | procedures, processes, map, authoring | Programme, Product owner | author/publish, process map | publish gates |
| Govern & design | decisions, approvals, ADM, architecture, requirements, models, copilot | Owner, Architecture Board | approvals, contracts, ADM, models | dual control, Board |
| Knowledge & memory | knowledge, search | all seats | remember/recall | tenancy |
| Measure & improve | improve, evals, benchmarks, usage, prompt audit, metering | Programme, Reviewer | evaluate, benchmark, meter | read-mostly |
| Operations & utilities | operations, platform, settings, tools | Owner, platform | settings, jobs, search | destructive confirm |
| Retained V1 | today, boards, cards, runs, approvals, requirements, models, processes, usage, settings, copilot | all seats | per the app route | same as `apps/web` |

### B2 — Actor / role matrix

| Seat preset | Landing | Queue emphasis | May change authority? | May write? |
|---|---|---|---|---|
| Delivery (product owner) | plan/board | cards, runs, approvals | no | only where the manifest and scopes allow |
| Programme (project office) | programme | tranche, benefits, ADM, registers | no | only where the manifest and scopes allow |
| Owner (tenant owner) | approvals/portfolio | approvals, members, settings | no — the server owns authority | yes, per scopes; destructive actions need approval |
| Reviewer (observer) | portfolio/read views | evidence and decisions | no | no; write affordances are absent |

### C1 — Data entity catalog

| Entity | Key content | Relationships |
|---|---|---|
| principal / membership | `GET /api/me`, tenant memberships, job keys | 1→* seats (presets) |
| tenant | `X-Active-Tenant`, config | 1→* read models |
| action manifest | name, effect, scopes, schema, confirm, tenancy, spec hash | 1→* action sheet rows |
| board / card / run | read models of the plan and execution | 1→* destinations |
| programme / tranche / benefit | MSP read models | 1→* programme wing |
| approval / decision | approval lifecycle, decision rows | 1→* govern wing |
| document / output | programme outputs and design pack | 1→* knowledge/design |
| architecture contract / artefact | ADM rows | 1→* design wing |
| event | SSE topic, `Last-Event-ID` | *→1 subscription |

No entity is new. The client reads them; writes go through actions. A client-side cache is a view
cache, never a source of truth.

### C2 — Application / function matrix

| Module | Responsibility | Contract |
|---|---|---|
| `src/auth` | PKCE, token memory, dev cookie, tenant header | `apps/web/src/auth.rs` contract |
| `src/api` | manifest, `actions.describe`, invocation envelope, RFC 9457 problems | `apps/web/src/api.rs` |
| `src/sse` | fetch stream, `Last-Event-ID`, resync | `apps/web/src/sse.rs` |
| `src/forms` | schema → form; JSON fallback | `actions.describe` input schema |
| `src/router/registry` | route rows + composition | `apps/web/src/app.rs` census |
| `src/shell` | seats, HUD, palette, map, sheet host | spec §2, §3 |
| `src/world` | `WorldView` port; canvas baseline; Bevy candidate | spec §6.2–§6.3 |
| `src/state` | view state; no programme store | spec §6.4 |
| `src/a11y` | focus, keyboard, contrast | spec §8 |
| `src/destinations` | one module per destination | spec §4, §6.1 |

### C3 — Conceptual data diagram

```
/api/me → principal ── memberships → tenant ── config
                            │
                    seat preset (view only)
                            │
manifest + actions.describe → action sheet → invocation → server refusal/result
                            │
boards/cards/runs · programme/tranche/benefit · approvals/decisions · documents · architecture
                            │
                      destination read model ── SSE topic → resync
```

### D1 — Technology view

Static HTML/JS/CSS sub-site served from the existing image and origin; no new port. The baseline
world simulation is `bevy_ecs 0.20.1` in `crates/walk-world`, compiled to wasm32 and loaded behind
`WorldView` (ADR 0022); the renderer is the canvas-2D projection (ADR 0023), measured against
WebGL2 in P0 (bundle, startup, frame time, WebGL requirements, CI build and DOM-overlay input);
WebGL2 remains a candidate behind the port. Browser APIs: fetch, streams, Web Crypto (PKCE), `sessionStorage` (verifier only),
`history`/URL. Build: GitHub-hosted preflight for the wasm compile (no local cargo); the walk's
static assets are copied by the Dockerfile. Tests: Playwright against the dev server; `node --test`
for pure modules. No SSR, no server runtime, no third-party hosted service.

### F1 — Migration and delivery diagram

```
P0 governance + census/instrument + spike + ADR 0022
 → P1 foundation + Portfolio read/write + fallback
 → P2 read parity → P3 write parity → P4 retained V1 → P5 slice-dependent surfaces → P6 parity gate
```

Build order: W0 census/instrument → W1 spike → W2 foundation → W3 reads → W4 writes → W5 V1 →
W6 slice-gate audit → W7 P6/primacy → W8 hardening. The split by file ownership is G0 (W0–W2),
S1–S3 (W3–W4 by wing), S4 (W5–W8). The P0 spike's throwaway workflow is deleted with the spike.

## 5. Artefact links

| From | Kind | To | Evidence |
|---|---|---|---|
| A2 requirements | derivation | A5 solution concept | each requirement maps to a module or contract |
| A1 principles | derivation | A5 solution concept | P1–P10 shape the client |
| A3 stakeholders | dependency | B1 interaction matrix | each wing names its seats |
| B1 interaction matrix | realization | C2 modules | each wing uses the shared seams |
| C1 data entities | realization | D1 technology view | reads/actions over the existing contract |
| A4 goals | derivation | A2 requirements | R1–R16 serve G1–G2 |
| F1 migration | dependency | W0–W8 | the build order |
| `apps/web` route table | reuse | census | the frozen baseline |
| `crates/api/src/web.rs` | reuse | D1 serving | the static sub-site model |
| ADR 0013 / D64 | superseded in part | A1 | ADR 0021 authorises this client only |

## Decisions

| ID | Decision | Rationale | Where it shows |
|---|---|---|---|
| AD1 | A static client at `https://game.nonagentic.ai/`, at the root, same image and service | the owner's product URL and a first-class client with no new runtime; the operator origin 308s `/walk/*` | D1, R17, ADR 0024 |
| AD2 | Route parity by construction (the walk path is the app path on the game host) | a missing route is visible, not invented; parity is an equality | router registry, census |
| AD3 | Full functionality through the existing contract | a second backend would fork the record | C1/C2, R3/R4 |
| AD4 | DOM sheets own all work; the world is navigation/state display | a canvas cannot carry audit-quality forms | P3, A5 |
| AD5 | Seats are view presets; the manifest and scopes own authority | a preset must never grant a permission | B2, R9 |
| AD6 | No fixtures, no client programme store | a fixture makes the parity claim false | P6, R12 |
| AD7 | `WorldView` port; `bevy_ecs` simulation (ADR 0022); canvas-2D renderer (ADR 0023), WebGL2 measured behind the port | the simulation and the renderer are decided; the port keeps the renderer replaceable | R8, D1, ADR 0022/0023 |
| AD8 | The instrument is a frozen census + per-destination specs | live manifests move; the gate must stay passable | plan §6, R13 |
| AD9 | Deep links served by the game-host dispatch + Rust matrix; the operator `/walk/*` 308s | without it a deep link renders the operator app or an operator surface leaks on the game host | R7, R17, W2 |
| AD10 | The game has its own public client `conveyor-game` in the `conveyor` realm; the API accepts the two-client `azp` allowlist | isolation without duplicating users; the replay guard stays first-party | R15, R18, D1 |

### D1 — Deployment and identity view (v2, 2026-11-27)

| Element | Design | Source |
|---|---|---|
| Edge | Cloudflare → `cloudflared-apps` tunnel → istio `n8n-gateway` → `conveyor-public-vs`; `game.nonagentic.ai` is a second host on the same VirtualService; TLS uses the wildcard edge and the origin's `noTLSVerify`; DNS is derived by the `dns-ensure` CronJob | gitops `clusters/strato/**` |
| Service | one `conveyor` Deployment and Service on 8930; no second image, port or runtime | `crates/api`, gitops `services/conveyor` |
| Host boundary | normalise `Host`; on a game host serve only the walk tree (`/srv/site/walk` at the root), `/config.json` and the API; a router-level gate refuses `/mcp` and `/design` before their routes; the operator index is never served; the operator host keeps its behaviour and 308s `/walk/*` to `CONVEYOR_GAME_PUBLIC_URL` | `crates/api/src/{lib,web}.rs`, ADR 0024 |
| Interface: browser ↔ API | same origin on both hosts; bearer access token, `X-Active-Tenant`, `If-Match`, `Idempotency-Key`; no CORS | spec §5, §10 |
| Interface: browser ↔ Keycloak | public client `conveyor-game`, standard flow + PKCE S256, redirect `https://game.nonagentic.ai/*`, web origin `https://game.nonagentic.ai`, audience mapper `conveyor`; the API accepts `azp ∈ {conveyor-web, conveyor-game}` | realm patch, ADR 0024 |
| Configuration | `/config.json` per host: game client id on the game host, operator client id elsewhere; same issuer and `dev_auth` | `crates/api/src/web.rs` |
| Rollback | remove the host from the VirtualService/tunnel, drop the two env values; the client is inert; no data migrated | CR-WALK-001 §8 |

## Acceptance

The delivery design gate: GEMBA-WALK operates under the P0 brief and the T9 capability. This set is
the PSA/HLD evidence; a deviation (a second backend, a fixture, a canvas-only work surface, a seat
that grants authority) is a change request, never a silent fork. The compliance review records one
level against the delivery contract when the project closes. The full TOGAF repository rows are
T9's; this set is publishable now and importable later.

The acceptance itself is the P6 gate in `docs/plans/GEMBA-WALK.md` §7 and §11, the instrument's
record `docs/evidence/walk-coverage.json`, and the owner's primacy decision.

## 7. Review and links

- Review date: 2026-12-03 (Architecture Board, after the P0 spike)
- Brief: `00-project-brief.md`
- Analysis: `01-inception-analysis.md`
- Register: `03-artifact-register.md`
- Plan: `docs/plans/GEMBA-WALK.md`
- Spec: `docs/design/GEMBA-WALK-SPEC.md`
- ADR: `docs/decisions/0021-gemba-walk-client.md`
