# Produce contract capture — 2026-11-16

The content service the `produce.script` family absorbs (narrative and script). Captured
from the pinned image, not from the rewrite: a document regenerated from the implementation
cannot fail for the reason the parity test exists.

| Field | Value |
|---|---|
| service | content service (narrative, script) |
| image_digest | sha256:71a79a0fd5392e36b8920f696f398c6dc8906933276735d9dfab88ae24e9e743 |
| source_revision | c542e35d8c7dc0df5182877b9a130a663daac333 |
| capture_date | 2026-11-16 |
| openapi_sha256 | 28de38166d3c3dc4068e54471ec0f6fd0bd4fab5c033fe73e40c824fb3d5027d |
| openapi | 3.1.0 |
| operations | 25 |
| capture_method | in-image ASGI `app.openapi()`, FastAPI JSONResponse separators |

The service boots its HTTP server only with a database and Redis present, so the document
was read from the same ASGI object the `/openapi.json` route serialises, inside the same
image at the same digest. The bytes are the bytes that route returns.

## Drop table

One row per operation in the captured document. A disposition is `ported`,
`dropped: transport`, `dropped: no consumer` or `dropped: deferred`; the test
`crates/produce/tests/parity.rs::capture` fails on an operation with no row or an
illegal disposition. Decisions are recorded in `crates/produce/contract/dispositions.tsv`.

| Operation | Disposition | Reason |
|---|---|---|
| `GET /health` | dropped: transport | liveness belongs to the runtime shape; an action has no process to health-check |
| `GET /health/live` | dropped: transport | liveness belongs to the runtime shape |
| `GET /healthz` | dropped: transport | duplicate liveness route |
| `GET /health/ready` | dropped: transport | readiness belongs to the runtime shape |
| `GET /readyz` | dropped: transport | duplicate readiness route |
| `POST /api/storyline` | ported | the storyline and its shot list, the first half of P3.1 |
| `GET /api/storyline/{job_id}` | dropped: transport | polling shape; the action returns the storyline synchronously |
| `POST /api/storyline/plan` | dropped: no consumer | the storyline operation returns its plan as part of the result; no caller needs a plan-only projection |
| `POST /api/storyline/from-findings` | dropped: deferred | needs the analysis family's findings model, which is not in this crate; no blueprint in plan 04 section 2 names it yet |
| `POST /api/shotlist` | ported | the shot list with stable shot ids, the second half of P3.1 |
| `POST /api/shotlist/enhance` | dropped: deferred | no blueprint names shot-list enhancement; port it when a deliverable needs it |
| `POST /api/shotlist/from-findings` | dropped: deferred | same findings model as the storyline conversion; no blueprint names it yet |
| `POST /api/narration` | ported | narration text, named in P3 |
| `GET /api/narration/{job_id}` | dropped: transport | polling shape; the action returns the narration synchronously |
| `POST /api/narration/shotlist` | ported | narration keyed to a shot list; the form the action takes because the shot list is an input |
| `POST /api/narrative/extract` | dropped: deferred | extracts narrative text from a supplied document; no blueprint in plan 04 section 2 names that input format |
| `POST /api/scripts/generate` | ported | script generation, named in P3 |
| `POST /api/scripts/generate/batch` | dropped: transport | a fan-out over N single calls; the workflow engine supplies concurrency |
| `POST /api/scripts/generate/campaign-pack` | dropped: deferred | multi-asset campaign generation needs the campaign blueprint, which is not among the sixteen |
| `DELETE /api/scripts/jobs/{job_id}` | dropped: transport | cancellation is the runner's; an action invocation has no job to cancel |
| `GET /api/scripts/jobs/{job_id}` | dropped: transport | polling shape; the action returns its result |
| `POST /api/scripts/jobs/{job_id}/validate` | ported | duration-budget validation with the offending shot ids, P3.2 |
| `POST /api/scripts/jobs/{job_id}/scaffold` | dropped: no consumer | writes a project tree to disk; file delivery is the pack's job and no blueprint names this output |
| `GET /api/scripts/batches/{batch_id}` | dropped: transport | polling shape for the batch fan-out |
| `GET /` | dropped: transport | the capability is an action, not an HTTP service; there is no root document to serve |
| **Total** | **25** | **6 ported, 12 transport, 2 no consumer, 5 deferred** |
