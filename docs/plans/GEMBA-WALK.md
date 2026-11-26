---
project: GEMBA-WALK
title: The Gemba Walk — a game-style candidate primary UI
status: proposed
depends_on: [P0, T18, T19]
spec_sections: ["§10.5", "§10.5.11", "§15.1 D64/D66/D67"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: [walk]
parity_ids: []
---

# GEMBA-WALK — the Gemba Walk client

Owner direction: 2026-11-26 · ADR: `docs/decisions/0021-gemba-walk-client.md` · Spec:
`docs/design/GEMBA-WALK-SPEC.md` · Baseline measured at `main@d958824f8` on 2026-11-26.

This plan is deliberately **not** a SPEC_V2 §12 `T##` slice. The programme-seed generator parses
only `docs/plans/slices/S*.md` and `**(T##)**` rows in §12, so a T-row would silently change
`seed/programme.json` and fail `build-programme-seed.py --check`. The walk is registered as a
cross-cutting project in `docs/plans/00-INDEX.md`; programme registration and the issue set are W0,
taken only once the owner accepts ADR 0021.

## 1. Objective and gate

Build a second, game-style client of the conveyor at `/walk/` with **all features and all
functionality** — every read and every write through the existing registry — and measure it well
enough that the owner can decide whether it becomes the primary UI.

**Gate (P6):** every §10.5 destination and every retained V1 route has a walk route and a Playwright
spec asserting empty, error, happy and one refusal or write; every action in the frozen census is
reachable and schema-rendered; the named write flows round-trip with the correct headers; the parity
instrument is green with no expired exemption; accessibility and performance budgets are met; the
owner signs the primacy decision. Until then `apps/web` is normative.

## 2. Starting state (measured, not assumed)

| Part | State | Evidence |
|---|---|---|
| Operator UI | Leptos CSR/WASM; **52** route patterns, **43** screen modules, **46** Playwright specs | `apps/web/src/app.rs`, `apps/web/src/screens/`, `apps/web/e2e/` |
| Backend contract | **597** registered actions; manifest, `actions.describe` (input/output JSON Schema, effect, confirm, tenancy, spec_hash), invocation envelope, RFC 9457 problems, `next_cursor`, SSE | `docs/design/data/actions.json`, `apps/web/src/api.rs`, `crates/actions/src/views.rs` |
| Auth | auth-code + PKCE S256, token in memory, verifier in `sessionStorage`; dev cookie; `/config.json` | `apps/web/src/auth.rs`, `apps/web/public/config.json` |
| Serving | any directory with `index.html` is a static sub-site; design pack copied to `/out/site/design`; `/walk/<deep-path>` currently falls through to the operator `index.html` | `crates/api/src/web.rs`, `Dockerfile:86-93` |
| Walk client | **absent**: no `/walk`, no `apps/walk`, zero mentions of gemba or walk in the tree | `grep -rni 'gemba\|/walk'` at the baseline |
| The mock | a local single-file artefact only; no auth, no registry, programme state in `localStorage`, embeds a 4.7 MB gzip copy of the design pack | owner's local file; not a repository input |
| Governance | ADR 0013/D64 say the app is the UI and reject a second frontend; ADR 0021 supersedes them **in part** for this client | `docs/decisions/0013-real-v2-ui.md`, `docs/SPEC_V2.md` §10.5.10, D64 |

The baseline counts are frozen by W0 into `docs/evidence/walk-census.json`. The gate is compared to
that census, not to a live manifest, so a later slice adding actions cannot make it unpassable; a
re-baseline is a signed commit that states what changed.

## 3. Scope

**IN** — a static client at `/walk/`; route parity with every `apps/web` path; the seven wings and
the destination rooms; the full read and write contract; seats as view presets; the DOM sheet
system; the schema-driven action sheet; the world port and the engine decision; the parity
instrument and its evidence; per-destination Playwright specs; accessibility and performance
budgets; deployment and the scoped deep-link fallback; the primacy runbook.

**OUT** — any new table, action, event, port or backend read model; a second decision store; a
replacement for `/design/`; native mobile; chat/audio/video; customer care (D48); SSR; a public
unauthenticated mode in v1; pixel parity with the mock; deleting `apps/web` before the gate.

**"All functionality" is defined as**: every §10.5 destination and every retained V1 route has a
walk route and a spec; every manifest action is reachable and form-renderable; every destination's
primary write flow is exercised end to end; every gate the server enforces is surfaced, never
pre-empted. It is not defined as pixel or copy parity with either existing client.

## 4. Relationship to the programme

- Not a §12 `T##` slice and not in `seed/programme.json` in this unit.
- W0 registers a programme project and creates the unit issues once ADR 0021 is accepted; the
  registration is a separate, reviewable data change (`seed/programme.json` + the issue ledger).
- The V2 coverage instrument (D65, `scripts/build-v2-coverage.py`) continues to measure `apps/web`
  and is not touched. The walk's evidence is `docs/evidence/walk-coverage.json`.
- The design pack (`/design/`) keeps its public role and is not rebuilt or embedded.

## 5. Architecture decisions (normative; detail in the spec)

1. Static sub-site at `/walk/`, same image and origin, no new port.
2. Route parity by construction: walk path = `/walk` + app path; `/walk/` is the entrance.
3. Full functionality through the existing contract; no backend surface.
4. DOM sheets own all work; the world is navigation and state display only.
5. Seats are view presets; the manifest and server scopes own authority.
6. No fixtures; no client-side programme store; no published snapshot as the app's source.
7. `WorldView` port; canvas-2D baseline; Bevy 0.18 WASM candidate; ADR 0022 after the P0 spike.
8. Clean URLs with a scoped `/walk/*` fallback in `crates/api/src/web.rs` (GET/HEAD, non-asset
   paths) and a Rust test; without it a deep link renders the operator app.
9. Schema-driven action sheet so no action is unreachable; unsupported schema falls back to a
   labelled JSON editor.
10. Parity is measured by a frozen census and per-destination specs, never by prose.

## 6. The parity model and instrument

`scripts/build-walk-coverage.py` (W0) is the only measure of walk parity. It must be able to run in
CI with no server and no database.

**Inputs**

- the frozen census `docs/evidence/walk-census.json`: `baseline {date, git_sha}`, the 52 app route
  patterns, the 597 action names and their scopes (names from `docs/design/data/actions.json`, the
  compiled registry dump; scopes from the action source scan used by `build-v2-coverage.py`);
- `apps/walk/src/router/registry.js`: one row per surface (walk path, app path, room, spec file);
- `apps/walk/e2e/*.spec.ts`.

**Per-action states** (each machine-checkable):

| State | Check |
|---|---|
| `reachable` | the action name is in the census and the walk's action sheet can route to it |
| `schema-rendered` | `actions.describe`'s input schema produces a form or the documented JSON fallback |
| `write-roundtrip` | a named destination flow invokes the action in dev and refetches its read model |
| `approval-tested` | a destructive action without its approval is refused and the refusal is shown |
| `sse-updated` | a subscribed screen reflects the topic (where the topic exists) |

**Per-destination states** — `absent`, `read`, `write`, `verified`, with the spec path and the last
green run. `verified` requires all four states plus the Playwright spec.

**Behavioural parity** — for each named flow, the same scripted task runs against `apps/web` and
`/walk/` in dev; the harness captures the `POST /api/actions/{name}/invocations` trace (names,
status, order) and compares it. Pixels are not compared.

**Exemptions** — `{surface, reason, signer, expires}`; an expired exemption fails the gate. The
owner or the plan's named maintainer signs; the default expiry is 30 days.

**Output** — `docs/evidence/walk-coverage.json` with a `--check` mode in the walk workflow and in
`web.yml` (or its successor) once the walk app is on `main`.

## 7. Stages and exit gates

| Stage | Work | Exit gate |
|---|---|---|
| **P0** | Governance (ADR 0021 accepted; §10.5.11/D67 land with this unit), census + instrument, spike | The spike proves PKCE redirect to `/walk/`, SSE via fetch, schema forms for read/write/destructive with approval, all headers, a deep link, and the static copy; it measures canvas-2D vs Bevy 0.18 WASM and ADR 0022 records the winner and budgets; no unproven assumption enters P1 |
| **P1** | Foundation + Portfolio end-to-end | Sign-in (dev and configured OIDC), tenant switch, manifest-driven nav, world entrance, Room `portfolio` read + **one real write**, deep link and back/forward, dev server, Dockerfile copy, `web.rs` fallback + test, spec green |
| **P2** | Read parity across §10.5 | Every destination renders its reads with loading/empty/partial/error/denied and the honest gate; each has its spec |
| **P3** | Write parity + gates | Every destination's writes go through the action sheet with `If-Match`/`Idempotency-Key`/approval, refusals shown verbatim, no optimistic mutation |
| **P4** | Retained V1 surfaces | All 52 route patterns have a walk route; the V1 families have reads and their writes; specs |
| **P5** | Slice-dependent surfaces | Meetings (T19), tools (T13–T15), operations (T7), ADM (T9), knowledge (T17) mirror `apps/web`'s state; a gate flips to live only when the owning slice does |
| **P6** | Parity gate + primacy runbook | Instrument green; a11y/perf met; owner sign-off; if yes, old-route redirect map, `apps/web` freeze, stabilisation period, retirement; if no, the walk stays an alternative client |

## 8. Work packages

Each is one issue, one branch, one PR (the ≤5 split is written per stage where a stage exceeds it).

| WP | Title | Files | Acceptance |
|---|---|---|---|
| **W0** | Governance, census and the instrument | `docs/evidence/walk-census.json`, `scripts/build-walk-coverage.py`, `docs/plans/00-INDEX.md` registration, the issue set | The census reproduces the baseline counts; `--check` fails on a missing route/action row and on an expired exemption; ADR 0021 accepted; programme registration merged |
| **W1** | P0 spike | throwaway `spike/walk/` (deleted after), `docs/decisions/0022-walk-world-engine.md`, `docs/evidence/walk-spike.md` | Every mechanic in P0 proven with recorded commands and numbers; engine ADR merged with budgets |
| **W2** | P1 foundation + Portfolio | `apps/walk/{index.html,src/**,e2e/**,dev-server.mjs,playwright.config.ts}`, `crates/api/src/web.rs` + test, `Dockerfile`, walk workflow | Sign-in, manifest nav, entrance, Portfolio read + one write, deep link, back/forward, spec green, fallback test green |
| **W3** | P2 read parity | `apps/walk/src/destinations/**`, `e2e/*.spec.ts`, registry rows | Every destination `read` and its spec asserts empty/error/happy/gate; the instrument is green for reads |
| **W4** | P3 write parity | destination modules, `src/forms/**`, specs | Every destination `write`; headers/approval/refusal tests; no optimistic mutation; the instrument is green for writes |
| **W5** | P4 retained V1 | destination modules + registry + specs | All 52 routes mapped; V1 reads/writes covered; redirects to the operator app where a flow is not yet ported (named, not silent) |
| **W6** | P5 slice-dependent | destination modules + specs | Each surface mirrors `apps/web`'s gate; a slice landing flips both clients in one contract change |
| **W7** | P6 gate + primacy | `docs/evidence/walk-coverage.json`, runbook, redirect map, `apps/web` freeze | Instrument green; a11y/perf recorded; owner decision; if yes, the redirect map is tested and the reversal path documented |
| **W8** | Hardening | `e2e/a11y.spec.ts`, perf harness, engine integration (if Bevy) | Budgets met; keyboard-only run; 5-minute soak; engine lazy-load and fallback proven |

**Single-writer seams:** `src/router/registry.js`, `src/api/manifest.js`, `src/shell/sheet-host.js`
and `src/world/port.js` are edited by one WP at a time; destination modules are disjoint.

## 9. Parity matrix (initial state at the baseline)

Every row starts `absent`. A row reaches `verified` only through §6.

| Surface | Walk route | Read | Write | Spec | Status |
|---|---|---|---|---|---|
| Organisation | `/walk/organisation` (+ `/walk/jobs`, `/walk/agents`, `/walk/tools`, `/walk/project-types`) | §10.5 | §10.5 | `organisation.spec.ts` | absent |
| Procedures | `/walk/procedures` (+ `/walk/build/processes`, `/walk/build/process-map`) | §10.5 | §10.5 | `procedures.spec.ts` | absent |
| Authoring | `/walk/authoring` | §10.5 | §10.5 | `authoring.spec.ts` | absent |
| Portfolio | `/walk/portfolio`, `/walk/portfolio/:programme_id` | §10.5 | §10.5 | `portfolio.spec.ts` | absent |
| Meetings | `/walk/meetings`, `/walk/meetings/:id` | T19 | T19 | `meetings.spec.ts` | absent |
| Programme | `/walk/programme` | §10.5 | §10.5 | `programme.spec.ts` | absent |
| Architecture (ADM) | `/walk/design/adm` | §10.5 | §10.5 | `adm.spec.ts` | absent |
| Plan | `/walk/plan` (+ `/walk/board/:board_id`) | §10.5 | §10.5 | `plan.spec.ts` | absent |
| Improve | `/walk/improve`, `/walk/evals/*` | §10.5 | §10.5 | `improve.spec.ts` | absent |
| Operations | `/walk/operations` | §10.5 | §10.5 | `operations.spec.ts` | absent |
| Tools | `/walk/tools` | §10.5 | §10.5 | `tools.spec.ts` | absent |
| V1 retained (Today, boards, cards, runs, approvals, requirements, models, system architecture, processes, map, search, usage, prompt audit, evals, benchmarks, settings, copilot, platform) | `/walk` + app path | literal GETs | actions | the matching `apps/web` spec | absent |
| Utilities (palette, sign-in, tenant switch, theme) | `/walk/` | `/api/me`, config | none | `shell`/`shell-login` | absent |

## 10. Risks

| Risk | Mitigation |
|---|---|
| 597 actions and 52 routes is a multi-tranche build | schema-driven action sheet gives full reachability early; destinations are prioritised by the owner; the instrument makes gaps visible |
| A second shell/auth/test harness drifts from the app | one contract, one census, behavioural trace parity on named flows, per-destination specs; a change to a contract updates both |
| Bevy on the web: bundle, startup, WebGL2/WebGPU, limited text/table widgets | P0 measures against the canvas baseline; the port keeps the engine replaceable; the baseline is never deleted |
| No local Rust builds (owner rule) slows the Bevy loop | all wasm builds on GitHub-hosted CI; the baseline needs no Rust; the spike is CI-first |
| Deep links today fall through to the operator app | the scoped `web.rs` fallback plus a Rust test lands in P1, before any link is published |
| OIDC redirect URI is an external change | P0 proves it; if the operator refuses, register a dedicated public client and serve its id to the walk |
| Two clients make the coverage record ambiguous | D67 and §10.5.11 keep D65 measuring `apps/web`; the walk has its own record and gate |
| Scope creep into a second backend | the Do-Not list in the spec; any need for new surface is a stop-and-escalate, not an action |
| The mock's fixtures or embedded pack leak into the client | explicit Do-Not; no `localStorage` programme state; a test asserts no snapshot URL is fetched by a signed-in walk |
| Accessibility regresses under a game UI | DOM-only work surfaces; keyboard-only spec; canvas is decorative and duplicated by the map |

## 11. Verification and definition of done

**Per work package** — the WP's own acceptance table, its Playwright spec(s), the parity instrument's
rows, and a contract test for each write (headers, refusal, refetch). A WP is not done because the
screen renders; it is done when the state, the write and the refusal are proven.

**Unit and type** — `node --test apps/walk/src` and `tsc --noEmit` (JSDoc) pass; pure modules only.

**Browser** — `npx playwright test -c apps/walk/playwright.config.ts` with the dev server; every spec
asserts empty, error, happy and one refusal or write; auth and deep-link gates included.

**Parity** — `python3 scripts/build-walk-coverage.py --check` exits 0; the record is committed; an
expired exemption fails.

**Accessibility** — keyboard-only navigation of every wing; focus return; contrast from computed
styles; 200% zoom and 390 px width with no horizontal scroll.

**Performance** — the §9 budgets; a 5-minute soak; the engine protocol recorded for ADR 0022.

**Docs** — `python3 docs/design/scripts/build-docs.py` and `node docs/design/scripts/check-docs.cjs`
pass whenever a Markdown file changes.

**DoD for the project** — the P6 gate, not "the UI looks finished". Until P6 the plan's matrix is
the truth and `apps/web` remains normative.

## 12. Session guide

**Read order** — `docs/decisions/0021-gemba-walk-client.md` → `docs/design/GEMBA-WALK-SPEC.md` →
this plan → SPEC_V2 §10.5 (the destination contracts) → `apps/web/src/api.rs` and
`apps/web/src/auth.rs` (the client contract) → the owning slice's plan for the destination.

**Commands** (available once W2 lands) —

```
node apps/walk/dev-server.mjs                       # :8932, proxies /api to :8930
npx playwright test -c apps/walk/playwright.config.ts
node --test apps/walk/src
npx tsc -p apps/walk --noEmit
python3 scripts/build-walk-coverage.py --check
python3 docs/design/scripts/build-docs.py && node docs/design/scripts/check-docs.cjs
```

**Rules**

- One issue per WP; push the lane branch before the work; one PR per repo per unit; no second PR
  without the audited `SECOND_PR=` reason.
- No Rust compilation on local hardware; the compile verdict is GitHub-hosted CI.
- No new table, action, event, port or decision store; if the walk seems to need one, stop and
  escalate — the answer is a contract-faithful screen, not a backend change.
- The engine ADR (0022) precedes any Bevy code beyond the spike; until then the canvas baseline is
  the implementation.
- Contract tests per WP: manifest gating, headers, refusal, refetch.
- The Do-Not list in the spec is the review checklist for every PR.

## 13. Non-goals

A second backend or decision store; SSR; a native mobile app; chat/AV; customer care; replacing the
design pack; a public unauthenticated v1; pixel parity with the mock; retiring `apps/web` before the
P6 gate; adding a SPEC_V2 §12 `T##` row or changing `seed/programme.json` in this unit.
