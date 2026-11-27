// The Jobs destination (S1): the catalogue, the published releases and the bindings that pin
// them, as `apps/web` reads them. Writes open the schema sheet; the refusal stays the server's.

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

/** @param {string} value */
function short(value) {
  return value === "—" || value.length <= 14 ? value : `${value.slice(0, 14)}…`;
}

/** @param {string} title @param {Array<{label: string, cell: (row: any) => string}>} columns @param {any[]} items @param {string} empty @param {string} testId */
function tableBlock(title, columns, items, empty, testId) {
  const block = el("section", {}, el("h2", { text: title }));
  if (items.length === 0) {
    block.append(el("div", { class: "empty" }, el("p", { text: empty })));
    return block;
  }
  const table = el("table", { class: "table", "data-testid": testId });
  table.append(
    el("thead", {}, el("tr", {}, ...columns.map((column) => el("th", { text: column.label })))),
  );
  const tbody = el("tbody");
  for (const row of items) {
    tbody.append(
      el("tr", {}, ...columns.map((column) => el("td", { text: column.cell(row) }))),
    );
  }
  table.append(tbody);
  block.append(table);
  return block;
}

/**
 * @param {any} ctx
 * @returns {Promise<{element: HTMLElement, title: string}>}
 */
export async function render(ctx) {
  const catalogue = el("div", { class: "destination" });
  const detail = el("div", { class: "destination" });
  const section = el(
    "section",
    { class: "destination" },
    el("p", { class: "eyebrow", text: "Organisation" }),
    el("h1", { text: "Jobs" }),
    el("p", {
      class: "lede",
      text: "The catalogue, the published releases, and the bindings that pin them.",
    }),
    catalogue,
    detail,
  );

  let selected = null;

  async function loadDetail() {
    detail.replaceChildren();
    if (!selected) return;
    const [releases, bindings, agents] = await Promise.all([
      ctx.hasAction("job_releases.list")
        ? ctx.api.invoke("job_releases.list", { job_id: selected }).then((r) => rows(r.output))
        : Promise.resolve([]),
      ctx.hasAction("job_bindings.list")
        ? ctx.api.invoke("job_bindings.list", { job_id: selected }).then((r) => rows(r.output))
        : Promise.resolve([]),
      ctx.hasAction("capability_agents.list")
        ? ctx.api
            .invoke("capability_agents.list", { page: { limit: 50 } })
            .then((r) => rows(r.output))
        : Promise.resolve([]),
    ]);

    const publish = el("div", { class: "destination-actions" });
    if (ctx.hasAction("job_releases.publish")) {
      publish.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Publish release",
          onClick: () =>
            ctx.shell.openAction("job_releases.publish", {
              title: "Publish release",
              initial: { job_id: selected },
              onDone: loadDetail,
            }),
        }),
      );
    }
    if (ctx.hasAction("job_bindings.create")) {
      publish.append(
        el("button", {
          class: "button",
          type: "button",
          text: "Create binding",
          onClick: () =>
            ctx.shell.openAction("job_bindings.create", {
              title: "Create binding",
              initial: {
                job_id: selected,
                executor_kind: "internal_agent",
                capability_agent_id: agents[0]?.id,
                release_id: releases[0]?.id,
              },
              onDone: loadDetail,
            }),
        }),
      );
    }
    detail.append(publish);

    detail.append(
      tableBlock(
        "Releases",
        [
          { label: "Hash", cell: (row) => short(text(row, "release_hash")) },
          { label: "Status", cell: (row) => text(row, "status") },
          { label: "Version", cell: (row) => int(row, "version") },
        ],
        releases,
        "No release has been published for this position.",
        "jobs-releases",
      ),
    );

    const bindingsBlock = el("section", {}, el("h2", { text: "Bindings" }));
    if (bindings.length === 0) {
      bindingsBlock.append(
        el("div", { class: "empty" }, el("p", { text: "No binding pins this position yet." })),
      );
    } else {
      const table = el("table", { class: "table", "data-testid": "jobs-bindings" });
      table.append(
        el(
          "thead",
          {},
          el(
            "tr",
            {},
            el("th", { text: "Pinned release" }),
            el("th", { text: "Executor" }),
            el("th", { text: "Status" }),
            el("th", {}),
          ),
        ),
      );
      const tbody = el("tbody");
      for (const row of bindings) {
        const bindingId = text(row, "id");
        const cells = [
          el("td", { text: short(text(row, "release_hash")) }),
          el("td", { text: text(row, "executor_kind") }),
          el("td", { text: text(row, "status") }),
          el("td", {}),
        ];
        if (ctx.hasAction("runs.create")) {
          cells[3].append(
            el("button", {
              class: "button ghost",
              type: "button",
              text: "Start run",
              onClick: () =>
                ctx.shell.openAction("runs.create", {
                  title: "Start a run",
                  initial: { job_binding_id: bindingId },
                  onDone: loadDetail,
                }),
            }),
          );
        }
        tbody.append(el("tr", {}, ...cells));
      }
      table.append(tbody);
      bindingsBlock.append(table);
    }
    detail.append(bindingsBlock);
  }

  async function loadCatalogue() {
    const result = ctx.hasAction("job_catalogue.list")
      ? await ctx.api.invoke("job_catalogue.list", {})
      : { output: [] };
    const items = rows(result.output);
    catalogue.replaceChildren();
    if (items.length === 0) {
      catalogue.append(
        el("div", { class: "empty" }, el("p", { text: "No catalogue positions in this tenant yet." })),
      );
      return;
    }
    const table = el("table", { class: "table", "data-testid": "jobs-catalogue" });
    table.append(
      el(
        "thead",
        {},
        el(
          "tr",
          {},
          el("th", { text: "Key" }),
          el("th", { text: "Title" }),
          el("th", { text: "Family" }),
          el("th", { text: "Status" }),
          el("th", {}),
        ),
      ),
    );
    const tbody = el("tbody");
    for (const row of items) {
      const id = text(row, "id");
      tbody.append(
        el(
          "tr",
          {},
          el("td", { text: text(row, "key") }),
          el("td", { text: text(row, "title") }),
          el("td", { text: text(row, "family") }),
          el("td", { text: text(row, "status") }),
          el("td", {}, el("button", {
            class: "button ghost",
            type: "button",
            text: "Open",
            onClick: () => {
              selected = id;
              void loadDetail();
            },
          })),
        ),
      );
    }
    table.append(tbody);
    catalogue.append(table);
  }

  await loadCatalogue();
  return { element: section, title: "Jobs" };
}
