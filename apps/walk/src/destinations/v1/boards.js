// The Board index (S4, retained V1): the tenant's projects, each with its boards, from the app's
// own read — `GET /api/projects?include=boards`, one keyset page at a time — plus the project-type
// catalogue the app shows beside them. There is no boards/projects list action, so none is
// invented. The only write is `projects.create`, through the schema sheet.

import { el } from "../../dom.js";

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
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: "Boards" }),
    el("p", {
      class: "lede",
      text: "The tenant's projects, each with its boards. A board opens its lanes and cards.",
    }),
    body,
  );

  /** @type {any[]} */
  let views = [];
  /** @type {string|null} */
  let cursor = null;
  /** @type {any[]} */
  let types = [];
  let typesError = "";

  function paint() {
    body.replaceChildren();
    const actions = el("div", { class: "destination-actions" });
    if (ctx.hasAction("projects.create")) {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "New project",
          onClick: () =>
            ctx.shell.openAction("projects.create", { title: "New project", onDone: load }),
        }),
      );
    }
    body.append(actions);

    if (views.length === 0) {
      body.append(el("div", { class: "empty" }, el("p", { text: "No project in this tenant yet." })));
    } else {
      const node = el("table", { class: "table", "data-testid": "boards-projects" });
      node.append(
        el(
          "thead",
          {},
          el(
            "tr",
            {},
            el("th", { text: "Key" }),
            el("th", { text: "Project" }),
            el("th", { text: "Status" }),
            el("th", { text: "Boards" }),
          ),
        ),
      );
      const tbody = el("tbody");
      for (const view of views) {
        const project = view?.project ?? view;
        const boards = Array.isArray(view?.boards) ? view.boards : [];
        const cell = el("td");
        if (boards.length === 0) {
          cell.append(el("span", { class: "field-hint", text: "No board yet" }));
        } else {
          for (const board of boards) {
            const boardId = typeof board?.id === "string" ? board.id : "";
            cell.append(
              el("a", {
                class: "button ghost",
                href: `/board/${boardId}`,
                text: text(board, "name"),
              }),
            );
          }
        }
        tbody.append(
          el(
            "tr",
            {},
            el("td", { text: text(project, "key") }),
            el("td", { text: text(project, "name") }),
            el("td", { text: text(project, "status") }),
            cell,
          ),
        );
      }
      node.append(tbody);
      body.append(node);
      if (cursor) {
        body.append(
          el("button", { class: "button", type: "button", text: "Load more", onClick: () => more() }),
        );
      }
    }

    if (typesError) {
      body.append(
        el(
          "section",
          {},
          el("h2", { text: "Project types" }),
          el("p", { class: "field-hint", text: typesError }),
        ),
      );
    } else if (types.length > 0) {
      body.append(
        el(
          "section",
          {},
          el("h2", { text: "Project types" }),
          el("p", {
            class: "field-hint",
            text: types.map((entry) => `${text(entry, "code")} — ${text(entry, "label")}`).join(" · "),
          }),
        ),
      );
    }
  }

  async function load() {
    const page = await ctx.api.request("/api/projects?limit=50&include=boards");
    views = rows(page);
    cursor = typeof page?.next_cursor === "string" ? page.next_cursor : null;
    types = [];
    typesError = "";
    if (ctx.hasAction("catalogues.list")) {
      try {
        const result = await ctx.api.invoke("catalogues.list", {
          kind: "project_type",
          include_inactive: false,
          limit: 50,
        });
        types = rows(result.output);
      } catch {
        typesError = "The project-type catalogue could not be read.";
      }
    }
    paint();
  }

  async function more() {
    if (!cursor) return;
    const page = await ctx.api.request(
      `/api/projects?limit=50&include=boards&cursor=${encodeURIComponent(cursor)}`,
    );
    views = views.concat(rows(page));
    cursor = typeof page?.next_cursor === "string" ? page.next_cursor : null;
    paint();
  }

  await load();
  return { element: section, title: "Boards" };
}
