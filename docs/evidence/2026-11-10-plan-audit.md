# Remaining development work — corrected assessment, 2026-11-10

Checkout: `lane/train-2026-11-09`, `0ace3ee`. This assessment replaces the earlier version of this file.

Three agents reread all 34 plan documents: the index, contract, data architecture, five source maps, all 25 slice plans and the development-process proposal. The primary agent read `docs/SPEC.md` and `docs/KNOWLEDGE-MEMORY.md`, checked findings against source, and consolidated the order below. No application code was changed.

This is a source/evidence assessment. No fresh application tests, live agent session, deployment inspection or benchmark was run. Historical receipts are not a passing release receipt at this checkout. Deployment items identify what must be verified; they do not assert that external infrastructure is absent.

## Scope correction

The owner's clarification is authoritative: **the old fr/kanban/n8n conveyor is separate. Both systems may run concurrently. Developing or launching the new conveyor does not require changing, retiring or proving equivalence to the old one.**

The earlier audit incorrectly promoted replacement assumptions from the spec/plans into launch dependencies. Remove these as automatic new-conveyor requirements:

- Freezing, dumping, shutting down or deleting old applications, databases, namespaces or workflows.
- Deactivating n8n flows, reaching an active-flow count of zero, or running `scripts/retire-n8n-flows.sh`.
- Reauthoring all 348 old flows, importing the old card ledger, or comparing old/new execution traces before launch.
- Switching the agent's existing old-conveyor registration or retiring its toolkit. A separate new-conveyor connection can coexist.
- Treating all 218 historically used features and legacy probes as an automatically binding launch checklist.
- Rolling back to the old conveyor. Release rollback concerns the new service's own image, configuration and database recovery.

Historical source maps/fixtures can inform product design and regression tests. Separation does **not** remove the new product's boards, procedures, native runtime, capabilities, channels or measurement features. Accept those against intended new behavior. An old behavior becomes backlog only when it is also a requirement of the new product.

## Assessment and execution order

The backend is substantially built. Remaining work is concentrated in **incorrect connections between implemented components, incomplete everyday UI, incomplete measurement execution/accounting, and missing integrated proof**.

Repair shared invocation/executor boundaries first. An action can currently be registered but unusable; a producer can exist but never execute; and a version can be recorded without being the configuration actually measured.

Distinguish independent operation of the core conveyor from completion of the entire declared product. Core operation requires correct authorization, real external calls, executable native processes, usable required core screens and tested deployment/recovery. Full product completion also includes benchmark producers, trustworthy eval/climb accounting, later authoring/improvement UI, recipe authoring and memory selection. Explicitly deferred work remains product work; it is not legacy cutover work.

An authorization defect in an enabled feature must be fixed before releasing that feature, even if its slice was outside historical S14 dependencies.

| Order | Work package | Concrete output / completion condition |
| --- | --- | --- |
| 0 | Correct release contract and native acceptance | Rewrite S14 for independent deployment; remove old shutdown/parity preconditions from active SPEC/index/S13 dependencies. List required new-product workflows and explicit later scope, each with setup, expected effects and executable checks. |
| 1 | Repair authorization and external invocation | Close generic eval-signoff bypass; identify Telegram senders and actual channel; make imported actions callable/restart-safe; supply upstream tenant credentials and a complete deadline. W1–W3. |
| 2 | Make native execution coherent | Enforce bound capability-agent configuration, fix workflow/procedure composition, and provide repeatable setup for selected new processes. Verify long-job recovery. W4–W6, V1. |
| 3 | Make model execution observable and reproducible | Complete ledger coverage, compiled prompt identity, actual-recipe audits and retention schedules. Reliable measurement/cost depends on this. W7–W8. |
| 4 | Complete core user workflows | Board/card authoring and paging, approval history, requirements, architecture/trace, procedures, search, copilot, declared administration and real OIDC test fixtures. W9–W10. Can proceed alongside orders 2–3 once contracts stabilize. |
| 5 | Finish measurement and improvement | Real benchmark/eval executor adapters, immutable subject pins, shared ledger-derived accounting/spend policy, and measurement UI wired to authoritative backends. W11–W14. Sandbox controls can be proved earlier. |
| 6 | Finish explicitly deferred authoring features | Tenant recipe CRUD/versioning/audit, memory selection, accepted workflow editor, remaining per-case eval auditor or an explicit scope amendment. W15–W16. |
| Release | One integrated proof at the release revision | Fresh-tenant setup, production OIDC, required real upstreams, native lifecycle/approval, real agent MCP integration, restart/retry/cancel, required browser flows and new-service rollback. V2–V4. No old-system gate. |

Define acceptance at the start; run focused checks as each task lands, then collect the integrated receipt. Core operation need not wait for explicitly deferred product features. Broken enabled actions still need implementation or explicit exclusion/disablement in that release's contract.

## Confirmed implementation and rework

### W1. Enforce eval-signoff policy on every approval path

**Confirmed defect; release blocker while eval gates are enabled.** `eval_signoffs.decide` requires `evals:approve` and a human (`crates/actions/src/eval/signoffs.rs:143`, `:159`). The same row can be decided through `approvals.decide`, requiring only `approvals:write`, with no eval policy check (`crates/actions/src/control/approvals.rs:47`, `:82`). The common writer checks state/quorum, not human-only authorization (`crates/store/src/approval.rs:691`). Generic approval decisions are agent-reachable.

Enforce the policy at a shared decision boundary, including human scope checks. Prove generic HTTP/MCP, task updates and Telegram cannot circumvent it; refusal must leave gate/decision rows unchanged. Preserve ordinary agent-answerable approvals.

### W2. Resolve Telegram senders and preserve transport attribution

**Confirmed defect.** The webhook checks chat/topic, then executes as `binding.config.user_id`, without resolving the message's `from` identity (`crates/api/src/routes/telegram.rs:165`, `:178`, `:219`). Different group responders can be attributed to one app member. Approval decisions separately infer channel from actor kind and deliberately label Telegram `web` (`crates/actions/src/control/approvals.rs:69`).

Map sender to an authorized app principal, or implement explicitly constrained delegation. Carry authenticated transport context through invocation. Test unbound/distinct responders and revoked permissions. Add decision-history readback (W9).

### W3. Complete the external-action execution contract

**Confirmed broken integration, independent of the old system.** OpenAPI ingest rejects compiled-name collisions, approves novel actions and persists them (`crates/actions/src/ingest.rs:395`, `:429`). Invocation sees only the compiled registry (`crates/actions/src/invoke.rs:526`). Next `serve` startup refuses approved names missing from that registry (`apps/server/src/main.rs:347`). A successful import therefore creates an uncallable action and can prevent the server restarting. The previous audit overstated this as worker-startup failure too; the shown assertion is in `serve`.

Build one tenant-aware resolution model for compiled/approved imported actions, consistently used by discovery, invocation, readiness, HTTP and MCP. Define refresh/revocation/restart behavior. Prove import → discover → invoke → restart → invoke, with breaking refresh held for approval.

Generic transport adds only optional correlation, without upstream credentials or tenant identity (`crates/actions/src/invoke.rs:842`). Timeout covers `send()` but not response-body reads (`:859`, `:902`, `:922`). Supply target/tenant credentials, correct path/query/body binding, supported upstream idempotency and a deadline spanning the response. Prove authenticated upstream calls and a stalled body. Verify actual coding/worker/billing contracts rather than treating declarations as execution proof.

Dedicated document-ingest and knowledge clients already resolve tokens. Preserve them. `retrieval.query` already uses `/api/query` in `crates/actions/src/integrations/external.rs`; the older `/query` plan discrepancy is documentation cleanup, not a current path defect.

### W4. Enforce bound capability-agent configuration

**Confirmed invariant breach.** The resolver accepts explicit `model` and `messages` and returns before consulting `capability_agent_id` (`crates/actions/src/workflow/agent.rs:262`). A bound run can substitute configuration despite D22's single persisted source and lower-only limits rule.

Resolve a bound run from its bound agent first; reject conflicting model/prompt/tools and higher limits. If explicit unbound execution remains supported, distinguish it and record truthful provenance. Test attempted replacements and valid lowered caps. Remove comments claiming S15/the prompt catalogue do not exist.

### W5. Repair native workflow → procedure composition

**Confirmed structural conflict, not legacy-coach parity.** A workflow procedure node requires the parent's card and starts a child procedure on that same card (`crates/actions/src/workflow/node.rs:683`, `:698`). The parent already occupies the single live-run slot (`migrations/0002_domain.sql:188`, `uq_run_live`).

Implement composition within a run or explicit parent/child semantics while preserving root work-item concurrency protection. Prove parent → procedure → result, cancellation, retry/restart and depth limits. Simply removing the unique constraint is not a sufficient design.

### W6. Provide repeatable setup for the new conveyor's processes

**Operational setup remains incomplete.** Catalogue/prompt seeding exists, but the general seed path does not install published procedures/workflows. The index explicitly records deferred procedure seeding (`docs/plans/00-INDEX.md:86`). Tests publishing fixtures are not production installation.

Define/install the processes intended for the new conveyor: published definitions, routing/matrix bindings, capability-agent configuration, required knowledge bindings/credentials and schedules. Make setup tenant-scoped, versioned and repeatable; prove a fresh tenant runs without test-only SQL. This does not require porting all old flows or installing every `fixtures/named` definition.

### W7. Complete inference coverage and compiled prompt identity

**Confirmed gaps.** Ordinary HTTP/MCP calls supply `usage: None` (`crates/api/src/actions.rs:194`, `crates/actions/src/mcp.rs:232`); capture requires that context (`crates/actions/src/invoke.rs:933`). Internal judge-negative audit probes also omit it (`apps/server/src/eval_audit.rs:545`). These model calls can escape D23's ledger.

Supply minimum context for every usage-capturing action, preserving run/step/turn/eval joins. Keep absent usage unknown; account for billed failures where observable. Implement durable capture recovery or explicitly amend the completeness guarantee: current behavior logs capture failure and continues (`crates/actions/src/invoke.rs:1091`). Do not retry a successful paid call solely to repair its accounting.

The compiler hashes compiled system text (`crates/domain/src/prompt.rs:248`), while invocation hashes the complete messages array (`crates/actions/src/invoke.rs:950`). Copilot discards compiled identity, records `copilot@1`, and converts readiness failure to generic 500 (`crates/api/src/copilot/turns.rs:388`, `:396`, `:611`). Carry recipe key/version/compiled hash intact; name any full-request hash separately. Preserve the readiness refusal before a model call. Prove identity across agent/copilot/audit.

The eval ledger join already exists in `migrations/0033_d23_eval_caller.sql`, and subject/judge callers supply it. Rework consumers; do not rebuild this schema because older comments claim it is absent.

### W8. Audit actual prompt sources and schedule retention

**Confirmed rework.** Copilot executes a tenant recipe but audit inventory scans `COPILOT_SYSTEM_PROMPT` (`apps/server/src/prompt_audit_run.rs:219`). Audit actual recipes/resolved source identities. The action-description audit surface is also declared but empty; S16 explicitly records that authored descriptions are absent (`docs/plans/slices/S16-inference-audit.md:237`). Implement that coverage or explicitly narrow its contract.

`inference_call.prune` and `prompt_audit.prune` handlers exist but are absent from `DEFAULT_SCHEDULES` (`crates/store/src/schedule.rs:35`). Install their schedule rows and prove scheduler dispatch. Retain existing audit/publish-gate machinery.

### W9. Complete core UI and repair existing screens

**Missing clients over existing backends.** Build/Design/Copilot remain planned shell entries (`apps/web/src/app.rs:246`, `:247`, `:267`). Requirements/revision, architecture/trace, procedure author/publish/execute, search and copilot-conversation workflows lack corresponding screens/routes.

Board UI reads only the first 50 projects and first 200 cards, consuming only `items` (`apps/web/src/screens/boards.rs:33`, `:99`). Its sole board write is move (`:4`). Complete paging and declared project/card/lane/label/custom-field authoring. Prove users can find/act on cards beyond page one.

Expose decision history/principal/channel through approval projection/cockpit; counts cannot render promised attribution (`crates/actions/src/control/views.rs:298`). Complete declared action/job/audit/licence/worker/billing administration workflows. The earlier assessment overreached by separately requiring standalone document/integration administration screens without tying them to a declared workflow.

### W10. Implement production OIDC browser-test mode

**Missing test capability, not proof that production login is broken.** Playwright comments promise OIDC, but shared API setup always sends `Bearer dev:alice` (`apps/web/e2e/helpers.ts:3`). `BASE_URL` changes origin; it does not implement production authentication (`apps/web/playwright.config.ts:3`).

Add a real OIDC session/fixture and corresponding authorized API setup. Run required workflows against the deployed app with dev auth disabled, including role/tenant refusals. Build this alongside screens rather than at the end.

### W11. Connect measurement subjects to the existing executor

**Confirmed broken/missing adapters.** Conveyor benchmark production invokes ordinary `agent.run` (`apps/server/src/benchmark_producer.rs:132`), whose handler always refuses because the loop is executor-driven (`crates/actions/src/workflow/agent.rs:219`). The producer records `no_attempt`. It also rereads mutable agent configuration per case (`benchmark_producer.rs:85`); recording `subject_version` at start does not freeze execution.

Create a real run/step through the shared executor, executing an immutable agent/procedure/configuration snapshot including prompt/actions/limits and relevant pins. Prove a case reaches patch grading and a current-agent edit cannot change an already-started benchmark. Do not relax the executor-only handler merely to make its caller succeed.

Three additional adapters explicitly refuse execution:

- Benchmark baseline: `409 arm_not_ready` (`crates/actions/src/benchmark/runs.rs:113`). Implement bounded one-shot production, source/edit contract and patch generation.
- Benchmark procedure subject: `409 arm_not_ready` (`:184`). Map a case into existing card-scoped execution and collect its patch/result.
- Eval agent subject: `409 subject_not_ready` (`crates/actions/src/eval/runs.rs:185`). Connect the existing loop through an executor-backed adapter.

Share dispatch/provenance/result collection. These are missing adapters, not missing agent runtimes or a missing benchmark grader. Reconcile the later RBAC ruling with the still-active agent tool allowlist before claiming the external agent can drive the improvement actions; preserve explicit human-only and held-out restrictions.

### W12. Unify report, climb and spend accounting over the ledger

**Confirmed inconsistency.** Backend report sums copied subject/judge/failure usage (`crates/actions/src/eval/report.rs:368`). Climb spend sums subject result usage (`crates/actions/src/eval/climbs.rs:486`), omitting other billed calls. Pricing handles input/output but not cache classes (`:354`); token fallback can be compared with a ceiling named cents. Run start explicitly records but does not enforce its ceiling (`crates/actions/src/eval/runs.rs:71`); climb admission checks already-accounted spend (`climbs.rs:1505`).

After W7, use one accounting read model for reports, decisions and spend checks: subject/judge calls, billed failed attempts, cache pricing and explicit unknown/unpriced amounts. Define hard execution ceiling versus round-admission threshold, then enforce/label consistently. Prove all consumers agree; regrade must not charge subject calls twice.

SPEC conflicts too: D25/§6 eval says ledger-derived cost; §6 hillclimb describes result-row sums. Resolve to one source of truth. Cost-climb narrative still prints train/test score columns (`crates/actions/src/eval/climbs.rs:2051`); provide the declared cost/quality narrative using the same model.

### W13. Repair and finish measurement UI

Eval list/detail, run start/cancel/regrade, signoff request/decide, report/transcript display and climb start/list/get/stop already exist. The previous blanket treatment as missing was too broad.

Concrete remaining work:

- Report fetches only 200 results and recomputes means/cost in-browser (`apps/web/src/screens/evals.rs:280`, `:745`). Use authoritative report/accounting APIs and paginate detail rows; a page must not become a whole-run score.
- Add instrument/case/metric authoring, audit/findings and export/interop controls, plus climb-round and cost-goal/guardrail workflows. Audit/interop tabs and hints still claim existing actions do not exist (`evals.rs:430`, `:437`, `:964`).
- Benchmark UI disables conveyor and describes subject arms as unbuilt (`apps/web/src/screens/benchmarks.rs:375`). Wire completed producers/subject selection, trend/candidate/comparison/promotion and improvement controls.

### W14. Prove the real measurement/improvement loop

**Outstanding evidence, not missing pure logic.** Existing receipts cover banked parser/verdict parity and stub sandbox lifecycle, not the actual configured Job, cleanup/resource policy, live gold/empty controls or repaired producer → grader → compare → keep/revert path (`docs/evidence/S17-CORE.md:169`, `docs/evidence/S18-B.md:57`).

Use the recorded grading-node decision (`hp1`, index); verify config rather than reopening it. Prove one live sandbox lifecycle, then controls and producer/improvement. Compare an authenticated real router response with its ledger row and retain repeatable audit evidence. Keep benchmark gold/empty and eval oracle/null/judge-negative controls distinct.

### W15. Finish deferred recipe/memory authoring

Tenant recipe CRUD/version activation/retirement with audit, and automatic memory selection → processing → `prompt.memory`, remain explicitly deferred (`docs/plans/slices/S21-message-construction.md:237`). Knowledge prompt sections currently render pack identities, not automatically selected retrieved content (`crates/domain/src/prompt.rs:414`). Design/implement selection without duplicating working `knowledge.query`.

Native memory accepts structured metadata on write but omits it from reads (`crates/store/src/memory.rs:47`, `crates/actions/src/memory/mod.rs:22`). Restore readback if native consumers require structured provenance. This is a native API concern, **not** an old-ledger migration/readback gate.

Prompt budgets, role sources, declared outputs and publish-time plan validation exist. Do not re-add them or reopen PDDL as storage.

### W16. Finish bounded later product features

- Accepted ADR 0002 chooses a Leptos/SVG workflow editor; it is not an implementation (`docs/decisions/0002-editor.md:7`, `:50`). Build load/layout/drag/save/reload over existing APIs. Runtime does not depend on it; no egui spike is needed.
- Tier-3 per-case eval LLM auditor is explicitly absent (`docs/evidence/S19-C.md:119`). Implement through bounded shared invocation/ledger, or record an explicit scope reduction. Mechanical audit/control/export machinery exists.

## Verification and unresolved questions

### V1. Establish long-job lease/recovery behavior

New concern, **not a demonstrated duplicate-effects bug**: default job lease is 60 seconds (`crates/api/src/config.rs:278`); worker awaits the handler without an evident heartbeat (`apps/server/src/worker.rs:268`); another worker can reclaim expired running work (`crates/store/src/jobs.rs:149`). Agent execution can take 300 seconds.

Run a legitimate handler beyond its lease with two workers, then kill/restart one worker. Check step/handler ownership, idempotency and completion without duplicate paid calls, overlapping effects or stranded runs. Depending on results, add renewal/fencing or adjust bounded execution/lease policy. Source inspection alone does not establish the final effect.

### V2. Build native behavioral acceptance

The historical inventory's 218 used entries reference legacy probes. S14 reachability rejects only connection failures/5xx, allowing unexpected 401/404/422 (`docs/plans/slices/S14-cutover.md:168`). It is unsuitable as the new release gate.

For each required new-product workflow, record native actions/UI, setup, expected status/body/effects, tenant/role refusals and executable checks. Historical IDs may provide provenance; there is no requirement to pass every old endpoint or add compatibility routes. Fail missing required checks, zero selected tests and unexpected statuses. Explicitly run search relevance, which is ignored by default (`crates/api/tests/search_eval.rs:355`).

### V3. Verify independent deployment and real integrations

Repo/CI bootstrap exists; `.github/workflows/ci.yml` requests deployment. Inspect actual GitOps/live state before declaring infrastructure unbuilt. Verify own DB roles/RLS/PgBouncer, migration job, worker/scheduler, static UI, OIDC, licence, secret sealing, required upstream credentials and network routes.

Prove fresh-tenant setup, native process → approval → resume → terminal result, cancellation and restart. Separately connect the real external agent to the new `/mcp`: import, real licence attribution, question/approval/completion and cross-tenant refusal without mutations. Task/polling implementations exist; the receipt still lacks a real agent run (`docs/evidence/S06b-agent-task.md:48`). Update proof rather than rebuilding the lifecycle.

The internal executor does not depend on external `agent`. Real integration remains declared acceptance when that channel is released. Test coexistence using distinct new identities, routes, registrations and data ownership; no old-service deletion/inactivity gate. Exercise new-service rollback/recovery only.

### V4. Produce one release-SHA receipt

Historical lane receipts at different revisions do not compose into one tested release. Record SHA/toolchain; run required workspace checks, served-spec lock check, production UI build/browser workflows; attach native acceptance and real-dependency results at that revision. Record executed test counts; repair stale filters/targets before using them as gates.

The development-process-speed document is an optimization proposal. Address measured iteration/CI bottlenecks; cache/cancellation-policy experiments are not product prerequisites. Local commit/push hook receipts are not Rust test receipts.

## Contradictions to reconcile

| Stale claim / contradiction | Required correction |
| --- | --- |
| SPEC D8/§8/§9 and S14 require replacement, old-engine rehearsals and decommission. | Apply independent-system/coexistence ruling. Rewrite objective, dependencies, commands and evidence; do not merely reorder shutdown steps. |
| S13 couples native runtime completion to reauthoring/retiring old flows. | Retain native execution/editor requirements and useful regression fixtures; remove retirement gates. |
| Signed old inventory/S15 ownership become a mandatory ledger migration. | Express retained native memory/provenance requirements directly; no automatic old-ledger import gate. |
| S14 HMAC/gateway settings and `mcp_server` approval wording contradict D19. | Direct new-app `/mcp` plus licence principal. Keep old connections untouched. |
| D27 means both compiled prompts and multi-tenant identity. | Assign distinct stable decision identifiers. |
| SPEC startup migrations contradict explicit code/compose `migrate`. | Document actual pre-rollout migration model; do not add implicit migration to `serve`. |
| Blanket version checks contradict guarded status/quorum transition exemptions. | Align SPEC/data architecture with CONTRACT's edit-versus-transition rule. |
| Old one-membership prose contradicts active-tenant selection. | Update plans; current multi-membership implementation exists. |
| D23 completeness contradicts optional/best-effort capture; prompt identities differ. | W7 coverage/recovery and compiled identity, or explicit contract amendment. |
| D25 ledger-derived cost contradicts hillclimb prose/code. | One ledger/accounting model (W12). |
| S20 evidence says retire special `AGENT_TOOLS`; invocation still uses it (`invoke.rs:532`). | Reconcile one authorization contract before expanding access, preserving human signoff/held-out restrictions. Necessary for claimed agent-driven improvement; not a wholesale auth rebuild. |
| R12 says no grading node chosen; index records hp1. | Carry recorded decision into current config/proof. |
| Frontmatter/counts/early remaining paragraphs/UI hints lag later build records. | Replace append-only status with current disposition/evidence. Counts are inventory, not backlog. Preserve applied migrations. |

## Coverage and completed work to preserve

“Present” means inspected implementation/existing evidence, not fresh full certification.

| Plans | Current disposition |
| --- | --- |
| S00; five source maps | Historical scope evidence exists. Native acceptance needs rework; no legacy launch census. |
| S01 | Leptos CSR decision made. Measure finished UI; no stack-choice redo. |
| S02–S03; contract/data architecture | Auth/tenancy/schema/jobs/SSE/audit exist. Integrated proof, V1 and contract reconciliation remain. |
| S04 | Compiled registry/invocation exist. W3 is substantive execution rework. |
| S05 | Board backend exists. Paging/authoring/browser behavior remain. |
| S06 | Control plane exists. W1–W2 and decision-history readback remain. |
| S06a | Superseded HMAC channel; remove active assumptions, do not implement it. |
| S06b–S06c | Direct MCP/lifecycle/durable tasks exist. Real-client proof remains. |
| S07–S08 | Requirements/search/governance/model/validation/render backends exist. Missing clients and relevant behavioral/relevance/consumer proof remain. |
| S09 | Documents including ingest, repositories, secrets/reports exist. Generic external execution and declared admin remain. |
| S10 | Copilot/AG-UI backend exists. Conversation UI, identity and real inference proof remain. |
| S11 | Procedure author/publish/steps/gates/retry/cancel exist. Native installation/UI/composition remain. |
| S12 | Real board/cockpit/runs/admin/measurement subsets exist. W9–W10 and W13 are targeted remainder. |
| S13 | Native engine/triggers/trace exist. W4–W5 and editor remain; old retirement excluded. |
| S14 | Replace with independent deployment/release plan. Repo/CI bootstrap exists. |
| S15 | Readiness, knowledge queries, hot/cold memory/consolidation exist. Native setup/selected extensions remain; no legacy import gate. |
| S16 | Ledger/audit/publish gate exist. W7–W8 and live proof remain. |
| S17 | Seed/specgen/parser/verdict/grader/comparison logic exists. Baseline and live sandbox/controls remain. |
| S18 | Trend/comparison/improvement bookkeeping exists. Executor-backed pinned producers and real loop/UI remain. |
| S19 | Instrument/runner/graders/signoffs/report/audit/export exist. Bypass, agent adapter, accounting, authoring/audit UI and Tier-3 auditor remain. |
| S20 | Split/climb/stall/regrade rules and some UI exist. Accounting/spend, round UI, cost narrative and authorization reconciliation remain. |
| S21 | Catalogue compiler/budgets/roles/plan-output validation exist. Recipe authoring, memory selection and executed identity remain. |
| development-process-speed | Separate optimization proposal; measure bottlenecks before adopting changes. |

Immediate implementation priorities are W1–W5. Largest visible product gap is W9. Measurement is incomplete until W7 and W11–W14 work together. None requires operating or retiring the old conveyor.
