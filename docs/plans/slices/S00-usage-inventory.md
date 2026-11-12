---
slice: S00
title: Usage inventory → parity/parity.yaml
status: planned
depends_on: []
spec_sections: ["§4", "§8 S0", "§9", "§11 R5"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: PENDING-S0   # S0 produces the ids; this plan is its producer
---

# S00 — Usage inventory → `parity/parity.yaml`

Programme project: S00 · issues none yet

**Slice:** S0 · **Executor:** a fresh session, verbatim · **Gate (SPEC §8):** the feature list is signed; zero-usage features are dropped, not ported.

**Read first:** `docs/SPEC.md` §4/§8/§9; `plans/01-CONTRACT.md` §12; `plans/02-source-map/{20,30,40,50}-*.md`.

## 1. Objective

Measure from durable and sampled evidence which fr, RM, Planka and n8n operations are actually used, then publish one signed `parity/parity.yaml` that every slice consumes as `parity_ids` and S14 gates on (100 % of `used`). D2/R5: zero-usage surface is dropped, not ported.

**Artifact:** `$HOME/Movies/conveyor/parity/parity.yaml`; supporting files in §5.

## 2. Starting state

- Measured 2026-11-07: the tree exists and holds only `docs/` — 25 files, no `Cargo.toml`, no `parity/`, no `.git` (CONTRACT §13).
- fr **785 served operations / 616 paths** (`backend/src/capabilities/served-spec.lock.json`, measured 2026-11-07); RM **314** decorators; Planka **67** openapi paths; n8n **348 workflows / 332 active**.
- Loki 24 h; `execution_entity` 3 d / 470 rows; Planka 5 test cards; fr `approvals=0` but `conveyor_flow_runs=693`; RM 293 card types (estimate). Re-measure.
- Read-only; SQL directly on master as `postgres`, **never through PgBouncer**, never a session `SET`; no prune, no state deletion; disk <10 % ⇒ STOP and report.

## 3. Scope

**IN:** every fr served operation plus handlers absent from the served spec; all RM routes; all Planka openapi paths; distinct `(package, resource, operation)` tuples in active n8n workflows; named flows `conveyor.start`, `wip.pull`, `ask_fr`, `err.card`; row counts and Keycloak clients as evidence.

**OUT:** coding-*, artifacts, worker_management, inference-router, licenses, billing_service (D12/D14); n8n flows as features (S13 re-authors them; 280 card-type×list flows are one feature); procedure_management's 54 endpoints (D10 — one SUMMARY line); UI screens (S12); performance; data migration (D7).

## 4. Source material — evidence sources and classes

Rank **A > B > C > D > E**; the highest class wins and is stored in `evidence_class`.

| # | source and query (exact in §6) | pin (origin/main, 2026-11-07) | window | proves / cannot prove | class |
|---|---|---|---|---|---|
| S1 | fr `served-spec.lock.json` / live OpenAPI | `fr@4a579e91d355` | current | the surface set (785) / usage | D |
| S2 | RM `src/routers` + mount map / OpenAPI | `rm@191086942ce5` | current | the surface set (314) / usage | D |
| A1 | n8n `workflow_entity.nodes`, `jsonb_array_elements`, active only | live 2026-11-07, no pin | durable | an active flow calls it / that it ran | A |
| A2 | call sites: mcp_server channel, telegram/email/teams webhooks, fr workers, cron | live 2026-11-07, no pin | durable | a reachable path needs it / frequency | A |
| B1 | Loki `{namespace="istio-system",container="istio-proxy"}`, `count_over_time` by `path` | live 2026-11-07, no pin | 24 h × 2 samples | traffic that day / older use | B |
| C1 | n8n `execution_entity`, `group by "workflowId"` | live 2026-11-07, no pin | ≤72 h / 470 rows | recent execution / older use | C |
| D1 | fr `pg_stat_user_tables`, then `count(*)` | live 2026-11-07, no pin | current | what ran (`conveyor_flow_runs=693`) | D |
| D2 | RM `count(*)` on `pm_*`, requirements, modelling | live 2026-11-07, no pin | current | catalogue is real / route use | D |
| D3 | Planka DB + `docs/openapi.json` | `kanban@aafb771cea4c` (file); DB live | current | nothing about usage — test data | D |
| E1 | Keycloak `client` + sessions | live 2026-11-07, no pin | current | clients, logins / per-route use | D/E |

A alone makes `used`; B needs repetition; C/D/E never do.

## 5. Deliverables and sign-off

Deliver `parity/parity.yaml` (one row per surface; contract artifact), `parity/schema.json` (`additionalProperties: false`, 9 required keys), `parity/surfaces/{fr,rm,planka,n8n}.txt`, `parity/evidence/*` (two Loki samples, execution census, row counts, `matched.tsv`), `parity/SUMMARY.md` (counts per area × status, `used` ids, evidence classes, gate output, sign block).

**Signed:** §7 passes; `SUMMARY.md` carries `signed_off: <reviewer> <date> <sha256 parity.yaml>`; the ledger row (`~/.claude/plans/program/LEDGER.md`) is updated. The reviewer (programme owner) signs that every `used` id will be built and every `dropped` id will not.

## 6. Build order — method

Steps 0–14, each ≤1 day, read-only. Run every SQL command on master: `ssh -i ~/.ssh/deploy-key admin@192.0.2.10 "sudo -u postgres psql -d <db> -Atc '<sql>'"`. **Every Loki zero needs a positive control** (same query minus the host filter > 0), else record `query_failed`, never "unused".

0. **Preflight and dirs.** Disk and tools; master databases; Loki template; create the output dirs; save `parity/evidence/00-preflight.txt`; any miss ⇒ STOP.
1. **fr manifest.** `served-spec.lock.json` (785 ops), union `routes/*.ts` handlers absent from the lock; live-OpenAPI fallback verified 2026-11-07: 200, 4,443,102 B; the public host 404s.
2. **RM manifest.** 314 decorators; pair each method with its path plus the mount prefix from `src/routers/__init__.py`; emit `rm<TAB>METHOD /path`.
3. **Planka manifest.** 67 openapi paths.
4. **n8n operations.** Distinct `(package, resource, operation)` in active workflows, plus the 348/332 census and four named flows. Parameter keys measured 2026-11-07 on an active Planka node: `resource`, `operation` and resource-specific fields (`boardId`, `listId`, `cardId`, `updateFields`, …) — no `url`/`method` key.
5. **Keycloak.** Database `keycloak`, table `client` (`realm_id`, `client_id`, `enabled`) verified 2026-11-07; clients only, never tokens.
6. **Normalise.** UUID/number segments → `:id`; strip query strings; uppercase methods; dedupe; sort `(app, surface)`; assign `P-0001…`; write `parity/inventory.tsv` and four `app<TAB>surface` manifests.
7. **Loki sample 1.** 24 h per host; the query below was validated on a 5 min window (positive control 3,743 lines; `path` series returned). Repeat per host; save raw JSON.
8. **Loki sample 2**, ≥24 h later, same command, new file. No sample 2 ⇒ no B-class `used`.
9. **Executions.** Columns are camelCase (`workflowId`, `startedAt`, `stoppedAt`, `createdAt`) — measured 2026-11-07, so double quotes are required; join to workflow names.
10. **Counts.** All **88 fr base tables** (86 domain + `_migrations`/`_migration_effects`) via `pg_stat_user_tables` — it lists tables, not the two `pg_stat_statements*` views, so the live `fr` database reads 88 here and 90 relations in `\dt+`; plus `count(*)` on the six conveyor tables; RM `count(*)` on the `pm_*` catalogue/model tables (57 domain tables, 58 incl. `alembic_version`); Planka per-table census (33 domain, 35 incl. `migration`/`migration_lock`; D3). Never quote estimates.
11. **Cross-match.** Layout measured 2026-11-07: `nodes/<Node>/operations/index.ts` calls ``ctx.helpers.request({ method: "GET", uri: `${baseUrl}/api/…` })``; parameters are declared in `nodes/<Node>/resources/*.resource.ts` or `descriptions/` — not `url:`/`method:` node fields. Maps operation → endpoint; join step 4 to fr/RM/Planka; write `parity/evidence/matched.tsv`.
12. **Classify and write.** Apply §9; write `parity/schema.json` and `parity/parity.yaml`, one row per source surface.
13. **Acceptance and conflicts.** Every `used` gets an acceptance string; re-sample every B-only `used`, demote if it does not repeat; A-vs-decision contradictions ⇒ STOP.
14. **Gate, SUMMARY, sign.** Run §7; write `parity/SUMMARY.md`; sign per §5.

Commands for steps 6, 10, 12–14 (from `$HOME/Movies/conveyor`):

```bash
# 6 — normalise, dedupe, assign ids
awk -F'\t' 'BEGIN{OFS="\t"} {s=$2; sub(/\?.*/,"",s); gsub(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/,":id",s); gsub(/[0-9]+/,":id",s); print $1, s}' parity/surfaces/*.txt | sort -u | awk -F'\t' 'BEGIN{OFS="\t"}{print "P-" sprintf("%04d",NR), $1, $2}' > parity/inventory.tsv
# 10 — six conveyor-table counts, on master (pipe into parity/evidence/fr-counts.tsv)
ssh -i ~/.ssh/deploy-key admin@192.0.2.10 "sudo -u postgres psql -d fr -Atc \"select 'approvals',count(*) from approvals union all select 'approval_decisions',count(*) from approval_decisions union all select 'card_drafts',count(*) from card_drafts union all select 'card_phase_lease',count(*) from card_phase_lease union all select 'conveyor_flow_runs',count(*) from conveyor_flow_runs union all select 'project_conveyor_start_attempts',count(*) from project_conveyor_start_attempts\""
# 12 — schema.json (9 required keys, no extras)
jq -n '{type:"array",items:{type:"object",additionalProperties:false,required:["id","app","area","surface","source","evidence","evidence_class","acceptance","status"],properties:{id:{type:"string",pattern:"^P-[0-9]{4}$"},app:{enum:["fr","rm","planka","n8n"]},area:{type:"string"},surface:{type:"string"},source:{type:"string"},evidence:{type:"string"},evidence_class:{enum:["A","B","C","D","E"]},acceptance:{type:"string"},status:{enum:["used","rare","unused","dropped"]}}}}' > parity/schema.json
# 13 — B/C rows to re-sample
yq -r '.[]|select(.status=="used" and (.evidence_class=="B" or .evidence_class=="C"))|"\(.id)\t\(.surface)"' parity/parity.yaml > parity/evidence/resample.txt
# 14 — SUMMARY counts and the signed hash
yq -r '.[]|.area+"\t"+.status' parity/parity.yaml | sort | uniq -c > parity/evidence/counts.tsv && shasum -a 256 parity/parity.yaml
```

## 7. Gate (executable)

From `$HOME/Movies/conveyor`, under `bash`; every line is an assertion and exits 0 only when the property holds. `check-jsonschema` is **not** installed on mini3 (verified 2026-11-07) — install it first.

```bash
set -euo pipefail
uv tool install check-jsonschema            # once; absent 2026-11-07
test -z "$(yq -r '.[].id' parity/parity.yaml | sort | uniq -d)"                                   # G1 ids unique
test -z "$(yq -r '.[].id' parity/parity.yaml | grep -Ev '^P-[0-9]{4}$' || true)"                  # G2 id shape
test -z "$(yq -r '.[] | select(.status == "used") | select((.acceptance // "") | test("^(test|e2e|probe):") | not) | .id' parity/parity.yaml)"  # G3 acceptance
diff <(cat parity/surfaces/*.txt | sort -u) <(yq -r '.[]|"\(.app)\t\(.surface)"' parity/parity.yaml | sort)                     # G4 exactly once
test "$(cat parity/surfaces/*.txt | sort -u | wc -l | tr -d ' ')" = "$(yq 'length' parity/parity.yaml)"                          # G5 reconciles
test "$(yq -r '[.[]|select(.status=="used" and .evidence_class=="B")]|length' parity/parity.yaml)" -eq 0 \
  || test "$(ls parity/evidence/loki-*sample*.json 2>/dev/null | wc -l | tr -d ' ')" -ge 2                                       # G6 two B samples
test -z "$(yq -r '.[] | select(.status == "dropped") | select((.evidence // "") | test("^(decision:|replacement:)") | not) | .id' parity/parity.yaml)"  # G7 dropped reason
check-jsonschema --schemafile parity/schema.json parity/parity.yaml
```

G4 = "every surface appears exactly once"; G5 reconciles; `dropped` rows start `evidence` with `decision:` or `replacement:`.

## 8. Traps

1. **Loki is 24 h** — zero ≠ unused; two samples + positive control; `query_failed` is a third state.
2. **Executions are 3 d / 470 rows** — never answer quarterly use.
3. **Planka is empty** — 4 boards / 5 test cards; use call sites.
4. **Frozen/legacy is most of fr** — 88 `defineCapability` vs 548 paths / 1,376 declarations; the lock holds 785 ops / 616 paths. Counting declarations understates fr ~6×.
5. **n8n public API paginates at 250** — use `workflow_entity`.
6. **`planka_card_id` joins are evidence only, never identity** (CONTRACT §3).
7. **One session `SET` through PgBouncer poisons the next client**; query master directly.
8. **`nodes` is `json`** — cast `::jsonb`; **Loki host is a quoted field, not a label** — filter by string.
9. **Idle ≠ unused** — `approvals=0`, `conveyor_flow_runs=693`; a LogQL pattern that parses nothing returns 0, not zero traffic.

## 9. Classification rules

Precedence **decision-drop → A → B → C → D**; first match wins.

| status | rule |
|---|---|
| `dropped` | A taken carry decision (SPEC §2/§4; 20/30-source-map carry lists): Planka attachments/memberships, RM bulk/promote/ledger, procedures (D10). Or `unused` with a named replacement. |
| `used` | A hit from an active caller; OR B ≥5 in each sample; OR B ≥1 in both + C ≥1 in 72 h. |
| `rare` | B 1–4 in either sample and no A; OR C alone; OR A only from an inactive flow; OR D-only; OR an RM surface in a D3 domain. |
| `unused` | No A/B/C hit, no D corroboration, not decision-dropped. |
| `dropped` (usage) | A `rare` row that at sign-off neither folds into a `used` action/enum nor reuses a `used` read model. |

Loki zero never downgrades an A `used`; a missing B sample caps at `rare`; C corroborates only; an A hit on a decision-dropped surface is a **contradiction** — STOP.

`acceptance`: `test:<crate>::<name>`, `e2e:<spec>::<name>`, `probe:<METHOD> <path>`; only `used` carries it.

## 10. Feature IDs and areas

`id: P-NNNN`, four digits, global, allocated once in `(app, surface)` order; never reused; dropped rows keep their id.

Areas: **board** (Planka board/list/card/label/comment/task/webhook + fr kanban/cards) · **control-plane** (approvals, claims, runs, andon, schedules, workers, katas) · **requirements** (RM requirements/catalogues/quality/search) · **modelling** (RM architecture_*, modeling_*, uml_*, toc_*) · **documents** (fr documents, repositories, assets) · **channels** (fr notifications/telegram/email/teams/inapp + MCP) · **copilot** (agui*, agent-core, sessions) · **admin** (admin/*, workspaces, departments, user_roles, tenants, secrets, licences) · **procedures** (S11 only; empty, D10) · **workflows** (named flows, engine + editor, card-type×list matrix).

`app` ∈ `fr | rm | planka | n8n` (CONTRACT §12); `surface` = `METHOD /path`, `<package>.<resource>.<operation>`, or a flow name. Two added detail keys; no contract field removed or retyped:

```yaml
- {id: P-0001, app: fr|rm|planka|n8n, area: <area>, surface: <route|action|flow>,
   source: <path|query>, evidence: <count|query|row>, evidence_class: A|B|C|D|E,
   acceptance: <test|e2e|probe>, status: used|rare|unused|dropped}
```

## 11. Decisions closed here

**Closed (binding):**
1. Unit = `(app, method, normalised path)`; n8n unit = `(package, resource, operation)`; flows are not features.
2. `rare` is carried only if it folds into a `used` action/enum; else dropped at sign-off.
3. Decision-drops outrank usage; an A hit on one is a STOP.
4. B-only `used` needs both samples; C never stands alone.
5. `parity/parity.yaml` is the only list; slices reference `parity_ids`.
6. No data migration (D7); RM is whole (D3), so zero-usage RM is `rare`; `procedure_management` is one SUMMARY line (D10).

## 12. Open questions

**Open (≤5, each with a default):**
1. Four named flows: procedures or platform features? **Default:** procedures; `err.card` → notification job.
2. Do `email`/`teams` fan-out survive (SPEC §5.8)? **Default:** drop unless A.
3. Which Keycloak realms count? **Default:** `portal` only.
4. Frozen route with live Loki traffic — ported? **Default:** port; record `frozen`.
5. All RM modelling tables empty? **Default:** `rare` under D3.
