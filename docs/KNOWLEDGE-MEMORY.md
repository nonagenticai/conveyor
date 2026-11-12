# Capability, knowledge and memory — design

**Date:** 2026-11-08 · **Status:** design for review · implements SPEC **D20**
**Related:** `SPEC.md` §6, §8 S15; `plans/slices/S15-capability-knowledge-memory.md`; `plans/02-DATA-ARCHITECTURE.md`;
`plans/slices/S09-documents-integrations.md` §5.6 (strata).

---

## 1. Why this document exists

The conveyor has a capability vocabulary (`catalogue_entry(kind='capability')`, 17 seeded rows) and a project
capability ledger planned in S08, but no design for what a capability *is*, what executes it, or what it knows and
remembers. "Capability agent" appears nowhere in the codebase; "memory" and "ontology" appear nowhere. The
previous program's capability design (C0–C5 maturity, 13 TIM capabilities) predates the rewrite and is not
carried.

This document defines the four things the rewrite needs, in the owner's own split:

- **knowledge**: *procedural* (what to do, in what order, under which gates) and *domain* (what is true about the
  work);
- **memory**: *procedural* (what actually happened) and *agent work-package* (what the agent learned while
  working one card).

## 2. Vocabulary

| term | meaning | where it lives |
|---|---|---|
| **Capability** | a named unit of ability: what the organisation can do (design, build, test, govern, operate). It is an aggregate, not a label. | `capability` |
| **Capability agent** | the configured executor of a capability: model, prompt, parameters, permitted actions, limits, and the knowledge/memory bindings it may use. | `capability_agent` |
| **Work package** | the unit an agent works on: **the card** in its list, plus the runs it spawns. The card id is the work-package id — no second identity. | `card` |
| **Procedure** | executable documentation for one path through a capability: ordered steps, each an action plus an optional gate. | `procedure`, `procedure_step` (S11) |
| **Capability maturity** | a computed grade (C0–C5) from evidence, never a stored opinion. | computed from execution history |

## 3. The model

| | **knowledge** | **memory** |
|---|---|---|
| **procedural** | **the ontology**: `capability → procedure → step → action \| gate`. It says what can be done, in what order, under which gate. | **execution history**: `run`, `approval`, `action_invocation`, `event`. It says what was actually done, by whom, with what outcome. |
| **domain** | **capability packs** in `strata_rag`: versioned, published, provenance-bearing documents about the domain a capability works in. | **work-package memory**: hot, transactional rows in the conveyor; cold, consolidated digests in `strata_rag`. |

The diagonal is the point: procedural knowledge is written by people and published; procedural memory is written by
the system and never edited; domain knowledge is curated and versioned; work-package memory is ephemeral and
per-card.

## 4. Capability as a first-class aggregate

`catalogue_entry(kind='capability')` is deleted and replaced by `capability` — the same reason `card_type` is a
table while 14 RM catalogues folded into `catalogue_entry`: it carries executable definition and is an FK target.

```
capability            id, tenant_id, code (natural key), label, description,
                      status ∈ draft|active|retired, maturity_note, meta
capability_relation   from_capability_id, to_capability_id, kind ∈ requires|enables|concern
```

`code` is the stable key. S08's planned `project_capability.capability_code` remains a text reference to it (S08 is
not built; when it is, it may promote to an FK).

**The ontology** is the directed graph `capability → procedure → step → action|gate`, plus `capability_relation`
between capabilities. A procedure may belong to a capability directly (`procedure.capability_id`) or through its
card type (`card_type.capability_id`). The step's `action` must exist in the action registry and its `gate_kind` in
`catalogue_entry(kind='gate')` — the same validation S11 already applies.

## 5. Capability agent

```
capability_agent   id, tenant_id, capability_id, key, status ∈ draft|active|retired,
                   model_ref (inference-router model), prompt_ref (<name>@<version>, built-in in v1),
                   params jsonb, allowed_actions text[], max_iterations ≤20, timeout_secs ≤300
```

It is **configuration**, not a loop (SPEC D22, CONTRACT §17). A `run` records `capability_id` and
`capability_agent_id`; the agent's `allowed_actions` become the run's scope (an action outside the list is refused
before execution, not after); the knowledge and memory bindings resolve through the capability. The loop that reads
this config is S13's `agent.run`: the run wiring stores `run.capability_agent_id`, the step executor resolves the row
into the step input, and the action is a pure loop; S10's copilot is an interactive surface and does not use it. The
model itself is never
local: the conveyor calls `inference-router` (SPEC §5.2), it does not host weights. The resolved
`{capability_agent_id, prompt_ref, model_ref}` travels in the `agent.run` step input, so the invocation hash pins
what ran.

## 6. Domain knowledge — `strata_rag` platform packs

**What it is.** A *pack* is an immutable, numbered version (`key@version`) of one platform-owner root store.
Draft → published → retired; published is frozen; a new version is a new root. Tenant documents shadow platform
ones through `supersedes`; provenance and a licence allow-list are enforced at ingest. This is a ready-made domain
knowledge layer; the conveyor binds to it rather than rebuilding it.

**Shape.** One pack, key `capability`, versioned as a whole. Each document carries `metadata.capability = <code>`
and its `provenance`. One pack, not one per capability: a pack is one root store and versions are immutable, so
per-capability packs multiply roots and version churn. Retrieval filters by `metadata` in SQL before rerank.

```
capability_knowledge   capability_id, pack_key, pack_version, min_score, required boolean
```

**Retrieval.** `knowledge.query {capability_id, query, top_k?}` resolves
`capability_knowledge → pack key/version → leaf ids` (cached; `GET /api/platform/packs/{key}/{version}/stores`),
then calls `POST /api/query` with `leaf_ids`, `metadata: {capability: code}`, `top_k`, `min_score`. Results carry
`layer`, `pack`, `text`, `title`, `source`, pages — the conveyor returns them unchanged and does not re-rank.

**Degradation.** If a `required` pack is unreachable or its pinned version is missing, the run fails
`knowledge_degraded`; an optional pack degrades to no hits with a recorded warning. Never silently answer from
nothing.

## 7. Procedural memory — execution history

No new table. `run`, `approval`, `approval_decision`, `action_invocation` and `event` already record who did what,
with which input hash and outcome. Capability maturity is a **computed projection** over that evidence (success
rate, retries, approval latency, refusal rate), not a stored opinion; it is computed on read and cached only if a
measurement shows it is needed.

This is also the input to the improvement loop: a capability whose runs fail at one step is evidenced by
`action_invocation` rows for that action, not by a narrative.

## 8. Work-package memory

**Hot (system of record).** Rows written in the run's own transaction:

```
work_package_memory   id, tenant_id, project_id, card_id, run_id?, capability_id?,
                      kind ∈ fact|attempt|decision|blocker|digest,
                      source ∈ agent|human|system,
                      body text, supersedes_id?, consolidated_at?, meta jsonb
```

`memory.remember` writes one row and emits `memory.recorded`. Hot rows are never deleted while the card exists;
they are the record, and a digest is derived from them.

**Cold (semantic recall).** The `memory.consolidate` job builds one digest per work package from its un-consolidated
rows and writes it to `strata_rag` as one document:

| field | value |
|---|---|
| store | the tenant's root store (one per tenant) |
| `source_id` | `conveyor:work:<card_id>` |
| `source_run` | the consolidation id |
| `metadata` | `{project_id, card_id, run_ids[], capability, last_consolidated_at}` |
| body | a fixed header line `work-package: <card_id>` followed by the digest |

⚠️ **The header line is load-bearing.** `strata_rag` deduplicates identical normalized text across sources: the
second source gets the *first* source's document, and pruning the first source deletes it. Short, templated card
notes make that collision likely; a card-id header makes every digest unique.

Re-consolidation follows strata's immutable-document model: ingest the new digest under a new `source_run`, then
`DELETE /api/sources/conveyor:work:<card_id>/documents?except_run=<new>`. The empty-run guard is left on.

**Recall.** `memory.recall {card_id, query?, limit?}` returns hot rows (structured, newest first) and, when a query
is given, semantic hits from `strata_rag` filtered by `source_ids: ["conveyor:work:<card_id>"]`. Hot always wins on
conflict: the digest is derived.

**When it is read.** At run start (the card's memory and the capability's domain knowledge are loaded into the
run's context) and at each step that declares `memory: read` in its procedure.

**Who writes it.** `memory.remember` is the only **action** that writes; the `memory.consolidate` job appends one
`kind='digest'` row and stamps `consolidated_at`. A copilot turn or an `agent.run` step records what it learned
through `memory.remember`, never by SQL; hot rows are the record and the digest is derived.

## 9. The binding chain

```
card → card_type → capability → { capability_knowledge → strata pack }
                              → { capability_agent → allowed_actions, model, limits }
run  → capability_id + capability_agent_id  (what actually ran, not what was default)
work_package_memory → card_id (the work package)
```

`card_type.capability_id` is the default; `run.capability_id` records the actual. A run may name a capability
different from its card type's default (a split, a repair), and the record shows it.

## 10. Actions, events, jobs

| kind | name | note |
|---|---|---|
| action | `capabilities.list`, `capabilities.get` | read |
| action | `capabilities.bind_knowledge` | emits `knowledge.bound`; admin scope |
| action | `capability_agents.create|update|list|get` | admin scope |
| action | `knowledge.query` | retrieval only — **no** `knowledge.answer` |
| action | `memory.remember`, `memory.recall` | hot write, hot+cold read |
| job | `memory.consolidate` | `dedupe_key = (card_id, last_row_id)`; idempotent |
| event | `memory.recorded` | produced by `memory.remember` |
| event | `knowledge.bound` | produced by `capabilities.bind_knowledge` |

`knowledge.ingest` is **not** added: S09's `documents.ingest` already delegates to `strata_ingest`, and that is the
only conversion path.

## 11. What this design does not do

- It does not host embeddings, a vector store, or a model. Retrieval is `strata_rag`; generation is the executor
  (the agent's `knowledge.answer`, or the conveyor copilot in S10).
- It does not copy the agent's knowledge plane. `agent` already queries strata with pinned packs; the conveyor owns
  the **binding** and exposes it, so the binding has one home.
- It does not make `strata` the system of record for anything. Memory rows and execution history are the record.
- It does not revive KAG (archived 2026-11-04).
- It does not build a model loop. `capability_agent` is configuration; the loop is S13's `agent.run` (D22), and
  S10's copilot is an interactive surface, not a work executor.

## 12. Open questions

| # | question | default |
|---|---|---|
| 1 | Who curates the `capability` pack? | The platform-owner tenant; the conveyor's platform admin acts as curator |
| 2 | Is one pack per capability ever right? | No by default; revisit only if a pack becomes large enough that versioning the whole set is a burden |
| 3 | Retention for hot memory? | Keep while the card exists; the digest is rebuildable, the rows are not |
| 4 | Does a human edit a digest? | No — digests are derived; edit the hot row and re-consolidate |
| 5 | Maturity C0–C5 mapping? | Computed from execution evidence in S15; the thresholds are a measurement, not a guess |
