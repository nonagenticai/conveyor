// The Validation destination (S1): every project's status and recorded evidence, from the
// tenant's own rows. Read-only; the programme name resolves through programmes.list.

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
function meta(row, key) {
  const value = row?.meta?.[key];
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
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Validation" }),
    el("p", {
      class: "lede",
      text: "Every project's status and recorded evidence, from the tenant's own rows.",
    }),
    body,
  );

  const [value, programmes] = await Promise.all([
    ctx.api.request("/api/projects?limit=200"),
    ctx.hasAction("programmes.list")
      ? ctx.api.invoke("programmes.list", { page: { limit: 50 } }).then((r) => rows(r.output))
      : Promise.resolve([]),
  ]);
  const projects = rows(value);

  if (projects.length === 0) {
    body.append(el("div", { class: "empty" }, el("p", { text: "No project in this tenant yet." })));
    return { element: section, title: "Validation" };
  }
  const name = (id) => {
    const found = programmes.find((row) => text(row, "id") === id);
    return found ? text(found, "name") : "—";
  };
  const table = el("table", { class: "table", "data-testid": "validation-table" });
  table.append(
    el("thead", {}, el("tr", {},
      el("th", { text: "Key" }),
      el("th", { text: "Project" }),
      el("th", { text: "Status" }),
      el("th", { text: "Phase" }),
      el("th", { text: "Evidence" }),
      el("th", { text: "Programme" }),
    )),
  );
  const tbody = el("tbody");
  for (const row of projects) {
    const programmeId = text(row, "programme_id");
    tbody.append(
      el("tr", {},
        el("td", { text: text(row, "key") }),
        el("td", { text: text(row, "name") }),
        el("td", { text: text(row, "status") }),
        el("td", { text: meta(row, "phase") }),
        el("td", { text: meta(row, "evidence") }),
        el("td", { text: programmeId === "—" ? "—" : name(programmeId) }),
      ),
    );
  }
  table.append(tbody);
  body.append(table);
  return { element: section, title: "Validation" };
}
