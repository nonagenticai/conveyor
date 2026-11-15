# The analysis service — capture and drop table (2026-11-15)

Issue #154 · part P1 · branch `issue/154-analysis`. This is the contract the absorption is measured
by: every operation the captured service serves is on this page, and every operation that is not
ported says why. The service's repository is private and is not linked from this public document; it
is identified by the revision and digest below.

## The pin

| Fact | Value |
|---|---|
| Service | the analysis stage service of the media estate (FastAPI, OpenAPI 3.1.0, service version `0.1.0`) |
| `source_revision` | `086e7f5c1248e0b880bdf5eeb0978abde6f70e55` — `origin/main`, 2026-10-22 |
| `image_digest` | `sha256:dcffb3784abb50d7a7a270d0366918cb25d121508d7cd36bc5a835ff5de1b2cc` — the container image the pod actually runs |
| `capture_date` | 2026-11-15 |
| `openapi_sha256` | `1bcd3894d5b4bce38151c8df8c4d7d13318cd567c028f0e28929bee0339d3b34` (222,684 bytes) |
| Served document | [`analysis-openapi-2026-11-15.json`](analysis-openapi-2026-11-15.json) — the bytes exactly as served, committed unmodified |
| Shape | **92 operations** over **79 paths**, 180 component schemas, no `servers` block |

Two facts about how the document was obtained, because both are easy to get wrong:

1. **It was read from the running pod's own loopback**, through the deployment's exec surface, not
   from a source tree. The document is therefore the contract the live image serves, not what some
   checkout says it should serve.
2. **The gitops manifest is behind the pod.** It pins image
   `sha256:65e9e34f783b36855769397dae64c12355d72bf12460054fa1d2a2dd3630da02` while the pod runs
   `sha256:dcffb378…`. The digest above is the one measured against the served bytes; a reader who
   checks the manifest will find a different number and should believe the pod.

### Which revision the served document is

The working copy of the service's repository on this machine sits on a branch that no longer exists
upstream, at `fad3856`, which is **not** an ancestor of `origin/main`. It must not be used as the
source of truth. The served document proves which line it came from: `SeoAuditResponse` in the
served document carries `unmeasured_groups`, `coverage`, `projection`, `grounding`, `exec_summary`,
`critique`, `facts`, `ai` and `scored_at`, and its `group_scores` accepts `null` — all of which exist
only on the `origin/main` line. The stale checkout's model has none of them and `group_scores:
dict[str, float]` with no nulls.

Every classification below was read from `origin/main` for the same reason.

## What the service is

Twelve domain tags plus a health surface. The tag counts are the served document's.

| Tag | Operations | What it holds |
|---|---|---|
| health | 7 | liveness/readiness/startup probes, Prometheus metrics, the root banner |
| analyze | 9 | the page-analysis pipeline: submit, poll, cancel, artifacts, page classification, narrative, its own sub-review |
| context | 3 | context inference, validation and enrichment |
| context-profiles | 4 | CRUD for stored context profiles |
| critique | 34 | the website critique pipeline and everything around it: jobs, batches, schedules, suppressions, analytics, findings, exports, reports, scoring weights, config overrides |
| review | 13 | the qualitative review pipeline: jobs, batches, comparison, findings, summary, video, usage, domain history |
| brand | 6 | brand extraction from a URL or a document, batch extraction, logo assets |
| brands | 9 | the stored brand-snapshot store: list, get, override, history, export, quality, cache invalidation, logo, comparison |
| icp | 1 | ideal-customer-profile derivation from a URL |
| artifact-score | 2 | quality scoring of an artifact, and the before/after delta |
| cro | 1 | the conversion scorecard for one page |
| seo_audit | 1 | the SEO health score for one page |
| marketing | 1 | the marketing scorecard for one brand/context |

## The classification rule

Every row carries exactly one `state` and, when dropped, exactly one `reason`. The reason literals
are the issue's; what each one is allowed to mean here is fixed so that a row can be checked rather
than believed. First match wins:

1. **ported** — a Rust entry point exists today. The `ground` cell names it, and the ported
   assertions live in `crates/analysis/tests/parity.rs`.
2. **dropped: transport** — the operation's subject *is* the service's own plumbing: liveness,
   metrics, job/batch/schedule ledger rows, progress streams, artefact and export bytes, logo bytes.
   Conveyor already owns that surface, so porting it would be a second copy of the platform. **A row
   may only claim this reason if its `ground` cell names the conveyor surface that replaces it**
   (`conveyor:liveness`, `conveyor:job-engine`, `conveyor:scheduler`, `conveyor:artefact-store`).
   A row that cannot name one is not transport.
3. **dropped: deferred** — an analysis core the port plan owns. The `ground` cell names the measured
   caller (`caller: <path:line>`) or the part of this issue that requires it (`part: P5`, `P6`).
4. **dropped: no consumer** — analysis-shaped, and neither 2 nor 3 holds. This is the issue's default
   for an operation with no measurable consumer; the scope of the consumer search is stated under
   “Consumers” below so a later reader can repeat it.

Columns: `method | path | operation_id | tag | state | reason | ground | evidence | request_ref |
response_ref`. `evidence` is what the handler's own work is, read from `origin/main` — the phrase
that makes the reason checkable, not a restatement of it.

`-` means "nothing declared": no request body, no response model, or no ground. It is never used in
the `reason` column of a dropped row.

## The drop table

<!-- BEGIN CAPTURE -->
| method | path | operation_id | tag | state | reason | ground | evidence | request_ref | response_ref |
|---|---|---|---|---|---|---|---|---|---|
| GET | / | service_root | health | dropped | transport | conveyor:liveness | root banner listing the service and its endpoint names | - | application/json |
| GET | /api/analyze | list_analysis_jobs | analyze | dropped | transport | conveyor:job-engine | paged tenant-scoped job row list with a status filter | - | AnalysisJobListResponse |
| POST | /api/analyze | start_analysis | analyze | dropped | deferred | caller: course_service/src/clients/analysis_client.py:48 (P6) | persists a job then runs the page-analysis and LLM site pipeline | AnalyzeRequest | AnalysisJobResponse |
| POST | /api/analyze/narrative | extract_narrative | analyze | dropped | no consumer | - | deterministic hook and quote selection from supplied analysed pages | NarrativeRequest | NarrativeResponse |
| POST | /api/analyze/pages | classify_pages | analyze | dropped | no consumer | - | deterministic HTML parse and page-type classification, no LLM | ClassifyRequest | ClassifyResponse |
| POST | /api/analyze/review | start_analyze_subreview | analyze | dropped | no consumer | - | reads a completed analysis and dispatches LLM reviews | src__domains__analyze__schemas__ReviewRequest | src__domains__analyze__schemas__ReviewJobResponse |
| GET | /api/analyze/review/{review_job_id} | get_analyze_subreview | analyze | dropped | no consumer | - | tiered lookup of a stored sub-review result | - | src__domains__analyze__schemas__ReviewResultResponse |
| DELETE | /api/analyze/{job_id} | cancel_analysis | analyze | dropped | transport | conveyor:job-engine | tenant-scoped status flip on a job row | - | CancelJobResponse |
| GET | /api/analyze/{job_id} | get_analysis | analyze | dropped | transport | conveyor:job-engine | memory, redis then database read of one job row | - | AnalysisResultResponse |
| GET | /api/analyze/{job_id}/artifacts | get_analysis_artifacts | analyze | dropped | transport | conveyor:artefact-store | cached then stored read of a job's artefacts | - | ArtifactsResponse |
| POST | /api/brand/batch-extract | batch_extract_brand | brand | dropped | transport | conveyor:job-engine | concurrent per-URL runs of the brand pipeline | BatchExtractRequest | BatchExtractResponse |
| POST | /api/brand/extract | extract_brand | brand | dropped | deferred | caller: w2v_orchestrator/src/api/routes/brand.py:47 (P5) | deterministic extraction from supplied analysis data; no LLM or vision | BrandExtractRequest | BrandResponse |
| POST | /api/brand/from-url | extract_brand_from_url | brand | dropped | deferred | caller: w2v_orchestrator/src/clients/brand_extractor_client.py:48 (P5) | fetch, DOM/colour/font scan and LLM stages, then persists the result | BrandFromUrlRequest | BrandResponse |
| POST | /api/brand/icp-from-url | analyze_icp_from_url | icp | dropped | deferred | caller: w2v_orchestrator/src/api/routes/brand.py:55 (P6) | scrapes landing, pricing and about pages, then an LLM produces the ICP | ICPFromUrlRequest | ICPResponse |
| POST | /api/brand/logo/download | download_brand_logo | brand | dropped | no consumer | - | SSRF-guarded download, content-type check, blob and row insert | LogoDownloadRequest | LogoAssetResponse |
| DELETE | /api/brand/logo/{asset_id} | delete_brand_logo | brand | dropped | no consumer | - | tenant-scoped asset row and blob delete | - | application/json |
| GET | /api/brand/logo/{asset_id} | get_brand_logo | brand | dropped | transport | conveyor:artefact-store | asset row lookup then stored image bytes | - | - |
| POST | /api/brand/score-artifact | score_artifact | artifact-score | dropped | deferred | caller: docs/MERQI_BRANDING_COACH_W2V_CONTRACT.md section 1 (P5) | static SEO, accessibility and performance scanners plus deterministic aggregation | ArtifactScoreRequest | ArtifactScoreResponse |
| POST | /api/brand/score-delta | score_delta | artifact-score | dropped | deferred | caller: docs/MERQI_BRANDING_COACH_W2V_CONTRACT.md section 2 (P5) | scores a before/after pair and diffs findings and category deltas | ScoreDeltaRequest | ScoreDeltaResponse |
| GET | /api/brands | list_brands | brands | dropped | no consumer | - | paginated tenant-scoped brand query | - | BrandListResponse |
| POST | /api/brands/compare | compare_brands | brands | dropped | deferred | part: P5.2 | deterministic field, colour and font comparison of two snapshots | BrandComparisonRequest | BrandComparisonResponse |
| GET | /api/brands/{domain} | get_brand | brands | dropped | no consumer | - | cache-first read of a stored brand row | - | BrandResponse |
| DELETE | /api/brands/{domain}/cache | invalidate_brand_cache | brands | dropped | no consumer | - | redis cache key deletion only | - | CacheInvalidationResponse |
| GET | /api/brands/{domain}/export | export_brand | brands | dropped | no consumer | - | flattens a stored brand to CSS variables and a fonts URL | - | BrandExportResponse |
| GET | /api/brands/{domain}/history | get_brand_history | brands | dropped | no consumer | - | reads stored brand version snapshots | - | BrandHistoryResponse |
| GET | /api/brands/{domain}/logo | get_brand_domain_logo | brands | dropped | transport | conveyor:artefact-store | latest domain asset streamed as bytes | - | - |
| PUT | /api/brands/{domain}/override | override_brand | brands | dropped | no consumer | - | applies overrides, diffs fields and performs a versioned upsert | BrandOverrideRequest | BrandOverrideResponse |
| GET | /api/brands/{domain}/quality | get_brand_quality_report | brands | dropped | no consumer | - | stored score or deterministic consistency scoring over the brand row | - | application/json |
| GET | /api/config/overrides | get_critique_config_overrides | critique | dropped | no consumer | - | enumerates redis overrides | - | ConfigOverridesResponse |
| DELETE | /api/config/overrides/{section}/{key} | delete_critique_config_override | critique | dropped | no consumer | - | deletes a redis override key | - | - |
| POST | /api/context/enrich | enrich_context | context | dropped | no consumer | - | deterministic merge of supplied and inferred context | object | application/json |
| POST | /api/context/infer | infer_context | context | dropped | no consumer | - | heuristic URL and page-type inference over supplied pages | object | application/json |
| GET | /api/context/profiles/ | list_context_profiles | context-profiles | dropped | no consumer | - | tenant-scoped profile row list; conveyor has no profile store | - | application/json |
| POST | /api/context/profiles/ | create_context_profile | context-profiles | dropped | no consumer | - | inserts a tenant-scoped profile row | ContextProfileCreate | ContextProfileResponse |
| DELETE | /api/context/profiles/{profile_id} | delete_context_profile | context-profiles | dropped | no consumer | - | deletes a tenant-scoped profile row | - | - |
| GET | /api/context/profiles/{profile_id} | get_context_profile | context-profiles | dropped | no consumer | - | reads one tenant-scoped profile row | - | ContextProfileResponse |
| POST | /api/context/validate | validate_context | context | dropped | no consumer | - | deterministic URL and pattern validation, no external calls | object | application/json |
| GET | /api/critique | list_critiques | critique | dropped | transport | conveyor:job-engine | paginated tenant-scoped job query | - | CritiqueListResponse |
| POST | /api/critique | create_critique | critique | dropped | deferred | caller: w2v_orchestrator/src/api/routes/critique.py:124 (P5) | fetch, deterministic a11y/perf/security scanners, LLM ux and summary | CritiqueRequest | CritiqueJobResponse |
| GET | /api/critique/analytics/domain/{domain} | get_critique_domain_analytics | critique | dropped | no consumer | - | SQL aggregates over stored jobs and findings | - | DomainAnalyticsResponse |
| GET | /api/critique/analytics/findings/top | get_critique_top_findings | critique | dropped | no consumer | - | ranks recurring findings from stored jobs | - | application/json |
| GET | /api/critique/analytics/summary | get_critique_tenant_summary | critique | dropped | no consumer | - | tenant-wide aggregates from stored rows | - | TenantAnalyticsResponse |
| GET | /api/critique/analytics/trends | get_critique_analytics_trends | critique | dropped | no consumer | - | domain score time series from stored jobs | - | DomainTrendResponse |
| POST | /api/critique/batch | create_critique_batch | critique | dropped | transport | conveyor:job-engine | creates N jobs over the critique pipeline | BatchCritiqueRequest | BatchCritiqueResponse |
| GET | /api/critique/batch/{batch_id} | get_critique_batch_status | critique | dropped | transport | conveyor:job-engine | batch counters recomputed from job rows | - | src__domains__critique__schemas__BatchStatusResponse |
| GET | /api/critique/compare | compare_critiques | critique | dropped | deferred | caller: w2v_orchestrator/src/clients/critique_client.py:65 (P5) | deterministic finding diff and score deltas | - | ComparisonResponse |
| GET | /api/critique/compare/history | list_critique_compare_history | critique | dropped | no consumer | - | recent completed jobs, diffs the latest two | - | HistoricalComparisonResponse |
| GET | /api/critique/schedules | list_critique_schedules | critique | dropped | transport | conveyor:scheduler | inline tenant-scoped schedule query | - | ScheduleListResponse |
| POST | /api/critique/schedules | create_critique_schedule | critique | dropped | transport | conveyor:scheduler | inserts a schedule row and computes the next cron run | CreateScheduleRequest | ScheduleResponse |
| DELETE | /api/critique/schedules/{schedule_id} | delete_critique_schedule | critique | dropped | transport | conveyor:scheduler | tenant-scoped schedule row delete | - | - |
| GET | /api/critique/schedules/{schedule_id} | get_critique_schedule | critique | dropped | transport | conveyor:scheduler | tenant-scoped schedule row read | - | ScheduleResponse |
| PUT | /api/critique/schedules/{schedule_id} | update_critique_schedule | critique | dropped | transport | conveyor:scheduler | mutates the row and recomputes the cron schedule | UpdateScheduleRequest | ScheduleResponse |
| GET | /api/critique/suppressions | list_critique_suppressions | critique | dropped | no consumer | - | inline tenant-scoped suppression query | - | SuppressionListResponse |
| POST | /api/critique/suppressions | create_critique_suppression | critique | dropped | no consumer | - | inserts a suppression row | CreateSuppressionRequest | SuppressionResponse |
| DELETE | /api/critique/suppressions/{suppression_id} | delete_critique_suppression | critique | dropped | no consumer | - | tenant-scoped suppression row delete | - | - |
| DELETE | /api/critique/{job_id} | delete_critique | critique | dropped | transport | conveyor:job-engine | deletes the job and its findings | - | - |
| GET | /api/critique/{job_id} | get_critique | critique | dropped | transport | conveyor:job-engine | single tenant-scoped job row | - | CritiqueResultResponse |
| GET | /api/critique/{job_id}/export/csv | export_critique_csv | critique | dropped | transport | conveyor:artefact-store | findings rows rendered to CSV bytes | - | - |
| GET | /api/critique/{job_id}/export/sarif | export_critique_sarif | critique | dropped | transport | conveyor:artefact-store | findings rows rendered to SARIF JSON | - | application/json |
| GET | /api/critique/{job_id}/findings | get_critique_findings | critique | dropped | no consumer | - | in-route filter and paginate of stored findings | - | src__domains__critique__schemas__FindingsListResponse |
| GET | /api/critique/{job_id}/findings/unified | get_critique_unified_findings | critique | dropped | no consumer | - | converts stored findings to the unified schema | - | application/json |
| POST | /api/critique/{job_id}/findings/{finding_id}/acknowledge | acknowledge_critique_finding | critique | dropped | no consumer | - | inserts a suppression and flips a finding flag | AcknowledgeFindingRequest | application/json |
| GET | /api/critique/{job_id}/progress | stream_critique_progress | critique | dropped | transport | conveyor:job-engine | SSE from redis with a database ownership preflight | - | - |
| GET | /api/critique/{job_id}/report | get_critique_report | critique | dropped | transport | conveyor:artefact-store | regenerates html and json from stored markdown | - | - |
| GET | /api/critique/{job_id}/report/pdf | get_critique_report_pdf | critique | dropped | transport | conveyor:artefact-store | html report to pdf bytes | - | - |
| POST | /api/critique/{job_id}/rerun | rerun_critique | critique | dropped | transport | conveyor:job-engine | clones the job and forces a rescan | RerunRequest | CritiqueJobResponse |
| PATCH | /api/critique/{job_id}/tags | update_critique_job_tags | critique | dropped | no consumer | - | overwrites the tags column | TagsUpdateRequest | CritiqueResultResponse |
| POST | /api/cro/score | score_cro | cro | dropped | deferred | caller: w2v_orchestrator/src/clients/analysis_scorecard_client.py:156 (P6) | deterministic CRO scorecard over supplied pages plus an optional LLM judge | CroScoreRequest | CroScoreResponse |
| GET | /api/domain/{domain}/history | get_review_domain_history | review | dropped | no consumer | - | rescored stored jobs; trend computed from the score sequence | - | DomainHistoryResponse |
| POST | /api/marketing/scorecard | score_marketing | marketing | dropped | deferred | caller: w2v_orchestrator/src/clients/analysis_scorecard_client.py:206 (P6) | deterministic marketing scorecard over supplied context plus optional LLM assessments | MarketingScorecardRequest | MarketingScorecardResponse |
| GET | /api/review | list_reviews | review | dropped | transport | conveyor:job-engine | paginated tenant-scoped job rows; no computation | - | ReviewListResponse |
| POST | /api/review | start_review | review | dropped | no consumer | - | LLM-backed qualitative review; creates a job row then drives prompt to text | src__domains__review__schemas__ReviewRequest | src__domains__review__schemas__ReviewJobResponse |
| POST | /api/review/batch | start_batch_review | review | dropped | no consumer | - | creates job and batch rows over the same LLM review pipeline | BatchReviewRequest | BatchReviewResponse |
| GET | /api/review/batch/{batch_id} | get_review_batch_status | review | dropped | transport | conveyor:job-engine | batch progress recomputed from child job rows | - | src__domains__review__schemas__BatchStatusResponse |
| GET | /api/review/compare | compare_reviews | review | dropped | no consumer | - | deterministic score deltas between two stored completed jobs | - | ReviewComparisonResponse |
| GET | /api/review/{job_id} | get_review | review | dropped | transport | conveyor:job-engine | cache-first then job row lookup | - | src__domains__review__schemas__ReviewResultResponse |
| GET | /api/review/{job_id}/findings | get_review_findings | review | dropped | no consumer | - | normalises stored category scores into a filtered finding list | - | src__domains__review__schemas__FindingsListResponse |
| GET | /api/review/{job_id}/progress | get_review_progress | review | dropped | transport | conveyor:job-engine | SSE loop polling the job row once a second | - | - |
| GET | /api/review/{job_id}/report | get_review_report | review | dropped | transport | conveyor:artefact-store | renders markdown, html, json and pdf from stored results | - | - |
| GET | /api/review/{job_id}/summary | get_review_summary | review | dropped | no consumer | - | deterministic aggregate score, grade and top findings over stored results | - | ReviewSummary |
| POST | /api/review/{job_id}/video | request_review_video | review | dropped | no consumer | - | POSTs findings to the video compile endpoint and returns its job id | - | ReviewVideoResponse |
| GET | /api/scoring/weights | get_critique_scoring_weights | critique | dropped | no consumer | - | operator-only redis weights | - | ScoringWeightsResponse |
| PUT | /api/scoring/weights | update_critique_scoring_weights | critique | dropped | no consumer | - | operator-only redis overrides | ScoringWeightsUpdateRequest | ScoringWeightsResponse |
| POST | /api/seo/audit | audit_seo | seo_audit | ported | - | analyse.site (crates/analysis) | deterministic base scan plus the seven audit groups over supplied URL and HTML | SeoAuditRequest | SeoAuditResponse |
| GET | /api/usage | get_review_usage | review | dropped | no consumer | - | reads stored daily usage counters for a tenant and date range | - | UsageListResponse |
| GET | /health | health_check | health | dropped | transport | conveyor:liveness | component health rollup | - | - |
| GET | /health/live | health_live | health | dropped | transport | conveyor:liveness | liveness probe, no dependencies | - | - |
| GET | /health/ready | health_ready | health | dropped | transport | conveyor:liveness | readiness probe over database, redis and storage | - | - |
| GET | /health/startup | health_startup | health | dropped | transport | conveyor:liveness | startup probe | - | - |
| GET | /healthz | health.live | health | dropped | transport | conveyor:liveness | duplicate liveness probe on the probe path | - | application/json |
| GET | /metrics | prometheus_metrics | health | dropped | transport | conveyor:liveness | Prometheus exposition of process counters | - | - |
| GET | /readyz | health.ready | health | dropped | transport | conveyor:liveness | duplicate readiness probe on the probe path | - | - |
<!-- END CAPTURE -->

## Counts

| State | Rows |
|---|---|
| ported | 1 — `audit_seo` |
| dropped: transport | 36 |
| dropped: deferred | 11 |
| dropped: no consumer | 44 |
| **total** | **92** |

Read the `transport` count with its guard in mind: every one of those 36 rows names the conveyor
surface that replaces it. Of the 11 deferred rows, 10 name a measured caller and one — `compare_brands`
— is required by part P5.2 and says so.

## Consumers and their calls

Measured by searching the estate for calls to the service's base URL and for the literal paths in
the table above. Covered: the orchestrator's source and scenario scripts, the frontend's source, the
course service, the content service, the video production service, the compose files and the
repository's own scripts. The service resolves through the discovery name `analysis`.

| Operation | Caller |
|---|---|
| `start_analysis` | `course_service/src/clients/analysis_client.py:48` |
| `get_analysis` | `course_service/src/clients/analysis_client.py:55` (poll at `:64`) |
| `create_critique` | `w2v_orchestrator/src/api/routes/critique.py:124`; `w2v_orchestrator/src/clients/critique_client.py:52` |
| `list_critiques` | `w2v_orchestrator/src/api/routes/critique.py:142` |
| `get_critique` | `w2v_orchestrator/src/api/routes/critique.py:172`; `critique_client.py:56` |
| `compare_critiques` | `w2v_orchestrator/src/api/routes/critique.py:158`; `critique_client.py:65` |
| `stream_critique_progress` | `w2v_orchestrator/src/api/routes/critique.py:182` (`proxy_stream`) |
| `get_critique_report_pdf` | `w2v_orchestrator/src/api/routes/critique.py:183`; `w2v_frontend/src/lib/api/downloads.ts` (`critiqueReportPdf`) |
| `score_cro` | `w2v_orchestrator/src/clients/analysis_scorecard_client.py:156` |
| `audit_seo` | `w2v_orchestrator/src/clients/analysis_scorecard_client.py:188` |
| `score_marketing` | `w2v_orchestrator/src/clients/analysis_scorecard_client.py:206` |
| `extract_brand_from_url` | `w2v_orchestrator/src/api/routes/brand.py:40`; `brand_extractor_client.py:48` |
| `extract_brand` | `w2v_orchestrator/src/api/routes/brand.py:47` |
| `analyze_icp_from_url` | `w2v_orchestrator/src/api/routes/brand.py:55` |
| `score_artifact` | contract: `docs/MERQI_BRANDING_COACH_W2V_CONTRACT.md` §1; exercised by `w2v_orchestrator/scripts/e2e_scenarios/score_artifact_happy.json` |
| `score_delta` | contract: `docs/MERQI_BRANDING_COACH_W2V_CONTRACT.md` §2 |

Fourteen of these are code callers; two cite a downstream product contract with no code on this box.
The remaining 76 operations have no caller in that set of repositories — that is the measurement the
`no consumer` rows rest on, and it is falsifiable by searching the same trees again.

## What this corrects

- The issue's "46 `operation_id`s across 9 domains" is stale. Measured: **92 operations across 12
  domain tags plus health**. This document is the measurement; the issue body is not.
- The issue's open question 1 ("which of the 46 captured operations has a measured consumer?") is
  answered per row: 16 of 92 have one, each with a file and line below.
- P5's `analyse.brand` comparison is `compare_brands`, which has no production caller — it is
  required by part P5.2, and the row says so rather than pretending a caller exists.

## Deviations, recorded rather than hidden

1. **No live request was issued.** Around two thirds of these operations mutate production rows
   (create, cancel, delete, batch, schedule). The pair per row is the **contract pair** read from the
   committed served document — request schema, response schema, declared success status — not a
   recorded transaction. This capture therefore supports coverage and wiring claims, not semantic
   parity. Behavioural samples, where a later part needs them, come from the source's own tests.
2. **One pin for the document, not one per row.** Repeating a digest 92 times is noise; each row
   addresses its pair by schema name into the committed document, which is pinned once.
3. **The gitops manifest does not match the pod** (see the pin). Deployment drift is out of scope for
   this unit; the capture records what runs.
4. **The P3/P4 port was written from the stale checkout.** The SEO scanner on `origin/main` has
   moved since: `geo.py` (+111 lines), `schema_coverage.py` (+97), `eeat.py` and `internal_linking.py`
   (±14), and the check-id set changed at the edges (`+seo-robots-txt-errored`,
   `+seo-sitemap-errored`, `−seo-keyword-intent`). The scoring kernel did **not** move — the group
   weights, the decay, the floor, `k` and the severity weights are identical, and `composite.py`
   only adds unmeasured-bucket handling, which does not apply when every group runs. The ported row's
   `ground` carries this footnote; re-porting `geo` and `schema_coverage` is an owner decision, not a
   silent edit inside P1.

## How to re-capture

```
# the digest the running pod reports
kubectl -n w2v get pod -l app=analysis-service -o jsonpath='{.items[0].status.containerStatuses[0].imageID}'

# the contract it serves, read over the pod's own loopback
kubectl -n w2v exec deploy/analysis-service -- python3 -c \
  'import urllib.request,sys;sys.stdout.write(urllib.request.urlopen("http://127.0.0.1:6303/openapi.json",timeout=15).read().decode())' \
  > analysis-openapi-<date>.json

shasum -a 256 analysis-openapi-<date>.json
```

Re-capture both together. A document from one pod and a digest from another is not a pin, it is a
coincidence.
