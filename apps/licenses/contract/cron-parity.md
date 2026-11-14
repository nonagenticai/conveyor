# Cron parity — Python scripts vs Rust subcommands (#45 P2b.1)

Issue #45 P2b.1 asks: *"Given the four CronJob commands, when the Rust binary runs each
subcommand against the synthetic seed, then its row effects match the corresponding Python
script run on the same seed (and the job records one structured log line)."*

This is the same-seed artifact. For every command and mode, the Python reference script and the
Rust subcommand each ran against their **own database reset from the same seeded template**, and
we compared:

1. the process exit code;
2. the one JSON line on stdout (every key and value except the run clock: `now`, `cutoff`, `at`);
3. the affected rows (`license` and `token`, all predicate-relevant columns), with a timestamp
   normalized to `<set-by-run>` only when it falls inside that run's wall-clock window;
4. every HTTP request the billing stub received (method, path, query, body, `X-Tenant-Id`).

**Pre-fix result: 7 of 8 command×mode pairs were EQUAL. `reconcile-metering` armed DIFFERED on one
column: Python's metering stamp also bumped `license.updated_at`; the Rust stamp did not.** That
mismatch was fixed in `19c0cee`, and the pair was re-run against the fixed binary: **all 8 pairs
are now EQUAL** — see
[Re-run after the `updated_at` fix](#re-run-after-the-updated_at-fix). The sections below keep the
pre-fix record; [The mismatch](#the-mismatch) documents the original divergence.

Reproduction inputs live beside this file: [`parity/seed.sql`](parity/seed.sql),
[`parity/billing-stub.py`](parity/billing-stub.py), [`parity/dump.sql`](parity/dump.sql),
[`parity/compare.py`](parity/compare.py).

---

## Method and environment

- **Database:** ephemeral `postgres:18` container `cron-parity-pg`, host port **25432** (8000 is
  never used). Two databases, `parity_py` and `parity_rs`, both created from the same seeded
  template `parity_tpl` immediately before their run (`DROP DATABASE … WITH (FORCE)` +
  `CREATE DATABASE … TEMPLATE parity_tpl`), so every timestamp the run does not write is
  byte-identical between the two.
- **Schema:** the crate's embedded baseline, applied with the crate's own binary:
  ```sh
  rbuild -- --config 'env.DATABASE_URL="postgres://parity:parity@127.0.0.1:25432/parity_py"' \
      run -p conveyor-licenses --locked -- migrate
  # -> migrate OK: 1 migration(s) embedded
  ```
  `parity_rs` was cloned from `parity_py` with `pg_dump --schema-only` after that migration.
- **Python side:** `/Users/mini3/Movies/licenses/.venv/bin/python scripts/<name>.py`, run from the
  repository root. The connection string is
  `postgresql+psycopg://parity:parity@127.0.0.1:25432/parity_py?sslmode=disable` (the throwaway
  container has no TLS). `ENVIRONMENT=production`, `KEYCLOAK_URL=http://127.0.0.1:9` with no
  `SERVICE_CLIENT_ID`/`SERVICE_CLIENT_SECRET`: the reference's in-process service-token provider
  is then unconfigured, so no `Authorization` header is sent — exactly the Rust client's behaviour
  with no service credentials.
- **Rust side:** the binary is built and executed through `rbuild` (never bare cargo). rbuild runs
  on **m1**, so two SSH reverse tunnels carried the ephemeral infrastructure to it:
  `-R 25432:127.0.0.1:25432` (Postgres) and `-R 8347:127.0.0.1:8347` (billing stub).
  `RBUILD_SLOTS=1` pinned the run to the warm slot built by `migrate`. Command template:
  ```sh
  RBUILD_SLOTS=1 rbuild -- \
    --config 'env.DATABASE_URL="postgres://parity:parity@127.0.0.1:25432/parity_rs"' \
    --config 'env.METERING_SERVICE_URL="http://127.0.0.1:8347"' \
    --config 'env.<KNOB>="<true|false>"' \
    --config 'env.<ACTOR>="parity-cron"' \
    --config 'env.REAP_GRACE_DAYS="30"' \
    --config 'env.RECONCILE_REVALIDATE="false"' \
    run -p conveyor-licenses --locked -- <subcommand>
  ```
- **Billing stub:** `parity/billing-stub.py 8347 <call-log.jsonl>`. It serves
  `GET /api/budget/list/{tenant}`, `GET /api/quota/check`, `POST /api/budget`,
  `POST /api/quota/config`, `DELETE /api/budget/{id}` and `DELETE /api/quota/config/{licence}`,
  restarted before every run with a fresh log. Initial facts: active `license_sync` budgets for
  L3 (`500000`), L11 (`600000`) and L5 (`500000`); quota rows for L3 (`1000`), L11 (`900`, a
  deliberate drift) and L5 (`1000`). All other licences are absent in billing.
- **No production identifiers:** every uuid, name, realm, subject, token hash and budget id in the
  seed and stub is invented. Nothing touched the live cluster or the live database.

### Seed shape (`parity/seed.sql`)

One app (`parity-app`), one plan (`quota_monthly=1000`, `token_budget_monthly=500000`), two
tenants (`parity-alpha` with no budget currency, `parity-money` with `USD`), one subject per
ACTIVE licence (the partial unique index `uq_license_active_tenant_app_subject` forbids sharing).

| Licence | Tenant | Status | `expires_at` | `created_at` | Shape under test |
|---|---|---|---|---|---|
| L1 `…0001` | alpha | active | now−10d | now−40d | stale `active` (expire target) |
| L2 `…0002` | alpha | active | now+30d | now−20d | live, absent in billing |
| L3 `…0003` | alpha | active | NULL | now−19d | perpetual, covered in billing |
| L4 `…0004` | alpha | expired | now−5d | now−50d | inside the 30-day grace |
| L5 `…0005` | money | expired | now−60d | now−90d | reapable; billing mirrors present |
| L6 `…0006` | alpha | revoked | now−200d | now−300d | already revoked |
| L7 `…0007` | alpha | revoked | now−20d | now−60d | soft-deleted (`deleted_at` set) |
| L8 `…0008` | alpha | expired | NULL | now−10d | `expires_at IS NULL` leg |
| L9 `…0009` | money | active | now+60d | now−5d | already stamped |
| L10 `…0010` | money | active | now+60d | now−4d | money tenant (USD) |
| L11 `…0011` | alpha | active | now+90d | now−3d | budget drift (`700000` vs billing `600000`) |
| L12 `…0012` | alpha | active | now+45d | now−2d | already stamped |

Tokens: K1/K2 expired on L2; K3 live on L3; K4 already revoked on L3; K5 perpetual on L3;
K6 expired one hour ago on L1. Predicate sanity after seeding:
`stale=1`, `reapable=1`, `expired_unrevoked_tokens=3`, `live_budget_owing=6`.

Every run below started from a fresh clone of that template. Row snapshots cover the columns
`license(id,status,revoked_at,updated_at,expires_at,deleted_at,metering_verified_at,
token_budget_override,quota_override)` and `token(id,license_id,revoked,revoked_at,expires_at,
last_used_at)`.

---

## 1. `expire-stale` — `scripts/expire_stale_licences.py`

```sh
# Python, dry then armed
env ENVIRONMENT=production \
    DATABASE_URL="postgresql+psycopg://parity:parity@127.0.0.1:25432/parity_py?sslmode=disable" \
    METERING_SERVICE_URL="http://127.0.0.1:8347" CORS_ALLOWED_ORIGINS="http://localhost:3000" \
    KEYCLOAK_URL="http://127.0.0.1:9" LOG_LEVEL=ERROR \
    EXPIRE_ACTOR=parity-cron EXPIRE_DRY_RUN=true|false \
    .venv/bin/python scripts/expire_stale_licences.py

# Rust, dry then armed
RBUILD_SLOTS=1 rbuild -- \
    --config 'env.DATABASE_URL="postgres://parity:parity@127.0.0.1:25432/parity_rs"' \
    --config 'env.EXPIRE_DRY_RUN="true|false"' --config 'env.EXPIRE_ACTOR="parity-cron"' \
    run -p conveyor-licenses --locked -- expire-stale
```

| Mode | Exit Python | Exit Rust | JSON line | Row effects |
|---|---|---|---|---|
| dry | 1 | 1 | identical except `now` | none written; `stale_before=stale_after=1`, `selected=[L1]`, `transitioned=0` |
| armed | 0 | 0 | identical except `now` | L1 `active→expired`, `updated_at`=run; all 11 other licences untouched; `stale_after=0` |

**Verdict: EQUAL (dry and armed).** Columns compared: all 9 licence columns + all 6 token columns
for 12 licences and 6 tokens; `row_diff={}` in both modes. Tokens unchanged.

```json
// dry — Python
{"actor": "parity-cron", "dry_run": true, "event": "expire_stale_licenses", "exit_code": 1, "limit": 500, "now": "2026-11-14T22:32:06.698393+00:00", "reason": "NOT CONVERGED (measure-only): 1 licence(s) are structurally active with a past expires_at and a dry run writes nothing, so all of them remain. This is the answer about the estate, not a list of failures. Would transition: ['aaaaaaaa-0000-4000-8000-000000000001']. Re-run with EXPIRE_DRY_RUN=false to retire them.", "selected": 1, "selected_license_ids": ["aaaaaaaa-0000-4000-8000-000000000001"], "stale_after": 1, "stale_before": 1, "transitioned": 0}
// dry — Rust
{"actor":"parity-cron","dry_run":true,"event":"expire_stale_licenses","exit_code":1,"limit":500,"now":"2026-11-14T22:32:13.706556+00:00","reason":"NOT CONVERGED (measure-only): 1 licence(s) are structurally active with a past expires_at and a dry run writes nothing, so all of them remain. This is the answer about the estate, not a list of failures. Would transition: ['aaaaaaaa-0000-4000-8000-000000000001']. Re-run with EXPIRE_DRY_RUN=false to retire them.","selected":1,"selected_license_ids":["aaaaaaaa-0000-4000-8000-000000000001"],"stale_after":1,"stale_before":1,"transitioned":0}
// armed — Python
{"actor": "parity-cron", "dry_run": false, "event": "expire_stale_licenses", "exit_code": 0, "limit": 500, "now": "2026-11-14T22:32:15.589542+00:00", "reason": "converged", "selected": 1, "selected_license_ids": ["aaaaaaaa-0000-4000-8000-000000000001"], "stale_after": 0, "stale_before": 1, "transitioned": 1}
// armed — Rust
{"actor":"parity-cron","dry_run":false,"event":"expire_stale_licenses","exit_code":0,"limit":500,"now":"2026-11-14T22:32:20.472407+00:00","reason":"converged","selected":1,"selected_license_ids":["aaaaaaaa-0000-4000-8000-000000000001"],"stale_after":0,"stale_before":1,"transitioned":1}
```

Billing calls: none in either implementation.

## 2. `reap-expired` — `scripts/reap_expired_licences.py`

```sh
# Python, dry then armed
env …as above… REAP_ACTOR=parity-cron REAP_DRY_RUN=true|false REAP_GRACE_DAYS=30 \
    .venv/bin/python scripts/reap_expired_licences.py

# Rust, dry then armed
RBUILD_SLOTS=1 rbuild -- …as above… \
    --config 'env.REAP_DRY_RUN="true|false"' --config 'env.REAP_ACTOR="parity-cron"' \
    --config 'env.REAP_GRACE_DAYS="30"' \
    run -p conveyor-licenses --locked -- reap-expired
```

| Mode | Exit Python | Exit Rust | JSON line | Row effects |
|---|---|---|---|---|
| dry | 1 | 1 | identical except `now`/`cutoff` | none written; `reapable_before=reapable_after=1`, `selected=[L5]`, `archived=0`, `teardown_mode="dry_run"`, `teardown_would_call=[{L5, money tenant}]` |
| armed | 0 | 0 | identical except `now`/`cutoff` | L5 `expired→revoked`, `revoked_at`=run, `updated_at`=run; `archived=1`, `teardown_mode="performed"`, `teardown_attempted=1`, no failures |

**Verdict: EQUAL (dry and armed).** Columns compared: all licence and token columns;
`row_diff={}`. L4 (inside the grace window), L6, L7 and L8 (`expires_at IS NULL`) all preserved.

```json
// dry — Python
{"actor": "parity-cron", "archived": 0, "cutoff": "2026-10-15T22:32:51.837346+00:00", "dry_run": true, "event": "reap_expired_licenses", "exit_code": 1, "grace_days": 30, "limit": 500, "now": "2026-11-14T22:32:51.837346+00:00", "reapable_after": 1, "reapable_before": 1, "reason": "NOT CONVERGED (measure-only): 1 licence(s) are status='expired' with an expiry older than the 30-day grace window, and a dry run writes nothing, so all of them remain — each one latching nonagentic_license_expired_rows{kind=\"license\",revoked=\"false\"} and pinning LicenseAlreadyExpired. This is the answer about the estate, not a list of failures. Would archive: ['aaaaaaaa-0000-4000-8000-000000000005']. Re-run with REAP_DRY_RUN=false to archive them.", "selected": 1, "selected_license_ids": ["aaaaaaaa-0000-4000-8000-000000000005"], "teardown_attempted": 0, "teardown_budget_failed": [], "teardown_mode": "dry_run", "teardown_quota_failed": [], "teardown_unresolved_tenant": [], "teardown_would_call": [{"license_id": "aaaaaaaa-0000-4000-8000-000000000005", "tenant_id": "11111111-1111-4111-8111-222222222222"}]}
// dry — Rust
{"actor":"parity-cron","archived":0,"cutoff":"2026-10-15T22:32:56.791507+00:00","dry_run":true,"event":"reap_expired_licenses","exit_code":1,"grace_days":30,"limit":500,"now":"2026-11-14T22:32:56.791507+00:00","reapable_after":1,"reapable_before":1,"reason":"NOT CONVERGED (measure-only): 1 licence(s) are status='expired' with an expiry older than the 30-day grace window, and a dry run writes nothing, so all of them remain — each one latching nonagentic_license_expired_rows{kind=\"license\",revoked=\"false\"} and pinning LicenseAlreadyExpired. This is the answer about the estate, not a list of failures. Would archive: ['aaaaaaaa-0000-4000-8000-000000000005']. Re-run with REAP_DRY_RUN=false to archive them.","selected":1,"selected_license_ids":["aaaaaaaa-0000-4000-8000-000000000005"],"teardown_attempted":0,"teardown_budget_failed":[],"teardown_mode":"dry_run","teardown_quota_failed":[],"teardown_unresolved_tenant":[],"teardown_would_call":[{"license_id":"aaaaaaaa-0000-4000-8000-000000000005","tenant_id":"11111111-1111-4111-8111-222222222222"}]}
// armed — Python
{"actor": "parity-cron", "archived": 1, "cutoff": "2026-10-15T22:32:58.793205+00:00", "dry_run": false, "event": "reap_expired_licenses", "exit_code": 0, "grace_days": 30, "limit": 500, "now": "2026-11-14T22:32:58.793205+00:00", "reapable_after": 0, "reapable_before": 1, "reason": "converged", "selected": 1, "selected_license_ids": ["aaaaaaaa-0000-4000-8000-000000000005"], "teardown_attempted": 1, "teardown_budget_failed": [], "teardown_mode": "performed", "teardown_quota_failed": [], "teardown_unresolved_tenant": []}
// armed — Rust
{"actor":"parity-cron","archived":1,"cutoff":"2026-10-15T22:33:05.743040+00:00","dry_run":false,"event":"reap_expired_licenses","exit_code":0,"grace_days":30,"limit":500,"now":"2026-11-14T22:33:05.743040+00:00","reapable_after":0,"reapable_before":1,"reason":"converged","selected":1,"selected_license_ids":["aaaaaaaa-0000-4000-8000-000000000005"],"teardown_attempted":1,"teardown_budget_failed":[],"teardown_mode":"performed","teardown_quota_failed":[],"teardown_unresolved_tenant":[]}
```

Armed billing teardown — identical request sets (sorted; both issue the quota delete and the
budget-list read concurrently):

| Method | Path | `X-Tenant-Id` |
|---|---|---|
| GET | `/api/budget/list/11111111-1111-4111-8111-222222222222` | `…222222` |
| DELETE | `/api/budget/cccccccc-0000-4000-8000-000000000005` | `…222222` |
| DELETE | `/api/quota/config/aaaaaaaa-0000-4000-8000-000000000005` | `…222222` |

Dry mode made **zero** HTTP calls in both implementations.

## 3. `revoke-expired-tokens` — `scripts/revoke_expired_tokens.py`

```sh
# Python, dry then armed
env …as above… REVOKE_ACTOR=parity-cron REVOKE_DRY_RUN=true|false \
    .venv/bin/python scripts/revoke_expired_tokens.py

# Rust, dry then armed
RBUILD_SLOTS=1 rbuild -- …as above… \
    --config 'env.REVOKE_DRY_RUN="true|false"' --config 'env.REVOKE_ACTOR="parity-cron"' \
    run -p conveyor-licenses --locked -- revoke-expired-tokens
```

| Mode | Exit Python | Exit Rust | JSON line | Row effects |
|---|---|---|---|---|
| dry | 1 | 1 | identical except `at` | none written; `expired_before=expired_after=3`, `acted=3`, `problems=["dry_run_found_expired_tokens: 3 still unrevoked"]` |
| armed | 0 | 0 | identical except `at` | K1, K2, K6 `revoked false→true`, `revoked_at`=run; `expired_after=0`, `converged=true`; K3, K4, K5 preserved |

**Verdict: EQUAL (dry and armed).** Columns compared: all 6 token columns for all 6 tokens, all 9
licence columns for all 12 licences; `row_diff={}`. K5 (`expires_at IS NULL`, perpetual) and K4
(already revoked, `revoked_at` preserved) were correctly untouched by both.

```json
// dry — Python
{"acted": 3, "actor": "parity-cron", "at": "2026-11-14T22:33:07.589095+00:00", "converged": false, "dry_run": true, "expired_after": 3, "expired_before": 3, "job": "revoke_expired_tokens", "limit": 500, "problems": ["dry_run_found_expired_tokens: 3 still unrevoked"], "status": "not_converged"}
// dry — Rust
{"acted":3,"actor":"parity-cron","at":"2026-11-14T22:33:12.769990+00:00","converged":false,"dry_run":true,"expired_after":3,"expired_before":3,"job":"revoke_expired_tokens","limit":500,"problems":["dry_run_found_expired_tokens: 3 still unrevoked"],"status":"not_converged"}
// armed — Python
{"acted": 3, "actor": "parity-cron", "at": "2026-11-14T22:33:14.555016+00:00", "converged": true, "dry_run": false, "expired_after": 0, "expired_before": 3, "job": "revoke_expired_tokens", "limit": 500, "problems": [], "status": "ok"}
// armed — Rust
{"acted":3,"actor":"parity-cron","at":"2026-11-14T22:33:20.437250+00:00","converged":true,"dry_run":false,"expired_after":0,"expired_before":3,"job":"revoke_expired_tokens","limit":500,"problems":[],"status":"ok"}
```

Billing calls: none in either implementation.

## 4. `reconcile-metering` — `scripts/reconcile_metering.py`

```sh
# Python, dry then armed
env …as above… RECONCILE_ACTOR=parity-cron RECONCILE_DRY_RUN=true|false \
    RECONCILE_REVALIDATE=false .venv/bin/python scripts/reconcile_metering.py

# Rust, dry then armed
RBUILD_SLOTS=1 rbuild -- …as above… \
    --config 'env.RECONCILE_DRY_RUN="true|false"' --config 'env.RECONCILE_ACTOR="parity-cron"' \
    --config 'env.RECONCILE_REVALIDATE="false"' \
    run -p conveyor-licenses --locked -- reconcile-metering
```

Selection: L2, L3, L10, L11 (`scanned=4`; L1 is past expiry, L9/L12 are already stamped).
`live_budget_owing_licenses=6` in both.

| Mode | Exit Python | Exit Rust | JSON line | Row effects |
|---|---|---|---|---|
| dry | 1 | 1 | identical | no row written (a dry run stamps nothing). `uncovered_budget=[L2]`, `uncovered_quota=[L2,L10]`, `budget_drift=[L11]`, `quota_drift=[L11]`, `budgets_skipped_money_tenant=1`, `quotas_already_configured=1`, `already_covered=1`, `repaired=0` |
| armed | 0 | 0 | identical | `metering_verified_at`=run on L2, L3, L10, L11; `repaired=4`, `budgets_created=1`, `budgets_updated=1`, `quotas_pushed=2`, `already_covered=1`, `budgets_skipped_money_tenant=1`, `quota_drift=[L11]` — **but `updated_at` differs: see below** |

**Verdict: dry EQUAL. armed DIFFERS on `license.updated_at` for L2, L3, L10, L11.** Every other
compared column, the whole JSON payload, both exit codes and all nine billing requests are equal.

```json
// dry — Python
{"actor": "parity-cron", "already_covered": 1, "budget_drift_license_ids": ["aaaaaaaa-0000-4000-8000-000000000011"], "budgets_created": 0, "budgets_skipped_money_tenant": 1, "budgets_skipped_sentinel": 0, "budgets_updated": 0, "dry_run": true, "event": "metering_reconcile", "exit_code": 1, "failed": 0, "failed_license_ids": [], "force_plan_quota": false, "license_id": null, "limit": 500, "live_budget_owing_licenses": 6, "quota_drift_license_ids": ["aaaaaaaa-0000-4000-8000-000000000011"], "quotas_already_configured": 1, "quotas_overwritten": 0, "quotas_pushed": 0, "reason": "NOT CONVERGED (measure-only): 2 of 4 scanned licence(s) have no request_quota_config row in billing: ['aaaaaaaa-0000-4000-8000-000000000002', 'aaaaaaaa-0000-4000-8000-000000000010']. Re-run without RECONCILE_DRY_RUN to create them.", "repaired": 0, "repaired_license_ids": [], "revalidate": false, "scanned": 4, "uncovered_budget_license_ids": ["aaaaaaaa-0000-4000-8000-000000000002"], "uncovered_quota_license_ids": ["aaaaaaaa-0000-4000-8000-000000000002", "aaaaaaaa-0000-4000-8000-000000000010"], "unknown_budget_license_ids": [], "unknown_quota_license_ids": [], "unknown_quota_reasons": {}}
// dry — Rust
{"actor":"parity-cron","already_covered":1,"budget_drift_license_ids":["aaaaaaaa-0000-4000-8000-000000000011"],"budgets_created":0,"budgets_skipped_money_tenant":1,"budgets_skipped_sentinel":0,"budgets_updated":0,"dry_run":true,"event":"metering_reconcile","exit_code":1,"failed":0,"failed_license_ids":[],"force_plan_quota":false,"license_id":null,"limit":500,"live_budget_owing_licenses":6,"quota_drift_license_ids":["aaaaaaaa-0000-4000-8000-000000000011"],"quotas_already_configured":1,"quotas_overwritten":0,"quotas_pushed":0,"reason":"NOT CONVERGED (measure-only): 2 of 4 scanned licence(s) have no request_quota_config row in billing: ['aaaaaaaa-0000-4000-8000-000000000002', 'aaaaaaaa-0000-4000-8000-000000000010']. Re-run without RECONCILE_DRY_RUN to create them.","repaired":0,"repaired_license_ids":[],"revalidate":false,"scanned":4,"uncovered_budget_license_ids":["aaaaaaaa-0000-4000-8000-000000000002"],"uncovered_quota_license_ids":["aaaaaaaa-0000-4000-8000-000000000002","aaaaaaaa-0000-4000-8000-000000000010"],"unknown_budget_license_ids":[],"unknown_quota_license_ids":[],"unknown_quota_reasons":{}}
// armed — Python
{"actor": "parity-cron", "already_covered": 1, "budget_drift_license_ids": ["aaaaaaaa-0000-4000-8000-000000000011"], "budgets_created": 1, "budgets_skipped_money_tenant": 1, "budgets_skipped_sentinel": 0, "budgets_updated": 1, "dry_run": false, "event": "metering_reconcile", "exit_code": 0, "failed": 0, "failed_license_ids": [], "force_plan_quota": false, "license_id": null, "limit": 500, "live_budget_owing_licenses": 6, "quota_drift_license_ids": ["aaaaaaaa-0000-4000-8000-000000000011"], "quotas_already_configured": 1, "quotas_overwritten": 0, "quotas_pushed": 2, "reason": "converged", "repaired": 4, "repaired_license_ids": ["aaaaaaaa-0000-4000-8000-000000000002", "aaaaaaaa-0000-4000-8000-000000000003", "aaaaaaaa-0000-4000-8000-000000000010", "aaaaaaaa-0000-4000-8000-000000000011"], "revalidate": false, "scanned": 4, "uncovered_budget_license_ids": ["aaaaaaaa-0000-4000-8000-000000000002"], "uncovered_quota_license_ids": ["aaaaaaaa-0000-4000-8000-000000000002", "aaaaaaaa-0000-4000-8000-000000000010"], "unknown_budget_license_ids": [], "unknown_quota_license_ids": [], "unknown_quota_reasons": {}}
// armed — Rust
{"actor":"parity-cron","already_covered":1,"budget_drift_license_ids":["aaaaaaaa-0000-4000-8000-000000000011"],"budgets_created":1,"budgets_skipped_money_tenant":1,"budgets_skipped_sentinel":0,"budgets_updated":1,"dry_run":false,"event":"metering_reconcile","exit_code":0,"failed":0,"failed_license_ids":[],"force_plan_quota":false,"license_id":null,"limit":500,"live_budget_owing_licenses":6,"quota_drift_license_ids":["aaaaaaaa-0000-4000-8000-000000000011"],"quotas_already_configured":1,"quotas_overwritten":0,"quotas_pushed":2,"reason":"converged","repaired":4,"repaired_license_ids":["aaaaaaaa-0000-4000-8000-000000000002","aaaaaaaa-0000-4000-8000-000000000003","aaaaaaaa-0000-4000-8000-000000000010","aaaaaaaa-0000-4000-8000-000000000011"],"revalidate":false,"scanned":4,"uncovered_budget_license_ids":["aaaaaaaa-0000-4000-8000-000000000002"],"uncovered_quota_license_ids":["aaaaaaaa-0000-4000-8000-000000000002","aaaaaaaa-0000-4000-8000-000000000010"],"unknown_budget_license_ids":[],"unknown_quota_license_ids":[],"unknown_quota_reasons":{}}
```

Billing calls, both implementations — identical 9-request sets:

| Mode | Calls |
|---|---|
| dry | `GET /api/budget/list/{alpha}`; `GET /api/quota/check` ×4 (L2, L3, L10, L11); no POST |
| armed | the same 5 reads, plus `POST /api/budget` L2 (`500000`) and L11 (`700000`); `POST /api/quota/config` L2 and L10 (`1000`, `app_id`/`app_name` = `parity-app`) |

The differing quota row (L11 holds `900`, effective `1000`) was deliberately **left alone** by
both and reported under `quota_drift_license_ids`; the money tenant L10 had its budget leg skipped
by both and only its quota pushed.

---

## The mismatch

`reconcile-metering` armed, one column: `license.updated_at` on the four stamped licences.

| Licence | Python after run | Rust after run |
|---|---|---|
| L2 `…0002` | `updated_at` = run clock | `updated_at` = seeded `2026-10-25T22:23:14Z` |
| L3 `…0003` | `updated_at` = run clock | `updated_at` = seeded `2026-10-26T22:23:14Z` |
| L10 `…0010` | `updated_at` = run clock | `updated_at` = seeded `2026-11-10T22:23:14Z` |
| L11 `…0011` | `updated_at` = run clock | `updated_at` = seeded `2026-11-11T22:23:14Z` |

`metering_verified_at`, `status`, every other compared column, the payload and all billing calls
are identical. The dry run does not stamp, so it has no divergence.

Cause, read from the two sources:

- Python (`/Users/mini3/Movies/licenses/main.py`): `_mark_metering_verified` calls
  `_stamp_license_metering(license_id, {License.metering_verified_at: now})`. The model declares
  `License.updated_at = Column(..., onupdate=utc_now, ...)` (`models.py`, `class License`), so the
  ORM UPDATE also writes `updated_at = now()`.
- Rust (`apps/licenses/src/commands/reconcile_metering.rs::stamp_verified`):
  `UPDATE license SET metering_verified_at = now() WHERE id = $1` — `updated_at` is untouched.
  Its docstring says "stamp `metering_verified_at` and NOTHING ELSE", which matches the Python
  function's docstring but not the Python row effect the ORM actually produces.

That is a genuine row-effect difference, not a clock artefact: the run-window normalizer only
rewrites timestamps that fall inside `[start, finish]`; the Rust values are the seeded values,
weeks old. It was reported by the pre-fix slice, not fixed there. Consumers that treat
`updated_at` as the licence's last-change marker saw a reconcile stamp as a change in Python and
no change in Rust.

**Status: fixed at `19c0cee`.** All four raw metering stamp/clear UPDATE sites now also set
`updated_at = now()` (`billing/metering.rs`, `commands/reconcile_metering.rs`,
`db/licence_admin.rs`, `db/catalogue_plans.rs`). The same-seed re-run in the next section shows
both implementations changing `updated_at` on the same four licences, with no remaining
difference.

---

## Re-run after the `updated_at` fix

The fix landed at `19c0ceea0bbcfe619e10ee1dcc957b86b9d7fcd1` (`fix: metering stamps advance
updated_at like the reference ORM (#45)`), branch `slice/45/cron-parity-rerun`. The pair that
differed — and, because the harness makes it cheap, all four commands — was re-run against a
binary built from that commit. The committed harness under [`parity/`](parity/) was used
unchanged; the method, seed, stub and comparator are those in §Method and environment.

### Candidate binary

```sh
RBUILD_SLOTS=1 rbuild -- build -p conveyor-licenses --locked
# Finished `dev` profile [unoptimized] target(s) in 0.29s
# (warm slot; the first, cold build compiled the dependency set in 1m25s)
```

Build host (m1): `~/build/slice-45-parity-rerun/target/debug/conveyor-licenses`.
macOS has no `sha256sum`; `shasum -a 256` reported:

```
71b8b2ddcd6f55e7ba99da0a258decf91c4f5bf90936a917e5044880a1464826
```

The hash was re-read after the whole matrix and was identical, so every run below executed that
binary.

### Environment and exact commands

Ephemeral `postgres:18` container `cron-parity-pg` on `127.0.0.1:25432` (8000 untouched); one
migration into `parity_py` with the candidate binary; one application of `parity/seed.sql` into
`parity_tpl`; every run reset from that template. The billing stub ran on 8347. m1 reached both
through reverse SSH tunnels:

```sh
ssh -N -R 25432:127.0.0.1:25432 -R 8347:127.0.0.1:8347 m1
```

```sh
# schema + seeded template
RBUILD_SLOTS=1 rbuild -- \
  --config 'env.DATABASE_URL="postgres://parity:parity@127.0.0.1:25432/parity_py"' \
  run -p conveyor-licenses --locked -- migrate        # migrate OK: 1 migration(s) embedded
psql -U parity -d parity_py -v ON_ERROR_STOP=1 -f parity/seed.sql
psql -U parity -d parity -c 'CREATE DATABASE parity_tpl TEMPLATE parity_py'

# per run: reset the implementation's database from the template, fresh stub, then
# the Python reference
env ENVIRONMENT=production \
    DATABASE_URL="postgresql+psycopg://parity:parity@127.0.0.1:25432/parity_py?sslmode=disable" \
    METERING_SERVICE_URL="http://127.0.0.1:8347" CORS_ALLOWED_ORIGINS="http://localhost:3000" \
    KEYCLOAK_URL="http://127.0.0.1:9" LOG_LEVEL=ERROR \
    <ACTOR>=parity-cron <DRY_RUN_KNOB>=true|false REAP_GRACE_DAYS=30 \
    RECONCILE_REVALIDATE=false \
    /Users/mini3/Movies/licenses/.venv/bin/python scripts/<script>.py

# Rust subcommand (RBUILD_SLOTS=1 pins the warm slot the build and migrate used)
RBUILD_SLOTS=1 rbuild -- \
    --config 'env.DATABASE_URL="postgres://parity:parity@127.0.0.1:25432/parity_rs"' \
    --config 'env.METERING_SERVICE_URL="http://127.0.0.1:8347"' \
    --config 'env.<DRY_RUN_KNOB>="true|false"' --config 'env.<ACTOR>="parity-cron"' \
    --config 'env.REAP_GRACE_DAYS="30"' --config 'env.RECONCILE_REVALIDATE="false"' \
    run -p conveyor-licenses --locked -- <subcommand>

# verdict
python3 parity/compare.py <command> <mode> \
    <py.json> <rs.json> <py.rc> <rs.rc> <py_rows.json> <rs_rows.json> \
    <py_calls.jsonl> <rs_calls.jsonl> <py_start> <py_finish> <rs_start> <rs_finish>
```

The per-command actor and dry-run knobs are exactly those in §1–§4. The matrix ran
2026-11-14 23:04–23:06 UTC.

### How `updated_at` is compared

The exact instant is deliberately **not** compared across the two runs:

- **Changed vs seed (primary evidence).** `SELECT id, updated_at FROM license ORDER BY id` was
  taken from `parity_tpl` before the matrix (the template every run is cloned from), and again
  from each run's own database afterwards. The comparison is per-licence "changed vs unchanged
  from the seeded value"; the two implementations' new instants are not compared with each other.
- **`compare.py` (secondary).** A timestamp is normalized to `<set-by-run>` only when it falls
  inside that implementation's own `[start, finish]` window; otherwise it must stay byte-identical
  to the seed. A pair is therefore EQUAL when **both changed the column** or **both preserved it**.
  Two clocks producing two different instants is not a difference.

### Result

All 8 command×mode pairs are EQUAL.

| Command | dry | armed |
|---|---|---|
| expire-stale | EQUAL | EQUAL |
| reap-expired | EQUAL | EQUAL |
| revoke-expired-tokens | EQUAL | EQUAL |
| reconcile-metering | EQUAL | **EQUAL** (pre-fix: DIFFERS) |

For every pair: both exit codes equal, `payload_diff={}`, `row_diff={}`,
`billing_calls_equal=true`.

`reconcile-metering` armed — both implementations changed `updated_at` from the seed on exactly
the four stamped licences, and on no other licence:

| Licence | Seed `updated_at` | Python after run | Rust after run |
|---|---|---|---|
| L2 `…0002` | `2026-10-25 23:04:28.031444+00` | `2026-11-14 23:04:40.266535+00` | `2026-11-14 23:04:49.280514+00` |
| L3 `…0003` | `2026-10-26 23:04:28.031444+00` | `2026-11-14 23:04:40.294491+00` | `2026-11-14 23:04:49.309234+00` |
| L10 `…0010` | `2026-11-10 23:04:28.031444+00` | `2026-11-14 23:04:40.316040+00` | `2026-11-14 23:04:49.345563+00` |
| L11 `…0011` | `2026-11-11 23:04:28.031444+00` | `2026-11-14 23:04:40.345495+00` | `2026-11-14 23:04:49.383061+00` |

The two instants differ because they are two separate runs (Python's process clock vs the
database's `now()`); that is expected, and it is exactly why the check is "changed vs seed", not
"equal instant". The dry run changed nothing on either side. The other armed commands changed only
their intended rows, identically in both implementations: `expire-stale` L1, `reap-expired` L5,
`revoke-expired-tokens` no licence `updated_at` (only the token `revoked_at` values compared by
the row dump).

### Remaining differences

None. `row_diff={}` for all four armed pairs, including `reconcile-metering`, which previously had
the `license.updated_at` divergence. The container and both tunnels were removed afterwards; mini3
and m1 ports 25432/8347 were verified free.

Scope note: these four cron subcommands exercise the reconcile stamp site
(`commands/reconcile_metering.rs`) and the expire/reap/revoke UPDATEs. The other three sites
touched by the same fix — `billing/metering.rs` (`metering_provisioned_at`) and the metering
clears in `db/licence_admin.rs` / `db/catalogue_plans.rs` — are not reached by these subcommands
and are outside this artifact's scope.
