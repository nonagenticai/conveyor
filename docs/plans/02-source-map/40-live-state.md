# 40 — LIVE STATE measured 2026-11-07 (read-only)

`slices: [S00, S05, S14]` · **No repository SHA applies to this file.** Every number below is a live reading
of 2026-11-07 with the command that produced it; re-measure before relying on any of it.

Measured while writing the plan set, from `mini3`. Every number below is a reading with its command — re-measure
before relying on it; none of it is a standing fact.

## Loki (the S0 access-log source)

- Service: `loki.loki.svc.cluster.local:3100`, ClusterIP `192.0.2.11`.
- Labels: `container`, `filename`, `job`, `namespace`, `pod`, `service_name`, `stream`. Only job value: `kubernetes-pods`.
- Ingress access logs live at `{namespace="istio-system", container="istio-proxy"}`. Line shape (measured):
  `[ts] "GET / HTTP/1.1" 200 - via_upstream - "-" 0 4540 86 58 "<x-forwarded-for>" "<ua>" "<request-id>" "fr.nonagentic.ai" ...`
  The host and path are plain quoted fields; per-path counts are a LogQL metric query over that selector.
- ⛔ **Retention is `24h`** (`gitops-production/clusters/strato/services/loki/loki/configmap-loki-config.yaml`, `retention_period: 24h`,
  config revision `2026-10-14-retention-24h-2gb-cap`). **The access log is a 24-hour sample, not a history.**
  S0 must label its evidence accordingly; it cannot answer "used at all this quarter" from Loki.
- Working query template (run from master):
  `curl -sG http://192.0.2.11:3100/loki/api/v1/query_range --data-urlencode 'query={namespace="istio-system",container="istio-proxy"} |= "fr.nonagentic.ai"' --data-urlencode "limit=2" --data-urlencode "start=$(( $(date +%s) - 3600 ))"`

## n8n

- Database `n8n` on master Postgres (direct `sudo -u postgres psql -d n8n`; never through pgbouncer).
- `workflow_entity`: **348 rows, 332 active** (confirmed 2026-11-07). `nodes` is `json`, `activeVersionId` set on 332.
- `execution_entity`: only **470 rows, 2026-11-04 15:27 → 2026-11-07 09:56** — execution history is pruned. It is
  **not** a usage history either.
- Node-type census (all 348 workflows, `jsonb_array_elements(nodes)`): `code` 3113, `@nonagentic/n8n-nodes-kanban.planka`
  2904, `if` 1244, `stickyNote` 1021, `langchain.agent` 941, `langchain.lmChatOpenAi` 941, `executeWorkflow` 915,
  `coding.codingTool` 756, `requirements.requirements` 746, `coding.coding` 586, `requirements.requirementsTool` 430,
  `artifacts.artifactsTool` 372, `webhook` 287. **The flow JSON is the durable record of which operations exist** —
  this, not execution history, is S0's primary n8n evidence.

## Planka (`kanban` database)

- Only one deployment exists (`kanban` namespace, 181 d old). No second instance anywhere in the cluster.
- Live data (2026-11-07): `project=4 board=4 list=12 card=5 comment=5 task=0 label=0 webhook=3 custom_field_value=0`.
  Every card is an `[agent-contract-test]` card. A `_conveyor_reset_20260922` table exists (a 2026-09-22 reset marker).
- ⛔ **Do not derive feature usage from Planka row counts** — the instance holds test data only. Derive from the
  fork's schema/feature set and from the call sites in fr and the n8n flows.

## fr

- Database `fr` on master Postgres: **86 domain tables** (88 base tables in `public`, 90 relations including two
  `pg_stat_statements*` views). Query: `select count(*) from pg_tables where schemaname='public'` (88) and
  `select relkind, count(*) from pg_class c join pg_namespace n on n.oid=c.relnamespace where n.nspname='public'
  and c.relkind in ('r','p','v','m') group by relkind` (`r` 88, `v` 2); domain = 88 − 2 bookkeeping
  (`_migrations`, `_migration_effects`).
- Conveyor-shaped tables exist (`card_drafts`, `card_phase_lease`, `approvals`, `approval_decisions`,
  `approval_callback_urls`, `conveyor_flow_runs`, `project_conveyor_start_attempts`, `card_contracts`, …), but live
  rows are sparse because the conveyor is idle: `approvals=0`, `approval_decisions=0`, `card_drafts=0`,
  `card_phase_lease=0`, `project_conveyor_start_attempts=0`; `conveyor_flow_runs=693`.
- Consequence: fr's **schema** is the evidence for which concepts exist; its **row counts** only prove which ran.

## RM

- Database `requirements_api`. Catalogue data is real and large: `pm_card_types=293`,
  `pm_project_type_card_types=282`, `pm_capabilities=68`, `pm_project_type_phases=46`, `pm_project_phases=32`,
  `pm_wip_limits=26`, `pm_project_types=26`. These counts are `pg_stat_user_tables` estimates — re-count with
  `count(*)` before quoting in `parity.yaml`.

## MCP channel wiring (fr ↔ nonagenticai_mcp_server)

- **fr backend deployment** carries `MCP_EVENTS_URL = http://mcp.mcp.svc.cluster.local:8033/api/channels/fr/events`
  — fr POSTs its signed approval/message events there (`gitops-production/clusters/strato/services/fr/deployment.yaml`).
- **mcp external driver** listens on **8036** (`containerPort`) and carries
  `MCP_FR_BASE_URL = http://fr-backend.fr.svc.cluster.local:8888` (`gitops-production/clusters/strato/services/mcp/external-driver-deployment.yaml`).
- Measured 2026-11-07; both are deployment values, so re-read the manifests before relying on them.

## What this changes in the SPEC

§9 assumed (1) ingress logs answer per-path usage, (2) n8n execution data answers which operations ran, (3) the
Planka feature table is a usage source. Measured: (1) is a 24-hour sample, (2) is a 3-day sample, (3) is test data.
S0 must therefore rank evidence: **flow JSON + code call sites (durable) > 24 h Loki sample > execution sample >
row counts**, and mark every `used` verdict with the evidence class that produced it.
