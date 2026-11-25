# T14 live gate — measured 2026-11-25

Executed on the #526 branch binary against the programme record (staging copy, tenant
`nonagentic`). Destinations were reached from m4 through ssh port-forwards to the cluster
(kubectl port-forward on strato) and to the model-host boxes (mini1/mini2) — the engine process
ran locally, so every HTTP call below is a real call to the estate service, not a stub.
Steps are journaled in `docs/evidence/t14/live/` (request and full response per act).

## What the gate requires, and what was measured

| Clause | Result | Evidence |
|---|---|---|
| each published toolkit has a **probe** | **PASS** — coding, image_gen, speech, transcription, scientific all `live` (http 200) | `36-probe-restored-final.json` |
| each published toolkit has **one live call** | **PARTIAL** — coding: **PASS** (two jobs queued through `tools.call`, ids `ef545018…`, `95ffcdac…`); speech/transcription: **blocked** 503, the tenant has no sealed `licence.token` and the audio services refuse an unauthenticated call; scientific: **blocked** 401, the service demands `X-Tenant-Id: Bearer <licence>` or an `Authorization` licence and the action sends neither; image_gen: **blocked** 503 in this process, the model host answered but the document store has no object storage configured locally (the deployed pod reaches MinIO) | `41-call-coding.json`, `40-call-speech.json`, `40-call-transcription.json`, `40-call-image_gen.json`, `15-call-scientific.json`, `37-call-after-restore.json` |
| scientific_processor **scaled up** for its probe and one live call, **then scaled back** | **scale acts PASS** — observed baseline 1 replica; `1→0` (park), `0→1` (scale-up), probe live; `1→0` (outage), probe unreachable; `0→1` (restore). Its live call is blocked by the service licence above | `23-scale-zero.log`, `24-scale-up.log`, `26-scale-outage.log`, `29-scale-restore.log`, `31-scale-outage-2.log`, `34-scale-restore-2.log` |
| a **zero-use report** exists | **PASS** — `tools.usage` returns one row per published tool version with its window count and `zero_use` flag | `38-zero-use-report.json` |
| a backend **outage unbinds** new runs and in-flight runs finish | **unbind PASS** — with scientific's pods gone (confirmed 0 pods), the probe reported `unreachable`, disabled 2 bindings, and `tools.call` was refused `409` **before any network call**; after restore the probe reported `live`, `bindings_restored: 2`, and the call reached the service again (its 401). The in-flight half is proved by `crates/actions/tests/estate_toolkits.rs::an_in_flight_run_finishes_after_the_unbind`; the live licence blocker prevented staging a long real call | `32-probe-outage-2.json`, `33-call-outage-2.json`, `36-probe-restored-final.json` |

## Publication wave

`toolkits.publish` published the embedded catalogue: coding/speech/transcription/image_gen to
version 2 (`version_bumped`), scientific created at version 1 as `published` (its manifest status
was changed from `draft` because the gate names it), retrieval/webscraper/lean4 created as
`draft`. `39-publish-idempotent.json` is the second run, showing `unchanged` for every toolkit.

## Product defects the live gate found (fixed on this branch)

1. **The outbound Idempotency-Key was never forwarded.** The coding service refuses a create
   without the header; the engine claimed the key for its own dedupe and sent nothing. The external
   dispatcher now forwards the caller's key (crates/actions/src/invoke.rs) and the admin CLI can
   carry one (`--idempotency-key`). The coding call above is the proof.
2. **`scientific` was not publishable** (manifest `status: draft`) although the gate names its
   probe. Status changed to `published`.

## Blockers that are not code

- **Tenant licences — a production licensing act, by the programme's own rule.** `speech`/
  `transcription` need a sealed `licence.token` for the audio services; `scientific` needs a
  licence its service accepts. The staging record holds only the `conveyor` app licence, and the
  production tenant's sealed `licence.token` was created after the record copy was taken. The
  programme already ruled on this class in `docs/evidence/S21-RUN.md:100`: minting a licence for
  a non-production tenant is **a production licensing act**, so these calls are deploy-bound
  proofs taken with the production credential on the deployed engine — not something the
  pre-merge record can or should mint for itself.
- **Object storage in the local engine.** `image_gen` renders but cannot store its document
  without MinIO; the deployed pod has it. This receipt is likewise deploy-bound.

Both are named, measured refusals — not silent gaps. Until the deployed proofs exist, T14's
live-call clause cannot be claimed, and the project stays open.
