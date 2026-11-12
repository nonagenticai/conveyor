---
slice: S15
title: Capability, knowledge and memory
status: planned
depends_on: [S03, S04, S06, S11]
spec_sections: ["§3 D20", "§6", "§8 S15"]
adds_tables: [capability, capability_agent, capability_knowledge, capability_relation, work_package_memory]
adds_actions: [capabilities.list, capabilities.get, capabilities.bind_knowledge, capability_agents.create, capability_agents.update, capability_agents.list, capability_agents.get, knowledge.query, memory.remember, memory.recall]
adds_events: [memory.recorded, knowledge.bound]
adds_modules: [capability, memory, knowledge]
parity_ids: PENDING-S0
---

⚠️ **Amended 2026-11-09 (process model).** `capability` gains `kind ∈ system|tenant` — `project_management`, `kanban_coach` and `kata` are the fixed system capabilities (always present, not deletable); every other capability is tenant-defined per project type. `capability_agent` gains `role ∈ process_agent|tool_agent`, `process_key text null` (the process/card type a process agent owns) and `toolset text null` (the toolset a tool agent executes), with a CHECK that exactly one of the two is set per role — migration `0033`. A capability is a knowledge area: its process agents hold the process domain knowledge, its tool agents make the tool calls. Run readiness resolves the run's **process agent**; the tool agent's `allowed_actions` is the exact set it executes.

# S15 — Capability, knowledge and memory

Programme project: S15 · issues #19

Design authority: `docs/KNOWLEDGE-MEMORY.md`. This slice builds it.

## 1. Objective

A capability becomes an aggregate with a **persisted executor spec**, a versioned domain-knowledge binding and two
memories. **S15 builds the spec and its resolution, not a model loop**: `capability_agent` is consumed by S13's
`agent.run` (D22, CONTRACT §17). At the end, a card's run resolves its capability and executor, the capability's
readiness is reported before the run starts, a cited passage is retrieved from a pinned `strata` pack,
work-package memory is written, consolidated into `strata_rag`, and recalled by `source_id`.

## 2. Starting state

- S03: `card_type`, `card`, `run`, `catalogue_entry` (including `kind='capability'`, 17 seeded rows), `approval`.
- S06: runs and approvals; S11: `procedure`, `procedure_step` and the action/gate validation.
- S09: `documents.ingest` → `strata_ingest`; `retrieval.query` is an external action. Neither is a capability
  binding.

## 3. Scope

**IN** — the five tables, the columns on `card_type`/`run`/`procedure`, the ten actions, two events, one job kind,
the `strata_rag` client (query + document write + source prune), and the migration that removes
`catalogue_entry(kind='capability')`.

**OUT** — **the model loop**: `capability_agent` is configuration (CONTRACT §17); the loop is S13's `agent.run`
and S10's copilot is the interactive surface, so S15 builds neither. Also out: embeddings, model hosting, answer
generation (`knowledge.answer` stays the executor's); a tenant-authored prompt store (v1 resolves built-in
`prompt_ref` only); KAG (archived 2026-11-04); the copilot (S10); S08's `project_capability` ledger — ⚠️ **dropped
by migration 0011** on the S00 day-2 verdict (0 rows, no consumer), so S15's `capability` aggregate has no
predecessor to reconcile with.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `strata_rag/README.md` | Tenants, Query filters, Document sources, Platform knowledge, Migrations | the request/response shapes and the precedence rules | the CLI |
| `strata_rag/src/{query.rs,platform.rs,api/handlers.rs}` | `QueryBody`, pack keys, document ingest fields | field names and limits | Rust types |
| `strata_ingest/README.md` | API, job states, re-ingest copy | why ingestion is not reimplemented | the job model |
| `crates/actions/src/integrations/external.rs` (S09) | the external-call shape, breaker | timeout/breaker conventions | OpenAPI ingestion for a typed client |
| `crates/store/src/approval.rs` | `insert_approval_tx` | the nullable-`run_id` pattern | — |

## 5. Deliverables

`02-DATA-ARCHITECTURE §2.15` carries the DDL; the summary is:

| table | key fields |
|---|---|
| `capability` | `code` unique per tenant, `status ∈ draft\|active\|retired`, `label`, `description`, `meta` |
| `capability_agent` | `capability_id`, `key`, `model_ref`, `prompt_ref` (`<name>@<version>`, built-in in v1), `params`, `allowed_actions text[]`, `max_iterations ≤20`, `timeout_secs ≤300`. **`prompt_ref` is also the prompt audit's provenance key** (S16): the audit reads the same `<name>@<version>` that the executor resolves, and an `inference_call` row records it, so a finding and the calls it explains name the same prompt version |
| `capability_knowledge` | `capability_id`, `pack_key`, `pack_version`, `min_score`, `required` |
| `capability_relation` | `from_capability_id`, `to_capability_id`, `kind ∈ requires\|enables\|concern` |
| `work_package_memory` | `project_id`, `card_id`, `run_id?`, `capability_id?`, `kind`, `source`, `body`, `supersedes_id?`, `consolidated_at?` |

Columns: `card_type.capability_id`, `run.capability_id`, `run.capability_agent_id`, `procedure.capability_id`.
Every table carries `tenant_id` and RLS (CONTRACT §2). One migration, **`0020_capability.sql`** — the plan named
`0014`, which the webhook seal took. ⚠️ **S11 already applied `0021_s11_run_capability` (2026-11-09): it adds
`run.capability_id`/`run.capability_agent_id` as forward references with no FK. Do not re-add them in 0020** — 0020
creates the capability tables and adds the FKs those two columns point at, plus `card_type.capability_id` and
`procedure.capability_id`. Re-check `migrations/` at start and take the next free number.

**`capability_agent` is configuration, not a loop** (CONTRACT §17). S15 validates at write: caps and a resolvable
built-in `prompt_ref`; `allowed_actions` is checked at **readiness**, not at write, so a capability may name
`agent.run` before S13 registers it (an unregistered entry is a `blocking[]` reason, never a write refusal). S15
fills S11's start seam: `resolve_start_context` returns the resolved `StartContext::Resolved{..}` (so
`procedure_runs.start` records `run.capability_id`/`run.capability_agent_id`), `ensure_start_ready` runs the readiness
checks and returns `409 capability_not_ready` with `blocking[]` before any run/step row is written, and the step
executor resolves the recorded row into the `agent.run` step input — which is the pure loop S13 ships. A run with an **external** executor has no `capability_agent`: it is recorded by its
principal, `run.external_ref` (the MCP task id) and `approval_decision.decided_by_kind`/`via`.

```
crates/domain/src/capability.rs             aggregate, relations, maturity projection
crates/domain/src/memory.rs                 memory kinds, supersession, digest rules
crates/store/src/capability.rs              queries; the catalogue migration
crates/store/src/memory.rs                  hot rows, consolidation query
crates/actions/src/capability/*.rs          ten actions
crates/api/src/knowledge/mod.rs             strata client: query, document, prune
crates/store/src/jobs/memory_consolidate.rs the job handler
crates/store/src/seed.rs                    import capability + capability_relation (CATALOGUE_EXPORT)
seed/catalogue.json                         remove the capability rows; seed/capability.json holds them
migrations/0020_capability.sql
```

## 6. Build order

1. `0020`: five tables, four columns, `catalogue_entry(kind='capability')` migrated into `capability` then deleted.
   `cargo run -q -p conveyor-server -- migrate` twice.
2. Seed the 17 capabilities and their relations from `seed/capability.json`; remove the capability rows from
   `seed/catalogue.json` and make `crates/store/src/seed.rs` import both. `cargo test -p conveyor-store capability_seed`.
3. Domain: status transitions, relation cycle refusal, allowed-action validation. `cargo test -p conveyor-domain capability`.
4. Store: CRUD + the binding query (`capability → packs`, `card_type → capability`). `cargo test -p conveyor-store capability`.
5. Actions: `capabilities.*`, `capability_agents.*`, `capabilities.bind_knowledge` (+ `knowledge.bound`).
   `cargo test -p conveyor-actions capability`.
6. `strata` client: `POST /api/query`, `POST /api/stores/{id}/documents`, `DELETE /api/sources/{sid}/documents`.
   Timeouts and a breaker as in S09; no retry on 4xx. `cargo test -p conveyor-api knowledge_client`.
7. `knowledge.query`: resolve pack → leaf ids (cache), filter `metadata.capability`, return hits unchanged.
   `cargo test -p conveyor-api --test knowledge_query`.
8. `memory.remember` + `memory.recorded`; hot read. `cargo test -p conveyor-actions memory`.
9. `memory.recall`: hot + semantic (`source_ids = ["conveyor:work:<card>"]`), hot wins.
   `cargo test -p conveyor-api --test memory_recall`.
10. `memory.consolidate`: build the digest with the `work-package: <card_id>` header, ingest with a new
    `source_run`, prune `except_run`, set `consolidated_at`. Idempotent on replay.
    `cargo test -p conveyor-store memory_consolidate`.
11. Wire `run.capability_id`/`capability_agent_id` at run creation; scope the run's actions from
    `allowed_actions`; carry `{capability_agent_id, prompt_ref, model_ref}` into the `agent.run` step input
    contract (S13 registers the action and owns the loop). `cargo test -p conveyor-actions run_capability`.
12. Readiness: `capabilities.get` returns `{ready, blocking[]}` and `procedure_runs.start` refuses on the same
    projection. `capability_agents.create|update` refuse caps above S13's ceiling (and the `prompt_ref` grammar);
    an unresolvable-but-well-formed `prompt_ref` and an unregistered `allowed_actions` entry are **readiness**
    blockers, not write errors (so `agent.run` may be named before S13 lands, and a prompt pin is checked against
    the tenant's catalogue). The start guard refuses `409 capability_not_ready` before the first step.
    `cargo test -p conveyor-api --test capability_readiness`.
13. Gate (§7).

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env
# a real strata_rag and its Postgres, not a mock
docker compose --profile strata up -d --wait
cargo run -q -p conveyor-server -- migrate
cargo test -p conveyor-domain capability
cargo test -p conveyor-store capability
cargo test -p conveyor-store memory_consolidate
cargo test -p conveyor-actions capability memory
cargo test -p conveyor-api --test knowledge_query
cargo test -p conveyor-api --test capability_readiness
cargo test -p conveyor-api --test memory_recall
cargo clippy --workspace --all-targets -- -D warnings
# the removed vocabulary has one home
test -z "$(grep -rn "kind = 'capability'" crates/ migrations/ | grep -v capability.rs)"
# no embedding code in the conveyor
test -z "$(grep -rn "pgvector\|hnsw\|embedding" crates/ migrations/ | grep -v '^Binary')"
```

| # | property | proof |
|---|---|---|
| a | domain knowledge | a capability bound to a published pack returns a hit carrying `pack` and `layer` |
| b | pinned version | moving the pack version does not change the pinned capability's hits |
| c | degradation | a `required` pack that is unreachable fails the run `knowledge_degraded`; an optional one warns |
| d | hot memory | `memory.remember` is transactional with the run and visible to `memory.recall` |
| e | cold memory | after consolidation, the digest is retrievable by `source_ids` and carries the card header |
| f | dedupe | two cards with identical note bodies produce two documents, each with its own `source_id` |
| g | idempotence | replaying `memory.consolidate` writes no second document for the same rows |
| h | binding | a card's run resolves `capability_id` and its agent's `allowed_actions`; an action outside them is refused |
| i | no second vector store | the grep in the gate is empty |
| j | readiness blocks before the run | a capability whose `required` pack pins a missing version returns `ready:false` with that pack in `blocking[]`, and `procedure_runs.start` is `409 capability_not_ready` with **zero** `run_step` rows; an unregistered `allowed_actions` entry (e.g. `agent.run` before S13) is in `blocking[]`, not a write error; a capability with a missing pack at **run time** is `knowledge_degraded` |
| k | caps and the prompt are validated at write | `capability_agents.create` with `max_iterations=21` or `timeout_secs=301` is `422`; a malformed `prompt_ref` (not `<name>@<version>`) is `422`; a **well-formed but unresolvable** `prompt_ref` is a readiness blocker (`prompt_unresolved`), not a write error — amended 2026-11-09 against CONTRACT §17 and §12's build record (the catalogue is S13's `0030`) |
| l | the executor config is the only source | the `agent.run` step input carries `{capability_agent_id, prompt_ref, model_ref}` resolved from the row; no handler reads a model or prompt from anywhere else (grep + test) |

## 8. Traps

1. **Cross-source dedupe in `strata_rag`** — identical text belongs to the first source; pruning that source deletes
   the shared document. The card-id header is the defence.
2. **Immutable documents** — there is no update-in-place; consolidation is ingest-new + prune-old, never PUT.
3. **One document per request, embedded synchronously** — never call `strata` per event; consolidate in a job.
4. **Pack targeting is explicit** — a default query searches only the caller's tree; platform leaves must be named
   in `leaf_ids`.
5. **Null `run_id` and `uq_appr_pending`** — nullable columns make multiple standalone rows legal; do not add an
   index to "fix" it.
6. **`catalogue_entry(kind='capability')` removal** — the seed and any `kind='capability'` lookup must move in the
   same migration, or a card type resolves to nothing.
7. **RLS** — every new table needs `tenant_id` and the policy; a table without it is a cross-tenant leak, not a bug.
8. **A second loop in S15.** `capability_agent` is configuration (CONTRACT §17). If this slice finds itself calling
   a model, it is building S13's `agent.run`; resolve the spec, carry `{capability_agent_id, prompt_ref, model_ref}`
   into the step input, and stop.
9. **A prompt table invented to fill `prompt_ref`.** v1 resolves a built-in `<name>@<version>`; a tenant-authored
   store is a later slice and would put an unversioned text blob behind an admin edit.
10. **Two retrieval paths.** `retrieval.query` (S09) is the raw strata passthrough; `knowledge.query` is the
    capability-bound one and is the **only** one a run uses. Do not add a third, and do not let a capability call
    `retrieval.query` directly.

## 9. Decisions closed here

1. **Capability is a table, not a catalogue kind** — it is an FK target and carries executable definition.
2. **One `capability` pack**, not one per capability — a pack is one root and versions are immutable.
3. **Conveyor owns the binding, strata owns retrieval** — no second knowledge plane in the conveyor.
4. **Hot rows are the record; digests are derived** — `strata` is never the system of record.
5. **Maturity is computed, not stored** — a projection over execution history.
6. **`capability_agent` is the persisted executor spec, not a loop** — the run wiring stores `run.capability_agent_id`;
   the step executor resolves it into the `agent.run` input; S15 proves validation, resolution and recording
   (CONTRACT §17).
7. **`prompt_ref` is a built-in versioned prompt in v1** — `<name>@<version>`, no prompt table; the resolved id
   and model travel in the `agent.run` step input, so the invocation hash pins them.
8. **Readiness is pre-run, not a mid-run warning** — `409 capability_not_ready` with `blocking[]` before the first
   step; `knowledge_degraded` is the runtime case where a pack disappears after start.
9. **`retrieval.query` and `knowledge.query` are deliberately distinct** — the first is a raw passthrough (S09),
   the second is capability-bound and is the only path a run uses.

**Why five tables and not one.** `capability` and `capability_relation` are the aggregate; `capability_agent` is
configuration with its own lifecycle; `capability_knowledge` is a versioned binding that changes on a different
cadence from the capability; `work_package_memory` is the only high-write table and must not share a row with
configuration. Folding them would put a per-run write path behind the same lock as an admin edit.

**CONTRACT-CHANGE REQUEST:** `02-DATA-ARCHITECTURE §2.16` permits no further table without a contract change, and
§8 declares only three forward references into `run`. Proposal: add S15's five tables (count 67 → 72) and two
forward references (`run.capability_id`, `run.capability_agent_id`, added by `0020`). No slice acts until amended.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Is `procedure.capability_id` required? | Nullable; `card_type.capability_id` is the default path |
| 2 | Consolidation cadence? | On run completion, plus a nightly sweep for abandoned cards |
| 3 | Maturity thresholds C0–C5? | Measured in the slice from real run/approval data; recorded, not guessed |
| 4 | One strata store per tenant or per project? | Per tenant; `source_id` carries the work package |
| 5 | Does `knowledge.query` accept raw leaf ids? | No — capability_id only; raw ids are an admin/debug scope |

## 11. Build record — steps 1–4 (2026-11-09)

**Built and gate-green.** `migrations/0020_capability.sql` (five tables, FORCE RLS, the
`card_type`/`procedure` nullable `capability_id` columns, the 17 catalogue rows moved then deleted),
`migrations/0027_run_capability_fks.sql`, `seed/capability.json` (+ the 17 rows removed from
`seed/catalogue.json`), `domain::capability`, `store::capability` (CRUD, agent create/update, pack
binding, relations, the two binding queries, the idempotent seed importer) and `Parent::Capability`.

Evidence: domain `capability` **7/7**; store `--test capability` **6/6** (the five behaviours plus
the typed-refusal test; the four-way RLS test covers all five tables) and `--test s07_seed` **3/3**; `--test s07_schema` **3/3** after the ledger
gained `20` and `27`. Bad input is a **typed store refusal, not a DB CHECK surfacing as a 500**:
blank code/label/key/model_ref, an unknown capability/agent status and `pack_version = 0` are all
`CapabilityStoreError` variants (the S11 lesson, applied before the actions exist). No action was
registered: the action count stays **237** and the served-spec
lock stays **258 ops** (`--check-lock` exit 0). The two new scopes take the scope count **61 → 63**;
no action references them yet, which is why the lock did not move.

### Amendments (CCRs)

1. **The two `run` FKs are migration `0027`, not `0020`.** 0021 owns the columns and runs AFTER
   0020 on a fresh database, so 0020's `ALTER TABLE run ADD CONSTRAINT` failed 42703 (measured).
   The five tables and the `card_type`/`procedure` columns stay in 0020. 0027 is the next free
   number; S20's reserved number moves to 0028.
2. **Owner decisions (2026-11-09):** `procedure.capability_id` is nullable and `card_type` is the
   default path; memory consolidates **on run completion only** (supersedes §10 Q2's nightly sweep);
   one `strata_rag` store per tenant (confirms §10 Q4).
3. **`CatalogueKind::Capability` stays as a parse value, but the importer refuses it**
   (`CatalogueStoreError::CapabilityMoved`, a `422` on `kind`). Removing the enum would change S03's
   fixed eleven-kind contract; refusing the write stops a legacy export re-creating the rows the
   migration deleted.
4. **Step 5 (the seven actions) is not built.** `parity_http_mcp` is red on 13 peer samples (S13's 6
   `workflows.*` + S19-B's 7 `eval_*`) and S19-B's lease still owns that file, so registering seven
   more actions cannot produce a green gate. The store is ready for them; the hand-off names the
   exact work.
5. **The §7 gate line `grep -v capability.rs` is wrong** — it would flag `0020_capability.sql`'s own
   data move. The migration writes `kind IN ('capability')`, so the check passes as written, but the
   next chunk should change the exclusion to `grep -v capability` rather than rely on that spelling.

## 12. Build record — step 5, the seven actions (2026-11-09 16:25)

**Built and gate-green.** `crates/actions/src/capability/{mod,views,capabilities,agents}.rs` and
`crates/actions/tests/capability.rs`:

- `capabilities.list` — keyset page ordered by code, optional `status`, `limit + 1` page probe;
  `capabilities.get` — by id or by code, with its knowledge bindings. Both read, scope
  `capabilities:read`. `{ready, blocking[]}` is a `TODO(s15-runtime)` in the view: readiness reads
  the strata client and the live registry (steps 7 and 12), not this document.
- `capabilities.bind_knowledge` — upsert on `(capability, pack, version)`, `knowledge.bound`
  appended in the same transaction (topic `capability:<id>`, aggregate `capability`).
- `capability_agents.create|update|list|get` — the spec is configuration (§17): caps `1..=20` /
  `1..=300`, `model_ref`, `prompt_ref` grammar and `allowed_actions` grammar/duplicates are refused
  by `domain::capability` as `422`s naming the field; registration of an `allowed_actions` entry
  and `prompt_ref` resolvability stay **readiness** concerns (step 12). `update` is a patch behind
  `If-Match` (`404` missing / `412` stale-with-`current` / `409 illegal_transition`); an explicit
  `prompt_ref: null` clears it, an absent one keeps it (a `deserialize_present` wrapper — plain
  `Option<Option<T>>` collapses `null` into "absent"). `create` defaults to the column's `draft`,
  `max_iterations = 8` (the §17 default) and `timeout_secs = 300`.
- `AGENT_TOOLS` gains the **four reads only** (36 → 40): configuring an executor is not an agent
  action. No migration, no new scope, no new event constant.

Evidence: actions `--test capability` **8/8** (list/get/pagination, bind upsert + event, the 422
matrix, If-Match 404/412/409, viewer read-only, cross-tenant 404 with zero writes, the agent-tool
surface), store `--test capability` **6/6**, actions `--lib` **61**, `registry` **8/8**,
`parity_http_mcp` **4/4 over 259 actions** (HTTP and MCP), `--check-lock` exit 0 at **280 ops**
(sha `6c6906c4…`). Counts: **259 actions (245 internal + 14 external) · 63 scopes · 280 ops** —
the tree also carries S13's `agent.run`/`inference.chat` and the RBAC lane's seven, which this
pass counted but did not build. Two reversal proofs fired: `bind_knowledge`'s scope set to
`capabilities:read` made the viewer test fail (200 vs 403); `MAX_ITERATIONS` 20 → 21 made the
21-cap sample fail **500 vs 422** — the DB CHECK, exactly what the domain refusal prevents.

Two repairs found while building: (1) `seed::catalogue` (the legacy `DEV_TENANTS` loop) no longer
imports capabilities — `seed::seed_catalogue(pool, slug)` is the capability-aware importer, and a
test that seeds through the old path sees an empty `capability` table; (2) the parity fixture's
`prepare` arm for `capability_agents.create` deleted the fixed key through `pg.owner` with no
`app.current_tenant`, so under FORCE RLS it matched **zero rows silently** and the second round's
create would 409 — found and fixed by the RBAC lane during its own parity pass (the arm now runs in
a tenant-scoped transaction). ⚠️ This is the "gates that cannot fail" shape: an `.expect()` on a
DELETE that affects no rows succeeds.

## 13. Build record — step 12, readiness (2026-11-09)

**Built and gate-green.** `crates/actions/src/capability/readiness.rs` is the ONE computation:
`capability_readiness_tx` collects `domain::capability::Readiness` blockers (the capability exists and
is `active`; exactly one active `capability_agent`; that agent's `prompt_ref` is an **active**
catalogue pin; every `allowed_actions` entry is registered; every **required** knowledge pack resolves
to at least one leaf). `readiness_fields` maps the blockers to `FieldError` once, and both surfaces
use it: `capabilities.get` returns `{ready, blocking[]}`, and `procedure_runs.start`'s
`ensure_start_ready` refuses `409 capability_not_ready` with the SAME array in `errors[]`.

- **The empty-leaves defect is fixed.** `pack_leaves` returns `Ok(vec![])` for a published-but-empty
  pack and the old guard treated that as ready. CONTRACT §17's "no resolvable leaves" now blocks
  (`pack_state`, unit-tested, with the empty arm proved by reversal).
- **`prompt_ref` is a readiness concern, not a write refusal** (row k amended above). Resolvability
  reads the tenant's catalogue (`store::prompt::get_prompt_tx`, S13's `0030`): absent **or retired**
  is `prompt_unresolved` — the same code the executor's own refusal emits. `capability_agents.create`
  keeps refusing only the grammar.
- **CCR 1.** CONTRACT §17 names `blocking[]` for both surfaces. The 409 keeps the API-wide `errors[]`
  envelope; the GET names its field `blocking`. Both carry the identical `Vec<FieldError>` produced
  by one function, so there is no second vocabulary to drift.
- Codes changed deliberately (pre-prod is forward-only): `agent_missing` (was `no_active_agent`),
  `action_not_registered` (was `unknown_action`), plus `pack_not_published`, `pack_unavailable`,
  `knowledge_not_configured`, `prompt_unresolved`, `agent_ambiguous`, `capability_missing`,
  `capability_not_active`.
- No new action, scope, event or migration: **266 actions · 40 AGENT_TOOLS** unchanged; the served
  spec's only action drift is `capabilities.get`'s output schema and its 15 s → 30 s budget.

Evidence (measured 2026-11-09, `rbuild` on m1): domain readiness **2/2**; actions `--test capability`
**9/9**, `--test run_capability` **3/3**; api `--test capability_readiness` **3/3** and its live
missing-pack case (real strata, `--ignored`); `registry` **8/8** at `EXPECTED 266`;
`parity_http_mcp` **4/4**; clippy clean on every target this lane touches; fmt clean on its files.
Reversal proofs fired: deleting the `Ok(empty)` arm fails the pack unit test; deleting the
`ActionNotRegistered` branch fails the GET assertion **and** makes `procedure_runs.start` return
**201** instead of 409.

Carried proof closed: the `0020` catalogue→capability move on a pre-existing row — ephemeral
`postgres:18`, `scripts/db-roles.sql` + all migrations, one `catalogue_entry(kind='capability')` row,
then the exact move block: one `capability` row with its label, the description lifted out of `meta`,
status `active`, and **zero** catalogue rows left. Transcript in `docs/evidence/S15-READINESS.md`.

⚠️ Not this lane's red, measured during the gate: workspace `clippy -D warnings` stops on
`crates/actions/tests/approval_expired.rs:87` (`explicit_auto_deref`); crate-wide `fmt --check` stops
on the tenancy lane's invite test; `--check-lock` exits 1 with **changed `capabilities.get`** (this
lane) and **six added `/api/platform|tenants` routes** (the tenancy lane, in flight). The last
settling lane writes the lock: from 287 ops on disk it becomes **293** once those six land.
