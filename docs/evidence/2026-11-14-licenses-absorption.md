# licenses absorption — evidence (2026-11-14)

Issue #45 · measurements taken at `4f9b03e` on branch `issue/45-licenses`; commits after that
point touch only this document and the generated design pack. The
reference service is pinned by commit and by image digest; no private hostname, address, token or
tenant identifier appears in this file.

## The reference and its pin

| Fact | Value |
|---|---|
| Reference repository | `nonagenticai/licenses` @ `4e8a08d25a239c5bd5c563e8fc5f49d6576e71e4` — the commit the live image's `sha-4e8a08d` tag names |
| Local checkout | `4885666`. `git diff --stat 4e8a08d..HEAD -- main.py models.py services/` is EMPTY and the worktree is clean for those paths: the service surface is byte-identical to the pin (measured 2026-11-14) |
| Capture | `contract/manifest.json` — `captured_at` 2026-11-14T07:35:25Z, image `sha256:13093d39f55999d2092a5ac60ebb61104fdc22057bf63d168313dbe5c740cee5`, lock sha256 `9c70313503614e7ddd0f197402c8b89a70d663e3bd54e5ac6e94a969f96413d8`, OpenAPI 3.1.0, 43 paths / 60 operations / 57 schemas |
| Live image (measured 2026-11-14) | Deployment `licenses` 2/2 Running `sha256:20dd796a9260708f19730ccb3dc01726567e1ef8c3a8ec9e22629190881ab8df` |
| Live database and role (measured) | database `licenses`, owner `licenses_user_v2`; that role is the service's own pair, and this unit keeps it — the rewrite is the same service, not a new one |

The live pods have moved past the captured digest since the capture at 07:35Z. The lock remains
the contract this unit ports against (60 operations); the live digest is a fact for the #95
window, not a reason to re-capture. The capture was byte-identical after `jq -S` to
`docs/openapi.json` at the pin (`contract/compare-report.txt`).

## Consumers and their calls

| Consumer | Calls today | Consuming source |
|---|---|---|
| conveyor backend | `POST /internal/tenants/{id}/bundle-token`, `GET /licenses/active`, `POST /license/introspect` | `apps/server/src/admin/upstream_tokens.rs:12,92`; `apps/server/src/worker.rs:171-187,574-594`; `crates/auth/src/licence.rs:14-18,197-251` (`LICENCES_BASE_URL`) |
| agent platform — runtime | `POST /internal/tenants/{id}/bundle-token`; `POST /internal/tenants/{id}/bundles/{key}/ensure` | `agent/crates/agent-api/src/tenants/app_licence.rs` (the minter); `agent/crates/agent-api/src/licenses_admin.rs:371` (ensure); `agent/crates/agent-api/src/config.rs:182,321,1194` |
| agent platform — signup | `PUT /tenants/{id}`, `POST /licenses/user/{sub}`, `POST /licenses/{id}/tokens`, `GET /apps/{key}`, `GET /apps/{key}/plans`, `GET /regions` | `agent/crates/agent-api/src/licenses_admin.rs:168-333` (`register_tenant`, `create_licence`, `mint`, `app_and_plan`, `region_id`); driven by `agent/crates/agent-api/src/signup/provisioning.rs`; the tests' fake mirrors the same calls at `agent/crates/agent-api/tests/common/fakes.rs:558-560` |
| portal-bff | `GET /licenses/user/{sub}`, `POST /licenses/user/{sub}`, `GET /licenses/admin/all`, `POST /licenses/{id}/regenerate-token` | `portal-bff/src/routes/licenses.py:109,205,237,348` |
| worker_management | the one tokenless BYO route: `GET /tenants/{uuid}/byo-mode` on the trusted network, failing open on any non-200. The auth exemption is the exact route shape and a UUID selector only (`byo_mode_open_selector`, `apps/licenses/src/auth.rs:315-326`); a name selector is an ordinary 401, and the write variants and the per-app paths keep their role gates | contract: `apps/licenses/src/router/tenant_byo.rs:13-20,273-277,301-305`. The WM checkout is absent under `~/Movies` (measured), so no WM source line is cited |
| billing | `GET /internal/licenses` | `billing_service/src/services/licence_budget_reconcile.py:205`; `billing_service/src/services/attribution_audit.py:452-463`; `billing_service/src/services/quota.py:269` |
| four CronJobs | the same image with `python scripts/<job>.py`, no arguments | `gitops-production/clusters/strato/services/licenses/cronjob-expire-stale-licences.yaml`, `…-reap-expired-licences.yaml`, `…-revoke-expired-tokens.yaml`, `…-metering-reconcile.yaml` |

`license-authz` is deliberately absent: it enforces at the ingress and keeps calling this service
over HTTP. It is out of scope (issue #45, Out of scope).

## What each #45 scenario shows

| Scenario | Result | Where |
|---|---|---|
| P1.1 capture the document | `contract/manifest.json` records date, digest, lock sha256, 43/60/57; `compare-report.txt` says `compare: byte-identical after jq -S` against the pin's `docs/openapi.json`. The LIVE service (image `20dd796a…`, moved since the capture) serves a document that is also byte-identical after `jq -S` to the lock — sha256 `9c703135…` on both sides, `contract/live-compare.txt`, measured 2026-11-14 | `apps/licenses/contract/` |
| P1.2 synthetic fixtures, no private refs | `bash scripts/ci/no-private-refs.sh` → exit 0, `OK: no private infrastructure references in <N> tracked files (6 allowlisted)` (the script prints the live count; the 2026-11-14 run reported 1525 while this file was being added — the count rises with every tracked file, so it is not pinned). The fixtures are `contract/stubs/synthetic-corpus.jsonl` and `contract/golden/` (all-zero UUIDs, `golden-unknown-app`, `golden-synthetic-unknown-token`); case ids in a shadow report are hashes of `method path body` | this head |
| P2.1 served document equals the lock | `conveyor-licenses --check-lock` → `lock OK: 60 operations` on stderr, exit 0: all 60 captured operations are served and the served document matches the lock under the contract projection. The flag runs before configuration and touches no database | `4f9b03e` (re-measured after the test flip and the two main merges; same result) |
| P2.2 migrations equal the live schema | `bash apps/licenses/contract/schema-diff.sh` → `schema OK: the migrations produce the live schema`, exit 0. The script applies `migrations/0001_baseline.sql` in one transaction to a throwaway `postgres:18.6`, dumps it with the live dump's options and diffs it against `contract/schema.lock.sql` (22 tables; 24 relations including the two `pg_stat_statements` views; the Rust ledger, ownership, privileges and tablespaces are excluded by construction). `CHECK` expressions are compared as token streams so deparse-only parenthesisation cannot fail the diff. `tests/migrations.rs` then pins `migrate` and `migrate --baseline` against a fresh database | this head |
| P2.3 the mint envelope | `a_mint_returns_the_seven_fields_and_the_raw_token_introspects`: `POST …/bundle-token?app=demo` answers 201 with `{token, expires_at, tenant_id, bundle, app, license_id, token_id}` — the raw token is 43 base64url chars with no padding, only its SHA-256 is stored, and the raw token introspects 200 on the same licence | `apps/licenses/tests/internal_bundles.rs:236-295` |
| P2b.1 four recurring commands' row effects | the effect table below; each command has an armed-transition test and a dry-run/vacuity test | `apps/licenses/tests/command_*.rs`, `billing_teardown.rs` |
| P2b.2 command shapes and exit codes | the argv/exit table below; the reference's asymmetry is reproduced | `apps/licenses/tests/command_argv.rs` |
| P3.1 golden replay (`contract/verify.sh`) | **NOT MEASURED at `32f9dbd`: there was no `golden/` set** (the measured capture is the next row). `verify.sh --candidate …` printed `cases=0 in apps/licenses/contract/golden (not captured yet) — a vacuous zero is not a pass` and exited 3. The comparator is proven non-vacuous by `shadow.sh --self-test`: `self-test ok: identical bodies compare equal; a mutated field is unexplained`, exit 0 | `32f9dbd` |
| P3.1 golden replay, captured corpus | The default `golden/` corpus now exists: **6 read-only, in-contract cases** captured from the live reference service and replayed against `conveyor-licenses`. Scope: `GET /health`, `GET /health/live`, `GET /health/ready`, the unknown-token `POST /license/introspect` (200, `active:false`, `reason:"token_not_found"`), and the two unauthenticated 401s (`GET /licenses/active?tenant_id=…&app_key=…`, `GET /licenses/user/<uuid>`). No production tenant, app key or token is in the corpus; authenticated success cases need a synthetic seed and remain integration-test and #95 coverage. Capture: `apps/licenses/contract/capture-golden.sh --live http://127.0.0.1:18544 --out apps/licenses/contract/golden` against a port-forward to the service pod — deliberately **not** through the public ingress host, whose istio ext-authz filter answers its own `text/plain` 401 (`Unauthorized: Bearer token required`, no `WWW-Authenticate`) before the application sees the request (measured 2026-11-14); the application's own 401 is the captured JSON envelope `{"error":{"message":"Missing authentication token","status":401}}`. Replay: throwaway `postgres:18.6` (container `slice45-licenses-pg`, host port 59315), `conveyor-licenses migrate` → `migrate OK: 1 migration(s) embedded`, `serve` on port 18545, then `bash apps/licenses/contract/verify.sh --candidate http://127.0.0.1:18545`: every case `ok`, **`cases=6 unexplained=0`**, exit 0. Candidate build: binary sha256 `70f3aaab4ecff9a1bdead02d928ff786d2d49a2347092634495bba261335420b`, built by `rbuild -- build -p conveyor-licenses --locked` from the tree at `a0e757b` (the test-flip commit; this slice adds only `contract/capture-golden.sh`, `contract/golden/` and this document — no Rust change). No tolerance was needed: the six cases matched without excusing anything, and `tolerances.json` stayed empty | `apps/licenses/contract/golden/`, `apps/licenses/contract/capture-golden.sh` |
| P3.2 live shadow window | **NOT MEASURABLE, moved to #95.** The service captures no request bodies: its `licenses.access` line carries `method, path, status, duration_ms, azp, realm, owner` and nothing else, and the pinned source has no `X-Capture` surface (measured 2026-11-14 at 09:57Z against the live pods and the pin; recorded on #45). The live pod's own log tail, read 2026-11-14, carries request lines of the form `INFO: … "GET /health/ready HTTP/1.1" 200 OK` — method, path, status, no body. `shadow.sh --live` therefore exits 3 by design and says so. The harness that would consume a window is built and proven: `shadow.sh` ranks cases by `sha256(method path body)`, `normalize.sh` masks only minted tokens and RFC3339 timestamps, `verify.sh`/`shadow.sh` treat `cases=0` as exit 3, and `--self-test` reports `self-test ok: identical bodies compare equal; a mutated field is unexplained` | `apps/licenses/contract/shadow.sh`, `tolerances.json`; #95 |
| P3.3 every tolerance carries a reason | `jq '[.tolerances[] | select((.reason // "") == "")] | length'` → `0`. The list is empty because the captured corpus needed no tolerance (6/6 cases matched byte-for-byte after `normalize.sh`); the note still names the four known differences that must be attached to their cases when the authenticated set lands. ⚠️ A `jq` check over an EMPTY list is vacuous by construction — the non-vacuity that matters is the comparator's, proven by `shadow.sh --self-test` (a mutated field is unexplained) | `apps/licenses/contract/tolerances.json` |
| P4 cutover | moved to #95; this unit ships no deployment and no production call | #95 |

## The four recurring commands (P2b.1)

All four write **one JSON line on stdout on every path**, including configuration failures; the
verdict rides the exit code. Configuration is environment-only, so the CronJob manifest remains
the only knob. Every sweep compares an independent hand-written count with its own selection at
one captured instant, so a broken or vacuous selection cannot read as success.

| Command | Selection predicate | Row effects | Exit codes | Rust test |
|---|---|---|---|---|
| `expire-stale` | `license.status = 'active' AND expires_at IS NOT NULL AND expires_at < $now`, oldest first, `LIMIT EXPIRE_LIMIT` (default 500, 1..=5000). Armed only, `FOR UPDATE` plus a re-check of `status='active'` | `UPDATE license SET status='expired', updated_at=$now WHERE id = ANY($ids) AND status='active'` in one transaction. A dry run takes no locks and writes nothing (and is NOT converged while stale rows exist) | 0 converged; 1 not converged (dry-run-over-stale, vacuous, truncated, over-fired, dry-run-wrote); 2 config/DB | `tests/command_expire_stale.rs` (6 tests; `an_armed_run_expires_exactly_the_stale_set_and_converges`, `a_dry_run_is_the_default_and_reports_the_selected_ids`) |
| `reap-expired` | `license.status = 'expired' AND expires_at IS NOT NULL AND expires_at < now - REAP_GRACE_DAYS` (default 30 days), oldest first, limit 500. Armed only, `FOR UPDATE` plus a re-check of `status='expired'` | `UPDATE license SET status='revoked', revoked_at=COALESCE(revoked_at,$now), updated_at=$now WHERE id = ANY($ids) AND status='expired'`; after the commit, tears down billing's quota mirror and `license_sync` token budget per archived licence (4 concurrent). A dry run reports `teardown_would_call` and sends nothing; a missing `METERING_SERVICE_URL` is `skipped` and un-converged | 0 converged; 1 not converged (including billing residue and the skipped-teardown manifest defect); 2 config/DB | `tests/command_reap_expired.rs` (7 tests; `an_armed_run_reaps_exactly_the_reapable_set_and_converges`, `a_billing_failure_does_not_change_the_reap_but_is_visible_in_the_payload`); `tests/billing_teardown.rs` (6 tests) |
| `revoke-expired-tokens` | `token.revoked = false AND expires_at IS NOT NULL AND expires_at < $now`, oldest first, limit 500 | `UPDATE token SET revoked = true, revoked_at = $now WHERE id = ANY($ids)` in one transaction; a dry run writes nothing and is not converged while expired tokens exist | 0 converged; 1 not converged; 2 config/DB | `tests/command_revoke_expired_tokens.rs` (5 tests; `an_armed_run_revokes_exactly_the_expired_set_and_converges`, `a_dry_run_writes_nothing_and_reports_the_scripts_problems`) |
| `reconcile-metering` | live licences whose effective metering owes billing: `status='active' AND (expires_at IS NULL OR expires_at > now())` and any of plan quota/token budget/licence overrides non-null; optional `RECONCILE_LICENSE_ID` narrowing; ordered by `created_at`, limit 500, skipping already-stamped licences unless `RECONCILE_REVALIDATE` | read-before-write: `GET /api/budget/list/{tenant}` — create absent, re-push a different `budget_tokens`, leave an unreadable list UNKNOWN; `GET /api/quota/check` — create absent, leave and report drift unless `RECONCILE_FORCE_PLAN_QUOTA`; stamps verified coverage. The same runner serves `POST /licenses/admin/metering/reconcile` | 0 converged; 1 not converged (unknown coverage, failures, dry-run uncovereds, vacuous/truncated); 2 config/DB/transport | `tests/command_reconcile_metering.rs` (10 tests; `an_uncovered_licence_gets_both_rows_created_and_is_stamped`, `a_dry_run_reads_but_pushes_nothing_and_reports_the_same_counts`); `tests/reconcile_route.rs` (5 tests, all passing at this head; its lock assertion expects success after `a0e757b` — the earlier failure and its fix are recorded below) |

## Command shapes and exit codes (P2b.2)

The reference scripts are invoked with **no arguments**. Three inspect `sys.argv` and refuse any
extra token with a usage line on stderr, one `_fail` JSON line on stdout and exit 2;
`revoke_expired_tokens.py` never looks at `sys.argv`, so extra words are ignored and the sweep
runs. The Rust binary applies that table **before clap**, so the CronJob path is unchanged, and
clap-only niceties cannot erase the audit line.

| Invocation | stdout | stderr | exit |
|---|---|---|---|
| `conveyor-licenses expire-stale` | one JSON line (the run's audit line, or its own config refusal) | — | 0/1/2 |
| `conveyor-licenses expire-stale surplus` | `{"actor":null,"error":"this job takes no arguments; configure it via EXPIRE_* env","event":"expire_stale_licenses","exit_code":2}` | `usage: conveyor-licenses expire-stale (configured via EXPIRE_* env)` | 2 |
| `conveyor-licenses reap-expired surplus` | the same shape with the `reap_expired_licenses` event and `REAP_*` | the matching usage line | 2 |
| `conveyor-licenses reconcile-metering surplus` | the same shape plus `"license_id":null` | the matching usage line | 2 |
| `conveyor-licenses revoke-expired-tokens surplus` | the real sweep's one JSON line; the extra word is dropped (flags are still handed to clap) | — | 0/1/2 |
| `conveyor-licenses --check-lock` | empty | `lock OK: 60 operations` or `error: …` | 0/1 |
| `conveyor-licenses --print-openapi` | the document | — | 0/1 |
| `serve`, `migrate`, `migrate --baseline` | unchanged | `migrate OK: …` / `baseline OK: …` | 0/1 |

Tests: `tests/command_argv.rs` (6 tests) — the refusal payload is asserted byte-for-byte, the
extra-word revoke run asserts the sweep actually runs, and the no-argument path asserts the
config refusal rather than the argv refusal.

## Test inventory

Measured on m1 at `4f9b03e` (a throwaway Postgres 18 cluster per run):
`rbuild -- test -p conveyor-licenses --locked` → **350 passed, 0 failed** across 36 test
binaries, exit 0. The per-target counts below are from the complete run:

| Test target | Tests | Failed |
|---|---:|---:|
| `src/lib.rs` (unit) | 45 | 0 |
| `src/main.rs` (unit) | 0 | 0 |
| `tests/billing_metering.rs` | 7 | 0 |
| `tests/billing_teardown.rs` | 6 | 0 |
| `tests/bundle_apply_rows.rs` | 8 | 0 |
| `tests/bundle_ensure_apply.rs` | 8 | 0 |
| `tests/bundle_ensure_plan.rs` | 25 | 0 |
| `tests/bundle_grants.rs` | 5 | 0 |
| `tests/catalogue_apps.rs` | 10 | 0 |
| `tests/catalogue_offerings.rs` | 10 | 0 |
| `tests/catalogue_plans.rs` | 10 | 0 |
| `tests/command_argv.rs` | 6 | 0 |
| `tests/command_expire_stale.rs` | 6 | 0 |
| `tests/command_reap_expired.rs` | 7 | 0 |
| `tests/command_reconcile_metering.rs` | 10 | 0 |
| `tests/command_revoke_expired_tokens.rs` | 5 | 0 |
| `tests/contract_drift.rs` | 1 | 0 |
| `tests/dependencies.rs` | 7 | 0 |
| `tests/internal_bundles.rs` | 17 | 0 |
| `tests/internal_ensure.rs` | 13 | 0 |
| `tests/internal_reads.rs` | 9 | 0 |
| `tests/internal_seat.rs` | 8 | 0 |
| `tests/introspect.rs` | 6 | 0 |
| `tests/issuance.rs` | 24 | 0 |
| `tests/licence_admin.rs` | 8 | 0 |
| `tests/licence_tokens.rs` | 33 | 0 |
| `tests/metrics.rs` | 1 | 0 |
| `tests/migrations.rs` | 6 | 0 |
| `tests/reconcile_route.rs` | 5 | 0 |
| `tests/revoke.rs` | 7 | 0 |
| `tests/tenant_byo.rs` | 6 | 0 |
| `tests/tenant_offerings.rs` | 8 | 0 |
| `tests/tenants_crud.rs` | 8 | 0 |
| `tests/user_licence_writes.rs` | 9 | 0 |
| `tests/user_licences.rs` | 6 | 0 |
| Doc-tests | 0 | 0 |
| **Total** | **350** | **0** |

### The two stale "port is incomplete" assertions (resolved in `a0e757b`)

- `contract::tests::the_lock_check_names_the_unported_operations` — formerly in
  `apps/licenses/src/contract/mod.rs` (the test was renamed; the line range was 143-176).
  It did `check(&served(), ROUTES).expect_err("the port is incomplete by construction in slice 1")`;
  with 60 of 60 operations served, `check` returns `Ok(())`, so it panicked:
  `the port is incomplete by construction in slice 1: ()`.
- `tests/reconcile_route.rs::the_lock_check_never_names_this_route` — the `expect_err` assertion
  at :524 (the test function begins at :530) panicked:
  `the port is incomplete by construction: ()`.

Both encoded the slice-era state in which `--check-lock` HAD to fail. **Resolved in `a0e757b`**:
the contract test is now `the_lock_check_accepts_the_complete_port` (asserts `check` returns Ok)
and the reconcile-route test calls `check(...).expect("the served contract equals the capture")`.
The crate's test command is green after that change; the remaining DoD gate is the full workspace
suite at the PR head, recorded in the PR.

## Recorded deviations

- **Guarded request-path invariants.** `apps/licenses/src/router/internal_ensure.rs:573,650-652`
  and `apps/licenses/src/auth.rs:653` use `expect`/`unreachable!` after a check that makes the bad
  state impossible; disclosed for completeness. The only added `.unwrap()`s are test-only
  (`db/tokens.rs`). There are no `todo!()`, `unimplemented!()`, `panic!` or `#[ignore]` in the crate.
- **Shared transport change.** This lane also extends `crates/auth/src/service.rs` with
  `with_timeout`/`delete` for the billing client's transport; the licenses billing tests exercise
  both. The `crates/auth` crate has no unit test of its own for the two methods.
- **Offering markup and access refusals.** Both failures answer the reference's **502** and
  restore the prior posture; pinned by
  `tests/tenant_offerings.rs::a_rejected_markup_restores_the_posture_and_answers_502` and
  `::an_access_refusal_restores_the_prior_posture_and_answers_502`.
- **CLI JSON is compact.** `serde_json::to_string`, not the script's `json.dumps` spacing; key
  order is the sorted order the scripts used (`sort_keys=True`). The refusal usage line names the
  Rust binary (`conveyor-licenses <subcommand>`) because that is the program being invoked; the
  reference printed the script's basename.
- **axum versus FastAPI on query-parameter syntax.** FastAPI rejects some malformed/repeated
  query parameters with a 422 `HTTPValidationError`; axum's extractor can answer 400. The
  schemas are aligned to `HTTPValidationError.detail` where a body is validated, and the split is
  documented per route; it is not claimed that every extractor-level rejection is byte-identical.
- **Implemented but untested.** The app-create unique-key race (`lost_race`) and the byo-mode
  503 `byo_mode_unresolved` fault path are implemented; no test drives either fault at this head.
- **Forwarded-token arm.** It uses the service verifier's configured trusted issuers
  (`KEYCLOAK_TRUSTED_ISSUERS`/`KEYCLOAK_URL`); it does not add a second, forwarded-token-specific
  trust list. A caller whose issuer is not in that list is refused on both arms.
- **`released_by_revoke_of`** records the subject UUID itself, not the `subject:<uuid>` spelling.

None of these is hidden behind a tolerance: `tolerances.json` is empty, so every one is either a
difference in a field a golden case would compare or an arm with no case yet.

## Cutover handoff to #95

1. **Image.** The `apps/licenses` image row is expected from the publish job's matrix once this PR
   merges; no Rust licenses image exists before that merge. #95 pins the digest it deploys.
2. **Conveyor.** Set `LICENCES_BASE_URL` to the deployed Service; the three call sites in
   `apps/server/src/{admin/upstream_tokens.rs,worker.rs}` and `crates/auth/src/licence.rs` are the
   only wiring, and the env value is the rollback flag.
3. **CronJobs.** Switch each job's command from `python scripts/<job>.py` to
   `conveyor-licenses expire-stale|reap-expired|revoke-expired-tokens|reconcile-metering`;
   schedules, env and the one-JSON-line contract stay. The argv table above is the proof that no
   schedule or argument change is required.
4. **Database.** `migrate --baseline` stamps a database already at the Python head without
   running DDL; the live pair (`licenses` / `licenses_user_v2`) is kept.
5. **Probe window.** #95 P3.1/P3.2 records the live window. Because the service captures no
   request bodies, a live window must come from the candidate's own traffic; and the recorded
   golden set (`capture-goldens.sh`/`oracle.sh` from a pinned synthetic seed) is the missing
   input for `verify.sh`'s `unexplained=0` leg. Neither is claimed here.
6. **Retirement.** `nonagenticai/licenses` is not archived until every consumer in the table
   above is off it. `license-authz` is not a consumer to switch; it stays external.
