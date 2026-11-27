# The Gemba Walk — world-layer architecture (bevy_ecs)

Status: Accepted · Date: 2026-11-26 · Owner direction: 2026-11-26 · ADR: 0022
Normative siblings: `GEMBA-WALK-SPEC.md` §2, §6.2, §10, §12 · Plan: `docs/plans/GEMBA-WALK.md`
Crate: `crates/walk-world` · Pinned engine: `bevy_ecs 0.20.1`

This document is the architecture description of the world layer. It fixes the crate boundary, the
entity/component model, the schedules, the message and observer contracts, the command path, the
projection rules, determinism, the wasm surface and the test strategy. It is normative for
`crates/walk-world`; the shell consumes it through the `WorldView` port and the wasm boundary.

It does **not** reopen SPEC §2: the world is navigation, the sheets are the work, and no capability
exists only in the world. The ECS renders no text that the DOM does not also carry.

## 1. Boundary and the three state classes

The server is the only source of programme truth. The ECS is a **view** of it plus the client state
the server has no table for. This is not a convention: it is the module boundary.

| Class | Examples | Owner | Written by | Persisted | Authority |
|---|---|---|---|---|---|
| **Server truth** (mirror) | project, tranche, card, register item, decision, architecture artefact, meeting | `projection` module | `project_snapshot`, `apply_delta` only | **never** | the API; the ECS may not author it |
| **Session** | seat preset, avatar pose, camera, open sheet, selection, filters, unsent draft | `session` module | intent and session systems | preferences only, under `conveyor.walk.*` | the client |
| **Derived** | proximity prompt, highlight, badge, mood, moodlet, draw list | `system/mood`, `view` | pure systems | no | a function of the two above |

Rules that follow, and that a review must enforce:

1. No system assigns a mirror field from local intent. A write leaves as an `ActionCall` and the
   mirrored value changes only when a receipt or a delta arrives.
2. `save()` serialises **session state only**. Serialising the mirror would be the "programme store"
   SPEC §6.4 forbids: a seed by another name.
3. `project()` is idempotent. Applying the same snapshot twice leaves every component and the entity
   key set identical.
4. A screen refetches after a write (SPEC §5); the mirror never becomes the refetch substitute.

## 2. Crate and dependency contract

```
crates/walk-world/
  Cargo.toml
  src/lib.rs              WalkWorld, WorldConfig, WalkWorldPlugin
  src/entity.rs           RecordKey, RecordKind, SceneKey, InteractableId
  src/component.rs        every component (derive Component)
  src/resource.rs         every resource (derive Resource)
  src/message.rs          every message (derive Message) and entity event (derive EntityEvent)
  src/projection.rs       snapshot/delta application, despawn guard
  src/command.rs          ActionCall, Outcome, the call state machine
  src/session.rs          seat, camera, sheet, save/load
  src/view.rs             FrameView, SheetContext, render_prepare
  src/replay.rs           the replay log and its determinism check
  src/system/{intent,locomotion,collision,interaction,camera,animation,mood,outbox}.rs
  src/wasm.rs             the wasm-bindgen surface (feature "wasm")
  tests/                  headless scenarios, properties, replay, ambiguity
```

```toml
[dependencies]
bevy_ecs   = { version = "=0.20.1", features = ["serialize", "bevy_debug_stepping", "debug"] }
bevy_app   = "=0.20.1"
bevy_time  = "=0.20.1"
serde      = { version = "1", features = ["derive"] }
serde_json = "1"

[dependencies.wasm-bindgen]
version = "=0.2.130"
optional = true

[dependencies.console_error_panic_hook]
version = "0.1.7"
optional = true

# bevy_time reads the platform clock through bevy_platform, which needs its web backend on wasm.
# Enabled per target so native builds never pull wasm-bindgen or web-sys.
[target.'cfg(target_arch = "wasm32")'.dependencies]
bevy_app = { version = "=0.20.1", features = ["web"] }

[features]
default = []
wasm = ["dep:wasm-bindgen", "dep:console_error_panic_hook"]
```

The defaults stay on. `bevy_app`'s default feature enables `bevy_ecs/default`, so pruning
`bevy_ecs` in isolation changes nothing; a real prune turns both off together and is verified by a
preflight compile, never assumed here.

`ScheduleBuildSettings`'s `ambiguity_detection` and `shuffle_seed` fields are gated by `bevy_ecs`'s
`debug` and `bevy_debug_stepping` features in 0.20.1; both are enabled, so the determinism guard is
compiled, not assumed.

**Not in the dependency graph, deliberately:** the `bevy` umbrella crate, `bevy_render`,
`bevy_winit`, `bevy_asset`, `bevy_ui`; `web-sys`, `leptos`, `gloo`. The crate is `rlib`, compiles
for host and `wasm32-unknown-unknown`, and has no DOM, no network and no storage.

Costs accepted, stated so a future session does not rediscover them: `bevy_ecs` joins every
workspace build (native and wasm); `multi_threaded` stays off; the 0.x API is pinned exactly and an
upgrade is its own issue.

## 3. Entities

Identity is a key, not an allocation order. Every mirrored entity carries exactly one `RecordKey`;
every scene-only entity carries a `SceneKey`. Spawning is idempotent by key: `project()` looks up by
key first, spawns only when absent.

| Entity | Key | Source | Cardinality |
|---|---|---|---|
| Project | `Record(Project, id)` | `project` via `programmes.*` reads | 52 nominal |
| Tranche | `Record(Tranche, id)` | `tranches.*` | 14 |
| Card / unit | `Record(Card, id)` | `cards.*`, board reads | open cards, ≤ 400 |
| Register item (risk, change, …) | `Record(RegisterItem, id)` | `registers.*` | per project |
| Decision | `Record(Decision, id)` | `decisions.*` | per project |
| Architecture artefact | `Record(Artifact, id)` | `architecture.artifacts_*` | per project |
| Meeting | `Record(Meeting, id)` | `meetings.*` | per board |
| Room | `Scene(Room, WingId)` | the route table's wings | 8 |
| Wall | `Scene(Wall, RouteId)` | the frozen census: one per route | 52 |
| Portal | `Scene(Portal, n)` | the wing graph | ~12 |
| Avatar | `Scene(Avatar, 0)` | one per session | 1 |
| Advisor | `Scene(Advisor, SeatId)` | the seat presets | ≤ 7 |
| Prop, badge, moodlet | `Scene(_, n)` | derived | bounded by the draw budget |

`InteractableId` is the new-type the port exchanges with the renderer: it is either a `RecordKey`
(open the record's sheet), a `WallId` (open the destination) or a `RoomKey` (open the room's sheet). The renderer never invents one.

## 4. Components

| Component | Field sketch | Written by | Source |
|---|---|---|---|
| `Position(Vec3)` | world position | `locomotion`, `spawn` | scene |
| `Velocity(Vec3)` | m/s | `intent`, `locomotion` | input |
| `Facing(f32)` | yaw radians | `intent`, `locomotion` | input |
| `Collider { half_extents }` | wall AABB | static | scene |
| `Avatar` | marker | `spawn` | session |
| `CameraRig { mode, yaw, distance, target }` | | `camera` | session |
| `InRoom(RoomKey)` | which room the avatar is in | `interaction` | derived |
| `Wall { route: RouteId }` | | `spawn` | census |
| `Interactable { target: InteractableId }` | | `spawn` | scene/projection |
| `Highlight { on: bool }` | | `interaction`, `session` | derived |
| `Badge { count: u16, kind }` | queue counts | `mood` | derived |
| `Mood { valence, label }` | | `mood` | derived |
| `Moodlet { expires_at_tick }` | | `mood` | derived |
| `RecordRef(RecordKey)` | reverse index | `projection` | mirror |
| `Version(u64)` | `If-Match` source | `projection` | mirror |
| `Title(String)`, `Headline(String)` | short labels only | `projection` | mirror |
| `PendingCall(CallId)` | presentation | `outbox` | command |
| `AbsentSince(u64)` | despawn guard generation | `projection` | projection |
| `AnimPhase(f32)` | | `animation` | derived |
| `SeatPreset(SeatId)` | | `session` | session |

Long-form content never enters a component. A component holds what the world draws or the command
path needs to build a request; the sheet fetches the record itself.

## 5. Resources

| Resource | Purpose |
|---|---|
| `WorldClock { tick: u64 }` | the only clock; incremented in `FixedUpdate` |
| `ActionManifest` | name → `{effect, confirm, scopes, idempotent, spec_hash}` from `actions.describe` |
| `Messages`-backed input streams | see §7; each `app.add_message::<T>()` |
| `Outbox { queue: Vec<ActionCall> }` | intents awaiting the transport; drained by the shell |
| `SnapshotGeneration { gen: u64 }` | the despawn guard |
| `SessionState { seat, sheet, selection }` | session truth |
| `CameraRig` | the camera resource (mirrored into the avatar entity for queries) |
| `FrameView` | the last `render_prepare` output |
| `Rand64 { state }` | seeded; cosmetic jitter only |
| `WorldConfig { seed, rooms, walls }` | construction inputs |
| `RecordIndex { key → Entity }` | the projection's forward lookup; the only writer is `projection` |
| `SceneIndex { walls, rooms }` | the scene-only reverse lookups the sheet uses for wall/room titles |
| `LastSeq` | the delta sequence gap detector; zero means no delta yet |
| `Intent` / `CallSeq` | the movement intent and the monotonic call counter |
| `PromptState` / `LastProblem` | the derived proximity prompt and the last refusal |

## 6. Schedules, sets and ordering

The simulation is `FixedUpdate` at 60 Hz (`Time<Fixed>`); presentation is `Update`. Every ordering
constraint is explicit; nothing relies on insertion order.

| Schedule | Set (in order) | Systems | Effect |
|---|---|---|---|
| `FixedUpdate` | `IntentSet` | `read_input`, `validate_against_manifest` | input message → `ActionRequested` / movement intent |
| | `LocomotionSet` | `apply_intent` | writes `Velocity`, `Facing` |
| | `CollisionSet` | `integrate`, `resolve_walls` | writes `Position`, clamps to floors |
| | `InteractionSet` | `resolve_proximity`, `resolve_pick` | writes `InRoom`, `Highlight`, emits `PromptChanged` |
| | `CameraSet` | `follow`, `boom_collision` | writes `CameraRig` |
| | `AnimationSet` | `advance_phase`, `apply_moodlets_expiry` | writes `AnimPhase`, drops expired moodlets |
| | `MoodSet` | `derive_mood`, `derive_badges` | writes `Mood`, `Badge` |
| | `OutboxSet` | `drain_requests` | writes `Outbox`, `PendingCall` |
| `Update` | `ProjectionSet` | `apply_snapshot`, `apply_delta`, `apply_receipt`, `despawn_absent` | mirror writes |
| | `RenderPrepareSet` | `build_frame_view` | writes `FrameView` |

Within each set the systems are `.chain()`ed where one reads what the previous writes. The
`ProjectionSet` is in `Update` because a receipt must land at the next frame, not at the next fixed
tick, and it touches no simulated state.

Why this shape: a `MessageReader` ordered **before** its writer sees the message on the **next**
frame. Same-frame reaction is therefore a scheduling fact, not a hope — the chains above are the
contract, and `same_frame_ordering` is a test.

## 7. Messages and observers

Bevy 0.20 has two mechanisms and they are not interchangeable. The rule:

| Mechanism | Use for | Not for |
|---|---|---|
| **Message** (`#[derive(Message)]`, `app.add_message::<T>()`, `MessageWriter::write`, `MessageReader::read`) | cross-system streams that need an ordering guarantee | per-entity reactions |
| **Observer** (`#[derive(Event)]` / `#[derive(EntityEvent)]`, `Commands::trigger`, `On<E>`) | per-entity reactions to an add/update/selection | global flow that another system must observe the same frame |

Messages:

| Message | Writer | Reader | Same-frame? |
|---|---|---|---|
| `InputIn` | the shell (via `push_input`) | `read_input` | yes |
| `SnapshotIn` | the shell | `apply_snapshot` | next frame |
| `DeltaIn` | the shell (SSE) | `apply_delta` | next frame |
| `ReceiptIn` | the shell | `apply_receipt` | next frame |
| `ActionRequested` | `validate_against_manifest` | `drain_requests` | **yes — chained** |
| `ActionResolved` | `apply_receipt` | the sheet presenter | next frame |
| `PromptChanged` | `resolve_proximity` | `build_frame_view` | yes (same schedule) |
| `SessionCommand` | the shell | session systems | next frame |

Entity events (observers), all `#[derive(EntityEvent)]` with an `entity` field read through
`On<E>`:

| Event | Triggered by | Observer effect |
|---|---|---|
| `RecordUpdated` | `projection` when a mirrored component changes | re-derive the badge/mood for that entity |
| `WallEntered` | `interaction` | open the destination sheet if the session says so |
| `SheetOpened` / `SheetClosed` | `session` | pin/unpin the subject from the despawn guard |
| `ReceiptApplied` | `projection` | clear `PendingCall`, announce on the entity |

Observers never drive ordering between systems; if an effect must happen before another system reads
it in the same tick, it is a message in a chain, not an observer.

## 8. The command path

A write is a state machine. The ECS owns the states; the shell owns the transport.

```
Draft ──submit──▶ Submitted ──drain──▶ Pending ──receipt──▶ Confirmed
  │                  │                    │
  │                  │                    ├──refusal(4xx)──▶ Refused
  │                  │                    ├──transport────▶ Failed
  │                  │                    └──deadline─────▶ TimedOut
  └──discard────────▶ (no call)
```

`ActionCall { id: CallId, action: String, input: serde_json::Value, idempotency_key: String,
if_match: Option<u64>, approval_id: Option<Uuid> }`.

| Rule | Why |
|---|---|
| The manifest's `effect` decides the confirmation: `read` none, `write` one confirm, `destructive` requires `approval_id` | SPEC §5, §6.5; the server's gate is the only gate |
| `confirm = card|card-typed` without an approval is not submitted; the sheet shows the refusal the server would give | never pre-select or auto-approve (SPEC §3) |
| `if_match` comes from the mirrored `Version`; a `412` refetches and shows both versions | never retry with a guessed version |
| `idempotency_key` is generated once per call and reused on retry | a retried write must not double-apply |
| No component field is set from the request body; only the receipt or the delta changes the mirror | the optimistic value is exactly the "client copy" SPEC §6.4 forbids |
| `PendingCall` is presentation state; a `Refused` clears it and surfaces the problem verbatim | the toast never claims a success the server did not return |
| Navigation does not cancel a submitted call; its result lands in the mirror | the act already left the client |

`Outcome` is `Receipt(Value) | Refused(Problem) | Failed(String) | TimedOut`; `resolve(id, outcome)`
is the only entry point, and it is a message (`ReceiptIn`) so the schedule stays the only mutator.

## 9. Projection

A snapshot is `{ generation: u64, records: Vec<Record> }`. `apply_snapshot`:

1. For each record, resolve the entity by `RecordKey`; insert or update the mirrored components;
   stamp `Version`; clear `AbsentSince`.
2. For every mirrored entity not seen in this generation, insert `AbsentSince(gen)`.
3. On the **following** generation, despawn every entity whose `AbsentSince` is older than one
   generation — **except** the entity pinned by `SessionState.sheet`.

A delta is `{ seq: u64, kind, op: upsert|remove, record }`. `apply_delta` upserts by key; a `seq`
gap, a `resync` frame, or a 409 from the stream triggers `push_snapshot` on the shell side. Remove
marks `AbsentSince`; it never despawns inline.

Properties the tests assert:

- `project(s); project(s)` ≡ `project(s)` — components and key set identical.
- `project(s1); delta(upsert r)` ≡ `project(s1 ∪ {r})` — for the same generation.
- A key absent for one generation survives; absent for two, it is despawned unless pinned.
- The mirror never round-trips through `save()`.

## 10. Determinism and replay

| Rule | Mechanism |
|---|---|
| One clock | `Time<Fixed>` at 60 Hz; `WorldClock.tick` is the only tick a system may read |
| Explicit order | `.chain()` within a set, sets ordered in `configure_sets` |
| No ambiguity | `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, shuffle_seed: Some(0), ..default() }` on both the `FixedUpdate` and `Update` schedules in `WalkWorld::new` |
| No ambient time | no `Instant`, `SystemTime`, `std::thread`, no `rand::thread_rng` |
| No iteration-order dependence | `BTreeMap`/`Vec` anywhere order affects state |
| Reproducible RNG | `Rand64` seeded from `WorldConfig.seed`; cosmetic only |

Replay: `ReplayLog { seed, entries: Vec<(tick, Entry)> }` where `Entry` is one of the four input
messages. `replay(log)` builds a fresh `WalkWorld`, applies each entry at its tick, and returns the
final `FrameView` hash. Equality across two runs and across native/wasm is the test.

## 11. WASM constraints and the boundary

- `multi_threaded` off; the executor is single-threaded. No `std::thread`.
- `App::new()` + `TimePlugin` only (`MinimalPlugins` is a `bevy` umbrella group, not a `bevy_app`
  export): no window, no renderer, no asset server. `ScheduleRunnerPlugin` is **not** used; the shell
  owns the frame loop and calls `advance(dt)` from `requestAnimationFrame`.
- The host-side surface is JSON at the edge, serde inside:

| Export | Signature | Notes |
|---|---|---|
| `new` | `(seed: u64, config_json: &str) -> WalkWorld` | rooms/walls come from the census |
| `advance` | `(dt_ms: f64)` | clamps a long frame to 4 ticks |
| `push_input` | `(json: &str)` | `InputIn` |
| `push_snapshot` | `(json: &str)` | `SnapshotIn` |
| `push_delta` | `(json: &str)` | `DeltaIn` |
| `resolve` | `(json: &str)` | `ReceiptIn` |
| `drain_calls` | `() -> String` | `Vec<ActionCall>`, emptied |
| `view` | `() -> String` | `FrameView` |
| `context` | `(interactable_json: &str) -> String` | `SheetContext` |
| `set_manifest` | `(json: &str)` | the role-filtered `actions.describe` manifest |
| `view_hash` | `() -> String` | the quantized `FrameView` hash the world spec compares |
| `save` / `load` | `() -> String` / `(&str)` | session only |

- `panic = "abort"` in the wasm profile; `console_error_panic_hook` installed in `new`.
- No `Instant`: `bevy_platform`'s web time is used by `bevy_time`; the shell supplies the delta.
- The `.wasm` is a lazy asset loaded after first paint; it is never in the sign-in critical path
  (SPEC §9).

## 12. Performance budgets

| Budget | Value | Measured by |
|---|---|---|
| Entities, reference programme | ≤ 5 000 | `view()` entity count in the spike scene |
| Fixed tick cost | ≤ 1 ms p50 at 500 entities | the replay benchmark on an M-series laptop and in CI |
| Wasm heap after 20 room changes | ≤ 30 MB, ≤ 1.5× first load | the soak test |
| `walk-world` wasm contribution | ≤ 600 KB gzip | the spike's bundle measurement |
| First `FrameView` after fetch | ≤ 100 ms | the spike |

Budgets are release criteria for P6, not hopes; a miss is recorded in the spike and ADR 0023, and
the mitigation is a measured prune (`bevy_ecs` and `bevy_app` defaults off together, `bevy_reflect`
off, `opt-level = "z"`, LTO, wasm-opt) verified by a preflight compile — never a different ECS.

## 13. Renderer adapter and the `WorldView` port

The port from SPEC §6.2 stays. Its producer changes: the ECS produces a `FrameView`, and the adapter
draws it. The adapter is a pure function of the frame view plus the canvas.

```
ECS (Rust, wasm)                    shell (JS)                       adapter (canvas-2D | WebGL2)
FrameView ──view()──▶ world/ecs ──▶ world/port ──setScene──▶ draw
InputIn   ◀──push_input──         ◀──on(pointer/key)──
```

`FrameView`:

| Field | Shape | Consumer |
|---|---|---|
| `camera` | `{mode, position, target, fov}` | the projection |
| `sprites` | `[{id: Option<InteractableId>, kind, position, size, facing, tint, label}]` | depth sort + draw; scene sprites (the avatar) have no interactable id |
| `overlays` | `[{id, kind, anchor, text}]` | prompts, badges, moodlets — short labels only |
| `highlight` | `Option<InteractableId>` | selection from a sheet or the palette |
| `room` | `RoomKey` | the HUD's location line |

Rules: no draw-list entry carries long text, a table or a form (SPEC §2); the adapter never mutates
the world — it returns `InputIn`; picking is `pointer → sprite id` in the adapter, resolved against
`InteractableId`, never a second ray-cast implementation in the ECS.

The renderer is **canvas-2D** (ADR 0023, accepted 2026-11-27), selected by the P0 spike; WebGL2
remains a measured candidate behind this port, not a dependency. This document fixes only what both
renderers consume.

## 14. Testing

Headless is the default: `WalkWorld::new` builds `App::new()` + `TimePlugin` + `WalkWorldPlugin`;
`advance(dt)` runs `First`, the clamped `FixedUpdate` ticks and `Update`; no browser, no canvas, no
network.

| Scenario (BDD) | Test |
|---|---|
| Given the avatar outside a wall, when it walks into it, then a prompt names the destination | `walk_to_wall_prompts` |
| Given a snapshot, when projected twice, then components and keys are identical | `projection_is_idempotent` |
| Given a record absent for one generation, when the next snapshot also misses it, then it despawns | `despawn_requires_two_generations` |
| Given an open sheet, when its record disappears from the snapshot, then the entity survives | `sheet_subject_is_pinned` |
| Given a destructive action without an approval, when submitted, then no call leaves and the refusal is shown | `destructive_requires_approval` |
| Given a submitted call, when a refusal arrives, then `PendingCall` is cleared and the problem is surfaced | `refusal_clears_pending` |
| Given an input and a writer in the same set, when run unchained, then the reader sees it next frame; chained, same frame | `same_frame_ordering` |
| Given a replay log, when replayed twice, then the frame-view hash is equal | `replay_is_deterministic` |
| Given the built schedule, when initialized with ambiguity detection, then no ambiguity is reported | `no_schedule_ambiguity` |
| Given the wasm module, when loaded in the shell, then `view()` matches the native frame view | Playwright `world.spec.ts` |

Playwright covers what only a browser can: the module loads, the canvas matches the frame view, a
sheet opens on selection, and a write round-trips with its headers.

## 15. Security

- The ECS holds no token, no URL it did not receive from the shell's `api` module, and no
  credentials. `save()` cannot leak one because it serialises session state only.
- The manifest gates what the sheet offers; the server authorises. The ECS refuses to submit an
  action absent from the manifest, and shows the server's refusal verbatim otherwise.
- Every server string in a component is a short label rendered as text by the shell; the draw list
  never carries HTML. The sheet renderer escapes as SPEC §9 requires.
- A record id is not a capability: opening a sheet is a read the server authorises.

## 16. Extension rules

Adding a component: name its writer system and its source in §4, or it is rejected. Adding a system:
assign it a set in §6 and state its inputs and outputs. Adding a message: classify it in §7 and say
whether it must be same-frame. Adding an entity kind: give it a `RecordKey` with a real source
table, or it is scene-only and carries a `SceneKey`.

Never in the ECS: network calls, timers, storage, DOM, `Instant`, threads, long-form content, a
second copy of a record's authoritative state.
