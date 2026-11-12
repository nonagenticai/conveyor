---
slice: S07
title: Requirements, catalogues, revisions, quality scoring, search
status: planned
depends_on: [S03, S04]
spec_sections: ["§4", "§6", "§8 S7", "§11 R2"]
adds_tables: [quality_evaluation, principle_conflict, project_request]
adds_actions: ['requirements.create', 'requirements.get', 'requirements.update', 'requirements.transition', 'requirements.list', 'requirements.compare', 'principles.create', 'principles.get', 'principles.update', 'principles.transition', 'principles.list', 'principles.list_conflicts', 'principles.dismiss_conflict', 'decisions.create', 'decisions.get', 'decisions.update', 'decisions.transition', 'decisions.list', 'catalogues.list', 'catalogues.import', 'quality.score', 'search.query', 'trace_links.create', 'trace_links.delete', 'trace_links.list', 'project_requests.create', 'project_requests.get', 'project_requests.update', 'project_requests.list', 'project_requests.approve', 'project_requests.reject', 'project_requests.promote']
adds_events: ['governed_asset.created', 'governed_asset.updated', 'governed_asset.transitioned', 'governed_asset.superseded', 'quality.evaluated', 'catalogue.imported', 'project_request.created', 'project_request.updated', 'project_request.approved', 'project_request.rejected', 'project_request.promoted']
adds_modules: [search]
parity_ids: PENDING-S0
---

# S07 — Requirements, catalogues, revisions, quality scoring, search

Programme project: S07 · issues none yet

<!-- 2026-11-08: `outline` and `outline_node` were built by 0007 and DROPPED by migration 0011 — S00 day 2
measured zero usage (RM toc_trees/toc_nodes/toc_settings 0 rows, fr project_outlines 0 rows, no `toc` operation
in any of the 348 flows, fr `/toc/*` 15 unused + 1 rare). `project_request` is untouched: it has no outline FK. -->

**Gate (SPEC §8 S7):** relevance eval set beats the agreed baseline.

## 1. Objective

RM's requirement surface lands on S03's `governed_asset`/`revision`/`catalogue_entry`/`trace_link`, not RM's 57 domain tables: one lifecycle for requirements, principles and ADRs, one `revision` per mutation with a working `compare`, read-only quality scoring, and FTS + trigram search.

## 2. Starting state

- S03: `governed_asset` (+ generated `tsv`, `gin(tsv)`), `revision`, `catalogue_entry`, `trace_link`, `card_type`, `project`, `card.tsv`.
- S04: registry, `/api/actions`, OpenAPI + MCP projection.
- S02: `migrate`, `event`, `job`.

## 3. Scope

**IN** §2.7's five tables, 32 actions, the `search` module, the eval harness, §7.2's coverage table.
**OUT** TOC API and screens (**S08**; `outline`/`outline_node` ship here), ArchiMate/UML/renders (**S08**), project CRUD (**S05**), `bulk` and RM's `promote`/`project-copy` (**dropped**), UI (**S12**).
**UI surface declared for S12**: `requirements editor/revisions` — `requirements.*` + `quality.score`, read models `governed_asset` + `revision` (`RevisionDiff`), and `search` — `search.query` ranked results over assets/cards/catalogue entries.

## 4. Source material

- **SPEC §4, §6, §8 S7, §11 R2** — one lifecycle, one revision table; not `entity_version_history`, the ~20 catalogue tables or the model/revision sprawl.
- **02-DATA-ARCHITECTURE §2.4, §2.7, §3.4, §4, §5** — columns and indexes verbatim, the state machine, the JSONB policy.
- **02-source-map/30-rm §1** (RM `9ee72ab`) — 28 routers → §7.2; `entity_version_history` has **no `tenant_id`**; not `bulk`, `pm_card_ledger`, promote/project-copy.
- **02-source-map/20-fr `6f7c509792b3ca1fc1fb00c198aebcec83f0af29`** (`library`, `observe`, `project-requests`) — asset shape, traceability, intake; not `research`/`tools` (S0).

## 5. Deliverables

```
migrations/0007_requirements.sql · crates/domain/src/{governance,quality,compare}.rs
crates/store/src/{governed_asset,revision,catalogue,quality,search,project_request}.rs
crates/actions/src/{governance,library}/*.rs · crates/api/src/search/{rank,query}.rs   # module, not a crate (CONTRACT §1)
apps/server/src/seed.rs                                   # the `migrate --seed` importer this slice owns
crates/api/src/{requirements,principles,decisions,catalogues,search,project_requests}.rs
crates/api/tests/{revision_model,catalogue_seed,quality_rubric,search_index,traceability,project_request,search_eval}.rs
seed/catalogue.json · tests/eval/search_queries.json   # eval DATA, not a test target
```

### 5.1 Tables — §2.7 verbatim, five only

```
outline            project_id, kind∈(wbs,ltp,toc), title, settings · uq(tenant,project,kind,title)
outline_node       outline_id, parent_id, position, kind, title, body, meta, edge_kind
                   · ix(tenant,outline,parent,position,id)
quality_evaluation entity_type, entity_id, score, criteria, evaluator_kind
                   · ix(tenant,entity_type,entity_id,created_at desc)
principle_conflict principle_a_id, principle_b_id, verdict, dismissed_by/at · uq(tenant,a,b)
project_request    external_ref, external_origin, payload, status∈(draft,approved,rejected,promoted),
                   project_id, repository_url · uq(tenant,external_ref)
```

All five: `tenant_id NOT NULL`, `FORCE` RLS, `tenant_isolation` (`USING`+`WITH CHECK`); `quality_evaluation` is append-only. The migration also adds `pg_trgm` and `gin(title gin_trgm_ops)` on `governed_asset`, `card`, `catalogue_entry(label)` — no column, key or constraint.

### 5.2 Actions

One handler serves `requirements.*`, `principles.*`, `decisions.*`: `create {key?,title,body?,acceptance?,project_id?}` → `{id,key,version,status,etag}`, `get`, `update`, `transition {id,to,superseded_by?}`, `list`, and `requirements.compare {id,from,to}` → `RevisionDiff`. Separate: `catalogues.list|import {dry_run?}`, `quality.score {revision_id}`, `search.query {q,types?,limit?}`, `trace_links.*`, `project_requests.*` (idempotent on `(tenant,external_ref)`).

Two more over `principle_conflict`: `principles.list_conflicts {verdict?,cursor?,limit?}` → page of `{id,principle_a_id,principle_b_id,verdict,dismissed_at}`; `principles.dismiss_conflict {id,reason?}` → `{id,verdict,dismissed_by,dismissed_at}`, `dismissed_by` from the principal.

**Kind is never an input** — `requirements.*` is bound to `kind='requirement'` at declaration, so it cannot reach a principle. `domain::governance::transition(kind,from,to)` is the only legality check (proptested); `superseded` requires `superseded_by_id` in the same statement (§3.4's immediate CHECK: `status <> 'superseded' or superseded_by_id is not null`).

### 5.3 Revision model

Every mutation writes one `revision` row in the same transaction: `entity_type='governed_asset'`, `seq = governed_asset.version`, `label ∈ created|updated|status:<to>`, `content` = the post-mutation snapshot, `content_hash` = SHA-256 of JCS-canonical JSON, `author_id`, `status` mapped `draft|in_review→draft`, `approved→published`, `superseded→superseded`. `uq(tenant,entity_type,entity_id,seq)` makes a double write fail loudly. `compare` diffs two `content` documents **in Rust** — field-level `added/removed/changed`, `acceptance` matched by criterion id, not index.

### 5.4 Catalogue import

`seed/catalogue.json` is a checked-in export of live RM, never called at runtime; re-count first (293/282/26 are `pg_stat` estimates). `migrate --seed [--tenant <slug>]` writes — `--tenant` optional, default **all tenants** (the local gates call plain `migrate --seed`); the same importer runs at tenant create. One row per kind (codes `UNVERIFIED` until S14 §6 step 8's re-count and export):

```
card_type → table card_type   bug_fix "Bug fix"  {"inputs":["repo"],"gates":["human_approval"]}   # its own table, NOT a catalogue kind
project_type         bug_fix "Bug fix"        {"phase_order":[…],"matrix":{"bug_fix":[…]}}
phase                do "Doing"               {"order":2}
capability           recovery "Recovery"      {}
wip_limit            active "Active WIP"      {"limit":3}
priority_level       p2 "High"                {"rank":2}
deliverable_kind     report "Report"          {}
milestone_kind       gate_review "Gate review" {}
gate                 human_approval "Human approval" {"dual_control":false}
board_template       standard "Standard board" {"lists":["intake","active","waiting","done","archive"]}
architecture_standard archimate_3_2 "ArchiMate 3.2" {"version":"3.2"}
```

Only the first row is a direct table write: `card_type` rows land in `card_type`, because `catalogue_entry.kind` is S03's fixed eleven-value vocabulary and does **not** include `card_type`. Every other line's left-hand name is a `catalogue_entry.kind`. The 282 type×card-type pairs sit in `project_type.meta.matrix` (what `may_move` reads), not 282 rows. Import is `ON CONFLICT (tenant,kind,code) DO UPDATE … WHERE (label,meta,sort,is_active) IS DISTINCT FROM (…)`.

### 5.5 Quality scoring

Scored: **one immutable revision** (`entity_type='revision'`, `entity_id=<revision.id>`); re-scoring yields the same number, history per row. Rubric v1, weights summing to 1, `score = 100·Σ(w·raw/5)`:

- **clarity (.20)** — title 15–80 chars, no vague-list term.
- **testability (.30)** — ≥1 `acceptance` entry with `given/when/then` or a numeric threshold.
- **atomicity (.20)** — ≤1 conjunction per sentence, mean ≤25 words.
- **traceability (.15)** — 0 links → 0; 1 → 3; ≥2 including a card → 5.
- **uniqueness (.15)** — same-kind trigram similarity: ≥0.9 → 0, ≤0.5 → 5.

`criteria` stores the rubric version and each raw/weighted value. **Read-only:** scoring writes only `quality_evaluation` — never `governed_asset`, never a status change or blocked transition — as `job(kind='quality.score', dedupe_key='revision:<id>')` enqueued **after commit**.

### 5.6 Search and the eval set

Three ranked arms (`governed_asset`, `card`, `catalogue_entry`) `UNION ALL`, each `LIMIT n`, outer `ORDER BY score, id`. Every predicate — `tenant_id = current_setting('app.current_tenant',true)::uuid`, `kind`, `status`, `project_id` — is SQL, so Rust sees at most `limit+1` rows. Score is `ts_rank_cd(tsv, websearch_to_tsquery('english',$1))·w₁ + similarity(title,$1)·w₂ + key-exact-bonus`, restricted by `tsv @@ tsquery OR title % $1`; the three weights are one committed constant tuned by grid search, not a learned model.

**Eval set:** `tests/eval/search_queries.json` — 40 queries `{id,text,intent,expected_top5,source}`, intents `exact_key|title|body_phrase|paraphrase|typo`, ≥6 each: ~20 from the Loki 24 h `/requirements/search*` sample, ~10 live probes, ~10 handwritten (Istio may log no query string; RM's router tests substitute). **Corpus:** a read-only RM export in a throwaway DB — an eval fixture, not a migration (D7) — keys preserved so both share one key space. **Expected ids are human-labelled, never from whichever system produced them**, or the baseline is a ceiling.

Arms: A `ILIKE` floor, B FTS + trigram (shipped), C live RM `/search` (baseline), D pgvector (contingency). `recall@5 = mean(|expected ∩ top5| / |expected|)`, MRR = mean 1/rank. **Targets:** B ≥ 0.75 recall@5, ≥ 0.60 MRR, **B ≥ C** on both; a loss above 0.05 recall@5 ships D, ≤ 0.05 is recorded; if C is unreachable, B ≥ A + 0.20.

### 5.7 Traceability

`trace_links.*` is the only writer of `trace_link`; S08 references it. S07 demonstrates the path: requirement `R` → `trace_links.create{requirement,R,'card',C,'traces'}` → `GET /api/requirements/{R}?include=trace` returns `C` **and** `GET /api/cards/{C}/traces` returns `R`, same tenant, cross-tenant 0 rows; `requirement→model_element` uses the same actions in S08. `project_requests.promote` creates its `project` in the same transaction via the store, never by re-calling `projects.create`.

### 5.8 Events

Outbox rows, one per mutation, in its transaction (CONTRACT §7).

| type | emitter | payload keys |
|---|---|---|
| `governed_asset.created` | `{requirements,principles,decisions}.create` | `id,key,kind,version` |
| `governed_asset.updated` | `.update` | `id,version,changed[]` |
| `governed_asset.transitioned` | `.transition` | `id,from,to,version` |
| `governed_asset.superseded` | `.transition` to `superseded` | `id,superseded_by,version` |
| `quality.evaluated` | `quality.score` job | `revision_id,score,rubric` |
| `catalogue.imported` | `catalogues.import` | `kind,created,updated,skipped` |
| `project_request.created` | `project_requests.create` | `id,external_ref,status` |
| `project_request.updated` | `.update` | `id,status` |
| `project_request.approved` | `.approve` | `id,project_id` |
| `project_request.rejected` | `.reject` | `id,reason` |
| `project_request.promoted` | `.promote` | `id,project_id` |

## 6. Build order

1. `cargo test --workspace`; `docker compose up -d db`; re-count RM in one read-only statement — `export RM_RECOUNT="BEGIN READ ONLY; SELECT 'pm_card_types',count(*) FROM pm_card_types UNION ALL SELECT 'pm_catalogue',count(*) FROM pm_catalogue UNION ALL SELECT 'pm_project_type_card_types',count(*) FROM pm_project_type_card_types; COMMIT;"; psql "$RM_READONLY_URL" -c "$RM_RECOUNT"`.
2. `0007_requirements.sql`: five tables, FKs, indexes, `FORCE` RLS, `pg_trgm`, trigram indexes;
   `cargo run -p conveyor-server -- migrate` twice (the binary re-applies the runtime grants; `sqlx migrate run`
   would not — CONTRACT §16).
3. Domain `governance::transition`, `compare::diff`, `quality::rubric`; `cargo test -p conveyor-domain`.
4. Store: asset write + revision insert in one transaction; `cargo test -p conveyor-store requirements`.
5. `requirements.*`, `trace_links.*`, `principles.*`, `decisions.*` in the registry; test that no input can set `kind`; `curl /api/actions | jq`.
6. `requirements.compare` diffs two `content` documents in Rust; snapshot test on a 3-revision asset — `cargo test -p conveyor-api --test revision_model`.
7. Export `seed/catalogue.json`; `migrate --seed --tenant dev-a` twice → `0/0/N` (`dev-a` is S02's dev tenant; a tenant row must exist first); `catalogues.list|import` with `dry_run`.
8. Rubric + `quality.score` + the post-commit job — `cargo test -p conveyor-api --test quality_rubric`; assert no `quality_evaluation` row in the asset transaction.
9. `search.query`: three arms, weights, `ts_headline` capped at 50 — `cargo test -p conveyor-api --test search_index`.
10. Eval harness: corpus loader, 40-query JSON, `--ignored` test that **asserts** recall@5 and MRR meet the
    agreed targets and beat baseline C, printing the numbers and exiting non-zero on a miss.
    `cargo test -p conveyor-api --test search_eval -- --ignored --nocapture`.
11. Baselines A and B (ILIKE floor; FTS + trigram) — `BASELINE=ab cargo test -p conveyor-api --test search_eval -- --ignored --nocapture`.
12. Baseline C, live RM `/search` replayed over the same corpus — `BASELINE=c RM_READONLY_URL="$RM_READONLY_URL" cargo test -p conveyor-api --test search_eval -- --ignored --nocapture`.
13. Weight grid search, one bounded run — `GRID=1 cargo test -p conveyor-api --test search_eval -- --ignored --nocapture`.
14. Decide D by §5.6 and write the completion record — `cargo test -p conveyor-api --test search_eval -- --ignored --nocapture`.
15. `project_requests.*` + `project_request.timeout` (S06 §5.3) — `cargo test -p conveyor-api --test project_request`; clippy.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo test -p conveyor-api --test search_index --test s07_assets --test s07_requests
cargo test -p conveyor-store --test s07_store --test s07_seed --test s07_governance
cargo test -p conveyor-api --test search_eval -- --ignored --nocapture
# ⚠️ Corrected 2026-11-09: this block named five targets that do not exist — `revision_model`, `catalogue_seed`,
# `quality_rubric`, `traceability`, `project_request`. `cargo test --test <missing>` is a hard error, so the gate
# could never have passed as written. The properties are proved by the targets above: the asset/revision model
# and its seed by `s07_store`/`s07_seed` (store crate), the rubric and quality scoring by `s07_assets`, the
# project-request surface by `s07_requests`, and traceability by `s07_governance` + `s07_requests`.
SEARCH_SQL="SELECT id FROM governed_asset WHERE tenant_id = current_setting('app.current_tenant',true)::uuid AND (tsv @@ websearch_to_tsquery('english','auth') OR title % 'auth') ORDER BY ts_rank_cd(tsv, websearch_to_tsquery('english','auth')) DESC, id LIMIT 20"
TENANT=$(psql "$DATABASE_URL" -tAc "select id from tenant where slug='dev-a'")   # `tenant` has no RLS
psql "$DATABASE_URL" -qAtc "BEGIN; SET LOCAL enable_seqscan=off; SET LOCAL app.current_tenant='$TENANT'; EXPLAIN (ANALYZE, BUFFERS) $SEARCH_SQL; COMMIT;" \
  | grep -qE 'governed_asset_tsv_idx|governed_asset_title_trgm_idx'
```

⚠️ **The grep above must name the GIN, not "any index".** The first version of this line matched
`Bitmap|Index Scan`, which is satisfied by the *degraded* plan — measured 2026-11-08, under FORCE RLS without
the LEAKPROOF provisioning, the planner answers `Bitmap Index Scan on governed_asset_project_idx` (a tenant
btree) plus a per-row filter, i.e. exactly the shape the check was supposed to reject. Both GINs are dead
until `scripts/db-roles.sql` marks `ts_match_vq` and `word_similarity_op` LEAKPROOF (CONTRACT §2). The real
proof is `crates/api/tests/search_index.rs`, which asserts the index **by name** as `conveyor_runtime`.

**Properties.** (a) 3 revisions — create+update+transition → `count(*)=3, max(seq)=3`, `compare 1..3` shows the status change. (b) seed idempotent — second run `{0,0,N}`. (c) indexes used — both GINs present, `EXPLAIN` under `enable_seqscan=off`, `idx_scan` rises on a 50k-row load (4k seq-scans: vacuous). (d) relevance — B meets §5.6's targets. (e) traceability — both directions, cross-tenant 0 rows. (f) quality read-only — `version`/`status` untouched, no row in the asset transaction. (g) no orphan — the list below covers all 28.

**Completion record** (CONTRACT §14): `A/B/C/D × recall@5 · MRR · 40 queries · corpus · date`, filled at close.

**RM coverage — 28 modules: 7 here, 2 dropped, 19 elsewhere.** No orphan.

- **S07 (7):** `requirements`, `principles`, `decisions` (one lifecycle; `promote`/`project-copy` dropped), `principle_conflicts`, `search`, `pm_catalogue` (one `catalogue_entry` + seed), `integration_tools` (`gap-to-workpackages` → S08).
- **Dropped (2):** `bulk` (registry invocations replace batch), `pm_card_ledger` (Planka-id key, SPEC §4).
- **Elsewhere (19):** `pm_projects` (S05); `toc`, `pm_programmes` (**S08**); `pm_resources`, `pm_risks`, `pm_benefits`, `architecture_{elements,relationships,views,standards,containers,tools,metamodel}`, `modeling_{models,exchange,document_export}`, `uml_{models,tools}` (S08; `_is_fr_close` dropped); `health` (S02).

## 8. Traps

1. **`entity_version_history` had no `tenant_id`.** Do not carry its RLS carve-out: `revision.tenant_id` is `NOT NULL`, so a bad join reaches no other tenant's history.
2. **No Rust-side filtering.** 10k rows narrowed in memory is fatal in production; predicates are SQL, each arm carries `LIMIT`.
3. **`tsvector` config.** The generated column needs a literal `'english'` (`current_setting(...)` is not immutable and is rejected) and `ts_rank_cd` must name the index's config or it scores 0.
4. **Trigram ≠ full-text.** `gin(title gin_trgm_ops)` serves `%`, `ILIKE` and `similarity()`; `tsv @@ tsquery` needs `gin(tsv)`; `similarity()` ordering without a `%` predicate scans the table.
5. **Quality inside the mutation transaction is a lock hazard.** Several trigram queries per criterion hold the asset's row lock.
6. **RM's mounts are unprefixed and unversioned** (`/requirements`, `/search`); the contract's are `/api/<resource>` — no alias, no rewrite; the durable name is the action.

## 9. Decisions closed here

1. One `revision` per mutation, `seq = version`, full snapshot, hash of canonical JSON; `compare` diffs two revisions in Rust.
2. Three resource groups, one handler; kind fixed by action name, never by input.
3. Card types stay in `card_type` (data-arch §10 Q1); everything else is `catalogue_entry`.
4. Catalogue seed is per-tenant data from a checked-in export, imported at tenant create.
5. Search ships FTS + trigram; pgvector only on the stated trigger.
6. Ranking is explicit committed weights, grid-tuned — no sklearn port.
7. Quality scores a revision: append-only, post-commit, advisory.
8. `trace_links.*` is the single writer of `trace_link`.

## 10. Open questions

1. **Does Loki's 24 h sample record query strings?** `UNVERIFIED`; default: assume not — RM router tests plus a probe.
2. **Is RM's `semantic_search` reproducible?** Default: replay `/search`; semantic is an upper bound.
3. **Does S08 re-create `outline`/`outline_node`?** Default: no — it references them; both stay empty until S08.
4. **`project_request.timeout` window** (fr's timer was 300 s)? Default: 24 h, measured before S14.
5. **Does `search.query` return cards?** Default: yes, but the eval scores governed assets until card text is real.
