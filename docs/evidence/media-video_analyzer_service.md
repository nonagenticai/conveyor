# video_analyzer_service absorption — evidence (2026-11-15)

Issue #139 · the ffmpeg executor image (`conveyor-media-ffmpeg`). The reference is pinned by
digest; no private hostname, slug, address or customer payload appears in this file.

## The contract

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/video_analyzer_service/` — taken 2026-11-15T16:11:54Z |
| Pinned image | `sha256:5836071586adf4035825c1076c84adfec94c2f1d762adc979fffb94d223ca29d` |
| Operations captured | 14 (13 paths) |
| Golden cases | 16 |
| Capability kinds after the drop table | 4 — `media.probe_video`, `media.analyze_video`, `media.analyze_audio`, `media.ocr` |

Captured from the running image against a scratch `postgres:18` and redis, with synthetic fixtures
(a 2-second 320×240 mp4 and a generated audio file). The job was separately driven to `completed`
from the captured submit body before the goldens were taken, so `analyze_submit` is a real 202 for
a real job, and `audio_beat_grid` is a real beat grid computed from the synthetic file.

## Parity accounting

`apps/media/contract/parity.toml` carries one row per captured operation; `apps/media/tests/parity.rs`
enforces it in both directions.

- **3 served as kinds**: `POST /api/analyze` and `POST /api/analyze/upload` → `media.analyze_video`;
  `POST /api/analyze/audio` → `media.analyze_audio`. (`upload` is served but uncaptured: the capture
  script sends JSON only, and multipart/form-data cannot be expressed; the executor takes the same
  bytes as a work-dir input.)
- **11 dropped with a reason**: health scaffolding and `/` (an executor is a batch pod with no HTTP
  surface), and every job-scoped read/delete route (`/api/analyze/{job_id}`, `/result`,
  `/verification`, `/artifact/{name}`, `DELETE`) because the job ledger, the result route and the
  artefact store are conveyor's (ADR 0007 §2–4).
- **2 kinds are new, not ported**: `media.probe_video` (the reference exposes probe data inside its
  analysis document; conveyor publishes it as its own kind so a caller can decide before spending a
  job) and `media.ocr` (ADR 0007 §1 puts tesseract in the image; the reference's OCR is a
  licence-gated vision-provider call, so no reference operation evidences a local one).

## Declared deviation — OCR

The captured job runs with `extract_ocr` **off**, because that stage calls the vision provider behind
`inference-router` and the rig has no licence token. The port's `media.ocr` is therefore a
tesseract-based **new** capability, not a parity claim, and its tests assert its own behaviour
(a real image, its text) rather than a golden.

## What the rewrite does

The reference was an asynchronous analysis service with a lease and a reaper. The executor does the
analysis inside one job and posts one result document. Measurements derivable from ffprobe/ffmpeg
output are reproduced in Rust; any parameter whose reference implementation reads raw pixels through
OpenCV is refused by name with a stable code.
