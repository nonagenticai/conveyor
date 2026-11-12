/* Programme-scale interaction study. All records and service responses below are local fixtures. */
"use strict";
window.installEnterprise = function (api) {
  const {
    db,
    state,
    esc: E,
    icon: I,
    badge: B,
    btn,
    navButton: N,
    header: H,
    navigate,
    render,
    openDialog,
    dialogForm,
    field: F,
    selectField: S,
    error,
    toast,
    activity,
    byId,
    nameOf,
    coreTools,
    download,
  } = api;
  const $ = (q, r = document) => r.querySelector(q);
  const eb = (label, action, attrs = "", cls = "", symbol = "") =>
    `<button class="btn ${cls}" data-e="${action}" ${attrs}>${symbol ? I(symbol) : ""}${E(label)}</button>`;
  const route = (label, page, cls = "", symbol = "arrow") =>
    btn(label, "navigate", `data-page="${page}"`, cls, symbol);
  const panel = (title, content, right = "") =>
    `<section class="e-panel"><div class="e-panel-head"><h2>${title}</h2>${right}</div>${content}</section>`;
  const empty = (text) => `<div class="empty">${E(text)}</div>`;
  const note = (text) => `<div class="info-note">${text}</div>`;
  const tabs = (values, active, action) =>
    `<div class="tabs e-tabs">${values.map((t) => eb(t, action, `data-value="${E(t)}" aria-pressed="${t === active}"`, t === active ? "e-tab selected" : "e-tab")).join("")}</div>`;
  const heading = (
    title,
    sub,
    actions = "",
    eyebrow = "NON-AGENTIC AI / ENTERPRISE CONVEYOR",
  ) => H(title, sub, actions, eyebrow);
  const fmt = (n) => n.toLocaleString("en-US");
  const x = {
    programme: "civic",
    tab: "Delivery map",
    project: "identity",
    expanded: new Set(["identity"]),
    knowledge: "Sources & ingestion",
    knowledgeQuery:
      "Which identity constraints apply to the target architecture?",
    retrieval: null,
    packAvailable: true,
    packDraft: 18,
    packPublished: 17,
    packRetired: false,
    selectedJob: "architecture_baseline_harvest",
    jobQuery: "",
    jobCapability: "all",
    jobPage: 0,
    runTab: "Activity",
    assurance: "Traceability",
    toc: "Current reality",
    tocNode: 1,
    baseline: 3,
    change: "Draft",
    sourceIndex: 0,
    passageIndex: 0,
    passageCorrection: false,
    retrievalExpected:
      "delegated identities use explicit action grants",
    retrievalSaved: false,
    caseName: "",
    workFilter: "All work",
    workSearch: "",
    hot: [],
    consolidated: 0,
    runState: "Waiting for review",
    runAttempt: 2,
    runBaseline: 3,
    runPack: "capability@17",
    queuedJobs: [],
    eventLog: [],
    contextPreview: false,
    reviewDecision: "",
    planning: "Baseline",
  };
  const jobSource = window.JOB_CATALOGUE;
  const jobs = jobSource.jobs.map((j) => ({
    ...j,
    id: j.name,
    status: "Seed",
    process: "",
    agent: "",
    version: 1,
    entry: "Approved inputs and applicable baseline",
    exit:
      (j.acceptance || []).join("\n") ||
      "Tenant-defined acceptance evidence required",
    required: true,
  }));
  const extraJobs = [
    [
      "benefits_realisation_review",
      "Benefits realisation review",
      "program_management",
      "Compare measured outcomes with the approved benefits baseline.",
    ],
    [
      "architecture_compliance_assessment",
      "Architecture compliance assessment",
      "architecture",
      "Assess solution evidence against principles, requirements and the architecture contract.",
    ],
    [
      "critical_chain_recovery",
      "Critical chain recovery",
      "project_management",
      "Identify the delivery constraint, model an injection and review the recovery plan.",
    ],
    [
      "knowledge_pack_assurance",
      "Knowledge pack assurance",
      "research",
      "Review provenance, supersession and readiness before publishing a domain pack.",
    ],
    [
      "business_case_gate",
      "Business case gate preparation",
      "program_management",
      "Assemble strategic alignment, options, benefits and stage evidence for a sponsor decision.",
    ],
    [
      "requirements_baseline_review",
      "Requirements baseline review",
      "architecture",
      "Check acceptance, coverage, conflicts and source revisions before baselining.",
    ],
  ];
  extraJobs.forEach(([id, title, capability, description]) =>
    jobs.unshift({
      id,
      name: id,
      title,
      capability,
      description,
      inputs: ["approved_baseline", "scope", "evidence_refs"],
      outputs: ["assessment", "recommendation"],
      acceptance: [
        "Every conclusion has evidence",
        "A person decides the gate",
      ],
      origin: "Tenant extension · design proposal",
      status: "Draft",
      process: "model",
      agent: "architect",
      version: 1,
      entry: "Approved scope and resolved required knowledge",
      exit: "Evidence attached; independent assessment; human sign-off",
      required: true,
    }),
  );
  // Bind one source job explicitly; other seeds require authoring before execution.
  Object.assign(
    jobs.find((j) => j.id === "architecture_baseline_harvest"),
    {
      status: "Published",
      process: "model",
      agent: "architect",
      inputs: [
        "repository_ref",
        "approved_scope_baseline",
        "required_knowledge_pin",
      ],
      origin: "RM / n8n seed + tenant contract overlay",
      entry:
        "Repository linked; architecture scope approved; capability@17 resolves",
      exit: "Baseline model and code bindings exist; validation attached; architect review accepted",
    },
  );
  const capMap = {
    development: "delivery",
    research: "discovery",
    analysis: "discovery",
    testing: "verification",
    bdd_tdd: "verification",
  };
  jobSource.capabilities.forEach((c) => {
    const id = capMap[c.name] || c.name;
    if (!byId("capabilities", id))
      db.capabilities.push({
        id,
        name: c.title || c.name.replaceAll("_", " "),
        description:
          c.description ||
          "Tenant capability seeded from the historical PM catalogue; extend for your operating model.",
        status: "Draft",
        version: 1,
        processes: [],
        knowledge: "capability@17",
      });
  });
  db.capabilities.forEach((c) => {
    c.knowledge = "capability@17";
  });
  const newActions = [
    "knowledge.query",
    "memory.remember",
    "memory.consolidate",
    "documents.ingest",
    "retrieval.query",
    "models.traceability",
    "models.impact",
    "models.gaps",
    "models.completeness",
    "models.publish",
    "model_views.generate",
    "outlines.create",
    "outline_nodes.create",
  ];
  newActions.forEach((id) => {
    if (!coreTools.some((t) => t.id === id))
      coreTools.push({
        id,
        title: id,
        group: "Conveyor",
        method: "ACTION",
        path: "S07 / S08 / S09 / S15 · local contract projection",
        role: /query|recall|traceability|impact|gaps|completeness/.test(id)
          ? "observer"
          : "producer",
        verifies: [],
        fields: [],
        source: "conveyor",
      });
  });
  db.agents
    .filter((a) => a.kind === "Internal agent")
    .forEach((a) => {
      a.tools.push(
        ...["knowledge.query", "memory.recall"].filter(
          (t) => !a.tools.includes(t),
        ),
      );
    });
  db.principles.push({
    id: "interoperability",
    key: "PRN-012",
    name: "Shared contracts before local integration",
    description:
      "Services exchange governed contracts and identifiers with named ownership. Changes are assessed across consuming workstreams.",
    acceptance:
      "Every shared contract has an owner, version, compatibility policy and consumer impact assessment.",
    status: "Approved",
    version: 3,
    project: "",
    history: [
      "v3 · Approved by architecture board",
      "v2 · Consumer impact obligations added",
    ],
  });
  db.requirements.push(
    {
      id: "shared-data",
      key: "REQ-208",
      name: "Establish authoritative shared identifiers",
      description:
        "Every citizen-facing service resolves an authoritative identifier through the shared data foundation. Legacy mappings retain provenance and an accountable data owner.",
      acceptance:
        "All in-scope services map their identifiers.\nMapping exceptions have approved dispositions.\nData contracts and migration tests cite the same baseline.",
      status: "Approved",
      version: 4,
      project: "data",
      principle: "interoperability",
      history: [
        "v4 · Baseline B3 approved",
        "v3 · Cross-workstream impact reviewed",
      ],
    },
    {
      id: "continuity-req",
      key: "REQ-312",
      name: "Demonstrate continuity through transition",
      description:
        "The transition plan demonstrates recoverability and service continuity for each migration wave, with evidence accepted by the service owner.",
      acceptance:
        "Each wave has a rollback and recovery test.\nThe service owner accepts operational evidence before cutover.",
      status: "Draft",
      version: 2,
      project: "continuity",
      principle: "evidence",
      history: [
        "v2 · Recovery evidence requirements added",
        "v1 · Transition requirement drafted",
      ],
    },
  );
  const enterpriseElements = [
    { id: "goal", name: "Reduce service fulfilment time", kind: "goal" },
    {
      id: "principle",
      name: "Shared contracts before local integration",
      kind: "principle",
      asset: "interoperability",
    },
    {
      id: "requirement",
      name: "Authoritative shared identifiers",
      kind: "requirement",
      asset: "shared-data",
    },
    { id: "identity-cap", name: "Trust & identity", kind: "capability" },
    { id: "data-cap", name: "Data interoperability", kind: "capability" },
    { id: "service-cap", name: "Citizen service delivery", kind: "capability" },
    {
      id: "identity-app",
      name: "Identity platform",
      kind: "application_component",
    },
    {
      id: "data-app",
      name: "Shared data foundation",
      kind: "application_component",
    },
    {
      id: "service-app",
      name: "Service orchestration",
      kind: "application_component",
    },
    { id: "migration-wp", name: "Transition wave 2", kind: "work_package" },
    { id: "contract", name: "Architecture contract", kind: "deliverable" },
    { id: "gap", name: "Legacy identifier fragmentation", kind: "gap" },
  ];
  db.models.unshift({
    id: "programme-landscape",
    name: "Connected services · enterprise landscape",
    language: "ArchiMate",
    project: "identity",
    version: 4,
    status: "Draft",
    elements: enterpriseElements,
    relationships: [
      {
        id: "e1",
        source: "principle",
        target: "requirement",
        kind: "influence",
      },
      {
        id: "e2",
        source: "requirement",
        target: "data-cap",
        kind: "association",
      },
      { id: "e3", source: "data-app", target: "data-cap", kind: "realization" },
      {
        id: "e4",
        source: "identity-app",
        target: "identity-cap",
        kind: "realization",
      },
      {
        id: "e5",
        source: "service-app",
        target: "service-cap",
        kind: "realization",
      },
      { id: "e6", source: "identity-app", target: "data-app", kind: "serving" },
      { id: "e7", source: "data-app", target: "service-app", kind: "serving" },
      {
        id: "e8",
        source: "migration-wp",
        target: "contract",
        kind: "realization",
      },
      { id: "e9", source: "service-cap", target: "goal", kind: "influence" },
    ],
    views: [
      {
        id: "v-programme",
        name: "Strategy to execution",
        kind: "archimate_view",
        elements: [
          "goal",
          "principle",
          "requirement",
          "identity-cap",
          "data-cap",
          "service-cap",
          "migration-wp",
          "contract",
          "gap",
        ],
      },
      {
        id: "v-service-cooperation",
        name: "Cross-project application cooperation",
        kind: "archimate_view",
        elements: ["identity-app", "data-app", "service-app"],
      },
      {
        id: "v-transition",
        name: "Transition work & gaps",
        kind: "archimate_view",
        elements: ["gap", "migration-wp", "contract", "data-app"],
      },
    ],
  });
  Object.assign(state.selected, {
    types: "transformation",
    capabilities: "architecture",
    processes: "model",
    agents: "architect",
    models: "programme-landscape",
  });
  state.modelView = "v-programme";
  let runJobPin = structuredClone(
    jobs.find((j) => j.id === "architecture_baseline_harvest"),
  );
  let runAgentPin = structuredClone(byId("agents", "architect"));
  let runProcessPin = structuredClone(byId("processes", "model"));
  db.types.unshift({
    id: "transformation",
    name: "Enterprise transformation",
    description:
      "Multi-wave transformation with architecture, requirements and delivery assurance at each stage.",
    status: "Published",
    version: 6,
    phases: [
      "Initiate",
      "Baseline",
      "Target design",
      "Transition planning",
      "Delivery",
      "Transition",
      "Benefits review",
    ],
    capabilities: db.capabilities.map((c) => c.id),
    cardTypes: jobs.map((j) => j.id),
    gates: [
      "Charter approval",
      "Architecture contract",
      "Integrated baseline",
      "Change control board",
      "Operational acceptance",
      "Benefits review",
    ],
  });
  const programmeDefs = [
    {
      id: "civic",
      key: "PGM-001",
      name: "Connected public services",
      sub: "Identity, data and service transformation",
      horizon: "2026 — 2029",
      sponsor: "Programme board",
      benefit: "Reduce service fulfilment time by 40%",
      budget: "€48.6m",
      risk: "Architecture decision overdue",
      tone: "amber",
    },
    {
      id: "resilience",
      key: "PGM-002",
      name: "Platform resilience",
      sub: "Infrastructure, security and operational continuity",
      horizon: "2026 — 2028",
      sponsor: "Technology board",
      benefit: "Meet the agreed service continuity objectives",
      budget: "€19.2m",
      risk: "On track",
      tone: "green",
    },
    {
      id: "operating",
      key: "PGM-003",
      name: "Adaptive operating model",
      sub: "Organisation, knowledge and agent operations",
      horizon: "2026 — 2028",
      sponsor: "Transformation office",
      benefit: "Increase verified delivery throughput by 25%",
      budget: "€12.4m",
      risk: "Knowledge readiness gap",
      tone: "amber",
    },
  ];
  const additions = [
    [
      "data",
      "PRJ-024",
      "Common data foundation",
      "civic",
      "Data & interoperability",
      684,
      238,
      "Target design",
      3,
      18,
    ],
    [
      "service",
      "PRJ-025",
      "Service orchestration",
      "civic",
      "Citizen services",
      872,
      321,
      "Delivery",
      2,
      23,
    ],
    [
      "migration",
      "PRJ-026",
      "Legacy estate transition",
      "civic",
      "Transition & adoption",
      946,
      116,
      "Transition planning",
      5,
      31,
    ],
    [
      "security",
      "PRJ-027",
      "Zero-trust platform",
      "resilience",
      "Security & assurance",
      528,
      214,
      "Delivery",
      0,
      16,
    ],
    [
      "continuity",
      "PRJ-028",
      "Operational continuity",
      "resilience",
      "Service operations",
      418,
      163,
      "Target design",
      1,
      10,
    ],
    [
      "cloud",
      "PRJ-029",
      "Cloud landing zones",
      "resilience",
      "Infrastructure",
      764,
      410,
      "Delivery",
      0,
      22,
    ],
    [
      "knowledge",
      "PRJ-030",
      "Enterprise knowledge fabric",
      "operating",
      "Knowledge & learning",
      368,
      121,
      "Baseline",
      4,
      12,
    ],
    [
      "workforce",
      "PRJ-031",
      "Agent operating model",
      "operating",
      "Organisation & capability",
      492,
      85,
      "Target design",
      1,
      9,
    ],
  ];
  additions.forEach(
    ([
      id,
      key,
      name,
      programme,
      stream,
      total,
      completed,
      phase,
      blocks,
      live,
    ]) =>
      db.projects.push({
        id,
        key,
        name,
        programme,
        stream,
        total,
        completed,
        enterprisePhase: phase,
        blocks,
        live,
        description: `${stream}: governed transformation work across architecture, requirements, delivery and acceptance.`,
        type: "transformation",
        phase: [
          "Initiate",
          "Baseline",
          "Target design",
          "Transition planning",
          "Delivery",
        ].indexOf(phase),
        status: blocks ? "Needs input" : "Running",
        tone: blocks ? "amber" : "green",
        agents: ["analyst", "architect", "builder", "verifier"],
        spent: 18.4,
        process: "model",
        symbol: "layers",
      }),
  );
  const first = [
    ["portal", "civic", "Citizen services", 862, 394, 2, 14],
    ["billing", "civic", "Core services", 724, 287, 1, 19],
    ["identity", "civic", "Trust & identity", 618, 242, 3, 17],
    ["insights", "operating", "Performance & benefits", 446, 134, 0, 11],
  ];
  first.forEach(([id, programme, stream, total, completed, blocks, live]) =>
    Object.assign(byId("projects", id), {
      programme,
      stream,
      total,
      completed,
      blocks,
      live,
      enterprisePhase:
        id === "identity"
          ? "Target design"
          : id === "portal"
            ? "Delivery"
            : "Baseline",
    }),
  );
  const selectedProgramme = () =>
    programmeDefs.find((p) => p.id === x.programme);
  const scopeProjects = () =>
    db.projects.filter((p) => p.programme === x.programme);
  const project = () => byId("projects", x.project) || scopeProjects()[0];
  const packages = [
    {
      id: "WP-2048",
      wbs: "1.2.3",
      project: "identity",
      name: "Harvest identity baseline architecture",
      job: "architecture_baseline_harvest",
      cap: "architecture",
      owner: "architect",
      state: "Review",
      evidence: "7 artifacts",
      risk: "Critical chain",
      requirement: "delegation",
    },
    {
      id: "WP-2049",
      wbs: "1.2.4",
      project: "identity",
      name: "Approve delegated identity requirements baseline",
      job: "requirements_baseline_review",
      cap: "architecture",
      owner: "analyst",
      state: "Blocked",
      evidence: "2 missing traces",
      risk: "Needs WP-2048",
      requirement: "delegation",
    },
    {
      id: "WP-2050",
      wbs: "1.3.1",
      project: "data",
      name: "Map shared identifiers and data ownership",
      job: "dependency_mapping",
      cap: "analysis",
      owner: "architect",
      state: "Active",
      evidence: "4 artifacts",
      risk: "Near critical",
      requirement: "delegation",
    },
    {
      id: "WP-2051",
      wbs: "2.1.2",
      project: "portal",
      name: "Verify citizen invitation boundaries",
      job: "verification_test",
      cap: "testing",
      owner: "verifier",
      state: "Review",
      evidence: "Test results",
      risk: "Ready for gate",
      requirement: "invite",
    },
    {
      id: "WP-2052",
      wbs: "3.1.4",
      project: "knowledge",
      name: "Validate pack provenance and supersession",
      job: "knowledge_pack_assurance",
      cap: "research",
      owner: "analyst",
      state: "Blocked",
      evidence: "Missing required pack",
      risk: "Readiness",
      requirement: "usage",
    },
    {
      id: "WP-2053",
      wbs: "4.2.1",
      project: "billing",
      name: "Prove billing contract parity",
      job: "rewrite_parity_qa",
      cap: "development",
      owner: "verifier",
      state: "Active",
      evidence: "Parity suite",
      risk: "External dependency",
      requirement: "contract",
    },
  ];
  const sourceJobs = [
    {
      id: "ING-028",
      title: "Identity architecture baseline.pdf",
      kind: "File · PDF",
      store: "tenant-civic-architecture",
      source: "architecture-baseline",
      cap: "architecture",
      step: 7,
      status: "Delivered",
      pages: 86,
      licence: "Tenant-owned",
      date: "10 Nov 2026",
      provenance: "Architecture Office · approved internal document",
    },
    {
      id: "ING-029",
      title: "Service catalogue and data contracts",
      kind: "API source",
      store: "tenant-civic-data",
      source: "service-catalogue",
      cap: "analysis",
      step: 4,
      status: "Enriching",
      pages: 124,
      licence: "Tenant-owned",
      date: "10 Nov 2026",
      provenance: "Service registry · snapshot 2026-11-10",
    },
    {
      id: "ING-030",
      title: "Transformation assurance handbook",
      kind: "URL · HTML",
      store: "tenant-civic-methods",
      source: "assurance-handbook",
      cap: "project_management",
      step: 2,
      status: "Detecting",
      pages: 32,
      licence: "Tenant-owned",
      date: "10 Nov 2026",
      provenance: "Transformation Office · owned internal handbook",
    },
  ];
  const sourceEvidence = {
    "architecture-baseline": {
      states: [
        ["Imported", "done", "ING-028 · source_run delivered"],
        ["Extracted", "warn", "86 pages · 2 parse errors · 1 missing table"],
        ["Reviewed", "done", "A. Mensah · source owner · 2026-11-10"],
        ["Published", "done", "capability@17 · immutable pack release"],
        ["Eligible", "done", "software → delivery → backend_engineer"],
        ["Retrieved", "done", "12 questions in the last programme cycle"],
        ["Included", "done", "context manifest for RUN-8821"],
      ],
      quality: {
        parseErrors: 2,
        missingTables: 1,
        duplicates: 3,
        conflicts: 1,
        freshness: "reviewed 2026-11-10 · next 2027-05-10",
        verifier: "A. Mensah · Architecture Office",
        role: "source owner",
        approval: "capability owner + tenant admin",
      },
      passages: [
        {
          locator: "p.42 · §3.2",
          original:
            "Delegated authority must be named explicitly in the contract; authentication alone does not transfer authority.",
          extracted:
            "Delegated authority must be named explicitly in the contract. Authentication alone does not transfer authority.",
          reviewed: true,
          enabled: true,
          score: "0.93",
        },
        {
          locator: "p.43 · table 4",
          original:
            "Scope | Owner | Evidence\nDelivery | Architecture Office | ADR-041\nVerification | Assurance | E-17",
          extracted: "[table omitted by the parser]",
          reviewed: false,
          enabled: false,
          score: "0.00",
        },
        {
          locator: "p.44 · §3.4",
          original:
            "An approved exception is recorded in the architecture register and reviewed at the next gate.",
          extracted:
            "An approved exception is recorded in the architecture register and reviewed at the next gate.",
          reviewed: true,
          enabled: true,
          score: "0.88",
        },
      ],
      usedBy: [
        ["Job", "software.delivery.backend_engineer"],
        ["Procedure", "proc-implement@7 · step 01"],
        ["Binding", "bind-backend · capability@17"],
        ["Runs", "RUN-8821 · RUN-8840"],
      ],
    },
    "service-catalogue": {
      states: [
        ["Imported", "done", "ING-029 · source_run queued"],
        ["Extracted", "warn", "124 units · 4 parse errors · contract tables pending"],
        ["Reviewed", "todo", "no person review yet"],
        ["Published", "todo", "not in a pack release"],
        ["Eligible", "todo", "not selected by a job pin"],
        ["Retrieved", "todo", "not retrievable before publication"],
        ["Included", "todo", "not in a context manifest"],
      ],
      quality: {
        parseErrors: 4,
        missingTables: 2,
        duplicates: 0,
        conflicts: 2,
        freshness: "acquired 2026-11-10 · review not started",
        verifier: "unassigned",
        role: "source owner",
        approval: "capability owner + tenant admin",
      },
      passages: [
        {
          locator: "p.12 · service table",
          original:
            "Service | Owner | Contract version\nidentity-api | Identity | v3\nbilling-api | Finance | v2",
          extracted: "[table extracted with two columns]",
          reviewed: false,
          enabled: false,
          score: "0.00",
        },
      ],
      usedBy: [["Job", "not bound"], ["Procedure", "not referenced"], ["Binding", "none"]],
    },
    "assurance-handbook": {
      states: [
        ["Imported", "done", "ING-030 · imported document registered"],
        ["Extracted", "todo", "acquisition has not completed"],
        ["Reviewed", "todo", "no person review yet"],
        ["Published", "todo", "not in a pack release"],
        ["Eligible", "todo", "not selected by a job pin"],
        ["Retrieved", "todo", "not retrievable before publication"],
        ["Included", "todo", "not in a context manifest"],
      ],
      quality: {
        parseErrors: 0,
        missingTables: 0,
        duplicates: 0,
        conflicts: 0,
        freshness: "acquired 2026-11-10 · extraction pending",
        verifier: "unassigned",
        role: "source owner",
        approval: "capability owner + tenant admin",
      },
      passages: [],
      usedBy: [["Job", "not bound"], ["Procedure", "not referenced"], ["Binding", "none"]],
    },
  };
  const evidenceFor = (s) =>
    sourceEvidence[s.source] || {
      states: [
        ["Imported", "done", s.id + " · source_run queued"],
        ["Extracted", "todo", "not extracted"],
        ["Reviewed", "todo", "no person review yet"],
        ["Published", "todo", "not in a pack release"],
        ["Eligible", "todo", "not selected by a job pin"],
        ["Retrieved", "todo", "not retrievable before publication"],
        ["Included", "todo", "not in a context manifest"],
      ],
      quality: {
        parseErrors: 0,
        missingTables: 0,
        duplicates: 0,
        conflicts: 0,
        freshness: "queued 2026-11-10",
        verifier: "unassigned",
        role: "source owner",
        approval: "capability owner + tenant admin",
      },
      passages: [],
      usedBy: [["Job", "not bound"]],
    };
  const stages = [
    "Acquire",
    "Scan",
    "Detect",
    "Convert",
    "Enrich",
    "Bundle",
    "Deliver",
  ];
  const pins = (run = false) =>
    `<span class="e-pin">Type @6</span><span class="e-pin">Process @${run ? runProcessPin.version : 2}</span><span class="e-pin">Agent @${run ? runAgentPin.version : 2}</span><span class="e-pin">${run ? x.runPack : "capability@17"}</span><span class="e-pin">Baseline B${run ? x.runBaseline : x.baseline}</span>`;
  const progSelector = () =>
    `<div class="e-programmes">${programmeDefs.map((p) => `<button class="e-programme ${p.id === x.programme ? "selected" : ""}" data-e="programme" data-id="${p.id}" aria-pressed="${p.id === x.programme}"><span class="e-programme-top"><span class="mono">${p.key}</span>${B(p.tone === "green" ? "On track" : "Attention", p.tone)}</span><strong>${p.name}</strong><small>${db.projects.filter((v) => v.programme === p.id).length} projects · ${p.horizon}</small></button>`).join("")}</div>`;
  const scopebar = () =>
    `<div class="e-scopebar"><div>${I("layers")}<strong>${selectedProgramme().name}</strong>${B("Programme", "green")}<span class="muted">${selectedProgramme().horizon}</span></div><div>${eb("Charter & outcomes", "charter", "", "quiet small", "file")}${eb("Baseline B" + x.baseline, "baseline", "", "small", "shield")}</div></div>`;
  function sidebar() {
    return `<div class="brand"><span class="brand-mark">${I("layers")}</span><div><div class="brand-word">conveyor</div><div class="brand-sub">THE OPERATING SYSTEM FOR WORK</div></div></div><div class="tenant-switch"><span class="tenant-mark">N</span><span class="grow">Non-Agentic AI<small>Enterprise workspace</small></span><span>⌄</span></div><nav>
      <div class="nav-group"><div class="nav-label">COMMAND</div>${N("overview", "Programme control", "grid")}${N("projects", "Portfolio & projects", "projects", db.projects.length)}${N("work", "Work breakdown", "board")}${N("execution", "Execution room", "run")}${N("decisions", "Decision inbox", "inbox", db.decisions.filter((d) => d.state === "Pending").length)}</div>
      <div class="nav-group"><div class="nav-label">GOVERNANCE & ASSURANCE</div>${N("assurance", "Assurance & baselines", "shield")}${N("constraints", "Constraints & TOC", "branch")}${N("requirements", "Requirements", "file")}${N("principles", "Principles", "book")}${N("models", "Architecture & UML", "model")}</div>
      <div class="nav-group"><div class="nav-label">DESIGN THE OPERATING MODEL</div>${N("types", "Project types", "layers")}${N("capabilities", "Capabilities", "grid")}${N("jobtypes", "Agent job types", "board", jobs.length)}${N("processes", "Processes", "flow")}${N("agents", "Agents", "agent")}${N("tools", "Tools & actions", "tools")}</div>
      <div class="nav-group"><div class="nav-label">KNOWLEDGE & MEMORY</div>${N("knowledge-design", "Knowledge design", "branch")}${N("knowledge", "Sources & retrieval", "book")}${N("memory", "Memory explorer", "layers")}</div>
      <div class="nav-group"><div class="nav-label">MEASURE & IMPROVE</div>${N("improvement", "Improvement control", "chart")}${N("evals", "Evaluation studio", "shield")}${N("hillclimb", "Hillclimb lab", "branch")}${N("benchmarks", "Benchmark harness", "code")}${N("usage", "Inference & prompt audit", "chart")}${N("runs", "Run history", "clock")}</div></nav><div class="sidebar-bottom"><b>Intent → evidence → improvement</b><div class="row"><i class="dot green"></i>Local interaction study</div></div>`;
  }
  function metrics(all = false) {
    const pp = all ? db.projects : scopeProjects();
    const vals = [
      [
        fmt(pp.length),
        "Projects in flight",
        all ? "Across 3 programmes" : "Within this programme",
      ],
      [
        fmt(pp.reduce((a, p) => a + p.total, 0)),
        "Planned work packages",
        "Summary roll-up · sample data",
      ],
      [
        pp.reduce((a, p) => a + (p.live || 0), 0),
        "Agent jobs in flight",
        "Independent, bounded executions",
      ],
      [
        pp.reduce((a, p) => a + (p.blocks || 0), 0),
        "Open blockers",
        "Knowledge, evidence & decisions",
      ],
    ];
    return `<div class="e-metrics">${vals.map(([v, l, s], i) => `<div><span class="label">${l}</span><strong class="${i === 3 ? "e-amber" : ""}">${v}</strong><small>${s}</small></div>`).join("")}</div>`;
  }
  function overview() {
    return `<div class="page enterprise">${heading("Programme control", "Steer outcomes, resolve constraints and govern the work at every level.", `${route("Decision inbox", "decisions", "", "inbox")}${eb("Review programme", "charter", "", "primary", "shield")}`)}${progSelector()}${scopebar()}${metrics()}${tabs(["Delivery map", "Integrated plan", "Gate readiness"], x.tab, "programme-tab")}${x.tab === "Delivery map" ? deliveryMap() : x.tab === "Integrated plan" ? integratedPlan() : gateReadiness()}<div class="e-provenance"><span><i class="dot green"></i> Sample programme snapshot · 10 Nov 2026, 14:32</span><span>Portfolio → programme → project → work package → run</span></div></div>`;
  }
  function projectRows(pp) {
    return `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Project / workstream</th><th>Lifecycle</th><th>Delivery</th><th>Live</th><th>Blockers</th></tr></thead><tbody>${pp.map((p) => `<tr class="${x.project === p.id ? "selected" : ""}"><td><button class="e-project-link" data-e="select-project" data-id="${p.id}"><span class="e-project-glyph">${I(p.symbol)}</span><span><strong>${E(p.name)}</strong><small>${E(p.key)} · ${E(p.stream || "New workstream")}</small></span></button></td><td>${E(p.enterprisePhase || "Initiate")}</td><td><div class="e-progress"><i style="width:${Math.round((p.completed / p.total) * 100)}%"></i></div><small>${fmt(p.completed)} / ${fmt(p.total)}</small></td><td><span class="e-live">${p.live || 0}</span></td><td>${p.blocks ? B(p.blocks + " open", "amber") : B("Clear", "green")}</td></tr>`).join("")}</tbody></table></div>`;
  }
  function deliveryMap() {
    const p = project(),
      rows = packages.filter((w) => w.project === p.id);
    return `<div class="e-main-grid"><div class="e-stack">${panel("Workstreams & projects", projectRows(scopeProjects()), `<span class="mono muted">${scopeProjects().length} projects</span>`)}${panel("Cross-project dependencies", `<div class="e-dependencies"><div><span class="e-number">01</span><div><strong>${x.programme === "civic" ? "Trust & identity" : x.programme === "resilience" ? "Zero-trust platform" : "Enterprise knowledge fabric"}</strong><small>Target architecture contract</small></div><span class="e-dep-arrow">→</span><div><strong>${x.programme === "civic" ? "Common data foundation" : x.programme === "resilience" ? "Cloud landing zones" : "Agent operating model"}</strong><small>${x.programme === "civic" ? "Identifier & ownership model" : "Approved dependency contract"}</small></div>${B("Critical chain", "amber")}</div><div><span class="e-number">02</span><div><strong>${x.programme === "civic" ? "Common data foundation" : x.programme === "resilience" ? "Cloud landing zones" : "Agent operating model"}</strong><small>Contract baseline</small></div><span class="e-dep-arrow">→</span><div><strong>${x.programme === "civic" ? "Service orchestration" : x.programme === "resilience" ? "Operational continuity" : "Usage insights"}</strong><small>Wave 2 integration gate</small></div>${B("12 days float")}</div></div>`, eb("Explore constraint", "goto-constraints", "", "quiet small", "arrow"))}</div><aside class="e-stack">${panel("Project focus", `<div class="e-focus"><span class="eyebrow">${E(p.key)} / ${E(p.stream || "")}</span><h2>${E(p.name)}</h2><p>${E(p.description)}</p><dl class="kv"><dt>Accountable</dt><dd>${p.id === "identity" ? "Enterprise architecture lead" : "Workstream lead"}</dd><dt>Current phase</dt><dd>${E(p.enterprisePhase || "Initiate")}</dd><dt>Published baseline</dt><dd>B${x.baseline} · scope, schedule & evidence</dd><dt>Next control point</dt><dd>Architecture contract review</dd></dl><div class="e-callout"><strong>${p.blocks || 0} blockers · ${p.live || 0} jobs in flight</strong><p>${p.id === "identity" ? "Unresolved delegation constraint is holding the architecture contract and two downstream workstreams." : "Inspect evidence and dependency readiness before releasing the next work package."}</p></div><div class="e-stack">${btn("Open project workbench", "open-project", `data-id="${p.id}"`, "primary", "arrow")}${eb("Inspect work breakdown", "project-work", `data-id="${p.id}"`, "", "board")}</div></div>`)}${panel("Next board decision", `<div class="e-focus"><span class="eyebrow e-amber">GATE G3 · ARCHITECTURE CONTRACT</span><h3>Accept the next architecture baseline?</h3><p>${p.blocks || 0} project blockers. Review model validation, requirements coverage and accountable sign-off before progression.</p><div class="e-pins">${pins()}</div>${route("Open assurance pack", "assurance", "small", "shield")}</div>`)}</aside></div>`;
  }
  function integratedPlan() {
    const pp = scopeProjects(),
      months = ["Nov 26", "Dec", "Jan 27", "Feb", "Mar", "Apr", "May", "Jun"];
    return `${panel("Integrated delivery plan", `<div class="e-plan-toolbar"><span>Approved baseline B${x.baseline} · forecast overlay</span>${eb("Propose baseline change", "change", "", "small", "edit")}</div><div class="e-table-scroll"><div class="e-plan"><div class="e-plan-head"><strong>Project / milestone</strong>${months.map((m) => `<span>${m}</span>`).join("")}</div>${pp.map((p, i) => `<div class="e-plan-row"><button data-e="select-plan" data-id="${p.id}"><strong>${E(p.name)}</strong><small>${i === 0 ? "◆ Critical chain" : "Baseline → forecast"}</small></button><div class="e-plan-track"><div class="e-plan-bar ${i === 0 ? "critical" : ""}" style="left:${i * 5}%;width:${56 - i * 3}%"><span>${i === 0 ? "Architecture contract → identity readiness" : "Baseline B" + x.baseline}</span></div><div class="e-plan-forecast" style="left:${i * 5 + 4}%;width:${56 - i * 3}%"></div><span class="e-milestone" style="left:${60 + i * 2}%" title="Human acceptance gate">◆</span></div></div>`).join("")}<div class="e-plan-legend"><span>━ Approved baseline</span><span>┄ Current forecast</span><span class="e-amber">◆ Human gate</span><span>Today: 10 Nov 2026</span></div></div></div>`)}<div class="e-two">${panel("Schedule assurance", `<div class="e-focus"><dl class="kv"><dt>Constraint</dt><dd>Identity architecture approval</dd><dt>Downstream exposure</dt><dd>Data contracts → service integration</dd><dt>Project buffer</dt><dd>8 of 15 days consumed</dd><dt>Forecast variance</dt><dd>+6 working days (sample)</dd></dl>${route("Inspect critical chain", "constraints", "small", "branch")}</div>`)}${panel("Controlled change", `<div class="e-focus"><h3>CR-017 · Re-sequence integration</h3><p>Move contract validation ahead of full migration. Review scope, schedule, cost, risk and requirements impact before approval.</p>${B(x.change, x.change === "Approved" ? "green" : "amber")}${eb("Open change request", "change", "", "small", "arrow")}</div>`)}</div>`;
  }
  function gateReadiness() {
    return `<div class="e-two">${panel(
      "Lifecycle gates",
      `<div class="e-gate-list">${[
        ["G1", "Charter & business case", "Accepted", "Sponsor"],
        [
          "G2",
          "Scope & requirements baseline",
          "Accepted",
          "Change control board",
        ],
        ["G3", "Target architecture contract", "Held", "Architecture board"],
        ["G4", "Transition & migration plan", "Upcoming", "Programme board"],
        ["G5", "Operational acceptance", "Upcoming", "Service owner"],
        ["G6", "Benefits realisation", "Upcoming", "Benefits owner"],
      ]
        .map(
          ([id, t, s, a]) =>
            `<button data-e="gate"><span class="e-gate-id">${id}</span><div><strong>${t}</strong><small>${a}</small></div>${B(s, s === "Accepted" ? "green" : s === "Held" ? "amber" : "")}</button>`,
        )
        .join("")}</div>`,
    )}${panel("G3 evidence checklist", readinessChecklist())}</div>`;
  }
  function readinessChecklist() {
    return `<div class="e-focus"><span class="eyebrow">TARGET ARCHITECTURE / BASELINE B${x.baseline}</span><h2>Evidence before progression</h2><div class="e-checklist">${[
      ["✓", "Approved business scope", "Charter C-001 @3"],
      ["✓", "Baseline & target views", "ArchiMate model @4 · UML @2"],
      ["!", "Requirements coverage", "2 requirements lack realisation traces"],
      ["!", "Principle conformance", "Delegation exception unresolved"],
      [
        x.packAvailable ? "✓" : "!",
        "Required domain knowledge",
        x.packAvailable
          ? "capability@17 · architecture filter"
          : "Pinned pack is unreachable",
      ],
      ["○", "Accountable human decision", "Architecture board · pending"],
    ]
      .map(
        ([v, l, d]) =>
          `<div><span class="${v === "!" ? "e-amber" : "e-green"}">${v}</span><div><strong>${l}</strong><small>${d}</small></div></div>`,
      )
      .join(
        "",
      )}</div>${route("Review traceability", "assurance", "primary", "shield")}</div>`;
  }
  function projects() {
    return `<div class="page enterprise">${heading("Portfolio & projects", "Concurrent programmes, independent project lifecycles and shared organisational capabilities.", btn("New project", "create-project", "", "primary", "plus"))}${metrics(true)}${programmeDefs.map((p) => panel(`${p.key} / ${p.name}`, projectRows(db.projects.filter((v) => v.programme === p.id)), eb("Programme control", "programme", `data-id="${p.id}"`, "small", "arrow"))).join("")}<p class="e-footnote">Project roll-ups are illustrative. Open a project to inspect its runnable sample workflow; detailed work-package records are a focused sample of the larger roll-up.</p></div>`;
  }
  function work() {
    const visible = packages.filter(
      (w) =>
        (x.workFilter === "All work" || w.project === x.workFilter) &&
        `${w.id} ${w.name} ${w.job}`
          .toLowerCase()
          .includes(x.workSearch.toLowerCase()),
    );
    return `<div class="page enterprise">${heading("Work breakdown", "A work package is the card. Its job type binds a capability, process, agent and acceptance contract.", eb("Create work package", "new-work", "", "primary", "plus"))}${scopebar()}<div class="e-hierarchy"><span>Programme</span> → <span>Project</span> → <span>WBS / deliverable</span> → <strong>Work package = card</strong> → <span>Runs / attempts</span></div><div class="toolbar"><label class="input-search">${I("search")}<input id="e-work-search" value="${E(x.workSearch)}" placeholder="Find work packages…" aria-label="Find work packages"></label><label class="e-select">Project <select id="e-work-filter"><option value="All work">All work</option>${db.projects.map((p) => `<option value="${p.id}" ${x.workFilter === p.id ? "selected" : ""}>${E(p.name)}</option>`).join("")}</select></label><span class="mono muted">${visible.length} detail records · focused sample</span></div>${workTree()}${panel("Work packages", `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>WBS / work package</th><th>Capability / agent job</th><th>State</th><th>Evidence</th><th>Dependency</th></tr></thead><tbody>${visible.map((w) => `<tr><td><button class="e-work-link" data-e="work-detail" data-id="${w.id}"><small>${w.wbs} · ${w.id} · ${E(nameOf("projects", w.project))}</small><strong>${E(w.name)}</strong></button></td><td><button class="text-button" data-e="job-open" data-id="${w.job}">${E(w.cap)} ↗</button><small class="e-code">${E(w.job)}</small></td><td>${B(w.state, w.state === "Blocked" ? "amber" : w.state === "Active" ? "green" : "purple")}</td><td>${E(w.evidence)}</td><td>${E(w.risk)}</td></tr>`).join("")}</tbody></table>${!visible.length ? empty("No work packages match this filter.") : ""}</div>`)}<div class="e-two">${panel("Progression contract", `<div class="e-focus"><h3>Readying → Doing → Inspecting → Releasing</h3><p>The historical list flows provide starting patterns. Each tenant authors job inputs, outputs, entry conditions, exit evidence and human gates in its own capability.</p>${route("Author agent job types", "jobtypes", "small", "board")}</div>`)}${panel("Keep three states visible", `<div class="e-focus"><dl class="kv"><dt>Project lifecycle</dt><dd>Target design</dd><dt>Work-package list</dt><dd>Review</dd><dt>Run / attempt</dt><dd>Waiting for human approval</dd></dl><p>A card move does not imply approval or change a programme baseline.</p></div>`)}</div></div>`;
  }
  function workTree() {
    return panel(
      "Programme work breakdown",
      `<div class="e-wbs-root"><span>${I("layers")}</span><div><strong>${selectedProgramme().key} · ${selectedProgramme().name}</strong><small>Approved WBS @7 · scope baseline B${x.baseline}</small></div>${B("Programme", "green")}</div><div class="e-wbs">${scopeProjects()
        .map((p, i) => {
          const detail = packages.filter((w) => w.project === p.id),
            open = x.expanded.has(p.id);
          return `<button class="e-wbs-project" data-e="wbs-toggle" data-id="${p.id}" aria-expanded="${open}"><span>${open ? "⌄" : "›"}</span><span class="mono">${String(i + 1).padStart(2, "0")}</span><div><strong>${E(p.name)}</strong><small>${E(p.stream || "New workstream")}</small></div><span>${fmt(p.total)} packages</span>${B(p.enterprisePhase || "Initiate")}</button>${open ? `<div class="e-wbs-branch"><div class="e-wbs-deliverable">${I("file")}<strong>Architecture, contracts & acceptance</strong><small>Deliverable / control account · ${detail.length} detailed sample packages</small></div>${detail.map((w) => `<button class="e-wbs-package" data-e="work-detail" data-id="${w.id}"><span>└</span><code>${w.wbs} / ${w.id}</code><strong>${E(w.name)}</strong>${B(w.state, w.state === "Blocked" ? "amber" : "green")}</button>`).join("") || '<p class="e-footnote">The roll-up includes this project. Detailed packages are outside the focused sample.</p>'}</div>` : ""}`;
        })
        .join("")}</div>`,
      eb("Create work package", "new-work", "", "small", "plus"),
    );
  }
  const jobCapability = (j) => capMap[j.capability] || j.capability;
  function jobTypes() {
    const filtered = jobs.filter(
      (j) =>
        (x.jobCapability === "all" || j.capability === x.jobCapability) &&
        `${j.title} ${j.id} ${j.capability} ${j.description}`
          .toLowerCase()
          .includes(x.jobQuery.toLowerCase()),
    );
    const selected =
      jobs.find((j) => j.id === x.selectedJob) || filtered[0] || jobs[0];
    const page = filtered.slice(x.jobPage * 18, x.jobPage * 18 + 18);
    return `<div class="page enterprise">${heading("Agent job types", "An extensible library of jobs inside capabilities. Historical card types are seeds; your operating model defines what comes next.", eb("Define job type", "edit-job", 'data-new="true"', "primary", "plus"))}<div class="e-hierarchy">${route("Project type", "types", "quiet small", "layers")} → ${route("Capability", "capabilities", "quiet small", "grid")} → <strong>Agent job type</strong> → ${route("Process", "processes", "quiet small", "flow")} → ${route("Agent & tools", "agents", "quiet small", "agent")}</div><div class="e-catalogue-meta"><strong>${jobs.length} job types</strong><span>${jobSource.jobs.length} historical definitions + ${jobs.length - jobSource.jobs.length} tenant extensions</span><span>Inputs · outputs · acceptance · process binding · grants · knowledge</span></div><div class="e-job-layout"><section class="e-panel e-job-list"><div class="e-job-filters"><label class="input-search">${I("search")}<input id="e-job-search" placeholder="Search names or original codes…" value="${E(x.jobQuery)}" aria-label="Search job types"></label><label class="e-select">Capability <select id="e-job-cap"><option value="all">All capabilities</option>${[
      ...new Set(jobs.map((j) => j.capability)),
    ]
      .sort()
      .map(
        (c) =>
          `<option value="${E(c)}" ${x.jobCapability === c ? "selected" : ""}>${E(c.replaceAll("_", " "))}</option>`,
      )
      .join(
        "",
      )}</select></label></div><div id="e-job-results">${page.map((j) => `<button class="e-job-row ${selected.id === j.id ? "selected" : ""}" data-e="job-select" data-id="${E(j.id)}"><strong>${E(j.title)}</strong><small>${E(j.capability.replaceAll("_", " "))}</small><span>${B(j.status, j.status === "Published" ? "green" : "")}<code>${E(j.id)}</code></span></button>`).join("") || empty("No matching job types.")}</div><div class="e-pagination">${eb("←", "job-page", 'data-dir="-1" aria-label="Previous page"' + (x.jobPage === 0 ? " disabled" : ""), "small")}<span>${filtered.length} matches · page ${x.jobPage + 1} / ${Math.max(1, Math.ceil(filtered.length / 18))}</span>${eb("→", "job-page", 'data-dir="1" aria-label="Next page"' + ((x.jobPage + 1) * 18 >= filtered.length ? " disabled" : ""), "small")}</div></section><section class="e-panel e-job-definition"><div class="e-panel-head"><span class="eyebrow">JOB DEFINITION / V${selected.version}</span>${eb("Edit definition", "edit-job", "", "small", "edit")}</div><div class="e-focus"><div class="row">${B(selected.status, selected.status === "Published" ? "green" : "amber")}${B(selected.origin.includes("Tenant") ? "Tenant authored" : "Historical seed")}</div><h2>${E(selected.title)}</h2><code class="e-code">${E(selected.id)}</code><p>${E(selected.description)}</p><div class="e-two e-contract"><section><h3>Input contract</h3>${listStrings(selected.inputs, "No explicit inputs in the source. Define them before publication.")}</section><section><h3>Expected outputs</h3>${listStrings(selected.outputs, "No explicit outputs in the source. Define evidence before publication.")}</section></div><dl class="kv"><dt>Capability</dt><dd>${E(selected.capability.replaceAll("_", " "))}</dd><dt>Process</dt><dd>${selected.process ? btn(nameOf("processes", selected.process) + " @" + byId("processes", selected.process)?.version, "open-definition", `data-kind="processes" data-id="${selected.process}"`, "quiet small", "flow") : B("Binding required", "amber")}</dd><dt>Executor</dt><dd>${selected.agent ? btn(nameOf("agents", selected.agent), "open-definition", `data-kind="agents" data-id="${selected.agent}"`, "quiet small", "agent") : B("Binding required", "amber")}</dd><dt>Domain knowledge</dt><dd>capability@17 · filter ${E(jobCapability(selected))}</dd><dt>Human authority</dt><dd>Named reviewer at the declared process gate</dd></dl><div class="e-contract"><h3>Entry conditions</h3><p>${E(selected.entry)}</p><h3>Exit / acceptance evidence</h3><p class="e-preline">${E(selected.exit)}</p></div><div class="e-pins">${pins()}</div><div class="e-action-bar">${eb("Check & publish draft", "publish-job", "", "", "shield")}${eb("Instantiate work package", "instantiate-job", "", "primary", "plus")}</div><div class="e-footnote">${E(selected.origin)} · Original codes are retained. Historical seed records require tenant review and explicit executable bindings.</div></div></section></div></div>`;
  }
  function listStrings(a, fallback) {
    return a?.length
      ? `<ul class="e-string-list">${a.map((s) => `<li>${E(typeof s === "string" ? s : JSON.stringify(s))}</li>`).join("")}</ul>`
      : `<p class="muted">${E(fallback)}</p>`;
  }
  function execution() {
    const j = runJobPin;
    const a = runAgentPin;
    const events = [
      [
        "14:31:02",
        "System",
        "Resolved published process, capability agent and knowledge pin.",
        "green",
      ],
      [
        "14:31:03",
        a.name,
        "knowledge.query · capability@17 · architecture · 3 cited hits",
        "green",
      ],
      [
        "14:31:04",
        a.name,
        "memory.recall · WP-2048 · 4 hot facts, 1 derived digest",
        "purple",
      ],
      [
        "14:31:11",
        "Coding service",
        "trigger_architecture_analysis → job status observed",
        "green",
      ],
      [
        "14:32:01",
        a.name,
        "Baseline model and code bindings attached. 2 unresolved requirement traces.",
        "green",
      ],
      [
        "14:32:08",
        "Gate",
        "Waiting for accountable architect. No worker held while waiting.",
        "amber",
      ],
      ...x.eventLog,
    ];
    return `<div class="page enterprise">${heading("Execution room", "Inspect one bounded job while the programme continues around it.", `${route("All runs", "runs", "", "clock")}${eb("Attempt start", "start-run", "", "primary", "run")}`)}<div class="e-run-strip"><div><span class="e-live-dot"></span><strong>${x.runState}</strong><small>RUN-8821 · WP-2048 · attempt ${x.runAttempt}</small></div><div><strong>${E(j.title)}</strong><small>Trust & identity / ${E(j.capability)}</small></div><div><strong>${E(a.name)}</strong><small>16 turns max · 300s timeout · explicit grants</small></div></div>${!x.packAvailable ? `<div class="e-blocker"><strong>Start refused: capability_not_ready</strong> Required capability@17 cannot resolve. No execution steps are created. ${route("Repair knowledge binding", "knowledge", "small", "book")}</div>` : ""}<div class="e-pins e-run-pins">${pins(true)}</div><div class="e-execution-grid"><div class="e-stack">${panel(
      "Published execution topology",
      `<div class="e-flow"><svg viewBox="0 0 810 205" role="img" aria-label="Resolve context, harvest baseline, validate traces, human architecture gate, then publish evidence"><path d="M130 80H200M330 80H400M530 80H600M465 112V164H665V112" fill="none" stroke="var(--line)" stroke-width="2" stroke-dasharray="5 4"/><text x="548" y="185" fill="var(--muted)" font-size="10">rework branch → next attempt</text></svg>${[
        ["01", "Context & memory", "Resolved", "book"],
        ["02", "Harvest baseline", "Evidence attached", "agent"],
        ["03", "Validate & inspect", "2 open traces", "model"],
        [
          "04",
          "Architecture gate",
          x.runState === "Completed" ? "Accepted" : "Human decision",
          "shield",
        ],
      ]
        .map(
          ([n, t, s, i], k) =>
            `<button class="e-flow-node ${k === 3 ? "gate" : ""}" style="left:${2 + k * 24.5}%" data-e="flow-step" data-index="${k}"><span>${I(i)}<small>${n}</small></span><strong>${t}</strong><small>${s}</small></button>`,
        )
        .join("")}</div>`,
    )}${tabs(["Activity", "Evidence", "Context & knowledge", "Permitted tools"], x.runTab, "run-tab")}${
      x.runTab === "Activity"
        ? panel(
            "Shared execution record",
            `<div class="e-transcript">${events.map(([time, actor, text, tone]) => `<article><span class="mono">${time}</span><div class="e-actor ${tone}">${E(actor.slice(0, 2).toUpperCase())}</div><div><strong>${E(actor)}</strong><p>${E(text)}</p></div></article>`).join("")}</div><form id="e-operator-note" class="e-note-form"><label for="e-note">Record operator guidance</label><div><input id="e-note" name="note" required placeholder="Add a note to this work package…"><button class="btn primary">Record note ↗</button></div><small>Guidance is recorded as project data; it does not rewrite the published procedure.</small></form>`,
          )
        : x.runTab === "Evidence"
          ? panel(
              "Inputs → outputs → verification",
              `<div class="e-focus"><div class="e-two">${[
                [
                  "Inputs",
                  "Repository snapshot · 3f80a2",
                  "Scope baseline B3",
                  "capability@17 · citations 1–3",
                ],
                [
                  "Outputs",
                  "ArchiMate baseline @4",
                  "Code binding report · 36 components",
                  "Validation report · 2 findings",
                ],
              ]
                .map(
                  ([title, ...items]) =>
                    `<section><h3>${title}</h3>${listStrings(items)}</section>`,
                )
                .join(
                  "",
                )}</div><div class="e-evidence-chain"><span>Producer result</span>→<span>Observer evidence</span>→<span>Independent assessment</span>→<strong>Human decision</strong></div>${route("Open models & views", "models", "small", "model")}${route("Review traceability", "assurance", "small", "shield")}</div>`,
            )
          : x.runTab === "Context & knowledge"
            ? contextContents()
            : panel(
                "Explicit action grants",
                `<div class="e-focus">${a.tools.map((t) => btn(t, "open-tool", `data-id="${E(t)}"`, "small", "tools")).join(" ")}${note("Every tool invocation is traced. Registration does not grant an action; an out-of-scope action is refused before execution.")}</div>`,
              )
    }</div><aside class="e-stack">${panel("Human control point", `<div class="e-focus"><span class="eyebrow e-amber">ARCHITECTURE REVIEW</span><h2>Review the baseline</h2><p>2 missing requirement traces remain. Inspect the evidence and record a reason for the decision.</p><dl class="kv"><dt>Accountable</dt><dd>Enterprise architect</dd><dt>Impact</dt><dd>Data and service workstreams</dd><dt>Authority</dt><dd>George · person · Web</dd></dl>${eb(x.reviewDecision ? "View recorded decision" : "Review gate evidence", "run-review", "", "primary", "shield")}${x.reviewDecision ? `<div class="e-callout">${E(x.reviewDecision)}</div>` : ""}</div>`)}${panel("Context readiness", `<div class="e-focus"><dl class="kv"><dt>Required pack</dt><dd>${B(x.packAvailable ? "Available · @17" : "Unreachable", x.packAvailable ? "green" : "amber")}</dd><dt>Model / recipe</dt><dd>${E(a.model)} / ${E(a.prompt)}</dd><dt>Hot memory</dt><dd>WP-2048 only</dd><dt>Prompt accounting</dt><dd>8,160 / 12,000 characters (fixture)</dd><dt>Compiled identity</dt><dd><code>sha256:a7c9…e20f</code></dd></dl>${route("Inspect knowledge plane", "knowledge", "small", "book")}</div>`)}${panel("Other work continues", `<div class="e-focus"><p><strong>${db.projects.reduce((s, p) => s + (p.live || 0), 0)} agent jobs</strong> across ${db.projects.length} projects. A parked human gate releases its worker.</p>${route("Programme control", "overview", "quiet small", "arrow")}</div>`)}</aside></div></div>`;
  }
  function contextContents() {
    return panel(
      "Context compiled for this call",
      `<div class="e-focus"><div class="e-budget"><span style="width:17%">Instructions</span><span style="width:24%">Procedure</span><span style="width:23%">Role</span><span style="width:20%">Knowledge</span><span style="width:16%">Card</span></div><div class="e-checklist">${[
        [
          "01",
          "Instructions & trust rules",
          "Published recipe · highest trust",
        ],
        ["02", "Procedure", "Design architecture @2 · current step & branch"],
        [
          "03",
          "Capability / role",
          "Solution architect @2 · granted actions & limits",
        ],
        [
          "04",
          "Domain knowledge",
          "knowledge.query → capability@17 · metadata.capability=architecture",
        ],
        [
          "05",
          "Project / card / memory",
          "WP-2048 · fresh transactional facts; tenant content treated as data",
        ],
      ]
        .map(
          ([n, t, s]) =>
            `<div><span class="mono">${n}</span><div><strong>${t}</strong><small>${s}</small></div></div>`,
        )
        .join(
          "",
        )}</div>${note("Select → fetch → render → budget → hash & record. Source versions are pinned; exceeding the context budget refuses the call with a measurement. Hot memory wins over a derived digest.")}${eb("Preview source manifest", "context-manifest", "", "small", "file")}</div>`,
    );
  }
  function assurance() {
    return `<div class="page enterprise">${heading("Assurance & baselines", "Keep intent, architecture, delivery evidence and accountable decisions connected.", `${eb("Propose change", "change", "", "", "edit")}${eb("Export assurance pack", "export-assurance", "", "primary", "download")}`)}${scopebar()}<div class="e-assurance-ribbon"><span><strong>TOGAF</strong> Architecture & migration governance</span><span><strong>PMP</strong> Scope, schedule, cost & change control</span><span><strong>Requirements</strong> Baselines, acceptance & traceability</span><span><strong>Principles</strong> Conformance & exceptions</span></div>${tabs(["Traceability", "Architecture cycle", "Control registers", "Baselines & changes"], x.assurance, "assurance-tab")}${x.assurance === "Traceability" ? traceability() : x.assurance === "Architecture cycle" ? architectureCycle() : x.assurance === "Control registers" ? registers() : baselineView()}<p class="e-footnote">Method controls are an interaction proposal. These sample checks do not establish standards conformance or production validation.</p></div>`;
  }
  function traceability() {
    return `<div class="e-main-grid"><div class="e-stack">${panel(
      "Intent → realisation → acceptance",
      `<div class="e-trace-path">${[
        ["Goal", "Reduce fulfilment time 40%"],
        ["Principle", "Tenant isolation @4"],
        ["Requirement", "Bound delegation @1"],
        ["Architecture", "Identity service landscape"],
        ["Work package", "WP-2048 → WP-2049"],
        ["Evidence", "Validation & human decision"],
      ]
        .map(
          ([t, l], i) =>
            `<button data-e="trace-node" data-index="${i}"><small>${t}</small><strong>${l}</strong><span>→</span></button>`,
        )
        .join("")}</div>`,
    )}${panel(
      "Requirement coverage",
      `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Requirement</th><th>Principle</th><th>Architecture</th><th>Work / evidence</th><th>Readiness</th></tr></thead><tbody>${db.requirements
        .map(
          (r, i) =>
            `<tr><td>${btn(r.key + " · " + r.name, "open-definition", `data-kind="requirements" data-id="${r.id}"`, "quiet small")}</td><td>${E(nameOf("principles", r.principle))}</td><td>${db.models.some((m) => m.elements.some((e) => e.asset === r.id)) ? "Model element linked" : "Missing realisation"}</td><td>${E(
              packages
                .filter((w) => w.requirement === r.id)
                .map((w) => w.id + " · " + w.state)
                .join(", ") || "Trace review required",
            )}</td><td>${B(r.status, r.status === "Approved" ? "green" : "amber")}</td></tr>`,
        )
        .join("")}</tbody></table></div>`,
    )}${panel("Evidence ownership", `<div class="e-focus"><p>Principles and requirements own their revisions and approval lifecycles. ArchiMate/UML elements reference the governed asset; a view contains occurrences of model elements.</p><div class="row">${route("Requirements CRUD", "requirements", "small", "file")}${route("Principles CRUD", "principles", "small", "shield")}${route("Models & views", "models", "small", "model")}</div></div>`)}</div><aside>${panel("Open findings", `<div class="e-focus"><div class="e-finding"><span>F-012 / HIGH</span><h3>Delegation constraint unresolved</h3><p>Target service boundary must demonstrate permitted delegation before the architecture contract is accepted.</p>${btn("Open requirement", "open-definition", 'data-kind="requirements" data-id="delegation"', "small", "arrow")}</div><div class="e-finding"><span>F-013 / MEDIUM</span><h3>Two realisation links missing</h3><p>Map approved requirements to target elements and work-package acceptance evidence.</p>${route("Open modelling workspace", "models", "small", "model")}</div><div class="e-finding"><span>CONTROL / REQUIRED</span><h3>Source freshness & knowledge pin</h3><p>capability@17 is pinned. Tenant supersession carries source and licence provenance.</p>${route("Inspect knowledge", "knowledge", "small", "book")}</div></div>`)}</aside></div>`;
  }
  function architectureCycle() {
    const phases = [
      ["Preliminary", "Principles, governance & capability", "Approved"],
      [
        "A · Architecture vision",
        "Scope, stakeholders & business case",
        "Approved",
      ],
      [
        "B · Business architecture",
        "Baseline / target business views",
        "Approved",
      ],
      [
        "C · Information systems",
        "Data and application architecture",
        "In review",
      ],
      [
        "D · Technology architecture",
        "Technology standards & target platform",
        "In review",
      ],
      [
        "E · Opportunities & solutions",
        "Options, gaps & transition plateaus",
        "Draft",
      ],
      [
        "F · Migration planning",
        "Work packages, dependencies & roadmap",
        "Draft",
      ],
      [
        "G · Implementation governance",
        "Architecture contract & compliance",
        "Upcoming",
      ],
      ["H · Architecture change", "Monitor change & benefits", "Continuous"],
    ];
    return `<div class="e-main-grid">${panel("Architecture development cycle", `<div class="e-cycle"><div class="e-cycle-core">Requirements management<span>Continuous across every phase</span></div>${phases.map(([p, d, s]) => `<button data-e="adm-phase" data-phase="${E(p)}"><div><strong>${p}</strong><small>${d}</small></div>${B(s, s === "Approved" ? "green" : s === "In review" ? "amber" : "")}</button>`).join("")}</div>`)}<div class="e-stack">${panel("Architecture repository", `<div class="e-focus"><h3>Baseline → target → transition</h3><p>One semantic model with purpose-specific views, explicit gaps and governed architecture decisions.</p><div class="e-checklist"><div>ArchiMate<span>Business, application, technology & motivation</span></div><div>UML<span>Class, component, use-case & sequence views</span></div><div>Decision record<span>Options, rationale, evidence & consequences</span></div></div>${route("Create model or view", "models", "primary", "model")}</div>`)}${readinessChecklist()}</div></div>`;
  }
  const controlRows = [
    [
      "R-014",
      "Risk",
      "Identity contract delays integration",
      "Architecture lead",
      "High",
      "Mitigate",
    ],
    [
      "I-009",
      "Issue",
      "Missing delegation acceptance evidence",
      "Requirements lead",
      "High",
      "Open",
    ],
    [
      "D-021",
      "Dependency",
      "Data contracts require identity baseline",
      "Data lead",
      "Critical",
      "Waiting",
    ],
    [
      "A-006",
      "Assumption",
      "Legacy identifier mapping is complete",
      "Migration lead",
      "Medium",
      "Unverified",
    ],
    [
      "B-003",
      "Benefit",
      "40% reduction in fulfilment lead time",
      "Benefits owner",
      "Target",
      "Measure after release",
    ],
  ];
  function registers() {
    return panel(
      "Programme control registers",
      `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Record</th><th>Type</th><th>Statement</th><th>Accountable</th><th>Exposure</th><th>Status</th></tr></thead><tbody>${controlRows.map(([id, t, s, a, e, status]) => `<tr><td>${eb(id, "control-record", `data-id="${id}"`, "quiet small")}</td><td>${t}</td><td>${s}</td><td>${a}</td><td>${B(e, e === "High" || e === "Critical" ? "amber" : "")}</td><td>${status}</td></tr>`).join("")}</tbody></table></div>${note("Registers are a proposed programme control surface. Earlier measured scope removed dedicated register/deliverable APIs; this mockup does not assert those endpoints exist.")}`,
    );
  }
  function baselineView() {
    return `<div class="e-two">${panel("Approved baseline", `<div class="e-focus"><span class="eyebrow">B${x.baseline} / APPROVED SNAPSHOT</span><h2>Programme delivery baseline</h2><dl class="kv"><dt>Scope</dt><dd>Charter @3 · WBS @7 · Requirements baseline @4</dd><dt>Schedule</dt><dd>Integrated plan @6 · dependency network @3</dd><dt>Cost</dt><dd>Programme envelope ${selectedProgramme().budget}</dd><dt>Architecture</dt><dd>Baseline @4 · target @2 · transition @1</dd><dt>Approval</dt><dd>Programme board · person · recorded decision</dd></dl>${note("Active runs retain their published definition and knowledge pins. An approved programme change creates a new baseline for subsequent work; it does not rewrite past executions.")}${eb("View baseline manifest", "baseline", "", "small", "file")}</div>`)}${panel("CR-017 / Change control", `<div class="e-focus">${B(x.change, x.change === "Approved" ? "green" : "amber")}<h2>Re-sequence contract validation</h2><p>Bring data contract validation forward to protect the identity critical chain.</p><dl class="kv"><dt>Scope impact</dt><dd>2 work packages split; no scope removed</dd><dt>Schedule impact</dt><dd>Recover 6 working days (sample estimate)</dd><dt>Cost impact</dt><dd>+€18,000 forecast (sample estimate)</dd><dt>Risk impact</dt><dd>Earlier uncertainty retirement; added coordination</dd><dt>Required approvals</dt><dd>Architecture lead + change control board</dd></dl>${eb("Review proposed change", "change", "", "primary", "shield")}</div>`)}</div>`;
  }
  const tocNodes = [
    {
      id: 0,
      title: "Late service integration",
      kind: "Undesirable effect",
      detail:
        "Service integration is waiting for stable data contracts. Observed in 3 work packages; evidence must be checked before treating this as causality.",
    },
    {
      id: 1,
      title: "Identity contract unresolved",
      kind: "Current constraint",
      detail:
        "Delegation acceptance is missing. The architecture contract holds a dependency used by both data and service workstreams.",
    },
    {
      id: 2,
      title: "Requirements lack realisation traces",
      kind: "Contributing cause",
      detail:
        "Two requirements have no accepted realisation link. Close these traces and attach evidence before the target-state gate.",
    },
    {
      id: 3,
      title: "Approval batches too large",
      kind: "Policy assumption",
      detail:
        "The whole architecture pack is reviewed as one batch. Validate whether smaller independently reviewable slices preserve governance.",
    },
    {
      id: 4,
      title: "Review a bounded contract slice",
      kind: "Proposed injection",
      detail:
        "Review the delegation contract and its tests first. Seek a controlled baseline change before re-sequencing downstream work.",
    },
  ];
  function constraints() {
    const n = tocNodes[x.tocNode];
    const future = x.toc === "Future reality",
      pre = x.toc === "Prerequisite tree";
    return `<div class="page enterprise">${heading("Constraints & TOC", "Theory of Constraints: make causal assumptions, injections and delivery constraints inspectable.", eb("Record an injection", "toc-injection", "", "primary", "plus"))}${scopebar()}${tabs(["Current reality", "Future reality", "Prerequisite tree", "Critical chain"], x.toc, "toc-tab")}${x.toc === "Critical chain" ? integratedPlan() : `<div class="e-main-grid">${panel(x.toc + " tree", `<div class="e-toc-legend"><span>Click a node to inspect evidence</span><span>Arrows express proposed causality</span></div><div class="e-toc"><svg viewBox="0 0 780 480" role="img" aria-label="Causal tree linking missing requirement traces and approval batch policy to the identity contract constraint and late integration"><defs><marker id="toc-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="var(--muted)"/></marker></defs><path d="M390 313V276H185V235M390 276H590V235M185 165V116H390V91M590 165V116H390" fill="none" stroke="var(--muted)" stroke-width="1.5" marker-end="url(#toc-arrow)"/><text x="360" y="263" font-size="10" fill="var(--muted)">AND / assumption</text></svg>${[0, 1, 2, 3].map((id, i) => `<button class="e-toc-node ${id === x.tocNode ? "selected" : ""} ${id === 1 ? "constraint" : ""}" style="--nx:${[34, 8, 61, 34][i]}%;--ny:${[3, 34, 34, 65][i]}%" data-e="toc-node" data-id="${id}"><small>${future ? ["Desired effect", "Injection", "Necessary outcome", "Condition"][i] : pre ? ["Objective", "Intermediate objective", "Obstacle", "Prerequisite"][i] : tocNodes[id].kind}</small><strong>${future ? ["Predictable integration delivery", "Review a bounded contract slice", "Accepted requirement realisation", "Independent evidence per slice"][i] : pre ? ["Release service integration", "Accept identity contract", "Missing acceptance evidence", "Assign reviewer & test slice"][i] : tocNodes[id].title}</strong><span>${id === 1 ? "3 downstream packages" : "Evidence & assumptions ↗"}</span></button>`).join("")}</div>`)}<aside class="e-stack">${panel("Constraint inspector", `<div class="e-focus">${B(n.kind, "amber")}<h2>${n.title}</h2><p>${n.detail}</p><dl class="kv"><dt>Owner</dt><dd>Enterprise architecture lead</dd><dt>Evidence</dt><dd>WP-2048 · F-012 · REQ-061</dd><dt>Validation</dt><dd>Hypothesis; review causal sufficiency</dd><dt>Next action</dt><dd>Exploit → subordinate → elevate</dd></dl>${eb("Propose recovery injection", "toc-injection", "", "primary", "branch")}${route("Review assurance evidence", "assurance", "small", "shield")}</div>`)}${panel("Five focusing steps", `<ol class="e-focus e-focusing"><li><strong>Identify</strong> the current constraint</li><li><strong>Exploit</strong> available review capacity</li><li><strong>Subordinate</strong> upstream WIP to that constraint</li><li><strong>Elevate</strong> through an approved change</li><li><strong>Repeat</strong> after the constraint moves</li></ol>`)}</aside></div>`}<p class="e-footnote">Thinking-process graphs and critical-chain controls are design proposals. The causal links and buffer values are sample hypotheses, not computed scheduling results.</p></div>`;
  }
  function knowledge() {
    return `<div class="page enterprise">${heading("Knowledge & memory", "From governed sources to cited decisions. Every capability knows what it can use, and every run records what it used.", `${eb("Add source", "new-source", "", "", "plus")}${eb("Inspect readiness", "knowledge-readiness", "", "primary", "shield")}`)}<div class="e-knowledge-map">${[
      [
        "Procedural knowledge",
        "Published process → steps → actions / gates",
        "Conveyor",
        "processes",
      ],
      [
        "Domain knowledge",
        "Published pack → version → citations",
        "strata_rag",
        "Packs & bindings",
      ],
      [
        "Procedural memory",
        "Runs → invocations → approvals → events",
        "Conveyor",
        "runs",
      ],
      [
        "Work-package memory",
        "Hot transactional facts → cold derived digest",
        "Conveyor + strata_rag",
        "Work-package memory",
      ],
    ]
      .map(
        ([t, d, o, p]) =>
          `<button data-e="knowledge-map" data-target="${p}"><span>${o}</span><strong>${t}</strong><small>${d}</small></button>`,
      )
      .join(
        "",
      )}</div>${tabs(["Sources & ingestion", "Packs & bindings", "Retrieval & citations", "Work-package memory", "Context compiler"], x.knowledge, "knowledge-tab")}${x.knowledge === "Sources & ingestion" ? ingestion() : x.knowledge === "Packs & bindings" ? packsView() : x.knowledge === "Retrieval & citations" ? retrieval() : x.knowledge === "Work-package memory" ? memory() : contextContents()}<p class="e-footnote">Service states, results and documents are sample data. This offline study makes no calls to strata_ingest, strata_rag or a model provider.</p></div>`;
  }
  function ingestionBase() {
    const s = sourceJobs[x.sourceIndex] || sourceJobs[0];
    return `<div class="e-main-grid"><div class="e-stack">${panel("Source intake", `<div class="e-service-route"><span>Document / URL / API</span>→<strong>strata_ingest</strong>→<strong>strata_rag</strong>→<span>Capability-bound retrieval</span></div><div class="e-source-list">${sourceJobs.map((s, i) => `<button class="${x.sourceIndex === i ? "selected" : ""}" data-e="source-select" data-index="${i}"><span class="e-project-glyph">${I("file")}</span><div><strong>${E(s.title)}</strong><small>${E(s.kind)} · ${E(s.source)} · ${s.pages} source units</small></div>${B(s.status, s.step === 7 ? "green" : "purple")}</button>`).join("")}</div>`, eb("Add source", "new-source", "", "small", "plus"))}${panel("Ingestion pipeline", `<div class="e-focus"><div class="between"><span class="mono">${E(s.id)} / strata_ingest</span>${eb(s.step === 7 ? "Replay as new source run" : "Advance sample job", "ingest-step", "", "small", "run")}</div><div class="e-ingest-stages">${stages.map((t, i) => `<div class="${i < s.step ? "done" : i === s.step ? "current" : ""}"><span>${i < s.step ? "✓" : i + 1}</span><strong>${t}</strong></div>`).join("")}</div><dl class="kv"><dt>Target leaf store</dt><dd><code>${E(s.store)}</code></dd><dt>Source identity</dt><dd><code>${E(s.source)}</code> / source_run ${E(s.id)}</dd><dt>Document handoff</dt><dd>documents.ingest → external_ref: ${E(s.id)}</dd><dt>Delivery result</dt><dd>${s.step === 7 ? "Bundle delivered; source document IDs available" : "Pending bundle delivery; not retrievable yet"}</dd></dl>${note("Upload requires an explicit tenant leaf store. A successful ingestion job makes documents available; publishing an immutable platform pack is a separate governance action.")}</div>`)}</div><aside class="e-stack">${panel("Provenance & policy", `<div class="e-focus"><span class="eyebrow">SOURCE RECORD</span><h2>${E(s.title)}</h2><dl class="kv"><dt>Origin</dt><dd>${E(s.provenance)}</dd><dt>Licence</dt><dd>${E(s.licence)}</dd><dt>Retrieved</dt><dd>${E(s.date)}</dd><dt>Capability metadata</dt><dd><code>${E(s.cap)}</code></dd><dt>Supersedes</dt><dd>${s.supersedes ? E(s.supersedes) : "None declared"}</dd><dt>Scope</dt><dd>Tenant-owned source documents</dd></dl>${eb("Inspect ingest contract", "ingest-contract", "", "small", "code")}</div>`)}${panel("Readiness is explicit", `<div class="e-focus"><h3>${x.packAvailable ? "Required knowledge resolves" : "Required knowledge unavailable"}</h3><p>A missing required pin blocks a run before its first step. Loss after start yields knowledge_degraded; optional sources may return no hits with a warning.</p>${eb(x.packAvailable ? "Simulate required-pack outage" : "Restore required pack", "pack-outage", "", "small", "replay")}</div>`)}</aside></div>`;
  }
  function ingestion() {
    return (
      ingestionBase() +
      ingestionWorkbench(sourceJobs[x.sourceIndex] || sourceJobs[0])
    );
  }
  function ingestionWorkbench(s) {
    const ev = evidenceFor(s);
    const p = ev.passages[x.passageIndex] || ev.passages[0];
    const stateBadge = (status) =>
      status === "done"
        ? B("done", "green")
        : status === "warn"
          ? B("needs attention", "amber")
          : B("not yet");
    const passageList = ev.passages.length
      ? `<div class="e-source-passages">${ev.passages
          .map(
            (p, i) =>
              `<button class="${i === x.passageIndex ? "selected" : ""}" data-e="passage-select" data-index="${i}"><span class="mono">${E(p.locator)}</span><span>${p.enabled ? I("check") : I("close")} ${E(p.extracted.slice(0, 64))}${p.extracted.length > 64 ? "…" : ""}</span></button>`,
          )
          .join("")}</div>`
      : `<div class="empty">No extracted passages. Extraction has not completed, so there is nothing to review or retrieve.</div>`;
    return `<div class="e-main-grid"><div class="e-stack">${panel(
      "Transformation chain · source to the call",
      `<div class="e-service-route e-source-chain"><span>Original document</span>→<span>Extracted content</span>→<span>Passages / entities</span>→<span>Reviewed knowledge</span>→<span>Published pack</span>→<span>Consuming jobs</span></div>${note("Each step is a separate achievement. An imported source is not necessarily extracted correctly; a published source is not necessarily eligible for this job.")}`,
      B(s.id, "purple"),
    )}${panel(
      "Seven distinct states",
      `<div class="e-table-scroll"><table class="e-table e-source-states"><thead><tr><th>State</th><th>Result</th><th>Evidence</th></tr></thead><tbody>${ev.states
        .map(
          ([state, status, evidence]) =>
            `<tr><td>${E(state)}</td><td>${stateBadge(status)}</td><td><small>${E(evidence)}</small></td></tr>`,
        )
        .join("")}</tbody></table></div>`,
    )}${panel(
      "Passage inspector",
      `${passageList}${
        p
          ? `<div class="e-two e-source-compare"><section><h3>Original · ${E(p.locator)}</h3><pre class="e-json">${E(p.original)}</pre></section><section><h3>Extracted passage</h3><pre class="e-json">${E(p.extracted)}</pre></section></div><dl class="kv"><dt>Locator</dt><dd><code>${E(p.locator)}</code></dd><dt>Review state</dt><dd>${p.reviewed ? B("reviewed by a person", "green") : B("awaiting review", "amber")}</dd><dt>Retrieval eligibility</dt><dd>${p.enabled ? B("enabled", "green") : B("disabled", "amber")}</dd><dt>Fixture relevance</dt><dd>${E(p.score)}</dd></dl><div class="row">${eb(p.enabled ? "Disable for retrieval" : "Enable for retrieval", "passage-toggle", "", "small")}${eb(p.reviewed ? "Reviewed" : "Mark reviewed", "passage-review", "", "small", "shield")}${eb("Propose correction", "passage-correct", "", "small", "edit")}${eb("Evaluate this passage", "source-eval", "", "small", "chart")}</div>${x.passageCorrection ? note("Correction is a versioned draft. It creates a review item (RV-01 style); the published pack is never edited in place. The original document remains the provenance anchor.") : ""}`
          : ""
      }`,
    )}</div><aside class="e-stack">${panel(
      "Quality & ownership",
      `<div class="e-focus"><h3>${E(s.title)}</h3><dl class="kv"><dt>Parse errors</dt><dd>${ev.quality.parseErrors}</dd><dt>Missing tables</dt><dd>${ev.quality.missingTables}</dd><dt>Duplicates</dt><dd>${ev.quality.duplicates}</dd><dt>Conflicting statements</dt><dd>${ev.quality.conflicts}</dd><dt>Freshness</dt><dd>${E(ev.quality.freshness)}</dd><dt>Named verifier</dt><dd>${E(ev.quality.verifier)}</dd><dt>Reviewer role</dt><dd>${E(ev.quality.role)}</dd><dt>Approval requirement</dt><dd>${E(ev.quality.approval)}</dd></dl></div>`,
      B(ev.quality.parseErrors ? "Review required" : "Clean", ev.quality.parseErrors ? "amber" : "green"),
    )}${panel(
      "Used by",
      `<div class="e-focus"><ul class="e-used-by">${ev.usedBy.map(([kind, name]) => `<li><span class="mono">${E(kind)}</span><strong>${E(name)}</strong></li>`).join("")}</ul>${note("A correction affects these consumers only through a new draft and a review gate. Existing run pins stay unchanged.")}</div>`,
    )}${panel(
      "Evaluate this source",
      `<div class="e-focus"><p>Measure extraction fidelity, expected-evidence recall and citation coverage against held-out cases.</p>${eb("Open object evaluation", "source-eval", "", "small", "chart")}</div>`,
    )}</aside></div>`;
  }
  function packsView() {
    return `<div class="e-main-grid"><div class="e-stack">${panel("Platform domain pack", `<div class="e-focus"><div class="between"><span class="eyebrow">ONE PACK / VERSIONED AS A WHOLE</span>${B(x.packRetired ? "Retired" : "Published", x.packRetired ? "amber" : "green")}</div><h2>capability@${x.packPublished}</h2><p>Immutable platform-owner root. Documents carry <code>metadata.capability</code> and provenance; retrieval filters by capability before reranking.</p><div class="e-version-track"><span>16 · retired</span><strong>${x.packPublished} · ${x.packRetired ? "retired" : "published"}</strong><span>${x.packDraft} · draft</span></div><div class="row">${eb("Review next version", "pack-review", "", "primary", "shield")}${eb("Retire version", "pack-retire", "", "small")}${eb("Export manifest", "pack-export", "", "small", "download")}${eb("Evaluate this pack", "pack-eval", "", "small", "chart")}</div>${note("Tenant documents may shadow platform documents through supersedes. They retain their source, licence and layer. A new pack version never silently changes an active run pin.")}</div>`)}${panel("Capability bindings", `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Capability</th><th>Pack pin</th><th>Required</th><th>Min score</th><th>Readiness</th></tr></thead><tbody>${["architecture", "discovery", "delivery", "verification"].map((c) => `<tr><td>${btn(nameOf("capabilities", c), "open-definition", `data-kind="capabilities" data-id="${c}"`, "quiet small")}</td><td><code>capability@17</code></td><td>Yes</td><td>0.72</td><td>${B(x.packAvailable ? "Ready" : "Missing pin", x.packAvailable ? "green" : "amber")}</td></tr>`).join("")}</tbody></table></div>`)}${panel("Pinned run vs next draft", `<div class="e-focus"><dl class="kv"><dt>RUN-8821</dt><dd>capability@17 · immutable execution input</dd><dt>New draft</dt><dd>capability@${x.packDraft} · requires review and publication</dd><dt>Adoption</dt><dd>Explicit binding revision for future runs</dd></dl></div>`)}</div><aside>${panel("Binding & service health", `<div class="e-focus"><dl class="kv"><dt>strata_ingest</dt><dd>${B("Configured", "green")}</dd><dt>strata_rag</dt><dd>${B(x.packAvailable ? "Pin resolves" : "Pin unavailable", x.packAvailable ? "green" : "amber")}</dd><dt>Run retrieval</dt><dd><code>knowledge.query</code></dd><dt>Raw integration</dt><dd><code>retrieval.query</code> · operator surface</dd><dt>Platform targeting</dt><dd>Explicit leaf IDs from the pinned pack</dd></dl>${eb(x.packAvailable ? "Simulate outage" : "Restore pack", "pack-outage", "", "small", "replay")}${route("Attempt a governed run", "execution", "small", "run")}</div>`)}</aside></div>`;
  }
  function retrieval() {
    return `<div class="e-main-grid"><div class="e-stack">${panel(
      "Capability-bound retrieval",
      `<form id="e-retrieval-form" class="e-focus"><label class="field"><span>Question</span><textarea name="query" required>${E(x.knowledgeQuery)}</textarea></label><div class="e-two">${S(
        "Capability",
        "capability",
        db.capabilities.map((c) => ({ id: c.id, name: c.name })),
        "architecture",
      )}${F("Top K", "top_k", "5", "number")}</div><label class="field"><span>Expected evidence</span><textarea name="expected" required>${E(x.retrievalExpected)}</textarea></label><div class="e-action-bar"><span class="mono muted">knowledge.query · capability@17 · min_score 0.72</span><button class="btn primary">Retrieve sample evidence ↗</button></div></form>`,
    )}${x.retrieval ? panel("Cited evidence", x.retrieval.error ? `<div class="e-blocker"><strong>${E(x.retrieval.error)}</strong> The required pinned source cannot resolve. No fabricated retrieval results.</div>` : `<div class="e-focus"><div class="between"><strong>${x.retrieval.hits.length} sample hits</strong><span class="mono muted">${E(x.retrieval.capability)} · SQL metadata filter → rerank</span></div>${x.retrieval.hits.map((h, i) => `<article class="e-citation"><div><strong>[${i + 1}] ${E(h.title)}</strong>${B(h.layer, h.layer === "tenant" ? "purple" : "green")}</div><p>${E(h.text)}</p><small>${E(h.source)} · ${h.pages} · score ${h.score} · ${E(h.pack)}</small>${eb("Inspect provenance", "citation", `data-index="${i}"`, "quiet small", "arrow")}</article>`).join("")}</div>`) : panel("Query contract", `<div class="e-focus"><p>The capability resolves the exact pack version and leaf IDs. strata_rag owns retrieval and reranking; Conveyor returns the source evidence with its layer, pack, title, source and pages.</p><p>Run a sample query to inspect the cited evidence and a tenant override.</p></div>`)}${x.retrieval ? expectedEvidencePanel() : ""}</div><aside>${panel("Retrieval boundary", `<div class="e-focus"><h3>Evidence travels with the answer</h3><dl class="kv"><dt>Platform layer</dt><dd>Published capability pack</dd><dt>Tenant layer</dt><dd>Owned domain sources and explicit supersession</dd><dt>Isolation</dt><dd>Tenant identity + allowed platform leaves</dd><dt>Memory scope</dt><dd>source_ids: conveyor:work:WP-2048</dd></dl>${note("The sample response is a deterministic offline fixture, not a semantic search over your documents.")}${eb(x.packAvailable ? "Simulate required-pack outage" : "Restore required pack", "pack-outage", "", "small", "replay")}</div>`)}</aside></div>`;
  }
  function expectedEvidencePanel() {
    const expected = x.retrievalExpected.trim().toLowerCase();
    const found = (x.retrieval?.hits || []).some((h) =>
      (h.text + " " + h.title + " " + h.source)
        .toLowerCase()
        .includes(expected),
    );
    return panel(
      "Expected evidence check",
      `<div class="e-focus"><p>Expected: <em>${E(x.retrievalExpected)}</em></p><p>${found ? B("found in the retrieved passages", "green") : B("missed · candidate regression", "amber")}</p>${note("Retrieving evidence is not the same as generating a correct answer. This check measures whether the expected passage was returned; the answer is graded separately.")}${x.retrievalSaved ? note("Candidate case saved: " + E(x.caseName) + " · human review required before it becomes acceptance criteria. Model assessment and measured outcome stay separate.") : eb("Save as evaluation case", "save-case", "", "small", "chart")}</div>`,
    );
  }
  function memory() {
    const facts = [
      {
        kind: "fact",
        body: "Delegated identities require an explicit approved action set.",
        source: "system",
        time: "14:30",
      },
      {
        kind: "attempt",
        body: "Attempt 1 produced a baseline with two unresolved realisation links.",
        source: "agent",
        time: "14:31",
      },
      {
        kind: "blocker",
        body: "Architecture gate waits for delegation acceptance evidence.",
        source: "human",
        time: "14:32",
      },
      ...x.hot,
    ];
    return `<div class="e-two">${panel("Hot work-package memory", `<div class="e-focus"><span class="eyebrow">SYSTEM OF RECORD / WP-2048</span><p>Transactional rows scoped to this card and project. New facts supersede earlier facts; the history remains available.</p><div class="e-memory-list">${facts.map((f) => `<article>${B(f.kind)}<p>${E(f.body)}</p><small>${E(f.source)} · ${f.time} · WP-2048 / identity</small></article>`).join("")}</div><form id="e-memory-form"><div class="e-two">${S("Kind", "kind", ["fact", "attempt", "decision", "blocker"], "fact")}${S("Source", "source", ["human"], "human")}</div><label class="field"><span>Memory entry</span><textarea name="body" required placeholder="Record an evidenced fact or decision…"></textarea></label><button class="btn primary">Remember locally</button></form></div>`)}${panel("Cold semantic memory", `<div class="e-focus"><span class="eyebrow">DERIVED / STRATA_RAG</span><h2>Work-package digest</h2><p>Consolidation creates a derived summary in the tenant root store. Hot transactional facts take precedence on conflict.</p><pre class="e-json">work-package: WP-2048\nsource_id: conveyor:work:WP-2048\nproject_id: identity\ncapability: architecture\nsource_run: consolidation-${x.consolidated + 1}</pre><dl class="kv"><dt>Unconsolidated rows</dt><dd>${Math.max(0, facts.length - x.consolidated)}</dd><dt>Digest lifecycle</dt><dd>Ingest new → prune old except new source_run</dd><dt>Recall boundary</dt><dd>This card's source_id only</dd><dt>Procedural memory</dt><dd>Immutable run / approval / action history</dd></dl>${eb("Consolidate sample memory", "consolidate", "", "primary", "layers")}${route("Inspect execution history", "runs", "small", "clock")}${note("The work-package header keeps two cards with identical notes distinct under document deduplication. No event-by-event vector writes.")}</div>`)}</div>`;
  }
  function jobForm(isNew = false) {
    const j = isNew
      ? {
          title: "",
          id: "",
          capability: "architecture",
          description: "",
          inputs: [],
          outputs: [],
          process: "model",
          agent: "architect",
          entry: "",
          exit: "",
        }
      : jobs.find((j) => j.id === x.selectedJob);
    dialogForm(
      isNew ? "Define an agent job type" : "Edit job type · new draft",
      F("Title", "name", j.title) +
        F("Stable job code", "code", j.id) +
        S(
          "Capability",
          "capability",
          [...new Set(jobs.map((j) => j.capability))],
          j.capability,
        ) +
        F("Purpose", "description", j.description, "textarea", "", true) +
        S(
          "Process",
          "process",
          db.processes.map((p) => ({ id: p.id, name: p.name })),
          j.process,
        ) +
        S(
          "Agent",
          "agent",
          db.agents.map((a) => ({ id: a.id, name: a.name })),
          j.agent,
        ) +
        F(
          "Required inputs (one per line)",
          "inputs",
          (j.inputs || []).join("\n"),
          "textarea",
        ) +
        F(
          "Expected outputs (one per line)",
          "outputs",
          (j.outputs || []).join("\n"),
          "textarea",
        ) +
        F("Entry conditions", "entry", j.entry, "textarea") +
        F("Exit evidence / acceptance", "exit", j.exit, "textarea"),
      "e-job-form",
      "Save local draft",
      `data-new="${isNew}"`,
    );
  }
  function changeDialog() {
    openDialog(
      "CR-017 · Integrated change control",
      `<div class="dialog-body"><div class="e-two"><div><span class="eyebrow">CURRENT BASELINE B${x.baseline}</span><h3>Contract validation after migration</h3><p>Full migration precedes review of the shared data contract. Current forecast consumes 8 of 15 buffer days.</p></div><div><span class="eyebrow">PROPOSED BASELINE B${x.baseline + 1}</span><h3>Validate the bounded contract first</h3><p>Split WP-2049 into contract validation and full migration verification. Retain both acceptance gates.</p></div></div><dl class="kv"><dt>Scope</dt><dd>2 packages split; requirements retained</dd><dt>Schedule</dt><dd>Forecast recovery 6 days</dd><dt>Cost</dt><dd>Forecast +€18,000</dd><dt>Risk</dt><dd>Reduce integration uncertainty; add coordination</dd><dt>Authority</dt><dd>Change control board · George acting as person in this mock</dd><dt>Evidence</dt><dd>Impact assessment IA-017; architecture finding F-012</dd></dl><label class="field"><span>Decision rationale</span><textarea id="e-change-reason" placeholder="Record the reason for the board decision…"></textarea></label><div id="form-error" class="error" role="alert" hidden></div>${note("Local simulation. Publishing B" + (x.baseline + 1) + " keeps existing run pins and historical evidence intact.")}</div>`,
      `${eb("Return for analysis", "change-return", "", "")}${eb("Approve & baseline", "change-approve", "", "primary", "shield")}`,
      true,
    );
  }
  function newWork(job) {
    const j = job || jobs.find((j) => j.id === x.selectedJob) || jobs[0];
    dialogForm(
      "Create work package",
      F("Name", "name", j.title) +
        S(
          "Project",
          "project",
          db.projects.map((p) => ({ id: p.id, name: p.name })),
          x.project,
        ) +
        S(
          "Agent job type",
          "job",
          jobs.map((j) => ({ id: j.id, name: j.title })),
          j.id,
        ) +
        F("WBS reference", "wbs", "1.2.5"),
      "e-work-form",
      "Create local work package",
    );
  }
  document.addEventListener("click", (e) => {
    const navigation = e.target.closest('[data-action="navigate"]');
    if (navigation && $("#dialog").contains(navigation)) $("#dialog").close();
    const el = e.target.closest("[data-e]");
    if (!el) return;
    const d = el.dataset,
      job = () => jobs.find((j) => j.id === x.selectedJob);
    switch (d.e) {
      case "programme":
        x.programme = d.id;
        x.project = scopeProjects()[0].id;
        navigate("overview");
        break;
      case "programme-tab":
        x.tab = d.value;
        render();
        break;
      case "select-project":
        x.project = d.id;
        if (state.page === "projects") {
          x.programme = byId("projects", d.id).programme || "civic";
          navigate("overview");
        } else render();
        break;
      case "select-plan":
        x.project = d.id;
        x.tab = "Delivery map";
        render();
        break;
      case "wbs-toggle":
        x.expanded.has(d.id) ? x.expanded.delete(d.id) : x.expanded.add(d.id);
        render();
        break;
      case "project-work":
        x.project = d.id;
        x.workFilter = d.id;
        navigate("work");
        break;
      case "charter": {
        const p = selectedProgramme();
        openDialog(
          p.key + " · Programme charter",
          `<div class="dialog-body"><span class="eyebrow">${E(p.horizon)} / APPROVED CHARTER @3</span><h2>${E(p.name)}</h2><p>${E(p.sub)}</p><dl class="kv"><dt>Outcome</dt><dd>${E(p.benefit)}</dd><dt>Sponsor</dt><dd>${E(p.sponsor)}</dd><dt>Investment envelope</dt><dd>${E(p.budget)} · illustrative</dd><dt>Governance</dt><dd>Programme board, architecture board, change control board</dd><dt>Success measure</dt><dd>Lead time, verified throughput, benefit adoption and residual risk</dd><dt>Delivery system</dt><dd>Tenant-defined types, capabilities, agent jobs, processes and gates</dd></dl>${note("The programme contains several projects. Each project has its own type, lifecycle, WBS, work-package queue and execution history.")}</div>`,
        );
        break;
      }
      case "baseline":
        openDialog(
          "Baseline B" + x.baseline + " · Snapshot manifest",
          `<div class="dialog-body"><pre class="e-json">${E(JSON.stringify({ programme: selectedProgramme().key, baseline: x.baseline, charter: 3, scope_wbs: 7, requirements: 4, schedule: 6, architecture: 4, knowledge_pin: "capability@17", change: x.change, active_run_pin: "capability@17" }, null, 2))}</pre>${note("Sample manifest. Published definitions and pack versions are immutable for active runs.")}</div>`,
        );
        break;
      case "goto-constraints":
        navigate("constraints");
        break;
      case "gate":
        x.assurance = "Traceability";
        navigate("assurance");
        break;
      case "assurance-tab":
        x.assurance = d.value;
        render();
        break;
      case "toc-tab":
        x.toc = d.value;
        render();
        break;
      case "toc-node":
        x.tocNode = Number(d.id);
        render();
        break;
      case "toc-injection":
        dialogForm(
          "Propose a constraint injection",
          F(
            "Name",
            "name",
            "Review delegation contract as an independent slice",
          ) +
            F(
              "Causal hypothesis",
              "description",
              "Smaller bounded review batches will reduce time held at the architecture gate.",
              "textarea",
              "",
              true,
            ) +
            F(
              "Expected evidence",
              "evidence",
              "Contract tests, accepted requirement traces, recorded reviewer decision",
              "textarea",
              "",
              true,
            ),
          "e-injection-form",
          "Record proposal",
        );
        break;
      case "trace-node":
        if (d.index === "1") navigate("principles", "isolation");
        else if (d.index === "2") navigate("requirements", "delegation");
        else if (d.index === "3") navigate("models", "identity-arch");
        else if (d.index === "4") navigate("work");
        else if (d.index === "5") navigate("execution");
        else $('[data-e="charter"]')?.click();
        break;
      case "adm-phase":
        openDialog(
          d.phase + " · Architecture work",
          `<div class="dialog-body"><p>Required artifacts and acceptance are authored in the tenant project type and its capability job contracts.</p><div class="e-stack">${route("Requirements baseline", "requirements", "", "file")}${route("Models, elements & views", "models", "", "model")}${route("Architecture jobs", "jobtypes", "", "board")}${route("Knowledge sources & pins", "knowledge", "", "book")}</div><p>Review baseline, target, gaps, stakeholders and evidence before the accountable gate decision.</p></div>`,
        );
        break;
      case "control-record": {
        const row = controlRows.find((r) => r[0] === d.id);
        openDialog(
          row[0] + " · " + row[1],
          `<div class="dialog-body"><h3>${row[2]}</h3><dl class="kv"><dt>Accountable</dt><dd>${row[3]}</dd><dt>Exposure</dt><dd>${row[4]}</dd><dt>Status</dt><dd>${row[5]}</dd><dt>Linked work</dt><dd>WP-2048 / architecture contract</dd></dl>${note("Register detail is a proposed control surface. Changes are reviewed through the programme change process.")}</div>`,
        );
        break;
      }
      case "change":
        changeDialog();
        break;
      case "change-return":
      case "change-approve": {
        const reason = $("#e-change-reason").value.trim();
        if (!reason) {
          error("Record a decision rationale.");
          break;
        }
        if (x.change === "Approved") {
          error("This change request already has a recorded approval.");
          break;
        }
        x.change = d.e === "change-approve" ? "Approved" : "Returned";
        if (x.change === "Approved") x.baseline++;
        activity("George", `${x.change} CR-017: ${reason}`);
        $("#dialog").close();
        render();
        toast(
          `${x.change} locally · ${x.change === "Approved" ? "new baseline B" + x.baseline : "analysis requested"}`,
        );
        break;
      }
      case "export-assurance":
        download(
          "conveyor-assurance-B" + x.baseline + ".json",
          JSON.stringify(
            {
              prototype: true,
              programme: selectedProgramme(),
              baseline: x.baseline,
              requirements: db.requirements,
              principles: db.principles,
              models: db.models,
              control_registers: controlRows,
              change: x.change,
              knowledge_pin: "capability@17",
            },
            null,
            2,
          ),
        );
        toast("Sample assurance pack exported.");
        break;
      case "job-select":
        x.selectedJob = d.id;
        render();
        break;
      case "job-open":
        x.selectedJob = d.id;
        x.jobQuery = "";
        x.jobCapability = "all";
        navigate("jobtypes");
        break;
      case "job-page":
        x.jobPage += Number(d.dir);
        render();
        break;
      case "edit-job":
        jobForm(d.new === "true");
        break;
      case "publish-job": {
        const j = job();
        if (j.status === "Published") {
          toast("Published version is frozen. Edit to create a draft.");
          break;
        }
        if (
          !j.process ||
          !j.agent ||
          !j.inputs?.length ||
          !j.outputs?.length ||
          !j.entry ||
          !j.exit
        ) {
          toast(
            "Publication held: define input/output contracts, process, agent and acceptance evidence.",
          );
          break;
        }
        const proc = byId("processes", j.process),
          a = byId("agents", j.agent);
        if (
          proc.capability !== jobCapability(j) ||
          !proc.steps.some((s) => s.agent === a.id)
        ) {
          toast(
            "Publication held: the selected process must belong to this capability and contain the selected agent.",
          );
          break;
        }
        if (!x.packAvailable) {
          toast("Publication held: required pack cannot resolve.");
          break;
        }
        j.status = "Published";
        activity("George", "Published agent job " + j.id);
        render();
        toast("Published local definition. Active runs retain earlier pins.");
        break;
      }
      case "instantiate-job":
        newWork(job());
        break;
      case "new-work":
        newWork();
        break;
      case "work-detail": {
        const w = packages.find((w) => w.id === d.id);
        openDialog(
          w.id + " · " + w.name,
          `<div class="dialog-body"><dl class="kv"><dt>Work package / card</dt><dd>${w.id} · WBS ${w.wbs}</dd><dt>Project</dt><dd>${E(nameOf("projects", w.project))}</dd><dt>Agent job</dt><dd><code>${E(w.job)}</code></dd><dt>Capability</dt><dd>${E(w.cap)}</dd><dt>Owner</dt><dd>${E(nameOf("agents", w.owner))}</dd><dt>Evidence</dt><dd>${E(w.evidence)}</dd><dt>Dependency</dt><dd>${E(w.risk)}</dd><dt>State</dt><dd>${E(w.state)}</dd></dl><div class="row">${eb("Inspect job definition", "job-open", `data-id="${E(w.job)}"`, "", "board")}${eb("Open execution room", "work-run", `data-id="${w.id}"`, "primary", "run")}</div></div>`,
        );
        break;
      }
      case "work-run": {
        const w = packages.find((w) => w.id === d.id);
        if (w.id !== "WP-2048") {
          toast(
            "Execution detail is available for the WP-2048 sample. This package has no recorded sample run.",
          );
          break;
        }
        x.selectedJob = w.job;
        $("#dialog").close();
        navigate("execution");
        break;
      }
      case "start-run": {
        const j = jobs.find((j) => j.id === "architecture_baseline_harvest");
        if (!x.packAvailable) {
          x.runState = "Blocked · required knowledge";
          render();
          toast("409 capability_not_ready · zero steps started.");
          break;
        }
        if (!j || j.status !== "Published" || !j.agent || !j.process) {
          toast(
            "Start refused: a published job with executable bindings is required.",
          );
          break;
        }
        if (x.runState === "Queued") {
          toast("This sample attempt is already queued.");
          break;
        }
        runJobPin = structuredClone(j);
        runAgentPin = structuredClone(byId("agents", j.agent));
        runProcessPin = structuredClone(byId("processes", j.process));
        x.runState = "Queued";
        x.runAttempt++;
        x.runBaseline = x.baseline;
        x.reviewDecision = "";
        x.eventLog.push([
          "now",
          "System",
          "New sample attempt queued with frozen definition and knowledge pins.",
          "green",
        ]);
        render();
        toast("Sample run queued.");
        break;
      }
      case "run-tab":
        x.runTab = d.value;
        render();
        break;
      case "flow-step":
        x.runTab = ["Context & knowledge", "Evidence", "Evidence", "Activity"][
          Number(d.index)
        ];
        render();
        if (d.index === "3")
          toast(
            "Architecture gate: use Review gate evidence to record a decision.",
          );
        break;
      case "run-review":
        openDialog(
          "Architecture gate · Human decision",
          `<div class="dialog-body"><h3>WP-2048 · Baseline architecture</h3><p>Producer artifacts are attached. Validation still names two missing traces. Approving this baseline review does not approve the programme target architecture contract.</p><dl class="kv"><dt>Actor</dt><dd>George · person · Web</dd><dt>Evidence</dt><dd>Model @4 · code bindings · validation report</dd><dt>Open findings</dt><dd>F-012 and F-013</dd></dl><label class="field"><span>Decision rationale / conditions</span><textarea id="e-review-reason" required></textarea></label><div id="form-error" class="error" role="alert" hidden></div>${x.reviewDecision ? note(E(x.reviewDecision)) : ""}</div>`,
          x.reviewDecision
            ? btn("Close", "close-dialog")
            : `${eb("Return for rework", "run-decision", 'data-value="Returned"')}${eb("Accept baseline review", "run-decision", 'data-value="Accepted"', "primary", "shield")}`,
        );
        break;
      case "run-decision": {
        const reason = $("#e-review-reason").value.trim();
        if (!reason) {
          error("A recorded reason is required for this gate.");
          break;
        }
        x.reviewDecision = d.value + " · " + reason;
        x.runState = d.value === "Accepted" ? "Completed" : "Rework requested";
        x.eventLog.push(["now", "George · person", x.reviewDecision, "amber"]);
        activity("George", x.reviewDecision);
        $("#dialog").close();
        render();
        toast(
          "Local human decision recorded. Programme gate remains separate.",
        );
        break;
      }
      case "context-manifest":
        openDialog(
          "Compiled context · Source manifest",
          `<div class="dialog-body"><pre class="e-json">${E(JSON.stringify({ prototype: true, run: "RUN-8821", card: "WP-2048", procedure: "model@2", agent: "architect@2", prompt_ref: "solution_architect@2", pack: "capability@17", capability: "architecture", knowledge_action: "knowledge.query", memory_source: "conveyor:work:WP-2048", characters: { measured: 8160, ceiling: 12000 }, hash: "sha256:a7c9…e20f (illustrative)" }, null, 2))}</pre></div>`,
        );
        break;
      case "knowledge-tab":
        x.knowledge = d.value;
        render();
        break;
      case "knowledge-map":
        if (["processes", "runs"].includes(d.target)) navigate(d.target);
        else {
          x.knowledge = d.target;
          render();
        }
        break;
      case "source-select":
        x.sourceIndex = Number(d.index);
        x.passageIndex = 0;
        x.passageCorrection = false;
        render();
        break;
      case "passage-select":
        x.passageIndex = Number(d.index);
        x.passageCorrection = false;
        render();
        break;
      case "passage-toggle": {
        const ev = evidenceFor(sourceJobs[x.sourceIndex]);
        const p = ev.passages[x.passageIndex];
        if (!p) break;
        p.enabled = !p.enabled;
        render();
        toast(
          p.enabled
            ? "Passage enabled for retrieval · the review state is unchanged"
            : "Passage disabled for retrieval · the record is retained",
        );
        break;
      }
      case "passage-review": {
        const ev = evidenceFor(sourceJobs[x.sourceIndex]);
        const p = ev.passages[x.passageIndex];
        if (!p) break;
        p.reviewed = !p.reviewed;
        render();
        toast(
          p.reviewed
            ? "Passage marked reviewed by a person"
            : "Passage returned to awaiting review",
        );
        break;
      }
      case "passage-correct": {
        const ev = evidenceFor(sourceJobs[x.sourceIndex]);
        const p = ev.passages[x.passageIndex];
        if (!p) break;
        dialogForm(
          "Propose a passage correction",
          `<p class="muted">The correction creates a versioned draft and a review item. It never edits the published pack in place, and the original document remains the provenance anchor.</p>${F(
            "Corrected passage",
            "text",
            p.extracted,
            "textarea",
            "",
            true,
          )}${F(
            "Why is this correction needed?",
            "rationale",
            "",
            "textarea",
            "Required; recorded with the draft.",
            true,
          )}`,
          "e-passage-form",
          "Create correction draft",
        );
        break;
      }
      case "source-eval":
        state.workbench = { tab: "Object evaluation", subject: "retrieval" };
        navigate("evals");
        break;
      case "pack-eval":
        state.workbench = { tab: "Object evaluation", subject: "knowledge" };
        navigate("evals");
        break;
      case "save-case":
        dialogForm(
          "Save a missed retrieval as an evaluation case",
          `<p class="muted">A case is a candidate, not an acceptance criterion. A person reviews it before it enters a held-out set; the model-assisted finding and the measured outcome remain separate.</p>${F(
            "Case name",
            "name",
            "retrieval-regression · delegated authority",
            "text",
            "",
            true,
          )}${F(
            "Question",
            "question",
            x.knowledgeQuery,
            "textarea",
            "",
            true,
          )}${F(
            "Expected evidence",
            "expected",
            x.retrievalExpected,
            "textarea",
            "",
            true,
          )}${S("Split", "split", [
            ["held-out", "Held-out · never used for tuning"],
            ["train", "Train · candidate only"],
          ], "held-out")}${F(
            "Why is this a regression?",
            "rationale",
            "",
            "textarea",
            "Required. Recorded as a model-assisted finding; a person confirms it.",
            true,
          )}`,
          "e-case-form",
          "Save candidate case",
        );
        break;
      case "new-source":
        dialogForm(
          "Add a governed source",
          F("Source title", "name", "") +
            S(
              "Source kind",
              "kind",
              ["File registration", "URL", "API source"],
              "URL",
            ) +
            F(
              "Explicit tenant leaf store",
              "store",
              "tenant-civic-architecture",
            ) +
            F("Source ID", "source", "") +
            F("Origin / source URL", "origin", "") +
            F("Licence / rights basis", "licence", "Tenant-owned") +
            S(
              "Capability metadata",
              "capability",
              db.capabilities.map((c) => ({ id: c.id, name: c.name })),
              "architecture",
            ) +
            F("Supersedes source (optional)", "supersedes", ""),
          "e-source-form",
          "Queue sample ingestion",
        );
        break;
      case "ingest-step": {
        const s = sourceJobs[x.sourceIndex];
        if (s.step === 7) {
          s.step = 0;
          s.id = "ING-" + (31 + sourceJobs.length);
          s.status = "Queued";
        } else {
          s.step++;
          s.status =
            s.step === 7
              ? "Delivered"
              : [
                  "Acquiring",
                  "Scanning",
                  "Detecting",
                  "Converting",
                  "Enriching",
                  "Bundling",
                  "Delivering",
                ][s.step];
        }
        render();
        toast(
          s.step === 7
            ? "Sample bundle delivered. Pack publication remains separate."
            : "Advanced sample ingestion stage.",
        );
        break;
      }
      case "ingest-contract":
        openDialog(
          "documents.ingest · Service boundary",
          `<div class="dialog-body"><pre class="e-json">documents.ingest {id, store_id, source_id, source_run}\n  → strata_ingest POST /api/ingest/jobs\n  → multipart store_id BEFORE file\n  ← 202 {jobs: [{job_id, status: "queued"}]}\n  → document.external_ref = jobs[0].job_id\n\nacquire → scan → detect → convert → enrich → bundle → deliver\n  → strata_rag tenant leaf store</pre>${note("A per-tenant sealed token identifies the tenant. A failed push records the invocation failure; the existing readable document is not marked failed. Ingestion does not itself publish a platform pack.")}</div>`,
        );
        break;
      case "pack-outage":
        x.packAvailable = !x.packAvailable;
        render();
        toast(
          x.packAvailable
            ? "Required capability@17 resolves again."
            : "Simulated missing required pin. New starts will be refused.",
        );
        break;
      case "knowledge-readiness":
        openDialog(
          "Capability readiness",
          `<div class="dialog-body">${B(x.packAvailable ? "Ready" : "Blocked", x.packAvailable ? "green" : "amber")}<dl class="kv"><dt>Required pack</dt><dd>capability@17 · ${x.packAvailable ? "resolved" : "unreachable"}</dd><dt>Procedure</dt><dd>Design architecture @2 · published</dd><dt>Prompt</dt><dd>solution_architect@2 · resolved</dd><dt>Action grants</dt><dd>Registered actions only</dd><dt>Pre-run guard</dt><dd>${x.packAvailable ? "Required knowledge resolved; job binding checks still apply" : "409 capability_not_ready · no step rows"}</dd><dt>Loss during run</dt><dd>knowledge_degraded</dd></dl></div>`,
        );
        break;
      case "pack-review":
        openDialog(
          "Review capability@" + x.packDraft,
          `<div class="dialog-body"><p>A new immutable version of the platform pack is proposed. The architecture documents add updated source evidence and supersession metadata.</p><dl class="kv"><dt>Review checks</dt><dd>Source ownership, licence allow-list, provenance, capability labels and supersession</dd><dt>Scope</dt><dd>One capability pack, versioned as a whole</dd><dt>Existing bindings</dt><dd>Remain pinned to capability@17</dd></dl><label class="field"><span>Publication rationale</span><textarea id="e-pack-reason"></textarea></label><div id="form-error" class="error" role="alert" hidden></div></div>`,
          eb("Publish sample version", "pack-publish", "", "primary", "shield"),
        );
        break;
      case "pack-publish":
        if (!$("#e-pack-reason").value.trim()) {
          error("Record the provenance and publication review rationale.");
          break;
        }
        x.packPublished = x.packDraft++;
        x.packRetired = false;
        $("#dialog").close();
        render();
        toast(
          "Published sample version. Existing capability and run bindings still pin @17.",
        );
        break;
      case "pack-retire":
        if (x.packRetired) {
          toast("This version is already retired.");
          break;
        }
        x.packRetired = true;
        if (x.packPublished === 17) x.packAvailable = false;
        render();
        toast("Version retired for new use. Historical pins remain visible.");
        break;
      case "pack-export":
        download(
          "capability-pack-manifest.json",
          JSON.stringify(
            {
              prototype: true,
              key: "capability",
              version: x.packPublished,
              state: x.packRetired ? "retired" : "published",
              metadata_filter: "capability",
              existing_binding: 17,
              sources: sourceJobs,
            },
            null,
            2,
          ),
        );
        break;
      case "citation": {
        const h = x.retrieval.hits[Number(d.index)];
        openDialog(
          "Citation provenance · " + h.title,
          `<div class="dialog-body"><p>${E(h.text)}</p><dl class="kv"><dt>Layer</dt><dd>${E(h.layer)}</dd><dt>Pack</dt><dd>${E(h.pack)}</dd><dt>Source</dt><dd>${E(h.source)}</dd><dt>Pages</dt><dd>${E(h.pages)}</dd><dt>Licence</dt><dd>Tenant-owned / platform-owned sample</dd><dt>Capability</dt><dd>${E(x.retrieval.capability)}</dd><dt>Supersedes</dt><dd>${h.layer === "tenant" ? "Platform delegation guidance, section 4" : "None"}</dd><dt>Retrieved</dt><dd>10 Nov 2026 · sample response</dd></dl></div>`,
        );
        break;
      }
      case "consolidate":
        x.consolidated = 3 + x.hot.length;
        activity("Memory", "Consolidated WP-2048 sample digest");
        render();
        toast(
          "Sample digest ingested; old source run pruned. Hot rows retained.",
        );
        break;
    }
    // Navigation from a detail dialog must not leave an obsolete modal over the destination.
    if (
      ["job-open", "trace-node", "adm-phase"].includes(d.e) &&
      $("#dialog").open &&
      d.e !== "adm-phase"
    )
      $("#dialog").close();
  });
  document.addEventListener("submit", (e) => {
    const form = e.target;
    if (!form.id.startsWith("e-")) return;
    e.preventDefault();
    const v = Object.fromEntries(new FormData(form));
    switch (form.id) {
      case "e-job-form": {
        const id = v.code.trim();
        if (!/^[a-z][a-z0-9_]*$/.test(id)) {
          error("Use a stable lowercase code with underscores.");
          return;
        }
        const previous = jobs.find((j) => j.id === x.selectedJob),
          isNew = form.dataset.new === "true";
        if (jobs.some((j) => j.id === id && (isNew || j !== previous))) {
          error("That job code already exists.");
          return;
        }
        const record = {
          id,
          name: id,
          title: v.name.trim(),
          capability: v.capability,
          description: v.description.trim(),
          inputs: v.inputs
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean),
          outputs: v.outputs
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean),
          process: v.process,
          agent: v.agent,
          entry: v.entry.trim(),
          exit: v.exit.trim(),
          status: "Draft",
          origin: "Tenant extension · local authoring",
          version: isNew ? 1 : previous.version + 1,
        };
        if (isNew) jobs.unshift(record);
        else Object.assign(previous, record);
        x.selectedJob = id;
        $("#dialog").close();
        render();
        toast("Saved local job definition draft.");
        break;
      }
      case "e-work-form": {
        const j = jobs.find((j) => j.id === v.job);
        if (j.status !== "Published") {
          error(
            "Select a published job type with executable capability, process and agent bindings.",
          );
          return;
        }
        const w = {
          id: "WP-" + (2100 + packages.length),
          wbs: v.wbs,
          project: v.project,
          name: v.name,
          job: j.id,
          cap: j.capability,
          owner: j.agent,
          state: "Ready",
          evidence: "Awaiting execution",
          risk: x.packAvailable
            ? "Ready to pull"
            : "Required knowledge unavailable",
        };
        packages.unshift(w);
        const p = byId("projects", v.project);
        p.total++;
        x.workFilter = v.project;
        $("#dialog").close();
        navigate("work");
        toast("Local work package created. Run starts still check readiness.");
        break;
      }
      case "e-source-form": {
        if (
          !v.store.trim() ||
          !v.source.trim() ||
          !v.origin.trim() ||
          !v.licence.trim()
        ) {
          error(
            "Supply an explicit tenant leaf store, source ID, origin and rights basis.",
          );
          return;
        }
        sourceJobs.unshift({
          id: "ING-" + (31 + sourceJobs.length),
          title: v.name,
          kind: v.kind,
          store: v.store,
          source: v.source,
          cap: v.capability,
          step: 0,
          status: "Queued",
          pages: 1,
          licence: v.licence,
          date: "10 Nov 2026",
          provenance: v.origin,
          supersedes: v.supersedes,
        });
        x.sourceIndex = 0;
        x.knowledge = "Sources & ingestion";
        $("#dialog").close();
        render();
        toast(
          "Queued local sample source. No upload or network request was made.",
        );
        break;
      }
      case "e-retrieval-form": {
        x.knowledgeQuery = v.query;
        x.retrievalExpected = v.expected;
        const cap = v.capability,
          k = Number(v.top_k);
        if (k < 1 || k > 20) {
          toast("Top K must be between 1 and 20.");
          return;
        }
        x.retrieval = x.packAvailable
          ? {
              capability: cap,
              hits: [
                {
                  title: "Capability assurance method",
                  text: "Architecture conclusions need accepted source evidence and traceable requirements before a human gate advances the work.",
                  layer: "platform",
                  source: "capability-methods-17",
                  pages: "pp. 12–14",
                  score: "0.93",
                  pack: "capability@17",
                },
                {
                  title:
                    cap === "architecture"
                      ? "Tenant identity decision record"
                      : nameOf("capabilities", cap) +
                        " · tenant operating guidance",
                  text:
                    cap === "architecture"
                      ? "For this tenant, delegated identities use explicit action grants. The target model must show the authority boundary and acceptance evidence."
                      : "This tenant capability uses the approved work-package contract, named ownership and independently reviewable evidence at each release gate.",
                  layer: "tenant",
                  source:
                    cap === "architecture"
                      ? "identity-architecture-baseline"
                      : "tenant-" + cap + "-guidance",
                  pages: "pp. 42–44",
                  score: "0.91",
                  pack: "tenant layer · supersedes platform guidance",
                },
                {
                  title: "Evidence before progression",
                  text: "A producer response alone does not establish completion. Observe the result and attach the evaluation to the exact work and revision.",
                  layer: "platform",
                  source: "capability-methods-17",
                  pages: "p. 18",
                  score: "0.86",
                  pack: "capability@17",
                },
              ].slice(0, k),
            }
          : { error: "knowledge_degraded", capability: cap };
        render();
        break;
      }
      case "e-passage-form": {
        if (!v.rationale.trim()) {
          error("Record why this correction is needed.");
          return;
        }
        x.passageCorrection = true;
        $("#dialog").close();
        render();
        toast(
          "Correction draft created · review item queued; published pack unchanged",
        );
        break;
      }
      case "e-case-form": {
        if (!v.name.trim() || !v.expected.trim()) {
          error("A case name and expected evidence are required.");
          return;
        }
        x.retrievalSaved = true;
        x.caseName = v.name;
        x.retrievalExpected = v.expected;
        $("#dialog").close();
        render();
        toast(
          "Candidate case saved · human review required before it becomes acceptance criteria",
        );
        break;
      }
      case "e-memory-form":
        x.hot.push({
          kind: v.kind,
          source: "human",
          body: v.body,
          time: "just now",
        });
        activity("George", "Recorded hot memory for WP-2048");
        render();
        toast("Memory recorded locally.");
        break;
      case "e-operator-note":
        x.hot.push({
          kind: "fact",
          source: "human",
          body: v.note,
          time: "just now",
        });
        x.eventLog.push([
          "now",
          "George · person",
          "Operator guidance recorded: " + v.note,
          "amber",
        ]);
        render();
        toast("Guidance added to card memory and execution record.");
        break;
      case "e-injection-form":
        activity("George", "Proposed constraint injection: " + v.name);
        controlRows.unshift([
          "INJ-" + (controlRows.length + 1),
          "Injection",
          v.name,
          "Programme board",
          "Review",
          "Proposed",
        ]);
        $("#dialog").close();
        toast(
          "Injection proposal recorded in control registers; approval pending.",
        );
        break;
    }
  });
  function preserveInput(id, update) {
    const el = $(id),
      pos = el.selectionStart;
    update(el.value);
    render();
    const next = $(id);
    next.focus();
    if (pos !== null) next.setSelectionRange(pos, pos);
  }
  document.addEventListener("input", (e) => {
    if (e.target.id === "e-job-search")
      preserveInput("#e-job-search", (v) => {
        x.jobQuery = v;
        x.jobPage = 0;
      });
    if (e.target.id === "e-work-search")
      preserveInput("#e-work-search", (v) => {
        x.workSearch = v;
      });
  });
  document.addEventListener("change", (e) => {
    if (e.target.id === "e-job-cap") {
      x.jobCapability = e.target.value;
      x.jobPage = 0;
      render();
    }
    if (e.target.id === "e-work-filter") {
      x.workFilter = e.target.value;
      render();
    }
  });
  // Fixture records for projects created through the existing project form join the portfolio.
  document.addEventListener("submit", (e) => {
    if (e.target.id === "project-form")
      setTimeout(() => {
        db.projects
          .filter((p) => !p.programme)
          .forEach((p) =>
            Object.assign(p, {
              programme: x.programme,
              stream: "New workstream",
              blocks: 0,
              live: 0,
            }),
          );
        if (state.page === "projects" || state.page === "overview") render();
      }, 0);
  });
  const titles = {
    overview: "Programme control",
    projects: "Portfolio & projects",
    work: "Work breakdown",
    jobtypes: "Agent job types",
    execution: "Execution room",
    assurance: "Assurance & baselines",
    constraints: "Constraints & TOC",
    knowledge: "Knowledge & memory",
  };
  return {
    sidebar,
    titles,
    searchEntries: () =>
      Object.entries(titles).map(([page, name]) => ({
        name,
        page,
        kind: "Destination",
      })),
    pages: {
      overview,
      projects,
      work,
      jobtypes: jobTypes,
      execution,
      assurance,
      constraints,
      knowledge,
    },
  };
};
