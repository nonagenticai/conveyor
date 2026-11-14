# Rust development process: audit and improvement plan

Audit date: 2026-11-10. Conveyor checkout: `0ace3ee`, branch
`lane/train-2026-11-09`. This is a proposal; no hooks, workflows, repository
settings, or cluster resources were changed during the audit.

## Findings supported by this repository and GitHub

| Area | Observed state | Implication |
| --- | --- | --- |
| Local hooks | Installed `.git/hooks/pre-commit` and `pre-push` come from `gitops-production/scripts/hooks`. They dispatch Python/Node checks, with no Rust commands. The latest local logs show a secret scan followed by the unsupported-project skip. | Full Rust compilation in these hooks does not explain current local commit latency. Measure a slow commit before removing checks. |
| Workspace | Ten members; Rust 1.99.0 is pinned in the toolchain file, manifest, Dockerfile, and caller workflow. | Splitting a monolithic crate into a workspace is already done. |
| Debug configuration | Root `[profile.dev]` uses `debug = 1`; shared CI uses `CARGO_PROFILE_DEV_DEBUG=line-tables-only`; the custom test command changes it to `0`. | Debug reduction already exists. Clippy and tests have different effective profile settings. The root comment calls `debug = 1` line tables only, but those are distinct Cargo settings. |
| CI structure | `.github/workflows/ci.yml` calls shared `rust-service.yml@main`, which calls `rust-library.yml@main`. Docker build and Rust checks run alongside each other. Formatting, Clippy, tests, and audit are sequential inside the heavy check job. | Important changes belong in `gitops-production`, not only this repository. |
| Incremental compilation | Shared CI already sets `CARGO_INCREMENTAL=0`. | No new optimization to apply here. Keep local incremental compilation available. |
| Actions cache | The cache API returned no saved caches. Shared CI restores Cargo caches, but saves only on non-PR runs after successful preceding steps and a disk-headroom check. | Cache configuration does not establish that usable cache entries exist. |
| Compiler cache | Repository secret names for MinIO-backed sccache exist, added November 9. Recent jobs successfully executed the sccache installation step. | This optimization is already being rolled out. Cache hits, misses, errors, and end-to-end benefit remain unverified. |
| Heavy runners | GitOps `runner-large-helmrelease.yaml` configures `maxRunners: 2`, a runner CPU limit of 4, memory limit of 16 GiB, and work volume limit of 20 GiB. Current shared Rust CI defaults Cargo build jobs to 8. | More jobs or higher compiler concurrency may increase contention. Validate deployed settings and CPU throttling before tuning. |
| Integration targets | Store has 40 top-level integration files, actions 51, API 51, server 22: 164 separate executables across those four packages. | Repeated compilation/linking and fixture startup deserve measurement before further splitting production crates. |
| Database fixtures | Server/store and actions harnesses use a process-local `OnceCell` for Postgres and a process-local mutex for database setup. Each test gets a fresh database and migrations. `TEST_DATABASE_URL` can select an externally started test server. | A shared CI server is feasible, but setup must become safe across processes. |
| Git process | PRs target both the integration train and other lane branches; PR #5 targets main. Every PR triggers the same full reusable service workflow. | Stacked work can repeatedly validate overlapping code and compete for the same runner pool. |

The shared GitOps main SHA observed near the end of inspection was
`322bbdcf334131517662756ce431395407c93f76`. Older runs examined here referenced
`7609bb5509a411d9c60d8200a976b97925d89ba4`. Configuration is changing during this
incident, so compare resolved workflow revisions when evaluating improvements.

### Measured delays

[Run 42680838271](https://github.com/nonagenticai/conveyor/actions/runs/42680838271)
took approximately 102.2 minutes from creation to completion:

| Component | Observed duration |
| --- | ---: |
| Run creation to heavy Rust job start | 75.1 minutes |
| Heavy Rust job | 27.1 minutes |
| Clippy step | 4.8 minutes |
| Entire test step | 18.9 minutes |
| Test step start to first tests executing | About 9.8 minutes |
| First tests executing to test failure | About 9.1 minutes |
| Docker job, running separately | 8.3 minutes |

The test log contains 88 completed binary summaries whose reported runtimes sum
to about 544 seconds before the renderer golden test fails. This is a partial
failed run, not a measurement of the complete passing suite. The first 9.8 minutes
include build preparation, compilation, and linking; Cargo timings are needed to
separate those costs.

[Run 42767949297](https://github.com/nonagenticai/conveyor/actions/runs/42767949297)
started its Rust job promptly but that job was cancelled after about 95 minutes.
Its log was unavailable, so its stall cannot be attributed confidently to
compilation, tests, sccache, or resource pressure. A nested secret-scan job also
failed during setup. Raising timeouts alone does not resolve either condition.

The 40-run snapshot contained 25 cancelled runs, 13 failures, one queued run, and
one running run, with no successes. Cancellations include superseded revisions;
this is evidence of churn, not a claim that 25 runs failed correctness checks.

### Limits of the audit

Cluster API access timed out, including an escalated retry. Runner limits above
are desired GitOps configuration, not verified live capacity. GitHub branch
protection and ruleset endpoints returned HTTP 403 with a plan-upgrade message;
required checks could not be verified. Repository metadata reports auto-merge
disabled. The inspected PR list contained no merged PRs, so post-merge latency
was reviewed from workflow structure rather than measured deployments.

## Review of Gemini's advice

- **Fast hooks and CI-owned full checks:** a useful general approach, but the
  installed hooks already do no Rust compilation. Add useful cheap Rust feedback
  rather than removing nonexistent Rust gates.
- **Changed crates:** useful for local feedback, but `cargo check -p X` checks X
  and its dependencies, not downstream consumers. Full CI coverage must remain
  until dependency-aware selection is demonstrated safe.
- **Documentation filtering:** useful. Rust UI changes are code changes;
  configuration, SQL, fixtures, embedded files, and build scripts can also affect
  compilation or behavior. Use a narrow documentation-only classification and
  treat unknown paths as requiring full validation.
- **Swatinem/rust-cache:** a reasonable implementation, not a solution by itself.
  Existing cache policy, branch scope, failure paths, and disk limits explain
  why replacing the cache action alone may accomplish little. Its default policy
  also removes workspace and incremental artifacts.
- **Disable CI incremental and reduce debug info:** already implemented. Prefer
  explicit Cargo profile settings over another global `RUSTFLAGS` change.
- **Parallel CI jobs:** formatting should move to a light runner. Splitting every
  compiler command or adding a shard matrix immediately would put more pressure
  on the configured two-slot heavy pool.
- **Nextest:** promising after fixture work. It runs each test in a separate
  process, so today's process-local server cache would become a container per
  test process. A shared server without cross-process coordination would expose
  concurrent role updates. Nextest also requires a separate doctest pass.
- **Faster linker:** Linux `x86_64-unknown-linux-gnu` already defaults to LLD from
  Rust 1.90. Blanket claims that default linkers are single-threaded or that macOS
  should automatically switch to LLD are too broad. Measure linking first.
- **Parallel frontend and Cranelift:** optional experiments requiring nightly
  support in the documented configurations. They add another toolchain and cache
  variant; they are not the first response to runner queues. A claimed fixed
  percentage improvement is not transferable to this workspace.
- **Dev LTO:** there is no custom dev LTO setting here; Cargo's unoptimized dev
  defaults already avoid the proposed whole-program optimization problem.

## Proposed implementation sequence

### 1. Establish trustworthy feedback and stop unnecessary heavy runs

Owner: shared CI changes in `gitops-production`; caller rules and developer
commands in `conveyor`.

1. Record queue time separately from execution. Split test compilation
   (`cargo test --workspace --locked --no-run --timings`) from execution while
   preserving the exact test set and the nonzero-test guard. Upload Cargo timings,
   test logs, sccache statistics, and disk usage on failure where the runner lives
   long enough to upload them. Collect pod termination evidence outside the pod
   so eviction and runner-agent failures remain diagnosable.
2. Extract formatting and cheap source checks into the light runner tier. Preserve
   the existing parallel Docker/check structure. Remove the duplicate nested
   secret scan through an explicit shared-workflow option that retains the outer
   scan; standalone Rust library consumers must keep their scan.
3. Add an always-running change classifier and final gate. Documentation-only
   edits should run cheap checks and skip compilation/image creation. The final
   gate must verify the classifier and every applicable check; a failed dependency
   must fail it. Avoid skipping an entire required workflow with `paths-ignore`.
4. Retain same-PR supersession cancellation. Introduce explicit readiness for
   expensive validation of lane PRs, such as ready-for-review or a validation
   label, while the train remains fully validated. Publish clear status explaining
   deferred validation. Adopt this only with agreement on how lanes enter the
   train; merging a lane must not bypass final combined validation.
5. Add versioned local Rust commands: formatting, scoped checking, one integration
   target, and full verification. Keep commit checks to secrets and formatting;
   do not put Docker or full compilation in every commit/push. Formatting must
   handle partially staged files without silently staging unrelated edits.

Validation: compare docs-only, Rust, SQL, fixture, UI, deleted-file, and unknown-path
changes. Confirm that failures and cancelled dependencies fail the final gate.
Confirm lane readiness transitions trigger the expected full run.

### 2. Make existing caches effective and resource use predictable

Owner: primarily `gitops-production`, with profile/caller changes in `conveyor`.

1. Verify MinIO sccache reads and writes using two runs of the same revision, then
   a small code edit. Report hit/miss counts, noncacheable reasons, and errors.
   Installation success is not the acceptance criterion. sccache does not cache
   final linker invocations or metadata-only checks, so it cannot eliminate all
   Clippy work or repeated integration-executable linking.
2. Establish one effective CI debug configuration for Clippy and tests. Benchmark
   `debug=0` against line tables with disk headroom recorded. Preserve local
   debugging and incremental builds. The profile alignment reduces configuration
   variation but does not make Clippy and test compilation identical.
3. Make Cargo registry/download caching usable before the first successful main
   deployment. Review controlled PR cache writes and cache ownership separately
   from compiled artifacts. Do not require a huge target archive to fit beside
   itself on the bounded work volume. Avoid cache writes from untrusted PR code
   into a cache trusted for releases.
4. Measure Cargo jobs 4 versus 8 against actual CPU limits and throttling. Inspect
   host capacity before increasing runner slots. Keep a main/deployment capacity
   budget so lane traffic cannot monopolize all heavy runners.
5. Pilot durable Docker caching. The current caller leaves registry build caching
   disabled; its Dockerfile copies all source before compiling and uses local
   BuildKit cache mounts. Ephemeral builders need an explicit persistence strategy.
   Registry layer caching and Cargo cache mounts are different mechanisms. Evaluate
   a dependency layer, such as a cargo-chef recipe, or persistent builders/sccache;
   prove reuse after an application-source edit on a fresh builder.

Validation: cold run, repeated identical run, one Rust-file edit, and lockfile
change. Compare queue, Clippy, compilation, execution, image build, peak disk,
and cache transfer costs separately. Check the same cases on a failure path.

### 3. Reduce integration-test build and database setup costs

Owner: `conveyor` harness/targets, followed by shared CI runner selection.

1. Benchmark consolidating related integration files into a few explicit suites
   per package. Preserve test discovery and names, avoid compiling modules both
   independently and inside a suite, and retain intentionally separate process
   tests. Compare test inventories and link counts before expanding the pilot.
2. Start one disposable Postgres 18 per CI job and pass its superuser test URL
   through the existing `TEST_DATABASE_URL` mechanism. Preserve fresh databases,
   runtime/owner role separation, and RLS assertions. Bootstrap shared roles once
   or serialize cluster-wide setup with a database advisory lock held on the same
   connection. A Rust mutex alone is insufficient across processes.
3. Pilot a migrated template database for ordinary tests if migration replay is
   material. Retain dedicated tests that run the real migration chain and role
   scripts from empty state. Keep template ownership and privileges equivalent
   to the existing fixture contract.
4. Enable nextest with explicitly bounded concurrency after shared setup is safe.
   Preserve doctests, the nonzero-test guard, and cleanup on failure/cancellation.
   The current custom `test_command` bypasses the shared `test_runner` selection;
   expose the debug profile as an input and remove that override before expecting
   `test_runner: nextest` to take effect.

Validation: compare complete test inventories; run tenancy, privilege, migration,
and concurrency tests repeatedly under the selected parallelism. Measure Postgres
container count and memory. Keep ignored live-service tests under their existing
policy; do not silently remove gates to obtain a faster result.

### 4. Simplify the merge path after the full suite is reliable

Owner: repository process/settings plus shared deployment workflow.

Use the train as the final integration gate while stacked lanes are active. Check
the combined revision before merging, avoid repeated update/merge/recheck churn,
and keep main deployment gated by successful validation. Confirm actual protection
capabilities before relying on a merge queue; none was verified in this audit.

Initially retain post-merge validation. Later consider promoting an already-built,
validated immutable image only when its source tree, merge result, configuration,
and provenance match the revision being shipped. A green PR head alone does not
establish that the eventual merge result is covered.

Version shared workflows deliberately so performance comparisons are repeatable.
Pinning only the outer caller does not freeze nested `@main` calls; version the
nested workflow/action references consistently.

## Acceptance targets

These are proposed budgets, not predicted speedups:

- Commit hooks: warm p95 under 5 seconds; no full Rust build or Docker startup.
- Documentation-only PR: feedback within 2 minutes under normal runner availability.
- Heavy-runner queue: p95 under 2 minutes after traffic and capacity are balanced.
- Warm full Rust validation: initial target under 15 minutes; tighten from measured
  compile and execution costs after the harness pilot.
- Evidence: every full run reports phase durations and cache effectiveness; runner
  loss has an external termination record.
- Correctness: unchanged intended test coverage, doctests, tenant isolation,
  privilege checks, migration coverage, and deployment gating.

## Primary references

- [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Cargo test execution](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Rust cache behavior](https://github.com/Swatinem/rust-cache/blob/master/README.md)
- [sccache Rust limitations](https://github.com/mozilla/sccache/blob/main/docs/Rust.md)
- [Nextest execution model](https://nexte.st/docs/running/)
- [LLD default from Rust 1.90](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/)
- [Parallel compiler frontend](https://blog.rust-lang.org/2023/11/09/parallel-rustc/)
- [Cranelift setup and limitations](https://github.com/rust-lang/rustc_codegen_cranelift)
- [GitHub skipped workflow behavior](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs)
- [Docker cache persistence](https://docs.docker.com/build/cache/optimize/)
