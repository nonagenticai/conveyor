# video_production_service absorption — evidence (2026-11-15)

Issue #139 · the browser executor image (`conveyor-media-browser`). The reference is pinned by
digest; no private hostname, slug, address or customer payload appears in this file.

## The contract

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/video_production_service/` — taken 2026-11-15T16:47:33Z |
| Pinned image | `sha256:016a2db10e4984813e138f280c07bf5e9fb6cd7053089b78fa7c315b12e72cd6` |
| Operations captured | 51 (47 paths) |
| Golden cases | 51 |
| Capability kinds after the drop table | 2 — `media.produce_video`, `media.record_browser` |

Captured from the running image against a scratch `postgres:18` and redis, with synthetic fixtures
(a one-page HTML document and a generated short mp4). One capture run was invalidated by a full
Docker VM and every case was re-taken; the goldens in the tree are from the second run.

## A measured defect in the live reference

**`POST /api/compile` cannot complete in the running digest.** Every submission dies in
`_initialize` with `AttributeError: 'CompilationJob' object has no attribute 'fps'` — the image's
ORM lacks the column — so the job stays `processing` forever. Production runs this exact digest.
What is pinned is therefore the **intake contract** (the 200 `AsyncJobResponse`), never a completed
compile. The port does not claim a render parity the reference cannot produce; its own
`media.produce_video` tests carry the render evidence.

## Parity accounting

`apps/media/contract/parity.toml` has one row per captured operation;
`apps/media/tests/parity.rs` fails on a missing, duplicated or orphan row.

- **7 served as kinds**: `POST /api/compile`, `POST /api/compile/title-card`,
  `POST /api/record/animation` → `media.produce_video`; `POST /api/record/content`,
  `POST /api/record/url`, `POST /api/record/part`, `POST /api/record/capture/screens` →
  `media.record_browser`.
- **44 dropped with a reason**, grouped: the job ledger (poll/status/cancel/result/report/download/
  thumbnail/url, batches, SSE events), catalogue CRUD (presets/themes/templates), metering and
  analytics, admin cleanup, preview, the findings adapter, edit planning, the root banner, metrics,
  and the five health aliases (an executor is a batch pod; its liveness is the Job's
  `activeDeadlineSeconds`).

## Declared deviations

1. **Recording is virtual-time sampling, not a wall-clock screencast.** The reference records a live
   Playwright context for the session's duration. The port renders the page at successive
   `--virtual-time-budget` values and encodes the frames with ffmpeg: deterministic, reproducible,
   and no new dependency. A page whose content depends on real network or real wall-clock time will
   differ from the reference; a deterministic page matches frame for frame.
2. **`url` recording is opt-in.** A remote URL is refused by name (`egress_denied`) unless the
   request carries `allow_network: true`. P2.2 keeps egress denied by default; a deployment that
   wants URL recording grants it per kind (#83).

## Boot traps worth keeping

Migrations are `initContainer`-only (head `012`); the service reads **`API_PORT`**, not `PORT`;
`AUTH_ENABLED=false` is ignored and the trusted lane still refuses a tenantless request; the licence
gate's default is off; local storage plus `COMPILATION_ALLOW_LOCAL_FILE_INPUTS=true` is what makes a
capture possible without an object store; the VP9-alpha probe passes at boot and Chromium starts
lazily per job, not at boot.
