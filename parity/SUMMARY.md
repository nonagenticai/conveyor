# S00 usage inventory — SUMMARY (FINAL, signed 2026-11-08)

**Status: SIGNED.** `parity/parity.yaml` is the contract artifact; every slice consumes `parity_ids`
from it and S14 gates on 100 % of `used`.

```
signed_off: georgedekker 2026-11-08
  sha256: 866acfc96fa2a608c98f13551243f7a75b47060bc3fa6b55751086f774d6ef25
  note: recorded by agent on owner instruction 2026-11-08. Every `used` id will be built at cutover
        and every `dropped` id will not.
```

- **Loki sample 1** instant `2026-11-07T22:17:46Z` (epoch 1794089866), 24 h lookback.
- **Loki sample 2** instant `2026-11-08T22:21:24Z` (epoch 1794176484), same query, 24 h lookback.
  The two windows are adjacent and disjoint: sample 2's epoch is sample 1's + 86400.
- **Positive control** 1,210,461 lines (sample 1) / 1,241,610 (sample 2) — the collector was alive
  and at comparable volume both nights, so a low row count is traffic, never a dead query.
- Raw evidence: `parity/evidence/loki-sample{1,2}-*.json` (11 files each, all `status=success`).

The raw Loki samples are **held privately** and are no longer in the public tree; the counts above and the method below remain.
- Method, pinned commands, reproduction proof and every deviation: **`parity/evidence/02-day2-method.txt`**.
  Counts: `parity/evidence/surface-counts-sample{1,2}.tsv`, decisions `03-reclassification.tsv`.

## The inventory — 1,300 surfaces

| app | surfaces |
|---|---:|
| fr | 798 (785 served-spec ops + 5 specExempt + 8 direct server.ts handlers) |
| rm | 314 (all route decorators × mount prefixes) |
| planka | 96 (67 openapi paths × methods) |
| n8n | 92 (87 active node-operation tuples + 5 named flows) |
| **total** | **1300** |

Day 1 produced 1,299; day 2 added the fifth active "other" flow, `kata.open`, as `P-1300` — an
active flow S13 must re-author, so leaving it out would have dropped it silently.

## Counts per area × status (final)

| area | used | rare | unused | dropped | total |
|---|---:|---:|---:|---:|---:|
| admin | 29 | 15 | 127 | 88 | 259 |
| board | 19 | 9 | 108 | 5 | 141 |
| channels | 2 | 0 | 18 | 0 | 20 |
| control-plane | 19 | 20 | 107 | 30 | 176 |
| copilot | 1 | 5 | 4 | 0 | 10 |
| documents | 4 | 1 | 48 | 111 | 164 |
| modelling | 56 | 126 | 125 | 0 | 307 |
| requirements | 67 | 119 | 0 | 10 | 196 |
| workflows | 21 | 0 | 6 | 0 | 27 |
| **total** | **218** | **295** | **543** | **244** | **1300** |

Per app: fr 59 used / 48 rare / 487 unused / 204 dropped · rm 60 / 244 / 0 / 10 ·
planka 7 / 3 / 56 / 30 · n8n 92 / 0 / 0 / 0.
Evidence classes: **A 173 · B 66 · D 1061**.

Movement from day 1 (167 used / 334 rare / 708 unused / 90 dropped):
**+24 `rare → used`** (B ≥ 5 in *each* sample), **11 `unused → rare`** (a day-2-only hit — a surface
with zero in sample 1 was still hit in sample 2 and can no longer claim zero usage), and
**154 → `dropped`** (the decisions below). `n8n` is unchanged at every row: all of it is class A.

## How statuses were assigned

Precedence **decision-drop → A → B → C → D**, first match wins.
A = a live call site (a typed operation in an active workflow, or a measured seam).
B = ingress Loki 24 h count, **needing ≥5 in each of the two samples** to make `used`.
C = n8n executions in 72 h; measured only per n8n operation tuple, so it never stood behind an HTTP
row — no C branch was applied rather than fabricating one.
D = source manifest, row-count corroboration, RM whole-system scope.

**The matcher was proved before it was used.** Day 1 matched by hand and left no program, so the
same matcher was applied to both samples and had to reproduce day 1's recorded per-row counts:
64 of 66 B rows reproduce exactly. The other two are day-1 hand subtractions of its own probe
traffic (documented at `02-day2-method.txt` §7); one of them, `P-0460`, straddles the ≥5 threshold
and was therefore kept `rare`. The reproduction also **caught a real bug** — a trailing `/` compiled
to a wildcard, so `GET /` swallowed 5,139 bot-scan paths (17× overcount). Fixed before any day-2
number was used.

## The five A-hit contradictions (slice §9 STOP) — resolved

| id | surface | call sites | resolution |
|---|---|---|---|
| P-1224 | `POST /pm/cards/{planka_card_id}/ledger` | 29 workflows / 145 sites | **used** |
| P-1101 | `GET /pm/cards/{planka_card_id}/ledger` | 20 / 65 | **used** |
| P-1203 | `POST /decisions/{did}/project-copy` | 3 | dropped, `replacement:decisions.update` |
| P-1242 | `POST /principles/{pid}/project-copy` | 2 | dropped, `replacement:principles.update` |
| P-1247 | `POST /requirements/{rid}/project-copy` | 2 | dropped, `replacement:requirements.update` |

**The ledger is the consequential one.** SPEC §4 forbids `planka_card_id` as card *identity*, and
that is the only thing that should be dropped. What the 49 call sites actually use is an append-only
per-card record of claims and facts — `kind: claim|fact`, `source{list,node,execution_id}`, and a
laya verdict (`score`/`accepted`/`reason`). **That capability must exist at cutover, re-keyed to the
conveyor's own card id.** The new schema has no home for it yet (no ledger/fact/claim action;
`02-DATA-ARCHITECTURE` §7.2 drops `pm_card_ledger` for the key). Candidates are S15's
`work_package_memory` (card_id, run_id, kind, source, body) or `event`. ⚠️ **This is an open design
item for the owning slice, not a settled assignment.**

**Project-copy is the other resolution.** RM's `.../project-copy` is the fork step of an
inherit → copy-into-project → promote lifecycle (`requirements_management/src/routers/_governed.py:33`,
409 `inherited_asset_requires_copy`). The new model has **no inheritance** — the word appears nowhere
in SPEC, the data architecture or S07, and `governed_asset` carries `project_id` with one lifecycle —
so there is nothing to fork and a project-scoped update replaces the copy step.

## Default-drops decided (SUMMARY day-1 item 5)

| set | rows | basis |
|---|---:|---|
| fr `/api/research/**` | 58 | no A/B/C hit in any of the 348 flows; KAG substrate, and KAG is reference-only |
| fr `/api/tools/{coding,bridge,analytics}/**` | 20 | coding fleet not absorbed (D14); bridge retired with KAG; analytics derived from run/event |
| fr `/api/admin/{licenses,pricing}**` | 53 | "Never absorbed: licenses, billing_service"; source map says licensing/pricing "no" |
| planka aux (users, access-tokens, notifications, notification-services, terms, background-images, project-managers, config/test-smtp) | 25 | SPEC §4 duplicate notification/membership stores; Keycloak replaces Planka identity; S06 owns notifications |
| fr `/api/tools/{kanban,n8n}/**` | 4 | **kept** — live monitoring surfaces |

## In-cluster call opacity (day-1 item 7) — 24 rows `rare → used`

Ingress Loki cannot see pod-to-pod calls, so a 24 h zero there is not evidence of disuse. Resolved
from source instead: 11 `POST /api/webhooks/n8n/*` are posted by `n8n-nodes-fr`, and 13
`/api/internal/*` are reached by the fr node's api-client, by **billing_service**
(`project_owner_map.py:43 TENANT_MAP_PATH`) and by the **n8n external execution hook**
(`n8n-custom/hooks/flow-run.js`, installed via gitops `EXTERNAL_HOOK_FILES`).
`/api/webhooks/n8n/wip-slot` exists in node source but is **retired in fr** (2026-10-13) and is
correctly absent from the inventory — S05/S13 must not recreate it.

## Acceptance strings (day-1 item 4) — intentionally not "upgraded"

Slice §9 defines `probe:<METHOD> <path>` as a legal acceptance form and G3 only tests the prefix, so
the `used` rows already pass. Naming a `test:<crate>::<name>` for a slice that is not built (S09–S13)
would be inventing identifiers. Kept `probe:` and said so rather than manufacture 100+ fake names.

## PENDING-S0 — the 13 tables `02-DATA-ARCHITECTURE` §9 assigns to S0

Evidence: the 348-flow node census, the day-1 row counts, and a migrations grep.
**KEEP** — `custom_field`, `custom_field_value` (1815 / 70 Planka rows), `webhook_subscription`
(3 live rows: they are what trigger the conveyor), `project_stakeholder` (21 fr rows),
`conversation`, `conversation_turn` (S10 is planned, not dropped).
**DROP** — `audience`, `audience_member` (no `streams` operation in 348 flows; `fr.streams` 0; fr has
no `/api/streams` route), `deliverable` (`project_deliverables` 0, `project_milestones` 0),
`project_capability` (`pm_project_capabilities` 0), `register_item` (`pm_resources`/`pm_risks`/
`pm_benefits` all 0), `outline`, `outline_node` (`toc_*` all 0 in RM **and** `project_outlines` 0 in
fr, so the project-start path is empty too).

⚠️ **The finding worth acting on:** seven of those dropped tables had **already been built** —
`audience`/`audience_member` in `0005` (S06), `deliverable`/`register_item`/`project_capability` in
`0008` (S08 Part A), `outline`/`outline_node` in `0007` (S07). The spec's own order is S0 **first**
(`00-INDEX.md:171`), so those features were built before the inventory that was supposed to gate
them, and the inventory says they should not have been. Executed by migration `0011` (7 tables + the
24 actions that served them; registry 177 → 153) with the gitops-side docs reconciled. Dropping a
built table is a migration, not a doc edit — it is recorded here as a decision, not as a silent
omission.

## Gate (slice §7) — run from `$HOME/Movies/conveyor`

G1 unique ids 0 dup · G2 id shape 0 bad · G3 acceptance 0 bad · G4 surfaces ↔ parity empty diff ·
G5 1300 = 1300 · G6 ≥ 2 samples · G7 dropped-without-reason 0 ·
`check-jsonschema --schemafile parity/schema.json parity/parity.yaml` → ok.

⚠️ **G6 as written is vacuous** — it only counts files, and 11 existed after day 1. The real
assertion, run instead: 24 B-class rows are `used` and **0** of them lack the "≥5 in each sample"
justification. Asserted, not assumed.

## Limits — do not over-read

- Loki is the **ingress** log only. Pod-to-pod calls are invisible; `/api/internal/*` and
  `/api/webhooks/n8n/*` were resolved from source call sites (§ above), not from traffic.
- n8n `code` nodes (3,125) could make arbitrary HTTP calls. No `httpRequest` node exists in any flow,
  but this remains **UNMEASURED**.
- Planka row counts are not usage evidence (D3); the 3 webhook rows and 21 boards are infrastructure.
- `GET /api/projects` was 63,802/24 h in sample 1 but is a **burst** (40,902 at 09:00Z); it is `used`
  on A evidence (n8n `fr.projects.listProjects`) regardless.
- The external probe's 288 requests/day and day 1's own openapi probes appear in the counts for `/`
  and `/openapi.json`; both rows are handled explicitly in `02-day2-method.txt` §7.
- Day 2 was quieter than day 1 (73 surfaces hit vs 55) — that is traffic, not a dead collector, and
  the positive control proves it.
