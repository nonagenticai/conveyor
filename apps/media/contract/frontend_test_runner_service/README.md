# The frontend-test-runner contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `frontend_test_runner_service:latest@sha256:db276ca39dd5021f06ee634aadcf90ba584f894e3d87b061ab15211f2350d6a3` (the registry host is private; the digest is the pin) |
| created | 2026-10-27T06:05:51Z (`linux/amd64`) |
| operations | 12 over 12 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |
| cases | 14 (`cases.txt`) |

> The commit that produced the image is not recorded: the image is the artefact and the digest is
> the pin. A digest that resolves in the registry is stronger evidence than a SHA nothing checks.

## How it was captured

The reference was booted as its **exact digest** against the card's scratch `postgres:18` and
`redis` (the rig's `conveyor-media-pg` / `conveyor-media-redis`), with the boot steps copied from
the Deployment:

- the schema was applied by the image itself, as the Deployment's `run-migrations` initContainer
  does (`alembic -c /app/alembic.ini upgrade head`, migrations 001→009);
- the app was started with its own entrypoint, `/app/.venv/bin/python3.14 -m src.asgi`, on
  container port **6316** exposed as **18203**;
- the HTTP calls were made against that instance, with the tenant header explicit on every
  tenant-scoped case.

The one deviation is the container runtime: the image could not be pulled into this host's Docker
daemon in time (the private registry serves the 747 MB Chromium layer at ~0.25 MB/s, and a
containerd export stream of the 940 MB image truncated at ~99% with `unexpected EOF`). Instead the
digest already unpacked on the cluster node that runs the pod was executed through its containerd,
with the scratch Postgres and Redis reached over SSH reverse tunnels. Same digest, same config
keys, same migrations, same entrypoint — only the runtime differs. The OpenAPI lock taken from this
instance is **byte-identical after sorting** to the one read from the live pod in `w2v` (same
digest), which is the cross-check that the deviation did not change the surface.

### Boot settings that mattered

1. **Migrations are an `initContainer`, not the entrypoint.** The Deployment runs
   `alembic upgrade head` in a `run-migrations` initContainer with the migration database URL. The
   image's own `CMD` is `/app/.venv/bin/python3.14 -m src.asgi`; it does not migrate. Locally the
   schema was applied first with that same command against the scratch database.
2. **`API_PORT`, not `PORT`.** The ConfigMap sets `API_PORT=6316` and this service reads it; the
   capture passes `API_PORT=18203`.
3. **The trusted-network lane is the only lane that pins the tenant contract.** `AUTH_ENABLED=false`
   is not a bypass in `w2v_common`. With `AUTH_ENABLED=true` + `AUTH_TRUST_INCLUSTER_IDENTITY=true` +
   `TRUSTED_CIDRS`, a request's source address must be inside the trusted CIDRs; a capture made
   over loopback therefore needs `127.0.0.1/32` in that list (otherwise the request falls through
   to the JWT lane and is 401 `AUTHENTICATION_REQUIRED`). On the trusted lane a request with **no**
   `X-Tenant-Id` is refused **401 `TENANT_REQUIRED`**:
   `"trusted-network requests must forward the requestor's X-Tenant-Id"`.
4. **The licence gate was left at its code default (off).** Production arms
   `W2V_LICENSE_GATE_ENABLED=true`; on the live pod every tenant-scoped route is 401 before the gate
   is even reached, so setting it in the capture would only record the absence of the licences
   service.
5. **MinIO is not a boot dependency here.** `STORAGE_BACKEND=minio` and a bucket are in the
   production ConfigMap, but the image defaults to local storage (`Local storage initialized at
   /app/data`), and readiness only probes Postgres and Redis. No captured case stores or fetches
   an artefact; the drop table below says why no operation needs it.
6. **`AUTH_LICENSE_APP_KEYS=w2v`** is passed through because it is in the Deployment's container env,
   not because a captured case exercises it.
7. **`TENANT_RLS_MODE=enforce` was not set.** The migration creates the RLS functions, but
   enforcement is the production ConfigMap's switch; the captured 404s are the same tenant-scope
   branch either way because no job row exists.

## Drop table

Every operation the reference serves, with its disposition. Nothing is silently omitted.

| operation | disposition | reason |
|---|---|---|
| `GET /` | **dropped** | service banner (`name`, `version`, endpoint index); no behavioural contract |
| `GET /health` | **served** | captured 200 `{"status":"ok","service":"frontend_test_runner_service"}` |
| `GET /healthz` | **served** | captured 200; Kubernetes liveness alias |
| `GET /health/live` | **served** | captured 200 `{"status":"alive"}` |
| `GET /health/ready` | **served** | captured 200; readiness probe, reports `database` and `redis` checks |
| `GET /readyz` | **served** | captured 200; alias of `/health/ready` |
| `POST /api/test-runner/execute` | **served as a kind, not as a route** | the captured body is an arbitrary Playwright spec runner: `package_url` (ZIP of specs) or `bdd_job_id` (fetch a package from `bdd-playwright-service`), plus `browsers`, `workers`, `grep_pattern`, `expect_recording`. The browser executor's one native Gherkin kind (`media.run_gherkin`) replaces the package source with inline feature text; the suite/recording knobs are dropped. Admission branches captured: 422 no source ("Either bdd_job_id or package_url must be provided"), 401 no tenant. |
| `GET /api/test-runner/{job_id}` | **dropped** | live job feed: polling and job state are conveyor's ledger, not the executor |
| `GET /api/test-runner/{job_id}/result` | **dropped** | live job feed: the ledger owns the run record |
| `GET /api/test-runner/{job_id}/report` | **dropped** | artifact store: HTML report hosting is not an executor kind |
| `GET /api/test-runner/{job_id}/clips/{clip_key}/video` | **dropped** | video-recording product: per-scenario clips are w2v's demo-video pipeline |
| `GET /api/test-runner/{job_id}/artifacts/{artifact_path}` | **dropped** | artifact store: a native run returns its verdict synchronously; no path-addressed store |

No test-suite CRUD routes exist in this reference (no create/update/delete of suites), so that
reason class is empty rather than unexamined.

## Route to kind mapping

| captured route | executor kind |
|---|---|
| `POST /api/test-runner/execute` | `media.run_gherkin` (source fields translated: `package_url`/`bdd_job_id` → `feature`; `target_url`, `timeout_per_test_ms`, `retries`, `trace_on_failure` kept) |
| five health routes | none — probe scaffolding re-expressed, not a kind |
| everything else | none — dropped, see table |

## The request shape of a test run

The captured schema is **not** feature text. `TestExecutionRequest` requires exactly one field in
the OpenAPI document — `required: ["target_url"]` — and the handler adds the source rule:

> `422 "Either bdd_job_id or package_url must be provided"`

Quoted from the lock:

- `target_url`: `"Live site URL to test against"` (string, schema validator requires `http`/`https`
  and a hostname)
- `package_url`: `"URL to download test ZIP package"` (string | null)
- `bdd_job_id`: `"BDD Playwright Service job ID to fetch package from"` (string | null)
- `auth_config`: credentials/SSO hints (`{type, login_url, username, password, sso_button,
  success_url}`) forwarded to the generated `auth.setup.ts`
- `browsers`: `["chromium"]` default, valid `chromium|firefox|webkit`, max 3
- `timeout_per_test_ms` (1000–600000, default 30000), `retries` (0–10), `workers` (1–16),
  `grep_pattern` (≤500), `max_run_seconds` (10–1800), `expect_recording` (default true)
- `screenshot_on_failure` is documented in the schema as **NOT IMPLEMENTED — accepted for backwards
  compatibility and IGNORED**

So the reference's run is *"download this Playwright suite (or ask the BDD service for one) and run
it against that URL"*. The browser executor's native Gherkin kind must instead accept the feature
document itself (`feature`), keep `target_url` and the per-test timeout/retries, and refuse the
package/BDD/suite/recording fields rather than silently ignoring them.

`fixtures/page.html` and `fixtures/smoke.feature` are synthetic. They were **not** posted to the
reference: no captured route accepts a `.feature` document (the two source fields are a ZIP URL and
an external job id), which is precisely why the native Gherkin kind is a new contract rather than a
route lifted from this service.

## Tolerance

The goldens are exact except for these fields, which are generated per request and cannot replay:

- `request_id` (ULID) and `timestamp` (RFC 3339) in **every** error envelope
  (`{"error":{"code",…,"request_id","timestamp"}}`), including 401/404/422.
- `/health/ready` and `/readyz` report the live `checks` map; the values are `database` and `redis`
  status strings (`"ok"` or `"error: …"`), which depend on the rig being up rather than on the
  service under test.
