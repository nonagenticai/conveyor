---
document: GEMBA-WALK/inception-analysis
version: 1
status: draft
project: GEMBA-WALK
category: inception-analysis
owner: business-analyst
reviewer: programme-architect
review_date: 2026-12-03
---

# GEMBA-WALK — Inception analysis

## Current state

Measured at `main@cedd84980` on 2026-11-26; the walk has no code in the tree.

| Area | State | Evidence |
|---|---|---|
| Operator UI | DONE (normative) | `apps/web/src/app.rs` Leptos `<Router>`: 52 route patterns, 43 screen modules |
| Browser specs | DONE (normative) | `apps/web/e2e/**`: 46 Playwright specs |
| Action registry | DONE | `docs/design/data/actions.json`: 597 actions; `actions.describe` carries input/output JSON Schema, effect, confirm, tenancy, spec hash |
| Auth | DONE | `apps/web/src/auth.rs`: auth-code + PKCE S256, token in memory, verifier in `sessionStorage`, dev cookie |
| Events | DONE | `apps/web/src/sse.rs`: fetch stream, bearer header, `Last-Event-ID`, `resync` |
| Static serving | PARTIAL | `crates/api/src/web.rs`: a directory with `index.html` is a sub-site; `/walk/<deep-path>` falls through to the operator `index.html` — no scoped fallback |
| `/walk` client | ABSENT | `grep -rni 'gemba\|/walk' apps crates` → no hit; no `apps/walk` |
| Programme row | ABSENT | `seed/programme.json` has 50 projects, none GEMBA-WALK |
| Inception set | ABSENT | `docs/plans/inception/GEMBA-WALK/` did not exist before this unit |
| The mock | LOCAL ONLY | `docs/Conveyor-Gemba-Walk.html` (6.7 MB, untracked): canvas-2D world, seats/committee/studio, a 25 Nov snapshot in `localStorage`; no auth, no registry, no server |

## Reuse sources

| Source | Path:symbol | Reuse decision |
|---|---|---|
| Identity | `apps/web/src/auth.rs` (PKCE, token in memory, dev cookie) | **Reuse the flow**, not the code: the walk implements the same auth-code + PKCE contract; the redirect URI is the only external change |
| Action client | `apps/web/src/api.rs` (`actions.describe`, invocation envelope) | Reuse the contract shape; the walk's client is its own module |
| Events | `apps/web/src/sse.rs` (fetch stream, `Last-Event-ID`) | Reuse the protocol; one subscription per screen |
| Route table | `apps/web/src/app.rs` (`<Router>`) | Reuse as the **census source** — the walk's routes are `/walk` + the app path by construction |
| Static serving | `crates/api/src/web.rs` (sub-site with `index.html`) | Reuse; **extend** with a scoped `/walk/*` fallback + Rust test |
| Design pack | `docs/design/README.md` (static sub-site, auth bootstrap, console.js) | Reuse the serving/auth pattern; the walk is not a pack page |
| Action specs | `crates/actions/src/**` `#[action(...)]` | Reuse as the schema/effect/scope source; no new action |
| Specs | `apps/web/e2e/**` | Reuse the assertions' content; the walk has its own suite |

## Build vs reuse

**Build:** the static sub-site (`apps/walk/index.html`, `src/main.js`, auth, api, forms, router
registry, shell, world port, state, a11y); the DOM sheet system; the schema-driven action sheet;
the seats presets; the canvas world and its `WorldView` implementation; the parity instrument and
the frozen census; the Playwright suite; the serving fallback and Dockerfile copy; the walk CI
workflow.

**Reuse, never rebuild:** the action registry, `actions.describe`, the invocation envelope and
headers, the identity provider, the read models, the server's refusals and the design pack. The walk
is a client of the existing contract; a capability the contract does not expose is an honest gate.

**Reject:** an iframe or WebView of the mock; a second backend; a second decision store; a
`localStorage` programme state; any fixture or embedded snapshot; a new table, action, event or port.

**The one architectural decision:** the client is a static sub-site served from the same image and
origin, with all work in DOM sheets and the world behind a `WorldView` port. That is what makes the
*renderer* replaceable (canvas-2D first, WebGL2 measured) while the simulation — `bevy_ecs` in
`crates/walk-world` (ADR 0022) — stays a pure Rust crate with no DOM coupling.

## Constraints and licences

- Same image and origin, no new port; the Dockerfile copies the static output and `web.rs` serves it.
- OIDC: the public client gains a `/walk/` redirect URI (or a dedicated public client); this is an
  external identity-operator change, recorded as `acceptance-blocked-by`.
- Renderer candidates: canvas-2D (no dependency); WebGL2 if the spike shows the canvas budget
  fails. The simulation is `bevy_ecs 0.20.1` (Apache-2.0 OR MIT) in its own crate. Any embedded font must carry its licence; the mock's embedded font is not
  carried without its licence.
- No private host, path, token or credential in any page; `no-private-refs` is a gate.
- The world crate is the only new Rust dependency; its wasm build runs on GitHub preflight.

## Risks

| Risk | Evidence it is real | Mitigation |
|---|---|---|
| Scope: 597 actions, 52 routes | measured census | schema-driven action sheet; frozen census; per-destination specs; P6 is the only done |
| Contract drift between clients | two clients, one registry | one contract; behavioural trace parity on named flows; a contract test per write |
| Auth duplication | a second PKCE flow | same flow, shared config, one identity provider; the dev cookie for local runs |
| Renderer misses budgets | WebGL2 startup and frame cost unknown | measure in P0; the canvas baseline stays; ADR 0023 decides |
| Deep links render the operator app | `web.rs` fallback absent | scoped fallback + Rust test in P1, before any link is published |
| No local Rust builds | owner rule | wasm compile on GitHub preflight; the baseline needs no Rust |
| Mock leakage | the mock embeds a snapshot and fixtures | Do-Not list; a test asserts no snapshot URL is fetched |
| Accessibility regression | a game UI invites canvas-only controls | DOM-only work; keyboard path; canvas duplicated by the map |

## Open questions

| # | Question | Default |
|---|---|---|
| 1 | canvas-2D or WebGL2 for the renderer? | undecided until the P0 spike; the simulation is `bevy_ecs` either way (ADR 0022) |
| 2 | shared public client with a `/walk/` redirect, or a dedicated public client? | shared unless the identity operator prefers isolation; the spike proves the dev path |
| 3 | how are the seat presets bound to real job keys? | by the job-key mapping the `GET /api/me` membership already carries; a preset never changes authority |
| 4 | does the walk ship in the same image as the app? | yes; same image and origin, no new port |

## Links

- Brief: `00-project-brief.md`
- Architecture: `02-architecture-design.md`
- Plan: `docs/plans/GEMBA-WALK.md`
- Spec: `docs/design/GEMBA-WALK-SPEC.md` §1–§3, §10
- ADR: `docs/decisions/0021-gemba-walk-client.md`
