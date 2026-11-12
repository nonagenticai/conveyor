# S21 step 1 — budgeted compile + the role component — evidence

Measured 2026-11-09 on mini3; every Rust command through `rbuild` on m1 (CONTRACT §16).
Lane `s21-message-construction`, joining the D27 seam that `s13-agent-inference-nodes` built.

## Built

- `migrations/0031_s21_prompt_budget.sql` — `prompt.budget_chars` and `prompt_section.budget_chars`
  (nullable = unbounded, `> 0` by CHECK) and `'role'` added to `prompt_section.source_kind`.
  Grants are re-applied by `migrate` (0030's rule); no grant block.
- `crates/domain/src/prompt.rs` — `PromptSource::Role`; `PromptSection.budget_chars` and
  `PromptDefinition.budget_chars`; `CapabilityFacts.intended_role`; two refusals,
  `SectionOverBudget { kind, measured, limit }` and `PromptOverBudget { measured, limit }`.
  `compile()` stable-sorts sections by **trust rank** (instructions → procedure →
  capability/role → knowledge → project/card), measures each rendered block (heading included,
  `.chars().count()`) against its section ceiling, measures the joined text against the prompt
  ceiling, and only then hashes. **Nothing is truncated**: both errors carry the measured size, and
  `budget = measured` compiles while `measured - 1` refuses.
- `crates/store/src/prompt.rs` — the recipe read carries both budget columns; `read_capability`
  reads `capability_agent.params` and exposes `params->>'role'` as `intended_role` (empty when the
  binding declares none). `params` is existing editable config on `capability_agents.create|update`,
  so no action schema and no served-spec hash moves.
- `crates/store/src/seed.rs` — `agent@1` gains the optional `role` section and is ordered
  `system, procedure, capability, role, knowledge, project_card`. Additive: an existing tenant's
  row is never rewritten.
- `crates/actions/src/prompt/mod.rs` — `PromptError::BudgetExceeded { reference, section, measured,
  limit }`, mapped to `409 capability_not_ready` with field `prompt_ref`, code
  `prompt_over_budget`, and a detail naming the section (or the whole prompt) and both numbers.
- No new action, scope, route, event or table. The served spec and `served-spec.lock.json` are
  untouched (the last settling lane writes the lock).

## Evidence

| command | result |
|---|---|
| `rbuild -- test -p conveyor-domain prompt::` | **6 passed / 0 failed** — every source renders in trust order; a missing required source refuses by name; a required role with no declared role refuses; the section budget boundary; the prompt budget boundary; byte-identical hashing |
| `rbuild -- test -p conveyor-store --lib` | **37 passed / 0 failed** — including `embedded_migrations_are_exactly_the_s0_set`, now **1…31** |
| `rbuild -- test -p conveyor-store --test prompt_catalogue` | **4 passed / 0 failed** — the seeded `agent@1` lists the six kinds in trust order; the budget round-trips; a `budget_chars = 0` insert is refused by `prompt_budget_chars_positive` / `prompt_section_budget_chars_positive` |
| `rbuild -- test -p conveyor-actions --test prompt_compile` | **4 passed / 0 failed** — the two original refusals, an over-budget whole-prompt recipe through `compile_for_audit` with its `409 prompt_over_budget` mapping (at the measured size it compiles), and a section-budget recipe proving the refusal names the section and the detail says "the system section" |
| `rbuild -- test -p conveyor-actions --test procedure_agent_resolved` | **1 passed / 0 failed** — a real `procedure.step` → `agent.run` compile; the binding's `params = {"role": "resolver"}` renders `## Role\nrole: resolver`; the ledger still records `agent@1` + a 64-char hash |
| `rbuild -- clippy -p conveyor-domain -p conveyor-store -p conveyor-actions --all-targets -- -D warnings` | exit **0** |
| `rbuild -- fmt -p conveyor-domain -p conveyor-store -p conveyor-actions -- --check` | clean for every file this lane touched; the remaining diffs are peer files (`eval/**`, `parity_http_mcp.rs`, `registry.rs`, `n8n_migration.rs`, and `seed.rs`'s new tenancy/benchmark blocks) |

## Reversal proofs (each fired, then restored)

1. **Section budget removed** (`&& false` on the ceiling check) →
   `an_over_budget_section_refuses_with_its_measured_size_never_truncates` **FAILED**.
2. **Prompt budget removed** → `an_over_budget_prompt_refuses_with_the_measured_total_never_truncates`
   **FAILED**.
3. **Role render arm deleted** (`PromptSource::Role => None`) →
   `every_source_renders_into_its_own_labelled_block_in_trust_order` **FAILED** (the required-role
   refusal still passes, correctly: the source is absent either way).
4. **Trust sort removed** → the same trust-order test **FAILED** (the recipe lists project/card
   before role and knowledge).

## Independent audit

A fresh `validator-deep` that wrote none of this returned **YES** across the six claims (domain
compile/trust order/budget/role, store round-trip + CHECKs, actions mapping, clippy). Its two
non-blocking findings were closed before handoff:

- the heading-inclusive measurement was not pinned by a mutation, so a future refactor could have
  measured only the body and silently loosened every section budget. The domain test now asserts
  `measured` byte-for-byte against `body + "## Project and card\n"`;
- the `Some(section)` arm of the action mapping was unasserted. `prompt_compile` now has a
  section-budget case that proves `section == Some(System)` and the detail names the section.

Re-measured after both fixes: domain prompt **6/6**, `prompt_compile` **4/4**, clippy clean.

## Found on the way

`crates/store/src/migrate.rs`'s `embedded_migrations_are_exactly_the_s0_set` expected the embedded
set to end at **29** while `0030_prompt_catalogue.sql` was already on disk and embedded — a red test
in the shared tree since the prompt-catalogue session updated `s07_schema.rs` only. Both ledger
lists now carry **30** and **31**.

## Remaining for S21 (not built here)

- the `plan` projection over the step DSL (§6) — a later step by the slice's own words;
- tenant-authored recipes (open question 4) — needs a CRUD surface and its audit gate;
- memory selection (open question 3) — `PromptFacts` carries no recalled items yet.
