---
slice: S17
title: Native benchmark harness
status: built 2026-11-09
depends_on: [S02, S03, S04, S09]
spec_sections: ["§3 D24", "§5.1", "§5.2", "§5.7", "§6", "§8 S17"]
adds_tables: [benchmark_suite, benchmark_instance, benchmark_run, benchmark_case]
adds_actions:
  [
    benchmark_suites.list,
    benchmark_runs.start,
    benchmark_runs.get,
    benchmark_runs.list,
    benchmark_runs.cancel,
    benchmark_runs.compare,
    benchmark_runs.promote,
    benchmark_cases.list,
  ]
adds_events: [benchmark_run.changed, benchmark_case.recorded]
adds_modules: [benchmark, sandbox]
parity_ids: PENDING-S0   # net-new: this slice ports no old surface, so S0 has no id for it
---

# S17 — Native benchmark harness

Programme project: S17 · issues #21

⚠️ **Process model 2026-11-09:** the subject is the **process** (`process_id`); the old `procedure_id` pin migrates in `0033` (a SOP is not a measured subject). The `conveyor` arm resolves the run's **process agent**.

## 1. Objective

`conveyor-bench` is absorbed: the conveyor becomes the whole measurement loop — seed the SWE-bench subset,
produce a patch per instance for an arm, run the graded test in a sandbox, parse the log with a ported
swebench parser, resolve it, store the verdict, and compare runs. At the end, a 50-instance run is an ordinary
tenant-scoped record with two controls that make its number publishable, and **no Python exists in the build or
the image**. The old repo is then frozen; its committed logs are this slice's oracle, not a dependency.

## 2. Starting state

- S04's registry, invocation/audit/idempotency, and the `job` runner exist; S09's `document` + object store are the
  log sink (`log_document_id`, §2.18).
- The banked artefacts are committed in `conveyor-bench/results/2026-09-17-full/artefacts/`: drive and grade
  run dirs, including per-instance eval logs and `results.jsonl` for the conveyor, baseline and control arms.
  The 17 runs that carry both a `results.jsonl` and an `eval-logs.tgz` are frozen into
  `crates/actions/tests/fixtures/benchmark/2026-09-17/` (557 rows; 555 graded + 2 harness-level k8s timeouts).
- No Kubernetes client exists anywhere in the workspace. The conveyor pod has no sandbox RBAC.

## 3. Scope

**IN.** Four tables (§2.18); the hashed seed artifact and its seeder; the pure grader (verdict ladder + the 12
repo parsers) and the exact McNemar; the k8s Job sandbox adapter; the `gold`, `empty` and `baseline` arms; the
single run driver and its reaper; the eight actions, two scopes and two events; screens **13** (benchmarks) and
**14** (improvement, data contract only) **declared** for S12.

**OUT.** The `conveyor` arm — it measures a capability-agent/procedure version and belongs to **S18**, which
waits for S13 and S15; this slice ships its columns and refuses it (`409 arm_not_ready`). Also out: hosting a
model (the baseline calls the router), any Hugging Face call at run time, importing the old harness's CLI, a
second service, and any Python in the image or the build.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `conveyor-bench/src/conveyor_bench/grader.py` (`47-241`) | `runner_script`, `job_name`, `build_manifests`, `grade_log`'s ladder | the ladder **verbatim**, the runner script verbatim, the 1 MiB ConfigMap cap | `make_test_spec` at run time — the eval script is generated at seed time by the Rust port |
| `conveyor-bench/src/conveyor_bench/baseline.py` | `SYS_DEFAULT`, the edit contract, `parse_reply`, `apply_reply`, the difflib writer | the prompt byte-for-byte and the refusal rules (zero/multiple anchors) | any Python implementation, `urllib` |
| `conveyor-bench/src/conveyor_bench/tasks.py`, `data/` | dataset pin, image lock, subset ids | the identities and the lock digests | the HF download at run time |
| `conveyor-bench/scripts/report-arms.py` (`46-190`) | arm derivation, `mcnemar`, the refusal rule | the rule and the two test vectors | the script itself |
| `conveyor-bench/results/2026-09-17-full/artefacts/*` | `results.jsonl`, `eval-logs.tgz`, `summary.json` | the parity fixtures | the published prose |
| `crates/actions/src/integrations/ingest.rs` | a transport with a typed failure ladder | fail-closed config, 502/422 mapping | its URL/token shape |

## 5. Deliverables

`migrations/0025_s17_benchmark.sql` (DDL exactly as `02-DATA-ARCHITECTURE §2.18`; update BOTH migration-ledger
lists — `crates/store/src/migrate.rs` and `crates/store/tests/s07_schema.rs`), `seed/benchmark/verified50-v1.json`
+ `seed/benchmark/images-verified50-v1.lock.json`, `tools/seed-benchmark/` (standalone crate: parquet reader +
artifact writer), `crates/domain/src/benchmark/{mod,arm,grade,parsers,mcnemar,specgen}.rs`,
`crates/domain/tests/{benchmark_specgen.rs,fixtures/benchmark/golden-eval-scripts.json}`,
`crates/store/src/seed_benchmark.rs`, `crates/store/src/benchmark.rs`,
`crates/actions/src/benchmark/{mod,runs,cases}.rs`,
`apps/server/src/sandbox.rs`, `apps/server/src/worker.rs` (the driver + reaper). Tests:
`crates/actions/tests/benchmark_parity.rs` (+ its frozen fixtures), `crates/api/tests/benchmark_rls.rs`,
`apps/server/tests/benchmark_jobs.rs`, `apps/server/tests/sandbox_live.rs` (`#[ignore]`, needs the cluster). Plus
`spec.rs` (two scopes), `crates/actions-derive/src/lib.rs` (the wire table), `served-spec.lock.json`, and the S12
screen declaration.

| action | input | output | scope |
|---|---|---|---|
| `benchmark_suites.list` | `{page}` | `Page<SuiteView>` (hashes, versions, baseline) | `benchmarks:read` |
| `benchmark_runs.start` | `{suite_id, arm, subject_kind?, capability_agent_id?, procedure_id?, label, concurrency?, only?, with_failing_test?}` | `RunView` (`queued`) | `benchmarks:write` |
| `benchmark_runs.get` | `{id}` | `RunView` | `benchmarks:read` |
| `benchmark_runs.list` | `{suite_id?, arm?, status?, page}` | `Page<RunView>` | `benchmarks:read` |
| `benchmark_runs.cancel` | `{id}` | `RunView` | `benchmarks:write` |
| `benchmark_runs.compare` | `{a_run_id, b_run_id}` | `{comparable, reason?, both, neither, a_only, b_only, discordant, p_value, missing_in_one?, by_repo?}` | `benchmarks:read` |
| `benchmark_runs.promote` | `{id}` | `SuiteView` (baseline set) | `benchmarks:write` |
| `benchmark_cases.list` | `{run_id, verdict?, page}` | `Page<CaseView>` | `benchmarks:read` |

Screens declared for S12: **#13 Benchmarks** (`benchmark_runs.*`, `benchmark_cases.list`, SSE
`benchmark_run:{id}`, `benchmarks.spec.ts`) and **#14 Improvement** (`benchmark_runs.trend` arrives with S18;
`improvement.spec.ts`). Jobs: `benchmark.run.poll`, `benchmark.reap`. Scopes: `benchmarks:read`,
`benchmarks:write`. Events: `benchmark_run.changed`, `benchmark_case.recorded`.

## 6. Build order

1. **Seed artifact** — `tools/seed-benchmark` (a standalone crate, not a workspace member) reads the pinned parquet
   and generates `seed/benchmark/verified50-v1.json` through the Rust port of swebench 5.0.2's `make_test_spec`
   (`crates/domain/src/benchmark/specgen.rs`), with `seed_sha256`; commit it. Assert its 50 instance ids equal
   `data/verified50-v1.json`. The harness runs once in development as the **golden oracle** (50 scripts,
   `crates/domain/tests/fixtures/benchmark/golden-eval-scripts.json`); no Python is in the build or the image.
2. Migration + `crates/store/src/migrate.rs` touch (the macro embeds at compile time). `cargo run -p
   conveyor-server -- migrate`; verify the ledger, not the log line.
3. `domain::benchmark::arm` — the `report-arms.py` derivation (`conveyor-*`, `control-*`, direct; `+failing-test`
   ⇒ `files+test`/`bug_fix`), and the `MixedArmLabels` refusal. Tests with the five banked labels.
4. `domain::benchmark::mcnemar` — exact two-sided, both vectors (9/p=0.180 counts-only, 8/p=0.289); the refusal.
5. `domain::benchmark::parsers` — extract the test-output section, then one parser per repo (12), producing a
   `test → status` map; resolve `FAIL_TO_PASS`/`PASS_TO_PASS`.
6. `domain::benchmark::grade` — `grade_log`'s ladder, including the infra classifier and the end-marker guard.
7. **Parity test** `crates/actions/tests/benchmark_parity.rs`, over fixtures copied to
   `crates/actions/tests/fixtures/benchmark/2026-09-17/` (the grade dirs' `results.jsonl` + `eval-logs.tgz`, the
   drive dirs' `predictions.jsonl`/`provenance.jsonl`/`summary.json`, and the seed) — every banked grade run's
   logs reproduce its `results.jsonl` row for row. This is the slice's central proof; do it before any sandbox
   code.
8. `store::benchmark` — suite/instance seed (idempotent by `seed_sha256`; conflict ⇒ 409), run create/get/list/
   cancel, case finalise, compare query (paired on the graded intersection), baseline promote, reap. RLS tests.
9. `actions::benchmark` — the eight handlers; scopes at the five code sites; registry `EXPECTED` and the api-lib
   count; a `parity_http_mcp` sample for each action.
10. `apps/server/src/sandbox.rs` — `kube` client, ConfigMap + Job, pod log, delete; deterministic names; fail-closed
    config. Unit-test the manifest builder; one `#[ignore]` live proof against the cluster.
11. Worker: `benchmark.run.poll` (start ≤ concurrency, poll, grade, finalise, re-enqueue per tick with
    `max_attempts=1`) and `benchmark.reap` in `DEFAULT_SCHEDULES` + `prune.rs`'s exact list.
12. `--write-lock`; schema/action/op counts in the index; `cargo test --workspace`; clippy; `/tmp/s17-gate.sh`.

## 7. Gate

```bash
set -euo pipefail
cd $HOME/Movies/conveyor
step() { printf '\n=== %s ===\n' "$1"; }

# Rust runs on m1 (CONTRACT §16): `~/.local/bin/cargo` refuses inside a Cargo project on mini3.
step "fmt + clippy"
rbuild -- fmt --all -- --check
rbuild -- clippy --workspace --all-targets -- -D warnings

step "the served spec matches the lock"
rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock >/dev/null

step "the banked logs reproduce the banked verdicts (the oracle)"
rbuild -- test -p conveyor-actions --test benchmark_parity -- --nocapture
# 555/555 graded rows reproduce their banked verdicts; the gold controls are 50/50, the empty 0/50

step "golden eval scripts (the harness oracle, 50/50)"
rbuild -- test -p conveyor-domain --test benchmark_specgen

step "arm derivation and McNemar"
rbuild -- test -p conveyor-domain benchmark

step "the workspace suite"
rbuild -- test --workspace --locked

step "the seed tool (standalone workspace; run from the repo root)"
rbuild -- clippy --manifest-path tools/seed-benchmark/Cargo.toml --all-targets -- -D warnings
rbuild -- fmt --manifest-path tools/seed-benchmark/Cargo.toml --check
rbuild -- run -q --manifest-path tools/seed-benchmark/Cargo.toml -- \
  --parquet <the pinned parquet> --images seed/benchmark/images-verified50-v1.lock.json \
  --ids <verified50-v1.json> --out .seed-gate/verified50-v1.json
cmp seed/benchmark/verified50-v1.json .seed-gate/verified50-v1.json

# ⚠️ This probe observes mini3's compose `conveyor_dev`, where the operator applies `migrate`.
# The `test` commands above run against m1's throwaway cluster, a DIFFERENT database — so the
# probe names its database rather than leaving the reader to assume it.
step "RLS on every new table (compose conveyor_dev)"
export DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5439/conveyor_dev'
for t in benchmark_suite benchmark_instance benchmark_run benchmark_case; do
  test "$(psql "$DATABASE_URL" -tAc "SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE relname='$t'")" = "t"
done

step "two tenants cannot see each other's run"
rbuild -- test -p conveyor-api --test benchmark_rls

step "the sandbox is fail-closed"
rbuild -- test -p conveyor-server --test benchmark_jobs

printf '\nGATE_GREEN\n'
```

A live sandbox proof is **not** in this block: it needs the grading node (§10). It is one `#[ignore]` test,
`rbuild -- test -p conveyor-server --test sandbox_live -- --ignored`, run once when the node exists.

## 8. Traps

1. **The parser is the score.** A regex that silently matches nothing turns a testbed fault into a model failure.
   The parity test must compare every instance, and an unparseable case is `infra_error`, never `unresolved`.
2. **A Job name is not free.** `swb-<run8>-<slug>` can exceed 63 chars and can collide on a re-run; the harness's
   `job_name` truncation is part of the port, and a 409 on create is a **reattach**.
3. **The ConfigMap is capped at 1 MiB by k8s**, and the three files share the cap. Refuse before create.
4. **The patch cap is 256 KiB in the domain and in the CHECK**; truncating a patch fabricates an `apply_failed`.
5. **Answers are not producer input.** `gold_patch`/`test_patch`/`FAIL_TO_PASS` are read only by the grader; a
   producer sees the problem statement and `oracle_files`. An action that returns them is the bug.
6. **A control miss is data, not a failure to hide.** Gold 50/50 and empty 0/50 are gates on the *grader*; a run
   that misses is recorded `control_ok=false` and its number is never shown as a score.
7. **The poll chain must survive a flaky runner.** A same-key re-enqueue is refused while the current job is
   running; use a per-tick dedupe key and `max_attempts=1`. The reaper keys on `last_polled_at`, not on case
   silence.
8. **No FK across tenant tables.** The `inference_call` rule (§2.17): a foreign key's RI check bypasses RLS. The
   two forward references are resolved in-tenant at write.

## 9. Decisions closed here

1. **The eval script is generated at seed time, in Rust.** swebench 5.0.2's `make_test_spec` takes the dataset's
   `eval_script` and only inserts the test-exit-code lines around the end marker (`utils.py:223-245`); that
   transform is ported byte for byte (`domain::benchmark::specgen`) and pinned by a golden test generated once by
   the harness. A new suite is a new hashed artifact. Python never runs in the build, the image or the product.
2. **The image lock and dataset revision are suite identity.** A re-seed with different hashes is a 409, not an
   update; a suite name cannot silently change what it measures.
3. **One writer per run.** `benchmark.run.poll` is the only writer of progress; there is no ingest action and no
   second driver.
4. **The baseline is the model control, not an arm of the product.** It is measured and displayed, never compared
   against a `conveyor` run whose oracle/prompt differ (§7's refusal).
5. **No benchmark action is in `AGENT_TOOLS`.** A model-driven principal cannot start a run or read an answer.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Which node runs the eval Jobs? | a designated node with a taint; the selector is deployment config, never a repointed `worker1` |
| 2 | May the baseline reach `raw.githubusercontent.com` from the pod? | yes if the namespace allows egress; otherwise fetch through a sandbox Job |
| 3 | Is the seed's one-time extraction acceptable as Python, though the product has none? | **RESOLVED 2026-11-09 (owner): no Python anywhere in the seed path.** The eval script is generated by the Rust port; the harness is a development-only golden oracle, and its output is a committed fixture. |
| 4 | Does a run pin a provider/model? | `model` is recorded from the router's response; a differing model is disclosed by `compare`, never refused |
| 5 | How long is a run's deadline? | the sum of per-case deadlines at the configured concurrency; the reaper fails a dead driver, not a slow run |
