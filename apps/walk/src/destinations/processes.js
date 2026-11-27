// The Processes destination (S2): `processes.list` and `processes.get`, the draft/publish/retire
// writes, and a process run's start. A detail route renders one process and its outputs; every
// write refetches, and a draft edit carries its lock version as If-Match.

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
function num(row, key) {
  const value = row?.[key];
  return typeof value === "number" ? String(value) : "—";
}

/** @param {string} title @param {string[]} labels @param {Array<Array<Node|string>>} bodyRows @param {string} testId */
function table(title, labels, bodyRows, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  const node = el("table", { class: "table", "data-testid": testId });
  node.append(el("thead", {}, el("tr", {}, ...labels.map((label) => el("th", { text: label })))));
  const tbody = el("tbody");
  for (const cells of bodyRows) {
    tbody.append(el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))));
  }
  node.append(tbody);
  block.append(node);
  return block;
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  if (!ctx.hasAction("processes.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Processes is planned" }),
        el("p", { class: "field-hint", text: "processes.list is not in this role's manifest; the screen appears when the process family is granted." }),
      ),
      title: "Processes is planned",
    };
  }
  return ctx.params?.process_id ? detail(ctx, ctx.params.process_id) : list(ctx);
}

/** @param {any} ctx @param {string} id */
async function detail(ctx, id) {
  const section = el("section", { class: "destination" });
  section.append(
    el("p", { class: "eyebrow" }, el("a", { href: "/build/processes", text: "← All processes" })),
    el("p", { class: "loading", role: "status", text: "Loading process…" }),
  );
  try {
    const result = await ctx.api.invoke("processes.get", { process_id: id });
    const process = result.output ?? {};
    section.replaceChildren(
      el("p", { class: "eyebrow" }, el("a", { href: "/build/processes", text: "← All processes" })),
      el("h1", { text: text(process, "title") }),
      el("p", { class: "lede", text: text(process, "description") }),
      el("dl", { class: "facts" },
        el("dt", { text: "Key" }), el("dd", { text: text(process, "key") }),
        el("dt", { text: "Status" }), el("dd", { text: text(process, "status") }),
        el("dt", { text: "Version" }), el("dd", { text: num(process, "version") }),
        el("dt", { text: "Card type" }), el("dd", { text: text(process, "card_type_code") }),
        el("dt", { text: "Owner" }), el("dd", { text: text(process, "owner_user_id") }),
      ),
    );
    const actions = el("div", { class: "destination-actions" });
    if (ctx.hasAction("process_runs.start")) {
      actions.append(
        el("button", {
          class: "button primary",
          type: "button",
          text: "Start a run",
          onClick: () => ctx.shell.openAction("process_runs.start", { title: "Start a run", initial: { process_id: process.id }, onDone: () => detail(ctx, id) }),
        }),
      );
    }
    if (ctx.hasAction("processes.update") && process.status === "draft") {
      actions.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("processes.update", {
              title: `Edit ${process.title}`,
              initial: { process_id: process.id, title: process.title },
              ifMatch: process.lock_version,
              onDone: () => detail(ctx, id),
            }),
        }),
      );
    }
    section.append(actions);
    section.append(el("h2", { text: "Outputs" }), el("pre", { class: "code", text: JSON.stringify(process.outputs ?? {}, null, 2) }));
    return { element: section, title: text(process, "title") };
  } catch (error) {
    section.replaceChildren(el("p", { class: "eyebrow" }, el("a", { href: "/build/processes", text: "← All processes" })), ctx.shell.problemPanel(error));
    return { element: section, title: "Process" };
  }
}

/** @param {any} ctx */
async function list(ctx) {
  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading processes…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Procedures & authoring" }),
    el("h1", { text: "Processes" }),
    el("p", { class: "lede", text: "The process definitions a card type runs, their drafts and published versions, and the run each one starts." }),
    actions,
    status,
    body,
  );

  async function load() {
    status.textContent = "Loading processes…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const processes = await ctx.api.invoke("processes.list", { page: { limit: 100 } }).then((r) => rows(r.output));
      if (ctx.hasAction("processes.create")) {
        actions.append(
          el("button", { class: "button primary", type: "button", text: "New process", onClick: () => ctx.shell.openAction("processes.create", { title: "New process", onDone: load }) }),
        );
      }
      status.textContent = processes.length === 0 ? "" : `${processes.length} process${processes.length === 1 ? "" : "es"}`;
      if (processes.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No process yet. This is the empty state, not an error." }),
            ctx.hasAction("processes.create")
              ? el("button", { class: "button", type: "button", text: "Create the first process", onClick: () => ctx.shell.openAction("processes.create", { title: "New process", onDone: load }) })
              : el("p", { class: "field-hint", text: "Your role cannot create processes." }),
          ),
        );
        return;
      }
      const bodyRows = processes.map((process) => {
        const cells = el("td", {});
        if (ctx.hasAction("processes.publish") && process.status === "draft") {
          cells.append(
            el("button", { class: "button ghost", type: "button", text: "Publish", onClick: () => ctx.shell.openAction("processes.publish", { title: "Publish", initial: { process_id: process.id }, onDone: load }) }),
          );
        }
        if (ctx.hasAction("processes.retire") && process.status === "published") {
          cells.append(
            el("button", { class: "button ghost", type: "button", text: "Retire", onClick: () => ctx.shell.openAction("processes.retire", { title: "Retire", initial: { process_id: process.id }, onDone: load }) }),
          );
        }
        return [
          el("a", { href: `/build/processes/${process.id}`, text: text(process, "title") }),
          text(process, "key"),
          text(process, "status"),
          num(process, "version"),
          cells,
        ];
      });
      body.append(table("Processes", ["Title", "Key", "Status", "Version", "Actions"], bodyRows, "processes-list"));
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Processes" };
}
