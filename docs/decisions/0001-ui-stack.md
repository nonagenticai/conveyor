# 0001 — UI stack

Status: Accepted · Date: 2026-11-08 · Slice: S01 (`spikes/web`, deleted after this ADR)

## Context

`conveyor` needs a Rust web UI. SPEC §5.7 sets **Leptos CSR as the prior**: the app is authenticated
and has no SEO requirement, so SSR's win is weak, while SSR hydration and double-fetch bugs are the
known trap. SSR is chosen only if measured TTI exceeds 2 s or the auth flow forces cookie-based SSR;
a tie, or a loss on both, is CSR. Dioxus is out of scope (admitted only if the phase-2 flow editor
forces it).

Both stacks were built from **one component tree** (`spikes/web/shared/ui.rs`, included by both
crates) against one deterministic fixture (axum, `:8791`: 500 cards in 10 lists, `card.moved` SSE
every 2 s, 50-message chat history, 80-token stream at 25 ms, fixture OP with auth-code + PKCE S256).
Two hard screens: the board (pointer + keyboard drag between lists, live SSE moves) and the chat
(streaming tokens, tool-call card with collapsed result). Leptos `0.9.0-rc1` (leptos / leptos_axum /
leptos_router pinned to that one minor), cargo-leptos 0.3.11, release + wasm-opt, served from the
built output (`/tmp/s1-csr/site`, and the built SSR binary on `:8802`).

Measurement conditions: 5 runs per stack for metrics 1–4 and 8; metrics 5–7 carry their own n in the row (n=1, not measured, n=2), because the coordinator time-box closed before their full sets. Median with min–max, fresh fixture per run. Every browser
run was throttled with the same values the Lighthouse flags name — 4× CPU slowdown and Slow 4G
(150 ms latency, 1638.4 kbps down, 675 kbps up) — via CDP `Emulation.setCPUThrottlingRate` and
`Network.emulateNetworkConditions`; no number below is an unthrottled localhost load. Browser:
Google Chrome 156 driven by Playwright 1.49.1 (`channel: 'chrome'`; the Playwright-pinned Chromium
download was incomplete on this box — noted, same engine family). Lighthouse 12 with the §7 flags
supplies the supplementary `interactive` number.

## Options

| option | shape | auth |
|---|---|---|
| CSR | Leptos `csr` build: static `index.html` + WASM, mount on load, no server render | auth-code + PKCE S256, access token held in memory, login gate on the deep link; an authenticated SSE needs a fetch + `ReadableStream` shim because `EventSource` cannot send `Authorization` |
| SSR | `leptos_axum`: server render of the same tree, then WASM hydration | server-side code exchange, HttpOnly session cookie + CSRF double-submit; `EventSource` sends the cookie same-origin, no shim |

## Measured

| # | metric | stack | value (median; min–max, n=5 unless stated) | command that produced it |
|---|---|---|---|---|
| 1 | interaction TTI (ms), throttled | CSR | 5426 (5408–5628); Lighthouse `interactive` 5703 (5553–5747), FCP 751 | `node /tmp/s1-tools/measure.js http://127.0.0.1:8801 tti <i>`; `npx -y lighthouse@12 http://127.0.0.1:8801/ --only-categories=performance --throttling-method=simulate --throttling.cpuSlowdownMultiplier=4 --throttling.requestLatencyMs=150 --throttling.downloadThroughputKbps=1638.4 --throttling.uploadThroughputKbps=675 --output=json --chrome-flags='--headless=new'` |
| 1 | interaction TTI (ms), throttled | SSR | 5736 (5705–5758); Lighthouse `interactive` 5855 (5853–5865), FCP 1655 | same two commands with `http://127.0.0.1:8802` |
| 2 | bundle, gzip-9 wasm+js (bytes) | CSR | 290723 (raw 809797) | `find /tmp/s1-csr/site -type f \( -name '*.wasm' -o -name '*.js' \) -exec sh -c 'gzip -9 -c "$1" \| wc -c' _ {} \; \| awk '{s+=$1} END {print s}'` |
| 2 | bundle, gzip-9 wasm+js (bytes) | SSR | 301017 (raw 844826); server binary 9315696, not shipped to the browser | same `find` over `/tmp/s1-ssr/site` |
| 3 | 500-card first render (ms), all-at-once / virtualised | CSR | 4726 (4724–4736) / 4726 (4714–4728), 240 cards in virtual mode | `node /tmp/s1-tools/measure.js http://127.0.0.1:8801 firstrender <i>` and `... virtual <i>` (reads `window.__s1.card500` / `virtualReady` set by a MutationObserver) |
| 3 | 500-card first render (ms), all-at-once / virtualised | SSR | 1444 (1441–1452) / 945 (901–948), 240 cards in virtual mode | same commands with `http://127.0.0.1:8802` (virtual via `/virt`) |
| 4 | drag frames, 2 s pointer drag | CSR | p95 16.7 ms, max 16.8, 80.0 % ≤16.7 (75.5–82.7), 0 long tasks >50 ms, 245–254 frames | `node /tmp/s1-tools/measure.js http://127.0.0.1:8801 drag <i>` (rAF deltas + `longtask` PerformanceObserver) |
| 4 | drag frames, 2 s pointer drag | SSR | p95 16.8 ms, max 16.8, 76.5 % ≤16.7 (71.8–80.4), 0 long tasks >50 ms, 240–243 frames | same command with `http://127.0.0.1:8802` |
| 5 | SSE → DOM (ms), 30 events | CSR | 1 run only, not the 5-run median: p50 1.0, p95 1.8 (n=30 events). The 5-run set did not complete inside the coordinator time-box — the first batch read `__s1.sse[id]` while the app marks the send time at `__s1["sse.<id>"]`, so it returned n=0 pairs; the harness was fixed and one run captured before closure | `node /tmp/s1-tools/measure.js http://127.0.0.1:8801 sse <i>` (fixture `ts` → `sse.<id>` → MutationObserver `sseDom.<id>`) |
| 5 | SSE → DOM (ms), 30 events | SSR | not measured: same harness key-path bug; no SSR runs completed inside the time-box | same command with `http://127.0.0.1:8802` |
| 6 | memory, 5 min soak | CSR | not measured: 5-minute profile skipped by coordinator time-box after the decision measurements closed | `node /tmp/s1-tools/measure.js http://127.0.0.1:8801 memory 1` (CDP `Performance.getMetrics` + `Memory.getDOMCounters` after 300 s) |
| 6 | memory, 5 min soak | SSR | not measured: 5-minute profile skipped by coordinator time-box after the decision measurements closed | same command with `http://127.0.0.1:8802` |
| 7 | cold build, release (s) | CSR | 44.41 (35.58–54.71) | `rm -rf /tmp/s1-cold-csr && /usr/bin/time -p env CARGO_TARGET_DIR=/tmp/s1-cold-csr cargo leptos build --release -p spike-csr --frontend-only` |
| 7 | cold build, release (s) | SSR | two runs: 56.54 and 79.95 (n=2; no median — the values are stated, not averaged). CSR used `--frontend-only`, SSR builds both server and wasm, so the comparison is context, not a decision input | `rm -rf /tmp/s1-cold-ssr && /usr/bin/time -p env CARGO_TARGET_DIR=/tmp/s1-cold-ssr cargo leptos build --release -p spike-ssr` |
| 8 | auth | CSR | pass, no forcing condition: auth-code + PKCE S256, 3/3 login→board→logout cycles, deep link gated while logged out (`/projects/demo/board` → sign-in), token in memory, no cookies; an authenticated SSE needs a fetch + `ReadableStream` shim (`EventSource` cannot send `Authorization`) — work, not a failure | `node /tmp/s1-tools/auth.js http://127.0.0.1:8801 3` |
| 8 | auth | SSR | pass, not forcing: HttpOnly `s1_session` + CSRF double-submit, server-side code exchange, 3/3 login→board→logout cycles, deep link gated (303 → `/login`), `EventSource` sends the cookie same-origin, no shim | `node /tmp/s1-tools/auth.js http://127.0.0.1:8802 3` |

## Decision

**CSR — Leptos `csr`/WASM SPA.** The prior holds: SSR did not beat CSR on interaction TTI, and the
auth flow did not force cookie-based SSR.

- Interaction TTI (navigationStart → the first drag that mutates list membership, `performance.now()`
  in the drag handler), 4× CPU / Slow 4G, n=5: **CSR 5426 ms (5408–5628)** vs **SSR 5736 ms
  (5705–5758)**. The ranges do not overlap; SSR is 310 ms (~5.7 %) slower to first interaction.
- Both stacks exceed the 2 s threshold on this harsh protocol, so SSR's *precondition* is met — but
  the rule also requires SSR to beat CSR on TTI or on auth. It loses on both.
- SSR does what SSR is for: 500 cards are in the DOM at **1444 ms** (virtualised: 945 ms) vs
  **4726 ms** (virtualised: 4726 ms) for CSR. That win is real, but the prior's premise is an
  authenticated app with no SEO need, and the win does not survive to interactivity: the same WASM
  must download and hydrate 500 server-rendered nodes, which is why SSR is slower to interact.
- The known SSR trap (hydration double-fetch) **did not reproduce** on Leptos 0.9.0-rc1: exactly one
  board fetch per page load, server-side, in 5/5 runs, including with the client fallback disabled
  (`?raw=1`). SSR lost on hydration CPU, not on a refetch.
- Auth: CSR auth-code + PKCE S256 works end to end — 3/3 login → board → logout cycles, deep link
  gated while logged out, token in memory, no cookies. It does not force cookies. The one CSR cost is
  that an *authenticated* SSE needs a fetch + `ReadableStream` shim (`EventSource` cannot set
  `Authorization`); that is work, not a forcing condition, and it is the same work any SPA does.
- Metric 4's §7 threshold — **≥95 % of frames ≤16.7 ms** — was **not met by either stack**: CSR 80.0 % (75.5–82.7), SSR 76.5 % (71.8–80.4), with 0 long tasks >50 ms on both. The miss is symmetric, so it does not change the stack choice; it is a drag-smoothness target for S12, not evidence for SSR.
- Lighthouse 12 (simulated 4×/Slow 4G): `interactive` CSR 5703 ms (5553–5747) vs SSR 5855 ms
  (5853–5865); FCP CSR 751 ms vs SSR 1655 ms; TBT ≤ 24 ms on both. The supplementary number agrees
  with the in-page interaction measurement: CSR is the faster stack to interact with.

## Consequences

- S12 builds `apps/web` as a Leptos CSR/WASM SPA. No server-side rendering, hence no hydration path
  and no double-render surface in the app; the UI can ship as static assets behind the same origin
  as the API.
- Auth is auth-code + PKCE with the access token held in memory. Reloads need a re-login (or silent
  re-auth) and the app owns the deep-link gate; the server needs no UI session/CSRF surface.
- The authenticated event streams (board, chat) need a fetch + `ReadableStream` SSE client because
  `EventSource` cannot carry the bearer token. Budget it in S12.
- First interaction waits on the WASM bundle: 790 KiB raw / 284 KiB (291 kB) gzip-9 for this spike. A release
  size budget belongs in S12 CI. Virtualisation stays available as a fallback but did not help CSR's
  boot-bound first render (4726 vs 4726 ms); it does help SSR (1444 → 945 ms).
- What SSR would have cost: HttpOnly session + CSRF + a server-side token exchange, a server binary
  to deploy and scale, per-screen hydration behaviour to re-verify, +10 KiB gzip-9 — for a 3.3× faster
  first paint and no interaction win.
- A public/unauthenticated screen that needs first paint without WASM is a scoped exception, not a
  stack change: SSR was measured slower to interact.

## Revisit triggers

- CSR interaction TTI on a real device/network that puts a hard first-paint requirement on an
  authenticated screen and where only server rendering meets it.
- A requirement that makes the token unavailable to the SPA (no CORS/refresh path) so only an
  HttpOnly cookie can authenticate the app.
- Leptos 0.9 final, or a later version, reversing the measured order by more than the 5.7 % here
  (this measured `0.9.0-rc1`).
- A screen needing SEO or public unauthenticated first paint at scale.
