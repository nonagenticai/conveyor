# The youtube_upload_service contract

Captured 2026-11-15 from the reference image, not from a rebuild:

| | |
|---|---|
| image | `…/youtube_upload_service@sha256:e5f44636ccb5e4e78e375b99f28e744477fa2f9e26a2181cd3d8270c66ae798e` |
| operations | 22 over 20 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |
| cases | 26 (one or more per operation, plus the tenant-less 401) |

> The image was **not** present on the capture host despite the brief; the pinned digest was pulled
> once from the registry and every case below came from that digest. Nothing here reads or writes the
> cluster, and no request reached Google.

## How it was captured

`apps/media/contract/rig.sh` — one scratch `postgres:18` and one `redis` on localhost, the reference
run as its own image (amd64 under emulation). Six boot facts are load-bearing:

1. **Migrations run in an `initContainer`, not in the entrypoint.** The Deployment applies
   `alembic upgrade head` before the app starts (`services/w2v/deployments.yaml`). Locally, one-shot:
   `docker run --rm -e DATABASE_URL=… <image> alembic upgrade head` (14 revisions). Without it every
   `/api` route fails on a missing relation.
2. **Postgres and Redis are fatal at boot.** The rig supplies `DATABASE_URL` (psycopg v3 form) and
   `REDIS_URL`; the app refuses to start without either.
3. **`ENCRYPTION_KEY` is needed for the stored-token path.** `PUT /api/settings/oauth-client` encrypts
   a client secret, so the capture used a synthetic, valid Fernet key (32-byte urlsafe base64):
   `Y2FwdHVyZS1zeW50aGV0aWMta2V5LTMyLWJ5dGVzISE=`. OAuth client id/secret stayed synthetic; empty
   defaults are accepted at boot.
4. **`AUTH_TRUST_INCLUSTER_IDENTITY=true` + `TRUSTED_CIDRS` only gets a tenant past the middleware.**
   It does not satisfy the app's own service-account gate.
5. **The orchestrator gate needs the licence lane, stubbed locally.** `/api/settings*`, `/api/auth/*`
   and `/api/admin/*` accept only "the orchestrator service account": a cell token issued by
   `w2v-orchestrator`, or a licence-introspected subject in `AUTH_DELEGATING_SUBJECTS`. Cell tokens
   are capped at 60 s with jti replay protection, so they cannot pin static goldens. Instead the
   service was run with `LICENSE_INTROSPECT_URL` pointed at a local stub that answers
   `POST /license/introspect` with
   `{"active":true,"tenant_id":"00000000-0000-0000-0000-0000000000c1","subject_id":"00000000-0000-0000-0000-0000000000a1","license_id":"00000000-0000-0000-0000-0000000000b1","app":"w2v"}`
   and `AUTH_DELEGATING_SUBJECTS=00000000-0000-0000-0000-0000000000a1`. Cases then present the
   licence the way the orchestrator does: `X-Tenant-Id: Bearer <opaque>`.
6. **`X-Owner-Tenant-Id` is directional.** `/api/upload*`, `/api/settings*` and `/api/auth/*`
   **require** it (400 without, 403 if not asserted by the orchestrator SA); `/api/admin/*` **refuses**
   it (403 "operator routes are not owner-tenant scoped"). The cases pin both.

The scratch database must be freshly migrated before a capture: `upload_jobs_empty` runs before
`upload_submit` creates a job, and `auth_connect` runs before `settings_oauth_put` so the consent URL
carries no client id.

## What changes in the rewrite

- Every one of the 22 operations is **served** — nothing is dropped from the lock. This service is
  the publisher the orchestrator calls; the OAuth consent surface is part of that contract.
- **Google remains an external dependency and is never captured.** The consent URL is built locally
  (no call), but the code exchange, the token refresh and the upload itself are per-request Google
  calls. Cases stop at the local answers.

## Operation drop table

| operation | captured | note |
|---|---|---|
| GET / | 200 `root` | served |
| GET /health, /healthz, /health/live | 200 | served |
| GET /health/ready, /readyz | 200 | served |
| GET /api/auth/status | 200 `auth_status_not_connected` | served (no grant) |
| POST /api/auth/connect | 200 `auth_connect` | served; consent URL built locally, no Google call |
| DELETE /api/auth/disconnect | 404 `auth_disconnect_missing` | not-found branch served; connected happy path **dropped** — needs a live grant from a real Google round-trip |
| GET /api/auth/callback | 400 ×2 (no params, unknown state) | refusal branches served; code-exchange happy path **dropped** — it calls Google |
| POST /api/upload | 200 `upload_submit`, 422 validation | job creation served; progression to `completed`/`youtube_video_id` **dropped** — needs a connected channel and a real YouTube upload |
| GET /api/upload/jobs | 200 empty, 401 `upload_jobs_no_tenant` | served (empty list); a non-empty published listing **dropped** — same Google dependency |
| GET /api/upload/jobs/{job_id} | 404 `job_get_missing` | not-found served; a published job's 200 **dropped** — same |
| POST /api/upload/jobs/{job_id}/retract | 404 `retract_missing` | not-found served; the 200 retract **dropped** — needs a published video |
| POST /api/upload/jobs/{job_id}/privacy | 404, 422 validation | missing + validation served; the 200 release **dropped** — same |
| GET /api/settings | 200 `settings_get` | served |
| PUT /api/settings/oauth-client | 200 `settings_oauth_put` | served (synthetic client, encrypted) |
| DELETE /api/settings/oauth-client | 200 `settings_oauth_delete` | served |
| GET /api/admin/platform-channel | 200 `admin_platform_get` (null) | served for the unconfigured state |
| PUT /api/admin/platform-channel | 422 validation | happy path **dropped** — needs a real refresh token, and a synthetic one would arm platform-channel publishing (a Google path) for later cases |
| GET /api/admin/tenant-settings | 200 `admin_tenants_get` | served |
| PUT /api/admin/tenants/{tenant_id}/platform-channel | 200 `admin_tenant_put` | served |

## Tolerance

| case | fields | why |
|---|---|---|
| every error body | `request_id`, `timestamp` | per-request UUIDv7 and clock reading |
| `upload_submit` | `job_id`, `created_at`, `started_at`, `completed_at`, `status`, `error_message` | `job_id`/timestamps are generated per submission; the worker may move the job from `pending` to `failed` ("YouTube is not connected…") before the body is serialised |
| `auth_connect` | `state`, `authorization_url` | opaque CSRF state and PKCE `code_challenge` are fresh per request; `authorization_url` embeds both |

Everything else is compared exactly.

## Redaction

`auth_connect.body`'s `state` and the `state` parameter inside `authorization_url` are replaced
with `capture-state-redacted`. The reference mints a 43-character base64url value there, which is
indistinguishable from a licence token to a secret scanner — and `state` is declared volatile
anyway, so the capture loses no evidence. Everything else in the body is the reference's own.
