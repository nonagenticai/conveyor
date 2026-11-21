/* Delivery view — the four levels of work the public pack renders.
   L1 portfolio: programmes + standalone change initiatives (Gantt)
   L2 programme: its projects, grouped by MSP tranche (Gantt + tranche/benefit rails)
   L3 project: a kanban of work packages and loose cards, plus its governance record
   L4 work package: the card's execution and its child cards as their own kanban

   Pure renderers over the published snapshot: no fetch, no state, no DOM events. The caller
   (v2.js) supplies the rows and the route; this module only turns data into HTML. Every number
   shown is either in the snapshot or explicitly "—": a project with no observed event is never a
   bar, and a call on an unpriced model is never "$0". The MSP/TOGAF/SAFE artefacts are the
   snapshot's own rows (`msp.json`, `architecture.json`, `work.json`); an absent row is stated as
   absent, never invented. */
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
  // The conformance audit's own placeholder rule (scripts/audit-programme-conformance.py): a
  // baseline that says one of these has not been measured.
  const PLACEHOLDER =
    /(not measured|no instrument|unknown|tbd|to be determined|n\/?a|placeholder|none)/i;
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
  // parser accepts both. `—` is the money answer when nothing is priced: zero is a claim.
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
  const shortDate = (ms) => {
    if (!Number.isFinite(ms)) return "—";
    const d = new Date(ms);
    return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
  };
  const byPosition = (a, b) => Number(a.position || 0) - Number(b.position || 0);
  const byNumber = (a, b) => Number(a.number || 0) - Number(b.number || 0);
  const byKey = (a, b) => String(a.key || "").localeCompare(String(b.key || ""));

  // One spend rendering for every level. `priced_calls` is the decider: tokens with no priced
  // call are "—*", never "$0.00".
  function spendText(tokensN, usdValue, priced, unpriced) {
    if (!Number.isFinite(tokensN)) return "no spend";
    const p = Number(priced || 0);
    const u = Number(unpriced || 0);
    if (p === 0 && u > 0) return `${tokens(tokensN)} tok · —*`;
    const money = Number.isFinite(num(usdValue)) ? usd(usdValue) : "—";
    return `${tokens(tokensN)} tok · ${money}${u > 0 ? "*" : ""}`;
  }

  function addSpend(acc, row) {
    if (!row) return acc;
    if (Number.isFinite(Number(row.total_tokens))) acc.tokens += Number(row.total_tokens);
    const n = num(row.cost_usd);
    if (Number.isFinite(n)) acc.usd += n;
    acc.priced += Number(row.priced_calls || 0);
    acc.unpriced += Number(row.unpriced_calls || 0);
    acc.any = true;
    return acc;
  }

  /* ------------------------------------------------------------- the artefacts */
  // MSP §8.3 and TOGAF §8.1 rows, scoped to one programme. A row without `programme_id` is from an
  // older snapshot or a study fixture; it is shown rather than dropped.
  const scoped = (rows, programmeId) =>
    list(rows).filter((row) => !row.programme_id || row.programme_id === programmeId);

  function programmeArtefacts(msp, programmeId) {
    const M = msp || {};
    const tranches = scoped(M.tranches, programmeId).sort(byNumber);
    const benefits = scoped(M.benefit_profiles, programmeId);
    const transitions = scoped(M.benefit_transitions, programmeId);
    const reviews = list(M.tranche_reviews);
    const stakeholders = scoped(M.stakeholder_profiles, programmeId);
    const links = scoped(M.benefit_links, programmeId);
    const cadences = scoped(M.cadences, programmeId);
    const measured = benefits.filter((row) => benefitMeasured(row, transitions));
    const realised = benefits.filter((row) => benefitRealised(row, transitions));
    const stateByNumber = new Map(tranches.map((row) => [Number(row.number), row]));
    return {
      tranches,
      benefits,
      transitions,
      reviews,
      stakeholders,
      links,
      cadences,
      stateByNumber,
      trancheSummary: trancheSummary(tranches),
      benefitsMeasured: measured.length,
      benefitsRealised: realised.length,
    };
  }

  function benefitMeasured(profile, transitions) {
    const own = list(transitions).filter((row) => row.benefit_id === profile.id);
    if (own.some((row) => row.baseline_measured_at)) return true;
    const baseline = String(profile.baseline || "").trim();
    return Boolean(baseline) && !PLACEHOLDER.test(baseline);
  }

  // `realized` is a transition with a post-transition measurement — never the profile's own status
  // (SPEC_V2 §8.3: a benefit whose baseline was never measured cannot claim an improvement).
  function benefitRealised(profile, transitions) {
    return list(transitions)
      .filter((row) => row.benefit_id === profile.id)
      .some((row) => row.post_measured_at);
  }

  // "T0 closed · T1–13 planned", compressing consecutive numbers that share a state.
  function trancheSummary(tranches) {
    if (!tranches.length) return "no tranche row";
    const groups = new Map();
    tranches.forEach((row) => {
      const state = String(row.state || "planned");
      if (!groups.has(state)) groups.set(state, []);
      groups.get(state).push(Number(row.number));
    });
    return [...groups.entries()]
      .map(([state, numbers]) => {
        const sorted = numbers.sort((a, b) => a - b);
        const runs = [];
        let from = sorted[0];
        let to = sorted[0];
        sorted.slice(1).forEach((n) => {
          if (n === to + 1) {
            to = n;
            return;
          }
          runs.push(from === to ? `T${from}` : `T${from}–${to}`);
          from = n;
          to = n;
        });
        runs.push(from === to ? `T${from}` : `T${from}–${to}`);
        return `${runs.join(", ")} ${state}`;
      })
      .join(" · ");
  }

  const tone = (state) =>
    /closed|approved|adopted|realised|realized|continue/i.test(String(state || ""))
      ? "ok"
      : /authorised|authorized|in_review|active|tracking/i.test(String(state || ""))
        ? "amber"
        : "";

  function chip(label, value, state, title) {
    return `<span class="rail-chip ${tone(state)}"${title ? ` title="${esc(title)}"` : ""}><b>${esc(
      label,
    )}</b>${value ? ` ${esc(value)}` : ""}</span>`;
  }

  // MSP tranche rail: one chip per tranche, state and review date where the row carries one.
  function trancheRail(artefacts) {
    const a = artefacts || {};
    const reviews = new Map();
    list(a.reviews).forEach((row) => {
      const existing = reviews.get(row.tranche_id);
      if (!existing || String(row.reviewed_at || row.created_at || "") > String(existing.reviewed_at || existing.created_at || "")) {
        reviews.set(row.tranche_id, row);
      }
    });
    const chips = list(a.tranches)
      .map((row) => {
        const review = reviews.get(row.id);
        const when = row.review_at || (review && (review.reviewed_at || review.created_at));
        const decision = review && review.decision ? ` · ${review.decision}` : "";
        return chip(
          `T${row.number}`,
          `${row.state || "planned"}${when ? ` · ${shortDate(parse(when))}` : ""}${decision}`,
          row.state,
          row.baseline_ref ? `baseline ${row.baseline_ref}` : "",
        );
      })
      .join("");
    const head = a.trancheSummary ? ` · ${a.trancheSummary}` : "";
    return `<div class="delivery-rail"><span class="rail-label">MSP tranches${esc(head)}</span><div class="rail-chips">${
      chips || `<span class="rail-chip">no tranche row published</span>`
    }</div></div>`;
  }

  // MSP benefit rail: the profile's own baseline/target, measured vs not.
  function benefitRail(artefacts) {
    const a = artefacts || {};
    const chips = list(a.benefits)
      .slice(0, 8)
      .map((row) => {
        const measured = benefitMeasured(row, a.transitions);
        const realised = benefitRealised(row, a.transitions);
        const state = realised ? "realised" : measured ? "measured" : "unmeasured";
        const target = String(row.target || row.observable_outcome || "").trim();
        return chip(
          row.reference || String(row.id || "").slice(0, 8),
          `${row.description || ""} · ${state}`,
          realised ? "closed" : measured ? "in_review" : "planned",
          [target ? `target: ${target}` : "", row.measurement_method ? `method: ${row.measurement_method}` : ""]
            .filter(Boolean)
            .join(" · "),
        );
      })
      .join("");
    const count = a.benefits
      ? `${a.benefitsRealised}/${list(a.benefits).length} realised · ${a.benefitsMeasured} with a measured baseline`
      : "";
    return `<div class="delivery-rail"><span class="rail-label">MSP benefits${esc(
      count ? ` · ${count}` : "",
    )}</span><div class="rail-chips">${
      chips || `<span class="rail-chip">no benefit profile published</span>`
    }</div></div>`;
  }

  // SAFe cadence and the TOGAF capability the programme runs under. Both are the snapshot's rows;
  // an empty set is named, not faked.
  function governanceLine(artefacts, architecture) {
    const a = artefacts || {};
    const arch = architecture || {};
    const cadence = list(a.cadences).length
      ? `${list(a.cadences).length} cadence row${list(a.cadences).length === 1 ? "" : "s"} (${[
          ...new Set(list(a.cadences).map((row) => row.kind).filter(Boolean)),
        ].join(", ") || "kind not named"})`
      : "no cadence row";
    const profile = list(arch.profiles)[0];
    const board = list(arch.board)[0];
    const contracts = list(arch.contracts).length;
    const reviews = list(arch.compliance_reviews || arch.reviews).length;
    const capability = profile
      ? `ADM profile <b>${esc(profile.key || profile.name || "—")}</b> ${esc(profile.status || "")}${
          board ? ` · board ${esc(board.status || "—")}` : ""
        }${contracts ? ` · ${contracts} contract${contracts === 1 ? "" : "s"}` : " · no contract"}`
      : "no ADM profile published";
    const counts = `${list(a.stakeholders).length} stakeholder profile${
      list(a.stakeholders).length === 1 ? "" : "s"
    } · ${list(a.links).length} benefit-map edge${list(a.links).length === 1 ? "" : "s"}${
      reviews ? ` · ${reviews} compliance review${reviews === 1 ? "" : "s"}` : ""
    }`;
    return `<div class="delivery-rail"><span class="rail-label">SAFe cadence · TOGAF capability</span><div class="rail-chips">${chip(
      "Cadence",
      cadence,
      list(a.cadences).length ? "active" : "planned",
    )}<span class="rail-chip ${profile ? tone(profile.status) : ""}">${capability}</span><span class="rail-chip">${esc(
      counts,
    )}</span></div></div>`;
  }

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
      const window = activity.get(project.id) || null;
      const completed = project.status === "closed" || project.status === "archived";
      const start = window ? parse(window.started_at) : null;
      const observed = window ? parse(window.last_activity_at) : null;
      const end = start == null ? null : completed ? observed ?? start : now;
      const spend = usage.get(project.id) || null;
      const counts = progress.get(project.id) || { done: 0, total: 0 };
      index.set(project.id, {
        id: project.id,
        key: project.key,
        name: project.name,
        sub: project.key,
        status: project.status || "draft",
        programmeId: project.programme_id || null,
        tranche:
          project.meta && project.meta.tranche != null ? Number(project.meta.tranche) : null,
        type: project.project_type_code || (project.meta || {}).track || null,
        state: completed ? "completed" : "in_progress",
        start,
        end: start == null ? null : Math.max(end ?? start, start),
        planStart: parse((project.meta || {}).plan_start),
        planEnd: parse((project.meta || {}).plan_end),
        done: counts.done,
        total: counts.total,
        tokens: spend ? Number(spend.total_tokens) : null,
        usd: spend ? spend.cost_usd : null,
        priced: spend ? Number(spend.priced_calls || 0) : null,
        unpriced: spend ? Number(spend.unpriced_calls || 0) : null,
      });
    });
    return index;
  }

  // Roll a programme up from its projects: earliest start, latest end (open while any project is
  // open — and then the end is the snapshot clock, never a sliver at the start).
  function rollUp(programme, members, clock) {
    const dated = members.filter((row) => Number.isFinite(row.start));
    const open = members.some((row) => row.state !== "completed");
    const start = dated.length ? Math.min(...dated.map((row) => row.start)) : null;
    const ends = dated.map((row) => row.end).filter(Number.isFinite);
    const end = start == null ? null : open ? clock : Math.max(...ends, start);
    const sums = members.reduce(
      (acc, row) => {
        if (Number.isFinite(row.tokens)) acc.tokens += row.tokens;
        const n = num(row.usd);
        if (Number.isFinite(n)) acc.usd += n;
        acc.priced += Number(row.priced || 0);
        acc.unpriced += Number(row.unpriced || 0);
        acc.done += row.done || 0;
        acc.total += row.total || 0;
        return acc;
      },
      { tokens: 0, usd: 0, priced: 0, unpriced: 0, done: 0, total: 0 },
    );
    const withSpend = members.some((row) => Number.isFinite(row.tokens));
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
      tokens: withSpend ? sums.tokens : null,
      usd: withSpend ? sums.usd : null,
      priced: withSpend ? sums.priced : null,
      unpriced: sums.unpriced,
      members,
    };
  }

  // L1: every in-flight programme, plus every standalone project. SPEC_V2 §5.1: closed and
  // archived programmes are history and are not listed; the row still carries the programme's
  // project counts so completed work is visible at portfolio level.
  function portfolioRows(programme, index, clock, msp) {
    const programmes = list((programme || {}).programme)
      .filter((row) => row.status !== "closed" && row.status !== "archived")
      .map((row) => {
        const members = [...index.values()].filter((p) => p.programmeId === row.id);
        return { ...rollUp(row, members, clock), artefacts: programmeArtefacts(msp, row.id) };
      });
    const standalone = [...index.values()].filter((p) => !p.programmeId);
    return { programmes, standalone };
  }

  // L2: one programme's projects (completed included — a programme's history is its own)
  // grouped by the MSP tranche carried on `project.meta.tranche`.
  function programmeRows(programme, index, programmeId, msp) {
    const row = list((programme || {}).programme).find((p) => p.id === programmeId);
    if (!row) return { programme: null, projects: [], artefacts: null };
    return {
      programme: row,
      projects: [...index.values()].filter((p) => p.programmeId === row.id),
      artefacts: programmeArtefacts(msp, row.id),
    };
  }

  // Insert a group row per tranche; the Gantt draws it as a full-width divider.
  function trancheRows(projects, artefacts) {
    const a = artefacts || {};
    const groups = new Map();
    list(projects).forEach((row) => {
      const key = row.tranche == null ? "—" : String(row.tranche);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(row);
    });
    const numbers = [...groups.keys()].sort((a, b) =>
      a === "—" ? 1 : b === "—" ? -1 : Number(a) - Number(b),
    );
    const out = [];
    numbers.forEach((key) => {
      const rows = groups.get(key).sort(byKey);
      const tranche = key === "—" ? null : (a.stateByNumber || new Map()).get(Number(key));
      const done = rows.filter((row) => row.state === "completed").length;
      out.push({
        group: true,
        id: `tranche-${key}`,
        label:
          key === "—"
            ? "Tranche not recorded"
            : `Tranche ${key}${tranche ? ` · ${tranche.state || "planned"}` : ""}`,
        sub: `${rows.length} project${rows.length === 1 ? "" : "s"} · ${done} completed`,
      });
      out.push(...rows);
    });
    return out;
  }

  function summary(rows) {
    const all = list(rows).filter((row) => !row.group);
    const inFlight = all.filter((row) => row.state !== "completed").length;
    const spend = all.reduce(
      (acc, row) => {
        if (Number.isFinite(row.tokens)) acc.tokens += row.tokens;
        const n = num(row.usd);
        if (Number.isFinite(n)) acc.usd += n;
        acc.priced += Number(row.priced || 0);
        acc.unpriced += Number(row.unpriced || 0);
        return acc;
      },
      { tokens: 0, usd: 0, priced: 0, unpriced: 0 },
    );
    return {
      count: all.length,
      inFlight,
      completed: all.length - inFlight,
      hasSpend: all.some((row) => Number.isFinite(row.tokens)),
      ...spend,
    };
  }

  /* ---------------------------------------------------------------- the Gantt */
  function domainOf(rows) {
    const visible = list(rows).filter((row) => !row.group);
    const starts = visible.map((row) => row.start).filter(Number.isFinite);
    const ends = visible
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
      out.push({
        at: t,
        label:
          step >= 28 * DAY
            ? `${MONTHS[new Date(t).getUTCMonth()]} ${String(new Date(t).getUTCFullYear()).slice(2)}`
            : `${new Date(t).getUTCDate()} ${MONTHS[new Date(t).getUTCMonth()]}`,
      });
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
    return `<span class="gantt-cost">${esc(spendText(row.tokens, row.usd, row.priced, unpriced))}${
      unpriced > 0
        ? ` <span class="muted" title="${unpriced} call(s) have no commercial price">unpriced</span>`
        : ""
    }</span>`;
  }

  function rowHtml(row, domain) {
    if (row.group) {
      return `<div class="gantt-group"><span>${esc(row.label)}</span><small>${esc(
        row.sub || "",
      )}</small></div>`;
    }
    const label = row.href
      ? `<a class="gantt-name" href="${esc(row.href)}">${esc(row.name)}</a>`
      : `<span class="gantt-name">${esc(row.name)}</span>`;
    const progress =
      row.total > 0
        ? `<span class="gantt-progress" title="${row.done} of ${row.total} cards done">${row.done}/${row.total} cards</span>`
        : "";
    const artefacts = row.artefacts
      ? `<span class="gantt-artefacts">${esc(
          [row.artefacts.trancheSummary, `${row.artefacts.benefitsMeasured}/${row.artefacts.benefits.length} benefits measured`]
            .filter(Boolean)
            .join(" · "),
        )}</span>`
      : "";
    const track = Number.isFinite(row.start)
      ? `${Number.isFinite(row.planStart) && Number.isFinite(row.planEnd) ? `<span class="gantt-plan" style="${bar({ start: row.planStart, end: row.planEnd }, domain)}" title="planned ${esc(day(row.planStart))} → ${esc(day(row.planEnd))}"></span>` : ""}<span class="gantt-actual ${row.state}" style="${bar(row, domain)}" title="${esc(row.state === "completed" ? "completed" : "in progress")} · ${esc(day(row.start))} → ${esc(day(row.end))}"><i></i></span>`
      : `<span class="gantt-nodate muted">no dated activity</span>`;
    return `<div class="gantt-row" data-state="${esc(row.state)}">
      <div class="gantt-label">${label}<small>${esc(row.sub || "")}${
        row.kind === "programme"
          ? ""
          : ` · ${esc(row.status)}${row.type ? ` · ${esc(row.type)}` : ""}`
      }</small>${progress}${artefacts}${costCell(row)}</div>
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
    return `<div class="gantt"><div class="gantt-scroll">
      <div class="gantt-head"><div class="gantt-head-label">${esc(options.label || "Work")}</div><div class="gantt-axis">${axis}${today}</div></div>
      <div class="gantt-body">${body}</div>
    </div><details class="gantt-data"><summary>Table view</summary>${ganttTable(visible)}</details></div>`;
  }

  function ganttTable(rows) {
    const body = list(rows)
      .filter((row) => !row.group)
      .map(
        (row) => `<tr><td><strong>${esc(row.name)}</strong><small>${esc(row.sub || "")}</small></td>
          <td>${esc(row.state === "completed" ? "completed" : "in progress")}</td>
          <td>${esc(day(row.start))}</td><td>${esc(day(row.end))}</td>
          <td>${esc(Number.isFinite(row.tokens) ? tokens(row.tokens) : "—")}</td>
          <td>${esc(
            Number.isFinite(row.tokens)
              ? spendText(row.tokens, row.usd, row.priced, row.unpriced).replace(/^.* tok · /, "")
              : "—",
          )}${
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

  // `children` is a Map on the L3 board and an array on the L4 package (it is exactly the
  // package's own children there); one reader handles both.
  function childrenOf(model, card) {
    if (Array.isArray(model.children)) return model.children;
    return model.children.get(card.id) || [];
  }

  function cardSpend(model, card) {
    const own = model.spend.get(card.id) || null;
    const acc = { tokens: 0, usd: 0, priced: 0, unpriced: 0, any: false };
    addSpend(acc, own);
    childrenOf(model, card).forEach((child) => addSpend(acc, model.spend.get(child.id)));
    return acc.any ? acc : null;
  }

  function tile(card, model) {
    const spend = cardSpend(model, card);
    const run = model.runs.get(card.id);
    const type = model.types.get(card.card_type_id);
    const children = model.children.get(card.id) || [];
    const closed = children.filter((child) => isDone(child, model.laneKinds)).length;
    const isPackage = children.length > 0;
    const issue =
      card.issue != null
        ? `<a class="kanban-issue" href="https://github.com/nonagenticai/conveyor/issues/${encodeURIComponent(card.issue)}" target="_blank" rel="noopener">#${esc(card.issue)}</a>`
        : "";
    // Every card opens L4: the card's own execution, and its children when it has any.
    const head = `<a class="kanban-open" href="${esc(model.wpBase)}/${esc(card.id)}">${esc(
      card.name,
    )}</a>`;
    const dod = type && type.definition && list(type.definition.acceptance).length
      ? `${list(type.definition.acceptance).length} DoD checks`
      : "";
    return `<article class="kanban-card${isPackage ? " work-package" : ""}${isDone(card, model.laneKinds) ? " done" : ""}">
      ${isPackage ? `<span class="kanban-kind">WORK PACKAGE · ${children.length}</span>` : ""}
      ${head}
      <small class="kanban-meta">${esc(type ? type.code : "—")}${card.assignee_name ? ` · ${esc(card.assignee_name)}` : ""} ${issue}</small>
      ${isPackage ? `<span class="kanban-count">${closed}/${children.length} cards done</span>` : ""}
      ${dod ? `<span class="kanban-count">${esc(dod)}</span>` : ""}
      ${run ? `<span class="kanban-run ${run.status === "succeeded" ? "ok" : run.status === "failed" ? "bad" : ""}">${esc(run.status)}</span>` : ""}
      <span class="kanban-cost">${
        spend
          ? esc(spendText(spend.tokens, spend.usd, spend.priced, spend.unpriced))
          : "no spend"
      }</span>
    </article>`;
  }

  // L3: one project's boards. Roots only; a root with children is the work package. Archived cards
  // stay visible in their own column — a closed project's work is still its work.
  function boardModel(work, projectId, measure) {
    const boards = list((work || {}).boards)
      .filter((row) => row.project_id === projectId)
      .sort(byPosition);
    const boardIds = new Set(boards.map((board) => board.id));
    const all = list((work || {}).cards).filter((card) => boardIds.has(card.board_id));
    const open = all.filter((card) => !card.archived_at);
    const archived = all.filter((card) => card.archived_at);
    const openIds = new Set(open.map((card) => card.id));
    const children = new Map();
    open.forEach((card) => {
      // A child whose parent is archived or absent is a loose card, not an orphan tile.
      if (!card.parent_card_id || !openIds.has(card.parent_card_id)) return;
      if (!children.has(card.parent_card_id)) children.set(card.parent_card_id, []);
      children.get(card.parent_card_id).push(card);
    });
    const roots = open.filter(
      (card) => !card.parent_card_id || !openIds.has(card.parent_card_id),
    );
    const lanes = new Map(
      list((work || {}).lists)
        .filter((lane) => boardIds.has(lane.board_id))
        .map((lane) => [lane.id, lane]),
    );
    return {
      projectId,
      boards,
      board: boards[0] || null,
      lanes: [...lanes.values()].sort(byPosition),
      laneMap: lanes,
      roots,
      archived,
      children,
      laneKinds: laneKind(work),
      types: new Map(list((work || {}).card_types).map((row) => [row.id, row])),
      runs: latestRun(work),
      spend: spendByCard(measure),
      wpBase: `#projects/${projectId}/wp`,
    };
  }

  function kanban(model, options = {}) {
    if (!model.boards.length) {
      return `<div class="kanban kanban-empty muted">${esc(
        options.noBoard || "This project has no board in the snapshot; cards and runs need one.",
      )}</div>`;
    }
    if (!model.roots.length && !model.archived.length) {
      return `<div class="kanban kanban-empty muted">${esc(
        options.emptyNote ||
          "No card on this board. Open a card to see its own execution and child cards.",
      )}</div>`;
    }
    const lanes = model.lanes.filter((lane) => lane.kind !== "archive");
    const sections = model.boards.map((board) => {
      const boardRoots = model.roots.filter((card) => card.board_id === board.id);
      const boardLanes = lanes.filter((lane) => lane.board_id === board.id);
      const columns = boardLanes.map((lane) => {
        const cards = boardRoots.filter((card) => card.list_id === lane.id).sort(byPosition);
        return `<section class="kanban-col" data-lane="${esc(lane.kind)}">
          <header><span>${esc(lane.name)}</span><b>${cards.length}</b></header>
          <div class="kanban-cards">${cards.map((card) => tile(card, model)).join("")}</div>
        </section>`;
      });
      const loose = boardRoots.filter((card) => !boardLanes.some((lane) => lane.id === card.list_id));
      if (loose.length) {
        columns.push(`<section class="kanban-col" data-lane="unknown">
          <header><span>No lane</span><b>${loose.length}</b></header>
          <div class="kanban-cards">${loose.map((card) => tile(card, model)).join("")}</div>
        </section>`);
      }
      const archived = model.archived.filter((card) => card.board_id === board.id);
      if (archived.length) {
        columns.push(`<section class="kanban-col kanban-archived" data-lane="archived">
          <header><span>Archived</span><b>${archived.length}</b></header>
          <div class="kanban-cards">${archived
            .sort(byPosition)
            .map((card) => tile(card, model))
            .join("")}</div>
        </section>`);
      }
      const heading =
        model.boards.length > 1
          ? `<div class="kanban-board-head">${esc(board.name)}</div>`
          : "";
      return `${heading}<div class="kanban">${columns.join("")}</div>`;
    });
    return `<div class="kanban-wrap">${sections.join("")}</div>`;
  }

  function runRows(work, card) {
    const runs = list((work || {}).runs)
      .filter((run) => run.card_id === card.id)
      .sort((a, b) => String(b.created_at || "").localeCompare(String(a.created_at || "")));
    const steps = new Map();
    const runIds = new Set(runs.map((run) => run.id));
    list((work || {}).run_steps).forEach((step) => {
      if (!runIds.has(step.run_id)) return;
      steps.set(step.run_id, (steps.get(step.run_id) || 0) + 1);
    });
    return runs.map((run) => ({ ...run, steps: steps.get(run.id) || 0 }));
  }

  // L4's execution record: what actually ran, what the gate said, what "done" means.
  function executionPanel(model, work) {
    const card = model.parent;
    if (!card) return "";
    const runs = runRows(work, card);
    const type = model.types.get(card.card_type_id) || null;
    const dod = type && type.definition ? list(type.definition.acceptance) : [];
    const approvals = list((work || {}).approvals).filter(
      (row) => row.card_id === card.id,
    );
    const spend = cardSpend(model, card);
    const runTable = runs.length
      ? `<div class="e-table-scroll"><table class="e-table"><thead><tr><th>Run</th><th>Status</th><th>Started</th><th>Finished</th><th>Steps</th></tr></thead><tbody>${runs
          .slice(0, 8)
          .map(
            (run) =>
              `<tr><td class="mono">${esc(String(run.id || "").slice(-8))}</td><td>${esc(
                run.status || "—",
              )}</td><td>${esc(shortDate(parse(run.started_at)))}</td><td>${esc(
                shortDate(parse(run.finished_at)),
              )}</td><td>${esc(run.steps)}</td></tr>`,
          )
          .join("")}</tbody></table></div>${
          runs.length > 8
            ? `<p class="muted">Showing the latest 8 of ${runs.length} runs.</p>`
            : ""
        }`
      : `<p class="muted">No run is recorded on this card.</p>`;
    const gate = approvals.length
      ? approvals
          .map(
            (row) =>
              `<span class="rail-chip ${tone(row.status)}" title="${esc(
                row.kind || "",
              )}"><b>${esc(row.gate || "gate")}</b> ${esc(row.status || "")}${
                Number(row.required_decisions || 1) > 1 ? ` · ${row.required_decisions} decisions` : ""
              }</span>`,
          )
          .join("")
      : `<span class="rail-chip">no approval on this card</span>`;
    const defined = dod.length
      ? `<ul class="exec-dod">${dod.map((line) => `<li>${esc(line)}</li>`).join("")}</ul>`
      : `<p class="muted">This card type carries no Definition of Done.</p>`;
    return `<div class="exec">
      <div class="exec-head"><span class="exec-title">Execution</span><span class="muted">${
        runs.length
          ? `${runs.length} run${runs.length === 1 ? "" : "s"} · latest ${esc(runs[0].status || "—")}`
          : "no run recorded"
      }${spend ? ` · ${esc(spendText(spend.tokens, spend.usd, spend.priced, spend.unpriced))}` : " · no spend"}</span></div>
      ${runTable}
      <div class="delivery-rail"><span class="rail-label">Gate</span><div class="rail-chips">${gate}</div></div>
      <div class="exec-dod-wrap"><span class="rail-label">Definition of Done${
        type ? ` · ${esc(type.code)}` : ""
      }</span>${defined}</div>
    </div>`;
  }

  // L4: the work package's own board. The parent lookup is scoped to the named project's boards, so
  // a card id from another project cannot render here. A childless card is a work package too: its
  // execution is the answer, and the page says there are no child cards.
  function packageModel(work, projectId, cardId, measure) {
    const boards = list((work || {}).boards).filter((board) => board.project_id === projectId);
    const boardIds = new Set(boards.map((board) => board.id));
    const cards = list((work || {}).cards).filter((card) => boardIds.has(card.board_id));
    const parent = cards.find((card) => card.id === cardId) || null;
    const children = cards.filter((card) => card.parent_card_id === cardId);
    const lanes = list((work || {}).lists)
      .filter((lane) => boardIds.has(lane.board_id))
      .sort(byPosition);
    return {
      projectId,
      parent,
      children,
      lanes,
      boards,
      laneKinds: laneKind(work),
      types: new Map(list((work || {}).card_types).map((row) => [row.id, row])),
      runs: latestRun(work),
      spend: spendByCard(measure),
      wpBase: `#projects/${projectId}/wp`,
      backHref: `#projects/${projectId}`,
    };
  }

  function packageBoard(model, work) {
    if (!model.parent) {
      return `<div class="kanban kanban-empty muted">This card is not on this project's boards in the published snapshot.</div>`;
    }
    const done = model.children.filter((child) => isDone(child, model.laneKinds)).length;
    const head = `<div class="kanban-package-head"><a class="gantt-name" href="${esc(model.backHref)}">← ${esc(
      model.parent.name,
    )}</a><span class="muted">work package · ${
      model.children.length ? `${done}/${model.children.length} cards done` : "no child cards"
    }</span></div>`;
    const execution = executionPanel(model, work);
    if (!model.children.length) {
      return `<div class="kanban-wrap">${head}${execution}<div class="kanban-empty muted">No child cards. This work package's execution is above; a child card would appear in its lane here.</div></div>`;
    }
    const lanes = model.lanes.filter((lane) => lane.kind !== "archive");
    const columns = lanes.map((lane) => {
      const cards = model.children.filter((card) => card.list_id === lane.id).sort(byPosition);
      return `<section class="kanban-col" data-lane="${esc(lane.kind)}">
        <header><span>${esc(lane.name)}</span><b>${cards.length}</b></header>
        <div class="kanban-cards">${cards.map((card) => packageTile(card, model)).join("")}</div>
      </section>`;
    });
    const orphan = model.children.filter(
      (card) => !lanes.some((lane) => lane.id === card.list_id),
    );
    const archived = model.children.filter((card) => card.archived_at);
    if (orphan.length) {
      columns.push(`<section class="kanban-col" data-lane="unknown"><header><span>No lane</span><b>${orphan.length}</b></header><div class="kanban-cards">${orphan
        .map((card) => packageTile(card, model))
        .join("")}</div></section>`);
    }
    if (archived.length) {
      columns.push(`<section class="kanban-col kanban-archived" data-lane="archived"><header><span>Archived</span><b>${archived.length}</b></header><div class="kanban-cards">${archived
        .map((card) => packageTile(card, model))
        .join("")}</div></section>`);
    }
    return `<div class="kanban-wrap">${head}${execution}<div class="kanban">${columns.join("")}</div></div>`;
  }

  function packageTile(card, model) {
    const spend = model.spend.get(card.id);
    return `<article class="kanban-card${isDone(card, model.laneKinds) ? " done" : ""}">
      <a class="kanban-open" href="${esc(model.wpBase)}/${esc(card.id)}">${esc(card.name)}</a>
      <small class="kanban-meta">${esc((model.types.get(card.card_type_id) || {}).code || "—")}${
        model.runs.get(card.id) ? ` · ${esc(model.runs.get(card.id).status)}` : ""
      }</small>
      <span class="kanban-cost">${
        spend ? esc(spendText(Number(spend.total_tokens), spend.cost_usd, spend.priced_calls, spend.unpriced_calls)) : "no spend"
      }</span>
    </article>`;
  }

  // L3's governance record: the project's own template, plan and TOGAF rows. Absent rows are named.
  function projectGovernance(project, msp, architecture, work) {
    if (!project) return "";
    const meta = project.meta || {};
    const contracts = list((architecture || {}).contracts).filter(
      (row) => row.project_id === project.id,
    );
    const reviews = list((architecture || {}).compliance_reviews || (architecture || {}).reviews).filter(
      (row) => row.project_id === project.id,
    );
    const wbs = list((msp || {}).wbs_nodes).filter((row) => row.project_id === project.id);
    const registers = list((msp || {}).register_items).filter((row) => row.project_id === project.id);
    const baselines = list((msp || {}).project_baselines).filter((row) => row.project_id === project.id);
    const approvals = list((work || {}).approvals).filter((row) => row.project_id === project.id);
    const chips = [
      chip("Type", project.project_type_code || meta.track || "—"),
      chip("Tranche", meta.tranche == null ? "—" : `T${meta.tranche}`),
      chip("Measured", meta.measured_status || "—"),
      project.phase_code ? chip("ADM phase", project.phase_code) : "",
      ...contracts.map((row) =>
        chip(
          "Contract",
          `${row.kind || "contract"} · ${row.phase_code || "—"} · ${row.status || "—"}`,
          row.status,
        ),
      ),
      ...reviews.map((row) =>
        chip("Compliance", `${row.level || "—"} · ${row.outcome || "—"}`, row.level),
      ),
      wbs.length ? chip("WBS", `${wbs.length} nodes`) : "",
      registers.length ? chip("Registers", `${registers.length} items`) : "",
      baselines.length ? chip("Baselines", `${baselines.length}`) : "",
      approvals.length ? chip("Gates", `${approvals.length}`) : "",
    ]
      .filter(Boolean)
      .join("");
    const declared = list(meta.expected_outputs);
    const closure = list(meta.closure_outputs);
    const missing = [];
    if (!contracts.length) missing.push("no architecture contract");
    if (!wbs.length) missing.push("no WBS row");
    if (!registers.length) missing.push("no register row");
    if (!baselines.length) missing.push("no baseline row");
    return `<div class="delivery-gov">
      <div class="delivery-rail"><span class="rail-label">Project record · TOGAF · PMP</span><div class="rail-chips">${chips}</div></div>
      <div class="delivery-rail"><span class="rail-label">Declared artefacts</span><div class="rail-chips">${
        declared.length
          ? chip("expected", `${declared.length}: ${declared.join(", ")}`)
          : `<span class="rail-chip">none declared</span>`
      }${closure.length ? chip("closure", closure.join(", ")) : ""}${
        missing.length ? `<span class="rail-chip">${esc(missing.join(" · "))}</span>` : ""
      }</div></div>
    </div>`;
  }

  return {
    activityIndex,
    portfolioRows,
    programmeRows,
    trancheRows,
    programmeArtefacts,
    trancheRail,
    benefitRail,
    governanceLine,
    projectGovernance,
    executionPanel,
    summary,
    gantt,
    ganttTable,
    boardModel,
    kanban,
    packageModel,
    packageBoard,
    spendText,
    tokens,
    usd,
  };
})();
