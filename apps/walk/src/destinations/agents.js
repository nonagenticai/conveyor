// The Agents destination (S1): `capability_agents.list` as `apps/web` reads it — the executor
// specs a binding can run. Read-only; no write action exists on the screen.

import { el } from "../dom.js";

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
function count(row, key) {
  return Array.isArray(row?.[key]) ? String(row[key].length) : "0";
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const body = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Organisation" }),
    el("h1", { text: "Agents" }),
    el("p", {
      class: "lede",
      text: "The capability agents a binding can execute, and the actions each may call.",
    }),
    body,
  );

  const result = ctx.hasAction("capability_agents.list")
    ? await ctx.api.invoke("capability_agents.list", { page: { limit: 50 } })
    : { output: [] };
  const items = rows(result.output);
  if (items.length === 0) {
    body.append(
      el("div", { class: "empty" }, el("p", { text: "No capability agents in this tenant yet." })),
    );
    return { element: section, title: "Agents" };
  }
  const table = el("table", { class: "table", "data-testid": "agents-table" });
  table.append(
    el(
      "thead",
      {},
      el(
        "tr",
        {},
        el("th", { text: "Key" }),
        el("th", { text: "Status" }),
        el("th", { text: "Model" }),
        el("th", { text: "Prompt" }),
        el("th", { text: "Allowed actions" }),
      ),
    ),
  );
  const tbody = el("tbody");
  for (const row of items) {
    tbody.append(
      el(
        "tr",
        {},
        el("td", { text: text(row, "key") }),
        el("td", { text: text(row, "status") }),
        el("td", { text: text(row, "model_ref") }),
        el("td", { text: text(row, "prompt_ref") }),
        el("td", { text: count(row, "allowed_actions") }),
      ),
    );
  }
  table.append(tbody);
  body.append(table);
  return { element: section, title: "Agents" };
}
