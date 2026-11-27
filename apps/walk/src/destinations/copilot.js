// The Copilot destination (S2, Govern & design). The copilot surface is route-transported, not an
// action family: the client reads `GET /api/copilot/actions` and, for a conversation, its turns.
// The screen renders whatever the route answers — a 401/403 is shown verbatim as the problem
// panel, and a missing route names the slice that will serve it (S10) rather than pretending.

import { el } from "../dom.js";

/** @param {any} value @returns {any[]} */
function rows(value) {
  if (Array.isArray(value?.actions)) return value.actions;
  if (Array.isArray(value?.items)) return value.items;
  return Array.isArray(value) ? value : [];
}

/** @param {any} row @param {string} key */
function text(row, key) {
  const value = row?.[key];
  return typeof value === "string" && value !== "" ? value : "—";
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const body = el("div", { class: "destination-body" });
  const status = el("p", { class: "loading", role: "status", text: "Loading the copilot surface…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Govern & design" }),
    el("h1", { text: "Copilot" }),
    el("p", {
      class: "lede",
      text: "The copilot's action surface and its conversations. This screen is route-transported: it reads the copilot routes directly, and names the slice that serves them when they are absent.",
    }),
    status,
    body,
  );
  try {
    const result = await ctx.api.request("/api/copilot/actions");
    const actions = rows(result);
    status.textContent = actions.length === 0 ? "" : `${actions.length} action${actions.length === 1 ? "" : "s"}`;
    if (actions.length === 0) {
      body.append(el("div", { class: "empty" }, el("p", { text: "The copilot route answers with no action yet; this is the empty state, not an error." })));
    } else {
      const node = el("table", { class: "table", "data-testid": "copilot-actions" });
      node.append(el("thead", {}, el("tr", {}, el("th", { text: "Action" }), el("th", { text: "Summary" }))));
      const tbody = el("tbody");
      for (const action of actions) {
        tbody.append(el("tr", {}, el("td", { text: text(action, "name") }), el("td", { text: text(action, "summary") })));
      }
      node.append(tbody);
      body.append(node);
    }
  } catch (error) {
    status.textContent = "";
    body.append(ctx.shell.problemPanel(error));
    body.append(
      el("p", {
        class: "field-hint",
        text: "If the route is absent, the copilot surface is served by the S10 slice; the screen names it instead of inventing a conversation.",
      }),
    );
  }
  return { element: section, title: "Copilot" };
}
