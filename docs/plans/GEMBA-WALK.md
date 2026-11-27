---
project: GEMBA-WALK
title: The Gemba Walk — a game-style candidate primary UI
status: approved
depends_on: [P0, T18, T19]
spec_sections: ["§10.5", "§10.5.11", "§15.1 D64/D66/D67/D68"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: [walk]
parity_ids: []
---

# GEMBA-WALK — the Gemba Walk client

Owner direction: 2026-11-26; amended 2026-11-27 (the game host and its own identity) · ADR:
`docs/decisions/0021-gemba-walk-client.md` (**Accepted** 2026-11-26 as a delegated rule decision) and
`docs/decisions/0024-gemba-walk-host-and-identity.md` (**Accepted** 2026-11-27; supersedes 0021
decisions 1 and 4 in part) · Change request:
`docs/plans/inception/GEMBA-WALK/08-change-request-001-host-and-identity.md` · Spec:
`docs/design/GEMBA-WALK-SPEC.md` · Procedural set: `docs/plans/inception/GEMBA-WALK/` · Baseline
re-measured at `main@01544010f` on 2026-11-27; the earlier baselines were `d958824f8`, `0d6db794b`,
`cedd84980` and `e2507c860`.

This plan is deliberately **not** a SPEC_V2 §12 `T##` slice. The programme-seed generator parses
only `docs/plans/slices/S*.md` and `**(T##)**` rows in §12, so a T-row would silently change
`seed/programme.json`; the frozen seed moves only by a reviewed data PR. The walk is registered as a
cross-cutting project in `docs/plans/00-INDEX.md`; the inception unit (#792) publishes the
MSP/TOGAF/SAFe set and registers the project, and the build issues G0/S1–S4 are created after the
publication gate.

## 1. Objective and gate

Build a second, game-style client of the conveyor at **`https://game.nonagentic.ai/`**, at the host
root, with **all features and all functionality** — every read and every write through the existing
registry — and measure it well enough that the owner can decide whether it becomes the primary UI.
The client has its own public Keycloak client, `conveyor-game`, and the operator origin's `/walk/*`
redirects to the game host (ADR 0024).

**Gate (P6):** every §10.5 destination and every retained V1 route has a walk route and a Playwright
spec asserting empty, error, happy and one refusal or write; every action in the frozen census is
reachable and schema-rendered; the named write flows round-trip with the correct headers; the parity
instrument is green with no expired exemption; accessibility and performance budgets are met; the
owner signs the primacy decision. Until then `apps/web` is normative.

## 2. Starting state (measured, not assumed)

| Part | State | Evidence |
|---|---|---|
| Operator UI | Leptos CSR/WASM; **52** route patterns, **43** screen modules, **46** Playwright specs | `apps/web/src/app.rs`, `apps/web/src/screens/`, `apps/web/e2e/` |
| Backend contract | **598** registered actions (572 attributes + 26 manual registrations); manifest, `actions.describe` (input/output JSON Schema, effect, confirm, tenancy, spec_hash), invocation envelope, RFC 9457 problems, `next_cursor`, SSE | `docs/design/data/actions.json`, `apps/web/src/api.rs`, `crates/actions/src/views.rs` |
| Auth | auth-code + PKCE S256, token in memory, verifier in `sessionStorage`; dev cookie; `/config.json`; realm `conveyor`, public client `conveyor-web`, `aud`/`azp` pinned | `apps/web/src/auth.rs`, `crates/auth/src/{config,jwks}.rs`, `crates/api/src/web.rs` |
| Serving | the same image serves a static tree and a directory sub-site; the design pack is copied to `/out/site/design`; host dispatch does not exist yet, so a game host would serve the operator app | `crates/api/src/web.rs`, `Dockerfile:79-93` |
| Hosting | Cloudflare → `cloudflared-apps` → istio gateway → `conveyor-public-vs` → `conveyor:8930`; DNS by the `dns-ensure` CronJob; `game.nonagentic.ai` exists nowhere | gitops-production `clusters/strato/**` |
| Walk client | **absent**: `apps/walk` carries only `data/world.json`; no index, modules, specs, dev server, workflow or Dockerfile copy | `git ls-files apps/walk` at the branch |
| The mock | a local single-file artefact only; no auth, no registry, programme state in `localStorage`, embeds a 4.7 MB gzip copy of the design pack | owner's local file; not a repository input |
| Governance | ADR 0013/D64 say the app is the UI and reject a second frontend; ADR 0021 supersedes them **in part** for this client | `docs/decisions/0013-real-v2-ui.md`, `docs/SPEC_V2.md` §10.5.10, D64 |

The baseline counts are frozen by W0 into `docs/evidence/walk-census.json`. The gate is compared to
that census, not to a live manifest, so a later slice adding actions cannot make it unpassable; a
re-baseline is a signed commit that states what changed.

## 3. Scope

**IN** — a static client at `https://game.nonagentic.ai/` (one image, one service, a second host);
route parity with every `apps/web` path at the host root; the seven wings and the destination rooms;
the full read and write contract; seats as view presets; the DOM sheet system; the schema-driven
action sheet; the world port and the engine decision; the parity instrument and its evidence;
per-destination Playwright specs; accessibility and performance budgets; the host dispatch, the
`/walk` 308 and the game host's boundary; its own public Keycloak client `conveyor-game`; the live
sign-in proof; the primacy runbook.

**OUT** — any new table, action, event, port or backend read model; a second decision store; a
replacement for `/design/`; native mobile; chat/audio/video; customer care (D48); SSR; a public
unauthenticated mode in v1; pixel parity with the mock; deleting `apps/web` before the gate.

**"All functionality" is defined as**: every §10.5 destination and every retained V1 route has a
walk route and a spec; every manifest action is reachable and form-renderable; every destination's
primary write flow is exercised end to end; every gate the server enforces is surfaced, never
pre-empted. It is not defined as pixel or copy parity with either existing client.

## 4. Relationship to the programme

- Not a §12 `T##` slice. The inception unit (#792) registers the project and publishes the
  inception set before any build issue is created; the registration is a reviewable data PR to
  `seed/programme.json` and `seed/programme-definition.json`, applied by `programmes.refresh` —
  never a hand edit of the live rows or of `docs/design/data/issues.json` (see §8.1).
- The V2 coverage instrument (D65, `scripts/build-v2-coverage.py`) continues to measure `apps/web`
  and is not touched. The walk's evidence is `docs/evidence/walk-coverage.json`.
- The design pack (`/design/`) keeps its public role and is not rebuilt or embedded.

## 5. Architecture decisions (normative; detail in the spec)

1. Served at `https://game.nonagentic.ai/`, at the root, from the same image and service as the
   API; the operator origin's `/walk/*` returns a 308 to the game host (ADR 0024, CR-WALK-001).
2. Route parity by construction: the walk path **is** the app path on the game host; `/` is the
   entrance. `/walk/*` remains only as the no-host dev/CI shape.
3. Full functionality through the existing contract; no backend surface.
4. DOM sheets own all work; the world is navigation and state display only.
5. Seats are view presets; the manifest and server scopes own authority.
6. No fixtures; no client-side programme store; no published snapshot as the app's source.
7. `WorldView` port; the simulation is `bevy_ecs 0.20.1` in `crates/walk-world` (ADR 0022); the
   renderer is canvas-2D (ADR 0023, after the P0 spike measured it against WebGL2; WebGL2 remains a
   candidate behind the port).
8. Host dispatch in `crates/api/src/web.rs` and a router-level gate in `crates/api/src/lib.rs`:
   the game host serves the walk tree, `/config.json` and the API, refuses `/mcp` and `/design`,
   and never serves the operator index; the operator host keeps its behaviour and 308s `/walk/*`.
   Rust tests cover the matrix; the redirect target is `CONVEYOR_GAME_PUBLIC_URL`, never the
   request `Host`.
9. Schema-driven action sheet so no action is unreachable; unsupported schema falls back to a
   labelled JSON editor.
10. Parity is measured by a frozen census and per-destination specs, never by prose.
11. The game has its own public client `conveyor-game` in the `conveyor` realm (not a new realm);
    the API accepts the two-client `azp` allowlist against the `conveyor` audience, and the live
    proof is a real `conveyor-e2e` sign-in. `apps/web` remains normative until P6.

## 6. The parity model and instrument

`scripts/build-walk-coverage.py` (W0) is the only measure of walk parity. It must be able to run in
CI with no server and no database.

**Inputs**

The frozen census `docs/evidence/walk-census.json` is written once at W0 and re-baselined only by
a signed commit:

```json
{
  "baseline": {"date": "YYYY-MM-DD", "git_sha": "<40 hex>"},
  "routes": [{"pattern": "/boards", "view": "BoardIndex", "spec": "v1/board.spec.ts"}],
  "actions": [{"name": "cards.move", "kind": "internal", "scopes": ["cards:write"],
               "effect": "write", "confirm": "none", "tenancy": "tenant",
               "idempotent": true}],
  "counts": {"routes": 52, "actions": 598}
}
```

The census is built from:

- route patterns and views — `scripts/build-v2-coverage.py:app_routes()` and
  `app_route_views()` (its `ROUTE` regex, line 42), the same parser the V2 record uses;
- action names, scopes, kind, effect, confirm, tenancy and idempotence — parsed from the
  `#[action(...)]` attributes under `crates/actions/src/**/*.rs`; the parsed names are
  cross-checked against the 598 names in `docs/design/data/actions.json`, so a parse miss fails
  the build instead of shrinking the census;
- `apps/walk/src/router/routes/{core,portfolio-organisation,procedures-governance,measure-operations-utilities,v1}.js`
  — one row per surface (walk path, app path, room, spec file), composed by
  `apps/walk/src/router/registry.js`. One route file per delivery sibling; a sibling never edits
  another's file;
- `apps/walk/e2e/<wing>/*.spec.ts`.

The census holds no live manifest and no server state.

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

**Behavioural parity** — for each named flow, the same scripted task runs against `apps/web` and the walk on the dev origin
(the game-host shape); the harness captures the `POST /api/actions/{name}/invocations` trace (names,
status, order) and compares it. Pixels are not compared.

**Exemptions** — a separate signed record, `docs/evidence/walk-exemptions.json`, holding
`{surface, reason, signer, expires}`; an expired exemption fails the gate. The owner or the plan's
named maintainer signs; the default expiry is 30 days.

**Output and check** — `docs/evidence/walk-coverage.json`, emitted by
`python3 scripts/build-walk-coverage.py`; `--check` exits non-zero when a census route has no
registry row, a census action lacks `reachable` or `schema-rendered`, a destination lacks its
spec, or an exemption is expired. The check runs in the walk workflow and in `web.yml` (or its
successor) once the walk app is on `main`; it needs no server, database or network.

## 7. Stages and exit gates

| Stage | Work | Exit gate |
|---|---|---|
| **P0** | Governance (ADR 0021 accepted; §10.5.11/D67 land with this unit), census + instrument, spike | The spike proves the PKCE S256 construction and records the exact dev issuer/redirect state (the game client and host are delivered by this unit under CR-WALK-001; the production proof is the deployed OIDC spec), SSE via fetch, schema forms for read/write/destructive with approval, all headers, a deep link served by the spike's own `/walk/*` fallback, and a static copy; it measures canvas-2D against WebGL2 and ADR 0023 records the measured budgets and selects canvas-2D (the simulation is `bevy_ecs` in `crates/walk-world`, ADR 0022). The production `web.rs` fallback and Dockerfile copy are W2 acceptance, not spike evidence; no unproven assumption enters P1 |
| **P1** | Foundation + Portfolio end-to-end | Sign-in (dev cookie in CI; the real `conveyor-game` PKCE sign-in in the deployed proof), tenant switch, manifest-driven nav, world entrance, Room `portfolio` read + **one real write**, deep link and back/forward, dev server, Dockerfile copy, the host dispatch + router gate + Rust host matrix, the Keycloak client and its read-back, spec green |
| **P2** | Read parity across §10.5 | Every destination renders its reads with loading/empty/partial/error/denied and the honest gate; each has its spec |
| **P3** | Write parity + gates | Every destination's writes go through the action sheet with `If-Match`/`Idempotency-Key`/approval, refusals shown verbatim, no optimistic mutation |
| **P4** | Retained V1 surfaces | All 52 route patterns have a walk route; the V1 families have reads and their writes; specs |
| **P5** | Slice-dependent surfaces | Meetings (T19), tools (T13–T15), operations (T7), ADM (T9), knowledge (T17) mirror `apps/web`'s state; S4 audits every gate, and a gate flips to live only when the owning slice does — as a follow-on issue on the sibling that owns the destination, never as an S4 edit |
| **P6** | Parity gate + primacy runbook | Instrument green; a11y/perf met; owner sign-off; if yes, old-route redirect map, `apps/web` freeze, stabilisation period, retirement; if no, the walk stays an alternative client |

**Identity is owned work, not an external prerequisite (2026-11-27).** CR-WALK-001 moves the game
client `conveyor-game` and the host into the unit: the realm-patch ConfigMap, the manual Job and
the admin-API read-back are delivery steps. The dev cookie still unblocks construction in CI; the
production sign-in is proven by the deployed OIDC spec as `conveyor-e2e`, never claimed from a
redirect.

**Rust builds.** No local cargo: the wasm compile, lint and tests run on GitHub-hosted
preflight/CI, and the issue records the run id; the PR opens only after that verdict, per the
repository's `CLAUDE.md`. The spike's throwaway workflow is deleted with the spike.

## 8. Delivery model and the issue split

**The repository rule.** `docs/plans/03-DELIVERY.md` is the authority: one plan = one issue = one
branch = exactly one PR by default; a plan may be split into at most **five** file-disjoint
siblings only with the written exception, and `lane/` branches are retired. The exception below is
approved.

**Approved exception (2026-11-26) — one parent + four siblings (5 total).** Approved as the written
exception by the build's rule approver under the published delegation; the inception unit (#792) is
the P0 §5 definition step, not a sixth split. The walk is multi-session by shape (43 screens, 598
actions), so the parent keeps the dependency-bearing foundation and the siblings own disjoint
destination wings:

| Issue | Contains | Files it owns | Acceptance |
|---|---|---|---|
| **Parent G0** | W0 governance/census/instrument, W1 spike, W2 foundation + Portfolio, serving glue | `scripts/build-walk-coverage.py`, `docs/evidence/walk-census.json`, `docs/evidence/walk-exemptions.json`, the initial `docs/evidence/walk-coverage.json`, `spike/walk/**` (deleted), `apps/walk/index.html`, `src/main.js`, `src/auth/**`, `src/api/**`, `src/forms/**`, `src/router/registry.js`, `src/router/routes/core.js`, `src/shell/**`, `src/world/**`, `src/state/**`, `src/a11y/**`, `src/destinations/portfolio.js`, `e2e/shell/**`, `e2e/portfolio/**`, `dev-server.mjs`, `playwright.config.ts`, `crates/auth/src/{config,jwks}.rs`, `crates/api/src/{config,lib,web}.rs`, `crates/api/tests/web_hosts.rs`, `Dockerfile`, the walk workflow, the gitops client/host change (separate PR) | P0/W0/W1/W2 acceptance; Portfolio read + one write; the five route files exist as stubs; the host matrix and the live `conveyor-game` sign-in green |
| **S1** | W3/W4 for Portfolio & plan (except `portfolio.*`) + Organisation | `src/destinations/{programme,plan,validation,execution,decisions,runs,meetings,organisation,jobs,agents,tools,project-types}.js`, `src/router/routes/portfolio-organisation.js`, `e2e/portfolio-organisation/**` | Every owned destination `read` and `write`; specs assert empty/error/happy/refusal/write; instrument green for its rows |
| **S2** | W3/W4 for Procedures & authoring + Govern & design | `src/destinations/{procedures,processes,process-map,authoring,adm,architecture,requirements,models,copilot}.js`, `src/router/routes/procedures-governance.js`, `e2e/procedures-governance/**` | Same |
| **S3** | W3/W4 for Knowledge + Measure & improve + Operations + Utilities | `src/destinations/{knowledge,improve,evals,benchmarks,usage,prompt-audit,metering,operations,search,platform,settings,approvals}.js`, `src/router/routes/measure-operations-utilities.js`, `e2e/measure-operations-utilities/**` | Same |
| **S4** | W5 retained V1, W6 slice-gate audit, W7 P6 gate + primacy, W8 hardening | `src/destinations/v1/**`, `src/router/routes/v1.js`, `e2e/v1/**`, the regenerated `docs/evidence/walk-coverage.json`, the P6 runbook and redirect map | All 52 routes; every slice gate matches `apps/web`; P6 gate; a11y/perf; owner sign-off |

**Dependencies.** The parent `blocks` S1–S4. S1–S3 are parallel and file-disjoint. S4 is
`blocked-by` S1–S3. The parent creates all five route files as empty row lists, so a sibling fills
only its own file and the duplicate-PR guard (`block-duplicate-pr.py`) never sees overlapping
edits.

**Work-package map.** W0–W2 are parts of the parent; W3–W4 are the destination work distributed
across S1–S3 by wing; W5–W8 are S4. The stage table in §7 stays the content order; the split is by
file ownership, not by read/write phase, because a destination module owns both its reads and its
writes — splitting reads from writes would make the two issues touch the same files. Where a
retained V1 route is already a §10.5 destination owned by S1–S3, S4 reuses that module and its
route row, adding only the V1-retained spec trace; it never edits another sibling's file. Its
`src/destinations/v1/**` holds only V1 surfaces with no owning destination module. W6 is an audit:
a slice landing is a follow-on issue on the sibling that owns the destination, not an S4 edit, so
the four wings stay file-disjoint.

**Generated record.** `docs/evidence/walk-coverage.json` is emitted by the instrument, never
hand-edited; each issue that changes coverage regenerates and commits it. It is the only shared
generated artifact in the split, and a generated-file overlap alone does not merge two siblings'
work; every hand-edited file stays disjoint.

**Shared seams.** `src/api/manifest.js`, `src/shell/sheet-host.js`, `src/world/port.js`,
`src/forms/**` and `src/a11y/**` are parent-owned; siblings consume them and do not edit them. A
sibling that needs a seam change stops and asks the parent's owner rather than forking the
contract.

**If the split is withdrawn** by change control, the whole plan is one issue (`GEMBA-WALK`), the
stages are commits on one `issue/<n>-gemba-walk` branch, and the P6 gate closes the same PR. The
plan states both paths so no session has to improvise.

### 8.1 Inception and build runbook

1. **Precondition — done.** ADR 0021 is Accepted (2026-11-26, delegated rule decision) and this
   docs unit (#792) carries the inception set. The set is published on the project by #792 before
   any build issue is claimed: P0 §5 makes an unpublished inception a hard gate, not a waiver.
2. **The inception unit (#792) registers the project** (a reviewable data PR, not prose):
   - add `GEMBA-WALK` to `seed/programme.json` as a programme project (name, plan path,
     `track: "programme"`, tranche 0, the inception issue number in `meta.issues`) and to
     `seed/programme-definition.json` under `projects` with
     `{"adm_phase": "Preliminary", "plateau": "definition"}`;
   - apply it with the `programmes.refresh` action (or a deploy, which seeds the same file), then
     run `python3 scripts/build-coverage.py --check` and
     `node docs/design/scripts/check-coverage.cjs` — an unknown unit is the early failure; update
     the seed-count assertions in the Rust tests that pin the project count. Never hand-edit
     `docs/design/data/issues.json` — `.github/workflows/issue-registry.yml` owns it;
   - the seed is frozen: the tranche-0 row is the registration of record, and the project's
     `meta.issues` is the link the design site shows. A later unit is added to that list by a
     follow-up data PR (the frozen seed never follows a label on its own).
3. **Publish the inception set** on the project (`documents.compose` → `documents.publish`), verify
   the bodies hash-equal to `docs/plans/inception/GEMBA-WALK/`, create the 10 typed artefact rows,
   and record the Phase-A SoAW through the Board with a distinct approver. Publish the benefit
   profile with the baseline `unmeasured`. This is the gate before step 4.
4. **Create G0 and S1–S4 after publication.** Create them from `.github/ISSUE_TEMPLATE/unit.md` with
   the `GEMBA-WALK` label; do not create them before the project's seed row is on `main`, and add
   their numbers to that row's `meta.issues` in a follow-up data PR (the frozen seed does not
   follow labels). G0's Programme fields: Tranche 0 (cross-cutting); Brief = the
   published `project-brief` + ADR 0021 + this plan; ADM `Preliminary`/`definition`; Benefits
   B-WALK-1 (enabling). Parts W0/W1/W2, each with a BDD `Verify:`; dependencies as in §8; the game client and host as owned delivery work (CR-WALK-001).
5. **G0's W0 freezes the census and builds the instrument** exactly as §6. Commit
   `docs/evidence/walk-census.json`, `docs/evidence/walk-exemptions.json` and
   `scripts/build-walk-coverage.py`. Acceptance: the script reproduces 52 routes and 598 actions;
   `--check` returns non-zero for a missing registry row, an unreachable action, a missing spec
   and an expired exemption; it runs with no server or database.
6. **Record the spike and the engine ADR** (§7 P0 / §8 W1), then the foundation (§7 P1 / §8 W2).

## 9. Parity matrix (initial state at the baseline)

Every row started `absent` at the baseline. A row reaches `verified` only through §6; the W3/W4
rows were recorded on 2026-11-27 (#814/#877, #815/#874, #816/#880), and S4's two V1 routes remain.

| Surface | Walk route | Read | Write | Spec | Status |
|---|---|---|---|---|---|
| Organisation | `/organisation` (+ `/jobs`, `/agents`, `/tools`, `/project-types`) | §10.5 | §10.5 | `organisation.spec.ts` | verified |
| Procedures | `/procedures` (+ `/build/processes`, `/build/process-map`) | §10.5 | §10.5 | `procedures.spec.ts` | verified |
| Authoring | `/authoring` | §10.5 | §10.5 | `authoring.spec.ts` | verified |
| Portfolio | `/portfolio`, `/portfolio/:programme_id` | §10.5 | §10.5 | `portfolio.spec.ts` | verified |
| Meetings | `/meetings`, `/meetings/:id` | T19 | T19 | `meetings.spec.ts` | verified |
| Programme | `/programme` | §10.5 | §10.5 | `programme.spec.ts` | verified |
| Architecture (ADM) | `/design/adm` | §10.5 | §10.5 | `adm.spec.ts` | verified |
| Plan | `/plan` (+ `/board/:board_id`) | §10.5 | §10.5 | `plan.spec.ts` | verified |
| Improve | `/improve`, `/evals/*` | §10.5 | §10.5 | `improve.spec.ts` | verified |
| Operations | `/operations` | §10.5 | §10.5 | `operations.spec.ts` | verified |
| Tools | `/tools` | §10.5 | §10.5 | `tools.spec.ts` | verified |
| V1 retained (Today, boards, cards, runs, approvals, requirements, models, system architecture, processes, map, search, usage, prompt audit, evals, benchmarks, settings, copilot, platform) | the app path | the app's read contract | actions | the matching `apps/web` spec; S4: `e2e/v1/board.spec.ts`, `e2e/v1/cockpit.spec.ts` | verified except `/boards`, `/card/:card_id` (S4) |
| Utilities (palette, sign-in, tenant switch, theme) | `/` | `/api/me`, config | none | `shell`/`shell-login` | verified |

## 10. Risks

| Risk | Mitigation |
|---|---|
| 598 actions and 52 routes is a multi-tranche build | schema-driven action sheet gives full reachability early; destinations are prioritised by the owner; the instrument makes gaps visible |
| A second shell/auth/test harness drifts from the app | one contract, one census, behavioural trace parity on named flows, per-destination specs; a change to a contract updates both |
| The world engine: wasm bundle and startup; the canvas renderer's density limit | the P0 spike measured canvas-2D (60 fps, 24 KB, 1.27 s cold load, stable heap); ADR 0023 selects it; the port keeps WebGL2 and the engine replaceable; the baseline is never deleted |
| No local Rust builds (owner rule) slows the Bevy loop | all wasm builds on GitHub-hosted CI; the baseline needs no Rust; the spike is CI-first |
| The game host leaks an operator surface | a router-level host gate before `mcp_auth`, a host-aware static dispatch, `app()` matrix tests, and live `/mcp` and `/design/*` 404 probes |
| The `azp` allowlist widens the replay guard | exactly two first-party ids, an optional game id never defaulted from the audience, and rejection tests |
| The Keycloak client drifts or the Job is skipped | a git-managed rep with a create-or-refuse preflight; the client, redirects, web origins, PKCE and audience mapper read back from the admin API |
| The live sign-in fails because the client or host is incomplete | the deployment order (image first, host second, Job third) and the deployed OIDC spec as the acceptance |
| Two clients make the coverage record ambiguous | D67 and §10.5.11 keep D65 measuring `apps/web`; the walk has its own record and gate |
| Scope creep into a second backend | the Do-Not list in the spec; any need for new surface is a stop-and-escalate, not an action |
| The mock's fixtures or embedded pack leak into the client | explicit Do-Not; no `localStorage` programme state; a test asserts no snapshot URL is fetched by a signed-in walk |
| Accessibility regresses under a game UI | DOM-only work surfaces; keyboard-only spec; canvas is decorative and duplicated by the map |

## 11. Verification and definition of done

**Per issue** — the issue's own parts and acceptance criteria, its Playwright spec(s), the parity
instrument's rows, and a contract test for each write (headers, refusal, refetch). An issue is not
done because the screen renders; it is done when the state, the write and the refusal are proven.

**Unit and type** — `node --test apps/walk/src` and `tsc --noEmit` (JSDoc) pass; pure modules only.

**Browser** — `npx playwright test -c apps/walk/playwright.config.ts` with the dev server; every spec
asserts empty, error, happy and one refusal or write; auth and deep-link gates included. The
deployed `oidc` project additionally performs one real PKCE sign-in as `conveyor-e2e` against
`https://game.nonagentic.ai/` and asserts the Portfolio read.

**Host matrix** — Rust tests through `app()` cover the game entry and deep link, assets and the
missing-asset 404, the per-host `/config.json`, the refusal of `/mcp` and `/design`, the operator
origin unchanged, the `/walk` 308 with the query, and the hostile-`Host` redirect guard.

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

**Read order** — `CLAUDE.md` → `docs/SPEC.md` → `docs/plans/00-INDEX.md` →
`docs/plans/03-DELIVERY.md` → `docs/decisions/0021-gemba-walk-client.md` →
`docs/decisions/0024-gemba-walk-host-and-identity.md` → `docs/plans/inception/GEMBA-WALK/README.md`
(the MSP/TOGAF/SAFe set) →
`docs/design/GEMBA-WALK-SPEC.md` → this plan → SPEC_V2 §10.5 (the destination contracts) →
`apps/web/src/api.rs` and `apps/web/src/auth.rs` (the client contract) → the owning slice's plan
for the destination.

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

- Claim the issue first (`issue-pickup`); no issue → no branch → no work. Work in
  `.worktrees/issue-<n>`, branch `issue/<n>-<slug>`, one PR with `Closes #<n>`. Slices are commits
  on that branch; they never get their own PR. `lane/` branches and the global lane protocol do
  not apply in this repository.
- The parent issue's five-sibling split is the only permitted split; a sibling that wants a sixth
  issue stops and asks the owner.
- No Rust compilation on local hardware; the compile/lint verdict is the repository's
  GitHub-hosted preflight, and the PR's CI is the final gate. Record the run id on the issue.
- No new table, action, event, port or decision store; if the walk seems to need one, stop and
  escalate — the answer is a contract-faithful screen, not a backend change.
- The engine ADR (0022) precedes any Bevy code beyond the spike; until then the canvas baseline is
  the implementation.
- Contract tests per issue: manifest gating, headers, refusal, refetch.
- The Do-Not list in the spec is the review checklist for every PR.

## 13. Non-goals

A second backend or decision store; SSR; a native mobile app; chat/AV; customer care; replacing the
design pack; a public unauthenticated v1; pixel parity with the mock; retiring `apps/web` before the
P6 gate; adding a SPEC_V2 §12 `T##` row. The only seed change is the sanctioned GEMBA-WALK
registration in the inception unit (#792); no other project's row moves.
