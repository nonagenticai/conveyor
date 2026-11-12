/* Conveyor workspace study. All mutations affect this page's fixture state only. */
"use strict";
const $ = (s, root = document) => root.querySelector(s);
const $$ = (s, root = document) => [...root.querySelectorAll(s)];
const icon = (name) =>
  `<svg class="icon" aria-hidden="true"><use href="#i-${name}"/></svg>`;
const esc = (value) =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const badge = (text, tone = "") =>
  `<span class="badge ${tone}">${esc(text)}</span>`;
const btn = (label, action, symbol = "", cls = "", extra = "") =>
  `<button class="btn ${cls}" data-action="${action}" ${extra}>${symbol ? icon(symbol) : ""}${label}</button>`;
const navItems = [
  ["workspace", "Workspace", "grid", "OPERATE"],
  ["board", "Work board", "board"],
  ["runs", "Runs", "run"],
  ["decisions", "Decisions", "decision"],
  ["capabilities", "Capabilities", "agent", "BUILD"],
  ["procedures", "Procedures", "flow"],
  ["knowledge", "Knowledge & memory", "book"],
  ["requirements", "Requirements", "file"],
  ["evals", "Evaluations", "chart", "IMPROVE"],
  ["usage", "Usage & audit", "pulse"],
  ["channels", "Channels", "chat", "CONNECT"],
  ["settings", "Settings", "settings"],
];
const nodes = [
  {
    id: "request",
    label: "Understand the request",
    kind: "Procedure",
    executor: "request_analysis@3",
    description:
      "Extract the objective, constraints and acceptance criteria from the approved project brief.",
    icon: "file",
    x: 25,
    y: 156,
    status: "Completed",
    time: "48s",
    color: "green",
    tools: ["requirements.list", "cards.update", "memory.remember"],
    output: "Scope brief · 6 requirements",
  },
  {
    id: "plan",
    label: "Plan the work",
    kind: "Internal agent",
    executor: "feature_planner@4",
    description:
      "Turn the approved scope into an implementation plan with explicit acceptance checks.",
    icon: "agent",
    x: 235,
    y: 156,
    status: "Completed",
    time: "1m 12s",
    color: "green",
    tools: ["requirements.list", "knowledge.query", "cards.create"],
    output: "Implementation plan · v4",
  },
  {
    id: "implement",
    label: "Implement the feature",
    kind: "External agent · MCP",
    executor: "agent / portal-developer",
    description:
      "Add workspace invitations and scoped project access. Changes are isolated on the feature branch.",
    icon: "code",
    x: 445,
    y: 60,
    status: "Completed",
    time: "4m 32s",
    color: "green",
    tools: ["cards.get", "documents.create", "approvals.list"],
    output: "Feature patch · 7 files",
  },
  {
    id: "verify",
    label: "Verify acceptance",
    kind: "Procedure",
    executor: "verify_feature@7",
    description:
      "Run the published acceptance checks against the feature patch and attach the evidence.",
    icon: "check",
    x: 445,
    y: 252,
    status: "Completed",
    time: "1m 06s",
    color: "green",
    tools: ["documents.get", "coding.tests", "documents.create"],
    output: "Acceptance report · 18/18",
  },
  {
    id: "review",
    label: "Human review",
    kind: "Approval gate",
    executor: "George · via Web",
    description:
      "Review the implementation and its acceptance evidence before releasing the work package.",
    icon: "decision",
    x: 655,
    y: 156,
    status: "Needs your review",
    time: "Waiting",
    color: "amber",
    tools: ["approvals.decide"],
    output: "Awaiting a person’s decision",
  },
  {
    id: "publish",
    label: "Publish the result",
    kind: "Procedure",
    executor: "publish_feature@2",
    description:
      "Publish the verified work package and record the release evidence after the human gate is satisfied.",
    icon: "link",
    x: 865,
    y: 156,
    status: "Not started",
    time: "Queued",
    color: "",
    tools: ["documents.create", "cards.move"],
    output: "Blocked by human review",
  },
];
const initialEvents = [
  [
    "14:42",
    "decision",
    "Implementation is ready for your review",
    "Human gate opened · release is waiting",
    "Web · Telegram · MCP",
  ],
  [
    "14:41",
    "check",
    "All 18 acceptance checks passed",
    "verify_feature@7 · acceptance-report.json",
    "Procedure",
  ],
  [
    "14:40",
    "code",
    "Feature patch attached",
    "portal-developer · 7 files changed · +186 / −24",
    "External agent",
  ],
  [
    "14:36",
    "agent",
    "Implementation plan accepted",
    "feature_planner@4 → portal-developer",
    "Internal agent",
  ],
];
const state = {
  view: "workspace",
  selected: "implement",
  inspectorTab: "overview",
  advanced: false,
  zoom: 1,
  replaying: false,
  events: [...initialEvents],
  decision: null,
  evalSigns: { inputs: true, grading: false },
  evalStarted: false,
  evalFilter: "all",
  runFilter: "all",
  cardQuery: "",
  runQuery: "",
  draft: false,
  cards: [
    {
      id: "CP-042",
      title: "Workspace invitations",
      lane: "Inspecting",
      type: "Feature",
      owner: "AG",
      color: "purple",
      description:
        "Invite members by email and enforce project-scoped access. All 18 acceptance checks pass; human review is pending.",
    },
    {
      id: "CP-043",
      title: "Audit trail for role changes",
      lane: "Doing",
      type: "Feature",
      owner: "AG",
      color: "green",
      description:
        "Capture who changed a member’s role, when, and through which channel.",
    },
    {
      id: "CP-044",
      title: "Member search and filtering",
      lane: "Readying",
      type: "Feature",
      owner: "GD",
      color: "green",
      description: "Search the member roster by name or email; filter by role.",
    },
    {
      id: "CP-045",
      title: "Document invitation expiry",
      lane: "Readying",
      type: "Document",
      owner: "GD",
      color: "",
      description:
        "Explain the seven-day invitation lifetime and how to send a new invitation.",
    },
    {
      id: "CP-039",
      title: "Tenant isolation checks",
      lane: "Done",
      type: "Quality",
      owner: "AG",
      color: "purple",
      description:
        "Negative access checks across tenants. Verified by the acceptance procedure.",
    },
    {
      id: "CP-038",
      title: "Authentication foundation",
      lane: "Done",
      type: "Feature",
      owner: "AG",
      color: "green",
      description: "Keycloak sign-in, session handling, and workspace context.",
    },
  ],
  runs: [
    {
      id: "RUN-148",
      name: "Workspace invitations",
      executor: "External agent · MCP",
      status: "Waiting approval",
      step: "Human review",
      cost: "$0.84",
      duration: "7m 38s",
    },
    {
      id: "RUN-147",
      name: "Audit trail for role changes",
      executor: "Internal agent",
      status: "Running",
      step: "Implement",
      cost: "$0.31",
      duration: "2m 14s",
    },
    {
      id: "RUN-146",
      name: "Tenant isolation checks",
      executor: "Procedure",
      status: "Completed",
      step: "6 / 6 steps",
      cost: "$0.12",
      duration: "1m 43s",
    },
    {
      id: "RUN-145",
      name: "Authentication foundation",
      executor: "External agent · MCP",
      status: "Completed",
      step: "5 / 5 steps",
      cost: "$0.76",
      duration: "6m 09s",
    },
    {
      id: "RUN-144",
      name: "Invitation email template",
      executor: "Internal agent",
      status: "Failed",
      step: "Required knowledge unavailable",
      cost: "$0.00",
      duration: "Readiness refused",
    },
  ],
  chat: [],
  timer: null,
  draftIntent:
    "Deliver an approved feature with a verified patch and a person’s review.",
  findings: { repeat: null, role: null },
};
const main = $("#main");
const modal = $("#modal");
let graphObserver;
let toastTimer;
let returnFocus;
try {
  const theme = localStorage.getItem("conveyor-prototype-theme");
  if (theme) document.documentElement.dataset.theme = theme;
} catch {}
function pendingCount() {
  return state.decision ? 0 : 1;
}
function renderNav() {
  let html = `<div class="brand"><img src="assets/conveyor.svg" alt="">conveyor</div><div class="brand-sub">FROM INTENT TO EVIDENCE</div><div class="workspace-label"><span class="square">N</span><div>Non-Agentic AI<small>Engineering workspace</small></div></div>`;
  navItems.forEach(([id, label, symbol, group]) => {
    if (group)
      html += `${id === "workspace" ? "" : "</div>"}<div class="nav-group"><div class="nav-heading">${group}</div>`;
    html += `<a class="nav-link ${state.view === id ? "active" : ""}" href="#${id}" ${state.view === id ? 'aria-current="page"' : ""}>${icon(symbol)}<span>${label}</span>${id === "decisions" && pendingCount() ? `<span class="nav-count wait">${pendingCount()}</span>` : id === "runs" ? `<span class="nav-count">${state.runs.filter((r) => r.status === "Running").length}</span>` : ""}</a>`;
  });
  html += `</div><div class="sidebar-bottom"><button class="nav-link" data-action="advanced" aria-pressed="${state.advanced}">${icon("code")}Advanced details<span class="nav-count">${state.advanced ? "ON" : "OFF"}</span></button><div class="sidebar-note"><b>One workspace. Three channels.</b><br>The same work, wherever you are.</div></div>`;
  $("#sidebar").innerHTML = html;
}
function head(title, sub, section, actions = "", tabs = "") {
  return `<section class="page-head ${tabs ? "" : "simple"}"><div class="eyebrow">${icon("grid")}${section}</div><div class="heading-row"><div><h1>${title}</h1><p class="subtitle">${sub}</p></div><div class="head-actions">${actions}</div></div>${tabs}</section>`;
}
function projectHead() {
  const tabs = `<div class="lifecycle" aria-label="Project lifecycle">${["Request", "Scope", "Build", "Verify", "Release"].map((p, i) => `${i ? '<span class="phase-line" aria-hidden="true"></span>' : ""}<div class="phase ${i < 3 ? "done" : i === 3 ? "current" : ""}"><span class="step">${i < 3 ? icon("check") : i + 1}</span>${p}</div>`).join("")}</div><nav class="tabs" aria-label="Project views">${[
    ["workspace", "Execution", "flow"],
    ["board", "Work board", "board"],
    ["artifacts", "Artifacts", "file"],
    ["activity", "Activity", "pulse"],
  ]
    .map(
      ([id, label, s]) =>
        `<button class="tab ${state.view === id ? "active" : ""}" data-action="navigate" data-view="${id}" ${state.view === id ? 'aria-current="page"' : ""}>${icon(s)}${label}${id === "artifacts" ? "<small>4</small>" : ""}</button>`,
    )
    .join("")}</nav>`;
  return head(
    "Customer portal",
    "Workspace access, without the back-and-forth.",
    "PROJECT / CP-001",
    `${btn("Project brief", "brief", "file", "", "")} ${btn("New work", "new-work", "plus", "primary")}`,
    tabs,
  );
}
function graph() {
  return `<section class="panel"><header class="panel-head"><div class="panel-title">${icon("flow")}<h2>From request to release</h2></div><div class="head-actions"><span class="meta">RUN-148 &nbsp; · &nbsp; v7</span>${btn(state.replaying ? "Playing…" : "Replay demo", "replay", "run", "tiny text", state.replaying ? "disabled" : "")}</div></header><div class="canvas-frame" aria-label="Execution graph; select a step to inspect it"><div class="graph-stage"><svg class="graph-edges" aria-hidden="true" viewBox="0 0 1080 430"><path class="complete" d="M213 214H235"/><path class="complete" d="M423 214H433Q439 214 439 208V124Q439 118 445 118"/><path class="complete" d="M539 176V252"/><path class="complete" d="M633 118H639Q649 118 649 128V204Q649 214 655 214"/><path class="complete" d="M633 310H639Q649 310 649 300V224Q649 214 655 214"/><path class="future" d="M843 214H865"/></svg><span class="node-note" style="left:25px;top:33px">A work package, with every handoff visible.</span><span class="node-note" style="left:551px;top:217px">patch → acceptance checks</span>${nodes.map((n) => `<button class="graph-node ${state.selected === n.id ? "selected" : ""}" style="left:${n.x}px;top:${n.y}px" data-action="select-node" data-id="${n.id}" aria-pressed="${state.selected === n.id}" aria-label="${n.label}: ${n.status}"><div class="node-top"><span class="node-avatar ${n.id === "implement" ? "purple" : n.id === "review" ? "amber" : ""}">${icon(n.icon)}</span><span>${n.kind}</span></div><h3>${n.label}</h3><p>${esc(n.executor)}</p><div class="node-foot">${badge(n.status, n.color)}<span>${n.time}</span></div></button>`).join("")}<span class="node-note" style="left:25px;top:399px">PROCEDURES + INTERNAL AGENTS + EXTERNAL EXECUTORS</span></div></div><footer class="canvas-legend"><div class="legend-items"><span><i class="dot"></i>Completed</span><span><i class="dot" style="background:var(--amber)"></i>Human gate</span><span><i class="dot pending"></i>Not started</span></div><div class="canvas-tools"><button data-action="zoom-out" aria-label="Zoom out">−</button><span class="zoom-label">${Math.round(state.zoom * 100)}%</span><button data-action="zoom-in" aria-label="Zoom in">+</button><button data-action="zoom-fit" aria-label="Fit graph">⛶</button></div></footer></section>`;
}
function eventsHtml(all = false) {
  return `<section class="panel activity-panel"><header class="panel-head"><div class="panel-title">${icon("pulse")}<h2>Execution feed</h2></div><span class="badge green"><i class="dot"></i>Fixture replay</span></header>${(all ? state.events : state.events.slice(0, 4)).map((e) => `<article class="event"><time>${e[0]}</time><span class="event-icon">${icon(e[1])}</span><div><strong>${esc(e[2])}</strong><p>${esc(e[3])}</p></div>${badge(e[4])}</article>`).join("")}<div class="timeline-note">${state.advanced ? "run:148 · action_invocation → run_step → approval" : "A recorded sample sequence. Replay to walk through the handoffs."}</div></section>`;
}
function inspector() {
  const n = nodes.find((x) => x.id === state.selected);
  let content = "";
  if (state.inspectorTab === "overview")
    content = `<dl class="key-values"><dt>Executor</dt><dd>${badge(n.kind, n.id === "implement" ? "purple" : "green")}</dd><dt>Work item</dt><dd>CP-042</dd><dt>Procedure</dt><dd class="mono">build_feature@7</dd><dt>Elapsed</dt><dd>${n.time}</dd><dt>Output</dt><dd>${n.id === "implement" ? "7 files changed" : n.id === "verify" ? "18 checks passed" : n.id === "review" ? "Decision required" : "1 artifact"}</dd><dt class="advanced-only">Run reference</dt><dd class="advanced-only mono">run:148/${n.id}</dd></dl><div class="divider"></div><div class="mini-label">${n.id === "implement" ? "Execution boundary" : "Readiness"}</div><div class="source-note">${n.id === "implement" ? "External agent, connected through Conveyor’s MCP surface. Conveyor records its handoffs and decisions; the agent platform owns its model loop." : n.id === "review" ? "A person reviews the evidence. A decision from any channel closes the same approval everywhere." : "Published procedure and required knowledge are pinned. Readiness is checked before execution."}</div><div class="divider"></div><div class="mini-label">Evidence</div><button class="resource-link" data-action="artifact" data-id="${n.id === "verify" ? "report" : n.id === "implement" ? "patch" : "plan"}">${icon("file")}<span>${esc(n.output)}<small>Attached to this work package</small></span></button>`;
  if (state.inspectorTab === "context")
    content = `<div class="mini-label">Context provenance</div>${[
      ["System", "conveyor_system@2"],
      ["Procedure", "build_feature@7"],
      ["Capability & role", "feature_developer@4"],
      ["Domain knowledge", "portal-access@3"],
      ["Project & card", "CP-001 / CP-042"],
      ["Work-package memory", "2 pinned facts"],
    ]
      .map(
        ([label, val]) =>
          `<div class="context-item">${label}<div class="mini-row"><span class="mono">${val}</span><span>pinned ✓</span></div></div>`,
      )
      .join(
        "",
      )}<div class="divider"></div><div class="mini-label">Compiled recipe · feature_work@4</div><div class="mini-row"><span>4,820 / 8,000 tokens</span><span>60%</span></div><div class="budget-bar"><span style="width:60%"></span></div><p class="source-note">Sources compile at call time. The ledger records recipe, version and hash; prompt bodies stay with the router. External agent context is managed by its platform.</p>`;
  if (state.inspectorTab === "tools")
    content = `<div class="mini-label">${n.kind.startsWith("External") ? "Conveyor MCP tools used" : "Registered actions"}</div>${n.tools.map((t) => `<div class="context-item"><span class="mono">${t}</span><div class="mini-row"><span>Invocation recorded</span><span>✓</span></div></div>`).join("")}<div class="divider"></div><p class="source-note">${n.kind.startsWith("External") ? "This lists Conveyor actions visible to this workspace. Coding tools and model calls used inside the external platform are not counted here." : "Actions use the same registry, permissions and audit trail across Web, Telegram and MCP."}</p>${n.kind === "Internal agent" ? '<div class="divider"></div><div class="mini-label">Loop caps</div><dl class="key-values"><dt>Turns</dt><dd>3 / 8</dd><dt>Tool calls</dt><dd>5 / 20</dd><dt>Wall time</dt><dd>72 / 300s</dd></dl>' : ""}`;
  return `<aside class="inspector" aria-label="Selected step"><section class="panel"><header class="panel-head"><h2>STEP INSPECTOR</h2>${badge(n.id === "implement" ? "03 / 06" : `0${nodes.indexOf(n) + 1} / 06`)}</header><div class="inspector-title"><span class="node-avatar ${n.id === "implement" ? "purple" : n.id === "review" ? "amber" : ""}">${icon(n.icon)}</span><h2>${n.label}</h2>${badge(n.status, n.color)}<p>${n.description}</p></div><div class="tabs" role="tablist" aria-label="Step detail">${["overview", "context", "tools"].map((t) => `<button id="inspector-tab-${t}" class="tab ${state.inspectorTab === t ? "active" : ""}" role="tab" aria-selected="${state.inspectorTab === t}" aria-controls="inspector-content" data-action="inspector-tab" data-id="${t}">${t[0].toUpperCase() + t.slice(1)}</button>`).join("")}</div><div class="inspector-body" id="inspector-content" role="tabpanel" aria-labelledby="inspector-tab-${state.inspectorTab}">${content}</div><footer class="inspector-footer">${btn("Inspect run", "run-detail", "run", "", 'data-id="RUN-148"')}${btn(n.id === "review" && !state.decision ? "Review" : "View output", n.id === "review" && !state.decision ? "review" : "artifact", n.id === "review" && !state.decision ? "decision" : "file", "", `data-id="${n.id === "verify" ? "report" : n.id === "implement" ? "patch" : "plan"}"`)}</footer></section><section class="copilot"><h3>${icon("chat")}A little help, in context.</h3><p>Ask about this work package, its evidence, or what needs to happen next.</p>${btn("Ask the copilot", "copilot", "arrow", "soft")}</section><div class="source-note">${icon("link")} Shared record across Web, Telegram and MCP. Decisions keep the actor and channel.</div></aside>`;
}
function workspace() {
  const attention = state.decision
    ? `<div class="attention" style="background:var(--accent-soft);color:var(--accent);border-color:var(--line)">${icon("check")}<span><strong>Review ${state.decision === "approve" ? "approved" : state.decision === "cancelled" ? "closed" : "returned"}.</strong> ${state.decision === "approve" ? "The release procedure can continue." : state.decision === "cancelled" ? "The sample run was cancelled." : "Implementation is back in Doing."}</span>${btn("View decision", "decision-history", "", "tiny text")}</div>`
    : `<div class="attention">${icon("decision")}<span><strong>Your review is needed.</strong> Workspace invitations is ready.</span><button data-action="review">Review decision &nbsp; →</button></div>`;
  return (
    projectHead() +
    `<div class="workspace"><div class="workspace-left">${attention}${graph()}${eventsHtml()}</div>${inspector()}</div>`
  );
}
function board() {
  const lanes = ["Readying", "Doing", "Inspecting", "Done"];
  return (
    projectHead() +
    `<div class="content"><div class="toolbar"><span class="body-copy">${state.cards.length} work items <span class="muted">· drag a card, or open it to change its list</span></span><label class="field-search">${icon("search")}<input type="search" id="card-search" placeholder="Filter work items…" aria-label="Filter work items" value="${esc(state.cardQuery)}"></label></div><div class="board">${lanes
      .map(
        (lane, i) =>
          `<section class="lane" data-lane="${lane}"><header class="lane-head"><span class="lane-name"><i class="dot" style="background:${["var(--faint)", "var(--accent)", "var(--amber)", "var(--purple)"][i]}"></i>${lane}</span><small>${state.cards.filter((c) => c.lane === lane).length}</small></header>${state.cards
            .filter(
              (c) =>
                c.lane === lane &&
                (c.title + " " + c.id)
                  .toLowerCase()
                  .includes(state.cardQuery.toLowerCase()),
            )
            .map(
              (c) =>
                `<button class="work-card" draggable="true" data-card="${c.id}" data-action="card" data-id="${c.id}">${badge(c.type, c.color)}<h3>${esc(c.title)}</h3>${c.id === "CP-042" && !state.decision ? badge("Human review", "amber") : ""}<footer><span>${c.id}</span><span class="avatar">${c.owner}</span></footer></button>`,
            )
            .join(
              "",
            )}${!state.cards.some((c) => c.lane === lane) ? '<p class="lane-hint">Drop work here</p>' : ""}</section>`,
      )
      .join(
        "",
      )}</div><p class="prototype-notice">The list is the card’s state. An open approval remains open if its card moves; decisions are recorded separately.</p></div>`
  );
}
function stats(items) {
  return `<div class="summary-grid">${items.map(([label, value, note]) => `<article class="stat"><div class="mini-label">${label}</div><div class="stat-value">${value}</div><p>${note}</p></article>`).join("")}</div>`;
}
function runs() {
  const rows = state.runs.filter(
    (r) =>
      (state.runFilter === "all" ||
        r.status.toLowerCase().includes(state.runFilter)) &&
      (r.name + " " + r.id)
        .toLowerCase()
        .includes(state.runQuery.toLowerCase()),
  );
  return (
    head(
      "Runs",
      "See what is executing, waiting, or ready for investigation.",
      "OPERATE / RUNS",
      btn("New work", "new-work", "plus", "primary"),
    ) +
    `<div class="content">${stats([
      [
        "Running",
        state.runs.filter((r) => r.status === "Running").length,
        "Across this project",
      ],
      [
        "Waiting for input",
        state.runs.filter((r) => r.status === "Waiting approval").length,
        "No worker held while waiting",
      ],
      [
        "Completed",
        state.runs.filter((r) => r.status === "Completed").length,
        "Evidence attached",
      ],
      [
        "Needs investigation",
        state.runs.filter((r) => r.status === "Failed").length,
        "Readiness and runtime failures",
      ],
    ])}<div class="toolbar"><div class="segments" aria-label="Run status filter">${["all", "running", "waiting", "failed"].map((f) => `<button class="${state.runFilter === f ? "active" : ""}" data-action="run-filter" data-id="${f}">${f[0].toUpperCase() + f.slice(1)}</button>`).join("")}</div><label class="field-search">${icon("search")}<input id="run-search" type="search" placeholder="Find a run…" aria-label="Find a run" value="${esc(state.runQuery)}"></label></div><section class="panel"><div class="table-wrap"><table><thead><tr><th>Work package / run</th><th>Executor</th><th>State</th><th>Current step</th><th>Duration</th><th>Conveyor model cost</th></tr></thead><tbody>${rows.map((r) => `<tr><td>${btn(esc(r.name), "run-detail", "", "text", `data-id="${r.id}"`)}<small>${r.id}</small></td><td>${badge(r.executor, r.executor.startsWith("External") ? "purple" : "")}</td><td>${badge(r.status, r.status === "Failed" ? "red" : r.status === "Waiting approval" ? "amber" : "green")}</td><td>${r.step}</td><td class="mono">${r.duration}</td><td class="mono">${r.cost}</td></tr>`).join("")}</tbody></table>${!rows.length ? '<div class="empty">No runs match this filter.</div>' : ""}</div></section><p class="prototype-notice">Model cost is derived from Conveyor’s inference ledger. Model usage inside an external executor is outside this total.</p></div>`
  );
}
function decisions() {
  return (
    head(
      "Decisions",
      "The work keeps moving when the right person can step in.",
      "OPERATE / DECISIONS",
      btn("Decision history", "decision-history", "clock"),
    ) +
    `<div class="content">${state.decision ? `<section class="panel empty">${icon("check")}<h2>You’re all caught up.</h2><p style="margin-top:10px">The implementation review was ${state.decision === "approve" ? "approved" : "returned for changes"} by George via Web.</p>${btn("Open execution", "navigate", "flow", "", 'data-view="workspace" style="margin-top:20px"')}</section>` : `<article class="panel decision-card"><span class="node-avatar amber">${icon("decision")}</span><div class="decision-content">${badge("Implementation review", "amber")}<h3>Workspace invitations</h3><p>7 changed files · 18 acceptance checks passed · requested 4 minutes ago</p><small class="muted">Customer portal / CP-042 · George · Web, Telegram, MCP</small></div><div class="head-actions">${btn("Review evidence", "review", "arrow", "primary")}</div></article>`}<section class="panel" style="margin-top:20px"><header class="panel-head"><h2>Every ask is an approval</h2></header><div class="body-copy" style="padding:20px">A scope question, a failed step, or a release review reaches the same inbox. A decision closes the shared request across channels. Each record keeps who decided, whether they were a person or agent, and the channel they used.</div></section></div>`
  );
}
const capabilities = [
  {
    id: "developer",
    name: "Feature development",
    kind: "External agent · MCP",
    desc: "Implement an approved work package, attach a patch, and return its acceptance evidence.",
    pin: "portal-developer@4",
    icon: "code",
    tone: "purple",
  },
  {
    id: "planner",
    name: "Work planning",
    kind: "Internal agent",
    desc: "Resolve the scope, assemble context, and decompose the work into verifiable steps.",
    pin: "feature_planner@4",
    icon: "agent",
    tone: "green",
  },
  {
    id: "verifier",
    name: "Acceptance verification",
    kind: "Procedure",
    desc: "Execute pinned checks, collect their outputs, and raise a gate when evidence is incomplete.",
    pin: "verify_feature@7",
    icon: "check",
    tone: "green",
  },
  {
    id: "researcher",
    name: "Requirements analysis",
    kind: "Internal agent",
    desc: "Read governed requirements, query bound knowledge, and surface open questions.",
    pin: "requirements_analyst@2",
    icon: "book",
    tone: "green",
  },
  {
    id: "kata",
    name: "Continuous improvement",
    kind: "Procedure",
    desc: "Measure a candidate against the baseline and make the next change reviewable.",
    pin: "kata_cycle@3",
    icon: "chart",
    tone: "green",
  },
  {
    id: "writer",
    name: "Release documentation",
    kind: "Internal agent",
    desc: "Write release notes from verified artifacts. Waiting for its required knowledge pack.",
    pin: "release_writer@1",
    icon: "file",
    tone: "amber",
    blocked: true,
  },
];
function capabilitiesView() {
  return (
    head(
      "Capabilities",
      "Define what can be done, with which knowledge and execution boundary.",
      "BUILD / CAPABILITIES",
      btn("Create capability", "create-capability", "plus", "primary"),
    ) +
    `<div class="content"><div class="toolbar"><span class="body-copy">Three executor kinds. One work record.</span>${badge(`${capabilities.filter((c) => !c.blocked).length} ready · ${capabilities.filter((c) => c.blocked).length} blocked`, "green")}</div><div class="card-grid">${capabilities.map((c) => `<button class="panel library-card" data-action="capability" data-id="${c.id}"><span class="node-avatar ${c.tone === "purple" ? "purple" : ""}">${icon(c.icon)}</span><h3>${esc(c.name)}</h3>${badge(c.kind, c.tone)}<p>${esc(c.desc)}</p><footer><span class="mono">${esc(c.pin)}</span><span style="color:var(--${c.blocked ? "amber" : "accent"})">${c.blocked ? "◷ Blocked" : "✓ Ready"}</span></footer></button>`).join("")}</div><p class="prototype-notice">The copilot is a user-scoped interactive assistant. Capability executors drive the work runs.</p></div>`
  );
}
function procedures() {
  return (
    head(
      "Procedures",
      "Make a reliable sequence inspectable, versioned, and repeatable.",
      "BUILD / PROCEDURES",
      btn("Edit a draft", "edit-procedure", "plus", "primary"),
    ) +
    `<div class="content"><div class="two-col"><section class="panel"><header class="panel-head"><div><h2>Build a feature</h2><p class="subtitle">${state.draft ? "Draft v8" : "Published v7"} · referenced by RUN-148</p></div>${badge(state.draft ? "Draft" : "Published", state.draft ? "amber" : "green")}</header><ol class="process-list">${[
      [
        "Read scope",
        "requirements.list → governed requirement revision",
        "Action",
      ],
      [
        "Plan implementation",
        "feature_planner@4 · bounded internal loop",
        "Agent",
      ],
      ["Execute work package", "External executor through /mcp", "External"],
      [
        "Verify acceptance",
        "verify_feature@7 · evidence attached",
        "Procedure",
      ],
      ["Review implementation", "Approval required · George", "Human gate"],
      ["Publish the result", "Record artifact and move card to Done", "Action"],
    ]
      .map(
        ([t, d, k]) =>
          `<li><div><strong>${t}</strong><small>${d}</small></div>${badge(k, k === "Human gate" ? "amber" : "")}</li>`,
      )
      .join(
        "",
      )}</ol><div class="panel-head" style="border:0">${btn("Inspect published version", "procedure-detail", "file")}${btn("Edit as draft", "edit-procedure", "code", "primary")}</div></section><section><div class="panel"><header class="panel-head"><h2>Version & publication</h2></header><div style="padding:20px"><dl class="key-values"><dt>Published</dt><dd>v7 · 08 Nov 2026</dd><dt>Executor bindings</dt><dd>2 pinned</dd><dt>Required knowledge</dt><dd>portal-access@3</dd><dt>Prompt recipe</dt><dd class="mono">feature_work@4</dd><dt>Pre-publish audit</dt><dd>${badge("Passed", "green")}</dd></dl><div class="divider"></div><p class="source-note">Published versions stay frozen for their runs. A draft becomes a new version only after validation and prompt audit.</p></div></div><div class="panel" style="margin-top:18px"><header class="panel-head"><h2>Other procedures</h2></header>${[
      ["Verify acceptance", "verify_feature@7"],
      ["Publish work package", "publish_feature@2"],
      ["Kata improvement cycle", "kata_cycle@3"],
    ]
      .map(
        ([t, p]) =>
          `<button class="resource-link" style="border:0;border-radius:0;width:100%" data-action="procedure-detail" data-id="${p}">${icon("flow")}<span>${t}<small>${p} · published</small></span></button>`,
      )
      .join("")}</div></section></div></div>`
  );
}
function knowledge() {
  return (
    head(
      "Knowledge & memory",
      "Pinned domain knowledge. Decisions and lessons carried forward with the work.",
      "BUILD / KNOWLEDGE",
      btn("Inspect bindings", "knowledge-bindings", "link"),
    ) +
    `<div class="content">${stats([
      ["Bound packs", "3", "All required sources resolve"],
      ["Pack versions", "Pinned", "No silent fallback"],
      ["Work-package facts", "2", "Recorded via memory.remember"],
      ["Consolidation", "14:35", "Hot record → cold digest"],
    ])}<div class="two-col"><section class="panel"><header class="panel-head"><h2>Domain knowledge</h2>${badge("strata_rag", "green")}</header><div class="table-wrap"><table><thead><tr><th>Pack</th><th>Version</th><th>Binding</th><th>State</th></tr></thead><tbody>${[
      ["Portal access rules", "3", "Required"],
      ["Engineering standards", "8", "Required"],
      ["UI patterns", "2", "Optional"],
    ]
      .map(
        ([t, v, k]) =>
          `<tr><td>${btn(t, "knowledge-pack", "", "text", `data-id="${t}"`)}</td><td class="mono">v${v}</td><td>${k}</td><td>${badge("Ready", "green")}</td></tr>`,
      )
      .join(
        "",
      )}</tbody></table></div><div style="padding:17px"><p class="source-note">Knowledge provides context; the governed assets and work records remain the source of truth.</p></div></section><section class="panel"><header class="panel-head"><h2>Work-package memory</h2>${badge("CP-042")}</header><article class="memory"><div class="mini-row"><span>DECISION</span><span>14:34 · George</span></div><p>Invitations expire after seven days. A new invitation replaces the outstanding one.</p>${badge("Scope review · Web", "green")}</article><article class="memory"><div class="mini-row"><span>LESSON</span><span>14:35 · planner</span></div><p>Verify tenant context on invitation acceptance, even when the email address matches an existing member.</p>${badge("feature_planner@4")}</article></section></div></div>`
  );
}
const requirements = [
  [
    "REQ-014",
    "Only admins can invite workspace members",
    "Access control",
    "Approved",
    "CP-042",
  ],
  [
    "REQ-015",
    "Invitations expire after seven days",
    "Invitation lifecycle",
    "Approved",
    "CP-042",
  ],
  [
    "REQ-016",
    "Members see only their assigned projects",
    "Tenant isolation",
    "Approved",
    "CP-039",
  ],
  [
    "REQ-017",
    "Role changes leave an audit trail",
    "Governance",
    "Approved",
    "CP-043",
  ],
  [
    "REQ-018",
    "An invitation can be revoked before acceptance",
    "Invitation lifecycle",
    "Draft",
    "CP-042",
  ],
];
function requirementsView() {
  return (
    head(
      "Requirements",
      "Keep the intent, its revisions, and its implementation connected.",
      "BUILD / REQUIREMENTS",
      btn("Add requirement", "new-requirement", "plus", "primary"),
    ) +
    `<div class="content"><div class="panel"><header class="panel-head"><h2>Customer portal · governed assets</h2>${badge(`${requirements.length} requirements`)}</header><div class="table-wrap"><table><thead><tr><th>Requirement</th><th>Catalogue</th><th>Revision</th><th>State</th><th>Work item</th></tr></thead><tbody>${requirements.map((r) => `<tr><td>${btn(esc(r[1]), "requirement", "", "text", `data-id="${r[0]}"`)}<small>${r[0]}</small></td><td>${esc(r[2])}</td><td class="mono">v${r[0] === "REQ-014" ? 3 : 1}</td><td>${badge(r[3], r[3] === "Approved" ? "green" : "amber")}</td><td>${btn(r[4], "card", "", "text", `data-id="${r[4]}"`)}</td></tr>`).join("")}</tbody></table></div></div><section class="panel" style="margin-top:20px"><header class="panel-head"><h2>Trace the intent to its evidence</h2></header><div class="resource-list" style="padding:20px">${btn("REQ-014 → CP-042 → RUN-148 → Acceptance report", "trace", "flow", "soft")}</div></section></div>`
  );
}
function evals() {
  return (
    head(
      "Evaluations",
      "Improve the system against a fixed instrument, with the held-out result in view.",
      "IMPROVE / EVALUATIONS",
      btn("Review sign-offs", "eval-signoff", "decision") +
        btn(
          state.evalStarted ? "View new run" : "Run evaluation",
          "start-eval",
          "run",
          "primary",
        ),
    ) +
    `<div class="content">${stats([
      [
        "Held-out pass rate",
        '84<span class="unit">%</span>',
        "21 / 25 · latest sample round",
      ],
      [
        "Baseline",
        '72<span class="unit">%</span>',
        "18 / 25 · same instrument",
      ],
      [
        "Mean cost",
        '<span style="font-size:24px">$0.84</span>',
        "Derived from inference calls",
      ],
      [
        "Noise floor",
        '± 4<span class="unit">pp</span>',
        "Illustrative fixture estimate",
      ],
    ])}<div class="two-col"><section class="panel"><header class="panel-head"><div><h2>Feature completion · climb 03</h2><p class="subtitle">Instrument v3 · fixed train/test split</p></div>${badge("Check-in required", "amber")}</header><div class="chart"><svg viewBox="0 0 620 230" role="img" aria-label="Held-out pass rate: baseline 72 percent; rounds one through four: 76, 80, 80, 84 percent. Illustrative data."><path class="grid" d="M45 25H590M45 75H590M45 125H590M45 175H590"/><text x="6" y="29">90%</text><text x="6" y="79">85%</text><text x="6" y="129">80%</text><text x="6" y="179">75%</text><path class="area" d="M55 205L185 165L315 125L445 125L575 85V205Z"/><path class="baseline" d="M45 205H590"/><path class="trend" d="M55 205L185 165L315 125L445 125L575 85"/>${[
      [55, 205],
      [185, 165],
      [315, 125],
      [445, 125],
      [575, 85],
    ]
      .map(([x, y]) => `<circle class="point" cx="${x}" cy="${y}" r="4"/>`)
      .join(
        "",
      )}<text x="40" y="226">Base</text><text x="175" y="226">R1</text><text x="305" y="226">R2</text><text x="435" y="226">R3</text><text x="565" y="226">R4</text><text x="550" y="67" style="fill:var(--accent)">84%</text></svg><div class="chart-caption"><span><i class="dot"></i>Held-out result</span><span>— — Baseline · 72%</span></div></div><div style="padding:0 20px 18px" class="source-note">The trend is descriptive. Confidence and statistical significance require the paired report; a percentage change alone does not establish improvement.</div></section><section class="panel"><header class="panel-head"><h2>Instrument readiness</h2>${badge("v3 · 7b9a2f1c")}</header><article class="audit-item"><h3>Inputs reviewed</h3><p>50 cases · provenance and split checked by George.</p>${badge("Signed · person via Web", "green")}</article><article class="audit-item"><h3>Grading review</h3><p>The pilot and grader need a person’s sign-off before the next run.</p>${badge(state.evalSigns.grading ? "Signed · person via Web" : "Awaiting sign-off", state.evalSigns.grading ? "green" : "amber")} ${btn("Review", "eval-signoff", "", "tiny text")}</article><article class="audit-item"><h3>Held-out access</h3><p>Agents see train transcripts. Test transcripts require a person with eval approval access.</p>${badge("25 train / 25 held-out")}</article></section></div><section class="panel" style="margin-top:20px"><header class="panel-head"><h2>Case report · round 4</h2><div class="segments">${["all", "train", "test"].map((f) => `<button class="${state.evalFilter === f ? "active" : ""}" data-action="eval-filter" data-id="${f}">${f === "test" ? "Held-out" : f[0].toUpperCase() + f.slice(1)}</button>`).join("")}</div></header><div class="table-wrap"><table><thead><tr><th>Case</th><th>Split</th><th>Acceptance</th><th>Cost</th><th>Latency</th><th>Trace</th></tr></thead><tbody>${[
      ["EV-01", "Invite an existing member", "train", "Pass", "$0.62", "24.1s"],
      ["EV-02", "Expired invitation link", "train", "Pass", "$0.81", "31.4s"],
      ["EV-26", "Cross-tenant invitation", "test", "Pass", "$0.91", "34.6s"],
      ["EV-27", "Revoked invitation", "test", "Fail", "$1.02", "42.1s"],
    ]
      .filter((r) => state.evalFilter === "all" || r[2] === state.evalFilter)
      .map(
        (r) =>
          `<tr><td>${r[1]}<small>${r[0]}</small></td><td>${r[2] === "test" ? "Held-out" : "Train"}</td><td>${badge(r[3], r[3] === "Pass" ? "green" : "red")}</td><td class="mono">${r[4]}</td><td class="mono">${r[5]}</td><td>${btn("Inspect", "eval-trace", "", "tiny text", `data-id="${r[0]}" data-split="${r[2]}"`)}</td></tr>`,
      )
      .join(
        "",
      )}</tbody></table></div></section><div class="toolbar" style="margin-top:18px"><p class="prototype-notice" style="margin:0">Showing 4 sample rows of the 50-case instrument. Editing an instrument voids its sign-offs.</p>${btn("Export sample report", "export-eval", "download")}</div></div>`
  );
}
function usage() {
  return (
    head(
      "Usage & audit",
      "Trace model calls to their work. Keep provider cache classes distinct.",
      "IMPROVE / USAGE",
      btn("Export ledger sample", "export-usage", "download"),
    ) +
    `<div class="content">${stats([
      ["Model spend", "$12.48", "Conveyor calls · sample day"],
      ["Model calls", "156", "Linked to a run or copilot turn"],
      [
        "Provider cache hit rate",
        '75<span class="unit">%</span>',
        "108 hits ÷ (108 hits + 36 misses)",
      ],
      [
        "Open prompt findings",
        Object.values(state.findings).filter((v) => !v).length,
        "Proposed changes · not applied",
      ],
    ])}<div class="two-col"><section class="panel"><header class="panel-head"><h2>Provider prompt cache</h2>${badge("156 calls")}</header><div style="padding:20px"><div class="cache-stack" aria-label="108 hits, 8 writes, 36 misses, 4 unknown"><span style="width:69.23%;background:var(--accent)"></span><span style="width:5.13%;background:var(--purple)"></span><span style="width:23.08%;background:var(--amber)"></span><span style="width:2.56%;background:var(--faint)"></span></div><dl class="cache-key">${[
      ["Hits", "108", "accent"],
      ["Writes", "8", "purple"],
      ["Misses", "36", "amber"],
      ["Unknown", "4", "faint"],
    ]
      .map(
        ([k, v, c]) =>
          `<dt><i class="dot" style="background:var(--${c})"></i>${k}</dt><dd>${v}</dd>`,
      )
      .join(
        "",
      )}</dl><div class="divider"></div><p class="source-note">A write warms the provider’s prompt cache; it is separate from a miss. Calls without usage are unknown. Writes and unknowns are excluded from the hit-rate denominator.</p></div></section><section class="panel"><header class="panel-head"><h2>Prompt audit findings</h2>${badge("Proposals only", "amber")}</header><article class="audit-item"><h3>Repeated instruction in knowledge context</h3><p>feature_work@4 · remove redundant instructions from the domain section.</p>${badge(state.findings.repeat || "High confidence", state.findings.repeat ? "green" : "amber")} ${btn("Review diff", "audit-finding", "", "tiny text", 'data-id="repeat"')}</article><article class="audit-item"><h3>Overlapping role and procedure instructions</h3><p>request_analysis@3 · clarify which source owns the completion condition.</p>${badge(state.findings.role || "Medium confidence")} ${btn("Review diff", "audit-finding", "", "tiny text", 'data-id="role"')}</article></section></div><section class="panel" style="margin-top:20px"><header class="panel-head"><h2>Inference ledger · sample calls</h2>${badge("Hashes & counts · no bodies")}</header><div class="table-wrap"><table><thead><tr><th>Caller</th><th>Requested / served model</th><th>Recipe</th><th>Input / output</th><th>Cache</th><th>Latency</th></tr></thead><tbody>${[
      [
        "RUN-148 / planner",
        "tenant-default",
        "served-model-A",
        "feature_work@4",
        "4,820 / 612",
        "Hit",
        "1.8s",
      ],
      [
        "RUN-147 / internal agent",
        "tenant-default",
        "served-model-A",
        "feature_work@4",
        "5,140 / 840",
        "Write",
        "3.2s",
      ],
      [
        "Copilot / turn 18",
        "tenant-default",
        "served-model-A",
        "copilot@2",
        "2,104 / 320",
        "Miss",
        "1.6s",
      ],
    ]
      .map(
        (r) =>
          `<tr><td>${esc(r[0])}</td><td>${r[1]}<small>${r[2]} · fixture model alias</small></td><td class="mono">${r[3]}<small>sha256: 80da…b4c1</small></td><td class="mono">${r[4]}</td><td>${badge(r[5], r[5] === "Hit" ? "green" : r[5] === "Write" ? "purple" : "amber")}</td><td class="mono">${r[6]}</td></tr>`,
      )
      .join("")}</tbody></table></div></section></div>`
  );
}
function channels() {
  return (
    head(
      "Channels",
      "Work and decisions travel through the channel you are already using.",
      "CONNECT / CHANNELS",
    ) +
    `<div class="content"><div class="card-grid">${[
      [
        "Web workspace",
        "Interactive operations",
        "chat",
        "George · person",
        "Review work, inspect evidence, and ask the copilot.",
      ],
      [
        "Telegram",
        "Questions & decisions",
        "chat",
        "George · person",
        "The same approval arrives with the same evidence and closes when answered anywhere.",
      ],
      [
        "MCP",
        "External execution",
        "code",
        "portal-developer · agent",
        "Project execution uses the Conveyor MCP surface and durable input requests.",
      ],
    ]
      .map(
        ([t, k, i, p, d]) =>
          `<article class="panel library-card"><span class="node-avatar">${icon(i)}</span><h3>${t}</h3>${badge(k, "green")}<p>${d}</p><footer><span>${p}</span>${btn("Inspect", "channel-detail", "", "tiny text", `data-id="${t}"`)}</footer></article>`,
      )
      .join(
        "",
      )}</div><section class="panel" style="margin-top:20px"><header class="panel-head"><h2>One decision, everywhere</h2></header><ol class="process-list"><li><div><strong>A run requests input</strong><small>Approval AP-021 · CP-042 · implementation_review</small></div></li><li><div><strong>The request is delivered to the bound channels</strong><small>Web inbox, Telegram message, MCP task input request</small></div></li><li><div><strong>The first valid decision closes it</strong><small>Actor kind + channel + note are recorded. Late decisions see the closed result.</small></div></li></ol><div style="padding:16px">${btn("Open shared approval", "review", "decision", "primary", state.decision ? "disabled" : "")}</div></section></div>`
  );
}
function settings() {
  return (
    head(
      "Workspace settings",
      "Members, execution permissions and the boundaries around shared work.",
      "CONNECT / SETTINGS",
      btn("Inspect permissions", "profile", "decision"),
    ) +
    `<div class="content"><section class="panel"><header class="panel-head"><h2>Members & principals</h2>${badge("Non-Agentic AI")}</header><div class="table-wrap"><table><thead><tr><th>Principal</th><th>Kind</th><th>Role</th><th>Channel</th><th>Access</th></tr></thead><tbody><tr><td>George<small>Workspace administrator</small></td><td>${badge("Person", "green")}</td><td>Admin</td><td>Web · Telegram</td><td>Tenant-scoped</td></tr><tr><td>Portal developer<small>External executor</small></td><td>${badge("Agent", "purple")}</td><td>Agent</td><td>MCP</td><td>Bound project actions</td></tr><tr><td>Feature planner<small>Internal executor</small></td><td>${badge("Agent", "purple")}</td><td>Capability binding</td><td>Engine</td><td>Declared allowlist</td></tr></tbody></table></div></section><div class="two-col" style="margin-top:20px"><section class="panel"><header class="panel-head"><h2>Tenant boundary</h2></header><div style="padding:20px"><p class="body-copy">The tenant owns its workspace. Admins manage membership and roles. Platform provisioning belongs to a separate super-admin surface.</p></div></section><section class="panel"><header class="panel-head"><h2>Prototype preferences</h2></header><div style="padding:20px;display:flex;gap:10px;flex-wrap:wrap">${btn("Toggle color theme", "theme", "sun")}${btn("Toggle advanced details", "advanced", "code")}${btn("Reset sample data", "reset", "clock")}</div></section></div></div>`
  );
}
const artifacts = [
  ["brief", "Project brief", "Approved scope · v3", "file"],
  ["plan", "Implementation plan", "feature_planner@4 · v4", "flow"],
  ["patch", "Feature patch", "7 files · +186 / −24", "code"],
  ["report", "Acceptance report", "18 / 18 passed · verify_feature@7", "check"],
];
function artifactsView() {
  return (
    projectHead() +
    `<div class="content"><div class="card-grid">${artifacts.map(([id, t, d, i]) => `<button class="panel library-card" data-action="artifact" data-id="${id}"><span class="node-avatar">${icon(i)}</span><h3>${t}</h3><p>${d}</p><footer><span>CP-042 · RUN-148</span><span>Inspect ↗</span></footer></button>`).join("")}</div></div>`
  );
}
const pages = {
  workspace,
  board,
  runs,
  decisions,
  capabilities: capabilitiesView,
  procedures,
  knowledge,
  requirements: requirementsView,
  evals,
  usage,
  channels,
  settings,
  artifacts: artifactsView,
  activity: () =>
    projectHead() + `<div class="content">${eventsHtml(true)}</div>`,
};
function render() {
  graphObserver?.disconnect();
  document.body.classList.toggle("advanced", state.advanced);
  renderNav();
  main.innerHTML = (pages[state.view] || workspace)();
  document.title = `${state.view === "workspace" ? "Customer portal" : state.view[0].toUpperCase() + state.view.slice(1)} · Conveyor`;
  bindGraph();
}
function navigate(view) {
  if (!pages[view]) return;
  state.view = view;
  document.body.classList.remove("menu-open");
  $(".scrim").hidden = true;
  $(".mobile-menu").setAttribute("aria-expanded", "false");
  if (location.hash.slice(1) !== view) location.hash = view;
  render();
  window.scrollTo(0, 0);
}
function bindGraph() {
  const frame = $(".canvas-frame");
  if (!frame) return;
  graphObserver?.disconnect();
  const scale = () => {
    const base =
      innerWidth <= 760 ? 0.72 : Math.min(1, frame.clientWidth / 1080);
    if (innerWidth > 760)
      frame.style.height = `${Math.max(300, 430 * base + 24)}px`;
    else frame.style.height = "350px";
    $(".graph-stage").style.transform = `scale(${base * state.zoom})`;
    $(".graph-stage").style.top =
      `${Math.max(10, (frame.clientHeight - 430 * base * state.zoom) / 2)}px`;
    $(".zoom-label").textContent = `${Math.round(state.zoom * 100)}%`;
  };
  scale();
  graphObserver = new ResizeObserver(scale);
  graphObserver.observe(frame);
}
function toast(message) {
  clearTimeout(toastTimer);
  $("#toast").textContent = message;
  $("#toast").classList.add("show");
  toastTimer = setTimeout(() => $("#toast").classList.remove("show"), 4200);
}
function openModal(title, body, actions = "", wide = false) {
  returnFocus = document.activeElement;
  modal.classList.toggle("wide", wide);
  modal.innerHTML = `<header class="modal-header"><h2 id="modal-title">${title}</h2><button class="icon-btn" data-action="close" aria-label="Close dialog">${icon("close")}</button></header>${body}${actions ? `<footer class="modal-actions">${actions}</footer>` : ""}`;
  if (!modal.open) modal.showModal();
}
function closeModal() {
  modal.close();
  if (returnFocus?.isConnected) returnFocus.focus();
}
modal.addEventListener("click", (e) => {
  if (e.target === modal) {
    const r = modal.getBoundingClientRect();
    if (
      e.clientX < r.left ||
      e.clientX > r.right ||
      e.clientY < r.top ||
      e.clientY > r.bottom
    )
      closeModal();
  }
});
function download(name, data) {
  const blob = new Blob([JSON.stringify(data, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  toast("Sample exported to your downloads.");
}
function review() {
  if (state.decision) {
    decisionHistory();
    return;
  }
  openModal(
    "Review the implementation",
    `<p class="modal-subtitle">Customer portal / CP-042 · approval AP-021</p><div class="modal-title-badge">${badge("Human gate", "amber")}<h3>Workspace invitations</h3></div><p class="body-copy">The external developer has attached the patch and the acceptance procedure has verified it. Your decision releases or returns this work package.</p><div class="review-evidence"><p>${icon("check")}18 / 18 acceptance checks passed</p><p>${icon("check")}Tenant boundary and invitation expiry covered</p><p>${icon("file")}7 changed files · +186 / −24 · feature/workspace-invites</p></div><div class="head-actions" style="justify-content:flex-start;margin:12px 0 18px">${btn("Inspect patch", "artifact", "code", "", 'data-id="patch"')}${btn("Inspect report", "artifact", "file", "", 'data-id="report"')}</div><label class="form-field">Decision note<textarea id="decision-note" placeholder="Add context for the team. Required when returning for changes."></textarea></label><p class="source-note">Deciding as <strong>George · person · Web</strong>. This closes the same request across Web, Telegram and MCP in the sample.</p>`,
    btn("Return for changes", "decide-reject", "", "danger") +
      btn("Approve implementation", "decide-approve", "check", "primary"),
  );
}
function decide(value) {
  const note = $("#decision-note")?.value.trim() || "";
  if (value === "reject" && !note) {
    $("#decision-note").setCustomValidity("Explain what needs to change.");
    $("#decision-note").reportValidity();
    $("#decision-note").addEventListener(
      "input",
      () => $("#decision-note").setCustomValidity(""),
      { once: true },
    );
    return;
  }
  if (state.decision) return;
  state.decision = value;
  state.decisionNote = note;
  state.runs.find((r) => r.id === "RUN-148").status =
    value === "approve" ? "Running" : "Cancelled";
  state.runs.find((r) => r.id === "RUN-148").step =
    value === "approve" ? "Publish the result" : "Returned for changes";
  const reviewNode = nodes.find((n) => n.id === "review");
  reviewNode.status = value === "approve" ? "Approved" : "Changes requested";
  reviewNode.color = value === "approve" ? "green" : "amber";
  reviewNode.time = "George";
  reviewNode.output = note || "Approved by George via Web";
  const publish = nodes.find((n) => n.id === "publish");
  publish.status = value === "approve" ? "Ready" : "Blocked";
  publish.color = value === "approve" ? "green" : "";
  publish.output =
    value === "approve"
      ? "Release is ready to be published"
      : "Blocked by requested changes";
  if (value === "reject")
    state.cards.find((c) => c.id === "CP-042").lane = "Doing";
  state.events.unshift([
    "14:43",
    "decision",
    `George ${value === "approve" ? "approved the implementation" : "requested changes"}`,
    note || "Implementation review · actor: person · channel: Web",
    "Person · Web",
  ]);
  closeModal();
  render();
  toast(
    value === "approve"
      ? "Approved in the demo. The release step is ready."
      : "Returned in the demo. The card is back in Doing.",
  );
}
function decisionHistory() {
  openModal(
    "Decision history",
    `<div class="review-evidence"><strong>Implementation review · AP-021</strong><p>${state.decision ? (state.decision === "approve" ? "Approved" : state.decision === "cancelled" ? "Closed · cancelled" : "Changes requested") : "Pending"}</p><p>${state.decision ? "George · person · via Web" : "Requested by the run · delivered to Web, Telegram and MCP"}</p><p>${esc(state.decisionNote || "No decision note recorded.")}</p></div><div class="review-evidence"><strong>Scope review · AP-018</strong><p>Approved · George · person · via Telegram</p><p>Invitation lifetime: seven days. Replace outstanding invites on resend.</p></div>`,
    btn("Close", "close"),
  );
}
function artifact(id = "patch") {
  if (id === "brief") {
    brief();
    return;
  }
  const content = {
    plan: [
      "Implementation plan",
      "1. Add workspace-scoped invitation records.\n2. Restrict invitation creation and revocation to admins.\n3. Verify invitation lifetime on acceptance.\n4. Resolve tenant context before granting project access.\n5. Run the pinned acceptance procedure.\n6. Ask a person to review before publishing.",
    ],
    patch: [
      "Feature patch",
      'diff --git a/src/invitations.rs b/src/invitations.rs\n@@ accept_invitation @@\n+ let invite = store.find_invitation(tenant, token).await?;\n+ invite.ensure_pending()?;\n+ invite.ensure_not_expired(clock.now())?;\n+ scopes.require("members:invite")?;\n+ store.accept_for_tenant(tenant, invite, member).await?;\n\nIllustrative excerpt · 7 files in the sample patch.\nThe full implementation is not part of this UI prototype.',
    ],
    report: [
      "Acceptance report",
      "verify_feature@7 / RUN-148\n\nPASS  Admin can send an invitation\nPASS  Non-admin cannot send an invitation\nPASS  Invitation expires after seven days\nPASS  Revoked invitation cannot be accepted\nPASS  Cross-tenant acceptance is refused\nPASS  Duplicate resend replaces pending invitation\n… 12 additional sample checks passed\n\nResult: 18 / 18 passed\nEvidence source: deterministic acceptance procedure",
    ],
  };
  const [title, text] = content[id] || content.plan;
  openModal(
    title,
    `<p class="modal-subtitle">CP-042 · attached evidence · illustrative sample</p><pre class="code-block">${esc(text)}</pre>`,
    btn("Back to review", "review", "decision") +
      btn(
        "Download sample",
        "download-artifact",
        "download",
        "",
        `data-id="${id}"`,
      ),
    true,
  );
}
function brief() {
  openModal(
    "Customer portal · project brief",
    `<div class="body-copy"><p>Give workspace administrators a dependable way to invite members and limit each member to the projects they should see.</p><div class="divider"></div><h3>Approved scope</h3><ul><li>Invite and revoke members by email.</li><li>Expire unaccepted invitations after seven days.</li><li>Enforce the tenant boundary on every acceptance.</li><li>Record role changes and review release evidence.</li></ul><h3>Acceptance</h3><p>Published checks must pass. Implementation requires a person’s review before the work package can be released.</p><div class="divider"></div><p>Approved revision v3 · George · via Telegram · sample record.</p></div>`,
    btn(
      "View requirements",
      "modal-nav",
      "file",
      "",
      'data-view="requirements"',
    ) + btn("Close", "close"),
  );
}
function runDetail(id) {
  const r = state.runs.find((x) => x.id === id);
  if (!r) return;
  const isMain = id === "RUN-148";
  openModal(
    `${esc(id)} · execution`,
    `<p class="modal-subtitle">${esc(r.name)} · ${esc(r.executor)}</p><div class="modal-title-badge">${badge(r.status, r.status === "Failed" ? "red" : r.status === "Waiting approval" ? "amber" : "green")}<span class="mono">${r.duration}</span></div>${isMain ? `<ol class="process-list">${nodes.map((n) => `<li><div><strong>${n.label}</strong><small>${esc(n.executor)} · ${esc(n.output)}</small></div>${badge(n.status, n.color)}</li>`).join("")}</ol>` : `<div class="review-evidence"><strong>${r.step}</strong><p>${r.status === "Failed" ? "Required knowledge pack release-notes@1 could not resolve. Execution was refused before a model call." : r.status === "Running" ? "Bounded executor is implementing the work package in this fixture." : "All declared steps completed; outputs are attached to the work package."}</p><p>Conveyor model cost: ${r.cost}</p></div>`}<details style="margin-top:18px"><summary class="body-copy">Invocation & attempt record</summary><pre class="code-block">${esc(JSON.stringify({ run: id, executor: r.executor, attempt: 1, status: r.status, workflow: "build_feature@7", scope: "project:CP-001", correlation_id: "demo-cp-001-148" }, null, 2))}</pre></details>`,
    btn("Close", "close") +
      (["Running", "Waiting approval", "Queued"].includes(r.status)
        ? btn(
            "Cancel sample run",
            "cancel-run",
            "",
            "danger",
            `data-id="${id}"`,
          )
        : ""),
    true,
  );
}
function newWork() {
  openModal(
    "Start with the work",
    `<p class="modal-subtitle">Create a work item in Customer portal. Nothing is sent to a model.</p><form id="new-work-form"><label class="form-field">What needs to happen?<input name="title" required maxlength="120" placeholder="e.g. Let admins revoke an invitation"></label><label class="form-field">Outcome and acceptance<textarea name="description" required placeholder="Describe the result and how we’ll know it works."></textarea></label><label class="form-field">Execution path<select name="executor"><option>Procedure</option><option>Internal agent</option><option>External agent · MCP</option></select><small>The real app checks bindings, pinned sources and permissions before starting.</small></label></form>`,
    btn("Cancel", "close") +
      '<button class="btn primary" form="new-work-form" type="submit">Create work item</button>',
  );
}
function card(id) {
  const c = state.cards.find((x) => x.id === id);
  if (!c) return;
  openModal(
    `${esc(c.id)} · ${esc(c.title)}`,
    `<div class="modal-title-badge">${badge(c.type, c.color)}${badge(c.lane)}</div><p class="body-copy">${esc(c.description)}</p><div class="divider"></div><label class="form-field">Move to list<select id="card-lane">${["Readying", "Doing", "Inspecting", "Done"].map((l) => `<option ${l === c.lane ? "selected" : ""}>${l}</option>`).join("")}</select></label><p class="source-note">A list move changes the work state. An open approval still needs its own decision.</p>`,
    btn("Close", "close") +
      btn("Save list", "save-card", "check", "primary", `data-id="${id}"`) +
      (id === "CP-042" && !state.decision
        ? btn("Review gate", "review", "decision")
        : ""),
  );
}
function capability(id) {
  const c = capabilities.find((x) => x.id === id);
  if (!c) return;
  openModal(
    esc(c.name),
    `<div class="modal-title-badge">${badge(c.kind, c.tone)}${badge(c.blocked ? "Not ready" : "Ready", c.blocked ? "amber" : "green")}</div><p class="body-copy">${esc(c.desc)}</p><div class="divider"></div><dl class="key-values"><dt>Executor pin</dt><dd class="mono">${esc(c.pin)}</dd><dt>Knowledge</dt><dd>portal-access@3</dd><dt>Prompt recipe</dt><dd class="mono">${c.kind === "Internal agent" ? "feature_work@4" : "Owned by executor"}</dd><dt>Action surface</dt><dd>${c.kind.startsWith("External") ? "Conveyor /mcp" : "Registered allowlist"}</dd></dl>${c.blocked ? '<div class="review-warning">Readiness refused: required pack release-notes@1 has no resolvable leaves. No run can start.</div>' : `<div class="review-evidence">${c.kind === "Internal agent" ? "Loop caps: 8 turns · 20 tool calls · 300 seconds. Exceeding a cap refuses execution; it never silently clamps." : "Procedure and external-agent execution use their own declared boundaries. Internal loop caps are not applied to an external platform."}</div>`}`,
    btn("Inspect knowledge", "modal-nav", "book", "", 'data-view="knowledge"') +
      btn("Close", "close"),
  );
}
let draftSteps = [
  "Read scope",
  "Plan implementation",
  "Execute work package",
  "Verify acceptance",
  "Review implementation",
  "Publish the result",
];
function editProcedure() {
  state.draft = true;
  openModal(
    "Build a feature · draft v8",
    `<p class="modal-subtitle">Edit a local draft. Runs keep their published v7 definition.</p><label class="form-field">Procedure intent<textarea id="procedure-intent">${esc(state.draftIntent)}</textarea></label><div class="mini-label">Step order · selected audit rule requires verification before review</div><div id="draft-steps">${draftSteps.map((s, i) => `<div class="step-edit"><span>${i + 1}. ${esc(s)}</span>${btn("↑", "step-up", "", "tiny", `data-index="${i}" aria-label="Move ${s} earlier" ${i === 0 ? "disabled" : ""}`)}${btn("↓", "step-down", "", "tiny", `data-index="${i}" aria-label="Move ${s} later" ${i === draftSteps.length - 1 ? "disabled" : ""}`)}</div>`).join("")}</div><label class="check-label"><input type="checkbox" id="audit-ack">Acknowledge any high-confidence prompt audit finding if publication returns one.</label>`,
    btn("Save draft", "save-draft", "file") +
      btn("Validate & publish sample", "publish-draft", "check", "primary"),
  );
}
function evalSignoff() {
  openModal(
    "Review the grading instrument",
    `<p class="modal-subtitle">Feature completion · v3 · digest 7b9a2f1c</p><div class="review-evidence"><p><strong>Input sign-off:</strong> ${state.evalSigns.inputs ? "Approved by George" : "Missing"}</p><p><strong>Grading sign-off:</strong> ${state.evalSigns.grading ? "Approved by George" : "Pending"}</p></div><div class="body-copy"><p>Acceptance is binary: all declared checks must pass. A retry is scored as a separate attempt. The subject never receives the expected answer.</p></div><div class="table-wrap" style="margin:16px 0"><table><thead><tr><th>Pilot input</th><th>Observed output</th><th>Grade</th></tr></thead><tbody><tr><td>Accept an expired invite</td><td>Refused with expiry reason</td><td>Pass</td></tr><tr><td>Accept across tenants</td><td>Access refused</td><td>Pass</td></tr><tr><td>Revoke pending invite</td><td>Invite remained usable</td><td>Fail</td></tr></tbody></table></div><p class="source-note">Three illustrative pilot rows. The production sign-off must expose the full instrument and every pilot trace. This sample records a local person’s decision for this digest.</p><label class="check-label"><input type="checkbox" id="eval-confirm" ${state.evalSigns.grading ? "checked" : ""}>I have reviewed the sample grading criteria and pilot outcomes.</label>`,
    btn("Close", "close") +
      btn(
        "Approve sample grading",
        "approve-eval",
        "decision",
        "primary",
        state.evalSigns.grading ? "disabled" : "",
      ),
    true,
  );
}
function startEval() {
  if (!state.evalSigns.inputs || !state.evalSigns.grading) {
    openModal(
      "A sign-off is still needed",
      `<div class="review-warning">eval_not_ready · missing gate: eval_grading</div><p class="body-copy">The current instrument must have input and grading sign-offs from a person. Starting another evaluation cannot bypass this gate.</p>`,
      btn("Close", "close") +
        btn("Review grading", "eval-signoff", "decision", "primary"),
    );
    return;
  }
  state.evalStarted = true;
  render();
  openModal(
    "Evaluation queued in the demo",
    `<p class="body-copy">Sample run ER-019 uses instrument v3 · digest 7b9a2f1c, 50 cases and the fixed train/test split.</p><div class="review-evidence">No model calls or worker jobs were started. This is the queued-state preview.</div>`,
    btn("Close", "close"),
  );
}
function copilot() {
  if (!state.chat.length)
    state.chat.push({
      role: "assistant",
      text: "Workspace invitations is waiting for George’s implementation review. The patch and all 18 passing acceptance checks are attached. I can explain the evidence or the execution boundary.",
    });
  openModal(
    "Copilot · CP-042",
    `<p class="modal-subtitle">Scripted local conversation · interactive help, scoped to this work.</p><div class="chat-log" id="chat-log">${state.chat.map((m) => `<div class="chat-message ${m.role === "user" ? "user" : ""}"><small>${m.role === "user" ? "George" : "Conveyor copilot · sample response"}</small>${esc(m.text)}</div>`).join("")}</div><form class="chat-form" id="chat-form"><input name="message" required placeholder="What is waiting on me?" aria-label="Ask the sample copilot"><button class="btn primary" type="submit">Send ${icon("arrow")}</button></form><p class="prototype-notice">The copilot explains and proposes. Work execution belongs to the run; decisions use the approval gate.</p>`,
    btn("Open review", "review", "decision") + btn("Close", "close"),
  );
}
function commands(query = "") {
  const list = [
    ...navItems.map(([id, label, symbol]) => ({
      label,
      symbol,
      action: "navigate",
      view: id,
      hint: "Destination",
    })),
    {
      label: "Review implementation",
      symbol: "decision",
      action: "review",
      hint: "Approval · CP-042",
    },
    {
      label: "Workspace invitations",
      symbol: "board",
      action: "card",
      id: "CP-042",
      hint: "Work item",
    },
    {
      label: "Start new work",
      symbol: "plus",
      action: "new-work",
      hint: "Create",
    },
    {
      label: "Ask copilot",
      symbol: "chat",
      action: "copilot",
      hint: "Context",
    },
    {
      label: "Inspect RUN-148",
      symbol: "run",
      action: "run-detail",
      id: "RUN-148",
      hint: "Execution",
    },
  ].filter((c) => c.label.toLowerCase().includes(query.toLowerCase()));
  return (
    list
      .map(
        (c) =>
          `<button class="command-result" data-action="${c.action}" ${c.view ? `data-view="${c.view}"` : ""} ${c.id ? `data-id="${c.id}"` : ""}>${icon(c.symbol)}${c.label}<small>${c.hint}</small></button>`,
      )
      .join("") || '<p class="empty">No matching work or destination.</p>'
  );
}
function search() {
  openModal(
    "Find your way",
    `<div class="command-input">${icon("search")}<input id="command-search" autofocus placeholder="Search work, actions, or destinations…" aria-label="Search commands"><kbd>esc</kbd></div><div class="command-results">${commands()}</div>`,
  );
  $("#command-search").focus();
}
function addEvent(symbol, title, detail, source) {
  state.events.unshift(["14:43", symbol, title, detail, source]);
}
function replay() {
  if (state.replaying) return;
  if (state.decision) {
    toast("Reset sample data in Settings to replay the pending review.");
    return;
  }
  state.replaying = true;
  state.events = [];
  let index = initialEvents.length - 1;
  render();
  state.timer = setInterval(() => {
    state.events.unshift(initialEvents[index]);
    index--;
    if (index < 0) {
      clearInterval(state.timer);
      state.replaying = false;
    }
    if (state.view === "workspace" || state.view === "activity") render();
  }, 850);
}
function profile() {
  openModal(
    "George · workspace admin",
    `<div class="modal-title-badge"><span class="avatar">GD</span><strong>Non-Agentic AI</strong>${badge("Person", "green")}</div><p class="body-copy">Sample admin principal. The tenant owns its workspace; there is no separate owner role.</p><div class="review-evidence"><p>Can review project work and eval instruments</p><p>Can manage workspace members and roles</p><p>Can inspect held-out traces with evals:approve</p><p>Cannot provision another tenant from this workspace</p></div>`,
    btn("Open settings", "modal-nav", "settings", "", 'data-view="settings"') +
      btn("Close", "close"),
  );
}
function about() {
  openModal(
    "A more visible conveyor",
    `<div class="body-copy"><p>This is an interactive design study for Conveyor. All data, replay events and decisions are local fixtures. Reloading restores work data; only the color preference is remembered.</p><p>The execution canvas borrows from <a href="https://github.com/OpenBMB/ChatDev" target="_blank" rel="noreferrer">ChatDev</a>. The inspectable workspace takes cues from <a href="https://github.com/deepseek-ai/deepseek-harness" target="_blank" rel="noreferrer">DeepSeek Harness</a>, with the operator and provenance patterns from fr, w2v, care-bus-agents and agent.</p><p>Conveyor’s own contracts govern the shape: three executor kinds, one approval primitive, versioned instruments and a compiled context recipe.</p><div class="divider"></div><p><a href="UI-SPEC.md">UI design notes</a> · <a href="web-ui-proposal.html">Earlier wireframe proposal</a></p></div>`,
    btn("Close", "close"),
  );
}
const actions = {
  menu() {
    const open = document.body.classList.toggle("menu-open");
    $(".scrim").hidden = !open;
    $(".mobile-menu").setAttribute("aria-expanded", String(open));
    if (open) $(".nav-link").focus();
  },
  theme() {
    const dark = document.documentElement.dataset.theme === "dark";
    document.documentElement.dataset.theme = dark ? "light" : "dark";
    try {
      localStorage.setItem("conveyor-prototype-theme", dark ? "light" : "dark");
    } catch {}
    toast(`${dark ? "Light" : "Dark"} theme`);
  },
  advanced() {
    state.advanced = !state.advanced;
    render();
    toast(
      state.advanced
        ? "Versions, references and invocation details are visible."
        : "Advanced details hidden.",
    );
  },
  navigate(el) {
    if (modal.open) closeModal();
    navigate(el.dataset.view);
  },
  "modal-nav"(el) {
    closeModal();
    navigate(el.dataset.view);
  },
  close: closeModal,
  search,
  profile,
  about,
  brief,
  review,
  "decision-history": decisionHistory,
  "decide-approve"() {
    decide("approve");
  },
  "decide-reject"() {
    decide("reject");
  },
  "select-node"(el) {
    state.selected = el.dataset.id;
    render();
    $(`[data-action="select-node"][data-id="${state.selected}"]`)?.focus({
      preventScroll: true,
    });
  },
  "inspector-tab"(el) {
    state.inspectorTab = el.dataset.id;
    render();
    $(`#inspector-tab-${state.inspectorTab}`)?.focus({ preventScroll: true });
  },
  "zoom-out"() {
    state.zoom = Math.max(0.7, state.zoom - 0.1);
    bindGraph();
  },
  "zoom-in"() {
    state.zoom = Math.min(1.5, state.zoom + 0.1);
    bindGraph();
  },
  "zoom-fit"() {
    state.zoom = 1;
    bindGraph();
  },
  replay,
  artifact(el) {
    artifact(el.dataset.id);
  },
  "run-detail"(el) {
    runDetail(el.dataset.id);
  },
  "new-work": newWork,
  card(el) {
    card(el.dataset.id);
  },
  capability(el) {
    capability(el.dataset.id);
  },
  "save-card"(el) {
    const c = state.cards.find((c) => c.id === el.dataset.id);
    c.lane = $("#card-lane").value;
    addEvent(
      "board",
      `${c.id} moved to ${c.lane}`,
      "George · local sample move",
      "Person · Web",
    );
    closeModal();
    render();
    toast(`Moved ${c.id} to ${c.lane}.`);
  },
  "run-filter"(el) {
    state.runFilter = el.dataset.id;
    render();
  },
  "cancel-run"(el) {
    const r = state.runs.find((r) => r.id === el.dataset.id);
    openModal(
      "Cancel this sample run?",
      `<p class="body-copy">${esc(r.id)} · ${esc(r.name)} will move to Cancelled in this prototype.</p>`,
      btn("Keep run", "close") +
        btn(
          "Cancel sample run",
          "confirm-cancel",
          "",
          "danger",
          `data-id="${r.id}"`,
        ),
    );
  },
  "confirm-cancel"(el) {
    const r = state.runs.find((r) => r.id === el.dataset.id);
    r.status = "Cancelled";
    r.step = "Cancelled by George";
    if (r.id === "RUN-148" && !state.decision) {
      state.decision = "cancelled";
      nodes.find((n) => n.id === "review").status = "Closed · cancelled";
      nodes.find((n) => n.id === "review").color = "";
      nodes.find((n) => n.id === "publish").status = "Cancelled";
    }
    addEvent("run", `${r.id} cancelled`, "George · local demo", "Person · Web");
    closeModal();
    render();
    toast("Sample run cancelled.");
  },
  "download-artifact"(el) {
    download(`conveyor-${el.dataset.id}-sample.json`, {
      prototype: true,
      artifact: el.dataset.id,
      work_item: "CP-042",
      run: "RUN-148",
      note: "Illustrative UI fixture, not a production artifact.",
    });
  },
  "create-capability"() {
    openModal(
      "Define a capability",
      `<form id="capability-form"><label class="form-field">Name<input name="name" required maxlength="80" placeholder="e.g. Security review"></label><label class="form-field">Purpose<textarea name="purpose" required placeholder="What outcome does it provide?"></textarea></label><label class="form-field">Executor kind<select name="kind"><option>Internal agent</option><option>Procedure</option><option>External agent · MCP</option></select></label><p class="source-note">New capabilities are blocked until required knowledge and executor bindings resolve.</p></form>`,
      btn("Cancel", "close") +
        '<button class="btn primary" type="submit" form="capability-form">Create sample capability</button>',
    );
  },
  "edit-procedure": editProcedure,
  "step-up"(el) {
    const i = Number(el.dataset.index);
    if (i > 0) {
      [draftSteps[i - 1], draftSteps[i]] = [draftSteps[i], draftSteps[i - 1]];
      editProcedure();
    }
  },
  "step-down"(el) {
    const i = Number(el.dataset.index);
    if (i < draftSteps.length - 1) {
      [draftSteps[i + 1], draftSteps[i]] = [draftSteps[i], draftSteps[i + 1]];
      editProcedure();
    }
  },
  "save-draft"() {
    state.draft = true;
    closeModal();
    render();
    toast("Draft v8 saved in this page’s sample state.");
  },
  "publish-draft"() {
    const verify = draftSteps.indexOf("Verify acceptance");
    const reviewIndex = draftSteps.indexOf("Review implementation");
    if (verify > reviewIndex && !$("#audit-ack").checked) {
      toast(
        "High-confidence finding: verification follows review. Reorder or acknowledge the finding.",
      );
      return;
    }
    closeModal();
    state.draft = false;
    render();
    toast(
      "Sample draft validated. Publication preview complete; no backend was changed.",
    );
  },
  "procedure-detail"(el) {
    const title = el.dataset.id || "build_feature@7";
    openModal(
      "Published procedure",
      `<p class="modal-subtitle">${esc(title)} · immutable published definition</p><ol class="process-list">${(title.startsWith("verify") ? ["Load patch artifact", "Run acceptance checks", "Attach report"] : title.startsWith("publish") ? ["Check approval decision", "Publish work package", "Move card to Done"] : title.startsWith("kata") ? ["Read measured baseline", "Propose one bounded change", "Run fixed eval instrument", "Compare held-out result", "Return for person’s check-in"] : draftSteps).map((s) => `<li><div><strong>${esc(s)}</strong><small>Declared step · output attached to run</small></div></li>`).join("")}</ol>`,
      btn("Close", "close"),
    );
  },
  "knowledge-bindings"() {
    openModal(
      "Pinned knowledge bindings",
      `<p class="body-copy">Readiness resolves each required pack before the run. A missing required source refuses the run; a missing optional source is reported.</p><div class="review-evidence"><p>portal-access@3 → feature development · required</p><p>engineering-standards@8 → all engineering capabilities · required</p><p>ui-patterns@2 → work planning · optional</p></div>`,
      btn("Close", "close"),
    );
  },
  "knowledge-pack"(el) {
    openModal(
      esc(el.dataset.id),
      `<div class="body-copy"><p>Domain knowledge pack from strata_rag, pinned at the capability binding.</p><div class="review-evidence"><p>Provenance: platform → engineering → customer portal</p><p>State: required leaves resolved</p><p>Retrieval: knowledge.query through the shared registry</p></div><p>Pack content is context. Approved requirements and work-package decisions remain authoritative.</p></div>`,
      btn("Close", "close"),
    );
  },
  requirement(el) {
    const r = requirements.find((r) => r[0] === el.dataset.id);
    openModal(
      `${esc(r[0])} · governed requirement`,
      `<div class="modal-title-badge">${badge(r[3], r[3] === "Approved" ? "green" : "amber")}${badge(r[2])}</div><h3>${esc(r[1])}</h3><div class="review-evidence"><p>Acceptance: the declared access boundary is enforced and both positive and negative cases are verified.</p><p>Implementation: ${r[4]} · project CP-001</p></div><details><summary class="body-copy">Revision history</summary><div class="body-copy" style="margin-top:12px">Current: ${r[0] === "REQ-014" ? "v3 · clarified admin scope<br>v2 · linked project access checks<br>v1 · initial requirement" : "v1 · initial requirement"}</div></details>`,
      btn("Trace implementation", "trace", "flow") + btn("Close", "close"),
    );
  },
  "new-requirement"() {
    openModal(
      "Add a requirement",
      `<form id="requirement-form"><label class="form-field">Requirement<input name="title" required maxlength="160" placeholder="Describe one verifiable requirement"></label><label class="form-field">Catalogue<select name="catalogue"><option>Access control</option><option>Invitation lifecycle</option><option>Governance</option></select></label></form>`,
      btn("Cancel", "close") +
        '<button class="btn primary" type="submit" form="requirement-form">Save draft requirement</button>',
    );
  },
  trace() {
    openModal(
      "Intent → implementation → evidence",
      `<ol class="process-list"><li><div><strong>REQ-014 · Approved requirement v3</strong><small>Only admins can invite workspace members</small></div></li><li><div><strong>CP-042 · Workspace invitations</strong><small>Work item in Inspecting</small></div></li><li><div><strong>RUN-148 · build_feature@7</strong><small>External executor + deterministic acceptance procedure</small></div></li><li><div><strong>Acceptance report</strong><small>18 / 18 passed · implementation review ${state.decision || "pending"}</small></div></li></ol>`,
      btn("Inspect report", "artifact", "file", "", 'data-id="report"') +
        btn("Close", "close"),
    );
  },
  "eval-signoff": evalSignoff,
  "start-eval": startEval,
  "approve-eval"() {
    if (!$("#eval-confirm").checked) {
      toast("Review and confirm the sample grading criteria first.");
      $("#eval-confirm").focus();
      return;
    }
    state.evalSigns.grading = true;
    closeModal();
    render();
    toast("Grading signed by George via Web for instrument v3.");
  },
  "eval-filter"(el) {
    state.evalFilter = el.dataset.id;
    render();
  },
  "eval-trace"(el) {
    const fail = el.dataset.id === "EV-27";
    openModal(
      `${esc(el.dataset.id)} · evaluation trace`,
      `<p class="modal-subtitle">${el.dataset.split === "test" ? "Held-out · visible to George with evals:approve" : "Train · visible to analyzer"} · instrument v3</p><ol class="process-list"><li><div><strong>Subject input</strong><small>${fail ? "Revoke a pending invitation and check that it cannot be accepted." : "Verify the requested invitation behavior against the bound tenant context."}</small></div></li><li><div><strong>Tool invocation</strong><small>Registered invitation action · attempt 1</small></div></li><li><div><strong>Observed output</strong><small>${fail ? "Invitation remained usable after revocation." : "Expected access boundary enforced."}</small></div></li><li><div><strong>Grader result</strong><small>${fail ? "Fail: acceptance criterion not met." : "Pass: declared checks satisfied."}</small></div>${badge(fail ? "Fail" : "Pass", fail ? "red" : "green")}</li></ol><div class="source-note" style="margin-top:18px">The expected answer is grader-only. An agent cannot open held-out transcripts in the real application.</div>`,
      btn("Close", "close"),
    );
  },
  "export-eval"() {
    download("conveyor-eval-sample.json", {
      prototype: true,
      instrument: "feature_completion@3",
      digest: "7b9a2f1c",
      split: { train: 25, test: 25 },
      rounds: [72, 76, 80, 80, 84],
      note: "Illustrative report, not a measured product result.",
    });
  },
  "export-usage"() {
    download("conveyor-usage-sample.json", {
      prototype: true,
      calls: 156,
      provider_prompt_cache: { hit: 108, write: 8, miss: 36, unknown: 4 },
      hit_ratio: 108 / (108 + 36),
      cost_usd: 12.48,
    });
  },
  "audit-finding"(el) {
    state.activeFinding = el.dataset.id;
    openModal(
      "Review the proposed prompt change",
      `<div class="modal-title-badge">${badge(el.dataset.id === "repeat" ? "High confidence" : "Medium confidence", "amber")}${badge("Proposed · never auto-applied")}</div><p class="body-copy">Remove repeated instructions from the knowledge section so the published procedure owns the completion condition.</p><pre class="code-block">- Knowledge: always validate acceptance before publishing.\n+ Knowledge: invitation lifecycle facts and domain rules.\n\nProcedure remains authoritative:\n  Verify acceptance → Human review → Publish.</pre><label class="form-field">Review note<textarea id="finding-note" placeholder="Explain the decision for the audit trail"></textarea></label>`,
      btn(
        "Reject proposal",
        "finding-decide",
        "",
        "",
        'data-value="Rejected"',
      ) +
        btn(
          "Accept proposal",
          "finding-decide",
          "check",
          "primary",
          'data-value="Accepted"',
        ),
    );
  },
  "finding-decide"(el) {
    state.findings[state.activeFinding] = el.dataset.value;
    closeModal();
    render();
    toast(
      `${el.dataset.value} in the demo. Applying a change requires editing and publishing a procedure draft.`,
    );
  },
  "channel-detail"(el) {
    const mcp = el.dataset.id === "MCP";
    openModal(
      esc(el.dataset.id),
      `<dl class="key-values"><dt>Principal kind</dt><dd>${mcp ? "Agent" : "Person"}</dd><dt>Record</dt><dd>Tenant-scoped</dd><dt>Decisions</dt><dd>approvals.decide</dd><dt>Transport</dt><dd>${mcp ? "/mcp · durable tasks" : el.dataset.id === "Telegram" ? "Bound Telegram account" : "Web + SSE"}</dd></dl><div class="divider"></div><pre class="code-block">${esc(mcp ? "projects.execute → project execution run\ntasks/get → progress + input request\ntasks/update → approval decision\ntasks/cancel → cancel execution" : "approval.requested → channel delivery\napprovals.decide → actor + channel\napproval.closed → close everywhere")}</pre><p class="prototype-notice">Connection and delivery state shown here are fixtures. No external accounts are connected by this prototype.</p>`,
      btn("Close", "close"),
    );
  },
  copilot,
  reset() {
    openModal(
      "Reset the local sample?",
      `<p class="body-copy">This restores the work board, approval, draft edits and sample runs. Your color preference is preserved.</p>`,
      btn("Keep changes", "close") +
        btn("Reset sample", "confirm-reset", "clock", "primary"),
    );
  },
  "confirm-reset"() {
    location.reload();
  },
};
document.addEventListener("click", (e) => {
  const destination = e.target.closest('a.nav-link[href^="#"]');
  if (destination) {
    e.preventDefault();
    navigate(destination.hash.slice(1));
    return;
  }
  const el = e.target.closest("[data-action]");
  if (!el || el.disabled) return;
  const action = el.dataset.action;
  if (el.classList.contains("command-result") && action === "navigate")
    closeModal();
  actions[action]?.(el);
});
document.addEventListener("submit", (e) => {
  const form = e.target;
  if (form.id === "new-work-form") {
    e.preventDefault();
    const data = new FormData(form);
    const title = String(data.get("title")).trim();
    const description = String(data.get("description")).trim();
    if (!title || !description) {
      toast("Add a title and acceptance criteria.");
      return;
    }
    const id = `CP-${String(46 + state.cards.filter((c) => Number(c.id.split("-")[1]) >= 46).length).padStart(3, "0")}`;
    state.cards.push({
      id,
      title,
      description,
      lane: "Readying",
      type: "Feature",
      owner: "GD",
      color: "green",
    });
    state.runs.unshift({
      id: `RUN-${149 + state.runs.filter((r) => Number(r.id.split("-")[1]) >= 149).length}`,
      name: title,
      executor: String(data.get("executor")),
      status: "Queued",
      step: "Readiness check",
      cost: "$0.00",
      duration: "Not started",
    });
    addEvent("plus", `${id} created`, title, "Person · Web");
    closeModal();
    navigate("board");
    toast(`${id} created locally. The sample run is queued.`);
  }
  if (form.id === "capability-form") {
    e.preventDefault();
    const data = new FormData(form);
    const name = String(data.get("name")).trim();
    if (!name) return;
    const id = `custom-${capabilities.length}`;
    capabilities.push({
      id,
      name,
      kind: String(data.get("kind")),
      desc: String(data.get("purpose")).trim(),
      pin: "unbound draft",
      icon: "agent",
      tone: "amber",
      blocked: true,
    });
    closeModal();
    render();
    toast(
      "Sample capability created. Bindings are required before it is ready.",
    );
  }
  if (form.id === "requirement-form") {
    e.preventDefault();
    const data = new FormData(form);
    const title = String(data.get("title")).trim();
    if (!title) return;
    requirements.push([
      `REQ-${String(14 + requirements.length).padStart(3, "0")}`,
      title,
      String(data.get("catalogue")),
      "Draft",
      "CP-044",
    ]);
    closeModal();
    render();
    toast("Draft requirement added to the sample catalogue.");
  }
  if (form.id === "chat-form") {
    e.preventDefault();
    const data = new FormData(form);
    const message = String(data.get("message")).trim();
    if (!message) return;
    state.chat.push({ role: "user", text: message });
    const answer = /cost|token|model/i.test(message)
      ? "Conveyor model calls for the planner cost $0.84 in this fixture. The external developer’s own model calls belong to the agent platform and are outside this total."
      : /context|knowledge|prompt/i.test(message)
        ? "The internal planner uses feature_work@4, portal-access@3, the approved requirements and two work-package memory facts. Sources compile at call time; recipe version and hash are recorded."
        : /test|check|evidence|patch/i.test(message)
          ? "The acceptance procedure reports 18 / 18 checks passing, including invitation expiry and tenant isolation. Seven changed files are attached. Open the review to inspect the patch and report."
          : state.decision
            ? `George has ${state.decision === "approve" ? "approved the implementation. The release procedure is ready" : "returned the work for changes. Check the decision note and updated work list"}.`
            : "George’s implementation review is the next step. The run waits without holding a worker. Use the review gate to approve or return it with a note.";
    state.chat.push({ role: "assistant", text: answer });
    copilot();
    $("#chat-log").scrollTop = $("#chat-log").scrollHeight;
    $("#chat-form input").focus();
  }
});
document.addEventListener("input", (e) => {
  if (e.target.id === "procedure-intent") state.draftIntent = e.target.value;
  if (e.target.id === "command-search")
    $(".command-results").innerHTML = commands(e.target.value);
  if (e.target.id === "card-search") {
    const start = e.target.selectionStart;
    state.cardQuery = e.target.value;
    main.innerHTML = board();
    $("#card-search").focus();
    if (start !== null) $("#card-search").setSelectionRange?.(start, start);
  }
  if (e.target.id === "run-search") {
    state.runQuery = e.target.value;
    const pos = e.target.selectionStart;
    main.innerHTML = runs();
    $("#run-search").focus();
    try {
      $("#run-search").setSelectionRange(pos, pos);
    } catch {}
  }
});
let dragged = null;
document.addEventListener("dragstart", (e) => {
  const card = e.target.closest("[data-card]");
  if (!card) return;
  dragged = card.dataset.card;
  e.dataTransfer.setData("text/plain", dragged);
  e.dataTransfer.effectAllowed = "move";
});
document.addEventListener("dragover", (e) => {
  const lane = e.target.closest("[data-lane]");
  if (lane) {
    e.preventDefault();
    lane.classList.add("over");
  }
});
document.addEventListener("dragleave", (e) => {
  const lane = e.target.closest("[data-lane]");
  if (lane && !lane.contains(e.relatedTarget)) lane.classList.remove("over");
});
document.addEventListener("drop", (e) => {
  const lane = e.target.closest("[data-lane]");
  if (!lane) return;
  e.preventDefault();
  const c = state.cards.find(
    (c) => c.id === (e.dataTransfer.getData("text/plain") || dragged),
  );
  if (!c) return;
  c.lane = lane.dataset.lane;
  addEvent(
    "board",
    `${c.id} moved to ${c.lane}`,
    "George · local sample move",
    "Person · Web",
  );
  render();
  toast(`Moved ${c.id} to ${c.lane}.`);
  dragged = null;
});
document.addEventListener("dragend", () => {
  $$(".lane.over").forEach((el) => el.classList.remove("over"));
  dragged = null;
});
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    search();
  }
  if (e.key === "Escape" && document.body.classList.contains("menu-open"))
    actions.menu();
  if (e.target.id === "command-search" && e.key === "ArrowDown") {
    e.preventDefault();
    $(".command-result")?.focus();
  }
  if (
    e.target.classList.contains("command-result") &&
    ["ArrowDown", "ArrowUp"].includes(e.key)
  ) {
    e.preventDefault();
    const list = $$(".command-result");
    const i = list.indexOf(e.target);
    list[
      (i + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length
    ].focus();
  }
  if (
    e.target.getAttribute("role") === "tab" &&
    ["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)
  ) {
    e.preventDefault();
    const tabs = $$('[role="tab"]');
    const i = tabs.indexOf(e.target);
    const next =
      e.key === "Home"
        ? 0
        : e.key === "End"
          ? tabs.length - 1
          : (i + (e.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    tabs[next].click();
  }
});
window.addEventListener("hashchange", () => {
  document.body.classList.remove("menu-open");
  $(".scrim").hidden = true;
  $(".mobile-menu").setAttribute("aria-expanded", "false");
  state.view = pages[location.hash.slice(1)]
    ? location.hash.slice(1)
    : "workspace";
  render();
});
window.addEventListener("beforeunload", () => clearInterval(state.timer));
state.view = pages[location.hash.slice(1)]
  ? location.hash.slice(1)
  : "workspace";
render();
