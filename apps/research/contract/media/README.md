# media-discovery, captured

The contract the Rust rewrite in `apps/research/src/media/` must serve. Captured from the reference
service, never regenerated from the rewrite — a lock produced by the implementation cannot fail for
the reason the check exists.

| | |
|---|---|
| Service | `media-discovery` 0.1.0 (`Media Discovery API`) |
| Deployed image | `media_discovery@sha256:376c127aafa1eb7c1ec49489c4751ad66543096c9d51b150752513d88e3b7449` (namespace `media-discovery`, 1/1); the registry host is deliberately not written down — `scripts/ci/no-private-refs.sh` refuses it in a public tree |
| Source commit | `9505a50` (2026-11-04) |
| Captured | 2026-11-15 |
| Document | 20 paths, **23 operations**, 19 schemas |
| Goldens | 36 cases, one request/status/body set each |

## Re-capturing

```bash
# The reference's own DEFAULT_TENANT_ID is a real customer's identifier. Override it before the
# boot so every tenant_id in the artifacts is the synthetic constant and nothing needs rewriting
# afterwards — the first capture substituted it after the fact, which is equivalent and worse.
export DEFAULT_TENANT_ID=00000000-0000-7000-8000-000000000001
MEDIA_DISCOVERY_SRC=/path/to/media-discovery ../run-local.sh 18809   # scratch Postgres, adapters off
psql "$MEDIA_DISCOVERY_DATABASE_URL" -f seed.sql                     # the pre-state, before the boot
./capture.sh --live http://127.0.0.1:18809 --image-digest sha256:376c… --source-commit 9505a50
./capture-goldens.sh --live http://127.0.0.1:18809
```

`DEFAULT_TENANT_ID` must be exported for the `alembic`/`uvicorn` step **and** for `seed.sql`, which
writes the same constant into its rows. `apps/research/tests/common` loads `seed.sql` verbatim.

**The order matters, and the first capture got it wrong.** `run-local.sh` prints the scratch
database URL it created; load `seed.sql` into it after `alembic upgrade head` and before the first
case runs. The original capture skipped that step and inherited a row from a previous run — its
`collections_empty` golden contained a collection created nine seconds earlier by
`collection_create_validation`, which creates a row and never deletes it. A fresh database answers
`[]`. The corrected capture seeds a deterministic pre-state and adds the happy-path cases the first
one could not cover.

`seed.sql` is synthetic throughout: three media items (a subject, a same-author sibling, and a
different-type/different-author control that `/related` must not return), one search session and
its two session items. It deliberately seeds **no collection**, so `collections_empty` proves
something; the collection round-trip is captured on the collection `collection_create` mints, and
the cases run create → add → get → remove → delete in that order.

The document captured from a local boot is identical to the one the deployed pod serves (same 23
operations, same 19 schema names, same `info` block — compared 2026-11-15). The goldens are captured
locally because the deployed service answers from a live customer database and reaches YouTube,
Vimeo and podcast APIs; neither is reproducible, and neither may be written to.

**Sanitised on capture.** The reference's default tenant is a real customer's identifier, and this
repository is public. Every `tenant_id` in these artifacts is `00000000-0000-7000-8000-000000000001`
— see the re-capture block above, which makes the reference answer under that constant so the
substitution is never a manual step. The first capture substituted it afterwards; the output is
identical, but a step someone can forget is a step that will be forgotten.

`tenant_id` is **not** in the tolerance list, on purpose. The constant is the one field the port
must reproduce exactly, and it reproduces it the only honest way: the parity harness sends
`X-Tenant-Id: 00000000-0000-7000-8000-000000000001` on every request, and the service has no
default tenant to fall back on. Were `tenant_id` tolerated, a port that ignored the header would
pass by answering a sentinel of its own.

## Refused by the reference — reproduced, not "fixed"

- **`POST /api/downloads` answers `501`** and creates no job. The reference starts no poller and
  claims no queue; `DownloadManager` is persistence-only and its "executed out of process" docstring
  is contradicted by its own router. The port answers the same 501 with the same message. Building a
  worker would be a new capability with no live consumer (ADR 0006 §4), not an absorption.
- **`GET /api/downloads/{id}`** therefore only ever answers `404`; captured.
- **Adapters** (youtube, vimeo, podcast, wikimedia, web_scraper) are disabled in the capture. Their
  degraded shapes — `sources_skipped` / `sources_unknown` / `sources_failed` — are what the goldens
  pin; each adapter's happy path is proven against a stub upstream, never against the vendor.

## Measured behaviour that a schema-only reading would get wrong

| Case | Reference answers |
|---|---|
| `collection_create_validation` (`"name": ""`) | **200** — an empty name is accepted; `collection_type` defaults to `playlist` |
| `search_validation_bad_media_type` (`"hologram"`) | **200** — an unknown media type is not a validation error; it simply matches nothing |
| `search_validation_missing_query` (`{}`) | **422** — pydantic's missing-field envelope |
| `score_empty` / `score_batch_empty` (`"item_ids": []`) | **400** `item_ids must not be empty` |
| `collection_add_items` with an unknown item | **404** |
| `collection_import_validation` with a non-URL | **400** |
| every 4xx/5xx | the same envelope: `{"error":{"code","message","status","request_id","timestamp"}}` |
| `GET /api/items/{id}/score` on a never-scored row | **200** — the score is computed in memory and **not** persisted; `POST` to the same path recomputes and persists it, which is why the cases run `GET` before `POST` |
| `GET /api/items/{id}/related` when the item has no embedding | the metadata top-up: same `media_type` **or** same author, newest first, capped by `limit` (default 10) |
| `GET /api/items/{id}/transcript` where the source is not youtube/podcast | `{"transcript": null, "language": null, "source": "none"}` |
| `POST /api/collections/{id}/items` with a well-formed id that exists in no tenant | **404** — the same `Collection not found or no valid items to add` as an unknown collection |
| `POST /api/score` and `POST /api/score/batch` | the same handler, the same answer |
| `POST /api/search` with a missing `query` | **422** with `"code":"VALIDATION_ERROR"`, **not** `HTTP_422` — a second envelope, and `details` is pydantic's own repr |

Quality scoring is seven weighted signals — freshness (exponential decay, 365-day half-life),
engagement (log-normalised views), authority, completeness, duration, community (likes/views) and
source reliability — at weights `0.15/0.20/0.15/0.15/0.10/0.15/0.10`, each rounded to four places
before the weighted mean is rounded to four. `freshness` and the total that consumes it are the two
values in the tolerance list, because both move with the wall clock.

## Tolerance

The parity test normalises these fields before comparing; anything else is an exact match, and a
difference in an undeclared field fails the run.

| Field | Why |
|---|---|
| `id` and the other row ids (`item_id`, `collection_id`, `job_id`, `session_id`, `media_item_id`) | the reference mints them per row |
| `created_at`, `updated_at`, `timestamp` | wall clock at capture time |
| `error.request_id` | one per request |
| `search_time_ms` | measured duration |
| `quality_score`, `components.freshness` | exponential decay from `now()`; frozen at capture |

The tolerance list is data, not prose: `tolerances.toml` in this directory is read by
`apps/research/tests/media_parity.rs`, which fails on a field it does not find declared.
