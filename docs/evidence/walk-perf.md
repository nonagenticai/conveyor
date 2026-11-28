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
| A five-minute soak within 1.5× the first-load heap | ≤ 1.5× | **1.89×** (2,560,361 → 4,850,904 B over 726 visits) | **NOT met** |

### The soak finding, isolated

Two soak runs at very different navigation rates produced the same absolute growth:

| Run | Visits in 5 min | First → last (B) | Ratio |
|---|---|---|---|
| Walking pace (six rooms + 2 s pause) | 726 | 2,560,361 → 4,850,904 | 1.89× |
| Stress (no pause) | 3,642 | 2,787,292 → 4,961,494 | 1.78× |

That the growth does not follow the visit count points at time, not navigation. A **control**
confirmed it: an idle session on the entrance, SSE stream open, no navigation at all, gained
**+444,903 B in 60 seconds** (2,798,540 → 3,243,443; ~7.4 KB/s retained after a forced GC). Sixty
seconds of that rate is ~0.44 MB; five minutes is ~2.2 MB — the soak's growth exactly.

So the budget failure is a real memory-retention defect in the running client, not a measurement
artefact and not a consequence of S4's two routes. It is filed as **#916** with this protocol and
these numbers. Per the SPEC, "a leaked interval or subscription is a defect": the suspect is the
per-event re-render path with the stream open (a refetch per SSE frame, on every subscribed
destination), but the retainer has not yet been identified — a heap-snapshot diff is the first step
of #916.

## Consequence for the P6 gate

Two of the three performance budgets are met with wide margins. The third is not, so S4 cannot
record the P6 performance criterion as passed and does not claim primacy. The gate remains wired in
`.github/workflows/walk.yml`; a green soak after #916 is the missing line.

## Re-running

```bash
cd apps/walk && BASE_URL=https://game.nonagentic.ai E2E_USER=… E2E_PASS=… \
  npx playwright test e2e/deployed/perf.spec.ts
```

The shell-byte and frame tests run in the deployed suite; the soak is opt-in (`SOAK=1`) so the
default deployed run stays short.
