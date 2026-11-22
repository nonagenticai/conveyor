# 0010 — The internal_agent executor runtime and its tool grant

Status: Accepted · Date: 2026-11-22 · Issue: #581 · Resolves: S21 §14, SPEC_V2 §4.4, §4.5, §7.2–§7.4, §7.7, §11.1, §11.4

## Decision

1. **A run dispatches on its binding's `executor_kind`, through one registry.** `run.execute`
   resolves the run's `job_binding_id`, re-checks the binding's status, its pinned release status
   and the snapshot hash, then dispatches through a runtime map keyed by `ExecutorKind`. The T0
   `conveyor_domain::executor::Registry` stays the kind/contract authority (it validates the
   binding at creation and at dispatch); the execution map lives in `conveyor_actions::executor`
   because domain must not depend on the store. Adding a kind is one `RunExecutor` implementation
   plus one `register` call — the run loop never matches on a kind. A run with no binding keeps
   the T0 start hook (`queued → running`, no step), which is what `projects.execute` uses.

2. **A plain run on a `procedure` binding is refused, not started.** A procedure binding executes
   through `process_runs.start`, which owns the one process start path (readiness, materialised
   steps, the first `process.step` job). Re-implementing that on an existing run would be a second
   start path. The refusal is `409 executor_kind_unsupported` naming `process_runs.start`; the run
   is recorded failed with that class.

3. **The frozen release is the execution authority.** `model_ref`, `prompt_recipe`, the loop caps
   and the tool grants come from `job_binding.release_snapshot.anatomy`; the live
   `capability_agent` row supplies its `allowed_actions`, params and caps as an outer ceiling. The
   effective value is the release's when present, else the row's; caps are the minimum of the two
   and the platform maxima. The same helpers feed the compiled prompt's capability section and the
   loop config, so what the model reads and what the call does cannot disagree. A missing model or
   recipe is `409 capability_not_ready` naming the field, before the first model call.

4. **The release anatomy is typed, and publish proves it.** `ReleaseAnatomy` (in
   `conveyor_domain::organisation`) parses the job definition's `executor_kind`, `model_ref`,
   `prompt_recipe`, `charter`, `authority`, `gate`, `limits` and `grants`. A grant is
   `{op, variant, max_side_effect, enabled}`; `op` is `<toolkit>.<operation>`, `variant` is absent
   or `default` (the tool platform has no variant column), `max_side_effect` is one of the four
   effect classes, `enabled` defaults true. A bare-string grant, an unknown effect, a duplicate
   `op`, an unknown `limits` key or an out-of-range cap is refused `422 release.anatomy_invalid`
   at `job_releases.publish`, before the hash. The anatomy lives inside the existing
   `release_snapshot` JSONB: no table, no column, no migration, and the grant is inside
   `release_hash` by construction.

5. **The tool grant is the release's operation allowlist; the binding narrows it and never widens
   it.** The model's surface is
   `offered = bound_tools(run) ∩ enabled(release.grants) ∩ effect ≤ min(grant.max_side_effect,
   binding.effect_ceiling)`, and only when the agent's `allowed_actions` names `tools.call`.
   `bound_tools` supplies published tools with their pinned schemas; the release names ops and
   never carries tool content; the binding pins versions, a ceiling and a credential alias and
   has no op column. Replacing any of the three with a wider set can only shrink or empty the
   surface. A grant op with no published bound tool is `409 tool_not_ready` naming the op, before
   the first model call — the model must not be offered a job it cannot do. A tool above either
   ceiling is narrowed off the surface, not an error. A name the surface never offered is refused
   `action-not-found` by the loop before execution, and writes no `tool_call`.

6. **Generation yields a draft, and only a gate applies it.** The run materialises exactly one
   `run_step` (position 1, attempt 1, `agent.run`) and drives the one loop. The output is an
   unattached `document` with `meta.generated = true`; `documents.update` already refuses to
   re-attach such a draft, and `ladder.apply_draft` (with an accepted L1/L3 decision bound to that
   draft — ADR 0011) is the only apply path. A run finishes `succeeded` with the draft or `failed`
   with a typed class; generation never moves a card, list or board.

7. **Laya decides the configured pre-check, and only when a policy exists.** The executor reads
   the effective kind-wide `gate.precheck` policy. No policy → no laya call and no decision row;
   `run_step.judge` records `no_policy`. A policy → one `laya.judge` call with the draft as
   `state`, the question and criteria from the policy's authored `meta` (defaulting to the
   two-level `accepted|rejected` score the process judge already uses), then one `decision` row
   through the ladder's one writer (`ladder.evaluate`), pinning the option-set version, the policy
   and refinement versions, the laya config version, the latency and the input hash. `accepted` →
   the run succeeds; `refused` → `laya_refused`; below threshold per the policy's action: `block`
   → refused, `annotate` → accepted with the note, `handover` → the run parks `waiting_approval`
   and a card-scoped `executor` approval naming the draft is opened; the approval applies it
   (ADR 0011). A laya outage follows `fail_policy` through the ladder's `unavailable`: `closed` →
   `gate_unavailable`, `open` → the configured action; an outage never records a counterfactual
   decision and never passes a closed gate.

8. **No second loop, no second compiler, no second tool path.** The executor calls
   `workflow::agent::drive` (the one capped loop, one `inference_call` per model call, one
   `action_invocation`/`tool_call` per tool call) and `prompt::compile_for_run_with_pins` (the one
   compiler). The one addition to the compiler is a synthesized Request source for a plain run
   that has no process: the release's `job_key`, title, charter, `procedure_ref` and authority
   render into the existing `process` section, so the four §4.5 sources exist without a new
   section kind and without stored PDDL.

## Why

S21 step 3 made the prompt genuinely dynamic (recipe authoring, memory selection, knowledge
content). The runtime that consumes it did not exist: `run.execute` was a bare start hook, the run
store never read the binding's kind, and a plain run could not even compile (`facts.process` empty,
`agent@1` requiring `process` + `capability`). The binding and release rows already existed (T0) and
the tool half already existed (#556/#568); this unit joins them at the one seam SPEC_V2 §4.5 names.

## Alternatives rejected

- **Make `conveyor_domain::Executor` carry the async execute method.** It would drag `sqlx`, the
  store and the invoker into domain. The domain registry stays the contract authority; the
  execution map lives in actions. Recorded here so a later session does not "simplify" it back.
- **Start a procedure run from `run.execute`.** A second process start path; refused instead.
- **A `tool_grant` table.** A second store for a fact the release already pins inside its hash;
  §11.1's "one owner, two rows" is the release snapshot plus `tool_binding`.
- **Refuse when the release and the live agent row disagree on the model/prompt.** It would force
  an immutable release to track a mutable row and leave the compiled section with no correct
  value. Release-over-row with min caps keeps one truth.
- **Park a plain run in `waiting_approval` on `handover` (in step 4).** Deferred, not rejected for
  good: no worker kind resumed a plain run then, so the run failed instead of holding `uq_run_live`
  forever. Step 5 (#590) built the park, the human decision and the `run.resume` worker; ADR 0011
  supersedes this bullet.
- **Enforce the grant in the broker for direct `tools.call` callers.** The broker's binding ceiling
  already governs a direct caller; the release grant bounds the *agent's choice*, which is decided
  where the surface is projected. A broker change is a separate unit if a direct caller must also
  be grant-bounded.

## Consequences

- `run_execute` becomes real engine work: DB and 5xx failures are retryable under the same
  idempotency prefix; every typed refusal is recorded on the run and the job returns success.
- The seeded jobs move from display-string grants/limits to the typed shapes; an existing release
  with display strings stays readable and a run bound to it fails `capability_not_ready` (the pin
  is immutable; a republish is the fix).
- `NewWorkflowStep.node_id` becomes `Option<String>` so the one `run_step` store serves both
  workflow nodes and the run-level step. The column is already nullable; no migration.
- `docs/design/data/coverage.json` measures S21's modules 4/4 once `executor` exists.
- No new action, scope or route; the registry count and the served-spec lock do not move.
