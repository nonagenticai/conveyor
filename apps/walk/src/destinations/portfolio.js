// The Portfolio destination (G0's exemplar): `programmes.list` as the app reads it, a create
// sheet with Idempotency-Key, an update sheet with If-Match, and every state the server can
// return. No optimistic mutation: every write refetches (SPEC §5).

import { el, clear } from "../dom.js";
import { ProblemError, problemLine } from "../api/problem.js";

/**
 * @typedef {{id: string, name: string, status: string, owner_user_id: string|null,
 *            version: number, created_at: string, updated_at: string}} Programme
 */

/** @param {import("../shell/shell.js").Shell} shell @param {unknown} error */
function problem(shell, error) {
  return shell.problemPanel(error);
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const detailId = ctx.params?.programme_id;
  const entrance = ctx.row?.app === "/";
  return detailId ? detail(ctx, detailId) : list(ctx, entrance);
}

/** @param {any} ctx @param {string} id */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  section.append(
    el("p", { class: "eyebrow", text: "Portfolio" },
      el("a", { href: "/portfolio", text: "← All programmes" })),
    el("p", { class: "loading", text: "Loading…" }),
  );
  try {
    const result = await ctx.api.invoke("programmes.get", { id });
    const programme = /** @type {Programme} */ (result.output);
    clear(section);
    section.append(
      el("p", { class: "eyebrow" }, el("a", { href: "/portfolio", text: "← All programmes" })),
      el("h1", { text: programme.name }),
      el("dl", { class: "facts" },
        el("dt", { text: "Status" }), el("dd", { text: programme.status }),
        el("dt", { text: "Owner" }), el("dd", { text: programme.owner_user_id ?? "—" }),
        el("dt", { text: "Version" }), el("dd", { text: String(programme.version) }),
        el("dt", { text: "Updated" }), el("dd", { text: programme.updated_at }),
      ),
    );
    if (ctx.hasAction("programmes.update")) {
      section.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("programmes.update", {
              title: `Edit ${programme.name}`,
              initial: {
                id: programme.id,
                name: programme.name,
                status: programme.status,
                owner_user_id: programme.owner_user_id ?? undefined,
              },
              ifMatch: programme.version,
              onDone: () => ctx.navigate(`/portfolio/${programme.id}`),
            }),
        }),
      );
    }
    return { element: section, title: programme.name };
  } catch (error) {
    clear(section);
    section.append(problem(ctx.shell, error));
    return { element: section, title: "Portfolio" };
  }
}

/** @param {any} ctx @param {boolean} entrance */
async function list(ctx, entrance) {
  const section = el("section", { class: "destination" });
  const heading = entrance ? "Today" : "Portfolio";
  const status = el("p", { class: "loading", role: "status", text: "Loading programmes…" });
  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  if (ctx.hasAction("programmes.create")) {
    actions.append(
      el("button", {
        class: "button primary",
        type: "button",
        text: "New programme",
        onClick: () =>
          ctx.shell.openAction("programmes.create", {
            title: "New programme",
            onDone: load,
          }),
      }),
    );
  }
  section.append(
    el("p", { class: "eyebrow", text: "Portfolio & plan" }),
    el("h1", { text: heading }),
    el("p", { class: "lede", text: "Programmes the tenant holds. Reads and writes go through the action registry." }),
    actions,
    status,
    body,
  );

  /** @param {Programme} programme */
  function row(programme) {
    const cells = [
      el("td", {}, el("a", { href: `/portfolio/${programme.id}`, text: programme.name })),
      el("td", { text: programme.status }),
      el("td", { text: programme.owner_user_id ?? "—" }),
      el("td", { text: programme.updated_at }),
      el("td", {}),
    ];
    if (ctx.hasAction("programmes.update")) {
      cells[4].append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("programmes.update", {
              title: `Edit ${programme.name}`,
              initial: {
                id: programme.id,
                name: programme.name,
                status: programme.status,
                owner_user_id: programme.owner_user_id ?? undefined,
              },
              ifMatch: programme.version,
              onDone: load,
            }),
        }),
      );
    }
    return el("tr", {}, ...cells);
  }

  async function load() {
    status.textContent = "Loading programmes…";
    clear(body);
    try {
      const result = await ctx.api.invoke("programmes.list", { limit: 200 });
      const items = Array.isArray(result.output?.items) ? result.output.items : [];
      status.textContent = items.length === 0 ? "" : `${items.length} programme${items.length === 1 ? "" : "s"}`;
      if (items.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No programmes yet. This is the empty state, not an error." }),
            ctx.hasAction("programmes.create")
              ? el("button", {
                  class: "button",
                  type: "button",
                  text: "Create the first programme",
                  onClick: () =>
                    ctx.shell.openAction("programmes.create", {
                      title: "New programme",
                      onDone: load,
                    }),
                })
              : el("p", { class: "field-hint", text: "Your role cannot create programmes." }),
          ),
        );
        return;
      }
      body.append(
        el("table", { class: "table" },
          el("thead", {},
            el("tr", {},
              el("th", { text: "Name" }),
              el("th", { text: "Status" }),
              el("th", { text: "Owner" }),
              el("th", { text: "Updated" }),
              el("th", { text: "Actions" }),
            ),
          ),
          el("tbody", {}, ...items.map((item) => row(/** @type {Programme} */ (item)))),
        ),
      );
    } catch (error) {
      status.textContent = "";
      body.append(problem(ctx.shell, error));
      if (error instanceof ProblemError && error.problem.status === 401) {
        // The shell's problem panel carries the sign-in action.
      }
    }
  }

  await load();
  return { element: section, title: heading };
}
