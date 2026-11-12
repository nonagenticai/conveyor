---
slice: S19
title: Eval builder
status: planned
depends_on: [S02, S03, S04, S06, S09, S10, S11, S16]
spec_sections: ["§3 D25", "§5.2", "§5.7", "§6", "§8 S19", "§9"]
adds_tables:
  [
    eval,
    eval_version,
    eval_case,
    eval_metric,
    eval_run,
    eval_case_result,
    eval_attempt_error,
    eval_trace,
    eval_audit,
    eval_audit_finding,
  ]
adds_actions:
  [
    evals.create,
    evals.get,
    evals.list,
    evals.update,
    evals.retire,
    evals.draw_split,
    eval_versions.create,
    eval_versions.get,
    eval_versions.list,
    eval_cases.create,
    eval_cases.update,
    eval_cases.delete,
    eval_cases.list,
    eval_cases.import,
    eval_metrics.create,
    eval_metrics.update,
    eval_metrics.delete,
    eval_signoffs.request,
    eval_signoffs.decide,
    eval_runs.start,
    eval_runs.get,
    eval_runs.list,
    eval_runs.cancel,
    eval_results.list,
    eval_results.get,
    eval_results.label,
    eval_runs.report,
    eval_runs.export,
    eval_audits.start,
    eval_audits.get,
    eval_audits.list,
    eval_findings.list,
    eval_findings.decide,
  ]
adds_events:
  [
    eval.created,
    eval.changed,
    eval_case.changed,
    eval_run.changed,
    eval_result.recorded,
    eval_audit.completed,
    eval_finding.decided,
  ]
adds_modules: [eval]
parity_ids: PENDING-S0   # net-new: this slice ports no old surface, so S0 has no id for it
---

# S19 — Eval builder

Programme project: S19 · issues #23, #46

## 1. Objective

The `claude-api` `build-eval` and `eval-audit` methods become an ordinary product surface: a person
defines **one** flow, builds an input set, signs off the inputs, chooses metrics and a grader,
signs off the grading, runs a pilot and then a baseline, and reads a report — all against rows, not
a `.claude/hillclimb/<flow>/` directory of `results.jsonl`, `traces/` and `report.html`. At the end,
"did this prompt, model or procedure change help?" has a number with a confidence interval, a
per-case transcript behind every score, and a health audit that says whether the number can be
believed at all.

## 2. Starting state

- S04's registry and invocation engine are the only way anything is invoked; `completions.create`
  (S10) is the one model call and runs against `inference-router`. There is no model SDK.
- S06's `approval` is the single human-escalation primitive and `job` is the only durable runner
  (`FOR UPDATE SKIP LOCKED`, `dedupe_key`, leases, `max_attempts`).
- S16's `inference_call` is planned and **not built** (`0024` reserved, unapplied). S11's
  `procedure`/`run_step` and S10's `conversation_turn` exist and are the trace sources; S09's
  `document` + object store are the export sink and the attachment store.

## 3. Scope

**IN.** The ten tables (§2.19); the `approval.eval_id` column and `uq_appr_eval_pending` index;
the thirty-three actions, three scopes and seven events; the four jobs; the three built-in
versioned prompts; screens **15–20** declared for S12. Parts A (define, inputs, sign-off), B (run,
grade, capture) and C (report, audit, interop).

**OUT.** Any UI — screens are declared here so S12 may not invent them. Building a model call of
its own: the runner invokes a registered action and never reconstructs a request, which is
`build-eval.md`'s *"reimplementing the app"* failure. Hosting a model, embeddings or a second
runtime (D22). The hillclimb loop and the cost search — **S20**. Amending the benchmark
instrument (S17/S18): they are untouched, and no `benchmark_*` name is reused. Absorbing
`inference-router` or `billing_service`; the conveyor derives cost from its own ledger and emits
none (D23). Grading *paths* rather than outcomes for agentic subjects — the end state is the
answer.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `…/claude-api/shared/evals/build-eval.md` (the installed skill) | Steps 0–4, the two sign-offs, the four runner properties, the row/trace/`_state` field set | the **method and the field set verbatim** | its file layout as a requirement — the rows are the record, the files are the export |
| `…/claude-api/shared/evals/eval-audit.md` | §1–§6 and the Tier-3 auditor prompt | the checks as **construction requirements**, and §6's "observations and suggestions, severity first" as the report's voice | the assumption that an agent runs the audit by reading code |
| `…/claude-api/shared/evals/report/SCHEMA.md` | `Metric`, `PerfField`, `Variant`, `RepResult`, `Turn`, `SplitScore` | the column and turn shapes, and the `label ≤ 14 chars` rule | `report.html` as the deliverable — a screen replaces it |
| `…/claude-api/shared/evals/report/build-report-lite.mjs` | how `trajectory/scores.tsv` is derived | that it is **derived, never written by the runner** | Node, or any second renderer |
| `crates/actions/src/invoke.rs` | `invoke`, `record_bookkeeping` | the one invocation path a case must use | a second invoker |
| `crates/actions/src/spec.rs` | `ActionSpec`, `Scope`, `AGENT_TOOLS` | the declaration pattern | its counts — they change here |
| `migrations/0002_domain.sql` (`approval`, `:221`) | `uq_appr_pending` and its comment *"This index, not check-then-insert, is the guarantee"* | the guarantee, extended to `eval_id` | a `meta`-only gate |
| `migrations/0001_platform.sql` (`job`, `:120`) | dedupe/lease/attempt columns | the runner's four properties for free | a second queue |

## 5. Deliverables

`migrations/0026_s19_eval.sql` (next free number at start; `0026` at time of writing — `0019`,
`0020` and `0024` stay reserved), DDL exactly as `02-DATA-ARCHITECTURE §2.19`, plus:

```sql
ALTER TABLE approval ADD COLUMN eval_id uuid;
CREATE UNIQUE INDEX uq_appr_eval_pending
    ON approval (tenant_id, eval_id, gate) WHERE status = 'pending' AND eval_id IS NOT NULL;
```

Modules: `crates/domain/src/eval/{mod,state,grade,split,cost,audit}.rs`,
`crates/store/src/eval/{mod,evals,cases,metrics,runs,results,audits}.rs`,
`crates/actions/src/eval/{mod,evals,versions,cases,metrics,signoffs,runs,results,report,audits}.rs`,
`crates/actions/src/prompts/{eval_judge_rubric_v1,eval_judge_pairwise_v1,eval_audit_v1}.md`
(`include_str!`). Plus `spec.rs` (three scopes, `AGENT_TOOLS`, the registry `EXPECTED` array),
`crates/actions-derive/src/lib.rs` (the wire table), `apps/server/src/worker.rs` (four arms),
`crates/api/src/eval.rs` (read routes), `served-spec.lock.json`.

| action | input | output | scope |
|---|---|---|---|
| `evals.create` | `{name, purpose, subject_kind, entry_action, model_ref, prompt_ref, inputs_schema, trace_source}` | `EvalView` (`draft`) | `evals:write` |
| `evals.get` / `list` / `update` / `retire` | `{id}` / `{status?, page}` / `{id, patch}` / `{id}` | `EvalView` / `Page<EvalView>` | `evals:read` / `evals:write` |
| `evals.draw_split` | `{eval_id, train_ratio, seed?}` | `VersionView` (`split_sha256`, per-tag counts) | `evals:write` |
| `eval_versions.create` | `{eval_id}` | `VersionView` (frozen copy + `digest`) | `evals:write` |
| `eval_cases.create` / `update` / `delete` / `list` / `import` | `{version_id, input, tags[], expected?, attachments?, source, gold_provenance, gold_model?}` … | `CaseView` / `Page<CaseView>` | `evals:write` |
| `eval_metrics.create` / `update` / `delete` | `{version_id, id, role, kind, better, label, unit?, grader_kind, grader, rubric?, ordinal}` | `MetricView` | `evals:write` |
| `eval_signoffs.request` | `{eval_id, gate: eval_inputs\|eval_grading\|eval_cost\|eval_harness, subject_sha256?, ask}` | `ApprovalOut` | `evals:write` |
| `eval_signoffs.decide` | `{approval_id, decision, note?}` | `ApprovalOut` | `evals:approve` |
| `eval_runs.start` | `{eval_id, version_id, variant, reps, mode: pilot\|full, case_ids?, cost_ceiling_cents?}` | `RunView` (`queued`) | `evals:approve` |
| `eval_runs.get` / `list` / `cancel` | `{id}` / `{eval_id?, status?, page}` / `{id}` | `RunView` / `Page<RunView>` | `evals:read` / `evals:approve` |
| `eval_results.list` | `{run_id, split?, tag?, status?, page}` | `Page<ResultView>` (no `expected`) | `evals:read` |
| `eval_results.get` | `{run_id, case_id, rep}` | `{result, turns}` — `split='test'` needs `evals:approve` | `evals:read` |
| `eval_results.label` | `{result_id, human_score, note?}` | `ResultView` (judge calibration) | `evals:approve` |
| `eval_runs.report` | `{run_id, group_by?}` | `{metrics[], perf[], per_tag[], errors, truncated, noise_floor, headroom, cost}` | `evals:read` |
| `eval_runs.export` | `{run_id}` | `{document_id}` (S09 document, gzip JSONL + traces) | `evals:read` |
| `eval_audits.start` / `get` / `list` | `{eval_id, version_id, target_run_id?}` / `{id}` / `{eval_id?, page}` | `AuditView` / `{audit, findings[]}` | `evals:write` / `evals:read` |
| `eval_findings.list` / `decide` | `{audit_id?, severity?, status?, page}` / `{finding_id, decision, note?}` | `Page<FindingView>` / `FindingView` | `evals:read` / `evals:write` |

Screens declared for S12: **#15 Evals** (`evals.*`, `eval_cases.*`, `eval_metrics.*`,
`eval_signoffs.*`; SSE `eval:{id}`), **#16 Eval run** (`eval_runs.*`, `eval_results.*`; SSE
`eval_run:{id}`, `eval_result.recorded`), **#17 Eval report** (`eval_runs.report/export`,
`eval_results.*`), **#18 Eval audit** (`eval_audits.*`, `eval_findings.*`), **#19 Eval sign-off**
(the two literal pauses as full-screen reviews), **#20 Eval interop** (import/export). Each with
its `apps/web/e2e/<name>.spec.ts`.

Jobs: `eval.case.run` (one `(case, rep)`), **`eval.run.tick`** (finaliser + heartbeat),
`eval.reap`, `eval.prune`. Scopes: `evals:read` (member), `evals:write` (member),
**`evals:approve` (`owner`/`admin` only)** — building is not signing, and the held-out trace and the
run cost both hang off the sign-off scope. ⛔ **`kata_executor` is deliberately NOT granted it**: that
role is held only by a machine principal, and a scope a machine holds cannot be a sign-off. `AGENT_TOOLS`
gains `evals.get/list`, `eval_cases.list/create`, `eval_results.list`, `eval_runs.get/report`,
`eval_audits.get`, `eval_findings.list` (22 → 30) and **never** `eval_signoffs.*`, `eval_results.get`
for a test split, or `eval_runs.start`.

## 6. Build order

**Part A — define, inputs, sign-off.** Steps 1–7. Proves: an eval reaches `inputs_approved` and a
run is refused until it does.
**Part B — run, grade, capture.** Steps 8–12. Proves: a 5-case pilot yields 5 complete result rows,
each with a transcript and a grade dict.
**Part C — report, audit, interop.** Steps 13–17. Proves: a report recomputes from raw rows and an
audit reproduces.

1. `0026_s19_eval.sql` + the `approval` column/index; touch `crates/store/src/migrate.rs` (the macro
   embeds at compile time). `rbuild -- run -p conveyor-server -- migrate`, then verify **the ledger**:
   `SELECT count(*) FROM _sqlx_migrations WHERE version=26 AND success` = 1.
2. `domain::eval` — subject kinds, `EVAL_TRANSITIONS`, the readiness predicate (which gates are
   missing), variant grammar `baseline|v<N>`, the retry policy. `rbuild -- test -p conveyor-domain eval_`.
3. `store::eval` — CRUD, the `(run, case, rep)` slot, the frozen version copy, RLS tests.
   `rbuild -- test -p conveyor-store eval_`.
4. `evals.*` + `eval_versions.*`. `rbuild -- test -p conveyor-actions evals`.
5. `eval_cases.*` + `eval_cases.import`; **`expected` is write-only** — no view carries it.
   `rbuild -- test -p conveyor-actions eval_cases`.
6. `evals.draw_split` — stratified by `tags[0]`, seeded from the version digest, fixed once
   (`409 split_locked` on re-draw), and train/test means checked within noise.
   `rbuild -- test -p conveyor-actions eval_split`.
7. `eval_signoffs.*` — open and decide an `approval` with `eval_id` and
   `meta.artifact_sha256`; a decided sign-off whose artifact digest no longer matches is
   **void**. `rbuild -- test -p conveyor-actions eval_signoff`.
8. **The case driver** — `eval.case.run`: build subject input from `input` alone (a type with no
   `expected` field), invoke through the engine, write `eval_case_result` + `eval_trace`; on a
   pre-output failure write `eval_attempt_error` and leave the slot empty.
   `rbuild -- test -p conveyor-server eval_runner`.
9. **The four runner properties** — `deadline_at` per attempt (not inactivity), jittered backoff
   with `attempt` visible, strict-per-attempt scoring, a failure class on every failed attempt.
   `rbuild -- test -p conveyor-server eval_runner_properties`.
10. `eval_runs.start` emits one `job` per `(case, rep)`; `eval.run.tick` finalises and heartbeats;
    `eval.reap` keys on `last_polled_at`. `rbuild -- test -p conveyor-server eval_jobs`.
11. **Graders** — `programmatic` (normalised exact/contains/schema/label), `rubric` and `pairwise`
    over `completions.create` with structured output, `human` (a queue); the judge model may not be
    the subject model (`422 judge_is_subject`). `rbuild -- test -p conveyor-actions eval_graders`.
12. The served-model assertion and `eval_results.label` (judge-vs-human agreement).
    `rbuild -- test -p conveyor-actions eval_served_model`.
13. `eval_runs.report` — mean + CI, noise floor `1/sqrt(n·reps)`, headroom, per-tag breakdown,
    error/truncation counts, cost derived from `inference_call`. `rbuild -- test -p conveyor-actions eval_report`.
14. `eval_audits.start` — §1–§6 mechanical checks, the **oracle ≈100% / null ≈0%** probe, the judge's
    three known negatives, and the noise-floor-vs-headroom-vs-smallest-shippable-delta line.
    `rbuild -- test -p conveyor-server eval_audit`.
15. `eval_runs.export` / `eval_cases.import` — the method's bundle in and out; no attempts on
    import. `rbuild -- test -p conveyor-actions eval_roundtrip`.
16. Scopes at the five sites, `AGENT_TOOLS`, `EXPECTED`, the api count, `parity_http_mcp` samples,
    SSE topics, `eval.prune` on a `schedule` row. `rbuild -- test -p conveyor-actions registry`.
17. `--print-openapi --write-lock`, counts into `00-INDEX.md`, the gate below.

## 7. Gate

⚠️ **Rust tests, not `psql` probes, and `rbuild`, not `cargo`.** Since 2026-11-09 the tests run on the build host
against a throwaway Postgres it creates and migrates itself (CONTRACT §16), so a local `psql` against compose
`:5439` observes a different database and proves nothing; "ten tables RLS-forced" and "0026 in the ledger" are
therefore ordinary `#[test]`s. `cargo` refuses on this machine — every line is `rbuild -- <args>`, and the last is
`rbuild --gate` (`fmt --check && clippy && test --locked`).

```bash
set -euo pipefail
cd $HOME/Movies/conveyor

# 1. ten tables RLS-forced, migration 0026 in the LEDGER (not the migrate log line, which says nothing)
rbuild -- test -p conveyor-store --test eval_schema -- --exact ten_tables_rls_forced_and_0026_applied

# 2. the sign-off index exists AND is the guarantee (not a check-then-insert)
rbuild -- test -p conveyor-store --test eval_signoff_idx -- --exact a_second_pending_signoff_is_refused

# 3. no run without both sign-offs; the refusal names the missing gate; an edit voids the sign-off
rbuild -- test -p conveyor-actions --test eval_signoff_gate

# 4. the answer key never reaches the subject
rbuild -- test -p conveyor-domain --test eval_isolation -- --exact subject_input_carries_no_expected_bytes

# 5. the held-out wall is access, not a prompt
rbuild -- test -p conveyor-api --test eval_heldout -- --exact an_agent_principal_cannot_read_a_test_trace

# 6. a failed attempt cannot occupy the (case,rep) slot
rbuild -- test -p conveyor-store --test eval_slot

# 7. cost is derived, never stored — a NEGATIVE column count, asserted in Rust so it can go red
rbuild -- test -p conveyor-store --test eval_constraints -- --exact no_eval_table_has_a_cost_column

# 8. oracle ≈100%, null ≈0%, an induced error is a classified attempt, the judge fails its three negatives
rbuild -- test -p conveyor-server --test eval_controls

# 9. the variant grammar, the grade-object shape, and the label length are database invariants
rbuild -- test -p conveyor-store --test eval_constraints

# 10. no answer-key or sign-off action is agent-reachable
rbuild -- test -p conveyor-actions --test registry -- --exact no_eval_signoff_or_answer_key_action_is_agent_reachable

# 11. the served surface and the lock agree (the repo's own checker, not a hand-rolled count)
rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock

# 12. fmt --check + clippy -D warnings + the whole suite, once, on the host
rbuild --gate

printf '\nGATE_GREEN\n'
```

⛔ `rbuild --gate` signals by exit code, not by `GATE_GREEN`; the `printf` is last only so a truncated log is
distinguishable from a finished gate. It takes the host's repo lock — a second run exits **75** and names the
owner, so a red gate and a busy host are not confused.

## 8. Traps

1. **`sqlx::migrate!` embeds the set at compile time.** A new `.sql` is invisible until the crate
   rebuilds and `migrate` exits 0 either way. Touch `migrate.rs`; verify by `_sqlx_migrations`.
2. **The sign-off is not a status field.** `uq_appr_pending` is over `(tenant_id, run_id, gate)` and
   `run_id` is NULL for an eval gate — Postgres treats NULLs as distinct, so a `meta`-only gate
   allows two pending sign-offs. The `eval_id` column and its own partial index are the guarantee;
   a `check-then-insert` is exactly what S03's comment forbids.
3. **A grade dict, not a scalar, whenever there is more than one metric.** A bare bool alongside
   declared metrics renders as a dash in every column — the method calls it out, so make it a CHECK.
4. **A failed attempt must never write the slot.** A row at `(case, rep)` makes resume skip it
   forever and scores plumbing as a model failure. `eval_attempt_error` is append-only across
   resumes; a later success supersedes its error rows and *does not delete* them.
5. **`expected` is grader-only.** It must not appear on any view, any export of a case list, or in
   `AGENT_TOOLS`. A "don't look at the answers" prompt is not a defence.
6. **The runner marks truncated, never zero.** A response cut at `max_tokens` is `status=truncated`,
   counted and excluded from the mean; scoring it wrong inflates every regression.
7. **`model` comes from the response, not the config.** A silently substituted served model
   invalidates the comparison; assert it and fail the attempt loudly.
8. **A per-case deadline is not an inactivity timer.** A hung stream can emit keepalives forever;
   only a total ceiling reclaims the slot — and a `timeout` is a classified attempt, not a zero.
9. **N×R jobs finalise nothing.** Without `eval.run.tick` a run stays `running` forever after its
   last case; the reaper keys on `last_polled_at`, never on case silence (the S17 lesson).
10. **Case text is untrusted.** It comes from transcripts and tickets. No `inner_html`, `html`/`svg`
    attachments in a sandboxed frame, binaries as S09 `document` refs, and a size cap on `turns`.

## 9. Decisions closed here

- **The instrument is versioned** (design cross-check 2026-11-09). `eval_version` freezes the case
  set, metrics, graders, prices and harness paths under one digest; a run names a version and a
  compare across versions is refused. `eval-audit.md` §2: scores before and after a grader change
  are not comparable. Copy-on-write is cheap (≤100 cases) and exact.
- **Sign-offs are `approval` rows**, with a nullable `eval_id` and its own partial unique index.
  One escalation primitive, so a sign-off is answerable in the web, Telegram and MCP channels for
  free. **Contract change taken:** an ALTER on S03's `approval` (additive, nullable) — CCR recorded.
- **`evals:approve` is its own scope.** Building is not signing, and the held-out trace and the run
  cost hang off it. `eval_signoffs.*` and `eval_runs.start` are absent from `AGENT_TOOLS`.
- **The answer key is structurally unreachable.** The subject-input type has no `expected` field,
  no action returns it, and no answer-key action is agent-reachable.
- **Cost is derived from `inference_call`, never stored** (D23). **Amendment taken by S19:**
  `inference_call.caller_kind` gains `eval` and `inference_call` gains a nullable `eval_run_id`
  referencing S19's `eval_run.id` only — never a `benchmark_run.id`, which is a different uuid and
  a different instrument. S17 records no inference calls of its own, so the ledger is the only cost
  record for both.
- **Two of the four grader kinds call a model, and both use structured output.** Free-text JSON
  fails on an unescaped quote in the explanation often enough to matter.
- **The report is a screen; export is interop.** No hand-rolled HTML anywhere — the one HTML surface
  the skill allows is its own escaping report builder, and here the escaping view is Leptos.
- **Retention:** `eval_trace` 90 days (it is the bulk); results, cases and audits live with the eval.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Does S19 block on S16, or record `usage` itself? | block on S16; if it slips, record `usage` on `eval_case_result` from the action response and derive cost with the same arithmetic — identical columns, different source |
| 2 | Which subjects are runnable at first? | `completions`, `copilot`, `procedure`, `external`; `agent_run` is declared and refused `409 subject_not_ready` until S13/S15, the shape S17 uses for its `conveyor` arm |
| 3 | Is the pilot a separate run row or a mode of the same one? | a mode (`mode: pilot`) writing to a `variant='baseline'` run marked `partial`; the full pass supersedes it, and the pilot's rows are kept |
| 4 | Does a human-graded metric block a run? | no — it parks the result `pending_label` and the report excludes it, naming the count; a queue that blocks would stop every automated pass |
| 5 | Can two evals share a case set? | no — a version is owned by one eval; `eval_cases.import` copies, it never links |
