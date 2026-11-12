---
slice: S01
title: UI spike — Leptos CSR vs Leptos SSR
status: planned
depends_on: []
spec_sections: ["§3 D5", "§5.2", "§5.7", "§6", "§7", "§8 S1"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: PENDING-S0
---

# S01 — UI spike: Leptos CSR (WASM) vs Leptos SSR

Programme project: S01 · issues none yet

**Executor:** a fresh session, verbatim · **Gate (SPEC §8 S1):** the decision is recorded with the measured numbers.

## 1. Objective

Decide by measurement whether S12 builds `conveyor`'s UI as Leptos CSR/WASM or Leptos SSR, and record it as an ADR. Evidence: eight numbers per stack from building both hard screens twice. Nothing else survives.

SPEC §5.7, verbatim: "**Leptos CSR is the prior** — the app is authenticated with no SEO need, so SSR's win is weak and SSR hydration/double-fetch bugs are the known trap. SSR is chosen only if measured TTI exceeds 2 s or the auth flow forces cookie-based SSR. Dioxus only if the phase-2 flow editor forces it." Corollary this gate enforces: **SSR must beat CSR on TTI or on auth; a tie, or a loss on both, is CSR.**

## 2. Starting state

- `conveyor` is docs-only. Measured 2026-11-07: the root holds `docs/` only — no `Cargo.toml`, `crates/`, `apps/`, git history. SPEC §5.2 names `apps/web`; this slice leaves it unpopulated.
- No Rust UI precedent anywhere. `agent`, `strata_ingest`, `strata_rag`, `template-rust-service` are headless HTTP services; both frontends are SvelteKit (fr: SvelteKit 2.70, 195 `+page.svelte`).
- Measured 2026-11-07: `rustup target list --installed` lists only `aarch64-apple-darwin`; `cargo-leptos` and `trunk` are absent and `cargo leptos` exits `no such command`. Ports 8791/8801/8802 are free. `cargo`/`rustc` are not on PATH without `source ~/.cargo/env`; 1.98.0 is the default, 1.99.0 is installed.

## 3. Scope

**IN**

- One fixture server (axum, ≤200 lines, no DB/docker) serving both screens.
- **Board:** 500 cards in 10 lists; drag between lists (pointer + keyboard); an SSE move applied without losing scroll or an in-flight drag.
- **Chat:** SSE token streaming, a tool-call card (name, arguments, collapsed result), 50-message history.
- Both screens **twice**: Leptos CSR (WASM, `hydrate`) and Leptos SSR (`leptos_axum`, render + hydration).
- Minimum auth: auth-code + PKCE against a local fixture OP (CSR token in memory; SSR HttpOnly cookie guard).
- `spikes/web/RESULTS.md`, `docs/decisions/0001-ui-stack.md`.

**OUT** — real Keycloak and the `conveyor` realm (S2, D13); authz beyond one logged-in gate; styling fidelity and layout below 1280 px; every other screen (S12); i18n; adopting any DnD or virtual-list library; any change to the workspace, `apps/web` or `crates/*`.

## 4. Source material

Read for requirements, not to port. fr pinned at `6f7c509792b3ca1fc1fb00c198aebcec83f0af29` (2026-11-07); `template-rust-service` at `0c7396a0130b3ed27904c03cce49d5415648f66f`. All paths verified 2026-11-07.

| path | read this | carry | do not carry |
|---|---|---|---|
| `fr/frontend/src/routes/(project)/projects/[id]/board/+page.svelte` | column layout, DnD, virtualisation | 10 columns, drag targets, live move | the Svelte DnD library, Svelte stores |
| `fr/frontend/src/routes/(project)/projects/[id]/cards/[cardId]/` | the cockpit's card fields | the field list | the Svelte components, fr API client |
| `fr/frontend/src/lib/components/ProjectForm.svelte` | form state, validation, errors | a validated-form shape | Svelte bindings, Zod schemas |
| `fr/frontend/src/routes/chat/+page.svelte`, `.../chat/streamChat.ts` | SSE loop, message model, tool calls | event names, ordering, history | SvelteKit load/auth code |
| SPEC §6 | list is state, no `state` column | the board model the fixture mirrors | — (requirement, not code) |
| `plans/02-source-map/10-agent-patterns.md` §template-rust-service | `OpenApiRouter`, `AppState` | axum wiring behind SSR (not sqlx) | sqlx, the template DB layer |

## 5. Deliverables

```
spikes/web/                 # throwaway, NOT a workspace member
  Cargo.toml                # empty [workspace] table — opts out of any parent
  fixture/src/main.rs       # ≤200 lines, axum only, port 8791
  csr/                      # Leptos CSR, wasm32-unknown-unknown, static, :8801
  ssr/                      # Leptos SSR + leptos_axum, :8802
  RESULTS.md                # 8 metrics × 2 stacks + the command that produced each
docs/decisions/0001-ui-stack.md
```

Fixture contract — deterministic, DB-free, `id:` monotonic:

| route | returns |
|---|---|
| `GET /api/boards/demo` | `{lists:[10], cards:[500]}`; card `{id, list_id, title, position, version}` |
| `GET /api/events?board=demo` | `text/event-stream`; `event: card.moved` every **2 s**; honours `Last-Event-ID` |
| `GET /api/chat/history` · `/api/chat/stream` | 50 messages · `token` ×80 at 25 ms, then `tool_call`, then `done` |
| `GET /health` | `ok` |

ADR skeleton (the only non-command block, per the brief): `# 0001 — UI stack` · Status/Date · `## Context` · `## Options` (CSR, SSR) · `## Measured` (8 rows × 2 stacks) · `## Decision` · `## Consequences` · `## Revisit triggers`.

## 6. Build order

Each step ≤1 day, each independently testable.

1. Toolchain and ports: `source ~/.cargo/env`; measure `rustc --version`, `rustup target list --installed`, `command -v cargo-leptos trunk`, `cargo leptos --help`, and `lsof -nP -iTCP:8791 -iTCP:8801 -iTCP:8802 -sTCP:LISTEN` (empty). Never 8000.
2. Scaffold `spikes/web/Cargo.toml` with the empty `[workspace]` and three packages; pin `leptos`, `leptos_axum`, `leptos_router` to one minor in `[workspace.dependencies]`.
3. Install the missing toolchain — `wasm32-unknown-unknown` and `cargo-leptos` — then confirm `cargo leptos --help` runs.
4. Fixture board endpoints: `GET /api/boards/demo` returns 500 cards; `GET /api/events` emits monotonic `id:` 2 s apart.
5. Fixture chat endpoints: 50-message history, 80 tokens then `tool_call` then `done`, and `/health`.
6. Board, CSR: 500 cards from the fixture, unstyled.
7. Board, CSR: drag between lists with pointer and keyboard; the move persists in fixture state.
8. Board, SSR: same component tree; count board fetches per page load in the server log and record the double-fetch diff.
9. Chat, CSR: streaming tokens, tool-call card, history.
10. Chat, SSR: the same, server-rendered then hydrated.
11. Auth fixture OP: `/authorize` code + `/token` PKCE S256.
12. Auth wiring per stack: CSR bearer in memory, SSR HttpOnly cookie + CSRF; log out/in 3× and deep-link while logged out.
13. Measurement: five runs per stack per §7; write `RESULTS.md` as you go.
14. ADR from `RESULTS.md`: splice its 8 × 2 table into `## Measured`, then delete the spike; confirm `apps/web` is still absent.

Commands for steps 2–14 (from `$HOME/Movies/conveyor`):

```bash
# 2 — three packages; the empty [workspace] opts out of any parent
mkdir -p spikes/web/{fixture,csr,ssr}/src && printf '[workspace]\n[workspace.dependencies]\n' > spikes/web/Cargo.toml \
  && for p in fixture csr ssr; do printf '[package]\nname = "spike-%s"\nversion = "0.1.0"\nedition = "2024"\n' "$p" > spikes/web/$p/Cargo.toml; done
# 3 — install (wasm target and cargo-leptos were both absent 2026-11-07)
source ~/.cargo/env && rustup target add wasm32-unknown-unknown && cargo install cargo-leptos --locked && cargo leptos --help | head -5
# 4 — board fixture (:8791)
source ~/.cargo/env && cargo run -p spike-fixture & sleep 2; curl -s http://127.0.0.1:8791/api/boards/demo | jq '.cards|length'; curl -sN 'http://127.0.0.1:8791/api/events?board=demo' | head -5
# 5 — chat fixture
curl -s http://127.0.0.1:8791/api/chat/history | jq 'length'; curl -sN 'http://127.0.0.1:8791/api/chat/stream' | head -5; curl -s http://127.0.0.1:8791/health
# 6 — CSR build and serve (:8801) from its own target dir; the CSR site is static, so a file server is right
source ~/.cargo/env && CARGO_TARGET_DIR=/tmp/s1-csr cargo leptos build --release -p spike-csr \
  && (cd /tmp/s1-csr/site && python3 -m http.server 8801 &) && sleep 2 \
  && npx -y playwright@1.49.1 screenshot --wait-for-selector='[data-card-id]' http://127.0.0.1:8801/ /tmp/s1-board-csr.png
# 7 — render smoke; the drag/keyboard assertion is the RESULTS row
npx -y playwright@1.49.1 screenshot --wait-for-selector='[data-card-id]' --wait-for-timeout=2000 http://127.0.0.1:8801/ /tmp/s1-board-drag.png
# 8 — SSR build and serve (:8802) from its own target dir; python3 -m http.server would return the shell,
#     never the server-rendered HTML (grep counts 0), so the SSR app runs the bin the build emits
#     (bin/site paths under the target dir are UNVERIFIED until built: ls /tmp/s1-csr/site /tmp/s1-ssr/site /tmp/s1-ssr/release)
source ~/.cargo/env && CARGO_TARGET_DIR=/tmp/s1-ssr cargo leptos build --release -p spike-ssr \
  && (LEPTOS_SITE_ROOT=/tmp/s1-ssr/site LEPTOS_SITE_ADDR=127.0.0.1:8802 /tmp/s1-ssr/release/spike-ssr &) && sleep 2 \
  && curl -sf http://127.0.0.1:8802/ | grep -o 'data-card-id' | wc -l
# 9 — chat CSR
npx -y playwright@1.49.1 screenshot --wait-for-selector='[data-message-id]' http://127.0.0.1:8801/chat /tmp/s1-chat-csr.png
# 10 — chat SSR
curl -sf http://127.0.0.1:8802/chat | grep -o 'data-message-id' | wc -l
# 11 — fixture OP: expect 302
curl -s -o /dev/null -w '%{http_code}\n' 'http://127.0.0.1:8791/authorize?response_type=code&client_id=s1&redirect_uri=http://127.0.0.1:8801/callback&code_challenge=x&code_challenge_method=S256'
# 12 — logged-out deep link: expect the login gate
npx -y playwright@1.49.1 screenshot --wait-for-selector='[data-login]' http://127.0.0.1:8801/projects/demo/board /tmp/s1-logged-out.png
# 13 — one timed run; repeat five times per stack
npx -y lighthouse@12 http://127.0.0.1:8801/ --only-categories=performance --throttling-method=simulate --throttling.cpuSlowdownMultiplier=4 --throttling.requestLatencyMs=150 --throttling.downloadThroughputKbps=1638.4 --throttling.uploadThroughputKbps=675 --output=json --output-path=/tmp/s1-csr-lh-1.json --chrome-flags='--headless=new'
# 14 — the ADR's ## Measured absorbs RESULTS.md's table BEFORE the spike is deleted
test -f docs/decisions/0001-ui-stack.md && grep -q '^## Decision' docs/decisions/0001-ui-stack.md \
  && { sed -n '1,/^## Measured$/p' docs/decisions/0001-ui-stack.md; echo; grep '^|' spikes/web/RESULTS.md; echo; sed -n '/^## Decision$/,$p' docs/decisions/0001-ui-stack.md; } > /tmp/s1-adr.md \
  && test "$(grep -cE '^\| *[1-8] ' /tmp/s1-adr.md)" -eq 16 \
  && mv /tmp/s1-adr.md docs/decisions/0001-ui-stack.md \
  && rm -rf spikes && test ! -e spikes && test ! -e apps/web && echo S1-DONE
```

## 7. Gate

Executable; all must be true. The numbers the gate cites come from this measurement protocol: five runs, median plus min/max, same machine, same fixture, servers fresh per run. Browser work takes the `browser-testing` lease; one browser at a time. **Variables used below:** `$i` is the run index (`for i in 1 2 3 4 5`); `<stack>` is the spike package (`spike-csr` or `spike-ssr`), what `-p <stack>` names and the `s1-cold-<stack>` target dir encodes; `<target>` is the stack's step 6/8 build dir (`/tmp/s1-csr` or `/tmp/s1-ssr`).

| # | metric | how | threshold |
|---|---|---|---|
| 1 | **TTI, throttled** | `npx -y lighthouse@12 http://127.0.0.1:8801/ --only-categories=performance --throttling-method=simulate --throttling.cpuSlowdownMultiplier=4 --throttling.requestLatencyMs=150 --throttling.downloadThroughputKbps=1638.4 --throttling.uploadThroughputKbps=675 --output=json --output-path=/tmp/s1-csr-lh-$i.json --chrome-flags='--headless=new'`. Decision number is interaction TTI: navigationStart → first drag that mutates membership (`performance.now()` in the drag handler). Lighthouse `interactive` may be `null` when WASM boot leaves no quiet window. | **>2000 ms fails CSR** |
| 2 | **Bundle** | `find <target>/site -type f \( -name '*.wasm' -o -name '*.js' \) -exec sh -c 'gzip -9 -c "$1" \| wc -c' _ {} \; \| awk '{s+=$1} END {print s}'` | gzip-9 bytes; raw too |
| 3 | **500-card first render** | navigationStart → 500th `[data-card-id]`, p50 of 5 | all-at-once (decision row) + virtualised (sub-row), both stacks |
| 4 | **Drag frames** | 2 s CDP drag; rAF deltas + `longtask` | pass = ≥95 % ≤16.7 ms, 0 tasks >50 ms; record p95 |
| 5 | **SSE → DOM** | fixture stamps T at send; app marks `sse.<id>` on message, `sse-dom.<id>` in a MutationObserver; 30 events | p50/p95; p95 ≤100 ms |
| 6 | **Memory, 5 min** | CDP `Performance.getMetrics` (JSHeapUsedSize) + `Memory.getDOMCounters` | record MB + nodes; >300 MB or >3× card nodes is a signal |
| 7 | **Cold build** | `source ~/.cargo/env && time CARGO_TARGET_DIR=/tmp/s1-cold-<stack> cargo leptos build --release -p <stack>` from empty | record; >10 min is a loop warning |
| 8 | **Auth** | flow shape, round trips, deep-link while logged out, whether SSE needs a fetch-stream shim (`EventSource` cannot send `Authorization`) | pass / **forcing condition**, not a race |

```bash
set -euo pipefail
cd $HOME/Movies/conveyor
test "$(grep -cE '^\| *[1-8] ' docs/decisions/0001-ui-stack.md)" = 16   # 16: 8 metrics × 2 stacks
test -f docs/decisions/0001-ui-stack.md && grep -q '^## Decision' docs/decisions/0001-ui-stack.md
grep -qE '^## (Measured|Consequences)' docs/decisions/0001-ui-stack.md
test ! -e spikes && echo SPIKE-GONE
test ! -e apps/web && echo APPS-WEB-STILL-EMPTY
test "$(find . -path '*spikes*' | wc -l)" -eq 0                      # 0
test -z "$(git status --short 2>/dev/null | grep spikes || true)"      # 0 (repo may not exist yet, CONTRACT §13)
test -z "$(lsof -nP -iTCP:8791 -iTCP:8801 -iTCP:8802 -sTCP:LISTEN || true)"   # empty
```

Plus: an SSR decision must quote a TTI >2000 ms or the auth forcing condition, citing the `## Measured` row (which
absorbed `RESULTS.md` before the spike was deleted — step 14). Ambiguous numbers mean repeat the spike, not adjudicate.

## 8. Traps

1. **SSR hydration double-fetch.** A resource awaited in a component body runs on the server *and* again on hydration. Count board fetches per page load: 1 correct, 2 is the trap. Record raw, then fixed; say which number reached the ADR.
2. **Auth cookie and the SSE edge.** SSR needs HttpOnly cookie + session + CSRF; CSR/PKCE has no cookie, and `EventSource` cannot set `Authorization`, so CSR needs a fetch+`ReadableStream` shim — measure it as work, not as a CSR failure. A BFF for CSR auth is SSR with extra steps.
3. **WASM size drives TTI.** Measure `--release` + `wasm-opt`, never a dev build (~10× larger); serve built `dist/` from a static server — `trunk serve` injects live-reload and distorts both. Report gzip-9.
4. **Version triad and axum wiring.** `leptos`/`leptos_axum`/`leptos_router` must be one minor, or `leptos_routes`/`LeptosOptions` types mismatch into an opaque error; pin once in `[workspace.dependencies]`.
5. **Fake SSE.** A precomputed list with a sleep is honest; polling or cached replay is not. Prove pacing with `curl -sN` (monotonic `id:`, 2 s apart) and the DOM change with a MutationObserver count; "the handler fired" is not evidence.
6. **TTI on unthrottled localhost.** A 40 ms load measures the machine. Always 4× CPU / Slow 4G, throttle values printed beside the number.
7. **All-at-once vs virtualised.** Eager 500-card render beats windowing, so eager-CSR vs windowed-SSR wins the wrong race. Both stacks get both modes; decision row is all-at-once.
8. **Spike joins the workspace.** S2's root `Cargo.toml` auto-detects a nested crate without its own `[workspace]` (join or "multiple workspace roots"). Put the empty table in; still delete the spike.
9. **SSR compiles twice.** `ssr` on the server, `hydrate` in WASM; server-only deps (axum, sqlx, tokio) must sit behind `#[cfg(feature = "ssr")]`.

## 9. Decisions closed here

| id | decision | rationale |
|---|---|---|
| S1-D1 | The ADR is the only durable artifact; the spike is deleted | CONTRACT §11 |
| S1-D2 | All-at-once 500-card render is the decision configuration; virtualised is fallback | worst case for both |
| S1-D3 | TTI is the interaction number; Lighthouse `interactive` is supplementary | WASM boot can leave no quiet window |
| S1-D4 | Auth is a pass/forcing-condition, not a latency race | the rule names auth as a forcing condition |
| S1-D5 | Dioxus is out of scope | SPEC §5.7 admits it only for the phase-2 editor |

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Rust DnD library, or hand-rolled pointer events + keyboard? | hand-rolled, no JS interop |
| 2 | Fallback virtual list: hand-rolled windowing or JS via `wasm-bindgen`? | hand-rolled |
| 3 | Does the fixture OP prove enough for Keycloak? | yes for mechanics; the real realm is S2's gate |
| 4 | Is `cargo-leptos` acceptable in CI? | yes; if cold build >10 min, record `trunk` + manual axum wiring and let S2 decide |
| 5 | Does SSR need an SSE relay? | no; the browser connects to `/api/events` directly in both |
