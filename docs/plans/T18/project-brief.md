# Project brief — T18, the real V2 UI

## Outcome

A signed-in tenant runs every V2 destination from the conveyor's own application, and the
programme measures those screens from the app's real routes and specs instead of the design mock.

## Boundary

In scope: the application shell and navigation, the ten V2 destinations in SPEC_V2 §10.5, the
L1–L4 delivery view, one Playwright spec per destination, and the measurement of the real routes.
Out of scope: new tables, actions, events or ports; SSR, a mobile app or a second router; retiring
the design pack or rebuilding the public programme site; any live tenant write inside a pull
request. The V1 routes stay; the pack stays the public site at `/design/` and the reviewed design
reference (SPEC_V2 D64).

## Reuse sources

- `apps/web/src/app.rs` — the router, sidebar and top bar; `api.rs`, `auth.rs`, `state.rs`,
  `components.rs`, `sse.rs`, `palette.rs`.
- `apps/web/e2e/fixture.ts`, `apps/web/dev-server.mjs`, `apps/web/playwright.config.ts` and the
  existing 31 specs.
- The compiled action registry (SPEC_V2 §10.4) and the existing V1 screens: boards, cockpit,
  processes, process map, evals, usage, prompt audit, platform, settings.
- `docs/design/v2.js` and `docs/design/STUDIO-SPEC.md` as the reviewed design reference, never as
  production code.
- The delivery view's derived sections (`programme`, `work`, `measure`) published by the pack and
  exposed by the owning slices' read actions.

## Inception units

- `docs/plans/T18/inception-analysis.md` — what exists in the app, what is reused, and the risks.
- `docs/plans/T18/architecture-design.md` — the route/read/write contracts, the decisions and the
  architecture artefacts.

## Dependencies

- **blocked-by (construction)**: T0, T1, T2, T3, P0 — the model and contracts, the plan spine, the
  decision ladder, the PM job and the programme definition.
- **acceptance-blocked-by**: T4, T5, T7, T9, T10, T11, T13, T16, T17 — each destination's live
  scenarios wait on its owning slice; construction proceeds with the honest "not yet live" state.
- **acceptance-blocked-by**: T12 — the final organisation loop is T12's acceptance driven from the UI.

## Acceptance

- The SPEC_V2 §12 T18 gate: every destination route exists with its spec; the signed-in tenant
  drives the portfolio, programme, plan, organisation, procedures, tools, improve and operations
  paths; the coverage record reports `exists` only from a real route and its spec.
- Each destination's Playwright spec asserts empty, error, happy and one refusal; `web.yml` is the
  browser gate.

## Capabilities

The tenant operator who runs the portfolio, programme, plan, organisation, procedures, authoring,
tools, improve and operations surfaces; and the PM, kanban-coach, kata, development and operations
jobs those destinations serve. The UI is their surface, not a new job.

## Plan

`docs/plans/T18-real-v2-ui.md`; SPEC_V2 §10.5 and §12 T18. Kickoff issue #559; the five delivery
units it creates.

## Risks

- A destination's action family is absent: the screen says **not yet live** and the live scenario is
  `acceptance-blocked-by` that slice — no local fetch is invented.
- The five units serialise on `apps/web/src/app.rs`; the split is sequenced, not parallel.
- The mock and the app can drift: the coverage instrument reads the app, so the drift is visible.
- Bundle growth: route-level rendering and the `web.yml` budget are the guard.
