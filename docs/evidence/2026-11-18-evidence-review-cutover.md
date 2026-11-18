# The evidence-review cutover — deployed and proved (2026-11-18)

Issue #319, part P6. The unit's code merged as `eee4e479` (PR #359) and the image it published is
the one this document proves. P6 cannot precede that merge: the Deployment pins the digest the merge
run publishes, and a proof against any other image would not be a proof of the merged code.

## P6.1 — deployed

| Check | Command | Result |
|---|---|---|
| image | `kubectl -n scientific-processor get deploy scientific-processor -o jsonpath='{.spec.template.spec.containers[0].image}'` | `ghcr.io/nonagenticai/conveyor-scientific@sha256:54f33fad7fd4ad40b744224a5e28b3a5679e71b33fcd1765bde97cd2fa640483` |
| published by | ci run `44375217397`, job `135052461379` | `sha-eee4e47: digest: sha256:54f33fad…640483` |
| gitops | PR nonagenticai/gitops-production#5339 (`ed256b63`) | replica 1, digest pinned, actor env added |
| pod | `kubectl -n scientific-processor get pods` | `scientific-processor-69d7f69497-9z4t8 1/1 Running 0` (0 restarts) |
| readiness | `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:18808/health/ready` | `200` — body `{"ready":true,"reason":null}` |
| bare tenant | `curl -H 'x-tenant-id: 11111111-1111-1111-1111-111111111111' …/api/reviews` | `401` — a name is not a credential |
| licence tenant | every call below carried `x-tenant-id: Bearer <licence>` | the review operations answered `2xx`, so the introspected tenant was accepted |
| human attribution | `x-actor-token: <verified user JWT>` | accepted; the same call without it is refused `403 machine_cannot_decide` (proved by the unit's own test and by the aborted pre-token attempts below) |

The database was migrated before the Deployment was repinned: `migrations/0001_evidence_review.sql`
applied as the owner `scientific_processor_user` into `scientific_processor`, then DML granted to the
non-owner runtime role `scientific_processor_app`:

```
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f 0001_evidence_review.sql
psql "$DATABASE_URL" -c "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO scientific_processor_app;
                         GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO scientific_processor_app;"
```

Result: `50` tables (3 pre-existing), `52` policies. The migration's notices — `role
evidence_review_worker is absent; cross-tenant policy on … skipped` — are expected: that role was the
retired relay's, and no relay runs. RLS smoke as the runtime role: `current_user =
scientific_processor_app`, `SELECT count(*) FROM reviews` = `0` with and without a tenant binding
(the table is empty; the point is that the query succeeds under the non-owner, `NOBYPASSRLS` role).

## P6.2 — one review, end to end

Three short-lived Keycloak users in the `conveyor` realm (`reviewer-a`, `reviewer-b`, `lead-reviewer`)
and one client (`evidence-review-cli`: public, direct-grant only, audience mapper `conveyor`, realm
role `review-lead` on the lead) were created through the master admin for this proof. Tokens were
minted by direct grant and carried in `X-Actor-Token`; none is recorded here, and the client and users
are removed after the run. Access tokens live five minutes, which the first attempt outlived — see
"Aborted attempts".

Every step below was issued through `kubectl port-forward svc/scientific-processor 18808:8808`, as
the lead unless marked otherwise.

| # | Step | Request | Status | Evidence |
|---|---|---|---|---|
| 1 | create review | `POST /api/reviews` | 201 | `review_id=01a1f019-b40a-727c-aebe-8916fff5fe68`, state `draft`, revision 1 |
| 2 | protocol | `POST /api/reviews/{id}/protocol-versions` | 201 | `protocol_version_id=01a1f019-b63c-777f-abe0-a317b5b72f44`, one stage `title_abstract` (2 reviewers) |
| 3 | lock | `POST …/protocol-versions/{p}:lock` | 200 | `locked_at` set; stages materialised (`stage_id=01a1f019-b884-713c-9f38-01a69501cdd7`) |
| 4 | activate | `POST /api/reviews/{id}:activate` | 200 | `draft → configured → active`, revision 3 |
| 5 | import | `POST /api/reviews/{id}/imports?source_kind=canonical_json` | 201 | 2 records: `01a1f019-bfbb…` (randomised trial), `01a1f019-c013…` (case report) |
| 6 | corpus seal | `POST /api/reviews/{id}/corpus-versions` | 201 | `corpus_version_id=01a1f019-c446-…`, member_count 2 |
| 7 | assignments | `POST /api/reviews/{id}/assignments` | 201 | both records to both reviewers at `title_abstract` |
| 8 | queues | `GET /api/reviews/{id}/screening/queue?stage_id=…` as reviewer-a and reviewer-b | 200, 200 | each queue is the caller's own assignment only |
| 9 | decisions | `POST /api/reviews/{id}/decisions` ×4 | 201 ×4 | record 1: A include, B exclude (`wrong_design`); record 2: A and B include |
| 10 | conflict | `GET /api/reviews/{id}/conflicts` | 200 | `01a1f019-d4ab-…`, `dispositions:"redacted"`, `unblinded:false` — the blinding layer, live |
| 11 | unblind | `POST …/conflicts/{c}:unblind` `{"disclose_to":[reviewer-a, reviewer-b]}` | 200 | audit row written first; `disclosed_to_subject_ids` = 3 (lead + both reviewers) |
| 12 | adjudicate | `POST …/conflicts/{c}:resolve` `{"disposition":"exclude",…}` | 200 | `adjudication_id=01a1f019-e371-…`, separation of duty held |
| 13 | flow | `GET /api/reviews/{id}/flow` | 200 | `counts`: identified 2, screened 2, included 1, excluded 1 (`wrong_design:1`), adjudicated 1, not_yet_screened 0; `canonical_counts_hash=sha256:b86896d3…c01a9b`; `projection_stale:false` |
| 14 | export | `POST /api/reviews/{id}/exports` | 201 | `export_id=01a1f019-ea6e-…`, `manifest_hash=sha256:6ff191c9…7a7bae`, `payload_digest=sha256:15faa18e…23f231`, `sealed_with=canonical_sha256`, included 1, omissions 1 |
| 15 | verify | `GET …/exports/{e}:verify` | 200 | `verified:true`; recomputed manifest and payload hashes equal the recorded ones |
| 16 | read | `GET …/exports/{e}?with_payload=true` | 200 | the sealed payload serves 1 included record and 1 omission; the counts equal step 13 |

The flow ledger, the sealed manifest and the export's own counts agree, which is the invariant the
whole family exists for: a published inclusion set that a later read contradicts would be the
failure, and it is not present.

### Aborted attempts, recorded

Two earlier invocations of the same script failed and wrote nothing that the final run relies on:

1. **401 `the presented actor token could not be verified` at the first call.** The tokens had
   outlived Keycloak's five-minute access lifespan while the earlier steps were prepared. Re-minted
   immediately before the run.
2. **A run against the same review whose assignment step failed** because the script read
   `/tmp/sub-a` while the bootstrap writes `/tmp/sub-reviewer-a`. Requests reached the service and
   were refused (`422` assignment, `404` decision without an assignment); the review itself
   (`01a1f019-2c4d-…`) remains in the tenant with an empty corpus and no decisions, and the proof
   above uses a distinct review (`01a1f019-b40a-…`). Recorded rather than deleted: `reviews` is
   append-only in spirit and archiving it is the disposal path.

## What this does and does not prove

- It proves the merged binary, the migrated schema, the non-owner RLS role, the licence tenant gate
  and the verified-user rung work together on a real review with two blinded reviewers, one
  conflict, one adjudication and one sealed export.
- It does not prove response equality against the retired service: no recorded responses exist
  (capture deviation 3). P5's parity suite proves operation coverage and the drop table; this run is
  the end-to-end replay.
- The proof identities are removed after this document is written; the Deployment keeps its actor
  configuration, and an unconfigured realm refuses every actor token by design.
