# SPEC V2 — conveyor: the organisational operating model

**Repo:** `nonagenticai/conveyor` · **Host:** `conveyor.nonagentic.ai` · **Port:** 8930 (existing; no new port)
**Date:** 2026-11-11 · **Status:** draft for review · no V2 code written
**Relationship to `SPEC.md`:** V1 (2026-11-07) remains the build record and stays binding for the slices it defines.
V2 extends it — it adds **the organisation that works the conveyor** and **the work hierarchy that organisation
needs**. V1 decisions D1–D27 stand unless a V2 decision explicitly extends or amends them (Appendix B). No V1 slice
is re-opened by this document.

> **In one paragraph.** V2 turns the conveyor from a work system into an organisation. Jobs are configured positions
> (industry → department → job) that receive cards, decide with rules and laya, generate with LLMs behind gates, and
> prove their work with runs and instruments. PM, kanban coach, kata, development and operations are jobs, not code.
> TOGAF/SAFe/PMP artefacts are rows, gates and evidence. Every cluster service a job may call is a versioned, bound,
> audited tool. The conveyor develops and operates itself under the same gates it imposes on any work. **The conveyor
> owns its whole model and is as self-contained as possible**: it reuses ideas — and where useful, code — from the
> agent, w2v, fr and care-bus applications, but holds no runtime dependency on any of them and treats the agent
> platform only as a user-interaction front end over MCP.

---

## 1. Context

### 1.1 What V1 built

V1 built **one Rust application to replace fr, RM, Planka and procedure_management** — the app is live; the old
applications remain until the S14 cutover. It built the machine that can carry work: projects, boards, lists, cards,
runs, approvals, procedures, capabilities, capability agents, knowledge and memory bindings, an inference ledger, a
native SWE-bench harness, evals, evals' hillclimb and the compiled-prompt pipeline.

The V1 build record's last whole-tree checkpoint (2026-11-09, `docs/plans/00-INDEX.md`) measured **83 tables and
243 actions**; S19–S21 lanes moved the registry past 290 served-spec ops that same day, so those numbers are a floor,
not a live figure. The app is **live** at `conveyor.nonagentic.ai` on main `85487a04`, 3 pods, `/health` and
`/health/ready` 200 on `:8930` (deploy record 2026-11-10).

V1 answers *how work is carried*. It does not answer *who the organisation is*: there is no industry, no department,
no job, no programme cadence, no PM/coach/kata role, no self-development project and no operations perspective. A
project is created by a person; a card is typed by a person; the improvement loop exists but nothing staffs it.

### 1.2 The three convergences

V2 exists because three independently built things have arrived at the same shape:

1. **`agent` is a user of the conveyor over MCP.** D19/D21 made the conveyor's own `/mcp` the interface: the agent
   creates a project, starts one durable task (`projects.execute`), answers its approvals as task input requests and
   receives the terminal result (the agent-side plan of 2026-11-08). It is a front end — a conversation surface like
   Telegram — with its own autonomy configuration; it is not a catalogue, an executor or a system of record for
   conveyor work. The **live end-to-end proof is the S06b/S14 gate**, not a claim about today's deployment.
2. **The organisational hierarchy is proven.** The agent platform's KH (D106–D115; its §4 rollout record of
   2026-10-23 reports agent#72 `e47c92e` deployed, migrations `0132`–`0139`, seed v3 applied) demonstrates the model
   the conveyor now implements itself: every job one industry, one or more ordered departments and an optional
   region; a job compiles base → industry → department archetype → job (with region after its parent) into an
   immutable release, with knowledge kinds, packs, instructions, settings, toolsets and System-1 gates. **The
   conveyor reuses the ideas, not the implementation** — it is self-contained (D33).
3. **The conveyor has the work and the proof.** Capability → procedure → step → action|gate is the procedural
   knowledge; `run`/`approval`/`action_invocation`/`event` is the procedural memory; `capability_agent` is the
   executor configuration; `inference_call` is the ledger; benchmark, evals and prompt audit are the instruments.

The missing piece is the **organisation**: the conveyor already carries work and proof, and V2 gives it its own
catalogue, immutable release, binding and inference configuration. The join is internal — no manifest is imported
from another system.

`care-bus-agents` remains the standalone customer-interaction demo; it is **not** part of the organisational runtime.
Customer care is not conveyor work (D48): the conveyor is built for project, continuous-line and line-management
work, starting with a real IT department's development and operations, and later any capability in any industry.

### 1.3 What the field does, and what it does not

As of the sources fetched 2026-11-11:

- **CrewAI** is a strong ergonomic layer for a *crew*: agents with roles/goals, tasks with expected outputs, and a
  `Process` that is either `sequential` or `hierarchical` (a manager agent delegates and validates). Memory is one
  unified `Memory` class over embeddings and a vector index. Typed control lives in **Flows**, not in the Crew
  abstraction; no runtime human-approval gate is documented on the Crews page. It is excellent at *starting* a small
  collaborative process and says nothing about portfolios, phase gates, evidence, budgets or an org chart.
- **ChatDev** has moved to **ChatDev 2.0 (DevAll, released 2026-01-07)**, a zero-code multi-agent platform; its 1.x
  "virtual software company" (CEO/CTO/programmer/reviewer/designer, design→coding→testing→documenting) remains the
  mental model. It demonstrates role specialisation and visible handoffs; it does not attempt an operating model.
- **n8n** is the best deterministic workflow engine we have used: a node graph with per-execution state and error
  branches. Its weakness is the one we measured in our own estate — **348 workflows, 280 of them the generated
  card-type × list matrix**, each carrying its own error exits. A flow engine gives you execution, not a process
  model.

The gap is consistent: the field builds **agent conversations**; real organisations run on **positions, work
products, gates, baselines and evidence**. That is the gap V2 fills, with the rigour the owner asked for — TOGAF for
architecture, SAFe for project/programme management, PMP artefacts — implemented as **executable rows and gates**, not
as documents.

### 1.4 What the previous conveyor taught us

The v4 kanban design (the last serious attempt, in `n8n-designs/design/choices/v4`) had the right roles and the
wrong substrate. We carry its concepts and delete its mechanism. Measured from those documents:

| Finding | Consequence for V2 |
|---|---|
| PM = `project_management`, "the only RM writer after the request", owns phases, artifact sign-offs and TODO/READY prioritisation | Keep the authority split; implement it as a **job** with a charter and gates, not a service account |
| Coach = "protector of flow"; every interruption becomes a coach card; triage transient/defect/decision; remedies `rerun\|move\|repull\|block\|ask\|note` | Keep one-card-per-interruption and the **closed remedy set**; add a recursion depth cap and an evidence requirement |
| Kata = `workflow_engineering`, six Gets, `wf_change`, replay 2/3 | Keep the six Gets and the measured candidate; replace flow-patching with versioned procedure/prompt/code changes |
| 280 generated card-type × list flows; four `wf_*` types "serve nothing"; the generator was deleted ("live flow is the truth") | **No generated flows.** Procedures are data; one engine |
| E32 duplicate card birth; 35 uncovered error exits; the kata record showed no kata closed (a moving count the source warns against quoting); an open-ended coach LLM rung fixed "0 of 22 blocks" | Idempotency is structural; the coach has a closed decision set; a kata that cannot measure is not opened |

The Python substrate is why these fixes never compounded. V2 keeps the Rust substrate V1 built: typed actions, one
schema, one binary, compiler-enforced crate boundaries, no flow generator.

---

## 2. The thesis

**An organisation is a set of positions making decisions and doing work under governance.**

Four units, and only four:

| Unit | What it is | Where it lives |
|---|---|---|
| **Job** | a position: a capability + a procedure + an inference configuration + an authority | `conveyor` (catalogue, release, binding) |
| **Card** | a work package: one unit of assigned work with an owner, a state and evidence | `conveyor` |
| **Approval** | a decision: the one human-escalation primitive (a question is an approval) | `conveyor` |
| **Run** | one attempt at a card through an executor, recorded | `conveyor` |

Everything else is a view or an instrument. Departments, programmes and portfolios organise jobs and cards; knowledge,
memory and tools configure a job; evals, benchmarks and the inference ledger measure it; TOGAF/SAFe/PMP supply the
process grammar. **The differentiator is not multi-agent conversation — it is that every handoff, decision and claim
is a durable row with an owner, a state and a proof.**

First principles the spec is built on:

1. **An organisation is a decision system, not a chat system.** Agents do not need to talk to each other; they need to
   exchange typed work products through a governed process.
2. **Most decisions are not language problems.** They are choices among configured options. A small calibrated model
   (laya) is cheaper, faster and more auditable than an LLM for that.
3. **Generation is the LLM's only job.** Text, code, plans and reports are generated; they become drafts; applying a
   draft is an action behind a gate.
4. **Trust comes from instruments, not from confidence.** A change is adopted because a held-out eval or benchmark
   moved, not because an agent says it improved.
5. **The system must be able to build and run itself** under the same gates it imposes on any work — otherwise the
   process is theatre.

---

## 3. Principles — hard invariants

| # | Principle | Enforcement |
|---|---|---|
| P1 | **One record per fact.** No concept is stored in two systems; systems join by pinned reference | Ownership matrix (§10.1); a contract test per seam |
| P2 | **Rule → laya → generation → human.** An LLM never selects, routes, classifies or approves | The decision ladder (§7); generation results have no path to a state transition |
| P3 | **No work without a card; no transition without an action; no completion without evidence** | Domain layer; `card_type.definition` acceptance; `run`/`approval` rows |
| P4 | **The executor never approves its own work** | Dual control on gates; `decided_by_kind`; approver ≠ assignee/executor |
| P5 | **Measurement before adoption.** A change to a procedure, prompt, job or code path needs an instrument | Eval/benchmark version pin; sign-offs; noise band |
| P6 | **The meta-jobs are ordinary jobs.** PM, coach, kata, development and operations use the same catalogue, card, run and approval primitives | No hard-coded role logic; every role is a `job_key` |
| P7 | **The system builds itself under its own gates.** Development projects change conveyor and its dependency repos through the same process | D34 guardrails; merge/deploy gates |
| P8 | **Fail closed on missing configuration.** An unresolved binding, pack, prompt source or tool refuses before the first model call or action | `409 *_not_ready` with `blocking[]` |
| P9 | **Every model call is ledgered and budgeted** | `inference_call`; per-run/per-tenant budgets |
| P10 | **No speculative aggregates.** A table exists because a named procedure uses it and a metric reads it | Design review; `D40`; usage measured after one programme cycle |
| P11 | **Tenant configuration over code.** Every policy, threshold, limit, gate condition and approval kind is a versioned tenant-scoped row with a platform default; code implements mechanisms, never policy values | `*_policy`/`*_config` rows; a policy change is itself a gate; a hard-coded policy value is a review finding |

---

## 4. The organisational model

### 4.1 The hierarchy

V2 adopts the KH hierarchy — proven by the agent platform (D106–D115) — as the conveyor's **own** organisation
spine. The concepts are reused; the implementation is the conveyor's (D33).

```
industry 1 ── * department (ordered, primary first)
                    └── * job
region?  ── optional sub-layer of any layer (folds immediately after its parent; no region content seeded yet)
```

- A job sits in **exactly one industry** and **one or more departments of that industry**, ordered, primary first
  (D106/D115). A department from another industry is `422 department_industry_mismatch`.
- Knowledge is layered **base → industry → department archetype → each department in declared order (with its region
  sub-layer) → job**. Kinds are declared at every layer; a kind is required or optional; `required` can only be
  tightened downward. Sibling departments that set one key to different values are a **compile error**
  (`department_conflict`), resolved only by a job-layer row.
- The **job release** is immutable and pinned. A run records `job_key` + `release_hash`; a release change is a new
  binding and a gate, never a silent update. There is no live design → `409 job_not_live`; no resolvable required
  knowledge → `409 release_not_ready` naming the kind and department.

The catalogue is the conveyor's own data (`industry`, `department`, `job_catalogue`, `job_release`); the release
compiles from the same layers and is stored as an immutable artefact with a content hash. No manifest is imported
from another system, and no other system is asked to validate a release.

### 4.2 Job anatomy — the configured position

A job is **configuration, not a loop**. Its release carries:

**One author: the conveyor.** A job is one record with one owner (D33). Every field below lives in the conveyor's
catalogue or its immutable release; nothing is authored in another system for a conveyor job.

| Field | Meaning | Author |
|---|---|---|
| `job_key` | stable position key, e.g. `software.delivery.backend_engineer` | conveyor (`job_catalogue`) |
| placement | industry, ordered departments, optional region | conveyor (`job_catalogue`) |
| `charter` | purpose, outcomes, non-goals, escalation path | conveyor (release layer `instruction`) |
| capability binding | one or more capabilities | conveyor (`job_binding`) |
| `procedure_ref` | published procedure version(s), each pinning its PKO release | conveyor (`procedure`) |
| `executor` | `procedure \| internal_agent` (V1 D22, amended by D46) | conveyor (`job_binding`) |
| `model_ref`, `params` | inference-router model and parameters | conveyor (release) |
| `prompt_recipe` | compiled-prompt recipe (`prompt_ref@version`) | conveyor (release) |
| knowledge pins | layer kind pins + `capability` pack versions | conveyor (release) |
| memory policy | recall scope, hot/cold, write rules | conveyor (release) |
| tool grants | the release's operation allowlist (`op`, `variant`, `max_side_effect`, `enabled`) and per-call tool gate | conveyor (release) |
| tool binding | pinned toolkit versions, effect ceiling and credential alias for the tools behind those grants (§11) | conveyor (`tool_binding`) |
| decision options | laya option sets and policies for the work decisions the job makes | conveyor (`decision_*`) |
| limits | max iterations, timeout, token and cost budget | conveyor (release) |
| gates | required approvals, dual control, evidence requirements | conveyor (binding + tenant policy) |
| authority | what the job may do without a gate | conveyor binding |
| metrics | computed from execution, never stored as opinion | conveyor |

**Rule:** a job may lower a limit at run time, never raise it. The resolved `{job_key, release_hash, procedure_ref,
model_ref, allowed_actions}` travels in the run's input hash, so the record pins what actually ran.

### 4.3 How jobs are derived

Jobs are authored, not invented at runtime. The derivation method is the use-case chain the agent platform proved
(D106–D115); the conveyor applies it to its own catalogue:

```
industry / department / job
  → business model (axis selection)
  → customer or internal journey (themes → stages)
  → stage = capability column
  → activity (customer | company)
  → activity-information objects (objects × DO | REQUEST | OBSERVE)
  → use case → context space + procedure
  → job draft
```

A **draft catalogue** may be generated from the chain; a person curates it into published jobs. V2 explicitly does
**not** auto-generate a thousand live jobs from an ontology — that is how the v4 generator produced four `wf_*` types
that "serve nothing". The catalogue grows by evidence: a missing job is opened as a kata or a PM request, authored,
bound, measured, then published.

### 4.4 The job binding — position, release, executor

```
job_binding
  id, tenant_id,
  job_key, release_hash,
  capability_id?, procedure_id?, executor_kind,
  status ∈ draft|active|retired, version,
  meta { department_scope[], authority, limits_override? }
```

- A card names a binding (`card.job_binding_id`), not a free-text role.
- A run records the binding **and** the resolved release it actually used.
- A binding is tenant-scoped; the same `job_key` can be bound to different capabilities in two tenants.
- Retiring a binding does not rewrite history: existing runs keep their pinned reference.
- **A binding pins one immutable release.** A new release is a new binding and a gate (D38); it never rewrites a
  live binding. A run request naming a retired binding, or naming a release that is not the binding's pin, is refused
  `409 job_binding_stale` before any model call; the refusal names the binding's release hash and the requested one.
  The check is internal — no catalogue call, no external caller-declared hash.
- The binding stores the release's content hash and its frozen configuration snapshot (`release_hash` +
  `release_snapshot` JSONB). It is a pinned artefact, like a lockfile, not a live mirror; a new release is a new
  binding and the old snapshot is never refreshed in place.
- Every binding also pins `executor_kind` and either a conveyor `procedure_id` (for a procedure executor) or the
  release's `capability_agent_id` (for an internal-agent executor). A binding with neither is refused at creation.

### 4.5 Executor kinds

V1 D22 is **amended by D46** (and by D61, once accepted): a run has one executor, and there are **two live kinds**
behind one internal `Executor` contract; the kind is to be an extensible enum.

| Executor | When | Config source |
|---|---|---|
| `procedure` | a deterministic sequence of steps; no model of its own | conveyor `procedure` |
| `internal_agent` | a step needs generation over a compiled prompt | conveyor `capability_agent` + the release |

**A procedure is a sequence of prompt parts.** Each step is an action — a PDDL action, a tool call, or a gate — and
the sequence is the plan. Deterministic steps execute directly; a step whose action needs generation hands to the
internal agent.

**The internal agent receives a dynamically engineered prompt** assembled at run time from four sources:

| Source | What it contributes |
|---|---|
| **Knowledge** | domain knowledge (pinned layer kinds and packs) and procedural knowledge (the capability/procedure/PKO) |
| **Memory** | episodic memory: the run's prior steps, the card's hot memory and the consolidated digest |
| **Request** | the action the agent must perform — the procedure step written in PDDL |
| **Tools** | the curated list of tools bound to this job, with their closed schemas |

The generation contract (§7.4) still applies: the output is a draft; applying it is a gated action.

**The executor seam (D61).** Both kinds are to implement one internal `Executor` contract (T0 builds it);
`executor_kind` is to be an extensible enum, not a branch through the run path. Adding a kind is a new implementation plus one enum value, never an edit to
the run loop. **There is no external executor today**, and an external runtime — the agent platform or any harness —
is an explicit non-goal until all four hold: a generic work-packet ingress that returns per-step evidence the
conveyor can verify (not outcome-level); governance at the boundary (a produce-only runtime, the conveyor ships);
separate owners and roadmaps; and it beats the internal agent on the conveyor's own benchmark. The agent platform is
a front end, not a runtime for conveyor jobs; if it is down, no conveyor run pauses.

### 4.6 Scope and the first departments

**The conveyor is for project work, continuous-line work and line-management work.** It is a general engine for
intelligent work execution with configurable human-in-the-loop: a tenant configures how much autonomy each job and
gate has, and may offload human interaction to the agent platform over MCP. It does **not** do customer care (D48);
customer care remains the agent platform's own product.

The first tenant is `nonagentic`; the first industry is `software`; the first department set is a real IT
department — **development and operations** — because the conveyor must develop and operate itself before it can be
trusted with any other industry's work. Two further tenants are in scope from the same model: **AHTAPol**
(industry `health-technology-assessment`) and **w2v** (industry `media`), each with its own departments and jobs
(D60) — the proof that the model is not software-specific. Nothing below is hard-coded — each row is a job in the
catalogue.

| Department | Jobs (initial) | First work |
|---|---|---|
| **delivery** | product owner, architect, tech lead, backend/frontend/data/platform engineer, reviewer, release manager, technical writer | deliver conveyor features and dependency-repo changes |
| **verification** | QA engineer, security reviewer, performance engineer | independent verification, red-team, eval authoring |
| **operations** | incident commander, SRE/on-call, platform operator | k8s/git incidents, runbooks, postmortems |
| **project office** | PM, programme manager, portfolio analyst | charters, WBS, baselines, cadence, change control |
| **improvement** | kanban coach, kata facilitator, measurement engineer | interruptions, systemic fixes, instruments |

The first **real** job to instantiate is *conveyor maintenance* — the system's own upkeep — followed by *feature
delivery* and then *operations*. Self-development is not a later phase; it is the first proof (§9).

**Scale is a catalogue property, not a process property.** A department's jobs are grouped into families (product,
architecture, engineering, verification, release, operations, knowledge); a family can hold dozens of specific jobs
(backend-rust, frontend-leptos, postgres, security-review, release-manager, …). The catalogue is searchable by
code/capability/department and paginated; a tenant activates the subset it needs. Thousands of jobs are thousands of
rows and bindings, not thousands of processes (D39).

---

## 5. The work model

### 5.1 The hierarchy

V1 already ships `programme` and `project.programme_id`. V2 completes the hierarchy:

```
tenant
  ├── portfolio   — computed: every project and programme in flight for the tenant (no row)
  └── programme   — a set of projects with shared configuration and project-level dependencies
        └── project — may also stand alone, outside any programme
              └── WBS / deliverable → work package (card) → run → step / attempt
```

**Owner definition (2026-11-11, D49).** A **portfolio** is *all projects and programmes in flight for a tenant*; a
**programme** is *a set of projects with shared configuration and project-level dependencies*. The portfolio is a
read model over `programme` and `project` (P10 — no row); the programme is a real aggregate carrying `shared_config`
and the project dependency graph.

| Level | Meaning | State |
|---|---|---|
| **Portfolio** | every project and programme in flight for the tenant — a computed view, never a row | none (read model) |
| **Programme** | a set of projects with shared config and project-level dependencies; carries the budget and cadence | `active \| paused \| closed` |
| **Project** | a temporary endeavour with a charter and a baseline | `draft \| active \| paused \| closed \| archived` |
| **WBS node** | phase, deliverable, work package or milestone; a tree under the project | open → accepted / cancelled |
| **Work package** | the card: one assignment, one owner, one list | list + `archived_at` (V1: the list is the state) |
| **Run** | one execution attempt | `queued → running → waiting_approval → running → succeeded \| failed \| cancelled` |

- **Portfolio membership** is derived: a project or programme is in flight when its status is in the tenant's
  configured in-flight set (default `active` and `paused`); closed and archived work is history and is not listed.
- **Programme shared config** is inherited by its projects: knowledge pins and packs, default job families,
  approval-policy defaults, budget envelope and cadence. A project overrides a key only when the programme does not
  declare it locked; the lock set is tenant policy.
- **Project-level dependencies are edges**, not prose: `project_dependency(from_project, to_project, kind ∈
  blocks | needs | supersedes | informs, status)`, inside or across programmes. The PM reads them when sequencing the
  WBS; a cycle is refused at creation.

### 5.2 Project types

`project.project_type_code` selects the phase template, the gate set and the default job family:

| Type | Template | Gates |
|---|---|---|
| **delivery** | charter → plan → build → verify → release → close (PMP) | design (conditional), merge, deploy, close |
| **architecture** | TOGAF ADM phases A–H (§8.1) | per-phase contract gate |
| **improvement (kata)** | six Gets (§6.3) | hypothesis, adoption |
| **development (self)** | design → branch → implement → CI → review → merge → deploy → verify (§9.1) | design (structural), merge, deploy |
| **operations** | detect → triage → mitigate → resolve → postmortem (§9.2) | destructive-action gate only |
| **setup (authoring)** | intake → asset design → review → publish → validate (§5.7, T17) | per-asset review; publish gate |

The project type is a **template**, not code: its phases are WBS node kinds, its gates are `approval` rows with the
gate name and evidence refs, and its artefacts are `governed_asset` rows (V1's one lifecycle for requirements,
principles and ADRs, extended to charter/contract/closure/postmortem).

### 5.3 Plan, state and execution are different things

Three separations are load-bearing and are the V1 fixes carried forward:

1. **Project lifecycle ≠ card state ≠ run state.** A card can move without answering its approval; a completed run
   does not close a project.
2. **Baseline ≠ forecast.** A baseline is a frozen, approved snapshot; a forecast is computed from current rows. A
   change to scope/schedule/cost is a **change request** (`project_register_item.kind='change'`) with an impact
   analysis and a person's decision; existing runs keep their pins.
3. **Plan ≠ evidence.** A WBS node is accepted when its acceptance criteria are met by an artefact/run, not when its
   checkbox is ticked.

### 5.4 Gates, evidence and dual control

- One primitive: **approval**. A gate is an `approval` row with `gate` (phase/DoD name), `kind`, `required_decisions`,
  `meta.evidence[]` (run ids, artefact ids, eval ids) and an optional `expires_at` (default: never expires).
- **Dual control** is `required_decisions ≥ 2` and distinct approvers; the executor/assignee/requester cannot be an
  approver for the gate that judges their work.
- **A gate with no evidence is refused** (`422 gate_evidence_missing`) unless the gate explicitly allows an override;
  the override is recorded on the approval and counted in the override metric.
- **Definition of Done** lives on `card_type.definition` and is checked by the domain layer before a card can enter a
  `done` list; an approval can override with a reason, which the coach reads.
- **No gate that cannot fail.** Every gate ships with a negative test (a run that must be refused). A gate that has
  never refused anything in a full programme cycle is flagged by the governance audit — that is a broken gate, not a
  healthy one.
- **Approvals are tenant-configurable and default to human.** Each gate names an `approval_policy` row: required
  decisions (`1` by default; `≥2` for dual control), the approver kind (`human` by default; a tenant may authorise a
  job or agent only for a gate it explicitly declares non-governance), and an expiry. The policy is itself a gated
  change; a gate whose policy is automatic is flagged in the governance audit. Human-in-the-loop may be offloaded to
  the agent platform over MCP, which presents the approval with its own autonomy configuration; the conveyor still
  records the decision and `decided_by_kind`/`via`.

### 5.5 Registers and change control

One generic register replaces five PM tables (the V1 lesson: RM's `pm_*` tables measured zero usage and were
dropped):

```
project_register_item
  id, tenant_id, project_id, kind ∈ risk|issue|assumption|dependency|benefit|change,
  code, title, body, owner_job_id, status, severity, probability, impact,
  links jsonb {cards[], requirements[], decisions[]}, meta, version
```

- A **change request** references the baseline it changes and requires a scope/schedule/cost/risk impact block; the
  approval records the person's rationale.
- A **risk** can open a mitigation card; a **dependency** can link two projects or an external system; a **benefit**
  is measured at closure (planned vs realised).

### 5.6 Procedure authoring and SOP ingestion

A procedure is the executable form of a standard operating procedure: an ordered sequence of steps, each an action
(PDDL), a tool call or a gate. Procedure design and management is a first-class conveyor feature (D47):

- **SOP intake.** A tenant supplies SOPs — PDF, web URL or another document — singly or as a batch. A customer
  document is untrusted input: it enters as a `governed_asset(kind='sop_source')` with provenance, never as
  instructions to the system.
- **Conversion.** The intake job harvests and curates the domain knowledge the SOP implies, then converts the SOP
  into **procedural knowledge (the Procedural Knowledge Ontology, PKO)** and a **procedure draft whose steps are
  written in PDDL**. The conversion job uses the web/research tools, and `lean4_prover` where a claim needs formal
  checking.
- **Two spaces, never mixed (D53).** The **action space** is the procedure step written in PDDL — actions with
  preconditions and effects over typed facts. The **procedural knowledge space** is the PKO — the ontology of tasks,
  steps, roles, artefacts and constraints. A step references PKO terms; a PKO entity never contains an executable step.
- **PKO storage and ingestion.** The PKO is relational, conveyor-owned and tenant-editable: an immutable
  `pko_release` pins `pko_entity` (name, label, kind, attributes, synonyms) and `pko_relation` (subject, predicate,
  object, cardinality) rows, layered base → industry → department → job. RDF/OWL/Turtle documents are ingest
  material — a converter turns them into draft rows behind the review gate; the conveyor runs no RDF store and no
  reasoner. The old `procedure_management` OWL/Turtle files import this way once (owner, 2026-11-11).
- **Process documents.** BPMN 2.0 XML, Visio and draw.io exports ingest on the same path: the diagram becomes a
  procedure draft (activities → steps, gateways → gates) plus PKO rows. The diagram is a source document; the
  procedure is the executable artefact, and the same review gate applies.
- **Design reuse.** The same pipeline designs capabilities, agents and jobs: an SOP yields a capability, a procedure,
  a job draft and its knowledge pins; a person reviews and publishes.
- **Knowledge pairing.** The domain knowledge harvested for an SOP is stored as a pinned knowledge pack in
  `strata_rag` and paired with the procedural knowledge; the procedure's release names both. The conveyor uses
  `strata_ingest` and `strata_rag` at least to the extent the agent platform already does — document ingestion,
  pinned-leaf retrieval, pack storage and consolidation — and never hosts embeddings.
- **Gates.** Every SOP-derived artefact is a draft behind a review gate (default: human, tenant-configurable). The
  ontology and the procedure are versioned and measured like any other procedure change (P5); a batch never
  publishes itself, and no SOP can grant a tool or widen an effect ceiling.

### 5.7 Tenant asset authoring — from chat and documents (draft)

**Owner concept, 2026-11-11 (D55); the first-wave asset matrix is an open question (T17).** The same intake pipeline
that converts an SOP is the general authoring path: a tenant supplies **chat input and documents**, and the conveyor
drafts **every tenant-scoped asset** it needs.

| Input | Produces (draft) |
|---|---|
| Chat (web or the agent front end over MCP) | organisation draft (industry, department, job catalogue), project templates, gate/approval policies, tool requests |
| SOP (PDF, URL, document) | capability, procedure (PDDL), PKO rows, knowledge pack, job draft |
| BPMN / Visio / draw.io | procedure (activities → steps, gateways → gates), PKO rows |
| RDF / OWL / Turtle | PKO entities and relations |
| Existing policy/procedure exports | procedure or policy drafts |

Rules:

- **Every output is a draft behind a review gate** (tenant-configurable, default human); publication is a release and
  a gate. A chat message or a document can never grant a tool, widen an effect ceiling, approve its own artefact or
  change an approval policy.
- **Asset order is dependency-ordered:** industry → department → job/capability → procedure/PKO → knowledge pack →
  policies/templates → tools. The authoring job refuses a draft whose referenced asset is unpublished.
- **The authoring job family is ordinary jobs** on the two executors (§4.5); the pipeline is a procedure, not a
  hard-coded wizard.
- **Instruments:** drafts accepted vs rejected, revisions per artefact, time from intake to first published job. An
  authored catalogue is measured by use — an asset no procedure uses is retired (P10).
- **The chat surface does not decide.** Chat input is a request; rules/laya route it; the LLM drafts; a person
  approves. Tenant asset creation is subject to the same decision ladder as any work (§7).

The detailed asset matrix, review depth per asset type and the first wave ship with T17; nothing above widens a tool
grant or a gate policy without its own approval.

---

## 6. The roles — meta-jobs as ordinary jobs

Each role is a `job_key` with a charter, a procedure, an authority matrix and metrics. They run on the **same** card,
run, approval and event primitives as any other job, on the conveyor's two executor kinds — a procedure or the
internal agent (§4.5).

### 6.1 PM — project management

| | |
|---|---|
| **Charter** | turn a request into a governed project; define scope, WBS, plan, baseline, dependencies, risks and acceptance; sequence work; route gates; close with evidence |
| **Inputs** | request, portfolio priorities, architecture contracts, constraints, historical velocity |
| **Outputs** | project + charter, WBS, baseline, cards with job bindings, gate decisions, status report, closure record |
| **Laya decides** | request class, project type, priority band (WSJF), required capability set, gate set, escalation target |
| **LLM generates** | charter text, WBS draft, status narrative, closure narrative |
| **Never** | approve its own gates; change an approved baseline without a change request; do the work of a delivery job |
| **Gates** | charter approval, baseline approval, change requests, closure |

The PM is the only job that may create a project and the only job that may close one. It cannot be the sole approver of
either.

### 6.2 Kanban coach — flow protection

| | |
|---|---|
| **Charter** | protect flow; every interruption becomes exactly one coach card; triage and either apply a bounded remedy or escalate; open a kata when the evidence says systemic |
| **Inputs** | board events, failed runs, stale leases, WIP breaches, gate timeouts, repeated signatures |
| **Outputs** | one coach card per interruption; a triage row; a remedy action; an escalation; a kata candidate |
| **Closed triage set** | `transient \| conveyor_defect \| job_defect \| project_decision \| external` |
| **Closed remedy set** | `retry \| move \| repull \| block \| ask \| note` |
| **Laya decides** | triage class and remedy, over the configured option set; confidence below policy → `ask` (human) |
| **LLM generates** | the diagnosis narrative attached to the card; never the remedy |
| **Never** | do the work it is unblocking ("the coach organises; a work capability does the work"); open a coach card about a coach card; retry the same signature beyond the policy cap |
| **Recursion guard** | one card per `(card_id, signature)` while open; a second same-signature block within the window escalates to a human, never to another coach card |

Measured lesson: the old open-ended coach LLM rung fixed "0 of 22 blocks", and duplicate coach cards made the board
unreadable. The closed sets and the per-signature uniqueness are the fix.

### 6.3 Kata — system improvement

| | |
|---|---|
| **Charter** | improve the system by changing a named part of it; prove the change with an instrument; hand adoption to a person |
| **Procedure** | six Gets: **Go to See** (observe the work and its evidence) → **Grasp the Situation** (problem statement with data) → **Get to Solution** (candidate change) → **Get to Standardization** (procedure/prompt/knowledge/job update) → **Get to Sustainability** (monitoring + regression guard) → **Get to Stretch** (yokoten: propagate to other jobs/departments) |
| **Inputs** | failed runs, eval deltas, benchmark regressions, prompt findings, coach escalations, ops postmortems |
| **Outputs** | observation, problem statement, hypothesis + pre-registered prediction, change candidate, measurement, adoption decision, propagation record |
| **Laya decides** | which class of change (procedure \| prompt \| knowledge \| tool \| job config \| code) and which target surface |
| **LLM generates** | the analysis and the candidate change (procedure text, prompt diff, code patch) |
| **Never** | edit the instrument that judges it; approve its own adoption; measure on the training split only; open a second kata for the same open signature |
| **Gates** | hypothesis approval (optional, for spend), adoption (mandatory), propagation (per target) |

A kata that cannot name an instrument and a success threshold is not opened. A failed kata keeps the andon cord until
a person releases it (V1/v4 ruling carried forward).

**The kata is its own tenant.** The improvement loop runs as tenant `kata`: its approvals, autonomy policy, budgets
and tool bindings are configured for the kata tenant, so a kata's write scope never inherits the target tenant's
autonomy (D45, owner 2026-11-11).

### 6.4 Development project — the self-developing path

A **project type**, staffed by delivery/verification jobs, whose deliverable is a change to the conveyor or one of its
dependency repositories.

| | |
|---|---|
| **Triggers** | a user request (PM), a kata candidate, a security/eval finding, a dependency upgrade |
| **Scope** | one repo and one lane per PR; scope is written into the charter before any branch |
| **Procedure** | design note (structural changes only) → lane branch → implement via coding jobs → CI green (fmt, clippy `-D warnings`, tests, gitleaks) → independent review → merge gate → deploy → live verify → close |
| **Tools** | the coding fleet's actions (`coding_jobs.create/get`), git/PR/CI status, the conveyor's own eval/benchmark actions |
| **Gates** | design (if architecture/contract change), merge, deploy; rollback plan required |
| **Never** | modify its own gates, approval policy, or the eval/benchmark instrument that measures it; force-push; merge with red CI; deploy without a live check |
| **Earned autonomy** | auto-merge is an `approval_policy` row with `approver_kind='auto'` and typed `conditions` — consecutive green runs, revert count, path scope, independent-reviewer requirement, expiry; every value is tenant configuration, never code; the policy is itself a gated change and defaults to off |

### 6.5 Operations — the platform perspective

The future the owner asked for: a job family that acts on **kubernetes or git issues**.

| | |
|---|---|
| **Charter** | detect, triage, mitigate, resolve and learn from incidents in the conveyor and its dependencies |
| **Inputs** | alerts, webhooks, schedules, user reports, cluster events, CI failures |
| **Outputs** | incident card, severity, runbook run, mitigation record, resolution, postmortem, kata candidate |
| **Read tools** | pods, deployments, events, logs, metrics, Flux status, git/CI status |
| **Write tools (gated)** | restart, scale, rollback, feature-flag flip and their read-backs; each requires `ops:write`, a dry-run, the tenant's configured approvals (default: human dual control), and an audit event. **The full write set ships in v1** — autonomy is configuration, not a reduced feature |
| **Autonomy** | tenant-configured per operation class; the default is human approval; an incident commander may hold a tenant-configured break-glass path with post-hoc review |
| **Laya decides** | severity and runbook selection over the configured sets; below threshold → escalate to on-call |
| **LLM generates** | diagnosis narrative, incident summary, postmortem draft |
| **Never** | use cluster-admin; act without a card; perform a destructive action without the tenant's configured approvals; "fix" a symptom by deleting evidence |

### 6.6 Collaboration protocol — handoffs are cards, not messages

1. **A job that needs work from another job creates and assigns a card.** It does not message an agent. The card names
   the job binding, the inputs (artefact ids), the acceptance criteria and the due date.
2. **A handoff is an action** (`cards.assign` / `cards.move` + `comment`/`artifact.attach`) producing an `event`.
   The receiving job's queue is its board/list; the PM's plan is the WBS.
3. **A question is an approval** (V1 D18/D19). If a job needs a human decision, it opens an `approval`; if it needs a
   decision from another job, it opens a card assigned to that job, optionally with a gate.
4. **Memory is written at handoff.** The sender records what the receiver must know (`memory.remember`), so a job does
   not depend on a conversation transcript that may be compacted.
5. **Every card has one live run** (`uq_run_live`); retries create attempts on the same run where the state machine
   permits, otherwise a new run with the previous as a link.
6. **Escalation is bounded**: a card may escalate to its parent WBS owner, then to the PM, then to the programme; the
   coach can escalate to a human at any point. No unbounded agent-to-agent loops.

### 6.7 Authority matrix (defaults; tenant-configurable)

| Action | PM | Coach | Kata | Delivery job | Ops | Human |
|---|---|---|---|---|---|---|
| Create/close project | ✅ | — | — | — | — | ✅ |
| Create/move card in project | ✅ | ✅ (remedy only) | — | own card | own incident | ✅ |
| Assign a job binding | ✅ | — | — | — | — | ✅ |
| Approve a phase gate | — | — | — | — | — | ✅ (dual) |
| Approve a merge | — | — | — | — | — | ✅ |
| Open a kata | ✅ | ✅ | — | — | ✅ | ✅ |
| Adopt a kata change | — | — | propose only | — | — | ✅ |
| Destructive ops action | — | — | — | — | propose + dry-run | ✅ (dual) |
| Publish a procedure | — | — | — | — | — | ✅ (or `procedures:write` + gate) |

The matrix is data (`tenant_role_scope` and the binding's `authority`), not branches in code. Every gate's required
decisions and approver kinds are tenant-configured policy rows defaulting to human approval (§5.4).

---

## 7. The decision ladder — laya first, LLM last

### 7.1 The four rungs

| Rung | Mechanism | Used for | Record |
|---|---|---|---|
| **L0 Rule** | deterministic Rust over typed facts | state transitions, WIP limits, WSJF arithmetic, thresholds, policy checks, acceptance | the row + `action_invocation` |
| **L1 System-1 (laya)** | calibrated choice/score/noul over a finite option set | triage, routing, job/capability selection, remedy selection, severity, priority band, gate pre-check | `decision` row with the answer, probabilities where the question type provides them, and confidence |
| **L2 Generation (LLM)** | text/code/plan/report synthesis | drafts, answers, patches, narratives, candidate procedures/prompts | `inference_call` + a draft artefact |
| **L3 Human** | approval | every gate; every override; every irreversible action | `approval` + `approval_decision` |

**The hard rule:** L2 has no path to a state transition. A generated output is stored as a draft artefact
(`governed_asset`, `document`, or a code patch); applying it is an **action**, and the action's gate is L1/L3. The
Rust type system enforces this: the generation result type is not accepted by any transition function.

### 7.2 The laya contract

`POST /v1/systemone {model, state, questions}` — `state` is the task text and `questions` is keyed by qid; each
question is `{type: noul|choice|score, instructions, criteria}` and the response carries `answers.<qid>`. `noul` is a
scalar answer, `choice` picks one of 2–255 labelled options, `score` is a 2–10 ordered scale; probabilities are
carried for the types that provide them. The estate's laya is live on pgx behind a Jev-compatible bridge (`:9210`),
~17–21 ms per request (measured 2026-10-10).

V2 rules:

- **Options are rows** (`decision_option`), authored and versioned, never free text from a prompt.
- **Questions are rows** (`decision_policy`), with `fire_when`, `min_confidence`, `fail_policy ∈ closed|open` and
  `action ∈ handover|block|annotate`. Safety gates fail closed; routing/quality gates fail open to a human or a note.
- **Configuration is per tenant, per case (D51).** A platform `decision_policy` defines the case; a tenant's
  `decision_policy_refinement` overrides the threshold value (or the whole rule), the fail action and, optionally, the
  laya inference config. Refinements are versioned; changing one is a gated policy change. The shape mirrors the agent
  platform's proven `job_gates.threshold` + `job_gate_rule_refinement`, reimplemented by the conveyor (D33).
- **Laya inference is tenant-scoped.** `laya_inference_config` pins `model_ref`, `params` and `timeout_ms` per tenant
  (provider kind `laya`, default `jev-latest`), optionally overridden per case. Every `decision` row records the policy
  version, the refinement version and the laya config version it used.
- **Every decision is recorded** with the option set version, the answer, probabilities where the type provides
  them, confidence, policy, latency and input hash. A decision is reproducible from the row.
- **Below threshold is an outcome, not an error**: escalate (`approval`) or refuse per policy. `Unavailable` is
  `gate_unavailable` and follows the fail policy; it is never silently treated as a pass.
- **The conveyor owns every question and option set its jobs evaluate**, including the internal agent's per-step
  gates. The agent platform owns only the questions its own conversation runtime evaluates — its product, not
  conveyor work.

### 7.3 Where laya is mandatory

| Decision | Options (examples) | Fail policy |
|---|---|---|
| Request classification | `new_feature \| bug \| question \| incident \| improvement` | open → human |
| Project type | `delivery \| architecture \| improvement \| development \| operations` | closed → PM |
| Job/capability routing | the tenant's active bindings for the department | closed → coach |
| Coach triage/remedy | the closed sets in §6.2 | open → `ask` |
| Severity | `sev1..sev4` | closed → on-call |
| Gate pre-check | `pass \| review \| refuse` | closed → human gate |
| Candidate selection in a kata | the registered candidate classes | open → kata facilitator |

An LLM may **describe** the options and recommend one in prose, but the recommendation is not consumed by the engine.

### 7.4 The generation contract

A generation call is an `inference_call` (V1 D23) whose result is bound to a draft artefact. The artefact carries
`prompt_ref`, `prompt_version`, `prompt_sha256`, `model_ref`, `usage`, `cost` (derived), and the trust order of its
sources. It becomes applicable only through an action with a gate.

Allowed generation: draft answer, draft charter/WBS, draft procedure, draft prompt/patch, code patch, status
narrative, postmortem narrative, lesson draft.
Forbidden generation: tool selection, routing, state transitions, gate decisions, scoring a submission, approving.

### 7.5 Budgets and the ledger

- Every run declares a token/cost budget; every job binding declares a default; exceeding it is a refusal
  (`429 budget_exceeded`) before the next call, not a silent truncation.
- `inference_call` keeps the prompt identity and usage, never the prompt/response bodies; the router keeps the text.
- The cache classes are defined (V1 D23): `cache_read > 0` = hit, `cache_read = 0 ∧ cache_creation > 0` = write,
  neither = miss, no usage = unknown.
- Cost is a read-time derivation over the ledger, never a stored column.

**Metrics are computed from these rows, never stored as opinion** (D37):

| Family | Metrics | Source rows |
|---|---|---|
| Flow | lead time, cycle time, queue time, block time, flow efficiency, WIP, throughput | `card`, `list`, `event`, `run` |
| Quality | first-pass yield, rework rate, defect escapes, eval score, benchmark delta | `run`, `approval`, `eval*`, `benchmark*` |
| Cost | tokens, cost per work package, cost per accepted artefact | `inference_call` |
| Governance | gate latency, approval rate, override rate, evidence completeness, never-refused gates | `approval`, `audit_event` |
| Knowledge | pack hit rate, citation rate, readiness blocks, lesson promotion rate | `decision`, `inference_call`, `lesson` |

**Metering — roll-ups and drill-down (D52).** Spend and usage are computed from the ledgers at call grain and
presented at every level of the organisation:

| Level | What it shows | Source rows |
|---|---|---|
| Tenant | total tokens, cost, tool spend and budget utilisation | `inference_call`, `tool_call`, `run` |
| Department / industry | the same, attributed through the job binding's primary department | + `job_binding` |
| Programme / portfolio | the same over the programme's projects; the portfolio is the in-flight set | + `project`, `project_dependency` |
| Project | cost per job, per card and per accepted artefact | + `card`, `wbs_node` |
| Job | cost per run, budget status, spend per tool | + `run`, `tool_call` |
| Run (drill-down) | cost per step and per call | `run_step`, `inference_call`, `tool_call` |
| Step / call (drill-down) | the ledger rows themselves | `inference_call`, `tool_call` |

- **Attribution:** a call belongs to its run → card → project (and its programme) → tenant; the job attribution is the
  run's `job_binding_id`; the department attribution is the binding's primary department, with a breakdown by the
  others. An unattributed call (`caller_kind='other'`) is reported separately, never silently dropped.
- **Cost is derived at read time** from the ledger and a pinned price version — tokens are stored, money is derived
  (D37). Budgets are enforced before the next call (`429`), not reported after the fact.
- **`billing_service` remains the charging system.** It already rolls up tenant × day × project × actor × app ×
  provider × model; the conveyor's metering reconciles to it and a divergence opens an incident, not a silent
  difference. `licenses` holds entitlement, not usage.
- **UI:** first-class panels on Portfolio (per project and programme), Organisation (per job and department) and
  Measure (tenant roll-up); the run inspector drills to step and tool call. The UI reads the same derivation the API
  serves; it never recomputes cost from a stale cache.

### 7.6 Knowledge and memory configuration

| | **Knowledge** | **Memory** |
|---|---|---|
| **Procedural** | `capability → procedure → step(PDDL) → action\|gate` + the pinned PKO release (conveyor) | `run`/`run_step`/`approval`/`action_invocation`/`event` (conveyor) |
| **Domain** | layered kinds + one versioned `capability` pack in `strata_rag`, pinned per job | **work-package memory**: hot rows in conveyor, one cold digest per card in `strata_rag` |
| **Organisational** | curated lessons promoted to a source draft (D36) | **lesson memory**: proposed → reviewed → promoted/rejected |

Rules carried from V1 and KH:

- A required kind with zero resolvable leaves **blocks the run** before the first step (checked by the conveyor
  before the first step); an optional source degrades with a recorded warning, never silently.
- A pack is pinned `key@version`; publishing a new version does not change a live run's pin.
- Work-package memory is hot rows + one derived digest; hot wins on conflict; the digest is rebuildable, the rows are
  not.
- **Lesson promotion** requires ≥2 independent evidence references (different cards/runs/incidents), a person's review
  and the normal source/pack release path. One bad run never becomes doctrine.
- The conveyor never hosts embeddings or a model; retrieval is `strata_rag`; generation is the executor.

### 7.7 Compiled prompts

V1 D27/S21 stands: a prompt is compiled at run time from the run's sources — instruction, procedure, capability/role,
knowledge and project/card sections in trust order — with character budgets, a measured refusal over budget, and a
recorded `prompt_sha256`. The compiled text is never stored. Missing required source → `409 capability_not_ready`
naming the pin and section, **before** a model call. An identical recipe + facts compiles byte-identically. For an
`internal_agent` step the four sources of §4.5 — knowledge, episodic memory, the PDDL request and the curated tool
list — are compiled into the same envelope; there is no other prompt compiler in the system.

---

## 8. Executable rigour — TOGAF, SAFe, PMP

The point of this section is that the rigour is **not a document library**. Every artefact below is a row with a
lifecycle; every gate is an approval; every claim links to evidence.

### 8.1 TOGAF ADM as an architecture project type

An `architecture` project walks the ADM phases as WBS nodes, each with its required artefacts and a contract gate:

| Phase | Artefacts (governed assets) | Gate |
|---|---|---|
| **Preliminary** | architecture principles, tailored ADM profile | principles approved |
| **A — Vision** | scope, stakeholders, architecture vision, constraints | vision contract |
| **B — Business** | business architecture, capabilities, value streams | business contract |
| **C — Information systems** | data + application architecture | IS contract |
| **D — Technology** | platform, infrastructure, deployment topology | technology contract |
| **E — Opportunities & solutions** | gap analysis, candidate solutions, work packages | solution contract |
| **F — Migration planning** | roadmap, increments, dependencies, cost/benefit | migration contract |
| **G — Implementation governance** | compliance reviews attached to delivery projects | per-delivery compliance |
| **H — Change management** | monitoring, new requests, lessons | change review → kata/PM |

**Requirements management is continuous**, not a phase: every requirement/principle/ADR is a `governed_asset` linked
by `trace_link` to the cards, runs and evidence that realise it. A delivery project cannot pass its design gate
without the architecture contract that governs it; a contract change is a new version and a new gate.

Scope note: this is a **lean ADM profile** for software delivery, not a claim of TOGAF certification or full
conformance. The phases, artefacts and gates are the ones the organisation will actually use and measure.

### 8.2 SAFe as the programme cadence

| SAFe concept | V2 implementation |
|---|---|
| Portfolio | the computed view of every project and programme in flight for the tenant; budget and strategy sit above it |
| Programme / ART | a `programme` with `shared_config`, a project dependency graph, a cadence and a standing backlog |
| PI planning | a `programme_cadence(kind='pi')` job that materialises the next cadence's draft cards from the backlog, ordered by WSJF, subject to WIP limits; a person approves the plan |
| Backlog refinement | a scheduled job that re-scores WSJF and flags stale items |
| System demo | a cadence event requiring each completed deliverable to exhibit its evidence; no demo, no credit |
| Inspect & adapt | a retro card producing lessons and kata candidates; owned by the coach |
| Lean governance | gates at value boundaries (phase/increment), not per card |

**WSJF is arithmetic, not opinion:** `(business_value + time_criticality + risk_reduction) / job_size`, all four typed
integer fields on the backlog/WBS row. The computed score is never stored; the inputs are. An LLM may draft the
narrative, never the numbers.

### 8.3 PMP artefacts as typed rows

| PMP artefact | V2 row |
|---|---|
| Charter | `governed_asset(kind='charter')` + project |
| WBS | `wbs_node` tree |
| Scope/schedule/cost baseline | `project_baseline` (frozen snapshot, approved) |
| Risk/issue/assumption/dependency/benefit registers | `project_register_item` (one table, six kinds) |
| Change control | `project_register_item(kind='change')` + approval with impact analysis |
| Acceptance/closure | `governed_asset(kind='closure')` + DoD evidence |

The registers are deliberately one table, not five: the v4 design had separate PM tables and V1 measured RM's `pm_*`
at zero usage. One generic register with a `kind` CHECK is leaner and still queryable.

### 8.4 The trace chain

```
requirement / principle / ADR (approved)
   → architecture contract (approved)
   → project charter (approved)
   → WBS deliverable → work package (card) → run → step → artefact / evidence
   → acceptance (approval) → closure
```

`trace_link` is V1's existing edge; V2 requires the link at creation time for architecture-governed work. The governance
screen answers one question end to end: *which requirement does this run prove, and where is the evidence?*

### 8.5 What is not claimed

No TOGAF certification, no SAFe compliance badge, no PMP certification. No full ADM conformance, no portfolio
optimiser, no schedule solver. The claim is narrower and testable: **the artefacts and gates the organisation uses are
rows, the process runs, and the evidence is traceable.**

---

## 9. Self-development and operations

### 9.1 Development projects — the conveyor builds itself

The organisation's first real job is to change its own substrate. A development project is an ordinary project with a
`development` type and a stricter gate set:

1. **Charter** names the repo, the lane, the scope and the acceptance evidence.
2. **Design note** for structural changes (architecture contract, migration plan, rollback).
3. **Lane branch** (`lane/<slug>`), one branch, at most one PR for the unit's life (house rule).
4. **Implement** through coding jobs; the executor cannot merge.
5. **Verify**: CI-equivalent locally first (fmt, clippy `-D warnings`, the full suite), then the pipeline; gitleaks;
   the relevant eval/benchmark if the change touches a measured surface.
6. **Review**: an independent reviewer job; the reviewer cannot be the implementer.
7. **Merge gate**: a human (or an earned auto-merge policy) approves; red CI is a refusal, never a retry-until-green.
8. **Deploy and verify live**: rollout by image digest, `/health`, the slice's own acceptance check; a rollback plan
   exists before the deploy.
9. **Close**: the closure record links the PR, the pipeline, the deploy evidence and the acceptance.

**Dependency repos are in scope.** A dependency change is a development project against that repo, with the same
gates; the conveyor's own dependency bump is a linked project, not a side effect. A dependency is in scope only if its
own change protocol permits an agent project; **shared infrastructure** (cluster control plane, ingress, DNS,
registry) remains a human-run change, never a development project.

### 9.2 Operations — the platform perspective

The operations job family closes the loop on the substrate:

- **Incident** is a card type with `service`, `severity`, `detected_at`, `mitigated_at`, `resolved_at` and a link to
  the run/evidence. Detection is an alert/webhook/schedule; a person can also open one.
- **Runbook** is a published procedure whose steps are k8s/git actions with gates. The first runbooks: pod crash-loop,
  rollout stuck, dependency/DB down, certificate expiry, disk pressure, queue backlog, stale lease storm.
- **Read-first**: diagnose with read actions (pods, deployments, events, logs, metrics, Flux status, git/CI). Write
  actions (restart, scale, rollback, flag flip) are `ops:write`, dry-run first, approved under the tenant's
  configured policy (default human dual control), and audited. The full write set ships in v1; autonomy is a tenant
  configuration, not a reduced feature.
- **Least privilege**: the ops connector uses a namespace-scoped service account, never cluster-admin; the action
  registry declares every verb.
- **Postmortem** is a `governed_asset`; a systemic finding becomes a kata candidate; a one-off becomes a runbook
  improvement. Blameless, evidence-linked, due within the policy window.
- **The andon cord** (V1/fr) is an incident-class action: stop new runs for a tenant/programme, keep the record, alert
  the on-call; only a person releases it.

### 9.3 Guardrails — the system cannot grade its own homework

- A development project **cannot modify the gate set, approval policy, eval instrument or benchmark suite** that
  judges it. Those are changed by a separate architecture/governance project with a different approver set.
- A kata **cannot approve** its own adoption; a kata that changed the instrument is void for that measurement.
- The **merge gate** and the **deploy gate** are separate decisions; a green CI is evidence, not an approval.
- **Auto-merge is earned**, policy-scoped and revocable: the conditions (track record, revert count, path scope,
  reviewer requirement, expiry) are tenant configuration rows, not constants in code; one revert revokes it and opens
  a kata. The default is off; the database model, the UI and the code all read the policy, never a hard-coded number.
- **Rollback is mandatory** for deployable changes; a deploy without a tested rollback is refused.
- The **benchmark harness and eval versions are frozen per measurement** (V1 D24–D26); a change to an instrument
  creates a new version and voids its sign-offs.

### 9.4 The improvement loop, end to end

```
alert / failed run / eval delta / prompt finding / coach escalation
  → coach card (one interruption, closed remedy set)
  → if systemic: kata (six Gets, evidence, candidate)
  → development project (code) or procedure/prompt/knowledge change (config)
  → measurement (benchmark / eval / run outcome, held-out)
  → adoption gate (person; dual control)
  → production (new procedure/job/binding version)
  → monitoring + lesson
```

This is one loop, not four. The kata-executor's inputs are the prompt findings and eval deltas V1 already produces;
the coach's escalations are cards; the PM's closure record is evidence; the operations postmortem feeds it back.

---

## 10. Architecture and ownership

### 10.1 Ownership matrix — one record per fact

| Concern | System of record | Consumer |
|---|---|---|
| Industry, department, job, job release, knowledge layers/kinds, packs, PKO/ontology releases, decision policies and their tenant refinements, laya inference config, model providers, tool grants, channels, conversations/turns | **`conveyor`** | jobs execute on it; humans and the agent front end |
| Tool estate: tool/toolkit versions, tool bindings, credential profiles, the `tool_call` ledger (§11) | **`conveyor`** | conveyor jobs through the broker; humans |
| Tool backends: the coding fleet, the kag-era services, the w2v fleet and `video-compiler`, AHTAPol services, k8s/git APIs | each service / the cluster | the conveyor broker only (§11.3) |
| Programme, project, WBS, board/list/card, run, approval, procedure, capability, requirements/principles/ADRs, evidence, evals/benchmark, inference ledger, decision options/policies, job bindings | **`conveyor`** | humans; the agent front end over `/mcp` |
| Documents/bytes | conveyor `document` + **MinIO**, written by the conveyor's own storage adapter under tenant-separated prefixes (§11.10) | all |
| Domain knowledge bytes/embeddings | `strata_rag` / `strata_ingest` | conveyor |
| Models/inference | `inference-router` for text; `mlx-api`/`modality-gateway` for media; laya on pgx | conveyor |
| Charging and entitlement: cost rollups, plans, licences | `licenses`, `billing_service`; the conveyor's metering read models are computed from its own ledgers | conveyor |
| Git/CI execution | the `coding-*` fleet, through the conveyor broker's coding toolkit (§11) | development jobs |
| Kubernetes | the cluster, through the ops toolkit (§11.2) | operations jobs |
| The agent platform's own conversations, cases, journeys and its autonomy configuration | **`agent`** (its product, outside the conveyor's model) | its own users and channels |

**The conveyor owns every decision it evaluates.** Its `decision` rows cover work routing, gates and the internal
agent's per-step choices; there is no second decision store for conveyor jobs. The agent platform's own runtime
decisions are its product and never enter the conveyor's model.

**The join is internal.** `job_binding` references the conveyor's own `job_catalogue`, `job_release`, `capability`
and `procedure` rows; `release_hash` pins the immutable release. There is no imported manifest and no live copy of
another system's data.

**Channels.** The conveyor's web/Telegram/MCP channels are the **work and approval** surfaces (cards, runs, gates,
task input requests). The agent platform is a conversation front end over MCP with its own autonomy configuration; a
conversation that needs internal work opens a card across the MCP seam. Customer care is not conveyor work (D48).

### 10.2 Seams and contracts

| Seam | Contract |
|---|---|
| agent → conveyor | MCP `/mcp`, licence bearer, MCP tasks (V1 D19/D21); the agent front end acts as a user; tool names carry `_meta.operation`; a task start names the `job_binding_id`; the tool surface is versioned like an external API (V1 R11) |
| conveyor → agent | **no call.** The agent front end may present approvals and questions with its own autonomy configuration; the conveyor records the decision and `decided_by_kind`/`via`. An agent outage does not stop any conveyor run |
| conveyor → laya | `POST /v1/systemone`, 1500 ms default / 3 s hard budget; fail policy per decision policy |
| conveyor → inference-router | V1 D23: `inference_call` ledger, prompt identity, cache classes |
| conveyor → strata | `knowledge.query` over pinned `leaf_ids`; `ingest.*` for authoring and SOP/RDF/process-document intake; `memory.consolidate` writes one digest per card; `strata` is never the system of record |
| conveyor → coding | the coding toolkit through the broker (§11.3): operations imported from the coding gateway's `openapi.json`; timeout/breaker; normalised result |
| conveyor → k8s | the ops toolkit through the broker: namespace-scoped service account; read/write verbs declared as tools with effect classes (§11.4) |
| conveyor → tool backends | the broker (§11.3): registered destination alias, sealed credential profile, closed input schema, effect class, one `tool_call` row. No caller-supplied URL at call time; no runtime dependency on `nonagenticai_mcp_server` (§11.7) |
| conveyor → MinIO | the storage adapter: S3 API to tenant-prefixed keys, server-side encryption, content hash, size cap; bucket administration is never a job action (§11.10) |
| any → human | approval; web, Telegram, an MCP task input request, or the agent front end under its own autonomy configuration |

**How a meta-job actually runs.** The job is a conveyor release; its internal agent executes a PDDL step from a
prompt compiled from the four sources of §4.5, and its turn opens or advances a **card**. The unit of work is the
card/run, not the turn: a turn can end, the worker can restart, and the card remains with its owner, state and
evidence. The conveyor records the run, every decision and every model call; the agent front end, if it presented
the conversation, records only its own turns.

### 10.3 Data model additions (proposed; refined in slice plans)

| Table | Purpose | Notes |
|---|---|---|
| `industry` / `department` | the organisation spine | ordered departments; a job in one industry |
| `job_catalogue` | the position: key, placement, status | the conveyor's own job table; not V1's durable `job` queue |
| `job_release` | the immutable compiled release | content hash, snapshot, status; a change is a new release |
| `job_binding` | the join of a position to its release/capability/procedure/executor | tenant-scoped, versioned; immutable `release_snapshot` + `release_hash`; `executor_kind` is to be extensible (D61, T0) |
| `approval_policy` | per-gate approvals | required decisions, approver kinds, expiry, autonomy conditions; default human |
| `wbs_node` | phase/deliverable/work package/milestone tree | `card_id` nullable; WSJF inputs; acceptance |
| `project_baseline` | frozen, approved scope/schedule/cost | immutable; change requests reference it |
| `programme_cadence` | PI/sprint/demo/retro/refinement | state machine; materialises cards |
| `project_register_item` | risk/issue/assumption/dependency/benefit/change | one generic register, six kinds |
| `decision` | one L1 decision | probabilities, confidence, policy, latency, input hash |
| `decision_option` | options for a choice/score question | authored, versioned |
| `decision_policy` | thresholds, fail policy, action | per decision kind/gate |
| `lesson` | organisational memory proposal | evidence refs, independent count, review state |
| `project_dependency` | project-level dependency edges | `blocks \| needs \| supersedes \| informs`; a cycle is refused |
| `decision_policy_refinement` | tenant per-case decision config | threshold/rule override, fail action, optional laya config; versioned |
| `laya_inference_config` | tenant-scoped laya inference | `model_ref`, `params`, `timeout_ms`; optional per-case override |
| `pko_release` | immutable PKO/ontology release | content hash, layers, status; pinned by a procedure |
| `pko_entity` / `pko_relation` | PKO terms and edges | kind, attributes, synonyms; subject/predicate/object/cardinality |

Reused, not created: `programme` (extended with `shared_config`), `project`, `card`, `run`, `approval`, `governed_asset` (extend `kind` to
`charter | architecture_contract | closure | postmortem | sop_source`), `trace_link`, `event`, `job`, `audit_event`,
`inference_call`, `eval*`, `benchmark*`; `procedure`/`procedure_step` extend with PDDL action text and a PKO pin. New
columns only where unavoidable: `card.job_binding_id`, `card.wbs_node_id`, `run.job_binding_id`/`release_hash`,
`project.project_type_code` (exists). The tool tables (`tool`, `toolkit`, `tool_binding`, `tool_call`) are specified
in §11.8.

### 10.4 Action families

| Family | Examples |
|---|---|
| programme/plan | `programmes.*`, `programme_config.*`, `project_dependencies.*`, `cadences.*`, `wbs.*`, `baselines.*`, `registers.*` |
| organisation | `industries.*`, `departments.*`, `jobs.create\|update\|release\|retire`, `job_releases.publish\|retire`, `job_bindings.create\|retire\|list` |
| procedures | `procedures.ingest_sop\|ingest_process\|ingest_rdf\|draft\|validate\|publish`, `capabilities.design\|publish`, `pko.import_rdf\|edit\|publish\|pin` |
| authoring | `authoring.intake\|draft\|review\|publish`, `authoring.assets` |
| approvals | `approval_policies.list\|get\|set`, `approvals.request\|decide\|escalate` |
| decision | `decisions.open\|evaluate\|decide\|escalate`, `decision_options.*`, `decision_policies.*\|refine`, `laya_configs.*` |
| PM | `pm.charter`, `pm.plan`, `pm.baseline`, `pm.close` |
| coach | `coach.interrupt`, `coach.triage`, `coach.remedy`, `coach.escalate` |
| kata | `kata.open\|observe\|analyse\|propose\|measure\|adopt\|propagate\|close` |
| development | `dev.open\|implement\|verify\|merge\|release\|rollback` (composes coding/git/CI actions) |
| operations | `ops.incident\|triage\|mitigate\|resolve\|postmortem`, `ops.k8s_*`, `ops.git_*` |
| tools | `tools.list\|get\|probe\|call\|publish\|retire`, `toolkits.import_openapi\|publish\|list`, `tool_bindings.create\|update\|retire`, `tool_calls.list`, `tool_authoring.draft\|validate\|test\|review` (§11.8) |
| memory | `lessons.propose\|review\|promote` |
| metrics | `metrics.flow`, `metrics.job`, `metrics.programme`, `metrics.maturity` |

Every action is an `ActionSpec` (V1 §5.5): one declaration → HTTP + MCP + OpenAPI + generated client.

### 10.5 UI additions

V1's nine destinations stand (Today, Board, Runs, Build, Design, Measure, Evals, Govern, Channels). V2 adds:

| Destination | Screens |
|---|---|
| **Organisation** | departments/jobs, job catalogue + release, authority matrix, approval policies, decision-policy refinements, laya config, job performance and metering |
| **Procedures** | capability/procedure workspace, SOP intake (single/batch), RDF/OWL and BPMN/Visio/draw.io intake, PDDL step editor, PKO editor (tenant-editable), knowledge-pack pairing, publish |
| **Authoring** | intake queue (chat + documents), asset draft catalogue, review/publish, asset dependency status |
| **Portfolio** | portfolio view (in-flight projects + programmes), programme shared config + dependency graph, backlog + WSJF, cadence (PI planning, demo, retro), capacity, metering per project |
| **Plan** | WBS, baseline vs forecast, registers, change control |
| **Improve** | kata queue, six-Gets workspace, lesson review, adoption |
| **Operations** | incidents, on-call, runbooks, postmortems, andon |

A **Tools** screen under Organisation lists toolkit versions, bindings, effect ceilings, health probes and recent
calls; the run inspector gains a tools panel (which tool version was called, with which credential alias, what it
returned). The board remains the work surface; the execution canvas remains the run surface, keeping the provenance
inspector pattern from `w2v_frontend` (`StageIOInspector`: every stage's input → output is inspectable). Metering
panels appear on Portfolio (per project and programme), Organisation (per job and department) and Measure (tenant
roll-up); the run inspector drills to step and tool call (D52). The org chart is not a new runtime.

### 10.6 Deployment and scale

- Same binary, same Postgres, same namespace, **no new port** (8930). New tables/actions only.
- Durable jobs scale by worker replicas; each run is bounded by its budget; the queue uses `FOR UPDATE SKIP LOCKED`
  (V1).
- A thousand jobs is a thousand catalogue rows and bindings, not a thousand processes. A thousand cards is one
  board with pagination and SSE topics. WIP limits and budgets are the pressure valves.
- The tool broker is in-process: no sidecar, no new port. Outbound calls use registered destination aliases, the
  storage adapter (MinIO) and the cluster's egress policy; the broker holds no model and stores no tool bodies.
- No Redis. No model hosted. The one internal agent loop (V1 S13 `agent.run`, configured by S15's `capability_agent`)
  is the only runtime; the `Executor` seam is a T0 deliverable (D61) that will make the kind extensible, and an
  external runtime remains an explicit non-goal until D61's four conditions hold.

---

## 11. The tool estate

**The claim.** A job is only as capable as what it may call. V2 makes the tool estate a first-class, versioned,
governed surface: every cluster service that offers a typed operation is a **tool**, tools are published in immutable
**toolkit** versions, a job's access is a **binding** with an effect ceiling, and every call is a **ledgered row**.
The conveyor owns the catalogue, the broker and the ledger. It does not route through `nonagenticai_mcp_server`
(D44), and it does not reach a backend by an ad-hoc HTTP call from a procedure or an agent.

### 11.1 Definitions — five things, one owner each

| Term | Meaning | Not to be confused with |
|---|---|---|
| **Tool** | one callable operation `<service>.<operation>`: closed input schema, output schema, effect class, idempotency, timeout, redaction, destination alias | a conveyor action (the conveyor's own op) |
| **Toolkit** | an immutable, versioned bundle of one service's tools + auth adapter + licence app + destination | a repo, a package |
| **Tool binding** | tenant-scoped grant of pinned toolkit versions to a job binding or a run, narrowed by effect ceiling, rate and credential alias | the release's tool grant |
| **Tool grant** | the release's operation allowlist (`op`, `variant`, `max_side_effect`, `enabled`) and per-call tool gate | the conveyor binding (tool existence + credential) |
| **Tool call** | one audited invocation: run/step, tool version, input hash, status, latency, bytes, cost | `action_invocation` |

**P1 applied: one owner, two rows.** The release's grant names the operations the job may use (inside the release
hash); the binding pins the toolkit versions and the credential alias. Both are conveyor rows; the release never
carries tool content, and the binding never widens the grant.

### 11.2 The estate — what exists today (measured 2026-11-11)

| Service (destination) | What it gives a job | Status | First job family |
|---|---|---|---|
| **coding fleet** — `coding.example.com`: gateway, analysis, containers, github, jobs, realtime, repo-jobs, security, test-runs, testing, ai | repository provisioning, dependency graph, verified writes, tests, security audit, PRs | live | delivery, verification, development projects |
| `strata_rag` | pinned-leaf retrieval (`knowledge.query`), routed RAG | live | every job |
| `strata_ingest` | files/URLs → markdown; ingestion | live | knowledge, AHTAPol acquisition |
| `webscraper` :9119 | four-backend scraping (Playwright, curl_cffi, Scrapy, httpx) → Markdown/JSON | live | research, SOP ingestion |
| `media_discovery` :8809 | federated media search, transcripts, quality scoring | live | media production |
| `scientific_processor` :8808 | meta-analysis, survival fitting, verification/trust scoring, sandboxed analysis | deployed; scaled to 0 today (measured 2026-11-11) — a binding needs a scale-up and a green probe | AHTAPol evidence |
| `lean4_prover` :8812 (mini1 host) | Lean 4 typecheck/elaborate/prove over REST | live | verification, SOP formal checks |
| `evidence_review` :8810 | protocol-locked PRISMA screening, blinded review, adjudication, inclusion export | built; not deployed | AHTAPol screening |
| `w2v-orchestrator` :6300 + stage fleet (analysis, content, slide, tts-gateway, video-production, video-analyzer, app-inspector, bdd-playwright, youtube-upload, frontend-test-runner, html-render; external asr/tts/image-gen on mini1) | deliverable runs and stage capabilities; rendered artifacts | live; **migration target — the stage fleet moves into the conveyor and is rewritten in Rust (D54)** | media production |
| `video-compiler` :8200 | ffmpeg render/compile jobs (slide/footage → video), durable `compile_jobs`, MinIO outputs | live; migration candidate — joins the conveyor workspace when rewritten in Rust (D58) | media production |
| ops — Kubernetes API, GitHub API, Flux, Prometheus/Loki/Grafana | read verbs (pods, deployments, events, logs, metrics, CI); write verbs gated | live (read) / gated (write) | operations |
| `billing_service`, `licenses` :8544 | usage and entitlement facts | live | project office, governance |
| AHTAPol `discovery` :8807 | source discovery, harvest and surveillance over guideline, literature and HTA sources | planned (spec); catalogue `draft` — binds when its slice deploys it (D56) | AHTAPol evidence |
| AHTAPol planned — 8811 research output, 8813 decision modelling, 8814 guideline registry, 8815 terminology broker | governed reports, CEA/CUA/PSA/BIA, guidelines, terminology | planned (specs) | AHTAPol |
| AHTAPol `8812` R stats sidecar | R engine for survival, EVPPI/EVSI, PSM/DES (GPL isolated) | planned (spec) | AHTAPol health economics |
| **BYO** — tenant OpenAPI toolkits + remote MCP servers | whatever the tenant is licensed for | per tenant (§11.5) | any tenant |

Notes that keep the table honest:

- **The kag-era tools are these four.** `webscraper`, `media_discovery`, `scientific_processor` and `lean4_prover`
  are the surviving kag-era tools and enter the first wave; `scientific_processor` is deployed but scaled to 0 today.
  `scientific_discovery` is not a separate service — the capability enters as the AHTAPol `discovery` :8807 draft
  (D56). `evidence_review` :8810 has no deployment in the cluster — it enters as `draft` and binds when its
  deployment lands.
- **Legacy kag is archived and not a tool.** Every kag repo — `kag`, `kag_orchestrator`, `kag_graph_service`,
  `kag_common`, `kag_clients`, `kag-frontend`, `hirag_service`, `karma_service`, `neo4j`, `n8n-nodes-kag` — is archived
  on GitHub (measured 2026-11-11), and no kag, hirag, karma or neo4j deployment exists in the cluster. The direction
  is a **Rust rewrite**, with the home decided per capability in `docs/SPEC_LEGACY_KAG.md`, the separate spec this V2
  creates; neo4j, hirag and karma are explicitly undecided there. `strata_ingest` and `strata_rag` are the live,
  shared ingest/retrieval services both platforms use, and the conveyor binds them at least to the extent the agent
  platform already does (D54).
- **External sources stay behind services.** PubMed, Europe PMC, ClinicalTrials.gov, NICE, GIN/INAHTA, EMA/FDA,
  ICD-11, SNOMED CT, ATC/DDD, RxNorm and the rest are called by the services that own their licences and rate limits,
  not by conveyor jobs directly (D42). Migration is expected, not exceptional: where a Python source must change to
  fit the conveyor's contracts it is re-architected rather than line-ported (owner, 2026-11-11).
- **Planned tools enter as `draft`.** An AHTAPol service that is specified but not live gets a catalogue entry and no
  binding; it cannot be called until its slice deploys it and its probe passes (P8).
- **`8812` is two distinct services.** `lean4_prover` is a launchd process on the mini1 host reached by a
  ServiceEntry; the AHTAPol R stats sidecar is a separate planned container reached only by 8808 and 8813 over
  internal HTTP. Same port number, different hosts and networks — never the same toolkit, and a port is not an
  identity.
- **AHTAPol's own documents overstate deployment.** They describe `evidence_review` :8810 and research output :8811 as
  "existing live"; the cluster has no deployment of either (measured 2026-11-11). The catalogue follows the cluster,
  not the document: both are `draft` until a slice deploys them and a probe passes.
- **AHTAPol's plan assumes archived kag components.** Its service documents call the kag gateway, `karma_service` :8801,
  `hirag_service` and `kag_orchestrator` :8806; every one is archived and none is deployed. Their functions are
  re-homed per `docs/SPEC_LEGACY_KAG.md` before the AHTAPol slice that needs them, and none may be bound as a tool (R19).

**Deliberately not tools.** `n8n` and `kata-n8n` (V1 replaces the flow corpus with procedures; n8n is a retired
system, not a tool), `fr` backend and `requirements_management` (replaced at cutover), Planka (`kanban`) and
`kanban-mcp` (retired with Planka), `w2v_frontend` and `portal-bff` (UI surfaces), Keycloak (the broker
authenticates; jobs do not call it), `agent` (a conversation front end and a client, not a tool backend — D33),
`care-bus-agents` and the `care-bus` fleet (customer care is the agent platform's product — D48), `inference-router`,
`modality-gateway` and laya (they are the L1 and L2 rungs, not job tools — a job never "calls a model" through a
tool; its executor does; `modality-gateway` folds into `mlx-api`, which may itself be rewritten in Rust), `provisioning`
:8546 (tenant/user/licence/inference-config control plane — onboarding stays with the website and provisioning
automation), `artifact_service` :8556 and `storage-api` :8391 (file plumbing; the conveyor uses MinIO directly,
§11.10), and the `mcp`/`mcp-executor`/`mcp-external-driver` deployments (the agent platform's gateway — outside the
path, §11.7, and permanently so, D59). **Product applications are clients, not backends:** `commonpractice` (+
coaching), `book-fulfillment`, `frappe`, `tsymeon-*`, `merqi-*`, `chat-bridge*`, `raisemytickets-site`,
`nonagentic-homepage`, `early-access-intake`. **Platform infrastructure** — `alloy-gateway`, `secrets`, PgBouncer,
the registries (`verdaccio`, `pypiserver`), mail (`maddy`, `listmonk`), Prometheus/Loki/Grafana, Istio, cert-manager
— is never a job tool; `worker-management` is legacy and the conveyor's own durable queue replaces it, so it is not
bound.

### 11.3 The broker — one call path

`tools.call` is the only way a conveyor-internal run reaches a **job backend**. The executor's own knowledge and
memory rungs are not tools: the compiled prompt's `knowledge.query` and the per-card `memory.consolidate` (§7.6) stay
where they are. A job that explicitly searches — rather than the executor grounding its prompt — uses the retrieval
toolkit like any other tool. (V1's twelve external actions — S09 Part B added eight, including
`coding_jobs.create|get`, `retrieval.query`, `artifacts.list`, `billing.summary` — are the seed of this registry and
migrate in T13: the backend operations onto toolkit operations, `artifacts.list` onto the storage adapter (§11.10).
They do not run beside it as a second path.)

1. **Resolve** the binding and the pinned tool version; missing binding or version fails closed (`409 tool_not_ready`)
   before any network call.
2. **Check effect** against the binding ceiling and the job authority (`403 tool_effect_forbidden`); check spend budget
   (`429 tool_budget_exceeded`).
3. **Validate** the input against the closed schema (`422 tool_input_invalid`) — `additionalProperties: false`, enums,
   patterns, bounds.
4. **Resolve the credential** from the sealed secret store by alias. The broker injects it; the prompt never carries it
   and the `tool_call` row never records it.
5. **Call the destination alias.** The base URL is registered and reviewed at publication; no caller-supplied URL at
   call time (SSRF rule). BYO destinations are tenant-allowlisted at registration.
6. **Normalise and redact**: the result envelope is `{status, data, checks[], next[]}` with
   `ok | precondition_failed | check_failed | pending | backend_error`; declared fields are redacted and the response
   size is capped.
7. **Record and emit**: one `tool_call` row (input hash, status, latency, bytes, cost) and one `tool.called` event. The
   call is never a card transition.

**Long operations are bounded.** Every poll has a fixed budget; on timeout the tool returns `pending` with a
`resume_token`, and the matching status tool continues from it. A timeout never fails silently.

**Writes verify themselves.** A write tool returns read-back `checks[]` and a `verdict`; a write with no verification
path cannot be published. Writes take an `idempotency_key`. These two rules are borrowed from the proven
`nonagenticai_mcp_server` tool contract and are not negotiable in the conveyor's registry.

### 11.4 Effect classes and gates

| Class | Examples | Gate |
|---|---|---|
| **read** | scrape, search, retrieve, pod status, CI status | none; audit only |
| **write** | code write, test run, PR, artifact upload, report assembly | job authority + audit; no human gate by default |
| **destructive** | k8s restart/scale/rollback, branch delete, data purge | approval + dry-run + dual control (`ops:write`) |
| **spend** | LLM generation, w2v render, TTS, paid data source | budget check + ledger |

A binding's ceiling narrows only: a job may lower its effect class at run time, never raise it. The mapping is data on
the tool version; it is not inferred from the HTTP method.

**Tool selection is not an LLM decision.** In a procedure run the step names its tool; a step that must choose among
tools uses L1 over the bound option set. In an `internal_agent` run the release's grant set bounds what the agent may
choose, and every choice is recorded; the broker never widens the grant.

### 11.5 BYO tools (D42)

A tenant may bring two kinds of tool and only two:

1. **OpenAPI toolkit** — an OpenAPI 3.1 document imported as a draft toolkit: operations, closed schemas, mappings,
   effect class per operation (default `read`), a destination base URL and a credential alias. Publication needs
   tenant-admin approval and a contract test against a recorded replay.
2. **Remote MCP server** — a streamable-HTTP MCP endpoint; the conveyor discovers its tools, binds a credential and
   exposes only the bound subset to the tenant's jobs (binding-scoped discovery).

Never: uploaded code, arbitrary container images, caller-supplied URLs at call time, or a tenant credential in a
prompt. The conveyor never executes tenant code; a tenant that needs executable logic runs its own service and
registers it. A tenant toolkit stays tenant-scoped; promoting it to the platform catalogue is a kata/development
project with a platform review, never an admin toggle.

### 11.6 Tools that generate tools (D43)

Development projects already act through the coding toolkit. Three generation paths, all governed:

1. **Spec → toolkit.** An OpenAPI document (a new service, or one being changed) is imported as draft operations,
   contract-tested, then published.
2. **Service → toolkit.** A new service is scaffolded from `template-rust-service`, deployed, and its OpenAPI is
   registered — the tool is the new service, not a wrapper around it.
3. **Missing operation → tool request.** A procedure step that needs an operation with no tool opens a tool-request
   card; a development job implements it, publishes the version, and the step is rebound to that pin.

The pipeline is a governed-asset lifecycle: `governed_asset(kind='tool_draft')` → validation (schema, effect, egress,
credential scope) → security review (a destructive tool needs a named approver) → `tools.publish` (immutable version)
→ binding. A tool cannot be published with wildcard egress, a free-form input schema, or a write effect without an
idempotency declaration. The tool that publishes a tool is itself a tool bound to the authoring job — and cannot
grant itself a wider effect ceiling.

**Tool code lives in the conveyor monorepo (D58).** A new tool is a crate or service inside the conveyor workspace,
registered from its own OpenAPI; a migrated tool joins the workspace on the day it is rewritten in Rust. An external
service stays bindable as a toolkit meanwhile. Nothing is imported from `nonagenticai_mcp_server`'s `toolkits/`
(`coding`, `fr`) — those are read as reference only (D44).

### 11.7 Relationship to `nonagenticai_mcp_server` (D44)

**Concepts borrowed (measured 2026-11-11):** the catalog shape — `mcp_services`/`mcp_sources` →
`mcp_tool_versions`/`mcp_toolkit_versions` → `mcp_bindings` with `mcp_credential_profiles` →
`mcp_tool_call_audit`; immutable version pinning; the closed Draft 2020-12 schema subset; binding-scoped MCP
discovery; the credential broker; the result envelope; write-verify. The server ships two toolkits today — `coding`
(11 operations) and `fr` (3) — and its `mcp_tool_call_audit` is the direct model for `tool_call`. **Not adopted:**
ephemeral Kata execution — BYO is declarative only, so the conveyor never runs uploaded code.

**Dependency refused.** The mcp server is a second control plane: Python, its own Postgres queue, its own catalog and
runner. The conveyor already owns actions, secrets, entitlements, audit and a durable job queue. One repo, one
schema, one queue (owner direction, 2026-11-11). The mcp server remains the **agent platform's** gateway for its own
backends; it is that platform's own roadmap, and the conveyor never routes through it. Its `coding` toolkit (repo
provisioning, verified writes, PRs) is the reference for the conveyor's coding toolkit. The `fr` toolkit retires with
fr at cutover (V1 D19).

**Two paths, permanently (D59).** There is no planned convergence: the agent platform keeps its own gateway, and the
conveyor broker is not exposed to it. "One audit path" is explicitly not a V2 goal; the conveyor's ledger covers
conveyor jobs only (§15.2 #11).

### 11.8 Data model and actions

| Table | Purpose | Notes |
|---|---|---|
| `tool` | one operation version | input/output schema, effect, idempotency, timeout, redact, destination alias, status |
| `toolkit` | a service's published bundle | immutable `key@version`, auth adapter, licence app, owner |
| `tool_binding` | job/run access | tenant-scoped, pinned toolkit versions, effect ceiling, credential alias, enabled |
| `tool_call` | the call ledger | run/step, tool version, input hash, status, latency, bytes, cost |

Reused, not created: V1 `secret` (credential storage), `governed_asset(kind='tool_draft')`, `event`, `audit_event`,
`inference_call` (for a spend tool that causes model calls). A tool call is **not** an `action_invocation` and never
transitions a card. The actions are the `tools.*` family in §10.4; each is an `ActionSpec` like every other action, so
it is served over HTTP, MCP and OpenAPI from one declaration.

### 11.9 Metrics, health and retirement

Per tool: calls, success rate, p95 latency, error classes, cost. Per job: tool-budget usage. Every published tool has
a `tools.probe` (read-only health); a probe failure unbinds the tool from new runs (`409 tool_not_ready`) while
in-flight runs finish. A tool with zero calls over one programme cycle is flagged and retired (P10). A schema-drift
signature — the backend changed under the contract — opens a kata, not a silent retry.

### 11.10 Object storage — direct MinIO, tenant-separated (D57)

The conveyor does not depend on `artifact_service` :8556 or `storage-api` :8391 for its own bytes. One storage adapter
writes and reads the platform's shared MinIO instance directly — not a private store — with tenant separation
enforced by key prefix and bucket policy: a mandatory tenant prefix (`tenant/<tenant_id>/`), project/run prefixes
beneath it, server-side encryption, a content hash per object, a size cap, and no bucket administration from a job. Credentials
are sealed broker credential profiles; a job never sees an S3 key. `artifact_service` and `storage-api` keep serving
their existing consumers; the conveyor keeps no compatibility shim by default — whether V1's `artifacts.list` callers
are migrated to the adapter or routed through a shim is settled in T13 (§15.2 #17).

---

## 12. Delivery plan — V2 slices

Each slice is proved locally (compose + real Postgres + integration tests) before it is called done; CI runs when a
slice integrates and when the repo merges. V2 slices build on V1's S-slices and do not re-open them.

| # | Slice | Gate |
|---|---|---|
| **T0** | **Model and contracts** — approve V2; the ownership matrix; `industry`/`department`/`job_catalogue`/`job_release`/`job_binding`; the decision-ladder types; the internal `Executor` contract (two live kinds, extensible `executor_kind`) | a job is created in the conveyor, a release is published, a binding pins its hash, and a run on a retired binding refuses `409 job_binding_stale`; no concept has two stores; a test-only third executor kind registers without editing the run path |
| **T1** | **Plan spine** — `wbs_node`, `project_baseline`, `project_register_item`, `project_dependency`, programme `shared_config` and cadence; project types; DoD | one project with a baseline, a change request and a refused self-approval; WSJF recomputes from inputs; the portfolio view lists the in-flight set; a dependency cycle is refused |
| **T2** | **Decision ladder** — `decision`/`decision_option`/`decision_policy`/`decision_policy_refinement`, `laya_inference_config`; laya client; policy evaluation; the generation split | a rule, a laya decision and a human decision produce the same audit shape; a tenant refinement changes a threshold without a code change; every decision pins the refinement and laya config versions; a generated draft cannot transition a card |
| **T3** | **PM job** — charter, WBS, baseline, staffing, gate routing, closure | PM runs a request end to end; cannot approve its own gate; a baseline change requires a change request |
| **T4** | **Kanban coach** — interruption taxonomy, closed remedy set, per-signature uniqueness, depth cap | an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata |
| **T5** | **Kata** — six Gets, evidence set, pre-registered prediction, candidate, measurement, adoption, propagation | a procedure change is measured on a held-out instrument; a regression is reverted; self-approval refused; an unmeasurable kata is not opened |
| **T6** | **Development project** — the self-development path on conveyor + one dependency repo | a real feature lands: lane → PR → CI → review → merge → deploy → live verify; the project cannot change its own gates; rollback tested |
| **T7** | **Operations** — incidents, runbooks, k8s read actions, the full gated write set, postmortem | an injected pod failure is triaged, mitigated by an approved write, resolved and postmortemed; a destructive action without the tenant's configured approvals is refused; restart, scale, rollback and flag-flip are callable |
| **T8** | **Organisational memory** — lessons, evidence refs, review, promotion to a knowledge source | a lesson with one evidence ref is refused; two independent refs + review promotes a source draft; a bad run never becomes doctrine |
| **T9** | **TOGAF profile** — architecture project type, phases A–H, contracts, delivery-project compliance | an architecture project passes A–F with contracts; a delivery project without its contract is refused at design; H produces a change/kata |
| **T10** | **SAFe cadence** — backlog, WSJF, PI planning, demo, inspect & adapt | PI planning materialises cards from the backlog by WSJF within WIP; a deliverable without evidence cannot demo; retro produces lessons |
| **T11** | **Metrics, metering and maturity** — flow, quality, cost, governance, C0–C5 (thresholds calibrated from real run/approval data and stored as configuration); metering roll-ups (tenant, department, programme, project, job) and drill-down (run, step, call); billing reconciliation | every metric and every metering level recomputes from rows; the reconciliation with `billing_service` is exact or opens an incident; a C0–C5 level changes only when the measured evidence crosses its recorded threshold; a gate that never refused is flagged |
| **T12** | **One organisation loop** — the acceptance of V2 | one tenant runs request → PM → delivery jobs → coach interruption → kata → development project → deploy → operations incident → lesson, all audited, within budget |
| **T13** | **Tool platform** — `tool`/`toolkit`/`tool_binding`/`tool_call`; the broker; effect classes; credential profiles; the MinIO storage adapter; the coding toolkit and one kag-era toolkit; V1's external actions migrated | a development job calls coding through the broker (not `nonagenticai_mcp_server`); a `tool_call` is recorded; a destructive call without the tenant's configured approvals is refused; schema-invalid input is refused before the backend call; a job writes and reads a tenant-prefixed MinIO object with no S3 key in the prompt |
| **T14** | **Estate toolkits** — import wave over the live services: coding fleet, strata, billing and licence reads, webscraper, media_discovery, scientific_processor, lean4_prover, video-compiler, w2v orchestrator, ops read tools; `evidence_review`, `discovery` :8807 and the planned AHTAPol services enter as `draft` | each published toolkit has a probe and one live call; `scientific_processor` is scaled up for its probe and one live call, then scaled back; a zero-use report exists; a backend outage unbinds new runs and in-flight runs finish |
| **T15** | **Tool authoring and BYO** — OpenAPI draft import, service-scaffold registration, remote-MCP binding, tenant binding, promotion path | a tool generated from a spec is published and called by a job; publication approval defaults to a human and is tenant-configurable; a BYO MCP server is bound, narrowed and audited; a tenant cannot exceed its effect ceiling |
| **T16** | **SOP, RDF/OWL and process-document ingestion** — SOP intake as a governed asset; RDF/OWL/Turtle and BPMN/Visio/draw.io conversion into PKO rows and PDDL procedure drafts; the PKO tables and tenant editing; capability/agent/job design from an SOP; the review gates | a real SOP, an OWL file and a BPMN diagram are each ingested end to end; the procedure draft is published behind a human gate; a tenant edits a PKO entity and the change is versioned; a job bound to the procedure runs the PDDL steps; an unreviewed source never binds |
| **T17** | **Tenant asset authoring** — chat + document intake producing drafts of every tenant asset (organisation, jobs, procedures, PKO, packs, policies, templates, tool requests), the review queue, publication | a tenant supplies a short brief plus documents, and a reviewed catalogue of departments, jobs, procedures and policies is published; no asset can publish itself or widen a tool grant |

Dependencies: T0 → T1 → T2 → T3; T4 after T3; T5 after T2 and V1's S16–S20; T6 after T5 **and T13**; T7 after T2/T4
**and T13**; T9/T10 after T1/T3; T8 after T5; T11 continuous; T12 after all; T13 after T2; T14 after T13; T15 after
T13 and T6; T16 after T2 and T13; T17 after T16 and T3. T9 and T10 may run in parallel once T1 and T3 exist.

**The first three months of work are T0–T3 plus T13.** They install the organisation spine and the tool spine;
everything else is a job that uses them. T16 follows the tool spine and is the first authoring feature built on it;
T17 generalises T16 to every tenant asset. The legacy kag and w2v Rust rewrites are governed by
`docs/SPEC_LEGACY_KAG.md` — a separate spec, not a V2 slice; V2 fixes only their direction (D54). A tenant that does
not use a V2 aggregate does not ship it (P10).

---

## 13. What this buys

| | V1 today | V2 target |
|---|---|---|
| Work | projects/cards/runs/approvals | the same, plus programme cadence, WBS, baselines, registers; a computed portfolio of everything in flight |
| Who does the work | a person creates a project; agents run runs | a conveyor-owned catalogue of jobs; PM/coach/kata/development/operations staff the loop |
| Organisation | none | industry → department(s) → job, immutable releases, tenant-scoped bindings |
| Procedures | capability → procedure → step | + SOP/RDF/OWL/BPMN ingestion → PKO + PDDL procedures, used to design capabilities, agents and jobs |
| Self-contained | concepts imported from other apps | **the conveyor owns its whole model**; the agent platform is a front end over MCP |
| Decisions | rules + agents + humans, ad hoc | **rule → laya → generation → human**, every decision recorded |
| LLM use | prompts and loops | **generation only**, behind a gate, measured |
| Process rigour | requirements, ADRs, capabilities | TOGAF ADM, SAFe cadence, PMP artefacts — executable |
| Improvement | evals/benchmark exist | coach → kata → dev → measure → adopt, one loop |
| Self-development | manual | development projects on conveyor + dependencies, under gates |
| Operations | alerts only | incident cards, runbooks, k8s/git actions, postmortems |
| Tools | 12 ad-hoc external actions | a governed estate: every cluster service a versioned toolkit, BYO tools, generated tools, every call ledgered |
| Metering | ledger + budgets | roll-ups at tenant, department, programme, project and job; drill-down to run, step and call; reconciled with billing |
| Authoring | SOP ingestion only | every tenant asset drafted from chat input and documents, behind review gates |
| Proof | run/approval/action records | + decision rows, lesson evidence, baseline/contract traceability |

The unglamorous win: **the organisation's work becomes auditable at the level of a single decision** — which option,
which confidence, which policy, which person, which evidence.

---

## 14. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **Over-abstraction** — the v4 lesson: four `wf_*` types "serve nothing", the generator was deleted | P10: no aggregate without a named procedure and a metric; T0–T3 only the spine; kill any table with zero use after one programme cycle |
| R2 | **Meta-job recursion** — coach cards about coach cards; 9/9 katas failed | closed triage/remedy sets, per-signature uniqueness, depth cap, evidence-before-kata, failed kata keeps the cord |
| R3 | **Reimplementation drift** — the conveyor duplicates concepts from the agent platform and they diverge | the conveyor owns its model (D33); the agent is a client with its own roadmap; concept parity is tested only where a channel contract depends on it; no runtime import |
| R4 | **LLM decision creep** — routing logic reappears in prompts | the decision-ladder types; generation result has no transition path; a code-review rule and a negative test per surface |
| R5 | **Governance theatre** — gates that cannot fail | every gate ships with a negative test; never-refused gates are flagged; evidence is required |
| R6 | **Self-development runaway** — agents change the system that judges them | §9.3: no changing own gates/instruments; merge and deploy are separate human decisions; auto-merge earned and revocable; rollback mandatory |
| R7 | **Cost/latency** — laya + LLM + evals at thousand-card scale | budgets per run/job/tenant; laya for choices; caching; the ledger makes spend visible; fail-closed on budget |
| R8 | **Release drift inside the conveyor** — a live binding points at a release that was superseded | immutable release; a release change is a new binding and a gate; a retired binding refuses `409 job_binding_stale`; a run records the release hash it used |
| R9 | **Scale of the catalogue** — thousands of jobs become unmanageable | derivation + curation; department/job search; publish in waves; metrics per job; retire by evidence |
| R10 | **Standards cosplay** — TOGAF/SAFe in name only | a lean profile with named artefacts and gates; no certification claims; the first architecture project is the test |
| R11 | **MCP contract break at cutover** | V1 R11 stands: the tool surface is versioned; S06b's tool-contract test; a job release pins the expected tool keys |
| R12 | **Operations blast radius** | read-first; namespace-scoped service account; dry-run; dual control; andon; no cluster-admin |
| R13 | **Tool sprawl** — dozens of unowned backends; a tool nobody calls | one toolkit owner per service; `tools.probe`; zero-use retirement after a programme cycle (P10); bindings reviewed with the job |
| R14 | **SSRF / credential leak** — a procedure or a tenant reaches an arbitrary URL or exfiltrates a secret | destination aliases only; sealed credential injection; redaction fields; response caps; BYO destinations reviewed at registration |
| R15 | **Tool contract drift** — a backend changes under a published schema | pinned toolkit version; scheduled probe; a drift signature unbinds and opens a kata; a changed contract is a new version, never a silent patch |
| R16 | **BYO blast radius** — a tenant tool writes or spends beyond its station | declarative-only BYO; effect ceiling per binding; tenant rate/budget limits; promotion requires platform review |
| R17 | **Authoring injection / knowledge poisoning** — an SOP, document, diagram or chat message carries instructions or false doctrine into a procedure or any tenant asset | every source is untrusted input; every derived artefact is a draft behind a human gate; provenance is recorded; no source can grant a tool, widen an effect ceiling or change a policy; the PKO is reviewed and versioned |
| R18 | **Tenant-configuration sprawl** — thousands of per-case refinements and autonomy policies diverge, and nobody can explain why a decision happened | platform defaults; every refinement and policy is versioned and gated; every decision row pins the policy, refinement and laya config versions; a per-tenant configuration report; unused refinements are retired (P10) |
| R19 | **The AHTAPol plan rests on archived kag components** — its service documents assume the kag gateway, `karma_service`, `hirag_service` and `kag_orchestrator`, all archived and undeployed | re-home each function per `SPEC_LEGACY_KAG.md` before the AHTAPol slice that needs it; no archived component may be bound as a tool; the slice gates on the re-homed service's probe, never on the old name |

---

## 15. Decisions and open questions

### 15.1 V2 decisions (D28–D61; D1–D27 of V1 stand, except D14 and D22 as amended below)

**D28–D61 are this spec's decisions.** They are written as answers because a spec that only lists questions is not
reviewable; each can be accepted, amended or rejected as a unit, and the entries carrying an owner ruling are marked
with its date. D33, D41, D43 and D45 carry the first review's owner rulings of 2026-11-11; D46–D48 record the procedure/SOP model, the SOP feature and
the scope that review produced; **D49–D55 carry the second review's owner rulings of 2026-11-11** (portfolio and
programme, tenant configuration, per-case laya config, metering, PKO, the legacy-kag direction and tenant asset
authoring); **D56–D60 carry the third review's rulings of 2026-11-11** (discovery, live-estate classification and
direct MinIO storage, the tool monorepo, the permanently separate agent tool path, and the AHTAPol/w2v tenant scope);
**D61 records the independent runtime review's recommendation** — executor seam to be built in T0, external runtime deferred —
and is proposed for acceptance with the rest.

| # | Decision | Answer |
|---|---|---|
| **D28** | Organisation model | **Industry → department(s), ordered → job** is first-class; a job is a configured position with a charter, capability binding, procedure, inference config, knowledge/memory pins, tool grants, decision options, limits and gates. Adopted from the agent platform's KH concepts (D106–D115) and **implemented by the conveyor** (D33); there is no external contract |
| **D29** | Work hierarchy | **Tenant → portfolio (computed) / programme (shared config + project dependency graph) → project → WBS/deliverable → work package (card) → run → step/attempt** (D49 refines the portfolio/programme definition); project types select phase templates, gate sets and job families; programme cadence carries the SAFe rhythm |
| **D30** | Decision ladder | **Rule → laya → generation → human.** An LLM never selects, routes, classifies or approves; a generated output is a draft and applying it is a gated action. Options and policies are rows; every L1 decision is recorded with its answer, probabilities where the type provides them, and confidence |
| **D31** | Meta-jobs | **PM, kanban coach, kata, development and operations are ordinary jobs** in the same catalogue and use the same card/run/approval primitives; no hard-coded role logic; the authority matrix is data |
| **D32** | Executable rigour | **TOGAF ADM, SAFe cadence and PMP artefacts are executable aggregates**, not documents: phases are WBS nodes, artefacts are governed assets, gates are approvals, evidence is traceable. A lean profile, not a certification claim |
| **D33** | **Ownership — the conveyor owns everything** | **The conveyor owns the whole model**: industry, department, job, job release, knowledge layers/kinds, packs, providers, laya gate questions/options, tool grants, channels, and the work/governance/measurement it already owned. **The agent platform is a user-interaction front end over MCP — a conversation surface like Telegram — with its own autonomy configuration and its own roadmap.** It is not a catalogue, an executor or a system of record for conveyor work. The conveyor reuses the agent's ideas (and where useful its code) but is self-contained (owner, 2026-11-11) |
| **D34** | Self-development | **Development projects change conveyor and its dependency repos** through lane/PR/CI/review/merge/deploy/verify; the system cannot change its own gates or instruments; merge and deploy are separate human decisions; auto-merge is earned and revocable, and its conditions are tenant configuration (D50) |
| **D35** | Operations | **Operations is a job family** acting on k8s and git: incidents as cards, runbooks as procedures, read-first actions, the full gated write set (restart, scale, rollback, flag flip) with dry-run and the tenant's configured approvals (default human dual control), postmortems feeding kata |
| **D36** | Organisational memory | **Lessons are proposed, reviewed and promoted**: ≥2 independent evidence references and a person's review before a lesson can become a knowledge source draft; execution → memory → lesson → knowledge |
| **D37** | Metrics and maturity | Flow, quality, cost and governance metrics and C0–C5 maturity are **computed read models over rows**, never stored opinions; the C0–C5 level thresholds are calibrated from real run/approval data in T11 and stored as configuration (D50), not hard-coded; a gate that never refused is flagged |
| **D38** | Job binding | A binding pins `job_key` + `release_hash` + an immutable `release_snapshot` + capability/procedure/executor; a release change is a new binding and a gate; a run against a retired binding or a release that is not its pin refuses `409 job_binding_stale` before any model call. The check is internal — no catalogue call, no caller-declared hash |
| **D39** | Scale | Thousands of jobs are catalogue rows and bindings, not processes; runs are durable jobs bounded by per-run budgets; WIP limits, leases and `uq_run_live` are the concurrency controls |
| **D40** | Lean governance | No speculative aggregate: a table exists because a named procedure uses it and a metric reads it; the T0–T3 spine ships first; an unused aggregate is removed, not defended |
| **D41** | **Tool estate ownership** | **The conveyor owns tool access in full** — `tool`/`toolkit` versions, the release's tool grants, `tool_binding`, credential profiles, the `tool_call` ledger, and the broker that executes every call. There is no agent-side grant and no second tool path (owner, 2026-11-11). The grant names ops; the binding pins versions and credentials; both are conveyor rows |
| **D42** | **BYO tools** | A tenant may bring **OpenAPI toolkits** and **remote MCP servers**; the conveyor never executes tenant code and never accepts a caller-supplied URL at call time. Tenant tools are tenant-scoped, effect-classed and rate-limited; promotion to the platform catalogue is a development project with a platform review |
| **D43** | **Tool generation** | Development projects **generate tools** through a governed authoring pipeline: OpenAPI import, service scaffold, or a tool-request card → validation → security review → publication → binding. **Approvals are tenant-configurable and default to a human approval**; a destructive tool needs a named approver. The publishing job cannot widen its own effect ceiling. The same default applies to every approval and question (owner, 2026-11-11) |
| **D44** | **`nonagenticai_mcp_server`** | **Concepts borrowed, dependency refused.** The conveyor implements its own registry, broker, credential injection and call ledger; it never routes through `nonagenticai_mcp_server` at runtime. The mcp server remains the agent platform's gateway for its own backends; it is that platform's own roadmap, and the conveyor neither uses nor audits it |
| **D45** | **Tool effects and proof** | Every tool declares one of **read / write / destructive / spend**; a binding's ceiling narrows only; writes verify themselves with read-back checks and an idempotency key; destructive calls need a dry-run and the tenant's configured approvals **in every environment, defaulting to human dual control**; spend calls need a budget; a tool with a failing probe cannot be bound to new runs. **The kata runs as its own tenant**, so its autonomy policy never inherits the work tenant's (owner, 2026-11-11) |
| **D46** | **Procedure and prompt model** | A procedure is a sequence of prompt parts: each step is an action (PDDL), a tool call or a gate. The **internal agent** executes a PDDL action with a prompt assembled at run time from four sources — knowledge (domain/procedural), episodic memory, the PDDL request, and the curated tool list. Two **live** executor kinds: `procedure` and `internal_agent`, behind one internal `Executor` contract with an extensible kind (D61, proposed); no external executor today (V1 D22 amended) |
| **D47** | **SOP ingestion** | Procedure design and management is a first-class feature: a tenant supplies SOPs (PDF, web URL, other; single or batch); the conveyor harvests and curates the domain knowledge and converts the SOP into **procedural knowledge (the Procedural Knowledge Ontology)** and a **procedure draft with PDDL steps**. The same pipeline designs capabilities, agents and jobs. It uses the web/research tools (and `lean4_prover` where formal checking helps). Every SOP-derived artefact is a draft behind a human gate; an unreviewed SOP never binds a job |
| **D48** | **Scope** | The conveyor is a **general engine for intelligent work execution** for **project, continuous-line and line-management work**. It starts with a real **IT department — development and operations** — because it must develop and operate itself, then extends to any capability/department in any industry. **It does not do customer care**: that remains the agent platform's own product. Human-in-the-loop is configurable per tenant and may be offloaded to the agent platform over MCP with the agent's own autonomy configuration |
| **D49** | **Portfolio and programme** | A **portfolio** is *all projects and programmes in flight for a tenant* — a computed read model, never a row. A **programme** is *a set of projects with shared configuration and project-level dependencies*: its `shared_config` (knowledge pins, job families, approval-policy defaults, budget, cadence) is inherited by its projects, and `project_dependency` edges (`blocks \| needs \| supersedes \| informs`) are the dependency graph. A project may stand alone (owner, 2026-11-11) |
| **D50** | **Tenant configuration over code** | Almost everything is tenant-configurable: policies, thresholds, gate conditions, approval kinds, limits and autonomy. Every such value is a versioned tenant-scoped row with a platform default; the database model, the UI and the code accommodate it, and code implements mechanisms, never policy values (P11). Auto-merge is one instance: its conditions are configuration, not a hard-coded track record (owner, 2026-11-11) |
| **D51** | **Per-case decision configuration** | Laya thresholds are configured by each tenant per decision case: a platform `decision_policy` defines the case; a tenant `decision_policy_refinement` overrides the threshold/rule, fail action and optionally the laya inference config, and `laya_inference_config` pins model, params and timeout per tenant. Every decision records the policy, refinement and config versions it used (owner, 2026-11-11) |
| **D52** | **Metering** | Metering is fine-grained and shown at every organisational level: roll-ups at tenant, department/industry, programme and project; job-level cost; drill-down to run, step and individual call. Cost is derived at read time from `inference_call`/`tool_call`; `billing_service` remains the charging system and the conveyor reconciles to it (owner, 2026-11-11) |
| **D53** | **PKO and process documents** | The **action space** (procedure steps in PDDL) and the **procedural knowledge space** (the PKO) are separate concepts. The PKO is relational, conveyor-owned and tenant-editable (`pko_release`/`pko_entity`/`pko_relation`); RDF/OWL/Turtle documents are ingested and converted into draft PKO rows, never read at runtime. BPMN 2.0, Visio and draw.io exports ingest on the same path into procedure drafts and PKO rows. Every derived artefact is a draft behind a review gate (owner, 2026-11-11) |
| **D54** | **Legacy kag and w2v** | Legacy kag is **rewritten in Rust**, not revived: every kag repo is archived and no kag/neo4j/hirag/karma deployment exists. The home of each capability — strata_ingest, strata_rag or the conveyor — is decided in `docs/SPEC_LEGACY_KAG.md`; neo4j, hirag and karma are explicitly undecided there. The w2v stage fleet **migrates into the conveyor and is rewritten in Rust**. Python sources may be re-architected rather than line-ported (owner, 2026-11-11) |
| **D55** | **Tenant asset authoring** | The platform can create **all tenant assets from tenant chat input and documents**: organisation, jobs, capabilities, procedures, PKO, knowledge packs, project templates, policies and tool requests. Every output is a draft behind a review gate; asset creation is dependency-ordered; a chat message or document can never grant a tool, widen an effect ceiling or change an approval policy. The asset matrix and first wave ship with T17 (owner, 2026-11-11) |
| **D56** | **Discovery tool** | `scientific_discovery` is not a separate service: the capability enters the catalogue as the **AHTAPol `discovery` :8807** toolkit, status `draft` until its slice deploys it and a probe passes. One name for one capability; no second discovery service is created (owner, 2026-11-11) |
| **D57** | **Live-estate classification and storage** | `video-compiler` :8200 **is a job tool**. `provisioning` :8546 stays control-plane automation used by the existing website and provisioning automation — not a conveyor tool. `modality-gateway` :8890 is generation infrastructure, folded into `mlx-api` (which may itself be rewritten in Rust) — not a job tool. `artifact_service` :8556 and `storage-api` :8391 are **not conveyor dependencies**: the conveyor writes and reads bytes directly to **MinIO** with tenant separation and shared platform use (§11.10) (owner, 2026-11-11) |
| **D58** | **Tool code in the conveyor monorepo** | The conveyor is **one large repo**: the platform plus every new or rewritten tool. A legacy Python service stays external and bindable as a toolkit until it is rewritten in Rust, at which point it joins the workspace. No tool is imported from `nonagenticai_mcp_server`; its two toolkits are reference only (owner, 2026-11-11) |
| **D59** | **Two tool paths, permanently** | The agent platform keeps its own gateway (`nonagenticai_mcp_server`) for its own backends; the conveyor broker serves conveyor jobs only. There is **no planned convergence** and the broker is not exposed to the agent. One ledger covers conveyor calls; the agent's calls are its product (owner, 2026-11-11) |
| **D60** | **AHTAPol and w2v scope** | The conveyor **covers the AHTAPol implementation and the w2v product** (owner, 2026-11-11 — "the conveyor will also cover the AHTAPol implementation"; every tool AHTAPol and w2v use is in the estate): their projects run as conveyor work and their services are estate toolkits. **AHTAPol is its own tenant, industry `health-technology-assessment`; w2v is its own tenant, industry `media`** — neither sits under `nonagentic`. The first engineering tenant remains `nonagentic`/software (owner, 2026-11-11) |
| **D61** | **Executor seam; external runtime deferred** | The conveyor keeps **one runtime**: `procedure` and `internal_agent` are two implementations of one internal `Executor` contract, and `executor_kind` is an extensible enum rather than a branch through the run path. **An external runtime is an explicit non-goal** until four conditions hold: a generic work-packet ingress returning per-step evidence the conveyor can verify (not outcome-level); governance at the boundary (a produce-only runtime, the conveyor ships); separate owners and roadmaps; and it beats the internal agent on the conveyor's own benchmark. D33/D44 stand — the agent platform is a front end, not a runtime — and D46's two live kinds stand. The product boundary is `/mcp`; only stateless infrastructure is shared (inference-router, strata, MinIO, tool backends). The seam preserves the option at near-zero cost (proposed 2026-11-11, from the independent runtime review) |

### 15.2 Open questions at review

**Resolved 2026-11-11 (owner, second review):** #1 portfolio and programme (D49); #3 laya thresholds (D51); #4
auto-merge and every gate condition (D50); #7 metering (D52); #8 legacy kag (D54); #9 external-source migration
(D54); #12 n8n (closed); #13 PKO depth (direction fixed in D53). Resolved in the first review: #2 (D33), #5
(D45/D48), #10 and #11 (the agent platform's own product, D33/D44). **Resolved 2026-11-11 (owner, third review):**
the discovery tool is the AHTAPol `discovery` :8807 draft (D56); the live-estate classification and direct MinIO
storage (D57); tool code in the conveyor monorepo (D58); the two tool paths stay separate permanently, re-closing #11
under D59; AHTAPol and w2v are conveyor-covered, each its own tenant and industry (D60). **Proposed 2026-11-11
(D61, from the independent runtime review; open until accepted):** the runtime split with the agent platform is
closed — one runtime, two live executor kinds behind an internal seam.

1. ~~Programme vs portfolio.~~ **Resolved (D49):** a **portfolio** is every project and programme in flight for a
   tenant — a computed read model, no row; a **programme** is a set of projects with shared config and project-level
   dependencies.
2. ~~Catalogue home.~~ **Resolved:** the conveyor owns the catalogue, the release and the binding (D33); there is no
   manifest contract.
3. ~~Laya model and thresholds.~~ **Resolved (D51):** thresholds are tenant configuration per decision case, with a
   tenant-scoped laya inference config; platform defaults apply where a tenant sets nothing.
4. ~~Auto-merge policy.~~ **Resolved (D50):** every gate condition — track record, path scope, reviewer requirement,
   expiry — is a tenant configuration row; the database model, the UI and the code read the policy. The default is
   off; there is no hard-coded track record.
5. ~~Ops write scope in v1.~~ **Resolved:** the full write set (restart, scale, rollback, flag flip) ships; autonomy
   is tenant-configured, defaulting to human approval (D45/D48).
6. **First job set.** The departments in §4.6 are the proposal; the first jobs and their charters are authored in
   T0/T3, not generated. The first real job remains conveyor maintenance; the first department set is IT development
   and operations (D48). *Open.*
7. ~~Metering.~~ **Resolved (D52):** fine-grained metering with roll-ups at tenant, department, programme, project
   and job, and drill-down to run, step and call; cost derived from the ledgers; reconciled with `billing_service`.
8. ~~Legacy kag.~~ **Resolved direction (D54):** a Rust rewrite; the home of each capability — including neo4j,
   hirag and karma — is decided in `docs/SPEC_LEGACY_KAG.md`; until that spec lands no legacy kag service is a
   conveyor tool.
9. ~~External source migration.~~ **Resolved direction (D54):** migrate or rewrite into the conveyor's estate;
   re-architecture is allowed where a line-port would not fit; external sources stay behind services until then.
10. ~~mcp_server migration.~~ **Closed:** the mcp server is the agent platform's own gateway; it is outside the
    conveyor's model (D44). There is no migration to plan.
11. ~~Agent-side tool calls.~~ **Closed:** the agent front end's own tool calls are its product; only calls through
    the conveyor broker enter the `tool_call` ledger (D41/D44).
12. ~~n8n legacy visibility.~~ **Closed:** n8n is reference only — a previous design that is superseded. No
    operations job reads its execution history; it is not a tool.
13. ~~Procedure ontology depth.~~ **Resolved direction (D53):** the action space (PDDL steps) and the procedural
    knowledge space (PKO) are separate; the PKO is relational, conveyor-owned and tenant-editable, fed by RDF/OWL
    ingestion. How deep the ontology goes and whether `lean4_prover` checks are mandatory for a procedure class are
    decided by the first T16 pilots.
14. **Tenant asset authoring — first wave.** D55 fixes the concept; which asset types ship first, the review depth
    per asset type, and whether a dedicated setup project type is needed are decided in T17. *Open.*
15. **w2v migration order.** D54 fixes that the stage fleet moves into the conveyor and is rewritten in Rust; which
    stage migrates first and how the live fleet and the rewrite coexist are decided in `docs/SPEC_LEGACY_KAG.md`.
    *Open.*
16. **Programme shared-config keys.** T1 authors the first key set; which keys a programme may lock and which a
    project may override is tenant policy. *Open.*
17. **`artifact_service` compatibility.** D57 removes the dependency; whether V1's `artifacts.list` callers move to
    the MinIO adapter inside T13, or a temporary shim is kept, is a T13 implementation decision. *Open.*
18. **`mlx-api` consolidation.** `modality-gateway` folds into `mlx-api`, which may be rewritten in Rust; the owning
    repo, the rewrite order and how the w2v stages call it are decided with the w2v migration (`SPEC_LEGACY_KAG.md`).
    *Open.*
19. ~~AHTAPol and w2v tenant shape.~~ **Resolved (D60):** AHTAPol is its own tenant with industry
    `health-technology-assessment`; w2v is its own tenant with industry `media`. Neither sits under `nonagentic`.
    The **first AHTAPol job set** is still authored with its implementation spec. *Job set open.*
20. **`discovery` :8807 build slice.** The toolkit is `draft`; which slice builds the service (the AHTAPol
    implementation or a conveyor tool slice), and the first job bound to it, is settled when that slice opens. *Open.*
21. **Runtime split with the agent platform.** **Proposed direction (D61, from the independent runtime review):**
    the conveyor keeps one runtime; two live executor kinds sit behind one internal contract, and an external
    runtime is deferred until D61's four conditions hold. The product boundary stays `/mcp`; only stateless
    infrastructure is shared (inference-router, strata, MinIO, tool backends). *Open until D61 is accepted; if
    rejected, decide the split here.*

---

## Appendix A — Vocabulary

| Term | Meaning | Not to be confused with |
|---|---|---|
| **Industry / department / job** | the organisational placement of a position | a tenant, a project, a card |
| **Job** | a configured position with a charter, capability binding, procedure, inference config and authority | `capability` (an ability), `capability_agent` (a conveyor-local executor config) |
| **Capability** | what the organisation can do (design, build, test, govern, operate) | a job (who does it) |
| **Procedure** | executable documentation: ordered steps, each an action + optional gate | a job (the position), a run (one attempt) |
| **Card** | the work package; the unit of assignment | a run (one execution), a conversation turn |
| **Run** | one attempt at a card through an executor | a job (the position), a card (the assignment) |
| **Executor** | the mechanism that executes a run: `procedure` or `internal_agent` behind one contract | a job (the position), a runtime backend |
| **Approval** | a decision/gate; a question is an approval | a comment, a notification |
| **Binding** | the join of a job to its immutable release, capability, procedure and executor | the release row itself |
| **Portfolio** | every project and programme in flight for a tenant — a computed view, never a row | a programme (a set of projects with shared config) |
| **Programme** | a set of projects with shared configuration and project-level dependencies; carries budget and cadence | a project (a temporary endeavour), the portfolio (a computed view) |
| **Kata** | a system-improvement job using the six Gets | a bug fix, a retry |
| **Coach** | the flow-protection job; triage + bounded remedies | a PM (plan and scope) |
| **Laya** | the System-1 calibrated choice/score/noul model | an LLM (generation) |
| **Tool** | one typed callable operation on a backend (`<service>.<operation>`) | a conveyor action, a capability |
| **Toolkit** | an immutable, versioned bundle of one service's tools | a package, a repo |
| **Tool binding** | tenant-scoped access to pinned toolkit versions with an effect ceiling | a tool grant (the release's op allowlist) |
| **Tool call** | one audited invocation through the broker | an `action_invocation` |
| **Storage adapter** | the conveyor's own MinIO client with tenant-separated prefixes and sealed credentials | `artifact_service`, `storage-api`, an S3 key in a prompt |
| **Effect class** | read / write / destructive / spend — the gate a call must pass | a role scope |
| **PDDL** | Planning Domain Definition Language — the notation a procedure step is written in | a prompt, a task description |
| **Action space** | the PDDL step: an action with preconditions and effects over typed facts, inside a procedure | the PKO (the knowledge that names and constrains it) |
| **PKO** | the Procedural Knowledge Ontology — relational entities and relations describing tasks, steps, roles, artefacts and constraints, ingested from SOPs, RDF/OWL and process diagrams | a procedure (the executable sequence), the PDDL action space |
| **Decision policy refinement** | a tenant's versioned override of a platform decision case (threshold/rule, fail action, laya config) | the platform `decision_policy` itself |
| **Authoring** | the job family that drafts tenant assets from chat input and documents | a chat assistant (it drafts; it never publishes) |
| **SOP** | a standard operating procedure a tenant supplies (PDF, URL, document) and the conveyor converts | the procedure itself, a prompt |
| **Continuous line** | standing, repeatable work (operations) as opposed to a temporary project | a project (a temporary endeavour) |

## Appendix B — V1 decisions: retained, amended, superseded

| V1 decision | V2 status |
|---|---|
| D1–D5, D7–D17 (build location, parity, scope, UI, data, cutover, procedures, API, identity, channels) | **Retained** |
| D6 (n8n later — in-app engine + `egui_xyflow` editor) | **Retained in V1, clarified by V2**: V1 S13 owns the in-app engine and the internal agent loop (`agent.run`) and re-authors the flows as procedures; V2 uses that engine as its procedure executor. The n8n product itself is reference only — a superseded design, not a tool (owner, 2026-11-11) |
| D18 (agent interface via `mcp_server`) | **Already superseded by V1's own D19** (2026-11-08); the surviving rule — a question is an approval — is carried into §6.6 |
| D19 (agent calls the conveyor's `/mcp`) | **Retained**; the agent platform is a user-interaction front end over MCP (D33) |
| D20 (capability/knowledge/memory) | **Extended** by D28 (organisation layer), D33 (ownership), D36 (lessons), D37 (metrics). One clarification: the conveyor owns the job release and its knowledge pins; there is no second copy in the agent platform |
| D21 (MCP tasks) / D22 (executor model) | **D21 retained. D22 amended by D46; D61 (proposed) adds the seam**: two live executor kinds (`procedure`, `internal_agent`) behind one internal `Executor` contract, with an extensible kind once D61 is accepted; the external agent executor is retired; an external runtime is a non-goal until D61's conditions hold; the conveyor never depends on `agent` |
| D23 (inference ledger/prompt audit) | **Retained**; D30 adds the decision ledger beside it |
| D24–D27 (benchmark, eval, hillclimb, compiled prompts) | **Retained**; they are the instruments D32/D37/P5 rely on. D27 governs every prompt the conveyor runs, including the internal agent's (§7.7) |
| D19's statement that `nonagenticai_mcp_server` remains the agent platform's gateway for every other backend | **Retained for the agent platform; amended for the conveyor** by D44: the conveyor implements its own broker and never routes through it |
| D14 (adjacent services — `artifacts` not absorbed) | **Amended by D57**: `artifact_service`/`storage-api` stay outside, but the conveyor does not call them for its own bytes — it uses MinIO directly under tenant prefixes |
| No V1 decision is superseded **by V2** | D18 was superseded by V1's own D19; D6 is clarified (V1's in-app engine is the executor; the n8n product is reference only); D14 is amended by D57; V2 adds D28–D61 and does not re-open the V1 build |

## Appendix C — What we carry from the old design, and what we delete

**Carried (concepts):** PM owns the project structure and is the only project writer after the request; per-phase and
per-artefact sign-offs; WIP limits on ready/doing; one coach card per interruption; the closed remedy set; the six
Gets; unique open kata signature; a failed kata keeps the andon cord until a person releases it; "no data, no
improvement"; measure live callers before deleting anything. One deliberate change: the old kata could not touch
**procedures** (measured 0 of 334 active flows); V2's procedures are first-class and kata may change them — the old
exclusion was a measurement of zero use, not a principle.

**Deleted (mechanism):** Planka as the state owner; 280 generated card-type × list flows; the flow generator; four
`wf_*` types; the open-ended coach LLM rung; kata pausing the whole conveyor for 30–45 minutes; duplicate card birth
(non-idempotent plumbing); 35 uncovered error exits; the Python substrate.

**Measured evidence:** E32 duplicate card birth; 35 uncovered error exits E1–E35; the kata record showed no kata
closed (the source records a moving count — 9, then 10, then 11 the same day — and warns against quoting it); the
generator deleted because "the live flow is the truth" (`n8n-designs/design/choices/v4`, `kata_v1.html`).

---

*End of SPEC V2. The V1 build record remains `docs/SPEC.md`; the slice plans remain `docs/plans/`. On approval, this
file is registered in `~/.claude/plans/program/INDEX.md`, the ownership matrix is signed, and T0 opens.*
