# The evidence-review service — capture and drop table (2026-11-18)

Issue #319 · part P1 · branch `issue/319-evidence-review`. This is the contract the absorption is
measured by: every operation the captured service serves is on this page, and every operation that
is not ported says why. The service's repository is private and is not linked from this public
document; it is identified by the revision and date below.

## The pin

| Fact | Value |
|---|---|
| Service | the evidence-review stage service (FastAPI, Python, service version `0.1.0`) |
| `source_revision` | `bac4f6612b2632621c7f6e2650259257fa17f877` — `origin/main`, committed 2026-11-18T00:07:13+01:00 (2026-11-17T23:07Z) |
| `capture_date` | 2026-11-18 |
| `migrations` | **28** files, `0001` through `0028` |
| `tables` | **47** distinct tables created by the migration head; 46 are ORM-mapped and `operation_idempotency` is created and used as raw SQL |
| Shape | **60 operations** over **10 route-bearing modules**; `src/api/routers/__init__.py` is empty and the service declares no root route |

How the document was obtained: every fact below was read from the service's git objects at
`origin/main` with `git show`, not from a working tree and not from a running pod. No live request
was issued.

The issue's baseline names revision `5e3a868` (2026-11-17). The pin above is one commit later; the
diff between the two revisions is `uv.lock` only (950 insertions, 647 deletions), so the routers and
migrations are byte-identical and every row below holds at both revisions.

### Tables, and why the issue's 26 is not this number

`op.create_table` is called 48 times across the 28 migrations; one occurrence is a docstring
example, leaving 47 distinct tables. The retired production database was measured at 26 tables in the
source README on 2026-09-02, before migrations `0019` through `0028` were added. Those later
migrations create the outbox, durable jobs, evidence snapshots, event log, inbox and artifact-record
tables, and they are part of the migration head this port must reproduce. The README also says no
migration ever created `domain_outbox`, `durable_jobs` or `job_attempts`; at this pin `0019` and
`0020` do. P2's "26 tables" is therefore a historical production measurement, not the schema the
migrations define. See “Deviations” below.

## What the service is

The service is the system of record for human judgement in an evidence review. It owns a
protocol-locked review, reversible deduplication, two blinded reviewers, append-only decisions,
conflict adjudication, a derived PRISMA flow ledger, and an inclusion-set export sealed with a
canonical hash. Its stated scope is `ER-001..ER-005` plus the `OUT-001` export contract. XLSX import,
automatic exclusion ranking, the proposal lane, appraisals and any regulatory canonical surface are
deliberately absent, and no route below restores them.

Three invariants hold across the whole service. **A machine cannot decide:** the actor seam refuses
a caller that presents a tenant but no subject, so no code path writes a decision without an
authenticated human (`src/actor.py`; the screening decision body deliberately has no reviewer field).
**Reviewer B cannot see reviewer A:** six layers, any one of which alone leaks — `FORCE ROW LEVEL
SECURITY` with a non-owner runtime role, one blinded read function
(`application/services/blinded_reads.py::visible_decisions`), a queue over the caller's own
assignments, cursors over assignments rather than records, `404` never `403`, and
`flow.require_no_owed_decisions` on every route that derives an aggregate. **History is append-only:**
`BEFORE UPDATE OR DELETE` triggers raise in the database, there is no `is_current` column, and
"current" is the tail of a supersession chain made linear by partial unique indexes.

### RLS with FORCE

Every one of the 47 tables is put behind row-level security by a per-migration helper:
`alembic/versions/0002_er_m002_reviews.py::_tenant_rls`,
`0003_er_m003_imports.py::_tenant_rls`, `0004_er_m004_deduplication.py::_tenant_rls`,
`0005_er_m005_screening.py::_tenant_rls`, `0017_er_m017_a_research_object_has_identity.py::_tenant_rls`,
`0018_er_m018_accepted_evidence_names_its_basis.py::_tenant_rls`,
`0019_er_m019_domain_outbox_commits_with_the_state.py::_tenant_rls`,
`0020_er_m020_a_long_job_survives_the_worker_that_ran_it.py::_tenant_rls`,
`0021_er_m021_a_snapshot_says_exactly_what_a_run_may_read.py::_tenant_rls`,
`0022_er_m022_an_event_can_finally_leave_the_service.py::_tenant_rls` and
`0025_er_m025_an_artifact_ref_is_never_a_storage_key.py::_tenant_rls`. Each helper emits both
`ENABLE ROW LEVEL SECURITY` and `FORCE ROW LEVEL SECURITY`; `screening_decisions` is the one explicit
pair because its policy is the blinding policy (`0005`, lines 338-339). `0009` deliberately lifts
FORCE on `screening_assignments` and `corpus_versions` for a backfill and restores it at lines
157-159; the lift is bounded to the migration and is not a runtime property. The application runs as
a non-owner role, which is what makes FORCE bind the table owner too.

### Append-only triggers

The append-only guard is a reusable PL/pgSQL function that raises on `UPDATE` or `DELETE`:
`0002_er_m002_reviews.py::_append_only_trigger` (state history),
`0004_er_m004_deduplication.py::_append_only` (resolutions and survivorship fields),
`0005_er_m005_screening.py::_append_only` (assignments, decisions, conflicts, adjudications,
unblinding events, inclusion exports), `0015_er_m015_the_ledger_inputs_are_append_only.py::_append_only`
(import batches, import rows, duplicate clusters and cluster members), and
`0018_er_m018_accepted_evidence_names_its_basis.py::_append_only` (the accepted-evidence identity
and version tables).
`0011_er_m011_review_stages_append_only.py::review_stages_append_only` adds the one table that was
described as append-only but was not guarded. `0025` guards `artifact_records`. The guards are
database constraints, not application convention; the service's own tests disarm each one inside a
rolled-back transaction to prove the refusal is not vacuous.

### Supersession chains

No table carries a mutable "current" flag. Linearity is a partial unique index: one un-superseded
head per subject and one successor per superseded row. Measured names include
`0003_er_m003_imports.py::uq_record_version_supersession_linear` and
`::uq_corpus_version_supersession_linear`; `0004_er_m004_deduplication.py::uq_resolution_supersession_linear`
and `::uq_resolution_one_head_per_cluster`;
`0005_er_m005_screening.py::uq_decision_one_current_per_reviewer`,
`::uq_decision_supersession_linear`, `::uq_adjudication_one_current_per_conflict` and
`::uq_adjudication_supersession_linear`; `0017_er_m017_a_research_object_has_identity.py::uq_research_object_version_supersession_linear`;
`0018_er_m018_accepted_evidence_names_its_basis.py::uq_extraction_dataset_supersession_linear`,
`::uq_observation_supersession_linear`, `::uq_research_assertion_supersession_linear`,
`::uq_acceptance_one_chain_per_subject` and `::uq_acceptance_supersession_linear`; and
`0021_er_m021_a_snapshot_says_exactly_what_a_run_may_read.py::uq_snapshot_one_chain_per_review`,
`::uq_snapshot_supersession_linear` and `::uq_snapshot_one_per_job`. A merge is undone by inserting a
new `split` row, not by editing or deleting the merge row.

### The canonical-hash seal

The seal has two halves. In the database, `corpus_versions.member_manifest_hash` is checked at
insert and on any membership change by
`0003_er_m003_imports.py::corpus_versions_verify_seal` and
`::corpus_versions_verify_seal_on_version`, which recompute the hash from the member rows and raise
when it differs. `inclusion_exports` carries both `canonical_counts_hash` and `manifest_hash`, with
`ck_export_hashes_are_distinct` ensuring the manifest is not merely a copy of the payload digest
(`0005_er_m005_screening.py`, lines 387-428). `0006_er_m006_export_seal_mode.py::SEAL_MODES` records
which algorithm sealed each row (`content_hash`, `canonical_sha256` or the historical `unknown`),
because an export sealed through a narrower fallback must remain legible rather than silently
upgraded. In the service, `src/application/services/flow.py::EXPORT_SCHEMA_REF` names the released
manifest contract, `::_seal_manifest` prefers `compute_content_hash` and falls back to RFC 8785
`canonical_sha256`, and `::seal_mode` reports the mode. The verify route recomputes both hashes from
the stored bytes; the supplied hash is never trusted.

## The classification rule

Every row carries exactly one `state` and, when dropped, exactly one `reason`. The reason literals
are the same three the analysis capture uses; the ground each one requires is fixed here so a row can
be checked rather than believed. First match wins:

1. **ported** — a Rust entry point exists today. The `ground` cell names it. This capture has no
   ported rows: P3 and P4 do not exist yet, so inventing one would make the table lie.
2. **dropped: transport** — the operation's subject is the service's own plumbing: liveness,
   readiness, metrics. Conveyor already owns that surface. A transport row must name the conveyor
   surface that replaces it; here every one is `conveyor:liveness`, the fleet probe-and-scrape
   surface. A row that cannot name one is not transport.
3. **dropped: deferred** — a domain operation the port plan owns. The `ground` cell names the part of
   issue #319 that requires it: `part: P3` (routes, actor seam, blinding layers and the review
   domain) or `part: P4` (the inclusion-set export and its seal). A deferred row may also name a
   measured caller as `caller: <path:line>`, but this capture uses the part form for every domain
   row because the port is not built yet. P5 owns the served contract and the parity gate, not an
   operation, so no row is deferred to P5.
4. **dropped: no consumer** — analysis-shaped, with no replacement and no owning part. This capture
   has none. An absent caller is not treated as a retirement decision here: issue #319 assigns every
   domain operation to P3 or P4, so a missing caller is recorded in “Consumers” rather than used to
   drop a row. The one deliberate consequence is that the `no consumer` literal does not appear.

Columns: `method | path | operation_id | tag | state | reason | ground | evidence | request_ref |
response_ref`. `evidence` is the handler's own work read from the pinned source, not a restatement of
the reason. `tag` is the router's declared FastAPI tag verbatim; `health` and `metrics` are the
router areas because those two modules declare no tag. `request_ref` is the declared Pydantic body
model, `object` for a free-form body with no model, and `-` for no body. `response_ref` is the
declared response model, `application/json` for a handler that returns a JSON object with no named
model, and `text/plain; version=0.0.4` for the Prometheus scrape. `-` means the source declares
nothing in that position; it is never used in the `reason` column of a dropped row.

## The drop table

<!-- BEGIN CAPTURE -->
| method | path | operation_id | tag | state | reason | ground | evidence | request_ref | response_ref |
|---|---|---|---|---|---|---|---|---|---|
| POST | /api/reviews | create_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | CreateReviewBody | application/json |
| GET | /api/reviews | list_reviews | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id} | get_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | - | application/json |
| PATCH | /api/reviews/{review_id} | patch_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | PatchReviewBody | application/json |
| GET | /api/reviews/{review_id}/state-history | get_review_state_history | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/protocol-versions | create_protocol_version | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | ProtocolBody | application/json |
| GET | /api/reviews/{review_id}/protocol-versions | list_protocol_versions | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/protocol-versions/{protocol_version_id}:lock | lock_protocol_version | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}:activate | activate_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | ActivateBody | application/json |
| POST | /api/reviews/{review_id}:pause | pause_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | TransitionBody | application/json |
| POST | /api/reviews/{review_id}:resume | resume_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | TransitionBody | application/json |
| POST | /api/reviews/{review_id}:close | close_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | TransitionBody | application/json |
| POST | /api/reviews/{review_id}:reopen | reopen_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | TransitionBody | application/json |
| POST | /api/reviews/{review_id}:archive | archive_review | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_reviews | TransitionBody | application/json |
| POST | /api/reviews/{review_id}/archive | archive_review_slash | Reviews | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | TransitionBody | application/json |
| POST | /api/reviews/{review_id}/research-objects | create_research_object | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | ResearchObjectBody | application/json |
| GET | /api/reviews/{review_id}/research-objects | list_research_objects | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/research-objects/{research_object_id}:revise | revise_research_object | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | ReviseObjectBody | application/json |
| POST | /api/reviews/{review_id}/research-objects/{research_object_id}/extractions | record_extraction | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | ExtractionBody | application/json |
| POST | /api/reviews/{review_id}/assertions | record_assertion | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | AssertionBody | application/json |
| POST | /api/reviews/{review_id}/acceptances | decide_acceptance | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | AcceptanceBody | application/json |
| GET | /api/reviews/{review_id}/acceptances | list_acceptances | Accepted evidence | ported | — | apps/scientific/src/evidence_review/evidence.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/exports | create_export | Exports | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| GET | /api/reviews/{review_id}/exports | list_exports | Exports | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| GET | /api/reviews/{review_id}/exports/{export_id}:verify | verify_export | Exports | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| GET | /api/reviews/{review_id}/exports/{export_id} | get_export | Exports | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| GET | /api/reviews/{review_id}/flow | get_review_flow | Flow | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/flow:rebuild | rebuild_review_flow | Flow | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/flow:audit | audit_review_flow | Flow | ported | — | apps/scientific/src/evidence_review/flow.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/review-jobs/{job_id} | get_review_job | Review jobs | ported | — | apps/scientific/src/evidence_review/jobs.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/review-jobs/{job_id}:cancel | cancel_review_job | Review jobs | ported | — | apps/scientific/src/evidence_review/jobs.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/imports | create_import | Imports | ported | — | apps/scientific/src/evidence_review/imports.rs | cargo test -p conveyor-scientific --test evidence_review_parity | object | application/json |
| POST | /api/reviews/{review_id}/imports/from-discovery | create_import_from_discovery | Imports | dropped | no consumer | the frozen-batch upstream (discovery-engine) is retired; no live caller exists | checks the lead gate, fetches a frozen batch, verifies both digests, then imports | FromDiscoveryBody | application/json |
| GET | /api/reviews/{review_id}/imports | list_imports | Imports | ported | — | apps/scientific/src/evidence_review/imports.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id}/records | list_records | Imports | ported | — | apps/scientific/src/evidence_review/imports.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id}/records/{review_record_id} | get_record | Imports | ported | — | apps/scientific/src/evidence_review/imports.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id}/quarantine | list_quarantined_rows | Imports | ported | — | apps/scientific/src/evidence_review/imports.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/corpus-versions | seal_corpus_version | Imports | ported | — | apps/scientific/src/evidence_review/corpus.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| GET | /api/reviews/{review_id}/corpus-versions | list_corpus_versions | Imports | ported | — | apps/scientific/src/evidence_review/corpus.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| GET | /api/reviews/{review_id}/corpus-versions/{corpus_version_id}/members | list_corpus_members | Imports | ported | — | apps/scientific/src/evidence_review/corpus.rs | cargo test -p conveyor-scientific --test evidence_review_exports | - | application/json |
| POST | /api/reviews/{review_id}/deduplication-runs | create_deduplication_run | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | DedupRunBody | application/json |
| GET | /api/reviews/{review_id}/deduplication-runs | list_deduplication_runs | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id}/duplicate-clusters | list_duplicate_clusters | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/duplicate-clusters/{cluster_id}:resolve | resolve_duplicate_cluster | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | ResolveBody | application/json |
| GET | /api/reviews/{review_id}/duplicate-clusters/{cluster_id}/resolutions | get_resolution_chain | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/assignments | create_assignments | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | AssignmentBody | application/json |
| GET | /api/reviews/{review_id}/screening/queue | get_screening_queue | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/decisions | record_decision | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | DecisionBody | application/json |
| GET | /api/reviews/{review_id}/decisions | list_visible_decisions | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id}/conflicts | list_conflicts | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/conflicts/{conflict_id}:unblind | unblind_conflict | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | UnblindBody | application/json |
| POST | /api/reviews/{review_id}/conflicts/{conflict_id}:resolve | adjudicate_conflict | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | AdjudicateBody | application/json |
| GET | /api/reviews/{review_id}/stages | list_stages | Screening | ported | — | apps/scientific/src/evidence_review/screening.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| POST | /api/reviews/{review_id}/evidence-snapshots | create_evidence_snapshot | Evidence snapshots | ported | — | apps/scientific/src/evidence_review/snapshots.rs | cargo test -p conveyor-scientific --test evidence_review_parity | object | application/json |
| GET | /api/reviews/{review_id}/evidence-snapshots | list_evidence_snapshots | Evidence snapshots | ported | — | apps/scientific/src/evidence_review/snapshots.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /api/reviews/{review_id}/evidence-snapshots/{snapshot_id} | get_evidence_snapshot | Evidence snapshots | ported | — | apps/scientific/src/evidence_review/snapshots.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /health | health_check | health | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /health/live | liveness_check | health | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /health/ready | readiness_check | health | ported | — | apps/scientific/src/evidence_review/reviews.rs | cargo test -p conveyor-scientific --test evidence_review_parity | - | application/json |
| GET | /metrics | metrics | metrics | dropped | transport | conveyor:liveness | renders the per-app Prometheus registry in the text exposition format; no tenant data | - | text/plain; version=0.0.4 |
<!-- END CAPTURE -->

## Counts

| State | Rows |
|---|---|
| ported | 0 |
| dropped: transport | 4 |
| dropped: deferred | 56 — 49 `part: P3`, 7 `part: P4` |
| dropped: no consumer | 0 |
| **total** | **60** |

The four transport rows are the three health probes and the metrics scrape. The seven P4 rows are the
four export routes and the three corpus-version routes that carry the seal; every other domain
operation is P3. Every dropped row names a ground: no cell in the `reason` or `ground` columns is
empty or `-`.

## Consumers and their calls

Measured by searching the same sibling repositories the analysis capture covered (`w2v` service tree,
`care-bus-agents`, `agent`, `helpdesk`, `scientific-processor`), plus `fr` in both directions
(frontend client and backend proxy) and the captured service's own scripts at the pinned revision. The
service is reached through the kag gateway in production; the `fr` backend also supports a direct
local mode. The kag gateway repository is not present under the searched roots, so its forwarder is
recorded as a reference from `fr` rather than as read source.

`fr`'s evidence-review client is a typed call surface over the byte-level proxy:
`fr/frontend/src/lib/api/reviews.ts` (all line numbers below) through
`fr/backend/src/routes/assets/kag-reviews.ts:149-152`. The proxy forwards GET, POST, PATCH, PUT and
DELETE and does not re-serialize the body, so every path under `/api/reviews` is reachable; the table
records the 27 operations for which a typed client function exists today.

| Operation | Caller |
|---|---|
| `list_reviews` | `fr/frontend/src/lib/api/reviews.ts:638` |
| `create_review` | `fr/frontend/src/lib/api/reviews.ts:640` |
| `create_protocol_version` | `fr/frontend/src/lib/api/reviews.ts:643` |
| `lock_protocol_version` | `fr/frontend/src/lib/api/reviews.ts:654` |
| `activate_review` | `fr/frontend/src/lib/api/reviews.ts:659` |
| `create_import` | `fr/frontend/src/lib/api/reviews.ts:668` |
| `create_import_from_discovery` | `fr/frontend/src/lib/api/reviews.ts:694` |
| `seal_corpus_version` | `fr/frontend/src/lib/api/reviews.ts:699` |
| `list_corpus_versions` | `fr/frontend/src/lib/api/reviews.ts:713` |
| `list_stages` | `fr/frontend/src/lib/api/reviews.ts:716` |
| `create_deduplication_run` | `fr/frontend/src/lib/api/reviews.ts:718` |
| `list_deduplication_runs` | `fr/frontend/src/lib/api/reviews.ts:735` |
| `list_duplicate_clusters` | `fr/frontend/src/lib/api/reviews.ts:763` |
| `resolve_duplicate_cluster` | `fr/frontend/src/lib/api/reviews.ts:768` |
| `create_assignments` | `fr/frontend/src/lib/api/reviews.ts:801` |
| `get_screening_queue` | `fr/frontend/src/lib/api/reviews.ts:962` |
| `record_decision` | `fr/frontend/src/lib/api/reviews.ts:977` |
| `list_conflicts` | `fr/frontend/src/lib/api/reviews.ts:1020` |
| `unblind_conflict` | `fr/frontend/src/lib/api/reviews.ts:1042` |
| `adjudicate_conflict` | `fr/frontend/src/lib/api/reviews.ts:1048` |
| `get_review_flow` | `fr/frontend/src/lib/api/reviews.ts:1058` |
| `rebuild_review_flow` | `fr/frontend/src/lib/api/reviews.ts:1060` |
| `create_export` | `fr/frontend/src/lib/api/reviews.ts:1065` |
| `list_exports` | `fr/frontend/src/lib/api/reviews.ts:1103` |
| `get_export` | `fr/frontend/src/lib/api/reviews.ts:1121` |
| `verify_export` | `fr/frontend/src/lib/api/reviews.ts:1152` |
| `list_quarantined_rows` | `fr/frontend/src/lib/api/reviews.ts:1175` |

The other 33 operations have no typed caller in that set. That includes the accepted-evidence routes,
the snapshot routes, the review-job read and cancel, the record detail, the visible-decision list,
the state history, the protocol list, the review detail and patch, the lifecycle commands other than
activate, and `flow:audit`. It is a measurement of code present on this machine, not of live traffic:
issue #319's own baseline says the service is retired and its database roles were deleted on
2026-10-30, so a green `fr` client is wiring, not proof that a pod answers.

## Deviations, recorded rather than hidden

1. **Revision.** The issue names `5e3a868` (2026-11-17); this capture reads
   `bac4f6612b2632621c7f6e2650259257fa17f877` (2026-11-18T00:07:13+01:00). The diff is `uv.lock` only,
   so routers, migrations and table definitions are unchanged. Both dates are recorded because a
   reader who checks the issue will find a different revision and should know why.
2. **Table count.** The issue and the source README say 26 tables. The migration head at this pin
   defines 47. The 26 is the retired production database measured on 2026-09-02, before migrations
   `0019` through `0028`; the README's claim that no migration created `domain_outbox`,
   `durable_jobs` or `job_attempts` is contradicted by `0019` and `0020` at this pin. P2's acceptance
   criterion must either adopt 47 or state the subset it means; this document records the measured
   head.
3. **No served-document capture.** Unlike the analysis capture, no OpenAPI JSON is committed beside
   this table and no live request was issued. The operation list is read from the route decorators
   and handler signatures at the pin. A later part that wants the analysis capture's digest-pinned
   contract check must capture the served document at P5; this table alone supports coverage and
   wiring claims, not semantic parity.
4. **`/metrics` has no operation_id.** The route is `include_in_schema=False` and declares no
   `operation_id`, so the function name `metrics` is used. The health routes declare operation_ids but
   no tags; `health` and `metrics` in the `tag` column are therefore area names, not declared tags.
   The declared tags elsewhere are capitalized (“Reviews”, “Accepted evidence”, “Exports”, “Flow”,
   “Review jobs”, “Imports”, “Screening”, “Evidence snapshots”); if the P5 parity test normalizes to
   lowercase, this column is the input to that normalization, not a served-document tag.
5. **Port disposition.** Every operation in P3/P4 is **ported** in this unit — 55 of the 60, each
   row naming the module that serves it and the check that proves it. The four transport operations
   (`health`, `health/live`, `health/ready`, `metrics`) are served by the conveyor's own liveness
   surface, as the `conveyor:liveness` ground records. `POST /imports/from-discovery` is
   **dropped: no consumer**: its frozen-batch upstream (discovery-engine) is retired, measured in
   P1, and no live caller sends a discovery session.
6. **Response refs are conventions, not schema names.** No route in this service declares a Pydantic
   `response_model`; every JSON handler returns `dict[str, Any]` or a `JSONResponse`.
   `application/json` in the `response_ref` column records that fact. A `-` records a request with no
   body. The parity test must treat these two conventions as declared, not infer a schema that the
   source does not have.
7. **The format source.** The brief names `docs/evidence/analysis-capture-2026-11-15.md` and
   `crates/analysis/tests/parity.rs`; neither exists at this worktree's base. The grammar here
   (markers, ten columns, one state and one reason per row) was read from the commit that introduced
   the analysis capture and its `mod capture` on conveyor issue #154. The P5 test named by issue #319
   (`cargo test -p conveyor-scientific --test evidence_review_capture`) is the reader this document
   is shaped for.
