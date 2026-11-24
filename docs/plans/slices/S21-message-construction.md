---
slice: S21
title: Message construction (the compiled prompt)
status: built
depends_on: [S09, S11, S13, S15, S16, T2, T13]
spec_sections: ["§3 D27", "§8 S21", "§4.4", "§4.5", "§7.7", "§10.2", "§11.1", "§11.4"]
adds_tables: [prompt, prompt_section, decision_option_set, decision_option_set_option]
adds_actions: [prompt_recipes.create, prompt_recipes.list, prompt_recipes.get, prompt_recipes.activate, prompt_recipes.retire, cards.assign, ladder.case, ladder.precheck, decision_option_sets.set, decision_option_sets.get]
adds_modules: [prompt, memory, knowledge, executor]
parity_ids: PENDING-S0
---

# S21 — Message construction (the compiled prompt)

Programme project: S21 · issues #15, #581, #590, #599, #612, #624, #649, #656

**Status:** `plan written 2026-11-09`; the v1 seam is built (D27) and steps 1–2 are built. Step 3
(the work-package prompt envelope — recipe authoring, memory selection, knowledge content) is
**built, #574, 2026-11-22**; the design is §12 and the build record is §13. Step 4 (the
`internal_agent` executor runtime and its dynamic tool grant) is **built, #581, 2026-11-22**; the
design is §14 and the build record is §15. Step 5 (the applying gate) is **built, #590,
2026-11-22**; the design is §16 and the receipt is `docs/evidence/S21-APPLY.md`. Step 6 (the work
package's run — the card's binding, the terminal run's memory and the live proof) is **built and
merged, #599, 2026-11-23**; the design is §17 and the receipt is `docs/evidence/S21-RUN.md` (the
live proof is recorded blocked on the estate's caller licence). Step 7 (the run's episodic memory) is
**built and merged, #612/PR #615, 2026-11-23**; the design is §18, the build record §19, the receipt
`docs/evidence/S21-MEMORY.md`. Step 8 (the Request source — the declared step in the process section
and in laya's case input) is **built and merged, #624/PR #627, 2026-11-23**; the design is §20 and the
receipt is `docs/evidence/S21-REQUEST.md`. Step 9 (the work package's handoff — `cards.assign`
assigns the binding, records what the receiver must know and starts the bound run) is **built and
merged, #649/PR #651, 2026-11-24**; the design is §21 and the receipt is
`docs/evidence/S21-HANDOFF.md`. Step 10 (the L1 laya gate — the ladder builds the case, the engine
makes the call and the decision cites the recorded exchange) is **open, #656**; the design is §22,
the decision record ADR 0018 and the receipt `docs/evidence/S21-LAYA.md`.
**Depends on:** S15 (capability/knowledge), S16 (prompt audit), S13 (agent.run), S11 (procedure steps),
S09 (`retrieval.query` / `strata` packs).
**Owner ruling:** George, 2026-11-09 — *a prompt is a run-time compilation from the procedure and the
other system/tenant/project sources, stored in Postgres; prompt-engineering is 2024, context-engineering
2025.*

## 1. What is already true (the seam this slice extends)

- `prompt` + `prompt_section` (`migrations/0030_prompt_catalogue.sql`): a **recipe**, one active version
  per `(tenant, key)`, exact-pin resolution by `<key>@<version>`.
- `conveyor_domain::prompt::compile`: pure renderer over `PromptFacts`; sections are `system`,
  `process`, `capability`, `role`, `project_card`, `knowledge` (step 3 adds `memory`); a required
  section with no source refuses by name; the compiled text's SHA-256 is the provenance key.
- `conveyor_actions::prompt`: three surfaces resolve through it — `agent.run` (resolved path), the
  copilot, and the prompt audit.
- `inference_call.prompt_ref` / `prompt_version` / `prompt_sha256` record which recipe produced which
  call. The compiled text itself is never stored.

This slice is the step **after** that: the compile becomes genuinely dynamic — a selection and budget
step over many candidate sources, not a fixed five-section template.

## 2. The problem with a fixed template

A fixed section list still decides *before* the run what the agent will read. The 2026-2027 target is
the opposite: at the moment of the call, the executor selects the smallest set of sources that makes
*this* step decidable, renders them in a fixed trust order, and records what it chose. The failure
mode today is not a missing section; it is a section that is present, long, and irrelevant — which
costs tokens and hides the one fact that mattered.

## 3. The three components

### 3.1 Procedural — what the agent is doing

Source: the run's `process`/`workflow` and its `run_step` (position, action, gate, `on_error`), plus
the step's declared intent. Rendered today from the frozen published version, so the procedure the
agent reads is the procedure the run was started under.

This is also where a formal representation is worth evaluating (see §6): a step is an action instance
with preconditions (gates), effects (its output), and failure branches (`on_error`). The renderer needs
the *intent* — "move the card to Done and comment why" — not a serialized state machine, so the
representation question is about validation and planning, not about what the model reads.

### 3.2 Domain / capability / role knowledge

Sources: `capability`, `capability_agent` (model, `allowed_actions`, caps), `capability_knowledge`
(pinned packs) and — at call time — `knowledge.query` against the bound `strata` packs. The rule that
survives from D20: a pack is *pinned* to a version; an unpinned or degraded pack is a readiness
blocker, never a silent fallback.

Role knowledge is the third element George named. In v1 the executor runs as `kata_executor` and the
allowlist is the tool surface; the *prompt* does not yet say which role's perspective the agent works
from. S21 adds a `role` section sourced from the run's capability binding (not from the caller's
scopes, which are an authorisation fact, not context).

### 3.3 Project / card knowledge

Sources: `project`, `board`, `list`, `card` (title, description, labels, links), and the work-package
memory digest (`memory.recall`). Two hard rules:

- **Trust:** card text is tenant-supplied **data**, never instruction. It renders inside a labelled
  block after the instruction block; a conflict between the two is resolved in favour of §3.1/§3.2,
  and the system section says so.
- **Freshness:** facts are read at compile time, every time. There is no cached prompt.

## 4. The compile pipeline (the S21 shape)

1. **Select** — the recipe names section *kinds* and a budget; the selector picks concrete sources for
   the run (which packs, which card fields, how many memory items), with a deterministic order.
2. **Fetch** — one store read per source class, inside the executor's transaction where one exists.
   The exception is knowledge: its targets are resolved in the transaction, the transaction is
   committed, and only then is the one retrieval path called with no transaction open (§12).
3. **Render** — the pure renderer, sections in trust order: instructions → procedure → capability/role
   → knowledge → project/card. Untrusted text never precedes the instruction block.
4. **Budget** — a token ceiling per section and per prompt; over-budget is a *refusal with the
   measurement*, never silent truncation (the same rule as the loop caps).
5. **Hash and record** — SHA-256 of the compiled text into `prompt_sha256`. Source identities
   (recipe version, pack `key@version`, memory row ids, knowledge hit ids) are rendered into the
   text, so they are in the hash's pre-image; a past call is reproducible without storing the text.
6. **Audit hook** — the compiled text is what S16 audits; a finding names the recipe version and the
   compiled hash it was found against.

## 5. Provenance and change

- A recipe is immutable per `(key, version)`; an edit is a new version, and `capability_agent.prompt_ref`
  pins one.
- `inference_call` names the pin and the hash. A finding, a call and the text are then one chain.
- Retiring a version stops new runs; past calls remain explicable because their pin still resolves.

## 6. PDDL — evaluation, not prototype

**What PDDL would buy.** A formal action model: preconditions and effects, a planner that can order
steps or detect unreachable goals, and a validation story that is not prose. If procedures are to be
composed (S13 re-authoring 348 flows, S18 improving them), a planner-friendly representation is
attractive.

**What the conveyor already has.** A typed step DSL — `position`, `action`, `input_template`,
`gate_kind`, `on_error`, `max_attempts` — frozen at publish and executed by a deterministic engine
with per-step idempotency, gates as real `approval` rows, and retries. It is already a small planning
domain, expressed in Postgres and enforced by the runtime.

**Why PDDL is not the storage format.** (a) PDDL assumes a closed-world state and full observability;
the conveyor's steps call APIs with side effects whose outcome is only known after the call. (b) A PDDL
action's effects are declarative; a step's effect is an arbitrary JSON output consumed by later
templates. (c) A planner's output would still have to be executed through the engine, so a plan is an
*optimisation*, not a second executor. (d) Authoring cost: every change to a procedure would need a
maintained PDDL model in addition to the step DSL, and two representations drift.

**Recommendation.** Keep the step DSL as the storage and execution truth. Add, in a later S21 step, a
**`plan` projection**: derive a read-only PDDL-ish model from a published procedure (actions = steps,
preconditions = gates, effects = declared outputs) for validation and reachability checks; use it to
*check* a procedure (an unreachable step, a gate that can never open, a step whose input references an
output no earlier step produces), never to execute it. Prototype only if a real procedure fails the
check in a way the renderer cannot explain.

## 7. Failure modes and readiness

- A required source missing → `409 capability_not_ready` with `blocking[]`, before the first step.
- A retired or unknown pin → the same refusal, naming the pin (already implemented).
- Over budget → a typed refusal naming the section and the measured size.
- A pack degraded mid-run → `knowledge_degraded`, the mid-run equivalent.

## 8. Open questions

1. **Budget units** — tokens (provider-tokenizer dependent) or characters? v1 should measure the
   provider's count only when it is cheap; otherwise a conservative character estimate with the measured
   actuals recorded in `inference_call`.
2. **Role section** — whose role: the capability binding's intended role, the run's principal, or both
   with a stated precedence?
3. **Memory** — how many recalled work-package items belong in a prompt before they crowd the card?
4. **Tenant-authored recipes** — when does a tenant get to edit a recipe, and what audit gate applies
   before an edit takes effect? (v1 seeds system recipes only.)
5. **Planner projection** — is the `plan` projection per-procedure, or computed once per published
   version and stored as a validation artifact?

**Status 2026-11-22:** Q1 and Q2 were decided in §10; Q5 was closed in §11; Q3 and Q4 are built by
step 3 (#15) and decided in `docs/decisions/0009-prompt-selection-and-recipe-authoring.md`.

## 9. Acceptance (for the build steps, not this document)

- A step whose recipe requires `procedure` and whose run has none fails by name, before a model call.
- The same recipe and the same facts compile byte-identically (hash equality is the test).
- Changing the card changes the hash; changing an unrelated tenant's card does not.
- An over-budget section refuses with the measured size, never truncates.
- The prompt audit can name the compiled text it audited by `(prompt_ref, prompt_sha256)`.

## 10. Build record — step 1: budgets + the role component (2026-11-09, lane `s21-message-construction`)

**Built.** Migration `0031_s21_prompt_budget.sql` gives the recipe its budget —
`prompt.budget_chars` and `prompt_section.budget_chars`, both nullable (= unbounded), both `> 0` by
CHECK — and adds `'role'` to `prompt_section.source_kind`. `conveyor_domain::prompt::compile` now:

1. stable-sorts the sections by **trust rank** (instructions → procedure → capability/role →
   knowledge → project/card; recipe position is the tie-break), so an untrusted section can never
   precede the instruction block even if the recipe lists it first;
2. renders each section, measures the **block as it will be written** (heading included, characters)
   and refuses `SectionOverBudget { kind, measured, limit }` when it exceeds its own ceiling;
3. measures the joined prompt and refuses `PromptOverBudget { measured, limit }` against the
   recipe's total ceiling;
4. hashes and returns — there is no truncation path anywhere. Both errors carry the measured size,
   and the boundary is exact: `budget = measured` compiles, `measured - 1` refuses (proved in the
   domain tests by re-reading the same measurement).

A new `role` source renders the capability binding's declared perspective as its own section
(`## Role\nrole: <value>`). `capability_agent.params->>'role'` is the v1 source: `params` is
existing editable config on `capability_agents.create|update`, so the role is usable immediately and
no action input/output schema (and therefore no served-spec hash) moves. A first-class column is the
graduation path when roles gain structure. An absent role leaves the section empty — an optional
`role` renders nothing (existing agents compile unchanged), a `required` one refuses by name like
every other missing source.

The seeded `agent@1` gains the optional role section and is re-ordered into trust order
(`system, procedure, capability, role, knowledge, project_card`); the seed stays additive, so an
existing tenant keeps the recipe it already has.

**Decisions recorded for the slice's open questions.** (1) Budgets are **characters** — the
deterministic fallback this document names; token counts are the provider's and already land in
`inference_call.prompt_tokens`. (2) The role is the **binding's** intended role, never the run
principal's scopes (an authorisation fact, not context). (3) Memory selection is **not** in this
step. (4) Tenant-authored recipes are **not** in this step — they need a CRUD surface (new actions).
(5) The `plan` projection stays a **later step**, as §6 says.

**Measured (rbuild on the build host, 2026-11-09).** `conveyor-domain` prompt **6/6**; `conveyor-store` lib
**37/37** (including `embedded_migrations_are_exactly_the_s0_set`, now through 31) and
`prompt_catalogue` **4/4**; `conveyor-actions` `prompt_compile` **4/4** and
`procedure_agent_resolved` **1/1**; `clippy -p conveyor-domain -p conveyor-store -p conveyor-actions
--all-targets -- -D warnings` exit 0. Four reversal proofs fired and were reverted: deleting the
section budget check, the total budget check, the `Role` render arm, and the trust sort each turned
its own test red. A fresh independent deep audit returned **YES**; its two non-blocking findings
(heading-inclusive measurement not mutation-pinned; the `Some(section)` mapping arm unasserted)
were closed and re-measured before handoff.

**Found on the way (not this lane's, fixed here because it was red in the tree).**
`crates/store/src/migrate.rs`'s embedded-set assertion ended at migration 29 while `0030` was
already embedded; the prompt-catalogue session had updated `s07_schema.rs` only. Both ledger lists
now carry 30 and 31.

**Remaining for S21 (at 2026-11-09).** The `plan` projection (§6), tenant-authored recipes (open
question 4: an edit is a new version, and the CRUD surface + its audit gate are the gate), and memory
selection (open question 3). The `plan` projection was built in §11; the other two are step 3 (§12).

## 11. Build record — step 2: the plan projection and declared step outputs (2026-11-09, lane `train-2026-11-09`)

**Owner ruling (2026-11-09): add the declaration.** §6 names three checks, but template forward
references (`template.rs:370`) and unknown gates (`procedure.rs:256`) are already refused at publish,
and the step DSL declared no outputs — so "input references an output no earlier step produces" had
nothing to check against. George's ruling: add `outputs` to the step, accept the migration and the
served-spec change, then write the lock and land the train. Memory selection (open Q3) stays
deferred with a recorded shape: memory selection → memory processing → a `prompt.memory` component.

**Built.** Migration `0032_s21_plan_outputs.sql` adds `procedure_step.outputs jsonb NOT NULL
DEFAULT '[]'` — an array of dot-paths, CHECKed to be an array. `StepInput` / `StepIn` / `StepOut`
carry it, the store binds and reads it, and publish does not copy step rows (it flips the status),
so one column covers a draft and its published version; the frozen revision snapshot carries the
declaration with the rest of `StepInput`.

`conveyor_domain::plan` is the projection and the validation:

- `project(steps) -> Plan` — each step as its action, gate, `on_error`, declared outputs and the
  earlier-step requirements its input template holds.
- `findings(steps, action_outputs, gate_facts, now) -> Vec<Finding>`:
  - `BadOutput` — a declared path is empty, padded, or has an empty segment;
  - `UndeclaredOutput` — the path does not resolve in the action's output schema (the walker handles
    `properties`, `items`, `anyOf`/`oneOf`/`allOf`, `additionalProperties` and `$ref` into `$defs`);
  - `UnproducibleInput` — a `steps.N.output.<path>` the producer never declares, or one that
    does not resolve in the producer's action output schema. A declaration covers itself and its
    descendants (`card` covers `card.id`); a whole-output reference must name a path;
  - `MaybeAbsentInput` — the producer has `on_error=continue`, so a failure persists `Null` and the
    consumer runs with it instead of the run stopping (measured at `step.rs:677-711`);
  - `FutureInput` — a reference to the current or a later step, which cannot have run yet. Publish
    refuses it earlier (`validate_publish`), but the projection is public and must not pass a plan
    whose edges point forward;
  - `GateNeverOpens` — the catalogue row's `meta.expires_at` is present but not an RFC3339 string.
    A VALID timestamp, even one already past, is not refused: the runtime opens the gate and the
    expiry applies the step's `on_error`
    (`procedure_step::an_expired_gate_applies_the_steps_on_error`);
  - `UnreachableStep` — the summary finding for a step carrying an input finding.
- Publish runs the projection after `validate_publish` and refuses `422 procedure.plan_invalid` with
  one field error per finding. Nothing is stored and nothing executes: the plan is derived on demand.

**Open questions.** (5) **closed** — the projection is on demand and enforced at publish, never
stored. (3) and (4) are step 3 (§12): selection → processing → `prompt.memory`, and the recipe
CRUD/activation lifecycle.

**Measured (rbuild on the build host, 2026-11-09).** `conveyor-domain` plan **14/14**; `conveyor-store`
`s11_procedure` **11/11** (including the declared-outputs round-trip) and `s07_schema` **3/3** (the
ledger through 32); `conveyor-actions` `procedure` **12/12** and `procedure_step` **14/14**
(including `procedure.plan_invalid`, the declared-output read-back and the expired-gate on_error
path); clippy `-D warnings` on the three crates `--all-targets` exit 0; rustfmt clean on the touched
files. **Eight** reversal proofs fired and were reverted: deleting the coverage rule, the
maybe-absent rule, the unreadable-expiry gate rule, the `$ref` arm, the reference's action-schema
check, the `FutureInput` arm and the `BadInputReference` emission each turned its own test red, and
removing the cyclic-`$ref` depth guard aborted the test binary on a stack overflow.

**Fresh independent deep audit: NO on first pass; its findings are fixed or answered.** (1) A
present-but-non-string `meta.expires_at` was silently treated as absent by the publish mapping — now
`GateNeverOpens`, with an actions test that inserts such a gate row. The workspace suite then caught
the first version refusing a VALID past timestamp, which `procedure_step::an_expired_gate_applies_the_steps_on_error`
proves is a supported `on_error` path; the check now refuses only an expiry the runtime cannot read.
(2) `allOf` is unioned, not
intersected — deliberate: the walker answers "does the schema DECLARE this path", and a flattened
schemars object adds members in a second branch; documented at `schema_has_path`. (3) The projection
did not reject a self/forward reference on its own — now `FutureInput`, with a test for both. (4) A
non-`#` `$ref` was false-refused — now an external ref does not refuse the declaration. (5) The
fixtures that reference `steps.N.output` are WORKFLOW definitions validated by
`conveyor_workflow::definition::validate_publish`, not procedures, so they are unaffected; the real
consequence — an existing procedure DRAFT must declare its outputs before it can be republished — is
deliberate and recorded in the migration comment.

**Second fresh audit: PASS.** A new agent verified the five dispositions and returned PASS with
three non-blocking findings; two were worth taking. (a) A cyclic `$ref` (root `#`, or a `$defs` entry
that points at itself) recursed without consuming a segment — now capped at 64 hops and refused;
schemars never emits a cycle, so this is hardening, not a reachable bug. (b) A `steps.` reference
whose index is not a positive integer was silently dropped — now `BadInputReference`. (c) A
`FutureInput` also raises the `UnreachableStep` summary; that is the intended shape — every input
finding carries the step-level summary.

**Measured after the second audit (rbuild on the build host, 2026-11-09).** Domain plan **14/14**, actions
`procedure` **12/12**.

**Served-spec lock.** Regenerated over the whole tree: **287 → 297 operations**, `--check-lock` exit
0. The full list of added/changed operations is in `docs/evidence/S21-PLAN.md`.

## 12. Build plan — step 3: the work-package prompt envelope (2026-11-22, issue #15)

**Why this step.** SPEC_V2 §4.5 names four sources for an internal agent's prompt — knowledge,
episodic memory, the PDDL request and the curated tool list — and §7.7 requires one compiler for
all of them. The v1 seam has three: the process/step section is the request, the capability section
names the allowed actions while the executor passes their closed schemas as the runtime `tools`
array, and knowledge is a list of pack ids. Memory is absent entirely, and a tenant cannot author a
recipe. This step closes those three gaps. Laya is not a prompt source: the judge's
`{model, state, questions}` document is built from the step output and the SOP's acceptance
criteria in `crates/actions/src/process/judge.rs`, and it stays there.

**P1 — recipe authoring.** Migration `0062_s21_prompt_selection.sql` adds `draft` to
`prompt.status` (default stays `active`, so seeds and existing rows are unchanged), one draft per
`(tenant, key)` (`uq_prompt_draft`), `prompt_section.selector jsonb NOT NULL DEFAULT '{}'`, and
`memory` to the section-kind CHECK. `prompt_recipes.create` computes the next version for the key
and inserts it `draft` with its sections; `list` and `get` read the catalogue; `activate` retires
the current active version and activates the target in one transaction (the unique active index
refuses a race); `retire` moves a non-retired version to `retired`. A new pin resolves only an
active row, exactly as today. The audit is the existing `action_invocation` row. New scopes are
`prompts:read` (members) and `prompts:write` (admin).

**P2 — memory selection.** A `memory` source and `PromptFacts.memory` join the domain; the recipe's
`selector` declares `kinds`, `limit` (default 8, max 32) and `include_digest`. Selection is
card-scoped and store-only: newest un-consolidated hot rows first (`created_at DESC, id DESC`), and
only the newest `kind='digest'` row if room remains — hot is never displaced. Rendering is
oldest-first as `[kind/source#id] body`; the ids make the selection part of the hash. An optional
section with no rows is absent; a required one refuses by name. The seeded `agent@1` gains an
optional memory section; existing tenants keep their recipe.

**P3 — knowledge content.** `crates/actions/src/knowledge/retrieval.rs` is extracted from
`query.rs`: `resolve_targets_tx` (capability, bindings, token) and `retrieve` (pack leaves + query).
`knowledge.query` reuses both, so there is still one retrieval implementation. `compile_for_run`
resolves the recipe, facts, memory and knowledge targets in a transaction, commits, then calls
`retrieve` with **no transaction open**, and only then runs the pure compiler. The query is
`selector.query`, else the card name + description, capped at 2,000 characters; with no query the
section renders pack identities only. A missing or failed required pack keeps the existing
`409 knowledge_degraded` semantics. The compiled text is never stored.

**Out of this step.** The `internal_agent` executor runtime and its dynamic tool grant remain the
next lane: T0 built the `Executor` contract, and the run path still records a binding without
dispatching on it. No laya change; no PDDL as storage; no second compiler.

**Acceptance.** `crates/actions/tests/prompt_recipes.rs` (new), `crates/domain/tests/prompt_memory.rs`
(new), and extensions to `prompt_catalogue`, `prompt_compile` and the memory store test. The
scenarios are issue #15 P1.1–P1.3, P2.1–P2.3 and P3.1–P3.2; the receipt is
`docs/evidence/S21-WORKPACKAGE.md`.

## 13. Build record — step 3: the work-package prompt envelope (2026-11-22, issue #15)

**Built.** Migration `0062_s21_prompt_selection.sql` (draft recipes, one draft and one active per
key, `prompt_section.selector`, the `memory` source kind). `prompt_recipes.create|get|list|activate|retire`
with `prompts:read|write`; `prompt_recipes.activate` retires the current active version in one
transaction. `PromptSource::Memory` selects the card's newest un-consolidated hot rows, then one
digest only if room remains, and renders `[kind/source#id] body`. `knowledge/retrieval.rs` is the
one retrieval path; `compile_for_run` resolves and commits before retrieving, so no network call
runs inside a compile transaction. Laya is untouched.

**Found by execution.** The first `0062` draft re-added `prompt_section_source_kind_check` with
0031's `procedure` spelling, but `0035_process_tree` had renamed it to `process`; every prompt seed
failed on the new `memory` section before the fix. The constraint now lists
`system, process, capability, role, project_card, knowledge, memory`.

**Measured at the branch head.** Issue #15's acceptance: `prompt_recipes` **7/7**, domain
`prompt_memory` **5/5**, store memory selection **1/1**, `prompt_knowledge` **4/4**. Regressions:
`prompt_compile` **7/7**, `process_agent_resolved` **4/4**, `prompt_catalogue` **4/4**,
`s07_schema` **3/3**, `registry` **12/12**, and the registry-wide HTTP/MCP parity test **1/1**.
The receipt is `docs/evidence/S21-WORKPACKAGE.md`.

**Reversal proofs.** Disabling the memory render arm turned the domain render and hash tests red;
removing the `AlreadyActive` guard made both racing activations answer `200`; dropping the selected
hits from `compile_for_run` removed the knowledge content from the compiled prompt.

**Generated.** `actions.json` **508** actions; coverage and the programme seed measure S21 **built**;
the document pack is regenerated; `served-spec.lock.json` is **539** operations. The lock refresh
also absorbed 12 operations and 20 hashes already missing on `main` (`#508`/`#511`).

**Next lane.** The `internal_agent` executor runtime and its dynamic tool grant. Tools are already
the executor's `tools` array and are named in the capability section; the runtime that dispatches
on `executor_kind` is not built here. §14 is the build plan; the unit is **#581**.

## 14. Build plan — step 4: the `internal_agent` executor runtime and its tool grant (2026-11-22, issue #581)

**Why this step.** Step 3 built the compiled-prompt envelope; nothing consumed it at the run level.
`run.execute` is a bare `queued → running` hook, the run store never reads the binding's
`executor_kind`, and a plain run cannot compile: `facts.process` is empty while the seeded `agent@1`
requires `process` + `capability`. SPEC_V2 §4.4/§4.5 name the target — a run executes under one
executor, the `internal_agent` gets the four-source prompt, generation yields a draft — and §11.1/
§11.4 name the grant that bounds the agent's tool choice. T0 built the contract and #556/#568 built
the bound-tool projection; this step joins them.

**P1 — dispatch.** `run.execute` resolves the run's `job_binding_id`; a run with no binding keeps
the T0 start hook. A bound run re-checks the binding (`active`), its release (`published`), the
snapshot hash, and the kind through the T0 `Registry`, then dispatches through a runtime map keyed
by `ExecutorKind` (`conveyor_actions::executor`). A `procedure` binding on a plain run is refused
`409 executor_kind_unsupported` — `process_runs.start` remains the one process start path. Every
typed refusal is recorded on the run before any model call; the job returns success, so a refusal
is never retried. A third kind registers in the map without editing the run loop.

**P2 — the internal agent.** The frozen release is the execution authority: `model_ref`,
`prompt_recipe`, the caps and the grants come from `release_snapshot.anatomy`; the live
`capability_agent` row is the outer ceiling. The run materialises exactly one `run_step`
(`agent.run`) and drives the one loop (`workflow::agent::drive`) with the one compiler
(`prompt::compile_for_run_with_pins`). A plain run has no process, so the release's `job_key`,
title, charter, `procedure_ref` and authority render as the Request source in the existing
`process` section — no new section kind, no stored PDDL. The output is an unattached `document`
with `meta.generated = true`; `ladder.apply_draft` with an accepted decision is the only apply path.

**P2b — the laya gate.** The effective kind-wide `gate.precheck` policy decides the draft's
disposition. No policy → no laya call, `run_step.judge = no_policy`, the run succeeds with the
draft. A policy → one `laya.judge` with the draft as `state` and the policy's authored question/
criteria (default: the two-level `accepted|rejected` score), then one `decision` row through
`ladder.evaluate` pinning the policy, refinement and laya-config versions, the answer, confidence,
latency and input hash. `accepted` → succeed; `refused` → `laya_refused`; `handover` → the run
fails `gate_requires_approval` and a card-scoped approval is opened; an outage follows
`fail_policy` and never records a counterfactual decision.

**P3 — the tool grant.** The release anatomy is typed (`ReleaseGrant { op, variant,
max_side_effect, enabled }`, `ReleaseLimits`) and parsed at `job_releases.publish`; a malformed
grant refuses `422 release.anatomy_invalid` before the hash, inside the existing snapshot (no
migration). The model's surface is `bound_tools(run) ∩ enabled(grants) ∩ effect ≤
min(grant.max_side_effect, binding.effect_ceiling)`, gated by `tools.call` in the agent's
`allowed_actions`. A grant op with no published bound tool refuses `409 tool_not_ready` before the
model call; a tool above either ceiling is narrowed off the surface; a name never offered is
refused `action-not-found` by the loop and writes no `tool_call`. The binding never widens the
grant — it has no op column to widen it with.

**Out of this step.** No new executor kind; no edit to `crates/domain/src/executor.rs`'s trait
shape; no broker change; no laya change; no PDDL storage; no second compiler; no external runtime;
no run-resume worker for a `handover` gate (step 5, #590, owns it); no mid-loop budget enforcement;
no migration. A `tool_binding` provisioning action remains a follow-up (tests insert raw rows; a
live job needs an operator row).

**Acceptance.** `crates/actions/tests/executor_runtime.rs` (new),
`crates/actions/tests/internal_agent_runtime.rs` (new), extensions to
`agent_run_resolved`, and `crates/store/tests/organisation.rs`. The scenarios are issue #581
P1–P4; the receipt is `docs/evidence/S21-EXECUTOR.md`.

## 15. Build record — step 4: the `internal_agent` executor runtime and its tool grant (2026-11-22, issue #581)

**Built.** `conveyor_actions::executor` — `execute_run` is the one dispatch point: a run with no
binding keeps the T0 start hook; a bound run re-checks the binding (active), its release
(published), the snapshot hash and the kind (through the T0 `Registry`), then dispatches through a
`RunExecutor` map. The `internal_agent` handler compiles with the one compiler (the release's
recipe; the release's model and caps win over the live row), materialises one `run_step`, drives
the one loop, writes the output as an unattached `document` with `meta.generated = true`, and
records the `gate.precheck` laya decision on the step. A `procedure` binding on a plain run is
refused — `process_runs.start` remains the one process start path. `ReleaseAnatomy` types the
job's execution fields; `job_releases.publish` refuses a malformed grant or limit before the hash,
and `job_bindings.create` refuses a kind the release disagrees with. The model's tool surface is
`bound_tools(run) ∩ enabled(grants) ∩ effect ≤ min(grant.max_side_effect, binding.effect_ceiling)`,
gated by `tools.call` in the agent's `allowlist`; an enabled grant with no bound tool refuses
`409 capability_not_ready` before the model call.

**Found by execution.** The first `documents.compose` call failed in the test because the loop acts
as the tenant's service actor and the tenant had none — `agent.run` had already recorded that trap
on 2026-11-17; the executor now resolves the same actor before it composes. The laya gate's first
test run recorded `gate_unavailable` because the tenant had no sealed `laya.token`; the seal is
part of the fixture. A numeric `decision.confidence` decodes as `NUMERIC`, not `f64`, in the test
query. The shared cargo target dir served a stale `conveyor-actions-derive` proc-macro after the
merge (an "unknown scope prompts:write" compile error); touching the derive crate rebuilt it.

**Measured at the branch head.** Issue #581's acceptance: `executor_runtime` **5/5**,
`internal_agent_runtime` **9/9**, `agent_run_resolved` **15/15**. Regressions: `prompt_compile`
**7/7**, `process_agent_resolved` **4/4**, `decision_ladder` **7/7**, `registry` **12/12**, store
`prompt_catalogue` **4/4**, store `organisation` **9/9**, domain `organisation` **12/12**, server
`run_retry` **4/4** and `andon` **3/3**. The receipt is `docs/evidence/S21-EXECUTOR.md`.

**Reversal proofs.** Disabling the binding re-check left a retired binding dispatchable; dropping
the grant filter offered a bound-but-ungranted tool; removing the grant-ceiling check offered a
write tool under a read grant; removing the readiness check called the model for a grant the
binding cannot reach; removing the Request source made the plain run fail `capability_not_ready`
on `process`; removing the decision record left `run_step.judge` empty.

**Generated.** `actions.json` **508** actions (no new action; `agent.run`'s output schema names the
compiled identity); coverage measures S21 **built** with **4/4** modules; the document pack is
regenerated; `served-spec.lock.json` is **539** operations with `agent.run` re-hashed, and the
refresh absorbed four hashes #573 had already left stale on `main`.

**Next lane.** Step 5 (issue #590, §16): the applying gate — `ladder.apply_draft` bound to the
exact draft and the approval-resume worker for a `handover` gate, so an escalated draft can be
decisioned and attached without a human editing the card. No PDDL storage, no second compiler, no
external runtime.

## 16. Build plan — step 5: the applying gate (2026-11-22, issue #590)

**Why this step.** Step 4 generated a draft and, on a `handover`, failed the run and opened a
card-scoped approval: the run was `failed` before a person could decide it, no worker applied the
draft, and `ladder.apply_draft` admitted any accepted non-rule decision naming the card. SPEC_V2
§5.1's `waiting_approval → running` edge, §7.1's L3 human gate and §7.4's "a generated draft applies
only through a gate" describe the target; rework finding `v1-w1c-2:T2` (P0) names the forgeable
gate. This step joins them: the handover parks, the person decides, the resume applies.

**P1 — the gate's binding.** A new `gate.draft_apply` kind for the L3 human decision; `ladder.record`
accepts `rung=human` only from a person and only citing an approved approval with an approve
decision (enforced in the one store writer); `ladder.apply_draft` accepts only an accepted non-rule
decision of `gate.precheck` (L1) or `gate.draft_apply` (L3) whose `meta` names this draft
(`draft_document_id`, and `prompt_sha256`/`run_id` when the document carries them). The apply is one
`pub(crate)` helper shared with the resume worker; `documents.update` stays closed to generated
drafts.

**P2 — the handover parks the run.** `decide()` returns a disposition instead of opening a gate in
its own transaction; the finish transaction opens the `executor` approval with
`open_gate_with_meta_tx` (`running → waiting_approval`, meta naming the draft, position/attempt,
threshold, policy and laya config), records the step `succeeded` and its judge, and does not fail
the run. The draft stays unattached; a retried `run.execute` is a no-op because the run has left
`queued`.

**P3 — decide, then resume.** An `executor` gate is a person's act (the approval writer enforces
`decided_by_kind = human`). On approval, `approvals.decide` records the L3 `gate.draft_apply`
decision bound to the approval and the draft, settles the run `waiting_approval → running`, and
enqueues `run.resume`. The worker applies the draft through the one helper, finishes the run
`succeeded` with one `run.succeeded`, and never calls the model. Rejection and expiry cancel the run
and leave the draft unattached; a reclaimed resume is a no-op.

**Out of this step.** No automatic apply of an L1-accepted draft; no new executor kind; no laya
change; no second apply path; no PDDL storage; no secondary prompt compiler; no mid-loop budget
enforcement; no `memory.remember` write-back; no card-driven run creation; no live model or laya
proof; no migration.

**Acceptance.** `crates/actions/tests/draft_apply.rs` (new), `internal_agent_runtime.rs` (extended
with the parked/approved/rejected/reclaimed scenarios), the rewritten ladder tests, and
`apps/server/tests/approval_expire.rs` (stays green). The scenarios are issue #590 P1–P3; the
receipt is `docs/evidence/S21-APPLY.md`.

**Next lane.** Step 6 (issue #599, §17): the work package's run — a card that names its job
binding drives the run that inherits it, every terminal root run records one attempt in the
card's hot memory, and the path is proved live against the real router and laya. No budget
mechanism (the #526 lane owns `v1-p75-1`), no PDDL storage, no second compiler, no external runtime.

## 17. Build plan — step 6: the work package's run (2026-11-22, issue #599)

**Why this step.** Steps 3–5 built the compiled prompt, the executor and the applying gate, but the
work package itself is still inert: `card.job_binding_id` (`migrations/0040_organisation.sql:203`)
has no reader or writer, so a run is bound only because its caller passed a binding, and the
executor never writes `work_package_memory`, so the prompt's memory section is always empty while
`memory.consolidate` is already enqueued on every terminal transition
(`crates/store/src/run.rs:641`). SPEC_V2 §4.4 ("a card names a binding"), §5.1 (card → run),
§6.6.4 ("memory is written at handoff") and §7.6 (hot rows + derived digest) describe the target;
rework `v1-p66-1` names the handoff gap. This step joins them.

**Not this step — the budget.** `v1-p75-1` (P0, "budget enforcement is off by default and no
run/binding declares a budget") is **already resolved on `origin/issue/526-u5a`** (commit
`8f1e5770`, `crates/actions/tests/tool_budget.rs` 14 passed, an invoke precheck that refuses an
unbudgeted capturing call, `422 budget_missing` at run start). A second gate here would be a
duplicate mechanism. The token ceiling (`ReleaseLimits.max_tokens`, still recorded-only) is that
gate's residual, not this step's.

**P1 — the card names the binding.** `cards.create` and `cards.update` accept `job_binding_id`; the
binding must resolve in the tenant and be `active` (`422 job_binding_unusable` otherwise); `CardOut`
exposes it. `runs.create` resolves the card's binding when the request omits one and refuses an
explicit binding that differs (`422 card_binding_mismatch`); a card with no binding keeps the plain
run path. The pin and stale checks stay in `insert_run_tx`. `cards.duplicate` leaves the binding
unset (an assignment is not content).

**P2 — the terminal run writes its memory.** `enqueue_memory_consolidation` becomes
`record_run_memory`: when a terminal run is card-scoped and a root, it inserts one
`work_package_memory` row through the existing `store::memory::insert_memory_tx` — kind `attempt`,
source `system`, `run_id` set, body naming the attempt and its outcome, `meta` carrying the status
and error class — and then enqueues the existing consolidation job. The row and the terminal status
commit in one transaction; a child run and a card-less run write none. The one-writer rule is
amended in CONTRACT §17 to name this runtime writer.

**P3 — the live wire proof.** An `#[ignore]` test in the existing
`internal_agent_runtime` binary drives one bound run against the estate's real inference-router and
real laya bridge on a fresh local test database (the `s3_wire.rs` pattern) and asserts the model
call, the `gate.precheck` decision, the draft, the attempt row and the run's terminal state. It
refuses when the endpoints or credentials are unset; a stub is not the proof.

**Out of this step.** No budget mechanism (cost, tool spend or token ceiling — `v1-p75-1` is the
#526 lane's); no automatic apply of an L1-accepted draft; no new `cards.assign` action; no
auto-start from a claim or lane move; no prior-step memory in the prompt; no UI; no PDDL storage;
no migration.

**Found and fixed in flight (2026-11-23).** The attempt row was written by
`transition_run_tx`/`fail_run_tx` only; `cancel_run_tx` and both reap sweeps (`run.execute` and
`process.step`) moved runs to a terminal state without it. All three now write the row through the
same `record_run_memory_for_id_tx`, in the same transaction; the store and server tests name each
path. The process-run clause is therefore no longer out of scope.

**Acceptance.** `crates/actions/tests/card_binding.rs` (new), `crates/store/tests/run_memory.rs`
(new), `internal_agent_runtime.rs` (extended with the next-run prompt and live scenarios). The
scenarios are issue #599 P1–P3; the receipt is `docs/evidence/S21-RUN.md`.

**Next lane.** Step 7 (issue #612, §18): the run's episodic memory — the run's own prior steps in
the compiled prompt, `memory.recall`'s hot/digest separation and the process-attempt proof. No
migration, no second compiler, no second memory writer.

## 18. Build plan — step 7: the run's episodic memory (2026-11-23, issue #612)

**Why this step.** Step 3 gave the Memory section the card's hot rows and one digest; steps 4–5
gave the run an executor, a compiled prompt and a gate; step 6 made the work package bind its job
and record its terminal attempt. SPEC_V2 §4.5 names the Memory source as three classes — the run's
prior steps, the card's hot memory and the consolidated digest — and only two are read: `read_step`
(`crates/store/src/prompt.rs:321`) selects position/action/node_id/on_result and nothing queries
`run_step` for earlier outputs. A multi-step run re-derives context it already produced, and a retry
cannot see why the previous attempt failed. Rework `v1-p45-1` names the remainder in its acceptance.
The adjacent defect `v1-p76-2`: `hot_for_card_tx` (`crates/store/src/memory.rs:221`) has no
`kind <> 'digest'` filter, so a derived digest is served as a hot row by `memory.recall` while the
prompt selector already filters it. This step joins them.

**P1 — the run's prior steps reach the prompt.** `PromptFacts` gains the prior-step lines; the
Memory renderer prepends them before the card's rows, each as
`[step/<status>#<position>.<attempt>] <action> <status>: <body>`. The store reads the run's own
`run_step` rows: terminal statuses `succeeded`/`failed`, `(position, attempt)` before the current
step's (so a retry sees its earlier attempt), ordered, bounded by `step_limit`. The selector gains
`include_steps` (default true) and `step_limit` (default 8, max 32). Bodies render whole; an
over-budget section refuses with the measurement, never truncates. A child reads only its own rows.

**P2 — `memory.recall` keeps the digest out of hot.** `hot_for_card_tx` filters `kind <> 'digest'`;
the digest stays reachable through the prompt selector's `include_digest` and the cold read. The
existing test that pins three hot rows (digest first) flips to two; the selector's digest append
stays asserted.

**P3 — the process attempt reaches the next prompt (proof).** #599 writes the terminal attempt on
every path; this step proves a process root run's attempt is read by the next run's prompt and that
a child writes none. No writer is added.

**Out of this step.** No card-driven run creation (a claim or lane move starting a run); no
`memory.remember` write-back at handoff; no automatic apply of an L1-accepted draft; no step
`input_template`/`outputs` as Request; no budget mechanism (the #526 lane's); no parent-run memory
in a child's prompt; no UI; no PDDL storage; no second compiler; no migration.

**Acceptance.** `crates/domain/tests/prompt_memory.rs` (extended), `crates/store/tests/prompt_catalogue.rs`
(prior-step selection) and `crates/store/tests/memory.rs` (the digest/hot flip), and
`crates/actions/tests/process_agent_resolved.rs` / `crates/store/tests/run_memory.rs` (the process
attempt reaches the next prompt). The scenarios are issue #612 P1–P3; the receipt is
`docs/evidence/S21-MEMORY.md`.

## 19. Build record — step 7: the run's episodic memory (2026-11-23, issue #612)

**Built and merged.** PR #615 merged as `0617dd60`: the run's own prior steps render in the one
Memory section before the card's hot rows and the digest
(`[step/<status>#<position>.<attempt>]`, whole bodies, a measured refusal over budget);
`memory.recall`'s hot excludes `digest`; the process attempt is proved to reach the next run's
prompt (the writer is #599's). ADR 0015, CONTRACT §25, DATA-ARCH §2.27; receipt
`docs/evidence/S21-MEMORY.md`. Measured at the head: domain `prompt_memory` **8**, store
`prompt_catalogue` **5**, store `memory` **4**, actions `process_agent_resolved` **5**; regressions
`internal_agent_runtime` 16 (+2 ignored), `prompt_compile` 7, `prompt_recipes` 7,
`agent_run_resolved` 15, `process_step` 20, `decision_ladder` 7, `draft_apply` 7, `s11_process` 14;
three reversal proofs fired; `--check-lock` exit 0. Rework `v1-p45-1` and `v1-p76-2` are cleared.
**No migration.**

## 20. Build plan — step 8: the Request source (2026-11-23, issue #624)

**Why this step.** §4.5's four sources are Knowledge, Memory, Request and Tools; steps 3–7 built
Knowledge, Memory and Tools, and the **Request is still synthesized rather than declared**.
`read_step` (`crates/store/src/prompt.rs`) selects position/attempt/action/node_id and joins
`on_result` only, so the resolved `run_step.input`, the frozen `process_step.outputs` and
`action_text`, and the step's gate never reach the prompt. The model reads `current step: N
(agent.run)` and is never told the arguments it must act on or the artifact it must produce; laya's
case input on the process path sees the resolved input but not the declaration, and on the run-level
path sees no request at all. This step makes the declared step one **request document** for both
consumers.

**P1 — the request facts and their render.** `StepFacts` gains `action_text`, `input` (resolved),
`outputs` (declared) and `gate`; one pure `step_request` builds
`{step, action, action_text, input, outputs, on_result, gate}` and the process section renders it as
labelled data after the current-step line. The store reads `run_step.input` and the joined
`process_step` columns in one read; only a process-driven step contributes them — a synthetic
run-level `agent.run` step contributes none of its executor envelope and keeps the release
definition as its request. Whole values; the section budget refuses with the measurement. No
migration: `run_step.input` is written at claim and `process_step.outputs`/`action_text` exist.

**P2 — the judge receives the same document.** The process judge passes the request document as its
`case_input` (today only `row.input`), and the run-level `internal_agent` judge gains the release
request built by one shared helper with `request_facts`. L1 and L2 cannot disagree about what was
asked.

**P3 — provenance.** The request is inside the compiled text, so `prompt_sha256` covers it; a change
to the resolved input alone moves the hash and the `inference_call` records it; identical facts
compile byte-identically.

**Out of this step.** No card-driven run creation; no `memory.remember` write-back; no automatic
apply of an L1-accepted draft; no budget mechanism (the #526 lane's); no prompt-budget authoring
(`v1-p77-2`); no NULL-hash guard (`v1-p77-1`); no release pinning (`v1-p42-1`); no workflow-node
request (a `run_step` with no `process_step_id`); no parent-run memory; no UI; no PDDL storage; no
second compiler; no migration.

**Acceptance.** New `crates/domain/tests/prompt_request.rs`; extended
`crates/store/tests/prompt_catalogue.rs`, `crates/actions/tests/process_agent_resolved.rs` and
`crates/actions/tests/internal_agent_runtime.rs`. The scenarios are issue #624 P1–P3; the receipt is
`docs/evidence/S21-REQUEST.md`.

## 21. Build plan — step 9: the work package's handoff (2026-11-24, issue #649)

**Why this step.** §5.1's level is card → run → step; steps 3–8 made the card name its binding, the
run compile the four sources and the judge receive the request, but the level has no entry of its
own. No card or board action starts a run — `cards.move` and `cards.update` touch no run, and
`runs.create` starts one only when a caller asks explicitly. SPEC_V2 §6.6.1–5 names the missing
verb: a job hands a card to another job with an action, **records what the receiver must know as
memory**, and every card has one live run. `memory.remember` has zero production callers, so
§6.6.4's handoff note is never written. ADR 0014 deferred exactly this unit ("a dedicated handoff
verb can come with the card-driven entry unit").

**P1 — the handoff's one transaction.** A new `cards.assign` (scopes `cards:write` + `runs:write`)
takes `{card_id, job_binding_id?, note, kind?, meta?, correlation_id?}` and presents the card's
version (If-Match or `_version`, like `cards.update`/`cards.move`), and, in one transaction:
resolves the target binding (the request's, else the card's), checks it is active and in the tenant,
refuses a card that already has a live root run (`409 card_has_live_run`), validates the note, sets
`card.job_binding_id`, writes one hot `work_package_memory` row through the memory writer, and
starts the bound run through the same helper `runs.create` uses (andon, binding pin, attempt,
`run.execute` enqueue, `run.created`). Every refusal precedes the first write. `cards.update`'s
`job_binding_id` stays the plain edit and never starts a run.

**P2 — the note is memory the next prompt reads.** The handoff row is an ordinary hot row, so the
selector renders it in the next run's Memory section (`[fact/<source>#<id>] …`) and its id is inside
`prompt_sha256`. One writer: `memory.remember`'s body is extracted and shared. No new writer.

**P3 — the refactors and the proofs.** `runs.create`'s run-creation body is factored into one
`pub(crate)` helper so the handoff cannot drift from the andon, the pin or the event; its existing
tests are the regression. The action is registered with both scopes, parity-sampled over HTTP and
MCP, and the served-spec lock regenerates. Five reversal proofs: the memory insert, the live-run
read, the binding check, the andon check and the note's source.

**Out of this step.** No auto-start on a bare move or a claim; no claim/renew/release verb; no
automatic apply of an L1-accepted draft; no budget mechanism (the #526 lane's); no handoff of a
project-level (card-less) run; no parent-run memory; no new event kind; no typed
inputs/acceptance-criteria authoring on the handoff; no UI; no PDDL storage; no second memory
writer; no migration.

**Acceptance.** New `crates/actions/tests/card_handoff.rs`; extended
`crates/store/tests/prompt_catalogue.rs`, `crates/actions/tests/registry.rs` and
`crates/actions/tests/parity_http_mcp.rs`; `runs.create`'s existing suites stay unchanged. The
scenarios are issue #649 P1–P3; the receipt is `docs/evidence/S21-HANDOFF.md`.
## 22. Build plan — step 10: the L1 laya gate makes its own call (2026-11-24, issue #656)

**Why this step.** §4.5's level hands generation to the internal agent and §7.4 makes every output
a draft behind a gated action; the gate is the L1 laya decision. The ladder that records it does
not make it. `ladder.evaluate` reads `answer`, `confidence`, `latency_ms` and `probabilities` from
the request body and writes `rung = 'laya'` with whatever policy and laya-config versions happen to
resolve, so a caller with `governance:write` can record a decision with an invented confidence and
clear any threshold. Options are inline request text written per decision, and `Unavailable` — which
`conveyor_domain::ladder` already models — is never produced by the action, so an outage cannot
become `gate_unavailable`. The executor is the one honest caller (it invokes `laya.judge` itself
and hands the answer over), and even it records the verdict without the call that produced it.
Rework `v1-w1c-3`, `v1-w1c-4` and `v1-w1c-5` are these three defects.

**P1 — the case is the ladder's, and the decision cites the call.** `ladder.case` (read) returns
`{case, case_hash, laya_call_key}` built from the effective policy and the tenant's
`laya_inference_config`; `ladder.precheck` is the external action that performs SPEC_V2 §7.2's
`POST /v1/systemone` (`governance:write`, bearer `laya.token`, outside any transaction);
`ladder.evaluate` loses `answer`, `confidence`, `probabilities`, `latency_ms` and `options`, takes
`laya_call_key`, rebuilds the case itself, and verifies the engine's recorded exchange
(`store::idempotency::lookup` with the case's request hash must return the stored 2xx response;
`store::actions::find_invocation_by_key` must name an `ok` invocation). The answer, confidence,
probabilities and latency come from that exchange; the decision records `meta.laya_call_key`,
`meta.laya_invocation_id` and `meta.case_hash`. A request carrying a verdict is `422
caller_supplied_verdict`; an unverifiable key is `422 laya_call_unverified`.

**P2 — options are authored, versioned rows.** Migration `0068` adds `decision_option_set` and
`decision_option_set_option` (one live set per `(tenant, kind, gate)`, a monotonic version, rows
with `key`, `label`, `payload.outcome`). `decision_option_sets.set` authors a set; `.get` reads it.
A `choice` question is built from the authored labels; the answer must be an authored key (`422
laya_answer_not_in_options`); a case that needs a choice and has no set is `409 option_set_missing`.
`decision_option` stays the per-decision record, written from the authored rows.

**P3 — the fail policy is applied and `fire_when` decides.** A failed verification or an unreadable
answer is `Unavailable` → `gate_unavailable`, resolved through the existing
`domain::ladder::unavailable(fail_policy, action)` (closed → refused; open → the action's outcome),
never a pass. `decision_policy` gains a nullable `fire_when` in `0068`; a case that does not fire is
not asked and records no decision. The executor consumes the action's outcome instead of
re-deriving it, and makes exactly one laya request per gate.

**P4 — the proofs and the register.** The existing tests that post a verdict are rewritten to go
through `ladder.case` → `ladder.precheck` → `ladder.evaluate`, never relaxed; new coverage in
`crates/actions/tests/ladder_laya.rs`; the registry, parity sample and served-spec lock move with
the new actions; five reversal proofs (the verdict, the case verification, the option check, the
fail policy, the single call). `v1-w1c-3/4/5` gain `resolved_by`.

**Out of this step.** Automatic apply of an L1-accepted draft (ADR 0011:77, its own unit); auto-start
on a move or a claim; any budget mechanism (the #526 lane's); release pinning (`v1-p42-1`); the SOP
judge (`laya.judge` keeps its own question); a second laya client; a job-driven asynchronous
evaluation; UI; PDDL storage.

**Acceptance.** The issue #656 P1–P4 scenarios at the PR head; `--check-lock` exit 0; the rework
audit clears `v1-w1c-3/4/5`; the receipt is `docs/evidence/S21-LAYA.md`.
