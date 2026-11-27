// The Project types destination (S1): `project_requests.list` and the draft → approved
// promotion path, as `apps/web` reads it. The approve sheet is the action's own schema; the
// server's refusal stays verbatim.

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
    el("h1", { text: "Project types" }),
    el("p", {
      class: "lede",
      text: "The tenant's project requests, from draft to approved and promoted.",
    }),
    body,
  );

  async function load() {
    const result = ctx.hasAction("project_requests.list")
      ? await ctx.api.invoke("project_requests.list", { page: { limit: 50 } })
      : { output: [] };
    const items = rows(result.output);
    body.replaceChildren();
    if (items.length === 0) {
      body.append(
        el("div", { class: "empty" }, el("p", { text: "No project request in this tenant yet." })),
      );
      return;
    }
    const table = el("table", { class: "table", "data-testid": "project-types-table" });
    table.append(
      el(
        "thead",
        {},
        el(
          "tr",
          {},
          el("th", { text: "External ref" }),
          el("th", { text: "Origin" }),
          el("th", { text: "Status" }),
          el("th", { text: "Repository" }),
          el("th", { text: "Project" }),
          el("th", {}),
        ),
      ),
    );
    const tbody = el("tbody");
    for (const row of items) {
      const cells = [
        el("td", { text: text(row, "external_ref") }),
        el("td", { text: text(row, "external_origin") }),
        el("td", { text: text(row, "status") }),
        el("td", { text: text(row, "repository_url") }),
        el("td", { text: text(row, "project_id") }),
        el("td", {}),
      ];
      if (text(row, "status") === "draft" && ctx.hasAction("project_requests.approve")) {
        const id = text(row, "id");
        cells[5].append(
          el("button", {
            class: "button ghost",
            type: "button",
            text: "Approve",
            onClick: () =>
              ctx.shell.openAction("project_requests.approve", {
                title: "Approve project request",
                initial: { id },
                onDone: load,
              }),
          }),
        );
      }
      tbody.append(el("tr", {}, ...cells));
    }
    table.append(tbody);
    body.append(table);
  }

  await load();
  return { element: section, title: "Project types" };
}
