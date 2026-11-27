// The Benchmarks destination (S3): `benchmark_suites.*` and `benchmark_runs.*` as the app reads
// them. A suite is the fixed task set; a run is one arm measured against it. Compare and trend are
// read actions with inputs, so they open the same sheet every action does; the server refuses a
// comparison it cannot make (CONTRACT §19) and that refusal is shown verbatim.

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
function shortId(value) {
  return typeof value === "string" && value !== "" ? value.slice(0, 8) : "—";
}

/** @param {any} run */
function runScore(run) {
  const resolved = run?.n_resolved;
  const graded = run?.n_graded;
  return typeof resolved === "number" && typeof graded === "number"
    ? `${resolved} / ${graded}`
    : "—";
}

/** @param {any} suite */
function suiteStatus(suite) {
  return typeof suite?.baseline_run_id === "string" ? "baseline promoted" : "no baseline";
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

/** @returns {{element: HTMLElement, title: string}} */
function planned() {
  return {
    element: el(
      "section",
      { class: "destination" },
      el("h1", { text: "Benchmarks is planned" }),
      el("p", {
        class: "field-hint",
        text: "benchmark_suites.list is not in this role's manifest; the screen appears when the benchmark family is granted.",
      }),
    ),
    title: "Benchmarks is planned",
  };
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("benchmark_suites.list")) return planned();
  const runId = ctx.params?.run_id;
  return runId ? detail(ctx, runId) : index(ctx);
}

/** @param {any} ctx */
async function index(ctx) {
  const section = el("section", { class: "destination" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading benchmarks…" });
  const body = el("div", { class: "destination-body" });
  section.append(
    el("p", { class: "eyebrow", text: "Measure & improve" }),
    el("h1", { text: "Benchmarks" }),
    el("p", {
      class: "lede",
      text: "A suite fixes the task set; a run measures one arm against it. Gold and empty control the suite, conveyor measures a capability agent, and a comparison is exact or refused.",
    }),
    actions,
    status,
    body,
  );
  /** @type {Map<string, string>} */
  const suiteNames = new Map();

  async function load() {
    status.textContent = "Loading benchmarks…";
    clear(actions);
    clear(body);
    if (ctx.hasAction("benchmark_runs.start")) {
      actions.append(
        el("button", {
          class: "button primary",
          type: "button",
          text: "Start a run",
          onClick: () =>
            ctx.shell.openAction("benchmark_runs.start", { title: "Start a run", onDone: load }),
        }),
      );
    }
    try {
      const result = await ctx.api.invoke("benchmark_suites.list", { limit: 100 });
      const suites = rows(result.output);
      status.textContent = suites.length === 0 ? "" : `${suites.length} suite${suites.length === 1 ? "" : "s"}`;
      for (const suite of suites) suiteNames.set(String(suite.id), text(suite, "slug"));
      if (suites.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No benchmark suite in this tenant. The embedded verified50 artifact seeds with `migrate --dev-seed`." }),
          ),
        );
      } else {
        body.append(
          table(
            "Suites",
            ["Suite", "Version", "Status"],
            suites.map((suite) => [
              text(suite, "slug"),
              text(suite, "swebench_version"),
              suiteStatus(suite),
            ]),
            "benchmarks-suites",
          ),
        );
      }
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
    if (!ctx.hasAction("benchmark_runs.list")) {
      body.append(note("benchmark_runs.list"));
      return;
    }
    try {
      const result = await ctx.api.invoke("benchmark_runs.list", { limit: 50 });
      const runs = rows(result.output);
      if (runs.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No run for this suite yet. Start a gold control to prove it." }),
          ),
        );
        return;
      }
      body.append(
        table(
          "Runs",
          ["Run", "Suite", "Arm", "Score", "Status"],
          runs.map((run) => [
            el("a", {
              href: `/evals/benchmarks/run/${run.id}`,
              text: shortId(run.id),
            }),
            suiteNames.get(String(run.suite_id)) ?? shortId(run.suite_id),
            text(run, "arm"),
            runScore(run),
            text(run, "status"),
          ]),
          "benchmarks-runs",
        ),
      );
    } catch (error) {
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Benchmarks" };
}

/**
 * @param {any} ctx
 * @param {string} runId
 */
async function detail(ctx, runId) {
  const section = el("section", { class: "destination" });
  section.append(
    el("p", { class: "eyebrow", text: "Measure & improve" },
      el("a", { href: "/evals/benchmarks", text: "← All benchmarks" })),
    el("p", { class: "loading", role: "status", text: "Loading the run…" }),
  );
  let title = "Benchmarks";

  async function load() {
    if (!ctx.hasAction("benchmark_runs.get")) {
      clear(section);
      section.append(
        el("p", { class: "eyebrow", text: "Measure & improve" },
          el("a", { href: "/evals/benchmarks", text: "← All benchmarks" })),
        el("h1", { text: "Benchmarks" }),
        note("benchmark_runs.get"),
      );
      return;
    }
    try {
      const result = await ctx.api.invoke("benchmark_runs.get", { id: runId });
      const run = result.output;
      title = `Benchmarks · ${shortId(run?.id)}`;
      clear(section);
      section.append(
        el("p", { class: "eyebrow", text: "Measure & improve" },
          el("a", { href: "/evals/benchmarks", text: "← All benchmarks" })),
        el("h1", { text: `Run ${shortId(run?.id)}` }),
        el("dl", { class: "facts", "data-testid": "benchmark-run" },
          el("dt", { text: "Suite" }), el("dd", { text: shortId(run?.suite_id) }),
          el("dt", { text: "Arm" }), el("dd", { text: text(run, "arm") }),
          el("dt", { text: "Label" }), el("dd", { text: text(run, "label") }),
          el("dt", { text: "Subject" }), el("dd", { text: text(run, "subject_kind") }),
          el("dt", { text: "Status" }), el("dd", { text: text(run, "status") }),
          el("dt", { text: "Model" }), el("dd", { text: text(run, "model") }),
          el("dt", { text: "Graded" }), el("dd", { text: num(run, "n_graded") }),
          el("dt", { text: "Resolved" }), el("dd", { text: num(run, "n_resolved") }),
          el("dt", { text: "No attempt" }), el("dd", { text: num(run, "n_no_attempt") }),
          el("dt", { text: "Created" }), el("dd", { text: text(run, "created_at") }),
        ),
      );
      const actions = el("div", { class: "destination-actions" });
      if (ctx.hasAction("benchmark_runs.compare")) {
        actions.append(
          el("button", {
            class: "button",
            type: "button",
            text: "Compare",
            onClick: () =>
              ctx.shell.openAction("benchmark_runs.compare", {
                title: "Compare two runs",
                initial: { a_run_id: run.id },
                onDone: load,
              }),
          }),
        );
      }
      if (ctx.hasAction("benchmark_runs.trend")) {
        actions.append(
          el("button", {
            class: "button",
            type: "button",
            text: "Trend",
            onClick: () =>
              ctx.shell.openAction("benchmark_runs.trend", {
                title: "The suite's trend",
                initial: { suite_id: run.suite_id },
                onDone: load,
              }),
          }),
        );
      }
      section.append(actions);
    } catch (error) {
      clear(section);
      section.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title };
}
