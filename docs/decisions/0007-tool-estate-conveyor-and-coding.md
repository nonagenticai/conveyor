# 0007 — The tool estate is conveyor plus coding

Status: Accepted · Date: 2026-11-15 · Issue: #138 · Amends: ADR 0006 §Decision (destination for the
research and media groups confirmed; the coding row stands), ADR 0005 §2–3 unchanged

## Decision

1. **Destination.** The tool estate completes inside this workspace. `conveyor` absorbs the three
   capability families; one repository stays outside it:

   | Destination | Shape | Owns |
   |---|---|---|
   | `apps/research` (exists) | workspace app, stateless HTTP | the web-scraper and media-discovery contracts: fetch, link scoring, declarative extraction, media search |
   | `apps/media` (new) | workspace app, two executor images | Chromium capture/record, ffmpeg/ffprobe, tesseract OCR, native Gherkin runner |
   | `crates/analysis` (new) | workspace library, no service | `analyse.*` rubrics/scoring/synthesis, seven generation providers, crawl, LLM/vision client |
   | the `coding` repository (exists) | separate public Rust repo | repository provisioning, dependency graph, verified writes, CI and security checks |
   | `apps/server`, `apps/web` and the existing apps | — | actions, procedures, job ledger, artefacts, deck/PPTX, thin operations, UI |

   This is the owner's ruling of 2026-11-15: **conveyor is as self-contained as possible**, and the
   coding service keeps its own repository and release cadence (ADR 0006's coding row is unchanged).
   The model servers (`asr`, `tts`, `vlm`, image generation) stay external HTTP hosts — they are
   model servers, not tool code.

2. **One stateful plane.** Conveyor holds the database, job queue and object store. The research app
   answers request/response and keeps nothing; the media executors keep nothing; the analysis
   library keeps nothing. A second queue, database or store credential anywhere in the estate is an
   architecture violation, not an implementation detail.

3. **Media work runs as per-job Kubernetes Jobs, not a second worker protocol.** Dispatch is built by
   conveyor, generalising the mechanism that already exists in `apps/server/src/sandbox.rs`
   (ConfigMap payload, **no service-account token**, CPU/memory/ephemeral caps, create/get/delete).
   The module is `crates/runner` — deliberately not "executor", because
   `crates/domain/src/executor.rs` already owns that word for the T0 run-executor seam. The two
   images (`media-browser`, `media-ffmpeg`) are built from workspace packages by the same
   per-package image mechanism the other apps use.

4. **The runner contract is the only interface a media job has.** A versioned `runner/v1`
   instruction and result generated from `crates/domain/src/runner.rs`; the schemas are checked in
   and drift-failing. The instruction travels in the Job's ConfigMap, a short-lived **job token** in
   a Secret. All bytes cross conveyor's own routes
   (`/api/jobs/{id}/inputs|outputs|result|progress`), authorized by the token *and* the job row
   (status, live lease, attempt, tenant) — because this repository **forbids presigned URLs**
   (`crates/actions/src/integrations/objects.rs`, `crates/api/src/documents.rs`). No store
   credential ever reaches a pod.

5. **Analysis is a library, not a service.** Its inherited surfaces are sub-second LLM orchestration.
   As per-job pods they pay cold start per call; as a service they duplicate conveyor's job, retry
   and artefact handling. One conveyor job owns a whole provider lifecycle (submit → poll → fetch).

6. **The mobile app-inspector is deferred outright.** It never worked in the live estate and no
   consumer depends on it. It is not ported, not measured for demand, and not bound as a tool; its
   deployment is retired in the cutover unit. Appium being WebDriver over HTTP is recorded only so a
   future reader does not mistake the deferral for a technical impossibility.

7. **PPTX is written in Rust.** The old slide service produced decks with `python-pptx` and rendered
   them with LibreOffice. The conveyor writes OOXML itself (`crates/deck`) from one deck model that
   also emits PDF/PNG through `crates/render`. Accepted deviation: LibreOffice-only layout fidelity
   is not reproduced, and the v1 template set is fixed (title, bullets, image, notes) so "PPTX
   support" cannot silently become unlimited work.

8. **All seven generation providers are ported** — Kling, Runway, Veo, WAN, Seedance, Sora, OpenAI —
   with provider calls tested only against recorded fixtures and the `(job_id, attempt)` idempotency
   key carried to the provider, so a retried attempt cannot double-spend.

9. **Public treatment.** This repository is already public. The `coding` repository and each
   absorbed Python repository are flipped public **after their own High code-scanning set is clear
   and `gitleaks` is clean**, and the Python repositories are archived once parity has landed. The
   flip and the archive belong to the cutover unit, not to the rewrite. No new repository is created
   by this ADR.

## Context — what was measured (2026-11-15)

- The w2v directory holds 26 git repositories; 19 are live. Six are MLX model servers that stay
  external and are not ports; the rest are ~340k lines of production Python across sixteen
  workloads.
- `apps/research` already exists on `main`: the web-scraper contract landed (#117, merged as #132)
  and the media-discovery contract is in flight (#118). Absorbing rather than extracting is
  zero-churn and matches the owner's self-containment ruling.
- **There is no usage inventory for the w2v services.** `parity/` covers fr, rm and planka only.
  The contract for the conversion is therefore the sixteen deliverable blueprints (§5.2.1), not
  endpoint parity; a Loki sample of the old surfaces precedes the port and records its drops,
  exactly as `parity/` did.
- The dispatch target already exists: `crates/store` leases jobs, `apps/server/src/worker.rs`
  renews leases, `apps/server/src/sandbox.rs` creates Jobs. What is new is the generalised runner,
  the contract, and the job-token byte routes.

## Consequences

- **ADR 0006 stands, amended in wording only.** Its `apps/scientific`, `apps/lean4-prover` and
  coding rows are unchanged; its research and media rows are confirmed as workspace apps. ADR 0005
  §2–3 are untouched: this repository is where a rewrite lands, and the two strata services remain
  external.
- **Issue #139** (the `apps/media` unit) keeps its acceptance content and its destination.
- **Issue #144** (the runner substrate) is the enabler for both media images and is unaffected by
  the destination question; only its dependency wording changes.
- **No cross-repository coupling remains**: no vendored schema pin, no git dependency, no new CI
  surface. The media executor binary is compiled from this workspace and its contract types are the
  same `conveyor-domain` types the server uses.
- **Sequence:** docs (this ADR) → runner substrate (#144) → `apps/media` images → `crates/analysis`
  → capability families, project type, deck, thin operations and frontend. #118 completes
  `apps/research` independently. Deployment, cutover, public flips and archive are a later unit.
- **Review gates this ADR creates**, each with the test that enforces it: no presigned URL (token
  route tests), no second queue/DB/credential (review + CI env assertion), no Node in any image (the
  Gherkin runner is native), no schema drift (`docs/contracts/runner/v1` check), no dead-endpoint
  port (usage sample recorded before the port).
