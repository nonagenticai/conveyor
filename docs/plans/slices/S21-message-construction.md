# S21 — Message construction (the compiled prompt)

Programme project: S21 · issues #25

**Status:** `plan written 2026-11-09`; the v1 seam is built (D27), the 2026-2027 design is this document.
**Depends on:** S15 (capability/knowledge), S16 (prompt audit), S13 (agent.run), S11 (procedure steps),
S09 (`retrieval.query` / `strata` packs).
**Owner ruling:** George, 2026-11-09 — *a prompt is a run-time compilation from the procedure and the
other system/tenant/project sources, stored in Postgres; prompt-engineering is 2024, context-engineering
2025.*

## 1. What is already true (the seam this slice extends)

- `prompt` + `prompt_section` (`migrations/0030_prompt_catalogue.sql`): a **recipe**, one active version
  per `(tenant, key)`, exact-pin resolution by `<key>@<version>`.
- `conveyor_domain::prompt::compile`: pure renderer over `PromptFacts`; sections are `system`,
  `procedure`, `capability`, `project_card`, `knowledge`; a required section with no source refuses by
  name; the compiled text's SHA-256 is the provenance key.
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

Source: the run's `procedure`/`workflow` and its `run_step` (position, action, gate, `on_error`), plus
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
3. **Render** — the pure renderer, sections in trust order: instructions → procedure → capability/role
   → knowledge → project/card. Untrusted text never precedes the instruction block.
4. **Budget** — a token ceiling per section and per prompt; over-budget is a *refusal with the
   measurement*, never silent truncation (the same rule as the loop caps).
5. **Hash and record** — SHA-256 of the compiled text into `prompt_sha256`, with the source versions in
   the hash's pre-image so a past call is reproducible.
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

**Measured (rbuild on m1, 2026-11-09).** `conveyor-domain` prompt **6/6**; `conveyor-store` lib
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

**Remaining for S21.** The `plan` projection (§6), tenant-authored recipes (open question 4: an edit
is a new version, and the CRUD surface + its audit gate are the gate), and memory selection (open
question 3) — none of them built here.

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
stored. (3) memory selection deferred; its shape is selection → processing → `prompt.memory`.
(4) tenant-authored recipes remain deferred.

**Measured (rbuild on m1, 2026-11-09).** `conveyor-domain` plan **14/14**; `conveyor-store`
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

**Measured after the second audit (rbuild on m1, 2026-11-09).** Domain plan **14/14**, actions
`procedure` **12/12**.

**Served-spec lock.** Regenerated over the whole tree: **287 → 297 operations**, `--check-lock` exit
0. The full list of added/changed operations is in `docs/evidence/S21-PLAN.md`.
