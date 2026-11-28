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

  // The tenant's own metering total, when the snapshot publishes no per-project/per-card split.
  // Absence of attribution is named, never rendered as "no spend".
  function tenantSpend(measure) {
    const inference = (measure || {}).inference || {};
    if (list(inference.by_project).length || list(inference.by_card).length) return null;
    const totals = inference.totals;
    if (!totals) return null;
    const row = Array.isArray(totals) ? totals[0] : totals;
    if (!row) return null;
    // A totals row with no amount is not a spend claim; stay silent rather than render "0 tok".
    if (!Number.isFinite(Number(row.total_tokens)) && !Number.isFinite(num(row.cost_usd))) {
      return null;
    }
    const acc = { tokens: 0, usd: 0, priced: 0, unpriced: 0, any: false };
    addSpend(acc, row);
    return acc;
  }

  // When `work.project_activity` is absent (or omits a project), the window is derived from the
  // observed cards and runs, exactly as the signed-in console computes it: first and last event.
  // A project with no card and no run still gets no bar.
  function observedWindows(work) {
    const boardProject = new Map(
      list((work || {}).boards).map((row) => [row.id, row.project_id]),
    );
    const cardProject = new Map();
    const events = new Map();
    const add = (projectId, value) => {
      const at = parse(value);
      if (!projectId || at == null) return;
      const entry = events.get(projectId) || { start: at, end: at };
      entry.start = Math.min(entry.start, at);
      entry.end = Math.max(entry.end, at);
      events.set(projectId, entry);
    };
    list((work || {}).cards).forEach((card) => {
      const projectId = boardProject.get(card.board_id);
      if (!projectId) return;
      cardProject.set(card.id, projectId);
      add(projectId, card.created_at);
      add(projectId, card.list_changed_at);
      add(projectId, card.updated_at);
      add(projectId, card.archived_at);
    });
    list((work || {}).runs).forEach((run) => {
      const projectId = run.project_id || cardProject.get(run.card_id);
      add(projectId, run.created_at);
      add(projectId, run.started_at);
      add(projectId, run.finished_at);
    });
    return events;
  }

  // A code finding's evidence is a location (`path:line`); any other evidence is its own text.
  function evidenceText(value) {
    if (value == null || value === "") return "";
    if (typeof value === "string") return value;
    if (Array.isArray(value)) return list(value).map(evidenceText).filter(Boolean).join(" · ");
    if (typeof value === "object") {
      const path = value.path || value.file || value.location;
      const line = value.line != null ? value.line : value.line_start;
      if (path && line != null) return `${path}:${line}`;
      if (path) return String(path);
      // An artefact finding names the artefact and the section it lacks, plus the snapshot
      // containers that were empty. Without this the register's package panel said "no evidence
      // reference" for every one of the artefact packages.
      if (value.artefact) {
        const section = value.section ? ` · ${value.section}` : "";
        const snapshot = list(value.snapshot)
          .map((entry) =>
            Array.isArray(entry)
              ? entry.filter((part) => part != null && part !== "").join("#")
              : evidenceText(entry),
          )
          .filter(Boolean)
          .join(", ");
        return `${value.artefact}${section}${snapshot ? ` (${snapshot})` : ""}`;
      }
      if (value.evidence) return evidenceText(value.evidence);
      if (value.subject) return evidenceText(value.subject);
      if (value.detail) return evidenceText(value.detail);
      return "";
    }
    return String(value);
  }

  // A tile cap keeps a lane readable; the note carries the full remainder, never a silent cut.
  const TILE_CAP = 25;
  function cardColumn(cards, render) {
    const all = list(cards);
    const shown = all.slice(0, TILE_CAP);
    const rest = all.length - shown.length;
    return `${shown.map(render).join("")}${
      rest > 0 ? `<div class="kanban-more">${rest} more</div>` : ""
    }`;
  }

  // WSJF only exists when job_size is positive and all three components are measured.
  function wsjfOf(node) {
    const size = num(node.job_size);
    if (!Number.isFinite(size) || size <= 0) return null;
    const parts = [node.business_value, node.time_criticality, node.risk_reduction].map(num);
    if (!parts.every(Number.isFinite)) return null;
    return parts.reduce((sum, value) => sum + value, 0) / size;
  }

  // `rework.apply_to` names a project by id or key; both resolve to the same route.
  function appliesTo(row, projectId, projectKey) {
    const target = row && row.apply_to;
    if (target == null) return false;
    return target === projectId || (projectKey != null && target === projectKey);
  }

  function projectRef(project) {
    if (project && typeof project === "object") {
      return { id: project.id, key: project.key || null };
    }
    return { id: project, key: null };
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
    // The realisation rows carry the project and the benefit reference; they are scoped by the
    // programme's own benefit references (a row for another programme's benefit is not ours).
    const realisationPublished = Array.isArray(M.benefit_realisation);
    const refs = new Set(benefits.map((row) => row.reference).filter(Boolean));
    const realisation = realisationPublished
      ? list(M.benefit_realisation)
          .filter((row) => !row.programme_id || row.programme_id === programmeId)
          .filter((row) => !refs.size || refs.has(row.reference))
      : [];
    const realisationByRef = new Map(realisation.map((row) => [row.reference, row]));
    const measured = benefits.filter((row) => benefitMeasured(row, transitions));
    const realised = benefits.filter((row) => benefitRealised(row, transitions));
    // A realisation claim counts only with a measured baseline / a numeric post measurement.
    const measuredRealisation = realisation.filter(
      (row) => row.baseline && row.baseline.measured_at && !row.baseline.placeholder,
    );
    const realisedRealisation = realisation.filter(
      (row) =>
        String(row.state || "").toLowerCase() === "realised" &&
        Number.isFinite(num(row.latest_post)),
    );
    const stateByNumber = new Map(tranches.map((row) => [Number(row.number), row]));
    return {
      tranches,
      benefits,
      transitions,
      reviews,
      stakeholders,
      links,
      cadences,
      realisationPublished,
      realisation,
      realisationByRef,
      stateByNumber,
      trancheSummary: trancheSummary(tranches),
      benefitsMeasured: realisationPublished ? measuredRealisation.length : measured.length,
      benefitsRealised: realisationPublished ? realisedRealisation.length : realised.length,
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

  // MSP benefit rail. When the register publishes `benefit_realisation`, that row is the truth
  // (state, measured vs placeholder baseline, delta); otherwise the older profile/transition rows
  // are read. Every profile renders — nothing is silently truncated.
  function benefitRail(artefacts) {
    const a = artefacts || {};
    const benefits = list(a.benefits);
    const byRef = a.realisationByRef || new Map();
    const useRealisation = Boolean(a.realisationPublished);
    const chips = benefits
      .map((row) => {
        if (useRealisation) {
          const real = byRef.get(row.reference) || null;
          const state = real ? realisationState(real) : "—";
          const baseline = real ? baselineText(real.baseline) : "baseline —";
          // The baseline state is visible, not hidden in a title: a phone has no hover.
          const baselineMark = real && real.baseline
            ? real.baseline.placeholder
              ? "baseline placeholder"
              : real.baseline.measured_at
                ? "baseline measured"
                : `baseline ${
                    real.baseline.value == null || real.baseline.value === ""
                      ? "—"
                      : real.baseline.value
                  }`
            : "baseline —";
          const delta =
            real && Number.isFinite(num(real.delta))
              ? ` · Δ ${num(real.delta) > 0 ? "+" : ""}${num(real.delta)}`
              : "";
          const target =
            real && real.target != null
              ? ` · target ${real.target}`
              : String(row.target || row.observable_outcome || "").trim()
                ? ` · target ${String(row.target || row.observable_outcome).trim()}`
                : "";
          const title = [
            baseline,
            real && real.measurement_method
              ? `method: ${real.measurement_method}`
              : row.measurement_method
                ? `method: ${row.measurement_method}`
                : "",
            real && real.owner_role ? `owner: ${real.owner_role}` : "",
            real && real.latest_post != null ? `latest post: ${real.latest_post}` : "",
          ]
            .filter(Boolean)
            .join(" · ");
          return chip(
            row.reference || String(row.id || "").slice(0, 8),
            `${row.description || ""} · ${state} · ${baselineMark}${delta}${target}`,
            state === "realised"
              ? "closed"
              : state === "baselined" || state === "transitioned"
                ? "in_review"
                : state === "retired"
                  ? "planned"
                  : "",
            title,
          );
        }
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
    const count = benefits.length
      ? `${a.benefitsRealised}/${benefits.length} realised · ${a.benefitsMeasured} with a measured baseline`
      : "";
    return `<div class="delivery-rail"><span class="rail-label">MSP benefits${esc(
      count ? ` · ${count}` : "",
    )}</span><div class="rail-chips">${
      chips || `<span class="rail-chip">no benefit profile published</span>`
    }</div></div>`;
  }

  // A claimed "realised" without a numeric post measurement is not realised; say "—" instead.
  function realisationState(row) {
    const state = String(row.state || "").toLowerCase();
    if (!state) return "—";
    if (state === "realised" && !Number.isFinite(num(row.latest_post))) return "—";
    return state;
  }

  function baselineText(baseline) {
    if (!baseline) return "baseline —";
    const value = baseline.value == null || baseline.value === "" ? "—" : baseline.value;
    if (baseline.placeholder) return `baseline ${value} (placeholder)`;
    if (baseline.measured_at) {
      return `baseline ${value} (measured ${shortDate(parse(baseline.measured_at))})`;
    }
    return `baseline ${value}`;
  }

  // MSP dependency register: typed/untyped edges, unknown targets and cycles.
  function dependencyRail(dependencies) {
    const d = dependencies || null;
    if (!d || (!d.edges && !d.counts)) {
      return `<div class="delivery-rail"><span class="rail-label">Dependencies</span><div class="rail-chips"><span class="rail-chip">no dependency row published</span></div></div>`;
    }
    const edges = list(d.edges);
    const counts = d.counts || {};
    const typed = Number.isFinite(Number(counts.typed))
      ? Number(counts.typed)
      : edges.filter((row) => row && row.typed).length;
    const untyped = Number.isFinite(Number(counts.untyped))
      ? Number(counts.untyped)
      : edges.filter((row) => row && !row.typed).length;
    const total = Number.isFinite(Number(counts.total)) ? Number(counts.total) : edges.length;
    const unknown = list(d.unknown_targets);
    const cycles = list(d.cycles);
    const chips = [
      chip(
        "Edges",
        `${total} · ${typed} typed · ${untyped} untyped`,
        untyped > 0 ? "in_review" : "approved",
      ),
      unknown.length
        ? chip(
            "Unknown targets",
            `${unknown.length}: ${unknown
              .slice(0, 3)
              .map((row) => `${row.from || "—"}→${row.to || "—"}`)
              .join(", ")}${unknown.length > 3 ? ` +${unknown.length - 3}` : ""}`,
            "planned",
          )
        : "",
      cycles.length
        ? chip(
            "Cycles",
            `${cycles
              .slice(0, 3)
              .map((cycle) => list(cycle).join(" → "))
              .join(" · ")}${cycles.length > 3 ? ` +${cycles.length - 3}` : ""}`,
            "planned",
          )
        : "",
      !unknown.length && !cycles.length
        ? chip("Integrity", "no unknown target · no cycle", "approved")
        : "",
    ]
      .filter(Boolean)
      .join("");
    return `<div class="delivery-rail"><span class="rail-label">Dependencies</span><div class="rail-chips">${chips}</div></div>`;
  }

  // MSP tranche reconciliation: declared state vs the review's authorisation, with its flags.
  function reconciliationRail(rows) {
    const listRows = list(rows);
    if (!listRows.length) {
      return `<div class="delivery-rail"><span class="rail-label">Tranche reconciliation</span><div class="rail-chips"><span class="rail-chip">no tranche reconciliation published</span></div></div>`;
    }
    const chips = listRows
      .sort(byNumber)
      .map((row) => {
        const review = row.review
          ? `${row.review.decision || "—"} ${shortDate(parse(row.review.decided_at))}`
          : "no review";
        const projects = row.projects || {};
        const counts = [
          projects.total != null
            ? `${projects.active == null ? "—" : projects.active}/${projects.total} active`
            : "",
          projects.closed != null ? `${projects.closed} closed` : "",
          projects.archived ? `${projects.archived} archived` : "",
        ]
          .filter(Boolean)
          .join(" · ");
        const flags = list(row.flags);
        return chip(
          `T${row.number == null ? "—" : row.number}`,
          `${row.declared_state || "—"} · ${row.authorised ? "authorised" : "not authorised"} · ${review}${
            counts ? ` · ${counts}` : ""
          }${flags.length ? ` · ${flags.length} flag${flags.length === 1 ? "" : "s"}` : ""}`,
          flags.length ? "in_review" : row.authorised ? "authorised" : "planned",
          flags.join(" · "),
        );
      })
      .join("");
    return `<div class="delivery-rail"><span class="rail-label">Tranche reconciliation</span><div class="rail-chips">${chips}</div></div>`;
  }

  // TOGAF A–H phase progress per project. Each cell names its own contract and compliance state.
  function phaseStrip(phaseProgress, projects) {
    const rows = list(phaseProgress);
    if (!rows.length) {
      return `<div class="delivery-rail"><span class="rail-label">TOGAF A–H phases</span><div class="rail-chips"><span class="rail-chip">phase progress not published</span></div></div>`;
    }
    const ids = new Set();
    const keys = new Set();
    list(projects).forEach((row) => {
      if (row && row.id) ids.add(row.id);
      if (row && row.key) keys.add(row.key);
    });
    const mine = rows.filter((row) => ids.has(row.project) || keys.has(row.project));
    const shown = list(projects).length ? mine : rows;
    if (!shown.length) {
      return `<div class="delivery-rail"><span class="rail-label">TOGAF A–H phases</span><div class="rail-chips"><span class="rail-chip">no phase progress for this programme's projects</span></div></div>`;
    }
    const rank = (value) => {
      const text = String(value || "").toUpperCase();
      if (/^PREL/.test(text)) return 0;
      const code = text.charCodeAt(0) - 64;
      return code >= 1 && code <= 8 ? code : 99;
    };
    const lines = shown
      .map((row) => {
        const phases = list(row.phases)
          .slice()
          .sort((a, b) => rank(a.phase) - rank(b.phase));
        const cells = phases
          .map((phase) => {
            const title = [
              `state ${phase.state || "—"}`,
              phase.contract ? `contract ${phase.contract}` : "",
              phase.compliance ? `compliance ${phase.compliance}` : "",
            ]
              .filter(Boolean)
              .join(" · ");
            return `<span class="phase-cell ${tone(phase.state)}" title="${esc(title)}"><b>${esc(
              phase.phase || "—",
            )}</b><small>${esc(phase.state || "—")}</small></span>`;
          })
          .join("");
        const declared = row.declared
          ? chip(
              "declared",
              `${row.declared.adm_phase || "—"}${
                row.declared.plateau ? ` · plateau ${row.declared.plateau}` : ""
              }`,
            )
          : chip("declared", "—");
        const next = row.next_required ? chip("next", row.next_required) : "";
        return `<div class="phase-row"><span class="phase-project">${esc(
          row.project || "—",
        )}</span><div class="phase-strip">${
          cells || `<span class="muted">no phase row</span>`
        }</div>${declared}${next}</div>`;
      })
      .join("");
    return `<div class="delivery-rail phase-rail"><span class="rail-label">TOGAF A–H phases</span><div class="phase-rows">${lines}</div></div>`;
  }

  // SAFe cadences: the programme's own rows, or the absence.
  function cadenceRail(artefacts) {
    const rows = list((artefacts || {}).cadences);
    const chips = rows
      .map((row) => {
        const kind = row.kind || row.name || "cadence";
        const cadence = row.cadence || row.frequency || row.rhythm || row.period || "";
        const owner = row.owner_role || row.owner || "";
        return chip(
          kind,
          [cadence, owner].filter(Boolean).join(" · "),
          row.state || "active",
        );
      })
      .join("");
    return `<div class="delivery-rail"><span class="rail-label">SAFe cadence</span><div class="rail-chips">${
      chips || `<span class="rail-chip">no cadence row</span>`
    }</div></div>`;
  }

  // Rework register grouped by standard: open P0/P1/P2 counts and one chip per package. A package
  // whose `apply_to` names a project links to that project's board; the rest say what they apply to.
  function reworkRail(rework, projects) {
    const register = rework || null;
    const packages = list(register && register.packages);
    if (!packages.length) {
      return `<div class="delivery-rail"><span class="rail-label">Rework</span><div class="rail-chips"><span class="rail-chip">${
        register ? "rework register · no package" : "rework register not published"
      }</span></div></div>`;
    }
    const target = new Map();
    list(projects).forEach((row) => {
      if (!row) return;
      if (row.id) target.set(row.id, row.id);
      if (row.key) target.set(row.key, row.id);
    });
    const open = packages.filter((row) => String(row.status || "open") !== "suppressed");
    const groups = new Map();
    open.forEach((row) => {
      const standard = String(row.standard || "—");
      if (!groups.has(standard)) groups.set(standard, { p0: 0, p1: 0, p2: 0 });
      const severity = String(row.severity || "").toUpperCase();
      if (severity === "P0" || severity === "P1" || severity === "P2") {
        groups.get(standard)[severity.toLowerCase()] += 1;
      }
    });
    const counts = [...groups.entries()]
      .map(
        ([standard, group]) =>
          `<span class="rail-chip"><b>${esc(standard)}</b> P0 ${group.p0} · P1 ${group.p1} · P2 ${group.p2}</span>`,
      )
      .join("");
    const items = open
      .slice(0, 12)
      .map((row) => {
        const label = `${row.severity ? String(row.severity).toUpperCase() + " · " : ""}${
          row.rule || row.key || "package"
        }`;
        const title = [row.title || "", row.apply_to ? `applies to ${row.apply_to}` : ""]
          .filter(Boolean)
          .join(" · ");
        const severity = String(row.severity || "").toLowerCase();
        const body = `${esc(label)}${
          row.title ? ` <small>${esc(row.title)}</small>` : ""
        }`;
        const id = target.get(row.apply_to);
        return id
          ? `<a class="rail-chip sev-${esc(severity)}" href="#projects/${esc(
              id,
            )}" title="${esc(title)}">${body}</a>`
          : `<span class="rail-chip sev-${esc(severity)}" title="${esc(title)}">${body}</span>`;
      })
      .join("");
    const more =
      open.length > 12
        ? `<span class="rail-chip">showing 12 of ${open.length} packages</span>`
        : "";
    const suppressed = packages.length - open.length;
    return `<div class="delivery-rail"><span class="rail-label">Rework${
      suppressed ? ` · ${suppressed} suppressed` : ""
    }</span><div class="rail-chips">${counts}${items}${more}</div></div>`;
  }

  // The output register (#756 P2): per project, the kinds the standard declares against the
  // kinds the project published. A missing kind is named, never rounded away, and a project no
  // class matches says so. The conformance gap register's posture rides beside it. A project
  // chip drills into that project's page.
  function registerRail(register, projects) {
    const data = register || null;
    if (!data) {
      return `<div class="delivery-rail"><span class="rail-label">Output register</span><div class="rail-chips"><span class="rail-chip">register not published</span></div></div>`;
    }
    const rows = list(data.projects);
    const totals = data.totals || {};
    const target = new Map();
    list(projects).forEach((row) => {
      if (!row) return;
      if (row.id) target.set(row.id, row.id);
      if (row.key) target.set(row.key, row.id);
    });
    const missingOf = (row) =>
      list(row.missing && row.missing.now).length + list(row.missing && row.missing.closure).length;
    const gapped = rows.filter((row) => missingOf(row) > 0);
    const classChips = list(data.classes)
      .map((cls) => {
        const count = list(cls.projects).length;
        return `<span class="rail-chip"><b>${esc(cls.name)}</b> ${count} project${
          count === 1 ? "" : "s"
        }</span>`;
      })
      .join("");
    const items = gapped
      .slice(0, 8)
      .map((row) => {
        const missing = [
          ...list(row.missing && row.missing.now),
          ...list(row.missing && row.missing.closure),
        ];
        const declared =
          list(row.declared && row.declared.now).length +
          list(row.declared && row.declared.closure).length;
        const title = `${row.key} · class ${row.class || "unmatched"} · declared ${declared} · evidenced ${
          list(row.evidenced).length
        } · missing ${missing.join(", ")}`;
        const id = target.get(row.id) || target.get(row.key);
        const label = `${esc(row.key)} · ${missing.length} missing`;
        const links = [];
        if (id) links.push(`<a href="#projects/${esc(id)}">page</a>`);
        const issue = list(row.issues)[0];
        if (issue) {
          links.push(
            `<a href="https://github.com/nonagenticai/conveyor/issues/${esc(issue)}">#${esc(issue)}</a>`,
          );
        }
        const drill = links.length ? ` · ${links.join(" · ")}` : "";
        return `<span class="rail-chip sev-p1" title="${esc(title)}">${label}${drill}</span>`;
      })
      .join("");
    const more =
      gapped.length > 8
        ? `<span class="rail-chip">showing 8 of ${gapped.length} projects with a gap</span>`
        : "";
    const unmatched = Number(totals.projects_without_class || 0);
    const posture = data.gaps
      ? `<span class="rail-chip">conformance gaps · ${Object.entries(data.gaps.counts || {})
          .map(([severity, count]) => `${esc(severity)} ${esc(count)}`)
          .join(" · ")}</span>`
      : `<span class="rail-chip">conformance gaps not recorded</span>`;
    return `<div class="delivery-rail"><span class="rail-label">Output register · ${esc(
      totals.projects || rows.length,
    )} projects · ${esc(totals.declared || 0)} declared · ${esc(
      totals.evidenced || 0,
    )} evidenced · ${esc(totals.missing || 0)} missing${
      unmatched ? ` · ${unmatched} with no class` : ""
    }</span><div class="rail-chips">${classChips}${items}${more}${posture}</div></div>`;
  }

  // L1 scorecard: one tile per standard from the register's own scorecard, plus the delivery tile.
  // A standard the register does not score shows "—"; a denominator is never invented.
  function scorecard(rework, delivery) {
    const register = rework || null;
    const score = register && register.scorecard ? register.scorecard : null;
    const d = delivery || {};
    if (!score) {
      return `<div class="delivery-scorecard"><span class="muted">rework register not published</span></div>`;
    }
    const openByStandard = new Map();
    list(register.packages).forEach((row) => {
      if (String(row.status || "open") === "suppressed") return;
      const standard = String(row.standard || "—");
      openByStandard.set(standard, (openByStandard.get(standard) || 0) + 1);
    });
    const tiles = ["MSP", "TOGAF", "SAFe", "PMP", "SPEC"]
      .map((standard) => {
        const row = score[standard] || null;
        const clean =
          row && Number.isFinite(num(row.rules_clean)) && Number.isFinite(num(row.rules))
            ? `${num(row.rules_clean)}/${num(row.rules)}`
            : "—";
        const counted = openByStandard.get(standard);
        const open =
          counted != null
            ? counted
            : row && Number.isFinite(num(row.packages)) && Number.isFinite(num(row.suppressed))
              ? Math.max(num(row.packages) - num(row.suppressed), 0)
              : "—";
        const findings = row && Number.isFinite(num(row.findings)) ? num(row.findings) : "—";
        const gaps =
          row && Number.isFinite(num(row.rules_with_gaps))
            ? `${num(row.rules_with_gaps)} rule${num(row.rules_with_gaps) === 1 ? "" : "s"} with gaps`
            : "";
        return `<div class="score-tile${row ? "" : " absent"}">
          <span class="score-standard">${esc(standard)}</span>
          <span class="score-value"><b>${esc(clean)}</b> clean</span>
          <span class="score-meta">${open === "—" ? "open —" : `${open} open`} · ${
            findings === "—" ? "findings —" : `${findings} findings`
          }</span>
          ${row ? "" : `<span class="score-meta">not in the register</span>`}
          ${gaps ? `<span class="score-meta">${esc(gaps)}</span>` : ""}
        </div>`;
      })
      .join("");
    const deliveryTile = `<div class="score-tile score-delivery">
      <span class="score-standard">Delivery</span>
      <span class="score-value"><b>${
        Number.isFinite(num(d.inFlight)) ? num(d.inFlight) : "—"
      }</b> in flight · <b>${
        Number.isFinite(num(d.completed)) ? num(d.completed) : "—"
      }</b> completed</span>
      <span class="score-meta">${esc(d.spend || "spend —")}</span>
    </div>`;
    const generated = register.generated_at
      ? `<p class="score-foot muted">register generated ${esc(day(parse(register.generated_at)))}</p>`
      : "";
    return `<div class="delivery-scorecard">${tiles}${deliveryTile}</div>${generated}`;
  }

  // SAFe cadence and the TOGAF capability the programme runs under. Both are the snapshot's rows;
  // an empty set is named, not faked.
  function governanceLine(artefacts, architecture) {
    const a = artefacts || {};
    const arch = architecture || {};
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
    return `<div class="delivery-rail"><span class="rail-label">TOGAF capability</span><div class="rail-chips"><span class="rail-chip ${
      profile ? tone(profile.status) : ""
    }">${capability}</span><span class="rail-chip">${esc(counts)}</span></div></div>`;
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
    // No `project_activity` row is not "no activity": the same first/last-event rule the signed-in
    // console uses derives the window from the cards and runs that are in the snapshot.
    const derived = observedWindows(work);
    const inference = (measure || {}).inference || {};
    const usage = new Map(list(inference.by_project).map((row) => [row.project_id, row]));
    const boardProject = new Map(
      list((work || {}).boards).map((row) => [row.id, row.project_id]),
    );
    const boardCount = new Map();
    boardProject.forEach((projectId) => {
      boardCount.set(projectId, (boardCount.get(projectId) || 0) + 1);
    });
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
      const observed = derived.get(project.id) || null;
      const window =
        activity.get(project.id) ||
        (observed
          ? {
              started_at: new Date(observed.start).toISOString(),
              last_activity_at: new Date(observed.end).toISOString(),
              derived: true,
            }
          : null);
      const completed = project.status === "closed" || project.status === "archived";
      const start = window ? parse(window.started_at) : null;
      const observedEnd = window ? parse(window.last_activity_at) : null;
      const end = start == null ? null : completed ? observedEnd ?? start : now;
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
        hasBoard: (boardCount.get(project.id) || 0) > 0,
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

  // L1's rows for the page. The portfolio READ MODEL stays in flight: SPEC_V2 §5.1 defines the
  // portfolio as "every project and programme in flight for the tenant" and says closed and
  // archived work is history and is not listed. The owner asked this page for work in progress
  // AND completed, including its token cost, so the page appends a clearly labelled history
  // section BELOW the in-flight Gantt: closed programmes and closed standalone projects with an
  // observed span. Archived work stays out (it is history in §5.1's sense and still renders at
  // L2 inside its programme); the in-flight projection (`portfolio.in_flight`) is unchanged.
  // A completed row with no observed event is not invented — it stays absent, so no bar is
  // manufactured from a plan date or `created_at`.
  //
  // The caller concatenates `programmes` then `standalone` (v2.js), so the history marker and
  // its rows ride the tail of `standalone`: that is the only position that renders after every
  // in-flight row without changing the caller. They are page rows, not an in-flight model row.
  function portfolioRows(programme, index, clock, msp) {
    const rows = list((programme || {}).programme);
    const membersOf = (id) => [...index.values()].filter((p) => p.programmeId === id);
    const programmes = rows
      .filter((row) => row.status !== "closed" && row.status !== "archived")
      .map((row) => ({
        ...rollUp(row, membersOf(row.id), clock),
        artefacts: programmeArtefacts(msp, row.id),
      }));
    // A standalone project is public only when it is active/paused AND carries a board or a card
    // (SPEC_V2 §5.1); a draft or a bare session fixture is not a portfolio row.
    const standalone = [...index.values()].filter(
      (p) =>
        !p.programmeId &&
        ["active", "paused"].includes(String(p.status || "").toLowerCase()) &&
        (p.hasBoard || p.total > 0),
    );
    // Completed history: closed programmes and closed standalone projects, each only when the
    // snapshot carries an observed span for it. Archived rows are deliberately not listed here.
    const completed = [
      ...rows
        .filter((row) => row.status === "closed")
        .map((row) => ({
          ...rollUp(row, membersOf(row.id), clock),
          artefacts: programmeArtefacts(msp, row.id),
        }))
        .filter((row) => Number.isFinite(row.start)),
      ...[...index.values()]
        .filter(
          (p) =>
            !p.programmeId &&
            String(p.status || "").toLowerCase() === "closed" &&
            (p.hasBoard || p.total > 0) &&
            Number.isFinite(p.start),
        )
        .sort((a, b) => (b.end ?? b.start) - (a.end ?? a.start)),
    ];
    if (!completed.length) return { programmes, standalone };
    const marker = {
      group: true,
      section: "history",
      id: "history-completed",
      label: "Completed · history",
      sub: `${completed.length} finished row${completed.length === 1 ? "" : "s"} · observed span`,
    };
    return { programmes, standalone: [...standalone, marker, ...completed] };
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
    // A completed row says so in words, not only in `data-state` and a hover title, and carries
    // its observed span as text: a phone has no title attribute. History rows always have a
    // finite start (portfolioRows filters the rest), so this is the span the bar itself spans.
    const state =
      row.state === "completed"
        ? `<span class="gantt-state completed rail-chip ok">completed</span>${
            Number.isFinite(row.start) && Number.isFinite(row.end)
              ? `<small class="gantt-span">${esc(day(row.start))} → ${esc(day(row.end))}</small>`
              : ""
          }`
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
      }</small>${state}${progress}${artefacts}${costCell(row)}</div>
      <div class="gantt-track">${track}</div>
    </div>`;
  }

  // One Gantt block: its own axis domain, rows and table. `gantt` composes one block for the
  // in-flight rows and, when the page appended a history marker, a second one below it. A block
  // with its own domain is what keeps an old completed span from flattening today's bars.
  function ganttBlock(rows, options = {}) {
    const visible = list(rows).filter((row) => row && !row.section);
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
    return `<div class="gantt${options.section ? ` gantt-${esc(options.section)}` : ""}"${
      options.section === "history" ? ' style="margin-top:12px"' : ""
    }><div class="gantt-scroll">
      <div class="gantt-head"><div class="gantt-head-label">${esc(options.label || "Work")}</div><div class="gantt-axis">${axis}${today}</div></div>
      <div class="gantt-body">${body}</div>
    </div><details class="gantt-data"><summary>Table view</summary>${ganttTable(visible)}</details></div>`;
  }

  // The page's one Gantt call. A `section: "history"` marker (portfolioRows) splits the rows: the
  // in-flight block keeps its own domain, and the completed block gets the marker's label and a
  // `gantt-history` class below it. No marker means the ordinary single block.
  function gantt(rows, options = {}) {
    const visible = list(rows).filter(Boolean);
    const marker = visible.find((row) => row && row.section === "history") || null;
    if (!marker) return ganttBlock(visible, options);
    const at = visible.indexOf(marker);
    const live = visible.slice(0, at);
    const history = visible.slice(at + 1);
    const head = live.length ? ganttBlock(live, options) : "";
    const tail = history.length
      ? ganttBlock(history, {
          ...options,
          label: marker.label || "Completed · history",
          section: "history",
          emptyNote: "No completed row carries an observed span.",
        })
      : "";
    return head || tail ? `${head}${tail}` : ganttBlock([], options);
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
    // A rework card carries its register key, rule and severity; the tile says so on the board.
    const rework = card.rework_key
      ? `<span class="kanban-rework sev-${esc(
          String(card.severity || "").toLowerCase(),
        )}">${esc(String(card.severity || "—").toUpperCase())} · ${esc(
          card.rule || card.rework_key,
        )}</span>`
      : "";
    return `<article class="kanban-card${isPackage ? " work-package" : ""}${isDone(card, model.laneKinds) ? " done" : ""}">
      ${isPackage ? `<span class="kanban-kind">WORK PACKAGE · ${children.length}</span>` : ""}
      ${head}
      <small class="kanban-meta">${esc(type ? type.code : "—")}${card.assignee_name ? ` · ${esc(card.assignee_name)}` : ""} ${issue}</small>
      ${rework}
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
  // stay visible in their own column — a closed project's work is still its work. The WBS plan and
  // the rework register are the project's own rows; `project` may be the row or its id.
  function boardModel(work, project, measure, msp, rework) {
    const ref = projectRef(project);
    const projectId = ref.id;
    const boards = list((work || {}).boards)
      .filter((row) => row.project_id === projectId)
      .sort(byPosition);
    const boardIds = new Set(boards.map((board) => board.id));
    const all = list((work || {}).cards).filter((card) => boardIds.has(card.board_id));
    const open = all.filter((card) => !card.archived_at);
    const archived = all.filter((card) => card.archived_at);
    const openIds = new Set(open.map((card) => card.id));
    const children = new Map();
    // Children include archived ones: a work package's progress counts the child that is done.
    // A child whose parent is archived or absent is a loose card, not an orphan tile.
    all.forEach((card) => {
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
    const wbs = list((msp || {}).wbs_nodes).filter(
      (row) => row.project_id === projectId || (ref.key && row.project_id === ref.key),
    );
    const plan = wbs.filter((node) =>
      /^(work[_-]?package|wp|package)$/i.test(String(node.kind || "")),
    );
    const packages = list((rework || {}).packages).filter((row) =>
      appliesTo(row, projectId, ref.key),
    );
    // A card and a register package match on the card's `rework_key` (the package key) or its rule.
    const cardKeys = new Set();
    all.forEach((card) => {
      if (card.rework_key) cardKeys.add(card.rework_key);
      if (card.rule) cardKeys.add(card.rule);
    });
    const unmatchedRework = packages.filter(
      (row) => !cardKeys.has(row.key) && !cardKeys.has(row.rule),
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
      unattributed: tenantSpend(measure),
      wbs,
      plan,
      reworkCards: roots.filter((card) => card.rework_key),
      reworkPackages: packages,
      unmatchedRework,
      wpBase: `#projects/${projectId}/wp`,
    };
  }

  // L3's Plan lane: the selected project's WBS work packages (never its phase/deliverable rows).
  // Acceptance is the node's own check count; WSJF appears only when job_size is positive.
  function planColumn(model) {
    const nodes = list(model.plan);
    const body = cardColumn(nodes, (node) => {
      const acceptance = list(node.acceptance).length;
      const wsjf = wsjfOf(node);
      const size = num(node.job_size);
      return `<article class="kanban-card plan-card">
        <span class="kanban-open">${esc(node.title || node.id || "—")}</span>
        <small class="kanban-meta">${esc(node.kind || "wbs")} · ${esc(node.status || "—")}</small>
        ${acceptance ? `<span class="kanban-count">${acceptance} acceptance check${acceptance === 1 ? "" : "s"}</span>` : ""}
        ${wsjf != null ? `<span class="kanban-count">WSJF ${wsjf.toFixed(1)}</span>` : size > 0 ? `<span class="kanban-count">WSJF —</span>` : ""}
      </article>`;
    });
    const empty = nodes.length
      ? ""
      : `<div class="kanban-more">${
          list(model.wbs).length
            ? `no WBS work package row (${list(model.wbs).length} node${list(model.wbs).length === 1 ? "" : "s"} of other kinds)`
            : "no WBS work package row"
        }</div>`;
    return `<section class="kanban-col kanban-plan" data-lane="plan"><header><span>Plan · WBS</span><b>${nodes.length}</b></header><div class="kanban-cards">${body}${empty}</div></section>`;
  }

  // A register package with no card yet is named as such; it links nowhere because there is no L4.
  function notMaterialisedTile(pkg) {
    return `<article class="kanban-card not-materialised">
      <span class="kanban-open">${esc(pkg.title || pkg.key || "rework package")}</span>
      <small class="kanban-meta">${esc(
        [pkg.standard, pkg.rule, String(pkg.severity || "").toUpperCase()]
          .filter(Boolean)
          .join(" · "),
      )}</small>
      <span class="kanban-count">not yet materialised</span>
    </article>`;
  }

  // L3's Rework lane: cards carrying a `rework_key`, plus register packages not yet materialised.
  function reworkColumn(model) {
    const cards = list(model.reworkCards);
    const pending = list(model.unmatchedRework);
    const total = cards.length + pending.length;
    return `<section class="kanban-col kanban-rework" data-lane="rework"><header><span>Rework</span><b>${total}</b></header><div class="kanban-cards">${cardColumn(
      cards,
      (card) => tile(card, model),
    )}${cardColumn(pending, notMaterialisedTile)}${
      total ? "" : `<div class="kanban-more">no rework card on this project</div>`
    }</div></section>`;
  }

  function kanban(model, options = {}) {
    const side =
      list(model.plan).length ||
      list(model.reworkCards).length ||
      list(model.unmatchedRework).length;
    if (!model.boards.length && !side) {
      return `<div class="kanban kanban-empty muted">${esc(
        options.noBoard || "This project has no board in the snapshot; cards and runs need one.",
      )}</div>`;
    }
    if (!model.roots.length && !model.archived.length && !side) {
      return `<div class="kanban kanban-empty muted">${esc(
        options.emptyNote ||
          "No card on this board. Open a card to see its own execution and child cards.",
      )}</div>`;
    }
    const lanes = model.lanes.filter((lane) => lane.kind !== "archive");
    const note = model.unattributed
      ? `<p class="muted kanban-note">${esc(
          spendText(
            model.unattributed.tokens,
            model.unattributed.usd,
            model.unattributed.priced,
            model.unattributed.unpriced,
          ),
        )} unattributed to a project.</p>`
      : "";
    const sections = model.boards.map((board, index) => {
      const boardRoots = model.roots.filter((card) => card.board_id === board.id);
      const boardLanes = lanes.filter((lane) => lane.board_id === board.id);
      const columns = boardLanes.map((lane) => {
        const cards = boardRoots.filter((card) => card.list_id === lane.id).sort(byPosition);
        return `<section class="kanban-col" data-lane="${esc(lane.kind)}">
          <header><span>${esc(lane.name)}</span><b>${cards.length}</b></header>
          <div class="kanban-cards">${cardColumn(cards, (card) => tile(card, model))}</div>
        </section>`;
      });
      const loose = boardRoots.filter((card) => !boardLanes.some((lane) => lane.id === card.list_id));
      if (loose.length) {
        columns.push(`<section class="kanban-col" data-lane="unknown">
          <header><span>No lane</span><b>${loose.length}</b></header>
          <div class="kanban-cards">${cardColumn(loose, (card) => tile(card, model))}</div>
        </section>`);
      }
      // The project-level Plan and Rework lanes belong to the project, not to one board.
      if (index === 0) columns.push(planColumn(model), reworkColumn(model));
      const archived = model.archived.filter((card) => card.board_id === board.id);
      if (archived.length) {
        columns.push(`<section class="kanban-col kanban-archived" data-lane="archived">
          <header><span>Archived</span><b>${archived.length}</b></header>
          <div class="kanban-cards">${cardColumn(
            archived.sort(byPosition),
            (card) => tile(card, model),
          )}</div>
        </section>`);
      }
      const heading =
        model.boards.length > 1
          ? `<div class="kanban-board-head">${esc(board.name)}</div>`
          : "";
      return `${heading}<div class="kanban">${columns.join("")}</div>`;
    });
    if (!model.boards.length) {
      sections.push(`<div class="kanban">${planColumn(model)}${reworkColumn(model)}</div>`);
    }
    return `<div class="kanban-wrap">${note}${sections.join("")}</div>`;
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
  // execution is the answer, and the page says there are no child cards. A card carrying a
  // `rework_key` also resolves its register package, rendered before the execution panel.
  function packageModel(work, project, cardId, measure, rework) {
    const ref = projectRef(project);
    const projectId = ref.id;
    const boards = list((work || {}).boards).filter((board) => board.project_id === projectId);
    const boardIds = new Set(boards.map((board) => board.id));
    const cards = list((work || {}).cards).filter((card) => boardIds.has(card.board_id));
    const parent = cards.find((card) => card.id === cardId) || null;
    const children = cards.filter((card) => card.parent_card_id === cardId);
    const lanes = list((work || {}).lists)
      .filter((lane) => boardIds.has(lane.board_id))
      .sort(byPosition);
    const packages = list((rework || {}).packages);
    const pkg = parent
      ? packages.find(
          (row) =>
            (parent.rework_key &&
              (row.key === parent.rework_key || row.rule === parent.rework_key)) ||
            (parent.rule && row.rule === parent.rule),
        ) || null
      : null;
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
      unattributed: tenantSpend(measure),
      rework: pkg,
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
    // The register package is the reason the card exists; it is read before the execution record.
    const rework = reworkPackagePanel(model.rework);
    const execution = executionPanel(model, work);
    const note = model.unattributed
      ? `<p class="muted kanban-note">${esc(
          spendText(
            model.unattributed.tokens,
            model.unattributed.usd,
            model.unattributed.priced,
            model.unattributed.unpriced,
          ),
        )} unattributed to a project.</p>`
      : "";
    if (!model.children.length) {
      return `<div class="kanban-wrap">${head}${rework}${execution}${note}<div class="kanban-empty muted">No child cards. This work package's execution is above; a child card would appear in its lane here.</div></div>`;
    }
    const lanes = model.lanes.filter((lane) => lane.kind !== "archive");
    const live = model.children.filter((card) => !card.archived_at);
    const columns = lanes.map((lane) => {
      const cards = live.filter((card) => card.list_id === lane.id).sort(byPosition);
      return `<section class="kanban-col" data-lane="${esc(lane.kind)}">
        <header><span>${esc(lane.name)}</span><b>${cards.length}</b></header>
        <div class="kanban-cards">${cardColumn(cards, (card) => packageTile(card, model))}</div>
      </section>`;
    });
    // An archived child renders once, in the Archived column — never also in its old lane.
    const orphan = live.filter((card) => !lanes.some((lane) => lane.id === card.list_id));
    const archived = model.children.filter((card) => card.archived_at);
    if (orphan.length) {
      columns.push(`<section class="kanban-col" data-lane="unknown"><header><span>No lane</span><b>${orphan.length}</b></header><div class="kanban-cards">${cardColumn(
        orphan,
        (card) => packageTile(card, model),
      )}</div></section>`);
    }
    if (archived.length) {
      columns.push(`<section class="kanban-col kanban-archived" data-lane="archived"><header><span>Archived</span><b>${archived.length}</b></header><div class="kanban-cards">${cardColumn(
        archived,
        (card) => packageTile(card, model),
      )}</div></section>`);
    }
    return `<div class="kanban-wrap">${head}${rework}${execution}${note}<div class="kanban">${columns.join("")}</div></div>`;
  }

  function packageTile(card, model) {
    const spend = model.spend.get(card.id);
    const rework = card.rework_key
      ? `<span class="kanban-rework sev-${esc(
          String(card.severity || "").toLowerCase(),
        )}">${esc(String(card.severity || "—").toUpperCase())} · ${esc(
          card.rule || card.rework_key,
        )}</span>`
      : "";
    return `<article class="kanban-card${isDone(card, model.laneKinds) ? " done" : ""}">
      <a class="kanban-open" href="${esc(model.wpBase)}/${esc(card.id)}">${esc(card.name)}</a>
      <small class="kanban-meta">${esc((model.types.get(card.card_type_id) || {}).code || "—")}${
        model.runs.get(card.id) ? ` · ${esc(model.runs.get(card.id).status)}` : ""
      }</small>
      ${rework}
      <span class="kanban-cost">${
        spend ? esc(spendText(Number(spend.total_tokens), spend.cost_usd, spend.priced_calls, spend.unpriced_calls)) : "no spend"
      }</span>
    </article>`;
  }

  // L4's register panel: why the card exists, what it must prove, and the exact acceptance command.
  // Evidence is the finding's own location (`path:line`); absence is named, never fabricated.
  function reworkPackagePanel(pkg) {
    if (!pkg) return "";
    const dod = list(pkg.dod);
    const met = dod.filter((row) => row && row.met).length;
    const subjects = list(pkg.subjects)
      .map((subject) => {
        const facts = [
          subject.severity ? String(subject.severity).toUpperCase() : "",
          subject.disposition || "",
          subject.owner ? `owner ${subject.owner}` : "",
          subject.expires ? `expires ${day(parse(subject.expires))}` : "",
          subject.reason || "",
        ]
          .filter(Boolean)
          .join(" · ");
        const evidence = evidenceText(subject.evidence);
        return `<li class="rework-subject"><span class="mono">${esc(
          subject.subject || "—",
        )}</span>${subject.detail ? ` <span>${esc(subject.detail)}</span>` : ""}${
          facts ? `<small class="muted">${esc(facts)}</small>` : ""
        }${evidence ? `<code class="rework-evidence">${esc(evidence)}</code>` : ""}</li>`;
      })
      .join("");
    const dodHtml = dod.length
      ? `<ul class="exec-dod">${dod
          .map(
            (row) =>
              `<li class="${row && row.met ? "met" : "open"}">${
                row && row.met ? "met" : "open"
              } · ${esc(row ? row.item : "—")}</li>`,
          )
          .join("")}</ul>`
      : `<p class="muted">This package carries no Definition of Done item.</p>`;
    const facts = [
      chip("Standard", [pkg.standard, pkg.spec].filter(Boolean).join(" · ") || "—"),
      chip("Rule", pkg.rule || "—"),
      chip("Severity", String(pkg.severity || "—").toUpperCase()),
      pkg.artefact ? chip("Artefact", pkg.artefact) : "",
      pkg.scope ? chip("Scope", pkg.scope) : "",
      pkg.apply_to ? chip("Applies to", pkg.apply_to) : "",
      list(pkg.depends_on).length ? chip("Depends on", list(pkg.depends_on).join(", ")) : "",
      pkg.owner_project ? chip("Owner", pkg.owner_project) : "",
    ]
      .filter(Boolean)
      .join("");
    const evidence = evidenceText(pkg.evidence);
    const baseline = pkg.baseline
      ? `<div class="exec-dod-wrap"><span class="rail-label">Baseline</span><code class="rework-evidence">${esc(
          evidenceText(pkg.baseline),
        )}</code></div>`
      : "";
    return `<div class="exec rework-package">
      <div class="exec-head"><span class="exec-title">Rework package</span><span class="muted">${esc(
        pkg.title || pkg.key || "",
      )} · ${met}/${dod.length} DoD met</span></div>
      <div class="delivery-rail"><span class="rail-label">Finding</span><div class="rail-chips">${facts}</div></div>
      ${baseline}
      <div class="exec-dod-wrap"><span class="rail-label">Subjects${
        list(pkg.subjects).length ? ` · ${list(pkg.subjects).length}` : ""
      }</span>${
        subjects
          ? `<ul class="rework-subjects">${subjects}</ul>`
          : `<p class="muted">No subject is published on this package.</p>`
      }</div>
      <div class="exec-dod-wrap"><span class="rail-label">Definition of Done</span>${dodHtml}</div>
      <div class="exec-dod-wrap"><span class="rail-label">Acceptance command</span><code class="rework-command">${esc(
        pkg.acceptance_command || "—",
      )}</code></div>
      <div class="exec-dod-wrap"><span class="rail-label">Evidence</span>${
        evidence
          ? `<code class="rework-evidence">${esc(evidence)}</code>`
          : `<p class="muted">No evidence reference is published.</p>`
      }</div>
    </div>`;
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

  // The generated standards record. Every MSP/TOGAF/SAFe/PMP artefact is a row with a status and
  // a published page; an absent or partial artefact names the package that must close. The rail
  // never invents an artefact: no record means "not published", not an empty set of green chips.
  // L2's roll-up: every project's own artefact record, worst first. The programme rail lists the
  // programme's artefacts; this answers "which projects carry the missing work" without opening
  // each project. The count includes the project's phase, architecture-artefact and card pages.
  function projectArtefactRollup(standards, projects) {
    const artefacts = list(standards && standards.artefacts);
    if (!artefacts.length) return "";
    const rows = list(projects)
      .map((project) => {
        const mine = artefacts.filter(
          (artefact) => (artefact.subject || {}).project_key === project.key,
        );
        return {
          project,
          total: mine.length,
          evidenced: mine.filter((artefact) => artefact.status === "evidenced").length,
          absent: mine.filter((artefact) => artefact.status === "absent").length,
        };
      })
      .filter((row) => row.total > 0)
      .sort(
        (left, right) =>
          right.absent - left.absent || String(left.project.key).localeCompare(String(right.project.key)),
      );
    if (!rows.length) return "";
    const shown = rows.slice(0, 12).map(
      (row) =>
        `<a class="rail-chip std-chip" href="#projects/${esc(row.project.id)}"><b>${esc(
          row.project.key,
        )}</b> ${row.evidenced}/${row.total} evidenced · ${row.absent} absent</a>`,
    );
    if (rows.length > 12) shown.push(`<span class="rail-chip">+${rows.length - 12} projects</span>`);
    return `<div class="delivery-rail std-rail"><span class="rail-label">Project artefact record</span><div class="rail-chips">${shown.join(
      "",
    )}</div></div>`;
  }

  function standardsSummary(standards) {
    const artefacts = list(standards && standards.artefacts);
    if (!artefacts.length) {
      return `<div class="delivery-rail std-rail"><span class="rail-label">Standards artefacts</span><div class="rail-chips"><span class="rail-chip">the standards record is not published</span></div></div>`;
    }
    const by = (standards && standards.by_standard) || {};
    const chips = Object.keys(by)
      .sort()
      .map((standard) => {
        const row = by[standard];
        return `<span class="rail-chip std-chip"><b>${esc(standard)}</b> ${row.evidenced}/${row.artefacts} evidenced · ${row.partial} partial · ${row.absent} absent</span>`;
      })
      .join("");
    return `<div class="delivery-rail std-rail"><span class="rail-label">Standards artefacts · <a href="docs/standards/index.html">index</a></span><div class="rail-chips">${chips}</div></div>`;
  }

  function artefactRail(standards, options = {}) {
    const artefacts = list(standards && standards.artefacts);
    if (!artefacts.length) {
      return `<div class="delivery-rail std-rail"><span class="rail-label">Standards artefacts</span><div class="rail-chips"><span class="rail-chip">the standards record is not published</span></div></div>`;
    }
    const rows = artefacts
      .filter((artefact) => {
        const subject = artefact.subject || {};
        if (options.cardId && subject.id === options.cardId) return true;
        if (options.projectKey && subject.project_key === options.projectKey) return true;
        if (
          options.programmeId &&
          subject.programme_id === options.programmeId &&
          ["programme", "capability", "tranche", "benefit"].includes(artefact.subject_kind)
        ) {
          return true;
        }
        return false;
      })
      .sort((left, right) => {
        const rank = { absent: 0, partial: 1, evidenced: 2 };
        return (
          (rank[left.status] ?? 3) - (rank[right.status] ?? 3) ||
          String(left.standard).localeCompare(String(right.standard)) ||
          String(left.category).localeCompare(String(right.category))
        );
      });
    if (!rows.length) {
      return `<div class="delivery-rail std-rail"><span class="rail-label">Standards artefacts</span><div class="rail-chips"><span class="rail-chip">no standards artefact applies to this subject</span></div></div>`;
    }
    const groups = new Map();
    rows.forEach((artefact) => {
      const bucket = groups.get(artefact.standard) || [];
      bucket.push(artefact);
      groups.set(artefact.standard, bucket);
    });
    const body = [...groups.keys()]
      .sort()
      .map((standard) => {
        const items = groups
          .get(standard)
          .map((artefact) => {
            const missing = list(artefact.sections)
              .filter((section) => section && section.status !== "evidenced")
              .map((section) => section.heading)
              .filter(Boolean);
            const missingText = missing.length
              ? `<small class="std-missing">missing: ${esc(
                  missing.slice(0, 6).join(", "),
                )}${missing.length > 6 ? ` +${missing.length - 6}` : ""}</small>`
              : "";
            return `<li><a class="std-link ${esc(artefact.status)}" href="docs/standards/${esc(
              artefact.id,
            )}.html">${esc(artefact.category)} · ${esc((artefact.subject || {}).title || "—")}</a><span class="std-state">${esc(
              artefact.status,
            )}</span>${
              artefact.status === "evidenced"
                ? ""
                : `<code class="std-package">${esc(artefact.package_key || "no package")}</code>`
            }${missingText}</li>`;
          })
          .join("");
        return `<div class="std-group"><span class="std-standard">${esc(standard)}</span><ul class="std-links">${items}</ul></div>`;
      })
      .join("");
    const counts = rows.reduce(
      (acc, artefact) => {
        acc[artefact.status] = (acc[artefact.status] || 0) + 1;
        return acc;
      },
      {},
    );
    return `<div class="delivery-rail std-rail"><span class="rail-label">Standards artefacts · ${
      rows.length
    } · ${counts.evidenced || 0} evidenced · ${counts.partial || 0} partial · ${
      counts.absent || 0
    } absent</span><div class="std-groups">${body}</div></div>`;
  }

  /* ------------------------------------------------------------ delivery unit
     SPEC_V2 §10.5.9 (#782 P2): one unit's real evidence, read from the snapshot's `delivery`
     section — the charter, the immutable steps, the three development gates and the closure.
     Every value is the row's own; a unit with no issue reference or no closure says
     "not recorded" rather than rendering a guess. The route id is user input, matched against
     the rows, never used as a selector. */
  function unitRepo(unit) {
    return typeof unit.repo === "string" && /^[^/\s]+\/[^/\s]+$/.test(unit.repo)
      ? unit.repo
      : null;
  }

  function unitIssueUrl(unit) {
    const ref = typeof unit.issue_ref === "string" ? unit.issue_ref : "";
    const hash = ref.lastIndexOf("#");
    const number = hash >= 0 ? ref.slice(hash + 1) : "";
    const repo = unitRepo(unit);
    return repo && /^\d+$/.test(number) ? `https://github.com/${repo}/issues/${number}` : null;
  }

  function unitClosure(closure, repo) {
    if (!closure || typeof closure !== "object") {
      return `<div class="empty">Closure: not recorded.</div>`;
    }
    const cell = (value) => (value == null || value === "" ? "—" : esc(value));
    const pr = closure.pr_url
      ? `<a href="${esc(closure.pr_url)}">${esc(closure.pr_url)}</a>`
      : "—";
    const ciHref =
      repo && closure.ci_run_id ? `https://github.com/${repo}/actions/runs/${closure.ci_run_id}` : null;
    const ci = ciHref
      ? `<a class="mono" href="${esc(ciHref)}">${esc(closure.ci_run_id)}</a>`
      : cell(closure.ci_run_id);
    return `<table class="matrix"><thead><tr><th>PR</th><th>CI</th><th>Merge</th><th>Digest</th><th>Live check</th><th>Reviewer</th></tr></thead><tbody><tr>
      <td>${pr}</td>
      <td>${ci} ${cell(closure.ci_verdict)}</td>
      <td class="mono">${cell(closure.merge_sha)}</td>
      <td class="mono">${cell(closure.deployed_digest)}</td>
      <td>${closure.live_check ? esc(JSON.stringify(closure.live_check)) : "—"}</td>
      <td>${cell(closure.reviewer)}</td></tr></tbody></table>`;
  }

  function unitDetail(data, issue) {
    const units = list(data && data.delivery && data.delivery.units);
    const wanted = String(issue == null ? "" : issue).trim();
    const unit = units.find((row) => {
      const ref = typeof row.issue_ref === "string" ? row.issue_ref : "";
      return ref !== "" && (ref === wanted || ref.endsWith("#" + wanted));
    });
    if (!unit) {
      return `<div class="page"><h2>Delivery unit</h2><div class="empty">Delivery unit: not recorded for <span class="mono">${esc(wanted || "—")}</span>. The snapshot carries ${units.length} unit${units.length === 1 ? "" : "s"}.</div></div>`;
    }
    const steps = list(unit.steps)
      .map((step) => {
        const evidence = step.evidence && typeof step.evidence === "object" ? step.evidence : {};
        const links = [];
        if (evidence.pr_url) links.push(`<a href="${esc(evidence.pr_url)}">PR</a>`);
        if (evidence.ci_run_id)
          links.push(`<span class="mono">CI ${esc(evidence.ci_run_id)}</span>`);
        if (evidence.merge_sha) links.push(`<span class="mono">${esc(evidence.merge_sha)}</span>`);
        return `<tr><td class="mono">${esc(step.step)}</td><td>${day(parse(step.recorded_at))}</td><td>${links.join(" ") || "—"}</td></tr>`;
      })
      .join("");
    const gates = list(unit.gates)
      .map(
        (gate) =>
          `<tr><td class="mono">${esc(gate.gate)}</td><td>${esc(gate.status)}</td><td>${day(parse(gate.decided_at))}</td></tr>`,
      )
      .join("");
    const url = unitIssueUrl(unit);
    const ref = typeof unit.issue_ref === "string" ? unit.issue_ref : "not recorded";
    const title = url ? `<a href="${esc(url)}" class="mono">${esc(ref)}</a>` : esc(ref);
    return `<div class="page"><h2>Delivery unit ${title}</h2>
      <div class="row"><span class="mono">${esc(unit.repo)}</span><span class="mono">${esc(unit.lane)}</span><span>${esc(unit.status)}</span></div>
      <h3>Steps</h3>${steps ? `<table class="matrix"><thead><tr><th>Step</th><th>Recorded</th><th>Evidence</th></tr></thead><tbody>${steps}</tbody></table>` : `<div class="empty">No steps recorded.</div>`}
      <h3>Gates</h3>${gates ? `<table class="matrix"><thead><tr><th>Gate</th><th>Status</th><th>Decided</th></tr></thead><tbody>${gates}</tbody></table>` : `<div class="empty">No gates recorded.</div>`}
      <h3>Closure</h3>${unitClosure(unit.closure, unitRepo(unit))}</div>`;
  }

  return {
    activityIndex,
    portfolioRows,
    programmeRows,
    trancheRows,
    programmeArtefacts,
    trancheRail,
    benefitRail,
    dependencyRail,
    reconciliationRail,
    phaseStrip,
    cadenceRail,
    reworkRail,
    registerRail,
    scorecard,
    standardsSummary,
    artefactRail,
    projectArtefactRollup,
    governanceLine,
    projectGovernance,
    executionPanel,
    reworkPackagePanel,
    summary,
    gantt,
    ganttTable,
    boardModel,
    kanban,
    packageModel,
    packageBoard,
    unitDetail,
    tenantSpend,
    spendText,
    tokens,
    usd,
  };
})();
