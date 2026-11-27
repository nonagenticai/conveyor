# 0023 — The Gemba Walk's renderer is canvas-2D

**Status:** Accepted · 2026-11-27 · **Approved:** delegated rule decision under the published
delegation of 2026-11-22 (expiry 2027-02-18) — not a human signature · **Owner direction:**
2026-11-26 · **Evidence:** walk-spike run
[45953201576](https://github.com/nonagenticai/conveyor/actions/runs/45953201576) at `ce63d718f`,
recorded in [`docs/evidence/walk-spike.md`](../evidence/walk-spike.md) and
[`walk-spike-results.json`](../evidence/walk-spike-results.json).

## Context

ADR 0022 decided the simulation: `bevy_ecs 0.20.1` in `crates/walk-world`, behind the
`WorldView` port. The renderer was left open: SPEC §6.3 names a canvas-2D baseline and a WebGL2
candidate, both consuming the same `FrameView`; the P0 spike was to measure them and this ADR was
to record the winner and the budgets. The baseline ships first; the port is the reversibility.

The spike measured the reference scene — six rooms, twelve figures, labels, one moving figure —
in Chromium 158 on a GitHub Actions runner, with 4× CPU throttling and Slow 4G emulation, five
cold runs per renderer.

## Decision

1. **canvas-2D is the renderer.** It is implemented as `apps/walk/src/world/canvas2d/` behind the
   `WorldView` port; it is the only renderer wired in W2.
2. **WebGL2 remains a candidate behind the port, not a dependency.** No `bevy_render`,
   `bevy_winit`, `bevy_asset`, `bevy_ui`, no WebGL context in the default path, no GPU requirement
   for a user to use the walk. The port keeps the swap mechanical.
3. **The measured budgets are release criteria.** At the reference scene: 60 fps target (p50
   ≤ 16.7 ms, p95 ≤ 20 ms; no frame > 50 ms after the first two), zero long tasks, cold load
   ≤ 1.5 s under the spike protocol (4× CPU, Slow 4G), initial JS ≤ 250 KB gzip, and a 5-minute
   soak within 1.5× the first-load heap with no upward trend. The measured values are in the
   record and well inside these.
4. **The simulation budgets stay ADR 0022's.** This ADR does not decide the wasm size (≤ 600 KB
   gzip), the fixed tick (≤ 1 ms p50 at 500 entities) or the entity ceiling (≤ 5 000); the
   `walk-world` crate and its world spec own them.
5. **The renderer never owns a capability.** Sheets, forms, tables and decisions are DOM; the
   canvas carries short labels duplicated in the DOM; the canvas is labelled and never traps focus
   (SPEC §6.2, §8).

## Measured

Five cold runs each; median (min–max). Both renderers drew the same reference scene.

| Metric | canvas-2D | WebGL2 |
|---|---|---|
| Frame p50 | 16.7 ms (16.7–16.7) | 16.7 ms (16.7–16.7) |
| Frame p95 | 16.8 ms (16.7–16.8) | 16.7–16.8 ms (16.7–16.8) |
| Long tasks | 0 | 0 |
| Cold load (median) | 1 269 ms (1 261–1 282) | 1 267 ms (1 264–1 281) |
| JS transfer | 24 388 B / 9 requests | 24 388 B / 9 requests |
| Heap after 5 s (median) | 6.0 MB (3.1–8.9) | 8.9 MB (4.2–13.8) |
| 5-minute soak | 2.54 MB → 2.71 MB (20 changes) → 1.93 MB | — |

The frame metric is a tie: both are vsync-capped at 60 fps. The tie is broken by what the
renderers cost to carry: canvas-2D needs no GPU/WebGL2 path, has the smaller heap, and is the
smaller implementation. The soak shows no growth.

## Alternatives considered

- **WebGL2 as the renderer.** Rejected: it ties canvas-2D on every measured frame/load metric at
  the reference scene, adds a GPU/WebGL2 requirement and a larger heap, and buys nothing the
  scene needs today. It remains available behind the port for the revisit trigger.
- **`bevy_render` / the Bevy render stack.** Rejected in ADR 0022 and here: tens of MB and a
  GPU-surface contract for a client whose only work surface is DOM.
- **A third renderer.** Not measured; the port makes one a future experiment, not a decision now.

## Consequences

- Positive: the smallest runtime, no GPU requirement, no second rendering stack; the frame loop is
  a pure function of `FrameView`, so the world remains testable headlessly and the renderer
  replaceable.
- Cost: a hand-written projection, depth sort and figures; dense scenes will eventually need more
  than canvas-2D can carry. That is the revisit trigger, not a present failure.
- The engine decision remains two-layered and explicit: the simulation is decided (ADR 0022), the
  renderer is decided (this ADR), and the port is the seam between them.

## Revisit triggers

- The reference scene or a real destination scene misses the frame budget on the canvas: p95
  > 20 ms, a frame > 50 ms after the first two, or > 5 000 visible entities.
- A measured WebGL2 win of more than 20% on frame p95 with no heap/requirement cost the owner
  accepts.
- The browser/accessibility requirements change such that WebGL2 is no longer the broader
  fallback.
- The P6 primacy decision; the renderer choice is not a reason to change primacy.

## Links

ADR 0022 (the simulation) · ADR 0021 (the client) · SPEC §6.3, §9, §12 · ECS §12–§13 ·
[`docs/evidence/walk-spike.md`](../evidence/walk-spike.md) · the run `45953201576`.
