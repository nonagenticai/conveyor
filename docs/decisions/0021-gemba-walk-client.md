# 0021 — The Gemba Walk is a separate client, a candidate primary UI

Status: Proposed · Date: 2026-11-26 · Owner direction: 2026-11-26 · Spec: SPEC_V2 §10.5.11, §15.1 D67
Supersedes in part: ADR 0013 decision 1 and its rejected "second frontend" alternative; D64

## Context

The only spatial interface today is a local mock: a single self-contained HTML file whose world is a
hand-written canvas projection, whose programme state is a seeded fixture in `localStorage`, and
whose "reports" and "decisions" are generated in the browser. It has no authentication, no action
registry and no server. It is evidence of an interaction model, not an implementation.

The owner has directed that this game-style interface **may become the primary UI**, and therefore
must be specified with all features and all functionality — every read and every write. That
direction is incompatible with two current rulings if read literally:

- ADR 0013, decision 1: *"The app is the UI. Every V2 destination is a route in `apps/web`, one
  Leptos Router, one binary, port 8930."* Its alternatives list rejects *"SSR or a second
  frontend… a second application doubles the auth flow, the shell and the test harness."*
- D64: *"Every V2 destination is a route in `apps/web` … not a page in the design pack."*

The existing rulings exist to prevent a second **operator console** built beside the app, and a mock
mistaken for an implementation. They do not settle whether a second **client** may exist when the
owner is considering replacing the first. This ADR settles that question and leaves the parity
decision open until it can be measured.

## Decision

1. **A separate client is authorised at `/walk/`.** It is a static sub-site served from the same
   image and origin as the API (the design pack's serving model), with its own assets, router,
   screens and Playwright suite. It adds no port.
2. **It is a candidate, not the primary.** `apps/web` remains the normative operator UI. The walk
   becomes primary only through the measured gate in decision 7, and that promotion is a runbook
   step, not a consequence of this ADR.
3. **It has full functionality through the existing contract.** Every read uses the literal `GET`
   routes or a read action; every write is
   `POST /api/actions/{name}/invocations` with the standard envelope and headers. It adds no table,
   action, event or decision store, and it renders no fixture. A capability the server does not
   expose is an honest gate, exactly as in `apps/web`.
4. **It uses the same identity.** auth-code + PKCE S256, access token in memory, PKCE verifier in
   `sessionStorage`, the dev cookie for local runs, and the same tenant header. The public OIDC
   client gains a `/walk/` redirect URI (or a dedicated public client if the identity operator
   prefers isolation); no token is ever stored in `localStorage`.
5. **The world is a renderer behind a port.** Records, tables, forms and decisions are DOM sheets in
   every case. The world implements a small `WorldView` interface; the engine is not assumed. The
   baseline is the mock's canvas-2D projection; the owner's candidate is **Bevy 0.18 compiled to
   WASM**. The P0 spike measures both and records the choice in ADR 0022.
6. **The spatial model never gates a capability.** Every room, record, form and decision has a
   deep link and a keyboard path. A user who never walks the world can use every feature.
7. **Primacy requires a measured parity gate.** The walk reaches parity when every §10.5 destination
   and every retained V1 route has a walk route and a Playwright spec asserting empty, error, happy
   and one refusal or write; every action in the frozen census is reachable and schema-rendered; the
   named write flows round-trip; the accessibility and performance budgets are met; and the owner
   signs the result. The instrument is `scripts/build-walk-coverage.py` and its record is
   `docs/evidence/walk-coverage.json` — never a prose claim.
8. **Primacy is reversible until `apps/web` is retired.** The surface is additive: removing the
   navigation link and the old-route redirect map restores the app with no data loss. Retirement of
   `apps/web` is a separate, later decision taken after a stabilisation period.
9. **The governing spec records the exception.** SPEC_V2 gains §10.5.11 (the client's boundary) and
   D67 (the owner ruling, the gate and the reversal path). The V2 coverage record (D65) is unchanged
   and continues to measure `apps/web`.

## Why

- The owner's direction is explicit, and the interaction model is genuinely different: a spatial,
  role-oriented walk is not a reskin of a sidebar console.
- A separate surface is the only way to evaluate a game engine without putting a frame loop and a
  WASM engine into the operator bundle, and the only way to retire either client without rewriting
  the other.
- The backend already supports it: 597 actions are registered, `actions.describe` carries the input
  and output JSON Schema, and every write already has an envelope, headers and a typed refusal. A
  second client therefore needs no backend fork; it needs a contract-faithful one.
- The risk this ADR accepts is duplication of shell, auth and tests. It is contained by one action
  registry, one decision store, one set of read models, a shared census, and a parity instrument
  that makes an omitted destination visible.

## Alternatives considered

- **A game mode inside `apps/web`.** Rejected: it couples a frame loop and (if Bevy) a WASM engine to
  the operator bundle, forces one release cadence on two very different surfaces, and makes the
  engine spike unmeasurable in isolation. It also cannot become a clean replacement path.
- **A reskin of `apps/web`.** Rejected: it delivers neither the spatial navigation nor the
  role/room model the owner asked for, at the cost of a full restyle of 43 screens.
- **An iframe or WebView of the mock.** Rejected: the mock has no authentication, no registry and no
  live rows; ADR 0013 already rejected embedding the design pack as a substitute for an
  implementation. It would make the coverage record lie.
- **A public read-only lens.** Rejected by the owner on 2026-11-26: the walk must have all
  functionality, so it must be able to write through the registry.
- **A second backend for the walk.** Rejected by §3 and P4, and by ADR 0013: one action registry,
  one decision store, no second source of truth.

## Consequences

- Positive: the game UI can be built and measured without weakening the app; either client can be
  retired without a backend change; the owner gets a real basis for the primary-UI decision.
- Cost: a second shell, auth flow and browser suite; two clients must track each destination's
  contract. The parity instrument and per-destination specs are the mitigation, and a change that
  updates one client without the other is visible in CI.
- Delivery: `docs/plans/03-DELIVERY.md` defaults to one plan = one issue. The walk is
  multi-session by shape, so the plan proposes a five-issue split — a dependency-bearing parent
  and four file-disjoint wings — which the owner must approve as the written exception at W0.
  Without that approval the whole plan is one issue. `lane/` branches are not used; the parent
  issue is claimed before any branch.
- Serving: a scoped `/walk/*` fallback in `crates/api/src/web.rs` (GET/HEAD only, non-asset paths)
  and a Dockerfile copy of the static output; both are serving changes, not API surface. Without the
  fallback, a walk deep link currently renders the operator app's `index.html`.
- Identity: an external redirect-URI registration is required before production sign-in works.
- Performance: if Bevy is selected, its wasm bundle and startup are the dominant risk; the P0 spike
  measures them and the canvas baseline remains a working fallback behind the same port.
- The design pack keeps its role: `/design/` is unchanged, and the walk does not ship inside it.

## Revisit triggers

- The parity gate cannot be met without adding backend surface (it must not be added).
- Bevy on the web misses the agreed budgets and the canvas baseline cannot close the gap.
- Maintaining two clients costs more than the primacy decision is worth.
- The owner declines primacy at P6; the walk remains an alternative client and this ADR stays
  proposed rather than accepted.

## Links

ADR 0001 (CSR by measurement) · ADR 0013 and D64 (the app is the UI) · SPEC_V2 §10.5, §10.5.11,
§15.1 D66/D67 · `docs/design/GEMBA-WALK-SPEC.md` · `docs/plans/GEMBA-WALK.md` · the parent issue
is created by the plan's §8.1 runbook after the owner accepts this ADR.
