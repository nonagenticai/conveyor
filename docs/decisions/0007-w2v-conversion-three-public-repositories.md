# 0007 — The w2v conversion: three public repositories, one stateful plane

Status: Accepted · Date: 2026-11-15 · Issue: #138 · Supersedes: ADR 0006 §Decision (the `apps/media`
row and its "joins `apps/`" destination), ADR 0005 §2–3 for these groups only

## Decision

1. **Destination.** The w2v fleet is rewritten into three public Rust repositories:

   | Repository | Shape | Owns |
   |---|---|---|
   | `conveyor` (public today) | monorepo, the only **network service** | actions, procedures, job ledger, artefacts, deck/PPTX, thin operations, UI |
   | `nonagenticai/media` (new, public) | executor **binaries** and two images | Chromium capture/record, ffmpeg/ffprobe, tesseract OCR, native Gherkin runner |
   | `nonagenticai/analysis` (new, public) | **library**, rev-pinned git dependency of conveyor | `analyse.*` rubrics/scoring/synthesis, seven generation providers, crawl, LLM/vision client |

   ADR 0006 grouped these services into `apps/media` inside this workspace. That destination is
   superseded; ADR 0006 §2's **combining rule** (combine by runtime shape, never by file count) is
   not, and is what produces two image families rather than one.

2. **One stateful plane.** The new repositories hold no database, no job queue, no Redis and no
   object-store credential. Conveyor claims, leases, retries and records; the executors execute.
   A second queue or a second Postgres in either new repository is an architecture violation, not
   an implementation detail.

3. **Per-job Kubernetes Jobs, not a second worker protocol.** Media work is dispatched as a
   `batch/v1` Job built by conveyor, generalising the mechanism that already exists in
   `apps/server/src/sandbox.rs` (ConfigMap payload, **no service-account token**, CPU/memory/
   ephemeral caps, create/get/delete). The new module is `crates/runner` — deliberately not
   "executor", because `crates/domain/src/executor.rs` already owns that word for the T0
   run-executor seam.

4. **The runner contract is the only cross-repository interface.** A versioned `runner/v1`
   instruction and result, generated from conveyor's domain types and vendored with a `sha256` pin
   in the media repository; CI fails on drift. The instruction travels in the Job's ConfigMap, the
   short-lived **job token** in a Secret. All bytes cross conveyor's own routes
   (`/api/jobs/{id}/inputs|outputs|result|progress`), authorized by the token *and* the job row
   (status, live lease, attempt, tenant) — because this repository forbids presigned URLs
   (`crates/actions/src/integrations/objects.rs`, `crates/api/src/documents.rs`). No store
   credential reaches a pod.

5. **`analysis` is a library, not a service.** Its inherited surfaces are sub-second LLM
   orchestration; as per-job pods they pay cold start per call, and as an HTTP service they
   duplicate conveyor's job, retry and artefact handling. One conveyor job owns a whole provider
   lifecycle (submit → poll → fetch). If load ever justifies a Deployment, it can lease work over
   conveyor's HTTP without moving code.

6. **PPTX is written in Rust.** `slide` produced decks with `python-pptx` and rendered them with
   LibreOffice. The conveyor writes OOXML itself (`crates/deck`) from one deck model that also
   emits PDF/PNG through `crates/render`. Accepted deviation: LibreOffice-only layout fidelity is
   not reproduced, and the v1 template set is fixed (title, bullets, image, notes) so "PPTX
   support" cannot silently become unlimited work.

7. **All seven generation providers are ported** — Kling, Runway, Veo, WAN, Seedance, Sora, OpenAI
   — with provider calls tested only against recorded fixtures and the `(job_id, attempt)`
   idempotency key carrying to the provider, so a retried attempt cannot double-spend.

8. **Public from the start, and after.** The two new repositories are created public and use the
   ADR 0003 CI shape (inline workflow, GitHub-hosted, no repository secret, ghcr image). Each
   absorbed Python repository is flipped public **after its own High code-scanning set is clear and
   `gitleaks` is clean**, and archived once parity has landed; the flip and the archive belong to
   the cutover unit, not to the rewrite.

9. **Public treatment of the old repositories.** No private programme repository is created
   (ADR 0005 §4 still holds). The new repositories publish code and images; they publish no host
   name, tailnet address or private path — `scripts/ci/no-private-refs.sh` is the gate.

## Context — what was measured (2026-11-15)

- The w2v directory holds 26 git repositories; 19 are live. Six are MLX model servers that stay
  external HTTP hosts (ADR 0006 consequences) and are not ports; the rest are ~340k lines of
  production Python across sixteen workloads.
- **There is no usage inventory for these services.** `parity/` covers fr, rm and planka only.
  The contract for this conversion is therefore the sixteen deliverable blueprints (§5.2.1), not
  endpoint parity; porting a 69-endpoint analysis service or a 30-endpoint provider fan-out by
  parity would port dead code. A Loki usage sample of the old surfaces precedes the port and
  records its drops, exactly as `parity/` did.
- The orchestrator is replaced, not ported: its role is procedures and project types (§5.2.1).
  `w2v_common` is deleted; only its media helpers survive, in the new crates. The SvelteKit
  frontend is replaced by `apps/web` (ADR 0001).
- `app-inspector` needs Appium over HTTP against a device farm. That is portable in principle (a
  WebDriver client, not a Rust "Appium"), but no consumer has been measured. It stays a bound
  Python tool and is out of v1; it is not deleted.
- The dispatch target already exists: `crates/store` leases jobs, `apps/server/src/worker.rs`
  renews leases, `apps/server/src/sandbox.rs` creates Jobs. What is new is the generalised runner,
  the contract, and the job-token byte routes.

## Consequences

- **ADR 0006 is amended, not deleted.** Its scientific, research, lean4-prover and coding rows
  stand. Its `apps/media` row is replaced by this ADR. Issue #139 (the `apps/media` unit) keeps its
  acceptance content — contract capture, runtime split, artefact governance — and loses its
  destination; it is rewritten by the media/analysis repository units, never executed as written.
- **ADR 0005 §3 narrows**: "rewrites land in this repository" becomes "the **control plane** lands
  in this repository". `strata_ingest`/`strata_rag` stay external (ADR 0005 §2 unchanged).
- **`SPEC_V2` §11.2's w2v row** no longer says the fleet moves *into* the conveyor; it moves into
  the two public repositories, bound through the same job and artefact contracts.
- **Sequence (owner ceiling ≤5 units):** docs (this ADR) → conveyor runner substrate → media
  repository ∥ analysis repository → capability families, project type, deck, thin operations and
  frontend in conveyor. Deployment, cutover, public flip and archive are a later unit.
- **Review gates this ADR creates**, each with the test that enforces it: no presigned URL (token
  route tests), no second queue/DB/credential (repository review + CI env assertion), no Node in
  any image (the Gherkin runner is native), no schema drift (pin check), no dead-endpoint port
  (usage sample recorded before the port).
