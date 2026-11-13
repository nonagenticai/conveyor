/* SPEC_V2 pack — the organisational operating model.
   Local interaction study only: every record below is a fixture, no backend is called,
   and every V2 destination is marked PROPOSED because SPEC_V2 is a draft with no V2 code. */
"use strict";
window.installV2 = function (api, enterprise) {
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
    toast,
    byId,
    nameOf,
    cataloguePage,
    toolPage,
    governancePage,
    usagePage,
  } = api;

  const $ = (q, r = document) => r.querySelector(q);
  const route = (label, page, cls = "", symbol = "arrow") =>
    btn(label, "navigate", `data-page="${page}"`, cls, symbol);
  const button = (label, action, attrs = "", cls = "", symbol = "") =>
    `<button type="button" class="btn ${cls}" data-v2="${action}" ${attrs}>${symbol ? I(symbol) : ""}${E(label)}</button>`;
  const panel = (title, body, right = "") =>
    `<section class="e-panel"><div class="e-panel-head"><h2>${title}</h2>${right}</div><div class="v2-panel-body">${body}</div></section>`;
  const note = (body, cls = "") => `<div class="v2-gate ${cls}">${body}</div>`;
  const tabs = (values, active, key) =>
    `<div class="tabs e-tabs" role="tablist">${values
      .map(
        (t) =>
          `<button type="button" class="btn e-tab ${t === active ? "selected" : ""}" data-v2="tab" data-key="${key}" data-value="${E(t)}" aria-pressed="${t === active}" role="tab">${E(t)}</button>`,
      )
      .join("")}</div>`;
  const table = (heads, rows) =>
    `<div class="e-table-scroll"><table class="e-table"><thead><tr>${heads
      .map((h) => `<th>${h}</th>`)
      .join("")}</tr></thead><tbody>${rows.join("")}</tbody></table></div>`;
  const kv = (pairs) =>
    `<dl class="kv">${pairs
      .map(([a, b]) => `<dt>${E(a)}</dt><dd>${b}</dd>`)
      .join("")}</dl>`;
  const money = (n) =>
    "$" + n.toLocaleString("en-US", { maximumFractionDigits: 2 });
  /* ------------------------------------------------------------- live snapshot */
  // `data/snapshot.js` is written by `conveyor snapshot`. Absent on a clean checkout, the pack
  // is the offline DESIGN study. Present, the rows the platform actually owns replace the
  // fixtures they cover, and the pages with no live source keep the DESIGN ribbon.
  const S = window.CONVEYOR_SNAPSHOT;
  const live = S && S.schema === "design-snapshot/v1" ? S : null;
  const list = (value) => (Array.isArray(value) ? value : []);
  const short = (value, n = 12) => String(value == null ? "" : value).slice(0, n);
  const count = {
    industries: 0,
    departments: 0,
    jobs: 0,
    releases: 0,
    bindings: 0,
    programmes: 0,
    projects: 0,
    calls: 0,
  };
  const liveSrc = live
    ? `LIVE · snapshot ${E(short(live.payload_sha256))} · ${E(live.generated_at || "")} · ${E(short(live.git_sha, 7))}`
    : "SPEC_V2 draft · local fixture";
  const ribbon = (scope, isLive = false) =>
    isLive && live
      ? `<div class="v2-ribbon live" role="note"><strong>LIVE</strong><span>${scope}</span><span class="v2-src">${liveSrc}</span></div>`
      : `<div class="v2-ribbon" role="note"><strong>V2 · PROPOSED</strong><span>${scope}</span><span class="v2-src">SPEC_V2 draft · local fixture</span></div>`;
  const heading = (title, sub, actions = "", area = "SPEC_V2 OPERATING MODEL") =>
    H(title, sub, actions, "NON-AGENTIC AI / " + area);
  const effect = (cls, label = cls) =>
    `<span class="v2-effect ${cls}">${E(label)}</span>`;
  const effects = (list) =>
    `<div class="v2-effects">${list.map((x) => effect(x)).join("")}</div>`;
  const flow = (nodes) =>
    `<div class="v2-flow">${nodes
      .map(
        (n, i) =>
          `${i ? `<span class="v2-arrow">${I("arrow")}</span>` : ""}<div><strong>${E(n[0])}</strong><br><small>${E(n[1] || "")}</small></div>`,
      )
      .join("")}</div>`;
  const stale = (subject) =>
    `<div class="v2-code">409 ${subject}\nThe binding pins release 3f9c…a1. The requested release 7d20…c4 is not this binding's pin. No model call was made.</div>`;

  // External feedback is a GitHub feature, not a service: the page links Discussions and a
  // prefilled issue, and the feedback issues themselves are published in the snapshot.
  const reportUrl = () => {
    const base = String((live && live.feedback_url) || "https://github.com/nonagenticai/conveyor")
      .replace(/\/+$/, "")
      .replace(/\/discussions$/, "");
    const page = (state && state.page) || "design";
    const title = encodeURIComponent("Design feedback: " + page);
    const body = encodeURIComponent(
      "Page: " + page +
        "\nSnapshot: " + ((live && live.payload_sha256) || "") +
        "\nGenerated: " + ((live && live.generated_at) || "") +
        "\nGit: " + ((live && live.git_sha) || "") +
        "\nURL: " + (typeof location !== "undefined" ? location.href : "") +
        "\n\nWhat I saw:\n\nWhat I expected:\n",
    );
    return base + "/issues/new?labels=feedback&title=" + title + "&body=" + body;
  };
  const dataPanel = () => {
    if (!live) return "";
    const feedback = list((live.programme || {}).feedback);
    const files = [
      "manifest.json",
      "programme.json",
      "organisation.json",
      "config.json",
      "work.json",
      "artefacts.json",
      "measure.json",
      "secrets.json",
      "evidence.json",
      "feedback.json",
    ];
    return panel(
      "Live data &amp; feedback",
      `<p class="muted">Rendered from the conveyor's own rows. Snapshot <span class="mono">${E(short(live.payload_sha256))}</span> generated <strong>${E(live.generated_at || "")}</strong> at <span class="mono">${E(short(live.git_sha, 12))}</span>. The raw files are public:</p>${table(
        ["Data file"],
        files.map(
          (file) =>
            `<tr><td><a class="e-project-link" href="data/${file}" target="_blank" rel="noopener"><span class="mono">${file}</span></a></td></tr>`,
        ),
      )}${feedback.length ? table(
        ["Feedback", "State", "Link"],
        feedback.map(
          (issue) =>
            `<tr><td>${E(issue.title)}</td><td>${B(issue.state === "OPEN" ? "Open" : "Closed", issue.state === "OPEN" ? "amber" : "green")}</td><td><a href="${E(issue.url)}" target="_blank" rel="noopener">#${E(issue.number)} ↗</a></td></tr>`,
        ),
      ) : note("No external feedback issues yet.")}<p class="muted">Document index: <a class="e-project-link" href="docs/documents.json" target="_blank" rel="noopener">documents.json</a> · <a class="e-project-link" href="docs/SPEC_V2.html">SPEC_V2 as HTML</a></p><div class="toolbar"><a class="btn" href="${E(live.feedback_url || "https://github.com/nonagenticai/conveyor/discussions")}" target="_blank" rel="noopener">Feedback &amp; discussion ↗</a><a class="btn" href="${E(reportUrl())}" target="_blank" rel="noopener">Report this screen ↗</a></div>`,
    );
  };

  const v = {
    tab: {},
    programme: "civic",
    job: "backend",
    department: "all",
    binding: "bind-backend",
    decision: "route",
    toolkit: "coding",
    incident: "INC-041",
    kata: "K-07",
    sop: 0,
    intake: "chat",
    intakeId: "IN-31",
    draftId: "AD-13",
    brief: "",
    packReady: false,
    pkoVersion: 3,
    pkoTerm: "PKO-T01",
    pkoKind: "all",
    pkoQuery: "",
    pkoFormal: false,
    explainMissing: false,
    explainRetired: false,
    lesson: "LSN-15",
    runbook: "RB-pod-crashloop",
    callId: "TC-9001",
    andon: true,
    meterLevel: "tenant",
    probe: { coding: "green", scientific: "green" },
    cycle: false,
    effectCeiling: "write",
    review: null,
  };

  /* ------------------------------------------------------------------ fixtures */
  const F = {
    industries: [
      { id: "software", name: "software", tenant: "nonagentic", note: "first tenant — IT development and operations" },
      { id: "hta", name: "health-technology-assessment", tenant: "AHTAPol", note: "own tenant, own departments" },
      { id: "media", name: "media", tenant: "w2v", note: "own tenant, own departments" },
    ],
    departments: [
      { id: "delivery", name: "delivery", primary: true, jobs: 7, first: "conveyor maintenance, feature delivery", region: "none" },
      { id: "verification", name: "verification", primary: false, jobs: 3, first: "independent verification, eval authoring" },
      { id: "operations", name: "operations", primary: false, jobs: 3, first: "k8s/git incidents, runbooks, postmortems" },
      { id: "project office", name: "project office", primary: false, jobs: 3, first: "charters, WBS, baselines, cadence" },
      { id: "improvement", name: "improvement", primary: false, jobs: 3, first: "interruptions, systemic fixes, instruments" },
    ],
    jobs: [
      {
        id: "backend",
        key: "software.delivery.backend_engineer",
        name: "Backend engineer",
        department: "delivery",
        family: "engineering",
        charter: "Implement bounded backend changes; prove them with tests; propose a reviewable patch.",
        capabilities: ["delivery"],
        procedure: "proc-implement",
        executor: "internal_agent",
        model: "router/medium",
        recipe: "implementation_role@4",
        knowledge: "base → software → delivery → backend",
        memory: "work-package hot + one digest",
        grants: ["coding_jobs.create", "coding_jobs.get", "test_runs.create"],
        binding: "toolkit coding@4 · ceiling write · credential coding.tenant",
        limits: "12 turns · 40k tokens · $2.50",
        gates: "design (conditional) · merge · deploy",
        authority: "implement, test, propose PR",
        status: "Published",
        release: "rel-backend-4",
        metrics: ["first-pass yield 82%", "rework 11%", "cost per accepted artefact $1.94"],
      },
      {
        id: "architect",
        key: "software.delivery.architect",
        name: "Architect",
        department: "delivery",
        family: "architecture",
        charter: "Turn intent into architecture contracts and traceable decisions.",
        capabilities: ["architecture"],
        procedure: "proc-arch",
        executor: "internal_agent",
        model: "router/high",
        recipe: "architecture_role@3",
        knowledge: "base → software → delivery → architecture",
        memory: "work-package hot + one digest",
        grants: ["models.create", "trace_links.create"],
        binding: "toolkit strata@2 · ceiling read · credential strata_rag.tenant",
        limits: "16 turns · 60k tokens · $4.00",
        gates: "design · architecture contract · closure",
        authority: "draft contracts and ADRs; no merge",
        status: "Published",
        release: "rel-architect-3",
        metrics: ["contract acceptance 76%", "phase gate latency 2.1 d"],
      },
      {
        id: "pm",
        key: "software.project_office.pm",
        name: "PM",
        department: "project office",
        family: "product",
        charter: "Turn a request into a governed project; sequence work; route gates; close with evidence.",
        capabilities: ["project_management"],
        procedure: "proc-pm",
        executor: "procedure",
        model: "laya + router/small",
        recipe: "pm_charter@2",
        knowledge: "base → software → project office",
        memory: "portfolio digest + project hot",
        grants: ["projects.create", "wbs.write", "approvals.request"],
        binding: "toolkit billing@1 · ceiling read · credential billing.tenant",
        limits: "8 turns · 20k tokens · $1.00",
        gates: "charter · baseline · change · closure",
        authority: "create/close project; never sole approver",
        status: "Published",
        release: "rel-pm-2",
        metrics: ["gate latency 1.8 d", "baseline churn 6%"],
      },
      {
        id: "coach",
        key: "software.improvement.kanban_coach",
        name: "Kanban coach",
        department: "improvement",
        family: "improvement",
        charter: "Protect flow; one interruption becomes exactly one coach card; bounded remedy or escalation.",
        capabilities: ["improvement"],
        procedure: "proc-coach",
        executor: "procedure",
        model: "laya",
        recipe: "coach_triage@3",
        knowledge: "base → software → improvement",
        memory: "board hot + interruption digest",
        grants: ["cards.create", "cards.move", "coach.triage", "coach.remedy"],
        binding: "toolkit coding@4 · ceiling read · credential coding.tenant",
        limits: "closed sets · no LLM remedy",
        gates: "escalation only",
        authority: "triage + remedy within the closed set",
        status: "Published",
        release: "rel-coach-3",
        metrics: ["blocks closed 64%", "duplicate coach cards 0"],
      },
      {
        id: "sre",
        key: "software.operations.sre",
        name: "SRE / on-call",
        department: "operations",
        family: "operations",
        charter: "Detect, triage, mitigate and learn from incidents in the conveyor and its dependencies.",
        capabilities: ["operations"],
        procedure: "proc-incident",
        executor: "internal_agent",
        model: "router/medium",
        recipe: "incident_role@2",
        knowledge: "base → software → operations",
        memory: "incident hot + postmortem digest",
        grants: ["ops.k8s_read", "ops.k8s_restart", "ops.git_read"],
        binding: "toolkit ops@5 · ceiling destructive · credential ops.namespace",
        limits: "10 turns · 30k tokens · $3.00",
        gates: "destructive action · postmortem",
        authority: "read-first; write behind a gate",
        status: "Published",
        release: "rel-sre-2",
        metrics: ["MTTR 42 m", "never-refused gates 0"],
      },
    ],
    releases: [
      { id: "rel-backend-4", job: "backend", hash: "3f9c1d7a8b42a1", status: "Published", at: "2026-11-01", note: "knowledge pins, tool grants and limits frozen" },
      { id: "rel-backend-5", job: "backend", hash: "7d20e4f19c4c4", status: "Draft", at: "2026-11-11", note: "new tool grant; requires a publish gate" },
      { id: "rel-architect-3", job: "architect", hash: "a8120f3e91b7", status: "Published", at: "2026-10-28", note: "contract review gate" },
      { id: "rel-pm-2", job: "pm", hash: "c41d7e8820aa", status: "Published", at: "2026-10-19", note: "charter and baseline procedure" },
    ],
    bindings: [
      { id: "bind-backend", job: "backend", release: "rel-backend-4", executor: "internal_agent", capability: "delivery", procedure: "proc-implement", status: "Active", tenant: "nonagentic", note: "pins release 3f9c…a1; new releases are new bindings" },
      { id: "bind-architect", job: "architect", release: "rel-architect-3", executor: "internal_agent", capability: "architecture", procedure: "proc-arch", status: "Active", tenant: "nonagentic" },
      { id: "bind-pm", job: "pm", release: "rel-pm-2", executor: "procedure", capability: "project_management", procedure: "proc-pm", status: "Active", tenant: "nonagentic" },
      { id: "bind-legacy", job: "backend", release: "rel-backend-3", executor: "internal_agent", capability: "delivery", procedure: "proc-implement", status: "Retired", tenant: "nonagentic", note: "history keeps its pin" },
    ],
    authority: [
      ["Create / close project", "✅", "—", "—", "—", "—", "✅"],
      ["Create / move card", "✅", "✅ remedy only", "—", "own card", "own incident", "✅"],
      ["Assign a job binding", "✅", "—", "—", "—", "—", "✅"],
      ["Approve a phase gate", "—", "—", "—", "—", "—", "✅ dual"],
      ["Approve a merge", "—", "—", "—", "—", "—", "✅"],
      ["Open a kata", "✅", "✅", "—", "—", "—", "✅"],
      ["Adopt a kata change", "—", "—", "propose", "—", "—", "✅"],
      ["Destructive ops action", "—", "—", "—", "—", "propose + dry-run", "✅ dual"],
      ["Publish a procedure", "—", "—", "—", "—", "—", "✅ or gated write"],
    ],
    approvalPolicies: [
      { gate: "Phase gate", decisions: 2, approver: "human", expiry: "never", note: "dual control; assignee cannot approve" },
      { gate: "Merge", decisions: 1, approver: "human", expiry: "14 d", note: "green CI is evidence, not approval" },
      { gate: "Deploy", decisions: 1, approver: "human", expiry: "24 h", note: "rollback plan required" },
      { gate: "Destructive ops", decisions: 2, approver: "human", expiry: "4 h", note: "dry-run + read-back + audit event" },
      { gate: "Kata adoption", decisions: 1, approver: "human", expiry: "never", note: "the kata cannot approve its own adoption" },
      { gate: "Tool publication", decisions: 1, approver: "human", expiry: "30 d", note: "security review for destructive tools" },
    ],
    decisionPolicies: [
      { id: "route", case: "Request classification", type: "choice", options: "new_feature · bug · question · incident · improvement", threshold: 0.72, fail: "closed → PM", action: "handover", record: "answer + confidence + policy v3" },
      { id: "type", case: "Project type", type: "choice", options: "delivery · architecture · improvement · development · operations", threshold: 0.75, fail: "closed → PM", action: "handover", record: "policy v2" },
      { id: "job", case: "Job / capability routing", type: "choice", options: "the tenant's active bindings for the department", threshold: 0.78, fail: "closed → coach", action: "handover", record: "refinement v3" },
      { id: "triage", case: "Coach triage / remedy", type: "choice", options: "transient · conveyor_defect · job_defect · project_decision · external; retry · move · repull · block · ask · note", threshold: 0.7, fail: "open → ask", action: "annotate", record: "policy v4" },
      { id: "severity", case: "Severity", type: "choice", options: "sev1 · sev2 · sev3 · sev4", threshold: 0.8, fail: "closed → on-call", action: "handover", record: "policy v2" },
      { id: "precheck", case: "Gate pre-check", type: "choice", options: "pass · review · refuse", threshold: 0.85, fail: "closed → human gate", action: "block", record: "policy v5" },
    ],
    refinements: [
      { id: "ref-route", policy: "route", tenant: "nonagentic", change: "threshold 0.72 → 0.78", fail: "closed → PM", laya: "laya@4", version: 3, gate: "policy change gate" },
      { id: "ref-triage", policy: "triage", tenant: "nonagentic", change: "fail action annotate → ask", fail: "open → human", laya: "laya@4", version: 2, gate: "policy change gate" },
    ],
    laya: { model: "jev-latest", kind: "laya", params: "calibrated · temperature 0", timeout: "1500 ms default / 3 s hard", version: 4, endpoint: "POST /v1/systemone", note: "~17–21 ms measured on pgx; unavailable is gate_unavailable, never a pass" },
    decisions: [
      { id: "dec-1", case: "Request classification", answer: "new_feature", confidence: 0.91, probs: "0.91 · 0.04 · 0.03 · 0.01 · 0.01", policy: "route v3", refine: "ref-route v3", laya: "laya@4", latency: "19 ms", hash: "sha256:2f7e…", outcome: "accepted" },
      { id: "dec-2", case: "Job / capability routing", answer: "backend_engineer", confidence: 0.74, probs: "0.74 · 0.16 · 0.10", policy: "job v2", refine: "ref-route v3", laya: "laya@4", latency: "21 ms", hash: "sha256:91ac…", outcome: "below threshold → coach" },
      { id: "dec-3", case: "Coach triage", answer: "conveyor_defect / retry", confidence: 0.68, probs: "0.68 · 0.19 · 0.13", policy: "triage v4", refine: "ref-triage v2", laya: "laya@4", latency: "18 ms", hash: "sha256:4d0b…", outcome: "open → ask human" },
      { id: "dec-4", case: "Gate pre-check", answer: "review", confidence: 0.87, probs: "0.07 · 0.87 · 0.06", policy: "precheck v5", refine: "—", laya: "laya@4", latency: "20 ms", hash: "sha256:aa31…", outcome: "human gate opened" },
    ],
    programmes: [
      { id: "civic", key: "PGM-001", name: "Connected public services", cadence: "PI-2026.4", budget: "€48.6m", tone: "amber", note: "identity, data and service transformation" },
      { id: "resilience", key: "PGM-002", name: "Platform resilience", cadence: "PI-2026.4", budget: "€19.2m", tone: "green", note: "infrastructure, security and continuity" },
      { id: "operating", key: "PGM-003", name: "Adaptive operating model", cadence: "PI-2026.4", budget: "€12.4m", tone: "green", note: "organisation, knowledge and agent operations" },
    ],
    projectDeps: [
      { from: "identity", to: "portal", kind: "blocks", status: "open", note: "identity contract gates the portal release" },
      { from: "billing", to: "identity", kind: "needs", status: "open", note: "billing reads the tenant identity contract" },
      { from: "insights", to: "portal", kind: "informs", status: "accepted", note: "usage evidence informs scope" },
    ],
    backlog: [
      { id: "BI-11", item: "Identity contract v2", value: 8, criticality: 5, risk: 3, size: 5, programme: "civic", status: "Ready" },
      { id: "BI-12", item: "Portal delegation gate", value: 5, criticality: 3, risk: 2, size: 2, programme: "civic", status: "Ready" },
      { id: "BI-13", item: "Billing reconciliation", value: 5, criticality: 2, risk: 3, size: 3, programme: "civic", status: "Stale" },
      { id: "BI-14", item: "Eval held-out refresh", value: 3, criticality: 4, risk: 5, size: 2, programme: "operating", status: "Ready" },
    ],
    wbs: [
      { id: "1.1", kind: "Phase", name: "Charter & baseline", accept: "charter approved · baseline B3 frozen", evidence: "GA-31 · approval A-88", state: "Accepted" },
      { id: "1.2", kind: "Deliverable", name: "Identity contract", accept: "architecture contract · traced requirements", evidence: "model M-4 · REQ-042@3", state: "Accepted" },
      { id: "1.2.3", kind: "Work package", name: "Harvest identity baseline architecture", accept: "7 artefacts · review", evidence: "WP-2048 · run RUN-151", state: "In review" },
      { id: "1.2.4", kind: "Work package", name: "Approve delegated identity requirements", accept: "baseline review", evidence: "WP-2049 · gate pending", state: "Blocked" },
      { id: "1.3", kind: "Milestone", name: "PI-2026.4 demo", accept: "each deliverable exhibits evidence", evidence: "demo D-14", state: "Planned" },
    ],
    baseline: { id: "B3", frozen: "2026-10-22", scope: "42 deliverables", schedule: "2026-12-18", cost: "€48.6m", approved: "George · person · Web", state: "Approved" },
    registers: [
      { id: "RSK-07", kind: "risk", title: "Identity contract decision overdue", owner: "architect", status: "Open", severity: "High", note: "blocks portal release" },
      { id: "ISS-21", kind: "issue", title: "Billing reconciliation mismatch", owner: "pm", status: "Mitigating", severity: "Medium", note: "opens an incident per metering policy" },
      { id: "ASM-04", kind: "assumption", title: "Tenant identity stays within programme scope", owner: "pm", status: "Accepted", severity: "Low", note: "review at gate C" },
      { id: "DEP-09", kind: "dependency", title: "Strata pack v17 availability", owner: "architect", status: "Open", severity: "Medium", note: "required knowledge pin" },
      { id: "BEN-02", kind: "benefit", title: "Fulfilment time −40%", owner: "pm", status: "Tracking", severity: "—", note: "planned vs realised at closure" },
    ],
    changes: [
      { id: "CR-018", title: "Add delegated identity scope", baseline: "B3", impact: "scope +2 deliverables · schedule +9 d · cost +€0.8m · risk medium", rationale: "", state: "Draft" },
      { id: "CR-017", title: "Move portal gate to PI-2026.4", baseline: "B2", impact: "schedule −14 d · cost −€0.2m · risk low", rationale: "Programme board rationale recorded", state: "Approved" },
    ],
    sops: [
      { id: "SOP-01", name: "Tenant onboarding SOP", source: "PDF · tenant-supplied", pages: 18, state: "Draft PKO + PDDL", provenance: "untrusted input · governed_asset(sop_source)" },
      { id: "SOP-02", name: "Release verification SOP", source: "URL · internal wiki", pages: 9, state: "Review", provenance: "untrusted input · provenance recorded" },
      { id: "SOP-03", name: "Incident response SOP", source: "Document · batch (3)", pages: 24, state: "Published", provenance: "reviewed · procedure v2 · pack capability@17" },
    ],
    pko: {
      release: "pko@3",
      entities: [
        { id: "PKO-T01", name: "verify_release", kind: "task", attrs: "requires evidence", synonyms: "release check" },
        { id: "PKO-T02", name: "approve_baseline", kind: "task", attrs: "dual control", synonyms: "baseline approval" },
        { id: "PKO-R01", name: "release manager", kind: "role", attrs: "accountable", synonyms: "RM" },
        { id: "PKO-A01", name: "release evidence", kind: "artefact", attrs: "run + eval", synonyms: "proof" },
        { id: "PKO-C01", name: "no unverified release", kind: "constraint", attrs: "hard", synonyms: "gate" },
      ],
      relations: [
        ["PKO-T01", "requires", "PKO-A01", "1..*"],
        ["PKO-R01", "performs", "PKO-T01", "1"],
        ["PKO-T02", "constrained_by", "PKO-C01", "1"],
      ],
    },
    intakes: [
      { id: "IN-31", kind: "Chat", text: "We need a small IT delivery department with PM, backend and verification jobs.", produces: ["organisation draft", "job drafts", "project template"], state: "Drafted" },
      { id: "IN-32", kind: "Document", text: "Tenant SOP bundle (3 files)", produces: ["capability", "procedure", "PKO rows", "knowledge pack"], state: "In review" },
      { id: "IN-33", kind: "Chat", text: "Add a tool request for a missing billing read operation.", produces: ["tool request", "tool draft"], state: "Blocked on dependency" },
    ],
    drafts: [
      { id: "AD-11", asset: "Department: delivery", depends: "industry software", state: "Draft", review: "human" },
      { id: "AD-12", asset: "Job: backend engineer", depends: "department delivery · capability delivery", state: "Draft", review: "human" },
      { id: "AD-13", asset: "Procedure: implement change", depends: "job draft · PKO terms", state: "Review", review: "human" },
      { id: "AD-14", asset: "Knowledge pack: identity", depends: "procedure draft", state: "Unpublished", review: "human" },
    ],
    katas: [
      {
        id: "K-07",
        title: "Reduce rework on identity contract changes",
        instrument: "eval architecture-v3 · held-out 80 cases",
        threshold: "≥8 pp above baseline, outside the 6.5 pp noise band",
        prediction: "first-pass yield +10 pp; rework −1.2 h per card",
        tenant: "kata (own tenant)",
        state: "Measuring",
        six: [
          ["Go to See", "Observe 12 runs and 4 blocked cards; evidence linked."],
          ["Grasp the Situation", "Problem: contract edits are not re-validated before review."],
          ["Get to Solution", "Candidate: add a validation step to procedure v8."],
          ["Get to Standardization", "Pending: procedure/prompt update after measurement."],
          ["Get to Sustainability", "Pending: monitoring and regression guard."],
          ["Get to Stretch", "Pending: yokoten to the architect job."],
        ],
      },
      {
        id: "K-06",
        title: "Cost hillclimb on prompt ordering",
        instrument: "inference ledger + prompt audit",
        threshold: "cost −15% at equal quality",
        prediction: "cache-hit ratio +20 pp",
        tenant: "kata (own tenant)",
        state: "Adoption review",
        six: [
          ["Go to See", "Audit found cache-hostile ordering."],
          ["Grasp the Situation", "Stable prefix is repeated after dynamic text."],
          ["Get to Solution", "Reorder source classes."],
          ["Get to Standardization", "Prompt recipe v5 prepared."],
          ["Get to Sustainability", "Regression guard on cache hit ratio."],
          ["Get to Stretch", "Apply to all internal-agent jobs."],
        ],
      },
    ],
    lessons: [
      { id: "LSN-14", title: "Validate contract edits before review", evidence: ["run RUN-151", "run RUN-158"], independent: 2, state: "Promote", review: "George · person" },
      { id: "LSN-15", title: "Use smaller model for routing", evidence: ["run RUN-160"], independent: 1, state: "Blocked", review: "needs a second independent ref" },
    ],
    adoptions: [
      { id: "ADP-03", kata: "K-06", change: "prompt recipe v5", measured: "cost −11% · quality within band", gate: "human adoption", state: "Pending" },
      { id: "ADP-02", kata: "K-05", change: "procedure v7", measured: "quality +4 pp", gate: "human adoption", state: "Reverted" },
    ],
    incidents: [
      { id: "INC-041", service: "conveyor-api", severity: "sev2", detected: "2026-11-11 09:14", mitigated: "09:31", resolved: "10:02", state: "Postmortem due", symptom: "pod crash-loop after rollout", runbook: "RB-pod-crashloop" },
      { id: "INC-042", service: "postgres-primary", severity: "sev1", detected: "2026-11-10 22:07", mitigated: "22:19", resolved: "23:40", state: "Closed", symptom: "connection saturation", runbook: "RB-db-down" },
    ],
    runbooks: [
      { id: "RB-pod-crashloop", name: "Pod crash-loop", read: "pods · events · logs", write: "rollout restart (destructive)", gate: "dry-run + dual control", tier: "runbook" },
      { id: "RB-db-down", name: "Dependency / DB down", read: "pods · readiness · logs", write: "restart dependency (destructive)", gate: "dry-run + dual control", tier: "runbook" },
      { id: "RB-cert", name: "Certificate expiry", read: "certificates · ingress", write: "rotate certificate (destructive)", gate: "dry-run + dual control", tier: "runbook" },
      { id: "RB-backlog", name: "Queue backlog", read: "queue depth · workers", write: "scale workers (destructive)", gate: "dry-run + dual control", tier: "runbook" },
    ],
    postmortems: [
      { id: "PM-19", incident: "INC-041", state: "Draft", finding: "rollout lacked a read-back check", outcome: "one-off → runbook improvement" },
      { id: "PM-18", incident: "INC-042", state: "Published", finding: "connection pool exhaustion", outcome: "systemic → kata candidate K-07" },
    ],
    toolkits: [
      { key: "coding", version: 4, service: "coding.example.com", status: "live", probe: "green", ops: 11, licence: "coding", note: "gateway, analysis, containers, github, jobs, realtime, repo-jobs, security, test-runs, testing, ai" },
      { key: "strata", version: 2, service: "strata_rag + strata_ingest", status: "live", probe: "green", ops: 6, licence: "strata", note: "pinned-leaf retrieval and ingestion" },
      { key: "webscraper", version: 3, service: "webscraper :9119", status: "live", probe: "green", ops: 4, licence: "webscraper", note: "four-backend scraping → Markdown/JSON" },
      { key: "media", version: 2, service: "media_discovery :8809", status: "live", probe: "green", ops: 3, licence: "media", note: "federated media search + transcripts" },
      { key: "scientific", version: 1, service: "scientific_processor :8808", status: "draft", probe: "unknown", ops: 5, licence: "ahtapol", note: "deployed but scaled to 0; probe needed" },
      { key: "lean4", version: 1, service: "lean4_prover :8812", status: "live", probe: "green", ops: 3, licence: "verification", note: "Lean 4 typecheck/elaborate/prove" },
      { key: "w2v", version: 1, service: "w2v-orchestrator :6300", status: "live", probe: "green", ops: 8, licence: "w2v", note: "deliverable runs; migration target" },
      { key: "ops", version: 5, service: "k8s · GitHub · Flux · Prometheus/Loki", status: "live", probe: "green", ops: 14, licence: "ops", note: "read verbs live; write verbs gated" },
      { key: "billing", version: 1, service: "billing_service + licenses :8544", status: "live", probe: "green", ops: 3, licence: "billing", note: "usage and entitlement facts" },
      { key: "discovery", version: 0, service: "AHTAPol discovery :8807", status: "draft", probe: "—", ops: 0, licence: "ahtapol", note: "specified; binds when its slice deploys it" },
    ],
    toolBindings: [
      { id: "TB-11", job: "backend", toolkit: "coding", pin: "coding@4", ceiling: "write", credential: "coding.tenant", rate: "60 calls / run" },
      { id: "TB-12", job: "architect", toolkit: "strata", pin: "strata@2", ceiling: "read", credential: "strata_rag.tenant", rate: "20 calls / run" },
      { id: "TB-13", job: "sre", toolkit: "ops", pin: "ops@5", ceiling: "destructive", credential: "ops.namespace", rate: "30 calls / run" },
      { id: "TB-14", job: "pm", toolkit: "billing", pin: "billing@1", ceiling: "read", credential: "billing.tenant", rate: "10 calls / run" },
    ],
    toolCalls: [
      { id: "TC-9001", run: "RUN-151", step: "03", tool: "coding.jobs.create", version: "coding@4", status: "ok", latency: "412 ms", bytes: "1.2 KB", cost: "$0.004" },
      { id: "TC-9002", run: "RUN-151", step: "04", tool: "coding.test_runs.create", version: "coding@4", status: "ok", latency: "8.4 s", bytes: "3.1 KB", cost: "$0.011" },
      { id: "TC-9003", run: "RUN-158", step: "02", tool: "ops.k8s_read", version: "ops@5", status: "ok", latency: "190 ms", bytes: "6.7 KB", cost: "$0.000" },
      { id: "TC-9004", run: "RUN-160", step: "01", tool: "strata.knowledge.query", version: "strata@2", status: "precondition_failed", latency: "—", bytes: "0", cost: "$0.000" },
    ],
    credentials: [
      { alias: "coding.tenant", secret: "coding.token", holder: "tenant", fingerprint: "fp_9a31…", state: "Configured" },
      { alias: "strata_rag.tenant", secret: "strata_rag.token", holder: "tenant", fingerprint: "fp_4c07…", state: "Configured" },
      { alias: "strata_ingest.tenant", secret: "strata_ingest.token", holder: "tenant", fingerprint: "fp_1d92…", state: "Configured" },
      { alias: "ops.namespace", secret: "ops.service_account", holder: "platform", fingerprint: "fp_7e10…", state: "Platform-managed" },
      { alias: "billing.tenant", secret: "billing.token", holder: "tenant", fingerprint: "fp_2b55…", state: "Configured" },
    ],
    meter: {
      tenant: { tokens: "18.4M", cost: "$412.80", tools: "$38.10", budget: "62%" },
      department: { label: "delivery (primary)", tokens: "11.2M", cost: "$251.40", tools: "$26.00", budget: "58%" },
      programme: { label: "Connected public services", tokens: "9.6M", cost: "$214.20", tools: "$21.40", budget: "54%" },
      project: { label: "Identity & access", tokens: "3.1M", cost: "$68.40", tools: "$7.20", budget: "41%" },
      job: { label: "software.delivery.backend_engineer", tokens: "1.4M", cost: "$31.10", tools: "$4.00", budget: "37%" },
      run: { label: "RUN-151", tokens: "184k", cost: "$4.12", tools: "$0.42", budget: "18%" },
      step: { label: "RUN-151 / step 04", tokens: "62k", cost: "$1.48", tools: "$0.11", budget: "—" },
      call: { label: "TC-9002 · coding.test_runs.create", tokens: "—", cost: "$0.011", tools: "$0.011", budget: "—" },
    },
    metrics: [
      ["Flow", "lead time 4.2 d · cycle time 2.1 d · WIP 14 · throughput 9 / wk", "card · list · event · run"],
      ["Quality", "first-pass yield 82% · rework 11% · benchmark Δ +12 pp", "run · approval · eval* · benchmark*"],
      ["Cost", "tokens 18.4M · cost / accepted artefact $1.94", "inference_call · tool_call"],
      ["Governance", "gate latency 1.8 d · override 3% · never-refused 1", "approval · audit_event"],
      ["Knowledge", "pack hit rate 71% · lesson promotion 2 / 6", "decision · inference_call · lesson"],
      ["Maturity", "C2 · thresholds calibrated from run/approval evidence", "configuration rows"],
    ],
    projectTypes: [
      ["delivery", "charter → plan → build → verify → release → close", "design (conditional) · merge · deploy · close"],
      ["architecture", "TOGAF ADM phases A–H", "per-phase contract gate"],
      ["improvement (kata)", "six Gets", "hypothesis · adoption"],
      ["development (self)", "design → branch → implement → CI → review → merge → deploy → verify", "design · merge · deploy"],
      ["operations", "detect → triage → mitigate → resolve → postmortem", "destructive-action gate"],
      ["setup (authoring)", "intake → asset design → review → publish → validate", "per-asset review · publish"],
    ],
  };

  /* -------------------------------------------------------------- live overrides */
  if (live) {
    const org = live.organisation || {};
    const prog = live.programme || {};
    const industries = list(org.industries);
    const departments = list(org.departments);
    const jobs = list(org.jobs);
    const releases = list(org.releases);
    const bindings = list(org.bindings);
    const programmes = list(prog.programme);
    const projects = list(prog.projects);
    const totals = ((live.measure || {}).inference || {}).totals || {};

    if (industries.length) {
      F.industries = industries.map((row) => ({
        id: row.key || row.id || row.name,
        name: row.name || row.key || row.id,
        tenant: live.tenant || "live",
        note: row.status || "live organisation row",
      }));
      count.industries = industries.length;
    }
    if (departments.length) {
      F.departments = departments.map((row) => ({
        id: row.id || row.name,
        name: row.name,
        primary: !!row.is_primary,
        jobs: jobs.filter((job) => list(job.department_ids).includes(row.id)).length,
        first: row.is_primary ? "primary department" : "live row",
        region: row.ordinal ? "ordinal " + row.ordinal : "optional sub-layer",
      }));
      count.departments = departments.length;
    }
    if (jobs.length) {
      F.jobs = jobs.map((job) => {
        const release = releases
          .filter((row) => row.job_id === job.id || row.job_key === job.key)
          .sort((a, b) => (b.version || 0) - (a.version || 0))[0];
        const binding = bindings.filter((row) => row.job_key === job.key || row.job_id === job.id)[0];
        return {
          id: job.id || job.key,
          key: job.key,
          name: job.name || job.key,
          department: list(job.department_names).join(" · ") || job.department || "—",
          family: job.family || "—",
          charter: job.charter || "",
          capabilities: list(job.capabilities),
          procedure: job.procedure_ref || job.process_key || "—",
          executor: job.executor_kind || "procedure",
          model: job.model_ref || "—",
          recipe: job.prompt_recipe || "—",
          knowledge: job.knowledge || "—",
          memory: job.memory || "—",
          grants: list(job.grants),
          binding: binding ? (binding.status || "binding") + " · " + short(binding.release_hash) : "—",
          limits: job.limits || "—",
          gates: job.gates || "—",
          authority: job.authority || "—",
          status: job.status === "active" ? "Published" : job.status === "retired" ? "Retired" : "Draft",
          release: release ? short(release.release_hash) : "—",
          metrics: list(job.metrics),
        };
      });
      count.jobs = jobs.length;
      v.job = F.jobs[0].id;
    }
    if (releases.length) {
      F.releases = releases.map((row) => ({
        id: row.id || row.release_hash,
        job: row.job_key || row.job_id || "—",
        hash: short(row.release_hash),
        status: row.status === "published" ? "Published" : "Draft",
        at: short(row.published_at || row.created_at, 10),
        note: row.status === "published" ? "pinned immutable release" : "not published",
      }));
      count.releases = releases.length;
    }
    if (bindings.length) {
      F.bindings = bindings.map((row) => ({
        id: row.id,
        job: row.job_key || row.job_id || "—",
        release: short(row.release_hash),
        executor: row.executor_kind || "procedure",
        capability: row.capability_id || "—",
        procedure: row.procedure_ref || "—",
        status: row.status === "active" ? "Active" : row.status === "retired" ? "Retired" : "Draft",
        tenant: live.tenant || "live",
        note: row.retired_at ? "retired; history keeps its pin" : "pins one release",
      }));
      count.bindings = bindings.length;
      v.binding = F.bindings[0].id;
    }
    if (programmes.length) {
      F.programmes = programmes.map((row) => ({
        id: row.id,
        key: "PGM",
        name: row.name,
        cadence: "—",
        budget: "—",
        tone: row.status === "active" ? "green" : "amber",
        note: row.status || "live programme row",
      }));
      count.programmes = programmes.length;
      v.programme = F.programmes[0].id;
    }
    if (projects.length) {
      const label = (status) =>
        ({ active: "Active", draft: "Draft", paused: "Paused", closed: "Closed", archived: "Archived" })[status] || status;
      const tone = (status) => (status === "active" ? "green" : status === "closed" || status === "archived" ? "" : "amber");
      // The board's cards are the project's plan units; a card is complete when its lane's
      // kind is `done`. A snapshot without boards leaves the counts at zero, not at a guess.
      const workBoards = list((live.work || {}).boards);
      const workLists = list((live.work || {}).lists);
      const workCards = list((live.work || {}).cards);
      const doneLists = new Set(workLists.filter((row) => row.kind === "done").map((row) => row.id));
      const countsFor = (projectId) => {
        const board = workBoards.find((row) => row.project_id === projectId);
        if (!board) return { completed: 0, total: 0 };
        const mine = workCards.filter((row) => row.board_id === board.id && !row.archived_at);
        return { completed: mine.filter((row) => doneLists.has(row.list_id)).length, total: mine.length };
      };
      db.projects = projects.map((row) => {
        const progress = countsFor(row.id);
        return {
          id: row.id,
          key: row.key,
          name: row.name,
          description: row.description || "",
          programme: row.programme_id,
          type: row.project_type_code || "delivery",
          phase: 0,
          status: label(row.status),
          tone: tone(row.status),
          agents: [],
          completed: progress.completed,
          total: progress.total,
          spent: 0,
          process: "feature",
          symbol: "projects",
        };
      });
      count.projects = projects.length;
    }
    if (totals && (totals.calls || totals.total_tokens)) {
      const human = (value) => {
        const n = Number(value) || 0;
        if (n >= 1e6) return (n / 1e6).toFixed(1) + "M";
        if (n >= 1e3) return Math.round(n / 1e3) + "k";
        return String(n);
      };
      F.meter.tenant = { tokens: human(totals.total_tokens), cost: "—", tools: "—", budget: "—" };
      if (F.programmes[0]) {
        F.meter.programme = {
          label: F.programmes[0].name,
          tokens: human(totals.total_tokens),
          cost: "—",
          tools: "—",
          budget: "—",
        };
      }
      F.metrics[2] = ["Cost", "tokens " + human(totals.total_tokens) + " · calls " + totals.calls + " · cost is derived by the app", "inference_call"];
      count.calls = Number(totals.calls) || 0;
    }
  }

  // Existing pages keep their ids and behaviour; V2 destinations are additive and link to them.
  const pkoDetail = {
    "PKO-T01": {
      definition:
        "Verify that a release has review evidence and a passed probe before it is published.",
      evidence: "SOP-02 · p.4 · release verification procedure",
      owner: "release manager",
      release: "pko@3",
      constraints: ["no unverified release"],
      steps: ["proc-implement v7 · step 04 (gate)"],
    },
    "PKO-T02": {
      definition:
        "Approve a baseline only when the required approvals and evidence references are present.",
      evidence: "SOP-01 · p.9 · baseline control",
      owner: "project office",
      release: "pko@3",
      constraints: ["dual control"],
      steps: ["proc-close v3 · step 02 (gate)"],
    },
    "PKO-R01": {
      definition:
        "The role accountable for release verification and for recording the verdict.",
      evidence: "SOP-02 · roles table",
      owner: "delivery",
      release: "pko@3",
      constraints: ["accountable"],
      steps: ["proc-implement v7 · step 04"],
    },
    "PKO-A01": {
      definition:
        "The run record, evaluation evidence and read-back that prove a release was verified.",
      evidence: "SOP-02 · appendix B",
      owner: "verification",
      release: "pko@3",
      constraints: ["evidence before progression"],
      steps: ["proc-implement v7 · step 04", "proc-close v3 · step 01"],
    },
    "PKO-C01": {
      definition:
        "A hard constraint: a release with no verification evidence cannot be published.",
      evidence: "SOP-02 · §4.1",
      owner: "verification",
      release: "pko@3",
      constraints: ["hard", "fails closed"],
      steps: ["proc-implement v7 · step 04 (gate)"],
    },
  };
  const pkoPredicates = {
    requires: { permitted: true, cardinality: "1..*" },
    performs: { permitted: true, cardinality: "1" },
    constrained_by: { permitted: true, cardinality: "1" },
    contains: {
      permitted: false,
      reason: "a task cannot contain another task in the base layer",
    },
  };
  const legacy = {
    overview: enterprise.pages.overview,
    projects: enterprise.pages.projects,
    work: enterprise.pages.work,
    jobtypes: enterprise.pages.jobtypes,
    improvement: enterprise.pages.improvement,
    constraints: enterprise.pages.constraints,
    knowledge: enterprise.pages.knowledge,
    execution: enterprise.pages.execution,
  };
  const v1Banner = (text) =>
    `<div class="v2-ribbon" role="note"><strong>V1 SURFACE</strong><span>${text}</span><span class="v2-src">Retained · the V2 destination is linked</span></div>`;
  const wrapLegacy = (fn, text) => () =>
    fn().replace(/(<div class="page[^>]*">)/, `$1${v1Banner(text)}`);

  // V2 §10.5: the run inspector gains a tools panel — which tool version was called,
  // with which credential alias, what it returned — plus the binding and release pin.
  function executionPage() {
    const calls = F.toolCalls
      .filter((c) => c.run === "RUN-151")
      .map(
        (c) =>
          `<tr><td class="mono">${E(c.step)}</td><td><strong class="mono">${E(c.tool)}</strong></td><td class="mono">${E(c.version)}</td><td class="mono">${E(c.credential || "coding.tenant")}</td><td>${B(c.status, c.status === "ok" ? "green" : "amber")}</td><td>${E(c.latency)}</td><td>${E(c.cost)}</td></tr>`,
      )
      .join("");
    const manifest = [
      ["capability@17 · p.42", "knowledge", "selected", "required kind · score 0.93", "1,820"],
      ["capability@17 · p.43", "knowledge", "excluded", "parse omission · passage disabled by a person", "0"],
      ["tenant identity decision", "knowledge", "selected", "tenant layer supersedes platform guidance", "1,240"],
      ["m-card · delegation claim", "memory", "selected", "same-card hot row wins", "410"],
      ["cold digest v4", "memory", "truncated", "hot rows win; context budget", "0"],
      ["repo.write · ci.read", "tools", "selected", "curated allowlist", "220"],
    ];
    const explain = panel(
      "Explain context · RUN-151",
      `${v.explainMissing ? note("<strong>409 capability_not_ready</strong> · required kind <code>domain.identity</code> has no resolvable leaf in verification. The run is refused before a model call; no context is compiled.", "bad") : ""}${table(
        ["Item", "Kind", "State", "Reason", "Chars"],
        manifest.map(
          (m) =>
            `<tr><td><strong>${E(m[0])}</strong></td><td>${E(m[1])}</td><td>${B(m[2], m[2] === "selected" ? "green" : m[2] === "excluded" ? "amber" : "purple")}</td><td><small>${E(m[3])}</small></td><td>${E(m[4])}</td></tr>`,
        ),
      )}${kv([
        ["Job binding", "bind-backend · active"],
        ["Release hash", `<span class="mono">3f9c…a1</span>`],
        ["Executor", B("procedure", "") + " with an internal_agent step"],
        ["Budget consumed", "8,160 / 12,000 characters · $1.84 · 184k tokens"],
        ["Governing versions", "policy route@3 · refinement delivery@2 · laya tenant@5"],
        ["Inheritance", "job row J-12 resolved <span class='mono'>domain.identity</span>"],
        ["Prompt identity", `<span class="mono">prompt_sha256:a7c9…e20f</span> · manifest v3`],
      ])}${v.explainRetired ? note("<strong>Not reconstructable</strong> · source_run ING-028 was retired. The manifest, source identities and hash remain, but the exact historical inputs are incomplete; no transcript is invented.", "bad") : note("Reconstruction available · all historical inputs are retained; the inspector can re-resolve the manifest but never renders the compiled text.", "ok")}${note("The compiled prompt text is never stored. This panel shows the recorded manifest, source identities and <span class='mono'>prompt_sha256</span>; it never renders the compiled prompt.")}<div class="row">${button("Simulate a missing required kind", "explain-missing", "", "small", "replay")}${button("Simulate a retired source", "explain-retired", "", "small", "replay")}${route("Open effective configuration", "knowledge-design", "small", "layers")}</div>`,
      B("manifest v3", "purple"),
    );
    const panelHtml =
      `<div class="v2-ribbon" role="note"><strong>V2 · PROPOSED</strong><span>The run inspector gains the tools panel and the explain-context manifest: which versions were eligible, what was selected, excluded or truncated, and why.</span><span class="v2-src">D41/D52 · proposed in T13</span></div>` +
      panel(
        "Run binding & tools",
        `${kv([["Job binding", "bind-backend · active"], ["Release hash", `<span class="mono">3f9c…a1</span>`], ["Executor", B("procedure", "") + " with an internal_agent step"], ["Budget", "$2.50 · 184k tokens · enforced before the next call"], ["Prompt sources", "knowledge · episodic memory · PDDL request · curated tool list"], ["Artifacts", "MinIO · tenant prefix · content hash"]])}${table(["Step", "Tool", "Version", "Credential", "Status", "Latency", "Cost"], [calls])}${note("One <span class='mono'>tool_call</span> row and one <span class='mono'>tool.called</span> event per call. A tool call is not an <span class='mono'>action_invocation</span> and never transitions a card; the credential is injected by the broker and never recorded.")}`,
        B("RUN-151", "green"),
      ) +
      explain;
    return legacy.execution().replace(/<\/div>\s*$/, panelHtml + "</div>");
  }
  const projects = () => db.projects.filter((p) => p.programme);
  const programme = () => F.programmes.find((p) => p.id === v.programme) || F.programmes[0];
  const job = () => F.jobs.find((j) => j.id === v.job) || F.jobs[0];
  const binding = () => F.bindings.find((b) => b.id === v.binding) || F.bindings[0];
  const toolkit = () => F.toolkits.find((t) => t.key === v.toolkit) || F.toolkits[0];
  const decision = () => F.decisionPolicies.find((d) => d.id === v.decision) || F.decisionPolicies[0];
  const pending = () => db.decisions.filter((d) => d.state === "Pending").length;
  const wsjf = (b) =>
    ((b.value + b.criticality + b.risk) / b.size).toFixed(1);

  /* ------------------------------------------------------------------ sidebar */
  function sidebar() {
    return `<div class="brand"><span class="brand-mark">${I("layers")}</span><div><div class="brand-word">conveyor</div><div class="brand-sub">THE OPERATING SYSTEM FOR WORK</div></div></div><div class="tenant-switch"><span class="tenant-mark">N</span><span class="grow">Non-Agentic AI<small>Organisation workspace</small></span><span>⌄</span></div><nav>
      <div class="nav-group"><div class="nav-label">PORTFOLIO &amp; PLAN</div>${N("overview", "Programme control", "grid")}${N("portfolio", "Portfolio", "layers")}${N("projects", "Projects", "projects", db.projects.length)}${N("plan", "Plan & WBS", "board")}${N("execution", "Execution room", "run")}${N("decisions", "Decision inbox", "inbox", pending())}${N("runs", "Run history", "clock")}</div>
      <div class="nav-group"><div class="nav-label">ORGANISATION</div>${N("organisation", "Organisation", "layers")}${N("jobs", "Jobs", "board", F.jobs.length)}${N("agents", "Agents", "agent")}${N("toolkits", "Tools", "tools")}${N("types", "Project types", "grid")}</div>
      <div class="nav-group"><div class="nav-label">PROCEDURES &amp; AUTHORING</div>${N("procedures", "Procedures", "flow")}${N("capabilities", "Capabilities", "layers")}${N("authoring", "Authoring", "book")}</div>
      <div class="nav-group"><div class="nav-label">GOVERN &amp; DESIGN</div>${N("assurance", "Assurance &amp; baselines", "shield")}${N("constraints", "Flow &amp; constraints", "branch")}${N("requirements", "Requirements", "file")}${N("principles", "Principles", "book")}${N("models", "Architecture &amp; UML", "model")}${N("architecture", "System architecture", "model")}${N("documents", "Documents", "file")}</div>
      <div class="nav-group"><div class="nav-label">KNOWLEDGE &amp; MEMORY</div>${N("knowledge-design", "Knowledge design", "branch")}${N("knowledge", "Sources &amp; retrieval", "book")}${N("memory", "Memory explorer", "layers")}</div>
      <div class="nav-group"><div class="nav-label">MEASURE &amp; IMPROVE</div>${N("improve", "Improve", "chart")}${N("evals", "Evaluation studio", "shield")}${N("hillclimb", "Hillclimb lab", "branch")}${N("benchmarks", "Benchmark harness", "code")}${N("usage", "Inference &amp; prompt audit", "chart")}${N("metering", "Metering", "chart")}</div>
      <div class="nav-group"><div class="nav-label">OPERATIONS</div>${N("operations", "Operations", "run")}</div></nav><div class="sidebar-bottom"><b>Position → release → binding → run</b><div class="row"><i class="dot green"></i>SPEC_V2 design study</div></div>`;
  }

  /* ------------------------------------------------------------------ Portfolio */
  function portfolio() {
    const tab = v.tab.portfolio || "In flight";
    const scoped = projects().filter((p) => p.programme === v.programme);
    const rows = scoped.map(
      (p) =>
        `<tr><td><strong>${E(p.name)}</strong><small>${E(p.key)} · ${E(p.stream || p.type)}</small></td><td>${E(p.enterprisePhase || p.status)}</td><td><div class="v2-bar"><i style="width:${Math.round(((p.completed || 0) / (p.total || 1)) * 100)}%"></i></div><small>${p.completed || 0} / ${p.total || 0}</small></td><td>${B(p.blocks ? p.blocks + " open" : "Clear", p.blocks ? "amber" : "green")}</td><td>${p.live || 0}</td><td>${money(p.spent || 0)}</td></tr>`,
    );
    const content =
      tab === "In flight"
        ? `${panel("In-flight portfolio", `<p class="muted">Every project and programme in flight for the tenant — a computed read model over <span class="mono">programme</span> and <span class="mono">project</span>; no portfolio row exists.</p>${table(["Project", "Lifecycle", "Delivery", "Blockers", "Live", "Spend"], rows)}`, B(rows.length + " projects", "green"))}${note("Portfolio membership is derived from the tenant's configured in-flight statuses (default active + paused). Closed and archived work is history and is not listed.", "ok")}`
        : tab === "Programmes"
          ? `${panel("Programmes", `${table(["Programme", "Cadence", "Budget", "Health"], F.programmes.map((p) => `<tr><td><strong>${E(p.name)}</strong><small>${E(p.key)} · ${E(p.note)}</small></td><td>${E(p.cadence)}</td><td>${E(p.budget)}</td><td>${B(p.tone === "green" ? "On track" : "Attention", p.tone)}</td></tr>`))}${note("A programme carries <span class='mono'>shared_config</span> (knowledge pins, job families, approval defaults, budget, cadence) inherited by its projects. A project may override a key only when the programme does not lock it.")}`, route("Open programme control", "overview", "small", "grid"))}`
          : tab === "Shared config & dependencies"
            ? `<div class="v2-split"><div>${panel("Shared configuration", `${table(["Key", "Value", "Lock", "Inherited by"], [["knowledge_pin", "capability@17", "locked", "3 projects"], ["job_families", "delivery · verification · operations", "locked", "3 projects"], ["approval_policy", "human · dual for phase gates", "tenant policy", "3 projects"], ["budget", "€48.6m", "locked", "3 projects"], ["cadence", "PI-2026.4 · demo D-14", "locked", "3 projects"]].map((r) => `<tr><td class="mono">${E(r[0])}</td><td>${E(r[1])}</td><td>${B(r[2], r[2] === "locked" ? "amber" : "")}</td><td>${E(r[3])}</td></tr>`))}`)}</div><div>${panel("Project dependency graph", `${F.projectDeps.map((d) => `<div class="v2-step"><span class="n">${E(d.kind)}</span><span class="grow"><strong>${E(d.from)} → ${E(d.to)}</strong><small>${E(d.note)} · ${E(d.status)}</small></span></div>`).join("")}${button("Add dependency", "cycle", "", "small", "plus")}${v.cycle ? note("<strong>422 project_dependency_cycle</strong> · identity → portal → billing → identity is a cycle. The edge was refused at creation.", "bad") : ""}${note("Edges are typed rows: <span class='mono'>blocks | needs | supersedes | informs</span>. A cycle is refused at creation, not repaired later.")}`, B("3 edges"))}</div></div>`
            : tab === "Backlog & WSJF"
              ? `${panel("Programme backlog", `<p class="muted">WSJF is arithmetic, not opinion: <span class="mono">(business_value + time_criticality + risk_reduction) / job_size</span>. All four inputs are typed integers; the score is computed, never stored.</p>${table(["ID", "Backlog item", "Value", "Criticality", "Risk", "Size", "WSJF", "Status"], F.backlog.map((b) => `<tr><td class="mono">${E(b.id)}</td><td>${E(b.item)}</td><td>${b.value}</td><td>${b.criticality}</td><td>${b.risk}</td><td>${b.size}</td><td><strong>${wsjf(b)}</strong></td><td>${B(b.status, b.status === "Stale" ? "amber" : "green")}</td></tr>`))}`, button("Re-score backlog", "wsjf", "", "small", "replay"))}${note("Backlog refinement is a scheduled job that re-scores WSJF and flags stale items. The plan is materialised for the next cadence; a person approves it.")}`
              : tab === "Cadence & capacity"
                ? `<div class="v2-split"><div>${panel("PI-2026.4 cadence", `${flow([["PI planning", "draft cards from backlog by WSJF within WIP"], ["System demo", "each deliverable exhibits evidence; no demo, no credit"], ["Inspect & adapt", "retro produces lessons and kata candidates"]])}${note("A programme_cadence row materialises the next cadence's draft cards. Cards are ordered by WSJF and bounded by the tenant's WIP limits; a person approves the plan.")}`)}</div><div>${panel("Capacity", `${table(["Team", "Members", "WIP", "Used"], [["Delivery", "7", "14", "11"], ["Verification", "3", "6", "5"], ["Operations", "3", "6", "2"], ["Project office", "3", "6", "4"]].map((r) => `<tr><td>${E(r[0])}</td><td>${r[1]}</td><td>${r[2]}</td><td>${r[3]}</td></tr>`))}${note("WIP is a pressure valve, not a report: a breach is a coach card and a laya remedy.")}`)}</div></div>`
                : `${panel("Metering · " + programme().name, `${table(["Level", "Tokens", "Cost", "Tool spend", "Budget"], [["Programme", "9.6M", "$214.20", "$21.40", "54%"], ["Project: Identity & access", "3.1M", "$68.40", "$7.20", "41%"], ["Project: Citizen services", "4.2M", "$101.10", "$9.80", "57%"], ["Project: Core services", "2.3M", "$44.70", "$4.40", "48%"]].map((r) => `<tr><td>${E(r[0])}</td><td>${r[1]}</td><td>${r[2]}</td><td>${r[3]}</td><td>${r[4]}</td></tr>`))}${note("Cost is derived at read time from <span class='mono'>inference_call</span> and <span class='mono'>tool_call</span>; tokens are stored, money is not. The same derivation feeds the API.")}`, route("Open metering", "metering", "small", "chart"))}`;
    return `<div class="page enterprise">${heading("Portfolio", "Every project and programme in flight for the tenant, its shared configuration, dependency graph, WSJF backlog and measured spend.", `${route("Plan & WBS", "plan", "", "board")}${route("Organisation", "organisation", "", "layers")}`, "PORTFOLIO & PLAN")}${ribbon("Portfolio is a computed read model (D49); programme shared config, cadence and metering are proposed V2 aggregates.", count.programmes > 0)}<div class="e-programmes">${F.programmes.map((p) => `<button class="e-programme ${p.id === v.programme ? "selected" : ""}" data-v2="programme" data-id="${p.id}" aria-pressed="${p.id === v.programme}"><span class="e-programme-top"><span class="mono">${p.key}</span>${B(p.tone === "green" ? "On track" : "Attention", p.tone)}</span><strong>${E(p.name)}</strong><small>${projects().filter((x) => x.programme === p.id).length} projects · ${E(p.cadence)}</small></button>`).join("")}</div><div class="e-scopebar"><div>${I("layers")}<strong>${E(programme().name)}</strong>${B("V2 portfolio", "green")}<span class="muted">${E(programme().note)}</span></div><div><span class="mono muted">${E(programme().cadence)} · ${E(programme().budget)}</span></div></div>${tabs(["In flight", "Programmes", "Shared config & dependencies", "Backlog & WSJF", "Cadence & capacity", "Metering"], tab, "portfolio")}${content}${dataPanel()}</div>`;
  }

  /* ------------------------------------------------------------------ Plan */
  function plan() {
    const work = live ? (live.work || {}) : {};
    const liveBoards = list(work.boards).length > 0;
    const tab = v.tab.plan || (liveBoards ? "Boards" : "WBS");
    const change = F.changes[0];
    const content =
      tab === "Boards"
        ? planBoards(work, liveBoards)
        : tab === "WBS"
        ? `${panel("Work breakdown", `${table(["WBS", "Kind", "Deliverable / work package", "Acceptance", "Evidence", "State"], F.wbs.map((n) => `<tr><td class="mono">${E(n.id)}</td><td>${E(n.kind)}</td><td><strong>${E(n.name)}</strong></td><td>${E(n.accept)}</td><td>${E(n.evidence)}</td><td>${B(n.state, n.state === "Accepted" ? "green" : n.state === "Blocked" ? "amber" : "")}</td></tr>`))}${note("A WBS node is accepted when its acceptance criteria are met by an artefact or run — not when a checkbox is ticked.")}`, B("5 nodes"))}`
        : tab === "Baseline vs forecast"
          ? `<div class="v2-split"><div>${panel("Baseline B3 (frozen)", `${kv([["Approved", E(F.baseline.approved)], ["Scope", E(F.baseline.scope)], ["Schedule", E(F.baseline.schedule)], ["Cost", E(F.baseline.cost)], ["State", B(F.baseline.state, "green")]])}${note("A baseline is a frozen, approved snapshot. It is not recomputed.")}`)}</div><div>${panel("Forecast (computed)", `${kv([["Scope", "44 deliverables <strong class='e-amber'>+2</strong>"], ["Schedule", "2026-12-27 <strong class='e-amber'>+9 d</strong>"], ["Cost", "€49.4m <strong class='e-amber'>+€0.8m</strong>"], ["Risk", "medium · identity contract"], ["Run pins", "unchanged · capability@17"]])}${note("Forecast is computed from current rows. A variance is not a change until a change request is approved.")}`)}</div></div>${note("Plan ≠ evidence: a deliverable needs an artefact/run, not a tick. Baseline ≠ forecast: a forecast never rewrites the approved baseline.", "ok")}`
          : tab === "Registers"
            ? `${panel("One generic register", `<p class="muted">One table, six kinds — risk · issue · assumption · dependency · benefit · change. The V1 lesson: five PM tables measured zero use.</p>${table(["ID", "Kind", "Title", "Owner", "Status", "Severity", "Link"], F.registers.map((r) => `<tr><td class="mono">${E(r.id)}</td><td>${B(r.kind)}</td><td>${E(r.title)}</td><td>${E(r.owner)}</td><td>${E(r.status)}</td><td>${E(r.severity)}</td><td><small>${E(r.note)}</small></td></tr>`))}`, button("New register item", "reg-new", "", "small", "plus"))}${note("A risk can open a mitigation card; a dependency can link two projects; a benefit is measured at closure (planned vs realised).")}`
            : `<div class="v2-split"><div>${panel("Change control · CR-018", `<h3>${E(change.title)}</h3>${kv([["Changes baseline", E(change.baseline)], ["Impact", E(change.impact)], ["Recorded rationale", change.rationale ? E(change.rationale) : "<span class='e-amber'>required for approve or return</span>"], ["Existing run pins", "unchanged · a new attempt captures B4"]])}<textarea data-v2-input="rationale" rows="3" placeholder="Record the person's rationale" aria-label="Change rationale">${E(change.rationale)}</textarea><div class="row" style="margin-top:10px">${button("Approve new baseline", "change-approve", "", "primary", "shield")}${button("Return for analysis", "change-return", "", "", "replay")}</div>${v.review === "change-blocked" ? note("<strong>422 change_rationale_required</strong> · a person's rationale is required to approve or return a change request. No baseline moved.", "bad") : v.review === "change-ok" ? note("<strong>Approved locally</strong> · baseline B4 drafted; existing runs keep B3 pins. A new sample attempt captures B4.", "ok") : ""}`)}</div><div>${panel("Impact analysis", `${kv([["Scope", "2 deliverables"], ["Schedule", "+9 days"], ["Cost", "+€0.8m"], ["Risk", "medium · identity contract"], ["Approval", "human · programme gate"]])}${note("The change approval, the baseline review and the programme gate are separate decisions.")}`)}</div></div>`;
    const planRibbon =
      tab === "Boards"
        ? ribbon("The programme's plan units are cards on their project's board, rendered from the conveyor's own rows. One card per registered issue; each names a real process.", liveBoards)
        : ribbon("V2 plan spine: wbs_node, project_baseline, project_register_item (six kinds), project_dependency and change control. Proposed in T1.");
    return `<div class="page enterprise">${heading("Plan & WBS", "The plan spine: the programme's boards and cards, then the WBS, a frozen baseline against a computed forecast, one six-kind register and controlled change.", `${route("Portfolio", "portfolio", "", "projects")}${route("Assurance", "assurance", "", "shield")}`, "PLAN")}${planRibbon}${tabs(["Boards", "WBS", "Baseline vs forecast", "Registers", "Change control"], tab, "plan")}${content}<div class="toolbar" style="margin-top:16px"><span class="mono muted">Existing V1 surfaces stay available:</span>${route("Work breakdown editor", "work", "small", "board")}${route("Assurance & baselines", "assurance", "small", "shield")}</div></div>`;
  }

  /* The plan units as real cards: project -> board -> lanes -> cards. LIVE when the snapshot
     carries boards; otherwise the committed plan seed (the same document the seeder installs). */
  function planBoards(work, liveBoards) {
    if (liveBoards) {
      const projects = list((live.programme || {}).projects);
      if (!projects.length) return panel("Boards", note("The snapshot carries boards but no projects."));
      const project = projects.find((row) => row.id === v.planProject) || projects[0];
      const board = list(work.boards).find((row) => row.project_id === project.id);
      if (!board) return panel(E(project.name), note("This project has no board in the snapshot."));
      const lists = list(work.lists)
        .filter((row) => row.board_id === board.id)
        .sort((a, b) => Number(a.position) - Number(b.position));
      const cards = list(work.cards).filter((row) => row.board_id === board.id && !row.archived_at);
      const types = new Map(list(work.card_types).map((row) => [row.id, row]));
      const runs = list(work.runs);
      const lanes = lists
        .map((lane) => {
          const laneCards = cards
            .filter((row) => row.list_id === lane.id)
            .sort((a, b) => Number(a.position) - Number(b.position));
          const rows = laneCards.map((card) => {
            const type = types.get(card.card_type_id);
            const run = runs
              .filter((row) => row.card_id === card.id)
              .sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)))[0];
            const issue =
              card.issue == null
                ? "—"
                : `<a class="e-project-link" href="https://github.com/nonagenticai/conveyor/issues/${E(card.issue)}" target="_blank" rel="noopener">#${E(card.issue)} ↗</a>`;
            return `<tr><td><strong>${E(card.name)}</strong><small class="mono">${E(card.unit_key || "")}</small></td><td>${B(type ? type.code : "—")}</td><td class="mono">${E(card.process_key || "—")}</td><td>${issue}</td><td>${run ? B(run.status) : "—"}</td></tr>`;
          });
          return panel(
            `${lane.name} · ${laneCards.length}`,
            rows.length
              ? table(["Card", "Type", "Process", "Issue", "Run"], rows)
              : `<p class="muted">No cards in this lane.</p>`,
          );
        })
        .join("");
      return `${panel("Project", `<label class="e-select">Project <select data-v2-input="plan-project">${projects.map((row) => `<option value="${E(row.id)}" ${row.id === project.id ? "selected" : ""}>${E(row.key)} · ${E(row.name)}</option>`).join("")}</select></label><p class="muted">Board <span class="mono">${E(board.name)}</span> · ${cards.length} cards · ${lists.length} lanes · read from snapshot ${E(short((live || {}).payload_sha256))}.</p>`)}${lanes}`;
    }
    const seed = window.CONVEYOR_PLAN_SEED;
    const projects = list((seed || {}).projects);
    if (!projects.length) return panel("Boards", note("No plan seed is loaded; the design fallback is unavailable."));
    const project = projects.find((row) => row.project_key === v.planSeedProject) || projects[0];
    const rows = list(project.units).map(
      (unit) =>
        `<tr><td><strong>${E(unit.title)}</strong><small class="mono">${E(unit.unit_key)}</small></td><td>${B(unit.state === "CLOSED" ? "Done" : "Open", unit.state === "CLOSED" ? "" : "amber")}</td><td>${B(unit.card_type)}</td><td class="mono">${E(unit.process_key)}</td><td>${unit.issue == null ? "—" : "#" + E(unit.issue)}</td></tr>`,
    );
    return panel(
      "Plan seed · DESIGN",
      `<label class="e-select">Project <select data-v2-input="plan-seed-project">${projects.map((row) => `<option value="${E(row.project_key)}" ${row.project_key === project.project_key ? "selected" : ""}>${E(row.project_key)} · ${E(row.plan || "")}</option>`).join("")}</select></label>${table(["Unit", "State", "Card type", "Process", "Issue"], rows)}${note("The committed plan seed — the same document the seeder installs. Live rows replace it when a snapshot carries boards.")}`,
    );
  }

  /* ------------------------------------------------------------------ Organisation */
  function organisation() {
    const tab = v.tab.organisation || "Authority matrix";
    const p = decision();
    const refine = F.refinements.find((r) => r.policy === p.id) || F.refinements[0];
    const content =
      tab === "Authority matrix"
        ? `${panel("Authority matrix (defaults; tenant-configurable)", `<p class="muted">The matrix is data — <span class="mono">tenant_role_scope</span> and the binding's <span class="mono">authority</span> — not branches in code.</p><div class="e-table-scroll"><table class="v2-matrix"><thead><tr><th>Action</th><th>PM</th><th>Coach</th><th>Kata</th><th>Delivery job</th><th>Ops</th><th>Human</th></tr></thead><tbody>${F.authority.map((r) => `<tr>${r.map((c) => `<td>${E(c)}</td>`).join("")}</tr>`).join("")}</tbody></table></div>${note("Every gate's required decisions and approver kinds are tenant-configured policy rows defaulting to human approval.")}`, B("9 actions"))}`
        : tab === "Approval policies"
          ? `${panel("Approval policies", `${table(["Gate", "Required decisions", "Approver", "Expiry", "Rule"], F.approvalPolicies.map((a) => `<tr><td><strong>${E(a.gate)}</strong></td><td>${a.decisions}${a.decisions > 1 ? " (dual control)" : ""}</td><td>${B(a.approver, "green")}</td><td>${E(a.expiry)}</td><td><small>${E(a.note)}</small></td></tr>`))}`, `${button("Try self-approval", "self-approve", "", "small", "shield")}`)}${note("<strong>Dual control</strong> is <span class='mono'>required_decisions ≥ 2</span> with distinct approvers; the executor, assignee or requester cannot approve the gate that judges their work. A gate with no evidence is refused (<span class='mono'>422 gate_evidence_missing</span>) unless an override is recorded and counted.")}${note("An <span class='mono'>auto</span> approver is flagged in the governance audit; the default is human. A gate that has never refused anything in a full programme cycle is a broken gate, not a healthy one.", "ok")}`
          : tab === "Decision ladder"
            ? `<div class="v2-ladder"><div><span class="lvl">L0 · RULE</span><h3>Deterministic</h3><p>State transitions, WIP limits, WSJF arithmetic, thresholds, policy checks and acceptance — the row plus <span class="mono">action_invocation</span>.</p></div><div><span class="lvl">L1 · LAYA</span><h3>Calibrated choice</h3><p>Triage, routing, job selection, remedy, severity, priority band, gate pre-check — a <span class="mono">decision</span> row with answer, probabilities and confidence.</p></div><div><span class="lvl">L2 · GENERATION</span><h3>LLM draft</h3><p>Drafts, patches, narratives and candidate procedures — an <span class="mono">inference_call</span> plus a draft artefact. No path to a state transition.</p></div><div><span class="lvl">L3 · HUMAN</span><h3>Approval</h3><p>Every gate, override and irreversible action — an <span class="mono">approval</span> plus <span class="mono">approval_decision</span> with <span class="mono">decided_by_kind</span> and <span class="mono">via</span>.</p></div></div>${note("<strong>The hard rule:</strong> L2 has no path to a state transition. A generated output is a draft artefact; applying it is an action, and the action's gate is L1/L3. A recommendation in prose is displayed but never consumed by the engine.", "ok")}${panel("Decision ledger", `${table(["Case", "Answer", "Confidence", "Probabilities", "Policy / refinement / laya", "Latency", "Input hash", "Outcome"], F.decisions.map((d) => `<tr><td><strong>${E(d.case)}</strong></td><td>${E(d.answer)}</td><td>${d.confidence.toFixed(2)}</td><td><small>${E(d.probs)}</small></td><td><small>${E(d.policy)} · ${E(d.refine)} · ${E(d.laya)}</small></td><td>${E(d.latency)}</td><td class="mono">${E(d.hash)}</td><td>${B(d.outcome, d.outcome.includes("below") || d.outcome.includes("human") ? "amber" : "green")}</td></tr>`))}${note("Every decision is reproducible from its row: option set version, answer, probabilities where the type provides them, confidence, policy, logit latency and input hash. <span class='mono'>Unavailable</span> is <span class='mono'>gate_unavailable</span> and follows the fail policy — never a silent pass.")}`, button("Show the generation boundary", "generation-boundary", "", "small", "shield"))}`
            : tab === "Refinements & laya config"
              ? `<div class="v2-split"><div>${panel("Platform policy → tenant refinement", `<div class="e-scope"><span class="mono">CASE</span><strong>${E(p.case)}</strong><span class="grow"></span><span class="mono muted">${E(p.type)} · options are rows</span></div>${kv([["Options", E(p.options)], ["Platform threshold", p.threshold.toFixed(2)], ["Tenant refinement", `${refine.change} · <strong>v${refine.version}</strong>`], ["Fail policy", E(refine.fail)], ["Laya config", E(refine.laya)], ["Change gate", E(refine.gate)]])}${button("Create a refinement version", "refine", "", "small", "edit")}${button("Restore platform default", "refine-reset", "", "small", "replay")}${note("A refinement overrides the threshold or rule, the fail action and optionally the laya inference config. It is versioned; changing one is a gated policy change. Every decision pins the refinement version it used.")}`, B("refinement v" + refine.version, "amber"))}</div><div>${panel("Laya inference config", `${kv([["Provider kind", E(F.laya.kind)], ["Model", E(F.laya.model)], ["Params", E(F.laya.params)], ["Timeout", E(F.laya.timeout)], ["Endpoint", E(F.laya.endpoint)], ["Version", "laya@" + F.laya.version]])}${note(E(F.laya.note))}`, B("tenant-scoped", "green"))}</div></div>`
              : `${panel("Job performance", `<p class="muted">Computed from execution rows, never stored as opinion. Cost is the same derivation the API serves; the run inspector drills to step and tool call.</p>${table(["Job", "Department", "Executor", "Release", "Metrics", "Binding"], F.jobs.map((j) => `<tr><td><strong>${E(j.name)}</strong><small>${E(j.key)}</small></td><td>${E(j.department)}</td><td>${B(j.executor === "procedure" ? "procedure" : "internal_agent", j.executor === "procedure" ? "" : "purple")}</td><td class="mono">${E(j.release)}</td><td><small>${j.metrics.map(E).join(" · ")}</small></td><td><small>${E(j.binding)}</small></td></tr>`))}`, route("Open metering", "metering", "small", "chart"))}${note("A job may lower a limit at run time, never raise it. The resolved <span class='mono'>{job_key, release_hash, procedure_ref, model_ref, allowed_actions}</span> travels in the run's input hash.")}`;
    return `<div class="page enterprise">${heading("Organisation", "Industry → department(s) → job; the authority matrix, tenant approval policy, the decision ladder and the laya configuration that every run pins.", `${route("Jobs", "jobs", "", "board")}${route("Tools", "toolkits", "", "tools")}`, "ORGANISATION")}${ribbon("V2 organisation spine: industry, department, job_catalogue, job_release, job_binding, approval_policy, decision_policy/refinement and laya_inference_config. Proposed in T0/T2.", count.industries > 0 || count.departments > 0)}${tabs(["Authority matrix", "Approval policies", "Decision ladder", "Refinements & laya config", "Job performance"], tab, "organisation")}${content}</div>`;
  }

  /* ------------------------------------------------------------------ Jobs */
  function jobs() {
    const tab = v.tab.jobs || "Catalogue";
    const j = job();
    const b = binding();
    const list = F.jobs.filter(
      (x) => v.department === "all" || !v.department || x.department === v.department,
    );
    const content =
      tab === "Catalogue"
        ? `<div class="v2-split"><div>${panel("Job catalogue", `<div class="toolbar"><span class="mono muted">FILTER</span><button class="btn ${!v.department || v.department === "all" ? "selected" : ""}" data-v2="dept" data-id="all">All departments</button>${F.departments.map((d) => `<button class="btn ${v.department === d.id ? "selected" : ""}" data-v2="dept" data-id="${d.id}">${E(d.name)}</button>`).join("")}</div>${F.industries.map((i) => `<div class="e-scope"><span class="tenant-mark">${i.id === "software" ? "N" : i.id === "hta" ? "A" : "W"}</span><strong>${E(i.industry || i.name)}</strong>${B(i.tenant, "green")}<span class="muted">${E(i.note)}</span></div>`).join("")}${table(["Job", "Department", "Family", "Executor", "Status"], list.map((x) => `<tr class="${x.id === v.job ? "selected" : ""}"><td><button class="e-project-link" data-v2="job" data-id="${x.id}"><span><strong>${E(x.name)}</strong><small class="mono">${E(x.key)}</small></span></button></td><td>${E(x.department)}</td><td>${E(x.family)}</td><td>${B(x.executor === "procedure" ? "procedure" : "internal_agent", x.executor === "procedure" ? "" : "purple")}</td><td>${B(x.status, "green")}</td></tr>`))}</div></div>`)}<div>${panel(E(j.name), `${kv([["Key", `<span class="mono">${E(j.key)}</span>`], ["Department", E(j.department)], ["Charter", E(j.charter)], ["Executor", B(j.executor, j.executor === "procedure" ? "" : "purple")], ["Release", `<span class="mono">${E(j.release)}</span>`], ["Binding", E(j.binding)]])}${button("Evaluate this job", "job-eval", "", "small", "chart")}${route("Open the full anatomy", "jobs", "small", "board")}${note("A job is configuration, not a loop. Its release carries the charter, capability binding, procedure, inference configuration, knowledge/memory pins, tool grants, limits, gates and authority.")}`)}</div></div>`
        : tab === "Departments"
          ? `${panel("The organisation spine", `<p class="muted">A job sits in exactly one industry and one or more ordered departments, primary first. A department from another industry is <span class="mono">422 department_industry_mismatch</span>. Knowledge compiles base → industry → department archetype → each department in order (with its region sub-layer) → job.</p>${table(["Industry", "Tenant", "Departments", "Jobs", "First work"], F.industries.map((i) => `<tr><td><strong>${E(i.name)}</strong></td><td>${E(i.tenant)}</td><td>${i.id === "software" ? F.departments.map((d) => E(d.name)).join(" · ") : "authored with the tenant"}</td><td>${i.id === "software" ? F.jobs.length : "—"}</td><td><small>${E(i.note)}</small></td></tr>`))}`, B("3 industries", "green"))}${panel("Departments · software", `${table(["Department", "Primary", "Jobs", "First work", "Region"], F.departments.map((d) => `<tr><td><strong>${E(d.name)}</strong></td><td>${d.primary ? B("primary", "green") : "—"}</td><td>${d.jobs}</td><td>${E(d.first)}</td><td>${d.region || "optional sub-layer"}</td></tr>`))}${note("A sibling department that sets one knowledge key to a different value is a <span class='mono'>department_conflict</span> compile error, resolved only by a job-layer row.")}`)}`
          : tab === "Releases"
            ? `${panel("Immutable releases", `${table(["Release", "Job", "Content hash", "Status", "Published", "Note"], F.releases.map((r) => `<tr><td class="mono">${E(r.id)}</td><td>${E(r.job)}</td><td class="mono">${E(r.hash)}</td><td>${B(r.status, r.status === "Published" ? "green" : "amber")}</td><td>${E(r.at)}</td><td><small>${E(r.note)}</small></td></tr>`))}`, button("Publish the draft release", "publish-release", "", "small", "shield"))}${v.review === "release" ? note("<strong>Publish gate opened</strong> · rel-backend-5 is still a draft. A release change is a new binding and a gate (D38); it never rewrites a live binding.", "ok") : ""}${note("A release is immutable and pinned by a content hash. A run records <span class='mono'>job_key + release_hash</span>; no live design → <span class='mono'>409 job_not_live</span>; no resolvable required knowledge → <span class='mono'>409 release_not_ready</span> naming the kind and department.")}`
            : tab === "Bindings"
              ? `<div class="v2-split"><div>${panel("Tenant bindings", `${table(["Binding", "Job", "Release", "Executor", "Status"], F.bindings.map((x) => `<tr class="${x.id === v.binding ? "selected" : ""}"><td><button class="e-project-link" data-v2="binding" data-id="${x.id}"><span><strong class="mono">${E(x.id)}</strong><small>${E(x.tenant)}</small></span></button></td><td>${E(x.job)}</td><td class="mono">${E(x.release)}</td><td>${B(x.executor === "procedure" ? "procedure" : "internal_agent", x.executor === "procedure" ? "" : "purple")}</td><td>${B(x.status, x.status === "Active" ? "green" : "amber")}</td></tr>`))}`, button("Retire the selected binding", "retire-binding", "", "small", "close"))}</div><div>${panel("Binding · " + b.id, `${kv([["Pins release", `<span class="mono">${E(b.release)}</span>`], ["Executor", B(b.executor, b.executor === "procedure" ? "" : "purple")], ["Capability", E(b.capability)], ["Procedure", `<span class="mono">${E(b.procedure)}</span>`], ["Status", B(b.status, b.status === "Active" ? "green" : "amber")], ["Note", E(b.note || "pinned snapshot; never refreshed in place")]])}${button("Run against a stale release", "stale", "", "small", "replay")}${v.review === "stale" ? stale("job_binding_stale") : ""}</div>`)}</div>`
              : tab === "Anatomy"
                ? `${panel(E(j.name) + " · anatomy", `<p class="muted">One author: the conveyor. Every field lives in the catalogue or the immutable release; nothing is authored in another system for a conveyor job.</p><div class="e-table-scroll"><table class="v2-matrix"><thead><tr><th>Field</th><th>Meaning</th><th>Value</th><th>Author</th></tr></thead><tbody>${[["job_key", "stable position key", j.key, "conveyor (job_catalogue)"], ["placement", "industry, ordered departments, region", "software · delivery (primary)" + (j.department !== "delivery" ? " · " + j.department : "") + " · no region", "conveyor"], ["charter", "purpose, outcomes, non-goals, escalation", j.charter, "conveyor (release instruction)"], ["capability binding", "one or more capabilities", j.capabilities.join(", "), "conveyor (job_binding)"], ["procedure_ref", "published procedure version(s)", j.procedure, "conveyor (procedure)"], ["executor", "procedure | internal_agent", j.executor, "conveyor (job_binding)"], ["model_ref, params", "inference-router model and parameters", j.model, "conveyor (release)"], ["prompt_recipe", "compiled-prompt recipe", j.recipe, "conveyor (release)"], ["knowledge pins", "layer kinds + capability pack", j.knowledge, "conveyor (release)"], ["memory policy", "recall scope, hot/cold, write rules", j.memory, "conveyor (release)"], ["tool grants", "operation allowlist + per-call gate", j.grants.join(", "), "conveyor (release)"], ["tool binding", "pinned toolkit versions, ceiling, credential", j.binding, "conveyor (tool_binding)"], ["decision options", "laya option sets and policies", "route · precheck", "conveyor (decision_*)"], ["limits", "max iterations, timeout, token/cost budget", j.limits, "conveyor (release)"], ["gates", "required approvals, dual control, evidence", j.gates, "conveyor (binding + policy)"], ["authority", "what the job may do without a gate", j.authority, "conveyor binding"], ["metrics", "computed from execution", j.metrics.join(" · "), "conveyor (read model)"]].map((r) => `<tr><td class="mono">${E(r[0])}</td><td>${E(r[1])}</td><td>${E(r[2])}</td><td><small>${E(r[3])}</small></td></tr>`).join("")}</tbody></table></div>${note("Rule: a job may lower a limit at run time, never raise it. The resolved <span class='mono'>{job_key, release_hash, procedure_ref, model_ref, allowed_actions}</span> travels in the run's input hash.")}`)}`
                : `${panel("Legacy import — not the live model", `<p class="muted">The S12 study carried 92 historical agent job seeds from the RM defaults and the earlier v4 registry. SPEC_V2 replaces that vocabulary with the conveyor's own <span class="mono">job_catalogue</span>: a person curates positions, publishes releases and binds them. The historical snapshot remains import material.</p>${kv([["Historical definitions", "92 RM/v4 entries"], ["Tenant examples", "6 design proposals"], ["Live model", "industry → department → jobs authored in T0/T3"], ["Use", "import reference; no historical job is automatically live"]])}${route("Open the historical job catalogue", "jobtypes", "small", "board")}${note("A draft catalogue may be generated from the derivation chain; a person curates it into published jobs. V2 does not auto-generate a thousand live jobs from an ontology.")}`)}`;
    return `<div class="page enterprise">${heading("Jobs", "The conveyor-owned catalogue of positions: placement, anatomy, immutable releases and tenant-scoped bindings.", `${route("Organisation", "organisation", "", "layers")}${route("Procedures", "procedures", "", "flow")}`, "ORGANISATION")}${ribbon("V2 job spine: industry, department, job_catalogue, job_release and job_binding. Proposed in T0; the historical 92-entry snapshot is import material only.", count.jobs > 0)}${tabs(["Catalogue", "Departments", "Releases", "Bindings", "Anatomy", "Legacy import"], tab, "jobs")}${content}</div>`;
  }

  /* ------------------------------------------------------------------ Procedures */
  function procedures() {
    const tab = v.tab.procedures || "Workspace";
    const sop = F.sops[v.sop] || F.sops[0];
    const pko = F.pko;
    const capRows = db.capabilities.slice(0, 5).map((c) => {
      const n = (c.processes || []).length;
      return `<div class="v2-step"><span class="n">${I("flow")}</span><span class="grow"><strong>${E(c.name)}</strong><small>${n} procedure(s) · ${E(c.knowledge || "knowledge pin required")}</small></span>${B(c.status, c.status === "Published" ? "green" : "amber")}</div>`;
    }).join("");
    const sopRows = F.sops.map((s, i) =>
      `<tr class="${i === v.sop ? "selected" : ""}"><td><button class="e-project-link" data-v2="sop" data-index="${i}"><span><strong>${E(s.name)}</strong><small>${E(s.source)}</small></span></button></td><td>${E(s.provenance)}</td><td>${s.pages}</td><td>${B(s.state, s.state === "Published" ? "green" : "amber")}</td></tr>`
    ).join("");
    const term = pko.entities.find((e) => e.id === v.pkoTerm) || pko.entities[0];
    const detail = pkoDetail[term.id] || {};
    const pkoRows = pko.entities
      .filter(
        (e) =>
          (v.pkoKind === "all" || e.kind === v.pkoKind) &&
          (!v.pkoQuery ||
            (
              e.name +
              " " +
              e.id +
              " " +
              ((pkoDetail[e.id] || {}).definition || "")
            )
              .toLowerCase()
              .includes(v.pkoQuery)),
      )
      .map(
        (e) =>
          `<tr class="${e.id === term.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="pko-term" data-id="${e.id}"><span><strong class="mono">${E(e.name)}</strong><small>${E(e.id)}</small></span></button></td><td>${B(e.kind)}</td><td>${E(e.attrs)}</td><td>${E((pkoDetail[e.id] || {}).owner || "—")}</td></tr>`,
      )
      .join("");
    const related = pko.relations.filter(
      (r) => r[0] === term.id || r[2] === term.id,
    );
    const relRows = related
      .map(
        (r) =>
          `<tr><td class="mono">${E(r[0])}</td><td>${E(r[1])}</td><td class="mono">${E(r[2])}</td><td>${E(r[3])}</td></tr>`,
      )
      .join("");
    const permitted = Object.entries(pkoPredicates)
      .filter(([, p]) => p.permitted)
      .map(([k, p]) => `${k} (${p.cardinality})`)
      .join(" · ");
    const graph = related
      .map((r) => {
        const otherId = r[0] === term.id ? r[2] : r[0];
        const other = pko.entities.find((e) => e.id === otherId) || {
          name: otherId,
          kind: "unknown",
        };
        return `<div class="v2-pko-edge"><span class="mono">${E(r[1])}</span><div class="node"><strong>${E(other.name)}</strong><small>${E(other.kind)}</small></div></div>`;
      })
      .join("");
    const formal = `<div class="v2-code">entity ${E(term.id)} {
  name: ${E(term.name)}
  kind: ${E(term.kind)}
  attributes: ${E(term.attrs)}
  synonyms: ${E(term.synonyms)}
  constraints: ${E((detail.constraints || []).join(", "))}
  release: ${E(detail.release || pko.release)}
}</div>`;
    const stepRows = [
      "01 · read the card and pinned sources (action)",
      "02 · draft the change (internal_agent · generation)",
      "03 · run tests (tool call)",
      "04 · independent review (gate)",
      "05 · merge (gate · human)",
    ].map((s) => {
      const [n, ...rest] = s.split(" · ");
      return `<div class="v2-step"><span class="n">${E(n)}</span><span class="grow">${E(rest.join(" · "))}</span></div>`;
    }).join("");
    let content = "";
    if (tab === "Workspace") {
      content =
        flow([
          ["Capability", "what the tenant can do"],
          ["Procedure", "published, versioned, pinned"],
          ["Step", "PDDL action | tool call | gate"],
          ["PKO", "procedural knowledge terms"],
          ["Knowledge pack", "pinned in strata_rag"],
        ]) +
        panel("Capability → procedure workspace", capRows, button("Open the procedure editor", "navigate", 'data-page="processes"', "small", "flow")) +
        note("Procedure design and management is first-class (D47): ordered steps, each an action (PDDL), a tool call or a gate. A published procedure is immutable; editing creates a draft and a gate.");
    } else if (tab === "SOP intake") {
      const detail =
        kv([
          ["Source", E(sop.source)],
          ["Provenance", E(sop.provenance)],
          ["State", B(sop.state, sop.state === "Published" ? "green" : "amber")],
          ["Outputs", "PKO rows · PDDL procedure draft · knowledge pack pairing"],
        ]) +
        note("A customer document is untrusted input: it enters as <span class='mono'>governed_asset(kind='sop_source')</span> with provenance, never as instructions to the system.") +
        button("Publish this procedure", "sop-publish", "", "small", "shield") +
        (v.review === "sop-published"
          ? note("<strong>Published behind the review gate</strong> · the procedure release names the PKO pin and the knowledge pack. A batch never publishes itself.", "ok")
          : v.review === "sop-refused"
            ? note("<strong>422 review_required</strong> · the SOP-derived artefacts are still drafts. An unreviewed SOP never binds a job.", "bad")
            : "");
      content =
        `<div class="v2-split"><div>${panel("SOP intake · single or batch", table(["SOP", "Source", "Pages", "State"], [sopRows]), `${button("Start batch intake", "sop-intake", "", "small", "plus")}${button("Advance conversion", "sop-advance", "", "small", "replay")}`)}</div><div>${panel(E(sop.name), detail)}</div></div>`;
    } else if (tab === "RDF/OWL & process docs") {
      content =
        panel("Ingestion paths", `<p class="muted">Documents are ingest material; the converter produces draft rows behind a review gate. The conveyor runs no RDF store and no reasoner.</p><div class="e-table-scroll"><table class="v2-matrix"><thead><tr><th>Input</th><th>Conversion</th><th>Produces</th><th>Gate</th></tr></thead><tbody><tr><td>RDF / OWL / Turtle</td><td>entity + relation extraction</td><td>pko_entity · pko_relation rows</td><td>per-asset review</td></tr><tr><td>BPMN 2.0 XML</td><td>activities → steps; gateways → gates</td><td>PDDL procedure draft + PKO rows</td><td>review · publish</td></tr><tr><td>Visio export</td><td>activities → steps; gateways → gates</td><td>procedure draft + PKO rows</td><td>review · publish</td></tr><tr><td>draw.io export</td><td>activities → steps; gateways → gates</td><td>procedure draft + PKO rows</td><td>review · publish</td></tr><tr><td>Existing policy / procedure export</td><td>text curation</td><td>procedure or policy draft</td><td>review · publish</td></tr></tbody></table></div>`, button("Convert the sample BPMN", "convert-doc", "", "small", "replay")) +
        note("The diagram is a source document; the procedure is the executable artefact. The same review gate applies to a single document and a batch.");
    } else if (tab === "PDDL steps") {
      content =
        panel("Action space · PDDL", `<div class="v2-code">(:action implement_change
  :parameters (?job - role ?card - work_package)
  :precondition (and (approved ?card) (knowledge-ready delivery))
  :effect (and (implemented ?card) (evidence-required ?card)))</div>${note("A step references PKO terms; a PKO entity never contains an executable step. The action space and the procedural knowledge space are separate concepts (D53).")}${button("Validate the step", "pddl-check", "", "small", "check")}`, B("procedure v7 · PDDL", "green")) +
        panel("Steps in proc-implement v7", `${stepRows}${route("Open the process editor", "processes", "small", "flow")}`);
    } else if (tab === "PKO editor") {
      content =
        `<div class="v2-split"><div>${panel(`PKO release ${pko.release}`, `<p class="muted">Relational, conveyor-owned and tenant-editable: an immutable <span class="mono">pko_release</span> pins entities and relations; layered base → industry → department → job.</p><div class="toolbar"><span class="mono muted">KIND</span>${["all", "task", "role", "artefact", "constraint"].map((k) => `<button class="btn ${v.pkoKind === k ? "selected" : ""}" data-v2="pko-kind" data-id="${k}">${E(k)}</button>`).join("")}<label class="input-search">${I("search")}<input data-v2-input="pko-search" placeholder="Search terms…" value="${E(v.pkoQuery)}" aria-label="Search PKO terms"></label></div>${table(["Entity", "Kind", "Attributes", "Owner"], [pkoRows])}`, button("Edit an entity → new release", "pko-edit", "", "small", "edit"))}</div><div>${panel(
          "Term · " + term.name,
          `${kv([
            ["Definition", E(detail.definition || "No definition recorded.")],
            ["Kind", B(term.kind)],
            ["Attributes", E(term.attrs)],
            ["Synonyms", E(term.synonyms)],
            ["Constraints", E((detail.constraints || []).join(" · ") || "none")],
            ["Source evidence", E(detail.evidence || "—")],
            ["Owner", E(detail.owner || "—")],
            ["Release", `<span class="mono">${E(detail.release || pko.release)}</span>`],
            ["Permitted relationships", E(permitted)],
          ])}${button(v.pkoFormal ? "Hide formal representation" : "Show formal representation", "pko-formal", "", "small", "code")}${v.pkoFormal ? formal : ""}${note("Every edit creates a draft release behind a review gate; a published release is never mutated.")}${button("Validate the term", "pko-validate", "", "small", "check")}${button("Attempt an invalid relation", "pko-add-relation", "", "small", "plus")}`,
          B(term.id, "purple"),
        )}</div></div><div class="v2-split"><div>${panel(
          "Relations for " + term.name,
          `${table(["Subject", "Predicate", "Object", "Cardinality"], [relRows])}${note("Permitted predicates: " + E(permitted) + ". An invalid relation is refused with <span class='mono'>422 pko_relation_invalid</span>; a term with no definition refuses publication.")}${button("Check publish readiness", "pko-publish-check", "", "small", "shield")}`,
        )}</div><div>${panel(
          "Usage & local graph",
          `<h3>Procedure steps using this term</h3><ul class="v2-used-by">${(detail.steps || []).map((s) => `<li>${E(s)}</li>`).join("")}</ul><h3>Relationships · 2 hops</h3><div class="v2-pko-graph" aria-label="Local PKO relationship graph"><div class="node selected"><strong>${E(term.name)}</strong><small>${E(term.kind)} · ${E(term.id)}</small></div>${graph}</div>${note("The graph is a local study. The table above is the authoritative accessible view; the conveyor runs no graph store or reasoner.")}${button("Evaluate this term", "pko-eval", "", "small", "chart")}`,
        )}</div></div>`;
    } else if (tab === "Knowledge pairing") {
      content =
        panel("Procedure ↔ knowledge pairing", `${flow([["Procedure v7", "pinned"], ["PKO release", pko.release], ["Domain pack", "capability@17 · strata_rag"], ["Run", "pins both"]])}${kv([["Domain pack", "capability@17 · published · required"], ["PKO release", `${pko.release} · immutable`], ["Readiness", v.packReady ? B("ready", "green") : B("blocked · capability_not_ready", "amber")], ["Retrieval", "pinned leaf ids · strata_rag owns embeddings"], ["Consolidation", "one cold digest per card in strata_rag"]])}${note("The procedure's release names both the procedural knowledge (PKO) and the domain knowledge pack. The conveyor never hosts embeddings or a model; retrieval is strata_rag.")}${route("Open knowledge design", "knowledge-design", "small", "branch")}`, B("pairing required", "green"));
    } else if (tab === "Process map") {
      content =
        panel("Process map", `${flow([["Intent", "card"], ["Activities", "steps"], ["Gateways", "approvals"], ["Evidence", "run + artefact"]])}${note("A BPMN diagram becomes a procedure draft: activities → steps, gateways → gates. The diagram is a source document, not the executable artefact.")}${route("Open the existing process map", "processes", "small", "flow")}`);
    } else {
      const checks = [
        "Review gate: human reviewer approved the draft",
        "PKO release pinned and reviewed",
        "Knowledge pack published and readable",
        "Tool grants unchanged by the SOP",
        "No SOP widened an effect ceiling",
      ].map((x, i) => {
        const mark = i === 2 && !v.packReady ? "—" : "✓";
        return `<div class="v2-step"><span class="n">${mark}</span><span class="grow">${E(x)}</span></div>`;
      }).join("");
      content =
        panel("Publish readiness", `${checks}${v.packReady ? note("All readiness checks pass. Publish opens a human gate; publication is not automatic.", "ok") : note("<strong>409 procedure_not_ready</strong> · the required knowledge pack capability@18 is unpublished. Publishing is refused before any run.", "bad")}`, `${button(v.packReady ? "Mark pack unpublished" : "Toggle pack availability", "toggle-pack", "", "small")}${button("Publish procedure v8", "publish-procedure", "", "primary", "shield")}`);
    }
    return `<div class="page enterprise">${heading("Procedures", "Capability → procedure → step, with SOP/RDF/OWL and BPMN/Visio/draw.io intake, the PDDL action space, the relational PKO and the knowledge pack pairing.", `${route("Capabilities", "capabilities", "", "layers")}${route("Authoring", "authoring", "", "book")}`, "PROCEDURES & AUTHORING")}${ribbon("V2 procedure spine: procedure/procedure_step with PDDL text and a PKO pin, pko_release/entity/relation, and the SOP→PKO→procedure conversion. Proposed in T16.")}${tabs(["Workspace", "SOP intake", "RDF/OWL & process docs", "PDDL steps", "PKO editor", "Knowledge pairing", "Process map", "Publish"], tab, "procedures")}${content}</div>`;
  }

  /* ------------------------------------------------------------------ Authoring */
  function authoring() {
    const tab = v.tab.authoring || "Intake queue";
    const selected = F.intakes.find((i) => i.id === v.intakeId) || F.intakes[0];
    const draft = F.drafts.find((d) => d.id === v.draftId) || F.drafts[2];
    let content = "";
    if (tab === "Intake queue") {
      const rows = F.intakes.map((i) =>
        `<div class="v2-step"><span class="n">${i.kind === "Chat" ? I("chat") : I("file")}</span><span class="grow"><strong>${E(i.id)} · ${E(i.kind)}</strong><small>${E(i.text)}</small><span class="mono muted">Produces: ${i.produces.map(E).join(" · ")}</span></span>${B(i.state, i.state === "In review" ? "amber" : "green")}</div>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Intake queue · chat + documents", `${rows}<div class="form-grid" style="margin-top:12px"><label class="field full"><span>Chat brief</span><textarea data-v2-input="brief" rows="3" placeholder="Describe the department, jobs, procedures or policies you need">${E(v.brief || "")}</textarea></label><label class="field full"><span>Document</span><input type="file" aria-label="Attach a document" disabled><small>Local study · no file leaves the browser</small></label></div>`, button("Draft from the brief", "intake-chat", "", "primary", "plus"))}</div><div>${panel("What the intake produces", `${flow([["Chat / document", "untrusted input"], ["Authoring job", "ordinary job"], ["Asset drafts", "review gate"], ["Publication", "a release and a gate"]])}${note("Every output is a draft behind a review gate (tenant-configurable, default human). A chat message or document can never grant a tool, widen an effect ceiling, approve its own artefact or change an approval policy.")}${note("Asset order is dependency-ordered: industry → department → job/capability → procedure/PKO → knowledge pack → policies/templates → tools. A draft whose referenced asset is unpublished is refused.")}`)}</div></div>`;
    } else if (tab === "Draft catalogue") {
      const rows = F.drafts.map((d) =>
        `<tr class="${d.id === draft.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="draft" data-id="${d.id}"><span><strong>${E(d.asset)}</strong><small class="mono">${E(d.id)}</small></span></button></td><td><small>${E(d.depends)}</small></td><td>${B(d.state, d.state === "Draft" ? "amber" : d.state === "Review" ? "purple" : "green")}</td><td>${E(d.review)}</td></tr>`
      ).join("");
      const detail =
        kv([
          ["Draft", draft.id],
          ["Depends on", E(draft.depends)],
          ["State", B(draft.state, "amber")],
          ["Review", E(draft.review)],
          ["Publication", "a release and a gate"],
        ]) +
        note("An authoring job refuses a draft whose referenced asset is unpublished. The dependency order is enforced, not advisory.") +
        (v.review === "asset-blocked"
          ? note("<strong>422 asset_dependency_unpublished</strong> · the knowledge pack in this chain is unpublished. The downstream draft cannot publish.", "bad")
          : v.review === "asset-gate"
            ? note("<strong>Review gate opened</strong> · the draft is not published until a person approves it.", "ok")
            : "");
      content =
        `<div class="v2-split"><div>${panel("Asset drafts", `${table(["Draft", "Depends on", "State", "Review"], [rows])}`, button("Publish the selected draft", "publish-asset", "", "small", "shield"))}</div><div>${panel(E(draft.asset), detail)}</div></div>`;
    } else if (tab === "Review & publish") {
      content =
        flow([
          ["Industry", v.packReady ? "published" : "draft"],
          ["Department", "draft"],
          ["Job / capability", "draft"],
          ["Procedure / PKO", "review"],
          ["Knowledge pack", "unpublished"],
          ["Policies / templates", "draft"],
          ["Tools", "request only"],
        ]) +
        panel("Review depth per asset", `${table(["Asset class", "Review", "Publication"], [["Organisation (industry, department)", "human · tenant admin", "release + gate"], ["Job catalogue / job draft", "human · department owner", "release + binding"], ["Capability / procedure / PKO", "human · domain reviewer", "versioned release"], ["Knowledge pack", "human · knowledge owner", "pinned pack version"], ["Policy / approval", "human · governance", "gated policy change"], ["Tool request", "security review", "tool publication"]].map((r) => `<tr><td>${E(r[0])}</td><td>${E(r[1])}</td><td>${E(r[2])}</td></tr>`))}${note("A person reviews and publishes. The authoring job family is ordinary jobs on the two executors; the pipeline is a procedure, not a hard-coded wizard.")}`);
    } else {
      const rows = F.drafts.map((d) =>
        `<div class="v2-step"><span class="n">${d.state === "Unpublished" ? "!" : d.state === "Review" ? "…" : "✓"}</span><span class="grow"><strong>${E(d.asset)}</strong><small>depends on ${E(d.depends)} · ${E(d.state)}</small></span>${B(d.state, d.state === "Unpublished" ? "amber" : "green")}</div>`
      ).join("");
      content = panel("Dependency status", `${rows}${note("An authored catalogue is measured by use: drafts accepted vs rejected, revisions per artefact, time from intake to first published job. An asset no procedure uses is retired (P10).")}`, B("1 blocker", "amber"));
    }
    return `<div class="page enterprise">${heading("Authoring", "A tenant supplies chat input and documents; the conveyor drafts every tenant-scoped asset behind a review gate.", `${route("Procedures", "procedures", "", "flow")}${route("Organisation", "organisation", "", "layers")}`, "PROCEDURES & AUTHORING")}${ribbon("V2 tenant asset authoring (D55): intake queue, dependency-ordered asset drafts, review/publish gates and the authoring instruments. Proposed in T17.")}${tabs(["Intake queue", "Draft catalogue", "Review & publish", "Dependency status"], tab, "authoring")}${content}</div>`;
  }

  /* ------------------------------------------------------------------ Improve */
  function improve() {
    const tab = v.tab.improve || "Kata queue";
    const kata = F.katas.find((k) => k.id === v.kata) || F.katas[0];
    const lesson = F.lessons.find((l) => l.id === v.lesson) || F.lessons[1];
    let content = "";
    if (tab === "Kata queue") {
      const rows = F.katas.map((k) =>
        `<tr class="${k.id === v.kata ? "selected" : ""}"><td><button class="e-project-link" data-v2="kata" data-id="${k.id}"><span><strong>${E(k.title)}</strong><small class="mono">${E(k.id)} · ${E(k.tenant)}</small></span></button></td><td>${E(k.instrument)}</td><td>${E(k.threshold)}</td><td>${B(k.state, k.state === "Measuring" ? "amber" : "green")}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Kata queue", `${table(["Kata", "Instrument", "Success threshold", "State"], [rows])}`, button("Try opening a kata without an instrument", "kata-open", "", "small", "plus"))}</div><div>${panel(E(kata.title), `${kv([["Tenant", E(kata.tenant)], ["Instrument", E(kata.instrument)], ["Threshold", E(kata.threshold)], ["Pre-registered prediction", E(kata.prediction)], ["State", B(kata.state, "amber")]])}${note("A kata that cannot name an instrument and a success threshold is not opened. The kata is its own tenant, so its write scope never inherits the target tenant's autonomy (D45).")}${note("The kata cannot edit the instrument that judges it, cannot approve its own adoption and cannot measure on the training split only.")}`)}</div></div>`;
    } else if (tab === "Six Gets") {
      const rows = kata.six.map((s, i) =>
        `<div class="v2-step"><span class="n">${String(i + 1).padStart(2, "0")}</span><span class="grow"><strong>${E(s[0])}</strong><small>${E(s[1])}</small></span>${B(i < 3 ? "evidence" : "pending", i < 3 ? "green" : "amber")}</div>`
      ).join("");
      content =
        panel(`Six Gets · ${kata.id}`, `<p class="muted">Go to See → Grasp the Situation → Get to Solution → Get to Standardization → Get to Sustainability → Get to Stretch.</p>${rows}${flow([["Observation", "evidence"], ["Hypothesis", "pre-registered"], ["Candidate", "change"], ["Measurement", "held-out"], ["Adoption", "human gate"], ["Propagation", "yokoten"]])}`, B(kata.state, "amber")) +
        note("A failed kata keeps the andon cord until a person releases it. A kata that cannot measure is not opened.");
    } else if (tab === "Coach") {
      content =
        panel("Closed triage and remedy sets", `<div class="v2-split"><div>${kv([["Triage", "transient · conveyor_defect · job_defect · project_decision · external"], ["Remedy", "retry · move · repull · block · ask · note"], ["Laya decides", "triage class and remedy over the configured option set"], ["Below threshold", "confidence below policy → ask (human)"], ["LLM generates", "the diagnosis narrative only — never the remedy"]])}</div><div>${note("One coach card per interruption; one card per <span class='mono'>(card_id, signature)</span> while open. A second same-signature block escalates to a human, never to another coach card.")}${note("The coach organises; a work capability does the work. It never opens a coach card about a coach card.")}</div></div>`, button("Inject a repeat signature", "coach-repeat", "", "small", "replay")) +
        panel("Open interruptions", `${table(["Card", "Signature", "Triage", "Remedy", "State"], [["C-142", "stale_lease", "conveyor_defect", "retry", "Open"], ["C-143", "wip_breach", "project_decision", "ask", "Escalated"], ["C-144", "failed_run", "transient", "retry", "Closed"]].map((r) => `<tr><td class="mono">${E(r[0])}</td><td class="mono">${E(r[1])}</td><td>${E(r[2])}</td><td>${B(r[3])}</td><td>${E(r[4])}</td></tr>`))}`, B("per-signature uniqueness", "green")) +
        note("Measured lesson: the old open-ended coach LLM rung fixed 0 of 22 blocks, and duplicate coach cards made the board unreadable. The closed sets and uniqueness are the fix.");
    } else if (tab === "Lessons") {
      const rows = F.lessons.map((l) =>
        `<tr class="${l.id === lesson.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="lesson" data-id="${l.id}"><span><strong>${E(l.title)}</strong><small class="mono">${E(l.id)}</small></span></button></td><td>${l.independent}</td><td><small>${l.evidence.map(E).join(" · ")}</small></td><td>${B(l.state, l.state === "Blocked" ? "amber" : "green")}</td><td>${E(l.review)}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Lesson review", `${table(["Lesson", "Independent refs", "Evidence", "State", "Review"], [rows])}`, button("Promote the selected lesson", "lesson-promote", "", "small", "shield"))}</div><div>${panel("Promotion gate", `${kv([["Rule", "≥ 2 independent evidence refs + a person's review"], ["Selected", lesson.id], ["Evidence refs", lesson.evidence.length], ["Independent", lesson.independent], ["State", B(lesson.state, lesson.state === "Blocked" ? "amber" : "green")], ["Path", "proposed → reviewed → promoted / rejected"]])}${note("One bad run never becomes doctrine. A promoted lesson follows the normal source/pack release path.")}${v.review === "lesson-blocked" ? note("<strong>422 lesson_evidence_insufficient</strong> · one evidence reference is not enough. The lesson stays proposed.", "bad") : v.review === "lesson-gate" ? note("<strong>Review gate opened</strong> · a person reviews the lesson before a source draft is prepared.", "ok") : ""}`)}</div></div>`;
    } else {
      const rows = F.adoptions.map((a) =>
        `<tr><td class="mono">${E(a.id)}</td><td>${E(a.kata)}</td><td>${E(a.change)}</td><td>${E(a.measured)}</td><td>${B(a.gate, "amber")}</td><td>${B(a.state, a.state === "Pending" ? "amber" : a.state === "Reverted" ? "bad" : "green")}</td></tr>`
      ).join("");
      content =
        panel("Adoption", `${table(["Adoption", "Kata", "Change", "Measurement", "Gate", "State"], [rows])}`, `${button("Approve the pending adoption", "adopt", "", "small", "shield")}${button("Revert a training-only gain", "adopt-revert", "", "small", "replay")}`) +
        note("Measurement is held-out; a training-only gain is reverted and a regression triggers a revert. Adoption prepares a procedure/prompt/job version; it does not publish it or change live runs.") +
        note("The kata proposes; a person adopts. An adoption needs the human gate and a dual-control approver set where the tenant configures it.", "ok");
    }
    return `<div class="page enterprise">${heading("Improve", "The system-improvement loop: coach interruptions, kata six Gets, evidence-backed lessons and human adoption.", `${route("Measure", "usage", "", "chart")}${route("Operations", "operations", "", "run")}`, "MEASURE & IMPROVE")}${ribbon("V2 improvement loop: coach (closed sets + uniqueness), kata (six Gets, own tenant), lessons (≥2 independent refs) and adoption. Proposed in T4/T5/T8.")}${tabs(["Kata queue", "Six Gets", "Coach", "Lessons", "Adoption"], tab, "improve")}${content}<div class="toolbar" style="margin-top:16px"><span class="mono muted">V1 surface:</span>${route("Improvement campaigns", "improvement", "small", "chart")}${route("Evaluation studio", "evals", "small", "shield")}${route("Hillclimb lab", "hillclimb", "small", "branch")}</div></div>`;
  }

  /* ------------------------------------------------------------------ Operations */
  function operations() {
    const tab = v.tab.operations || "Incidents";
    const inc = F.incidents.find((i) => i.id === v.incident) || F.incidents[0];
    const rb = F.runbooks.find((r) => r.id === v.runbook) || F.runbooks[0];
    let content = "";
    if (tab === "Incidents") {
      const rows = F.incidents.map((i) =>
        `<tr class="${i.id === inc.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="incident" data-id="${i.id}"><span><strong>${E(i.id)} · ${E(i.service)}</strong><small>${E(i.symptom)}</small></span></button></td><td>${B(i.severity, i.severity === "sev1" ? "bad" : "amber")}</td><td>${E(i.detected)}</td><td>${E(i.resolved || "—")}</td><td>${B(i.state, i.state === "Closed" ? "green" : "amber")}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Incidents", `${table(["Incident", "Severity", "Detected", "Resolved", "State"], [rows])}`, button("Open an incident card", "gate", "", "small", "plus"))}</div><div>${panel(E(inc.id) + " · " + E(inc.service), `${kv([["Severity", `${B(inc.severity, inc.severity === "sev1" ? "bad" : "amber")} · laya confidence 0.86`], ["Detected", E(inc.detected)], ["Mitigated", E(inc.mitigated)], ["Resolved", E(inc.resolved)], ["State", B(inc.state, "amber")], ["Symptom", E(inc.symptom)], ["Runbook", `<span class="mono">${E(inc.runbook)}</span>`], ["Evidence", "run RUN-158 · logs · read-back checks"]])}${note("Severity and runbook selection are laya decisions over the configured option sets; below threshold escalates to on-call. A person can also open an incident.")}${route("Open the runbook", "operations", "small", "run")}`)}</div></div>`;
    } else if (tab === "On-call") {
      content =
        panel("On-call and escalation", `${flow([["Alert", "detect"], ["Laya triage", "severity + runbook"], ["On-call", "human"], ["Break-glass", "tenant-configured"], ["Post-hoc review", "mandatory"]])}${kv([["Default autonomy", "human approval"], ["Break-glass", "incident commander · tenant-configured · post-hoc review"], ["Escalation", "card → parent WBS → PM → programme; coach can escalate to a human"], ["Pages", "still a human decision"]] )}${note("The full write set ships in v1; autonomy is configuration, not a reduced feature. A destructive action without the tenant's configured approvals is refused.")}`) +
        note("The andon cord is an incident-class action: stop new runs for a tenant/programme, keep the record, alert the on-call; only a person releases it.");
    } else if (tab === "Runbooks") {
      const rows = F.runbooks.map((r) =>
        `<tr class="${r.id === rb.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="runbook" data-id="${r.id}"><span><strong>${E(r.id)}</strong><small>${E(r.name)}</small></span></button></td><td>${E(r.read)}</td><td>${E(r.write)}</td><td>${B(r.gate, "amber")}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Runbooks", `${table(["Runbook", "Read", "Write", "Gate"], [rows])}`, button("Dry-run the selected runbook", "dry-run", "", "small", "replay"))}</div><div>${panel(E(rb.name), `${kv([["Read verbs", E(rb.read)], ["Write verb", E(rb.write)], ["Effect class", "destructive"], ["Gate", E(rb.gate)], ["Service account", "namespace-scoped · never cluster-admin"], ["Audit", "one tool_call row + one audit event"]])}${button("Request the destructive write", "destructive", "", "small", "shield")}${button("Execute without approval", "blocked", 'data-msg="403 ops:write required · dry-run + dual-control approval + read-back"', "small", "close")}${v.review === "dry-run" ? note("<strong>Dry-run passed</strong> · read-back checks declared. The write still needs the tenant's configured approvals.", "ok") : ""}`)}</div></div>`;
    } else if (tab === "Read & write") {
      content =
        panel("Kubernetes and git verbs", `${table(["Verb", "Effect class", "Gate"], [["pods · deployments · events · logs · metrics · Flux status · git/CI status", "read", "none · audit only"], ["rollout restart", "destructive", "dry-run + dual control + read-back"], ["scale", "destructive", "dry-run + dual control + read-back"], ["rollback", "destructive", "dry-run + dual control + read-back"], ["feature-flag flip", "destructive", "dry-run + dual control + read-back"], ["PR / branch / merge", "write | destructive", "job authority · merge gate"]] .map((r) => `<tr><td>${E(r[0])}</td><td>${effects(r[1].split(" | "))}</td><td>${E(r[2])}</td></tr>`))}${note("The ops connector uses a namespace-scoped service account, never cluster-admin. The action registry declares every verb; the tool version carries the effect class, not the HTTP method.")}`, B("read-first", "green"));
    } else if (tab === "Postmortems") {
      content =
        panel("Postmortems", `${table(["Postmortem", "Incident", "Findings", "Outcome", "State"], F.postmortems.map((p) => `<tr><td class="mono">${E(p.id)}</td><td>${E(p.incident)}</td><td>${E(p.finding)}</td><td>${E(p.outcome)}</td><td>${B(p.state, p.state === "Published" ? "green" : "amber")}</td></tr>`))}`, button("Publish postmortem", "postmortem", "", "small", "shield")) +
        note("A postmortem is a governed asset: blameless, evidence-linked and due within the policy window. A systemic finding becomes a kata candidate; a one-off becomes a runbook improvement.") +
        note("The andon cord remains pulled until a person releases it; a failed kata keeps it too.", v.andon ? "bad" : "ok");
    } else {
      content =
        panel("Andon cord", `${note(v.andon ? "<strong>ANDON PULLED</strong> · every n8n-style execution and every new conveyor run for the tenant/programme is stopped. The record is kept and the on-call is alerted. Only a person releases it." : "<strong>Andon released</strong> · new runs are admitted again. The release is a person's decision and is recorded.", v.andon ? "bad" : "ok")}${kv([["Scope", "tenant or programme"], ["Effect", "stops new runs · keeps the record · alerts on-call"], ["Release", "a person only"], ["Related", "a failed kata keeps the cord until released"]])}`, button(v.andon ? "Release the andon cord" : "Pull the andon cord", "andon-toggle", "", "small", "shield")) +
        note("The andon cord is an incident-class action, not a pause button for one run. It is the estate-wide stop in V1 and remains a person-only release in V2.");
    }
    return `<div class="page enterprise">${heading("Operations", "Incidents, on-call, runbooks, read-first k8s/git actions and postmortems — the platform perspective as a job family.", `${route("Improve", "improve", "", "chart")}${route("Tools", "toolkits", "", "tools")}`, "OPERATIONS")}${ribbon("V2 operations job family: incident cards, runbooks as procedures, the gated write set, postmortems and the andon cord. Proposed in T7.")}${tabs(["Incidents", "On-call", "Runbooks", "Read & write", "Postmortems", "Andon"], tab, "operations")}${content}</div>`;
  }

  /* ------------------------------------------------------------------ Tools */
  function toolkits() {
    const tab = v.tab.toolkits || "Toolkits";
    const tk = toolkit();
    const tb = F.toolBindings.find((b) => b.job === job().id) || F.toolBindings[0];
    const call = F.toolCalls.find((c) => c.id === v.callId) || F.toolCalls[0];
    let content = "";
    if (tab === "Toolkits") {
      const rows = F.toolkits.map((t) =>
        `<tr class="${t.key === v.toolkit ? "selected" : ""}"><td><button class="e-project-link" data-v2="toolkit" data-key="${t.key}"><span><strong>${E(t.key)}@${t.version}</strong><small>${E(t.service)}</small></span></button></td><td>${t.ops}</td><td>${B(t.status, t.status === "live" ? "green" : "amber")}</td><td>${B(v.probe[t.key] === "failed" ? "failed" : t.probe, v.probe[t.key] === "failed" ? "bad" : t.probe === "green" ? "green" : "amber")}</td><td>${E(t.licence)}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Toolkits", `${table(["Toolkit", "Ops", "Status", "Probe", "Licence app"], [rows])}`)}</div><div>${panel(E(tk.key) + "@" + tk.version, `${kv([["Service", E(tk.service)], ["Operations", tk.ops], ["Status", B(tk.status, tk.status === "live" ? "green" : "amber")], ["Probe", B(v.probe[tk.key] || tk.probe, "green")], ["Licence app", E(tk.licence)], ["Immutability", "published key@version · never patched in place"]])}${note(E(tk.note))}${note("A toolkit is an immutable, versioned bundle of one service's tools plus an auth adapter, licence app and destination. A port is not an identity: 8812 is two distinct services on different hosts.")}`)}</div></div>`;
    } else if (tab === "Bindings & grants") {
      const rows = F.toolBindings.map((b) =>
        `<tr class="${b.id === tb.id ? "selected" : ""}"><td class="mono">${E(b.id)}</td><td>${E(b.job)}</td><td class="mono">${E(b.pin)}</td><td>${B(b.ceiling, b.ceiling === "destructive" ? "bad" : b.ceiling === "write" ? "purple" : "green")}</td><td>${E(b.credential)}</td><td>${E(b.rate)}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("Tool bindings", `${table(["Binding", "Job", "Pinned toolkit", "Ceiling", "Credential alias", "Rate"], [rows])}`)}</div><div>${panel("Binding · " + tb.id, `${kv([["Job", E(tb.job)], ["Pinned toolkit", `<span class="mono">${E(tb.pin)}</span>`], ["Effect ceiling", B(tb.ceiling, "purple")], ["Credential alias", `<span class="mono">${E(tb.credential)}</span>`], ["Rate", E(tb.rate)], ["Grant", "the release's operation allowlist (inside the release hash)"]])}${note("The release's grant names what the job may use; the binding pins toolkit versions and the credential alias. Both are conveyor rows; neither widens the other.")}<div class="toolbar"><span class="mono muted">TRY A CEILING</span>${["read", "write", "destructive", "spend"].map((l) => button(l, "ceiling", `data-level="${l}"`, "small")).join("")}</div>${v.review === "ceiling" ? note("<strong>403 tool_effect_forbidden</strong> · the binding narrows only. A job may lower its effect class at run time, never raise it.", "bad") : ""}`)}</div></div>`;
    } else if (tab === "Calls ledger") {
      const rows = F.toolCalls.map((c) =>
        `<tr class="${c.id === call.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="call" data-id="${c.id}"><span><strong class="mono">${E(c.tool)}</strong><small>${E(c.id)} · ${E(c.run)} / step ${E(c.step)}</small></span></button></td><td class="mono">${E(c.version)}</td><td>${B(c.status, c.status === "ok" ? "green" : "amber")}</td><td>${E(c.latency)}</td><td>${E(c.bytes)}</td><td>${E(c.cost)}</td></tr>`
      ).join("");
      content =
        `<div class="v2-split"><div>${panel("tool_call ledger", `${table(["Tool", "Version", "Status", "Latency", "Bytes", "Cost"], [rows])}`, B(call.id, "green"))}</div><div>${panel("Call · " + call.id, `${kv([["Run / step", `${E(call.run)} / ${E(call.step)}`], ["Tool", `<span class="mono">${E(call.tool)}</span>`], ["Tool version", E(call.version)], ["Status", B(call.status, "green")], ["Latency", E(call.latency)], ["Bytes", E(call.bytes)], ["Cost", E(call.cost)], ["Input hash", "sha256:71b2…"], ["Credential", "injected by the broker · never recorded"], ["Envelope", "{status, data, checks[], next[]}"]])}${note("One <span class='mono'>tool_call</span> row and one <span class='mono'>tool.called</span> event. A tool call is not an <span class='mono'>action_invocation</span> and never transitions a card.")}`)}</div></div>`;
    } else if (tab === "Probes") {
      const rows = F.toolkits.map((t) =>
        `<tr><td class="mono">${E(t.key)}@${t.version}</td><td>${B(v.probe[t.key] === "failed" ? "failed" : t.probe, v.probe[t.key] === "failed" ? "bad" : t.probe === "green" ? "green" : "amber")}</td><td>${v.probe[t.key] === "failed" ? note("New runs unbind (409 tool_not_ready); in-flight runs finish.", "bad") : "<small>Read-only health probe</small>"}</td><td>${button(v.probe[t.key] === "failed" ? "Restore probe" : "Simulate probe failure", "probe", `data-key="${t.key}"`, "small", "replay")}</td></tr>`
      ).join("");
      content =
        panel("Health probes", `${table(["Toolkit", "Probe", "Effect", "Action"], [rows])}${note("Every published tool has a <span class='mono'>tools.probe</span> (read-only). A probe failure unbinds the tool from new runs; in-flight runs finish. A schema-drift signature opens a kata, not a silent retry.")}${note("A tool with zero calls over one programme cycle is flagged and retired (P10).")}`, B("per toolkit", "green"));
    } else if (tab === "BYO import") {
      content =
        `<div class="v2-split"><div>${panel("OpenAPI toolkit", `${kv([["Document", "OpenAPI 3.1"], ["Effect class", "default read; per operation"], ["Destination", "tenant-allowlisted base URL"], ["Credential", "sealed profile by alias"], ["Publication", "tenant-admin approval + contract test"]])}${button("Import as a draft toolkit", "byo-import", "", "small", "plus")}`)}</div><div>${panel("Remote MCP server", `${kv([["Endpoint", "streamable-HTTP MCP"], ["Discovery", "binding-scoped"], ["Exposure", "only the bound subset"], ["Credential", "sealed profile by alias"], ["Promotion", "a kata/development project with platform review"]])}${button("Import as a draft MCP binding", "byo-import", "", "small", "plus")}`)}</div></div>` +
        note("A tenant may bring two kinds of tool and only two: an OpenAPI toolkit and a remote MCP server. Never uploaded code, arbitrary container images, caller-supplied URLs at call time, or a tenant credential in a prompt. The conveyor never executes tenant code.") +
        note("A BYO destination is tenant-allowlisted at registration; a binding's effect ceiling narrows only. Promotion to the platform catalogue is never an admin toggle.");
    } else if (tab === "Tool authoring") {
      content =
        panel("Three governed generation paths", `${flow([["Spec → toolkit", "OpenAPI document"], ["Service → toolkit", "scaffold + register"], ["Missing op → tool request", "card → implementation"]])}${table(["Stage", "Control"], [["Draft", "governed_asset(kind='tool_draft')"], ["Validation", "schema, effect, egress, credential scope"], ["Security review", "a destructive tool needs a named approver"], ["Publication", "tools.publish · immutable version"], ["Binding", "tool_bindings.create · ceiling narrows"]].map((r) => `<tr><td>${E(r[0])}</td><td>${E(r[1])}</td></tr>`))}${note("A tool cannot be published with wildcard egress, a free-form input schema, or a write effect without an idempotency declaration. The publishing job cannot widen its own effect ceiling.")}`, button("Publish a tool draft", "tool-publish", "", "small", "shield")) +
        note("Tool code lives in the conveyor monorepo (D58): a new tool is a crate or service inside the workspace. A legacy Python service stays external and bindable until it is rewritten in Rust.");
    } else if (tab === "Storage") {
      content =
        panel("MinIO storage adapter", `${flow([["Job", "asks for bytes"], ["Adapter", "tenant prefix"], ["MinIO", "shared platform"], ["Read-back", "content hash + size"]])}${kv([["Prefix", "tenant/<tenant_id>/project/run"], ["Encryption", "server-side"], ["Integrity", "content hash per object"], ["Cap", "size cap enforced"], ["Credentials", "sealed broker profile · a job never sees an S3 key"], ["Bucket admin", "never a job action"]])}${note("The conveyor does not depend on artifact_service or storage-api for its own bytes (D57). They keep serving their existing consumers; the conveyor keeps no compatibility shim by default.")}`, B("D57", "green")) +
        note("Artifacts written by a run are tenant-separated and hashed; the run inspector links the object, never an S3 credential.");
    } else {
      const rows = F.credentials.map((c) =>
        `<tr><td class="mono">${E(c.alias)}</td><td class="mono">${E(c.secret)}</td><td>${E(c.holder)}</td><td class="mono">${E(c.fingerprint)}</td><td>${B(c.state, c.state === "Configured" ? "green" : "amber")}</td></tr>`
      ).join("");
      content =
        panel("Credential profiles", `${table(["Alias", "Sealed secret", "Holder", "Fingerprint", "State"], [rows])}${note("The broker resolves the credential by alias and injects it into the call. The prompt never carries it and the <span class='mono'>tool_call</span> row never records it. Only metadata is read back; no action reveals a value.")}${note("A missing credential is a refusal, never a silent fallback. Tenant BYO tokens live in the conveyor database; deployment configuration holds service base URLs, not secrets.")}`, B("metadata only", "green")) +
        note("V1's wired names are strata_ingest.token, strata_rag.token and laya.token; the planned services are listed with disabled controls until their consumers exist.");
    }
    return `<div class="page enterprise">${heading("Tools", "The governed tool estate: toolkit versions, bindings, effect ceilings, the call ledger, probes, BYO tools, authoring and direct MinIO storage.", `${route("Organisation", "organisation", "", "layers")}${route("Operations", "operations", "", "run")}`, "ORGANISATION")}${ribbon("V2 tool estate: tool, toolkit, tool_binding, tool_call, the broker, effect classes and the MinIO adapter. Proposed in T13/T14/T15; V1's twelve external actions are the seed.")}${tabs(["Toolkits", "Bindings & grants", "Calls ledger", "Probes", "BYO import", "Tool authoring", "Storage", "Credential profiles"], tab, "toolkits")}${content}<div class="toolbar" style="margin-top:16px"><span class="mono muted">V1 surface:</span>${route("Coding action catalogue (367 operations)", "tools", "small", "code")}</div></div>`;
  }

  /* ------------------------------------------------------------------ Metering */
  function metering() {
    const tab = v.tab.metering || "Roll-ups";
    const levels = ["tenant", "department", "programme", "project", "job", "run", "step", "call"];
    const m = F.meter[v.meterLevel] || F.meter.tenant;
    let content = "";
    if (tab === "Roll-ups") {
      const rows = [
        ["Tenant", "18.4M", "$412.80", "$38.10", "62%"],
        ["Department / industry — delivery (primary)", "11.2M", "$251.40", "$26.00", "58%"],
        ["Programme — Connected public services", "9.6M", "$214.20", "$21.40", "54%"],
        ["Project — Identity & access", "3.1M", "$68.40", "$7.20", "41%"],
        ["Job — software.delivery.backend_engineer", "1.4M", "$31.10", "$4.00", "37%"],
        ["Run — RUN-151", "184k", "$4.12", "$0.42", "18%"],
        ["Step — RUN-151 / 04", "62k", "$1.48", "$0.11", "—"],
        ["Call — TC-9002", "—", "$0.011", "$0.011", "—"],
      ].map((r) => `<tr><td>${E(r[0])}</td><td>${r[1]}</td><td>${r[2]}</td><td>${r[3]}</td><td>${r[4]}</td></tr>`).join("");
      content =
        `<div class="v2-meter"><div><span>Tenant spend</span><strong>${E(m.cost)}</strong><small>${m.tokens} tokens · ${m.budget} of budget</small></div><div><span>Tool spend</span><strong>${E(m.tools)}</strong><small>derived from tool_call</small></div><div><span>Selected level</span><strong>${E(v.meterLevel)}</strong><small>${E(m.label || "whole tenant")}</small></div></div>` +
        `<div class="toolbar" style="margin:16px 0"><span class="mono muted">DRILL LEVEL</span>${levels.map((l) => button(l, "meter", `data-level="${l}"`, v.meterLevel === l ? "selected" : "small")).join("")}</div>` +
        panel("One derivation, every level", `${table(["Level", "Tokens", "Cost", "Tool spend", "Budget"], [rows])}${note("Spend and usage are computed from the ledgers at call grain and presented at every level. Cost is a read-time derivation from <span class='mono'>inference_call</span> and <span class='mono'>tool_call</span>; tokens are stored, money is derived (D37).")}${note("A budget is enforced before the next call (<span class='mono'>429 budget_exceeded</span>), not reported after the fact.")}`, B("D52", "green"));
    } else if (tab === "Attribution") {
      content =
        flow([["Call", "inference_call / tool_call"], ["Run", "run"], ["Card", "work package"], ["Project", "card.project_id"], ["Programme", "project.programme_id"], ["Tenant", "tenant_id"]]) +
        panel("Attribution rules", `${table(["Rule", "Behaviour"], [["Call → run → card → project → programme → tenant", "the join is internal; no external caller declares it"], ["Job attribution", "the run's job_binding_id"], ["Department attribution", "the binding's primary department, with a breakdown by the others"], ["Unattributed call", "caller_kind='other' is reported separately, never silently dropped"], ["Billing", "billing_service remains the charging system; a divergence opens an incident, not a silent difference"], ["Licences", "licenses holds entitlement, not usage"]].map((r) => `<tr><td>${E(r[0])}</td><td>${E(r[1])}</td></tr>`))}${note("The conveyor's metering reconciles to billing_service at tenant × day × project × actor × app × provider × model. A divergence is an incident.")}`, B("unattributed: 0.4%", "amber"));
    } else {
      content =
        panel("Metrics and maturity", `${table(["Family", "Computed read model", "Source rows"], F.metrics.map((r) => `<tr><td><strong>${E(r[0])}</strong></td><td>${E(r[1])}</td><td><small>${E(r[2])}</small></td></tr>`))}${note("Metrics are computed from rows, never stored as opinions. The C0–C5 thresholds are calibrated from real run/approval data and stored as configuration (D50), not hard-coded.")}${note("A gate that has never refused anything in a full programme cycle is flagged by the governance audit — a broken gate, not a healthy one.", "bad")}`, B("C2", "green")) +
        panel("Reconciliation", `${kv([["Conveyor metering", "$412.80 tenant total"], ["billing_service", "$412.80 · exact"], ["Divergence policy", "open an incident, never a silent difference"], ["Price version", "pinned per read"], ["Attribution gap", "0.4% · reported separately"]])}${note("The same derivation feeds the API and the UI; a panel never recomputes cost from a stale cache.")}`);
    }
    return `<div class="page enterprise">${heading("Metering", "Roll-ups at tenant, department, programme, project and job; drill-down to run, step and tool call; one derivation reconciled with billing.", `${route("Portfolio", "portfolio", "", "layers")}${route("Measure", "usage", "", "chart")}`, "MEASURE & IMPROVE")}${ribbon("V2 metering (D52): read models over inference_call and tool_call, attribution, C0–C5 metrics and billing reconciliation. Proposed in T11.", count.calls > 0)}${tabs(["Roll-ups", "Attribution", "Metrics & maturity"], tab, "metering")}${content}</div>`;
  }

  const pages = {
    portfolio,
    plan,
    organisation,
    jobs,
    procedures,
    authoring,
    improve,
    operations,
    toolkits,
    metering,
    // The architecture map is a design-study destination, not a V2 aggregate: it
    // draws the spec and the destination map. Its markup lives in architecture.js.
    architecture: () => window.ArchitecturePack.page(api),
    // The tracked Markdown (spec, plans, evidence, decisions) rendered at build time.
    documents: () => window.DocsPack.page(api),
    // V1 surfaces retained, with a banner that points at the V2 destination.
    overview: wrapLegacy(
      legacy.overview,
      "Programme control is the V1 programme surface. V2 adds the computed Portfolio and the Plan; open Portfolio for the in-flight set.",
    ),
    jobtypes: wrapLegacy(
      legacy.jobtypes,
      "Historical agent job catalogue — import material, not the live model. Open Jobs for the V2 catalogue, releases and bindings.",
    ),
    improvement: wrapLegacy(
      legacy.improvement,
      "V1 improvement campaigns. The V2 Improve destination adds the coach, kata six Gets, lessons and adoption.",
    ),
    tools: wrapLegacy(
      toolPage,
      "V1 coding-action catalogue — reference for the coding toolkit import. n8n is reference only, never a tool. Open Tools for toolkits, bindings and the call ledger.",
    ),
    constraints: wrapLegacy(
      legacy.constraints,
      "The TOC graph is an optional constraint-reasoning study. V2's method is SAFe/PMP/TOGAF plus the coach's closed triage and remedy sets.",
    ),
    knowledge: wrapLegacy(
      legacy.knowledge,
      "V1 keeps the source, pack, retrieval and work-package memory path. The source transformation inspector, seven-state model, retrieval test and object-evaluation entry points are V2 · PROPOSED extensions; procedural knowledge adds the PKO pin and organisational memory adds reviewed lessons.",
    ),
    agents: wrapLegacy(
      () => cataloguePage("agents"),
      "A run has one of two executor kinds — procedure or internal_agent (D46). This page configures the internal agent.",
    ),
    execution: executionPage,
  };
  const titles = {
    portfolio: "Portfolio",
    plan: "Plan & WBS",
    organisation: "Organisation",
    jobs: "Jobs",
    procedures: "Procedures",
    authoring: "Authoring",
    improve: "Improve",
    operations: "Operations",
    toolkits: "Tools",
    metering: "Metering",
    architecture: "System architecture",
    documents: "Documents",
  };

  /* ------------------------------------------------------------------ events */
  document.addEventListener("change", (event) => {
    const planProject = event.target.closest('[data-v2-input="plan-project"]');
    if (planProject) {
      v.planProject = planProject.value;
      render();
      return;
    }
    const planSeedProject = event.target.closest('[data-v2-input="plan-seed-project"]');
    if (planSeedProject) {
      v.planSeedProject = planSeedProject.value;
      render();
      return;
    }
    const el = event.target.closest('[data-v2-input="pko-search"]');
    if (!el) return;
    v.pkoQuery = el.value.trim().toLowerCase();
    render();
  });
  document.addEventListener("click", (event) => {
    const el = event.target.closest("[data-v2]");
    if (!el) return;
    const d = el.dataset;
    switch (d.v2) {
      case "tab":
        v.tab[d.key] = d.value;
        render();
        break;
      case "programme":
        v.programme = d.id;
        render();
        break;
      case "job":
        v.job = d.id;
        render();
        break;
      case "dept":
        v.department = d.id;
        render();
        break;
      case "self-approve":
        toast("422 dual_control_required · the assignee cannot approve the gate that judges their work");
        break;
      case "generation-boundary":
        toast("L2 generation has no path to a state transition — the draft applies only through a gated action");
        break;
      case "refine":
        F.refinements.forEach((r) => r.version++);
        render();
        toast("New refinement version created · policy-change gate opened");
        break;
      case "refine-reset":
        F.refinements.forEach((r) => (r.version = 1));
        render();
        toast("Tenant refinement removed · the platform default applies");
        break;
      case "publish-release":
        v.review = "release";
        render();
        toast("Publish gate opened · the draft release is not live");
        break;
      case "retire-binding":
        F.bindings.find((x) => x.id === v.binding).status = "Retired";
        render();
        toast("Binding retired locally · history keeps its pinned release");
        break;
      case "binding":
        v.binding = d.id;
        render();
        break;
      case "decision":
        v.decision = d.id;
        render();
        break;
      case "toolkit":
        v.toolkit = d.key;
        render();
        break;
      case "incident":
        v.incident = d.id;
        render();
        break;
      case "runbook":
        v.runbook = d.id;
        render();
        break;
      case "call":
        v.callId = d.id;
        render();
        break;
      case "lesson":
        v.lesson = d.id;
        render();
        break;
      case "kata-open":
        toast("422 kata_instrument_required · a kata without an instrument and success threshold is not opened");
        break;
      case "coach-repeat":
        toast("This signature is already open · a second same-signature block escalates to a human, never another coach card");
        break;
      case "lesson-promote": {
        const l = F.lessons.find((x) => x.id === v.lesson);
        if (!l || l.independent < 2) {
          v.review = "lesson-blocked";
          render();
          toast("422 lesson_evidence_insufficient · one evidence reference is not enough");
        } else {
          v.review = "lesson-gate";
          render();
          toast("Review gate opened · a person reviews the lesson before a source draft");
        }
        break;
      }
      case "adopt":
        F.adoptions[0].state = "Approved";
        render();
        toast("Human adoption gate recorded · the change prepares a procedure version");
        break;
      case "adopt-revert":
        F.adoptions[1].state = "Reverted";
        render();
        toast("Reverted · a training-only or regressed gain is not adopted");
        break;
      case "dry-run":
        v.review = "dry-run";
        render();
        toast("Dry-run passed · read-back checks declared; approval still required");
        break;
      case "destructive":
        toast("Approval requested · dry-run + dual control + audit event; the write is not executed");
        break;
      case "postmortem":
        toast("Postmortem gate opened · blameless and evidence-linked");
        break;
      case "andon-toggle":
        v.andon = !v.andon;
        render();
        toast(v.andon ? "Andon pulled · new starts refused; record kept; on-call alerted" : "Andon released by George · person · Web");
        break;
      case "ceiling": {
        const order = ["read", "write", "destructive", "spend"];
        const b = F.toolBindings.find((x) => x.job === job().id) || F.toolBindings[0];
        if (order.indexOf(d.level) > order.indexOf(b.ceiling)) {
          v.review = "ceiling";
          render();
          toast("403 tool_effect_forbidden · a binding narrows only");
        } else {
          b.ceiling = d.level;
          v.review = null;
          render();
          toast("Ceiling narrowed locally · the grant is unchanged");
        }
        break;
      }
      case "byo-import":
        toast("Draft toolkit imported · tenant-admin approval + contract test required");
        break;
      case "tool-publish":
        toast("Publish gate opened · the publishing job cannot widen its own effect ceiling");
        break;
      case "kata":
        v.kata = d.id;
        render();
        break;
      case "sop":
        v.sop = Number(d.index);
        render();
        break;
      case "sop-intake":
        toast("Batch intake created drafts only · no publication, no tool grant");
        break;
      case "sop-advance":
        F.sops[v.sop].state =
          F.sops[v.sop].state === "Draft PKO + PDDL" ? "Review" : F.sops[v.sop].state;
        render();
        toast("Conversion advanced · review gate remains open");
        break;
      case "sop-publish":
        if (F.sops[v.sop].state === "Review") {
          F.sops[v.sop].state = "Published";
          v.review = "sop-published";
        } else {
          v.review = "sop-refused";
        }
        render();
        break;
      case "convert-doc":
        toast("BPMN converted to a procedure draft + PKO rows · review gate opened");
        break;
      case "pddl-check":
        toast("422 pko_term_unknown · PKO-T99 is not in pko@" + v.pkoVersion);
        break;
      case "pko-edit":
        v.pkoVersion++;
        F.pko.release = "pko@" + v.pkoVersion;
        render();
        toast("New PKO release draft pko@" + v.pkoVersion + " · review gate opened");
        break;
      case "pko-term":
        v.pkoTerm = d.id;
        render();
        break;
      case "pko-kind":
        v.pkoKind = d.id;
        render();
        break;
      case "pko-formal":
        v.pkoFormal = !v.pkoFormal;
        render();
        break;
      case "pko-validate":
        toast(
          "422 pko_relation_invalid · task 'contains' task is not permitted in pko@" +
            v.pkoVersion,
        );
        break;
      case "pko-add-relation":
        toast(
          "422 pko_relation_invalid · a task cannot contain another task in the base layer",
        );
        break;
      case "pko-publish-check":
        toast(
          "409 pko_not_ready · PKO-T99 has no definition · publication refused",
        );
        break;
      case "pko-eval":
        state.workbench = { tab: "Object evaluation", subject: "pko" };
        navigate("evals");
        break;
      case "job-eval":
        state.workbench = { tab: "Object evaluation", subject: "job" };
        navigate("evals");
        break;
      case "explain-missing":
        v.explainMissing = !v.explainMissing;
        render();
        toast(
          v.explainMissing
            ? "409 capability_not_ready · no model call was made"
            : "Required kind restored; the manifest resolves",
        );
        break;
      case "explain-retired":
        v.explainRetired = !v.explainRetired;
        render();
        toast(
          v.explainRetired
            ? "Manifest retained; exact reconstruction is not possible"
            : "All historical inputs retained; reconstruction available",
        );
        break;
      case "toggle-pack":
        v.packReady = !v.packReady;
        render();
        toast(v.packReady ? "Knowledge pack capability@18 published (local)" : "Knowledge pack unpublished (local)");
        break;
      case "publish-procedure":
        toast(
          v.packReady
            ? "Publish gate opened · procedure v8 remains a draft until a person approves"
            : "409 procedure_not_ready · required knowledge pack capability@18 is unpublished",
        );
        break;
      case "intake-chat":
        v.brief = $('[data-v2-input="brief"]')?.value.trim() || "";
        toast(
          v.brief
            ? "Asset drafts created from the brief · every output is a draft behind a review gate"
            : "Enter a chat brief first",
        );
        break;
      case "draft":
        v.draftId = d.id;
        render();
        break;
      case "publish-asset":
        if (F.drafts.some((x) => x.state === "Unpublished")) {
          v.review = "asset-blocked";
          render();
          toast("422 asset_dependency_unpublished · a referenced asset is unpublished");
        } else {
          v.review = "asset-gate";
          render();
          toast("Review gate opened · the draft is not published until a person approves");
        }
        break;
      case "intake":
        v.intake = d.kind;
        render();
        break;
      case "meter":
        v.meterLevel = d.level;
        render();
        break;
      case "probe":
        v.probe[d.key] = v.probe[d.key] === "green" ? "failed" : "green";
        render();
        toast(
          v.probe[d.key] === "failed"
            ? `${d.key} probe failed · new runs unbind (409 tool_not_ready)`
            : `${d.key} probe green`,
        );
        break;
      case "cycle":
        v.cycle = true;
        render();
        toast("422 project_dependency_cycle · the edge was refused");
        break;
      case "wsjf":
        toast("WSJF recomputed from the four typed inputs; the score is never stored");
        render();
        break;
      case "stale":
        v.review = "stale";
        render();
        toast("409 job_binding_stale · refused before any model call");
        break;
      case "change-approve":
      case "change-return": {
        const rationale = $('[data-v2-input="rationale"]')?.value.trim();
        if (!rationale) {
          v.review = "change-blocked";
          render();
          break;
        }
        F.changes[0].rationale = rationale;
        v.review = "change-ok";
        render();
        toast(
          d.v2 === "change-approve"
            ? "Approved locally · baseline B4 drafted; run pins unchanged"
            : "Returned locally · analysis requested; baseline unchanged",
        );
        break;
      }
      case "andon":
        v.review = v.review === "andon-released" ? null : "andon-released";
        render();
        toast(
          v.review
            ? "Andon released by George · person · Web"
            : "Andon cord pulled · new starts refused; record kept",
        );
        break;
      case "gate":
        toast("Human gate recorded · decided_by_kind = person · via Web");
        break;
      case "blocked":
        toast(d.msg || "Refused by the local design fixture");
        break;
      default:
        break;
    }
  });

  return { sidebar, titles, pages };
};
