/* Local interaction study, grounded in D20/D23–27, S15–21 and the current Rust screens.
   Proposed scope projections and authoring controls are explicitly identified in the UI. */
"use strict";
window.installLearning = function (api, enterprise) {
  const {
    db,
    state,
    esc: E,
    icon: I,
    badge: B,
    btn,
    header: H,
    navigate,
    render,
    openDialog,
    dialogForm,
    field: F,
    selectField,
    toast,
    error,
    activity,
    byId,
    nameOf,
    download,
  } = api;
  const S = (label, name, value, options) =>
    selectField(
      label,
      name,
      options.map((o) => (Array.isArray(o) ? { id: o[0], name: o[1] } : o)),
      value,
    );
  const $ = (q) => document.querySelector(q);
  const button = (label, action, attrs = "", cls = "", symbol = "") =>
    `<button type="button" class="btn ${cls}" data-l="${action}" ${attrs}>${symbol ? I(symbol) : ""}${E(label)}</button>`;
  const route = (label, page, cls = "", symbol = "arrow") =>
    btn(label, "navigate", `data-page="${page}"`, cls, symbol);
  const panel = (title, body, action = "", bodyClass = "l-body") =>
    `<section class="l-panel"><div class="l-panel-head"><h2>${E(title)}</h2>${action}</div><div class="${bodyClass}">${body}</div></section>`;
  const note = (title, body, tone = "") =>
    `<div class="l-note ${tone}"><strong>${E(title)}</strong>${body}</div>`;
  const tabs = (values, current, key) =>
    `<div class="l-tabs" aria-label="Workspace views">${values.map((v) => button(v, "tab", `data-key="${key}" data-value="${E(v)}" aria-pressed="${v === current}"`, v === current ? "selected" : "")).join("")}</div>`;
  const rows = (pairs) =>
    `<dl class="kv">${pairs.map(([a, b]) => `<dt>${E(a)}</dt><dd>${b}</dd>`).join("")}</dl>`;
  const table = (heads, records) =>
    `<div class="l-table-wrap"><table class="l-table"><thead><tr>${heads.map((h) => `<th>${E(h)}</th>`).join("")}</tr></thead><tbody>${records.join("")}</tbody></table></div>`;
  const metrics = (items) =>
    `<div class="l-metrics">${items.map(([label, value, detail, tone = ""]) => `<div><span>${E(label)}</span><strong class="${tone}">${E(value)}</strong><small>${E(detail)}</small></div>`).join("")}</div>`;
  const boundary = (text) =>
    `<p class="l-boundary">LOCAL DESIGN STUDY · ${text} All numbers, traces and decisions here are illustrative and reset on reload.</p>`;
  const heading = (title, sub, actions = "", area = "MEASURE & IMPROVE") =>
    H(title, sub, actions, "NON-AGENTIC AI / " + area);

  const l = {
    cap: "architecture",
    scope: "capability",
    designTab: "Knowledge contract",
    selections: {},
    memoryPolicies: {},
    bindingRevision: 4,
    process: "model",
    agent: "architect",
    memoryScope: "Work package",
    memoryItem: "m-card",
    memoryTab: "Records",
    recall: false,
    promoted: false,
    promotionDraft: false,
    memoryQuery: "delegation",
    actor: "reviewer",
    evalTab: "Instrument",
    evalId: "architecture",
    version: 3,
    caseRevision: 1,
    gates: { inputs: true, grading: false, cost: true, harness: true },
    evalRun: false,
    evalMode: "Pilot · 5 cases",
    evals: [],
    evalStates: {},
    climbStates: {},
    cases: [],
    climbTab: "Rounds & evidence",
    climbId: "quality",
    signed: false,
    stopped: false,
    triaged: false,
    pendingRound: null,
    publishedDraft: false,
    roundIndex: 1,
    benchTab: "Runs & controls",
    benchmarkA: "baseline",
    benchmarkB: "candidate",
    promotedBenchmark: false,
    campaignScope: "All capabilities",
    auditTab: "Inference ledger",
    auditStatus: "Proposed",
    auditStarted: false,
    ceiling: 300,
    delta: 8,
    target: "Held-out acceptance",
    roundGate: true,
    recallNotice: "",
    knowledgeOutage: false,
    configView: "Draft",
    configConflict: true,
    configMissing: false,
    configResolved: false,
    configDraft: null,
    configRevision: 1,
    reviewTab: "Queue",
    reviewItem: "RV-01",
    reviewStates: {},
    workflowStep: 1,
    memoryView: "Current",
    memoryDim: "All",
    successor: false,
    successorText: "",
    excluded: {},
    digestRebuilt: false,
    policyCompare: false,
    evalSubject: "retrieval",
    replayScheduled: false,
    replayError: "",
    replayCutoff: "2026-10-31T23:59",
    replayMemory: "isolated",
  };
  const profiles = {
    architecture: {
      label: "Enterprise architecture",
      process: "model",
      agent: "architect",
      pack: "architecture",
      intent: "Architecture compliance assessment",
      role: "Assess target architecture against approved principles, requirements and exceptions.",
      knowledge: [
        "TOGAF ADM reference",
        "ArchiMate / UML modelling rules",
        "Tenant architecture principles",
      ],
      caseTitle: "Delegated identity",
      subject: "Design architecture @2",
      recipe: "solution_architect@2",
      job: "WP-2048",
    },
    delivery: {
      label: "Software delivery",
      process: "feature",
      agent: "builder",
      pack: "delivery",
      intent: "Verified implementation increment",
      role: "Implement the approved requirement and produce independently verifiable evidence.",
      knowledge: [
        "Repository engineering standards",
        "Service contract conventions",
        "Tenant coding principles",
      ],
      caseTitle: "Invitation acceptance",
      subject: "Build a feature @7",
      recipe: "implementation_role@4",
      job: "C-142",
    },
    discovery: {
      label: "Requirements engineering",
      process: "discover",
      agent: "analyst",
      pack: "requirements",
      intent: "Requirements baseline review",
      role: "Elicit unambiguous requirements with acceptance evidence and source traceability.",
      knowledge: [
        "Requirements quality criteria",
        "Stakeholder source register",
        "Tenant discovery principles",
      ],
      caseTitle: "Conflicting requirements",
      subject: "Discover requirements @3",
      recipe: "requirements_analyst@2",
      job: "WP-2061",
    },
    verification: {
      label: "Independent assurance",
      process: "verify",
      agent: "verifier",
      pack: "verification",
      intent: "Independent evidence review",
      role: "Observe outcomes independently and classify unmet acceptance criteria.",
      knowledge: [
        "Verification standards",
        "Evidence classification guide",
        "Tenant assurance principles",
      ],
      caseTitle: "Unverified producer claim",
      subject: "Verify increment @3",
      recipe: "verification_role@3",
      job: "WP-2102",
    },
  };
  const profile = () => profiles[l.cap] || profiles.architecture;
  const selectionAt = (scope = l.scope) =>
    l.selections[l.cap + ":" + scope] ||
    (scope === "capability" ? "both" : "inherit");
  function effectiveSources(scope = l.scope) {
    let sources = [0, 1, 2];
    for (const level of ["capability", "process", "agent"]) {
      const selection = selectionAt(level);
      if (selection === "none") sources = [];
      else if (selection === "standards")
        sources = sources.filter((id) => id < 2);
      if (level === scope) break;
    }
    return sources;
  }
  const memoryPolicyAt = () =>
    l.memoryPolicies[l.cap + ":" + l.scope] ||
    (l.scope === "capability" ? "hot_digest" : "inherit");
  function effectiveMemory() {
    let rank = 2;
    for (const scope of ["capability", "process", "agent"]) {
      const policy = l.memoryPolicies[l.cap + ":" + scope];
      if (policy === "none") rank = 0;
      if (policy === "hot_only") rank = Math.min(rank, 1);
      if (scope === l.scope) break;
    }
    return [
      "Recall disabled",
      "Same-card hot rows only",
      "Same-card hot rows + cold digest",
    ][rank];
  }
  const selectedEval = () =>
    l.evals.find((e) => e.id === l.evalId) || l.evals[0];
  function switchEval(id) {
    l.evalStates[l.evalId] = {
      gates: { ...l.gates },
      version: l.version,
      evalRun: l.evalRun,
    };
    l.evalId = id;
    Object.assign(
      l,
      l.evalStates[id] || {
        gates: { inputs: false, grading: false, cost: false, harness: false },
        version: 1,
        evalRun: false,
      },
    );
  }
  function switchClimb(id) {
    if (id === l.climbId) return;
    const keys = [
      "signed",
      "stopped",
      "target",
      "ceiling",
      "delta",
      "roundGate",
    ];
    l.climbStates[l.climbId] = Object.fromEntries(
      keys.map((key) => [key, l[key]]),
    );
    l.climbId = id;
    Object.assign(
      l,
      l.climbStates[id] || {
        signed: false,
        stopped: false,
        target: id === "cost" ? "cost_usd" : "Held-out acceptance",
        ceiling: 300,
        delta: 8,
        roundGate: true,
      },
    );
    l.climbTab = "Rounds & evidence";
  }
  l.evals = [
    {
      id: "architecture",
      name: "Architecture acceptance",
      capability: "architecture",
      subject: "Design architecture @2",
      cases: 240,
      tags: "Delegation / traceability / conformance",
      ready: false,
    },
    {
      id: "delivery",
      name: "Verified coding outcomes",
      capability: "delivery",
      subject: "Build a feature @7",
      cases: 120,
      tags: "API / UI / migration",
      ready: true,
    },
    {
      id: "requirements",
      name: "Requirements baseline quality",
      capability: "discovery",
      subject: "Discover requirements @3",
      cases: 90,
      tags: "Ambiguity / conflicts / acceptance",
      ready: true,
    },
  ];
  l.cases = [
    {
      id: "AC-001",
      title: "Delegated identity",
      tag: "Delegation",
      split: "train",
      input:
        "Assess the target identity model against REQ-214 and PR-012. Identify the missing delegation constraint.",
    },
    {
      id: "AC-002",
      title: "Requirement realisation",
      tag: "Traceability",
      split: "train",
      input:
        "Review the realisation links in model M-04. Report requirements without accepted evidence.",
    },
    {
      id: "AC-003",
      title: "Recorded exception",
      tag: "Conformance",
      split: "train",
      input:
        "Evaluate the proposed exception against PR-008 and the approved architecture contract.",
    },
    {
      id: "AC-161",
      title: "Held-out case",
      tag: "Delegation",
      split: "test",
      input:
        "Reviewer fixture: evaluate a second delegation relationship with an unacknowledged authority boundary.",
    },
  ];
  const rounds = [
    {
      n: 0,
      change: "Published process baseline",
      train: 72,
      test: 70,
      cost: 48.6,
      decision: "Baseline",
      kind: "baseline",
    },
    {
      n: 1,
      change: "Select the cited constraint before drafting",
      train: 84,
      test: 85,
      cost: 47.1,
      decision: "Keep",
      kind: "coverage",
    },
    {
      n: 2,
      change: "Add more worked training examples",
      train: 94,
      test: 85,
      cost: 56.3,
      decision: "Revert",
      kind: "overfit",
    },
    {
      n: 3,
      change: "Expand the role instructions",
      train: 86,
      test: 85,
      cost: 52.2,
      decision: "Revert",
      kind: "flat",
    },
  ];
  l.cases.forEach((c) => {
    c.evalId = "architecture";
  });
  const memoryRecords = [
    {
      id: "m-tenant",
      level: "Tenant",
      title: "Canonical identity ownership",
      body: "Use the tenant-approved identity ownership convention. Three projects cite the same approved decision.",
      kind: "Curated lesson",
      source: "ADR-041 · approved revision 3",
      store: "Governed asset + tenant source document",
      owner: "Architecture board",
      status: "Published",
      visibility: "Tenant, only through an explicit source binding",
      proposal: true,
    },
    {
      id: "m-programme",
      level: "Programme",
      title: "Architecture contract is the constraint",
      body: "Identity contract approval constrains data and service workstreams; the recovery injection remains a hypothesis until the gate is accepted.",
      kind: "Decision projection",
      source: "CIVIC · B3 · CR-017",
      store: "Projection of governed decisions and events",
      owner: "Programme director",
      status: "Read only",
      visibility: "Programme-authorised work; not implicit work-package recall",
      proposal: true,
    },
    {
      id: "m-project",
      level: "Project",
      title: "Delegation boundary accepted with conditions",
      body: "REQ-214 must name delegated authority explicitly. No downstream contract should infer that authority from authentication alone.",
      kind: "Project decision",
      source: "Trust & identity · ADR-041",
      store: "Governed asset + approved source registration",
      owner: "Project architect",
      status: "Approved",
      visibility: "Identity project; broader use needs explicit promotion",
      proposal: true,
    },
    {
      id: "m-capability",
      level: "Capability",
      title: "Check realisation before compliance",
      body: "Two architecture reviews missed a realisation edge. Candidate lesson: require the observer to enumerate untraced requirements.",
      kind: "Candidate lesson",
      source: "architecture · 2 work-package references",
      store: "Derived review projection; proposed curation workflow",
      owner: "Capability owner",
      status: "Needs review",
      visibility:
        "Curation queue; not automatically injected into other projects",
      proposal: true,
    },
    {
      id: "m-process",
      level: "Process",
      title: "Gate G3 returned at step 4",
      body: "The architecture review gate was returned because two requirement traces were absent. The published procedure stayed at version 2.",
      kind: "Procedural memory",
      source: "model@2 · RUN-8821 · approval AP-91",
      store: "run / run_step / approval / event",
      owner: "System event",
      status: "Append only",
      visibility:
        "Process view over authorised runs; no independent memory store",
    },
    {
      id: "m-agent",
      level: "Agent",
      title: "Architect evidence profile",
      body: "The architect executor queried the pinned pack before proposing a model view. Model calls and action invocations retain the run identity.",
      kind: "Executor history",
      source: "architect@2 · RUN-8821 · CALL-774",
      store: "Projection of run + action_invocation + inference_call",
      owner: "System event",
      status: "Append only",
      visibility:
        "Agent view over authorised runs; no private cross-card scratchpad",
    },
    {
      id: "m-card",
      level: "Work package",
      title: "Delegation relation remains unverified",
      body: "The producer claims the delegation edge exists. Independent observer evidence is still missing. Keep this as a claim until the verification action records its verdict.",
      kind: "Claim / fact / verdict",
      source: "WP-2048 · RUN-8821 · invocation INV-441",
      store: "work_package_memory (hot)",
      owner: "Architect · agent · MCP",
      status: "Claim",
      visibility: "tenant + project + card; card id is the work-package id",
    },
    {
      id: "m-run",
      level: "Run / attempt",
      title: "Attempt 2 used the pinned architecture pack",
      body: "Readiness passed for capability@17 and model@2. The run parked at the human architecture gate and released its worker.",
      kind: "Execution event",
      source: "RUN-8821 · attempt 2 · step 4",
      store: "run_step / approval / action_invocation / event",
      owner: "Procedure executor",
      status: "Append only",
      visibility: "This run; its configuration pins remain immutable",
    },
  ];
  const recordMeta = {
    "m-tenant": {
      type: "Lesson",
      recordedAt: "2026-10-14",
      validFrom: "2026-10-14",
      validTo: "2027-10-14",
      humanAccess: "tenant admin · read",
      agentRecall: "eligible only through an explicit source binding",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "ADR-041 · approved revision 3",
      recallUsage: "3 projects cite it",
      evalRefs: "none",
    },
    "m-programme": {
      type: "Decision",
      recordedAt: "2026-10-21",
      validFrom: "2026-10-21",
      validTo: "gate C",
      humanAccess: "programme director · read",
      agentRecall: "programme-authorised work only",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "CIVIC · B3 · CR-017",
      recallUsage: "1 card",
      evalRefs: "none",
    },
    "m-project": {
      type: "Decision",
      recordedAt: "2026-10-29",
      validFrom: "2026-10-29",
      validTo: "project closure",
      humanAccess: "project architect · read",
      agentRecall: "identity project · explicit promotion needed",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "Trust & identity · ADR-041",
      recallUsage: "2 cards",
      evalRefs: "retrieval test RT-04",
    },
    "m-capability": {
      type: "Lesson candidate",
      recordedAt: "2026-11-04",
      validFrom: "2026-11-04",
      validTo: "open",
      humanAccess: "capability owner · review",
      agentRecall: "not eligible until promoted",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "architecture · 2 work-package references",
      recallUsage: "curation queue only",
      evalRefs: "eval architecture-v3",
    },
    "m-process": {
      type: "Observation",
      recordedAt: "2026-11-05",
      validFrom: "2026-11-05",
      validTo: "append-only",
      humanAccess: "process view over authorised runs",
      agentRecall: "procedural history · not free text",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "model@2 · RUN-8821 · approval AP-91",
      recallUsage: "read by the run inspector",
      evalRefs: "none",
    },
    "m-agent": {
      type: "Observation",
      recordedAt: "2026-11-05",
      validFrom: "2026-11-05",
      validTo: "append-only",
      humanAccess: "agent view over authorised runs",
      agentRecall: "no private cross-card scratchpad",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "architect@2 · RUN-8821 · CALL-774",
      recallUsage: "2 turns",
      evalRefs: "prompt audit PA-12",
    },
    "m-card": {
      type: "Claim",
      recordedAt: "2026-11-05 14:32",
      validFrom: "2026-11-05",
      validTo: "open — claim unverified",
      humanAccess: "tenant + project + card",
      agentRecall: "same card only · hot rows win",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "WP-2048 · RUN-8821 · invocation INV-441",
      recallUsage: "4 reads · 1 write",
      evalRefs: "retrieval test RT-04",
    },
    "m-run": {
      type: "Observation",
      recordedAt: "2026-11-05",
      validFrom: "2026-11-05",
      validTo: "append-only",
      humanAccess: "this run",
      agentRecall: "pinned configuration only",
      supersedes: "—",
      supersededBy: "—",
      derivedFrom: "RUN-8821 · attempt 2 · step 4",
      recallUsage: "read by the run inspector",
      evalRefs: "none",
    },
  };
  memoryRecords.forEach((m) => Object.assign(m, recordMeta[m.id]));
  memoryRecords.push(
    {
      id: "m-industry",
      level: "Industry",
      title: "Software industry baseline applies",
      body: "The industry layer sets the delivery and verification obligations that every department inherits. It never selects a tenant's passages by itself.",
      kind: "Decision",
      source: "industry software · layer manifest",
      store: "Release layer (proposed read model)",
      owner: "Tenant admin",
      status: "Published",
      visibility: "Tenant-wide; no automatic recall",
      proposal: true,
    },
    {
      id: "m-department",
      level: "Department",
      title: "Verification owns the identity contract",
      body: "The job-layer row resolved the delivery/verification conflict. The decision is visible at the department layer and pinned by the release.",
      kind: "Decision",
      source: "job row J-12 · rel-backend-1",
      store: "Release layer (proposed read model)",
      owner: "Capability owner",
      status: "Approved",
      visibility: "Department and job views; agent recall needs a binding",
      proposal: true,
    },
    {
      id: "m-job",
      level: "Job",
      title: "backend_engineer release pin",
      body: "The run records job_key and release_hash. A release change creates a new binding and a gate; it never rewrites the running pin.",
      kind: "Summary",
      source: "software.delivery.backend_engineer · rel-backend-1",
      store: "job_binding + release_snapshot (proposed)",
      owner: "Conveyor",
      status: "Pinned",
      visibility: "Job and run views",
      proposal: true,
    },
    {
      id: "m-step",
      level: "Step / call",
      title: "CALL-774 retrieved three passages",
      body: "The call returned two platform passages and one tenant passage; the context manifest selected two and excluded one for budget.",
      kind: "Observation",
      source: "RUN-8821 · step 03 · CALL-774",
      store: "inference_call + tool_call",
      owner: "Procedure executor",
      status: "Append only",
      visibility: "This run and its inspector",
    },
    {
      id: "m-source",
      level: "Source",
      title: "Identity baseline p.42 reviewed",
      body: "A person reviewed the passage and enabled it for retrieval. The original document and the locator remain the provenance anchor.",
      kind: "Observation",
      source: "ING-028 · p.42 · §3.2",
      store: "governed_asset + strata_ingest source_run",
      owner: "A. Mensah · source owner",
      status: "Reviewed",
      visibility: "Tenant source view; correction needs a draft",
    },
    {
      id: "m-published",
      level: "Published knowledge",
      title: "capability@17 published",
      body: "The immutable pack release names the reviewed passages. A new version is a separate release; a live run keeps its pin.",
      kind: "Summary",
      source: "capability@17 · pack release",
      store: "strata_rag pinned pack",
      owner: "Capability owner",
      status: "Published",
      visibility: "Eligible through a job's knowledge pin",
    },
    {
      id: "m-digest",
      level: "Digest",
      title: "WP-2048 cold digest v4",
      body: "The derived digest summarises the card's hot rows. It is rebuildable; the hot rows are the record of truth.",
      kind: "Summary",
      source: "consolidation-4 · conveyor:work:WP-2048",
      store: "strata_rag cold digest",
      owner: "System projection",
      status: "Derived",
      visibility: "Same card only; hot rows win",
    },
    {
      id: "m-lesson",
      level: "Lesson",
      title: "LSN-17 awaits a second independent reference",
      body: "The candidate lesson has one reference (WP-2048/RUN-8821). Promotion needs a second independent card, run or incident plus a person's review.",
      kind: "Lesson candidate",
      source: "LSN-17 · one reference",
      store: "lesson memory (proposed)",
      owner: "Capability owner",
      status: "Needs evidence",
      visibility: "Curation queue only",
      proposal: true,
    },
  );
  const cardDigest = {
    text: "WP-2048 · delegation relation remains a claim (unverified). Attempt 2 used capability@17. Gate G3 returned at step 4 with two missing traces.",
    included: ["m-card · claim (hot wins)", "m-run · attempt 2", "m-process · gate G3 return"],
    omitted: ["m-project · outside the card boundary", "m-tenant · needs an explicit source binding"],
    conflicts: ["m-programme treats the relation as accepted; the card claim is unverified — hot wins and the conflict is retained"],
  };
  const orgConfig = {
    job: "software.delivery.backend_engineer",
    release: "rel-backend-5",
    currentRelease: "rel-backend-1",
    layers: [
      ["Base", "platform pack · shared rules"],
      ["Industry", "software"],
      ["Department archetype", "delivery"],
      ["Department", "delivery · primary"],
      ["Department", "verification"],
      ["Department", "operations"],
      ["Department", "project office"],
      ["Department", "improvement"],
      ["Job", "software.delivery.backend_engineer · rel-backend-5 (draft)"],
    ],
    settings: [
      {
        key: "domain.architecture",
        kind: "domain",
        origin: "base + department",
        current: "capability@17 · delivery-handbook@4",
        draft: "capability@17 · delivery-handbook@4",
        required: true,
      },
      {
        key: "domain.identity",
        kind: "domain",
        origin: "department (delivery + verification)",
        current: "identity-v2 · job row J-12",
        draft: "delivery: identity-v2 · verification: identity-v3",
        required: true,
        conflict: true,
      },
      {
        key: "procedure.release",
        kind: "procedural",
        origin: "department archetype",
        current: "proc-implement@7",
        draft: "proc-implement@7",
        required: true,
      },
      {
        key: "memory.policy",
        kind: "memory",
        origin: "industry",
        current: "same-card hot rows + cold digest",
        draft: "same-card hot rows only · job override",
        required: false,
      },
      {
        key: "context.budget",
        kind: "context",
        origin: "base",
        current: "12,000 characters",
        draft: "12,000 characters",
        required: true,
      },
      {
        key: "tool.grants",
        kind: "tool",
        origin: "job binding",
        current: "repo.write · ci.read",
        draft: "repo.write · ci.read",
        required: false,
      },
    ],
    consumers: [
      ["Job", "software.delivery.backend_engineer", "new release draft"],
      ["Procedure", "proc-implement@7", "pins unchanged"],
      ["Binding", "bind-backend", "rebind behind a gate"],
      ["Runs", "3 runs on the current pin", "not rewritten"],
    ],
    conflict: {
      key: "domain.identity",
      delivery: "identity-v2 · Delivery handbook@4 · page 18",
      verification: "identity-v3 · Verification standard@2 · clause 4.1",
      resolution: "identity-v3 with a documented delivery exception",
    },
    reviewItems: [
      {
        id: "RV-01",
        kind: "Source correction",
        object: "Identity architecture baseline.pdf · passage p.42",
        reviewer: "Source owner · Architecture Office",
        decisions: "1 person",
        evidence: "correction diff · locator p.42",
        age: "2 days",
        state: "In review",
      },
      {
        id: "RV-02",
        kind: "Pack version",
        object: "capability@18",
        reviewer: "Capability owner",
        decisions: "1 person",
        evidence: "reviewed passages · provenance · licence",
        age: "1 day",
        state: "Draft",
      },
      {
        id: "RV-03",
        kind: "Binding revision",
        object: "bind-backend · rel-backend-5",
        reviewer: "Tenant admin",
        decisions: "2 people · dual control",
        evidence: "release hash · stale-binding check",
        age: "4 hours",
        state: "Approved",
      },
      {
        id: "RV-04",
        kind: "PKO release",
        object: "pko@4 · verify_release",
        reviewer: "Capability owner",
        decisions: "1 person",
        evidence: "term usage · validation report",
        age: "3 days",
        state: "Published",
      },
      {
        id: "RV-05",
        kind: "Lesson promotion",
        object: "LSN-17 · claim remains unverified",
        reviewer: "Capability owner",
        decisions: "1 person",
        evidence: "1 reference — second independent reference missing",
        age: "1 day",
        state: "Draft",
      },
    ],
    workflow: [
      ["Failed run", "RUN-151 · verification failed"],
      ["Inspect evidence", "context + work-package memory"],
      ["Identify cause", "stale department guidance"],
      ["Draft correction", "job-layer row · gate"],
      ["Compare baseline", "current release vs draft"],
      ["Held-out review", "object evaluation"],
      ["Approve release", "person · rationale"],
      ["Monitor jobs", "dependents and affected runs"],
    ],
  };
  const configReady = () =>
    !l.configMissing && (l.configResolved || !l.configConflict);
  const workflowStrip = () =>
    `<div class="l-workflow" aria-label="Knowledge workbench workflow"><span class="eyebrow">WORKBENCH · INSPECT → EXPLAIN → TEST → REVISE → REVIEW → RELEASE</span><div class="l-workflow-steps">${orgConfig.workflow
      .map(
        ([title, detail], i) =>
          `<button data-l="workflow-go" data-step="${i + 1}" class="${l.workflowStep === i + 1 ? "selected" : ""}"><span>${i + 1}</span><strong>${E(title)}</strong><small>${E(detail)}</small></button>`,
      )
      .join("")}</div></div>`;
  const scopeBar = (extra = "") =>
    `<div class="l-scope">${I("layers")}<strong>Tenant operating model</strong><span class="muted">/</span><label>Capability <select data-l-change="capability" aria-label="Capability context">${Object.entries(
      profiles,
    )
      .map(
        ([id, p]) =>
          `<option value="${id}" ${id === l.cap ? "selected" : ""}>${E(p.label)}</option>`,
      )
      .join(
        "",
      )}</select></label><span class="grow"></span>${extra || B("Design sandbox")}</div>`;
  function knowledgeDesign() {
    const p = profile(),
      scopeName = {
        capability: p.label,
        process: p.subject,
        agent: nameOf("agents", p.agent),
        run: "Resolved call",
      }[l.scope];
    const chosen = effectiveSources();
    const mainTitle =
      l.designTab === "Effective configuration"
        ? "Job release · " + orgConfig.job
        : l.designTab === "Review & release"
          ? "Knowledge release review"
          : scopeName;
    const crumb =
      l.designTab === "Effective configuration"
        ? `TENANT / software → DELIVERY (PRIMARY) → VERIFICATION → OPERATIONS → PROJECT OFFICE → IMPROVEMENT → JOB / ${E(orgConfig.job)}`
        : l.designTab === "Review & release"
          ? "KNOWLEDGE & MEMORY / REVIEW & RELEASE QUEUE"
          : `CAPABILITY / ${E(p.pack)} → PROCESS / ${E(p.subject)} → AGENT / ${E(p.agent)}`;
    const intro =
      l.designTab === "Effective configuration"
        ? `<div class="l-tagline">${B("Job release · draft", "purple")}${B(orgConfig.release)}${B(configReady() ? "Ready for review" : "Blocked", configReady() ? "green" : "amber")}</div><h2>What this job knows, layer by layer</h2><p>Base → industry → department archetype → ordered departments → job. A conflicting department value is a compile error until a job-layer row resolves it.</p>`
        : l.designTab === "Review & release"
          ? `<div class="l-tagline">${B("Queue", "purple")}${B("Person approval required")}</div><h2>Review and release</h2><p>Every correction, pack version, PKO draft, lesson promotion and binding revision waits here for a person with the named authority.</p>`
          : `<div class="l-tagline">${B(l.scope === "capability" ? "Binding authority" : l.scope === "run" ? "Runtime projection" : "Selection policy · proposal", l.scope === "capability" ? "green" : "purple")}${B("capability@17")}${B("Future runs only")}</div><h2>${E(l.scope === "capability" ? "A reusable knowledge contract" : l.scope === "process" ? "The right sources for this job" : l.scope === "agent" ? "A role with bounded context" : "Explain this model call")}</h2><p>${E(l.scope === "capability" ? "Bind governed domain knowledge once. Processes and agents select relevant passages within this boundary; every call can explain the selection." : l.scope === "process" ? "Procedural knowledge is the published procedure itself: intent, ordered steps, actions, declared outputs, gates and failure branches." : l.scope === "agent" ? p.role : "Instructions → published procedure → capability / role → cited knowledge → project and card data. Source identity travels into the compiled hash.")}</p>`;
    const mainAction =
      l.designTab === "Effective configuration"
        ? button("Edit at this layer", "config-edit", "", "small", "edit")
        : l.designTab === "Review & release"
          ? ""
          : button("Edit selection", "edit-selection", "", "small", "edit");
    const leftPanel =
      l.designTab === "Effective configuration"
        ? panel(
            "Organisation layers",
            `<div class="l-tree l-layer-tree">${orgConfig.layers
              .map(
                ([layer, value], i) =>
                  `<div class="l-layer ${i === orgConfig.layers.length - 1 ? "selected" : ""}"><span class="eyebrow">${E(layer.toUpperCase())}</span><span>${I(i === 0 ? "layers" : i === orgConfig.layers.length - 1 ? "agent" : "flow")}<strong>${E(value)}</strong></span></div>`,
              )
              .join("")}</div>`,
            B(orgConfig.layers.length + " layers"),
            "",
          )
        : panel(
            "Context hierarchy",
            `<div class="l-tree"><span class="eyebrow">TENANT / SHARED FOUNDATION</span><button data-l="design-scope" data-scope="capability" class="${l.scope === "capability" ? "selected" : ""}">${I("grid")}<span><strong>${E(p.label)}</strong><small>Pack binding · policy · readiness</small></span></button><button data-l="design-scope" data-scope="process" class="child ${l.scope === "process" ? "selected" : ""}">${I("flow")}<span><strong>${E(p.subject)}</strong><small>Intent · step source selection</small></span></button><button data-l="design-scope" data-scope="agent" class="grandchild ${l.scope === "agent" ? "selected" : ""}">${I("agent")}<span><strong>${E(nameOf("agents", p.agent))}</strong><small>Role · grants · memory policy</small></span></button><button data-l="design-scope" data-scope="run" class="child ${l.scope === "run" ? "selected" : ""}">${I("run")}<span><strong>Resolved call</strong><small>Sources · budget · provenance</small></span></button><span class="eyebrow">OTHER CONTEXT PLANES</span>${route("Project & work-package memory", "memory", "quiet small", "layers")}${route("Knowledge effectiveness eval", "evals", "quiet small", "chart")}</div>`,
            B("4 layers"),
            "",
          );
    return `<div class="page enterprise learning">${heading("Knowledge design", "Design what a capability knows, what each process needs, and what each agent receives for this step.", route("Sources & ingestion", "knowledge", "", "book"), "KNOWLEDGE & MEMORY")}${scopeBar(B("Contract draft · r" + l.bindingRevision, "purple"))}${workflowStrip()}<div class="l-grid">${leftPanel}
    <div>${panel(mainTitle, `<div class="l-crumb">${crumb}</div>${intro}${tabs(["Effective configuration", "Knowledge contract", "Memory policy", "Readiness & impact", "Review & release"], l.designTab, "designTab")}${l.designTab === "Effective configuration" ? effectiveConfiguration() : l.designTab === "Review & release" ? reviewRelease() : l.designTab === "Knowledge contract" ? knowledgeContract(chosen) : l.designTab === "Memory policy" ? memoryPolicy() : readiness()}`, mainAction)}${l.designTab === "Effective configuration" || l.designTab === "Review & release" ? "" : panel(
      "Effective context · inherited → selected → pinned",
      table(
        ["Layer", "Definition", "Effective for this call"],
        [
          `<tr><td>Capability</td><td>capability@17<small>metadata.capability = ${E(p.pack)}</small></td><td>${B("Inherited", "green")}<small>Required · minimum score 0.72</small></td></tr>`,
          `<tr><td>Process / step</td><td>${E(p.subject)}<small>${E(p.intent)}</small></td><td>${B("Selected")}<small>${chosen.length} domain sources · published steps</small></td></tr>`,
          `<tr><td>Agent binding</td><td>${E(p.recipe)}<small>${E(p.agent)} · declared role</small></td><td>${B("Narrowed")}<small>Source ceiling + permitted tools + same-card recall</small></td></tr>`,
          `<tr><td>Run / call</td><td>${E(p.job)} / CALL-774<small>Procedure + executor + pack versions</small></td><td>${B(l.knowledgeOutage || !chosen.length ? "Blocked" : "Pinned", l.knowledgeOutage || !chosen.length ? "amber" : "green")}<small>Measured at compile time</small></td></tr>`,
        ],
      ),
      "",
      "",
    )}</div><aside class="l-inspector">${l.designTab === "Effective configuration" ? configInspector() : l.designTab === "Review & release" ? reviewInspector() : `${panel(
      "Resolution preview",
      `<span class="eyebrow">FOR THE SELECTED STEP</span><h2>${chosen.length && !l.knowledgeOutage ? "Ready" : "Blocked"}</h2>${rows(
        [
          ["Job", E(p.job)],
          ["Pack", "<code>capability@17</code>"],
          ["Filter", E(p.pack)],
          ["Domain sources", `${chosen.length} selected / 3 available`],
          ["Procedure", "Published exact version"],
          ["Memory", E(effectiveMemory())],
          ["Role", E(p.agent)],
        ],
      )}<div class="l-budget"><span style="width:18%"></span><span style="width:22%;opacity:.8"></span><span style="width:12%;opacity:.6"></span><span style="width:16%;opacity:.4"></span></div><p>8,160 / 12,000 characters · fixture measurement. Provider tokens are recorded separately.</p>${button("Compile preview", "compile", "", "primary", "code")}${button(l.knowledgeOutage ? "Restore source" : "Simulate missing source", "design-outage", "", "small")}${note("Version boundary", "Saving a selection revises this design draft. RUN-8821 retains its published procedure, executor and capability@17 pins.")}`,
    )}${panel("From knowledge to improvement", `<p>A weak citation, missing source or stale memory becomes a classified finding with a measurable hypothesis.</p>${button("Open improvement control", "design-improve", "", "small", "chart")}${route("Inspect scope & memory", "memory", "small", "layers")}`)}`}</aside></div>${boundary("Capability bindings and work-package memory follow S15. The effective job-release configuration and review queue are V2 proposals; they do not create a separate pack or an automatic recall across projects.")}</div>`;
  }
  function effectiveConfiguration() {
    const view = l.configView;
    const draftValue = (s) => {
      if (s.conflict && !l.configResolved) return "department_conflict — unresolved";
      if (l.configDraft && l.configDraft.key === s.key) return l.configDraft.value;
      return s.draft;
    };
    const records = orgConfig.settings.map((s) => {
      const dv = draftValue(s);
      if (view === "Current release")
        return `<tr><td><strong class="mono">${E(s.key)}</strong><small>${E(s.kind)}</small></td><td>${E(s.origin)}</td><td>${E(s.current)}</td><td>${s.required ? B("required", "green") : B("optional")}</td></tr>`;
      if (view === "Draft") {
        const conflict = s.conflict && !l.configResolved;
        const draft = l.configDraft && l.configDraft.key === s.key;
        return `<tr class="${conflict ? "l-row-conflict" : ""}"><td><strong class="mono">${E(s.key)}</strong><small>${E(s.kind)}</small></td><td>${E(s.origin)}</td><td>${conflict ? B("department_conflict", "amber") + "<br>" + E(dv) : draft ? B("job-layer draft", "purple") + "<br>" + E(dv) : E(dv)}</td><td>${s.required ? B("required", "green") : B("optional")}${draft ? B("override", "purple") : ""}</td></tr>`;
      }
      const changed = s.current !== dv;
      return `<tr><td><strong class="mono">${E(s.key)}</strong></td><td>${E(s.current)}</td><td>${E(dv)}</td><td>${changed ? B("changed", "amber") : B("unchanged")}</td></tr>`;
    });
    const heads =
      view === "Current release"
        ? ["Setting", "Originating layer", "Current release value", "Required"]
        : view === "Draft"
          ? ["Setting", "Originating layer", "Draft value", "Override / requirement"]
          : ["Setting", "Current release", "Draft", "Change"];
    const ready = configReady();
    const status =
      view === "Current release"
        ? note(
            "Published release",
            "This is the binding's current pin and is immutable. Switch to Draft to propose a change, or Compare to see the difference.",
          )
        : l.configMissing
          ? note("409 release_not_ready", "Required kind <code>domain.identity</code> has zero resolvable leaves in department <strong>verification</strong>. Add a job-layer row or restore the source; no run may start against this release.", "amber")
          : l.configConflict && !l.configResolved
            ? note("department_conflict", `<strong>delivery</strong> sets <code>domain.identity</code> to ${E(orgConfig.conflict.delivery)}. <strong>verification</strong> sets it to ${E(orgConfig.conflict.verification)}. A sibling conflict is a compile error; only a job-layer row resolves it.`, "amber")
            : note(ready ? "Release draft resolves" : "Release blocked", ready ? "Every required kind resolves and the department conflict is resolved at the job layer. Opening the review gate does not publish the release." : "The release cannot open its review gate.", ready ? "" : "amber");
    const draftControls =
      view === "Current release"
        ? ""
        : `${l.configConflict && !l.configResolved ? button("Resolve at job layer", "config-resolve", "", "primary", "shield") : ""}${l.configMissing ? button("Restore the missing kind", "config-missing", "", "small") : button("Simulate a missing required kind", "config-missing", "", "small")}`;
    return `<div class="v2-ribbon" role="note"><strong>V2 · PROPOSED</strong><span>Effective job-release configuration over industry → ordered departments → job, with <code>department_conflict</code> resolution at the job layer. Proposed in T0/T3; local fixture.</span><span class="v2-src">No write path</span></div>${rows([["Job", `<span class="mono">${E(orgConfig.job)}</span>`], ["Draft release", `<span class="mono">${E(orgConfig.release)}</span> · revision ${l.configRevision}`], ["Current release", `<span class="mono">${E(orgConfig.currentRelease)}</span> · immutable`], ["Scope", "one industry · ordered departments · no region"]])}<div class="l-config-controls" role="group" aria-label="Configuration view">${["Current release", "Draft", "Compare"].map((v) => button(v, "config-view", `data-value="${v}" aria-pressed="${v === view}"`, v === view ? "selected" : "")).join("")}</div>${table(heads, records)}${status}${draftControls}${button("Open review & release", "review-open", "", "small", "shield")}<h3>Dependents of this draft</h3>${table(["Object", "Kind", "Impact"], orgConfig.consumers.map(([o, k, i]) => `<tr><td>${E(o)}</td><td>${E(k)}</td><td>${E(i)}</td></tr>`))}${note("Draft, not release", "Editing creates a draft job-layer row and a review gate. The current release and every existing run pin remain unchanged.")}`;
  }
  function configInspector() {
    return `${panel(
      "Release readiness",
      `${rows([
        ["Release", E(orgConfig.release)],
        ["Current pin", E(orgConfig.currentRelease)],
        ["Required kinds", configReady() ? "resolved" : "blocked"],
        ["Conflict", l.configResolved ? "resolved at job layer" : l.configConflict ? "department_conflict" : "none"],
        ["Dependents", orgConfig.consumers.length + " objects"],
        ["Authority", "capability owner + tenant admin"],
      ])}${B(configReady() ? "Ready for review" : "Blocked", configReady() ? "green" : "amber")}${note("No publish from here", "The review queue records the person, rationale and evidence. Publishing is a separate gate and never rewrites a live binding.")}${button("Open review & release", "review-open", "", "small", "shield")}`,
    )}${panel("Why this view", `<p class="muted">The capability contract answers “what may this capability know?”. The job release answers “what does this position actually compile?”. Both are visible; neither silently substitutes for the other.</p>${route("Sources & retrieval", "knowledge", "small", "book")}`)}`;
  }
  function reviewRelease() {
    const item =
      orgConfig.reviewItems.find((x) => x.id === l.reviewItem) ||
      orgConfig.reviewItems[0];
    const stateOf = (x) => l.reviewStates[x.id] || x.state;
    const state = stateOf(item);
    const count = (s) => orgConfig.reviewItems.filter((x) => stateOf(x) === s).length;
    return `<div class="v2-ribbon" role="note"><strong>V2 · PROPOSED</strong><span>Review queue and change-set lifecycle (Draft → In review → Approved → Published → Superseded / Rolled back). The existing approval rows are the seed; no write path.</span><span class="v2-src">Person authority</span></div>${metrics([
      ["Open items", String(orgConfig.reviewItems.length), "across three surfaces"],
      ["Awaiting a person", String(count("In review") + count("Draft")), "no agent approval"],
      ["Published this cycle", String(count("Published")), "immutable"],
      ["Agent approvals", "0", "403 human_approval_required"],
    ])}${table(
      ["Item", "Kind", "Reviewer", "Decisions", "Evidence", "Age", "State"],
      orgConfig.reviewItems.map(
        (x) =>
          `<tr class="${x.id === item.id ? "selected" : ""}"><td><button class="e-project-link" data-l="review-select" data-id="${x.id}"><span><strong>${E(x.object)}</strong><small class="mono">${E(x.id)}</small></span></button></td><td>${E(x.kind)}</td><td>${E(x.reviewer)}</td><td>${E(x.decisions)}</td><td><small>${E(x.evidence)}</small></td><td>${E(x.age)}</td><td>${B(stateOf(x), stateOf(x) === "Published" ? "green" : stateOf(x) === "Draft" ? "amber" : "")}</td></tr>`,
      ),
    )}<h3>Selected change set</h3>${rows([
      ["Object", E(item.object)],
      ["Required reviewer", E(item.reviewer)],
      ["Decisions", E(item.decisions)],
      ["Evidence", E(item.evidence)],
      ["Model / agent approval", B("forbidden", "amber")],
      ["State history", ["Draft", "In review", "Approved", "Published", "Superseded"].map((s) => B(s, s === state ? "purple" : "")).join(" ")],
    ])}${state === "Published" ? note("Published releases are immutable", "A correction creates a new version; rollback creates a new version and keeps this one in the supersession audit.") : state === "Superseded" ? note("Superseded", "The new draft is linked to this published version; the original remains visible in the audit.") : note("Gate", "Advancing from In review to Approved requires a person's rationale and the evidence named above.")}<div class="row">${button(state === "Published" ? "Published · immutable" : "Advance state", "review-advance", "", "primary", "shield")}${button("Roll back", "review-rollback", "", "small", "replay")}${button("Attempt agent approval", "review-agent", "", "small", "agent")}</div>`;
  }
  function reviewInspector() {
    return `${panel(
      "Review authority",
      `${rows([
        ["Queue owner", "tenant admin"],
        ["Source corrections", "source owner"],
        ["Pack versions", "capability owner"],
        ["Binding revisions", "tenant admin · dual control"],
        ["Lesson promotion", "capability owner + ≥2 evidence refs"],
        ["Agent approval", B("forbidden", "amber")],
      ])}${note("The queue is the gate", "A queue row is a draft change set. Publication is a separate immutable release; the person, rationale and evidence are recorded on the approval.")}${route("Open procedures", "procedures", "small", "flow")}`,
    )}${panel("Sources & memory", `<p class="muted">A correction or promotion arrives here from the source, passage, memory-record or policy inspector. The queue never approves its own proposal.</p>${route("Open sources & retrieval", "knowledge", "small", "book")}${route("Open memory explorer", "memory", "small", "layers")}`)}`;
  }

  function knowledgeContract(chosen) {
    const p = profile();
    return `${rows([
      ["Binding owner", E(p.label)],
      ["Pack root", "<code>capability@17</code> · one versioned platform pack"],
      [
        "Tenant layer",
        "Approved tenant sources may supersede platform passages",
      ],
      ["Use in process", E(p.intent)],
      [
        "Required source policy",
        "Refuse before a model call if a required selection cannot resolve",
      ],
    ])}<h3 style="margin-top:22px">Domain sources selected for ${E(l.scope)}</h3>${p.knowledge.map((title, i) => `<div class="l-source">${I(i === 2 ? "shield" : "book")}<div class="grow"><strong>${E(title)}</strong><small>${i === 2 ? "Tenant layer · supersedes platform guidance where declared" : "Platform layer · capability@17 · source provenance preserved"}<br>${i === 0 ? "Procedure selects applicable ADM phase and control objectives" : i === 1 ? "Agent selects concepts, relations and validation rules for this step" : "Project restrictions remain subordinate to approved principles"}</small></div>${B(chosen.includes(i) ? "Selected" : "Excluded", chosen.includes(i) ? "green" : "")}</div>`).join("")}${note("Selection is a narrowing boundary", "A process or agent cannot widen the tenant, capability or permitted source scope by changing a prompt. Source versions and selection reasons appear in the call manifest.", "blue")}`;
  }
  function memoryPolicy() {
    return `${note("Effective recall policy", E(effectiveMemory()) + ". Scope: " + E(l.scope) + ". A child may narrow its parent’s read policy; procedural execution history is always retained.", "blue")}${button("Edit memory selection", "edit-memory-policy", "", "small", "edit")}${table(
      ["Scope", "Read / write policy", "Authority"],
      [
        "<tr><td>Tenant / programme / project</td><td>Approved governed sources only<small>No automatic pool of every agent’s notes</small></td><td>Explicit binding<small>Proposed curation UI</small></td></tr>",
        "<tr><td>Capability / process / agent</td><td>Review authorised history; propose a reusable lesson<small>Original events remain immutable</small></td><td>Projection + human review</td></tr>",
        "<tr><td>Work package</td><td>memory.remember / memory.recall<small>Facts, claims, verdicts, attempts, decisions, blockers</small></td><td>Tenant + project + card</td></tr>",
        "<tr><td>Run / attempt</td><td>Execution history and compiled source manifest<small>Agent context is ephemeral; run history persists</small></td><td>Append-only events</td></tr>",
      ],
    )}${note("Hot rows win", "Recall is restricted to <code>conveyor:work:&lt;card_id&gt;</code>. Cold digests are derived; they do not replace the transactional record. Promotion into capability or tenant knowledge is a reviewed change.")}${route("Open memory explorer", "memory", "small", "layers")}`;
  }
  function readiness() {
    const blocked = l.knowledgeOutage || !effectiveSources().length;
    return `${note(blocked ? "409 capability_not_ready" : "All selected sources resolve", blocked ? "Required knowledge selection cannot resolve. No run or step is created and no inference call is made." : "Procedure, recipe, registered action grants and pinned domain knowledge resolve before the first step.", blocked ? "amber" : "")} ${rows(
      [
        [
          "Required pack",
          l.knowledgeOutage
            ? B("Unavailable", "amber")
            : B("Resolved · @17", "green"),
        ],
        [
          "Selection",
          !effectiveSources().length
            ? B("Empty required section", "amber")
            : B("Within capability scope", "green"),
        ],
        ["Procedure", "Published @2 · frozen"],
        ["Agent grants", "Registered actions only · capped internal executor"],
        ["Affected use", "New starts only; active run pins remain unchanged"],
        ["During execution", "Loss of knowledge yields knowledge_degraded"],
      ],
    )}${route("Inspect execution pins", "execution", "small", "run")}`;
  }
  function memoryExplorer() {
    const groups = [
      ["Organisation", ["Tenant", "Industry", "Department", "Job"]],
      ["Work", ["Programme", "Project", "Work package"]],
      ["Execution", ["Capability", "Process", "Agent", "Run / attempt", "Step / call"]],
      ["Lifecycle", ["Source", "Published knowledge", "Digest", "Lesson"]],
    ];
    const levelInfo = {
      Tenant: ["Curated institutional knowledge", "layers"],
      Industry: ["Industry baseline and obligations", "layers"],
      Department: ["Department archetype and decisions", "layers"],
      Job: ["Job release pin and configuration", "agent"],
      Programme: ["Decision & constraint projections", "layers"],
      Project: ["Approved project decisions", "layers"],
      "Work package": ["Hot facts, claims & verdicts", "file"],
      Capability: ["Cross-run lesson candidates", "flow"],
      Process: ["Procedural execution history", "flow"],
      Agent: ["Executor history projection", "flow"],
      "Run / attempt": ["Events, approvals & calls", "file"],
      "Step / call": ["Inference and tool-call records", "file"],
      Source: ["Source review and provenance", "file"],
      "Published knowledge": ["Immutable pack releases", "book"],
      Digest: ["Derived cold summaries", "layers"],
      Lesson: ["Reviewed reusable lessons", "branch"],
    };
    const visibleGroups =
      l.memoryDim === "All"
        ? groups
        : groups.filter(([g]) => g === l.memoryDim);
    const records = memoryRecords.filter((m) => m.level === l.memoryScope);
    const m =
      records.find((r) => r.id === l.memoryItem) ||
      records[0] ||
      memoryRecords[0];
    const excluded = l.excluded[m.id];
    const recordList = records.length
      ? records
          .map(
            (r) =>
              `<button class="l-record ${r.id === m.id ? "selected" : ""}" data-l="memory-select" data-id="${r.id}"><div class="l-tagline">${B(r.type || r.kind, "purple")}${B(r.status, r.status === "Claim" || r.status === "Needs evidence" ? "amber" : "green")}${l.excluded[r.id] ? B("recall excluded", "amber") : ""}${l.successor && r.id === "m-card" ? B("corrected", "purple") : ""}</div><strong>${E(r.title)}</strong><p>${E(r.body)}</p><small class="mono muted">${E(r.source)}</small></button>`,
          )
          .join("")
      : `<div class="empty">No fixture records at this level. The view exists; it is empty because no row resolves here.</div>`;
    const levelTree = visibleGroups
      .map(
        ([group, list]) =>
          `<span class="eyebrow">${E(group.toUpperCase())}</span>${list
            .map(
              (s) =>
                `<button data-l="memory-level" data-value="${s}" class="${s === l.memoryScope ? "selected" : ""}">${I(levelInfo[s][1])}<span><strong>${E(s)}</strong><small>${E(levelInfo[s][0])}</small></span></button>`,
            )
            .join("")}`,
      )
      .join("");
    return `<div class="page enterprise learning">${heading("Memory explorer", "See what happened, what was learned, and where a lesson is allowed to travel.", button("Recall for a work package", "recall", "", "primary", "search"), "KNOWLEDGE & MEMORY")}<div class="l-scope">${I("shield")}<strong>Non-Agentic AI</strong><span> / Civic transformation / Trust & identity</span><span class="grow"></span>${B("Hot record → derived digest → reviewed knowledge")}</div>${metrics(
      [
        ["Record of truth", "Conveyor", "Events + transactional memory"],
        ["Recall boundary", "1 card", "Tenant + project + work package"],
        ["Cold memory", "strata_rag", "Derived digest; hot wins"],
        ["Wider reuse", "Reviewed", "Explicit source promotion"],
      ],
    )}<div class="l-grid">${panel("Memory levels", `<div class="l-dimensions" role="group" aria-label="Memory dimensions">${["All", "Organisation", "Work", "Execution", "Lifecycle"].map((g) => button(g, "memory-dim", `data-value="${g}" aria-pressed="${g === l.memoryDim}"`, g === l.memoryDim ? "selected" : "", "")).join("")}</div><div class="l-tree l-level-tree">${levelTree}</div>`, "", "")}<div>${panel(l.memoryScope + " memory", `${tabs(["Records", "Recall & isolation", "Promotion trail"], l.memoryTab, "memoryTab")}${l.memoryTab === "Records" ? recordList + (l.policyCompare ? memoryPolicyCompare() : "") : l.memoryTab === "Recall & isolation" ? recallView() : promotionTrail()}`, B(m.proposal ? "Proposed scope view" : "S15 record / projection", m.proposal ? "purple" : ""))}${panel(
      "Physical storage ≠ navigation scope",
      rows([
        [
          "Procedural knowledge",
          "Published capability → procedure → step → action / gate",
        ],
        [
          "Domain knowledge",
          "Pinned strata_rag capability pack + tenant source layer",
        ],
        [
          "Procedural memory",
          "Conveyor runs, steps, approvals, invocations and events",
        ],
        ["Work-package memory", "Hot Conveyor rows + cold derived card digest"],
        [
          "Higher-level views",
          "Authorised projections and curated sources; no additional autonomous memory stores",
        ],
      ]),
    )}</div><aside class="l-inspector">${panel("Record inspector", memoryInspector(m, excluded))}</aside></div>${boundary("S15 defines procedural and card memory. The record taxonomy, Current/History/Derivation views, successor and exclusion actions and the dimension selector are workbench proposals; one record can appear in several authorised views without creating several stores.")}</div>`;
  }
  function memoryInspector(m, excluded) {
    const view = l.memoryView;
    const views = `<div class="l-record-views" role="group" aria-label="Record views">${["Current", "History", "Derivation"]
      .map(
        (v) =>
          `<button class="${v === view ? "selected" : ""}" data-l="memory-view" data-value="${v}" aria-pressed="${v === view}">${E(v)}</button>`,
      )
      .join("")}</div>`;
    let body = "";
    if (view === "Current") {
      body = `${rows([
        ["What is this?", `${B(m.type || m.kind, "purple")} ${B(m.status, m.status === "Claim" || m.status === "Needs evidence" ? "amber" : "green")}`],
        ["Where did it originate?", `${E(m.source)}<br><small>${E(m.derivedFrom || m.store)}</small>`],
        ["When applicable?", `${E(m.recordedAt || "—")} · valid ${E(m.validFrom || "—")} → ${E(m.validTo || "—")}`],
        ["Who can use it?", `Human inspection: ${E(m.humanAccess || "authorised view")}<br>Agent recall: ${excluded ? B("excluded · " + E(excluded), "amber") : E(m.agentRecall || "not eligible")}`],
        ["What changed?", l.successor && m.id === "m-card" ? "Successor m-card-2 recorded; this record is retained." : E(m.supersededBy && m.supersededBy !== "—" ? "Superseded by " + m.supersededBy : "No successor recorded")],
        ["Did it help?", `${E(m.recallUsage || "not measured")} · ${E(m.evalRefs || "no eval reference")}`],
      ])}${m.level === "Work package" ? note("Claim is not a verified fact", "Producer output cannot establish verification. Observer evidence and a verdict remain separately traceable.", "amber") : ""}${m.proposal ? note("Design extension", "This broader view projects existing decisions or proposes curation. Production memory.recall remains scoped to a single card.", "blue") : ""}`;
    } else if (view === "History") {
      body = `${note(l.successor ? "Successor recorded" : "No corrections yet", l.successor ? "m-card-2 supersedes this record. The original remains the record of truth for its time; the successor is a draft until a person reviews it." : "Corrections are added as successors, never overwrites. The original and its validity period are retained.", l.successor ? "" : "blue")}${rows([
        ["Original", E(m.body)],
        ["Recorded", E(m.recordedAt || "—")],
        ["Supersedes", E(m.supersedes || "—")],
        ["Superseded by", l.successor && m.id === "m-card" ? "m-card-2 · successor draft" : E(m.supersededBy || "—")],
        ["Original retained", "yes · never overwritten"],
      ])}${l.successor && m.id === "m-card" ? `<div class="l-successor">${B("successor", "purple")}<p>${E(l.successorText)}</p><small class="mono muted">m-card-2 · supersedes m-card · review required</small></div>` : button("Correct by adding successor", "memory-successor", "", "small", "edit")}`;
    } else {
      const hasDigest = m.level === "Work package" || m.level === "Digest";
      body = hasDigest
        ? `${note("Cold digest · derived, never the record of truth", E(cardDigest.text), "blue")}<h3>Included hot rows</h3><ul class="l-list">${cardDigest.included.map((x) => `<li>${E(x)}</li>`).join("")}</ul><h3>Omitted</h3><ul class="l-list">${cardDigest.omitted.map((x) => `<li>${E(x)}</li>`).join("")}</ul><h3>Conflicts</h3><ul class="l-list">${cardDigest.conflicts.map((x) => `<li>${E(x)}</li>`).join("")}</ul>${l.digestRebuilt ? note("Digest rebuilt v5", "The dry-run diff was confirmed. The hot rows are unchanged and the previous digest is retained in the history.") : button("Rebuild digest (dry run)", "memory-digest", "", "small", "layers")}`
        : note("No cold digest at this level", "This level is an authorised projection or an append-only record. A digest exists only for a work package and its derived form.", "blue");
    }
    return `<span class="eyebrow">${E(m.level.toUpperCase())} / ${E(m.id)}</span><h2>${E(m.title)}</h2>${views}${body}<div class="row">${m.level === "Work package" ? button("Propose reusable lesson", "promote-memory", "", "small", "branch") : ""}${button(excluded ? "Recall excluded" : "Exclude from recall", "memory-exclude", excluded ? "disabled" : "", "small", "close")}${button("Correct by adding successor", "memory-successor", "", "small", "edit")}${button("Compare memory policy", "memory-policy-compare", "", "small", "chart")}${button("Evaluate this record", "eval-object", 'data-subject="memory"', "small", "chart")}${button("Open originating evidence", "memory-origin", `data-id="${m.id}"`, "small", "file")}</div>`;
  }
  function memoryPolicyCompare() {
    return panel(
      "Memory policy comparison",
      `${table(
        ["Arm", "Recall breadth", "Cases", "Delta", "Cost"],
        [
          "<tr><td>Memory disabled</td><td>no recall</td><td>80 held-out</td><td>baseline</td><td>$0.00</td></tr>",
          "<tr><td>Current policy</td><td>same-card hot + cold</td><td>80 held-out</td><td>+3 pp</td><td>$0.42</td></tr>",
          "<tr><td>Candidate policy</td><td>same-card hot only</td><td>80 held-out</td><td>+5 pp · review the band</td><td>$0.38</td></tr>",
        ],
      )}${note("Separate the judgements", "Measured outcome, model assessment and human review stay separate. The candidate is not adopted until a person approves it; a training-only gain is reverted.")}${button("Open object evaluation", "eval-object", 'data-subject="memory"', "small", "chart")}${button("Evaluate scope policy", "eval-object", 'data-subject="scope"', "small", "chart")}`,
    );
  }
  function recallView() {
    return `<form id="l-recall-form"><label class="field"><span>Recall within work package</span><select name="card"><option>WP-2048</option><option>WP-2061</option></select></label>${F("Question", "query", l.memoryQuery)}<button class="btn primary">Recall local fixture</button></form>${l.recall && l.recallNotice === "No fixture memory for WP-2061" ? note("No fixture memory for WP-2061", "No rows returned. Memory from WP-2048 is excluded by the card boundary.", "blue") : l.recall ? note(l.recallNotice || "2 records · WP-2048 only", `Hot: delegation claim remains unverified.<br>Cold: previous attempt summary, source_id <code>conveyor:work:WP-2048</code>.<br>Hot rows win on conflict. No facts from other cards or projects are included.`) : note("Explicit access boundary", "The memory scope tree changes what a person can inspect. It does not grant an agent broader recall.", "blue")}`;
  }
  function promotionTrail() {
    return `${note(l.promoted ? "Approved tenant source draft" : l.promotionDraft ? "Candidate waiting for owner review" : "No lesson promoted", l.promoted ? "The person’s decision creates a draft source registration for curation. Publishing knowledge and updating a capability binding remain separate actions." : "A raw card claim can become a lesson candidate with its claim status intact. Verification and owner review are required before wider reuse.", l.promoted ? "" : "amber")}${rows(
      [
        ["Origin", "WP-2048 → RUN-8821 → INV-441"],
        [
          "Candidate",
          l.promotionDraft
            ? "LESSON-017 · retained as unverified claim"
            : "Not proposed",
        ],
        [
          "Owner review",
          l.promoted
            ? "George · person · Web · rationale recorded"
            : "Awaiting independent evidence",
        ],
        ["Destination", "Tenant source draft · architecture metadata"],
        [
          "Evidence references",
          l.promoted
            ? "2 independent · WP-2048/RUN-8821 + WP-2061/INC-041"
            : "1 reference · a second independent card, run or incident is required",
        ],
        ["Rule", "≥2 independent evidence references + a person's review before promotion"],
        ["Active run", "Retains capability@17; no automatic re-binding"],
      ],
    )}${button("Propose reusable lesson", "promote-memory", "", "small", "branch")}${route("Review knowledge sources", "knowledge", "small", "book")}`;
  }
  function improvement() {
    const campaigns = [
      [
        "architecture",
        "Architecture acceptance",
        "Knowledge selection",
        "Eval hillclimb",
        "Triage required",
        "hillclimb",
      ],
      [
        "delivery",
        "Verified coding outcomes",
        "Procedure / executor",
        "Benchmark kata",
        "Candidate comparison",
        "benchmarks",
      ],
      [
        "discovery",
        "Requirements baseline quality",
        "Rubric calibration",
        "Eval audit",
        "Inputs review",
        "evals",
      ],
      [
        "verification",
        "Assurance execution cost",
        "Cache / model / effort",
        "Cost hillclimb",
        "Plan sign-off",
        "cost",
      ],
    ].filter(
      (c) => l.campaignScope === "All capabilities" || c[0] === l.campaignScope,
    );
    return `<div class="page enterprise learning">${heading("Improvement control", "Improve the operating model with evidence. Every candidate names the capability, process, agent and context it changes.", button("New improvement plan", "new-campaign", "", "primary", "plus"))}<div class="l-scope">${I("chart")}<strong>Tenant improvement portfolio</strong><span>Independent of project delivery state</span><span class="grow"></span><label>View <select data-l-change="campaign" aria-label="Filter improvement capability"><option>All capabilities</option>${Object.entries(
      profiles,
    )
      .map(
        ([id, p]) =>
          `<option value="${id}" ${l.campaignScope === id ? "selected" : ""}>${E(p.label)}</option>`,
      )
      .join("")}</select></label></div>${metrics([
      ["Active experiments", "4", "Across 4 capabilities"],
      [
        "Best held-out delta",
        "+15 pp",
        "Architecture · frozen instrument",
        "l-green",
      ],
      ["Human control points", "3", "Plan / grading / adoption", "l-amber"],
      ["Spend discipline", "$300", "Signed ceiling per quality campaign"],
    ])}<div class="l-flow">${[
      [
        "01 / OBSERVE",
        "Find the constraint",
        "Failures · costs · missing evidence",
      ],
      ["02 / DESIGN", "One hypothesis", "Source, procedure, role or model"],
      ["03 / MEASURE", "Frozen instrument", "Eval or benchmark controls"],
      ["04 / DECIDE", "Keep / revert / triage", "Held-out result + guardrails"],
      ["05 / ADOPT", "Review a new version", "Future runs · published pins"],
    ]
      .map(
        ([a, b, c], i) =>
          `<div class="${i === 3 ? "current" : ""}"><small>${a}</small><strong>${b}</strong><span>${c}</span></div>`,
      )
      .join("")}</div><div class="l-dual"><div>${panel(
      "Improvement campaigns",
      table(
        [
          "Capability / objective",
          "Change surface",
          "Instrument",
          "Next decision",
        ],
        campaigns.map(
          ([cap, title, surface, kind, status, dest]) =>
            `<tr><td><strong>${E(title)}</strong><small>${E(profiles[cap].label)}</small></td><td>${E(surface)}<small>Pinned candidate version</small></td><td>${E(kind)}<small>${cap === "delivery" ? "verified50-v1 · controls required" : "Version + split + grader fixed"}</small></td><td>${button(status, "campaign-open", `data-dest="${dest}" data-cap="${cap}"`, "small", "arrow")}</td></tr>`,
        ),
      ),
      B("Illustrative portfolio"),
      "",
    )}${panel(
      "Version lineage · architecture",
      `<div class="l-chain">${[
        ["PUBLISHED", "Process @2"],
        ["CANDIDATE", "Source selection r4"],
        ["MEASUREMENT", "Eval @3 · v1"],
        ["ADOPTION", "New procedure draft"],
      ]
        .map(
          ([a, b], i) =>
            `<button data-l="lineage" data-index="${i}"><small>${a}</small><strong>${b}</strong></button>`,
        )
        .join("")}</div>${rows([
        [
          "Baseline run",
          "EV-400 · recipe solution_architect@2 · capability@17",
        ],
        ["Candidate run", "EV-401 · same instrument / split / grader"],
        ["Observed signal", "Held-out 70% → 85%; train 72% → 84%"],
        [
          "Decision boundary",
          "Kept experiment; publication still needs a person",
        ],
      ])}`,
    )}</div><aside>${panel("The next useful intervention", `<span class="eyebrow l-amber">ARCHITECTURE / TWO FLAT ROUNDS</span><h2>Stop changing prose. Classify the failures.</h2><p>The last two rounds did not move held-out acceptance. The next round must examine artifact gaps, grader disagreement, harness failures, structural limits and variance.</p>${button("Open stall triage", "campaign-open", 'data-dest="hillclimb" data-cap="architecture"', "primary", "branch")}`)}${panel("Two measurement instruments", `<h3>Task evaluations</h3><p>Tenant-authored cases and outcome metrics, frozen versions, sign-offs and a held-out wall. Quality and cost climbs consume this instrument.</p><h3>Benchmark harness</h3><p>Hashed SWE-bench seed, gold / empty controls, parity grader, paired comparisons. Kata consumes its public actions.</p>${route("Inspect benchmark controls", "benchmarks", "small", "code")}`)}${panel("Audit feeds the same loop", `<p>Prompt findings are candidate inputs, linked to recipe version, compiled hash and inference calls.</p>${route("Inference & prompt audit", "usage", "small", "chart")}`)}</aside></div>${boundary("The eval climb rules exist in crates/domain/src/eval/climb.rs. Portfolio coordination, source selection experiments, S18 subject benchmark arms and adoption review are design projections; no experiment runs against a live service here.")}</div>`;
  }
  function chart() {
    const pts = rounds.filter((r) => typeof r.train === "number"),
      maxX = Math.max(3, ...pts.map((r) => r.n));
    const x = (n) => 55 + (n * 490) / maxX,
      y = (n) => 185 - (n - 50) * 2.8;
    return `<div class="l-chart"><div class="l-chart-legend"><span><i class="test"></i>Held-out acceptance</span><span><i class="train"></i>Train acceptance</span><span>Decision band: ${l.delta} pp</span></div><div class="l-chart-scroll"><svg viewBox="0 0 600 235" role="img" aria-label="Train and held-out acceptance by round. Baseline 72 and 70 percent; best round 84 and 85 percent. Further training gains leave held-out unchanged.">${[50, 60, 70, 80, 90, 100].map((n) => `<path d="M48 ${y(n)}H565" stroke="var(--line)"/><text x="7" y="${y(n) + 4}">${n}%</text>`).join("")}<path d="M${pts.map((r) => `${x(r.n)},${y(r.train)}`).join(" L")}" fill="none" stroke="var(--l-blue)" stroke-width="2" stroke-dasharray="5 5"/><path d="M${pts.map((r) => `${x(r.n)},${y(r.test)}`).join(" L")}" fill="none" stroke="var(--accent)" stroke-width="2.5"/>${pts.map((r) => `<circle cx="${x(r.n)}" cy="${y(r.test)}" r="4" fill="var(--accent)"/><circle cx="${x(r.n)}" cy="${y(r.train)}" r="3" fill="var(--l-blue)"/><text x="${x(r.n) - 15}" y="215">${r.n ? "R" + r.n : "Base"}</text>`).join("")}</svg></div></div>`;
  }
  function hillclimb() {
    const cost = l.climbId === "cost",
      round = rounds.find((r) => r.n === l.roundIndex) || rounds[1];
    return `<div class="page enterprise learning">${heading("Hillclimb lab", "One hypothesis per round. Decide on held-out outcomes, then return to the person directing the climb.", `${button("Edit plan", "climb-plan", "", "", "edit")}${button(l.stopped ? "View final report" : "Stop & name winner", "climb-stop", "", "", "shield")}`, "MEASURE & IMPROVE / EVAL CLIMBS")}<div class="l-scope">${I("branch")}<label>Campaign <select aria-label="Hillclimb campaign" data-l-change="climb"><option value="quality" ${!cost ? "selected" : ""}>Architecture acceptance · quality</option><option value="cost" ${cost ? "selected" : ""}>Assurance execution · cost</option></select></label><span class="grow"></span>${B(l.stopped ? "Stopped · v1 wins" : l.signed ? "Plan signed" : "Plan sign-off required", l.signed ? "green" : "amber")}${B("eval@3 · split 91c7 · grader@2")}</div>${metrics(
      cost
        ? [
            ["Goal", "↓ cost", "Quality held ≥ 85%"],
            ["Baseline / case", "$0.20", "Illustrative priced ledger"],
            ["Adoption gates", "4", "Registered before round 1"],
            ["Search stage", "Cache", "Fixed lever order"],
          ]
        : [
            ["Held-out baseline", "70%", "80 cases × 3 reps"],
            ["Best held-out", "85%", "v1 · +15 percentage points", "l-green"],
            ["Noise floor", "6.5 pp", "1 / √(80 × 3) · band 8 pp"],
            [
              "Spend / ceiling",
              `$${rounds.reduce((s, r) => s + r.cost, 0).toFixed(0)} / $${l.ceiling}`,
              "Derived fixture call usage · all variants",
            ],
          ],
    )}${tabs(["Rounds & evidence", "Plan & boundaries", "Paired comparison", "Final report"], l.climbTab, "climbTab")}${
      l.climbTab === "Plan & boundaries"
        ? climbPlanView()
        : l.climbTab === "Paired comparison"
          ? comparisonView()
          : l.climbTab === "Final report"
            ? finalReport()
            : cost
              ? costWalk()
              : `<div class="l-dual"><div>${panel("Train can rise while held-out stands still", chart(), B("Frozen instrument"), "")}${panel(
                  "Round history",
                  table(
                    [
                      "Round / hypothesis",
                      "Train",
                      "Held-out",
                      "Cost",
                      "Decision",
                    ],
                    rounds.map(
                      (r) =>
                        `<tr class="${r.n === l.roundIndex ? "selected" : ""}"><td><button data-l="round-select" data-index="${r.n}">${r.n ? "v" + r.n : "baseline"} · ${E(r.change)}</button><small>${r.n === 1 ? "Same recipe + domain pack; one selection change" : r.n > 1 ? "Compared with kept v1" : "Published process @2"}</small></td><td class="numeric">${r.train == null ? "—" : r.train + "%"}</td><td class="numeric">${r.test == null ? "—" : r.test + "%"}</td><td class="numeric">$${r.cost.toFixed(2)}</td><td>${B(r.decision, r.decision === "Keep" ? "green" : "amber")}</td></tr>`,
                    ),
                  ),
                  button(
                    "Propose one round",
                    "propose-round",
                    "",
                    "small",
                    "plus",
                  ),
                  "",
                )}${!l.triaged ? note("Next round: stall triage required", "Two consecutive held-out deltas did not clear the band. Classify the failure before another content change. A fourth content round against the same wall is refused.", "amber") : note("Triage recorded", "The next hypothesis can address the classified artifact gap. The instrument, split and grader stay fixed.")}</div><aside>${panel(
                  "Selected round · " + (round.n ? "v" + round.n : "baseline"),
                  `<span class="eyebrow">${E(round.decision.toUpperCase())}</span><h2>${E(round.change)}</h2>${rows(
                    [
                      [
                        "Prediction",
                        round.n === 1
                          ? "Held-out +8–18 pp; falsified below +8 pp"
                          : "Improve acceptance beyond the kept version",
                      ],
                      [
                        "Change surface",
                        round.n === 1
                          ? "Process / agent source selection"
                          : "Role / context recipe draft",
                      ],
                      [
                        "Train / test",
                        `${round.train ?? "—"}% / ${round.test ?? "—"}%`,
                      ],
                      [
                        "Decision rule",
                        round.kind === "triage"
                          ? "Diagnostic round; no candidate adoption"
                          : round.kind === "repeat"
                            ? "Train ↓ / test ↑ ⇒ repeat before keeping"
                            : round.kind === "baseline"
                              ? "Reference measurement"
                              : round.kind === "overfit"
                                ? "Train ↑ / test flat ⇒ revert"
                                : round.kind === "coverage"
                                  ? "Both ↑ beyond band ⇒ keep"
                                  : "Held-out flat ⇒ revert",
                      ],
                      ["Production effect", "No published definition changed"],
                    ],
                  )}${button("Inspect patch & transcript pair", "round-evidence", "", "small", "file")}${route("Inspect knowledge selection", "knowledge-design", "small", "book")}`,
                )}${panel("Next action", `${l.stopped ? note("Climb stopped", "Winner v1 is named in the final report. Adoption remains a reviewed draft.") : !l.signed ? `${note("Sign the experiment plan", "Goal, guardrails, budget, fixed split and stopping rule must be approved before another round.", "amber")}${button("Review & sign plan", "climb-sign", "", "primary", "shield")}` : !l.triaged ? `${button("Categorise stalled failures", "triage", "", "primary", "branch")}` : `${button("Propose one round", "propose-round", "", "primary", "plus")}`}<p>A round returns to the caller. An analyzer can propose from train evidence; the held-out trace needs reviewer access.</p>`)}${panel("Held-out access", actorControl())}</aside></div>`
    }${boundary("S20 rule and action design; round results are local deterministic fixtures. A winning patch becomes a draft for a person to review. The mock never rewrites a published procedure.")}</div>`;
  }
  function actorControl() {
    return `<label class="field"><span>Preview as</span><select data-l-change="actor" aria-label="Evidence access role"><option value="reviewer" ${l.actor === "reviewer" ? "selected" : ""}>George · person / reviewer</option><option value="analyzer" ${l.actor === "analyzer" ? "selected" : ""}>Analyzer · agent</option></select></label><p>${l.actor === "reviewer" ? "evals:approve · held-out evidence and sign-offs available in this role preview." : "evals:write · train evidence only. No held-out transcript, answer key, budget or plan sign-off."}</p>`;
  }
  function climbPlanView() {
    return `<div class="l-dual">${panel(
      "Signed experiment contract",
      `${rows([
        [
          "Target",
          E(
            l.climbId === "cost"
              ? "cost_usd ↓; hold acceptance ≥ 85%"
              : l.target + " ↑",
          ),
        ],
        ["Instrument", "Architecture acceptance @3 · frozen digest 73e8"],
        ["Split / grader", "160 train / 80 held-out · split 91c7 · rubric@2"],
        [
          "Detectable delta",
          `${l.delta} percentage points; measured noise floor 6.5 pp`,
        ],
        ["Stopping", "Plateau K = 3, maximum 6 rounds, or manual stop"],
        [
          "Cost ceiling",
          `$${l.ceiling}; usage-derived spend, no independent counter`,
        ],
        [
          "Cadence",
          l.roundGate
            ? "One round per call + person’s round sign-off"
            : "One round per call under the signed plan",
        ],
        [
          "Off limits",
          "Answer key, held-out transcripts for analyzer, case set, split, graders, production definitions",
        ],
        [
          "Signature",
          l.signed
            ? "George · person · Web · plan digest recorded"
            : "Awaiting person",
        ],
      ])}${note("Allowed change surfaces", "One hypothesis can change a candidate procedure, role recipe, source selection, model / effort or allowed tools, within the signed boundaries. Changes to cases, graders or splits create a new eval version and refuse direct comparison.", "blue")}${button("Edit plan", "climb-plan", "", "small", "edit")}${button("Review & sign plan", "climb-sign", "", "primary", "shield")}`,
    )}${panel("Stop before another expensive pass", `<h3>When the signal is too small</h3><p>A predicted gain below the noise band is refused with its measured bound.</p><h3>When two rounds are flat</h3><p>The next round requires failure buckets. After three flat content rounds, further content changes are refused.</p><h3>When quality falls</h3><p>A cheaper candidate that violates a registered quality guardrail cannot win.</p>`)}</div>`;
  }
  function exactP(a, b) {
    const n = a + b;
    if (!n) return 1;
    let term = 2 ** -n,
      sum = term;
    for (let i = 1; i <= Math.min(a, b); i++) {
      term *= (n - i + 1) / i;
      sum += term;
    }
    return Math.min(1, 2 * sum);
  }
  function comparisonView() {
    return `<div class="l-report">${panel(
      "Paired held-out comparison",
      `${rows([
        ["A / B", "Baseline EV-400 / kept v1 EV-401"],
        ["Comparable", "Same eval version, grader set and fixed split"],
        ["Both pass / neither", "53 / 9"],
        ["A only / B only", "3 / 15 · 18 discordant pairs"],
        ["Paired delta", "+15 pp · rep 1, 80 paired cases"],
        [
          "Exact two-sided McNemar",
          `p = ${exactP(3, 15).toFixed(4)} · significant at 0.05`,
        ],
      ])}${note("Statistical test and operational band", "The exact test supports the paired change. The 15 pp gain also clears the signed 8 pp band; train and guardrail metrics are checked separately.")}${button("Try incompatible version", "compare-refuse", "", "small", "shield")}`,
    )}${panel("Before / after evidence", `${l.actor === "analyzer" ? note("Held-out traces restricted", "The analyzer receives train evidence only. A reviewer can inspect held-out transcript pairs; changing the UI preview role models the access distinction.", "amber") : transcriptPairs()}${actorControl()}`)}</div>`;
  }
  function transcriptPairs() {
    return `<div class="l-pair"><small>PAIR 1 · AC-161 / DELEGATION</small><p><strong>Before:</strong> asserted that authentication implied delegated authority; no source cited.</p><p><strong>After:</strong> identifies the missing authority edge; cites tenant PR-012, p. 4, capability@17; returns an unverified relation for independent review.</p></div><div class="l-pair"><small>PAIR 2 · AC-174 / TRACEABILITY</small><p><strong>Before:</strong> accepted the model view while two requirements were untraced.</p><p><strong>After:</strong> names the two gaps and links each required observer check to its acceptance criterion.</p></div>`;
  }
  function costWalk() {
    return `<div class="l-dual">${panel(
      "Cost search · quality is a registered guardrail",
      `<div class="l-flow">${["Caching health", "Prompt audit", "Model × effort", "Prompt climb", "Joint confirm"].map((v, i) => `<div class="${i === 0 ? "current" : ""}"><small>0${i + 1}</small><strong>${v}</strong><span>${i ? "After preceding lever" : "Current lever"}</span></div>`).join("")}</div>${rows(
        [
          ["Primary target", "Cost per completed case ↓"],
          ["Quality floor", "Held-out acceptance ≥ 85%"],
          ["Latency gate", "p95 ≤ 12 seconds"],
          ["Served model", "Must match the candidate route’s declared model"],
          [
            "Comparison",
            "Same instrument / grader / split; ledger-derived cost",
          ],
          ["Candidate adoption", "Cheaper AND every registered gate passes"],
        ],
      )}${table(["Candidate", "Cost / case", "Quality", "Adoption"], ["<tr><td>Cache prefix ordering</td><td>$0.16</td><td>85%</td><td>Eligible after repeat confirmation</td></tr>", "<tr><td>Cheaper model route</td><td>$0.09</td><td>76%</td><td>Refused · quality below 85%</td></tr>"])}${button("Test guardrail refusal", "cost-refuse", "", "small", "shield")}`,
    )}${panel("Before round 1", `${note("Register the gates first", "The search order is persisted with the plan. Resuming a campaign continues at its current lever; it does not restart the search.")}${button("Review cost plan", "climb-sign", "", "primary", "shield")}${route("Inspect provider cache classes", "usage", "small", "chart")}${note("Provider cache ≠ response cache", "Read > 0: hit. Creation > 0 without read: write. Neither: miss. Missing usage: unknown. A write is a deliberate warm, not a miss.", "blue")}`)}</div>`;
  }
  function finalReport() {
    return `<div class="l-dual">${panel(
      "Winner · v1 / cited source selection",
      `<div class="l-tagline">${B(l.stopped ? "Final" : "Working report", "green")}${B("Subject still process @2")}</div><h2>Keep the measured selection change.</h2><p>Held-out acceptance improved 70% → 85%. Later context expansion improved training outcomes without moving held-out, and was reverted.</p>${rows(
        [
          [
            "What changed",
            "Selected the required constraint passage before generating the architecture view",
          ],
          [
            "What moved",
            "Train +12 pp; held-out +15 pp; no quality guardrail breach",
          ],
          [
            "What remains",
            "Artifact gap 4; grader disagreement 2; harness 1; structural 3; variance 2 (sample triage)",
          ],
          [
            "Adoption",
            l.publishedDraft
              ? "Procedure @3 draft created for review; published @2 remains active"
              : "Person reviews a new procedure draft; active run pins remain unchanged",
          ],
        ],
      )}${button(l.publishedDraft ? "Review adoption draft" : "Prepare adoption draft", "adopt-winner", "", "primary", "edit")}${button("Export campaign report", "export-climb", "", "small", "download")}`,
    )}${panel("Evidence and limits", `${l.actor === "reviewer" ? transcriptPairs() : note("Held-out transcript access restricted", "Reviewer access is required for the final before/after pairs.", "amber")}${note("New measurement after instrument change", "Fixing the rubric creates a new eval version. Regrade stored transcripts into new runs for every compared variant before re-comparing; never overwrite previous results.", "blue")}${button("Review regrade path", "regrade", "", "small", "replay")}`)}</div>`;
  }
  function evalStudio() {
    if (state.workbench?.tab === "Object evaluation") {
      l.evalTab = "Object evaluation";
      l.evalSubject = state.workbench.subject || l.evalSubject;
      state.workbench = null;
    }
    const ev = selectedEval(),
      ready = Object.values(l.gates).every(Boolean);
    return `<div class="page enterprise learning">${heading("Evaluation studio", "Build an outcome instrument, review its inputs and grading, then measure the actual registered entry action.", button("New evaluation", "new-eval", "", "primary", "plus"))}<div class="l-scope">${I("shield")}<strong>${E(ev.name)}</strong>${B("Instrument @" + l.version)}<span class="mono">${ev.id === "architecture" && l.version === 3 ? "fixture digest 73e8 · split 91c7 · grader@2" : "new draft identity · approvals required"}</span><span class="grow"></span>${B(ready ? "Ready for a run" : "Instrument review required", ready ? "green" : "amber")}</div><div class="l-grid">${panel("Instruments", `<div class="l-tree">${l.evals.map((e) => `<button data-l="eval-select" data-id="${e.id}" class="${e.id === ev.id ? "selected" : ""}">${I("file")}<span><strong>${E(e.name)}</strong><small>${e.cases} cases · ${E(e.subject)}</small></span></button>`).join("")}</div>`, B(l.evals.length), "")}<div>${tabs(["Instrument", "Object evaluation", "Cases & splits", "Run & results", "Report", "Audit", "Sign-offs", "Interop"], l.evalTab, "evalTab")}${evalBody(ev)}</div><aside class="l-inspector">${panel(
      "Version & approvals",
      `${rows([
        ["Subject", E(ev.subject)],
        ["Entry action", "<code>procedure_runs.start</code>"],
        ["Trace source", "run → run_step → action_invocation"],
        [
          "Inputs",
          B(
            l.gates.inputs ? "Signed" : "Missing",
            l.gates.inputs ? "green" : "amber",
          ),
        ],
        [
          "Grading",
          B(
            l.gates.grading ? "Signed" : "Missing",
            l.gates.grading ? "green" : "amber",
          ),
        ],
        [
          "Cost / harness",
          B(l.gates.cost && l.gates.harness ? "Signed" : "Missing", "green"),
        ],
      ])}${button("Review pilot grading", "eval-sign", 'data-gate="grading"', "primary", "shield")}${button("Run evaluation", "eval-run", "", "small", "run")}${note("Editing invalidates approvals", "A changed instrument needs a new version and new digest-bound sign-offs. A climb cannot change the case set or redraw its split.", "amber")}`,
    )}${panel("Evidence access", actorControl())}${panel("Use this instrument", `${button("Open quality hillclimb", "open-quality", "", "small", "branch")}${button("Open cost hillclimb", "open-cost", "", "small", "chart")}<p>A pilot validates the harness and rubric. A full frozen train / test pass supports candidate decisions.</p>`)}</aside></div>${boundary("Cases, runs, sign-offs and climb correspond to the current eval surface. Expanded authoring, eval audit and interop are planned S19 design controls. Fixtures contain no real answer key; test traces are excluded from the analyzer view.")}</div>`;
  }
  function objectEvaluation() {
    const subjects = [
      ["knowledge", "Knowledge extraction", "missing content · extraction fidelity", "extraction-suite v2", "40 cases", "Ready"],
      ["retrieval", "Retrieval", "expected-evidence recall · citation coverage", "retrieval-regression v3", "80 held-out", "Measuring"],
      ["pko", "Procedural knowledge (PKO)", "undefined terms · invalid relations · constraints", "pko-validation v1", "24 terms", "Ready"],
      ["memory", "Memory", "recall usefulness · stale or conflicting facts", "memory-policy v2", "80 held-out", "Candidate"],
      ["scope", "Scope policies", "cross-card/project exclusion · authorised recall", "isolation-suite v1", "32 cases", "Ready"],
      ["job", "Job outcomes", "acceptance · failure · latency · cost", "architecture-v3", "80 held-out", "Baseline"],
    ];
    const selected = subjects.find((s) => s[0] === l.evalSubject) || subjects[1];
    return `<div class="v2-ribbon" role="note"><strong>V2 · PROPOSED</strong><span>Per-object evaluation: extraction, retrieval, PKO, memory, scope policy and job outcome subjects, with a memory disabled/current/candidate comparison and a cutoff-bound replay. The S17–S21 instruments are the seed; no new evaluation action is claimed.</span><span class="v2-src">Local fixture</span></div>${metrics([
      ["Subjects", String(subjects.length), "one per knowledge/memory object"],
      ["Held-out cases", "80", "paired, never pooled"],
      ["Human reviews", "1 pending", "separate from model assessment"],
      ["Regressions", "1", "captured per case"],
    ])}${table(
      ["Object", "Instrument", "Metric", "Dataset", "Status"],
      subjects.map(
        (s) =>
          `<tr class="${s[0] === selected[0] ? "selected" : ""}"><td><button class="e-project-link" data-l="eval-subject" data-id="${s[0]}"><span><strong>${E(s[1])}</strong><small>${E(s[0])}</small></span></button></td><td>${E(s[3])}</td><td><small>${E(s[2])}</small></td><td>${E(s[4])}</td><td>${B(s[5], s[5] === "Measuring" || s[5] === "Candidate" ? "amber" : "green")}</td></tr>`,
      ),
    )}<h3>${E(selected[1])} · ${E(selected[3])}</h3>${rows([
      ["Metric", E(selected[2])],
      ["Dataset", E(selected[4]) + " · immutable split"],
      ["Baseline", selected[0] === "memory" ? "current memory policy" : "pinned subject version"],
      ["Human review", B("separate authority", "green")],
      ["Model assessment", B("advisory only", "amber")],
    ])}${panel(
      "Memory policy comparison",
      `${table(
        ["Arm", "Cases", "Resolved", "Delta", "Uncertainty", "Cost"],
        [
          "<tr><td><strong>Memory disabled</strong></td><td>80 held-out</td><td>64</td><td>baseline</td><td>—</td><td>$0.00</td></tr>",
          "<tr><td><strong>Current policy</strong></td><td>80 held-out</td><td>67</td><td>+3 pp</td><td>±6.5 pp noise floor</td><td>$0.42</td></tr>",
          "<tr><td><strong>Candidate policy</strong></td><td>80 held-out</td><td>69</td><td>+5 pp · inside the 8 pp band</td><td>±6.5 pp noise floor</td><td>$0.38</td></tr>",
        ],
      )}<h3>Per-case outcomes</h3>${table(
        ["Case", "Disabled", "Current", "Candidate", "Human review", "Model assessment", "Measured"],
        [
          "<tr><td>C-17</td><td>fail</td><td>pass</td><td>pass</td><td>reviewed</td><td>agrees</td><td>pass</td></tr>",
          "<tr><td>C-23</td><td>pass</td><td>pass</td><td>fail · regression</td><td>pending</td><td>flags regression</td><td>fail</td></tr>",
          "<tr><td>C-31</td><td>pass</td><td>pass</td><td>pass</td><td>reviewed</td><td>neutral</td><td>pass</td></tr>",
        ],
      )}${note("Separate the judgements", "Human review status, model assessment and measured outcome are visibly separate. A model cannot approve its own adoption, and a candidate inside the noise band is not a gain.")}`,
    )}${panel(
      "Replay with a time cutoff",
      `${l.replayError ? note("Replay refused", E(l.replayError), "amber") : ""}<form id="l-replay-form" class="l-form-grid"><label class="field"><span>Time cutoff</span><input type="datetime-local" name="cutoff" value="${E(l.replayCutoff)}" required></label><label class="field"><span>Memory state</span><select name="memory"><option value="isolated" ${l.replayMemory === "isolated" ? "selected" : ""}>Isolated memory · no later rows</option><option value="current" ${l.replayMemory === "current" ? "selected" : ""}>Current live memory</option></select></label><label class="field"><span>Subject</span><select name="subject">${subjects.map((s) => `<option value="${s[0]}" ${s[0] === selected[0] ? "selected" : ""}>${E(s[1])}</option>`).join("")}</select></label>${F("Replay rationale", "rationale", "", "textarea", "Required; recorded with the replay.", true)}<button class="btn primary">Schedule replay</button></form>${l.replayScheduled ? note("Replay scheduled (local fixture)", `Cutoff ${E(l.replayCutoff)} · isolated memory · no production change. Later information cannot leak into earlier cases.`, "") : note("Why a cutoff", "A replay must reconstruct the memory state at the cutoff. Using current live memory would leak later information into earlier cases and invalidate the comparison.", "blue")}`,
    )}`;
  }
  function evalBody(ev) {
    if (l.evalTab === "Object evaluation") return objectEvaluation();
    if (ev.id !== "architecture")
      return panel(
        "Instrument draft",
        `<h2>${E(ev.name)}</h2><p>${E(ev.subject)} · ${ev.cases} illustrative cases. The complete run and report fixtures belong to Architecture acceptance; no results are invented for this instrument.</p>${rows(
          [
            ["Capability", E(ev.capability)],
            ["Case taxonomy", E(ev.tags)],
            [
              "Detailed sample rows",
              String(l.cases.filter((c) => c.evalId === ev.id).length),
            ],
          ],
        )}${button("Edit instrument draft", "edit-eval", "", "small", "edit")}${button("Add case to next draft", "add-case", "", "small", "plus")}${button("Import cases", "import-cases", "", "small", "file")}`,
      );
    if (l.evalTab === "Instrument")
      return panel(
        "Outcome contract",
        `<span class="eyebrow">${E(ev.capability.toUpperCase())} / ${E(ev.name)}</span><h2>Measure the result of the work.</h2><p>Does the output satisfy requirements, principles and acceptance evidence? Agentic subjects are graded on their final outcome; tool paths remain diagnostic evidence.</p>${rows(
          [
            ["Purpose", E(ev.name)],
            [
              "Input schema",
              "Model reference + requirement baseline + principle set + work-package context",
            ],
            [
              "Case provenance",
              "Approved review examples + owner-curated synthetic edge cases",
            ],
            ["Tags", E(ev.tags)],
            [
              "Frozen version",
              "Cases, metrics, graders, prices and harness identity under one digest",
            ],
            [
              "Split",
              "160 train / 80 held-out · stratified by first tag · locked once",
            ],
          ],
        )}<h3 style="margin-top:20px">Metrics & graders</h3>${table(["Metric", "Role", "Grader"], ["<tr><td>Acceptance</td><td>Headline · ↑</td><td>Programmatic criteria + rubric@2</td></tr>", "<tr><td>Citation support</td><td>Quality guardrail · ≥ 95%</td><td>Independent rubric judge</td></tr>", "<tr><td>Cost / latency</td><td>Performance · ↓</td><td>Captured usage + elapsed time</td></tr>"])}${note("Four grader kinds", "Programmatic, rubric, pairwise and human. Judge model differs from subject; human labels calibrate judge agreement. A pending label is excluded and counted, never silently scored zero.", "blue")}${button("Edit instrument draft", "edit-eval", "", "small", "edit")}${button("Edit metrics", "edit-metrics", "", "small", "settings")}`,
        B("S19 authoring proposal", "purple"),
      );
    if (l.evalTab === "Cases & splits")
      return panel(
        "Cases and immutable split",
        `${note("Answer key is grader-only", "Subject input contains no expected field. Held-out traces need evals:approve; an analyzer can inspect train cases only.", "blue")}${table(
          ["Case", "Tag", "Split", "Input / provenance"],
          l.cases
            .filter(
              (c) =>
                c.evalId === l.evalId &&
                (l.actor === "reviewer" || c.split !== "test"),
            )
            .map(
              (c) =>
                `<tr><td><button data-l="case-open" data-id="${c.id}">${E(c.id)}</button><small>${E(c.title)}</small></td><td>${E(c.tag)}</td><td>${B(c.split, c.split === "test" ? "purple" : "")}</td><td>${E(c.input)}<small>Curated fixture · source revision ${l.caseRevision}</small></td></tr>`,
            ),
        )}<p class="l-caption">${l.cases.length} detailed sample rows from ${ev.cases} illustrative cases. Expected outputs are not exposed in the case list.</p><div class="l-actions">${button("Add case to next draft", "add-case", "", "small", "plus")}${button("Inspect locked split", "split-lock", "", "small", "shield")}</div>`,
        B("Train 160 / held-out 80"),
      );
    if (l.evalTab === "Sign-offs")
      return panel(
        "Review the instrument at its literal pauses",
        `<p>A person approves the exact artifact digest. Signing a previous revision cannot authorise a changed instrument.</p>${Object.entries(
          l.gates,
        )
          .map(
            ([key, value]) =>
              `<div class="l-approval-row">${I("shield")}<div class="grow"><strong>${{ inputs: "Inputs & gold provenance", grading: "Pilot grading & rubric", cost: "Cost ceiling", harness: "Harness controls" }[key]}</strong><small>${value ? "George · person · Web · digest 73e8" : "Awaiting review of current artifact"}</small></div>${B(value ? "Signed" : "Missing", value ? "green" : "amber")}${button("Review", "eval-sign", `data-gate="${key}"`, "small")}</div>`,
          )
          .join(
            "",
          )}${note("Machine work is not a person’s signature", "An analyzer may propose rounds. It cannot set its own budget or sign the plan, and kata_executor does not gain evals:approve.", "amber")}`,
      );
    if (l.evalTab === "Run & results")
      return panel(
        "Run the registered subject",
        `${l.evalRun ? note("EV-402 · sample pass complete", `${E(l.evalMode)} · every case has a visible outcome, attempts and trace identity.`) : note("No run started in this session", "The sample report is available for inspection. Starting a new run checks all sign-offs first.", "blue")}<div class="l-actions">${button("Run 5-case pilot", "eval-run", 'data-mode="pilot"', "primary", "run")}${button("Run full pass", "eval-run", 'data-mode="full"', "", "run")}</div>${table(
          ["Case / rep", "Outcome", "Grade", "Attempts / evidence"],
          [
            `<tr><td>AC-001 / 1</td><td>${B("complete", "green")}</td><td>{acceptance: 1, citation: 1}</td><td>${button("Inspect trace", "case-open", 'data-id="AC-001"', "small")}<small>1 attempt · served-model matched</small></td></tr>`,
            `<tr><td>AC-002 / 1</td><td>${B("truncated", "amber")}</td><td>Excluded; counted separately</td><td>Token ceiling reached<small>Retained as truncation, not a wrong answer</small></td></tr>`,
            `<tr><td>AC-003 / 1</td><td>${B("attempt error", "amber")}</td><td>No result slot occupied</td><td>Timeout → jittered retry<small>Total per-attempt deadline</small></td></tr>`,
          ],
        )}<p class="l-caption">Three diagnostic fixtures. Errors never become a zero grade; successful retries fill the case / rep result slot and preserve earlier error attempts.</p>`,
      );
    if (l.evalTab === "Report")
      return `${metrics([
        ["Held-out acceptance", "85%", "95% CI 79–91% · illustrative"],
        ["Train acceptance", "84%", "160 cases × 3 reps"],
        ["Error / truncated", "2 / 3", "Excluded and counted"],
        ["Headroom / noise", "15 / 6.5 pp", "Smallest shippable delta 8 pp"],
      ])}${panel("What changed and where", `${table(["Tag", "Acceptance", "Remaining failures"], ["<tr><td>Delegation</td><td>88%</td><td>Missing source authority</td></tr>", "<tr><td>Traceability</td><td>84%</td><td>Unrealised acceptance links</td></tr>", "<tr><td>Conformance</td><td>83%</td><td>Unapproved principle exceptions</td></tr>"])}${note("Report derives from raw rows", "Grade objects, attempts, token usage, served model and latency remain inspectable. Missing prices show tokens instead of an invented dollar cost.")}${button("Compare variants", "show-comparison", "", "small", "chart")}${button("Export sample report", "eval-export", "", "small", "download")}`)}`;
    if (l.evalTab === "Audit")
      return panel(
        "Is this instrument trustworthy?",
        `${B("S19 planned audit surface", "purple")}<h2>Check the instrument before optimising it.</h2>${rows(
          [
            [
              "Oracle / null probes",
              "≈100% / ≈0%; validate grader discrimination",
            ],
            ["Judge calibration", "Three known negatives + reviewer labels"],
            [
              "Signal",
              "Noise floor 6.5 pp · headroom 15 pp · minimum delta 8 pp",
            ],
            [
              "Isolation",
              "No answer key in subject input; no held-out traces to analyzer",
            ],
            [
              "Reproducibility",
              "Version, split, grader and served-model identities fixed",
            ],
          ],
        )}${note("Finding: rubric conflates claim and verification", "High severity · medium confidence. A producer’s assertion should not receive the same score as independent observer evidence.", "amber")}${button("Review finding & regrade path", "regrade", "", "primary", "shield")}`,
      );
    return panel(
      "Import / export",
      `${B("S19 planned interop", "purple")}<h2>Cases and results keep their provenance.</h2><p>A versioned bundle includes the instrument, metrics, source identities and permitted transcripts. Import copies cases into a new draft; it does not fabricate attempts or link a mutable case set.</p>${button("Export local sample bundle", "eval-export", "", "primary", "download")}${button("Import case JSON into draft", "import-cases", "", "small", "plus")}`,
    );
  }
  function benchmarks() {
    const a = l.benchmarkA,
      b = l.benchmarkB,
      comparable = a !== "incompatible" && b !== "incompatible";
    const arms = {
      gold: [50, "Suite control · gold patch"],
      empty: [0, "Suite control · no patch"],
      baseline: [20, "One-shot model control · planned producer"],
      candidate: [30, "Conveyor subject @8 · planned S18 arm"],
      incompatible: [28, "files+test oracle · different prompt"],
    };
    const controls = [a, b].some((v) => ["gold", "empty"].includes(v));
    return `<div class="page enterprise learning">${heading("Benchmark harness", "A separate measurement instrument: pinned cases, independent verdicts, controls and paired comparisons.", button("Start sample control run", "bench-start", "", "primary", "run"))}<div class="l-scope">${I("code")}<strong>SWE-bench · verified50-v1</strong>${B("50 pinned instances")}${B("seed sha256: 49d0…")}<span class="grow"></span><span class="mono">image lock + eval-script digest</span></div>${note("Implementation boundary", "The current Rust screen offers gold and empty control runs. Baseline / Conveyor subject results below illustrate the planned S18 kata workflow, labelled as fixtures; this mock does not imply those producers are available.", "blue")}${tabs(["Runs & controls", "Paired comparison", "Kata trend"], l.benchTab, "benchTab")}${
      l.benchTab === "Kata trend"
        ? benchmarkTrend()
        : l.benchTab === "Paired comparison"
          ? `<div class="l-dual">${panel(
              "Choose a valid comparison",
              `<div class="l-form-grid">${S(
                "Run A",
                "bench-a",
                a,
                Object.entries(arms).map(([v, d]) => [v, d[1]]),
              )}${S(
                "Run B",
                "bench-b",
                b,
                Object.entries(arms).map(([v, d]) => [v, d[1]]),
              )}</div>${button("Compare selected runs", "bench-compare", "", "primary", "chart")}${
                !comparable
                  ? note(
                      "409 not_comparable",
                      "Oracle or prompt differs. The instrument refuses the comparison instead of plotting a misleading delta.",
                      "amber",
                    )
                  : controls
                    ? note(
                        "Suite control comparison",
                        "Gold / empty runs validate the instrument; this is not a capability quality comparison. Controls cannot become the subject baseline.",
                        "blue",
                      )
                    : `${note("Comparable · paired intersection", "Same suite hash, image lock, files oracle and bug_fix prompt. Example baseline vs candidate pairing shown below.")}${rows(
                        [
                          [
                            "A / B resolved",
                            `${arms[a][0]} / 50 · ${arms[b][0]} / 50`,
                          ],
                          [
                            "Both / neither",
                            a === b
                              ? `${arms[a][0]} / ${50 - arms[a][0]}`
                              : "18 / 18",
                          ],
                          [
                            "A only / B only",
                            a === b
                              ? "0 / 0"
                              : a === "candidate"
                                ? "12 / 2"
                                : "2 / 12",
                          ],
                          [
                            "Exact McNemar",
                            a === b
                              ? "p = 1.0000"
                              : `p = ${exactP(2, 12).toFixed(4)}`,
                          ],
                          [
                            "Missing in one",
                            "0 · case-level failures classified before pairing",
                          ],
                        ],
                      )}${button("Promote candidate baseline", "bench-promote", "", "small", "shield")}`
              }`,
            )}${panel(
              "Verdict provenance",
              `<h3>Producer output is not a verdict</h3><p>The parity grader reads sandbox test logs. Resolved, unresolved and infrastructure failure are distinct outcomes.</p>${rows(
                [
                  ["Sandbox", "One capped k8s Job per case"],
                  ["Evidence", "Patch → test log → parity grader → verdict"],
                  [
                    "Comparison",
                    "Exact two-sided McNemar on valid paired cases",
                  ],
                  [
                    "Promotion",
                    "Completed subject run with valid controls; explicit decision",
                  ],
                ],
              )}`,
            )}</div>`
          : `<div class="l-dual">${panel(
              "Run arms and controls",
              table(
                ["Arm", "Resolved", "Purpose", "Readiness"],
                Object.entries(arms)
                  .filter(([id]) => id !== "incompatible")
                  .map(
                    ([id, [count, desc]]) =>
                      `<tr><td><strong>${id === "candidate" ? "conveyor" : id}</strong><small>${E(desc)}</small></td><td class="numeric">${count} / 50</td><td>${id === "gold" ? "Oracle control" : id === "empty" ? "Null control" : id === "baseline" ? "One-shot reference" : "Pinned capability_agent / procedure"}</td><td>${B(["gold", "empty"].includes(id) ? "Control fixture" : "S18 planned fixture", ["gold", "empty"].includes(id) ? "green" : "purple")}</td></tr>`,
                  ),
              ),
              B("Instrument checks before claims"),
              "",
            )}${panel("Case evidence", table(["Instance", "Verdict", "Observation"], ["<tr><td>django__django-11099</td><td>resolved</td><td>FAIL_TO_PASS passed; PASS_TO_PASS preserved</td></tr>", "<tr><td>sympy__sympy-18057</td><td>unresolved</td><td>Patch applied, required tests still fail</td></tr>", "<tr><td>sample infra case</td><td>infra_failure</td><td>Sandbox deadline; not a wrong-answer verdict</td></tr>"]), button("Inspect case log", "bench-log", "", "small", "file"), "")}</div><aside>${panel(
              "Candidate control",
              `<span class="eyebrow">KATA / VERSION-TO-VERSION</span><h2>${l.promotedBenchmark ? "@8 baseline promoted" : "@8 awaits comparison"}</h2>${rows(
                [
                  ["Current subject", "Build a feature @7"],
                  ["Candidate", "Build a feature @8"],
                  ["Knowledge pin", "capability@17 · delivery"],
                  ["Suite controls", "Gold 50 / 50; empty 0 / 50"],
                  [
                    "Subject baseline",
                    l.promotedBenchmark
                      ? "Candidate BR-108 · @8"
                      : "BR-107 · @7",
                  ],
                ],
              )}${button("Open paired comparison", "bench-tab", 'data-value="Paired comparison"', "primary", "chart")}${button("Try promoting gold control", "bench-control-refuse", "", "small", "shield")}`,
            )}${panel("Kata uses the public actions", `<p>Start → wait → compare → record keep / revert / inconclusive. The improvement job does not read answer keys or set verdicts.</p>${route("Improvement control", "improvement", "small", "arrow")}`)}</aside></div>`
    }${boundary("Benchmark data is independent from task eval data. Full subject arms and version trends are planned S18 projections; gold / empty controls and comparison rules are grounded in S17 and the current screen.")}</div>`;
  }
  function benchmarkTrend() {
    return `${panel("Capability version trend · subject runs only", `${table(["Subject version", "Resolved", "Baseline delta", "Decision"], ["<tr><td>Process @6</td><td>16 / 50 · 32%</td><td>Historical</td><td>Superseded baseline</td></tr>", "<tr><td>Process @7</td><td>20 / 50 · 40%</td><td>Baseline</td><td>Current promoted subject</td></tr>", "<tr><td>Process @8</td><td>30 / 50 · 60%</td><td>+20 pp · paired p ≈ 0.0129</td><td>Keep candidate · review promotion</td></tr>"])}${note("Keep the controls separate", "Gold, empty and the one-shot model control validate the instrument. The capability trend contains only pinned Conveyor subject versions.", "blue")}${button("Review baseline promotion", "bench-promote", "", "primary", "shield")}`)}`;
  }
  function usage() {
    return `<div class="page enterprise learning">${heading("Inference & prompt audit", "Trace every model call to its run, recipe and compiled source identity. Turn audit findings into measured candidates.", button("Start prompt audit", "audit-start", "", "primary", "search"))}<div class="l-scope">${I("chart")}<strong>Conveyor-owned model calls</strong><span>All projects · tenant scoped</span><span class="grow"></span>${B("No prompt or response bodies in the ledger")}</div>${metrics(
      [
        ["Provider cache hit rate", "75%", "108 / (108 + 36); writes excluded"],
        ["Cache writes / unknown", "8 / 4", "Warm writes are not misses"],
        [
          "Recipe findings",
          "3",
          "One high-confidence publish blocker",
          "l-amber",
        ],
        ["Audit trigger", "Every 100", "Ledger capture cadence + publish seam"],
      ],
    )}${tabs(["Inference ledger", "Prompt audit", "Audit policy"], l.auditTab, "auditTab")}<div class="l-dual">${
      l.auditTab === "Inference ledger"
        ? panel(
            "Call identity & cache accounting",
            table(
              [
                "Call / work",
                "Recipe / hash",
                "Model / usage",
                "Cache / elapsed",
              ],
              [
                "<tr><td>CALL-774<small>RUN-8821 / architecture / step 2</small></td><td>solution_architect@2<small>compiled sha256:a7c9…e20f</small></td><td>requested = served<small>input 4,120 / output 840</small></td><td>hit · read 3,400<small>2.4 seconds</small></td></tr>",
                "<tr><td>CALL-775<small>EV-401 / v1 / case AC-001</small></td><td>solution_architect@2<small>compiled sha256:ce72…8fb1</small></td><td>requested = served<small>input 4,180 / output 810</small></td><td>write · creation 3,400<small>2.7 seconds</small></td></tr>",
                "<tr><td>CALL-776<small>Copilot / turn CT-14</small></td><td>copilot@1<small>compiled sha256:78e1…92ab</small></td><td>requested = served<small>usage absent</small></td><td>unknown<small>1.9 seconds</small></td></tr>",
              ],
            ),
            button("Inspect source manifest", "compile", "", "small", "code"),
            "",
          )
        : l.auditTab === "Prompt audit"
          ? panel(
              "Finding PA-017 · source ordering",
              `<div class="l-tagline">${B(l.auditStarted ? "Audit completed · fixture" : "Recorded sample audit", "purple")}${B(l.auditStatus, l.auditStatus === "Proposed" ? "amber" : "green")}</div><h2>Keep dynamic card data after stable instructions.</h2><p>The rendered source inventory places volatile project text ahead of the stable procedure prefix. The proposed diff reorders source classes while preserving the instruction boundary.</p>${rows(
                [
                  [
                    "Scope / target",
                    "architecture · solution_architect@2 · current model route",
                  ],
                  ["Provenance", "CALL-774 · compiled sha256:a7c9…e20f"],
                  ["Confidence", "High · cache-hostile ordering"],
                  [
                    "Anti-patterns",
                    "Ordering / duplication / contradictory instruction / missing structure",
                  ],
                  [
                    "Proposed change",
                    "Move stable procedure and role before project-card data",
                  ],
                  ["Decision", E(l.auditStatus)],
                ],
              )}<div class="l-code">- project_card → procedure → capability / role\n+ system → procedure → capability / role → knowledge → project_card</div>${button("Review finding", "audit-review", "", "primary", "shield")}${button("Create improvement hypothesis", "audit-hypothesis", "", "small", "branch")}`,
            )
          : panel(
              "Trigger and publication policy",
              `${rows([
                ["Manual", "Person or kata invokes prompt_audits.start"],
                [
                  "After N executions",
                  "Count tenant inference_call rows at capture time; no separate counter",
                ],
                [
                  "On procedure publication",
                  "Synchronous prompt_audits.check on the draft",
                ],
                [
                  "High-confidence findings",
                  "422 procedure_audit_findings; a person may acknowledge with an audited reason",
                ],
                [
                  "Finding transitions",
                  "Person-controlled; audits propose and never apply",
                ],
              ])}${button("Preview publication refusal", "audit-publish-refuse", "", "small", "shield")}`,
            )
    }<aside>${panel("One chain of provenance", `<div class="l-crumb">CAPABILITY → PROCEDURE → RECIPE → COMPILED HASH → CALL → FINDING → EXPERIMENT</div><p>The prompt is compiled at call time. Recipe and source versions identify it; the ledger retains identity and usage, without prompt text.</p>${route("Inspect knowledge compiler", "knowledge-design", "small", "code")}${button("Open cost hillclimb", "open-cost", "", "small", "chart")}`)}${panel("Audit makes a proposal", `<p>A finding changes no live process. Accepted findings become one-hypothesis candidates, measured on a frozen instrument.</p>${route("Improvement control", "improvement", "small", "branch")}`)}</aside></div>${boundary("S16 ledger / prompt audit contracts and S21 provenance. Ledger rows and findings are local fixtures; no provider, billing or router database is queried.")}</div>`;
  }
  function info(title, body, tone = "") {
    openDialog(
      title,
      `<div class="dialog-body learning">${note(title, body, tone)}</div>`,
    );
  }
  function close() {
    $("#dialog").close();
  }
  function reviewerRequired() {
    if (l.actor === "reviewer") return true;
    info(
      "Reviewer access required",
      "This action needs evals:approve held by a person. The analyzer can propose a train-informed round; it cannot approve a plan, budget or held-out trace.",
      "amber",
    );
    return false;
  }
  function signForm(title, id, body) {
    dialogForm(
      title,
      `<div class="full learning">${body}</div>${F("Decision rationale", "rationale", "", "textarea", "Required; recorded with George · person · Web", true)}`,
      id,
      "Record review decision",
    );
  }
  function invalidateInstrument() {
    l.version++;
    l.caseRevision++;
    l.gates = { inputs: false, grading: false, cost: false, harness: false };
    l.evalRun = false;
    toast(
      "New instrument draft created; digest-bound sign-offs invalidated. Published runs remain unchanged.",
    );
  }
  function compilePreview() {
    const p = profile(),
      blocked = l.knowledgeOutage || !effectiveSources().length;
    openDialog(
      "Resolved context · " + p.job,
      `<div class="dialog-body learning">${note(blocked ? "409 capability_not_ready" : "Compile preview ready", blocked ? "Required knowledge section cannot resolve. No model call is made." : "Select → fetch → render → budget → hash & record. Fixture source manifest; no prompt text is persisted.", blocked ? "amber" : "")}${rows(
        [
          ["Instructions", "system@1 · highest trust"],
          ["Procedure", E(p.subject) + " · immutable published version"],
          ["Capability / role", E(p.label) + " / " + E(p.role)],
          [
            "Knowledge",
            `capability@17 · metadata.capability=${E(p.pack)} · ${effectiveSources().length} sources · ${E(selectionAt())}`,
          ],
          ["Memory", `${E(effectiveMemory())} · conveyor:work:${E(p.job)}`],
          [
            "Project / card",
            "Trust & identity / " +
              E(p.job) +
              " · tenant data, below instructions",
          ],
          [
            "Character measurement",
            "8,160 / 12,000 total; section ceilings enforced",
          ],
          [
            "Recorded identity",
            E(p.recipe) + " · fixture compiled sha256:a7c9…e20f",
          ],
        ],
      )}${button("Test over-budget refusal", "budget-refuse", "", "small", "shield")}</div>`,
      "",
      true,
    );
  }
  document.addEventListener("click", (event) => {
    const el = event.target.closest("[data-l]");
    if (!el) return;
    const d = el.dataset;
    switch (d.l) {
      case "tab":
        l[d.key] = d.value;
        render();
        break;
      case "design-scope":
        l.scope = d.scope;
        render();
        break;
      case "design-entry": {
        const kind = d.kind,
          id = d.id;
        const c =
          kind === "capabilities"
            ? id
            : kind === "processes"
              ? byId("processes", id)?.capability
              : db.processes.find((p) => p.steps.some((s) => s.agent === id))
                  ?.capability;
        if (!profiles[c]) {
          info(
            "Knowledge contract needs configuration",
            `${E(nameOf("capabilities", c || id))} has no detailed context fixture in this study. Its existing tenant definition is preserved. Bind its knowledge sources and process / agent selections before a run; another capability’s context is never substituted. The designer provides four representative capability contexts.`,
            "blue",
          );
          break;
        }
        l.cap = c;
        l.scope =
          kind === "agents"
            ? "agent"
            : kind === "processes"
              ? "process"
              : "capability";
        navigate("knowledge-design");
        break;
      }
      case "edit-selection":
        if (l.scope === "run") {
          info(
            "Runtime context is read only",
            "This view resolves the capability, process and agent design. Select one of those definitions to revise its future context selection; recorded run pins stay immutable.",
          );
          break;
        }
        dialogForm(
          "Select knowledge within the capability boundary",
          S("Source selection · " + l.scope, "selection", selectionAt(), [
            ...(l.scope === "capability"
              ? []
              : [["inherit", "Inherit parent’s effective selection"]]),
            ["both", "All sources allowed by parent"],
            ["standards", "Standards + modelling only"],
            ["none", "No domain sources · demonstrate refusal"],
          ]) +
            F(
              "Reason for this step’s selection",
              "rationale",
              "Only the applicable phase and relation rules are needed for this step.",
              "textarea",
              "Design draft; current run pins stay fixed.",
              true,
            ),
          "l-selection-form",
          "Save context selection",
        );
        break;
      case "edit-memory-policy":
        if (l.scope === "run") {
          info(
            "Runtime memory selection is read only",
            "Edit the capability, process or agent policy for future calls. Past source manifests remain fixed.",
          );
          break;
        }
        dialogForm(
          "Memory selection · " + l.scope,
          S("Read policy", "policy", memoryPolicyAt(), [
            ...(l.scope === "capability"
              ? []
              : [["inherit", "Inherit parent policy"]]),
            [
              "hot_digest",
              "Same-card hot rows + cold digest, within parent policy",
            ],
            ["hot_only", "Same-card hot rows only"],
            ["none", "No work-package recall for this step"],
          ]) +
            F(
              "Selection rationale",
              "rationale",
              "Keep only the work-package evidence needed for this step.",
              "textarea",
              "Proposed S21 memory-selection stage; no cross-card recall.",
              true,
            ),
          "l-memory-policy-form",
          "Save memory policy draft",
        );
        break;
      case "design-outage":
        l.knowledgeOutage = !l.knowledgeOutage;
        render();
        break;
      case "compile":
        compilePreview();
        break;
      case "budget-refuse":
        info(
          "SectionOverBudget",
          "knowledge measured 4,120 characters against a 4,000-character ceiling. Compilation refuses with the measurement; no silent truncation and no inference call.",
          "amber",
        );
        break;
      case "design-improve":
        l.campaignScope = l.cap;
        navigate("improvement");
        break;
      case "workflow-go": {
        const step = Number(d.step);
        l.workflowStep = step;
        if (step === 1) return navigate("execution");
        if (step === 2) return navigate("memory");
        if (step === 3) return navigate("knowledge");
        if (step === 6) return navigate("evals");
        l.designTab =
          step === 7 ? "Review & release" : "Effective configuration";
        if (step === 5) l.configView = "Compare";
        render();
        break;
      }
      case "config-view":
        l.configView = d.value;
        render();
        break;
      case "config-missing":
        l.configMissing = !l.configMissing;
        render();
        toast(
          l.configMissing
            ? "409 release_not_ready · required kind domain.identity has zero leaves in verification"
            : "Required kind restored · the release draft can resolve",
        );
        break;
      case "config-resolve":
        dialogForm(
          "Resolve department_conflict at the job layer",
          `<p class="muted">delivery sets <code>domain.identity</code> to ${E(orgConfig.conflict.delivery)}. verification sets it to ${E(orgConfig.conflict.verification)}. A sibling conflict is a compile error; the job-layer row is the only resolution.</p>${S(
            "Job-layer value",
            "value",
            orgConfig.conflict.resolution,
            [
              ["identity-v2", "identity-v2 · delivery value"],
              ["identity-v3", "identity-v3 · verification value"],
              [
                orgConfig.conflict.resolution,
                "identity-v3 with a documented delivery exception",
              ],
            ],
          )}${F(
            "Rationale",
            "rationale",
            "Verification owns the identity contract; delivery's exception is documented in the job charter.",
            "textarea",
            "Required. Creates a draft row behind a review gate.",
            true,
          )}`,
          "l-config-resolve-form",
          "Create draft job-layer row",
        );
        break;
      case "config-edit":
        dialogForm(
          "Edit at this layer",
          S(
            "Setting",
            "setting",
            orgConfig.settings[0].key,
            orgConfig.settings.map((s) => [s.key, s.key]),
          ) +
            F(
              "Draft value",
              "value",
              orgConfig.settings[0].draft,
              "text",
              "This changes the draft release, not the current release.",
              true,
            ) +
            `<p class="muted">Scope: job layer · affected consumers: ${E(orgConfig.consumers.map((c) => c[0]).join(", "))}. The current release and active run pins stay fixed.</p>` +
            F(
              "Rationale",
              "rationale",
              "",
              "textarea",
              "Required for the review gate.",
              true,
            ),
          "l-config-edit-form",
          "Save draft",
        );
        break;
      case "review-open":
        l.designTab = "Review & release";
        render();
        break;
      case "review-select":
        l.reviewItem = d.id;
        render();
        break;
      case "review-agent":
        toast(
          "403 human_approval_required · a model or the proposing agent cannot approve this change",
        );
        break;
      case "review-advance": {
        const item =
          orgConfig.reviewItems.find((x) => x.id === l.reviewItem) ||
          orgConfig.reviewItems[0];
        const state = l.reviewStates[item.id] || item.state;
        if (state === "Published") {
          info(
            "Published releases are immutable",
            "Create a new version, or roll back to create a superseding version. The published release is never mutated.",
          );
        } else if (state === "Superseded") {
          info(
            "Superseded",
            "This change set was rolled back. Open the new draft in the queue.",
          );
        } else if (state === "In review") {
          dialogForm(
            "Approve this change set",
            `${rows([
              ["Object", E(item.object)],
              ["Reviewer", E(item.reviewer)],
              ["Evidence", E(item.evidence)],
            ])}${F(
              "Person rationale",
              "rationale",
              "",
              "textarea",
              "Required; recorded with the approval.",
              true,
            )}${F(
              "Evidence references",
              "evidence",
              item.evidence,
              "text",
              "The evidence named above is recorded with the approval.",
              true,
            )}`,
            "l-review-form",
            "Record person approval",
          );
        } else {
          l.reviewStates[item.id] =
            state === "Draft"
              ? "In review"
              : state === "Approved"
                ? "Published"
                : "Approved";
          render();
          toast(
            state === "Draft"
              ? "Change set entered review · a person must decide"
              : state === "Approved"
                ? "Change set published · the release is immutable"
                : "Change set advanced",
          );
        }
        break;
      }
      case "review-rollback": {
        const item =
          orgConfig.reviewItems.find((x) => x.id === l.reviewItem) ||
          orgConfig.reviewItems[0];
        dialogForm(
          "Roll back this release",
          `${rows([["Object", E(item.object)]])}${F(
            "Rollback rationale",
            "rationale",
            "",
            "textarea",
            "Required. Creates a new version; the original stays in the supersession audit.",
            true,
          )}`,
          "l-rollback-form",
          "Record rollback",
        );
        break;
      }
      case "memory-level":
        l.memoryScope = d.value;
        l.memoryTab = "Records";
        l.memoryView = "Current";
        render();
        break;
      case "memory-dim": {
        l.memoryDim = d.value;
        const group = {
          Organisation: ["Tenant", "Industry", "Department", "Job"],
          Work: ["Programme", "Project", "Work package"],
          Execution: ["Capability", "Process", "Agent", "Run / attempt", "Step / call"],
          Lifecycle: ["Source", "Published knowledge", "Digest", "Lesson"],
        }[d.value];
        if (group && !group.includes(l.memoryScope)) l.memoryScope = group[0];
        l.memoryView = "Current";
        render();
        break;
      }
      case "memory-view":
        l.memoryView = d.value;
        render();
        break;
      case "memory-select":
        l.memoryItem = d.id;
        l.memoryView = "Current";
        render();
        break;
      case "memory-successor":
        dialogForm(
          "Correct by adding a successor",
          `${note("The original record is retained. The successor is a new record linked by supersedes; it is a draft until a person reviews it.", "", "blue")}${F(
            "Corrected record",
            "text",
            l.successorText ||
              memoryRecords.find((x) => x.id === l.memoryItem)?.body ||
              "",
            "textarea",
            "",
            true,
          )}${F(
            "Why is this correction needed?",
            "rationale",
            "",
            "textarea",
            "Required; recorded with the successor.",
            true,
          )}`,
          "l-successor-form",
          "Save successor draft",
        );
        break;
      case "memory-exclude":
        dialogForm(
          "Exclude this record from agent recall",
          `${note("Exclusion changes agent recall eligibility only. Human inspection, the record and its history are retained.", "", "blue")}${F(
            "Reason",
            "reason",
            "",
            "textarea",
            "Required; recorded with the exclusion.",
            true,
          )}`,
          "l-exclude-form",
          "Exclude from recall",
        );
        break;
      case "memory-digest":
        dialogForm(
          "Rebuild the cold digest (dry run)",
          `${note("Dry-run diff", `Included: ${cardDigest.included.length} hot rows · omitted: ${cardDigest.omitted.length} · conflicts: ${cardDigest.conflicts.length}. The hot rows are the record of truth and are not modified.`)}${S(
            "Action",
            "decision",
            "rebuild",
            [
              ["rebuild", "Rebuild from the current hot rows"],
              ["cancel", "Cancel"],
            ],
          )}${F("Rationale", "rationale", "", "textarea", "", true)}`,
          "l-digest-form",
          "Confirm rebuild",
        );
        break;
      case "memory-policy-compare":
        l.policyCompare = true;
        render();
        toast(
          "Memory policy comparison opened · memory disabled / current / candidate",
        );
        break;
      case "recall":
        l.memoryTab = "Recall & isolation";
        navigate("memory");
        break;
      case "memory-lifecycle":
        navigate("knowledge");
        document
          .querySelector(
            '[data-e="knowledge-tab"][data-value="Work-package memory"]',
          )
          ?.click();
        break;
      case "memory-origin": {
        const m = memoryRecords.find((m) => m.id === d.id);
        info(
          "Originating record",
          `${E(m.source)}<br>${E(m.store)}<br>${E(m.body)}<br>Source events remain append-only. This panel is a scoped projection.`,
        );
        break;
      }
      case "promote-memory":
        dialogForm(
          "Propose a reusable lesson",
          F(
            "Lesson",
            "lesson",
            "Require independent observation before accepting a model relation.",
            "textarea",
            "",
            true,
          ) +
            F(
              "Provenance / evidence",
              "evidence",
              "WP-2048 → RUN-8821 → INV-441; original producer claim remains unverified.",
              "textarea",
              "",
              true,
            ) +
            S("Next step", "decision", "propose", [
              ["propose", "Submit candidate to capability owner"],
              ["approve", "Person review: prepare a tenant source draft"],
            ]) +
            S("Independent evidence state", "evidence_status", "unverified", [
              ["unverified", "Original claim is still unverified"],
              [
                "observed",
                "Independent observer evidence attached in this fixture",
              ],
            ]) +
            F(
              "Review rationale",
              "rationale",
              "",
              "textarea",
              "Required for approval; verification evidence must be recorded.",
              true,
            ),
          "l-promote-form",
          "Record curation step",
        );
        break;
      case "campaign-open":
        l.cap = d.cap;
        if (d.dest === "cost") {
          switchClimb("cost");
          navigate("hillclimb");
        } else {
          switchClimb("quality");
          navigate(d.dest);
        }
        break;
      case "new-campaign":
        dialogForm(
          "New improvement plan",
          F("Name", "name", "", "text", "", true) +
            S(
              "Capability",
              "capability",
              l.cap,
              Object.entries(profiles).map(([id, p]) => [id, p.label]),
            ) +
            S("Instrument", "instrument", "eval", [
              ["eval", "Task eval · quality hillclimb"],
              ["cost", "Task eval · cost hillclimb"],
              ["benchmark", "SWE-bench · kata comparison"],
            ]) +
            F(
              "Hypothesis",
              "hypothesis",
              "",
              "textarea",
              "One change with a predicted observable outcome",
              true,
            ),
          "l-campaign-form",
          "Prepare plan for review",
        );
        break;
      case "lineage":
        if (d.index === "0") navigate("processes", "model");
        else if (d.index === "1") navigate("knowledge-design");
        else if (d.index === "2") navigate("evals");
        else {
          l.climbTab = "Final report";
          navigate("hillclimb");
        }
        break;
      case "round-select":
        l.roundIndex = Number(d.index);
        render();
        break;
      case "climb-plan":
        dialogForm(
          "Experiment plan & stopping rules",
          F("Goal", "target", l.target) +
            F("Cost ceiling (USD)", "ceiling", l.ceiling, "number") +
            F("Smallest detectable delta (pp)", "delta", l.delta, "number") +
            S("Round approval cadence", "gate", l.roundGate ? "yes" : "no", [
              ["yes", "Person sign-off before each round"],
              ["no", "One call per round under signed plan"],
            ]) +
            F(
              "Off limits",
              "off_limits",
              "Cases, graders, held-out traces, fixed split and published production definitions",
              "textarea",
              "",
              true,
            ),
          "l-plan-form",
          "Save unsigned plan",
        );
        break;
      case "climb-sign":
        if (reviewerRequired())
          signForm(
            "Sign the experiment plan",
            "l-climb-sign-form",
            `${rows([
              [
                "Goal",
                l.climbId === "cost"
                  ? "Cost ↓; hold acceptance ≥85%"
                  : E(l.target) + " ↑",
              ],
              ["Ceiling", `$${l.ceiling}`],
              ["Noise band", l.delta + " pp; noise floor 6.5 pp"],
              ["Stopping", "K=3 plateau / 6 rounds / manual"],
              [
                "Off limits",
                "Instrument, split, grader, answer key and production versions",
              ],
            ])}${note("Immutable measurement", "A changed plan clears its signature. A round records one hypothesis and returns to the caller.")}`,
          );
        break;
      case "triage":
        if (!l.signed) {
          info(
            "Plan signature missing",
            "Review and sign the plan before recording the next round.",
            "amber",
          );
          break;
        }
        dialogForm(
          "Categorise stalled failures",
          [
            ["Artifact gap", "artifact_gap", 4],
            ["Grader disagreement", "grader_disagreement", 2],
            ["Harness", "harness", 1],
            ["Structural", "structural", 3],
            ["Variance", "variance", 2],
          ]
            .map(
              ([label, key, n]) =>
                `<label class="field"><span>${label}</span><input name="${key}" type="number" min="0" step="1" value="${n}" required></label>`,
            )
            .join("") +
            F(
              "One-line evidence and next hypothesis",
              "rationale",
              "Source coverage is incomplete for four cases; obtain the missing authoritative passage before changing the role text.",
              "textarea",
              "",
              true,
            ),
          "l-triage-form",
          "Record triage round",
        );
        break;
      case "propose-round":
        if (!l.signed)
          info(
            "Unsigned plan",
            "A person must sign the goal, budget, boundaries and stopping rule first.",
            "amber",
          );
        else if (l.stopped)
          info(
            "Climb stopped",
            "The winner is recorded. Start a new signed campaign for another goal.",
          );
        else if (!l.triaged)
          info(
            "Stall triage required",
            "Two consecutive flat rounds require failure buckets before another content hypothesis.",
            "amber",
          );
        else
          dialogForm(
            "Propose one measured round",
            F(
              "Hypothesis",
              "hypothesis",
              "Select the missing authoritative source for the failed conformance cases.",
              "textarea",
              "",
              true,
            ) +
              F("Predicted improvement (pp)", "gain", 9, "number") +
              F(
                "Falsifier",
                "falsifier",
                "Held-out delta is below the signed band or citation support regresses.",
                "textarea",
                "",
                true,
              ) +
              S("Local replay scenario", "scenario", "overfit", [
                ["overfit", "Train rises; held-out flat → revert"],
                ["repeat", "Train falls; test rises → repeat confirmation"],
                ["guardrail", "Cheaper but quality below floor → refuse"],
              ]),
            "l-round-form",
            "Queue one sample round",
          );
        break;
      case "round-evidence":
        openDialog(
          "Candidate patch & outcome evidence",
          `<div class="dialog-body learning"><div class="l-code">change: source selection r4\n+ fetch cited delegation constraint before model generation\n+ preserve source pages, pack version and observer requirement\nunchanged: eval@3, split 91c7, rubric@2, published process@2</div>${l.actor === "reviewer" ? transcriptPairs() : note("Train-only analyzer view", "AC-001: missing authority boundary before; cited constraint after. Held-out transcript pairs are unavailable to this principal.", "amber")}</div>`,
          "",
          true,
        );
        break;
      case "climb-stop":
        if (l.stopped) {
          l.climbTab = "Final report";
          render();
        } else if (reviewerRequired())
          signForm(
            "Stop climb & record winner",
            "l-stop-form",
            note(
              "Proposed winner: v1",
              "Best held-out score 85%; later rounds reverted. A stopped climb records its report and patch, without applying a production change.",
            ),
          );
        break;
      case "adopt-winner":
        if (reviewerRequired())
          signForm(
            "Prepare adoption draft",
            "l-adopt-form",
            note(
              "Review the winning patch",
              "Create Design architecture @3 as a draft carrying the selection change. Published @2 and active run pins remain unchanged. Publication and audit are separate production actions.",
            ),
          );
        break;
      case "export-climb":
        download(
          "conveyor-climb-report.json",
          JSON.stringify(
            {
              prototype: true,
              instrument: "architecture@3",
              split: "91c7",
              grader: "rubric@2",
              winner: "v1",
              rounds,
              narrative:
                "Keep cited source selection; expanded examples did not improve held-out.",
              production_changed: false,
            },
            null,
            2,
          ),
        );
        break;
      case "compare-refuse":
        info(
          "409 not_comparable",
          "Version @3 and @4 have different grader identities. Regrade each variant under the same current version before comparing.",
          "amber",
        );
        break;
      case "cost-refuse":
        info(
          "Candidate refused · quality guardrail",
          "$0.09 / case is cheaper, but 76% held-out acceptance is below the registered 85% floor. The cheaper model cannot win this cost walk.",
          "amber",
        );
        break;
      case "open-quality":
        switchClimb("quality");
        navigate("hillclimb");
        break;
      case "open-cost":
        switchClimb("cost");
        navigate("hillclimb");
        break;
      case "show-comparison":
        l.climbTab = "Paired comparison";
        navigate("hillclimb");
        break;
      case "regrade":
        info(
          "Rubric fix → new version → new runs",
          "Record the grader finding and create instrument @4. Regrade stored transcripts for every compared variant into new runs; earlier results remain intact. Compare only the resulting @4 runs with identical grader and split. This planned S19 audit flow never rewrites an old grade.",
          "blue",
        );
        break;
      case "eval-select":
        switchEval(d.id);
        l.evalTab = "Instrument";
        render();
        break;
      case "eval-subject":
        l.evalSubject = d.id;
        l.evalTab = "Object evaluation";
        render();
        break;
      case "eval-object":
        l.evalSubject = d.subject;
        l.evalTab = "Object evaluation";
        navigate("evals");
        break;
      case "new-eval":
        dialogForm(
          "Create evaluation draft",
          F("Name", "name", "", "text", "", true) +
            S(
              "Capability",
              "capability",
              l.cap,
              Object.entries(profiles).map(([id, p]) => [id, p.label]),
            ) +
            F("Outcome to measure", "purpose", "", "textarea", "", true),
          "l-new-eval-form",
          "Create local instrument draft",
        );
        break;
      case "edit-eval":
        dialogForm(
          "Edit next instrument draft",
          F("Name", "name", selectedEval().name, "text", "", true) +
            F(
              "Case taxonomy / tags",
              "tags",
              selectedEval().tags,
              "textarea",
              "Changing the instrument voids existing sign-offs.",
              true,
            ),
          "l-edit-eval-form",
          "Create next unsigned version",
        );
        break;
      case "edit-metrics":
        dialogForm(
          "Edit outcome metrics",
          F("Headline metric", "metric", "Acceptance") +
            S("Grader kind", "grader", "rubric", [
              "programmatic",
              "rubric",
              "pairwise",
              "human",
            ]) +
            F(
              "Acceptance rubric",
              "rubric",
              "A model relation needs independently observed evidence and a cited constraint.",
              "textarea",
              "Rubric edits change the grader digest and invalidate sign-offs.",
              true,
            ),
          "l-metrics-form",
          "Save next metric draft",
        );
        break;
      case "eval-sign":
        if (reviewerRequired()) {
          l.pendingGate = d.gate;
          signForm(
            "Review " + d.gate + " sign-off",
            "l-eval-sign-form",
            note(
              "Exact artifact review",
              `Instrument @${l.version}; case, split and grading digests fixed. Check the pilot labels, oracle / null probes, judge disagreement and gold provenance before approving.`,
            ),
          );
        }
        break;
      case "eval-run":
        if (!reviewerRequired()) break;
        if (selectedEval().id !== "architecture") {
          info(
            "No execution fixture for this instrument",
            "Author the cases, freeze its split and review its artifact. This design only replays Architecture acceptance; no results are fabricated for a new draft.",
            "blue",
          );
          break;
        }
        if (!Object.values(l.gates).every(Boolean))
          info(
            "Run refused · missing sign-offs",
            `Current artifact needs: ${Object.entries(l.gates)
              .filter(([, v]) => !v)
              .map(([k]) => k)
              .join(", ")}. No evaluation jobs are created.`,
            "amber",
          );
        else {
          l.evalRun = true;
          l.evalMode =
            d.mode === "full"
              ? "Full pass · frozen train / test"
              : "Pilot · 5 cases";
          l.evalTab = "Run & results";
          render();
          toast(
            "Local evaluation fixture replayed through the registered subject contract.",
          );
        }
        break;
      case "case-open": {
        const c = l.cases.find((c) => c.id === d.id);
        if (!c) break;
        if (c.split === "test" && !reviewerRequired()) break;
        info(
          c.id + " · " + c.title,
          `${E(c.input)}<br>Trace: procedure_runs.start → run → step → action_invocation.<br>Grade: {acceptance: 1, citation: 1}; served model matched.<br>Gold outputs remain grader-only.`,
        );
        break;
      }
      case "add-case":
        dialogForm(
          "Add case to next instrument draft",
          F("Title", "name", "", "text", "", true) +
            F(
              "Subject input",
              "input",
              "",
              "textarea",
              "No expected field is sent to the subject",
              true,
            ) +
            F("First tag", "tag", "Conformance"),
          "l-case-form",
          "Add case & invalidate sign-offs",
        );
        break;
      case "split-lock":
        info(
          "409 split_locked",
          "The stratified train/test split was drawn once from the version digest. A climb cannot redraw it; a new case set creates a new instrument version.",
          "amber",
        );
        break;
      case "eval-export":
        download(
          "conveyor-eval-fixture.json",
          JSON.stringify(
            {
              prototype: true,
              eval: selectedEval(),
              version: l.version,
              case_samples: l.cases.filter(
                (c) =>
                  c.evalId === l.evalId &&
                  (l.actor === "reviewer" || c.split !== "test"),
              ),
              signoffs: l.gates,
              answer_key_included: false,
            },
            null,
            2,
          ),
        );
        break;
      case "import-cases":
        dialogForm(
          "Import case JSON into a new draft",
          F(
            "Cases (JSON array)",
            "json",
            '[{"title":"Missing evidence","input":"Assess the model relation","tag":"Conformance"}]',
            "textarea",
            "title, input and tag required; no attempts imported.",
            true,
          ),
          "l-import-form",
          "Import as unsigned draft",
        );
        break;
      case "bench-tab":
        l.benchTab = d.value;
        render();
        break;
      case "bench-compare":
        l.benchmarkA = $('[name="bench-a"]').value;
        l.benchmarkB = $('[name="bench-b"]').value;
        render();
        break;
      case "bench-control-refuse":
        info(
          "Control cannot be subject baseline",
          "Gold and empty validate the suite. Promoting a control would corrupt the meaning of the capability baseline.",
          "amber",
        );
        break;
      case "bench-promote":
        if ([l.benchmarkA, l.benchmarkB].includes("incompatible"))
          info(
            "Comparison refused",
            "Oracle and prompt identities must match.",
            "amber",
          );
        else if (
          [l.benchmarkA, l.benchmarkB].some((v) =>
            ["gold", "empty"].includes(v),
          )
        )
          info(
            "Control cannot be subject baseline",
            "Choose two subject runs with matching oracle and prompt. Controls validate the suite; they cannot become the capability baseline.",
            "amber",
          );
        else
          signForm(
            "Promote subject baseline · local preview",
            "l-bench-promote-form",
            note(
              "BR-108 · procedure @8",
              "Gold / empty controls valid. Candidate 30/50 vs baseline 20/50; paired exact McNemar p≈0.0129. Promotion updates the suite’s subject baseline; publishing a process remains a separate change.",
            ),
          );
        break;
      case "bench-start":
        info(
          "Gold control · local replay complete",
          "50 / 50 oracle fixtures resolved. Suite seed, image lock, scripts and parity-grader identity are pinned. No k8s Job or live model was started.",
        );
        break;
      case "bench-log":
        openDialog(
          "Case log → independent verdict",
          `<div class="dialog-body learning"><div class="l-code">instance: django__django-11099\nseed: verified50-v1\npatch_apply: ok\nFAIL_TO_PASS: passed\nPASS_TO_PASS: preserved\nend_marker: present\ngrader: parity engine\nverdict: resolved</div>${note("Verdict authority", "The grader derives the verdict from logs. The producer cannot set resolved and a person cannot overwrite it.")}</div>`,
        );
        break;
      case "audit-start":
        l.auditStarted = true;
        l.auditTab = "Prompt audit";
        render();
        toast("One bounded audit call replayed; proposal only.");
        break;
      case "audit-review":
        if (reviewerRequired())
          dialogForm(
            "Review prompt finding PA-017",
            S("Finding decision", "decision", "Accepted", [
              "Accepted",
              "Rejected",
              "Needs evidence",
            ]) +
              F(
                "Rationale",
                "rationale",
                "",
                "textarea",
                "Required; no changes applied by this review.",
                true,
              ),
            "l-audit-form",
            "Record person’s finding decision",
          );
        break;
      case "audit-hypothesis":
        l.cap = "architecture";
        l.campaignScope = "architecture";
        navigate("improvement");
        toast(
          "PA-017 linked as the source-ordering hypothesis; no patch applied.",
        );
        break;
      case "audit-publish-refuse":
        info(
          "422 procedure_audit_findings",
          "High-confidence source-ordering finding blocks publication. A person can explicitly acknowledge it with a recorded rationale; this preview does not publish a procedure.",
          "amber",
        );
        break;
    }
  });
  document.addEventListener("change", (event) => {
    const key = event.target.dataset.lChange;
    if (key === "capability") {
      l.cap = event.target.value;
      l.scope = "capability";
      render();
    }
    if (key === "campaign") {
      l.campaignScope = event.target.value;
      render();
    }
    if (key === "climb") {
      switchClimb(event.target.value);
      l.climbTab = "Rounds & evidence";
      render();
    }
    if (key === "actor") {
      l.actor = event.target.value;
      render();
    }
  });
  document.addEventListener("submit", (event) => {
    const id = event.target.id;
    if (!id.startsWith("l-")) return;
    event.preventDefault();
    const v = Object.fromEntries(new FormData(event.target));
    const needsReason = [
      "l-climb-sign-form",
      "l-stop-form",
      "l-adopt-form",
      "l-eval-sign-form",
      "l-bench-promote-form",
      "l-audit-form",
    ];
    if (needsReason.includes(id) && !v.rationale?.trim())
      return error("Record a rationale for this person’s decision.");
    if (id === "l-replay-form") {
      l.replayError = "";
      if (!v.rationale.trim())
        l.replayError = "Record why this replay is scheduled.";
      else if (v.memory !== "isolated")
        l.replayError =
          "Replay requires an isolated memory state; live memory would leak later information into earlier cases.";
      else {
        l.replayCutoff = v.cutoff;
        l.replayMemory = v.memory;
        l.evalSubject = v.subject;
        l.replayScheduled = true;
      }
      l.evalTab = "Object evaluation";
    }
    if (id === "l-successor-form") {
      if (!v.text.trim() || !v.rationale.trim())
        return error("A corrected record and rationale are required.");
      l.successor = true;
      l.successorText = v.text;
      l.memoryView = "History";
    }
    if (id === "l-exclude-form") {
      if (!v.reason.trim())
        return error("Record why the record is excluded from agent recall.");
      l.excluded[l.memoryItem] = v.reason;
    }
    if (id === "l-digest-form") {
      if (v.decision !== "rebuild")
        return error("Choose rebuild to confirm the dry run.");
      if (!v.rationale.trim()) return error("Record why the digest is rebuilt.");
      l.digestRebuilt = true;
      l.memoryView = "Derivation";
    }
    if (id === "l-config-resolve-form") {
      if (!v.rationale.trim())
        return error("Record the job-layer rationale for this resolution.");
      l.configResolved = true;
      l.configDraft = {
        key: orgConfig.conflict.key,
        value: v.value,
        rationale: v.rationale,
      };
      l.configRevision++;
      l.configView = "Draft";
    }
    if (id === "l-config-edit-form") {
      if (!v.rationale.trim()) return error("Record the draft rationale.");
      l.configDraft = {
        key: v.setting,
        value: v.value,
        rationale: v.rationale,
      };
      if (v.setting === orgConfig.conflict.key) l.configResolved = true;
      l.configRevision++;
      l.configView = "Draft";
    }
    if (id === "l-review-form") {
      if (!v.rationale.trim())
        return error("A person's rationale is required for approval.");
      const item =
        orgConfig.reviewItems.find((x) => x.id === l.reviewItem) ||
        orgConfig.reviewItems[0];
      l.reviewStates[item.id] = "Approved";
    }
    if (id === "l-rollback-form") {
      if (!v.rationale.trim()) return error("Record the rollback rationale.");
      const item =
        orgConfig.reviewItems.find((x) => x.id === l.reviewItem) ||
        orgConfig.reviewItems[0];
      l.reviewStates[item.id] = "Superseded";
      orgConfig.reviewItems.push({
        id: item.id + "-b" + (orgConfig.reviewItems.length + 1),
        kind: item.kind,
        object: item.object + " · rollback draft",
        reviewer: item.reviewer,
        decisions: item.decisions,
        evidence: "rollback rationale · supersedes " + item.id,
        age: "just now",
        state: "Draft",
      });
      l.reviewItem = orgConfig.reviewItems[orgConfig.reviewItems.length - 1].id;
      l.reviewStates[l.reviewItem] = "Draft";
    }
    if (id === "l-selection-form") {
      if (!v.rationale.trim())
        return error("Record why these sources are selected.");
      l.selections[l.cap + ":" + l.scope] = v.selection;
      l.bindingRevision++;
    }
    if (id === "l-memory-policy-form") {
      if (!v.rationale.trim())
        return error("Record the memory selection rationale.");
      l.memoryPolicies[l.cap + ":" + l.scope] = v.policy;
      l.bindingRevision++;
    }
    if (id === "l-plan-form") {
      if (Number(v.delta) < 6.5)
        return error(
          "Predicted decision band must exceed the measured 6.5 pp noise floor.",
        );
      l.ceiling = Number(v.ceiling);
      l.delta = Number(v.delta);
      l.target = v.target;
      l.roundGate = v.gate === "yes";
      l.signed = false;
      l.stopped = false;
    }
    if (id === "l-climb-sign-form") {
      l.signed = true;
      activity(
        "George · person · Web",
        "Signed improvement plan with budget and stopping rules.",
      );
    }
    if (id === "l-stop-form") {
      l.stopped = true;
      l.climbTab = "Final report";
    }
    if (id === "l-adopt-form") {
      l.publishedDraft = true;
      activity(
        "George · person · Web",
        "Prepared architecture procedure @3 adoption draft; published @2 unchanged.",
      );
    }
    if (id === "l-triage-form") {
      if (!v.rationale.trim())
        return error("Record a causal explanation and evidence.");
      l.triaged = true;
      rounds.push({
        n: rounds.length,
        change: "Stall triage · five failure buckets",
        train: null,
        test: null,
        cost: 0,
        decision: "Triage",
        kind: "triage",
      });
    }
    if (id === "l-round-form") {
      if (!v.hypothesis.trim() || !v.falsifier.trim())
        return error("A hypothesis and falsifier are required.");
      if (Number(v.gain) < l.delta)
        return error(
          `Predicted ${v.gain} pp is below the signed ${l.delta} pp decision band.`,
        );
      if (rounds.reduce((sum, r) => sum + r.cost, 0) + 32.4 > l.ceiling)
        return error(
          "Cost ceiling would be exceeded by the next sample pass. Stop or obtain a new signed budget before another round.",
        );
      if (v.scenario === "guardrail")
        return error(
          "Candidate refused: quality 76% < registered floor 85%, even though cost falls.",
        );
      l.pendingRound = { ...v };
      close();
      if (l.roundGate) {
        if (reviewerRequired())
          signForm(
            "Review proposed round",
            "l-round-sign-form",
            note(
              "One hypothesis",
              `${E(v.hypothesis)}<br>Predicted +${E(v.gain)} pp; falsifier: ${E(v.falsifier)}<br>Instrument, split and graders remain fixed.`,
            ),
          );
      } else finishRound();
      return;
    }
    if (id === "l-round-sign-form") {
      if (!v.rationale?.trim())
        return error("Record a rationale for this round approval.");
      finishRound();
      return;
    }
    if (id === "l-promote-form") {
      if (!v.lesson.trim() || !v.evidence.trim())
        return error("Lesson and source provenance are required.");
      if (
        v.decision === "approve" &&
        (!v.rationale.trim() || v.evidence_status !== "observed")
      )
        return error(
          "Approval needs a rationale and explicit independent verification evidence. The original claim remains unverified.",
        );
      l.promotionDraft = true;
      l.promoted = v.decision === "approve";
      l.memoryTab = "Promotion trail";
    }
    if (id === "l-recall-form") {
      l.recall = true;
      l.memoryQuery = v.query;
      l.recallNotice =
        v.card === "WP-2048"
          ? "2 records · WP-2048 only"
          : "No fixture memory for WP-2061";
      render();
      return;
    }
    if (id === "l-campaign-form") {
      if (!v.hypothesis.trim())
        return error("A single measurable hypothesis is required.");
      l.cap = v.capability;
      switchClimb(v.instrument === "cost" ? "cost" : "quality");
      l.target = v.name;
      l.signed = false;
      close();
      navigate(v.instrument === "benchmark" ? "benchmarks" : "hillclimb");
      return;
    }
    if (id === "l-new-eval-form") {
      const ev = {
        id: "draft-" + l.evals.length,
        name: v.name,
        capability: v.capability,
        subject: profiles[v.capability].subject,
        cases: 0,
        tags: "Unclassified",
        ready: false,
      };
      l.evals.push(ev);
      switchEval(ev.id);
      l.evalTab = "Instrument";
    }
    if (id === "l-edit-eval-form") {
      Object.assign(selectedEval(), { name: v.name, tags: v.tags });
      invalidateInstrument();
    }
    if (id === "l-metrics-form") {
      if (!v.rubric.trim()) return error("Record the outcome grading rubric.");
      selectedEval().metric = v.metric;
      selectedEval().grader = v.grader;
      invalidateInstrument();
    }
    if (id === "l-eval-sign-form") {
      l.gates[l.pendingGate] = true;
      activity(
        "George · person · Web",
        `Approved eval ${l.pendingGate} artifact @${l.version}.`,
      );
    }
    if (id === "l-case-form") {
      if (!v.input.trim() || !v.tag.trim())
        return error("Subject input and first tag are required.");
      l.cases.push({
        id: "AC-" + (l.cases.length + 200),
        title: v.name,
        input: v.input,
        tag: v.tag,
        split: "train",
        evalId: l.evalId,
      });
      selectedEval().cases++;
      invalidateInstrument();
    }
    if (id === "l-import-form") {
      let items;
      try {
        items = JSON.parse(v.json);
      } catch {
        return error("Use a valid JSON array.");
      }
      if (
        !Array.isArray(items) ||
        !items.length ||
        items.length > 100 ||
        items.some(
          (i) =>
            !i ||
            typeof i.title !== "string" ||
            typeof i.input !== "string" ||
            typeof i.tag !== "string",
        )
      )
        return error(
          "Import 1–100 cases, each with title, input and tag strings.",
        );
      items.forEach((i) =>
        l.cases.push({
          id: "AC-" + (l.cases.length + 200),
          title: i.title,
          input: i.input,
          tag: i.tag,
          split: "train",
          evalId: l.evalId,
        }),
      );
      selectedEval().cases += items.length;
      invalidateInstrument();
    }
    if (id === "l-bench-promote-form") {
      l.promotedBenchmark = true;
    }
    if (id === "l-audit-form") l.auditStatus = v.decision;
    close();
    render();
    toast(
      "Local draft / decision recorded; published definitions and active run pins retained.",
    );
  });
  function finishRound() {
    const v = l.pendingRound;
    if (!v) return;
    const repeat = v.scenario === "repeat";
    rounds.push({
      n: rounds.length,
      change: v.hypothesis,
      train: repeat ? 72 : 96,
      test: repeat ? 94 : 85,
      cost: 32.4,
      decision: repeat ? "Confirm again" : "Revert",
      kind: repeat ? "repeat" : "overfit",
    });
    l.roundIndex = rounds.length - 1;
    l.pendingRound = null;
    close();
    render();
    toast(
      "One sample round completed and returned to caller. No automatic production change.",
    );
  }
  const oldOverview = enterprise.pages.overview;
  const operatingStrip = () =>
    `<div class="l-operating-strip">${[
      [
        "knowledge-design",
        "branch",
        "Design knowledge at every layer",
        "Capability → process → agent → resolved call",
      ],
      [
        "memory",
        "layers",
        "Inspect scoped memory",
        "History → card facts → reviewed reusable lessons",
      ],
      [
        "improvement",
        "chart",
        "Measure and improve",
        "Evals · hillclimb · benchmarks · cost",
      ],
    ]
      .map(
        ([page, icon, title, desc]) =>
          `<button data-action="navigate" data-page="${page}">${I(icon)}<span><strong>${title}</strong><small>${desc}</small></span>${I("arrow")}</button>`,
      )
      .join("")}</div>`;
  const oldKnowledge = enterprise.pages.knowledge;
  const titles = {
    "knowledge-design": "Knowledge design",
    memory: "Memory explorer",
    improvement: "Improvement control",
    hillclimb: "Hillclimb lab",
    benchmarks: "Benchmark harness",
    evals: "Evaluation studio",
    usage: "Inference & prompt audit",
  };
  return {
    titles,
    pages: {
      "knowledge-design": knowledgeDesign,
      memory: memoryExplorer,
      improvement,
      hillclimb,
      benchmarks,
      evals: evalStudio,
      usage,
      overview: () =>
        oldOverview().replace(
          '<div class="e-programmes">',
          operatingStrip() + '<div class="e-programmes">',
        ),
      knowledge: () =>
        oldKnowledge().replace(
          '<div class="e-knowledge-map">',
          operatingStrip() + '<div class="e-knowledge-map">',
        ),
    },
  };
};
