# video_compiler_service absorption — evidence (2026-11-15)

Issue #139 · the ffmpeg executor image (`conveyor-media-ffmpeg`). The reference is pinned by
digest; no private hostname, slug, address or customer payload appears in this file.

## The contract

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/video_compiler_service/` — taken 2026-11-15T16:06:38Z |
| Pinned image | `sha256:9895faae83d370047f3aba4f67533e6df8f4ee9c80dd53d58faccbb3c4bd9322` |
| Operations captured | 10 (10 paths) |
| Golden cases | 14 |
| Capability kinds after the drop table | 3 — `media.compile_video`, `media.thumbnail`, `media.concat_audio` |

Captured from the **running image**, not a rebuild: the rig (`apps/media/contract/rig.sh`) pulled
the digest above and booted it against a scratch `postgres:18` and redis on one Docker network.
Fixtures are synthetic — a generated 2-second mp4 and a generated WAV. No live customer payload was
used, and none could be: the rig has no route to an estate host.

## Parity accounting

`apps/media/contract/parity.toml` has one row per captured operation, and
`apps/media/tests/parity.rs` fails if a row is missing, duplicated, or names an operation no lock
carries.

- **3 served as kinds**: `POST /api/compile` → `media.compile_video`; `POST /api/thumbnails` →
  `media.thumbnail`; `POST /api/concat-audio` → `media.concat_audio`.
- **7 dropped with a reason**: the four health endpoints and `/readyz` (an executor is a batch pod,
  not a service; its liveness is the Job's `activeDeadlineSeconds`), `/api/jobs/{job_id}` and
  `/api/jobs/cancel` (job status and cancellation are conveyor's ledger, ADR 0007 §2–4).

The goldens pin the **submit and validation** branches. The render branch uploaded its master to an
object store, which the rig deliberately does not supply, so the executor's own tests carry that
evidence instead: a real compile, a real thumbnail and a real concat against local `ffmpeg`.

## Tolerances

The captured error envelopes carry `request_id` (ULID) and `timestamp`, and `compile_submit`
carries a minted `job_id` and `poll_url`. Those are the only fields that cannot replay; the parity
table names each, and everything else in a golden is compared exactly.

## What the rewrite does

The reference answered 202 and rendered asynchronously behind a lease and a reaper. The executor
does the work **inside one job**: inputs arrive as files in the work directory, the encoded master
leaves as a declared output through conveyor's own upload route. Anything the reference offered
that this port does not (grading, caption burn-in, ambient audio bed, cursor/focus tracks) is
refused by name with a stable code — never accepted and ignored.
