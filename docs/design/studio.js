"use strict";
(() => {
  const $ = (s, root = document) => root.querySelector(s);
  const $$ = (s, root = document) => [...root.querySelectorAll(s)];
  const esc = (x) =>
    String(x ?? "").replace(
      /[&<>"']/g,
      (c) =>
        ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        })[c],
    );
  const uid = (prefix) =>
    `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  const paths = {
    grid: "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
    projects: "M3 7h7l2 2h9v11H3zM3 7V4h7l2 3",
    flow: "M3 9h6v6H3zM15 2h6v6h-6zM15 16h6v6h-6zM9 12h3V5h3M12 12v7h3",
    agent: "M5 7h14v13H5zM12 3v4M8 12h1M15 12h1M9 16h6M2 11v5M22 11v5",
    check: "m5 12 4 4L19 6",
    shield: "m12 2 9 4v6c0 6-9 10-9 10S3 18 3 12V6zM8 12l3 3 5-6",
    book: "M12 5C9 2 4 3 2 4v16c4-2 7-1 10 1 3-2 6-3 10-1V4c-2-1-7-2-10 1Zm0 0v16",
    file: "M14 2H5v20h15V8zM14 2v6h6M8 12h8M8 16h6",
    chart: "M3 3v18h19M7 16l4-5 4 2 6-8",
    tools: "m14 6 4 4 4-4a7 7 0 0 1-9 9l-7 7-4-4 7-7a7 7 0 0 1 9-9z",
    code: "m8 6-6 6 6 6M16 6l6 6-6 6M14 3l-4 18",
    run: "m7 3 15 9-15 9z",
    search: "M16 16l5 5M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0",
    plus: "M12 4v16M4 12h16",
    arrow: "M4 12h16m-6-6 6 6-6 6",
    close: "m6 6 12 12M18 6 6 18",
    clock: "M12 6v6l4 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0",
    layers: "m12 2 10 6-10 6L2 8zM2 12l10 6 10-6M2 16l10 6 10-6",
    globe:
      "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18",
    inbox: "M3 3h18v18H3zM3 14h5l2 3h4l2-3h5",
    board: "M3 3h18v18H3zM9 3v18M15 3v18M6 7v5M12 7v8M18 7v3",
    model: "M3 3h6v6H3zM15 15h6v6h-6zM15 3h6v6h-6zM9 6h6M6 9v9h9",
    person: "M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M4 22v-3a8 8 0 0 1 16 0v3",
    download: "M12 2v13m-5-5 5 5 5-5M4 16v5h16v-5",
    chat: "M3 3h18v14H8l-5 5zM7 7h10M7 11h7",
    bell: "M5 17h14l-2-3V8a5 5 0 0 0-10 0v6zM10 21h4",
    edit: "m15 3 6 6-12 12H3v-6zM12 6l6 6",
    settings: "M3 7h18M3 17h18M9 4v6M16 14v6",
    branch: "M6 3v14a3 3 0 1 0 3 3M18 3v6a3 3 0 0 1-3 3H6",
    replay: "M3 11a9 9 0 1 1 2 7M3 3v8h8",
  };
  const icon = (name) =>
    `<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="${paths[name] || paths.file}"/></svg>`;
  const badge = (label, color = "") =>
    `<span class="badge ${color}">${esc(label)}</span>`;
  const btn = (label, action, data = "", style = "", symbol = "") =>
    `<button class="btn ${style}" data-action="${action}" ${data}>${symbol ? icon(symbol) : ""}${esc(label)}</button>`;
  const navButton = (page, label, symbol, count = "") =>
    `<button class="nav-link ${state.page === page || (page === "projects" && state.page === "project") ? "active" : ""}" data-action="navigate" data-page="${page}" ${state.page === page ? 'aria-current="page"' : ""}>${icon(symbol)}${label}${count ? `<span class="nav-count ${page === "decisions" ? "attention" : ""}">${count}</span>` : ""}</button>`;
  const db = {
    types: [
      {
        id: "product",
        name: "Product development",
        description:
          "Turn approved intent into a verified, reviewable software increment.",
        status: "Published",
        version: 4,
        phases: ["Discover", "Define", "Design", "Build", "Verify", "Release"],
        capabilities: ["discovery", "architecture", "delivery", "verification"],
        cardTypes: [
          "Requirement",
          "Architecture task",
          "Feature",
          "Test report",
        ],
        gates: [
          "Scope approval",
          "Architecture review",
          "Implementation review",
        ],
      },
      {
        id: "modernisation",
        name: "Service modernisation",
        description:
          "Migrate a service with explicit contracts, parity checks and architecture evidence.",
        status: "Published",
        version: 2,
        phases: ["Assess", "Baseline", "Migrate", "Prove parity", "Cut over"],
        capabilities: ["architecture", "delivery", "verification"],
        cardTypes: ["Migration task", "Contract", "Parity report"],
        gates: ["Baseline approval", "Cutover sign-off"],
      },
      {
        id: "research",
        name: "Research & discovery",
        description:
          "Investigate a question and turn evidence into a decision.",
        status: "Published",
        version: 3,
        phases: ["Frame", "Investigate", "Synthesize", "Review"],
        capabilities: ["discovery", "architecture"],
        cardTypes: ["Research question", "Finding", "Decision"],
        gates: ["Research review"],
      },
      {
        id: "incident",
        name: "Incident resolution",
        description: "Reproduce, isolate and repair a production defect.",
        status: "Draft",
        version: 1,
        phases: ["Triage", "Reproduce", "Repair", "Verify"],
        capabilities: ["delivery", "verification"],
        cardTypes: ["Bug", "Test report"],
        gates: ["Fix review"],
      },
    ],
    capabilities: [
      {
        id: "discovery",
        name: "Requirements discovery",
        description:
          "Refine stakeholder intent into testable requirements and traceable decisions.",
        status: "Published",
        version: 3,
        processes: ["refine"],
        knowledge: "requirements-method@3",
      },
      {
        id: "architecture",
        name: "Architecture & modelling",
        description:
          "Build ArchiMate and UML models, validate views and document architecture decisions.",
        status: "Published",
        version: 2,
        processes: ["model"],
        knowledge: "architecture-standards@2",
      },
      {
        id: "delivery",
        name: "Software delivery",
        description:
          "Implement a work package in a coding workspace and produce a reviewable patch.",
        status: "Published",
        version: 7,
        processes: ["feature", "repair"],
        knowledge: "engineering-handbook@5",
      },
      {
        id: "verification",
        name: "Verification & quality",
        description:
          "Collect independent test, contract and architecture evidence for a work package.",
        status: "Published",
        version: 4,
        processes: ["verify"],
        knowledge: "quality-policy@4",
      },
      {
        id: "improve",
        name: "Continuous improvement",
        description:
          "Evaluate procedure changes against a signed, versioned instrument.",
        status: "Draft",
        version: 1,
        processes: ["improve"],
        knowledge: "eval-method@1",
      },
    ],
    agents: [
      {
        id: "analyst",
        name: "Requirements analyst",
        role: "Make the intent precise and testable.",
        kind: "Internal agent",
        model: "reasoning",
        prompt: "requirements_analyst@3",
        maxTurns: 12,
        budget: 1.5,
        tools: [
          "requirements.list",
          "requirements.create",
          "requirements.update",
          "principles.list",
          "trace_links.create",
        ],
        status: "Published",
        version: 3,
      },
      {
        id: "architect",
        name: "Solution architect",
        role: "Connect requirements to a coherent model and a small set of explicit decisions.",
        kind: "Internal agent",
        model: "reasoning",
        prompt: "solution_architect@2",
        maxTurns: 16,
        budget: 2,
        tools: [
          "models.create",
          "model_elements.create",
          "model_relationships.create",
          "model_views.create",
          "models.validate",
          "decisions.create",
          "trigger_architecture_analysis",
        ],
        status: "Published",
        version: 2,
      },
      {
        id: "builder",
        name: "Implementation agent",
        role: "Implement the approved plan, verify the resulting files and attach a patch.",
        kind: "Internal agent",
        model: "router/medium",
        prompt: "implementation_role@4",
        maxTurns: 24,
        budget: 3,
        tools: [
          "import_repository_as_workspace",
          "list_workspace_files",
          "write_workspace_file",
          "exec_in_workspace_async",
          "get_exec_job_status",
          "get_workspace_git_status",
          "create_pull_request",
        ],
        status: "Published",
        version: 4,
      },
      {
        id: "verifier",
        name: "Verification agent",
        role: "Run the approved test suite and report independent evidence.",
        kind: "Internal agent",
        model: "generation",
        prompt: "verify_feature@4",
        maxTurns: 10,
        budget: 1,
        tools: [
          "create_test_suite",
          "execute_test_suite",
          "get_test_suite_results",
          "get_workspace_git_status",
          "list_workspace_files",
        ],
        status: "Published",
        version: 4,
      },
      {
        id: "planner",
        name: "Delivery planner",
        role: "Convert the approved requirement and model into a bounded implementation plan.",
        kind: "Internal agent",
        model: "reasoning",
        prompt: "delivery_plan@2",
        maxTurns: 8,
        budget: 0.8,
        tools: [
          "requirements.get",
          "principles.list",
          "model_views.get",
          "list_workspace_files",
          "memory.recall",
        ],
        status: "Published",
        version: 2,
      },
    ],
    processes: [
      {
        id: "feature",
        name: "Build a feature",
        description:
          "Plan → implement → independently verify → ask for a human review.",
        capability: "delivery",
        status: "Published",
        version: 7,
        steps: [
          {
            name: "Read approved scope",
            kind: "Action",
            agent: "",
            tool: "requirements.get",
          },
          {
            name: "Plan implementation",
            kind: "Internal agent",
            agent: "planner",
            tool: "list_workspace_files",
          },
          {
            name: "Implement work package",
            kind: "Internal agent",
            agent: "builder",
            tool: "exec_in_workspace_async",
          },
          {
            name: "Verify acceptance",
            kind: "Internal agent",
            agent: "verifier",
            tool: "execute_test_suite",
          },
          {
            name: "Review implementation",
            kind: "Human gate",
            agent: "",
            tool: "approvals.decide",
          },
          {
            name: "Publish result",
            kind: "Action",
            agent: "",
            tool: "cards.update",
          },
        ],
      },
      {
        id: "repair",
        name: "Repair a defect",
        description:
          "Reproduce the issue, make the smallest fix and prove the regression.",
        capability: "delivery",
        status: "Published",
        version: 3,
        steps: [
          {
            name: "Reproduce defect",
            kind: "Internal agent",
            agent: "verifier",
            tool: "exec_in_workspace_async",
          },
          {
            name: "Implement repair",
            kind: "Internal agent",
            agent: "builder",
            tool: "write_workspace_file",
          },
          {
            name: "Regression review",
            kind: "Human gate",
            agent: "",
            tool: "approvals.decide",
          },
        ],
      },
      {
        id: "refine",
        name: "Refine requirements",
        description:
          "Read principles, draft acceptance criteria and request scope approval.",
        capability: "discovery",
        status: "Published",
        version: 3,
        steps: [
          {
            name: "Read governing principles",
            kind: "Action",
            agent: "",
            tool: "principles.list",
          },
          {
            name: "Refine requirement",
            kind: "Internal agent",
            agent: "analyst",
            tool: "requirements.create",
          },
          {
            name: "Approve scope",
            kind: "Human gate",
            agent: "",
            tool: "approvals.decide",
          },
        ],
      },
      {
        id: "model",
        name: "Design architecture",
        description:
          "Create a model and views, validate and request an architecture review.",
        capability: "architecture",
        status: "Published",
        version: 2,
        steps: [
          {
            name: "Read requirements",
            kind: "Action",
            agent: "",
            tool: "requirements.list",
          },
          {
            name: "Create model and views",
            kind: "Internal agent",
            agent: "architect",
            tool: "models.create",
          },
          {
            name: "Validate model",
            kind: "Action",
            agent: "",
            tool: "models.validate",
          },
          {
            name: "Review architecture",
            kind: "Human gate",
            agent: "",
            tool: "approvals.decide",
          },
        ],
      },
      {
        id: "verify",
        name: "Verify a work package",
        description: "Execute a pinned suite and attach the results.",
        capability: "verification",
        status: "Published",
        version: 4,
        steps: [
          {
            name: "Inspect workspace state",
            kind: "Action",
            agent: "",
            tool: "get_workspace_git_status",
          },
          {
            name: "Execute test suite",
            kind: "Internal agent",
            agent: "verifier",
            tool: "execute_test_suite",
          },
          {
            name: "Collect evidence",
            kind: "Action",
            agent: "",
            tool: "get_test_suite_results",
          },
        ],
      },
      {
        id: "improve",
        name: "Evaluate a candidate",
        description:
          "Measure held-out results under the same instrument digest.",
        capability: "improve",
        status: "Draft",
        version: 1,
        steps: [
          {
            name: "Check instrument sign-offs",
            kind: "Human gate",
            agent: "",
            tool: "eval_signoffs.decide",
          },
          {
            name: "Run evaluation",
            kind: "Action",
            agent: "",
            tool: "eval_runs.start",
          },
        ],
      },
    ],
    projects: [
      {
        id: "portal",
        key: "PRJ-014",
        name: "Customer portal",
        description:
          "Self-service workspace invitations with clear access boundaries.",
        type: "product",
        phase: 4,
        status: "Needs review",
        tone: "amber",
        agents: ["planner", "builder", "verifier"],
        completed: 8,
        total: 12,
        spent: 4.28,
        process: "feature",
        symbol: "projects",
      },
      {
        id: "billing",
        key: "PRJ-018",
        name: "Billing API migration",
        description:
          "Move invoicing to Rust while preserving the public API contract.",
        type: "modernisation",
        phase: 2,
        status: "Running",
        tone: "green",
        agents: ["architect", "builder", "verifier"],
        completed: 5,
        total: 18,
        spent: 6.31,
        process: "feature",
        symbol: "code",
      },
      {
        id: "identity",
        key: "PRJ-021",
        name: "Identity & access",
        description:
          "Model tenant identity, service boundaries and delegation rules.",
        type: "research",
        phase: 2,
        status: "Needs input",
        tone: "amber",
        agents: ["analyst", "architect"],
        completed: 3,
        total: 7,
        spent: 1.48,
        process: "model",
        symbol: "shield",
      },
      {
        id: "insights",
        key: "PRJ-023",
        name: "Usage insights",
        description:
          "Make usage and cost attribution understandable for each tenant.",
        type: "product",
        phase: 1,
        status: "Running",
        tone: "green",
        agents: ["analyst", "planner"],
        completed: 2,
        total: 9,
        spent: 0.77,
        process: "refine",
        symbol: "chart",
      },
    ],
    decisions: [
      {
        id: "review",
        project: "portal",
        name: "Review workspace invitations",
        type: "Implementation review",
        age: "12 min",
        state: "Pending",
        note: "",
      },
      {
        id: "scope",
        project: "identity",
        name: "Confirm service-account delegation",
        type: "Scope question",
        age: "26 min",
        state: "Pending",
        note: "",
      },
      {
        id: "grading",
        project: "billing",
        name: "Sign off contract-parity grading",
        type: "Eval grading",
        age: "41 min",
        state: "Pending",
        note: "",
      },
    ],
    requirements: [
      {
        id: "invite",
        key: "REQ-042",
        name: "Invite a member to a workspace",
        description:
          "An administrator can invite a person to a workspace. The invitation is tenant-bound, expires after 72 hours and can be accepted only once.",
        acceptance:
          "Only an administrator can create an invitation.\nA used or expired invitation is refused.\nAn invitation never grants access to another tenant.\nThe decision is recorded in the audit history.",
        status: "Approved",
        version: 3,
        project: "portal",
        principle: "isolation",
        history: [
          "v3 · Approved · George · today, 09:12",
          "v2 · Acceptance criteria clarified · Requirements analyst",
          "v1 · Draft created · George",
        ],
      },
      {
        id: "contract",
        key: "REQ-057",
        name: "Preserve the invoice API contract",
        description:
          "The migrated service must preserve the observable invoice behavior under the pinned parity suite.",
        acceptance:
          "Every approved contract case passes.\nError codes and response shapes are unchanged.",
        status: "Approved",
        version: 2,
        project: "billing",
        principle: "evidence",
        history: ["v2 · Approved · George", "v1 · Imported contract scope"],
      },
      {
        id: "delegation",
        key: "REQ-061",
        name: "Bound service-account delegation",
        description:
          "A delegated service identity receives only the explicitly approved actions in its tenant.",
        acceptance: "A delegated action outside the allowlist is refused.",
        status: "Draft",
        version: 1,
        project: "identity",
        principle: "isolation",
        history: ["v1 · Draft created · Requirements analyst"],
      },
      {
        id: "usage",
        key: "REQ-063",
        name: "Attribute usage to its source",
        description:
          "Each model call can be traced to the project, run or conversation that incurred it.",
        acceptance:
          "Usage with an unavailable source is marked unknown.\nAgent-platform calls are its own product and excluded from Conveyor model totals.",
        status: "Draft",
        version: 1,
        project: "insights",
        principle: "evidence",
        history: ["v1 · Draft created · George"],
      },
    ],
    principles: [
      {
        id: "isolation",
        key: "PRN-001",
        name: "Tenant isolation by construction",
        description:
          "Every resource and every action resolves to the authenticated tenant. Tenant boundaries are enforced before the action executes.",
        acceptance:
          "Use tenant-bound identities and explicit action grants. A cross-tenant reference must never expand access.",
        status: "Approved",
        version: 4,
        project: "",
        history: [
          "v4 · Approved · George",
          "v3 · Delegation guidance added",
          "v1 · Principle established",
        ],
      },
      {
        id: "evidence",
        key: "PRN-004",
        name: "Evidence before progression",
        description:
          "A work package advances when its declared evidence and review gates are satisfied.",
        acceptance:
          "Tests, model validation and human decisions must be traceable to the exact work and version they evaluate.",
        status: "Approved",
        version: 2,
        project: "",
        history: ["v2 · Approved · George", "v1 · Principle established"],
      },
      {
        id: "human",
        key: "PRN-007",
        name: "A question is an approval",
        description:
          "Questions, sign-offs and reviews use one durable decision primitive across Web, Telegram and MCP.",
        acceptance:
          "A question waits without holding a worker. Its answer records the actor kind and delivery channel.",
        status: "Approved",
        version: 1,
        project: "",
        history: ["v1 · Approved · George"],
      },
    ],
    models: [
      {
        id: "portal-arch",
        name: "Customer portal architecture",
        language: "ArchiMate",
        project: "portal",
        version: 3,
        status: "Draft",
        views: [
          {
            id: "v-context",
            name: "Access & invitation context",
            kind: "archimate_view",
            elements: ["prn", "req", "admin", "invites", "app", "data"],
          },
          {
            id: "v-application",
            name: "Application cooperation",
            kind: "archimate_view",
            elements: ["app", "data"],
          },
        ],
        elements: [
          {
            id: "prn",
            name: "Tenant isolation",
            kind: "principle",
            asset: "isolation",
          },
          {
            id: "req",
            name: "Workspace invitations",
            kind: "requirement",
            asset: "invite",
          },
          {
            id: "admin",
            name: "Workspace administrator",
            kind: "business_role",
          },
          { id: "invites", name: "Invite a member", kind: "business_process" },
          { id: "app", name: "Customer portal", kind: "application_component" },
          { id: "data", name: "Invitation", kind: "data_object" },
        ],
        relationships: [
          { id: "r1", source: "prn", target: "req", kind: "influence" },
          { id: "r2", source: "req", target: "invites", kind: "association" },
          { id: "r3", source: "admin", target: "invites", kind: "assignment" },
          { id: "r4", source: "app", target: "invites", kind: "serving" },
          { id: "r5", source: "app", target: "data", kind: "access" },
        ],
      },
      {
        id: "portal-uml",
        name: "Invitation domain",
        language: "UML",
        project: "portal",
        version: 2,
        status: "Draft",
        views: [
          {
            id: "v-class",
            name: "Invitation class model",
            kind: "class",
            elements: ["workspace", "invitation", "member"],
          },
        ],
        elements: [
          {
            id: "workspace",
            name: "Workspace",
            kind: "class",
            documentation: "id: UUID\ntenantId: UUID",
          },
          {
            id: "invitation",
            name: "Invitation",
            kind: "class",
            asset: "invite",
            documentation:
              "token: String\nexpiresAt: DateTime\naccept(): Result",
          },
          {
            id: "member",
            name: "Member",
            kind: "class",
            documentation: "id: UUID\nrole: WorkspaceRole",
          },
        ],
        relationships: [
          {
            id: "u1",
            source: "workspace",
            target: "invitation",
            kind: "composition",
          },
          {
            id: "u2",
            source: "invitation",
            target: "member",
            kind: "dependency",
          },
        ],
      },
      {
        id: "identity-arch",
        name: "Identity service landscape",
        language: "ArchiMate",
        project: "identity",
        version: 1,
        status: "Draft",
        views: [
          {
            id: "v-identity",
            name: "Service boundaries",
            kind: "archimate_view",
            elements: ["tenant", "service", "identity"],
          },
        ],
        elements: [
          { id: "tenant", name: "Tenant", kind: "business_actor" },
          {
            id: "service",
            name: "Delegated service",
            kind: "application_component",
          },
          {
            id: "identity",
            name: "Identity provider",
            kind: "application_component",
          },
        ],
        relationships: [
          { id: "i1", source: "identity", target: "service", kind: "serving" },
          { id: "i2", source: "service", target: "tenant", kind: "serving" },
        ],
      },
    ],
    cards: [
      {
        id: "C-142",
        project: "portal",
        name: "Workspace invitations",
        list: "Review",
        requirement: "invite",
      },
      {
        id: "C-144",
        project: "portal",
        name: "Invitation email template",
        list: "Ready",
        requirement: "invite",
      },
      {
        id: "C-145",
        project: "portal",
        name: "Expired invitation state",
        list: "Build",
        requirement: "invite",
      },
      {
        id: "C-139",
        project: "portal",
        name: "Workspace access policy",
        list: "Done",
        requirement: "invite",
      },
      {
        id: "C-203",
        project: "billing",
        name: "Port invoice endpoint",
        list: "Build",
        requirement: "contract",
      },
    ],
    activity: [
      {
        actor: "Verification agent",
        text: "attached a test report to Customer portal",
        time: "2 min ago",
        tone: "green",
      },
      {
        actor: "Implementation agent",
        text: "handed a patch to Verification agent",
        time: "6 min ago",
        tone: "purple",
      },
      {
        actor: "George",
        text: "approved the Billing API migration scope",
        time: "24 min ago",
        tone: "orange",
      },
    ],
    customTools: [],
  };
  const coreTools = [
    "requirements.list",
    "requirements.get",
    "requirements.create",
    "requirements.update",
    "principles.list",
    "principles.create",
    "principles.update",
    "trace_links.create",
    "models.create",
    "models.validate",
    "model_elements.create",
    "model_relationships.create",
    "model_views.create",
    "model_views.get",
    "decisions.create",
    "memory.recall",
    "approvals.decide",
    "cards.update",
    "eval_runs.start",
    "eval_signoffs.decide",
  ].map((id) => ({
    id,
    title: id.split(".").join(" · "),
    group: "Conveyor",
    method: "ACTION",
    path: "One action registry · Web / Telegram / MCP",
    role:
      id.endsWith(".get") || id.endsWith(".list") || id.endsWith(".recall")
        ? "observer"
        : "producer",
    verifies: [],
    fields: [],
    source: "conveyor",
  }));
  const coding = window.CODING_CATALOGUE;
  const allTools = () => [
    ...coding.operations,
    ...coreTools,
    ...db.customTools,
  ];
  const byId = (collection, id) => db[collection].find((x) => x.id === id);
  const nameOf = (collection, id) =>
    byId(collection, id)?.name || id || "Unassigned";
  const pending = () => db.decisions.filter((d) => d.state === "Pending");
  const toolOf = (id) => allTools().find((t) => t.id === id);
  const initials = (id) =>
    nameOf("agents", id)
      .split(" ")
      .map((x) => x[0])
      .slice(0, 2)
      .join("");
  const state = {
    page: "overview",
    project: "portal",
    projectTab: "Workbench",
    mode: "Canvas",
    inspectorTab: "Evidence",
    node: 4,
    selected: {
      types: "product",
      capabilities: "delivery",
      processes: "feature",
      agents: "builder",
      requirements: "invite",
      principles: "isolation",
      models: "portal-arch",
    },
    catalogTab: "Definition",
    projectFilter: "All projects",
    entityQuery: "",
    toolQuery: "",
    toolGroup: "All actions",
    tool: "exec_in_workspace_async",
    modelView: "v-context",
    modelTab: "Elements",
    modelElement: null,
    decision: "review",
    runFilter: "All runs",
    modelReport: null,
    chat: [],
    evalSigned: false,
    evalQueued: false,
    zoom: 100,
  };
  let toastTimer;
  function toast(text) {
    $("#toast").textContent = text;
    $("#toast").classList.add("show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => $("#toast").classList.remove("show"), 3600);
  }
  function activity(actor, text, tone = "green") {
    db.activity.unshift({ actor, text, time: "just now", tone });
  }
  function navigate(page, id) {
    state.page = page;
    history.replaceState(null, "", "#" + page);
    state.entityQuery = "";
    state.catalogTab = "Definition";
    if (id && state.selected[page] !== undefined) state.selected[page] = id;
    if (page === "project" && id) {
      state.project = id;
      state.node = id === "portal" ? 4 : 0;
      state.projectTab = "Workbench";
    }
    if (page === "models") {
      const m = byId("models", state.selected.models);
      state.modelView = m?.views[0]?.id;
      state.modelReport = null;
    }
    document.body.classList.remove("nav-open");
    $(".nav-scrim").hidden = true;
    render();
    window.scrollTo(0, 0);
  }
  function header(title, subtitle, actions = "", eyebrow = "Tenant workspace") {
    return `<div class="page-head"><div><span class="eyebrow">${esc(eyebrow)}</span><h1>${esc(title)}</h1><p>${esc(subtitle)}</p></div><div class="row">${actions}</div></div>`;
  }
  function renderSidebar() {
    if (enterprise) {
      $("#sidebar").innerHTML = enterprise.sidebar();
      const nav = $("#sidebar nav"),
        active = $("#sidebar .nav-link.active");
      if (nav && active)
        nav.scrollTop = Math.max(
          0,
          active.offsetTop - nav.offsetTop - nav.clientHeight + 50,
        );
      return;
    }
    $("#sidebar").innerHTML =
      `<div class="brand"><span class="brand-mark">${icon("layers")}</span><div><div class="brand-word">conveyor</div><div class="brand-sub">WORK, WITH INTENT</div></div></div><div class="tenant-switch"><span class="tenant-mark">N</span><span class="grow">Non-Agentic AI<small style="display:block;font-size:9px;color:var(--nav-muted)">Tenant workspace</small></span><span>⌄</span></div><nav>
<div class="nav-group"><div class="nav-label">OPERATE</div>${navButton("overview", "Overview", "grid")}${navButton("projects", "Projects", "projects", db.projects.length)}${navButton("decisions", "Decisions", "inbox", pending().length || "")}${navButton("runs", "Runs & activity", "run")}</div>
<div class="nav-group"><div class="nav-label">DESIGN YOUR CONVEYOR</div>${navButton("types", "Project types", "layers")}${navButton("capabilities", "Capabilities", "grid")}${navButton("processes", "Processes", "flow")}${navButton("agents", "Agents", "agent")}${navButton("tools", "Tools & actions", "tools")}</div>
<div class="nav-group"><div class="nav-label">GOVERN & MODEL</div>${navButton("requirements", "Requirements", "file")}${navButton("principles", "Principles", "shield")}${navButton("models", "Architecture & UML", "model")}</div>
<div class="nav-group"><div class="nav-label">LEARN</div>${navButton("evals", "Evaluations", "chart")}${navButton("usage", "Usage & audit", "clock")}</div></nav><div class="sidebar-bottom"><b>A system that gets better.</b>Your processes. Your agents. Your work.<div class="row"><i class="dot" style="color:#9fb59d"></i>Local design study <span class="grow"></span><button data-action="about" aria-label="About this design" style="color:inherit">↗</button></div></div>`;
  }
  function stats() {
    const running = db.projects.filter((p) => p.status === "Running").length;
    return `<div class="metrics">${[
      [
        db.projects.length,
        "Projects in flight",
        `${running} running · ${db.projects.filter((p) => p.tone === "amber").length} waiting${db.projects.some((p) => p.status === "Queued") ? " · " + db.projects.filter((p) => p.status === "Queued").length + " queued" : ""}`,
        "green",
      ],
      [
        new Set(db.projects.flatMap((p) => p.agents)).size,
        "Agents assigned",
        "Across all active projects",
        "purple",
      ],
      [
        pending().length,
        "Decisions waiting",
        "Your input moves the work forward",
        "orange",
      ],
      [
        "$" + db.projects.reduce((n, p) => n + p.spent, 0).toFixed(2),
        "Conveyor usage today",
        "Agent-platform costs excluded",
        "green",
      ],
    ]
      .map(
        ([n, l, s, c]) =>
          `<div class="metric"><div class="label"><i class="dot ${c}"></i>${l}</div><div class="metric-number">${n}</div><small>${s}</small><svg viewBox="0 0 75 30" aria-hidden="true"><path d="M0 26 8 22 15 24 24 14 31 18 40 12 48 15 56 5 64 9 75 1" fill="none" stroke="${c === "orange" ? "#dba16b" : c === "purple" ? "#b3a3cd" : "#9db7a4"}" stroke-width="1.5"/></svg></div>`,
      )
      .join("")}</div>`;
  }
  function projectCard(p) {
    const type = p.typePin || byId("types", p.type);
    return `<button class="project-card" data-action="open-project" data-id="${p.id}"><div class="project-card-top"><span class="project-symbol ${p.id === "identity" ? "purple" : p.id === "billing" ? "orange" : ""}">${icon(p.symbol)}</span><div class="grow"><h3>${esc(p.name)}</h3><div class="project-meta"><span class="mono">${esc(p.key)}</span> · ${esc(type?.name)}</div></div>${badge(p.status, p.tone)}</div><p class="description">${esc(p.description)}</p><div class="mini-pipeline" aria-label="Phase ${p.phase + 1} of ${type?.phases.length}">${(type?.phases || []).map((_, i) => `<span class="${i < p.phase ? "done" : i === p.phase ? (p.tone === "amber" ? "current" : "running") : ""}"></span>`).join("")}</div><div class="project-foot"><span>${esc(type?.phases[p.phase] || "Queued")} <span style="opacity:.45">/</span> ${p.completed} of ${p.total} work items</span><div class="row"><span class="avatar-stack">${p.agents.map((id) => `<span title="${esc(nameOf("agents", id))}">${initials(id)}</span>`).join("")}</span><span>↗</span></div></div></button>`;
  }
  function decisionRows() {
    return (
      pending()
        .map(
          (d) =>
            `<button class="decision-row" data-action="review" data-id="${d.id}"><span class="overline">${esc(nameOf("projects", d.project))} <span style="float:right">↗</span></span><h3>${esc(d.name)}</h3><small>${esc(d.type)} · waiting ${d.age}</small></button>`,
        )
        .join("") ||
      '<div class="empty">All caught up.<p>The work can keep moving.</p></div>'
    );
  }
  function activityList() {
    return db.activity
      .slice(0, 4)
      .map(
        (a) =>
          `<div class="activity-item"><i class="dot ${a.tone}"></i><div><strong>${esc(a.actor)}</strong> ${esc(a.text)}<small>${esc(a.time)}</small></div></div>`,
      )
      .join("");
  }
  function overview() {
    const projects = db.projects.filter(
      (p) =>
        state.projectFilter === "All projects" ||
        (state.projectFilter === "Needs attention"
          ? p.tone === "amber"
          : p.status === "Running"),
    );
    return `<div class="page">${header("Your work, in motion.", "A clear line from intent to execution. Across every project in your tenant.", btn("New project", "create-project", "", "primary", "plus"), "NON-AGENTIC AI / OPERATIONS")}${stats()}<div class="overview-grid"><div><div class="section-title"><h2>Active projects <span>${db.projects.length.toString().padStart(2, "0")}</span></h2>${btn("View all", "navigate", 'data-page="projects"', "quiet small", "arrow")}</div><div class="filter-row"><div class="segmented">${["All projects", "Running", "Needs attention"].map((x) => `<button class="${state.projectFilter === x ? "selected" : ""}" data-action="project-filter" data-value="${x}" aria-pressed="${state.projectFilter === x}">${x}</button>`).join("")}</div><span class="mono muted">${projects.length} projects</span></div><div class="project-list">${projects.map(projectCard).join("") || '<div class="empty">No projects match this filter.</div>'}</div><div class="configuration-banner">${icon("flow")}<div class="grow"><h3>Built around the way you work.</h3><p>Define a project type. Connect capabilities, processes, agents and tools. Reuse them across projects.</p></div>${btn("Open studio", "navigate", 'data-page="types"', "small", "arrow")}</div></div><aside class="overview-aside"><section class="attention-panel"><div class="attention-intro"><span class="eyebrow">${pending().length.toString().padStart(2, "0")} DECISIONS WAITING</span><h2>The human part.</h2><p>A few things need your judgment.</p></div>${decisionRows()}<div class="attention-footer"><span class="mono muted" style="font-size:9px">One inbox · Web, Telegram & MCP</span></div></section><section class="activity-panel"><div class="section-title"><h2>Along the conveyor</h2><i class="dot green"></i></div>${activityList()}${btn("Open activity", "navigate", 'data-page="runs"', "quiet small", "arrow")}</section></aside></div></div>`;
  }
  function projectsPage() {
    return `<div class="page">${header("Projects", "Every project has its own type, scope, work packages and execution history.", btn("New project", "create-project", "", "primary", "plus"))}${stats()}<div class="toolbar"><span class="mono muted">${db.projects.length} ACTIVE PROJECTS · TENANT SCOPE</span>${btn("Manage project types", "navigate", 'data-page="types"', "small", "layers")}</div><div class="project-list">${db.projects.map(projectCard).join("")}</div></div>`;
  }
  function chain() {
    return `<div class="chain" aria-label="Tenant configuration hierarchy">${[
      ["types", "Project type", "layers"],
      ["capabilities", "Capability", "grid"],
      ["processes", "Process", "flow"],
      ["agents", "Agent", "agent"],
      ["tools", "Tools & actions", "tools"],
    ]
      .map(
        ([p, l, i], n) =>
          `${n ? '<span class="chevron">›</span>' : ""}<button class="${state.page === p ? "selected" : ""}" data-action="navigate" data-page="${p}">${icon(i)}${l}</button>`,
      )
      .join(
        "",
      )}<span class="grow"></span><span class="mono muted" style="font-size:8px;white-space:nowrap">TENANT DEFINITIONS</span></div>`;
  }
  function projectPage() {
    const p = byId("projects", state.project),
      type = p.typePin || byId("types", p.type);
    const captions = {
      Workbench:
        "Follow the work, inspect the evidence and step in at the right moment.",
      "Work board":
        "Work packages keep their own state, separate from the project lifecycle.",
      Requirements:
        "Approved intent and the principles that govern this project.",
      Architecture: "Models and views linked to this project.",
      Artifacts: "Outputs produced by this project’s processes.",
    };
    return `<div class="page">${header(p.name, captions[state.projectTab], `${btn("Project brief", "brief", "", "", "file")}${btn("Add work", "add-card", "", "primary", "plus")}`, `${p.key} / ${type.name}`)}<div class="lifecycle">${type.phases.map((phase, i) => `<button class="${i === p.phase ? "current" : ""}" data-action="phase-info" data-index="${i}"><span class="step-number">${i < p.phase ? "✓" : String(i + 1).padStart(2, "0")}</span>${esc(phase)}</button>`).join("")}</div><div class="between" style="margin-bottom:8px"><div class="tabs" style="margin-bottom:0;flex:1">${[
      ["Workbench", "flow"],
      ["Work board", "board"],
      ["Requirements", "file"],
      ["Architecture", "model"],
      ["Artifacts", "layers"],
    ]
      .map(
        ([t, i]) =>
          `<button class="${state.projectTab === t ? "selected" : ""}" data-action="project-tab" data-value="${t}" aria-pressed="${state.projectTab === t}">${icon(i)}${t}</button>`,
      )
      .join(
        "",
      )}</div></div><div style="height:15px"></div>${state.projectTab === "Workbench" ? workbench(p) : state.projectTab === "Work board" ? board(p) : state.projectTab === "Requirements" ? projectRequirements(p) : state.projectTab === "Architecture" ? projectModels(p) : artifacts(p)}</div>`;
  }
  function nodeStatus(i, p) {
    const d = db.decisions.find((x) => x.project === p.id && x.id === "review");
    if (p.id === "portal") {
      if (d?.state === "Returned")
        return i === 2
          ? "Ready for revision"
          : i === 3
            ? "Awaiting revised patch"
            : i === 4
              ? "Returned"
              : i === 5
                ? "Queued"
                : "Completed";
      if (d?.state === "Approved") return i === 5 ? "Queued" : "Completed";
      return i < 4 ? "Completed" : i === 4 ? "Needs review" : "Queued";
    }
    return i < p.phase
      ? "Completed"
      : i === p.phase
        ? p.tone === "amber"
          ? "Waiting for input"
          : "Running"
        : "Queued";
  }
  function workbench(p) {
    const proc = p.processPin || byId("processes", p.process);
    state.node = Math.min(state.node, proc.steps.length - 1);
    const ask = db.decisions.find(
      (d) => d.project === p.id && d.state === "Pending",
    );
    return `<div class="workspace-layout"><div class="workbench-column">${ask ? `<div class="ask-strip">${icon("bell")}<div class="grow"><strong>${esc(ask.name)}</strong><small>${esc(ask.type)} · the run is waiting for your decision</small></div>${btn("Open review", "review", `data-id="${ask.id}"`, "small")}</div>` : ""}<div class="canvas-panel"><div class="canvas-toolbar"><div class="row">${icon("flow")}<h3 style="font-size:12px">${esc(proc.name)}</h3>${badge("v" + proc.version)}</div><div class="segmented">${["Canvas", "Session"].map((m) => `<button class="${state.mode === m ? "selected" : ""}" data-action="mode" data-value="${m}">${m}</button>`).join("")}</div></div>${state.mode === "Canvas" ? canvas(p, proc) : `<div class="transcript-body" style="max-height:480px;min-height:345px">${messages(p, proc)}</div>`}<div class="canvas-legend"><span class="row"><i class="dot green"></i>Internal agent</span><span class="row"><i class="dot purple"></i>Deterministic action</span><span class="row"><i class="dot amber"></i>Human gate</span><span class="grow"></span><button data-action="open-definition" data-kind="processes" data-id="${proc.id}" class="text-button">Inspect process ↗</button></div></div>${state.mode === "Canvas" ? `<section class="panel transcript"><div class="transcript-head"><h3>Execution transcript</h3><span class="mono muted" style="font-size:8px">SAMPLE RUN · ${p.key}</span></div><div class="transcript-body">${messages(p, proc)}</div>${composer()}</section>` : `<section class="panel transcript">${composer()}</section>`}</div>${inspector(p, proc)}</div>`;
  }
  function canvas(p, proc) {
    const positions = [
      [30, 51],
      [262, 51],
      [494, 51],
      [494, 224],
      [262, 224],
      [30, 224],
    ];
    return `<div class="canvas-viewport"><div class="canvas" style="zoom:${state.zoom / 100};height:${Math.max(389, 397 + 173 * Math.ceil(Math.max(0, proc.steps.length - 6) / 3))}px"><span class="lane-label" style="left:30px;top:21px">01 / UNDERSTAND & EXECUTE</span><span class="lane-label" style="left:30px;top:196px">02 / VERIFY & HAND OVER</span><svg class="graph-connections" viewBox="0 0 720 389" preserveAspectRatio="none" aria-hidden="true"><defs><marker id="arrowhead" markerWidth="5" markerHeight="5" refX="4" refY="2.5" orient="auto"><path d="M0 0 5 2.5 0 5" fill="#91aa9a" stroke="none"/></marker></defs>${proc.steps.length > 1 ? '<path d="M226 112H262" marker-end="url(#arrowhead)"/>' : ""}${proc.steps.length > 2 ? '<path d="M458 112H494" marker-end="url(#arrowhead)"/>' : ""}${proc.steps.length > 3 ? '<path d="M690 112h15v173h-15" marker-end="url(#arrowhead)"/>' : ""}${proc.steps.length > 4 ? '<path d="M494 285H458" marker-end="url(#arrowhead)"/>' : ""}${proc.steps.length > 5 ? '<path class="future" d="M262 285H226" marker-end="url(#arrowhead)"/>' : ""}</svg>${proc.steps
      .map((s, i) => {
        const status = nodeStatus(i, p),
          pos = positions[i] || [
            30 + 232 * (i % 3),
            397 + 173 * Math.floor((i - 6) / 3),
          ];
        return `<button class="node ${state.node === i ? "selected" : ""} ${s.kind === "External agent" ? "external" : ""} ${status.includes("review") || status.includes("Waiting") ? "waiting" : ""}" style="left:${pos[0]}px;top:${pos[1]}px" data-action="node" data-index="${i}" aria-pressed="${state.node === i}"><div class="between"><span class="node-icon">${icon(s.kind === "Human gate" ? "person" : s.kind.includes("agent") ? "agent" : "file")}</span><span class="mono muted" style="font-size:8px">${String(i + 1).padStart(2, "0")} / ${s.kind === "Human gate" ? "GATE" : s.kind === "Action" ? "ACTION" : s.kind === "External agent" ? "EXTERNAL" : "AGENT"}</span></div><span class="node-title">${esc(s.name)}</span><span class="node-sub">${esc(s.agent ? nameOf("agents", s.agent) : s.kind === "Human gate" ? "George · person" : s.tool)}</span><span class="node-status"><span class="row" style="gap:4px"><i class="dot ${status === "Completed" ? "green" : status === "Needs review" ? "amber" : ""}"></i>${status}</span><span>${status === "Completed" ? "✓" : status === "Running" ? "↻" : "—"}</span></span></button>`;
      })
      .join(
        "",
      )}<div style="position:absolute;bottom:9px;left:30px" class="row"><button class="btn small" data-action="zoom" data-value="-10" aria-label="Zoom out">−</button><span class="mono muted" style="font-size:9px">${state.zoom}%</span><button class="btn small" data-action="zoom" data-value="10" aria-label="Zoom in">+</button></div></div></div>`;
  }
  function messages(p, proc) {
    let content;
    if (p.id === "portal")
      content = `<div class="message"><span class="agent-avatar">IA</span><div class="grow"><div class="who">Implementation agent ${badge("External", "purple")}<small>10:42:18</small></div><p>The invitation flow is implemented. Tokens expire after 72 hours, are single-use and are bound to the workspace’s tenant. The patch is ready for independent verification.</p><details class="tool-call"><summary>${icon("code")}exec_in_workspace_async <span class="grow"></span><span class="green">completed</span></summary><pre>${esc(JSON.stringify({ input: { container_name: "portal-c142", command: ["cargo", "test", "invitations"] }, output: { job_id: "exec-817", status: "completed", exit_code: 0 }, verified_by: ["get_exec_job_status", "get_workspace_git_status", "list_workspace_files"] }, null, 2))}</pre></details></div></div><div class="message"><span class="agent-avatar internal">VA</span><div class="grow"><div class="who">Verification agent ${badge("Internal", "green")}<small>10:46:05</small></div><p>All 24 acceptance checks passed. Cross-tenant and expired-token cases are included. Attached the report and requested an implementation review.</p><details class="tool-call"><summary>${icon("check")}get_test_suite_results <span class="grow"></span><span class="green">24 / 24 passed</span></summary><pre>${esc(JSON.stringify({ suite_id: "invitations-v3", passed: 24, failed: 0, requirement: "REQ-042@3", artifact: "invitation-verification.json" }, null, 2))}</pre></details></div></div>`;
    else
      content = `<div class="message"><span class="agent-avatar internal">${initials(p.agents[0])}</span><div class="grow"><div class="who">${esc(nameOf("agents", p.agents[0]))}${badge("Sample execution", "green")}<small>10:48:12</small></div><p>${esc(p.description)} The ${esc(proc.name.toLowerCase())} process is executing against its published version. Select a step to inspect its actor, inputs and permitted tools.</p><details class="tool-call"><summary>${icon("file")}${esc(proc.steps[0]?.tool || "requirements.list")}<span class="grow"></span>recorded</summary><pre>${esc(JSON.stringify({ project: p.key, project_type: p.type, process: proc.id, version: proc.version, status: p.status }, null, 2))}</pre></details></div></div>`;
    const decision = db.decisions.find(
      (d) => d.project === p.id && d.state !== "Pending",
    );
    if (decision)
      content += `<div class="message"><span class="agent-avatar person">GD</span><div class="grow"><div class="who">George ${badge("Person · Web", "orange")}<small>just now</small></div><p>${esc(decision.state)}${decision.note ? ": " + esc(decision.note) : ". Decision recorded; the next step is queued."}</p></div></div>`;
    return (
      content +
      state.chat
        .filter((m) => m.project === p.id)
        .map(
          (m) =>
            `<div class="message"><span class="agent-avatar ${m.person ? "person" : "internal"}">${m.person ? "GD" : "CO"}</span><div class="grow"><div class="who">${m.person ? "George" : "Copilot"}${badge(m.person ? "Person" : "Scripted demo", m.person ? "orange" : "green")}<small>just now</small></div><p>${esc(m.text)}</p></div></div>`,
        )
        .join("")
    );
  }
  function composer() {
    return `<form class="composer" id="chat-form"><label class="eyebrow" for="chat-input" style="margin-bottom:5px;font-size:8px">ASK YOUR COPILOT</label><textarea id="chat-input" name="message" placeholder="Ask about this work, its context or the next step…" aria-label="Message to copilot" required></textarea><div class="composer-bottom"><span>Context: ${esc(nameOf("projects", state.project))} · scripted local demo</span><button class="send-button" type="submit" aria-label="Send message">${icon("arrow")}</button></div></form>`;
  }
  function inspector(p, proc) {
    const s = proc.steps[state.node],
      a = p.agentPins?.[s.agent] || byId("agents", s.agent),
      status = nodeStatus(state.node, p),
      review = db.decisions.find(
        (d) => d.project === p.id && d.id === "review",
      );
    return `<aside class="inspector"><div class="inspector-head"><span class="eyebrow">STEP ${String(state.node + 1).padStart(2, "0")} / INSPECTOR</span>${badge(s.kind, s.kind === "External agent" ? "purple" : s.kind === "Human gate" ? "amber" : "green")}<h2>${esc(s.name)}</h2><p>${esc(a?.role || (s.kind === "Human gate" ? "A person reviews the evidence before the work continues." : "A registered action in the published process."))}</p></div><div class="inspector-tabs" aria-label="Step inspection">${["Evidence", "Context", "Tools"].map((t) => `<button class="${state.inspectorTab === t ? "selected" : ""}" data-action="inspector-tab" data-value="${t}" aria-pressed="${state.inspectorTab === t}">${t}</button>`).join("")}</div><div class="inspector-body"><div class="stack">${state.inspectorTab === "Evidence" ? `<dl class="kv"><dt>Status</dt><dd>${badge(status, status === "Completed" ? "green" : status.includes("review") ? "amber" : "")}</dd><dt>Process</dt><dd>${esc(proc.name)} @${proc.version}</dd><dt>Executor</dt><dd>${esc(a?.name || (s.kind === "Human gate" ? "George · person" : "Procedure"))}</dd></dl>${evidenceMarkup(p, s, status)}` : state.inspectorTab === "Context" ? `<div><h4>Compiled from pinned sources</h4><dl class="kv"><dt>Procedure</dt><dd>${esc(proc.id)}@${proc.version}</dd><dt>Agent role</dt><dd>${esc(a?.prompt || "procedure_context@2")}</dd><dt>Knowledge</dt><dd>${esc(byId("capabilities", proc.capability)?.knowledge)}</dd><dt>Project & card</dt><dd>${esc(p.key)} / ${p.id === "portal" ? "C-142" : "scope"}</dd></dl></div><button class="binding-card" data-action="open-definition" data-kind="requirements" data-id="${db.requirements.find((r) => r.project === p.id)?.id || "invite"}">${icon("file")}<span><strong>Governing requirement</strong><small>Open its revision, criteria and traces</small></span><span class="arrow">↗</span></button><div class="info-note">Required sources must resolve before an internal agent starts. The inference ledger retains the recipe identity and hash, without prompt bodies.</div>` : `<div><h4>${a ? "Agent tool allowlist" : "Registered step action"}</h4>${(a?.tools || [s.tool]).map((t) => `<button class="file-row" data-action="open-tool" data-id="${esc(t)}">${icon("tools")}<span class="mono grow" style="font-size:9px;overflow-wrap:anywhere">${esc(t)}</span>↗</button>`).join("")}</div><div class="info-note">${s.kind === "External agent" ? "This is the tenant’s declared external-agent binding. Conveyor observes MCP tasks and returned evidence; private external execution is not a Conveyor model call." : "Tools are granted to this agent by its tenant definition. A process step selects an actor and its permitted action."}</div>`}</div></div><div class="inspector-footer">${s.kind === "Human gate" && review?.state === "Pending" ? `${btn("Review implementation", "review", 'data-id="review"', "primary", "shield")}<small>George · person · decision via Web</small>` : s.agent ? `${btn("Inspect agent definition", "open-definition", `data-kind="agents" data-id="${s.agent}"`, "small", "agent")}<small>Definition and run-time state are separate.</small>` : `${btn("Inspect process definition", "open-definition", `data-kind="processes" data-id="${proc.id}"`, "small", "flow")}<small>Published v${proc.version} is pinned to this run.</small>`}</div></aside>`;
  }
  function board(p) {
    const lists = ["Ready", "Build", "Review", "Done"];
    return `<div class="toolbar"><span class="mono muted">WORK PACKAGES / ${esc(p.key)}</span>${btn("New work item", "add-card", "", "primary small", "plus")}</div><div class="board">${lists
      .map(
        (l) =>
          `<section class="board-column" data-list="${l}"><h3>${l.toUpperCase()} <span>${db.cards.filter((c) => c.project === p.id && c.list === l).length}</span></h3>${db.cards
            .filter((c) => c.project === p.id && c.list === l)
            .map(
              (c) =>
                `<button class="work-card" draggable="true" data-card="${c.id}" data-action="card" data-id="${c.id}"><small>${esc(c.id)}</small><h4>${esc(c.name)}</h4><div class="between">${badge("Work package", l === "Review" ? "amber" : "")}<span class="mono muted" style="font-size:8px">${esc(byId("requirements", c.requirement)?.key || "Unlinked")}</span></div></button>`,
            )
            .join("")}</section>`,
      )
      .join(
        "",
      )}</div><p class="muted" style="font-size:11px;margin-top:15px">Drag a card to move it, or open it and choose a list. A card move does not answer a pending approval.</p>`;
  }
  function projectRequirements(p) {
    const req = db.requirements.filter((r) => r.project === p.id);
    return `<div class="panel"><div class="panel-head"><h2>Requirements & principles</h2>${btn("Create requirement", "create-entity", 'data-kind="requirements"', "small", "plus")}</div><div class="panel-body stack">${req.map((r) => `<button class="binding-card" data-action="open-definition" data-kind="requirements" data-id="${r.id}">${icon("file")}<span class="grow"><strong>${esc(r.name)}</strong><small>${esc(r.key)} · v${r.version} · ${esc(r.status)}</small></span>${badge(r.status, r.status === "Approved" ? "green" : "")}<span>↗</span></button>`).join("") || '<p class="muted">No requirements linked yet.</p>'}<div class="info-note">Tenant principles apply across projects. Each requirement retains its scope, revisions, acceptance criteria and model traces.</div>${db.principles.map((r) => `<button class="file-row" data-action="open-definition" data-kind="principles" data-id="${r.id}">${icon("shield")}<span class="grow">${esc(r.name)}</span><span class="mono muted">${r.key}</span></button>`).join("")}</div></div>`;
  }
  function projectModels(p) {
    return `<div class="toolbar"><span class="mono muted">PROJECT MODELS & VIEWS</span>${btn("New model", "create-model", `data-project="${p.id}"`, "primary small", "plus")}</div><div class="split">${
      db.models
        .filter((m) => m.project === p.id)
        .map(
          (m) =>
            `<button class="panel" style="text-align:left;padding:23px" data-action="open-model" data-id="${m.id}"><div class="between">${badge(m.language, m.language === "UML" ? "purple" : "green")}${icon("model")}</div><h2 style="margin:16px 0 8px">${esc(m.name)}</h2><p class="muted" style="font-size:11px">${m.elements.length} elements · ${m.relationships.length} relationships · ${m.views.length} views</p><div class="row" style="margin-top:20px">${m.views.map((v) => badge(v.name)).join("")}</div></button>`,
        )
        .join("") ||
      '<div class="empty">No models yet. Create an ArchiMate or UML model for this project.</div>'
    }</div>`;
  }
  function artifacts(p) {
    return `<div class="panel"><div class="panel-head"><h2>Evidence & outputs</h2><span class="mono muted">${esc(p.key)}</span></div><div class="panel-body"><button class="file-row" data-action="artifact" data-kind="patch">${icon("code")}<span class="grow">implementation.patch<small>Implementation agent · produced by Build a feature v7</small></span>${badge("Patch", "purple")}↗</button><button class="file-row" data-action="artifact" data-kind="report">${icon("file")}<span class="grow">verification-report.json<small>Verification agent · linked to REQ-042@3</small></span>${badge("Test report", "green")}↗</button><button class="file-row" data-action="open-model" data-id="${db.models.find((m) => m.project === p.id)?.id || "portal-arch"}">${icon("model")}<span class="grow">Architecture model & views<small>Shared model elements with governed-asset trace links</small></span>${badge("Model")}↗</button></div></div>`;
  }
  const catalogueInfo = {
    types: {
      title: "Project types",
      subtitle:
        "Define the lifecycle, allowed work and capability coverage of a project.",
      singular: "project type",
      symbol: "layers",
    },
    capabilities: {
      title: "Capabilities",
      subtitle:
        "Define what your tenant can do, and the processes that carry it out.",
      singular: "capability",
      symbol: "grid",
    },
    processes: {
      title: "Processes",
      subtitle:
        "Design the steps, actors, tool actions and human gates for each capability.",
      singular: "process",
      symbol: "flow",
    },
    agents: {
      title: "Agents",
      subtitle:
        "Define each role, its executor kind, context, tools and operating limits.",
      singular: "agent",
      symbol: "agent",
    },
  };
  function entityList(kind, items, selected) {
    return `<section class="entity-list"><div class="entity-list-head"><label class="input-search">${icon("search")}<input id="entity-search" placeholder="Filter ${esc(kind)}…" value="${esc(state.entityQuery)}" aria-label="Filter ${kind}"></label></div><div id="entity-items">${items.map((x) => `<button class="entity ${selected === x.id ? "active" : ""}" data-action="select-entity" data-kind="${kind}" data-id="${x.id}"><span class="entity-title">${esc(x.name)}</span><div class="between">${badge(x.status, x.status === "Published" || x.status === "Approved" ? "green" : "")}<span class="mono muted" style="font-size:9px">v${x.version}</span></div><small>${kind === "types" ? db.projects.filter((p) => p.type === x.id).length + " active projects" : kind === "capabilities" ? x.processes.length + " processes" : kind === "processes" ? nameOf("capabilities", x.capability) : kind === "agents" ? x.kind : kind === "requirements" ? nameOf("projects", x.project) : "Tenant principle"}</small></button>`).join("") || '<div class="empty">No matching definitions.</div>'}</div></section>`;
  }
  function cataloguePage(kind) {
    const info = catalogueInfo[kind],
      items = db[kind].filter((x) =>
        x.name.toLowerCase().includes(state.entityQuery.toLowerCase()),
      );
    let item = byId(kind, state.selected[kind]);
    if (!item) {
      item = db[kind][0];
      state.selected[kind] = item?.id;
    }
    return `<div class="page">${header(info.title, info.subtitle, btn("New " + info.singular, "create-entity", `data-kind="${kind}"`, "primary", "plus"), "TENANT STUDIO / " + info.title.toUpperCase())}${chain()}<div class="catalog-layout">${entityList(kind, items, item?.id)}${item ? `<section class="panel"><div class="detail-header"><div class="between"><div class="row">${badge(item.status, item.status === "Published" ? "green" : "")}${badge("Tenant definition")}<span class="mono muted">v${item.version}</span></div>${btn("Edit definition", "edit-entity", `data-kind="${kind}" data-id="${item.id}"`, "small", "edit")}</div><h2>${esc(item.name)}</h2><p>${esc(item.description || item.role)}</p></div><div class="detail-content"><div class="tabs">${["Definition", "Used by", "Versions"].map((t) => `<button class="${state.catalogTab === t ? "selected" : ""}" data-action="catalog-tab" data-value="${t}">${t}</button>`).join("")}</div>${state.catalogTab === "Used by" ? usedBy(kind, item) : state.catalogTab === "Versions" ? versions(kind, item) : kind === "types" ? typeDefinition(item) : kind === "capabilities" ? capabilityDefinition(item) : kind === "processes" ? processDefinition(item) : agentDefinition(item)}</div><div class="panel-head" style="border-bottom:0;border-top:1px solid var(--line)"><span class="mono muted" style="font-size:8px">CHANGES HERE ARE LOCAL DESIGN FIXTURES</span><div class="row">${item.status === "Draft" ? btn("Publish preview", "publish", `data-kind="${kind}" data-id="${item.id}"`, "small", "check") : ""}${btn(item.status === "Draft" ? "Delete draft" : "Retire definition", "delete-entity", `data-kind="${kind}" data-id="${item.id}"`, "quiet small danger")}</div></div></section>` : '<div class="panel empty">Create the first definition.</div>'}</div></div>`;
  }
  function binding(kind, id, note = "") {
    const x = byId(kind, id);
    return x
      ? `<button class="binding-card" data-action="open-definition" data-kind="${kind}" data-id="${id}">${icon(catalogueInfo[kind]?.symbol || "file")}<span class="grow"><strong>${esc(x.name)}</strong><small>${esc(note || x.description || x.role || x.status)}</small></span><span class="arrow">↗</span></button>`
      : "";
  }
  function typeDefinition(t) {
    return `<div class="stack"><div class="split"><section><div class="subheading"><h3>Ordered lifecycle</h3>${badge(t.phases.length + " phases")}</div><div class="phase-list">${t.phases.map((p, i) => `<div class="phase-item"><span class="number">${String(i + 1).padStart(2, "0")}</span><div><h4>${esc(p)}</h4><p>${esc(t.gates[Math.min(i, t.gates.length - 1)] || "Evidence required before progression")}</p></div></div>`).join("")}</div></section><section><div class="subheading"><h3>Capabilities available</h3>${btn("Change", "edit-entity", `data-kind="types" data-id="${t.id}"`, "quiet small")}</div>${t.capabilities.map((id) => binding("capabilities", id)).join("")}<h3 style="font-size:12px;margin:20px 0 10px">Allowed agent job types (${t.cardTypes.length})</h3><div class="row" style="flex-wrap:wrap">${t.cardTypes
      .slice(0, 8)
      .map((x) => badge(x))
      .join(
        "",
      )}</div><div style="margin-top:12px">${btn("Browse agent job library", "navigate", 'data-page="jobtypes"', "small", "board")}</div></section></div><section><div class="subheading"><h3>Phase × capability matrix</h3>${badge("Draft mapping")}</div><div class="matrix-wrap"><table class="matrix"><thead><tr><th>Phase</th>${t.capabilities.map((id) => `<th>${esc(nameOf("capabilities", id))}</th>`).join("")}</tr></thead><tbody>${t.phases
      .map(
        (p, i) =>
          `<tr><td>${esc(p)}</td>${t.capabilities
            .map((id, j) => {
              const key = p + "|" + id;
              const checked =
                t.matrix?.[key] ??
                j ===
                  Math.min(
                    Math.floor((i * t.capabilities.length) / t.phases.length),
                    t.capabilities.length - 1,
                  );
              return `<td><input type="checkbox" data-matrix="${esc(key)}" data-type="${t.id}" ${checked ? "checked" : ""} aria-label="${esc(p)} uses ${esc(nameOf("capabilities", id))}"></td>`;
            })
            .join("")}</tr>`,
      )
      .join(
        "",
      )}</tbody></table></div><p class="muted" style="font-size:10px;margin-top:10px">Enable the capabilities available in each phase. This matrix is a local draft; existing project runs keep their pinned definition.</p></section></div>`;
  }
  function contextDesignLink(kind, id) {
    return `<div class="info-note"><strong>Knowledge, memory & improvement</strong><p style="margin:7px 0">Inspect inherited sources, process selections, agent context and scoped memory.</p><button class="btn small" data-l="design-entry" data-kind="${kind}" data-id="${esc(id)}">Design knowledge & memory →</button></div>`;
  }
  function capabilityDefinition(c) {
    return `<div class="stack">${contextDesignLink("capabilities", c.id)}<div class="split"><section><div class="subheading"><h3>Processes for this capability</h3>${btn("Add process", "create-entity", 'data-kind="processes"', "quiet small", "plus")}</div>${c.processes.map((id) => binding("processes", id, `${byId("processes", id)?.steps.length || 0} steps · ${byId("processes", id)?.status}`)).join("") || '<p class="muted">No process is bound yet.</p>'}</section><section><h3 style="font-size:12px;margin-bottom:15px">Knowledge & readiness</h3><dl class="kv"><dt>Required domain pack</dt><dd class="mono">${esc(c.knowledge)}</dd><dt>Pin policy</dt><dd>Exact version</dd><dt>Missing required source</dt><dd>${badge("Blocks readiness", "amber")}</dd><dt>Work-package memory</dt><dd>Project & card scoped</dd></dl><div class="info-note" style="margin-top:20px">A capability groups reusable processes. Each process selects steps; each agent step uses an explicit role and tool grant.</div></section></div><div><h3 style="font-size:12px;margin-bottom:13px">Used by project types</h3>${
      db.types
        .filter((t) => t.capabilities.includes(c.id))
        .map((t) => binding("types", t.id))
        .join("") || '<p class="muted">Not linked to a project type yet.</p>'
    }</div></div>`;
  }
  function processDefinition(p) {
    return `<div class="stack">${contextDesignLink("processes", p.id)}${binding("capabilities", p.capability, "Parent capability · open definition")}<section><div class="subheading"><h3>Process steps</h3>${btn("Add step", "add-step", `data-id="${p.id}"`, "small", "plus")}</div><div class="phase-list">${p.steps.map((s, i) => `<div class="phase-item"><span class="number">${String(i + 1).padStart(2, "0")}</span><div class="grow"><div class="row" style="flex-wrap:wrap"><h4>${esc(s.name)}</h4>${badge(s.kind, s.kind === "External agent" ? "purple" : s.kind === "Human gate" ? "amber" : "")}</div><p>${esc(s.agent ? nameOf("agents", s.agent) : s.kind === "Human gate" ? "Person approval required" : "Deterministic action")} · <button class="text-button mono" style="font-size:9px" data-action="open-tool" data-id="${esc(s.tool)}">${esc(s.tool)} ↗</button></p></div><div class="phase-actions">${btn("↑", "step-up", `data-id="${p.id}" data-index="${i}" aria-label="Move ${esc(s.name)} up"`, "quiet small")}${btn("Edit", "edit-step", `data-id="${p.id}" data-index="${i}"`, "quiet small")}</div></div>`).join("")}</div></section><div class="info-note">A process is presented here as a versioned procedure. Deterministic actions, bounded internal agents and human gates are distinct step kinds; a run has one of two executors — procedure or internal_agent (D46). Editing creates a draft; active runs retain their published version.</div></div>`;
  }
  function agentDefinition(a) {
    return `<div class="stack">${contextDesignLink("agents", a.id)}<div class="split"><section><h3 style="font-size:12px;margin-bottom:16px">Executor & context</h3><dl class="kv"><dt>Executor kind</dt><dd>${badge(a.kind, a.kind === "External agent" ? "purple" : "green")}</dd><dt>Model route</dt><dd>${esc(a.model)}</dd><dt>Role recipe</dt><dd class="mono">${esc(a.prompt)}</dd><dt>Maximum turns</dt><dd>${a.maxTurns}</dd><dt>Internal call budget</dt><dd>${a.kind === "External agent" ? "External platform" : "$" + a.budget.toFixed(2)}</dd><dt>Scope</dt><dd>Tenant + project + work package</dd></dl><div class="info-note" style="margin-top:20px">${a.kind === "External agent" ? "The external agent operates through Conveyor MCP tasks. Its private loop and inference spending remain on the external platform. These are tenant-declared tool bindings." : "The internal loop is capped and uses only the permitted tools below. Missing context or an unresolved required pack blocks readiness."}</div></section><section><div class="subheading"><h3>Granted tools</h3>${badge(a.tools.length + " actions")}</div>${a.tools.map((t) => `<button class="file-row" data-action="open-tool" data-id="${esc(t)}">${icon("tools")}<span class="grow mono" style="font-size:9px;overflow-wrap:anywhere">${esc(t)}</span>${badge(toolOf(t)?.role || "Unknown", toolOf(t)?.role === "observer" ? "green" : "")}↗</button>`).join("")}</section></div><section><div class="subheading"><h3>Acts in these processes</h3></div>${
      db.processes
        .filter((p) => p.steps.some((s) => s.agent === a.id))
        .map((p) => binding("processes", p.id))
        .join("") || '<p class="muted">Not assigned to a process yet.</p>'
    }</section></div>`;
  }
  function usedBy(kind, item) {
    let rows = [];
    if (kind === "types")
      rows = db.projects
        .filter((p) => p.type === item.id)
        .map((p) => projectCard(p));
    if (kind === "capabilities")
      rows = db.types
        .filter((t) => t.capabilities.includes(item.id))
        .map((t) => binding("types", t.id));
    if (kind === "processes")
      rows = db.projects
        .filter((p) => p.process === item.id)
        .map((p) => projectCard(p));
    if (kind === "agents")
      rows = db.processes
        .filter((p) => p.steps.some((s) => s.agent === item.id))
        .map((p) => binding("processes", p.id));
    return `<div class="stack">${rows.join("") || '<div class="empty">No references yet.</div>'}</div>`;
  }
  function versions(kind, item) {
    return `<div class="stack"><div class="info-note">The current ${esc(catalogueInfo[kind]?.singular || "asset")} definition is v${item.version}. Published run bindings stay pinned when a new draft is edited.</div>${(item.history || [`v${item.version} · ${item.status} · tenant definition`, ...(item.version > 1 ? [`v${item.version - 1} · Previous published version`] : [])]).map((h) => `<div class="revision">${esc(h)}<small>Definition version retained for provenance</small></div>`).join("")}</div>`;
  }
  function toolPage() {
    const tools = allTools(),
      groups = [
        "All actions",
        "Conveyor",
        ...new Set(coding.operations.map((t) => t.group)),
        "Tenant tools",
      ];
    const filtered = tools.filter(
      (t) =>
        (state.toolGroup === "All actions" ||
          (state.toolGroup === "Tenant tools"
            ? t.source === "tenant"
            : t.group === state.toolGroup)) &&
        (t.id + " " + t.title + " " + t.group + " " + (t.n8nOperation || ""))
          .toLowerCase()
          .includes(state.toolQuery.toLowerCase()),
    );
    const selected = tools.find((t) => t.id === state.tool) || filtered[0];
    return `<div class="page">${header("Tools & actions", "The real coding action surface, alongside Conveyor actions and tenant tool bindings.", btn("Register tenant tool", "create-tool", "", "primary", "plus"), "TENANT STUDIO / ACTION LIBRARY")}<div class="toolbar"><div class="row" style="flex-wrap:wrap">${badge(coding.count + " coding API operations", "purple")}${badge(coding.nodeOptionCount + " node options")}${badge(coreTools.length + " Conveyor examples")}${badge(db.customTools.length + " tenant tools")}</div><label class="input-search">${icon("search")}<input id="tool-search" value="${esc(state.toolQuery)}" placeholder="Find an action, group or operation…" aria-label="Search tools"></label></div><div class="tool-layout"><nav class="tool-categories" aria-label="Action groups">${groups.map((g) => `<button class="tool-category ${state.toolGroup === g ? "selected" : ""}" data-action="tool-group" data-value="${esc(g)}">${esc(g)}<span>${g === "All actions" ? tools.length : g === "Tenant tools" ? db.customTools.length : tools.filter((t) => t.group === g).length}</span></button>`).join("")}</nav><section class="panel"><div class="panel-head"><h2>Available actions</h2><span class="mono muted">${filtered.length}</span></div><div class="tools-results">${filtered.map((t) => `<button class="tool-item ${t.id === selected?.id ? "active" : ""}" data-action="select-tool" data-id="${esc(t.id)}"><strong>${esc(t.id)}</strong><p>${esc(t.title)}</p><div class="row">${badge(t.method, t.method === "GET" ? "green" : "")}${badge(t.role, t.role === "observer" ? "green" : t.role === "producer" ? "purple" : "")}<small>${esc(t.group)}</small></div></button>`).join("") || '<div class="empty">No matching actions.</div>'}</div></section>${selected ? toolDetail(selected) : ""}</div><p class="muted mono" style="font-size:8px;margin-top:16px;overflow-wrap:anywhere">CODING SOURCE SNAPSHOT · ${coding.count} operations · SHA-256 ${coding.sha256.slice(0, 20)}… · no service connection</p></div>`;
  }
  function toolDetail(t) {
    return `<aside class="panel tool-detail"><div class="panel-head"><h2>Action contract</h2>${icon("tools")}</div><div class="panel-body"><div class="stack"><div><div class="row" style="margin-bottom:12px">${badge(t.group, t.source === "conveyor" ? "green" : "purple")}${badge(t.role, t.role === "observer" ? "green" : "")}</div><h3>${esc(t.id)}</h3><div class="endpoint">${esc(t.method)} ${esc(t.path)}</div><p>${esc(t.title)}</p>${t.n8nOperation ? `<div class="endpoint"><span class="muted">n8n node option</span><br>${esc(t.n8nResource)} / ${esc(t.n8nOperation)}</div>` : t.source ? "" : '<p class="muted" style="font-size:9px;margin-top:9px">API operation · no matching node option in the local resource snapshot.</p>'}</div><div><h4 style="margin-bottom:6px">Inputs from source schema</h4>${t.fields.length ? t.fields.map((f) => `<div class="schema-row"><span style="overflow-wrap:anywhere">${esc(f.name)}</span><span class="muted">${esc(f.type)}</span>${f.required ? '<span class="required">required</span>' : ""}</div>`).join("") : '<p style="font-size:10px">No top-level fields in this display snapshot. Consult the registered schema before execution.</p>'}</div><div><h4 style="margin-bottom:8px">Verification observers</h4>${t.verifies.length ? t.verifies.map((id) => `<button class="file-row" data-action="open-tool" data-id="${esc(id)}"><span class="mono grow" style="font-size:9px">${esc(id)}</span>↗</button>`).join("") : '<p style="font-size:10px">No verification observers declared in the snapshot.</p>'}</div><div><h4 style="margin-bottom:8px">Granted to</h4>${
      db.agents
        .filter((a) => a.tools.includes(t.id))
        .map(
          (a) =>
            `<button class="file-row" data-action="open-definition" data-kind="agents" data-id="${a.id}">${icon("agent")}<span class="grow">${esc(a.name)}</span>↗</button>`,
        )
        .join("") ||
      '<p style="font-size:10px">Not granted to a tenant agent yet.</p>'
    }${btn("Manage agent grant", "grant-tool", `data-id="${esc(t.id)}"`, "primary small", "plus")}</div>${t.source === "tenant" ? `<div class="row">${btn("Edit binding", "edit-tool", `data-id="${esc(t.id)}"`, "small", "edit")}${btn("Delete binding", "delete-tool", `data-id="${esc(t.id)}"`, "quiet small danger")}</div>` : ""}<div class="info-note">A producer’s successful response is followed by its declared observers. Unknown metadata remains unclassified. Registering a tool does not grant it to every agent.</div></div></div></aside>`;
  }
  function governancePage(kind) {
    const isReq = kind === "requirements",
      items = db[kind].filter((x) =>
        x.name.toLowerCase().includes(state.entityQuery.toLowerCase()),
      );
    let item = byId(kind, state.selected[kind]) || db[kind][0];
    state.selected[kind] = item?.id;
    return `<div class="page">${header(isReq ? "Requirements" : "Principles", isReq ? "Create, refine and govern the intent behind every work package." : "The tenant’s shared rules, with a lifecycle, rationale and revision history.", btn(isReq ? "New requirement" : "New principle", "create-entity", `data-kind="${kind}"`, "primary", "plus"), "GOVERNANCE / " + (isReq ? "REQUIREMENTS" : "PRINCIPLES"))}<div class="catalog-layout">${entityList(kind, items, item?.id)}${
      item
        ? `<section class="panel"><div class="detail-header"><div class="between"><div class="row">${badge(item.status, item.status === "Approved" ? "green" : "")}<span class="mono muted">${esc(item.key)} @${item.version}</span></div>${btn("Edit revision", "edit-entity", `data-kind="${kind}" data-id="${item.id}"`, "small", "edit")}</div><h2>${esc(item.name)}</h2><p>${item.project ? "Project: " + esc(nameOf("projects", item.project)) : "Tenant-wide · applies across all projects"}</p></div><div class="detail-content stack"><div class="governance-body"><h3>${isReq ? "Requirement" : "Statement"}</h3>${esc(item.description)}</div><div><h3 style="font-size:12px;margin-bottom:10px">${isReq ? "Acceptance criteria" : "Rationale & implications"}</h3><ul class="acceptance-list">${item.acceptance
            .split("\n")
            .filter(Boolean)
            .map((x) => `<li>${esc(x)}</li>`)
            .join(
              "",
            )}</ul></div><div><div class="subheading"><h3>Traceability</h3>${badge("Live local references")}</div><div class="trace-chain">${isReq && item.principle ? `<button data-action="open-definition" data-kind="principles" data-id="${item.principle}">${esc(byId("principles", item.principle)?.key)} · governing principle ↗</button><span>→</span>` : ""}<span class="badge">${esc(item.key)}</span>${(isReq ? db.cards.filter((c) => c.requirement === item.id) : db.requirements.filter((r) => r.principle === item.id)).map((x) => `<span>→</span><button data-action="${isReq ? "card" : "open-definition"}" ${isReq ? "" : `data-kind="requirements"`} data-id="${x.id}">${esc(x.key || x.id)} · ${esc(x.name)} ↗</button>`).join("")}${db.models
            .filter((m) => m.elements.some((e) => e.asset === item.id))
            .map(
              (m) =>
                `<span>→</span><button data-action="open-model" data-id="${m.id}">${esc(m.name)} ↗</button>`,
            )
            .join(
              "",
            )}</div></div><section><div class="subheading"><h3>Revision history</h3>${badge("v" + item.version)}</div>${item.history.map((h) => `<div class="revision">${esc(h)}</div>`).join("")}</section></div><div class="panel-head" style="border-top:1px solid var(--line);border-bottom:0"><span class="muted" style="font-size:10px">An edit creates a new draft revision.</span><div class="row">${item.status === "Draft" ? btn("Approve revision", "approve-asset", `data-kind="${kind}" data-id="${item.id}"`, "primary small", "check") : ""}${btn(item.status === "Draft" ? "Delete draft" : "Retire", "delete-entity", `data-kind="${kind}" data-id="${item.id}"`, "small quiet danger")}</div></div></section>`
        : '<div class="panel empty">Create the first governed asset.</div>'
    }</div></div>`;
  }
  const modelKinds = {
    ArchiMate: [
      "business_actor",
      "business_role",
      "business_process",
      "business_function",
      "business_service",
      "business_object",
      "application_component",
      "application_service",
      "application_interface",
      "data_object",
      "node",
      "system_software",
      "technology_service",
      "artifact",
      "resource",
      "capability",
      "course_of_action",
      "value_stream",
      "stakeholder",
      "driver",
      "assessment",
      "goal",
      "outcome",
      "requirement",
      "principle",
      "work_package",
      "deliverable",
      "gap",
      "grouping",
      "location",
    ],
    UML: [
      "class",
      "interface",
      "enumeration",
      "datatype",
      "package",
      "component",
      "object",
      "actor",
      "use_case",
      "lifeline",
    ],
  };
  const relationshipKinds = {
    ArchiMate: [
      "composition",
      "aggregation",
      "assignment",
      "realization",
      "serving",
      "access",
      "influence",
      "triggering",
      "flow",
      "specialization",
      "association",
    ],
    UML: [
      "association",
      "aggregation",
      "composition",
      "generalization",
      "realization",
      "dependency",
      "include",
      "extend",
      "message",
    ],
  };
  function modelSvg(m, view) {
    const elements = m.elements.filter(
        (e) => !view || view.elements.includes(e.id),
      ),
      rows = Math.max(1, Math.ceil(elements.length / 2)),
      height = Math.max(340, rows * 125 + 40);
    const coords = new Map(
      elements.map((e, i) => [
        e.id,
        { x: i % 2 === 0 ? 38 : 350, y: 30 + Math.floor(i / 2) * 125 },
      ]),
    );
    const color = (k) =>
      ["principle", "requirement", "goal", "outcome"].includes(k)
        ? "motivation"
        : k.startsWith("business")
          ? "business"
          : k.includes("application") || k === "data_object"
            ? "application"
            : "technology";
    return `<svg class="model-svg" id="model-svg" viewBox="0 0 600 ${height}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="${esc(view?.name || m.name)}"><title>${esc(m.language + " model: " + m.name)}</title><defs><marker id="model-arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="m0 0 7 3.5-7 3.5" fill="none" stroke="#758b7d"/></marker></defs>${m.relationships
      .filter((r) => coords.has(r.source) && coords.has(r.target))
      .map((r) => {
        const a = coords.get(r.source),
          b = coords.get(r.target),
          same = a.y === b.y;
        const start = same
            ? `${a.x + 210},${a.y + 42}`
            : `${a.x + 105},${a.y + 84}`,
          end = same ? `${b.x},${b.y + 42}` : `${b.x + 105},${b.y}`;
        return `<g><path class="edge" d="M${start} L${end}" marker-end="url(#model-arrow)" ${["dependency", "realization", "influence"].includes(r.kind) ? 'stroke-dasharray="4 3"' : ""}/><text x="${same ? (a.x + b.x + 210) / 2 + 4 : (a.x + b.x) / 2 + 111}" y="${same ? a.y + 31 : (a.y + b.y + 84) / 2}" text-anchor="${same ? "middle" : "start"}" font-size="8">${esc(r.kind)}</text></g>`;
      })
      .join("")}${elements
      .map((e) => {
        const { x, y } = coords.get(e.id);
        return `<g class="model-node ${state.modelElement === e.id ? "selected" : ""}" data-action="model-element" data-id="${e.id}" tabindex="0" role="button" aria-label="Inspect ${esc(e.name)}"><rect class="el ${m.language === "ArchiMate" ? color(e.kind) : ""}" x="${x}" y="${y}" width="210" height="84" rx="${e.kind === "business_process" ? 25 : 3}"/><text x="${x + 13}" y="${y + 20}" font-size="8" opacity=".65">${esc(m.language === "UML" ? "«" + e.kind + "»" : e.kind.replaceAll("_", " "))}</text><text x="${x + 13}" y="${y + 41}" font-size="12" font-weight="500">${esc(e.name.slice(0, 28))}</text>${m.language === "UML" ? `<line x1="${x}" y1="${y + 50}" x2="${x + 210}" y2="${y + 50}" stroke="#b6c7bc"/><text x="${x + 13}" y="${y + 68}" font-size="8">${esc((e.documentation || "").split("\n")[0])}</text>` : `<text x="${x + 13}" y="${y + 63}" font-size="8" opacity=".65">${e.asset ? "↗ " + esc(db.requirements.find((r) => r.id === e.asset)?.key || db.principles.find((r) => r.id === e.asset)?.key || "governed asset") : "Model element"}</text>`}</g>`;
      })
      .join("")}</svg>`;
  }
  function modelsPage() {
    const m = byId("models", state.selected.models) || db.models[0];
    if (!m)
      return `<div class="page">${header("Architecture & UML", "Create a model, add elements and derive views.", btn("New model", "create-model", "", "primary", "plus"))}<div class="empty">No models yet.</div></div>`;
    state.selected.models = m.id;
    const view = m.views.find((v) => v.id === state.modelView) || m.views[0];
    return `<div class="page">${header("Architecture & UML", "Shared model elements. Purposeful views. Traceable to principles and requirements.", btn("New model", "create-model", "", "primary", "plus"), "GOVERNANCE / MODELLING")}<div class="model-layout"><section class="entity-list model-list">${db.models.map((x) => `<button class="entity ${m.id === x.id ? "active" : ""}" data-action="open-model" data-id="${x.id}"><span class="entity-title">${esc(x.name)}</span>${badge(x.language, x.language === "UML" ? "purple" : "green")}<small>${esc(nameOf("projects", x.project))} · ${x.views.length} views</small></button>`).join("")}</section><div class="stack"><section class="panel"><div class="detail-header"><div class="between"><div class="row">${badge(m.language, m.language === "UML" ? "purple" : "green")}${badge(m.status)}<span class="mono muted">v${m.version}</span></div>${btn("Edit model", "edit-model", `data-id="${m.id}"`, "small", "edit")}</div><h2>${esc(m.name)}</h2><p>${esc(nameOf("projects", m.project))} · ${m.language === "UML" ? "conveyor-uml-2.5.1-v1" : "conveyor-archimate-3.2-v1"}</p></div><div class="model-view-bar"><div class="row"><label class="mono muted" for="model-view" style="font-size:9px">VIEW</label><select id="model-view">${m.views.map((v) => `<option value="${v.id}" ${view?.id === v.id ? "selected" : ""}>${esc(v.name)}</option>`).join("")}</select>${btn("Edit", "edit-view", `data-id="${view?.id || ""}"`, "quiet small")}</div><div class="row">${btn("New view", "create-view", "", "small", "plus")}${btn("Export SVG", "export-svg", "", "small", "download")}</div></div><div class="model-canvas-wrap">${modelSvg(m, view)}</div><div class="panel-head" style="border-top:1px solid var(--line);border-bottom:0;flex-wrap:wrap"><div class="row">${btn("Add element", "create-element", "", "small", "plus")}${btn("Add relationship", "create-relationship", "", "small", "branch")}</div>${btn("Check structure", "validate-model", "", "small", "check")}</div></section><section class="panel"><div class="panel-head"><h2>Model content</h2><div class="segmented">${["Elements", "Relationships", "Views", "Validation"].map((t) => `<button class="${state.modelTab === t ? "selected" : ""}" data-action="model-tab" data-value="${t}">${t}</button>`).join("")}</div></div><div class="panel-body">${modelContent(m)}</div></section></div></div></div>`;
  }
  function modelContent(m) {
    if (state.modelTab === "Validation")
      return `<div class="info-note">${state.modelReport || "Run Check structure to check local element kinds, references and view membership. This prototype does not implement the production ArchiMate/UML semantic validator."}</div>`;
    if (state.modelTab === "Relationships")
      return `<div class="matrix-wrap"><table><thead><tr><th>Source</th><th>Relationship</th><th>Target</th><th></th></tr></thead><tbody>${m.relationships.map((r) => `<tr><td>${esc(m.elements.find((e) => e.id === r.source)?.name)}</td><td class="mono">${esc(r.kind)}</td><td>${esc(m.elements.find((e) => e.id === r.target)?.name)}</td><td>${btn("Delete", "delete-model-part", `data-kind="relationships" data-id="${r.id}"`, "quiet small danger")}</td></tr>`).join("")}</tbody></table></div>`;
    if (state.modelTab === "Views")
      return `<div class="stack">${m.views.map((v) => `<div class="between"><div><strong style="font-size:12px">${esc(v.name)}</strong><p class="muted" style="font-size:10px">${esc(v.kind)} · ${v.elements.length} element occurrences</p></div><div class="row">${btn("Edit", "edit-view", `data-id="${v.id}"`, "small")}${btn("Delete", "delete-model-part", `data-kind="views" data-id="${v.id}"`, "quiet small danger")}</div></div>`).join("")}</div>`;
    return `<div class="matrix-wrap"><table><thead><tr><th>Element</th><th>Kind</th><th>Trace</th><th></th></tr></thead><tbody>${m.elements.map((e) => `<tr><td><button class="text-button" data-action="model-element" data-id="${e.id}">${esc(e.name)}</button></td><td class="mono">${esc(e.kind)}</td><td class="mono">${esc(db.requirements.find((r) => r.id === e.asset)?.key || db.principles.find((r) => r.id === e.asset)?.key || "—")}</td><td>${btn("Edit", "edit-element", `data-id="${e.id}"`, "quiet small")}</td></tr>`).join("")}</tbody></table></div>`;
  }
  function decisionsPage() {
    return `<div class="page">${header("Decisions", "One tenant inbox for questions, reviews and sign-offs across every project.", badge(pending().length + " waiting", "amber"))}<div class="split"><section class="panel"><div class="panel-head"><h2>Needs your judgment</h2>${icon("inbox")}</div>${decisionRows()}</section><section class="panel"><div class="panel-head"><h2>Decision history</h2>${badge("Person / agent + channel")}</div><div class="panel-body">${
      db.decisions
        .filter((d) => d.state !== "Pending")
        .map(
          (d) =>
            `<div class="revision"><div class="between"><strong>${esc(d.name)}</strong>${badge(d.state, d.state === "Approved" ? "green" : "amber")}</div><small>George · person · Web · ${esc(nameOf("projects", d.project))}</small>${d.note ? `<p style="font-size:11px;margin-top:9px">${esc(d.note)}</p>` : ""}</div>`,
        )
        .join("") ||
      '<div class="empty">Your next decision will appear here.<p>Decisions keep their actor kind, channel and evidence reference.</p></div>'
    }</div></section></div></div>`;
  }
  function runsPage() {
    const rows = db.projects.filter(
      (p) =>
        state.runFilter === "All runs" ||
        (state.runFilter === "Running"
          ? p.status === "Running"
          : p.tone === "amber"),
    );
    return `<div class="page">${header("Runs & activity", "Observe concurrent work across projects without losing its process, actor or provenance.", btn("Open projects", "navigate", 'data-page="projects"', "", "projects"))}<div class="toolbar"><div class="segmented">${["All runs", "Running", "Waiting"].map((t) => `<button class="${state.runFilter === t ? "selected" : ""}" data-action="run-filter" data-value="${t}">${t}</button>`).join("")}</div><span class="mono muted">TENANT / ${rows.length} RUNS</span></div><section class="panel"><div class="matrix-wrap"><table><thead><tr><th>Project / run</th><th>Process</th><th>Executor</th><th>State</th><th>Conveyor cost</th><th></th></tr></thead><tbody>${rows.map((p, i) => `<tr><td><strong>${esc(p.name)}</strong><small>${esc(p.key)} / RUN-${148 + i}</small></td><td>${esc(nameOf("processes", p.process))}<small>Pinned v${byId("processes", p.process)?.version}</small></td><td>${badge("procedure", "")}<small>${p.agents.length} roles · internal_agent steps</small></td><td>${badge(p.status, p.tone)}</td><td>$${p.spent.toFixed(2)}</td><td>${btn("Inspect", "open-project", `data-id="${p.id}"`, "small", "arrow")}</td></tr>`).join("")}</tbody></table></div></section><div class="split" style="margin-top:24px"><section class="panel"><div class="panel-head"><h2>Recent events</h2>${icon("clock")}</div><div class="panel-body">${activityList()}</div></section><section class="panel"><div class="panel-head"><h2>Execution boundaries</h2></div><div class="panel-body stack"><div class="info-note">Procedure and internal_agent are the only two executor kinds (D46); the external executor is retired. A waiting approval holds no worker.</div><div class="info-note">Published definitions are pinned. Tenant edits create new drafts; they do not rewrite an in-flight project run.</div></div></section></div></div>`;
  }
  function evalPage() {
    return `<div class="page">${header("Evaluations", "Measure a single flow with a frozen instrument and a person’s sign-off.", btn(state.evalQueued ? "Evaluation queued" : "Start evaluation", "start-eval", "", "primary", "run"), "LEARN / VERSIONED INSTRUMENTS")}<div class="split"><section class="panel"><div class="detail-header"><div class="row">${badge("Contract parity", "purple")}${badge("v2")}<span class="mono muted">sha256: 73e8b4c9…</span></div><h2>Invoice contract parity</h2><p>Billing API migration · one subject flow · 30 illustrative cases</p></div><div class="panel-body stack"><dl class="kv"><dt>Input sign-off</dt><dd>${badge("Approved · George", "green")}</dd><dt>Grading sign-off</dt><dd>${badge(state.evalSigned ? "Approved · George" : "Missing", state.evalSigned ? "green" : "amber")}</dd><dt>Training / held-out</dt><dd>20 / 10 cases</dd><dt>Headline</dt><dd>Held-out contract pass rate</dd><dt>State</dt><dd>${badge(state.evalQueued ? "Queued preview" : "Awaiting sign-off", state.evalQueued ? "green" : "amber")}</dd></dl>${btn("Review pilot grading", "review", 'data-id="grading"', "", "shield")}<div class="info-note">Starting is refused while the instrument’s grading sign-off is missing. A changed instrument invalidates its sign-offs. The executor is not given held-out transcripts.</div></div></section><section class="panel"><div class="panel-head"><h2>Improvement candidates</h2>${badge("Illustrative")}</div><div class="panel-body stack"><div><h3>Build a feature v8</h3><p class="muted" style="font-size:12px;margin-top:8px">Candidate: replace repeated file context with a pinned source recipe. No procedure is automatically changed.</p></div><div class="code">Subject: procedure · Build a feature
Instrument: invitation-acceptance@3
Baseline: published procedure v7
Candidate: draft procedure v8
Decision: pending held-out measurement</div>${btn("Inspect process", "open-definition", 'data-kind="processes" data-id="feature"', "small", "flow")}</div></section></div></div>`;
  }
  function usagePage() {
    return `<div class="page">${header("Usage & audit", "Follow model costs to the work that incurred them. Audit proposals stay reviewable.", btn("Export sample ledger", "export-usage", "", "", "download"), "LEARN / INFERENCE LEDGER")}${stats()}<div class="split"><section class="panel"><div class="panel-head"><h2>Provider prompt cache</h2>${badge("Illustrative")}</div><div class="panel-body stack"><div class="between"><span class="muted">Hit ratio</span><span style="font-size:32px">75%</span></div><div style="display:flex;height:16px;border-radius:4px;overflow:hidden"><span style="width:69.2%;background:#6c9f87"></span><span style="width:23.1%;background:#c3cebf"></span><span style="width:5.1%;background:#d5ac77"></span><span style="width:2.6%;background:#a89bb9"></span></div><dl class="kv"><dt>Hits / misses</dt><dd>108 / 36</dd><dt>Writes / unknown</dt><dd>8 / 4</dd></dl><div class="info-note">108 ÷ (108 + 36). Writes and unknown calls are excluded from the ratio. Agent-platform inference is its own product and outside this ledger.</div></div></section><section class="panel"><div class="panel-head"><h2>Prompt audit proposal</h2>${badge("Review required", "amber")}</div><div class="panel-body stack"><h3>Reuse the stable procedure prefix</h3><p class="muted" style="font-size:12px">The draft repeats dynamic work-package text ahead of the stable process context. Move the pinned procedure and role sources before project data.</p><div class="code">Recipe: implementation_role@4
Finding: cache-hostile ordering
Proposal: reorder source classes
Effect: no changes applied automatically</div>${btn("Inspect agent context", "open-definition", 'data-kind="agents" data-id="builder"', "small", "agent")}</div></section></div></div>`;
  }
  function render() {
    renderSidebar();
    const titles = {
      overview: "Overview",
      projects: "Projects",
      project: nameOf("projects", state.project),
      types: "Project types",
      capabilities: "Capabilities",
      processes: "Processes",
      agents: "Agents",
      tools: "Tools & actions",
      requirements: "Requirements",
      principles: "Principles",
      models: "Architecture & UML",
      decisions: "Decisions",
      runs: "Runs & activity",
      evals: "Evaluations",
      usage: "Usage & audit",
    };
    const title = enterprise?.titles[state.page] || titles[state.page];
    $("#crumb").textContent = title;
    document.title = title + " · Conveyor";
    let html;
    if (enterprise?.pages[state.page]) html = enterprise.pages[state.page]();
    else if (catalogueInfo[state.page]) html = cataloguePage(state.page);
    else if (["requirements", "principles"].includes(state.page))
      html = governancePage(state.page);
    else
      html = (
        {
          overview,
          projects: projectsPage,
          project: projectPage,
          tools: toolPage,
          models: modelsPage,
          decisions: decisionsPage,
          runs: runsPage,
          evals: evalPage,
          usage: usagePage,
        }[state.page] || overview
      )();
    $("#main").innerHTML = html;
  }
  // The operator console signs in and out after boot. Re-render through the normal path, and
  // restore the destination console.js saved before the OIDC redirect (the query string
  // returns without a hash). A page it does not know is not invented.
  window.addEventListener("conveyor-console", () => {
    const hash = location.hash.slice(1);
    const known = [
      "overview",
      "projects",
      "types",
      "capabilities",
      "processes",
      "agents",
      "tools",
      "requirements",
      "principles",
      "models",
      "decisions",
      "runs",
      "evals",
      "usage",
    ];
    if (hash && (enterprise?.pages?.[hash] || known.includes(hash))) state.page = hash;
    render();
  });
  function evidenceMarkup(p, step, status) {
    if (p.id !== "portal")
      return '<div class="info-note">This step has no attached output in the sample fixture. Inspect Context for its source bindings and Tools for its declared actions.</div>';
    const n = state.node;
    if (n === 0)
      return `<div><h4>Approved input</h4>${binding("requirements", "invite", "REQ-042@3 · approved requirement")}${binding("principles", "isolation", "PRN-001@4 · governing principle")}</div><div class="checks"><div>${icon("check")}Requirement revision resolved</div><div>${icon("check")}Scope approved before implementation</div></div>`;
    if (n === 1)
      return `<div><h4>Implementation plan</h4><div class="code">1. Add a tenant-bound invitation token.
2. Enforce single use and 72-hour expiry.
3. Test tenant, role and expiry boundaries.
4. Attach a patch for independent review.</div></div><div class="checks"><div>${icon("check")}Derived from REQ-042@3</div></div>`;
    if (n === 5)
      return '<div class="info-note">Publication is queued. No output has been produced by this step yet. The approval decision is recorded separately from a repository merge.</div>';
    const patch = `<button class="file-row" data-action="artifact" data-kind="patch">${icon("code")}<span class="grow">implementation.patch<small>3 files · +128 / −14</small></span>↗</button>`;
    const report = `<button class="file-row" data-action="artifact" data-kind="report">${icon("file")}<span class="grow">verification-report.json<small>24 checks · all passed</small></span>↗</button>`;
    const returned =
      db.decisions.find((d) => d.id === "review").state === "Returned";
    return `<div><h4>${returned ? "Previous evidence · revision requested" : "Attached evidence"}</h4>${n === 3 ? "" : patch}${n === 2 ? "" : report}</div><div class="checks"><div>${icon("check")}Acceptance criteria traced</div><div>${icon("check")}Cross-tenant behavior checked</div>${n === 4 ? `<div>${icon("check")}Independent evidence attached</div>` : ""}</div>${returned ? '<div class="info-note">These artifacts belong to the prior attempt. A revised patch needs fresh verification and another human review.</div>' : ""}`;
  }
  const modal = $("#dialog");
  function openDialog(title, body, footer = "", wide = false) {
    modal.innerHTML = `<div class="dialog-head"><h2 id="dialog-title">${esc(title)}</h2><button class="icon-button" data-action="close-dialog" aria-label="Close dialog">${icon("close")}</button></div>${body}<div class="dialog-foot">${footer || btn("Close", "close-dialog")}</div>`;
    modal.style.width = wide ? "min(860px,calc(100vw - 28px))" : "";
    if (!modal.open) modal.showModal();
  }
  function dialogForm(title, body, id, submit = "Save local draft", data = "") {
    openDialog(
      title,
      `<form id="${id}" ${data}><div class="dialog-body"><div class="form-grid">${body}</div><div id="form-error" class="error" role="alert" hidden></div></div></form>`,
      `<small>Local prototype · no backend writes</small>${btn("Cancel", "close-dialog")}<button class="btn primary" type="submit" form="${id}">${esc(submit)}</button>`,
    );
  }
  function error(message) {
    let el = $("#form-error");
    if (!el) {
      el = document.createElement("div");
      el.className = "error";
      el.id = "form-error";
      el.setAttribute("role", "alert");
      $(".dialog-body", modal)?.append(el);
    }
    el.hidden = false;
    el.textContent = message;
  }
  function field(
    label,
    name,
    value = "",
    type = "text",
    hint = "",
    full = false,
  ) {
    return `<label class="field ${full ? "full" : ""}"><span>${esc(label)}</span>${type === "textarea" ? `<textarea name="${name}" ${name === "description" || name === "name" ? "required" : ""}>${esc(value)}</textarea>` : `<input name="${name}" type="${type}" value="${esc(value)}" ${["name", "key", "endpoint"].includes(name) ? "required" : ""} ${type === "number" ? 'min="1" step="1"' : ""}>`}${hint ? `<small>${esc(hint)}</small>` : ""}</label>`;
  }
  function selectField(label, name, options, value, full = false) {
    return `<label class="field ${full ? "full" : ""}"><span>${esc(label)}</span><select name="${name}">${options
      .map((o) => {
        const id = typeof o === "string" ? o : o.id,
          name = typeof o === "string" ? o : o.name;
        return `<option value="${esc(id)}" ${value === id ? "selected" : ""}>${esc(name)}</option>`;
      })
      .join("")}</select></label>`;
  }
  function checkField(label, name, options, selected = [], hint = "") {
    return `<div class="field full"><span>${esc(label)}</span><div class="checkbox-list">${options.map((o) => `<label><input type="checkbox" name="${name}" value="${esc(o.id)}" ${selected.includes(o.id) ? "checked" : ""}>${esc(o.name)}${o.note ? ` <small>${esc(o.note)}</small>` : ""}</label>`).join("")}</div>${hint ? `<small>${esc(hint)}</small>` : ""}</div>`;
  }
  function editEntity(kind, id) {
    const x = byId(kind, id),
      v = x || {},
      isGov = ["requirements", "principles"].includes(kind),
      info = catalogueInfo[kind];
    let body =
      field("Name", "name", v.name || "", "text", "", true) +
      field(
        isGov ? "Statement / requirement" : "Purpose",
        "description",
        v.description || v.role || "",
        "textarea",
        "",
        true,
      );
    if (kind === "types")
      body +=
        field(
          "Ordered phases",
          "phases",
          (
            v.phases || ["Discover", "Define", "Build", "Verify", "Release"]
          ).join("\n"),
          "textarea",
          "One phase per line. These are the tenant’s lifecycle stages.",
          true,
        ) +
        checkField(
          "Capabilities",
          "capabilities",
          db.capabilities.filter((c) => c.status !== "Retired"),
          v.capabilities || [],
        ) +
        field(
          "Allowed agent job types (card-type codes)",
          "cardTypes",
          (v.cardTypes || ["Work package"]).join("\n"),
          "textarea",
          "One job-type code per line. The job library supplies the extensible contracts.",
        ) +
        field(
          "Human gates",
          "gates",
          (v.gates || ["Implementation review"]).join("\n"),
          "textarea",
          "One gate per line.",
        );
    if (kind === "capabilities")
      body += field(
        "Required knowledge pack pin",
        "knowledge",
        v.knowledge || "domain-pack@1",
        "text",
        "Use an exact version, e.g. engineering-handbook@5.",
        true,
      );
    if (kind === "processes")
      body +=
        selectField(
          "Parent capability",
          "capability",
          db.capabilities,
          v.capability || state.selected.capabilities || "delivery",
          true,
        ) +
        `<div class="info-note" style="grid-column:1/-1">Save the process, then add and edit its steps. A step chooses a kind, agent binding, tool action and intent.</div>`;
    if (kind === "agents")
      body +=
        selectField(
          "Executor kind",
          "kind",
          ["Internal agent"],
          v.kind || "Internal agent",
        ) +
        field(
          "Model route",
          "model",
          v.model || "reasoning",
        ) +
        field("Role recipe pin", "prompt", v.prompt || "role_recipe@1") +
        field(
          "Maximum internal turns",
          "maxTurns",
          v.maxTurns || 12,
          "number",
        ) +
        `<label class="field"><span>Internal call budget ($)</span><input type="number" name="budget" min="0.01" step="0.01" value="${v.budget || 1}"><small>Enforced for the internal loop. External inference stays external.</small></label>` +
        checkField(
          "Tool grants",
          "tools",
          allTools()
            .filter(
              (t) =>
                (v.tools || []).includes(t.id) ||
                [
                  "exec_in_workspace_async",
                  "get_exec_job_status",
                  "get_workspace_git_status",
                  "list_workspace_files",
                  "write_workspace_file",
                  "execute_test_suite",
                  "get_test_suite_results",
                  "create_pull_request",
                  "models.create",
                  "model_views.create",
                  "requirements.create",
                  "principles.list",
                ].includes(t.id),
            )
            .map((t) => ({ id: t.id, name: t.id, note: t.role })),
          v.tools || [],
          "Browse Tools & actions to grant any additional action.",
        );
    if (isGov)
      body +=
        field(
          "Key",
          "key",
          v.key ||
            `${kind === "requirements" ? "REQ" : "PRN"}-${String(db[kind].length + 100).padStart(3, "0")}`,
        ) +
        selectField(
          "Scope",
          "project",
          [{ id: "", name: "Tenant-wide" }, ...db.projects],
          v.project || "",
        ) +
        field(
          kind === "requirements"
            ? "Acceptance criteria"
            : "Rationale & implications",
          "acceptance",
          v.acceptance || "",
          "textarea",
          "One criterion or implication per line.",
          true,
        ) +
        (kind === "requirements"
          ? selectField(
              "Governing principle",
              "principle",
              [{ id: "", name: "No explicit link" }, ...db.principles],
              v.principle || "",
              true,
            )
          : "");
    dialogForm(
      `${x ? "Edit" : "Create"} ${info?.singular || (kind === "requirements" ? "requirement" : "principle")}`,
      body,
      "entity-form",
      x ? "Save new draft revision" : "Create local draft",
      `data-kind="${kind}" data-id="${esc(id || "")}"`,
    );
  }
  function touch(x) {
    if (x.status !== "Draft") {
      x.version++;
      x.status = "Draft";
    }
    x.history = x.history || [];
    x.history.unshift(`v${x.version} · Draft edited · George · just now`);
  }
  function saveEntity(form) {
    const f = new FormData(form),
      kind = form.dataset.kind,
      id = form.dataset.id,
      x = byId(kind, id),
      name = String(f.get("name") || "").trim();
    if (!name) return error("Give the definition a name.");
    if (
      db[kind].some(
        (y) => y.id !== id && y.name.toLowerCase() === name.toLowerCase(),
      )
    )
      return error("A definition with this name already exists.");
    const obj = x
      ? structuredClone(x)
      : { id: uid(kind), status: "Draft", version: 1, history: [] };
    obj.name = name;
    obj.description = String(f.get("description") || "").trim();
    if (kind === "types") {
      obj.phases = String(f.get("phases"))
        .split("\n")
        .map((x) => x.trim())
        .filter(Boolean);
      if (!obj.phases.length) return error("Define at least one phase.");
      if (new Set(obj.phases).size !== obj.phases.length)
        return error("Phase names must be unique.");
      obj.capabilities = f.getAll("capabilities");
      obj.cardTypes = String(f.get("cardTypes")).split("\n").filter(Boolean);
      obj.gates = String(f.get("gates")).split("\n").filter(Boolean);
      obj.matrix = obj.matrix || {};
    }
    if (kind === "capabilities") {
      obj.knowledge = String(f.get("knowledge") || "");
      obj.processes = obj.processes || [];
    }
    if (kind === "processes") {
      obj.capability = String(f.get("capability"));
      obj.steps = obj.steps || [];
    }
    if (kind === "agents") {
      obj.role = obj.description;
      obj.kind = String(f.get("kind"));
      obj.model = String(f.get("model"));
      obj.prompt = String(f.get("prompt"));
      obj.maxTurns = Number(f.get("maxTurns"));
      obj.budget = Number(f.get("budget"));
      obj.tools = f.getAll("tools");
    }
    if (["requirements", "principles"].includes(kind)) {
      obj.key = String(f.get("key")).trim();
      if (db[kind].some((y) => y.id !== id && y.key === obj.key))
        return error("This key is already in use.");
      obj.project = String(f.get("project"));
      obj.acceptance = String(f.get("acceptance"));
      if (kind === "requirements")
        obj.principle = String(f.get("principle") || "");
      obj.history.unshift(
        `v${obj.version} · ${x ? "New draft revision" : "Draft created"} · George · just now`,
      );
    }
    if (x) {
      touch(obj);
      Object.assign(x, obj);
    } else db[kind].push(obj);
    if (kind === "processes") syncProcesses();
    state.selected[kind] = obj.id;
    state.entityQuery = "";
    state.catalogTab = "Definition";
    activity("George", `${x ? "edited" : "created"} ${obj.name}`, "orange");
    modal.close();
    render();
    toast(`${obj.name} saved as a local draft.`);
  }
  function syncProcesses() {
    db.capabilities.forEach(
      (c) =>
        (c.processes = db.processes
          .filter((p) => p.capability === c.id)
          .map((p) => p.id)),
    );
  }
  function editStep(processId, index) {
    const p = byId("processes", processId),
      s = p.steps[index] || {};
    dialogForm(
      index === undefined ? "Add process step" : "Edit process step",
      field("Step intent", "name", s.name || "", "text", "", true) +
        selectField(
          "Step kind",
          "kind",
          ["Action", "Internal agent", "Human gate"],
          s.kind || "Internal agent",
        ) +
        selectField(
          "Agent role",
          "agent",
          [{ id: "", name: "No agent (action or gate)" }, ...db.agents],
          s.agent || "",
        ) +
        selectField(
          "Registered tool action",
          "tool",
          allTools().map((t) => ({ id: t.id, name: t.id })),
          s.tool || "requirements.get",
          true,
        ),
      "step-form",
      "Save draft step",
      `data-id="${processId}" data-index="${index ?? ""}"`,
    );
    if (index !== undefined)
      $(".dialog-foot").insertAdjacentHTML(
        "afterbegin",
        btn(
          "Delete step",
          "delete-step",
          `data-id="${processId}" data-index="${index}"`,
          "quiet danger",
        ),
      );
  }
  function saveStep(form) {
    const f = new FormData(form),
      p = byId("processes", form.dataset.id),
      i = form.dataset.index === "" ? undefined : Number(form.dataset.index),
      s = {
        name: String(f.get("name")).trim(),
        kind: String(f.get("kind")),
        agent: String(f.get("agent")),
        tool: String(f.get("tool")),
      };
    if (s.kind.includes("agent")) {
      const a = byId("agents", s.agent);
      if (!a) return error("Choose an agent for this step.");
      if (a.kind !== s.kind)
        return error("The selected role must match the step’s executor kind.");
      if (!a.tools.includes(s.tool))
        return error(
          "This action is not granted to the selected agent. Add the tool grant in Agents first.",
        );
    } else s.agent = "";
    touch(p);
    if (i === undefined) p.steps.push(s);
    else p.steps[i] = s;
    modal.close();
    render();
    toast(
      "Draft process step saved. Existing runs retain their pinned version.",
    );
  }
  function references(kind, id) {
    if (kind === "types")
      return db.projects.filter((p) => p.type === id).map((p) => p.name);
    if (kind === "capabilities")
      return [
        ...db.types
          .filter((t) => t.capabilities.includes(id))
          .map((t) => t.name),
        ...db.processes.filter((p) => p.capability === id).map((p) => p.name),
      ];
    if (kind === "processes")
      return db.projects.filter((p) => p.process === id).map((p) => p.name);
    if (kind === "agents")
      return db.processes
        .filter((p) => p.steps.some((s) => s.agent === id))
        .map((p) => p.name);
    if (kind === "requirements")
      return [
        ...db.cards.filter((c) => c.requirement === id).map((c) => c.name),
        ...db.models
          .filter((m) => m.elements.some((e) => e.asset === id))
          .map((m) => m.name),
      ];
    if (kind === "principles")
      return [
        ...db.requirements.filter((r) => r.principle === id).map((r) => r.name),
        ...db.models
          .filter((m) => m.elements.some((e) => e.asset === id))
          .map((m) => m.name),
      ];
    return [];
  }
  function deleteEntity(kind, id) {
    const x = byId(kind, id),
      refs = references(kind, id);
    if (x.status === "Draft" && refs.length)
      return openDialog(
        "This draft is still referenced",
        `<div class="dialog-body stack"><p>Remove these links before deleting ${esc(x.name)}.</p><div class="code">${esc(refs.join("\n"))}</div></div>`,
      );
    const retire = x.status !== "Draft";
    openDialog(
      retire ? "Retire definition" : "Delete local draft",
      `<div class="dialog-body"><p>${retire ? "Retire" : "Delete"} <strong>${esc(x.name)}</strong>?</p><p class="muted" style="font-size:12px;margin-top:12px">${retire ? "Historical runs and trace links retain this version. It will be marked retired for new work." : "This removes an unreferenced draft from the local fixture."}</p></div>`,
      `${btn("Cancel", "close-dialog")}${btn(retire ? "Retire" : "Delete", "confirm-delete", `data-kind="${kind}" data-id="${id}"`, "danger")}`,
    );
  }
  function publish(kind, id) {
    const x = byId(kind, id);
    let issues = [];
    if (kind === "types" && !x.capabilities.length)
      issues.push("Bind at least one capability.");
    if (kind === "capabilities" && !x.processes.length)
      issues.push("Add a process for this capability.");
    if (kind === "agents") {
      if (!x.tools.length) issues.push("Grant at least one tool.");
      if (!/@\d+$/.test(x.prompt)) issues.push("Pin a role recipe version.");
    }
    if (kind === "processes") {
      if (!x.steps.length) issues.push("Add at least one process step.");
      x.steps.forEach((s) => {
        if (
          s.kind.includes("agent") &&
          !byId("agents", s.agent)?.tools.includes(s.tool)
        )
          issues.push(`${s.name}: agent lacks the selected tool grant.`);
      });
    }
    if (issues.length)
      return openDialog(
        "Definition is not ready",
        `<div class="dialog-body stack"><p>Resolve these draft issues before the publication preview:</p><div class="code">${esc(issues.join("\n"))}</div></div>`,
      );
    x.status = "Published";
    x.history = x.history || [];
    x.history.unshift(`v${x.version} · Published preview · George · just now`);
    render();
    toast(
      "Local publication preview recorded. No production definition was published.",
    );
  }
  function createProject() {
    dialogForm(
      "Start a project",
      field("Project name", "name", "", "text", "", true) +
        field("Objective", "description", "", "textarea", "", true) +
        selectField(
          "Published project type",
          "type",
          db.types.filter((t) => t.status === "Published"),
          "product",
          true,
        ),
      "project-form",
      "Create project",
    );
  }
  function saveProject(form) {
    const f = new FormData(form),
      type = byId("types", String(f.get("type")));
    if (!type) return error("Publish a project type first.");
    const proc = db.processes.find(
      (p) =>
        type.capabilities.includes(p.capability) && p.status === "Published",
    );
    if (!proc)
      return error(
        "This type needs a published process before a project can start.",
      );
    const p = {
      id: uid("project"),
      key: "PRJ-" + String(db.projects.length + 24).padStart(3, "0"),
      name: String(f.get("name")).trim(),
      description: String(f.get("description")).trim(),
      type: type.id,
      phase: 0,
      status: "Queued",
      tone: "",
      agents: [...new Set(proc.steps.map((s) => s.agent).filter(Boolean))],
      completed: 0,
      total: 1,
      spent: 0,
      process: proc.id,
      symbol: "projects",
      typePin: structuredClone(type),
      processPin: structuredClone(proc),
    };
    p.agentPins = Object.fromEntries(
      p.agents.map((id) => [id, structuredClone(byId("agents", id))]),
    );
    db.projects.push(p);
    db.cards.push({
      id: "C-" + (300 + db.cards.length),
      project: p.id,
      name: "Define approved scope",
      list: "Ready",
      requirement: "",
    });
    activity("George", `created ${p.name}`, "orange");
    modal.close();
    navigate("project", p.id);
    toast(
      "Project created locally with pinned definitions and a queued sample run.",
    );
  }
  function cardDialog(id) {
    const c = byId("cards", id);
    dialogForm(
      c.name,
      field("Work item name", "name", c.name, "text", "", true) +
        selectField(
          "Work list",
          "list",
          ["Ready", "Build", "Review", "Done"],
          c.list,
        ) +
        selectField(
          "Requirement trace",
          "requirement",
          [
            { id: "", name: "No linked requirement" },
            ...db.requirements.filter((r) => r.project === c.project),
          ],
          c.requirement,
        ),
      "card-form",
      "Save work item",
      `data-id="${id}"`,
    );
  }
  function addCard() {
    const p = byId("projects", state.project);
    dialogForm(
      "New work item",
      field("Work item name", "name", "", "text", "", true) +
        selectField(
          "Initial list",
          "list",
          ["Ready", "Build", "Review", "Done"],
          "Ready",
        ) +
        selectField(
          "Requirement",
          "requirement",
          [
            { id: "", name: "No linked requirement" },
            ...db.requirements.filter((r) => r.project === p.id),
          ],
          "",
        ),
      "card-form",
      "Create work item",
    );
  }
  function review(id) {
    const d = byId("decisions", id);
    if (!d) return;
    if (d.state !== "Pending")
      return openDialog(
        "Decision already recorded",
        `<div class="dialog-body"><p>${esc(d.state)} · George · person · Web</p></div>`,
      );
    const body =
      id === "review"
        ? `<div class="stack"><div class="between">${badge("Implementation review", "amber")}<span class="mono muted">Customer portal / C-142</span></div><h3>Workspace invitations</h3><p class="muted" style="font-size:12px">Review the patch and independent verification against REQ-042@3. Approval releases the next process step; it does not merge a repository.</p><div class="diff"><span class="muted">src/invitations.rs · illustrative patch</span><span class="remove">− let invite = Invitation::new(email);</span><span class="add">+ let invite = Invitation::for_tenant(tenant.id, workspace.id, email);</span><span class="add">+ invite.expires_at = now + Duration::hours(72);</span><span class="add">+ invite.require_unused_token()?;</span></div><div class="checks"><div>${icon("check")}24 / 24 acceptance checks passed</div><div>${icon("check")}Tenant isolation and expired tokens verified</div><div>${icon("check")}Patch and report attached by distinct actors</div></div><div class="row">${btn("Inspect full patch", "artifact", 'data-kind="patch"', "small", "code")}${btn("Inspect report", "artifact", 'data-kind="report"', "small", "file")}</div></div>`
        : id === "scope"
          ? `<div class="stack">${badge("Scope question", "amber")}<h3>May a service account delegate its access?</h3><p class="muted" style="font-size:12px">The analyst needs a decision for REQ-061. Proposed boundary: a service identity can delegate only a strict subset of its granted actions, within its own tenant. Further delegation is refused.</p><div class="info-note">Governing principle: PRN-001 · Tenant isolation by construction. Your answer becomes a durable approval decision.</div></div>`
          : `<div class="stack">${badge("Grading sign-off", "amber")}<h3>Invoice contract parity · instrument v2</h3><p class="muted" style="font-size:12px">Review the pilot outputs against the expected verdicts. This is a person-only sign-off for digest 73e8b4c9…</p><div class="matrix-wrap"><table><thead><tr><th>Pilot case</th><th>Observed / expected</th><th>Grader</th></tr></thead><tbody><tr><td>Valid invoice</td><td>201 + invoice ID / 201 + invoice ID</td><td>${badge("Pass", "green")}</td></tr><tr><td>Unknown account</td><td>404 + account_missing / same</td><td>${badge("Pass", "green")}</td></tr><tr><td>Invalid amount</td><td>422 + amount_invalid / same</td><td>${badge("Pass", "green")}</td></tr></tbody></table></div><div class="info-note">Illustrative pilot for design review. Approving records a local sign-off for this digest and enables the queued evaluation preview.</div></div>`;
    openDialog(
      d.name,
      `<div class="dialog-body stack">${body}<label class="field"><span>${id === "scope" ? "Your answer (required)" : "Review note (required when returning)"}</span><textarea id="decision-note" placeholder="${id === "scope" ? "Record the boundary the process should follow…" : "Add context for the next actor…"}"></textarea></label><div id="form-error" class="error" role="alert" hidden></div></div>`,
      `<small>George · person · via Web</small>${btn("Return for changes", "decide", `data-id="${id}" data-value="Returned"`)}${btn(id === "scope" ? "Record answer" : id === "grading" ? "Sign off grading" : "Approve & continue", "decide", `data-id="${id}" data-value="Approved"`, "primary", "check")}`,
      true,
    );
  }
  function decide(id, value) {
    const d = byId("decisions", id),
      note = ($("#decision-note")?.value || "").trim();
    if (!d || d.state !== "Pending") return;
    if ((value === "Returned" || id === "scope") && !note)
      return error(
        id === "scope"
          ? "Write the answer the process should receive."
          : "Add a note explaining the changes needed.",
      );
    d.state = value;
    d.note = note;
    const p = byId("projects", d.project);
    if (id === "review") {
      p.status = value === "Approved" ? "Running" : "Changes requested";
      p.tone = value === "Approved" ? "green" : "amber";
      if (value === "Approved") p.phase = 5;
      state.node = value === "Approved" ? 5 : 2;
      const c = byId("cards", "C-142");
      c.list = value === "Approved" ? "Review" : "Build";
    }
    if (id === "scope") {
      p.status = value === "Approved" ? "Running" : "Needs input";
      p.tone = value === "Approved" ? "green" : "amber";
    }
    if (id === "grading") {
      state.evalSigned = value === "Approved";
    }
    activity("George", `${value.toLowerCase()} ${d.name}`, "orange");
    modal.close();
    render();
    toast(
      value === "Approved"
        ? "Decision recorded locally · George · person · Web."
        : "Returned with your note.",
    );
  }
  function artifact(kind) {
    const report = {
      project: "PRJ-014",
      card: "C-142",
      requirement: "REQ-042@3",
      producer: "Verification agent",
      suite: "invitations-v3",
      passed: 24,
      failed: 0,
      checks: [
        "tenant isolation",
        "72-hour expiry",
        "single-use token",
        "admin-only creation",
      ],
      illustrative: true,
    };
    const patch =
      "--- a/src/invitations.rs\n+++ b/src/invitations.rs\n@@ create_invitation\n- let invite = Invitation::new(email);\n+ let invite = Invitation::for_tenant(tenant.id, workspace.id, email);\n+ invite.expires_at = now + Duration::hours(72);\n+ invite.require_unused_token()?;\n\n// Illustrative patch for this design study.";
    openDialog(
      kind === "patch" ? "implementation.patch" : "verification-report.json",
      `<div class="dialog-body"><span class="eyebrow">ILLUSTRATIVE EVIDENCE / CUSTOMER PORTAL</span><pre class="code">${esc(kind === "patch" ? patch : JSON.stringify(report, null, 2))}</pre></div>`,
      `${btn("Close", "close-dialog")}${btn("Download sample", "download-artifact", `data-kind="${kind}"`, "primary", "download")}`,
      true,
    );
  }
  function download(name, text, type = "application/json") {
    const url = URL.createObjectURL(new Blob([text], { type })),
      a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 5000);
  }
  function grantTool(id) {
    const t = toolOf(id);
    dialogForm(
      "Manage agent grant",
      `<div class="info-note" style="grid-column:1/-1"><span class="mono">${esc(t.id)}</span><br>${esc(t.title)}</div>` +
        checkField(
          "Agents allowed to use this action",
          "agents",
          db.agents,
          db.agents.filter((a) => a.tools.includes(id)).map((a) => a.id),
          "This changes draft tool bindings. Each internal actor must be explicitly permitted.",
        ),
      "grant-form",
      "Save draft grants",
      `data-id="${esc(id)}"`,
    );
  }
  function editTenantTool(id) {
    const t = db.customTools.find((t) => t.id === id);
    dialogForm(
      t ? "Edit tenant tool" : "Register a tenant tool",
      field(
        "Action identifier",
        "key",
        t?.id || "tenant.",
        "text",
        "Stable action key; no credentials.",
        true,
      ) +
        field("Display name", "name", t?.title || "", "text", "", true) +
        field(
          "Endpoint",
          "endpoint",
          t?.path || "https://tools.example.com/mcp",
          "url",
          "Only a design binding; no connection is opened.",
          true,
        ) +
        selectField(
          "Declared effect",
          "role",
          ["unclassified", "observer", "producer"],
          t?.role || "unclassified",
        ),
      "tool-form",
      t ? "Save local binding" : "Register local binding",
      `data-id="${esc(id || "")}"`,
    );
    if (t) $('#tool-form [name="key"]').readOnly = true;
  }
  function editModel(id) {
    const m = byId("models", id);
    dialogForm(
      m ? "Edit model" : "Create a model",
      field("Model name", "name", m?.name || "", "text", "", true) +
        (m
          ? `<div class="info-note" style="grid-column:1/-1">Language: ${esc(m.language)}. Elements and views belong to this model.</div>`
          : selectField(
              "Modelling language",
              "language",
              ["ArchiMate", "UML"],
              "ArchiMate",
            )) +
        selectField(
          "Project scope",
          "project",
          db.projects,
          m?.project || state.project,
        ),
      "model-form",
      m ? "Save model" : "Create model",
      `data-id="${id || ""}"`,
    );
  }
  function editView(id) {
    const m = byId("models", state.selected.models),
      v = m.views.find((v) => v.id === id);
    dialogForm(
      v ? "Edit model view" : "Create a model view",
      field("View name", "name", v?.name || "", "text", "", true) +
        selectField(
          "View kind",
          "kind",
          m.language === "UML"
            ? ["class", "package", "component", "use-case", "sequence"]
            : ["archimate_view"],
          v?.kind || "class",
          true,
        ) +
        checkField(
          "Element occurrences in this view",
          "elements",
          m.elements,
          v?.elements || m.elements.map((e) => e.id),
          "Views select existing elements; they do not copy the model’s semantics.",
        ),
      "view-form",
      v ? "Save view" : "Create view",
      `data-id="${id || ""}"`,
    );
  }
  function editElement(id) {
    const m = byId("models", state.selected.models),
      e = m.elements.find((e) => e.id === id);
    dialogForm(
      e ? "Edit model element" : "Add model element",
      field("Element name", "name", e?.name || "", "text", "", true) +
        selectField(
          "Element kind",
          "kind",
          modelKinds[m.language],
          e?.kind || modelKinds[m.language][0],
          true,
        ) +
        field(
          "Documentation / attributes",
          "documentation",
          e?.documentation || "",
          "textarea",
          "For UML, include attributes or operations as documentation.",
          true,
        ) +
        selectField(
          "Governed-asset trace",
          "asset",
          [
            { id: "", name: "No linked asset" },
            ...db.requirements.map((r) => ({
              id: r.id,
              name: r.key + " · " + r.name,
            })),
            ...db.principles.map((r) => ({
              id: r.id,
              name: r.key + " · " + r.name,
            })),
          ],
          e?.asset || "",
          true,
        ),
      "element-form",
      "Save element",
      `data-id="${id || ""}"`,
    );
    if (e)
      $(".dialog-foot").insertAdjacentHTML(
        "afterbegin",
        btn(
          "Delete element",
          "delete-model-part",
          `data-kind="elements" data-id="${e.id}"`,
          "quiet danger",
        ),
      );
  }
  function createRelationship() {
    const m = byId("models", state.selected.models);
    if (m.elements.length < 2) return toast("Add at least two elements first.");
    dialogForm(
      "Add model relationship",
      selectField("Source element", "source", m.elements, m.elements[0].id) +
        selectField("Target element", "target", m.elements, m.elements[1].id) +
        selectField(
          "Relationship kind",
          "kind",
          relationshipKinds[m.language],
          relationshipKinds[m.language][0],
          true,
        ),
      "relationship-form",
      "Create relationship",
    );
  }
  function mutateModel(m) {
    m.version++;
    m.status = "Draft";
    state.modelReport = null;
  }
  function validateModel() {
    const m = byId("models", state.selected.models),
      ids = new Set(m.elements.map((e) => e.id)),
      issues = [];
    m.elements.forEach((e) => {
      if (!modelKinds[m.language].includes(e.kind))
        issues.push("Element kind outside the curated profile: " + e.name);
      if (
        e.asset &&
        !db.requirements.some((r) => r.id === e.asset) &&
        !db.principles.some((p) => p.id === e.asset)
      )
        issues.push("Broken governed-asset link: " + e.name);
    });
    m.relationships.forEach((r) => {
      if (r.source === r.target) issues.push("Self-reference: " + r.id);
      if (!ids.has(r.source) || !ids.has(r.target))
        issues.push("Missing relationship endpoint: " + r.id);
      if (!relationshipKinds[m.language].includes(r.kind))
        issues.push("Relationship kind outside profile: " + r.kind);
    });
    m.views.forEach((v) =>
      v.elements.forEach((id) => {
        if (!ids.has(id)) issues.push("Missing view element: " + id);
      }),
    );
    state.modelReport = issues.length
      ? "Structural findings: " + issues.join("; ")
      : `Local structural check passed: ${m.elements.length} elements, ${m.relationships.length} relationships, ${m.views.length} views. Full semantic validation is outside this prototype.`;
    state.modelTab = "Validation";
    render();
    toast(
      issues.length
        ? "Structural issues found."
        : "Local model structure checked.",
    );
  }
  function modelElementDialog(id) {
    const m = byId("models", state.selected.models),
      e = m.elements.find((e) => e.id === id);
    state.modelElement = id;
    render();
    openDialog(
      e.name,
      `<div class="dialog-body stack">${badge(e.kind, m.language === "ArchiMate" ? "green" : "purple")}<div class="info-note">An element belongs to the model. Views contain occurrences of this same element.</div><dl class="kv"><dt>Model</dt><dd>${esc(m.name)}</dd><dt>Views</dt><dd>${
        m.views
          .filter((v) => v.elements.includes(e.id))
          .map((v) => esc(v.name))
          .join(", ") || "None"
      }</dd><dt>Trace</dt><dd>${esc(db.requirements.find((r) => r.id === e.asset)?.key || db.principles.find((r) => r.id === e.asset)?.key || "Unlinked")}</dd></dl><p class="muted" style="font-size:12px;white-space:pre-wrap">${esc(e.documentation || "No documentation yet.")}</p>${e.asset ? btn("Open governed asset", "open-asset", `data-id="${e.asset}"`, "small", "file") : ""}</div>`,
      `${btn("Close", "close-dialog")}${btn("Edit element", "edit-element", `data-id="${id}"`, "primary", "edit")}`,
    );
  }
  function deleteModelPart(kind, id) {
    const m = byId("models", state.selected.models);
    if (kind === "views" && m.views.length === 1)
      return toast("Keep at least one view in a model.");
    if (kind === "elements") {
      m.relationships = m.relationships.filter(
        (r) => r.source !== id && r.target !== id,
      );
      m.views.forEach((v) => (v.elements = v.elements.filter((e) => e !== id)));
    }
    m[kind] = m[kind].filter((x) => x.id !== id);
    mutateModel(m);
    modal.close();
    render();
    toast(
      "Removed from the local model. Related view occurrences and relationships were updated.",
    );
  }
  function search() {
    const entries = [
      ...(enterprise?.searchEntries() || []),
      ...Object.entries(catalogueInfo).map(([page, i]) => ({
        name: i.title,
        page,
        kind: "Destination",
      })),
      ...[
        "overview",
        "projects",
        "decisions",
        "tools",
        "requirements",
        "principles",
        "models",
        "evals",
        "usage",
      ].map((page) => ({
        name:
          {
            models: "Architecture & UML",
            evals: "Evaluations",
            tools: "Tools & actions",
            usage: "Usage & audit",
          }[page] || page,
        page,
        kind: "Destination",
      })),
      ...db.projects.map((p) => ({
        name: p.name,
        page: "project",
        id: p.id,
        kind: "Project",
      })),
      ...[
        "types",
        "capabilities",
        "processes",
        "agents",
        "requirements",
        "principles",
        "models",
      ].flatMap((page) =>
        db[page].map((x) => ({ name: x.name, page, id: x.id, kind: page })),
      ),
    ];
    openDialog(
      "Search your workspace",
      `<div class="dialog-body"><label class="input-search">${icon("search")}<input id="global-search" placeholder="Projects, definitions, requirements…" autocomplete="off" aria-label="Search workspace content"></label><div class="search-results" id="search-results"></div></div>`,
    );
    function results(q) {
      $("#search-results").innerHTML =
        entries
          .filter((e) => e.name.toLowerCase().includes(q.toLowerCase()))
          .slice(0, 16)
          .map(
            (e) =>
              `<button class="search-result" data-action="search-go" data-page="${e.page}" data-id="${e.id || ""}">${icon("search")}<span>${esc(e.name)}</span><small>${esc(e.kind)}</small></button>`,
          )
          .join("") ||
        '<div class="empty">No matching workspace records.</div>';
    }
    results("");
    $("#global-search").addEventListener("input", (e) =>
      results(e.target.value),
    );
    $("#global-search").focus();
  }
  function about() {
    openDialog(
      "A tenant’s entire conveyor",
      `<div class="dialog-body stack"><p>This is a local, interactive design study. Work data resets on reload; only the color theme is remembered. No backend, model, coding workspace or repository is connected.</p><div class="info-note">Operate many projects. Define project types → capabilities → processes → agents → tools. Govern intent with principles and requirements; connect it to ArchiMate/UML models and views.</div><p style="font-size:12px" class="muted">Influences: ChatDev’s workflow canvas, DeepSeek Harness’s workbench, the Care Bus agent console, fr’s tenant catalogue and PM matrix, w2v’s stage input/output inspector, and requirements_management’s model/view distinction.</p><p style="font-size:12px" class="muted">The coding catalogue is extracted from the local n8n-nodes-coding OpenAPI snapshot: ${coding.count} operations. Definitions, publication, grants, validation and decisions are UI simulations. Semantic modelling validation and production readiness enforcement remain server responsibilities.</p><div class="row" style="flex-wrap:wrap"><a href="docs/design/STUDIO-SPEC.html">Design rationale ↗</a><a href="docs/SPEC.html">Product spec ↗</a></div></div>`,
    );
  }
  function saveGrant(form) {
    const f = new FormData(form),
      tool = form.dataset.id,
      ids = f.getAll("agents");
    db.agents.forEach((a) => {
      const has = a.tools.includes(tool),
        want = ids.includes(a.id);
      if (has !== want) {
        touch(a);
        a.tools = want ? [...a.tools, tool] : a.tools.filter((t) => t !== tool);
      }
    });
    modal.close();
    render();
    toast("Draft agent tool grants updated locally.");
  }
  document.addEventListener("click", (e) => {
    const b = e.target.closest("[data-action]");
    if (!b) return;
    const d = b.dataset;
    switch (d.action) {
      case "navigate":
        navigate(d.page);
        break;
      case "menu": {
        const open = document.body.classList.toggle("nav-open");
        $(".nav-scrim").hidden = !open;
        $(".mobile-menu").setAttribute("aria-expanded", String(open));
        break;
      }
      case "theme": {
        const theme =
          document.documentElement.dataset.theme === "dark" ? "light" : "dark";
        document.documentElement.dataset.theme = theme;
        try {
          localStorage.setItem("conveyor-studio-theme", theme);
        } catch {}
        break;
      }
      case "profile":
        openDialog(
          "George · tenant admin",
          '<div class="dialog-body stack"><div class="row"><span class="user-avatar" style="display:grid;place-items:center">GD</span><div><h3>George</h3><p class="muted" style="font-size:12px">Person · Non-Agentic AI · admin</p></div></div><div class="info-note">Tenant definitions and human sign-offs are shown with admin access in this prototype. People and agents remain distinct actor kinds.</div></div>',
        );
        break;
      case "about":
        about();
        break;
      case "search":
        search();
        break;
      case "search-go":
        modal.close();
        if (d.page === "project") navigate("project", d.id);
        else if (d.page === "models" && d.id) {
          state.selected.models = d.id;
          navigate("models");
        } else navigate(d.page, d.id);
        break;
      case "close-dialog":
        modal.close();
        break;
      case "create-project":
        createProject();
        break;
      case "open-project":
        navigate("project", d.id);
        break;
      case "project-filter":
        state.projectFilter = d.value;
        render();
        break;
      case "project-tab":
        state.projectTab = d.value;
        render();
        break;
      case "mode":
        state.mode = d.value;
        render();
        break;
      case "node":
        state.node = Number(d.index);
        render();
        break;
      case "inspector-tab":
        state.inspectorTab = d.value;
        render();
        break;
      case "zoom":
        state.zoom = Math.max(60, Math.min(130, state.zoom + Number(d.value)));
        render();
        break;
      case "brief": {
        const p = byId("projects", state.project),
          t = p.typePin || byId("types", p.type);
        openDialog(
          p.name + " · project brief",
          `<div class="dialog-body stack"><div class="row">${badge(p.key)}${badge(t.name, "green")}</div><p>${esc(p.description)}</p><dl class="kv"><dt>Lifecycle</dt><dd>${esc(t.phases[p.phase] || "Queued")}</dd><dt>Type pin</dt><dd>${esc(t.name)}@${t.version}</dd><dt>Process pin</dt><dd>${esc(p.processPin?.name || nameOf("processes", p.process))}@${p.processPin?.version || byId("processes", p.process)?.version}</dd></dl><div class="info-note">A project has independent scope, work packages, decisions and run history. Tenant definitions are reused through pinned versions.</div></div>`,
        );
        break;
      }
      case "phase-info": {
        const p = byId("projects", state.project),
          t = p.typePin || byId("types", p.type),
          i = Number(d.index);
        openDialog(
          t.phases[i],
          `<div class="dialog-body stack">${badge(i < p.phase ? "Completed" : i === p.phase ? "Current phase" : "Upcoming", i === p.phase ? "green" : "")}<p>Defined by ${esc(t.name)}@${t.version}.</p><div class="info-note">A phase transition follows its required evidence and gate. Selecting a lifecycle stage inspects it; the project’s work cards keep their own state.</div></div>`,
          `${btn("Close", "close-dialog")}${btn("Inspect project type", "open-definition", `data-kind="types" data-id="${p.type}"`, "primary")}`,
        );
        break;
      }
      case "add-card":
        addCard();
        break;
      case "card":
        cardDialog(d.id);
        break;
      case "review":
        review(d.id);
        break;
      case "decide":
        decide(d.id, d.value);
        break;
      case "artifact":
        artifact(d.kind);
        break;
      case "download-artifact":
        download(
          d.kind === "patch"
            ? "implementation.patch"
            : "verification-report.json",
          d.kind === "patch"
            ? "// Illustrative Conveyor design patch\n+ invitation.require_unused_token()?;\n"
            : JSON.stringify(
                {
                  illustrative: true,
                  project: "PRJ-014",
                  requirement: "REQ-042@3",
                  passed: 24,
                  failed: 0,
                },
                null,
                2,
              ),
          d.kind === "patch" ? "text/plain" : "application/json",
        );
        toast("Sample downloaded.");
        break;
      case "open-definition":
        modal.close();
        navigate(d.kind, d.id);
        break;
      case "select-entity":
        state.selected[d.kind] = d.id;
        state.catalogTab = "Definition";
        render();
        break;
      case "catalog-tab":
        state.catalogTab = d.value;
        render();
        break;
      case "create-entity":
        editEntity(d.kind);
        break;
      case "edit-entity":
        editEntity(d.kind, d.id);
        break;
      case "delete-entity":
        deleteEntity(d.kind, d.id);
        break;
      case "confirm-delete": {
        const x = byId(d.kind, d.id);
        if (x.status === "Draft") {
          db[d.kind] = db[d.kind].filter((x) => x.id !== d.id);
          syncProcesses();
          state.selected[d.kind] = db[d.kind][0]?.id;
        } else {
          x.status = "Retired";
          x.history = x.history || [];
          x.history.unshift(`v${x.version} · Retired · George`);
        }
        modal.close();
        render();
        toast(
          "Local definition " +
            (x.status === "Retired" ? "retired." : "deleted."),
        );
        break;
      }
      case "publish":
        publish(d.kind, d.id);
        break;
      case "approve-asset": {
        const x = byId(d.kind, d.id);
        x.status = "Approved";
        x.history.unshift(`v${x.version} · Approved · George · person · Web`);
        activity("George", `approved ${x.key}`, "orange");
        render();
        toast("Draft revision approved in this local design study.");
        break;
      }
      case "add-step":
        editStep(d.id);
        break;
      case "edit-step":
        editStep(d.id, Number(d.index));
        break;
      case "delete-step": {
        const p = byId("processes", d.id);
        touch(p);
        p.steps.splice(Number(d.index), 1);
        modal.close();
        render();
        toast("Step removed from the local process draft.");
        break;
      }
      case "step-up": {
        const p = byId("processes", d.id),
          i = Number(d.index);
        if (i > 0) {
          touch(p);
          [p.steps[i - 1], p.steps[i]] = [p.steps[i], p.steps[i - 1]];
          render();
          toast("Draft step order updated.");
        } else toast("This is already the first step.");
        break;
      }
      case "open-tool": {
        modal.close();
        state.tool = d.id;
        state.toolGroup = "All actions";
        state.toolQuery = "";
        navigate("tools");
        break;
      }
      case "select-tool":
        state.tool = d.id;
        render();
        break;
      case "tool-group":
        state.toolGroup = d.value;
        {
          const next = allTools().find(
            (t) =>
              (d.value === "All actions" ||
                (d.value === "Tenant tools"
                  ? t.source === "tenant"
                  : t.group === d.value)) &&
              (
                t.id +
                " " +
                t.title +
                " " +
                t.group +
                " " +
                (t.n8nOperation || "")
              )
                .toLowerCase()
                .includes(state.toolQuery.toLowerCase()),
          );
          if (next) state.tool = next.id;
        }
        render();
        break;
      case "grant-tool":
        grantTool(d.id);
        break;
      case "create-tool":
        editTenantTool();
        break;
      case "edit-tool":
        editTenantTool(d.id);
        break;
      case "delete-tool": {
        const refs = db.agents.filter((a) => a.tools.includes(d.id));
        if (refs.length) {
          openDialog(
            "This tool is still granted",
            `<div class="dialog-body stack"><p>Remove these draft grants before deleting the tenant binding:</p><div class="code">${esc(refs.map((a) => a.name).join("\n"))}</div></div>`,
          );
        } else {
          db.customTools = db.customTools.filter((t) => t.id !== d.id);
          state.tool = allTools()[0].id;
          state.toolGroup = "All actions";
          render();
          toast("Unreferenced tenant tool binding deleted locally.");
        }
        break;
      }
      case "open-model":
        state.selected.models = d.id;
        state.modelElement = null;
        state.modelView = byId("models", d.id)?.views[0]?.id;
        state.modelReport = null;
        state.modelTab = "Elements";
        modal.close();
        navigate("models");
        break;
      case "create-model":
        if (d.project) state.project = d.project;
        editModel();
        break;
      case "edit-model":
        editModel(d.id);
        break;
      case "model-tab":
        state.modelTab = d.value;
        render();
        break;
      case "create-view":
        editView();
        break;
      case "edit-view":
        editView(d.id);
        break;
      case "create-element":
        editElement();
        break;
      case "edit-element":
        editElement(d.id);
        break;
      case "model-element":
        modelElementDialog(d.id);
        break;
      case "open-asset":
        modal.close();
        navigate(
          db.requirements.some((r) => r.id === d.id)
            ? "requirements"
            : "principles",
          d.id,
        );
        break;
      case "create-relationship":
        createRelationship();
        break;
      case "delete-model-part":
        deleteModelPart(d.kind, d.id);
        break;
      case "validate-model":
        validateModel();
        break;
      case "export-svg": {
        const svg = $("#model-svg").cloneNode(true);
        svg.setAttribute("width", "1000");
        svg.setAttribute(
          "height",
          String((1000 * svg.viewBox.baseVal.height) / 600),
        );
        const style = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "style",
        );
        style.textContent =
          "text{fill:#172c2b;font-family:Arial,sans-serif}.el{fill:#fff;stroke:#b8c8bd;stroke-width:1.3}.motivation{fill:#eee9f6;stroke:#cbbfe0}.business{fill:#fff5df;stroke:#d6c69d}.application{fill:#e4f0e8;stroke:#a4c2b0}.technology{fill:#edf1ef;stroke:#b4c2bc}.edge{stroke:#74877b;stroke-width:1.2;fill:none}";
        svg.prepend(style);
        download(
          "conveyor-model-view.svg",
          new XMLSerializer().serializeToString(svg),
          "image/svg+xml",
        );
        toast("Current model view exported as SVG.");
        break;
      }
      case "run-filter":
        state.runFilter = d.value;
        render();
        break;
      case "start-eval":
        if (!state.evalSigned) {
          review("grading");
          error(
            "Start refused: grading sign-off is missing for instrument digest 73e8b4c9. Review the pilot first.",
          );
        } else if (state.evalQueued)
          toast("The local evaluation preview is already queued.");
        else {
          state.evalQueued = true;
          activity(
            "George",
            "queued the contract-parity evaluation preview",
            "orange",
          );
          render();
          toast(
            "Evaluation queued in the local preview. No workers were started.",
          );
        }
        break;
      case "export-usage":
        download(
          "conveyor-usage-sample.json",
          JSON.stringify(
            {
              illustrative: true,
              cache: {
                hit: 108,
                miss: 36,
                write: 8,
                unknown: 4,
                ratio: 108 / 144,
              },
              projects: db.projects.map((p) => ({
                project: p.key,
                cost: p.spent,
              })),
              external_platform_calls_excluded: true,
            },
            null,
            2,
          ),
        );
        toast("Sample usage report downloaded.");
        break;
    }
  });
  document.addEventListener("submit", (e) => {
    const form = e.target;
    if (!form.id) return;
    e.preventDefault();
    switch (form.id) {
      case "entity-form":
        saveEntity(form);
        break;
      case "step-form":
        saveStep(form);
        break;
      case "project-form":
        saveProject(form);
        break;
      case "grant-form":
        saveGrant(form);
        break;
      case "card-form": {
        const f = new FormData(form),
          x = byId("cards", form.dataset.id),
          p = byId("projects", state.project);
        if (x) {
          x.name = String(f.get("name")).trim();
          x.list = String(f.get("list"));
          x.requirement = String(f.get("requirement"));
        } else {
          db.cards.push({
            id: "C-" + (300 + db.cards.length),
            project: p.id,
            name: String(f.get("name")).trim(),
            list: String(f.get("list")),
            requirement: String(f.get("requirement")),
          });
          p.total++;
        }
        modal.close();
        render();
        toast("Work item saved locally.");
        break;
      }
      case "chat-form": {
        const f = new FormData(form),
          message = String(f.get("message") || "").trim();
        if (!message) return;
        state.chat.push({
          project: state.project,
          text: message,
          person: true,
        });
        const p = byId("projects", state.project),
          proc = p.processPin || byId("processes", p.process),
          step = proc.steps[state.node];
        state.chat.push({
          project: state.project,
          person: false,
          text: `This project is ${p.status.toLowerCase()} under ${proc.name}@${proc.version}. The selected step is “${step.name}”. Its actor is ${step.agent ? nameOf("agents", step.agent) : step.kind === "Human gate" ? "a person" : "the procedure"}, and its declared action is ${step.tool}. You can inspect Context for source pins or Tools for its grants. This reply is scripted for the prototype.`,
        });
        render();
        $(".transcript-body")?.scrollTo({ top: 99999 });
        break;
      }
      case "tool-form": {
        const f = new FormData(form),
          id = String(f.get("key")).trim();
        if (!/^[a-zA-Z][\w.-]+$/.test(id))
          return error(
            "Use an action identifier with letters, numbers, dots, underscores or hyphens.",
          );
        const existing = db.customTools.find((t) => t.id === form.dataset.id);
        if (toolOf(id) && !existing)
          return error("This action identifier already exists.");
        const binding = {
          id: existing?.id || id,
          title: String(f.get("name")),
          group: "Tenant tools",
          source: "tenant",
          method: "MCP",
          path: String(f.get("endpoint")),
          role: String(f.get("role")),
          verifies: [],
          fields: [],
        };
        if (existing) Object.assign(existing, binding);
        else db.customTools.push(binding);
        state.tool = id;
        state.toolGroup = "Tenant tools";
        modal.close();
        render();
        toast(
          "Local tool binding registered. Grant it to a specific agent to use it.",
        );
        break;
      }
      case "model-form": {
        const f = new FormData(form),
          m = byId("models", form.dataset.id);
        if (m) {
          m.name = String(f.get("name"));
          m.project = String(f.get("project"));
          mutateModel(m);
        } else {
          const id = uid("model"),
            v = uid("view"),
            lang = String(f.get("language"));
          db.models.push({
            id,
            name: String(f.get("name")),
            project: String(f.get("project")),
            language: lang,
            status: "Draft",
            version: 1,
            elements: [],
            relationships: [],
            views: [
              {
                id: v,
                name: "Initial view",
                kind: lang === "UML" ? "class" : "archimate_view",
                elements: [],
              },
            ],
          });
          state.selected.models = id;
          state.modelView = v;
        }
        modal.close();
        navigate("models");
        toast("Local model saved. Add elements, relationships and views.");
        break;
      }
      case "view-form": {
        const f = new FormData(form),
          m = byId("models", state.selected.models),
          existing = m.views.find((v) => v.id === form.dataset.id),
          v = {
            id: existing?.id || uid("view"),
            name: String(f.get("name")),
            kind: String(f.get("kind")),
            elements: f.getAll("elements"),
          };
        if (existing) Object.assign(existing, v);
        else m.views.push(v);
        mutateModel(m);
        state.modelView = v.id;
        modal.close();
        render();
        toast("Model view saved. It references the existing elements.");
        break;
      }
      case "element-form": {
        const f = new FormData(form),
          m = byId("models", state.selected.models),
          existing = m.elements.find((e) => e.id === form.dataset.id),
          el = {
            id: existing?.id || uid("element"),
            name: String(f.get("name")),
            kind: String(f.get("kind")),
            documentation: String(f.get("documentation")),
            asset: String(f.get("asset")),
          };
        if (existing) Object.assign(existing, el);
        else {
          m.elements.push(el);
          const v = m.views.find((v) => v.id === state.modelView) || m.views[0];
          v.elements.push(el.id);
        }
        mutateModel(m);
        modal.close();
        render();
        toast("Element saved in the model and current view.");
        break;
      }
      case "relationship-form": {
        const f = new FormData(form),
          m = byId("models", state.selected.models),
          source = String(f.get("source")),
          target = String(f.get("target"));
        if (source === target)
          return error("Choose different source and target elements.");
        m.relationships.push({
          id: uid("relationship"),
          source,
          target,
          kind: String(f.get("kind")),
        });
        mutateModel(m);
        modal.close();
        render();
        toast("Relationship created in the local model.");
        break;
      }
    }
  });
  document.addEventListener("input", (e) => {
    const input = e.target;
    if (input.id === "entity-search") {
      state.entityQuery = input.value;
      const start = input.selectionStart;
      const y = window.scrollY;
      render();
      $("#entity-search")?.focus();
      $("#entity-search")?.setSelectionRange(start, start);
      window.scrollTo(0, y);
    }
    if (input.id === "tool-search") {
      state.toolQuery = input.value;
      const start = input.selectionStart,
        y = window.scrollY;
      render();
      $("#tool-search").focus();
      $("#tool-search").setSelectionRange(start, start);
      window.scrollTo(0, y);
    }
  });
  document.addEventListener("change", (e) => {
    const el = e.target;
    if (el.dataset.matrix) {
      const t = byId("types", el.dataset.type);
      touch(t);
      t.matrix = t.matrix || {};
      t.matrix[el.dataset.matrix] = el.checked;
      render();
      toast(
        "Phase mapping saved as a local draft. Existing project pins stay unchanged.",
      );
    }
    if (el.id === "model-view") {
      state.modelView = el.value;
      state.modelElement = null;
      render();
    }
  });
  document.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      search();
    }
    if (
      e.target.matches(".model-node") &&
      (e.key === "Enter" || e.key === " ")
    ) {
      e.preventDefault();
      modelElementDialog(e.target.dataset.id);
    }
    if (e.key === "Escape" && document.body.classList.contains("nav-open")) {
      document.body.classList.remove("nav-open");
      $(".nav-scrim").hidden = true;
      $(".mobile-menu").setAttribute("aria-expanded", "false");
    }
  });
  let draggedCard;
  document.addEventListener("dragstart", (e) => {
    const card = e.target.closest("[data-card]");
    if (card) {
      draggedCard = card.dataset.card;
      e.dataTransfer.setData("text/plain", draggedCard);
      e.dataTransfer.effectAllowed = "move";
    }
  });
  document.addEventListener("dragover", (e) => {
    if (e.target.closest("[data-list]") && draggedCard) e.preventDefault();
  });
  document.addEventListener("drop", (e) => {
    const list = e.target.closest("[data-list]");
    if (list && draggedCard) {
      e.preventDefault();
      const card = byId("cards", draggedCard);
      if (card) card.list = list.dataset.list;
      draggedCard = null;
      render();
      toast("Work card moved locally. Approval state is unchanged.");
    }
  });
  modal.addEventListener("click", (e) => {
    if (e.target === modal) {
      const r = modal.getBoundingClientRect();
      if (
        e.clientX < r.left ||
        e.clientX > r.right ||
        e.clientY < r.top ||
        e.clientY > r.bottom
      )
        modal.close();
    }
  });
  try {
    document.documentElement.dataset.theme =
      localStorage.getItem("conveyor-studio-theme") || "light";
  } catch {}
  const studioApi = {
    db,
    state,
    esc,
    icon,
    badge,
    btn,
    navButton,
    header,
    navigate,
    render,
    openDialog,
    dialogForm,
    field,
    selectField,
    error,
    toast,
    activity,
    byId,
    nameOf,
    cataloguePage,
    toolPage,
    governancePage,
    usagePage,
    coreTools,
    toolOf,
    download,
  };
  const enterprise = window.installEnterprise(studioApi);
  const learning = window.installLearning(studioApi, enterprise);
  Object.assign(enterprise.pages, learning.pages);
  Object.assign(enterprise.titles, learning.titles);
  // SPEC_V2 pack: owns the sidebar and the organisation, plan, authoring, operations,
  // tools and metering destinations. It composes the existing editors instead of forking them.
  const v2Pack = window.installV2 ? window.installV2(studioApi, enterprise) : null;
  if (v2Pack) {
    Object.assign(enterprise.pages, v2Pack.pages);
    Object.assign(enterprise.titles, v2Pack.titles);
    enterprise.sidebar = v2Pack.sidebar;
  }
  // Keep in-flight project definitions immutable while the tenant edits its next version.
  db.projects.forEach((p) => {
    p.typePin = structuredClone(byId("types", p.type));
    p.processPin = structuredClone(byId("processes", p.process));
  });
  const repairVerifier = byId("agents", "verifier");
  repairVerifier.tools.push("exec_in_workspace_async", "get_exec_job_status");
  db.projects.forEach((p) => {
    p.agentPins = Object.fromEntries(
      [
        ...new Set([
          ...p.agents,
          ...p.processPin.steps.map((s) => s.agent).filter(Boolean),
        ]),
      ].map((id) => [id, structuredClone(byId("agents", id))]),
    );
  });
  // Allow direct links into the prototype's information architecture.
  const initial = location.hash.slice(1);
  if (
    [
      "overview",
      "projects",
      "types",
      "capabilities",
      "processes",
      "agents",
      "tools",
      "requirements",
      "principles",
      "models",
      "decisions",
      "runs",
      "evals",
      "usage",
      ...Object.keys(enterprise.pages),
    ].includes(initial)
  )
    state.page = initial;
  render();
})();
