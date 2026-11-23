# Inception analysis — T18, the real V2 UI

## Current state

The real application is `apps/web` (Leptos CSR/WASM, one binary, port 8930). At `main@490ad555` it
has 31 routes (`apps/web/src/app.rs:139-169`), 25 screen modules (`apps/web/src/screens/mod.rs`) and
31 Playwright specs (`apps/web/e2e/`), all V1. It authenticates with OIDC PKCE and holds the token
in memory (`src/auth.rs`); reads are literal `GET`s and writes are
`POST /api/actions/{name}/invocations` (`src/api.rs`). There is no V2 route, no `/portfolio`, no
`/organisation`, no `/procedures`, no `/authoring`, no `/plan`, no `/improve`, no `/operations`, no
`/tools`, no `/design/adm`.

The V2 destinations exist only in the static design pack (`docs/design/studio.html`, `v2.js`): 26
page ids in seven nav groups, plus the L1–L4 delivery view added on 2026-11-21 by #528. The pack has
no live rows and no write path; its `LIVE` ribbon is a snapshot of the published programme.

The programme's V2 coverage record (`docs/evidence/v2-coverage.json`) has 60 rows: 46 `exists`, 6
`extend`, 8 `new`. Its destination rows credit the mock page: `destination:Organisation` and
`destination:Procedures` are `exists` because `docs/design/v2.js` registers those pages, not because
the app has a route. That is the measurement defect this project corrects (SPEC_V2 D65).

The action registry (SPEC_V2 §10.4) already carries most destination families, or the owning slice
owns them: `jobs.*`, `procedures.*`, `authoring.*`, `programmes.*`, `benefits.*`, `tranches.*`,
`wbs.*`, `baselines.*`, `registers.*`, `architecture.*`, `coach.*`, `kata.*`, `lessons.*`, `ops.*`,
`tools.*`, `toolkits.*`, `tool_bindings.*`, `tool_calls.*`, `metrics.*`.

## Reuse sources

- **The shell and client**: `Router`, `Sidebar`, `TopBar`, `Toast`, `Palette` in `app.rs`; `api.rs`
  request shapes (`If-Match`, `Idempotency-Key`, approval id); `state.rs` tenant/me/manifest signals;
  `sse.rs`; `auth.rs`; `components.rs`.
- **The test harness**: `apps/web/e2e/fixture.ts` (OIDC bearer injection or the dev cookie),
  `playwright.config.ts` (dev server on 8931, API on 8930), and `web.yml` on GitHub-hosted runners.
- **The existing screens**: boards, cockpit, processes, process map, evals, usage, prompt audit,
  platform and settings — linked from the V2 destinations rather than re-implemented.
- **The design reference**: `docs/design/v2.js` page inventory, nav groups, interaction patterns and
  the delivery view; `STUDIO-SPEC.md` for the review paths.
- **The read models**: the published `programme`/`work`/`measure` sections, exposed through the
  owning slices' read actions.

## Build vs reuse

**Reuse** the application, the action registry, the auth flow and the e2e harness. **Do not** embed
the design pack, copy its fixture arrays, add server functions, or build a second frontend. **Build**
only the routes, screen modules, navigation, honest states and specs. A backend capability that is
missing is a typed dependency on its owning slice; the UI never reaches around the action registry
with an ad-hoc fetch.

## Risks

- **Action gaps**: a destination may have no read or write action yet. The screen must say **not yet
  live** and name the missing action/slice; it must not render an empty success.
- **Single-writer router**: `apps/web/src/app.rs` and `screens/mod.rs` are touched by every unit, so
  the five units are sequenced with `status:blocked` on their predecessor.
- **Auth and tenancy**: tenant switching, a signed-out deep link and a forbidden route are distinct
  states; each needs a spec.
- **Bundle and render cost**: the V2 screens add WASM and table weight; the web workflow's budget and
  virtualisation threshold are the guard.
- **Mock drift**: the pack and the app will diverge; the coverage instrument measures the app, so the
  divergence is recorded rather than hidden.

## Open questions

- Which read action exposes the L1–L4 delivery sections to a signed-in tenant? Default: the owning
  slice's read action; until it exists the view reads the existing programme/project reads and names
  the partial section.
- The metering drill-down's read path: default `metrics.*` when T11 exposes it.

## Links

Issue #559 · SPEC_V2 §10.5, §12 T18, D64, D65 · `docs/plans/T18-real-v2-ui.md` ·
`docs/decisions/0013-real-v2-ui.md`.
