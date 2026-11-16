# html_render_service absorption — evidence (2026-11-15)

Issue #139 · the browser executor image (`conveyor-media-browser`). The reference is pinned by
digest; no private hostname, slug, address or customer payload appears in this file.

## The contract

| Fact | Value |
|---|---|
| Capture | `apps/media/contract/html_render_service/` — taken 2026-11-15 |
| Pinned image | `sha256:19ae2c9fe2ee2bf32e0f06794337f1557708029f867aaab02cdb1963a50c2fd4` |
| Operations captured | 37 (33 paths) |
| Golden cases | 10 |
| Capability kinds after the drop table | 1 — `media.render_html` |

Captured from the running image (amd64 under emulation) against a scratch `postgres:18` and redis,
with one hand-written, self-contained HTML fixture: no external stylesheet, font, script or image,
so the render needs no network.

## Parity accounting

`apps/media/contract/parity.toml` has one row per captured operation;
`apps/media/tests/parity.rs` fails on a missing, duplicated or orphan row.

- **1 served as a kind**: `POST /api/designs/render/pdf` → `media.render_html`. PDF and PNG are the
  same browser path (print vs. a sized screenshot); the captured case is the PDF, and the executor's
  own tests carry the PNG.
- **36 dropped with a reason**, in four groups: the job ledger and every job-scoped artefact or
  history route (17 — conveyor's ledger, result route and artefact store, ADR 0007 §2–4), batch
  orchestration (6 — a batch is a plan over runs, not a capability), the tenant style catalogue
  (6 — administration, not a capability), design authoring and the refine pipeline (3 — the redesign
  model behind inference-router is licence-gated and writes to object storage, so it is not
  absorbable from this capture), the root banner (1) and the five health aliases (an executor is a
  batch pod; its liveness is the Job's `activeDeadlineSeconds`).

## Traps worth keeping

- This service reads **`API_PORT`**, not `PORT`; the rig exports the latter.
- A non-UUID `X-Tenant-Id` is refused exactly like a missing one (`TENANT_REQUIRED`), so a capture
  case that "tests a bad tenant" is really testing the same 401.
- PDF bytes are **nondeterministic** (`/CreationDate`, `/ID` inside the document), so the golden is
  compared by invariants (page count, text extraction, size band), not byte equality.

## What the rewrite does

One capability: an HTML document in, a PDF or PNG out, driven by the image's own Chromium. The
Playwright `wait_until` vocabulary has no headless equivalent and is refused by name
(`wait_until_not_supported`) with `wait_ms` as the substitute — a declared gap, not a silent one.
