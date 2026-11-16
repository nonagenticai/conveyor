# The video-analyzer-service contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `video_analyzer_service@sha256:5836071586adf4035825c1076c84adfec94c2f1d762adc979fffb94d223ca29d` (linux/amd64 under emulation on this arm64 host; nothing else differs from the pod) |
| operations | 14 over 13 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |
| goldens | 16 cases in `cases.txt` -> `golden/` |

> The cluster Deployment pinned a different digest on the same day
> (`sha256:60a01bdb…`). The digest above is the artefact this capture was taken from; re-verify which
> one the registry and the Deployment agree on before treating either as the rewrite's target.

## How it was captured

`apps/media/contract/rig.sh` — scratch Postgres 18 and Redis on a private docker network, the
reference run as its own image, and one extra one-shot container (`cap-media`, the same image with
`python -m http.server`) serving two locally generated 2-second `testsrc` files (320x240 @ 10fps;
the audio variant adds a 440 Hz sine track). Five settings are load-bearing:

1. **Migrations do not run in the image's entrypoint.** The Deployment applies them from an
   `initContainers` step (`alembic upgrade head`, `services/w2v/deployments.yaml`). Locally:
   `docker run --rm -e DATABASE_URL=… <image> alembic upgrade head` (7 revisions,
   `001_initial` -> `007_durable_reclaim`). Without it every DB route answers 500.
2. **`TRANSCRIPTION_CLIENT_URL` is required at boot.** It is pointed at an unreachable local
   address (`http://127.0.0.1:9`) and only branches that never call it are captured; the submit case
   switches the external stages off so the pipeline completes without it.
3. **`ALLOWED_URL_HOSTS=["cap-media"]`.** The SSRF guard refuses private/reserved addresses before
   fetching `video_url`/`audio_url`; the exact-host allowlist is the reference's own bypass (the pod
   uses it for its object store). Without it the synthetic file is refused and nothing is analysed.
4. **`AUTH_TRUST_INCLUSTER_IDENTITY=true` + `TRUSTED_CIDRS`** (rig defaults) is the lane that works;
   a request with **no** `X-Tenant-Id` is still refused (401 `TENANT_REQUIRED`, captured as
   `audio_no_tenant`). The tenant is supplied by header on every authenticated case.
5. **`vision_provider: "fake"` does NOT avoid the WM backend.** Measured: the job still failed at
   `scene_detection` asking for an `inference-router` licence token. The capturing submit therefore
   turns the external stages off (`extract_frame_descriptions`, `extract_ocr`,
   `generate_ui_inventory`, `generate_walkthrough`) and the job completes end-to-end locally: probe
   (2.0 s, 320x240), two extracted frames, quality metrics, frame-index artefact.

`METERING_REDIS_URL` defaults empty (no cost events); the numba beat warm-up is non-fatal (first
healthy answer ~60 s after start). Chromium is not used. `STORAGE_BACKEND` defaults to local; no
MinIO and no LLM tokens are needed.

## The analysis that the goldens pin

- `analyze_submit` is a real 202 for a real job over the synthetic mp4 (the job was separately
  driven to `completed` from this exact body before the goldens were taken: real frames, real
  sharpness/brightness/motion metrics, frame-index artefact).
- `audio_beat_grid` is the reference's **synchronous** route: the golden body is an actual beat grid
  computed from the synthetic file (`librosa.beat_track+onset_phase`, 147.66 bpm over 2.02 s).

## Every operation: served or dropped

**Nothing is dropped.** All 14 operations are served by the rewrite; what differs is whether the
goldens can replay them.

| operation | status | reason |
|---|---|---|
| `GET /` | served, captured | root manifest |
| `GET /health`, `/health/live`, `/health/ready`, `/healthz`, `/readyz` | served, captured | health scaffolding (re-expressed through the workspace's health shape; these cases pin the answers) |
| `POST /api/analyze` | served, captured (202) | real submit; the job lifecycle itself is not replayable from a static cases file |
| `POST /api/analyze/audio` | served, captured (200) | synchronous real beat grid |
| `POST /api/analyze/upload` | **served, uncaptured** | multipart/form-data; `capture-goldens.sh` sends an `application/json` body only, so this format cannot express it. Parity unverified — the rewrite still owes it |
| `GET /api/analyze/{job_id}` | served, captured at not-found | job ids are server-generated ULIDs; a static path can only pin the 404 branch |
| `GET /api/analyze/{job_id}/result` | served, captured at not-found | same |
| `GET /api/analyze/{job_id}/verification` | served, captured at not-found | same |
| `GET /api/analyze/{job_id}/artifact/{name}` | served, captured at not-found | same |
| `DELETE /api/analyze/{job_id}` | served, captured at not-found | same |

Successful job-scoped responses are documented here (see above) but are not golden-replayable; the
rewrite's own tests must cover the success path.

## Tolerance

Per-submission `job_id` (ULID) in `analyze_submit.body` is generated and cannot replay; compare by
shape only. Every error envelope carries a per-request `request_id` (ULID) and `timestamp`
(ISO-8601 UTC) — also shape-only. Everything else is exact: `status`/`stage`/`progress` in the 202
are constants, the beat-grid numbers are deterministic for the pinned image and the fixed input, and
the health/validation/not-found bodies are stable.

A replay must start the same one-shot `cap-media` file server on the rig network with both synthetic
files, or the two analysis cases cannot fetch their input.
