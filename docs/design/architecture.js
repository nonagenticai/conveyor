/* System architecture — ArchiMate 3.2 high-level views, pre-rendered Mermaid low-level
   diagrams and the UI-destination map for SPEC_V2. Local design study only: every
   record is a fixture, no backend is called, and each V2 element is a proposal. */
"use strict";
window.ArchitecturePack = (function () {
  let api = null;
  const state = { tab: "High level" };
  const TABS = [
    "High level",
    "Runtime",
    "Data & knowledge",
    "Improvement & delivery",
    "UI map",
  ];
  const E = (value) =>
    String(value).replace(
      /[&<>"']/g,
      (ch) =>
        ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        })[ch],
    );
  const badge = (label, tone = "") =>
    api ? api.badge(label, tone) : `<span class="badge ${tone}">${E(label)}</span>`;
  const button = (label, data, cls = "", symbol = "") =>
    api ? api.btn(label, "navigate", data, cls, symbol) : label;

  /* ---------------------------------------------------------------- Mermaid sources */
  const SOURCES = {
    context: `flowchart LR
  subgraph CH["Channels"]
    W["Web studio"]:::v1
    T["Telegram"]:::v1
    M["MCP / agent front end"]:::v1
  end
  subgraph CV["Conveyor - one binary - :8930"]
    API["API - ActionSpec registry"]:::v1
    DOM["Domain - work + governance"]:::v1
    WF["Workflow - durable queue"]:::v1
    EXE["Executors - procedure / internal_agent"]:::v1
    LAD["Decision ladder L0-L3"]:::v2
    KM["Knowledge and memory"]:::v1
    BRK["Tool broker"]:::v2
    INS["Instruments - evals, benchmarks, hillclimb"]:::v1
    MET["Metering read models"]:::v1
  end
  subgraph BK["Backends"]
    LAYA["laya - System-1"]:::ext
    ROUTER["inference-router - L2"]:::ext
    STRATA["strata_ingest / strata_rag"]:::ext
    TOOLS["coding, ops, w2v and AHTAPol toolkits"]:::ext
    MINIO["MinIO"]:::ext
    BILL["billing_service / licenses"]:::ext
    KC["Keycloak"]:::ext
  end
  W --> API
  T --> API
  M --> API
  API --> DOM
  DOM --> WF
  WF --> EXE
  EXE --> LAD
  EXE --> KM
  EXE --> ROUTER
  EXE --> BRK
  BRK --> TOOLS
  KM --> STRATA
  MET --> BILL
  API --> KC
  DOM --> MINIO
  classDef v1 fill:#eef3ef,stroke:#8aa596,color:#1f2a24
  classDef v2 fill:#ffffff,stroke:#8aa596,stroke-dasharray:5 3,color:#1f2a24
  classDef ext fill:#f4f1f8,stroke:#b9acce,color:#1f2a24`,
    "run-sequence": `sequenceDiagram
  autonumber
  actor P as Person or channel
  participant API as API
  participant WF as Workflow
  participant B as Job binding
  participant PC as Prompt compiler
  participant KM as Knowledge and memory
  participant L as Laya (L1)
  participant R as inference-router (L2)
  participant BR as Tool broker
  participant G as Approval gate
  participant EV as Evidence
  P->>API: start a run for a card and binding
  API->>WF: enqueue the run
  WF->>B: check the pinned release_hash
  alt stale or retired binding
    B-->>WF: 409 job_binding_stale
    WF-->>P: refused before any model call
  else pinned release
    WF->>PC: compile the step
    PC->>KM: knowledge pins and episodic memory
    KM-->>PC: selected sources and prompt_sha256
    opt procedure executor
      PC-->>WF: PDDL action
      WF->>BR: tools.call when the step names a tool
      BR-->>WF: status, data, checks, next
    end
    opt internal_agent executor
      PC-->>R: generation over the compiled prompt
      R-->>WF: draft artefact only
      WF->>L: decide the configured option set
      L-->>WF: answer and confidence
    end
    opt gate requires approval
      WF->>G: request the gate
      G-->>WF: approval and approval_decision
    end
    WF->>EV: run, step, tool_call and inference_call rows
    WF-->>P: terminal result
  end`,
    "broker-sequence": `sequenceDiagram
  autonumber
  participant EX as Executor
  participant BR as tools.call broker
  participant REG as Binding and pinned version
  participant SEC as Sealed credential store
  participant BE as Destination alias
  participant LED as tool_call ledger
  EX->>BR: service.operation and input
  BR->>REG: resolve the binding and version
  alt missing binding or version
    REG-->>BR: 409 tool_not_ready
    BR-->>EX: refused before the network call
  end
  BR->>BR: check the effect ceiling and spend budget
  alt effect or budget breach
    BR-->>EX: 403 tool_effect_forbidden or 429 tool_budget_exceeded
  end
  BR->>BR: validate the closed input schema
  alt invalid input
    BR-->>EX: 422 tool_input_invalid
  end
  BR->>SEC: resolve the credential by alias
  SEC-->>BR: credential never reaches the prompt or ledger
  BR->>BE: call the registered destination alias
  BE-->>BR: raw result
  BR->>BR: normalise and redact into status, data, checks, next
  BR->>LED: one tool_call row and one tool.called event
  LED-->>BR: recorded
  BR-->>EX: normalised envelope`,
    "run-state": `stateDiagram-v2
  [*] --> queued
  queued --> running: lease
  running --> waiting_approval: gate
  waiting_approval --> running: approved
  waiting_approval --> failed: refused
  running --> succeeded: evidence
  running --> failed: error or refusal
  running --> cancelled: cancel
  succeeded --> [*]
  failed --> [*]
  cancelled --> [*]
  note right of running: one live run per card (uq_run_live); a retry is an attempt`,
    "decision-ladder": `flowchart TD
  L0["L0 - Rule: deterministic Rust"]:::rule
  L1["L1 - Laya: calibrated choice, score or noul"]:::laya
  L2["L2 - Generation: LLM text, code, plans"]:::gen
  L3["L3 - Human: approval, override, irreversible action"]:::human
  R0[("row and action_invocation")]:::row
  R1[("decision row: answer and confidence")]:::row
  R2[("inference_call and draft artefact")]:::row
  R3[("approval and approval_decision")]:::row
  L0 --> R0
  L1 --> R1
  L2 --> R2
  L3 --> R3
  R2 -. no path to a state transition .-> X["Refused by the type system"]:::refused
  L1 --> L3
  R1 --> L3
  classDef rule fill:#eef3ef,stroke:#8aa596,color:#1f2a24
  classDef laya fill:#f4f1f8,stroke:#b9acce,color:#1f2a24
  classDef gen fill:#ffffff,stroke:#8aa596,stroke-dasharray:5 3,color:#1f2a24
  classDef human fill:#f7f4ea,stroke:#b9a77a,color:#1f2a24
  classDef row fill:#ffffff,stroke:#a9b8b0,color:#1f2a24
  classDef refused fill:#fdf1ee,stroke:#c98b7b,color:#1f2a24`,
    "er-organisation-work": `erDiagram
  TENANT ||--o{ INDUSTRY : has
  INDUSTRY ||--o{ DEPARTMENT : orders
  DEPARTMENT ||--o{ JOB_CATALOGUE : places
  JOB_CATALOGUE ||--o{ JOB_RELEASE : publishes
  JOB_RELEASE ||--o{ JOB_BINDING : pinned_by
  TENANT ||--o{ PROGRAMME : owns
  PROGRAMME ||--o{ PROJECT : groups
  PROJECT ||--o{ WBS_NODE : plans
  WBS_NODE ||--o{ CARD : contains
  CARD ||--o{ RUN : executes
  RUN ||--o{ RUN_STEP : has
  CARD ||--o{ APPROVAL : gates
  JOB_BINDING ||--o{ CARD : assigned_to
  JOB_BINDING ||--o{ RUN : used_by`,
    "er-execution-decisions": `erDiagram
  RUN ||--o{ RUN_STEP : has
  RUN ||--o{ INFERENCE_CALL : ledgers
  RUN ||--o{ TOOL_CALL : ledgers
  RUN ||--o{ DECISION : records
  DECISION }o--|| DECISION_POLICY : follows
  DECISION }o--|| DECISION_OPTION : selects
  DECISION_POLICY ||--o{ DECISION_POLICY_REFINEMENT : refined_by
  DECISION_POLICY_REFINEMENT }o--|| LAYA_INFERENCE_CONFIG : pins
  CARD ||--o{ APPROVAL : gates
  APPROVAL ||--o{ APPROVAL_DECISION : records
  RUN ||--o{ GOVERNED_ASSET : produces
  GOVERNED_ASSET ||--o{ TRACE_LINK : traced_by`,
    "er-knowledge-tools": `erDiagram
  CAPABILITY ||--o{ PROCEDURE : realises
  PROCEDURE ||--o{ PROCEDURE_STEP : orders
  PROCEDURE }o--|| PKO_RELEASE : pins
  PKO_RELEASE ||--o{ PKO_ENTITY : contains
  PKO_ENTITY ||--o{ PKO_RELATION : subject
  PROCEDURE_STEP }o--o{ PKO_ENTITY : references
  CAPABILITY }o--o{ KNOWLEDGE_PACK : pinned
  CARD ||--o{ MEMORY_HOT : writes
  CARD ||--o{ MEMORY_DIGEST : consolidates
  LESSON }o--o{ EVIDENCE : needs
  JOB_BINDING }o--o{ TOOL_BINDING : grants
  TOOLKIT ||--o{ TOOL : versions
  TOOL_BINDING }o--|| TOOLKIT : pins
  TOOL_CALL }o--|| TOOL : invokes`,
    "knowledge-pipeline": `flowchart LR
  S1["SOP - PDF, URL, document"]:::src
  S2["RDF / OWL / Turtle"]:::src
  S3["BPMN / Visio / draw.io"]:::src
  S4["Chat or document brief"]:::src
  ING["Intake - governed_asset"]:::v1
  CONV["Conversion job"]:::v2
  PKO["PKO draft - entities and relations"]:::v2
  PROC["Procedure draft - PDDL steps"]:::v2
  PACK["Knowledge pack draft - strata_rag"]:::v1
  REV["Review gate - a person"]:::v2
  PUB["Published release - pinned"]:::v2
  RUN["Run - readiness check"]:::v1
  BLOCK["409 release_not_ready"]:::bad
  S1 --> ING
  S2 --> ING
  S3 --> ING
  S4 --> ING
  ING --> CONV
  CONV --> PKO
  CONV --> PROC
  CONV --> PACK
  PKO --> REV
  PROC --> REV
  PACK --> REV
  REV --> PUB
  PUB --> RUN
  RUN --> BLOCK
  classDef src fill:#f4f1f8,stroke:#b9acce,color:#1f2a24
  classDef v1 fill:#eef3ef,stroke:#8aa596,color:#1f2a24
  classDef v2 fill:#ffffff,stroke:#8aa596,stroke-dasharray:5 3,color:#1f2a24
  classDef bad fill:#fdf1ee,stroke:#c98b7b,color:#1f2a24`,
    "memory-lifecycle": `stateDiagram-v2
  state "Hot rows - card" as Hot
  state "Cold digest - one per card" as Digest
  state "Lesson candidate" as Cand
  state "Reviewed lesson" as Reviewed
  state "Promoted knowledge" as Promoted
  [*] --> Hot: observation, claim or verdict
  Hot --> Hot: successor - original retained
  Hot --> Digest: consolidate
  Digest --> Digest: rebuild - dry-run diff
  Hot --> Cand: propose lesson
  Cand --> Reviewed: person and at least two evidence refs
  Cand --> Rejected: one ref or unsupported
  Reviewed --> Promoted: source or pack release
  Promoted --> [*]
  Rejected --> [*]
  note right of Hot: hot wins on conflict; rows are not rebuildable
  note right of Digest: the digest is rebuildable; omissions stay visible`,
    "improvement-loop": `flowchart LR
  A["Alert, failed run, eval delta or prompt finding"]:::src
  C["Coach card - one interruption, closed remedy set"]:::v2
  K["Kata - six Gets, instrument and threshold"]:::v2
  D["Development project or procedure, prompt, knowledge or job change"]:::v2
  M["Measurement - held-out eval, benchmark or run outcome"]:::v1
  AD["Adoption gate - person, dual control"]:::v2
  P["Production - new procedure, job or binding version"]:::v2
  L["Monitoring and lesson"]:::v1
  X["Unmeasurable kata is not opened"]:::bad
  A --> C
  C --> K
  K --> D
  D --> M
  M --> AD
  AD --> P
  P --> L
  K -. refused .-> X
  classDef src fill:#f4f1f8,stroke:#b9acce,color:#1f2a24
  classDef v1 fill:#eef3ef,stroke:#8aa596,color:#1f2a24
  classDef v2 fill:#ffffff,stroke:#8aa596,stroke-dasharray:5 3,color:#1f2a24
  classDef bad fill:#fdf1ee,stroke:#c98b7b,color:#1f2a24`,
    "slice-dag": `flowchart LR
  T0["T0 - Model and contracts"]:::slice
  T1["T1 - Plan spine"]:::slice
  T2["T2 - Decision ladder"]:::slice
  T3["T3 - PM job"]:::slice
  T4["T4 - Coach"]:::slice
  T5["T5 - Kata"]:::slice
  T6["T6 - Development"]:::slice
  T7["T7 - Operations"]:::slice
  T8["T8 - Organisational memory"]:::slice
  T9["T9 - TOGAF profile"]:::slice
  T10["T10 - SAFe cadence"]:::slice
  T11["T11 - Metrics and metering"]:::slice
  T12["T12 - One organisation loop"]:::final
  T13["T13 - Tool platform"]:::slice
  T14["T14 - Estate toolkits"]:::slice
  T15["T15 - Tool authoring and BYO"]:::slice
  T16["T16 - SOP, RDF and process ingestion"]:::slice
  T17["T17 - Tenant asset authoring"]:::slice
  T0 --> T1
  T1 --> T2
  T2 --> T3
  T3 --> T4
  T2 --> T5
  T5 --> T6
  T2 --> T13
  T13 --> T14
  T13 --> T15
  T6 --> T15
  T2 --> T16
  T13 --> T16
  T16 --> T17
  T3 --> T17
  T1 --> T9
  T1 --> T10
  T3 --> T9
  T3 --> T10
  T5 --> T8
  T13 --> T6
  T13 --> T7
  T4 --> T7
  T11 --> T12
  T3 --> T12
  T7 --> T12
  T17 --> T12
  classDef slice fill:#eef3ef,stroke:#8aa596,color:#1f2a24
  classDef final fill:#f7f4ea,stroke:#b9a77a,color:#1f2a24`,
    "ui-map": `flowchart LR
  subgraph ARCH["SPEC_V2 architecture"]
    ORG["Organisation spine"]:::v2
    PLAN["Portfolio, programme and plan"]:::v2
    RUN["Execution, run and gate"]:::v1
    DEC["Decision ladder"]:::v2
    KM["Knowledge and memory"]:::v1
    TOOL["Tool estate and broker"]:::v2
    IMP["Improve, coach and kata"]:::v2
    OPS["Operations"]:::v2
    MET["Metering and instruments"]:::v1
    AUTH["Identity, licences and channels"]:::v1
  end
  subgraph UI["Studio destinations"]
    U1["Programme control and Projects"]:::ui
    U2["Portfolio and Plan and WBS"]:::ui
    U3["Execution room, Runs and Decision inbox"]:::ui
    U4["Organisation, Jobs and Agents"]:::ui
    U5["Knowledge design, Sources and Memory"]:::ui
    U6["Tools, Procedures and Authoring"]:::ui
    U7["Improve, Evals, Benchmarks and Hillclimb"]:::ui
    U8["Operations and Assurance"]:::ui
    U9["Metering and Usage and audit"]:::ui
    U10["Requirements, Principles, Models and Types"]:::ui
  end
  ORG --> U4
  ORG --> U10
  PLAN --> U1
  PLAN --> U2
  RUN --> U3
  DEC --> U3
  KM --> U5
  TOOL --> U6
  IMP --> U7
  OPS --> U8
  MET --> U9
  AUTH --> U10
  classDef v1 fill:#eef3ef,stroke:#8aa596,color:#1f2a24
  classDef v2 fill:#ffffff,stroke:#8aa596,stroke-dasharray:5 3,color:#1f2a24
  classDef ui fill:#f4f1f8,stroke:#b9acce,color:#1f2a24`,
  };

  const DIAGRAMS = {
    context: {
      title: "Component and context",
      spec: "§10.1, §10.6",
      caption:
        "One binary on :8930. Channels call the API; the API, domain and workflow stay internal; every backend is reached through a typed rung (router, strata, broker) — never by an ad-hoc call.",
    },
    "run-sequence": {
      title: "Run execution — binding, compile, decide, gate",
      spec: "§4.4, §4.5, §7.7",
      caption:
        "A stale or retired binding refuses 409 before any model call. The internal agent receives the four sources; its output is a draft, and the gate is the only route to a transition.",
    },
    "broker-sequence": {
      title: "Tool broker — one call path",
      spec: "§11.3, §11.4",
      caption:
        "Resolve, check effect and spend, validate the closed schema, inject the sealed credential, call the registered alias, normalise and redact, record one tool_call. Every refusal happens before or instead of a backend call.",
    },
    "run-state": {
      title: "Run state machine",
      spec: "§5.1, §6.6",
      caption:
        "One live run per card; a retry is an attempt on the same run where the state machine permits, otherwise a new run linked to the previous one.",
    },
    "decision-ladder": {
      title: "Decision ladder — rule, laya, generation, human",
      spec: "§7.1–§7.4, D30",
      caption:
        "The hard rule: L2 has no path to a state transition. Generated text is a draft artefact; applying it is an action whose gate is L1 or L3.",
    },
    "er-organisation-work": {
      title: "Organisation and work spine",
      spec: "§4.1–§4.5, §5.1, §10.3",
      caption:
        "The job release is immutable and pinned by a binding; the binding is tenant-scoped. The portfolio is a computed read model, never a row.",
      tall: true,
    },
    "er-execution-decisions": {
      title: "Execution, decisions and evidence",
      spec: "§7.1–§7.5, §10.3",
      caption:
        "Every run records the binding, the decisions it made, the calls it made and the assets it produced. A decision pins the policy, refinement and laya config versions it used.",
      tall: true,
    },
    "er-knowledge-tools": {
      title: "Knowledge, memory and the tool estate",
      spec: "§7.6, §11.1, §11.8, §10.3",
      caption:
        "Procedural knowledge (procedure + PKO) and domain knowledge (packs) are pinned per job. Hot memory lives in the conveyor; one digest per card lives in strata_rag. Tools are versioned and bound; every call is ledgered.",
      tall: true,
    },
    "knowledge-pipeline": {
      title: "Knowledge pipeline — source to pinned release",
      spec: "§5.6, §5.7, D47, D53",
      caption:
        "SOPs, RDF/OWL/Turtle, process documents and chat are untrusted input. Conversion drafts PKO rows, PDDL steps and a knowledge pack behind a human review gate; an unreviewed source never binds.",
    },
    "memory-lifecycle": {
      title: "Memory and lesson lifecycle",
      spec: "§7.6, D36",
      caption:
        "Hot rows win on conflict and are not rebuildable; the cold digest is rebuildable with a dry-run diff. A lesson needs a person and at least two independent evidence references before it can become a knowledge source.",
    },
    "improvement-loop": {
      title: "One improvement loop",
      spec: "§9.4, D34",
      caption:
        "Alert, failed run, eval delta, prompt finding or coach escalation → coach → kata → development or configuration change → held-out measurement → adoption → production → lesson. Not four loops.",
    },
    "slice-dag": {
      title: "Delivery slices T0–T17",
      spec: "§12",
      caption:
        "T0–T3 plus T13 are the first three months: the organisation spine, the decision ladder and the tool spine. Everything else is a job that uses them.",
    },
    "ui-map": {
      title: "Architecture → studio destinations",
      spec: "§10.5, STUDIO-SPEC destination map",
      caption:
        "Each destination is a view over one or more aggregates; V1 surfaces carry no ribbon, V2 previews never claim a write path. The table below covers every sidebar destination.",
      tall: true,
    },
  };

  /* ---------------------------------------------------------------- UI map fixtures */
  const UI_ROWS = [
    ["Programme control", "overview", "Portfolio read model · programme cadence", "§5.1, §8.2", "R"],
    ["Portfolio", "portfolio", "Computed portfolio · programme shared config · dependencies · WSJF · metering", "§5.1, §5.5, §8.2, D49", "P"],
    ["Projects", "projects", "Project · WBS node · work package (card)", "§5.1–§5.5", "R"],
    ["Contribute", "contribute", "GitHub issue registry · ready units · claim protocol · feedback", "§10.5, 03-DELIVERY", "R"],
    ["Plan & WBS", "plan", "wbs_node · project_baseline · register · change control", "§5.3–§5.5", "P"],
    ["Execution room", "execution", "Run · prompt compiler · tool panel · explain context", "§7.7, §10.5", "R + P"],
    ["Decision inbox", "decisions", "Approval · gate · dual control · decided_by_kind", "§5.4", "R"],
    ["Run history", "runs", "Run · step · attempt · two executor kinds", "§4.5, §5.1", "R"],
    ["Organisation", "organisation", "Authority matrix · approval policies · decision ladder · laya config", "§6, §7, D50", "P"],
    ["Jobs", "jobs", "job_catalogue · job_release · job_binding", "§4.1–§4.5, D38", "P"],
    ["Agents", "agents", "capability_agent · internal_agent executor", "§4.5, D46", "R + P"],
    ["Tools", "toolkits", "tool · toolkit · tool_binding · tool_call · broker", "§11.1–§11.9, D41", "P"],
    ["Project types", "types", "Project type template · gates · WBS kinds", "§5.2", "R + P"],
    ["Procedures", "procedures", "capability · procedure · step · SOP · PKO · publish", "§5.6, D47, D53", "P"],
    ["Capabilities", "capabilities", "Capability · procedure binding", "§7.6", "R"],
    ["Authoring", "authoring", "Tenant asset intake · drafts · review · publish", "§5.7, D55", "P"],
    ["Assurance & baselines", "assurance", "Trace chain · ADM phases · baseline · closure", "§8", "R"],
    ["Flow & constraints", "constraints", "TOC study — optional, not the V2 method", "§8 note", "L"],
    ["Requirements", "requirements", "governed_asset · trace_link", "§8.4", "R"],
    ["Principles", "principles", "governed_asset · architecture principles", "§8.1", "R"],
    ["Architecture & UML", "models", "ArchiMate/UML model study", "§8.1, §10.5", "R"],
    ["System architecture", "architecture", "This page: ArchiMate views, Mermaid design, UI map", "§3–§11", "design study"],
    ["Documents", "documents", "Tracked spec · plans · evidence · decisions rendered as HTML", "§10.5, docs/", "R"],
    ["Knowledge design", "knowledge-design", "Effective configuration · inheritance · review queue", "§7.6, workbench", "R + P"],
    ["Sources & retrieval", "knowledge", "Source transformation · seven states · packs · retrieval", "§7.6, workbench", "R + P"],
    ["Memory explorer", "memory", "Hot/cold memory · record taxonomy · lessons", "§7.6, workbench", "R + P"],
    ["Improve", "improve", "Coach · kata six Gets · lessons · adoption", "§6.2–§6.3, §9.4", "P"],
    ["Evaluation studio", "evals", "Eval cases, runs, report, sign-off, climb", "§7.5, S17–S21", "R"],
    ["Hillclimb lab", "hillclimb", "Eval hillclimb", "§7.5, S20", "R"],
    ["Benchmark harness", "benchmarks", "Benchmark suite, run and comparison", "§7.5, S18", "R"],
    ["Inference & prompt audit", "usage", "Inference ledger · prompt audit", "§7.5, S21", "R"],
    ["Metering", "metering", "Roll-ups · attribution · C0–C5 · billing reconciliation", "§7.5, D52", "P"],
    ["Operations", "operations", "Incidents · runbooks · postmortems · andon", "§6.5, §9.2", "P"],
    ["Agent job types", "jobtypes", "Historical 92-entry catalogue (reference)", "—", "L"],
    ["Coding action catalogue", "tools", "367 coding operations — reference for the toolkit", "§11.3", "L"],
  ];
  const ADMIN_ROWS = [
    ["Providers & credentials", "settings/providers", "secrets.list/create/rotate/delete · write-only values · credential profiles", "§11.10, tenant administration"],
    ["Integrations", "settings/integrations", "channels.bind/unbind · MCP · Telegram · Keycloak · schedules · webhooks", "§10.2, tenant administration"],
    ["Licences & billing", "settings/licences", "licences.get · billing document · service × user matrix preview", "§7.5, D52, tenant administration"],
    ["Model tiers", "measure/model-tiers", "default/small/medium/high · provider, model, write-only key · posture", "tenant administration, future"],
  ];

  /* ---------------------------------------------------------------- ArchiMate views */
  const VIEWS = [
    {
      id: "org",
      title: "Organisation and work — motivation and business layers",
      spec: "§3–§7",
      width: 1240,
      height: 920,
      bands: [
        { x: 20, y: 16, w: 1200, h: 118, label: "MOTIVATION", cls: "motivation" },
        { x: 20, y: 150, w: 1200, h: 750, label: "BUSINESS", cls: "business" },
      ],
      elements: [
        { id: "principles", kind: "constraint", name: "Principles P1–P11", sub: "one record per fact · evidence before adoption", x: 40, y: 36, w: 330, h: 78, status: "V2" },
        { id: "outcome", kind: "outcome", name: "Auditable organisation", sub: "every decision, handoff and claim is a row", x: 410, y: 36, w: 340, h: 78, status: "V2" },
        { id: "rigour", kind: "requirement", name: "Executable rigour", sub: "TOGAF · SAFe · PMP as rows and gates", x: 790, y: 36, w: 400, h: 78, status: "V2" },
        { id: "tenant", kind: "business_actor", name: "Tenant admin", sub: "provisions, reviews, licences", x: 40, y: 182, w: 210, h: 56, status: "V1" },
        { id: "human", kind: "business_actor", name: "Human approver", sub: "the L3 gate", x: 40, y: 250, w: 210, h: 56, status: "V1" },
        { id: "pm", kind: "business_role", name: "PM", sub: "charter · WBS · baseline · close", x: 40, y: 332, w: 210, h: 56, status: "V2" },
        { id: "coach", kind: "business_role", name: "Kanban coach", sub: "one interruption, closed remedies", x: 40, y: 400, w: 210, h: 56, status: "V2" },
        { id: "kata", kind: "business_role", name: "Kata facilitator", sub: "six Gets · measured change", x: 40, y: 468, w: 210, h: 56, status: "V2" },
        { id: "delivery", kind: "business_role", name: "Delivery job", sub: "deliver, verify, release", x: 40, y: 536, w: 210, h: 56, status: "V2" },
        { id: "opsjob", kind: "business_role", name: "Operations job", sub: "detect, mitigate, postmortem", x: 40, y: 604, w: 210, h: 56, status: "V2" },
        { id: "reviewer", kind: "business_role", name: "Reviewer job", sub: "independent review", x: 40, y: 672, w: 210, h: 56, status: "V2" },
        { id: "project", kind: "business_process", name: "Project lifecycle", sub: "request → charter → WBS → gate → close", x: 310, y: 182, w: 300, h: 64, status: "V2" },
        { id: "executionLoop", kind: "business_process", name: "Execution loop", sub: "card → run → step → evidence", x: 310, y: 270, w: 300, h: 64, status: "V1" },
        { id: "improvement", kind: "business_process", name: "Improvement loop", sub: "coach → kata → develop → adopt", x: 310, y: 358, w: 300, h: 64, status: "V2" },
        { id: "opsloop", kind: "business_process", name: "Operations loop", sub: "detect → triage → mitigate → resolve", x: 310, y: 446, w: 300, h: 64, status: "V2" },
        { id: "gates", kind: "business_process", name: "Governance gates", sub: "approval · dual control · evidence", x: 310, y: 534, w: 300, h: 64, status: "V2" },
        { id: "portfolioService", kind: "business_service", name: "Portfolio & plan", sub: "in-flight set · WSJF · cadence", x: 670, y: 182, w: 250, h: 64, status: "V2" },
        { id: "workService", kind: "business_service", name: "Work execution", sub: "one card, one owner, one run", x: 670, y: 270, w: 250, h: 64, status: "V1" },
        { id: "measureService", kind: "business_service", name: "Measurement", sub: "evals · benchmarks · metering", x: 670, y: 358, w: 250, h: 64, status: "V1" },
        { id: "governanceService", kind: "business_service", name: "Governance", sub: "policies · authority · trace links", x: 670, y: 446, w: 250, h: 64, status: "V2" },
        { id: "jobRelease", kind: "business_object", name: "Job release", sub: "immutable · content hash", x: 980, y: 182, w: 240, h: 58, status: "V2" },
        { id: "jobBinding", kind: "business_object", name: "Job binding", sub: "tenant-scoped · pins release_hash", x: 980, y: 726, w: 240, h: 58, status: "V2" },
        { id: "card", kind: "business_object", name: "Card / work package", sub: "one assignment, one owner", x: 980, y: 250, w: 240, h: 58, status: "V1" },
        { id: "approval", kind: "business_object", name: "Approval", sub: "one human decision primitive", x: 980, y: 318, w: 240, h: 58, status: "V1" },
        { id: "run", kind: "business_object", name: "Run / step", sub: "one attempt, pinned binding", x: 980, y: 386, w: 240, h: 58, status: "V1" },
        { id: "evidence", kind: "business_object", name: "Evidence / artefact", sub: "traceable to the run", x: 980, y: 454, w: 240, h: 58, status: "V1" },
        { id: "memory", kind: "business_object", name: "Memory hot / digest", sub: "observations, claims, verdicts", x: 980, y: 522, w: 240, h: 58, status: "V1" },
        { id: "pack", kind: "business_object", name: "Knowledge pack", sub: "pinned key@version", x: 980, y: 590, w: 240, h: 58, status: "V1" },
        { id: "lesson", kind: "business_object", name: "Lesson", sub: "proposed → reviewed → promoted", x: 980, y: 658, w: 240, h: 58, status: "V2" },
      ],
      relations: [
        { from: "pm", to: "project", kind: "assignment" },
        { from: "coach", to: "improvement", kind: "assignment" },
        { from: "kata", to: "improvement", kind: "assignment" },
        { from: "delivery", to: "executionLoop", kind: "assignment" },
        { from: "opsjob", to: "opsloop", kind: "assignment" },
        { from: "reviewer", to: "gates", kind: "assignment" },
        { from: "human", to: "gates", kind: "assignment" },
        { from: "project", to: "portfolioService", kind: "realization" },
        { from: "executionLoop", to: "workService", kind: "realization" },
        { from: "improvement", to: "measureService", kind: "realization" },
        { from: "opsloop", to: "governanceService", kind: "realization" },
        { from: "gates", to: "governanceService", kind: "realization" },
        { from: "project", to: "jobRelease", kind: "access" },
        { from: "jobRelease", to: "jobBinding", kind: "realization" },
        { from: "jobBinding", to: "card", kind: "assignment" },
        { from: "executionLoop", to: "card", kind: "access" },
        { from: "executionLoop", to: "run", kind: "access" },
        { from: "gates", to: "approval", kind: "access" },
        { from: "measureService", to: "evidence", kind: "access" },
        { from: "executionLoop", to: "memory", kind: "access" },
        { from: "improvement", to: "lesson", kind: "access" },
        { from: "executionLoop", to: "pack", kind: "access" },
        { from: "workService", to: "outcome", kind: "realization" },
        { from: "principles", to: "project", kind: "influence" },
        { from: "principles", to: "executionLoop", kind: "influence" },
        { from: "rigour", to: "portfolioService", kind: "influence" },
      ],
    },
    {
      id: "app",
      title: "Platform and integrations — application layer",
      spec: "§10.1–§10.2, §11.7",
      width: 1240,
      height: 760,
      bands: [
        { x: 20, y: 16, w: 220, h: 728, label: "CHANNELS", cls: "application" },
        { x: 260, y: 16, w: 560, h: 728, label: "APPLICATION — CONVEYOR", cls: "application" },
        { x: 840, y: 16, w: 380, h: 728, label: "APPLICATION SERVICES — BACKENDS", cls: "application" },
      ],
      elements: [
        { id: "web", kind: "application_component", name: "Web studio", sub: "cards, runs, gates", x: 40, y: 66, w: 180, h: 58, status: "V1" },
        { id: "telegram", kind: "application_component", name: "Telegram", sub: "channel, not an executor", x: 40, y: 142, w: 180, h: 58, status: "V1" },
        { id: "mcp", kind: "application_component", name: "MCP / agent", sub: "front end, own autonomy", x: 40, y: 218, w: 180, h: 58, status: "V1" },
        { id: "agentNote", kind: "constraint", name: "Agent = channel", sub: "no reverse call; no executor", x: 40, y: 320, w: 180, h: 64, status: "V2" },
        { id: "api", kind: "application_component", name: "API · ActionSpec", sub: "HTTP · MCP · OpenAPI from one declaration", x: 280, y: 66, w: 240, h: 58, status: "V1" },
        { id: "domain", kind: "application_component", name: "Domain", sub: "work + governance state machines", x: 280, y: 142, w: 240, h: 58, status: "V1" },
        { id: "workflow", kind: "application_component", name: "Workflow", sub: "durable queue · leases · attempts", x: 280, y: 218, w: 240, h: 58, status: "V1" },
        { id: "executors", kind: "application_component", name: "Executors", sub: "procedure / internal_agent", x: 280, y: 294, w: 240, h: 58, status: "V1" },
        { id: "auth", kind: "application_component", name: "Auth · licences", sub: "tenant · role · entitlement", x: 280, y: 370, w: 240, h: 58, status: "V1" },
        { id: "metering", kind: "application_component", name: "Metering", sub: "read-time derivation from ledgers", x: 280, y: 446, w: 240, h: 58, status: "V1" },
        { id: "ladder", kind: "application_component", name: "Decision ladder", sub: "L0 rule · L1 laya · L2 gen · L3 human", x: 540, y: 66, w: 240, h: 58, status: "V2" },
        { id: "knowledge", kind: "application_component", name: "Knowledge & memory", sub: "pins · hot rows · digest", x: 540, y: 142, w: 240, h: 58, status: "V1" },
        { id: "broker", kind: "application_component", name: "Tool broker", sub: "one call path · effect ceilings", x: 540, y: 218, w: 240, h: 58, status: "V2" },
        { id: "instruments", kind: "application_component", name: "Instruments", sub: "evals · benchmarks · hillclimb", x: 540, y: 294, w: 240, h: 58, status: "V1" },
        { id: "storage", kind: "application_component", name: "Storage adapter", sub: "MinIO · tenant prefix", x: 540, y: 370, w: 240, h: 58, status: "V1" },
        { id: "events", kind: "application_component", name: "Events · audit", sub: "one event per action", x: 540, y: 446, w: 240, h: 58, status: "V1" },
        { id: "laya", kind: "application_service", name: "laya", sub: "System-1 · /v1/systemone", x: 860, y: 66, w: 160, h: 58, status: "V2" },
        { id: "router", kind: "application_service", name: "inference-router", sub: "L2 generation · ledger", x: 860, y: 138, w: 160, h: 58, status: "V1" },
        { id: "strata", kind: "application_service", name: "strata_ingest / strata_rag", sub: "pinned-leaf retrieval", x: 860, y: 210, w: 160, h: 58, status: "V1" },
        { id: "toolkits", kind: "application_service", name: "Toolkits", sub: "coding · ops · w2v · AHTAPol", x: 860, y: 282, w: 160, h: 58, status: "V2" },
        { id: "minio", kind: "application_service", name: "MinIO", sub: "tenant-separated bytes", x: 860, y: 354, w: 160, h: 58, status: "V1" },
        { id: "billing", kind: "application_service", name: "billing_service / licenses", sub: "charging + entitlement", x: 860, y: 426, w: 160, h: 58, status: "V1" },
        { id: "keycloak", kind: "application_service", name: "Keycloak", sub: "identity", x: 860, y: 498, w: 160, h: 58, status: "V1" },
      ],
      relations: [
        { from: "web", to: "api", kind: "serving" },
        { from: "telegram", to: "api", kind: "serving" },
        { from: "mcp", to: "api", kind: "serving" },
        { from: "api", to: "domain", kind: "serving" },
        { from: "domain", to: "workflow", kind: "serving" },
        { from: "workflow", to: "executors", kind: "serving" },
        { from: "executors", to: "ladder", kind: "serving" },
        { from: "executors", to: "knowledge", kind: "access" },
        { from: "executors", to: "router", kind: "serving" },
        { from: "executors", to: "broker", kind: "serving" },
        { from: "broker", to: "toolkits", kind: "serving" },
        { from: "knowledge", to: "strata", kind: "flow" },
        { from: "api", to: "auth", kind: "serving" },
        { from: "auth", to: "keycloak", kind: "flow" },
        { from: "metering", to: "billing", kind: "flow" },
        { from: "domain", to: "storage", kind: "serving" },
        { from: "storage", to: "minio", kind: "flow" },
        { from: "instruments", to: "metering", kind: "flow" },
        { from: "executors", to: "events", kind: "flow" },
        { from: "ladder", to: "laya", kind: "flow" },
      ],
    },
    {
      id: "tech",
      title: "Deployment — technology layer",
      spec: "§10.6, D57, D58",
      width: 1240,
      height: 760,
      bands: [
        { x: 20, y: 16, w: 1200, h: 136, label: "EDGE", cls: "technology" },
        { x: 20, y: 168, w: 1200, h: 356, label: "KUBERNETES — STRATO", cls: "technology" },
        { x: 20, y: 540, w: 1200, h: 200, label: "DATA AND EXTERNAL SERVICES", cls: "technology" },
      ],
      elements: [
        { id: "ingress", kind: "technology_node", name: "Istio ingress", sub: "conveyor.nonagentic.ai", x: 60, y: 44, w: 260, h: 76, status: "V1" },
        { id: "tls", kind: "technology_node", name: "TLS · DNS", sub: "cert-manager · Cloudflare", x: 360, y: 44, w: 220, h: 76, status: "V1" },
        { id: "kc", kind: "technology_node", name: "Keycloak", sub: "OIDC · tenant realm", x: 620, y: 44, w: 220, h: 76, status: "V1" },
        { id: "service", kind: "technology_node", name: "conveyor Service :8930", sub: "one port, no sidecar", x: 60, y: 210, w: 260, h: 76, status: "V1" },
        { id: "pods", kind: "technology_node", name: "conveyor pods · 3", sub: "one binary · liveness/readiness", x: 380, y: 210, w: 280, h: 76, status: "V1" },
        { id: "artifact", kind: "artifact", name: "image · sha-<sha>", sub: "promoted to :latest", x: 720, y: 210, w: 300, h: 76, status: "V1" },
        { id: "flux", kind: "technology_node", name: "Flux + registry", sub: "image digest → deployment", x: 60, y: 342, w: 240, h: 70, status: "V1" },
        { id: "postgres", kind: "system_software", name: "Postgres 18", sub: "primary · 83+ tables", x: 380, y: 342, w: 240, h: 70, status: "V1" },
        { id: "minio", kind: "system_software", name: "MinIO", sub: "tenant/<id>/ prefix · SSE", x: 700, y: 342, w: 260, h: 70, status: "V1" },
        { id: "constraint", kind: "constraint", name: "No Redis · no new port · no external executor", sub: "the tool broker is in-process", x: 60, y: 448, w: 320, h: 64, status: "V1" },
        { id: "laya", kind: "technology_node", name: "laya on pgx :9210", sub: "System-1 bridge", x: 60, y: 576, w: 240, h: 64, status: "V2" },
        { id: "router", kind: "technology_node", name: "inference-router", sub: "L2 models", x: 330, y: 576, w: 220, h: 64, status: "V1" },
        { id: "strata", kind: "technology_node", name: "strata_ingest / strata_rag", sub: "knowledge bytes and retrieval", x: 580, y: 576, w: 280, h: 64, status: "V1" },
        { id: "tools", kind: "technology_node", name: "coding · ops · w2v · AHTAPol", sub: "registered destination aliases", x: 890, y: 576, w: 310, h: 64, status: "V2" },
      ],
      relations: [
        { from: "ingress", to: "service", kind: "serving" },
        { from: "tls", to: "ingress", kind: "serving" },
        { from: "kc", to: "pods", kind: "flow" },
        { from: "service", to: "pods", kind: "serving" },
        { from: "artifact", to: "pods", kind: "assignment" },
        { from: "flux", to: "pods", kind: "assignment" },
        { from: "pods", to: "postgres", kind: "access" },
        { from: "pods", to: "minio", kind: "access" },
        { from: "pods", to: "laya", kind: "serving" },
        { from: "pods", to: "router", kind: "serving" },
        { from: "pods", to: "strata", kind: "serving" },
        { from: "pods", to: "tools", kind: "serving" },
      ],
    },
  ];

  /* ---------------------------------------------------------------- helpers */
  const lines = (text, max) => {
    const words = String(text).split(/\s+/);
    const out = [];
    let line = "";
    for (const word of words) {
      const next = (line ? line + " " : "") + word;
      if (next.length <= max || !line) line = next;
      else {
        out.push(line);
        line = word;
      }
    }
    if (line) out.push(line);
    return out.slice(0, 3);
  };

  const layerClass = (kind) => {
    if (["constraint", "outcome", "requirement", "principle"].includes(kind)) return "motivation";
    if (kind.startsWith("business")) return "business";
    if (kind === "technology_node" || kind === "system_software" || kind === "artifact") return "technology";
    return "application";
  };

  const glyph = (kind, x, y) => {
    const s = 'class="arch-glyph"';
    switch (kind) {
      case "business_actor":
        return `<circle ${s} cx="${x + 11}" cy="${y + 9}" r="4"/><path ${s} d="M${x + 11} ${y + 13}v8M${x + 11} ${y + 16}h-6M${x + 11} ${y + 16}h6M${x + 11} ${y + 21}l-4 5M${x + 11} ${y + 21}l4 5"/>`;
      case "business_role":
        return `<rect ${s} x="${x + 5}" y="${y + 4}" width="12" height="14" rx="6"/><circle ${s} cx="${x + 11}" cy="${y + 9}" r="3"/>`;
      case "business_process":
        return `<rect ${s} x="${x + 4}" y="${y + 5}" width="14" height="14" rx="4"/><path ${s} d="M${x + 7} ${y + 12}h8m-3 -4 4 4-4 4"/>`;
      case "business_service":
        return `<rect ${s} x="${x + 4}" y="${y + 5}" width="14" height="14" rx="7"/><path ${s} d="M${x + 8} ${y + 12}h6"/>`;
      case "business_object":
        return `<rect ${s} x="${x + 4}" y="${y + 5}" width="14" height="14"/><path ${s} d="M${x + 4} ${y + 9}h14"/>`;
      case "application_component":
        return `<rect ${s} x="${x + 4}" y="${y + 4}" width="14" height="16" rx="2"/><path ${s} d="M${x + 7} ${y + 7}v10M${x + 10} ${y + 7}v10"/>`;
      case "technology_node":
        return `<path ${s} d="M${x + 5} ${y + 7}l4 -3h11v15h-15zM${x + 5} ${y + 7}h11M${x + 16} ${y + 7}v15"/>`;
      case "system_software":
        return `<rect ${s} x="${x + 4}" y="${y + 4}" width="16" height="16" rx="2"/><path ${s} d="M${x + 7} ${y + 9}h10M${x + 7} ${y + 13}h10M${x + 7} ${y + 17}h10"/>`;
      case "artifact":
        return `<path ${s} d="M${x + 5} ${y + 4}h9l4 4v13H${x + 5}zM${x + 14} ${y + 4}v4h4"/>`;
      case "constraint":
        return `<rect ${s} x="${x + 4}" y="${y + 4}" width="16" height="16" rx="2"/><path ${s} d="M${x + 9} ${y + 16}c-4 -5 1 -9 4 -6"/>`;
      case "outcome":
        return `<rect ${s} x="${x + 4}" y="${y + 5}" width="16" height="14" rx="7"/><path ${s} d="M${x + 8} ${y + 12}h7m-3 -3v6"/>`;
      case "requirement":
        return `<path ${s} d="M${x + 5} ${y + 6}h14v13H${x + 5}zM${x + 8} ${y + 10}h8M${x + 8} ${y + 13}h8M${x + 8} ${y + 16}h5"/>`;
      case "application_service":
        return `<rect ${s} x="${x + 4}" y="${y + 5}" width="16" height="14" rx="7"/><path ${s} d="M${x + 8} ${y + 12}h8m-3 -3 3 3-3 3"/>`;
      default:
        return `<rect ${s} x="${x + 4}" y="${y + 4}" width="16" height="16"/>`;
    }
  };

  const shape = (kind, e) => {
    const { x, y, w, h } = e;
    switch (kind) {
      case "business_actor":
      case "business_role":
      case "business_object":
      case "application_component":
      case "system_software":
      case "artifact":
        return `<rect class="arch-shape" x="${x}" y="${y}" width="${w}" height="${h}" rx="3"/>`;
      case "business_process":
      case "business_service":
      case "application_service":
      case "outcome":
        return `<rect class="arch-shape" x="${x}" y="${y}" width="${w}" height="${h}" rx="16"/>`;
      case "technology_node":
        return `<rect class="arch-shape" x="${x}" y="${y}" width="${w}" height="${h}" rx="3"/><path class="arch-shape-top" d="M${x} ${y + 9}h${w}M${x + 9} ${y}v${h}"/>`;
      case "constraint":
        return `<rect class="arch-shape arch-dashed" x="${x}" y="${y}" width="${w}" height="${h}" rx="3"/>`;
      case "requirement":
        return `<rect class="arch-shape" x="${x}" y="${y}" width="${w}" height="${h}" rx="3"/>`;
      default:
        return `<rect class="arch-shape" x="${x}" y="${y}" width="${w}" height="${h}" rx="3"/>`;
    }
  };

  const statusChip = (e) => {
    if (!e.status || e.status === "—") return "";
    const w = e.status === "V1" ? 20 : 22;
    const x = e.x + e.w - w - 6;
    const y = e.y + 6;
    return `<rect class="arch-status-chip ${e.status === "V1" ? "v1" : "v2"}" x="${x}" y="${y}" width="${w}" height="12" rx="2"/><text class="arch-status-text" x="${x + w / 2}" y="${y + 9}">${E(e.status)}</text>`;
  };

  const anchor = (a, b) => {
    const ax = a.x + a.w / 2;
    const ay = a.y + a.h / 2;
    const bx = b.x + b.w / 2;
    const by = b.y + b.h / 2;
    if (Math.abs(bx - ax) >= Math.abs(by - ay)) {
      return bx > ax
        ? [a.x + a.w, ay, b.x, by, "h"]
        : [a.x, ay, b.x + b.w, by, "h"];
    }
    return by > ay
      ? [ax, a.y + a.h, bx, b.y, "v"]
      : [ax, a.y, bx, b.y + b.h, "v"];
  };

  const relationPath = (a, b) => {
    const [x1, y1, x2, y2, dir] = anchor(a, b);
    if (dir === "h") {
      const mid = (x1 + x2) / 2;
      return `M${x1} ${y1} H${mid} V${y2} H${x2}`;
    }
    const mid = (y1 + y2) / 2;
    return `M${x1} ${y1} V${mid} H${x2} V${y2}`;
  };

  const REL = {
    assignment: { marker: "circle", dash: false },
    realization: { marker: "triangle", dash: true },
    serving: { marker: "open", dash: false },
    access: { marker: "open", dash: true },
    influence: { marker: "open", dash: true },
    flow: { marker: "arrow", dash: true },
  };

  const archSvg = (view) => {
    const byId = Object.fromEntries(view.elements.map((e) => [e.id, e]));
    const marker = (name) => `arch-${view.id}-${name}`;
    return `<svg class="arch-svg" viewBox="0 0 ${view.width} ${view.height}" role="img" aria-labelledby="arch-${view.id}-title arch-${view.id}-desc" xmlns="http://www.w3.org/2000/svg">
      <title id="arch-${view.id}-title">${E(view.title)}</title>
      <desc id="arch-${view.id}-desc">ArchiMate 3.2 view: ${E(view.title)}. ${E(view.spec)}.</desc>
      <defs>
        <marker id="${marker("arrow")}" markerWidth="9" markerHeight="9" refX="8" refY="4.5" orient="auto"><path class="arch-marker-fill" d="M0 0 L9 4.5 L0 9 z"/></marker>
        <marker id="${marker("open")}" markerWidth="10" markerHeight="10" refX="9" refY="5" orient="auto"><path class="arch-marker-open" d="M0 0 L10 5 L0 10"/></marker>
        <marker id="${marker("triangle")}" markerWidth="10" markerHeight="10" refX="9" refY="5" orient="auto"><path class="arch-marker-triangle" d="M0 0 L10 5 L0 10 z"/></marker>
        <marker id="${marker("circle")}" markerWidth="7" markerHeight="7" refX="3.5" refY="3.5" orient="auto"><circle class="arch-marker-fill" cx="3.5" cy="3.5" r="3"/></marker>
      </defs>
      ${view.bands
        .map(
          (b) =>
            `<rect class="arch-band arch-band-${b.cls}" x="${b.x}" y="${b.y}" width="${b.w}" height="${b.h}"/><text class="arch-band-label" x="${b.x + 12}" y="${b.y + 20}">${E(b.label)}</text>`,
        )
        .join("")}
      ${view.relations
        .map((r) => {
          const a = byId[r.from];
          const b = byId[r.to];
          if (!a || !b) return "";
          const rel = REL[r.kind] || REL.serving;
          const markerId = marker(rel.marker);
          return `<path class="arch-rel arch-rel-${r.kind}" d="${relationPath(a, b)}" ${rel.dash ? 'stroke-dasharray="5 4"' : ""} marker-start="${
            r.kind === "assignment" ? `url(#${markerId})` : ""
          }" marker-end="url(#${markerId})"><title>${E(r.kind)}</title></path>`;
        })
        .join("")}
      ${view.elements
        .map((e) => {
          const nameLines = lines(e.name, Math.max(10, Math.floor(e.w / 8)));
          const subLines = e.sub ? lines(e.sub, Math.max(12, Math.floor(e.w / 6.4))) : [];
          return `<g class="arch-el arch-${e.kind} arch-layer-${layerClass(e.kind)}" data-status="${E(e.status || "")}" data-kind="${E(e.kind)}"><title>${E(e.name)} — ${E(e.kind.replaceAll("_", " "))}${e.status ? " — " + E(e.status) : ""}</title>${shape(
            e.kind,
            e,
          )}${glyph(e.kind, e.x + 8, e.y + 8)}${statusChip(e)}${nameLines
            .map(
              (line, i) =>
                `<text class="arch-name" x="${e.x + 34}" y="${e.y + 24 + i * 13}">${E(line)}</text>`,
            )
            .join("")}${subLines
            .slice(0, 2)
            .map(
              (line, i) =>
                `<text class="arch-sub" x="${e.x + 34}" y="${e.y + 24 + nameLines.length * 13 + i * 11}">${E(line)}</text>`,
            )
            .join("")}</g>`;
        })
        .join("")}
    </svg>`;
  };

  const mini = (kind) => {
    const e = { x: 4, y: 6, w: 46, h: 24 };
    return `<svg class="arch-mini" viewBox="0 0 54 36" aria-hidden="true">${shape(kind, e)}${glyph(kind, 8, 8)}</svg>`;
  };

  const archLegend = () => {
    const kinds = [
      ["business_actor", "Business actor"],
      ["business_role", "Business role"],
      ["business_process", "Business process"],
      ["business_service", "Business service"],
      ["business_object", "Business object"],
      ["application_component", "Application component"],
      ["technology_node", "Technology node"],
      ["system_software", "System software"],
    ];
    return `<div class="arch-legend">
      <div class="arch-legend-row"><span class="arch-legend-head">LAYERS</span><span class="arch-swatch motivation"></span>Motivation<span class="arch-swatch business"></span>Business<span class="arch-swatch application"></span>Application<span class="arch-swatch technology"></span>Technology</div>
      <div class="arch-legend-row"><span class="arch-legend-head">ELEMENTS</span>${kinds
        .map(([kind, label]) => `<span class="arch-legend-item">${mini(kind)}${E(label)}</span>`)
        .join("")}</div>
      <div class="arch-legend-row"><span class="arch-legend-head">RELATIONS</span><span class="arch-legend-item"><svg class="arch-mini" viewBox="0 0 54 20" aria-hidden="true"><path class="arch-rel arch-rel-assignment" d="M4 10 H50"/></svg>assignment</span><span class="arch-legend-item"><svg class="arch-mini" viewBox="0 0 54 20" aria-hidden="true"><path class="arch-rel arch-rel-realization" d="M4 10 H50" stroke-dasharray="5 4"/></svg>realization</span><span class="arch-legend-item"><svg class="arch-mini" viewBox="0 0 54 20" aria-hidden="true"><path class="arch-rel arch-rel-serving" d="M4 10 H50"/></svg>serving</span><span class="arch-legend-item"><svg class="arch-mini" viewBox="0 0 54 20" aria-hidden="true"><path class="arch-rel arch-rel-access" d="M4 10 H50" stroke-dasharray="5 4"/></svg>access / influence</span><span class="arch-legend-item"><svg class="arch-mini" viewBox="0 0 54 20" aria-hidden="true"><path class="arch-rel arch-rel-flow" d="M4 10 H50" stroke-dasharray="5 4"/></svg>flow</span></div>
      <div class="arch-legend-row"><span class="arch-legend-head">STATUS</span><span class="arch-chip v1">V1</span>implemented in the Rust app<span class="arch-chip v2">V2</span>proposed in SPEC_V2 — no V2 code</div>
    </div>`;
  };

  /* ---------------------------------------------------------------- page blocks */
  const panel = (title, spec, body) =>
    `<section class="e-panel"><div class="e-panel-head"><h2>${E(title)}</h2><span class="mono muted">${E(spec)}</span></div><div class="v2-panel-body">${body}</div></section>`;

  const figure = (key) => {
    const d = DIAGRAMS[key];
    if (!d) return "";
    const src = SOURCES[key] || "";
    return `<figure class="arch-figure${d.tall ? " arch-tall" : ""}" data-diagram="${E(key)}">
      <div class="arch-frame"><img class="arch-light" src="architecture/diagrams/${E(key)}.light.svg" alt="${E(d.title)}"><img class="arch-dark" src="architecture/diagrams/${E(key)}.dark.svg" alt="${E(d.title)}"></div>
      <figcaption><strong>${E(d.title)}</strong><span class="mono">${E(d.spec)}</span><span>${E(d.caption)}</span></figcaption>
      <details class="arch-source"><summary>Mermaid source · pre-rendered to SVG at authoring time</summary><pre>${E(src)}</pre></details>
    </figure>`;
  };

  const archFigure = (view) => `<figure class="arch-figure arch-archimate" data-view="${E(view.id)}">
      <div class="arch-frame">${archSvg(view)}</div>
      <figcaption><strong>${E(view.title)}</strong><span class="mono">${E(view.spec)}</span></figcaption>
    </figure>`;

  const tabbar = () =>
    `<div class="tabs e-tabs arch-tabs" role="tablist">${TABS.map(
      (t) =>
        `<button type="button" class="btn e-tab ${t === state.tab ? "selected" : ""}" data-arch-tab="${E(t)}" role="tab" aria-pressed="${t === state.tab}">${E(t)}</button>`,
    ).join("")}</div>`;

  const statusBadge = (status) => {
    const tone =
      status === "R" ? "green" : status === "P" || status === "R + P" ? "amber" : status === "L" ? "purple" : "";
    return badge(status, tone);
  };

  const uiTable = (rows, admin = false) =>
    `<div class="e-table-scroll"><table class="e-table arch-ui-table"><thead><tr><th>Studio destination</th><th>Page id</th><th>Architecture element(s)</th><th>SPEC_V2</th>${admin ? "" : "<th>Status</th>"}</tr></thead><tbody>${rows
      .map(
        (r) =>
          `<tr ${admin ? `data-admin-surface="${E(r[1])}"` : `data-ui-page="${E(r[1])}"`}><td><strong>${E(r[0])}</strong></td><td class="mono">${E(r[1])}</td><td>${E(r[2])}</td><td class="mono">${E(r[3])}</td>${admin ? "" : `<td>${statusBadge(r[4])}</td>`}</tr>`,
      )
      .join("")}</tbody></table></div>`;

  const highLevel = () =>
    `<div class="arch-stack">
      ${panel(
        "How to read this page",
        "STUDIO-SPEC · three-way status",
        `<div class="arch-grid-4">
          <div class="arch-note"><strong>Job</strong><span>A configured position: charter, capability, procedure, inference, knowledge, tools, limits and gates.</span><span class="mono">§4</span></div>
          <div class="arch-note"><strong>Card</strong><span>The work package: one assignment, one owner, one state.</span><span class="mono">§5</span></div>
          <div class="arch-note"><strong>Approval</strong><span>The one human-escalation primitive; a question is an approval.</span><span class="mono">§5.4</span></div>
          <div class="arch-note"><strong>Run</strong><span>One attempt through an executor, pinned to a binding and a release hash.</span><span class="mono">§4.5</span></div>
        </div>
        <p class="arch-prose">Everything else is a view or an instrument. The diagrams use ArchiMate 3.2 shapes; <span class="arch-chip v1">V1</span> elements are implemented in the Rust app, <span class="arch-chip v2">V2</span> elements are proposed in the draft with no V2 code, and the legacy study surfaces are labelled in the UI map. Each caption names the SPEC_V2 section it draws.</p>`,
      )}
      ${panel("Organisation and work — motivation and business", "§3–§7", archFigure(VIEWS[0]))}
      ${panel("Platform and integrations — application", "§10.1–§10.2, §11.7", archFigure(VIEWS[1]))}
      ${panel("Deployment — technology", "§10.6, D57, D58", archFigure(VIEWS[2]))}
      ${panel("ArchiMate notation", "ArchiMate 3.2", archLegend())}
    </div>`;

  const runtime = () =>
    `<div class="arch-stack">
      ${panel("Component and context", "§10.1, §10.6", figure("context"))}
      ${panel("Run execution", "§4.4, §4.5, §7.7", figure("run-sequence"))}
      ${panel("Tool broker", "§11.3, §11.4", figure("broker-sequence"))}
      <div class="arch-two">${panel("Run state machine", "§5.1, §6.6", figure("run-state"))}${panel("Decision ladder", "§7.1–§7.4, D30", figure("decision-ladder"))}</div>
      ${panel(
        "Hard invariants",
        "§3, P2–P9",
        `<div class="arch-grid-3">
          <div class="arch-note"><strong>No work without a card</strong><span>No transition without an action; no completion without evidence.</span></div>
          <div class="arch-note"><strong>L2 has no transition path</strong><span>A generated output is a draft artefact; applying it is a gated action.</span></div>
          <div class="arch-note"><strong>Fail closed on missing configuration</strong><span>An unresolved binding, pack, prompt source or tool refuses before the first model call or action.</span></div>
        </div>`,
      )}
    </div>`;

  const dataKnowledge = () =>
    `<div class="arch-stack">
      ${panel("Organisation and work spine", "§4.1–§4.5, §5.1, §10.3", figure("er-organisation-work"))}
      ${panel("Execution, decisions and evidence", "§7.1–§7.5, §10.3", figure("er-execution-decisions"))}
      ${panel("Knowledge, memory and tools", "§7.6, §11.1, §11.8", figure("er-knowledge-tools"))}
      ${panel("Knowledge pipeline", "§5.6, §5.7, D47, D53", figure("knowledge-pipeline"))}
      ${panel("Memory and lesson lifecycle", "§7.6, D36", figure("memory-lifecycle"))}
      ${panel(
        "New versus reused",
        "§10.3",
        `<div class="arch-matrix"><div><span class="mono">NEW IN V2</span><p>industry · department · job_catalogue · job_release · job_binding · wbs_node · project_baseline · project_register_item · project_dependency · programme_cadence · decision · decision_option · decision_policy · decision_policy_refinement · laya_inference_config · pko_release · pko_entity · pko_relation · lesson · tool · toolkit · tool_binding · tool_call</p></div><div><span class="mono">REUSED FROM V1</span><p>tenant · programme (extended) · project · card · run · run_step · approval · governed_asset (extended) · trace_link · event · job · audit_event · inference_call · eval* · benchmark* · procedure / procedure_step (extended) · secret</p></div></div>`,
      )}
    </div>`;

  const improvement = () =>
    `<div class="arch-stack">
      ${panel("One improvement loop", "§9.4, D34", figure("improvement-loop"))}
      ${panel("Delivery slices T0–T17", "§12", figure("slice-dag"))}
      ${panel(
        "Executable rigour — rows and gates, not documents",
        "§8, D32",
        `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Framework</th><th>V2 implementation</th><th>Gate / evidence</th><th>Destination</th></tr></thead><tbody>
          <tr><td><strong>TOGAF ADM</strong></td><td>Architecture project type; phases A–H as WBS nodes; contracts as governed assets</td><td>Per-phase contract gate; delivery refuses without its contract</td><td>Assurance &amp; baselines · Requirements</td></tr>
          <tr><td><strong>SAFe</strong></td><td>Portfolio (computed) · programme/ART · PI planning · backlog · system demo · inspect &amp; adapt</td><td>No demo, no credit; PI plan approved by a person</td><td>Portfolio · Plan &amp; WBS</td></tr>
          <tr><td><strong>PMP</strong></td><td>Charter · WBS · baseline · one six-kind register · change control · closure</td><td>Change impact and rationale; closure evidence</td><td>Projects · Plan &amp; WBS</td></tr>
          <tr><td><strong>Kata</strong></td><td>Six Gets; named instrument and success threshold</td><td>Adoption gate; training-only gain reverted</td><td>Improve</td></tr>
        </tbody></table></div>
        <p class="arch-prose">No TOGAF certification, no SAFe compliance badge, no PMP certification. The claim is that the artefacts and gates the organisation uses are rows, the process runs, and the evidence is traceable.</p>`,
      )}
    </div>`;

  const uiMap = () =>
    `<div class="arch-stack">
      ${panel("Architecture → studio destinations", "§10.5, STUDIO-SPEC destination map", figure("ui-map"))}
      ${panel(
        "Every sidebar destination",
        "35 destinations · R = V1 implemented · P = V2 proposed · L = legacy study",
        `${uiTable(UI_ROWS)}<p class="arch-prose">The row for a destination names the aggregates and rows it views; a V2 preview never claims a write path. The <span class="mono">architecture</span> row is this page itself.</p>`,
      )}
      ${panel(
        "Tenant administration — app surfaces",
        "S12 app screens · not mockup destinations",
        `${uiTable(ADMIN_ROWS, true)}<p class="arch-prose">Providers and credentials are write-only; a missing token is a refusal, never a fallback. Licences are entitlement; metering is usage. The model-tier form mirrors the fr vocabulary and is a labelled preview.</p>`,
      )}
      ${panel(
        "How the UI was derived",
        "STUDIO-SPEC · V1 / V2 / legacy",
        `<div class="arch-grid-3">
          <div class="arch-note"><strong>V1 · implemented</strong><span>Projects, cards, runs, approvals, procedures, knowledge/memory, evals, benchmarks, hillclimb, prompt audit, ledger, secrets, channels, licences and billing.</span></div>
          <div class="arch-note"><strong>V2 · proposed</strong><span>Organisation spine, plan spine, decision ladder, PM/coach/kata/development/operations jobs, the tool estate, metering, authoring and the knowledge workbench.</span></div>
          <div class="arch-note"><strong>L · legacy study</strong><span>The historical job catalogue, the coding action catalogue and the TOC study. Retained and labelled; never presented as the live model.</span></div>
        </div>`,
      )}
    </div>`;

  const page = (studioApi) => {
    api = studioApi;
    const body =
      state.tab === "High level"
        ? highLevel()
        : state.tab === "Runtime"
          ? runtime()
          : state.tab === "Data & knowledge"
            ? dataKnowledge()
            : state.tab === "Improvement & delivery"
              ? improvement()
              : uiMap();
    return `<div class="page enterprise arch-page">${api.header(
      "System architecture",
      "ArchiMate 3.2 high-level views and Mermaid low-level design of SPEC_V2 — and how every studio destination maps onto it.",
      `${button("Portfolio", 'data-page="portfolio"', "", "layers")}${button("Jobs", 'data-page="jobs"', "", "board")}<button type="button" class="btn" data-arch-tab="UI map">${api.icon("model")}UI map</button>`,
      "GOVERN & DESIGN / ARCHITECTURE",
    )}<div class="v2-ribbon" role="note"><strong>V2 · PROPOSED</strong><span>ArchiMate 3.2 + Mermaid design study. V1 elements are solid; V2 aggregates are dashed and proposed (T0–T17). The Mermaid SVGs are pre-rendered locally; the page makes no external request.</span><span class="v2-src">SPEC_V2 draft · no V2 code written · local design fixture</span></div>${tabbar()}${body}</div>`;
  };

  document.addEventListener("click", (event) => {
    const el = event.target.closest("[data-arch-tab]");
    if (!el || !api) return;
    const next = el.getAttribute("data-arch-tab");
    if (TABS.includes(next)) {
      state.tab = next;
      api.render();
    }
  });

  return {
    page,
    sources: SOURCES,
    views: VIEWS,
    uiRows: UI_ROWS,
    adminRows: ADMIN_ROWS,
    select(tab) {
      if (TABS.includes(tab)) state.tab = tab;
    },
  };
})();
