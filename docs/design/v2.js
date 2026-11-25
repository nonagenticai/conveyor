/* SPEC_V2 pack — the organisational operating model.
   Local interaction study only: every record below is a fixture and this file calls no backend.
   The live console reads and writes the real rows behind these screens; `?design=1` keeps this
   study, and a signed-out direct hash is a sign-in gate (docs/design/README.md). */
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
    download,
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
  // The surface checks render the design fixtures, not the working tree's published payload: on
  // a publish branch `data/snapshot.js` is the live pack and would hide every fixture the checks
  // assert. The checks set this flag before any page script runs; a visitor never does (#468).
  const qaNoSnapshot = window.CONVEYOR_QA_NO_SNAPSHOT === true;
  if (qaNoSnapshot) {
    // The generated outputs index is a publish branch's live payload too; the public empty-shape
    // assertions need the placeholder, exactly as for the snapshot.
    window.CONVEYOR_OUTPUTS = null;
  }
  const live = !qaNoSnapshot && S && S.schema === "design-snapshot/v1" ? S : null;
  // The V2 product-design study is not part of the public programme site. It stays available
  // to signed-in operators, and to a reviewer who asks for it explicitly with `?design=1`
  // (the mode `check-v2.cjs` exercises). Without either, a design destination is a sign-in gate.
  const designStudy = (() => {
    try {
      return new URLSearchParams(window.location.search).has("design");
    } catch {
      return false;
    }
  })();
  const studyOn = () => consoleOn() || designStudy;
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

  /* ------------------------------------------------------------- operator console */
  // Served by the app with a same-origin `/config.json`, console.js signs an operator in and
  // the Plan surface reads and writes the platform's own rows. On the homepage mirror and
  // from `file://` there is no config, `consoleOn()` is false, and the snapshot path below is
  // exactly what it was. Nothing is simulated on this path: every control is an action.
  const C = window.ConveyorConsole || null;
  const consoleOn = () => !!(C && C.signedIn());
  const consoleAdmState = () => ({
    loaded: false,
    loading: false,
    profile: { data: null, loaded: false, loading: false, error: null },
    standard: { items: [], loaded: false, loading: false, error: null },
    artifacts: { items: [], loaded: false, loading: false, error: null },
    contracts: { items: [], loaded: false, loading: false, error: null },
    boards: { items: [], loaded: false, loading: false, error: null },
    maturity: { items: [], loaded: false, loading: false, error: null },
    skills: { items: [], loaded: false, loading: false, error: null },
    entries: {},
    links: {},
  });
  const cs = {
    loaded: false,
    loading: false,
    error: null,
    projects: [],
    issues: [],
    types: [],
    byProject: {},
    loadingProject: null,
    lastError: null,
    outputs: {},
    outputsLoading: null,
    register: { items: [], loaded: false, loading: false, error: null, next_cursor: null },
    requests: { items: [], loaded: false, loading: false, error: null, next_cursor: null },
    programme: null,
    runs: [],
    runsLoaded: false,
    runsLoading: false,
    runDetail: {},
    runDetailLoading: null,
    loadingAll: false,
    adm: consoleAdmState(),
    proc: null,
  };
  const consoleIssueUrl = (number) =>
    `https://github.com/nonagenticai/conveyor/issues/${encodeURIComponent(number)}`;
  const consoleIssueOf = (card) =>
    card.issue != null ? card.issue : card.meta ? card.meta.issue : null;
  const consoleError = (error) =>
    (error && (error.detail || error.message)) || "The request failed.";
  const consoleRank = (key) =>
    /^S/.test(String(key)) ? 0 : /^T/.test(String(key)) ? 1 : 2;

  function consoleReset() {
    cs.loaded = false;
    cs.loading = false;
    cs.error = null;
    cs.projects = [];
    cs.issues = [];
    cs.types = [];
    cs.byProject = {};
    cs.loadingProject = null;
    cs.lastError = null;
    cs.outputs = {};
    cs.outputsLoading = null;
    cs.register = { items: [], loaded: false, loading: false, error: null, next_cursor: null };
    cs.requests = { items: [], loaded: false, loading: false, error: null, next_cursor: null };
    cs.programme = null;
    cs.runs = [];
    cs.runsLoaded = false;
    cs.runsLoading = false;
    cs.runDetail = {};
    cs.runDetailLoading = null;
    cs.loadingAll = false;
    cs.adm = consoleAdmState();
    cs.proc = null;
  }

  async function consoleLoad() {
    if (!consoleOn() || cs.loading) return;
    cs.loading = true;
    cs.error = null;
    try {
      const [projectPage, issueRows] = await Promise.all([
        C.get("/api/projects?limit=200"),
        C.issues(),
      ]);
      cs.projects = list(projectPage.items);
      cs.issues = list(issueRows);
      cs.loaded = true;
      if (C.can("programmes.list")) {
        try {
          const programmes = await C.action("programmes.list", { page: { limit: 10 } });
          cs.programme = list(programmes.items)[0] || null;
        } catch (_) {
          cs.programme = null;
        }
      }
      if (C.can("runs.list")) consoleLoadRuns();
      if (C.can("card_types.list")) {
        try {
          const types = await C.action("card_types.list", {});
          cs.types = list(types.items);
        } catch (_) {
          cs.types = [];
        }
      }
    } catch (error) {
      cs.error = error;
    } finally {
      cs.loading = false;
      render();
    }
  }

  async function consoleLoadProject(projectId) {
    if (!consoleOn() || !projectId) return;
    const current = cs.byProject[projectId];
    if (current && !current.stale) return;
    if (cs.loadingProject === projectId) return;
    cs.loadingProject = projectId;
    try {
      const view = await C.get("/api/projects/" + encodeURIComponent(projectId));
      const board = list(view.boards)[0];
      // A project without a board is a real state, not a read failure: its outputs and
      // checklist are documents, and the seed leaves archived rows board-less. Cache the
      // absence so the next render does not re-fetch it and never shows "loading" forever.
      if (!board) {
        cs.byProject[projectId] = { board: null, lists: [], cards: [], stale: false };
        return;
      }
      const [boardView, cardPage] = await Promise.all([
        C.get("/api/boards/" + encodeURIComponent(board.id)),
        C.get("/api/boards/" + encodeURIComponent(board.id) + "/cards?limit=200"),
      ]);
      cs.byProject[projectId] = {
        board: boardView.board || board,
        lists: list(boardView.lists).map((row) => row.list || row),
        cards: list(cardPage.items),
        stale: false,
      };
    } catch (error) {
      cs.byProject[projectId] = { error };
    } finally {
      cs.loadingProject = null;
      render();
    }
  }

  function consoleRefresh(projectId) {
    if (projectId) {
      const current = cs.byProject[projectId];
      cs.byProject[projectId] = current ? Object.assign({}, current, { stale: true }) : { stale: true };
    }
    render();
    if (projectId) consoleLoadProject(projectId);
  }

  // The programme table can show a project's card progress only once its board is loaded. The
  // public pack carries every board; the app does not, so this loads them on request.
  async function consoleLoadAllBoards() {
    if (!consoleOn() || cs.loadingAll) return;
    cs.loadingAll = true;
    render();
    for (const project of consoleProjects()) {
      const data = cs.byProject[project.id];
      if (!data || data.stale || data.error) await consoleLoadProject(project.id);
    }
    cs.loadingAll = false;
    render();
  }

  function consoleProjects() {
    return [...cs.projects].sort(
      (a, b) => consoleRank(a.key) - consoleRank(b.key) || String(a.key).localeCompare(String(b.key)),
    );
  }

  function consoleSelected() {
    const ordered = consoleProjects();
    return ordered.find((row) => row.id === v.planProject) || ordered[0] || null;
  }

  function consoleIssueRows(project) {
    const linked = list((project.meta || {}).issues);
    const byNumber = new Map(cs.issues.map((row) => [String(row.number), row]));
    return linked.map(
      (number) =>
        byNumber.get(String(number)) || {
          number,
          state: "UNKNOWN",
          title: "Not in the published registry",
          url: consoleIssueUrl(number),
          labels: [],
        },
    );
  }

  async function consoleIntake(projectId, issueNumber) {
    const project = cs.projects.find((row) => row.id === projectId);
    const data = cs.byProject[projectId];
    const issue = cs.issues.find((row) => String(row.number) === String(issueNumber));
    if (!project || !data || data.error) {
      toast("The project's board is not loaded yet");
      return;
    }
    if (!data.board) {
      toast("This project has no board; intake a card needs one");
      return;
    }
    const lane = list(data.lists).find((row) => row.kind === "intake") || list(data.lists)[0];
    if (!lane) {
      toast("This board has no lane to intake into");
      return;
    }
    const preferred = list(cs.types).find((row) => row.code === "architecture_task");
    const type = v.intakeType || (preferred && preferred.code) || (list(cs.types)[0] || {}).code;
    if (!type) {
      toast("No card type is available for this tenant; nothing was intaken");
      return;
    }
    try {
      await C.action("cards.create", {
        board_id: data.board.id,
        list_id: lane.id,
        name: (issue && issue.title) || `Issue #${issueNumber}`,
        card_type_code: type,
        meta: {
          issue: Number(issueNumber),
          unit_key: project.key,
          plan: (project.meta || {}).plan,
          process_key: `${type}.READY`,
        },
      });
    } catch (error) {
      cs.lastError = error;
      toast("Intake refused · " + consoleError(error));
      render();
      return;
    }
    cs.lastError = null;
    toast(`Issue #${issueNumber} intaken as a card`);
    consoleRefresh(projectId);
  }

  async function consoleMove(cardId) {
    const select = document.getElementById("console-move");
    if (!select) return;
    const project = consoleSelected();
    try {
      const fresh = await C.get("/api/cards/" + encodeURIComponent(cardId));
      await C.action("cards.move", { id: cardId, list_id: select.value }, { etag: fresh.card.version });
    } catch (error) {
      cs.lastError = error;
      toast("Move refused · " + consoleError(error));
      if (project) consoleRefresh(project.id);
      return;
    }
    cs.lastError = null;
    toast("Card moved");
    const dialog = document.getElementById("dialog");
    if (dialog && dialog.open) dialog.close();
    if (project) consoleRefresh(project.id);
  }

  async function consoleComment(cardId) {
    const input = document.getElementById("console-comment");
    const body = (input && input.value ? input.value : "").trim();
    if (!body) {
      toast("A comment needs a body");
      return;
    }
    const project = consoleSelected();
    try {
      await C.action("cards.comment", { id: cardId, body });
    } catch (error) {
      cs.lastError = error;
      toast("Comment refused · " + consoleError(error));
      return;
    }
    cs.lastError = null;
    toast("Comment recorded");
    if (project) consoleRefresh(project.id);
    consoleCardDialog(cardId);
  }

  async function consoleClaim(cardId) {
    const project = consoleSelected();
    try {
      const fresh = await C.get("/api/cards/" + encodeURIComponent(cardId));
      await C.action("cards.claim", { card_id: cardId }, { etag: fresh.card.version });
    } catch (error) {
      cs.lastError = error;
      toast("Claim refused · " + consoleError(error));
      return;
    }
    cs.lastError = null;
    toast("Card claimed");
    if (project) consoleRefresh(project.id);
    consoleCardDialog(cardId);
  }

  async function consoleRelease(cardId) {
    const project = consoleSelected();
    try {
      await C.action("cards.release", { card_id: cardId });
    } catch (error) {
      cs.lastError = error;
      toast("Release refused · " + consoleError(error));
      return;
    }
    cs.lastError = null;
    toast("Claim released");
    if (project) consoleRefresh(project.id);
    consoleCardDialog(cardId);
  }

  async function consoleCardDialog(cardId) {
    if (!consoleOn()) return;
    let view;
    try {
      view = await C.get("/api/cards/" + encodeURIComponent(cardId));
    } catch (error) {
      toast("Read refused · " + consoleError(error));
      return;
    }
    const card = view.card;
    const project = consoleSelected();
    const data = project ? cs.byProject[project.id] : null;
    const lanes = data ? list(data.lists) : [];
    const comments = list(view.comments);
    const issue = consoleIssueOf(card);
    const body = `<div class="dialog-body console-dialog-body">${kv([
      ["Type", `<span class="mono">${E(card.card_type_code)}</span>`],
      ["Lane", E(((lanes.find((row) => row.id === card.list_id) || {}).name) || card.list_id)],
      [
        "Issue",
        issue != null
          ? `<a class="e-project-link" target="_blank" rel="noopener" href="${E(consoleIssueUrl(issue))}">#${E(issue)} ↗</a>`
          : "—",
      ],
      ["Version", `<span class="mono">${E(card.version)}</span>`],
      ["Claim", card.assignee_id ? E(card.assignee_id) : "unclaimed"],
    ])}<div class="console-actions"><select id="console-move" aria-label="Target lane">${lanes
      .map(
        (lane) =>
          `<option value="${E(lane.id)}" ${lane.id === card.list_id ? "selected" : ""}>${E(lane.name)}</option>`,
      )
      .join(
        "",
      )}</select>${button("Move", "console-move", `data-id="${E(card.id)}"`, "small", "arrow")}${
      C.can("cards.claim")
        ? button("Claim", "console-claim", `data-id="${E(card.id)}"`, "small", "shield")
        : ""
    }${
      C.can("cards.release")
        ? button("Release", "console-release", `data-id="${E(card.id)}"`, "small", "replay")
        : ""
    }</div><div><h3>Comments</h3>${
      comments.length
        ? comments
            .map(
              (comment) =>
                `<div class="console-comment"><small>${E(comment.author_name || "service")} · ${E(short(comment.created_at, 16))}</small>${E(comment.body)}</div>`,
            )
            .join("")
        : `<p class="muted">No comments yet.</p>`
    }</div><label class="field full"><span>Comment</span><textarea id="console-comment" rows="3" placeholder="Record what changed or what you need"></textarea></label>${button(
      "Comment",
      "console-comment",
      `data-id="${E(card.id)}"`,
      "primary",
      "message",
    )}</div>`;
    openDialog(card.name, body, btn("Close", "close-dialog"), true);
  }

  function consoleProjectSelect(project) {
    return `<label class="e-select">Project <select data-v2-input="plan-project">${consoleProjects()
      .map(
        (row) =>
          `<option value="${E(row.id)}" ${row.id === project.id ? "selected" : ""}>${E(row.key)} · ${E(row.name)}</option>`,
      )
      .join("")}</select></label>`;
  }

  /* ------------------------------------------------------------- programme workspace */
  // The Projects destination: live from the platform when the console is signed in, read-only
  // from the published snapshot on the homepage. Every write below is an action; a control that
  // cannot be called is not rendered (or is rendered disabled with the reason).

  const planHref = (plan) => {
    const raw = String(plan || "").trim();
    if (!raw) return "";
    return "docs/" + raw.replace(/^docs\//, "").replace(/\.md(?=#|$)/, ".html");
  };
  const projectIssues = (project) => list((project.meta || {}).issues);
  const issueLabels = (issue) =>
    list(issue.labels)
      .map((label) => (typeof label === "string" ? label : label && label.name))
      .filter(Boolean);
  const isFeedback = (issue) => issueLabels(issue).includes("feedback");
  const orderedProjects = (rows) =>
    [...list(rows)].sort(
      (a, b) => consoleRank(a.key) - consoleRank(b.key) || String(a.key).localeCompare(String(b.key)),
    );
  const snapshotProjects = () => orderedProjects(list(((live || {}).programme || {}).projects));

  /* ------------------------------------------------------------ delivery view */
  // The four levels read one published source: `programme` (programmes + projects), `work`
  // (boards/lists/cards/runs/project_activity) and `measure` (per-project/per-card spend). Signed
  // in, the same shape is assembled from the console's own reads; a project whose board has not
  // loaded yet simply has no bar. A route id is user input, so it is validated as a UUID and used
  // only as a lookup key — never as a function name.
  const DELIVERY_UUID =
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  const routeSegments = () =>
    String(location.hash || "")
      .replace(/^#/, "")
      .split("/")
      .filter(Boolean)
      .map((part) => {
        try {
          return decodeURIComponent(part);
        } catch {
          return part;
        }
      });
  const routeUuid = (index) => {
    const part = routeSegments()[index];
    return part && DELIVERY_UUID.test(part) ? part : null;
  };
  const deliveryPack = () => window.CONVEYOR_DELIVERY || null;

  // Signed in, the console assembles the same shape from its own reads. It has no snapshot, so
  // each project's activity window is computed from the cards and runs it has actually loaded —
  // the same rule the snapshot's `project_activity` uses (first and last observed event), never
  // from the project row's `created_at`. The metering roll-up is not loaded on this page, so spend
  // is honestly absent rather than zero.
  function consoleActivity(projects, boards, cards, runs) {
    const boardProject = new Map(boards.map((row) => [row.id, row.project_id]));
    const events = new Map();
    const add = (projectId, value) => {
      const at = Date.parse(value || "");
      if (!projectId || !Number.isFinite(at)) return;
      const entry = events.get(projectId) || { start: at, end: at };
      entry.start = Math.min(entry.start, at);
      entry.end = Math.max(entry.end, at);
      events.set(projectId, entry);
    };
    cards.forEach((card) => {
      const projectId = boardProject.get(card.board_id);
      add(projectId, card.created_at);
      add(projectId, card.list_changed_at);
      add(projectId, card.archived_at);
    });
    runs.forEach((run) => {
      add(run.project_id, run.created_at);
      add(run.project_id, run.started_at);
      add(run.project_id, run.finished_at);
    });
    return list(projects)
      .filter((project) => events.has(project.id))
      .map((project) => {
        const entry = events.get(project.id);
        return {
          project_id: project.id,
          started_at: new Date(entry.start).toISOString(),
          last_activity_at: new Date(entry.end).toISOString(),
        };
      });
  }

  function deliveryData() {
    if (!consoleOn()) {
      return live
        ? {
            programme: live.programme || {},
            work: live.work || {},
            measure: live.measure || {},
            msp: live.msp || {},
            architecture: live.architecture || {},
            // The audit's rework register rides in the snapshot beside the rows it judges. A pack
            // published from the committed snapshot carries it as its own generated global.
            rework: live.rework || window.CONVEYOR_REWORK || null,
            // The generated MSP/TOGAF/SAFe/PMP record is its own published file.
            standards: window.CONVEYOR_STANDARDS || null,
            generated_at: live.generated_at || null,
          }
        : null;
    }
    const boards = [];
    const lists = [];
    const cards = [];
    Object.values(cs.byProject || {}).forEach((entry) => {
      if (!entry || entry.error) return;
      if (entry.board) boards.push(entry.board);
      lists.push(...list(entry.lists));
      cards.push(...list(entry.cards));
    });
    const programmes = cs.msp && cs.msp.programme ? [cs.msp.programme] : [];
    const projects = consoleProjects();
    const runs = list(cs.runs);
    return {
      programme: { programme: programmes, projects },
      work: {
        boards,
        lists,
        cards,
        runs,
        project_activity: consoleActivity(projects, boards, cards, runs),
      },
      measure: {},
      msp: cs.msp
        ? {
            tranches: cs.msp.tranches,
            benefit_profiles: cs.msp.benefits,
            benefit_links: cs.msp.links,
            benefit_transitions: cs.msp.transitions,
            stakeholder_profiles: cs.msp.stakeholders,
            cadences: cs.msp.cadences,
            wbs_nodes: cs.msp.wbs,
            tranche_reviews: cs.msp.reviews,
            register_items: cs.msp.registers,
            project_baselines: cs.msp.baselines,
            // The reconciliation and realisation reads are published snapshot rows; the console
            // does not compose them, so they are taken from `live` when it is loaded.
            benefit_realisation: live && live.msp ? live.msp.benefit_realisation : null,
            dependencies: live && live.msp ? live.msp.dependencies : null,
            tranche_reconciliation:
              live && live.msp ? live.msp.tranche_reconciliation : null,
          }
        : {},
      // Signed in, the ADM capability is the console's own reads (`consoleLoadAdm`); it is
      // loaded by the delivery page rather than invented here. Until it lands the rails say
      // what is true at that moment — no profile yet — and the load re-renders.
      architecture: {
        profiles: cs.adm.profile.data ? [cs.adm.profile.data] : [],
        standard: cs.adm.standard.items,
        contracts: cs.adm.contracts.items,
        board: cs.adm.boards.items,
        maturity: cs.adm.maturity.items,
        skills: cs.adm.skills.items,
        phase_progress: live && live.architecture ? live.architecture.phase_progress : null,
      },
      // Signed in, the register is still the published audit's: a live snapshot embeds it, and a
      // pack published from the committed snapshot carries it as CONVEYOR_REWORK. The scorecard
      // labels its own `generated_at` rather than presenting it as "now".
      rework: (live && live.rework) || window.CONVEYOR_REWORK || null,
      standards: window.CONVEYOR_STANDARDS || null,
      generated_at: null,
    };
  }

  function deliveryTimeline() {
    const D = deliveryPack();
    if (consoleOn()) consoleLoadAdm();
    const data = deliveryData();
    if (!D || !data) {
      return panel(
        "Delivery timeline",
        note(
          "No published snapshot is loaded. The timeline reads the programme's own rows; sign in at the app origin to operate the live programme.",
          "bad",
        ),
      );
    }
    const clock = Date.parse(data.generated_at || "");
    const index = D.activityIndex(
      data.programme,
      data.work,
      data.measure,
      Number.isFinite(clock) ? clock : Date.now(),
    );
    const selected = routeUuid(1);
    const candidate = selected ? D.programmeRows(data.programme, index, selected, data.msp) : null;
    // An id that names no programme falls back to the portfolio rather than an empty drill.
    const view = candidate && candidate.programme ? candidate : null;
    const l1 = view ? null : D.portfolioRows(data.programme, index, Number.isFinite(clock) ? clock : Date.now(), data.msp);
    const rows = view
      ? D.trancheRows(view.projects, view.artefacts)
      : [...l1.programmes, ...l1.standalone];
    const linked = rows.map((row) =>
      row.group
        ? row
        : {
            ...row,
            href: row.kind === "programme" ? `#portfolio/${row.id}` : `#projects/${row.id}`,
          },
    );
    const totals = D.summary(linked);
    const projects = [...index.values()];
    const projectFlight = projects.filter((row) => row.state !== "completed").length;
    // A tenant total with no project or card attribution is spend that exists; it is named as
    // unattributed rather than reported as "no spend".
    const unattributed = D.tenantSpend(data.measure);
    const spend = totals.hasSpend
      ? D.spendText(totals.tokens, totals.usd, totals.priced, totals.unpriced)
      : unattributed
        ? `${D.spendText(
            unattributed.tokens,
            unattributed.usd,
            unattributed.priced,
            unattributed.unpriced,
          )} unattributed to a project`
        : "no spend recorded";
    // At L1 the portfolio's own rows are the programmes; the project counts behind them are what
    // makes completed work legible at portfolio level (SPEC_V2 §5.1 keeps closed rows out of the
    // in-flight portfolio; their bars live inside the programme at L2).
    const summary = `<div class="delivery-summary">${
      view
        ? `<span><b>${totals.count}</b> ${totals.count === 1 ? "project" : "projects"}</span><span><b>${totals.inFlight}</b> in flight</span><span><b>${totals.completed}</b> completed</span>`
        : `<span><b>${totals.count}</b> ${totals.count === 1 ? "row" : "rows"}</span><span><b>${
            projects.length
          }</b> ${projects.length === 1 ? "project" : "projects"}</span><span><b>${projectFlight}</b> in flight</span><span><b>${
            projects.length - projectFlight
          }</b> completed</span>`
    }<span>${E(spend)}</span></div>`;
    // L1's scorecard sits above the Gantt: the audit's standards record plus the delivery count.
    // A programme view carries the same record in its rails instead.
    const scorecard = view
      ? ""
      : D.scorecard(data.rework, {
          inFlight: projectFlight,
          completed: projects.length - projectFlight,
          spend,
        });
    // Signed in, the console loads a project's board on demand; a project whose board has not
    // loaded has no observed event to draw yet. Say so, and offer the one action that loads them.
    const pendingBoards = consoleOn()
      ? consoleProjects().filter((project) => !cs.byProject[project.id]).length
      : 0;
    const boardsNote = pendingBoards
      ? `<div class="delivery-crumb"><span class="muted">${pendingBoards} project${
          pendingBoards === 1 ? "'s board is" : "s' boards are"
        } not loaded yet, so their activity is not drawn.</span>${button(
          "Load boards",
          "programme-load-boards",
          "",
          "small",
          "board",
        )}</div>`
      : "";
    const head = view
      ? `<div class="delivery-crumb"><a class="gantt-name" href="#portfolio">← Portfolio</a><span class="muted">${E(
          view.programme ? view.programme.name : "Programme",
        )}</span><a class="gantt-name" href="#programme">Programme definition →</a></div>${D.trancheRail(
          view.artefacts,
        )}${D.benefitRail(view.artefacts)}${D.dependencyRail(data.msp ? data.msp.dependencies : null)}${D.reconciliationRail(
          data.msp ? data.msp.tranche_reconciliation : null,
        )}${D.phaseStrip(data.architecture ? data.architecture.phase_progress : null, view.projects)}${D.cadenceRail(
          view.artefacts,
        )}${D.reworkRail(data.rework, view.projects)}${D.artefactRail(data.standards, {
          programmeId: view.programme ? view.programme.id : "",
        })}${D.projectArtefactRollup(data.standards, view.projects)}${D.governanceLine(
          view.artefacts,
          data.architecture,
        )}`
      : "";
    const body = `${head}${boardsNote}${summary}${scorecard}${D.standardsSummary(
      data.standards,
    )}${D.gantt(linked, {
      label: view ? "Project · MSP tranche" : "Programme / initiative",
      emptyNote: view
        ? "This programme has no project in the published snapshot."
        : "No in-flight programme or standalone initiative is published yet. Closed and archived work is history; a programme's own history is shown inside it.",
    })}${note(
      "A bar runs from the first observed card or run activity to completion — or to the snapshot time while the work is in flight. A project with no observed event says so rather than inventing a bar. A programme rolls its projects up and carries its MSP tranche and benefit state; at programme level the projects are grouped by tranche. Spend is the ledger's tokens and its derived cost; an unpriced model is counted and marked, never priced as free.",
    )}`;
    return panel(
      view ? "Delivery timeline · programme" : "Delivery timeline · portfolio",
      body,
      view
        ? `<span class="console-live">${totals.count} ${totals.count === 1 ? "project" : "projects"}</span>`
        : `<span class="console-live">${totals.count} ${totals.count === 1 ? "row" : "rows"}</span>`,
    );
  }

  function deliveryBoard(project) {
    const D = deliveryPack();
    if (consoleOn()) consoleLoadAdm();
    const data = deliveryData();
    if (!D || !data) {
      return panel(
        "Board",
        note(
          "No published snapshot is loaded. The board reads the programme's own cards; sign in at the app origin to operate it.",
          "bad",
        ),
      );
    }
    if (consoleOn() && !cs.byProject[project.id]) {
      return panel("Board", note("Loading this project's board…"));
    }
    const cardId = routeUuid(3);
    const projectId = routeUuid(1);
    const isPackage = cardId && projectId === project.id && routeSegments()[2] === "wp";
    if (isPackage) {
      const model = D.packageModel(data.work, project, cardId, data.measure, data.rework);
      const done = model.children.filter(
        (card) => card.archived_at || ["done", "archive"].includes(model.laneKinds.get(card.list_id)),
      ).length;
      return panel(
        `${project.key} · Work package`,
        `${D.packageBoard(model, data.work)}${D.artefactRail(data.standards, {
          cardId: model.parent ? model.parent.id : cardId,
          projectKey: project.key,
        })}${note(
          "A work package is the card: its execution (runs, gate, Definition of Done, spend) is shown above its own board. The board below holds its child cards across their board's lanes; archived children stay visible as done. A card with no children is still a work package — its execution is the whole answer.",
        )}`,
        model.parent
          ? B(
              model.children.length
                ? `${done}/${model.children.length} children done`
                : "no child cards",
              model.children.length && done === model.children.length ? "green" : "",
            )
          : "",
      );
    }
    const model = D.boardModel(data.work, project, data.measure, data.msp, data.rework);
    const packages = model.roots.filter((card) => (model.children.get(card.id) || []).length > 0);
    return panel(
      `${project.key} · Board`,
      `${D.projectGovernance(project, data.msp, data.architecture, data.work)}${D.artefactRail(
        data.standards,
        { projectKey: project.key },
      )}${D.kanban(model)}${note(
        "A card with children is a work package; open any card for its own board and its execution (runs, gate, Definition of Done, spend). Tokens and cost are the card's own ledger; the work-package tile adds its children.",
      )}`,
      model.boards.length
        ? B(
            `${model.roots.length} cards · ${packages.length} packages · ${model.archived.length} archived`,
            model.roots.length ? "green" : "",
          )
        : "",
    );
  }

  /* ------------------------------------------------------- MSP programme store */
  // One source, fail closed (#305 Part C): signed in the programme definition, plan spine,
  // dependencies and in-flight rows come from the actions; signed out they come from the
  // published snapshot (`SNAP.msp`) read-only; `?design=1` keeps the study fixtures; with no
  // published rows the page says so and never renders a fixture on the live site.
  const MSP0 = () => ({
    loaded: false,
    loading: false,
    error: null,
    programme: null,
    projectId: null,
    documents: [],
    benefits: [],
    links: [],
    transitions: [],
    tranches: [],
    reviews: [],
    stakeholders: [],
    wbs: [],
    baselines: [],
    registers: [],
    cadences: [],
    dependencies: [],
    inFlight: [],
  });
  const msp = () => (cs.msp = cs.msp || MSP0());
  const mspProjectOf = (rows) =>
    list(rows).find((p) => p.key === "P0") || list(rows)[0] || null;
  const mspSource = () => {
    if (consoleOn()) return Object.assign({ readOnly: false, live: true }, msp());
    const M = ((live || {}).msp || {});
    const projects = snapshotProjects();
    return {
      readOnly: true,
      live: !!live,
      programme: list(((live || {}).programme || {}).programme)[0] || null,
      projectId: (mspProjectOf(projects) || {}).id || null,
      documents: list(((live || {}).artefacts || {}).documents),
      benefits: list(M.benefit_profiles),
      links: list(M.benefit_links),
      transitions: list(M.benefit_transitions),
      tranches: list(M.tranches),
      reviews: list(M.tranche_reviews),
      stakeholders: list(M.stakeholder_profiles),
      wbs: list(M.wbs_nodes),
      baselines: list(M.programme_baselines).concat(list(M.project_baselines)),
      registers: list(M.register_items),
      cadences: list(M.cadences),
      dependencies: list(M.project_dependencies),
      inFlight: [],
    };
  };
  const mspError = () => {
    const m = msp();
    return m.error
      ? note(`<strong>${E(m.error.status || "Error")} · ${E(consoleError(m.error))}</strong> The row was not changed.`, "bad")
      : "";
  };
  const mspField = (key, label, mode, type, value, source) => {
    const full = mode === "area";
    const tag = (inner) =>
      `<label class="field${full ? " full" : ""}"><span>${E(label)}</span>${inner}</label>`;
    if (Array.isArray(mode)) {
      return tag(`<select id="msp-${key}">${[["", "—"]].concat(mode.map((x) => [x, x])).map(([val, text]) => `<option value="${E(val)}" ${String(value || "") === String(val) ? "selected" : ""}>${E(text)}</option>`).join("")}</select>`);
    }
    if (type === "benefit" || type === "project") {
      const rows = type === "benefit" ? source.benefits : snapshotProjects().concat(consoleOn() ? consoleProjects() : []);
      const seen = new Set();
      return tag(`<select id="msp-${key}"><option value="">—</option>${rows.filter((row) => row.id && !seen.has(row.id) && seen.add(row.id)).map((row) => `<option value="${E(row.id)}" ${String(value || "") === String(row.id) ? "selected" : ""}>${E(type === "benefit" ? row.reference || short(row.id, 8) : row.key || short(row.id, 8))}</option>`).join("")}</select>`);
    }
    if (type === "int" || type === "float") {
      return tag(`<input id="msp-${key}" type="number" step="${type === "int" ? 1 : "any"}" value="${E(value == null ? "" : value)}">`);
    }
    if (full) return tag(`<textarea id="msp-${key}" rows="2">${E(value == null ? "" : value)}</textarea>`);
    return tag(`<input id="msp-${key}" value="${E(value == null ? "" : value)}">`);
  };
  const MSP_DEFS = {
    programme: [["name", "Name"], ["status", "Status", ["active", "paused", "closed"]]],
    benefit: [
      ["reference", "Reference"], ["description", "Description", "area"], ["category", "Category"],
      ["objectives", "Objectives", "area"], ["observable_outcome", "Observable outcome", "area"],
      ["kpis", "KPIs", "area"], ["baseline", "Baseline"], ["target", "Target"],
      ["measurement_method", "Measurement method", "area"], ["financial_value", "Financial value"],
      ["cost", "Cost"], ["capabilities", "Capabilities", "area"], ["preconditions", "Preconditions", "area"],
      ["risks", "Risks", "area"], ["owner_job_id", "Owner job id"],
      ["benefit_owner_job_id", "Benefit owner job id"], ["operational_area", "Operational area"], ["status", "Status"],
    ],
    link: [
      ["benefit_id", "Benefit", null, "benefit"], ["output_ref", "Output"], ["capability_ref", "Capability"],
      ["operational_outcome", "Operational outcome", "area"], ["strategic_objective", "Strategic objective"],
      ["kind", "Kind", ["benefit", "disbenefit"]],
    ],
    transition: [
      ["benefit_id", "Benefit", null, "benefit"], ["baseline_measured_at", "Baseline measured (date)"],
      ["readiness", "Readiness", "area"], ["enacted_at", "Enacted (date)"],
      ["post_measured_at", "Post measured (date)"], ["retired_at", "Legacy retired (date)"],
      ["evidence", "Evidence", "area"],
    ],
    tranche: [
      ["number", "Number", null, "int"], ["entry_criteria", "Entry criteria", "area"],
      ["exit_criteria", "Exit criteria", "area"], ["baseline_ref", "Baseline ref"], ["review_at", "Review at (date)"],
    ],
    review: [
      ["decision", "Decision", ["continue", "pause", "stop"]],
      ["baselines", "Evidence · baselines", "area"], ["benefit_status", "Evidence · benefit status", "area"],
      ["risk_register", "Evidence · risk register", "area"],
    ],
    stakeholder: [
      ["name", "Name"], ["project_id", "Project id"], ["concerns", "Concerns", "area"],
      ["sensitivities", "Sensitivities", "area"], ["importance", "Importance"], ["influence", "Influence"],
      ["interest", "Interest"], ["exposure", "Exposure"], ["support_level", "Support level"],
      ["support_trend", "Support trend"], ["engagement_target", "Engagement target"],
      ["benefits", "Benefits", "area"], ["disbenefits", "Disbenefits", "area"],
    ],
    wbs: [
      ["kind", "Kind", ["phase", "deliverable", "work_package", "milestone"]], ["title", "Title"],
      ["parent_id", "Parent id"], ["body", "Body", "area"], ["acceptance", "Acceptance", "area"],
      ["business_value", "Business value", null, "int"], ["time_criticality", "Time criticality", null, "int"],
      ["risk_reduction", "Risk reduction", null, "int"], ["job_size", "Job size", null, "int"],
      ["position", "Position", null, "float"],
    ],
    register: [
      ["kind", "Kind", ["risk", "issue", "assumption", "dependency", "benefit", "change"]],
      ["title", "Title"], ["body", "Body", "area"], ["status", "Status"], ["severity", "Severity"],
      ["probability", "Probability"], ["impact", "Impact", null, "int"],
    ],
    cadence: [
      ["kind", "Kind", ["pi", "sprint", "demo", "retro", "refinement"]], ["period", "Period"], ["state", "State"],
    ],
    dependency: [["to_project", "To project", null, "project"], ["kind", "Kind", ["blocks", "needs", "supersedes", "informs"]]],
  };
  const mspDialog = (kind, title, row = {}, source = mspSource()) => {
    const body = `<div class="dialog-body console-dialog-body"><div class="form-grid">${(MSP_DEFS[kind] || []).map(([key, label, mode, type]) => mspField(key, label, mode, type, row[key], source)).join("")}</div><p class="muted">Every field is sent to the action; an If-Match version guards an edit.</p></div>`;
    const footer = `${button("Save", "msp-save", `data-kind="${E(kind)}" data-id="${E(row.id || "")}" data-version="${E(row.version == null ? "" : row.version)}"`, "primary", "check")}${btn("Close", "close-dialog")}`;
    openDialog(title, body, footer, true);
  };
  const mspValue = (key) => {
    const el = document.getElementById("msp-" + key);
    return el ? String(el.value == null ? "" : el.value).trim() : "";
  };
  const mspPayload = (kind) => {
    const payload = {};
    for (const [key, , , type] of MSP_DEFS[kind] || []) {
      const raw = mspValue(key);
      if (!raw) continue;
      const value = type === "int" ? parseInt(raw, 10) : type === "float" ? parseFloat(raw) : raw;
      // The store's `*_at` fields are timestamptz: RFC 3339. A date-only entry is accepted and
      // anchored at midnight UTC rather than left to fail the schema.
      payload[key] =
        key.endsWith("_at") && /^\d{4}-\d{2}-\d{2}$/.test(value) ? `${value}T00:00:00Z` : value;
    }
    return payload;
  };
  async function consoleLoadMsp() {
    const m = msp();
    if (!consoleOn() || m.loading || m.loaded) return;
    m.loading = true;
    m.error = null;
    try {
      const programmes = await C.action("programmes.list", { page: { limit: 10 } });
      m.programme = list(programmes.items)[0] || null;
      // The project-scoped MSP reads need the tenant's projects. The console load may still be
      // in flight when this page renders, so read them here rather than risking an empty
      // project id and a silently skipped set of rows.
      let projects = consoleProjects();
      if (!projects.length) {
        try {
          const page = await C.get("/api/projects?limit=200");
          projects = list(page.items);
          if (!cs.loaded) cs.projects = projects;
        } catch (_) {
          projects = [];
        }
      }
      const p0 = mspProjectOf(projects) || null;
      m.projectId = p0 ? p0.id : null;
      const pid = m.programme ? m.programme.id : null;
      const projectId = m.projectId;
      const safe = async (name, input) => {
        try { return await C.action(name, input); } catch (error) { return { error }; }
      };
      const [benefits, links, transitions, tranches, reviews, stakeholders, cadences, wbs, baselines, registers, dependencies, documents, inFlight] = await Promise.all([
        pid ? safe("benefits.list", { programme_id: pid }) : {},
        pid ? safe("benefits.link_list", { programme_id: pid }) : {},
        pid ? safe("benefits.transition_list", { programme_id: pid }) : {},
        pid ? safe("tranches.list", { programme_id: pid }) : {},
        pid ? safe("tranches.review_list", { programme_id: pid }) : {},
        pid ? safe("stakeholders.profile_list", { programme_id: pid }) : {},
        pid ? safe("cadences.list", { programme_id: pid }) : {},
        projectId ? safe("wbs.list", { project_id: projectId }) : {},
        projectId ? safe("baselines.list", { project_id: projectId }) : {},
        projectId ? safe("registers.list", { project_id: projectId }) : {},
        projectId ? safe("project_dependencies.list", { project_id: projectId }) : {},
        projectId ? safe("documents.list", { project_id: projectId, page: { limit: 100 } }) : {},
        safe("portfolio.in_flight", {}),
      ]);
      m.benefits = list(benefits.items);
      m.links = list(links.items);
      m.transitions = list(transitions.items);
      m.tranches = list(tranches.items);
      m.reviews = list(reviews.items);
      m.stakeholders = list(stakeholders.items);
      m.cadences = list(cadences.items);
      m.wbs = list(wbs.items);
      m.baselines = list(baselines.items);
      m.registers = list(registers.items);
      m.dependencies = list(dependencies.items);
      m.documents = list(documents.items).filter((doc) => doc.status !== "deleted");
      m.inFlight = list(inFlight.items);
      m.loaded = true;
    } catch (error) {
      m.error = error;
    } finally {
      m.loading = false;
      render();
    }
  }
  function mspRefresh() {
    cs.msp = null;
    render();
    if (consoleOn()) consoleLoadMsp();
  }
  // The programme's truth is generated in the repository and compiled into the image; this
  // action re-applies it to the tenant. Idempotent — a second click reports every row skipped.
  async function consoleRefreshProgramme() {
    if (!consoleOn()) return;
    try {
      const out = await C.action("programmes.refresh", {});
      cs.loaded = false;
      cs.projects = [];
      mspRefresh();
      consoleLoad();
      toast(
        "Programme refreshed · " +
          `${out.created || 0} created · ${out.updated || 0} updated · ${out.skipped || 0} skipped`,
      );
    } catch (error) {
      msp().error = error;
      render();
      toast(consoleError(error));
    }
  }
  async function mspWrite(name, input, etag, message) {
    try {
      await C.action(name, input, etag == null ? {} : { etag: String(etag) });
      toast(message || "Saved");
      mspRefresh();
      return true;
    } catch (error) {
      msp().error = error;
      render();
      toast("Refused · " + consoleError(error));
      return false;
    }
  }
  async function mspSave(kind, id, version) {
    const source = mspSource();
    const programmeId = (source.programme || {}).id;
    const projectId = source.projectId;
    const payload = mspPayload(kind);
    const etag = version === "" ? null : version;
    let name;
    let input;
    if (kind === "programme") {
      name = "programmes.update";
      input = Object.assign({ id }, payload);
    } else if (kind === "benefit") {
      name = id ? "benefits.update" : "benefits.create";
      input = id ? Object.assign({ id }, payload) : Object.assign({ programme_id: programmeId }, payload);
    } else if (kind === "link") {
      name = "benefits.link";
      input = Object.assign({ programme_id: programmeId }, payload);
    } else if (kind === "transition") {
      name = "benefits.transition_upsert";
      input = Object.assign({}, payload);
      const existing = source.transitions.find((row) => row.benefit_id === payload.benefit_id);
      if (existing) input._etag = existing.version;
    } else if (kind === "tranche") {
      name = id ? "tranches.update" : "tranches.create";
      input = id ? Object.assign({ id }, payload) : Object.assign({ programme_id: programmeId }, payload);
    } else if (kind === "review") {
      name = "tranches.review";
      input = { id, decision: payload.decision, evidence: { baselines: payload.baselines || "", benefit_status: payload.benefit_status || "", risk_register: payload.risk_register || "" } };
    } else if (kind === "stakeholder") {
      name = id ? "stakeholders.profile_update" : "stakeholders.profile_create";
      input = id ? Object.assign({ id }, payload) : Object.assign({ programme_id: programmeId }, payload);
    } else if (kind === "wbs") {
      name = id ? "wbs.update" : "wbs.create";
      input = id ? Object.assign({ id }, payload) : Object.assign({ project_id: projectId }, payload);
    } else if (kind === "register") {
      name = id ? "registers.update" : "registers.create";
      input = id ? Object.assign({ id }, payload) : Object.assign({ project_id: projectId }, payload);
    } else if (kind === "cadence") {
      name = id ? "cadences.update" : "cadences.create";
      input = id ? Object.assign({ id }, payload) : Object.assign({ programme_id: programmeId }, payload);
    } else if (kind === "dependency") {
      name = "project_dependencies.create";
      input = Object.assign({ project_id: projectId }, payload);
    }
    if (!name) return false;
    if (kind === "link" && !payload.benefit_id) {
      toast("Choose a benefit first");
      return false;
    }
    const transitionEtag = kind === "transition" && input ? input._etag : null;
    if (input) delete input._etag;
    return mspWrite(name, input, id ? etag : transitionEtag || null, "Saved");
  }
  async function mspDelete(kind, id, version) {
    const actions = {
      benefit: "benefits.delete",
      link: "benefits.unlink",
      tranche: "tranches.delete",
      stakeholder: "stakeholders.profile_delete",
      wbs: "wbs.delete",
      register: "registers.delete",
      cadence: "cadences.delete",
      dependency: "project_dependencies.delete",
    };
    if (!actions[kind]) return false;
    return mspWrite(actions[kind], { id }, version === "" ? null : version, "Deleted");
  }
  const mspName = (source, id) => {
    const row = list(source.benefits).find((b) => b.id === id);
    return row ? row.reference || short(id, 8) : short(id || "", 8);
  };
  const mspProjectName = (id) => {
    const row = snapshotProjects().concat(consoleOn() ? consoleProjects() : []).find((p) => p.id === id);
    return row ? row.key : short(id || "", 8);
  };
  const mspDocLink = (doc) => {
    const raw = doc.public_url || doc.public_path || "";
    const url = raw ? safeUrl(raw) : null;
    return url ? `<a class="e-project-link" href="${E(url)}" target="_blank" rel="noopener">public ↗</a>` : `<span class="muted">—</span>`;
  };
  function mspConfirmDelete(kind, id, version) {
    openDialog(
      "Delete row",
      `<div class="dialog-body console-dialog-body"><p class="muted">Delete this ${E(kind)} row? The action is version-guarded and a referenced row is refused (409).</p></div>`,
      `${button("Delete", "msp-delete-save", `data-kind="${E(kind)}" data-id="${E(id)}" data-version="${E(version == null ? "" : version)}"`, "primary", "close")}${btn("Close", "close-dialog")}`,
    );
  }
  function mspEditRow(kind, id) {
    const source = mspSource();
    const rows = kind === "benefit" ? source.benefits : kind === "tranche" ? source.tranches : kind === "stakeholder" ? source.stakeholders : kind === "wbs" ? source.wbs : kind === "register" ? source.registers : kind === "cadence" ? source.cadences : [];
    const row = list(rows).find((x) => x.id === id);
    if (!row) return toast("That row is not loaded; reload the page");
    return mspDialog(kind, "Edit " + kind, row, source);
  }
  function mspDefinition(source) {
    const p = source.programme || {};
    return `${mspError()}${panel(
      "Programme",
      `${kv([
        ["Name", E(p.name || "—")],
        ["Key", E(p.key || p.slug || "—")],
        ["Status", B(p.status || "active", p.status === "active" || !p.status ? "green" : "amber")],
        ["Version", E(p.version == null ? "—" : "v" + p.version)],
      ])}${note("The programme row is the store's row. A definition artefact is a document; the gate is its publishable state, not a local approval.")}`,
      `${source.readOnly ? "" : button("Edit programme", "msp-programme-edit", "", "small", "edit")}${
        source.readOnly || !C.can("programmes.refresh")
          ? ""
          : button("Refresh programme", "programmes-refresh", "", "small", "replay")
      }`,
    )}${panel(
      "Definition set",
      `${table(
        ["Artefact", "Category", "Publishable", "Version", "Public"],
        list(source.documents).length
          ? list(source.documents).map((doc) => `<tr><td><strong>${E(doc.title || doc.category || short(doc.id, 8))}</strong><small class="mono">${E(short(doc.id, 12))}</small></td><td>${E(doc.category || "—")}</td><td>${doc.publishable === undefined ? `<span class="muted">—</span>` : B(doc.publishable ? "publishable" : "not publishable", doc.publishable ? "green" : "amber")}</td><td>${E(doc.version == null ? "—" : "v" + doc.version)}</td><td>${mspDocLink(doc)}</td></tr>`)
          : [`<tr><td colspan="5">No definition-set document is published for this project.</td></tr>`],
      )}${note("A definition artefact is a document row: its producer, reviewer and approver live in its own history. The public page shows only a document that is publishable.")}`,
      B(`${list(source.documents).length} documents`),
    )}`;
  }
  /* The programme's projects as one dossier table, read from the rows this destination already
     loads: signed in the console's `/api/projects` read (`cs.projects`), published the snapshot's
     `live.programme.projects`. Nothing is fetched here, no fixture is drawn on a store path, and
     a column with no source says so instead of guessing. */
  function mspDossier(source) {
    const signed = consoleOn();
    const rows = signed ? consoleProjects() : snapshotProjects();
    const M = (live || {}).msp || {};
    const mspState = signed ? msp() : null;
    // `baselines.list` and `documents.list` are read for the definition-set project only.
    const scopedProjectId = mspState ? mspState.projectId : null;
    const trancheByNumber = new Map();
    list(source.tranches).forEach((t) => {
      const number = Number(t.number);
      if (Number.isFinite(number)) trancheByNumber.set(number, t);
    });
    const baselines = list(source.baselines);
    const baselinesLoaded = signed
      ? !!(mspState && mspState.loaded)
      : Array.isArray(M.project_baselines);
    const documents = list(source.documents);
    const documentsLoaded = signed
      ? !!(mspState && mspState.loaded)
      : Array.isArray(((live || {}).artefacts || {}).documents);
    const statusToneOf = (status) =>
      status === "active" ? "green" : status === "closed" || status === "archived" ? "" : "amber";
    const trancheCell = (project) => {
      const number = (project.meta || {}).tranche;
      if (number == null || String(number).trim() === "") {
        return `<span class="muted">not in this read</span>`;
      }
      // The tranche rows carry no project id; the number on the project's own row is the join.
      const row = trancheByNumber.get(Number(number));
      return `T${E(number)}${row ? ` · ${B(row.state || "planned", row.state === "closed" ? "green" : "amber")}` : ""}`;
    };
    const baselineCell = (project) => {
      if (!baselinesLoaded) return `<span class="muted">not in this read</span>`;
      if (signed && project.id !== scopedProjectId) {
        // The console's baseline read is one project's, so another project's absence here is
        // not evidence that it holds no baseline.
        return `<span class="muted">not in this read</span>`;
      }
      const matches = baselines.filter((b) => b.project_id === project.id);
      return matches.length
        ? matches.map((b) => `<code>${E(b.kind || "—")}</code>`).join(" ")
        : `<span class="muted">none recorded</span>`;
    };
    const evidenceCell = (project) => {
      const issues = projectIssues(project).length;
      const scoped = !signed || project.id === scopedProjectId;
      const docs =
        documentsLoaded && scoped
          ? documents.filter((d) => d.project_id === project.id)
          : null;
      const evidence = docs ? docs.filter((d) => d.category === "evidence").length : 0;
      const documentsText = docs
        ? `${docs.length} document${docs.length === 1 ? "" : "s"}${evidence ? ` · ${evidence} evidence` : ""}`
        : "documents not in this read";
      return `<small>${E(documentsText)} · ${issues} linked issue${issues === 1 ? "" : "s"}</small>`;
    };
    const provenance = signed
      ? "signed in · cs.projects, the console's own /api/projects read"
      : live
        ? "published snapshot · live.programme.projects"
        : "no project store or snapshot is loaded on this pack";
    return `${mspError()}${panel(
      "Projects dossier",
      `${table(
        ["Project", "Status", "Tranche", "Baseline", "Evidence / output"],
        rows.length
          ? rows.map((project) => `<tr><td><strong>${E(project.key || "—")}</strong><small>${E(project.name || "")}</small></td><td>${B(project.status || "—", statusToneOf(project.status))}</td><td>${trancheCell(project)}</td><td>${baselineCell(project)}</td><td>${evidenceCell(project)}</td></tr>`)
          : [`<tr><td colspan="5">No project rows in this store/snapshot.</td></tr>`],
      )}${note(
        `Source: <span class="mono">${E(provenance)}</span>. The tranche is the project row's own <span class="mono">meta.tranche</span>, matched by number to the programme's tranche rows; those rows carry no project id, so a project without one reads "not in this read". Baselines match <span class="mono">project_id</span> in the rows this destination already loads; signed in the MSP read is scoped to the definition-set project, so another project's baseline reads "not in this read", not "none". Documents are counted per project from the loaded document rows; linked issues are the project's own <span class="mono">meta.issues</span>. Nothing is fetched here and no fixture is rendered.`,
      )}`,
      B(`${rows.length} project${rows.length === 1 ? "" : "s"}`, rows.length ? "green" : ""),
    )}`;
  }
  function mspBenefits(source) {
    const rows = list(source.benefits);
    const chosen = rows.find((b) => b.id === v.mspBenefit) || rows[0] || {};
    const links = list(source.links);
    return `${mspError()}<div class="v2-split"><div>${panel(
      "Benefit profiles",
      `${table(
        ["Reference", "Benefit", "Category", "Status"],
        rows.length
          ? rows.map((b) => `<tr class="${b.id === chosen.id ? "selected" : ""}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="msp-benefit" data-id="${E(b.id)}">${E(b.reference || short(b.id, 8))}</button></td><td><strong>${E(b.description || "—")}</strong><small>${E(b.observable_outcome || "")}</small></td><td>${E(b.category || "—")}</td><td>${B(b.status || "—", b.status === "tracking" || b.status === "Tracking" ? "green" : "amber")}</td></tr>`)
          : [`<tr><td colspan="4">No benefit profile is recorded.</td></tr>`],
      )}${source.readOnly ? "" : `<div class="toolbar">${button("New benefit", "msp-benefit-new", "", "small", "plus")}</div>`}`,
      B(`${rows.length} profiles`),
    )}</div><div>${panel(
      "Profile · " + E(chosen.reference || "none"),
      chosen.id
        ? `${kv([
            ["Benefit", E(chosen.description || "—")],
            ["Category", E(chosen.category || "—")],
            ["Objectives", E(chosen.objectives || "—")],
            ["Observable outcome", E(chosen.observable_outcome || "—")],
            ["KPIs", E(chosen.kpis || "—")],
            ["Baseline", E(chosen.baseline == null ? "—" : chosen.baseline)],
            ["Target", E(chosen.target == null ? "—" : chosen.target)],
            ["Measurement method", E(chosen.measurement_method || "—")],
            ["Financial value", E(chosen.financial_value || "—")],
            ["Cost", E(chosen.cost || "—")],
            ["Capabilities", E(chosen.capabilities || "—")],
            ["Preconditions", E(chosen.preconditions || "—")],
            ["Risks", E(chosen.risks || "—")],
            ["Operational area", E(chosen.operational_area || "—")],
            ["Status", B(chosen.status || "—", chosen.status === "tracking" || chosen.status === "Tracking" ? "green" : "amber")],
          ])}${source.readOnly ? "" : `<div class="toolbar">${button("Edit", "msp-benefit-edit", `data-id="${E(chosen.id)}"`, "small", "edit")}${button("Delete", "msp-delete", `data-kind="benefit" data-id="${E(chosen.id)}" data-version="${E(chosen.version)}"`, "small", "close")}${button("Add map edge", "msp-link-new", `data-id="${E(chosen.id)}"`, "small", "plus")}</div>`}`
        : note("Select a benefit profile, or create the first one."),
    )}</div></div>${panel(
      "Benefits map",
      `${table(
        ["Output", "Capability", "Operational outcome", "Benefit", "Strategic objective", ""],
        links.length
          ? links.map((l) => `<tr><td><strong>${E(l.output_ref || "—")}</strong></td><td>${E(l.capability_ref || "—")}</td><td>${E(l.operational_outcome || "—")}</td><td class="mono">${E(mspName(source, l.benefit_id))}</td><td>${E(l.strategic_objective || "—")}</td><td>${source.readOnly ? "" : button("Unlink", "msp-delete", `data-kind="link" data-id="${E(l.id)}" data-version="${E(l.version)}"`, "small", "close")}</td></tr>`)
          : [`<tr><td colspan="6">No benefit-map edge is recorded.</td></tr>`],
      )}${note("Read left to right: an output enables a capability, which changes an operational outcome, which realizes a benefit under a strategic objective.")}`,
      B(`${links.length} edges`),
    )}`;
  }
  function mspTranches(source) {
    const rows = list(source.tranches);
    const chosen = rows.find((t) => t.id === v.mspTranche) || rows[0] || {};
    const reviews = list(source.reviews).filter((r) => !chosen.id || r.tranche_id === chosen.id);
    return `${mspError()}<div class="v2-split"><div>${panel(
      "Tranches",
      `${table(
        ["#", "State", "Review at", "Baseline"],
        rows.length
          ? rows.map((t) => `<tr class="${t.id === chosen.id ? "selected" : ""}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="msp-tranche" data-id="${E(t.id)}">${E(t.number)}</button></td><td>${B(t.state || "planned", t.state === "reviewed" ? "green" : "amber")}</td><td>${E(t.review_at || "—")}</td><td>${E(t.baseline_ref || "—")}</td></tr>`)
          : [`<tr><td colspan="4">No tranche is recorded.</td></tr>`],
      )}${source.readOnly ? "" : `<div class="toolbar">${button("New tranche", "msp-tranche-new", "", "small", "plus")}</div>`}`,
      B(`${rows.length} tranches`),
    )}</div><div>${panel(
      "Review · tranche " + E(chosen.number == null ? "—" : chosen.number),
      chosen.id
        ? `${kv([
            ["Tranche", E(String(chosen.number))],
            ["State", B(chosen.state || "planned", chosen.state === "reviewed" ? "green" : "amber")],
            ["Entry criteria", E(chosen.entry_criteria || "—")],
            ["Exit criteria", E(chosen.exit_criteria || "—")],
            ["Baseline ref", E(chosen.baseline_ref || "—")],
            ["Review at", E(chosen.review_at || "—")],
          ])}${source.readOnly ? "" : `<div class="toolbar">${button("Edit", "msp-tranche-edit", `data-id="${E(chosen.id)}"`, "small", "edit")}${button("Record review", "msp-review", `data-id="${E(chosen.id)}"`, "small", "shield")}${button("Delete", "msp-delete", `data-kind="tranche" data-id="${E(chosen.id)}" data-version="${E(chosen.version)}"`, "small", "close")}</div>`}`
        : note("Select a tranche, or create the first one."),
    )}</div></div>${panel(
      "Recorded reviews",
      table(
        ["Tranche", "Decision", "Evidence", "At"],
        reviews.length
          ? reviews.map((r) => `<tr><td>${E(String(r.tranche_number == null ? short(r.tranche_id, 8) : r.tranche_number))}</td><td>${B(r.decision || "—", r.decision === "continue" ? "green" : r.decision === "stop" ? "bad" : "amber")}</td><td><small>${E(JSON.stringify(r.evidence || {}))}</small></td><td><small>${E(r.created_at || r.reviewed_at || "—")}</small></td></tr>`)
          : [`<tr><td colspan="4">No review is recorded. A review with no evidence is refused by the action.</td></tr>`],
      ),
    )}`;
  }
  function mspRealization(source) {
    const rows = list(source.transitions);
    const benefits = list(source.benefits);
    return `${mspError()}${panel(
      "Transition and measurement",
      `${table(
        ["Benefit", "Baseline measured", "Readiness", "Enacted", "Post measured", "Legacy retired", ""],
        rows.length
          ? rows.map((r) => `<tr><td class="mono">${E(mspName(source, r.benefit_id))}</td><td><small>${E(r.baseline_measured_at || "not measured")}</small></td><td><small>${E(typeof r.readiness === "object" ? JSON.stringify(r.readiness) : r.readiness || "—")}</small></td><td><small>${E(r.enacted_at || "—")}</small></td><td><small>${E(r.post_measured_at || "—")}</small></td><td><small>${E(r.retired_at || "—")}</small></td><td>${source.readOnly ? "" : button("Update", "msp-transition", `data-id="${E(r.benefit_id)}"`, "small", "edit")}</td></tr>`)
          : [`<tr><td colspan="7">No transition is recorded.</td></tr>`],
      )}${note("The baseline is measured before the transition; a benefit with no measured baseline cannot claim an improvement.")}`,
      source.readOnly ? "" : button("Record transition", "msp-transition-new", "", "small", "run"),
    )}${panel(
      "Profiles",
      table(["Reference", "Observable outcome", "Status"], benefits.length ? benefits.map((b) => `<tr><td class="mono">${E(b.reference || short(b.id, 8))}</td><td>${E(b.observable_outcome || "—")}</td><td>${B(b.status || "—", b.status === "tracking" || b.status === "Tracking" ? "green" : "amber")}</td></tr>`) : [`<tr><td colspan="3">No benefit profile is recorded.</td></tr>`]),
    )}`;
  }
  function mspStakeholders(source) {
    const rows = list(source.stakeholders);
    const chosen = rows.find((s) => s.id === v.mspStakeholder) || rows[0] || {};
    return `${mspError()}<div class="v2-split"><div>${panel(
      "Stakeholders",
      `${table(
        ["Name", "Importance", "Influence", "Support"],
        rows.length
          ? rows.map((s) => `<tr class="${s.id === chosen.id ? "selected" : ""}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="msp-stakeholder" data-id="${E(s.id)}">${E(s.name || short(s.id, 8))}</button></td><td>${E(s.importance || "—")}</td><td>${E(s.influence || "—")}</td><td>${B(s.support_level || "—", s.support_level === "supportive" ? "green" : "amber")}</td></tr>`)
          : [`<tr><td colspan="4">No stakeholder profile is recorded.</td></tr>`],
      )}${source.readOnly ? "" : `<div class="toolbar">${button("New stakeholder", "msp-stakeholder-new", "", "small", "plus")}</div>`}`,
      B(`${rows.length} profiles`),
    )}</div><div>${panel(
      "Profile · " + E(chosen.name || "none"),
      chosen.id
        ? `${kv([
            ["Name", E(chosen.name || "—")],
            ["Concerns", E(chosen.concerns || "—")],
            ["Sensitivities", E(chosen.sensitivities || "—")],
            ["Importance", E(chosen.importance || "—")],
            ["Influence", E(chosen.influence || "—")],
            ["Interest", E(chosen.interest || "—")],
            ["Exposure", E(chosen.exposure || "—")],
            ["Support level", E(chosen.support_level || "—")],
            ["Support trend", E(chosen.support_trend || "—")],
            ["Engagement target", E(chosen.engagement_target || "—")],
            ["Benefits", E(chosen.benefits || "—")],
            ["Disbenefits", E(chosen.disbenefits || "—")],
          ])}${source.readOnly ? "" : `<div class="toolbar">${button("Edit", "msp-stakeholder-edit", `data-id="${E(chosen.id)}"`, "small", "edit")}${button("Delete", "msp-delete", `data-kind="stakeholder" data-id="${E(chosen.id)}" data-version="${E(chosen.version)}"`, "small", "close")}</div>`}`
        : note("Select a stakeholder, or create the first one."),
    )}</div></div>`;
  }
  const MSP_TABS = ["Definition", "Dossier", "Benefits map & profiles", "Tranches & reviews", "Benefits realization", "Stakeholders"];
  function mspDefinitionBody(tab) {
    const source = mspSource();
    if (consoleOn() && !msp().loaded && !msp().error) {
      return note("Loading the programme store…");
    }
    return (
      tab === "Definition"
        ? mspDefinition(source)
        : tab === "Dossier"
          ? mspDossier(source)
          : tab === "Benefits map & profiles"
            ? mspBenefits(source)
            : tab === "Tranches & reviews"
              ? mspTranches(source)
              : tab === "Benefits realization"
                ? mspRealization(source)
                : mspStakeholders(source)
    );
  }
  function programmeDefinitionLive() {
    if (!msp().loaded && !msp().loading) consoleLoadMsp();
    const tab = MSP_TABS.includes(v.tab.programme) ? v.tab.programme : "Definition";
    return `<div class="page enterprise">${heading(
      "Programme definition",
      "The programme's mandate, definition artefacts, benefit profiles and map, tranches and reviews, realization and stakeholders — read from the programme store.",
      `${route("Portfolio", "portfolio", "", "layers")}${route("Plan & WBS", "plan", "", "board")}`,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon("The programme definition, benefits, tranches, realization and stakeholder profiles are the store's own rows.", true)}${tabs(MSP_TABS, tab, "programme")}${mspDefinitionBody(tab)}</div>`;
  }
  function programmeDefinitionSnapshot() {
    const tab = MSP_TABS.includes(v.tab.programme) ? v.tab.programme : "Definition";
    return `<div class="page enterprise">${heading(
      "Programme definition",
      "The programme's definition set, benefits, tranches, realization and stakeholders from the published snapshot.",
      `${route("Portfolio", "portfolio", "", "layers")}${route("Plan & WBS", "plan", "", "board")}`,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon("The published programme snapshot. Read-only: sign in at the app origin to operate the programme.", true)}${tabs(MSP_TABS, tab, "programme")}${mspDefinitionBody(tab)}</div>`;
  }

  // Signed in, every programme surface reads the tenant's own rows; on the public pack the
  // snapshot's real rows; on a clean checkout the design study's fixtures. The ribbon says
  // which one is in front of the reader.
  const consoleRibbon = (scope, active) =>
    active
      ? `<div class="v2-ribbon live" role="note"><strong>LIVE</strong><span>${scope}</span><span class="v2-src">${
          consoleOn() ? "API · no snapshot" : liveSrc
        }</span></div>`
      : ribbon(scope, false);
  const runTone = (status) =>
    status === "succeeded"
      ? "green"
      : status === "failed"
        ? "bad"
        : status === "waiting_approval"
          ? "amber"
          : "";
  const runLive = (status) => ["queued", "running", "waiting_approval"].includes(status);
  const runWhen = (run) => run.started_at || run.created_at || run.updated_at || "";
  const projectKeyOf = (id) => {
    const row = [...consoleProjects(), ...snapshotProjects()].find((p) => p.id === id);
    return row ? row.key : short(id || "", 8);
  };
  const cardOf = (id) => {
    for (const pid of Object.keys(cs.byProject || {})) {
      const found = list((cs.byProject[pid] || {}).cards).find((card) => card.id === id);
      if (found) return found;
    }
    return list(((live || {}).work || {}).cards).find((card) => card.id === id) || null;
  };
  // A link taken from a run output is data, not markup: only http(s) is a link. A
  // `javascript:` or `data:` value renders as text, because the console holds a session in
  // this origin and a click on an agent-authored value must never execute here.
  const safeUrl = (value) => {
    try {
      const url = new URL(String(value), window.location.origin);
      return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
    } catch (_) {
      return null;
    }
  };
  const outputCell = (output) => {
    if (output == null) return `<span class="muted">—</span>`;
    const value = typeof output === "object" ? output.output || output : output;
    const raw =
      (value && (value.pr_url || value.url || value.pull_request_url)) ||
      (typeof output === "object" && output ? output.pr_url : null);
    const url = raw ? safeUrl(raw) : null;
    if (url) {
      return `<a class="e-project-link" href="${E(url)}" target="_blank" rel="noopener">open ↗</a>`;
    }
    const text = typeof value === "string" ? value : JSON.stringify(value);
    return `<span class="mono">${E(short(text, 90))}</span>`;
  };

  async function consoleLoadOutputs(projectId) {
    if (!consoleOn() || !projectId || cs.outputsLoading === projectId) return;
    if (cs.outputs[projectId]) return; // already loaded; a refresh deletes the key first
    cs.outputsLoading = projectId;
    try {
      const page = await C.action("documents.list", {
        project_id: projectId,
        page: { limit: 100 },
      });
      cs.outputs[projectId] = {
        items: list(page.items).filter((doc) => doc.status !== "deleted"),
        error: null,
      };
    } catch (error) {
      cs.outputs[projectId] = { items: [], error };
    } finally {
      cs.outputsLoading = null;
      render();
    }
  }

  // The cross-project register. One `documents.list` page at a time: the operator sees the
  // tenant's outputs, and "Load more" follows the cursor instead of silently stopping at a cap.
  async function consoleLoadRegister(more = false) {
    if (!consoleOn() || cs.register.loading) return;
    if (cs.register.loaded && !more) return;
    cs.register.loading = true;
    try {
      const page = await C.action("documents.list", {
        kind: "document",
        page: {
          limit: 200,
          ...(more && cs.register.next_cursor ? { cursor: cs.register.next_cursor } : {}),
        },
      });
      const items = list(page.items).filter((doc) => doc.status !== "deleted");
      cs.register.items = more ? [...cs.register.items, ...items] : items;
      cs.register.next_cursor = page.next_cursor || null;
      cs.register.error = null;
      cs.register.loaded = true;
    } catch (error) {
      cs.register.error = error;
    } finally {
      cs.register.loading = false;
      render();
    }
  }

  // The admin's review queue: every draft whose author asked for publication. It is a
  // tenant-wide query, not the register's loaded page, so its count is honest at any size.
  async function consoleLoadRequests(more = false) {
    if (!consoleOn() || !isAdmin() || cs.requests.loading) return;
    if (cs.requests.loaded && !more) return;
    cs.requests.loading = true;
    try {
      const page = await C.action("documents.list", {
        publish_requested: true,
        page: {
          limit: 50,
          ...(more && cs.requests.next_cursor ? { cursor: cs.requests.next_cursor } : {}),
        },
      });
      const items = list(page.items).filter((doc) => doc.status !== "deleted");
      cs.requests.items = more ? [...cs.requests.items, ...items] : items;
      cs.requests.next_cursor = page.next_cursor || null;
      cs.requests.error = null;
      cs.requests.loaded = true;
    } catch (error) {
      cs.requests.error = error;
    } finally {
      cs.requests.loading = false;
      render();
    }
  }

  // After a write the register and the owning project's panel both reload. A single helper keeps
  // the two caches from disagreeing about the row that just changed.
  function consoleRefreshOutputs(projectId) {
    cs.register.loaded = false;
    cs.register.items = [];
    cs.register.next_cursor = null;
    cs.requests = { items: [], loaded: false, loading: false, error: null, next_cursor: null };
    if (projectId) delete cs.outputs[projectId];
    if (!consoleOn()) return;
    if (projectId) consoleLoadOutputs(projectId);
    consoleLoadRegister();
  }

  /* ------------------------------------------------- architecture capability (ADM) */
  // The screen's four tabs read the tenant's own aggregates: the tailored profile with its
  // standard, the artefacts, the contracts, the board and the two assessment baselines. Each
  // read settles on its own, so one refused call leaves the rest of the screen standing and
  // names the call that failed instead of blanking the page.
  async function consoleLoadAdm() {
    if (!consoleOn() || cs.adm.loading || cs.adm.loaded) return;
    cs.adm.loading = true;
    const reads = [
      ["profile", "architecture.profile_get", {}],
      ["artifacts", "architecture.artifacts_list", {}],
      ["contracts", "architecture.contracts_list", {}],
      ["boards", "architecture.board_list", { kind: "architecture_board" }],
      ["maturity", "architecture.maturity_list", {}],
      ["skills", "architecture.skills_list", {}],
    ];
    const results = await Promise.allSettled(
      reads.map(([, name, input]) => C.action(name, input)),
    );
    results.forEach((result, index) => {
      const key = reads[index][0];
      const error = result.status === "rejected" ? result.reason : null;
      const value = result.status === "fulfilled" ? result.value : null;
      // `profile_get` answers the profile and its standard together; each keeps its own state
      // so a panel can name exactly the read that refused.
      if (key === "profile") {
        cs.adm.profile = {
          data: error ? null : (value && value.profile) || null,
          loaded: !error,
          loading: false,
          error,
        };
        cs.adm.standard = {
          items: error ? [] : list(value && value.standard),
          loaded: !error,
          loading: false,
          error,
        };
        return;
      }
      cs.adm[key] = {
        items: error ? [] : list(value && value.items),
        loaded: !error,
        loading: false,
        error,
      };
    });
    cs.adm.loaded = true;
    cs.adm.loading = false;
    render();
  }

  // The inspector's rows and edges, one artefact at a time. Both reads are cached together;
  // a failed read keeps its error so the panel says which call refused.
  async function consoleLoadAdmEntries(artifactId) {
    if (!consoleOn() || !artifactId || cs.adm.entries[artifactId]) return;
    cs.adm.entries[artifactId] = { items: [], loaded: false, loading: true, error: null };
    const [entries, links] = await Promise.allSettled([
      C.action("architecture.artifacts_entries", { id: artifactId }),
      C.action("architecture.artifacts_links", { id: artifactId }),
    ]);
    const entriesError = entries.status === "rejected" ? entries.reason : null;
    cs.adm.entries[artifactId] = {
      items: entriesError ? [] : list(entries.value && entries.value.items),
      loaded: !entriesError,
      loading: false,
      error: entriesError,
    };
    const linksError = links.status === "rejected" ? links.reason : null;
    cs.adm.links[artifactId] = {
      items: linksError ? [] : list(links.value && links.value.items),
      loaded: !linksError,
      loading: false,
      error: linksError,
    };
    render();
  }

  /* ------------------------------------------------------------- execution room */
  // The runs API already returns everything the room needs: `runs.list` filters by status and
  // card, `runs.get` carries the steps, the gates, the process and the inputs. No control below
  // is simulated; each one is the action it names.
  async function consoleLoadRuns() {
    if (!consoleOn() || cs.runsLoading) return;
    cs.runsLoading = true;
    try {
      const page = await C.action("runs.list", { page: { limit: 200 } });
      cs.runs = list(page.items);
      cs.runsLoaded = true;
    } catch (error) {
      cs.runs = [];
      cs.runsLoaded = true;
      cs.lastError = error;
    } finally {
      cs.runsLoading = false;
      render();
    }
  }

  async function consoleRunOpen(id) {
    if (!consoleOn() || !id) return;
    v.runId = id;
    if (cs.runDetail[id] && !cs.runDetail[id].error) {
      render();
      return;
    }
    cs.runDetailLoading = id;
    render();
    try {
      cs.runDetail[id] = await C.action("runs.get", { id });
    } catch (error) {
      cs.runDetail[id] = { error };
    } finally {
      cs.runDetailLoading = null;
      render();
    }
  }

  function consoleRunCancel(id) {
    const body = `<div class="dialog-body console-dialog-body"><p class="muted">Cancelling run <span class="mono">${E(short(id, 12))}</span> voids its pending gates. Its rows survive.</p><label class="field full"><span>Reason</span><textarea id="run-cancel-reason" rows="3" placeholder="Why this run is cancelled"></textarea></label></div>`;
    openDialog(
      "Cancel run",
      body,
      `${button("Cancel run", "run-cancel-save", `data-id="${E(id)}"`, "primary", "replay")}${btn("Close", "close-dialog")}`,
      true,
    );
  }

  async function consoleRunCancelSave(id) {
    const reason = (document.getElementById("run-cancel-reason") || {}).value || "";
    try {
      await C.action("runs.cancel", { id, reason: reason.trim() || null });
      closeWorkspaceDialog();
      delete cs.runDetail[id];
      toast("Run cancelled");
      if (v.runId === id) v.runId = null;
      consoleLoadRuns();
    } catch (error) {
      cs.lastError = error;
      toast("Cancel refused · " + consoleError(error));
      render();
    }
  }

  function consoleApprovalDecide(approvalId, decision) {
    const label = decision === "approve" ? "Approve gate" : "Reject gate";
    const body = `<div class="dialog-body console-dialog-body"><p class="muted">The decision is recorded against the gate; the same approver cannot decide it twice.</p><label class="field full"><span>Note</span><textarea id="approval-note" rows="3" placeholder="Why you decided this"></textarea></label></div>`;
    openDialog(
      label,
      body,
      `${button(label, "approval-save", `data-id="${E(approvalId)}" data-decision="${E(decision)}"`, "primary", "shield")}${btn("Close", "close-dialog")}`,
      true,
    );
  }

  async function consoleApprovalSave(approvalId, decision) {
    const note = (document.getElementById("approval-note") || {}).value || "";
    try {
      await C.action("approvals.decide", {
        approval_id: approvalId,
        decision,
        note: note.trim() || null,
      });
      closeWorkspaceDialog();
      toast(decision === "approve" ? "Gate approved" : "Gate rejected");
      if (v.runId) delete cs.runDetail[v.runId];
      consoleLoadRuns();
      if (v.runId) consoleRunOpen(v.runId);
    } catch (error) {
      cs.lastError = error;
      toast("Decision refused · " + consoleError(error));
      render();
    }
  }

  async function consoleStartRun(cardId, processKey) {
    if (!processKey) {
      toast("This card has no process binding");
      return;
    }
    try {
      const run = await C.action("process_runs.start", {
        card_id: cardId,
        process_key: processKey,
        inputs: {},
      });
      cs.lastError = null;
      toast(`Run ${short(run.id, 8)} · ${run.status || "queued"}`);
      v.runId = run.id;
      v.tab.execution = "All";
      navigate("execution");
      consoleLoadRuns();
      consoleRunOpen(run.id);
    } catch (error) {
      cs.lastError = error;
      toast("Start refused · " + consoleError(error));
      render();
    }
  }

  function projectsPage() {
    const signed = consoleOn();
    const canSignIn = !!(C && C.config());
    if (signed && !cs.loaded && !cs.loading && !cs.error) consoleLoad();
    const rows = signed ? consoleProjects() : snapshotProjects();
    const actions = signed
      ? C.can("projects.create")
        ? button("New project", "project-new", "", "small", "plus")
        : ""
      : canSignIn
        ? button("Sign in to operate", "console-signin", "", "primary", "shield")
        : "";
    if (!rows.length) {
      return `<div class="page enterprise">${heading("Projects", "The programme's projects; live when signed in, read-only from the published snapshot on the public site.", actions, "PORTFOLIO & PLAN")}${panel(
        "Projects",
        signed
          ? cs.error
            ? note(`<strong>Read refused</strong> · ${E(consoleError(cs.error))}`, "bad")
            : note("Loading the tenant's projects…")
          : note("No published snapshot is loaded. Sign in to operate the live programme."),
      )}</div>`;
    }
    // A deep link (`#projects/<id>` or `#projects/<id>/wp/<cardId>`) wins over the in-page picker;
    // an id that is not in this tenant falls back rather than rendering an empty page.
    const current =
      rows.find((row) => row.id === routeUuid(1)) ||
      rows.find((row) => row.id === v.projectPage) ||
      rows.find((row) => /^S/.test(String(row.key))) ||
      rows[0];
    if (signed) {
      consoleLoadProject(current.id);
      consoleLoadOutputs(current.id);
    }
    // Signed out, the picker selects a project from the published snapshot; the move handler is
    // the same one the console uses, and its loaders already no-op without a session.
    const select = `<label class="e-select">Project <select data-v2-input="project-page">${rows
      .map(
        (row) =>
          `<option value="${E(row.id)}" ${row.id === current.id ? "selected" : ""}>${E(row.key)} · ${E(row.name)}</option>`,
      )
      .join("")}</select></label>`;
    const plan = planHref((current.meta || {}).plan);
    const repo = current.repo_url
      ? `<a class="e-project-link" href="${E(current.repo_url)}" target="_blank" rel="noopener">repository ↗</a>`
      : "—";
    // #433: the programme is downloadable. `download` keeps the reader on the page; the two files
    // are the same rows this panel renders, so a screenshot can be checked against a file.
    const downloads =
      '<a class="e-project-link" href="data/programme.json" download>programme.json ↓</a> · ' +
      '<a class="e-project-link" href="data/programme.csv" download>programme.csv ↓</a>';
    const packStatus = (current.meta || {}).measured_status || "";
    const head = panel(
      `${E(current.key)} · ${E(current.name)}`,
      `${select}${
        current.description ? `<p class="muted">${E(current.description)}</p>` : ""
      }${kv([
        [
          "Status",
          B(
            current.status || "draft",
            current.status === "active"
              ? "green"
              : current.status === "closed" || current.status === "archived"
                ? ""
                : "amber",
          ),
        ],
        [
          "Pack coverage",
          packStatus
            ? B(packStatus, packStatus === "built" || packStatus === "standing" ? "green" : "amber")
            : `<span class="muted">—</span>`,
        ],
        ["Repository", repo],
        ["Plan", plan ? `<a class="e-project-link" href="${E(plan)}">${E((current.meta || {}).plan)} ↗</a>` : `<span class="muted">—</span>`],
        ["Linked issues", `${projectIssues(current).length}`],
        ["Download", downloads],
      ])}${
        signed
          ? `<div class="console-actions">${C.can("projects.update") ? button("Edit project", "project-edit", `data-id="${E(current.id)}"`, "small", "edit") : ""}${C.can("projects.update") && (current.status === "draft" || current.status === "paused") ? button("Activate", "project-activate", `data-id="${E(current.id)}"`, "small") : ""}${C.can("projects.update") && current.status === "active" ? button("Pause", "project-pause", `data-id="${E(current.id)}"`, "small") : ""}${C.can("projects.update") && current.status === "closed" ? button("Archive", "project-archive", `data-id="${E(current.id)}"`, "small") : ""}${C.can("projects.close") ? button("Close project", "project-close", `data-id="${E(current.id)}"`, "small", "archive") : ""}</div>`
          : ""
      }`,
      signed ? `<span class="console-live">live</span>` : "",
    );
    const detail = signed ? projectWorkspace(current) : projectReadOnly(current, rows);
    return `<div class="page enterprise">${heading(
      "Projects",
      "The programme's projects — each with its board, plan, issues, outputs and publish state. Live when signed in; a public snapshot otherwise.",
      actions,
      "PORTFOLIO & PLAN",
    )}${head}${detail}</div>`;
  }

  /* ------------------------------------------------------------- outputs */
  // `seed/output-standard.json` is the source; the builder writes `data/output-standard.js` for
  // this file. The promoted columns are authoritative; `meta` is the pre-0042 fallback.
  const OUTPUT_STANDARD = window.CONVEYOR_OUTPUT_STANDARD || { categories: [], aliases: {} };
  const canonicalCategory = (value) => {
    const raw = String(value || "");
    return (OUTPUT_STANDARD.aliases || {})[raw] || raw;
  };
  const outputCategoryOf = (doc) =>
    canonicalCategory(doc.category || ((doc.meta || {}).category) || "");
  const outputUnitOf = (doc) => String(doc.unit || ((doc.meta || {}).unit) || "");
  const outputAuthorOf = (doc) => String(doc.created_by_name || "");
  const outputStateOf = (doc) =>
    doc.publishable
      ? "public"
      : (doc.meta || {}).rework
        ? "returned"
        : doc.publish_requested_at
          ? "review requested"
          : "draft";
  const isAdmin = () => !!(C && C.role && C.role() === "admin");
  const INCEPTION_CATEGORIES = ["inception-analysis", "architecture-design"];
  const OUTPUT_CATEGORIES = list(OUTPUT_STANDARD.categories);

  // The standard's template per category is data (#245); the compose dialog seeds its skeleton
  // and the completeness panel reads the same required headings the server refuses to publish
  // without. The normalisation mirrors the domain function exactly.
  const outputTemplateOf = (category) =>
    (OUTPUT_STANDARD.templates || {})[category] || null;
  const headingKeyOf = (value) =>
    String(value).split(/\s+/).filter(Boolean).join(" ").toLowerCase();
  const headingsOf = (markdown) => {
    const found = new Set();
    let fence = null;
    for (const line of String(markdown || "").split("\n")) {
      const trimmed = line.replace(/^\s+/, "");
      const opener = trimmed.match(/^(```+|~~~+)/);
      if (opener) {
        const marker = opener[1][0];
        if (!fence) fence = marker;
        else if (fence === marker) fence = null;
        continue;
      }
      if (fence) continue;
      const atx = trimmed.match(/^(#{1,6})\s+(.*?)\s*$/);
      if (!atx) continue;
      const text = atx[2].replace(/#+$/, "").trim();
      if (text) found.add(headingKeyOf(text));
    }
    return found;
  };
  const missingSectionsOf = (category, body) => {
    const template = outputTemplateOf(category);
    if (!template) return [];
    const present = headingsOf(body);
    return list(template.required).filter((heading) => !present.has(headingKeyOf(heading)));
  };
  const completenessOf = (category, body) => {
    const template = outputTemplateOf(category);
    if (!template) return `<p class="muted">This category has no template in the standard.</p>`;
    const present = headingsOf(body);
    return `<ul class="console-completeness">${list(template.required)
      .map((heading) =>
        present.has(headingKeyOf(heading))
          ? `<li class="is-present">✓ ${E(heading)}</li>`
          : `<li class="is-missing">○ ${E(heading)}</li>`,
      )
      .join("")}</ul>`;
  };
  const findOutput = (id) =>
    [...list(cs.requests.items), ...list(cs.register.items)].find((doc) => doc.id === id) ||
    null;
  const missingFromError = (error) =>
    list(error && error.errors)
      .filter((row) => row.code === "output_incomplete")
      .map((row) => String(row.detail || "").replace(/^missing required section:\s*/, ""))
      .filter(Boolean);

  // The completeness panel is live on the dialog's own fields; it is not a `render()` state
  // change, so typing never redraws (and never loses) the draft.
  function wireCompleteness() {
    const area = document.getElementById("output-body");
    const slot = document.getElementById("output-completeness");
    if (!area || !slot) return;
    const category = document.getElementById("output-category");
    const update = () =>
      (slot.innerHTML = completenessOf((category || {}).value || "", area.value));
    area.addEventListener("input", update);
    if (category) category.addEventListener("change", update);
    update();
  }

  // The classes a person may create a project against. The keyed classes (P0, SPEC-V2) are the
  // programme's own rows, and `archived` is a measurement, not a creation state. #221 adds
  // classes to the same generated standard; this list follows it without a code change.
  const OUTPUT_CLASSES = list(OUTPUT_STANDARD.classes).filter(
    (cls) => !(cls.when || {}).key && cls.name !== "archived",
  );
  const classLabel = (cls) =>
    `${cls.name} · ${list(cls.now).join(", ")}${
      list(cls.closure).length ? ` · closure ${list(cls.closure).join(", ")}` : ""
    }`;
  // Resolve a project's class exactly as the seed builder does: every `when` fact must match,
  // first match wins, and a project that matches nothing is custom rather than guessed.
  const classOfProject = (project) => {
    const meta = project.meta || {};
    const facts = {
      key: project.key,
      track: meta.track,
      status: project.status,
      measured_status: meta.measured_status,
    };
    return (
      list(OUTPUT_STANDARD.classes).find((cls) =>
        Object.entries(cls.when || {}).every(([name, value]) => facts[name] === value),
      ) || null
    );
  };
  // A class whose `when` omits a fact still needs a representative value, or the first-match
  // resolver would fall through to the earlier class (`v2-built` is any v2 not `proposed`).
  const CLASS_STATUS = {
    "v2-proposed": "proposed",
    "v2-built": "built",
    "v1-active": "partial",
    "v1-built": "built",
    programme: "planned",
  };
  // The meta a class writes: its facts and its due-now / at-closure sets. A class with no
  // `track`/`measured_status` fact leaves the project's current value alone.
  const classMeta = (cls, tranche) => {
    const meta = {
      expected_outputs: list(cls.now),
      closure_outputs: list(cls.closure),
      output_class: cls.name,
    };
    if ((cls.when || {}).track) meta.track = cls.when.track;
    const status = (cls.when || {}).measured_status || CLASS_STATUS[cls.name];
    if (status) meta.measured_status = status;
    const number = Number(tranche);
    if (String(tranche ?? "").trim() !== "" && Number.isFinite(number)) meta.tranche = number;
    return meta;
  };

  // The admin's review queue. Publishing is the same action as the register's Publish button;
  // there is no approve/deny action, so the view surfaces exactly the draft that needs one.
  function reviewRequestsPanel() {
    if (!isAdmin()) return "";
    consoleLoadRequests();
    const state = cs.requests;
    if (state.error) {
      return panel(
        "Review requests",
        note(`<strong>Read refused</strong> · ${E(consoleError(state.error))}`, "bad"),
      );
    }
    if (!state.loaded) return panel("Review requests", note("Loading the publish requests…"));
    if (!state.items.length) {
      return panel("Review requests", note("No draft is waiting for review."), B("0", ""));
    }
    const rows = state.items.map((doc) => {
      const actions = [
        C.can("documents.get")
          ? button("Read", "request-read", `data-id="${E(doc.id)}"`, "small", "file")
          : "",
        C.can("documents.publish")
          ? button(
              "Publish",
              "request-publish",
              `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
              "small",
              "shield",
            )
          : "",
        C.can("documents.return")
          ? button(
              "Return",
              "request-return",
              `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
              "small",
              "replay",
            )
          : "",
        C.can("documents.compose")
          ? button(
              "Open a kata",
              "request-kata",
              `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
              "small",
              "lightning",
            )
          : "",
      ].join("");
      const project = consoleProjects().find((row) => row.id === doc.project_id);
      return `<tr><td><strong>${E(doc.title)}</strong><small class="mono">v${E(doc.version)}</small></td><td>${E(doc.project_key || (project || {}).key || "—")}</td><td>${E(outputAuthorOf(doc) || "—")}</td><td><small>${E(short(doc.updated_at, 16))}</small></td><td><div class="console-actions">${actions}</div></td></tr>`;
    });
    return panel(
      "Review requests",
      `${table(["Output", "Project", "Author", "Updated", "Operate"], rows)}${note("A request is a draft whose author asked for publication. Publishing is an admin decision; there is no separate approve action.")}`,
      B(`${state.items.length} waiting`, "amber"),
    );
  }

  function outputsPage() {
    const signed = consoleOn();
    if (signed) {
      if (!cs.loaded && !cs.loading && !cs.error) consoleLoad();
      consoleLoadRegister();
    }
    const pack = window.CONVEYOR_OUTPUTS || {};
    const all = signed
      ? cs.register.items
      : list(pack.outputs).map((row) => ({ ...row, publishable: true }));
    const projectFilter = v.outputsProject || "";
    const stateFilter = v.outputsState || "";
    const categoryFilter = v.outputsCategory || "";
    const unitFilter = v.outputsUnit || "";
    const categories = [...new Set(all.map(outputCategoryOf).filter(Boolean))].sort();
    const units = [...new Set(all.map(outputUnitOf).filter(Boolean))].sort();
    const rows = all.filter((doc) => {
      if (projectFilter && doc.project_id !== projectFilter) return false;
      if (stateFilter && outputStateOf(doc) !== stateFilter) return false;
      if (categoryFilter && outputCategoryOf(doc) !== categoryFilter) return false;
      if (unitFilter && outputUnitOf(doc) !== unitFilter) return false;
      return true;
    });
    const projectName = (doc) => {
      const project = consoleProjects().find((row) => row.id === doc.project_id);
      return project
        ? `${project.key} · ${project.name}`
        : doc.project_key || doc.project_name || "—";
    };
    const pageLink = (doc) =>
      doc.publishable
        ? ` <a class="e-project-link" href="docs/output-${E(doc.id)}.html" target="_blank" rel="noopener">public ↗</a>`
        : "";
    const filters = signed
      ? `<div class="toolbar">
          <label class="e-select">Project <select data-v2-input="outputs-filter" data-key="outputsProject"><option value="">Every project</option>${consoleProjects()
            .map(
              (p) =>
                `<option value="${E(p.id)}" ${projectFilter === p.id ? "selected" : ""}>${E(p.key)} · ${E(p.name)}</option>`,
            )
            .join("")}</select></label>
          <label class="e-select">State <select data-v2-input="outputs-filter" data-key="outputsState"><option value="">Any</option><option value="public" ${stateFilter === "public" ? "selected" : ""}>Published</option><option value="review requested" ${stateFilter === "review requested" ? "selected" : ""}>Review requested</option><option value="draft" ${stateFilter === "draft" ? "selected" : ""}>Draft</option><option value="returned" ${stateFilter === "returned" ? "selected" : ""}>Returned</option></select></label>
          <label class="e-select">Category <select data-v2-input="outputs-filter" data-key="outputsCategory"><option value="">Any</option>${categories
            .map(
              (c) =>
                `<option value="${E(c)}" ${categoryFilter === c ? "selected" : ""}>${E(c)}</option>`,
            )
            .join("")}</select></label>
          <label class="e-select">Unit <select data-v2-input="outputs-filter" data-key="outputsUnit"><option value="">Any</option>${units
            .map(
              (u) =>
                `<option value="${E(u)}" ${unitFilter === u ? "selected" : ""}>${E(u)}</option>`,
            )
            .join("")}</select></label>
          <span class="mono muted">${rows.length} of ${all.length} loaded</span>
        </div>`
      : "";
    const signedRows = rows.map((doc) => {
      const actions = [
        C.can("documents.get")
          ? button("Read", "output-read", `data-id="${E(doc.id)}"`, "small", "file")
          : "",
        C.can("documents.update")
          ? button("Edit", "output-edit", `data-id="${E(doc.id)}"`, "small", "edit")
          : "",
        C.can("documents.revision_list")
          ? button(
              "History",
              "output-history",
              `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
              "small",
              "file",
            )
          : "",
        C.can("documents.create")
          ? button("Attach", "output-attach", `data-id="${E(doc.id)}"`, "small", "plus")
          : "",
        !doc.publishable && C.can("documents.request_publish") && !doc.publish_requested_at
          ? button(
              "Request publish",
              "output-request-publish",
              `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
              "small",
              "shield",
            )
          : "",
        isAdmin() && C.can("documents.publish")
          ? button(
              doc.publishable ? "Unpublish" : "Publish",
              "output-publish",
              `data-id="${E(doc.id)}" data-public="${doc.publishable ? "1" : "0"}" data-version="${E(doc.version)}"`,
              "small",
              "shield",
            )
          : "",
        C.can("documents.download")
          ? button("Download", "output-download", `data-id="${E(doc.id)}"`, "small", "download")
          : "",
        C.can("documents.delete")
          ? button(
              "Delete",
              "output-delete",
              `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
              "small",
              "archive",
            )
          : "",
      ].join("");
      const state = outputStateOf(doc);
      return `<tr><td><strong>${E(doc.title)}</strong><small class="mono">v${E(doc.version)}</small></td><td>${E(projectName(doc))}</td><td>${E(outputUnitOf(doc) || "—")}</td><td>${E(outputCategoryOf(doc) || "—")}</td><td>${E(outputAuthorOf(doc) || "—")}</td><td>${B(state, doc.publishable ? "green" : "")}${pageLink(doc)}</td><td><div class="console-actions">${actions}</div></td></tr>`;
    });
    const publicRows = rows.map(
      (doc) =>
        `<tr><td><a class="e-project-link" href="${E(doc.path || `docs/output-${doc.id}.html`)}" target="_blank" rel="noopener"><strong>${E(doc.title)}</strong></a></td><td>${E(doc.project_key || doc.project_name || "—")}</td><td><small>${E(short(doc.updated_at, 16))}</small></td></tr>`,
    );
    const body = signed
      ? table(
          ["Output", "Project", "Unit", "Category", "Author", "State", "Operate"],
          signedRows,
        )
      : table(["Output", "Project", "Updated"], publicRows);
    const more =
      signed && cs.register.next_cursor
        ? `<div class="row" style="margin-top:10px">${button("Load more", "outputs-more", "", "small", "arrow")}</div>`
        : "";
    const compose = signed && C.can("documents.compose")
      ? button("Compose output", "output-new-register", "", "small", "plus")
      : "";
    return `<div class="page enterprise">${heading(
      "Outputs",
      "Every programme output in one place — composed, edited, published and deleted against the project and the unit it belongs to.",
      compose,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon(
      signed
        ? "Documents read from the platform's own rows. Publishing is the output's own publishable flag; the public page follows on the next request."
        : "Published outputs only, read from the live outputs.js index.",
      signed ? true : list(pack.outputs).length > 0,
    )}${filters}${signed ? reviewRequestsPanel() : ""}${
      signed && cs.register.error
        ? note(`<strong>Read refused</strong> · ${E(consoleError(cs.register.error))}`, "bad")
        : ""
    }${panel(
      "Outputs",
      (signed ? signedRows : publicRows).length
        ? `${body}${more}`
        : `<p class="muted">${
            signed
              ? "No output matches these filters. Compose one, or clear a filter."
              : "No output is published yet."
          }</p>`,
      B(`${rows.length}`, rows.length ? "green" : ""),
    )}</div>`;
  }

  function projectReadOnly(current, rows) {
    const body = rows
      .map(
        (row) =>
          `<tr><td><button type="button" class="console-card-link" data-v2="project-select" data-id="${E(row.id)}"><strong>${E(row.key)}</strong> ${E(row.name)}</button></td><td>${B(
            row.status || "draft",
            row.status === "active" ? "green" : row.status === "closed" || row.status === "archived" ? "" : "amber",
          )}</td><td><small>${E(((row.meta || {}).track) || "")}</small></td><td>${
            planHref((row.meta || {}).plan)
              ? `<a class="e-project-link" href="${E(planHref((row.meta || {}).plan))}">plan ↗</a>`
              : "—"
          }</td></tr>`,
      )
      .join("");
    return `${deliveryBoard(current)}${publicExpectedOutputPanel(current)}${panel(
      "All projects",
      `${table(["Project", "Status", "Track", "Plan"], [body])}${note(
        "This is the published snapshot, read-only. Sign in at the app origin to edit a project, compose outputs or publish.",
      )}`,
      B(`${rows.length} projects`, "green"),
    )}`;
  }

  function coverageOf(key) {
    const cov = window.CONVEYOR_COVERAGE || {};
    return list(cov.projects).find((row) => row.key === key) || null;
  }

  function coveragePanel(project) {
    const row = coverageOf(project.key);
    if (!row) {
      return panel(
        "Programme truth",
        note("This project has no row in the measured programme (`data/coverage.json`)."),
      );
    }
    const units = row.units || {};
    const deps = list(row.blocked_by);
    const plan = planHref((project.meta || {}).plan || row.plan);
    const evidence = list(row.evidence);
    const tone =
      row.status === "built"
        ? "green"
        : row.status === "partial"
          ? "amber"
          : row.status === "proposed"
            ? "purple"
            : "";
    return panel(
      "Programme truth",
      `${kv([
        ["Tranche", E(row.tranche ?? "—")],
        ["Measured", B(row.status || "unknown", tone)],
        [
          "Blocked by",
          deps.length
            ? deps.map((dep) => `<span class="mono">${E(dep)}</span>`).join(" · ")
            : "—",
        ],
        [
          "Plan",
          plan
            ? `<a class="e-project-link" href="${E(plan)}">${E((project.meta || {}).plan || row.plan)} ↗</a>`
            : "—",
        ],
        [
          "Units",
          `<small>${E(units.open ?? 0)} open · ${E(units.claimed ?? 0)} claimed · ${E(units.blocked ?? 0)} blocked · ${E((units.numbers || []).length)} linked</small>`,
        ],
      ])}${
        evidence.length
          ? `<p class="muted">Evidence: ${evidence.map((path) => `<span class="mono">${E(path)}</span>`).join(" · ")}</p>`
          : ""
      }${row.gap ? note(`<strong>Gap</strong> · ${E(row.gap)}`, "amber") : ""}`,
      B(`T${E(row.tranche ?? "?")}`, tone || "purple"),
    );
  }

  // The public checklist: published outputs only; a private draft never counts or renders here.
  function publicExpectedOutputPanel(project) {
    const title = "Expected outputs";
    const outputs = list((window.CONVEYOR_OUTPUTS || {}).outputs).filter(
      (doc) => doc.project_id === project.id,
    );
    const expected = list((project.meta || {}).expected_outputs);
    const closure = list((project.meta || {}).closure_outputs);
    const categories = expected.length ? expected : INCEPTION_CATEGORIES;
    const stateOf = (category) => {
      const docs = outputs.filter((doc) => outputCategoryOf(doc) === category);
      return {
        docs,
        label: docs.length ? "published" : "missing",
        tone: docs.length ? "green" : "bad",
      };
    };
    let published = 0;
    let nextMarked = false;
    const row = (category, markNext) => {
      const current = stateOf(category);
      if (current.docs.length) published += 1;
      const next = markNext && !current.docs.length && !nextMarked;
      if (next) nextMarked = true;
      return `<tr><td><strong>${E(category)}</strong>${next ? ` ${B("next", "amber")}` : ""}</td><td>${B(current.label, current.tone)}</td><td>${current.docs.length ? E(current.docs.map((doc) => doc.title).join(" · ")) : "—"}</td><td>—</td></tr>`;
    };
    const rows = categories.map((category) => row(category, true));
    if (project.key === "P0") {
      const delivery = snapshotProjects().filter(
        (row) => row.key !== "P0" && list((row.meta || {}).expected_outputs).length > 0,
      );
      const briefed = new Set(
        list((window.CONVEYOR_OUTPUTS || {}).outputs)
          .filter((doc) => outputCategoryOf(doc) === "project-brief")
          .map((doc) => doc.project_id),
      );
      const covered = delivery.filter((row) => briefed.has(row.id)).length;
      rows.push(
        `<tr><td><strong>project briefs</strong></td><td>${B(`${covered}/${delivery.length} published`, covered === delivery.length && delivery.length ? "green" : "amber")}</td><td><small>Across every delivery project</small></td><td>—</td></tr>`,
      );
    }
    let closurePublished = 0;
    const closureRows = closure.map((category) => {
      const current = stateOf(category);
      if (current.docs.length) closurePublished += 1;
      return `<tr><td><strong>${E(category)}</strong></td><td>${B(current.label, current.tone)}</td><td>${current.docs.length ? E(current.docs.map((doc) => doc.title).join(" · ")) : "—"}</td><td>—</td></tr>`;
    });
    const closureBlock = closure.length
      ? `<h4>At closure</h4>${table(["Output", "State", "Document", "Action"], closureRows)}`
      : "";
    return panel(
      title,
      `${table(["Output", "State", "Document", "Action"], rows)}${closureBlock}${note("Published outputs only. A private draft is not part of the public window.")}`,
      B(
        `${published}/${categories.length} published${closure.length ? ` · closure ${closurePublished}/${closure.length}` : ""}`,
        published === categories.length ? "green" : "amber",
      ),
    );
  }

  // The programme work queue: published outputs against the standard's due-now set, ordered by
  // tranche. The signed-in caller passes every document; the public one passes `outputs.js`.
  function programmeQueue(projects, docs) {
    const rows = projects
      .filter((project) => list((project.meta || {}).expected_outputs).length > 0)
      .map((project) => {
        const expected = list((project.meta || {}).expected_outputs);
        const closure = list((project.meta || {}).closure_outputs);
        const mine = docs.filter((doc) => doc.project_id === project.id);
        const isPublished = (category) =>
          mine.some(
            (doc) =>
              outputCategoryOf(doc) === category &&
              (doc.publishable || doc.publishable === undefined),
          );
        return {
          project,
          meta: project.meta || {},
          expected,
          closure,
          published: expected.filter(isPublished).length,
          closurePublished: closure.filter(isPublished).length,
          // #386: the at-closure set is owed only once the project is closed. An active project's
          // missing at-closure output is not debt; a closed project's is (the closure gate in
          // crates/actions/src/programme.rs is the action-side mirror of this rule).
          closureMissing:
            project.status === "closed" ? closure.filter((category) => !isPublished(category)) : [],
          next: expected.find((category) => !isPublished(category)) || "",
        };
      })
      .sort(
        (left, right) =>
          (left.meta.tranche ?? 99) - (right.meta.tranche ?? 99) ||
          String(left.project.key).localeCompare(String(right.project.key)),
      );
    const body = rows.map(
      ({ project, meta, expected, closure, published, closurePublished, closureMissing, next }) => {
        const measured = ((coverageOf(project.key) || {}).status) || meta.measured_status || "—";
        // #386: a closed project's missing at-closure category is named in the row and tones the
        // cell bad. An active project keeps the plain count — its closure set is not owed yet.
        const closureCell = closure.length
          ? `${B(`${closurePublished}/${closure.length}`, closureMissing.length ? "bad" : closurePublished === closure.length ? "green" : "amber")}${closureMissing.length ? `<small> missing: ${E(closureMissing.join(", "))}</small>` : ""}`
          : "—";
        return `<tr><td><strong>${E(project.key)}</strong> <small>${E(project.name)}</small></td><td>${E(meta.tranche ?? "—")}</td><td>${E(measured)}</td><td>${B(`${published}/${expected.length}`, expected.length && published === expected.length ? "green" : "amber")}</td><td>${closureCell}</td><td>${next ? `<span class="mono">${E(next)}</span>` : "—"}</td></tr>`;
      },
    );
    const due = rows.filter((row) => row.next.length > 0).length;
    const closureOwed = rows.reduce((total, row) => total + row.closureMissing.length, 0);
    const queueBadge = closureOwed
      ? `${due} outputs due · ${closureOwed} closure ${closureOwed === 1 ? "output" : "outputs"} owed`
      : `${due} outputs due`;
    return panel(
      "Programme queue",
      `${table(
        ["Project", "Tranche", "Measured", "Outputs", "Closure", "Next output"],
        body.length ? body : [`<tr><td colspan="6">No projects.</td></tr>`],
      )}${note("Published outputs against the due-now set; `closure` counts the at-closure set. A closed project's unpublished at-closure output is debt; an active project's is not owed yet. Ordered by tranche, then project key. The next output is the first due-now category with no published document.")}`,
      B(queueBadge, due || closureOwed ? "amber" : "green"),
    );
  }

  function expectedOutputPanel(project) {
    const title = "Expected outputs";
    const state = cs.outputs[project.id];
    if (!state) return panel(title, note("Loading this project's outputs…"));
    if (state.error) {
      return panel(
        title,
        note(`<strong>Read refused</strong> · ${E(consoleError(state.error))}`, "bad"),
      );
    }
    const expected = list((project.meta || {}).expected_outputs);
    const closure = list((project.meta || {}).closure_outputs);
    const categories = expected.length ? expected : INCEPTION_CATEGORIES;
    const stateOf = (category) => {
      const docs = state.items.filter((doc) => outputCategoryOf(doc) === category);
      const isPublished = docs.some((doc) => doc.publishable);
      return {
        docs,
        isPublished,
        label: docs.length ? (isPublished ? "published" : "present, private") : "missing",
        tone: isPublished ? "green" : docs.length ? "amber" : "bad",
      };
    };
    const compose = (category) =>
      C.can("documents.compose")
        ? button(
            `Compose ${category}`,
            "output-new-template",
            `data-project="${E(project.id)}" data-category="${E(category)}" data-unit="${E(project.key)}"`,
            "small",
            "plus",
          )
        : "";
    let published = 0;
    let nextMarked = false;
    const row = (category, markNext) => {
      const current = stateOf(category);
      if (current.isPublished) published += 1;
      // The first due-now category with no published document is the next output to compose.
      const next = markNext && !current.isPublished && !nextMarked;
      if (next) nextMarked = true;
      return `<tr><td><strong>${E(category)}</strong>${next ? ` ${B("next", "amber")}` : ""}</td><td>${B(current.label, current.tone)}</td><td>${current.docs.length ? E(current.docs.map((doc) => doc.title).join(" · ")) : "—"}</td><td>${compose(category)}</td></tr>`;
    };
    const rows = categories.map((category) => row(category, true));
    // P0's artefact #7 is the brief set across every delivery project; it is a computed
    // coverage row, not a document on P0.
    if (project.key === "P0") {
      consoleLoadRegister();
      const delivery = consoleProjects().filter(
        (row) => row.key !== "P0" && list((row.meta || {}).expected_outputs).length > 0,
      );
      const briefed = new Set(
        list(cs.register.items)
          .filter((doc) => outputCategoryOf(doc) === "project-brief" && doc.publishable)
          .map((doc) => doc.project_id),
      );
      const covered = delivery.filter((row) => briefed.has(row.id)).length;
      rows.push(
        `<tr><td><strong>project briefs</strong></td><td>${B(`${covered}/${delivery.length} published`, covered === delivery.length && delivery.length ? "green" : "amber")}</td><td><small>Across every delivery project</small></td><td></td></tr>`,
      );
    }
    let closurePublished = 0;
    const closureRows = closure.map((category) => {
      const current = stateOf(category);
      if (current.isPublished) closurePublished += 1;
      return `<tr><td><strong>${E(category)}</strong></td><td>${B(current.label, current.tone)}</td><td>${current.docs.length ? E(current.docs.map((doc) => doc.title).join(" · ")) : "—"}</td><td>${compose(category)}</td></tr>`;
    });
    const closureBlock = closure.length
      ? `<h4>At closure</h4>${table(["Output", "State", "Document", "Action"], closureRows)}`
      : "";
    return panel(
      title,
      `${table(["Output", "State", "Document", "Action"], rows)}${closureBlock}${note("The standard's due-now set is required now; the at-closure set is required at closure. The first missing due-now row is the next output to compose. A missing output is a visible gap; publish is an admin decision.")}`,
      B(
        `${published}/${categories.length} published${closure.length ? ` · closure ${closurePublished}/${closure.length}` : ""}`,
        published === categories.length ? "green" : "amber",
      ),
    );
  }

  function projectWorkspace(project) {
    const data = cs.byProject[project.id];
    const issues = consoleIssueRows(project);
    const board = data && data.board
      ? panel(
          `${project.key} · Board`,
          `${kv([
            ["Board", E(data.board.name)],
            ["Lanes", String(list(data.lists).length)],
            ["Cards", String(list(data.cards).filter((row) => !row.archived_at).length)],
          ])}${route("Open in Plan", "plan", "small", "board")}`,
        )
      : panel(
          "Board",
          !data
            ? note("Loading this project's board…")
            : data.error
              ? note(`<strong>Read refused</strong> · ${E(consoleError(data.error))}`, "bad")
              : note("No board. This project's outputs and checklist work without one; cards and runs need a board."),
        );
    const issueRows = issues.map(
      (issue) =>
        `<tr><td><a class="e-project-link" href="${E(issue.url || consoleIssueUrl(issue.number))}" target="_blank" rel="noopener">#${E(issue.number)} ↗</a> <strong>${E(issue.title)}</strong></td><td>${B(
          issue.state === "CLOSED" ? "Closed" : issue.state === "OPEN" ? "Open" : issue.state,
          issue.state === "CLOSED" ? "" : "amber",
        )}</td></tr>`,
    );
    const issuePanel = panel(
      `${project.key} · Linked issues`,
      issueRows.length
        ? table(["Issue", "GitHub state"], issueRows)
        : `<p class="muted">No GitHub issues are linked to this project. Intake a feedback issue below, or add issues to the project meta.</p>`,
      B(`${issues.filter((row) => row.state === "OPEN").length} open`, "green"),
    );
    const lanes = data ? list(data.lists) : [];
    const laneName = new Map(lanes.map((row) => [row.id, row.name]));
    const laneKind = new Map(lanes.map((row) => [row.id, row.kind]));
    const runsByCard = new Map();
    list(cs.runs).forEach((run) => {
      if (run.card_id && !runsByCard.has(run.card_id)) runsByCard.set(run.card_id, run);
    });
    const processOf = (card) => (card.meta || {}).process_key || card.process_key || "";
    const typeOf = (card) =>
      list(cs.types).find((row) => row.code === card.card_type_code) || null;
    const expectedOutputs = (card) =>
      list(((typeOf(card) || {}).definition || {}).outputs);
    const cardRows = (data && !data.error ? list(data.cards) : [])
      .filter((card) => !card.archived_at)
      .map((card) => {
        const processKey = processOf(card);
        const run = runsByCard.get(card.id);
        const issue = (card.meta || {}).issue;
        return `<tr><td><button type="button" class="console-card-link" data-v2="console-card" data-id="${E(card.id)}"><strong>${E(card.name)}</strong></button><small class="mono">${E(card.card_type_code)}</small></td><td>${E(laneName.get(card.list_id) || laneKind.get(card.list_id) || "—")}</td><td class="mono">${E(processKey || "not bound")}</td><td>${
          run
            ? `<button type="button" class="console-card-link" data-v2="run-open" data-id="${E(run.id)}">${B(run.status, runTone(run.status))}</button>`
            : "—"
        }</td><td><small>${E(expectedOutputs(card).join(" · ") || "—")}</small></td><td><div class="console-actions">${
          processKey && C.can("process_runs.start") && !(run && runLive(run.status))
            ? button("Start run", "run-start", `data-id="${E(card.id)}" data-key="${E(processKey)}"`, "small", "run")
            : ""
        }${
          C.can("documents.compose")
            ? button("Output", "output-card", `data-id="${E(card.id)}" data-project="${E(project.id)}"`, "small", "file")
            : ""
        }${
          issue != null
            ? `<a class="e-project-link" href="${E(consoleIssueUrl(issue))}" target="_blank" rel="noopener">#${E(issue)} ↗</a>`
            : ""
        }</div></td></tr>`;
      });
    const cardPanel =
      data && !data.error
        ? panel(
            `${project.key} · Cards`,
            cardRows.length
              ? table(["Card", "Lane", "Process", "Latest run", "Expected outputs", "Operate"], cardRows)
              : `<p class="muted">No open card. Intake an issue from the feedback panel; intake binds the card type's READY process.</p>`,
            B(`${cardRows.length} open`, cardRows.length ? "green" : ""),
          )
        : "";
    const failure = cs.lastError
      ? note(`<strong>Last refusal</strong> · ${E(consoleError(cs.lastError))}`, "bad")
      : "";
    return `${failure}${deliveryBoard(project)}${coveragePanel(project)}${board}${cardPanel}${issuePanel}${expectedOutputPanel(project)}${projectOutputs(project)}${projectFeedback(project)}`;
  }

  function projectOutputs(project) {
    const state = cs.outputs[project.id];
    if (!state) return panel("Outputs", note("Loading this project's outputs…"));
    if (state.error) {
      return panel(
        "Outputs",
        note(`<strong>Read refused</strong> · ${E(consoleError(state.error))}`, "bad"),
      );
    }
    // Grouped by the unit and the category the author recorded: a project's work is read unit by
    // unit, not as one flat list. A row with no metadata is shown honestly as "no unit".
    const groups = new Map();
    state.items.forEach((doc) => {
      const key = `${outputUnitOf(doc) || "no unit"} · ${outputCategoryOf(doc) || "uncategorised"}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(doc);
    });
    const rows = [...groups.entries()].flatMap(([key, docs]) => [
      `<tr class="console-group"><td colspan="5"><strong>${E(key)}</strong></td></tr>`,
      ...docs.map((doc) => {
        const actions = [
          C.can("documents.get")
            ? button("Read", "output-read", `data-id="${E(doc.id)}"`, "small", "file")
            : "",
          C.can("documents.update")
            ? button("Edit", "output-edit", `data-id="${E(doc.id)}"`, "small", "edit")
            : "",
          C.can("documents.revision_list")
            ? button(
                "History",
                "output-history",
                `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
                "small",
                "file",
              )
            : "",
          C.can("documents.create")
            ? button("Attach", "output-attach", `data-id="${E(doc.id)}"`, "small", "plus")
            : "",
          !doc.publishable && C.can("documents.request_publish") && !doc.publish_requested_at
            ? button(
                "Request publish",
                "output-request-publish",
                `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
                "small",
                "shield",
              )
            : "",
          isAdmin() && C.can("documents.publish")
            ? button(
                doc.publishable ? "Unpublish" : "Publish",
                "output-publish",
                `data-id="${E(doc.id)}" data-public="${doc.publishable ? "1" : "0"}" data-version="${E(doc.version)}"`,
                "small",
                "shield",
              )
            : "",
        ].join("");
        return `<tr><td><strong>${E(doc.title)}</strong><small class="mono">v${E(doc.version)} · ${E(doc.content_type || "text/markdown")}</small></td><td>${E(outputAuthorOf(doc) || "—")}</td><td>${B(outputStateOf(doc), doc.publishable ? "green" : "")}</td><td><small>${E(short(doc.updated_at, 16))}</small></td><td><div class="console-actions">${actions}</div></td></tr>`;
      }),
    ]);
    return panel(
      `${project.key} · Outputs`,
      rows.length
        ? table(["Output", "Author", "State", "Updated", "Operate"], rows)
        : `<p class="muted">No output yet. An output is a design note, evidence record or decision bound to this project and, optionally, a card.</p>`,
      C.can("documents.compose")
        ? button("Compose output", "output-new", `data-project="${E(project.id)}"`, "small", "plus")
        : "",
    );
  }

  function projectFeedback(project) {
    const feedback = cs.issues.filter(isFeedback);
    const target = v.feedbackProject || project.id;
    const picker = `<label class="e-select">Intake into <select data-v2-input="feedback-project">${consoleProjects()
      .map(
        (row) =>
          `<option value="${E(row.id)}" ${row.id === target ? "selected" : ""}>${E(row.key)} · ${E(row.name)}</option>`,
      )
      .join("")}</select></label>`;
    const intakeType =
      v.intakeType ||
      (list(cs.types).find((row) => row.code === "architecture_task") || {}).code ||
      (list(cs.types)[0] || {}).code ||
      "";
    const typePicker = `<label class="e-select">Card type <select data-v2-input="intake-type" aria-label="Intake card type">${list(cs.types)
      .map(
        (row) =>
          `<option value="${E(row.code)}" ${row.code === intakeType ? "selected" : ""}>${E(row.code)}</option>`,
      )
      .join("")}</select></label>`;
    const rows = feedback.map(
      (issue) =>
        `<tr><td><a class="e-project-link" href="${E(issue.url || consoleIssueUrl(issue.number))}" target="_blank" rel="noopener">#${E(
          issue.number,
        )} ↗</a> <strong>${E(issue.title)}</strong></td><td>${B(
          issue.state === "CLOSED" ? "Closed" : "Open",
          issue.state === "CLOSED" ? "" : "amber",
        )}</td><td>${
          C.can("cards.create")
            ? button("Intake", "feedback-intake", `data-issue="${E(issue.number)}"`, "small", "plus")
            : "—"
        }</td></tr>`,
    );
    return panel(
      "Public feedback",
      `${picker}${typePicker}${
        rows.length
          ? table(["Feedback issue", "GitHub state", "Operate"], rows)
          : `<p class="muted">No feedback issue is registered yet. The public feedback template opens one on GitHub.</p>`
      }${note(
        "Feedback stays a GitHub issue. Intake creates a card in the chosen project's intake lane and records the issue number on it; the issue itself is not modified.",
      )}`,
      B(`${feedback.length} feedback`, feedback.length ? "amber" : ""),
    );
  }


  /* ------------------------------------------------------------- workspace actions */
  const closeWorkspaceDialog = () => {
    const dialog = document.getElementById("dialog");
    if (dialog && dialog.open) dialog.close();
  };
  const projectById = (id) => cs.projects.find((row) => row.id === id) || null;

  function consoleProjectNew() {
    if (!consoleOn()) return;
    const classes = OUTPUT_CLASSES.map(
      (cls) => `<option value="${E(cls.name)}">${E(classLabel(cls))}</option>`,
    ).join("");
    const body = `<div class="dialog-body console-dialog-body"><label class="field full"><span>Name</span><input id="project-name" placeholder="The part of the conveyor this project delivers"></label><label class="field full"><span>Key (optional)</span><input id="project-key" placeholder="T18" spellcheck="false"></label><label class="field full"><span>Description</span><textarea id="project-description" rows="3"></textarea></label><label class="field full"><span>Delivery class</span><select id="project-class"><option value="">Choose a class…</option>${classes}</select></label><label class="field full"><span>Tranche</span><input id="project-tranche" type="number" min="0" step="1" placeholder="0"></label><label class="field full"><span>Repository URL</span><input id="project-repo" placeholder="https://github.com/nonagenticai/conveyor" spellcheck="false"></label><label class="field full"><span>Plan path (docs/…)</span><input id="project-plan" placeholder="docs/plans/slices/S00-usage-inventory.md" spellcheck="false"></label><p class="muted">The class sets the expected due-now and at-closure outputs from the seeded standard. The seed owns its own 48 rows; a project created here is not in the seed.</p></div>`;
    openDialog(
      "New project",
      body,
      `${button("Create", "project-create", "", "primary", "plus")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  async function consoleProjectCreate() {
    const title = (document.getElementById("project-name") || {}).value || "";
    const key = (document.getElementById("project-key") || {}).value || "";
    const description = (document.getElementById("project-description") || {}).value || "";
    const plan = (document.getElementById("project-plan") || {}).value || "";
    const repo = (document.getElementById("project-repo") || {}).value || "";
    const tranche = (document.getElementById("project-tranche") || {}).value || "";
    const chosenClass = (document.getElementById("project-class") || {}).value || "";
    if (!title.trim()) {
      toast("A project needs a name");
      return;
    }
    const cls = OUTPUT_CLASSES.find((row) => row.name === chosenClass) || null;
    // Fail closed: a project without a class would be absent from the programme queue and its
    // checklist would fall back to a guess. The owner chooses the class, never a default.
    if (!cls) {
      toast("Choose a delivery class for the project");
      return;
    }
    const programme =
      (cs.programme && cs.programme.id) ||
      (consoleProjects().find((row) => row.programme_id) || {}).programme_id ||
      null;
    const payload = {
      title: title.trim(),
      description: description.trim(),
      programme_id: programme,
    };
    if (key.trim()) payload.key = key.trim().toUpperCase();
    if (repo.trim()) payload.repo_url = repo.trim();
    let projectId = null;
    try {
      const created = await C.action("projects.create", payload);
      projectId = created && created.project ? created.project.id : null;
      if (!projectId) throw new Error("projects.create returned no project");
      // `projects.create` has no meta input: the class and plan are a second, explicit write.
      // The programme owns these keys, so the write goes through `projects.programme_meta`.
      const meta = classMeta(cls, tranche);
      if (plan.trim()) meta.plan = plan.trim();
      try {
        await C.action(
          "projects.programme_meta",
          { id: projectId, meta },
          { etag: created.project.version },
        );
      } catch (error) {
        // The project exists — say exactly that, never a false "create refused".
        cs.lastError = error;
        toast("Project created, class not saved · " + consoleError(error));
      }
      closeWorkspaceDialog();
      cs.loaded = false;
      cs.byProject = {};
      v.projectPage = projectId;
      toast(`Project created · ${cls.name}`);
      render();
      consoleLoad();
    } catch (error) {
      cs.lastError = error;
      toast("Create refused · " + consoleError(error));
      render();
    }
  }

  function consoleProjectEdit(id) {
    const project = projectById(id);
    if (!project) return;
    const currentClass = classOfProject(project);
    const classes = OUTPUT_CLASSES.map(
      (cls) =>
        `<option value="${E(cls.name)}" ${currentClass && currentClass.name === cls.name ? "selected" : ""}>${E(classLabel(cls))}</option>`,
    ).join("");
    const keep = currentClass
      ? ""
      : `<option value="" selected>Keep the current expected outputs (no matching class)</option>`;
    const body = `<div class="dialog-body console-dialog-body"><label class="field full"><span>Name</span><input id="project-name" value="${E(project.name)}"></label><label class="field full"><span>Description</span><textarea id="project-description" rows="3">${E(project.description || "")}</textarea></label><label class="field full"><span>Delivery class</span><select id="project-class">${keep}${classes}</select></label><label class="field full"><span>Tranche</span><input id="project-tranche" type="number" min="0" step="1" value="${E((project.meta || {}).tranche ?? "")}"></label><label class="field full"><span>Repository URL</span><input id="project-repo" value="${E(project.repo_url || "")}" spellcheck="false"></label><label class="field full"><span>Plan path (docs/…)</span><input id="project-plan" value="${E(((project.meta || {}).plan) || "")}" spellcheck="false"></label><p class="muted">Choosing a class recomputes the expected due-now and at-closure outputs; other meta keys are preserved. The seed owns its own 48 rows — a later seed run restores their class and sets.</p></div>`;
    openDialog(
      `${project.key} · edit`,
      body,
      `${button("Save", "project-save", `data-id="${E(project.id)}"`, "primary", "shield")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  async function consoleProjectSave(id) {
    const project = projectById(id);
    if (!project) return;
    const name = (document.getElementById("project-name") || {}).value || "";
    const description = (document.getElementById("project-description") || {}).value || "";
    const repoUrl = (document.getElementById("project-repo") || {}).value || "";
    const plan = (document.getElementById("project-plan") || {}).value || "";
    const tranche = (document.getElementById("project-tranche") || {}).value || "";
    const chosenClass = (document.getElementById("project-class") || {}).value || "";
    if (!name.trim()) {
      toast("A project needs a name");
      return;
    }
    const currentClass = classOfProject(project);
    const cls = OUTPUT_CLASSES.find((row) => row.name === chosenClass) || null;
    const meta = { plan: plan.trim() };
    if (cls && (!currentClass || currentClass.name !== cls.name)) {
      Object.assign(meta, classMeta(cls, tranche));
    } else if (String(tranche).trim() !== "" && Number.isFinite(Number(tranche))) {
      meta.tranche = Number(tranche);
    }
    try {
      // The project's own fields and the programme-owned meta are two writes with two
      // authorities; the second takes the version the first produced.
      const updated = await C.action(
        "projects.update",
        {
          id: project.id,
          name: name.trim(),
          description: description.trim(),
          repo_url: repoUrl.trim(),
        },
        { etag: project.version },
      );
      const nextVersion = updated && updated.project ? updated.project.version : null;
      if (nextVersion && Object.keys(meta).length) {
        await C.action(
          "projects.programme_meta",
          { id: project.id, meta },
          { etag: nextVersion },
        );
      }
      closeWorkspaceDialog();
      cs.loaded = false;
      cs.byProject = {};
      toast("Project updated");
      render();
      consoleLoad();
    } catch (error) {
      cs.lastError = error;
      toast("Edit refused · " + consoleError(error));
      render();
    }
  }

  function consoleProjectClose(id) {
    const project = projectById(id);
    if (!project) return;
    const body = `<div class="dialog-body console-dialog-body"><p class="muted">Closing <strong>${E(project.key)}</strong> is a status transition; the row and its history survive.</p><label class="field full"><span>Reason</span><textarea id="project-close-reason" rows="3" placeholder="Why this project is closing"></textarea></label></div>`;
    openDialog(
      `Close ${project.key}`,
      body,
      `${button("Close project", "project-close-save", `data-id="${E(project.id)}"`, "primary", "archive")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  // A status transition is its own action: `projects.update` no longer writes status.
  async function consoleProjectTransition(id, action, verb, done) {
    try {
      await C.action(action, { id });
      closeWorkspaceDialog();
      cs.loaded = false;
      cs.byProject = {};
      toast(done);
      render();
      consoleLoad();
    } catch (error) {
      cs.lastError = error;
      toast(`${verb} refused · ` + consoleError(error));
      render();
    }
  }

  async function consoleProjectCloseSave(id) {
    const reason = (document.getElementById("project-close-reason") || {}).value || "";
    try {
      await C.action("projects.close", { id, reason: reason.trim() || null });
      closeWorkspaceDialog();
      cs.loaded = false;
      cs.byProject = {};
      toast("Project closed");
      render();
      consoleLoad();
    } catch (error) {
      cs.lastError = error;
      toast("Close refused · " + consoleError(error));
      render();
    }
  }

  function consoleOutputNew(projectId, cardId, template = {}) {
    v.outputCard = cardId || null;
    const project = projectId ? projectById(projectId) : null;
    // Prefill the category with the template's, else the project's first expected output that is
    // not yet published. The unit is the project key unless the caller named one.
    const state = project ? cs.outputs[project.id] : null;
    const expected = list(((project || {}).meta || {}).expected_outputs);
    const nextCategory =
      template.category ||
      expected.find(
        (category) =>
          !(state && !state.error ? state.items : []).some(
            (doc) => doc.publishable && outputCategoryOf(doc) === category,
          ),
      ) ||
      "";
    const unitValue = template.unit || (project ? project.key : "");
    const projectSelect = projectId
      ? ""
      : `<label class="field full"><span>Project</span><select id="output-project">${consoleProjects()
          .map((p) => `<option value="${E(p.id)}">${E(p.key)} · ${E(p.name)}</option>`)
          .join("")}</select></label>`;
    const options = OUTPUT_CATEGORIES.map(
      (c) =>
        `<option value="${E(c)}" ${nextCategory === c ? "selected" : ""}>${E(c)}</option>`,
    ).join("");
    const seedBody =
      template.body ||
      (nextCategory ? (outputTemplateOf(nextCategory) || {}).skeleton || "" : "");
    const body = `<div class="dialog-body console-dialog-body">${cardId ? `<p class="muted">Bound to card <span class="mono">${E(short(cardId, 12))}</span>.</p>` : ""}${projectSelect}<label class="field full"><span>Title</span><input id="output-title" value="${E(template.title || "")}" placeholder="What this output records"></label><label class="field full"><span>Unit (project key or issue number)</span><input id="output-unit" value="${E(unitValue)}" placeholder="${E((project || {}).key || "121")}" spellcheck="false"></label><label class="field full"><span>Category</span><select id="output-category"><option value="">—</option>${options}</select></label><label class="field full"><span>Body (Markdown)</span><textarea id="output-body" rows="10" placeholder="# Design note&#10;&#10;What was decided and why.">${E(seedBody)}</textarea></label><div class="output-completeness-panel"><span class="mono muted">Completeness · the standard's required sections</span><div id="output-completeness"></div></div><p class="muted">The body is capped at 256 KiB. Saving keeps it a draft; an admin publishes it after review, and the server refuses a publish missing a required section.</p></div>`;
    openDialog(
      nextCategory ? `Compose ${nextCategory}` : "Compose output",
      body,
      `${button("Save output", "output-save", `data-project="${E(projectId || "")}"`, "primary", "plus")}${btn("Cancel", "close-dialog")}`,
      true,
    );
    wireCompleteness();
  }

  async function consoleOutputSave(projectId) {
    const title = (document.getElementById("output-title") || {}).value || "";
    const body = (document.getElementById("output-body") || {}).value || "";
    const chosen =
      (document.getElementById("output-project") || {}).value || projectId || "";
    const unit = ((document.getElementById("output-unit") || {}).value || "").trim();
    const category = (document.getElementById("output-category") || {}).value || "";
    if (!title.trim() || !body.trim()) {
      toast("An output needs a title and a body");
      return;
    }
    if (!chosen) {
      toast("Choose a project for this output");
      return;
    }
    try {
      await C.action("documents.compose", {
        project_id: chosen,
        card_id: v.outputCard || undefined,
        title: title.trim(),
        body,
        category: category || undefined,
        unit: unit || undefined,
      });
      v.outputCard = null;
      closeWorkspaceDialog();
      toast("Output saved as a draft");
      consoleRefreshOutputs(chosen);
    } catch (error) {
      cs.lastError = error;
      toast("Compose refused · " + consoleError(error));
      render();
    }
  }

  async function consoleOutputEdit(id) {
    let doc;
    try {
      doc = await C.action("documents.get", { id });
    } catch (error) {
      toast("Read refused · " + consoleError(error));
      return;
    }
    const locked = doc.publishable && !isAdmin();
    const options = OUTPUT_CATEGORIES.map(
      (c) =>
        `<option value="${E(c)}" ${outputCategoryOf(doc) === c ? "selected" : ""}>${E(c)}</option>`,
    ).join("");
    const rework = (doc.meta || {}).rework;
    const reworkNote = rework
      ? note(
          `<strong>Returned</strong> · ${E(rework.note || "")}${
            list(rework.missing).length
              ? `<br><span class="mono">Missing: ${list(rework.missing).map(E).join(", ")}</span>`
              : ""
          }`,
          "amber",
        )
      : "";
    const body = `<div class="dialog-body console-dialog-body">${reworkNote}${
      doc.publishable
        ? note(
            locked
              ? "<strong>Published</strong> · this output is public; only an admin can edit it. Ask an admin to unpublish it first."
              : "<strong>Published</strong> · an edit is versioned and stays public until an admin unpublishes it.",
          )
        : ""
    }<label class="field full"><span>Title</span><input id="output-title" value="${E(doc.title)}" ${locked ? "disabled" : ""}></label><label class="field full"><span>Category</span><select id="output-category" ${locked ? "disabled" : ""}><option value="">—</option>${options}</select></label><label class="field full"><span>Unit (project key or issue)</span><input id="output-unit" value="${E(outputUnitOf(doc))}" spellcheck="false" ${locked ? "disabled" : ""}></label><label class="field full"><span>Body (Markdown)</span><textarea id="output-body" rows="10" ${locked ? "disabled" : ""}>${E(doc.body || "")}</textarea></label><div class="output-completeness-panel"><span class="mono muted">Completeness · the standard's required sections</span><div id="output-completeness"></div></div><p class="muted">Version ${E(doc.version)} · content hash <span class="mono">${E(short(doc.content_hash, 12))}</span>. Every save keeps the previous version in the history.</p></div>`;
    const actions = `${
      locked
        ? ""
        : button(
            "Save changes",
            "output-update",
            `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`,
            "primary",
            "shield",
          )
    }${C.can("documents.preview") ? button("Preview", "output-preview", `data-id="${E(doc.id)}"`, "", "eye") : ""}${button("History", "output-history", `data-id="${E(doc.id)}" data-version="${E(doc.version)}"`, "", "file")}${btn("Cancel", "close-dialog")}`;
    openDialog(`Edit · ${doc.title}`, body, actions, true);
    wireCompleteness();
  }

  async function consoleOutputUpdate(id, version) {
    const title = (document.getElementById("output-title") || {}).value || "";
    const body = (document.getElementById("output-body") || {}).value || "";
    const category = (document.getElementById("output-category") || {}).value || "";
    const unit = ((document.getElementById("output-unit") || {}).value || "").trim();
    if (!title.trim() || !body.trim()) {
      toast("An output needs a title and a body");
      return;
    }
    try {
      const updated = await C.action(
        "documents.update",
        { id, title: title.trim(), body, category, unit },
        { etag: Number(version) },
      );
      closeWorkspaceDialog();
      toast("Output updated");
      consoleRefreshOutputs(updated.project_id);
    } catch (error) {
      cs.lastError = error;
      toast("Update refused · " + consoleError(error));
      render();
    }
  }

  async function consoleOutputPublish(id, publishable, version) {
    try {
      const updated = await C.action(
        "documents.publish",
        { id, publish: publishable },
        { etag: Number(version) },
      );
      toast(publishable ? "Output published" : "Output unpublished");
      consoleRefreshOutputs(updated.project_id);
    } catch (error) {
      cs.lastError = error;
      const missing = missingFromError(error);
      if (publishable && error.status === 409 && missing.length) {
        // The block names what is missing and offers the coach's triage; it is not a dead end.
        consoleIncompleteDialog(id, version, missing);
        return;
      }
      toast("Publish refused · " + consoleError(error));
      render();
    }
  }

  async function consoleOutputRequestPublish(id, version) {
    try {
      const updated = await C.action(
        "documents.request_publish",
        { id },
        { etag: Number(version) },
      );
      toast("Publish requested");
      consoleRefreshOutputs(updated.project_id);
    } catch (error) {
      cs.lastError = error;
      toast("Request refused · " + consoleError(error));
      render();
    }
  }

  async function consoleOutputHistory(id, version) {
    try {
      const revisions = list(await C.action("documents.revision_list", { id, limit: 50 }));
      const rows = revisions.map((rev) => {
        const actions = [
          button(
            "View",
            "revision-view",
            `data-id="${E(id)}" data-seq="${E(rev.seq)}"`,
            "small",
            "file",
          ),
          C.can("documents.revision_restore")
            ? button(
                "Restore",
                "revision-restore",
                `data-id="${E(id)}" data-seq="${E(rev.seq)}" data-version="${E(version)}"`,
                "small",
                "replay",
              )
            : "",
        ].join("");
        return `<tr><td class="mono">v${E(rev.seq)}</td><td>${E(rev.label)}</td><td>${E(rev.author || "—")}</td><td><small>${E(short(rev.created_at, 16))}</small></td><td><div class="console-actions">${actions}</div></td></tr>`;
      });
      openDialog(
        "History",
        `<div class="dialog-body console-dialog-body">${
          rows.length
            ? table(["Version", "Change", "Author", "When", "Operate"], rows)
            : "<p class='muted'>No revision recorded yet.</p>"
        }<p class="muted">Restore copies the chosen text forward as a new draft; it never rewrites history and never republishes.</p></div>`,
        btn("Close", "close-dialog"),
        true,
      );
    } catch (error) {
      toast("History refused · " + consoleError(error));
    }
  }

  async function consoleRevisionView(id, seq) {
    try {
      const rev = await C.action("documents.revision_get", { id, seq: Number(seq) });
      const content = rev.content || {};
      const body = `<div class="dialog-body console-dialog-body">${kv([
        ["Version", `<span class="mono">v${E(rev.seq)}</span>`],
        ["Change", E(rev.label)],
        ["Author", E(rev.author || "—")],
      ])}<pre class="console-output-body">${E(content.body || "(no body)")}</pre></div>`;
      openDialog(`Version ${seq}`, body, btn("Close", "close-dialog"), true);
    } catch (error) {
      toast("Version refused · " + consoleError(error));
    }
  }

  function consoleRevisionRestore(id, seq, version) {
    openDialog(
      "Restore version",
      `<div class="dialog-body console-dialog-body"><p>Restore <span class="mono">v${E(seq)}</span> as a new draft? The current version stays in the history, and the restored text is not public until an admin publishes it.</p></div>`,
      `${button("Restore", "revision-restore-confirm", `data-id="${E(id)}" data-seq="${E(seq)}" data-version="${E(version)}"`, "primary", "replay")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  async function consoleRevisionRestoreConfirm(id, seq, version) {
    try {
      const updated = await C.action(
        "documents.revision_restore",
        { id, seq: Number(seq) },
        { etag: Number(version) },
      );
      closeWorkspaceDialog();
      toast("Version restored as a draft");
      consoleRefreshOutputs(updated.project_id);
    } catch (error) {
      cs.lastError = error;
      toast("Restore refused · " + consoleError(error));
      render();
    }
  }

  /* ---------------------------------------------------------- attachments */
  const fileBase64 = (file) =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result || "").split(",")[1] || "");
      reader.onerror = () => reject(reader.error || new Error("the file could not be read"));
      reader.readAsDataURL(file);
    });

  const contentTypeOf = (name) => {
    const extension = String(name || "").toLowerCase().split(".").pop();
    return (
      {
        png: "image/png",
        jpg: "image/jpeg",
        jpeg: "image/jpeg",
        pdf: "application/pdf",
        svg: "image/svg+xml",
        json: "application/json",
        csv: "text/csv",
        md: "text/markdown",
        txt: "text/plain",
      }[extension] || "text/plain"
    );
  };

  async function consoleOutputAttach(id) {
    openDialog(
      "Attachments",
      `<div class="dialog-body console-dialog-body"><div id="output-attachments">Loading attachments…</div><label class="field full"><span>Add a file (10 MiB cap)</span><input type="file" data-v2-input="output-attachment" data-id="${E(id)}" accept=".png,.jpg,.jpeg,.pdf,.svg,.json,.csv,.md,.txt"></label><p class="muted">PNG and JPEG are shown inline on the public page; every other type is served as a download. An attachment is public only while its output is.</p></div>`,
      btn("Close", "close-dialog"),
      true,
    );
    consoleLoadAttachments(id);
  }

  async function consoleLoadAttachments(id) {
    const host = document.getElementById("output-attachments");
    if (!host) return;
    try {
      const page = await C.action("documents.list", {
        parent_document_id: id,
        kind: "attachment",
        page: { limit: 50 },
      });
      const rows = list(page.items);
      host.innerHTML = rows.length
        ? `<ul class="console-attachments">${rows
            .map(
              (row) =>
                `<li><strong>${E(row.title)}</strong> <small class="mono">${E(row.content_type || "")}${
                  row.size_bytes ? ` · ${E(row.size_bytes)} B` : ""
                }</small> ${C.can("documents.download") ? button("Download", "attachment-download", `data-id="${E(row.id)}" data-type="${E(row.content_type || "")}" data-name="${E(row.title)}"`, "small", "download") : ""}</li>`,
            )
            .join("")}</ul>`
        : `<p class="muted">No attachment yet.</p>`;
    } catch (error) {
      host.innerHTML = `<p class="muted">Attachments could not be read: ${E(consoleError(error))}</p>`;
    }
  }

  async function consoleOutputAttachFile(input) {
    const file = input.files && input.files[0];
    const id = input.dataset.id;
    if (!file || !id) return;
    if (file.size > 10 * 1024 * 1024) {
      toast("The file is larger than 10 MiB");
      input.value = "";
      return;
    }
    try {
      const content_base64 = await fileBase64(file);
      await C.action("documents.create", {
        title: file.name,
        content_type: file.type || contentTypeOf(file.name),
        content_base64,
        parent_document_id: id,
      });
      toast("Attachment uploaded");
      input.value = "";
      consoleLoadAttachments(id);
    } catch (error) {
      cs.lastError = error;
      toast("Attachment refused · " + consoleError(error));
    }
  }

  async function consoleAttachmentDownload(id, contentType, filename) {
    try {
      const dl = await C.action("documents.download", { id });
      const bytes = Uint8Array.from(atob(dl.content_base64 || ""), (c) => c.charCodeAt(0));
      const blob = new Blob([bytes], {
        type: dl.content_type || contentType || "application/octet-stream",
      });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = filename || "attachment";
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      toast("Download refused · " + consoleError(error));
    }
  }

  async function consoleOutputRead(id) {
    try {
      const doc = await C.action("documents.get", { id });
      const body = `<div class="dialog-body console-dialog-body">${kv([
        ["Status", B(doc.publishable ? "public" : "private", doc.publishable ? "green" : "")],
        ["Version", `<span class="mono">${E(doc.version)}</span>`],
        ["Content type", E(doc.content_type || "text/markdown")],
        ["Updated", E(doc.updated_at)],
      ])}<pre class="console-output-body">${E(doc.body || "(no body)")}</pre></div>`;
      openDialog(doc.title, body, btn("Close", "close-dialog"), true);
    } catch (error) {
      toast("Read refused · " + consoleError(error));
    }
  }

  async function consoleOutputDownload(id) {
    try {
      const dl = await C.action("documents.download", { id });
      const bytes = Uint8Array.from(atob(dl.content_base64 || ""), (c) => c.charCodeAt(0));
      const text = new TextDecoder().decode(bytes);
      const extension = (dl.content_type || "").includes("markdown") ? "md" : "txt";
      download(`output-${short(id, 8)}.${extension}`, text, dl.content_type || "text/plain");
    } catch (error) {
      toast("Download refused · " + consoleError(error));
    }
  }

  function consoleOutputDelete(id, version) {
    openDialog(
      "Delete output",
      `<div class="dialog-body console-dialog-body"><p>A delete is a tombstone: the row leaves the public page and the register. The command is version-guarded (<span class="mono">v${E(version)}</span>).</p></div>`,
      `${button("Delete", "output-delete-confirm", `data-id="${E(id)}" data-version="${E(version)}"`, "primary", "archive")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  async function consoleOutputDeleteConfirm(id, version) {
    try {
      await C.action("documents.delete", { id }, { etag: Number(version) });
      closeWorkspaceDialog();
      toast("Output deleted");
      consoleRefreshOutputs(null);
    } catch (error) {
      cs.lastError = error;
      toast("Delete refused · " + consoleError(error));
      render();
    }
  }

  function consoleOutputTemplate(projectId, category) {
    const project = projectById(projectId);
    if (!project) return;
    const template = outputTemplateOf(category);
    if (!template) {
      toast(`No template for ${category} in the standard`);
      return;
    }
    const label = category.replace(/-/g, " ");
    const heading = label.charAt(0).toUpperCase() + label.slice(1);
    consoleOutputNew(projectId, null, {
      category,
      title: `${heading} — ${project.key} ${project.name}`,
      body: template.skeleton,
    });
  }

  async function consoleOutputPreview(id) {
    let preview;
    try {
      preview = await C.action("documents.preview", { id });
    } catch (error) {
      toast("Preview refused · " + consoleError(error));
      return;
    }
    const missing = list(preview.missing);
    const body = `<div class="dialog-body console-dialog-body"><p class="muted">Rendered by the same renderer as the public page. ${
      missing.length
        ? `Still missing: <span class="mono">${missing.map(E).join(", ")}</span>`
        : "Every required section is present."
    }</p><iframe class="console-preview" title="Output preview" sandbox srcdoc="${E(preview.html)}" style="width:100%;height:420px;border:1px solid var(--line);border-radius:8px;background:#fff"></iframe><p class="muted">Author HTML is shown as text and a non-http(s)/mailto link is inert; the public page would serve exactly this.</p></div>`;
    openDialog("Preview", body, btn("Close", "close-dialog"), true);
  }

  // The triage a blocked publish offers: fix it, return it, or record a system issue as a kata.
  function consoleIncompleteDialog(id, version, missing) {
    const body = `<div class="dialog-body console-dialog-body"><p><strong>409 output_incomplete</strong> · the standard requires these sections before this output can be public:</p><ul class="console-completeness">${missing
      .map((heading) => `<li class="is-missing">○ ${E(heading)}</li>`)
      .join("")}</ul>${note("Edit the draft and publish again, return it to the author with a note, or open a kata when the gap is the system's (template, instrument or source).")}</div>`;
    openDialog(
      "Publish blocked · output_incomplete",
      body,
      `${button("Return with a note", "request-return", `data-id="${E(id)}" data-version="${E(version)}"`, "", "replay")}${button("Open a kata", "request-kata", `data-id="${E(id)}" data-version="${E(version)}"`, "", "lightning")}${button("Edit the draft", "output-edit", `data-id="${E(id)}"`, "primary", "edit")}${btn("Close", "close-dialog")}`,
      true,
    );
  }

  function consoleReturnDialog(id, version) {
    const doc = findOutput(id) || {};
    const body = `<div class="dialog-body console-dialog-body"><p class="muted">Returning <strong>${E(doc.title || "this output")}</strong> to its author. The server records the sections the standard finds missing.</p><label class="field full"><span>Reason (the author sees this)</span><textarea id="return-note" rows="4" placeholder="What must change before it can be public."></textarea></label><p class="muted">The publish request clears; the draft's content is unchanged.</p></div>`;
    openDialog(
      "Return this output",
      body,
      `${button("Return to the author", "return-save", `data-id="${E(id)}" data-version="${E(version)}"`, "primary", "replay")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  async function consoleReturnSave(id, version) {
    const noteText = ((document.getElementById("return-note") || {}).value || "").trim();
    if (!noteText) {
      toast("A return needs a reason");
      return;
    }
    const doc = findOutput(id) || {};
    try {
      const returned = await C.action("documents.return", { id, note: noteText }, {
        etag: Number(version),
      });
      const missing = list(((returned.meta || {}).rework || {}).missing);
      closeWorkspaceDialog();
      toast(
        missing.length
          ? `Returned · missing: ${missing.join(", ")}`
          : "Returned to the author",
      );
      consoleRefreshOutputs(doc.project_id || null);
      render();
    } catch (error) {
      toast("Return refused · " + consoleError(error));
    }
  }

  function consoleKataDialog(id, version) {
    const source = findOutput(id) || {};
    const projects = consoleProjects();
    const kata = projects.find((row) => row.key === "S18") || projects[0] || null;
    const template = outputTemplateOf("decision");
    const skeleton = template ? template.skeleton : "# Decision\n\n## Context\n\n## Decision\n\n## Consequences\n\n## Evidence\n";
    const body = `<div class="dialog-body console-dialog-body"><p class="muted">A kata records a <strong>system</strong> defect found while reviewing. The source is returned with a note that points at the kata; the kata is a decision on the kata project, never an edit to the source.</p><label class="field full"><span>Kata project</span><select id="kata-project">${projects
      .map(
        (row) =>
          `<option value="${E(row.id)}" ${kata && row.id === kata.id ? "selected" : ""}>${E(row.key)} · ${E(row.name)}</option>`,
      )
      .join("")}</select></label><label class="field full"><span>Coach note (what the system must fix)</span><textarea id="kata-note" rows="3" placeholder="The missing template, instrument or source."></textarea></label><label class="field full"><span>Kata body (Markdown, the decision template)</span><textarea id="kata-body" rows="8">${E(skeleton)}</textarea></label></div>`;
    openDialog(
      "Open a kata",
      body,
      `${button("Open the kata and return the source", "kata-save", `data-id="${E(id)}" data-version="${E(version)}"`, "primary", "lightning")}${btn("Cancel", "close-dialog")}`,
      true,
    );
  }

  async function consoleKataSave(id, version) {
    const source = findOutput(id) || {};
    const projectId = ((document.getElementById("kata-project") || {}).value || "").trim();
    const noteText = ((document.getElementById("kata-note") || {}).value || "").trim();
    const body = ((document.getElementById("kata-body") || {}).value || "").trim();
    const project = consoleProjects().find((row) => row.id === projectId);
    if (!projectId || !project) {
      toast("Choose the kata project");
      return;
    }
    if (!noteText) {
      toast("A kata needs the coach's note");
      return;
    }
    if (!body) {
      toast("The kata needs a body");
      return;
    }
    let kata;
    try {
      kata = await C.action("documents.compose", {
        project_id: projectId,
        title: `Kata — ${source.title || "output"}`,
        body,
        category: "decision",
        unit: project.key,
        meta: {
          kata: true,
          system_issue: true,
          source_document_id: id,
          source_project_id: source.project_id || undefined,
          coach_note: noteText,
        },
        publishable: true,
      });
    } catch (error) {
      toast("Kata refused · " + consoleError(error));
      return;
    }
    try {
      await C.action(
        "documents.return",
        { id, note: `Kata ${kata.id} records the system issue: ${noteText}` },
        { etag: Number(version) },
      );
      toast(`Kata ${short(kata.id, 12)} opened and the source returned`);
    } catch (error) {
      toast(
        `Kata ${short(kata.id, 12)} opened; the source was not returned · ${consoleError(error)}`,
      );
    }
    closeWorkspaceDialog();
    consoleRefreshOutputs(source.project_id || null);
    render();
  }

  async function consoleFeedbackIntake(issueNumber) {
    const select = document.querySelector('[data-v2-input="feedback-project"]');
    const projectId = select ? select.value : "";
    if (!projectId) {
      toast("Choose the project to intake into");
      return;
    }
    v.feedbackProject = projectId;
    if (!cs.byProject[projectId] || !cs.byProject[projectId].board) {
      await consoleLoadProject(projectId);
    }
    await consoleIntake(projectId, issueNumber);
  }


  /* The Plan · Boards tab on the operator console: the tenant's own projects, its boards,
     its cards and its GitHub registry — read live, written through the action API. */
  function consolePlanBoards() {
    if (!cs.loaded) {
      if (!cs.loading && !cs.error) consoleLoad();
      return panel(
        "Boards",
        cs.error
          ? note(`<strong>Read refused</strong> · ${E(consoleError(cs.error))}`, "bad")
          : note("Loading the platform's own programme…"),
      );
    }
    const project = consoleSelected();
    if (!project) return panel("Boards", note("This tenant holds no projects."));
    const select = consoleProjectSelect(project);
    const data = cs.byProject[project.id];
    if (data && data.error) {
      return `${panel("Project", select)}${panel(
        "Boards",
        note(`<strong>Read refused</strong> · ${E(consoleError(data.error))}`, "bad"),
      )}`;
    }
    if (!data) {
      consoleLoadProject(project.id);
      return `${panel("Project", select)}${panel("Boards", note("Loading this project's board…"))}`;
    }
    if (!data.board) {
      return `${panel("Project", select)}${panel(
        "Boards",
        note("No board. This project's outputs and checklist work without one; the Plan board and card intake need one."),
      )}`;
    }
    const lanes = list(data.lists);
    const cards = list(data.cards).filter((row) => !row.archived_at);
    const issues = consoleIssueRows(project);
    const intakeLane = lanes.find((row) => row.kind === "intake") || lanes[0];
    const cardFor = (issue) =>
      cards.find((card) => String(consoleIssueOf(card)) === String(issue.number));
    const status = C.status();
    const typeOptions = list(cs.types)
      .map((row) => row.code)
      .filter(Boolean)
      .sort();
    const selectedType =
      v.intakeType ||
      (typeOptions.includes("architecture_task") ? "architecture_task" : typeOptions[0]) ||
      "";
    const typeSelect = `<div class="console-actions"><label class="mono muted">Intake card type</label><select data-v2-input="intake-type" aria-label="Card type">${
      typeOptions.length
        ? typeOptions
            .map(
              (code) =>
                `<option value="${E(code)}" ${code === selectedType ? "selected" : ""}>${E(code)}</option>`,
            )
            .join("")
        : `<option value="" disabled>no card type in this tenant</option>`
    }</select></div>`;
    const operator = panel(
      "Operator",
      `<p class="muted">Signed in as <strong>${E(status.name)}</strong> · tenant <span class="mono">${E(status.tenant)}</span>. Every control on this page calls the platform; nothing is simulated.</p>${typeSelect}${button(
        "Reload board",
        "console-reload",
        `data-project="${E(project.id)}"`,
        "small",
        "replay",
      )}${
        cs.lastError
          ? note(`<strong>Last action refused</strong> · ${E(consoleError(cs.lastError))}`, "bad")
          : ""
      }${note(
        "This unit wires the card loop — intake an issue, move, comment, claim and release. Approvals, documents and evidence rows are separate units: the live tenant holds no approval rows yet, and published intermediate outputs need the document contract.",
      )}`,
    );
    const issueRows = issues.map((issue) => {
      const card = cardFor(issue);
      const lane = card ? lanes.find((row) => row.id === card.list_id) : null;
      const action =
        card == null && intakeLane && C.can("cards.create")
          ? button(
              "Intake",
              "console-intake",
              `data-project="${E(project.id)}" data-issue="${E(issue.number)}"`,
              "small",
              "plus",
            )
          : card
            ? `<span class="console-live">card</span>`
            : "—";
      return `<tr><td><a class="e-project-link" href="${E(issue.url || consoleIssueUrl(issue.number))}" target="_blank" rel="noopener">#${E(issue.number)} ↗</a> <strong>${E(issue.title)}</strong></td><td>${B(
        issue.state === "CLOSED" ? "Closed" : issue.state === "OPEN" ? "Open" : issue.state,
        issue.state === "CLOSED" ? "" : "amber",
      )}</td><td>${
        card ? `<strong>${E(card.name)}</strong><small class="mono">${E(lane ? lane.name : "")}</small>` : `<span class="muted">— not intaken</span>`
      }</td><td>${action}</td></tr>`;
    });
    const issuePanel = panel(
      `${project.key} · GitHub issues`,
      issueRows.length
        ? table(["Issue", "GitHub state", "Card", "Operate"], issueRows)
        : `<p class="muted">No GitHub issues are registered for this project.</p>`,
      B(`${issues.filter((row) => row.state === "OPEN").length} open`, "green"),
    );
    const lanePanels = lanes
      .map((lane) => {
        const laneCards = cards
          .filter((row) => row.list_id === lane.id)
          .sort((a, b) => Number(a.position) - Number(b.position));
        const rows = laneCards.map((card) => {
          const issue = consoleIssueOf(card);
          return `<tr><td><button type="button" class="console-card-link" data-v2="console-card" data-id="${E(card.id)}"><strong>${E(card.name)}</strong></button><small class="mono">${E(card.card_type_code || "")}</small></td><td>${card.assignee_id ? B("claimed", "amber") : "—"}</td><td>${
            issue != null
              ? `<a class="e-project-link" href="${E(consoleIssueUrl(issue))}" target="_blank" rel="noopener">#${E(issue)} ↗</a>`
              : "—"
          }</td></tr>`;
        });
        return panel(
          `${lane.name} · ${laneCards.length}`,
          rows.length
            ? table(["Card", "Claim", "Issue"], rows)
            : `<p class="muted">No cards in this lane.</p>`,
        );
      })
      .join("");
    return `${panel(
      "Project",
      `${select}<p class="muted">Board <span class="mono">${E(data.board.name)}</span> · ${cards.length} cards · ${lanes.length} lanes · issues joined to the platform's own cards.</p>`,
      `<span class="console-live">live</span>`,
    )}${operator}${issuePanel}${lanePanels}`;
  }

  // A sign-in/out is a different surface: reset the cache and let the page re-render.
  if (C) {
    C.onChange(() => {
      if (!C.signedIn()) consoleReset();
      else {
        cs.loaded = false;
        cs.byProject = {};
      }
      render();
    });
  }

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
    // Only files the public window actually serves. The old list named data/manifest.json,
    // data/programme.json and friends, which are 404 on the live site.
    const files = [
      ["data/snapshot.js", "the published programme snapshot"],
      ["docs/outputs.js", "the published outputs index"],
      ["docs/documents.json", "the generated document index"],
    ];
    return panel(
      "Live data &amp; feedback",
      `<p class="muted">Rendered from the conveyor's own rows. Snapshot <span class="mono">${E(short(live.payload_sha256))}</span> generated <strong>${E(live.generated_at || "")}</strong> at <span class="mono">${E(short(live.git_sha, 12))}</span>. The public data files:</p>${table(
        ["Data file"],
        files.map(
          ([file, label]) =>
            `<tr><td><a class="e-project-link" href="${E(file)}" target="_blank" rel="noopener"><span class="mono">${E(file)}</span></a> <small>${E(label)}</small></td></tr>`,
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
    procProcess: null,
    procProcedure: null,
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
    benefit: "BEN-01",
    tranche: 0,
    programmeGate: null,
    adm: "ART-001",
    admPhase: "B",
    admType: "all",
    admGate: null,
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
    /* The MSP layer: programme definition, benefit profiles, the benefits map, tranches and
       realization. These are this study's fixtures; the live surface reads and writes the
       programme store through programmes.*, benefits.*, tranches.*, stakeholders.* and
       cadences.* (SPEC_V2 §8.3). */
    programmeDefinition: {
      key: "PGM-CB",
      name: "conveyor buildout",
      sponsors: "sponsoring group · N. de Vries (chair) · G. Dekker (SRO) · R. Slot",
      mandate: { state: "Draft", approved: "—", version: "v3" },
      fields: [
        ["Strategic fit", "One operating model for every conveyor work stream; the programme contributes to the mission of a self-operating organisation."],
        ["Current position", "V1 is live; T0's organisation spine is merged; 50 projects are in the programme. The programme's definition set is at tranche 0."],
        ["Drivers", "One-off tooling cost, duplicated project machinery and unmeasured outcomes across the estate."],
        ["Deliverables and boundaries", "The organisation spine, the plan spine, the decision ladder, the tool estate and benefits realization. Out of scope: customer-facing applications."],
        ["Delivery and assurance", "Iterative delivery inside tranches; each tranche review decides continue, pause or stop on evidence."],
        ["Success and constraints", "Benefits realized against their profiles; no project starts before the first tranche is authorised; public/opt-in licensing frame."],
      ],
      artefacts: [
        { id: "PD-01", kind: "Mandate", title: "Programme mandate", producer: "programme manager", reviewer: "BCM", approver: "sponsoring group", version: "v3", state: "Draft", gate: "authorise the programme" },
        { id: "PD-02", kind: "Brief", title: "Programme brief", producer: "SRO", reviewer: "programme board", approver: "sponsoring group", version: "v2", state: "Approved", gate: "authorise definition" },
        { id: "PD-03", kind: "Blueprint", title: "Blueprint — target architecture and operating model", producer: "programme architect", reviewer: "BCM", approver: "SRO", version: "v4", state: "Approved", gate: "blueprint contract" },
        { id: "PD-04", kind: "Business case", title: "Business case — options, appraisal and cash flow", producer: "programme manager", reviewer: "BCM", approver: "SRO", version: "v3", state: "In review", gate: "investment decision" },
        { id: "PD-05", kind: "Benefits map", title: "Benefits map and profiles", producer: "BCM", reviewer: "programme manager", approver: "SRO", version: "v2", state: "Draft", gate: "profiles accepted" },
        { id: "PD-06", kind: "Projects dossier", title: "Projects dossier — outputs, resources, dependencies", producer: "programme manager", reviewer: "BCM", approver: "SRO", version: "v5", state: "Draft", gate: "first-tranche authorisation" },
        { id: "PD-07", kind: "Programme plan", title: "Programme plan and tranches", producer: "programme manager", reviewer: "BCM", approver: "SRO", version: "v6", state: "Approved", gate: "first-tranche authorisation" },
        { id: "PD-08", kind: "Governance", title: "Governance, roles and the strategy set", producer: "programme manager", reviewer: "BCM", approver: "SRO", version: "v2", state: "Approved", gate: "governance baseline" },
        { id: "PD-09", kind: "Stakeholders", title: "Stakeholder profiles", producer: "BCM", reviewer: "programme manager", approver: "SRO", version: "v2", state: "Draft", gate: "engagement plan" },
        { id: "PD-10", kind: "Realization", title: "Benefits management strategy and realization plan", producer: "programme manager", reviewer: "BCM", approver: "SRO", version: "v1", state: "Draft", gate: "transition readiness" },
      ],
      options: [
        ["Do nothing", "No programme; the estate keeps its duplicated machinery — rejected: the cost and the evidence gap continue."],
        ["Programme, all four tranches", "Recommended: definition first, then the spine, knowledge/tools, operations."],
        ["Projects only, no programme", "Cheaper to start — rejected: no shared configuration, no tranche review, no benefit owner."],
      ],
    },
    benefits: [
      { id: "BEN-01", title: "Delivery lead time", category: "efficiency", objective: "faster IT delivery", outcome: "a card reaches acceptance with less queue time", kpi: "median cycle time (days)", baseline: 14, target: 8, current: 12.1, method: "card and run timestamps, weekly", value: "€0.4m/yr avoided contractor capacity", cost: "€0.12m", owner: "BCM · delivery", area: "delivery", conditions: "jobs bound; coach enabled; WIP limits configured", deps: "T1, T4", state: "Tracking" },
      { id: "BEN-02", title: "Evidence at every gate", category: "governance", objective: "faster IT delivery; data as an asset", outcome: "a gate decision names its evidence", kpi: "gates with an evidence reference (%)", baseline: 41, target: 95, current: 78, method: "approval × evidence join, daily", value: "non-cash", cost: "€0.05m", owner: "BCM · project office", area: "project office", conditions: "approval policies configured; evidence captured at the action", deps: "T1", state: "Tracking" },
      { id: "BEN-03", title: "Zero-use tools retired", category: "cost", objective: "manageability; lower cost", outcome: "the estate carries only tools a job calls", kpi: "toolkits retired after a zero-call cycle", baseline: 0, target: 12, current: 4, method: "tool_call roll-up per programme cycle", value: "€0.2m/yr", cost: "€0.08m", owner: "BCM · operations", area: "operations", conditions: "tool ledger live; a probe per toolkit", deps: "T13, T14", state: "At risk" },
      { id: "BEN-04", title: "Review latency", category: "disbenefit", objective: "sustainable delivery", outcome: "dual control added no more than 8 h to a decision", kpi: "median gate decision latency (h)", baseline: 0, target: 8, current: 5.4, method: "approval opened → decided", value: "—", cost: "—", owner: "BCM · delivery", area: "delivery", conditions: "approver on call; a gate names its expiry", deps: "T1", state: "Monitoring", disbenefit: true },
    ],
    benefitLinks: [
      { output: "T1 plan spine", capability: "WBS, baseline and register rows", outcome: "the plan is managed and evidenced", benefit: "BEN-02", objective: "faster IT delivery" },
      { output: "T3 PM job", capability: "charter, baseline and change control", outcome: "fewer unplanned changes", benefit: "BEN-01", objective: "faster IT delivery" },
      { output: "T13 tool platform", capability: "tool call ledger + probes", outcome: "zero-use tools are visible", benefit: "BEN-03", objective: "manageability" },
      { output: "T4 kanban coach", capability: "WIP limits + one coach card", outcome: "flow is protected", benefit: "BEN-04", objective: "sustainable delivery" },
    ],
    tranches: [
      { no: 0, name: "Definition", projects: "P0, SPEC-V2, DESIGN-PUBLISH", entry: "mandate approved", exit: "definition set accepted; benefits baselined", baseline: "B0", review: "2026-12-04", state: "In review", decision: "Pending" },
      { no: 1, name: "Organisation spine", projects: "T1, T2, T3", entry: "definition accepted", exit: "PM runs a request end to end", baseline: "B1", review: "2027-01-15", state: "Planned", decision: "—" },
      { no: 2, name: "Knowledge and tools", projects: "T8, T13, T14", entry: "T1 accepted", exit: "a toolkit is probed and called; a lesson is promoted", baseline: "B2", review: "2027-02-26", state: "Planned", decision: "—" },
      { no: 3, name: "Operations", projects: "T4, T5, T7, T10", entry: "T2 accepted", exit: "one incident postmortemed; one benefit transitioned and measured", baseline: "B3", review: "2027-04-09", state: "Planned", decision: "—" },
    ],
    realization: [
      { benefit: "BEN-01", baselineMeasured: "— (planned 2026-12-08, 14 days)", readiness: "Amber · coach not yet bound", transition: "2027-01-12", post: "4 weekly measures", legacy: "manual cycle-time spreadsheet", owner: "BCM · delivery", state: "Planned" },
      { benefit: "BEN-02", baselineMeasured: "2026-12-01 · 41%", readiness: "Green", transition: "2026-12-15", post: "daily join, monthly review", legacy: "gate audit export", owner: "BCM · project office", state: "In transition" },
      { benefit: "BEN-03", baselineMeasured: "— · not measured", readiness: "Red · tool ledger not live", transition: "—", post: "—", legacy: "—", owner: "BCM · operations", state: "Blocked · baseline missing" },
      { benefit: "BEN-04", baselineMeasured: "2026-11-20 · 0 h (no dual control)", readiness: "Green", transition: "2026-12-01", post: "weekly", legacy: "single-reviewer approvals", owner: "BCM · delivery", state: "Monitoring" },
    ],
    /* The TOGAF ADM capability: the Preliminary phase (principles, tailored profile, board,
       maturity, skills), the phase rail with typed artefacts, the contracts and the compliance
       and waiver register. These are this study's fixtures; the live screen reads the architecture
       rows through architecture.* (SPEC_V2 §8.1, T9). */
    architectureCapability: {
      key: "ARCH-CB",
      name: "conveyor buildout architecture capability",
      principles: [
        { id: "P-01", statement: "One record per fact", rationale: "A second store is a second truth; the systems of record diverge silently", implications: "No derived status is stored; no shadow table; the read model computes" },
        { id: "P-02", statement: "Rows and gates, not a document library", rationale: "A document with no lifecycle has no gate and no evidence", implications: "Every artefact carries metadata, an owner, a reviewer and a gate" },
        { id: "P-03", statement: "Tenant configuration over code", rationale: "Policy values differ per tenant and change without a deploy", implications: "Thresholds, approval kinds and maturity sets are versioned tenant rows" },
        { id: "P-04", statement: "Reuse before build", rationale: "The repository already holds the building block more often than not", implications: "A solution artefact names the enterprise artefact it reuses or declares the deviation" },
        { id: "P-05", statement: "The executor cannot approve its own gate", rationale: "A gate that cannot refuse is not a gate", implications: "Requester and executor are refused as approvers; dual control defaults on" },
        { id: "P-06", statement: "A baseline is measured before it moves", rationale: "A benefit or an architecture with no baseline cannot claim an improvement", implications: "A transition or a re-baseline goes through change control, never through a review" },
      ],
      profile: {
        name: "Tailored ADM profile · software delivery",
        phases: "Preliminary + A–H",
        psa: "13 artefact types mandatory at inception",
        hld: "24 artefact types mandatory by phase C",
        sd: "18 artefact types mandatory by phase E",
        contracts: "SAW (A) · delivery contract (F) · statement of intent (completion) · request for architecture (H)",
        adopted: "2026-11-12 · Architecture Board decision BD-01",
      },
      board: {
        name: "Architecture Board",
        chair: "CIO",
        members: "enterprise architects · business-unit leaders · the programme architect",
        charter: "Support the roadmap, assess proposed changes against it, apply the principles where the roadmap is incomplete, commission solution architectures and submit them to the delivery Design Board",
        authority: "commission architecture work · approve principles and contracts · accept or reject deviations",
        limits: "cannot approve a delivery gate it requested · cannot waive a deviation it owns · no budget authority",
      },
      maturity: [
        { dimension: "Architecture process", baseline: 2, stage1: 3, stage2: 4 },
        { dimension: "Architecture development", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "Business linkage", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "Senior management involvement", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "Operating unit participation", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "Architecture communication", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "IT security", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "Architecture governance", baseline: 1, stage1: 2, stage2: 3 },
        { dimension: "IT investment and acquisition", baseline: 1, stage1: 2, stage2: 3 },
      ],
      skills: [
        { category: "General", role: "programme architect", current: 4, target: 4 },
        { category: "Business skills and methods", role: "programme architect", current: 3, target: 4 },
        { category: "Architecture skills", role: "programme architect", current: 3, target: 4 },
        { category: "Programme and project management", role: "programme manager", current: 4, target: 4 },
        { category: "General IT knowledge", role: "solution architect", current: 4, target: 4 },
        { category: "Technical IT skills", role: "solution architect", current: 4, target: 5 },
        { category: "Legal environment", role: "asset owner", current: 2, target: 3 },
      ],
      phases: [
        { code: "Preliminary", name: "Preliminary", plateau: "psa", required: 5, approved: 5, missing: [], contract: "capability adopted (BD-01)", state: "Closed" },
        { code: "A", name: "Vision", plateau: "psa", required: 6, approved: 6, missing: [], contract: "SAW-01 v2 · approved", state: "Closed" },
        { code: "B", name: "Business", plateau: "hld", required: 24, approved: 22, missing: ["ART-011 · Business Footprint Diagram", "ART-019 · Process Flow Diagram"], contract: "BC-01 · draft", state: "In progress" },
        { code: "C", name: "Information systems", plateau: "hld", required: 18, approved: 0, missing: [], contract: "IS-01 · not requested", state: "Planned" },
        { code: "D", name: "Technology", plateau: "hld", required: 9, approved: 0, missing: [], contract: "TC-01 · not requested", state: "Planned" },
        { code: "E", name: "Opportunities & solutions", plateau: "sd", required: 6, approved: 0, missing: [], contract: "SC-01 · not requested", state: "Planned" },
        { code: "F", name: "Migration planning", plateau: "sd", required: 4, approved: 0, missing: [], contract: "DC-01 · not requested", state: "Planned" },
        { code: "G", name: "Implementation governance", plateau: "sd", required: 1, approved: 0, missing: [], contract: "per-delivery compliance", state: "Planned" },
        { code: "H", name: "Change management", plateau: "sd", required: 3, approved: 0, missing: [], contract: "RAW-01 · none", state: "Planned" },
      ],
      contracts: [
        { id: "SAW-01", kind: "Statement of Architecture Work", phase: "A", version: "v2", state: "Approved", owner: "programme architect", reviewer: "architecture forum", body: "assignment, scope, responsibilities, acceptance criteria" },
        { id: "BC-01", kind: "Business contract", phase: "B", version: "v1", state: "Draft", owner: "domain architect", reviewer: "Architecture Board", body: "business architecture delivered, review method" },
        { id: "DC-01", kind: "Delivery contract", phase: "F", version: "v1", state: "Not requested", owner: "programme manager", reviewer: "asset owner", body: "delivery commitments and compliance method" },
        { id: "SI-01", kind: "Statement of intent", phase: "completion", version: "—", state: "Not requested", owner: "asset owner", reviewer: "Architecture Board", body: "intended use, ownership, measures of effectiveness" },
        { id: "RAW-01", kind: "Request for Architecture Work", phase: "H", version: "—", state: "None", owner: "business sponsor", reviewer: "Design Board", body: "further architecture work with a Lean Canvas" },
      ],
      artifacts: [
        { id: "ART-001", title: "Principles catalog", type: "catalog", phase: "Preliminary", plateau: "psa", state: "Approved", version: "v3", owner: "programme architect", reviewer: "Architecture Board", reviewed: "2026-11-12", document: "document · arch-principles-v3", entries: 6 },
        { id: "ART-002", title: "Principle derivation diagram", type: "diagram", phase: "Preliminary", plateau: "psa", state: "Approved", version: "v2", owner: "programme architect", reviewer: "Architecture Board", reviewed: "2026-11-12", document: "document · arch-principles-v3#derivation", entries: 2, viewpoint: "ArchiMate 3.1 / Motivation" },
        { id: "ART-003", title: "Stakeholder map and matrix", type: "matrix", phase: "A", plateau: "psa", state: "Approved", version: "v2", owner: "programme architect", reviewer: "SRO", reviewed: "2026-11-13", document: "document · arch-vision-v2", entries: 6 },
        { id: "ART-004", title: "Driver / goal / objective catalog", type: "catalog", phase: "A", plateau: "psa", state: "Approved", version: "v1", owner: "programme architect", reviewer: "SRO", reviewed: "2026-11-13", document: "document · arch-vision-v2#motivation", entries: 7 },
        { id: "ART-005", title: "Requirements catalog", type: "catalog", phase: "A", plateau: "psa", state: "Approved", version: "v4", owner: "requirements lead", reviewer: "Architecture Board", reviewed: "2026-11-13", document: "document · arch-requirements-v4", entries: 41 },
        { id: "ART-006", title: "Solution concept diagram", type: "diagram", phase: "A", plateau: "psa", state: "Approved", version: "v2", owner: "solution architect", reviewer: "Architecture Board", reviewed: "2026-11-13", document: "document · arch-vision-v2#concept", entries: 3, viewpoint: "ArchiMate 3.1 / Layered" },
        { id: "ART-007", title: "Benefits diagram", type: "diagram", phase: "E", plateau: "sd", state: "Draft", version: "v1", owner: "BCM", reviewer: "SRO", reviewed: "—", document: "document · arch-benefits-draft", entries: 4, viewpoint: "ArchiMate 3.1 / Layered" },
        { id: "ART-008", title: "Business service / function catalog", type: "catalog", phase: "B", plateau: "hld", state: "Approved", version: "v2", owner: "domain architect", reviewer: "SRO", reviewed: "2026-11-15", document: "document · arch-business-v2", entries: 18 },
        { id: "ART-009", title: "Process / event / control / product catalog", type: "catalog", phase: "B", plateau: "hld", state: "Approved", version: "v1", owner: "domain architect", reviewer: "SRO", reviewed: "2026-11-15", document: "document · arch-business-v2#process", entries: 26 },
        { id: "ART-010", title: "Actor / role matrix", type: "matrix", phase: "B", plateau: "hld", state: "Approved", version: "v1", owner: "domain architect", reviewer: "SRO", reviewed: "2026-11-15", document: "document · arch-business-v2#actors", entries: 12 },
        { id: "ART-011", title: "Business footprint diagram", type: "diagram", phase: "B", plateau: "hld", state: "Draft", version: "v1", owner: "domain architect", reviewer: "SRO", reviewed: "—", document: "document · arch-business-draft", entries: 5, viewpoint: "ArchiMate 3.1 / Layered" },
        { id: "ART-012", title: "Data entity / data component catalog", type: "catalog", phase: "C", plateau: "hld", state: "Approved", version: "v2", owner: "data architect", reviewer: "Architecture Board", reviewed: "2026-11-15", document: "document · arch-data-v2", entries: 31 },
        { id: "ART-013", title: "Application portfolio catalog", type: "catalog", phase: "C", plateau: "hld", state: "Approved", version: "v2", owner: "application architect", reviewer: "Architecture Board", reviewed: "2026-11-15", document: "document · arch-application-v2", entries: 24 },
        { id: "ART-014", title: "Application / data matrix", type: "matrix", phase: "C", plateau: "hld", state: "Draft", version: "v1", owner: "application architect", reviewer: "Architecture Board", reviewed: "—", document: "document · arch-application-draft", entries: 20 },
        { id: "ART-015", title: "Technology standards catalog", type: "catalog", phase: "D", plateau: "hld", state: "Approved", version: "v2", owner: "platform architect", reviewer: "Architecture Board", reviewed: "2026-11-15", document: "document · arch-technology-v2", entries: 15 },
        { id: "ART-016", title: "Application communication diagram", type: "diagram", phase: "C", plateau: "sd", state: "Planned", version: "—", owner: "application architect", reviewer: "Architecture Board", reviewed: "—", document: "—", entries: 0, viewpoint: "ArchiMate 3.1 / Layered" },
        { id: "ART-017", title: "Project context diagram", type: "diagram", phase: "E", plateau: "sd", state: "Planned", version: "—", owner: "solution architect", reviewer: "Design Board", reviewed: "—", document: "—", entries: 0, viewpoint: "ArchiMate 3.1 / Layered" },
        { id: "ART-018", title: "Technology portfolio catalog", type: "catalog", phase: "D", plateau: "sd", state: "Planned", version: "—", owner: "platform architect", reviewer: "Architecture Board", reviewed: "—", document: "—", entries: 0 },
        { id: "ART-019", title: "Process flow diagram", type: "diagram", phase: "B", plateau: "hld", state: "Draft", version: "v1", owner: "domain architect", reviewer: "SRO", reviewed: "—", document: "document · arch-business-draft#flow", entries: 7, viewpoint: "ArchiMate 3.1 / Process" },
      ],
      artifactEntries: {
        "ART-003": {
          caption: "Stakeholder map · power × interest",
          headers: ["Stakeholder", "Power", "Interest", "Engagement"],
          rows: [
            ["Executive leadership", "high", "low", "keep satisfied · mandate and funding"],
            ["Business unit leaders", "high", "high", "manage closely · operating-practice change"],
            ["Asset owners", "high", "high", "manage closely · waiver acceptance"],
            ["IT management", "medium", "high", "manage closely · delivery interface"],
            ["Project managers", "low", "high", "keep informed · phase inputs"],
            ["Operations / DevOps", "low", "high", "keep informed · run evidence"],
          ],
        },
        "ART-005": {
          caption: "Requirements catalog · condition / constraint / assumption / delta",
          headers: ["UID", "Type", "Requirement", "Category", "Priority", "Source", "Owner"],
          rows: [
            ["REQ-011", "condition", "A gate decision names its evidence", "governance", "must", "SRO", "programme board"],
            ["REQ-012", "constraint", "No second store for any fact", "data", "must", "CIO", "architecture board"],
            ["REQ-013", "assumption", "The buildout runs on one tenant per industry", "scope", "should", "programme manager", "SRO"],
            ["REQ-014", "delta", "Phase B artefacts replace the V1 rail map", "business", "could", "domain architect", "SRO"],
          ],
        },
        "ART-010": {
          caption: "Actor / role matrix",
          headers: ["Actor", "Role", "Responsibility"],
          rows: [
            ["programme manager", "accountable", "owns the tranche plan and the gate route"],
            ["programme architect", "responsible", "authors the artefacts and the contracts"],
            ["BCM", "consulted", "owns the benefit profiles and the transitions"],
            ["SRO", "accountable", "decides the tranche and the design gate"],
          ],
        },
      },
      artifactLinks: [
        { from: "ART-001", to: "ART-002", kind: "derivation" },
        { from: "ART-003", to: "ART-002", kind: "derivation" },
        { from: "ART-004", to: "ART-002", kind: "derivation" },
        { from: "ART-002", to: "ART-005", kind: "derivation" },
        { from: "ART-005", to: "ART-006", kind: "realization" },
        { from: "ART-004", to: "ART-008", kind: "derivation" },
        { from: "ART-008", to: "ART-011", kind: "derivation" },
        { from: "ART-012", to: "ART-014", kind: "dependency" },
        { from: "ART-013", to: "ART-014", kind: "dependency" },
        { from: "ART-013", to: "ART-016", kind: "derivation" },
        { from: "ART-013", to: "ART-017", kind: "reuse" },
        { from: "ART-015", to: "ART-018", kind: "derivation" },
        { from: "ART-004", to: "ART-007", kind: "derivation" },
      ],
      boardDecisions: [
        { id: "BD-01", decision: "Adopt the tailored ADM profile", evidence: "profile v1 · phased A–H · contract kinds", state: "Approved" },
        { id: "BD-02", decision: "Approve principles catalog v3", evidence: "P-01…P-06 with rationale and implications", state: "Approved" },
        { id: "BD-03", decision: "Commission the T9 solution architecture", evidence: "SAW-01 · scope and acceptance", state: "Approved" },
        { id: "BD-04", decision: "Accept waiver W-01 for T13", evidence: "asset owner acceptance · review 2027-01-15", state: "Approved" },
      ],
      compliance: [
        { project: "T1 plan spine", contract: "DC-01", level: "Consistent", findings: "two SD artefacts deferred; no deviation from the data model", date: "2026-11-14" },
        { project: "T9 ADM capability", contract: "SAW-01", level: "Compliant", findings: "phase A artefacts approved; the IS contract is not yet requested", date: "2026-11-15" },
        { project: "T13 tool platform", contract: "DC-02", level: "Non-conformant", findings: "the tool_call row omits the credential alias", date: "2026-11-15" },
      ],
      waivers: [
        { id: "W-01", project: "T13 tool platform", deviation: "credential alias recorded in the broker log, not the tool_call row", owner: "asset owner · operations", mitigation: "broker-log join until T13b", costRisk: "€0.1m · low", review: "2027-01-15", state: "Active" },
      ],
      hOutcomes: [
        { id: "H-01", finding: "two competing identity contracts in the estate", outcome: "Change request CR-019", state: "Routed to the change gate" },
        { id: "H-02", finding: "a gate never refused in a full cycle", outcome: "Kata candidate K-014", state: "Routed to the kata lane" },
      ],
    },
    changes: [
      { id: "CR-018", title: "Add delegated identity scope", baseline: "B3", impact: "scope +2 deliverables · schedule +9 d · cost +€0.8m · risk medium", rationale: "", state: "Draft" },
      { id: "CR-017", title: "Move portal gate to PI-2026.4", baseline: "B2", impact: "schedule −14 d · cost −€0.2m · risk low", rationale: "Programme board rationale recorded", state: "Approved" },
    ],
    /* Stakeholder profiles in the shape the live store uses (`stakeholders.profile_list`:
       name, importance, influence, interest, support_level, support_trend, engagement_target,
       concerns, sensitivities, disbenefits); organisation and role are study-only columns. */
    stakeholders: [
      { id: "SH-01", name: "N. de Vries", organisation: "sponsoring group", role: "programme sponsor · chair", importance: "High", influence: "High · funds the tranche and decides continue, pause or stop", interest: "High · outcome and cost", support_level: "supportive", support_trend: "stable", engagement_target: "manage closely · every tranche review and mandate gate", concerns: "benefit profiles claim outcomes no baseline has measured yet", sensitivities: "public commitments", disbenefits: "sponsor attention is finite; a competing programme is paused" },
      { id: "SH-02", name: "G. Dekker", organisation: "asset owners", role: "SRO · asset owner, operations", importance: "High", influence: "High · accepts waivers, operational risk and cost", interest: "High · run cost and reliability", support_level: "supportive", support_trend: "improving", engagement_target: "manage closely · every design gate and waiver", concerns: "a waiver without a reassessment date becomes permanent", sensitivities: "outage window", disbenefits: "extra maintenance load on the operations job" },
      { id: "SH-03", name: "R. Slot", organisation: "delivery", role: "programme manager", importance: "Medium", influence: "Medium · owns the WBS, the cadence and the gate route", interest: "High · schedule and dependencies", support_level: "neutral", support_trend: "stable", engagement_target: "keep informed · weekly plan review", concerns: "a re-baseline after every forecast variance", sensitivities: "schedule slip", disbenefits: "rework when a baseline moves" },
    ],
    sops: [
      { id: "SOP-01", name: "Tenant onboarding SOP", source: "PDF · tenant-supplied", pages: 18, state: "Draft · PDDL", provenance: "untrusted input · governed_asset(sop_source)" },
      { id: "SOP-02", name: "Release verification SOP", source: "URL · internal wiki", pages: 9, state: "Review", provenance: "untrusted input · provenance recorded" },
      { id: "SOP-03", name: "Incident response SOP", source: "Document · batch (3)", pages: 24, state: "Published", provenance: "reviewed · procedure v2 · pack capability@17" },
    ],
    intakes: [
      { id: "IN-31", kind: "Chat", text: "We need a small IT delivery department with PM, backend and verification jobs.", produces: ["organisation draft", "job drafts", "project template"], state: "Drafted" },
      { id: "IN-32", kind: "Document", text: "Tenant SOP bundle (3 files)", produces: ["capability", "procedure", "knowledge pack"], state: "In review" },
      { id: "IN-33", kind: "Chat", text: "Add a tool request for a missing billing read operation.", produces: ["tool request", "tool draft"], state: "Blocked on dependency" },
    ],
    drafts: [
      { id: "AD-11", asset: "Department: delivery", depends: "industry software", state: "Draft", review: "human" },
      { id: "AD-12", asset: "Job: backend engineer", depends: "department delivery · capability delivery", state: "Draft", review: "human" },
      { id: "AD-13", asset: "Procedure: implement change", depends: "job draft · PDDL terms", state: "Review", review: "human" },
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
      // The enterprise pages share `db`; they select a programme by id and must see the
      // live rows (T0 publish surfaced the blank default route: the fixture id "civic"
      // matched no live project and `project()` was undefined).
      db.programmes = F.programmes.map((row) => ({ ...row, horizon: "—" }));
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
      // The real plan progress is GitHub's: the linked issues' closed/total. Cards are the
      // execution handle, not the plan; a snapshot with no cards does not erase the issues.
      const registry = list((prog.issue_registry || {}).issues);
      const issueByNumber = new Map(registry.map((row) => [String(row.number), row]));
      const issueProgress = (row) => {
        const linked = list((row.meta || {}).issues);
        if (!linked.length) return null;
        const closed = linked.filter(
          (number) => (issueByNumber.get(String(number)) || {}).state === "CLOSED",
        ).length;
        return { completed: closed, total: linked.length };
      };
      const doneLists = new Set(workLists.filter((row) => row.kind === "done").map((row) => row.id));
      const countsFor = (projectId) => {
        const board = workBoards.find((row) => row.project_id === projectId);
        if (!board) return { completed: 0, total: 0 };
        const mine = workCards.filter((row) => row.board_id === board.id && !row.archived_at);
        return { completed: mine.filter((row) => doneLists.has(row.list_id)).length, total: mine.length };
      };
      db.projects = projects.map((row) => {
        const progress = countsFor(row.id);
        const issues = issueProgress(row);
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
          completed: issues ? issues.completed : progress.completed,
          total: issues ? issues.total : progress.total,
          cards: progress.total,
          issues: issues ? issues.total : 0,
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
  function executionStudy() {
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
  /* -------------------------------------------------------------- execution room (live) */
  function runDetailPanel(id) {
    const detail = cs.runDetail[id];
    if (!detail) {
      return panel(
        `Run ${short(id, 8)}`,
        note(cs.runDetailLoading === id ? "Loading the run's trace…" : "Select a run."),
      );
    }
    if (detail.error) {
      return panel(
        `Run ${short(id, 8)}`,
        note(`<strong>Read refused</strong> · ${E(consoleError(detail.error))}`, "bad"),
      );
    }
    const run = detail.run || {};
    const steps = list(detail.steps);
    const approvals = list(detail.approvals);
    const stepRows = steps.map(
      (step) =>
        `<tr><td class="mono">${E(step.position)}.${E(step.attempt || 1)}</td><td class="mono">${E(step.action)}</td><td>${B(step.status, step.status === "succeeded" ? "green" : step.status === "failed" ? "bad" : "amber")}</td><td>${outputCell(step.output)}</td><td><small>${E(step.error ? short(step.error, 70) : step.judge ? "judged" : "—")}</small></td></tr>`,
    );
    const approvalRows = approvals.map(
      (approval) =>
        `<tr><td class="mono">${E(approval.gate)}</td><td>${B(approval.status, approval.status === "pending" ? "amber" : approval.status === "approved" ? "green" : "bad")}</td><td>${E(approval.required_decisions)}</td><td>${
          approval.status === "pending" && C.can("approvals.decide")
            ? `<div class="console-actions">${button("Approve", "approval-approve", `data-id="${E(approval.id)}"`, "small", "shield")}${button("Reject", "approval-reject", `data-id="${E(approval.id)}"`, "small", "archive")}</div>`
            : "—"
        }</td></tr>`,
    );
    const summary = kv([
      ["Status", B(run.status || "—", runTone(run.status))],
      ["Project", E(projectKeyOf(run.project_id))],
      ["Card", run.card_id && cardOf(run.card_id) ? E(cardOf(run.card_id).name) : E(run.card_id || "—")],
      ["Process", detail.process ? `${E(detail.process.title)} · v${E(detail.process.version)}` : "—"],
      ["Attempt", E(run.attempt || 1)],
      ["Started", E(short(runWhen(run), 19))],
      ["Finished", E(short(run.finished_at || "—", 19))],
      ["Error", run.error_message ? `${E(run.error_class || "error")} · ${E(short(run.error_message, 120))}` : "—"],
    ]);
    const children = list((detail.tree || {}).children);
    return panel(
      `Run ${short(id, 8)} · trace`,
      `${summary}${
        steps.length
          ? table(["#", "Action", "Status", "Output", "Judge / error"], stepRows)
          : note("No step has been recorded yet. A run with no step is queued or failed before dispatch.")
      }${
        approvals.length
          ? `<h3>Gates</h3>${table(["Gate", "State", "Quorum", "Decide"], approvalRows)}`
          : ""
      }${
        children.length
          ? `<h3>Child runs</h3>${table(
              ["Run", "Status", "Depth"],
              children.map(
                (child) =>
                  `<tr><td><button type="button" class="console-card-link" data-v2="run-open" data-id="${E(child.id)}">${E(short(child.id, 8))}</button></td><td>${B(child.status, runTone(child.status))}</td><td>${E(child.depth)}</td></tr>`,
              ),
            )}`
          : ""
      }${note(`Inputs: <span class="mono">${E(short(JSON.stringify(detail.inputs || {}), 300))}</span>`)}<div class="console-actions">${
        runLive(run.status) && C.can("runs.cancel")
          ? button("Cancel run", "run-cancel", `data-id="${E(id)}"`, "small", "replay")
          : ""
      }${button("Refresh", "run-refresh", `data-id="${E(id)}"`, "small", "replay")}</div>`,
      B(run.status || "—", runTone(run.status)),
    );
  }

  function consoleExecutionPage() {
    const signed = consoleOn();
    if (signed && !cs.runsLoaded && !cs.runsLoading) consoleLoadRuns();
    const tab = v.tab.execution || "All";
    const filter =
      {
        All: null,
        Running: "running",
        "Waiting approval": "waiting_approval",
        Failed: "failed",
        Succeeded: "succeeded",
        Queued: "queued",
      }[tab] || null;
    const runs = signed ? cs.runs : list(((live || {}).work || {}).runs);
    const shown = runs.filter((run) => (filter ? run.status === filter : true));
    const rows = shown.map((run) => {
      const card = run.card_id ? cardOf(run.card_id) : null;
      return `<tr><td><button type="button" class="console-card-link" data-v2="run-open" data-id="${E(run.id)}">${E(short(run.id, 8))}</button></td><td>${E(projectKeyOf(run.project_id))}</td><td>${card ? E(card.name) : "—"}</td><td>${B(run.status, runTone(run.status))}</td><td>${E(run.attempt || 1)}</td><td><small>${E(short(runWhen(run), 19))}</small></td><td>${run.error_message ? E(short(run.error_message, 60)) : "—"}</td></tr>`;
    });
    const queue = panel(
      "Runs",
      `${
        rows.length
          ? table(["Run", "Project", "Card", "Status", "Attempt", "Started", "Error"], rows)
          : note(
              signed
                ? "No run matches this filter. Start one from a card on Projects or Plan."
                : "The published snapshot carries no run yet. Sign in at the app origin to see the tenant's runs.",
            )
      }${signed && !cs.runsLoaded ? note("Loading the tenant's runs…") : ""}`,
      signed ? button("Refresh", "run-refresh", "", "small", "replay") : "",
    );
    const detail = signed && v.runId ? runDetailPanel(v.runId) : "";
    return `<div class="page enterprise">${heading(
      "Execution room",
      "Every run the programme has started: its steps, the tools it called, the gates it waits on and the outputs it produced.",
      `${route("Projects", "projects", "", "projects")}${route("Plan & WBS", "plan", "", "board")}`,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon(
      signed
        ? "Runs, steps and gates read from the platform's own rows. A run starts from a card; every control is an action."
        : "The runs the published snapshot carries, read-only.",
      signed || !!live,
    )}${signed ? tabs(["All", "Running", "Waiting approval", "Failed", "Succeeded", "Queued"], tab, "execution") : ""}${queue}${detail}</div>`;
  }

  // Signed in, or published with a snapshot, the room is real; offline it stays the design
  // study, clearly labelled PROPOSED.
  function executionPage() {
    if (consoleOn() || live) return consoleExecutionPage();
    if (studyOn()) return executionStudy();
    return `<div class="page enterprise">${heading("Execution room", "Every run the programme has started: its steps, tools, gates and outputs.", "", "PORTFOLIO & PLAN")}${note("No published snapshot is loaded. The execution room is live-only; sign in at the app origin to read the tenant's runs.")}</div>`;
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
  // The public site carries the programme. The V2 design destinations are the operator
  // console's; they are not announced signed-out and a direct hash gets a sign-in gate. The
  // destinations that can read a published snapshot (`programme`, `adm`) stay in this set so a
  // direct hash still renders, and they are announced only when the snapshot carries their rows.
  // #470: `live` alone is not that test — an empty or partial snapshot (the homepage/no-config
  // shape) must not announce `programme`; a published definition must.
  const PUBLIC_PAGES = new Set([
    "portfolio",
    "projects",
    "plan",
    "validation",
    "procedures",
    "execution",
    "outputs",
    "documents",
    "contribute",
    "architecture",
    "programme",
    "adm",
  ]);

  function publicSidebar() {
    const projectCount = live ? snapshotProjects().length : 0;
    return `<div class="brand"><span class="brand-mark">${I("layers")}</span><div><div class="brand-word">conveyor</div><div class="brand-sub">THE OPERATING SYSTEM FOR WORK</div></div></div><div class="tenant-switch"><span class="tenant-mark">N</span><span class="grow">Public programme<small>conveyor.nonagentic.ai/design</small></span></div><nav>
      <div class="nav-group"><div class="nav-label">PROGRAMME</div>${N("portfolio", "Portfolio", "layers")}${live && list((live.programme || {}).programme).length ? N("programme", "Programme definition", "shield") : ""}${N("projects", "Projects", "projects", projectCount)}${N("outputs", "Outputs", "file")}${N("plan", "Plan &amp; WBS", "board")}${N("validation", "Validation", "check")}${N("procedures", "Procedures", "flow")}${N("execution", "Execution room", "run")}${N("documents", "Documents", "file")}${N("contribute", "Contribute", "grid")}${N("architecture", "System architecture", "model")}</div></nav><div class="sidebar-bottom"><b>Position → release → binding → run</b><div class="row"><i class="dot green"></i>${live ? "LIVE programme" : "No snapshot loaded"}</div></div>`;
  }

  function sidebar() {
    if (!studyOn()) return publicSidebar();
    const projectCount =
      consoleOn() && cs.projects.length
        ? cs.projects.length
        : live
          ? snapshotProjects().length
          : db.projects.length;
    return `<div class="brand"><span class="brand-mark">${I("layers")}</span><div><div class="brand-word">conveyor</div><div class="brand-sub">THE OPERATING SYSTEM FOR WORK</div></div></div><div class="tenant-switch"><span class="tenant-mark">N</span><span class="grow">Non-Agentic AI<small>Organisation workspace</small></span><span>⌄</span></div><nav>
      <div class="nav-group"><div class="nav-label">PORTFOLIO &amp; PLAN</div>${N("overview", "Programme control", "grid")}${N("portfolio", "Portfolio", "layers")}${N("programme", "Programme definition", "shield")}${N("projects", "Projects", "projects", projectCount)}${N("outputs", "Outputs", "file")}${N("contribute", "Contribute", "grid")}${N("plan", "Plan & WBS", "board")}${N("validation", "Validation", "check")}${N("execution", "Execution room", "run")}${N("meetings", "Meetings", "inbox")}${N("decisions", "Decision inbox", "inbox", pending())}${N("runs", "Run history", "clock")}</div>
      <div class="nav-group"><div class="nav-label">ORGANISATION</div>${N("organisation", "Organisation", "layers")}${N("jobs", "Jobs", "board", F.jobs.length)}${N("agents", "Agents", "agent")}${N("toolkits", "Tools", "tools")}${N("types", "Project types", "grid")}</div>
      <div class="nav-group"><div class="nav-label">PROCEDURES &amp; AUTHORING</div>${N("procedures", "Procedures", "flow")}${N("capabilities", "Capabilities", "layers")}${N("authoring", "Authoring", "book")}</div>
      <div class="nav-group"><div class="nav-label">GOVERN &amp; DESIGN</div>${N("assurance", "Assurance &amp; baselines", "shield")}${N("constraints", "Flow &amp; constraints", "branch")}${N("requirements", "Requirements", "file")}${N("principles", "Principles", "book")}${N("models", "Architecture &amp; UML", "model")}${N("adm", "Architecture capability", "shield")}${N("architecture", "System architecture", "model")}${N("documents", "Documents", "file")}</div>
      <div class="nav-group"><div class="nav-label">KNOWLEDGE &amp; MEMORY</div>${N("knowledge-design", "Knowledge design", "branch")}${N("knowledge", "Sources &amp; retrieval", "book")}${N("memory", "Memory explorer", "layers")}</div>
      <div class="nav-group"><div class="nav-label">MEASURE &amp; IMPROVE</div>${N("improve", "Improve", "chart")}${N("evals", "Evaluation studio", "shield")}${N("hillclimb", "Hillclimb lab", "branch")}${N("benchmarks", "Benchmark harness", "code")}${N("usage", "Inference &amp; prompt audit", "chart")}${N("metering", "Metering", "chart")}</div>
      <div class="nav-group"><div class="nav-label">OPERATIONS</div>${N("operations", "Operations", "run")}</div></nav><div class="sidebar-bottom"><b>Position → release → binding → run</b><div class="row"><i class="dot green"></i>SPEC_V2 design study</div></div>`;
  }

  /* ------------------------------------------------------------------ Portfolio */
  // Signed in, the portfolio is the tenant's own programme read from the API. The public pack
  // keeps the snapshot's real rows through the live override below; a clean checkout keeps the
  // design study. The signed-in page never renders a fixture project or a fixture programme.
  //
  // The programme views beside the tabs — the ranked WSJF backlog, the cadence, lane capacity
  // and per-project metering — read rows this pack already loads: signed in the console's own
  // reads (`msp()`, `cs`), published the snapshot's (`live.msp`, `live.work`, `live.measure`).
  // An absent source is one honest sentence; no fixture renders on either path and no cost or
  // capacity figure is invented.
  const countText = (value) => Number(value || 0).toLocaleString("en-US");
  const portfolioWbs = () =>
    consoleOn() ? list(msp().wbs) : list((((live || {}).msp || {}).wbs_nodes));
  const portfolioCadences = () =>
    consoleOn() ? list(msp().cadences) : list((((live || {}).msp || {}).cadences));
  // `programme_cadence` carries no date column; a tenant-recorded review/next date rides in meta.
  const cadenceWhen = (row) => {
    const meta = (row || {}).meta || {};
    return meta.review_at || meta.next_at || "";
  };
  const projectIdKey = (id) => (id ? projectKeyOf(id) : "—");

  function portfolioBacklogPanel() {
    const signed = consoleOn();
    const ranked = portfolioWbs()
      .map((node) => ({
        node,
        value: Number(node.business_value),
        criticality: Number(node.time_criticality),
        risk: Number(node.risk_reduction),
        size: Number(node.job_size),
      }))
      .filter(
        (row) =>
          [row.value, row.criticality, row.risk, row.size].every(Number.isFinite) &&
          row.size > 0,
      )
      .map((row) => ({
        node: row.node,
        score: wsjf({
          value: row.value,
          criticality: row.criticality,
          risk: row.risk,
          size: row.size,
        }),
      }))
      .sort((a, b) => Number(b.score) - Number(a.score));
    const body = ranked.length
      ? `${table(
          ["#", "Item", "Project", "WSJF", "Status"],
          ranked.map(
            (row, index) =>
              `<tr><td>${index + 1}</td><td><strong>${E(row.node.title || "—")}</strong><small>${E(row.node.kind || "wbs node")}</small></td><td class="mono">${E(projectIdKey(row.node.project_id))}</td><td><strong>${E(row.score)}</strong></td><td>${B(row.node.status || "open")}</td></tr>`,
          ),
        )}${note("WSJF is arithmetic, not opinion: (business value + time criticality + risk reduction) ÷ job size, computed on read from the WBS node's four typed inputs and never stored.")}`
      : note(
          signed
            ? "No WBS node carries all four WSJF inputs yet, so there is no ranked backlog to show. WSJF is computed on read from a node's business value, time criticality, risk reduction and job size; a node with job size 0 cannot score."
            : "The published snapshot carries no WBS node with the four WSJF inputs, so there is no ranked backlog to show. WSJF would be computed on read from the snapshot's wbs_node rows; no score is invented here.",
        );
    return panel(
      "WSJF backlog",
      body,
      ranked.length ? B(`${ranked.length} scored`, "green") : "",
    );
  }

  function portfolioCadencePanel() {
    const signed = consoleOn();
    const cadences = portfolioCadences();
    const rows = cadences.map(
      (row) =>
        `<tr><td>${B(row.kind || "—", row.kind === "pi" ? "green" : "")}</td><td>${E(row.period || "—")}</td><td>${B(row.state || "—", ["active", "running"].includes(row.state) ? "green" : "")}</td><td><small>${E(short(cadenceWhen(row), 19) || "—")}</small></td></tr>`,
    );
    return panel(
      "Cadence",
      rows.length
        ? `${table(["Kind", "Period", "State", "Next / review"], rows)}${note("The programme's cadence rows: PI planning, sprint, demo, retro and refinement. A date appears only when the row's metadata carries one.")}`
        : note(
            signed
              ? "No cadence row is recorded for this programme, so no PI, sprint, demo, retro or refinement rhythm is shown. A cadence is a programme_cadence row read by cadences.list."
              : "The published snapshot carries no programme cadence row, so no PI, sprint, demo, retro or refinement rhythm is shown.",
          ),
      rows.length ? B(`${rows.length} cadence${rows.length === 1 ? "" : "s"}`, "green") : "",
    );
  }

  function portfolioCapacityPanel() {
    const signed = consoleOn();
    const lanes = [];
    if (signed) {
      consoleProjects().forEach((project) => {
        const data = cs.byProject[project.id];
        if (!data || data.error || !data.lists) return;
        const liveCards = new Map();
        list(data.cards).forEach((card) => {
          if (card.archived_at) return;
          liveCards.set(card.list_id, (liveCards.get(card.list_id) || 0) + 1);
        });
        list(data.lists).forEach((lane) => {
          if (lane.wip_limit == null) return;
          lanes.push({ project: project.key, lane, onLane: liveCards.get(lane.id) || 0 });
        });
      });
    } else {
      const work = (live || {}).work || {};
      const boardProject = new Map(list(work.boards).map((row) => [row.id, row.project_id]));
      const liveCards = new Map();
      list(work.cards).forEach((card) => {
        if (card.archived_at) return;
        liveCards.set(card.list_id, (liveCards.get(card.list_id) || 0) + 1);
      });
      list(work.lists).forEach((lane) => {
        if (lane.wip_limit == null) return;
        lanes.push({
          project: projectIdKey(boardProject.get(lane.board_id)),
          lane,
          onLane: liveCards.get(lane.id) || 0,
        });
      });
    }
    const rows = lanes.map((row) => {
      const headroom = row.lane.wip_limit - row.onLane;
      return `<tr><td class="mono">${E(row.project)}</td><td>${E(row.lane.name || "—")}<small>${E(row.lane.kind || "lane")}</small></td><td>${E(row.lane.wip_limit)}</td><td>${E(row.onLane)}</td><td>${B(`${headroom} ${headroom < 0 ? "over cap" : "free"}`, headroom < 0 ? "amber" : "green")}</td></tr>`;
    });
    return panel(
      "Capacity",
      rows.length
        ? `${table(["Project", "Lane", "WIP cap", "Live cards", "Headroom"], rows)}${note("A cap is the lane's own soft WIP limit (list.wip_limit) and a live card is a card on that lane that is not archived. This is lane capacity, not machine or model capacity.")}`
        : note(
            signed
              ? "No lane on the boards loaded in this session carries a WIP cap, so no capacity figure is shown. Capacity reads each lane's list.wip_limit — open a project or load all boards from In flight — and run concurrency from runs.list; no percentage is invented."
              : "No published lane carries a WIP cap, so no capacity figure is shown. The snapshot's work.lists carry list.wip_limit and its work.runs carry the runs; neither is reported here as a percentage.",
          ),
      rows.length ? B(`${rows.length} capped lane${rows.length === 1 ? "" : "s"}`, "green") : "",
    );
  }

  function portfolioMeteringPanel() {
    const signed = consoleOn();
    const runs = signed ? list(cs.runs) : list((((live || {}).work || {}).runs));
    const groups = new Map();
    runs.forEach((run) => {
      const key = run.project_id || "";
      const entry = groups.get(key) || { key, runs: 0, active: 0, succeeded: 0, failed: 0 };
      entry.runs += 1;
      if (["queued", "running", "waiting_approval"].includes(run.status)) entry.active += 1;
      else if (run.status === "succeeded") entry.succeeded += 1;
      else if (run.status === "failed") entry.failed += 1;
      groups.set(key, entry);
    });
    const rows = [...groups.values()]
      .sort((a, b) => b.runs - a.runs)
      .map(
        (row) =>
          `<tr><td class="mono">${E(projectIdKey(row.key))}</td><td>${E(countText(row.runs))}</td><td>${E(countText(row.active))}</td><td>${E(countText(row.succeeded))}</td><td>${E(countText(row.failed))}</td></tr>`,
      );
    // Cost is never recomputed here: the console does not fetch the project-level roll-up and the
    // snapshot aggregates by caller and model only, so the panel names that source and stops.
    const spendNote = signed
      ? "Run counts are the runs.list page this console loaded. Cost and tool spend are read from metrics.usage at level=project; this page does not fetch that roll-up, so no cost is shown or estimated."
      : "Run counts are the published snapshot's work.runs. Its measure.inference section aggregates calls and tokens by caller and model, not by project, so no per-project cost or tool spend is shown or estimated.";
    return panel(
      "Metering by project",
      rows.length
        ? `${table(["Project", "Runs", "Active", "Succeeded", "Failed"], rows)}${note(spendNote)}`
        : note(
            signed
              ? `${cs.runsLoaded ? "The tenant has no run row yet." : "The tenant's run rows have not loaded in this session."} ${spendNote}`
              : `The published snapshot carries no run row. ${spendNote}`,
          ),
      rows.length ? B(`${countText(runs.length)} runs`) : "",
    );
  }

  function consolePortfolio() {
    if (!cs.loaded && !cs.loading && !cs.error) consoleLoad();
    consoleLoadRegister();
    consoleLoadMsp();
    const rows = consoleProjects();
    const index = new Map(cs.issues.map((row) => [String(row.number), row]));
    // No programme row is a real answer for a tenant: render the absence. Naming a fixture
    // programme here is how the page would keep looking live after the row was gone.
    const programme = cs.programme || null;
    // The operator console opens on the in-flight queue it acts from; a programme drill
    // (`#portfolio/<id>`) is a timeline route and opens there directly. The public page renders
    // the timeline above its queue, so the visual is not console-only.
    const tab = ["Timeline", "In flight", "Dependencies", "Programme", "Backlog"].includes(
      v.tab.portfolio,
    )
      ? v.tab.portfolio
      : routeUuid(1)
        ? "Timeline"
        : "In flight";
    const statusTone = (status) =>
      status === "active"
        ? "green"
        : status === "closed" || status === "archived"
          ? ""
          : "amber";
    const unitStats = (project) => {
      const linked = list((project.meta || {}).issues);
      const issues = linked.map((number) => index.get(String(number))).filter(Boolean);
      return {
        total: linked.length,
        open: issues.filter((row) => (row.state || "OPEN") === "OPEN").length,
        ready: issues.filter((row) => issueLabels(row).includes("status:ready")).length,
        claimed: issues.filter((row) => issueLabels(row).includes("status:claimed")).length,
        blocked: issues.filter((row) => issueLabels(row).includes("status:blocked")).length,
      };
    };
    const cardStats = (project) => {
      const data = cs.byProject[project.id];
      if (!data || data.error || !data.cards) return null;
      const kinds = new Map(list(data.lists).map((row) => [row.id, row.kind]));
      const mine = list(data.cards).filter((row) => !row.archived_at);
      return {
        done: mine.filter((row) => ["done", "archive"].includes(kinds.get(row.list_id))).length,
        total: mine.length,
      };
    };
    const inFlight = rows.map((project) => {
      const units = unitStats(project);
      const cards = cardStats(project);
      const plan = planHref((project.meta || {}).plan);
      return `<tr><td><strong>${E(project.name)}</strong><small>${E(project.key)} · ${E(
        (project.meta || {}).track || project.project_type_code || "",
      )}</small></td><td>${B(project.status || "draft", statusTone(project.status))}</td><td><small>${
        units.open
      } open / ${units.total} linked</small></td><td>${
        cards ? `${cards.done} / ${cards.total}` : `<span class="muted">open a project</span>`
      }</td><td>${B(
        units.blocked ? units.blocked + " blocked" : "Clear",
        units.blocked ? "amber" : "green",
      )}</td><td>${
        plan ? `<a class="e-project-link" href="${E(plan)}">plan ↗</a>` : "—"
      }</td></tr>`;
    });
    // Typed edges are the T1 aggregate (`project_dependencies`). The seed's `meta.depends_on`
    // keys remain visible as a legacy projection when no typed edge has been recorded.
    const typed = list(msp().dependencies);
    const deps = typed.map(
      (d) => `<tr><td class="mono">${E(mspProjectName(d.from_project))}</td><td>→</td><td class="mono">${E(mspProjectName(d.to_project))}</td><td>${B(d.kind || "—")}</td><td>${E(d.status || "—")}</td><td>${button("Delete", "msp-delete", `data-kind="dependency" data-id="${E(d.id)}"`, "small", "close")}</td></tr>`,
    );
    if (!typed.length) rows.forEach((project) =>
      list((project.meta || {}).depends_on).forEach((key) => {
        const dep = rows.find((row) => row.key === key) || null;
        deps.push(
          `<tr><td class="mono">${E(project.key)}</td><td>→</td><td class="mono">${E(key)}</td><td>${E(
            dep ? dep.name : "not in this tenant",
          )}</td><td>${B(
            dep ? dep.status : "missing",
            dep && (dep.status === "closed" || dep.status === "archived") ? "green" : "amber",
          )}</td><td><span class="muted">seed</span></td></tr>`,
        );
      }),
    );
    const totals = (pick) => rows.reduce((sum, project) => sum + pick(unitStats(project)), 0);
    const summary = kv([
      ["Programme", programme ? `${E(programme.name)} ${B(programme.status || "active", "green")}` : `<span class="muted">no programme row</span>`],
      [
        "Projects",
        `${rows.length} · ${rows.filter((p) => p.status === "active").length} active · ${
          rows.filter((p) => p.status === "draft").length
        } draft · ${
          rows.filter((p) => p.status === "closed" || p.status === "archived").length
        } closed`,
      ],
      [
        "Units",
        `${totals((u) => u.open)} open · ${totals((u) => u.ready)} ready · ${totals(
          (u) => u.claimed,
        )} claimed · ${totals((u) => u.blocked)} blocked`,
      ],
      ["Cards", `${rows.filter((p) => cardStats(p)).length} project boards loaded`],
    ]);
    const programmeRow = msp().programme || programme || {};
    const inFlightRows = list(msp().inFlight);
    const content =
      tab === "Timeline"
        ? deliveryTimeline()
        : tab === "Dependencies"
        ? panel(
            "Project dependencies",
            `${deps.length
              ? table(["Project", "", "Dependency", "Kind / name", "State", ""], deps)
              : note("No dependency edge is recorded. An edge is typed blocks · needs · supersedes · informs; a cycle is refused by the action.")}${note("The typed edges come from <span class='mono'>project_dependencies</span>; the seed's <span class='mono'>meta.depends_on</span> keys are shown only where no typed edge exists.")}`,
            `${B(`${deps.length} edges`)}${button("New dependency", "msp-dependency-new", "", "small", "plus")}`,
          )
        : tab === "Programme"
          ? panel(
              "Programme",
              `${summary}${programmeQueue(rows, list(cs.register.items))}${kv([
                ["Programme row", E(programmeRow.name || "—")],
                ["Key", E(programmeRow.key || programmeRow.slug || "—")],
                ["Status", B(programmeRow.status || "—", programmeRow.status === "active" ? "green" : "amber")],
                ["Version", E(programmeRow.version == null ? "—" : "v" + programmeRow.version)],
                ["In flight", `${inFlightRows.length} projects in the tenant's in-flight set`],
              ])}${inFlightRows.length ? table(["Project", "Status"], inFlightRows.map((row) => `<tr><td><strong>${E(row.name || mspProjectName(row.id))}</strong><small>${E(row.key || "")}</small></td><td>${B(row.status || "—", row.status === "active" ? "green" : "amber")}</td></tr>`)) : note("portfolio.in_flight returned no project.")}${note("Programme configuration is a store row (programmes.update). The in-flight set is the tenant's portfolio configuration, read by portfolio.in_flight.")}`,
              `${button("Edit programme", "msp-programme-edit", "", "small", "edit")}${B(programmeRow.status || "no programme row", programmeRow.status === "active" ? "green" : "")}`,
            )
          : tab === "Backlog"
            ? `${portfolioBacklogPanel()}${portfolioCadencePanel()}${portfolioCapacityPanel()}${portfolioMeteringPanel()}`
            : panel(
                "In-flight portfolio",
                `${table(["Project", "Status", "Units", "Cards", "Blockers", "Plan"], inFlight)}${inFlightRows.length ? panel("portfolio.in_flight", table(["Project", "Key", "Status"], inFlightRows.map((row) => `<tr><td>${E(row.name || "—")}</td><td class="mono">${E(row.key || "—")}</td><td>${B(row.status || "—", row.status === "active" ? "green" : "amber")}</td></tr>`)), B(`${inFlightRows.length} in flight`)) : ""}${note(
                  "Every row is a tenant project. Units are the GitHub issues linked on the project; cards are the project's board; a card is complete when its lane kind is done or archive.",
                )}`,
                `${B(`${rows.length} projects`, "green")}${button(
                  "Load all boards",
                  "programme-load-boards",
                  "",
                  "small",
                  "board",
                )}`,
              );
    return `<div class="page enterprise">${heading(
      "Portfolio",
      "The programme's real projects, their linked units, their boards and their dependency edges.",
      `${route("Projects", "projects", "", "projects")}${route("Plan & WBS", "plan", "", "board")}`,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon(
      "Projects, units, boards and dependency edges read from the platform's own rows.",
      true,
    )}${summary}${tabs(
      ["Timeline", "In flight", "Dependencies", "Programme", "Backlog"],
      tab,
      "portfolio",
    )}${content}</div>`;
  }

  function portfolio() {
    if (consoleOn()) return consolePortfolio();
    if (live) return publicPortfolio();
    return `<div class="page enterprise">${heading("Portfolio", "The programme's real projects, tranches and linked units, read from Postgres.", "", "PORTFOLIO & PLAN")}${note("No published snapshot is loaded. The public site serves the programme from the database; sign in at the app origin to read it.", "bad")}</div>`;
  }

  // Signed out, the portfolio is the snapshot's real programme — never the design fixture.
  function publicPortfolio() {
    const rows = snapshotProjects();
    const programme = list((live.programme || {}).programme)[0] || {};
    const body = rows.map((p) => {
      const meta = p.meta || {};
      return `<tr><td><strong>${E(p.name)}</strong><small>${E(p.key)}${p.repo_url ? ` · ${E(p.repo_url)}` : ""}</small></td><td>${E(meta.tranche ?? "—")}</td><td>${B(p.status || "—", p.status === "active" ? "green" : p.status === "closed" ? "" : "amber")}</td><td>${E(meta.measured_status || p.phase_code || "—")}</td><td><small>${E(projectIssues(p).length)} linked</small></td></tr>`;
    });
    const dependencies = rows.flatMap((p) =>
      list((p.meta || {}).depends_on).map((dep) => ({ from: p.key, to: dep })),
    );
    const depRows = dependencies
      .slice(0, 60)
      .map((d) => `<tr><td class="mono">${E(d.from)}</td><td class="mono">${E(d.to)}</td></tr>`);
    return `<div class="page enterprise">${heading(
      "Portfolio",
      "The conveyor programme's real projects, tranches and linked units, read from Postgres.",
      `${route("Plan & WBS", "plan", "", "board")}${route("Projects", "projects", "", "projects")}`,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon(
      `The published programme snapshot${programme.name ? ` · ${E(programme.name)}` : ""}: ${rows.length} projects and their dependency edges.`,
      rows.length > 0,
    )}${deliveryTimeline()}${programmeQueue(rows, list((window.CONVEYOR_OUTPUTS || {}).outputs))}${panel(
      "Projects",
      `<p class="muted">Every row is a programme project in the published snapshot, with the GitHub units linked on it.</p>${table(
        ["Project", "Tranche", "Status", "Measured", "Units"],
        rows.length
          ? body
          : [`<tr><td colspan="5">No project is published yet.</td></tr>`],
      )}`,
      B(`${rows.length} projects`, rows.length ? "green" : ""),
    )}${panel(
      "Dependency edges",
      `${table(
        ["From", "To"],
        depRows.length
          ? depRows
          : [`<tr><td colspan="2">No dependency edges are published.</td></tr>`],
      )}`,
    )}${portfolioBacklogPanel()}${portfolioCadencePanel()}${portfolioCapacityPanel()}${portfolioMeteringPanel()}</div>`;
  }

  /* ------------------------------------------------ Programme definition (MSP) */
  // SPEC_V2 §8.3 / D62. One source, fail closed: signed in the store's actions; signed out the
  // published snapshot read-only; `?design=1` the study fixtures; otherwise the rows are not
  // published and no fixture is rendered on the live site.
  function programmeDefinitionPage() {
    if (consoleOn()) return programmeDefinitionLive();
    if (live && live.msp) return programmeDefinitionSnapshot();
    if (studyOn()) return programmeDefinitionStudy();
    return `<div class="page enterprise">${heading(
      "Programme definition",
      "The programme's definition set, benefits, tranches, realization and stakeholders.",
      `${route("Portfolio", "portfolio", "", "layers")}`,
      "PORTFOLIO & PLAN",
    )}${note("The programme-definition rows are not published on this pack. This destination is part of the conveyor design study and is available after signing in at the app origin.", "bad")}</div>`;
  }

  function programmeDefinitionStudy() {
    const tab = v.tab.programme || "Definition";
    const def = F.programmeDefinition;
    const benefit = F.benefits.find((b) => b.id === v.benefit) || F.benefits[0];
    const tranche = F.tranches.find((t) => t.no === v.tranche) || F.tranches[0];
    const row = F.realization.find((r) => r.benefit === benefit.id) || F.realization[0];
    const gate = v.programmeGate;
    const span = Math.max(benefit.baseline, benefit.target, benefit.current, 1);
    const pct = (n) => Math.min(100, Math.round((n / span) * 100));
    const bar = `<div class="v2-bar" style="position:relative"><i style="width:${pct(benefit.current)}%"></i></div><div class="row" style="justify-content:space-between;margin-top:4px"><small class="muted">baseline ${E(benefit.baseline)}</small><small class="muted">current ${E(benefit.current)}</small><small class="muted">target ${E(benefit.target)}</small></div>`;
    const reviewEvidence =
      tranche.no === 0
        ? [
            ["Baselines frozen and approved", "B0 · B1"],
            ["Benefits against their profiles", "BEN-02 78% · BEN-03 at risk"],
            ["Risk and issue register", "2 open · 1 high"],
            ["Blueprint alignment", "no deviation"],
            ["Tranche exit criteria", "3 of 4 met"],
          ]
        : [["First review is due on " + tranche.review, "not started"]];
    // Stakeholders: the store's own profiles when present, the study fixture otherwise.
    const stakeholderRows = list(mspSource().stakeholders).length
      ? list(mspSource().stakeholders)
      : F.stakeholders;
    const stakeholdersPanel = panel(
      "Stakeholder profiles",
      `${table(
        ["Name", "Organisation", "Role", "Influence", "Interest", "Support", "Disbenefits"],
        stakeholderRows.map(
          (s) =>
            `<tr><td><strong>${E(s.name || "—")}</strong><small>${E(s.id || "")}</small></td><td>${E(s.organisation || "—")}</td><td><small>${E(s.role || s.engagement_target || "—")}</small></td><td><small>${E(s.influence || "—")}</small></td><td><small>${E(s.interest || "—")}</small></td><td>${B(s.support_level || "—", s.support_level === "supportive" ? "green" : s.support_level === "neutral" ? "amber" : "")}</td><td><small>${E(s.disbenefits || "—")}</small></td></tr>`,
        ),
      )}${note("A profile names what the programme wants from the engagement and what the engagement costs the stakeholder. Influence without interest is kept satisfied, interest without influence is kept informed, both high is managed closely; a disbenefit is tracked as evidence, not as a reason to omit the row.")}`,
      B(`${stakeholderRows.length} profiles`, stakeholderRows.length ? "green" : ""),
    );

    const definition =
      `${panel("Mandate · " + def.name, `${table(
        ["Field", "Entry"],
        def.fields.map(([field, entry]) => `<tr><td><strong>${E(field)}</strong></td><td>${E(entry)}</td></tr>`),
      )}${kv([
        ["Producer", "programme manager"],
        ["Reviewer", "BCM"],
        ["Approver", "sponsoring group"],
        ["Version", E(def.mandate.version)],
        ["State", B(gate === "mandate-ok" ? "Approved" : def.mandate.state, gate === "mandate-ok" ? "green" : "amber")],
        ["Sponsors", E(def.sponsors)],
      ])}${gate === "mandate-ok" ? note("<strong>Approved</strong> · the sponsoring group authorised the mandate. The programme may enter definition; the brief is the next gate.", "ok") : gate === "mandate-return" ? note("<strong>Returned</strong> · the sponsoring group returned the mandate. No definition work starts and no delivery project is opened.", "bad") : note("The mandate is a decision, not a document: the sponsoring group approves the six fields, then the brief, then the first tranche. The programme cannot skip a gate.")}`, `${button("Approve the mandate", "programme-mandate", "", "small", "shield")}${button("Return for revision", "programme-mandate-return", "", "small", "replay")}`)}` +
      `${panel("Definition set", `${table(
        ["ID", "Artefact", "Producer", "Reviewer", "Approver", "Version", "State", "Gate"],
        def.artefacts.map((a) => `<tr><td class="mono">${E(a.id)}</td><td><strong>${E(a.title)}</strong><small>${E(a.kind)}</small></td><td><small>${E(a.producer)}</small></td><td><small>${E(a.reviewer)}</small></td><td><small>${E(a.approver)}</small></td><td>${E(a.version)}</td><td>${B(a.state, a.state === "Approved" ? "green" : a.state === "In review" ? "amber" : "")}</td><td><small>${E(a.gate)}</small></td></tr>`),
      )}${note("Every artefact names its producer, reviewer and approver. The set is the programme's, not a document library: a missing approval is a missing gate.")}`, B(`${def.artefacts.length} artefacts`))}` +
      `${panel("Delivery options", `${table(
        ["Option", "Assessment"],
        def.options.map(([option, assessment]) => `<tr><td><strong>${E(option)}</strong></td><td>${E(assessment)}</td></tr>`),
      )}${note("Doing nothing is always an option and is assessed as one. The business case has two return paths: redesign the future state, or rework the project solutions.")}`)}`;

    const benefits =
      `<div class="v2-split"><div>${panel("Benefit profiles", `${table(
        ["ID", "Benefit", "Owner", "State"],
        F.benefits.map((b) => `<tr data-v2="benefit" data-id="${b.id}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="benefit" data-id="${b.id}">${E(b.id)}</button></td><td><strong>${E(b.title)}</strong><small>${E(b.kpi)}</small></td><td><small>${E(b.owner)}</small></td><td>${B(b.state, b.state === "Tracking" ? "green" : b.state === "At risk" ? "amber" : "")}</td></tr>`),
      )}`, B(`${F.benefits.length} profiles`))}</div><div>${panel(
        "Profile · " + benefit.id,
        `${kv([
          ["Benefit", E(benefit.title)],
          ["Category", E(benefit.category)],
          ["Objective", E(benefit.objective)],
          ["Observable outcome", E(benefit.outcome)],
          ["KPI", E(benefit.kpi)],
          ["Measurement method", E(benefit.method)],
          ["Financial value", E(benefit.value)],
          ["Cost to realize", E(benefit.cost)],
          ["Conditions", E(benefit.conditions)],
          ["Dependencies", E(benefit.deps)],
          ["Owner", E(benefit.owner)],
          ["Operational area", E(benefit.area)],
        ])}${bar}${note("A capability gives a team a new option; whether it uses the option, and whether use improves performance, are separate questions. The baseline is measured while the old practice is still observable.")}`,
        B(benefit.state, benefit.state === "Tracking" ? "green" : "amber"),
      )}</div></div>${panel("Benefits map", `${table(
        ["Output", "Capability", "Operational outcome", "Benefit", "Strategic objective"],
        F.benefitLinks.map((l) => `<tr><td><strong>${E(l.output)}</strong></td><td>${E(l.capability)}</td><td>${E(l.outcome)}</td><td class="mono">${E(l.benefit)}</td><td>${E(l.objective)}</td></tr>`),
      )}${note("Read left to right. A dependency slip names the benefits put at risk through these edges. A disbenefit is tracked with the same profile discipline.")}`)}`;

    const tranches =
      `<div class="v2-split"><div>${panel("Tranches", `${table(
        ["#", "Tranche", "Projects", "Baseline", "Review", "State"],
        F.tranches.map((t) => `<tr data-v2="tranche-review" data-id="${t.no}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="tranche-review" data-id="${t.no}">${t.no}</button></td><td><strong>${E(t.name)}</strong></td><td><small>${E(t.projects)}</small></td><td>${E(t.baseline)}</td><td>${E(t.review)}</td><td>${B(t.state, t.state === "In review" ? "amber" : "")}</td></tr>`),
      )}`, B(`${F.tranches.length} tranches`))}</div><div>${panel(
        "Review · tranche " + tranche.no,
        `${kv([
          ["Tranche", E(tranche.name)],
          ["Projects", E(tranche.projects)],
          ["Entry criteria", E(tranche.entry)],
          ["Exit criteria", E(tranche.exit)],
          ["Management baseline", E(tranche.baseline)],
          ["Review", E(tranche.review)],
          ["Decision", E(tranche.decision)],
        ])}${table(
          ["Evidence", "State"],
          reviewEvidence.map((r) => `<tr><td>${E(r[0])}</td><td>${E(r[1])}</td></tr>`),
        )}${gate === "tranche-continue" ? note("<strong>Decision recorded · continue</strong> · tranche 0 closes and B1 becomes the management baseline for tranche 1. The next tranche is authorised on this evidence.", "ok") : gate === "tranche-hold" ? note("<strong>Decision recorded · pause</strong> · tranche 1 does not start; the definition set returns to the board with the named gaps.", "bad") : note("A review with no evidence is refused. A re-baseline goes through change control; the review never rewrites the baseline.")}`,
        `${button("Continue to tranche 1", "tranche-continue", "", "small", "shield")}${button("Hold the tranche", "tranche-hold", "", "small", "replay")}`,
      )}</div></div>${note("A tranche is not a PI: the PI cadence is the delivery rhythm inside it; the review is the sponsoring group's decision to continue, pause or stop on evidence.")}`;

    const realization =
      `${panel("Transition and measurement", `${table(
        ["Benefit", "Baseline measured", "Readiness", "Transition", "Post-transition", "Legacy retired", "Owner", "State"],
        F.realization.map((r) => `<tr data-v2="transition-select" data-id="${r.benefit}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="transition-select" data-id="${r.benefit}">${E(r.benefit)}</button></td><td><small>${E(r.baselineMeasured)}</small></td><td><small>${E(r.readiness)}</small></td><td>${E(r.transition)}</td><td><small>${E(r.post)}</small></td><td><small>${E(r.legacy)}</small></td><td><small>${E(r.owner)}</small></td><td>${B(r.state, r.state.startsWith("Blocked") ? "amber" : r.state === "Monitoring" ? "green" : "")}</td></tr>`),
      )}${note("The BCM measures the baseline before the transition, plans it, assesses readiness, enacts it, measures post-transition and retires the legacy practice. A benefit whose baseline was never measured cannot claim an improvement.")}`)}` +
      `${panel("Selected benefit · " + benefit.id, `${kv([
        ["Benefit", E(benefit.title)],
        ["Baseline", E(row.baselineMeasured)],
        ["Readiness", E(row.readiness)],
        ["Transition", E(row.transition)],
        ["Post-transition measurement", E(row.post)],
        ["Legacy retirement", E(row.legacy)],
        ["Post-closure owner", E(row.owner)],
      ])}${gate === "transition-ok" ? note("<strong>Transition recorded</strong> · the post-transition measurement opens on the planned date; the legacy practice is retired after the first post measure. The comparison with the baseline is the sponsor's evidence.", "ok") : gate === "transition-blocked" ? note("<strong>422 benefit_baseline_missing</strong> · this benefit has no measured baseline. Measure the baseline first; the tool ledger is not live.", "bad") : note("The transition is the BCM's procedure. It is not a status change on the project, and the programme does not close until the realization plan names who keeps measuring.")}`, button("Record the transition", "transition-record", "", "small", "run"))}`;

    const content =
      tab === "Definition"
        ? definition
        : tab === "Benefits map & profiles"
          ? benefits
          : tab === "Tranches & reviews"
            ? tranches
            : tab === "Benefits realization"
              ? realization
              : tab === "Dossier"
                ? mspDossier(mspSource())
                : stakeholdersPanel;

    return `<div class="page enterprise">${heading(
      "Programme definition",
      "The programme's mandate, brief, blueprint, business case, benefits, tranches and realized outcomes — the MSP layer of SPEC_V2.",
      `${route("Portfolio", "portfolio", "", "layers")}${route("Plan & WBS", "plan", "", "board")}`,
      "PORTFOLIO & PLAN",
    )}${note("Design study · these are fixtures, not published rows. The live page reads the programme store when signed in, and the published snapshot read-only on the public site.")}${tabs(MSP_TABS, tab, "programme")}${content}</div>`;
  }

  /* --------------------------------------- Architecture capability (TOGAF ADM) */
  // SPEC_V2 §8.1 / D63. Same rule as the programme definition: signed in, the four tabs read
  // the tenant's own rows through the architecture actions; signed out, the published snapshot
  // read-only; `?design=1` keeps the study fixtures. No control on the live or snapshot path
  // writes — the card that adds the controls adds the actions with them.
  const ADM_TABS = ["Capability", "ADM phases", "Artefact repository", "Board & compliance"];
  const admPhaseOrder = (code) => {
    const value = String(code || "");
    if (value === "Preliminary") return 0;
    if (/^[A-H]$/.test(value)) return 1 + value.charCodeAt(0) - 65;
    return 20;
  };
  const admWhen = (value) => (value ? String(value).slice(0, 10) : "—");
  const admTone = (status) => {
    const value = String(status || "").toLowerCase();
    if (value === "approved" || value === "live" || value === "closed" || value === "active") {
      return "green";
    }
    if (value === "draft" || value === "in_review" || value === "in progress") return "amber";
    return "";
  };
  const admText = (value, n = 200) => {
    if (value == null || value === "") return "—";
    if (Array.isArray(value)) {
      return value.length
        ? short(
            value
              .map((row) => (row && typeof row === "object" ? JSON.stringify(row) : String(row)))
              .join(" · "),
            n,
          )
        : "—";
    }
    if (typeof value === "object") {
      const parts = Object.entries(value).map(([key, row]) => {
        const text = Array.isArray(row)
          ? row.length
            ? row.join(", ")
            : "—"
          : row && typeof row === "object"
            ? JSON.stringify(row)
            : row;
        return `${key}: ${text}`;
      });
      return parts.length ? short(parts.join(" · "), n) : "—";
    }
    return String(value);
  };
  const admScore = (value) => {
    const score = Number(value);
    return Number.isFinite(score)
      ? B(String(score), score >= 4 ? "green" : score >= 2 ? "" : "amber")
      : "—";
  };
  const admShortId = (value) => `<span class="mono">${E(short(value || "", 8) || "—")}</span>`;
  const architectureCapabilityHeading = () =>
    heading(
      "Architecture capability",
      "The TOGAF ADM as a capability and a project: principles, the tailored profile, the typed artefact repository, the phase contracts, the board, compliance and waivers — SPEC_V2 §8.1.",
      `${route("Programme definition", "programme", "", "shield")}${route("System architecture", "architecture", "", "model")}`,
      "GOVERN & DESIGN",
    );

  // One body over the four tabs, shared by the live and the snapshot reads once each source
  // has normalized its rows. `errors` names a failed read per section; `entriesFor` and
  // `linksFor` answer the inspector's cached state for one artefact.
  function architectureAdmBody(model) {
    const {
      profile,
      standard,
      artifacts,
      contracts,
      boards,
      maturity,
      skills,
      errors = {},
      entriesFor,
      linksFor,
    } = model;
    const tab = ADM_TABS.includes(v.tab.adm) ? v.tab.adm : "Capability";
    const failure = (what, error) =>
      error
        ? note(`<strong>${E(what)} read refused</strong> · ${E(consoleError(error))}`, "bad")
        : "";
    const artName = (id) => {
      const row = list(artifacts).find((item) => item.id === id);
      return row ? row.title : short(id || "", 8);
    };
    const phases = (() => {
      const codes = new Set();
      list(standard).forEach((row) => codes.add(row.phase_code));
      list(artifacts).forEach((row) => codes.add(row.phase_code));
      list(contracts).forEach((row) => codes.add(row.phase_code));
      return [...codes]
        .filter(Boolean)
        .sort(
          (a, b) => admPhaseOrder(a) - admPhaseOrder(b) || String(a).localeCompare(String(b)),
        )
        .map((code) => {
          const types = list(standard).filter((row) => row.phase_code === code);
          const rows = list(artifacts).filter((row) => row.phase_code === code);
          const deals = list(contracts).filter((row) => row.phase_code === code);
          const required = types.filter((row) => row.mandatory);
          const approved = new Set(
            rows.filter((row) => row.status === "approved").map((row) => row.artifact_type_id),
          );
          const approvedCount = required.filter((row) => approved.has(row.id)).length;
          const liveDeal = deals.find((row) => row.status === "live") || null;
          // A phase is closed only when its mandatory set is approved and its contract is
          // live — the store's own rule, never the contract alone.
          const closed = !!liveDeal && approvedCount === required.length;
          const plateaus = [...new Set(types.concat(rows).map((row) => row.plateau).filter(Boolean))];
          return {
            code,
            plateau: plateaus.join(" · ") || "—",
            required: required.length,
            approved: approvedCount,
            missing: required
              .filter((row) => !approved.has(row.id))
              .map((row) => row.name || row.key || "unnamed artefact"),
            contract: liveDeal
              ? `${liveDeal.kind} · live`
              : deals[0]
                ? `${deals[0].kind} · ${deals[0].status}`
                : "—",
            state: closed
              ? "Closed"
              : approvedCount > 0 || deals.length || rows.length
                ? "In progress"
                : "Planned",
          };
        });
    })();

    const board = list(boards)[0] || null;
    const principles =
      list(artifacts).find((row) => row.artifact_kind === "principles_catalog") || null;
    const principleState = principles ? entriesFor(principles) : null;
    const capability = `<div class="v2-split"><div>${panel(
      `Principles catalog${
        principles ? ` · <span class="mono">${E(short(principles.id, 8))}</span>` : ""
      }`,
      errors.artifacts
        ? failure("artefacts", errors.artifacts)
        : !principles
          ? note("No principles catalog is recorded in this repository yet.")
          : principleState.error
            ? failure("principles entries", principleState.error)
            : !principleState.loaded
              ? `${note(
                  principleState.loading
                    ? "Reading the principles catalog's entries…"
                    : "Its entries load when the artefact is selected.",
                )}${
                  principleState.loading
                    ? ""
                    : button(
                        "Read the principles catalog",
                        "adm-artifact",
                        `data-id="${E(principles.id)}"`,
                        "small",
                        "model",
                      )
                }`
              : principleState.items.length
                ? `${table(
                    ["Principle", "Statement"],
                    principleState.items.map(
                      (row) =>
                        `<tr><td class="mono">${E(admText(row.axis_a, 40))}</td><td>${E(
                          admText(row.value, 200),
                        )}</td></tr>`,
                    ),
                  )}${note("The principles catalog is an ordinary artefact: its entries are rows and the board approves the version.")}`
                : note("The principles catalog carries no entries yet."),
      principles ? B(`v${principles.artifact_version || 0}`, "green") : "",
    )}</div><div>${panel(
      "Tailored ADM profile",
      errors.profile
        ? failure("profile", errors.profile)
        : !profile
          ? note("No tailored ADM profile is recorded in this tenant yet.")
          : `${kv([
              ["Profile", E(profile.name)],
              ["Key", `<span class="mono">${E(profile.key)}</span>`],
              ["Phases", E(admText(profile.phase_set))],
              ["Requirements", E(admText(profile.artifact_requirements, 240))],
              ["Contracts", E(admText(profile.contract_kinds))],
              ["Status", B(profile.status || "—", admTone(profile.status))],
              ["Adopted", E(admWhen(profile.adopted_at))],
              ["Board decision", E(short(profile.board_decision_id, 8) || "—")],
              ["Standard", `${list(standard).length} artefact types`],
              ["Version", E(profile.version)],
            ])}${note("The profile is the tenant's tailoring: which phases run, which artefact types are mandatory per phase and plateau, and which contract closes each phase.")}${failure("standard", errors.standard)}`,
      profile ? B(profile.status || "—", admTone(profile.status)) : "",
    )}${panel(
      "Architecture Board",
      errors.boards
        ? failure("board", errors.boards)
        : !board
          ? note("No architecture board is recorded in this tenant yet.")
          : `${kv([
              ["Body", E(board.name)],
              ["Key", `<span class="mono">${E(board.key || "—")}</span>`],
              ["Charter", E(admText(board.charter))],
              ["Commissioning authority", E(admText(board.commissioning_authority))],
              ["Decision limits", E(admText(board.decision_limits))],
              ["Membership", E(admText(board.members))],
              ["Status", B(board.status || "—", admTone(board.status))],
            ])}${note("The board is a governance body with authority rows, not a meeting. It cannot approve a delivery gate it requested and cannot waive a deviation it owns.")}`,
      board ? B(`${board.key || "board"} · v${board.version}`, "green") : "",
    )}</div></div>` +
      `<div class="v2-split"><div>${panel(
        "Maturity · the newest assessment",
        errors.maturity
          ? failure("maturity", errors.maturity)
          : !maturity || !list(maturity.dimensions).length
            ? note("No maturity assessment is recorded in this tenant yet.")
            : `${table(
                ["Dimension", "Score"],
                list(maturity.dimensions).map(
                  (row) =>
                    `<tr><td>${E(row.key)}</td><td>${admScore(row.score)}</td></tr>`,
                ),
              )}${note("Scale 0–5: none, initial, under development, defined, managed, measured. The baseline is dated evidence, not an assertion.")}`,
        maturity ? B(maturity.stage || "assessment") : "",
      )}</div><div>${panel(
        "Skills · the newest assessment",
        errors.skills
          ? failure("skills", errors.skills)
          : !list(skills).length
            ? note("No skills assessment is recorded in this tenant yet.")
            : `${table(
                ["Role", "Category", "Proficiency"],
                list(skills).map(
                  (row) =>
                    `<tr><td><small>${E(row.role)}</small></td><td>${E(row.key)}</td><td>${admScore(
                      row.score,
                    )}</td></tr>`,
                ),
              )}${note("The Architecture Skills Framework assesses proficiency per role. The development plan is a Preliminary-phase output.")}`,
        B(`${list(skills).length} categories`),
      )}</div></div>`;

    const phase = phases.find((row) => row.code === v.admPhase) || phases[0] || null;
    const phasesTab = `<div class="v2-split"><div>${panel(
      "Phase rail",
      errors.standard || errors.artifacts || errors.contracts
        ? `${failure("standard", errors.standard)}${failure("artefacts", errors.artifacts)}${failure("contracts", errors.contracts)}`
        : phases.length
          ? `${table(
              ["Phase", "Plateau", "Mandatory", "Approved", "Contract", "State"],
              phases.map(
                (row) =>
                  `<tr data-v2="adm-phase" data-id="${E(row.code)}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="adm-phase" data-id="${E(row.code)}">${E(row.code)}</button></td><td>${E(row.plateau)}</td><td>${row.required}</td><td>${row.approved}</td><td><small>${E(row.contract)}</small></td><td>${B(row.state, admTone(row.state))}</td></tr>`,
              ),
            )}${note("Each phase closes on an approved mandatory set and a live contract; a phase cannot be skipped. The set is the standard filtered by the profile.")}`
          : note("The standard and the repository carry no phase code yet."),
    )}</div><div>${panel(
      `Phase · ${phase ? E(phase.code) : "—"}`,
      !phase
        ? note("No phase is recorded yet.")
        : `${kv([
            ["Plateau", E(phase.plateau)],
            ["Mandatory artefacts", `${phase.approved} approved of ${phase.required}`],
            ["Contract", E(phase.contract)],
            ["State", B(phase.state, admTone(phase.state))],
          ])}${
            phase.missing.length
              ? `<h4>Missing mandatory set</h4><ul>${phase.missing
                  .slice(0, 12)
                  .map((name) => `<li>${E(name)}</li>`)
                  .join("")}${
                  phase.missing.length > 12
                    ? `<li>… ${phase.missing.length - 12} more</li>`
                    : ""
                }</ul>`
              : ""
          }${note("A phase closes only when every artefact the profile marks mandatory is approved and its contract is live. This screen records nothing; the write controls are the next unit.")}`,
    )}${panel(
      "Contracts across the ADM",
      errors.contracts
        ? failure("contracts", errors.contracts)
        : list(contracts).length
          ? `${table(
              ["ID", "Contract", "Phase", "Version", "State", "Approved by"],
              list(contracts).map(
                (row) =>
                  `<tr><td>${admShortId(row.id)}</td><td><strong>${E(row.kind)}</strong><small>${E(
                    admText(row.title),
                  )}</small></td><td>${E(row.phase_code)}</td><td>${E(
                    row.version,
                  )}</td><td>${B(row.status, admTone(row.status))}</td><td><small>${E(
                    short(row.approved_by, 8) || "—",
                  )}</small></td></tr>`,
              ),
            )}${note("The Statement of Architecture Work is distinct from the designs produced under it; a contract change is a new version and re-opens its gate.")}`
          : note("No architecture contract is recorded in this tenant yet."),
    )}</div></div>`;

    const typeFilter = v.admType || "all";
    const kinds = ["all", "catalog", "matrix", "diagram"].concat(
      [...new Set(list(artifacts).map((row) => row.artifact_kind).filter(Boolean))].filter(
        (kind) => !["catalog", "matrix", "diagram"].includes(kind),
      ),
    );
    const visible = list(artifacts).filter(
      (row) => typeFilter === "all" || row.artifact_kind === typeFilter,
    );
    const art = list(artifacts).find((row) => row.id === v.adm) || list(artifacts)[0] || null;
    const entryState = art ? entriesFor(art) : null;
    const hasAxisB = !!(entryState && entryState.items.some((row) => row.axis_b));
    const entryBlock = !art
      ? note("No artefact is recorded in this repository yet.")
      : !entryState || entryState.error
        ? failure("artefact entries", entryState && entryState.error)
        : !entryState.loaded
          ? `${note(
              entryState.loading
                ? "Reading this artefact's entries…"
                : "Its entries load when the artefact is selected.",
            )}${
              entryState.loading
                ? ""
                : button(
                    "Read this artefact",
                    "adm-artifact",
                    `data-id="${E(art.id)}"`,
                    "small",
                    "model",
                  )
            }`
          : entryState.items.length
            ? `${table(
                hasAxisB ? ["Axis A", "Axis B", "Value"] : ["Axis A", "Value"],
                entryState.items.map(
                  (row) =>
                    `<tr><td class="mono">${E(admText(row.axis_a, 60))}</td>${
                      hasAxisB ? `<td class="mono">${E(admText(row.axis_b, 60))}</td>` : ""
                    }<td>${E(admText(row.value, 200))}</td></tr>`,
                ),
              )}${note("A catalog's rows are entries; a matrix's cells carry both axes.")}`
            : `${kv([
                ["Document", E(short(art.document_id, 8) || "—")],
                ["Metadata", E(admText(art.meta))],
              ])}${note("No entries are recorded on this artefact yet.")}`;
    const linkState = art ? linksFor(art) : null;
    const links = linkState && linkState.items ? linkState.items : [];
    const linksBlock =
      linkState && linkState.error
        ? failure("artefact links", linkState.error)
        : !linkState || !linkState.loaded
          ? note(linkState && linkState.loading ? "Reading this artefact's links…" : "Its links load with its entries.")
          : links.length
            ? table(
                ["Kind", "Direction", "Other artefact"],
                links.map((row) => {
                  const outgoing = row.from_artifact_id === art.id;
                  const other = outgoing ? row.to_artifact_id : row.from_artifact_id;
                  return `<tr><td class="mono">${E(row.kind)}</td><td>${
                    outgoing ? "out" : "in"
                  }</td><td><strong>${E(artName(other))}</strong></td></tr>`;
                }),
              )
            : note("No links are recorded on this artefact.");
    const artifactsTab =
      `<div class="toolbar"><span class="mono muted">TYPE</span>${kinds
        .map(
          (kind) =>
            `<button class="btn ${typeFilter === kind ? "selected" : ""}" data-v2="adm-type" data-id="${E(
              kind,
            )}">${E(kind)}</button>`,
        )
        .join("")}</div>` +
      `<div class="v2-split"><div>${panel(
        "Artefact repository",
        errors.artifacts
          ? failure("artefacts", errors.artifacts)
          : `${
              visible.length
                ? table(
                    ["ID", "Artefact", "Type", "Phase", "Plateau", "State", "Version"],
                    visible.map(
                      (row) =>
                        `<tr data-v2="adm-artifact" data-id="${E(row.id)}" style="cursor:pointer" class="${
                          art && row.id === art.id ? "selected" : ""
                        }"><td><button type="button" class="btn small" data-v2="adm-artifact" data-id="${E(
                          row.id,
                        )}">${E(short(row.id, 8))}</button></td><td><strong>${E(
                          row.title,
                        )}</strong></td><td>${B(
                          row.artifact_kind,
                          row.artifact_kind === "catalog" ||
                            row.artifact_kind === "principles_catalog"
                            ? "purple"
                            : row.artifact_kind === "matrix"
                              ? ""
                              : "amber",
                        )}</td><td><small>${E(row.phase_code)}</small></td><td>${E(
                          row.plateau,
                        )}</td><td>${B(row.status, admTone(row.status))}</td><td>${E(
                          row.artifact_version,
                        )}</td></tr>`,
                    ),
                  )
                : note("No artefact matches this type.")
            }${note("A catalog is queried, a matrix is joined, a diagram is a document with a viewpoint. The repository answers what an artefact derives from, realizes and reuses.")}`,
        B(`${list(artifacts).length} artefacts`),
      )}</div><div>${panel(
        `Artefact · ${art ? `<span class="mono">${E(short(art.id, 8))}</span>` : "—"}`,
        !art
          ? note("Select an artefact to inspect it.")
          : `${kv([
              ["Title", E(art.title)],
              ["Type", B(art.artifact_kind, "purple")],
              ["Phase", E(art.phase_code)],
              ["Plateau", E(art.plateau)],
              ["Status", B(art.status, admTone(art.status))],
              ["Version", E(art.artifact_version)],
              ["Purpose", E(admText(art.purpose))],
              ["Scope", E(admText(art.scope))],
              ["Document", E(short(art.document_id, 8) || "—")],
              ["Contract", E(short(art.contract_id, 8) || "—")],
            ])}${entryBlock}`,
      )}${art ? panel("Links · derivation, realization, reuse, dependency", linksBlock) : ""}</div></div>`;

    const boardTab = `<div class="v2-split"><div>${panel(
      "Architecture boards",
      errors.boards
        ? failure("board", errors.boards)
        : list(boards).length
          ? `${table(
              ["Key", "Body", "Kind", "Status", "Commissioning authority", "Members", "Version"],
              list(boards).map(
                (row) =>
                  `<tr><td class="mono">${E(row.key)}</td><td><strong>${E(
                    row.name,
                  )}</strong></td><td>${E(row.kind)}</td><td>${B(
                    row.status,
                    admTone(row.status),
                  )}</td><td><small>${E(
                    admText(row.commissioning_authority),
                  )}</small></td><td><small>${E(admText(row.members))}</small></td><td>${E(
                    row.version,
                  )}</td></tr>`,
              ),
            )}${note("The board supports the roadmap, assesses changes against it, applies the principles and commissions solution architectures.")}`
          : note("No architecture board is recorded in this tenant yet."),
    )}</div><div>${panel(
      `Board · ${board ? `<span class="mono">${E(board.key || "")}</span>` : "—"}`,
      !board
        ? note("No board row is loaded.")
        : `${kv([
            ["Body", E(board.name)],
            ["Kind", E(board.kind)],
            ["Charter", E(admText(board.charter))],
            ["Commissioning authority", E(admText(board.commissioning_authority))],
            ["Decision limits", E(admText(board.decision_limits))],
            ["Membership", E(admText(board.members))],
            ["Status", B(board.status, admTone(board.status))],
            ["Version", E(board.version)],
          ])}${note("The board cannot approve a delivery gate it requested and cannot waive a deviation it owns. This screen reads the rows; the decision controls are the next unit.")}`,
    )}</div></div>`;

    return tab === "Capability"
      ? capability
      : tab === "ADM phases"
        ? phasesTab
        : tab === "Artefact repository"
          ? artifactsTab
          : boardTab;
  }

  // Signed in: the tenant's rows. The reads fire once; every later render draws the cache.
  function architectureCapabilityLive() {
    if (!cs.adm.loaded && !cs.adm.loading) consoleLoadAdm();
    const artifacts = list(cs.adm.artifacts.items);
    const selected = artifacts.find((row) => row.id === v.adm) || artifacts[0] || null;
    if (cs.adm.loaded && selected) consoleLoadAdmEntries(selected.id);
    const skillItems = list(cs.adm.skills.items);
    const firstAssessment = skillItems[0] ? skillItems[0].assessment_id : null;
    const skills = skillItems
      .filter((row) => row.assessment_id === firstAssessment)
      .flatMap((row) =>
        list(row.categories).map((cat) => ({ role: row.role, key: cat.key, score: cat.score })),
      );
    const model = {
      profile: cs.adm.profile.data,
      standard: cs.adm.standard.items,
      artifacts,
      contracts: cs.adm.contracts.items,
      boards: cs.adm.boards.items,
      maturity: list(cs.adm.maturity.items)[0] || null,
      skills,
      errors: {
        profile: cs.adm.profile.error,
        standard: cs.adm.standard.error,
        artifacts: cs.adm.artifacts.error,
        contracts: cs.adm.contracts.error,
        boards: cs.adm.boards.error,
        maturity: cs.adm.maturity.error,
        skills: cs.adm.skills.error,
      },
      entriesFor: (art) =>
        cs.adm.entries[art.id] || { items: [], loaded: false, loading: false, error: null },
      linksFor: (art) =>
        cs.adm.links[art.id] || { items: [], loaded: false, loading: false, error: null },
    };
    const tab = ADM_TABS.includes(v.tab.adm) ? v.tab.adm : "Capability";
    return `<div class="page enterprise">${architectureCapabilityHeading()}${consoleRibbon(
      "The tailored profile, the artefact repository, the phase contracts, the board and the two baselines are the tenant's own rows.",
      true,
    )}${tabs(ADM_TABS, tab, "adm")}${
      cs.adm.loaded
        ? architectureAdmBody(model)
        : note(
            "Reading the architecture capability · the profile and its standard, the artefacts, the contracts, the board, the maturity and the skills assessment.",
          )
    }</div>`;
  }

  // Signed out: the published snapshot's `architecture` section, read-only and with no call.
  function architectureCapabilitySnapshot() {
    const A = (live && live.architecture) || {};
    const profiles = list(A.profiles);
    const maturityGroups = (() => {
      const groups = [];
      const index = new Map();
      list(A.maturity).forEach((row) => {
        if (!index.has(row.assessment_id)) {
          index.set(row.assessment_id, groups.length);
          groups.push({ assessment_id: row.assessment_id, stage: row.stage || "", dimensions: [] });
        }
        groups[index.get(row.assessment_id)].dimensions.push({
          key: row.dimension,
          score: row.score,
        });
      });
      return groups;
    })();
    const skillRows = list(A.skills);
    const firstSkills = skillRows[0] ? skillRows[0].assessment_id : null;
    const skills = skillRows
      .filter((row) => row.assessment_id === firstSkills)
      .map((row) => ({ role: row.role, key: row.category, score: row.proficiency }));
    const links = list(A.links);
    const artifacts = list(A.artifacts);
    const model = {
      profile: profiles.find((row) => row.status === "adopted") || profiles[0] || null,
      standard: list(A.standard),
      artifacts,
      contracts: list(A.contracts),
      boards: list(A.board),
      maturity: maturityGroups[0] || null,
      skills,
      errors: {},
      entriesFor: (art) => ({
        items: list(art && art.entries),
        loaded: true,
        loading: false,
        error: null,
      }),
      linksFor: (art) => ({
        items: art
          ? links.filter(
              (row) => row.from_artifact_id === art.id || row.to_artifact_id === art.id,
            )
          : [],
        loaded: true,
        loading: false,
        error: null,
      }),
    };
    const tab = ADM_TABS.includes(v.tab.adm) ? v.tab.adm : "Capability";
    return `<div class="page enterprise">${architectureCapabilityHeading()}${consoleRibbon(
      "The published architecture snapshot: the tailored profile, the artefact repository, the contracts, the board and the baselines. Read-only: sign in at the app origin to operate the capability.",
      true,
    )}${tabs(ADM_TABS, tab, "adm")}${architectureAdmBody(model)}</div>`;
  }

  function architectureCapabilityPage() {
    if (consoleOn()) return architectureCapabilityLive();
    if (designStudy) return architectureCapabilityStudy();
    if (live && live.architecture) return architectureCapabilitySnapshot();
    return `<div class="page enterprise">${architectureCapabilityHeading()}${note(
      "The architecture-capability rows are not published on this pack. This destination is part of the conveyor design study and is available after signing in at the app origin.",
      "bad",
    )}</div>`;
  }

  // SPEC_V2 §8.1 / D63: the Preliminary capability, the ADM phase rail with typed artefacts, the
  // artefact repository and the board/compliance register. The `?design=1` reviewer study —
  // local fixtures only, kept byte-for-byte behind that explicit mode.
  function architectureCapabilityStudy() {
    const cap = F.architectureCapability;
    const tab = v.tab.adm || "Capability";
    const gate = v.admGate;
    const art = cap.artifacts.find((a) => a.id === v.adm) || cap.artifacts[0];
    const phase = cap.phases.find((p) => p.code === v.admPhase) || cap.phases[2];
    const typeFilter = v.admType || "all";
    const tone = (state) =>
      state === "Approved" || state === "Closed"
        ? "green"
        : state === "Draft" || state === "In progress"
          ? "amber"
          : "";
    const score = (n) => B(String(n), n >= 4 ? "green" : n >= 2 ? "" : "amber");
    const missing = phase.missing.length
      ? phase.missing
      : phase.approved < phase.required
        ? [`${phase.required - phase.approved} mandatory artefacts are not yet approved`]
        : [];

    const capability =
      `<div class="v2-split"><div>${panel(
        "Principles catalog · ART-001 v3",
        `${table(
          ["ID", "Principle", "Rationale", "Implications"],
          cap.principles.map((p) => `<tr><td class="mono">${E(p.id)}</td><td><strong>${E(p.statement)}</strong></td><td><small>${E(p.rationale)}</small></td><td><small>${E(p.implications)}</small></td></tr>`),
        )}${note("The principles catalog is an ordinary artefact: entries are rows, the Architecture Board approves the version, and a change supersedes it.")}`,
        B("approved · v3", "green"),
      )}</div><div>${panel(
        "Tailored ADM profile",
        `${kv([
          ["Profile", E(cap.profile.name)],
          ["Phases", E(cap.profile.phases)],
          ["PSA set", E(cap.profile.psa)],
          ["HLD set", E(cap.profile.hld)],
          ["SD set", E(cap.profile.sd)],
          ["Contracts", E(cap.profile.contracts)],
          ["Adopted", E(cap.profile.adopted)],
        ])}${note("The profile is the tenant's tailoring: which phases run, which artefact types are mandatory per phase and plateau, and which contract closes each phase. The mandatory set grows with the assessed maturity.")}`,
      )}${panel(
        "Architecture Board",
        `${kv([
          ["Body", E(cap.board.name)],
          ["Chair", E(cap.board.chair)],
          ["Membership", E(cap.board.members)],
          ["Charter", E(cap.board.charter)],
          ["Commissioning authority", E(cap.board.authority)],
          ["Decision limits", E(cap.board.limits)],
        ])}${note("The board is a governance body with authority rows, not a meeting. It cannot approve a delivery gate it requested and cannot waive a deviation it owns.")}`,
        B("charter · BD-01", "green"),
      )}</div></div>` +
      `<div class="v2-split"><div>${panel(
        "Maturity · nine dimensions",
        `${table(
          ["Dimension", "Baseline", "Stage 1", "Stage 2"],
          cap.maturity.map((m) => `<tr><td>${E(m.dimension)}</td><td>${score(m.baseline)}</td><td>${score(m.stage1)}</td><td>${score(m.stage2)}</td></tr>`),
        )}${note("Scale 0–5: none, initial, under development, defined, managed, measured. Each stage is a milestone a tranche review holds the capability to; the baseline is dated evidence, not an assertion.")}`,
      )}</div><div>${panel(
        "Skills · seven categories",
        `${table(
          ["Category", "Role", "Current", "Target"],
          cap.skills.map((s) => `<tr><td>${E(s.category)}</td><td><small>${E(s.role)}</small></td><td>${score(s.current)}</td><td>${score(s.target)}</td></tr>`),
        )}${note("The Architecture Skills Framework assesses proficiency per role. The development plan is a Preliminary-phase output, aligned with the maturity stages.")}`,
      )}</div></div>`;

    const advanceNote =
      gate === "adm-advance-closed"
        ? note(`<strong>Phase already closed</strong> · ${E(phase.code)} reached its contract and no further advance is recorded.`, "ok")
        : gate === "adm-advance-refused"
          ? note(`<strong>422 phase_artifacts_missing</strong> · ${E(missing.join(" · "))}. ${E(phase.contract)} is not live; the phase stays open and no contract row is written.`, "bad")
          : gate === "adm-advance-ok"
            ? note(`<strong>Phase closed</strong> · every mandatory artefact is approved and the contract is live. The next phase is open.`, "ok")
            : note("A phase closes only when every artefact the profile marks mandatory is approved, the required links resolve and its contract is live. The refusal names the missing artefact, never a generic error.");

    const phases =
      `<div class="v2-split"><div>${panel(
        "Phase rail",
        `${table(
          ["Phase", "Plateau", "Mandatory", "Approved", "Contract", "State"],
          cap.phases.map((p) => `<tr data-v2="adm-phase" data-id="${E(p.code)}" style="cursor:pointer"><td><button type="button" class="btn small" data-v2="adm-phase" data-id="${E(p.code)}">${E(p.code)}</button></td><td>${E(p.plateau)}</td><td>${p.required}</td><td>${p.approved}</td><td><small>${E(p.contract)}</small></td><td>${B(p.state, tone(p.state))}</td></tr>`),
        )}${note("Each phase is a WBS node. Its mandatory set comes from the standard filtered by the profile; it closes on an approved contract; a phase cannot be skipped.")}`,
      )}</div><div>${panel(
        "Phase · " + phase.code + " " + phase.name,
        `${kv([
          ["Plateau", E(phase.plateau)],
          ["Mandatory artefacts", `${phase.approved} approved of ${phase.required}`],
          ["Contract", E(phase.contract)],
          ["State", B(phase.state, tone(phase.state))],
        ])}${missing.length ? `<h4>Missing mandatory set</h4><ul>${missing.map((m) => `<li>${E(m)}</li>`).join("")}</ul>` : ""}${advanceNote}`,
        button("Advance the phase", "adm-advance", "", "small", "run"),
      )}${panel(
        "Contracts across the ADM",
        `${table(
          ["ID", "Contract", "Phase", "Version", "State", "Owner", "Reviewer"],
          cap.contracts.map((c) => `<tr><td class="mono">${E(c.id)}</td><td><strong>${E(c.kind)}</strong><small>${E(c.body)}</small></td><td>${E(c.phase)}</td><td>${E(c.version)}</td><td>${B(c.state, c.state === "Approved" ? "green" : c.state === "Draft" ? "amber" : "")}</td><td><small>${E(c.owner)}</small></td><td><small>${E(c.reviewer)}</small></td></tr>`),
        )}${note("The Statement of Architecture Work is distinct from the designs produced under it. A contract change is a new version and re-opens its gate; the approved body is immutable.")}`,
      )}</div></div>`;

    const entries =
      cap.artifactEntries[art.id] ||
      (art.id === "ART-001"
        ? {
            caption: "Principles catalog entries · statement, rationale, implications",
            headers: ["ID", "Statement", "Rationale", "Implications"],
            rows: cap.principles.map((p) => [p.id, p.statement, p.rationale, p.implications]),
          }
        : null);
    const links = cap.artifactLinks.filter((l) => l.from === art.id || l.to === art.id);
    const entryBlock = entries
      ? `${table(
          entries.headers,
          entries.rows.map((r) => `<tr>${r.map((cell) => `<td>${E(cell)}</td>`).join("")}</tr>`),
        )}${note(E(entries.caption))}`
      : art.type === "diagram"
        ? `${kv([
            ["Viewpoint", E(art.viewpoint)],
            ["Metamodel", "ArchiMate 3.1"],
            ["Document", E(art.document)],
          ])}${note("A diagram is a document with typed metadata and a viewpoint, not a live metamodel. The row carries the metadata; the pixels live in the document.")}`
        : note("No entries are recorded on this artefact yet.");
    const artifacts =
      `<div class="toolbar"><span class="mono muted">TYPE</span>${["all", "catalog", "matrix", "diagram"].map((t) => `<button class="btn ${typeFilter === t ? "selected" : ""}" data-v2="adm-type" data-id="${t}">${E(t)}</button>`).join("")}</div>` +
      `<div class="v2-split"><div>${panel(
        "Artefact repository",
        `${table(
          ["ID", "Artefact", "Type", "Phase", "Plateau", "State", "Version", "Owner"],
          cap.artifacts
            .filter((a) => typeFilter === "all" || a.type === typeFilter)
            .map((a) => `<tr data-v2="adm-artifact" data-id="${a.id}" style="cursor:pointer" class="${a.id === art.id ? "selected" : ""}"><td><button type="button" class="btn small" data-v2="adm-artifact" data-id="${a.id}">${E(a.id)}</button></td><td><strong>${E(a.title)}</strong></td><td>${B(a.type, a.type === "catalog" ? "purple" : a.type === "matrix" ? "" : "amber")}</td><td><small>${E(a.phase)}</small></td><td>${E(a.plateau)}</td><td>${B(a.state, tone(a.state))}</td><td>${E(a.version)}</td><td><small>${E(a.owner)}</small></td></tr>`),
        )}${note("A catalog is queried, a matrix is joined, a diagram is a document with a viewpoint. The repository answers what an artefact derives from, realizes and reuses.")}`,
        B(`${cap.artifacts.length} artefacts`),
      )}</div><div>${panel(
        "Artefact · " + art.id,
        `${kv([
          ["Title", E(art.title)],
          ["Type", B(art.type, art.type === "catalog" ? "purple" : art.type === "matrix" ? "" : "amber")],
          ["Phase", E(art.phase)],
          ["Plateau", E(art.plateau)],
          ["Status", B(art.state, tone(art.state))],
          ["Version", E(art.version)],
          ["Author", E(art.owner)],
          ["Reviewer", E(art.reviewer)],
          ["Review date", E(art.reviewed)],
          ["Document", E(art.document)],
          ["Entries", E(art.entries)],
        ])}${gate === "adm-artifact-approved" ? note(`<strong>Approved</strong> · a new version supersedes ${E(art.version)}; the approved body is unchanged and the phase gate re-opens.`, "ok") : ""}${entryBlock}`,
        button("Approve a new version", "adm-approve", "", "small", "shield"),
      )}${panel(
        "Links · derivation, realization, reuse, dependency",
        `${links.length ? table(["Kind", "Direction", "Other artefact"], links.map((l) => `<tr><td class="mono">${E(l.kind)}</td><td>${l.from === art.id ? "out" : "in"}</td><td><strong>${E(l.from === art.id ? l.to : l.from)}</strong></td></tr>`)) : note("No links are recorded on this artefact.")}`,
      )}</div></div>`;

    const board =
      `<div class="v2-split"><div>${panel(
        "Architecture Board decisions",
        `${table(
          ["ID", "Decision", "Evidence", "State"],
          cap.boardDecisions.map((d) => `<tr><td class="mono">${E(d.id)}</td><td><strong>${E(d.decision)}</strong></td><td><small>${E(d.evidence)}</small></td><td>${B(d.state, "green")}</td></tr>`),
        )}${note("The board supports the roadmap, assesses changes against it, applies the principles and commissions solution architectures. Its decision limits are rows, not convention.")}`,
      )}${panel(
        "Phase H outcomes · exactly one per review",
        `${table(
          ["Review", "Finding", "Outcome", "State"],
          cap.hOutcomes.map((h) => `<tr><td class="mono">${E(h.id)}</td><td>${E(h.finding)}</td><td><strong>${E(h.outcome)}</strong></td><td><small>${E(h.state)}</small></td></tr>`),
        )}${gate === "adm-h-change" ? note("<strong>Change request recorded</strong> · CR-020 with its impact block is routed to the change gate; the project baseline is unchanged until that gate is approved.", "ok") : gate === "adm-h-kata" ? note("<strong>Kata candidate recorded</strong> · K-015 is routed to the kata lane and no change register row is written.", "ok") : gate === "adm-h-refused" ? note("<strong>422 phase_h_outcome_exists</strong> · this review already recorded a change request. A review has exactly one outcome.", "bad") : note("A change review records exactly one outcome: a change request with impact, or a kata candidate. A review with no verdict cannot close phase H. A Request for Architecture Work opens further architecture work.")}`,
        `${button("Record a change", "adm-h-change", "", "small", "shield")}${button("Record a kata", "adm-h-kata", "", "small", "chart")}`,
      )}</div><div>${panel(
        "Compliance reviews · six levels",
        `${table(
          ["Project", "Contract", "Level", "Findings", "Date"],
          cap.compliance.map((c) => `<tr><td><strong>${E(c.project)}</strong></td><td class="mono">${E(c.contract)}</td><td>${B(c.level, c.level === "Non-conformant" ? "bad" : c.level === "Compliant" || c.level === "Fully conformant" ? "green" : "amber")}</td><td><small>${E(c.findings)}</small></td><td>${E(c.date)}</td></tr>`),
        )}${gate === "adm-review-recorded" ? note("<strong>Review recorded · Consistent</strong> · one level per review; the findings are the evidence the delivery project carries.", "ok") : gate === "adm-review-refused" ? note("<strong>422 compliance_level_exists</strong> · a review records exactly one of the six levels; a second is refused.", "bad") : note("The levels describe different relationships, not a maturity ladder: irrelevant, consistent, compliant, conformant, fully conformant, non-conformant.")}`,
        `${button("Record a review", "adm-review", "", "small", "check")}${button("Record a second level", "adm-review-second", "", "small", "replay")}`,
      )}${panel(
        "Waivers",
        `${table(
          ["ID", "Project", "Deviation", "Accepting owner", "Mitigation", "Cost / risk", "Review", "State"],
          cap.waivers.map((w) => `<tr><td class="mono">${E(w.id)}</td><td>${E(w.project)}</td><td><small>${E(w.deviation)}</small></td><td>${E(w.owner)}</td><td><small>${E(w.mitigation)}</small></td><td>${E(w.costRisk)}</td><td>${E(w.review)}</td><td>${B(w.state, "amber")}</td></tr>`),
        )}${gate === "adm-waiver-refused" ? note("<strong>422 waiver_owner_required</strong> · a waiver without an accepting asset owner and a reassessment date is refused; the deviation stays non-conformant.", "bad") : gate === "adm-waiver-ok" ? note("<strong>Waiver W-02 recorded</strong> · accepting owner asset owner · operations; reassessment 2027-02-12.", "ok") : note("A deviation is rejected, mitigated, or accepted for a defined period. The asset owner accepts the risk — outages, maintenance and additional cost.")}`,
        `${button("Record a waiver", "adm-waiver", "", "small", "shield")}${button("Record with owner + date", "adm-waiver-ok", "", "small", "check")}`,
      )}</div></div>`;

    const content =
      tab === "Capability" ? capability : tab === "ADM phases" ? phases : tab === "Artefact repository" ? artifacts : board;

    return `<div class="page enterprise">${heading(
      "Architecture capability",
      "The TOGAF ADM as a capability and a project: principles, the tailored profile, the typed artefact repository, the phase contracts, the board, compliance and waivers — SPEC_V2 §8.1.",
      `${route("Programme definition", "programme", "", "shield")}${route("System architecture", "architecture", "", "model")}`,
      "GOVERN & DESIGN",
    )}${ribbon("TOGAF ADM capability: architecture_artifact (catalog | matrix | diagram), artifact_type, adm_profile, architecture_contract, architecture_compliance_review, architecture_waiver, governance_body, architecture_maturity, architecture_skill_assessment. Proposed in T9; D63.")}${tabs(["Capability", "ADM phases", "Artefact repository", "Board & compliance"], tab, "adm")}${content}</div>`;
  }

  /* -------------------------------------------------------------- Validation */
  // The measured programme: every status comes from `docs/design/data/coverage.json`, which
  // `scripts/build-coverage.py` derives from the slice frontmatter, the migrations, the
  // compiled registry dump and this file. The page renders the data; it never computes a
  // status, so the published claim and the gate cannot disagree.
  function validation() {
    const cov = window.CONVEYOR_COVERAGE || null;
    const tab = v.tab.validation || "Programme";
    if (!cov) {
      return `<div class="page enterprise">${heading("Validation", "The programme measured against its own plans — projects, tranches, screens and the gap list.", "", "PLAN")}${note("No coverage data is loaded. Run <span class='mono'>python3 scripts/build-coverage.py</span> and ship <span class='mono'>data/coverage.js</span>.", "bad")}</div>`;
    }
    const projects = cov.projects || [];
    const screens = cov.screens || [];
    const tone = (status) =>
      status === "built" || status === "live" || status === "standing"
        ? "green"
        : status === "partial" || status === "mixed"
          ? "amber"
          : "";
    const units = (u) =>
      `${u.open} open${u.claimed ? ` · ${u.claimed} claimed` : ""}${u.blocked ? ` · ${u.blocked} blocked` : ""}`;
    const summary = `<div class="v2-split"><div>${panel(
      "Programme",
      `${kv([
        ["Projects", `${cov.summary.projects.total}`],
        ["Built", `${cov.summary.projects.by_status.built}`],
        ["Partial", `${cov.summary.projects.by_status.partial}`],
        ["Proposed (V2)", `${cov.summary.projects.by_status.proposed}`],
        ["Uncovered", `${cov.summary.projects.uncovered.length}`],
      ])}${note(
        "A project is <strong>built</strong> when every artefact its plan declares exists; <strong>partial</strong> when some do; <strong>proposed</strong> when its V2 slice has no code. A non-built project with no open unit fails the gate.",
        "ok",
      )}`,
    )}</div><div>${panel(
      "Surfaces",
      `${kv([
        ["Screens", `${cov.summary.screens.total}`],
        ["Live", `${cov.summary.screens.by_status.live}`],
        ["Mixed", `${cov.summary.screens.by_status.mixed}`],
        ["Proposed", `${cov.summary.screens.by_status.proposed}`],
        ["Legacy V1", `${cov.summary.screens.by_status.legacy}`],
      ])}${note(
        "A <strong>live</strong> screen reads or writes the platform; <strong>mixed</strong> means part of the page is real; <strong>proposed</strong> is a V2 destination with no backend call; <strong>legacy</strong> is a retained V1 surface.",
        "ok",
      )}`,
    )}</div></div>`;
    const tranches = [...new Set(projects.map((p) => p.tranche))].sort((a, b) => a - b);
    const programme = tranches
      .map((tranche) => {
        const rows = projects.filter((p) => p.tranche === tranche);
        return panel(
          `Tranche ${tranche} · ${rows.length} project${rows.length === 1 ? "" : "s"}`,
          table(
            ["Project", "Track", "Status", "Measured t/a/m", "Units", "Blocked by", "Gap"],
            rows.map(
              (p) =>
                `<tr><td><strong>${E(p.key)}</strong><small>${E(p.name)}</small></td><td>${E(p.track)}</td><td>${B(p.status, tone(p.status))}</td><td class="mono">${p.measured.tables[0]}/${p.measured.tables[1]} · ${p.measured.actions[0]}/${p.measured.actions[1]} · ${p.measured.modules[0]}/${p.measured.modules[1]}</td><td>${E(units(p.units))}</td><td>${p.blocked_by.length ? p.blocked_by.map((k) => `<code>${E(k)}</code>`).join(" ") : "—"}</td><td><small>${p.gap ? E(p.gap) : "—"}</small></td></tr>`,
            ),
          ),
          B(`${rows.filter((p) => p.status === "built").length} built`, "green"),
        );
      })
      .join("");
    const screenRows = table(
      ["Screen", "Status", "Evidence", "Gap"],
      screens.map(
        (s) =>
          `<tr><td><strong>${E(s.title)}</strong><small>${E(s.key)}</small></td><td>${B(s.status, tone(s.status))}</td><td><small>${s.evidence.map((e) => `<code>${E(e)}</code>`).join(" ")}</small></td><td><small>${s.gap ? E(s.gap) : "—"}</small></td></tr>`,
      ),
    );
    const actionRows = table(
      ["Action", "Invoked at"],
      cov.actions_invoked.map(
        (a) =>
          `<tr><td class="mono">${E(a.name)}</td><td><small>v2.js:${a.line}</small></td></tr>`,
      ),
    );
    const content =
      tab === "Programme"
        ? `<div>${programme}</div>`
        : tab === "Screens"
          ? panel("Surfaces", `${screenRows}${note("Every row resolves in the pack; a missing page or an unknown status fails <span class='mono'>check-coverage.cjs</span>.")}`)
          : panel("Actions invoked by the pack", `${actionRows}${note("Every name resolves in the compiled registry dump <span class='mono'>docs/design/data/actions.json</span>.")}`);
    return `<div class="page enterprise">${heading(
      "Validation",
      "The programme measured against its own plans: every project's status with evidence, its tranche, its units and its gap; every screen's real-versus-proposed state.",
      `${route("Portfolio", "portfolio", "", "projects")}${route("Plan & WBS", "plan", "", "board")}`,
      "PLAN",
    )}<div class="v2-ribbon live" role="note"><strong>MEASURED</strong><span>Every status is derived from the slice frontmatter, the migrations, the compiled registry dump and this pack; <span class="mono">check-coverage.cjs</span> is the gate.</span><span class="v2-src">docs/design/data/coverage.json</span></div>${summary}${tabs(["Programme", "Screens", "Actions"], tab, "validation")}${content}</div>`;
  }

  /* ------------------------------------------------------------------ Plan */
  function plan() {
    const signed = consoleOn();
    if (signed) consoleLoadMsp();
    const work = live ? (live.work || {}) : {};
    const liveBoards = list(work.boards).length > 0;
    const snapshotMsp = !signed && !!(live && live.msp);
    const consoleCapable = !!(C && C.config());
    const names = signed || snapshotMsp
      ? ["Boards", "WBS", "Baseline vs forecast", "Registers", "Cadence", "Change control"]
      : studyOn()
        ? ["Boards", "Change control"]
        : ["Boards"];
    const tab = names.includes(v.tab.plan) ? v.tab.plan : "Boards";
    const content = tab === "Boards" ? planBoards(work) : planMspContent(tab);
    const planRibbon =
      tab === "Boards"
        ? consoleOn()
          ? `<div class="v2-ribbon live" role="note"><strong>LIVE</strong><span>Operator console · the platform's own projects, boards and cards, joined to the GitHub issue registry. Signed in as ${E(C.status().name)} · tenant ${E(C.status().tenant)}.</span><span class="v2-src">API · no snapshot</span></div>`
          : ribbon("LIVE · GitHub issues are read at publish and joined to the conveyor's own cards and runs. A unit with no real card is shown as not intaken; nothing is seeded.", liveBoards)
        : consoleRibbon("The plan spine reads the programme store: WBS nodes, baselines, registers and cadences. Signed out it is the published snapshot, read-only.", signed || snapshotMsp);
    return `<div class="page enterprise">${heading("Plan & WBS", "The plan spine: WBS, frozen baselines, one six-kind register and cadence. Boards join GitHub issues to the conveyor's own cards and runs.", `${route("Portfolio", "portfolio", "", "projects")}${route("Assurance", "assurance", "", "shield")}`, "PLAN")}${planRibbon}${tabs(names, tab, "plan")}${content}<div class="toolbar" style="margin-top:16px"><span class="mono muted">Existing V1 surfaces stay available:</span>${signed ? `${route("Work breakdown editor", "work", "small", "board")}${route("Assurance & baselines", "assurance", "small", "shield")}` : ""}<span class="muted">Change control is T10 and is on the Change control tab: a request names the baseline it moves, its impact and a decision with a rationale.${snapshotMsp ? " The published snapshot is read-only." : ""}</span></div></div>`;
  }

  function planMspContent(tab) {
    const source = mspSource();
    if (consoleOn() && !msp().loaded && !msp().error) return note("Loading the plan spine…");
    if (tab === "WBS") {
      const rows = list(source.wbs);
      return `${mspError()}${panel("Work breakdown", `${table(
        ["WBS", "Kind", "Deliverable / work package", "Acceptance", "State", ""],
        rows.length ? rows.map((n) => `<tr><td class="mono">${E(n.code || short(n.id, 8))}</td><td>${E(n.kind || "—")}</td><td><strong>${E(n.title || "—")}</strong><small>${E(n.body || "")}</small></td><td>${E(typeof n.acceptance === "object" ? JSON.stringify(n.acceptance) : n.acceptance || "—")}</td><td>${B(n.status || "planned", n.status === "accepted" ? "green" : n.status === "blocked" ? "amber" : "")}</td><td>${source.readOnly ? "" : `${button("Edit", "msp-wbs-edit", `data-id="${E(n.id)}"`, "small", "edit")}${button("Delete", "msp-delete", `data-kind="wbs" data-id="${E(n.id)}" data-version="${E(n.version)}"`, "small", "close")}`}</td></tr>`)
          : [`<tr><td colspan="6">No WBS node is recorded for the definition-set project.</td></tr>`]
      )}${note("A node is accepted when its acceptance criteria are met by an artefact or a run — not when a checkbox is ticked.")}`, source.readOnly ? "" : button("New WBS node", "msp-wbs-new", "", "small", "plus"))}`;
    }
    if (tab === "Baseline vs forecast") {
      const rows = list(source.baselines);
      return `${mspError()}${panel("Frozen baselines", `${table(
        ["Kind", "Taken at", "Reference", "Version"],
        rows.length ? rows.map((b) => `<tr><td>${B(b.kind || "—")}</td><td>${E(b.taken_at || b.created_at || "—")}</td><td class="mono">${E(b.id || "—")}</td><td>${E(b.version == null ? "—" : "v" + b.version)}</td></tr>`)
          : [`<tr><td colspan="4">No baseline is recorded. A baseline is a frozen, approved snapshot and is never recomputed.</td></tr>`]
      )}${note("A forecast is derived from current rows; a variance is not a change until change control (T10) approves it.")}`, source.readOnly ? "" : `<div class="toolbar">${["scope", "schedule", "cost"].map((kind) => button("Take " + kind, "msp-baseline-take", `data-kind="${kind}"`, "small", "shield")).join("")}</div>`)}${panel("Forecast", `${kv([["Scope", "computed from the WBS rows"], ["Schedule", "computed from the node dates"], ["Cost", "computed from the register and job rows"]])}${note("Change control is T10 and its approved requests are on the Change control tab; a forecast never rewrites an approved baseline.")}`)}`;
    }
    if (tab === "Registers") {
      const rows = list(source.registers);
      return `${mspError()}${panel("One generic register", `<p class="muted">One table, six kinds — risk · issue · assumption · dependency · benefit · change.</p>${table(
        ["Kind", "Title", "Status", "Severity", ""],
        rows.length ? rows.map((r) => `<tr><td>${B(r.kind || "—")}</td><td><strong>${E(r.title || "—")}</strong><small>${E(r.body || "")}</small></td><td>${E(r.status || "—")}</td><td>${E(r.severity || "—")}</td><td>${source.readOnly ? "" : `${button("Edit", "msp-register-edit", `data-id="${E(r.id)}"`, "small", "edit")}${button("Delete", "msp-delete", `data-kind="register" data-id="${E(r.id)}" data-version="${E(r.version)}"`, "small", "close")}`}</td></tr>`)
          : [`<tr><td colspan="5">No register item is recorded.</td></tr>`]
      )}${note("A risk can open a mitigation card; a dependency can link two projects; a benefit is measured at closure.")}`, source.readOnly ? "" : button("New register item", "msp-register-new", "", "small", "plus"))}`;
    }
    if (tab === "Change control") return planChanges(source);
    const rows = list(source.cadences);
    return `${mspError()}${panel("Cadence", `${table(
      ["Kind", "Period", "State", ""],
      rows.length ? rows.map((c) => `<tr><td>${B(c.kind || "—")}</td><td><strong>${E(c.period || "—")}</strong></td><td>${E(c.state || "—")}</td><td>${source.readOnly ? "" : `${button("Edit", "msp-cadence-edit", `data-id="${E(c.id)}"`, "small", "edit")}${button("Delete", "msp-delete", `data-kind="cadence" data-id="${E(c.id)}" data-version="${E(c.version)}"`, "small", "close")}`}</td></tr>`)
        : [`<tr><td colspan="4">No cadence is recorded.</td></tr>`]
    )}${note("A tranche is not a PI: the PI cadence is the delivery rhythm inside it; the review is the sponsoring group's decision to continue, pause or stop on evidence.")}`, source.readOnly ? "" : button("New cadence", "msp-cadence-new", "", "small", "plus"))}`;
  }

  /* Change control (T10) on the plan spine: the programme register's `change` rows when a
     store is loaded; the design study's change-request fixture in reviewer mode only, so a
     live or published store with no change row says so instead of rendering fiction. */
  function planChanges(source) {
    const register = list(source.registers).filter(
      (r) => String(r.kind || "").toLowerCase() === "change",
    );
    const study = !consoleOn() && !(live && live.msp);
    const rows = register.length
      ? register.map((r) => ({
          id: r.id || "—",
          title: r.title || "—",
          baseline: r.baseline || r.baseline_ref || "—",
          impact:
            typeof r.impact === "object" && r.impact
              ? JSON.stringify(r.impact)
              : r.body || r.impact || "—",
          decision: r.status || r.decision || "Open",
          rationale: r.rationale || r.note || "—",
        }))
      : study
        ? F.changes.map((c) => ({
            id: c.id,
            title: c.title,
            baseline: c.baseline,
            impact: c.impact,
            decision: c.state,
            rationale: c.rationale || "—",
          }))
        : [];
    const tone = (decision) =>
      /approved/i.test(decision)
        ? "green"
        : /rejected|returned|refused/i.test(decision)
          ? "bad"
          : "amber";
    const tableRows = rows.length
      ? table(
          ["ID", "Change request", "Baseline", "Impact", "Decision", "Rationale"],
          rows.map(
            (c) =>
              `<tr><td class="mono">${E(c.id)}</td><td><strong>${E(c.title)}</strong></td><td>${E(c.baseline)}</td><td><small>${E(c.impact)}</small></td><td>${B(c.decision, tone(c.decision))}</td><td><small>${E(c.rationale)}</small></td></tr>`,
          ),
        )
      : `<p class="muted">No change request is recorded against this programme.</p>`;
    const decide = study
      ? `<label class="field full"><span>Decision rationale</span><input data-v2-input="rationale" placeholder="Why the request is approved or returned; a decision with no rationale is refused" aria-label="Decision rationale"></label><div class="toolbar">${button("Approve the request", "change-approve", "", "small", "shield")}${button("Return for analysis", "change-return", "", "small", "replay")}</div>${v.review === "change-blocked" ? note("<strong>422 change_rationale_required</strong> · the decision was refused; the baseline is unchanged.", "bad") : v.review === "change-ok" ? note("<strong>Rationale recorded</strong> · the decision is recorded by the change gate; the baseline changes only through that gate, never through a forecast.", "ok") : ""}`
      : "";
    return `${mspError()}${panel(
      "Change control",
      `${tableRows}${note(study ? "Design study · <span class='mono'>F.changes</span> renders because no programme register row of kind <span class='mono'>change</span> is loaded. A change request names the baseline it moves and carries an impact block; only an approved request moves a baseline." : "A change request is a register row of kind <span class='mono'>change</span>: it names the baseline it moves and carries an impact block. Only an approved request moves a baseline.")}${decide}`,
      B(`${rows.length} request${rows.length === 1 ? "" : "s"}`, rows.length ? "amber" : ""),
    )}`;
  }

  /* The plan is a join, not a seed: the project's GitHub issues (the registry fetched at
     publish) against the conveyor's own cards and runs. A unit with no real card is shown
     honestly as "not intaken"; nothing is fabricated to fill a lane. */
  function planBoards(work) {
    if (consoleOn()) return consolePlanBoards();
    const prog = live ? live.programme || {} : {};
    const projects = list(prog.projects);
    if (!live || !projects.length) {
      const signIn =
        C && C.config()
          ? `<div class="console-actions" style="margin-top:10px">${button("Sign in to operate the programme", "console-signin", "", "primary", "shield")}</div>`
          : "";
      return panel(
        "Boards",
        note(`No published snapshot is loaded. The Plan boards are live-only: the published snapshot joins GitHub issues to the conveyor's own cards and runs. The WBS tab shows the proposed V2 plan spine.${signIn}`),
      );
    }
    const registry = list((prog.issue_registry || {}).issues);
    const issueByNumber = new Map(registry.map((row) => [String(row.number), row]));
    // The snapshot orders projects by key, which puts the cross-cutting keys first. The
    // plan surface opens on the buildout: S slices, then T slices, then the rest.
    const rank = (key) => (/^S/.test(String(key)) ? 0 : /^T/.test(String(key)) ? 1 : 2);
    const ordered = [...projects].sort(
      (a, b) => rank(a.key) - rank(b.key) || String(a.key).localeCompare(String(b.key)),
    );
    const project = ordered.find((row) => row.id === v.planProject) || ordered[0];
    const board = list(work.boards).find((row) => row.project_id === project.id);
    const lists = list(work.lists)
      .filter((row) => row.board_id === (board || {}).id)
      .sort((a, b) => Number(a.position) - Number(b.position));
    const cards = list(work.cards).filter(
      (row) => row.board_id === (board || {}).id && !row.archived_at,
    );
    const types = new Map(list(work.card_types).map((row) => [row.id, row]));
    const laneName = new Map(lists.map((row) => [row.id, row.name]));
    const runs = list(work.runs);
    const steps = list(work.run_steps);
    const runFor = (card) =>
      runs
        .filter((row) => row.card_id === card.id)
        .sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)))[0];
    const linked = list((project.meta || {}).issues);
    const issues = linked.map(
      (number) =>
        issueByNumber.get(String(number)) || {
          number,
          state: "UNKNOWN",
          title: "Not in the published registry",
          url: `https://github.com/nonagenticai/conveyor/issues/${number}`,
          labels: [],
        },
    );
    const open = issues.filter((row) => row.state === "OPEN").length;
    const closed = issues.filter((row) => row.state === "CLOSED").length;
    const statusOf = (issue) => {
      const label = (issue.labels || [])
        .map((row) => row.name)
        .find((name) => String(name).startsWith("status:"));
      return label ? label.slice("status:".length) : "";
    };
    const cardsFor = (issue) =>
      cards.filter((row) => row.issue != null && String(row.issue) === String(issue.number));
    const cardCell = (issue) => {
      const mine = cardsFor(issue);
      if (!mine.length) return '<span class="muted">— not intaken</span>';
      return mine
        .map((card) => {
          const type = types.get(card.card_type_id);
          return `<strong>${E(card.name)}</strong><small class="mono">${E(type ? type.code : "—")} · ${E(card.process_key || "no process")}</small>`;
        })
        .join("<br>");
    };
    const laneCell = (issue) => {
      const mine = cardsFor(issue);
      return mine.length
        ? mine.map((card) => E(laneName.get(card.list_id) || "—")).join("<br>")
        : "—";
    };
    const runCell = (issue) => {
      const mine = cardsFor(issue)
        .map((card) => runFor(card))
        .filter(Boolean);
      if (!mine.length) return "—";
      return mine
        .map(
          (run) =>
            `${B(run.status)}<small class="mono">${E(run.error_class || short(run.id, 8))}</small>`,
        )
        .join("<br>");
    };
    const evidenceCell = (issue) => {
      const mine = cardsFor(issue)
        .map((card) => runFor(card))
        .filter(Boolean);
      if (!mine.length) return "—";
      return mine
        .map(
          (run) =>
            `<span class="mono">${steps.filter((row) => row.run_id === run.id).length} step(s)</span>`,
        )
        .join("<br>");
    };
    const issueRows = issues.map(
      (issue) =>
        `<tr><td><a class="e-project-link" href="${E(issue.url)}" target="_blank" rel="noopener">#${E(issue.number)} ↗</a> <strong>${E(issue.title)}</strong></td><td>${B(issue.state === "CLOSED" ? "Closed" : issue.state === "OPEN" ? "Open" : issue.state, issue.state === "CLOSED" ? "" : "amber")}${statusOf(issue) ? ` <small class="mono">${E(statusOf(issue))}</small>` : ""}</td><td>${cardCell(issue)}</td><td>${laneCell(issue)}</td><td>${runCell(issue)}</td><td>${evidenceCell(issue)}</td></tr>`,
    );
    const issuePanel = panel(
      `${project.key} · GitHub issues`,
      issueRows.length
        ? table(["Issue", "GitHub state", "Card", "Lane", "Run", "Evidence"], issueRows)
        : `<p class="muted">No GitHub issues are registered for this project.</p>`,
      B(`${open} open · ${closed} closed`),
    );
    const unlinked = cards.filter((row) => row.issue == null);
    const unlinkedPanel = unlinked.length
      ? panel(
          "Cards without an issue link",
          table(
            ["Card", "Type", "Lane", "Run"],
            unlinked.map((card) => {
              const type = types.get(card.card_type_id);
              const run = runFor(card);
              return `<tr><td><strong>${E(card.name)}</strong><small class="mono">${E(card.process_key || "")}</small></td><td>${B(type ? type.code : "—")}</td><td>${E(laneName.get(card.list_id) || "—")}</td><td>${run ? B(run.status) : "—"}</td></tr>`;
            }),
          ),
        )
      : "";
    const lanes = lists
      .map((lane) => {
        const laneCards = cards
          .filter((row) => row.list_id === lane.id)
          .sort((a, b) => Number(a.position) - Number(b.position));
        const rows = laneCards.map((card) => {
          const type = types.get(card.card_type_id);
          const run = runFor(card);
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
    const boardLine = board
      ? `Board <span class="mono">${E(board.name)}</span> · ${cards.length} cards · ${lists.length} lanes`
      : "This project has no board in the snapshot.";
    return `${panel(
      "Project",
      `<label class="e-select">Project <select data-v2-input="plan-project">${ordered.map((row) => `<option value="${E(row.id)}" ${row.id === project.id ? "selected" : ""}>${E(row.key)} · ${E(row.name)}</option>`).join("")}</select></label><p class="muted">${boardLine} · ${issues.length} linked issues · registry fetched ${E(short(live.generated_at, 16) || "—")} · snapshot ${E(short(live.payload_sha256))}.</p>`,
    )}${issuePanel}${unlinkedPanel}${lanes}`;
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
  // T16: the destination IS the intake and its gates. Signed in it reads and writes the
  // platform's own procedures and processes; signed out it renders the published snapshot
  // (config.procedures / processes / process_steps) read-only; `?design=1` keeps the study.
  // The owner decision of 2026-11-19 removed the PKO store: procedural knowledge is the
  // procedure/step/PDDL space, and an RDF/OWL document becomes a strata knowledge pack.
  function procedures() {
    if (consoleOn()) return proceduresLive();
    if (live && live.config) return proceduresSnapshot();
    if (studyOn()) return proceduresStudy();
    return `<div class="page enterprise">${heading(
      "Procedures",
      "The SOP intake, the PDDL steps and the human publication gates.",
      "",
      "PROCEDURES & AUTHORING",
    )}${note("The procedure rows are not published on this pack. The intake and its gates are operator actions: sign in at the app origin, or ask for the design study with <span class=\"mono\">?design=1</span>.", "bad")}</div>`;
  }

  function procState() {
    if (!cs.proc) {
      cs.proc = {
        loaded: false,
        loading: false,
        error: null,
        procedures: [],
        processes: [],
        detail: {},
        loadingDetail: null,
        detailError: null,
        gates: [],
        gatesFor: null,
        gatesLoading: false,
        last: null,
        gate: null,
        actionError: null,
      };
    }
    return cs.proc;
  }

  async function consoleLoadProcedures() {
    const p = procState();
    if (!consoleOn() || p.loading || p.loaded) return;
    p.loading = true;
    p.error = null;
    try {
      const [procedurePage, processPage] = await Promise.all([
        C.can("procedures.list")
          ? C.action("procedures.list", { page: { limit: 200 } })
          : Promise.resolve({ items: [] }),
        C.can("processes.list")
          ? C.action("processes.list", { page: { limit: 200 } })
          : Promise.resolve({ items: [] }),
      ]);
      p.procedures = list(procedurePage.items);
      p.processes = list(processPage.items);
      p.loaded = true;
    } catch (error) {
      p.error = error;
    } finally {
      p.loading = false;
      render();
    }
  }

  async function consoleLoadProcDetail(processId) {
    const p = procState();
    if (!consoleOn() || !processId || p.detail[processId] || p.loadingDetail === processId) return;
    p.loadingDetail = processId;
    p.detailError = null;
    try {
      p.detail[processId] = await C.action("processes.get", { process_id: processId });
    } catch (error) {
      p.detailError = error;
    } finally {
      p.loadingDetail = null;
      render();
    }
  }

  async function consoleLoadProcGates(process) {
    const p = procState();
    if (!consoleOn() || !process || !C.can("approvals.list")) return;
    if (p.gatesFor === process.id || p.gatesLoading) return;
    p.gatesFor = process.id;
    p.gatesLoading = true;
    p.actionError = null;
    try {
      const procedure = procProcedureFor(process);
      const targets = [process.id, procedure && procedure.id].filter(Boolean);
      const [pending, approved] = await Promise.all([
        C.action("approvals.list", { status: "pending", page: { limit: 200 } }),
        C.action("approvals.list", { status: "approved", page: { limit: 200 } }),
      ]);
      p.gates = [...list(pending.items), ...list(approved.items)].filter((gate) =>
        targets.includes(gate.target),
      );
    } catch (error) {
      p.actionError = error;
    } finally {
      p.gatesLoading = false;
      render();
    }
  }

  function consoleRefreshProcedures() {
    const p = procState();
    p.loaded = false;
    p.detail = {};
    p.gates = [];
    p.gatesFor = null;
    p.actionError = null;
    consoleLoadProcedures();
  }

  const procInput = (name) => {
    const el = $(`[data-v2-input="${name}"]`);
    return el && typeof el.value === "string" ? el.value.trim() : "";
  };

  const procSource = async (kind) => {
    const fileEl = $(`[data-v2-input="proc-${kind}-file"]`);
    const file = fileEl && fileEl.files && fileEl.files[0];
    if (file) {
      const bytes = await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result || "").split(",").pop() || "");
        reader.onerror = () => reject(reader.error || new Error("the file could not be read"));
        reader.readAsDataURL(file);
      });
      const fallback = kind === "bpmn" ? "bpmn" : kind === "rdf" ? "rdf" : "text";
      return {
        kind: file.type === "application/pdf" ? "pdf" : fallback,
        media_type: file.type || undefined,
        filename: file.name,
        content_base64: bytes,
      };
    }
    const text = procInput(`proc-${kind}-text`);
    if (text) {
      return { kind: kind === "sop" ? "text" : kind, media_type: "text/plain", text };
    }
    // The action's SourceIn carries `kind: "url"`; the field is the fallback when no file or
    // text was supplied, so a document behind an address is ingest material too.
    const url = procInput(`proc-${kind}-url`);
    if (url) return { kind: "url", url };
    throw new Error("Paste the document, attach a file, or give a URL.");
  };

  async function consoleProcIngest(kind) {
    const p = procState();
    if (!consoleOn()) return;
    p.actionError = null;
    p.last = null;
    try {
      const title = procInput(`proc-${kind}-title`);
      const sourceKey = procInput(`proc-${kind}-source-key`);
      const processKey = procInput(`proc-${kind}-process-key`);
      if (!title || !sourceKey) throw new Error("A title and a source key are required.");
      const source = await procSource(kind);
      let output;
      if (kind === "rdf") {
        const storeId = procInput("proc-rdf-store-id");
        if (!storeId) throw new Error("An RDF import needs the strata store id.");
        output = await C.action("procedures.ingest_rdf", {
          title,
          source_key: sourceKey,
          source,
          strata_store_id: storeId,
        });
      } else if (kind === "bpmn") {
        if (!processKey) throw new Error("A BPMN conversion needs a process key.");
        output = await C.action("procedures.ingest_process", {
          title,
          process_key: processKey,
          source_key: sourceKey,
          source,
        });
      } else {
        if (!processKey) throw new Error("An SOP ingest needs a process key.");
        output = await C.action("procedures.ingest_sop", {
          title,
          process_key: processKey,
          source_key: sourceKey,
          source,
        });
      }
      p.last = { kind, output };
      if (output && output.process_id) v.procProcess = String(output.process_id);
      if (output && output.procedure_id) v.procProcedure = String(output.procedure_id);
      consoleRefreshProcedures();
      toast("Draft created · nothing is published and no tool was granted");
    } catch (error) {
      p.actionError = error;
      render();
      toast(consoleError(error));
    }
  }

  async function consoleProcValidate() {
    const p = procState();
    if (!consoleOn() || !v.procProcess) {
      toast("Select a draft process first");
      return;
    }
    p.actionError = null;
    try {
      const output = await C.action("procedures.validate", { process_id: v.procProcess });
      p.last = { kind: "validate", output };
    } catch (error) {
      p.actionError = error;
    }
    render();
  }

  async function consoleProcReview(decision) {
    const p = procState();
    const detail = v.procProcess ? p.detail[v.procProcess] : null;
    const assetId = detail && detail.source_asset_id;
    if (!consoleOn() || !assetId) {
      toast("The selected draft names no source asset");
      return;
    }
    p.actionError = null;
    try {
      const output = await C.action("procedures.review_source", {
        asset_id: assetId,
        decision,
        note: procInput("proc-review-note"),
      });
      p.last = { kind: "review", output };
      delete p.detail[v.procProcess];
      consoleLoadProcDetail(v.procProcess);
    } catch (error) {
      p.actionError = error;
    }
    render();
  }

  function procProcedureFor(process) {
    const p = procState();
    if (!process) return null;
    return (
      p.procedures.find((row) => row.id === v.procProcedure && row.key === process.key) ||
      p.procedures.find((row) => row.key === process.key) ||
      null
    );
  }

  async function consoleProcRequestGate(gate) {
    const p = procState();
    if (!consoleOn()) return;
    const process = procSelected(p);
    if (!process) {
      toast("Select a draft first");
      return;
    }
    const target = gate === "procedures.publish" ? (procProcedureFor(process) || {}).id : process.id;
    if (!target) {
      toast("No draft names this target yet");
      return;
    }
    p.actionError = null;
    try {
      p.gate = await C.action("approvals.request", {
        gate,
        kind: "publication",
        target,
        description:
          gate === "procedures.publish"
            ? "Release the converted SOP judge"
            : "Release the executable procedure",
      });
      p.gates = [];
      p.gatesFor = null;
      consoleLoadProcGates(process);
      toast("Gate requested · a second person decides");
    } catch (error) {
      p.actionError = error;
      render();
      toast(consoleError(error));
    }
  }

  // A gate is decided by a SECOND person: the requester's session cannot decide its own gate
  // (the store refuses it), so each listed gate carries its own decision control and the
  // decider's session works from the list, not from the requester's `p.gate` slot.
  function consoleReloadProcGates() {
    const p = procState();
    p.gates = [];
    p.gatesFor = null;
    const process = procSelected(p);
    if (process) consoleLoadProcGates(process);
    render();
  }

  async function consoleProcDecide(approvalId, decision) {
    const p = procState();
    if (!consoleOn() || !approvalId) {
      toast("Request a gate first");
      return;
    }
    p.actionError = null;
    try {
      const decided = await C.action("approvals.decide", {
        approval_id: approvalId,
        decision,
        note: decision === "approve" ? "Approved in the operator console" : "Rejected in the operator console",
      });
      if (p.gate && p.gate.id === approvalId) p.gate = decided;
      p.gates = [];
      p.gatesFor = null;
      const process = procSelected(p);
      if (process) consoleLoadProcGates(process);
      toast("Gate " + (decided.status || decision));
    } catch (error) {
      p.actionError = error;
      render();
      toast(consoleError(error));
    }
  }

  // The approved gate a publish may name, matched by ITS OWN gate name and target — not by the
  // last request this session made. A listed approved gate is what lets the second person's
  // decision release the artefact in the first person's session.
  function procApprovedGate(p, gateName, target) {
    const listed = list(p.gates).find(
      (gate) => gate.status === "approved" && gate.gate === gateName && gate.target === target,
    );
    if (listed) return listed;
    if (
      p.gate &&
      p.gate.status === "approved" &&
      p.gate.gate === gateName &&
      p.gate.target === target
    ) {
      return p.gate;
    }
    return null;
  }

  const procSelfGate = (gate) => {
    const me = C && C.me ? C.me() : null;
    const id = me && me.principal ? me.principal.id : null;
    return !!(id && gate.requested_by === id);
  };

  function procGateState(p, gateName, target) {
    if (!target) return B("no draft", "");
    const rows = list(p.gates).filter((gate) => gate.gate === gateName && gate.target === target);
    if (rows.some((gate) => gate.status === "approved")) return B("approved", "green");
    if (rows.some((gate) => gate.status === "pending")) return B("pending · a second person decides", "amber");
    if (rows.some((gate) => gate.status === "rejected")) return B("rejected", "bad");
    return B("none · request it below", "");
  }

  async function consoleProcPublish(which) {
    const p = procState();
    if (!consoleOn()) return;
    const process = procSelected(p);
    if (!process) {
      toast("Select a draft first");
      return;
    }
    const procedure = procProcedureFor(process);
    const target = which === "procedure" ? (procedure || {}).id : process.id;
    const gateName = which === "procedure" ? "procedures.publish" : "processes.publish";
    if (!target) {
      toast("No draft names this target yet");
      return;
    }
    const gate = procApprovedGate(p, gateName, target);
    const options = gate ? { approvalId: gate.id } : {};
    p.actionError = null;
    try {
      let output;
      if (which === "procedure") {
        output = await C.action("procedures.publish", { procedure_id: target }, options);
      } else {
        output = await C.action("processes.publish", { process_id: target }, options);
      }
      p.last = { kind: "publish", output };
      delete p.detail[process.id];
      consoleRefreshProcedures();
      toast(
        gate
          ? "Published · the approved gate " + short(gate.id, 8) + " is recorded"
          : "Published · no gate was required by the tenant policy",
      );
    } catch (error) {
      p.actionError = error;
      render();
      toast(consoleError(error));
    }
  }

  const procTone = (status) =>
    status === "published" || status === "approved"
      ? "green"
      : status === "draft" || status === "pending"
        ? "amber"
        : "";

  const procErrorNote = (p) => {
    const error = p.error || p.actionError || p.detailError;
    return error ? note("<strong>Request refused</strong> · " + E(consoleError(error)), "bad") : "";
  };

  function procSelected(p) {
    if (!v.procProcess) return null;
    return p.detail[v.procProcess] || p.processes.find((row) => row.id === v.procProcess) || null;
  }

  const procField = (label, name, placeholder) =>
    `<label class="field full"><span>${E(label)}</span><input data-v2-input="${name}" placeholder="${E(placeholder)}" aria-label="${E(label)}"></label>`;
  const procText = (label, name, placeholder) =>
    `<label class="field full"><span>${E(label)}</span><textarea data-v2-input="${name}" rows="4" placeholder="${E(placeholder)}" aria-label="${E(label)}"></textarea></label>`;

  const procIntakeForm = (kind) => {
    const n = (field) => `proc-${kind}-${field}`;
    return `<div class="form-grid">${procField("Title", n("title"), "Synthetic receiving SOP")}${
      kind === "rdf" ? "" : procField("Process key", n("process-key"), "receiving")
    }${procField("Source key", n("source-key"), "receiving-sop")}${
      kind === "rdf" ? procField("Strata store id", n("store-id"), "the tenant's strata_ingest store uuid") : ""
    }${procField("Source URL", n("url"), "https://…")}${procText("Or paste the document", n("text"), "Paste the SOP text, the BPMN XML or the Turtle source")}<label class="field full"><span>Or attach a file</span><input type="file" data-v2-input="${n("file")}" aria-label="Attach the source document"></label></div>`;
  };

  const procResultPanel = (p) => {
    if (!p.last) {
      return note("No result yet. An ingest answers the source asset, the draft judge and the draft process — nothing is published.");
    }
    const kind = p.last.kind;
    const output = p.last.output || {};
    if (kind === "validate") {
      const checks = list(output.checks);
      return panel(
        "Validation",
        `${output.valid ? note("Every check passes; the human gate still decides.", "ok") : note("The draft is not valid.", "bad")}${table(
          ["Check"],
          checks.length ? checks.map((check) => `<tr><td>${E(check)}</td></tr>`) : [`<tr><td>No checks were returned.</td></tr>`],
        )}`,
      );
    }
    if (kind === "review") {
      return panel(
        "Source review",
        `${kv([
          ["Asset", `<span class="mono">${E(output.asset_id || "—")}</span>`],
          ["Status", B(output.status || "—", procTone(output.status))],
        ])}${note("A source decision is a person's act: the approval unblocks the derived drafts, and it never publishes them.")}`,
      );
    }
    if (kind === "publish") {
      return panel(
        "Published",
        `${kv([
          ["Id", `<span class="mono">${E(output.id || "—")}</span>`],
          ["Status", B(output.status || "published", procTone(output.status))],
          ["Version", E(output.version == null ? "—" : output.version)],
        ])}${note("The released version records its source and, when one was required, the human approval that authorised it.")}`,
      );
    }
    const rows = [];
    for (const key of [
      "source_asset_id",
      "procedure_id",
      "process_id",
      "document_id",
      "steps",
      "converter_version",
      "knowledge_pack_ref",
      "capability_id",
      "agent_id",
      "job_id",
    ]) {
      if (output[key] != null && output[key] !== "") {
        rows.push([key, `<span class="mono">${E(String(output[key]))}</span>`]);
      }
    }
    return panel(
      "Ingest result · " + E(kind),
      `${rows.length ? kv(rows) : note("The action returned no draft.")}${note("Every derived artefact is a draft: nothing is published and no tool grant was requested.")}`,
    );
  };

  const procProcedureRows = (p) =>
    p.procedures
      .map(
        (row) =>
          `<tr class="${row.id === v.procProcedure ? "selected" : ""}"><td><span class="mono">${E(row.key)}</span><small>${E(short(row.id, 8))}</small></td><td>${E(row.title)}</td><td>${B(row.status, procTone(row.status))}</td><td>v${E(row.version)}</td><td class="mono">${E(row.source_asset_id ? short(row.source_asset_id, 8) : "—")}</td><td class="mono">${E(row.knowledge_pack_ref ? short(row.knowledge_pack_ref, 18) : "—")}</td></tr>`,
      )
      .join("");

  const procProcessRows = (p, selectable) =>
    p.processes
      .map((row) => {
        const name = selectable
          ? `<button class="e-project-link" data-v2="proc-process" data-id="${E(row.id)}"><span><strong class="mono">${E(row.key)}</strong><small>${E(row.title)}</small></span></button>`
          : `<span><strong class="mono">${E(row.key)}</strong><small>${E(row.title)}</small></span>`;
        return `<tr class="${row.id === v.procProcess ? "selected" : ""}"><td>${name}</td><td>${B(row.status, procTone(row.status))}</td><td>v${E(row.version)}</td><td>${list(row.steps).length || "—"}</td><td class="mono">${E(row.converter_version ? short(row.converter_version, 14) : "—")}</td><td class="mono">${E(row.knowledge_pack_ref ? short(row.knowledge_pack_ref, 18) : "—")}</td></tr>`;
      })
      .join("");

  function proceduresLive() {
    const p = procState();
    if (!p.loaded && !p.loading && !p.error) consoleLoadProcedures();
    const selected = procSelected(p);
    if (v.procProcess && !p.detail[v.procProcess] && p.loadingDetail !== v.procProcess) {
      consoleLoadProcDetail(v.procProcess);
    }
    if (selected) consoleLoadProcGates(selected);
    const tab = v.tab.procedures || "Workspace";
    const selectedProcedure = selected ? procProcedureFor(selected) : null;
    const drafts = p.procedures.filter((row) => row.status === "draft");
    const published = p.procedures.filter((row) => row.status === "published");
    const steps = selected ? list(selected.steps) : [];
    let content = "";
    if (tab === "Workspace") {
      content =
        `<div class="v2-meter"><div><span>Draft procedures</span><strong>${drafts.length}</strong><small>awaiting review and the gate</small></div><div><span>Published procedures</span><strong>${published.length}</strong><small>immutable versions</small></div><div><span>Processes</span><strong>${p.processes.length}</strong><small>${p.processes.filter((row) => row.status === "draft").length} draft</small></div></div>` +
        `<div class="v2-split"><div>${panel(
          "Procedures",
          `${
            p.loading
              ? note("Loading the tenant's procedures…")
              : p.procedures.length
                ? table(["Key", "Title", "Status", "Version", "Source", "Knowledge pack"], [procProcedureRows(p)])
                : note("No procedure has been created in this tenant.")
          }${note("A converted SOP produces a draft judge (procedure) and a draft executable (process); nothing publishes itself.")}`,
          B(`${p.procedures.length} procedures`),
        )}</div><div>${panel(
          "Processes",
          `${
            p.processes.length
              ? table(["Key", "Status", "Version", "Steps", "Converter", "Knowledge pack"], [procProcessRows(p, true)])
              : note("No process has been created in this tenant.")
          }${note("Select a process to read its steps, its source and its gates.")}`,
          B(`${p.processes.length} processes`),
        )}</div></div>`;
    } else if (tab === "SOP intake") {
      content = `<div class="v2-split"><div>${panel(
        "SOP intake",
        procIntakeForm("sop"),
        C.can("procedures.ingest_sop") ? button("Ingest the SOP", "proc-ingest-sop", "", "primary", "plus") : "",
      )}</div><div>${procResultPanel(p)}</div></div>${note(
        "A customer document is untrusted input: it enters as <span class='mono'>governed_asset(kind='sop_source')</span> with provenance, and the derived procedures stay drafts behind a human source review and the publication gate. An SOP can never grant a tool.",
      )}`;
    } else if (tab === "RDF/OWL & process docs") {
      content =
        panel(
          "Ingestion paths",
          `<p class="muted">Documents are ingest material; the converter produces draft rows behind a review gate. The conveyor runs no RDF store and no reasoner: an ontology becomes a strata knowledge pack.</p>${table(
            ["Input", "Conversion", "Produces", "Gate"],
            [
              ["SOP (PDF, text, URL)", "prose → PDDL steps", "draft judge + draft process + capability/agent/job drafts", "source review · human publication gate"],
              ["BPMN 2.0 XML", "activities → steps; gateways → gate steps", "PDDL procedure draft + draft process", "source review · human publication gate"],
              ["RDF / OWL / Turtle", "submitted to strata_ingest", "a strata knowledge pack the drafts name", "the import job + review"],
            ].map((row) => `<tr>${row.map((cell) => `<td>${E(cell)}</td>`).join("")}</tr>`),
          )}`,
        ) +
        `<div class="v2-split"><div>${panel(
          "BPMN 2.0",
          procIntakeForm("bpmn"),
          C.can("procedures.ingest_process") ? button("Convert the BPMN", "proc-ingest-bpmn", "", "small", "replay") : "",
        )}</div><div>${panel(
          "RDF / OWL / Turtle",
          procIntakeForm("rdf"),
          C.can("procedures.ingest_rdf") ? button("Import the ontology", "proc-ingest-rdf", "", "small", "branch") : "",
        )}</div></div>${procResultPanel(p)}${note(
          "The diagram is a source document; the procedure is the executable artefact. A knowledge pack is pinned by reference — the conveyor never hosts embeddings.",
        )}`;
    } else if (tab === "PDDL steps") {
      content = selected
        ? `${panel(
            "PDDL steps · " + E(selected.key),
            `${
              steps.length
                ? table(
                    ["#", "Kind", "Actor", "Action", "Action text", "Preconditions", "Effects", "Judge"],
                    steps.map(
                      (step) =>
                        `<tr><td>${E(step.position)}</td><td>${E(step.kind)}</td><td>${E(step.actor)}</td><td class="mono">${E(step.action || "—")}</td><td>${E(step.action_text || "—")}</td><td class="mono">${E(JSON.stringify(step.pddl_preconditions || []))}</td><td class="mono">${E(JSON.stringify(step.pddl_effects || []))}</td><td class="mono">${E(step.procedure_id ? short(step.procedure_id, 8) : "—")}</td></tr>`,
                    ),
                  )
                : note("This draft carries no steps.")
            }`,
            C.can("procedures.validate") ? button("Validate the draft", "proc-validate", "", "small", "check") : "",
          )}${procResultPanel(p)}${note(
            "A step is an action (PDDL), a tool call or a gate. Validation checks the literals, each step's judge and that no tool grant was requested.",
          )}`
        : note("Select a draft process in Workspace first.");
    } else if (tab === "Knowledge pairing") {
      content = selected
        ? `${panel(
            "Procedure ↔ knowledge pairing",
            `${flow([
              ["Procedure", selected.key],
              ["Source asset", short(selected.source_asset_id || "—", 8)],
              ["Knowledge pack", selected.knowledge_pack_ref || "none"],
              ["Run", "pins the published version"],
            ])}${kv([
              ["Source asset", `<span class="mono">${E(selected.source_asset_id || "—")}</span>`],
              ["Knowledge pack", `<span class="mono">${E(selected.knowledge_pack_ref || "none")}</span>`],
              ["Converter", E(selected.converter_version || "—")],
              ["Recorded approval", `<span class="mono">${E(selected.approval_id || "—")}</span>`],
              ["Judge procedure", `<span class="mono">${E((selectedProcedure || {}).id || "—")}</span>`],
            ])}${note(
              "An RDF/OWL document is imported into strata_ingest and its job id is the knowledge pack a converted draft names. The conveyor hosts no RDF store, no graph store and no reasoner; a procedure pins the pack and the published version, never a mutable release.",
            )}`,
          )}${panel(
            "The source",
            `<label class="field full"><span>Review note</span><input data-v2-input="proc-review-note" placeholder="Why the source is accepted or refused" aria-label="Review note"></label>${
              C.can("procedures.review_source")
                ? `${button("Approve the source", "proc-review-approve", "", "small", "shield")}${button("Reject the source", "proc-review-reject", "", "small", "close")}`
                : note("processes:write is required to review a source.")
            }${procResultPanel(p)}`,
          )}`
        : note("Select a draft process in Workspace first.");
    } else if (tab === "Process map") {
      content = panel(
        "Process map",
        `${flow([
          ["Intent", "card"],
          ["Activities", "steps"],
          ["Gateways", "gate steps"],
          ["Evidence", "run + artefact"],
        ])}${note(
          "A BPMN diagram becomes a procedure draft: activities → steps, gateways → gate steps. The diagram is a source document, not the executable artefact.",
        )}${
          selected
            ? kv([
                ["Selected process", `<span class="mono">${E(selected.key)}</span>`],
                ["Steps", E(steps.length)],
                ["Status", B(selected.status, procTone(selected.status))],
              ])
            : ""
        }`,
      );
    } else {
      content = selected
        ? `${panel(
            "Publish readiness · " + E(selected.key),
            `${kv([
              ["Status", B(selected.status, procTone(selected.status))],
              ["Source asset", `<span class="mono">${E(selected.source_asset_id || "—")}</span>`],
              ["Recorded approval", `<span class="mono">${E(selected.approval_id || "—")}</span>`],
              ["Judge procedure", `<span class="mono">${E((selectedProcedure || {}).id || "—")}</span>`],
              ["Executable gate", procGateState(p, "processes.publish", selected.id)],
              ["Judge gate", procGateState(p, "procedures.publish", (selectedProcedure || {}).id)],
            ])}${
              C.can("approvals.request")
                ? `${button("Request the executable gate", "proc-gate-process", "", "small", "shield")}${button("Request the judge gate", "proc-gate-procedure", "", "small", "shield")}`
                : ""
            }${C.can("processes.publish") ? button("Publish the process", "proc-publish-process", "", "primary", "shield") : ""}${
              C.can("procedures.publish") ? button("Publish the judge", "proc-publish-procedure", "", "small", "shield") : ""
            }`,
          )}${panel(
            "Publication gates",
            `${
              p.gate
                ? kv([
                    ["Last request", `<span class="mono">${E(p.gate.id || "—")}</span>`],
                    ["Gate", E(p.gate.gate || "—")],
                    ["Status", B(p.gate.status || "pending", procTone(p.gate.status))],
                    ["Approvals", `${E(p.gate.approvals || 0)} / ${E(p.gate.required_decisions || 1)}`],
                  ])
                : note("No gate has been requested in this session.")
            }${
              p.gates.length
                ? table(
                    ["Gate", "Status", "Target", "Approvals", "Decision"],
                    p.gates.map(
                      (gate) =>
                        `<tr><td class="mono">${E(gate.gate)}</td><td>${B(gate.status, procTone(gate.status))}</td><td class="mono">${E(short(gate.target || "—", 8))}</td><td>${E(gate.approvals)} / ${E(gate.required_decisions)}</td><td>${
                          gate.status === "pending" && C.can("approvals.decide") && !procSelfGate(gate)
                            ? `${button("Approve", "proc-gate-approve", `data-id="${E(gate.id)}"`, "small", "check")}${button("Reject", "proc-gate-reject", `data-id="${E(gate.id)}"`, "small", "close")}`
                            : `<span class="muted">${gate.status === "pending" ? "another person decides" : "—"}</span>`
                        }</td></tr>`,
                    ),
                  )
                : note("No pending or approved gate names this process.")
            }${
              C.can("approvals.list")
                ? `<div class="toolbar">${button("Reload the gates", "proc-gate-reload", "", "small", "replay")}</div>`
                : ""
            }${note(
              "The requester cannot decide their own gate: a second person decides a listed gate, and publishing then names that approved gate. The approval id and the source are recorded on the released version. A tenant policy that waives the gate publishes without one.",
            )}${procResultPanel(p)}`,
          )}`
        : note("Select a draft process in Workspace first.");
    }
    return `<div class="page enterprise">${heading(
      "Procedures",
      "The SOP/BPMN/RDF intake, the PDDL steps and the two human publication gates; an ontology becomes a strata knowledge pack.",
      `${route("Capabilities", "capabilities", "", "layers")}`,
      "PROCEDURES & AUTHORING",
    )}${consoleRibbon(
      "T16 live: the intake writes drafts, the source review and the two publication gates are human acts.",
      true,
    )}${tabs(
      ["Workspace", "SOP intake", "RDF/OWL & process docs", "PDDL steps", "Knowledge pairing", "Process map", "Publish"],
      tab,
      "procedures",
    )}${content}${procErrorNote(p)}</div>`;
  }

  function proceduresSnapshot() {
    const config = (live && live.config) || {};
    const procedures = list(config.procedures);
    const processes = list(config.processes);
    const steps = list(config.process_steps);
    const selectedId = v.procProcess || ((processes[0] || {}).id) || null;
    const selected = processes.find((row) => row.id === selectedId) || null;
    const selectedSteps = steps
      .filter((step) => step.process_id === selectedId)
      .sort((a, b) => a.position - b.position);
    const tab = v.tab.procedures || "Workspace";
    let content = "";
    if (tab === "Workspace") {
      content =
        `<div class="v2-meter"><div><span>Procedures</span><strong>${procedures.length}</strong><small>rows in the snapshot</small></div><div><span>Processes</span><strong>${processes.length}</strong><small>${processes.filter((row) => row.status === "published").length} published</small></div><div><span>Steps</span><strong>${steps.length}</strong><small>process_step rows</small></div></div>` +
        `<div class="v2-split"><div>${panel(
          "Procedures",
          `${
            procedures.length
              ? table(
                  ["Key", "Title", "Status", "Version"],
                  procedures.map(
                    (row) =>
                      `<tr><td class="mono">${E(row.key)}</td><td>${E(row.title)}</td><td>${B(row.status, procTone(row.status))}</td><td>v${E(row.version)}</td></tr>`,
                  ),
                )
              : note("No procedure row is in this snapshot.")
          }`,
          B(`${procedures.length} procedures`),
        )}</div><div>${panel(
          "Processes",
          `${
            processes.length
              ? table(
                  ["Key", "Status", "Version"],
                  processes.map(
                    (row) =>
                      `<tr class="${row.id === selectedId ? "selected" : ""}"><td><button class="e-project-link" data-v2="proc-process" data-id="${E(row.id)}"><span><strong class="mono">${E(row.key)}</strong><small>${E(row.title)}</small></span></button></td><td>${B(row.status, procTone(row.status))}</td><td>v${E(row.version)}</td></tr>`,
                  ),
                )
              : note("No process row is in this snapshot.")
          }`,
          B(`${processes.length} processes`),
        )}</div></div>`;
    } else if (tab === "PDDL steps") {
      content = selected
        ? panel(
            "Steps · " + E(selected.key),
            `${
              selectedSteps.length
                ? table(
                    ["#", "Kind", "Actor", "Action", "Judge"],
                    selectedSteps.map(
                      (step) =>
                        `<tr><td>${E(step.position)}</td><td>${E(step.kind)}</td><td>${E(step.actor)}</td><td class="mono">${E(step.action || "—")}</td><td class="mono">${E(step.procedure_id ? short(step.procedure_id, 8) : "—")}</td></tr>`,
                    ),
                  )
                : note("This process carries no published step rows.")
            }`,
          )
        : note("Select a process in Workspace.");
    } else if (tab === "Knowledge pairing") {
      content = panel(
        "Procedure ↔ knowledge pairing",
        `${flow([
          ["Procedure", selected ? selected.key : "—"],
          ["Knowledge pack", "operator detail"],
          ["Run", "pins the published version"],
        ])}${note(
          "The public window publishes the procedure and step structure; the derivation facts (the governed source, the knowledge pack reference and the converter) are operator detail and are read after signing in.",
        )}`,
      );
    } else if (tab === "Process map") {
      content = panel(
        "Process map",
        `${flow([
          ["Intent", "card"],
          ["Activities", "steps"],
          ["Gateways", "gate steps"],
          ["Evidence", "run + artefact"],
        ])}${note("The published process_step rows are the map the snapshot carries.")}`,
      );
    } else {
      content =
        panel(
          "Ingestion paths",
          `<p class="muted">Documents are ingest material; the converter produces draft rows behind a review gate. The conveyor runs no RDF store and no reasoner: an ontology becomes a strata knowledge pack.</p>${table(
            ["Input", "Conversion", "Produces", "Gate"],
            [
              ["SOP (PDF, text, URL)", "prose → PDDL steps", "draft judge + draft process", "source review · human publication gate"],
              ["BPMN 2.0 XML", "activities → steps; gateways → gate steps", "PDDL procedure draft + draft process", "source review · human publication gate"],
              ["RDF / OWL / Turtle", "submitted to strata_ingest", "a strata knowledge pack the drafts name", "the import job + review"],
            ].map((row) => `<tr>${row.map((cell) => `<td>${E(cell)}</td>`).join("")}</tr>`),
          )}`,
        ) +
        note(
          "The intake is an operator action: sign in at the app origin to upload a document, review its source and decide its publication gate. This published page is read-only and makes no API request.",
        );
    }
    return `<div class="page enterprise">${heading(
      "Procedures",
      "The published procedure and process rows: the intake's results, their steps and their judges.",
      `${route("Capabilities", "capabilities", "", "layers")}`,
      "PROCEDURES & AUTHORING",
    )}${ribbon("Published snapshot: procedure, process and process_step rows. The intake and its gates are operator actions.", true)}${tabs(
      ["Workspace", "SOP intake", "RDF/OWL & process docs", "PDDL steps", "Knowledge pairing", "Process map", "Publish"],
      tab,
      "procedures",
    )}${content}</div>`;
  }

  function proceduresStudy() {
    const tab = v.tab.procedures || "Workspace";
    const sop = F.sops[v.sop] || F.sops[0];
    const capRows = list(db.capabilities)
      .slice(0, 5)
      .map(
        (c) =>
          `<div class="v2-step"><span class="n">${I("flow")}</span><span class="grow"><strong>${E(c.name)}</strong><small>${list(c.processes).length} procedure(s) · ${E(c.knowledge || "knowledge pin required")}</small></span>${B(c.status, c.status === "Published" ? "green" : "amber")}</div>`,
      )
      .join("");
    const sopRows = F.sops
      .map(
        (row, index) =>
          `<tr class="${index === v.sop ? "selected" : ""}"><td><button class="e-project-link" data-v2="sop" data-index="${index}"><span><strong>${E(row.name)}</strong><small>${E(row.source)}</small></span></button></td><td>${E(row.provenance)}</td><td>${E(row.pages)}</td><td>${B(row.state, row.state === "Published" ? "green" : "amber")}</td></tr>`,
      )
      .join("");
    let content = "";
    if (tab === "Workspace") {
      content =
        flow([
          ["Capability", "what the tenant can do"],
          ["Procedure", "published, versioned, pinned"],
          ["Step", "PDDL action | tool call | gate"],
          ["Knowledge pack", "pinned in strata_rag"],
        ]) +
        panel(
          "Capability → procedure workspace",
          capRows,
          button("Open the procedure editor", "navigate", 'data-page="processes"', "small", "flow"),
        ) +
        note(
          "Procedure design and management is first-class (D47): ordered steps, each an action (PDDL), a tool call or a gate. A published procedure is immutable; editing creates a draft and a gate.",
        );
    } else if (tab === "SOP intake") {
      const detail = kv([
        ["Source", E(sop.source)],
        ["Provenance", E(sop.provenance)],
        ["State", B(sop.state, sop.state === "Published" ? "green" : "amber")],
        ["Outputs", "draft judge · PDDL procedure draft · knowledge pack reference"],
      ]) +
        note(
          "A customer document is untrusted input: it enters as <span class='mono'>governed_asset(kind='sop_source')</span> with provenance, never as instructions to the system.",
        ) +
        button("Publish this procedure", "sop-publish", "", "small", "shield") +
        (v.review === "sop-published"
          ? note("<strong>Published behind the review gate</strong> · the procedure release names the knowledge pack. A batch never publishes itself.", "ok")
          : v.review === "sop-refused"
            ? note("<strong>422 review_required</strong> · the SOP-derived artefacts are still drafts. An unreviewed SOP never binds a job.", "bad")
            : "");
      content = `<div class="v2-split"><div>${panel(
        "SOP intake · single or batch",
        table(["SOP", "Source", "Pages", "State"], [sopRows]),
        `${button("Start batch intake", "sop-intake", "", "small", "plus")}${button("Advance conversion", "sop-advance", "", "small", "replay")}`,
      )}</div><div>${panel(E(sop.name), detail)}</div></div>`;
    } else if (tab === "RDF/OWL & process docs") {
      content =
        panel(
          "Ingestion paths",
          `<p class="muted">Documents are ingest material; the converter produces draft rows behind a review gate. The conveyor runs no RDF store and no reasoner: an ontology becomes a strata knowledge pack.</p><div class="e-table-scroll"><table class="v2-matrix"><thead><tr><th>Input</th><th>Conversion</th><th>Produces</th><th>Gate</th></tr></thead><tbody><tr><td>SOP (PDF, URL, document)</td><td>prose → PDDL steps</td><td>draft judge + draft process</td><td>review · publish</td></tr><tr><td>RDF / OWL / Turtle</td><td>submitted to strata_ingest</td><td>a strata knowledge pack</td><td>the import job + review</td></tr><tr><td>BPMN 2.0 XML</td><td>activities → steps; gateways → gates</td><td>PDDL procedure draft</td><td>review · publish</td></tr><tr><td>Visio / draw.io export</td><td>activities → steps; gateways → gates</td><td>procedure draft</td><td>review · publish</td></tr><tr><td>Existing policy / procedure export</td><td>text curation</td><td>procedure or policy draft</td><td>review · publish</td></tr></tbody></table></div>`,
          button("Convert the sample BPMN", "convert-doc", "", "small", "replay"),
        ) +
        note("The diagram is a source document; the procedure is the executable artefact. The same review gate applies to a single document and a batch.");
    } else if (tab === "PDDL steps") {
      content =
        panel(
          "Action space · PDDL",
          `<div class="v2-code">(:action implement_change
  :parameters (?job - role ?card - work_package)
  :precondition (and (approved ?card) (knowledge-ready delivery))
  :effect (and (implemented ?card) (evidence-required ?card)))</div>${note("The action space is the procedure's steps: each step is an action (PDDL), a tool call or a gate. The knowledge space is a pinned pack, not a store the conveyor hosts.")}${button("Validate the step", "pddl-check", "", "small", "check")}`,
          B("procedure v7 · PDDL", "green"),
        ) +
        panel(
          "Steps in proc-implement v7",
          `${[
            "01 · read the card and pinned sources (action)",
            "02 · draft the change (internal_agent · generation)",
            "03 · run tests (tool call)",
            "04 · independent review (gate)",
            "05 · merge (gate · human)",
          ]
            .map((line) => {
              const [n, ...rest] = line.split(" · ");
              return `<div class="v2-step"><span class="n">${E(n)}</span><span class="grow">${E(rest.join(" · "))}</span></div>`;
            })
            .join("")}${route("Open the process editor", "processes", "small", "flow")}`,
        );
    } else if (tab === "Knowledge pairing") {
      content = panel(
        "Procedure ↔ knowledge pairing",
        `${flow([
          ["Procedure v7", "pinned"],
          ["Knowledge pack", "capability@17 · strata_rag"],
          ["Run", "pins both"],
        ])}${kv([
          ["Domain pack", "capability@17 · published · required"],
          ["Source provenance", "the governed sop_source is reviewed before the draft publishes"],
          ["Readiness", v.packReady ? B("ready", "green") : B("blocked · capability_not_ready", "amber")],
          ["Retrieval", "pinned leaf ids · strata_rag owns embeddings"],
          ["Consolidation", "one cold digest per card in strata_rag"],
        ])}${note("The procedure names the knowledge pack its steps read. The conveyor never hosts embeddings or a model; retrieval is strata_rag.")}${route("Open knowledge design", "knowledge-design", "small", "branch")}`,
        B("pairing required", "green"),
      );
    } else if (tab === "Process map") {
      content = panel(
        "Process map",
        `${flow([
          ["Intent", "card"],
          ["Activities", "steps"],
          ["Gateways", "approvals"],
          ["Evidence", "run + artefact"],
        ])}${note("A BPMN diagram becomes a procedure draft: activities → steps, gateways → gates. The diagram is a source document, not the executable artefact.")}${route("Open the existing process map", "processes", "small", "flow")}`,
      );
    } else {
      const checks = [
        "Review gate: human reviewer approved the draft",
        "Knowledge pack published and readable",
        "Tool grants unchanged by the SOP",
        "No SOP widened an effect ceiling",
      ]
        .map((x, i) => {
          const mark = i === 1 && !v.packReady ? "—" : "✓";
          return `<div class="v2-step"><span class="n">${mark}</span><span class="grow">${E(x)}</span></div>`;
        })
        .join("");
      content = panel(
        "Publish readiness",
        `${checks}${
          v.packReady
            ? note("All readiness checks pass. Publish opens a human gate; publication is not automatic.", "ok")
            : note("<strong>409 procedure_not_ready</strong> · the required knowledge pack capability@18 is unpublished. Publishing is refused before any run.", "bad")
        }`,
        `${button(v.packReady ? "Mark pack unpublished" : "Toggle pack availability", "toggle-pack", "", "small")}${button("Publish procedure v8", "publish-procedure", "", "primary", "shield")}`,
      );
    }
    return `<div class="page enterprise">${heading(
      "Procedures",
      "Capability → procedure → step, with SOP/RDF/OWL and BPMN/Visio/draw.io intake, the PDDL action space and the knowledge pack pairing.",
      `${route("Capabilities", "capabilities", "", "layers")}${route("Authoring", "authoring", "", "book")}`,
      "PROCEDURES & AUTHORING",
    )}${ribbon(
      "V2 procedure spine: procedure/procedure_step with PDDL text and a knowledge pack reference, the source review and the human publication gate. Proposed in T16.",
    )}${tabs(
      ["Workspace", "SOP intake", "RDF/OWL & process docs", "PDDL steps", "Knowledge pairing", "Process map", "Publish"],
      tab,
      "procedures",
    )}${content}</div>`;
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
        `<div class="v2-split"><div>${panel("Intake queue · chat + documents", `${rows}<div class="form-grid" style="margin-top:12px"><label class="field full"><span>Chat brief</span><textarea data-v2-input="brief" rows="3" placeholder="Describe the department, jobs, procedures or policies you need">${E(v.brief || "")}</textarea></label><label class="field full"><span>Document</span><input type="file" aria-label="Attach a document" disabled><small>Local study · no file leaves the browser</small></label></div>`, button("Draft from the brief", "intake-chat", "", "primary", "plus"))}</div><div>${panel("What the intake produces", `${flow([["Chat / document", "untrusted input"], ["Authoring job", "ordinary job"], ["Asset drafts", "review gate"], ["Publication", "a release and a gate"]])}${note("Every output is a draft behind a review gate (tenant-configurable, default human). A chat message or document can never grant a tool, widen an effect ceiling, approve its own artefact or change an approval policy.")}${note("Asset order is dependency-ordered: industry → department → job/capability → procedure → knowledge pack → policies/templates → tools. A draft whose referenced asset is unpublished is refused.")}`)}</div></div>`;
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
          ["Procedure", "review"],
          ["Knowledge pack", "unpublished"],
          ["Policies / templates", "draft"],
          ["Tools", "request only"],
        ]) +
        panel("Review depth per asset", `${table(["Asset class", "Review", "Publication"], [["Organisation (industry, department)", "human · tenant admin", "release + gate"], ["Job catalogue / job draft", "human · department owner", "release + binding"], ["Capability / procedure", "human · domain reviewer", "versioned release"], ["Knowledge pack", "human · knowledge owner", "pinned pack version"], ["Policy / approval", "human · governance", "gated policy change"], ["Tool request", "security review", "tool publication"]].map((r) => `<tr><td>${E(r[0])}</td><td>${E(r[1])}</td><td>${E(r[2])}</td></tr>`))}${note("A person reviews and publishes. The authoring job family is ordinary jobs on the two executors; the pipeline is a procedure, not a hard-coded wizard.")}`);
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
  // The live read of the T7 incident record (SPEC_V2 §9.2, #434). An incident is an `incident`
  // card: `ops.incident|triage|mitigate|postmortem|resolve` write its meta, and the published
  // snapshot carries the service, severity, the four timestamps and the runbook/postmortem
  // titles. Read-only by the owner's decision — every write stays on the operator/agent path.
  function operations() {
    if (live && live.ops) return operationsLive();
    return operationsStudy();
  }

  function operationsLive() {
    const O = live.ops || {};
    const incidents = list(O.incidents);
    const OPERATIONS_TABS = [
      "Incidents",
      "Runbooks",
      "Postmortems",
      "On-call",
      "Read & write",
      "Andon",
    ];
    const tab = OPERATIONS_TABS.includes(v.tab.operations) ? v.tab.operations : "Incidents";
    // The state is derived from the four timestamps the lifecycle writes; never a guessed field.
    const stateOf = (i) =>
      i.resolved_at ? "resolved" : i.mitigated_at ? "mitigated" : i.triaged_at ? "triaging" : "open";
    const toneOf = (state) =>
      state === "resolved"
        ? "green"
        : state === "open"
          ? "amber"
          : state === "mitigated"
            ? "purple"
            : "";
    const when = (value) =>
      value ? E(String(value).slice(0, 19).replace("T", " ")) : "—";
    const inc = incidents.find((i) => i.id === v.incident) || incidents[0] || null;
    let content = "";
    if (tab === "Incidents") {
      const rows = incidents.map(
        (i) =>
          `<tr class="${inc && i.id === inc.id ? "selected" : ""}"><td><button class="e-project-link" data-v2="incident" data-id="${E(i.id)}"><span><strong>${E(i.name)}</strong><small>${E(i.service || "—")}</small></span></button></td><td>${B(i.severity || "—", i.severity === "sev1" ? "bad" : "amber")}</td><td>${when(i.detected_at)}</td><td>${when(i.resolved_at)}</td><td>${B(stateOf(i), toneOf(stateOf(i)))}</td></tr>`,
      );
      content = inc
        ? `<div class="v2-split"><div>${panel(
            "Incidents",
            `${table(["Incident", "Severity", "Detected", "Resolved", "State"], rows)}`,
            B(`${incidents.length} incident${incidents.length === 1 ? "" : "s"}`, "green"),
          )}</div><div>${panel(
            E(inc.name),
            `${kv([
              ["Service", E(inc.service || "—")],
              ["Severity", B(inc.severity || "—", inc.severity === "sev1" ? "bad" : "amber")],
              ["State", B(stateOf(inc), toneOf(stateOf(inc)))],
              ["Detected", when(inc.detected_at)],
              ["Triaged", when(inc.triaged_at)],
              ["Mitigated", when(inc.mitigated_at)],
              ["Resolved", when(inc.resolved_at)],
              [
                "Runbook",
                inc.runbook_key
                  ? `<span class="mono">${E(inc.runbook_key)}</span> · ${E(inc.runbook_title || "")}`
                  : "—",
              ],
              [
                "Postmortem",
                inc.postmortem_asset_id
                  ? E(inc.postmortem_title || inc.postmortem_asset_id)
                  : "—",
              ],
            ])}${note(
              "Read-only: the lifecycle is written by <span class='mono'>ops.incident</span>, <span class='mono'>ops.triage</span>, <span class='mono'>ops.mitigate</span>, <span class='mono'>ops.postmortem</span> and <span class='mono'>ops.resolve</span>. The severity and the runbook selection come from the published set; a mitigation is a destructive action under the tenant's approvals.",
            )}`,
          )}</div></div>`
        : panel(
            "Incidents",
            note(
              "No incident is in the published record. An incident is opened by <span class='mono'>ops.incident</span>; this screen is the read side only.",
            ),
          );
    } else if (tab === "Runbooks") {
      const groups = new Map();
      incidents.forEach((i) => {
        if (!i.runbook_procedure_id) return;
        const row = groups.get(i.runbook_procedure_id) || {
          key: i.runbook_key || i.runbook_procedure_id,
          title: i.runbook_title || "",
          count: 0,
          last: i.detected_at || "",
        };
        row.count += 1;
        if ((i.detected_at || "") > row.last) row.last = i.detected_at || "";
        groups.set(i.runbook_procedure_id, row);
      });
      const rows = [...groups.values()].map(
        (r) =>
          `<tr><td class="mono">${E(r.key)}</td><td>${E(r.title || "—")}</td><td>${r.count}</td><td>${when(r.last)}</td></tr>`,
      );
      content = groups.size
        ? panel(
            "Runbooks",
            `${table(["Runbook", "Procedure title", "Incidents", "Last detected"], rows)}${note(
              "A runbook is a <strong>published procedure</strong> (SPEC_V2 §9.2). The incident records the selection; the procedure body stays on the Documents surface.",
            )}`,
          )
        : panel(
            "Runbooks",
            note(
              "No incident has selected a runbook yet. A triage names one published procedure; nothing else is a runbook.",
            ),
          );
    } else if (tab === "Postmortems") {
      const withPostmortem = incidents.filter((i) => i.postmortem_asset_id);
      const rows = withPostmortem.map(
        (i) =>
          `<tr><td>${E(i.postmortem_title || i.postmortem_asset_id)}</td><td>${E(i.name)}</td><td>${when(i.resolved_at)}</td><td>${B(i.resolved_at ? "resolved" : "open", i.resolved_at ? "green" : "amber")}</td></tr>`,
      );
      content = withPostmortem.length
        ? panel(
            "Postmortems",
            `${table(["Postmortem", "Incident", "Resolved", "State"], rows)}${note(
              "One postmortem per incident: a governed asset linked from the card. A systemic finding opens exactly one kata candidate.",
            )}`,
          )
        : panel(
            "Postmortems",
            note(
              "No incident carries a postmortem yet. One closes each resolved incident, and only after a mitigation.",
            ),
          );
    } else if (tab === "On-call") {
      content =
        panel(
          "On-call and escalation",
          `${flow([["Alert", "detect"], ["Laya triage", "severity + runbook"], ["On-call", "human"], ["Break-glass", "tenant-configured"], ["Post-hoc review", "mandatory"]])}${kv([["Default autonomy", "human approval"], ["Break-glass", "incident commander · tenant-configured · post-hoc review"], ["Escalation", "card → parent WBS → PM → programme; coach can escalate to a human"], ["Pages", "still a human decision"]])}`,
        ) +
        note(
          "The full write set ships in v1; autonomy is configuration, not a reduced feature. A destructive action without the tenant's configured approvals is refused.",
        );
    } else if (tab === "Read & write") {
      content = panel(
        "Kubernetes and git verbs",
        `${table(["Verb", "Effect class", "Gate"], [["pods · deployments · events · logs · metrics · Flux status · git/CI status", "read", "none · audit only"], ["rollout restart", "destructive", "dry-run + dual control + read-back"], ["scale", "destructive", "dry-run + dual control + read-back"], ["rollback", "destructive", "dry-run + dual control + read-back"], ["feature-flag flip", "destructive", "dry-run + dual control + read-back"], ["PR / branch / merge", "write | destructive", "job authority · merge gate"]].map((r) => `<tr><td>${E(r[0])}</td><td>${effects(r[1].split(" | "))}</td><td>${E(r[2])}</td></tr>`))}${note("The ops connector uses a namespace-scoped service account, never cluster-admin. The action registry declares every verb; the tool version carries the effect class, not the HTTP method.")}`,
        B("read-first", "green"),
      );
    } else {
      content = panel(
        "Andon cord",
        `${note(
          "The andon cord is an incident-class action, not a pause button for one run: it stops new runs for a tenant or programme, keeps the record and alerts the on-call. Only a person releases it, and the release is recorded. A failed kata keeps it pulled.",
        )}${kv([["Scope", "tenant or programme"], ["Effect", "stops new runs · keeps the record · alerts on-call"], ["Release", "a person only"], ["Related", "a failed kata keeps the cord until released"]])}`,
      );
    }
    return `<div class="page enterprise">${heading("Operations", "Incidents, on-call, runbooks, read-first k8s/git actions and postmortems — the platform perspective as a job family.", `${route("Improve", "improve", "", "chart")}${route("Tools", "toolkits", "", "tools")}`, "OPERATIONS")}${consoleRibbon(
      "The published incident record: service, severity, the four lifecycle timestamps, the runbook selection and the linked postmortem, read from the live snapshot. Read-only — writes stay on the operator/agent path.",
      true,
    )}${tabs(OPERATIONS_TABS, tab, "operations")}${content}</div>`;
  }

  // DESIGN mode: the SPEC_V2 study with local fixtures, kept for the checkout with no snapshot.
  function operationsStudy() {
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
  // Live: the platform publishes `measure.inference` (calls, tokens, derived cost, tool spend and
  // cache hits by caller and by model) and a `note` that fixes the cost rule. The screen renders
  // those rows and nothing else; the design study's fixture roll-ups appear only where no
  // snapshot is loaded, and a snapshot without the section says so rather than borrowing a
  // fixture number (#458).
  function metering() {
    if (live) return meteringLive();
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

  /// The live Metering screen: `measure.inference` from the published snapshot, or one honest
  /// sentence when the snapshot carries none. No fixture roll-up, no invented cost figure — the
  /// snapshot's own `note` states the cost rule (it is derived at read time, never stored).
  function meteringLive() {
    const measure = live.measure || {};
    const inference = measure.inference || null;
    const num = (value) => Number(value || 0).toLocaleString("en-US");
    const stat = (label, value, sub) =>
      `<div class="metric"><div class="label"><i class="dot green"></i>${E(label)}</div><div class="metric-number">${E(value)}</div><small>${E(sub)}</small></div>`;
    const head = `${heading("Metering", "The platform's own inference aggregates, read from the published snapshot: calls, tokens, derived cost, tool spend and cache hits by caller and by model.", `${route("Portfolio", "portfolio", "", "layers")}${route("Usage", "usage", "", "chart")}`, "MEASURE & IMPROVE")}${ribbon("Live measurement: <span class=\"mono\">measure.inference</span> from the published snapshot. Cost is a read-time derivation, never a stored figure.", true)}`;
    const ruled = measure.note ? note(E(measure.note)) : "";
    if (!inference) {
      return `<div class="page enterprise">${head}${panel(
        "No measurement published",
        `<p class="muted">This snapshot carries no <span class="mono">measure.inference</span> section, so there is nothing to report. The screen shows no fixture number in that case.</p>`,
      )}${ruled}</div>`;
    }
    const totals = inference.totals || {};
    // Money is a read-time derivation (D52), published as a rounded JSON number. Small spend is
    // shown to four decimals so a real $0.00028 is not rendered as "$0.00"; an absent figure is
    // "—", never a claimed zero.
    const usd = (value) => {
      const n = Number(value);
      if (!Number.isFinite(n)) return "—";
      return "$" + n.toFixed(n > 0 && n < 0.01 ? 4 : 2);
    };
    const tiles = `<div class="metrics">${[
      stat("Calls", num(totals.calls), `${num(totals.non_ok_calls)} not ok`),
      stat(
        "Tokens",
        num(totals.total_tokens),
        `${num(totals.prompt_tokens)} prompt · ${num(totals.completion_tokens)} completion`,
      ),
      stat("Cost", usd(totals.cost_usd), "derived at read time (D52)"),
      stat("Tool spend", usd(totals.tool_cost_usd), "tool_call ledger"),
      stat(
        "Cache hits",
        num(totals.cache_hit_calls),
        `${num(totals.cache_miss_calls)} misses · ${num(totals.cache_write_calls)} writes · ${num(totals.unknown_cache_calls)} unknown`,
      ),
      stat("Unpriced calls", num(totals.unpriced_calls), "no price row — counted, never free"),
    ].join("")}</div>`;
    // The derivation is stated, not implied. A budget percentage needs a declared ceiling; the
    // snapshot publishes no budget, so none is invented.
    const derivation = note(
      "Cost is derived at read time from <span class='mono'>inference_call</span> × <span class='mono'>commercial_price</span> — priced by token or by usage unit (D52) — and is never a stored column; tool spend is the <span class='mono'>tool_call</span> ledger's recorded cost. A call the price join cannot price is counted as unpriced, never rendered as free. Budget utilisation needs a declared ceiling: the snapshot publishes no budget, so no percentage is shown.",
    );
    const callerRows = list(inference.by_caller).map(
      (row) =>
        `<tr><td>${E(row.caller_kind || "unknown")}</td><td>${num(row.calls)}</td><td>${num(row.total_tokens)}</td></tr>`,
    );
    const modelRows = list(inference.by_model).map(
      (row) =>
        `<tr><td>${E(row.model || "unknown")}</td><td>${num(row.calls)}</td><td>${num(row.prompt_tokens)}</td><td>${num(row.completion_tokens)}</td><td>${num(row.total_tokens)}</td><td>${num(row.cache_hits)}</td></tr>`,
    );
    return `<div class="page enterprise">${head}${tiles}${derivation}${panel(
      "By caller",
      callerRows.length
        ? table(["Caller", "Calls", "Tokens"], callerRows)
        : `<p class="muted">The snapshot carries no caller rows.</p>`,
    )}${panel(
      "By model",
      modelRows.length
        ? table(
            ["Model", "Calls", "Prompt tokens", "Completion tokens", "Total tokens", "Cache hits"],
            modelRows,
          )
        : `<p class="muted">The snapshot carries no model rows.</p>`,
    )}${ruled}</div>`;
  }

  /* ------------------------------------------------------------------ Contribute */
  // The public path into the programme. It reads the issue registry at publish and writes
  // nothing; a claim and a design report both happen on GitHub.
  function contributePage() {
    const signed = consoleOn();
    if (signed && !cs.loaded && !cs.loading && !cs.error) consoleLoad();
    const registry = signed
      ? cs.issues
      : list((((live || {}).programme || {}).issue_registry || {}).issues);
    const openRows = registry.filter((issue) => (issue.state || "OPEN") === "OPEN");
    const withLabel = (label) =>
      openRows.filter((issue) => issueLabels(issue).includes(label));
    const ready = withLabel("status:ready");
    const claimed = withLabel("status:claimed");
    const blocked = withLabel("status:blocked");
    const rows = signed ? consoleProjects() : snapshotProjects();
    const owner = new Map();
    rows.forEach((project) =>
      list((project.meta || {}).issues).forEach((number) =>
        owner.set(String(number), project),
      ),
    );
    const groups = new Map();
    ready.forEach((issue) => {
      const project = owner.get(String(issue.number));
      const key = project ? project.key : "Unassigned";
      if (!groups.has(key)) groups.set(key, { project, issues: [] });
      groups.get(key).issues.push(issue);
    });
    const panels = [...groups.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, group]) =>
        panel(
          `${E(key)} · ready units`,
          table(
            ["Issue", "Unit", "Labels"],
            group.issues.map(
              (issue) =>
                `<tr><td><a class="e-project-link" href="${E(
                  issue.url || consoleIssueUrl(issue.number),
                )}" target="_blank" rel="noopener">#${E(issue.number)} ↗</a></td><td><strong>${E(
                  issue.title,
                )}</strong></td><td><small>${E(issueLabels(issue).join(" · "))}</small></td></tr>`,
            ),
          ),
          B(`${group.issues.length} ready`, "green"),
        ),
      );
    const summary = `<div class="metrics">${[
      [openRows.length, "Open units", `${claimed.length} claimed · ${blocked.length} blocked`, "green"],
      [ready.length, "Marked ready", "published issue snapshot; verify on GitHub", "purple"],
      [
        blocked.length,
        "Blocked",
        "a plain Blocker: line names an open issue",
        blocked.length ? "orange" : "green",
      ],
    ]
      .map(
        ([value, label, sub, tone]) =>
          `<div class="metric"><div class="label"><i class="dot ${tone}"></i>${E(
            label,
          )}</div><div class="metric-number">${E(value)}</div><small>${E(sub)}</small></div>`,
      )
      .join("")}</div>`;
    const startHere = panel(
      "Choose a first contribution",
      `${table(
        ["Time", "Path", "Start here"],
        [
          ["5 min", "Report design friction", "Name the page, what you expected and what happened. A screenshot is optional."],
          ["15–30 min", "Review the proposal", "Find one unclear term, a missing decision or an assumption that fails in practice."],
          ["30+ min", "Improve docs or code", "Take a maintainer-triaged unit with acceptance criteria. A typo, a broken link or a docs clarification can go straight to a pull request."],
        ].map((row) => `<tr><td><strong>${E(row[0])}</strong></td><td>${E(row[1])}</td><td>${E(row[2])}</td></tr>`),
      )}${note(
        `V1 is the live build record; <a href="https://github.com/nonagenticai/conveyor/blob/main/docs/SPEC_V2.md" target="_blank" rel="noopener">SPEC_V2 defines V2 ↗</a> and this site marks what is live and what is still proposed. AI coding assistants are welcome: you review the diff, run the checks and sign off your own commits. Every merged contribution is credited in <a href="https://github.com/nonagenticai/conveyor/blob/main/CONTRIBUTORS.md" target="_blank" rel="noopener">CONTRIBUTORS.md ↗</a> unless the contributor opts out. See the <a href="https://github.com/nonagenticai/conveyor/blob/main/CONTRIBUTING.md" target="_blank" rel="noopener">contribution workflow ↗</a>.`,
      )}<div class="toolbar"><a class="btn" href="https://github.com/nonagenticai/conveyor/issues/new?template=feedback.yml" target="_blank" rel="noopener">Send design feedback ↗</a><a class="btn" href="https://github.com/nonagenticai/conveyor/blob/main/docs/SPEC_V2.md" target="_blank" rel="noopener">Review the V2 proposal ↗</a><a class="btn" href="https://github.com/nonagenticai/conveyor/issues?q=is%3Aissue+is%3Aopen+label%3Aunit" target="_blank" rel="noopener">Browse unit issues ↗</a><a class="btn" href="https://github.com/nonagenticai/conveyor/blob/main/CONTRIBUTING.md" target="_blank" rel="noopener">Contribution workflow ↗</a></div>`,
      B("START HERE", "purple"),
    );
    const protocol = panel(
      "How a unit is claimed",
      `<p class="muted">A change that does not alter behaviour — a typo, a broken link, a docs clarification — can go straight to a pull request; no claim is needed.</p><p class="muted">For a unit, the claim is four empty checks, a pushed branch and a comment. Until all four are empty the unit is someone else's; join it, never start a parallel branch.</p><pre class="v2-code">gh issue view N --json state,labels,comments,closedByPullRequestsReferences
gh pr list --state all --search "#N"
git ls-remote --heads origin "issue/N-*"
git log origin/main --grep "#N"</pre><p class="muted">Then push <span class="mono">issue/N-&lt;slug&gt;</span>, add <span class="mono">status:claimed</span> and a comment naming the branch and HEAD. One issue is one branch and exactly one PR with <span class="mono">Closes #N</span>. A contributor without write access comments with the plan; a maintainer applies the claim.</p><div class="toolbar"><a class="btn" href="https://github.com/nonagenticai/conveyor/issues/new?template=unit.md" target="_blank" rel="noopener">Propose a unit ↗</a><a class="btn" href="https://github.com/nonagenticai/conveyor/issues/new?template=feedback.yml" target="_blank" rel="noopener">Report design feedback ↗</a><a class="btn" href="${E(
        (live || {}).feedback_url || "https://github.com/nonagenticai/conveyor/discussions",
      )}" target="_blank" rel="noopener">Discuss ↗</a><a class="btn" href="docs/plans/03-DELIVERY.html">Delivery contract ↗</a></div>`,
      B(`${ready.length} ready`, ready.length ? "green" : ""),
    );
    return `<div class="page enterprise">${heading(
      "Contribute",
      "Start with design feedback, review the proposal, or take an accepted unit of work; a small fix can go straight to a pull request. Claims and code live on GitHub.",
      `${route("Projects", "projects", "", "projects")}`,
      "PORTFOLIO & PLAN",
    )}${consoleRibbon(
      "Issue status is a published snapshot, not a live claim lock. Verify the issue, branch and PR on GitHub before starting; this page writes nothing.",
      registry.length > 0,
    )}${summary}${startHere}${
      panels.join("") ||
      note(
        "No ready unit is published right now. A unit becomes <span class='mono'>status:ready</span> when its dependencies are met.",
      )
    }${protocol}</div>`;
  }

  // T19: the decision forum. A meeting is a card, never a chat; the record is rows — agenda,
  // attendance, positions and the linked approval's decision rows. This page is the design
  // study's reference for the real `/meetings` destination (SPEC_V2 §6.8, D66).
  function meetingsPage() {
    const agenda = [
      ["1", "approval", "The tranche-4 exit review", "baselines, benefit status", "sponsoring group", "decision"],
      ["2", "register_item", "Risk: the only Postgres standby", "the register row, the last restore", "operations, architecture", "action"],
      ["3", "asset", "Waiver: the T9 contract is unapproved", "the waiver, the compliance review", "architecture", "commitment"],
      ["4", "cadence", "PI planning: the next increment", "the backlog by WSJF, capacity", "programme, engineering", "information"],
    ];
    const seats = [
      ["chair", "governance_body · Architecture Board", "accepted"],
      ["member", "job_binding · Enterprise architect", "accepted"],
      ["member", "app_user · Head of operations", "accepted"],
      ["observer", "job_binding · Audit", "tentative"],
    ];
    const positions = [
      ["Enterprise architect", "approve", "The evidence is complete; the dissent is the standby risk."],
      ["Head of operations", "approve_with_conditions", "Condition: the restore drill is booked before exit."],
      ["Audit (observer)", "request_evidence", "Recorded, never counted."],
    ];
    return `<div class="page enterprise">${heading(
      "Meetings",
      "The decision forum: agenda items resolve existing approvals, seats record typed positions, and the outcome is arithmetic over both.",
      route("Portfolio", "portfolio", "", "layers"),
      "PORTFOLIO &amp; PLAN",
    )}${note(
      "A meeting is a card, never a chat. Its DoD is every agenda item closed with an outcome and the minutes published. The LLM drafts the pre-read and the minutes; it never selects an item, decides, infers a position or marks attendance.",
    )}<div class="grid two">${panel(
      "The room",
      table(
        ["#", "Resolved", "Item", "Evidence", "Departments", "Outcome"],
        agenda.map(
          (row) =>
            `<tr>${row.map((cell, i) => `<td>${i === 0 ? `<span class="mono">${E(cell)}</span>` : E(cell)}</td>`).join("")}</tr>`,
        ),
      ),
    )}${panel(
      "Seats",
      table(
        ["Seat", "Participant", "Response"],
        seats.map(
          (row) =>
            `<tr><td>${E(row[0])}</td><td>${E(row[1])}</td><td>${E(row[2])}</td></tr>`,
        ),
      ),
    )}</div><div class="grid two">${panel(
      "Positions — the live rows",
      table(
        ["Participant", "Stance", "Rationale"],
        positions.map(
          (row) =>
            `<tr><td>${E(row[0])}</td><td>${E(row[1])}</td><td>${E(row[2])}</td></tr>`,
        ),
      ) +
        note(
          "One live position per participant per item; a changed stance supersedes the old row. An observer's position is kept and never counted. Every qualifying seat becomes one <span class='mono'>approval_decision</span> row with <span class='mono'>via='meeting'</span>.",
        ),
    )}${panel(
      "The outcome arithmetic",
      flow([
        ["Quorum &amp; required departments", "missing · inquorate"],
        ["The meeting's rule", "unanimous · majority · chair"],
        ["The approval's policy", "dual control · evidence · P4"],
        ["The stricter result wins", "rows, dissent and minutes"],
      ]) + kv([
        ["Agenda conversion", "75% — three of four items decided"],
        ["Quorum-miss rate", "0% — every required department seated"],
        ["Decision latency", "36 minutes from open to the first decision"],
        ["Commitment slippage", "0 — the condition is due before exit"],
        ["Action closure", "one action card, open"],
        ["Person-hours per decision", "1.5 — four seats × 1.2 h ÷ 3 decisions"],
        ["Decisions per meeting", "3"],
      ]),
    )}</div>${panel(
      "The framework forums",
      table(
        ["Framework", "Procedure", "Convener", "Agenda sources", "Rule"],
        [
          ["MSP", "programme board", "sponsoring group / SRO", "register, benefit-at-risk, tranche decisions", "chair"],
          ["MSP", "tranche review", "sponsoring group", "baselines, benefit status, risk register", "unanimous or chair"],
          ["TOGAF", "Architecture Board sitting", "governance_body", "contracts, compliance, waivers, change requests", "majority"],
          ["SAFe", "PI planning", "programme manager", "backlog by WSJF, capacity", "majority"],
          ["SAFe", "inspect &amp; adapt", "coach", "retro findings, lessons, kata candidates", "majority"],
        ].map(
          (row) =>
            `<tr>${row.map((cell) => `<td>${E(cell)}</td>`).join("")}</tr>`,
        ),
      ) +
        note(
          "The same <span class='mono'>meeting_type</span> mechanism serves all three frameworks. A forum whose instances produce no decision over a cadence cycle is flagged and retired.",
        ),
    )}<div class="grid two">${panel(
      "Forum policies",
      table(
        ["Key", "Version", "Decision rule", "Quorum", "Evidence minimum", "Status"],
        [
          ["architecture_board", "v3", "majority", "4", "2", "active"],
          ["programme_board", "v2", "chair", "3", "1", "active"],
          ["pi_planning", "v1", "majority", "3", "0", "active"],
        ].map(
          (row) =>
            `<tr>${row.map((cell) => `<td>${E(cell)}</td>`).join("")}</tr>`,
        ),
      ) +
        note(
          "A policy change is a new version through the authoring route; in-flight meetings keep the version they pinned.",
        ),
    )}${panel(
      "Board sitting",
      kv([
        ["Policy", "architecture_board · v3 — majority, quorum 4"],
        ["Linked approval", "APR-014 — the T9 contract is unapproved"],
        ["Subject", "the T9 interface contract, contract version 2"],
        ["Seats", "chair · enterprise architect · head of operations · audit (observer)"],
      ]) +
        note(
          "Every qualifying seat writes one <span class='mono'>approval_decision</span> with <span class='mono'>via='meeting'</span>; an observer's position is kept and never counted.",
        ),
    )}</div>${panel(
      "Cadence",
      table(
        ["Cadence row", "Meeting type", "Next occurrence", "Materialises"],
        [
          ["Architecture Board — monthly", "architecture_board v3", "2026-12-03 14:00 UTC", "one meeting"],
          ["PI planning — quarterly", "pi_planning v1", "2026-12-07 09:00 UTC", "one meeting"],
        ].map(
          (row) =>
            `<tr>${row.map((cell) => `<td>${E(cell)}</td>`).join("")}</tr>`,
        ),
      ) +
        note(
          "A cadence row names its <span class='mono'>meeting_type</span>; <span class='mono'>cadences.materialise</span> materialises exactly one meeting for that occurrence.",
        ),
    )}</div>`;
  }

  const pages = {
    portfolio,
    programme: programmeDefinitionPage,
    adm: architectureCapabilityPage,
    plan,
    validation,
    projects: projectsPage,
    outputs: outputsPage,
    contribute: contributePage,
    organisation,
    jobs,
    procedures,
    authoring,
    improve,
    operations,
    toolkits,
    metering,
    meetings: meetingsPage,
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
      "V1 keeps the source, pack, retrieval and work-package memory path. The source transformation inspector, seven-state model, retrieval test and object-evaluation entry points are V2 · PROPOSED extensions; procedural knowledge adds the knowledge-pack pin and organisational memory adds reviewed lessons.",
    ),
    agents: wrapLegacy(
      () => cataloguePage("agents"),
      "A run has one of two executor kinds — procedure or internal_agent (D46). This page configures the internal agent.",
    ),
    execution: executionPage,
  };
  const titles = {
    portfolio: "Portfolio",
    programme: "Programme definition",
    adm: "Architecture capability",
    plan: "Plan & WBS",
    validation: "Validation",
    projects: "Projects",
    outputs: "Outputs",
    contribute: "Contribute",
    organisation: "Organisation",
    jobs: "Jobs",
    procedures: "Procedures",
    authoring: "Authoring",
    improve: "Improve",
    operations: "Operations",
    toolkits: "Tools",
    metering: "Metering",
    meetings: "Meetings",
    architecture: "System architecture",
    documents: "Documents",
  };

  // Signed out, a design-study destination is not part of the public site — even by direct
  // hash. The public pages are the programme; the rest is the operator console's after sign-in.
  const signInGate = (title) =>
    `<div class="page enterprise">${heading(title, "This destination is part of the conveyor design study and is available after sign-in.", "", "DESIGN STUDY")}${note("The public site carries the programme: Portfolio, Projects, Plan, Validation, Procedures, Execution, Documents, Contribute and the system architecture. Sign in at the top of the page to open the operator console's design destinations.")}</div>`;
  for (const key of Object.keys(pages)) {
    if (PUBLIC_PAGES.has(key)) continue;
    const inner = pages[key];
    pages[key] = () => (studyOn() ? inner() : signInGate(titles[key] || key));
  }
  // Every public programme page carries the same feedback affordance: Discussions and a
  // prefilled issue. Signed out only — the operator console has the actions, and the public
  // page writes nothing and makes no GitHub call (the links are anchors).
  for (const key of PUBLIC_PAGES) {
    const inner = pages[key];
    pages[key] = () => (consoleOn() ? inner() : `${inner()}${dataPanel()}`);
  }

  /* ------------------------------------------------------------------ events */
  document.addEventListener("change", (event) => {
    const projectPage = event.target.closest('[data-v2-input="project-page"]');
    if (projectPage) {
      v.projectPage = projectPage.value;
      v.feedbackProject = null;
      render();
      consoleLoadProject(projectPage.value);
      consoleLoadOutputs(projectPage.value);
      return;
    }
    const feedbackProject = event.target.closest('[data-v2-input="feedback-project"]');
    if (feedbackProject) {
      v.feedbackProject = feedbackProject.value;
      render();
      return;
    }
    const planProject = event.target.closest('[data-v2-input="plan-project"]');
    if (planProject) {
      v.planProject = planProject.value;
      render();
      return;
    }
    const intakeType = event.target.closest('[data-v2-input="intake-type"]');
    if (intakeType) {
      v.intakeType = intakeType.value;
      return;
    }
    const outputsFilter = event.target.closest('[data-v2-input="outputs-filter"]');
    if (outputsFilter) {
      v[outputsFilter.dataset.key] = outputsFilter.value;
      render();
      return;
    }
    const attachment = event.target.closest('[data-v2-input="output-attachment"]');
    if (attachment) {
      consoleOutputAttachFile(attachment);
      return;
    }
  });
  document.addEventListener("click", (event) => {
    const el = event.target.closest("[data-v2]");
    if (!el) return;
    const d = el.dataset;
    switch (d.v2) {
      /* operator console — every case below is a real action, never a local mutation */
      case "console-signin":
        if (C) C.signIn();
        break;
      case "console-reload":
        // A reload is the whole read path, not just one board: the programme list is where a
        // dead session (401) and a moved project are actually visible.
        cs.loaded = false;
        cs.byProject = {};
        render();
        consoleLoad();
        break;
      case "console-intake":
        consoleIntake(d.project, d.issue);
        break;
      case "console-card":
        consoleCardDialog(d.id);
        break;
      case "console-move":
        consoleMove(d.id);
        break;
      case "console-comment":
        consoleComment(d.id);
        break;
      case "console-claim":
        consoleClaim(d.id);
        break;
      case "console-release":
        consoleRelease(d.id);
        break;
      /* programme workspace — every case below is a real action */
      case "project-select":
        v.projectPage = d.id;
        v.feedbackProject = null;
        render();
        if (consoleOn()) {
          consoleLoadProject(d.id);
          consoleLoadOutputs(d.id);
        }
        break;
      case "project-new":
        consoleProjectNew();
        break;
      case "project-edit":
        consoleProjectEdit(d.id);
        break;
      case "project-create":
        consoleProjectCreate();
        break;
      case "project-save":
        consoleProjectSave(d.id);
        break;
      case "project-close":
        consoleProjectClose(d.id);
        break;
      case "project-close-save":
        consoleProjectCloseSave(d.id);
        break;
      case "project-activate":
        consoleProjectTransition(d.id, "projects.activate", "Activate", "Project activated");
        break;
      case "project-pause":
        consoleProjectTransition(d.id, "projects.pause", "Pause", "Project paused");
        break;
      case "project-archive":
        consoleProjectTransition(d.id, "projects.archive", "Archive", "Project archived");
        break;
      /* execution room — every case is the action it names */
      case "run-start":
        consoleStartRun(d.id, d.key);
        break;
      case "programme-load-boards":
        consoleLoadAllBoards();
        break;
      case "run-open":
        consoleRunOpen(d.id);
        break;
      case "run-cancel":
        consoleRunCancel(d.id);
        break;
      case "run-cancel-save":
        consoleRunCancelSave(d.id);
        break;
      case "run-refresh":
        if (d.id) {
          delete cs.runDetail[d.id];
          consoleRunOpen(d.id);
        } else {
          cs.runsLoaded = false;
          consoleLoadRuns();
        }
        break;
      case "approval-approve":
        consoleApprovalDecide(d.id, "approve");
        break;
      case "approval-reject":
        consoleApprovalDecide(d.id, "reject");
        break;
      case "approval-save":
        consoleApprovalSave(d.id, d.decision);
        break;
      case "output-card":
        consoleOutputNew(d.project, d.id);
        break;
      case "output-new":
        consoleOutputNew(d.project);
        break;
      case "output-save":
        consoleOutputSave(d.project);
        break;
      case "output-edit":
        consoleOutputEdit(d.id);
        break;
      case "output-update":
        consoleOutputUpdate(d.id, d.version);
        break;
      case "output-publish":
        consoleOutputPublish(d.id, d.public !== "1", d.version);
        break;
      case "output-read":
        consoleOutputRead(d.id);
        break;
      case "output-download":
        consoleOutputDownload(d.id);
        break;
      case "output-delete":
        consoleOutputDelete(d.id, d.version);
        break;
      case "output-delete-confirm":
        consoleOutputDeleteConfirm(d.id, d.version);
        break;
      case "output-request-publish":
        consoleOutputRequestPublish(d.id, d.version);
        break;
      /* the admin's review queue: the same actions, a distinct control so the register's own
         buttons stay unambiguous */
      case "request-read":
        consoleOutputRead(d.id);
        break;
      case "request-publish":
        consoleOutputPublish(d.id, true, d.version);
        break;
      case "request-return":
        consoleReturnDialog(d.id, d.version);
        break;
      case "return-save":
        consoleReturnSave(d.id, d.version);
        break;
      case "request-kata":
        consoleKataDialog(d.id, d.version);
        break;
      case "kata-save":
        consoleKataSave(d.id, d.version);
        break;
      case "output-preview":
        consoleOutputPreview(d.id);
        break;
      case "output-history":
        consoleOutputHistory(d.id, d.version);
        break;
      case "output-attach":
        consoleOutputAttach(d.id);
        break;
      case "revision-view":
        consoleRevisionView(d.id, d.seq);
        break;
      case "revision-restore":
        consoleRevisionRestore(d.id, d.seq, d.version);
        break;
      case "revision-restore-confirm":
        consoleRevisionRestoreConfirm(d.id, d.seq, d.version);
        break;
      case "attachment-download":
        consoleAttachmentDownload(d.id, d.type, d.name);
        break;
      case "outputs-more":
        consoleLoadRegister(true);
        break;
      case "output-new-register":
        consoleOutputNew();
        break;
      case "output-new-template":
        consoleOutputTemplate(d.project, d.category);
        break;
      case "feedback-intake":
        consoleFeedbackIntake(d.issue);
        break;
      case "tab":
        v.tab[d.key] = d.value;
        render();
        break;
      case "adm-type":
        v.admType = d.id;
        render();
        break;
      case "adm-artifact":
        v.adm = d.id;
        v.admGate = null;
        render();
        break;
      case "adm-phase":
        v.admPhase = d.id;
        v.admGate = null;
        render();
        break;
      case "adm-advance": {
        const ph = F.architectureCapability.phases.find((p) => p.code === v.admPhase);
        const blocked = ph && (ph.missing.length || ph.approved < ph.required);
        v.admGate = !ph
          ? null
          : ph.state === "Closed"
            ? "adm-advance-closed"
            : blocked
              ? "adm-advance-refused"
              : "adm-advance-ok";
        render();
        toast(
          v.admGate === "adm-advance-refused"
            ? "422 phase_artifacts_missing · the phase stays open"
            : v.admGate === "adm-advance-closed"
              ? "Phase already closed · no further advance"
              : "Phase closed · the next phase is open",
        );
        break;
      }
      case "adm-approve":
        v.admGate = "adm-artifact-approved";
        render();
        toast("Artefact approved · a new version supersedes the old; the phase gate re-opens");
        break;
      case "adm-review":
        v.admGate = "adm-review-recorded";
        render();
        toast("Compliance review recorded · Consistent");
        break;
      case "adm-review-second":
        v.admGate = "adm-review-refused";
        render();
        toast("422 compliance_level_exists · one level per review");
        break;
      case "adm-waiver":
        v.admGate = "adm-waiver-refused";
        render();
        toast("422 waiver_owner_required · name the accepting owner and the review date");
        break;
      case "adm-waiver-ok":
        v.admGate = "adm-waiver-ok";
        render();
        toast("Waiver W-02 recorded · reassessment 2027-02-12");
        break;
      case "adm-h-change":
      case "adm-h-kata":
        v.admGate =
          v.admGate === "adm-h-change" || v.admGate === "adm-h-kata" ? "adm-h-refused" : d.v2;
        render();
        toast(
          v.admGate === "adm-h-refused"
            ? "422 phase_h_outcome_exists · exactly one outcome per review"
            : v.admGate === "adm-h-change"
              ? "Change request recorded · routed to the change gate"
              : "Kata candidate recorded · routed to the kata lane",
        );
        break;
      case "programme":
        v.programme = d.id;
        render();
        break;
      case "programme-mandate":
        v.programmeGate = "mandate-ok";
        render();
        toast("Mandate approved · the sponsoring group authorised the programme");
        break;
      case "programme-mandate-return":
        v.programmeGate = "mandate-return";
        render();
        toast("Mandate returned · no definition work starts");
        break;
      case "benefit":
      case "transition-select":
        v.benefit = d.id;
        v.programmeGate = null;
        render();
        break;
      case "tranche-review":
        v.tranche = Number(d.id);
        v.programmeGate = null;
        render();
        break;
      case "tranche-continue":
        v.programmeGate = "tranche-continue";
        render();
        toast("Tranche review recorded · continue");
        break;
      case "tranche-hold":
        v.programmeGate = "tranche-hold";
        render();
        toast("Tranche review recorded · pause");
        break;
      case "transition-record": {
        const record = F.realization.find((r) => r.benefit === v.benefit);
        v.programmeGate =
          record && !String(record.baselineMeasured).includes("not measured")
            ? "transition-ok"
            : "transition-blocked";
        render();
        toast(
          v.programmeGate === "transition-ok"
            ? "Transition recorded · post-transition measurement opens"
            : "422 benefit_baseline_missing · measure the baseline first",
        );
        break;
      }
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
          F.sops[v.sop].state === "Draft · PDDL" ? "Review" : F.sops[v.sop].state;
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
        toast("BPMN converted to a procedure draft · review gate opened");
        break;
      case "pddl-check":
        toast("The step validates against its published judge · the human gate remains");
        break;
      case "proc-process":
        v.procProcess = d.id;
        render();
        break;
      case "proc-ingest-sop":
        consoleProcIngest("sop");
        break;
      case "proc-ingest-bpmn":
        consoleProcIngest("bpmn");
        break;
      case "proc-ingest-rdf":
        consoleProcIngest("rdf");
        break;
      case "proc-validate":
        consoleProcValidate();
        break;
      case "proc-review-approve":
        consoleProcReview("approve");
        break;
      case "proc-review-reject":
        consoleProcReview("reject");
        break;
      case "proc-gate-process":
        consoleProcRequestGate("processes.publish");
        break;
      case "proc-gate-procedure":
        consoleProcRequestGate("procedures.publish");
        break;
      case "proc-gate-reload":
        consoleReloadProcGates();
        break;
      case "proc-gate-approve":
        consoleProcDecide(d.id, "approve");
        break;
      case "proc-gate-reject":
        consoleProcDecide(d.id, "reject");
        break;
      case "proc-publish-process":
        consoleProcPublish("process");
        break;
      case "proc-publish-procedure":
        consoleProcPublish("procedure");
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
      case "msp-programme-edit":
        mspDialog("programme", "Edit programme", msp().programme || {}, mspSource());
        break;
      case "programmes-refresh":
        consoleRefreshProgramme();
        break;
      case "msp-benefit-new":
        mspDialog("benefit", "New benefit profile", {}, mspSource());
        break;
      case "msp-benefit-edit":
        mspEditRow("benefit", d.id);
        break;
      case "msp-benefit":
        v.mspBenefit = d.id;
        render();
        break;
      case "msp-link-new":
        v.mspBenefit = d.id;
        mspDialog("link", "Add benefit-map edge", { benefit_id: d.id }, mspSource());
        break;
      case "msp-transition":
        v.mspBenefit = d.id;
        mspDialog("transition", "Record transition", list(mspSource().transitions).find((x) => x.benefit_id === d.id) || { benefit_id: d.id }, mspSource());
        break;
      case "msp-transition-new":
        mspDialog("transition", "Record transition", { benefit_id: v.mspBenefit || "" }, mspSource());
        break;
      case "msp-tranche":
        v.mspTranche = d.id;
        render();
        break;
      case "msp-tranche-new":
        mspDialog("tranche", "New tranche", {}, mspSource());
        break;
      case "msp-tranche-edit":
        mspEditRow("tranche", d.id);
        break;
      case "msp-review":
        mspDialog("review", "Tranche review", { id: d.id }, mspSource());
        break;
      case "msp-stakeholder":
        v.mspStakeholder = d.id;
        render();
        break;
      case "msp-stakeholder-new":
        mspDialog("stakeholder", "New stakeholder profile", {}, mspSource());
        break;
      case "msp-stakeholder-edit":
        mspEditRow("stakeholder", d.id);
        break;
      case "msp-wbs-new":
        mspDialog("wbs", "New WBS node", {}, mspSource());
        break;
      case "msp-wbs-edit":
        mspEditRow("wbs", d.id);
        break;
      case "msp-register-new":
        mspDialog("register", "New register item", {}, mspSource());
        break;
      case "msp-register-edit":
        mspEditRow("register", d.id);
        break;
      case "msp-cadence-new":
        mspDialog("cadence", "New cadence", {}, mspSource());
        break;
      case "msp-cadence-edit":
        mspEditRow("cadence", d.id);
        break;
      case "msp-dependency-new":
        mspDialog("dependency", "New dependency", {}, mspSource());
        break;
      case "msp-delete":
        mspConfirmDelete(d.kind, d.id, d.version);
        break;
      case "msp-delete-save":
        mspDelete(d.kind, d.id, d.version).then((ok) => {
          if (ok) {
            const modal = document.getElementById("dialog");
            if (modal) modal.close();
          }
        });
        break;
      case "msp-save":
        mspSave(d.kind, d.id, d.version).then((ok) => {
          if (ok) {
            const modal = document.getElementById("dialog");
            if (modal) modal.close();
          }
        });
        break;
      case "msp-baseline-take":
        mspWrite("baselines.take", { project_id: mspSource().projectId, kind: d.kind }, null, "Baseline taken · " + d.kind);
        break;
      default:
        break;
    }
  });

  return {
    sidebar,
    titles,
    pages,
    // The public/design boundary, exported so `studio.js` can gate the legacy pages it
    // merges into the same table (they predate this pack and are not in `pages`).
    publicPages: PUBLIC_PAGES,
    studyOn,
    gate: signInGate,
  };
};
