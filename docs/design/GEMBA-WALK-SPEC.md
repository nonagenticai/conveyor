# The Gemba Walk — client and architecture specification

Status: Proposed · Date: 2026-11-26 · Owner direction: 2026-11-26 · ADR: 0021
Normative siblings: SPEC_V2 §10.5, §10.5.11, §15.1 D64/D66/D67 · Plan: `docs/plans/GEMBA-WALK.md`

This document specifies a second client of the conveyor — a game-style, spatial interface — as a
candidate primary UI. It is written so a future session can build it without reading the local
single-file mock, and so a reviewer can tell a real state from a designed one. It is a specification,
not an implementation claim: every destination starts `absent` and is credited only by a route, a
spec and the parity instrument.

## 1. Status and relationship to `apps/web`

- The walk is a **separate client** served at `/walk/` from the same image and origin. It adds no
  port, no table, no action and no event.
- `apps/web` remains the **normative operator UI** until the parity gate in §13 and the owner's
  primacy decision. A V2 destination is still measured, for D65, from `apps/web`'s route table and
  its Playwright spec; the walk is measured by its own instrument.
- "Primary" means the walk owns the default entry point and the operator redirects; it does **not**
  mean a second backend, a second decision store or a second set of read models.
- The design pack at `/design/` keeps its role as the public programme site and reviewed reference.
  The walk is not a pack page and never ships inside `/design/`.

## 2. Experience model

**The world is navigation; the sheets are the work.** The canvas renders rooms, figures and state;
every record, table, form, confirmation and decision is a DOM sheet with ordinary HTML semantics.
No capability exists only in the world, and no text entry, table or long-form content is rendered
inside the canvas or a game engine.

- **Entrance** — `/walk/` is the arrival surface: the map, the current seat, the queues that need a
  decision, and a fast path into every wing. It is usable with no pointer and no walk.
- **Wings** — the seven navigation groups of SPEC_V2 §10.5.1 (Portfolio & plan; Organisation;
  Procedures & authoring; Govern & design; Knowledge & memory; Measure & improve; Operations) become
  wings. A wing is a corridor of rooms; a room is a destination; a desk inside a room is a tab or a
  sub-screen; a sheet is a record or a task.
- **The first six rooms** are the mock's validated set, rebuilt on real data: Architecture,
  Portfolio, Delivery (the board), Project desk, Coach and Kata. They are evidence that the
  interaction works, not the limit of the map. Every other destination gets a room before P6.
- **HUD** — the principal, the active tenant, connection/stream state, the approval and meeting
  queues, the command palette, the map, the camera control and help. The HUD is always reachable by
  keyboard; it is the shortest path to any deep link.
- **Movement** — pointer-drag or arrow keys to look, `WASD`/arrows to move, first/third person
  toggle with a labelled button and `V`, wheel zoom, and a boom that shortens near walls rather than
  clipping. The camera is a preference, never a requirement.
- **Fast travel** — the map and the palette move focus to any room or record. A user can complete an
  entire task through links, the palette and sheets without ever moving the avatar.

## 3. Seats and authority

- The principal and memberships come from `GET /api/me`; the active tenant is sent as
  `X-Active-Tenant` on every call.
- **Seats are view presets, not privileges.** Delivery (product owner), Programme (project office),
  Owner (tenant owner) and Reviewer (observer) select a default landing, queue and room emphasis.
  Changing a seat never changes a permission: the server's manifest, scopes and refusal are the only
  authority. A control whose action is absent from the manifest is not rendered.
- The mock's Customer seat is **not** carried over. Customer care is out of conveyor's scope (D48);
  the external stakeholder reads the public pack at `/design/`, not a seat in the walk. The Product
  owner and Owner presets map to real job keys and memberships; the Programme preset is the project
  office lens.
- The walk never infers a position, signs for a user, or pre-selects an approval. It presents the
  server's gate and the signed-in principal's own choice, exactly as §10.5.4 requires.

## 4. Information architecture and routing

**Route parity by construction.** Every `apps/web` path has a walk path formed by the `/walk`
prefix: `/portfolio` → `/walk/portfolio`, `/board/:board_id` → `/walk/board/:board_id`,
`/design/adm` → `/walk/design/adm`. `/walk/` is the entrance (the app's `/`). This makes the parity
instrument a set comparison and the old-route redirect map mechanical.

| Wing | Rooms (destination routes, walk-prefixed) | Read path (families) | Write path (families) | Spec |
|---|---|---|---|---|
| Portfolio & plan | `/walk/portfolio`, `/walk/portfolio/:programme_id`, `/walk/programme`, `/walk/plan`, `/walk/validation`, `/walk/execution`, `/walk/decisions`, `/walk/runs`, `/walk/runs/:run_id`, `/walk/meetings`, `/walk/meetings/:id`, `/walk/` (Today) | portfolio read models, `programmes.*`, `benefits.*`, `tranches.*`, `wbs.*`, `baselines.*`, `registers.*`, `meetings.*`, `approvals.*`, `runs.*`, `metrics.programme` | `programmes.*`, `benefits.*`, `tranches.*`, `wbs.*`, `baselines.take`, `registers.*`, `meetings.*`, `approvals.decide`, `runs.cancel` | `portfolio`, `programme`, `plan`, `validation`, `execution`, `meetings`, `run-queue`, `run-detail` |
| Organisation | `/walk/organisation`, `/walk/jobs`, `/walk/agents`, `/walk/tools`, `/walk/project-types` | `industries.list`, `departments.list`, `jobs.*`, `job_releases.*`, `job_bindings.*`, `approval_policies.*`, `decision_policies.*`, `refinements.*`, `laya_configs.*`, `ladder.list`, `tools.*`, `toolkits.*`, `tool_bindings.*`, `tool_calls.*` | the create/update/release/retire verbs those families expose; `tools.probe|call|publish|retire`, `toolkits.*`, `tool_bindings.*`, `tool_authoring.*` | `organisation`, `jobs`, `agents`, `tools`, `project-types` |
| Procedures & authoring | `/walk/procedures`, `/walk/build/processes`, `/walk/build/processes/:process_id`, `/walk/build/process-map`, `/walk/build/process-map/:map_id`, `/walk/authoring` | `procedures.*`, `capabilities.*`, `processes.*`, `approvals.list`, `authoring.assets` | `procedures.*`, `procedures.ingest_*`, `authoring.*` | `procedures`, `processes`, `process-map`, `authoring` |
| Govern & design | `/walk/design/adm`, `/walk/architecture`, `/walk/design/requirements`, `/walk/design/requirements/:id`, `/walk/design/models`, `/walk/design/models/:model_id`, `/walk/copilot` | `architecture.*`, `repository_search`, `contracts_list`, `profile_get`, requirements/models reads | `artifacts_*`, `contracts_*`, `phases_advance`, `reviews_*`, `board_*` | `adm`, `trace-architecture`, `requirements`, `editor`, `copilot` |
| Knowledge & memory | `/walk/knowledge` | the knowledge/memory reads of the owning slice; the route renders **not yet live** until they exist | the owning slice's writes | `knowledge` |
| Measure & improve | `/walk/improve`, `/walk/evals`, `/walk/evals/:eval_id`, `/walk/evals/benchmarks`, `/walk/evals/benchmarks/run/:run_id`, `/walk/measure/usage`, `/walk/measure/prompt-audit`, `/walk/metering` | coach queue, `kata.*`, lessons, evals/benchmarks reads, metering reads | `coach.*`, `kata.*`, `lessons.*` | `improve`, `evals`, `benchmarks`, `inference-usage`, `prompt-audit`, `metering` |
| Operations | `/walk/operations` | incidents, runbooks, on-call reads | `ops.*` (gated) | `operations` |
| Utilities | `/walk/search`, `/walk/platform`, `/walk/settings/members`, `/walk/settings/roles`, `/walk/settings/inference`, `/walk/settings/admin`, `/walk/approvals` | the corresponding reads | the corresponding writes | `search`, `platform`, `members`, `roles`, `inference`, `admin`, `shell` |

The table is the map, not a substitute for §10.5's contracts: the read/write families, states and
refusals are §10.5.2's and the owning slice's. A destination whose slice is not built renders the
same **not yet live** gate as `apps/web`, naming the missing action or slice, and keeps its route.

**Routing rules**

- Clean URLs (`/walk/<app path>`), HTML5 history. The server must serve `/walk/index.html` for a
  non-asset path under `/walk/` (see §10); without that change a walk deep link falls through to the
  operator app's `index.html`.
- Query strings are preserved and are part of the deep link. A sheet that represents a record uses a
  path or query the server can be linked to; a purely visual camera state is never a URL requirement.
- Back/forward restores the room and the sheet. Closing a sheet returns focus to its opener.
- A signed-out deep link is a sign-in gate that returns to the same path.

## 5. Data and action contract

The walk is a contract-faithful client of the existing server; it invents nothing.

- **Manifest.** `GET /api/actions` returns the role-filtered manifest (`name`, `version`, `kind`,
  `spec_hash`, `scopes`, `idempotent`, `timeout_ms`). The walk caches it per session and renders a
  control only when the action is present. A manifest refresh after a 403 is allowed; a control is
  never shown on a guess.
- **Describe.** `actions.describe {name}` returns `input` and `output` JSON Schema plus `effect`
  (`read | write | destructive`), `confirm` (`none | card | card-typed`), `tenancy`, `spec_hash` and
  the scope list. This is what makes a generic action sheet possible: the form is generated from the
  schema, and the effect/confirm/tenancy drive the confirmation and approval UX.
- **Reads.** Literal `GET` routes only (the §10.5.2 read paths), with `next_cursor` pagination.
  No GraphQL, no ad-hoc read endpoint, no published snapshot as the app's source.
- **Writes.** `POST /api/actions/{name}/invocations` with `{"input": …}` and, for a destructive
  call, `{"input": …, "approval_id": "…"}`. Headers: `Authorization: Bearer`, `X-Active-Tenant`,
  `If-Match: <version>` on an update, `Idempotency-Key` on a create. No optimistic mutation: after a
  write the screen refetches its read model.
- **Errors.** RFC 9457 `application/problem+json` mapped by status: 401 sign-in, 403 permission,
  404 absent, 409 conflict/gate, 412 precondition (refetch; never guess a new version), 422 invalid
  (field errors shown verbatim), 5xx with `request_id`. The server's refusal is shown, not replaced
  by a client rule.
- **Dual control.** A gated decision names its approval; the walk never decides on the principal's
  behalf and never auto-acquires an approval. The approval and meeting queues are read views; the
  act is the principal's.
- **Live updates.** SSE topics (board, card, run, approval) via `fetch` + `ReadableStream` because
  `EventSource` cannot carry the bearer. A screen without a topic polls and says so. The stream is
  cancelled on navigation.
- **Cancellation.** Every screen load is abortable; a slow response for a left screen is discarded.

## 6. Client architecture

### 6.1 Modules

```
apps/walk/
  index.html
  src/main.js                 bootstrap: config, auth, manifest, router, shell, world
  src/auth/                   config load, PKCE, dev login, token in memory, /api/me
  src/api/                    fetch client, problem mapping, manifest cache, describe cache, SSE
  src/forms/                  JSON Schema -> fields; validation; unsupported schema fallback
  src/router/registry.js      composes the per-wing route files; holds no destination rows
  src/router/routes/          one file per delivery sibling:
    core.js                     shell, entrance, Portfolio (the parent's exemplar)
    portfolio-organisation.js   S1's rows
    procedures-governance.js    S2's rows
    measure-operations-utilities.js  S3's rows
    v1.js                       S4's retained V1 rows
  src/shell/                  HUD, palette, map, notifications, sheet host, toasts
  src/world/port.js           the WorldView interface and scene types (JSDoc)
  src/world/canvas2d/         baseline renderer: projection, depth sort, collision, figures
  src/world/bevy/             optional candidate: wasm loader + input/overlay bridge
  src/destinations/           one module per destination; read views + curated flows
  src/state/                  session store, per-screen stores, per-viewer prefs
  src/a11y/                   focus management, skip link, shortcuts, announcements
  e2e/<wing>/                 one Playwright spec per destination + shell/auth/parity
  dev-server.mjs              static + same-origin proxy to :8930
  playwright.config.ts
```

Each route file exports an array of `{walk, app, room, spec, load}` rows, where `load` is a lazy
`import()` of the destination module; `registry.js` concatenates the rows in a fixed order. The
parent creates all five files as empty arrays, so a delivery sibling adds rows to its own file and
never edits a shared one.

Dependency rule: `destinations` may import `api`, `forms`, `shell` and `router`; `shell` may import
`api`, `router`, `world/port`; `world/*` imports only `world/port` and pure helpers. No destination
imports another destination. A route file may lazily load only its own wing's destination
modules, never another wing's. No engine module fetches, decides or stores authority.

**Shared seams** (parent-owned; siblings consume, never edit):

| Module | Responsibility | Minimum API |
|---|---|---|
| `auth` | PKCE, dev login, token, `/api/me` | `signIn()`, `signOut()`, `token()`, `principal()` |
| `api` | same-origin fetch, headers, problems | `request(path, opts)`, `invoke(name, input, opts)` |
| `manifest` | role-filtered action set | `load()`, `has(name)`, `scopes(name)` |
| `forms` | JSON Schema → fields | `render(schema, value) -> {element, value, errors}` |
| `router` | path ↔ room/sheet, history | `navigate(path)`, `current()`, `onChange(fn)` |
| `shell/sheet-host` | sheet lifecycle, focus | `open(spec)`, `close()`, `toast(problem)` |
| `world/port` | engine-neutral scene | `mount`, `setScene`, `setCamera`, `setHighlight`, `on`, `resize`, `dispose` |
| `sse` | token-bearing stream | `subscribe(topic, fn) -> unsubscribe` |
| `state` | session + per-viewer prefs | `session`, `prefs`, `subscribe(fn)` |

The exact parameter objects are the implementation's; the names and the responsibilities are not.
`src/api` owns the header set (`Authorization`, `X-Active-Tenant`, `If-Match`, `Idempotency-Key`,
`approval_id`) and the problem mapping; a destination never constructs them.

### 6.2 The `WorldView` port

The world is replaceable. The contract (JSDoc-typed; the exact names are the implementation's):

- `mount(container, options): Promise<void>`
- `setScene(scene)` — rooms, doors, figures, state badges; derived from read models by the shell.
- `setCamera({mode, focus, zoom})` — first/third, room focus, zoom.
- `setHighlight({roomId, recordId})` — selection from a sheet or the palette.
- `on(event, handler)` — `enter-room`, `select`, `hover`, `escape`; returns an unsubscribe.
- `resize()`, `dispose()`.

Constraints for every implementation: no text input, no tables, no long-form content; all scene text
is a short label duplicated in the DOM; the canvas is labelled and never traps focus; a pure-DOM
map/list view exposes the same rooms and records; no network access; no authority.

### 6.3 Renderer candidates

- **Baseline — canvas 2D pseudo-3D.** The mock's approach: a hand-written projection, painter's
  depth sort, flat-shaded faces, local textures and procedural figures. It has no engine dependency,
  no WebGL requirement and a small payload. It is the default until ADR 0022 says otherwise.
- **Candidate — Bevy 0.18, WASM.** The world as a Rust/WASM module behind the same port, loaded as
  a static asset. Use Bevy's `2d`/`ui` feature bundles rather than the default set; keep bevy_ui for
  labels only (DOM remains the work surface). The build runs on GitHub-hosted CI only (the owner's
  no-local-Rust rule). If selected, ADR 0022 records the measured reason and the bundle/startup
  budgets; if not, the canvas baseline stays and no Rust enters the client.

### 6.4 State

- **Server state** — fetched per screen, held in memory, refetched after a write; never written to
  storage.
- **Session** — token in memory; PKCE verifier in `sessionStorage`; tenant and membership in memory.
- **Per-viewer preferences** — last room, camera mode, seat preset, reduced-motion override and
  help-dismissed, under a `conveyor.walk.*` key in `localStorage`. Preferences are conveniences and
  the app renders correctly when storage is empty or throws.
- **No programme store.** The mock's `conveyor.gemba.v1` object (walls, cards, issues, decisions,
  reports) is not carried over. A client-side copy of programme state is a fixture by another name.

### 6.5 Sheets and forms

- A sheet is one of: record, list/table, form, confirmation, refusal, history. It has a title, a
  close control, focus capture and return, and a URL that can be reloaded.
- The schema form renderer maps JSON Schema to labelled fields (string, number, boolean, enum, array,
  object, date-time), shows required fields and descriptions, validates before sending where the
  schema allows, and always lets the server be the authority. An unsupported construct falls back to
  a labelled JSON editor with parse validation, so **no action is unreachable** because of a widget.
- A write shows the effect class: a `write` confirms once; a `destructive` action shows the approval
  requirement and, when the server refuses for a missing approval, the refusal verbatim.
- A 412 refetches the record and shows both versions' `version`; it never retries with a guessed
  `If-Match`.

## 7. Design system

The walk carries the mock's validated warm palette, embedded as tokens (no external stylesheet):

| Token | Value | Use |
|---|---|---|
| `--paper` / `--paper-2` / `--paper-3` | `#f8efd9` / `#fffaee` / `#efdfc0` | sheet grounds, highlights, recesses |
| `--wood` / `--wood-2` / `--wood-3` | `#6d4726` / `#8a5a30` / `#ab7744` | game bar, rails, controls |
| `--teal` / `--teal-2` / `--teal-3` | `#2f8070` / `#1f5e53` / `#4aa38f` | navigation, active states |
| `--plumb` / `--plumb-2` | `#8fd14f` / `#c4f07c` | the active figure, moodlets |
| `--amber` / `--amber-2` | `#e9a83f` / `#f7cd72` | focus, warnings |
| `--coral` | `#d96a5a` | refusal and destructive accents |
| `--ink` / `--muted` / `--line` | `#3b2d1e` / `#7b6851` / `#dcc8a1` | text and rules |

- **Type** — Fraunces for display headings (embedded WOFF2, SIL OFL, licence text retained), and a
  rounded system stack (`ui-rounded, "SF Pro Rounded", "Trebuchet MS", system-ui`) for UI text.
- **Shape** — radii 12–22 px, layered warm shadows with inset highlights, glossy pill buttons,
  parchment sheets with ribbon headers, stitched document borders, round wooden controls, amber
  focus rings.
- **Components** — game bar, play tabs, tools bar, identity card with moodlets, committee card, map,
  wall dock, sheet, seat list, agenda, case file, report document, minute, empty/loading/error/
  refusal panels, wall desk panels, cards, metric tiles, grid table, world overlays (location,
  keybar, pad).
- **Motion** — short easing, no parallax or spin when `prefers-reduced-motion: reduce`; the plumbob
  and camera animations are decorative only.
- **Density** — the game chrome yields to the sheet: at a narrow width or 200% zoom, the sheet is
  full-screen and the world is optional.

## 8. Accessibility and responsive behaviour

- Every destination, record, form and decision is reachable by keyboard alone; the skip link
  precedes the HUD; the palette is a keyboard path to any route and action.
- The canvas is `role="img"` with an accessible name and is not focus-trapped; a DOM map/list exposes
  the same rooms and records. No information exists only in pixels.
- Sheets manage focus (capture, escape, return), have labelled headings and form fields, and announce
  refusals with a live region. Tables have captions and header scopes.
- Contrast meets WCAG 2.2 AA for ink-on-paper and cream-on-wood; focus is always visible.
- 200% zoom and 390 px width: no horizontal page scroll, 16 px gutters, sheets full-screen, controls
  at least 44 px.
- Reduced motion is honoured; autoplay, audio and timed interactions are absent.

## 9. Performance and security

**Performance budgets** (provisional; ADR 0022 fixes any engine-specific number from the spike):

- First load of the shell + entrance: ≤ 250 KB gzip of JS/HTML/CSS, excluding an optional wasm
  engine; a destination module lazy-loads on first visit.
- World: 60 fps target on an M-series laptop at the reference scene; no frame over 50 ms after the
  first two; degrade labels/effects rather than drop input.
- Sheets: virtualise any table above 200 rows; one read per screen plus its stream, not per row.
- Memory: a 5-minute soak with a stream and repeated navigation stays within 1.5× the first-load
  heap; a leaked interval or subscription is a defect.
- If Bevy is selected, the release wasm and its cold interactive time are measured under the same
  protocol as the baseline; the engine is a lazy asset, never part of the sign-in path's critical
  budget.

**Security**

- Access token in memory only; PKCE verifier in `sessionStorage`; never a token in `localStorage`,
  a URL or a log. The walk follows `apps/web/src/auth.rs`'s model.
- Every server string is rendered as text; schema-generated labels and descriptions are escaped.
  No `innerHTML` of server data; no `eval`; no third-party script, font or image origins.
- The client is same-origin with the API; it sends no request to `/api` when signed out and holds no
  embedded credentials.
- A refusal is displayed with its action code and the server's message; a toast never claims a
  success the server did not return.
- The walk adds no CSP weakening. If a strict policy is introduced, it must work under it (no inline
  script injection, no remote assets).

## 10. Deployment and operational contract

- **Static sub-site.** `apps/walk`'s build output is copied to `/out/site/walk` in the image. The
  server already serves a directory with its own `index.html` as a sub-site (the `/design/` model).
- **Deep links.** `crates/api/src/web.rs` currently falls back to the site-root `index.html` for a
  non-asset path that is not a directory index, so `/walk/portfolio` would render the operator app.
  Add a scoped rule: for `GET`/`HEAD`, a path under `/walk/` with no file extension serves
  `/walk/index.html`. It never applies to `/api`, `/mcp`, health or metrics. A Rust test asserts the
  fallback and the 404 for a missing asset.
- **Local development.** `apps/walk/dev-server.mjs` serves the walk under `/walk/` on port 8932 and
  proxies `/api`, `/health`, `/config.json` and `/__dev/login` to `127.0.0.1:8930`, preserving the
  production path shape. Playwright uses `http://127.0.0.1:8932` as its base URL.
- **Identity.** The same issuer and public client; add the walk's redirect URI
  (`https://<host>/walk/`, and the dev origin's `/walk/`) to the client's valid redirect URIs. If the
  identity operator prefers isolation, register a dedicated public client and serve its id to the
  walk rather than widening the app's client. This is an external configuration prerequisite, not a
  repository change.
- **Configuration.** `/config.json` is served at the origin root; the walk fetches it absolutely and
  uses the same `issuer`, `client_id` and `dev_auth` fields.
- **Caching.** The entry document is not cached as a stale build (`no-store` or the site's existing
  behaviour); assets may be cached with a content hash or a version query. No new CDN or cache layer.
- **No new port in production.** The production entry is the existing origin at `/walk/`; port 8932
  is local development only. Port 8000 is never used.
- **If Bevy is selected.** The wasm build (`wasm32-unknown-unknown` + wasm-bindgen) runs in
  GitHub-hosted CI, never on a local machine; the output lands in the same static tree. The static
  contract above does not change.

## 11. Verification

- **Unit (`node:test`)** — projection, collision, depth sort, scene derivation, route mapping,
  schema-to-field mapping, problem mapping. Pure modules only; no DOM.
- **Type check** — `tsc --noEmit` over JSDoc-typed modules; a type error fails the walk workflow.
- **Playwright, one spec per destination under `e2e/<wing>/`** — empty, error, happy and one
  refusal (or one write where the destination owns a write), plus manifest gating (a control
  absent when its action is absent) and the headers on a write (`If-Match`, `Idempotency-Key`,
  `approval_id` where applicable).
- **Auth** — dev login, deep-link gate, sign-out, and a 401 path; the OIDC redirect is exercised in
  the P0 spike and in a deployed smoke where configured.
- **Parity instrument** — `scripts/build-walk-coverage.py` compares the frozen census, the composed
  route registry (`src/router/registry.js` fed by `src/router/routes/*.js`) and the walk specs; it
  emits `docs/evidence/walk-coverage.json` and exits non-zero on a missing destination, an expired
  exemption or a reachability gap. Exemptions live in `docs/evidence/walk-exemptions.json`. The
  census schema and gate are in `docs/plans/GEMBA-WALK.md` §6.
- **Accessibility** — a keyboard-only navigation spec; focus/name assertions; a manual screen-reader
  pass per wing; contrast checked from computed styles.
- **Performance** — the budgets in §9, measured in CI where stable and recorded in the plan's
  evidence otherwise; the engine protocol in §12 is separate and feeds ADR 0022.

## 12. The engine decision protocol (P0)

The P0 spike runs the same reference scene (six rooms, 12 figures, labels, one moving figure) on the
same machine class, cold cache, five runs, median reported with min–max, under 4× CPU throttling and
Slow 4G for load metrics. It records:

| Metric | Why it matters |
|---|---|
| release bytes (gzip) and request count | first-load budget; wasm is lazy but must be bounded |
| cold load → first interactive world (ms) | whether the walk opens like a page or like a game |
| steady frame p50/p95 and long tasks | whether the world is smooth at the reference scene |
| heap after 5 minutes and after 20 room changes | leaks decide long-session viability |
| browser requirements (WebGL2/WebGPU, fallback) | whether low-end and remote users are excluded |
| CI build time and reproducibility | the no-local-Rust rule makes CI the only build |
| DOM-overlay input/focus and screen-reader path | the engine must not own the work surface |

ADR 0022 records the winner and the budgets. The baseline is not deleted when Bevy wins; it remains
the fallback behind the port.

## 13. Parity gate (before any primacy claim)

The walk reaches parity only when all of these are true and recorded:

1. Every §10.5 destination and every retained V1 route in `apps/web/src/app.rs` has a walk route and
   a Playwright spec asserting empty, error, happy and one refusal or write.
2. Every action in the frozen census is present in the manifest path, describable, and renderable by
   the schema form (or its JSON fallback); every destination's named write flows round-trip with the
   correct headers and a refusal path.
3. The parity instrument reports no missing row and no expired exemption.
4. The accessibility and performance budgets are met and recorded.
5. The owner signs the result. Only then may the walk become the default entry point; `apps/web` is
   frozen, then retired under the P6 runbook, and the decision is reversible until the deletion.

## 14. Do-Not list

- Do not invent a REST endpoint, a read model or an action. Reads are literal `GET`s; writes are
  actions.
- Do not hardcode which actions exist. The manifest is the contract.
- Do not use the published snapshot, a seed, a fixture or `localStorage` as the app's programme
  state.
- Do not omit `Authorization`, `X-Active-Tenant`, `If-Match` or `Idempotency-Key`, or send a
  destructive action without its approval id.
- Do not render text entry, tables or long-form content in the canvas or the game UI.
- Do not make walking mandatory for a capability.
- Do not embed the design pack or copy its 4.7 MB payload; link to `/design/`.
- Do not add a table, an action, an event, a decision store or a SPEC_V2 §12 T-row for the walk.
- Do not infer, pre-select or auto-approve a position; the server's refusal is the UI's answer.
- Do not put a token in storage or a URL; do not add a third-party origin.
- Do not claim a destination is implemented from its mock page or its route alone.

## 15. Non-goals and revisit triggers

**Non-goals** — a second backend; a second decision store; native mobile apps; chat, audio or video;
customer care (D48); replacing `/design/`; SSR; a public unauthenticated mode in v1; pixel parity with
the mock.

**Revisit triggers** — the parity gate cannot be met without new backend surface; Bevy on the web
misses the budgets and the canvas baseline cannot close the gap; two-client maintenance exceeds the
value; the owner declines primacy at P6 (the walk stays an alternative client); the operator app's
own architecture changes enough that the shared contract or the census must be re-baselined.
