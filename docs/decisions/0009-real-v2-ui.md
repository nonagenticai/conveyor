# 0009 — The real V2 UI is the app, not the design pack

Status: Proposed · Date: 2026-11-22 · Issue: #559 · Spec: SPEC_V2 §10.5, D64, D65

## Context

The only V2 UI today is the static design pack (`docs/design/studio.html` + `v2.js`): 26 pages, no
live rows, no write path. The real application (`apps/web`, Leptos CSR/WASM in the same binary on
port 8930) has 31 routes and 25 screens, all V1; it has no `/portfolio`, `/organisation`,
`/procedures`, `/authoring`, `/plan`, `/improve`, `/operations`, `/tools` or `/design/adm`.

The programme's V2 coverage instrument (`scripts/build-v2-coverage.py`) credits the mock pages as
`exists` — `docs/evidence/v2-coverage.json` says `destination:Organisation` and
`destination:Procedures` are implemented because `v2.js` registers the page ids. The record therefore
overstates the real UI, and a reader cannot tell a reviewed design from a shipped screen.

The owner's ask is the real V2 UI: the application, not the mock. This ADR records how it is built and
how the programme measures it.

## Decision

1. **The app is the UI.** Every V2 destination is a route in `apps/web`, one Leptos Router, one
   binary, port 8930. The design pack remains the public programme site at `/design/` and the
   reviewed design reference; it is never the operator surface.
2. **No fixtures in production.** A production screen reads live rows and writes only through a
   registered action. An unwired destination renders **not yet live**, names the missing action or
   slice, and keeps its route. A mock page is evidence of intent, never of implementation.
3. **No new backend surface for the UI.** The destination contracts reuse the action families in
   SPEC_V2 §10.4. A missing capability is `acceptance-blocked-by` its owning slice; the UI does not
   add a table, an action, an event or an ad-hoc fetch.
4. **Measure the real routes.** `scripts/build-v2-coverage.py` resolves each §10.5 destination
   against `apps/web/src/app.rs` and its named Playwright spec. `exists` requires the V2 route **and**
   the spec; a V1 ancestor route may justify `extend`, never `exists`; the mock page is cited as
   design reference only.
5. **One spec per destination.** Each screen ships with a Playwright spec that asserts empty, error,
   happy and one refusal; `web.yml` on `apps/web/**` is the browser gate. The five delivery units are
   sequenced on the `app.rs`/`mod.rs` single-writer seam.

## Alternatives considered

- **Promote `console.js` in the design pack to the production UI.** Rejected: it has no live auth, no
  action writes and no real states; it would be a second implementation and an operator could not
  reach it.
- **SSR or a second frontend.** Rejected: ADR 0001 chose CSR on measurement, no measured need has
  appeared, and a second application doubles the auth flow, the shell and the test harness.
- **Embed the pack (iframe/WebView) inside the app.** Rejected: it still cannot write through the
  registry, it hides the mock behind a chrome, and it makes the coverage record lie twice.
- **Keep measuring the mock as the UI.** Rejected: that is the present defect; a page is not an
  implementation.

## Consequences

- Positive: one UI, one auth flow, one test harness; the programme's record matches what an operator
  can actually do; the pack keeps its public role.
- Cost: the five units serialise on `app.rs`; every destination needs its spec; a destination whose
  slice is unfinished shows an honest wait rather than a screen.
- Neutral: no schema, action or event change; the V1 routes are untouched and linked.

## Links

SPEC_V2 §10.5, §12 T18, D64, D65 · `docs/plans/T18-real-v2-ui.md` · issue #559.
