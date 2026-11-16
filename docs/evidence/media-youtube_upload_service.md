# youtube_upload_service capture — evidence (2026-11-15)

Issue #139 · **captured here, implemented in the thin-operations unit.** ADR 0007 §1 hosts thin
operations in `apps/server`; this file records the contract that unit will be held to. The
reference is pinned by digest; no private hostname, slug, address or customer payload appears here.

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/youtube_upload_service/` — taken 2026-11-15T16:04:37Z |
| Pinned image | `sha256:e5f44636ccb5e4e78e375b99f28e744477fa2f9e26a2181cd3d8270c66ae798e` |
| Operations captured | 22 (20 paths) |
| Golden cases | 26 |

## What the capture pins

- OAuth connect/status/callback/disconnect, and the per-tenant OAuth client settings.
- Upload jobs: submit, list, read, privacy change, retract.
- The platform-channel administration surface.
- Health (`/health`, `/health/live`, `/health/ready`, `/healthz`, `/readyz`).

Synthetic fixtures only: a fake OAuth token and a two-second mp4. One value is **redacted in the
golden**: OAuth `state` (43 base64url characters shape-match a licence token to a secret scanner).
The field is volatile, so no evidence is lost.

## Why it is not in this unit

Uploading is a thin HTTP call to an external provider plus a credential store — no browser, no
ffmpeg, no local media work. Its home is `apps/server`, beside the job ledger that already owns the
job rows, and ADR 0007's sequence puts thin operations after the media images.
