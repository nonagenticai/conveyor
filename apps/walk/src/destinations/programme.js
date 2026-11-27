// The Programme destination (S1): the programme definition read, with the T10 gates exactly as
// `apps/web` renders them — the benefit profiles, benefits map and tranche reviews are not live.

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
function int(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** @param {string} detail */
function notYetLive(detail) {
  return el(
    "section",
    { class: "panel" },
    el("h2", { text: "Not yet live — T10" }),
    el("p", { class: "field-hint", text: detail }),
  );
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
    el("h1", { text: "Programme" }),
    el("p", { class: "lede", text: "The programme definition read, from the tenant's rows." }),
    body,
  );

  if (!ctx.hasAction("programmes.list")) {
    body.append(
      notYetLive("The programme definition read is not in this principal's manifest"),
    );
    return { element: section, title: "Programme" };
  }

  const [programmes, projects] = await Promise.all([
    ctx.api.invoke("programmes.list", { limit: 200 }).then((r) => rows(r.output)),
    ctx.api
      .request("/api/projects?limit=200")
      .then((value) => rows(value))
      .catch(() => []),
  ]);
  programmes.sort((left, right) => String(left?.name ?? "").localeCompare(String(right?.name ?? "")));

  if (programmes.length === 0) {
    body.append(el("div", { class: "empty" }, el("p", { text: "No programmes in this tenant yet." })));
  } else {
    const table = el("table", { class: "table", "data-testid": "programme-table" });
    table.append(
      el(
        "thead",
        {},
        el(
          "tr",
          {},
          el("th", { text: "Programme" }),
          el("th", { text: "Status" }),
          el("th", { text: "Projects" }),
          el("th", { text: "Owner" }),
          el("th", { text: "Version" }),
        ),
      ),
    );
    const tbody = el("tbody");
    for (const programme of programmes) {
      const id = text(programme, "id");
      const count = projects.filter((project) => text(project, "programme_id") === id).length;
      tbody.append(
        el(
          "tr",
          {},
          el("td", { text: text(programme, "name") }),
          el("td", { text: text(programme, "status") }),
          el("td", { text: String(count) }),
          el("td", { text: text(programme, "owner_user_id") }),
          el("td", { text: int(programme, "version") }),
        ),
      );
    }
    table.append(tbody);
    body.append(table);
  }
  body.append(
    notYetLive(
      "The benefit profiles, the benefits map and the tranche reviews are not rendered here yet",
    ),
  );
  return { element: section, title: "Programme" };
}
