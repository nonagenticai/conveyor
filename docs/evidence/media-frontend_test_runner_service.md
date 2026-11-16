# frontend_test_runner_service absorption — evidence (2026-11-15)

Issue #139 · the browser executor image (`conveyor-media-browser`). The reference is pinned by
digest; no private hostname, slug, address or customer payload appears in this file.

## The contract

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/frontend_test_runner_service/` — taken 2026-11-15T19:13:16Z |
| Pinned image | `sha256:db276ca39dd5021f06ee634aadcf90ba584f894e3d87b061ab15211f2350d6a3` |
| Operations captured | 12 (12 paths) |
| Golden cases | 14 |
| Capability kinds after the drop table | 1 — `media.run_gherkin` |

The lock was taken from the running process and is **byte-identical to the live pod's**. The rig
could not pull this image at a usable rate (~0.25 MB/s over the site uplink; a containerd export
truncated at 99 %), so the reference ran on another node's containerd against the rig's scratch
Postgres and Redis over SSH tunnels. The digest is the same; only the host differed.

## The one absorbed kind, and the contract it changes

The reference's `POST /api/test-runner/execute` accepts a **Playwright suite**: a ZIP
`package_url` or a `bdd_job_id` pointing at the retired `bdd-playwright-service`, plus suite knobs
(`browsers`, `workers`, `grep_pattern`, `expect_recording`) — and `screenshot_on_failure` was
**accepted and ignored**. ADR 0007 §1 gives the browser image a **native Gherkin runner**, so the
kind takes the feature document itself (`feature` + `target_url`) and keeps only `timeout_per_test_ms`
and `retries`. `screenshot_on_failure` is either implemented or omitted, never accepted and ignored
again; the suite and recording fields are refused by name.

**Declared gap:** the step vocabulary is navigation and assertions, executed with the image's own
Chromium (`--dump-dom`). Interaction steps (click, type, scroll, hover, upload) are refused by name
with `step_not_supported` until this image carries a CDP client — the one dependency the workspace
does not have and this unit did not add. `trace_on_failure` is refused for the same reason.

## Parity accounting

`apps/media/contract/parity.toml` has one row per captured operation; `apps/media/tests/parity.rs`
enforces it in both directions.

- **1 served as a kind**: `POST /api/test-runner/execute` → `media.run_gherkin`, with the changed
  request source recorded in the row's note.
- **11 dropped with a reason**: the live job feed and result/report routes (conveyor's ledger), the
  artefact route (conveyor's store), the per-scenario clip video (a demo-video product, not a
  capability), the root banner, and the five health aliases (an executor is a batch pod; its
  liveness is the Job's `activeDeadlineSeconds`).

## Boot traps worth keeping

Migrations are the Deployment's `initContainer` (`alembic upgrade head`, 001→009); the service reads
**`API_PORT`**, not `PORT`; the trusted-network lane needs `AUTH_ENABLED=true` and a `TRUSTED_CIDRS`
that contains the caller's source — a loopback caller needs `127.0.0.1/32` or it gets a JWT
`AUTHENTICATION_REQUIRED` instead; a missing `X-Tenant-Id` is 401 `TENANT_REQUIRED`; the licence
gate defaults off; no object store is needed (local storage); `TENANT_RLS_MODE` was left unset.
