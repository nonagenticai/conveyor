# S17 core — measured evidence (steps 1–7)

Session `s17-benchmark-core`, 2026-11-09. Docs were already landed by `movies-01`; this lane built the
pure measurement core and proved it against the frozen 2026-09-17 artefacts. Nothing here needs a cluster.

## What was built

| file | what |
|---|---|
| `crates/domain/src/benchmark/{mod,arm,mcnemar,specgen,parsers,grade}.rs` | arm identity, exact McNemar, the swebench 5.0.2 eval-script transform, 6 parser functions covering all 12 repos, the verdict ladder |
| `crates/domain/tests/benchmark_specgen.rs` + `tests/fixtures/benchmark/golden-eval-scripts.json` | 50 golden eval scripts produced once by `swebench==5.0.2` |
| `tools/seed-benchmark/` (standalone crate, not a workspace member) | reads the pinned parquet, writes the hashed seed artifact |
| `seed/benchmark/verified50-v1.json` + `seed/benchmark/images-verified50-v1.lock.json` | the seed and its image lock |
| `migrations/0025_s17_benchmark.sql` | the four tables of DATA-ARCH §2.18 + RLS |
| `crates/actions/tests/benchmark_parity.rs` + `tests/fixtures/benchmark/2026-09-17/` | the 17 frozen runs (557 rows) and the parity gate |
| ledger lists | `crates/store/src/migrate.rs`, `crates/store/tests/s07_schema.rs` (both end at 25) |

## The eval-script oracle (one-time, development only)

swebench 5.0.2's `make_test_spec` does **not** build the script from a spec table: it takes the dataset's
`eval_script`, strips `#!/bin/bash`/`set -uxo pipefail`, inserts

```
SWEBENCH_TEST_EXIT_CODE=$?
: '>>>>> End Test Output'
echo ">>>>> Test Exit Code: $SWEBENCH_TEST_EXIT_CODE"
```

and re-adds the header (`swebench/harness/utils.py:215-245`, `types.py:44-49`). The golden fixture was produced
with the old repo's own pinned environment, run once and never in the build or the image:

```bash
cd ~/Movies/conveyor-bench
UV_PROJECT_ENVIRONMENT=/tmp/s17-oracle-venv uv sync --frozen
/tmp/s17-oracle-venv/bin/python - <<'EOF'
import json, pyarrow.parquet as pq
from swebench.harness.utils import make_test_spec
ids = json.load(open('data/verified50-v1.json'))
rows = {r['instance_id']: r for r in pq.read_table('/tmp/s17-oracle/verified-78f471bf.parquet').to_pylist()
        if r['instance_id'] in set(ids)}
json.dump([{'instance_id': i, 'eval_script': make_test_spec(rows[i]).eval_script,
            'dataset_eval_script': rows[i]['eval_script'], 'log_parser': rows[i]['log_parser'], ...}
           for i in ids], open('/tmp/s17-oracle/golden-eval-scripts.json', 'w'))
EOF
```

Dataset pinned at `SWE-bench/SWE-bench_Verified` @ `78f471bf655a3137b2e8a75af1501690ec009ec3`
(parquet sha256 `030cfd7f2a704c4c0226e7f104c725a3b41230b1d3517f9c915ad7ea5be3fa25`, 6,304,616 bytes).

## Measured

| claim | measurement |
|---|---|
| Golden eval scripts | **50/50 byte-identical** (`cargo test -p conveyor-domain --test benchmark_specgen`) |
| Seed artifact | 50 instances, 760,354 bytes, `seed_sha256 27421012ad8bbd712228c8a0be14a02bf82093d8777cf88a1c72b7ec2118ea0a`, `image_lock_sha256 ed5d1fabc5b835012a4969a7e4df7a1bbd947860dbb3aa99e6ab0ca89e8cb8f6` |
| Seed determinism | regenerated to `/tmp` and `cmp`-identical to the committed file |
| Parity | **555/555 graded rows** across 17 banked runs, every row's `status`, `resolved`, four `tests_status` lists and `infra_failure` equal to `results.jsonl` |
| Ungraded rows | 2 rows (k8s API timeouts, `report` absent, empty log) — the sandbox driver's `infra_error`, outside the grader's set by construction |
| Gold controls | `091615504508` 50/50, `091709203869` 50/50 |
| Empty controls | `09161555116e` 0/50, `0917093013f5` 0/50 |
| Early gold runs | `09161231d54b` 0/2 and `091612332af6` 0/50 — historical pre-fix runs, **reproduced**, not controls |
| McNemar pair | `09180535e0cc` (conveyor-oracle-files+failing-test, 24) × `091719199e42` (qwen-oneshot-oracle-files+failing-test, 28) = both 22 · neither 20 · a-only 2 · b-only 6 · discordant 8 · **p = 0.289** |
| Refusal | `09171352d352` (oracle `files`) × `091719199e42` (`files+test`) = `comparable:false` |
| Unit vectors | `p = 0.180` (21/20/2/7 ⇒ 9 discordant) and `p = 0.289` (22/20/2/6 ⇒ 8) |
| Migration | `_sqlx_migrations` row 25 `s17 benchmark` success; the four tables `relrowsecurity AND relforcerowsecurity` = `t` (compose `conveyor_dev`, mini3) |

## Gate — run on m1 through `rbuild` (CONTRACT §16; mini3 refuses local cargo)

| step | result |
|---|---|
| `rbuild -- test --workspace --locked` | **615 passed · 0 failed · 3 ignored across 116 test binaries** (detached run, exit 0); the only later source change was improve-01's whitespace-only `jwks.rs` reformat, after which `-p conveyor-auth` was re-run green |
| `rbuild -- clippy --workspace --all-targets -- -D warnings` | clean |
| `rbuild -- test -p conveyor-domain --test benchmark_specgen` | 3/3 (golden 50/50) |
| `rbuild -- test -p conveyor-actions --test benchmark_parity -- --nocapture` | 2/2 (555/555 rows) |
| `rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock` | 210 ops, unchanged |
| seed tool on m1 (clippy, fmt, run + `cmp`) | clean; regenerated artifact byte-identical to the committed one |
| `rbuild -- fmt --all -- --check` | **green (exit 0, no diff)** after improve-01 reformatted `crates/auth/src/jwks.rs` (whitespace-only, its file); `-p conveyor-auth` re-ran green afterwards (37 tests) |
| RLS + ledger (local psql, compose `conveyor_dev`) | four tables `t`/`t`; migration 25 success |

⚠️ The `psql` probes observe mini3's compose database, **not** the m1 throwaway test cluster the `test`
commands run against; the slice's gate block now says so explicitly. `benchmark_rls` (steps 8–12) will make
the app-level assertion a Rust test.

## Mutation checks (a test must be able to fail)

| mutation | expected | observed |
|---|---|---|
| `parse_log_pytest`: key `parts[1]` → `parts[0]` (also hits matplotlib) | parity red | **red — 2 failures**, restored (cmp-identical) |
| McNemar tail `0..=k` → `0..k` | both p-vectors red | **red — 3 failures**, restored |
| arm comparability also requires the pipeline | conveyor-vs-qwen pair refused | **red — 2 failures**, restored |
| the `APPLY_PATCH_FAIL` early return disabled | apply-failed ladder test red | **red — 2 failures**, restored |
| eval-script insertion index `i + 2` → `i + 1` | golden 50/50 red | **red — 2 failures**, restored |

Every new test was shown able to fail before the gate was taken green.

## Not in this lane

Steps 8–12: `store::benchmark`, the eight actions/scopes/registry counts, `apps/server/src/sandbox.rs`,
`benchmark.run.poll`/`benchmark.reap`, `--write-lock`. The `conveyor` arm and screen 14 are S18; screens are S12.

## Steps 8–12, session 2 (2026-11-09) — store, sandbox, driver built; action wiring held by peers

Landed and measured (all commands via `rbuild` on m1):

| piece | proof |
|---|---|
| `store::benchmark` | `benchmark_rls` green: seed created → same `seed_sha256` a no-op → different hash `SeedConflict`; grader projection returns answers, producer projection does not; run counters derived from cases (2/2/1); a finalised case refuses a second write; an over-cap patch refuses before the CHECK; the paired map spans both runs; promote sets `baseline_run_id`; tenant B sees nothing and a forged `tenant_id` is `42501`; owner sees 0 on all four (FORCE), superuser sees 1/2/2/4 |
| `seed_benchmark` + `migrate --seed` | the artifact is embedded (same pattern as `seed/catalogue.json`) and the seed is idempotent per tenant |
| `apps/server/src/sandbox.rs` | fail-closed (`NotConfigured` without `SANDBOX_NAMESPACE`, `Credential` without a token); manifests carry `automountServiceAccountToken=false`, `enableServiceLinks=false`, the pull secret, the digest image, caps, the 1,000,000-byte ConfigMap cap and `POD_DEADLINE_S`; the node selector/toleration are env config (SPEC R12), never hardcoded; the runner script keeps swebench 5.0.2's four `GIT_APPLY_CMDS` in order (read from the pinned sdist, not memory) |
| `benchmark_worker::poll_with` | a stub Kubernetes API + the in-memory object store drive two ticks: tick 1 starts exactly 2 deterministic Jobs, tick 2 reads them Succeeded, grades both logs `unresolved`, stores 2 S09 documents, finishes the run with `control_ok = Some(false)` |
| clippy / fmt | `clippy -p conveyor-server -p conveyor-store --all-targets -D warnings` clean; fmt clean on every touched file |

Mutations (each restored afterwards): `automountServiceAccountToken=true` → manifest test red; gold
`control_ok = Some(true)` → driver test red; the seed-conflict comparison disabled → RLS test red. The
`job_name` test also caught a real collision — two UUIDv7 ids from the same millisecond share the leading
8 hex digits, so the name now uses 12 hex digits of SHA-256 over the full run id.

Deviations and gaps recorded:
- The sandbox client uses the workspace `reqwest` instead of `kube` + `k8s-openapi` (five REST calls; no new
  dependency tree in a lock three lanes share). The wire behaviour and the manifest contract are unchanged.
- ⛔ **The `baseline` arm's producer has no build step in S17 §6.** `baseline.py` (pinned system prompt,
  `raw.githubusercontent.com` source fetch, edit contract, difflib patch writer) is 569 lines and is not in the
  slice's deliverables; `benchmark_runs.start` refuses `baseline` with `409 arm_not_ready` until a producer port
  is planned. `gold` and `empty` — the two controls — are built.
- **Not landed at session end:** the 8 actions' registration (`actions/src/lib.rs`, `spec.rs` 2 scopes,
  `actions-derive`, `registry.rs` counts, `api/src/lib.rs`, `portfolio_roundtrip`, `served-spec.lock.json`), the two
  `worker.rs` dispatch arms, the `benchmark.reap` schedule row, and the full S17 gate. The module code exists in
  `crates/actions/src/benchmark/{mod,runs,cases}.rs` and is unregistered — the shared files are leased by
  `s19-eval-build-a` (mid-pass) then `inference-ledger-prompt-audit` (S16), which agreed to hand over with measured
  counts and the lock hash.

## Steps 8–12 integration (2026-11-09, session 2 continued — parallel with S16/S13 per the owner)

Landed after the shared-file window opened:

- **Actions registered.** `pub mod benchmark;` (actions crate), `BenchmarksRead`/`BenchmarksWrite`
  (ranks: read = member+, write = member+ like `procedures:write`), the 8 action declarations, the
  derive wire table, and the parity samples. Counts moved from S16's 216/57 to **224 actions
  (211 internal + 13 external) · 59 scopes · served-spec.lock 245 ops** (`--check-lock` exit 0).
  `registry.rs` and `api/src/lib.rs` assert 224; `portfolio_roundtrip` asserts 59. At the time of
  writing, S13's `workflows.*` registration was landing on top (its lane re-derives the totals).
- **Two dispatch arms** in `worker.rs` (`benchmark.run.poll`, `benchmark.reap`) and the
  `benchmark.reap` schedule row (`*/5 * * * *`, `DEFAULT_SCHEDULES` 15 → 16).
- **Parity samples**: one per benchmark action, with a fixture suite + two graded runs
  (`benchmark_fixture`) and a per-round queued run for `benchmark_runs.cancel`. `SANDBOX_NAMESPACE`
  is set in the test so `start` queues rather than answering 503.

Measured (m1/`rbuild`):

| check | result |
|---|---|
| `registry` | 8/8 (224 actions, (211,13)) |
| `parity_http_mcp` | 4/4 — all 224 actions, HTTP and MCP |
| `benchmark_parity` / `benchmark_specgen` | 2/2 (555/555) · 3/3 (golden 50/50) |
| `conveyor-domain --lib benchmark` | 34/34 |
| `benchmark_rls` | 1/1 |
| `benchmark_jobs` + `prune` | 8/8 + 13/13 |
| `clippy --workspace --all-targets -D warnings` | clean |
| seed tool clippy + fmt | clean; artifact regenerated from the pinned parquet and **byte-identical** (`seed_sha256 2742…ea0a`) |
| compose `conveyor_dev` RLS probe | `benchmark_suite/instance/run/case` all `t` (enabled + forced) |

⚠️ **Parallel-lane caveat, measured:** the workspace-wide `fmt --all -- --check` and
`cargo test --workspace --locked` were run while S19's runner lane and S13's workflow lane were
mid-pass; their in-flight files (`domain/tests/eval_*`, `store/tests/eval_*`, `store/src/workflow/**`,
`store/tests/workflow_publish.rs`) were the only fmt diffs, and S13's `workflows.*` registration was
still landing in `registry.rs`. The S17 targets above are green on their own; the workspace-wide
numbers must be re-taken once those lanes settle. Nothing in that red is an S17 file.

## Final gate record (2026-11-09 ~14:25)

Every S17 target is green: `registry` 8/8 (224 actions, (211,13)); `parity_http_mcp` 4/4 over all
224 actions on both transports; `benchmark_parity` 2/2 (**555/555** banked rows); `benchmark_specgen`
3/3 (golden **50/50**); `conveyor-domain --lib benchmark` 34/34; `benchmark_rls` 1/1; `benchmark_jobs`
8/8 (the stub-Kubernetes tick starts, grades, stores the log as an S09 document and finishes a gold
control `control_ok=false`); `prune` 13/13 (the new `benchmark.reap` row included); `clippy
--workspace --all-targets -D warnings` clean; `--check-lock` exit 0 at **245** ops; the seed artifact
regenerated from the pinned parquet is byte-identical; the compose `conveyor_dev` probe reports
`benchmark_suite/instance/run/case` all `t` (enabled + forced).

⛔ **The workspace-wide `cargo test --workspace --locked` could not be taken green — twice — because
of live peer edits, measured at the time, in neither case an S17 file:**
1. 14:12:56 — `crates/actions/src/workflow/{actions,mod,views}.rs` failed to compile against a
   snapshot taken before S13's derive-macro scope entry landed. S13 reports it fixed; a fresh
   `check --workspace` at ~14:16 was **clean**.
2. 14:17 — `crates/actions/src/eval/runs.rs:264` (S19-B's lane, mid-edit) fails `E0308`
   (`variant.as_str()` returns `String`, `find_run_tx` takes `&str`; fixed by its owner).
3. 14:20 — **compiled and ran**. The only failing target is `parity_http_mcp`; at that instant its
   assertion named 13 actions without samples: S13's 6 `workflows.*` and S19-B's 7
   `eval_runs.{start,get,list,cancel}` + `eval_results.{list,get,label}`. **Every one of S17's 8
   benchmark actions was present** (never in the missing list). Cargo stops at the first failing
   target, so the binaries after `parity_http_mcp` did not run in that pass (they are covered
   individually above).
4. 14:35 — **re-measured after S13 landed its six samples**: the missing list is now exactly S19-B's
   7 `eval_runs.*`/`eval_results.*`. S13's `workflows.*` samples are present and the lock is 258 ops
   (`--check-lock` 0). The one remaining workspace red is S19-B's fixture work — its own next session
   (an approved eval with a locked split, and a filled result slot). S17 contributes nothing to it.
The last lane to settle owns the final workspace gate and the last `--write-lock`. S17's own
targets above are green; this is the shared-tree cost, not an S17 defect.

**Lock, final state (14:29):** the live spec after the peer registrations is **237 actions**, and I regenerated
`served-spec.lock.json` from it — **258 operations**, `--check-lock` exit 0, with 8 benchmark, 6 `workflows.*`,
4 `eval_runs.*` and 3 `eval_results.*` operations present. The 245-op figure above is this lane's own
224-action state; 258 is what the tree now holds.
