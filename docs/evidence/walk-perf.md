# The walk's performance record (W7)

Measured 2026-11-28 for S4 (#817) against the deployed game host `https://game.nonagentic.ai/`
with a real `conveyor-e2e` sign-in, and against the local harness where noted. Every number is
printed by `apps/walk/e2e/deployed/perf.spec.ts`; nothing here is estimated.

## Protocol

```bash
cd apps/walk
BASE_URL=https://game.nonagentic.ai E2E_USER=conveyor-e2e E2E_PASS=… \
  npx playwright test e2e/deployed/perf.spec.ts          # shell bytes + frames
SOAK=1 … npx playwright test e2e/deployed/perf.spec.ts --grep soak   # the five-minute soak
```

- **Shell bytes** are measured after a cold load (CDP `Network.setCacheDisabled`), summing
  `encodedBodySize` over same-origin `.js`/`.css` plus the document. A cache hit reports
  `transferSize` 0 but still carries `encodedBodySize`, so a warm reload cannot understate it.
- **Frames** are 300 `requestAnimationFrame` deltas over five seconds; the first two are discarded
  as the mount cost.
- **Soak** is five minutes of navigation across six rooms with the SSE stream open, and a heap
  reading after a forced CDP `HeapProfiler.collectGarbage`, so it measures retained memory rather
  than uncollected garbage.

## Results

| Budget (`GEMBA-WALK-SPEC.md` §9) | Budget | Measured | Verdict |
|---|---|---|---|
| Shell + entrance, JS/HTML/CSS, gzip, excluding wasm | ≤ 250 KB | **36.2 KB** (25 resources; document 810 B) | **met** (14.5 % of budget) |
| No frame over 50 ms after the first two | ≤ 50 ms | p95 **16.7 ms**, worst **16.8 ms** (300 frames) | **met** (60 fps) |
| A five-minute soak within 1.5× the first-load heap | ≤ 1.5× | **1.059×** (3,786,641 → 4,011,855 B over 702 visits, warm baseline) | **met** |

### The soak: what the budget was actually measuring

Two corrections were made after the first version of this record, both measured on the deployed
origin with a real `conveyor-e2e` sign-in and forced GC at every reading.

**1 — the metric was a floor.** Chrome reports `performance.memory.usedJSHeapSize` as exactly
`10000000` for any heap under 10 MB unless `--enable-precise-memory-info` is set, and
`playwright-core` 1.65.0 sets no such switch. The same page reads `10000000` without it and
`2623436` with it. `playwright.config.ts` now sets it (#916).

**2 — the baseline was taken before session history existed.** Every in-app navigation adds one
`pushState` entry; Chrome caps the list at 50. Measured on the shipped client:

| Experiment | Δ (after forced GC) | Notes |
|---|---|---|
| 60 s idle on `/plan` | **−520 B** | flat; the frame loop is not a retainer |
| 50 full reloads of `/plan` | **−36,380 B** | flat; a reload resets the client |
| 48 in-app navigations | **+698,358 B** | DOM nodes 63 → 63; `history.length` 4 → **50** |
| 70 in-app navigations | **+644,986 B** | `history.length` already **50**; growth stopped |

The 48- and 70-navigation runs land within 53 KB of each other while one performed 22 more
navigations: growth halts exactly at the history cap. The earlier record's "time-bound, ~7.4 KB/s,
SSE-driven" reading did not reproduce — an idle minute is flat — and the earlier control
(+444,903 B in 60 s) could not be reproduced on the shipped client in four attempts.

So the budget compared a first-load heap taken at **0** history entries to a settled heap at **50**,
which any `pushState` SPA exceeds by construction. `perf.spec.ts` now fills the cap with a
50-navigation warm-up, asserts `history.length === 50`, and only then reads the baseline.

| Run | Baseline | First → last (B) | Visits | Ratio | Verdict |
|---|---|---|---|---|---|
| Cold (as first recorded) | 0 entries | 2,560,361 → 4,850,904 | 726 | 1.89× | not met — protocol |
| Cold, precise metric | 0 entries | 2,795,646 → 4,347,762 | 708 | 1.555× | not met — protocol |
| **Warm (shipped protocol)** | **50 entries** | **3,786,641 → 4,011,855** | **702** | **1.059×** | **met** |

Retention across a five-minute, 702-navigation session is therefore ~225 KB. There is no leak; the
two defects #916 did fix — the precise-memory switch and a render generation that stops a stale
render overwriting a newer screen — are both kept, and the first is what made this number readable.

Two caveats stated rather than hidden: the frame p50 is not recorded (the p95 above is), and the
SPEC §8 16 px gutter is not separately measured — the 390 px/200 % zoom measurement lives in
`walk-a11y.md`.

## Consequence for the P6 gate

All three performance budgets are met. The soak is met **after** the protocol correction above and
not before it; the first version of this record said "not met", and that verdict was the protocol's,
not the client's. S4 records the P6 performance criterion as passed. Primacy remains the owner's
decision and is not claimed here.

## Re-running

```bash
cd apps/walk && BASE_URL=https://game.nonagentic.ai E2E_USER=… E2E_PASS=… \
  npx playwright test e2e/deployed/perf.spec.ts
```

The shell-byte and frame tests run in the deployed suite; the soak is opt-in (`SOAK=1`) so the
default deployed run stays short.
