# tts_gateway capture — evidence (2026-11-15)

Issue #139 · **captured here, implemented in the thin-operations unit.** ADR 0007 §1 hosts thin
operations in `apps/server` beside the job ledger, and its sequence line puts them after the two
media images; this file records the contract the later unit will be held to. The reference is
pinned by digest; no private hostname, slug, address or customer payload appears here.

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/tts_gateway/` — taken 2026-11-15T15:58:21Z |
| Pinned image | `sha256:8068472f07537acb42b2e5d26e7ec0b05b3da223a5ab5d9ac9dc25ba96428d99` |
| Operations captured | 10 (10 paths) |
| Golden cases | 13 |

## What the capture pins

- Synthesis batches (`POST /api/tts/batches`, `GET /api/tts/batches/{batch_id}`) and the worker
  surface (`claim`, `heartbeat`, `release`, `result`, `fail`) — the gateway's fan-out to the model
  hosts, with the degraded shape when a worker is unreachable.
- Health (`/health`, `/livez`, `/readyz`).

Synthetic fixtures only: a known voice and a two-word text. The three boot traps that cost time are
recorded in `apps/media/contract/tts_gateway/README.md` (initContainer migrations,
`AUTH_ENABLED=false` being ignored, and the worker surface being an allowlist keyed on the subject).

## Why it is not in this unit

The gateway keeps no state of its own; it is a thin HTTP fan-out. In conveyor it runs beside the job
ledger in `apps/server`, which already owns the queue, the retries and the artefact routes — the
home ADR 0007 §1 gives it. Landing it in the media images would put a scheduler in a pod.
