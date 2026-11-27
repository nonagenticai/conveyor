// The Evals destination (S3): `evals.*`, `eval_cases.*`, `eval_runs.*` and `eval_audits.*` as the
// app reads them. The eval is one flow; its cases, runs and audits are the three tabs the server
// already pages. Every write goes through the action sheet and every write refetches (SPEC §5).

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {string} key */
function text(row, key) {
  const value = row?.[key];
  return typeof value === "string" && value !== "" ? value : "—";
}

/** @param {any} row @param {string} key */
function num(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** @param {unknown} value */
function score(value) {
  return typeof value === "number" ? String(value) : "—";
}

/** @param {unknown} value */
function list(value) {
  return Array.isArray(value) && value.length > 0 ? value.map(String).join(", ") : "—";
}

/** @param {unknown} value */
function shortId(value) {
  return typeof value === "string" && value !== "" ? value.slice(0, 8) : "—";
}

/** @param {unknown} value @param {number} max */
function excerpt(value, max) {
  if (typeof value !== "string" || value === "") return "—";
  return value.length > max ? `${value.slice(0, max)}…` : value;
}

/** @param {string} title @param {string[]} labels @param {Array<Array<Node|string>>} bodyRows @param {string} testId */
function table(title, labels, bodyRows, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
    );
  }
  node.append(tbody);
  block.append(node);
  return block;
}

/** @param {string} action */
function note(action) {
  return el("p", {
    class: "field-hint",
    text: `${action} is not in this role's manifest; this section appears when the action is granted.`,
  });
}

/** @param {any} ctx @returns {{element: HTMLElement, title: string}} */
function planned(ctx) {
  return {
    element: el(
      "section",
      { class: "destination" },
      el("h1", { text: "Evals is planned" }),
      el("p", {
        class: "field-hint",
        text: "evals.list is not in this role's manifest; the screen appears when the eval family is granted.",
      }),
    ),
    title: "Evals is planned",
  };
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("evals.list")) return planned(ctx);
  const evalId = ctx.params?.eval_id;
  return evalId ? detail(ctx, evalId) : index(ctx);
}

/** @param {any} ctx */
async function index(ctx) {
  const section = el("section", { class: "destination" });
  const status = el("p", { class: "loading", role: "status", text: "Loading evals…" });
  const actions = el("div", { class: "destination-actions" });
  const body = el("div", { class: "destination-body" });
  section.append(
    el("p", { class: "eyebrow", text: "Measure & improve" }),
    el("h1", { text: "Evals" }),
    el("p", {
      class: "lede",
      text: "One flow per eval: its cases, its frozen versions, its runs and the health audit over each frozen instrument.",
    }),
    actions,
    status,
    body,
  );

  async function load() {
    status.textContent = "Loading evals…";
    clear(actions);
    clear(body);
    try {
      const result = await ctx.api.invoke("evals.list", { limit: 100 });
      const items = rows(result.output);
      if (ctx.hasAction("evals.create")) {
        actions.append(
          el("button", {
            class: "button primary",
            type: "button",
            text: "New eval",
            onClick: () =>
              ctx.shell.openAction("evals.create", { title: "New eval", onDone: load }),
          }),
        );
      }
      status.textContent =
        items.length === 0 ? "" : `${items.length} eval${items.length === 1 ? "" : "s"}`;
      if (items.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No eval in this tenant yet." }),
            ctx.hasAction("evals.create")
              ? el("button", {
                  class: "button",
                  type: "button",
                  text: "Create the first eval",
                  onClick: () =>
                    ctx.shell.openAction("evals.create", { title: "New eval", onDone: load }),
                })
              : el("p", { class: "field-hint", text: "Your role cannot create evals." }),
          ),
        );
        return;
      }
      body.append(
        table(
          "Evals",
          ["Name", "Subject", "Entry action", "Status"],
          items.map((item) => [
            el("a", { href: `/evals/${item.id}`, text: text(item, "name") }),
            text(item, "subject_kind"),
            text(item, "entry_action"),
            text(item, "status"),
          ]),
          "evals-list",
        ),
      );
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Evals" };
}

/**
 * @param {any} ctx
 * @param {string} id
 */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  section.append(
    el("p", { class: "eyebrow", text: "Measure & improve" },
      el("a", { href: "/evals", text: "← All evals" })),
    el("p", { class: "loading", role: "status", text: "Loading the eval…" }),
  );
  let title = "Evals";
  const requested = ctx.params?.tab;
  const activeTab = requested === "runs" || requested === "audits" ? requested : "cases";

  async function load() {
    if (!ctx.hasAction("evals.get")) {
      clear(section);
      section.append(
        el("p", { class: "eyebrow", text: "Measure & improve" },
          el("a", { href: "/evals", text: "← All evals" })),
        el("h1", { text: "Evals" }),
        note("evals.get"),
      );
      return;
    }
    try {
      const result = await ctx.api.invoke("evals.get", { id });
      const evalRow = result.output;
      title = typeof evalRow?.name === "string" && evalRow.name !== "" ? evalRow.name : "Evals";
      clear(section);
      section.append(
        el("p", { class: "eyebrow", text: "Measure & improve" },
          el("a", { href: "/evals", text: "← All evals" })),
        el("h1", { text: text(evalRow, "name") }),
        el("dl", { class: "facts" },
          el("dt", { text: "Purpose" }), el("dd", { text: text(evalRow, "purpose") }),
          el("dt", { text: "Subject" }), el("dd", { text: text(evalRow, "subject_kind") }),
          el("dt", { text: "Entry action" }), el("dd", { text: text(evalRow, "entry_action") }),
          el("dt", { text: "Status" }), el("dd", { text: text(evalRow, "status") }),
          el("dt", { text: "Version" }), el("dd", { text: shortId(evalRow?.current_version_id) }),
          el("dt", { text: "Updated" }), el("dd", { text: text(evalRow, "updated_at") }),
        ),
      );
      const tabs = el("nav", { class: "destination-actions", "aria-label": "Eval tabs" });
      for (const [name, label] of [["cases", "Cases"], ["runs", "Runs"], ["audits", "Audits"]]) {
        tabs.append(
          el("a", {
            href: `/evals/${evalRow.id}/${name}`,
            class: name === activeTab ? "button primary" : "button ghost",
            "aria-current": name === activeTab ? "page" : undefined,
            text: label,
          }),
        );
      }
      const host = el("div", { class: "destination-body" });
      section.append(tabs, host);
      if (activeTab === "runs") await runsTab(host, ctx, evalRow, load);
      else if (activeTab === "audits") await auditsTab(host, ctx, evalRow, load);
      else await casesTab(host, ctx, evalRow, load);
    } catch (error) {
      clear(section);
      section.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title };
}

/**
 * The three tab loaders share the read/empty/error shape; the action bar is appended first so a
 * write stays reachable while the table loads or a read is refused.
 * @param {HTMLElement} host
 * @param {any} ctx
 * @param {string} action
 * @param {Record<string, unknown>} input
 * @param {string} emptyText
 * @param {string} title
 * @param {string[]} labels
 * @param {(item: any) => Array<Node|string>} cells
 * @param {string} testId
 */
async function readTab(host, ctx, action, input, emptyText, title, labels, cells, testId) {
  const body = el("div", {});
  host.append(body);
  if (!ctx.hasAction(action)) {
    body.append(note(action));
    return;
  }
  body.append(el("p", { class: "loading", role: "status", text: "Loading…" }));
  try {
    const result = await ctx.api.invoke(action, input);
    const items = rows(result.output);
    clear(body);
    if (items.length === 0) {
      body.append(el("div", { class: "empty" }, el("p", { text: emptyText })));
      return;
    }
    body.append(table(title, labels, items.map(cells), testId));
  } catch (error) {
    clear(body);
    body.append(ctx.shell.problemPanel(error));
  }
}

/**
 * @param {HTMLElement} host
 * @param {any} ctx
 * @param {any} evalRow
 * @param {() => Promise<void>} reload
 */
async function casesTab(host, ctx, evalRow, reload) {
  clear(host);
  const versionId =
    typeof evalRow?.current_version_id === "string" ? evalRow.current_version_id : null;
  if (versionId !== null && ctx.hasAction("eval_cases.create")) {
    host.append(
      el("div", { class: "destination-actions" },
        el("button", {
          class: "button",
          type: "button",
          text: "New case",
          onClick: () =>
            ctx.shell.openAction("eval_cases.create", {
              title: "New case",
              initial: { version_id: versionId },
              onDone: reload,
            }),
        }),
      ),
    );
  }
  if (versionId === null) {
    host.append(
      el("div", { class: "empty" },
        el("p", { text: "This eval has no frozen version, so there are no cases to list." }),
      ),
    );
    return;
  }
  await readTab(
    host,
    ctx,
    "eval_cases.list",
    { version_id: versionId, limit: 100 },
    "This version has no cases.",
    "Cases",
    ["#", "Case", "Tags", "Split", "Source", "Gold"],
    (item) => [
      typeof item?.ordinal === "number" ? String(item.ordinal + 1) : "—",
      excerpt(item?.input, 90),
      list(item?.tags),
      text(item, "split"),
      text(item, "source"),
      text(item, "gold_provenance"),
    ],
    "eval-cases",
  );
}

/**
 * @param {HTMLElement} host
 * @param {any} ctx
 * @param {any} evalRow
 * @param {() => Promise<void>} reload
 */
async function runsTab(host, ctx, evalRow, reload) {
  clear(host);
  if (ctx.hasAction("eval_runs.start")) {
    /** @type {Record<string, unknown>} */
    const initial = { eval_id: evalRow.id };
    if (typeof evalRow?.current_version_id === "string") {
      initial.version_id = evalRow.current_version_id;
    }
    host.append(
      el("div", { class: "destination-actions" },
        el("button", {
          class: "button",
          type: "button",
          text: "Start a run",
          onClick: () =>
            ctx.shell.openAction("eval_runs.start", {
              title: "Start a run",
              initial,
              onDone: reload,
            }),
        }),
      ),
    );
  }
  await readTab(
    host,
    ctx,
    "eval_runs.list",
    { eval_id: evalRow.id, limit: 100 },
    "No run yet. A run needs both sign-offs and a locked split (see the Sign-off tab).",
    "Runs",
    ["Variant", "Mode", "Status", "Round", "Cases", "Reps", "Started"],
    (item) => [
      text(item, "variant"),
      text(item, "mode"),
      text(item, "status"),
      num(item, "round"),
      num(item, "case_count"),
      num(item, "reps"),
      text(item, "started_at"),
    ],
    "eval-runs",
  );
}

/**
 * @param {HTMLElement} host
 * @param {any} ctx
 * @param {any} evalRow
 * @param {() => Promise<void>} reload
 */
async function auditsTab(host, ctx, evalRow, reload) {
  clear(host);
  if (ctx.hasAction("eval_audits.start")) {
    /** @type {Record<string, unknown>} */
    const initial = { eval_id: evalRow.id };
    if (typeof evalRow?.current_version_id === "string") {
      initial.version_id = evalRow.current_version_id;
    }
    host.append(
      el("div", { class: "destination-actions" },
        el("button", {
          class: "button",
          type: "button",
          text: "Audit",
          onClick: () =>
            ctx.shell.openAction("eval_audits.start", {
              title: "Audit the frozen version",
              initial,
              onDone: reload,
            }),
        }),
      ),
    );
  }
  await readTab(
    host,
    ctx,
    "eval_audits.list",
    { eval_id: evalRow.id, limit: 100 },
    "No audit has run for this instrument yet. Run one; the job writes the findings.",
    "Audits",
    ["Audit", "Status", "Oracle", "Null", "When"],
    (item) => [
      shortId(item?.id),
      text(item, "status"),
      score(item?.oracle_score),
      score(item?.null_score),
      text(item, "created_at"),
    ],
    "eval-audits",
  );
}
