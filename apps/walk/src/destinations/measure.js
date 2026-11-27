// The Measure destination (#888): the landing reads the same instruments as the app's
// MeasureScreen (evals.list, benchmark_suites.list, prompt_audits.list, inference_calls.list and
// metrics.usage at tenant level) and links to the screens that own each. No number is computed
// on the client that the server did not return.

import { el, clear } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.items)) return value.items;
  if (Array.isArray(value?.rows)) return value.rows;
  return Array.isArray(value) ? value : [];
}

/**
 * @param {string} title
 * @param {string[]} labels
 * @param {Array<Array<Node|string>>} bodyRows
 * @param {string} testId
 */
function table(title, labels, bodyRows, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(
      el(
        "tr",
        {},
        ...cells.map((cell) =>
          cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) }),
        ),
      ),
    );
  }
  node.append(tbody);
  block.append(node);
  return block;
}

/** The instruments the app's MeasureScreen reads, and the screen that owns each. */
const INSTRUMENTS = [
  { name: "evals.list", label: "Evals", open: "/evals", input: { limit: 5 } },
  { name: "benchmark_suites.list", label: "Benchmarks", open: "/evals/benchmarks", input: { limit: 5 } },
  { name: "prompt_audits.list", label: "Prompt audit", open: "/measure/prompt-audit", input: { limit: 5 } },
  { name: "inference_calls.list", label: "Inference usage", open: "/measure/usage", input: { limit: 5 } },
  { name: "metrics.usage", label: "Metering", open: "/metering", input: { level: "tenant" } },
];

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const available = INSTRUMENTS.filter((instrument) => ctx.hasAction(instrument.name));
  if (available.length === 0) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("p", { class: "eyebrow", text: "Measure & improve" }),
        el("h1", { text: "Measure is planned" }),
        el("p", {
          class: "field-hint",
          text: "None of evals.list, benchmark_suites.list, prompt_audits.list, inference_calls.list or metrics.usage is in this role's manifest; the panels appear as the instruments are granted.",
        }),
      ),
      title: "Measure is planned",
    };
  }

  const body = el("div", { class: "destination-body" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Measure & improve" }),
    el("h1", { text: "Measure" }),
    el("p", {
      class: "lede",
      text: "The retained instruments, each read from the server's own action and linked to the screen that renders it in full.",
    }),
    body,
  );

  const summary = [];
  const panels = el("div");
  body.append(panels);

  for (const instrument of available) {
    const status = el("p", { class: "loading", role: "status", text: `Reading ${instrument.label.toLowerCase()}…` });
    const host = el("div");
    const panel = el(
      "section",
      { class: "panel", "data-testid": `measure-${instrument.name.replace(/\./g, "-")}` },
      el("h2", { text: instrument.label }),
      status,
      host,
    );
    panels.append(panel);
    try {
      const result = await ctx.api.invoke(instrument.name, instrument.input);
      const items = rows(result.output);
      status.textContent = "";
      host.append(
        el("p", { class: "muted", text: `${items.length} row(s) read` }),
        el("p", { class: "muted" }, el("a", { href: instrument.open, text: `Open ${instrument.label.toLowerCase()}` })),
      );
      summary.push({ label: instrument.label, count: items.length, open: instrument.open });
    } catch (error) {
      status.textContent = "";
      clear(host);
      host.append(ctx.shell.problemPanel(error));
    }
  }

  if (summary.length > 0) {
    body.prepend(
      table(
        "Instruments",
        ["Instrument", "Rows read", "Open"],
        summary.map((row) => [row.label, String(row.count), el("a", { href: row.open, text: row.open })]),
        "measure-instruments",
      ),
    );
  }

  return { element: section, title: "Measure" };
}
