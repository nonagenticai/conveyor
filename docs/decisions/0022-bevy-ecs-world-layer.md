# 0022 — The world layer is a `bevy_ecs` simulation in its own crate

Status: Accepted · 2026-11-26 · delegated rule decision under the published delegation (owner
direction 2026-11-26; expiry 2027-02-18). Not a human signature.

Supersedes ADR 0021 decision 5 **in part**: the simulation engine is no longer an open question. It
leaves ADR 0021's other decisions untouched, and it does not decide the renderer, which remains
ADR 0023's subject.

## Context

ADR 0021 authorises a second, game-style client at `/walk/` and left the engine open: canvas-2D was
the baseline, Bevy a candidate, and the P0 spike was to choose (`docs/design/GEMBA-WALK-SPEC.md`
§12). The owner has now directed that the Bevy ECS be designed as part of the architecture rather
than left to the spike.

Facts measured for this decision (fetched 2026-11-26, not remembered):

- `bevy_ecs` **0.20.1** (published 2026-10-28), MSRV **1.95.0**; the repository pins Rust
  **1.100.0** (`rust-toolchain.toml`), so the toolchain is compatible without a bump.
- `multi_threaded` is **off** by default and `std` is on: the crate compiles for
  `wasm32-unknown-unknown` without a thread pool.
- Since **0.17** the buffered event API is `Message` (`MessageReader`/`MessageWriter`); `Event` is
  the observer API. In **0.20** the observer parameter is `On<E>`. A message reader ordered before
  its writer sees the message on the next frame — ordering is a scheduling fact.
- Bevy runs systems in parallel by default; order is guaranteed only by `.chain()` and sets, and
  `ScheduleBuildSettings { ambiguity_detection }` (default `Ignore`) is the available guard.
- The conveyor has **no `room` or `seat` tables**: rooms are routes and seats are view presets
  (`docs/design/GEMBA-WALK-SPEC.md` §2–§3). The world is therefore client state over a server-owned
  record set — exactly the shape an ECS models well and a form framework models badly.
- The served action surface is 597 names (`docs/design/data/actions.json`); the SSE vocabulary is 31
  event types (`crates/actions/src/board/events.rs`).

## Decision

1. **`bevy_ecs 0.20.1` is the simulation layer of the world**, exact-pinned. The crate depends on
   `bevy_ecs`, `bevy_app` and `bevy_time` only, on their defaults, with `bevy_app`'s `web` feature
   enabled for `wasm32` — `bevy_time` reads the clock through `bevy_platform`, whose web backend the
   wasm build needs. Feature pruning is a *measured* bundle lever, not an assumption: `bevy_app`'s
   default re-enables `bevy_ecs`'s, so a prune turns both off together and is verified by a
   preflight compile. The `bevy` umbrella crate, `bevy_render`, `bevy_winit`, `bevy_asset` and
   `bevy_ui` are **not** dependencies.
2. **The simulation is a crate, not a leaf**: `crates/walk-world`, an `rlib` with no DOM, no
   web-sys, no network and no storage, compiled for host and `wasm32-unknown-unknown`, so it is
   tested headlessly by `checks-shard` rather than only in a browser.
3. **Three state classes, enforced by module boundary**: server truth is a *mirror* the ECS may not
   author; session state is owned; derived state is a pure function of the two. A write leaves as an
   `ActionCall` and the mirror changes only on a receipt or a delta.
4. **The mirror is never persisted.** `save()` serialises session state only — a stored programme
   copy is the "programme store" SPEC §6.4 forbids.
5. **Writes are intents validated against the live manifest** (`actions.describe`: `effect`,
   `confirm`, scopes). Optimistic UI is presentation-only: no component holds an unconfirmed
   domain value.
6. **Determinism is a schedule property**: `FixedUpdate` at 60 Hz with `Time<Fixed>`, explicit
   `.chain()`/sets, ambiguity detection at `LogLevel::Error` and a fixed `shuffle_seed` in tests, a
   seeded RNG resource, no wall-clock, no threads, ordered iteration.
7. **Identity is `RecordKey { kind, id }`**; projection is idempotent and despawn waits one snapshot
   generation (with the open sheet's subject pinned).
8. **The renderer stays behind the `WorldView` port.** The ECS produces a `FrameView`; canvas-2D and
   WebGL2 both consume it. ADR 0023 picks between them on measured evidence.

## Consequences

**Gained.** A deterministic, headless-testable world: every BDD scenario is a Rust test in
`checks-shard`, not a browser run. Real Bevy idioms (`Component`, `Resource`, `Message`, observers,
`Commands`, `Query<&mut _>`, plugins, fixed schedules) instead of a bespoke loop. The world's state
classes become explicit, which is what makes the "no programme store" rule enforceable rather than
aspirational.

**Paid.** `bevy_ecs` joins every workspace build, native and wasm; a cold compile is minutes, and the
no-local-Rust rule means CI is the only compile gate. The 0.x API moves fast: the 0.17
Event/Message split and the 0.20 `Trigger`→`On` rename are the reason every signature is written
against 0.20.1 and pinned to it. The wasm build is single-threaded, has no `Instant`/`SystemTime`,
and needs `panic = "abort"`. The crate adds a second build artifact to the Docker web stage.

**Mitigations.** Exact pin plus one dedicated upgrade issue per minor; feature pruning
(`default-features = false`, `bevy_reflect` off) is the bundle-size lever; the ambiguity test is the
determinism guard, not reviewer discipline; the renderer port keeps the one genuinely reversible
choice reversible.

**Not decided here.** The renderer (ADR 0023); the shell's language and module layout (SPEC §6.1,
JavaScript ESM); the route census, the parity instrument and the primacy decision (plan §6, §13).

## Alternatives considered

- **A hand-rolled ECS.** Fewer dependencies, but it would hand-write archetype storage, a schedule,
  change detection and ambiguity tooling — the parts of `bevy_ecs` that are hardest to get right and
  the parts a deterministic test depends on.
- **The full `bevy` umbrella.** It brings the renderer: tens of megabytes of wasm, a GPU-surface
  contract and a CI build cost, for a client whose only work surface must stay DOM (SPEC §2). The
  world would gain a renderer it may not be allowed to use.
- **No world layer.** Contradicts ADR 0021's product decision and would reduce `/walk/` to a second
  operator UI with a different theme.
- **Leave it to the spike.** That is what ADR 0021 said, and the owner has overruled it: an
  architecture the build must implement cannot be a spike output.

## Links

- `docs/design/GEMBA-WALK-ECS.md` — the architecture description this decision authorises.
- `docs/decisions/0021-gemba-walk-client.md` — the client's authorisation; decision 6 amended here.
- `docs/design/GEMBA-WALK-SPEC.md` §6.2, §12 — the port and the (now renderer-only) decision protocol.
- `docs/plans/GEMBA-WALK.md` — W1 (renderer spike) and W2 (foundation) re-scoped accordingly.
