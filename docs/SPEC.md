# SPEC — `conveyor`: one Rust application

**Repo:** `nonagenticai/conveyor` · **Namespace:** `conveyor` · **Host:** `conveyor.nonagentic.ai` · **Keycloak realm:** `conveyor` · **Licence app_key:** `conveyor` · **Port:** 8930 (verify free locally and in-cluster before assigning)
**Date:** 2026-11-07 · **Status:** draft for review · no code written
**On approval:** this file becomes `docs/SPEC.md` in `nonagenticai/conveyor` (first commit) and is registered in `~/.claude/plans/program/INDEX.md`.

---

## 1. Context

The conveyor is spread across four applications, a package fleet and a flow engine. Every card event crosses at least three of them, and the latent bugs live at those seams, not inside any one app.

Measured 2026-11-07 (read-only; `origin/main` and the live cluster):

| System | Shape | Size | Data | API |
|---|---|---|---|---|
| **fr** | SvelteKit 5 + Bun/TypeScript backend | 7,046 files; 658k LOC TS (backend 362k, frontend 96k, generated clients 198k); 108k LOC Svelte | **86 domain tables** (88 base tables, 90 relations incl. 2 `pg_stat_statements*` views), 224 migrations, Postgres via PgBouncer | 1,376 endpoint declarations over 548 paths, all `/api/...`; 785 served operations over 616 paths in `served-spec.lock.json` |
| **requirements_management** | FastAPI + SQLAlchemy 2 async | 553 files, 150k PY LOC | **57 domain tables** (58 base incl. `alembic_version`), 82 Alembic revisions | 314 routes, mounted at `/requirements/...` |
| **Planka** (`planka-private`, forked image) | Node + its own React UI | own namespace `kanban`, own DB + Redis | card/list state — **the system of record** for work items | REST, consumed by fr and by 317 n8n flows (312 active) |
| **procedure_management** | FastAPI, archived 2026-11-04 | 239 PY files | ~13 tables (procedures, steps, PDL-style pre/postconditions) | 54 endpoints, **no live consumer** |
| **n8n nodes** | 24 `n8n-nodes-*` repos (**8 cloned locally**; the ninth local package, `license-validator`, is not one of the 24) | TypeScript packages | — | wrap fr / RM / Planka / coding APIs |
| **n8n flows** | live DB | 348 workflows, 332 active — 280 card-type×list (276 active), 51 `cap.*` procedures, 10 `zz.*` (0 active), 7 other (5 active: `conveyor.start`, `ask_fr`, `wip.pull`, `kata.open`, `err.card`; 2 dead) | — | webhooks + executeWorkflow |

The failures this produces are structural, not local:

- **Identity is split.** A card's state is in Planka, its lease and approvals are in fr, its project in RM. **280 active flows** — 276 of them the card-type×list matrix, 4 of them `cap.*` procedures — call `getProjectByPlankaBoard` on every event just to recover the project (measured 2026-11-07, `workflow_entity.nodes::text LIKE '%getProjectByPlankaBoard%'`; 284 rows include 4 inactive).
- **Duplicate stores.** Two comment stores, two notification stores, three membership notions (Planka board members, fr `user_roles`, Keycloak claims), three tenant resolutions.
- **Stringly-typed contracts.** 92 fr capability modules (21.5k LOC) exist largely to validate payloads that never had a type; n8n flows exchange JSON with no schema.
- **Schema history as archaeology.** 224 fr migrations and 82 RM revisions encode decisions nobody can reconstruct; one fix often needs a migration in one system and a payload change in another.

The conveyor works, but each fix is a cross-system change. Tenant ambiguity, card-state races, approval delivery and lease leaks are **class** problems under this architecture, not bugs to patch one at a time.

## 2. What this spec proposes

**One repo, one service, one Postgres database, one binary** (subcommands `serve`, `migrate`, `worker`, `admin`), in safe Rust on the estate's existing standard (`template-rust-service`: Rust 1.99, edition 2024, axum 0.8, sqlx 0.9, utoipa, Postgres).

Absorbed:

- **fr entirely** — backend and UI: tenant/workspace admin, projects, cards, leases, approvals, andon, runs, comms/multicast, notifications, documents, repositories, billing/metering display, licence gate, secrets, Copilot/AG-UI, **the three operating channels (web app, Telegram, MCP)**, and the machine API other apps use.
- **All of RM** — requirements, catalogues, revisions, quality scoring, principles/ADRs, enterprise architecture (ArchiMate 3.2), UML 2.5.1 modelling, portfolio stakeholders, search, renders/exports. **Not** TOC, PM registers, project capabilities or deliverables — measured at zero usage by S00 and dropped (see D3's note).
- **The Planka features actually used** — boards, lists, cards, labels, board memberships, comments, custom field values, webhooks, task lists, card moves, due dates, read-only stopwatch. **Not** attachments, **not** card-level memberships.
- **The n8n node packages** — collapsed into one in-app **action registry** exposed over HTTP and MCP.
- **The procedure concepts**, redesigned from scratch (no PDL preconditions/effects port).
- **`conveyor-bench`** — the SWE-bench measurement harness, rewritten in Rust as part of this app (D24): the dataset is a hashed seed artifact, the grader is a parity-ported verdict engine, the sandbox is a k8s Job, and the old Python repo is frozen and archived. The benchmark is the kata improvement loop's instrument.
- **Later phase, same spec:** n8n itself — an in-app **process engine** (capability → process → step → procedure/SOP → task/tool), a **process-map view** (the user-facing DAG) and a stage/map editor, re-authoring the 348 flows as retained regression fixtures (no retirement gate). Execution is an ordered list of steps; each step has a **procedure (SOP)** with definition of ready, definition of done, acceptance criteria and a laya/eval inspection (D28). The 280 card-type × list processes are a tenant-facing **default catalogue**, and tenant-defined capabilities/processes are first-class (D29). The n8n DAG is not the execution model (D30). **Built 2026-11-10:** the **process is the top-level executor**; the **workflow engine is retained** as the tool-sequence executor (owner ruling 2026-11-09) — a process `call` step resolves `child_process_key` process-first, workflow-second — and the default catalogue ships (70 type files / 280 processes / 350 SOPs).

Not absorbed: `licenses`, `billing_service`, `inference-router`, `worker_management`, `artifacts`, the `coding-*` fleet, **`agent`** and **`nonagenticai_mcp_server`**. They stay external and are integrated by API; coding services keep their own action registry generated from their `openapi.json`.

Two of those are live consumers of fr and are the reason the cutover is a contract change, not just a deploy:
`agent` runs the conveyor's project lifecycle, and at cutover it reaches the conveyor over **the conveyor's own MCP
surface** (§4.5). That surface is claimed and tested as a first-class cutover gate — the deployment must not silently
break the agent platform — while state transitions remain status-guarded, not version-guarded (CONTRACT §3).

## 3. Decisions already taken (George, 2026-11-07)

| # | Decision | Answer |
|---|---|---|
| D1 | Build location | New repo, new gitops deployment (new namespace, DB + DB user, image) |
| D2 | Parity target | "The stuff we actually use", re-architected — old decisions need not be carried over |
| D3 | RM scope | **All** of RM, including ArchiMate/UML/TOC/PM — **bounded by D2 and the S0 inventory; see the note under this table** |
| D4 | fr scope | Full backend and the API other apps use — not only the web UI |
| D5 | UI | **Rust UI**; spike first to choose Leptos SSR vs a WASM SPA |
| D6 | n8n | **Later phase in this spec** — an in-app **process engine**: capability → process → step → procedure (SOP) → task/tool. Execution is an ordered list of steps; each step has a **procedure (SOP)** with a definition of ready, a definition of done, acceptance criteria and an eval/inspection; the **DAG is the user-facing process map** of processes and participating capabilities, not the executor (D30). A stage/map editor in the Leptos app; `egui_xyflow` is not adopted. Amended 2026-11-09 |
| D7 | Data | **Fresh start, discard** the old databases |
| D8 | Cutover | Full cutover when finished; old system **frozen** (bug fixes only) during the build |
| D9 | Local-first | Local = development and proof; CI is not the iteration loop |
| D10 | Procedures | **Redesign from scratch** |
| D11 | API | **No backwards compatibility**; new paradigm; n8n node modules in scope |
| D12 | Licences/billing | Stay separate repos; the new app gets its own app-level licence; absorbing them is future work |
| D13 | Identity | Keep Keycloak; **new realm** dedicated to this app |
| D14 | Adjacent services | Not absorbed (worker_management, artifacts, coding-*) |
| D15 | Name | `conveyor` for repo, namespace, host and realm |
| D16 | Ingress host | **`conveyor.nonagentic.ai`** — a new host; `fr.nonagentic.ai` remains the old app's host; the conveyor coexists with it and never requires the old app to stop |
| D17 | Channels | **Web app, Telegram and MCP are all in scope** as first-class channels — not a REST API with a web UI bolted on. S06d generalises them into one **connector standard** (see D28) |
| D18 | Agent interface | ~~`nonagenticai_mcp_server` is the permanent interface between the conveyor and the `agent` app~~ — **SUPERSEDED by D19 (2026-11-08)**. Its one surviving rule: **a question is an approval** |
| D19 | Agent interface | **`agent` calls the conveyor's own MCP surface (`/mcp`)** — same wire, same action registry, no HMAC channel, no second gateway. `nonagenticai_mcp_server` remains the agent platform's tool gateway for every **other** backend (coding, care-bus, builder); its `fr` toolkit retires with fr at cutover. A licence bearer resolves to `app_user(kind='agent')`, and the decision records `decided_by_kind='agent'` |
| D20 | Capability, knowledge, memory | **Capability is a first-class aggregate** with a `CapabilityAgent` executor. **Knowledge** is procedural (the `capability → process → step → procedure (SOP) → action\|gate` ontology, stored as `process`/`process_step`/`procedure`) and domain (`strata_rag` platform packs, one `capability` pack, pinned per capability). **Memory** is procedural (execution history: run/approval/invocation) and work-package (hot rows in the conveyor + cold consolidated digests in `strata_rag`). `strata` is never the system of record. **Amended 2026-11-09:** a capability is a **knowledge area**; within it, **process agents** (one per process/card type) hold the process domain knowledge and **tool agents** (one per toolset) make the tool calls. The process agent decides; the tool agent executes. **A capability's kind is `system` (`project_management`, `kanban_coach`, `kata` — always present, not deletable) or `tenant` (defined per project type).** A **process** is an ordered sequence of steps; every step has a **procedure (SOP)** — definition of ready, definition of done, acceptance criteria and an eval/inspection — and a process declares its **user-facing outputs**. The **process map** is the user-facing DAG (D30) |
**D3 as executed (2026-11-08).** D3 and D2 are read together, not in tension: D2 makes parity "the stuff we
actually use", and §8's S0 gate drops zero-usage surface rather than porting it. S00 day 2 measured RM's TOC
(`toc_trees`/`toc_nodes`/`toc_settings`, 0 rows, no consumer in any of the 348 flows) and its PM registers,
capabilities and deliverables (`pm_*` all 0, no route) at **zero usage**, so those five tables and the 24 actions
that existed only to serve them were dropped by migration `0011` (`docs/plans/02-DATA-ARCHITECTURE.md` §9).
Everything else of RM stays: requirements, catalogues, revisions, quality scoring, principles/ADRs, ArchiMate 3.2,
UML 2.5.1, models/views/validation, render/export, search, **and `project_stakeholder` (21 live rows)**. Reversing
a drop would be a new migration from the old RM database, which stays read-only; the conveyor never imports it and never gates on the old system stopping.

| D21 | Long-running work | **The conveyor implements the MCP tasks extension (SEP-2663).** A task is a **project execution run**; a pending approval is its **input request**; the answer is an approval decision. `projects.execute` starts it, `tasks/get` reports progress and asks, `tasks/update` answers, `tasks/cancel` cancels — so a project can wait days for a human at no worker cost. `mcp_server` keeps its own task runtime for its other backends |
| D22 | Executor model | **One executor slot, two kinds (amended 2026-11-09).** A `run` is driven by a **process** (ordered steps; an **agent step** runs the internal `agent.run` loop under its **process agent**, a **computed/tool step** invokes an action executed by a **tool agent**, a **gate** parks the run), or by an **external agent** over `/mcp` + MCP tasks (D19/D21). A step's output is inspected by its SOP's **judge** (laya/eval) which accepts or rejects; a reject loops the step (bounded) or blocks the card → the coach. `capability_agent` (S15) is the one persisted spec: its rows carry the role (`process_agent` / `tool_agent`); the conveyor hosts no model and no second runtime and never depends on `agent` as a component. Caps, scope, prompt version and pre-run readiness are enforced and recorded. The copilot (S10) is an interactive surface, not a work executor |
| D23 | Inference ledger and prompt audit | **The conveyor records every model call it makes through its own invocation path — W7's coverage — and audits the prompts it runs** (S16; amended 2026-11-12: a call that omits usage context, or whose capture fails, is logged and continues; the ledger never claims a row it did not write, and no total is inferred). `inference_call` holds one tenant-scoped row per invocation — model and served model, token counts, the two provider prompt-cache classes, latency, and the prompt's identity (`prompt_ref` + version + the rendered-prompt SHA-256), joined to `run`/`run_step`/`conversation_turn`, and carrying **no prompt or response bodies**: the router keeps the text, the conveyor keeps the key. **Cache-hit is defined rather than assumed** — `cache_read > 0` is a hit, `cache_read = 0 ∧ cache_creation > 0` is a **write** (a deliberate warm at ~1.25×, not a miss), neither is a miss, and a response without `usage` is `unknown`; the router has no response cache, so no other reading is available. The **prompt audit** follows the `claude-api` `prompt-audit` method — scope and target model, inventory, provenance, classification, the four anti-pattern groups, then a report of findings with confidence and a proposed diff — it **proposes and never applies**, and it runs as a procedure with one bounded `completions.create` call, not as a second runtime. **Three things trigger it:** a person or the kata (`prompt_audits.start`); a tenant-configured **after N executions** policy, enforced against the ledger itself **at capture time** — the Nth call recorded since the tenant's last audit starts the audit, and no separate counter exists that could drift from the calls it counts; and **publishing a procedure**, which checks the draft and refuses a `high`-confidence finding (`422 procedure_audit_findings`) unless the publisher acknowledges it, with the override recorded on the publish's audit event. Findings are the **kata-executor's** input — it reads `prompt_findings.list` and starts audits of its own — so the improvement loop and this audit are one loop, not two. Where the audit is less certain than publication requires: it **proposes**, and every transition of a finding is a person's |
| D24 | Benchmark harness | **`conveyor-bench` is absorbed, not integrated: the conveyor owns the measurement loop in Rust and there is no Python in the product** (S17/S18). The SWE-bench instrument's dataset is a **hashed seed artifact** (`seed/benchmark/<slug>.json`: the pinned 50-instance subset, its image lock, and the `eval_script` per instance) — not a runtime download, and the script is **generated at seed time by the conveyor's own Rust port of swebench 5.0.2's `make_test_spec`**, pinned to the harness byte for byte by a golden test (the harness runs once in development as the oracle; no Python is in the build or the image). The **grader is a parity port** of the harness's verdict ladder and its per-repo log parsers: `resolved` is never computed by the producer or by a person. The **sandbox is one k8s Job per case** (untrusted patch, no service-account token, resource caps, pod deadline), driven by a single durable job. The arms are `gold`/`empty` (the controls that make a number publishable), `baseline` (the one-shot model control), and `conveyor` (the product, pinned to the `capability_agent`/`procedure` version it measured, which is S18 after S13/S15). Comparison is the harness's exact two-sided McNemar, refusing arms whose oracle or prompt differ. **The kata continuous-improvement loop consumes it**: a version-to-version delta, a promoted baseline, and a significant-regression flag, with the improvement iteration invoking the same actions a person would. The old repo's committed logs and results are the port's test oracle; nothing in the delivery requires a parity comparison, a freeze or an archive. |
| D25 | Eval builder | **The `build-eval` and `eval-audit` methods become a first-class aggregate, not a directory of files** (S19). One **eval** measures **one** Claude-powered flow, and its subject is a **registered action** — `completions`, the copilot turn, a procedure, or a registered external action (`agent_run` declared and refused `409 subject_not_ready` until S13/S15) — invoked through the ordinary engine and never re-implemented. An eval is a **versioned instrument**: `eval_version` freezes its cases, metrics, graders, prices and harness paths under one digest, and a comparison across versions is refused, because scores taken before and after a grader change are not comparable. The method's **two literal sign-offs are `approval` rows** (`eval_inputs`, `eval_grading`) carrying the instrument digest, and `eval_runs.start` fails closed `409 eval_not_ready` naming every missing gate; an edit to the instrument **voids** its sign-off, and no sign-off action is agent-reachable — a sign-off a machine can give is not a sign-off. The runner is the durable `job` table: one job per `(case, rep)`, a hard per-case ceiling, jittered backoff with the attempt visible, strict-per-attempt scoring, and a **failure class on every failed attempt in `eval_attempt_error`, which never occupies the `(case, rep)` slot**. **The answer key never reaches the subject** — `expected` is write-only, and the subject-input type has no such field. **Cost is derived from `inference_call` at read time, never stored** (D23). The report is a screen; `eval_runs.export` emits the method's `results.jsonl`/`traces/` bundle for interop, never hand-rolled HTML, because case text is untrusted input |
| D26 | Eval hillclimb | **A climb is a sequence of `eval_run` variants over one version, and the held-out set is the only thing that makes its result credible** (S20). A climb records its **goal** (`target`, `direction`, `hold[]`), its **stopping condition** (plateau at K ≥ 3 rounds, a round count, or manual), its **delta set above the measured noise floor**, and an optional spend ceiling; a round is analyze → apply → run → record and then **returns to the caller**, so the check-in cadence is a person's by default. The **split is drawn at random, stratified by `tags[0]`, never by baseline score**, and is fixed once: a train slice selected for low scores guarantees regression to the mean and a flat held-out number. The analyzer sees **train transcripts only** and the headline is test — enforced as **access** (`evals:approve` gates a test-split trace and an `agent` principal does not hold it), not as an instruction. After two rounds that do not clear the noise band the next round is a **categorisation** round (`artifact_gap \| grader_disagreement \| harness \| structural \| variance`), and a further content round is refused. A `goal.target = cost_usd` climb is the same loop under a quality guardrail, following the `cost-hillclimb` lever order. **A grader fix never re-grades in place**: `eval_runs.regrade` carries a finished run's **stored** outputs onto the eval's **current version** — a new run, one `eval.regrade` job per stored slot matched by `eval_case.ordinal`, never a second pipeline call and never a second count of the subject's tokens. It is `evals:approve` (it creates a run and can spend a judge call), it refuses the source's own version (`409 version_unchanged`), and because comparison refuses two versions, a grader change means **regrade every arm, then compare new-vs-new** |
| D27 | Compiled prompts | **A prompt is compiled at run time from the run's sources, not stored as a document** (S21; owner ruling 2026-11-09). Prompts live in Postgres: `prompt` + `prompt_section` hold a versioned **recipe** — ordered sections over `system`, `procedure`, `capability`, `project_card` and `knowledge` — and `conveyor_domain::prompt::compile` renders it against the run's facts (the procedure/workflow and its step, the capability/agent, the project and card, the bound packs). A required source the surface lacks refuses `409 capability_not_ready` naming the pin and the section, **before** a model call. The compiled text is never stored: `inference_call.prompt_ref`/`prompt_version`/`prompt_sha256` name the recipe and the hash, and a source changing changes the next compile rather than a stored copy. All three surfaces — `agent.run`, the copilot and the prompt audit — resolve through the one catalogue. S21 extends this renderer to selection, budgeting and the `plan` projection; PDDL is evaluated as a validation representation, never adopted as the storage format |
| D28 | The laya decision model | **Every process step's output is judged before the step is done.** The SOP names the judge — **laya** (the estate's TypeSafe AI decision service), an S19 eval, or a deterministic checker — and the acceptance criteria. The judge returns `accept` or `reject` with a probability/margin and the criteria evidence. A **reject loops the step** (bounded by the SOP/process); an exhausted loop **blocks the card → the coach**. A laya outage **waits** — it never fails open and is never counted as a reject — **bounded**: after `JUDGE_MAX_PARKS` (64) the step fails CLOSED with `judge_unavailable` and its ordinary `on_result` applies (`fail` — the default — fails the run, so the card goes BLOCKED and the coach is notified; `continue` skips the step); an unjudged output is never accepted. The verdict is recorded on the run step (`judge`: decision, probability/margin, criteria, output hash), so acceptance is auditable and replayable. The conveyor hosts no judge; `laya.judge` is an external action |
| D29 | Tenant-defined capabilities and processes | **The system capabilities are fixed; everything else is the tenant's.** `project_management`, `kanban_coach` and `kata` are always present and cannot be deleted. Every other capability — with its process agents, tool agents, processes and process map — is authored per tenant and bound to project types. The 280 card-type × list processes are a **default catalogue** a tenant may instantiate, override or ignore, not a fixed system set. A process, its SOPs and its map are versioned; a run pins the versions it used. **Built 2026-11-10:** the default catalogue ships as 70 type files / 280 processes / 350 SOPs under `seed/processes/`, loaded by `crates/store/src/process_seed.rs`; a tenant may instantiate or override any of it. |
| D30 | The process map | **The DAG is the user-facing process map, not the executor.** A project type carries a versioned map: nodes are processes, capabilities, phases or milestones; edges are flow. It is declarative — it cannot reorder a step — but the PM plans from it and the UI renders it with the project's live position (running, done, blocked). A prescribed sequence of external API calls is a process whose steps are tool calls, not a second execution model. `w2v`/media-production orchestration is a **later capability**, not a slice now; the tenant-defined model is what will carry it |
| D31 | Connector standard | **Web, Telegram and MCP are three instances of one interaction-connector class** (S06d). A kind is a registry entry — typed config + schema, sealed-secret **names**, compiled default grants, and thin `authenticate`/`parse`/`send` functions; the core owns identity, the shared command grammar, multicast, first-response close and the delivery state. A binding stores config and secret names in Postgres; the tenant-admin configures it and its per-action grants through the web UI (`channels.kinds|bind|get|list|test|policy_*`). A pending approval is offered to every eligible channel (`interaction.dispatch`), the first decision resolves it, and the other recipients are expired with "superseded: resolved via <channel>". A machine channel can never be granted an eval sign-off, a held-out read, tenant/channel/member administration or workflow authoring. Adding a JSON gateway is a `webhook` binding and no code; a native API is one connector module. |
| D32 | One identity, many tenants | **A person holds a role per tenant; the request names the active tenant with `X-Active-Tenant`, resolved against the caller's own membership rows** (distinct from D27's compiled prompts — split 2026-11-12). An absent header with more than one membership is a `400`, a tenant with no row is a `403`, and the role is the selected row's. There is no platform-wide super-admin shortcut: the absent row is the non-reputation proof. |

## 4. Scope boundaries

**Absorbed now:** fr backend + UI; RM in full; Planka used-features; action registry (replacing the n8n nodes); app-level licences; the `conveyor` Keycloak realm; **the inference ledger and the prompt audit (D23)** — the conveyor's own tenant-scoped record of the model calls *it* makes, never a read of the router's or billing's databases; **the benchmark harness (D24)** — `conveyor-bench` rewritten in Rust as part of this app, with its dataset as a hashed seed artifact and no Python left in the product; **the eval builder and its hillclimb (D25/D26)** — the `claude-api` `build-eval`, `eval-audit`, `eval-hillclimb` and `cost-hillclimb` methods rewritten as an aggregate, its actions and its screens, with nothing absorbed from another service.

**Absorbed later (same spec):** the n8n process engine + a stage editor; the 348 flows re-authored as typed **processes** (stored as procedures) and computed handlers. The n8n **graph editor** is not carried; the workflow engine itself is **retained** as the tool-sequence executor (owner ruling 2026-11-09), and the DAG returns only as the process map (D30).

**Never absorbed:** `licenses`, `billing_service`, `inference-router`, `worker_management`, `artifacts`, `coding-*`.

**Explicitly not carried over** — this list is the point of the rewrite:

- Planka as a process/DB/Redis, and the board↔project lookup round-trip.
- `planka_card_id` as card identity; leases stored apart from card state.
- Duplicate comment / notification / membership stores.
- The 224-migration accreted schema (one baseline instead).
- The three-realm Keycloak split, the licence-delegation maze, and fr's super-admin `X-Active-Tenant` impersonation path: one platform-admin impersonation path, explicitly audited. (**D32** returns `X-Active-Tenant` — not as fr's super-admin override but as a per-tenant role selector resolved against the caller's own membership rows.)
- Capability modules whose only job is validating untyped inbound payloads.
- `setInterval` workers (durable job table instead).
- Redis as a state store — it returns only as the **bus**, the stage transport (§11 R6, §5.6; S13's `crates/bus`), never as the system of record.
- RM's `entity_version_history`, ~20 catalogue tables, `toc_trees/nodes` and the model/revision/publication/review/finding sprawl: one `revision` table, one `catalogue_entry(kind, code, label, meta)`, one `outline_node`, flattened models.

### 4.5 The agent interface — the conveyor's own MCP surface (D19)

`agent` reaches the conveyor over **`/mcp`** — the rmcp streamable-HTTP surface the app already serves from its
action registry (§5.5), with the same tools, schemas and authorization as HTTP. `nonagenticai_mcp_server` remains
the agent platform's tool gateway for every **other** backend; its `fr` toolkit retires with fr at cutover. There
is no HMAC channel, no text command grammar and no second agent-facing gateway.

The agent runs the whole lifecycle this way: it creates a project, starts one durable task, reads its progress, answers
the approvals the work raises through that task, receives the final output and closes the project.

| seam | contract |
|---|---|
| agent → conveyor | MCP `tools/call` over `/mcp`; `Authorization: Bearer <licence>`; the licence resolves the tenant and an `app_user(kind='agent')` with a membership scoped to the agent tool list |
| tool naming | every MCP tool carries `_meta.operation = <action name>`; the agent imports ops from that key, and without it the op is named `tool.<snake>` |
| tool set | `projects.create\|get\|close\|execute`, `runs.list`, `approvals.list\|decide\|request`, `cards.get`, `events.list` |
| progress | **MCP tasks (D21)**: `projects.execute` returns a task handle; `tasks/get` reports status and presents each pending approval as an `elicitation/create` input request; `tasks/update` answers it; `tasks/cancel` cancels. Polling `projects.get` + `approvals.list` remains the fallback for a client that cannot run tasks |
| escalation | `approvals.request` opens a gate (`run_id` nullable); a human answers in the web or Telegram channel, or through the task |
| attribution | `approval_decision.decided_by_kind='agent'` and `via` name the licence principal; a human's answer relayed through the agent is recorded as `human` with `via` naming the agent |

Three rules survive from D18 and are not reopened: **a question is an approval** (one primitive, no second
concept); **the answer vocabulary is two verbs plus text** (`approve` / `reject` with a note); and **the agent is a
first-class principal**, not a borrowed human.

Two consequences:

1. **S06b is the cutover gate.** S14 depends on one real `agent` task completing over `/mcp` against the deployed
   conveyor; there is no other proof of the seam.
2. **The tool surface is versioned as an external API (R11).** Adding, renaming or removing a tool the agent uses
   is a contract change, not a refactor.

## 5. Architecture

### 5.1 Stack and standards

Rust 1.99, edition 2024, axum 0.8, tokio, sqlx 0.9 (runtime queries only, embedded migrations), utoipa + utoipa-axum, `schemars`, tracing JSON, thiserror, serde, uuid v7, `metrics` + Prometheus exposition. `#![forbid(unsafe_code)]` in every workspace crate.

Key crates: `rmcp` (MCP server), `openapiv3` (ingest external service specs), `progenitor` (generate the TS client), `openidconnect` (Keycloak OIDC), `object_store` (file bytes), `kube` (the benchmark sandbox's Job/ConfigMap/log client, S17), `resvg`/`usvg` (PNG), `svg2pdf` (PDF), `quick-xml` (XMI). ⚠️ **PDF was `typst` in the first draft and is `svg2pdf`** (changed 2026-11-08, S08 Part B2): typst is a full typesetting engine that lays the page out a second time from a different input, so a PNG and a PDF of the same view could disagree and no test would catch it. `svg2pdf 0.13` consumes the *same* `usvg` tree `resvg` rasterises, so all three formats are encodings of one parse — and it removes ~130 crates from the build.

### 5.2 Repository layout

One repo, one binary, a Cargo workspace so boundaries are compiler-enforced:

```
crates/
  domain/        pure aggregates, state machines, invariants — no IO, no sqlx
  store/         queries + migrations + outbox (the ONLY crate that speaks SQL)
  api/           axum routers, utoipa, problem+json errors
  actions/       ActionSpec, #[action] derive, registry, invocation, MCP projection, external OpenAPI ingest
  auth/          OIDC/JWKS, TenantId extractor, licence extractor, RLS context
  render/        pure model → SVG / PNG / PDF / XMI / markdown
  bus/           the care-bus port: envelope, Redis Streams, consumer groups, dead letter (S13)
  workflow/      the RETAINED workflow engine: the tool-sequence executor a process `call` step falls back to
                 (owner ruling 2026-11-09; the graph editor is not carried)
apps/
  server/        composition root: serve | migrate | worker | admin
  web/           the Rust UI
migrations/      one baseline + forward migrations
```

Modules, not crates (no independent version, splitting only adds friction): `jobs`, `realtime`, `search`, `integrations`, `benchmark` (the pure grader, parsers and arm rules, S17), `eval` (the instrument, the case runner, the graders and the climb rules, S19/S20) and `sandbox` (the k8s Job adapter in `apps/server`).

**Reused, not reinvented.** The primary reference is **`agent`** (`nonagenticai/agent`, ns `agent`) — 1,736 tracked files, 772 Rust files / 412k raw lines (~243k in `src`), 2 crates, 147 migrations, 3,767 test attributes. It already runs the exact platform pieces S2 needs, in this estate, under this cluster. `agent` stays a separate service; its **patterns** are copied, never linked.

| Source | What to take |
|---|---|
| `agent/crates/agent-api/src/tenancy.rs` + `migrations/0001_tenancy.sql` | `TenantCtx`/`JobCtx`; `set_config('app.current_tenant', …, true)` — transaction-local, PgBouncer-safe; RLS `ENABLE` + `tenant_isolation` policies; runtime role `NOSUPERUSER NOBYPASSRLS`; `SET LOCAL ROLE agent_platform` for cross-tenant reads |
| `agent/crates/agent-api/src/auth.rs` | Per-issuer JWKS caches (600 s TTL), trusted-issuer pinning, `aud`/`azp` checks, fail before key lookup |
| `agent/crates/agent-api/src/licensing/gate.rs` | Fail-closed licence verdict: no verdict, stale, unavailable or unknown status all refuse |
| `agent/crates/agent-api/src/sse_hub.rs`, `src/conversations/hub.rs` | SSE-only realtime: 15 s keepalive, `Last-Event-ID` replay, slow-client closure |
| `agent/crates/agent-api/src/provision/outbox.rs` + `migrations/0035_provision_outbox.sql` | Transactional outbox: enqueue in-tx, fenced drainer (`FOR UPDATE SKIP LOCKED`, `lease_until`/`lease_token`), send outside the transaction, uuidv7 as Idempotency-Key |
| `agent/crates/agent-api/migrations/0034_audit.sql` + `src/audit/` | Append-only `audit_event` (SELECT/INSERT only), vocabulary whitelist, secret/email scrubbing, size cap, fail-loud |
| `agent/crates/agent-api/src/lease.rs` + `migrations/0190_b8_worker_lease.sql` | Lease claim/renew/release with TTL and a sweep — the model for card claims and job leases |
| `agent/crates/agent-api/src/runtime/catalog.rs`, `src/runtime/orchestrator.rs` | `OperationCatalog` / `PinnedCatalog`: the typed operation registry the action registry is modelled on |
| `agent/crates/agent-api/src/runtime/bus.rs` (ported from `care-bus-agents@96c608c`) | The Redis Streams bus: one envelope `{id, topic, from, correlation_id, procedure, tenant_id, job, payload}`, `work.*`/`tool.*` topics, consumer groups `process\|tool`, XACK/XAUTOCLAIM, dead letter after 5 deliveries, tenant scope. It is the stage transport (S13), never the system of record |
| `agent/crates/agent-api/src/mcp_client/client.rs`, `creds.rs`, `external.rs` | How a Rust service speaks MCP — the platform client (tasks) and the external-server client (plain calls); the client side of S06b's gate |
| `template-rust-service/src/tenant.rs`, `parity.toml` | `TenantId` extractor + `TenantHeader` OpenAPI shape; the parity-contract model for `parity.yaml` |
| `strata_rag` (`src/tenant.rs`, `src/store_providers.rs`, `src/metrics.rs`, `tests/`) | Only the narrow pieces `agent` does not cover: pgvector patterns, Prometheus exposition, testcontainers Postgres |
| `strata_ingest` / `strata_rag` (the knowledge layer) | The integration target for document ingestion, embeddings and retrieval — the conveyor does not absorb KAG |
| `fr/backend/src/capabilities/define.ts`, `derive-openapi.ts`, `derive-mcp.ts` | The action-registry shape fr already proved (one declaration → OpenAPI + MCP + handler) |
| `fr/backend/src/lib/licence-introspect.ts`, `require-fr-license.ts` | Licence-token shape rule (JWT-shaped tokens are never retried); central gate + single-flight cache + fail-closed |
| `requirements_management/src/main.py`, `src/routers/__init__.py` | Router composition and mount map for the RM domains being ported |

### 5.3 Data

One Postgres 18 database `conveyor`, exclusive to this service, through PgBouncer (transaction pooling). Two roles, both this service's own (the house rule is one DB and one DB user **per service**, not one role): `conveyor_migrate` owns the schema and runs migrations; `conveyor_runtime` is `NOSUPERUSER NOBYPASSRLS` and is the only role the app queries as. Migrations run **before rollout as the separate `migrate` subcommand** with `DATABASE_MIGRATE_URL` pointing directly at Postgres (sqlx's migrator holds a session advisory lock); `serve` never migrates implicitly. `DATABASE_URL` points at PgBouncer.

- `tenant_id` on every tenant-owned table. Enforcement is two-layer: the store API takes `TenantId` as a required argument (a missing scope is a compile error), and RLS is set per transaction with **`set_config('app.current_tenant', …, true)`** — transaction-local, never a session-level `SET`, which leaks through PgBouncer transaction pooling. This is `agent`'s exact mechanism (§5.2).
- Every state change writes an **`event` row in the same transaction** (transactional outbox) and `pg_notify`s it. SSE fan-out, webhooks and n8n callbacks read from the outbox — never from a second write path.
- **One identity per work item.** The card id is the card id everywhere; no external id is ever a primary key.
- `audit_event` and `action_invocation` (actor, action, input hash, outcome, trace id) are written in the mutation transaction. Audit is structural, not a later feature.

### 5.4 Auth and tenancy

- Keycloak realm `conveyor`; clients: public PKCE for the UI, confidential service account for the backend. Browser = auth-code + PKCE; APIs = bearer only.
- Humans: OIDC; authorization from the app's own `membership` table (role per tenant), not from realm claims. `super_admin` is the one platform role; impersonation is one explicit, audited path.
- One identity, many tenants (**D32**): a person may hold a role per tenant. The request names the active tenant with `X-Active-Tenant`, resolved against **the caller's own membership rows** — an absent header with more than one membership is a `400`, a tenant with no row is a `403`, and the role is the selected row's. There is no platform-wide super-admin shortcut: the absent row is the non-reputation proof.
- Machines: licence tokens (non-JWT bearer) resolved against the `licenses` service with a short-TTL local cache, **fail-closed** — keeping fr's shape rule: a JWT-shaped token that fails verification is never retried as a licence token.
- App-level licence: per-tenant `GET /licenses/active?tenant_id=&app_key=conveyor`, synced into an `app_licence` table by a job and enforced by one `RequireLicence` extractor. Core requests never call the licensing service synchronously.

### 5.5 Action registry (replaces the n8n nodes and fr's capability modules)

Every operation the outside world can invoke is one `ActionSpec`: `name`, version, `kind: internal | external`, input schema, output schema, scopes, idempotent flag, timeout (the `workflow` kind is retained with the workflow engine, owner ruling 2026-11-09) — declared by a `#[action]` derive and registered at compile time. From that single declaration the app generates:

- `GET /api/actions` (the manifest) and `POST /api/actions/{name}/invocations`,
- the OpenAPI document,
- the **MCP tool list** served by `rmcp` — the app is its own MCP server, no sidecar. This is the conveyor's MCP surface, and `agent` is a client of it (§4.5); `nonagenticai_mcp_server` remains the agent platform's gateway for its other backends,
- a generated TypeScript client for external consumers.

External coding services register their `openapi.json`; `openapiv3` ingestion turns their operations into `kind: external` actions invoked through a tenant token, with timeout and circuit breaking, and normalized results. Schema refreshes are diffed; a breaking change needs an explicit approval.

### 5.6 Realtime and jobs

- **SSE** (`GET /api/events?topics=board:{id},run:{id}`) with `Last-Event-ID` replay. fr's realtime need is server→client invalidation; a WebSocket is added only if presence is later proven necessary.
- Durable `job` table with `FOR UPDATE SKIP LOCKED` pollers; one scheduler task materialises due `schedule` rows under an advisory lock. Handlers are idempotent on `(kind, dedupe_key)`. This replaces fr's 14 `setInterval` workers with restart-safe, observable runs.
- **Redis returns as the bus, not the system of record (2026-11-09).** The care-bus port (from `agent-api/src/runtime/bus.rs`) carries stage work over Redis Streams with consumer groups, XACK/XAUTOCLAIM and a dead-letter stream; Postgres `run`/`run_step`/`event` remain the record, and the outbox re-delivers after a Redis loss. Caches stay in-process with explicit invalidation; the licence cache TTL is the only one that matters for correctness.

### 5.7 UI

Rust, chosen by a **spike** (§8 S1) that builds the board (500 cards, drag, live updates) and the chat/copilot stream twice. Decision rule up front: **Leptos CSR is the prior** — the app is authenticated with no SEO need, so SSR's win is weak and SSR hydration/double-fetch bugs are the known trap. SSR is chosen only if measured TTI exceeds 2 s or the auth flow forces cookie-based SSR. Dioxus only if the phase-2 flow editor forces it. The `progenitor`-generated client is used either way.

Screen order after the spike: shell/login → board → card cockpit (run + approvals) → run queue → requirements editor/revisions → trace/architecture → procedures → search → admin (actions, integrations, licences) → **inference usage → prompt audit → benchmarks → improvement → evals → eval report → eval audit → eval hillclimb** → process stages (the process screen) → process map (the user-facing DAG, 2026-11-09).

### 5.8 Channels

The product is operated through **three first-class channels**, and every one of them lands on the same domain model, the same action registry and the same authorization:

| Channel | Inbound | Outbound |
|---|---|---|
| **Web app** | the Rust UI (auth-code + PKCE) | the UI itself + SSE |
| **Telegram** | bot webhook (secret-token header), reply→decision conversion | notification fan-out to chats/topics, first-response-counts |
| **MCP** | the app's own MCP server (`rmcp`): tenant-scoped tools over the action registry | tool results + async events |

Channel behaviour is one module (`channel`), not per-channel copies: a message is a `delivery` row with a channel kind, a recipient set and a state machine; the reply path resolves to the same `approval`/`comment`/`card` mutation regardless of channel. This is the piece fr implements three times; it is written once here.

Email and Teams fan-out are **not** in the in-scope list — they stay only if the S0 usage inventory shows them used.

### 5.9 Deployment

New namespace `conveyor`, DB + user `conveyor`, PgBouncer pool entry, ImagePolicy, ingress `conveyor.nonagentic.ai`, and the gitops `rust-service.yml` CI archetype. The repo starts from `template-rust-service` via `scripts/new-rust-target.sh` (owner `nonagenticai` → archetype CI). Local: `docker compose` with `postgres:18` + pgvector and the binary in dev-auth mode — the proof environment, not a second deployment target.

## 6. Unified domain model

`Tenant 1—* Project 1—* Board 1—* List 1—* Card`; `Card *—1 CardType`, `*—* Label`, `1—* Comment / Task / Run`; `Requirement *—* Card` via `trace_link`; `Process 1—* ProcessStep`; `Run *—1 (Card | Project execution)` and references process/step; `Run 1—* Run` as a **tree** (`parent_run_id`, `step_id`, `depth ≤ 3`); `Approval *—1 Run`; `ArtifactRef` attaches to any aggregate.

**The list is the card state.** Lists carry `kind: intake | active | waiting | done | archive`; a card's lifecycle is `list_id` + `archived_at`, with no second `state` column. Execution state lives on `Run` (`queued → running → waiting_approval → running → succeeded | failed | cancelled`). The card-type × list rules validate moves in the domain layer. This kills fr's card-state/list disagreement class outright.

A claim on a card is one conditional `UPDATE` (assignee + lease expiry + `version`), not a second lease store; lost updates are impossible because every mutation carries `If-Match`/version.

Other state machines: `Approval pending → approved | rejected | expired | voided` (`voided` when `runs.cancel` withdraws a gate that will never be answered), at most one pending per `(run, gate)`; `Requirement draft → in_review → approved → superseded`; `Process (procedure) draft → published → retired`.

**An approval is the one human-escalation primitive** — approvals and questions are the same row (§4.5, D19). Each decision records the *kind* of decider, `decided_by_kind ∈ human | triage_agent | rule | agent`, plus a `via` blob naming the channel, so an agent's decision is auditable as an agent's and dual control can tell it from a person's. This is fr's mechanism carried forward, not a new one.

**Processes, steps and SOPs (redesigned, amended 2026-11-09).** A **process** is an ordered list of **steps**; a step is one of four kinds: an **agent task** (a process agent runs the bounded loop over its tools), a **computed task** (one action), a **gate** (an approval parks the run in `waiting_approval`), or a **child-process call** (a child run, depth ≤ 3). There are no executable predicates, loop nodes or general graph; a step's typed outcome is `continue | fail | retry`, bounded by `max_attempts`. **Each step has a procedure (SOP)** — a versioned definition of **ready** (inputs and preconditions), **done** (outputs and checks), **acceptance criteria**, and an **eval/inspection** of the output (the laya judge, an eval, or a deterministic checker; D28). A process declares its **user-facing outputs** — the deliverables a person sees when the process completes. One process per `(card_type, list)` — 70 × 4 = 280 — is the **default catalogue** (S13); a tenant may instantiate, override or design its own process for a project type (D29). **As built (2026-11-10):** the catalogue is 70 type files / 280 processes / 350 SOPs loaded from `seed/processes/` by `crates/store/src/process_seed.rs`; a type file's start process is applied alone (`apply_type`) and the whole catalogue together (`apply_all`), and the loader test applies the whole catalogue twice on a fresh database to prove publish validation and idempotency. `Run` is a **tree**: one live **root** run per card (one per project when card-less); a live parent may start child runs for a capability call or the coach, cancel cascades, and one live child exists per `(parent, step)` with one row per attempt.

**The process map is the DAG (D30).** A project type carries a versioned **process map** — nodes are processes, capabilities, phases or milestones, edges are flow — rendered for the user to show what is happening and which capabilities participate. It is **declarative**: it does not execute, and it cannot reorder a step; the PM plans from it, and the UI overlays the project's live position (running, done, blocked). A prescribed sequence of external API calls is a **process whose steps are tool calls**, not a second execution model.

**Capability, knowledge and memory (D20, amended 2026-11-09).** `Capability 1—* CapabilityAgent`;
`Capability *—* Process` and `*—* CapabilityKnowledge`. A capability is a **knowledge area** with
`kind ∈ system | tenant`: `project_management`, `kanban_coach` and `kata` are the built-in system capabilities
(always present, not deletable); every other capability is tenant-defined per project type. Every process **step**
has a **procedure (SOP)** — definition of ready, definition of done, acceptance criteria and an inspection — and a
process declares its **user-facing outputs**. Its
`CapabilityAgent` rows carry a **role**: a **process agent** (one per process / card type — the worker that owns
the process domain knowledge) or a **tool agent** (one per toolset — the executor that makes the tool calls). Each
row is the configured executor for its role (model reference, prompt, permitted actions, limits), and a `run`
records which capability and which process agent actually executed. Knowledge is **procedural** (the
`capability → process → step → procedure (SOP) → action|gate` ontology) and **domain** (`strata_rag` platform packs, one
`capability` pack, pinned per capability). Memory is **procedural** (the execution history already in
`run`/`approval`/`action_invocation`) and **work-package** (`work_package_memory` hot rows written with the run,
consolidated into `strata_rag` as one digest per card). The full model is `docs/KNOWLEDGE-MEMORY.md`; `strata` is
never the system of record and the conveyor never embeds or generates.

**The executor model (D22, amended 2026-11-09).** The **process is the top-level executor**; the **workflow engine** (`crates/workflow`, the `workflow` table) is **retained** as the tool-sequence executor (owner ruling 2026-11-09) — a `call` step resolves `child_process_key` process-first, workflow-second — and it is not a second top-level executor. A `run` is driven by a **process** (S11/S13): its **agent
steps** run the internal bounded loop (`agent.run`) under the process agent, its **computed steps** invoke one
action, its **tool calls** are executed by the tool agents, and a **gate** parks the run. An **external agent**
over `/mcp` (D19) running a project as an MCP task (D21) remains the third executor kind. The persisted spec for
the internal loop is `capability_agent` (role `process_agent`) — model, prompt, permitted actions, limits; the
toolset pin is `capability_agent` (role `tool_agent`). The run's wiring resolves the process-agent row into the
`agent.run` step input, which is what the loop reads; a step input may lower a cap, never raise it above the row,
and no other stored or hard-coded source may supply these fields. The conveyor hosts no model,
no embeddings and no second runtime, and does not depend on `agent`: an external executor is recorded by its
principal (`action_invocation`), the task id (`run.external_ref`), and `approval_decision.decided_by_kind`/`via`.
A **run does not start** when a required knowledge pack has no resolvable leaves, an `allowed_actions` entry is not
a registered action, or the prompt does not resolve — `409 capability_not_ready` with `blocking[]`, before the
first step. The copilot (S10) is an interactive, user-scoped surface over the same action registry; it is not a
work executor and needs no `capability_agent`.

**The inference ledger and the prompt audit (D23).** `InferenceCall *—1 Run` (or `*—1 RunStep`, `*—1 ConversationTurn`),
written by the engine whenever an external action that declares `captures_usage` returns a `usage` block;
`PromptAudit 1—* PromptAuditFinding`, each finding carrying the `prompt-audit` skill's own columns — location,
evidence, pattern (`1a`…`1f`, `2`, `3`, `4`), why obsolete, confidence, action, replacement. The audit reads the
three prompt surfaces the conveyor owns — procedure step templates, every action's `description`, and the
copilot's `SYSTEM_PROMPT` — plus `capability_agent.prompt_ref` once S15 lands, and it reports rather than edits:
applying a rewrite is an ordinary `procedures.update` on a draft, decided by a person through
`prompt_findings.decide`. Cache measurement is the evidence half of the same feature: the skill's Group 4 names
"no token accounting" as the prerequisite for every other finding, and "cache-hostile ordering" is only visible
as `cache_creation > 0` with `cache_read ≈ 0` on a stable prefix — which is why the cache report groups by `prompt_sha` and not only by model and caller.

**Triggers and the publish seam.** A `PromptAudit` is created three ways: manually (`prompt_audits.start`), by the tenant's *after N executions* cadence (`tenant.settings.prompt_audit`, enforced **at capture time** by counting the tenant's `inference_call` rows since its last audit — the Nth-call event starts it), or by `procedures.publish` calling the synchronous `prompt_audits.check` on the draft it is publishing — where a `high`-confidence finding is refused with `422 procedure_audit_findings`, and `acknowledge_findings: true` overrides it into the publish's `audit_event`. Findings are the kata-executor's input, not a dead report.

**The benchmark harness (D24).** `BenchmarkSuite 1—* BenchmarkInstance`, `BenchmarkSuite 1—* BenchmarkRun 1—*
BenchmarkCase`, and a run names the executor it measured (`capability_agent_id` or `process_id`, no FK — §2.17's
RLS reason). A suite is an **immutable instrument**: its `dataset_revision`, `seed_sha256`, `image_lock_sha256`,
`swebench_version` (the semantics the grader reproduces) and `parser_version` are its identity, and re-seeding a
slug with different hashes is refused. An instance carries the problem statement and the oracle files the
producer may see **separately** from the answers (`gold_patch`, `test_patch`, `FAIL_TO_PASS`, `PASS_TO_PASS`,
`eval_script`), which only the grader reads; no action returns them and no benchmark action is grantable to a machine channel.
A run is `arm × subject`: `gold`/`empty` are the controls, `baseline` is the one-shot model, and `conveyor`
measures a pinned capability-agent or process version (S18). The grader is the only authority on `resolved` —
the harness's verdict ladder, ported and proved against the banked logs — and comparison is the exact two-sided
McNemar, refused when suite, oracle or prompt differ (the pipeline is what is compared, so it may differ). The benchmark is the kata improvement loop's measuring
instrument: the loop proposes a candidate version, starts a run through the same action a person uses, compares
it with the promoted baseline, and keeps or reverts.

**The eval builder (D25).** `Eval 1—* EvalVersion 1—* (EvalCase | EvalMetric)`, and
`EvalRun *—1 EvalVersion 1—* (EvalCaseResult | EvalAttemptError | EvalTrace)`; `EvalAudit 1—* EvalAuditFinding`.
An **eval** is one flow: its subject kind and entry action, its per-case extras, and how it is traced. A
**version** is the frozen instrument — the case set, the metrics and one grader per metric
(`programmatic | pairwise | rubric | human`), the token prices and the harness paths — under a single digest;
`eval_run` names a version, and a comparison across versions, graders or splits is refused, because a score
taken before a grader change is not comparable with one after it. The method's **two literal sign-offs** —
*"do not proceed until the user has looked and said yes"* and *"would you have scored any of these
differently?"* — are `approval` rows at `eval_inputs` and `eval_grading`, carrying the instrument digest so an
edit **voids** the sign-off rather than leaving it quietly valid; `eval_runs.start` fails closed with
`409 eval_not_ready` and names every missing gate. Signing is its own scope (`evals:approve`) and is absent
from one machine-channel grant set (the S06d floor; the `AGENT_TOOLS` allowlist, renamed — not retired). The **runner** is the `job` table, one job per `(case, rep)`: a hard per-case wall-clock
ceiling, jittered backoff with the attempt count visible, strict-per-attempt scoring, and a **failure class on
every failed attempt** in `eval_attempt_error` — the `errors.jsonl` sidecar — which never occupies the
`(case, rep)` slot, so a resume re-runs exactly what produced no output. A result carries `model` **from the
response**, `usage`, and a `grade` object keyed by metric id; `status='truncated'` is counted and excluded from
the mean, never scored as wrong. **The answer key is structurally unreachable**: `expected` is write-only, the
subject-input type has no such field, and no answer-key action is agent-reachable. Cost is **derived** from
`inference_call` at read time, and the report — a screen, not a generated page — computes every aggregate in
Rust over the rows.

**The eval hillclimb (D26).** `EvalClimb *—1 EvalVersion`; a **round** is an `EvalRun` with `variant='v<N>'`
carrying `change_md`, `change_patch`, a pre-registered prediction and a `summary.suspicious` flag. A climb
records its goal (`target`, `direction`, `hold[]`), its stopping condition (plateau at K ≥ 3, a round count, or
manual) and a `delta` set **above the measured noise floor** — a change whose best case sits inside the floor is
kept or reverted by chance. The **split belongs to the version**: drawn at random, stratified by `tags[0]`,
never by baseline score, fixed once. **The analyzer sees train transcripts only and the headline is test**, and
that wall is *access*, not instruction — a test-split trace requires `evals:approve`, which an `agent`
principal does not hold, so "the analyzer never read test" is a property of the permission model rather than a
claim about its behaviour. `eval_climbs.round` runs one round and returns, because the method's check-in
cadence is a person's; after two rounds that do not clear the band the next round must categorise
(`artifact_gap | grader_disagreement | harness | structural | variance`) and a further content round is
refused. Comparison reuses S17's exact two-sided McNemar — one implementation in the estate, so a published
delta cannot come from a second, subtly different test — and **spend is never a column**: it is derived at read time from the same ledger model as the report — subject, judge and billed-failure calls, cache classes, explicit unknown/unpriced (W12).

**Platform tables:** `event`, `job`, `schedule`, `audit_event`, `action_invocation`, `idempotency_key`, `app_licence`.

## 7. API

- `/api/<resource>` — unversioned, per the house rule (never `/api/v1/<resource>`). No compatibility shims (D11); OpenAPI is the contract and clients are generated.
- `GET /api/actions` + `POST /api/actions/{name}/invocations` for registry operations; `/mcp` for MCP; `/health`, `/health/live`, `/health/ready`.
- RFC 9457 problem+json errors, cursor pagination, `If-Match` on mutations, `Idempotency-Key` on POST, one request id in every response.

## 8. Delivery plan

Each slice is proved locally (compose + real Postgres + integration tests) before it is called done. CI runs only when a slice integrates and when the repo merges.

| # | Slice | Gate |
|---|---|---|
| **S0** | **Usage inventory** — live usage from n8n DB, Planka webhooks/fields, fr/RM access logs, Keycloak clients → `parity.yaml` | The feature list is signed; zero-usage features are dropped, not ported |
| S1 | **UI spike**: Leptos SSR vs WASM SPA on board + chat | Decision recorded with the measured numbers |
| S2 | Skeleton: workspace, compose, tenancy, migrations, audit, auth, job runner, SSE | Two tenants isolated (incl. RLS); login works |
| S3 | Domain + schema: aggregates, state machines, ~60 tables | Proptest on transitions; schema review |
| S4 | Action registry: derive, OpenAPI, MCP, invocation/audit/idempotency, external ingest | 20 actions identical over HTTP and MCP |
| S5 | Board: projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks, SSE | Replay a live board export; two-client race; 1k-card load |
| S6 | Conveyor control plane: claims, runs, approvals, andon, comms/multicast, notifications | Golden card-type×list path; dual-control approval; retry |
| **S06b** | **Agent direct interface** — `_meta.operation`, machine authenticator, principal kind, `approvals.request` | one real `agent` task over `/mcp`; `decided_by_kind='agent'`; a cross-tenant licence refused with zero mutations |
| **S06c** | **MCP tasks** — the extension, `projects.execute`, a project execution run, approval-as-input-request | a task waits through a pending approval, is answered, and reaches a terminal result; cross-tenant id is `-32602` |
| S7 | Requirements + catalogues + revisions + quality scoring + search | Relevance eval set beats an agreed baseline |
| S8 | Governance + modelling: principles/ADRs, ArchiMate, UML, views/diagrams, validation, render/export, portfolio stakeholders (**TOC, registers, capabilities and deliverables dropped by the S00 verdicts**) | Golden SVG/PNG/PDF/XMI fixtures |
| S9 | Documents, repositories, secrets, workers UI, reports/metrics, billing display | Feature-list items green |
| S10 | Copilot / AG-UI + tenant MCP interface | Scripted agent session passes |
| S11 | **Processes, steps and SOPs** — ordered steps (agent, computed, gate, call), the per-step procedure (definition of ready/done, acceptance criteria, inspection), the process's user-facing outputs, actor/process-agent resolution | Author → publish → execute a process run with a gate, a child call and a judged step |
| S12 | UI build-out of every screen on the chosen stack | Playwright click-through per feature-list item |
| S13 | **Process engine + care-bus** — the step executor, the run tree, the laya/eval judge wiring, the user-facing **process map** (the DAG view), the Redis Streams bus, `projects.create` absorbing the start, `wip.pull`/`err.card` as computed handlers, and the 348 flows re-authored as processes/handlers; the workflow engine is **retained** as the tool-sequence executor | Execution-trace diff against the old flows; a child run under a live parent; a reject loops and a laya outage waits; a Redis loss re-delivers without a double outcome. **Built 2026-11-10** — migrations `0035_process_tree` and `0036_s19_output_grade`; the judge (`checker\|laya\|eval`) inside the step executor with a bounded park; the `call` step (process-first, workflow-second, child poll, `on_result`, cancel cascade, root-only `err.card`); `seed/processes/maps.yaml` (3 maps: `it`, `kata`, `py2rust`); `crates/bus`; the 70 type files / 280 processes / 350 SOPs catalogue. **Outstanding:** the tool-sequence `workflows/` files (no `coding_jobs`/`workspaces` action exists in this repo, so a sequence cannot name a registered action) and the n8n trace fixtures (no recordings in this repo) |
| S14 | Independent deployment: deploy, seed, coexistence with fr / RM / Planka (**rewrite: #44**) | the new product's own acceptance passes; the S06b agent run passes against the deployed conveyor; new-service rollback tested |
| S15 | **Capability, knowledge and memory** — capability aggregate, capability-agent, `strata` pack binding, work-package memory | a pinned pack returns a cited hit; memory write → consolidate → recall by `source_id` |
| S16 | **Inference ledger and prompt audit** (D23) — `inference_call` written inside the external call's bookkeeping transaction, the cache hit/write/miss report grouped by prompt, and the prompt audit over procedures, action descriptions, the copilot prompt and (later) `prompt_ref`, triggered manually **or** by a configured *after N executions* policy, and run against a draft **at publish** | a real `completions.create` call is captured with its cache class and join keys; an audit produces findings with the skill's columns and a proposed diff and a second run reproduces them; publishing a procedure with an unacknowledged high-confidence finding is refused; no finding can edit the surface it audits |
| S17 | **Native benchmark harness** (D24) — absorb `conveyor-bench`: the hashed seed artifact, the parity-ported grader (verdict ladder + 12 repo parsers), the k8s Job sandbox, the gold/empty/baseline arms, the single run driver, exact McNemar comparison and the promote/baseline control | the banked eval logs reproduce the banked verdicts row for row; gold 50/50 and empty 0/50; the banked comparable pair reproduces 8 discordant / p = 0.289; one live sandbox Job produces a verdict and is deleted; no Python in the build or the image |
| S18 | **Kata continuous improvement** (D24) — the `conveyor` arm pinned to `capability_agent`/`procedure` versions, the promoted baseline, the trend read model, and the improvement job that starts → waits → compares → keeps or reverts | a candidate version and its baseline compare with an exact McNemar delta; a significantly worse candidate is flagged; the improvement job performs the same calls a person would and records its verdict |
| S19 | **Eval builder** (D25) — the `build-eval` and `eval-audit` methods as an aggregate: the versioned instrument, the reviewed input set with a drawn split, the four grader kinds, the case runner with its four properties, the report, and the health audit with its oracle/null controls | the two sign-offs are load-bearing (a start before them is `409 eval_not_ready`, an edit voids them); a failed attempt leaves its `(case, rep)` slot empty and a re-run fills it; oracle ≈100 % and null ≈0 % through the real runner and grader; a report recomputes from raw rows; no answer-key or sign-off action is agent-reachable |
| S20 | **Eval hillclimb** (D26) — the `eval-hillclimb` and `cost-hillclimb` loop: goal and guardrails, stopping condition, the round protocol, stall triage, and the paired comparison | a two-round climb keeps one round and reverts the other with the reason recorded; a paired delta reproduces by hand and refuses across versions; a fourth content round against a wall is refused; a cost walk registers its adoption gates before round 1 |
| S21 | **Message construction** (D27) — the compiled prompt taken from a fixed five-section renderer to run-time selection with budgets: the procedural, domain/capability/role and project/card components, the `plan` projection over the step DSL (PDDL evaluated, never adopted as the storage format), and the audit hook that names the compiled hash | a required source missing refuses by name before a model call; identical recipe + facts compile byte-identically; an over-budget section refuses with its measured size, never truncates |

S8 is independent of the conveyor spine and can run in parallel once S2–S4 exist. ⚠️ **Corrected 2026-11-09:
this said S13 too.** S13 `depends_on` includes **S11** (`docs/plans/00-INDEX.md`), because its `agent.run` takes
the resolved `capability_agent` (D22/§17) and its flows are re-authored as processes — it is not parallel with
the spine. The UI spike runs first because it changes S12's entire cost. **S14 is blocked by three things beyond parity: S12 (the UI build-out), S13 (the native engine and its retained regression fixtures) and S06b (the `agent` → conveyor `/mcp` path is proven live).** ⚠️ Corrected 2026-11-09: this said "two things" and omitted S12, which S14's own `depends_on` names. S06c is additive: S06b's polling path remains the fallback, so a client that cannot run tasks still works and S14 does not depend on it. S15 sits after S11 and is not a cutover precondition unless capability knowledge is in the cutover proof. **S19 depends on S16** — the token accounting it derives cost from is the ledger, and `eval-audit.md` names token accounting as the prerequisite for every other finding — and on **S09** for the object store its export and attachments use, and **S20 depends on S19**; neither is in S14's dependency set, and both may land after cutover without touching the native acceptance.
⚠️ **Amended 2026-11-09:** the process model supersedes the DAG, but S17/S18's subject pins stay
`capability_agent` / **process** — the subject is the process (`process_id`); the old `procedure_id` pin migrates in `0035`, and a SOP is not a measured subject.

## 9. Verification

**"What we actually use" is measured, not assumed.** The S0 inventory draws on three independent sources: (1) ingress access logs — Istio access logging is already enabled (`clusters/strato/istio/telemetry-ingress-access-logs.yaml`, stdout → Alloy → Loki), so per-path counts can be derived without touching the frozen apps; (2) the n8n workflow DB — which operations the 280 card-type flows actually call; (3) the Planka feature table already measured. `parity.yaml` gets one ID per used feature, each with an executable acceptance test.

**Per slice:** local compose Postgres + the binary; integration tests against real Postgres; no mocked transport (behaviour tests only); proptests on state machines; **rendered artefacts as checked-in golden bytes plus a SHA-256 manifest** (`insta` for JSON schemas — a binary document in a `.snap` is unreadable in a diff and `insta accept` blesses changes nobody looked at); Playwright click-through for UI slices; k6 for board load.

**Before cutover:** replay recorded real payloads through the new action registry; run the real conveyor end-to-end against the new app while n8n still executes; then run it again on the in-app engine. Cutover is proven by a completed conveyor run, not by a green build.

**The inference record (D23) is proved against a real call, not a fixture.** S16's gate takes one `completions.create`
through the router and asserts the `inference_call` row against the router's own response — the same `usage` numbers
and a `cache_status` derived by the stated rule — then re-runs the prompt audit a second time and requires the
findings to reproduce. A cache probe that never sees `cache_creation > 0` (a prefix below the provider's minimum)
must report `miss`, not `hit`: the definitions in D23 are what make the report falsifiable.

**The eval (D25/D26) is proved by controls and by a refused start, not by a green run.** S19's gate starts a
run before either sign-off is decided and requires `409 eval_not_ready` naming the missing gate, then edits a
case and requires the *inputs* sign-off to read void — that second line is what makes "the sign-offs are the
product" falsifiable rather than decorative. Two controls go through the real runner and the real grader: an
**oracle** (the reference answers) must score ≈100 % and a **null** (an empty output, then a constant answer)
≈0 %; a judge is fed an empty string, "I don't know" and a confident answer to the wrong question and must fail
all three. An induced provider error must land in `eval_attempt_error` with a failure class and leave its
`(case, rep)` `eval_case_result` row absent, so a re-run executes it — never a silent zero. A test-split trace is
requested by an `agent` principal and must be refused. S20's gate requires a paired delta to reproduce by hand
from the per-case rows and a comparison across versions to be refused, because the one number the loop
publishes must be recomputable from the record and not from a second implementation of the arithmetic.

## 10. What this buys

| | Today | Target |
|---|---|---|
| Applications replaced | fr + RM + Planka + procedure_management | **1** (agent and mcp_server remain separate; the conveyor absorbs `mcp_server`'s `fr` toolkit) |
| Package fleet | 24 node repos | **0** (action registry) |
| Databases | 3 (+ Redis) | **1 Postgres + 1 Redis** (the bus; never the record) |
| Schemas | 86 + 57 + 33 domain tables (fr + RM + Planka), 306 migrations | 1 baseline, **88 tables on disk today** (65 with S15's capability/memory, plus S12's three RBAC tables, S16's three inference-ledger tables, S17's four benchmark tables, S19's ten eval tables, S20's climb and S21's two prompt tables); the process model takes it to **89** when `0033`/`0034` land (process/step/SOP + process_map, and S13's `workflow` drops at `0034`) — the original target was "~60" and S00 measured five of the 67 first planned as unused |
| APIs | 1,376 + 314 + Planka REST | one typed registry + OpenAPI |
| Card state owners | Planka (state) + fr (lease/approval) + RM (project) | **1** |
| Realtime stores | fr WS + Planka webhooks + Redis | one SSE stream fed by the outbox + the Redis bus (transport only) |
| Languages | TypeScript, Python, SQL, Svelte | **Rust** (+ SQL) |

## 11. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **RM modelling/render/XMI** — ArchiMate 3.2 + UML 2.5.1 validation, SVG/PDF rendering, XMI export. The largest unknown. Full XMI conformance is infeasible. | Ship a scoped profile validated against actual consumers early in S8; `resvg` + `svg2pdf` on one `usvg` tree (all three formats are encodings of one parse), and vendored DejaVu faces embedded with `include_bytes!` — never reimplement cairosvg font handling, and never read the host's fonts. |
| R2 | **Search without spaCy/scikit-learn.** | Build an eval set from live queries first; FTS + `pg_trgm` first, pgvector second; ranking is explicit Rust rules. Trap: porting sklearn scoring one-to-one. |
| R3 | **Copilot / AG-UI** (12.7k LOC, still evolving). | Port late (S10); keep the AG-UI wire contract if consumers depend on it. |
| R4 | **n8n replacement** — 348 flows, agent orchestration, LLM tool-calling loops. The n8n **execution** model does not fit: v4's macro/micro mantras are sequences of agents, and the one-live-run-per-card rule made child flows (the coach) impossible (measured 2026-11-09). | S13 is its own phase; the **process model** (capability → process → step → SOP → task/tool) replaces execution; the DAG returns only as the user-facing **process map** (D30); flows re-authored as processes/handlers, not converted. The **workflow engine is retained** (owner ruling 2026-11-09) as the tool-sequence executor a process `call` step can name — never as a second top-level executor |
| R5 | **Porting dead surface** — 1,376 fr endpoints. | S0 gates everything; zero-usage routes are dropped. |
| R6 | **Redis returns as the bus** (2026-11-09) — a second state store beside Postgres can drift or lose in-flight work. | The bus is a nudge, never the record: `run`/`run_step`/`event` hold the truth, the outbox re-delivers after a Redis loss, handlers are idempotent on the envelope id and the natural key, and the stream has no PVC. Caches stay in-process. |
| R7 | **Freeze drift** — the old conveyor still gets bug fixes the new app must absorb. | Keep a dated list of merged fixes on the frozen repos; review at each slice boundary. |
| R8 | **Tenant/auth rewrite** is the highest-bug-surface area. | Tenant typed into the store layer; one authorization module; negative tests per role; only `SET LOCAL` under PgBouncer. |
| R9 | **Big-bang cutover, no data migration** — every integration cuts over at once. | Freeze + S0 inventory + replay harness; run the conveyor against the new app before switching. |
| R10 | **Porting fr's accreted model** is the single biggest failure mode. | Baseline schema built from the domain model, not from the 224 migrations; schema-review gate before S3 closes. |
| R11 | **Breaking `agent` at cutover.** It reaches the conveyor over `/mcp`; changing a tool name, its input/output schema, the licence principal, the approval vocabulary or the task methods and their payloads silently strands the agent platform. | S06b's tool-contract test + one live agent task before S14; the tool surface and the task contract are versioned like an external API even though nothing else is. |
| R12 | **The grading node is `hp1`** (owner decision 2026-11-09: a node selector plus a quota). The old harness pinned `worker1` for its 1.1–1.24 GB eval pods; worker1 was wiped 2026-11-01. `hp1` is not hp2 (CI capacity) and not mf1 (MinIO + the PG standby). The sandbox stays fail-closed (`503 benchmark_sandbox_not_configured`) until the first live run. | The parser-parity check and all pure logic run without a cluster; the sandbox is fail-closed (`503 benchmark_sandbox_not_configured`). The node selector/taint is deployment config, recorded as `hp1`, never silently repointed. |
| R13 | **Grader fidelity.** The Rust port must reproduce swebench 5.0.2's verdicts for the 12 repos; a parser that differs silently changes a published score. | The banked eval logs are the executable oracle: 50/50 verdict equality plus the gold 50/50 / empty 0/50 controls are build gates; an unparseable case is `infra_error`, never a charged `unresolved`. |
| R14 | **The laya judge is external and its decision is load-bearing.** A laya outage or a badly chosen threshold can stall or mis-accept every step. | An outage **waits**, never fails open (D28); the verdict is recorded with its probability/margin and criteria, so a disputed acceptance is replayable; a tenant's threshold/criteria are versioned with the SOP; a judge that cannot decide is a typed block, never a silent accept. |
| R15 | **Tenant-authored processes and SOPs can be wrong** — a tenant can design a sequence that skips a required step or sets acceptance criteria it cannot verify. | System capabilities (PM, coach, kata) are fixed and cannot be reordered (D29); a process publishes only with valid steps and SOPs; the 280 default catalogue is the safe starting point; a run pins its process/SOP versions, so a later edit cannot rewrite history. |

## 12. Open decisions at review

1. **Port 8930** — confirm free locally and in the cluster before the slice that binds it (the house rule: never 8000).

*Decided 2026-11-08: `mcp_server`'s long-term role is **D19** in §3 — the agent calls the conveyor's own MCP surface, and `mcp_server` remains the gateway for every other backend. D18 is superseded. A session that wants a second agent-facing gateway is proposing a change to a taken decision.*
