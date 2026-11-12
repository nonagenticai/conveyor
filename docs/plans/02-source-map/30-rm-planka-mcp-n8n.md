# 30 — Source map: RM · Planka · mcp_server · n8n

Reference for S00, S05, S06a, S07, S08, S13. Symbols and paths only — no line numbers.
`UNVERIFIED` = not checked here; `PENDING-S0` = needs the live inventory.

Pinned 2026-11-07: requirements_management local HEAD `9ee72ab4e178` (5 behind `origin/main` `191086942ce5`),
kanban `aafb771cea4c` (= `origin/main`). All symbols and paths below were read from these local trees.

---

## 1. requirements_management

repo: `$HOME/Movies/requirements_management` · sha: `9ee72ab4e1785c5bba636df025d8e06d73eef2a5` · slices: [S07, S08]

Measured: 28 router modules / 33 router objects, **314 route decorators**, **56** `__tablename__`
literals. Mounts are unprefixed and unversioned (no `/api` anywhere in RM).

| path | symbols | what it is | slice | carry / do-not-carry |
|---|---|---|---|---|
| `src/routers/__init__.py` | `register_routers()` | the mount map; static/tool first, wildcard CRUD last | S07/S08 | carry as the domain inventory |
| `src/routers/requirements.py` | `router`, `/requirements` | requirement CRUD + `/{rid}/version`, `/versions`, `/versions/{v1}/compare/{v2}`, `/promote`, `/project-copy`, `/quality`, `/priority` | S07 | carry entity + revision/compare; drop promote/project-copy machinery |
| `src/routers/principles.py` | `router`, `/principles` | principles with the same lifecycle | S08 | carry |
| `src/routers/decisions.py` | `router`, `/decisions` | ADRs, same lifecycle | S08 | carry |
| `src/routers/principle_conflicts.py` | `router`, `/principle-conflicts` | persisted conflict verdicts, read + manual dismiss | S08 | carry table; the reconciler is already gone |
| `src/routers/toc.py` | `router`, `/toc` | Dettmer LTP trees, nodes, necessity/sufficiency edges, graph, quality, `/config` | S08 | carry trees as `outline_node`; drop `toc_trees/nodes` split |
| `src/routers/search.py` | `router`, `/search`, `/semantic_search` | FTS + semantic across entities | S07 | carry as `search` module; eval set first (R2) |
| `src/routers/bulk.py` | `router`, `/bulk`, `/requirements|principles|decisions/bulk` | multi-entity batch ops | S07 | do-not-carry — registry invocations replace it |
| `src/routers/pm_programmes.py` | `router`, `/pm/programmes` | programme portfolio | S08 | carry |
| `src/routers/pm_projects.py` | `router`, `/pm/projects` | project CRUD + brief versions, capabilities/events, coach-actions, criteria, close (`_is_fr_close`) | S06/S08 | carry project; drop the fr-only close special case |
| `src/routers/pm_card_ledger.py` | `router`, `/pm/cards/{planka_card_id}/ledger` | append-only facts/claims keyed by **Planka card id** | S06 | do-not-carry as an external-id table (SPEC §4) |
| `src/routers/pm_resources.py` · `pm_risks.py` · `pm_benefits.py` | `router` | resource, risk and benefit registers | S08 | carry |
| `src/routers/pm_catalogue.py` | 12 routers: `project_phases`, `capabilities`, `card_types`, `project_types`, `catalogue`, `recipes`, `wip_limits`, `priority_levels`, `deliverable_kinds`, `milestone_kinds`, `gates`, `board_templates` | the vocabulary projects are composed from; `catalogue` imports defaults | S06/S08 | collapse to `catalogue_entry(kind, code, label, meta)` — this is the ~20-table sprawl of SPEC §4 |
| `src/routers/architecture_elements.py` · `_relationships` · `_views` · `_standards` · `_containers` | `router` per module | ArchiMate element/relationship/view/standard store; containers feed fr's queue selector | S08 | carry model; views become flattened |
| `src/routers/architecture_tools.py` | `router`, `/architecture/tools` | traceability matrix, gap/impact analysis, ADM completeness, capability heatmap, compliance, conformance, derive-relationships, generate-view | S08 | carry as read-only computations |
| `src/routers/architecture_metamodel.py` | `metamodel_router`, `viewpoints_router`, `rulesets_router` | ArchiMate 3.2 reference data generated from Archi 5.10.0 + rule catalogue | S08 | carry `src/data/archimate/3.2/registry.json` as data |
| `src/routers/modeling_models.py` | `router`, `validation_runs_router`, `change_sets_router`, `trace_links_router` | language-neutral model store: revisions, publications, reviews, validation runs/findings, change sets, trace links | S08 | do-not-carry the five-table sprawl → one `revision` |
| `src/routers/modeling_exchange.py` | `exchange_router`, `view_render_router` | ArchiMate exchange import/export + view render | S08 | carry as `render` |
| `src/routers/modeling_document_export.py` | `router` | one PDF page / zipped SVG per view or diagram | S08 | carry |
| `src/routers/uml_models.py` | `router`, `revision_render_router` | UML 2.5.1 subset: elements, relationships, interactions, diagrams, objects, connections, render | S08 | carry (flattened) |
| `src/routers/uml_tools.py` | `router` | stateless UML validation | S08 | carry as a pure domain check |
| `src/routers/integration_tools.py` | `router`, `/integration/tools` | `gap-to-workpackages`, `requirement-trace` | S07 | carry requirement→card trace |
| `src/routers/health.py` | `router` | `/health`, `/health/live`, `/health/ready`; public | S02 | carry |

**Models (`src/models/`).** Tables: `requirements`, `principles`, `decisions`,
`principle_conflicts`, `quality_evaluations`, `idempotency_keys`; `toc_trees`, `toc_nodes`,
`toc_settings`; `architecture_elements`, `architecture_relationships`, `architecture_views` (+
`_view_objects`, `_view_connections`), `architecture_standards`, `architecture_element_realisation_bindings`;
`modeling_models`, `modeling_model_revisions`, `modeling_publications`, `modeling_reviews`,
`modeling_validation_runs`, `modeling_validation_findings`, `modeling_change_sets`, `modeling_trace_links`;
`uml_elements`, `uml_relationships`, `uml_interactions`, `uml_diagrams` (+ objects, connections);
and the `pm_*` family — projects, programmes, resources, risks, benefits, the catalogue set in §1,
card ledger, brief versions, coach actions and the capability/phase event tables.

**`entity_version_history`** (`src/models/version_history.py`) is the one table with **no `tenant_id`**,
excluded from RLS and reached only via scoped parents. Do not carry the table or its policy exception;
one `revision` row per mutation covers it (SPEC §4).

---

## 2. kanban (Planka)

repo: `$HOME/Movies/kanban` · sha: `aafb771cea4c3e9f7a51363fd4a3920b2f719936` · slices: [S05, S06]

**Identity correction — load-bearing.** This checkout is `nonagenticai/kanban-mcp` (`mcp-kanban`
0.1.0), an MCP wrapper over Planka 2.0 — **not** the `planka-private` fork. `planka/` holds only
`.env`/`.vscode` and is EMPTY otherwise; there is no `server/db` or model source on this machine.
The deployed image is `registry.example.com:5000/planka-private:latest` (gitops
`gitops-production/clusters/strato/services/kanban/deployment.yaml`), built elsewhere. ⇒ **The fork's source is not
local, so S05 must port from two measured sources, not from code:** (1) `docs/openapi.json`,
**PLANKA API `2.0.0-rc.4`, 67 paths**, base `/api` — the wire contract; and (2) the **live
`kanban` Postgres schema**, introspected at S05 — it is the **column/FK/index authority**;
`50-schema-inventory.md` carries the 33 table names, not their columns. Anything the fork does that neither
reveals is `UNVERIFIED` and must be measured at S05 against the running instance, not guessed.

| path | symbols | what it is | slice | carry / do-not-carry |
|---|---|---|---|---|
| `docs/openapi.json` | `Card`, `List`, `Board`, `TaskList`, `CustomFieldValue`, `Label`, `Comment`, `BoardMembership` | the authoritative field shapes | S05 | carry as port source |
| `/projects`, `/projects/{id}`, `/projects/{projectId}/boards`, `/boards/{id}` | `Board` | project → board | S05 | carry; kills the board↔project round-trip |
| `/boards/{boardId}/lists`, `/lists/{id}`, `/lists/{id}/sort`, `/clear`, `/move-cards` | `List` = `{boardId, type, position, name, color}` | the lists | S05 | carry; `List.type` is what SPEC §6's `kind` maps onto |
| `/cards/{id}`, `/lists/{listId}/cards`, `/cards/{id}/duplicate` | `Card` = `{listId, prevListId, isClosed, listChangedAt, dueDate, isDueCompleted, stopwatch, position, name, description, type}` | the cards | S05 | carry; **no `state` column** — state is `listId` + `isClosed` |
| `/boards/{id}/labels`, `/cards/{cardId}/card-labels` | `Label` | labels | S05 | carry |
| `/board-memberships/{id}`, `/boards/{boardId}/board-memberships` | `BoardMembership` = `{role, canComment}` | board memberships | S05/S06 | carry |
| `/cards/{cardId}/comments`, `/comments/{id}` | `Comment` = `{cardId, userId, text}` | comments | S05 | carry (single store) |
| `/cards/{cardId}/custom-field-values/…`, `/custom-field-groups/…`, `/custom-fields/{id}` | `CustomFieldValue` = `{cardId, customFieldGroupId, customFieldId, content}` | custom field values | S05 | carry values; flatten groups/fields |
| `/webhooks`, `/webhooks/{id}` | `events`, `excludedEvents`, `url`, `accessToken` | instance-wide, admin-only webhooks | S05 | carry payload+semantics; drop admin-only instance scoping |
| `/cards/{cardId}/task-lists`, `/task-lists/{taskListId}/tasks`, `/tasks/{id}` | `TaskList` | task lists | S05 | carry |
| card move | `PATCH /cards/{id}`, `/lists/{id}/move-cards` | list change | S05 | carry as `cards.move` |
| due dates | `Card.dueDate`, `isDueCompleted` | due dates | S05 | carry |
| stopwatch | `Card.stopwatch` (`startedAt`, `total`) | per-card time | S05 | carry **read-only** |
| `/cards/{cardId}/attachments`, `/attachments/{id}`; `/cards/{cardId}/card-memberships` | — | attachments; card-level memberships | — | **OUT** (SPEC §2) |
| `/notifications`, `/notification-services`, `/terms/{type}`, `/users`, `/access-tokens`, `/config/test-smtp`, `/background-images`, `/project-managers` | — | not in the used-feature list | — | S0 decides; default drop |

**Webhook payload (S05 ports this).** Planka POSTs `{event, data, prevData, user}` to the webhook URL,
with `Authorization: Bearer <accessToken>` when the webhook has one. Event names are strings such as
`cardCreate`, `cardUpdate`, `cardDelete`; on the wire `events` / `excludedEvents` are
**comma-separated** (the REST API takes/returns arrays). Sources: `operations/webhooks.ts`,
`mcpServer.ts` (`mcp_kanban_webhook_manager`). `UNVERIFIED`: the per-event `data` envelope — it lives
in the fork, which is not checked out here.

---

## 3. nonagenticai_mcp_server (the S06a conformance gate)

repo: `$HOME/Movies/nonagenticai_mcp_server` · sha: `dd2cabf6c2de6b2f9ac16917970c52eab1c7ad57` · slices: [S06a, S04, S14]

| path | symbols | what it is | slice | carry |
|---|---|---|---|---|
| `src/external/signing.py` | `sign`, `verify`, `_digest`, `SKEW_SECONDS`, `SIGNATURE_HEADER`, `TIMESTAMP_HEADER`, `IDEMPOTENCY_HEADER` | the HMAC scheme, both directions | S06a | reimplement byte-identically |
| `src/external/fr.py` | `FrAdapter`, `FrCommandAdapter`, `INBOUND_PATH`, `TEXT_LIMIT`, `FR_COMMANDS`, `decision_text`, `FrRefusedError` | outbound client: envelope, grammar, status mapping | S06a | carry |
| `src/routers/fr_channel.py` | `events`, `verify`, `EVENT_TYPES`, `START_ACTION`, `START_ENDINGS`, `_approval`, `_message` | inbound events receiver, 204 semantics | S06a | carry |
| `tests/test_fr_adapter.py`, `tests/test_fr_channel_events.py` | sign/verify roundtrip; 401/413/503/204, replay, tombstone, first-report-wins | pin both directions | S06a | port as the conformance suite |
| `src/config.py` | `fr_base_url`, `fr_channel_secret`, `env_prefix="MCP_"` | env names | S06a | `MCP_FR_BASE_URL`, `MCP_FR_CHANNEL_SECRET` |

**HMAC (exact).** Canonical string is `<unix-seconds-timestamp>.<raw-body-bytes>`, HMAC-SHA256 keyed
by the shared secret, hex, prefixed `sha256=`. Headers: `X-Channel-Timestamp` (ASCII digits) and
`X-Channel-Signature: sha256=<hex>`; the sender also sets `Idempotency-Key` (a uuid4) but this
receiver does **not** validate it. Tolerance is **±300 s** (`SKEW_SECONDS = 300`). The body is
verified as **raw bytes as transmitted** — re-serializing breaks it; header lookup is
case-insensitive. Secrets: fr side `FR_MCP_CHANNEL_SECRET`, mcp side `MCP_FR_CHANNEL_SECRET`;
base URL `MCP_FR_BASE_URL`.

**Inbound (mcp → fr).** `POST {MCP_FR_BASE_URL}/api/webhooks/mcp/incoming`. JSON envelope
`{address, text, correlation_id}` plus optional `job`, `on_behalf_of`, serialized compact and
`sort_keys=True`. Grammar (text, max **4000** chars, fr `MAX_TEXT`):
`projects.create {json}` (`title` and `project_type` required; passthrough `description`,
`problem_statement`, `desired_outcome`, `repository_url`), `projects.get {"id"}`, `projects.close {"id"}`,
`approve <approvalId> [comment]`, `reject <approvalId> [reason]`. Identity is the envelope `address`:
an `agent:<tenant_id>` address must equal the run's tenant (else `actor_tenant_mismatch`).

**Response vocabulary** — HTTP 200 body `status` ∈ `result` | `pending-approval` | `no-action` |
`error` | `unknown-sender`; plus `result.project_id`, `approvalId`, `code`. Client mapping:
`unknown-sender` → `actor_unresolved`; `no-action`/`error` → sanitized `code` (`^[a-z_]{1,64}$`);
non-JSON or non-object → `external_result_invalid`; 5xx → `fr_unavailable`; other HTTP → `fr_rejected`.

**Outbound (fr → mcp).** `POST {MCP_EVENTS_URL}/api/channels/fr/events`, same signing. ⚠️ The outbound URL is
**`MCP_EVENTS_URL`**, not `MCP_FR_BASE_URL` — the latter points at fr itself, so a sender built on it has no
destination. Measured 2026-11-07: fr's backend deployment carries `MCP_EVENTS_URL=http://mcp.mcp.svc.cluster.local:8033/api/channels/fr/events`. Body `type` ∈
`approval.requested` | `approval.closed` | `message`; **all three also carry `type` and `address`**. Measured from
the emitter (`fr/backend/src/domain/engagement/adapters/mcp.ts:34-67`, pinned `6f7c5097`):
`approval.requested` carries `approval_id, project_id, tenant_id, correlation_id, kind, description,
change_summary, impact_analysis, proposed_answer, proposed_answer_note, allowed_decisions, deblock_actions,
expires_at, text` (`change_summary`/`impact_analysis`/`proposed_answer`/`proposed_answer_note`/`deblock_actions`
optional; `allowed_decisions` is `['approved','rejected']`; `text` is the numbered menu every channel sends);
`approval.closed` carries `approval_id, project_id, tenant_id, correlation_id, final_status`;
`message` carries `correlation_id`, `kind` (`approval-request` | `action-result` | `action-error` | `text` |
`report`), `text` and `meta` (`approvalId`, `actionName`, `result.project_id`, `outcome`).
The mcp receiver reads only `approval_id`, `expires_at` and `project_id`/`correlation_id` (the last two via
`_runs`/`_record`); the remaining fields are carried for parity, not consumed.

**204 + idempotent close.** Every accepted event answers **204 with an empty body** — including
malformed JSON and unknown `type`. Errors are 413 (`payload_too_large`, >64 KiB), 503
(`channel_unconfigured`), 401 (`invalid_signature`). Close is idempotent in both directions:
`inputs.close_everywhere(approval)` fans out and a per-run tombstone means a late `approval.requested`
for a closed approval is ignored; `_record` is a conditional `UPDATE` that only writes when
`external_ref` has neither `project_id` nor `start_outcome` — **first report wins**. A start approval
closed as `rejected`/`expired`/`voided` records `start_outcome`; an approved start records `project_id`.

**Cutover changes** are exactly two values: `MCP_FR_BASE_URL` and the secret. Nothing else in
mcp_server changes.

---

## 4. n8n node packages

repo: `$HOME/Movies/n8n_separated_modules` (plain bundle dir, **no `.git`**) · sha: `n/a` — per-clone:
`n8n-nodes-fr a6ac51d`, `n8n-nodes-kanban eca73b4`, `n8n-nodes-requirements 13d94c6`,
`n8n-nodes-coding 0abcea3`, `n8n-nodes-artifacts f90f1da`, `n8n-nodes-mcp da2890f`,
`n8n-nodes-workflows a6dfd9b`, `n8n-nodes-observability 09a515f`, `license-validator 0d27fc3`;
scripts repo `$HOME/Movies/n8n_module_scripts b7dad4f` · slices: [S13, S04, S00]

SPEC §2 claims 24 `n8n-nodes-*` repos; **8 are cloned here** (`ls -d n8n_separated_modules/n8n-nodes-* | wc -l`
= 8: artifacts, coding, fr, kanban, mcp, observability, requirements, workflows), **the other 16 are `PENDING-S0`**
(org repo listing / npm). `license-validator` is also local but is **not** an `n8n-nodes-*` package — 9 local
packages in total, 8 of them `n8n-nodes-*`. ⛔ S13/S04 must not infer that an operation exists or does not exist
from this table: a package that is not cloned is **unmeasured**, not absent.

| path | symbols | what it is | slice | carry / do-not-carry |
|---|---|---|---|---|
| `n8n-nodes-fr/nodes/Fr/resources` | `andon`, `approvals`, `auth`, `cards`, `comments`, `decompositions`, `documents`, `katas`, `metrics`, `projects`, `webhooks`, `_legacy` | the fr write surface as n8n sees it | S04 | use as the action-name source; drop `_legacy` |
| `n8n-nodes-kanban/nodes/Planka/operations` | `action`, `attachment`, `baseCustomFieldGroup`, `board`, `card`, `comment`, `config`, `customField`, `customFieldGroup`, `customFieldValue`, `label`, `list`, `membership`, `notification`, `notificationService`, `project`, `projectManager`, `task`, `taskList`, `terms`, `user`, `webhook` | every Planka call the flows make | S05 | keep the in-scope subset only |
| `n8n-nodes-requirements/nodes/Requirements/resources` | 40 resources mirroring the RM routers (`requirements`, `principles`, `decisions`, `toc`, `architecture-*`, `modeling`, `pm-*`, `uml`, `search`, `quality`) | RM as n8n sees it — a cross-check on the §1 mount map | S07/S08 | confirms no RM domain is missed |
| `n8n-nodes-coding/nodes/Coding/resources` | ~60 resources (analysis, workspaces, parity, security, jobs) | coding fleet, **not absorbed** (D14) | — | external actions via OpenAPI ingest |
| `n8n-nodes-artifacts/…/Artifacts` | `artifacts` | not absorbed | — | external action |
| `n8n-nodes-mcp/…/Mcp` | `audit`, `tools`, `tool-versions`, `default` | MCP gateway client | S04 | reference only |
| `n8n-nodes-workflows/…/Workflows` | `execution`, `webhook`, `workflow` | n8n managing itself | S13 | dies with n8n |
| `n8n-nodes-observability/…/Observability` | `grafana`, `loki`, `metering`, `prometheus`, `pushgateway` | read-only display | S09 | external |
| `license-validator` | credential helper | node licence auth | S04 | replaced by `RequireLicence` |

**Counts CONFIRMED against the live DB (2026-11-07, `sudo -u postgres psql -d n8n`):**
`workflow_entity` = **348 rows, 332 active** (`activeVersionId` set on 332). Measured split: **280** card-type ×
list matrix flows (**276 active**; 70 each of `.READY`/`.DOING`/`.QA`/`.DONE`), **51** `cap.*` (51 active),
**10** `zz.*` (**0 active**), and **7 other** — 5 active (`conveyor.start`, `ask_fr`, `wip.pull`, `kata.open`,
`err.card`) and 2 inactive (`dispatcher`, a `PROBE` workflow). The remaining S0 task is the **per-flow operation
census**: record, per flow, the fr/RM/Planka operations it calls by parsing `workflow_entity.nodes` (json).
⚠️ Use the DB, not the public API — the API paginates at 250.
Node-type census across all 348 (2026-11-07): `code` 3113, `kanban.planka` 2904, `if` 1244,
`langchain.agent` 941, `executeWorkflow` 915, `coding.codingTool` 756, `requirements.requirements` 746,
`coding.coding` 586, `requirements.requirementsTool` 430, `artifacts.artifactsTool` 372, `webhook` 287.
