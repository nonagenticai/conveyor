# The tts-gateway contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `…/tts_gateway@sha256:8068472f07537acb42b2e5d26e7ec0b05b3da223a5ab5d9ac9dc25ba96428d99` (live pod `tts-gateway-5f7dcfc5b7-hlhm4`) |
| operations | 11 over 10 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |

> The commit that produced the image is not recorded: the image is the artefact and the digest is
> the pin. A digest that resolves in the registry is stronger evidence than a SHA nothing checks.

## How it was captured

`apps/media/contract/rig.sh` — one scratch `postgres:18` and one `redis`, both on localhost, and
the reference run as its own image (amd64 under emulation on this host; nothing else differs from
the pod). Three settings are load-bearing and are the ones a reader will otherwise rediscover:

1. **Migrations do not run in the image's entrypoint.** The Deployment applies them from an
   `initContainers` step (`alembic upgrade head`, `services/w2v/deployments.yaml`). Locally:
   `docker run --rm -e DATABASE_URL=… <image> alembic upgrade head`. Without it every `/api` route
   answers 500 `relation "tts_batch_jobs" does not exist` — which is what the first capture attempt
   recorded before the schema existed.
2. **`AUTH_ENABLED=false` does not disable auth.** The service passes its own override to the
   factory. The lane that works is `AUTH_TRUST_INCLUSTER_IDENTITY=true` + `TRUSTED_CIDRS` covering
   the Docker bridge, and it is also the honest one: a request with **no** `X-Tenant-Id` is still
   refused (401 `AUTHENTICATION_REQUIRED`, captured as `batches_get_no_tenant`). The tenant is
   supplied by header on every case; no default tenant exists in the reference.
3. **The worker surface is an allowlist.** `claim`, `heartbeat`, `release`, `result` and `fail` are
   refused 403 `worker surface is not configured` until `WORKER_SUBJECT_IDS` names a subject. Under
   the trusted-network lane the synthetic subject **is** the tenant id, so the capture runs with
   `WORKER_SUBJECT_IDS=<the capture tenant>`.

## What changes in the rewrite

- **Health scaffolding** (`/health`, `/livez`, `/readyz`) is re-expressed through the workspace's
  own health shape rather than copied; the three cases pin the reference's answers as the parity
  target.
- **The model backend stays remote.** `tts_backend`/kokoro on the mini hosts is a model server and
  is not absorbed (ADR 0006 §Consequences): `claim` carries `provider: "kokoro"` and the worker,
  not the gateway, calls the backend.
- **Every one of the 11 operations is served.** Nothing is dropped: the batch submit/poll pair is
  what a producer calls, and the claim/heartbeat/result/release/fail set is what the remote workers
  call — the conveyor replaces the producer, not the fleet.

## Tolerance

`job_id` (ULID) and `idempotency_key` (sha256) are generated per submission and cannot replay;
they are declared in `tolerances.toml` and compared by shape only. Everything else is exact.
