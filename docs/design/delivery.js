/* Delivery view — the four levels of work the public pack renders.
   L1 portfolio: programmes + standalone change initiatives (Gantt)
   L2 programme: its projects (Gantt)
   L3 project: a kanban of work packages and loose cards
   L4 work package: its child cards as their own kanban

   Pure renderers over the published snapshot: no fetch, no state, no DOM events. The caller
   (v2.js) supplies the rows and the route; this module only turns data into HTML. Every number
   shown is either in the snapshot or explicitly "—": a missing activity window is never a bar,
   and an unpriced model is never "$0". */
"use strict";
window.CONVEYOR_DELIVERY = (function () {
  const DAY = 86_400_000;
  const MONTHS = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const list = (value) => (Array.isArray(value) ? value : []);
  const esc = (value) =>
    String(value == null ? "" : value).replace(
      /[&<>"']/g,
      (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
    );
  const num = (value) => (typeof value === "number" ? value : parseFloat(value));
  const parse = (value) => {
    if (value == null || value === "") return null;
    const t = Date.parse(value);
    return Number.isFinite(t) ? t : null;
  };
  const tokens = (n) => {
    if (!Number.isFinite(n)) return "—";
    if (n >= 1e9) return (n / 1e9).toFixed(1) + "B";
    if (n >= 1e6) return (n / 1e6).toFixed(1) + "M";
    if (n >= 1e3) return (n / 1e3).toFixed(1) + "k";
    return String(n);
  };
  // `cost_usd` is a decimal STRING in the snapshot (the metering report's own rendering), so the
  // parser accepts both. An unpriced balance is never shown as money.
  const usd = (value) => {
    const n = num(value);
    if (!Number.isFinite(n)) return "—";
    return "$" + n.toFixed(n > 0 && n < 0.01 ? 4 : 2);
  };
  const day = (ms) => {
    if (!Number.isFinite(ms)) return "—";
    const d = new Date(ms);
    return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
  };
  const byPosition = (a, b) => Number(a.position || 0) - Number(b.position || 0);

  /* ---------------------------------------------------------------- the model */
  // The activity window and the spend are the snapshot's own rows (`work.project_activity`,
  // `measure.inference.by_project`). A project with no event has no bar; the page says so.
  function activityIndex(programme, work, measure, clock) {
    const now = Number.isFinite(clock) ? clock : parse((programme || {}).generated_at) ?? Date.now();
    const projects = list((programme || {}).projects);
    const activity = new Map(
      list((work || {}).project_activity).map((row) => [row.project_id, row]),
    );
    const inference = ((measure || {}).inference || {});
    const usage = new Map(list(inference.by_project).map((row) => [row.project_id, row]));
    const boardProject = new Map(
      list((work || {}).boards).map((row) => [row.id, row.project_id]),
    );
    const lanes = new Map(list((work || {}).lists).map((row) => [row.id, row]));
    const progress = new Map();
    list((work || {}).cards).forEach((card) => {
      const projectId = boardProject.get(card.board_id);
      if (!projectId) return;
      const entry = progress.get(projectId) || { done: 0, total: 0 };
      entry.total += 1;
      const kind = (lanes.get(card.list_id) || {}).kind;
      if (card.archived_at || kind === "done" || kind === "archive") entry.done += 1;
      progress.set(projectId, entry);
    });
    const index = new Map();
    projects.forEach((project) => {
      const window = activity.get(project.id) || {};
      const start = parse(window.started_at) ?? parse(project.created_at);
      const completed = project.status === "closed" || project.status === "archived";
      const observed = parse(window.last_activity_at);
      const end = completed ? observed ?? parse(project.updated_at) ?? start : now;
      const spend = usage.get(project.id) || null;
      const counts = progress.get(project.id) || { done: 0, total: 0 };
      index.set(project.id, {
        id: project.id,
        key: project.key,
        name: project.name,
        sub: project.key,
        status: project.status || "draft",
        programmeId: project.programme_id || null,
        state: completed ? "completed" : "in_progress",
        start,
        end: start == null ? null : Math.max(end ?? start, start),
        planStart: parse(((project.meta || {}).plan_start)),
        planEnd: parse(((project.meta || {}).plan_end)),
        done: counts.done,
        total: counts.total,
        tokens: spend ? spend.total_tokens : null,
        usd: spend ? spend.cost_usd : null,
        unpriced: spend ? spend.unpriced_calls : null,
      });
    });
    return index;
  }

  // Roll a programme up from its projects: earliest start, latest end (open if any project is
  // open), summed spend. A programme with no project has no bar and says so.
  function rollUp(programme, members) {
    const dated = members.filter((row) => Number.isFinite(row.start));
    const open = members.some((row) => row.state !== "completed");
    const start = dated.length ? Math.min(...dated.map((row) => row.start)) : null;
    const ends = dated
      .map((row) => (row.state === "completed" ? row.end : null))
      .filter(Number.isFinite);
    const end = start == null ? null : open || !ends.length ? null : Math.max(...ends);
    const sums = members.reduce(
      (acc, row) => {
        if (Number.isFinite(row.tokens)) acc.tokens += row.tokens;
        const n = num(row.usd);
        if (Number.isFinite(n)) acc.usd += n;
        acc.unpriced += Number(row.unpriced || 0);
        acc.done += row.done || 0;
        acc.total += row.total || 0;
        return acc;
      },
      { tokens: 0, usd: 0, unpriced: 0, done: 0, total: 0 },
    );
    const withSpend = members.filter((row) => Number.isFinite(row.tokens));
    const completed = programme.status === "closed" || (members.length > 0 && !open);
    return {
      id: programme.id,
      key: programme.name,
      name: programme.name,
      sub: `${members.length} project${members.length === 1 ? "" : "s"}`,
      status: programme.status || "active",
      kind: "programme",
      state: completed ? "completed" : "in_progress",
      start,
      end,
      planStart: null,
      planEnd: null,
      done: sums.done,
      total: sums.total,
      tokens: withSpend.length ? sums.tokens : null,
      usd: withSpend.length ? sums.usd : null,
      unpriced: sums.unpriced,
      members,
    };
  }

  // L1: every programme, plus every project that does not belong to one.
  function portfolioRows(programme, index) {
    const programmes = list((programme || {}).programme);
    const rows = programmes.map((row) =>
      rollUp(
        row,
        [...index.values()].filter((p) => p.programmeId === row.id),
      ),
    );
    const standalone = [...index.values()].filter((p) => !p.programmeId);
    return { programmes: rows, standalone };
  }

  // L2: one programme's projects.
  function programmeRows(programme, index, programmeId) {
    const row = list((programme || {}).programme).find((p) => p.id === programmeId);
    if (!row) return { programme: null, projects: [] };
    return {
      programme: row,
      projects: [...index.values()].filter((p) => p.programmeId === row.id),
    };
  }

  function summary(rows) {
    const all = list(rows);
    const inFlight = all.filter((row) => row.state !== "completed").length;
    const spend = all.reduce(
      (acc, row) => {
        if (Number.isFinite(row.tokens)) acc.tokens += row.tokens;
        const n = num(row.usd);
        if (Number.isFinite(n)) acc.usd += n;
        acc.unpriced += Number(row.unpriced || 0);
        return acc;
      },
      { tokens: 0, usd: 0, unpriced: 0 },
    );
    return { count: all.length, inFlight, completed: all.length - inFlight, ...spend };
  }

  /* ---------------------------------------------------------------- the Gantt */
  function domainOf(rows) {
    const starts = rows.map((row) => row.start).filter(Number.isFinite);
    const ends = rows
      .map((row) => (Number.isFinite(row.end) ? row.end : row.start))
      .filter(Number.isFinite);
    if (!starts.length) return null;
    let from = Math.min(...starts);
    let to = Math.max(...ends);
    const pad = Math.max((to - from) * 0.04, DAY / 2);
    from -= pad;
    to += pad;
    if (to - from < DAY) to = from + DAY;
    return { from, to };
  }

  function ticks(domain) {
    const span = domain.to - domain.from;
    const step =
      span <= 14 * DAY ? DAY : span <= 120 * DAY ? 7 * DAY : span <= 730 * DAY ? 30 * DAY : 90 * DAY;
    const out = [];
    // Start on a UTC day boundary, then walk forward.
    let t = Math.ceil(domain.from / DAY) * DAY;
    if (step > DAY) t = Math.ceil(domain.from / step) * step;
    for (; t <= domain.to; t += step) {
      out.push({ at: t, label: step >= 28 * DAY ? `${MONTHS[new Date(t).getUTCMonth()]} ${String(new Date(t).getUTCFullYear()).slice(2)}` : `${new Date(t).getUTCDate()} ${MONTHS[new Date(t).getUTCMonth()]}` });
    }
    return { step, marks: out };
  }

  function bar(row, domain) {
    const span = domain.to - domain.from;
    const left = ((row.start - domain.from) / span) * 100;
    const raw = (((row.end ?? row.start) - row.start) / span) * 100;
    const width = Math.max(raw, 0.8);
    return `left:${Math.max(0, left).toFixed(3)}%;width:${Math.min(100 - Math.max(0, left), width).toFixed(3)}%`;
  }

  function costCell(row) {
    if (!Number.isFinite(row.tokens)) {
      return `<span class="gantt-cost muted">no spend</span>`;
    }
    const unpriced = Number(row.unpriced || 0);
    return `<span class="gantt-cost"><span class="mono">${esc(tokens(row.tokens))}</span> tok${
      unpriced > 0
        ? ` · <span class="muted" title="${unpriced} call(s) have no commercial price">${esc(usd(row.usd))}*</span>`
        : ` · <span class="mono">${esc(usd(row.usd))}</span>`
    }</span>`;
  }

  function rowHtml(row, domain) {
    const label = row.href
      ? `<a class="gantt-name" href="${esc(row.href)}">${esc(row.name)}</a>`
      : `<span class="gantt-name">${esc(row.name)}</span>`;
    const progress =
      row.total > 0
        ? `<span class="gantt-progress" title="${row.done} of ${row.total} cards done">${row.done}/${row.total}</span>`
        : "";
    const track = Number.isFinite(row.start)
      ? `${Number.isFinite(row.planStart) && Number.isFinite(row.planEnd) ? `<span class="gantt-plan" style="${bar({ start: row.planStart, end: row.planEnd }, domain)}" title="planned ${esc(day(row.planStart))} → ${esc(day(row.planEnd))}"></span>` : ""}<span class="gantt-actual ${row.state}" style="${bar(row, domain)}" title="${esc(row.state === "completed" ? "completed" : "in progress")} · ${esc(day(row.start))} → ${esc(day(row.end))}"><i></i></span>`
      : `<span class="gantt-nodate muted">no dated activity</span>`;
    return `<div class="gantt-row" data-state="${esc(row.state)}">
      <div class="gantt-label">${label}<small>${esc(row.sub || "")}${
        row.kind === "programme" ? "" : ` · ${esc(row.status)}`
      }</small>${progress}${costCell(row)}</div>
      <div class="gantt-track">${track}</div>
    </div>`;
  }

  function gantt(rows, options = {}) {
    const visible = list(rows).filter(Boolean);
    if (!visible.length) {
      return `<div class="gantt gantt-empty muted">${esc(options.emptyNote || "No work is published for this level.")}</div>`;
    }
    const domain = domainOf(visible);
    const axis = domain
      ? ticks(domain)
          .marks.map(
            (mark) =>
              `<span class="gantt-tick" style="left:${(((mark.at - domain.from) / (domain.to - domain.from)) * 100).toFixed(3)}%">${esc(mark.label)}</span>`,
          )
          .join("")
      : "";
    const today =
      domain && Date.now() >= domain.from && Date.now() <= domain.to
        ? `<span class="gantt-now" style="left:${(((Date.now() - domain.from) / (domain.to - domain.from)) * 100).toFixed(3)}%" title="today"></span>`
        : "";
    const body = visible.map((row) => rowHtml(row, domain || { from: 0, to: 1 })).join("");
    return `<div class="gantt">
      <div class="gantt-head"><div class="gantt-head-label">${esc(options.label || "Work")}</div><div class="gantt-axis">${axis}${today}</div></div>
      <div class="gantt-body">${body}</div>
      <details class="gantt-data"><summary>Table view</summary>${ganttTable(visible)}</details>
    </div>`;
  }

  function ganttTable(rows) {
    const body = list(rows)
      .map(
        (row) => `<tr><td><strong>${esc(row.name)}</strong><small>${esc(row.sub || "")}</small></td>
          <td>${esc(row.state === "completed" ? "completed" : "in progress")}</td>
          <td>${esc(day(row.start))}</td><td>${esc(day(row.end))}</td>
          <td>${esc(Number.isFinite(row.tokens) ? tokens(row.tokens) : "—")}</td>
          <td>${esc(Number.isFinite(row.tokens) ? usd(row.usd) : "—")}${
            Number(row.unpriced || 0) > 0 ? ` <small class="muted">${Number(row.unpriced)} unpriced</small>` : ""
          }</td></tr>`,
      )
      .join("");
    return `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Work</th><th>State</th><th>Start</th><th>End</th><th>Tokens</th><th>Cost</th></tr></thead><tbody>${body}</tbody></table></div>`;
  }

  /* ---------------------------------------------------------------- the kanban */
  function spendByCard(measure) {
    const inference = ((measure || {}).inference || {});
    return new Map(list(inference.by_card).map((row) => [row.card_id, row]));
  }

  function latestRun(work) {
    const map = new Map();
    list((work || {}).runs).forEach((run) => {
      if (!run.card_id) return;
      const previous = map.get(run.card_id);
      if (!previous || String(run.created_at) > String(previous.created_at)) map.set(run.card_id, run);
    });
    return map;
  }

  function laneKind(work) {
    return new Map(list((work || {}).lists).map((row) => [row.id, row.kind]));
  }

  function isDone(card, lanes) {
    return Boolean(card.archived_at) || ["done", "archive"].includes(lanes.get(card.list_id));
  }

  function tile(card, model) {
    const spend = model.spend.get(card.id);
    const run = model.runs.get(card.id);
    const type = model.types.get(card.card_type_id);
    const children = model.children.get(card.id) || [];
    const closed = children.filter((child) => isDone(child, model.laneKinds)).length;
    const isPackage = children.length > 0;
    const issue =
      card.issue != null
        ? `<a class="kanban-issue" href="https://github.com/nonagenticai/conveyor/issues/${encodeURIComponent(card.issue)}" target="_blank" rel="noopener">#${esc(card.issue)}</a>`
        : "";
    const head = isPackage
      ? `<a class="kanban-open" href="${esc(model.wpBase)}/${esc(card.id)}">${esc(card.name)}</a>`
      : `<span class="kanban-title">${esc(card.name)}</span>`;
    const childSpend = children.filter((child) => model.spend.has(child.id));
    const hasSpend = Boolean(spend) || childSpend.length > 0;
    const totalTokens = !hasSpend
      ? null
      : children.reduce((sum, child) => {
          const own = model.spend.get(child.id);
          return sum + (own ? Number(own.total_tokens) || 0 : 0);
        }, spend ? Number(spend.total_tokens) || 0 : 0);
    const unpriced = children.reduce(
      (sum, child) => sum + Number((model.spend.get(child.id) || {}).unpriced_calls || 0),
      Number((spend || {}).unpriced_calls || 0),
    );
    const costUsd = !hasSpend
      ? NaN
      : children.reduce(
          (sum, child) => {
            const n = num((model.spend.get(child.id) || {}).cost_usd);
            return sum + (Number.isFinite(n) ? n : 0);
          },
          spend ? num(spend.cost_usd) || 0 : 0,
        );
    return `<article class="kanban-card${isPackage ? " work-package" : ""}${isDone(card, model.laneKinds) ? " done" : ""}">
      ${isPackage ? `<span class="kanban-kind">WORK PACKAGE · ${children.length}</span>` : ""}
      ${head}
      <small class="kanban-meta">${esc(type ? type.code : "—")}${card.assignee_name ? ` · ${esc(card.assignee_name)}` : ""} ${issue}</small>
      ${isPackage ? `<span class="kanban-count">${closed}/${children.length} cards done</span>` : ""}
      ${run ? `<span class="kanban-run ${run.status === "succeeded" ? "ok" : run.status === "failed" ? "bad" : ""}">${esc(run.status)}</span>` : ""}
      <span class="kanban-cost">${Number.isFinite(totalTokens) ? `${esc(tokens(totalTokens))} tok${Number.isFinite(costUsd) ? ` · ${esc(usd(costUsd))}${unpriced > 0 ? "*" : ""}` : ""}` : "no spend"}</span>
    </article>`;
  }

  // L3: one project's board. Roots only; a root with children is the work package.
  function boardModel(work, projectId, measure) {
    const boards = list((work || {}).boards).filter((row) => row.project_id === projectId);
    const board = boards[0] || null;
    const lanes = board
      ? list((work || {}).lists).filter((row) => row.board_id === board.id).sort(byPosition)
      : [];
    const all = list((work || {}).cards);
    const open = all.filter((card) => !card.archived_at);
    const openIds = new Set(open.map((card) => card.id));
    const children = new Map();
    open.forEach((card) => {
      // A child whose parent is archived or absent is a loose card, not an orphan tile.
      if (!card.parent_card_id || !openIds.has(card.parent_card_id)) return;
      if (!children.has(card.parent_card_id)) children.set(card.parent_card_id, []);
      children.get(card.parent_card_id).push(card);
    });
    const roots = board
      ? open.filter(
          (card) =>
            card.board_id === board.id &&
            (!card.parent_card_id || !openIds.has(card.parent_card_id)),
        )
      : [];
    return {
      projectId,
      board,
      lanes,
      roots,
      children,
      laneKinds: laneKind(work),
      types: new Map(list((work || {}).card_types).map((row) => [row.id, row])),
      runs: latestRun(work),
      spend: spendByCard(measure),
      wpBase: `#projects/${projectId}/wp`,
    };
  }

  function kanban(model, options = {}) {
    if (!model.board) {
      return `<div class="kanban kanban-empty muted">${esc(
        options.noBoard || "This project has no board in the snapshot; cards and runs need one.",
      )}</div>`;
    }
    if (!model.roots.length) {
      return `<div class="kanban kanban-empty muted">${esc(
        options.emptyNote || "No open card on this board. A card with children is the work package.",
      )}</div>`;
    }
    const columns = model.lanes.map((lane) => {
      const cards = model.roots.filter((card) => card.list_id === lane.id).sort(byPosition);
      return `<section class="kanban-col" data-lane="${esc(lane.kind)}">
        <header><span>${esc(lane.name)}</span><b>${cards.length}</b></header>
        <div class="kanban-cards">${cards.map((card) => tile(card, model)).join("")}</div>
      </section>`;
    });
    return `<div class="kanban">${columns.join("")}</div>`;
  }

  // L4: the work package's own board — its children, including archived ones (marked done).
  function packageModel(work, projectId, cardId, measure) {
    const cards = list((work || {}).cards);
    const parent = cards.find((card) => card.id === cardId) || null;
    const children = cards.filter((card) => card.parent_card_id === cardId);
    const boards = list((work || {}).boards).filter((board) =>
      board.project_id === projectId,
    );
    const lanes = [];
    const seen = new Set();
    boards
      .sort(byPosition)
      .forEach((board) =>
        list((work || {}).lists)
          .filter((lane) => lane.board_id === board.id)
          .sort(byPosition)
          .forEach((lane) => {
            if (seen.has(lane.id)) return;
            seen.add(lane.id);
            lanes.push(lane);
          }),
      );
    return {
      projectId,
      parent,
      children,
      lanes,
      laneKinds: laneKind(work),
      types: new Map(list((work || {}).card_types).map((row) => [row.id, row])),
      runs: latestRun(work),
      spend: spendByCard(measure),
      wpBase: `#projects/${projectId}/wp`,
      backHref: `#projects/${projectId}`,
    };
  }

  function packageBoard(model) {
    if (!model.parent) {
      return `<div class="kanban kanban-empty muted">This work package is not in the published snapshot.</div>`;
    }
    const done = model.children.filter((child) => isDone(child, model.laneKinds)).length;
    const columns = model.lanes.map((lane) => {
      const cards = model.children.filter((card) => card.list_id === lane.id).sort(byPosition);
      return `<section class="kanban-col" data-lane="${esc(lane.kind)}">
        <header><span>${esc(lane.name)}</span><b>${cards.length}</b></header>
        <div class="kanban-cards">${cards
          .map(
            (card) =>
              `<article class="kanban-card${isDone(card, model.laneKinds) ? " done" : ""}">
                 <span class="kanban-title">${esc(card.name)}</span>
                 <small class="kanban-meta">${esc((model.types.get(card.card_type_id) || {}).code || "—")}${
                   model.runs.get(card.id) ? ` · ${esc(model.runs.get(card.id).status)}` : ""
                 }</small>
                 <span class="kanban-cost">${
                   model.spend.get(card.id)
                     ? `${esc(tokens(Number(model.spend.get(card.id).total_tokens)))} tok · ${esc(usd(model.spend.get(card.id).cost_usd))}${Number(model.spend.get(card.id).unpriced_calls) > 0 ? "*" : ""}`
                     : "no spend"
                 }</span>
               </article>`,
          )
          .join("")}</div>
      </section>`;
    });
    const orphan = model.children.filter((card) => !model.lanes.some((lane) => lane.id === card.list_id));
    const orphanCol = orphan.length
      ? `<section class="kanban-col" data-lane="unknown"><header><span>No lane</span><b>${orphan.length}</b></header><div class="kanban-cards">${orphan
          .map((card) => `<article class="kanban-card done"><span class="kanban-title">${esc(card.name)}</span></article>`)
          .join("")}</div></section>`
      : "";
    return `<div class="kanban-wrap">
      <div class="kanban-package-head"><a class="gantt-name" href="${esc(model.backHref)}">← ${esc(model.parent.name)}</a><span class="muted">work package · ${done}/${model.children.length} cards done</span></div>
      <div class="kanban">${columns.join("")}${orphanCol}</div>
    </div>`;
  }

  return {
    activityIndex,
    portfolioRows,
    programmeRows,
    summary,
    gantt,
    ganttTable,
    boardModel,
    kanban,
    packageModel,
    packageBoard,
    tokens,
    usd,
  };
})();
