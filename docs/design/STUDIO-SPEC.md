# Conveyor studio — SPEC_V2 operating model

Open [studio.html](studio.html) directly in a browser. All assets and fonts are adjacent and local;
no build, server or account is required. The pack is also published at
**https://conveyor.nonagentic.ai/design/** from the conveyor app image (`/design` redirects to
`/design/`); the browser-only path above remains the offline entry point.

This revision aligns the studio design pack with **`docs/SPEC_V2.md`** (2026-11-11, draft for review).
V1 (`docs/SPEC.md`) remains the build record and stays binding for the slices it defines. SPEC_V2
adds the organisation that works the conveyor and the work hierarchy that organisation needs; it
states that no V2 code has been written. The studio therefore distinguishes three things on every
screen:

- **V1 surfaces** — implemented in the Rust app (projects, cards, runs, approvals, procedures,
  capabilities, knowledge/memory, evals, benchmarks, hillclimb, prompt audit, the ledger, secrets,
  channels, licences and billing). These carry no V2 badge and link to their existing pages.
- **V2 destinations** — proposed aggregates and actions from SPEC_V2. Every new destination carries
  a `V2 · PROPOSED` ribbon naming the tables/actions it previews. There is no write path, no
  backend call and no claim that the aggregate exists.
- **Legacy study surfaces** — the S12 surfaces V2 supersedes or reframes (the historical 92-entry
  job catalogue, the coding action catalogue, the TOC reasoning study). They remain reachable and
  are banner-labelled, never presented as the live model.

## Review paths

1. **Portfolio → Plan.** Open Portfolio: in-flight projects and programmes (a computed read model,
   no portfolio row), programme shared configuration, the typed project-dependency graph, the
   WSJF backlog, the PI cadence and per-project metering. Switch programme: the set re-scopes.
   Add a dependency that closes a cycle: it is refused. Open Plan: WBS, a frozen baseline against
   a computed forecast, the one six-kind register and controlled change. Approve a change without
   a rationale: `422 change_rationale_required`; with a rationale, a new baseline is drafted and
   existing run pins are unchanged.
2. **Organisation → Jobs → binding → run.** Open Jobs: the conveyor-owned catalogue
   (`job_key`, placement, anatomy), immutable releases and tenant-scoped bindings. Publish a draft
   release: a gate opens. Run against a stale release: `409 job_binding_stale` before any model
   call. Retire a binding: history keeps its pinned release. Open the Anatomy tab for the full
   release field set. The `Legacy import` tab explains why the 92 historical job seeds are import
   material, not the live model.
3. **Decision ladder.** Organisation → Approval policies: dual control refuses a self-approval.
   Decision ladder: L0 rule → L1 laya → L2 generation → L3 human; a generation result has no path
   to a state transition. Refinements & laya config: a tenant refinement overrides a platform
   threshold or fail action, creates a new version and is itself a gated policy change; every
   decision row pins the policy, refinement and laya config versions it used.
4. **Procedures → PKO → Authoring.** Procedures: capability → procedure → step; SOP intake
   (single or batch), RDF/OWL/Turtle and BPMN/Visio/draw.io conversion, the PDDL action space, the
   relational PKO, knowledge-pack pairing and publish. An unreviewed SOP-derived procedure refuses
   publication; a PDDL step with an unknown PKO term refuses; a publish with an unpublished pack
   refuses `409 procedure_not_ready`. Authoring: a chat brief produces drafts only; a draft whose
   referenced asset is unpublished refuses `422 asset_dependency_unpublished`.
5. **Run inspector and the tool estate.** Execution room: the run pins its job binding and release
   hash, uses one of two executors (`procedure` or `internal_agent`), and the internal agent's
   prompt is compiled from four sources — knowledge, episodic memory, the PDDL request and the
   curated tool list. The Tools panel shows the tool version, credential alias and returned
   evidence; the call opens a `tool_call` row. Tools: toolkit versions, bindings and grants, effect
   ceilings, the call ledger, probes, BYO import, tool authoring, the MinIO adapter and credential
   profiles. A binding that tries to widen its grant gets `403 tool_effect_forbidden`; a failed
   probe unbinds new runs (`409 tool_not_ready`) while in-flight runs finish.
6. **Improve.** Coach: one interruption becomes exactly one coach card over the closed triage and
   remedy sets; a repeat signature escalates to a human. Kata queue: a kata without an instrument
   and success threshold is not opened; the six Gets steer the work; the kata is its own tenant.
   Lessons: promotion needs ≥ 2 independent evidence references and a person's review; one
   reference refuses. Adoption: the kata proposes, a person adopts; a training-only or regressed
   gain is reverted.
7. **Operations.** Incidents: an incident card with service, severity, detection/mitigation/
   resolution and an evidence link. Runbooks: read verbs first; a destructive write needs a dry-run,
   the tenant's configured approvals (default human dual control) and a read-back. Executing
   without approval is refused. Postmortems are governed assets; a systemic finding becomes a kata
   candidate. The andon cord stops new runs, keeps the record and is released only by a person.
8. **Measure and the retained knowledge path.** The S17–S21 instruments remain: evaluation studio,
   benchmark harness, hillclimb lab and inference/prompt audit. Metering adds the tenant,
   department, programme, project, job, run, step and call levels with one derivation and billing
   reconciliation. The knowledge design, sources & retrieval and memory explorer path is retained;
   procedural knowledge now pins the PKO release and organisational memory adds reviewed lessons.
9. **Knowledge and memory workbench.** Knowledge design: switch to Effective configuration, resolve
   a `department_conflict` at the job layer, compare Current/Draft and inspect the dependent jobs.
   Sources & retrieval: inspect the original beside an extracted passage, see the seven distinct
   states and quality signals, run a retrieval test with expected evidence and save a missed case
   for review. Memory: inspect a claim, open Current/History/Derivation, correct by adding a
   successor, exclude from recall and rebuild the digest from a dry-run diff. Procedures: open a PKO
   term, follow its usage to procedure steps and refuse an invalid relation. Execution: open Explain
   context and see what was selected, excluded or truncated, with the reason and hash — never the
   compiled prompt text. Evals: compare memory disabled/current/candidate and replay with a time
   cutoff. Review & release: the queue shows the reviewer, evidence and gate; nothing publishes
   itself.

## Operating model — the organisation spine

**Industry → ordered department(s) → job**, with an optional region sub-layer. A job sits in
exactly one industry and one or more departments of that industry, primary first; a department from
another industry is `422 department_industry_mismatch`. Knowledge compiles **base → industry →
department archetype → each department in declared order (with its region) → job**. A sibling
department that sets one key to a different value is a `department_conflict` compile error,
resolved only by a job-layer row.

The studio's first tenant is `nonagentic`, the first industry `software`, and the first department
set is a real IT department — delivery, verification, operations, project office and improvement.
AHTAPol (industry `health-technology-assessment`) and w2v (industry `media`) are separate tenants
from the same model (D60).

A job is **configuration, not a loop**. Its release carries:

| Field | Meaning | Author |
|---|---|---|
| `job_key` | stable position key, e.g. `software.delivery.backend_engineer` | conveyor (`job_catalogue`) |
| placement | industry, ordered departments, optional region | conveyor (`job_catalogue`) |
| charter | purpose, outcomes, non-goals, escalation path | conveyor (release instruction) |
| capability binding | one or more capabilities | conveyor (`job_binding`) |
| `procedure_ref` | published procedure version(s), each pinning its PKO release | conveyor (`procedure`) |
| executor | `procedure \| internal_agent` (D46) | conveyor (`job_binding`) |
| `model_ref`, `params` | inference-router model and parameters | conveyor (release) |
| `prompt_recipe` | compiled-prompt recipe (`prompt_ref@version`) | conveyor (release) |
| knowledge pins | layer kind pins + `capability` pack versions | conveyor (release) |
| memory policy | recall scope, hot/cold, write rules | conveyor (release) |
| tool grants | operation allowlist and per-call tool gate | conveyor (release) |
| tool binding | pinned toolkit versions, effect ceiling, credential alias | conveyor (`tool_binding`) |
| decision options | laya option sets and policies | conveyor (`decision_*`) |
| limits | max iterations, timeout, token and cost budget | conveyor (release) |
| gates | required approvals, dual control, evidence requirements | conveyor (binding + tenant policy) |
| authority | what the job may do without a gate | conveyor binding |
| metrics | computed from execution, never stored as opinion | conveyor (read model) |

A **release is immutable** and pinned by a content hash. A **binding** is tenant-scoped and pins
one immutable release (`release_hash` + frozen `release_snapshot` JSONB), the capability, the
procedure or capability agent, and the executor kind. A release change is a new binding and a gate,
never a silent update. A run against a retired binding, or a release that is not the binding's pin,
is refused `409 job_binding_stale` before any model call. The check is internal — no catalogue call
and no caller-declared hash.

**Two executor kinds only (D46):**

- `procedure` — a deterministic sequence of steps; each step is an action (PDDL), a tool call or a
  gate.
- `internal_agent` — a step that needs generation over a compiled prompt. The prompt is assembled
  at run time from knowledge, episodic memory, the PDDL request and the curated tool list.

There is no external executor. The agent platform is a user-interaction front end over MCP, not a
runtime for conveyor jobs (D33). MCP and Telegram are channels, not executors.

**Scale is a catalogue property.** A thousand jobs is a thousand catalogue rows and bindings, not
a thousand processes (D39). The catalogue is searchable by code/capability/department and paginated;
a tenant activates the subset it needs.

## Work model — portfolio, programme, project, plan

**Tenant → portfolio (computed) / programme (shared config + dependency graph) → project → WBS /
deliverable → work package (card) → run → step / attempt.**

| Level | Meaning | State |
|---|---|---|
| Portfolio | every project and programme in flight for the tenant — a computed view, never a row | none (read model) |
| Programme | a set of projects with shared configuration and project-level dependencies; carries the budget and cadence | `active \| paused \| closed` |
| Project | a temporary endeavour with a charter and a baseline | `draft \| active \| paused \| closed \| archived` |
| WBS node | phase, deliverable, work package or milestone | open → accepted / cancelled |
| Work package | the card: one assignment, one owner, one list | list + `archived_at` |
| Run | one execution attempt | `queued → running → waiting_approval → running → succeeded \| failed \| cancelled` |

- **Portfolio membership is derived** from the tenant's configured in-flight statuses (default
  active and paused); closed and archived work is history.
- **Programme shared config** (knowledge pins and packs, default job families, approval-policy
  defaults, budget envelope, cadence) is inherited by its projects. A project overrides a key only
  when the programme does not lock it; the lock set is tenant policy.
- **Project dependencies are edges**: `project_dependency(from, to, kind ∈ blocks | needs |
  supersedes | informs, status)`, inside or across programmes. A cycle is refused at creation.
- **Project types select templates, gates and job families**: delivery (PMP phases), architecture
  (TOGAF ADM A–H), improvement (kata six Gets), development (self-development path), operations
  (detect → triage → mitigate → resolve → postmortem) and setup/authoring.
- **Plan ≠ state ≠ execution.** A card can move without answering its approval; a completed run
  does not close a project. A baseline is a frozen approved snapshot; a forecast is computed from
  current rows. Plan ≠ evidence: a WBS node is accepted only when its criteria are met by an
  artefact or run.
- **One generic register** replaces five PM tables: `project_register_item(kind ∈ risk | issue |
  assumption | dependency | benefit | change)`. A change request carries a scope/schedule/cost/risk
  impact block and records the person's rationale; existing runs keep their pins.
- **Cadence** is a `programme_cadence` row: PI planning materialises draft cards from the backlog
  by WSJF within WIP limits; a person approves the plan. The system demo requires every completed
  deliverable to exhibit evidence; inspect & adapt produces lessons and kata candidates.
  **WSJF is arithmetic**: `(business_value + time_criticality + risk_reduction) / job_size`, four
  typed integer inputs; the score is computed, never stored.
- **Gates are approvals.** A gate names its required decisions, approver kinds, evidence refs and
  expiry. Dual control is `required_decisions ≥ 2` with distinct approvers; the executor, assignee
  or requester cannot approve the gate that judges their work. A gate with no evidence is refused
  unless an override is recorded and counted. A gate that never refuses anything in a full
  programme cycle is flagged as broken, not healthy.

## Roles — meta-jobs as ordinary jobs

PM, kanban coach, kata facilitator, development projects and operations are `job_key`s using the
same card, run, approval and event primitives. The authority matrix is data
(`tenant_role_scope` plus the binding's `authority`), not branches in code.

| Role | Charter | Never |
|---|---|---|
| **PM** | request → governed project; charter, WBS, baseline, staffing, gate routing, closure | approve its own gates; change an approved baseline without a change request; do delivery work |
| **Kanban coach** | protect flow; one interruption = one coach card; closed triage/remedy sets | do the work it unblocks; open a coach card about a coach card; retry the same signature beyond the policy cap |
| **Kata** | improve a named part of the system; prove it with an instrument; hand adoption to a person | edit the instrument that judges it; approve its own adoption; measure on the training split only |
| **Development** | change the conveyor or a dependency repo through lane/PR/CI/review/merge/deploy/verify | modify its own gates, approval policy or the instrument that measures it; force-push; merge red CI |
| **Operations** | detect, triage, mitigate, resolve and learn from incidents | use cluster-admin; act without a card; perform a destructive action without the configured approvals |

Handoffs are cards, not messages. A question is an approval. Memory is written at handoff so a job
does not depend on a conversation transcript. Escalation is bounded: card → parent WBS owner → PM
→ programme; the coach can escalate to a human at any point. The **kata is its own tenant** (D45),
so its autonomy policy never inherits the target tenant's.

## Decision ladder — rule → laya → generation → human

| Rung | Mechanism | Used for | Record |
|---|---|---|---|
| **L0 Rule** | deterministic Rust over typed facts | state transitions, WIP limits, WSJF arithmetic, thresholds, policy checks, acceptance | the row + `action_invocation` |
| **L1 System-1 (laya)** | calibrated choice/score/noul over a finite option set | triage, routing, job/capability selection, remedy, severity, priority band, gate pre-check | `decision` row with the answer, probabilities where the type provides them, and confidence |
| **L2 Generation (LLM)** | text/code/plan/report synthesis | drafts, answers, patches, narratives, candidate procedures/prompts | `inference_call` + a draft artefact |
| **L3 Human** | approval | every gate, override and irreversible action | `approval` + `approval_decision` |

**The hard rule:** L2 has no path to a state transition. A generated output is a draft artefact;
applying it is an action, and the action's gate is L1/L3. The Rust type system enforces this; a
prose recommendation from an LLM is displayed but never consumed by the engine.

- **Options are rows** (`decision_option`), authored and versioned. **Questions are rows**
  (`decision_policy`) with `fire_when`, `min_confidence`, `fail_policy ∈ closed | open` and
  `action ∈ handover | block | annotate`. Safety gates fail closed; routing/quality gates fail open
  to a human or a note.
- **Configuration is per tenant, per case (D51).** A platform `decision_policy` defines the case; a
  tenant's `decision_policy_refinement` overrides the threshold or rule, the fail action and,
  optionally, the laya inference config. Refinements are versioned and gated.
- **Laya inference is tenant-scoped.** `laya_inference_config` pins model, params and timeout per
  tenant. Every decision records the policy, refinement and laya config versions it used, plus
  latency and input hash. Below threshold is an outcome — escalate or refuse per policy, never an
  error and never a silent pass. `Unavailable` is `gate_unavailable` and follows the fail policy.
- **Generation** is an `inference_call` bound to a draft artefact with prompt identity, model,
  usage, derived cost and source trust order. Forbidden: tool selection, routing, state
  transitions, gate decisions, scoring a submission, approving.
- **Budgets** are declared per run and job; exceeding one is `429 budget_exceeded` before the next
  call, never a silent truncation. Cost is derived at read time from the ledger.

## Procedures, SOP and the PKO

A **procedure** is executable documentation: an ordered sequence of steps, each an action (PDDL), a
tool call or a gate. Procedure design and management is first-class (D47).

- **SOP intake** — a tenant supplies PDF, URL or another document, singly or as a batch. A customer
  document is untrusted input: it enters as `governed_asset(kind='sop_source')` with provenance,
  never as instructions to the system.
- **Conversion** — the intake job harvests and curates the domain knowledge the SOP implies, then
  converts it into procedural knowledge (**PKO**) and a procedure draft whose steps are PDDL.
- **Two spaces, never mixed (D53).** The action space is the PDDL step — actions with preconditions
  and effects over typed facts. The procedural knowledge space is the PKO — tasks, steps, roles,
  artefacts and constraints. A step references PKO terms; a PKO entity never contains an executable
  step.
- **PKO storage** — relational, conveyor-owned and tenant-editable: an immutable `pko_release` pins
  `pko_entity` and `pko_relation` rows, layered base → industry → department → job. RDF/OWL/Turtle
  documents are ingest material converted into draft rows; the conveyor runs no RDF store and no
  reasoner.
- **Process documents** — BPMN 2.0 XML, Visio and draw.io exports ingest on the same path:
  activities → steps, gateways → gates, plus PKO rows.
- **Knowledge pairing** — the harvested domain knowledge is a pinned knowledge pack in `strata_rag`,
  paired with the procedural knowledge; the procedure release names both. Readiness blocks a run
  before the first step.
- **Gates** — every SOP-derived artefact is a draft behind a review gate (default human). A batch
  never publishes itself, and no SOP can grant a tool or widen an effect ceiling.

**Tenant asset authoring (D55).** The same intake pipeline drafts every tenant asset from chat
input and documents: organisation, jobs, capabilities, procedures, PKO, knowledge packs, project
templates, policies and tool requests. Every output is a draft behind a review gate; the asset order
is dependency-ordered (industry → department → job/capability → procedure/PKO → knowledge pack →
policies/templates → tools); a draft whose referenced asset is unpublished is refused; a chat
message or document can never grant a tool, widen an effect ceiling or change an approval policy.
The authoring job family is ordinary jobs on the two executors; the pipeline is a procedure, not a
hard-coded wizard.

## Tool estate

A job is only as capable as what it may call. Every cluster service that offers a typed operation is
a **tool**; tools are published in immutable **toolkit** versions; a job's access is a **binding**
with an effect ceiling; every call is a **ledgered row**.

| Term | Meaning | Not to be confused with |
|---|---|---|
| **Tool** | one callable operation `<service>.<operation>`: closed input schema, output schema, effect class, idempotency, timeout, redaction, destination alias | a conveyor action |
| **Toolkit** | an immutable, versioned bundle of one service's tools + auth adapter + licence app + destination | a repo, a package |
| **Tool binding** | tenant-scoped grant of pinned toolkit versions to a job binding or run, narrowed by effect ceiling, rate and credential alias | the release's tool grant |
| **Tool grant** | the release's operation allowlist and per-call tool gate | the binding (existence + credential) |
| **Tool call** | one audited invocation: run/step, tool version, input hash, status, latency, bytes, cost | `action_invocation` |

The measured estate includes the coding fleet, `strata_rag`, `strata_ingest`, `webscraper`,
`media_discovery`, `scientific_processor`, `lean4_prover`, `evidence_review`, the w2v orchestrator
and stage fleet, `video-compiler`, the ops read/write verbs, `billing_service`/`licenses`, the
AHTAPol `discovery` draft and the planned AHTAPol services. Planned tools enter as `draft` and
cannot be called until their slice deploys them and a probe passes. n8n, the `fr`/RM backends,
Planka, Keycloak, the agent platform, `care-bus`, `inference-router`/laya/modality-gateway,
`provisioning`, `artifact_service`/`storage-api` and the `mcp` deployments are deliberately not
job tools.

**The broker is the one call path** (`tools.call`): resolve the binding and pinned version → check
the effect ceiling and spend → validate the closed input schema → resolve the sealed credential by
alias → call the registered destination alias (no caller-supplied URL) → normalise and redact into
`{status, data, checks[], next[]}` → record one `tool_call` row and one `tool.called` event. A
missing binding or version is `409 tool_not_ready`; an effect breach is `403 tool_effect_forbidden`;
an invalid input is `422 tool_input_invalid`; an exhausted budget is `429 tool_budget_exceeded`.
Writes return read-back `checks[]` and take an idempotency key; a write with no verification path
cannot be published.

**Effect classes** are read / write / destructive / spend. A binding narrows only; a job may lower
its effect class at run time, never raise it. Destructive calls need a dry-run and the tenant's
configured approvals (default human dual control) in every environment. Spend calls need a budget.

**BYO tools (D42)** are OpenAPI 3.1 toolkits and remote MCP servers. Never uploaded code,
arbitrary container images, caller-supplied URLs at call time, or a tenant credential in a prompt.
The conveyor never executes tenant code; a tenant that needs executable logic runs its own service
and registers it. Promotion to the platform catalogue is a development project with a platform
review, never an admin toggle.

**Tool authoring** has three governed paths: spec → toolkit, service → toolkit, and a missing
operation → tool-request card. Draft → validation (schema, effect, egress, credential scope) →
security review → `tools.publish` (immutable version) → binding. The tool that publishes a tool is
itself a tool and cannot widen its own effect ceiling.

**Probes and retirement:** every published tool has a read-only `tools.probe`; a probe failure
unbinds new runs while in-flight runs finish. A tool with zero calls over one programme cycle is
flagged and retired. A schema-drift signature opens a kata.

**Storage (D57):** the conveyor writes and reads its own bytes directly to the shared platform
MinIO with a mandatory tenant prefix, server-side encryption, a content hash per object, a size cap
and sealed credential profiles. `artifact_service` and `storage-api` are not conveyor dependencies;
a job never sees an S3 key.

**Relationship to `nonagenticai_mcp_server` (D44, D59):** concepts borrowed, dependency refused.
The conveyor implements its own registry, broker, credential injection and call ledger. The agent
platform keeps its own gateway; there is no planned convergence and the conveyor broker is not
exposed to the agent.

## Executable rigour — TOGAF, SAFe, PMP

The rigour is rows and gates, not a document library.

- **TOGAF ADM as an architecture project type:** phases A–H as WBS nodes, each with required
  artefacts and a contract gate; requirements management is continuous, linked by `trace_link`.
- **SAFe as the programme cadence:** portfolio (computed), programme/ART with shared config and a
  dependency graph, PI planning, backlog refinement, system demo (no demo, no credit), inspect &
  adapt, lean governance at value boundaries.
- **PMP artefacts as typed rows:** charter (`governed_asset`), WBS (`wbs_node`), baseline
  (`project_baseline`), the one six-kind register, change control with impact analysis, closure.
- **The trace chain:** requirement/principle/ADR → architecture contract → charter → WBS
  deliverable → work package → run → step → artefact/evidence → acceptance → closure. The
  governance screen answers one question end to end: which requirement does this run prove, and
  where is the evidence?
- **What is not claimed:** no TOGAF certification, no SAFe compliance badge, no PMP certification,
  no full ADM conformance, no portfolio optimiser, no schedule solver. The claim is that the
  artefacts and gates the organisation uses are rows, the process runs, and the evidence is
  traceable.

The earlier **TOC study** remains in the studio as an optional constraint-reasoning view. It is not
part of SPEC_V2's method: V2's method is SAFe/PMP/TOGAF plus the coach's closed triage and remedy
sets. The TOC bars, float and buffer figures are illustrative; no scheduling solver runs.

## Knowledge and memory workbench

Knowledge management is not a document library and memory is not a transcript. The workbench lets a
tenant **inspect → explain → test → revise → review → release**, and answers four questions for every
job, work package and run:

- What does this job know, and where did that knowledge come from?
- What does it remember, and which parts are observations, claims or verified conclusions?
- Why did a particular run receive — or exclude — this information?
- Would a proposed change improve results, and what could it regress?

| | **Knowledge** | **Memory** |
|---|---|---|
| **Procedural** | `capability → procedure → step(PDDL) → action\|gate` + the pinned PKO release (conveyor) | `run`/`run_step`/`approval`/`action_invocation`/`event` (conveyor) |
| **Domain** | layered kinds + one versioned `capability` pack in `strata_rag`, pinned per job | work-package memory: hot rows in conveyor, one cold digest per card in `strata_rag` |
| **Organisational** | curated lessons promoted to a source draft (D36) | lesson memory: proposed → reviewed → promoted/rejected |

Rules carried from V1 and the KH hierarchy:

- A required kind with zero resolvable leaves **blocks the run** before the first step; an optional
  source degrades with a recorded warning, never silently.
- A pack is pinned `key@version`; publishing a new version does not change a live run's pin.
- Work-package memory is hot rows + one derived digest; hot wins on conflict; the digest is
  rebuildable, the rows are not.
- **Lesson promotion** needs ≥ 2 independent evidence references (different cards/runs/incidents),
  a person's review and the normal source/pack release path. One bad run never becomes doctrine.
- The conveyor never hosts embeddings or a model; retrieval is `strata_rag`; generation is the
  executor.

### Effective configuration — what applies and what can change

The designer keeps its capability → process → agent → resolved-call view and gains a first-class
**effective configuration** view for a job release. It shows the V2 layering rather than hiding it:

- **base → industry → department archetype → each department in declared order (with its region
  sub-layer) → job.** The table is `setting · originating layer · inherited value · override ·
  effective value`; every row names its source pin (`capability@17`, `delivery-handbook@4`, …).
- A required kind can only be tightened downward. A sibling department that sets one key to a
  different value is a **`department_conflict` compile error**, resolved only by a job-layer row;
  the resolution dialog compares both definitions and their sources and requires a value and a
  rationale.
- A required kind with zero resolvable leaves names the kind and department and refuses
  `409 release_not_ready`; the UI explains the resolution action instead of only showing "blocked".
- **Current release / Draft / Compare** are distinct views. Editing shows the layer, the scope of the
  change and the affected consumers (dependent jobs, procedures and bindings) before saving. Saving
  creates a draft release behind a review gate; a published release is never mutated.
- The job-release scope is deliberately separate from the retained capability-scope fixtures: a
  capability contract and a job release are different configuration objects, and the UI says which
  one is in view.

### Sources and retrieval — the transformation into usable evidence

A source record connects the whole chain **original → extracted content → passages/entities →
reviewed knowledge → published pack → consuming jobs**, and keeps seven achievements distinct:

1. imported (the source job succeeded);
2. extracted (parsing preserved the content);
3. reviewed (a person checked the extracted passages);
4. published (a pack release names them);
5. eligible for this job (the layer/kind/capability pins select them);
6. retrieved for this query;
7. included in this call (the context manifest).

An imported source is not necessarily extracted correctly; a published source is not necessarily
eligible for this job. The inspector shows the original beside the extracted passage and its locator
(page/section), enables or disables a passage, and records its review state. Corrections are proposed
as **versioned drafts**, never in-place edits. Quality signals are explicit: parsing errors, missing
tables, duplicates, conflicting statements, freshness, provenance/licence and the jobs that consume
the source. Each source also shows a named verifier, the next review date and the approval
requirement.

**Test retrieval sits beside the content.** The tenant supplies a question and the evidence it
expects, then inspects the returned passages with locator, score and layer. When the expected
evidence is missed, "Save as evaluation case" records a **candidate** with its split and expected
evidence; a person reviews it, and a model's assessment never becomes the acceptance criterion. The
UI keeps **finding evidence** separate from **generating a correct answer**: retrieval can find the
right passage and the answer can still be wrong.

### Memory — records, interpretations and reusable lessons

Every memory detail exposes the record taxonomy — **observation, claim, verdict, decision, blocker,
summary, lesson candidate** — and six questions:

| Question | Required information |
|---|---|
| What is this? | type and status (claim ≠ verified conclusion) |
| Where did it originate? | card, run, step, actor and supporting evidence |
| When was it applicable? | recorded time, validity period and supersession |
| Who can use it? | **human inspection authority and agent recall eligibility, separately** |
| What changed? | corrections, superseding records and derived summaries |
| Did it help? | recall usage, evaluation evidence and linked findings |

**Current / History / Derivation** are distinct views. The cold digest opens beside its supporting hot
records, with omitted and conflicting material visible; "Rebuild digest" is a dry-run diff and an
explicit confirmation. The actions are correct by adding a successor (the original is retained),
exclude from recall (a reason is required and history is kept), rebuild digest, propose lesson and
compare memory policy. Actions that change a release, exclusion or promotion show their reviewer
role and approval requirement in the review queue; each record shows its recall usage and
evaluation evidence.

The **scope selector** groups the levels by **organisation, work, execution and lifecycle**. One record
can appear in several authorised views without creating several memory stores; a tenant
administrator's ability to inspect records is separate from an agent's permission to recall them.

### Procedural knowledge — the PKO term workspace

The PKO is edited as structured terms and relations, not as a diagram. Each term carries its
definition, kind, permitted relationships (with cardinality), constraints, source evidence, owner
and release. Terms link to the procedure steps that use them; a searchable, kind-filterable table
handles routine editing, and a local two-hop graph helps exploration with the table as the
accessible authority. An edit creates a draft release and a review gate; an unknown term or an
invalid relation is refused (`422 pko_term_unknown` / `422 pko_relation_invalid`), and a term with
no definition blocks publication (`409 pko_not_ready`). RDF/OWL/Turtle and BPMN/Visio/draw.io remain
ingest material converted into draft rows behind the same gate.

### Explain context — why a run received or excluded information

From a run or call, the run inspector explains the context that was compiled:

- eligible source and memory versions, with their pins;
- what was retrieved, selected, excluded or truncated, **with the reason** for each and its budget
  impact;
- the inheritance rules and overrides that applied;
- a missing required item and the refusal code it produced;
- the budget consumed and the governing policy, refinement and laya-config versions.

The compiled prompt text is **never stored**, so the inspector never renders it. It shows the
recorded manifest, source identities and `prompt_sha256`, and states whether an exact reconstruction
is possible from retained inputs. When it is not, it names what is missing rather than inventing a
transcript.

### Evaluation from every object

The existing S17–S21 instruments gain contextual entry points from sources, passages, packs, PKO
terms, memory records, policies and jobs. The subject table makes the measured thing explicit:

| Subject | What the tenant measures |
|---|---|
| Knowledge extraction | missing content, incorrect extraction and evidence fidelity |
| Retrieval | expected-evidence recall, relevance and citation coverage |
| Procedural knowledge (PKO) | undefined terms, invalid relationships, contradictory constraints and scenario coverage |
| Memory | recall usefulness, stale/conflicting facts, unsupported synthesis and digest omissions |
| Scope policies | cross-card/project/tenant exclusion and authorised recall |
| Job outcomes | acceptance, failures, latency, context size and cost |

A memory change can compare **memory disabled / current policy / candidate policy**. Replay tests
have a defined time cutoff and isolated memory state so later information cannot leak into earlier
cases. Results show per-case improvements and regressions with sample size, uncertainty and cost;
human review status, model assessment and measured outcome stay visibly separate.

### Review and release

The review path is a queue, not a decorative stepper. Items from all three surfaces — source
corrections, pack versions and binding revisions, PKO release drafts, memory lesson promotions and
policy refinements — appear with the required reviewer role (source owner, capability owner, tenant
admin), required decisions, evidence references and age. The change-set lifecycle is **Draft → In
review → Approved → Published (immutable) → Superseded / Rolled back**. Approval records the person,
rationale, timestamp and evidence; a model or the proposing agent cannot approve. Rollback creates a
new version and keeps the supersession audit visible.

### The workflow and its acceptance criterion

A tenant can complete the whole loop through the UI:

> failed run → inspect its evidence and memory → identify the cause → draft a source or policy
> correction → compare against the baseline → review held-out results → approve a new release →
> monitor affected jobs.

The acceptance criterion is that this path is completable with visible evidence and authority at
every step. Backend capabilities that do not exist are labelled **V2 · PROPOSED**; the prototype
never implies that a preview is a working write path.

### Platform research — verified 2026-11-12

Two independent documentation passes verified all 16 links in the review brief. This was a
documentation review, not a hands-on audit of product accounts. The Letta ADE link is the V1 SDK
reference and is marked legacy/deprecated; the concepts are borrowed, not the current product shape.

| Platform | Surface reviewed | Borrowed into the workbench |
|---|---|---|
| Dify | [Knowledge Pipeline](https://dify.ai/blog/introducing-knowledge-pipeline), [Test retrieval](https://docs.dify.ai/en/cloud/use-dify/knowledge/test-retrieval) | visible processing stages with inspectable intermediate inputs; isolated retrieval tests |
| RAGFlow | [chunk management](https://ragflow.io/docs/v1.0.0/chunk_parsing_results_and_knowledge_fragment_management), [retrieval testing](https://ragflow.io/docs/v1.0.0/retrieval_testing) | original document beside extracted passages with a locator; corrections become versioned drafts |
| Letta | [ADE](https://docs.letta.com/v1-sdk/ade), [memory blocks](https://www.letta.com/blog/memory-blocks/) | named, bounded memory sections; visible budgets; explicit edit authority; context viewer and archival search |
| Zep | [graph visualizer](https://help.getzep.com/v4/graph-visualizer-assistant), [graph overview](https://help.getzep.com/graph-overview) | inspect entities/facts/source episodes; follow retrieval steps; validity timestamps and "why do we believe this?" |
| Mem0 | [memory lifecycle](https://mem0.ai/blog/stale-ai-agent-memory-and-how-mem0-dream-fixes-it) | active/superseded/merged/synthesized records; source evidence; current memory versus retained history |
| Guru | [verification](https://help.getguru.com/docs/verifying-and-unverifying-cards), [AI Agent Center](https://help.getguru.com/docs/training-guru-ai-agent-center) | named verifiers, review dates, verification status and a quality log; review queues and explanations |
| Glean | [verification](https://docs.glean.com/user-guide/knowledge/verification/how-verification-works) | source-owner responsibility, verification tasks, deprecation and visible reviewer/date |
| Palantir | [Ontology Manager](https://www.palantir.com/docs/foundry/ontology-manager/overview), [ontology proposals](https://www.palantir.com/docs/foundry/ontologies/review-ontology-proposals) | types, properties, relationships, dependencies, usage and observability; proposals reviewed before integration |
| WebProtégé | [user guide](https://protegewiki.stanford.edu/wiki/WebProtegeUsersGuide) | progressive disclosure: terms and relationships first, formal representation on demand; revision snapshots |
| LangSmith | [evaluation types](https://docs.langchain.com/langsmith/evaluation-types) | curated datasets, baseline/candidate experiments, per-example regressions and production findings becoming test cases |

**What Conveyor does differently:** every correction is a versioned draft behind a review gate
(RAGFlow's in-place chunk edit is not allowed); inheritance conflicts are compile errors resolved
only at the job layer; human inspection and agent recall are separate authorities; compiled prompt
text is never stored; and evaluation is tenant-authored with held-out, paired comparisons rather than
only daily checks derived from real answers.

### Honest status

| Workbench surface | Status |
|---|---|
| Capability → procedure → step, pack pins, retrieval, hot/cold work-package memory, lessons, evals, benchmarks, hillclimb, prompt audit | **V1 · implemented** (S15–S21) |
| Effective job-release configuration over industry/department/job, `department_conflict`, job-layer resolution, dependents | **V2 · proposed** (T0/T3); no V2 code |
| Seven-state source transformation, passage inspector, correction drafts, per-object evaluation and candidate cases | **V2 · proposed design extension**; no committed table or action |
| Memory taxonomy, Current/History/Derivation, successor/exclude/digest-rebuild/policy-compare actions | **V1 hot rows and consolidation + proposed curation views** |
| PKO term workspace, permitted relationships, graph and impact review | **V2 · proposed** (T16) |
| Explain-context manifest | V1 records `prompt_sha256`; the detailed manifest is proposed under SPEC_V2 §7.7 |
| Review queue and change-set lifecycle | **proposed**; the existing approval rows are the seed |

**Instruments carried from V1 (S17–S21):**

- **Benchmark / kata:** pinned SWE-bench suite hashes, image lock, parity grader and gold/empty
  controls; comparison refuses a different suite, oracle or prompt; a service job records
  keep/revert/inconclusive; subject-version trends and baseline promotion.
- **Task eval / hillclimb:** tenant outcome cases, versioned metrics and graders, immutable split,
  digest-bound sign-offs and held-out evidence access. The climb records goal, guardrails, allowed
  changes, prediction/falsifier, noise band, ceiling, cadence and stopping condition. Cost search
  uses caching health → prompt audit → model × effort → prompt climb → joint confirmation.

The quality fixture uses 160 train / 80 held-out cases and 3 repetitions; its approximate noise
floor is `1/sqrt(80*3) ≈ 6.5 pp` and the default decision band is 8 pp. Baseline is 70% held-out;
v1 reaches 85%. For the displayed rep-1 paired comparison, both=53, neither=9, baseline-only=3,
candidate-only=15; the exact two-sided McNemar value is computed from those fixture counts
(`p≈0.0075`). Subsequent training-only improvements are reverted. Benchmark fixture counts use a
separate 50-case instrument (both=18, neither=18, baseline-only=2, candidate-only=12; `p≈0.0129`).
The two instruments are not pooled. These calculations explain the study's local decisions; they do
not measure real application quality.

**Metrics and metering (D37, D52).** Flow, quality, cost, governance and knowledge metrics are
computed read models over rows, never stored opinions. C0–C5 maturity thresholds are calibrated
from real run/approval data and stored as configuration, not hard-coded. A gate that never refused
is flagged. Metering rolls up at tenant, department/industry, programme, project and job, and drills
down to run, step and call; attribution is call → run → card → project → programme → tenant; the
job attribution is the run's binding; the department attribution is the binding's primary
department, with a breakdown by the others; an unattributed call is reported separately, never
dropped. `billing_service` remains the charging system; the conveyor's metering reconciles to it
and a divergence opens an incident. The UI reads the same derivation the API serves.

**The improvement loop is one loop:** alert / failed run / eval delta / prompt finding / coach
escalation → coach card (one interruption, closed remedy set) → kata (six Gets, evidence,
candidate) → development project or procedure/prompt/knowledge change → held-out measurement →
adoption gate (person; dual control) → production (new procedure/job/binding version) → monitoring
and lesson. The kata's inputs are the prompt findings and eval deltas V1 already produces.

## Execution and real actions

The execution room combines the selectable topology with activity, evidence, compiled context, the
permitted tools, readiness and a human gate. A run records its job binding and the release hash it
actually used, its executor kind and its budget. Operator notes append local card memory. Gate
decisions require reasons and record **George · person · Web**. The detailed sample remains
WP-2048; other work packages do not invent a transcript when no sample run exists.

The **compiled prompt** is built at run time from instruction, procedure, role, knowledge and
project/card sections in trust order, with character budgets, a measured refusal over budget and a
recorded `prompt_sha256`; the compiled text is never stored. For an `internal_agent` step the four
sources — knowledge, episodic memory, the PDDL request and the curated tool list — are compiled into
the same envelope; there is no other prompt compiler.

The **coding catalogue** remains available as a local source snapshot: 367 API operations from the
coding gateway's OpenAPI (via the `n8n-nodes-coding` package), with original resource/operation
names, routes, fields and producer/observer metadata. It is the V1 reference for the coding
toolkit's operations; n8n is a retired system, not a tool, and the V2 tool estate imports the
toolkit through the broker. Registration and agent grants stay separate; a producer response alone
never establishes verification.

## Tenant administration — BYO credentials, integrations, licences and tiers

The studio's tenant-administration surfaces are implemented in the Rust app (`/settings/providers`,
`/settings/integrations`, `/settings/licences`, `/measure/model-tiers`); the studio documents them
and the Tools screen shows the V2 credential-profile and toolkit-licence vocabulary.

- **Providers & credentials** (`/settings/providers`): real `secrets.list/create/rotate/delete`;
  only metadata is read back and no action reveals a value. The wired names are
  `strata_ingest.token`, `strata_rag.token` and `laya.token`. inference-router, mlx-api, image-gen,
  tts, asr, webscraper_service, the w2v orchestrator, the agent-platform MCP relay and the Telegram
  bot are planned rows; the Telegram bot token is deployment configuration today. A missing token is
  a refusal, never a silent fallback. In V2 these become **credential profiles** resolved by the
  broker by alias; the prompt and the `tool_call` row never carry the value.
- **Integrations** (`/settings/integrations`): MCP and Telegram bind/unbind are real
  (`channels.bind/unbind`; the Telegram config uses `chat_id` and `topic_id`); Keycloak is the app's
  real `/config.json`; schedules, deliveries and webhooks are real. `channels.list` is a declared gap
  and the screen names it. MCP and Telegram are **channels**, never executors.
- **Licences & billing** (`/settings/licences`): `licences.get` and the billing document are real;
  the service × user matrix is a preview. The toolkit's `licence app` links entitlement to the tool
  estate. The proposed contract is a tenant-scoped `service_licence` table plus
  `service_licences.list/assign/revoke`; the seeded seats are super-admin, execution, kata and the
  test users. The matrix uses the w2v slot vocabulary (`llm`, `vlm`, `image`, `tts`, `asr`, `ocr`,
  `video_gen`) plus strata_ingest, strata_rag, laya, inference-router, mlx-api and the w2v
  orchestrator. w2v's licence states are `active | expiring_soon | expired | revoked | none |
  not_onboarded`; the conveyor's own `app_licence` states are `active | expired | revoked |
  unknown`.
- **LLM tiers (future).** The vocabulary and form mirror fr's real `/settings/llm-provider`:
  `__default__`, `small` (flash), `medium` (reasoning), `high` (frontier); each tier carries a
  provider, model, base URL, a write-only API key with a keep/clear tri-state, temperature and max
  tokens, plus Test / Save / Remove. A tier with no row falls back to the default. The licence
  posture is `local_allowed | byo_required | platform_only`. SPEC_V2 makes model providers and
  `model_ref` conveyor-owned; no model-tier registry exists today. The proposed contract is
  `model_tiers.list/upsert`; `/measure/model-tiers` is a labelled preview whose only live controls
  are links into evals, benchmarks and hillclimb.
- **GPU and media slots.** The service vocabulary follows w2v_frontend's tenant slots: `llm`,
  `vlm`, `image`, `tts`, `asr`, `ocr` and `video_gen`; each slot's real form is provider type, base
  URL, model, a voice for `tts`, and a write-only key with a fingerprint and Replace / Clear /
  Keep, plus Load models / Test / Save / Remove. `webscraper_service` and `media_discovery` are
  platform-only in w2v; `w2v_orchestrator` is the backend and its tenant licence belongs in the
  licence matrix. The agent's provider form adds Name, Kind (`openai | laya | w2v`), comma-separated
  Models and a Remove-the-stored-token control; care-bus's route config adds region, timeout,
  fallback chain, PII redaction, vision EXIF and a monthly spend cap. The conveyor preview shows the
  union of those fields and wires none of them.

These surfaces do not change the boundaries above: no action, table or event was added by the
design pack, and every preview row is local design data.

## System architecture

The **System architecture** destination (`architecture`, under GOVERN & DESIGN) is the reverse map
of this design pack: it draws SPEC_V2 and then shows which studio destination views which part of
it. It is a design study, not a claim that V2 exists.

### High level — ArchiMate 3.2

Three inline-SVG views use the ArchiMate 3.2 notation and layer bands:

- **Organisation and work** (motivation + business): principles P1–P11, the tenant and human
  approver, the PM/coach/kata/delivery/operations/reviewer positions, the four processes, the four
  services and the objects (job release, job binding, card, approval, run, evidence, memory, pack,
  lesson). Solid = V1 implemented; dashed = V2 proposed; each element carries a V1/V2 chip.
- **Platform and integrations** (application): Web/Telegram/MCP channels, the conveyor components
  (API/ActionSpec, domain, workflow, executors, decision ladder, knowledge/memory, tool broker,
  instruments, metering, auth, storage, events) and the backend services (laya, inference-router,
  strata, toolkits, MinIO, billing/licenses, Keycloak). The agent platform is a channel only.
- **Deployment** (technology): ingress → `conveyor :8930` pods → Postgres 18/MinIO; Flux and the
  registry; the external services; and the explicit “no Redis, no new port, no external executor”
  constraint. The tool broker is in-process.

A notation legend names the element shapes, the relationship line types and the V1/V2 status
convention. Every view cites its SPEC_V2 sections.

### Low level — Mermaid

Thirteen `.mmd` sources are pre-rendered to light and dark SVG pairs at authoring time and
committed under `architecture/diagrams/`; `scripts/build-architecture-diagrams.sh` regenerates them.
The page never loads a Mermaid runtime and makes no external request. The diagrams cover:

- the component/context flow, the run-execution sequence (procedure vs internal_agent, binding
  check, prompt compile, gates), the seven-step tool-broker sequence with its 409/403/422/429
  refusals, the run state machine and the L0–L3 decision ladder;
- three ER diagrams (organisation/work, execution/decisions, knowledge/tools), the knowledge
  pipeline (SOP/RDF/BPMN/chat → PKO + PDDL + pack → review → publish → pin → run) and the memory /
  lesson lifecycle;
- the one improvement loop and the T0–T17 slice DAG.

The Mermaid source is shown in a collapsible block under each figure; `check-architecture.cjs`
asserts it is byte-equal to the `.mmd` file, so the picture and the source cannot drift.

### UI map

A Mermaid overview plus a table for **every sidebar destination**: page id, architecture
element(s), SPEC_V2 section(s) and status (`R` V1 implemented, `P` V2 proposed, `L` legacy study).
A second table lists the tenant-administration app surfaces (providers, integrations, licences,
model tiers) that are not mockup destinations. The map is the explicit answer to “where does this
feature live in the design?”.

### Honest status

The ArchiMate views, the Mermaid sources and the UI map are **design-study artefacts**. No V1
element is changed by this page; every V2 element is a proposal. The page is one destination in the
same offline pack and obeys the same no-backend, no-network, no-write-path rules.

## Destination map

Status: **R** = V1-implemented surface, **P** = V2 proposed preview (ribbon, no write path), **L** =
legacy study retained with a banner. Existing page ids and routes are unchanged; new destinations
are additive.

| V2 destination | Studio page id | Screens | Status |
|---|---|---|---|
| Today / programme control | `overview` | Delivery map, integrated plan, gate readiness | R |
| **Portfolio** | `portfolio` | In flight, programmes, shared config & dependencies, backlog & WSJF, cadence & capacity, metering | **P** |
| Projects | `projects` | live/snapshot programme workspace: project list, board, plan, linked issues, text outputs, public feedback, publish | R (V1 fixture retired; #70 live/snapshot) |
| **Plan** | `plan` | WBS, baseline vs forecast, registers, change control | **P** |
| Execution room | `execution` | activity, evidence, context, tools, explain-context manifest | R + P tools / explain context |
| Decision inbox | `decisions` | approvals | R |
| Run history | `runs` | runs, two executors, binding pin | R |
| **Organisation** | `organisation` | authority matrix, approval policies, decision ladder, refinements & laya config, job performance | **P** |
| **Jobs** | `jobs` | catalogue, departments, releases, bindings, anatomy, legacy import | **P** |
| Agents / executors | `agents` | internal agent configuration | R + P note |
| **Tools** | `toolkits` | toolkits, bindings & grants, calls, probes, BYO, authoring, storage, credential profiles | **P** |
| Project types | `types` | definition, used by, versions | R + P V2 types |
| **Procedures** | `procedures` | workspace, SOP intake, RDF/OWL & process docs, PDDL, PKO term workspace, pairing, process map, publish | **P** |
| Capabilities | `capabilities` | definition, used by, versions | R |
| **Authoring** | `authoring` | intake queue, drafts, review & publish, dependency status | **P** |
| Assurance & baselines | `assurance` | traceability, architecture cycle, registers, baselines | R |
| Flow & constraints | `constraints` | flow/coach; the TOC study is optional | L |
| Requirements / principles / models | `requirements`, `principles`, `models` | governance and ArchiMate/UML | R |
| Knowledge design / sources / memory | `knowledge-design`, `knowledge`, `memory` | effective configuration, source transformation, record lifecycle, review & release, evaluation | R + P workbench |
| **System architecture** | `architecture` | ArchiMate high-level views, Mermaid low-level design, UI destination map | design study |
| **Improve** | `improve` | kata queue, six Gets, coach, lessons, adoption | **P** |
| **Improve** | `improve` | kata queue, six Gets, coach, lessons, adoption | **P** |
| Evaluation / benchmark / hillclimb / audit | `evals`, `benchmarks`, `hillclimb`, `usage` | S17–S21 instruments | R |
| **Metering** | `metering` | roll-ups, attribution, metrics & maturity | **P** |
| **Operations** | `operations` | incidents, on-call, runbooks, read/write, postmortems, andon | **P** |
| Agent job types | `jobtypes` | historical 92-entry snapshot | L |
| Coding action catalogue | `tools` | 367 coding operations | L |

## Existing implementation versus proposed UI

**Implemented in V1 (the Rust app, live at `conveyor.nonagentic.ai`):** the one binary with
projects, boards, lists, cards, runs, approvals, procedures, capabilities, capability agents,
knowledge/memory bindings, the inference ledger, the native SWE-bench harness, evals, the eval
hillclimb and the compiled-prompt pipeline; secrets, channels, webhooks, schedules, notifications,
`licences.get`, billing, members, roles, audit and the admin surfaces. The last whole-tree
checkpoint measured 83 tables and 243 actions; later S19–S21 lanes moved the served-spec past 290
ops, so those are floors, not live figures. The eval screen already exposes cases/runs/report/
sign-offs/climb and marks authoring, audit and interop as planned; the benchmark screen permits
gold/empty controls and refuses unavailable subject arms.

**Proposed in V2 (T0–T17, no V2 code written):** the organisation spine (`industry`, `department`,
`job_catalogue`, `job_release`, `job_binding`); the plan spine (`wbs_node`, `project_baseline`,
`project_register_item`, `project_dependency`, programme shared config and cadence); the decision
tables and laya client; PM/coach/kata/development/operations jobs; lessons; PKO and SOP/RDF/BPMN
ingestion; tenant asset authoring; the tool estate (`tool`, `toolkit`, `tool_binding`, `tool_call`,
the broker, BYO and authoring); metering roll-ups; the MinIO storage adapter; and the knowledge and
memory workbench (effective job-release configuration, the seven-state source transformation,
memory record lifecycle and derivation views, the review/release queue, the explain-context
manifest and per-object evaluation). The studio renders these as labelled previews.

The V1 retention is concrete: capability packs, `strata_ingest`/`strata_rag` retrieval, hot
work-package rows and cold digests, lesson promotion, the compiled-prompt pipeline and the S17–S21
instruments exist in the Rust app. The workbench's organisation inheritance, source/passage
inspector, record taxonomy and actions, PKO term workspace, explain-context manifest and object
evaluation are design previews; no table or action was added by this pack.

## Files and boundaries

`studio.html` is the entry point. `studio.js` / `studio.css` retain the S12 editors and
workbenches; `enterprise.js` / `enterprise.css` supply the programme surfaces and the sources &
retrieval workbench; `learning.js` / `learning.css` add knowledge design, the effective
configuration and review surfaces, scoped memory and measurement; `v2.js` / `v2.css` add the
SPEC_V2 destinations (portfolio, plan, organisation, jobs, procedures, authoring, improve,
operations, tools, metering), the PKO term workspace, the explain-context panel and the V2 sidebar
and banners; `architecture.js` / `architecture.css` add the System architecture page, its ArchiMate
model and the UI map; `architecture/diagrams/` holds the Mermaid `.mmd` sources and their
pre-rendered light/dark SVG pairs; `job-catalogue.js` and `coding-catalogue.js` are local source
snapshots. Local IBM Plex fonts and code-native SVG are used. The architecture page adds only the
committed SVG diagram pairs beyond the existing stylesheets.

`data/` is the live-data boundary. `data/snapshot.js` is a placeholder by default
(`window.CONVEYOR_SNAPSHOT = null`, DESIGN mode); `conveyor snapshot --out docs/design/data`
overwrites it with a `design-snapshot/v1` payload plus the raw `manifest.json`,
`programme.json`, `organisation.json`, `config.json`, `work.json`, `artefacts.json`,
`measure.json`, `secrets.json`, `evidence.json` and `feedback.json` files. When the payload is
present, the Portfolio, Organisation, Jobs, Plan (Boards tab) and Metering pages replace the
fixtures they cover with the platform's own rows and show a **LIVE** ribbon with the payload hash,
timestamp and git sha; every destination or tab with no live source keeps the DESIGN ribbon.
The Plan **Boards** tab is a join, not a seed: it renders each project's issues from
`programme.json`'s `issue_registry` (and the project's `meta.issues`) against the `work.json` cards
and runs. A unit with no real card is shown as **not intaken**; there is no committed plan
fallback. `data/issues.json` is the committed GitHub issue registry (generated by
`scripts/fetch-issues.sh`). The payload never calls GitHub or the app backend at page load.

### Two surfaces — the snapshot and the console

The pack has two deployments with different contracts:

- **The public snapshot** (`nonagentic.ai/design`): read-only. The page load never calls GitHub or
  the app backend, there are no accounts, and the privacy contract is structural rows only. On a
  checkout with no snapshot it is the offline `DESIGN` study. Everything resets on reload except
  the theme preference.
- **The operator console** (`conveyor.nonagentic.ai/design`, served by the app image): the same
  pack, same origin as the API. `console.js` loads `/config.json` from that origin, signs in with
  auth-code + PKCE S256 against realm `conveyor` / public client `conveyor-web` (the access token
  is held in memory, ADR 0001) and calls the API the server already serves. The Plan **Boards**
  tab then renders the tenant's own projects, boards and cards from the read routes and offers
  the real card loop — `cards.create` (intake an issue), `cards.move`, `cards.comment`,
  `cards.claim`, `cards.release` — gated by `GET /api/actions` and by the session. The
  **Projects** destination adds the programme workspace: project create/edit/close/execute
  (`projects.update` under If-Match), text outputs (`documents.compose` / `documents.update`, with
  the `publishable` flag), feedback intake and **Publish**. Publish enqueues `publish.design`,
  which builds the pack and opens a pull request on the homepage repository; the console never
  merges it. Every control produces the action request it names; nothing is simulated on this
  path. Signed out (and on any origin with no config, including `file://` and the homepage) there
  is no API request and no write control.

The image's web stage copies the generated documents (`docs/`) and the data directory beside the
runtime files, so the console's Documents destination and data links resolve. The console gate is
`docs/design/scripts/check-console.cjs`: the signed-out pack is asserted to make zero API requests,
the signed-in pack is asserted to send the intake/move/comment/claim/release requests, and the
401/412 paths are asserted to be rendered rather than retried. `check-studio.cjs` keeps asserting
zero external network dependencies for the offline study.

Approvals, gates, document/evidence authoring and the publish pipeline are **not** wired here.
The live tenant holds no approval rows yet, and published intermediate outputs need their own
document contract; both are separate units. Native dialogs, text-labelled status, keyboard
navigation, reduced motion and internal scrolling support the interaction study. Wider WBS tables,
models, timelines and graphs keep internal scroll.

Regenerate the historical snapshot with:

```sh
python3 docs/design/scripts/build-job-catalogue.py
```

The script reads sibling repositories, writes only `job-catalogue.js` and records the RM source
SHA-256. It is a display extractor, not an executable migration or a parser of live n8n prompts.

## Verification

Five headless Chrome passes run against `studio.html` using `playwright-core` and local Chrome; set
`NODE_PATH` if Playwright is installed outside this repo, `CHROME_BIN` for a non-default browser,
and `CONVEYOR_QA_OUTPUT` to change the screenshot directory (defaults to the system temporary
directory). Run from any working directory:

```sh
node docs/design/scripts/check-studio.cjs       # 234 checks
node docs/design/scripts/check-learning.cjs     # 139 checks
node docs/design/scripts/check-v2.cjs           # 158 checks
node docs/design/scripts/check-workbench.cjs    # 119 checks
node docs/design/scripts/check-architecture.cjs # 141 checks
node docs/design/scripts/check-data.cjs         # snapshot gate (DESIGN: no-op)
```

`check-data.cjs` is the generated-data gate: when `data/snapshot.js` carries a payload it verifies
the manifest schema, every file's SHA-256 and byte length, the `window.CONVEYOR_SNAPSHOT`
assignment, the secret-metadata-only rule, the absence of emails and credential-shaped strings,
and that `studio.html` loads the snapshot before `v2.js`. With the placeholder it reports DESIGN
mode and exits 0.

- `check-studio.cjs` covers programme switching, project creation, WBS expansion, job authoring
  and publication, work-package creation, human decisions, immutable execution pins, baseline
  change, TOC inspection, source ingestion, provenance, cited retrieval, missing-pack refusal, pack
  publication, hot memory, consolidation, context manifests and original coding-option lookup.
- `check-learning.cjs` covers independent capability/process/agent selection, memory-policy
  narrowing, all eight memory levels, card recall isolation, lesson curation, sign-off requirements
  and invalidation, analyzer/reviewer views, noise-band refusal, stall triage, one-round approval,
  training-only revert, paired exact comparisons, incompatible instrument/oracle refusal,
  independent instrument/campaign state, case authoring, cost guardrails, adoption drafts and
  prompt audit decisions.
- `check-v2.cjs` covers the ten V2 destinations and the System architecture map at 1440/1024/390,
  deep links and breadcrumbs,
  programme switching and WSJF, dependency-cycle refusal, change control with and without a
  rationale, dual control and the decision ladder, release/binding/stale-binding refusal, SOP and
  PDDL refusals, PKO versioning, publish readiness, authoring dependency blocks, kata/lesson/
  adoption gates, dry-run and destructive approvals, the andon cord, probe unbinding, effect
  ceilings, the tool-call ledger, BYO and tool authoring, metering drill-down and the run-inspector
  binding/tools panel — plus the V1 banners on the legacy surfaces.
- `check-workbench.cjs` covers the knowledge and memory workbench: the effective-configuration
  inheritance table and conflict resolution, missing-required refusal, Current/Draft/Compare,
  dependents and the review/release queue (reviewer roles, change-set states, no agent approval,
  rollback and supersession); the seven source states, passage locator and correction draft, quality
  signals, retrieval test with expected evidence and the candidate eval case; the memory taxonomy,
  six questions, Current/History/Derivation, successor/exclude/digest actions and one-reference
  lesson refusal; the PKO term workspace, usage and invalid-relation refusal; the explain-context
  manifest with reasons, budget, hash and reconstruction status; and the object-evaluation subjects
  and memory three-arm comparison — at 1440/1024/390.
- `check-architecture.cjs` covers the System architecture destination: title/breadcrumb/sidebar and
  deep link, the five tabs, the three ArchiMate views and their legend, all thirteen embedded Mermaid
  sources (asserted byte-equal to their `.mmd` files), the five runtime, five data/knowledge and two
  improvement diagrams, every sidebar destination in the UI map, the four tenant-administration
  rows, 1440/1024/390, the dark-theme SVG pair and no runtime errors, failed assets or external
  requests.

All five passes record no runtime errors, failed assets or external page requests. Each V2
destination and each workbench surface was checked at **1440, 1024 and 390 pixels**; mobile
navigation and Escape dismissal were checked in the studio pass. Screenshots cover the ten V2
destinations, the workbench surfaces, the architecture views, the mobile portfolio and the dark
operations view. JavaScript syntax, local asset resolution and `git diff --check` are part of the
final gate.

These checks verify the offline interaction study, not backend integration, security enforcement,
scheduling calculations, semantic model legality or standards conformance.
