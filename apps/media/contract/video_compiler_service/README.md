# The video-compiler contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `…/video_compiler_service@sha256:9895faae83d370047f3aba4f67533e6df8f4ee9c80dd53d58faccbb3c4bd9322` |
| operations | 10 over 10 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |
| cases | 14 (`cases.txt`) |

> The commit that produced the image is not recorded: the image is the artefact and the digest is
> the pin. A digest that resolves in the registry is stronger evidence than a SHA nothing checks.

## How it was captured

`apps/media/contract/rig.sh` — one scratch `postgres:18` and one `redis` on localhost, and the
reference run as its own image (amd64 under emulation on this host; nothing else differs from the
pod).

### Boot settings that mattered

1. **The schema must exist before the app starts.** The Deployment applies migrations from an
   `initContainers` step, not from the entrypoint:
   `alembic -c /app/alembic.ini upgrade head`. It is not merely a warm-up — the app connects to
   Postgres during startup and its reaper immediately runs reclaim/prune SQL against the migrated
   schema, so a missing schema is a fatal boot rather than a 500 on a later request. Locally:
   `docker run --rm -e DATABASE_URL=postgresql+psycopg://… <image> alembic -c /app/alembic.ini upgrade head`
   (applied 001→010).
2. **One `DATABASE_URL` is enough in the rig.** The pod splits the two database roles across two
   Secrets — `MIGRATION_DATABASE_URL` for the DDL initContainer and `RUNTIME_DATABASE_URL` for the
   app — but both materialise as an env var named `DATABASE_URL`, which is what the rig supplies.
   The privileged/RLS split is a cluster privilege boundary, not a second application setting.
3. **`PORT`, not `API_PORT`.** The rig exports `API_PORT`; this service reads `PORT` (the
   ConfigMap sets `PORT=8200`). The capture passes `PORT=8200` explicitly.
4. **Auth is the trusted-network lane, and it still demands a tenant.**
   `AUTH_TRUST_INCLUSTER_IDENTITY=true` + `TRUSTED_CIDRS` (both supplied by `rig.sh`) is the lane
   that works; `AUTH_ENABLED=false` is ignored by the shared middleware. A request with **no**
   `X-Tenant-Id` is refused **401 `TENANT_REQUIRED`** — note the code differs from tts-gateway's
   `AUTHENTICATION_REQUIRED`, so this is pinned separately rather than copied.
5. **`METERING_REDIS_URL` may stay empty.** Boot logs a warning and the cost bus stays silent;
   it is not a boot dependency. Same for `MINIO_ENDPOINT`: startup is fine without it and
   `/health/ready` reports `"minio":"unreachable"`. Neither is needed by any operation captured
   here (see the drop table), so the capture runs with both unset — no cluster endpoint, no
   credentials.
6. **The licence gate was left at its code default (off).** The production ConfigMap arms
   `W2V_LICENSE_GATE_ENABLED=true` and points at an introspection URL; setting it in the rig would
   only capture the absence of that service. `ALLOW_LOCAL_FILE_INPUTS` also stays at its default.
7. **ffmpeg is in the image**, but no captured case reaches a render: the render branches need
   fetchable inputs and an object store (drop table below).

## Drop table

Every operation the reference serves, with its disposition. Nothing is silently omitted.

| operation | disposition | reason / branch captured |
|---|---|---|
| `GET /health` | **served** | captured 200 (reaper state included) |
| `GET /healthz` | **served** | captured 200; Kubernetes startup-probe target, same handler as `/health` |
| `GET /health/live` | **served** | captured 200; liveness-probe target |
| `GET /health/ready` | **served** | captured 200; readiness-probe target, reports store + database |
| `GET /readyz` | **served** | captured 200; alias of `/health/ready` |
| `POST /api/compile` | **served** | happy path captured: 202 `JobAccepted` from durable job state, plus 422 validation and 401 no-tenant. The **render** branch is not captured: it uploads the master to an object store, which the rig deliberately does not supply. |
| `GET /api/jobs/{job_id}` | **served** | not-found branch captured (404). A 200 job body needs a completed render — same object-store gap. |
| `POST /api/jobs/cancel` | **served** | captured 200 (`cancelled: []`, `killed: 0` — no live job under the prefix) and 422 validation |
| `POST /api/thumbnails` | **served** | validation branch captured (422). No happy path: it fetches `video_url` and the fetcher refuses internal/metadata hostnames, so a locally served fixture cannot be injected; output candidates also land in an object store. |
| `POST /api/concat-audio` | **served** | validation branch captured (422). Happy path needs fetchable audio inputs and an object store. |

Nothing was dropped. The four health endpoints and the two probes are not redundant: the probes
name three different paths, so the rewrite has to keep all of them.

## Tolerance

The goldens are exact except for these fields, which are generated per request and cannot replay:

- `request_id` (ULID) and `timestamp` (RFC 3339) in **every** error envelope
  (`{"error":{…}}`), including 401/404/422.
- `job_id` and `poll_url` in `compile_submit` (the id is minted per submission and `poll_url`
  embeds it).

One value is **environment-dependent rather than nondeterministic**: `/health/ready` and `/readyz`
report `"minio":"unreachable"` here because no object store was supplied. In the cluster it reads
`"reachable"`. The rewrite must report its own store state, so parity is the shape and the
`postgres` value, not the literal `minio` string.

## What changes in the rewrite

- **Health scaffolding is re-expressed**, not copied: `/health`, `/healthz`, `/health/live`,
  `/health/ready` and `/readyz` are pinned as parity targets because the Deployment's probes name
  `/healthz`, `/health/ready` and `/health/live`.
- **The render itself is asynchronous by contract.** `POST /api/compile` commits durable job state
  and answers 202 with a poll URL; the renderer, the lease and the reaper are the part that needs
  ffmpeg plus an object store, and they are outside what these goldens pin.
- **The Idempotency-Key dedup index is part of the store**, not of this HTTP surface: `cancel`
  takes a key *prefix*, so the header contract is exercised only by a caller that sends one.
