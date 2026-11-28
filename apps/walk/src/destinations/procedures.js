// The Procedures destination (S2): `procedures.*` and `capabilities.*` as the app reads them,
// the ingest verbs, and the states the server can return. Every write goes through the action
// sheet; a draft edit carries its lock version as If-Match, and every write refetches.

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
    tbody.append(
      el("tr", {}, ...cells.map((cell) => (cell instanceof Node ? el("td", {}, cell) : el("td", { text: String(cell) })))),
    );
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
  if (!ctx.hasAction("procedures.list")) {
    return {
      element: el(
        "section",
        { class: "destination" },
        el("h1", { text: "Procedures is planned" }),
        el("p", {
          class: "field-hint",
          text: "procedures.list is not in this role's manifest; the screen appears when the procedure family is granted.",
        }),
      ),
      title: "Procedures is planned",
    };
  }

  const body = el("div", { class: "destination-body" });
  const actions = el("div", { class: "destination-actions" });
  const status = el("p", { class: "loading", role: "status", text: "Loading procedures…" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Procedures & authoring" }),
    el("h1", { text: "Procedures" }),
    el("p", {
      class: "lede",
      text: "The procedure families, their drafts and published versions, the capabilities they bind, the gates they wait on, and the three ingest paths that convert an existing SOP, process or RDF source into a draft.",
    }),
    actions,
    status,
    body,
  );

  /** @param {string} name @param {string} label @param {Record<string, unknown>} [initial] **/
  function button(name, label, initial = {}) {
    return el("button", {
      class: "button",
      type: "button",
      text: label,
      onClick: () => ctx.shell.openAction(name, { title: label, initial, onDone: load }),
    });
  }

  /** @param {any} procedure */
  function rowActions(procedure) {
    const cells = el("td", {});
    if (ctx.hasAction("procedures.update") && procedure.status === "draft") {
      cells.append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Edit",
          onClick: () =>
            ctx.shell.openAction("procedures.update", {
              title: `Edit ${procedure.title}`,
              initial: { procedure_id: procedure.id, title: procedure.title },
              ifMatch: procedure.lock_version,
              onDone: load,
            }),
        }),
      );
    }
    if (ctx.hasAction("procedures.validate")) {
      cells.append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Validate",
          onClick: () => ctx.shell.openAction("procedures.validate", { title: "Validate", initial: { procedure_id: procedure.id }, onDone: load }),
        }),
      );
    }
    if (ctx.hasAction("procedures.publish") && procedure.status === "draft") {
      cells.append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Publish",
          onClick: () => ctx.shell.openAction("procedures.publish", { title: "Publish", initial: { procedure_id: procedure.id }, onDone: load }),
        }),
      );
    }
    if (ctx.hasAction("procedures.retire") && procedure.status === "published") {
      cells.append(
        el("button", {
          class: "button ghost",
          type: "button",
          text: "Retire",
          onClick: () => ctx.shell.openAction("procedures.retire", { title: "Retire", initial: { procedure_id: procedure.id }, onDone: load }),
        }),
      );
    }
    return cells;
  }

  async function load() {
    status.textContent = "Loading procedures…";
    actions.replaceChildren();
    body.replaceChildren();
    try {
      const procedures = await ctx.api.invoke("procedures.list", { page: { limit: 100 } }).then((r) => rows(r.output));
      const processes = ctx.hasAction("processes.list")
        ? await ctx.api.invoke("processes.list", { page: { limit: 50 } }).then((r) => rows(r.output))
        : [];
      const capabilities = ctx.hasAction("capabilities.list")
        ? await ctx.api.invoke("capabilities.list", { limit: 50 }).then((r) => rows(r.output))
        : [];
      const approvals = ctx.hasAction("approvals.list")
        ? await ctx.api.invoke("approvals.list", { page: { limit: 50 } }).then((r) => rows(r.output))
        : [];

      if (ctx.hasAction("procedures.create")) actions.append(button("procedures.create", "New procedure"));
      if (ctx.hasAction("procedures.ingest_sop")) actions.append(button("procedures.ingest_sop", "Ingest SOP"));
      if (ctx.hasAction("procedures.ingest_process")) actions.append(button("procedures.ingest_process", "Ingest process"));
      if (ctx.hasAction("procedures.ingest_rdf")) actions.append(button("procedures.ingest_rdf", "Ingest RDF"));

      status.textContent = procedures.length === 0 ? "" : `${procedures.length} procedure${procedures.length === 1 ? "" : "s"}`;
      if (procedures.length === 0) {
        body.append(
          el("div", { class: "empty" },
            el("p", { text: "No procedure yet. This is the empty state, not an error." }),
            ctx.hasAction("procedures.create")
              ? el("button", { class: "button", type: "button", text: "Create the first procedure", onClick: () => ctx.shell.openAction("procedures.create", { title: "New procedure", onDone: load }) })
              : el("p", { class: "field-hint", text: "Your role cannot create procedures." }),
          ),
        );
      } else {
        body.append(
          table(
            "Procedures",
            ["Title", "Key", "Status", "Version", "Actions"],
            procedures.map((procedure) => [text(procedure, "title"), text(procedure, "key"), text(procedure, "status"), num(procedure, "version"), rowActions(procedure)]),
            "procedures-list",
          ),
        );
      }
      if (processes.length > 0) {
        body.append(
          table("Processes", ["Title", "Key", "Status"], processes.map((row) => [text(row, "title"), text(row, "key"), text(row, "status")]), "procedures-processes"),
        );
      }
      if (capabilities.length > 0) {
        body.append(
          table("Capabilities", ["Key", "Name", "Status"], capabilities.map((row) => [text(row, "key"), text(row, "name"), text(row, "status")]), "procedures-capabilities"),
        );
      }
      if (approvals.length > 0) {
        body.append(
          table("Approvals", ["Gate", "Kind", "Status"], approvals.map((row) => [text(row, "gate"), text(row, "kind"), text(row, "status")]), "procedures-approvals"),
        );
      }
    } catch (error) {
      status.textContent = "";
      body.append(ctx.shell.problemPanel(error));
    }
  }

  await load();
  return { element: section, title: "Procedures" };
}
