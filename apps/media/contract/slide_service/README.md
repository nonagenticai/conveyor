# The slide-service contract

Captured 2026-11-15 from the **running** reference image, not from a rebuild:

| | |
|---|---|
| image | `slide_service:latest@sha256:2ea3aacd271cbd53191e39b6a741c7272ecceaaac8f70edb50b41b8b2e66d8e0` (the registry host is private; the digest is the pin) |
| operations | 18 over 15 paths |
| lock | `openapi.lock.json` (sorted, taken verbatim from `/openapi.json`) |
| operation list | `operations.txt` |
| cases | 32 (`cases.txt`) |
| goldens | `golden/` — 128 files, one request/status/content-type/body set per case |
| fixtures | `fixtures/one_slide.json` and `fixtures/pixel.png` — synthetic only |

The live Deployment's pod template carries this same digest
(`kubectl -n w2v get deploy/slide-service -o jsonpath='{.spec.template.spec.containers[0].image}'`),
so every statement below is about production, not only about the rig. The commit that produced the
image is not recorded: the image is the artefact and the digest is the pin.

## What this service is

`slide_service` is the reference deck generator: it accepts a deck request (`POST /api/decks`,
202 + job id), resolves each slide's template, writes the deck with `python-pptx`, converts the
PPTX to PDF with headless LibreOffice (`soffice`), rasterises every slide's HTML/CSS through a
headless Chromium pool, and stores the PPTX/PDF/PNGs plus a JSON manifest. Templates are
tenant-owned or platform (`is_system`) rows in PostgreSQL, with frozen version snapshots.

The reference's `layout_type` is a **free string** in its request schema, and the renderer registry
behind it names sixteen layouts (`src/services/template_renderer.py`). The fixed four-layout set is
a conveyor decision, not the reference's contract — see *Reproduction boundary* below.

## How it was captured

`apps/media/contract/rig.sh` — the scratch `postgres:18` and `redis` on the rig network, and the
reference run as its own image (amd64 under emulation on this arm64 host; nothing else differs from
the pod). Scratch database `slide_service`; host port 18204 → container port 6312.

### Boot settings that worked

1. **Migrations are the Deployment's initContainer, not the entrypoint.** The Deployment runs
   `alembic upgrade head` on the same image. Locally:

   ```
   docker run --rm --network conveyor-capture \
     -e DATABASE_URL='postgresql+psycopg://cap:cap@conveyor-media-pg:5432/slide_service' \
     <image> alembic upgrade head
   ```

   This image's head is `0006_template_versions_fk`; `0001` → `0006` applied cleanly. The service
   never calls `create_all`, so a missing schema is a boot/500 failure rather than a warm-up.
   `slide_service/seed.sql` is loaded **after** migrations and before the first case.
2. **The service reads `API_PORT`, not `PORT`.** The pod's ConfigMap sets `API_PORT=6312`; the rig
   passes the same name, and the image's own default is 6312. `ENVIRONMENT=development` keeps
   logs human-readable.
3. **Auth is the trusted-network lane, and it still demands a tenant.** `AUTH_ENABLED=false` is
   ignored by the shared middleware. `AUTH_TRUST_INCLUSTER_IDENTITY=true` + `TRUSTED_CIDRS`
   (supplied by `rig.sh`) works, but a request with **no** `X-Tenant-Id` is **401
   `TENANT_REQUIRED`**. A non-UUID and the nil UUID get the **same** middleware 401 — the route's
   own uuid-vs-nil messages are unreachable behind it. The reference README's claim that a trusted
   network is "assigned the default tenant" is stale; measured here, there is no fallback tenant.
4. **Storage was pointed at local disk.** Production sets `STORAGE_BACKEND=minio` and an internal
   object-store endpoint; the rig passes `STORAGE_BACKEND=local STORAGE_LOCAL_PATH=/tmp/storage`.
   The service logs `Local storage initialized at /tmp/storage`; every artefact is written there.
5. **No LLM token is needed to boot.** `LLM_BACKEND` defaults to `ir`; with no `LLM_IR_TOKEN` the
   image logs `LLM calls will raise IRNotConfiguredError` and starts anyway. Every captured deck
   request sets `content_shaper.enabled=false`, so no shaping call is attempted.
6. **The licence gate was left at its code default (off).** Production arms
   `W2V_LICENSE_GATE_ENABLED=true`; setting it here would only capture the absence of the licences
   service. `AUTH_LICENSE_APP_KEYS` also stays unset (the image logs the "any app accepted"
   warning); it is a licence-lane check, not a boot dependency.
7. **`TENANT_RLS_MODE` was left unset.** The rig's `cap` role is the Postgres superuser, so RLS
   would not bind anyway; the HTTP tenant contract is still enforced (cases 12–17, 27–30).
8. **Chromium starts eagerly but browsers start lazily.** The pool logs
   `Browser pool started: size=2, relaunch every 200 leases`; `/health` at boot reports
   `2 slot(s), 2 idle, 0 browser(s) live`. After the first render it reports 2 live. That count is
   environment-dependent; the goldens record the boot-time value.
9. **LibreOffice is in the image and the PDF path works.** `soffice --headless --convert-to pdf`
   converted the captured one-slide PPTX to a one-page PDF (28,355 bytes) with no extra setup.
10. **System templates are seeded by the app at startup.** The log line is
    `Seeded 16 system templates` / `Froze 16 system template versions`; `/health` reports
    `healthy: all 16 system templates present`. The capture's deck cases use the synthetic
    tenant-owned template fixed by `seed.sql`, not a system row (system template ids are uuid7,
    so a static case cannot name one).

### Measured end to end (not just shape)

The `deck_create` case (202) starts a real background build; polled to completion it reported
`completed`, `slide_count=1`, and every artefact downloaded:

| artefact | measured |
|---|---|
| `GET /api/decks/{id}/pptx` | 200, `application/vnd.openxmlformats-officedocument.presentationml.presentation`, 28,636 B, `Microsoft OOXML` |
| `GET /api/decks/{id}/pdf` | 200, `application/pdf`, 28,355 B, PDF 1.7, 1 page |
| `GET /api/decks/{id}/slides/0.png` | 200, `image/png`, 85,128 B, 3840×2160 (Chromium `device_scale_factor=2.0`) |
| `GET /api/decks/{id}/manifest` | 200, `application/json`, 2,602 B, one slide, 8.0 s timing hint |

The `deck_create_image` case uses the second synthetic template fixed by `seed.sql`
(`layout_type: feature_spotlight`) and inlines `fixtures/pixel.png` as a
`data:image/png;base64,…` `image_url`. It also completed; its measured artefacts were pptx
29,219 B, pdf 40,864 B and a 3840×2160 PNG of 64,265 B, and its manifest reported
`image_slots_requested=1, image_slots_failed=0, image_slots_undrawn=0`. Both builds finished in
about 5.5 s under emulation.

The reference's own log carries a fidelity defect the rewrite removes: `Brand display font 'Arial'
is not bundled … the PNG renders the CSS fallback (Inter/system) while deck.pptx sets 'Arial' on
every run: the two artifacts of the same deck disagree about the typeface.` One model in
`crates/deck` produces the PPTX, PDF and PNG, so they cannot disagree about the text or typeface.

**Measured vocabulary boundary:** creating a tenant template with `layout_type` `bullets`, `image`
or `notes` is accepted by `POST /api/templates` (the column is a free string) but the deck build
then fails with `Unknown layout_type: <value>`. The reference draws only the sixteen values listed
below. The four fixed v1 layouts are therefore a rewrite vocabulary; only `title` overlaps.

## Reproduction boundary

The deck model in `crates/deck` is internally tagged by layout and names exactly four values:

```json
{"layout":"title","title":"Opening","subtitle":"Q3"}
{"layout":"bullets","title":"Agenda","bullets":["One","Two"]}
{"layout":"image","title":null,"image":"chart.png","caption":"Revenue"}
{"layout":"notes","title":"Speaker notes","notes":["Say this"]}
```

Quoted field values: `"title"`, `"bullets"`, `"image"`, `"notes"`. `serde` refuses any other value
by name (`a_layout_outside_the_fixed_four_is_refused_by_name`), and the same model is the input to
`to_pptx` and to the `decks.render` action (ADR 0007 §7; issue #161 P1–P3). The reference names no
layout enum in its request schema: `TemplateCreateRequest.layout_type` is
`{"type":"string","maxLength":50,"description":"Layout identifier"}`.

The reference's sixteen live `layout_type` values and their disposition:

| reference `layout_type` | disposition |
|---|---|
| `title` | reproduced by `crates/deck` — the `title` layout |
| `agenda`, `section_divider`, `timeline`, `roadmap`, `process_flow`, `two_up` | dropped — bullet/timeline list layouts outside the fixed v1 set; `crates/deck`'s `bullets` layout covers the list content it can express |
| `feature_spotlight`, `finding_detail`, `tutorial_step`, `comparison` | dropped — image layouts outside the fixed v1 set; `crates/deck`'s `image` layout covers the single-image content it can express |
| `scorecard`, `metric_summary`, `stat`, `cta`, `quote` | dropped — bespoke python-pptx layout families outside the fixed v1 set |

Speaker notes are not a reference `layout_type`: the reference stores `speaker_notes` as slide
content and writes it into the PPTX notes part. `crates/deck`'s `notes` layout reproduces that
feature. Every layout above the four-value boundary is dropped for the ADR 0007 §7 reason: its
fidelity comes from a python-pptx renderer and/or a Chromium CSS template, and neither is
reproduced.

## Drop table

Every operation in `operations.txt`, with its disposition. Nothing is silently omitted.

| operation | disposition | reason |
|---|---|---|
| `POST /api/decks` | **reproduced by `crates/deck`** | the deck capability: one deck model → OOXML (`to_pptx`) and PDF/PNG through `crates/render`; the four layouts above. The `202` job intake itself is conveyor's ledger (the rewrite runs `decks.render` in `apps/server`, P3). |
| `GET /api/decks/{job_id}/pptx` | **reproduced by `crates/deck`** | OOXML writer, one slide per layout; retrieval is conveyor's document/artefact route (P3.2), not this job-scoped route. The reference's `python-pptx` assembly path is dropped as the python-pptx-only surface. |
| `GET /api/decks/{job_id}/pdf` | **reproduced by `crates/deck`** | the same model renders to PDF through `crates/render` (P2). The reference's `python-pptx` → LibreOffice (`soffice`) conversion is a **LibreOffice-only rendering path and is dropped** as the accepted fidelity deviation (ADR 0007 §7). |
| `GET /api/decks/{job_id}/slides/{slide_index}.png` | **reproduced by `crates/deck`** | per-slide PNG through `crates/render` (P2). The reference's HTML/CSS template rasteriser (headless Chromium, 2× device scale) is dropped; the renderer is pure and layout fidelity is the same accepted deviation. |
| `GET /api/decks/{job_id}` | dropped | job status polling is conveyor's job ledger (ADR 0007 §2–4); the action returns its artefact through conveyor, and there is no per-deck service to poll. |
| `GET /api/decks/{job_id}/manifest` | dropped | the manifest is a storage index (`storage_path`, `download_path`, `presigned_url`) for ledger-owned artefacts; conveyor's document/artefact route owns artefact metadata. The deck model itself is the `crates/deck` input, not this JSON. |
| `GET /api/templates` | dropped | template catalogue administration; the v1 set is fixed (ADR 0007 §7, issue #161 out-of-scope). |
| `POST /api/templates` | dropped | arbitrary template authoring; the reference accepts any `layout_type` string and only sixteen Python renderers exist (measured: `bullets`/`image`/`notes` create but fail to render). |
| `GET /api/templates/{template_id}` | dropped | catalogue read; the fixed v1 set is code, not tenant rows. |
| `PUT /api/templates/{template_id}` | dropped | catalogue edit and auto-versioning; outside the fixed v1 set. |
| `DELETE /api/templates/{template_id}` | dropped | catalogue soft-delete; outside the fixed v1 set. |
| `GET /api/templates/{template_id}/versions` | dropped | frozen template-version catalogue; `crates/deck`'s four layouts are versioned with the code, and conveyor's ledger owns deck provenance. |
| `GET /` | dropped | service banner listing this Python service's endpoints and environment; not a capability, and there is no deck HTTP service in the rewrite. |
| `GET /health` | dropped | there is no deck service: `crates/deck` is a library executed in `apps/server`, whose own health contract covers the process (decks are not an executor kind — hence no `capabilities.txt`). The case still pins the reference's probe answer for parity. |
| `GET /health/live` | dropped | same as `GET /health`: no long-lived deck process to probe. |
| `GET /health/ready` | dropped | same as `GET /health`: readiness belongs to `apps/server`, not to a deck service. |
| `GET /healthz` | dropped | same as `GET /health/live`. |
| `GET /readyz` | dropped | same as `GET /health/ready`. |

No captured operation is unaccounted for, and no dropped operation is missing a reason.

### Why there is no `capabilities.txt`

Issue #161's P4.1 lists `capabilities.txt` beside the other artefacts. It is deliberately absent
here: decks are not an executor kind. `crates/deck` is a library, `decks.render` runs in
`apps/server` (P3), and no deck image is ever dispatched; there is no capability set to declare.
The reference capture is still complete without it — the lock, operation list, manifest, cases and
goldens are above.

## Tolerance

The goldens are exact except for values generated per request, which cannot replay:

- `request_id` (ULID) and `timestamp` (RFC 3339) in every error envelope — including all 401/404/422
  cases.
- `job_id` in `deck_create`, and the artefacts' embedded job id.
- `id` in `template_create` (uuid7 minted per row); `created_at` where present.
- `/health`'s browser count (`0 browser(s) live` at boot, 2 after a render) and the presence of
  `system_templates` diagnostic text.
- `template_versions` is `[]` in its golden because it is captured before any deck has used the
  seeded template; a later read carries the frozen version.

The goldens are otherwise the reference's own bytes: `openapi.lock.json` is the sorted live
document, and the fixtures are hand-made (a one-slide JSON request and a 16×16 RGBA PNG).
