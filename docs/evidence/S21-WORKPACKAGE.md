# S21 step 3 — the work-package prompt envelope (issue #15)

Date: 2026-11-22 · Branch: `issue/15-s21-workpackage-prompt` · Baseline: `origin/main@490ad555`
Design: S21 §12 · ADR `docs/decisions/0009-prompt-selection-and-recipe-authoring.md`.

The executor now compiles a work package's prompt from the run's process, capability/role, the
card's memory, and the capability's pinned knowledge content; a tenant can author and activate the
recipe that names those sources. The compiled text is still never stored, and laya still consumes
its own typed judge document rather than this prompt.

## What landed

| Part | Change |
|---|---|
| P1 recipe authoring | Migration `0062_s21_prompt_selection.sql`: `draft` status, `uq_prompt_draft`, `prompt_section.selector`, `memory` source kind. Actions `prompt_recipes.create\|get\|list\|activate\|retire`; scopes `prompts:read` (open) / `prompts:write` (admin). Activation retires the current active version in one transaction; a race refuses with the unique active index. |
| P2 memory selection | `PromptSource::Memory`; `MemorySelector {kinds, limit 1–32 default 8, include_digest}`; the card's newest un-consolidated hot rows first, then one digest only if room remains; rendered oldest-first as `[kind/source#id] body`; ids are in the hash. `agent@1` gains an optional memory section for new tenants. |
| P3 knowledge content | `crates/actions/src/knowledge/retrieval.rs` splits target resolution (DB) from retrieval (network). `compile_for_run` resolves recipe/facts/memory/targets in one transaction, **commits**, retrieves through the same path `knowledge.query` uses, then compiles. No network call runs inside a compile transaction; no second retrieval implementation. `KnowledgeSelector {query, top_k 1–32 default 8}`; a missing query falls back to the card name + description. |
| Laya | Untouched. The judge's `{model, state, questions}` document and the decision ladder remain outside the compiler. |

## Measured at the branch head

The shared Cargo target was contaminated by another worktree's build (a foreign `conveyor-domain`
appeared in the diagnostics), so these were run in an isolated copy of this exact branch with a
private target directory. The PR CI is the final gate and runs the same commands from a clean tree.

| Scenario | Command | Result |
|---|---|---|
| P1 lifecycle + race + scope + audit | `cargo test -p conveyor-actions --test prompt_recipes` | **7/7** |
| P2 rendering, selection identity, determinism, selector bounds | `cargo test -p conveyor-domain --test prompt_memory` | **5/5** |
| P2 store selection (hot first, digest, kinds, bounds, consolidation) | `cargo test -p conveyor-store --test memory the_prompt_selection` | **1/1** |
| P3 compile over a real run, pack, memory and card | `cargo test -p conveyor-actions --test prompt_knowledge` | **4/4** |
| Existing prompt surfaces | `cargo test -p conveyor-actions --test prompt_compile` | **7/7** |
| Existing executor compile | `cargo test -p conveyor-actions --test process_agent_resolved` | **4/4** |
| Recipe store + seed | `cargo test -p conveyor-store --test prompt_catalogue` | **4/4** |
| Migration ledger through 62 | `cargo test -p conveyor-store --test s07_schema` | **3/3** |
| Registry, dump and count | `cargo test -p conveyor-actions --test registry` | **12/12** |
| Registry-wide HTTP/MCP parity | `cargo test -p conveyor-actions --test parity_http_mcp every_registered_action_returns_the_same_body_over_http_and_mcp` | **1/1** (25 s) |

The first P1 run exposed a real defect: `0062` re-added the section-kind CHECK with the pre-0035
`procedure` spelling, dropping `process`, so every prompt seed failed. The check now lists
`system, process, capability, role, project_card, knowledge, memory`, and `prompt_catalogue` is
green.

## Reversal proofs

| Delete | Test that turns red |
|---|---|
| The `PromptSource::Memory` render arm (return `None`) | `prompt_memory` **2 red**: the memory rows disappear and a changed selection no longer changes the hash |
| The `AlreadyActive` guard in `activate_prompt_tx` | `prompt_recipes` race test red: both activations answered `200` instead of `[200, 409]` |
| `facts.knowledge.hits = knowledge_hits(&retrieved.hits)` | `prompt_knowledge` red: the knowledge section rendered the pack identity but no retrieved hit content |

## Generated artifacts

- `docs/design/data/actions.json` — **508** actions (503 + the five `prompt_recipes.*`).
- `docs/design/data/coverage.json` + `seed/programme.json` — S21 measures **built** (5/5 declared
  actions, 2/2 tables, 3/3 modules).
- `docs/design/docs/**` — the 113-source document pack regenerated; only the edited documents move.
- `served-spec.lock.json` — 539 operations. The refresh also absorbed **12 operations and 20
  hashes already missing from the committed lock** (`#508`/`#511`); the drift predates this branch.
  Verified with `conveyor --print-openapi --check-lock` (the bare `--check-lock` tries to open a
  database first).

## Out of scope

The V2 `internal_agent` executor runtime and its dynamic tool grant remain the next lane; T0 built
the `Executor` contract only. No laya change, no PDDL storage, no second compiler, no duplicate
`knowledge.query`, no recipe-authoring UI.
