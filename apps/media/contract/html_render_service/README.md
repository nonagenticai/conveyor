# The html-render-service contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `registry.internal:5000/html_render_service:latest@sha256:19ae2c9fe2ee2bf32e0f06794337f1557708029f867aaab02cdb1963a50c2fd4` |
| operations | 37 over 33 paths |
| lock | `openapi.lock.json` (sorted, taken from `/openapi.json`) |
| operation list | `operations.txt` |
| cases | 10 (`cases.txt`) |
| fixture | `fixtures/capture_page.html` (hand-written, one page, self-contained) |

The gitops pin is stale; the digest above was read from the **live Deployment's pod template**
(`kubectl -n w2v get deploy/html-render-service`), which is the truth. The commit that produced the
image is not recorded: the image is the artefact and the digest is the pin.

## How it was captured

`apps/media/contract/rig.sh` — one scratch `postgres:18` (`conveyor-media-pg`) and one `redis`
(`conveyor-media-redis`) on the rig network, and the reference run as its own image (amd64 under
emulation on this host; nothing else differs from the pod). Database `html_render_service`, host
port 18201 → container port 6319.

### Boot settings that mattered

1. **The schema must exist before the app starts.** The Deployment applies migrations from an
   `initContainer`, not from the entrypoint: `alembic upgrade head` (command `["alembic","upgrade","head"]`
   on the same image). Locally:
   `docker run --rm --network conveyor-capture -e DATABASE_URL=postgresql+psycopg://cap:cap@conveyor-media-pg:5432/html_render_service <image> alembic upgrade head`
   applied `0001_initial` → `0012_settle_orphan_variants`.
2. **This service reads `API_PORT`, not `PORT`.** The rig exports `API_PORT=<container port>`, which
   is exactly the ConfigMap's name (`API_PORT=6319`), so no extra env was needed. (The video
   compiler needed `PORT=`; do not copy that here.)
3. **Auth is the trusted-network lane, and it still demands a tenant.**
   `AUTH_ENABLED=true` in `w2v-common-config` is not a bypass. `AUTH_TRUST_INCLUSTER_IDENTITY=true` +
   `TRUSTED_CIDRS` (supplied by `rig.sh`) works, but a request with no `X-Tenant-Id` is refused
   **401 `TENANT_REQUIRED`** — and so is a header that is not a UUID (`not-a-uuid` → the same 401),
   i.e. an unparseable tenant is treated as missing, not as a different error.
4. **No LLM is reachable, and that is not a boot failure.** `LLM_BACKEND` defaults to `ir`; with no
   `LLM_IR_TOKEN` the service logs `LLM calls will raise IRNotConfiguredError` and starts anyway. This
   is why no redesign job can be executed in the rig (see the drop table) — it is a capability gap,
   not a rig defect.
5. **Storage was pointed at local disk.** Production sets `STORAGE_BACKEND=minio` +
   `STORAGE_MINIO_ENDPOINT=minio.internal:8389`; the rig has no object store, so the capture passes
   `STORAGE_BACKEND=local STORAGE_LOCAL_PATH=/tmp/storage`. `/health/ready` therefore reports only
   `database` — parity is the shape, not a store name.
6. **The licence gate was left at its code default (off).** Production arms
   `W2V_LICENSE_GATE_ENABLED=true` against the licences service; setting it in the rig would only
   capture the absence of that service.
7. **`TENANT_RLS_MODE` was left unset** (production: `enforce`). The rig's `cap` role is the
   postgres image's superuser, so RLS would not bind anyway, and `POST /api/designs/render/pdf` touches
   no table — but a future job-pipeline capture must supply a non-superuser role or the RLS contract
   is invisible.
8. **`GET /` is unauthenticated in practice** even though the OpenAPI declares the tenant header
   parameter: it answered 200 with no header. It is dropped (below), so this is recorded, not pinned.
9. **Chromium starts inside the container** (`browser pool started: size=2`) and `/health` reports
   its state (`2 slot(s), 2 idle, 2 browser(s) live`). That value is environment-dependent; the
   rewrite reports its own pool.

## Drop table

Every operation the reference serves, with its disposition. Nothing is silently omitted.

| operation | disposition | reason |
|---|---|---|
| `GET /health` | **served** | captured 200; component health (database, redis, api, job workers, browser pool, batch progress) |
| `GET /healthz` | **served** | captured 200; alias of `/health/live` |
| `GET /health/live` | **served** | captured 200; the Deployment's **liveness** probe target |
| `GET /health/ready` | **served** | captured 200; the Deployment's **readiness** probe target |
| `GET /readyz` | **served** | captured 200; alias of `/health/ready` |
| `POST /api/designs/render/pdf` | **served** → `media.render_html` | the one stateless capability route: tenant-authenticated HTML→PDF; captured 200 (PDF), 422 (empty html / missing body), 401 (no or non-UUID tenant) |
| `GET /` | **dropped** | service banner listing the Python service's own endpoints and environment; not a capability, no caller; the executor has no HTTP surface at all |
| `POST /api/designs` | **dropped** | job create — the conveyor owns the job ledger. The pipeline behind it also needs inference-router + object storage and cannot be executed here |
| `GET /api/designs/jobs` | **dropped** | job list — ledger |
| `GET /api/designs/jobs/{job_id}` | **dropped** | job status — ledger |
| `POST /api/designs/jobs/{job_id}/cancel` | **dropped** | job cancel — ledger |
| `POST /api/designs/jobs/{job_id}/refine` | **dropped** | job create from feedback — ledger; same pipeline gap |
| `GET /api/designs/jobs/{job_id}/history` | **dropped** | iteration history is job state — ledger |
| `GET /api/designs/jobs/{job_id}/webhooks` | **dropped** | webhook delivery log is job state — ledger |
| `POST /api/designs/batch` | **dropped** | batch job create — ledger |
| `GET /api/designs/batch` | **dropped** | batch list — ledger |
| `GET /api/designs/batch/{batch_id}` | **dropped** | batch status — ledger |
| `POST /api/designs/batch/{batch_id}/cancel` | **dropped** | batch cancel — ledger |
| `POST /api/designs/batch/{batch_id}/retry-failed` | **dropped** | re-queues child jobs (job create) — ledger |
| `GET /api/designs/batch/{batch_id}/results` | **dropped** | batch results are job state/artefacts — ledger |
| `GET /api/designs/jobs/{job_id}/result` | **dropped** | job-scoped artefact retrieval — the ledger owns job state and the artefact store |
| `GET /api/designs/jobs/{job_id}/html` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/component-html` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/variants/{style}/html` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/pdf` | **dropped** | Chromium print, but keyed to a stored job row — same capability as `media.render_html`; retrieval belongs to the ledger |
| `GET /api/designs/jobs/{job_id}/screenshots/{viewport}` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/original/{viewport}` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/sections/{section_name}` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/compare` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/comparison/{viewport}` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/jobs/{job_id}/diff/{viewport}` | **dropped** | job-scoped artefact retrieval — artefact store |
| `GET /api/designs/styles` | **dropped** | tenant style-catalogue administration, not a capability |
| `POST /api/designs/styles` | **dropped** | tenant style-catalogue administration |
| `GET /api/designs/styles/{style_id}` | **dropped** | tenant style-catalogue administration |
| `PUT /api/designs/styles/{style_id}` | **dropped** | tenant style-catalogue administration |
| `DELETE /api/designs/styles/{style_id}` | **dropped** | tenant style-catalogue administration |
| `POST /api/designs/styles/{style_id}/preview` | **dropped** | preview route; renders an admin style over a snippet, no capability of its own |

No served route is unaccounted for and no dropped route is missing a reason.

## Route → kind

| route | kind |
|---|---|
| `POST /api/designs/render/pdf` | `media.render_html` (params: `html`; output: PDF) |
| the five health routes | none — probe/parity targets, not job kinds |
| everything else | none — dropped as above |

The capture pins **PDF only**. The reference exposes no stateless PNG route, so PNG (and the
viewport parameters) in `render.rs` rest on the browser image's own contract, not on this capture.
A future capture of the redesign pipeline would be a separate decision: it would need a stubbed
inference-router and an object store, and would add kinds — this capture deliberately does not
invent them.

## Tolerance

The goldens are exact except for these fields, which are generated per request and cannot replay:

- `request_id` (ULID) and `timestamp` (RFC 3339) in every error envelope (`{"error":{…}}`),
  including 401 and 422.
- **The PDF bytes in `golden/render_pdf.body`.** Chromium stamps `/CreationDate` and `/ModDate` and
  a document `/ID`, so two renders of the same HTML differ (measured: two MD5s, same input). Parity
  is the invariants: `%PDF-1.4` magic, page count, and the visible text produced from
  `fixtures/capture_page.html` — not a byte comparison.
- `/health` embeds live component state (`job_workers`: `1/1`, `browser_pool`: `2 slot(s), 2 idle`).
  The rewrite reports its own workers and pool, so parity is the key set and the shape, not the
  counts.
- `/health/ready` reports only `database` because the rig uses local storage; with MinIO the
  reference reports a store too. Parity is the shape and the `database` value.

## What changes in the rewrite

- **Health scaffolding is re-expressed, not copied.** The image is a job executor, not an HTTP
  service (ADR 0007 §1–4); the five health paths are pinned only because the reference Deployment's
  probes name `/health/live` and `/health/ready`, and any process that does serve a probe port must
  answer those names.
- **The capability is `media.render_html`**, already implemented by `apps/media/src/browser/render.rs`
  (Chromium headless: `--print-to-pdf` / `--screenshot`). This capture is its evidence: tenant
  required, empty HTML refused, PDF bytes streamed, no persistence.
- **The redesign product is not absorbed by this capture.** Generation, variants, screenshots,
  diffs, comparisons, sections, refinement and webhooks all live behind job routes the ledger owns,
  and their execution needs the inference router and an object store. They are dropped with reasons,
  not silently lost; if the group later absorbs the pipeline it needs its own capture.
