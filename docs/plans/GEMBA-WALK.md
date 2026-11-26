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
`docs/design/GEMBA-WALK-SPEC.md` · Baseline measured at `main@d958824f8` on 2026-11-26 and
re-verified at `main@0d6db794b` on 2026-11-26 (counts unchanged).

This plan is deliberately **not** a SPEC_V2 §12 `T##` slice. The programme-seed generator parses
only `docs/plans/slices/S*.md` and `**(T##)**` rows in §12, so a T-row would silently change
`seed/programme.json` and fail `build-programme-seed.py --check`. The walk is registered as a
cross-cutting project in `docs/plans/00-INDEX.md`; the parent issue and the programme registration
are W0, taken only once the owner accepts ADR 0021.

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
- W0 creates the parent issue and registers the programme project once ADR 0021 is accepted; the
  registration is a reviewable data change through `scripts/build-programme-seed.py` and
  `seed/programme-definition.json`, never a hand-edit of `seed/programme.json` or
  `docs/design/data/issues.json` (see §8.1).
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

The frozen census `docs/evidence/walk-census.json` is written once at W0 and re-baselined only by
a signed commit:

```json
{
  "baseline": {"date": "YYYY-MM-DD", "git_sha": "<40 hex>"},
  "routes": [{"pattern": "/boards", "view": "BoardIndex", "spec": "v1/board.spec.ts"}],
  "actions": [{"name": "cards.move", "kind": "internal", "scopes": ["cards:write"],
               "effect": "write", "confirm": "none", "tenancy": "tenant"}],
  "counts": {"routes": 52, "actions": 597}
}
```

The census is built from:

- route patterns and views — `scripts/build-v2-coverage.py:app_routes()` and
  `app_route_views()` (its `ROUTE` regex, line 42), the same parser the V2 record uses;
- action names, scopes, kind, effect, confirm, tenancy and idempotence — parsed from the
  `#[action(...)]` attributes under `crates/actions/src/**/*.rs`; the parsed names are
  cross-checked against the 597 names in `docs/design/data/actions.json`, so a parse miss fails
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

**Behavioural parity** — for each named flow, the same scripted task runs against `apps/web` and
`/walk/` in dev; the harness captures the `POST /api/actions/{name}/invocations` trace (names,
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
| **P0** | Governance (ADR 0021 accepted; §10.5.11/D67 land with this unit), census + instrument, spike | The spike proves PKCE redirect to `/walk/` on the dev origin, SSE via fetch, schema forms for read/write/destructive with approval, all headers, a deep link served by the spike's own `/walk/*` fallback, and a static copy; it measures canvas-2D vs Bevy 0.18 WASM and ADR 0022 records the winner and budgets. The production `web.rs` fallback and Dockerfile copy are W2 acceptance, not spike evidence; no unproven assumption enters P1 |
| **P1** | Foundation + Portfolio end-to-end | Sign-in (dev; configured OIDC once the external redirect URI is registered — otherwise that one scenario is acceptance-blocked, not a construction blocker), tenant switch, manifest-driven nav, world entrance, Room `portfolio` read + **one real write**, deep link and back/forward, dev server, Dockerfile copy, `web.rs` fallback + test, spec green |
| **P2** | Read parity across §10.5 | Every destination renders its reads with loading/empty/partial/error/denied and the honest gate; each has its spec |
| **P3** | Write parity + gates | Every destination's writes go through the action sheet with `If-Match`/`Idempotency-Key`/approval, refusals shown verbatim, no optimistic mutation |
| **P4** | Retained V1 surfaces | All 52 route patterns have a walk route; the V1 families have reads and their writes; specs |
| **P5** | Slice-dependent surfaces | Meetings (T19), tools (T13–T15), operations (T7), ADM (T9), knowledge (T17) mirror `apps/web`'s state; a gate flips to live only when the owning slice does |
| **P6** | Parity gate + primacy runbook | Instrument green; a11y/perf met; owner sign-off; if yes, old-route redirect map, `apps/web` freeze, stabilisation period, retirement; if no, the walk stays an alternative client |

**External prerequisites.** The `/walk/` OIDC redirect-URI registration is an external change: the
parent issue records it in Dependencies as an external `acceptance-blocked-by` with the owner as
the actor, not as a `status:needs-human` label (that label would freeze all construction). The dev
cookie (`/__dev/login`) and the dev origin's redirect URI unblock the spike; the walk never waits
on the registration to build.

**Rust builds.** No local cargo: the wasm compile, lint and tests run on GitHub-hosted
preflight/CI, and the issue records the run id; the PR opens only after that verdict, per the
repository's `CLAUDE.md`. The spike's throwaway workflow is deleted with the spike.

## 8. Delivery model and the issue split

**The repository rule.** `docs/plans/03-DELIVERY.md` is the authority: one plan = one issue = one
branch = exactly one PR by default; a plan may be split into at most **five** file-disjoint
siblings only with the owner's written exception, and `lane/` branches are retired. This plan is
therefore **one unit** unless the owner approves the split below.

**Proposed exception — one parent + four siblings (5 total).** The walk is multi-session by shape
(43 screens, 597 actions), so the parent keeps the dependency-bearing foundation and the siblings
own disjoint destination wings:

| Issue | Contains | Files it owns | Acceptance |
|---|---|---|---|
| **Parent G0** | W0 governance/census/instrument, W1 spike, W2 foundation + Portfolio, serving glue | `scripts/build-walk-coverage.py`, `docs/evidence/walk-*`, `spike/walk/**` (deleted), `apps/walk/index.html`, `src/main.js`, `src/auth/**`, `src/api/**`, `src/forms/**`, `src/router/registry.js`, `src/router/routes/core.js`, `src/shell/**`, `src/world/**`, `src/state/**`, `src/a11y/**`, `src/destinations/portfolio.js`, `e2e/shell/**`, `e2e/portfolio/**`, `dev-server.mjs`, `playwright.config.ts`, `crates/api/src/web.rs`, `Dockerfile`, the walk workflow | P0/W0/W1/W2 acceptance; Portfolio read + one write; the five route files exist as stubs |
| **S1** | W3/W4 for Portfolio & plan (except `portfolio.*`) + Organisation | `src/destinations/{programme,plan,validation,execution,decisions,runs,meetings,organisation,jobs,agents,tools,project-types}.js`, `src/router/routes/portfolio-organisation.js`, `e2e/portfolio-organisation/**` | Every owned destination `read` and `write`; specs assert empty/error/happy/refusal/write; instrument green for its rows |
| **S2** | W3/W4 for Procedures & authoring + Govern & design | `src/destinations/{procedures,processes,process-map,authoring,adm,architecture,requirements,models,copilot}.js`, `src/router/routes/procedures-governance.js`, `e2e/procedures-governance/**` | Same |
| **S3** | W3/W4 for Knowledge + Measure & improve + Operations + Utilities | `src/destinations/{knowledge,improve,evals,benchmarks,usage,prompt-audit,metering,operations,search,platform,settings,approvals}.js`, `src/router/routes/measure-operations-utilities.js`, `e2e/measure-operations-utilities/**` | Same |
| **S4** | W5 retained V1, W6 slice-dependent, W7 P6 gate + primacy, W8 hardening | `src/destinations/v1/**`, `src/router/routes/v1.js`, `e2e/v1/**`, `docs/evidence/walk-coverage.json`, the P6 runbook and redirect map | All 52 routes; P6 gate; a11y/perf; owner sign-off |

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
`src/destinations/v1/**` holds only V1 surfaces with no owning destination module.

**Shared seams.** `src/api/manifest.js`, `src/shell/sheet-host.js`, `src/world/port.js`,
`src/forms/**` and `src/a11y/**` are parent-owned; siblings consume them and do not edit them. A
sibling that needs a seam change stops and asks the parent's owner rather than forking the
contract.

**If the owner declines the split.** The whole plan is one issue (`GEMBA-WALK`), the stages are
commits on one `issue/<n>-gemba-walk` branch, and the P6 gate closes the same PR. The plan states
both paths so no session has to improvise.

### 8.1 W0 runbook — the first session

1. **Precondition.** The owner has accepted ADR 0021 in writing — either by directing its status
   line (`docs/decisions/0021-gemba-walk-client.md:3`) to `Accepted` in this docs PR before it
   merges, or in the W0 branch's first commit — after reviewing the "supersedes in part" wording,
   and the docs are on `main`. Until then the parent issue is `status:blocked`; a session must not
   start W0 on a Proposed ADR.
2. **Create the label and the parent issue.** Create the `GEMBA-WALK` label if absent
   (`gh label create GEMBA-WALK ...`), then create the parent issue from
   `.github/ISSUE_TEMPLATE/unit.md` with that label and `status:ready` once step 1 is true
   (otherwise `status:blocked`):
   - Goal: the walk foundation is buildable, measured and served at `/walk/`.
   - Plan source: `docs/plans/GEMBA-WALK.md` §7–§8.
   - Programme: Tranche `unmapped` — this is a cross-cutting programme unit with no programme
     tranche (the convention #760 and #745 use; tranche 0 is closed); Brief
     `docs/decisions/0021-gemba-walk-client.md` and this plan; ADM `Preliminary` / `definition`;
     Benefits `enabling — no direct benefit` with one sentence of reason.
   - Baseline: `main@<sha>` measured at claim time; doc blob shas from
     `git rev-parse main:docs/plans/GEMBA-WALK.md` and
     `main:docs/design/GEMBA-WALK-SPEC.md`.
   - Parts: W0, W1 and W2 from §7–§8, each with at least one BDD `Verify:` command.
   - Dependencies: the plan's `depends_on` edges ([P0, T18, T19]), each resolved to a live issue
     at claim time (a closed slice is a `related` note, not a blocker); the OIDC registration as
     an external `acceptance-blocked-by` (owner/ops; not an issue and not a `status:needs-human`
     label); the sibling edges from the table above.
3. **Register the project** (a reviewable data change, not prose). The label is what
   `project_for_issue()` matches, so it must exist before the seed is regenerated:
   - add `GEMBA-WALK` to `CROSS_CUTTING` in `scripts/build-programme-seed.py` (name, plan path,
     `track: "programme"`, issue numbers) and to `seed/programme-definition.json` under
     `projects` with `{"adm_phase": "Preliminary", "plateau": "definition"}`;
   - run `python3 scripts/build-programme-seed.py` and
     `python3 scripts/check-programme-definition.py`; never hand-edit
     `docs/design/data/issues.json` — `.github/workflows/issue-registry.yml` owns it.
4. **Freeze the census and build the instrument** exactly as §6. Commit
   `docs/evidence/walk-census.json`, `docs/evidence/walk-exemptions.json` and
   `scripts/build-walk-coverage.py`. Acceptance: the script reproduces 52 routes and 597 actions;
   `--check` returns non-zero for a missing registry row, an unreachable action, a missing spec
   and an expired exemption; it runs with no server or database.
5. **Record the spike and the engine ADR** (§7 P0 / §8 W1), then the foundation (§7 P1 / §8 W2).

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

**Per issue** — the issue's own parts and acceptance criteria, its Playwright spec(s), the parity
instrument's rows, and a contract test for each write (headers, refusal, refetch). An issue is not
done because the screen renders; it is done when the state, the write and the refusal are proven.

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

**Read order** — `CLAUDE.md` → `docs/SPEC.md` → `docs/plans/00-INDEX.md` →
`docs/plans/03-DELIVERY.md` → `docs/decisions/0021-gemba-walk-client.md` →
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
P6 gate; adding a SPEC_V2 §12 `T##` row or changing `seed/programme.json` in this unit.
