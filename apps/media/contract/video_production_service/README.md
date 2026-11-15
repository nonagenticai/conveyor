# The video-production-service contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `…/video_production_service@sha256:016a2db10e4984813e138f280c07bf5e9fb6cd7053089b78fa7c315b12e72cd6` |
| operations | 51 over 47 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |
| cases | 51 (`cases.txt`) |
| fixtures | `fixtures/` — synthetic only: `page.html`, `animation.html`, `clip.mp4` |

The running pod's `status.containerStatuses[*].imageID` is this same digest, so every statement
below is about production, not only about the rig.

## What this service is

The largest member of the media group and the only one with two distinct execution shapes:

1. **Production** — deterministic video assembly: ffmpeg composition of a shot list, Pillow title
   cards, and a frame-stepped animation renderer that screenshots PNG frames and encodes them.
2. **Browser recording** — a live headless Chromium session captured in wall-clock real time
   (Playwright `record_video_dir`), plus a screenshot-capture mode that returns a zip of PNGs.

That split is the whole kind decomposition (`capabilities.txt`).

## How it was captured

`apps/media/contract/rig.sh` — one scratch `postgres:18` and one `redis` on localhost, and the
reference run as its own image (amd64 under emulation on this host; nothing else differs from the
pod). Host port 18202 → container 6308.

Migrations are **not** run by the entrypoint. The Deployment runs them in an initContainer with
the privileged URL; locally the equivalent is:

```
docker run --rm --network conveyor-capture \
  -e DATABASE_URL='postgresql+psycopg://cap:cap@conveyor-media-pg:5432/<db>' \
  <image> alembic upgrade head
```

This image's head is **012** (`012_recording_max_frame_seconds`); 001→012 applied cleanly.

### Boot settings that worked

The rig supplies `DATABASE_URL`, `REDIS_URL`, `API_PORT` (= container port 6308),
`ENVIRONMENT=development`, `AUTH_TRUST_INCLUSTER_IDENTITY=true` and the rig's default
`TRUSTED_CIDRS`. The capture added only:

```
STORAGE_BACKEND=local
STORAGE_LOCAL_PATH=/tmp/vps-data
COMPILATION_ALLOW_LOCAL_FILE_INPUTS=true
OTEL_SDK_DISABLED=true
```

1. **`API_PORT`, not `PORT`.** The image's `python -m src.asgi` serves `settings.api.port`, read
   from `API_PORT` (the ConfigMap sets 6308; the code default is also 6308).
2. **The trusted-network auth lane still demands a tenant.** `AUTH_ENABLED=false` is ignored by
   the shared middleware. A request with no `X-Tenant-Id` is **401 `TENANT_REQUIRED`** — the same
   code as the video-compiler capture, not tts-gateway's `AUTHENTICATION_REQUIRED`.
3. **Storage is local, deliberately.** Production sets `STORAGE_BACKEND=minio` and an internal
   endpoint; the captured operations write their outputs through the storage facade, and the local
   backend exercises that same facade without a cluster endpoint. Nothing here needs MinIO.
4. **`COMPILATION_ALLOW_LOCAL_FILE_INPUTS=true`** opens the sink-layer local-file gate so a
   synthetic clip can be supplied as a bare path. With the gate closed, a local ref is accepted
   only inside `STORAGE_LOCAL_PATH`; that is the production posture and the default.
5. **The licence gate was left at its code default (off).** Arming it in the rig would only
   capture the absence of the licences service. `AUTH_LICENSE_APP_KEYS` was also left unset (the
   image logs a warning and accepts any app key; it is a licence-lane check, not a boot setting).
6. **`TENANT_RLS_MODE` was left at its default (`enforce`).** The rig's `cap` role is a Postgres
   superuser, so RLS is bypassed at the SQL layer while the HTTP tenant contract is still enforced.
   Production connects as the non-owner `w2v_video_production_app` role; the difference is a
   database privilege boundary, not an application setting.
7. **No Chromium is started at boot.** Measured: startup wires `RecorderService` and
   `CompilerService` lazily; a browser launches per recording job. The expected "Chromium pool at
   boot" was not observed. Boot to a 200 on `/health` was **~1.5 s**, not minutes: the VP9-alpha
   probe drives one real ffmpeg encode and logged `VP9 alpha capability: OK` immediately. The
   10-minute allowance was not needed.

Fixtures were copied into the container at `/tmp/fixtures`:

- `clip.mp4` — 2.0 s, 320×240 @ 10 fps, H.264 (ffmpeg `testsrc`), 10 890 bytes;
- `page.html` — a one-page self-contained document, no external resources;
- `animation.html` — a self-contained template exposing `window.RENDER_DURATION_MS` and
  `window.render(t)`.

## Measured reference defect: `POST /api/compile` cannot complete a job

This is not a rig artefact. Every compile job accepted by this digest dies immediately in the
background task:

```
AttributeError("'CompilationJob' object has no attribute 'fps'")
  /app/src/services/compilation/compiler_service.py:525 in _initialize
```

`_initialize` passes `fps=job.fps` into preset resolution, unconditionally. The image's
`CompilationJob` ORM model has **no `fps` column** (verified inside the container:
`hasattr(CompilationJob, "fps") is False`). In the source repo, commit `1ccc2ff` added
`fps=job.fps`; the model column arrived later, after this image was built. The job row is left at
`status="processing"` forever — the background task is gone, so the reaper eventually fails it.

The same digest runs in production (`imageID` checked), therefore **no compile request can ever
complete on the live reference today**. The likely reason this has gone unnoticed is that the
orchestrator's compile traffic goes elsewhere; the defect is invisible until someone calls this
service's `/api/compile`.

Consequences for this capture:

- `compile_submit`, `compile_findings_video` and the job-scoped routes are pinned at their HTTP
  shape (submission accepted; not-found branches), not at a completed render. There is no way to
  capture a completion pair from this image.
- The **intended** pipeline is still measurable through the routes that do run: `title-card` and
  `record/animation` perform real render/encode work, and the source shows the same ffmpeg
  composition stages.
- The rewrite must treat `POST /api/compile` as intake-only and must **not** copy the `fps` defect.

## Can the pipeline be reproduced as per-shot PNG render + ffmpeg encode?

Measured, not inferred:

| path | mechanism | measured result | needs continuous capture? |
|---|---|---|---|
| `POST /api/record/animation` | frame-stepped: seek `t`, screenshot PNG per frame, ffmpeg-encode the sequence | 320×240 @ 10 fps, 1.0 s → 10 PNG frames → H.264 1.000 s + silent AAC, 3 628 B, 3.7 s wall clock (capture raster 640×480, scale 2) | **no** |
| `POST /api/record/content` | Playwright `record_video_dir` screencast of a live context, then post-process | inline one-page HTML → 320×240, 1.8 s @ 25 fps, H.264 + AAC, 26 807 B, ~3.8 s wall clock | **yes** |
| `POST /api/compile` (intended) | per-part normalize/edit/title cards (Pillow or the same animation renderer) → concat/xfade → effects → final mp4 | intake + background start measured; completion blocked by the `fps` defect | **no**, once per-shot assets exist |

**Answer:** the production pipeline (compile + animation + title cards) is reproducible as a
per-shot render (PNG frames or a supplied clip) plus ffmpeg encode, and the animation route proves
that path works end to end in this image. Live browser recording is not: its timeline is
wall-clock-driven and requires a running Chromium for the recording duration. That is why the
decomposition is two kinds and not one.

Supporting evidence in the source: `src/services/recording/animation_recorder.py` steps frames and
encodes them; `src/services/recording/content_recorder.py` records the context; the compile
segment builders dispatch `title_card` (Pillow or `AnimationRecorder`), `slide_preview` /
`generated_image` / `comparison_frame` (image → motion clip) and `browser_recording` (supplied
clip), then assemble with ffmpeg.

## Route → kind mapping

Only the intake routes of the two kinds are absorbed. Everything the conveyor's job ledger owns
(polling, status, cancel, result, download, batches) is dropped, as are CRUD catalogs and
observability.

| route | kind |
|---|---|
| `POST /api/compile` | `media.produce_video` |
| `POST /api/compile/title-card` | `media.produce_video` |
| `POST /api/record/animation` | `media.produce_video` |
| `POST /api/record/content` | `media.record_browser` |
| `POST /api/record/url` | `media.record_browser` |
| `POST /api/record/part` | `media.record_browser` |
| `POST /api/record/capture/screens` | `media.record_browser` |

## Drop table

Every operation in `operations.txt`, with its disposition. Nothing is silently omitted.

| operation | disposition | reason |
|---|---|---|
| `GET /health` | **served** (scaffolding) | probe parity; reports DB/Redis/storage/concurrency readiness, same handler family as the rewrite's health contract |
| `GET /healthz` | **served** (scaffolding) | Kubernetes startup-probe target |
| `GET /health/live` | **served** (scaffolding) | liveness-probe target |
| `GET /health/ready` | **served** (scaffolding) | readiness-probe target |
| `GET /readyz` | **served** (scaffolding) | alias of `/health/ready` |
| `POST /api/compile` | **kind: `media.produce_video`** | shot list + supplied recordings → assembled deliverable; pinned at intake (completion blocked by the `fps` defect) |
| `POST /api/compile/title-card` | **kind: `media.produce_video`** | single title-card shot render (Pillow → mp4); happy path captured |
| `POST /api/record/animation` | **kind: `media.produce_video`** | frame-stepped PNG render + ffmpeg encode; happy path measured end to end |
| `POST /api/record/content` | **kind: `media.record_browser`** | HTML content → continuous browser recording; happy path measured |
| `POST /api/record/url` | **kind: `media.record_browser`** | live URL → continuous browser recording; the rig pins the SSRF/validation branches (happy path needs an external public URL) |
| `POST /api/record/part` | **kind: `media.record_browser`** | scripted browser part; pinned at validation (needs a scripts directory) |
| `POST /api/record/capture/screens` | **kind: `media.record_browser`** | one screenshot per demo step → zip; pinned at the SSRF/validation branch (needs a public URL) |
| `GET /api/compile/jobs` | **drop** (ledger) | job list/polling is the conveyor ledger's; the executor surfaces intake only |
| `GET /api/compile/jobs/{job_id}` | **drop** (ledger) | status polling |
| `GET /api/compile/jobs/{job_id}/report` | **drop** (ledger) | per-job stage report |
| `GET /api/compile/jobs/{job_id}/result` | **drop** (ledger) | result metadata |
| `GET /api/compile/jobs/{job_id}/thumbnail` | **drop** (ledger) | artifact read of a ledger-owned job |
| `POST /api/compile/jobs/{job_id}/thumbnail` | **drop** (ledger) | artifact write against a ledger-owned job |
| `GET /api/compile/jobs/{job_id}/video` | **drop** (ledger) | artifact download |
| `GET /api/compile/jobs/{job_id}/video-url` | **drop** (ledger) | artifact URL minting |
| `DELETE /api/compile/jobs/{job_id}` | **drop** (ledger) | cancel is a ledger transition |
| `POST /api/compile/batch` | **drop** (ledger) | fan-out of several compiles is ledger orchestration |
| `GET /api/compile/batch/{batch_id}` | **drop** (ledger) | batch status |
| `POST /api/compile/preview` | **drop** (preview) | derived preview of a ledger-owned job output; not a capability |
| `POST /api/compile/audio-export` | **drop** (ledger artifact) | extracts audio from a ledger-owned job's output by `job_id` |
| `POST /api/compile/edit-plan` | **drop** (planning) | LLM edit planning; the ledger/orchestrator owns planning, and the response is a plan, not media |
| `POST /api/compile/findings-video` | **drop** (adapter) | findings → shot-list shaping is presentation policy; the caller/ledger supplies the shot list to `media.produce_video` |
| `POST /api/compile/admin/cleanup` | **drop** (admin) | retention/maintenance; ledger-owned |
| `GET /api/compile/presets` | **drop** (catalogue) | static config data; the executor receives explicit config in the job request |
| `GET /api/compile/presets/{name}` | **drop** (catalogue) | static config lookup |
| `GET /api/compile/presets/custom` | **drop** (catalogue) | per-tenant CRUD rows belong to the config/ledger store |
| `POST /api/compile/presets/custom` | **drop** (catalogue) | per-tenant CRUD |
| `DELETE /api/compile/presets/custom/{preset_id}` | **drop** (catalogue) | per-tenant CRUD |
| `GET /api/compile/themes` | **drop** (catalogue) | static theme catalogue |
| `GET /api/compile/themes/{name}` | **drop** (catalogue) | static theme lookup |
| `GET /api/compile/templates` | **drop** (catalogue) | template catalogue (seeded per boot) |
| `POST /api/compile/templates` | **drop** (catalogue) | per-tenant CRUD |
| `DELETE /api/compile/templates/{template_id}` | **drop** (catalogue) | per-tenant CRUD |
| `GET /api/record/jobs` | **drop** (ledger) | recording job list/polling |
| `GET /api/record/jobs/{job_id}` | **drop** (ledger) | recording status |
| `GET /api/record/jobs/{job_id}/result` | **drop** (ledger) | recording result metadata |
| `GET /api/record/jobs/{job_id}/video` | **drop** (ledger) | artifact download |
| `POST /api/record/jobs/{job_id}/cancel` | **drop** (ledger) | cancel is a ledger transition |
| `POST /api/record/batch` | **drop** (ledger) | recording fan-out |
| `POST /api/record/batch_from_artifacts` | **drop** (ledger) | fan-out plus artifact retrieval; ledger orchestration |
| `GET /api/record/batch/{batch_id}` | **drop** (ledger) | batch status |
| `GET /api/analytics/usage` | **drop** (metering) | usage aggregation is billing/metering, ledger-owned |
| `GET /api/analytics/usage/daily` | **drop** (metering) | daily usage aggregation |
| `GET /events/{tenant_id}` | **drop** (observability) | SSE job-progress stream; ledger-owned and not a request/response pair |
| `GET /metrics` | **drop** (observability) | Prometheus exposition scaffolding; operational, not a business operation |
| `GET /` | **drop** (informational) | endpoint index advertising routes the executor will not serve |

Count: 7 absorbed (3 produce, 4 record), 5 health scaffolding, 39 dropped.

## Tolerance

The goldens are exact except for these fields, which are generated per request and cannot replay:

- `request_id` (ULID) and `timestamp` (RFC 3339) in **every** error envelope (`{"error":{…}}`),
  including 401/404/422.
- `job_id` and `poll_url` in every async submission (`compile_submit`, `compile_findings_video`,
  `record_animation`, `record_content`); `poll_url` embeds the minted id.
- `video_path` in `compile_title_card`: it embeds a fresh `titlecard-<uuid7>` directory.
- `created_at` in `compile_templates`: the built-in rows are seeded per process boot, so the
  timestamp is this boot's, not a fixture constant. Every other template field is static.
- `/health` `concurrency` block is live state (`active`, `queued`, `available_slots`); it reads 0
  in an idle rig.
- `analytics_usage` counters: written only when a job reaches a terminal state, so they read zero
  here; a replay against a live ledger must compare shape, not counts.

One value is **environment-dependent rather than nondeterministic**: `/health` reports storage
`healthy` through the local backend here; production reports the same field through MinIO. The
parity target is the shape and the `database`/`redis` values, not the backend name.

## What changes in the rewrite

- **The job ledger owns the lifecycle.** The executor gets seven intake routes and the health
  scaffolding; polling/status/cancel/result/download/batches are not ported.
- **`media.produce_video` is deterministic and asset-driven.** It must accept a shot list with
  explicit assets and return the deliverable. The reference's intended compile stages are
  `initialize → process_segments → assemble_video → apply_effects → finalize`, where each segment
  is either a supplied clip, a Pillow title card, a frame-stepped animation clip, or an image
  motion clip. The animation path is the measured proof that per-shot PNG + ffmpeg works.
- **`media.record_browser` needs a real browser for the recording duration.** It must not be
  modelled as a per-shot render: the reference records the browser context in wall-clock time.
- **Do not copy the `fps` defect.** Any compile port must either carry the model column and the
  preset plumbing together, or not read the field at all.
- **Health scaffolding is re-expressed, not copied**, because the Deployment's probes name
  `/healthz`, `/health/ready` and `/health/live` (startup, readiness, liveness respectively).
- **Auth is the trusted-network lane with a mandatory tenant**, and `401 TENANT_REQUIRED` is the
  pinned refusal, not a silently disabled auth mode.
