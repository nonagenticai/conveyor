// The Metering destination (S3): the conveyor's ledger figure, the tenant's metering row and
// the inference usage report over one 30-day window. Every figure is read-time data from its
// own action; a read that fails says so instead of rendering a zero. No writes.

import { el } from "../dom.js";

/** @param {any} row @param {...string} keys */
function text(row, ...keys) {
  for (const key of keys) {
    const value = row?.[key];
    if (typeof value === "string" && value !== "") return value;
    if (typeof value === "number") return String(value);
  }
  return "—";
}

/** @param {any} row @param {string} key */
function num(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** A USD figure; the inference report sends decimals as strings, the ledgers as numbers.
 * @param {any} row @param {string} key */
function money(row, key) {
  const value = row?.[key];
  if (typeof value === "number" && Number.isFinite(value)) return `$${value.toFixed(4)}`;
  if (typeof value === "string" && value !== "") return `$${value}`;
  return "—";
}

/** A read's row list: `rows` (the ledgers), `items` (a page) or a bare array.
 * @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.rows)) return value.rows;
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} value */
function valueLine(value) {
  if (typeof value === "string") return value;
  if (value === undefined || value === null) return "—";
  try {
    const encoded = JSON.stringify(value);
    return encoded === undefined ? "—" : encoded;
  } catch {
    return "—";
  }
}

/** A summary's breakdown rows, shown verbatim: a shape the summary carries is never dropped.
 * @param {any} summary @param {number} colspan @returns {Array<Array<Node|string>>} */
function breakdown(summary, colspan) {
  return rows(summary).map((item) => [
    el("td", {
      colspan,
      text: Object.entries(item).map(([key, value]) => `${key}: ${valueLine(value)}`).join(" · "),
    }),
  ]);
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

/** The one shape an absent read takes: named, never rendered as an empty table.
 * @param {string} title @param {string} action */
function skipped(title, action) {
  return el(
    "section",
    {},
    el("h2", { text: title }),
    el("p", { class: "field-hint", text: `${action} is not in this role's manifest; this section is skipped.` }),
  );
}

/** @param {string} title @param {HTMLElement} panel */
function failed(title, panel) {
  return el("section", {}, el("h2", { text: title }), panel);
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("billing.summary")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Metering is planned" }),
        el("p", {
          class: "field-hint",
          text: "billing.summary is not in this role's manifest; the screen appears when the metering family is granted.",
        }),
      ),
      title: "Metering is planned",
    };
  }

  const until = new Date();
  const since = new Date(until.getTime() - 30 * 24 * 60 * 60 * 1000);
  const period = { since: since.toISOString(), until: until.toISOString() };

  const billingHost = el("div");
  const usageHost = el("div");
  const inferenceHost = el("div");

  async function loadBilling() {
    const title = "Billing summary";
    if (!ctx.hasAction("billing.summary")) {
      billingHost.append(skipped(title, "billing.summary"));
      return;
    }
    try {
      const result = await ctx.api.invoke("billing.summary", { window_days: 30 });
      const summary = result.output ?? {};
      billingHost.append(
        table(
          title,
          ["From", "To", "Total USD", "Inference calls", "Tool calls"],
          [
            [
              text(summary, "from"),
              text(summary, "to"),
              money(summary, "total_usd"),
              num(summary, "inference_calls"),
              num(summary, "tool_calls"),
            ],
            ...breakdown(summary, 5),
          ],
          "metering-billing",
        ),
      );
    } catch (error) {
      billingHost.append(failed(title, ctx.shell.problemPanel(error)));
    }
  }

  async function loadUsage() {
    const title = "Usage by tenant";
    if (!ctx.hasAction("metrics.usage")) {
      usageHost.append(skipped(title, "metrics.usage"));
      return;
    }
    try {
      const result = await ctx.api.invoke("metrics.usage", { level: "tenant" });
      const list = rows(result.output);
      usageHost.append(
        table(
          title,
          ["Key", "Calls", "Tool calls", "Tokens", "Cost USD", "Runs", "Participant hours"],
          list.map((row) => [
            text(row, "key"),
            num(row, "calls"),
            num(row, "tool_calls"),
            num(row, "tokens"),
            money(row, "cost_usd"),
            num(row, "runs"),
            num(row, "participant_hours"),
          ]),
          "metering-usage",
        ),
      );
    } catch (error) {
      usageHost.append(failed(title, ctx.shell.problemPanel(error)));
    }
  }

  async function loadInference() {
    const title = "Inference by model";
    if (!ctx.hasAction("inference_usage.report")) {
      inferenceHost.append(skipped(title, "inference_usage.report"));
      return;
    }
    try {
      const result = await ctx.api.invoke("inference_usage.report", {
        since: period.since,
        until: period.until,
        group_by: "model",
      });
      const list = rows(result.output);
      inferenceHost.append(
        table(
          title,
          ["Model", "Calls", "Prompt tokens", "Completion tokens", "Cost USD", "Priced calls", "Unpriced calls", "Hit ratio"],
          list.map((row) => [
            text(row, "bucket", "model"),
            num(row, "calls"),
            num(row, "prompt_tokens"),
            num(row, "completion_tokens"),
            money(row, "cost_usd"),
            num(row, "priced_calls"),
            num(row, "unpriced_calls"),
            num(row, "hit_ratio"),
          ]),
          "metering-inference",
        ),
      );
    } catch (error) {
      inferenceHost.append(failed(title, ctx.shell.problemPanel(error)));
    }
  }

  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Measure & improve" }),
    el("h1", { text: "Metering" }),
    el("p", {
      class: "lede",
      text: "The conveyor's own ledger figure, the tenant's metering row and the inference usage report over the last 30 days. Each source reports its own availability; a failed read is never a zero.",
    }),
    el("p", { class: "field-hint", text: `Window ${period.since} → ${period.until}` }),
    billingHost,
    usageHost,
    inferenceHost,
  );

  await Promise.all([loadBilling(), loadUsage(), loadInference()]);
  return { element: section, title: "Metering" };
}
