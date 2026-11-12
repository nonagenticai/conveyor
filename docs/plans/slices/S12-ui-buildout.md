---
slice: S12
title: UI build-out
status: built
depends_on: [S01, S02, S04, S05, S06, S07, S08, S09, S10, S11, S17, S18]
spec_sections: ["§5.7", "§8 S12"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: PENDING-S0
---

⚠️ **Amended 2026-11-09 (process model).** Screen 7 (`procedures`) becomes the **process view**: the ordered **steps** with their `kind`, `actor` (process agent / tool agent / system / human), the step's **SOP** (definition of ready/done, acceptance criteria, inspection), the **judge verdict** and the process's **user-facing outputs**; gate and child-call badges. A new screen **22 `process map`** renders the per-project-type **DAG** (processes/capabilities/phases) with the project's live position — a Leptos/SVG map, read/author, not an execution graph. The run detail (`/runs/:id`) gains the **run tree** (parent/children, depth, step) and the dead-letter state. The execution engine has no graph editor.

# S12 — UI build-out

Programme project: S12 · issues #16, #41, #42, #43

## 1. Objective

Build every declared screen on S01's stack, each proved by a Playwright click-through against a real API and a real database. S12 designs nothing and invents nothing: no component library (S01's), no action, table, event or screen. The output is the screens plus their specs.

## 2. Starting state

- S01 has recorded the stack (Leptos CSR is the prior) and deleted `spikes/web/`.
- S02 and S05–S11 each carry a **"UI surface declared for S12"** line: the screen, the actions it calls, the read model and its SSE topic.
- `GET /api/actions` serves the whole registry; every action a screen calls already exists and is tested.

## 3. Scope

**IN** — the twenty-two screens of §5 on S01's stack, each with a Playwright spec that asserts empty state, error state and happy path. **OUT** — design, component-library choice, any new action/table/event/screen, the **execution-graph** editor (the engine has none; screen 22's process map is a declarative map), server-rendered pages (CONTRACT §11: no stack assumption beyond S01's decision).

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `slices/S01-ui-spike.md` | the chosen stack and its ADR | the decision | the spike code (deleted) |
| `slices/S02,S05–S11` §3 "UI surface declared for S12" | each screen's actions, read models, SSE topics | them verbatim — a screen may not call an action its declaring slice does not name | their API shapes re-invented |
| `02-source-map/20-fr.md` §"Frontend (S12)" (`fr` `6f7c5097`) | the 195 `+page.svelte` route groups | coverage check only | the pages themselves |
| `docs/SPEC.md` §5.7 | the screen order | the order | — |

## 5. Deliverables — the screen list

| # | screen | declared by | actions | read model / SSE | spec |
|---|---|---|---|---|---|
| 1 | shell/login | S02 | `me.get`, `tenants.get` | `/api/me`; no table | `shell-login.spec.ts` |
| 2 | board | S05 | `boards.get`, `cards.*`, `lists.move`, `labels.*`, `custom_fields.*` | `BoardView`; SSE `board:{id}`, `card:{id}` | `board.spec.ts` |
| 3 | card cockpit | S05, S06, S11 | `cards.get/update/claim/release`, `runs.start/cancel`, `approvals.decide/list`, `procedure_runs.start` | `CardView` + `ApprovalOut` (renders `decided_by_kind` on each approval decision — an `agent` badge distinct from a person's — plus `via.channel`, so an agent decision shows which channel carried it); SSE `run:{id}`, `approval:{id}` | `card-cockpit.spec.ts` |
| 4 | run queue | S06 | `runs.list` | `run` (queued partial index) | `run-queue.spec.ts` |
| 5 | requirements editor/revisions | S07 | `requirements.*`, `quality.score` | `governed_asset` + `revision` (`RevisionDiff`) | `requirements.spec.ts` |
| 6 | trace/architecture | S08 | `model_views.get`, `models.traceability` | `model_view` + rendered object; `model_validation_run`; SSE `model:{id}` | `trace-architecture.spec.ts` |
| 7 | processes | S11 | `processes.*`, `procedures.*` (SOP), `process_runs.start/cancel` | `process` + `process_step` + `procedure` (SOP) + `run_step`; SSE `run:{id}` | `processes.spec.ts` |
| 8 | search | S07 | `search.query` | ranked assets/cards/catalogue entries | `search.spec.ts` |
| 9 | admin | S09 | `actions.*`, `jobs.*`, `audit.list`, `licences.get`, `workers.list`, `billing.summary` | `/api/actions`, `audit_event`, `app_licence` | `admin.spec.ts` |
| 10 | copilot conversation | S10 | conversation routes; `approvals.decide` | `conversation_turn`; SSE `conversation:{id}` | `copilot.spec.ts` |
| 11 | inference usage | S16 | `inference_usage.report`, `inference_calls.list` | `inference_call` — hit / write / miss / unknown over time, ratio per model and per caller, and the call table with its run/turn join | `inference-usage.spec.ts` |
| 12 | prompt audit | S16 | `prompt_audits.start/list/get`, `prompt_findings.list/decide`, and the cadence via `tenants.update_settings` | `prompt_audit` + `prompt_audit_finding` — findings grouped by pattern (`1a`…`4`) with confidence, evidence and the proposed diff; accept/reject per finding; the *after N executions* setting; the findings a refused publish returned | `prompt-audit.spec.ts` |
| 13 | benchmarks | S17 | `benchmark_suites.list`, `benchmark_runs.start/get/list/cancel/compare/promote`, `benchmark_cases.list` | `benchmark_run` + `benchmark_case` + `benchmark_suite` — arm/subject, controls, resolved/n, per-case verdict/patch/log, the paired matrix and exact McNemar with its refusal banner; SSE `benchmark_run:{id}` | `benchmarks.spec.ts` |
| 14 | improvement | S18 | `benchmark_runs.trend`, plus `benchmark_runs.start/compare/promote` for the candidate control | `benchmark_run` grouped by `capability_agent_id`/`procedure_id` — rate per version against the promoted baseline, control badges, regression flags | `improvement.spec.ts` |
| 15 | evals | S19 | `evals.create/get/list/update/retire`, `eval_versions.*`, `eval_cases.*`, `eval_metrics.*`, `evals.draw_split` | `eval` + `eval_version` + `eval_case` + `eval_metric` — one flow per eval, the version digest, the case table with tags/expected-provenance/split, the metric list with its grader and the headline rule (first `binary`) | `evals.spec.ts` |
| 16 | eval run | S19 | `eval_runs.start/get/list/cancel`, `eval_results.list/get`, `eval_results.label` | `eval_run` + `eval_case_result` + `eval_attempt_error` — N×R progress, retries and the failure sidecar beside the results, truncation count, served-model mismatches, the cost derived from the ledger; SSE `eval_run:{id}`, `eval_result.recorded` | `eval-run.spec.ts` |
| 17 | eval report | S19 | `eval_runs.report/export`, `eval_results.list/get` | per-case table with **every declared metric column** and the perf columns, click-through transcripts with tool calls as their own cards, attachments, per-tag breakdown, mean + CI against the noise floor; the screen that replaces `report.html` | `eval-report.spec.ts` |
| 18 | eval audit | S19 | `eval_audits.start/get/list`, `eval_findings.list/decide` | `eval_audit` + `eval_audit_finding` — findings by §1–§6 and severity, the oracle/null control scores, the noise-floor-vs-headroom line, evidence links; accept/fix/dismiss per finding | `eval-audit.spec.ts` |
| 19 | eval sign-off | S19 | `eval_signoffs.request/decide` | the two literal pauses as full-screen reviews: **every** input with its text rendered as data (never HTML), the pilot's grades beside their transcripts, and an approve/reject with a note — the screen that makes the number trusted | `eval-signoff.spec.ts` |
| 20 | eval interop | S19 | `eval_runs.export`, `eval_cases.import` | the method's `results.jsonl`/`traces/` bundle in and out through S09 documents; import names the eval and variant it lands as | `eval-interop.spec.ts` |
| 21 | eval hillclimb | S20 | `eval_climbs.start/get/list/stop/round/compare`, `eval_runs.report/get` | `eval_climb` + the round's `eval_run` — goal and guardrails, stopping condition, the round table with train and test side by side, `$/run` and derived spend, the diff drawer, the stall-triage buckets, and the final four-part summary; SSE `eval_climb.changed`, `eval_climb.round_completed` | `eval-hillclimb.spec.ts` |
| 22 | process map | S13 | `process_maps.get/list`, `processes.list/get` | `process_map` + `process` — the per-project-type DAG of processes/capabilities/phases, the project's live position (running/done/blocked) derived from the runs; read/author on a Leptos/SVG map, never an execution graph | `process-map.spec.ts` |

Screens 11–21 are declared by **S16**, **S17**, **S18**, **S19** and **S20**, and screen 22 by **S13**; they are the only rows here that name a slice after S11.
They enter S12's build order only when their slice is built — S12 renders a report the slice produces and invents
neither the read model nor its columns. Row 13 is not a cutover proof: S14 does not depend on it, so it may be
deferred past the other rows without moving cutover, and the same is true of rows 15–21.

⚠️ **Two of these screens are the product, not a view of it.** Rows 15 and 19 are where the `build-eval` method's
two literal pauses live — the input review and the grading review. A sign-off is an `approval` row, so the same
ask is answerable from Telegram and `/mcp`; the screen is the web channel of one primitive, not a fourth
mechanism.

Workflow editor: **S13**, not here. Specs live at `apps/web/e2e/<name>.spec.ts` with
`apps/web/playwright.config.ts`.

**Coverage rule:** every `used` fr UI row of `parity/parity.yaml` maps to one screen above or is recorded
**API-only** with its reason. A coverage table with no orphan proves it — the 195 fr pages are an input, never a
target.

## 6. Build order

One screen at a time, table order; no parallelism.

0. Harness: `npx playwright install chromium`; `apps/web/playwright.config.ts` with `webServer` running the real binary against a seeded ephemeral database, started with `CONVEYOR_DEV_AUTH=1`, the dev cookie `conveyor_dev=alice` in `storageState` **only in dev mode** (CONTRACT §10) — it is never attached on the OIDC path. **When `BASE_URL` is exported the config starts no webServer and targets that base**; the login helper picks its mode from `DEV_AUTH_TOKEN`: set ⇒ the dev cookie (local, and S14's local rehearsal), unset ⇒ real OIDC with `E2E_USER`/`E2E_PASS` (a dedicated Keycloak test user seeded at S14 step 7) — which is how the same suite runs against the deployed app, where dev-auth is refused. One helper, two modes. `npx playwright test --list` → 0 tests, exit 0.
1. shell/login — slot, `me.get`/`tenants.get`, `/api/me`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/shell-login.spec.ts`.
2. board — `boards.get`/`cards.*`, `BoardView` + `board:{id}`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/board.spec.ts`.
3. card cockpit — card/run/approval actions, `CardView` + `ApprovalOut` with the `decided_by_kind` agent badge on each decision; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/card-cockpit.spec.ts`.
4. run queue — `runs.list`, queued projection; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/run-queue.spec.ts`.
5. requirements — `requirements.*`, `revision` + `RevisionDiff`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/requirements.spec.ts`.
6. trace/architecture — `model_views.get` + rendered object; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/trace-architecture.spec.ts`.
7. processes — `processes.*`, `procedures.*`, `process_step`, `run_step`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/processes.spec.ts`.
8. search — `search.query`, ranked page; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/search.spec.ts`.
9. admin — admin actions, `/api/actions` + `app_licence`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/admin.spec.ts`.
10. copilot — conversation routes, `conversation:{id}`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/copilot.spec.ts`.
11. inference usage — `inference_usage.report`, `inference_calls.list`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/inference-usage.spec.ts`.
12. prompt audit — `prompt_audits.*`, `prompt_findings.*`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/prompt-audit.spec.ts`.
13. benchmarks — `benchmark_runs.*`, `benchmark_cases.list`, `benchmark_run:{id}`; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/benchmarks.spec.ts`.
14. improvement — `benchmark_runs.trend` + the candidate controls; `npx playwright test -c apps/web/playwright.config.ts apps/web/e2e/improvement.spec.ts`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cd $HOME/Movies/conveyor
docker compose up -d db && cargo run -p conveyor-server -- migrate --dev-seed
CONVEYOR_DEV_AUTH=1 cargo run -p conveyor-server -- serve &
trap 'kill %1 2>/dev/null || true' EXIT
for i in $(seq 1 30); do curl -sf -H 'authorization: Bearer dev:alice' localhost:8930/health/ready >/dev/null && break; sleep 1; done   # readiness, not a fixed sleep
test "$(yq '[.[]|select(.status=="used" and .app=="fr" and .area!="workflows")]|length' parity/parity.yaml)" -gt 0   # YAML, not JSON — jq cannot read it
curl -sf -H 'authorization: Bearer dev:alice' localhost:8930/api/actions | jq -e '.actions|length > 0'   # the app is up and authenticated
cd apps/web && npx playwright install chromium
BASE_URL=http://127.0.0.1:8930 DEV_AUTH_TOKEN=dev:alice npx playwright test --reporter=list   # every spec, against the app started above (config skips its webServer)
```

- Every spec asserts empty state, error state and happy path — a spec that only asserts the happy path fails review.
- Every action is resolved against `GET /api/actions` before the first click; a missing action fails at build time.
- The coverage table has no orphan; the count it explains equals the `yq` line above.

## 8. Traps

1. **Rebuilding S05–S11's components inside S12** (board column, approval card, run badge) — one component, owned by the screen that first needs it.
2. **A screen calling the database.** Writes go through `POST /api/actions/{name}/invocations`; reads use the declared projections.
3. **Playwright passing on a mocked API.** Seed real rows, run the real binary; runtime request mocking is a green light that means nothing.
4. **Copying fr's 195-page UI.** It is a coverage check, not a template — the rewrite drops far more than it keeps.
5. **A screen no slice declared.** If a row has no declaring slice, it is not built; it is an open question.

## 9. Decisions closed here

1. **S12 invents no screen.** Every row cites a declaring slice; the table is the whole scope.
2. **S12 owns no action, table or event** — it consumes the registry as served.
3. **The workflow editor is S13's**, not a twelfth row.
4. **The copilot screen ships because S10 declared it.** SPEC §5.7's screen order predates the copilot surface, so this row is filed as a `CONTRACT-CHANGE REQUEST` (add one screen to §5.7's order, UI-only); it is **recorded, not self-approved** — the main session adjudicates it in `00-INDEX.md`'s decision register before S12 closes.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Does the run queue need its own route, or is it a filter on the board? | its own route; S06 declared a projection for it |
| 2 | Are keyboard-only flows in scope? | no — one accessibility pass after the last screen, not per screen |
| 3 | Who owns shared components (card tile, approval card)? | the first screen that needs one; later screens import it |
| 4 | Do screens ship behind a flag? | no — one app, one tenant population, cutover switches the host |
