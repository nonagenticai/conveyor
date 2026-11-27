---
document: GEMBA-WALK/project-brief
version: 2
status: draft
project: GEMBA-WALK
category: project-brief
owner: business-analyst
reviewer: SRO
review_date: 2026-12-03
---

# GEMBA-WALK — Project brief

> **Amendment 2026-11-27 (CR-WALK-001, ADR 0024).** The client is served at
> `https://game.nonagentic.ai/` at the root, with its own public client `conveyor-game` in the
> `conveyor` realm; the operator origin's `/walk/*` 308s to it, and `/walk/` remains only the
> no-host dev/CI shape. The action count is 598 (572 attributes + 26 manual registrations). Read
> "`/walk/X`" in the analysis sections below as the app path `/X` on the game host.

## Outcome

A signed-in tenant member can complete every operator task in a separate, game-style client at
`https://game.nonagentic.ai/` — the same reads, writes, gates and refusals as `apps/web` — and the parity is measured by
an instrument, so the owner can decide whether the walk becomes the primary UI on evidence rather
than on a mock.

## Boundary

**In:** a static client at `https://game.nonagentic.ai/` (same image and service, a second host;
the operator origin's `/walk/*` redirects to it); route parity with the 52 `apps/web` route patterns
at the host root; reachability and schema-rendering of the 598 registered actions; the
seven wings and their rooms; seats as view presets; DOM sheets as the only work surface; the
`WorldView` port and the canvas-2D baseline; the parity instrument and its frozen census; Playwright
specs per destination; the host dispatch, the `/walk` 308 and the game-host boundary; its own
public Keycloak client `conveyor-game` and the live sign-in proof; accessibility and performance
budgets; deployment; the P6 primacy runbook.

**Out:** no table, action, event, port or decision store; no second backend or read model; no SSR;
no native mobile, chat, audio or video; no calendar/AV dependency; no fixture or embedded snapshot;
the mock is not committed and never the client's source; the design pack at `/design/` is unchanged;
`apps/web` is not retired before the P6 gate; primacy is not claimed from prose.

**It must not fork:** the action registry, the decision store, the read models, the auth flow's
identity provider, or the Playwright contract. A capability the server does not expose is an honest
gate, exactly as in `apps/web`.

## Reuse sources

| Source | Path:symbol | Question the analysis answers |
|---|---|---|
| Auth flow | `apps/web/src/auth.rs` (PKCE S256, token in memory, dev cookie) | can the same public client mint a session for `/walk/`, or does it need its own redirect URI? |
| Action client | `apps/web/src/api.rs` (`actions.describe`, `POST /api/actions/{name}/invocations`) | is the client thin enough to share the envelope and header contract? |
| Events | `apps/web/src/sse.rs` (fetch stream, `Last-Event-ID`, resync) | can a second client subscribe without a new server surface? |
| Route table | `apps/web/src/app.rs` (`<Router>`) | what exactly is the frozen census, and how is it parsed? |
| Static serving | `crates/api/src/web.rs` (static sub-site, SPA fallback) | does a `/walk/*` fallback exist, and what does a deep link do today? |
| Action registry | `crates/actions/src/**`, `docs/design/data/actions.json` | are all 598 actions schema-described, and which are destructive or approval-gated? |
| Specs | `apps/web/e2e/**` | which behaviours must the walk's specs assert, and which are contract tests? |
| Design pack | `docs/design/README.md` (serving, auth, console) | what serving and auth pattern is already proven for a static sub-site? |

## Inception units

The analysis and architecture outputs are published with this brief:
`01-inception-analysis.md` and `02-architecture-design.md`, with the typed set in
`03-artifact-register.md`. Both are documents on the project and carry the standard's metadata.

## Dependencies

| Relation | Issue | Why |
|---|---|---|
| related | #792 | this inception unit (the procedural set and the live publication) |
| related | #120, #121, #122 | the P0 definition set and the per-project inception standard (closed) |
| related | #395 | one live architecture contract per delivery project; the SoAW is this project's Phase-A subject |
| related | #526 | the programme rework/publish lane whose transport this unit reuses |
| blocks | G0 (to create) | the build unit is not claimed until the three inception documents are published |
| blocks | the gitops-production change | the `conveyor-game` client, the tunnel ingress, the VirtualService host and the deployment env; delivered in the same unit (CR-WALK-001) |

## Acceptance

Each part carries a BDD scenario with its `Verify:` command; the plan's stage gates are the build's
acceptance.

- **Foundation (P1).** Given the merged seed and the dev server, when a signed-in member opens
  `https://game.nonagentic.ai/`, then the entrance renders the Portfolio reads and one real write round-trips.
  Verify: `npx playwright test -c apps/walk/playwright.config.ts --grep foundation` (to create).
- **Read parity (P2).** Given every §10.5 destination, when it loads, then loading, empty, partial,
  error and denied states render with the honest gate.
  Verify: `npx playwright test -c apps/walk/playwright.config.ts --grep reads` (to create).
- **Write parity (P3).** Given a write action, when it is submitted, then the envelope, `If-Match`,
  `Idempotency-Key` and approval are sent, and a refusal is shown verbatim with no optimistic
  mutation. Verify: the contract test per action family (to create).
- **Retained V1 (P4).** Given the frozen census, when the instrument runs, then every route has a
  walk route and a spec, and every action is reachable and schema-rendered.
  Verify: `python3 scripts/build-walk-coverage.py --check` (to create at P0).
- **P6 gate.** Given the instrument green, when the accessibility, performance and primacy checks
  run, then the owner signs the primacy decision — or the walk stays an alternative client.
  Verify: the P6 runbook (to create).

## Capabilities

The owner/sponsoring group (primacy decision), the programme manager (tranche and issue queue), the
programme architect (ADM and contract), the delivery seats (Product owner, Programme, Owner,
Reviewer presets), platform/security (serving, identity, headers), and accessibility/UX (keyboard
and contrast). The customer is not a seat: the external stakeholder reads the public pack (D48).

## Plan

`docs/plans/GEMBA-WALK.md` — stages P0–P6, work packages W0–W8, the approved five-issue build split
(G0 + S1–S4). Implementation issues are created only after this inception is published.

## Risks

| Risk | Posture |
|---|---|
| 598 actions and 52 routes is multi-session | the schema-driven action sheet gives early reachability; the frozen census and per-destination specs make gaps visible |
| A second shell/auth/test harness drifts from the app | one contract, one census, behavioural trace parity on named flows, a contract test per write |
| Bevy on the web misses the budgets | canvas-2D baseline first; the P0 spike measures both; ADR 0022 decides |
| No local Rust builds | wasm builds on GitHub preflight; the baseline needs no Rust |
| Deep links render the operator app | the scoped `web.rs` fallback and its Rust test land in P1, before any link is published |
| The game host leaks an operator surface | a router-level host gate, a host-aware static dispatch and `app()` matrix tests; `/mcp` and `/design` are refused on the game host |
| The mock's fixtures leak into the client | the Do-Not list; a test asserts no snapshot URL is fetched by a signed-in walk |
| Accessibility regresses under a game UI | DOM-only work surfaces; keyboard-only spec; canvas is decorative and duplicated by the map |

## Gaps

At this brief's date the live register has no GEMBA-WALK row (`seed/programme.json` has 50 projects,
none of them GEMBA-WALK); there is no `/walk` route, app or spec in the tree. The three inception
documents are authored here and published by this unit (#792); the build units G0 and S1–S4 do not
exist until then. The game host and its client are delivered by CR-WALK-001; there is no external identity gap.

## Links

- Plan: `docs/plans/GEMBA-WALK.md`
- Spec: `docs/design/GEMBA-WALK-SPEC.md`
- ADR: `docs/decisions/0021-gemba-walk-client.md`
- Analysis: `01-inception-analysis.md`
- Architecture: `02-architecture-design.md`
- Register: `03-artifact-register.md`
- MSP: `04-msp-profile.md`
- SAFe: `05-safe-epic.md`
- SoAW: `06-statement-of-architecture-work.md`
- Governance: `07-governance-and-evidence.md`
- Spec reference: `docs/SPEC_V2.md` §10.5, §10.5.11, D64/D66/D67
